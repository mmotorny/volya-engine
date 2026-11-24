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

use std::collections::HashMap;
use std::rc::Rc;

use volya_engine_formats::level_geom::{Layout, LevelGeom};
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
}

impl Program {
    /// The vertex layout the program reads.
    pub fn layout(self) -> Layout {
        match self {
            Self::LightMapped | Self::Terrain => Layout::LightMapped,
            Self::Tree => Layout::Tree,
            _ => Layout::VertexLit,
        }
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
        }
    }
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
/// vertex buffer `vertex_buffer`, with surface `surface`.
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
}

/// Bytes of one draw's record in the draws buffer.
const DRAW_BYTES: usize = 144;
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
    sky: SkyPipelines,
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

fn vertex_layout(layout: Layout) -> wgpu::VertexBufferLayout<'static> {
    const LIGHT_MAPPED: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Unorm8x4, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Sint16x2, 5 => Sint16x2
    ];
    const VERTEX_LIT: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Unorm8x4, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Unorm8x4, 5 => Sint16x2
    ];
    const TREE: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Unorm8x4, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Sint16x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: 32,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: match layout {
            Layout::LightMapped => &LIGHT_MAPPED,
            Layout::Tree => &TREE,
            _ => &VERTEX_LIT,
        },
    }
}

impl WorldPipelines {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
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
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world"),
            bind_group_layouts: &[Some(&frame_layout), Some(&surface_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world"),
            source: wgpu::ShaderSource::Wgsl(include_str!("world.wgsl").into()),
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
        Self {
            format,
            frame_layout,
            surface_layout,
            pipeline_layout,
            shader,
            pipelines: HashMap::new(),
            samplers,
            white: white.create_view(&Default::default()),
            sky: SkyPipelines::new(device, format),
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
    _draw_buffer: wgpu::Buffer,
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
    strict: Vec<(usize, WorldDraw)>,
    sky: Option<SkyResources>,
    sky_error: Option<String>,
    draws: usize,
    skipped: usize,
    buffer_bytes: u64,
    texture_bytes: u64,
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

    pub fn draw_count(&self) -> usize {
        self.0.draws
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

    pub fn same(&self, other: &World) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn draw_record(surface: &Surface<Texture>, d: &WorldDraw, out: &mut Vec<u8>) {
    push_mat(out, &d.transform);
    push_vec4(out, d.sphere);
    let (scale, bias, sun) = match &d.tree {
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
            0.0,
            0.0,
        ],
    );
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
        for d in draws {
            if d.vertex_buffer as usize >= geom.vertex_buffers.len()
                || d.index_buffer as usize >= geom.index_buffers.len()
            {
                return Err(format!("draw {d:?} names a missing buffer"));
            }
        }
        let device = self.device.clone();
        let format = self.format;
        let pipelines = self
            .world_pipelines
            .get_or_insert_with(|| WorldPipelines::new(&device, &self.queue, format));
        let mut buffer_bytes = 0;
        let vertex_buffers: Vec<wgpu::Buffer> = geom
            .vertex_buffers
            .iter()
            .map(|vb| {
                buffer_bytes += vb.data.len() as u64;
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("level.geom vertices"),
                    contents: &vb.data,
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
            .collect();
        let index_buffers: Vec<wgpu::Buffer> = geom
            .index_buffers
            .iter()
            .map(|ib| {
                let mut data: Vec<u8> = ib.iter().flat_map(|i| i.to_le_bytes()).collect();
                // Buffer sizes are multiples of 4.
                data.resize(data.len().next_multiple_of(4).max(4), 0);
                buffer_bytes += data.len() as u64;
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("level.geom indices"),
                    contents: &data,
                    usage: wgpu::BufferUsages::INDEX,
                })
            })
            .collect();

        // Draws that can be drawn: surface present, layouts agree.
        let usable = |d: &WorldDraw| {
            surfaces.get(d.surface as usize).is_some_and(|s| {
                geom.vertex_buffers[d.vertex_buffer as usize].layout == Some(s.program.layout())
            })
        };
        let kept: Vec<WorldDraw> = draws.iter().copied().filter(usable).collect();
        let skipped = draws.len() - kept.len();

        let mut records = Vec::with_capacity(kept.len().max(1) * DRAW_BYTES);
        for d in &kept {
            draw_record(&surfaces[d.surface as usize], d, &mut records);
        }
        records.resize(records.len().max(DRAW_BYTES), 0);
        buffer_bytes += records.len() as u64;
        let draw_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world draws"),
            contents: &records,
            usage: wgpu::BufferUsages::STORAGE,
        });
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
                    resource: draw_buffer.as_entire_binding(),
                },
            ],
        });

        // Textures once each (by handle), for their views and memory.
        let mut textures: Vec<Texture> = Vec::new();
        let mut texture_bytes = 0;
        let surface_groups: Vec<wgpu::BindGroup> = surfaces
            .iter()
            .map(|s| {
                let views: Vec<wgpu::TextureView> = s
                    .textures
                    .iter()
                    .map(|t| match t {
                        Some(t) => {
                            if !textures.iter().any(|k| k.texture == t.texture) {
                                texture_bytes += self::texture_bytes(&t.texture);
                                textures.push(t.clone());
                            }
                            t.texture.create_view(&wgpu::TextureViewDescriptor {
                                dimension: Some(wgpu::TextureViewDimension::D2),
                                array_layer_count: Some(1),
                                ..Default::default()
                            })
                        }
                        None => pipelines.white.clone(),
                    })
                    .collect();
                let mut entries: Vec<wgpu::BindGroupEntry> = views
                    .iter()
                    .enumerate()
                    .map(|(i, v)| wgpu::BindGroupEntry {
                        binding: i as u32,
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
                        wgpu::IndexFormat::Uint16,
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
        let early = record(&early);
        let late = record(&late);
        let (sky, sky_error) = match sky.map(|t| SkyResources::new(&device, &pipelines.sky, t)) {
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
            _draw_buffer: draw_buffer,
            _textures: textures,
            frame,
            frame_group,
            surface_groups,
            surface_pipelines,
            early,
            late,
            strict,
            sky,
            sky_error,
            draws: kept.len(),
            skipped,
            buffer_bytes,
            texture_bytes,
        })))
    }

    /// The depth buffer for a `width` × `height` target.
    fn depth_view(&mut self, width: u32, height: u32) -> wgpu::TextureView {
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
    pub(crate) fn encode_world(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        frame: &WorldFrame,
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

        // Step 6 order: decreasing squared distance (back to front).
        let mut strict = inner.strict.clone();
        let distance = |d: &WorldDraw| {
            (0..3)
                .map(|i| (d.sphere[i] - frame.eye[i]).powi(2))
                .sum::<f32>()
        };
        strict.sort_by(|a, b| distance(&b.1).total_cmp(&distance(&a.1)));

        let depth = self.depth_view(size.0, size.1);
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
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // Section 10.3: step 1, the sky (step 3, sky.md section 10), step
        // 5, then step 6.
        pass.execute_bundles([&inner.early]);
        if let Some((resources, sky_frame)) = sky {
            resources.draw(&mut pass, sky_frame, size);
        }
        pass.execute_bundles([&inner.late]);
        pass.set_bind_group(0, &inner.frame_group, &[]);
        for (i, d) in strict {
            pass.set_pipeline(&inner.surface_pipelines[d.surface as usize]);
            pass.set_bind_group(1, &inner.surface_groups[d.surface as usize], &[]);
            pass.set_vertex_buffer(0, inner.vertex_buffers[d.vertex_buffer as usize].slice(..));
            pass.set_index_buffer(
                inner.index_buffers[d.index_buffer as usize].slice(..),
                wgpu::IndexFormat::Uint16,
            );
            let instance = i as u32;
            pass.draw_indexed(
                d.first_index..d.first_index + d.index_count,
                d.first_vertex as i32,
                instance..instance + 1,
            );
        }
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
