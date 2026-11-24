// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What surrounds the static world (docs/behavior/sky.md): the sky box
//! (section 5), the cloud dome (6) and the sun sprite (7.4, 7.5), drawn
//! after the opaque world (section 10).

use wgpu::util::DeviceExt;

use crate::Texture;
use crate::world::{DEPTH_FORMAT, Mat4, multiply, push_mat, push_vec4};

/// The sky box: positions and texture coordinates (section 5.1).
pub const SKY_BOX: [([f32; 3], [f32; 3]); 12] = [
    ([-1.0, -1.0, -1.0], [-1.0, -1.01, -1.0]),
    ([1.0, -1.0, -1.0], [1.0, -1.01, -1.0]),
    ([-1.0, -1.0, 1.0], [-1.0, -1.01, 1.0]),
    ([1.0, -1.0, 1.0], [1.0, -1.01, 1.0]),
    ([-1.0, 1.0, -1.0], [-1.0, 1.0, -1.0]),
    ([1.0, 1.0, -1.0], [1.0, 1.0, -1.0]),
    ([-1.0, 1.0, 1.0], [-1.0, 1.0, 1.0]),
    ([1.0, 1.0, 1.0], [1.0, 1.0, 1.0]),
    ([-1.0, 0.0, -1.0], [-1.0, -1.0, -1.0]),
    ([1.0, 0.0, -1.0], [1.0, -1.0, -1.0]),
    ([1.0, 0.0, 1.0], [1.0, -1.0, 1.0]),
    ([-1.0, 0.0, 1.0], [-1.0, -1.0, 1.0]),
];

pub const SKY_BOX_TRIANGLES: [[u16; 3]; 20] = [
    [0, 2, 3],
    [3, 1, 0],
    [4, 5, 7],
    [7, 6, 4],
    [0, 1, 9],
    [9, 8, 0],
    [8, 9, 5],
    [5, 4, 8],
    [1, 3, 10],
    [10, 9, 1],
    [9, 10, 7],
    [7, 5, 9],
    [3, 2, 11],
    [11, 10, 3],
    [10, 11, 6],
    [6, 7, 10],
    [2, 0, 8],
    [8, 11, 2],
    [11, 8, 4],
    [4, 6, 11],
];

/// The viewport depth range of the sky box and the cloud dome (5.5).
pub const SKY_DEPTH: (f32, f32) = (0.99999, 1.0);

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    a.map(|c| c / l)
}

/// The cloud dome (section 6.2): the upper half of a twice-subdivided
/// icosahedron on the unit sphere, every triangle facing the centre.
pub fn cloud_dome() -> (Vec<[f32; 3]>, Vec<[u16; 3]>) {
    let (s, c) = (1.0 / 5f32.sqrt(), 2.0 / 5f32.sqrt());
    let tau = std::f32::consts::TAU;
    let mut v: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0], [0.0, -1.0, 0.0]];
    for k in 0..5 {
        let a = tau * k as f32 / 5.0;
        v.push([c * a.cos(), s, c * a.sin()]);
    }
    for k in 0..5 {
        let a = tau * k as f32 / 5.0 + tau / 10.0;
        v.push([c * a.cos(), -s, c * a.sin()]);
    }
    let (u, l) = (|k: usize| 2 + k % 5, |k: usize| 7 + k % 5);
    let mut faces: Vec<[usize; 3]> = Vec::new();
    for k in 0..5 {
        faces.push([0, u(k), u(k + 1)]);
        faces.push([1, l(k + 1), l(k)]);
        faces.push([u(k), l(k), u(k + 1)]);
        faces.push([u(k + 1), l(k), l(k + 1)]);
    }
    for _ in 0..2 {
        let mut midpoints = std::collections::HashMap::new();
        let mut next = Vec::new();
        for [a, b, c] in faces {
            let mut mid = |i: usize, j: usize| {
                *midpoints.entry((i.min(j), i.max(j))).or_insert_with(|| {
                    let m = unit([0, 1, 2].map(|k| (v[i][k] + v[j][k]) / 2.0));
                    v.push(m);
                    v.len() - 1
                })
            };
            let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
            next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        faces = next;
    }
    // The upper half, renumbered; wound to face the centre.
    let mut index = vec![None; v.len()];
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for f in faces {
        if f.iter().any(|&i| v[i][1] < -1e-6) {
            continue;
        }
        let [a, b, c] = f.map(|i| {
            *index[i].get_or_insert_with(|| {
                vertices.push(v[i]);
                vertices.len() - 1
            }) as u16
        });
        let n = cross(
            sub(vertices[b as usize], vertices[a as usize]),
            sub(vertices[c as usize], vertices[a as usize]),
        );
        let outward = dot(n, vertices[a as usize]) > 0.0;
        triangles.push(if outward { [a, c, b] } else { [a, b, c] });
    }
    (vertices, triangles)
}

/// The textures of the sky passes: the two sections' cube maps and cloud
/// textures, and the sun sprite's texture if the lens-flare section draws
/// one.
#[derive(Clone, Debug)]
pub struct SkyTextures<T> {
    pub cubes: [T; 2],
    pub clouds: [T; 2],
    pub sun: Option<T>,
}

/// The sky's values in one frame (bytes as the specs give them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyFrame {
    /// Bytes of C_sky and of the hourly blend weight t (5.3).
    pub color: [u8; 4],
    /// θ_sky, radians.
    pub rotation: f32,
    /// Colour 0 and colour 1 of the cloud dome (6.3); `None` when the
    /// clouds are not drawn (6.1).
    pub clouds: Option<([u8; 4], [u8; 4])>,
    /// The sun sprite's corners 0 … 3 and colour (7.4); `None` when not
    /// drawn.
    pub sun: Option<([[f32; 3]; 4], [u8; 4])>,
}

/// Bytes of the sky constants.
const SKY_BYTES: u64 = 336;

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
    let source = include_str!("sky.wgsl");
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
    cubes: wgpu::BindGroup,
    clouds: wgpu::BindGroup,
    sun: Option<wgpu::BindGroup>,
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

fn bytes_of<T: Copy>(values: &[T], each: impl Fn(&T, &mut Vec<u8>)) -> Vec<u8> {
    let mut out = Vec::new();
    for v in values {
        each(v, &mut out);
    }
    out.resize(out.len().next_multiple_of(4), 0);
    out
}

impl SkyResources {
    /// Fails when a cube map is not a 6-face texture.
    pub(crate) fn new(
        device: &wgpu::Device,
        pipelines: &SkyPipelines,
        textures: &SkyTextures<Texture>,
    ) -> Result<Self, String> {
        for c in &textures.cubes {
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
        let (c0, c1) = (
            view(&textures.cubes[0], cube),
            view(&textures.cubes[1], cube),
        );
        let cubes = group(
            &pipelines.cube_layout,
            &[
                wgpu::BindingResource::TextureView(&c0),
                wgpu::BindingResource::TextureView(&c1),
                wgpu::BindingResource::Sampler(&pipelines.clamp),
            ],
        );
        let (k0, k1) = (
            view(&textures.clouds[0], flat),
            view(&textures.clouds[1], flat),
        );
        let clouds = group(
            &pipelines.flat2_layout,
            &[
                wgpu::BindingResource::TextureView(&k0),
                wgpu::BindingResource::TextureView(&k1),
                wgpu::BindingResource::Sampler(&pipelines.wrap),
            ],
        );
        let sun = textures.sun.as_ref().map(|t| {
            let v = view(t, flat);
            group(
                &pipelines.flat1_layout,
                &[
                    wgpu::BindingResource::TextureView(&v),
                    wgpu::BindingResource::Sampler(&pipelines.clamp),
                ],
            )
        });
        let init = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        let box_vertices = init(
            "sky box",
            &bytes_of(&SKY_BOX, |(p, c), out| {
                for v in p.iter().chain(c) {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }),
            wgpu::BufferUsages::VERTEX,
        );
        let box_indices = init(
            "sky box",
            &bytes_of(&SKY_BOX_TRIANGLES.concat(), |i, out| {
                out.extend_from_slice(&i.to_le_bytes())
            }),
            wgpu::BufferUsages::INDEX,
        );
        let (dome, triangles) = cloud_dome();
        let dome_vertices = init(
            "cloud dome",
            &bytes_of(&dome, |p, out| {
                for v in p {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }),
            wgpu::BufferUsages::VERTEX,
        );
        let indices = triangles.concat();
        let dome_indices = init(
            "cloud dome",
            &bytes_of(&indices, |i, out| out.extend_from_slice(&i.to_le_bytes())),
            wgpu::BufferUsages::INDEX,
        );
        let mut kept = textures.cubes.to_vec();
        kept.extend(textures.clouds.iter().cloned());
        kept.extend(textures.sun.iter().cloned());
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
            dome_index_count: indices.len() as u32,
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
        let (s, c) = frame.rotation.sin_cos();
        let world = |h: f32, v: f32| -> Mat4 {
            [
                [h * c, 0.0, -h * s, 0.0],
                [0.0, v, 0.0, 0.0],
                [h * s, 0.0, h * c, 0.0],
                [eye[0], eye[1], eye[2], 1.0],
            ]
        };
        let byte = |b: [u8; 4]| b.map(|v| f32::from(v) / 255.0);
        let mut out = Vec::with_capacity(SKY_BYTES as usize);
        push_mat(&mut out, &multiply(&world(1.0, 1.0), view_projection));
        push_mat(&mut out, &multiply(&world(10.0, 0.4), view_projection));
        push_mat(&mut out, view_projection);
        push_vec4(&mut out, byte(frame.color));
        let (directions, color) = frame.clouds.unwrap_or_default();
        push_vec4(&mut out, byte(directions));
        push_vec4(&mut out, byte(color));
        push_vec4(&mut out, [time, 0.0, 0.0, 0.0]);
        let (corners, sun) = frame.sun.unwrap_or_default();
        for p in corners {
            push_vec4(&mut out, [p[0], p[1], p[2], 1.0]);
        }
        push_vec4(&mut out, byte(sun));
        queue.write_buffer(&self.uniforms, 0, &out);
    }

    /// Section 10's step 3: the sky box, the sun sprite, the cloud dome.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass, frame: &SkyFrame, size: (u32, u32)) {
        let (w, h) = (size.0 as f32, size.1 as f32);
        pass.set_bind_group(0, &self.uniform_group, &[]);
        pass.set_viewport(0.0, 0.0, w, h, SKY_DEPTH.0, SKY_DEPTH.1);
        pass.set_pipeline(&self.sky);
        pass.set_bind_group(1, &self.cubes, &[]);
        pass.set_vertex_buffer(0, self.box_vertices.slice(..));
        pass.set_index_buffer(self.box_indices.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..60, 0, 0..1);
        pass.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
        if let (Some(_), Some(sun)) = (frame.sun, &self.sun) {
            pass.set_pipeline(&self.sun_pipeline);
            pass.set_bind_group(1, sun, &[]);
            pass.draw(0..6, 0..1);
        }
        if frame.clouds.is_some() {
            pass.set_viewport(0.0, 0.0, w, h, SKY_DEPTH.0, SKY_DEPTH.1);
            pass.set_pipeline(&self.clouds_pipeline);
            pass.set_bind_group(1, &self.clouds, &[]);
            pass.set_vertex_buffer(0, self.dome_vertices.slice(..));
            pass.set_index_buffer(self.dome_indices.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..self.dome_index_count, 0, 0..1);
            pass.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_box_faces_the_centre() {
        let tex_y: f32 = SKY_BOX.iter().map(|(_, c)| c[1]).sum();
        assert!((tex_y + 4.04).abs() < 1e-5);
        assert_eq!(SKY_BOX.iter().map(|(p, _)| p[1]).sum::<f32>(), 0.0);
        for t in SKY_BOX_TRIANGLES {
            let [a, b, c] = t.map(|i| SKY_BOX[i as usize].0);
            let n = cross(sub(b, a), sub(c, a));
            let centroid = [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0);
            assert!(dot(n, centroid) < 0.0, "{t:?} faces outward");
        }
    }

    /// The interpolated texture coordinate where the ray from the centre
    /// along `d` hits the box (attributes interpolate linearly across each
    /// planar triangle, sky.md 5.6).
    fn lookup(d: [f32; 3]) -> [f32; 3] {
        for t in SKY_BOX_TRIANGLES {
            let [(a, ca), (b, cb), (c, cc)] = t.map(|i| SKY_BOX[i as usize]);
            let (e1, e2) = (sub(b, a), sub(c, a));
            let n = cross(e1, e2);
            let denom = dot(n, d);
            if denom.abs() < 1e-9 {
                continue;
            }
            let k = dot(n, a) / denom;
            if k <= 0.0 {
                continue;
            }
            let p = d.map(|x| x * k);
            // Barycentric coordinates of p.
            let w = sub(p, a);
            let (d00, d01, d11) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
            let (d20, d21) = (dot(w, e1), dot(w, e2));
            let det = d00 * d11 - d01 * d01;
            let v = (d11 * d20 - d01 * d21) / det;
            let u = (d00 * d21 - d01 * d20) / det;
            if v >= -1e-6 && u >= -1e-6 && v + u <= 1.0 + 1e-6 {
                return std::array::from_fn(|i| ca[i] + v * (cb[i] - ca[i]) + u * (cc[i] - ca[i]));
            }
        }
        panic!("{d:?} hits nothing")
    }

    #[test]
    fn sky_box_lookup_follows_section_5_6() {
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
        for (d, c) in [
            // Bottom face: the whole −y face, scaled by 1/1.01.
            ([0.0, -1.0, 0.2], [0.0, -1.01, 0.2]),
            ([0.5, -1.0, -0.9], [0.5, -1.01, -0.9]),
            // Lower side quad: −y border texels.
            ([1.0, -0.5, 0.0], [1.0, -1.005, 0.0]),
            // Horizon.
            ([0.0, 0.0, 1.0], [0.0, -1.0, 1.0]),
            // Upper side quads: the side face stretched; its middle at
            // p.y = 0.5.
            ([0.0, 0.5, 1.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.75, 0.25], [-1.0, 0.5, 0.25]),
            // Top face: a direct lookup.
            ([0.0, 2.0, 1.0], [0.0, 1.0, 0.5]),
        ] {
            let got = lookup(d);
            assert!(close(got, c), "{d:?}: {got:?} != {c:?}");
        }
    }

    #[test]
    fn cloud_dome_matches_the_spec() {
        let (v, t) = cloud_dome();
        assert_eq!((v.len(), t.len()), (91, 160));
        assert_eq!(v.iter().filter(|p| p[1].abs() < 1e-6).count(), 20);
        let sum: f32 = v.iter().map(|p| p[1]).sum();
        assert!((sum - 40.1546).abs() < 1e-3, "{sum}");
        for tri in t {
            let [a, b, c] = tri.map(|i| v[i as usize]);
            assert!(dot(cross(sub(b, a), sub(c, a)), a) < 0.0);
        }
        assert!(module_source("clouds").contains("fn vs_clouds"));
        assert!(!module_source("clouds").contains("fn vs_sun"));
        assert!(module_source("sun").contains("fn fs_sun"));
    }
}
