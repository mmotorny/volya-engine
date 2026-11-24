// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game's level from the Russian edition (BYOA): which level loads
//! (docs/behavior/level-loading.md section 2.2), the start view
//! (docs/behavior/start-position.md) rendered headless, with load and frame
//! times. `VOLYA_DUMP_LEVEL=<file.ppm>` writes the start frame locally for
//! inspection. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::Instant;

use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::{self, LevelScreen};
use volya_engine::present::Presenter;
use volya_engine::render::{Renderer, wgpu};
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn renderer() -> Renderer {
    pollster::block_on(async {
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
    })
}

/// Writes `pixels` (RGBA) as a PPM when `VOLYA_DUMP_LEVEL` is set.
fn dump(pixels: &[u8], width: u32, height: u32) {
    if let Some(path) = std::env::var_os("VOLYA_DUMP_LEVEL") {
        let mut ppm = format!("P6 {width} {height} 255\n").into_bytes();
        for p in pixels.chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
}

#[test]
fn new_game_start_view_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();

    let (name, actor) = level::new_game_level(&tree, &data).unwrap();
    assert_eq!((name.as_str(), actor.game_graph_vertex), ("l01_escape", 4));

    let start = Instant::now();
    let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();
    let loaded = start.elapsed();
    let state = screen.state();
    assert_eq!(state.draws, 7_782);
    for (got, want) in state.eye.iter().zip([-246.7263, -23.26326, -134.43387]) {
        assert!((got - want).abs() < 1e-4, "eye {:?}", state.eye);
    }
    // start-position.md 7.1: two modifiers apply at the start eye.
    assert!(
        (state.far_plane - 254.478).abs() < 1e-3,
        "{}",
        state.far_plane
    );

    let mut gpu = renderer();
    let mut presenter = Presenter::new();
    let scene = screen.scene(SIZE, 0);
    let start = Instant::now();
    let list = presenter.draw_list(&mut gpu, &tree, &scene, 0);
    let pixels =
        pollster::block_on(gpu.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
            .unwrap();
    let first = start.elapsed();
    let errors = presenter.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    let world = presenter.world().expect("world uploaded").clone();
    assert!(
        screen.world().take_geometry().is_none(),
        "CPU copy released"
    );

    // Frame time: the same view again, the world already on the GPU.
    let start = Instant::now();
    const FRAMES: u32 = 10;
    for _ in 0..FRAMES {
        let list = presenter.draw_list(&mut gpu, &tree, &scene, 0);
        pollster::block_on(gpu.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
            .unwrap();
    }
    let frame = start.elapsed() / FRAMES;
    eprintln!(
        "level: loaded in {loaded:.1?}, first frame (upload) {first:.1?}, then {frame:.1?} per \
         frame with readback; GPU buffers {:.1} MiB, textures {:.1} MiB; {} draws, {} skipped",
        world.gpu_bytes() as f64 / (1 << 20) as f64,
        world.texture_bytes() as f64 / (1 << 20) as f64,
        world.draw_count(),
        world.skipped(),
    );
    let level = screen.world();
    eprintln!(
        "{} surfaces; not drawn: {:?}",
        level.surfaces.len(),
        level.not_drawn
    );
    assert_eq!(world.skipped(), 0);
    dump(&pixels, SIZE.width, SIZE.height);

    // The start view looks at the trader's surroundings: most of the screen
    // is world, in many colours, and the sky (cleared) shows somewhere.
    let clear = scene.world.as_ref().unwrap().environment.fog_rgb();
    let clear = clear.map(|c| (c * 255.0).round() as u8);
    let total = (SIZE.width * SIZE.height) as usize;
    let background = pixels
        .chunks(4)
        .filter(|p| p[..3].iter().zip(clear).all(|(a, b)| a.abs_diff(b) <= 1))
        .count();
    let distinct: std::collections::BTreeSet<&[u8]> = pixels.chunks(4).collect();
    eprintln!(
        "background {:.1} %, {} colours",
        100.0 * background as f64 / total as f64,
        distinct.len()
    );
    assert!(background * 2 < total, "mostly world");
    assert!(distinct.len() > 500, "{} colours", distinct.len());

    // Outdoors, above the start (`VOLYA_OUTDOOR=x,y,z,yaw,pitch` picks
    // another view): terrain, trees and fog, and the clear colour where
    // nothing is drawn.
    let view: Vec<f32> = std::env::var("VOLYA_OUTDOOR")
        .unwrap_or_else(|_| "-180,5,-120,1.2,0.15".into())
        .split(',')
        .map(|v| v.trim().parse().unwrap())
        .collect();
    screen.set_camera(
        [view[0], view[1], view[2]],
        volya_engine::core::camera::Angles {
            yaw: view[3],
            pitch: view[4],
            roll: 0.0,
        },
    );
    let scene = screen.scene(SIZE, 5000);
    let list = presenter.draw_list(&mut gpu, &tree, &scene, 5000);
    let pixels =
        pollster::block_on(gpu.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
            .unwrap();
    if let Some(path) = std::env::var_os("VOLYA_DUMP_OUTDOOR") {
        let mut ppm = format!("P6 {} {} 255\n", SIZE.width, SIZE.height).into_bytes();
        for p in pixels.chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
    let distinct: std::collections::BTreeSet<&[u8]> = pixels.chunks(4).collect();
    assert!(distinct.len() > 500, "outdoors: {} colours", distinct.len());
    // The sky (docs/behavior/sky.md) covers the top rows: varied, and not
    // the clear colour.
    let top = &pixels[..(SIZE.width * 16 * 4) as usize];
    let cleared = top
        .chunks(4)
        .filter(|p| p[..3].iter().zip(clear).all(|(a, b)| a.abs_diff(b) <= 1))
        .count();
    let sky: std::collections::BTreeSet<&[u8]> = top.chunks(4).collect();
    assert!(
        cleared * 100 < top.len() / 4,
        "{cleared} cleared sky pixels"
    );
    assert!(sky.len() > 50, "{} sky colours", sky.len());
}
