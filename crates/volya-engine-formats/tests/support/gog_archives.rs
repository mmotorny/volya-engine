// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Test support: the Russian edition's `.db` / `.xdb` archives, extracted
//! once from the GOG installer named by `VOLYA_GOG_INSTALLER` and cached under
//! `target/tmp/`. Shared by game-data tests of several crates (`#[path]`).

#![allow(dead_code)] // Each test crate uses a different part.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use volya_engine_installer::{Installer, extract, hex};

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

/// An archive extracted into the cache.
pub struct CachedArchive {
    /// Path relative to the game root, `/`-separated.
    pub path: String,
    pub file: PathBuf,
    pub size: u64,
    /// MD5 recorded by the installer; extraction verified the bytes against it.
    pub md5: String,
}

/// The game's archives (`gamedata.db*`, `mods/*.xdb*`) in installer order, or
/// `None` (after saying so) when `VOLYA_GOG_INSTALLER` is unset.
pub fn archives() -> Option<Vec<CachedArchive>> {
    static LOCK: Mutex<()> = Mutex::new(());
    let Some(dir) = std::env::var_os("VOLYA_GOG_INSTALLER") else {
        eprintln!("skipped: VOLYA_GOG_INSTALLER is unset");
        return None;
    };
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cache = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("gog-ru-RU-archives");

    let exe = fs::read_dir(&dir)
        .expect("VOLYA_GOG_INSTALLER")
        .map(|e| e.unwrap().path())
        .find(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("setup_s.t.a.l.k.e.r._shadow_of_chernobyl")
                && name.contains("(russian)")
                && name.ends_with(".exe")
        })
        .expect("no Russian Shadow of Chernobyl installer .exe");
    let bin = exe.with_file_name(format!(
        "{}-1.bin",
        exe.file_stem().unwrap().to_string_lossy()
    ));
    let installer = Installer::parse(BufReader::new(File::open(&exe).unwrap())).unwrap();
    let mut bin = BufReader::new(File::open(bin).unwrap());
    let mut archives = Vec::new();
    for file in installer
        .game_files("ru-RU")
        .filter(|f| is_archive(&f.path))
    {
        let path = cache.join(&file.path);
        archives.push(CachedArchive {
            path: file.path.clone(),
            file: path.clone(),
            size: file.size,
            md5: hex(&file.md5),
        });
        if fs::metadata(&path).is_ok_and(|m| m.len() == file.size) {
            continue;
        }
        let start = Instant::now();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let partial = cache.join(format!("{}.partial", file.path));
        let mut out = BufWriter::new(File::create(&partial).unwrap());
        extract(file, &mut bin, &mut out).unwrap();
        out.flush().unwrap();
        drop(out);
        fs::rename(&partial, &path).unwrap();
        eprintln!(
            "extracted {} ({} bytes) in {:.1?}",
            file.path,
            file.size,
            start.elapsed()
        );
    }
    Some(archives)
}
