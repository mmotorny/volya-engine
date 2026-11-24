// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The static collision mesh `levels\<level>\level.cform`
//! (docs/formats/level-cform.md section 2): a header, the vertex positions
//! and 16-byte triangle records. Not chunked.

use std::io;

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

impl Cform {
    /// Parses the whole file. A version other than 4 or a size other than
    /// `36 + 12·V + 16·T` is an error (section 2.1); so is a vertex index
    /// `≥ V`, which the spec does not describe (our choice, so later code
    /// never reads outside the vertices).
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(invalid(format!("level.cform: {} bytes", data.len())));
        }
        let version = u32_at(data, 0);
        if version != VERSION {
            return Err(invalid(format!(
                "level.cform: version {version}, expected {VERSION}"
            )));
        }
        let v = u32_at(data, 4) as usize;
        let t = u32_at(data, 8) as usize;
        let expected = v
            .checked_mul(VERTEX_SIZE)
            .and_then(|a| t.checked_mul(TRIANGLE_SIZE).map(|b| (a, b)))
            .and_then(|(a, b)| a.checked_add(b)?.checked_add(HEADER_SIZE));
        if expected != Some(data.len()) {
            return Err(invalid(format!(
                "level.cform: {} bytes for {v} vertices and {t} triangles",
                data.len()
            )));
        }
        let vertex_end = HEADER_SIZE + v * VERTEX_SIZE;
        let vertices: Vec<[f32; 3]> = data[HEADER_SIZE..vertex_end]
            .as_chunks::<VERTEX_SIZE>()
            .0
            .iter()
            .map(|b| float3(b))
            .collect();
        let mut triangles = Vec::with_capacity(t);
        for (i, r) in data[vertex_end..]
            .as_chunks::<TRIANGLE_SIZE>()
            .0
            .iter()
            .enumerate()
        {
            let vertices = [u32_at(r, 0), u32_at(r, 4), u32_at(r, 8)];
            if vertices.iter().any(|&k| k as usize >= v) {
                return Err(invalid(format!(
                    "level.cform: triangle {i} has vertex indices {vertices:?}, {v} vertices"
                )));
            }
            triangles.push(CformTriangle {
                vertices,
                attributes: u32_at(r, 12),
            });
        }
        Ok(Self {
            bbox_min: float3(&data[12..24]),
            bbox_max: float3(&data[24..36]),
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
