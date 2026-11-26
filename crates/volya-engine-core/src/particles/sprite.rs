// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The sprites of an effect's particles (particle-drawing.md sections 3.1
//! and 4): one quad per live particle, in list order, built from the
//! particles' current values and the camera.

use volya_engine_formats::particles::{EffectRecord, effect_flags as f};

use super::actions::FRAME_UNIT;
use super::{
    EffectInstance, FLT_MIN, Matrix, Particle, Vec3, add, cross, dot, place_direction, place_point,
    scale, sub,
};

/// The camera a frame is drawn with (P7 4.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// **E**.
    pub eye: Vec3,
    /// **F**, unit.
    pub forward: Vec3,
    /// **U** as the view gives it, unit.
    pub up: Vec3,
}

impl Camera {
    /// **F**, **U**, **R**: **R** = **U** × **F**, then **U** = **F** × **R**.
    fn axes(&self) -> (Vec3, Vec3, Vec3) {
        let r = cross(self.up, self.forward);
        let u = cross(self.forward, r);
        (self.forward, u, r)
    }
}

/// A quad's vertex: world position, the particle's colour (A in the top
/// byte, then R, G, B) and texture coordinates (P7 4.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteVertex {
    pub position: Vec3,
    pub colour: u32,
    pub uv: [f32; 2],
}

/// The speed below which a particle is at rest (P7 4.3).
const AT_REST: f32 = 1.0e-7;

/// The texture rectangle of a particle (P7 3.1): (*u*₀, *v*₀, *u*₁, *v*₁).
pub fn frame_rect(effect: EffectRecord<'_>, frame: u16) -> [f32; 4] {
    if effect.flags & f::FRAMES == 0 {
        return [0.0, 0.0, 1.0, 1.0];
    }
    let fr = &effect.frames;
    let i = (f32::from(frame) * FRAME_UNIT).floor() as i64;
    // Frames per row of 0 or less (none in the data) take one per row (our
    // choice: the spec divides by it).
    let per_row = i64::from(fr.per_row.max(1));
    let u0 = (i.rem_euclid(per_row)) as f32 * fr.width;
    let v0 = (i.div_euclid(per_row)) as f32 * fr.height;
    [u0, v0, u0 + fr.width, v0 + fr.height]
}

/// *Q*(**a**)'s rows 0 and 2 (P7 4.3).
fn q_rows(a: Vec3) -> (Vec3, Vec3) {
    let (s0, c0) = (a[0].sin(), a[0].cos());
    let (s1, c1) = (a[1].sin(), a[1].cos());
    let (s2, c2) = (a[2].sin(), a[2].cos());
    let row0 = [c2 * c1 - s2 * s1 * s0, -s2 * c0, s2 * c1 * s0 + c2 * s1];
    let row2 = [-c0 * s1, s0, c0 * c1];
    (row0, row2)
}

/// **d**(**a**), computed as (−sin(−*a*ᵧ) · cos(−*a*ₓ), sin(−*a*ₓ),
/// cos(−*a*ᵧ) · cos(−*a*ₓ)).
fn axis_of(a: Vec3) -> Vec3 {
    let (x, y) = (-a[0], -a[1]);
    [-y.sin() * x.cos(), x.sin(), y.cos() * x.cos()]
}

/// **D** × **F**, divided by its length only when its squared length is
/// above FLT_MIN.
fn across_view(d: Vec3, forward: Vec3) -> Vec3 {
    let s = cross(d, forward);
    let l2 = dot(s, s);
    if l2 > FLT_MIN {
        let l = l2.sqrt();
        s.map(|c| c / l)
    } else {
        s
    }
}

/// **a** · (1/√(**a** · **a**)), with no guard.
fn unit(a: Vec3) -> Vec3 {
    scale(a, 1.0 / dot(a, a).sqrt())
}

/// The sprite's up **T** and right **S** (P7 4.3); `local` maps the
/// fixed, axis and across cases' vectors.
fn orientation(
    effect: EffectRecord<'_>,
    p: &Particle,
    speed: f32,
    (forward, up, right): (Vec3, Vec3, Vec3),
    local: Option<&Matrix>,
) -> (Vec3, Vec3) {
    let map = |d: Vec3| local.map_or(d, |m| place_direction(d, m));
    let flags = effect.flags;
    if flags & f::ALIGN_TO_PATH == 0 {
        return (up, right);
    }
    let a = effect.alignment;
    if speed < AT_REST {
        if flags & f::DEFAULT_WORLD_ALIGN != 0 {
            let (row0, row2) = q_rows(a);
            (map(row2), map(row0))
        } else {
            let d = map(axis_of(a));
            (d, across_view(d, forward))
        }
    } else {
        let k = scale(p.velocity, 1.0 / speed);
        if flags & f::FACE_ALIGN != 0 {
            let e = if k[1].abs() <= 0.99 {
                [0.0, 1.0, 0.0]
            } else {
                [0.0, 0.0, 1.0]
            };
            let s = unit(cross(e, k));
            let t = unit(cross(k, s));
            (map(t), map(s))
        } else {
            let d = map(k);
            (d, across_view(d, forward))
        }
    }
}

/// Appends the four vertices of each live particle of `instance` (its
/// record `effect`) seen by `camera`, in list order (P7 4.4). An effect
/// without a sprite draws nothing (section 2).
pub fn quads(
    effect: EffectRecord<'_>,
    instance: &EffectInstance,
    camera: &Camera,
    out: &mut Vec<SpriteVertex>,
) {
    if effect.flags & f::SPRITE == 0 {
        return;
    }
    let axes = camera.axes();
    let local = instance.local();
    for p in instance.particles() {
        let [u0, v0, u1, v1] = frame_rect(effect, p.frame);
        let speed = dot(p.velocity, p.velocity).sqrt();
        let mut w = p.size[0] * 0.5;
        let mut h = p.size[1] * 0.5;
        if effect.flags & f::VELOCITY_SCALE != 0 {
            w += effect.velocity_scale[0] * speed;
            h += effect.velocity_scale[1] * speed;
        }
        let (t, s) = orientation(effect, p, speed, axes, local);
        let x = local.map_or(p.position, |m| place_point(p.position, m));
        let alpha = p.rotation[0];
        let (sin, cos) = (alpha.sin(), alpha.cos());
        let a = scale(add(scale(s, cos), scale(t, sin)), w);
        let b = scale(sub(scale(t, cos), scale(s, sin)), h);
        let c = p.colour;
        for (position, uv) in [
            (sub(sub(x, a), b), [u0, v1]),
            (add(sub(x, a), b), [u0, v0]),
            (sub(add(x, a), b), [u1, v1]),
            (add(add(x, a), b), [u1, v0]),
        ] {
            out.push(SpriteVertex {
                position,
                colour: c,
                uv,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{effect, muhi};
    use super::*;

    const CAMERA: Camera = Camera {
        eye: [0.0; 3],
        forward: [0.0, 0.0, 1.0],
        up: [0.0, 1.0, 0.0],
    };

    fn close(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
    }

    /// One particle drawn by an effect with `flags`.
    fn draw(flags: u32, p: Particle, velocity_scale: Vec3, alignment: Vec3) -> Vec<SpriteVertex> {
        let mut record = effect("e", 1, flags, 0.0, Vec::new());
        record.velocity_scale = velocity_scale;
        record.alignment = alignment;
        let mut instance = EffectInstance::new(0, record.record());
        instance.particles_mut().push(p);
        let mut out = Vec::new();
        quads(record.record(), &instance, &CAMERA, &mut out);
        out
    }

    fn particle(x: Vec3, s: Vec3, rho: f32, w: Vec3) -> Particle {
        Particle {
            position: x,
            size: s,
            rotation: [rho, 0.0, 0.0],
            velocity: w,
            ..Particle::default()
        }
    }

    #[test]
    fn a_facing_sprite_is_p7s_vector_1() {
        let v = draw(
            0x1,
            particle([1.0, 2.0, 10.0], [2.0, 1.0, 1.0], 0.0, [0.0; 3]),
            [0.0; 3],
            [0.0; 3],
        );
        let want = [
            ([0.0, 1.5, 10.0], [0.0, 1.0]),
            ([0.0, 2.5, 10.0], [0.0, 0.0]),
            ([2.0, 1.5, 10.0], [1.0, 1.0]),
            ([2.0, 2.5, 10.0], [1.0, 0.0]),
        ];
        for (got, (x, uv)) in v.iter().zip(want) {
            assert!(close(got.position, x), "{got:?}");
            assert_eq!(got.uv, uv);
        }
    }

    #[test]
    fn a_turned_sprite_is_p7s_vector_2() {
        let v = draw(
            0x1,
            particle(
                [1.0, 2.0, 10.0],
                [2.0, 1.0, 1.0],
                std::f32::consts::FRAC_PI_2,
                [0.0; 3],
            ),
            [0.0; 3],
            [0.0; 3],
        );
        assert!(close(v[0].position, [1.5, 1.0, 10.0]), "{:?}", v[0]);
        assert!(close(v[3].position, [0.5, 3.0, 10.0]), "{:?}", v[3]);
    }

    #[test]
    fn frames_pick_p7s_rectangle() {
        // Vector 3: frame 765, 2 per row, 0.5 × 0.5.
        let mut record = muhi();
        record.flags = 0x401;
        assert_eq!(frame_rect(record.record(), 765), [0.5, 0.5, 1.0, 1.0]);
        record.flags = 0x1;
        assert_eq!(frame_rect(record.record(), 765), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn along_motion_stretches_as_p7s_vector_5() {
        let v = draw(
            0x1 | 0x8000 | 0x40000,
            particle([0.0, 0.0, 10.0], [1.0; 3], 0.0, [0.0, 3.0, 0.0]),
            [0.0, 0.5, 0.0],
            [0.0; 3],
        );
        let want = [
            [-0.5, -2.0, 10.0],
            [-0.5, 2.0, 10.0],
            [0.5, -2.0, 10.0],
            [0.5, 2.0, 10.0],
        ];
        for (got, x) in v.iter().zip(want) {
            assert!(close(got.position, x), "{got:?}");
        }
    }

    /// **T** and **S** of a particle of an effect with `flags`.
    fn ts(flags: u32, w: Vec3, a: Vec3) -> (Vec3, Vec3) {
        let mut record = effect("e", 1, flags, 0.0, Vec::new());
        record.alignment = a;
        let p = particle([0.0; 3], [1.0; 3], 0.0, w);
        let speed = dot(w, w).sqrt();
        orientation(record.record(), &p, speed, CAMERA.axes(), None)
    }

    #[test]
    fn alignment_cases_are_p7s_vectors_6_to_8() {
        // 6: across motion.
        let (t, s) = ts(0x1 | 0x8000 | 0x200000, [0.0, 0.0, 2.0], [0.0; 3]);
        assert!(
            close(s, [1.0, 0.0, 0.0]) && close(t, [0.0, 1.0, 0.0]),
            "{t:?} {s:?}"
        );
        // 7: fixed at rest, and the axis at rest.
        let a = [-1.570_796_4, 0.0, 0.0];
        let (t, s) = ts(0x1 | 0x8000 | 0x100000, [0.0; 3], a);
        assert!(close(t, [0.0, -1.0, -4.4e-8]) && close(s, [1.0, 0.0, 0.0]));
        let (t, _) = ts(0x1 | 0x8000, [0.0; 3], a);
        assert!(close(t, [0.0, 1.0, -4.4e-8]), "{t:?}");
        // 8: the axis along the view degenerates.
        let (t, s) = ts(0x1 | 0x8000, [0.0, 0.0, 5.0], [0.0; 3]);
        assert_eq!((t, s), ([0.0, 0.0, 1.0], [0.0; 3]));
    }

    #[test]
    fn an_effect_without_a_sprite_draws_nothing() {
        let v = draw(
            0,
            particle([0.0; 3], [1.0; 3], 0.0, [0.0; 3]),
            [0.0; 3],
            [0.0; 3],
        );
        assert!(v.is_empty());
    }

    #[test]
    fn local_mode_maps_positions_but_not_the_facing_axes() {
        let record = effect("e", 1, 0x1, 0.0, Vec::new());
        let mut instance = EffectInstance::new(0, record.record());
        let mut m = super::super::IDENTITY;
        m[3] = [5.0, 0.0, 0.0, 1.0];
        instance.place(&m, [0.0; 3], super::super::Mode::Local);
        instance
            .particles_mut()
            .push(particle([0.0, 0.0, 10.0], [2.0; 3], 0.0, [0.0; 3]));
        let mut out = Vec::new();
        quads(record.record(), &instance, &CAMERA, &mut out);
        assert!(close(out[0].position, [4.0, -1.0, 10.0]), "{:?}", out[0]);
    }
}
