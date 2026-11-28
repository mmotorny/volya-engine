// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the contacts of movable bodies are made of, apart from the solver
//! (docs/behavior/physics-contacts.md, cited by section): the collision
//! classes and their filter (1.3), the contact properties of two materials
//! (2.2), the shapes' bounds (1.1, 4.1) and supports (3.5) and the
//! contacts with the level's mesh ([`level`], section 3). The solver
//! gathers them (`ode::collide`). Cylinders get no contacts yet
//! (TODO(#1010)).

pub mod level;
#[cfg(test)]
mod tests;

use volya_engine_formats::gamemtl::{BOUNCY, GameMaterial};

use crate::collision::body::{WORLD_DAMPING, WORLD_SPRING};
use crate::collision::geometry::{Vec3, add, dot, sub};
use crate::physics::shell::{Shape, Shell};

/// The class word's bits (1.3).
pub mod class {
    /// Has a group number.
    pub const GROUP: u32 = 0x1;
    pub const IGNORE_STATIC: u32 = 0x2;
    pub const DYNAMIC: u32 = 0x4;
    pub const IGNORE_DYNAMIC: u32 = 0x8;
    pub const CHARACTER: u32 = 0x10;
    pub const IGNORE_CHARACTERS: u32 = 0x20;
    /// Written, never read (1.3).
    pub const SMALL: u32 = 0x40;
    pub const IGNORE_SMALL: u32 = 0x80;
    pub const RAGDOLL: u32 = 0x100;
    pub const IGNORE_RAGDOLL: u32 = 0x200;
}

/// An object's collision class (1.3): its class word and group number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Class {
    pub word: u32,
    pub group: u32,
}

impl Class {
    /// A character's body when made (1.3, 5).
    pub const CHARACTER: Self = Self {
        word: class::DYNAMIC | class::CHARACTER,
        group: 0,
    };

    /// A shell's when made, in single play (1.3): dynamic, `ignore_static`
    /// (set only with fixed bones, physics-shells.md 5.2), its group number,
    /// the small-object and ragdoll marks. Only death shells get "ignore
    /// characters once at rest" (1.3), and there are none: bit 0x20 is
    /// never set (7.1: the door at rest blocks the player's body).
    pub fn shell(shell: &Shell) -> Self {
        let c = shell.collision_class;
        let mut word = class::DYNAMIC;
        let bit = |on: bool, b: u32| if on { b } else { 0 };
        word |= bit(shell.ignore_static, class::IGNORE_STATIC);
        word |= bit(c.small_object, class::SMALL);
        word |= bit(c.ignore_small_objects, class::IGNORE_SMALL);
        word |= bit(c.ignore_ragdoll, class::IGNORE_RAGDOLL);
        let group = match shell.collision_group {
            Some(g) => {
                word |= class::GROUP;
                g
            }
            None => 0,
        };
        Self { word, group }
    }

    pub fn ignores_static(self) -> bool {
        self.word & class::IGNORE_STATIC != 0
    }

    /// The filter (1.3): whether the pair is not collided.
    pub fn excludes(self, other: Self) -> bool {
        let (a, b) = (self.word, other.word);
        if a & b & class::GROUP != 0 {
            return self.group == other.group;
        }
        ((b & 0x114) << 1) & a & 0x228 != 0 || ((a & 0x114) << 1) & b & 0x228 != 0
    }
}

/// ODE's contact surface modes (public 0.5.0's `contact.h`).
pub mod mode {
    pub const BOUNCE: i32 = 0x004;
    pub const SOFT_ERP: i32 = 0x008;
    pub const SOFT_CFM: i32 = 0x010;
    /// The friction pyramid in both directions (`dContactApprox1`).
    pub const APPROX1: i32 = 0x3000;
}

/// A contact's properties (2.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Properties {
    /// The surface modes ([`mode`]).
    pub mode: i32,
    pub mu: f32,
    pub erp: f32,
    pub cfm: f32,
    /// The bounce and its threshold speed: the unused fields hold 0.01 and
    /// 1.5 without the bounce mode.
    pub bounce: f32,
    pub bounce_speed: f32,
}

/// 2.2: the properties of a contact of materials `a` and `b` at step `h`.
pub fn properties(a: &GameMaterial, b: &GameMaterial, h: f32) -> Properties {
    let k = h * a.spring * b.spring * WORLD_SPRING;
    let d = a.damping * b.damping * WORLD_DAMPING;
    let mut p = Properties {
        mode: mode::SOFT_ERP | mode::SOFT_CFM | mode::APPROX1,
        mu: a.friction * b.friction,
        erp: k / (k + d),
        cfm: 1.0 / (k + d),
        bounce: 0.01,
        bounce_speed: 1.5,
    };
    if a.has(BOUNCY) && b.has(BOUNCY) {
        p.mode |= mode::BOUNCE;
        p.bounce = a.bounce.min(b.bounce);
        p.bounce_speed = a.bounce_start_speed.max(b.bounce_start_speed);
    }
    p
}

/// A world-space box: its least and greatest corner.
pub type Bounds = (Vec3, Vec3);

pub fn overlap(a: &Bounds, b: &Bounds) -> bool {
    (0..3).all(|k| a.0[k] <= b.1[k] && b.0[k] <= a.1[k])
}

/// The union of two boxes.
pub fn union(a: &Bounds, b: &Bounds) -> Bounds {
    (
        std::array::from_fn(|k| a.0[k].min(b.0[k])),
        std::array::from_fn(|k| a.1[k].max(b.1[k])),
    )
}

/// A world-space shape's half extent per world axis (1.1, 4.1; the
/// cylinder's, `r·√(R_i0² + R_i2²) + 0.5·|R_i1·l|`, with its axis column 1).
pub fn reach(shape: &Shape) -> Vec3 {
    match *shape {
        Shape::Box {
            axes, half_size, ..
        } => std::array::from_fn(|i| (0..3).map(|k| (axes[k][i] * half_size[k]).abs()).sum()),
        Shape::Sphere { radius, .. } => [radius; 3],
        Shape::Cylinder {
            direction,
            height,
            radius,
            ..
        } => std::array::from_fn(|i| {
            let d = direction[i];
            radius * (1.0 - d * d).max(0.0).sqrt() + 0.5 * (d * height).abs()
        }),
    }
}

pub fn bounds(shape: &Shape) -> Bounds {
    let (c, e) = (shape.centre(), reach(shape));
    (sub(c, e), add(c, e))
}

/// 3.5: a shape's support along the unit `n`: how far it reaches from its
/// centre.
pub fn support(shape: &Shape, n: Vec3) -> f32 {
    match *shape {
        Shape::Sphere { radius, .. } => radius,
        Shape::Box {
            axes, half_size, ..
        } => (0..3).map(|k| half_size[k] * dot(axes[k], n).abs()).sum(),
        Shape::Cylinder {
            direction,
            height,
            radius,
            ..
        } => {
            let c = dot(direction, n).abs().min(1.0);
            0.5 * height * c + radius * (1.0 - c * c).sqrt()
        }
    }
}

/// A contact as a collider gives it: its normal points from the second
/// geometry towards the first (the library's convention).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawContact {
    pub position: Vec3,
    pub normal: Vec3,
    pub depth: f32,
}
