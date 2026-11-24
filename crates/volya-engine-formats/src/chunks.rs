// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The chunk container shared by most binary game files
//! (docs/formats/chunks.md): sequences of `u32` id word, `u32` size, payload,
//! nested to any depth.
//!
//! [`Chunks`] walks a sequence held in memory; [`index`] walks one in a
//! `Read + Seek` stream without reading the payloads, for large files read as
//! streams (`level.geom`).

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};

use crate::db::decompress_directory;

/// Bit 31 of the id word: the payload is compressed (section 5).
pub const COMPRESSED: u32 = 0x8000_0000;
/// Header size: id word and size.
pub const HEADER_SIZE: usize = 8;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// One chunk of a sequence in memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    /// The full id word, compressed flag included (section 4).
    pub id_word: u32,
    /// Offset of the payload within the sequence.
    pub offset: usize,
    /// The payload as stored.
    pub payload: &'a [u8],
}

impl<'a> Chunk<'a> {
    /// Bits 0–30 of the id word.
    pub fn id(&self) -> u32 {
        self.id_word & !COMPRESSED
    }

    pub fn is_compressed(&self) -> bool {
        self.id_word & COMPRESSED != 0
    }

    /// The chunk's content: the payload, decoded if compressed (section 5:
    /// LZHUF, no descrambling). We always decode, as the spec recommends.
    pub fn content(&self) -> io::Result<Cow<'a, [u8]>> {
        if !self.is_compressed() {
            return Ok(Cow::Borrowed(self.payload));
        }
        decompress_directory(self.payload)
            .map(Cow::Owned)
            .map_err(|e| invalid(format!("compressed chunk {:#x}: {e}", self.id())))
    }

    /// The payload, which must not be compressed (formats read as streams,
    /// section 5).
    pub fn uncompressed(&self) -> io::Result<&'a [u8]> {
        if self.is_compressed() {
            return Err(invalid(format!(
                "chunk {:#x} is compressed where that is not allowed",
                self.id()
            )));
        }
        Ok(self.payload)
    }
}

/// A chunk sequence in memory, validated on construction: every size fits the
/// bytes remaining and the chunks fill the region exactly (section 1).
#[derive(Clone, Debug, Default)]
pub struct Chunks<'a> {
    chunks: Vec<Chunk<'a>>,
}

impl<'a> Chunks<'a> {
    /// Walks the sequence filling `data`.
    pub fn parse(data: &'a [u8]) -> io::Result<Self> {
        let mut chunks = Vec::new();
        let mut offset = 0;
        while offset < data.len() {
            let rest = &data[offset..];
            let Some((header, rest)) = rest.split_first_chunk::<HEADER_SIZE>() else {
                return Err(invalid(format!(
                    "{} bytes left over after the last chunk",
                    rest.len()
                )));
            };
            let id_word = u32::from_le_bytes(header[..4].try_into().unwrap());
            let size = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
            if size > rest.len() {
                return Err(invalid(format!(
                    "chunk {:#x} at {offset} claims {size} bytes, {} remain",
                    id_word & !COMPRESSED,
                    rest.len()
                )));
            }
            chunks.push(Chunk {
                id_word,
                offset: offset + HEADER_SIZE,
                payload: &rest[..size],
            });
            offset += HEADER_SIZE + size;
        }
        Ok(Self { chunks })
    }

    /// Every chunk in stored order, duplicates and empty ones included
    /// (section 4).
    pub fn iter(&self) -> impl Iterator<Item = &Chunk<'a>> {
        self.chunks.iter()
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// The first chunk with id `id`, compressed flag ignored (section 3).
    /// Unlike the game's lookup, an empty chunk is returned rather than
    /// reported as "not found"; callers that need the game's view check
    /// [`Chunk::payload`]'s length.
    pub fn find(&self, id: u32) -> Option<&Chunk<'a>> {
        self.chunks.iter().find(|c| c.id() == id)
    }

    /// The first chunk with id `id`, or an error naming `what` if missing.
    pub fn require(&self, id: u32, what: &str) -> io::Result<&Chunk<'a>> {
        self.find(id)
            .ok_or_else(|| invalid(format!("{what}: chunk {id:#x} missing")))
    }

    /// A list stored as sub-chunks 0, 1, 2, …: looked up by id in that order
    /// up to the first id that is not found or whose chunk is empty
    /// (section 2).
    pub fn list(&self) -> Vec<&Chunk<'a>> {
        let mut first = HashMap::new();
        for chunk in &self.chunks {
            first.entry(chunk.id()).or_insert(chunk);
        }
        (0..)
            .map_while(|id| first.get(&id).copied().filter(|c| !c.payload.is_empty()))
            .collect()
    }
}

/// A chunk header found in a stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkHeader {
    /// The full id word, compressed flag included.
    pub id_word: u32,
    /// Stream position of the payload.
    pub offset: u64,
    /// Stored payload size.
    pub size: u32,
}

impl ChunkHeader {
    pub fn id(&self) -> u32 {
        self.id_word & !COMPRESSED
    }

    pub fn is_compressed(&self) -> bool {
        self.id_word & COMPRESSED != 0
    }

    /// Reads the stored payload.
    pub fn read_payload(&self, reader: &mut (impl Read + Seek)) -> io::Result<Vec<u8>> {
        reader.seek(SeekFrom::Start(self.offset))?;
        let mut payload = vec![0; self.size as usize];
        reader.read_exact(&mut payload)?;
        Ok(payload)
    }
}

/// Walks the top-level chunk sequence filling `reader` (start to end) and
/// returns the headers in stored order, validated as [`Chunks::parse`] does.
pub fn index(reader: &mut (impl Read + Seek)) -> io::Result<Vec<ChunkHeader>> {
    let end = reader.seek(SeekFrom::End(0))?;
    let mut headers = Vec::new();
    let mut offset = 0u64;
    while offset < end {
        let remaining = end - offset;
        if remaining < HEADER_SIZE as u64 {
            return Err(invalid(format!(
                "{remaining} bytes left over after the last chunk"
            )));
        }
        let mut header = [0u8; HEADER_SIZE];
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut header)?;
        let id_word = u32::from_le_bytes(header[..4].try_into().unwrap());
        let size = u32::from_le_bytes(header[4..].try_into().unwrap());
        if u64::from(size) > remaining - HEADER_SIZE as u64 {
            return Err(invalid(format!(
                "chunk {:#x} at {offset} claims {size} bytes, {} remain",
                id_word & !COMPRESSED,
                remaining - HEADER_SIZE as u64
            )));
        }
        headers.push(ChunkHeader {
            id_word,
            offset: offset + HEADER_SIZE as u64,
            size,
        });
        offset += HEADER_SIZE as u64 + u64::from(size);
    }
    Ok(headers)
}

/// The first header with id `id`, compressed flag ignored (section 3).
pub fn find_header(headers: &[ChunkHeader], id: u32) -> Option<&ChunkHeader> {
    headers.iter().find(|h| h.id() == id)
}

#[cfg(test)]
pub(crate) fn chunk(id_word: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = id_word.to_le_bytes().to_vec();
    out.extend((payload.len() as u32).to_le_bytes());
    out.extend(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn walks_lookups_and_iteration() {
        let mut data = chunk(4, b"four");
        data.extend(chunk(1, b"a"));
        data.extend(chunk(2, b""));
        data.extend(chunk(1, b"second one"));
        let chunks = Chunks::parse(&data).unwrap();
        assert_eq!(chunks.len(), 4);
        // Any order; the first of duplicate ids wins.
        assert_eq!(chunks.find(1).unwrap().payload, b"a");
        assert_eq!(chunks.find(1).unwrap().offset, 20);
        assert_eq!(chunks.find(2).unwrap().payload, b"");
        assert!(chunks.find(3).is_none());
        assert!(chunks.require(3, "test").is_err());
        // Iteration visits duplicates and empty chunks.
        let ids: Vec<u32> = chunks.iter().map(Chunk::id).collect();
        assert_eq!(ids, [4, 1, 2, 1]);
        assert!(Chunks::parse(&[]).unwrap().is_empty());

        let headers = index(&mut Cursor::new(&data)).unwrap();
        assert_eq!(headers.len(), 4);
        let h = find_header(&headers, 1).unwrap();
        assert_eq!((h.offset, h.size), (20, 1));
        assert_eq!(h.read_payload(&mut Cursor::new(&data)).unwrap(), b"a");
    }

    #[test]
    fn rejects_malformed_sequences() {
        let mut long = chunk(1, b"abc");
        long[4] = 4;
        assert!(Chunks::parse(&long).is_err());
        assert!(index(&mut Cursor::new(&long)).is_err());
        for extra in 1..8 {
            let mut data = chunk(1, b"abc");
            data.extend(vec![0; extra]);
            assert!(Chunks::parse(&data).is_err(), "{extra} bytes left");
            assert!(index(&mut Cursor::new(&data)).is_err());
        }
    }

    #[test]
    fn lists_end_at_first_gap_or_empty_chunk() {
        let mut data = chunk(1, b"b");
        data.extend(chunk(0, b"a"));
        data.extend(chunk(3, b"d"));
        let chunks = Chunks::parse(&data).unwrap();
        let items: Vec<&[u8]> = chunks.list().iter().map(|c| c.payload).collect();
        assert_eq!(items, [b"a", b"b"]);

        let mut data = chunk(0, b"a");
        data.extend(chunk(1, b""));
        data.extend(chunk(2, b"c"));
        assert_eq!(Chunks::parse(&data).unwrap().list().len(), 1);
    }

    #[test]
    fn compressed_flag_is_ignored_by_lookup_and_decoded() {
        // An LZHUF stream decoding to 0 bytes: just the length.
        let data = chunk(COMPRESSED | 7, &[0, 0, 0, 0]);
        let chunks = Chunks::parse(&data).unwrap();
        let c = chunks.find(7).unwrap();
        assert!(c.is_compressed());
        assert_eq!(c.id_word, 0x8000_0007);
        assert!(c.content().unwrap().is_empty());
        assert!(c.uncompressed().is_err());
        let plain = chunk(7, b"x");
        let chunks = Chunks::parse(&plain).unwrap();
        assert_eq!(&*chunks.find(7).unwrap().content().unwrap(), b"x");
    }
}
