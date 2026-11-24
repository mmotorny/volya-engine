// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Static level geometry, `level.geom` and `level.geomx`
//! (docs/formats/level-geom.md): vertex buffers with their declarations,
//! index buffers, progressive-mesh records, and the decoding of the packed
//! vertex attributes into floats.
//!
//! Buffers keep their stored bytes ([`VertexBuffer::data`]): every element
//! type maps to a GPU vertex format (FLOAT3 to three floats, D3DCOLOR to four
//! normalized bytes with bytes 0 and 2 swapped, SHORT2/SHORT4 to signed
//! 16-bit integers), so a renderer may upload them packed and decode in its
//! vertex program as section 8 does. [`VertexBuffer::decode`] gives the same
//! values on the CPU.

use std::io::{self, Read, Seek};

use crate::chunks::{self, Chunks};

/// Chunk ids (section 2).
pub const HEADER_CHUNK: u32 = 1;
pub const VERTEX_BUFFERS_CHUNK: u32 = 9;
pub const INDEX_BUFFERS_CHUNK: u32 = 10;
pub const SLIDE_WINDOWS_CHUNK: u32 = 11;

/// Element types (section 4).
pub const FLOAT3: u8 = 2;
pub const D3DCOLOR: u8 = 4;
pub const SHORT2: u8 = 6;
pub const SHORT4: u8 = 7;

/// Element usages (section 4).
pub const POSITION: u8 = 0;
pub const NORMAL: u8 = 3;
pub const TEXCOORD: u8 = 5;
pub const TANGENT: u8 = 6;
pub const BINORMAL: u8 = 7;
pub const COLOR: u8 = 10;

/// Stream value of the declaration's end marker.
const END_STREAM: u16 = 0xFF;
/// Elements a declaration may have before its end marker (section 3).
const MAX_ELEMENTS: usize = 64;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A little-endian reader over a byte slice that fails on overrun.
struct Bytes<'a> {
    data: &'a [u8],
    what: &'static str,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if n > self.data.len() {
            return Err(invalid(format!(
                "{}: needs {n} bytes, {} remain",
                self.what,
                self.data.len()
            )));
        }
        let (head, rest) = self.data.split_at(n);
        self.data = rest;
        Ok(head)
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    /// `count × size` bytes, failing before allocating for a count the data
    /// cannot hold.
    fn array(&mut self, count: u32, size: usize) -> io::Result<&'a [u8]> {
        let n = (count as usize)
            .checked_mul(size)
            .ok_or_else(|| invalid(format!("{}: count {count} too large", self.what)))?;
        self.take(n)
    }

    fn finish(&self) -> io::Result<()> {
        if self.data.is_empty() {
            Ok(())
        } else {
            Err(invalid(format!(
                "{}: {} bytes left over",
                self.what,
                self.data.len()
            )))
        }
    }
}

/// One vertex declaration element (section 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Element {
    pub stream: u16,
    pub offset: u16,
    pub kind: u8,
    pub method: u8,
    pub usage: u8,
    pub usage_index: u8,
}

impl Element {
    const fn new(offset: u16, kind: u8, usage: u8, usage_index: u8) -> Self {
        Self {
            stream: 0,
            offset,
            kind,
            method: 0,
            usage,
            usage_index,
        }
    }

    /// Stored size of the element's type; `None` for types the spec does not
    /// describe.
    pub fn size(&self) -> Option<usize> {
        match self.kind {
            FLOAT3 => Some(12),
            D3DCOLOR | SHORT2 => Some(4),
            SHORT4 => Some(8),
            _ => None,
        }
    }

    /// The 4-component value a vertex program receives for this element of
    /// `vertex` (section 4's table).
    pub fn program_input(&self, vertex: &[u8]) -> [f32; 4] {
        let at = &vertex[usize::from(self.offset)..];
        let f32_at = |i: usize| f32::from_le_bytes(at[4 * i..4 * i + 4].try_into().unwrap());
        let i16_at = |i: usize| f32::from(i16::from_le_bytes([at[2 * i], at[2 * i + 1]]));
        match self.kind {
            FLOAT3 => [f32_at(0), f32_at(1), f32_at(2), 1.0],
            D3DCOLOR => [at[2], at[1], at[0], at[3]].map(|b| f32::from(b) / 255.0),
            SHORT2 => [i16_at(0), i16_at(1), 0.0, 1.0],
            SHORT4 => [i16_at(0), i16_at(1), i16_at(2), i16_at(3)],
            _ => unreachable!("declarations hold only types with a size"),
        }
    }
}

/// The declarations the shipped files use (section 4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Trees: position, normal, tangent, binormal, SHORT4 texcoord.
    Tree,
    /// Vertex-lit: position, normal, tangent, binormal, colour, SHORT2
    /// texcoord.
    VertexLit,
    /// Light-mapped: position, normal, tangent, binormal, two SHORT2
    /// texcoords.
    LightMapped,
    /// `level.geomx`: position only.
    Position,
}

const COMMON: [Element; 4] = [
    Element::new(0, FLOAT3, POSITION, 0),
    Element::new(12, D3DCOLOR, NORMAL, 0),
    Element::new(16, D3DCOLOR, TANGENT, 0),
    Element::new(20, D3DCOLOR, BINORMAL, 0),
];

impl Layout {
    /// The exact element list of each layout, end marker excluded.
    pub fn elements(self) -> Vec<Element> {
        let rest: &[Element] = match self {
            Self::Tree => &[Element::new(24, SHORT4, TEXCOORD, 0)],
            Self::VertexLit => &[
                Element::new(24, D3DCOLOR, COLOR, 0),
                Element::new(28, SHORT2, TEXCOORD, 0),
            ],
            Self::LightMapped => &[
                Element::new(24, SHORT2, TEXCOORD, 0),
                Element::new(28, SHORT2, TEXCOORD, 1),
            ],
            Self::Position => return vec![COMMON[0]],
        };
        COMMON.iter().chain(rest).copied().collect()
    }

    /// The layout whose element list is exactly `elements`.
    pub fn recognize(elements: &[Element]) -> Option<Self> {
        [
            Self::Tree,
            Self::VertexLit,
            Self::LightMapped,
            Self::Position,
        ]
        .into_iter()
        .find(|l| l.elements() == elements)
    }
}

// D3DCOLOR helpers of section 8: bytes `b0 b1 b2 b3`. Decoded values are
// computed in double precision and rounded to `f32` once, so each is the
// float nearest the spec's exact value.

fn rgb(e: &[u8]) -> [f32; 3] {
    [e[2], e[1], e[0]].map(|b| (f64::from(b) / 255.0) as f32)
}

fn alpha(e: &[u8]) -> f32 {
    (f64::from(e[3]) / 255.0) as f32
}

fn alpha64(e: &[u8]) -> f64 {
    f64::from(e[3]) / 255.0
}

/// `2 × rgb − 1`.
fn vec(e: &[u8]) -> [f32; 3] {
    [e[2], e[1], e[0]].map(|b| (2.0 * f64::from(b) / 255.0 - 1.0) as f32)
}

fn f32x3(b: &[u8]) -> [f32; 3] {
    [0, 4, 8].map(|i| f32::from_le_bytes(b[i..i + 4].try_into().unwrap()))
}

fn i16_at(b: &[u8], i: usize) -> f32 {
    f32::from(i16::from_le_bytes([b[2 * i], b[2 * i + 1]]))
}

/// Position, decoded normal and the raw tangent and binormal of layouts A, B
/// and C (section 8.1).
fn common(v: &[u8]) -> ([f32; 3], [f32; 3], &[u8], &[u8]) {
    (f32x3(v), vec(&v[12..16]), &v[16..20], &v[20..24])
}

/// Base texture coordinate of layouts B and C: `(s + a/255) × 32 / 32768`
/// with the tangent and binormal alphas as fractions (sections 8.2, 8.3).
fn base_uv(texcoord: &[u8], tangent: &[u8], binormal: &[u8]) -> [f32; 2] {
    const SCALE: f64 = 32.0 / 32768.0;
    let s = |i| f64::from(i16_at(texcoord, i));
    [
        ((s(0) + alpha64(tangent)) * SCALE) as f32,
        ((s(1) + alpha64(binormal)) * SCALE) as f32,
    ]
}

/// Decoded vertex of layout A (section 8.4). Coordinates are the tree's own,
/// to be transformed by the visual's transform.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct TreeVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub binormal: [f32; 3],
    /// `(s0, s1) / 2048`.
    pub uv: [f32; 2],
    /// Wind rigidity `s2 / 2048`.
    pub wind: f32,
    /// The single light level `alpha(normal)`.
    pub light: f32,
}

/// Decoded vertex of layout B (section 8.3), in level coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct VertexLitVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub binormal: [f32; 3],
    pub uv: [f32; 2],
    /// Precomputed light, `rgb(colour)`.
    pub light: [f32; 3],
    /// Sun visibility, `alpha(colour)`.
    pub sun: f32,
    /// Hemisphere (sky) visibility, `alpha(normal)`.
    pub hemi: f32,
}

/// Decoded vertex of layout C (section 8.2), in level coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct LightMappedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub binormal: [f32; 3],
    pub uv: [f32; 2],
    /// Light-map coordinate `(l0, l1) / 32768`.
    pub lightmap_uv: [f32; 2],
    /// Hemisphere-occlusion term `alpha(normal)` (unused by the light-mapped
    /// programs).
    pub hemi: f32,
}

/// A vertex buffer's vertices decoded by its layout.
#[derive(Clone, Debug, PartialEq)]
pub enum Decoded {
    Tree(Vec<TreeVertex>),
    VertexLit(Vec<VertexLitVertex>),
    LightMapped(Vec<LightMappedVertex>),
    /// Layout P: positions in level coordinates (section 8.5).
    Position(Vec<[f32; 3]>),
}

/// One vertex buffer of chunk 9 (section 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexBuffer {
    /// Declaration elements, end marker excluded.
    pub elements: Vec<Element>,
    /// The declaration's layout, if it is one of section 4.1's.
    pub layout: Option<Layout>,
    /// Vertex size: the maximum `offset + size` over the elements.
    pub stride: usize,
    pub vertex_count: u32,
    /// `vertex_count × stride` bytes as stored.
    pub data: Vec<u8>,
}

impl VertexBuffer {
    /// The stored bytes of vertex `i`.
    pub fn vertex(&self, i: usize) -> &[u8] {
        &self.data[i * self.stride..(i + 1) * self.stride]
    }

    fn vertices(&self) -> impl Iterator<Item = &[u8]> {
        // A zero stride (empty declaration) cannot hold vertex data.
        self.data.chunks_exact(self.stride.max(1))
    }

    /// Decodes every vertex as section 8 defines for its layout; `None` for
    /// declarations that are not one of section 4.1's layouts (their elements
    /// can still be read with [`Element::program_input`]).
    pub fn decode(&self) -> Option<Decoded> {
        Some(match self.layout? {
            Layout::Tree => Decoded::Tree(
                self.vertices()
                    .map(|v| {
                        let (position, normal, tangent, binormal) = common(v);
                        let t = &v[24..32];
                        TreeVertex {
                            position,
                            normal,
                            tangent: vec(tangent),
                            binormal: vec(binormal),
                            uv: [i16_at(t, 0) / 2048.0, i16_at(t, 1) / 2048.0],
                            wind: i16_at(t, 2) / 2048.0,
                            light: alpha(&v[12..16]),
                        }
                    })
                    .collect(),
            ),
            Layout::VertexLit => Decoded::VertexLit(
                self.vertices()
                    .map(|v| {
                        let (position, normal, tangent, binormal) = common(v);
                        let colour = &v[24..28];
                        VertexLitVertex {
                            position,
                            normal,
                            tangent: vec(tangent),
                            binormal: vec(binormal),
                            uv: base_uv(&v[28..32], tangent, binormal),
                            light: rgb(colour),
                            sun: alpha(colour),
                            hemi: alpha(&v[12..16]),
                        }
                    })
                    .collect(),
            ),
            Layout::LightMapped => Decoded::LightMapped(
                self.vertices()
                    .map(|v| {
                        let (position, normal, tangent, binormal) = common(v);
                        let l = &v[28..32];
                        LightMappedVertex {
                            position,
                            normal,
                            tangent: vec(tangent),
                            binormal: vec(binormal),
                            uv: base_uv(&v[24..28], tangent, binormal),
                            lightmap_uv: [i16_at(l, 0) / 32768.0, i16_at(l, 1) / 32768.0],
                            hemi: alpha(&v[12..16]),
                        }
                    })
                    .collect(),
            ),
            Layout::Position => Decoded::Position(self.vertices().map(f32x3).collect()),
        })
    }

    /// Positions of every vertex: the FLOAT3 position element, which every
    /// layout has at offset 0 (section 8.1, 8.5).
    pub fn positions(&self) -> Option<Vec<[f32; 3]>> {
        let position = self
            .elements
            .iter()
            .find(|e| e.usage == POSITION && e.usage_index == 0 && e.kind == FLOAT3)?;
        let at = usize::from(position.offset);
        Some(self.vertices().map(|v| f32x3(&v[at..])).collect())
    }
}

/// Steps of continuous level of detail (section 6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlideWindow {
    /// Four reserved words, read and ignored.
    pub reserved: [u32; 4],
    /// From most to least detailed.
    pub steps: Vec<Step>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// First index of the step, relative to the visual's first index.
    pub index_offset: u32,
    pub triangle_count: u16,
    /// Vertices (from the visual's first vertex) the step uses.
    pub vertex_count: u16,
}

impl SlideWindow {
    /// The step the first renderer draws at detail factor `f` in [0, 1]
    /// (1 = full detail): `floor((1 − f) × (step_count − 1) + 0.5)`. We clamp
    /// `f` to [0, 1]; `None` without steps.
    pub fn step_for_detail(&self, f: f32) -> Option<usize> {
        let last = self.steps.len().checked_sub(1)?;
        let f = f.clamp(0.0, 1.0);
        Some((((1.0 - f) * last as f32 + 0.5).floor() as usize).min(last))
    }
}

/// A parsed `level.geom` or `level.geomx`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LevelGeom {
    /// Chunk 1: version and a value of unestablished meaning; `None` when the
    /// chunk is absent or not 4 bytes (no renderer reads it, section 2, so
    /// we do not fail on it).
    pub header: Option<(u16, u16)>,
    pub vertex_buffers: Vec<VertexBuffer>,
    /// Chunk 10; empty when the chunk is absent (optional for the first
    /// renderer).
    pub index_buffers: Vec<Vec<u16>>,
    /// Chunk 11; empty when absent.
    pub slide_windows: Vec<SlideWindow>,
}

impl LevelGeom {
    /// Parses a whole file in memory.
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(file)?;
        let get = |id| -> io::Result<Option<&[u8]>> {
            chunks.find(id).map(|c| c.uncompressed()).transpose()
        };
        Self::from_chunks(
            get(HEADER_CHUNK)?,
            get(VERTEX_BUFFERS_CHUNK)?,
            get(INDEX_BUFFERS_CHUNK)?,
            get(SLIDE_WINDOWS_CHUNK)?,
        )
    }

    /// Reads a file from a stream, reading only the chunks it needs.
    pub fn read(reader: &mut (impl Read + Seek)) -> io::Result<Self> {
        let headers = chunks::index(reader)?;
        let mut get = |id| -> io::Result<Option<Vec<u8>>> {
            let Some(h) = chunks::find_header(&headers, id) else {
                return Ok(None);
            };
            if h.is_compressed() {
                return Err(invalid(format!("level geometry: chunk {id} is compressed")));
            }
            h.read_payload(reader).map(Some)
        };
        let header = get(HEADER_CHUNK)?;
        let vertices = get(VERTEX_BUFFERS_CHUNK)?;
        let indices = get(INDEX_BUFFERS_CHUNK)?;
        let windows = get(SLIDE_WINDOWS_CHUNK)?;
        Self::from_chunks(
            header.as_deref(),
            vertices.as_deref(),
            indices.as_deref(),
            windows.as_deref(),
        )
    }

    fn from_chunks(
        header: Option<&[u8]>,
        vertices: Option<&[u8]>,
        indices: Option<&[u8]>,
        windows: Option<&[u8]>,
    ) -> io::Result<Self> {
        let header = header.and_then(|h| {
            let h: [u8; 4] = h.try_into().ok()?;
            Some((
                u16::from_le_bytes([h[0], h[1]]),
                u16::from_le_bytes([h[2], h[3]]),
            ))
        });
        let vertices = vertices.ok_or_else(|| invalid("level geometry: no vertex buffers"))?;
        Ok(Self {
            header,
            vertex_buffers: parse_vertex_buffers(vertices)?,
            index_buffers: indices.map_or(Ok(Vec::new()), parse_index_buffers)?,
            slide_windows: windows.map_or(Ok(Vec::new()), parse_slide_windows)?,
        })
    }
}

/// Chunk 9 (section 3).
pub fn parse_vertex_buffers(chunk: &[u8]) -> io::Result<Vec<VertexBuffer>> {
    let mut b = Bytes {
        data: chunk,
        what: "vertex buffers",
    };
    let count = b.u32()?;
    let mut buffers = Vec::new();
    for i in 0..count {
        let mut elements = Vec::new();
        loop {
            let e = b.take(8)?;
            let element = Element {
                stream: u16::from_le_bytes([e[0], e[1]]),
                offset: u16::from_le_bytes([e[2], e[3]]),
                kind: e[4],
                method: e[5],
                usage: e[6],
                usage_index: e[7],
            };
            if element.stream == END_STREAM {
                break;
            }
            if elements.len() == MAX_ELEMENTS {
                return Err(invalid(format!(
                    "vertex buffer {i}: more than {MAX_ELEMENTS} elements"
                )));
            }
            // The spec describes only stream 0, the one stored here.
            if element.stream != 0 {
                return Err(invalid(format!(
                    "vertex buffer {i}: element of stream {}",
                    element.stream
                )));
            }
            if element.size().is_none() {
                return Err(invalid(format!(
                    "vertex buffer {i}: unknown element type {}",
                    element.kind
                )));
            }
            elements.push(element);
        }
        let stride = elements
            .iter()
            .map(|e| usize::from(e.offset) + e.size().unwrap())
            .max()
            .unwrap_or(0);
        let vertex_count = b.u32()?;
        if stride == 0 && vertex_count != 0 {
            return Err(invalid(format!(
                "vertex buffer {i}: vertices without elements"
            )));
        }
        let data = b.array(vertex_count, stride)?.to_vec();
        buffers.push(VertexBuffer {
            layout: Layout::recognize(&elements),
            elements,
            stride,
            vertex_count,
            data,
        });
    }
    b.finish()?;
    Ok(buffers)
}

/// Chunk 10 (section 5).
pub fn parse_index_buffers(chunk: &[u8]) -> io::Result<Vec<Vec<u16>>> {
    let mut b = Bytes {
        data: chunk,
        what: "index buffers",
    };
    let count = b.u32()?;
    let mut buffers = Vec::new();
    for _ in 0..count {
        let n = b.u32()?;
        let data = b.array(n, 2)?;
        buffers.push(
            data.as_chunks::<2>()
                .0
                .iter()
                .map(|i| u16::from_le_bytes(*i))
                .collect(),
        );
    }
    b.finish()?;
    Ok(buffers)
}

/// Chunk 11 (section 6). The spec does not say that the records fill the
/// chunk; we require it, as for chunks 9 and 10.
pub fn parse_slide_windows(chunk: &[u8]) -> io::Result<Vec<SlideWindow>> {
    let mut b = Bytes {
        data: chunk,
        what: "progressive-mesh records",
    };
    let count = b.u32()?;
    let mut records = Vec::new();
    for _ in 0..count {
        let reserved = [b.u32()?, b.u32()?, b.u32()?, b.u32()?];
        let n = b.u32()?;
        let steps = b
            .array(n, 8)?
            .as_chunks::<8>()
            .0
            .iter()
            .map(|s| Step {
                index_offset: u32::from_le_bytes(s[..4].try_into().unwrap()),
                triangle_count: u16::from_le_bytes([s[4], s[5]]),
                vertex_count: u16::from_le_bytes([s[6], s[7]]),
            })
            .collect();
        records.push(SlideWindow { reserved, steps });
    }
    b.finish()?;
    Ok(records)
}

/// A visual's reference into the geometry buffers (section 7): six `u32` as
/// stored in the visual's data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeometryRef {
    pub vertex_buffer: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub index_buffer: u32,
    pub first_index: u32,
    pub index_count: u32,
}

impl GeometryRef {
    pub fn parse(record: &[u8; 24]) -> Self {
        let u = |i: usize| u32::from_le_bytes(record[4 * i..4 * i + 4].try_into().unwrap());
        Self {
            vertex_buffer: u(0),
            first_vertex: u(1),
            vertex_count: u(2),
            index_buffer: u(3),
            first_index: u(4),
            index_count: u(5),
        }
    }

    /// The visual's triangles as vertex numbers within its vertex buffer:
    /// `first_vertex + IB[first_index + 3t + j]`. Errors when a range lies
    /// outside its buffer or an index reaches past the visual's vertices.
    pub fn triangles(&self, geom: &LevelGeom) -> io::Result<Vec<[u32; 3]>> {
        let vb = geom
            .vertex_buffers
            .get(self.vertex_buffer as usize)
            .ok_or_else(|| invalid(format!("no vertex buffer {}", self.vertex_buffer)))?;
        let ib = geom
            .index_buffers
            .get(self.index_buffer as usize)
            .ok_or_else(|| invalid(format!("no index buffer {}", self.index_buffer)))?;
        let vertex_end = u64::from(self.first_vertex) + u64::from(self.vertex_count);
        if vertex_end > u64::from(vb.vertex_count) {
            return Err(invalid("vertex range outside its buffer"));
        }
        let first = self.first_index as usize;
        let indices = first
            .checked_add(self.index_count as usize)
            .and_then(|end| ib.get(first..end))
            .ok_or_else(|| invalid("index range outside its buffer"))?;
        indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let mut out = [0; 3];
                for (o, &i) in out.iter_mut().zip(t) {
                    if u32::from(i) >= self.vertex_count {
                        return Err(invalid(format!(
                            "index {i} beyond the visual's {} vertices",
                            self.vertex_count
                        )));
                    }
                    *o = self.first_vertex + u32::from(i);
                }
                Ok(out)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::chunk;
    use std::io::Cursor;

    fn declaration(elements: &[Element]) -> Vec<u8> {
        let mut out = Vec::new();
        for e in elements {
            out.extend(e.stream.to_le_bytes());
            out.extend(e.offset.to_le_bytes());
            out.extend([e.kind, e.method, e.usage, e.usage_index]);
        }
        out.extend([0xFF, 0, 0, 0, 17, 0, 0, 0]);
        out
    }

    fn vertex_chunk(buffers: &[(Layout, Vec<Vec<u8>>)]) -> Vec<u8> {
        let mut out = (buffers.len() as u32).to_le_bytes().to_vec();
        for (layout, vertices) in buffers {
            out.extend(declaration(&layout.elements()));
            out.extend((vertices.len() as u32).to_le_bytes());
            for v in vertices {
                out.extend(v);
            }
        }
        out
    }

    fn common(position: [f32; 3], normal: [u8; 4], tangent: [u8; 4], binormal: [u8; 4]) -> Vec<u8> {
        let mut v: Vec<u8> = position.iter().flat_map(|f| f.to_le_bytes()).collect();
        v.extend(normal);
        v.extend(tangent);
        v.extend(binormal);
        v
    }

    fn shorts(values: &[i16]) -> Vec<u8> {
        values.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn decodes_every_layout() {
        // Normal bytes b0 b1 b2 b3 = (0, 255, 128, 51): x from b2, z from b0.
        let normal = [0, 255, 128, 51];
        let mut tree = common([1.0, 2.0, 3.0], normal, [255; 4], [0; 4]);
        tree.extend(shorts(&[2048, -1024, 4096, 0]));
        let mut lit = common([-1.0, 0.5, 8.0], normal, [0, 0, 0, 255], [0, 0, 0, 0]);
        lit.extend([10, 20, 30, 255]);
        lit.extend(shorts(&[1024, -3]));
        let mut lmap = common([0.0; 3], normal, [0, 0, 0, 0], [0, 0, 0, 255]);
        lmap.extend(shorts(&[-1024, 2047]));
        lmap.extend(shorts(&[16384, 32767]));
        let pos: Vec<u8> = [4.0f32, 5.0, 6.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let data = vertex_chunk(&[
            (Layout::Tree, vec![tree]),
            (Layout::VertexLit, vec![lit]),
            (Layout::LightMapped, vec![lmap]),
            (Layout::Position, vec![pos]),
        ]);
        let buffers = parse_vertex_buffers(&data).unwrap();
        let strides: Vec<usize> = buffers.iter().map(|b| b.stride).collect();
        assert_eq!(strides, [32, 32, 32, 12]);

        let n = [(2.0 * 128.0 / 255.0 - 1.0f64) as f32, 1.0, -1.0];
        let Some(Decoded::Tree(t)) = buffers[0].decode() else {
            panic!()
        };
        assert_eq!(t[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(t[0].normal, n);
        assert_eq!(t[0].tangent, [1.0; 3]);
        assert_eq!(t[0].binormal, [-1.0; 3]);
        assert_eq!(t[0].uv, [1.0, -0.5]);
        assert_eq!(t[0].wind, 2.0);
        assert_eq!(t[0].light, 51.0 / 255.0);

        let Some(Decoded::VertexLit(l)) = buffers[1].decode() else {
            panic!()
        };
        // u gets the tangent's alpha as a fraction: (1024 + 1) / 1024.
        assert_eq!(l[0].uv, [1025.0 / 1024.0, -3.0 / 1024.0]);
        assert_eq!(l[0].light, [30.0 / 255.0, 20.0 / 255.0, 10.0 / 255.0]);
        assert_eq!((l[0].sun, l[0].hemi), (1.0, 51.0 / 255.0));

        let Some(Decoded::LightMapped(c)) = buffers[2].decode() else {
            panic!()
        };
        assert_eq!(c[0].uv, [-1.0, 2048.0 / 1024.0]);
        assert_eq!(c[0].lightmap_uv, [0.5, 32767.0 / 32768.0]);

        assert_eq!(
            buffers[3].decode(),
            Some(Decoded::Position(vec![[4.0, 5.0, 6.0]]))
        );
        assert_eq!(buffers[0].positions().unwrap(), [[1.0, 2.0, 3.0]]);

        // Generic program inputs agree (section 4's table).
        let e = buffers[1].elements[1];
        assert_eq!(
            e.program_input(buffers[1].vertex(0)),
            [128.0 / 255.0, 1.0, 0.0, 51.0 / 255.0]
        );
        let e = buffers[0].elements[4];
        assert_eq!(
            e.program_input(buffers[0].vertex(0)),
            [2048.0, -1024.0, 4096.0, 0.0]
        );
        let e = buffers[1].elements[5];
        assert_eq!(
            e.program_input(buffers[1].vertex(0)),
            [1024.0, -3.0, 0.0, 1.0]
        );
    }

    #[test]
    fn generic_declarations_and_errors() {
        // An unrecognised but describable declaration: stride from the
        // furthest element, not the sum.
        let elements = [
            Element::new(4, FLOAT3, POSITION, 0),
            Element::new(0, D3DCOLOR, COLOR, 0),
        ];
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend(declaration(&elements));
        data.extend(2u32.to_le_bytes());
        data.extend([0; 32]);
        let b = &parse_vertex_buffers(&data).unwrap()[0];
        assert_eq!((b.stride, b.layout, b.vertex_count), (16, None, 2));
        assert!(b.decode().is_none());
        assert_eq!(b.positions().unwrap().len(), 2);

        // Left-over bytes, truncation, unknown type, other stream.
        let mut extra = data.clone();
        extra.push(0);
        assert!(parse_vertex_buffers(&extra).is_err());
        assert!(parse_vertex_buffers(&data[..data.len() - 1]).is_err());
        let mut unknown = data.clone();
        unknown[4 + 4] = 3;
        assert!(parse_vertex_buffers(&unknown).is_err());
        let mut stream = data.clone();
        stream[4] = 1;
        assert!(parse_vertex_buffers(&stream).is_err());
        // More than 64 elements before the end marker.
        let many = vec![Element::new(0, FLOAT3, POSITION, 0); 65];
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend(declaration(&many));
        data.extend(0u32.to_le_bytes());
        assert!(parse_vertex_buffers(&data).is_err());
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend(declaration(&many[..64]));
        data.extend(0u32.to_le_bytes());
        assert!(parse_vertex_buffers(&data).is_ok());
    }

    fn sample_file(header: bool, with_indices: bool) -> Vec<u8> {
        let pos: Vec<u8> = [0.0f32; 9].iter().flat_map(|f| f.to_le_bytes()).collect();
        let vertices = vertex_chunk(&[(
            Layout::Position,
            pos.chunks(12).map(<[u8]>::to_vec).collect(),
        )]);
        let mut indices = 2u32.to_le_bytes().to_vec();
        indices.extend(3u32.to_le_bytes());
        indices.extend(shorts(&[0, 1, 2]));
        indices.extend(0u32.to_le_bytes());
        let mut windows = 1u32.to_le_bytes().to_vec();
        windows.extend([0; 16]);
        windows.extend(2u32.to_le_bytes());
        windows.extend(3u32.to_le_bytes());
        windows.extend(shorts(&[1, 3]));
        windows.extend(0u32.to_le_bytes());
        windows.extend(shorts(&[0, 0]));
        let mut file = Vec::new();
        if header {
            file.extend(chunk(HEADER_CHUNK, &[14, 0, 18, 0]));
        }
        file.extend(chunk(VERTEX_BUFFERS_CHUNK, &vertices));
        if with_indices {
            file.extend(chunk(INDEX_BUFFERS_CHUNK, &indices));
        }
        file.extend(chunk(SLIDE_WINDOWS_CHUNK, &windows));
        file
    }

    #[test]
    fn parses_files_from_bytes_and_streams() {
        let file = sample_file(true, true);
        let geom = LevelGeom::parse(&file).unwrap();
        assert_eq!(geom, LevelGeom::read(&mut Cursor::new(&file)).unwrap());
        assert_eq!(geom.header, Some((14, 18)));
        assert_eq!(geom.index_buffers, [vec![0, 1, 2], vec![]]);
        let w = &geom.slide_windows[0];
        assert_eq!(
            w.steps[0],
            Step {
                index_offset: 3,
                triangle_count: 1,
                vertex_count: 3
            }
        );
        assert_eq!(w.step_for_detail(1.0), Some(0));
        assert_eq!(w.step_for_detail(0.0), Some(1));
        assert_eq!(w.step_for_detail(0.6), Some(0));
        assert_eq!(w.step_for_detail(0.4), Some(1));
        assert_eq!(SlideWindow::default().step_for_detail(1.0), None);

        // Header and index buffers are optional, vertex buffers are not.
        let geom = LevelGeom::parse(&sample_file(false, false)).unwrap();
        assert_eq!(geom.header, None);
        assert!(geom.index_buffers.is_empty());
        assert!(LevelGeom::parse(&chunk(HEADER_CHUNK, &[0; 4])).is_err());
        // Compressed chunks are not allowed in these files.
        let mut compressed = sample_file(true, true);
        compressed[8 + 4 + 3] |= 0x80;
        assert!(LevelGeom::parse(&compressed).is_err());
        assert!(LevelGeom::read(&mut Cursor::new(&compressed)).is_err());
    }

    #[test]
    fn resolves_geometry_references() {
        let geom = LevelGeom::parse(&sample_file(true, true)).unwrap();
        let mut record = [0u8; 24];
        for (i, v) in [0u32, 0, 3, 0, 0, 3].iter().enumerate() {
            record[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
        let r = GeometryRef::parse(&record);
        assert_eq!(r.triangles(&geom).unwrap(), [[0, 1, 2]]);
        let shifted = GeometryRef {
            first_vertex: 1,
            vertex_count: 2,
            ..r
        };
        assert!(shifted.triangles(&geom).is_err(), "index 2 ≥ 2 vertices");
        let past = GeometryRef {
            index_count: 6,
            ..r
        };
        assert!(past.triangles(&geom).is_err());
        let no_buffer = GeometryRef {
            index_buffer: 5,
            ..r
        };
        assert!(no_buffer.triangles(&geom).is_err());
    }
}
