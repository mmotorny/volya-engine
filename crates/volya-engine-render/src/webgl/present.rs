// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Frame presentation (docs/behavior/frame-presentation.md) on WebGL 2.
//! Until #477 a stand-in keeping the target's size.

use crate::Texture;
use crate::gl::Gl;

pub use crate::shared::present::*;

/// The off-screen target (1.2): its size (#477).
pub(crate) struct Targets {
    screen: (u32, u32),
    supersample: u32,
    pub(crate) size: (u32, u32),
}

/// The presentation's programs, the target and the values in force.
pub(crate) struct PostProcess {
    pub(crate) targets: Option<Targets>,
    /// The last world frame went through the target.
    pub(crate) used: bool,
}

impl PostProcess {
    pub(crate) fn new(_gl: &Gl) -> Self {
        Self {
            targets: None,
            used: false,
        }
    }

    pub(crate) fn set_noise(&mut self, _texture: &Texture) {}

    pub(crate) fn prepare(&mut self, _gl: &Gl, screen: (u32, u32), supersample: u32) {
        let built = self
            .targets
            .as_ref()
            .is_some_and(|t| t.screen == screen && t.supersample == supersample);
        if built || screen.0 == 0 || screen.1 == 0 {
            return;
        }
        self.targets = Some(Targets {
            screen,
            supersample,
            size: target_size(screen.0, screen.1, supersample),
        });
    }
}
