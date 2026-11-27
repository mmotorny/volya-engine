// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What each shipped level's own files take, Russian edition (BYOA), so the
//! largest level's memory is known beside `l01_escape`'s
//! (docs/architecture/memory.md): the heap the collision world and the
//! detail objects keep (counted as requested), the `level` file's parse
//! peak, what the file cache holds for the detail objects and the
//! navigation against the files they stand for, and as stored the geometry
//! the GPU takes and the textures the level's own surfaces name. Reported, never judged. Its
//! own test binary, so no other test allocates meanwhile. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]
// A counting allocator needs `unsafe impl GlobalAlloc`; it only forwards to
// the memory manager, the app's global allocator.
#![allow(unsafe_code)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::alloc::{GlobalAlloc, Layout};
use std::collections::HashSet;
use std::io::{Seek, SeekFrom};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use volya_engine::core::collision::CollisionWorld;
use volya_engine::core::file_cache;
use volya_engine::core::memory_manager::MemoryManager;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{self, EntryKind, StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level_details;
use volya_engine_formats::cross_table::CrossTable;
use volya_engine_formats::gamemtl;
use volya_engine_formats::level::Level;
use volya_engine_formats::level_ai::LevelAi;
use volya_engine_formats::ui::Resolution;

struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { MemoryManager.alloc(layout) };
        if !p.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Relaxed) + layout.size();
            PEAK.fetch_max(now, Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { MemoryManager.dealloc(p, layout) };
        CURRENT.fetch_sub(layout.size(), Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { MemoryManager.realloc(p, layout, size) };
        if !q.is_null() {
            if size > layout.size() {
                let now = CURRENT.fetch_add(size - layout.size(), Relaxed) + (size - layout.size());
                PEAK.fetch_max(now, Relaxed);
            } else {
                CURRENT.fetch_sub(layout.size() - size, Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1e6
}

/// Runs `f`: what it keeps live and its peak above its start.
fn measured<T>(f: impl FnOnce() -> T) -> (T, u64, u64) {
    let before = CURRENT.load(Relaxed);
    PEAK.store(before, Relaxed);
    let out = f();
    let kept = CURRENT.load(Relaxed).saturating_sub(before) as u64;
    (out, kept, (PEAK.load(Relaxed) - before) as u64)
}

/// The size of file `path`, 0 if absent.
fn size(tree: &dyn Vfs, path: &str) -> u64 {
    tree.open(path)
        .and_then(|mut f| f.seek(SeekFrom::End(0)))
        .unwrap_or(0)
}

#[test]
fn level_files_memory() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    let mtl = vfs::read(
        &tree,
        &data.aliases.path("$game_data$", "gamemtl.xr").unwrap(),
    )
    .unwrap();
    let materials = gamemtl::parse(&mtl).unwrap();
    let mut levels: Vec<String> = (tree.read_dir("gamedata/levels").unwrap().into_iter())
        .filter(|e| e.kind == EntryKind::Dir)
        .map(|e| e.name)
        .filter(|l| tree.exists(&format!("gamedata/levels/{l}/level.cform")))
        .collect();
    levels.sort();
    eprintln!(
        "level: collision kept, details kept, `level` read peak (heap MB); the detail slot grid in the file cache of `level.details`, the navigation positions and `level.gct` of `level.ai` and `level.gct` (MB, the bytes beside on the heap); geometry, the surfaces' textures (as stored MB)"
    );
    let mut rows = Vec::new();
    for name in &levels {
        let dir = data.aliases.path("$game_levels$", name).unwrap();
        let file = |f: &str| format!("{dir}\\{f}");
        let collision =
            CollisionWorld::read(tree.open(&file("level.cform")).unwrap(), materials.clone())
                .unwrap();
        let collision_kept = collision.memory_bytes() as u64;
        drop(collision);
        let (details, details_kept, _) =
            measured(|| level_details::read(&tree, &dir, &data.system).unwrap());
        // Its slot grid in the file cache, against the file it stands for.
        let grid = file_cache::bytes();
        drop(details);
        let details_file = size(&tree, &file("level.details"));
        // The file in the file cache, outside the heap, read in place.
        let (level, _, parse_peak) =
            measured(|| Level::parse(&file_cache::read(&tree, &file("level")).unwrap()).unwrap());
        // The textures the level's own surfaces name (`shader/texture,…`),
        // each once, from the level's folder or the game's textures.
        let mut seen = HashSet::new();
        let mut textures = 0;
        for entry in &level.shaders {
            let Some((_, names)) = entry.split_once('/') else {
                continue;
            };
            for t in names.split(',').filter(|t| !t.is_empty()) {
                if !seen.insert(t.to_owned()) {
                    continue;
                }
                let own = size(&tree, &file(&format!("{t}.dds")));
                textures += if own > 0 {
                    own
                } else {
                    let path = data
                        .aliases
                        .path("$game_textures$", &format!("{t}.dds"))
                        .unwrap();
                    size(&tree, &path)
                };
            }
        }
        drop(level);
        let geometry = size(&tree, &file("level.geom"));
        // The navigation as a level holds it: the positions held under
        // `level.ai`'s path, the cross table's cells under `level.gct`'s; on the heap
        // only their handles, and an order of the level vertices where the
        // file doesn't store them sorted.
        let (ai_path, gct_path) = (file("level.ai"), file("level.gct"));
        let navigation_files = size(&tree, &ai_path) + size(&tree, &gct_path);
        let (navigation, navigation_kept, _) = measured(|| {
            (tree.exists(&ai_path) && tree.exists(&gct_path)).then(|| {
                let ai = LevelAi::read_in(tree.open(&ai_path).unwrap(), |len| {
                    file_cache::hold(&ai_path, len)
                })
                .unwrap();
                let cross = CrossTable::read_in(tree.open(&gct_path).unwrap(), |len| {
                    file_cache::hold(&gct_path, len)
                })
                .unwrap();
                let sorted = (ai.packed_all())
                    .zip(ai.packed_all().skip(1))
                    .all(|(a, b)| a <= b);
                (ai, cross, sorted)
            })
        });
        let navigation_held = file_cache::bytes();
        let unsorted = navigation.as_ref().is_some_and(|n| !n.2);
        drop(navigation);
        eprintln!(
            "{name}: {:.1}, {:.1}, {:.1}; {:.1} of {:.1}, {:.1} of {:.1} ({} bytes on the heap{}); {:.1}, {:.1}",
            mb(collision_kept),
            mb(details_kept),
            mb(parse_peak),
            mb(grid),
            mb(details_file),
            mb(navigation_held),
            mb(navigation_files),
            navigation_kept,
            if unsorted { ", vertices unsorted" } else { "" },
            mb(geometry),
            mb(textures)
        );
        rows.push((
            name.clone(),
            collision_kept + details_kept,
            geometry + textures,
        ));
    }
    let heap = rows.iter().max_by_key(|r| r.1).unwrap();
    let gpu = rows.iter().max_by_key(|r| r.2).unwrap();
    eprintln!(
        "the most heap (collision and details): {} {:.1} MB; the most GPU as stored (geometry and surfaces' textures): {} {:.1} MB",
        heap.0,
        mb(heap.1),
        gpu.0,
        mb(gpu.2)
    );
}
