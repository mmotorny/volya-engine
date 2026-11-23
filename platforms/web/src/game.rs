// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The imported game in OPFS, from the engine worker's side: mounting it as
//! one file tree and the operations the page uses to browse it.
//!
//! Layout under the `volya-engine` namespace: `game/` holds the extracted
//! files, `import.json` the [`Manifest`] written by the importer.

use std::cell::RefCell;
use std::io::Read;

use serde::{Deserialize, Serialize};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{EntryKind, Vfs, VfsFile};
use wasm_bindgen::{JsCast, JsValue};

use crate::opfs;

pub const GAME_DIR: &str = "game";
pub const MANIFEST: &str = "import.json";
/// Language of the game data imported (docs/roadmap.md).
pub const LANGUAGE: &str = "ru-RU";

/// What the importer has written; `complete` once every file is in place.
#[derive(Default, Serialize, Deserialize)]
pub struct Manifest {
    pub language: String,
    pub complete: bool,
    pub files: Vec<ManifestFile>,
}

#[derive(Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
    pub md5: String,
}

struct Game {
    opfs: &'static opfs::OpfsVfs,
    tree: DbVfs<Box<dyn VfsFile + 'static>>,
    manifest: Manifest,
}

thread_local! {
    static GAME: RefCell<Option<Game>> = const { RefCell::new(None) };
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

pub async fn read_manifest() -> Result<Option<Manifest>, JsValue> {
    let namespace = opfs::namespace().await?;
    match opfs::read_file(&namespace, MANIFEST).await? {
        Some(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(error),
        None => Ok(None),
    }
}

#[derive(Serialize)]
pub struct Status {
    imported: bool,
    /// Files the importer has written so far (equals `total` once imported).
    written: usize,
    total: Option<usize>,
    bytes: u64,
}

/// Mounts the game if it is imported and not yet mounted, then reports.
pub async fn status() -> Result<Status, JsValue> {
    if let Some(status) = GAME.with(|g| g.borrow().as_ref().map(mounted_status)) {
        return Ok(status);
    }
    let Some(manifest) = read_manifest().await? else {
        return Ok(Status {
            imported: false,
            written: 0,
            total: None,
            bytes: 0,
        });
    };
    if !manifest.complete {
        return Ok(Status {
            imported: false,
            written: manifest.files.len(),
            total: None,
            bytes: manifest.files.iter().map(|f| f.size).sum(),
        });
    }
    let namespace = opfs::namespace().await?;
    let dir = opfs::create_dir(&namespace, GAME_DIR).await?;
    let opened = mount_when_unlocked(&dir).await?;
    // Leaked so the tree can borrow it for 'static; `unmount` closes it.
    let vfs: &'static opfs::OpfsVfs = Box::leak(Box::new(opened));
    for file in &manifest.files {
        let size = vfs.open(&file.path).map(|f| f.size()).ok();
        if size != Some(file.size) {
            vfs.close();
            return Err(error(format!(
                "imported game is damaged ({} missing or wrong size); import it again",
                file.path
            )));
        }
    }
    let mods = vfs.read_dir("mods").is_ok().then_some("mods");
    let tree = DbVfs::open_game(vfs, mods).map_err(|e| {
        vfs.close();
        error(e)
    })?;
    let game = Game {
        opfs: vfs,
        tree,
        manifest,
    };
    let status = mounted_status(&game);
    GAME.with(|g| *g.borrow_mut() = Some(game));
    Ok(status)
}

/// Mounts `dir`, waiting out file locks held by a previous page whose engine
/// worker is still shutting down (a reload); a lock that persists means the
/// game is open elsewhere.
async fn mount_when_unlocked(
    dir: &web_sys::FileSystemDirectoryHandle,
) -> Result<opfs::OpfsVfs, JsValue> {
    const ATTEMPTS: u32 = 20;
    for attempt in 1..=ATTEMPTS {
        match opfs::OpfsVfs::mount(dir).await {
            Ok(vfs) => return Ok(vfs),
            Err(e) if is_lock_error(&e) && attempt < ATTEMPTS => opfs::sleep(250).await?,
            Err(e) if is_lock_error(&e) => {
                return Err(error(
                    "the game is open in another tab or window of this app; close it and reload",
                ));
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// Whether `e` is the error a locked OPFS file gives (`NoModificationAllowedError`
/// in Chrome, `InvalidStateError` in WebKit).
fn is_lock_error(e: &JsValue) -> bool {
    e.dyn_ref::<js_sys::Error>()
        .map(|e| String::from(e.name()))
        .is_some_and(|name| name == "NoModificationAllowedError" || name == "InvalidStateError")
}

fn mounted_status(game: &Game) -> Status {
    Status {
        imported: true,
        written: game.manifest.files.len(),
        total: Some(game.manifest.files.len()),
        bytes: game.manifest.files.iter().map(|f| f.size).sum(),
    }
}

/// Releases the imported files so they can be rewritten or removed.
pub fn unmount() {
    if let Some(game) = GAME.with(|g| g.borrow_mut().take()) {
        drop(game.tree);
        game.opfs.close();
    }
}

/// Deletes the imported game.
pub async fn forget() -> Result<(), JsValue> {
    unmount();
    let namespace = opfs::namespace().await?;
    // Absent entries are fine.
    let _ = opfs::remove(&namespace, GAME_DIR).await;
    let _ = opfs::remove(&namespace, MANIFEST).await;
    Ok(())
}

fn with_tree<T>(f: impl FnOnce(&dyn Vfs) -> std::io::Result<T>) -> Result<T, JsValue> {
    GAME.with(|g| {
        let game = g.borrow();
        let game = game.as_ref().ok_or_else(|| error("game not imported"))?;
        f(&game.tree).map_err(error)
    })
}

#[derive(Serialize)]
pub struct Entry {
    name: String,
    dir: bool,
    size: u64,
}

pub fn read_dir(path: &str) -> Result<Vec<Entry>, JsValue> {
    with_tree(|tree| {
        let mut entries: Vec<Entry> = tree
            .read_dir(path)?
            .into_iter()
            .map(|e| Entry {
                name: e.name,
                dir: e.kind == EntryKind::Dir,
                size: e.size,
            })
            .collect();
        // Directories first, then by name.
        entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.cmp(&b.name)));
        Ok(entries)
    })
}

#[derive(Serialize)]
pub struct Head {
    size: u64,
    bytes: Vec<u8>,
}

/// The first `max` bytes of a file, for previews.
pub fn read_head(path: &str, max: u64) -> Result<Head, JsValue> {
    with_tree(|tree| {
        let file = tree.open(path)?;
        let size = file.size();
        let mut bytes = Vec::new();
        file.take(max).read_to_end(&mut bytes)?;
        Ok(Head { size, bytes })
    })
}
