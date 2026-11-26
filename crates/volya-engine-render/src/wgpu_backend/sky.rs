// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What surrounds the static world (docs/behavior/sky.md): the sky box
//! (section 5), the cloud dome (6) and the sun sprite (7.4, 7.5), drawn
//! after the opaque world (section 10).

use crate::Texture;
use crate::world::{DEPTH_FORMAT, Mat4, upload_buffer};

pub use crate::shared::sky::*;

pub(crate) struct SkyPipelines {
    uniform_layout: wgpu::BindGroupLayout,
    cube_layout: wgpu::BindGroupLayout,
    flat2_layout: wgpu::BindGroupLayout,
    flat1_layout: wgpu::BindGroupLayout,
    sky: wgpu::RenderPipeline,
    clouds: wgpu::RenderPipeline,
    sun: wgpu::RenderPipeline,
    clamp: wgpu::Sampler,
    wrap: wgpu::Sampler,
}

fn entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty,
        count: None,
    }
}

fn texture(dimension: wgpu::TextureViewDimension) -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: true },
        view_dimension: dimension,
        multisampled: false,
    }
}

const SAMPLER: wgpu::BindingType = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);

/// The part of sky.wgsl before the first pass, plus the pass `name`.
fn module_source(name: &str) -> String {
    let source = include_str!("../sky.wgsl");
    let common = &source[..source.find("//-- ").unwrap_or(source.len())];
    let start = source
        .find(&format!("//-- {name}\n"))
        .expect("pass in sky.wgsl");
    let rest = &source[start + 5..];
    let end = rest.find("//-- ").map_or(source.len(), |e| start + 5 + e);
    format!("{common}{}", &source[start..end])
}

impl SkyPipelines {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = |label, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let uniform_layout = layout(
            "sky constants",
            &[entry(
                0,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            )],
        );
        let cube = texture(wgpu::TextureViewDimension::Cube);
        let flat = texture(wgpu::TextureViewDimension::D2);
        let cube_layout = layout(
            "sky cubes",
            &[entry(0, cube), entry(1, cube), entry(2, SAMPLER)],
        );
        let flat2_layout = layout(
            "clouds",
            &[entry(0, flat), entry(1, flat), entry(2, SAMPLER)],
        );
        let flat1_layout = layout("sun", &[entry(0, flat), entry(1, SAMPLER)]);
        let pipeline = |name: &str,
                        textures: &wgpu::BindGroupLayout,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        blend: Option<wgpu::BlendState>| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(name),
                source: wgpu::ShaderSource::Wgsl(module_source(name).into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(name),
                bind_group_layouts: &[Some(&uniform_layout), Some(textures)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(name),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(&format!("vs_{name}")),
                    compilation_options: Default::default(),
                    buffers,
                },
                // Default culling (static-shading.md 10.1): every sky and
                // cloud triangle faces the centre, front-facing from inside.
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                // Depth test on (less-or-equal), depth write off.
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(&format!("fs_{name}")),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let sky = pipeline(
            "sky",
            &cube_layout,
            &[Some(wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            })],
            None,
        );
        let clouds = pipeline(
            "clouds",
            &flat2_layout,
            &[Some(wgpu::VertexBufferLayout {
                array_stride: 12,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3],
            })],
            Some(crate::world::alpha_blending()),
        );
        // effects\sun: source alpha / one (sky.md 7.5).
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let sun = pipeline(
            "sun",
            &flat1_layout,
            &[],
            Some(wgpu::BlendState {
                color: add,
                alpha: add,
            }),
        );
        // Sky: clamp, linear, no mip (5.4); sun: clamp (7.5). Clouds: wrap,
        // anisotropic, linear mip (6.3).
        let clamp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("s_sky"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            lod_max_clamp: 0.0,
            ..Default::default()
        });
        let wrap = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("s_clouds"),
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
            uniform_layout,
            cube_layout,
            flat2_layout,
            flat1_layout,
            sky,
            clouds,
            sun,
            clamp,
            wrap,
        }
    }
}

/// The sky's GPU resources for one world.
pub(crate) struct SkyResources {
    uniforms: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    /// Per section A: A's and the next section's cube maps.
    cubes: Vec<wgpu::BindGroup>,
    /// Per section A: A's and the next section's cloud textures.
    clouds: Vec<wgpu::BindGroup>,
    /// Per section: its sun sprite.
    sun: Vec<Option<wgpu::BindGroup>>,
    box_vertices: wgpu::Buffer,
    box_indices: wgpu::Buffer,
    dome_vertices: wgpu::Buffer,
    dome_indices: wgpu::Buffer,
    dome_index_count: u32,
    sky: wgpu::RenderPipeline,
    clouds_pipeline: wgpu::RenderPipeline,
    sun_pipeline: wgpu::RenderPipeline,
    _textures: Vec<Texture>,
}

fn view(t: &Texture, dimension: wgpu::TextureViewDimension) -> wgpu::TextureView {
    t.texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(dimension),
        array_layer_count: Some(if dimension == wgpu::TextureViewDimension::Cube {
            6
        } else {
            1
        }),
        ..Default::default()
    })
}

impl SkyResources {
    /// Fails when there are no sections or a cube map is not a 6-face
    /// texture.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipelines: &SkyPipelines,
        textures: &SkyTextures<Texture>,
    ) -> Result<Self, String> {
        let sections = &textures.sections;
        if sections.is_empty() {
            return Err("no weather sections".into());
        }
        for s in sections {
            let c = &s.cube;
            if c.texture.depth_or_array_layers() != 6 || c.texture.width() != c.texture.height() {
                return Err("sky texture is not a cube map".into());
            }
        }
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky constants"),
            size: SKY_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = |layout: &wgpu::BindGroupLayout, entries: &[wgpu::BindingResource]| {
            let entries: Vec<wgpu::BindGroupEntry> = entries
                .iter()
                .enumerate()
                .map(|(i, r)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: r.clone(),
                })
                .collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout,
                entries: &entries,
            })
        };
        let uniform_group = group(&pipelines.uniform_layout, &[uniforms.as_entire_binding()]);
        let cube = wgpu::TextureViewDimension::Cube;
        let flat = wgpu::TextureViewDimension::D2;
        let cube_views: Vec<_> = sections.iter().map(|s| view(&s.cube, cube)).collect();
        let cloud_views: Vec<_> = sections.iter().map(|s| view(&s.clouds, flat)).collect();
        let n = sections.len();
        let pair = |views: &[wgpu::TextureView], layout, sampler, a: usize| {
            group(
                layout,
                &[
                    wgpu::BindingResource::TextureView(&views[a]),
                    wgpu::BindingResource::TextureView(&views[(a + 1) % n]),
                    wgpu::BindingResource::Sampler(sampler),
                ],
            )
        };
        let cubes = (0..n)
            .map(|a| pair(&cube_views, &pipelines.cube_layout, &pipelines.clamp, a))
            .collect();
        let clouds = (0..n)
            .map(|a| pair(&cloud_views, &pipelines.flat2_layout, &pipelines.wrap, a))
            .collect();
        let sun = sections
            .iter()
            .map(|s| {
                s.sun.as_ref().map(|t| {
                    let v = view(t, flat);
                    group(
                        &pipelines.flat1_layout,
                        &[
                            wgpu::BindingResource::TextureView(&v),
                            wgpu::BindingResource::Sampler(&pipelines.clamp),
                        ],
                    )
                })
            })
            .collect();
        let init =
            |label, contents: &[u8], usage| upload_buffer(device, queue, label, usage, contents);
        let (box_vertices, box_indices) = box_bytes();
        let box_vertices = init("sky box", &box_vertices, wgpu::BufferUsages::VERTEX);
        let box_indices = init("sky box", &box_indices, wgpu::BufferUsages::INDEX);
        let (dome_vertices, dome_indices, dome_index_count) = dome_bytes();
        let dome_vertices = init("cloud dome", &dome_vertices, wgpu::BufferUsages::VERTEX);
        let dome_indices = init("cloud dome", &dome_indices, wgpu::BufferUsages::INDEX);
        let mut kept: Vec<Texture> = Vec::new();
        for t in sections
            .iter()
            .flat_map(|s| [Some(&s.cube), Some(&s.clouds), s.sun.as_ref()])
            .flatten()
        {
            if !kept.iter().any(|k| k.texture == t.texture) {
                kept.push(t.clone());
            }
        }
        Ok(Self {
            uniforms,
            uniform_group,
            cubes,
            clouds,
            sun,
            box_vertices,
            box_indices,
            dome_vertices,
            dome_indices,
            dome_index_count,
            sky: pipelines.sky.clone(),
            clouds_pipeline: pipelines.clouds.clone(),
            sun_pipeline: pipelines.sun.clone(),
            _textures: kept,
        })
    }

    pub(crate) fn textures(&self) -> &[Texture] {
        &self._textures
    }

    /// Writes this frame's constants.
    pub(crate) fn write(
        &self,
        queue: &wgpu::Queue,
        frame: &SkyFrame,
        eye: [f32; 3],
        view_projection: &Mat4,
        time: f32,
    ) {
        queue.write_buffer(
            &self.uniforms,
            0,
            &constants(frame, eye, view_projection, time),
        );
    }

    /// Section 10's step 3: the sky box, the sun sprite, the cloud dome.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass, frame: &SkyFrame, size: (u32, u32)) {
        let (w, h) = (size.0 as f32, size.1 as f32);
        crate::stats::bind_group();
        pass.set_bind_group(0, &self.uniform_group, &[]);
        pass.set_viewport(0.0, 0.0, w, h, SKY_DEPTH.0, SKY_DEPTH.1);
        crate::stats::pipeline();
        pass.set_pipeline(&self.sky);
        let a = frame.section % self.cubes.len();
        crate::stats::bind_group();
        pass.set_bind_group(1, &self.cubes[a], &[]);
        crate::stats::buffer();
        pass.set_vertex_buffer(0, self.box_vertices.slice(..));
        crate::stats::buffer();
        pass.set_index_buffer(self.box_indices.slice(..), wgpu::IndexFormat::Uint16);
        crate::stats::draw();
        pass.draw_indexed(0..60, 0, 0..1);
        pass.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
        let sun = self.sun.get(frame.sun_section).and_then(Option::as_ref);
        if let (Some(_), Some(sun)) = (frame.sun, sun) {
            crate::stats::pipeline();
            pass.set_pipeline(&self.sun_pipeline);
            crate::stats::bind_group();
            pass.set_bind_group(1, sun, &[]);
            crate::stats::draw();
            pass.draw(0..6, 0..1);
        }
        if frame.clouds.is_some() {
            pass.set_viewport(0.0, 0.0, w, h, SKY_DEPTH.0, SKY_DEPTH.1);
            crate::stats::pipeline();
            pass.set_pipeline(&self.clouds_pipeline);
            crate::stats::bind_group();
            pass.set_bind_group(1, &self.clouds[a], &[]);
            crate::stats::buffer();
            pass.set_vertex_buffer(0, self.dome_vertices.slice(..));
            crate::stats::buffer();
            pass.set_index_buffer(self.dome_indices.slice(..), wgpu::IndexFormat::Uint16);
            crate::stats::draw();
            pass.draw_indexed(0..self.dome_index_count, 0, 0..1);
            pass.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_pass_is_a_module_of_its_own() {
        assert!(module_source("clouds").contains("fn vs_clouds"));
        assert!(!module_source("clouds").contains("fn vs_sun"));
        assert!(module_source("sun").contains("fn fs_sun"));
    }
}
