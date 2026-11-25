// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's cross table `levels\<level>\level.gct`
//! (docs/behavior/offline-simulation.md section 2): for each level vertex
//! of the level's `level.ai`, the nearest game vertex and its distance. A
//! chunk sequence of two chunks.

use std::io;

use crate::chunks::Chunks;
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
    /// Parses the file. A missing chunk 0 or 1, or a chunk 0 of another
    /// size, is the game's "Cross table is corrupted!"; a version other than
    /// 8 its "Cross table version mismatch!".
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(file)?;
        let corrupted = || invalid("Cross table is corrupted!");
        let header = chunks.find(HEADER_CHUNK).ok_or_else(corrupted)?.content()?;
        let cells = chunks.find(CELLS_CHUNK).ok_or_else(corrupted)?.content()?;
        if header.len() != HEADER_SIZE {
            return Err(corrupted());
        }
        let mut h = Reader::new(&header);
        if h.u32()? != VERSION {
            return Err(invalid("Cross table version mismatch!"));
        }
        let n = h.u32()? as usize;
        let game_vertex_count = h.u32()?;
        let level_identifier = h.take(16)?.try_into().unwrap();
        let game_identifier = h.take(16)?.try_into().unwrap();
        if cells.len() != CELL_SIZE * n {
            return Err(corrupted());
        }
        let mut game_vertices = Vec::with_capacity(n);
        let mut distances = Vec::with_capacity(n);
        for c in cells.as_chunks::<CELL_SIZE>().0 {
            game_vertices.push(u16::from_le_bytes([c[0], c[1]]));
            distances.push(f32::from_le_bytes([c[2], c[3], c[4], c[5]]));
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
    }
}
