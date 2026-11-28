// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The file system's index (script API F 5; db-archive.md 7's combined
//! index of paths): every file and folder by its full path, lowercase, `\`
//! between folders, a folder's ending in `\`. It is a view: the archive tree
//! the [`Trees`] lend, under the loose files the engine adds
//! ([`FileIndex::set_loose_folder`]: the saved-games folder and the other
//! folders of `$app_data_root$`), and the changes scripts made to either.
//! What scripts change on the disk is kept for the platform
//! ([`FileIndex::take_disk_changes`]).

use std::collections::BTreeMap;
use std::io;
use std::rc::Rc;

use volya_engine_core::vfs::{self, EntryKind, Vfs};
use volya_engine_formats::cp1251;

use crate::loader::Trees;

/// `fs_file.vfs` of a loose file (5.3).
pub const LOOSE: u32 = u32::MAX;

/// An entry of the index, as `fs_file` shows it (5.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileEntry {
    pub vfs: u32,
    pub ptr: u32,
    pub size_real: u32,
    pub size_compressed: u32,
    /// The last-write time, seconds since 1970-01-01 UTC (low 32 bits).
    pub modif: u32,
}

impl FileEntry {
    /// A loose file of `size` bytes last written at `modif` (5.3).
    pub fn loose(size: u32, modif: u32) -> Self {
        FileEntry {
            vfs: LOOSE,
            ptr: 0,
            size_real: size,
            size_compressed: size,
            modif,
        }
    }

    /// A member of the archive tree. Our choice where 5.3 leaves the
    /// archive's fields believed or unestablished: `vfs` 0, `ptr` 0, both
    /// sizes the file's, `modif` 0 (the tree keeps none of them).
    fn archive(size: u32) -> Self {
        FileEntry {
            vfs: 0,
            ptr: 0,
            size_real: size,
            size_compressed: size,
            modif: 0,
        }
    }
}

/// A loose file the engine adds, relative to its folder (`\` between
/// folders).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LooseFile {
    pub path: String,
    pub size: u32,
    /// Seconds since 1970-01-01 UTC.
    pub modif: u32,
}

/// Reads the loose files the engine added, by full path.
pub trait LooseFiles {
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
}

/// A change scripts made to the disk, by full path, for the platform to
/// apply in order. Only loose files change the disk: removing an archive
/// member removes its entry alone (5.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiskChange {
    /// Delete the file.
    Delete(String),
    /// Rename the file, making `to`'s folders first (5.4).
    Rename { from: String, to: String },
    /// Write a new file with these bytes (`file_copy`, 5.4).
    Write { path: String, bytes: Vec<u8> },
    /// Remove the (empty) folder (`dir_delete`, 5.4).
    RemoveFolder(String),
}

/// Where an entry the index holds over the tree reads its bytes.
#[derive(Clone, Debug)]
enum Source {
    /// A loose file at its own full path.
    Loose,
    /// A member of the archive tree at this path (moved by `file_rename`).
    Archive(String),
    /// A folder.
    Folder,
}

#[derive(Clone, Debug)]
struct Overlaid {
    entry: FileEntry,
    source: Source,
}

/// The index (see the module's docs).
pub struct FileIndex {
    trees: Rc<dyn Trees>,
    /// Entries over the tree's, by full path: `None` removes the tree's.
    overlay: BTreeMap<String, Option<Overlaid>>,
    loose: Option<Rc<dyn LooseFiles>>,
    changes: Vec<DiskChange>,
}

/// Whether `path` is a full path as the index holds them: lowercase, `\`
/// between non-empty folder names other than `.` and `..`, a folder's
/// ending in `\` (`""` is the root).
fn is_key(path: &str) -> bool {
    if path.bytes().any(|b| b.is_ascii_uppercase() || b == b'/') {
        return false;
    }
    let body = path.strip_suffix('\\').unwrap_or(path);
    body.is_empty() || body.split('\\').all(|p| !matches!(p, "" | "." | ".."))
}

fn is_folder(path: &str) -> bool {
    path.is_empty() || path.ends_with('\\')
}

/// The full path's bytes, for the index's byte order.
fn bytes(path: &str) -> Vec<u8> {
    cp1251::encode(path).unwrap_or_else(|| path.as_bytes().to_vec())
}

fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, path.to_owned())
}

impl FileIndex {
    /// The index over the archive tree `trees` lends.
    pub fn new(trees: Rc<dyn Trees>) -> Self {
        FileIndex {
            trees,
            overlay: BTreeMap::new(),
            loose: None,
            changes: Vec::new(),
        }
    }

    /// Indexes the loose files of the folder `folder` (a full path ending in
    /// `\`, lowercased here) as the platform listed them, replacing what the
    /// index had of loose files below it: the folder, its parents and every
    /// folder of `files` get entries. `reader` reads them.
    pub fn set_loose_folder(
        &mut self,
        folder: &str,
        files: &[LooseFile],
        reader: Rc<dyn LooseFiles>,
    ) {
        let folder = folder.to_ascii_lowercase();
        self.overlay
            .retain(|path, e| !(path.starts_with(&folder) && e.is_some()));
        self.add_folders(&folder);
        for file in files {
            let path = format!("{folder}{}", file.path.replace('/', "\\")).to_ascii_lowercase();
            self.add_folders(&path);
            self.overlay.insert(
                path,
                Some(Overlaid {
                    entry: FileEntry::loose(file.size, file.modif),
                    source: Source::Loose,
                }),
            );
        }
        self.loose = Some(reader);
    }

    /// Entries for every folder of `path` (itself, if it is a folder).
    fn add_folders(&mut self, path: &str) {
        for (i, _) in path.match_indices('\\') {
            self.overlay
                .entry(path[..=i].to_owned())
                .or_insert(Some(Overlaid {
                    entry: FileEntry::default(),
                    source: Source::Folder,
                }));
        }
    }

    /// The disk changes since the last call, in order.
    pub fn take_disk_changes(&mut self) -> Vec<DiskChange> {
        std::mem::take(&mut self.changes)
    }

    /// The tree's entry at `key`.
    fn tree_entry(&self, key: &str) -> Option<FileEntry> {
        if !is_key(key) {
            return None;
        }
        let mut found = None;
        let _ = self.trees.with_trees(&mut |tree, _| {
            found = tree_entry(tree, key);
        });
        found
    }

    /// The entry of full path `path` (exactly as given), or `None`.
    pub fn entry(&self, path: &str) -> Option<FileEntry> {
        match self.overlay.get(path) {
            Some(e) => e.as_ref().map(|o| o.entry),
            None => self.tree_entry(path),
        }
    }

    /// The bytes of the file at full path `path`.
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let tree_path = match self.overlay.get(path) {
            Some(None) => return Err(not_found(path)),
            Some(Some(Overlaid {
                source: Source::Loose,
                ..
            })) => {
                return match &self.loose {
                    Some(loose) => loose.read(path),
                    None => Err(not_found(path)),
                };
            }
            Some(Some(Overlaid {
                source: Source::Folder,
                ..
            })) => return Err(io::Error::new(io::ErrorKind::InvalidInput, path.to_owned())),
            Some(Some(Overlaid {
                source: Source::Archive(from),
                ..
            })) => from.as_str(),
            None if is_key(path) && !is_folder(path) => path,
            None => return Err(not_found(path)),
        };
        let mut read = Err(io::Error::other("the game's files are unavailable"));
        let lent = self
            .trees
            .with_trees(&mut |tree, _| read = vfs::read(tree, tree_path));
        lent.and(read)
    }

    /// Every entry below `folder` (full paths starting with it, not itself),
    /// in byte order of the full path; `None` when the index has no entry
    /// for `folder`.
    pub fn below(&self, folder: &str) -> Option<Vec<(String, FileEntry)>> {
        self.entry(folder)?;
        let mut found: BTreeMap<Vec<u8>, (String, FileEntry)> = BTreeMap::new();
        // The tree's folder holding every path starting with `folder`.
        let walked = &folder[..folder.rfind('\\').map_or(0, |i| i + 1)];
        if is_key(walked) {
            let _ = self.trees.with_trees(&mut |tree, _| {
                walk(tree, walked, &mut |path, entry| {
                    if path.starts_with(folder) && path != folder {
                        found.insert(bytes(&path), (path, entry));
                    }
                });
            });
        }
        for (path, e) in self.overlay.range(folder.to_owned()..) {
            if !path.starts_with(folder) {
                break;
            }
            if path == folder {
                continue;
            }
            match e {
                Some(o) => {
                    found.insert(bytes(path), (path.clone(), o.entry));
                }
                None => {
                    found.remove(&bytes(path));
                }
            }
        }
        Some(found.into_values().collect())
    }

    /// Whether the entry at `path` is a loose file: changing it changes the
    /// disk.
    fn on_disk(&self, path: &str) -> bool {
        matches!(
            self.overlay.get(path),
            Some(Some(Overlaid {
                source: Source::Loose,
                ..
            }))
        )
    }

    /// `file_delete` (5.4): when the index has `path`, deletes the file from
    /// the disk and its entry from the index. Whether it was deleted.
    pub fn delete(&mut self, path: &str) -> bool {
        if self.entry(path).is_none() {
            return false;
        }
        if self.on_disk(path) {
            self.changes.push(DiskChange::Delete(path.to_owned()));
        }
        self.overlay.insert(path.to_owned(), None);
        true
    }

    /// `file_rename(from, to, overwrite)` (5.4).
    pub fn rename(&mut self, from: &str, to: &str, overwrite: bool) {
        let Some(entry) = self.entry(from) else {
            return;
        };
        if self.entry(to).is_some() {
            if !overwrite {
                return;
            }
            self.delete(to);
        }
        let source = match self.overlay.get(from) {
            Some(Some(o)) => o.source.clone(),
            _ => Source::Archive(from.to_owned()),
        };
        let to = to.to_ascii_lowercase();
        if matches!(source, Source::Loose) {
            self.changes.push(DiskChange::Rename {
                from: from.to_owned(),
                to: to.clone(),
            });
        }
        self.overlay.insert(from.to_owned(), None);
        self.add_folders(&to);
        self.overlay.insert(to, Some(Overlaid { entry, source }));
    }

    /// `file_copy(from, to)` (5.4): when the index has `from`, its bytes are
    /// written to the new file `to`. The index is left as it is: 5.4 adds no
    /// entry, and the new file is indexed when its folder is listed again.
    pub fn copy(&mut self, from: &str, to: &str) -> io::Result<()> {
        if self.entry(from).is_none() {
            return Ok(());
        }
        let bytes = self.read(from)?;
        self.changes.push(DiskChange::Write {
            path: to.to_owned(),
            bytes,
        });
        Ok(())
    }

    /// `dir_delete(path, remove_files)` (5.4): unless `remove_files` is
    /// false and a file is below the folder, every file below it is deleted
    /// (as [`Self::delete`]) and the folders below it are removed from the
    /// disk, deepest first; their index entries stay (5.4 removes them from
    /// the disk only).
    // TODO(#994): gap 4, a folder the index has no entry for: the simplest
    // reading, nothing is deleted.
    pub fn delete_folder(&mut self, folder: &str, remove_files: bool) {
        let Some(below) = self.below(folder) else {
            return;
        };
        let (folders, files): (Vec<_>, Vec<_>) = below.into_iter().partition(|(p, _)| is_folder(p));
        if !remove_files && !files.is_empty() {
            return;
        }
        for (path, _) in &files {
            self.delete(path);
        }
        for (path, _) in folders.iter().rev() {
            if matches!(
                self.overlay.get(path),
                Some(Some(Overlaid {
                    source: Source::Folder,
                    ..
                }))
            ) {
                self.changes.push(DiskChange::RemoveFolder(path.clone()));
            }
        }
    }
}

/// `tree`'s entry at `key` (a full path in the index's form).
fn tree_entry(tree: &dyn Vfs, key: &str) -> Option<FileEntry> {
    if is_folder(key) {
        return tree.read_dir(key).ok().map(|_| FileEntry::default());
    }
    let (dir, name) = key.rsplit_once('\\').unwrap_or(("", key));
    tree.read_dir(dir)
        .ok()?
        .into_iter()
        .find(|e| e.kind == EntryKind::File && e.name == name)
        .map(|e| FileEntry::archive(e.size.min(u64::from(u32::MAX)) as u32))
}

/// Calls `f` with every entry below the tree's folder `dir` (full paths).
fn walk(tree: &dyn Vfs, dir: &str, f: &mut dyn FnMut(String, FileEntry)) {
    let Ok(entries) = tree.read_dir(dir) else {
        return;
    };
    for e in entries {
        let name = e.name.to_ascii_lowercase();
        match e.kind {
            EntryKind::File => f(
                format!("{dir}{name}"),
                FileEntry::archive(e.size.min(u64::from(u32::MAX)) as u32),
            ),
            EntryKind::Dir => {
                let sub = format!("{dir}{name}\\");
                f(sub.clone(), FileEntry::default());
                walk(tree, &sub, f);
            }
        }
    }
}

/// The index as a [`Vfs`] (reads only), for configuration files and their
/// includes: paths are looked up lowercased with `\` between folders, as
/// [`volya_engine_core::config`] makes them.
pub(crate) struct IndexVfs<'a>(pub &'a FileIndex);

impl Vfs for IndexVfs<'_> {
    fn open(&self, path: &str) -> io::Result<Box<dyn vfs::VfsFile + '_>> {
        let bytes = self.0.read(&path.replace('/', "\\"))?;
        Ok(Box::new(Memory(io::Cursor::new(bytes))))
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<vfs::DirEntry>> {
        Err(not_found(path))
    }
}

struct Memory(io::Cursor<Vec<u8>>);

impl io::Read for Memory {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Seek for Memory {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

impl vfs::VfsFile for Memory {
    fn size(&self) -> u64 {
        self.0.get_ref().len() as u64
    }
}
