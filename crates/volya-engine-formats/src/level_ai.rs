// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's `levels\<level>\level.ai`, only the header and each level
//! vertex's position (docs/behavior/offline-simulation.md section 3), and
//! on request its links and cover values
//! (docs/behavior/script-api-level.md 3.1, [`LevelAiLinks`]). Not
//! chunked; read as a stream, keeping 5 of each vertex's 21 bytes (its
//! packed column and row and its height, as stored) in one block of bytes
//! the reader's caller provides ([`LevelAi::read_in`]), and the links and
//! cover values (14 more) in another: the engine's file cache counts them
//! against the file (docs/architecture/memory.md, File-backed data).

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
/// Offset of the byte whose upper 4 bits are the vertex's light
/// (script-api-game-object-basics.md 4).
const VERTEX_LIGHT: usize = 11;
/// Bytes kept of a vertex: the packed column and row (`u24`), the height
/// (`u16`), as stored.
pub const KEPT: usize = 5;
/// The largest packed column and row (section 5.2).
pub const PACKED_LIMIT: u32 = 0x7F_FFFF;
/// Bytes kept of a vertex for [`LevelAiLinks`]: its four links (bytes
/// 0–11) and its cover values (12–13), as stored.
pub const LINKS_KEPT: usize = 14;
/// A link's value meaning none (script-api-level.md 3.1).
pub const NO_LINK: u32 = 0x7F_FFFF;

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
        file: impl Read + Seek,
        alloc: impl FnOnce(usize) -> io::Result<B>,
    ) -> io::Result<Self> {
        Self::read_with(file, alloc, None, NO_LINKS).map(|(ai, _)| ai)
    }

    /// [`Self::read_in`], also giving byte 11 of each vertex in order (the
    /// light the scripts read, script-api-game-object-basics.md 4,
    /// `level_vertex_light`), one byte per vertex on the heap.
    pub fn read_in_with_lights<B: DerefMut<Target = [u8]> + Send + Sync + 'static>(
        file: impl Read + Seek,
        alloc: impl FnOnce(usize) -> io::Result<B>,
    ) -> io::Result<(Self, Vec<u8>)> {
        let mut lights = Vec::new();
        let (ai, _) = Self::read_with(file, alloc, Some(&mut lights), NO_LINKS)?;
        Ok((ai, lights))
    }

    /// [`Self::read_in_with_lights`], also keeping each vertex's links and
    /// cover values in the zeroed bytes `alloc_links` gives for their size
    /// ([`LevelAiLinks`]).
    pub fn read_in_with_lights_and_links<
        B: DerefMut<Target = [u8]> + Send + Sync + 'static,
        L: DerefMut<Target = [u8]> + Send + Sync + 'static,
    >(
        file: impl Read + Seek,
        alloc: impl FnOnce(usize) -> io::Result<B>,
        alloc_links: impl FnOnce(usize) -> io::Result<L>,
    ) -> io::Result<(Self, Vec<u8>, LevelAiLinks)> {
        let mut lights = Vec::new();
        let (ai, links) = Self::read_with(file, alloc, Some(&mut lights), Some(alloc_links))?;
        Ok((ai, lights, links.expect("asked for")))
    }

    fn read_with<
        B: DerefMut<Target = [u8]> + Send + Sync + 'static,
        L: DerefMut<Target = [u8]> + Send + Sync + 'static,
    >(
        mut file: impl Read + Seek,
        alloc: impl FnOnce(usize) -> io::Result<B>,
        mut lights: Option<&mut Vec<u8>>,
        alloc_links: Option<impl FnOnce(usize) -> io::Result<L>>,
    ) -> io::Result<(Self, Option<LevelAiLinks>)> {
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
        if let Some(lights) = lights.as_deref_mut() {
            lights.reserve_exact(n);
        }
        let mut links = match alloc_links {
            Some(alloc) => {
                let links = alloc(n * LINKS_KEPT)?;
                if links.len() != n * LINKS_KEPT {
                    return Err(invalid("level.ai: no room for the links"));
                }
                Some(links)
            }
            None => None,
        };
        let mut links_out =
            (links.as_deref_mut()).map(|l| l.as_chunks_mut::<LINKS_KEPT>().0.iter_mut());
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
            if let Some(out) = links_out.as_mut() {
                for (v, to) in bytes.as_chunks::<VERTEX_SIZE>().0.iter().zip(&mut *out) {
                    to.copy_from_slice(&v[..LINKS_KEPT]);
                }
            }
            if let Some(lights) = lights.as_deref_mut() {
                lights.extend(
                    bytes
                        .as_chunks::<VERTEX_SIZE>()
                        .0
                        .iter()
                        .map(|v| v[VERTEX_LIGHT]),
                );
            }
            left -= batch;
        }
        let links = links.map(|l| LevelAiLinks { bytes: Box::new(l) });
        let ai = Self {
            header,
            vertices: Box::new(kept),
        };
        Ok((ai, links))
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

/// No links asked for ([`LevelAi::read_with`]).
const NO_LINKS: Option<HeapLinks> = None;

/// Links kept on the heap.
type HeapLinks = fn(usize) -> io::Result<Vec<u8>>;

/// Each level vertex's links and cover values (script-api-level.md 3.1):
/// bytes 0–13 of its record, as stored.
pub struct LevelAiLinks {
    bytes: Box<dyn Bytes>,
}

impl fmt::Debug for LevelAiLinks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LevelAiLinks")
            .field("vertices", &self.len())
            .finish()
    }
}

impl LevelAiLinks {
    /// The vertices' (links, cover values), kept on the heap: links 0 to
    /// 3 (−x, +z, +x, −z; [`NO_LINK`] for none), cover values *c*0 to
    /// *c*3 (each 0 … 15).
    pub fn new(vertices: &[([u32; 4], [u8; 4])]) -> Self {
        let mut bytes = Vec::with_capacity(vertices.len() * LINKS_KEPT);
        for (links, cover) in vertices {
            let packed = (links.iter().enumerate()).fold(0u128, |p, (i, &l)| {
                p | (u128::from(l & NO_LINK) << (23 * i))
            });
            bytes.extend(&packed.to_le_bytes()[..12]);
            let c = (cover.iter().enumerate())
                .fold(0u16, |c, (i, &v)| c | (u16::from(v & 15) << (4 * i)));
            bytes.extend(c.to_le_bytes());
        }
        Self {
            bytes: Box::new(bytes),
        }
    }

    pub fn len(&self) -> usize {
        self.bytes.len() / LINKS_KEPT
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn vertex(&self, k: u32) -> Option<&[u8; LINKS_KEPT]> {
        self.bytes.as_chunks::<LINKS_KEPT>().0.get(k as usize)
    }

    /// Level vertex `k`'s four links, 23 bits each at bits 0, 23, 46 and
    /// 69 of bytes 0–11: −x, +z, +x, −z; [`NO_LINK`] for none.
    pub fn links(&self, k: u32) -> Option<[u32; 4]> {
        let v = self.vertex(k)?;
        let mut b = [0u8; 16];
        b[..12].copy_from_slice(&v[..12]);
        let packed = u128::from_le_bytes(b);
        Some(std::array::from_fn(|i| {
            ((packed >> (23 * i)) as u32) & NO_LINK
        }))
    }

    /// Level vertex `k`'s cover values *c*0 to *c*3: bits 0–3, 4–7, 8–11
    /// and 12–15 of the little-endian 16-bit value at bytes 12–13.
    pub fn cover(&self, k: u32) -> Option<[u8; 4]> {
        let v = self.vertex(k)?;
        let c = u16::from_le_bytes([v[12], v[13]]);
        Some(std::array::from_fn(|i| ((c >> (4 * i)) & 15) as u8))
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

    #[test]
    fn gives_each_vertexs_light_byte() {
        let mut file = level_ai_file(7.0, 3.5, &[(0, 0, 0), (2, 3, 65535)]);
        file[HEADER_SIZE + VERTEX_LIGHT] = 0xF3;
        file[HEADER_SIZE + VERTEX_SIZE + VERTEX_LIGHT] = 0x70;
        let (ai, lights) =
            LevelAi::read_in_with_lights(io::Cursor::new(&file[..]), |len| Ok(vec![0; len]))
                .unwrap();
        assert_eq!(lights, [0xF3, 0x70]);
        assert_eq!(ai, LevelAi::read(io::Cursor::new(&file[..])).unwrap());
    }

    #[test]
    fn gives_each_vertexs_links_and_cover() {
        let mut file = level_ai_file(7.0, 3.5, &[(0, 0, 0), (2, 3, 65535)]);
        // Vertex 0 of `l01_escape` (script-api-level.md 8.1), bytes 0-13.
        let record = [
            0xff, 0xff, 0xff, 0x00, 0x00, 0xc0, 0x05, 0x00, 0xe0, 0xff, 0xff, 0x0f, 0x93, 0x7d,
        ];
        file[HEADER_SIZE + VERTEX_SIZE..][..LINKS_KEPT].copy_from_slice(&record);
        let (ai, lights, links) = LevelAi::read_in_with_lights_and_links(
            io::Cursor::new(&file[..]),
            |len| Ok(vec![0; len]),
            |len| Ok(vec![0; len]),
        )
        .unwrap();
        assert_eq!(ai, LevelAi::read(io::Cursor::new(&file[..])).unwrap());
        assert_eq!(lights, [0, 0x0f]);
        assert_eq!(links.len(), 2);
        assert_eq!(links.links(1), Some([NO_LINK, 1, 23, NO_LINK]));
        assert_eq!(links.cover(1), Some([3, 9, 13, 7]));
        assert_eq!(links.links(0), Some([0; 4]));
        assert_eq!(links.links(2), None);
        let made =
            LevelAiLinks::new(&[([0; 4], [0; 4]), ([NO_LINK, 1, 23, NO_LINK], [3, 9, 13, 7])]);
        assert_eq!(made.bytes[LINKS_KEPT..], record[..]);
    }
}
