// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! [`Vfs`] over the Origin Private File System. Worker-only: it relies on
//! `FileSystemSyncAccessHandle`.
//!
//! Opening handles is asynchronous, reading is not, so [`OpfsVfs::mount`]
//! walks a directory once and opens a sync access handle for every file;
//! afterwards all access is synchronous. Handles lock their files until the
//! VFS is dropped.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{self, Read, Seek, SeekFrom};

use js_sys::Reflect;
use volya_engine::core::vfs::{DirEntry, EntryKind, Vfs, VfsFile, normalize};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemHandle, FileSystemHandleKind, FileSystemReadWriteOptions,
    FileSystemRemoveOptions, FileSystemSyncAccessHandle, WorkerGlobalScope,
};

/// Every file the engine stores lives under this OPFS directory: the
/// github.io origin is shared with unrelated sites (docs/architecture.md).
pub const NAMESPACE: &str = "volya-engine";

/// The engine's OPFS namespace directory, created if needed.
pub async fn namespace() -> Result<FileSystemDirectoryHandle, JsValue> {
    let scope: WorkerGlobalScope = js_sys::global().dyn_into()?;
    let root: FileSystemDirectoryHandle =
        JsFuture::from(scope.navigator().storage().get_directory())
            .await?
            .dyn_into()?;
    subdir(&root, NAMESPACE, true).await
}

/// Resolves after `ms` milliseconds.
pub async fn sleep(ms: i32) -> Result<(), JsValue> {
    let scope: WorkerGlobalScope = js_sys::global().dyn_into()?;
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = scope.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    JsFuture::from(promise).await.map(drop)
}

/// Creates (or opens) directory `name` in `dir`.
pub async fn create_dir(
    dir: &FileSystemDirectoryHandle,
    name: &str,
) -> Result<FileSystemDirectoryHandle, JsValue> {
    subdir(dir, name, true).await
}

async fn subdir(
    dir: &FileSystemDirectoryHandle,
    name: &str,
    create: bool,
) -> Result<FileSystemDirectoryHandle, JsValue> {
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(create);
    JsFuture::from(dir.get_directory_handle_with_options(name, &options))
        .await?
        .dyn_into()
}

/// Opens `path` under `root` for synchronous access, creating it and its
/// directories as needed. The caller must close the handle.
/// A read-only sync access handle: browsers that support the mode (Chrome
/// 121 and later) let any number of pages hold one, so several copies of the
/// app can read the game at once. Others ignore the option and lock the file
/// to one handle; a second copy then gets their lock error. Through
/// `Reflect`: web-sys has the option only among its unstable APIs.
async fn open_read_only(
    file: &FileSystemFileHandle,
) -> Result<FileSystemSyncAccessHandle, JsValue> {
    let options = js_sys::Object::new();
    Reflect::set(&options, &"mode".into(), &"read-only".into())?;
    let create: js_sys::Function =
        Reflect::get(file, &"createSyncAccessHandle".into())?.dyn_into()?;
    let promise: js_sys::Promise = create.call1(file, &options)?.dyn_into()?;
    JsFuture::from(promise).await?.dyn_into()
}

pub async fn open_sync(
    root: &FileSystemDirectoryHandle,
    path: &str,
) -> Result<FileSystemSyncAccessHandle, JsValue> {
    let path = normalize(path).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let (dirs, name) = path.rsplit_once('/').unwrap_or(("", &path));
    let mut dir = root.clone();
    for part in dirs.split('/').filter(|p| !p.is_empty()) {
        dir = subdir(&dir, part, true).await?;
    }
    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let file: FileSystemFileHandle =
        JsFuture::from(dir.get_file_handle_with_options(name, &options))
            .await?
            .dyn_into()?;
    JsFuture::from(file.create_sync_access_handle())
        .await?
        .dyn_into()
}

/// Reads the whole file `name` in `dir`, or `None` if it doesn't exist.
pub async fn read_file(
    dir: &FileSystemDirectoryHandle,
    name: &str,
) -> Result<Option<Vec<u8>>, JsValue> {
    let Ok(file) = JsFuture::from(dir.get_file_handle(name)).await else {
        return Ok(None);
    };
    let file: FileSystemFileHandle = file.dyn_into()?;
    let handle: FileSystemSyncAccessHandle = JsFuture::from(file.create_sync_access_handle())
        .await?
        .dyn_into()?;
    let at = FileSystemReadWriteOptions::new();
    at.set_at(0.0);
    let result = handle.get_size().and_then(|size| {
        let mut data = vec![0u8; size as usize];
        handle
            .read_with_u8_array_and_options(&mut data, &at)
            .map(|_| data)
    });
    handle.close();
    result.map(Some)
}

/// Writes `data` to `path` under `root`, creating directories as needed.
pub async fn write_file(
    root: &FileSystemDirectoryHandle,
    path: &str,
    data: &[u8],
) -> Result<(), JsValue> {
    let handle = open_sync(root, path).await?;
    let at = FileSystemReadWriteOptions::new();
    at.set_at(0.0);
    let result = handle
        .write_with_u8_array_and_options(data, &at)
        .and_then(|_| handle.truncate_with_f64(data.len() as f64))
        .and_then(|_| handle.flush());
    handle.close();
    result
}

/// Removes `name` (file or directory, recursively) from `dir`.
pub async fn remove(dir: &FileSystemDirectoryHandle, name: &str) -> Result<(), JsValue> {
    let options = FileSystemRemoveOptions::new();
    options.set_recursive(true);
    JsFuture::from(dir.remove_entry_with_options(name, &options))
        .await
        .map(drop)
}

struct File {
    handle: FileSystemSyncAccessHandle,
    size: u64,
}

impl Drop for File {
    fn drop(&mut self) {
        self.handle.close();
    }
}

pub struct OpfsVfs {
    files: BTreeMap<String, File>,
    /// Directory path to its sorted entries.
    dirs: BTreeMap<String, Vec<DirEntry>>,
}

impl OpfsVfs {
    /// Releases every file lock now; later reads fail. For a VFS that is
    /// never dropped (e.g. leaked to give borrowers a `'static` lifetime).
    pub fn close(&self) {
        for file in self.files.values() {
            file.handle.close();
        }
    }

    /// Mounts the tree under `root`, opening every file read-only.
    pub async fn mount(root: &FileSystemDirectoryHandle) -> Result<Self, JsValue> {
        let mut vfs = Self {
            files: BTreeMap::new(),
            dirs: BTreeMap::new(),
        };
        let mut pending = vec![(String::new(), root.clone())];
        while let Some((path, dir)) = pending.pop() {
            let mut entries = Vec::new();
            let iter = dir.values();
            loop {
                let next = JsFuture::from(iter.next()?).await?;
                if Reflect::get(&next, &"done".into())?.is_truthy() {
                    break;
                }
                let handle: FileSystemHandle = Reflect::get(&next, &"value".into())?.dyn_into()?;
                let name = handle.name();
                let child = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}/{name}")
                };
                if handle.kind() == FileSystemHandleKind::Directory {
                    pending.push((child, handle.dyn_into()?));
                    entries.push(DirEntry {
                        name,
                        kind: EntryKind::Dir,
                        size: 0,
                    });
                } else {
                    let file: FileSystemFileHandle = handle.dyn_into()?;
                    let handle = open_read_only(&file).await?;
                    let size = handle.get_size()? as u64;
                    vfs.files.insert(child, File { handle, size });
                    entries.push(DirEntry {
                        name,
                        kind: EntryKind::File,
                        size,
                    });
                }
            }
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            vfs.dirs.insert(path, entries);
        }
        Ok(vfs)
    }
}

fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("no such file: {path:?}"))
}

impl Vfs for OpfsVfs {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        let file = self
            .files
            .get(&normalize(path)?)
            .ok_or_else(|| not_found(path))?;
        Ok(Box::new(OpfsFile {
            file,
            pos: Cell::new(0),
        }))
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        self.dirs
            .get(&normalize(path)?)
            .cloned()
            .ok_or_else(|| not_found(path))
    }
}

struct OpfsFile<'a> {
    file: &'a File,
    pos: Cell<u64>,
}

impl Read for OpfsFile<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let at = FileSystemReadWriteOptions::new();
        at.set_at(self.pos.get() as f64);
        let n = self
            .file
            .handle
            .read_with_u8_array_and_options(buf, &at)
            .map_err(|e| io::Error::other(format!("OPFS read: {e:?}")))? as u64;
        self.pos.set(self.pos.get() + n);
        Ok(n as usize)
    }
}

impl Seek for OpfsFile<'_> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let (base, offset) = match pos {
            SeekFrom::Start(n) => (0, n as i64),
            SeekFrom::Current(n) => (self.pos.get(), n),
            SeekFrom::End(n) => (self.file.size, n),
        };
        let new = base
            .checked_add_signed(offset)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?;
        self.pos.set(new);
        Ok(new)
    }
}

impl VfsFile for OpfsFile<'_> {
    fn size(&self) -> u64 {
        self.file.size
    }
}
