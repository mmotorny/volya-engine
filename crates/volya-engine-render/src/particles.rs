// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Particle effects in the world pass (docs/behavior/particle-drawing.md
//! sections 4.4 – 7): each effect one indexed batch of quads with its pass,
//! queued where section 7.4 puts it, and the distortion pass (section
//! 6.2; the composition is the presentation's `postprocess_d`, 6.3). Vertex and index data are written anew each
//! frame into buffers kept and grown; indices are absolute (no base vertex), and nothing reads a
//! storage buffer, so the WebGL 2 path draws them alike.

use std::collections::HashMap;

use wgpu::util::DeviceExt;

use crate::Texture;
use crate::world::{DEPTH_FORMAT, Mat4, alpha_blending, fog_plane, multiply, push_mat};

/// A pass's blending (section 5.2; source / destination factors).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParticleBlend {
    /// `SET`: off.
    Off,
    /// `BLEND`: source alpha / inverse source alpha.
    Alpha,
    /// `ADD`: one / one.
    Add,
    /// `MUL`: destination colour / zero.
    Mul,
    /// `MUL_2X`: destination colour / source colour.
    Mul2x,
    /// `ALPHA-ADD`: source alpha / one.
    AlphaAdd,
}

/// The pixel program and its fixed states (sections 5.2 – 5.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParticleProgram {
    /// `particle`, alpha test A · 255 > 200, fog on (`SET`).
    Set,
    /// `particle`, alpha test A · 255 > 0, no fog.
    Blended,
    /// `particle`, no alpha test, no fog (the scripted shaders).
    Plain,
    /// `particle_distort`, no alpha test, no fog.
    Distort,
    /// A glow's `S_SET   ` pass of `effects\glow` (glows.md 6,
    /// static-shading.md 7.5): colour = texel × vertex colour, alpha test
    /// A · 255 > 0, no fog, depth test on, texture clamped with linear
    /// filtering.
    Glow,
    /// `effects\flare`: alpha test A · 255 > 2, depth test off.
    Flare,
}

/// Face culling during the effect's draw (section 4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParticleCull {
    None,
    /// Triangles appearing clockwise on screen are culled.
    Clockwise,
    /// Counter-clockwise ones.
    CounterClockwise,
}

/// Where an effect is drawn in the frame (section 7.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleQueue {
    /// With the non-sorted passes of priorities 0 – 1 (step 1).
    Early,
    /// With the non-sorted passes of priorities 2 – 3 (step 4).
    Late,
    /// In the strict-sorted list, back to front (step 5).
    Strict,
    /// In the distortion list, back to front (step 7).
    Distortion,
    /// The glows, after the strict-sorted passes (glows.md section 4).
    Glow,
}

/// One pass of an effect's shader.
#[derive(Clone, Debug)]
pub struct ParticlePass {
    pub program: ParticleProgram,
    pub blend: ParticleBlend,
    pub depth_write: bool,
    /// `s_base` clamps (`Texture clamp`); otherwise it wraps.
    pub clamp: bool,
    /// `s_base`.
    pub base: Option<Texture>,
    /// `s_distort`; `None` samples (0, 0, 0, 1) (believed, section 5.3;
    /// TODO(#329)).
    pub distort: Option<Texture>,
}

/// A quad corner: world position, colour (A, R, G, B from the top byte),
/// texture coordinates (section 4.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleVertex {
    pub position: [f32; 3],
    pub colour: u32,
    pub uv: [f32; 2],
}

/// An effect's sprites with one of its passes.
#[derive(Clone, Debug)]
pub struct ParticleBatch {
    pub pass: ParticlePass,
    pub queue: ParticleQueue,
    pub cull: ParticleCull,
    /// |**E** − **C**′|² + 10⁻⁵ (section 7.2).
    pub key: f32,
    /// Four per particle, in the particle list's order.
    pub vertices: Vec<ParticleVertex>,
}

/// Bytes of one vertex: position, colour, uv (24, section 4.4).
const VERTEX_BYTES: u64 = 24;

/// The vertex and index buffers' first size: 10,922 particles' vertices,
/// more than `l01_escape`'s effect objects can hold at once (8,122). Made
/// with the renderer: buffers made with a level's world stay allocated amid
/// the level freed after it and split the wasm heap.
const INITIAL_BYTES: u64 = 1024 * 1024;

/// The passes of the effects' shaders (program, blending, depth write):
/// class `PARTICLE`'s six blendings (section 5.2) and the scripted
/// shaders' normal and distortion passes (5.3).
const PASSES: &[(ParticleProgram, ParticleBlend, bool)] = &[
    (ParticleProgram::Set, ParticleBlend::Off, true),
    (ParticleProgram::Blended, ParticleBlend::Alpha, false),
    (ParticleProgram::Blended, ParticleBlend::Add, false),
    (ParticleProgram::Blended, ParticleBlend::Mul, false),
    (ParticleProgram::Blended, ParticleBlend::Mul2x, false),
    (ParticleProgram::Blended, ParticleBlend::AlphaAdd, false),
    (ParticleProgram::Plain, ParticleBlend::Add, false),
    (ParticleProgram::Distort, ParticleBlend::Alpha, false),
    (ParticleProgram::Glow, ParticleBlend::AlphaAdd, false),
    (ParticleProgram::Flare, ParticleBlend::AlphaAdd, false),
];

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    program: ParticleProgram,
    blend: ParticleBlend,
    depth_write: bool,
    cull: ParticleCull,
}

/// Pipelines and fixed resources for drawing particles into one format.
pub(crate) struct ParticlePipelines {
    format: wgpu::TextureFormat,
    texture_layout: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    /// `s_base` clamped and wrapped (anisotropic, linear mip), `s_distort`
    /// (wrap, linear).
    clamp: wgpu::Sampler,
    wrap: wgpu::Sampler,
    linear: wgpu::Sampler,
    /// The glows' texture: clamped, linear minification, magnification
    /// and mip filtering.
    glow: wgpu::Sampler,
    /// (0, 0, 0, 1): `s_distort` without a texture.
    black: wgpu::TextureView,
    frame: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    /// The frames' vertex and index buffers, kept and grown.
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
}

/// A batch ready to draw.
pub(crate) struct Prepared {
    pub queue: ParticleQueue,
    pub key: f32,
    key_pipeline: PipelineKey,
    group: wgpu::BindGroup,
    indices: std::ops::Range<u32>,
}

/// The frame's particle draws, with their buffers.
pub(crate) struct PreparedFrame {
    pub draws: Vec<Prepared>,
    /// Some sprite of the distortion list may reach the screen: none can
    /// when every sprite lies wholly outside one plane of the view volume.
    pub distortion_shown: bool,
    frame_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
}

impl PreparedFrame {
    pub fn has(&self, queue: ParticleQueue) -> bool {
        self.draws.iter().any(|d| d.queue == queue)
    }

    /// Draws `draw` into `pass`.
    pub fn draw(
        &self,
        pipelines: &ParticlePipelines,
        pass: &mut wgpu::RenderPass,
        draw: &Prepared,
    ) {
        let Some(pipeline) = pipelines.pipelines.get(&draw.key_pipeline) else {
            return;
        };
        crate::stats::pipeline();
        pass.set_pipeline(pipeline);
        crate::stats::bind_group();
        pass.set_bind_group(0, &self.frame_group, &[]);
        crate::stats::bind_group();
        pass.set_bind_group(1, &draw.group, &[]);
        crate::stats::buffer();
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        crate::stats::buffer();
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        crate::stats::draw();
        pass.draw_indexed(draw.indices.clone(), 0, 0..1);
    }

    /// Draws every batch of `queue` in queue order.
    pub fn draw_queue(
        &self,
        pipelines: &ParticlePipelines,
        pass: &mut wgpu::RenderPass,
        queue: ParticleQueue,
    ) {
        for d in self.draws.iter().filter(|d| d.queue == queue) {
            self.draw(pipelines, pass, d);
        }
    }
}

fn blend_state(blend: ParticleBlend) -> Option<wgpu::BlendState> {
    use wgpu::BlendFactor as F;
    let state = |src, dst| {
        let c = wgpu::BlendComponent {
            src_factor: src,
            dst_factor: dst,
            operation: wgpu::BlendOperation::Add,
        };
        wgpu::BlendState { color: c, alpha: c }
    };
    match blend {
        ParticleBlend::Off => None,
        ParticleBlend::Alpha => Some(alpha_blending()),
        ParticleBlend::Add => Some(state(F::One, F::One)),
        ParticleBlend::Mul => Some(state(F::Dst, F::Zero)),
        ParticleBlend::Mul2x => Some(state(F::Dst, F::Src)),
        ParticleBlend::AlphaAdd => Some(state(F::SrcAlpha, F::One)),
    }
}

impl ParticlePipelines {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle frame"),
            entries: &[uniform(
                0,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            )],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle textures"),
            entries: &[texture(0), sampler(1), texture(2), sampler(3)],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[Some(&frame_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle"),
            source: wgpu::ShaderSource::Wgsl(include_str!("particle.wgsl").into()),
        });
        // Section 5.2: anisotropic minification and magnification, linear
        // mip (static-shading.md 4.4's anisotropy 16, mip bias 0); 5.3:
        // `s_distort` linear.
        let linear = wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        };
        let wrap = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("particle s_base"),
            anisotropy_clamp: 16,
            ..linear.clone()
        });
        let clamp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("particle s_base clamped"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            anisotropy_clamp: 16,
            ..linear.clone()
        });
        let glow = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glow"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..linear.clone()
        });
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("particle s_distort"),
            ..linear
        });
        let black = device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("particle no texture"),
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
                wgpu::util::TextureDataOrder::LayerMajor,
                &[0, 0, 0, 255],
            )
            .create_view(&Default::default());
        let frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle frame"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            }],
        });
        let mut pipelines = Self {
            format,
            texture_layout,
            layout,
            shader,
            pipelines: HashMap::with_capacity(PASSES.len() * 3),
            clamp,
            wrap,
            linear,
            glow,
            black,
            frame,
            frame_group,
            vertices: None,
            indices: None,
        };
        // Every pass the effects' shaders resolve to, with each culling,
        // and buffers for the first level's effects: nothing of this is
        // allocated while a level's data fills the heap.
        for &(program, blend, depth_write) in PASSES {
            for cull in [
                ParticleCull::None,
                ParticleCull::Clockwise,
                ParticleCull::CounterClockwise,
            ] {
                pipelines.pipeline(
                    device,
                    PipelineKey {
                        program,
                        blend,
                        depth_write,
                        cull,
                    },
                );
            }
        }
        let usage = wgpu::BufferUsages::VERTEX;
        grown(
            device,
            &mut pipelines.vertices,
            INITIAL_BYTES,
            usage,
            "particle vertices",
        );
        let usage = wgpu::BufferUsages::INDEX;
        grown(
            device,
            &mut pipelines.indices,
            INITIAL_BYTES,
            usage,
            "particle indices",
        );
        pipelines
    }

    /// Grows the vertex and index buffers to hold `particles` sprites.
    pub fn reserve(&mut self, device: &wgpu::Device, particles: usize) {
        let quads = particles as u64;
        grown(
            device,
            &mut self.vertices,
            quads * 4 * VERTEX_BYTES,
            wgpu::BufferUsages::VERTEX,
            "particle vertices",
        );
        grown(
            device,
            &mut self.indices,
            quads * 6 * 4,
            wgpu::BufferUsages::INDEX,
            "particle indices",
        );
    }

    /// The vertex and index buffers' sizes in bytes.
    pub fn buffer_sizes(&self) -> (u64, u64) {
        let size = |b: &Option<wgpu::Buffer>| b.as_ref().map_or(0, wgpu::Buffer::size);
        (size(&self.vertices), size(&self.indices))
    }

    fn pipeline(&mut self, device: &wgpu::Device, key: PipelineKey) {
        if self.pipelines.contains_key(&key) {
            return;
        }
        let entry = match key.program {
            ParticleProgram::Set => "fs_set",
            ParticleProgram::Blended => "fs_blended",
            ParticleProgram::Plain => "fs_plain",
            ParticleProgram::Distort => "fs_distort",
            ParticleProgram::Glow => "fs_glow",
            ParticleProgram::Flare => "fs_flare",
        };
        // static-shading.md 10.1's convention, as the world's: a face is
        // front when it appears clockwise on screen.
        let (front_face, cull_mode) = match key.cull {
            ParticleCull::None => (wgpu::FrontFace::Cw, None),
            ParticleCull::Clockwise => (wgpu::FrontFace::Ccw, Some(wgpu::Face::Back)),
            ParticleCull::CounterClockwise => (wgpu::FrontFace::Cw, Some(wgpu::Face::Back)),
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry),
            layout: Some(&self.layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_particle"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_BYTES,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, 1 => Unorm8x4, 2 => Float32x2
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                front_face,
                cull_mode,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(key.depth_write),
                depth_compare: Some(if key.program == ParticleProgram::Flare {
                    wgpu::CompareFunction::Always
                } else {
                    wgpu::CompareFunction::LessEqual
                }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // The distortion program draws only into the
                    // distortion target.
                    format: if key.program == ParticleProgram::Distort {
                        crate::present::DISTORT_FORMAT
                    } else {
                        self.format
                    },
                    blend: blend_state(key.blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key, pipeline);
    }

    /// Writes the frame's vertices, indices and constants and makes each
    /// batch's bind group.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batches: &[ParticleBatch],
        view: &Mat4,
        projection: &Mat4,
        fog: ([f32; 3], f32, f32),
    ) -> Option<PreparedFrame> {
        let batches: Vec<&ParticleBatch> = batches
            .iter()
            .filter(|b| {
                // A normal pass without its texture is left out (reported
                // where it failed to load).
                let textured = b.pass.base.is_some() || b.pass.program == ParticleProgram::Distort;
                !b.vertices.is_empty() && textured
            })
            .collect();
        if batches.is_empty() {
            return None;
        }
        let view_projection = multiply(view, projection);
        let mut constants = Vec::with_capacity(96);
        push_mat(&mut constants, &view_projection);
        for v in fog_plane(&view_projection, fog.1, fog.2) {
            constants.extend_from_slice(&v.to_le_bytes());
        }
        for v in [fog.0[0], fog.0[1], fog.0[2], 0.0] {
            constants.extend_from_slice(&v.to_le_bytes());
        }
        queue.write_buffer(&self.frame, 0, &constants);
        let mut vertex_bytes = Vec::new();
        let mut index_data: Vec<u32> = Vec::new();
        let mut draws = Vec::with_capacity(batches.len());
        let distortion_shown = batches.iter().any(|b| {
            b.queue == ParticleQueue::Distortion
                && b.vertices
                    .chunks(4)
                    .any(|q| !outside_view(&view_projection, q))
        });
        for b in batches {
            let first = (vertex_bytes.len() as u64 / VERTEX_BYTES) as u32;
            for v in &b.vertices {
                for c in v.position {
                    vertex_bytes.extend_from_slice(&c.to_le_bytes());
                }
                // A, R, G, B from the top byte to (R, G, B, A) bytes.
                let [a, r, g, bl] = v.colour.to_be_bytes();
                vertex_bytes.extend_from_slice(&[r, g, bl, a]);
                for c in v.uv {
                    vertex_bytes.extend_from_slice(&c.to_le_bytes());
                }
            }
            let start = index_data.len() as u32;
            for q in 0..(b.vertices.len() / 4) as u32 {
                let o = first + 4 * q;
                // Triangles (0, 1, 2) and (3, 2, 1).
                index_data.extend([o, o + 1, o + 2, o + 3, o + 2, o + 1]);
            }
            let key = PipelineKey {
                program: b.pass.program,
                blend: b.pass.blend,
                depth_write: b.pass.depth_write,
                cull: b.cull,
            };
            self.pipeline(device, key);
            let base = b
                .pass
                .base
                .as_ref()
                .map(|t| t.wgpu().create_view(&Default::default()));
            let distort =
                (b.pass.distort.as_ref()).map(|t| t.wgpu().create_view(&Default::default()));
            let base_view = base.as_ref().unwrap_or(&self.black);
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("particle textures"),
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(base_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(
                            if matches!(
                                b.pass.program,
                                ParticleProgram::Glow | ParticleProgram::Flare
                            ) {
                                &self.glow
                            } else if b.pass.clamp {
                                &self.clamp
                            } else {
                                &self.wrap
                            },
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            distort.as_ref().unwrap_or(&self.black),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.linear),
                    },
                ],
            });
            draws.push(Prepared {
                queue: b.queue,
                key: b.key,
                key_pipeline: key,
                group,
                indices: start..index_data.len() as u32,
            });
        }
        // Kept from frame to frame, grown as needed, and written with
        // `Queue::write_buffer`, which copies nothing into the wasm heap on
        // the web (unlike a buffer mapped at creation).
        let index_bytes: Vec<u8> = index_data.iter().flat_map(|i| i.to_le_bytes()).collect();
        let vertices = grown(
            device,
            &mut self.vertices,
            vertex_bytes.len() as u64,
            wgpu::BufferUsages::VERTEX,
            "particle vertices",
        );
        let indices = grown(
            device,
            &mut self.indices,
            index_bytes.len() as u64,
            wgpu::BufferUsages::INDEX,
            "particle indices",
        );
        queue.write_buffer(&vertices, 0, &vertex_bytes);
        queue.write_buffer(&indices, 0, &index_bytes);
        Some(PreparedFrame {
            draws,
            distortion_shown,
            frame_group: self.frame_group.clone(),
            vertices,
            indices,
        })
    }

    /// The distortion pass (section 6.2): the distortion list drawn into
    /// the cleared `distortion` target, back to front, against the
    /// scene's `depth`.
    pub fn distort(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &PreparedFrame,
        distortion: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        let grey = 127.0 / 255.0;
        let timed = crate::timing::pass_writes("distortion", false);
        crate::stats::pass();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("distortion"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: distortion,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: grey,
                        g: grey,
                        b: grey,
                        a: grey,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: crate::timing::writes(&timed),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        for d in sorted(frame, ParticleQueue::Distortion) {
            frame.draw(self, &mut pass, d);
        }
    }
}

/// Whether the sprite of `corners` lies wholly outside one plane of the
/// view volume of `view_projection` (row vectors; depth 0 … w), by a
/// margin, so it draws no pixel.
fn outside_view(view_projection: &Mat4, corners: &[ParticleVertex]) -> bool {
    let clip: Vec<[f32; 4]> = corners
        .iter()
        .map(|v| {
            let p = [v.position[0], v.position[1], v.position[2], 1.0];
            std::array::from_fn(|i| (0..4).map(|k| p[k] * view_projection[k][i]).sum())
        })
        .collect();
    let margin = |c: &[f32; 4]| 1e-3 * c[3].abs() + 1e-6;
    let all = |f: &dyn Fn(&[f32; 4]) -> bool| clip.iter().all(f);
    all(&|c| c[0] > c[3] + margin(c))
        || all(&|c| c[0] < -c[3] - margin(c))
        || all(&|c| c[1] > c[3] + margin(c))
        || all(&|c| c[1] < -c[3] - margin(c))
        || all(&|c| c[2] > c[3] + margin(c))
        || all(&|c| c[2] < -margin(c))
}

/// `buffer`, replaced by one of the next power of two (at least 1 KiB) when
/// it holds fewer than `bytes`.
fn grown(
    device: &wgpu::Device,
    buffer: &mut Option<wgpu::Buffer>,
    bytes: u64,
    usage: wgpu::BufferUsages,
    label: &str,
) -> wgpu::Buffer {
    if buffer.as_ref().is_none_or(|b| b.size() < bytes) {
        *buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (bytes.next_power_of_two()).max(1024),
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    buffer.clone().expect("made")
}

/// The draws of `queue` in decreasing key order (back to front), equal
/// keys in queue order (section 7.3).
pub(crate) fn sorted(frame: &PreparedFrame, queue: ParticleQueue) -> Vec<&Prepared> {
    let mut draws: Vec<&Prepared> = frame.draws.iter().filter(|d| d.queue == queue).collect();
    draws.sort_by(|a, b| b.key.total_cmp(&a.key));
    draws
}
