// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The billboard groups' distant billboards in the world pass
//! (docs/behavior/static-shading.md 8.4, 10.3) on WebGL 2: the frame's
//! quads, built on the CPU, written into one vertex buffer made with the
//! level for every group it has, drawn without indices: element 4 after the
//! priority 0–1 draws (alpha test 200, depth written), element 1 after the
//! sky (blended, alpha test 8), both faces.

use crate::Texture;
use crate::gl::{
    self, Blend, Buffer, BufferKind, Consts as C, Filter, Gl, RasterState, SamplerDesc,
    VertexArray, VertexFormat, Wrap,
};
use crate::world::{Mat4, WorldLight, attribute, less_equal};

use super::{link, points, units};

pub use crate::shared::billboards::*;

const VERTICES: gl::VertexLayout = gl::VertexLayout {
    stride: VERTEX_BYTES as u32,
    instanced: false,
    attributes: &[
        attribute(0, VertexFormat::Float32x3, 0),
        attribute(1, VertexFormat::Float32x3, 12),
        attribute(2, VertexFormat::Float32x3, 24),
        attribute(3, VertexFormat::Float32x3, 36),
        attribute(4, VertexFormat::Unorm8x4, 48),
        attribute(5, VertexFormat::Float32x2, 52),
        attribute(6, VertexFormat::Float32x2, 60),
        attribute(7, VertexFormat::Unorm8x4, 68),
        attribute(8, VertexFormat::Unorm8x4, 72),
    ],
};

/// Element 4: no blending, depth test less-or-equal with writes, no
/// culling (10.1).
const OPAQUE: RasterState = RasterState {
    depth: less_equal(true),
    ..RasterState::OPAQUE
};

/// Elements 0, 1: source-alpha / inverse-source-alpha, depth write off.
const BLENDED: RasterState = RasterState {
    blend: Some(Blend::ALPHA),
    depth: less_equal(false),
    ..RasterState::OPAQUE
};

/// The script's default sampler: wrap, linear, linear mip.
const SAMPLER: SamplerDesc = SamplerDesc {
    mag: Filter::Linear,
    min: Filter::Linear,
    mip: Filter::Linear,
    wrap: [Wrap::Repeat; 3],
    anisotropy: 1,
    top_level: false,
};

/// The level's billboards on the GPU.
struct Level {
    vertices: Buffer,
    array: VertexArray,
    /// Each set's atlas and its `_nm`.
    sets: Vec<(gl::Texture, gl::Texture)>,
}

/// The programs (made with the renderer) and the level's resources.
pub(crate) struct BillboardPrograms {
    frame: Buffer,
    opaque: gl::Program,
    blended: gl::Program,
    sampler: gl::Sampler,
    level: Option<Level>,
}

impl BillboardPrograms {
    pub(crate) fn new(gl: &Gl) -> Self {
        let bindings = [(1, 0), (1, 1)];
        let program = |fs| {
            link(
                gl,
                "billboards",
                "vs_lod",
                fs,
                &bindings,
                points::BILLBOARDS,
            )
        };
        Self {
            frame: gl.buffer(BufferKind::Uniform, FRAME_BYTES),
            opaque: program("fs_lod_opaque"),
            blended: program("fs_lod_blended"),
            sampler: gl.sampler(SAMPLER),
            level: None,
        }
    }

    /// The level's texture sets (each an atlas and its `_nm`), with room
    /// for `groups` billboards a frame.
    pub(crate) fn set_level(&mut self, gl: &Gl, sets: &[(Texture, Texture)], groups: usize) {
        self.level = None;
        if groups == 0 || sets.is_empty() {
            return;
        }
        let vertices = gl.buffer(
            BufferKind::Dynamic,
            (groups * QUAD_VERTICES) as u64 * VERTEX_BYTES,
        );
        let array = gl.vertex_array(&[(&vertices, 0, &VERTICES)], None);
        self.level = Some(Level {
            vertices,
            array,
            sets: (sets.iter())
                .map(|(a, b)| (a.texture.clone(), b.texture.clone()))
                .collect(),
        });
    }

    pub(crate) fn clear_level(&mut self) {
        self.level = None;
    }

    /// Bytes of the level's buffer on the GPU.
    pub(crate) fn gpu_bytes(&self) -> u64 {
        self.level.as_ref().map_or(0, |l| l.vertices.size())
    }

    /// Writes the frame's constants and vertices; whether there is
    /// anything to draw.
    pub(crate) fn prepare(
        &self,
        gl: &Gl,
        frame: &BillboardFrame,
        view_projection: &Mat4,
        light: &WorldLight,
        mip_bias: f32,
    ) -> bool {
        let Some(level) = &self.level else {
            return false;
        };
        if frame.draws.is_empty() {
            return false;
        }
        gl.write_buffer(
            &self.frame,
            0,
            &frame_constants(view_projection, light, mip_bias),
        );
        let room = (level.vertices.size() / VERTEX_BYTES) as usize;
        let n = frame.vertices.len().min(room);
        gl.write_buffer(
            &level.vertices,
            0,
            bytemuck::cast_slice(&frame.vertices[..n]),
        );
        true
    }

    /// Draws the frame's billboards with element 4 (`opaque`) or element
    /// 1 into the bound framebuffer.
    pub(crate) fn draw(&self, gl: &Gl, frame: &BillboardFrame, opaque: bool) {
        let Some(level) = &self.level else {
            return;
        };
        let room = (level.vertices.size() / VERTEX_BYTES) as u32;
        crate::stats::bind_group();
        gl.bind_uniforms(points::BILLBOARDS, &self.frame, 0, FRAME_BYTES);
        crate::stats::pipeline();
        gl.use_program(if opaque { &self.opaque } else { &self.blended });
        gl.set_raster(if opaque { &OPAQUE } else { &BLENDED });
        crate::stats::buffer();
        gl.bind_vertex_array(&level.array);
        let mut current = None;
        for d in &frame.draws {
            let Some((base, hemi)) = level.sets.get(d.set as usize) else {
                continue;
            };
            let end = (d.first + d.count).min(room);
            if end <= d.first {
                continue;
            }
            if current != Some(d.set) {
                current = Some(d.set);
                crate::stats::bind_group();
                gl.bind_texture(units::OTHER, base, &self.sampler);
                gl.bind_texture(units::OTHER + 1, hemi, &self.sampler);
            }
            crate::stats::draw();
            gl.draw_arrays(C::TRIANGLES, d.first, end - d.first, 1);
        }
    }
}
