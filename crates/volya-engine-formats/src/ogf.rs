// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The visual structure (`.ogf`) of the level's static visuals
//! (docs/formats/ogf.md): header, geometry references into `level.geom` /
//! `level.geomx`, groups, tree instances, progressive meshes and distant
//! billboards.

use std::io;

use crate::chunks::Chunks;
use crate::level_geom::{GeometryRef, ProgressiveMesh, parse_progressive_mesh};

/// Chunk ids (sections 2–7).
pub const HEADER_CHUNK: u32 = 1;
pub const WINDOWS_CHUNK: u32 = 6;
pub const CHILDREN_CHUNK: u32 = 10;
pub const BILLBOARDS_CHUNK: u32 = 11;
pub const TREE_CHUNK: u32 = 12;
pub const WINDOW_RECORD_CHUNK: u32 = 20;
pub const GEOMETRY_CHUNK: u32 = 21;
pub const GEOMETRY_X_CHUNK: u32 = 22;

/// The only header version (section 2).
const VERSION: u8 = 4;
const HEADER_SIZE: usize = 44;
const TREE_SIZE: usize = 104;
const BILLBOARD_VERTEX_SIZE: usize = 28;
const BILLBOARDS_SIZE: usize = 8 * 4 * BILLBOARD_VERTEX_SIZE;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn float3(b: &[u8], at: usize) -> [f32; 3] {
    [f32_at(b, at), f32_at(b, at + 4), f32_at(b, at + 8)]
}

/// Visual types of the static world (section 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VisualType {
    Mesh = 0,
    Group = 1,
    Progressive = 2,
    BillboardGroup = 6,
    Tree = 7,
    ProgressiveTree = 11,
}

impl VisualType {
    pub fn from_u8(v: u8) -> io::Result<Self> {
        Ok(match v {
            0 => Self::Mesh,
            1 => Self::Group,
            2 => Self::Progressive,
            6 => Self::BillboardGroup,
            7 => Self::Tree,
            11 => Self::ProgressiveTree,
            // Types of standalone model files, out of scope (section 1).
            3..=5 | 8..=10 => {
                return Err(invalid(format!("visual type {v} is not a static visual")));
            }
            _ => return Err(invalid(format!("Unknown visual type {v}"))),
        })
    }

    /// Whether the type has a geometry reference (chunk 21).
    pub fn has_geometry(self) -> bool {
        !matches!(self, Self::Group | Self::BillboardGroup)
    }

    pub fn is_progressive(self) -> bool {
        matches!(self, Self::Progressive | Self::ProgressiveTree)
    }

    pub fn is_tree(self) -> bool {
        matches!(self, Self::Tree | Self::ProgressiveTree)
    }
}

/// Chunk 1 (section 2). Bounds are world coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub kind: VisualType,
    /// Index into the `level` shader table; 0 = none.
    pub shader: u16,
    pub box_min: [f32; 3],
    pub box_max: [f32; 3],
    pub sphere_centre: [f32; 3],
    pub sphere_radius: f32,
}

impl Header {
    pub fn parse(b: &[u8]) -> io::Result<Self> {
        if b.len() != HEADER_SIZE {
            return Err(invalid(format!("visual header of {} bytes", b.len())));
        }
        if b[0] != VERSION {
            return Err(invalid(format!("visual version {}", b[0])));
        }
        Ok(Self {
            kind: VisualType::from_u8(b[1])?,
            shader: u16::from_le_bytes([b[2], b[3]]),
            box_min: float3(b, 4),
            box_max: float3(b, 16),
            sphere_centre: float3(b, 28),
            sphere_radius: f32_at(b, 40),
        })
    }
}

/// Chunk 12 (section 5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeInstance {
    /// Row-major 4 × 4, row-vector convention: world = (x, y, z, 1) × M.
    pub transform: [f32; 16],
    /// `s_r, s_g, s_b, s_hemi, s_sun`.
    pub scale: [f32; 5],
    /// `b_r, b_g, b_b, b_hemi, b_sun`.
    pub bias: [f32; 5],
}

/// The factor applied to every stored lighting value (section 5).
const TREE_LIGHT_FACTOR: f32 = 0.75;

impl TreeInstance {
    fn parse(b: &[u8]) -> io::Result<Self> {
        if b.len() != TREE_SIZE {
            return Err(invalid(format!("tree instance of {} bytes", b.len())));
        }
        Ok(Self {
            transform: std::array::from_fn(|i| f32_at(b, 4 * i)),
            scale: std::array::from_fn(|i| f32_at(b, 64 + 4 * i)),
            bias: std::array::from_fn(|i| f32_at(b, 84 + 4 * i)),
        })
    }

    /// The transform as rows.
    pub fn matrix(&self) -> [[f32; 4]; 4] {
        std::array::from_fn(|r| std::array::from_fn(|c| self.transform[4 * r + c]))
    }

    /// A tree-space position in world coordinates.
    pub fn transform_point(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.transform;
        std::array::from_fn(|c| p[0] * m[c] + p[1] * m[4 + c] + p[2] * m[8 + c] + m[12 + c])
    }

    /// A tree-space normal by the upper 3 × 3 part (not renormalized).
    pub fn transform_normal(&self, n: [f32; 3]) -> [f32; 3] {
        let m = &self.transform;
        std::array::from_fn(|c| n[0] * m[c] + n[1] * m[4 + c] + n[2] * m[8 + c])
    }

    /// World height of the tree's base, used by the wind sway: M[13].
    pub fn base_height(&self) -> f32 {
        self.transform[13]
    }

    /// `c_scale` = 0.75 × (s_r, s_g, s_b, s_hemi).
    pub fn c_scale(&self) -> [f32; 4] {
        std::array::from_fn(|i| TREE_LIGHT_FACTOR * self.scale[i])
    }

    /// `c_bias` = 0.75 × (b_r, b_g, b_b, b_hemi) + (ambient, 0).
    pub fn c_bias(&self, ambient: [f32; 3]) -> [f32; 4] {
        std::array::from_fn(|i| TREE_LIGHT_FACTOR * self.bias[i] + ambient.get(i).unwrap_or(&0.0))
    }

    /// `c_sun` = (0.75 s_sun, 0.75 b_sun, 0, 0).
    pub fn c_sun(&self) -> [f32; 4] {
        [
            TREE_LIGHT_FACTOR * self.scale[4],
            TREE_LIGHT_FACTOR * self.bias[4],
            0.0,
            0.0,
        ]
    }
}

/// One billboard vertex (section 7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BillboardVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    /// D3DCOLOR as stored (bytes B, G, R, A): light colour and hemisphere.
    pub colour: [u8; 4],
    pub sun: u8,
}

/// Chunk 11: 8 faces of 4 vertices (section 7).
#[derive(Clone, Debug, PartialEq)]
pub struct Billboards {
    pub faces: [[BillboardVertex; 4]; 8],
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Normalizes `a` unless its squared length is at most 1.1754944e−38.
fn normalize_or_keep(a: [f32; 3]) -> [f32; 3] {
    let sq = dot(a, a);
    if sq <= 1.175_494_4e-38 {
        a
    } else {
        a.map(|c| c / sq.sqrt())
    }
}

/// The faces chosen for a drawn billboard (section 7, steps 1–2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceChoice {
    /// The face facing the camera most (largest weight).
    pub f1: usize,
    /// The second.
    pub f2: usize,
    /// Blend factor towards `f1`, 0 … 255.
    pub beta: u8,
}

impl Billboards {
    fn parse(b: &[u8]) -> io::Result<Self> {
        if b.len() != BILLBOARDS_SIZE {
            return Err(invalid(format!("billboards of {} bytes", b.len())));
        }
        Ok(Self {
            faces: std::array::from_fn(|f| {
                std::array::from_fn(|k| {
                    let v = &b[112 * f + 28 * k..][..BILLBOARD_VERTEX_SIZE];
                    BillboardVertex {
                        position: float3(v, 0),
                        uv: [f32_at(v, 12), f32_at(v, 16)],
                        colour: v[20..24].try_into().unwrap(),
                        sun: v[24],
                    }
                })
            }),
        })
    }

    /// The selection normal of face `f`: −normalize(¼ Σ tᵢ) of its corner
    /// normals.
    pub fn face_normal(&self, f: usize) -> [f32; 3] {
        let v = self.faces[f].map(|v| v.position);
        let mut sum = [0.0; 3];
        for i in 0..4 {
            let (a, b, c) = (v[i], v[(i + 1) % 4], v[(i + 2) % 4]);
            let t = normalize_or_keep(cross(sub(b, a), sub(c, b)));
            for (s, t) in sum.iter_mut().zip(t) {
                *s += t / 4.0;
            }
        }
        let len = dot(sum, sum).sqrt();
        sum.map(|c| -c / len)
    }

    /// Faces and blend factor for a camera at `camera`, with the group's
    /// sphere centre `centre`. Equal weights keep face order (our choice; the
    /// spec does not say how ties sort).
    pub fn choose_faces(&self, centre: [f32; 3], camera: [f32; 3]) -> FaceChoice {
        let d = sub(centre, camera);
        let len = dot(d, d).sqrt();
        let e = d.map(|c| c / len);
        let w: [f32; 8] = std::array::from_fn(|f| dot(e, self.face_normal(f)));
        let mut order: [usize; 8] = std::array::from_fn(|i| i);
        order.sort_by(|&a, &b| w[a].total_cmp(&w[b]));
        let (f1, f2, f3) = (order[7], order[6], order[5]);
        let beta = (((1.0 - (w[f2] - w[f3]) / (w[f1] - w[f3])) + 1.0) * 0.5 * 255.5).floor();
        FaceChoice {
            f1,
            f2,
            // A NaN (all three weights equal) becomes 0: our choice.
            beta: beta.clamp(0.0, 255.0) as u8,
        }
    }
}

/// The billboard group's coverage factor `k` from its header (section 7).
/// We clamp the middle half-extent to the radius (the spec does not cover
/// a half-extent beyond it).
pub fn coverage_factor(header: &Header) -> f32 {
    let r = header.sphere_radius;
    let mut h: [f32; 3] = std::array::from_fn(|i| (header.box_max[i] - header.box_min[i]) / 2.0);
    h.sort_by(f32::total_cmp);
    let m = h[1].min(r);
    2.0 * (r * r * (m / r).asin() + m * (r * r - m * m).sqrt()) / (std::f32::consts::PI * r * r)
}

/// Billboard fade `α` for screen measure `sigma` (section 7, step 3).
pub fn billboard_fade(sigma: f32, t_a: f32, t_b: f32) -> u8 {
    let a = ((1.0 - (sigma - t_b) / (t_a - t_b).max(1e-7)) * 255.0).floor();
    a.clamp(0.0, 255.0) as u8
}

/// A visual of the static world.
#[derive(Clone, Debug, PartialEq)]
pub struct Visual {
    pub header: Header,
    /// Chunk 21: into `level.geom`.
    pub geometry: Option<GeometryRef>,
    /// Chunk 22's nested chunk 21: into `level.geomx` (second renderer).
    pub geometry_x: Option<GeometryRef>,
    /// Chunk 22's nested chunk 6 (type 2).
    pub window_list_x: Option<ProgressiveMesh>,
    /// Chunk 10 (types 1 and 6): child visual indices.
    pub children: Vec<u32>,
    /// Chunk 6 (type 2).
    pub window_list: Option<ProgressiveMesh>,
    /// Chunk 20 (type 11): window record in `level.geom` chunk 11.
    pub window_record: Option<u32>,
    /// Chunk 12 (types 7 and 11).
    pub tree: Option<TreeInstance>,
    /// Chunk 11 (type 6).
    pub billboards: Option<Box<Billboards>>,
}

fn geometry_ref(b: &[u8]) -> io::Result<GeometryRef> {
    let record: &[u8; 24] = b
        .try_into()
        .map_err(|_| invalid(format!("geometry reference of {} bytes", b.len())))?;
    Ok(GeometryRef::parse(record))
}

impl Visual {
    /// Parses one visual (a chunk sequence). The chunks each type needs
    /// (section 1) are required; other chunks are ignored.
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(data)?;
        let header = Header::parse(&chunks.require(HEADER_CHUNK, "visual")?.content()?)?;
        let kind = header.kind;
        let need = |id: u32, needed: bool| -> io::Result<Option<Vec<u8>>> {
            match chunks.find(id) {
                Some(c) if needed => Ok(Some(c.content()?.into_owned())),
                None if needed => Err(invalid(format!("visual of type {kind:?}: no chunk {id}"))),
                _ => Ok(None),
            }
        };
        let geometry = need(GEOMETRY_CHUNK, kind.has_geometry())?
            .map(|b| geometry_ref(&b))
            .transpose()?;
        let grouped = matches!(kind, VisualType::Group | VisualType::BillboardGroup);
        let children = match need(CHILDREN_CHUNK, grouped)? {
            Some(b) => parse_children(&b)?,
            None => Vec::new(),
        };
        let window_list = need(WINDOWS_CHUNK, kind == VisualType::Progressive)?
            .map(|b| parse_progressive_mesh(&b))
            .transpose()?;
        let window_record = need(WINDOW_RECORD_CHUNK, kind == VisualType::ProgressiveTree)?
            .map(|b| {
                <[u8; 4]>::try_from(b.as_slice())
                    .map(u32::from_le_bytes)
                    .map_err(|_| invalid("window record index not 4 bytes"))
            })
            .transpose()?;
        let tree = need(TREE_CHUNK, kind.is_tree())?
            .map(|b| TreeInstance::parse(&b))
            .transpose()?;
        let billboards = need(BILLBOARDS_CHUNK, kind == VisualType::BillboardGroup)?
            .map(|b| Billboards::parse(&b).map(Box::new))
            .transpose()?;
        let (mut geometry_x, mut window_list_x) = (None, None);
        if let Some(c) = chunks.find(GEOMETRY_X_CHUNK) {
            let content = c.content()?;
            let nested = Chunks::parse(&content)?;
            if let Some(g) = nested.find(GEOMETRY_CHUNK) {
                geometry_x = Some(geometry_ref(&g.content()?)?);
            }
            if let Some(w) = nested.find(WINDOWS_CHUNK) {
                window_list_x = Some(parse_progressive_mesh(&w.content()?)?);
            }
        }
        Ok(Self {
            header,
            geometry,
            geometry_x,
            window_list_x,
            children,
            window_list,
            window_record,
            tree,
            billboards,
        })
    }

    /// The detail levels of a progressive visual: its own chunk 6 (type 2) or
    /// its record of `level.geom` chunk 11 (type 11).
    pub fn detail_levels<'a>(
        &'a self,
        records: &'a [ProgressiveMesh],
    ) -> Option<&'a ProgressiveMesh> {
        match self.window_record {
            Some(k) => records.get(k as usize),
            None => self.window_list.as_ref(),
        }
    }
}

/// Chunk 10: `u32 n`, `n × u32`, filling the chunk (exactness our choice).
fn parse_children(b: &[u8]) -> io::Result<Vec<u32>> {
    let (n, rest) = b
        .split_first_chunk::<4>()
        .ok_or_else(|| invalid("group without child count"))?;
    let n = u32::from_le_bytes(*n) as usize;
    if n.checked_mul(4) != Some(rest.len()) {
        return Err(invalid(format!(
            "group of {n} children in {} bytes",
            rest.len()
        )));
    }
    Ok((0..n).map(|i| u32_at(rest, 4 * i)).collect())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::chunks::chunk;

    pub fn header(kind: u8, shader: u16, min: [f32; 3], max: [f32; 3], radius: f32) -> Vec<u8> {
        let mut h = vec![VERSION, kind];
        h.extend(shader.to_le_bytes());
        let centre: [f32; 3] = std::array::from_fn(|i| (min[i] + max[i]) / 2.0);
        for f in min.iter().chain(&max).chain(&centre).chain([&radius]) {
            h.extend(f.to_le_bytes());
        }
        h
    }

    pub fn words(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn parses_each_type() {
        let geometry = words(&[1, 2, 3, 4, 5, 6]);
        let mut mesh = chunk(GEOMETRY_CHUNK, &geometry);
        let mut nested = chunk(GEOMETRY_CHUNK, &words(&[7, 8, 9, 10, 11, 12]));
        nested.extend(chunk(
            WINDOWS_CHUNK,
            &words(&[0, 0, 0, 0, 1, 3, 0x0003_0001]),
        ));
        mesh.extend(chunk(GEOMETRY_X_CHUNK, &nested));
        mesh.extend(chunk(HEADER_CHUNK, &header(0, 5, [0.0; 3], [1.0; 3], 1.0)));
        let v = Visual::parse(&mesh).unwrap();
        assert_eq!(v.header.kind, VisualType::Mesh);
        assert_eq!(v.header.shader, 5);
        assert_eq!(v.geometry.unwrap().index_count, 6);
        assert_eq!(v.geometry_x.unwrap().vertex_buffer, 7);
        assert_eq!(v.window_list_x.unwrap().steps[0].vertex_count, 3);

        let mut group = chunk(HEADER_CHUNK, &header(1, 0, [0.0; 3], [1.0; 3], 1.0));
        group.extend(chunk(CHILDREN_CHUNK, &words(&[2, 0, 1])));
        assert_eq!(Visual::parse(&group).unwrap().children, [0, 1]);

        let mut tree = chunk(HEADER_CHUNK, &header(11, 3, [0.0; 3], [1.0; 3], 1.0));
        tree.extend(chunk(GEOMETRY_CHUNK, &geometry));
        let mut m: Vec<f32> = vec![0.0; 26];
        m[0] = 2.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[12] = 10.0;
        m[13] = 20.0;
        m[15] = 1.0;
        m[16 + 3] = 0.4; // s_hemi
        m[21 + 4] = 0.8; // b_sun
        tree.extend(chunk(
            TREE_CHUNK,
            &m.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>(),
        ));
        tree.extend(chunk(WINDOW_RECORD_CHUNK, &words(&[1])));
        let v = Visual::parse(&tree).unwrap();
        let t = v.tree.unwrap();
        assert_eq!(t.transform_point([1.0, 2.0, 3.0]), [12.0, 22.0, 3.0]);
        assert_eq!(t.transform_normal([1.0, 0.0, 0.0]), [2.0, 0.0, 0.0]);
        assert_eq!(t.base_height(), 20.0);
        assert_eq!(t.c_scale()[3], 0.3);
        assert_eq!(t.c_bias([0.5, 0.25, 1.0]), [0.5, 0.25, 1.0, 0.0]);
        assert_eq!(t.c_sun(), [0.0, 0.6, 0.0, 0.0]);
        assert_eq!(v.window_record, Some(1));
        let records = [ProgressiveMesh::default(), ProgressiveMesh::default()];
        assert!(std::ptr::eq(
            v.detail_levels(&records).unwrap(),
            &records[1]
        ));
    }

    #[test]
    fn rejects_bad_visuals() {
        let h = header(0, 1, [0.0; 3], [1.0; 3], 1.0);
        assert!(
            Visual::parse(&chunk(HEADER_CHUNK, &h)).is_err(),
            "mesh without chunk 21"
        );
        for kind in [3, 10, 12] {
            let h = header(kind, 1, [0.0; 3], [1.0; 3], 1.0);
            assert!(Header::parse(&h).is_err(), "type {kind}");
        }
        let mut v5 = h.clone();
        v5[0] = 5;
        assert!(Header::parse(&v5).is_err());
        assert!(Header::parse(&h[..40]).is_err());
        let mut group = chunk(HEADER_CHUNK, &header(1, 0, [0.0; 3], [1.0; 3], 1.0));
        group.extend(chunk(CHILDREN_CHUNK, &words(&[2, 0])));
        assert!(Visual::parse(&group).is_err(), "child list too short");
    }

    #[test]
    fn billboard_selection() {
        // Face f: a unit square facing direction angle f × 45° (normal
        // pointing outward = −face normal convention of the spec).
        let mut data = Vec::new();
        for f in 0..8 {
            let a = f as f32 * std::f32::consts::FRAC_PI_4;
            let (s, c) = a.sin_cos();
            // Square in the plane perpendicular to (c, 0, s), wound so that
            // the cross products point along −(c, 0, s).
            let right = [-s, 0.0, c];
            let corners = [(-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (1.0, -1.0)];
            for (x, y) in corners {
                let p = [c + x * right[0], y, s + x * right[2]];
                for v in p {
                    data.extend(f32::to_le_bytes(v));
                }
                data.extend([0u8; 16]);
            }
        }
        let b = Billboards::parse(&data).unwrap();
        let n0 = b.face_normal(0);
        // Corner normals point along ±x; the face normal is their negation.
        assert!(
            (n0[0].abs() - 1.0).abs() < 1e-6 && n0[1].abs() < 1e-6,
            "{n0:?}"
        );
        let camera = n0.map(|c| -100.0 * c);
        let choice = b.choose_faces([0.0; 3], camera);
        assert_eq!(choice.f1, 0);
        assert!(choice.f2 == 1 || choice.f2 == 7);
        // F2 and F3 have equal weights (symmetric neighbours): β = 255.
        assert_eq!(choice.beta, 255);

        let h = Header::parse(&header(6, 1, [-1.0, 0.0, -2.0], [1.0, 4.0, 2.0], 2.0)).unwrap();
        // Half-extents 1, 2, 2: m = 2 = R, k = 2 (R² π/2) / (π R²) = 1.
        assert!((coverage_factor(&h) - 1.0).abs() < 1e-6);
        assert_eq!(billboard_fade(1.0, 2.0, 1.0), 255);
        assert_eq!(billboard_fade(2.0, 2.0, 1.0), 0);
        assert_eq!(billboard_fade(1.5, 2.0, 1.0), 127);
    }
}
