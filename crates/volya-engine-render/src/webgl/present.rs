// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Frame presentation (docs/behavior/frame-presentation.md) on WebGL 2: the
//! world drawn into an off-screen target
//! sized by `r__supersample` and reduced to the screen by the presentation
//! quad, through `postprocess` or, after a distortion pass,
//! `postprocess_d`.

use crate::gl::{
    self, Buffer, BufferKind, Consts as C, Framebuffer, Gl, RasterState, SamplerDesc, TexFormat,
    VertexArray, Wrap,
};
use crate::{Distortion, Renderer, Target, Texture};

use super::{LINEAR_CLAMPED, link, points, units};

pub use crate::shared::present::*;

/// The target and what is built with it (1.2): the colour target
/// `$user$rendertarget`, the distortion target `$user$distort` and a depth
/// buffer, all W_t × H_t, in two framebuffers: the scene's, and the
/// distortion's with the scene's depth.
#[derive(Clone)]
pub(crate) struct Targets {
    screen: (u32, u32),
    supersample: u32,
    pub(crate) size: (u32, u32),
    colour: gl::Texture,
    distort: gl::Texture,
    pub(crate) scene: Framebuffer,
    pub(crate) distortion: Framebuffer,
}

/// The presentation's programs, the target and the values in force.
pub(crate) struct PostProcess {
    postprocess: gl::Program,
    postprocess_d: gl::Program,
    constants: Buffer,
    scene_sampler: gl::Sampler,
    noise_sampler: gl::Sampler,
    /// `fx\fx_noise2` once given; white until then.
    noise: gl::Texture,
    /// One texel of the distortion target's clear value (127 / 255).
    cleared: gl::Texture,
    pub(crate) targets: Option<Targets>,
    /// The values last handed over, or the defaults since the target was
    /// built (2.3).
    pub(crate) effects: ScreenEffects,
    /// The noise's countdown T and offsets (3.2).
    countdown: f32,
    offsets: [u32; 2],
    /// The shared generator's state for the noise offsets: not
    /// reproducible in the original, so any start serves.
    random: u32,
    /// The previous frame had a distortion pass (3.1).
    pub(crate) distorted: bool,
    /// The last world frame went through the target.
    pub(crate) used: bool,
}

impl PostProcess {
    pub(crate) fn new(gl: &Gl) -> Self {
        let bindings = [(0, 1), (0, 3), (0, 5)];
        let program = |fs| link(gl, "postprocess", "vs_quad", fs, &bindings, points::POST);
        let noise = gl.texture(TexFormat::Rgba8, 1, 1, 1, false);
        gl.write_texture(&noise, 0, 0, 0, 0, 1, 1, &mut [255; 4]);
        let cleared = gl.texture(TexFormat::Rg8, 1, 1, 1, false);
        gl.write_texture(&cleared, 0, 0, 0, 0, 1, 1, &mut [127; 2]);
        Self {
            postprocess: program("fs_postprocess"),
            postprocess_d: program("fs_postprocess_d"),
            constants: gl.buffer(BufferKind::Uniform, CONSTANTS),
            // s_base0, s_base1: clamped, linear without mip; s_noise
            // wrapped, linear without mip (3.4), its top level only.
            scene_sampler: gl.sampler(LINEAR_CLAMPED),
            noise_sampler: gl.sampler(SamplerDesc {
                wrap: [Wrap::Repeat, Wrap::Repeat, Wrap::Clamp],
                top_level: true,
                ..LINEAR_CLAMPED
            }),
            noise,
            cleared,
            targets: None,
            effects: ScreenEffects::DEFAULTS,
            countdown: 0.0,
            offsets: [0; 2],
            random: 1,
            distorted: false,
            used: false,
        }
    }

    /// The noise texture: its top level only is sampled (3.4).
    pub(crate) fn set_noise(&mut self, texture: &Texture) {
        self.noise = texture.texture.clone();
    }

    /// Builds the target for a `screen` and `supersample` when either
    /// differs from the one built (as the original at device creation and
    /// every reset, 1.2): the values in force return to the defaults.
    pub(crate) fn prepare(&mut self, gl: &Gl, screen: (u32, u32), supersample: u32) {
        let built = self
            .targets
            .as_ref()
            .is_some_and(|t| t.screen == screen && t.supersample == supersample);
        if built || screen.0 == 0 || screen.1 == 0 {
            return;
        }
        let size = target_size(screen.0, screen.1, supersample);
        self.targets = None;
        let make = |format| gl.texture(format, size.0, size.1, 1, false);
        let (colour, distort, depth) = (
            make(TexFormat::Rgba8),
            make(TexFormat::Rg8),
            make(TexFormat::Depth32Float),
        );
        self.targets = Some(Targets {
            screen,
            supersample,
            size,
            scene: gl.framebuffer(Some(&colour), Some(&depth)),
            distortion: gl.framebuffer(Some(&distort), Some(&depth)),
            colour,
            distort,
        });
        self.effects = ScreenEffects::DEFAULTS;
        self.countdown = 0.0;
    }

    /// Whether the frame goes through the target (3.1 step 1).
    pub(crate) fn use_target(&self, supersample: u32) -> bool {
        self.targets.is_some() && (self.effects.active() || supersample > 1 || self.distorted)
    }

    /// One draw of the shared generator.
    fn draw_random(&mut self) -> u32 {
        self.random = self.random.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.random >> 16) & 0x7FFF
    }

    /// The presentation quad (3.1 step 5) into `screen`, with
    /// `postprocess_d` after a distortion pass (reading a texel of the
    /// target's clear value where the pass was not drawn), then `overlay`
    /// (the menus and HUD) in the same pass. Returns whether it drew (a
    /// target was built).
    pub(crate) fn draw(
        &mut self,
        gl: &Gl,
        empty: &VertexArray,
        screen: &Target,
        presentation: &Presentation,
        distortion: Distortion,
        overlay: impl FnOnce(),
    ) -> bool {
        let Some(t) = self.targets.clone() else {
            return false;
        };
        if t.size.1 != t.screen.1 {
            self.effects.blur = 1.0;
        }
        let n = noise_cell(self.effects.noise_grain);
        self.countdown -= presentation.frame_time;
        if self.countdown < 0.0 {
            let ox = self.draw_random() % n;
            let oy = self.draw_random() % n;
            self.offsets = [ox, oy];
            while self.countdown < 0.0 {
                self.countdown += 1.0 / self.effects.noise_fps;
            }
        }
        let corners = quad(
            t.screen,
            t.size,
            &self.effects,
            presentation.pps,
            self.offsets,
        );
        let bytes = quad_constants(t.screen, &corners, &self.effects);
        gl.write_buffer(&self.constants, 0, &bytes);
        let distort = if distortion.drawn {
            &t.distort
        } else {
            &self.cleared
        };
        Renderer::begin_pass(
            gl,
            &screen.framebuffer,
            screen.size(),
            Some([0.0, 0.0, 0.0, 1.0]),
            None,
        );
        crate::stats::pipeline();
        gl.use_program(if distortion.pass {
            &self.postprocess_d
        } else {
            &self.postprocess
        });
        // Pass states (3.4): no depth test or write, no blending.
        gl.set_raster(&RasterState::OPAQUE);
        crate::stats::bind_group();
        gl.bind_uniforms(points::POST, &self.constants, 0, CONSTANTS);
        gl.bind_texture(units::OTHER, &t.colour, &self.scene_sampler);
        gl.bind_texture(units::OTHER + 1, &self.noise, &self.noise_sampler);
        gl.bind_texture(units::OTHER + 2, distort, &self.scene_sampler);
        gl.bind_vertex_array(empty);
        crate::stats::draw();
        gl.draw_arrays(C::TRIANGLE_STRIP, 0, 4, 1);
        overlay();
        true
    }
}
