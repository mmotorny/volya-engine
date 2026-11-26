// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! [`Vfs`] over the game's `.db` / `.xdb` archives combined into one tree
//! (docs/formats/db-archive.md, section 7).
//!
//! Each archive is mounted at a directory; later mounts replace earlier
//! entries file by file, and nothing is ever removed. Paths are compared
//! case-insensitively (ASCII), so lookups and listed names are lowercase.

use std::cell::RefCell;
use std::collections::BTreeMap;
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

/// No file at a node.
const NO_FILE: u32 = u32::MAX;
/// A node that is no directory.
const NOT_DIR: u32 = u32::MAX;

/// A name of the tree: a file, a directory or both (a name that is both is
/// not covered by the spec and is reported as a file).
#[derive(Clone, Copy)]
struct Node {
    /// The name's start in [`DbVfs::names`].
    name: u32,
    name_len: u32,
    /// The archive holding the file, [`NO_FILE`] when the node is no file.
    archive: u32,
    offset: u32,
    original_size: u32,
    stored_size: u32,
    /// The children: `children` nodes from `first_child`, sorted by name;
    /// `children` is [`NOT_DIR`] when the node is no directory.
    first_child: u32,
    children: u32,
}

impl Node {
    const ROOT: Self = Self {
        name: 0,
        name_len: 0,
        archive: NO_FILE,
        offset: 0,
        original_size: 0,
        stored_size: 0,
        first_child: 1,
        children: 0,
    };

    fn is_file(&self) -> bool {
        self.archive != NO_FILE
    }

    fn is_dir(&self) -> bool {
        self.children != NOT_DIR
    }
}

/// The combined tree of mounted archives, reading each through `R`.
///
/// The tree is a region (docs/architecture/memory.md): every name stored
/// once in one buffer and every node in one table, both sized once from the
/// archives. Node 0 is the root; each directory's children are a run of the
/// table sorted by name, so a path is found a component at a time.
pub struct DbVfs<R> {
    archives: Vec<RefCell<R>>,
    /// Every node's name, lowercase.
    names: String,
    nodes: Vec<Node>,
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
        // Each archive's table is read and added in turn; the tree is built
        // once from them all.
        let mut tree = BTreeMap::new();
        let mut readers = Vec::new();
        for archive in game_archives(game, mods_dir)? {
            let file = game.open(&archive.path)?;
            // docs/formats/db-archive.md §2 doesn't say which path the `.xdb`
            // rule applies to; we use the path relative to the game root.
            let opened = Archive::open(file, Scrambling::for_path(&archive.path))
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", archive.path)))?;
            let index = index(readers.len());
            readers.push(add_archive(&mut tree, index, &archive.mount, opened)?);
        }
        vfs.archives = readers.into_iter().map(RefCell::new).collect();
        vfs.build(tree);
        Ok(vfs)
    }
}

/// A node while the tree is built: whether it is a directory, and its file
/// (a [`Node`] of which only the file's fields count).
#[derive(Default)]
struct Pending {
    dir: bool,
    file: Option<Node>,
}

/// Marks `path` and its ancestors (not the root) directories.
fn add_dir(tree: &mut BTreeMap<String, Pending>, path: &str) {
    let mut end = 0;
    for part in path.split('/') {
        end += part.len();
        if end > 0 {
            match tree.get_mut(&path[..end]) {
                Some(pending) => pending.dir = true,
                None => {
                    let dir = Pending {
                        dir: true,
                        file: None,
                    };
                    tree.insert(path[..end].to_owned(), dir);
                }
            }
        }
        end += 1;
    }
}

/// Adds `archive`'s entries to `tree`, mounted at `mount`, as archive
/// `index`; returns its reader.
fn add_archive<R: Read + Seek>(
    tree: &mut BTreeMap<String, Pending>,
    index: u32,
    mount: &str,
    archive: Archive<R>,
) -> io::Result<R> {
    let mount = key(mount)?;
    let (reader, entries) = archive.into_parts();
    add_dir(tree, &mount);
    for entry in entries {
        let path = key(&format!("{mount}/{}", entry.path))?;
        if path.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("entry {:?} names the mount directory", entry.path),
            ));
        }
        if entry.is_dir() {
            add_dir(tree, &path);
            continue;
        }
        if let Some((parent, _)) = path.rsplit_once('/') {
            add_dir(tree, parent);
        }
        tree.entry(path).or_default().file = Some(Node {
            archive: index,
            offset: entry.offset,
            original_size: entry.original_size,
            stored_size: entry.stored_size,
            ..Node::ROOT
        });
    }
    Ok(reader)
}

fn index(i: usize) -> u32 {
    u32::try_from(i).expect("the archive index fits u32")
}

impl<R: Read + Seek> DbVfs<R> {
    pub fn new() -> Self {
        Self {
            archives: Vec::new(),
            names: String::new(),
            nodes: vec![Node::ROOT],
        }
    }

    /// Mounts `archive` at directory `mount` (`""` for the root). Its files
    /// replace any already mounted at the same paths. The tree is unchanged
    /// when an entry's path is rejected.
    pub fn mount(&mut self, mount: &str, archive: Archive<R>) -> io::Result<()> {
        // Every path of the tree as it is, then the archive's.
        let mut tree = BTreeMap::new();
        for (path, node) in self.paths().into_iter().zip(&self.nodes).skip(1) {
            let pending = Pending {
                dir: node.is_dir(),
                file: node.is_file().then_some(*node),
            };
            tree.insert(path, pending);
        }
        let reader = add_archive(&mut tree, index(self.archives.len()), mount, archive)?;
        self.archives.push(RefCell::new(reader));
        self.build(tree);
        Ok(())
    }

    /// Lays out `tree` (every path but the root's) as the tree, breadth
    /// first: each directory's children in a run.
    fn build(&mut self, tree: BTreeMap<String, Pending>) {
        let sorted: Vec<(String, Pending)> = tree.into_iter().collect();
        // Node ids: 0 the root, 1 + i the path `sorted[i]`.
        let count = sorted.len() + 1;
        let parents: Vec<u32> = sorted
            .iter()
            .map(|(path, _)| match path.rsplit_once('/') {
                None => 0,
                Some((up, _)) => {
                    let i = sorted
                        .binary_search_by(|(p, _)| p.as_str().cmp(up))
                        .expect("ancestors added");
                    index(i + 1)
                }
            })
            .collect();
        // Each id's children, in path order, which is their names' order:
        // `kids[start[id]..start[id + 1]]`.
        let mut start = vec![0u32; count + 1];
        for &p in &parents {
            start[p as usize + 1] += 1;
        }
        for i in 0..count {
            start[i + 1] += start[i];
        }
        let mut kids = vec![0u32; sorted.len()];
        let mut fill = start.clone();
        for (i, &p) in parents.iter().enumerate() {
            kids[fill[p as usize] as usize] = index(i + 1);
            fill[p as usize] += 1;
        }
        drop((parents, fill));
        // Breadth first: the root, then each node's children in a run.
        let mut order = Vec::with_capacity(count);
        order.push(0u32);
        let mut first = vec![0u32; count];
        let mut k = 0;
        while k < order.len() {
            let id = order[k] as usize;
            first[id] = index(order.len());
            order.extend_from_slice(&kids[start[id] as usize..start[id + 1] as usize]);
            k += 1;
        }
        let name_of = |id: u32| -> &str {
            if id == 0 {
                return "";
            }
            let path = &sorted[id as usize - 1].0;
            path.rsplit_once('/').map_or(path.as_str(), |(_, n)| n)
        };
        let mut names = String::with_capacity(order.iter().map(|&id| name_of(id).len()).sum());
        let mut nodes = Vec::with_capacity(count);
        for &id in &order {
            let name = name_of(id);
            let children = start[id as usize + 1] - start[id as usize];
            let mut node = Node {
                name: index(names.len()),
                name_len: index(name.len()),
                first_child: first[id as usize],
                children,
                ..Node::ROOT
            };
            if id != 0 {
                let pending = &sorted[id as usize - 1].1;
                if !pending.dir && children == 0 {
                    node.children = NOT_DIR;
                }
                if let Some(file) = &pending.file {
                    node.archive = file.archive;
                    node.offset = file.offset;
                    node.original_size = file.original_size;
                    node.stored_size = file.stored_size;
                }
            }
            nodes.push(node);
            names.push_str(name);
        }
        self.names = names;
        self.nodes = nodes;
    }

    /// Every node's path, by node (the root's is `""`).
    fn paths(&self) -> Vec<String> {
        let mut paths = vec![String::new(); self.nodes.len()];
        for (i, node) in self.nodes.iter().enumerate() {
            let first = node.first_child as usize;
            for kid in first..first + self.children(node).len() {
                let name = self.name(&self.nodes[kid]);
                paths[kid] = if i == 0 {
                    name.to_owned()
                } else {
                    format!("{}/{name}", paths[i])
                };
            }
        }
        paths
    }

    fn name(&self, node: &Node) -> &str {
        &self.names[node.name as usize..(node.name + node.name_len) as usize]
    }

    fn children(&self, node: &Node) -> &[Node] {
        if !node.is_dir() {
            return &[];
        }
        let first = node.first_child as usize;
        &self.nodes[first..first + node.children as usize]
    }

    /// The node at `key`, a [`key`] result.
    fn find(&self, key: &str) -> Option<&Node> {
        let mut node = &self.nodes[0];
        if key.is_empty() {
            return Some(node);
        }
        for part in key.split('/') {
            let kids = self.children(node);
            node = &kids[kids.binary_search_by(|k| self.name(k).cmp(part)).ok()?];
        }
        Some(node)
    }

    /// The mount-order index of the archive whose entry `path` is (the one
    /// that wins), for diagnostics; `None` when no file has that path.
    pub fn archive_of(&self, path: &str) -> Option<usize> {
        let node = self.find(&key(path).ok()?)?;
        node.is_file().then_some(node.archive as usize)
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
        let file = match self.find(&key) {
            Some(node) if node.is_file() => *node,
            Some(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("not a file: {path:?}"),
                ));
            }
            None => return Err(not_found(path)),
        };
        let reader = &self.archives[file.archive as usize];
        if file.stored_size != file.original_size {
            let entry = Entry {
                path: key,
                original_size: file.original_size,
                stored_size: file.stored_size,
                checksum: 0,
                offset: file.offset,
            };
            let data = entry.read(&mut *reader.borrow_mut())?;
            return Ok(Box::new(MemoryFile(Cursor::new(data))));
        }
        Ok(Box::new(StoredFile {
            reader,
            start: file.offset.into(),
            size: file.original_size.into(),
            pos: 0,
        }))
    }

    fn exists(&self, path: &str) -> bool {
        key(path).is_ok_and(|k| self.find(&k).is_some_and(Node::is_file))
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        let key = key(path)?;
        let dir = match self.find(&key) {
            Some(node) if node.is_dir() => node,
            Some(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("not a directory: {path:?}"),
                ));
            }
            None => return Err(not_found(path)),
        };
        Ok(self
            .children(dir)
            .iter()
            .map(|node| DirEntry {
                name: self.name(node).to_owned(),
                kind: if node.is_file() {
                    EntryKind::File
                } else {
                    EntryKind::Dir
                },
                size: if node.is_file() {
                    node.original_size.into()
                } else {
                    0
                },
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
        assert!(vfs.exists("GameData\\A\\X.ltx"));
        assert!(!vfs.exists("gamedata/a"), "a directory");
        assert!(!vfs.exists("gamedata/a/z.ltx"));
    }

    #[test]
    fn a_name_both_file_and_directory_is_listed_as_a_file() {
        let mut vfs = DbVfs::new();
        let files: [(&str, &[u8]); 3] = [("a\\x", b"xx"), ("a-b", b""), ("a", b"f")];
        vfs.mount("", archive(&["e\\"], &files)).unwrap();
        let listed: Vec<_> = vfs
            .read_dir("")
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.kind, e.size))
            .collect();
        let expected = [("a", EntryKind::File, 1), ("a-b", EntryKind::File, 0)];
        let mut expected: Vec<_> = expected.map(|(n, k, s)| (n.to_owned(), k, s)).into();
        expected.push(("e".to_owned(), EntryKind::Dir, 0));
        assert_eq!(listed, expected);
        assert_eq!(super::super::read(&vfs, "a").unwrap(), b"f");
        assert_eq!(super::super::read(&vfs, "A/X").unwrap(), b"xx");
        assert_eq!(vfs.read_dir("a").unwrap()[0].name, "x");
        assert!(vfs.read_dir("e").unwrap().is_empty());
        assert!(vfs.open("e").is_err() && vfs.read_dir("a-b").is_err());
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
