// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's static world on the GPU: `level.geom`'s vertex and index
//! buffers uploaded as stored (docs/formats/level-geom.md; packed attributes
//! are decoded in the vertex programs), one draw per static visual, recorded
//! once into a render bundle and replayed every frame with the frame's
//! camera.

use std::rc::Rc;

use volya_engine_formats::level_geom::LevelGeom;
use wgpu::util::DeviceExt;

use crate::Renderer;

/// Depth buffer format of the world pass.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Row-vector convention (`p · M`), stored row by row.
pub type Mat4 = [[f32; 4]; 4];

/// One draw of the static world: `index_count` indices of index buffer
/// `index_buffer` from `first_index`, each relative to `first_vertex` of
/// vertex buffer `vertex_buffer`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldDraw {
    pub vertex_buffer: u32,
    pub first_vertex: u32,
    pub index_buffer: u32,
    pub first_index: u32,
    pub index_count: u32,
    /// Model to world.
    pub transform: Mat4,
}

/// Bytes of one draw's record in the draws buffer.
const DRAW_BYTES: usize = 64;
/// Bytes of the frame constants.
const FRAME_BYTES: u64 = 64;

struct Inner {
    /// Kept for as long as the bundle that draws from them.
    _buffers: Vec<wgpu::Buffer>,
    frame: wgpu::Buffer,
    bundle: wgpu::RenderBundle,
    draws: usize,
    bytes: u64,
}

/// A static world uploaded to the GPU; cheap to clone (a handle).
#[derive(Clone)]
pub struct World(Rc<Inner>);

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("draws", &self.0.draws)
            .field("bytes", &self.0.bytes)
            .finish()
    }
}

impl World {
    /// Bytes of GPU buffers held (vertices, indices, per-draw data).
    pub fn gpu_bytes(&self) -> u64 {
        self.0.bytes
    }

    pub fn draw_count(&self) -> usize {
        self.0.draws
    }

    pub fn same(&self, other: &World) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The world as seen in one frame.
#[derive(Clone, Debug)]
pub struct WorldFrame {
    pub world: World,
    /// View × projection (start-position.md sections 6 and 7).
    pub view_projection: Mat4,
    /// What shows where nothing is drawn (RGB, 0 … 1).
    pub clear: [f32; 3],
}

/// Pipelines shared by every world drawn into one target format.
pub(crate) struct WorldPipelines {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

impl WorldPipelines {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world"),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world"),
            source: wgpu::ShaderSource::Wgsl(include_str!("world.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                // Every layout of level.geom starts with a FLOAT3 position
                // and a D3DCOLOR normal (level-geom.md section 4.1).
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4],
                })],
            },
            // Triangles that appear counter-clockwise on screen are culled
            // (static-shading.md section 10.1).
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
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
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { layout, pipeline }
    }
}

fn mat_bytes(m: &Mat4, out: &mut Vec<u8>) {
    for v in m.iter().flatten() {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// `a · b`, row-vector convention.
pub fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

impl Renderer {
    /// Uploads `geom`'s buffers as stored and records `draws`. Fails when a
    /// draw names a buffer `geom` lacks.
    pub fn create_world(&mut self, geom: &LevelGeom, draws: &[WorldDraw]) -> Result<World, String> {
        for d in draws {
            if d.vertex_buffer as usize >= geom.vertex_buffers.len()
                || d.index_buffer as usize >= geom.index_buffers.len()
            {
                return Err(format!("draw {d:?} names a missing buffer"));
            }
        }
        let device = &self.device;
        let pipelines = self
            .world_pipelines
            .get_or_insert_with(|| WorldPipelines::new(device, self.format));
        let mut bytes = 0;
        let vertex_buffers: Vec<wgpu::Buffer> = geom
            .vertex_buffers
            .iter()
            .map(|vb| {
                bytes += vb.data.len() as u64;
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
                bytes += data.len() as u64;
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("level.geom indices"),
                    contents: &data,
                    usage: wgpu::BufferUsages::INDEX,
                })
            })
            .collect();
        let mut records = Vec::with_capacity(draws.len().max(1) * DRAW_BYTES);
        for d in draws {
            mat_bytes(&d.transform, &mut records);
        }
        records.resize(records.len().max(DRAW_BYTES), 0);
        bytes += records.len() as u64;
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world"),
            layout: &pipelines.layout,
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

        // Draws grouped by buffers, so state changes only between groups.
        let mut order: Vec<usize> = (0..draws.len()).collect();
        order.sort_by_key(|&i| (draws[i].vertex_buffer, draws[i].index_buffer));
        let mut encoder =
            device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                label: Some("world"),
                color_formats: &[Some(self.format)],
                depth_stencil: Some(wgpu::RenderBundleDepthStencil {
                    format: DEPTH_FORMAT,
                    depth_read_only: false,
                    stencil_read_only: true,
                }),
                sample_count: 1,
                multiview: None,
            });
        encoder.set_pipeline(&pipelines.pipeline);
        encoder.set_bind_group(0, &bind_group, &[]);
        let mut bound = None;
        for i in order {
            let d = &draws[i];
            if bound != Some((d.vertex_buffer, d.index_buffer)) {
                bound = Some((d.vertex_buffer, d.index_buffer));
                encoder.set_vertex_buffer(0, vertex_buffers[d.vertex_buffer as usize].slice(..));
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
        let bundle = encoder.finish(&wgpu::RenderBundleDescriptor {
            label: Some("world"),
        });
        Ok(World(Rc::new(Inner {
            _buffers: vertex_buffers
                .into_iter()
                .chain(index_buffers)
                .chain([draw_buffer])
                .collect(),
            frame,
            bundle,
            draws: draws.len(),
            bytes,
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
    /// frame's clear colour, then the static world.
    pub(crate) fn encode_world(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        frame: &WorldFrame,
    ) {
        let mut constants = Vec::with_capacity(FRAME_BYTES as usize);
        mat_bytes(&frame.view_projection, &mut constants);
        let inner = &frame.world.0;
        self.queue.write_buffer(&inner.frame, 0, &constants);
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
        pass.execute_bundles([&inner.bundle]);
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
}
