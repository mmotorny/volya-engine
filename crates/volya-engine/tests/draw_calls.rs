// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the first level's frames ask of the GPU (#387), from the Russian
//! edition (BYOA): a few views of `l01_escape` at the shipped settings
//! (`r__supersample` 2, `r__detail_density` 0.2) drawn, with each frame's
//! draw calls and state changes by part
//! (`Renderer::take_frame_stats`) and its encoding time.
//!
//! `VOLYA_DUMP_DRAWS=<dir>` writes every view's pixels there (raw RGBA,
//! drawn without the particle effects, whose simulation does not repeat,
//! and drawn a few times, as the Mac's GPU doesn't always repeat a frame
//! exactly); `VOLYA_COMPARE_DRAWS=<dir>[:<dir>…]` asserts every pixel of
//! every view equals the one written there (in any of the directories, so
//! several runs of the build before) in one of its drawings, so a change
//! that should not alter the picture is checked against the build before
//! it. Run it with `--release` for meaningful times. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use volya_engine::core::camera::Angles;
use volya_engine::core::input::Input;
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::present::Presenter;
use volya_engine::render::present::Presentation;
use volya_engine::render::stats::{FrameStats, Part};
use volya_engine::render::{Renderer, wgpu};
use volya_engine_formats::ui::Resolution;

/// A view's eye and angles; `None` for the start view as loaded.
type Camera = ([f32; 3], Angles);

const SIZE: Resolution = Resolution::new(1024, 768);
/// Drawings of each view compared.
const REPEATS: usize = 32;

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

fn report(view: &str, s: &FrameStats, encode: Duration) {
    let t = s.total();
    eprintln!(
        "{view}: {} draws, {} pipelines, {} bind groups, {} buffers, {} passes, \
         {} submits; encoded in {encode:.2?}",
        t.draws, t.pipelines, t.bind_groups, t.buffers, s.passes, s.submits
    );
    for part in Part::ALL {
        let c = s.part(part);
        if c != Default::default() {
            eprintln!(
                "    {:<12} {:>5} draws {:>5} pipelines {:>5} bind groups {:>5} buffers",
                part.name(),
                c.draws,
                c.pipelines,
                c.bind_groups,
                c.buffers
            );
        }
    }
}

/// The static world's calls by program and kind.
fn report_static(calls: &volya_engine::render::stats::StaticCalls) {
    let mut rows = calls.rows.clone();
    rows.sort_by_key(|r| std::cmp::Reverse(r.2));
    for (program, kind, n, draws) in rows {
        eprintln!("    static {program:<18} {kind:?}: {n} calls for {draws} draws");
    }
}

// The turned views' yaws, quarter turns rounded, are those of reference
// pictures already written.
#[allow(clippy::approx_constant)]
#[test]
fn draw_calls_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let dump = std::env::var_os("VOLYA_DUMP_DRAWS").map(PathBuf::from);
    let compare: Option<Vec<PathBuf>> =
        std::env::var_os("VOLYA_COMPARE_DRAWS").map(|v| std::env::split_paths(&v).collect());
    let presentation = Presentation {
        supersample: 2,
        ..Default::default()
    };
    let mut compared = 0;
    // The trees blended, as drawn, and unblended
    // (`Renderer::set_unblended_trees`), which the Mac's GPU repeats
    // exactly, so comparisons there are exact everywhere.
    for unblended in [false, true] {
        let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();
        screen.set_detail_settings(0.2, 0.5);
        screen.set_supersample(2);
        // One update: the level's lamps on, so the frames have light passes.
        screen.update(16, &Input::new());
        let start_eye = screen.state().eye;
        let (x, z) = (-205.0, -112.0);
        let (ground, _) = screen
            .collision()
            .floor_below(x, z, 100.0, -100.0)
            .expect("ground");
        let angles = |yaw: f32, pitch: f32| Angles {
            yaw,
            pitch,
            roll: 0.0,
        };
        // The start view as loaded, then turned; outdoors west of the start
        // on the ground (grass), and above it looking over the level.
        let views: Vec<(&str, Option<Camera>)> = vec![
            ("start", None),
            ("start-yaw-90", Some((start_eye, angles(1.571, 0.0)))),
            ("start-yaw-180", Some((start_eye, angles(3.142, 0.0)))),
            ("start-yaw-270", Some((start_eye, angles(4.712, 0.0)))),
            (
                "outdoors",
                Some((
                    [x, ground + 1.7, z],
                    angles(std::f32::consts::FRAC_PI_2, 0.25),
                )),
            ),
            (
                "overview",
                Some((
                    [x, ground + 60.0, z],
                    angles(std::f32::consts::FRAC_PI_2, 0.5),
                )),
            ),
        ];
        let mut r = renderer();
        r.set_unblended_trees(unblended);
        let mut presenter = Presenter::new();
        let mut now = 0;
        for (name, camera) in views {
            if let Some((eye, angles)) = camera {
                screen.set_camera(eye, angles);
            }
            // A few frames for the detail objects' cache (#407), the last
            // one measured.
            let mut stats = FrameStats::default();
            let mut calls = Default::default();
            let mut encode = Duration::ZERO;
            for _ in 0..3 {
                now += 16;
                screen.paused(now);
                let mut scene = screen.scene(SIZE, 0);
                scene.presentation = presentation;
                r.take_frame_stats();
                r.take_static_calls();
                let start = Instant::now();
                let list = presenter.draw_list(&mut r, &tree, &scene, 0);
                let frame = r.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK);
                encode = start.elapsed();
                stats = r.take_frame_stats();
                calls = r.take_static_calls();
                pollster::block_on(frame).unwrap();
            }
            if !unblended {
                report(name, &stats, encode);
                report_static(&calls);
            }
            // The particles left out of the pixels compared: their
            // simulation does not repeat from run to run (measured).
            let mut scene = screen.scene(SIZE, 0);
            scene.presentation = presentation;
            if let Some(w) = scene.world.as_mut() {
                w.particles.clear();
            }
            // Drawn a few times: the same list doesn't always give the same
            // pixels on the Mac's GPU (measured: a few hundred, on the trees
            // seen from outdoors), so a pixel matches when it equals the
            // reference's at that point in any of its drawings.
            let list = presenter.draw_list(&mut r, &tree, &scene, 0);
            let drawings: Vec<Vec<u8>> = (0..REPEATS)
                .map(|_| {
                    pollster::block_on(r.capture(
                        &list,
                        SIZE.width,
                        SIZE.height,
                        wgpu::Color::BLACK,
                    ))
                    .unwrap()
                })
                .collect();
            let unstable = (0..drawings[0].len() / 4)
                .filter(|&p| {
                    let px = |d: &Vec<u8>| d[4 * p..4 * p + 4].to_vec();
                    drawings.iter().any(|d| px(d) != px(&drawings[0]))
                })
                .count();
            eprintln!("{name}: {unstable} pixels vary between drawings of the same list");
            let file = format!("{name}{}.rgba", if unblended { "-unblended" } else { "" });
            if let Some(dir) = &dump {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::write(dir.join(&file), drawings.concat()).unwrap();
            }
            if let Some(dirs) = &compare {
                let before: Vec<u8> = dirs
                    .iter()
                    .flat_map(|dir| std::fs::read(dir.join(&file)).unwrap())
                    .collect();
                let len = drawings[0].len();
                let at =
                    |d: &[u8], p: usize| -> [u8; 4] { d[4 * p..4 * p + 4].try_into().unwrap() };
                // Where the reference repeats itself, every drawing must
                // equal it; elsewhere each must be one of its values.
                let (mut differ, mut varying, mut outside) = (0, 0, 0);
                for p in 0..len / 4 {
                    let reference: Vec<[u8; 4]> = before.chunks(len).map(|d| at(d, p)).collect();
                    let stable = reference.iter().all(|v| *v == reference[0]);
                    if !stable {
                        varying += 1;
                    }
                    if drawings.iter().any(|d| !reference.contains(&at(d, p))) {
                        if stable {
                            differ += 1;
                        } else {
                            outside += 1;
                        }
                    }
                }
                eprintln!(
                    "{file}: {differ} pixels differ where the reference repeats itself; it \
                     varies at {varying}, where {outside} take another value; this build \
                     varies at {unstable}"
                );
                // Where the GPU doesn't repeat the reference itself (the
                // blended trees, #435), other runs and other ways of
                // issuing the same draws differ at pixels it repeated too
                // (measured: up to about 430 of 803 from above, while with
                // the trees unblended every way matched exactly), so only
                // views that repeat themselves are held to it.
                if varying == 0 {
                    assert_eq!(differ, 0, "{file}: {differ} pixels differ");
                }
                compared += 1;
            }
        }
        assert!(presenter.take_errors().is_empty());
    }
    if compare.is_some() {
        eprintln!("{compared} views compared");
    }
}

/// The phone's native size (iPhone 17 Pro Max, landscape), which the
/// user's video mode is (#387, their diagnostic log).
const PHONE: Resolution = Resolution::new(2868, 1320);

/// Mean GPU times of `frames` frames of `scene`'s list at [`PHONE`]: the
/// frame's and each pass's, milliseconds.
fn gpu_times(
    r: &mut Renderer,
    presenter: &mut Presenter,
    tree: &dyn volya_engine::core::vfs::Vfs,
    scene: &volya_engine::scene::Scene,
    target: &wgpu::Texture,
    frames: usize,
) -> (f64, Vec<(&'static str, f64)>) {
    let mut times = Vec::new();
    let mut passes: Vec<(&'static str, f64, u32)> = Vec::new();
    for _ in 0..frames {
        let list = presenter.draw_list(r, tree, scene, 0);
        r.render_scaled(
            target,
            &list,
            wgpu::Color::BLACK,
            (PHONE.width, PHONE.height),
        );
        let _ = r.device().poll(wgpu::PollType::wait_indefinitely());
        times.extend(r.take_gpu_times());
        for (name, ms) in r.take_gpu_pass_times() {
            match passes.iter_mut().find(|p| p.0 == name) {
                Some(p) => {
                    p.1 += ms;
                    p.2 += 1;
                }
                None => passes.push((name, ms, 1)),
            }
        }
    }
    let mean = times.iter().sum::<f64>() / times.len().max(1) as f64;
    (
        mean,
        passes
            .into_iter()
            .map(|(n, t, c)| (n, t / f64::from(c)))
            .collect(),
    )
}

/// The GPU's time per frame and per pass at the phone's size on the Mac's
/// GPU, with parts of the frame left out to tell their cost (#387).
#[test]
fn gpu_cost_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, PHONE).unwrap();
    let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();
    screen.set_detail_settings(0.2, 0.5);
    let mut r = renderer();
    if !r.can_time_gpu() {
        eprintln!("skipped: no timestamp queries");
        return;
    }
    r.set_gpu_timing(true);
    let target = r.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("phone"),
        size: wgpu::Extent3d {
            width: PHONE.width,
            height: PHONE.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut presenter = Presenter::new();
    let start_eye = screen.state().eye;
    let start_angles = Angles {
        yaw: screen.state().yaw,
        pitch: screen.state().pitch,
        roll: 0.0,
    };
    let (x, z) = (-205.0, -112.0);
    let (ground, _) = screen
        .collision()
        .floor_below(x, z, 100.0, -100.0)
        .expect("ground");
    let views = [
        ("start", start_eye, start_angles),
        (
            "outdoors",
            [x, ground + 1.7, z],
            Angles {
                yaw: std::f32::consts::FRAC_PI_2,
                pitch: 0.25,
                roll: 0.0,
            },
        ),
        // Up at the sky: almost nothing of the world on screen, so its
        // cost there is what doesn't depend on pixels.
        (
            "sky",
            [x, ground + 1.7, z],
            Angles {
                yaw: std::f32::consts::FRAC_PI_2,
                pitch: -1.4,
                roll: 0.0,
            },
        ),
    ];
    let mut now = 0;
    for (name, eye, angles) in views {
        screen.set_camera(eye, angles);
        for supersample in [2, 1] {
            screen.set_supersample(supersample);
            for _ in 0..3 {
                now += 16;
                screen.paused(now);
            }
            let mut scene = screen.scene(PHONE, 0);
            scene.presentation = Presentation {
                supersample,
                ..Default::default()
            };
            let mut variants = vec![("whole", scene.clone())];
            if supersample == 2 {
                let mut v = scene.clone();
                v.world.as_mut().unwrap().particles.clear();
                variants.push(("no particles", v));
                let mut v = scene.clone();
                v.world.as_mut().unwrap().details = None;
                variants.push(("no details", v));
                let mut v = scene.clone();
                let w = v.world.as_mut().unwrap();
                w.lights.clear();
                w.glows.clear();
                variants.push(("no lights or glows", v));
                let mut v = scene.clone();
                v.world.as_mut().unwrap().sky = None;
                variants.push(("no sky", v));
            }
            for (variant, scene) in variants {
                // Warm up (pipelines, the target), then measure.
                gpu_times(&mut r, &mut presenter, &tree, &scene, &target, 5);
                let (frame, passes) = gpu_times(&mut r, &mut presenter, &tree, &scene, &target, 30);
                let busy: f64 = passes.iter().map(|p| p.1).sum();
                let passes: Vec<String> =
                    passes.iter().map(|(n, t)| format!("{n} {t:.3}")).collect();
                eprintln!(
                    "{name}, r__supersample {supersample}, {variant}: GPU \
                     {frame:.2} ms a frame, passes {busy:.3} ({}); target {:?}",
                    passes.join(", "),
                    r.presentation_target().map(|t| t.0)
                );
            }
        }
    }
    assert!(presenter.take_errors().is_empty());
}
