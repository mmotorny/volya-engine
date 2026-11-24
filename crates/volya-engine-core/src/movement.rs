// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The on-foot player's movement and first-person view
//! (docs/behavior/player-movement.md, cited by section): the movement
//! request becomes a movement state (walk, run, sprint, crouch, low crouch,
//! jump; sections 3–4) and a desired acceleration (5) that drives the
//! collision body of `collision::body` (docs/formats/level-cform.md) at the
//! fixed physics step (6); the eye follows the body (7, start-position.md
//! 4.2), looking turns the camera (8), and moving bobs it (9).
//!
//! [`Player::frame`] runs one rendered frame from a [`Request`], the
//! actions of the frame (a platform's input mapped onto the spec's actions
//! of section 2 elsewhere). Out of scope, as the spec allows a first
//! version (section 10): leaning, aiming, climbing, the character's
//! condition (always able to run, sprint and move), impulses from other
//! objects, and the body turning on its own (the heading follows the
//! camera, see [`HEADING_SLACK`]).

use std::f32::consts::{FRAC_PI_4, PI};
use std::io;

use volya_engine_formats::cp1251;
use volya_engine_formats::gamemtl::DEADLY;

use crate::camera::{self, Angles};
use crate::collision::CollisionWorld;
use crate::collision::body::{
    BodyConfig, PlayerBody, STEP, StepContacts, StepInput, drive_direction, side_force_coefficient,
    step_assisted_drive,
};
use crate::collision::geometry::{Vec3, add, cross, dot, length, normalize, scale, sub};
use crate::config::{Config, Section, value};

/// Gravity, m/s² downward: a constant of the game (6.1).
pub const GRAVITY: f32 = 19.62;
/// The movement time's cap, ms (3.2).
pub const MOVEMENT_TIME_CAP_MS: u64 = 100;
/// The display frame time's cap and smoothing weight of the new time (3.2).
pub const FRAME_TIME_CAP: f32 = 0.1;
pub const FRAME_TIME_WEIGHT: f32 = 0.9;
/// Physics steps per frame at most (6.1: `dt_f ≤ 0.1`, `h = 0.01`).
pub const MAX_STEPS: u32 = 10;
/// Opposite keys cancel, and a direction this short is none (5.1).
pub const DIRECTION_EPSILON: f32 = 0.000_01;
/// The ground speed cap is this × `|a|` (6.3); otherwise [`FREE_SPEED`].
pub const SPEED_CAP_FACTOR: f32 = 0.1;
pub const FREE_SPEED: f32 = 150.0;
/// Below this cap the velocity is zeroed (6.3), and vectors shorter than
/// this are used as they are in the hand-over (6.1).
pub const TINY: f32 = 0.000_000_1;
/// The hand-over ignores a new acceleration whose direction differs by less
/// than this per component and whose length by less than
/// [`HAND_OVER_LENGTH`] (6.1).
pub const HAND_OVER_DIRECTION: f32 = 0.05;
pub const HAND_OVER_LENGTH: f32 = 0.5;
/// Driving force per kilogram and friction factor on walkable ground, and
/// with contacts but no walkable ground (6.2).
pub const DRIVE: f32 = 25.0;
pub const DRIVE_STEEP: f32 = 37.5;
/// Down force per kilogram while moving with no contact (6.2).
pub const DOWN_FORCE: f32 = 50.0;
/// The total force is limited to this × `m / h` (6.1).
pub const FORCE_LIMIT: f32 = 5.0;
/// The drag's coefficient per m/s of speed (6.3).
pub const DRAG: f32 = 0.0002;
/// Jump cooldown, and how much of it must elapse on the ground before the
/// jump flag clears (4.4).
pub const JUMP_COOLDOWN: f32 = 0.3;
pub const JUMP_FLAG_TIME: f32 = 0.1;
/// A take-off needs a recorded contact normal steeper than this (6.6).
pub const TAKE_OFF_NORMAL_Y: f32 = 0.5;
/// The take-off keeps this × the horizontal velocity, plus this × the
/// input direction (6.6).
pub const TAKE_OFF_KEEP: f32 = 1.2;
pub const TAKE_OFF_INPUT: f32 = 0.2;
/// Jump assistance (6.6): the push along the input while short of this
/// progress, its force (N), and the brake's coefficient.
pub const JUMP_ASSIST_DISTANCE: f32 = 0.3;
pub const JUMP_ASSIST_FORCE: f32 = 1000.0;
pub const JUMP_BRAKE: f32 = 3000.0;
/// Falling starts this long into an airborne phase (6.7).
pub const FALL_DELAY: f32 = 0.2;
/// Landing flags: above this impact speed, for this long, or the hard
/// landing's time (6.7).
pub const LANDING_SPEED: f32 = 4.0;
pub const LANDING_TIME: f32 = 0.1;
pub const HARD_LANDING_TIME: f32 = 0.3;
/// The state's direction flags clear below this speed (9.1).
pub const SLOW_SPEED: f32 = 0.2;
/// Mouse look: `0.02 × K` with `K = 1`, the vertical factor, and the field
/// of view the sensitivity is relative to (8.1).
pub const MOUSE_SCALE: f32 = 0.02;
pub const MOUSE_PITCH_FACTOR: f32 = 0.75;
pub const MOUSE_FOV_DEGREES: f32 = 67.5;
/// Bobbing's fade rate, crouch factor and the fade below which it stops
/// (9.1).
pub const BOB_FADE_RATE: f32 = 5.0;
pub const BOB_CROUCH: f32 = 0.75;
pub const BOB_MIN_FADE: f32 = 0.000_01;
/// The landing effect's rate and height (9.2).
pub const LANDING_EFFECT_RATE: f32 = 3.5;
pub const LANDING_EFFECT_HEIGHT: f32 = 0.15;
/// The eye's lag behind a rising position (start-position.md 4.2).
pub const EYE_LAG: f32 = 0.2;
pub const EYE_CATCH_UP: f32 = 1.5;
/// The body turns once the camera's yaw is more than π/4 away from its
/// heading (5.3). Body turning is out of scope (section 10); our choice is
/// to keep the heading within π/4 of the yaw.
pub const HEADING_SLACK: f32 = FRAC_PI_4;
/// `mouse_sens`'s range (8.1).
pub const MOUSE_SENS_RANGE: [f32; 2] = [0.05, 0.6];

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The values of section 1 read from the game's configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementConfig {
    pub walk_accel: f32,
    pub run_coef: f32,
    pub run_back_coef: f32,
    pub walk_back_coef: f32,
    pub crouch_coef: f32,
    pub sprint_koef: f32,
    pub walk_strafe_coef: f32,
    pub run_strafe_coef: f32,
    pub jump_speed: f32,
    pub air_control_param: f32,
    pub camera_height_factor: f32,
    pub crash_speed_min: f32,
    pub crash_speed_max: f32,
    /// `ph_box0_size` … `ph_box2_size` and `ph_mass`.
    pub body: BodyConfig,
    /// `[actor_firsteye_cam]`: `lim_pitch` and `rot_speed`.
    pub lim_pitch: [f32; 2],
    pub rot_speed: [f32; 3],
    pub bobbing: Bobbing,
}

/// `[bobbing_effector]` (9.1). Limping is out of scope, so its values are
/// not read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bobbing {
    pub run_amplitude: f32,
    pub run_speed: f32,
    pub walk_amplitude: f32,
    pub walk_speed: f32,
}

fn required<'a>(section: &'a Section, key: &str) -> io::Result<&'a str> {
    section
        .get(key)
        .flatten()
        .ok_or_else(|| invalid(format!("[{}] has no {key}", section.name())))
}

fn section<'a>(config: &'a Config, name: &str) -> io::Result<&'a Section> {
    config
        .section(name)
        .ok_or_else(|| invalid(format!("no section [{name}]")))
}

impl MovementConfig {
    /// From the system configuration: the actor's section `actor` (named by
    /// its spawn record), `[actor_firsteye_cam]` and `[bobbing_effector]`.
    /// A missing key is an error (the spec does not say what the game does
    /// then), except the strafe factors, 1.0 when absent (1.1).
    pub fn read(system: &Config, actor: &str) -> io::Result<Self> {
        let a = section(system, actor)?;
        let float = |key: &str| required(a, key).map(value::float);
        let optional = |key: &str| a.get(key).flatten().map_or(1.0, value::float);
        let cam = section(system, "actor_firsteye_cam")?;
        let bob = section(system, "bobbing_effector")?;
        let bob_float = |key: &str| required(bob, key).map(value::float);
        Ok(Self {
            walk_accel: float("walk_accel")?,
            run_coef: float("run_coef")?,
            run_back_coef: float("run_back_coef")?,
            walk_back_coef: float("walk_back_coef")?,
            crouch_coef: float("crouch_coef")?,
            sprint_koef: float("sprint_koef")?,
            walk_strafe_coef: optional("walk_strafe_coef"),
            run_strafe_coef: optional("run_strafe_coef"),
            jump_speed: float("jump_speed")?,
            air_control_param: float("air_control_param")?,
            camera_height_factor: float("camera_height_factor")?,
            crash_speed_min: float("ph_crash_speed_min")?,
            crash_speed_max: float("ph_crash_speed_max")?,
            body: BodyConfig::from_section(a)?,
            lim_pitch: value::floats(required(cam, "lim_pitch")?),
            rot_speed: value::floats(required(cam, "rot_speed")?),
            bobbing: Bobbing {
                run_amplitude: bob_float("run_amplitude")?,
                run_speed: bob_float("run_speed")?,
                walk_amplitude: bob_float("walk_amplitude")?,
                walk_speed: bob_float("walk_speed")?,
            },
        })
    }

    /// The eye's height above the feet in box `index`: `2 × size.y ×
    /// camera_height_factor` (section 7).
    pub fn eye_height(&self, index: usize) -> f32 {
        2.0 * self.body.boxes[index][1] * self.camera_height_factor
    }
}

/// The console settings movement reads (1.2), from their built-in values
/// with the user settings (`user.ltx`) applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub always_run: bool,
    pub mouse_sens: f32,
    pub mouse_invert: bool,
    /// Camera inertia (start-position.md section 6).
    pub cam_inert: f32,
}

impl Default for Settings {
    /// The built-in values (1.2).
    fn default() -> Self {
        Self {
            always_run: true,
            mouse_sens: 0.12,
            mouse_invert: false,
            cam_inert: 0.7,
        }
    }
}

impl Settings {
    /// The built-in values with the `<name> <value>` lines of `files`
    /// applied in order. A flag is on for `on` (any other value is off, our
    /// choice). `mouse_sens` is limited to its range (8.1) and `cam_inert`
    /// to 0 … 1; that out-of-range values are limited rather than refused
    /// is our choice.
    pub fn read(files: &[&[u8]]) -> Self {
        let mut s = Self::default();
        for file in files {
            for line in cp1251::decode(file).lines() {
                let mut words = line.split_whitespace();
                let (Some(name), Some(v)) = (words.next(), words.next()) else {
                    continue;
                };
                match name {
                    "g_always_run" => s.always_run = v.eq_ignore_ascii_case("on"),
                    "mouse_invert" => s.mouse_invert = v.eq_ignore_ascii_case("on"),
                    "mouse_sens" => {
                        s.mouse_sens =
                            value::float(v).clamp(MOUSE_SENS_RANGE[0], MOUSE_SENS_RANGE[1])
                    }
                    "cam_inert" => s.cam_inert = value::float(v).clamp(0.0, 1.0),
                    _ => {}
                }
            }
        }
        s
    }
}

/// One frame's actions (section 2), as a platform's input mapping makes
/// them: held actions as held, press actions as this frame's presses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Request {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    /// `accel` held: walk while running is the default (4.1).
    pub accel: bool,
    /// `crouch` held.
    pub crouch: bool,
    /// `crouch` released this frame (it releases the crouch toggle, 4.2).
    pub crouch_released: bool,
    pub crouch_toggle: bool,
    pub sprint_toggle: bool,
    pub jump_pressed: bool,
    pub jump_released: bool,
    /// `wpn_fire` pressed: cancels a requested sprint (4.3).
    pub fire: bool,
    /// Relative mouse motion this frame, in counts (8.1).
    pub mouse: [f32; 2],
    /// Keyboard look, −1 … +1 of `rot_speed`: x turns right, y looks down
    /// (8.2). Analog sources may use the range between.
    pub turn: [f32; 2],
}

/// The movement state (section 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub accel: bool,
    pub crouch: bool,
    pub sprint: bool,
    pub jump: bool,
    pub falling: bool,
    pub landing: bool,
}

impl Flags {
    fn any_direction(&self) -> bool {
        self.forward || self.back || self.left || self.right
    }

    fn clear_directions(&mut self) {
        self.forward = false;
        self.back = false;
        self.left = false;
        self.right = false;
    }
}

/// What the camera shows this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub eye: Vec3,
    pub direction: Vec3,
    pub up: Vec3,
}

/// The player in a level.
#[derive(Clone, Debug)]
pub struct Player {
    pub config: MovementConfig,
    pub settings: Settings,
    body: PlayerBody,
    /// The camera's yaw and pitch (8; start-position.md 5).
    angles: Angles,
    /// The body's heading `θ` (5.3).
    heading: f32,
    state: Flags,
    /// The request's persistent flags (3): sprint and jump, set and
    /// cleared by presses, and the crouch toggle (4.2).
    sprint_request: bool,
    jump_request: bool,
    crouch_toggle: bool,
    /// The jump key has been released since the previous jump or landing
    /// (4.4).
    jump_released: bool,
    cooldown: f32,
    /// The falling countdown (6.7); its value at creation is not
    /// established, [`FALL_DELAY`] is our choice.
    fall_countdown: f32,
    /// Time left of the landing flag (6.7).
    landing_left: f32,
    /// The acceleration held by the body `a_k` and the speed limit from
    /// the latest offered `|a|` (6.1).
    accepted: Vec3,
    speed_limit: f32,
    /// The physics accumulator, s (6.1).
    accumulator: f32,
    /// The previous frame's `dt_f` and the engine clock of the previous
    /// frame, ms (3.2).
    frame_time: f32,
    last_ms: u64,
    /// The take-off velocity to set at the next physics step (6.6).
    take_off: Option<Vec3>,
    /// The take-off position while jump assistance lasts (6.6).
    jump_assist: Option<Vec3>,
    /// The most upward contact normal of the last step with contacts
    /// (6.6).
    last_ground: Option<Vec3>,
    /// The current airborne phase has touched something (6.7, 9.2).
    touched: bool,
    /// The feet's height as the eye follows it (start-position.md 4.2).
    eye_feet: f32,
    /// Bobbing's time and fade (9.1).
    bob_time: f32,
    bob_fade: f32,
    /// The landing effect's time and strength (9.2).
    landing_effect: Option<(f32, f32)>,
    /// The previous frame's eye and direction, for camera inertia.
    inert: Option<(Vec3, Vec3)>,
    view: View,
}

/// What the engine interface reports about the player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub airborne: bool,
    pub flags: Flags,
    /// The collision box: 0 standing, 1 crouch, 2 low crouch.
    pub posture: usize,
    pub sprint_requested: bool,
    pub crouch_toggled: bool,
}

fn unit_or_tiny(v: Vec3) -> Vec3 {
    let l = length(v);
    if l < TINY { v } else { scale(v, 1.0 / l) }
}

fn horizontal(v: Vec3) -> Vec3 {
    [v[0], 0.0, v[2]]
}

impl Player {
    /// A player standing at `position` (feet) with the spawn's camera
    /// angles, at engine clock `now` (ms).
    pub fn new(
        config: MovementConfig,
        settings: Settings,
        position: Vec3,
        angles: Angles,
        now: u64,
    ) -> Self {
        let mut p = Self {
            config,
            settings,
            body: PlayerBody::new(config.body, position),
            angles,
            heading: angles.yaw,
            state: Flags::default(),
            sprint_request: false,
            jump_request: false,
            crouch_toggle: false,
            jump_released: true,
            cooldown: 0.0,
            fall_countdown: FALL_DELAY,
            landing_left: 0.0,
            accepted: [0.0; 3],
            speed_limit: 0.0,
            accumulator: 0.0,
            frame_time: 0.0,
            last_ms: now,
            take_off: None,
            jump_assist: None,
            last_ground: None,
            touched: false,
            eye_feet: position[1],
            bob_time: 0.0,
            bob_fade: 0.0,
            landing_effect: None,
            inert: None,
            view: View {
                eye: [0.0; 3],
                direction: [0.0, 0.0, 1.0],
                up: [0.0, 1.0, 0.0],
            },
        };
        p.view = p.compute_view(0.0);
        p
    }

    /// Places the feet at `position` with camera `angles`, at rest (tests
    /// and debugging).
    pub fn place(&mut self, position: Vec3, angles: Angles) {
        self.body.set_position(position);
        self.body.set_velocity([0.0; 3]);
        self.angles = angles;
        self.heading = angles.yaw;
        self.eye_feet = position[1];
        self.inert = None;
        self.view = self.compute_view(0.0);
    }

    pub fn view(&self) -> View {
        self.view
    }

    pub fn angles(&self) -> Angles {
        self.angles
    }

    pub fn body(&self) -> &PlayerBody {
        &self.body
    }

    pub fn state(&self) -> PlayerState {
        PlayerState {
            position: self.body.position(),
            velocity: self.body.velocity(),
            airborne: self.body.airborne(),
            flags: self.state,
            posture: self.body.box_index(),
            sprint_requested: self.sprint_request,
            crouch_toggled: self.crouch_toggle,
        }
    }

    /// A paused frame at `now`: nothing runs and `dt_f` is 0 (3.2).
    pub fn paused(&mut self, now: u64) {
        self.last_ms = now;
        self.frame_time = 0.0;
    }

    /// Whether flags `f` run (4.1); leaning and aiming are out of scope, so
    /// only `g_always_run` and `accel` decide, and the character is always
    /// able to run.
    fn runs(&self, accel: bool) -> bool {
        self.settings.always_run != accel
    }

    /// One rendered frame at engine clock `now` (ms) with `request`.
    pub fn frame(&mut self, world: &CollisionWorld, now: u64, request: &Request) -> View {
        // 3.2: the two frame times.
        let ms = now.saturating_sub(self.last_ms);
        self.last_ms = now;
        let dt_m = ms.min(MOVEMENT_TIME_CAP_MS) as f32 * 0.001;
        let t = ms as f32 * 0.001;
        let dt_f = (FRAME_TIME_WEIGHT * t + (1.0 - FRAME_TIME_WEIGHT) * self.frame_time)
            .min(FRAME_TIME_CAP);
        self.frame_time = dt_f;

        // The order within a frame is our choice where the spec gives none:
        // look (8), the movement update (3-5), the hand-over and take-off
        // (6.1, 6.6), the physics steps (6.1), the direction clearing of 9.1
        // (after the steps, on the speed they leave: our choice), the camera
        // (7, 9).
        self.look(request, dt_f);
        let (a, take_off) = self.movement_update(world, now, request, dt_m);
        self.hand_over(a);
        if take_off {
            self.evaluate_take_off();
        }
        self.physics(world, dt_f);
        // 9.1: blocked, the state stops "moving".
        let v = self.body.velocity();
        if length(v) < SLOW_SPEED && !self.state.jump && !self.state.falling {
            self.state.clear_directions();
        }
        self.view = self.compute_view(dt_f);
        self.view
    }

    /// Section 8: mouse and keyboard look, pitch limited.
    fn look(&mut self, request: &Request, dt_f: f32) {
        let s = self.mouse_scale();
        let invert = if self.settings.mouse_invert {
            -1.0
        } else {
            1.0
        };
        let rot = self.config.rot_speed;
        self.angles.yaw += request.mouse[0] * s + request.turn[0] * rot[0] * dt_f;
        self.angles.pitch +=
            request.mouse[1] * s * MOUSE_PITCH_FACTOR * invert + request.turn[1] * rot[1] * dt_f;
        let [lo, hi] = self.config.lim_pitch;
        self.angles.pitch = self.angles.pitch.clamp(lo, hi);
    }

    /// Radians of yaw per mouse count (8.1), not aiming and with no active
    /// item (`c = 1`).
    pub fn mouse_scale(&self) -> f32 {
        self.settings.mouse_sens * MOUSE_SCALE * (camera::FOV_Y_DEGREES / MOUSE_FOV_DEGREES)
    }

    /// Sections 3–5: the state from the request, and the desired
    /// acceleration; whether a jump starts.
    fn movement_update(
        &mut self,
        world: &CollisionWorld,
        now: u64,
        request: &Request,
        dt_m: f32,
    ) -> (Vec3, bool) {
        // Press actions (4.2–4.4).
        if request.sprint_toggle {
            self.sprint_request = !self.sprint_request;
        }
        if request.fire {
            self.sprint_request = false;
        }
        if request.crouch_toggle {
            self.crouch_toggle = !self.crouch_toggle;
        }
        if request.crouch_released {
            self.crouch_toggle = false;
        }
        if request.jump_pressed {
            self.jump_request = true;
        }
        if request.jump_released {
            self.jump_request = false;
            self.jump_released = true;
        }
        let crouch = request.crouch || self.crouch_toggle;
        let air = self.body.airborne();

        // 3.1 item 1: the falling countdown (6.7).
        if air && !self.state.falling {
            self.fall_countdown -= dt_m;
            if self.fall_countdown <= 0.0 {
                self.state.falling = true;
                self.state.jump = false;
                self.fall_countdown = FALL_DELAY;
            }
        }
        // The landing flag's time runs out (its clock is our choice).
        self.landing_left = (self.landing_left - dt_m).max(0.0);
        self.state.landing = self.landing_left > 0.0;

        // 5.1: the local direction (the unable-to-move check of 5.4 always
        // passes: condition is out of scope).
        let flag = |b: bool| f32::from(u8::from(b));
        let z = flag(request.forward) - flag(request.back);
        let x = flag(request.right) - flag(request.left);

        let mut take_off = false;
        let a = if air {
            // 3.1: the raw direction turned by the heading; the state keeps
            // its direction, accel and sprint flags.
            self.turn_by_heading(x, z)
        } else {
            let s = &mut self.state;
            s.forward = request.forward;
            s.back = request.back;
            s.left = request.left;
            s.right = request.right;
            if z.abs() < DIRECTION_EPSILON {
                s.forward = false;
                s.back = false;
            }
            if x.abs() < DIRECTION_EPSILON {
                s.left = false;
                s.right = false;
            }
            // 4.2: entering a crouch, and changing its height while crouched
            // (also on the frame the crouch request ends, before standing up).
            let request_runs = self.runs(request.accel);
            let mut keep_accel = false;
            if crouch && !self.state.crouch {
                let target = if request_runs { 1 } else { 2 };
                if self.body.request_box(world, target, now) {
                    self.state.crouch = true;
                }
            } else if self.state.crouch {
                let state_runs = self.runs(self.state.accel);
                if !state_runs && request_runs {
                    keep_accel = !self.body.request_box(world, 1, now);
                } else if state_runs && !request_runs {
                    self.body.request_box(world, 2, now);
                }
            }
            if !keep_accel {
                self.state.accel = request.accel;
            }
            // 4.3: sprint.
            let s = &mut self.state;
            let cancel = !(s.forward || s.left || s.right) || s.crouch || !request_runs;
            if cancel {
                self.sprint_request = false;
                s.sprint = false;
            } else {
                s.sprint = self.sprint_request && !s.left && !s.right;
            }
            // 4.4: jump.
            self.cooldown -= dt_m;
            if self.jump_request && !s.jump && self.cooldown <= 0.0 && self.jump_released {
                s.jump = true;
                self.cooldown = JUMP_COOLDOWN;
                self.jump_released = false;
                take_off = true;
            } else if s.jump && JUMP_COOLDOWN - self.cooldown > JUMP_FLAG_TIME {
                s.jump = false;
                self.cooldown = JUMP_COOLDOWN;
            }
            self.desired_acceleration(x, z)
        };
        // 4.2: standing up, on every frame.
        if self.state.crouch && !crouch && self.body.request_box(world, 0, now) {
            self.state.crouch = false;
        }
        // 5.3: the heading for the next frame.
        if self.state.any_direction() {
            self.heading = self.angles.yaw;
        } else {
            let off = self.angles.yaw - self.heading;
            if off.abs() > HEADING_SLACK {
                self.heading = self.angles.yaw - HEADING_SLACK.copysign(off);
            }
        }
        (a, take_off)
    }

    /// Section 5 items 2 and 3 on a frame that is not an air frame.
    fn desired_acceleration(&self, x: f32, z: f32) -> Vec3 {
        let s = &self.state;
        let len = x.hypot(z);
        if !s.any_direction() || len <= DIRECTION_EPSILON {
            return [0.0; 3];
        }
        let c = &self.config;
        let running = self.runs(s.accel);
        let m = match (running, s.back) {
            (true, true) => c.run_back_coef,
            (true, false) => c.run_coef,
            (false, true) => c.walk_back_coef,
            (false, false) => 1.0,
        };
        let crouch = if s.crouch { c.crouch_coef } else { 1.0 };
        let sprint = if s.sprint { c.sprint_koef } else { 1.0 };
        let strafe = match (s.left || s.right, s.crouch, running) {
            (true, false, true) => c.run_strafe_coef,
            (true, false, false) => c.walk_strafe_coef,
            _ => 1.0,
        };
        let magnitude = c.walk_accel * m * crouch * sprint * strafe;
        scale(self.turn_by_heading(x / len, z / len), magnitude)
    }

    /// `(x, 0, z)` turned by the heading (5.3).
    fn turn_by_heading(&self, x: f32, z: f32) -> Vec3 {
        let (sin, cos) = self.heading.sin_cos();
        [x * cos + z * sin, 0.0, -x * sin + z * cos]
    }

    /// 6.1: the speed limit from the offered `|a|`; the vector accepted
    /// unless close to the one held.
    fn hand_over(&mut self, a: Vec3) {
        self.speed_limit = SPEED_CAP_FACTOR * length(a);
        let (new, old) = (unit_or_tiny(a), unit_or_tiny(self.accepted));
        let close = (0..3).all(|i| (new[i] - old[i]).abs() < HAND_OVER_DIRECTION)
            && (length(a) - length(self.accepted)).abs() < HAND_OVER_LENGTH;
        if !close {
            self.accepted = a;
        }
    }

    /// 6.6: a take-off is accepted on a recorded contact normal steeper than
    /// [`TAKE_OFF_NORMAL_Y`] while not airborne.
    fn evaluate_take_off(&mut self) {
        let ground = self.last_ground.is_some_and(|n| n[1] > TAKE_OFF_NORMAL_Y);
        if self.body.airborne() || !ground {
            return;
        }
        let v = self.body.velocity();
        let a = scale(self.accepted, 1.0 / length(self.accepted).max(1.0));
        self.take_off = Some([
            TAKE_OFF_KEEP * v[0] + TAKE_OFF_INPUT * a[0],
            self.config.jump_speed,
            TAKE_OFF_KEEP * v[2] + TAKE_OFF_INPUT * a[2],
        ]);
    }

    /// 6.1: the frame's physics steps.
    fn physics(&mut self, world: &CollisionWorld, dt_f: f32) {
        self.accumulator += dt_f;
        let mut steps = 0;
        while self.accumulator >= STEP && steps < MAX_STEPS {
            self.accumulator -= STEP;
            steps += 1;
            self.step(world);
        }
        // Whole steps only; what did not fit carries over (at most a step's
        // worth beyond MAX_STEPS, which `dt_f ≤ 0.1` never leaves).
        self.accumulator = self.accumulator.min(STEP * MAX_STEPS as f32);
    }

    fn step(&mut self, world: &CollisionWorld) {
        let m = self.config.body.mass;
        let a = self.accepted;
        let take_off = self.take_off.take();
        if let Some(v) = take_off {
            self.body.set_velocity(v);
            self.jump_assist = Some(self.body.position());
        }
        let airborne = self.body.airborne();
        let before = self.body.position();
        let c = self.body.begin_step(
            world,
            &StepInput {
                acceleration: a,
                jump_requested: take_off.is_some(),
            },
        );
        if c.ground.is_some() {
            self.last_ground = c.ground;
        }
        let force = self.forces(&c, airborne, take_off.is_some());
        let r = self.body.finish_step(force, take_off.is_some());

        // 6.3: the speed cap, then the drag.
        let moving = c.moving;
        let limit = if moving && !r.airborne {
            self.speed_limit
        } else {
            FREE_SPEED
        };
        let mut v = self.body.velocity();
        let speed = length(v);
        if speed > limit {
            let loose = r.airborne || !c.walkable;
            if limit < TINY {
                v = [0.0; 3];
            } else if loose && v[1] < 0.0 && v[1] > -FREE_SPEED {
                let k = limit / speed;
                v = [v[0] * k, v[1], v[2] * k];
            } else {
                let limited = scale(v, limit / speed);
                self.body.move_by(scale(sub(limited, v), STEP));
                v = limited;
            }
            if moving && !r.airborne {
                let target = add(before, scale(v, STEP));
                self.body.move_by(sub(target, self.body.position()));
            }
        }
        let speed = length(v);
        let k = (DRAG * speed).min(m / STEP);
        v = sub(v, scale(v, k * STEP / m));
        self.body.set_velocity(v);

        // 6.6: jump assistance ends on walkable ground.
        if take_off.is_none() && c.walkable {
            self.jump_assist = None;
        }
        // 6.7, 9.2: the step the character first touches something while
        // airborne (the step applying a take-off is not yet airborne).
        if airborne && take_off.is_none() && c.ground.is_some() && !self.touched {
            self.touched = true;
            self.land(world, c.impact_speed);
        } else if r.landed && self.state.falling {
            // Our reading: a contact held from the take-off (a wall) is the
            // first touch, and the falling flag set after it would then never
            // clear (the spec clears it only on that touch); so the step
            // ending the airborne phase (level-cform.md 6.4) lands too while
            // the character is falling.
            self.land(world, c.impact_speed);
        }
        if !r.airborne {
            self.touched = false;
        }
    }

    /// The character's forces of a step (6.2–6.6), gravity included, within
    /// the force limit of 6.1.
    fn forces(&self, c: &StepContacts, airborne: bool, taking_off: bool) -> Vec3 {
        let m = self.config.body.mass;
        let f = c.traction;
        let a = self.accepted;
        let mut force = [0.0, -GRAVITY * m, 0.0];
        let input = normalize(horizontal(c.direction));
        let side = |force: &mut Vec3| {
            if let Some(d) = input {
                let s = cross(d, [0.0, 1.0, 0.0]);
                let k = side_force_coefficient(c);
                let along = dot(self.body.velocity(), s);
                *force = add(*force, scale(s, -k * f * along));
            }
        };
        if c.moving && !airborne {
            let drive = if c.walkable {
                scale(drive_direction(c), DRIVE * f * m)
            } else {
                scale(drive_direction(c), DRIVE_STEEP * f * m)
            };
            let drive = if c.step_assist {
                step_assisted_drive(drive, c, m)
            } else {
                drive
            };
            force = add(force, drive);
            side(&mut force);
            if c.ground.is_none() {
                force[1] -= DOWN_FORCE * m;
            }
        } else if c.moving && c.step_assist {
            side(&mut force);
        }
        if airborne {
            force = add(force, scale(a, m * self.config.air_control_param));
        }
        // 6.6: jump assistance; not on the step applying the take-off, which
        // starts it (our choice).
        if let Some(p0) = self.jump_assist.filter(|_| !taking_off) {
            let ah = horizontal(a);
            let l = length(ah);
            if l > 0.0 {
                let along = scale(ah, 1.0 / l);
                let progress = dot(along, horizontal(sub(self.body.position(), p0)));
                if progress < JUMP_ASSIST_DISTANCE {
                    force = add(force, scale(along, JUMP_ASSIST_FORCE));
                }
                let vh = horizontal(self.body.velocity());
                let against = dot(ah, vh);
                if against < 0.0 {
                    force = add(force, scale(vh, JUMP_BRAKE * against / (l * dot(vh, vh))));
                }
            }
        }
        let limit = FORCE_LIMIT * m / STEP;
        let total = length(force);
        if total > limit {
            force = scale(force, limit / total);
        }
        force
    }

    /// 6.7 and 9.2: the first touch of an airborne phase at impact speed
    /// `v_i`.
    fn land(&mut self, world: &CollisionWorld, v_i: f32) {
        let deadly = self
            .body
            .ground_material()
            .and_then(|m| world.materials().get(m as usize))
            .is_some_and(|m| m.has(DEADLY));
        if self.state.falling && v_i > LANDING_SPEED {
            self.landing_left = if v_i > self.config.crash_speed_min || deadly {
                HARD_LANDING_TIME
            } else {
                LANDING_TIME
            };
            self.state.landing = true;
        }
        self.state.falling = false;
        self.state.jump = false;
        self.cooldown = JUMP_COOLDOWN;
        if self.jump_request {
            self.jump_released = false;
        }
        let p = (v_i / self.config.crash_speed_max).clamp(0.0, 1.0).powi(2);
        self.landing_effect = Some((0.0, p));
    }

    /// The camera: the eye above the feet (7, with the lag of
    /// start-position.md 4.2), then bobbing (9.1), the landing effect (9.2)
    /// and camera inertia (start-position.md 6).
    fn compute_view(&mut self, dt_f: f32) -> View {
        let feet = self.body.position();
        if self.body.airborne() || feet[1] <= self.eye_feet {
            self.eye_feet = feet[1];
        } else {
            self.eye_feet = (self.eye_feet + EYE_CATCH_UP * dt_f).clamp(feet[1] - EYE_LAG, feet[1]);
        }
        let h = self.config.eye_height(self.body.box_index());
        let mut eye = [feet[0], self.eye_feet + h, feet[2]];
        let (mut d, mut u) = self.angles.direction_up();

        // 9.1: bobbing.
        self.bob_time += dt_f;
        if self.state.any_direction() {
            self.bob_fade = if self.bob_fade < 1.0 {
                self.bob_fade + BOB_FADE_RATE * dt_f
            } else {
                1.0
            };
        } else if self.bob_fade > 0.0 {
            self.bob_fade -= BOB_FADE_RATE * dt_f;
        } else {
            self.bob_fade = 0.0;
        }
        let w = self.bob_fade;
        if w.abs() >= BOB_MIN_FADE {
            let k = if self.state.crouch { BOB_CROUCH } else { 1.0 };
            let bob = &self.config.bobbing;
            let (amplitude, rate) = if self.runs(self.state.accel) {
                (bob.run_amplitude, bob.run_speed)
            } else {
                (bob.walk_amplitude, bob.walk_speed)
            };
            let phi = rate * k * self.bob_time;
            let b = amplitude * k;
            let delta = (phi.sin() * b).abs() * w;
            let rho = phi.cos() * w * b;
            eye[1] += delta;
            let r = cross(u, d);
            let (sr, cr) = rho.sin_cos();
            let (sd, cd) = delta.sin_cos();
            let d2 = add(add(scale(d, cr * cd), scale(r, -sr * cd)), scale(u, sd));
            let u2 = add(
                add(scale(d, sr * sr - cr * cr * sd), scale(u, cr * cd)),
                scale(r, cr * sr * sd + sr * cr),
            );
            (d, u) = (d2, u2);
        }

        // 9.2: the landing effect.
        if let Some((t, p)) = &mut self.landing_effect {
            *t += LANDING_EFFECT_RATE * dt_f;
            if *t < 1.0 {
                eye[1] -= ((*t + 1.0) * PI).sin() * *p * LANDING_EFFECT_HEIGHT;
            } else {
                self.landing_effect = None;
            }
        }

        // Camera inertia (start-position.md 6).
        if self.settings.cam_inert > 0.0
            && let Some((old_eye, old_d)) = self.inert
        {
            eye = camera::inert(old_eye, eye, self.settings.cam_inert);
            d = camera::inert(old_d, d, self.settings.cam_inert);
        }
        self.inert = Some((eye, d));
        View {
            eye,
            direction: d,
            up: u,
        }
    }
}

#[cfg(test)]
mod tests;
