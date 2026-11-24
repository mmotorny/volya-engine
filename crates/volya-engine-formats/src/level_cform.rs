// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The static collision mesh `levels\<level>\level.cform`
//! (docs/formats/level-cform.md section 2): a header, the vertex positions
//! and 16-byte triangle records. Not chunked.

use std::io::{self, Cursor, Read, Seek, SeekFrom};

/// The only version the game accepts (section 2.1).
pub const VERSION: u32 = 4;
/// Header bytes: version, counts, bounding box (section 2.1).
pub const HEADER_SIZE: usize = 36;
/// Bytes of one vertex: a `float3` position.
pub const VERTEX_SIZE: usize = 12;
/// Bytes of one triangle record (section 2.2).
pub const TRIANGLE_SIZE: usize = 16;

/// Bits 0–13 of the attribute word: the material (section 2.2).
pub const MATERIAL_MASK: u32 = 0x3FFF;
/// Bit 14 of the attribute word.
pub const BIT_14: u32 = 0x4000;
/// Bit 15 of the attribute word.
pub const BIT_15: u32 = 0x8000;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// One triangle record (section 2.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CformTriangle {
    /// Vertex indices `i0, i1, i2`.
    pub vertices: [u32; 3],
    /// The attribute word `w`.
    pub attributes: u32,
}

impl CformTriangle {
    /// Bits 0–13: the material id in the file (an index after the load-time
    /// mapping, section 3.3).
    pub fn material(&self) -> u32 {
        self.attributes & MATERIAL_MASK
    }

    /// Bits 16–31: the sector index.
    pub fn sector(&self) -> u32 {
        self.attributes >> 16
    }

    /// Whether two vertex indices are equal: zero area (section 2.4).
    pub fn is_degenerate(&self) -> bool {
        let [a, b, c] = self.vertices;
        a == b || b == c || a == c
    }
}

/// A parsed `level.cform`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cform {
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<CformTriangle>,
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn float3(b: &[u8]) -> [f32; 3] {
    [
        f32::from_le_bytes(b[0..4].try_into().unwrap()),
        f32::from_le_bytes(b[4..8].try_into().unwrap()),
        f32::from_le_bytes(b[8..12].try_into().unwrap()),
    ]
}

/// The header of a `level.cform` (section 2.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CformHeader {
    pub vertex_count: u32,
    pub triangle_count: u32,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
}

/// Records read per call to the underlying reader.
const BATCH: usize = 4096;

/// Reads a `level.cform` from a stream in order (header, vertices,
/// triangles) without holding the file, so a caller can keep the triangles
/// in its own form.
pub struct CformReader<R> {
    reader: R,
    header: CformHeader,
}

impl<R: Read + Seek> CformReader<R> {
    /// Reads and checks the header. A version other than 4 or a stream
    /// length other than `36 + 12·V + 16·T` is an error (section 2.1).
    pub fn new(mut reader: R) -> io::Result<Self> {
        let length = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        if length < HEADER_SIZE as u64 {
            return Err(invalid(format!("level.cform: {length} bytes")));
        }
        let mut h = [0u8; HEADER_SIZE];
        reader.read_exact(&mut h)?;
        let version = u32_at(&h, 0);
        if version != VERSION {
            return Err(invalid(format!(
                "level.cform: version {version}, expected {VERSION}"
            )));
        }
        let (v, t) = (u32_at(&h, 4), u32_at(&h, 8));
        let expected = HEADER_SIZE as u64
            + u64::from(v) * VERTEX_SIZE as u64
            + u64::from(t) * TRIANGLE_SIZE as u64;
        if expected != length {
            return Err(invalid(format!(
                "level.cform: {length} bytes for {v} vertices and {t} triangles"
            )));
        }
        Ok(Self {
            reader,
            header: CformHeader {
                vertex_count: v,
                triangle_count: t,
                bbox_min: float3(&h[12..24]),
                bbox_max: float3(&h[24..36]),
            },
        })
    }

    pub fn header(&self) -> &CformHeader {
        &self.header
    }

    /// Calls `each` with `count` records of `size` bytes, read in batches.
    fn records<const N: usize>(
        &mut self,
        offset: u64,
        count: u32,
        mut each: impl FnMut(usize, &[u8; N]) -> io::Result<()>,
    ) -> io::Result<()> {
        self.reader.seek(SeekFrom::Start(offset))?;
        let mut buffer = vec![0u8; BATCH * N];
        let mut done = 0usize;
        let count = count as usize;
        while done < count {
            let n = (count - done).min(BATCH);
            self.reader.read_exact(&mut buffer[..n * N])?;
            for (k, r) in buffer[..n * N].as_chunks::<N>().0.iter().enumerate() {
                each(done + k, r)?;
            }
            done += n;
        }
        Ok(())
    }

    /// The vertex positions.
    pub fn vertices(&mut self) -> io::Result<Vec<[f32; 3]>> {
        let mut out = Vec::with_capacity(self.header.vertex_count as usize);
        self.records::<VERTEX_SIZE>(HEADER_SIZE as u64, self.header.vertex_count, |_, r| {
            out.push(float3(r));
            Ok(())
        })?;
        Ok(out)
    }

    /// Calls `each` with every triangle's index and record, in file order.
    /// A vertex index `≥ V`, which the spec does not describe, is an error
    /// (our choice, so later code never reads outside the vertices).
    pub fn triangles(
        &mut self,
        mut each: impl FnMut(usize, CformTriangle) -> io::Result<()>,
    ) -> io::Result<()> {
        let v = self.header.vertex_count;
        let offset = HEADER_SIZE as u64 + u64::from(v) * VERTEX_SIZE as u64;
        self.records::<TRIANGLE_SIZE>(offset, self.header.triangle_count, |i, r| {
            let vertices = [u32_at(r, 0), u32_at(r, 4), u32_at(r, 8)];
            if vertices.iter().any(|&k| k >= v) {
                return Err(invalid(format!(
                    "level.cform: triangle {i} has vertex indices {vertices:?}, {v} vertices"
                )));
            }
            each(
                i,
                CformTriangle {
                    vertices,
                    attributes: u32_at(r, 12),
                },
            )
        })
    }
}

impl Cform {
    /// Parses a whole file held in memory, as [`CformReader`] reads it.
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let mut reader = CformReader::new(Cursor::new(data))?;
        let header = *reader.header();
        let vertices = reader.vertices()?;
        let mut triangles = Vec::with_capacity(header.triangle_count as usize);
        reader.triangles(|_, t| {
            triangles.push(t);
            Ok(())
        })?;
        Ok(Self {
            bbox_min: header.bbox_min,
            bbox_max: header.bbox_max,
            vertices,
            triangles,
        })
    }
}

#[cfg(test)]
pub(crate) fn file(vertices: &[[f32; 3]], triangles: &[CformTriangle]) -> Vec<u8> {
    let mut out = VERSION.to_le_bytes().to_vec();
    out.extend((vertices.len() as u32).to_le_bytes());
    out.extend((triangles.len() as u32).to_le_bytes());
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in vertices {
        for k in 0..3 {
            min[k] = min[k].min(v[k]);
            max[k] = max[k].max(v[k]);
        }
    }
    for f in min.iter().chain(&max) {
        out.extend(f.to_le_bytes());
    }
    for v in vertices {
        for f in v {
            out.extend(f.to_le_bytes());
        }
    }
    for t in triangles {
        for i in t.vertices {
            out.extend(i.to_le_bytes());
        }
        out.extend(t.attributes.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_vertices_and_triangles() {
        let vertices = [[0.0, 0.0, 0.0], [1.0, 2.0, 3.0], [-1.0, 0.5, 4.0]];
        let triangles = [
            CformTriangle {
                vertices: [0, 1, 2],
                attributes: (23 << 16) | BIT_15 | 81,
            },
            CformTriangle {
                vertices: [2, 2, 1],
                attributes: 12,
            },
        ];
        let data = file(&vertices, &triangles);
        assert_eq!(data.len(), 36 + 3 * 12 + 2 * 16);
        let cform = Cform::parse(&data).unwrap();
        assert_eq!(cform.bbox_min, [-1.0, 0.0, 0.0]);
        assert_eq!(cform.bbox_max, [1.0, 2.0, 4.0]);
        assert_eq!(cform.vertices, vertices);
        assert_eq!(cform.triangles, triangles);
        let t = cform.triangles[0];
        assert_eq!(
            (t.material(), t.sector(), t.is_degenerate()),
            (81, 23, false)
        );
        assert!(cform.triangles[1].is_degenerate());
    }

    #[test]
    fn rejects_bad_version_size_and_indices() {
        let vertices = [[0.0; 3]; 3];
        let good = file(
            &vertices,
            &[CformTriangle {
                vertices: [0, 1, 2],
                attributes: 0,
            }],
        );
        let mut bad = good.clone();
        bad[0] = 3;
        assert!(Cform::parse(&bad).is_err());
        let mut long = good.clone();
        long.push(0);
        assert!(Cform::parse(&long).is_err());
        assert!(Cform::parse(&good[..good.len() - 1]).is_err());
        assert!(Cform::parse(&good[..20]).is_err());
        let out_of_range = file(
            &vertices,
            &[CformTriangle {
                vertices: [0, 1, 3],
                attributes: 0,
            }],
        );
        assert!(Cform::parse(&out_of_range).is_err());
    }
}
