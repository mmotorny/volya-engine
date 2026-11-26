// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What surrounds the static world (docs/behavior/sky.md) on WebGL 2: the
//! sky box (section 5), the cloud dome (6)
//! and the sun sprite (7.4, 7.5), drawn after the opaque world (section
//! 10).

use crate::Texture;
use crate::gl::{
    self, Buffer, BufferKind, Consts as C, Filter, FrontFace, Gl, RasterState, SamplerDesc,
    VertexArray, VertexFormat, Wrap,
};
use crate::world::{ALPHA_BLENDING, Mat4, attribute, blend, less_equal};

use super::{link, points, units};

pub use crate::shared::sky::*;

const BOX: gl::VertexLayout = gl::VertexLayout {
    stride: 24,
    instanced: false,
    attributes: &[
        attribute(0, VertexFormat::Float32x3, 0),
        attribute(1, VertexFormat::Float32x3, 12),
    ],
};

const DOME: gl::VertexLayout = gl::VertexLayout {
    stride: 12,
    instanced: false,
    attributes: &[attribute(0, VertexFormat::Float32x3, 0)],
};

/// Default culling (static-shading.md 10.1): every sky and cloud triangle
/// faces the centre, front-facing from inside; depth test on
/// (less-or-equal), depth write off.
const fn raster(blend: Option<gl::Blend>) -> RasterState {
    RasterState {
        blend,
        depth: less_equal(false),
        cull: Some(C::BACK),
        front_face: FrontFace::Cw,
        color_mask: [true; 4],
    }
}

/// The sky's programs and samplers, made with the renderer.
pub(crate) struct SkyPrograms {
    sky: gl::Program,
    clouds: gl::Program,
    sun: gl::Program,
    /// Sky: clamp, linear, no mip (5.4); sun: clamp (7.5).
    clamp: gl::Sampler,
    /// Clouds: wrap, anisotropic, linear mip (6.3).
    wrap: gl::Sampler,
    /// The sun's vertices come from their index.
    empty: VertexArray,
}

impl SkyPrograms {
    pub(crate) fn new(gl: &Gl) -> Self {
        let pass = |name: &str| {
            link(
                gl,
                "sky",
                &format!("vs_{name}"),
                &format!("fs_{name}"),
                &[(1, 0), (1, 1)],
                points::SKY,
            )
        };
        Self {
            sky: pass("sky"),
            clouds: pass("clouds"),
            sun: pass("sun"),
            clamp: gl.sampler(SamplerDesc {
                mag: Filter::Linear,
                min: Filter::Linear,
                mip: Filter::Nearest,
                wrap: [Wrap::Clamp; 3],
                anisotropy: 1,
                top_level: true,
            }),
            wrap: gl.sampler(SamplerDesc {
                mag: Filter::Linear,
                min: Filter::Linear,
                mip: Filter::Linear,
                wrap: [Wrap::Repeat; 3],
                anisotropy: 16,
                top_level: false,
            }),
            empty: gl.vertex_array(&[], None),
        }
    }
}

/// The sky's GPU resources for one world.
pub(crate) struct SkyResources {
    uniforms: Buffer,
    /// Per section: its cube map, its cloud texture and its sun sprite's.
    cubes: Vec<gl::Texture>,
    clouds: Vec<gl::Texture>,
    sun: Vec<Option<gl::Texture>>,
    box_array: VertexArray,
    dome_array: VertexArray,
    dome_index_count: u32,
    _buffers: [Buffer; 4],
    textures: Vec<Texture>,
}

impl SkyResources {
    /// Fails when there are no sections or a cube map is not a 6-face
    /// texture.
    pub(crate) fn new(gl: &Gl, textures: &SkyTextures<Texture>) -> Result<Self, String> {
        let sections = &textures.sections;
        if sections.is_empty() {
            return Err("no weather sections".into());
        }
        if sections.iter().any(|s| !s.cube.texture.is_cube()) {
            return Err("sky texture is not a cube map".into());
        }
        let (box_vertices, box_indices) = box_bytes();
        let (dome_vertices, dome_indices, dome_index_count) = dome_bytes();
        let buffers = [
            gl.buffer_with(BufferKind::Vertex, &box_vertices),
            gl.buffer_with(BufferKind::Index, &box_indices),
            gl.buffer_with(BufferKind::Vertex, &dome_vertices),
            gl.buffer_with(BufferKind::Index, &dome_indices),
        ];
        let box_array = gl.vertex_array(&[(&buffers[0], 0, &BOX)], Some(&buffers[1]));
        let dome_array = gl.vertex_array(&[(&buffers[2], 0, &DOME)], Some(&buffers[3]));
        let mut kept: Vec<Texture> = Vec::new();
        for t in sections
            .iter()
            .flat_map(|s| [Some(&s.cube), Some(&s.clouds), s.sun.as_ref()])
            .flatten()
        {
            if !kept.iter().any(|k| k.texture.same(&t.texture)) {
                kept.push(t.clone());
            }
        }
        Ok(Self {
            uniforms: gl.buffer(BufferKind::Uniform, SKY_BYTES),
            cubes: sections.iter().map(|s| s.cube.texture.clone()).collect(),
            clouds: sections.iter().map(|s| s.clouds.texture.clone()).collect(),
            sun: (sections.iter())
                .map(|s| s.sun.as_ref().map(|t| t.texture.clone()))
                .collect(),
            box_array,
            dome_array,
            dome_index_count,
            _buffers: buffers,
            textures: kept,
        })
    }

    pub(crate) fn textures(&self) -> &[Texture] {
        &self.textures
    }

    /// Writes this frame's constants.
    pub(crate) fn write(
        &self,
        gl: &Gl,
        frame: &SkyFrame,
        eye: [f32; 3],
        view_projection: &Mat4,
        time: f32,
    ) {
        gl.write_buffer(
            &self.uniforms,
            0,
            &constants(frame, eye, view_projection, time),
        );
    }

    /// Section 10's step 3: the sky box, the sun sprite, the cloud dome.
    pub(crate) fn draw(&self, gl: &Gl, p: &SkyPrograms, frame: &SkyFrame, size: (u32, u32)) {
        let (w, h) = (size.0 as i32, size.1 as i32);
        let n = self.cubes.len();
        crate::stats::bind_group();
        gl.bind_uniforms(points::SKY, &self.uniforms, 0, SKY_BYTES);
        gl.viewport(0, 0, w, h);
        gl.depth_range(SKY_DEPTH.0, SKY_DEPTH.1);
        crate::stats::pipeline();
        gl.use_program(&p.sky);
        gl.set_raster(&raster(None));
        let a = frame.section % n;
        crate::stats::bind_group();
        gl.bind_texture(units::OTHER, &self.cubes[a], &p.clamp);
        gl.bind_texture(units::OTHER + 1, &self.cubes[(a + 1) % n], &p.clamp);
        crate::stats::buffer();
        gl.bind_vertex_array(&self.box_array);
        crate::stats::draw();
        gl.draw_elements(C::TRIANGLES, 0, 60, false, 1);
        gl.depth_range(0.0, 1.0);
        let sun = self.sun.get(frame.sun_section).and_then(Option::as_ref);
        if let (Some(_), Some(sun)) = (frame.sun, sun) {
            crate::stats::pipeline();
            gl.use_program(&p.sun);
            // effects\sun: source alpha / one (sky.md 7.5).
            gl.set_raster(&raster(Some(blend(C::SRC_ALPHA, C::ONE))));
            crate::stats::bind_group();
            gl.bind_texture(units::OTHER, sun, &p.clamp);
            gl.bind_vertex_array(&p.empty);
            crate::stats::draw();
            gl.draw_arrays(C::TRIANGLES, 0, 6, 1);
        }
        if frame.clouds.is_some() {
            gl.depth_range(SKY_DEPTH.0, SKY_DEPTH.1);
            crate::stats::pipeline();
            gl.use_program(&p.clouds);
            gl.set_raster(&raster(Some(ALPHA_BLENDING)));
            crate::stats::bind_group();
            gl.bind_texture(units::OTHER, &self.clouds[a], &p.wrap);
            gl.bind_texture(units::OTHER + 1, &self.clouds[(a + 1) % n], &p.wrap);
            crate::stats::buffer();
            gl.bind_vertex_array(&self.dome_array);
            crate::stats::draw();
            gl.draw_elements(C::TRIANGLES, 0, self.dome_index_count, false, 1);
            gl.depth_range(0.0, 1.0);
        }
    }
}
