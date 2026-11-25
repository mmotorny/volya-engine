// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Importing the game from a GOG installer into OPFS, run in the import
//! worker (`static/import-worker.js`); docs/architecture/game-data.md.
//!
//! Installer files arrive as JS *sources*: objects with a `size` and a
//! synchronous `read(offset, length) -> Uint8Array` (a picked `File` read with
//! `FileReaderSync`, or, in tests, HTTP range requests).

use std::cell::RefCell;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

use js_sys::{Function, Reflect, Uint8Array};
use serde::Serialize;
use volya_engine_installer::{GameFile, Installer, extract, hex};
use wasm_bindgen::prelude::*;
use web_sys::{FileSystemReadWriteOptions, FileSystemSyncAccessHandle};

use crate::game::{GAME_DIR, MANIFEST, Manifest, ManifestFile};
use crate::opfs;

/// `Read + Seek` over a JS source.
struct Source {
    object: JsValue,
    read: Function,
    size: u64,
    pos: u64,
}

impl Source {
    fn new(object: JsValue) -> Result<Self, JsValue> {
        let size = Reflect::get(&object, &"size".into())?
            .as_f64()
            .ok_or("source has no size")? as u64;
        let read = Reflect::get(&object, &"read".into())?.dyn_into()?;
        Ok(Self {
            object,
            read,
            size,
            pos: 0,
        })
    }
}

impl Read for Source {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = (buf.len() as u64).min(self.size.saturating_sub(self.pos));
        if len == 0 {
            return Ok(0);
        }
        let chunk: Uint8Array = self
            .read
            .call2(
                &self.object,
                &(self.pos as f64).into(),
                &(len as f64).into(),
            )
            .and_then(|v| v.dyn_into())
            .map_err(|e| io::Error::other(format!("installer read: {e:?}")))?;
        let n = (chunk.length() as usize).min(buf.len());
        chunk.subarray(0, n as u32).copy_to(&mut buf[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Source {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let (base, offset) = match pos {
            SeekFrom::Start(n) => (0, n as i64),
            SeekFrom::Current(n) => (self.pos, n),
            SeekFrom::End(n) => (self.size, n),
        };
        self.pos = base
            .checked_add_signed(offset)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?;
        Ok(self.pos)
    }
}

/// Files the engine reads: the game archives (docs/formats/gog-installer.md),
/// the executable holding the splash picture and the default user settings
/// (docs/behavior/startup.md sections 3 and 4).
fn wanted(file: &GameFile) -> bool {
    file.path.starts_with("gamedata.db")
        || (file.path.starts_with("mods/") && file.path.contains(".xdb"))
        || file.path == volya_engine::app::SPLASH_EXE
        || file.path == volya_engine::data::DEFAULT_USER_LTX
}

thread_local! {
    /// The planned files and their language (empty when the installer tags
    /// none), for `run_import`.
    static PLAN: RefCell<(String, Vec<GameFile>)> = const { RefCell::new((String::new(), Vec::new())) };
}

#[derive(Serialize)]
struct Plan {
    /// The language chosen from the installer's data (empty when it tags none).
    language: String,
    files: Vec<PlanFile>,
    total: u64,
}

#[derive(Serialize)]
struct PlanFile {
    path: String,
    size: u64,
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Parses the installer's `.exe` and returns the files an import would write
/// (JSON `{language, files: [{path, size}], total}`). The language comes from
/// the installer's data, preferring the device's `preferred` languages
/// (BCP 47 tags) among those it offers.
#[wasm_bindgen]
pub fn plan_import(exe: JsValue, preferred: Vec<String>) -> Result<String, JsValue> {
    let mut source = Source::new(exe)?;
    let mut bytes = Vec::with_capacity(source.size as usize);
    source.read_to_end(&mut bytes).map_err(error)?;
    let installer = Installer::parse(Cursor::new(bytes)).map_err(error)?;
    let language = installer.choose_language(&preferred).unwrap_or_default();
    let files: Vec<GameFile> = installer
        .game_files(&language)
        .filter(|f| wanted(f))
        .cloned()
        .collect();
    if files.is_empty() {
        return Err(error("no game archives in this installer"));
    }
    let plan = Plan {
        language: language.clone(),
        total: files.iter().map(|f| f.size).sum(),
        files: files
            .iter()
            .map(|f| PlanFile {
                path: f.path.clone(),
                size: f.size,
            })
            .collect(),
    };
    PLAN.with(|p| *p.borrow_mut() = (language, files));
    serde_json::to_string(&plan).map_err(error)
}

/// Writes inflated bytes to an OPFS file, reporting progress.
struct OpfsWriter<'a> {
    handle: &'a FileSystemSyncAccessHandle,
    at: FileSystemReadWriteOptions,
    pos: u64,
    progress: &'a dyn Fn(u64),
}

impl Write for OpfsWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.at.set_at(self.pos as f64);
        let n = self
            .handle
            .write_with_u8_array_and_options(buf, &self.at)
            .map_err(|e| io::Error::other(format!("OPFS write: {e:?}")))? as usize;
        self.pos += n as u64;
        (self.progress)(n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.handle
            .flush()
            .map_err(|e| io::Error::other(format!("OPFS flush: {e:?}")))
    }
}

/// Extracts the planned files (or only those whose paths are in `only`) from
/// the `.bin` into OPFS, verifying every MD5. Resumes after an interruption:
/// files the manifest already lists are skipped. Calls
/// `progress(doneBytes, totalBytes, path)` as data is written.
#[wasm_bindgen]
pub async fn run_import(bin: JsValue, only: JsValue, progress: Function) -> Result<(), JsValue> {
    let only: Option<Vec<String>> = if only.is_null() || only.is_undefined() {
        None
    } else {
        Some(serde_wasm_only(&only)?)
    };
    let (language, files): (String, Vec<GameFile>) = PLAN.with(|p| {
        let plan = p.borrow();
        let files = plan
            .1
            .iter()
            .filter(|f| only.as_ref().is_none_or(|o| o.contains(&f.path)))
            .cloned()
            .collect();
        (plan.0.clone(), files)
    });
    if files.is_empty() {
        return Err(error("nothing to import; plan first"));
    }
    let mut bin = Source::new(bin)?;
    let namespace = opfs::namespace().await?;
    let dir = opfs::create_dir(&namespace, GAME_DIR).await?;
    // An unreadable record can't vouch for any file: start afresh.
    let mut manifest = crate::game::read_manifest()
        .await
        .unwrap_or_default()
        .filter(|m| m.language == language)
        .unwrap_or_default();
    manifest.language = language;
    manifest.complete = false;

    let total: u64 = files.iter().map(|f| f.size).sum();
    manifest.total_bytes = total;
    let done = std::cell::Cell::new(0u64);
    for file in &files {
        let md5 = hex(&file.md5);
        if manifest
            .files
            .iter()
            .any(|m| m.path == file.path && m.md5 == md5)
        {
            done.set(done.get() + file.size);
            continue;
        }
        let handle = opfs::open_sync(&dir, &file.path).await?;
        let report = |n: u64| {
            done.set(done.get() + n);
            let _ = progress.call3(
                &JsValue::NULL,
                &(done.get() as f64).into(),
                &(total as f64).into(),
                &file.path.as_str().into(),
            );
        };
        let mut writer = OpfsWriter {
            handle: &handle,
            at: FileSystemReadWriteOptions::new(),
            pos: 0,
            progress: &report,
        };
        let result = extract(file, &mut bin, &mut writer)
            .and_then(|()| writer.flush())
            .map_err(error)
            .and_then(|()| handle.truncate_with_f64(file.size as f64))
            .and_then(|()| handle.flush());
        handle.close();
        result?;
        manifest.files.push(ManifestFile {
            path: file.path.clone(),
            size: file.size,
            md5,
        });
        write_manifest(&namespace, &manifest).await?;
    }
    manifest.complete = true;
    write_manifest(&namespace, &manifest).await
}

async fn write_manifest(
    namespace: &web_sys::FileSystemDirectoryHandle,
    manifest: &Manifest,
) -> Result<(), JsValue> {
    let json = serde_json::to_vec(manifest).map_err(error)?;
    opfs::write_file(namespace, MANIFEST, &json).await
}

fn serde_wasm_only(value: &JsValue) -> Result<Vec<String>, JsValue> {
    let array: js_sys::Array = value.clone().dyn_into()?;
    array
        .iter()
        .map(|v| v.as_string().ok_or_else(|| error("`only` must list paths")))
        .collect()
}
