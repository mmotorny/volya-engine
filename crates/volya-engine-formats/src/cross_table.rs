// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's cross table `levels\<level>\level.gct`
//! (docs/behavior/offline-simulation.md section 2): for each level vertex
//! of the level's `level.ai`, the nearest game vertex and its distance. A
//! chunk sequence of two chunks, read as a stream into a leaner structure
//! of the same cells (#797): the distinct cells in a table, each level
//! vertex's an index into it (a level's cells repeat: `l01_escape`'s
//! 595,499 hold 13,444 distinct ones). The bytes are given by the reader's
//! caller (the engine's file cache, docs/architecture/memory.md,
//! File-backed data).

use std::collections::HashMap;
use std::fmt;
use std::io::{self, Cursor, ErrorKind, Read, Seek, SeekFrom};
use std::ops::DerefMut;

use crate::Bytes;
use crate::chunks;
use crate::packet::Reader;

/// The only cross-table version read.
pub const VERSION: u32 = 8;
const HEADER_CHUNK: u32 = 0;
const CELLS_CHUNK: u32 = 1;
const HEADER_SIZE: usize = 44;
const CELL_SIZE: usize = 6;
/// Cells read from the file at a time.
const BATCH: usize = 4096;

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
    /// The distinct cells, each 6 bytes as stored (little-endian: the
    /// nearest game vertex `u16`, its distance in metres `f32`), then per
    /// level vertex its cell's index, `width` bytes little-endian.
    bytes: Box<dyn Bytes>,
    /// The distinct cells.
    distinct: usize,
    /// Bytes of an index: 2, or 4 where more than 65,536 cells are
    /// distinct.
    width: usize,
    /// The level vertex count *N*.
    len: usize,
}

impl fmt::Debug for CrossTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CrossTable")
            .field("game_vertex_count", &self.game_vertex_count)
            .field("level_identifier", &self.level_identifier)
            .field("game_identifier", &self.game_identifier)
            .field("cells", &self.len())
            .field("distinct", &self.distinct)
            .finish()
    }
}

impl PartialEq for CrossTable {
    fn eq(&self, other: &Self) -> bool {
        self.game_vertex_count == other.game_vertex_count
            && self.level_identifier == other.level_identifier
            && self.game_identifier == other.game_identifier
            && self.len == other.len
            && (0..self.len).all(|k| self.cell_bytes(k) == other.cell_bytes(k))
    }
}

/// The distinct cells of a sequence, by first appearance.
#[derive(Default)]
struct Distinct {
    index: HashMap<[u8; CELL_SIZE], u32>,
    cells: Vec<[u8; CELL_SIZE]>,
}

impl Distinct {
    fn add(&mut self, cell: [u8; CELL_SIZE]) {
        let next = self.cells.len() as u32;
        if *self.index.entry(cell).or_insert(next) == next {
            self.cells.push(cell);
        }
    }

    fn width(&self) -> usize {
        if self.cells.len() <= 1 << 16 { 2 } else { 4 }
    }

    /// The bytes for `n` cells' indices after the table.
    fn bytes(&self, n: usize) -> usize {
        self.cells.len() * CELL_SIZE + n * self.width()
    }

    /// Writes the table into `out` (its first [`Self::bytes`]).
    fn write_table(&self, out: &mut [u8]) {
        for (to, c) in out
            .as_chunks_mut::<CELL_SIZE>()
            .0
            .iter_mut()
            .zip(&self.cells)
        {
            *to = *c;
        }
    }

    /// Writes `cell`'s index into `to` (its width).
    fn write_index(&self, cell: &[u8; CELL_SIZE], to: &mut [u8]) {
        let i = self.index[cell];
        to.copy_from_slice(&i.to_le_bytes()[..to.len()]);
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
        let cells: Vec<[u8; CELL_SIZE]> = cells.iter().map(encode).collect();
        let mut distinct = Distinct::default();
        cells.iter().for_each(|&c| distinct.add(c));
        let width = distinct.width();
        let mut bytes = vec![0; distinct.bytes(cells.len())];
        let (table, indices) = bytes.split_at_mut(distinct.cells.len() * CELL_SIZE);
        distinct.write_table(table);
        for (c, to) in cells.iter().zip(indices.chunks_exact_mut(width)) {
            distinct.write_index(c, to);
        }
        Self {
            game_vertex_count,
            level_identifier,
            game_identifier,
            bytes: Box::new(bytes),
            distinct: distinct.cells.len(),
            width,
            len: cells.len(),
        }
    }

    /// Parses `file`, the table kept on the heap ([`Self::read_in`]).
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        Self::read_in(Cursor::new(file), |len| Ok(vec![0; len]))
    }

    /// Reads the cross table `file`, keeping its cells in the zeroed bytes
    /// `alloc` gives for their size. The first chunk of each id counts
    /// (chunks.md 3). A missing chunk 0 or 1, a chunk 0 of another size,
    /// or cells that are not *N* × 6 bytes are the game's "Cross table is
    /// corrupted!"; a version other than 8 its "Cross table version
    /// mismatch!". A compressed chunk is an error (our choice: the shipped
    /// tables are stored).
    pub fn read_in<B: DerefMut<Target = [u8]> + Send + Sync + 'static>(
        mut file: impl Read + Seek,
        alloc: impl FnOnce(usize) -> io::Result<B>,
    ) -> io::Result<Self> {
        let corrupted = || invalid("Cross table is corrupted!");
        let eof = |e: io::Error| {
            if e.kind() == ErrorKind::UnexpectedEof {
                corrupted()
            } else {
                e
            }
        };
        let length = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(0))?;
        // (offset, size) of the first chunk of each.
        let mut header: Option<(u64, usize)> = None;
        let mut cells: Option<(u64, usize)> = None;
        let mut at = 0u64;
        while at < length {
            let mut head = [0u8; chunks::HEADER_SIZE];
            file.read_exact(&mut head).map_err(eof)?;
            let id_word = u32::from_le_bytes(head[..4].try_into().unwrap());
            let size = u32::from_le_bytes(head[4..].try_into().unwrap()) as usize;
            let payload = at + chunks::HEADER_SIZE as u64;
            if size as u64 > length - payload {
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
                header = Some((payload, size));
            } else if wanted {
                if !size.is_multiple_of(CELL_SIZE) {
                    return Err(corrupted());
                }
                cells = Some((payload, size));
            }
            at = payload + size as u64;
            file.seek(SeekFrom::Start(at))?;
        }
        let (header, _) = header.ok_or_else(corrupted)?;
        let (cells, size) = cells.ok_or_else(corrupted)?;
        let mut head = [0u8; HEADER_SIZE];
        file.seek(SeekFrom::Start(header))?;
        file.read_exact(&mut head)?;
        let mut h = Reader::new(&head);
        if h.u32()? != VERSION {
            return Err(invalid("Cross table version mismatch!"));
        }
        let n = h.u32()? as usize;
        let game_vertex_count = h.u32()?;
        let level_identifier = h.take(16)?.try_into().unwrap();
        let game_identifier = h.take(16)?.try_into().unwrap();
        if size / CELL_SIZE != n {
            return Err(corrupted());
        }
        // Each cell visited in level vertex order, a batch at a time.
        let mut buffer = vec![0u8; CELL_SIZE * BATCH.min(n)];
        let mut each = |file: &mut dyn ReadSeek, f: &mut dyn FnMut(usize, &[u8; CELL_SIZE])| {
            file.seek(SeekFrom::Start(cells))?;
            let mut k = 0;
            while k < n {
                let batch = (n - k).min(BATCH);
                let bytes = &mut buffer[..CELL_SIZE * batch];
                file.read_exact(bytes)?;
                for c in bytes.as_chunks::<CELL_SIZE>().0 {
                    f(k, c);
                    k += 1;
                }
            }
            io::Result::Ok(())
        };
        let mut distinct = Distinct::default();
        each(&mut file, &mut |_, c| distinct.add(*c))?;
        let width = distinct.width();
        let table = distinct.cells.len() * CELL_SIZE;
        let mut kept = alloc(distinct.bytes(n))?;
        if kept.len() != distinct.bytes(n) {
            return Err(invalid("cross table: no room for the cells"));
        }
        distinct.write_table(&mut kept[..table]);
        let indices = &mut kept[table..];
        each(&mut file, &mut |k, c| {
            distinct.write_index(c, &mut indices[k * width..(k + 1) * width]);
        })?;
        Ok(Self {
            game_vertex_count,
            level_identifier,
            game_identifier,
            bytes: Box::new(kept),
            distinct: distinct.cells.len(),
            width,
            len: n,
        })
    }

    /// The stored bytes of level vertex `k`'s cell (`k` below *N*).
    fn cell_bytes(&self, k: usize) -> &[u8; CELL_SIZE] {
        let table = self.distinct * CELL_SIZE;
        let at = table + k * self.width;
        let mut index = [0u8; 4];
        index[..self.width].copy_from_slice(&self.bytes[at..at + self.width]);
        let i = u32::from_le_bytes(index) as usize;
        self.bytes[i * CELL_SIZE..(i + 1) * CELL_SIZE]
            .try_into()
            .unwrap()
    }

    /// The level vertex count *N*.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The distinct cells.
    pub fn distinct(&self) -> usize {
        self.distinct
    }

    /// The cell of level vertex `k`: (game vertex, distance).
    pub fn cell(&self, k: u32) -> Option<(u16, f32)> {
        ((k as usize) < self.len).then(|| decode(self.cell_bytes(k as usize)))
    }

    /// Every cell, in level vertex order.
    pub fn cells(&self) -> impl Iterator<Item = (u16, f32)> + '_ {
        (0..self.len).map(|k| decode(self.cell_bytes(k)))
    }
}

/// A file read and seeked: [`CrossTable::read_in`]'s passes over it.
trait ReadSeek: Read + Seek {}

impl<T: Read + Seek> ReadSeek for T {}

fn encode(&(v, d): &(u16, f32)) -> [u8; CELL_SIZE] {
    let mut c = [0; CELL_SIZE];
    c[..2].copy_from_slice(&v.to_le_bytes());
    c[2..].copy_from_slice(&d.to_le_bytes());
    c
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

    /// Repeated cells are kept once and read back as stored, their bits
    /// included (a negative zero, a NaN); past 65,536 distinct cells the
    /// indices widen.
    #[test]
    fn keeps_distinct_cells_once() {
        let nan = f32::from_bits(0x7fc0_1234);
        let cells: Vec<(u16, f32)> = (0..1000)
            .map(|k| match k % 4 {
                0 => (7, 1.5),
                1 => (7, -0.0),
                2 => (9, nan),
                _ => (7, 0.0),
            })
            .collect();
        let file = cross_table_file(&cells, [1; 16], [2; 16]);
        let t = CrossTable::parse(&file).unwrap();
        assert_eq!(t.distinct(), 4);
        assert_eq!(t.bytes.len(), 4 * CELL_SIZE + 1000 * 2);
        let bits = |c: &[(u16, f32)]| c.iter().map(|(v, d)| (*v, d.to_bits())).collect::<Vec<_>>();
        assert_eq!(bits(&t.cells().collect::<Vec<_>>()), bits(&cells));
        let many: Vec<(u16, f32)> = (0..70_000).map(|k| (k as u16, k as f32)).collect();
        let t = CrossTable::parse(&cross_table_file(&many, [1; 16], [2; 16])).unwrap();
        assert_eq!((t.distinct(), t.width), (70_000, 4));
        assert_eq!(t.cell(69_999), Some((69_999u32 as u16, 69_999.0)));
        assert!(t.cells().eq(many.iter().copied()));
    }
}
