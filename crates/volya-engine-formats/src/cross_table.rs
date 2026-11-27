// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's cross table `levels\<level>\level.gct`
//! (docs/behavior/offline-simulation.md section 2): for each level vertex
//! of the level's `level.ai`, the nearest game vertex and its distance. A
//! chunk sequence of two chunks, read in place: the cells stay in the file's
//! bytes, which the reader's caller holds (the engine's file cache,
//! docs/architecture/memory.md, File-backed data).

use std::fmt;
use std::io;
use std::ops::Range;

use crate::Bytes;
use crate::chunks;
use crate::packet::Reader;

/// The only cross-table version read.
pub const VERSION: u32 = 8;
const HEADER_CHUNK: u32 = 0;
const CELLS_CHUNK: u32 = 1;
const HEADER_SIZE: usize = 44;
const CELL_SIZE: usize = 6;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The cross table, one cell per level vertex.
pub struct CrossTable {
    /// The level's number of game vertices.
    pub game_vertex_count: u32,
    /// Must equal the `level.ai` identifier.
    pub level_identifier: [u8; 16],
    /// Must equal the game graph's identifier (1.8).
    pub game_identifier: [u8; 16],
    /// The file, or the cells alone.
    bytes: Box<dyn Bytes>,
    /// Where in `bytes` the cells are, each 6 bytes, little-endian: the
    /// nearest game vertex (`u16`), its distance in metres (`f32`).
    cells: Range<usize>,
}

impl fmt::Debug for CrossTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CrossTable")
            .field("game_vertex_count", &self.game_vertex_count)
            .field("level_identifier", &self.level_identifier)
            .field("game_identifier", &self.game_identifier)
            .field("cells", &self.len())
            .finish()
    }
}

impl PartialEq for CrossTable {
    fn eq(&self, other: &Self) -> bool {
        self.game_vertex_count == other.game_vertex_count
            && self.level_identifier == other.level_identifier
            && self.game_identifier == other.game_identifier
            && self.cell_bytes() == other.cell_bytes()
    }
}

impl CrossTable {
    /// A table of `cells`, each (game vertex, distance), on the heap.
    pub fn new(
        game_vertex_count: u32,
        level_identifier: [u8; 16],
        game_identifier: [u8; 16],
        cells: &[(u16, f32)],
    ) -> Self {
        let mut bytes = Vec::with_capacity(cells.len() * CELL_SIZE);
        for (v, d) in cells {
            bytes.extend(v.to_le_bytes());
            bytes.extend(d.to_le_bytes());
        }
        Self {
            game_vertex_count,
            level_identifier,
            game_identifier,
            cells: 0..bytes.len(),
            bytes: Box::new(bytes),
        }
    }

    /// Parses a copy of the file ([`CrossTable::in_place`]).
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        Self::in_place(file.to_vec())
    }

    /// Reads the file `file` in place, keeping it. The first chunk of each
    /// id counts (chunks.md 3). A missing chunk 0 or 1, a chunk 0 of
    /// another size, or cells that are not *N* × 6 bytes are the game's
    /// "Cross table is corrupted!"; a version other than 8 its "Cross table
    /// version mismatch!". A compressed chunk is an error (our choice: the
    /// shipped tables are stored).
    pub fn in_place(file: impl Bytes + 'static) -> io::Result<Self> {
        let corrupted = || invalid("Cross table is corrupted!");
        let length = file.len();
        let mut at = 0;
        let mut header: Option<Range<usize>> = None;
        let mut cells: Option<Range<usize>> = None;
        while at < length {
            let head = file
                .get(at..at + chunks::HEADER_SIZE)
                .ok_or_else(corrupted)?;
            let id_word = u32::from_le_bytes(head[..4].try_into().unwrap());
            let size = u32::from_le_bytes(head[4..].try_into().unwrap()) as usize;
            let payload = at + chunks::HEADER_SIZE;
            if size > length - payload {
                return Err(corrupted());
            }
            let id = id_word & !chunks::COMPRESSED;
            let wanted = match id {
                HEADER_CHUNK => header.is_none(),
                CELLS_CHUNK => cells.is_none(),
                _ => false,
            };
            if wanted && id_word & chunks::COMPRESSED != 0 {
                return Err(invalid("cross table: compressed chunk"));
            }
            if wanted && id == HEADER_CHUNK {
                if size != HEADER_SIZE {
                    return Err(corrupted());
                }
                header = Some(payload..payload + size);
            } else if wanted {
                if !size.is_multiple_of(CELL_SIZE) {
                    return Err(corrupted());
                }
                cells = Some(payload..payload + size);
            }
            at = payload + size;
        }
        let header = header.ok_or_else(corrupted)?;
        let cells = cells.ok_or_else(corrupted)?;
        let mut h = Reader::new(&file[header]);
        if h.u32()? != VERSION {
            return Err(invalid("Cross table version mismatch!"));
        }
        let n = h.u32()? as usize;
        let game_vertex_count = h.u32()?;
        let level_identifier = h.take(16)?.try_into().unwrap();
        let game_identifier = h.take(16)?.try_into().unwrap();
        if cells.len() / CELL_SIZE != n {
            return Err(corrupted());
        }
        Ok(Self {
            game_vertex_count,
            level_identifier,
            game_identifier,
            bytes: Box::new(file),
            cells,
        })
    }

    fn cell_bytes(&self) -> &[[u8; CELL_SIZE]] {
        self.bytes[self.cells.clone()].as_chunks::<CELL_SIZE>().0
    }

    /// The level vertex count *N*.
    pub fn len(&self) -> usize {
        self.cells.len() / CELL_SIZE
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The cell of level vertex `k`: (game vertex, distance).
    pub fn cell(&self, k: u32) -> Option<(u16, f32)> {
        self.cell_bytes().get(k as usize).map(decode)
    }

    /// Every cell, in level vertex order.
    pub fn cells(&self) -> impl Iterator<Item = (u16, f32)> + '_ {
        self.cell_bytes().iter().map(decode)
    }
}

fn decode(c: &[u8; CELL_SIZE]) -> (u16, f32) {
    (
        u16::from_le_bytes([c[0], c[1]]),
        f32::from_le_bytes([c[2], c[3], c[4], c[5]]),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A cross-table file of `cells` with identifiers `level` and `game`.
    pub fn cross_table_file(cells: &[(u16, f32)], level: [u8; 16], game: [u8; 16]) -> Vec<u8> {
        let mut header = Vec::new();
        header.extend(VERSION.to_le_bytes());
        header.extend((cells.len() as u32).to_le_bytes());
        header.extend(1u32.to_le_bytes());
        header.extend(level);
        header.extend(game);
        let mut body = Vec::new();
        for (v, d) in cells {
            body.extend(v.to_le_bytes());
            body.extend(d.to_le_bytes());
        }
        let mut file = Vec::new();
        for (id, payload) in [(0u32, header), (1, body)] {
            file.extend(id.to_le_bytes());
            file.extend((payload.len() as u32).to_le_bytes());
            file.extend(payload);
        }
        file
    }

    #[test]
    fn parses_cells() {
        let file = cross_table_file(&[(3, 0.0), (4, 1.4)], [1; 16], [2; 16]);
        let t = CrossTable::parse(&file).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t.cell(1), Some((4, 1.4)));
        assert_eq!(t.cell(2), None);
        assert_eq!((t.level_identifier, t.game_identifier), ([1; 16], [2; 16]));
        assert_eq!(t.cells().collect::<Vec<_>>(), [(3, 0.0), (4, 1.4)]);
        assert_eq!(
            t,
            CrossTable::new(1, [1; 16], [2; 16], &[(3, 0.0), (4, 1.4)])
        );
        assert!(CrossTable::parse(&file[..52]).is_err());
        // A count other than the cells', a chunk beyond the file.
        let mut wrong = file.clone();
        wrong[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(CrossTable::parse(&wrong).is_err());
        let mut long = file.clone();
        long[56..60].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(CrossTable::parse(&long).is_err());
        // Another chunk between them is skipped.
        let mut other = 7u32.to_le_bytes().to_vec();
        other.extend(2u32.to_le_bytes());
        other.extend([0, 0]);
        let mut with = file[..52].to_vec();
        with.extend(other);
        with.extend(&file[52..]);
        assert_eq!(CrossTable::parse(&with).unwrap(), t);
    }
}
