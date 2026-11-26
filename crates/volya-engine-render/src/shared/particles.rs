// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Particle effects' platform-free part, both renderers'
//! (docs/behavior/particle-drawing.md sections 4.4 – 7): the batches, their
//! passes and queues, and the frame's vertices and constants as written.

use std::ops::Range;

use crate::Texture;
use crate::world::{Mat4, fog_plane, push_mat};

/// A pass's blending (section 5.2; source / destination factors).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParticleBlend {
    /// `SET`: off.
    Off,
    /// `BLEND`: source alpha / inverse source alpha.
    Alpha,
    /// `ADD`: one / one.
    Add,
    /// `MUL`: destination colour / zero.
    Mul,
    /// `MUL_2X`: destination colour / source colour.
    Mul2x,
    /// `ALPHA-ADD`: source alpha / one.
    AlphaAdd,
}

/// The pixel program and its fixed states (sections 5.2 – 5.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParticleProgram {
    /// `particle`, alpha test A · 255 > 200, fog on (`SET`).
    Set,
    /// `particle`, alpha test A · 255 > 0, no fog.
    Blended,
    /// `particle`, no alpha test, no fog (the scripted shaders).
    Plain,
    /// `particle_distort`, no alpha test, no fog.
    Distort,
    /// A glow's `S_SET   ` pass of `effects\glow` (glows.md 6,
    /// static-shading.md 7.5): colour = texel × vertex colour, alpha test
    /// A · 255 > 0, no fog, depth test on, texture clamped with linear
    /// filtering.
    Glow,
    /// `effects\flare`: alpha test A · 255 > 2, depth test off.
    Flare,
}

/// Face culling during the effect's draw (section 4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParticleCull {
    None,
    /// Triangles appearing clockwise on screen are culled.
    Clockwise,
    /// Counter-clockwise ones.
    CounterClockwise,
}

/// Where an effect is drawn in the frame (section 7.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleQueue {
    /// With the non-sorted passes of priorities 0 – 1 (step 1).
    Early,
    /// With the non-sorted passes of priorities 2 – 3 (step 4).
    Late,
    /// In the strict-sorted list, back to front (step 5).
    Strict,
    /// In the distortion list, back to front (step 7).
    Distortion,
    /// The glows, after the strict-sorted passes (glows.md section 4).
    Glow,
}

/// One pass of an effect's shader.
#[derive(Clone, Debug)]
pub struct ParticlePass {
    pub program: ParticleProgram,
    pub blend: ParticleBlend,
    pub depth_write: bool,
    /// `s_base` clamps (`Texture clamp`); otherwise it wraps.
    pub clamp: bool,
    /// `s_base`.
    pub base: Option<Texture>,
    /// `s_distort`; `None` samples (0, 0, 0, 1) (believed, section 5.3;
    /// TODO(#329)).
    pub distort: Option<Texture>,
}

/// A quad corner: world position, colour (A, R, G, B from the top byte),
/// texture coordinates (section 4.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleVertex {
    pub position: [f32; 3],
    pub colour: u32,
    pub uv: [f32; 2],
}

/// An effect's sprites with one of its passes.
#[derive(Clone, Debug)]
pub struct ParticleBatch {
    pub pass: ParticlePass,
    pub queue: ParticleQueue,
    pub cull: ParticleCull,
    /// |**E** − **C**′|² + 10⁻⁵ (section 7.2).
    pub key: f32,
    /// Four per particle, in the particle list's order.
    pub vertices: Vec<ParticleVertex>,
}

/// Bytes of one vertex: position, colour, uv (24, section 4.4).
pub(crate) const VERTEX_BYTES: u64 = 24;

/// Vertices of a sprite: triangles (0, 1, 2) and (3, 2, 1) of its four
/// corners.
pub(crate) const SPRITE_VERTICES: [usize; 6] = [0, 1, 2, 3, 2, 1];

/// The vertex buffer's first size: 14,563 particles' vertices, more than
/// `l01_escape`'s effect objects can hold at once (8,122). Made
/// with the renderer: buffers made with a level's world stay allocated amid
/// the level freed after it and split the wasm heap.
pub(crate) const INITIAL_BYTES: u64 = 2 * 1024 * 1024;

/// The passes of the effects' shaders (program, blending, depth write):
/// class `PARTICLE`'s six blendings (section 5.2) and the scripted
/// shaders' normal and distortion passes (5.3).
pub(crate) const PASSES: &[(ParticleProgram, ParticleBlend, bool)] = &[
    (ParticleProgram::Set, ParticleBlend::Off, true),
    (ParticleProgram::Blended, ParticleBlend::Alpha, false),
    (ParticleProgram::Blended, ParticleBlend::Add, false),
    (ParticleProgram::Blended, ParticleBlend::Mul, false),
    (ParticleProgram::Blended, ParticleBlend::Mul2x, false),
    (ParticleProgram::Blended, ParticleBlend::AlphaAdd, false),
    (ParticleProgram::Plain, ParticleBlend::Add, false),
    (ParticleProgram::Distort, ParticleBlend::Alpha, false),
    (ParticleProgram::Glow, ParticleBlend::AlphaAdd, false),
    (ParticleProgram::Flare, ParticleBlend::AlphaAdd, false),
];

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PipelineKey {
    pub(crate) program: ParticleProgram,
    pub(crate) blend: ParticleBlend,
    pub(crate) depth_write: bool,
    pub(crate) cull: ParticleCull,
}

/// Whether the sprite of `corners` lies wholly outside one plane of the
/// view volume of `view_projection` (row vectors; depth 0 … w), by a
/// margin, so it draws no pixel.
pub(crate) fn outside_view(view_projection: &Mat4, corners: &[ParticleVertex]) -> bool {
    let clip: Vec<[f32; 4]> = corners
        .iter()
        .map(|v| {
            let p = [v.position[0], v.position[1], v.position[2], 1.0];
            std::array::from_fn(|i| (0..4).map(|k| p[k] * view_projection[k][i]).sum())
        })
        .collect();
    let margin = |c: &[f32; 4]| 1e-3 * c[3].abs() + 1e-6;
    let all = |f: &dyn Fn(&[f32; 4]) -> bool| clip.iter().all(f);
    all(&|c| c[0] > c[3] + margin(c))
        || all(&|c| c[0] < -c[3] - margin(c))
        || all(&|c| c[1] > c[3] + margin(c))
        || all(&|c| c[1] < -c[3] - margin(c))
        || all(&|c| c[2] > c[3] + margin(c))
        || all(&|c| c[2] < -margin(c))
}

/// The batches drawn: those with sprites and, but for a distortion pass,
/// a texture (a normal pass without one is left out, reported where it
/// failed to load).
pub(crate) fn drawn(batches: &[ParticleBatch]) -> Vec<&ParticleBatch> {
    batches
        .iter()
        .filter(|b| {
            let textured = b.pass.base.is_some() || b.pass.program == ParticleProgram::Distort;
            !b.vertices.is_empty() && textured
        })
        .collect()
}

/// The frame's constants (96 bytes): the view-projection, the fog plane
/// and the fog colour (`fog`: colour, near, far).
pub(crate) fn frame_constants(view_projection: &Mat4, fog: ([f32; 3], f32, f32)) -> Vec<u8> {
    let mut constants = Vec::with_capacity(96);
    push_mat(&mut constants, view_projection);
    for v in fog_plane(view_projection, fog.1, fog.2) {
        constants.extend_from_slice(&v.to_le_bytes());
    }
    for v in [fog.0[0], fog.0[1], fog.0[2], 0.0] {
        constants.extend_from_slice(&v.to_le_bytes());
    }
    constants
}

/// Whether some sprite of the distortion list may reach the screen: none
/// can when every sprite lies wholly outside one plane of the view volume.
pub(crate) fn distortion_shown(batches: &[&ParticleBatch], view_projection: &Mat4) -> bool {
    batches.iter().any(|b| {
        b.queue == ParticleQueue::Distortion
            && b.vertices
                .chunks(4)
                .any(|q| !outside_view(view_projection, q))
    })
}

/// Appends the vertices of `b`'s sprites to `out`, six a sprite; the
/// vertex range they take.
pub(crate) fn push_sprites(b: &ParticleBatch, out: &mut Vec<u8>) -> Range<u32> {
    let first = (out.len() as u64 / VERTEX_BYTES) as u32;
    for q in b.vertices.as_chunks::<4>().0 {
        for v in SPRITE_VERTICES.map(|i| q[i]) {
            for c in v.position {
                out.extend_from_slice(&c.to_le_bytes());
            }
            // A, R, G, B from the top byte to (R, G, B, A) bytes.
            let [a, r, g, bl] = v.colour.to_be_bytes();
            out.extend_from_slice(&[r, g, bl, a]);
            for c in v.uv {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
    }
    first..(out.len() as u64 / VERTEX_BYTES) as u32
}
