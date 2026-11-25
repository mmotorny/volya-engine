// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the levels' placed effects cost on the Russian edition (BYOA;
//! docs/behavior/level-placed-effects.md): the heap the library and the
//! instances hold, and the CPU time of a frame's update with the camera at
//! the effects, for `l01_escape` and, for scale, the level with the most
//! records (`l12_stancia_2`). Its own test binary, so no other test
//! allocates meanwhile; meaningful in release builds. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]
// A counting allocator needs `unsafe impl GlobalAlloc`; it only forwards to
// the system allocator.
#![allow(unsafe_code)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use volya_engine_core::particles::placed::PlacedEffects;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, read};
use volya_engine_formats::particles::{Library, parse_placed};

struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            if size > layout.size() {
                let now = CURRENT.fetch_add(size - layout.size(), Ordering::Relaxed)
                    + (size - layout.size());
                PEAK.fetch_max(now, Ordering::Relaxed);
            } else {
                CURRENT.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn held() -> usize {
    CURRENT.load(Ordering::Relaxed)
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[test]
fn placed_effects_memory_and_frame_time() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let file = read(&tree, "gamedata/particles.xr").unwrap();

    let base = held();
    PEAK.store(base, Ordering::Relaxed);
    let t = Instant::now();
    let library = Arc::new(Library::parse(&file).unwrap());
    let parse = t.elapsed();
    let after_library = held();
    drop(file);
    eprintln!(
        "library: {:.2} MiB held (peak {:.2} MiB with the file), parsed in {:.1} ms",
        mib(after_library - base),
        mib(PEAK.load(Ordering::Relaxed) - base),
        parse.as_secs_f64() * 1e3
    );
    assert!(mib(after_library - base) < 8.0, "library");

    for level in ["l01_escape", "l12_stancia_2"] {
        let bytes = read(
            &tree,
            &format!("gamedata\\levels\\{level}\\level.ps_static"),
        )
        .unwrap();
        let records = parse_placed(&bytes).unwrap();
        let mut times = Vec::with_capacity(records.len() * 600 + 600);
        let before = held();
        let mut placed = PlacedEffects::new(library.clone(), &records, 0).unwrap();
        // The camera at each record in turn, 10 s each, 60 frames a
        // second: every instance near it updated about every frame.
        let mut peak_held = 0;
        for r in &records {
            let camera = [r.transform[3][0], r.transform[3][1], r.transform[3][2]];
            for _ in 0..600 {
                let t = Instant::now();
                placed.frame(16, camera);
                times.push(t.elapsed().as_secs_f64() * 1e6);
                peak_held = peak_held.max(held() - before);
            }
            if level == "l01_escape" {
                // Its one record; then ten more seconds.
                for _ in 0..600 {
                    let t = Instant::now();
                    placed.frame(17, camera);
                    times.push(t.elapsed().as_secs_f64() * 1e6);
                }
            }
        }
        times.sort_by(f64::total_cmp);
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        eprintln!(
            "{level}: {} records, {} particles; frame update mean {:.1} µs, median {:.1} µs, \
             99th {:.1} µs, max {:.1} µs; instances hold {:.1} KiB (peak {:.1} KiB)",
            records.len(),
            placed.particle_count(),
            mean,
            times[times.len() / 2],
            times[times.len() * 99 / 100],
            times[times.len() - 1],
            (held() - before) as f64 / 1024.0,
            peak_held as f64 / 1024.0,
        );
        if !cfg!(debug_assertions) && level == "l01_escape" {
            assert!(times[times.len() * 99 / 100] < 100.0, "99th percentile");
        }
    }
}
