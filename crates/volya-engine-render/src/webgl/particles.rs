// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Particle effects in the world pass on WebGL 2. Until #477 a stand-in
//! keeping the vertex buffer's size.

use crate::gl::Gl;

pub use crate::shared::particles::*;

/// The particles' programs and buffers (#477).
pub(crate) struct ParticlePrograms {
    bytes: u64,
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
}
