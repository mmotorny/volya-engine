// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Following a [`Route`] with the movement's own requests (our design):
//! each frame says where to run, in which posture, when to jump and, on a
//! ladder, where the camera must look for the climbing rules
//! (climbing.md 2.5: facing the ladder, looking up to climb up and down
//! to climb down).

use super::{Kind, LadderEnds, Route, Waypoint, horizontal_distance};
use crate::collision::geometry::{Vec3, add, length, scale, sub};
use crate::movement::{PlayerState, Request};

/// A waypoint on foot counts as reached within this many metres,
/// horizontally (our choice).
pub const REACH: f32 = 0.3;
/// The vertical slack of reaching a waypoint (our choice).
pub const REACH_HEIGHT: f32 = 0.6;
/// The posture an edge needs is taken this far before its start (our
/// choice).
pub const POSTURE_AHEAD: f32 = 0.7;
/// A jump up onto a ledge is pressed once the feet are this near the
/// ledge's floor, a jump across once this near the take-off (our choices,
/// from the jump's arc at running speed: player-movement.md 6.6).
pub const JUMP_UP_TRIGGER: f32 = 1.1;
pub const JUMP_OVER_TRIGGER: f32 = 0.45;
/// A jump is pressed once running at this fraction of the run speed, or
/// after [`RUN_UP_MS`] on its edge anyway (our choices).
pub const RUN_UP_SPEED: f32 = 2.0;
pub const RUN_UP_MS: u64 = 500;
/// A failed jump is tried again after this many milliseconds on the
/// ground (our choice).
pub const JUMP_RETRY_MS: u64 = 700;
/// The camera's pitch on a ladder (+ down; our choices: past the 20° that
/// turns climbing down, climbing.md 2.1, and the 9° that takes it from the
/// top, 2.5).
pub const CLIMB_UP_PITCH: f32 = -0.3;
pub const CLIMB_DOWN_PITCH: f32 = 0.7;
/// The trip turns the camera at most this fast, rad/s (our choice).
pub const TURN_RATE: f32 = 6.0;
/// Farther than this from the leg under way the trip is off its route
/// (our choice).
pub const OFF_ROUTE: f32 = 1.5;
/// The trip is stuck when its distance to the waypoint under way hasn't
/// shrunk by [`PROGRESS`] metres for [`STUCK_MS`] (our choices).
pub const STUCK_MS: u64 = 700;
pub const PROGRESS: f32 = 0.05;

/// One frame of following a route.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steer {
    /// Where to run (only its horizontal bearing counts).
    pub toward: Vec3,
    /// 0 standing, 1 crouch, 2 low crouch.
    pub posture: usize,
    /// Press jump this frame.
    pub jump: bool,
    /// The camera's yaw and pitch the trip turns to, on a ladder.
    pub look: Option<[f32; 2]>,
}

/// What following does this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Follow {
    Steer(Steer),
    /// The last waypoint is reached.
    Done,
    /// No progress toward the waypoint under way.
    Stuck,
    /// Pushed off the leg under way.
    Off,
}

/// A route being followed.
#[derive(Clone, Debug, PartialEq)]
pub struct Follower {
    route: Route,
    next: usize,
    /// When the waypoint under way became so.
    since: u64,
    jumped: Option<u64>,
    /// Jump was pressed on the latest frame: released on this one.
    jump_down: bool,
    /// The posture the latest frame asked for.
    posture: usize,
    closest: f32,
    progress_at: u64,
}

fn yaw_to(v: Vec3) -> f32 {
    v[0].atan2(v[2])
}

impl Follower {
    pub fn new(route: Route, now: u64) -> Self {
        Self {
            route,
            next: 1,
            since: now,
            jumped: None,
            jump_down: false,
            posture: 0,
            closest: f32::INFINITY,
            progress_at: now,
        }
    }

    /// Sets `r` to follow `steer` for the player `p`: forward along the
    /// bearing ([`Request::toward`]), crouch held for a crouch (running for
    /// the crouch and walking for the low crouch, as `accel` makes it with
    /// `always_run`, 4.1–4.2; standing, `accel` is left as it is),
    /// jump pressed and released on the next frame, and, unless `look`ing
    /// is the user's this frame, the camera turned over `dt` seconds.
    pub fn apply(
        &mut self,
        steer: &Steer,
        p: &PlayerState,
        always_run: bool,
        dt: f32,
        r: &mut Request,
    ) {
        let off = sub(steer.toward, p.position);
        let bearing = if off[0].hypot(off[2]) < 1e-3 {
            p.yaw
        } else {
            off[0].atan2(off[2])
        };
        // Within half a turn of the yaw, as the heading is.
        let tau = std::f32::consts::TAU;
        let mut turn = bearing - p.yaw;
        turn -= tau * (turn / tau).round();
        r.forward = true;
        r.toward = Some(p.yaw + turn);
        // Standing, the caller's walk or run stands.
        if steer.posture > 0 {
            r.crouch = true;
            r.accel = (steer.posture == 2) == always_run;
        }
        self.posture = steer.posture;
        r.jump_released |= self.jump_down;
        r.jump_pressed |= steer.jump;
        self.jump_down = steer.jump;
        if let Some(look) = steer.look
            && r.look == [0.0; 2]
        {
            r.look = turn_toward(look, p.yaw, p.pitch, dt);
        }
    }

    /// The posture the latest frame asked for: 0 standing, 1 crouch, 2
    /// low crouch.
    pub fn posture(&self) -> usize {
        self.posture
    }

    pub fn route(&self) -> &Route {
        &self.route
    }

    /// The waypoints still ahead.
    pub fn ahead(&self) -> &[Waypoint] {
        &self.route.waypoints[self.next.min(self.route.waypoints.len())..]
    }

    fn advance(&mut self, now: u64) {
        self.next += 1;
        self.since = now;
        self.jumped = None;
        self.closest = f32::INFINITY;
        self.progress_at = now;
    }

    /// Whether the feet at `p` have reached waypoint `w`.
    fn reached(w: &Waypoint, p: &PlayerState) -> bool {
        let at = p.position;
        let dy = (at[1] - w.at[1]).abs();
        match w.kind {
            Kind::Walk | Kind::Crouch | Kind::Crawl | Kind::Drop => {
                horizontal_distance(at, w.at) < REACH && dy < REACH_HEIGHT
            }
            Kind::JumpUp | Kind::JumpOver => {
                !p.airborne && horizontal_distance(at, w.at) < 0.45 && dy < 0.35
            }
            Kind::ClimbUp | Kind::ClimbDown => {
                !p.flags.climbing && dy < 0.4 && horizontal_distance(at, w.at) < 1.2
            }
        }
    }

    /// One frame for the player `p` at `now`.
    pub fn frame(&mut self, p: &PlayerState, now: u64) -> Follow {
        // Passed waypoints, and a take-off reached early enough to jump.
        while let Some(&here) = self.route.waypoints.get(self.next) {
            let take_off = self
                .route
                .waypoints
                .get(self.next + 1)
                .is_some_and(|j| match j.kind {
                    Kind::JumpUp => horizontal_distance(p.position, j.at) <= JUMP_UP_TRIGGER,
                    Kind::JumpOver => horizontal_distance(p.position, here.at) <= JUMP_OVER_TRIGGER,
                    _ => false,
                })
                && !p.airborne;
            if Self::reached(&here, p) || take_off {
                self.advance(now);
            } else {
                break;
            }
        }
        let w = &self.route.waypoints;
        let Some(&here) = w.get(self.next) else {
            return Follow::Done;
        };
        let before = w[self.next - 1];
        // Progress toward the waypoint under way.
        let distance = length(sub(here.at, p.position));
        if distance < self.closest - PROGRESS {
            self.closest = distance;
            self.progress_at = now;
        } else if now.saturating_sub(self.progress_at) >= STUCK_MS {
            return Follow::Stuck;
        }
        if matches!(
            here.kind,
            Kind::Walk | Kind::Crouch | Kind::Crawl | Kind::Drop
        ) && off_leg(before.at, here.at, p.position) > OFF_ROUTE
        {
            return Follow::Off;
        }
        // The posture: the leg's, or the next leg's once near its start.
        let mut posture = here.kind.posture();
        if let Some(n) = w.get(self.next + 1)
            && horizontal_distance(p.position, here.at) < POSTURE_AHEAD
        {
            posture = posture.max(n.kind.posture());
        }
        let mut steer = Steer {
            toward: here.at,
            posture,
            jump: false,
            look: None,
        };
        match here.kind {
            Kind::JumpUp | Kind::JumpOver => {
                let speed = p.velocity[0].hypot(p.velocity[2]);
                let ready = speed >= RUN_UP_SPEED || now.saturating_sub(self.since) >= RUN_UP_MS;
                let again = self
                    .jumped
                    .is_none_or(|t| !p.airborne && now.saturating_sub(t) >= JUMP_RETRY_MS);
                if !p.airborne && ready && again {
                    steer.jump = true;
                    self.jumped = Some(now);
                }
            }
            Kind::ClimbUp => {
                if let Some(l) = here.ladder {
                    climb_up(&l, p, &mut steer);
                }
            }
            Kind::ClimbDown => {
                if let Some(l) = here.ladder {
                    climb_down(&l, p, &mut steer);
                }
            }
            _ => {}
        }
        Follow::Steer(steer)
    }
}

/// How far `p` is from the leg `a` → `b`, horizontally.
fn off_leg(a: Vec3, b: Vec3, p: Vec3) -> f32 {
    let d = [b[0] - a[0], b[2] - a[2]];
    let q = [p[0] - a[0], p[2] - a[2]];
    let l2 = d[0] * d[0] + d[1] * d[1];
    let t = if l2 > 0.0 {
        ((q[0] * d[0] + q[1] * d[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (q[0] - d[0] * t).hypot(q[1] - d[1] * t)
}

/// Up ladder `l`: to the floor in front of its foot, then into it facing
/// it and looking up, until off it at the top.
fn climb_up(l: &LadderEnds, p: &PlayerState, steer: &mut Steer) {
    let foot = add(l.lower, scale(l.out, 0.5));
    let face = yaw_to(scale(l.out, -1.0));
    let below_top = p.position[1] < l.upper[1] - 0.3;
    if p.flags.climbing || below_top && horizontal_distance(p.position, foot) < 0.7 {
        steer.toward = sub(p.position, l.out);
        steer.look = Some([face, CLIMB_UP_PITCH]);
    } else if below_top {
        steer.toward = foot;
    }
}

/// Down ladder `l`: over the edge in front of its top, facing it and
/// looking down, until off it at its foot.
fn climb_down(l: &LadderEnds, p: &PlayerState, steer: &mut Steer) {
    let over = add(l.upper, scale(l.out, 0.55));
    let face = yaw_to(scale(l.out, -1.0));
    let above_foot = p.position[1] > l.lower[1] + 0.5;
    if p.flags.climbing {
        steer.toward = add(p.position, l.out);
        steer.look = Some([face, CLIMB_DOWN_PITCH]);
    } else if above_foot {
        steer.toward = over;
        if horizontal_distance(p.position, over) < 1.5 {
            steer.look = Some([face, CLIMB_DOWN_PITCH]);
        }
    }
}

/// This frame's turn toward `look` (yaw, pitch) from the camera's `yaw`
/// and `pitch` over `dt` seconds, at most [`TURN_RATE`].
pub fn turn_toward(look: [f32; 2], yaw: f32, pitch: f32, dt: f32) -> [f32; 2] {
    let tau = std::f32::consts::TAU;
    let mut dy = look[0] - yaw;
    dy -= tau * (dy / tau).round();
    let most = TURN_RATE * dt;
    [dy.clamp(-most, most), (look[1] - pitch).clamp(-most, most)]
}
