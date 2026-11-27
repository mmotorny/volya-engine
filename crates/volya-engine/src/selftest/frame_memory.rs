// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Heap traffic of a running level (once `tests/frame_memory.rs`, #494):
//! the allocations and bytes each frame of `l01_escape` makes on the wasm
//! heap (the level's update, its sounds, its scene and the draw list),
//! with the camera turning at the start position, and what the level holds
//! after them, and the renderer's pool (*r*) the frames' lists take.
//! Reported, not held to a budget (docs/architecture/memory.md).
//! Needs the platform's heap counts ([`Context::heap`]); the frames run
//! without yielding, so nothing else allocates meanwhile.

use super::{Context, HeapCount, Outcome, Problem, Run, ensure};
use crate::core::camera::Angles;
use crate::core::input::Input;
use crate::core::pause::GlobalClock;
use crate::core::sound::system::{Listener, NoDevice, SoundSystem};
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::level_sound::{LevelSounds, SoundRun};
use crate::present::Presenter;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("a level frame's heap traffic", |c| {
    Box::pin(level_frame_heap_traffic(c))
})];

const SIZE: Resolution = Resolution::new(1024, 768);
/// Frames measured, after as many warm-up frames as [`WARMUP`].
const FRAMES: u64 = 600;
const WARMUP: u64 = 60;

pub async fn level_frame_heap_traffic(cx: &Context<'_>) -> Outcome {
    let Some(heap) = cx.heap() else {
        return Err(Problem::Skip("the platform doesn't count the heap".into()));
    };
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let mut level = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    let sound = level.take_sound_data().ok_or("the level's sound data")?;
    let mut system = SoundSystem::new(24);
    let mut level_sounds = LevelSounds::create(&mut system, tree, &data.aliases, &sound)
        .map_err(|e| format!("level sounds: {e}"))?;
    drop(sound);
    let mut run = SoundRun::default();
    let mut clock = GlobalClock::default();
    let mut renderer = cx.renderer();
    let mut presenter = Presenter::new();
    let eye = level.player().view().eye;

    // Each part's allocations and bytes over the measured frames.
    let mut parts = [
        ("update", 0, 0),
        ("sounds", 0, 0),
        ("scene", 0, 0),
        ("draw list", 0, 0),
    ];
    let mut worst = 0;
    let mut live_after_warmup = 0;
    for t in 1..=FRAMES + WARMUP {
        let measured = t > WARMUP;
        if t == WARMUP + 1 {
            live_after_warmup = heap().live;
        }
        let ms = 16 * t;
        let start = heap();
        let mut at = start;
        let mut part = |k: usize, at: &mut HeapCount| {
            let now = heap();
            if measured {
                parts[k].1 += now.allocations - at.allocations;
                parts[k].2 += now.bytes - at.bytes;
            }
            *at = now;
        };
        let angles = Angles {
            yaw: t as f32 * 0.01,
            pitch: 0.0,
            roll: 0.0,
        };
        level.set_camera(eye, angles);
        level.update(ms, &Input::new());
        part(0, &mut at);
        level_sounds.zone_events(&mut system, level.zone_sound_events());
        let v = level.player().view();
        system.set_listener(Listener {
            position: v.eye,
            direction: v.direction,
            up: v.up,
        });
        let now = clock.frame(ms, false);
        let (inputs, mut particles) = level.sound_frame_parts();
        run.frame(
            &mut system,
            &mut level_sounds,
            &inputs,
            now,
            false,
            &mut particles,
        )
        .map_err(|e| format!("sounds: {e}"))?;
        system.update(ms, &mut NoDevice);
        part(1, &mut at);
        let scene = level.scene(SIZE);
        part(2, &mut at);
        let list = presenter.draw_list(&mut renderer, tree, &scene, ms);
        drop(scene);
        presenter.recycle(list);
        part(3, &mut at);
        if measured {
            worst = worst.max(at.allocations - start.allocations);
        }
    }
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    let (allocs, bytes) = parts.iter().fold((0, 0), |(a, b), p| (a + p.1, b + p.2));
    let per = |n: usize| n as f64 / FRAMES as f64;
    let parts: Vec<String> = parts
        .iter()
        .map(|(name, a, b)| format!("{name} {:.1} and {:.1} KB", per(*a), per(*b) / 1e3))
        .collect();
    Ok(format!(
        "per frame ({FRAMES} frames after {WARMUP}): {:.0} allocations and {:.1} KB \
         (most allocations in a frame: {worst}): {}; live during the frames {:+.2} MB; \
         the renderer's pool (r) {} K after them",
        per(allocs),
        per(bytes) / 1e3,
        parts.join(", "),
        (heap().live as f64 - live_after_warmup as f64) / 1e6,
        crate::core::memory_manager::RENDER.in_use_bytes() / 1024
    ))
}
