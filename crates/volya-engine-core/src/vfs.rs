// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Virtual filesystem: synchronous, read-only `Read + Seek` access to a tree
//! of files, independent of where they live (`std::fs` natively, OPFS in the
//! browser). See docs/architecture.md (Threading: I/O is synchronous).
//!
//! Paths are relative, `/`-separated and case-sensitive; `\` is accepted as a
//! separator, `.` and empty segments are ignored, and `..` is rejected.

pub mod db;
pub mod read_ahead;

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

/// A read-only file tree.
pub trait Vfs {
    /// Opens the file at `path` for reading.
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>>;

    /// Lists the directory at `path` (`""` is the root), sorted by name.
    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>>;

    /// Whether a file exists at `path`. Backends that can answer without
    /// opening (and, for compressed archive entries, decompressing) the
    /// file override this.
    fn exists(&self, path: &str) -> bool {
        self.open(path).is_ok()
    }
}

/// An open file.
pub trait VfsFile: Read + Seek {
    /// File size in bytes.
    fn size(&self) -> u64;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
    /// File size in bytes; 0 for directories.
    pub size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

/// Normalizes `path` to `a/b/c` form; see the module docs for the rules.
pub fn normalize(path: &str) -> io::Result<String> {
    let mut parts = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("`..` in VFS path {path:?}"),
                ));
            }
            _ => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

/// Reads the whole file at `path`.
pub fn read(vfs: &dyn Vfs, path: &str) -> io::Result<Vec<u8>> {
    let mut file = vfs.open(path)?;
    let mut data = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut data)?;
    Ok(data)
}

/// Reads the whole file at `path` into `buffer`, replacing its contents.
/// Reading many files through one buffer keeps their bytes in one block:
/// a buffer per file is a large allocation each time, which a fragmented
/// heap may only satisfy with new memory (the wasm heap never shrinks,
/// #357). A buffer too small is freed before its replacement is made.
pub fn read_into(vfs: &dyn Vfs, path: &str, buffer: &mut Vec<u8>) -> io::Result<()> {
    let mut file = vfs.open(path)?;
    let size = file.size() as usize;
    buffer.clear();
    if buffer.capacity() < size {
        *buffer = Vec::new();
        buffer.reserve_exact(size);
    }
    file.read_to_end(buffer)?;
    Ok(())
}

/// [`Vfs`] over a directory of the native filesystem.
pub struct StdVfs {
    root: PathBuf,
}

impl StdVfs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, path: &str) -> io::Result<PathBuf> {
        Ok(self.root.join(normalize(path)?))
    }
}

impl Vfs for StdVfs {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        let file = fs::File::open(self.resolve(path)?)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("not a file: {path:?}"),
            ));
        }
        Ok(Box::new(StdFile {
            file,
            size: meta.len(),
        }))
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.resolve(path)?)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            let Ok(name) = entry.file_name().into_string() else {
                continue; // Not representable as a VFS path.
            };
            entries.push(if meta.is_dir() {
                DirEntry {
                    name,
                    kind: EntryKind::Dir,
                    size: 0,
                }
            } else {
                DirEntry {
                    name,
                    kind: EntryKind::File,
                    size: meta.len(),
                }
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }
}

struct StdFile {
    file: fs::File,
    size: u64,
}

impl Read for StdFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for StdFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}

impl VfsFile for StdFile {
    fn size(&self) -> u64 {
        self.size
    }
}

/// Shared conformance check run against every [`Vfs`] backend: write
/// [`conformance::FILES`] with the backend's native means, then call
/// [`conformance::check`].
pub mod conformance {
    use super::*;

    /// Fixture tree: `(path, contents)`.
    pub const FILES: &[(&str, &[u8])] = &[
        ("a.txt", b"hello"),
        ("empty", b""),
        ("sub/deeper/c.txt", b"deep"),
    ];

    /// A larger file for seek tests: bytes `0..=255` repeated 64 times.
    pub const BIG_PATH: &str = "sub/big.bin";

    pub fn big_contents() -> Vec<u8> {
        (0..=255u8).cycle().take(256 * 64).collect()
    }

    /// Checks `vfs`, which must contain exactly the fixture tree.
    pub fn check(vfs: &dyn Vfs) -> Result<(), String> {
        let e = |what: &str, err: io::Error| format!("{what}: {err}");

        for (path, contents) in FILES {
            let data = read(vfs, path).map_err(|err| e(path, err))?;
            ensure(data == *contents, format!("{path}: wrong contents"))?;
        }
        let alt = read(vfs, "./sub\\deeper//c.txt").map_err(|err| e("unnormalized path", err))?;
        ensure(alt == b"deep", "unnormalized path: wrong contents")?;

        let big = big_contents();
        let mut file = vfs.open(BIG_PATH).map_err(|err| e(BIG_PATH, err))?;
        ensure(file.size() == big.len() as u64, "big: size")?;
        let mut buf = [0u8; 10];
        file.seek(SeekFrom::Start(1000))
            .and_then(|_| file.read_exact(&mut buf))
            .map_err(|err| e("big: seek start", err))?;
        ensure(buf[..] == big[1000..1010], "big: data after seek start")?;
        file.seek(SeekFrom::Current(-5))
            .and_then(|_| file.read_exact(&mut buf[..1]))
            .map_err(|err| e("big: seek current", err))?;
        ensure(buf[0] == big[1005], "big: data after seek current")?;
        let pos = file
            .seek(SeekFrom::End(-1))
            .map_err(|err| e("big: seek end", err))?;
        ensure(pos == big.len() as u64 - 1, "big: position after seek end")?;
        let n = file
            .read(&mut buf)
            .map_err(|err| e("big: read at end", err))?;
        ensure(n == 1 && buf[0] == big[big.len() - 1], "big: read at end")?;
        let n = file
            .read(&mut buf)
            .map_err(|err| e("big: read past end", err))?;
        ensure(n == 0, "big: read past end returns 0")?;

        let names = |path: &str| -> Result<Vec<(String, EntryKind, u64)>, String> {
            Ok(vfs
                .read_dir(path)
                .map_err(|err| e(&format!("read_dir {path:?}"), err))?
                .into_iter()
                .map(|d| (d.name, d.kind, d.size))
                .collect())
        };
        use EntryKind::{Dir, File};
        ensure(
            names("")?
                == [
                    ("a.txt".into(), File, 5),
                    ("empty".into(), File, 0),
                    ("sub".into(), Dir, 0),
                ],
            format!("read_dir root: {:?}", names("")?),
        )?;
        ensure(
            names("sub")?
                == [
                    ("big.bin".into(), File, big.len() as u64),
                    ("deeper".into(), Dir, 0),
                ],
            format!("read_dir sub: {:?}", names("sub")?),
        )?;

        ensure(vfs.open("missing").is_err(), "missing file opens")?;
        ensure(vfs.open("sub").is_err(), "directory opens as a file")?;
        ensure(vfs.open("../a.txt").is_err(), "`..` accepted")?;
        ensure(vfs.read_dir("a.txt").is_err(), "file lists as a directory")?;
        Ok(())
    }

    fn ensure(condition: bool, message: impl Into<String>) -> Result<(), String> {
        if condition {
            Ok(())
        } else {
            Err(message.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize("a/b").unwrap(), "a/b");
        assert_eq!(normalize("/a//./b/").unwrap(), "a/b");
        assert_eq!(normalize("a\\b").unwrap(), "a/b");
        assert_eq!(normalize("").unwrap(), "");
        assert!(normalize("a/../b").is_err());
    }

    #[test]
    fn std_vfs_conforms() {
        let root = std::env::temp_dir().join(format!("volya-vfs-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let files = conformance::FILES
            .iter()
            .map(|(p, c)| (*p, c.to_vec()))
            .chain([(conformance::BIG_PATH, conformance::big_contents())]);
        for (path, contents) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let result = conformance::check(&StdVfs::new(&root));
        fs::remove_dir_all(&root).unwrap();
        result.unwrap();
    }
}
