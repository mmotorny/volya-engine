// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Head bone turning (docs/behavior/head-bone-turning.md, TC4): the head
//! callback a trader attaches at its spawn to its model's bone
//! [`HEAD_BONE`], in follow mode, turning the head bone toward the level's
//! controlled object by the turning angle *A* (section 3) after the motions
//! have posed it (section 4).

use volya_engine_formats::model::Mat4;

/// The bone the head callback is attached to (section 2).
pub const HEAD_BONE: &str = "bip01_head";

/// ε of the target heading (3.2).
const EPSILON: f32 = 1e-7;
/// The trader's heading's tilt test, 2⁻¹⁹ (3.3).
const TILT: f32 = 1.0 / 524_288.0;
const PI: f32 = std::f32::consts::PI;
/// f32(2π) = 6.28318548 (3.4).
const TWO_PI: f32 = std::f32::consts::TAU;
/// f32(1 / (2π)) = 0.159154937 (3.4).
const INVERSE_TWO_PI: f32 = (1.0 / std::f64::consts::TAU) as f32;

/// atan(*x*) in extended precision, rounded to single precision.
fn atan(x: f32) -> f32 {
    f64::from(x).atan() as f32
}

/// atan2(*a*, *b*) in extended precision, rounded to single precision.
fn atan2(a: f32, b: f32) -> f32 {
    f64::from(a).atan2(f64::from(b)) as f32
}

/// The target heading *y* of the horizontal offset (*dx*, *dz*) from the
/// trader to the target (3.2): a heading whose forward direction
/// (−sin *y*, 0, cos *y*) points along it; 0 when both are below ε.
pub fn target_heading(dx: f32, dz: f32) -> f32 {
    if dx.abs() < EPSILON && dz.abs() < EPSILON {
        0.0
    } else if dz.abs() < EPSILON {
        if dx > 0.0 { -PI / 2.0 } else { PI / 2.0 }
    } else {
        let r = atan(dx / dz);
        if dz >= 0.0 { -r } else { -(r - PI) }
    }
}

/// The heading *h* of transform `x` (3.3).
pub fn heading(x: &Mat4) -> f32 {
    if (x[0][1] * x[0][1] + x[1][1] * x[1][1]).sqrt() > TILT {
        -atan2(x[2][0], x[2][2])
    } else {
        -atan2(-x[0][2], x[0][0])
    }
}

/// `d` brought into −π … π (3.4).
pub fn wrap(d: f32) -> f32 {
    if (-PI..=PI).contains(&d) {
        return d;
    }
    let x = d * INVERSE_TWO_PI;
    let mut f = x - x.trunc();
    if f < 0.0 {
        f += 1.0;
    }
    f *= TWO_PI;
    if f > PI {
        f -= TWO_PI;
    }
    f
}

/// The turning angle *A* (section 3) of a trader with transform `trader`
/// toward a target at `target`: the target heading less the trader's
/// heading, wrapped; heights play no part. No limit, rate or state (3.5).
pub fn turning_angle(target: [f32; 3], trader: &Mat4) -> f32 {
    let dx = target[0] - trader[3][0];
    let dz = target[2] - trader[3][2];
    wrap(target_heading(dx, dz) - heading(trader))
}

/// The head bone's current matrix *T* turned by `a` about its own x axis
/// (section 4): *T*′ = *R* · *T*, its translation kept.
pub fn turn(t: &mut Mat4, a: f32) {
    let (s, c) = f64::from(a).sin_cos();
    let (s, c) = (s as f32, c as f32);
    let (t1, t2) = (t[1], t[2]);
    for i in 0..3 {
        t[1][i] = c * t1[i] - s * t2[i];
        t[2][i] = s * t1[i] + c * t2[i];
    }
    for row in &mut t[..3] {
        row[3] = 0.0;
    }
    t[3][3] = 1.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transform of an object with angles (0, `a`, 0) at `p`: row 2
    /// its forward direction (−sin *a*, 0, cos *a*) (3.3).
    fn yawed(p: [f32; 3], a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        [
            [c, 0.0, s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [-s, 0.0, c, 0.0],
            [p[0], p[1], p[2], 1.0],
        ]
    }

    #[test]
    fn the_worked_example() {
        // Section 6: `escape_trader` and the player's record.
        let trader = yawed([-244.6927, -24.958, -133.96], 1.549_194);
        let player = [-246.7263, -24.7933, -134.4339];
        assert!((heading(&trader) - 1.549_194).abs() < 1e-6);
        let (dx, dz) = (player[0] - trader[3][0], player[2] - trader[3][2]);
        assert!((dx + 2.0336).abs() < 1e-4 && (dz + 0.4739).abs() < 1e-4);
        // The positions as section 6 rounds them: the angles to 1e-4
        // (the game data's own give them to 1e-5, tests/head_bone_turning.rs).
        assert!((target_heading(dx, dz) - 1.799_727_6).abs() < 1e-4);
        assert!((turning_angle(player, &trader) - 0.250_533_6).abs() < 1e-4);
    }

    #[test]
    fn the_target_heading_points_at_the_target() {
        for (dx, dz) in [(1.0, 2.0), (-3.0, 0.5), (2.0, -1.0), (-0.1, -4.0)] {
            let y = target_heading(dx, dz);
            let n = (dx * dx + dz * dz).sqrt();
            assert!((-y.sin() - dx / n).abs() < 1e-5, "{dx}, {dz}");
            assert!((y.cos() - dz / n).abs() < 1e-5, "{dx}, {dz}");
        }
        assert_eq!(target_heading(0.0, 0.0), 0.0);
        assert_eq!(target_heading(1.0, 0.0), -PI / 2.0);
        assert_eq!(target_heading(-1.0, 0.0), PI / 2.0);
    }

    #[test]
    fn the_difference_is_wrapped() {
        assert_eq!(wrap(1.0), 1.0);
        assert_eq!(wrap(-PI), -PI);
        assert!((wrap(4.0) - (4.0 - TWO_PI)).abs() < 1e-6);
        assert!((wrap(-4.0) - (TWO_PI - 4.0)).abs() < 1e-6);
        assert!((wrap(7.0) - (7.0 - TWO_PI)).abs() < 1e-5);
        // A target behind the trader: no limit (3.5).
        let trader = yawed([0.0; 3], 3.0);
        let a = turning_angle([0.0, 0.0, -1.0], &trader);
        assert!((a - (PI - 3.0)).abs() < 1e-6, "{a}");
        let a = turning_angle([0.0, 0.0, 1.0], &trader);
        assert!((a + 3.0).abs() < 1e-6, "{a}");
    }

    #[test]
    fn the_turn_is_about_the_bone_x_axis() {
        let mut t = yawed([1.0, 2.0, 3.0], 0.4);
        let before = t;
        turn(&mut t, 0.3);
        assert_eq!(t[0], before[0]);
        assert_eq!(t[3], before[3]);
        let (s, c) = 0.3f32.sin_cos();
        for i in 0..3 {
            assert!((t[1][i] - (c * before[1][i] - s * before[2][i])).abs() < 1e-6);
            assert!((t[2][i] - (s * before[1][i] + c * before[2][i])).abs() < 1e-6);
        }
    }
}
