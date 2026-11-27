// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Particle effects in the world pass (docs/behavior/particle-drawing.md
//! sections 4.4 – 7) on WebGL 2: each
//! effect one batch of quads with its pass, queued where section 7.4 puts
//! it, and the distortion pass (section 6.2). The vertices are written anew
//! each frame into one buffer kept and grown, six a sprite, and drawn
//! without indices.

use std::collections::HashMap;

use crate::Renderer;
use crate::gl::{
    self, Buffer, BufferKind, Consts as C, Depth, Filter, Framebuffer, FrontFace, Gl, Owner,
    RasterState, SamplerDesc, TexFormat, VertexArray, VertexFormat, Wrap,
};
use crate::world::{ALPHA_BLENDING, Mat4, attribute, blend, multiply};

use super::{link, points, units};

pub use crate::shared::particles::*;

const LAYOUT: gl::VertexLayout = gl::VertexLayout {
    stride: VERTEX_BYTES as u32,
    instanced: false,
    attributes: &[
        attribute(0, VertexFormat::Float32x3, 0),
        attribute(1, VertexFormat::Unorm8x4, 12),
        attribute(2, VertexFormat::Float32x2, 16),
    ],
};

/// Bytes of the frame constants.
const FRAME_BYTES: u64 = 96;

fn blend_state(b: ParticleBlend) -> Option<gl::Blend> {
    match b {
        ParticleBlend::Off => None,
        ParticleBlend::Alpha => Some(ALPHA_BLENDING),
        ParticleBlend::Add => Some(blend(C::ONE, C::ONE)),
        ParticleBlend::Mul => Some(blend(C::DST_COLOR, C::ZERO)),
        ParticleBlend::Mul2x => Some(blend(C::DST_COLOR, C::SRC_COLOR)),
        ParticleBlend::AlphaAdd => Some(blend(C::SRC_ALPHA, C::ONE)),
    }
}

/// The states of `key`'s pipeline.
fn raster(key: &PipelineKey) -> RasterState {
    // static-shading.md 10.1's convention, as the world's: a face is
    // front when it appears clockwise on screen.
    let (front_face, cull) = match key.cull {
        ParticleCull::None => (FrontFace::Cw, None),
        ParticleCull::Clockwise => (FrontFace::Ccw, Some(C::BACK)),
        ParticleCull::CounterClockwise => (FrontFace::Cw, Some(C::BACK)),
    };
    RasterState {
        blend: blend_state(key.blend),
        depth: Some(Depth {
            compare: if key.program == ParticleProgram::Flare {
                C::ALWAYS
            } else {
                C::LEQUAL
            },
            write: key.depth_write,
        }),
        cull,
        front_face,
        color_mask: [true; 4],
    }
}

/// The vertex buffer, kept and grown, with its vertex array.
struct Vertices {
    buffer: Buffer,
    array: VertexArray,
}

/// Programs and fixed resources for drawing particles.
pub(crate) struct ParticlePrograms {
    programs: HashMap<ParticleProgram, gl::Program>,
    /// `s_base` clamped and wrapped (anisotropic, linear mip), `s_distort`
    /// (wrap, linear).
    clamp: gl::Sampler,
    wrap: gl::Sampler,
    linear: gl::Sampler,
    /// The glows' texture: clamped, linear minification, magnification
    /// and mip filtering.
    glow: gl::Sampler,
    /// (0, 0, 0, 1): `s_distort` without a texture.
    black: gl::Texture,
    frame: Buffer,
    vertices: Vertices,
}

/// A batch ready to draw.
pub(crate) struct Prepared {
    pub queue: ParticleQueue,
    pub key: f32,
    program: ParticleProgram,
    raster: RasterState,
    base: gl::Texture,
    base_sampler: gl::Sampler,
    distort: gl::Texture,
    vertices: std::ops::Range<u32>,
}

/// The frame's particle draws.
pub(crate) struct PreparedFrame {
    pub draws: Vec<Prepared>,
    /// Some sprite of the distortion list may reach the screen: none can
    /// when every sprite lies wholly outside one plane of the view volume.
    pub distortion_shown: bool,
}

impl PreparedFrame {
    pub fn has(&self, queue: ParticleQueue) -> bool {
        self.draws.iter().any(|d| d.queue == queue)
    }

    /// Draws `draw` into the bound framebuffer.
    pub fn draw(&self, gl: &Gl, p: &ParticlePrograms, draw: &Prepared) {
        let Some(program) = p.programs.get(&draw.program) else {
            return;
        };
        crate::stats::pipeline();
        gl.use_program(program);
        gl.set_raster(&draw.raster);
        crate::stats::bind_group();
        gl.bind_uniforms(points::PARTICLES, &p.frame, 0, FRAME_BYTES);
        crate::stats::bind_group();
        gl.bind_texture(units::OTHER, &draw.base, &draw.base_sampler);
        gl.bind_texture(units::OTHER + 1, &draw.distort, &p.linear);
        crate::stats::buffer();
        gl.bind_vertex_array(&p.vertices.array);
        crate::stats::draw();
        gl.draw_arrays(
            C::TRIANGLES,
            draw.vertices.start,
            draw.vertices.len() as u32,
            1,
        );
    }

    /// Draws every batch of `queue` in queue order.
    pub fn draw_queue(&self, gl: &Gl, p: &ParticlePrograms, queue: ParticleQueue) {
        for d in self.draws.iter().filter(|d| d.queue == queue) {
            self.draw(gl, p, d);
        }
    }
}

/// A vertex buffer of at least `bytes`: the next power of two, at least
/// 1 KiB.
fn vertices(gl: &Gl, bytes: u64) -> Vertices {
    gl.owning(Owner::Particles, || {
        let buffer = gl.buffer(BufferKind::Dynamic, bytes.next_power_of_two().max(1024));
        let array = gl.vertex_array(&[(&buffer, 0, &LAYOUT)], None);
        Vertices { buffer, array }
    })
}

/// Section 5.2: anisotropic minification and magnification (at the value
/// in force, texture-quality-filtering.md 5), linear mip; 5.3: `s_distort`
/// linear.
const LINEAR: SamplerDesc = SamplerDesc {
    mag: Filter::Linear,
    min: Filter::Linear,
    mip: Filter::Linear,
    wrap: [Wrap::Repeat; 3],
    anisotropy: 1,
    top_level: false,
};

impl ParticlePrograms {
    /// `s_base`'s anisotropic samplers at maximum anisotropy `anisotropy`.
    pub(crate) fn set_anisotropy(&mut self, gl: &Gl, anisotropy: u16) {
        self.wrap = gl.sampler(SamplerDesc {
            anisotropy,
            ..LINEAR
        });
        self.clamp = gl.sampler(SamplerDesc {
            wrap: [Wrap::Clamp; 3],
            anisotropy,
            ..LINEAR
        });
    }

    pub(crate) fn new(gl: &Gl) -> Self {
        let program = |fs| {
            link(
                gl,
                "particle",
                "vs_particle",
                fs,
                &[(1, 0), (1, 2)],
                points::PARTICLES,
            )
        };
        let programs = [
            (ParticleProgram::Set, "fs_set"),
            (ParticleProgram::Portal, "fs_portal"),
            (ParticleProgram::Blended, "fs_blended"),
            (ParticleProgram::Plain, "fs_plain"),
            (ParticleProgram::Distort, "fs_distort"),
            (ParticleProgram::Glow, "fs_glow"),
            (ParticleProgram::Flare, "fs_flare"),
        ]
        .into_iter()
        .map(|(p, fs)| (p, program(fs)))
        .collect();
        let linear = LINEAR;
        let black = gl.texture(TexFormat::Rgba8, 1, 1, 1, false);
        gl.write_texture(&black, 0, 0, 0, 0, 1, 1, &mut [0, 0, 0, 255]);
        Self {
            programs,
            wrap: gl.sampler(SamplerDesc {
                anisotropy: 16,
                ..linear
            }),
            clamp: gl.sampler(SamplerDesc {
                wrap: [Wrap::Clamp; 3],
                anisotropy: 16,
                ..linear
            }),
            glow: gl.sampler(SamplerDesc {
                wrap: [Wrap::Clamp; 3],
                ..linear
            }),
            linear: gl.sampler(linear),
            black,
            frame: gl.buffer(BufferKind::Uniform, FRAME_BYTES),
            // Buffers for the first level's effects, made with the
            // renderer: nothing of this is allocated while a level's data
            // fills the heap.
            vertices: vertices(gl, INITIAL_BYTES),
        }
    }

    /// Grows the vertex buffer to hold `particles` sprites.
    pub(crate) fn reserve(&mut self, gl: &Gl, particles: usize) {
        self.grow(
            gl,
            particles as u64 * SPRITE_VERTICES.len() as u64 * VERTEX_BYTES,
        );
    }

    fn grow(&mut self, gl: &Gl, bytes: u64) {
        if self.vertices.buffer.size() < bytes {
            self.vertices = vertices(gl, bytes);
        }
    }

    /// The vertex buffer's size in bytes.
    pub(crate) fn buffer_size(&self) -> u64 {
        self.vertices.buffer.size()
    }

    /// Writes the frame's vertices and constants and makes each batch's
    /// draw.
    pub(crate) fn prepare(
        &mut self,
        gl: &Gl,
        batches: &[ParticleBatch],
        view: &Mat4,
        projection: &Mat4,
        fog: ([f32; 3], f32, f32),
        mip_bias: f32,
    ) -> Option<PreparedFrame> {
        let batches = drawn(batches);
        if batches.is_empty() {
            return None;
        }
        let view_projection = multiply(view, projection);
        gl.write_buffer(
            &self.frame,
            0,
            &frame_constants(&view_projection, fog, mip_bias),
        );
        let mut vertex_bytes = Vec::new();
        let mut draws = Vec::with_capacity(batches.len());
        let distortion_shown = distortion_shown(&batches, &view_projection);
        for b in batches {
            let range = push_sprites(b, &mut vertex_bytes);
            let key = PipelineKey {
                program: b.pass.program,
                blend: b.pass.blend,
                depth_write: b.pass.depth_write,
                cull: b.cull,
            };
            let base_sampler = if matches!(
                b.pass.program,
                ParticleProgram::Glow | ParticleProgram::Flare
            ) {
                &self.glow
            } else if b.pass.clamp {
                &self.clamp
            } else {
                &self.wrap
            };
            let texture = |t: &Option<crate::Texture>| {
                t.as_ref()
                    .map_or_else(|| self.black.clone(), |t| t.texture.clone())
            };
            draws.push(Prepared {
                queue: b.queue,
                key: b.key,
                program: b.pass.program,
                raster: raster(&key),
                base: texture(&b.pass.base),
                base_sampler: base_sampler.clone(),
                distort: texture(&b.pass.distort),
                vertices: range,
            });
        }
        self.grow(gl, vertex_bytes.len() as u64);
        gl.write_buffer(&self.vertices.buffer, 0, &vertex_bytes);
        Some(PreparedFrame {
            draws,
            distortion_shown,
        })
    }

    /// The distortion pass (section 6.2): the distortion list drawn into
    /// the cleared `distortion` framebuffer (of `size`), back to front,
    /// against the scene's depth.
    pub(crate) fn distort(
        &self,
        gl: &Gl,
        frame: &PreparedFrame,
        distortion: &Framebuffer,
        size: (u32, u32),
    ) {
        let grey = 127.0 / 255.0;
        Renderer::begin_pass(gl, distortion, size, Some([grey; 4]), None);
        for d in sorted(frame, ParticleQueue::Distortion) {
            frame.draw(gl, self, d);
        }
        gl.invalidate_depth();
    }
}

/// The draws of `queue` in decreasing key order (back to front), equal
/// keys in queue order (section 7.3).
pub(crate) fn sorted(frame: &PreparedFrame, queue: ParticleQueue) -> Vec<&Prepared> {
    let mut draws: Vec<&Prepared> = frame.draws.iter().filter(|d| d.queue == queue).collect();
    draws.sort_by(|a, b| b.key.total_cmp(&a.key));
    draws
}
