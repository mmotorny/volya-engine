// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Detail objects in the world pass on WebGL 2. Until #477 a stand-in
//! drawing nothing.

use crate::Texture;
use crate::gl::Gl;
use crate::world::Mat4;

pub use crate::shared::details::*;

/// The detail objects' programs and the level's models (#477).
pub(crate) struct DetailPrograms;

impl DetailPrograms {
    pub(crate) fn new(_gl: &Gl) -> Self {
        Self
    }

    pub(crate) fn set_level(
        &mut self,
        _gl: &Gl,
        _models: &[DetailGeometry],
        _texture: &Texture,
        _alpha_test: bool,
        _capacity: usize,
    ) {
    }

    pub(crate) fn clear_level(&mut self) {}

    pub(crate) fn gpu_bytes(&self) -> u64 {
        0
    }

    pub(crate) fn prepare(
        &self,
        _gl: &Gl,
        _frame: &DetailFrame,
        _view_projection: &Mat4,
        _light: [[f32; 3]; 3],
    ) -> bool {
        false
    }

    pub(crate) fn draw(&self, _gl: &Gl, _frame: &DetailFrame) {}
}
