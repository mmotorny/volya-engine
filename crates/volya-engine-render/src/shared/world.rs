// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The static world's platform-free part
//! (docs/behavior/static-shading.md): what a frame and its draws are, the
//! per-draw records, the culling done on the CPU and the rebasing of the
//! level's index ranges at upload.

use std::collections::HashMap;
use std::io;

use volya_engine_formats::level_geom::{GeomBuffer, GeomSource, Layout};

use crate::sky::SkyFrame;
use crate::{Texture, World};

/// Row-vector convention (`p · M`), stored row by row.
pub type Mat4 = [[f32; 4]; 4];

/// The maximum anisotropy and mip level-of-detail bias in force on the
/// samplers (texture-quality-filtering.md 5): the bias on every mipmapped
/// sampler, the anisotropy on the anisotropic ones (static-shading.md 4.4's
/// and those that follow it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filtering {
    /// 1 … 16.
    pub anisotropy: u16,
    pub mip_bias: f32,
}

impl Default for Filtering {
    /// The shipped `user.ltx`'s `r__tf_aniso 16`, `r1_tf_mipbias 0`.
    fn default() -> Self {
        Self {
            anisotropy: 16,
            mip_bias: 0.0,
        }
    }
}
/// The tree constants' factor: stored values × 0.5 at load, × K = 1.5 per
/// draw (section 7.3).
pub(crate) const TREE_FACTOR: f32 = 0.75;

/// The vertex and pixel programs of a pass (sections 7, 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Program {
    /// `lmap` / `lmap_dt` (`LM`, `LM_AREF`).
    LightMapped,
    /// `impl_dt` (`BmmDold`, terrain).
    Terrain,
    /// `vert` / `vert_dt` (`V`, `V_AREF`).
    VertexLit,
    /// `tree_w` / `tree_s` (+ `_dt`) with `vert` / `vert_dt` (`D_TREE`).
    Tree,
    /// `wmark` / `vert` (`effects\wallmarkblend`).
    Wallmark,
    /// `simple` (`selflight`).
    SelfLight,
    /// `base_lplanes` (`effects\lightplanes`).
    LightPlanes,
    /// `model_def_lq` (class `MODEL   `, element 1; level-objects.md 5.2).
    Model,
    /// `model_env_lq` (class `MODELEbB`, element 1).
    ModelEnv,
    /// `model_def_lqs` / `simple` (`models\selflight`, `models\selflightl`).
    ModelSelfLight,
    /// `model_def_lplanes` / `base_lplanes` (`models\lightplanes`).
    ModelLightPlanes,
}

/// The vertex layout a program reads: one of `level.geom`'s, or the
/// objects' models' (level-objects.md section 4.7: position as `s16` × 3
/// and one unused, normal as `u8` × 3 and one unused, texture coordinate as
/// `s16` × 2; 16 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertexLayout {
    Level(Layout),
    Model,
}

/// Bytes of one model vertex ([`VertexLayout::Model`]).
pub const MODEL_VERTEX_BYTES: usize = 16;

/// One model vertex as stored for drawing: the quantized position, normal
/// and texture coordinate of level-objects.md section 4.7.
pub fn model_vertex(position: [i16; 3], normal: [u8; 3], uv: [i16; 2]) -> [u8; MODEL_VERTEX_BYTES] {
    let mut v = [0; MODEL_VERTEX_BYTES];
    for (i, c) in position.iter().enumerate() {
        v[2 * i..2 * i + 2].copy_from_slice(&c.to_le_bytes());
    }
    v[8..11].copy_from_slice(&normal);
    v[12..14].copy_from_slice(&uv[0].to_le_bytes());
    v[14..16].copy_from_slice(&uv[1].to_le_bytes());
    v
}

/// The objects' models on the GPU side: every model vertex
/// ([`model_vertex`]) in one buffer and every index in another; each draw
/// names its range.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectGeometry {
    pub vertices: Vec<u8>,
    pub indices: Vec<u16>,
}

impl Program {
    /// The vertex layout the program reads.
    pub fn layout(self) -> VertexLayout {
        match self {
            Self::LightMapped | Self::Terrain => VertexLayout::Level(Layout::LightMapped),
            Self::Tree => VertexLayout::Level(Layout::Tree),
            Self::Model | Self::ModelEnv | Self::ModelSelfLight | Self::ModelLightPlanes => {
                VertexLayout::Model
            }
            _ => VertexLayout::Level(Layout::VertexLit),
        }
    }

    /// Whether the program draws the objects' models.
    pub fn is_model(self) -> bool {
        self.layout() == VertexLayout::Model
    }

    /// Its name in the frame statistics.
    pub fn name(self) -> &'static str {
        self.entry_points().0
    }

    pub(crate) fn entry_points(self) -> (&'static str, &'static str) {
        match self {
            Self::LightMapped => ("vs_lmap", "fs_lmap"),
            Self::Terrain => ("vs_terrain", "fs_terrain"),
            Self::VertexLit => ("vs_vert", "fs_vert"),
            Self::Tree => ("vs_tree", "fs_vert"),
            Self::Wallmark => ("vs_wmark", "fs_vert"),
            Self::SelfLight => ("vs_simple", "fs_simple"),
            Self::LightPlanes => ("vs_lplanes", "fs_lplanes"),
            Self::Model => ("vs_model", "fs_vert"),
            Self::ModelEnv => ("vs_model_env", "fs_model_env"),
            Self::ModelSelfLight => ("vs_model_simple", "fs_simple"),
            Self::ModelLightPlanes => ("vs_model_lplanes", "fs_lplanes"),
        }
    }
}

/// The vertex program of a surface's light elements (2 point, 3 spot;
/// docs/behavior/dynamic-lights.md section 6.5), with `add_point` or
/// `add_spot`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LightProgram {
    /// `lmap_*`, `impl_*` (the light-mapped layout).
    LightMapped,
    /// `vert_*`, `simple_*` (the vertex-lit layout).
    VertexLit,
    /// `wmark_*`.
    Wallmark,
    /// `tree_s_*` with object LOD, else `tree_w_*`.
    Tree,
    /// `model_def_*`.
    Model,
}

impl LightProgram {
    pub(crate) const ALL: [Self; 5] = [
        Self::LightMapped,
        Self::VertexLit,
        Self::Wallmark,
        Self::Tree,
        Self::Model,
    ];

    pub(crate) fn entry_point(self) -> &'static str {
        match self {
            Self::LightMapped => "vs_light_lmap",
            Self::VertexLit => "vs_light_vert",
            Self::Wallmark => "vs_light_wmark",
            Self::Tree => "vs_light_tree",
            Self::Model => "vs_light_model",
        }
    }

    /// The vertex layout the program reads.
    pub fn layout(self) -> VertexLayout {
        match self {
            Self::LightMapped => VertexLayout::Level(Layout::LightMapped),
            Self::VertexLit | Self::Wallmark => VertexLayout::Level(Layout::VertexLit),
            Self::Tree => VertexLayout::Level(Layout::Tree),
            Self::Model => VertexLayout::Model,
        }
    }
}

/// A shader's light elements (dynamic-lights.md section 6.1): their vertex
/// program, alpha reference and priority. Only priorities 0 and 1 are ever
/// lit (section 5.5, rule 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightElement {
    pub program: LightProgram,
    /// Passes when alpha·255 > it.
    pub alpha_ref: u8,
    pub priority: u8,
}

/// Blending of a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Blend {
    Off,
    /// Source alpha / inverse source alpha.
    Alpha,
    /// Source alpha / one.
    Add,
}

/// How a surface is drawn: the normal pass of its shader entry. `T` is
/// the texture: a name while resolving, a [`Texture`] when drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface<T> {
    pub program: Program,
    pub blend: Blend,
    pub depth_write: bool,
    pub fog: bool,
    /// Alpha test reference: passes when alpha·255 > ref.
    pub alpha_ref: Option<u8>,
    /// `dt_params` scale when element 0 uses the detail texture.
    pub detail_scale: Option<f32>,
    pub priority: u8,
    pub strict: bool,
    /// Trees: object LOD (no wave in element 0).
    pub object_lod: bool,
    /// Base, second (light map), hemi map, detail.
    pub textures: [Option<T>; 4],
    /// The cube environment map (`MODELEbB`, level-objects.md 5.2).
    pub env: Option<T>,
    /// The light elements, if the shader has them.
    pub light: Option<LightElement>,
}

impl<T> Surface<T> {
    pub fn map_textures<U>(self, mut f: impl FnMut(T) -> Option<U>) -> Surface<U> {
        let [a, b, c, d] = self.textures;
        Surface {
            program: self.program,
            blend: self.blend,
            depth_write: self.depth_write,
            fog: self.fog,
            alpha_ref: self.alpha_ref,
            detail_scale: self.detail_scale,
            priority: self.priority,
            strict: self.strict,
            object_lod: self.object_lod,
            textures: [
                a.and_then(&mut f),
                b.and_then(&mut f),
                c.and_then(&mut f),
                d.and_then(&mut f),
            ],
            env: self.env.and_then(&mut f),
            light: self.light,
        }
    }
}

/// A tree instance's stored lighting (ogf.md section 5): scale and bias
/// (r, g, b, hemi, sun).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeLight {
    pub scale: [f32; 5],
    pub bias: [f32; 5],
}

/// One draw of the static world: `index_count` indices of index buffer
/// `index_buffer` from `first_index`, each relative to `first_vertex` of
/// vertex buffer `vertex_buffer`, with surface `surface`. An object's draw
/// (`object` set) reads [`ObjectGeometry`] instead, and its buffer indices
/// are ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldDraw {
    pub surface: u32,
    pub vertex_buffer: u32,
    pub first_vertex: u32,
    pub index_buffer: u32,
    pub first_index: u32,
    pub index_count: u32,
    /// Model to world.
    pub transform: Mat4,
    /// Bounding sphere: centre, radius.
    pub sphere: [f32; 4],
    pub tree: Option<TreeLight>,
    /// An object's `L_dynamic_props` (level-objects.md 5.4).
    pub object: Option<[f32; 4]>,
}

/// Bytes of one draw's record in the draws buffer.
pub(crate) const DRAW_BYTES: usize = 144;
/// RGBA32F texels per draw record, and records per row of the draws
/// texture (world.wgsl's `DRAW_TEXELS`, `DRAWS_PER_ROW`): WebGL 2's vertex
/// programs have no storage buffers.
pub(crate) const DRAW_TEXELS: u32 = (DRAW_BYTES / 16) as u32;
pub(crate) const DRAWS_PER_ROW: u32 = 64;

/// Bytes of the frame constants.
pub(crate) const FRAME_BYTES: u64 = 288;

/// The blended environment's values the passes use (section 6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldLight {
    pub ambient: [f32; 3],
    pub hemi: [f32; 3],
    pub sun_color: [f32; 3],
    /// The direction sunlight travels.
    pub sun_dir: [f32; 3],
    /// The fog unit's colour (already truncated to bytes).
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
}

/// The world as seen in one frame.
#[derive(Clone, Debug)]
pub struct WorldFrame {
    pub world: World,
    /// Start-position.md sections 6 and 7.
    pub view: Mat4,
    pub projection: Mat4,
    pub eye: [f32; 3],
    pub forward: [f32; 3],
    /// Vertical field of view in degrees (size culling, 10.2).
    pub fov_y_degrees: f32,
    pub light: WorldLight,
    /// Seconds (tree sway, cloud drift).
    pub time: f32,
    /// The sky's values; `None` draws no sky.
    pub sky: Option<SkyFrame>,
    /// What shows where nothing is drawn (RGB, 0 … 1).
    pub clear: [f32; 3],
    /// The particle effects drawn (particle-drawing.md), each with its
    /// place in the frame, and the glows (glows.md section 6).
    pub particles: Vec<crate::particles::ParticleBatch>,
    /// The frame's dynamic light passes in their order
    /// (dynamic-lights.md 5.1).
    pub lights: Vec<LightPass>,
    /// How the frame reaches the screen (frame-presentation.md).
    pub presentation: crate::present::Presentation,
    /// The screen-effect values handed over for this frame
    /// (frame-presentation.md 2.2); `None` keeps those in force.
    pub effects: Option<crate::present::ScreenEffects>,
    /// The detail objects drawn (detail-objects.md 5.4, 6).
    pub details: Option<crate::details::DetailFrame>,
    /// The billboard groups' billboards drawn (static-shading.md 8.4).
    pub billboards: Option<crate::billboards::BillboardFrame>,
    /// The samplers' filtering in force.
    pub filtering: Filtering,
    /// `r__geometry_lod` (section 10.2; object-detail-level.md 1).
    pub geometry_lod: f32,
    /// By world draw: those the camera pass keeps (visibility-scope.md 5);
    /// `None` draws every one that may show.
    pub drawn: Option<std::sync::Arc<Vec<bool>>>,
}

/// One dynamic light's pass (docs/behavior/dynamic-lights.md sections 5,
/// 6): the static draws and objects its frustum meets are drawn again
/// with their light element, adding to the image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightPass {
    /// Element 3 (spot) or 2 (point).
    pub spot: bool,
    /// `L_dynamic_pos`.
    pub position: [f32; 4],
    /// `L_dynamic_color`.
    pub colour: [f32; 4],
    /// `L_dynamic_xform`.
    pub xform: Mat4,
    /// M = V · Q_L: the light's frustum is its clip volume (5.3).
    pub frustum: Mat4,
}

/// The attenuation textures of the light passes (section 6.4):
/// `internal\internal_light_att`, `…_attclip`, `…_attpoint`.
#[derive(Clone, Debug)]
pub struct LightTextures {
    pub att: Texture,
    pub attclip: Texture,
    pub attpoint: Texture,
}

/// Bytes of one light pass's constants, a uniform buffer offset apart.
pub(crate) const LIGHT_SLOT: u64 = 256;
/// Light passes the constants buffer holds at first; it grows beyond.
pub(crate) const LIGHT_SLOTS: u64 = 64;

/// A frustum's planes (a, b, c, d), `a x + b y + c z + d ≥ 0` inside,
/// normalised, from a row-vector clip matrix (0 ≤ z ≤ w).
pub(crate) fn frustum_planes(m: &Mat4) -> [[f32; 4]; 6] {
    let col = |c: usize| [m[0][c], m[1][c], m[2][c], m[3][c]];
    let (x, y, z, w) = (col(0), col(1), col(2), col(3));
    let plane = |p: [f32; 4]| {
        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-20);
        p.map(|v| v / l)
    };
    let add = |a: [f32; 4], b: [f32; 4], s: f32| std::array::from_fn(|i| a[i] + s * b[i]);
    [
        plane(add(w, x, 1.0)),
        plane(add(w, x, -1.0)),
        plane(add(w, y, 1.0)),
        plane(add(w, y, -1.0)),
        plane(z),
        plane(add(w, z, -1.0)),
    ]
}

/// Whether a sphere is not outside the planes by more than its radius.
pub(crate) fn meets(planes: &[[f32; 4]; 6], s: [f32; 4]) -> bool {
    planes
        .iter()
        .all(|p| p[0] * s[0] + p[1] * s[1] + p[2] * s[2] + p[3] >= -s[3])
}

/// A kept draw whose surface has light elements of priority 0 or 1
/// (section 5.5, rule 4).
#[derive(Clone, Copy, Debug)]
pub(crate) struct LitDraw {
    /// Its index among the kept draws (its record).
    pub(crate) kept: u32,
    pub(crate) program: LightProgram,
    pub(crate) surface: u32,
    pub(crate) vertex_buffer: u32,
    pub(crate) index_buffer: u32,
    pub(crate) first_vertex: u32,
    pub(crate) first_index: u32,
    pub(crate) index_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PipelineKey {
    pub(crate) program: Program,
    pub(crate) blend: Blend,
    pub(crate) depth_write: bool,
}

/// A level vertex's draw where draws share it (world.wgsl's `SHARED`).
pub(crate) const SHARED: u32 = u32::MAX;

pub(crate) fn push_vec4(out: &mut Vec<u8>, v: [f32; 4]) {
    for c in v {
        out.extend_from_slice(&c.to_le_bytes());
    }
}

pub(crate) fn push_mat(out: &mut Vec<u8>, m: &Mat4) {
    for row in m {
        push_vec4(out, *row);
    }
}

pub(crate) fn vec4(v: [f32; 3], w: f32) -> [f32; 4] {
    [v[0], v[1], v[2], w]
}

/// The frame constants of `frame` ([`FRAME_BYTES`]) with `view_projection`
/// for a target of `size`.
pub(crate) fn frame_constants(
    frame: &WorldFrame,
    view_projection: &Mat4,
    size: (u32, u32),
) -> Vec<u8> {
    let l = &frame.light;
    let tau = std::f32::consts::TAU;
    let t = frame.time;
    let wind = {
        let (x, z) = ((0.6283 * t).sin(), (0.6283 * t).cos());
        let len = x.hypot(z).max(1e-9);
        [0.005 * x / len, 0.0, 0.005 * z / len, 0.0]
    };
    let mut constants = Vec::with_capacity(FRAME_BYTES as usize);
    push_mat(&mut constants, view_projection);
    push_mat(&mut constants, &frame.view);
    push_vec4(
        &mut constants,
        vec4(
            frame.eye,
            ssa_min(size.0, size.1, frame.fov_y_degrees, frame.geometry_lod),
        ),
    );
    push_vec4(
        &mut constants,
        vec4(frame.forward, frame.filtering.mip_bias),
    );
    push_vec4(&mut constants, vec4(l.ambient, 0.0));
    push_vec4(&mut constants, vec4(l.hemi, 0.0));
    push_vec4(&mut constants, vec4(l.sun_color, 0.0));
    push_vec4(&mut constants, vec4(l.sun_dir, 0.0));
    push_vec4(
        &mut constants,
        fog_plane(view_projection, l.fog_near, l.fog_far),
    );
    push_vec4(&mut constants, vec4(l.fog_color, 0.0));
    push_vec4(&mut constants, [0.1 / tau, 0.01 / tau, 0.11 / tau, t / tau]);
    push_vec4(&mut constants, wind);
    constants
}

/// The light pass `l`'s constants, the start of its slot of
/// [`LIGHT_SLOT`] bytes.
pub(crate) fn light_constants(l: &LightPass) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(112);
    push_vec4(&mut bytes, l.position);
    push_vec4(&mut bytes, l.colour);
    push_mat(&mut bytes, &l.xform);
    push_vec4(&mut bytes, [f32::from(u8::from(l.spot)), 0.0, 0.0, 0.0]);
    bytes
}

/// `a · b`, row-vector convention.
pub fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

/// fog_plane from the view-projection matrix (section 9.3): with
/// q = third column + fourth column and ℓ = |q.xyz|,
/// (−q.xyz / (ℓ (far − near)), 1 − (q.w/ℓ − near)/(far − near)).
pub fn fog_plane(view_projection: &Mat4, fog_near: f32, fog_far: f32) -> [f32; 4] {
    let q: [f32; 4] = std::array::from_fn(|r| view_projection[r][2] + view_projection[r][3]);
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
    let span = fog_far - fog_near;
    [
        -q[0] / (l * span),
        -q[1] / (l * span),
        -q[2] / (l * span),
        1.0 - (q[3] / l - fog_near) / span,
    ]
}

/// ssa_min for a `width` × `height` render target, vertical field of view
/// `fov` degrees and `r__geometry_lod` `geometry_lod` (section 10.2).
pub fn ssa_min(width: u32, height: u32, fov: f32, geometry_lod: f32) -> f32 {
    let k = 1.0 / ((geometry_lod + 1e-7) * (90.0 / fov).powi(2) * width as f32 * height as f32);
    12.25 * k
}

/// What a draw's call needs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DrawCall {
    pub(crate) surface: u32,
    pub(crate) vertex_buffer: u32,
    pub(crate) index_buffer: u32,
    pub(crate) first_index: u32,
    pub(crate) index_count: u32,
    pub(crate) first_vertex: i32,
}

/// Whether a draw with bounding sphere `s` may show in a frame with view
/// frustum `planes`, camera position `eye` and `ssa_min`: false only where
/// the shaders would drop all of it (outside the clip volume, or below
/// ssa_min, section 10.2), with margins so that rounding never culls what
/// they would draw.
pub(crate) fn may_show(s: [f32; 4], planes: &[[f32; 4]; 6], eye: [f32; 3], ssa_min: f32) -> bool {
    if s[3] <= 0.0 {
        // The shaders' size test never passes.
        return false;
    }
    let r = s[3] * 1.001 + 0.01;
    if planes
        .iter()
        .any(|p| p[0] * s[0] + p[1] * s[1] + p[2] * s[2] + p[3] < -r)
    {
        return false;
    }
    let d2: f32 = (0..3).map(|i| (eye[i] - s[i]).powi(2)).sum();
    s[3] / (d2 + 1e-5) > ssa_min * 0.99
}

thread_local! {
    /// The world's draw calls emitted in the frame being encoded, by
    /// [`Kind`] (#443).
    pub(crate) static EMITTED: std::cell::Cell<[u32; 5]> = const { std::cell::Cell::new([0; 5]) };
}

/// The kinds of the world's draw calls, for the diagnostic log (#443).
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Merged,
    Single,
    Object,
    Light,
    Strict,
}

/// The names of [`Kind`]s, in order.
pub const CALL_KINDS: [&str; 5] = ["merged", "single", "object", "light", "strict"];

/// Counts a draw call of `kind` in [`EMITTED`].
pub(crate) fn emitted(kind: Kind) {
    let mut e = EMITTED.get();
    e[kind as usize] += 1;
    EMITTED.set(e);
}

/// One step of the static world: its draws in their fixed order, and the
/// groups they are drawn in (ranges of `order`): consecutive draws of one
/// surface and buffers whose vertices are their own and whose index ranges
/// follow one another, else a draw alone.
pub(crate) struct Step {
    pub(crate) order: Vec<u32>,
    pub(crate) groups: Vec<std::ops::Range<u32>>,
}

/// Most draws in a group: a group drawn whole for one draw that may show
/// draws the others' vertices too, which yield no pixels.
pub(crate) const MAX_GROUP: u32 = 16;

impl Step {
    pub(crate) fn new(order: Vec<u32>, calls: &[DrawCall], exclusive: &[bool]) -> Self {
        let mut groups: Vec<std::ops::Range<u32>> = Vec::new();
        for (k, &i) in order.iter().enumerate() {
            let d = &calls[i as usize];
            let joins = groups.last().is_some_and(|g| {
                let j = order[g.end as usize - 1];
                let p = &calls[j as usize];
                exclusive[i as usize]
                    && exclusive[j as usize]
                    && (p.surface, p.vertex_buffer, p.index_buffer)
                        == (d.surface, d.vertex_buffer, d.index_buffer)
                    && p.first_index + p.index_count == d.first_index
            });
            match groups.last_mut() {
                Some(g) if joins && g.end - g.start < MAX_GROUP => g.end += 1,
                _ => groups.push(k as u32..k as u32 + 1),
            }
        }
        Self { order, groups }
    }
}

pub(crate) fn draw_record(surface: &Surface<Texture>, d: &WorldDraw, out: &mut Vec<u8>) {
    push_mat(out, &d.transform);
    push_vec4(out, d.sphere);
    let (scale, bias, sun) = match &d.tree {
        // An object's L_dynamic_props in c_scale's place.
        None if d.object.is_some() => (d.object.unwrap_or_default(), [0.0; 4], [0.0; 2]),
        Some(t) => {
            let f = |v: [f32; 5]| v.map(|c| c * TREE_FACTOR);
            let (s, b) = (f(t.scale), f(t.bias));
            (
                [s[0], s[1], s[2], s[3]],
                [b[0], b[1], b[2], b[3]],
                [s[4], b[4]],
            )
        }
        None => ([0.0; 4], [0.0; 4], [0.0; 2]),
    };
    push_vec4(out, scale);
    push_vec4(out, bias);
    push_vec4(
        out,
        [
            sun[0],
            sun[1],
            surface.detail_scale.unwrap_or(0.0),
            surface.alpha_ref.map_or(-1.0, f32::from),
        ],
    );
    push_vec4(
        out,
        [
            f32::from(u8::from(surface.fog)),
            f32::from(u8::from(surface.object_lod)),
            // The light elements' alpha reference.
            surface.light.map_or(0.0, |l| f32::from(l.alpha_ref)),
            0.0,
        ],
    );
}

/// Most bytes of a buffer the CPU holds at once while streaming it to the
/// GPU.
pub const UPLOAD_PIECE: usize = 2 << 20;

/// An index range of a stored index buffer, uploaded with `base` added at
/// index `at` of the rebased buffer (the level's draws read vertex buffer
/// `vb` from vertex 0).
#[derive(Clone, Copy)]
pub(crate) struct Segment {
    pub(crate) at: u32,
    pub(crate) first: u32,
    pub(crate) count: u32,
    pub(crate) base: u32,
    pub(crate) vb: u32,
}

/// What the upload learns of a level segment: the first draw using it, how
/// many do, and the vertices its indices reach (`[min, max]`, empty while
/// `min > max`).
#[derive(Clone, Copy)]
pub(crate) struct SegmentUse {
    pub(crate) draw: u32,
    pub(crate) uses: u32,
    pub(crate) span: [u32; 2],
}

/// Where each draw's rebased index range begins, by (object draw, index
/// buffer, first index, index count, base vertex).
/// A draw's index range: (object draw, index buffer, first index, index
/// count, base vertex).
pub(crate) type RangeKey = (bool, u32, u32, u32, u32);

#[derive(Default)]
pub(crate) struct Rebased(pub(crate) HashMap<RangeKey, u32>);

impl Rebased {
    pub(crate) fn key(d: &WorldDraw) -> (bool, u32, u32, u32, u32) {
        (
            d.object.is_some(),
            d.index_buffer,
            d.first_index,
            d.index_count,
            d.first_vertex,
        )
    }

    /// Places `d`'s range with `place` (which returns where it begins)
    /// unless a draw with the same range and base placed it already.
    pub(crate) fn add(&mut self, d: &WorldDraw, place: impl FnOnce() -> u32) {
        self.0.entry(Self::key(d)).or_insert_with(place);
    }

    pub(crate) fn first_index(&self, d: &WorldDraw) -> u32 {
        self.0.get(&Self::key(d)).copied().unwrap_or(d.first_index)
    }
}

/// Fills `out`, the bytes from `at` of a rebased index buffer laid out by
/// `segments` (32-bit indices when `wide`, else 16-bit), from stored index
/// buffer `source` of `geom`, widening each segment's span in `uses` by
/// the vertices it reaches.
pub(crate) fn fill_rebased(
    geom: &mut dyn GeomSource,
    source: GeomBuffer,
    segments: &[Segment],
    uses: &mut [SegmentUse],
    wide: bool,
    at: u64,
    out: &mut [u8],
) -> io::Result<()> {
    let size = if wide { 4 } else { 2 };
    let first = (at / size) as u32;
    let n = (out.len() / size as usize) as u32;
    let mut k = segments.partition_point(|s| s.at + s.count <= first);
    let mut done = 0;
    let mut stored = Vec::new();
    while done < n {
        let s = segments
            .get(k)
            .ok_or_else(|| io::Error::other("rebased indices past their ranges"))?;
        let from = first + done - s.at;
        let take = (s.count - from).min(n - done);
        stored.resize(2 * take as usize, 0);
        geom.read(source, 2 * u64::from(s.first + from), &mut stored)?;
        let dst = &mut out[size as usize * done as usize..size as usize * (done + take) as usize];
        let span = &mut uses[k].span;
        for (j, pair) in stored.as_chunks::<2>().0.iter().enumerate() {
            let index = u32::from(u16::from_le_bytes(*pair)) + s.base;
            span[0] = span[0].min(index);
            span[1] = span[1].max(index);
            if wide {
                dst[4 * j..4 * j + 4].copy_from_slice(&index.to_le_bytes());
            } else {
                let index = u16::try_from(index).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "a rebased index passes 16 bits")
                })?;
                dst[2 * j..2 * j + 2].copy_from_slice(&index.to_le_bytes());
            }
        }
        done += take;
        k += 1;
    }
    Ok(())
}

/// Each level vertex buffer's vertices that one draw alone reaches, as
/// sorted `(first, last, draw)`: those of segments used by one draw whose
/// spans meet no other segment's on the same buffer.
pub(crate) fn owned_spans(
    segments: &[Vec<Segment>],
    uses: &[Vec<SegmentUse>],
    vertex_buffers: usize,
) -> Vec<Vec<(u32, u32, u32)>> {
    let mut spans: Vec<Vec<(u32, u32, u32, bool)>> = vec![Vec::new(); vertex_buffers];
    for (segments, uses) in segments.iter().zip(uses) {
        for (s, u) in segments.iter().zip(uses) {
            if u.span[0] <= u.span[1] {
                spans[s.vb as usize].push((u.span[0], u.span[1], u.draw, u.uses == 1));
            }
        }
    }
    spans
        .into_iter()
        .map(|mut v| {
            v.sort_unstable();
            let mut owned = Vec::with_capacity(v.len());
            // The furthest vertex reached by the spans before each.
            let mut reach: Option<u32> = None;
            for (i, &(first, last, draw, single)) in v.iter().enumerate() {
                let before = reach.is_some_and(|r| r >= first);
                let after = v.get(i + 1).is_some_and(|n| n.0 <= last);
                if single && !before && !after {
                    owned.push((first, last, draw));
                }
                reach = Some(reach.map_or(last, |r| r.max(last)));
            }
            owned
        })
        .collect()
}

/// Fills `out`, the bytes from `at` of vertex buffer `vb`'s draw ids: each
/// vertex's owning draw, else [`SHARED`].
pub(crate) fn fill_draw_ids(owned: &[(u32, u32, u32)], at: u64, out: &mut [u8]) {
    let first = (at / 4) as u32;
    let mut k = owned.partition_point(|o| o.1 < first);
    for (j, slot) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let v = first + j as u32;
        while owned.get(k).is_some_and(|o| o.1 < v) {
            k += 1;
        }
        let id = match owned.get(k) {
            Some(&(f, _, draw)) if f <= v => draw,
            _ => SHARED,
        };
        slot.copy_from_slice(&id.to_le_bytes());
    }
}

/// Fills `out`, the bytes from `at` of the objects' rebased index buffer
/// laid out by `segments` (32-bit indices when `wide`, else 16-bit), from
/// the objects' `indices`; past the last segment with zeros (a buffer is at
/// least 4 bytes).
pub(crate) fn fill_object_rebased(
    indices: &[u16],
    segments: &[Segment],
    wide: bool,
    at: u64,
    out: &mut [u8],
) -> io::Result<()> {
    let size = if wide { 4 } else { 2 };
    let first = (at / size as u64) as u32;
    let mut k = segments.partition_point(|s| s.at + s.count <= first);
    for (j, slot) in out.chunks_exact_mut(size).enumerate() {
        let i = first + j as u32;
        while segments.get(k).is_some_and(|s| s.at + s.count <= i) {
            k += 1;
        }
        let Some(s) = segments.get(k) else {
            slot.fill(0);
            continue;
        };
        let stored = indices
            .get((s.first + i - s.at) as usize)
            .ok_or_else(|| io::Error::other("the objects' indices changed"))?;
        let index = u32::from(*stored) + s.base;
        if wide {
            slot.copy_from_slice(&index.to_le_bytes());
        } else {
            slot.copy_from_slice(&(index as u16).to_le_bytes());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The objects' rebased indices filled piece by piece match the whole
    /// buffer made at once, 16- and 32-bit, a piece starting mid-segment,
    /// zeros past the last.
    #[test]
    fn object_indices_rebase_piece_by_piece() {
        let indices: Vec<u16> = (0..40).map(|i| (i * 7 % 23) as u16).collect();
        let segments = [
            Segment {
                at: 0,
                first: 3,
                count: 9,
                base: 0,
                vb: 0,
            },
            Segment {
                at: 9,
                first: 0,
                count: 14,
                base: 70_000,
                vb: 0,
            },
            Segment {
                at: 23,
                first: 20,
                count: 11,
                base: 5,
                vb: 0,
            },
        ];
        for wide in [false, true] {
            let size = if wide { 4 } else { 2 };
            let mut whole = Vec::new();
            for s in &segments {
                for &i in &indices[s.first as usize..(s.first + s.count) as usize] {
                    let v = u32::from(i) + s.base;
                    if wide {
                        whole.extend_from_slice(&v.to_le_bytes());
                    } else {
                        whole.extend_from_slice(&(v as u16).to_le_bytes());
                    }
                }
            }
            whole.resize(whole.len() + 2 * size, 0);
            let mut pieces = vec![0; whole.len()];
            for (k, piece) in pieces.chunks_mut(3 * size).enumerate() {
                let at = (k * 3 * size) as u64;
                fill_object_rebased(&indices, &segments, wide, at, piece).unwrap();
            }
            assert_eq!(pieces, whole, "wide {wide}");
        }
    }

    #[test]
    fn multiply_is_row_vector_composition() {
        let mut a: Mat4 = [[0.0; 4]; 4];
        let mut b: Mat4 = [[0.0; 4]; 4];
        for i in 0..4 {
            a[i][i] = 2.0;
            b[i][i] = 1.0;
        }
        // b translates by (1, 2, 3).
        b[3] = [1.0, 2.0, 3.0, 1.0];
        let m = multiply(&a, &b);
        // (1, 1, 1, 1) · a · b = (2, 2, 2, 2) · b = (4, 6, 8, 2).
        let p = [1.0f32; 4];
        let out: Vec<f32> = (0..4)
            .map(|c| (0..4).map(|k| p[k] * m[k][c]).sum())
            .collect();
        assert_eq!(out, [4.0, 6.0, 8.0, 2.0]);
    }

    #[test]
    fn fog_plane_and_ssa_min() {
        // A camera at the origin looking along +z (start-position.md 7).
        let (n, z) = (0.2, 300.0);
        let mut p: Mat4 = [[0.0; 4]; 4];
        p[0][0] = 1.0;
        p[1][1] = 1.0;
        p[2][2] = z / (z - n);
        p[2][3] = 1.0;
        p[3][2] = -n * z / (z - n);
        let plane = fog_plane(&p, 10.0, 110.0);
        // F = 1 − (d − near)/(far − near), d the distance from the plane
        // q·(P, 1) = 0, which lies near the camera.
        let offset = -p[3][2] / (p[2][2] + 1.0);
        let f = |depth: f32| plane[2] * depth + plane[3];
        assert!((f(10.0 + offset) - 1.0).abs() < 1e-4);
        assert!(f(110.0 + offset).abs() < 1e-4);
        assert!((f(60.0 + offset) - 0.5).abs() < 1e-4);
        let expected = 12.25 / (1024.0 * 768.0);
        assert!((ssa_min(1024, 768, 90.0, 1.0) - expected).abs() < 1e-5 * expected);
        // object-detail-level.md 1: a higher detail lowers the threshold.
        let half = ssa_min(1024, 768, 90.0, 0.5);
        assert!((half - 2.0 * expected).abs() < 1e-5 * expected);
    }
}
