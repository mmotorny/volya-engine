// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What surrounds the static world (docs/behavior/sky.md) on WebGL 2.
//! Until #477 a stand-in that keeps the sky's textures and draws nothing.

use crate::Texture;
use crate::gl::Gl;
use crate::world::Mat4;

pub use crate::shared::sky::*;

/// The sky's programs (#477).
pub(crate) struct SkyPrograms;

impl SkyPrograms {
    pub(crate) fn new(_gl: &Gl) -> Self {
        Self
    }
}

/// The sky's GPU resources for one world (#477).
pub(crate) struct SkyResources {
    textures: Vec<Texture>,
}

impl SkyResources {
    pub(crate) fn new(_gl: &Gl, textures: &SkyTextures<Texture>) -> Result<Self, String> {
        Ok(Self {
            textures: (textures.sections.iter())
                .flat_map(|s| [Some(&s.cube), Some(&s.clouds), s.sun.as_ref()])
                .flatten()
                .cloned()
                .collect(),
        })
    }

    pub(crate) fn textures(&self) -> &[Texture] {
        &self.textures
    }

    pub(crate) fn write(
        &self,
        _gl: &Gl,
        _frame: &SkyFrame,
        _eye: [f32; 3],
        _view_projection: &Mat4,
        _time: f32,
    ) {
    }

    pub(crate) fn draw(
        &self,
        _gl: &Gl,
        _programs: &SkyPrograms,
        _frame: &SkyFrame,
        _size: (u32, u32),
    ) {
    }
}
