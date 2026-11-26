// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Test support: the Russian edition's `.db` / `.xdb` archives, extracted
//! once from the GOG installer named by `VOLYA_GOG_INSTALLER` into a cache
//! shared by every checkout (see [`cache_dir`]). Shared by game-data tests of
//! several crates (`#[path]`).

#![allow(dead_code)] // Each test crate uses a different part.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
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

/// A game file extracted into the cache.
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
    game_files("archives", is_archive)
}

/// The installer's `ru-RU` game files whose paths pass `select`, extracted
/// into `<cache_dir>/<name>/` in the game-root layout, in installer order; or
/// `None` (after saying so) when `VOLYA_GOG_INSTALLER` is unset. A file
/// already there with the installer's size is reused; a file appears only
/// once complete (written beside it, then renamed). An exclusive lock on
/// `<cache_dir>/<name>.lock` serialises extraction across processes.
pub fn game_files(name: &str, select: impl Fn(&str) -> bool) -> Option<Vec<CachedArchive>> {
    let Some(dir) = std::env::var_os("VOLYA_GOG_INSTALLER") else {
        eprintln!("skipped: VOLYA_GOG_INSTALLER is unset");
        return None;
    };
    let (exe, bin) = installer_parts(Path::new(&dir));
    let cache = cache_dir(&exe, &bin).join(name);
    fs::create_dir_all(&cache).unwrap();
    let lock = File::create(cache.with_extension("lock")).unwrap();
    lock.lock().unwrap();

    let installer = Installer::parse(BufReader::new(File::open(&exe).unwrap())).unwrap();
    let mut bin = BufReader::new(File::open(bin).unwrap());
    let mut files = Vec::new();
    for file in installer.game_files("ru-RU").filter(|f| select(&f.path)) {
        let path = cache.join(&file.path);
        files.push(CachedArchive {
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
            "extracted {} ({} bytes) to {} in {:.1?}",
            file.path,
            file.size,
            cache.display(),
            start.elapsed()
        );
    }
    Some(files)
}

/// The Russian Shadow of Chernobyl installer's `.exe` and `-1.bin` in `dir`.
fn installer_parts(dir: &Path) -> (PathBuf, PathBuf) {
    let exe = fs::read_dir(dir)
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
    (exe, bin)
}

/// One installer's cache, outside every checkout: in
/// `~/Library/Caches/volya-engine/` on macOS, `$XDG_CACHE_HOME/volya-engine/`
/// or `~/.cache/volya-engine/` elsewhere, named from the installer parts'
/// names and sizes.
fn cache_dir(exe: &Path, bin: &Path) -> PathBuf {
    let home = || PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let base = if cfg!(target_os = "macos") {
        home().join("Library/Caches")
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".cache"))
    };
    let size = |p: &Path| {
        fs::metadata(p)
            .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
            .len()
    };
    base.join("volya-engine").join(format!(
        "{}-{}-{}",
        exe.file_stem().unwrap().to_string_lossy(),
        size(exe),
        size(bin)
    ))
}
