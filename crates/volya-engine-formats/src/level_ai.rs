// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's `levels\<level>\level.ai`, only the header and each level
//! vertex's position (docs/behavior/offline-simulation.md section 3). Not
//! chunked; read as a stream, keeping 6 of each vertex's 21 bytes.

use std::io::{self, Read};

use crate::packet::Reader;

/// The only version read.
pub const VERSION: u32 = 8;
const HEADER_SIZE: usize = 56;
const VERTEX_SIZE: usize = 21;
/// Offset of the packed column and row in a vertex.
const VERTEX_PACKED: usize = 16;
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
#[derive(Clone, Debug, PartialEq)]
pub struct LevelAi {
    pub header: LevelAiHeader,
    /// Per level vertex: the packed column and row *p* (`u24`).
    pub packed: Vec<u32>,
    /// Per level vertex: the height *q*.
    pub heights: Vec<u16>,
}

impl LevelAi {
    /// Reads `file` to its end, which must come right after the last
    /// vertex. A version other than 8 is an error.
    pub fn read(mut file: impl Read) -> io::Result<Self> {
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
        let n = header.vertex_count as usize;
        let mut packed = Vec::with_capacity(n);
        let mut heights = Vec::with_capacity(n);
        // A batch of whole vertices at a time.
        let mut buffer = vec![0u8; VERTEX_SIZE * 4096];
        let mut left = n;
        while left > 0 {
            let batch = left.min(4096);
            let bytes = &mut buffer[..VERTEX_SIZE * batch];
            file.read_exact(bytes)?;
            for v in bytes.as_chunks::<VERTEX_SIZE>().0 {
                let p = &v[VERTEX_PACKED..];
                packed.push(u32::from_le_bytes([p[0], p[1], p[2], 0]));
                heights.push(u16::from_le_bytes([p[3], p[4]]));
            }
            left -= batch;
        }
        if file.read(&mut [0u8; 1])? != 0 {
            return Err(invalid("level.ai: bytes after the last vertex"));
        }
        Ok(Self {
            header,
            packed,
            heights,
        })
    }

    pub fn len(&self) -> usize {
        self.packed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packed.is_empty()
    }

    /// The position of level vertex `k` (section 3):
    /// x = x0 + c · floor(p / r), z = z0 + c · (p mod r),
    /// y = y0 + q / 65535 · h.
    pub fn position(&self, k: u32) -> Option<[f32; 3]> {
        let p = *self.packed.get(k as usize)?;
        let q = self.heights[k as usize];
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
        let ai = LevelAi::read(&file[..]).unwrap();
        assert_eq!(ai.header.row_length(), 6);
        assert_eq!(ai.header.column_count(), 11);
        let p = ai.position(1).unwrap();
        assert!((p[0] - 1.4).abs() < 1e-6 && (p[1] - 10.0).abs() < 1e-6);
        assert!((p[2] - 2.1).abs() < 1e-6);
        assert!(ai.in_cell(1, [1.7, 0.0, 2.3]));
        assert!(!ai.in_cell(1, [1.8, 0.0, 2.1]));
        assert_eq!(ai.packed_at([1.5, 0.0, 2.0]), Some(2 * 6 + 3));
        assert_eq!(ai.packed_at([-0.3, 0.0, 0.0]), Some(0));
        assert_eq!(ai.packed_at([-0.4, 0.0, 0.0]), None);
        assert_eq!(ai.packed_at([7.3, 0.0, 0.0]), Some(10 * 6));
        assert!(LevelAi::read(&file[..file.len() - 1]).is_err());
        let mut longer = file.clone();
        longer.push(0);
        assert!(LevelAi::read(&longer[..]).is_err());
    }
}
