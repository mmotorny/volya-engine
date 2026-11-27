// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's `levels\<level>\level.ai`, only the header and each level
//! vertex's position (docs/behavior/offline-simulation.md section 3). Not
//! chunked; read as a stream, keeping 5 of each vertex's 21 bytes (its
//! packed column and row and its height, as stored) in one block of bytes
//! the reader's caller provides ([`LevelAi::read_in`]): the engine's file
//! cache counts it against the file (docs/architecture/memory.md,
//! File-backed data).

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::DerefMut;

use crate::Bytes;
use crate::packet::Reader;

/// The only version read.
pub const VERSION: u32 = 8;
const HEADER_SIZE: usize = 56;
const VERTEX_SIZE: usize = 21;
/// Offset of the packed column and row in a vertex.
const VERTEX_PACKED: usize = 16;
/// Bytes kept of a vertex: the packed column and row (`u24`), the height
/// (`u16`), as stored.
pub const KEPT: usize = 5;
/// The largest packed column and row (section 5.2).
pub const PACKED_LIMIT: u32 = 0x7F_FFFF;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The header (section 3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelAiHeader {
    pub vertex_count: u32,
    /// Cell size *c*.
    pub cell_size: f32,
    /// Height range *h*.
    pub height_range: f32,
    pub box_min: [f32; 3],
    pub box_max: [f32; 3],
    pub identifier: [u8; 16],
}

impl LevelAiHeader {
    /// The row length *r* = floor((*z1* − *z0*) / *c* + 1.501).
    pub fn row_length(&self) -> u32 {
        ((self.box_max[2] - self.box_min[2]) / self.cell_size + 1.501).floor() as u32
    }

    /// The column count *s* = floor((*x1* − *x0*) / *c* + 1.501).
    pub fn column_count(&self) -> u32 {
        ((self.box_max[0] - self.box_min[0]) / self.cell_size + 1.501).floor() as u32
    }
}

/// The level vertices' positions.
pub struct LevelAi {
    pub header: LevelAiHeader,
    /// Per level vertex, [`KEPT`] bytes, little-endian: the packed column
    /// and row *p* (`u24`), the height *q* (`u16`).
    vertices: Box<dyn Bytes>,
}

impl fmt::Debug for LevelAi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LevelAi")
            .field("header", &self.header)
            .field("vertices", &self.len())
            .finish()
    }
}

impl PartialEq for LevelAi {
    fn eq(&self, other: &Self) -> bool {
        self.header == other.header && self.vertices[..] == other.vertices[..]
    }
}

impl LevelAi {
    /// The level vertices `vertices`, each (packed column and row, height),
    /// kept on the heap.
    pub fn new(header: LevelAiHeader, vertices: &[(u32, u16)]) -> Self {
        let mut bytes = Vec::with_capacity(vertices.len() * KEPT);
        for &(p, q) in vertices {
            bytes.extend(&p.to_le_bytes()[..3]);
            bytes.extend(q.to_le_bytes());
        }
        Self {
            header,
            vertices: Box::new(bytes),
        }
    }

    /// Reads `file` to its end, the positions kept on the heap
    /// ([`Self::read_in`]).
    pub fn read(file: impl Read + Seek) -> io::Result<Self> {
        Self::read_in(file, |len| Ok(vec![0; len]))
    }

    /// Reads `file` to its end, which must come right after the last
    /// vertex, keeping the positions in the zeroed bytes `alloc` gives for
    /// their size. A version other than 8 is an error.
    pub fn read_in<B: DerefMut<Target = [u8]> + Send + Sync + 'static>(
        mut file: impl Read + Seek,
        alloc: impl FnOnce(usize) -> io::Result<B>,
    ) -> io::Result<Self> {
        let length = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(0))?;
        let mut head = [0u8; HEADER_SIZE];
        file.read_exact(&mut head)?;
        let mut h = Reader::new(&head);
        let version = h.u32()?;
        if version != VERSION {
            return Err(invalid(format!("level.ai: version {version}")));
        }
        let header = LevelAiHeader {
            vertex_count: h.u32()?,
            cell_size: h.float()?,
            height_range: h.float()?,
            box_min: h.float3()?,
            box_max: h.float3()?,
            identifier: h.take(16)?.try_into().unwrap(),
        };
        // The vertices must fill the rest of the stream exactly before
        // anything is allocated for them.
        let expected = u64::from(header.vertex_count) * VERTEX_SIZE as u64 + HEADER_SIZE as u64;
        if expected != length {
            return Err(invalid(format!(
                "level.ai: {} vertices need {expected} bytes, the file has {length}",
                header.vertex_count
            )));
        }
        let n = header.vertex_count as usize;
        let mut kept = alloc(n * KEPT)?;
        if kept.len() != n * KEPT {
            return Err(invalid("level.ai: no room for the positions"));
        }
        // A batch of whole vertices at a time.
        let mut buffer = vec![0u8; VERTEX_SIZE * 4096];
        let mut out = kept.as_chunks_mut::<KEPT>().0.iter_mut();
        let mut left = n;
        while left > 0 {
            let batch = left.min(4096);
            let bytes = &mut buffer[..VERTEX_SIZE * batch];
            file.read_exact(bytes)?;
            for (v, to) in bytes.as_chunks::<VERTEX_SIZE>().0.iter().zip(&mut out) {
                to.copy_from_slice(&v[VERTEX_PACKED..]);
            }
            left -= batch;
        }
        Ok(Self {
            header,
            vertices: Box::new(kept),
        })
    }

    pub fn len(&self) -> usize {
        self.vertices.len() / KEPT
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    fn vertex(&self, k: u32) -> Option<&[u8; KEPT]> {
        self.vertices.as_chunks::<KEPT>().0.get(k as usize)
    }

    /// The packed column and row *p* of level vertex `k`.
    pub fn packed(&self, k: u32) -> Option<u32> {
        self.vertex(k)
            .map(|v| u32::from_le_bytes([v[0], v[1], v[2], 0]))
    }

    /// Every level vertex's packed column and row, in order.
    pub fn packed_all(&self) -> impl Iterator<Item = u32> + '_ {
        let chunks = self.vertices.as_chunks::<KEPT>().0;
        chunks
            .iter()
            .map(|v| u32::from_le_bytes([v[0], v[1], v[2], 0]))
    }

    /// The position of level vertex `k` (section 3):
    /// x = x0 + c · floor(p / r), z = z0 + c · (p mod r),
    /// y = y0 + q / 65535 · h.
    pub fn position(&self, k: u32) -> Option<[f32; 3]> {
        let v = self.vertex(k)?;
        let p = u32::from_le_bytes([v[0], v[1], v[2], 0]);
        let q = u16::from_le_bytes([v[3], v[4]]);
        let h = &self.header;
        let r = h.row_length().max(1);
        Some([
            h.box_min[0] + h.cell_size * (p / r) as f32,
            h.box_min[1] + f32::from(q) / 65535.0 * h.height_range,
            h.box_min[2] + h.cell_size * (p % r) as f32,
        ])
    }

    /// The packed column and row of the cell holding `position`, or `None`
    /// when it is off the level's grid (section 5.2): x or z more than
    /// *c*/2 outside the box, column floor((x − x0) / c + 0.5) not below
    /// *s*, row floor((z − z0) / c + 0.5) not below *r*, or column · r +
    /// row not below 0x7FFFFF.
    pub fn packed_at(&self, position: [f32; 3]) -> Option<u32> {
        let h = &self.header;
        let half = h.cell_size / 2.0;
        let [x, _, z] = position;
        if x < h.box_min[0] - half
            || x > h.box_max[0] + half
            || z < h.box_min[2] - half
            || z > h.box_max[2] + half
        {
            return None;
        }
        let column = ((x - h.box_min[0]) / h.cell_size + 0.5).floor().max(0.0) as u32;
        let row = ((z - h.box_min[2]) / h.cell_size + 0.5).floor().max(0.0) as u32;
        let r = h.row_length();
        if column >= h.column_count() || row >= r {
            return None;
        }
        let p = u64::from(column) * u64::from(r) + u64::from(row);
        (p < u64::from(PACKED_LIMIT)).then_some(p as u32)
    }

    /// Whether `position` lies in the cell of level vertex `k` (x and z
    /// only): the square of side *c* centred on its x and z (section 3).
    pub fn in_cell(&self, k: u32, position: [f32; 3]) -> bool {
        let Some(c) = self.position(k) else {
            return false;
        };
        let half = self.header.cell_size / 2.0;
        (position[0] - c[0]).abs() <= half && (position[2] - c[2]).abs() <= half
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A `level.ai` file: cell size 0.7, height range 10, box (0, 0, 0) …
    /// (`width`, 10, `depth`), vertices as (column, row, q).
    pub fn level_ai_file(width: f32, depth: f32, vertices: &[(u32, u32, u16)]) -> Vec<u8> {
        let mut file = Vec::new();
        file.extend(VERSION.to_le_bytes());
        file.extend((vertices.len() as u32).to_le_bytes());
        for f in [0.7f32, 10.0, 0.0, 0.0, 0.0, width, 10.0, depth] {
            file.extend(f.to_le_bytes());
        }
        file.extend([7; 16]);
        let r = ((depth / 0.7) + 1.501).floor() as u32;
        for (column, row, q) in vertices {
            let mut v = [0u8; VERTEX_SIZE];
            v[16..19].copy_from_slice(&(column * r + row).to_le_bytes()[..3]);
            v[19..21].copy_from_slice(&q.to_le_bytes());
            file.extend(v);
        }
        file
    }

    #[test]
    fn decodes_positions_and_cells() {
        let file = level_ai_file(7.0, 3.5, &[(0, 0, 0), (2, 3, 65535)]);
        let ai = LevelAi::read(io::Cursor::new(&file[..])).unwrap();
        assert_eq!(ai.header.row_length(), 6);
        assert_eq!(ai.header.column_count(), 11);
        assert_eq!(ai.packed(1), Some(2 * 6 + 3));
        assert_eq!(ai.packed_all().collect::<Vec<_>>(), [0, 2 * 6 + 3]);
        assert_eq!(ai, LevelAi::new(ai.header, &[(0, 0), (2 * 6 + 3, 65535)]));
        let p = ai.position(1).unwrap();
        assert!((p[0] - 1.4).abs() < 1e-6 && (p[1] - 10.0).abs() < 1e-6);
        assert!((p[2] - 2.1).abs() < 1e-6);
        assert_eq!(ai.position(2), None);
        assert!(ai.in_cell(1, [1.7, 0.0, 2.3]));
        assert!(!ai.in_cell(1, [1.8, 0.0, 2.1]));
        assert_eq!(ai.packed_at([1.5, 0.0, 2.0]), Some(2 * 6 + 3));
        assert_eq!(ai.packed_at([-0.3, 0.0, 0.0]), Some(0));
        assert_eq!(ai.packed_at([-0.4, 0.0, 0.0]), None);
        assert_eq!(ai.packed_at([7.3, 0.0, 0.0]), Some(10 * 6));
        assert!(LevelAi::read(io::Cursor::new(&file[..file.len() - 1])).is_err());
        let mut longer = file.clone();
        longer.push(0);
        assert!(LevelAi::read(io::Cursor::new(&longer[..])).is_err());
        let mut huge = file.clone();
        huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(LevelAi::read(io::Cursor::new(&huge[..])).is_err());
    }
}
