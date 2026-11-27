// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Heap use of each New Game loading stage on the Russian edition (BYOA):
//! what each caption keeps (its `* phase cmem` growth) against the
//! original's and the end-of-load *h* against the original's, with no
//! headroom: every figure above one is an "EXCESS" note to file as an issue,
//! never a failure (docs/architecture/memory.md, Budgets). Beside them, each
//! stage's peak above its start, the allocations it makes and the blocks it
//! keeps, and what the program's data keeps. Its own test binary, so no
//! other test allocates meanwhile. Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

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

/// What the original keeps under each caption, K (docs/architecture/
/// memory.md, The original: its New Game of `l01_escape`, #667).
const ORIGINAL_KEPT: [(Stage, u64); 17] = [
    (Stage::PrefetchingObjects, 85_047),
    (Stage::ServerStarting, 205),
    (Stage::LoadingAlifeSimulator, 0),
    (Stage::CreatingNewGame, 32_164),
    (Stage::ServerConnecting, 0),
    (Stage::ClientConnectingTo, 8),
    (Stage::OpeningStream, 0),
    (Stage::LoadingCform, 72_669),
    (Stage::LoadingShaders, 1_400),
    (Stage::LoadingGeometry, 353),
    (Stage::LoadingSpatialDb, 4_297),
    (Stage::LoadingDetails, 10),
    (Stage::LoadingSectorsPortals, 143),
    (Stage::LoadingAiObjects, 0),
    (Stage::ClientSpawning, 4_708),
    (Stage::LoadingTextures, 1),
    (Stage::ClientSynchronising, 0),
];
/// The original's *h* at the load's end, K.
const ORIGINAL_H: u64 = 227_120;
/// The original's *h* when the load began, K (console-diagnostics.md 2.4.1):
/// reported beside, not a budget.
const ORIGINAL_START_H: u64 = 26_115;

/// K, as the original's figures: rounded down.
fn k(bytes: usize) -> u64 {
    bytes as u64 / 1024
}

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
    // `* phase cmem` as each caption begins.
    let mut marks = Vec::new();
    for stage in Stage::ALL {
        let before = CURRENT.load(Ordering::Relaxed);
        marks.push((stage, before));
        let (blocks, allocs) = (
            BLOCKS.load(Ordering::Relaxed),
            ALLOCS.load(Ordering::Relaxed),
        );
        PEAK.store(before, Ordering::Relaxed);
        load.run(stage, &tree, &data).unwrap();
        let peak = PEAK.load(Ordering::Relaxed);
        highest = highest.max(peak - base);
        eprintln!(
            "{stage:?}: {} K at its start (`* phase cmem`), peak +{:.1} MB above it; {} allocations, keeps {:+} blocks",
            k(before),
            mb(peak - before),
            ALLOCS.load(Ordering::Relaxed) - allocs,
            BLOCKS.load(Ordering::Relaxed) as isize - blocks as isize,
        );
    }
    // The level made ends the load (natively: no frames, no overlay).
    let screen = load.finish(0).unwrap();
    let end = CURRENT.load(Ordering::Relaxed);
    let mut excesses = Vec::new();
    for (i, &(stage, cmem)) in marks.iter().enumerate() {
        let next = marks.get(i + 1).map_or(end, |m| m.1);
        let kept = k(next) as i64 - k(cmem) as i64;
        let original = ORIGINAL_KEPT.iter().find(|o| o.0 == stage).unwrap().1;
        eprintln!(
            "{}: keeps {kept} K, the original {original} K",
            stage.caption_id()
        );
        if kept > original as i64 {
            excesses.push(format!(
                "{} keeps {kept} K, the original {original} K",
                stage.caption_id()
            ));
        }
    }
    eprintln!(
        "the load: {} K at its start (the original {ORIGINAL_START_H} K), h {} K at its end (the original {ORIGINAL_H} K), peak +{:.1} MB above its start",
        k(base),
        k(end),
        mb(highest)
    );
    if k(end) > ORIGINAL_H {
        excesses.push(format!(
            "h {} K at the load's end, the original {ORIGINAL_H} K",
            k(end)
        ));
    }
    for excess in &excesses {
        eprintln!("EXCESS over the original's (to file): {excess}");
    }
    eprintln!(
        "blocks live: {} before the game's data, {} with it, {} at the load's end",
        game_blocks.0,
        game_blocks.1,
        BLOCKS.load(Ordering::Relaxed)
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
    freed(
        "the configuration (system.ltx and its includes)",
        &mut || {
            drop(system.take());
        },
    );
    freed("the string tables", &mut || drop(strings.take()));
    freed("the effect library", &mut || drop(effects.take()));
}
