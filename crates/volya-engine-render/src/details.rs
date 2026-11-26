// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Detail objects in the world pass (docs/behavior/detail-objects.md
//! section 6): the frame's instances of each model drawn instanced, sway 1
//! of every model, then sway 2, then still, between the priority 0–1 draws
//! and the sky. Instances come in a vertex buffer stepped per instance,
//! bound at each draw's offset (no first instance), and the models' indices
//! are rebased at upload (no base vertex), as WebGL 2 needs.

use std::sync::Arc;

use crate::Texture;
use crate::world::{DEPTH_FORMAT, Mat4, push_mat, push_vec4};

/// Bytes of one vertex: position, then u, v and the height fraction.
const VERTEX_BYTES: u64 = 24;
/// Bytes of one instance: position and drawing scale, then cos θ, sin θ
/// and the light factors.
pub const INSTANCE_BYTES: u64 = 32;
/// Bytes of the frame constants.
const FRAME_BYTES: u64 = 192;

/// The lists, as [`DetailDraw::list`] numbers them.
pub const SWAY_1: u8 = 0;
pub const SWAY_2: u8 = 1;
pub const STILL: u8 = 2;

/// A model's geometry as the programs read it (6.3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailGeometry {
    /// Position, u, v, height fraction φ, each of the last three stored
    /// as floor(value · 16384) / 16384.
    pub vertices: Vec<[f32; 6]>,
    pub indices: Vec<u16>,
}

/// floor(value · 16384), clamped to −32,768 … 32,767, / 16384 (6.3).
fn quantize(value: f32) -> f32 {
    (value * 16384.0).floor().clamp(-32768.0, 32767.0) / 16384.0
}

impl DetailGeometry {
    /// From a model's vertices (position, u, v) and indices: φ = p.y / h,
    /// h the vertex bounds' height (0 for a model without height, whose
    /// instances are still anyway).
    pub fn new(vertices: &[([f32; 3], [f32; 2])], indices: &[u16]) -> Self {
        let lo = vertices
            .iter()
            .map(|v| v.0[1])
            .fold(f32::INFINITY, f32::min);
        let hi = vertices
            .iter()
            .map(|v| v.0[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let h = hi - lo;
        Self {
            vertices: vertices
                .iter()
                .map(|&([x, y, z], [u, v])| {
                    let phi = if h > 0.0 { quantize(y / h) } else { 0.0 };
                    [x, y, z, quantize(u), quantize(v), phi]
                })
                .collect(),
            indices: indices.to_vec(),
        }
    }
}

/// One draw: `count` instances of `model` from `first` in one list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetailDraw {
    pub model: u8,
    pub list: u8,
    pub first: u32,
    pub count: u32,
}

/// The detail objects of a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct DetailFrame {
    /// Per instance: x, y, z, σ′, cos θ, sin θ, sun factor, hemisphere
    /// factor.
    pub instances: Arc<Vec<[f32; 8]>>,
    /// In drawing order (6.1).
    pub draws: Vec<DetailDraw>,
    /// The five sway values in use: amp1, amp2, rot1, rot2, speed (6.5).
    pub sway: [f32; 5],
    /// The engine's global time in seconds (6.5).
    pub time: f64,
}

/// The sway constants of 6.5: (w, δ) of sway 1 and of sway 2.
pub fn wave_constants(sway: [f32; 5], t: f64) -> [[f32; 4]; 4] {
    use std::f64::consts::{PI, TAU};
    let [amp1, amp2, rot1, rot2, speed] = sway.map(f64::from);
    let w = (t * speed / TAU) as f32;
    let dir = |amp: f64, rot: f64| {
        let a = TAU * t / rot;
        [(amp * a.sin()) as f32, 0.0, (amp * a.cos()) as f32, 0.0]
    };
    let k = |d: f64| (1.0 / (d * PI)) as f32;
    [
        [k(10.0), k(14.0), k(6.0), w],
        dir(amp1, rot1),
        [k(6.0), k(14.0), k(10.0), w],
        dir(amp2, rot2),
    ]
}

/// A level's detail models on the GPU.
struct Level {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// Each model's index range.
    ranges: Vec<std::ops::Range<u32>>,
    texture: wgpu::BindGroup,
    instances: wgpu::Buffer,
    /// The alpha reference × 255, −1 for none.
    alpha_ref: f32,
}

/// The pipelines (made with the renderer) and the level's resources.
pub(crate) struct DetailPipelines {
    texture_layout: wgpu::BindGroupLayout,
    frame: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    /// Still, sway 1, sway 2.
    pipelines: [wgpu::RenderPipeline; 3],
    sampler: wgpu::Sampler,
    level: Option<Level>,
}

impl DetailPipelines {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("details frame"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(FRAME_BYTES),
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("details texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("details"),
            source: wgpu::ShaderSource::Wgsl(include_str!("details.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("details"),
            bind_group_layouts: &[Some(&frame_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: VERTEX_BYTES,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: INSTANCE_BYTES,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4],
            }),
        ];
        // 6.1, 6.2: both faces, depth test less-or-equal with writes, no
        // blending.
        let pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_detail"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("details frame"),
            size: FRAME_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("details frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            }],
        });
        // s_base: wrap, anisotropic, linear mip (6.2; the anisotropy of
        // static-shading.md 4.4 with its default).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("details s_base"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });
        Self {
            texture_layout,
            frame,
            frame_group,
            pipelines: [
                pipeline("vs_still"),
                pipeline("vs_wave1"),
                pipeline("vs_wave2"),
            ],
            sampler,
            level: None,
        }
    }

    /// The level's models, their texture and alpha test, with room for
    /// `capacity` instances a frame.
    pub(crate) fn set_level(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        models: &[DetailGeometry],
        texture: &Texture,
        alpha_test: bool,
        capacity: usize,
    ) {
        self.level = None;
        let mut vertices: Vec<u8> = Vec::new();
        let mut indices: Vec<u8> = Vec::new();
        let mut ranges = Vec::with_capacity(models.len());
        let mut first_vertex = 0u32;
        let mut first_index = 0u32;
        for m in models {
            for v in &m.vertices {
                vertices.extend(v.iter().flat_map(|f| f.to_le_bytes()));
            }
            for &i in &m.indices {
                let rebased = u16::try_from(u32::from(i) + first_vertex).unwrap_or(u16::MAX);
                indices.extend(rebased.to_le_bytes());
            }
            ranges.push(first_index..first_index + m.indices.len() as u32);
            first_vertex += m.vertices.len() as u32;
            first_index += m.indices.len() as u32;
        }
        // Index data padded to the copy alignment.
        while !indices.len().is_multiple_of(4) {
            indices.push(0);
        }
        let buffer = |label, data: &[u8], usage| {
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (data.len() as u64).max(4),
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&b, 0, data);
            b
        };
        let view = texture.wgpu().create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            array_layer_count: Some(1),
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("details texture"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.level = Some(Level {
            vertices: buffer("details vertices", &vertices, wgpu::BufferUsages::VERTEX),
            indices: buffer("details indices", &indices, wgpu::BufferUsages::INDEX),
            ranges,
            texture: group,
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("details instances"),
                size: (capacity.max(1) as u64) * INSTANCE_BYTES,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            alpha_ref: if alpha_test { 200.0 } else { -1.0 },
        });
    }

    pub(crate) fn clear_level(&mut self) {
        self.level = None;
    }

    /// Bytes of the level's buffers on the GPU.
    pub(crate) fn gpu_bytes(&self) -> u64 {
        self.level.as_ref().map_or(0, |l| {
            l.vertices.size() + l.indices.size() + l.instances.size()
        })
    }

    /// Writes the frame's constants and instances; whether there is
    /// anything to draw.
    pub(crate) fn prepare(
        &self,
        queue: &wgpu::Queue,
        frame: &DetailFrame,
        view_projection: &Mat4,
        light: [[f32; 3]; 3],
    ) -> bool {
        let Some(level) = &self.level else {
            return false;
        };
        if frame.draws.is_empty() {
            return false;
        }
        let mut constants = Vec::with_capacity(FRAME_BYTES as usize);
        push_mat(&mut constants, view_projection);
        for c in wave_constants(frame.sway, frame.time) {
            push_vec4(&mut constants, c);
        }
        for [r, g, b] in light {
            push_vec4(&mut constants, [r, g, b, 0.0]);
        }
        push_vec4(&mut constants, [level.alpha_ref, 0.0, 0.0, 0.0]);
        queue.write_buffer(&self.frame, 0, &constants);
        let room = (level.instances.size() / INSTANCE_BYTES) as usize;
        let n = frame.instances.len().min(room);
        queue.write_buffer(
            &level.instances,
            0,
            bytemuck::cast_slice(&frame.instances[..n]),
        );
        true
    }

    /// Draws the frame's lists (6.1) into `pass`.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &DetailFrame) {
        let Some(level) = &self.level else {
            return;
        };
        let room = (level.instances.size() / INSTANCE_BYTES) as u32;
        crate::stats::bind_group();
        pass.set_bind_group(0, &self.frame_group, &[]);
        crate::stats::bind_group();
        pass.set_bind_group(1, &level.texture, &[]);
        crate::stats::buffer();
        pass.set_vertex_buffer(0, level.vertices.slice(..));
        crate::stats::buffer();
        pass.set_index_buffer(level.indices.slice(..), wgpu::IndexFormat::Uint16);
        let mut current = None;
        for d in &frame.draws {
            let Some(range) = level.ranges.get(usize::from(d.model)) else {
                continue;
            };
            let end = (d.first + d.count).min(room);
            if end <= d.first {
                continue;
            }
            let pipeline = match d.list {
                SWAY_1 => 1,
                SWAY_2 => 2,
                _ => 0,
            };
            if current != Some(pipeline) {
                current = Some(pipeline);
                crate::stats::pipeline();
                pass.set_pipeline(&self.pipelines[pipeline]);
            }
            let bytes = u64::from(d.first) * INSTANCE_BYTES..u64::from(end) * INSTANCE_BYTES;
            crate::stats::buffer();
            pass.set_vertex_buffer(1, level.instances.slice(bytes));
            crate::stats::draw();
            pass.draw_indexed(range.clone(), 0, 0..end - d.first);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_coordinates_and_the_height_fraction() {
        // 6.3: u, v and φ in 1/16384 steps, φ = p.y / h.
        let g = DetailGeometry::new(
            &[
                ([0.0, 0.0, 0.0], [0.1, 1.0]),
                ([0.0, 0.6, 0.0], [0.99999, -0.01]),
            ],
            &[0, 1, 0],
        );
        assert_eq!(g.vertices[0][3..], [1638.0 / 16384.0, 1.0, 0.0]);
        assert_eq!(
            g.vertices[1][3..],
            [16383.0 / 16384.0, -164.0 / 16384.0, 1.0]
        );
        // A flat model's φ is 0.
        let flat = DetailGeometry::new(&[([0.0; 3], [0.0; 2]); 3], &[0, 1, 2]);
        assert!(flat.vertices.iter().all(|v| v[5] == 0.0));
    }

    #[test]
    #[allow(clippy::excessive_precision)] // As the spec writes them.
    fn the_wind_constants() {
        // 6.5 with the shipped values at W = 0.5, at t = 0 and a quarter of
        // rot1 later.
        let sway = [0.225, 0.125, 17.5, 0.75, 1.25];
        let c = wave_constants(sway, 0.0);
        assert!((c[0][0] - 0.031_830_989).abs() < 1e-8);
        assert!((c[0][1] - 0.022_736_421).abs() < 1e-8);
        assert!((c[0][2] - 0.053_051_647).abs() < 1e-8);
        assert_eq!(c[2][0], c[0][2]);
        assert_eq!(c[1], [0.0, 0.0, 0.225, 0.0]);
        assert_eq!(c[3], [0.0, 0.0, 0.125, 0.0]);
        let c = wave_constants(sway, 17.5 / 4.0);
        assert!((c[1][0] - 0.225).abs() < 1e-6 && c[1][2].abs() < 1e-6);
        assert!((c[0][3] - (17.5 / 4.0 * 1.25 / std::f64::consts::TAU) as f32).abs() < 1e-6);
    }
}
