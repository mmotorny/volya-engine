// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's static world on the GPU, shaded as the first renderer does
//! (docs/behavior/static-shading.md): `level.geom`'s vertex and index
//! buffers uploaded as stored (docs/formats/level-geom.md; packed attributes
//! are decoded in the vertex programs), one draw per static visual with its
//! surface (programs, textures, states).
//!
//! The draws that do not depend on the camera's position for their order
//! are recorded once into render bundles; what depends on the camera per
//! draw (the near/far element of section 5, the size culling of 10.2, the
//! detail fade) is decided in the vertex programs from per-draw data. Only
//! the strict-sorted draws are ordered on the CPU each frame (10.3).

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::rc::Rc;

use volya_engine_formats::level_geom::{GeomBuffer, GeomLayout, GeomSource, Layout, LevelGeom};
use wgpu::util::DeviceExt;

use crate::sky::{SkyFrame, SkyPipelines, SkyResources, SkyTextures};
use crate::{Renderer, Texture};

/// Depth buffer format of the world pass.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Row-vector convention (`p · M`), stored row by row.
pub type Mat4 = [[f32; 4]; 4];

/// Maximum anisotropy, clamp(`r__tf_aniso`, 1, 16) with the default 16
/// (section 4.4).
const ANISOTROPY: u16 = 16;
/// The tree constants' factor: stored values × 0.5 at load, × K = 1.5 per
/// draw (section 7.3).
const TREE_FACTOR: f32 = 0.75;

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

    fn entry_points(self) -> (&'static str, &'static str) {
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
    const ALL: [Self; 5] = [
        Self::LightMapped,
        Self::VertexLit,
        Self::Wallmark,
        Self::Tree,
        Self::Model,
    ];

    fn entry_point(self) -> &'static str {
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
const DRAW_BYTES: usize = 144;
/// [`Renderer::compat`]: RGBA32F texels per draw record, and records per
/// row of the draws texture.
const DRAW_TEXELS: u32 = (DRAW_BYTES / 16) as u32;
const DRAWS_PER_ROW: u32 = 64;

/// The world program's source: as written, or for [`Renderer::compat`] with
/// the draws read from a texture (`draw_texels`, [`DRAW_TEXELS`] texels per
/// draw, [`DRAWS_PER_ROW`] draws per row) instead of a storage buffer, which
/// WebGL 2's vertex programs lack.
pub fn world_source(compat: bool) -> String {
    let source = include_str!("world.wgsl");
    if !compat {
        return source.to_owned();
    }
    const STORAGE: &str = "@group(0) @binding(1) var<storage, read> draws: array<Draw>;";
    let texture = format!(
        "@group(0) @binding(1) var draw_texels: texture_2d<f32>;

fn draw_at(i: u32) -> Draw {{
    let x = i32((i % {DRAWS_PER_ROW}u) * {DRAW_TEXELS}u);
    let y = i32(i / {DRAWS_PER_ROW}u);
    let t = array<vec4<f32>, {DRAW_TEXELS}>(
        textureLoad(draw_texels, vec2(x, y), 0),
        textureLoad(draw_texels, vec2(x + 1, y), 0),
        textureLoad(draw_texels, vec2(x + 2, y), 0),
        textureLoad(draw_texels, vec2(x + 3, y), 0),
        textureLoad(draw_texels, vec2(x + 4, y), 0),
        textureLoad(draw_texels, vec2(x + 5, y), 0),
        textureLoad(draw_texels, vec2(x + 6, y), 0),
        textureLoad(draw_texels, vec2(x + 7, y), 0),
        textureLoad(draw_texels, vec2(x + 8, y), 0),
    );
    return Draw(mat4x4(t[0], t[1], t[2], t[3]), t[4], t[5], t[6], t[7], t[8]);
}}"
    );
    assert!(
        source.contains(STORAGE),
        "world.wgsl: the draws binding changed"
    );
    source
        .replace(STORAGE, &texture)
        .replace("draws[v.draw]", "draw_at(v.draw)")
}

/// The per-draw records on the GPU.
enum DrawData {
    Buffer(wgpu::Buffer),
    /// [`Renderer::compat`]: an RGBA32F texture ([`world_source`]).
    Texture(wgpu::Texture),
}
/// Bytes of the frame constants.
const FRAME_BYTES: u64 = 288;

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
const LIGHT_SLOT: u64 = 256;
/// Light passes the constants buffer holds at first; it grows beyond.
const LIGHT_SLOTS: u64 = 64;

/// A frustum's planes (a, b, c, d), `a x + b y + c z + d ≥ 0` inside,
/// normalised, from a row-vector clip matrix (0 ≤ z ≤ w).
fn frustum_planes(m: &Mat4) -> [[f32; 4]; 6] {
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
fn meets(planes: &[[f32; 4]; 6], s: [f32; 4]) -> bool {
    planes
        .iter()
        .all(|p| p[0] * s[0] + p[1] * s[1] + p[2] * s[2] + p[3] >= -s[3])
}

/// A kept draw whose surface has light elements of priority 0 or 1
/// (section 5.5, rule 4).
#[derive(Clone, Copy, Debug)]
struct LitDraw {
    /// Its index among the kept draws (its record).
    kept: u32,
    program: LightProgram,
    surface: u32,
    vertex_buffer: u32,
    index_buffer: u32,
    first_vertex: u32,
    first_index: u32,
    index_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PipelineKey {
    program: Program,
    blend: Blend,
    depth_write: bool,
}

/// Pipelines and layouts shared by every world drawn into one format.
pub(crate) struct WorldPipelines {
    format: wgpu::TextureFormat,
    frame_layout: wgpu::BindGroupLayout,
    surface_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    samplers: [wgpu::Sampler; 3],
    white: wgpu::TextureView,
    white_cube: wgpu::TextureView,
    sky: SkyPipelines,
    /// The light passes (dynamic-lights.md 6): their layout, pipelines,
    /// the attenuation sampler, the constants (one slot per pass) and the
    /// point and spot bind groups once the textures are given.
    light_layout: wgpu::BindGroupLayout,
    light_pipelines: HashMap<LightProgram, wgpu::RenderPipeline>,
    light_sampler: wgpu::Sampler,
    light_constants: wgpu::Buffer,
    light_slots: u64,
    light_textures: Option<LightTextures>,
    light_groups: Option<[wgpu::BindGroup; 2]>,
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn vertex_layout(layout: VertexLayout) -> wgpu::VertexBufferLayout<'static> {
    const LIGHT_MAPPED: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Unorm8x4, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Sint16x2, 5 => Sint16x2
    ];
    const VERTEX_LIT: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Unorm8x4, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Unorm8x4, 5 => Sint16x2
    ];
    const TREE: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Unorm8x4, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Sint16x4
    ];
    const MODEL: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Sint16x4, 1 => Unorm8x4, 2 => Sint16x2
    ];
    wgpu::VertexBufferLayout {
        array_stride: match layout {
            VertexLayout::Model => MODEL_VERTEX_BYTES as u64,
            VertexLayout::Level(_) => 32,
        },
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: match layout {
            VertexLayout::Level(Layout::LightMapped) => &LIGHT_MAPPED,
            VertexLayout::Level(Layout::Tree) => &TREE,
            VertexLayout::Model => &MODEL,
            _ => &VERTEX_LIT,
        },
    }
}

impl WorldPipelines {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        compat: bool,
    ) -> Self {
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world frame"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: if compat {
                        wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        }
                    } else {
                        wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        }
                    },
                    count: None,
                },
            ],
        });
        let surface_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world surface"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                texture_entry(2),
                texture_entry(3),
                sampler_entry(4),
                sampler_entry(5),
                sampler_entry(6),
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world"),
            bind_group_layouts: &[Some(&frame_layout), Some(&surface_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world"),
            source: wgpu::ShaderSource::Wgsl(world_source(compat).into()),
        });
        // Section 4.4: s_base / s_detail wrap, anisotropic (linear
        // min/mag/mip with anisotropy); s_lmap wrap, linear; s_hemi clamp,
        // linear, no mip. Mip bias 0 (`r1_tf_mipbias`).
        let linear = wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        };
        let samplers = [
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("s_base"),
                anisotropy_clamp: ANISOTROPY,
                ..linear.clone()
            }),
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("s_lmap"),
                ..linear.clone()
            }),
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("s_hemi"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                lod_max_clamp: 0.0,
                ..linear
            }),
        ];
        let white = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("white"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            Default::default(),
            &[255; 4],
        );
        let white_cube = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("white cube"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            Default::default(),
            &[255; 24],
        );
        // dynamic-lights.md 6.2: s_lmap and s_att clamp, linear, the first
        // level only.
        let light_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("s_att"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_max_clamp: 0.0,
            ..Default::default()
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world light"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(LIGHT_SLOT),
                    },
                    count: None,
                },
                texture_entry(1),
                texture_entry(2),
                sampler_entry(3),
            ],
        });
        let light_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("world light"),
                bind_group_layouts: &[
                    Some(&frame_layout),
                    Some(&surface_layout),
                    Some(&light_layout),
                ],
                immediate_size: 0,
            });
        // One / one, the result added and clamped (section 6.2).
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let light_pipelines = LightProgram::ALL
            .iter()
            .map(|&program| {
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(program.entry_point()),
                    layout: Some(&light_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(program.entry_point()),
                        compilation_options: Default::default(),
                        buffers: &[Some(vertex_layout(program.layout()))],
                    },
                    primitive: wgpu::PrimitiveState {
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: Some(wgpu::Face::Back),
                        ..Default::default()
                    },
                    // Depth test on (less or equal), write off.
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::LessEqual),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_light"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(wgpu::BlendState {
                                color: add,
                                alpha: add,
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                });
                (program, pipeline)
            })
            .collect();
        Self {
            format,
            frame_layout,
            surface_layout,
            pipeline_layout,
            shader,
            pipelines: HashMap::new(),
            samplers,
            white: white.create_view(&Default::default()),
            white_cube: white_cube.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            }),
            sky: SkyPipelines::new(device, format),
            light_layout,
            light_pipelines,
            light_sampler,
            light_constants: light_buffer(device, LIGHT_SLOTS),
            light_slots: LIGHT_SLOTS,
            light_textures: None,
            light_groups: None,
        }
    }

    /// The point and spot bind groups over the constants and `textures`
    /// (section 6.2: point `s_lmap` and `s_att` the round falloff; spot
    /// `s_lmap` the projected pattern, `s_att` the depth falloff).
    fn light_groups(&mut self, device: &wgpu::Device, textures: LightTextures) {
        let view = |t: &Texture| {
            t.texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                array_layer_count: Some(1),
                ..Default::default()
            })
        };
        let group = |lmap: &Texture, att: &Texture| {
            let (lmap, att) = (view(lmap), view(att));
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("world light"),
                layout: &self.light_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.light_constants,
                            offset: 0,
                            size: wgpu::BufferSize::new(LIGHT_SLOT),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&lmap),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&att),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.light_sampler),
                    },
                ],
            })
        };
        self.light_groups = Some([
            group(&textures.attpoint, &textures.attpoint),
            group(&textures.att, &textures.attclip),
        ]);
        self.light_textures = Some(textures);
    }

    /// Room for `passes` light passes' constants, grown as needed.
    fn light_room(&mut self, device: &wgpu::Device, passes: u64) {
        if passes <= self.light_slots {
            return;
        }
        self.light_slots = passes.next_power_of_two();
        self.light_constants = light_buffer(device, self.light_slots);
        if let Some(t) = self.light_textures.take() {
            self.light_groups(device, t);
        }
    }

    fn pipeline(&mut self, device: &wgpu::Device, key: PipelineKey) -> &wgpu::RenderPipeline {
        let Self {
            format,
            pipeline_layout,
            shader,
            pipelines,
            ..
        } = self;
        pipelines.entry(key).or_insert_with(|| {
            let (vs, fs) = key.program.entry_points();
            let additive = wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            };
            let blend = match key.blend {
                Blend::Off => None,
                // Direct3D 9 applies the colour factors to alpha too.
                Blend::Alpha => Some(alpha_blending()),
                Blend::Add => Some(wgpu::BlendState {
                    color: additive,
                    alpha: additive,
                }),
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_layout(key.program.layout()))],
                },
                // Triangles that appear counter-clockwise on screen are
                // culled (section 10.1).
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(key.depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: *format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }
}

/// The light passes' constants buffer for `slots` passes.
fn light_buffer(device: &wgpu::Device, slots: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("world lights"),
        size: slots * LIGHT_SLOT,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

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

fn vec4(v: [f32; 3], w: f32) -> [f32; 4] {
    [v[0], v[1], v[2], w]
}

/// Source alpha / inverse source alpha for colour and alpha alike.
pub(crate) fn alpha_blending() -> wgpu::BlendState {
    let c = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    };
    wgpu::BlendState { color: c, alpha: c }
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

/// The original's render target for a `width` × `height` screen (section
/// 10.2): each axis × √`r__supersample` (default 2), clamped to 128 … 2048
/// and made even. Truncating the product and making it even by rounding
/// down are our choices (the spec does not say how). We draw at the screen
/// size; this target only feeds the size threshold, so culling matches the
/// original's.
pub fn supersampled(width: u32, height: u32) -> (u32, u32) {
    let axis = |v: u32| ((v as f32 * 2f32.sqrt()) as u32).clamp(128, 2048) & !1;
    (axis(width), axis(height))
}

/// ssa_min for a `width` × `height` render target and vertical field of
/// view `fov` degrees (section 10.2, `r__geometry_lod` 1).
pub fn ssa_min(width: u32, height: u32, fov: f32) -> f32 {
    let k = 1.0 / ((1.0 + 1e-7) * (90.0 / fov).powi(2) * width as f32 * height as f32);
    12.25 * k
}

/// Bytes a texture occupies on the GPU (all faces and levels).
pub fn texture_bytes(t: &wgpu::Texture) -> u64 {
    let format = t.format();
    let (bw, bh) = format.block_dimensions();
    let block = u64::from(format.block_copy_size(None).unwrap_or(4));
    (0..t.mip_level_count())
        .map(|level| {
            let w = (t.width() >> level).max(1).div_ceil(bw);
            let h = (t.height() >> level).max(1).div_ceil(bh);
            u64::from(w) * u64::from(h) * block
        })
        .sum::<u64>()
        * u64::from(t.depth_or_array_layers())
}

struct Inner {
    /// Kept for as long as the bundles that draw from them.
    vertex_buffers: Vec<wgpu::Buffer>,
    index_buffers: Vec<wgpu::Buffer>,
    /// Each index buffer's format: 16-bit as stored, 32-bit for indices
    /// rebased past 16 bits ([`Renderer::compat`]).
    index_formats: Vec<wgpu::IndexFormat>,
    /// Per-draw data, one record per kept draw; placements rewrite it.
    draw_data: DrawData,
    /// Each draw given to the world: its index among the kept draws.
    kept_of: Vec<Option<u32>>,
    _textures: Vec<Texture>,
    frame: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    surface_groups: Vec<wgpu::BindGroup>,
    /// Pipeline of each surface.
    surface_pipelines: Vec<wgpu::RenderPipeline>,
    /// Section 10.3 step 1 (priority 0–1, not strict) and step 5 (2–3).
    early: wgpu::RenderBundle,
    late: wgpu::RenderBundle,
    /// Step 6: strict-sorted draws, ordered each frame.
    strict: RefCell<Vec<(usize, WorldDraw)>>,
    /// The draws light passes may draw again.
    lit: Vec<LitDraw>,
    /// The bounding sphere each kept draw's record holds now.
    spheres: RefCell<Vec<[f32; 4]>>,
    sky: Option<SkyResources>,
    sky_error: Option<String>,
    draws: usize,
    skipped: usize,
    buffer_bytes: u64,
    texture_bytes: u64,
    object_draws: usize,
    object_buffer_bytes: u64,
    object_texture_bytes: u64,
}

/// A static world uploaded to the GPU; cheap to clone (a handle).
#[derive(Clone)]
pub struct World(Rc<Inner>);

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("draws", &self.0.draws)
            .field("buffer_bytes", &self.0.buffer_bytes)
            .field("texture_bytes", &self.0.texture_bytes)
            .finish()
    }
}

impl World {
    /// Bytes of GPU buffers held (vertices, indices, per-draw data).
    pub fn gpu_bytes(&self) -> u64 {
        self.0.buffer_bytes
    }

    /// Bytes of the textures the surfaces use.
    pub fn texture_bytes(&self) -> u64 {
        self.0.texture_bytes
    }

    /// Draws of the static world.
    pub fn draw_count(&self) -> usize {
        self.0.draws
    }

    /// Draws of the objects' models.
    pub fn object_draw_count(&self) -> usize {
        self.0.object_draws
    }

    /// Bytes of GPU buffers held for the objects' models and their draws
    /// (included in [`World::gpu_bytes`]).
    pub fn object_gpu_bytes(&self) -> u64 {
        self.0.object_buffer_bytes
    }

    /// Bytes of the textures only the objects' surfaces use (included in
    /// [`World::texture_bytes`]).
    pub fn object_texture_bytes(&self) -> u64 {
        self.0.object_texture_bytes
    }

    /// Draws left out because their vertex layout does not suit their
    /// surface's program, or their surface is missing.
    pub fn skipped(&self) -> usize {
        self.0.skipped
    }

    /// Why the sky could not be set up, if it could not.
    pub fn sky_error(&self) -> Option<&str> {
        self.0.sky_error.as_deref()
    }

    /// The bounding sphere draw `draw` (its index among the draws the world
    /// was created with) is drawn with now; radius 0: hidden. `None` for a
    /// draw left out.
    pub fn draw_sphere(&self, draw: usize) -> Option<[f32; 4]> {
        let k = (*self.0.kept_of.get(draw)?)?;
        Some(self.0.spheres.borrow()[k as usize])
    }

    pub fn same(&self, other: &World) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn draw_record(surface: &Surface<Texture>, d: &WorldDraw, out: &mut Vec<u8>) {
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

/// A buffer holding `contents` (its size rounded up to whole 4-byte words,
/// zero-padded), written with `Queue::write_buffer`. Unlike a buffer mapped
/// at creation, this makes no copy of `contents` in the wasm heap on the web
/// (wgpu's WebGPU backend passes the bytes to the browser as they are).
pub(crate) fn upload_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    usage: wgpu::BufferUsages,
    contents: &[u8],
) -> wgpu::Buffer {
    let size = (contents.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let whole = contents.len() / 4 * 4;
    if whole > 0 {
        queue.write_buffer(&buffer, 0, &contents[..whole]);
    }
    if whole < contents.len() {
        let mut tail = [0; 4];
        tail[..contents.len() - whole].copy_from_slice(&contents[whole..]);
        queue.write_buffer(&buffer, whole as u64, &tail);
    }
    buffer
}

/// [`Renderer::compat`]: an index range of a stored index buffer, uploaded
/// with `base` added at index `at` of the rebased buffer.
#[derive(Clone, Copy)]
struct Segment {
    at: u32,
    first: u32,
    count: u32,
    base: u32,
}

/// [`Renderer::compat`]: where each draw's rebased index range begins, by
/// (object draw, index buffer, first index, index count, base vertex).
#[derive(Default)]
struct Rebased(HashMap<(bool, u32, u32, u32, u32), u32>);

impl Rebased {
    fn key(d: &WorldDraw) -> (bool, u32, u32, u32, u32) {
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
    fn add(&mut self, d: &WorldDraw, place: impl FnOnce() -> u32) {
        self.0.entry(Self::key(d)).or_insert_with(place);
    }

    fn first_index(&self, d: &WorldDraw) -> u32 {
        self.0.get(&Self::key(d)).copied().unwrap_or(d.first_index)
    }
}

/// Fills `out`, the bytes from `at` of a rebased index buffer laid out by
/// `segments`, from stored index buffer `source` of `geom`.
fn fill_rebased(
    geom: &mut dyn GeomSource,
    source: GeomBuffer,
    segments: &[Segment],
    at: u64,
    out: &mut [u8],
) -> io::Result<()> {
    let first = (at / 2) as u32;
    let n = (out.len() / 2) as u32;
    let mut k = segments.partition_point(|s| s.at + s.count <= first);
    let mut done = 0;
    while done < n {
        let s = segments
            .get(k)
            .ok_or_else(|| io::Error::other("rebased indices past their ranges"))?;
        let from = first + done - s.at;
        let take = (s.count - from).min(n - done);
        let dst = &mut out[2 * done as usize..2 * (done + take) as usize];
        geom.read(source, 2 * u64::from(s.first + from), dst)?;
        for pair in dst.as_chunks_mut::<2>().0 {
            let index = u32::from(u16::from_le_bytes(*pair)) + s.base;
            let index = u16::try_from(index).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "a rebased index passes 16 bits")
            })?;
            *pair = index.to_le_bytes();
        }
        done += take;
        k += 1;
    }
    Ok(())
}

/// Where a buffer of a level's geometry takes its bytes from.
enum Source {
    /// Stored vertex buffer `i` of the level's geometry.
    Vertices(u32),
    /// Stored index buffer `i`, as stored.
    Indices(u32),
    /// Stored index buffer `i`, rebased by its segments
    /// ([`Renderer::compat`]).
    Rebased(u32),
    /// The objects' vertices.
    ObjectVertices,
    /// The objects' indices, as held.
    ObjectIndices,
    /// Bytes made when the upload began (the objects' rebased indices).
    Held(Vec<u8>),
}

/// A buffer of a [`GeometryUpload`] and where its bytes come from.
struct Job {
    buffer: wgpu::Buffer,
    label: &'static str,
    source: Source,
    bytes: u64,
}

/// A level's static geometry and the objects' models on their way to the
/// GPU ([`Renderer::begin_geometry`]): filled a piece at a time by
/// [`GeometryUpload::step`], so a platform can spread it over task turns.
/// On wgpu's GL backend written bytes stay staged in CPU memory (the wasm
/// heap on the web) until a submission is known finished, which WebGL
/// reports only after control returns to the browser (documented: the
/// WebGL 2 specification's sync objects never signal in the frame they
/// were issued), so an upload done at once gathers all of it there (#184).
pub struct GeometryUpload {
    jobs: Vec<Job>,
    /// The job being filled, and how far.
    next: usize,
    at: u64,
    piece: Vec<u8>,
    compat: bool,
    segments: Vec<Vec<Segment>>,
    rebased: Rebased,
    vertex_buffers: Vec<wgpu::Buffer>,
    index_buffers: Vec<wgpu::Buffer>,
    index_formats: Vec<wgpu::IndexFormat>,
    vertex_layouts: Vec<Option<Layout>>,
    buffer_bytes: u64,
    object_buffer_bytes: u64,
    object_vb: u32,
    object_ib: u32,
    /// The draws' index ranges it was begun for.
    ranges: Vec<(bool, u32, u32, u32, u32)>,
}

impl GeometryUpload {
    /// Uploads about `budget` more bytes (whole pieces of at most
    /// [`UPLOAD_PIECE`]), reading the level's buffers from `geom` and the
    /// objects' from `objects` (those it was begun with). Returns whether
    /// everything is uploaded.
    pub fn step(
        &mut self,
        renderer: &Renderer,
        geom: &mut dyn GeomSource,
        objects: &ObjectGeometry,
        budget: u64,
    ) -> Result<bool, String> {
        let mut spent = 0;
        while let Some(job) = self.jobs.get(self.next) {
            if self.at >= job.bytes {
                self.next += 1;
                self.at = 0;
                continue;
            }
            if spent >= budget {
                return Ok(false);
            }
            let at = self.at;
            let n = (job.bytes - at).min(UPLOAD_PIECE as u64) as usize;
            self.piece.clear();
            self.piece.resize(n.next_multiple_of(4), 0);
            let out = &mut self.piece[..n];
            let range = at as usize..at as usize + n;
            match &job.source {
                Source::Vertices(i) => geom.read(GeomBuffer::Vertices(*i), at, out),
                Source::Indices(i) => geom.read(GeomBuffer::Indices(*i), at, out),
                Source::Rebased(i) => fill_rebased(
                    geom,
                    GeomBuffer::Indices(*i),
                    &self.segments[*i as usize],
                    at,
                    out,
                ),
                Source::ObjectVertices => objects
                    .vertices
                    .get(range)
                    .map(|v| out.copy_from_slice(v))
                    .ok_or_else(|| io::Error::other("the objects' vertices changed")),
                Source::ObjectIndices => {
                    for (j, o) in out.iter_mut().enumerate() {
                        let at = at as usize + j;
                        *o = objects.indices[at / 2].to_le_bytes()[at % 2];
                    }
                    Ok(())
                }
                Source::Held(bytes) => {
                    out.copy_from_slice(&bytes[range]);
                    Ok(())
                }
            }
            .map_err(|e| format!("{}: {e}", job.label))?;
            renderer.queue.write_buffer(&job.buffer, at, &self.piece);
            if self.compat {
                // Each piece submitted at once, so the staging of finished
                // ones can go (see the type's description).
                renderer.queue.submit([]);
                let _ = renderer.device.poll(wgpu::PollType::Poll);
            }
            self.at += n as u64;
            spent += n as u64;
        }
        Ok(true)
    }

    /// The bytes it uploads in all.
    pub fn bytes(&self) -> u64 {
        self.jobs.iter().map(|j| j.bytes).sum()
    }
}

impl Renderer {
    /// Uploads `geom`'s buffers as stored and records `draws` with
    /// `surfaces`. A draw whose vertex layout does not suit its surface's
    /// program, or whose surface is missing, is left out and counted
    /// ([`World::skipped`]). `sky` sets up the sky passes; when that fails
    /// the world has no sky ([`World::sky_error`]). Fails when a draw names
    /// a buffer `geom` lacks.
    pub fn create_world(
        &mut self,
        geom: &LevelGeom,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        self.create_world_with(geom, &ObjectGeometry::default(), surfaces, draws, sky)
    }

    /// [`Renderer::create_world`] with the objects' models: `objects`'
    /// buffers are uploaded beside `geom`'s, and draws with
    /// [`WorldDraw::object`] set read them. Fails when such a draw's range
    /// lies outside them.
    pub fn create_world_with(
        &mut self,
        geom: &LevelGeom,
        objects: &ObjectGeometry,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        self.create_world_from(&mut geom.source(), objects, surfaces, draws, sky)
    }

    /// [`Renderer::create_world_with`] reading the static geometry from
    /// `geom` as it uploads it, in pieces of at most [`UPLOAD_PIECE`]
    /// bytes, so the CPU never holds more of it than one piece. Also fails
    /// when reading fails.
    pub fn create_world_from(
        &mut self,
        geom: &mut dyn GeomSource,
        objects: &ObjectGeometry,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        let mut upload = self.begin_geometry(geom.layout(), objects, draws)?;
        upload.step(self, geom, objects, u64::MAX)?;
        self.finish_world(upload, objects, surfaces, draws, sky)
    }

    /// Begins uploading a level's static geometry laid out as `layout` and
    /// the objects' models for `draws` ([`Renderer::create_world_from`] in
    /// steps): the buffers are made, empty, and filled by
    /// [`GeometryUpload::step`]; [`Renderer::finish_world`] then makes the
    /// world. Fails when a draw names a buffer the geometry lacks.
    pub fn begin_geometry(
        &mut self,
        layout: &GeomLayout,
        objects: &ObjectGeometry,
        draws: &[WorldDraw],
    ) -> Result<GeometryUpload, String> {
        if !objects.vertices.len().is_multiple_of(MODEL_VERTEX_BYTES) {
            return Err("object vertices are not whole model vertices".into());
        }
        let object_vertices = (objects.vertices.len() / MODEL_VERTEX_BYTES) as u64;
        for d in draws {
            let fits = if d.object.is_some() {
                u64::from(d.first_index) + u64::from(d.index_count) <= objects.indices.len() as u64
                    && u64::from(d.first_vertex) < object_vertices.max(1)
            } else {
                (d.vertex_buffer as usize) < layout.vertex_buffers.len()
                    && (d.index_buffer as usize) < layout.index_counts.len()
            };
            if !fits {
                return Err(format!("draw {d:?} names a missing buffer"));
            }
        }
        let vertex_layouts: Vec<Option<Layout>> =
            layout.vertex_buffers.iter().map(|b| b.layout).collect();
        let compat = self.compat;
        let device = &self.device;
        let new_buffer = |label: &str, usage: wgpu::BufferUsages, bytes: u64, min_size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes
                    .next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
                    .max(min_size),
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let mut jobs = Vec::new();
        let mut buffer_bytes = 0;
        let mut vertex_buffers = Vec::with_capacity(layout.vertex_buffers.len() + 1);
        for (i, b) in layout.vertex_buffers.iter().enumerate() {
            let bytes = b.bytes();
            buffer_bytes += bytes;
            let label = "level.geom vertices";
            let buffer = new_buffer(label, wgpu::BufferUsages::VERTEX, bytes, 0);
            vertex_buffers.push(buffer.clone());
            jobs.push(Job {
                buffer,
                label,
                source: Source::Vertices(i as u32),
                bytes,
            });
        }
        // Without base vertices (compat), each index range a draw uses is
        // uploaded with its draw's first vertex added, ranges one after
        // another per index buffer (a range drawn from two bases twice);
        // the draws then read from vertex 0.
        let mut rebased = Rebased::default();
        let mut segments: Vec<Vec<Segment>> = vec![Vec::new(); layout.index_counts.len()];
        if compat {
            for d in draws.iter().filter(|d| d.object.is_none()) {
                let segments = &mut segments[d.index_buffer as usize];
                rebased.add(d, || {
                    let at = segments.last().map_or(0, |s| s.at + s.count);
                    segments.push(Segment {
                        at,
                        first: d.first_index,
                        count: d.index_count,
                        base: d.first_vertex,
                    });
                    at
                });
            }
        }
        let mut index_buffers = Vec::with_capacity(layout.index_counts.len() + 1);
        let mut index_formats = Vec::with_capacity(layout.index_counts.len() + 1);
        for (i, &count) in layout.index_counts.iter().enumerate() {
            let bytes = if compat {
                2 * segments[i].last().map_or(0, |s| u64::from(s.at + s.count))
            } else {
                2 * u64::from(count)
            };
            // Index buffers are at least 4 bytes, as before any were empty.
            buffer_bytes += bytes.next_multiple_of(4).max(4);
            index_formats.push(wgpu::IndexFormat::Uint16);
            let label = "level.geom indices";
            let buffer = new_buffer(label, wgpu::BufferUsages::INDEX, bytes, 4);
            index_buffers.push(buffer.clone());
            jobs.push(Job {
                buffer,
                label,
                source: if compat {
                    Source::Rebased(i as u32)
                } else {
                    Source::Indices(i as u32)
                },
                bytes,
            });
        }
        // The objects' buffers follow the level's.
        let (object_vb, object_ib) = (vertex_buffers.len() as u32, index_buffers.len() as u32);
        let mut object_buffer_bytes = 0;
        if !objects.vertices.is_empty() {
            // Whole 16-byte vertices: a multiple of 4 bytes, as buffers need.
            let bytes = objects.vertices.len() as u64;
            let label = "object vertices";
            let buffer = new_buffer(label, wgpu::BufferUsages::VERTEX, bytes, 0);
            vertex_buffers.push(buffer.clone());
            jobs.push(Job {
                buffer,
                label,
                source: Source::ObjectVertices,
                bytes,
            });
            object_buffer_bytes += bytes;
            let label = "object indices";
            if compat {
                // Rebased in memory (the objects' indices are held there),
                // 32-bit once they pass 16 bits.
                let mut out: Vec<u32> = Vec::new();
                for d in draws.iter().filter(|d| d.object.is_some()) {
                    rebased.add(d, || {
                        let at = out.len() as u32;
                        let range =
                            d.first_index as usize..(d.first_index + d.index_count) as usize;
                        out.extend(
                            objects.indices[range]
                                .iter()
                                .map(|&i| u32::from(i) + d.first_vertex),
                        );
                        at
                    });
                }
                let wide = out.iter().any(|&i| i > u32::from(u16::MAX));
                let mut held: Vec<u8> = if wide {
                    out.iter().flat_map(|i| i.to_le_bytes()).collect()
                } else {
                    out.iter().flat_map(|&i| (i as u16).to_le_bytes()).collect()
                };
                drop(out);
                held.resize(held.len().max(4), 0);
                let bytes = held.len() as u64;
                object_buffer_bytes += bytes.next_multiple_of(4);
                index_formats.push(if wide {
                    wgpu::IndexFormat::Uint32
                } else {
                    wgpu::IndexFormat::Uint16
                });
                let buffer = new_buffer(label, wgpu::BufferUsages::INDEX, bytes, 0);
                index_buffers.push(buffer.clone());
                jobs.push(Job {
                    buffer,
                    label,
                    source: Source::Held(held),
                    bytes,
                });
            } else {
                let bytes = 2 * objects.indices.len() as u64;
                object_buffer_bytes += bytes.next_multiple_of(4).max(4);
                index_formats.push(wgpu::IndexFormat::Uint16);
                let buffer = new_buffer(label, wgpu::BufferUsages::INDEX, bytes, 4);
                index_buffers.push(buffer.clone());
                jobs.push(Job {
                    buffer,
                    label,
                    source: Source::ObjectIndices,
                    bytes,
                });
            }
        }
        Ok(GeometryUpload {
            jobs,
            next: 0,
            at: 0,
            piece: Vec::new(),
            compat,
            segments,
            rebased,
            vertex_buffers,
            index_buffers,
            index_formats,
            vertex_layouts,
            buffer_bytes,
            object_buffer_bytes,
            object_vb,
            object_ib,
            ranges: draws.iter().map(Rebased::key).collect(),
        })
    }

    /// The world of a finished [`GeometryUpload`] (begun with the same
    /// `objects` and `draws`), with `surfaces` and `sky` as
    /// [`Renderer::create_world_from`] takes them.
    pub fn finish_world(
        &mut self,
        upload: GeometryUpload,
        objects: &ObjectGeometry,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        if upload.next < upload.jobs.len() {
            return Err("the geometry is not all uploaded".into());
        }
        if upload.ranges.len() != draws.len()
            || upload
                .ranges
                .iter()
                .zip(draws)
                .any(|(r, d)| *r != Rebased::key(d))
        {
            return Err("the geometry was uploaded for other draws".into());
        }
        let GeometryUpload {
            rebased,
            vertex_buffers,
            index_buffers,
            index_formats,
            vertex_layouts,
            mut buffer_bytes,
            mut object_buffer_bytes,
            object_vb,
            object_ib,
            ..
        } = upload;
        let device = self.device.clone();
        let queue = self.queue.clone();
        let format = self.format;
        let compat = self.compat;
        if self.world_pipelines.is_none() {
            self.make_world_pipelines();
        }
        let pipelines = self.world_pipelines.as_mut().expect("made");
        buffer_bytes += object_buffer_bytes;

        // Draws that can be drawn: surface present, layouts agree. Object
        // draws are pointed at the objects' buffers.
        let usable = |d: &WorldDraw| {
            surfaces.get(d.surface as usize).is_some_and(|s| {
                if d.object.is_some() {
                    s.program.layout() == VertexLayout::Model && !objects.vertices.is_empty()
                } else {
                    vertex_layouts[d.vertex_buffer as usize].map(VertexLayout::Level)
                        == Some(s.program.layout())
                }
            })
        };
        let mut kept_of = Vec::with_capacity(draws.len());
        let mut next = 0u32;
        for d in draws {
            kept_of.push(usable(d).then(|| {
                next += 1;
                next - 1
            }));
        }
        let kept: Vec<WorldDraw> = draws
            .iter()
            .copied()
            .filter(usable)
            .map(|mut d| {
                if compat {
                    d.first_index = rebased.first_index(&d);
                    d.first_vertex = 0;
                }
                if d.object.is_some() {
                    d.vertex_buffer = object_vb;
                    d.index_buffer = object_ib;
                }
                d
            })
            .collect();
        let skipped = draws.len() - kept.len();
        let object_draws = kept.iter().filter(|d| d.object.is_some()).count();
        // The draws light passes may draw again (dynamic-lights.md 5.5
        // rule 4), grouped by program and surface.
        let mut lit: Vec<LitDraw> = (kept.iter().enumerate())
            .filter_map(|(k, d)| {
                let s = &surfaces[d.surface as usize];
                let light = s.light.filter(|l| l.priority <= 1)?;
                (light.program.layout() == s.program.layout()).then_some(LitDraw {
                    kept: k as u32,
                    program: light.program,
                    surface: d.surface,
                    vertex_buffer: d.vertex_buffer,
                    index_buffer: d.index_buffer,
                    first_vertex: d.first_vertex,
                    first_index: d.first_index,
                    index_count: d.index_count,
                })
            })
            .collect();
        lit.sort_by_key(|l| (l.program as u8, l.surface, l.vertex_buffer, l.index_buffer));
        lit.shrink_to_fit();

        let mut records = Vec::with_capacity(kept.len().max(1) * DRAW_BYTES);
        for d in &kept {
            draw_record(&surfaces[d.surface as usize], d, &mut records);
        }
        records.resize(records.len().max(DRAW_BYTES), 0);
        buffer_bytes += records.len() as u64;
        object_buffer_bytes += (object_draws * DRAW_BYTES) as u64;
        let draw_data = if compat {
            // Whole rows of the draws texture.
            let rows = (kept.len().max(1) as u32).div_ceil(DRAWS_PER_ROW);
            records.resize(rows as usize * DRAWS_PER_ROW as usize * DRAW_BYTES, 0);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("world draws"),
                size: wgpu::Extent3d {
                    width: DRAWS_PER_ROW * DRAW_TEXELS,
                    height: rows,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                texture.as_image_copy(),
                &records,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(DRAWS_PER_ROW * DRAW_BYTES as u32),
                    rows_per_image: None,
                },
                texture.size(),
            );
            DrawData::Texture(texture)
        } else {
            DrawData::Buffer(upload_buffer(
                &device,
                &queue,
                "world draws",
                wgpu::BufferUsages::STORAGE,
                &records,
            ))
        };
        let draw_view;
        let draw_resource = match &draw_data {
            DrawData::Buffer(b) => b.as_entire_binding(),
            DrawData::Texture(t) => {
                draw_view = t.create_view(&Default::default());
                wgpu::BindingResource::TextureView(&draw_view)
            }
        };
        let frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world frame"),
            size: FRAME_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world frame"),
            layout: &pipelines.frame_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: draw_resource,
                },
            ],
        });

        // Textures once each (by handle), for their views and memory; a
        // texture counts as the objects' when no static draw's surface uses
        // it.
        let mut static_surface = vec![false; surfaces.len()];
        for d in kept.iter().filter(|d| d.object.is_none()) {
            static_surface[d.surface as usize] = true;
        }
        let mut textures: Vec<(Texture, bool)> = Vec::new();
        for (s, &st) in surfaces.iter().zip(&static_surface) {
            for t in s.textures.iter().chain([&s.env]).flatten() {
                match textures.iter_mut().find(|(k, _)| k.texture == t.texture) {
                    Some((_, used)) => *used |= st,
                    None => textures.push((t.clone(), st)),
                }
            }
        }
        let (mut texture_bytes, mut object_texture_bytes) = (0, 0);
        for (t, st) in &textures {
            let bytes = self::texture_bytes(&t.texture);
            texture_bytes += bytes;
            if !st {
                object_texture_bytes += bytes;
            }
        }
        let mut textures: Vec<Texture> = textures.into_iter().map(|(t, _)| t).collect();
        let surface_groups: Vec<wgpu::BindGroup> = surfaces
            .iter()
            .map(|s| {
                let mut views: Vec<wgpu::TextureView> = s
                    .textures
                    .iter()
                    .map(|t| match t {
                        Some(t) => t.texture.create_view(&wgpu::TextureViewDescriptor {
                            dimension: Some(wgpu::TextureViewDimension::D2),
                            array_layer_count: Some(1),
                            ..Default::default()
                        }),
                        None => pipelines.white.clone(),
                    })
                    .collect();
                // A map that is not a 6-face square texture binds white (our
                // choice).
                views.push(match &s.env {
                    Some(t)
                        if t.texture.depth_or_array_layers() == 6
                            && t.texture.width() == t.texture.height() =>
                    {
                        t.texture.create_view(&wgpu::TextureViewDescriptor {
                            dimension: Some(wgpu::TextureViewDimension::Cube),
                            array_layer_count: Some(6),
                            ..Default::default()
                        })
                    }
                    _ => pipelines.white_cube.clone(),
                });
                let mut entries: Vec<wgpu::BindGroupEntry> = views
                    .iter()
                    .enumerate()
                    .map(|(i, v)| wgpu::BindGroupEntry {
                        // The cube map is binding 7, after the samplers.
                        binding: if i < 4 { i as u32 } else { 7 },
                        resource: wgpu::BindingResource::TextureView(v),
                    })
                    .collect();
                for (i, sampler) in pipelines.samplers.iter().enumerate() {
                    entries.push(wgpu::BindGroupEntry {
                        binding: 4 + i as u32,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    });
                }
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("world surface"),
                    layout: &pipelines.surface_layout,
                    entries: &entries,
                })
            })
            .collect();
        let surface_pipelines: Vec<wgpu::RenderPipeline> = surfaces
            .iter()
            .map(|s| {
                pipelines
                    .pipeline(
                        &device,
                        PipelineKey {
                            program: s.program,
                            blend: s.blend,
                            depth_write: s.depth_write,
                        },
                    )
                    .clone()
            })
            .collect();

        // Section 10.3: step 1 is priorities 0–1, step 5 priorities 2–3,
        // each grouped by state (opaque before blended); strict-sorted
        // draws are step 6. Blended draws within a step keep this fixed
        // order rather than near to far by ssa (our choice: the exact order
        // is unestablished, and a fixed one keeps them in the bundles).
        let mut early = Vec::new();
        let mut late = Vec::new();
        let mut strict = Vec::new();
        for (i, d) in kept.iter().enumerate() {
            let s = &surfaces[d.surface as usize];
            if s.strict {
                strict.push((i, *d));
            } else if s.priority <= 1 {
                early.push(i);
            } else {
                late.push(i);
            }
        }
        let order_key = |i: &usize| {
            let d = &kept[*i];
            let s = &surfaces[d.surface as usize];
            (
                s.blend != Blend::Off,
                s.program as u8,
                d.surface,
                d.vertex_buffer,
                d.index_buffer,
            )
        };
        early.sort_by_key(order_key);
        late.sort_by_key(order_key);
        let record = |order: &[usize]| {
            let mut encoder =
                device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                    label: Some("world"),
                    color_formats: &[Some(format)],
                    depth_stencil: Some(wgpu::RenderBundleDepthStencil {
                        format: DEPTH_FORMAT,
                        depth_read_only: false,
                        stencil_read_only: true,
                    }),
                    sample_count: 1,
                    multiview: None,
                });
            encoder.set_bind_group(0, &frame_group, &[]);
            let mut surface = None;
            let mut buffers = None;
            for &i in order {
                let d = &kept[i];
                if surface != Some(d.surface) {
                    surface = Some(d.surface);
                    encoder.set_pipeline(&surface_pipelines[d.surface as usize]);
                    encoder.set_bind_group(1, &surface_groups[d.surface as usize], &[]);
                }
                if buffers != Some((d.vertex_buffer, d.index_buffer)) {
                    buffers = Some((d.vertex_buffer, d.index_buffer));
                    encoder
                        .set_vertex_buffer(0, vertex_buffers[d.vertex_buffer as usize].slice(..));
                    encoder.set_index_buffer(
                        index_buffers[d.index_buffer as usize].slice(..),
                        index_formats[d.index_buffer as usize],
                    );
                }
                let instance = i as u32;
                encoder.draw_indexed(
                    d.first_index..d.first_index + d.index_count,
                    d.first_vertex as i32,
                    instance..instance + 1,
                );
            }
            encoder.finish(&wgpu::RenderBundleDescriptor {
                label: Some("world"),
            })
        };
        if compat {
            self.queue.submit([]);
            let _ = device.poll(wgpu::PollType::Poll);
        }
        let early = record(&early);
        let late = record(&late);
        let (sky, sky_error) =
            match sky.map(|t| SkyResources::new(&device, &queue, &pipelines.sky, t)) {
                Some(Ok(sky)) => {
                    for t in sky.textures() {
                        if !textures.iter().any(|k| k.texture == t.texture) {
                            texture_bytes += self::texture_bytes(&t.texture);
                            textures.push(t.clone());
                        }
                    }
                    (Some(sky), None)
                }
                Some(Err(e)) => (None, Some(e)),
                None => (None, None),
            };
        Ok(World(Rc::new(Inner {
            vertex_buffers,
            index_buffers,
            index_formats,
            draw_data,
            kept_of,
            _textures: textures,
            frame,
            frame_group,
            surface_groups,
            surface_pipelines,
            early,
            late,
            strict: RefCell::new(strict),
            lit,
            spheres: RefCell::new(kept.iter().map(|d| d.sphere).collect()),
            sky,
            sky_error,
            draws: kept.len() - object_draws,
            skipped,
            buffer_bytes,
            texture_bytes,
            object_draws,
            object_buffer_bytes,
            object_texture_bytes,
        })))
    }

    /// Places draw `draw` (its index among the draws `world` was created
    /// with) at `transform` with bounding sphere `sphere`; a radius of 0
    /// hides it (its screen size is then never above ssa_min, section
    /// 10.2). A draw left out of the world is ignored.
    pub fn place_draw(&self, world: &World, draw: usize, transform: &Mat4, sphere: [f32; 4]) {
        let inner = &world.0;
        let Some(Some(k)) = inner.kept_of.get(draw).copied() else {
            return;
        };
        let mut bytes = Vec::with_capacity(80);
        push_mat(&mut bytes, transform);
        push_vec4(&mut bytes, sphere);
        match &inner.draw_data {
            DrawData::Buffer(b) => self
                .queue
                .write_buffer(b, k as u64 * DRAW_BYTES as u64, &bytes),
            DrawData::Texture(t) => self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: t,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: k % DRAWS_PER_ROW * DRAW_TEXELS,
                        y: k / DRAWS_PER_ROW,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes.len() as u32),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: (bytes.len() / 16) as u32,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            ),
        }
        inner.spheres.borrow_mut()[k as usize] = sphere;
        for (i, d) in inner.strict.borrow_mut().iter_mut() {
            if *i == k as usize {
                d.transform = *transform;
                d.sphere = sphere;
            }
        }
    }

    /// The depth buffer for a `width` × `height` target.
    pub(crate) fn depth_view(&mut self, width: u32, height: u32) -> wgpu::TextureView {
        let fits = self
            .depth
            .as_ref()
            .is_some_and(|d| d.width() == width && d.height() == height);
        if !fits {
            self.depth = Some(self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            }));
        }
        self.depth
            .as_ref()
            .expect("depth created")
            .create_view(&Default::default())
    }

    /// Records the world pass of `frame` into `encoder`: cleared to the
    /// frame's clear colour, then the static world in the order of section
    /// 10.3.
    /// `particles` are the frame's prepared effects; `keep_depth` stores
    /// the depth for the distortion pass.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode_world(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        frame: &WorldFrame,
        particles: Option<(
            &crate::particles::ParticlePipelines,
            &crate::particles::PreparedFrame,
        )>,
        keep_depth: bool,
    ) {
        let inner = &frame.world.0;
        let view_projection = multiply(&frame.view, &frame.projection);
        let l = &frame.light;
        let tau = std::f32::consts::TAU;
        let t = frame.time;
        let wind = {
            let (x, z) = ((0.6283 * t).sin(), (0.6283 * t).cos());
            let len = x.hypot(z).max(1e-9);
            [0.005 * x / len, 0.0, 0.005 * z / len, 0.0]
        };
        let mut constants = Vec::with_capacity(FRAME_BYTES as usize);
        push_mat(&mut constants, &view_projection);
        push_mat(&mut constants, &frame.view);
        push_vec4(
            &mut constants,
            vec4(frame.eye, {
                let (w, h) = supersampled(size.0, size.1);
                ssa_min(w, h, frame.fov_y_degrees)
            }),
        );
        push_vec4(&mut constants, vec4(frame.forward, 0.0));
        push_vec4(&mut constants, vec4(l.ambient, 0.0));
        push_vec4(&mut constants, vec4(l.hemi, 0.0));
        push_vec4(&mut constants, vec4(l.sun_color, 0.0));
        push_vec4(&mut constants, vec4(l.sun_dir, 0.0));
        push_vec4(
            &mut constants,
            fog_plane(&view_projection, l.fog_near, l.fog_far),
        );
        push_vec4(&mut constants, vec4(l.fog_color, 0.0));
        push_vec4(&mut constants, [0.1 / tau, 0.01 / tau, 0.11 / tau, t / tau]);
        push_vec4(&mut constants, wind);
        self.queue.write_buffer(&inner.frame, 0, &constants);
        let sky = inner.sky.as_ref().zip(frame.sky.as_ref());
        if let Some((resources, sky_frame)) = sky {
            resources.write(&self.queue, sky_frame, frame.eye, &view_projection, t);
        }

        // Step 6 order: decreasing squared distance (back to front), the
        // strict-sorted effects among them by their keys (particle-drawing.md
        // 7.3; on equal keys the static draws first, our choice: the queue
        // order of the two is not stated).
        let strict = inner.strict.borrow().clone();
        let distance = |d: &WorldDraw| {
            (0..3)
                .map(|i| (d.sphere[i] - frame.eye[i]).powi(2))
                .sum::<f32>()
        };
        enum Sorted<'a> {
            Static(usize, WorldDraw),
            Effect(&'a crate::particles::Prepared),
        }
        let mut sorted: Vec<(f32, Sorted)> = strict
            .into_iter()
            .map(|(i, d)| (distance(&d), Sorted::Static(i, d)))
            .collect();
        if let Some((_, prepared)) = particles {
            let queue = crate::particles::ParticleQueue::Strict;
            sorted.extend(
                (prepared.draws.iter())
                    .filter(|d| d.queue == queue)
                    .map(|d| (d.key, Sorted::Effect(d))),
            );
        }
        sorted.sort_by(|a, b| b.0.total_cmp(&a.0));

        let depth = self.depth_view(size.0, size.1);
        let lights = self.prepare_lights(frame, &view_projection);
        let light_pipelines = self.world_pipelines.as_ref().filter(|_| !lights.is_empty());
        let [r, g, b] = frame.clear.map(f64::from);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: if keep_depth {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // Section 10.3: step 1, the sky (step 3, sky.md section 10), step
        // 5, then step 6; the effects within steps 1, 5 and 6
        // (particle-drawing.md 7.4).
        use crate::particles::ParticleQueue;
        pass.execute_bundles([&inner.early]);
        if let Some((pipelines, prepared)) = particles {
            prepared.draw_queue(pipelines, &mut pass, ParticleQueue::Early);
        }
        if let Some((resources, sky_frame)) = sky {
            resources.draw(&mut pass, sky_frame, size);
        }
        // The light passes follow the sky (dynamic-lights.md 5.1).
        if let Some(p) = light_pipelines
            && let Some(groups) = &p.light_groups
        {
            pass.set_bind_group(0, &inner.frame_group, &[]);
            for (offset, spot, draws) in &lights {
                pass.set_bind_group(2, &groups[usize::from(*spot)], &[*offset]);
                let mut program = None;
                let mut surface = None;
                let mut buffers = None;
                for &j in draws {
                    let d = &inner.lit[j as usize];
                    if program != Some(d.program) {
                        program = Some(d.program);
                        pass.set_pipeline(&p.light_pipelines[&d.program]);
                    }
                    if surface != Some(d.surface) {
                        surface = Some(d.surface);
                        pass.set_bind_group(1, &inner.surface_groups[d.surface as usize], &[]);
                    }
                    if buffers != Some((d.vertex_buffer, d.index_buffer)) {
                        buffers = Some((d.vertex_buffer, d.index_buffer));
                        pass.set_vertex_buffer(
                            0,
                            inner.vertex_buffers[d.vertex_buffer as usize].slice(..),
                        );
                        pass.set_index_buffer(
                            inner.index_buffers[d.index_buffer as usize].slice(..),
                            inner.index_formats[d.index_buffer as usize],
                        );
                    }
                    pass.draw_indexed(
                        d.first_index..d.first_index + d.index_count,
                        d.first_vertex as i32,
                        d.kept..d.kept + 1,
                    );
                }
            }
        }
        pass.execute_bundles([&inner.late]);
        if let Some((pipelines, prepared)) = particles {
            prepared.draw_queue(pipelines, &mut pass, ParticleQueue::Late);
        }
        for (_, item) in sorted {
            let (i, d) = match item {
                Sorted::Static(i, d) => (i, d),
                Sorted::Effect(e) => {
                    if let Some((pipelines, prepared)) = particles {
                        prepared.draw(pipelines, &mut pass, e);
                    }
                    continue;
                }
            };
            pass.set_bind_group(0, &inner.frame_group, &[]);
            pass.set_pipeline(&inner.surface_pipelines[d.surface as usize]);
            pass.set_bind_group(1, &inner.surface_groups[d.surface as usize], &[]);
            pass.set_vertex_buffer(0, inner.vertex_buffers[d.vertex_buffer as usize].slice(..));
            pass.set_index_buffer(
                inner.index_buffers[d.index_buffer as usize].slice(..),
                inner.index_formats[d.index_buffer as usize],
            );
            let instance = i as u32;
            pass.draw_indexed(
                d.first_index..d.first_index + d.index_count,
                d.first_vertex as i32,
                instance..instance + 1,
            );
        }
        // Step 7: the glows (glows.md section 4).
        if let Some((pipelines, prepared)) = particles {
            prepared.draw_queue(pipelines, &mut pass, ParticleQueue::Glow);
        }
    }

    /// The world's pipelines for the current mode, with the light
    /// textures given so far.
    pub(crate) fn make_world_pipelines(&mut self) {
        let mut p = WorldPipelines::new(&self.device, &self.queue, self.format, self.compat);
        if let Some(t) = self.light_textures.clone() {
            p.light_groups(&self.device, t);
        }
        self.world_pipelines = Some(p);
    }

    /// Gives the light passes their attenuation textures
    /// (dynamic-lights.md 6.4); without them no light pass is drawn.
    pub fn set_light_textures(&mut self, textures: LightTextures) {
        self.light_textures = Some(textures.clone());
        if let Some(p) = &mut self.world_pipelines {
            p.light_groups(&self.device, textures);
        }
    }

    /// Writes the constants of `frame`'s light passes and returns each
    /// pass with draws: its constants' offset, whether it is a spot pass,
    /// and the lit draws (indices into the world's) that its frustum and
    /// the camera's view-projection `camera` meet: a superset of those the
    /// camera's pass submitted (section 5.5, rule 1, as level.md 9.2
    /// allows); the projected-size rule 3 is the shaders' as in the
    /// camera's pass.
    // TODO(#347): the first-person models' part of each pass (section 5.5)
    // waits for hud-scope.md item 26.
    fn prepare_lights(&mut self, frame: &WorldFrame, camera: &Mat4) -> Vec<(u32, bool, Vec<u32>)> {
        let Some(p) = self.world_pipelines.as_mut() else {
            return Vec::new();
        };
        if frame.lights.is_empty() || p.light_groups.is_none() {
            return Vec::new();
        }
        p.light_room(&self.device, frame.lights.len() as u64);
        let inner = &frame.world.0;
        let spheres = inner.spheres.borrow();
        let camera = frustum_planes(camera);
        let mut constants = vec![0u8; frame.lights.len() * LIGHT_SLOT as usize];
        let mut out = Vec::new();
        for (i, l) in frame.lights.iter().enumerate() {
            let planes = frustum_planes(&l.frustum);
            let draws: Vec<u32> = (inner.lit.iter().enumerate())
                .filter(|(_, d)| {
                    let s = spheres[d.kept as usize];
                    s[3] > 0.0 && meets(&planes, s) && meets(&camera, s)
                })
                .map(|(j, _)| j as u32)
                .collect();
            if draws.is_empty() {
                continue;
            }
            let at = i * LIGHT_SLOT as usize;
            let mut bytes = Vec::with_capacity(112);
            push_vec4(&mut bytes, l.position);
            push_vec4(&mut bytes, l.colour);
            push_mat(&mut bytes, &l.xform);
            push_vec4(&mut bytes, [f32::from(u8::from(l.spot)), 0.0, 0.0, 0.0]);
            constants[at..at + bytes.len()].copy_from_slice(&bytes);
            out.push((at as u32, l.spot, draws));
        }
        if !out.is_empty() {
            self.queue.write_buffer(&p.light_constants, 0, &constants);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!((ssa_min(1024, 768, 90.0) - expected).abs() < 1e-5 * expected);
    }
}
