// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Standalone model files (`.ogf`) of the level's spawned objects
//! (docs/formats/skeleton-ogf.md, cited by section): skeleton models (visual
//! types 10 and 3) holding skinned meshes (types 5 and 4) with 1-link or
//! 2-link vertices, bones with their bind pose, how the first renderer
//! stores the vertices (docs/behavior/model-shading.md 1), and the object
//! transform of docs/behavior/object-visual-and-transform.md 4, whose
//! rotation the bind pose shares.
//!
//! Chunks a model needs for drawing at rest are required; the description
//! and motions (chunks 14, 15, 18, 19) are not read, the user data (chunk
//! 17) only by [`user_data`].

use std::io;

use crate::bone_physics::{self, BonePhysics};
use crate::chunks::Chunks;
use crate::level_geom::{ProgressiveMesh, parse_progressive_mesh};
use crate::packet::{Reader, invalid};

/// Top-level and child chunk ids (sections 3, 5).
pub const HEADER_CHUNK: u32 = 1;
pub const TEXTURE_CHUNK: u32 = 2;
pub const VERTICES_CHUNK: u32 = 3;
pub const INDICES_CHUNK: u32 = 4;
pub const DETAIL_WINDOWS_CHUNK: u32 = 6;
pub const CHILDREN_CHUNK: u32 = 9;
pub const BONE_NAMES_CHUNK: u32 = 13;
pub const BONE_DATA_CHUNK: u32 = 16;
/// LTX text (section 3; read by torches, docs/behavior/torch.md 1.2).
pub const USER_DATA_CHUNK: u32 = 17;

/// Vertex formats of skinned meshes (section 6).
pub const ONE_LINK: u32 = 0x1207_1980;
pub const TWO_LINK: u32 = 0x240E_3300;

/// The only header version (ogf.md section 2).
const VERSION: u8 = 4;
const HEADER_SIZE: usize = 44;
const ONE_LINK_SIZE: usize = 60;
const TWO_LINK_SIZE: usize = 64;

/// Row-vector convention (`[p 1] · M`), rows stored in order.
pub type Mat4 = [[f32; 4]; 4];

/// Visual types of a model file (section 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelType {
    /// Type 3: draws exactly like a skeleton model at rest (model-shading.md 2).
    AnimatedSkeleton = 3,
    /// Type 10.
    Skeleton = 10,
}

/// Visual types of a skeleton model's child meshes (section 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MeshType {
    /// Type 4: with detail windows (chunk 6).
    SkinnedProgressive = 4,
    /// Type 5.
    Skinned = 5,
}

/// Chunk 1 of a model file or a child mesh, the layout of ogf.md section 2.
/// Bounds are in model space (bind pose, section 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    /// Byte 1 as stored.
    pub kind: u8,
    /// Bytes 2–3: 0 in every model file and not used (section 1).
    pub shader: u16,
    pub box_min: [f32; 3],
    pub box_max: [f32; 3],
    pub sphere_centre: [f32; 3],
    pub sphere_radius: f32,
}

impl Header {
    pub fn parse(b: &[u8]) -> io::Result<Self> {
        if b.len() != HEADER_SIZE {
            return Err(invalid(format!("model header of {} bytes", b.len())));
        }
        if b[0] != VERSION {
            return Err(invalid(format!("model header version {}", b[0])));
        }
        let mut r = Reader::new(&b[2..]);
        Ok(Self {
            kind: b[1],
            shader: r.u16()?,
            box_min: r.float3()?,
            box_max: r.float3()?,
            sphere_centre: r.float3()?,
            sphere_radius: r.float()?,
        })
    }
}

/// Which bones a mesh's vertices link to (section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Links {
    /// Format 0x12071980: one bone per vertex.
    One,
    /// Format 0x240E3300: two bones, the second weighted.
    Two,
}

/// One vertex of a skinned mesh as stored (section 6). 1-link vertices
/// have `bones = [b, b]` and weight 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinnedVertex {
    /// Model space, bind pose.
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub binormal: [f32; 3],
    /// Direct3D convention (v grows downward).
    pub uv: [f32; 2],
    /// `b` (1-link) or `b₀, b₁` (2-link).
    pub bones: [u16; 2],
    /// Weight `w` of `b₁` (2-link); 0 for 1-link.
    pub weight: f32,
}

/// A child mesh of a skeleton model (section 5).
#[derive(Clone, Debug, PartialEq)]
pub struct SkinnedMesh {
    pub header: Header,
    pub kind: MeshType,
    /// Chunk 2: the texture name, then the engine shader name.
    pub texture: String,
    pub shader: String,
    pub links: Links,
    pub vertices: Vec<SkinnedVertex>,
    /// A triangle list, 0-based into this mesh's vertices.
    pub indices: Vec<u16>,
    /// Chunk 6 (type 4).
    pub windows: Option<ProgressiveMesh>,
}

impl SkinnedMesh {
    /// The indices of the full-detail mesh: all of them (type 5), or the
    /// window of detail level 0 (type 4, model-shading.md 3).
    pub fn full_detail_indices(&self) -> &[u16] {
        match self.windows.as_ref().and_then(|w| w.steps.first()) {
            Some(s) => {
                let start = s.index_offset as usize;
                &self.indices[start..start + 3 * usize::from(s.triangle_count)]
            }
            None => &self.indices,
        }
    }

    /// The indices any detail window draws, as a range of
    /// [`SkinnedMesh::indices`]: all of them (type 5), or from the first
    /// window's start to the last window's end (type 4, object-detail-level.md
    /// 4).
    pub fn detail_span(&self) -> std::ops::Range<usize> {
        match &self.windows {
            Some(w) if !w.steps.is_empty() => {
                let start = w.steps.iter().map(|s| s.index_offset as usize);
                let end = (w.steps.iter())
                    .map(|s| s.index_offset as usize + 3 * usize::from(s.triangle_count));
                start.min().unwrap_or(0)..end.max().unwrap_or(0)
            }
            _ => 0..self.indices.len(),
        }
    }

    /// The largest bone index any vertex names.
    pub fn largest_bone(&self) -> u16 {
        self.vertices
            .iter()
            .map(|v| match self.links {
                Links::One => v.bones[0],
                Links::Two => v.bones[0].max(v.bones[1]),
            })
            .max()
            .unwrap_or(0)
    }

    /// How the first renderer stores the mesh's vertices (model-shading.md 1) on a
    /// device reporting `constants` vertex shader constants. Software
    /// skinning replaces the hardware 1-link and 2-link modes only (our
    /// reading of "instead of either hardware mode").
    pub fn render_mode(&self, constants: u32) -> RenderMode {
        let single = self.links == Links::One
            && self
                .vertices
                .windows(2)
                .all(|w| w[0].bones[0] == w[1].bones[0]);
        if single {
            return RenderMode::SingleBone;
        }
        if u32::from(self.largest_bone()) > constants.saturating_sub(22) / 3 {
            return RenderMode::Software;
        }
        match self.links {
            Links::One => RenderMode::OneLink,
            Links::Two => RenderMode::TwoLink,
        }
    }
}

/// The modes of model-shading.md 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RenderMode {
    SingleBone,
    /// Hardware 1-link.
    OneLink,
    /// Hardware 2-link.
    TwoLink,
    /// Floats, skinned on the CPU.
    Software,
}

/// A bone of chunks 13 and 16 (section 4).
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    /// Lower-case in the data.
    pub name: String,
    /// Index of the parent bone; `None` for the root.
    pub parent: Option<usize>,
    /// Chunk 13's box in the bone's own bind space: rotation rows, centre,
    /// half-size.
    pub box_rotation: [[f32; 3]; 3],
    pub box_centre: [f32; 3],
    pub box_half_size: [f32; 3],
    /// Chunk 16's bind rotation `(r₀, r₁, r₂)` and position `t`.
    pub bind_rotation: [f32; 3],
    pub bind_position: [f32; 3],
    /// The rest of chunk 16's record (docs/formats/bone-physics.md).
    pub physics: BonePhysics,
}

/// A skeleton model (types 10 and 3, section 3).
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub header: Header,
    pub kind: ModelType,
    /// Chunk 9's sub-chunks 0, 1, 2, … in order.
    pub meshes: Vec<SkinnedMesh>,
    pub bones: Vec<Bone>,
}

impl Model {
    /// Parses a model file. Types other than 10 and 3 are not models of
    /// spawned objects (section 1: no record references them).
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(file)?;
        let header = Header::parse(&chunks.require(HEADER_CHUNK, "model")?.content()?)?;
        let kind = match header.kind {
            3 => ModelType::AnimatedSkeleton,
            10 => ModelType::Skeleton,
            k => return Err(invalid(format!("visual type {k} is not a skeleton model"))),
        };
        let children = chunks
            .require(CHILDREN_CHUNK, "skeleton model")?
            .content()?;
        let children = Chunks::parse(&children)?;
        let meshes = children
            .list()
            .into_iter()
            .map(|c| SkinnedMesh::parse(&c.content()?))
            .collect::<io::Result<Vec<_>>>()?;
        let names = chunks
            .require(BONE_NAMES_CHUNK, "skeleton model")?
            .content()?;
        let data = chunks
            .require(BONE_DATA_CHUNK, "skeleton model")?
            .content()?;
        let mut bones = parse_bones(&names, &data)?;
        for m in &meshes {
            if usize::from(m.largest_bone()) >= bones.len() {
                return Err(invalid(format!(
                    "bone index {} of {} bones",
                    m.largest_bone(),
                    bones.len()
                )));
            }
        }
        check_breakable(&mut bones, &meshes);
        Ok(Self {
            header,
            kind,
            meshes,
            bones,
        })
    }

    /// The bone named `name`.
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// Each bone's model-space bind matrix `B` (section 4).
    pub fn bind_pose(&self) -> io::Result<Vec<Mat4>> {
        bind_pose(&self.bones)
    }
}

/// A model file's header and bones only (chunks 1, 13, 16), its meshes
/// not read: for what needs a skeleton without drawing it. Chunk 16 is as
/// stored: the load-time check of breakable bones, which needs the meshes,
/// is [`Model::parse`]'s.
pub fn skeleton(file: &[u8]) -> io::Result<(Header, Vec<Bone>)> {
    let chunks = Chunks::parse(file)?;
    let header = Header::parse(&chunks.require(HEADER_CHUNK, "model")?.content()?)?;
    let names = chunks
        .require(BONE_NAMES_CHUNK, "skeleton model")?
        .content()?;
    let data = chunks
        .require(BONE_DATA_CHUNK, "skeleton model")?
        .content()?;
    Ok((header, parse_bones(&names, &data)?))
}

/// A model file's user data (chunk 17): its text, up to a terminating NUL
/// if it has one; `None` without the chunk.
pub fn user_data(file: &[u8]) -> io::Result<Option<Vec<u8>>> {
    let chunks = Chunks::parse(file)?;
    let Some(chunk) = chunks.find(USER_DATA_CHUNK) else {
        return Ok(None);
    };
    let content = chunk.content()?;
    let end = content
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(content.len());
    Ok(Some(content[..end].to_vec()))
}

impl SkinnedMesh {
    /// Parses one child mesh (section 5).
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(data)?;
        let header = Header::parse(&chunks.require(HEADER_CHUNK, "skinned mesh")?.content()?)?;
        let kind = match header.kind {
            4 => MeshType::SkinnedProgressive,
            5 => MeshType::Skinned,
            k => {
                return Err(invalid(format!(
                    "child visual type {k} is not a skinned mesh"
                )));
            }
        };
        let names = chunks.require(TEXTURE_CHUNK, "skinned mesh")?.content()?;
        let mut r = Reader::new(&names);
        let texture = r.string()?;
        let shader = r.string()?;
        let vertices = chunks.require(VERTICES_CHUNK, "skinned mesh")?.content()?;
        let (links, vertices) = parse_vertices(&vertices)?;
        let indices = chunks.require(INDICES_CHUNK, "skinned mesh")?.content()?;
        let indices = parse_indices(&indices)?;
        if let Some(&i) = indices.iter().find(|&&i| usize::from(i) >= vertices.len()) {
            return Err(invalid(format!("index {i} of {} vertices", vertices.len())));
        }
        let windows = match kind {
            MeshType::SkinnedProgressive => {
                let w = chunks.require(DETAIL_WINDOWS_CHUNK, "skinned progressive mesh")?;
                let w = parse_progressive_mesh(&w.content()?)?;
                for s in &w.steps {
                    // u64: on wasm32 a usize sum of a malformed offset wraps.
                    let end = u64::from(s.index_offset) + 3 * u64::from(s.triangle_count);
                    if end > indices.len() as u64 {
                        return Err(invalid(format!(
                            "detail window ends at {end} of {} indices",
                            indices.len()
                        )));
                    }
                }
                if w.steps.is_empty() {
                    return Err(invalid("skinned progressive mesh without detail levels"));
                }
                Some(w)
            }
            MeshType::Skinned => None,
        };
        Ok(Self {
            header,
            kind,
            texture,
            shader,
            links,
            vertices,
            indices,
            windows,
        })
    }
}

/// Chunk 3: `u32` format, `u32` count, then the records (section 6).
fn parse_vertices(b: &[u8]) -> io::Result<(Links, Vec<SkinnedVertex>)> {
    let mut r = Reader::new(b);
    let format = r.u32()?;
    let count = r.u32()? as usize;
    let (links, size) = match format {
        ONE_LINK => (Links::One, ONE_LINK_SIZE),
        TWO_LINK => (Links::Two, TWO_LINK_SIZE),
        f => {
            return Err(invalid(format!(
                "Invalid vertex type in skinned model ({f:#x})"
            )));
        }
    };
    let data = r.take(
        count
            .checked_mul(size)
            .ok_or_else(|| invalid("vertex count overflows"))?,
    )?;
    r.finish("skinned vertices")?;
    let bone =
        |v: u32| u16::try_from(v).map_err(|_| invalid(format!("bone index {v} beyond 65535")));
    let vertices = data
        .chunks_exact(size)
        .map(|v| {
            let mut r = Reader::new(v);
            Ok(match links {
                Links::One => {
                    let position = r.float3()?;
                    let normal = r.float3()?;
                    let tangent = r.float3()?;
                    let binormal = r.float3()?;
                    let uv = [r.float()?, r.float()?];
                    let b = bone(r.u32()?)?;
                    SkinnedVertex {
                        position,
                        normal,
                        tangent,
                        binormal,
                        uv,
                        bones: [b, b],
                        weight: 0.0,
                    }
                }
                Links::Two => {
                    let bones = [r.u16()?, r.u16()?];
                    let position = r.float3()?;
                    let normal = r.float3()?;
                    let tangent = r.float3()?;
                    let binormal = r.float3()?;
                    let weight = r.float()?;
                    let uv = [r.float()?, r.float()?];
                    SkinnedVertex {
                        position,
                        normal,
                        tangent,
                        binormal,
                        uv,
                        bones,
                        weight,
                    }
                }
            })
        })
        .collect::<io::Result<_>>()?;
    Ok((links, vertices))
}

/// Chunk 4: `u32` count (a multiple of 3), then that many `u16`.
fn parse_indices(b: &[u8]) -> io::Result<Vec<u16>> {
    let mut r = Reader::new(b);
    let count = r.u32()? as usize;
    if !count.is_multiple_of(3) {
        return Err(invalid(format!("{count} indices, not a triangle list")));
    }
    let data = r.take(
        count
            .checked_mul(2)
            .ok_or_else(|| invalid("index count overflows"))?,
    )?;
    r.finish("skinned indices")?;
    Ok(data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&i| u16::from_le_bytes(i))
        .collect())
}

/// Chunks 13 and 16 (section 4): one root, every parent named, chunk 16
/// one record per bone consumed exactly.
fn parse_bones(names: &[u8], data: &[u8]) -> io::Result<Vec<Bone>> {
    let mut r = Reader::new(names);
    let count = r.u32()? as usize;
    let mut named = Vec::with_capacity(count.min(names.len()));
    for _ in 0..count {
        let name = r.string()?;
        let parent = r.string()?;
        let mut f = [0.0f32; 15];
        for v in &mut f {
            *v = r.float()?;
        }
        named.push((name, parent, f));
    }
    r.finish("bone names")?;
    let mut d = Reader::new(data);
    let mut bones = Vec::with_capacity(named.len());
    for (name, parent_name, f) in &named {
        let (physics, bind_rotation, bind_position) =
            bone_physics::read_record(&mut d).map_err(|e| invalid(format!("bone {name}: {e}")))?;
        let parent = if parent_name.is_empty() {
            None
        } else {
            Some(
                named
                    .iter()
                    .position(|(n, _, _)| n == parent_name)
                    .ok_or_else(|| invalid(format!("bone {name}: no parent {parent_name}")))?,
            )
        };
        bones.push(Bone {
            name: name.clone(),
            parent,
            box_rotation: [[f[0], f[1], f[2]], [f[3], f[4], f[5]], [f[6], f[7], f[8]]],
            box_centre: [f[9], f[10], f[11]],
            box_half_size: [f[12], f[13], f[14]],
            bind_rotation,
            bind_position,
            physics,
        });
    }
    d.finish("bone data")?;
    let roots = bones.iter().filter(|b| b.parent.is_none()).count();
    if roots != 1 {
        return Err(invalid(format!("{roots} root bones")));
    }
    Ok(bones)
}

/// The load-time check of breakable bones (bone-physics.md 3.4): a child
/// mesh's bones are those its triangles' vertices name, both of a 2-link
/// vertex whatever the weight.
fn check_breakable(bones: &mut [Bone], meshes: &[SkinnedMesh]) {
    let parents: Vec<_> = bones.iter().map(|b| b.parent).collect();
    let mut physics: Vec<_> = bones.iter_mut().map(|b| &mut b.physics).collect();
    let used = meshes.iter().map(|m| {
        m.indices.iter().flat_map(move |&i| {
            let v = &m.vertices[usize::from(i)];
            match m.links {
                Links::One => vec![usize::from(v.bones[0])],
                Links::Two => vec![usize::from(v.bones[0]), usize::from(v.bones[1])],
            }
        })
    });
    bone_physics::check_breakable(&parents, &mut physics, used);
}

/// The rotation of object-visual-and-transform.md 4 for angles `a` = (a₀, a₁, a₂), as rows.
pub fn rotation(a: [f32; 3]) -> [[f32; 3]; 3] {
    let (sx, cx) = a[0].sin_cos();
    let (sy, cy) = a[1].sin_cos();
    let (sz, cz) = a[2].sin_cos();
    [
        [cz * cy - sz * sx * sy, -sz * cx, sz * sx * cy + cz * sy],
        [cz * sx * sy + sz * cy, cz * cx, sz * sy - cz * sx * cy],
        [-cx * sy, sx, cx * cy],
    ]
}

/// `[[R, 0], [p, 1]]` with `R` = [`rotation`] of `angles`: an object's
/// transform from its spawn position and angles (object-visual-and-transform.md 4), and a bone's
/// local bind matrix at negated angles (section 4).
pub fn transform(position: [f32; 3], angles: [f32; 3]) -> Mat4 {
    let r = rotation(angles);
    [
        [r[0][0], r[0][1], r[0][2], 0.0],
        [r[1][0], r[1][1], r[1][2], 0.0],
        [r[2][0], r[2][1], r[2][2], 0.0],
        [position[0], position[1], position[2], 1.0],
    ]
}

/// `a · b`, row-vector convention.
pub fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

/// `[p 1] · M`.
pub fn transform_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|c| p[0] * m[0][c] + p[1] * m[1][c] + p[2] * m[2][c] + m[3][c])
}

/// Each bone's model-space bind matrix: `B = L · B(parent)`, `B(root) =
/// L(root)`, `L = [[R(−r₀, −r₁, −r₂), 0], [t, 1]]` (section 4). Fails on
/// a parent cycle.
pub fn bind_pose(bones: &[Bone]) -> io::Result<Vec<Mat4>> {
    let local: Vec<Mat4> = bones
        .iter()
        .map(|b| transform(b.bind_position, b.bind_rotation.map(|r| -r)))
        .collect();
    let mut pose: Vec<Option<Mat4>> = vec![None; bones.len()];
    for start in 0..bones.len() {
        // The chain up to a bone already placed, then back down.
        let mut chain = vec![start];
        while let Some(p) = bones[*chain.last().unwrap()].parent {
            if pose[p].is_some() {
                break;
            }
            if chain.len() > bones.len() {
                return Err(invalid("bone hierarchy has a cycle"));
            }
            chain.push(p);
        }
        for &i in chain.iter().rev() {
            if pose[i].is_some() {
                continue;
            }
            pose[i] = Some(match bones[i].parent {
                Some(p) => multiply(&local[i], &pose[p].expect("parent placed")),
                None => local[i],
            });
        }
    }
    Ok(pose
        .into_iter()
        .map(|m| m.expect("every bone placed"))
        .collect())
}

/// 32767 / 12 and 32767 / 16 in single precision (model-shading.md 1).
const POSITION_SCALE: f32 = 32767.0 / 12.0;
const UV_SCALE: f32 = 32767.0 / 16.0;
/// The smallest normal f32, the normalisation threshold of model-shading.md 1.
const TINY: f32 = 1.175_494_4e-38;

fn clamp_i16(v: f32) -> i16 {
    v.floor().clamp(-32768.0, 32767.0) as i16
}

/// A position component as the hardware modes store it (model-shading.md 1).
pub fn quantize_position(p: f32) -> i16 {
    clamp_i16(p * POSITION_SCALE)
}

/// A stored position component as drawn: `q · 12 / 32768`.
pub fn position_value(q: i16) -> f32 {
    f32::from(q) * 12.0 / 32768.0
}

/// A normal (also tangent, binormal) as stored: normalised if its squared
/// length exceeds the threshold, then per component
/// `clamp(floor((n + 1) · 127.5), 0, 255)`.
pub fn quantize_normal(n: [f32; 3]) -> [u8; 3] {
    let sq = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
    let n = if sq > TINY {
        n.map(|c| c / sq.sqrt())
    } else {
        n
    };
    n.map(|c| ((c + 1.0) * 127.5).floor().clamp(0.0, 255.0) as u8)
}

/// A stored normal component as drawn: `2 · c / 255 − 1`.
pub fn normal_value(c: u8) -> f32 {
    2.0 * f32::from(c) / 255.0 - 1.0
}

/// A texture coordinate as stored.
pub fn quantize_uv(t: f32) -> i16 {
    clamp_i16(t * UV_SCALE)
}

/// A stored texture coordinate as drawn: `q · 16 / 32768`.
pub fn uv_value(q: i16) -> f32 {
    f32::from(q) * 16.0 / 32768.0
}

/// A 2-link weight as stored: `clamp(floor(w · 255 + 0.5), 0, 255)`.
pub fn quantize_weight(w: f32) -> u8 {
    (w * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8
}

/// A stored weight as drawn: `c / 255`.
pub fn weight_value(c: u8) -> f32 {
    f32::from(c) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::chunk;

    fn header(kind: u8, min: [f32; 3], max: [f32; 3]) -> Vec<u8> {
        let mut h = vec![VERSION, kind, 0, 0];
        let centre: [f32; 3] = std::array::from_fn(|i| (min[i] + max[i]) / 2.0);
        for f in min.iter().chain(&max).chain(&centre).chain([&1.0]) {
            h.extend(f.to_le_bytes());
        }
        h
    }

    fn floats(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    fn string(s: &str) -> Vec<u8> {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        b
    }

    /// A child mesh of `kind` with three vertices (1-link to bones 0, 0, 1,
    /// or 2-link) and one triangle.
    fn mesh(kind: u8, two_link: bool) -> Vec<u8> {
        let mut m = chunk(HEADER_CHUNK, &header(kind, [0.0; 3], [1.0; 3]));
        let mut names = string("wood\\wood_walls8");
        names.extend(string("models\\model"));
        m.extend(chunk(TEXTURE_CHUNK, &names));
        let mut v = Vec::new();
        v.extend(if two_link { TWO_LINK } else { ONE_LINK }.to_le_bytes());
        v.extend(3u32.to_le_bytes());
        for i in 0..3u16 {
            let p = [f32::from(i), 1.0, 0.0];
            let rest = floats(&[p[0], p[1], p[2], 0.0, 1.0, 0.0, 1., 0., 0., 0., 0., 1.]);
            if two_link {
                v.extend(i.min(1).to_le_bytes());
                v.extend(2u16.to_le_bytes());
                v.extend(&rest);
                v.extend(floats(&[0.25, 0.5, 0.75]));
            } else {
                v.extend(&rest);
                v.extend(floats(&[0.5, 0.75]));
                v.extend(u32::from(i.saturating_sub(1)).to_le_bytes());
            }
        }
        m.extend(chunk(VERTICES_CHUNK, &v));
        let mut ix = 6u32.to_le_bytes().to_vec();
        for i in [0u16, 1, 2, 2, 1, 0] {
            ix.extend(i.to_le_bytes());
        }
        m.extend(chunk(INDICES_CHUNK, &ix));
        if kind == 4 {
            // Level 0: the second triangle, 3 vertices; level 1: 1 triangle
            // from 0.
            let mut w = vec![0u8; 16];
            w.extend(2u32.to_le_bytes());
            for (offset, tri, verts) in [(3u32, 1u16, 3u16), (0, 1, 3)] {
                w.extend(offset.to_le_bytes());
                w.extend(tri.to_le_bytes());
                w.extend(verts.to_le_bytes());
            }
            m.extend(chunk(DETAIL_WINDOWS_CHUNK, &w));
        }
        m
    }

    /// Bones `root` (bind position (0, 1, 0), turned by π/2 about y),
    /// `child` of `root` at (1, 0, 0), `leaf` of `child`.
    fn bones() -> (Vec<u8>, Vec<u8>) {
        let mut names = 3u32.to_le_bytes().to_vec();
        let mut data = Vec::new();
        let list = [
            (
                "root",
                "",
                [0.0, std::f32::consts::FRAC_PI_2, 0.0],
                [0.0, 1.0, 0.0],
            ),
            ("child", "root", [0.0; 3], [1.0, 0.0, 0.0]),
            ("leaf", "child", [0.0; 3], [0.0, 0.0, 2.0]),
        ];
        for (i, (name, parent, rotation, position)) in list.into_iter().enumerate() {
            names.extend(string(name));
            names.extend(string(parent));
            names.extend(floats(&[0.5; 15]));
            // Version 1 for two bones, 0 (no friction) for the last.
            let version = u32::from(i < 2);
            data.extend(version.to_le_bytes());
            data.extend(string("default"));
            data.extend([0u8; 112]); // Shape type 0 and its parts.
            data.extend(0u32.to_le_bytes());
            data.extend(floats(&[0.0; 14]));
            data.extend(0u32.to_le_bytes());
            data.extend(floats(&[0.0; 2]));
            if version != 0 {
                data.extend(floats(&[1.0]));
            }
            data.extend(floats(&rotation));
            data.extend(floats(&position));
            data.extend(floats(&[10.0, 0.0, 0.0, 0.0]));
        }
        (names, data)
    }

    fn model(kind: u8, meshes: &[Vec<u8>]) -> Vec<u8> {
        let mut children = Vec::new();
        for (i, m) in meshes.iter().enumerate() {
            children.extend(chunk(i as u32, m));
        }
        let (names, data) = bones();
        let mut f = chunk(HEADER_CHUNK, &header(kind, [-1.0; 3], [2.0; 3]));
        f.extend(chunk(18, b"description"));
        f.extend(chunk(CHILDREN_CHUNK, &children));
        f.extend(chunk(BONE_NAMES_CHUNK, &names));
        f.extend(chunk(BONE_DATA_CHUNK, &data));
        f
    }

    #[test]
    fn parses_skeleton_models() {
        let file = model(10, &[mesh(5, false), mesh(4, true)]);
        let m = Model::parse(&file).unwrap();
        assert_eq!(m.kind, ModelType::Skeleton);
        assert_eq!(m.header.box_max, [2.0; 3]);
        assert_eq!(m.meshes.len(), 2);

        let a = &m.meshes[0];
        assert_eq!(
            (a.kind, a.links, a.texture.as_str(), a.shader.as_str()),
            (
                MeshType::Skinned,
                Links::One,
                "wood\\wood_walls8",
                "models\\model"
            )
        );
        assert_eq!(a.vertices[2].position, [2.0, 1.0, 0.0]);
        assert_eq!(a.vertices[2].normal, [0.0, 1.0, 0.0]);
        assert_eq!(a.vertices[2].uv, [0.5, 0.75]);
        assert_eq!(a.vertices[2].bones, [1, 1]);
        assert_eq!(a.full_detail_indices(), [0, 1, 2, 2, 1, 0]);
        assert_eq!(a.render_mode(256), RenderMode::OneLink);

        let b = &m.meshes[1];
        assert_eq!(
            (b.kind, b.links),
            (MeshType::SkinnedProgressive, Links::Two)
        );
        assert_eq!(b.vertices[1].bones, [1, 2]);
        assert_eq!(b.vertices[1].weight, 0.25);
        assert_eq!(b.vertices[1].uv, [0.5, 0.75]);
        assert_eq!(b.vertices[1].binormal, [0.0, 0.0, 1.0]);
        // Detail level 0: one triangle from index 3.
        assert_eq!(b.full_detail_indices(), [2, 1, 0]);
        // Both windows: indices 0 … 6.
        assert_eq!((a.detail_span(), b.detail_span()), (0..6, 0..6));
        assert_eq!(b.render_mode(256), RenderMode::TwoLink);
        // (K − 22) / 3 = 1 < bone 2: software.
        assert_eq!(b.render_mode(25), RenderMode::Software);

        let animated = Model::parse(&model(3, &[mesh(5, false)])).unwrap();
        assert_eq!(animated.kind, ModelType::AnimatedSkeleton);
    }

    #[test]
    fn single_bone_mode() {
        let mut one = SkinnedMesh::parse(&mesh(5, false)).unwrap();
        for v in &mut one.vertices {
            v.bones = [3, 3];
        }
        assert_eq!(one.render_mode(256), RenderMode::SingleBone);
        assert_eq!(one.render_mode(0), RenderMode::SingleBone);
    }

    #[test]
    fn bones_and_bind_pose() {
        let m = Model::parse(&model(10, &[mesh(5, false)])).unwrap();
        let names: Vec<_> = m
            .bones
            .iter()
            .map(|b| (b.name.as_str(), b.parent))
            .collect();
        assert_eq!(
            names,
            [("root", None), ("child", Some(0)), ("leaf", Some(1))]
        );
        assert_eq!(m.bones[2].bind_position, [0.0, 0.0, 2.0]);
        assert_eq!(m.bones[0].box_half_size, [0.5; 3]);
        assert_eq!(m.bone("leaf"), Some(2));
        // Chunk 16: friction only where the version's low bits are not 0.
        let p = &m.bones[2].physics;
        assert_eq!((p.version, p.friction, p.mass), (0, None, 10.0));
        assert_eq!(m.bones[0].physics.friction, Some(1.0));
        assert_eq!(p.material, "default");
        let pose = m.bind_pose().unwrap();
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6);
        // The root at (0, 1, 0), rotated by R(0, −π/2, 0): its rows are
        // (0, 0, −1), (0, 1, 0), (1, 0, 0), so x → (0, 0, −1).
        assert!(close(transform_point(&pose[0], [0.0; 3]), [0.0, 1.0, 0.0]));
        assert!(close(
            transform_point(&pose[0], [1.0, 0.0, 0.0]),
            [0.0, 1.0, -1.0]
        ));
        // The child's local (1, 0, 0) goes through the root: B = L · B(root).
        assert!(close(transform_point(&pose[1], [0.0; 3]), [0.0, 1.0, -1.0]));
        // The leaf's (0, 0, 2) in the child's frame, whose axes are the
        // root's: z → (1, 0, 0).
        assert!(close(transform_point(&pose[2], [0.0; 3]), [2.0, 1.0, -1.0]));
    }

    #[test]
    fn rotation_rows() {
        // object-visual-and-transform.md 5.2, object 79: angles (0, 4.712372, 0).
        let r = rotation([0.0, 4.712_372, 0.0]);
        assert!((r[0][0] - -0.000_017).abs() < 1e-6 && (r[0][2] - -1.0).abs() < 1e-6);
        assert!((r[2][0] - 1.0).abs() < 1e-6 && (r[2][2] - -0.000_017).abs() < 1e-6);
        // Object 0: angles (0.581842, −1.455716, −0.267673).
        let r = rotation([0.581_842, -1.455_716, -0.267_673]);
        let want = [
            [-0.033_655, 0.220_967, -0.974_700],
            [-0.556_858, 0.805_701, 0.201_881],
            [0.829_926, 0.549_564, 0.095_932],
        ];
        for (row, w) in r.iter().zip(want) {
            for (a, b) in row.iter().zip(w) {
                assert!((a - b).abs() < 2e-6, "{r:?}");
            }
        }
        let x = transform([1.0, 2.0, 3.0], [0.0, std::f32::consts::FRAC_PI_2, 0.0]);
        let p = transform_point(&x, [1.0, 0.0, 0.0]);
        assert!(
            (p[0] - 1.0).abs() < 1e-6 && (p[2] - 4.0).abs() < 1e-6,
            "{p:?}"
        );
    }

    #[test]
    fn quantization() {
        // Section 4.7.
        assert_eq!(quantize_position(1.0), 2730);
        assert_eq!(quantize_position(-1.0), -2731);
        assert_eq!(quantize_position(16.87), 32767);
        assert_eq!(quantize_position(-20.0), -32768);
        assert!((position_value(quantize_position(1.0)) - 1.0).abs() < 12.0 / 32768.0);
        assert_eq!(quantize_normal([0.0, 2.0, 0.0]), [127, 255, 127]);
        assert_eq!(quantize_normal([-1.0, 0.0, 0.0]), [0, 127, 127]);
        // A zero normal stays zero: 127 each, drawn as a little below 0.
        assert_eq!(quantize_normal([0.0; 3]), [127; 3]);
        assert!((normal_value(127) - -1.0 / 255.0).abs() < 1e-7);
        assert_eq!(normal_value(255), 1.0);
        assert_eq!(quantize_uv(0.5), 1023);
        assert_eq!(quantize_uv(16.08), 32767);
        assert!((uv_value(quantize_uv(0.5)) - 0.5).abs() < 2.0 * 16.0 / 32768.0);
        assert_eq!(quantize_weight(0.5), 128);
        assert_eq!(quantize_weight(1.2), 255);
        assert_eq!(weight_value(255), 1.0);
    }

    #[test]
    fn rejects_bad_models() {
        assert!(Model::parse(&model(1, &[mesh(5, false)])).is_err(), "group");
        assert!(
            Model::parse(&model(10, &[mesh(0, false)])).is_err(),
            "mesh child"
        );
        let mut bad = mesh(5, false);
        // The vertex format word follows the header and texture chunks.
        let at = 8 + 44 + 8 + "wood\\wood_walls8\0models\\model\0".len() + 8;
        bad[at..at + 4].copy_from_slice(&0x112u32.to_le_bytes());
        assert!(
            SkinnedMesh::parse(&bad)
                .unwrap_err()
                .to_string()
                .contains("Invalid vertex type")
        );
        let mut out_of_range = mesh(5, false);
        let n = out_of_range.len();
        out_of_range[n - 2..].copy_from_slice(&3u16.to_le_bytes());
        assert!(SkinnedMesh::parse(&out_of_range).is_err(), "index 3 of 3");
        // A detail window whose offset would wrap a 32-bit sum.
        let mut wrapping = mesh(4, false);
        let n = wrapping.len();
        // The last window record: offset, triangles, vertices.
        wrapping[n - 8..n - 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(
            SkinnedMesh::parse(&wrapping)
                .unwrap_err()
                .to_string()
                .contains("detail window ends")
        );
        // A bone index beyond the bones.
        let mut far = mesh(5, false);
        let at = 8 + 44 + 8 + 30 + 8 + 8 + 2 * 60 + 56;
        far[at..at + 4].copy_from_slice(&7u32.to_le_bytes());
        assert!(Model::parse(&model(10, &[far])).is_err(), "bone 7 of 3");
    }
}
