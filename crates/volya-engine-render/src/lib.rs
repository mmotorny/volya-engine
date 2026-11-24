// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `wgpu` renderer (docs/architecture.md, Crates). Platform-free: the platform
//! supplies a device, a queue and a target (a surface texture or any render
//! attachment) in the format given to [`Renderer::new`].
//!
//! What it draws so far is the 2D layer of the title menu
//! (docs/behavior/title-menu.md): a [`DrawList`] of textured rectangles in a
//! 1024 × 768 UI space, stretched independently per axis to the target, drawn
//! in order with alpha blending. Textures come from DDS files
//! ([`Renderer::create_texture`]) or video frames ([`VideoTexture`]).
//!
//! Colour values pass through unchanged (no sRGB conversion): use a
//! non-sRGB target format.

mod capture;
mod texture;
mod video;

use std::ops::Range;

pub use texture::TextureError;
pub use video::VideoTexture;
pub use wgpu;

/// Size of the UI space in units.
pub const UI_WIDTH: f32 = 1024.0;
pub const UI_HEIGHT: f32 = 768.0;

/// Optional device features the renderer uses when the adapter has them:
/// request these when creating the device.
pub fn device_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC
}

/// An axis-aligned rectangle: UI units for screen positions, texels for
/// texture regions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The whole UI space.
    pub const SCREEN: Self = Self::new(0.0, 0.0, UI_WIDTH, UI_HEIGHT);
}

/// A texture the renderer can draw: cheap to clone (a handle).
#[derive(Clone, Debug)]
pub struct Texture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl Texture {
    /// Allocated size in texels; texture regions are relative to it.
    pub fn size(&self) -> (u32, u32) {
        (self.texture.width(), self.texture.height())
    }

    /// The underlying texture (every face and mip level).
    pub fn wgpu(&self) -> &wgpu::Texture {
        &self.texture
    }
}

/// One textured rectangle.
#[derive(Clone, Debug)]
pub struct Quad {
    /// Where to draw, in UI units.
    pub rect: Rect,
    pub texture: Texture,
    /// The texture region to draw, in texels of the allocated texture.
    pub source: Rect,
    /// Multiplies the texel colour: RGBA, 255 = unchanged.
    pub color: [u8; 4],
    /// When set, the corners to draw at instead of `rect`, in UI units, in
    /// the order top-left, top-right, bottom-right, bottom-left of `source`
    /// (e.g. a rotated picture).
    pub corners: Option<[[f32; 2]; 4]>,
}

impl Quad {
    /// The whole of `texture`, untinted.
    pub fn new(rect: Rect, texture: &Texture) -> Self {
        let (width, height) = texture.size();
        Self {
            rect,
            texture: texture.clone(),
            source: Rect::new(0.0, 0.0, width as f32, height as f32),
            color: [255; 4],
            corners: None,
        }
    }
}

/// What to draw in one frame, back to front.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub quads: Vec<Quad>,
}

/// Bytes per vertex: position (2 × f32), texture coordinates (2 × f32),
/// colour (4 × u8).
const VERTEX_BYTES: u64 = 20;
const VERTICES_PER_QUAD: u32 = 6;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    vertices: wgpu::Buffer,
    video: video::Converter,
}

impl Renderer {
    /// A renderer drawing into targets of `format`.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui texture"),
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
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_BYTES,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Straight (non-premultiplied) alpha.
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // Bilinear within a level, nearest level; edges clamped.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            device: device.clone(),
            queue: queue.clone(),
            format,
            pipeline,
            texture_layout,
            sampler,
            vertices: vertex_buffer(device, 64),
            video: video::Converter::new(device),
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// An RGBA texture (8 bits per channel, rows top to bottom), one level.
    pub fn create_rgba_texture(&self, width: u32, height: u32, rgba: &[u8]) -> Texture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rgba"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            texture.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            texture.size(),
        );
        self.wrap(texture)
    }

    /// Makes `texture` drawable: its first layer, all levels.
    fn wrap(&self, texture: wgpu::Texture) -> Texture {
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            array_layer_count: Some(1),
            ..Default::default()
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui texture"),
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
        Texture {
            texture,
            bind_group,
        }
    }

    /// Draws `list` into `target` (cleared to `clear` first) and submits.
    pub fn render(&mut self, target: &wgpu::TextureView, list: &DrawList, clear: wgpu::Color) {
        let batches = self.upload_vertices(list);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ui") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !batches.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                for (texture, vertices) in batches {
                    pass.set_bind_group(0, &list.quads[texture].texture.bind_group, &[]);
                    pass.draw(vertices, 0..1);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Writes the vertices of `list` and returns its draw batches: runs of
    /// quads sharing a texture, as (index of a quad with that texture, vertex
    /// range).
    fn upload_vertices(&mut self, list: &DrawList) -> Vec<(usize, Range<u32>)> {
        let mut data = Vec::with_capacity(list.quads.len() * (VERTICES_PER_QUAD * 20) as usize);
        let mut batches: Vec<(usize, Range<u32>)> = Vec::new();
        for (i, quad) in list.quads.iter().enumerate() {
            let (tw, th) = quad.texture.size();
            let (r, s) = (quad.rect, quad.source);
            let (u0, v0) = (s.x / tw as f32, s.y / th as f32);
            let (u1, v1) = ((s.x + s.width) / tw as f32, (s.y + s.height) / th as f32);
            let [tl, tr, br, bl] = quad.corners.unwrap_or([
                [r.x, r.y],
                [r.x + r.width, r.y],
                [r.x + r.width, r.y + r.height],
                [r.x, r.y + r.height],
            ]);
            // Two triangles: top-left, bottom-left, top-right; top-right,
            // bottom-left, bottom-right.
            for ([x, y], u, v) in [
                (tl, u0, v0),
                (bl, u0, v1),
                (tr, u1, v0),
                (tr, u1, v0),
                (bl, u0, v1),
                (br, u1, v1),
            ] {
                for value in [x, y, u, v] {
                    data.extend_from_slice(&value.to_le_bytes());
                }
                data.extend_from_slice(&quad.color);
            }
            let end = (i as u32 + 1) * VERTICES_PER_QUAD;
            match batches.last_mut() {
                Some((first, range))
                    if list.quads[*first].texture.bind_group == quad.texture.bind_group =>
                {
                    range.end = end;
                }
                _ => batches.push((i, end - VERTICES_PER_QUAD..end)),
            }
        }
        if data.len() as u64 > self.vertices.size() {
            let quads =
                (data.len() as u64 / (VERTEX_BYTES * VERTICES_PER_QUAD as u64)).next_power_of_two();
            self.vertices = vertex_buffer(&self.device, quads);
        }
        if !data.is_empty() {
            self.queue.write_buffer(&self.vertices, 0, &data);
        }
        batches
    }
}

fn vertex_buffer(device: &wgpu::Device, quads: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ui vertices"),
        size: quads * VERTICES_PER_QUAD as u64 * VERTEX_BYTES,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
