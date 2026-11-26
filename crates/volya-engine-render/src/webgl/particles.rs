// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Particle effects in the world pass on WebGL 2. Until #477 a stand-in
//! keeping the vertex buffer's size and drawing nothing.

use crate::gl::Gl;
use crate::world::Mat4;

pub use crate::shared::particles::*;

/// The particles' programs and buffers (#477).
pub(crate) struct ParticlePrograms {
    bytes: u64,
}

/// A batch ready to draw.
pub(crate) struct Prepared {
    pub queue: ParticleQueue,
    pub key: f32,
}

/// The frame's particle draws.
pub(crate) struct PreparedFrame {
    pub draws: Vec<Prepared>,
    pub distortion_shown: bool,
}

impl PreparedFrame {
    pub fn has(&self, queue: ParticleQueue) -> bool {
        self.draws.iter().any(|d| d.queue == queue)
    }

    pub fn draw(&self, _gl: &Gl, _programs: &ParticlePrograms, _draw: &Prepared) {}

    pub fn draw_queue(&self, _gl: &Gl, _programs: &ParticlePrograms, _queue: ParticleQueue) {}
}

impl ParticlePrograms {
    pub(crate) fn new(_gl: &Gl) -> Self {
        Self {
            bytes: INITIAL_BYTES,
        }
    }

    pub(crate) fn reserve(&mut self, _gl: &Gl, particles: usize) {
        let bytes = particles as u64 * SPRITE_VERTICES.len() as u64 * VERTEX_BYTES;
        if bytes > self.bytes {
            self.bytes = bytes.next_power_of_two().max(1024);
        }
    }

    pub(crate) fn buffer_size(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn prepare(
        &mut self,
        _gl: &Gl,
        _batches: &[ParticleBatch],
        _view: &Mat4,
        _projection: &Mat4,
        _fog: ([f32; 3], f32, f32),
    ) -> Option<PreparedFrame> {
        None
    }

    pub(crate) fn distort(
        &self,
        _gl: &Gl,
        _frame: &PreparedFrame,
        _distortion: &crate::gl::Framebuffer,
        _size: (u32, u32),
    ) {
    }
}
