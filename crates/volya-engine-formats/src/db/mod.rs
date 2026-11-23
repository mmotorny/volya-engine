// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `.db` / `.xdb` game-data archives (docs/formats/db-archive.md).
//!
//! An archive is a sequence of chunks; the directory chunk holds the file
//! table, scrambled (except in `.xdb` archives) and compressed. File data is
//! stored verbatim or as LZO1X. Combining several archives into one tree is
//! the VFS's job (`volya-engine-core`).

mod lzhuf;
mod scramble;

use std::io::{self, Read, Seek, SeekFrom};

pub use lzhuf::decode as decompress_directory;
pub use scramble::descramble;

/// Chunk type of the directory chunk.
const DIRECTORY_CHUNK: u32 = 1;
/// Compressed flag in a chunk id; the other bits are the chunk type.
const COMPRESSED: u32 = 0x8000_0000;
/// Directory entry body: three 32-bit fields, path, 32-bit offset.
const ENTRY_FIXED: usize = 16;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Whether an archive's directory is scrambled, which depends on the
/// archive's file path (section 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scrambling {
    Scrambled,
    Plain,
}

impl Scrambling {
    /// Scrambling of the archive at `path`: plain if the path contains
    /// `.xdb`, compared case-insensitively like all game paths (section 7).
    pub fn for_path(path: &str) -> Self {
        if path.to_ascii_lowercase().contains(".xdb") {
            Self::Plain
        } else {
            Self::Scrambled
        }
    }
}

/// One file table entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Path relative to the archive's mount directory, `\`-separated, as
    /// stored. A trailing `\` names a directory. Each stored byte becomes the
    /// char with the same code (U+0000–U+00FF); this edition's paths are
    /// ASCII.
    pub path: String,
    /// Length of the file's contents.
    pub original_size: u32,
    /// Length of the file's bytes in the archive.
    pub stored_size: u32,
    /// CRC-32 of the stored bytes; not needed for reading.
    pub checksum: u32,
    /// Absolute archive offset of the stored bytes.
    pub offset: u32,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.path.ends_with('\\')
    }

    /// Whether the stored bytes are LZO1X rather than the contents verbatim.
    pub fn is_compressed(&self) -> bool {
        self.stored_size != self.original_size
    }

    /// Reads the stored bytes from `archive` and returns the file's contents.
    pub fn read(&self, archive: &mut (impl Read + Seek)) -> io::Result<Vec<u8>> {
        if self.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is a directory", self.path),
            ));
        }
        let mut stored = Vec::new();
        archive.seek(SeekFrom::Start(self.offset.into()))?;
        archive
            .take(self.stored_size.into())
            .read_to_end(&mut stored)?;
        if stored.len() != self.stored_size as usize {
            return Err(invalid(format!("{}: stored bytes truncated", self.path)));
        }
        if !self.is_compressed() {
            return Ok(stored);
        }
        decompress_file(&stored, self.original_size as usize)
            .map_err(|e| invalid(format!("{}: {e}", self.path)))
    }
}

/// Decompresses LZO1X file data (section 6) to exactly `size` bytes.
pub fn decompress_file(stored: &[u8], size: usize) -> io::Result<Vec<u8>> {
    let data = lzokay_native::decompress_all(stored, Some(size))
        .map_err(|e| invalid(format!("LZO1X: {e}")))?;
    if data.len() != size {
        return Err(invalid(format!(
            "LZO1X: {} bytes decompressed, expected {size}",
            data.len()
        )));
    }
    Ok(data)
}

/// An open archive: its file table and the reader holding its bytes.
#[derive(Debug)]
pub struct Archive<R> {
    reader: R,
    entries: Vec<Entry>,
}

impl<R: Read + Seek> Archive<R> {
    /// Reads the file table of the archive in `reader`.
    pub fn open(mut reader: R, scrambling: Scrambling) -> io::Result<Self> {
        let entries = read_table(&mut reader, scrambling)?;
        Ok(Self { reader, entries })
    }

    /// File table entries in stored order, directories included.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry with exactly this stored path.
    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// Reads the contents of `entry`, which must come from this archive.
    pub fn read(&mut self, entry: &Entry) -> io::Result<Vec<u8>> {
        entry.read(&mut self.reader)
    }

    /// The reader and the file table.
    pub fn into_parts(self) -> (R, Vec<Entry>) {
        (self.reader, self.entries)
    }
}

/// Reads and decodes the file table of the archive in `reader` (sections 1–3).
pub fn read_table(
    reader: &mut (impl Read + Seek),
    scrambling: Scrambling,
) -> io::Result<Vec<Entry>> {
    parse_table(&read_directory_chunk(reader)?.decode(scrambling)?)
}

/// The directory chunk, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryChunk {
    /// Archive offset of the chunk header.
    pub offset: u64,
    /// The chunk id's compressed flag.
    pub compressed: bool,
    pub payload: Vec<u8>,
}

impl DirectoryChunk {
    /// The decoded file table: the payload, descrambled and decompressed if
    /// the compressed flag is set (section 2).
    pub fn decode(&self, scrambling: Scrambling) -> io::Result<Vec<u8>> {
        if !self.compressed {
            return Ok(self.payload.clone());
        }
        let mut payload = self.payload.clone();
        if scrambling == Scrambling::Scrambled {
            descramble(&mut payload);
        }
        decompress_directory(&payload)
    }
}

/// Walks the chunks from offset 0 to the first directory chunk (section 1).
pub fn read_directory_chunk(reader: &mut (impl Read + Seek)) -> io::Result<DirectoryChunk> {
    let end = reader.seek(SeekFrom::End(0))?;
    let mut offset = 0u64;
    loop {
        if end - offset < 8 {
            return Err(invalid("no directory chunk"));
        }
        let mut header = [0u8; 8];
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut header)?;
        let id = u32::from_le_bytes(header[..4].try_into().unwrap());
        let len = u64::from(u32::from_le_bytes(header[4..].try_into().unwrap()));
        if len > end - offset - 8 {
            return Err(invalid(format!("chunk at {offset} runs past end of file")));
        }
        if id & !COMPRESSED == DIRECTORY_CHUNK {
            let mut payload = vec![0; len as usize];
            reader.read_exact(&mut payload)?;
            return Ok(DirectoryChunk {
                offset,
                compressed: id & COMPRESSED != 0,
                payload,
            });
        }
        offset += 8 + len;
    }
}

/// Splits a decoded file table into entries (section 3).
pub fn parse_table(mut table: &[u8]) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    while !table.is_empty() {
        let Some((len, rest)) = table.split_first_chunk::<2>() else {
            return Err(invalid("file table ends inside an entry length"));
        };
        let len = usize::from(u16::from_le_bytes(*len));
        if len < ENTRY_FIXED || len > rest.len() {
            return Err(invalid(format!(
                "file table entry {} has invalid length {len}",
                entries.len()
            )));
        }
        let (body, rest) = rest.split_at(len);
        let u32_at = |at: usize| u32::from_le_bytes(body[at..at + 4].try_into().unwrap());
        entries.push(Entry {
            original_size: u32_at(0),
            stored_size: u32_at(4),
            checksum: u32_at(8),
            path: body[12..len - 4].iter().map(|&b| char::from(b)).collect(),
            offset: u32_at(len - 4),
        });
        table = rest;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn table_entry(path: &str, original: u32, stored: u32, crc: u32, offset: u32) -> Vec<u8> {
        let len = (ENTRY_FIXED + path.len()) as u16;
        let mut out = len.to_le_bytes().to_vec();
        for v in [original, stored, crc] {
            out.extend(v.to_le_bytes());
        }
        out.extend(path.as_bytes());
        out.extend(offset.to_le_bytes());
        out
    }

    fn chunk(id: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = id.to_le_bytes().to_vec();
        out.extend((payload.len() as u32).to_le_bytes());
        out.extend(payload);
        out
    }

    #[test]
    fn reads_archive_with_plain_directory() {
        let hello = b"hello";
        let packed = lzokay_native::compress(&[b'x'; 300]).unwrap();
        let mut data = hello.to_vec();
        data.extend(&packed);
        let mut table = table_entry("dir\\", 0, 0, 0, 0);
        table.extend(table_entry("dir\\a.txt", 5, 5, crc32fast::hash(hello), 8));
        table.extend(table_entry(
            "dir\\x.bin",
            300,
            packed.len() as u32,
            crc32fast::hash(&packed),
            13,
        ));
        // An unknown chunk type before the directory is skipped.
        let mut file = chunk(0, &data);
        file.extend(chunk(7, b"ignored"));
        file.extend(chunk(DIRECTORY_CHUNK, &table));

        let mut archive = Archive::open(Cursor::new(file), Scrambling::Scrambled).unwrap();
        let entries = archive.entries().to_vec();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_dir());
        assert_eq!(entries[1].path, "dir\\a.txt");
        assert_eq!(archive.read(&entries[1]).unwrap(), hello);
        assert!(entries[2].is_compressed());
        assert_eq!(archive.read(&entries[2]).unwrap(), [b'x'; 300]);
        assert!(archive.read(&entries[0]).is_err());
        assert_eq!(archive.find("dir\\a.txt"), Some(&entries[1]));
    }

    #[test]
    fn rejects_malformed_archives() {
        let open = |bytes: Vec<u8>| Archive::open(Cursor::new(bytes), Scrambling::Plain);
        assert!(open(chunk(0, b"data")).is_err(), "no directory chunk");
        let mut long = chunk(DIRECTORY_CHUNK, b"");
        long[4] = 9;
        assert!(open(long).is_err(), "chunk past end");
        assert!(
            open(chunk(DIRECTORY_CHUNK, &[15, 0])).is_err(),
            "short entry"
        );
        let mut truncated = table_entry("a", 1, 1, 0, 100);
        truncated.pop();
        assert!(open(chunk(DIRECTORY_CHUNK, &truncated)).is_err());
        let mut archive = open(chunk(DIRECTORY_CHUNK, &table_entry("a", 4, 4, 0, 100))).unwrap();
        let entry = archive.entries()[0].clone();
        assert!(archive.read(&entry).is_err(), "data past end");
    }

    #[test]
    fn scrambling_depends_on_path() {
        assert_eq!(Scrambling::for_path("gamedata.db0"), Scrambling::Scrambled);
        assert_eq!(
            Scrambling::for_path("mods/mp_military_2.xdb0"),
            Scrambling::Plain
        );
        assert_eq!(Scrambling::for_path("MODS/X.XDB1"), Scrambling::Plain);
    }
}
