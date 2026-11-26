// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `wgpu` renderer (docs/architecture/README.md). Platform-free: the platform
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

pub(crate) mod capture;
pub mod details;
pub mod particles;
pub mod present;
pub(crate) mod ramp;
pub mod sky;
pub(crate) mod texture;
pub(crate) mod timing;
pub(crate) mod video;
pub mod world;

use std::ops::Range;

use crate::stats;

pub use texture::{
    TextureError, TexturePath, TextureUpload, Transcoded, transcode, transcode_into, transcoded_len,
};
pub use video::VideoTexture;
pub use wgpu;
pub use world::{World, WorldDraw, WorldFrame};

/// Size of the UI space in units.
pub const UI_WIDTH: f32 = 1024.0;
pub const UI_HEIGHT: f32 = 768.0;

/// Optional device features the renderer uses when the adapter has them:
/// request these when creating the device.
pub fn device_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    adapter.features()
        & (wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::TEXTURE_COMPRESSION_ETC2
            | wgpu::Features::TIMESTAMP_QUERY)
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
    /// Holds Y'CbCr, converted when drawn (a [`VideoTexture`]).
    ycbcr: bool,
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
    /// When set, a colour per corner (in the order of `corners`) instead of
    /// `color`, interpolated across the quad.
    pub corner_colors: Option<[[u8; 4]; 4]>,
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
            corner_colors: None,
        }
    }
}

/// What to draw in one frame: the 3D world, if any, then the quads over
/// it, back to front.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub world: Option<WorldFrame>,
    pub quads: Vec<Quad>,
    /// The display's gamma ramp the picture is shown through
    /// (docs/behavior/display-gamma.md): entry *i* / 65535 for channel
    /// value *i*, the same for R, G and B; `None` for the identity.
    pub ramp: Option<[u16; 256]>,
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
    /// For Y'CbCr video textures.
    movie_pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// For video textures.
    movie_sampler: wgpu::Sampler,
    vertices: wgpu::Buffer,
    video: video::Converter,
    /// Made with the renderer (and again when the mode changes).
    world_pipelines: Option<world::WorldPipelines>,
    /// The light passes' attenuation textures, once given.
    light_textures: Option<world::LightTextures>,
    /// The draws the last frame's light passes drew.
    light_draws: usize,
    /// The world pass's depth buffer, sized to the latest target.
    depth: Option<wgpu::Texture>,
    /// The frame [`Renderer::render_scaled`] draws into.
    offscreen: Option<Texture>,
    /// Textures made ahead of their first use, by source path
    /// ([`Renderer::add_prepared`]).
    prepared: std::cell::RefCell<std::collections::HashMap<String, Texture>>,
    /// Bytes of texture data written so far ([`Renderer::texture_bytes_written`]).
    texture_bytes: std::cell::Cell<u64>,
    /// Bytes of world geometry written so far ([`Renderer::geometry_bytes_written`]).
    geometry_bytes: std::cell::Cell<u64>,
    /// The particles' pipelines and buffers (always set; taken while a
    /// frame is encoded).
    particles: Option<particles::ParticlePipelines>,
    /// The off-screen target and the presentation quad
    /// (frame-presentation.md).
    post: present::PostProcess,
    /// Detail objects' pipelines and the level's models (detail-objects.md).
    details: details::DetailPipelines,
    /// The display's gamma ramp (display-gamma.md).
    ramp: ramp::RampPass,
    /// Times frames on the GPU while on ([`Renderer::set_gpu_timing`]).
    timer: Option<timing::FrameTimer>,
    /// [`Renderer::set_unblended_trees`].
    unblended_trees: bool,
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
            source: wgpu::ShaderSource::Wgsl(include_str!("../ui.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline_for = |entry_point: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry_point),
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
                    entry_point: Some(entry_point),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        // Straight (non-premultiplied) alpha; also the
                        // movie shader's blending (startup.md 6.6).
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = pipeline_for("fs");
        let movie_pipeline = pipeline_for("fs_movie");
        // Bilinear within a level, nearest level; edges clamped.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Video textures: wrap addressing, linear filtering (startup.md
        // 6.6). Anisotropic filtering, which the original enables, is left
        // off (our choice): these quads are scaled nearly evenly, where it
        // makes no visible difference.
        let movie_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("movie"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut renderer = Self {
            device: device.clone(),
            queue: queue.clone(),
            format,
            pipeline,
            movie_pipeline,
            texture_layout,
            sampler,
            movie_sampler,
            vertices: vertex_buffer(device, 64),
            video: video::Converter::new(device),
            world_pipelines: None,
            light_textures: None,
            light_draws: 0,
            depth: None,
            offscreen: None,
            prepared: Default::default(),
            texture_bytes: Default::default(),
            geometry_bytes: Default::default(),
            // Made with the renderer, before any level: what lives across
            // levels is allocated below their data, so a level's freed
            // memory stays one block for the next load (measured: made with
            // the first effect drawn, the second New Game's wasm heap grew
            // from 99 to 112 MiB).
            particles: Some(particles::ParticlePipelines::new(device, queue, format)),
            post: present::PostProcess::new(device, queue, format),
            details: details::DetailPipelines::new(device, format),
            ramp: ramp::RampPass::new(device, format),
            timer: None,
            unblended_trees: false,
        };
        // The world's pipelines too (the light passes' among them).
        renderer.make_world_pipelines();
        renderer
    }

    /// Grows the particles' vertex buffer to hold `particles`
    /// sprites at once, so that no buffer is made while a level runs (made
    /// with its world).
    pub fn reserve_particles(&mut self, particles: usize) {
        if let Some(p) = &mut self.particles {
            p.reserve(&self.device, particles);
        }
    }

    /// The particles' vertex buffer's size in bytes.
    pub fn particle_buffer_size(&self) -> u64 {
        self.particles.as_ref().map_or(0, |p| p.buffer_size())
    }

    /// Keeps `texture`, made from the file at `path`, for
    /// [`Renderer::take_prepared`].
    pub fn add_prepared(&self, path: &str, texture: Texture) {
        self.prepared.borrow_mut().insert(path.to_owned(), texture);
    }

    /// Bytes of texture data written since the renderer was made: on the
    /// GL backend what is held in the wasm heap until WebGL reports the
    /// writes done ([`Renderer::flush_uploads`]).
    pub fn texture_bytes_written(&self) -> u64 {
        self.texture_bytes.get()
    }

    /// Bytes of the worlds' geometry written since the renderer was made
    /// (`GeometryUpload::step`), for the diagnostic log (#443).
    pub fn geometry_bytes_written(&self) -> u64 {
        self.geometry_bytes.get()
    }

    /// The texture prepared for `path`, if any (it is then forgotten).
    pub fn take_prepared(&self, path: &str) -> Option<Texture> {
        self.prepared.borrow_mut().remove(path)
    }

    /// Submits the uploads written so far and frees the staging of finished
    /// submissions: wgpu's GL backend holds written bytes in CPU memory (the
    /// wasm heap on the web) until then (#113).
    pub fn flush_uploads(&self) {
        crate::stats::submit();
        self.queue.submit([]);
        let _ = self.device.poll(wgpu::PollType::Poll);
    }

    /// Forgets every prepared texture.
    pub fn clear_prepared(&self) {
        self.prepared.borrow_mut().clear();
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
        self.wrap_with(texture, &self.sampler, false)
    }

    /// Makes video `texture` drawable as Y'CbCr.
    fn wrap_video(&self, texture: wgpu::Texture) -> Texture {
        self.wrap_with(texture, &self.movie_sampler, true)
    }

    fn wrap_with(&self, texture: wgpu::Texture, sampler: &wgpu::Sampler, ycbcr: bool) -> Texture {
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
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Texture {
            texture,
            bind_group,
            ycbcr,
        }
    }

    /// Draws `list` into `target` (cleared to `clear` first, or to the
    /// world's clear colour when it has a world) and submits; through the
    /// list's gamma ramp, if any.
    pub fn render(&mut self, target: &wgpu::Texture, list: &DrawList, clear: wgpu::Color) {
        self.draw_ramped(target, list, clear, None);
    }

    /// [`Renderer::render`], the frame timed in `timed` (without the ramp's
    /// pass).
    fn draw_ramped(
        &mut self,
        target: &wgpu::Texture,
        list: &DrawList,
        clear: wgpu::Color,
        timed: Option<usize>,
    ) {
        let size = (target.width(), target.height());
        match &list.ramp {
            None => self.draw(target, list, clear, timed),
            Some(table) => {
                let frame = self.offscreen_frame(size);
                self.draw(frame.wgpu(), list, clear, timed);
                let rect = [0.0, 0.0, size.0 as f32, size.1 as f32];
                self.draw_ramp(frame.wgpu(), target, rect, table);
            }
        }
    }

    /// Shows `frame` in `target` within `rect` through the gamma `table`.
    fn draw_ramp(
        &self,
        frame: &wgpu::Texture,
        target: &wgpu::Texture,
        rect: [f32; 4],
        table: &[u16; 256],
    ) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ramp"),
            });
        let view = target.create_view(&Default::default());
        self.ramp.draw(
            &self.device,
            &self.queue,
            &mut encoder,
            frame,
            &view,
            rect,
            table,
        );
        crate::stats::submit();
        self.queue.submit([encoder.finish()]);
    }

    /// The frame drawn into before it is scaled or shown through the ramp,
    /// `size` pixels.
    fn offscreen_frame(&mut self, size: (u32, u32)) -> Texture {
        let stale = self.offscreen.as_ref().is_none_or(|t| t.size() != size);
        if stale {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("frame"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.offscreen = Some(self.wrap(texture));
        }
        self.offscreen.clone().expect("offscreen frame")
    }

    /// Draws `list` into `target` as [`Renderer::render`], without the
    /// ramp; its last pass ends the frame timed in `timed`.
    fn draw(
        &mut self,
        target: &wgpu::Texture,
        list: &DrawList,
        clear: wgpu::Color,
        timed: Option<usize>,
    ) {
        let batches = self.upload_vertices(list);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ui") });
        let target_size = (target.width(), target.height());
        let target = &target.create_view(&Default::default());
        let drawn = match &list.world {
            Some(frame) => self.present_frame(
                &mut encoder,
                target,
                target_size,
                frame,
                timed.is_some(),
                &list.quads,
                &batches,
            ),
            None => false,
        };
        let load = match list.world {
            Some(_) => wgpu::LoadOp::Load,
            None => wgpu::LoadOp::Clear(clear),
        };
        if !drawn {
            let timed = crate::timing::pass_writes("ui", timed.is_some());
            crate::stats::pass();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: crate::timing::writes(&timed),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.draw_quads(&mut pass, &list.quads, &batches);
        }
        crate::stats::submit();
        tracing::info_span!("render.submit").in_scope(|| self.queue.submit([encoder.finish()]));
    }

    /// Draws `list` at `size` pixels and shows the result in `target`
    /// scaled to fit with its aspect ratio kept, centred, black around it
    /// (#49). At the target's own size it draws straight into it.
    ///
    /// While GPU timing is on ([`Renderer::set_gpu_timing`]) the frame is
    /// timed ([`Renderer::take_gpu_times`]).
    pub fn render_scaled(
        &mut self,
        target: &wgpu::Texture,
        list: &DrawList,
        clear: wgpu::Color,
        size: (u32, u32),
    ) {
        let timed = self.timer.as_mut().and_then(|t| t.begin());
        self.draw_scaled(target, list, clear, size, timed);
        if let (Some(timer), Some(slot)) = (self.timer.as_mut(), timed) {
            timer.end(&self.device, &self.queue, slot);
        }
    }

    /// The draw calls and state changes encoded since the last call
    /// (#387).
    pub fn take_frame_stats(&self) -> stats::FrameStats {
        stats::take()
    }

    /// The static world's draw calls by program and kind since the last
    /// call (#387).
    pub fn take_static_calls(&self) -> stats::StaticCalls {
        stats::take_static()
    }

    /// Whether the device can time frames on the GPU (timestamp queries).
    pub fn can_time_gpu(&self) -> bool {
        self.device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
    }

    /// Turns timing frames on the GPU on or off (off by default; nothing
    /// where the device can't, [`Renderer::can_time_gpu`]).
    pub fn set_gpu_timing(&mut self, on: bool) {
        if on != self.timer.is_some() {
            self.timer = if on {
                timing::FrameTimer::new(&self.device, &self.queue)
            } else {
                None
            };
        }
    }

    /// The GPU times of the frames [`Renderer::render_scaled`] drew that
    /// were read back since the last call, milliseconds: the time its passes
    /// kept the GPU busy, the sum of [`Renderer::take_gpu_pass_times`]'. Read back once the GPU
    /// is done, a few frames late, when the device is polled.
    pub fn take_gpu_times(&mut self) -> Vec<f64> {
        self.timer.as_mut().map(|t| t.take()).unwrap_or_default()
    }

    /// The GPU times of the passes of the frames [`Renderer::take_gpu_times`]
    /// returned, milliseconds, by pass name (`world`, `distortion`,
    /// `presentation`, `ui`, `fit`, `ramp`; #387).
    pub fn take_gpu_pass_times(&mut self) -> Vec<(&'static str, f64)> {
        self.timer
            .as_mut()
            .map(|t| t.take_passes())
            .unwrap_or_default()
    }

    /// [`Renderer::render_scaled`]'s drawing; its last pass ends the
    /// frame timed in `timed`.
    fn draw_scaled(
        &mut self,
        target: &wgpu::Texture,
        list: &DrawList,
        clear: wgpu::Color,
        size: (u32, u32),
        timed: Option<usize>,
    ) {
        if size == (target.width(), target.height()) || size.0 == 0 || size.1 == 0 {
            self.draw_ramped(target, list, clear, timed);
            return;
        }
        let frame = self.offscreen_frame(size);
        self.draw(frame.wgpu(), list, clear, None);
        let [x, y, w, h] = volya_engine_formats::ui::Resolution::new(size.0, size.1)
            .letterbox((target.width() as f32, target.height() as f32));
        if let Some(table) = &list.ramp {
            self.draw_ramp(frame.wgpu(), target, [x, y, w, h], table);
            return;
        }
        let blit = DrawList {
            quads: vec![Quad::new(Rect::SCREEN, &frame)],
            ..Default::default()
        };
        let batches = self.upload_vertices(&blit);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fit") });
        {
            let view = target.create_view(&Default::default());
            let timed = crate::timing::pass_writes("fit", timed.is_some());
            crate::stats::pass();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: crate::timing::writes(&timed),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if w >= 1.0 && h >= 1.0 {
                pass.set_viewport(x, y, w, h, 0.0, 1.0);
                crate::stats::buffer();
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                crate::stats::pipeline();
                pass.set_pipeline(&self.pipeline);
                crate::stats::bind_group();
                pass.set_bind_group(0, &frame.bind_group, &[]);
                for (_, vertices) in batches {
                    crate::stats::draw();
                    pass.draw(vertices, 0..1);
                }
            }
        }
        crate::stats::submit();
        tracing::info_span!("render.submit_fit").in_scope(|| self.queue.submit([encoder.finish()]));
    }

    /// The level's detail models (docs/behavior/detail-objects.md), their
    /// texture, whether their shader has the alpha test, and room for
    /// `capacity` instances a frame: made with the level's world, so
    /// nothing is made while it runs.
    pub fn set_details(
        &mut self,
        models: &[details::DetailGeometry],
        texture: &Texture,
        alpha_test: bool,
        capacity: usize,
    ) {
        self.details.set_level(
            &self.device,
            &self.queue,
            models,
            texture,
            alpha_test,
            capacity,
        );
    }

    /// Drops the level's detail models.
    pub fn clear_details(&mut self) {
        self.details.clear_level();
    }

    /// Bytes of the detail objects' buffers on the GPU.
    pub fn details_gpu_bytes(&self) -> u64 {
        self.details.gpu_bytes()
    }

    /// Builds the off-screen target for a `screen` of the game's size and
    /// `r__supersample` `supersample` when either changed
    /// (frame-presentation.md 1.2: at device creation and every reset);
    /// the platform calls it every frame, so it is made before a level is.
    pub fn prepare_presentation(&mut self, screen: (u32, u32), supersample: u32) {
        self.post.prepare(&self.device, screen, supersample);
    }

    /// The `$user$rendertarget` size and whether the last world frame went
    /// through it (tests).
    pub fn presentation_target(&self) -> Option<((u32, u32), bool)> {
        self.post.targets.as_ref().map(|t| (t.size, self.post.used))
    }

    /// The noise texture of the presentation (`fx\fx_noise2`,
    /// frame-presentation.md 3.4).
    pub fn set_presentation_noise(&mut self, texture: &Texture) {
        self.post.set_noise(texture.wgpu());
    }

    /// Draws `quads` in their `batches` ([`Renderer::upload_vertices`])
    /// into `pass`.
    fn draw_quads(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        quads: &[Quad],
        batches: &[(usize, Range<u32>)],
    ) {
        draw_quads(
            pass,
            &self.vertices,
            [&self.pipeline, &self.movie_pipeline],
            quads,
            batches,
        );
    }

    /// One world frame into the `screen` of `size` (frame-presentation.md
    /// 3.1): the world into the off-screen target and the presentation quad
    /// onto the screen with `quads` over it in the same pass, or the world
    /// straight onto the screen. Returns whether `quads` were drawn; `last`:
    /// the presentation ends the timed frame.
    #[allow(clippy::too_many_arguments)]
    fn present_frame(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        screen: &wgpu::TextureView,
        size: (u32, u32),
        frame: &WorldFrame,
        last: bool,
        quads: &[Quad],
        batches: &[(usize, Range<u32>)],
    ) -> bool {
        let p = frame.presentation;
        self.post.prepare(&self.device, size, p.supersample);
        // The hand-over of the frame's camera update (2.2).
        if let Some(e) = frame.effects {
            self.post.effects = e;
        }
        let through = self.post.use_target(p.supersample);
        self.post.used = through;
        let mut drawn = false;
        let distorted = match self.post.targets.clone().filter(|_| through) {
            Some(t) => {
                let colour = t.colour.create_view(&Default::default());
                let distort = t.distort.create_view(&Default::default());
                let depth = t.depth.create_view(&Default::default());
                let distortion = tracing::info_span!("render.world").in_scope(|| {
                    self.encode_frame(encoder, &colour, t.size, &depth, frame, Some(&distort))
                });
                let (vertices, pipelines) =
                    (&self.vertices, [&self.pipeline, &self.movie_pipeline]);
                drawn = tracing::info_span!("render.post").in_scope(|| {
                    self.post.draw(
                        &self.device,
                        &self.queue,
                        encoder,
                        screen,
                        &p,
                        distortion,
                        last,
                        |pass| draw_quads(pass, vertices, pipelines, quads, batches),
                    )
                });
                distortion.pass
            }
            None => {
                let depth = self.depth_view(size.0, size.1);
                tracing::info_span!("render.world")
                    .in_scope(|| self.encode_frame(encoder, screen, size, &depth, frame, None))
                    .pass
            }
        };
        self.post.distorted = distorted;
        drawn
    }

    /// The world pass of `frame` with its particle effects into `target` of
    /// `size` with `depth`, then, with a distortion list, the distortion
    /// pass into `distort` (particle-drawing.md 6.2). Returns whether the
    /// frame had a distortion pass and whether it was drawn. Drawn straight to the screen (no
    /// `distort`), the pass's picture would not be shown
    /// (frame-presentation.md 3.1), so it is left undrawn, but the frame
    /// still counts as one with a distortion pass.
    fn encode_frame(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        depth: &wgpu::TextureView,
        frame: &WorldFrame,
        distort: Option<&wgpu::TextureView>,
    ) -> Distortion {
        let mut pipelines = self.particles.take();
        let prepared = if frame.particles.is_empty() {
            None
        } else {
            let pipelines = pipelines.get_or_insert_with(|| {
                particles::ParticlePipelines::new(&self.device, &self.queue, self.format)
            });
            let l = &frame.light;
            pipelines.prepare(
                &self.device,
                &self.queue,
                &frame.particles,
                &frame.view,
                &frame.projection,
                (l.fog_color, l.fog_near, l.fog_far),
            )
        };
        let both = pipelines.as_ref().zip(prepared.as_ref());
        let distorted = both.is_some_and(|(_, p)| p.has(particles::ParticleQueue::Distortion));
        // A distortion list none of whose sprites reaches the screen
        // leaves the target as cleared: the presentation reads a texel of
        // the clear value instead and the pass is not drawn (#433).
        let drawn = both.is_some_and(|(_, p)| p.distortion_shown);
        let shown = distort.filter(|_| distorted && drawn);
        self.encode_world(encoder, target, size, depth, frame, both, shown.is_some());
        if let (Some((pipelines, prepared)), Some(distort)) = (both, shown) {
            // TODO(#329): particle-drawing.md 6.2 does not state the view
            // and projection of the distortion pass; the scene's
            // (believed).
            stats::set_part(stats::Part::Particles);
            pipelines.distort(encoder, prepared, distort, depth);
            stats::set_part(stats::Part::Screen);
        }
        self.particles = pipelines;
        Distortion {
            pass: distorted,
            drawn: shown.is_some(),
        }
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
            let [ctl, ctr, cbr, cbl] = quad.corner_colors.unwrap_or([quad.color; 4]);
            for ([x, y], u, v, color) in [
                (tl, u0, v0, ctl),
                (bl, u0, v1, cbl),
                (tr, u1, v0, ctr),
                (tr, u1, v0, ctr),
                (bl, u0, v1, cbl),
                (br, u1, v1, cbr),
            ] {
                for value in [x, y, u, v] {
                    data.extend_from_slice(&value.to_le_bytes());
                }
                data.extend_from_slice(&color);
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

/// A world frame's distortion pass (particle-drawing.md 6.2).
#[derive(Clone, Copy)]
pub(crate) struct Distortion {
    /// The frame has a distortion list (frame-presentation.md 3.1).
    pub pass: bool,
    /// The pass was drawn into the distortion target.
    pub drawn: bool,
}

/// Draws `quads` in their `batches` from `vertices` into `pass`, with the
/// UI pipeline or, for Y'CbCr textures, the movie pipeline.
fn draw_quads(
    pass: &mut wgpu::RenderPass<'_>,
    vertices: &wgpu::Buffer,
    [pipeline, movie]: [&wgpu::RenderPipeline; 2],
    quads: &[Quad],
    batches: &[(usize, Range<u32>)],
) {
    if batches.is_empty() {
        return;
    }
    crate::stats::buffer();
    pass.set_vertex_buffer(0, vertices.slice(..));
    for (texture, range) in batches {
        let texture = &quads[*texture].texture;
        crate::stats::pipeline();
        pass.set_pipeline(if texture.ycbcr { movie } else { pipeline });
        crate::stats::bind_group();
        pass.set_bind_group(0, &texture.bind_group, &[]);
        crate::stats::draw();
        pass.draw(range.clone(), 0..1);
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
