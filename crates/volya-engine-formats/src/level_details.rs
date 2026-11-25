// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `levels\<level>\level.details` (docs/formats/level-details.md): the
//! detail models and the grid of 2 × 2 m slots saying which grow where.
//! The slot grid is kept as stored (16 bytes a slot) and decoded on lookup.

use std::io::{self, Cursor, Read, Seek};

use crate::chunks::{self, ChunkHeader, Chunks};
use crate::cp1251;

/// The only version the game accepts (section 2.1).
pub const VERSION: u32 = 3;
/// The header's size (section 2.1).
pub const HEADER_SIZE: usize = 24;
/// Bytes of one slot record (section 4.2).
pub const SLOT_SIZE: usize = 16;
/// A slot's model id meaning "no part" (section 4.2).
pub const NO_MODEL: u8 = 63;
/// Entries of the game's model table (section 3): ids 0 … 62 are usable.
pub const MAX_MODELS: u32 = 64;
/// The side of a slot, metres (section 4.1).
pub const SLOT_METRES: f32 = 2.0;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The header (chunk 0, section 2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u32,
    pub model_count: u32,
    /// Grid column of world cell x = 0, and row of world cell z = 0.
    pub offset_x: i32,
    pub offset_z: i32,
    pub size_x: u32,
    pub size_z: u32,
}

/// One vertex of a model (section 3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// One model (a sub-chunk of chunk 1, section 3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct DetailModel {
    /// The engine shader and texture list, as an LV 3 entry's parts.
    pub shader: String,
    pub texture: String,
    pub flags: u32,
    pub min_scale: f32,
    pub max_scale: f32,
    pub vertices: Vec<DetailVertex>,
    /// An indexed triangle list.
    pub indices: Vec<u16>,
}

impl DetailModel {
    /// Flags bit 0: the model never sways (section 3.2).
    pub fn still(&self) -> bool {
        self.flags & 1 != 0
    }

    /// The vertex bounds: minimum and maximum corner.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for v in &self.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v.position[k]);
                hi[k] = hi[k].max(v.position[k]);
            }
        }
        (lo, hi)
    }
}

/// A slot record (section 4.2): four 32-bit words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Slot(pub [u32; 4]);

impl Slot {
    /// A slot whose four parts are none: a cell outside the grid (4.1).
    pub const NONE: Self = Self([63 << 20 | 63 << 26, 63 | 63 << 6, 0, 0]);

    /// w0 bits 0–11.
    pub fn base(&self) -> u32 {
        self.0[0] & 0xFFF
    }

    /// w0 bits 12–19.
    pub fn height(&self) -> u32 {
        (self.0[0] >> 12) & 0xFF
    }

    /// The lowest y of the slot: base × 0.2 − 200 m.
    pub fn y_low(&self) -> f32 {
        self.base() as f32 * 0.2 - 200.0
    }

    /// The top y of the slot: lowest y + height × 0.1 m.
    pub fn y_top(&self) -> f32 {
        self.y_low() + self.height() as f32 * 0.1
    }

    /// The model ids of parts 0 … 3 (63: no part).
    pub fn ids(&self) -> [u8; 4] {
        let [w0, w1, ..] = self.0;
        [
            ((w0 >> 20) & 63) as u8,
            ((w0 >> 26) & 63) as u8,
            (w1 & 63) as u8,
            ((w1 >> 6) & 63) as u8,
        ]
    }

    /// Whether all four parts are none (section 4.2).
    pub fn is_empty(&self) -> bool {
        self.ids() == [NO_MODEL; 4]
    }

    /// w1 bits 12–15: the sun factor, 0 … 15.
    pub fn sun(&self) -> u32 {
        (self.0[1] >> 12) & 15
    }

    /// w1 bits 16–19: the hemisphere factor, 0 … 15.
    pub fn hemi(&self) -> u32 {
        (self.0[1] >> 16) & 15
    }

    /// w1 bits 20–31: three 4-bit values read but not used for drawing
    /// (section 4.4).
    pub fn unused(&self) -> [u32; 3] {
        let w1 = self.0[1];
        [(w1 >> 20) & 15, (w1 >> 24) & 15, (w1 >> 28) & 15]
    }

    /// The densities d0 … d3 of `part` at the slot's corners (section 4.3:
    /// smallest x and z, largest x, largest z, both largest), 0 … 15.
    pub fn densities(&self, part: usize) -> [u8; 4] {
        let word = self.0[2 + part / 2] >> (16 * (part % 2));
        std::array::from_fn(|k| ((word >> (4 * k)) & 15) as u8)
    }
}

/// A parsed `level.details`.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelDetails {
    pub header: Header,
    pub models: Vec<DetailModel>,
    /// Chunk 2 as stored: size_x × size_z records, row by row.
    slots: Vec<u8>,
}

/// Reads a zero-terminated string of at most 255 characters.
fn string(data: &[u8], at: &mut usize) -> io::Result<String> {
    let rest = &data[*at..];
    let end = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| invalid("level.details: unterminated string"))?;
    if end > 255 {
        return Err(invalid("level.details: string longer than 255 characters"));
    }
    *at += end + 1;
    Ok(cp1251::decode(&rest[..end]))
}

fn take<'a>(data: &'a [u8], at: &mut usize, n: usize) -> io::Result<&'a [u8]> {
    let end = at
        .checked_add(n)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| invalid("level.details: model record past its chunk"))?;
    let out = &data[*at..end];
    *at = end;
    Ok(out)
}

fn u32_at(data: &[u8], at: &mut usize) -> io::Result<u32> {
    Ok(u32::from_le_bytes(take(data, at, 4)?.try_into().unwrap()))
}

fn f32_at(data: &[u8], at: &mut usize) -> io::Result<f32> {
    Ok(f32::from_bits(u32_at(data, at)?))
}

impl DetailModel {
    /// One model record (section 3.1). An index count not a multiple of 3
    /// is the game's assertion failure. An index not below the vertex count
    /// and a record shorter than its fields are errors; bytes after the
    /// record are ignored (neither is established: review gaps).
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let mut at = 0;
        let shader = string(data, &mut at)?;
        let texture = string(data, &mut at)?;
        let flags = u32_at(data, &mut at)?;
        let min_scale = f32_at(data, &mut at)?;
        let max_scale = f32_at(data, &mut at)?;
        let vertex_count = u32_at(data, &mut at)? as usize;
        let index_count = u32_at(data, &mut at)? as usize;
        if !index_count.is_multiple_of(3) {
            return Err(invalid(format!(
                "level.details: index count {index_count} is not a multiple of 3"
            )));
        }
        let raw = take(data, &mut at, vertex_count.saturating_mul(20))?;
        let vertices = raw
            .as_chunks::<20>()
            .0
            .iter()
            .map(|v| {
                let f = |i: usize| f32::from_le_bytes(v[4 * i..4 * i + 4].try_into().unwrap());
                DetailVertex {
                    position: [f(0), f(1), f(2)],
                    uv: [f(3), f(4)],
                }
            })
            .collect();
        let raw = take(data, &mut at, index_count.saturating_mul(2))?;
        let indices: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        if let Some(&i) = indices.iter().find(|&&i| usize::from(i) >= vertex_count) {
            return Err(invalid(format!(
                "level.details: index {i} of {vertex_count} vertices"
            )));
        }
        Ok(Self {
            shader,
            texture,
            flags,
            min_scale,
            max_scale,
            vertices,
            indices,
        })
    }
}

impl Header {
    /// Chunk 0 (section 2.1): 24 bytes, version 3 (otherwise the game's
    /// assertion failure); a model count above the table's 64 entries is
    /// unestablished, an error here.
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() != HEADER_SIZE {
            return Err(invalid(format!(
                "level.details: header of {} bytes",
                data.len()
            )));
        }
        let w = |i: usize| u32::from_le_bytes(data[4 * i..4 * i + 4].try_into().unwrap());
        let header = Self {
            version: w(0),
            model_count: w(1),
            offset_x: w(2) as i32,
            offset_z: w(3) as i32,
            size_x: w(4),
            size_z: w(5),
        };
        if header.version != VERSION {
            return Err(invalid(format!(
                "level.details: version {}",
                header.version
            )));
        }
        if header.model_count > MAX_MODELS {
            return Err(invalid(format!(
                "level.details: {} models",
                header.model_count
            )));
        }
        Ok(header)
    }

    /// Bytes of the slot grid.
    fn grid_bytes(&self) -> usize {
        self.size_x as usize * self.size_z as usize * SLOT_SIZE
    }
}

/// Models `count` of chunk 1 (section 3): sub-chunk *i* for model *i*; a
/// missing one is unestablished, an error here.
fn models(chunk1: &[u8], count: u32) -> io::Result<Vec<DetailModel>> {
    let sub = Chunks::parse(chunk1)?;
    (0..count)
        .map(|i| {
            let c = sub.require(i, "level.details model")?;
            DetailModel::parse(&c.content()?)
        })
        .collect()
}

fn require(headers: &[ChunkHeader], id: u32) -> io::Result<&ChunkHeader> {
    chunks::find_header(headers, id)
        .ok_or_else(|| invalid(format!("level.details: chunk {id} missing")))
}

impl LevelDetails {
    /// Reads the file from a stream, holding at most its slot grid and its
    /// models at once. A missing chunk is unestablished (a review gap), an
    /// error here; so is a slot grid shorter than size_x × size_z records
    /// (section 2.1: a malformed file). A longer one's extra bytes are
    /// never read.
    pub fn read(reader: &mut (impl Read + Seek)) -> io::Result<Self> {
        let headers = chunks::index(reader)?;
        let header = Header::parse(&require(&headers, 0)?.read_content(reader)?)?;
        let models = models(
            &require(&headers, 1)?.read_content(reader)?,
            header.model_count,
        )?;
        let grid = require(&headers, 2)?;
        let need = header.grid_bytes();
        if (grid.size as usize) < need {
            return Err(invalid(format!(
                "level.details: slot grid of {} bytes for {} × {} slots",
                grid.size, header.size_x, header.size_z
            )));
        }
        let mut slots = grid.read_content(reader)?;
        slots.truncate(need);
        Ok(Self {
            header,
            models,
            slots,
        })
    }

    /// The file from its bytes.
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        Self::read(&mut Cursor::new(bytes))
    }

    /// Record (`ix`, `iz`) of the grid (section 4).
    pub fn record(&self, ix: u32, iz: u32) -> Slot {
        let at = (iz as usize * self.header.size_x as usize + ix as usize) * SLOT_SIZE;
        let w = |k: usize| {
            u32::from_le_bytes(self.slots[at + 4 * k..at + 4 * k + 4].try_into().unwrap())
        };
        Slot([w(0), w(1), w(2), w(3)])
    }

    /// The slot of world cell (`cx`, `cz`), covering x 2cx … 2cx + 2 and
    /// z 2cz … 2cz + 2 (section 4.1); outside the grid, one with no part.
    pub fn slot(&self, cx: i32, cz: i32) -> Slot {
        let h = &self.header;
        let ix = i64::from(cx) + i64::from(h.offset_x);
        let iz = i64::from(cz) + i64::from(h.offset_z);
        if ix < 0 || iz < 0 || ix >= i64::from(h.size_x) || iz >= i64::from(h.size_z) {
            return Slot::NONE;
        }
        self.record(ix as u32, iz as u32)
    }

    /// Every record, row by row.
    pub fn records(&self) -> impl Iterator<Item = Slot> + '_ {
        self.slots.as_chunks::<SLOT_SIZE>().0.iter().map(|r| {
            let w = |k: usize| u32::from_le_bytes(r[4 * k..4 * k + 4].try_into().unwrap());
            Slot([w(0), w(1), w(2), w(3)])
        })
    }

    /// Heap bytes held.
    pub fn memory_bytes(&self) -> usize {
        self.slots.capacity()
            + self
                .models
                .iter()
                .map(|m| m.vertices.capacity() * 20 + m.indices.capacity() * 2)
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::chunk;

    fn model_record(flags: u32, vertices: &[[f32; 5]], indices: &[u16]) -> Vec<u8> {
        let mut out = b"details\\blend\0build_details\0".to_vec();
        out.extend(flags.to_le_bytes());
        out.extend(0.8f32.to_le_bytes());
        out.extend(1.3f32.to_le_bytes());
        out.extend((vertices.len() as u32).to_le_bytes());
        out.extend((indices.len() as u32).to_le_bytes());
        for v in vertices {
            for f in v {
                out.extend(f.to_le_bytes());
            }
        }
        for i in indices {
            out.extend(i.to_le_bytes());
        }
        out
    }

    fn header(version: u32, models: u32, offset: [i32; 2], size: [u32; 2]) -> Vec<u8> {
        [
            version,
            models,
            offset[0] as u32,
            offset[1] as u32,
            size[0],
            size[1],
        ]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect()
    }

    /// A 2 × 1 grid in the stored order 1, 2, 0.
    fn file(version: u32, grid: &[[u32; 4]]) -> Vec<u8> {
        let quad = [
            [0.0, 0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0, 1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0, 1.0, 0.0],
        ];
        let models = chunk(0, &model_record(1, &quad, &[0, 1, 2, 2, 1, 3]));
        let slots: Vec<u8> = grid
            .iter()
            .flatten()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        let mut out = chunk(1, &models);
        out.extend(chunk(2, &slots));
        out.extend(chunk(0, &header(version, 1, [-1, 0], [2, 1])));
        out
    }

    #[test]
    fn a_file_and_its_slots() {
        // base 1000, height 50, ids 0, 63, 63, 5, sun 8, hemi 13, the
        // unused values 1, 2, 3; part 0's densities 1, 2, 3, 15.
        let w0 = 1000 | 50 << 12 | 63 << 26; // part 0's id 0
        let w1 = 63 | 5 << 6 | 8 << 12 | 13 << 16 | 1 << 20 | 2 << 24 | 3 << 28;
        let w2 = 1 | 2 << 4 | 3 << 8 | 15 << 12 | 7 << 16;
        let w3 = 0xF00F_0000;
        let d = LevelDetails::parse(&file(3, &[Slot::NONE.0, [w0, w1, w2, w3]])).unwrap();
        assert_eq!(
            d.header,
            Header {
                version: 3,
                model_count: 1,
                offset_x: -1,
                offset_z: 0,
                size_x: 2,
                size_z: 1,
            }
        );
        let m = &d.models[0];
        assert_eq!(
            (m.shader.as_str(), m.texture.as_str()),
            ("details\\blend", "build_details")
        );
        assert!(m.still());
        assert_eq!(m.vertices[3].uv, [1.0, 0.0]);
        assert_eq!(m.bounds(), ([0.0; 3], [1.0, 0.0, 1.0]));
        // Record (1, 0) is world cell (1 − (−1), 0) = (2, 0).
        let s = d.slot(2, 0);
        assert_eq!(s, d.record(1, 0));
        assert_eq!((s.base(), s.height()), (1000, 50));
        assert!((s.y_low() - 0.0).abs() < 1e-4 && (s.y_top() - 5.0).abs() < 1e-4);
        assert_eq!(s.ids(), [0, 63, 63, 5]);
        assert_eq!((s.sun(), s.hemi(), s.unused()), (8, 13, [1, 2, 3]));
        assert_eq!(s.densities(0), [1, 2, 3, 15]);
        assert_eq!(s.densities(1), [7, 0, 0, 0]);
        assert_eq!(s.densities(3), [15, 0, 0, 15]);
        assert!(!s.is_empty());
        assert!(d.slot(1, 0).is_empty());
        // Outside the grid: no part.
        assert_eq!(d.slot(3, 0), Slot::NONE);
        assert_eq!(d.slot(0, 0), Slot::NONE);
        assert_eq!(d.slot(2, -1), Slot::NONE);
    }

    #[test]
    fn what_the_game_refuses() {
        // Another version: an assertion failure.
        assert!(LevelDetails::parse(&file(2, &[[0; 4]; 2])).is_err());
        // A grid shorter than its records.
        assert!(LevelDetails::parse(&file(3, &[[0; 4]])).is_err());
        // An index count not a multiple of 3.
        let bad = model_record(0, &[[0.0; 5]; 3], &[0, 1]);
        assert!(DetailModel::parse(&bad).is_err());
        let ok = model_record(0, &[[0.0; 5]; 3], &[0, 1, 2]);
        assert_eq!(DetailModel::parse(&ok).unwrap().indices, [0, 1, 2]);
    }
}
