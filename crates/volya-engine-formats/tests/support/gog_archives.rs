// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Test support: the Russian edition's game files, as Bazel's `@gog_game`
//! holds them (docs/architecture/build.md, Game data): its `manifest.tsv`,
//! named by `VOLYA_GOG_GAME`, lists them (path, size, MD5, in installer
//! order; empty without game data), in `files/` beside it. Shared by
//! game-data tests of several crates (`#[path]`).

#![allow(dead_code)] // Each test crate uses a different part.

use std::fs;
use std::path::{Path, PathBuf};

/// Whether `path` (relative to the game root, `/`-separated) is an archive:
/// a root file whose last extension starts with `.db`, or `mods/*.xdb*`.
pub fn is_archive(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let root_db = !path.contains('/')
        && name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.starts_with("db"));
    root_db || (path.starts_with("mods/") && name.contains(".xdb"))
}

/// A game file, extracted.
pub struct CachedArchive {
    /// Path relative to the game root, `/`-separated.
    pub path: String,
    pub file: PathBuf,
    pub size: u64,
    /// MD5 recorded by the installer; extraction verified the bytes against it.
    pub md5: String,
}

/// The game's archives (`gamedata.db*`, `mods/*.xdb*`) in installer order, or
/// `None` (after saying so) without game data.
pub fn archives() -> Option<Vec<CachedArchive>> {
    game_files("archives", is_archive)
}

/// The game files whose paths pass `select`, in installer order, or `None`
/// (after saying so) without game data. `_name` names the selection.
pub fn game_files(_name: &str, select: impl Fn(&str) -> bool) -> Option<Vec<CachedArchive>> {
    let Some(manifest) = std::env::var_os("VOLYA_GOG_GAME") else {
        eprintln!("skipped: no game data (VOLYA_GOG_GAME is unset: run the test with Bazel)");
        return None;
    };
    // The runfiles hold the manifest alone; the files are beside its target.
    let manifest = Path::new(&manifest);
    let manifest =
        fs::canonicalize(manifest).unwrap_or_else(|e| panic!("{}: {e}", manifest.display()));
    let text = fs::read_to_string(&manifest).unwrap();
    if text.is_empty() {
        eprintln!("skipped: no game data (VOLYA_GOG_INSTALLER is unset)");
        return None;
    }
    let dir = manifest.parent().unwrap().join("files");
    let files = text
        .lines()
        .map(|line| {
            let mut fields = line.split('\t');
            let (Some(path), Some(size), Some(md5)) = (fields.next(), fields.next(), fields.next())
            else {
                panic!("{}: {line:?}", manifest.display());
            };
            (path, size.parse().unwrap(), md5)
        })
        .filter(|(path, _, _)| select(path))
        .map(|(path, size, md5)| CachedArchive {
            path: path.to_owned(),
            file: dir.join(path),
            size,
            md5: md5.to_owned(),
        })
        .collect();
    Some(files)
}
