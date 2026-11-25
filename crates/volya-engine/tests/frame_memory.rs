// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Heap traffic of a running level on the Russian edition (BYOA): the
//! allocations and bytes each frame of `l01_escape` makes and frees (the
//! level's update, its sounds, its scene and the draw list), with the
//! camera turning at the start position, and what the level holds after
//! them. Reported, not held to a budget (docs/architecture.md, Memory).
//! Its own test binary, so no other test allocates meanwhile; run it with
//! `--release`. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]
// A counting allocator needs `unsafe impl GlobalAlloc`; it only forwards to
// the system allocator.
#![allow(unsafe_code)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use volya_engine::core::camera::Angles;
use volya_engine::core::input::Input;
use volya_engine::core::pause::GlobalClock;
use volya_engine::core::sound::system::{Listener, NoDevice, SoundSystem};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::level_sound::{LevelSounds, SoundRun};
use volya_engine::present::Presenter;
use volya_engine::render::{Renderer, wgpu};
use volya_engine_formats::ui::Resolution;

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
/// Allocations made (a growing reallocation counts as one).
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
/// Bytes allocated (a growing reallocation counts its new size).
static BYTES: AtomicUsize = AtomicUsize::new(0);

fn grew(by: usize, size: usize) {
    LIVE.fetch_add(by, Relaxed);
    ALLOCS.fetch_add(1, Relaxed);
    BYTES.fetch_add(size, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            grew(layout.size(), layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            if size > layout.size() {
                grew(size - layout.size(), size);
            } else {
                LIVE.fetch_sub(layout.size() - size, Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const SIZE: Resolution = Resolution::new(1024, 768);
const FRAMES: u64 = 600;

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

#[test]
fn level_frame_heap_traffic() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();
    let sound = level.take_sound_data().unwrap();
    let mut system = SoundSystem::new(24);
    let mut level_sounds = LevelSounds::create(&mut system, &tree, &data.aliases, &sound).unwrap();
    drop(sound);
    let mut run = SoundRun::default();
    let mut clock = GlobalClock::default();
    let mut renderer = renderer();
    let mut presenter = Presenter::new();
    let eye = level.player().view().eye;

    // Each part's allocations and bytes over the measured frames.
    let mut parts = [("update", 0, 0), ("sounds", 0, 0), ("scene", 0, 0), ("draw list", 0, 0)];
    let mut worst = 0;
    let mut live_after_warmup = 0;
    for t in 1..=FRAMES + 60 {
        let measured = t > 60;
        if t == 61 {
            live_after_warmup = LIVE.load(Relaxed);
        }
        let ms = 16 * t;
        let mut at = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
        let frame_start = at.0;
        let mut part = |k: usize, at: &mut (usize, usize)| {
            let now = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
            if measured {
                parts[k].1 += now.0 - at.0;
                parts[k].2 += now.1 - at.1;
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
        run.frame(&mut system, &mut level_sounds, &inputs, now, false, &mut particles)
            .unwrap();
        system.update(ms, &mut NoDevice);
        part(1, &mut at);
        let scene = level.scene(SIZE, ms);
        part(2, &mut at);
        let list = presenter.draw_list(&mut renderer, &tree, &scene, ms);
        drop((list, scene));
        part(3, &mut at);
        if measured {
            worst = worst.max(at.0 - frame_start);
        }
    }
    let (allocs, bytes) = parts
        .iter()
        .fold((0, 0), |(a, b), p| (a + p.1, b + p.2));
    eprintln!(
        "per frame ({FRAMES} frames): {:.0} allocations, {:.1} KB allocated (most allocations in a frame: {worst})",
        allocs as f64 / FRAMES as f64,
        bytes as f64 / FRAMES as f64 / 1e3
    );
    for (name, a, b) in parts {
        eprintln!(
            "  {name}: {:.1} allocations, {:.1} KB",
            a as f64 / FRAMES as f64,
            b as f64 / FRAMES as f64 / 1e3
        );
    }
    eprintln!(
        "live during the frames: {:+.2} MB",
        (LIVE.load(Relaxed) as f64 - live_after_warmup as f64) / 1e6
    );
    assert!(presenter.take_errors().is_empty());
}
