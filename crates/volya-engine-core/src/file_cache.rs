// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The file cache (docs/architecture/memory.md, File-backed data): what
//! the original reads in place from its mapped archives is held here, read
//! in place, as wasm can't map a file. A file is held whole, in bytes taken
//! from `System` directly ([`SystemBytes`]), so they are not in *h*
//! (`* phase cmem`): the cache has a figure and a budget of its own. A file
//! read again while held is shared, not read twice; it goes when its last
//! [`CachedFile`] does.
//!
//! The figures: the bytes held now and by file ([`held`]), the most since
//! the load began ([`begin_load`], [`most`]), and per window between two
//! [`mark`]s (a loading caption) the most held and every file held in it,
//! against the budget [`budget`] makes of them.

use std::io::{self, Read};
use std::ops::Deref;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use crate::memory_manager::SystemBytes;
use crate::vfs::Vfs;

/// A file held in the cache, read in place.
#[derive(Clone)]
pub struct CachedFile(Arc<Held>);

struct Held {
    path: String,
    bytes: SystemBytes,
}

impl Deref for CachedFile {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0.bytes
    }
}

impl AsRef<[u8]> for CachedFile {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let mut state = state();
        state.held.retain(|(_, h)| h.strong_count() > 0);
        state.bytes -= self.bytes.len() as u64;
    }
}

/// The cache's figures over a window (a loading caption), bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// The most held at once.
    pub most: u64,
    /// Every file held in it, once each, with its size.
    pub files: Vec<(String, u64)>,
}

#[derive(Default)]
struct State {
    held: Vec<(String, Weak<Held>)>,
    bytes: u64,
    most: u64,
    window: Window,
}

static STATE: Mutex<State> = Mutex::new(State {
    held: Vec::new(),
    bytes: 0,
    most: 0,
    window: Window {
        most: 0,
        files: Vec::new(),
    },
});

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

impl State {
    fn seen(&mut self, path: &str, size: u64) {
        if !self.window.files.iter().any(|(p, _)| p == path) {
            self.window.files.push((path.to_owned(), size));
        }
    }
}

/// The file at `path` of `tree`, held whole: shared if held already, else
/// read.
pub fn read(tree: &dyn Vfs, path: &str) -> io::Result<CachedFile> {
    if let Some(held) = {
        let state = state();
        state
            .held
            .iter()
            .find(|(p, _)| p == path)
            .and_then(|(_, h)| h.upgrade())
    } {
        return Ok(CachedFile(held));
    }
    let mut file = tree.open(path)?;
    let size = usize::try_from(file.size())
        .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "file too large"))?;
    let mut bytes = SystemBytes::zeroed(size)
        .ok_or_else(|| io::Error::new(io::ErrorKind::OutOfMemory, "no memory for the file"))?;
    file.read_exact(&mut bytes)?;
    let held = Arc::new(Held {
        path: path.to_owned(),
        bytes,
    });
    let mut state = state();
    state.held.push((path.to_owned(), Arc::downgrade(&held)));
    state.bytes += size as u64;
    state.most = state.most.max(state.bytes);
    state.window.most = state.window.most.max(state.bytes);
    state.seen(path, size as u64);
    Ok(CachedFile(held))
}

/// The bytes held now.
pub fn bytes() -> u64 {
    state().bytes
}

/// The files held now, with their sizes.
pub fn held() -> Vec<(String, u64)> {
    // Taken out of the lock first: the last of one dropped would take it.
    let held: Vec<Arc<Held>> = state()
        .held
        .iter()
        .filter_map(|(_, h)| h.upgrade())
        .collect();
    held.iter()
        .map(|h| (h.path.clone(), h.bytes.len() as u64))
        .collect()
}

/// The most held since the load began.
pub fn most() -> u64 {
    state().most
}

/// A load begins: its most starts from what is held now, as does the
/// first window.
pub fn begin_load() {
    let held = held();
    let mut state = state();
    state.most = state.bytes;
    state.window = Window::default();
    open_window(&mut state, held);
}

fn open_window(state: &mut State, held: Vec<(String, u64)>) {
    state.window.most = state.bytes;
    for (path, size) in held {
        state.seen(&path, size);
    }
}

/// Ends the window open (a caption's) and opens the next, which begins
/// with what is held now: the ended one's figures.
pub fn mark() -> Window {
    let held = held();
    let mut state = state();
    let ended = std::mem::take(&mut state.window);
    open_window(&mut state, held);
    ended
}

/// The size of the file at `path` of `tree` from its directory's listing
/// (opening a compressed archive entry would unpack it), `None` where
/// there is none.
pub fn file_size(tree: &dyn Vfs, path: &str) -> Option<u64> {
    let path = crate::vfs::normalize(path).ok()?;
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", &path));
    tree.read_dir(dir)
        .ok()?
        .into_iter()
        .find(|e| e.name == name && e.kind == crate::vfs::EntryKind::File)
        .map(|e| e.size)
}

/// The budget of a window in which the original has the files `open` (the
/// mapped files open then, with their sizes; docs/architecture/memory.md,
/// File-backed data): their bytes, and beside them the largest other file
/// held in it (the file being read; one at a time).
pub fn budget(window: &Window, open: &[(String, u64)]) -> u64 {
    let mapped: u64 = open.iter().map(|(_, size)| size).sum();
    let read = window
        .files
        .iter()
        .filter(|(p, _)| !open.iter().any(|(o, _)| o == p))
        .map(|(_, size)| *size)
        .max()
        .unwrap_or(0);
    mapped + read
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::conformance::{BIG_PATH, big_contents};
    use crate::vfs::{DirEntry, VfsFile};
    use std::io::Cursor;

    struct Files;

    struct File(Cursor<Vec<u8>>);

    impl Read for File {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl io::Seek for File {
        fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
            self.0.seek(pos)
        }
    }

    impl VfsFile for File {
        fn size(&self) -> u64 {
            self.0.get_ref().len() as u64
        }
    }

    impl Vfs for Files {
        fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
            match path {
                BIG_PATH => Ok(Box::new(File(Cursor::new(big_contents())))),
                "small" => Ok(Box::new(File(Cursor::new(vec![1; 100])))),
                _ => Err(io::ErrorKind::NotFound.into()),
            }
        }

        fn read_dir(&self, _: &str) -> io::Result<Vec<DirEntry>> {
            Ok(Vec::new())
        }
    }

    // One test: the cache is the program's, so its figures are only
    // checked where nothing else uses it meanwhile.
    #[test]
    fn files_are_held_whole_shared_and_counted_until_dropped() {
        begin_load();
        let big = read(&Files, BIG_PATH).unwrap();
        assert_eq!(&big[..], &big_contents()[..]);
        let again = read(&Files, BIG_PATH).unwrap();
        assert_eq!(bytes(), 16_384, "shared, not read twice");
        drop(again);
        assert_eq!(held(), [(BIG_PATH.to_owned(), 16_384)]);
        let first = mark();
        assert_eq!(first.most, 16_384);
        drop(big);
        assert_eq!(bytes(), 0);
        let small = read(&Files, "small").unwrap();
        drop(small);
        assert!(read(&Files, "missing").is_err());
        let second = mark();
        // The window began with the big file held.
        assert_eq!(second.most, 16_384);
        assert_eq!(second.files.len(), 2);
        // The original has the big file open: its bytes, and the small one
        // read beside it.
        let open = [(BIG_PATH.to_owned(), 16_384)];
        assert_eq!(budget(&second, &open), 16_484);
        assert_eq!(budget(&second, &[]), 16_384);
        assert_eq!(most(), 16_384);
        assert_eq!(mark(), Window::default());
    }
}
