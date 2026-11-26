// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Detail objects in the world pass (docs/behavior/detail-objects.md
//! section 6) on WebGL 2, as wgpu's renderer draws them: the frame's
//! instances of each model drawn instanced, sway 1 of every model, then
//! sway 2, then still, between the priority 0–1 draws and the sky.
//! Instances come in a vertex buffer stepped per instance whose attributes
//! point at each draw's first (no first instance), the one place
//! attributes change per draw; the models' indices are rebased at upload
//! (no base vertex).

use crate::Texture;
use crate::gl::{
    self, Buffer, BufferKind, Consts as C, Filter, Gl, RasterState, SamplerDesc, VertexArray,
    VertexFormat, Wrap,
};
use crate::world::{Mat4, attribute, less_equal};

use super::{link, points, units};

pub use crate::shared::details::*;

const VERTICES: gl::VertexLayout = gl::VertexLayout {
    stride: VERTEX_BYTES as u32,
    instanced: false,
    attributes: &[
        attribute(0, VertexFormat::Float32x3, 0),
        attribute(1, VertexFormat::Float32x3, 12),
    ],
};

const INSTANCES: gl::VertexLayout = gl::VertexLayout {
    stride: INSTANCE_BYTES as u32,
    instanced: true,
    attributes: &[
        attribute(2, VertexFormat::Float32x4, 0),
        attribute(3, VertexFormat::Float32x4, 16),
    ],
};

/// 6.1, 6.2: both faces, depth test less-or-equal with writes, no
/// blending.
const RASTER: RasterState = RasterState {
    depth: less_equal(true),
    ..RasterState::OPAQUE
};

/// A level's detail models on the GPU.
struct Level {
    vertices: Buffer,
    indices: Buffer,
    instances: Buffer,
    array: VertexArray,
    /// Each model's index range.
    ranges: Vec<std::ops::Range<u32>>,
    texture: gl::Texture,
    /// The alpha reference × 255, −1 for none.
    alpha_ref: f32,
}

/// The programs (made with the renderer) and the level's resources.
pub(crate) struct DetailPrograms {
    frame: Buffer,
    /// Still, sway 1, sway 2.
    programs: [gl::Program; 3],
    sampler: gl::Sampler,
    level: Option<Level>,
}

impl DetailPrograms {
    pub(crate) fn new(gl: &Gl) -> Self {
        let program = |vs| link(gl, "details", vs, "fs_detail", &[(1, 0)], points::DETAILS);
        Self {
            frame: gl.buffer(BufferKind::Uniform, FRAME_BYTES),
            programs: [
                program("vs_still"),
                program("vs_wave1"),
                program("vs_wave2"),
            ],
            // s_base: wrap, anisotropic, linear mip (6.2; the anisotropy of
            // static-shading.md 4.4 with its default).
            sampler: gl.sampler(SamplerDesc {
                mag: Filter::Linear,
                min: Filter::Linear,
                mip: Filter::Linear,
                wrap: [Wrap::Repeat; 3],
                anisotropy: 16,
                top_level: false,
            }),
            level: None,
        }
    }

    /// The level's models, their texture and alpha test, with room for
    /// `capacity` instances a frame.
    pub(crate) fn set_level(
        &mut self,
        gl: &Gl,
        models: &[DetailGeometry],
        texture: &Texture,
        alpha_test: bool,
        capacity: usize,
    ) {
        self.level = None;
        let (vertices, indices, ranges) = level_bytes(models);
        let buffer = |kind, data: &[u8]| {
            let b = gl.buffer(kind, (data.len() as u64).max(4));
            gl.write_buffer(&b, 0, data);
            b
        };
        let vertices = buffer(BufferKind::Vertex, &vertices);
        let indices = buffer(BufferKind::Index, &indices);
        let instances = gl.buffer(
            BufferKind::Dynamic,
            (capacity.max(1) as u64) * INSTANCE_BYTES,
        );
        let array = gl.vertex_array(
            &[(&vertices, 0, &VERTICES), (&instances, 0, &INSTANCES)],
            Some(&indices),
        );
        // A cube map has no 2D view on WebGL 2 (#492): the level's detail
        // texture is a 2D one.
        self.level = Some(Level {
            vertices,
            indices,
            instances,
            array,
            ranges,
            texture: texture.texture.clone(),
            alpha_ref: if alpha_test { 200.0 } else { -1.0 },
        });
    }

    pub(crate) fn clear_level(&mut self) {
        self.level = None;
    }

    /// Bytes of the level's buffers on the GPU.
    pub(crate) fn gpu_bytes(&self) -> u64 {
        self.level.as_ref().map_or(0, |l| {
            l.vertices.size() + l.indices.size() + l.instances.size()
        })
    }

    /// Writes the frame's constants and instances; whether there is
    /// anything to draw.
    pub(crate) fn prepare(
        &self,
        gl: &Gl,
        frame: &DetailFrame,
        view_projection: &Mat4,
        light: [[f32; 3]; 3],
    ) -> bool {
        let Some(level) = &self.level else {
            return false;
        };
        if frame.draws.is_empty() {
            return false;
        }
        let constants = frame_constants(frame, view_projection, light, level.alpha_ref);
        gl.write_buffer(&self.frame, 0, &constants);
        let room = (level.instances.size() / INSTANCE_BYTES) as usize;
        let n = frame.instances.len().min(room);
        gl.write_buffer(
            &level.instances,
            0,
            bytemuck::cast_slice(&frame.instances[..n]),
        );
        true
    }

    /// Draws the frame's lists (6.1) into the bound framebuffer.
    pub(crate) fn draw(&self, gl: &Gl, frame: &DetailFrame) {
        let Some(level) = &self.level else {
            return;
        };
        let room = (level.instances.size() / INSTANCE_BYTES) as u32;
        crate::stats::bind_group();
        gl.bind_uniforms(points::DETAILS, &self.frame, 0, FRAME_BYTES);
        crate::stats::bind_group();
        gl.bind_texture(units::OTHER, &level.texture, &self.sampler);
        crate::stats::buffer();
        gl.bind_vertex_array(&level.array);
        gl.set_raster(&RASTER);
        let mut current = None;
        for d in &frame.draws {
            let Some(range) = level.ranges.get(usize::from(d.model)) else {
                continue;
            };
            let end = (d.first + d.count).min(room);
            if end <= d.first {
                continue;
            }
            let program = match d.list {
                SWAY_1 => 1,
                SWAY_2 => 2,
                _ => 0,
            };
            if current != Some(program) {
                current = Some(program);
                crate::stats::pipeline();
                gl.use_program(&self.programs[program]);
            }
            crate::stats::buffer();
            gl.repoint(
                &level.array,
                &level.instances,
                u64::from(d.first) * INSTANCE_BYTES,
                &INSTANCES,
            );
            crate::stats::draw();
            gl.draw_elements(
                C::TRIANGLES,
                range.start,
                range.len() as u32,
                false,
                end - d.first,
            );
        }
    }
}
