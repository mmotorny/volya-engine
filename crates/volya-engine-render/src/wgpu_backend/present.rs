// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Frame presentation (docs/behavior/frame-presentation.md): the world
//! drawn into an off-screen target sized by `r__supersample` and reduced
//! to the screen by the presentation quad, through `postprocess` or, after
//! a distortion pass, `postprocess_d`.

use wgpu::util::DeviceExt;

use crate::world::DEPTH_FORMAT;

pub use crate::shared::present::*;

/// The target and what is built with it (1.2): the colour target
/// `$user$rendertarget`, the distortion target `$user$distort` and a depth
/// buffer, all W_t × H_t.
#[derive(Clone)]
pub(crate) struct Targets {
    screen: (u32, u32),
    supersample: u32,
    pub(crate) size: (u32, u32),
    pub(crate) colour: wgpu::Texture,
    pub(crate) distort: wgpu::Texture,
    pub(crate) depth: wgpu::Texture,
}

/// The distortion target's format: `postprocess_d` reads only its red
/// and green, and the distortion pass blends each channel by the source's
/// alpha alone, so two channels of the same 8 bits give the same offsets
/// with half the target's memory traffic (#433).
pub(crate) const DISTORT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;

/// The presentation's pipelines, the target and the values in force.
pub(crate) struct PostProcess {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    postprocess: wgpu::RenderPipeline,
    postprocess_d: wgpu::RenderPipeline,
    constants: wgpu::Buffer,
    scene_sampler: wgpu::Sampler,
    noise_sampler: wgpu::Sampler,
    /// `fx\fx_noise2` once given; white until then.
    noise: wgpu::TextureView,
    /// One texel of the distortion target's clear value (127 / 255).
    cleared: wgpu::TextureView,
    pub(crate) targets: Option<Targets>,
    /// The values last handed over, or the defaults since the target was
    /// built (2.3).
    pub(crate) effects: ScreenEffects,
    /// The noise's countdown T and offsets (3.2).
    countdown: f32,
    offsets: [u32; 2],
    /// The shared generator's state for the noise offsets: not
    /// reproducible in the original, so any start serves.
    random: u32,
    /// The previous frame had a distortion pass (3.1).
    pub(crate) distorted: bool,
    /// The last world frame went through the target.
    pub(crate) used: bool,
}

impl PostProcess {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("postprocess"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(CONSTANTS),
                    },
                    count: None,
                },
                texture(1),
                sampler(2),
                texture(3),
                sampler(4),
                texture(5),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("postprocess"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../postprocess.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("postprocess"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        // Pass states (3.4): no depth test or write, no blending.
        let pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_quad"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
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
        let postprocess = pipeline("fs_postprocess");
        let postprocess_d = pipeline("fs_postprocess_d");
        let constants = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("postprocess"),
            size: CONSTANTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // s_base0, s_base1: clamped, linear without mip; s_noise wrapped,
        // linear without mip (3.4).
        let scene_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("postprocess scene"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("postprocess noise"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let noise = device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("postprocess no noise"),
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
                &[255; 4],
            )
            .create_view(&Default::default());
        let cleared = device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("$user$distort cleared"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DISTORT_FORMAT,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &[127; 2],
            )
            .create_view(&Default::default());
        Self {
            format,
            layout,
            cleared,
            postprocess,
            postprocess_d,
            constants,
            scene_sampler,
            noise_sampler,
            noise,
            targets: None,
            effects: ScreenEffects::DEFAULTS,
            countdown: 0.0,
            offsets: [0; 2],
            random: 1,
            distorted: false,
            used: false,
        }
    }

    /// The noise texture: its top level only is sampled (3.4).
    pub(crate) fn set_noise(&mut self, texture: &wgpu::Texture) {
        self.noise = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: 0,
            mip_level_count: Some(1),
            array_layer_count: Some(1),
            ..Default::default()
        });
    }

    /// Builds the target for a `screen` and `supersample` when either
    /// differs from the one built (as the original at device creation and
    /// every reset, 1.2): the values in force return to the defaults.
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, screen: (u32, u32), supersample: u32) {
        let built = self
            .targets
            .as_ref()
            .is_some_and(|t| t.screen == screen && t.supersample == supersample);
        if built || screen.0 == 0 || screen.1 == 0 {
            return;
        }
        let size = target_size(screen.0, screen.1, supersample);
        let make = |label, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let colour = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        self.targets = Some(Targets {
            screen,
            supersample,
            size,
            colour: make("$user$rendertarget", self.format, colour),
            distort: make("$user$distort", DISTORT_FORMAT, colour),
            depth: make(
                "$user$rendertarget depth",
                DEPTH_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
        });
        self.effects = ScreenEffects::DEFAULTS;
        self.countdown = 0.0;
    }

    /// Whether the frame goes through the target (3.1 step 1).
    pub(crate) fn use_target(&self, supersample: u32) -> bool {
        self.targets.is_some() && (self.effects.active() || supersample > 1 || self.distorted)
    }

    /// One draw of the shared generator.
    fn draw_random(&mut self) -> u32 {
        self.random = self.random.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.random >> 16) & 0x7FFF
    }

    /// The presentation quad (3.1 step 5) into `screen`, with
    /// `postprocess_d` after a distortion pass (reading a texel of the
    /// target's clear value where the pass was not drawn), then `overlay` (the menus and HUD)
    /// in the same pass, so the frame is written once (#433). `last`: the
    /// pass ends the timed frame. Returns whether it drew (a target was
    /// built).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        screen: &wgpu::TextureView,
        presentation: &Presentation,
        distortion: crate::Distortion,
        last: bool,
        overlay: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) -> bool {
        let Some(t) = self.targets.clone() else {
            return false;
        };
        if t.size.1 != t.screen.1 {
            self.effects.blur = 1.0;
        }
        let n = noise_cell(self.effects.noise_grain);
        self.countdown -= presentation.frame_time;
        if self.countdown < 0.0 {
            let ox = self.draw_random() % n;
            let oy = self.draw_random() % n;
            self.offsets = [ox, oy];
            while self.countdown < 0.0 {
                self.countdown += 1.0 / self.effects.noise_fps;
            }
        }
        let corners = quad(
            t.screen,
            t.size,
            &self.effects,
            presentation.pps,
            self.offsets,
        );
        let bytes = quad_constants(t.screen, &corners, &self.effects);
        queue.write_buffer(&self.constants, 0, &bytes);
        let colour = t.colour.create_view(&Default::default());
        let distort = if distortion.drawn {
            t.distort.create_view(&Default::default())
        } else {
            self.cleared.clone()
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("postprocess"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.constants.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&colour),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.scene_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.noise),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.noise_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&distort),
                },
            ],
        });
        let timed = crate::timing::pass_writes("presentation", last);
        crate::stats::pass();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("postprocess"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: screen,
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
        crate::stats::pipeline();
        pass.set_pipeline(if distortion.pass {
            &self.postprocess_d
        } else {
            &self.postprocess
        });
        crate::stats::bind_group();
        pass.set_bind_group(0, &group, &[]);
        crate::stats::draw();
        pass.draw(0..4, 0..1);
        overlay(&mut pass);
        true
    }
}
