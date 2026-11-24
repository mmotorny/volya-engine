// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! [`Vfs`] over the game's `.db` / `.xdb` archives combined into one tree
//! (docs/formats/db-archive.md, section 7).
//!
//! Each archive is mounted at a directory; later mounts replace earlier
//! entries file by file, and nothing is ever removed. Paths are compared
//! case-insensitively (ASCII), so lookups and listed names are lowercase.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::io::{self, Cursor, Read, Seek, SeekFrom};

use volya_engine_formats::db::{Archive, Entry, Scrambling};

use super::{DirEntry, EntryKind, Vfs, VfsFile, normalize};

/// An archive of the game and where it mounts, both relative to the game root
/// and `/`-separated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameArchive {
    pub path: String,
    pub mount: String,
}

/// Finds the game's archives in `game`, a [`Vfs`] rooted at the game root, in
/// mount order: files of the root directory whose last extension starts with
/// `.db`, by ascending byte order of name, each mounted at its path without
/// the last extension (`gamedata.db0` at `gamedata`); then, if `mods_dir` is
/// given, its `*.xdb*` files mounted at `gamedata`. The relative order of
/// several `.xdb` archives is not specified; they are taken by name too.
pub fn game_archives(game: &dyn Vfs, mods_dir: Option<&str>) -> io::Result<Vec<GameArchive>> {
    let files = |dir: &str| -> io::Result<Vec<String>> {
        let mut names: Vec<String> = game
            .read_dir(dir)?
            .into_iter()
            .filter(|e| e.kind == EntryKind::File)
            .map(|e| e.name)
            .collect();
        names.sort();
        Ok(names)
    };
    let mut archives = Vec::new();
    for name in files("")? {
        if let Some((stem, ext)) = name.rsplit_once('.')
            && ext.to_ascii_lowercase().starts_with("db")
        {
            archives.push(GameArchive {
                mount: stem.to_owned(),
                path: name,
            });
        }
    }
    if let Some(mods) = mods_dir {
        let mods = normalize(mods)?;
        for name in files(&mods)? {
            if name.to_ascii_lowercase().contains(".xdb") {
                archives.push(GameArchive {
                    path: format!("{mods}/{name}"),
                    mount: "gamedata".to_owned(),
                });
            }
        }
    }
    Ok(archives)
}

struct FileRef {
    archive: usize,
    entry: Entry,
}

/// The combined tree of mounted archives, reading each through `R`.
pub struct DbVfs<R> {
    archives: Vec<RefCell<R>>,
    files: HashMap<String, FileRef>,
    /// Every directory (`""` is the root) with the names of its children.
    dirs: HashMap<String, BTreeSet<String>>,
}

impl<R: Read + Seek> Default for DbVfs<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> DbVfs<Box<dyn VfsFile + 'a>> {
    /// Opens and mounts the archives [`game_archives`] finds in `game`.
    pub fn open_game(game: &'a dyn Vfs, mods_dir: Option<&str>) -> io::Result<Self> {
        let mut vfs = Self::new();
        for archive in game_archives(game, mods_dir)? {
            let file = game.open(&archive.path)?;
            // docs/formats/db-archive.md §2 doesn't say which path the `.xdb`
            // rule applies to; we use the path relative to the game root.
            let opened = Archive::open(file, Scrambling::for_path(&archive.path))
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", archive.path)))?;
            vfs.mount(&archive.mount, opened)?;
        }
        Ok(vfs)
    }
}

impl<R: Read + Seek> DbVfs<R> {
    pub fn new() -> Self {
        Self {
            archives: Vec::new(),
            files: HashMap::new(),
            dirs: HashMap::from([(String::new(), BTreeSet::new())]),
        }
    }

    /// Mounts `archive` at directory `mount` (`""` for the root). Its files
    /// replace any already mounted at the same paths.
    pub fn mount(&mut self, mount: &str, archive: Archive<R>) -> io::Result<()> {
        let mount = key(mount)?;
        let (reader, entries) = archive.into_parts();
        let index = self.archives.len();
        // Validate every path before changing the tree.
        let mut placed = Vec::with_capacity(entries.len());
        for entry in entries {
            let path = key(&format!("{mount}/{}", entry.path))?;
            if path.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("entry {:?} names the mount directory", entry.path),
                ));
            }
            placed.push((path, entry));
        }
        self.add_dir(&mount);
        for (path, entry) in placed {
            if entry.is_dir() {
                self.add_dir(&path);
            } else {
                self.add_child(&path);
                self.files.insert(
                    path,
                    FileRef {
                        archive: index,
                        entry,
                    },
                );
            }
        }
        self.archives.push(RefCell::new(reader));
        Ok(())
    }

    /// The mount-order index of the archive whose entry `path` is (the one
    /// that wins), for diagnostics; `None` when no file has that path.
    pub fn archive_of(&self, path: &str) -> Option<usize> {
        self.files.get(&key(path).ok()?).map(|f| f.archive)
    }

    /// Adds directory `path` and its ancestors.
    fn add_dir(&mut self, path: &str) {
        if !path.is_empty() && !self.dirs.contains_key(path) {
            self.dirs.insert(path.to_owned(), BTreeSet::new());
            self.add_child(path);
        }
    }

    /// Records `path` in its parent directory, creating the ancestors.
    fn add_child(&mut self, path: &str) {
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        self.add_dir(parent);
        self.dirs
            .get_mut(parent)
            .expect("parent added")
            .insert(name.to_owned());
    }
}

/// Lookup key: normalized and lowercased.
fn key(path: &str) -> io::Result<String> {
    Ok(normalize(path)?.to_ascii_lowercase())
}

fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("not found: {path:?}"))
}

impl<R: Read + Seek> Vfs for DbVfs<R> {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        let key = key(path)?;
        let Some(file) = self.files.get(&key) else {
            return Err(if self.dirs.contains_key(&key) {
                io::Error::new(io::ErrorKind::InvalidInput, format!("not a file: {path:?}"))
            } else {
                not_found(path)
            });
        };
        let reader = &self.archives[file.archive];
        let entry = &file.entry;
        if entry.is_compressed() {
            let data = entry.read(&mut *reader.borrow_mut())?;
            return Ok(Box::new(MemoryFile(Cursor::new(data))));
        }
        Ok(Box::new(StoredFile {
            reader,
            start: entry.offset.into(),
            size: entry.original_size.into(),
            pos: 0,
        }))
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        let key = key(path)?;
        let Some(children) = self.dirs.get(&key) else {
            return Err(if self.files.contains_key(&key) {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("not a directory: {path:?}"),
                )
            } else {
                not_found(path)
            });
        };
        Ok(children
            .iter()
            .map(|name| {
                let child = if key.is_empty() {
                    name.clone()
                } else {
                    format!("{key}/{name}")
                };
                // A name that is both a file and a directory (not covered by
                // the spec) is reported as a file.
                match self.files.get(&child) {
                    Some(file) => DirEntry {
                        name: name.clone(),
                        kind: EntryKind::File,
                        size: file.entry.original_size.into(),
                    },
                    None => DirEntry {
                        name: name.clone(),
                        kind: EntryKind::Dir,
                        size: 0,
                    },
                }
            })
            .collect())
    }
}

/// A verbatim entry, read straight from its archive.
struct StoredFile<'a, R> {
    reader: &'a RefCell<R>,
    start: u64,
    size: u64,
    pos: u64,
}

impl<R: Read + Seek> Read for StoredFile<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.size.saturating_sub(self.pos);
        let len = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        if len == 0 {
            return Ok(0);
        }
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(self.start + self.pos))?;
        let n = reader.read(&mut buf[..len])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R> Seek for StoredFile<'_, R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
            SeekFrom::End(delta) => self.size.checked_add_signed(delta),
        };
        self.pos = target.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "seek before start of file")
        })?;
        Ok(self.pos)
    }
}

impl<R: Read + Seek> VfsFile for StoredFile<'_, R> {
    fn size(&self) -> u64 {
        self.size
    }
}

/// A decompressed entry.
struct MemoryFile(Cursor<Vec<u8>>);

impl Read for MemoryFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for MemoryFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

impl VfsFile for MemoryFile {
    fn size(&self) -> u64 {
        self.0.get_ref().len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::super::conformance;
    use super::*;

    /// An archive with a plain directory chunk holding `files` (stored path,
    /// contents) and directory entries `dirs`.
    fn archive(dirs: &[&str], files: &[(&str, &[u8])]) -> Archive<Cursor<Vec<u8>>> {
        fn entry(table: &mut Vec<u8>, path: &str, size: usize, offset: usize) {
            table.extend(((16 + path.len()) as u16).to_le_bytes());
            for v in [size, size, 0] {
                table.extend((v as u32).to_le_bytes());
            }
            table.extend(path.as_bytes());
            table.extend((offset as u32).to_le_bytes());
        }
        let mut data: Vec<u8> = Vec::new();
        let mut table = Vec::new();
        for dir in dirs {
            entry(&mut table, dir, 0, 0);
        }
        for (path, contents) in files {
            entry(&mut table, path, contents.len(), 8 + data.len());
            data.extend(*contents);
        }
        let mut bytes = 0u32.to_le_bytes().to_vec();
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(data);
        bytes.extend(1u32.to_le_bytes());
        bytes.extend((table.len() as u32).to_le_bytes());
        bytes.extend(table);
        Archive::open(Cursor::new(bytes), Scrambling::Plain).unwrap()
    }

    #[test]
    fn conforms() {
        let big = conformance::big_contents();
        let mut files: Vec<(String, &[u8])> = conformance::FILES
            .iter()
            .map(|(p, c)| (p.replace('/', "\\"), *c))
            .collect();
        files.push((conformance::BIG_PATH.replace('/', "\\"), &big));
        let files: Vec<(&str, &[u8])> = files.iter().map(|(p, c)| (p.as_str(), *c)).collect();
        // Split across two archives, the second without directory entries.
        let mut vfs = DbVfs::new();
        vfs.mount("", archive(&["sub\\"], &files[..2])).unwrap();
        vfs.mount("", archive(&[], &files[2..])).unwrap();
        conformance::check(&vfs).unwrap();
    }

    #[test]
    fn later_archives_override_file_by_file() {
        let mut vfs = DbVfs::new();
        vfs.mount(
            "gamedata",
            archive(&[], &[("a\\x.ltx", b"old"), ("a\\y.ltx", b"kept")]),
        )
        .unwrap();
        vfs.mount("gamedata", archive(&["a\\"], &[("a\\x.ltx", b"new!")]))
            .unwrap();
        assert_eq!(
            super::super::read(&vfs, "gamedata/a/x.ltx").unwrap(),
            b"new!"
        );
        assert_eq!(
            super::super::read(&vfs, "GameData\\A\\Y.LTX").unwrap(),
            b"kept"
        );
        let names: Vec<_> = vfs
            .read_dir("gamedata/a")
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.size))
            .collect();
        assert_eq!(names, [("x.ltx".into(), 4), ("y.ltx".into(), 4)]);
        assert_eq!(vfs.read_dir("").unwrap()[0].name, "gamedata");
    }

    #[test]
    fn rejects_escaping_paths() {
        let mut vfs = DbVfs::new();
        assert!(vfs.mount("", archive(&[], &[("..\\x", b"")])).is_err());
        assert!(vfs.read_dir("").unwrap().is_empty());
    }

    #[test]
    fn finds_game_archives_in_mount_order() {
        struct Names(Vec<&'static str>);
        impl Vfs for Names {
            fn open(&self, _: &str) -> io::Result<Box<dyn VfsFile + '_>> {
                unimplemented!()
            }
            fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
                let prefix = if path.is_empty() {
                    String::new()
                } else {
                    format!("{path}/")
                };
                Ok(self
                    .0
                    .iter()
                    .filter_map(|p| p.strip_prefix(&prefix))
                    .filter(|p| !p.contains('/'))
                    .map(|name| DirEntry {
                        name: name.into(),
                        kind: EntryKind::File,
                        size: 0,
                    })
                    .collect())
            }
        }
        let game = Names(vec![
            "gamedata.dba",
            "gamedata.db1",
            "goggame-1207660573.hashdb",
            "gamedata.db0",
            "mods/mp_military_2.xdb0",
            "mods/readme.txt",
        ]);
        let found: Vec<_> = game_archives(&game, Some("mods"))
            .unwrap()
            .into_iter()
            .map(|a| (a.path, a.mount))
            .collect();
        let expected = [
            ("gamedata.db0", "gamedata"),
            ("gamedata.db1", "gamedata"),
            ("gamedata.dba", "gamedata"),
            ("mods/mp_military_2.xdb0", "gamedata"),
        ];
        assert_eq!(found, expected.map(|(p, m)| (p.to_owned(), m.to_owned())));
        assert_eq!(game_archives(&game, None).unwrap().len(), 3);
    }
}
