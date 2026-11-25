// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the offline simulation of a New Game costs on the Russian edition
//! (BYOA; docs/behavior/offline-simulation.md): the heap the game graph,
//! the registry of every object and the current level's navigation data
//! keep, the time to create them, and the time of one simulation update
//! while the player walks across `l01_escape`. Its own test binary, so no
//! other test allocates meanwhile; meaningful in release builds. Skips
//! when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]
// A counting allocator needs `unsafe impl GlobalAlloc`; it only forwards to
// the system allocator.
#![allow(unsafe_code)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use volya_engine_core::config;
use volya_engine_core::simulation::graph::Graph;
use volya_engine_core::simulation::navigation::Navigation;
use volya_engine_core::simulation::{AlifeConfig, Simulation};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, read};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::patrol_path::PatrolPaths;

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

fn held() -> usize {
    CURRENT.load(Ordering::Relaxed)
}

#[test]
fn simulation_memory_and_update_time() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let system = config::load(&tree, "gamedata/config/system.ltx").unwrap();
    let alife = config::load(&tree, "gamedata/config/alife.ltx").unwrap();
    let spawn_file = read(&tree, "gamedata/spawns/all.spawn").unwrap();
    let graph_file = read(&tree, "gamedata/game.graph").unwrap();

    let base = held();
    PEAK.store(base, Ordering::Relaxed);
    let t = Instant::now();
    let graph = GameGraph::parse(&graph_file).unwrap();
    let paths = PatrolPaths::parse(&spawn_file).unwrap();
    let after_graph = held();
    let mut sim = Simulation::new(
        Graph::new(graph),
        paths,
        AlifeConfig::read(&alife, &system).unwrap(),
    );
    sim.new_game(&spawn_file, &system, &mut |_, _| Ok(5))
        .unwrap();
    let created = t.elapsed();
    let after_registry = held();
    let t = Instant::now();
    let nav = Navigation::read(&tree, "gamedata\\levels\\l01_escape", &sim.graph.graph, 1).unwrap();
    sim.set_navigation(nav).unwrap();
    let navigation = t.elapsed();
    let after_navigation = held();
    let peak = PEAK.load(Ordering::Relaxed);
    eprintln!(
        "graph + paths {:.1} MB, registry of {} objects {:.1} MB, navigation {:.1} MB, \
         total {:.1} MB (peak {:.1} MB); created in {:.0} ms, navigation in {:.0} ms",
        mb(after_graph - base),
        sim.len(),
        mb(after_registry - after_graph),
        mb(after_navigation - after_registry),
        mb(after_navigation - base),
        mb(peak - base),
        created.as_secs_f64() * 1e3,
        navigation.as_secs_f64() * 1e3,
    );

    // The player walks from the start across the level and back, 2 m per
    // frame, one update per frame (60 Hz, ×10: 167 game ms).
    let start = 63_471_533_400_000u64;
    let at = sim.object(0).unwrap().record.header.position;
    let mut times = Vec::new();
    let mut switches = 0;
    for frame in 0..1_000u64 {
        let x = if frame < 500 {
            frame as f32 * 2.0
        } else {
            (1_000 - frame) as f32 * 2.0
        };
        sim.set_actor_position([at[0] + x * 0.3, at[1], at[2] + x]);
        let t = Instant::now();
        let c = sim.update(start + frame * 167, 10.0, 5).unwrap();
        times.push(t.elapsed().as_secs_f64() * 1e6);
        switches += c.online.len() + c.offline.len();
    }
    let first = times[0];
    let rest = &mut times[1..];
    rest.sort_by(f64::total_cmp);
    eprintln!(
        "update: first {:.0} µs, then median {:.0} µs, 99th {:.0} µs, max {:.0} µs; \
         {switches} switches over 1,000 frames; heap after {:.1} MB",
        first,
        rest[rest.len() / 2],
        rest[rest.len() * 99 / 100],
        rest[rest.len() - 1],
        mb(held() - base),
    );
    assert!(switches > 671);
    // Budgets: measured (23.7 MB kept, 23.7 MB peak), plus a modest
    // margin; the update time only where optimized (21 µs median, 37 µs
    // 99th percentile, measured).
    assert!(mb(after_navigation - base) < 27.0, "kept");
    assert!(mb(peak - base) < 27.0, "peak");
    if !cfg!(debug_assertions) {
        assert!(rest[rest.len() / 2] < 200.0, "median update");
        assert!(
            rest[rest.len() * 99 / 100] < 500.0,
            "99th percentile update"
        );
    }
}
