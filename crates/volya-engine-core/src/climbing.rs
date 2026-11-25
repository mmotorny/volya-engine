// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Ladders and climbing them (docs/behavior/climbing.md, S12, cited by
//! section): a ladder's frame from its record (1.2), the quantities and
//! predicates of the player against it (2.1), the climbing states and
//! their per-step rules (2.2 – 2.5), the climbing drive (3.2), the jump off
//! a ladder (3.5) and the yaw limits taken on a ladder (3.6).
//!
//! The ladder boxes live in the collision world
//! ([`crate::collision::CollisionWorld::set_ladders`]); the player's body
//! collides with them (1.3, `collision::body`) and the movement layer runs
//! the states ([`crate::movement`]). Climbing plays no sound of its own
//! (section 4): the climbing leg cycles have no steps
//! ([`crate::footsteps::leg_cycle`]).

use volya_engine_formats::model::rotation;
use volya_engine_formats::object::{Object, ObjectRecord, Shape};

use crate::collision::geometry::{Vec3, add, dot, front_normal, length, scale, sub};

/// A normal extent below this is raised to it, the centre moved back (1.2).
pub const MIN_NORMAL_EXTENT: f32 = 0.2;
/// The collision box's half-sizes are at least this (1.2).
pub const MIN_HALF_SIZE: f32 = 0.005;
/// `facing`: the cosine of 22.5° (2.1).
pub const FACING_COS: f32 = 0.923_879_5;
/// The intent's camera offset, `sin 20°` (2.1).
pub const INTENT_PITCH: f32 = 0.342_020_15;
/// `γ` above this scales by the camera (2.1).
pub const INTENT_THRESHOLD: f32 = 0.001;
/// A ladder is taken within this distance of its axis, and let go beyond
/// it (2.4, 2.5).
pub const AXIS_REACH: f32 = 1.5;
/// The states at an end are entered this far past `r` from it (2.5).
pub const END_REACH: f32 = 0.3;
/// `cos 45°`: input this far from the normal lets go, a jump this far
/// sideways goes sideways (2.5, 3.5).
pub const COS_45: f32 = 0.707_106_77;
/// Looking down by more than this (`c.y` below its negative) takes the
/// ladder from the top (2.5).
pub const TOP_LOOK_DOWN: f32 = 0.157_079_64;
/// The small lengths of 2.5 and the conventions.
pub const TINY: f32 = 0.000_000_1;
/// The climbing drive per kilogram (3.2).
pub const CLIMB_DRIVE: f32 = 50.0;
/// The take-off speed off a ladder (3.5).
pub const JUMP_OFF_SPEED: f32 = 3.0;
/// `|â|²` at or below this leaves `â` as `a` (3.5).
pub const JUMP_DIRECTION_EPSILON: f32 = 1.175_494_4e-38;
/// The yaw is limited to this either side of the ladder's (3.6).
pub const YAW_LIMIT: f32 = 1.0;
/// The ladder's material (1.2).
pub const LADDER_MATERIAL: &str = "materials\\fake_ladders";
/// The change leaving → free is refused within this distance and time
/// (2.3).
pub const LEAVE_DISTANCE: f32 = 2.0;
pub const LEAVE_MS: u32 = 3_000;

fn unit(v: Vec3) -> Vec3 {
    let l = length(v);
    if l < TINY {
        [0.0; 3]
    } else {
        scale(v, 1.0 / l)
    }
}

/// `hcos(u, v)` (conventions): `None` when its denominator is 0.
pub fn hcos(u: Vec3, v: Vec3) -> Option<f32> {
    let d = ((u[0] * u[0] + u[2] * u[2]) * (v[0] * v[0] + v[2] * v[2])).sqrt();
    (d != 0.0).then(|| (u[0] * v[0] + u[2] * v[2]) / d)
}

/// *facing*(`v`) for camera direction `c` (2.1).
pub fn facing(c: Vec3, v: Vec3) -> bool {
    hcos(c, v).is_some_and(|k| k > FACING_COS)
}

/// The intent `k` (2.1) of acceleration `a` for camera direction `c`.
pub fn intent(a: Vec3, c: Vec3, v_n: Vec3) -> f32 {
    let g = dot(a, v_n);
    if g > INTENT_THRESHOLD {
        (c[1] + INTENT_PITCH) * g
    } else {
        g
    }
}

/// A ladder's frame (1.2) and collision box.
#[derive(Clone, Debug, PartialEq)]
pub struct Ladder {
    /// The record's name.
    pub name: String,
    /// The rows chosen as axis, side and normal (1.2 step 2).
    pub roles: [usize; 3],
    /// The normal extent was raised to 0.2.
    pub raised: bool,
    /// `A` and `S` were negated (step 3).
    pub flipped: bool,
    /// **C**, **A**, **S**, **N** and their directions.
    pub centre: Vec3,
    pub axis: Vec3,
    pub side: Vec3,
    pub normal: Vec3,
    pub axis_dir: Vec3,
    pub side_dir: Vec3,
    pub normal_dir: Vec3,
    /// **B** and **T** (step 4).
    pub lower: Vec3,
    pub upper: Vec3,
    /// The largest extent before step 2 (its use is believed, 2.4).
    pub radius: f32,
    /// The collision box: the record's rotation rows and half-sizes.
    pub rows: [Vec3; 3],
    pub half: Vec3,
    /// The box's faces as twelve triangles facing out (our choice: the
    /// player's shapes collide with them as with the level's triangles).
    pub triangles: [[Vec3; 3]; 12],
    pub min: Vec3,
    pub max: Vec3,
}

impl Ladder {
    /// A ladder at `position` with spawn `angles` and box extents
    /// `e0, e1, e2` (1.2).
    pub fn new(name: &str, position: Vec3, angles: Vec3, extents: Vec3) -> Self {
        let rows = rotation(angles);
        let [e0, e1, e2] = extents;
        // Step 2: axis, side, normal.
        let roles = if e0 < e1 {
            if e2 <= e0 {
                [1, 0, 2]
            } else if e2 <= e1 {
                [1, 2, 0]
            } else {
                [2, 1, 0]
            }
        } else if e2 <= e1 {
            [0, 1, 2]
        } else if e2 < e0 {
            [0, 2, 1]
        } else {
            [2, 0, 1]
        };
        let mut half = extents;
        let [ia, is, i_n] = roles;
        let raised = half[i_n] < MIN_NORMAL_EXTENT;
        let centre = if raised {
            half[i_n] = MIN_NORMAL_EXTENT;
            sub(position, scale(rows[i_n], MIN_NORMAL_EXTENT))
        } else {
            position
        };
        // Step 3.
        let mut axis = scale(rows[ia], half[ia]);
        let mut side = scale(rows[is], half[is]);
        let normal = scale(rows[i_n], half[i_n]);
        let flipped = axis[1] < 0.0;
        if flipped {
            axis = scale(axis, -1.0);
            side = scale(side, -1.0);
        }
        let radius = e0.max(e1).max(e2);
        let lower = add(sub(centre, axis), normal);
        let upper = add(add(centre, axis), normal);
        let half = half.map(|h| h.max(MIN_HALF_SIZE));
        let (triangles, min, max) = box_triangles(centre, rows, half);
        Self {
            name: name.to_owned(),
            roles,
            raised,
            flipped,
            centre,
            axis,
            side,
            normal,
            axis_dir: unit(axis),
            side_dir: unit(side),
            normal_dir: unit(normal),
            lower,
            upper,
            radius,
            rows,
            half,
            triangles,
            min,
            max,
        }
    }

    /// The ladder of a `climable_object` record (1.1): its first shape is
    /// a box. `None` for any other record, or one whose first shape is not
    /// a box (none in the shipped data, 1.1; our choice: no ladder).
    pub fn from_record(record: &ObjectRecord) -> Option<Self> {
        let Object::Climbable(c) = &record.object else {
            return None;
        };
        let Some(Shape::Box(b)) = c.shapes.first() else {
            return None;
        };
        let h = &record.header;
        Some(Self::new(
            &h.name,
            h.position,
            h.angles,
            [b[0][0], b[1][1], b[2][2]],
        ))
    }

    /// The quantities of 2.1 for body centre `p`.
    pub fn measure(&self, p: Vec3) -> Measure {
        let a = self.axis_dir;
        let q = add(self.centre, scale(a, dot(a, sub(p, self.centre))));
        let to_axis = sub(q, p);
        let delta = dot(self.normal_dir, to_axis);
        let to_b = sub(self.lower, p);
        let to_t = sub(self.upper, p);
        Measure {
            d_axis: length(to_axis),
            towards_axis: unit(to_axis),
            delta,
            v_n: scale(self.normal_dir, delta),
            lat: dot(self.side_dir, to_axis).abs(),
            h_low: dot(a, sub(p, self.lower)),
            h_top: dot(a, sub(self.upper, p)),
            d_b: length(to_b),
            d_t: length(to_t),
            w_b: unit(to_b),
            w_t: unit(to_t),
        }
    }

    /// *front*(`f`) for body centre `p` and radius `r` (2.1).
    pub fn front(&self, p: Vec3, r: f32, f: f32) -> bool {
        self.measure(p).front(self, r, f)
    }
}

/// The box centred at `c` with axes `rows` and half-sizes `half` as twelve
/// triangles whose front sides face out, and its bounds.
fn box_triangles(c: Vec3, rows: [Vec3; 3], half: Vec3) -> ([[Vec3; 3]; 12], Vec3, Vec3) {
    let corner = |s: [f32; 3]| (0..3).fold(c, |p, k| add(p, scale(rows[k], s[k] * half[k])));
    let mut triangles = [[[0.0; 3]; 3]; 12];
    let mut n = 0;
    for k in 0..3 {
        let (i, j) = ((k + 1) % 3, (k + 2) % 3);
        for sign in [-1.0f32, 1.0] {
            let at = |a: f32, b: f32| {
                let mut s = [0.0; 3];
                s[k] = sign;
                s[i] = a;
                s[j] = b;
                corner(s)
            };
            let q = [at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)];
            let out = scale(rows[k], sign);
            for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                let facing_out = front_normal(&t).is_some_and(|f| dot(f, out) > 0.0);
                triangles[n] = if facing_out { t } else { [t[0], t[2], t[1]] };
                n += 1;
            }
        }
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in triangles.iter().flatten() {
        for k in 0..3 {
            min[k] = min[k].min(v[k]);
            max[k] = max[k].max(v[k]);
        }
    }
    (triangles, min, max)
}

/// The quantities of 2.1 for one body centre **P**.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    pub d_axis: f32,
    /// `û`.
    pub towards_axis: Vec3,
    /// `δ`, negative in front.
    pub delta: f32,
    pub v_n: Vec3,
    pub lat: f32,
    pub h_low: f32,
    pub h_top: f32,
    pub d_b: f32,
    pub d_t: f32,
    /// `ŵ_B`, `ŵ_T`.
    pub w_b: Vec3,
    pub w_t: Vec3,
}

impl Measure {
    /// *front*(`f`) (2.1).
    pub fn front(&self, l: &Ladder, r: f32, f: f32) -> bool {
        -self.delta > length(l.normal) + r / 2.0 + f
    }

    /// *in range* (2.1).
    pub fn in_range(&self, l: &Ladder, r: f32) -> bool {
        self.delta.abs() < length(l.normal) + r + 0.05
            && self.lat < length(l.side)
            && self.h_low > -0.2
            && self.h_top + r > 0.0
    }
}

/// The climbing states (2.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClimbState {
    #[default]
    Free,
    Near,
    AtTop,
    AtBottom,
    ClimbingUp,
    ClimbingDown,
    Leaving,
}

impl ClimbState {
    /// The player is climbing (2.2); gravity is off in exactly these
    /// states (2.3).
    pub fn climbing(self) -> bool {
        matches!(self, Self::ClimbingUp | Self::ClimbingDown)
    }
}

/// What a step's rules read (2.5): the body centre **P**, the body radius
/// `r`, the acceleration `a` the body holds, the camera direction **c**,
/// the body's velocity, the clock (ms) and the body's mass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepView {
    pub centre: Vec3,
    pub r: f32,
    pub a: Vec3,
    pub camera: Vec3,
    pub velocity: Vec3,
    pub clock: u32,
    pub mass: f32,
    pub gravity: f32,
}

/// The driving force while on or at a ladder (3.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drive {
    /// `F = ĉ · 50 · T · m` along this direction, in place of the ground
    /// rules.
    Climb(Vec3),
    /// The ground rules, with the input's horizontal direction replaced by
    /// this one's where given (steering onto an end), and without the
    /// step-assistance factor 4 where `no_assist_factor`.
    Ground {
        steer: Option<Vec3>,
        no_assist_factor: bool,
    },
}

/// The player's ladder and climbing state (2.2) with the last accepted
/// change (2.3).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Climber {
    ladder: Option<usize>,
    state: ClimbState,
    /// The clock and body centre of the last accepted change; none before
    /// the first (its value at creation is not specified; our choice).
    last: Option<(u32, Vec3)>,
}

impl Climber {
    pub fn ladder(&self) -> Option<usize> {
        self.ladder
    }

    pub fn state(&self) -> ClimbState {
        self.state
    }

    /// Tries the change to `to` at `clock` with body centre `centre`
    /// (2.3); whether it was accepted.
    pub fn change(&mut self, to: ClimbState, clock: u32, centre: Vec3) -> bool {
        let (d_min, t_min) = if self.state == ClimbState::Leaving && to == ClimbState::Free {
            (LEAVE_DISTANCE, LEAVE_MS)
        } else {
            (0.0, 0)
        };
        if let Some((t, p)) = self.last
            && length(sub(centre, p)) <= d_min
            && clock.wrapping_sub(t) <= t_min
        {
            return false;
        }
        self.last = Some((clock, centre));
        self.state = to;
        true
    }

    /// The physics world found ladder `index` near the player (2.4).
    pub fn found(&mut self, ladders: &[Ladder], index: usize, view: &StepView) {
        if self.ladder == Some(index) {
            return;
        }
        let d = ladders[index].measure(view.centre).d_axis;
        if d > AXIS_REACH {
            return;
        }
        if let Some(own) = self.ladder.and_then(|i| ladders.get(i))
            && d > own.measure(view.centre).d_axis
        {
            return;
        }
        self.change(ClimbState::Near, view.clock, view.centre);
        self.ladder = Some(index);
    }

    /// The jump off the ladder applied this step: the state becomes
    /// *leaving* (3.5).
    pub fn jumped(&mut self, view: &StepView) {
        self.change(ClimbState::Leaving, view.clock, view.centre);
    }

    /// The rules of the state at the start of a step (2.5); the extra force
    /// they apply in this step.
    pub fn rules(&mut self, ladders: &[Ladder], v: &StepView) -> Vec3 {
        use ClimbState::*;
        let Some(l) = self.ladder.and_then(|i| ladders.get(i)) else {
            return [0.0; 3];
        };
        let m = l.measure(v.centre);
        let r = v.r;
        let k = intent(v.a, v.camera, m.v_n);
        let (clock, p) = (v.clock, v.centre);
        let mut force = [0.0; 3];
        let ends = |s: &mut Self| {
            if m.d_t > m.d_b {
                if m.d_b < r + END_REACH {
                    s.change(AtBottom, clock, p);
                }
            } else if m.d_t < r + END_REACH {
                s.change(AtTop, clock, p);
            }
        };
        let sideways =
            length(v.a) >= TINY && m.d_axis >= TINY && dot(l.normal_dir, unit(v.a)).abs() < COS_45;
        let hold = |force: &mut Vec3, s: &mut Self| {
            if m.d_axis - r > AXIS_REACH {
                s.change(Free, clock, p);
            }
            if length(v.a) < TINY && dot(l.normal, m.towards_axis) < 0.0 {
                *force = add(*force, scale(m.towards_axis, v.mass * v.gravity));
            }
        };
        match self.state {
            Free => self.ladder = None,
            Near => {
                if m.front(l, r, 0.0) && m.in_range(l, r) && facing(v.camera, m.v_n) {
                    let to = if k > 0.0 { ClimbingUp } else { ClimbingDown };
                    self.change(to, clock, p);
                } else {
                    ends(self);
                }
            }
            AtTop => {
                if m.in_range(l, r)
                    && v.camera[1] < -TOP_LOOK_DOWN
                    && m.delta.abs() > r / 3.0
                    && m.front(l, r, 0.1)
                {
                    self.change(ClimbingDown, clock, p);
                }
                if m.d_t - r > AXIS_REACH {
                    self.change(Free, clock, p);
                }
            }
            AtBottom => {
                if m.in_range(l, r)
                    && facing(v.camera, m.w_b)
                    && dot(v.a, m.w_b) > 0.0
                    && k > 0.0
                    && m.front(l, r, 0.0)
                {
                    self.change(ClimbingUp, clock, p);
                }
                if m.d_b - r > AXIS_REACH {
                    self.change(Free, clock, p);
                }
            }
            ClimbingUp => {
                if k < 0.0 && m.front(l, r, 0.0) {
                    self.change(ClimbingDown, clock, p);
                }
                if sideways {
                    self.change(Leaving, clock, p);
                }
                if m.h_top + r < 0.1 {
                    self.change(AtTop, clock, p);
                }
                hold(&mut force, self);
            }
            ClimbingDown => {
                if k > 0.0 && m.front(l, r, 0.0) {
                    self.change(ClimbingUp, clock, p);
                }
                if sideways {
                    self.change(Leaving, clock, p);
                }
                if m.h_low - r < 0.1 {
                    self.change(AtBottom, clock, p);
                }
                hold(&mut force, self);
                if m.h_top < -r {
                    self.change(Free, clock, p);
                }
                if v.velocity[1] > TINY {
                    force = add(force, [0.0, -v.mass * v.gravity, 0.0]);
                }
            }
            Leaving => {
                ends(self);
                self.change(Free, clock, p);
            }
        }
        force
    }

    /// The driving force's rule in the current state (3.2), for camera
    /// direction `camera` and held acceleration `a`; `None` without a
    /// ladder or where the ground rules apply unchanged.
    pub fn drive(&self, ladders: &[Ladder], v: &StepView) -> Option<Drive> {
        use ClimbState::*;
        let l = self.ladder.and_then(|i| ladders.get(i))?;
        let m = l.measure(v.centre);
        match self.state {
            ClimbingUp => Some(Drive::Climb(unit(add(l.axis_dir, m.towards_axis)))),
            ClimbingDown if m.front(l, v.r, 0.0) => {
                Some(Drive::Climb(unit(sub(m.towards_axis, l.axis_dir))))
            }
            ClimbingDown => Some(Drive::Ground {
                steer: None,
                no_assist_factor: true,
            }),
            AtTop | AtBottom => {
                let (w, d) = if self.state == AtTop {
                    (m.w_t, m.d_t)
                } else {
                    (m.w_b, m.d_b)
                };
                (facing(v.camera, w) && d >= 0.001 && dot(v.a, w) > 0.0).then_some(Drive::Ground {
                    steer: Some(w),
                    no_assist_factor: false,
                })
            }
            Free | Near | Leaving => None,
        }
    }
}

/// The velocity a jump off ladder `l` sets (3.5) for held acceleration
/// `a`.
pub fn jump_velocity(l: &Ladder, a: Vec3) -> Vec3 {
    let a2 = dot(a, a);
    let dir = if a2 <= JUMP_DIRECTION_EPSILON {
        a
    } else {
        scale(a, 1.0 / a2.sqrt())
    };
    let across = dot(dir, l.side_dir);
    let j = if across.abs() > COS_45 {
        unit(add(l.normal_dir, scale(l.side_dir, across.signum())))
    } else {
        l.normal_dir
    };
    scale(j, JUMP_OFF_SPEED)
}

/// `x` brought into `[−π, π]` (3.6): a value outside is replaced by
/// `2π · frac(x / 2π)`, then reduced by 2π if above π. `frac` is taken as
/// `x − floor(x)` (our reading; the spec does not say for negative `x`).
pub fn wrap_angle(x: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    if (-PI..=PI).contains(&x) {
        return x;
    }
    let f = x / TAU;
    let y = TAU * (f - f.floor());
    if y > PI { y - TAU } else { y }
}

/// The yaw limits taken on the frame the climbing flag is set (3.6): the
/// ladder's normal direction `n`, the body's heading `heading` and the
/// camera yaw `yaw` as stored; `None` when the yaw is not limited.
pub fn yaw_limits(n: Vec3, heading: f32, yaw: f32) -> Option<[f32; 2]> {
    use std::f32::consts::{PI, TAU};
    let psi = if n[1].abs() > COS_45 {
        heading
    } else {
        (-n[0]).atan2(-n[2])
    };
    let mut d = wrap_angle(psi) - wrap_angle(yaw);
    if d > PI {
        d -= TAU;
    } else if d < -PI {
        d += TAU;
    }
    (-YAW_LIMIT < d && d < YAW_LIMIT).then_some([yaw + d - YAW_LIMIT, yaw + d + YAW_LIMIT])
}

#[cfg(test)]
mod tests;
