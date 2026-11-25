// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Video frames as textures (docs/behavior/startup.md 6.6, the pixel-shader
//! path). The texture is RGBA8, one level, the next power of two in each
//! dimension of the picture; texel (x, y) of the picture block holds the
//! samples unconverted: red = Y', green = Cb, blue = Cr, alpha = 255. Each
//! frame's planes upload as they are (1.5 bytes per pixel for 4:2:0) and a
//! render pass packs them into the texture (`yuv.wgsl`); drawing converts to
//! RGB after filtering (`ui.wgsl`, `fs_movie`).
//!
//! Texels outside the picture are never written; the spec leaves their
//! content to the graphics runtime (startup.md 9). Our choice: Y'CbCr black
//! (16, 128, 128, 255), so filtering at a picture edge blends in black.

use volya_engine_formats::video::{Frame, Plane};

use crate::{Rect, Renderer, Texture};

/// The pass packing a frame's planes into a video texture, shared by all
/// video textures.
pub(crate) struct Converter {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl Converter {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let plane = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("video planes"),
            entries: &[plane(0), plane(1), plane(2)],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("yuv"),
            source: wgpu::ShaderSource::Wgsl(include_str!("yuv.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("yuv"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("yuv"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout }
    }
}

/// The planes of the current frame on the GPU.
struct Planes {
    textures: [wgpu::Texture; 3],
    bind_group: wgpu::BindGroup,
}

/// A texture showing video frames ([`Renderer::update_video`]).
pub struct VideoTexture {
    texture: Texture,
    width: u32,
    height: u32,
    planes: Option<Planes>,
}

impl VideoTexture {
    /// The texture to draw.
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    /// The frame's region of the texture, in texels.
    pub fn frame_rect(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32, self.height as f32)
    }
}

/// The initial content of a video texture: Y'CbCr black (our choice, see
/// the module documentation).
const OUTSIDE: wgpu::Color = wgpu::Color {
    r: 16.0 / 255.0,
    g: 128.0 / 255.0,
    b: 128.0 / 255.0,
    a: 1.0,
};

impl Renderer {
    /// A texture for frames whose picture is `width` × `height` pixels.
    pub fn create_video_texture(&self, width: u32, height: u32) -> VideoTexture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("video"),
            size: wgpu::Extent3d {
                width: width.next_power_of_two(),
                height: height.next_power_of_two(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        crate::stats::pass();
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("video fill"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &texture.create_view(&Default::default()),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(OUTSIDE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        crate::stats::submit();
        self.queue.submit([encoder.finish()]);
        VideoTexture {
            texture: self.wrap_video(texture),
            width,
            height,
            planes: None,
        }
    }

    /// Shows `frame` in `video`: its top-left picture-sized block
    /// (startup.md 6.6).
    ///
    /// # Panics
    ///
    /// If the frame's picture is not the size `video` was created for.
    pub fn update_video(&self, video: &mut VideoTexture, frame: &Frame) {
        let size = |p: &Plane| (p.width as u32, p.height as u32);
        let frame_planes = frame.texture_planes();
        assert_eq!(
            size(&frame_planes[0]),
            (video.width, video.height),
            "video frame size"
        );
        let stale = video.planes.as_ref().is_none_or(|planes| {
            planes
                .textures
                .iter()
                .zip(&frame_planes)
                .any(|(t, p)| (t.width(), t.height()) != size(p))
        });
        if stale {
            video.planes = Some(self.create_planes(&frame_planes));
        }
        let planes = video.planes.as_ref().expect("planes allocated");
        for (texture, plane) in planes.textures.iter().zip(&frame_planes) {
            self.queue.write_texture(
                texture.as_image_copy(),
                plane.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(plane.stride as u32),
                    rows_per_image: None,
                },
                texture.size(),
            );
        }
        let target = video.texture.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("yuv") });
        {
            crate::stats::pass();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("yuv"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Outside the viewport the texture keeps its initial
                        // content.
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(0.0, 0.0, video.width as f32, video.height as f32, 0.0, 1.0);
            crate::stats::pipeline();
            pass.set_pipeline(&self.video.pipeline);
            crate::stats::bind_group();
            pass.set_bind_group(0, &planes.bind_group, &[]);
            crate::stats::draw();
            pass.draw(0..3, 0..1);
        }
        crate::stats::submit();
        self.queue.submit([encoder.finish()]);
    }

    fn create_planes(&self, planes: &[Plane; 3]) -> Planes {
        let textures = planes.each_ref().map(|p| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("video plane"),
                size: wgpu::Extent3d {
                    width: p.width as u32,
                    height: p.height as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });
        let views = textures
            .each_ref()
            .map(|t| t.create_view(&Default::default()));
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("video planes"),
            layout: &self.video.layout,
            entries: &[0, 1, 2].map(|i| wgpu::BindGroupEntry {
                binding: i,
                resource: wgpu::BindingResource::TextureView(&views[i as usize]),
            }),
        });
        Planes {
            textures,
            bind_group,
        }
    }
}
