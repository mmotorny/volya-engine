// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The display's gamma ramp (docs/behavior/display-gamma.md): the last
//! step of a presented picture, mapping every channel through the table.

use crate::gl::{self, Buffer, BufferKind, Consts as C, Gl, RasterState, VertexArray};

use super::{LINEAR_CLAMPED, Renderer, Target, link, points, units};

/// Bytes of the ramp's constants: the rectangle and 256 entries.
const CONSTANTS: u64 = 16 + 256 * 4;

/// The ramp's program and constants, made with the renderer.
pub(crate) struct RampPass {
    program: gl::Program,
    constants: Buffer,
    /// As the frame's fit: bilinear, edges clamped.
    sampler: gl::Sampler,
}

impl RampPass {
    pub(crate) fn new(gl: &Gl) -> Self {
        Self {
            program: link(gl, "ramp", "vs_screen", "fs_ramp", &[(0, 1)], points::RAMP),
            constants: gl.buffer(BufferKind::Uniform, CONSTANTS),
            sampler: gl.sampler(LINEAR_CLAMPED),
        }
    }

    /// Draws `frame` into `target` within `rect` (target pixels: x, y,
    /// width, height), black around it, every channel mapped through
    /// `table`.
    pub(crate) fn draw(
        &self,
        gl: &Gl,
        empty: &VertexArray,
        frame: &Target,
        target: &Target,
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
        gl.write_buffer(&self.constants, 0, &bytes);
        Renderer::begin_pass(
            gl,
            &target.framebuffer,
            target.size(),
            Some([0.0, 0.0, 0.0, 1.0]),
            None,
        );
        crate::stats::pipeline();
        gl.use_program(&self.program);
        gl.set_raster(&RasterState::OPAQUE);
        crate::stats::bind_group();
        gl.bind_uniforms(points::RAMP, &self.constants, 0, CONSTANTS);
        gl.bind_texture(units::OTHER, &frame.texture.texture, &self.sampler);
        gl.bind_vertex_array(empty);
        crate::stats::draw();
        gl.draw_arrays(C::TRIANGLES, 0, 3, 1);
    }
}
