// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The display's gamma ramp (docs/behavior/display-gamma.md): the last
//! step of a presented picture, mapping every channel through the table.

/// Bytes of the ramp's constants: the rectangle and 256 entries.
const CONSTANTS: u64 = 16 + 256 * 4;

/// The ramp's pipeline and constants, made with the renderer.
pub(crate) struct RampPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    constants: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

impl RampPass {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ramp"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(CONSTANTS),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ramp"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ramp.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ramp"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ramp"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_screen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ramp"),
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
        let constants = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ramp"),
            size: CONSTANTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // As the frame's fit: bilinear, edges clamped.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ramp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            constants,
            sampler,
        }
    }

    /// Draws `frame` into `target` within `rect` (target pixels: x, y,
    /// width, height), black around it, every channel mapped through
    /// `table`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &wgpu::Texture,
        target: &wgpu::TextureView,
        [x, y, w, h]: [f32; 4],
        table: &[u16; 256],
    ) {
        let mut bytes = Vec::with_capacity(CONSTANTS as usize);
        for v in [x, y, x + w, y + h] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        for &e in table {
            bytes.extend_from_slice(&(f32::from(e) / 65535.0).to_le_bytes());
        }
        queue.write_buffer(&self.constants, 0, &bytes);
        let view = frame.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ramp"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.constants.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let timed = crate::timing::pass_writes("ramp", false);
        crate::stats::pass();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ramp"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(&self.pipeline);
        crate::stats::bind_group();
        pass.set_bind_group(0, &group, &[]);
        crate::stats::draw();
        pass.draw(0..3, 0..1);
    }
}
