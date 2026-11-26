// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The imported game in OPFS, from the engine worker's side: mounting it as
//! one file tree and the operations the page uses to browse it.
//!
//! Layout under the `volya-engine` namespace: `game/` holds the extracted
//! files, `import.json` the [`Manifest`] written by the importer, `user.ltx`
//! the user configuration file and `savedgames/` the saved games.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use volya_engine::core::audio::Playback;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{EntryKind, Vfs, VfsFile};
use volya_engine::saved_games::{HEAD, LocalTime, SaveFiles, SavedFile, SavedGames};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{FileSystemFileHandle, FileSystemHandleKind};

use crate::opfs;

pub const GAME_DIR: &str = "game";
pub const MANIFEST: &str = "import.json";
/// The user configuration file (docs/behavior/options-screens.md section
/// 12): Volya Engine's own copy beside the imported game, never written
/// into the game's files.
pub const USER_CONFIG: &str = "user.ltx";

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
    /// The installer the files came from (absent in manifests written
    /// before it was recorded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installer: Option<InstallerIdentity>,
}

/// Who the installer is, as its files and headers say (diagnostics).
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct InstallerIdentity {
    /// The picked files' names.
    pub exe: String,
    pub bin: String,
    /// The setup's `AppName` and `AppVersion`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
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
        Some(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| error(format!("{MANIFEST}: {e}"))),
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
    /// Why there's no usable import although one was made: its record is
    /// unreadable. Shown on the import screen; importing starts afresh.
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

/// Mounts the game if it is imported and not yet mounted, then reports.
pub async fn status() -> Result<Status, JsValue> {
    if let Some(status) = GAME.with(|g| g.borrow().as_ref().map(mounted_status)) {
        return Ok(status);
    }
    let manifest = match read_manifest().await {
        Ok(manifest) => manifest,
        Err(e) => {
            return Ok(Status {
                imported: false,
                written: 0,
                total: None,
                bytes: 0,
                total_bytes: None,
                problem: Some(e.as_string().unwrap_or_default()),
            });
        }
    };
    let Some(manifest) = manifest else {
        return Ok(Status {
            imported: false,
            written: 0,
            total: None,
            bytes: 0,
            total_bytes: None,
            problem: None,
        });
    };
    if !manifest.complete {
        return Ok(Status {
            imported: false,
            written: manifest.files.len(),
            total: None,
            bytes: manifest.files.iter().map(|f| f.size).sum(),
            total_bytes: (manifest.total_bytes > 0).then_some(manifest.total_bytes),
            problem: None,
        });
    }
    // Files that can't be opened (locked by another copy of the app in
    // browsers without read-only handles, missing, damaged): still imported;
    // Play reports the error (`ensure_mounted`).
    match mount(manifest).await {
        Ok(status) => Ok(status),
        Err((_, manifest)) => Ok(Status {
            imported: true,
            written: manifest.files.len(),
            total: Some(manifest.files.len()),
            bytes: manifest.files.iter().map(|f| f.size).sum(),
            total_bytes: None,
            problem: None,
        }),
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
    let game = open(manifest).await?;
    let status = mounted_status(&game);
    GAME.with(|g| *g.borrow_mut() = Some(game));
    Ok(status)
}

/// The imported game opened for a test of its own (render checks reading
/// game data, #472): the app's mount is released first, as the files open
/// once. Closed when dropped; the app mounts the game again when it needs
/// it.
#[cfg(feature = "test-ops")]
pub struct OwnTree(Option<Game>);

#[cfg(feature = "test-ops")]
impl OwnTree {
    pub async fn open() -> Result<Self, JsValue> {
        unmount();
        let manifest = read_manifest()
            .await?
            .filter(|m| m.complete)
            .ok_or_else(|| error("game not imported"))?;
        open(manifest)
            .await
            .map(|g| Self(Some(g)))
            .map_err(|(e, _)| e)
    }

    /// The combined archive tree.
    pub fn tree(&self) -> &dyn Vfs {
        &self.0.as_ref().expect("open until dropped").tree
    }
}

#[cfg(feature = "test-ops")]
impl Drop for OwnTree {
    fn drop(&mut self) {
        if let Some(game) = self.0.take() {
            drop(game.tree);
            game.opfs.close();
        }
    }
}

/// [`mount`]'s opening, without keeping the game.
async fn open(manifest: Manifest) -> Result<Game, (JsValue, Manifest)> {
    let namespace = match opfs::namespace().await {
        Ok(n) => n,
        Err(e) => return Err((e, manifest)),
    };
    let dir = match opfs::create_dir(&namespace, GAME_DIR).await {
        Ok(d) => d,
        Err(e) => return Err((e, manifest)),
    };
    let opened = match opfs::OpfsVfs::mount(&dir).await {
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
    Ok(Game {
        opfs: vfs,
        tree,
        manifest,
    })
}

fn mounted_status(game: &Game) -> Status {
    Status {
        imported: true,
        written: game.manifest.files.len(),
        total: Some(game.manifest.files.len()),
        bytes: game.manifest.files.iter().map(|f| f.size).sum(),
        total_bytes: None,
        problem: None,
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

/// The imported game as the diagnostic log's session event records it
/// (docs/architecture/diagnostics.md): `game` (`imported`, `partly
/// imported`, `not imported`, or `unreadable` with `game_problem`) and, from
/// the import's record, `game_version` (the title menu's caption),
/// `game_language` and, where recorded, the installer's `installer_exe`,
/// `installer_bin`, `installer_name` and `installer_version`.
pub async fn identity() -> serde_json::Map<String, serde_json::Value> {
    let mounted = GAME.with(|g| {
        let g = g.borrow();
        let m = &g.as_ref()?.manifest;
        Some(Manifest {
            language: m.language.clone(),
            complete: m.complete,
            installer: m.installer.clone(),
            ..Manifest::default()
        })
    });
    let manifest = match mounted {
        Some(m) => Ok(Some(m)),
        None => read_manifest().await,
    };
    let mut args = serde_json::Map::new();
    let mut text = |key: &str, value: &str| {
        args.insert(key.into(), value.into());
    };
    match manifest {
        Err(e) => {
            text("game", "unreadable");
            text("game_problem", &e.as_string().unwrap_or_default());
        }
        Ok(None) => text("game", "not imported"),
        Ok(Some(m)) => {
            text(
                "game",
                if m.complete {
                    "imported"
                } else {
                    "partly imported"
                },
            );
            text(
                "game_version",
                &format!("ver. {}", volya_engine::data::BUILT_IN_VERSION),
            );
            text("game_language", &m.language);
            if let Some(i) = &m.installer {
                text("installer_exe", &i.exe);
                text("installer_bin", &i.bin);
                if let Some(name) = &i.app_name {
                    text("installer_name", name);
                }
                if let Some(version) = &i.app_version {
                    text("installer_version", version);
                }
            }
        }
    }
    args
}

/// Runs `f` on the combined archive tree and the game root (loose files).
pub fn with_trees<T>(f: impl FnOnce(&dyn Vfs, &dyn Vfs) -> T) -> Result<T, JsValue> {
    with_game(|game| {
        #[cfg(feature = "test-ops")]
        if let Some(suffix) = hidden::active() {
            let tree = hidden::Hiding {
                inner: &game.tree,
                suffix,
            };
            return Ok(f(&tree, game.opfs));
        }
        Ok(f(&game.tree, game.opfs))
    })
}

/// A file the archive tree hides (tests: the `debug_hide_file` op), as a
/// missing one would be: a fatal error the game meets.
#[cfg(feature = "test-ops")]
pub mod hidden {
    use std::cell::RefCell;
    use std::io;

    use volya_engine::core::vfs::{self, DirEntry, Vfs, VfsFile};

    thread_local! {
        /// The end of the paths hidden, lowercase.
        static HIDDEN: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    /// Hides the paths ending in `suffix` (`None`: none).
    pub fn set(suffix: Option<&str>) {
        HIDDEN.with(|h| *h.borrow_mut() = suffix.map(str::to_ascii_lowercase));
    }

    /// The end of the paths hidden now.
    pub fn active() -> Option<String> {
        HIDDEN.with(|h| h.borrow().clone())
    }

    pub struct Hiding<'a> {
        pub inner: &'a dyn Vfs,
        pub suffix: String,
    }

    impl Hiding<'_> {
        fn hides(&self, path: &str) -> bool {
            vfs::normalize(path).is_ok_and(|p| p.to_ascii_lowercase().ends_with(&self.suffix))
        }
    }

    impl Vfs for Hiding<'_> {
        fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
            if self.hides(path) {
                return Err(io::ErrorKind::NotFound.into());
            }
            self.inner.open(path)
        }
        fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
            self.inner.read_dir(path)
        }
        fn exists(&self, path: &str) -> bool {
            !self.hides(path) && self.inner.exists(path)
        }
    }
}

/// Releases the imported files so they can be rewritten or removed.
pub fn unmount() {
    crate::engine::stop("unmounted", "the game's files were released");
    crate::audio::stop_all();
    if let Some(game) = GAME.with(|g| g.borrow_mut().take()) {
        drop(game.tree);
        game.opfs.close();
    }
}

/// Attempts at opening the user configuration file, and the pause between.
const CONFIG_ATTEMPTS: u32 = 20;
const CONFIG_RETRY_MS: i32 = 25;

/// The stored user configuration file, `None` before the first store
/// (retried while a write holds it, as [`write_user_config`]).
pub async fn read_user_config() -> Result<Option<Vec<u8>>, JsValue> {
    let namespace = opfs::namespace().await?;
    let mut attempt = 0;
    loop {
        match opfs::read_file(&namespace, USER_CONFIG).await {
            Err(_) if attempt < CONFIG_ATTEMPTS => {
                attempt += 1;
                opfs::sleep(CONFIG_RETRY_MS).await?;
            }
            result => return result,
        }
    }
}

/// Stores the user configuration file. A file's sync access handle is
/// exclusive, so a read of it at the same moment (or an earlier write still
/// open) makes the attempt fail: it is retried a few times (our choice).
pub async fn write_user_config(bytes: Vec<u8>) -> Result<(), JsValue> {
    let namespace = opfs::namespace().await?;
    let mut attempt = 0;
    loop {
        match opfs::write_file(&namespace, USER_CONFIG, &bytes).await {
            Err(_) if attempt < CONFIG_ATTEMPTS => {
                attempt += 1;
                opfs::sleep(CONFIG_RETRY_MS).await?;
            }
            result => return result,
        }
    }
}

/// The saved-games folder (`$game_saves$`, docs/formats/sav.md section
/// 1.1): Volya Engine's own, beside `user.ltx` (`$app_data_root$`). Nothing
/// creates it yet: saves are not written (#75).
pub const SAVED_GAMES: &str = "savedgames";

thread_local! {
    /// The `.sav` files directly in the saved-games folder, as the `File`
    /// snapshots the listing took (no access handle is held).
    static SAVES: RefCell<HashMap<String, web_sys::File>> = RefCell::new(HashMap::new());
}

/// The bytes of `blob`, read synchronously (`FileReaderSync`, which
/// dedicated workers have; documented).
fn read_blob(blob: &web_sys::Blob) -> Result<Vec<u8>, JsValue> {
    let buffer = web_sys::FileReaderSync::new()?.read_as_array_buffer(blob)?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// Reads the `.sav` files of [`SAVES`] when the load screen asks.
struct SaveReader;

impl SaveFiles for SaveReader {
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        let file = SAVES.with(|s| s.borrow().get(path).cloned());
        let file =
            file.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, path.to_owned()))?;
        // A file changed since the listing fails to read (the snapshot is
        // stale); the screen then treats it as not valid.
        read_blob(&file).map_err(|e| std::io::Error::other(format!("{path}: {e:?}")))
    }
}

/// The saved-games folder's files with their modification times, and the
/// first bytes of the `.sav` files directly in it (what the validity test
/// reads); those files' contents are read when the load screen asks. No
/// file stays open, so another copy of the app can delete saves meanwhile.
/// Empty without the folder.
pub async fn saved_games() -> Result<SavedGames, JsValue> {
    close_saved_games();
    let namespace = opfs::namespace().await?;
    let Some(root) = opfs::existing_dir(&namespace, SAVED_GAMES).await? else {
        return Ok(SavedGames::default());
    };
    let mut files = Vec::new();
    let mut snapshots = HashMap::new();
    let mut pending = vec![(String::new(), root)];
    while let Some((path, dir)) = pending.pop() {
        for handle in opfs::entries(&dir).await? {
            let name = handle.name();
            let child = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}\\{name}")
            };
            if handle.kind() == FileSystemHandleKind::Directory {
                pending.push((child, handle.dyn_into()?));
                continue;
            }
            let file: FileSystemFileHandle = handle.dyn_into()?;
            let blob: web_sys::File = JsFuture::from(file.get_file()).await?.dyn_into()?;
            let ms = blob.last_modified();
            // The browser's local time zone.
            let date = js_sys::Date::new(&JsValue::from_f64(ms));
            let modified = LocalTime {
                year: date.get_full_year(),
                month: date.get_month() + 1,
                day: date.get_date(),
                hour: date.get_hours(),
                minute: date.get_minutes(),
            };
            let mut head = Vec::new();
            if path.is_empty() && name.to_lowercase().ends_with(".sav") {
                let first = blob.slice_with_i32_and_i32(0, HEAD as i32)?;
                head = read_blob(&first).unwrap_or_default();
                snapshots.insert(child.clone(), blob);
            }
            files.push(SavedFile {
                path: child,
                modified_ms: ms,
                modified,
                head,
            });
        }
    }
    SAVES.with(|s| *s.borrow_mut() = snapshots);
    Ok(SavedGames::new(files, Some(Rc::new(SaveReader))))
}

/// Forgets the saved games' snapshots.
pub fn close_saved_games() {
    SAVES.with(|s| s.borrow_mut().clear());
}

/// Deletes file `path` (relative to the saved-games folder, `\` between
/// folders) from it; only that file goes.
pub async fn delete_saved_file(path: &str) -> Result<(), JsValue> {
    SAVES.with(|s| s.borrow_mut().remove(path));
    let namespace = opfs::namespace().await?;
    let Some(mut dir) = opfs::existing_dir(&namespace, SAVED_GAMES).await? else {
        return Ok(());
    };
    let mut parts: Vec<&str> = path.split('\\').collect();
    let name = parts.pop().unwrap_or_default();
    for part in parts {
        let Some(d) = opfs::existing_dir(&dir, part).await? else {
            return Ok(());
        };
        dir = d;
    }
    opfs::remove_file(&dir, name).await
}

/// Deletes the stored user configuration file (tests); the installer's copy
/// applies again.
pub async fn forget_user_config() -> Result<(), JsValue> {
    let namespace = opfs::namespace().await?;
    // An absent file is fine.
    let _ = opfs::remove(&namespace, USER_CONFIG).await;
    Ok(())
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

/// [`read`] into `buffer` ([`volya_engine::core::vfs::read_into`]).
pub fn read_into(path: &str, buffer: &mut Vec<u8>) -> Result<(), JsValue> {
    with_tree(|tree| volya_engine::core::vfs::read_into(tree, path, buffer))
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
pub fn base64(bytes: &[u8]) -> String {
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
