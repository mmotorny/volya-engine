// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Particle effects over time: an effect instance and its particles
//! (docs/behavior/particle-lifecycle.md, "P2"), what its actions do in a
//! step (particle-motion-actions.md, "P3"; particle-appearance-actions.md,
//! "P4"), groups of effects (particle-groups.md, "P6"), and the effects a
//! level places (level-placed-effects.md, "P8"). The records are
//! `volya_engine_formats::particles`' (particles-xr.md, "P1").
//!
//! Arithmetic is `f32`, rounded per operation in the order the specs write
//! it. Where a spec leaves the precision believed or disputed (the x87
//! parts of P3's conventions and P4 1.2), per-operation `f32` is used too,
//! as P4 1.2 believes the game runs and P3's review note confirms within
//! its checks' tolerances. Square roots, quotients, `sin`, `cos`, `exp`
//! and `ln` are Rust's `f32` functions.
//!
//! What a drawing step reads ([`EffectInstance::particles`]): each
//! particle's position, size, colour, rotation and frame values, with the
//! effect record's sprite and frames.

pub mod effect;
pub mod group;
pub mod placed;
pub mod random;
pub mod region;
pub mod sprite;

mod actions;
#[cfg(test)]
pub(crate) mod fixture;
mod noise;

pub use effect::{EffectInstance, Mode};
pub use group::GroupInstance;
pub use random::Random;

/// A vector (x, y, z).
pub type Vec3 = [f32; 3];
/// A 4 × 4 matrix, row by row, in the row-vector convention of
/// docs/behavior/level-objects.md: a point **p** is placed as [**p** 1] · *M*.
pub type Matrix = [[f32; 4]; 4];

/// Runs one action's step with Δ `dt` on `particles` as an effect alone
/// would (room for `max`), `time` being `turbulence`'s τ: for checks
/// against the specs' per-record vectors.
pub fn apply(
    action: &volya_engine_formats::particles::Action,
    particles: &mut Vec<Particle>,
    max: usize,
    random: &mut Random,
    dt: f32,
    time: &mut f32,
) {
    let mut s = actions::Step {
        particles,
        max,
        effect_flags: 0,
        frame_count: 16,
        random,
        notices: None,
        dt,
    };
    actions::run(&action.kind, action.flags, time, &mut s);
}

/// The identity matrix.
pub const IDENTITY: Matrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// The step time Δ, seconds (P2 section 8.3).
pub const STEP: f32 = 0.033;
/// The step's length in milliseconds (P2 section 8.2).
pub const STEP_MS: u32 = 33;
/// FLT_MIN (P2 conventions).
pub const FLT_MIN: f32 = f32::MIN_POSITIVE;

/// A particle (P2 section 3.1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Particle {
    /// Position **x**.
    pub position: Vec3,
    /// Previous position **x**ₚ.
    pub previous: Vec3,
    /// Velocity **w**ₚ.
    pub velocity: Vec3,
    /// Size **s** (3 components).
    pub size: Vec3,
    /// Rotation **ρ** (3 components).
    pub rotation: Vec3,
    /// Colour as one 32-bit value A · 2²⁴ + R · 2¹⁶ + G · 2⁸ + B.
    pub colour: u32,
    /// Age *a*, seconds.
    pub age: f32,
    /// Frame number (P7).
    pub frame: u16,
    /// Frame flags (P7).
    pub frame_flags: u16,
}

/// An axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    /// The empty box of P6 5.5: minimum +3.4028235 × 10³⁸, maximum
    /// −3.4028235 × 10³⁸ on each axis.
    pub const EMPTY: Bounds = Bounds {
        min: [f32::MAX; 3],
        max: [f32::MIN; 3],
    };

    /// `position` ± 0.001 on each axis (P2 8.2, P6 5.1).
    pub fn around(position: Vec3) -> Self {
        Self {
            min: position.map(|c| c - 0.001),
            max: position.map(|c| c + 0.001),
        }
    }

    /// Not empty: minimum ≤ maximum on all three axes (P6 5.5).
    pub fn is_set(&self) -> bool {
        (0..3).all(|i| self.min[i] <= self.max[i])
    }

    /// Merges `other` when it is not empty.
    pub fn merge(&mut self, other: &Bounds) {
        if other.is_set() {
            for i in 0..3 {
                self.min[i] = self.min[i].min(other.min[i]);
                self.max[i] = self.max[i].max(other.max[i]);
            }
        }
    }

    /// The box's centre and half its diagonal.
    pub fn sphere(&self) -> Sphere {
        let centre = [0, 1, 2].map(|i| (self.min[i] + self.max[i]) * 0.5);
        let d = sub(self.max, self.min);
        Sphere {
            centre,
            radius: dot(d, d).sqrt() * 0.5,
        }
    }
}

/// A bounding sphere.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sphere {
    pub centre: Vec3,
    pub radius: f32,
}

pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn scale(a: Vec3, k: f32) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub(crate) fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// [**p** 1] · *M*.
pub fn place_point(p: Vec3, m: &Matrix) -> Vec3 {
    [0, 1, 2].map(|j| p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + m[3][j])
}

/// [**d** 0] · *M*.
pub fn place_direction(d: Vec3, m: &Matrix) -> Vec3 {
    [0, 1, 2].map(|j| d[0] * m[0][j] + d[1] * m[1][j] + d[2] * m[2][j])
}

/// *M* with row 3 replaced by (0, 0, 0, 1).
pub fn without_translation(m: &Matrix) -> Matrix {
    let mut r = *m;
    r[3] = [0.0, 0.0, 0.0, 1.0];
    r
}

/// The identity with *M*'s row 3 (translation only).
pub fn translation_only(m: &Matrix) -> Matrix {
    let mut r = IDENTITY;
    r[3] = m[3];
    r
}
