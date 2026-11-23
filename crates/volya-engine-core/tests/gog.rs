// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The combined archive tree of the Russian edition (BYOA), checked against
//! docs/formats/db-archive.md (sections 7 and 8). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::Instant;

use md5::{Digest, Md5};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};

fn md5_hex(data: &[u8]) -> String {
    Md5::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Counts the files under `dir`.
fn count_files(vfs: &dyn Vfs, dir: &str) -> usize {
    vfs.read_dir(dir)
        .unwrap()
        .into_iter()
        .map(|e| {
            let path = format!("{dir}/{}", e.name);
            match e.kind {
                EntryKind::File => 1,
                EntryKind::Dir => count_files(vfs, &path),
            }
        })
        .sum()
}

#[test]
fn combines_russian_archives() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let start = Instant::now();
    let vfs = DbVfs::open_game(&root, Some("mods")).unwrap();
    eprintln!(
        "mounted {} archives in {:.1?}",
        archives.len(),
        start.elapsed()
    );

    assert_eq!(
        root.read_dir("")
            .unwrap()
            .iter()
            .filter(|e| e.kind == EntryKind::File)
            .count(),
        14,
        "root archives"
    );
    let top: Vec<String> = vfs
        .read_dir("")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(top, ["gamedata"]);
    assert_eq!(count_files(&vfs, "gamedata"), 25_707);

    for (path, md5) in [
        (
            "gamedata/config/system.ltx",
            "3c35ee63942d827a970e2ddf6096cb40",
        ),
        (
            "gamedata/config/creatures/actor.ltx",
            "c35d9476c2d399f493158169168857a5",
        ),
        (
            "gamedata/config/text/rus/string_table_general.xml",
            "ea3d5d35bb518eefa32a520f205a49eb",
        ),
        (
            "gamedata/scripts/_g.script",
            "c7562102faf72dc95222be7f913533df",
        ),
        (
            "gamedata/levels/mp_military_2/level.game",
            "b3f3577f8ecd502d69f7fb93e182fb77",
        ),
    ] {
        assert_eq!(md5_hex(&read(&vfs, path).unwrap()), md5, "{path}");
    }
    let system = read(&vfs, "gamedata/config/system.ltx").unwrap();
    let system = String::from_utf8_lossy(&system);
    assert!(system.contains("current_server_entity_version = 7"));
    let map = vfs
        .open("gamedata/textures/map/map_mp_military_2.dds")
        .unwrap();
    assert_eq!(map.size(), 5_592_560);
}
