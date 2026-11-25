// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Heap use of each New Game loading stage on the Russian edition (BYOA):
//! the peak above the stage's start and what the stage keeps. The loading
//! peak is the wasm heap's high-water mark, which never shrinks. Its own
//! test binary, so no other test allocates meanwhile. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]
// A counting allocator needs `unsafe impl GlobalAlloc`; it only forwards to
// the system allocator.
#![allow(unsafe_code)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::NewGameLoad;
use volya_engine::loading_screen::Stage;
use volya_engine_formats::ui::Resolution;

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

fn mb(bytes: usize) -> f64 {
    bytes as f64 / 1e6
}

#[test]
fn loading_stage_memory() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    let mut load = NewGameLoad::default();
    let base = CURRENT.load(Ordering::Relaxed);
    let mut highest = 0;
    for stage in Stage::ALL {
        let before = CURRENT.load(Ordering::Relaxed);
        PEAK.store(before, Ordering::Relaxed);
        load.run(stage, &tree, &data).unwrap();
        let (peak, after) = (
            PEAK.load(Ordering::Relaxed),
            CURRENT.load(Ordering::Relaxed),
        );
        highest = highest.max(peak - base);
        eprintln!(
            "{stage:?}: peak +{:.1} MB above its start, keeps {:+.1} MB (loading total {:.1} MB)",
            mb(peak - before),
            mb(after) - mb(before),
            mb(after - base)
        );
        // Budgets: measured, plus a modest margin.
        if stage == Stage::ServerConnecting {
            // The offline simulation: 19.8 MB peak and kept, measured.
            assert!(mb(peak - before) < 23.0, "{stage:?} peak");
            assert!(mb(after) - mb(before) < 23.0, "{stage:?} kept");
        }
    }
    eprintln!("loading peak: {:.1} MB above the start", mb(highest));
    // 188.3 MB measured.
    assert!(mb(highest) < 200.0, "loading peak");
}
