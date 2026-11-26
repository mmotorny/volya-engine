// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Heap use of each New Game loading stage on the Russian edition (BYOA):
//! the peak above the stage's start and what the stage keeps, with the
//! allocations it makes and the blocks it keeps; each peak is reported
//! against a budget (an "OVER BUDGET" note, never a failure: budgets are
//! reported, not gates). The loading peak is the wasm heap's high-water mark,
//! which never shrinks. Its own test binary, so no other test allocates
//! meanwhile. Skips when `VOLYA_GOG_INSTALLER` is unset.

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
/// Blocks live: many small long-lived ones split the free space the wasm
/// heap's large blocks need (docs/architecture/memory.md).
static BLOCKS: AtomicUsize = AtomicUsize::new(0);
/// Allocations made (a growing reallocation counts as one).
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
            BLOCKS.fetch_add(1, Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
        BLOCKS.fetch_sub(1, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            if size > layout.size() {
                ALLOCS.fetch_add(1, Ordering::Relaxed);
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

/// Each stage's peak above its start, MB: the measured peak (native,
/// `l01_escape`, #79) with a margin, so a regression is noted. Stages not
/// listed allocate next to nothing.
const STAGE_BUDGETS: [(Stage, f64); 9] = [
    (Stage::CreatingNewGame, 1.5), // 0.7: both files streamed (12.8 before, #403)
    (Stage::ServerConnecting, 19.0), // 17.2: the offline simulation (#370, #403)
    (Stage::OpeningStream, 14.0),  // 12.6: the `level` file and its parse
    (Stage::LoadingCform, 38.0),   // 35.1: the collision world it keeps (#371)
    (Stage::LoadingGeometry, 1.5), // 0.7: the geometry's layout only
    (Stage::LoadingSpatialDb, 7.0), // 5.4: index pieces and the draw list
    // 9.5: `level.details`' slot grid (8.4) and the cache (#411).
    (Stage::LoadingDetails, 10.5),
    // 11.0: the objects' models (9.4) and the zones' effect objects.
    (Stage::ClientSpawning, 12.5),
    (Stage::LoadingTextures, 4.5), // 3.2
];
/// What the archive index keeps, MB: 1.3 measured, in 18 blocks (7.7 in
/// 87,312 before #300).
const INDEX_BUDGET: f64 = 2.0;
/// What the configuration keeps, MB: 1.2 measured (5.6 in about 100,000
/// blocks before #299).
const CONFIG_BUDGET: f64 = 1.5;
/// What the string tables keep, MB: 1.2 measured, in 2 blocks (1.8 in
/// 15,344 before #484).
const STRINGS_BUDGET: f64 = 1.5;
/// What the effect library keeps, MB: 1.4 measured, in 1,095 blocks (1.9
/// in 8,411 before #484), read at launch (with each level's
/// `st_loading_ai_objects` before, peaking 3.3).
const EFFECTS_BUDGET: f64 = 1.8;
/// Any other stage's peak, MB.
const OTHER_BUDGET: f64 = 0.5;
/// The whole load's peak above its start, MB (72.8 measured with the
/// detail objects' grid, #411; 63.3 without it, 82.3 before #370 and #371,
/// 188.3 before #79).
const LOADING_BUDGET: f64 = 79.0;

#[test]
fn loading_stage_memory() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let (before, blocks) = (
        CURRENT.load(Ordering::Relaxed),
        BLOCKS.load(Ordering::Relaxed),
    );
    PEAK.store(before, Ordering::Relaxed);
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let start = CURRENT.load(Ordering::Relaxed);
    eprintln!(
        "the archive index: keeps {:.1} MB in {} blocks, peak +{:.1} MB",
        mb(start - before),
        BLOCKS.load(Ordering::Relaxed) - blocks,
        mb(PEAK.load(Ordering::Relaxed) - before)
    );
    if mb(start - before) > INDEX_BUDGET {
        eprintln!("  OVER BUDGET: the archive index keeps over {INDEX_BUDGET} MB");
    }
    let mut game_blocks = (BLOCKS.load(Ordering::Relaxed), 0);
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    eprintln!(
        "the game's data (configuration, UI, strings): keeps {:.1} MB",
        mb(CURRENT.load(Ordering::Relaxed) - start)
    );
    game_blocks.1 = BLOCKS.load(Ordering::Relaxed);
    let mut load = NewGameLoad::default();
    let base = CURRENT.load(Ordering::Relaxed);
    let mut highest = 0;
    for stage in Stage::ALL {
        let before = CURRENT.load(Ordering::Relaxed);
        let (blocks, allocs) = (
            BLOCKS.load(Ordering::Relaxed),
            ALLOCS.load(Ordering::Relaxed),
        );
        PEAK.store(before, Ordering::Relaxed);
        load.run(stage, &tree, &data).unwrap();
        let (peak, after) = (
            PEAK.load(Ordering::Relaxed),
            CURRENT.load(Ordering::Relaxed),
        );
        highest = highest.max(peak - base);
        eprintln!(
            "{stage:?}: peak +{:.1} MB above its start, keeps {:+.1} MB (loading total {:.1} MB); {} allocations, keeps {:+} blocks",
            mb(peak - before),
            mb(after) - mb(before),
            mb(after - base),
            ALLOCS.load(Ordering::Relaxed) - allocs,
            BLOCKS.load(Ordering::Relaxed) as isize - blocks as isize,
        );
        let budget = STAGE_BUDGETS
            .iter()
            .find(|(s, _)| *s == stage)
            .map_or(OTHER_BUDGET, |&(_, b)| b);
        if mb(peak - before) > budget {
            eprintln!("  OVER BUDGET: {stage:?} peaks over its {budget} MB budget");
        }
        // The offline simulation keeps 17.2 MB with the game graph
        // (measured).
        if stage == Stage::ServerConnecting && mb(after) - mb(before) >= 19.0 {
            eprintln!("  OVER BUDGET: {stage:?} keeps 19 MB or more");
        }
    }
    eprintln!("loading peak: {:.1} MB above the start", mb(highest));
    eprintln!(
        "blocks live: {} before the game's data, {} with it, {} at the load's end",
        game_blocks.0,
        game_blocks.1,
        BLOCKS.load(Ordering::Relaxed)
    );
    let before = CURRENT.load(Ordering::Relaxed);
    let screen = load.finish(0).unwrap();
    eprintln!(
        "the level made: {:+.1} MB, the level keeps {:.1} MB",
        mb(CURRENT.load(Ordering::Relaxed)) - mb(before),
        mb(CURRENT.load(Ordering::Relaxed) - base)
    );
    // What the steady level holds besides: the objects' geometry goes
    // once uploaded; the configuration and the string tables stay.
    let freed = |what: &str, f: &mut dyn FnMut()| {
        let (before, blocks) = (
            CURRENT.load(Ordering::Relaxed),
            BLOCKS.load(Ordering::Relaxed),
        );
        f();
        let freed = mb(before - CURRENT.load(Ordering::Relaxed));
        eprintln!(
            "{what}: {freed:.1} MB in {} blocks",
            blocks - BLOCKS.load(Ordering::Relaxed)
        );
        freed
    };
    freed("the objects' geometry until uploaded", &mut || {
        drop(screen.world().objects.take_geometry());
    });
    drop(screen);
    // What outlives the level: kept for the next one.
    eprintln!(
        "after the level: {:.1} MB and {:+} blocks above the load's start",
        mb(CURRENT.load(Ordering::Relaxed).saturating_sub(base)),
        BLOCKS.load(Ordering::Relaxed) as isize - game_blocks.1 as isize
    );
    let (mut system, mut strings, mut effects) =
        (Some(data.system), Some(data.strings), Some(data.effects));
    let configuration = freed(
        "the configuration (system.ltx and its includes)",
        &mut || {
            drop(system.take());
        },
    );
    if configuration > CONFIG_BUDGET {
        eprintln!("  OVER BUDGET: the configuration keeps over {CONFIG_BUDGET} MB");
    }
    if freed("the string tables", &mut || drop(strings.take())) > STRINGS_BUDGET {
        eprintln!("  OVER BUDGET: the string tables keep over {STRINGS_BUDGET} MB");
    }
    if freed("the effect library", &mut || drop(effects.take())) > EFFECTS_BUDGET {
        eprintln!("  OVER BUDGET: the effect library keeps over {EFFECTS_BUDGET} MB");
    }
    if mb(highest) > LOADING_BUDGET {
        eprintln!(
            "OVER BUDGET: the load peaks at +{:.1} MB, over its {LOADING_BUDGET} MB budget",
            mb(highest)
        );
    }
}
