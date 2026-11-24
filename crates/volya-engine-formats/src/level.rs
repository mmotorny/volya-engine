// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level directory's main file `levels\<name>\level`
//! (docs/formats/level.md): header, shader/texture table, visuals (in the
//! structure of [`crate::ogf`]), portals, sectors, lights and glows; plus the
//! draw list of a static frame (ogf.md section 8).

use std::io;

use crate::chunks::Chunks;
use crate::cp1251;
use crate::level_geom::LevelGeom;
use crate::ogf::{TreeInstance, Visual, VisualType};

/// Top-level chunk ids (section 1).
pub const HEADER_CHUNK: u32 = 1;
pub const SHADERS_CHUNK: u32 = 2;
pub const VISUALS_CHUNK: u32 = 3;
pub const PORTALS_CHUNK: u32 = 4;
pub const LIGHTS_CHUNK: u32 = 6;
pub const GLOWS_CHUNK: u32 = 7;
pub const SECTORS_CHUNK: u32 = 8;

/// Sub-chunks of a sector (section 6).
const SECTOR_PORTALS: u32 = 1;
const SECTOR_ROOT: u32 = 2;

const VERSION: u16 = 14;
const PORTAL_SIZE: usize = 80;
const LIGHT_SIZE: usize = 108;
const GLOW_SIZE: usize = 18;

/// `D3DLIGHT9` light types (section 7).
pub const LIGHT_POINT: u32 = 1;
pub const LIGHT_SPOT: u32 = 2;
pub const LIGHT_DIRECTIONAL: u32 = 3;

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

/// A shader/texture table entry split at its first `/` (section 3):
/// `(engine shader, texture list)`; `None` for an empty entry ("no
/// shader"). A non-empty entry without `/` is taken as an engine shader with
/// an empty texture list (our choice; no shipped entry lacks the `/`).
pub fn split_shader_entry(entry: &str) -> Option<(&str, &str)> {
    if entry.is_empty() {
        return None;
    }
    Some(entry.split_once('/').unwrap_or((entry, "")))
}

/// The texture names of a texture list (comma-separated).
pub fn textures(list: &str) -> impl Iterator<Item = &str> {
    list.split(',').filter(|t| !t.is_empty())
}

/// A portal record (section 5).
#[derive(Clone, Debug, PartialEq)]
pub struct Portal {
    pub sector_a: u16,
    pub sector_b: u16,
    /// The `count` used vertices.
    pub vertices: Vec<[f32; 3]>,
}

impl Portal {
    /// The plane `(n, d)`: sector A lies where `n·p + d > 0`. Errors when
    /// every triangle is degenerate ("Invalid portal detected").
    pub fn plane(&self) -> io::Result<([f32; 3], f32)> {
        let v = &self.vertices;
        let mut sum = [0.0f32; 3];
        let mut used = 0;
        for i in 1..v.len().saturating_sub(1) {
            let c = cross(sub(v[i], v[0]), sub(v[i + 1], v[i]));
            let len = dot(c, c).sqrt();
            if len <= 1e-7 {
                continue;
            }
            for (s, c) in sum.iter_mut().zip(c) {
                *s += c / len;
            }
            used += 1;
        }
        if used == 0 {
            return Err(invalid("Invalid portal detected"));
        }
        let len = dot(sum, sum).sqrt();
        let n = sum.map(|c| c / len);
        Ok((n, -dot(n, v[0])))
    }

    /// The bounding sphere: centre of the vertices' box and the distance to
    /// its maximum corner.
    pub fn sphere(&self) -> ([f32; 3], f32) {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for p in &self.vertices {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let c: [f32; 3] = std::array::from_fn(|i| (min[i] + max[i]) / 2.0);
        let d = sub(max, c);
        (c, dot(d, d).sqrt())
    }
}

/// A sector (section 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sector {
    /// Indices into the portals.
    pub portals: Vec<u16>,
    /// Index of the root visual.
    pub root: u32,
}

/// The fields of a light record the first renderer uses (section 7).
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    /// The leading u32 (meaning unestablished).
    pub leading: u32,
    /// `D3DLIGHT9` type: [`LIGHT_POINT`], [`LIGHT_SPOT`] or
    /// [`LIGHT_DIRECTIONAL`].
    pub kind: u32,
    /// Diffuse colour (r, g, b, a).
    pub diffuse: [f32; 4],
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub range: f32,
}

/// A glow record (section 8).
#[derive(Clone, Debug, PartialEq)]
pub struct Glow {
    pub position: [f32; 3],
    pub radius: f32,
    /// Shader index into the shader/texture table.
    pub shader: u16,
}

/// A parsed `level` file.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    /// Format version (14) and the second u16 (meaning unestablished).
    pub header: (u16, u16),
    /// The shader/texture table; entry 0 is empty in the shipped levels.
    pub shaders: Vec<String>,
    pub visuals: Vec<Visual>,
    pub portals: Vec<Portal>,
    pub sectors: Vec<Sector>,
    pub lights: Vec<Light>,
    pub glows: Vec<Glow>,
}

impl Level {
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(file)?;
        let content = |id: u32| -> io::Result<Vec<u8>> {
            Ok(chunks
                .find(id)
                .map(|c| c.content())
                .transpose()?
                .map(|c| c.into_owned())
                .unwrap_or_default())
        };
        let header = chunks.require(HEADER_CHUNK, "level")?.content()?;
        let header: [u8; 4] = header
            .as_ref()
            .try_into()
            .map_err(|_| invalid("level header not 4 bytes"))?;
        let header = (
            u16::from_le_bytes([header[0], header[1]]),
            u16::from_le_bytes([header[2], header[3]]),
        );
        if header.0 != VERSION {
            return Err(invalid(format!("level version {}", header.0)));
        }
        let shaders = chunks
            .require(SHADERS_CHUNK, "Level doesn't builded correctly.")?
            .content()?;
        let shaders = parse_shaders(&shaders)?;

        let visuals_chunk = chunks.require(VISUALS_CHUNK, "level")?.content()?;
        let visuals: Vec<Visual> = Chunks::parse(&visuals_chunk)?
            .list()
            .into_iter()
            .map(|c| {
                Visual::parse(&c.content()?).map_err(|e| invalid(format!("visual {}: {e}", c.id())))
            })
            .collect::<io::Result<_>>()?;
        for (i, v) in visuals.iter().enumerate() {
            if v.children.iter().any(|&c| c as usize >= visuals.len()) {
                return Err(invalid(format!("visual {i}: child out of range")));
            }
        }

        // Portals and lights are counted by size, so a missing chunk has no
        // records (chunks.md section 3).
        let portals = parse_portals(&content(PORTALS_CHUNK)?)?;
        let lights = parse_lights(&content(LIGHTS_CHUNK)?)?;
        if !lights.iter().any(|l| l.kind == LIGHT_DIRECTIONAL) {
            return Err(invalid("Where is sun?"));
        }
        let glows = parse_glows(&content(GLOWS_CHUNK)?)?;
        let sectors_chunk = chunks.require(SECTORS_CHUNK, "level")?.content()?;
        let sectors = parse_sectors(&sectors_chunk)?;
        for (s, sector) in sectors.iter().enumerate() {
            if sector.root as usize >= visuals.len()
                || sector
                    .portals
                    .iter()
                    .any(|&p| usize::from(p) >= portals.len())
            {
                return Err(invalid(format!("sector {s}: index out of range")));
            }
        }
        Ok(Self {
            header,
            shaders,
            visuals,
            portals,
            sectors,
            lights,
            glows,
        })
    }

    /// The draw list of a static frame without the camera-dependent parts
    /// (ogf.md section 8.3, last paragraph): every sector visited with the
    /// full view (level.md section 9.2's superset), groups descended,
    /// billboard groups descended to their trees and their billboards not
    /// drawn, meshes and trees whole, progressive visuals at level 0. A
    /// visual reached twice is drawn once (level.md section 9.3).
    pub fn static_draw_items(&self, geom: &LevelGeom) -> io::Result<Vec<DrawItem>> {
        let mut visited = vec![false; self.visuals.len()];
        let mut stack: Vec<u32> = self.sectors.iter().rev().map(|s| s.root).collect();
        let mut items = Vec::new();
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut visited[i as usize], true) {
                continue;
            }
            let v = &self.visuals[i as usize];
            match v.header.kind {
                VisualType::Group | VisualType::BillboardGroup => {
                    stack.extend(v.children.iter().rev());
                }
                _ => items.push(self.draw_item(i, v, geom)?),
            }
        }
        Ok(items)
    }

    fn draw_item(&self, index: u32, v: &Visual, geom: &LevelGeom) -> io::Result<DrawItem> {
        let what = |m: &str| invalid(format!("visual {index}: {m}"));
        let g = v.geometry.ok_or_else(|| what("no geometry"))?;
        let (offset, triangles, vertices) = if v.header.kind.is_progressive() {
            let level0 = v
                .detail_levels(&geom.progressive_meshes)
                .and_then(|w| w.steps.first())
                .ok_or_else(|| what("no detail levels"))?;
            (
                level0.index_offset,
                u32::from(level0.triangle_count),
                u32::from(level0.vertex_count),
            )
        } else {
            (0, g.index_count / 3, g.vertex_count)
        };
        let first_index = g.first_index.checked_add(offset);
        let in_range = first_index.is_some_and(|f| {
            geom.index_buffers
                .get(g.index_buffer as usize)
                .is_some_and(|ib| u64::from(f) + 3 * u64::from(triangles) <= ib.len() as u64)
        }) && geom
            .vertex_buffers
            .get(g.vertex_buffer as usize)
            .is_some_and(|vb| {
                u64::from(g.first_vertex) + u64::from(vertices) <= u64::from(vb.vertex_count)
            });
        if !in_range || vertices > g.vertex_count {
            return Err(what("geometry range outside its buffers"));
        }
        Ok(DrawItem {
            visual: index,
            kind: v.header.kind,
            shader: v.header.shader,
            vertex_buffer: g.vertex_buffer,
            first_vertex: g.first_vertex,
            vertex_count: vertices,
            index_buffer: g.index_buffer,
            first_index: first_index.unwrap(),
            triangle_count: triangles,
            transform: v.tree.map_or(IDENTITY, |t| t.matrix()),
            tree: v.tree,
        })
    }
}

const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// One draw of a static frame: `triangle_count` triangles from index
/// buffer `index_buffer` starting at `first_index`, each index relative to
/// `first_vertex` of vertex buffer `vertex_buffer` (`level.geom`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawItem {
    pub visual: u32,
    pub kind: VisualType,
    /// Index into [`Level::shaders`].
    pub shader: u16,
    pub vertex_buffer: u32,
    pub first_vertex: u32,
    /// Vertices the draw uses, from `first_vertex`.
    pub vertex_count: u32,
    pub index_buffer: u32,
    pub first_index: u32,
    pub triangle_count: u32,
    /// World transform, row-vector convention: identity for meshes (world
    /// coordinates), the tree instance's transform for trees.
    pub transform: [[f32; 4]; 4],
    /// The tree instance (per-instance lighting, ogf.md section 5).
    pub tree: Option<TreeInstance>,
}

/// Chunk 2: `u32 count`, then `count` zero-terminated strings filling the
/// chunk.
fn parse_shaders(b: &[u8]) -> io::Result<Vec<String>> {
    let (count, mut rest) = b
        .split_first_chunk::<4>()
        .ok_or_else(|| invalid("shader table without count"))?;
    let count = u32::from_le_bytes(*count);
    let mut out = Vec::new();
    for _ in 0..count {
        let end = rest
            .iter()
            .position(|&c| c == 0)
            .ok_or_else(|| invalid("shader table: unterminated string"))?;
        out.push(cp1251::decode(&rest[..end]));
        rest = &rest[end + 1..];
    }
    if !rest.is_empty() {
        return Err(invalid("shader table: bytes after the strings"));
    }
    Ok(out)
}

fn parse_portals(b: &[u8]) -> io::Result<Vec<Portal>> {
    let (records, rest) = b.as_chunks::<PORTAL_SIZE>();
    if !rest.is_empty() {
        return Err(invalid("portal chunk not a multiple of 80 bytes"));
    }
    records
        .iter()
        .map(|r| {
            let count = u32_at(r, 76) as usize;
            if count > 6 {
                return Err(invalid(format!("portal of {count} vertices")));
            }
            Ok(Portal {
                sector_a: u16::from_le_bytes([r[0], r[1]]),
                sector_b: u16::from_le_bytes([r[2], r[3]]),
                vertices: (0..count).map(|i| float3(r, 4 + 12 * i)).collect(),
            })
        })
        .collect()
}

/// Chunk 6: size / 108 records. The spec does not say a remainder is
/// checked; we reject one.
fn parse_lights(b: &[u8]) -> io::Result<Vec<Light>> {
    let (records, rest) = b.as_chunks::<LIGHT_SIZE>();
    if !rest.is_empty() {
        return Err(invalid("light chunk not a multiple of 108 bytes"));
    }
    Ok(records
        .iter()
        .map(|r| Light {
            leading: u32_at(r, 0),
            kind: u32_at(r, 4),
            diffuse: std::array::from_fn(|i| f32_at(r, 8 + 4 * i)),
            position: float3(r, 56),
            direction: float3(r, 68),
            range: f32_at(r, 80),
        })
        .collect())
}

fn parse_glows(b: &[u8]) -> io::Result<Vec<Glow>> {
    let (records, rest) = b.as_chunks::<GLOW_SIZE>();
    if b.is_empty() || !rest.is_empty() {
        return Err(invalid("glow chunk empty or not a multiple of 18 bytes"));
    }
    Ok(records
        .iter()
        .map(|r| Glow {
            position: float3(r, 0),
            radius: f32_at(r, 12),
            shader: u16::from_le_bytes([r[16], r[17]]),
        })
        .collect())
}

fn parse_sectors(b: &[u8]) -> io::Result<Vec<Sector>> {
    Chunks::parse(b)?
        .list()
        .into_iter()
        .map(|c| {
            let content = c.content()?;
            let chunks = Chunks::parse(&content)?;
            let portals = match chunks.find(SECTOR_PORTALS) {
                Some(p) => {
                    let p = p.content()?;
                    let (list, rest) = p.as_chunks::<2>();
                    if !rest.is_empty() {
                        return Err(invalid("sector portal list of odd size"));
                    }
                    list.iter().map(|i| u16::from_le_bytes(*i)).collect()
                }
                None => Vec::new(),
            };
            let root = chunks.require(SECTOR_ROOT, "sector")?.content()?;
            let root: [u8; 4] = root
                .as_ref()
                .try_into()
                .map_err(|_| invalid("sector root not 4 bytes"))?;
            Ok(Sector {
                portals,
                root: u32::from_le_bytes(root),
            })
        })
        .collect()
}

/// Screen-size thresholds of a frame (section 9.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thresholds {
    pub t_min: f32,
    pub t_a: f32,
    pub t_b: f32,
    pub p_lo: f32,
    pub p_hi: f32,
}

/// Console defaults (section 9.4).
pub const GEOMETRY_LOD: f32 = 1.0;
pub const SSA_LOD_A: f32 = 64.0;
pub const SSA_LOD_B: f32 = 48.0;

impl Thresholds {
    /// For a `width` × `height` target, field of view `fov` in degrees and
    /// the three console settings.
    pub fn new(
        width: u32,
        height: u32,
        fov: f32,
        geometry_lod: f32,
        ssa_lod_a: f32,
        ssa_lod_b: f32,
    ) -> Self {
        let s = (geometry_lod + 1e-7) * (90.0 / fov).powi(2) * width as f32 * height as f32;
        Self {
            t_min: 3.5f32.powi(2) / s,
            t_a: (ssa_lod_a / 3.0).powi(2) / s,
            t_b: (ssa_lod_b / 3.0).powi(2) / s,
            p_lo: (16.0f32 / 0.75).powi(2) / s,
            p_hi: (64.0f32 / 0.75).powi(2) / s,
        }
    }

    /// Detail value `q` of a progressive visual with screen measure `sigma`
    /// (ogf.md section 8.3).
    pub fn detail(&self, sigma: f32) -> f32 {
        ((sigma - self.p_lo) / (self.p_hi - self.p_lo))
            .clamp(0.0, 1.0)
            .sqrt()
    }
}

/// Screen measure `σ = R / (D² + 1e−5)` of a visual (ogf.md section 8.3);
/// billboard groups multiply it by their coverage factor.
pub fn screen_measure(radius: f32, centre: [f32; 3], camera: [f32; 3]) -> f32 {
    let d = sub(centre, camera);
    radius / (dot(d, d) + 1e-5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::chunk;
    use crate::ogf::tests::{header, words};

    fn floats(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// A level of two sectors: sector 0's root group 2 holds mesh 0 and
    /// group 1 → mesh 0 again (drawn once); sector 1's root is billboard
    /// group 4 with tree 3.
    fn sample() -> Vec<u8> {
        let mut shaders = 3u32.to_le_bytes().to_vec();
        shaders.extend(b"\0default/a,lmap#1_1\0details\\lod/level_lods\0");
        let mut mesh = chunk(1, &header(0, 1, [0.0; 3], [1.0; 3], 1.0));
        mesh.extend(chunk(21, &words(&[0, 0, 3, 0, 0, 3])));
        let mut group1 = chunk(1, &header(1, 0, [0.0; 3], [1.0; 3], 1.0));
        group1.extend(chunk(10, &words(&[1, 0])));
        let mut group2 = chunk(1, &header(1, 0, [0.0; 3], [1.0; 3], 1.0));
        group2.extend(chunk(10, &words(&[2, 0, 1])));
        let mut tree = chunk(1, &header(7, 1, [0.0; 3], [1.0; 3], 1.0));
        tree.extend(chunk(21, &words(&[0, 0, 3, 0, 0, 3])));
        let mut m = vec![0.0f32; 26];
        for i in [0, 5, 10, 15] {
            m[i] = 1.0;
        }
        m[12] = 5.0;
        tree.extend(chunk(12, &floats(&m)));
        let mut bb = chunk(1, &header(6, 2, [0.0; 3], [1.0; 3], 1.0));
        bb.extend(chunk(10, &words(&[1, 3])));
        bb.extend(chunk(11, &[0; 896]));
        let mut visuals = Vec::new();
        for (i, v) in [mesh, group1, group2, tree, bb].iter().enumerate() {
            visuals.extend(chunk(i as u32, v));
        }
        let mut portal = vec![0u8; 80];
        portal[0] = 0;
        portal[2] = 1;
        let quad = [
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        portal[4..52].copy_from_slice(&floats(&quad.concat()));
        portal[76] = 4;
        let mut lights = vec![0u8; 108];
        lights[4] = 3;
        lights[68..80].copy_from_slice(&floats(&[0.0, -1.0, 0.0]));
        let mut glow = floats(&[1.0, 2.0, 3.0, 0.5]);
        glow.extend(1u16.to_le_bytes());
        let mut sector0 = chunk(1, &[0, 0]);
        sector0.extend(chunk(2, &words(&[2])));
        let mut sector1 = chunk(1, &[0, 0]);
        sector1.extend(chunk(2, &words(&[4])));
        let mut sectors = chunk(0, &sector0);
        sectors.extend(chunk(1, &sector1));

        let mut file = chunk(1, &[14, 0, 1, 0]);
        file.extend(chunk(4, &portal));
        file.extend(chunk(6, &lights));
        file.extend(chunk(7, &glow));
        file.extend(chunk(3, &visuals));
        file.extend(chunk(2, &shaders));
        file.extend(chunk(8, &sectors));
        file
    }

    fn geom() -> LevelGeom {
        use crate::level_geom::{ProgressiveMesh, VertexBuffer};
        LevelGeom {
            header: None,
            vertex_buffers: vec![VertexBuffer {
                elements: Vec::new(),
                layout: None,
                stride: 12,
                vertex_count: 3,
                data: vec![0; 36],
            }],
            index_buffers: vec![vec![0, 1, 2]],
            progressive_meshes: vec![ProgressiveMesh::default()],
        }
    }

    #[test]
    fn parses_a_level() {
        let level = Level::parse(&sample()).unwrap();
        assert_eq!(level.header, (14, 1));
        assert_eq!(level.shaders.len(), 3);
        assert_eq!(
            split_shader_entry(&level.shaders[1]),
            Some(("default", "a,lmap#1_1"))
        );
        assert_eq!(split_shader_entry(""), None);
        assert_eq!(split_shader_entry("a/b/c"), Some(("a", "b/c")));
        assert_eq!(
            textures("a,lmap#1_1").collect::<Vec<_>>(),
            ["a", "lmap#1_1"]
        );
        assert_eq!(level.visuals.len(), 5);
        assert_eq!(
            level.sectors[1],
            Sector {
                portals: vec![0],
                root: 4
            }
        );
        assert_eq!(level.glows[0].shader, 1);
        let (n, d) = level.portals[0].plane().unwrap();
        // (v1 − v0) × (v2 − v1) = (0, 1, 0) × (0, 0, 1) = (1, 0, 0).
        assert_eq!((n, d), ([1.0, 0.0, 0.0], 0.0));
        let (c, r) = level.portals[0].sphere();
        assert_eq!(c, [0.0, 0.5, 0.5]);
        assert!((r - 0.5f32.sqrt()).abs() < 1e-6);
        assert!(
            Portal {
                sector_a: 0,
                sector_b: 0,
                vertices: vec![[0.0; 3]; 3]
            }
            .plane()
            .is_err()
        );

        let items = level.static_draw_items(&geom()).unwrap();
        let visuals: Vec<u32> = items.iter().map(|i| i.visual).collect();
        assert_eq!(visuals, [0, 3], "mesh once, tree, no billboard");
        assert_eq!(items[0].transform, IDENTITY);
        assert_eq!(items[1].transform[3][0], 5.0);
        assert_eq!(items[0].triangle_count, 1);
    }

    #[test]
    fn rejects_broken_levels() {
        let file = sample();
        let chunks = Chunks::parse(&file).unwrap();
        let without = |id: u32| -> Vec<u8> {
            chunks
                .iter()
                .filter(|c| c.id() != id)
                .flat_map(|c| chunk(c.id_word, c.payload))
                .collect()
        };
        assert!(Level::parse(&without(SHADERS_CHUNK)).is_err());
        assert!(Level::parse(&without(GLOWS_CHUNK)).is_err());
        assert!(Level::parse(&without(LIGHTS_CHUNK)).is_err(), "no sun");
        assert!(
            Level::parse(&without(PORTALS_CHUNK)).is_err(),
            "sector lists portal 0"
        );
        let mut v13 = file.clone();
        v13[8] = 13;
        assert!(Level::parse(&v13).is_err());
    }

    #[test]
    fn thresholds() {
        let t = Thresholds::new(1024, 768, 90.0, GEOMETRY_LOD, SSA_LOD_A, SSA_LOD_B);
        let s = (1.0 + 1e-7) * 1024.0 * 768.0;
        assert!((t.t_min - 12.25 / s).abs() < 1e-12);
        assert!((t.p_lo * s - 455.11).abs() < 0.01);
        assert!((t.p_hi * s - 7281.78).abs() < 0.01);
        assert!(t.t_b < t.t_a);
        assert_eq!(t.detail(0.0), 0.0);
        assert_eq!(t.detail(1.0), 1.0);
        assert_eq!(
            screen_measure(2.0, [0.0, 0.0, 10.0], [0.0; 3]),
            2.0 / (100.0 + 1e-5)
        );
    }
}
