// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The first level's grass and detail objects from the Russian edition
//! (BYOA): drawn on both renderer paths through the supersampled target at
//! the shipped `r__detail_density` and `r__supersample`, against the same
//! view without them (docs/behavior/detail-objects.md), with the pass's and
//! the frame's times and the GPU memory. `VOLYA_DUMP_GRASS=<file.ppm>` and
//! `VOLYA_DUMP_GRASS_OFF=<file.ppm>` write the frames locally. Run it with
//! `--release` for meaningful times. Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::Instant;

use volya_engine::core::camera::Angles;
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::present::Presenter;
use volya_engine::render::present::Presentation;
use volya_engine::render::{Renderer, wgpu};
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn renderer(compat: bool) -> Renderer {
    let mut r = pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: volya_engine::render::device_features(&adapter),
                ..Default::default()
            })
            .await
            .unwrap();
        Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm)
    });
    r.set_compat(compat);
    r
}

fn dump(var: &str, pixels: &[u8]) {
    if let Some(path) = std::env::var_os(var) {
        let mut ppm = format!("P6 {} {} 255\n", SIZE.width, SIZE.height).into_bytes();
        for p in pixels.chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let se: f64 = a
        .chunks(4)
        .zip(b.chunks(4))
        .flat_map(|(x, y)| (0..3).map(move |c| (f64::from(x[c]) - f64::from(y[c])).powi(2)))
        .sum();
    10.0 * (255.0f64.powi(2) / (se / (a.len() / 4 * 3) as f64)).log10()
}

#[test]
fn grass_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut pictures = Vec::new();
    for compat in [false, true] {
        let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();
        let world = screen.world().clone();
        let d = world
            .details
            .as_ref()
            .expect("l01_escape has detail objects");
        assert_eq!(d.models.len(), 21);
        assert!(d.alpha_test, "details\\blend has Alpha-blend");
        assert!(
            d.texture.ends_with("l01_escape\\build_details.dds"),
            "{}",
            d.texture
        );
        // The shipped user.ltx: r__detail_density 0.2, r__supersample 2.
        screen.set_detail_settings(0.2, 0.5);
        screen.set_supersample(2);
        // Outdoors west of the start, on the ground, looking along +x and
        // a little down.
        let (x, z) = (-205.0, -112.0);
        let (ground, _) = screen
            .collision()
            .floor_below(x, z, 100.0, -100.0)
            .expect("ground");
        screen.set_camera(
            [x, ground + 1.7, z],
            Angles {
                yaw: std::f32::consts::FRAC_PI_2,
                pitch: 0.25,
                roll: 0.0,
            },
        );
        let mut r = renderer(compat);
        let mut presenter = Presenter::new();
        let draw =
            |r: &mut Renderer, presenter: &mut Presenter, scene: &volya_engine::scene::Scene| {
                let list = presenter.draw_list(r, &tree, scene, 0);
                pollster::block_on(r.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
                    .unwrap()
            };
        let presentation = Presentation {
            supersample: 2,
            ..Default::default()
        };
        // The first frame's pass draws nothing (TODO(#407)); the next
        // builds the whole cache and selects.
        let mut scene = screen.scene(SIZE, 0);
        assert!(
            scene
                .world
                .as_ref()
                .unwrap()
                .details
                .as_ref()
                .unwrap()
                .frame
                .instances
                .is_empty()
        );
        scene.presentation = presentation;
        draw(&mut r, &mut presenter, &scene);
        let mut pass_times = Vec::new();
        let mut frames = Vec::new();
        for f in 0..20 {
            screen.paused(16 * (f + 1));
            let start = Instant::now();
            let mut scene = screen.scene(SIZE, 0);
            pass_times.push(start.elapsed());
            scene.presentation = presentation;
            frames.push(scene);
        }
        let scene = frames.pop().unwrap();
        let drawn = scene.world.as_ref().unwrap().details.as_ref().unwrap();
        let instances = drawn.frame.instances.len();
        let draws = drawn.frame.draws.len();
        assert!(instances > 1000, "{instances} instances drawn");
        let start = Instant::now();
        let grass = draw(&mut r, &mut presenter, &scene);
        let with = start.elapsed();
        let mut bare = scene.clone();
        bare.world.as_mut().unwrap().details = None;
        let start = Instant::now();
        let without = draw(&mut r, &mut presenter, &bare);
        let without_t = start.elapsed();
        assert!(presenter.take_errors().is_empty());
        let changed = grass
            .chunks(4)
            .zip(without.chunks(4))
            .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(x, y)| x.abs_diff(*y) > 24))
            .count();
        let share = changed as f64 / (SIZE.width * SIZE.height) as f64;
        let cache = screen.details().unwrap();
        eprintln!(
            "compat {compat}: {} instances in the cache ({:.1} MB held), {instances} drawn in \
             {draws} draws; the first pass (whole cache) {:.1?}, later passes {:.1?} on average; \
             frame with readback {with:.1?}, without {without_t:.1?}; details' GPU buffers \
             {:.1} MiB; {:.1} % of the pixels changed",
            cache.cache.instances().count(),
            cache.cache.memory_bytes() as f64 / 1e6,
            pass_times[0],
            pass_times[1..].iter().sum::<std::time::Duration>() / (pass_times.len() - 1) as u32,
            r.details_gpu_bytes() as f64 / (1 << 20) as f64,
            share * 100.0,
        );
        assert!(
            share > 0.02,
            "grass changes {:.2} % of the picture",
            share * 100.0
        );
        if !compat {
            dump("VOLYA_DUMP_GRASS", &grass);
            dump("VOLYA_DUMP_GRASS_OFF", &without);
        }
        pictures.push(grass);
    }
    // Both paths draw the same grass.
    let q = psnr(&pictures[0], &pictures[1]);
    eprintln!("PSNR of the WebGL 2 path's grass against WebGPU's: {q:.1} dB");
    assert!(q > 35.0, "{q:.1} dB");
}
