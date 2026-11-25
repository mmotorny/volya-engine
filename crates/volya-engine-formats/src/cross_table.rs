// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's cross table `levels\<level>\level.gct`
//! (docs/behavior/offline-simulation.md section 2): for each level vertex
//! of the level's `level.ai`, the nearest game vertex and its distance. A
//! chunk sequence of two chunks.

use std::io::{self, Read, Seek, SeekFrom};

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
#[derive(Clone, Debug, PartialEq)]
pub struct CrossTable {
    /// The level's number of game vertices.
    pub game_vertex_count: u32,
    /// Must equal the `level.ai` identifier.
    pub level_identifier: [u8; 16],
    /// Must equal the game graph's identifier (1.8).
    pub game_identifier: [u8; 16],
    /// Per level vertex: the nearest game vertex.
    pub game_vertices: Vec<u16>,
    /// Per level vertex: its distance to that game vertex, metres.
    pub distances: Vec<f32>,
}

impl CrossTable {
    /// Parses the file in memory ([`CrossTable::read`]).
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        Self::read(io::Cursor::new(file))
    }

    /// Reads the file as a stream, one chunk header at a time, the cells in
    /// batches, without holding the file. The first chunk of each id counts
    /// (chunks.md 3). A missing chunk 0 or 1, a chunk 0 of another size, or
    /// cells that are not *N* × 6 bytes are the game's "Cross table is
    /// corrupted!"; a version other than 8 its "Cross table version
    /// mismatch!". A compressed chunk is an error (our choice: the shipped
    /// tables are stored).
    pub fn read(mut file: impl Read + Seek) -> io::Result<Self> {
        let corrupted = || invalid("Cross table is corrupted!");
        let length = file.seek(SeekFrom::End(0))?;
        let mut at = file.seek(SeekFrom::Start(0))?;
        let mut header: Option<[u8; HEADER_SIZE]> = None;
        let mut cells: Option<(Vec<u16>, Vec<f32>)> = None;
        while at < length {
            let mut head = [0u8; chunks::HEADER_SIZE];
            file.read_exact(&mut head)?;
            let id_word = u32::from_le_bytes(head[..4].try_into().unwrap());
            let size = u64::from(u32::from_le_bytes(head[4..].try_into().unwrap()));
            let payload = at + chunks::HEADER_SIZE as u64;
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
                if size != HEADER_SIZE as u64 {
                    return Err(corrupted());
                }
                let mut h = [0u8; HEADER_SIZE];
                file.read_exact(&mut h)?;
                header = Some(h);
            } else if wanted {
                if size % CELL_SIZE as u64 != 0 {
                    return Err(corrupted());
                }
                // Bounded by the stream's length, checked above.
                let n = (size / CELL_SIZE as u64) as usize;
                let mut vertices = Vec::with_capacity(n);
                let mut distances = Vec::with_capacity(n);
                let mut buffer = vec![0u8; CELL_SIZE * 4096];
                let mut left = n;
                while left > 0 {
                    let batch = left.min(4096);
                    let bytes = &mut buffer[..CELL_SIZE * batch];
                    file.read_exact(bytes)?;
                    for c in bytes.as_chunks::<CELL_SIZE>().0 {
                        vertices.push(u16::from_le_bytes([c[0], c[1]]));
                        distances.push(f32::from_le_bytes([c[2], c[3], c[4], c[5]]));
                    }
                    left -= batch;
                }
                cells = Some((vertices, distances));
            }
            at = payload + size;
            file.seek(SeekFrom::Start(at))?;
        }
        let header = header.ok_or_else(corrupted)?;
        let (game_vertices, distances) = cells.ok_or_else(corrupted)?;
        let mut h = Reader::new(&header);
        if h.u32()? != VERSION {
            return Err(invalid("Cross table version mismatch!"));
        }
        let n = h.u32()? as usize;
        let game_vertex_count = h.u32()?;
        let level_identifier = h.take(16)?.try_into().unwrap();
        let game_identifier = h.take(16)?.try_into().unwrap();
        if game_vertices.len() != n {
            return Err(corrupted());
        }
        Ok(Self {
            game_vertex_count,
            level_identifier,
            game_identifier,
            game_vertices,
            distances,
        })
    }

    /// The level vertex count *N*.
    pub fn len(&self) -> usize {
        self.game_vertices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.game_vertices.is_empty()
    }

    /// The cell of level vertex `k`: (game vertex, distance).
    pub fn cell(&self, k: u32) -> Option<(u16, f32)> {
        let k = k as usize;
        Some((*self.game_vertices.get(k)?, self.distances[k]))
    }
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
