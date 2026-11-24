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
use volya_engine::core::audio::Playback;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{EntryKind, Vfs, VfsFile};
use wasm_bindgen::{JsCast, JsValue};

use crate::opfs;

pub const GAME_DIR: &str = "game";
pub const MANIFEST: &str = "import.json";

/// What the importer has written; `complete` once every file is in place.
#[derive(Default, Serialize, Deserialize)]
pub struct Manifest {
    pub language: String,
    pub complete: bool,
    pub files: Vec<ManifestFile>,
    /// Bytes the import writes in all (0 in manifests written before it was
    /// recorded), so an interrupted import can say how far it got.
    #[serde(default)]
    pub total_bytes: u64,
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
    /// Bytes an interrupted import was writing in all, when recorded.
    total_bytes: Option<u64>,
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
            total_bytes: None,
        });
    };
    if !manifest.complete {
        return Ok(Status {
            imported: false,
            written: manifest.files.len(),
            total: None,
            bytes: manifest.files.iter().map(|f| f.size).sum(),
            total_bytes: (manifest.total_bytes > 0).then_some(manifest.total_bytes),
        });
    }
    // Files locked by another copy of the app (browsers without read-only
    // handles): still imported; Play reports the browser's error (`mount`).
    match mount(manifest).await {
        Ok(status) => Ok(status),
        Err((e, manifest)) if is_lock_error(&e) => Ok(Status {
            imported: true,
            written: manifest.files.len(),
            total: Some(manifest.files.len()),
            bytes: manifest.files.iter().map(|f| f.size).sum(),
            total_bytes: None,
        }),
        Err((e, _)) => Err(e),
    }
}

/// Mounts the imported game if it isn't: the browser's error otherwise (e.g.
/// its lock error when another copy of the app holds the files).
pub async fn ensure_mounted() -> Result<(), JsValue> {
    if GAME.with(|g| g.borrow().is_some()) {
        return Ok(());
    }
    let manifest = read_manifest()
        .await?
        .filter(|m| m.complete)
        .ok_or_else(|| error("game not imported"))?;
    mount(manifest).await.map(|_| ()).map_err(|(e, _)| e)
}

/// Opens every imported file and the archive tree; gives the manifest back
/// with the error.
async fn mount(manifest: Manifest) -> Result<Status, (JsValue, Manifest)> {
    let namespace = match opfs::namespace().await {
        Ok(n) => n,
        Err(e) => return Err((e, manifest)),
    };
    let dir = match opfs::create_dir(&namespace, GAME_DIR).await {
        Ok(d) => d,
        Err(e) => return Err((e, manifest)),
    };
    let opened = match mount_when_unlocked(&dir).await {
        Ok(v) => v,
        Err(e) => return Err((e, manifest)),
    };
    // Leaked so the tree can borrow it for 'static; `unmount` closes it.
    let vfs: &'static opfs::OpfsVfs = Box::leak(Box::new(opened));
    for file in &manifest.files {
        let size = vfs.open(&file.path).map(|f| f.size()).ok();
        if size != Some(file.size) {
            vfs.close();
            let message = format!(
                "imported game is damaged ({} missing or wrong size); import it again",
                file.path
            );
            return Err((error(message), manifest));
        }
    }
    let mods = vfs.read_dir("mods").is_ok().then_some("mods");
    let tree = match DbVfs::open_game(vfs, mods) {
        Ok(t) => t,
        Err(e) => {
            vfs.close();
            return Err((error(e), manifest));
        }
    };
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
/// game is open elsewhere (`None`).
/// Mounts, retrying a lock error for 5 s: after a reload the old page's
/// worker may still be letting go of its handles.
async fn mount_when_unlocked(
    dir: &web_sys::FileSystemDirectoryHandle,
) -> Result<opfs::OpfsVfs, JsValue> {
    const ATTEMPTS: u32 = 20;
    for attempt in 1..=ATTEMPTS {
        match opfs::OpfsVfs::mount(dir).await {
            Err(e) if is_lock_error(&e) && attempt < ATTEMPTS => opfs::sleep(250).await?,
            result => return result,
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
        total_bytes: None,
    }
}

fn with_game<T>(f: impl FnOnce(&Game) -> Result<T, JsValue>) -> Result<T, JsValue> {
    GAME.with(|g| {
        let game = g.borrow();
        f(game.as_ref().ok_or_else(|| error("game not imported"))?)
    })
}

/// Plays an Ogg Vorbis file from the game tree; returns the voice id.
pub fn play_sound(path: &str, playback: Playback) -> Result<u32, JsValue> {
    with_game(|game| crate::audio::play(&game.tree, path, playback))
}

/// Starts a music track from the game tree; returns the voice ids.
pub fn play_music(track: &str) -> Result<[u32; 2], JsValue> {
    with_game(|game| crate::audio::play_music(&game.tree, track))
}

/// Runs `f` on the combined archive tree and the game root (loose files).
pub fn with_trees<T>(f: impl FnOnce(&dyn Vfs, &dyn Vfs) -> T) -> Result<T, JsValue> {
    with_game(|game| Ok(f(&game.tree, game.opfs)))
}

/// Releases the imported files so they can be rewritten or removed.
pub fn unmount() {
    crate::engine::stop();
    crate::audio::stop_all();
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
    with_game(|game| f(&game.tree).map_err(error))
}

/// The whole file at `path` in the game tree.
pub fn read(path: &str) -> Result<Vec<u8>, JsValue> {
    with_tree(|tree| volya_engine::core::vfs::read(tree, path))
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

#[derive(Serialize)]
pub struct Cover {
    width: u32,
    height: u32,
    /// RGBA, 8 bits per channel, rows top to bottom, in base64.
    rgba: String,
}

/// Art from the imported game for the home screen: its splash picture
/// (startup.md section 3), `None` when the game has none.
pub fn cover() -> Result<Option<Cover>, JsValue> {
    with_game(|game| {
        let splash = volya_engine::app::splash(game.opfs).map_err(error)?;
        Ok(splash.map(|image| Cover {
            width: image.width,
            height: image.height,
            rgba: base64(&image.pixels),
        }))
    })
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}
