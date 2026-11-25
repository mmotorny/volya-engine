// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The collision step of an effect with flag bit 0x10000
//! (docs/behavior/particle-collision.md, "P5"): each live particle's motion
//! of the step tested with a ray against the level's static collision mesh,
//! and the bounce or removal of the particles that hit.
//!
//! The object part of the ray (mask 3, flag 0x80000; P5 3.3) tests no
//! objects: TODO(#388). An instance in local mode is tested with its
//! particles' stored values as world coordinates, as P5's text reads:
//! TODO(#389).

use volya_engine_formats::particles::{Collision, effect_flags};

use super::effect::Notice;
use super::{FLT_MIN, Particle, STEP, Vec3, add, cross, dot, scale, sub};
use crate::collision::CollisionWorld;

/// 10⁻⁵ (the `f32` 0x3727C5AC): the shortest segment tested, and the
/// smallest determinant of a hit (P5 3.2, 4).
const EPSILON: f32 = f32::from_bits(0x3727_C5AC);

/// The ray's query mask (P5 3.1): 2 tests the static mesh, 3 also objects.
pub const MASK_STATIC: u32 = 2;
pub const MASK_OBJECTS: u32 = 3;

/// The mask the flag word `flags` gives (P5 3.1).
pub fn mask(flags: u32) -> u32 {
    if flags & effect_flags::COLLIDE_QUERY != 0 {
        MASK_OBJECTS
    } else {
        MASK_STATIC
    }
}

/// What a ray hit (P5 3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// A triangle of the static mesh (by index) at distance `t`.
    Triangle { index: u32, t: f32 },
    /// An object at distance `t`.
    Object { t: f32 },
}

/// The distance at which the ray from `o` along the unit direction `d`
/// meets triangle `v` from its front side (P5 3.2: Möller–Trumbore with
/// back faces culled), unless it is not above 0.
pub fn front_hit(o: Vec3, d: Vec3, v: &[Vec3; 3]) -> Option<f32> {
    let e1 = sub(v[1], v[0]);
    let e2 = sub(v[2], v[0]);
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.is_nan() || det < EPSILON {
        return None;
    }
    let s = sub(o, v[0]);
    let u = dot(s, p);
    if !(0.0..=det).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let w = dot(d, q);
    if !(w >= 0.0 && u + w <= det) {
        return None;
    }
    let t = dot(e2, q) * (1.0 / det);
    (t > 0.0).then_some(t)
}

/// The ray query (P5 3.1): the nearest hit at a distance below `length`
/// from `o` along the unit direction `d`, with `mask`.
pub fn ray(world: &CollisionWorld, o: Vec3, d: Vec3, length: f32, mask: u32) -> Option<Hit> {
    let hit = world
        .ray_nearest(o, d, length, |i| {
            front_hit(o, d, &world.corners(i)).filter(|&t| t < length)
        })
        .map(|(t, index)| Hit::Triangle { index, t });
    if mask & 0x1 != 0 {
        // TODO(#388): collidable objects with a skeleton collision form,
        // each tested below the nearest distance so far.
    }
    hit
}

/// A triangle's unit front normal as P5 4 step 3 forms it.
pub fn triangle_normal(v: &[Vec3; 3]) -> Vec3 {
    let n = cross(sub(v[1], v[0]), sub(v[2], v[1]));
    let n2 = dot(n, n);
    if n2 > FLT_MIN {
        scale(n, (1.0 / n2).sqrt())
    } else {
        n
    }
}

/// The bounce of P5 4 step 5: the velocity `w` off normal `n`.
pub fn bounce(w: Vec3, n: Vec3, c: &Collision) -> Vec3 {
    let wn = dot(w, n);
    let mut t = sub(w, scale(n, wn));
    // Unless κ ≥ |t|² (so also when either is not a number).
    let t2 = dot(t, t);
    if t2.is_nan() || c.cutoff.is_nan() || c.cutoff < t2 {
        t = scale(t, c.tangential);
    }
    sub(t, scale(scale(n, wn), c.resilience))
}

/// The collision step (P5 2, 4, 5) over the first `count` of `particles`,
/// the live ones, with the effect's flag word `flags` and chunk 33's
/// values `c`; returns the count after removals. Removed particles' slots
/// are left as P5 5 says (the storage keeps its length), and a removal is
/// reported to `notices` first (P2 3.2). Without `world` the rays hit
/// nothing.
pub fn step(
    particles: &mut [Particle],
    count: usize,
    flags: u32,
    c: &Collision,
    world: Option<&CollisionWorld>,
    mut notices: Option<&mut Vec<Notice>>,
) -> usize {
    let mask = mask(flags);
    let destroy = flags & effect_flags::COLLIDE_DESTROY != 0;
    let mut n = count;
    for i in (0..count).rev() {
        let p = &mut particles[i];
        let mut removed = false;
        for _ in 0..2 {
            let delta = sub(p.position, p.previous);
            // The root compared before rounding, then rounded.
            let root = f64::from(dot(delta, delta)).sqrt();
            if root.is_nan() || root < f64::from(EPSILON) {
                p.position = p.previous;
                break;
            }
            let length = root as f32;
            let d = scale(delta, 1.0 / length);
            let Some(hit) = world.and_then(|w| ray(w, p.previous, d, length, mask)) else {
                break;
            };
            let normal = match hit {
                Hit::Triangle { index, .. } => triangle_normal(&world.expect("hit").corners(index)),
                Hit::Object { .. } => [0.0, 1.0, 0.0],
            };
            if destroy {
                removed = true;
                break;
            }
            p.velocity = bounce(p.velocity, normal, c);
            p.position = add(p.previous, scale(p.velocity, STEP));
        }
        if removed {
            if let Some(notices) = notices.as_deref_mut() {
                let p = &particles[i];
                notices.push(Notice::Death {
                    index: i,
                    position: p.position,
                    previous: p.previous,
                });
            }
            n -= 1;
            particles[i] = particles[n];
        }
    }
    n
}

#[cfg(test)]
mod tests;
