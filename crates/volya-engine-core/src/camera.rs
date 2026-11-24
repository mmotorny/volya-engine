// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's first-person camera at the start of a game
//! (docs/behavior/start-position.md, sections 1 and 4–7): eye height, heading
//! and pitch from the spawn angles, view and projection matrices, far plane.
//!
//! World coordinates are the level's: metres, +y up, left-handed (section 1).
//! Matrices use the row-vector convention, `p · M`, stored row by row.

use std::io;

use crate::config::{Section, value};

/// Vertical field of view, fixed by the game (section 7).
pub const FOV_Y_DEGREES: f32 = 67.5;
/// Near plane (section 7).
pub const NEAR: f32 = 0.2;
/// Near plane while the eye is within [`NEAR_CLEARANCE`] of collision.
pub const NEAR_CLOSE: f32 = 0.01;
/// Half-size of the cube around the eye that selects [`NEAR_CLOSE`].
pub const NEAR_CLEARANCE: f32 = 0.2;
/// The first-person camera's own aspect value.
pub const CAMERA_ASPECT: f32 = 1.0;
/// Starting values of the smoothed field of view (degrees), aspect factor
/// and far plane when the camera is created (section 7.2).
pub const INITIAL_FOV_DEGREES: f32 = 90.0;
pub const INITIAL_ASPECT: f32 = 1.0;
pub const INITIAL_FAR: f32 = 100.0;

pub type Vec3 = [f32; 3];
pub type Mat4 = [[f32; 4]; 4];

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: Vec3) -> Vec3 {
    let len = dot(a, a).sqrt();
    a.map(|c| c / len)
}

/// What the eye height needs from the actor's configuration section
/// (section 4.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorBody {
    pub camera_height_factor: f32,
    /// `ph_box0_size`: half-extents of the collision box current at spawn.
    pub box0_size: Vec3,
}

impl ActorBody {
    /// Reads `camera_height_factor` and `ph_box0_size` from the actor's
    /// section (`actor`). A missing key is an error (the spec does not say
    /// what the game does then).
    pub fn from_section(section: &Section) -> io::Result<Self> {
        let get = |key: &str| {
            section.get(key).flatten().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("[{}] has no {key}", section.name()),
                )
            })
        };
        Ok(Self {
            camera_height_factor: value::float(get("camera_height_factor")?),
            box0_size: value::floats(get("ph_box0_size")?),
        })
    }

    /// `h = H × camera_height_factor` with `H = 2 × ph_box0_size.y`.
    pub fn eye_height(&self) -> f32 {
        2.0 * self.box0_size[1] * self.camera_height_factor
    }

    /// The eye, directly above the actor's (feet) position.
    pub fn eye(&self, position: Vec3) -> Vec3 {
        [position[0], position[1] + self.eye_height(), position[2]]
    }
}

/// Camera angles in radians (section 5).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Angles {
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
}

impl Angles {
    /// From a spawn object's stored angles: yaw = −a1, pitch = a0, roll = 0
    /// (a2 is not used).
    pub fn from_spawn(angles: Vec3) -> Self {
        Self {
            yaw: -angles[1],
            pitch: angles[0],
            roll: 0.0,
        }
    }

    /// `R = Ry(yaw) · Rx(pitch) · Rz(roll)` for column vectors.
    fn rotation(&self) -> [[f32; 3]; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let (sr, cr) = self.roll.sin_cos();
        let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
        let rx = [[1.0, 0.0, 0.0], [0.0, cp, -sp], [0.0, sp, cp]];
        let rz = [[cr, -sr, 0.0], [sr, cr, 0.0], [0.0, 0.0, 1.0]];
        mul3(mul3(ry, rx), rz)
    }

    /// Viewing direction `R · (0, 0, 1)` and up vector `R · (0, 1, 0)`.
    /// Yaw 0, pitch 0 looks along +z; positive yaw turns toward +x; positive
    /// pitch looks down.
    pub fn direction_up(&self) -> (Vec3, Vec3) {
        let r = self.rotation();
        ([r[0][2], r[1][2], r[2][2]], [r[0][1], r[1][1], r[2][1]])
    }
}

fn mul3(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    out
}

/// The view matrix from eye `e`, direction `d` and up `u` (section 6):
/// view-space x = (p − e)·r, y = (p − e)·u', z = (p − e)·d with
/// `u' = normalize(u − d (u·d))` and `r = u' × d`.
pub fn view_matrix(e: Vec3, d: Vec3, u: Vec3) -> Mat4 {
    let ud = dot(u, d);
    let u = normalize([u[0] - d[0] * ud, u[1] - d[1] * ud, u[2] - d[2] * ud]);
    let r = cross(u, d);
    [
        [r[0], u[0], d[0], 0.0],
        [r[1], u[1], d[1], 0.0],
        [r[2], u[2], d[2], 0.0],
        [-dot(e, r), -dot(e, u), -dot(e, d), 1.0],
    ]
}

/// The aspect factor: `(height ÷ 2) / (width ÷ 2)`, halves rounded down to
/// integers, times [`CAMERA_ASPECT`] (section 7). Halves below 1 count as 1,
/// so a target narrower than 2 pixels gives a finite factor (our choice; the
/// spec does not cover it).
pub fn aspect_factor(width: u32, height: u32) -> f32 {
    (height / 2).max(1) as f32 / (width / 2).max(1) as f32 * CAMERA_ASPECT
}

/// The projection matrix for vertical field of view `fov_y` (radians),
/// aspect factor `aspect`, near `n` and far `z` (section 7). Maps view space
/// to x, y in −1 … 1 (+x right, +y up) and depth 0 (near) … 1 (far) after
/// division by w.
pub fn projection(fov_y: f32, aspect: f32, n: f32, z: f32) -> Mat4 {
    let t = (fov_y / 2.0).tan();
    [
        [aspect / t, 0.0, 0.0, 0.0],
        [0.0, 1.0 / t, 0.0, 0.0],
        [0.0, 0.0, z / (z - n), 1.0],
        [0.0, 0.0, -n * z / (z - n), 0.0],
    ]
}

/// The weight of an environment modifier at `camera`: `(1 − δ/ρ) × π` when
/// the distance δ to its centre is below its radius ρ (section 7.1).
pub fn modifier_weight(centre: Vec3, radius: f32, power: f32, camera: Vec3) -> Option<f32> {
    let d = [
        camera[0] - centre[0],
        camera[1] - centre[1],
        camera[2] - centre[2],
    ];
    let distance = dot(d, d).sqrt();
    (distance < radius).then(|| (1.0 - distance / radius) * power)
}

/// The far plane (section 7.1): the two weather sections' `far_plane` values
/// blended by `t`, with the applying modifiers' `(weight, far_plane)` pairs,
/// scaled by `rs_vis_distance`.
pub fn far_plane(
    far1: f32,
    far2: f32,
    t: f32,
    modifiers: impl IntoIterator<Item = (f32, f32)>,
    vis_distance: f32,
) -> f32 {
    let (mut sum, mut weights) = ((1.0 - t) * far1 + t * far2, 0.0);
    for (w, f) in modifiers {
        sum += w * f;
        weights += w;
    }
    sum * vis_distance / (1.0 + weights)
}

/// One frame of the smoothing of field of view, aspect and far plane
/// (section 7.2): `value + k (target − value)`, `k = clamp(10 × dt, 0, 1)`.
pub fn smooth(value: f32, target: f32, dt: f32) -> f32 {
    value + (10.0 * dt).clamp(0.0, 1.0) * (target - value)
}

/// Camera inertia (section 6): `old × cam_inert + computed × (1 − cam_inert)`
/// per component, for position and direction alike.
pub fn inert(old: Vec3, computed: Vec3, cam_inert: f32) -> Vec3 {
    [0, 1, 2].map(|i| old[i] * cam_inert + computed[i] * (1.0 - cam_inert))
}

/// Free-camera speed, metres per second (our design: the free camera is not
/// game behaviour).
pub const FREE_SPEED: f32 = 4.0;
/// Speed factor while the speed modifier is held.
pub const FREE_FAST: f32 = 5.0;
/// Pitch limit of the free camera, the first-person camera's `lim_pitch`
/// (section 5).
pub const FREE_PITCH_LIMIT: f32 = 1.5;

/// One frame's request to the free camera.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FreeMove {
    /// Along the viewing direction, −1 … 1 (more for touch beyond the
    /// stick's rim).
    pub forward: f32,
    /// Toward the camera's right, −1 … 1.
    pub right: f32,
    /// Along world +y, −1 … 1.
    pub up: f32,
    /// The speed modifier.
    pub fast: bool,
    /// Turn this frame, radians: yaw (positive toward +x, i.e. right) and
    /// pitch (positive looks down).
    pub yaw: f32,
    pub pitch: f32,
}

/// A camera flying freely from the player's start (our design).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreeCamera {
    pub eye: Vec3,
    pub angles: Angles,
}

impl FreeCamera {
    pub fn new(eye: Vec3, angles: Angles) -> Self {
        Self { eye, angles }
    }

    /// Applies `m` over `dt` seconds: turn first, then move along the new
    /// viewing direction, the horizontal right vector and world up.
    pub fn apply(&mut self, m: &FreeMove, dt: f32) {
        self.angles.yaw = (self.angles.yaw + m.yaw).rem_euclid(std::f32::consts::TAU);
        self.angles.pitch =
            (self.angles.pitch + m.pitch).clamp(-FREE_PITCH_LIMIT, FREE_PITCH_LIMIT);
        let (d, _) = self.angles.direction_up();
        let (sy, cy) = self.angles.yaw.sin_cos();
        let right = [cy, 0.0, -sy];
        let speed = FREE_SPEED * if m.fast { FREE_FAST } else { 1.0 } * dt.max(0.0);
        for i in 0..3 {
            self.eye[i] += speed * (m.forward * d[i] + m.right * right[i]);
        }
        self.eye[1] += speed * m.up;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f32], b: &[f32], eps: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= eps)
    }

    #[test]
    fn basis_conventions() {
        let (d, u) = Angles::default().direction_up();
        assert_eq!((d, u), ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]));
        let quarter = std::f32::consts::FRAC_PI_2;
        let (d, _) = Angles {
            yaw: quarter,
            ..Default::default()
        }
        .direction_up();
        assert!(close(&d, &[1.0, 0.0, 0.0], 1e-6), "positive yaw toward +x");
        let (d, u) = Angles {
            pitch: 0.3,
            ..Default::default()
        }
        .direction_up();
        assert!(d[1] < 0.0, "positive pitch looks down");
        assert!(close(&u, &[0.0, 0.3f32.cos(), 0.3f32.sin()], 1e-6));
        // The closed form of section 5 for roll 0.
        let a = Angles {
            yaw: 1.0,
            pitch: -0.4,
            roll: 0.0,
        };
        let (d, u) = a.direction_up();
        let (sy, cy, sp, cp) = (1f32.sin(), 1f32.cos(), (-0.4f32).sin(), (-0.4f32).cos());
        assert!(close(&d, &[cp * sy, -sp, cp * cy], 1e-6));
        assert!(close(&u, &[sp * sy, cp, sp * cy], 1e-6));
        // Right = up × direction: +x for the default view (left-handed).
        let v = view_matrix([0.0; 3], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        assert_eq!([v[0][0], v[1][0], v[2][0]], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn projection_and_aspect() {
        assert_eq!(aspect_factor(1024, 768), 0.75);
        assert_eq!(aspect_factor(1920, 1080), 0.5625);
        assert_eq!(aspect_factor(1281, 1023), 511.0 / 640.0);
        assert_eq!(aspect_factor(1, 0), 1.0);
        let p = projection(FOV_Y_DEGREES.to_radians(), 0.75, NEAR, 350.0);
        assert!((p[0][0] - 1.122454).abs() < 1e-5);
        assert!((p[1][1] - 1.496606).abs() < 1e-5);
        // Depth: near → 0, far → 1.
        let depth = |z: f32| (z * p[2][2] + p[3][2]) / z;
        assert!(depth(NEAR).abs() < 1e-6);
        assert!((depth(350.0) - 1.0).abs() < 1e-6);
        let hfov = 2.0 * ((FOV_Y_DEGREES.to_radians() / 2.0).tan() / 0.5625).atan();
        assert!((hfov.to_degrees() - 99.82).abs() < 0.01);
    }

    #[test]
    fn far_plane_and_smoothing() {
        assert_eq!(far_plane(350.0, 350.0, 0.3, [], 1.0), 350.0);
        assert_eq!(far_plane(100.0, 300.0, 0.5, [], 1.5), 300.0);
        assert_eq!(far_plane(350.0, 350.0, 0.0, [(1.0, 150.0)], 1.0), 250.0);
        assert_eq!(
            modifier_weight([0.0; 3], 4.0, 2.0, [0.0, 2.0, 0.0]),
            Some(1.0)
        );
        assert_eq!(modifier_weight([0.0; 3], 4.0, 2.0, [0.0, 4.0, 0.0]), None);
        assert_eq!(smooth(90.0, 67.5, 0.05), 78.75);
        assert_eq!(smooth(90.0, 67.5, 0.2), 67.5);
        assert_eq!(smooth(90.0, 67.5, -1.0), 90.0);
        assert_eq!(inert([0.0; 3], [1.0, 2.0, 4.0], 0.75), [0.25, 0.5, 1.0]);
        assert_eq!(inert([3.0; 3], [1.0; 3], 0.0), [1.0; 3]);
    }

    #[test]
    fn free_camera_moves_and_turns() {
        let mut c = FreeCamera::new([0.0; 3], Angles::default());
        let forward = FreeMove {
            forward: 1.0,
            ..Default::default()
        };
        c.apply(&forward, 0.5);
        assert!(close(&c.eye, &[0.0, 0.0, 2.0], 1e-6), "{:?}", c.eye);
        // Right is +x at yaw 0; up is world +y; fast is five times.
        c.apply(
            &FreeMove {
                right: 1.0,
                up: -1.0,
                fast: true,
                ..Default::default()
            },
            0.1,
        );
        assert!(close(&c.eye, &[2.0, -2.0, 2.0], 1e-6), "{:?}", c.eye);
        // A quarter turn right: forward is now +x; pitch clamps.
        c.apply(
            &FreeMove {
                yaw: std::f32::consts::FRAC_PI_2,
                pitch: 3.0,
                ..Default::default()
            },
            0.0,
        );
        assert_eq!(c.angles.pitch, FREE_PITCH_LIMIT);
        c.angles.pitch = 0.0;
        c.apply(&forward, 0.25);
        assert!(close(&c.eye, &[3.0, -2.0, 2.0], 1e-5), "{:?}", c.eye);
        // Looking down, forward descends.
        c.angles.pitch = 1.0;
        let y = c.eye[1];
        c.apply(&forward, 1.0);
        assert!(c.eye[1] < y);
    }
}
