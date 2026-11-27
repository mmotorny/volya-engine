// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the first level's frames ask of the GPU (#387; once
//! `tests/draw_calls.rs`): a few views of `l01_escape` at the shipped
//! settings (`r__supersample` 2, `r__detail_density` 0.2) drawn, with each
//! frame's draw calls and state changes by part
//! (`Renderer::take_frame_stats`), WebGL 2's calls by name and the
//! encoding time.
//!
//! Each view is kept as a frame (`<view>`, and `<view>-unblended` with the
//! trees unblended), drawn without the particle effects, whose simulation
//! does not repeat, and drawn a few times, as the GPU doesn't always
//! repeat a frame exactly; with an earlier run's frames
//! (`VOLYA_COMPARE_DIR`) every pixel of a view this build repeats exactly
//! is held to the earlier run's, so a change that should not alter the
//! picture is checked against the build before it.

use super::{Context, Outcome, Problem, Run, capture, capture_scaled, ensure, now_ms};
use crate::core::camera::Angles;
use crate::core::input::Input;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::Renderer;
use crate::render::present::Presentation;
use crate::render::stats::{FrameStats, Part, StaticCalls};
use crate::scene::Scene;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[
    ("draw calls from game data", |c| {
        Box::pin(draw_calls_from_game_data(c))
    }),
    ("GPU cost at the phone's size", |c| {
        Box::pin(gpu_cost_from_game_data(c))
    }),
];

/// A view's eye and angles; `None` for the start view as loaded.
type Camera = ([f32; 3], Angles);

const SIZE: Resolution = Resolution::new(1024, 768);
/// Drawings of each view compared.
const REPEATS: usize = 32;

fn report(view: &str, s: &FrameStats, encode: f64) -> String {
    let t = s.total();
    let mut out = format!(
        "{view}: {} draws, {} pipelines, {} bind groups, {} buffers, {} passes, \
         {} submits; encoded in {encode:.2} ms",
        t.draws, t.pipelines, t.bind_groups, t.buffers, s.passes, s.submits
    );
    for part in Part::ALL {
        let c = s.part(part);
        if c != Default::default() {
            out += &format!(
                "; {} {} draws {} pipelines {} bind groups {} buffers",
                part.name(),
                c.draws,
                c.pipelines,
                c.bind_groups,
                c.buffers
            );
        }
    }
    out
}

/// The static world's calls by program and kind.
fn report_static(calls: &StaticCalls) -> String {
    let mut rows = calls.rows.clone();
    rows.sort_by_key(|r| std::cmp::Reverse(r.2));
    rows.iter()
        .map(|(program, kind, n, draws)| {
            format!("static {program} {kind:?}: {n} calls for {draws} draws")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// WebGL 2's calls by name, most first.
fn report_gl(calls: &Option<Vec<(String, u64)>>) -> String {
    match calls {
        None => "WebGL calls not counted here".into(),
        Some(calls) => {
            let total: u64 = calls.iter().map(|c| c.1).sum();
            let named: Vec<String> = calls.iter().map(|(n, c)| format!("{n} {c}")).collect();
            format!(
                "{total} WebGL calls (the readback's among them): {}",
                named.join(", ")
            )
        }
    }
}

// The turned views' yaws, quarter turns rounded, are those of reference
// pictures already written.
#[allow(clippy::approx_constant)]
pub async fn draw_calls_from_game_data(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let presentation = Presentation {
        supersample: 2,
        ..Default::default()
    };
    let mut notes = Vec::new();
    // The trees blended, as drawn, and unblended
    // (`Renderer::set_unblended_trees`), which the GPU repeats exactly, so
    // comparisons there are exact everywhere.
    for unblended in [false, true] {
        let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
        screen.set_detail_settings(0.2, 0.5);
        screen.set_supersample(2);
        // One update: the level's lamps on, so the frames have light passes.
        screen.update(16, &Input::new());
        let start_eye = screen.state().eye;
        let (x, z) = (-205.0, -112.0);
        let (ground, _) = screen
            .collision()
            .floor_below(x, z, 100.0, -100.0)
            .ok_or("ground")?;
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
        let mut r = cx.renderer();
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
            let mut encode = 0.0;
            let mut gl = None;
            for _ in 0..3 {
                now += 16;
                screen.paused(now);
                let mut scene = screen.scene(SIZE, 0);
                scene.presentation = presentation;
                r.take_frame_stats();
                r.take_static_calls();
                let start = now_ms();
                let (frame, counted) = cx.gl_calls(|| {
                    let list = presenter.draw_list(&mut r, tree, &scene, 0);
                    r.capture(&list, SIZE.width, SIZE.height, crate::render::Color::BLACK)
                });
                encode = now_ms() - start;
                gl = counted;
                stats = r.take_frame_stats();
                calls = r.take_static_calls();
                frame.await.map_err(|e| format!("capture: {e}"))?;
            }
            if !unblended {
                notes.push(report(name, &stats, encode));
                notes.push(report_static(&calls));
                notes.push(format!("{name}: {}", report_gl(&gl)));
            }
            // The particles left out of the pixels compared (the level
            // checks' views compare them in the parity suite).
            let mut scene = screen.scene(SIZE, 0);
            scene.presentation = presentation;
            if let Some(w) = scene.world.as_mut() {
                w.particles = Default::default();
            }
            // Drawn a few times: the same list doesn't always give the same
            // pixels (measured on the Mac's GPU: a few hundred, on the trees
            // seen from outdoors); a view that repeats itself is held to
            // the earlier run's exactly.
            let list = presenter.draw_list(&mut r, tree, &scene, 0);
            let mut drawings = Vec::with_capacity(REPEATS);
            for _ in 0..REPEATS {
                drawings.push(capture(&mut r, &list, SIZE.width, SIZE.height).await?);
            }
            let first = &drawings[0];
            let unstable = (0..first.len() / 4)
                .filter(|&p| {
                    drawings
                        .iter()
                        .any(|d| d[4 * p..4 * p + 4] != first[4 * p..4 * p + 4])
                })
                .count();
            notes.push(format!(
                "{name}{}: {unstable} pixels vary between drawings of the same list",
                if unblended { " (trees unblended)" } else { "" }
            ));
            let file = format!("{name}{}", if unblended { "-unblended" } else { "" });
            cx.dump_compared(&file, SIZE.width, SIZE.height, first, unstable == 0);
        }
        let errors = presenter.take_errors();
        ensure!(errors.is_empty(), "{errors:?}");
    }
    Ok(notes.join("; "))
}

/// The phone's native size (iPhone 17 Pro Max, landscape), which the
/// user's video mode is (#387, their diagnostic log).
const PHONE: Resolution = Resolution::new(2868, 1320);

/// Mean GPU times of `frames` frames of `scene`'s list at [`PHONE`]: the
/// frame's and each pass's, milliseconds.
async fn gpu_times(
    r: &mut Renderer,
    presenter: &mut Presenter,
    tree: &dyn crate::core::vfs::Vfs,
    scene: &Scene,
    frames: usize,
) -> Result<(f64, Vec<(&'static str, f64)>), Problem> {
    let mut times = Vec::new();
    let mut passes: Vec<(&'static str, f64, u32)> = Vec::new();
    for _ in 0..frames {
        let list = presenter.draw_list(r, tree, scene, 0);
        let size = (PHONE.width, PHONE.height);
        capture_scaled(r, &list, size, PHONE.width, PHONE.height).await?;
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
    Ok((
        mean,
        passes
            .into_iter()
            .map(|(n, t, c)| (n, t / f64::from(c)))
            .collect(),
    ))
}

/// The GPU's time per frame and per pass at the phone's size, with parts
/// of the frame left out to tell their cost (#387), where the renderer has
/// timestamp queries.
pub async fn gpu_cost_from_game_data(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let mut r = cx.renderer();
    if !r.can_time_gpu() {
        return Err(Problem::Skip("no timestamp queries".into()));
    }
    let data = GameData::read(tree, None, PHONE).map_err(|e| format!("game data: {e}"))?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    screen.set_detail_settings(0.2, 0.5);
    r.set_gpu_timing(true);
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
        .ok_or("ground")?;
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
    let mut notes = Vec::new();
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
                if let Some(w) = v.world.as_mut() {
                    w.particles = Default::default();
                }
                variants.push(("no particles", v));
                let mut v = scene.clone();
                if let Some(w) = v.world.as_mut() {
                    w.details = None;
                }
                variants.push(("no details", v));
                let mut v = scene.clone();
                if let Some(w) = v.world.as_mut() {
                    w.lights = Default::default();
                    w.glows = Default::default();
                }
                variants.push(("no lights or glows", v));
                let mut v = scene.clone();
                if let Some(w) = v.world.as_mut() {
                    w.sky = None;
                }
                variants.push(("no sky", v));
            }
            for (variant, scene) in variants {
                // Warm up (pipelines, the target), then measure.
                gpu_times(&mut r, &mut presenter, tree, &scene, 5).await?;
                let (frame, passes) = gpu_times(&mut r, &mut presenter, tree, &scene, 30).await?;
                let busy: f64 = passes.iter().map(|p| p.1).sum();
                let passes: Vec<String> =
                    passes.iter().map(|(n, t)| format!("{n} {t:.3}")).collect();
                notes.push(format!(
                    "{name}, r__supersample {supersample}, {variant}: GPU \
                     {frame:.2} ms a frame, passes {busy:.3} ({}); target {:?}",
                    passes.join(", "),
                    r.presentation_target().map(|t| t.0)
                ));
            }
        }
    }
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    Ok(notes.join("; "))
}
