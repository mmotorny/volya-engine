// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A file read ahead in a bounded window, for decoders that keep their
//! reader between calls (videos play over many frames) while the VFS is
//! only lent for each call. The decoder reads the window through one clone;
//! its owner tops the window up from the VFS before each decoding call. So
//! memory stays at about [`READ_AHEAD`] per file instead of the whole file.

use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};
use std::rc::Rc;

use super::Vfs;

/// Bytes kept ahead of the read position after a top-up.
pub const READ_AHEAD: usize = 1 << 20;

#[derive(Debug)]
struct Window {
    path: String,
    size: u64,
    /// File offset of `data[0]`.
    start: u64,
    data: Vec<u8>,
    pos: u64,
}

/// See the module docs. Clones share the window and the read position.
#[derive(Clone, Debug)]
pub struct ReadAhead(Rc<RefCell<Window>>);

impl ReadAhead {
    /// Opens the file at `path` of `vfs`, filling the first window.
    pub fn open(vfs: &dyn Vfs, path: &str) -> io::Result<Self> {
        let size = vfs.open(path)?.size();
        let this = Self(Rc::new(RefCell::new(Window {
            path: path.to_owned(),
            size,
            start: 0,
            data: Vec::new(),
            pos: 0,
        })));
        this.top_up(vfs)?;
        Ok(this)
    }

    /// The file's size in bytes.
    pub fn size(&self) -> u64 {
        self.0.borrow().size
    }

    /// Bytes held in memory.
    pub fn held(&self) -> usize {
        self.0.borrow().data.len()
    }

    /// Makes at least half of [`READ_AHEAD`] (or the rest of the file)
    /// available from the read position, reading from `vfs` only when less
    /// is left; bytes before the position are dropped then.
    pub fn top_up(&self, vfs: &dyn Vfs) -> io::Result<()> {
        let mut w = self.0.borrow_mut();
        let held_end = w.start + w.data.len() as u64;
        if w.pos < w.start || w.pos > held_end {
            // Moved outside the window (a rewind): start afresh there.
            w.start = w.pos;
            w.data.clear();
        }
        let held_end = w.start + w.data.len() as u64;
        let want_end = (w.pos + READ_AHEAD as u64).min(w.size);
        let ahead = held_end - w.pos;
        if held_end >= w.size || ahead >= READ_AHEAD as u64 / 2 {
            return Ok(());
        }
        let consumed = (w.pos - w.start) as usize;
        w.data.drain(..consumed);
        w.start = w.pos;
        let mut file = vfs.open(&w.path)?;
        file.seek(SeekFrom::Start(held_end))?;
        let len = w.data.len();
        w.data.resize(len + (want_end - held_end) as usize, 0);
        file.read_exact(&mut w.data[len..])?;
        Ok(())
    }
}

impl Read for ReadAhead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut w = self.0.borrow_mut();
        if w.pos >= w.size {
            return Ok(0);
        }
        let held_end = w.start + w.data.len() as u64;
        if w.pos < w.start || w.pos >= held_end {
            return Err(io::Error::other(format!(
                "{}: read at {} outside the read-ahead window",
                w.path, w.pos
            )));
        }
        let from = (w.pos - w.start) as usize;
        let n = buf.len().min(w.data.len() - from);
        buf[..n].copy_from_slice(&w.data[from..from + n]);
        w.pos += n as u64;
        Ok(n)
    }
}

impl Seek for ReadAhead {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let mut w = self.0.borrow_mut();
        let (base, offset) = match pos {
            SeekFrom::Start(n) => (0, n as i64),
            SeekFrom::Current(n) => (w.pos, n),
            SeekFrom::End(n) => (w.size, n),
        };
        w.pos = base
            .checked_add_signed(offset)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?;
        Ok(w.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::{DirEntry, VfsFile};
    use std::io::Cursor;

    /// One file of `n` bytes (byte i = i mod 251), counting opens.
    struct One(Vec<u8>, std::cell::Cell<u32>);

    struct File(Cursor<Vec<u8>>);
    impl Read for File {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }
    impl Seek for File {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.0.seek(pos)
        }
    }
    impl VfsFile for File {
        fn size(&self) -> u64 {
            self.0.get_ref().len() as u64
        }
    }
    impl Vfs for One {
        fn open(&self, _: &str) -> io::Result<Box<dyn VfsFile + '_>> {
            self.1.set(self.1.get() + 1);
            Ok(Box::new(File(Cursor::new(self.0.clone()))))
        }
        fn read_dir(&self, _: &str) -> io::Result<Vec<DirEntry>> {
            Ok(Vec::new())
        }
    }

    fn one(n: usize) -> One {
        One(
            (0..n).map(|i| (i % 251) as u8).collect(),
            Default::default(),
        )
    }

    #[test]
    fn reads_the_whole_file_holding_a_bounded_window() {
        let vfs = one(3 * READ_AHEAD + 123);
        let mut r = ReadAhead::open(&vfs, "f").unwrap();
        let mut reader = r.clone();
        let mut all = Vec::new();
        let mut buf = vec![0; 70_000];
        loop {
            r.top_up(&vfs).unwrap();
            assert!(r.held() <= READ_AHEAD, "{}", r.held());
            let n = reader.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            all.extend_from_slice(&buf[..n]);
        }
        assert_eq!(all, vfs.0);
        // A few top-ups, not one read per call.
        assert!(vfs.1.get() < 10, "{} opens", vfs.1.get());
        // Rewinding refills from the start.
        r.seek(SeekFrom::Start(0)).unwrap();
        r.top_up(&vfs).unwrap();
        let mut head = [0; 4];
        r.read_exact(&mut head).unwrap();
        assert_eq!(head, [0, 1, 2, 3]);
    }

    #[test]
    fn reading_past_the_window_fails_instead_of_blocking() {
        let vfs = one(2 * READ_AHEAD);
        let mut r = ReadAhead::open(&vfs, "f").unwrap();
        r.seek(SeekFrom::Start(READ_AHEAD as u64 + 1)).unwrap();
        assert!(r.read(&mut [0; 4]).is_err());
    }
}
