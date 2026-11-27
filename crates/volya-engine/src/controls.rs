// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's controls in a level: every input path mapped onto the
//! movement and look actions of docs/behavior/player-movement.md section 2,
//! making a [`movement::Request`] each frame.
//!
//! **Keyboard and mouse** follow the game: the effective bindings of
//! `config\default_controls.ltx` with the user settings applied on top
//! ([`Bindings::read`], section 2.1), and mouse look from relative motion
//! (pointer lock on the web), one count per screen unit (our choice).
//!
//! **Gamepad and touch are our design** (the original has neither, section
//! 2.1), mapped onto the same actions and kept conservative:
//!
//! | Action | Gamepad | Touch, standard (the default) | Touch, experimental |
//! |---|---|---|---|
//! | move | left stick | the move stick (left half) | a tap in the level: forward toward where it meets the level ([`TapMove`]) |
//! | walk instead of run | the stick pushed less than [`RUN_DEFLECTION`], kept after it is released | the same | a trip in the low crouch walks; otherwise it runs |
//! | look | right stick, at `rot_speed` at full deflection (8.2) | a drag on the right half, or one begun on Jump or Crouch, as mouse counts (8.1) through [`look_gain`] | a drag anywhere in the level, or one begun on Jump or Crouch, as direct manipulation with momentum ([`TouchLookDrag`]) |
//! | jump | south (A / cross) | Jump | Jump |
//! | crouch | west (X / square), as `crouch_toggle` | Crouch, as `crouch_toggle` | the same |
//! | sprint | left stick pressed, as `sprint_toggle` | the stick pushed forward past its rim, kept by the sprint lock ([`SprintLock`]) | a double tap: that trip sprints |
//! | open the menu | east (B / circle), as the key bound to `quit` | Menu | Menu |
//! | open the inventory screen | north (Y / triangle) | Inventory, at the top right | the same |
//! | `torch` (docs/behavior/torch.md 8) | d-pad up | Torch, at the top right beside Inventory (#363) | the same |
//! | `night_vision` (15.3) | d-pad down | Night vision, beside Torch, shown while it can act (#363) | the same |
//! | open and close the PDA as `active_jobs` (docs/behavior/pda-window.md 4.1) | d-pad left; in the window, east (B / circle) closes it as the key bound to `quit` | PDA, at the top right beside Inventory, shown over the open window too (#730) | the same |
//!
//! The page offers both touch controls, standard by default, by the app's
//! setting (#802); each sends its own events ([`Input::touch_stick`] and
//! [`Input::touch_look_moves`]; [`Input::touch_looks`]), and these controls
//! act on whichever arrive.
//!
//! Climbing ladders (docs/behavior/climbing.md) has no control of its own
//! on any path: forward climbs up, back or looking down past 20° climbs
//! down, sideways lets go (on a leaning ladder a stick's diagonal sector
//! too, as W with A does on the keys), and jump jumps off, all through the
//! actions above.
//!
//! While a screen that takes the mouse is open over the running level (the
//! inventory screen, docs/behavior/inventory-screen.md section 1.3, or the
//! PDA window, docs/behavior/pda-window.md section 6), only
//! the keyboard's keys reach the character, but those the screen uses
//! ([`Controls::keys_only`]).
//!
//! Sticks give the spec's digital directions: eight sectors of 45°
//! ([`stick_directions`]). Crouch is a toggle on the gamepad and on touch,
//! since holding it while moving and looking would need a third thumb. The
//! walk or run choice is latched (our design), so the low crouch (a crouch
//! while walking, 4.2) can be entered and held without a thumb on
//! anything: a stick's by a push whose peak deflection stays below
//! [`RUN_DEFLECTION`] (walking) or reaches it (running), kept once
//! released; tap-to-move's by Crouch swiped down (walking) and a trip begun
//! standing (running). Nothing latches until set, and any keyboard movement
//! key clears both latches, so the keyboard's `accel` works as bound.
//!
//! The standard touch controls sprint as mobile shooters do (our design):
//! the stick pushed forward past its rim requests the sprint, and let go on
//! the lock target above it keeps running forward and sprinting hands-free
//! ([`SprintLock`]). The movement layer's rules still decide: whenever they
//! cancel the request (4.3: stopping, crouching, walking, firing), the lock
//! ends. Unlike the keys and the gamepad, whose `sprint_toggle` works in any
//! direction (the spec then sprints only forward), the touch stick requests
//! the sprint only when pushed within 45° of forward ([`SPRINT_CONE`]), or
//! of its lock target where a thumb landing high moved the target aside.

use std::collections::HashMap;

use crate::core::collision::CollisionWorld;
use crate::core::collision::geometry::{
    Vec3, add, cross, dot, length, normalize, ray_triangle_distance, scale, sub,
};
use crate::core::input::{
    Action, Control, GamepadAxis, GamepadButton, Input, Key, LookPhase, MouseButton, TouchButton,
    TouchLook,
};
use crate::core::movement::{PlayerState, Request, Settings, View};
use volya_engine_formats::cp1251;

/// The actions of section 2.2 this engine acts on, the inventory screen's
/// (`inventory`, `drop`; docs/behavior/inventory-screen.md 6.1) and the
/// torch's (`torch`, `night_vision`; docs/behavior/torch.md 8, 15.3) and
/// the PDA window's (`active_jobs`, `map`, `contacts`;
/// docs/behavior/pda-window.md 4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MoveAction {
    Forward,
    Back,
    LStrafe,
    RStrafe,
    Accel,
    Crouch,
    CrouchToggle,
    SprintToggle,
    Jump,
    Left,
    Right,
    Up,
    Down,
    WpnFire,
    Inventory,
    Drop,
    Torch,
    NightVision,
    ActiveJobs,
    Map,
    Contacts,
}

impl MoveAction {
    pub const ALL: [MoveAction; 21] = [
        MoveAction::Forward,
        MoveAction::Back,
        MoveAction::LStrafe,
        MoveAction::RStrafe,
        MoveAction::Accel,
        MoveAction::Crouch,
        MoveAction::CrouchToggle,
        MoveAction::SprintToggle,
        MoveAction::Jump,
        MoveAction::Left,
        MoveAction::Right,
        MoveAction::Up,
        MoveAction::Down,
        MoveAction::WpnFire,
        MoveAction::Inventory,
        MoveAction::Drop,
        MoveAction::Torch,
        MoveAction::NightVision,
        MoveAction::ActiveJobs,
        MoveAction::Map,
        MoveAction::Contacts,
    ];

    /// The action's name in `bind` lines.
    pub fn name(self) -> &'static str {
        match self {
            MoveAction::Forward => "forward",
            MoveAction::Back => "back",
            MoveAction::LStrafe => "lstrafe",
            MoveAction::RStrafe => "rstrafe",
            MoveAction::Accel => "accel",
            MoveAction::Crouch => "crouch",
            MoveAction::CrouchToggle => "crouch_toggle",
            MoveAction::SprintToggle => "sprint_toggle",
            MoveAction::Jump => "jump",
            MoveAction::Left => "left",
            MoveAction::Right => "right",
            MoveAction::Up => "up",
            MoveAction::Down => "down",
            MoveAction::WpnFire => "wpn_fire",
            MoveAction::Inventory => "inventory",
            MoveAction::Drop => "drop",
            MoveAction::Torch => "torch",
            MoveAction::NightVision => "night_vision",
            MoveAction::ActiveJobs => "active_jobs",
            MoveAction::Map => "map",
            MoveAction::Contacts => "contacts",
        }
    }
}

/// The control a key name of `bind` lines names (options-screens.md
/// section 11.2, case-insensitive), `None` for keys this engine does not
/// receive.
pub fn control_named(name: &str) -> Option<Control> {
    MouseButton::from_key_name(name)
        .map(Control::Mouse)
        .or_else(|| Key::from_dik_name(name).map(Control::Key))
}

/// The effective bindings: each action's primary (`bind`) and secondary
/// (`bind_sec`) control.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bindings {
    bound: HashMap<MoveAction, [Option<Control>; 2]>,
}

impl Bindings {
    /// The bindings of the console lines of `files` in order (section
    /// 2.1): `unbindall` removes every binding made so far, `bind` and
    /// `bind_sec` set an action's primary and secondary key. A key this
    /// engine does not receive leaves the slot empty. Binding a key to one
    /// action does not unbind it from another here: the files read are the
    /// console's saved bindings ([`crate::core::console::Console::save`]),
    /// which applied that rule already.
    pub fn read(files: &[&[u8]]) -> Self {
        let mut b = Self::default();
        for file in files {
            for line in cp1251::decode(file).lines() {
                let words: Vec<&str> = line.split_whitespace().collect();
                let slot = match words.first() {
                    Some(&"unbindall") => {
                        b.bound.clear();
                        continue;
                    }
                    Some(&"bind") => 0,
                    Some(&"bind_sec") => 1,
                    _ => continue,
                };
                let (Some(name), Some(key)) = (words.get(1), words.get(2)) else {
                    continue;
                };
                let Some(action) = MoveAction::ALL.into_iter().find(|a| a.name() == *name) else {
                    continue;
                };
                let control = control_named(key);
                b.bound.entry(action).or_default()[slot] = control;
            }
        }
        b
    }

    /// The controls bound to `action`.
    pub fn controls(&self, action: MoveAction) -> impl Iterator<Item = Control> + '_ {
        self.bound
            .get(&action)
            .into_iter()
            .flat_map(|slots| slots.iter().flatten().copied())
    }
}

/// Stick deflection ignored around the centre (our choice).
pub const DEAD_ZONE: f32 = 0.2;
/// Below this deflection a stick walks, beyond it runs (our choice).
pub const RUN_DEFLECTION: f32 = 0.6;
/// A stick direction counts once its component is this share of the
/// deflection: sin 22.5°, so the eight directions share the circle.
pub const SECTOR: f32 = 0.382_683_43;

/// Forward, back, left, right and whether to run, from a stick at `[x, y]`
/// (+x right, +y down); `None` inside the dead zone.
pub fn stick_directions([x, y]: [f32; 2]) -> Option<([bool; 4], bool)> {
    let len = x.hypot(y);
    if len < DEAD_ZONE {
        return None;
    }
    let t = SECTOR * len;
    Some(([-y > t, y > t, -x > t, x > t], len >= RUN_DEFLECTION))
}

/// The touch stick is past its rim at this reach (stick radii), pushed
/// toward the lock target (our choice: clearly beyond the full deflection,
/// so a full push alone runs).
pub const SPRINT_RIM: f32 = 1.3;
/// Back below this reach, it is within the rim again (our choice: a little
/// hysteresis, so a thumb resting on the rim doesn't flicker).
pub const SPRINT_RIM_EXIT: f32 = 1.15;
/// Past the rim counts within 45° of the lock target's direction (our
/// choice): the cosine of that angle.
pub const SPRINT_CONE: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// How far straight ahead of the stick's centre the sprint lock target is
/// centred, in stick radii (our choice: above the rim, within a thumb's
/// reach).
pub const LOCK_REACH: f32 = 2.1;
/// The lock target's radius, in stick radii (our choice: 31 CSS pixels at
/// the page's 56-pixel stick radius, a 62-pixel target).
pub const LOCK_RADIUS: f32 = 0.55;
/// The closest the lock target comes: all of it past the rim.
pub const LOCK_MIN_REACH: f32 = SPRINT_RIM + LOCK_RADIUS;

/// The lock target's centre relative to the stick's (stick radii, +y
/// down) for the page's `centre` ([`Input::touch_lock_target`]): straight
/// ahead at [`LOCK_REACH`] by default. Where that wouldn't fit on the
/// screen (a thumb landing high), the page draws it closer, down to
/// [`LOCK_MIN_REACH`], or turned toward the side (our choice); its
/// distance is kept within those, and nonsense gives the default.
pub fn lock_target(centre: Option<[f32; 2]>) -> [f32; 2] {
    let default = [0.0, -LOCK_REACH];
    let Some([x, y]) = centre else {
        return default;
    };
    let len = x.hypot(y);
    if !len.is_finite() || len < 1e-3 {
        return default;
    }
    let scale = len.clamp(LOCK_MIN_REACH, LOCK_REACH) / len;
    [x * scale, y * scale]
}

/// The sprint lock of the touch stick (our design, after mobile shooters):
///
/// - pushed past the rim ([`SPRINT_RIM`]) toward the lock target (within
///   [`SPRINT_CONE`] of its direction: straight ahead unless the target
///   had to move aside), the stick requests the sprint;
/// - back within the rim ([`SPRINT_RIM_EXIT`]) or turned aside while held,
///   it ends the sprint it requested;
/// - let go on the lock target ([`lock_target`], [`LOCK_RADIUS`]) while
///   sprinting, it locks: the stick reads full forward with nobody on it;
/// - let go elsewhere, or its touch cancelled, it ends the sprint it
///   requested;
/// - a touch on the stick, or everything released (focus lost), ends the
///   lock and its sprint;
/// - whenever the movement layer cancels the sprint request, the lock ends
///   and a held push past the rim requests nothing more until it returns
///   within the rim (the spec asks for a fresh request, 4.3).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SprintLock {
    phase: LockPhase,
    /// The stick's latest value, across frames.
    last: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LockPhase {
    #[default]
    Free,
    /// Held past the rim; `sprinting` until the movement layer cancels the
    /// request this push made.
    Past {
        sprinting: bool,
    },
    Locked,
}

impl SprintLock {
    /// Whether the lock holds the stick forward.
    pub fn locked(&self) -> bool {
        self.phase == LockPhase::Locked
    }

    /// Whether the stick is held past the rim.
    pub fn past(&self) -> bool {
        matches!(self.phase, LockPhase::Past { .. })
    }

    /// Lets the stick go from the lock at once, leaving the sprint request
    /// to the movement layer (a crouch about to cancel it).
    pub fn unlock(&mut self) {
        if self.locked() {
            self.phase = LockPhase::Free;
        }
    }

    /// One frame: the stick's `values` this frame in order
    /// ([`Input::touch_stick_values`]; `None` a cancelled touch), whether
    /// everything was `released` since ([`Input::sticks_released`]),
    /// whether the sprint is `requested` after the previous frame, and the
    /// lock target's `centre` ([`lock_target`]). Returns whether to press
    /// `sprint_toggle`.
    pub fn frame(
        &mut self,
        values: &[Option<[f32; 2]>],
        released: bool,
        requested: bool,
        centre: [f32; 2],
    ) -> bool {
        // The movement layer cancelled the request (made by the previous
        // frame at the latest).
        if !requested {
            match self.phase {
                LockPhase::Past { sprinting: true } => {
                    self.phase = LockPhase::Past { sprinting: false }
                }
                LockPhase::Locked => self.phase = LockPhase::Free,
                _ => {}
            }
        }
        let reach = centre[0].hypot(centre[1]);
        let toward = [centre[0] / reach, centre[1] / reach];
        let mut ended = false;
        let mut end = |phase: &mut LockPhase| {
            if matches!(
                *phase,
                LockPhase::Past { sprinting: true } | LockPhase::Locked
            ) {
                ended = true;
            }
            *phase = LockPhase::Free;
        };
        for &value in values {
            let Some(v) = value else {
                // Cancelled: no lock, whatever the last value was.
                self.last = [0.0; 2];
                end(&mut self.phase);
                continue;
            };
            let len = v[0].hypot(v[1]);
            let ahead = v[0] * toward[0] + v[1] * toward[1] >= SPRINT_CONE * len;
            let let_go = v == [0.0; 2] && self.last != [0.0; 2];
            match self.phase {
                // Touched again.
                LockPhase::Locked => end(&mut self.phase),
                LockPhase::Past { sprinting } if let_go => {
                    let d = [self.last[0] - centre[0], self.last[1] - centre[1]];
                    if sprinting && d[0].hypot(d[1]) <= LOCK_RADIUS {
                        self.phase = LockPhase::Locked;
                    } else {
                        end(&mut self.phase);
                    }
                }
                LockPhase::Past { .. } if len < SPRINT_RIM_EXIT || !ahead => end(&mut self.phase),
                _ => {}
            }
            if self.phase == LockPhase::Free && len >= SPRINT_RIM && ahead {
                self.phase = LockPhase::Past { sprinting: true };
            }
            self.last = v;
        }
        if released {
            self.last = [0.0; 2];
            end(&mut self.phase);
        }
        let want = match self.phase {
            LockPhase::Past { sprinting: true } | LockPhase::Locked => Some(true),
            _ if ended => Some(false),
            _ => None,
        };
        want.is_some_and(|w| w != requested)
    }
}

/// The mouse counts a touch look drag turns per CSS pixel up to
/// [`LOOK_PRECISE_SPEED`] (the user's choice, #747: a slow drag across the
/// look half, about 400 px, turns about 180° at the default `mouse_sens`).
pub const LOOK_BASE_GAIN: f32 = 3.25;
/// Up to this speed of a touch look drag (CSS pixels per second) it turns
/// [`LOOK_BASE_GAIN`] counts a pixel (our choice: slow aiming).
pub const LOOK_PRECISE_SPEED: f32 = 300.0;
/// From this speed on a drag turns [`LOOK_MAX_GAIN`] counts a pixel (our
/// choice: a quick flick across the look half, about 2,500 px/s, passes it).
pub const LOOK_FAST_SPEED: f32 = 2000.0;
/// The counts a fast drag turns per CSS pixel (the user's choice, #747:
/// the original's 1,000-DPI mouse at the default `mouse_sens`, about
/// 137.5° per inch of thumb travel at about 153 CSS px an inch).
pub const LOOK_MAX_GAIN: f32 = 6.5;

/// The touch look's acceleration (our design, as mobile shooters do): a
/// drag at `speed` CSS pixels per second gives this many mouse counts per
/// pixel: [`LOOK_BASE_GAIN`] up to [`LOOK_PRECISE_SPEED`], rising linearly to
/// [`LOOK_MAX_GAIN`] at [`LOOK_FAST_SPEED`], level beyond. The counts then
/// turn by `mouse_sens` and `mouse_invert` as any mouse's do (8.1). The
/// speed is the page's, smoothed over its drag's own event times
/// ([`Input::touch_look_moves`]), so how the events fall into frames
/// doesn't change the gain.
pub fn look_gain(speed: f32) -> f32 {
    let t = (speed - LOOK_PRECISE_SPEED) / (LOOK_FAST_SPEED - LOOK_PRECISE_SPEED);
    if t.is_nan() {
        return LOOK_BASE_GAIN;
    }
    LOOK_BASE_GAIN + t.clamp(0.0, 1.0) * (LOOK_MAX_GAIN - LOOK_BASE_GAIN)
}

/// The touch look's momentum keeps this much of its speed every
/// millisecond: UIScrollView's normal deceleration rate (believed: 0.998
/// is read from UIKit at run time; Apple documents no value).
pub const LOOK_DECELERATION: f64 = 0.998;
/// A lift's velocity is fitted to the finger's moves of its last this
/// many milliseconds (Chromium's and Android's velocity trackers' horizon).
pub const LOOK_VELOCITY_WINDOW_MS: f64 = 100.0;
/// A pause this long between moves starts the fit afresh (Chromium's and
/// Android's trackers: the pointer is taken to have stopped).
pub const LOOK_STOPPED_MOVE_MS: f64 = 40.0;
/// A lift this long after the last move flings nothing (Chromium's
/// tracker: the pointer stopped before it lifted).
pub const LOOK_STOPPED_UP_MS: f64 = 80.0;
/// The fastest a flick turns as it is let go, radians per millisecond
/// (our choice, as Android caps a fling: 720° a second, which then turns
/// about 360° more).
pub const LOOK_MAX_SPEED: f32 = 4.0 * std::f32::consts::PI / 1000.0;
/// Momentum ends once all it would still turn is below this, radians
/// (our choice: 0.05°).
pub const LOOK_REST: f32 = 0.000_872_664_6;
/// A frame further than this (milliseconds) from the previous one the
/// controls saw ends the momentum: the level's controls weren't running
/// (our choice).
pub const LOOK_GAP_MS: u64 = 250;

/// The turn `[yaw, pitch]` (radians, right and down) that keeps the
/// scene's direction under a finger at `from` under it at `to`, at pitch
/// `pitch` limited to `limits`, for a view whose half vertical field of
/// view has the tangent `tan_half` (positions as [`TouchLook::at`]). Pitch
/// is solved first, from the direction's elevation, then yaw from its
/// azimuth; where the limits stop the pitch, the yaw still follows.
pub fn grab_turn(
    from: [f32; 2],
    to: [f32; 2],
    pitch: f32,
    limits: [f32; 2],
    tan_half: f32,
) -> [f32; 2] {
    let camera = |a: [f32; 2]| {
        let v = [a[0] * tan_half, -a[1] * tan_half, 1.0];
        let l = (v[0] * v[0] + v[1] * v[1] + 1.0).sqrt();
        v.map(|c| c / l)
    };
    // From the camera at `p` (positive looks down) to a yaw-0 world.
    let world = |c: [f32; 3], p: f32| {
        let (s, k) = p.sin_cos();
        [c[0], c[1] * k - c[2] * s, c[1] * s + c[2] * k]
    };
    let d = world(camera(from), pitch);
    let c = camera(to);
    let rho = c[1].hypot(c[2]);
    let alpha = c[1].atan2(c[2]);
    let p = (alpha - (d[1] / rho).clamp(-1.0, 1.0).asin()).clamp(limits[0], limits[1]);
    let w = world(c, p);
    let mut yaw = d[0].atan2(d[2]) - w[0].atan2(w[2]);
    if yaw > std::f32::consts::PI {
        yaw -= std::f32::consts::TAU;
    } else if yaw < -std::f32::consts::PI {
        yaw += std::f32::consts::TAU;
    }
    [yaw, p - pitch]
}

/// One look finger: where it is, the turn it made, and its recent moves'
/// times and turns for its velocity; where and when it went down, and
/// whether it has left the tap's slop (looking).
#[derive(Clone, Debug)]
struct LookFinger {
    id: u32,
    at: [f32; 2],
    turned: [f32; 2],
    samples: Vec<(f64, [f32; 2])>,
    down: f64,
    dragging: bool,
}

impl LookFinger {
    /// Its velocity at a lift at `time` (radians a millisecond): the
    /// least-squares slope of its turn over the window, none if it had
    /// stopped.
    fn velocity(&self, time: f64) -> [f32; 2] {
        let Some(&(last, _)) = self.samples.last() else {
            return [0.0; 2];
        };
        if time - last > LOOK_STOPPED_UP_MS {
            return [0.0; 2];
        }
        let recent: Vec<_> = self
            .samples
            .iter()
            .filter(|(t, _)| last - t <= LOOK_VELOCITY_WINDOW_MS)
            .collect();
        let n = recent.len() as f64;
        let mean_t = recent.iter().map(|(t, _)| t).sum::<f64>() / n;
        let var: f64 = recent.iter().map(|(t, _)| (t - mean_t).powi(2)).sum();
        if recent.len() < 2 || var <= 0.0 {
            return [0.0; 2];
        }
        let v = [0, 1].map(|i| {
            let mean = recent.iter().map(|(_, a)| f64::from(a[i])).sum::<f64>() / n;
            let cov: f64 = recent
                .iter()
                .map(|(t, a)| (t - mean_t) * (f64::from(a[i]) - mean))
                .sum();
            (cov / var) as f32
        });
        let speed = v[0].hypot(v[1]);
        if speed > LOOK_MAX_SPEED {
            v.map(|c| c * LOOK_MAX_SPEED / speed)
        } else {
            v
        }
    }
}

/// Touch look as direct manipulation (our design, after iOS's scroll
/// views and Maps): once a finger has left [`TAP_SLOP_PX`] around where
/// it went down, the scene's direction under it there stays under it
/// ([`grab_turn`]); let go moving, the view keeps turning at the finger's
/// velocity, slowing by [`LOOK_DECELERATION`] a millisecond over the
/// frames' real time, until a finger looks again. A finger lifted within
/// the slop inside [`TAP_MS`] is a tap ([`TouchLookDrag::taps`]).
#[derive(Clone, Debug, Default)]
pub struct TouchLookDrag {
    fingers: Vec<LookFinger>,
    /// The momentum's turn, radians a millisecond.
    momentum: [f32; 2],
    last: Option<u64>,
    taps: Vec<Tap>,
}

impl TouchLookDrag {
    /// The momentum's turn now, radians a millisecond.
    pub fn momentum(&self) -> [f32; 2] {
        self.momentum
    }

    /// The latest frame's taps, in order.
    pub fn taps(&self) -> &[Tap] {
        &self.taps
    }

    /// One frame at `now` (milliseconds) with this frame's look events at
    /// the camera's `pitch` within `limits`: the turn `[yaw, pitch]` to
    /// make. `released` ends every finger and the momentum.
    pub fn frame(
        &mut self,
        looks: &[TouchLook],
        released: bool,
        mut pitch: f32,
        limits: [f32; 2],
        now: u64,
    ) -> [f32; 2] {
        let dt = self.last.map_or(0, |l| now.saturating_sub(l));
        self.last = Some(now);
        self.taps.clear();
        if dt > LOOK_GAP_MS || released {
            self.momentum = [0.0; 2];
        }
        let tan_half = (crate::core::camera::FOV_Y_DEGREES.to_radians() / 2.0).tan();
        let mut turn = [0.0f32; 2];
        if self.fingers.is_empty() && self.momentum != [0.0; 2] {
            // The frame's share of v·r^t: v (1 − r^dt) / −ln r.
            let r = LOOK_DECELERATION;
            let keep = r.powf(dt as f64);
            let travel = ((1.0 - keep) / -r.ln()) as f32;
            let mut d = self.momentum.map(|v| v * travel);
            self.momentum = self.momentum.map(|v| v * keep as f32);
            let to = (pitch + d[1]).clamp(limits[0], limits[1]);
            if to != pitch + d[1] {
                d[1] = to - pitch;
                self.momentum[1] = 0.0;
            }
            turn = d;
            pitch = to;
            let rest = self.momentum[0].hypot(self.momentum[1]) / -(r.ln() as f32);
            if rest < LOOK_REST {
                self.momentum = [0.0; 2];
            }
        }
        for look in looks {
            let found = self.fingers.iter().position(|f| f.id == look.finger);
            match look.phase {
                LookPhase::Down | LookPhase::Drag => {
                    self.momentum = [0.0; 2];
                    if let Some(i) = found {
                        self.fingers.remove(i);
                    }
                    self.fingers.push(LookFinger {
                        id: look.finger,
                        at: look.at,
                        turned: [0.0; 2],
                        samples: vec![(look.time, [0.0; 2])],
                        down: look.time,
                        dragging: look.phase == LookPhase::Drag,
                    });
                }
                LookPhase::Move | LookPhase::Up => {
                    let Some(i) = found else { continue };
                    let f = &mut self.fingers[i];
                    // Within the slop the finger stays where it went
                    // down; past it, it grabs the scene from there.
                    let moved = (look.at[0] - f.at[0]).hypot(look.at[1] - f.at[1]);
                    f.dragging |= moved * look.half_height > TAP_SLOP_PX;
                    if !f.dragging {
                        if look.phase == LookPhase::Up {
                            if look.time - f.down <= TAP_MS {
                                self.taps.push(Tap {
                                    at: f.at,
                                    down: f.down,
                                    up: look.time,
                                    half_height: look.half_height,
                                });
                            }
                            self.fingers.remove(i);
                        }
                        continue;
                    }
                    if look.at != f.at {
                        let d = grab_turn(f.at, look.at, pitch, limits, tan_half);
                        f.at = look.at;
                        pitch += d[1];
                        for k in 0..2 {
                            turn[k] += d[k];
                            f.turned[k] += d[k];
                        }
                        if f.samples
                            .last()
                            .is_some_and(|&(t, _)| look.time - t > LOOK_STOPPED_MOVE_MS)
                        {
                            f.samples.clear();
                        }
                        f.samples.push((look.time, f.turned));
                        let window = look.time - LOOK_VELOCITY_WINDOW_MS;
                        f.samples.retain(|&(t, _)| t >= window);
                    }
                    if look.phase == LookPhase::Up {
                        let v = f.velocity(look.time);
                        self.fingers.remove(i);
                        self.momentum = if self.fingers.is_empty() { v } else { [0.0; 2] };
                    }
                }
                LookPhase::Cancel => {
                    if let Some(i) = found {
                        self.fingers.remove(i);
                    }
                    self.momentum = [0.0; 2];
                }
            }
        }
        if released {
            self.fingers.clear();
            self.momentum = [0.0; 2];
        }
        turn
    }
}

/// A touch that stays within this many CSS pixels of where it went down
/// is no drag (our value, the touchpad's tap slop): it doesn't look.
pub const TAP_SLOP_PX: f32 = 10.0;
/// A touch within the slop lifted within this many milliseconds of going
/// down is a tap (our value, the touchpad's).
pub const TAP_MS: f64 = 500.0;
/// A tap going down within this many milliseconds of the previous tap's
/// lift (the engine's double-click time) and within [`DOUBLE_TAP_PX`] of
/// it is a double tap: that trip sprints.
pub const DOUBLE_TAP_MS: f64 = crate::core::input::DOUBLE_CLICK_MS;
/// How far apart, in CSS pixels, a double tap's taps may be (our choice:
/// about a fingertip's width).
pub const DOUBLE_TAP_PX: f32 = 40.0;
/// How far a tap's ray reaches into the level, metres (our choice).
pub const TAP_REACH: f32 = 100.0;
/// A trip ends once the feet are within this many metres of its
/// destination (our choice).
pub const ARRIVAL: f32 = 0.3;
/// A trip ends when its distance hasn't shrunk by [`PROGRESS`] metres for
/// this many milliseconds (our choice): stuck against a wall, at a ledge
/// it can't climb, short of a destination on a wall.
pub const STUCK_MS: u64 = 500;
pub const PROGRESS: f32 = 0.05;
/// A tap within this many CSS pixels of the trip's destination marker
/// cancels the trip (our choice: the page's 26-pixel ring's radius plus
/// [`TAP_SLOP_PX`]).
pub const MARKER_HIT_PX: f32 = 13.0 + TAP_SLOP_PX;

/// What a tap meets, from the view it saw: where its ray meets the level
/// ([`tap_destination`]), and where a point shows in the picture
/// ([`picture_point`]: the trip's marker).
pub trait TapView {
    fn destination(&self, at: [f32; 2]) -> Option<Vec3>;
    fn picture_point(&self, point: Vec3) -> Option<[f32; 2]>;
}

/// No level: taps meet nothing.
impl TapView for () {
    fn destination(&self, _: [f32; 2]) -> Option<Vec3> {
        None
    }
    fn picture_point(&self, _: Vec3) -> Option<[f32; 2]> {
        None
    }
}

/// A level's collision geometry seen from a view.
pub struct LevelView<'a> {
    pub world: &'a CollisionWorld,
    pub view: View,
}

impl TapView for LevelView<'_> {
    fn destination(&self, at: [f32; 2]) -> Option<Vec3> {
        tap_destination(self.world, &self.view, at)
    }
    fn picture_point(&self, point: Vec3) -> Option<[f32; 2]> {
        picture_point(&self.view, point)
    }
}

/// A tap on the picture: where (as [`TouchLook::at`]), when it went down
/// and was lifted (the page's milliseconds), and the picture's half height
/// in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tap {
    pub at: [f32; 2],
    pub down: f64,
    pub up: f64,
    pub half_height: f32,
}

/// The view's right and up vectors for its `direction` and `up`, as the
/// view matrix makes them ([`crate::core::camera::view_matrix`]).
fn view_axes(view: &View) -> (Vec3, Vec3) {
    let d = view.direction;
    let ud = dot(view.up, d);
    let u = normalize([
        view.up[0] - d[0] * ud,
        view.up[1] - d[1] * ud,
        view.up[2] - d[2] * ud,
    ])
    .unwrap_or([0.0, 1.0, 0.0]);
    (cross(u, d), u)
}

/// The tangent of half the game's vertical field of view.
fn tan_half_fov() -> f32 {
    (crate::core::camera::FOV_Y_DEGREES.to_radians() / 2.0).tan()
}

/// Where in the picture (as [`TouchLook::at`]) `point` shows from `view`,
/// `None` nearer than the near plane or behind.
pub fn picture_point(view: &View, point: Vec3) -> Option<[f32; 2]> {
    let (r, u) = view_axes(view);
    let rel = sub(point, view.eye);
    let z = dot(rel, view.direction);
    if z < crate::core::camera::NEAR {
        return None;
    }
    let t = tan_half_fov();
    Some([dot(rel, r) / z / t, -dot(rel, u) / z / t])
}

/// A tap's destination: where the ray from `view`'s eye through the
/// picture's point `at` (as [`TouchLook::at`]) first meets a solid
/// triangle of `world`, from either side, within [`TAP_REACH`]
/// (pass-through ones, invisible walls among them, let it through).
pub fn tap_destination(world: &CollisionWorld, view: &View, at: [f32; 2]) -> Option<Vec3> {
    let (r, u) = view_axes(view);
    let t = tan_half_fov();
    let dir = normalize(add(
        add(scale(r, at[0] * t), scale(u, -at[1] * t)),
        view.direction,
    ))
    .unwrap_or(view.direction);
    let eye = view.eye;
    world
        .ray_nearest(eye, dir, TAP_REACH, |i| {
            if !world.is_solid(i) {
                return None;
            }
            ray_triangle_distance(eye, dir, &world.corners(i))
                .filter(|d| (0.0..=TAP_REACH).contains(d))
        })
        .map(|(d, _)| add(eye, scale(dir, d)))
}

/// A trip under way to a tapped destination.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trip {
    pub destination: Vec3,
    /// A double tap's: sprinting, until the movement layer cancels it.
    pub sprint: bool,
    /// The closest the feet have come, and when they last came
    /// [`PROGRESS`] closer.
    closest: f32,
    progress_at: u64,
}

/// Tap-to-move (our design, an experiment in place of a move stick): a
/// tap in the level sets a destination where its ray meets the level
/// ([`tap_destination`]), and the character runs there in a straight line
/// as if forward were held toward it, through the movement layer's own
/// physics, collisions and footsteps ([`Request::toward`]):
///
/// - it ends on arrival within [`ARRIVAL`], when stuck (no [`PROGRESS`]
///   for [`STUCK_MS`]), on a tap that meets nothing, when a key or the
///   gamepad moves, when a screen covers the level, on a blur and on a
///   frame over [`LOOK_GAP_MS`] from the previous one;
/// - a new tap sets a new destination; a double tap's trip sprints, the
///   movement layer's rules still deciding (4.3: a crouch or a walk
///   cancels it, and a cancelled sprint isn't asked for again that trip);
/// - a tap on the destination's marker (within [`MARKER_HIT_PX`]) cancels
///   the trip, unless it is a double tap's second tap (within
///   [`DOUBLE_TAP_MS`] of the previous tap): that sprints;
/// - looking doesn't end it: the trip steers by the world, not the view.
#[derive(Clone, Debug, Default)]
pub struct TapMove {
    trip: Option<Trip>,
    /// How the latest trip that arrived, got stuck or was cancelled on its
    /// marker ended, and its distance from its destination then.
    ended: Option<(TripEnd, f32)>,
    last_tap: Option<Tap>,
    /// The sprint request is this trip's (pressed by it, not yet ended).
    owns_sprint: bool,
    last: Option<u64>,
}

/// How a trip ended on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TripEnd {
    Arrived,
    Stuck,
    /// A tap on its marker.
    Cancelled,
}

impl TapMove {
    /// The trip under way.
    pub fn trip(&self) -> Option<&Trip> {
        self.trip.as_ref()
    }

    /// How the latest trip to end on its own or on its marker ended
    /// (arrived, stuck, cancelled), and its feet's distance from its
    /// destination then.
    pub fn ended(&self) -> Option<(TripEnd, f32)> {
        self.ended
    }

    /// One frame at `now`: this frame's taps with their destinations,
    /// where the trip's destination shows in the picture the taps saw
    /// (`marker`), whether something else `ended` the trip, and the player
    /// after the previous frame. Returns the bearing to run along (forward
    /// held, [`Request::toward`]) and whether to press `sprint_toggle`.
    pub fn frame(
        &mut self,
        taps: &[(Tap, Option<Vec3>)],
        mut marker: Option<[f32; 2]>,
        ended: bool,
        player: &PlayerState,
        now: u64,
    ) -> (Option<f32>, bool) {
        let gap = self
            .last
            .is_some_and(|l| now.saturating_sub(l) > LOOK_GAP_MS);
        self.last = Some(now);
        if ended || gap {
            self.trip = None;
        }
        // The movement layer cancelled the sprint the trip asked for.
        if self.owns_sprint && !player.sprint_requested {
            self.owns_sprint = false;
            if let Some(t) = &mut self.trip {
                t.sprint = false;
            }
        }
        for &(tap, destination) in taps {
            let double = self.last_tap.is_some_and(|l| {
                let px = (tap.at[0] - l.at[0]).hypot(tap.at[1] - l.at[1]) * tap.half_height;
                tap.down - l.up < DOUBLE_TAP_MS && px <= DOUBLE_TAP_PX
            });
            // A tap on the marker, past the double tap's time: the trip
            // ends where it is.
            let late = self
                .last_tap
                .is_none_or(|l| tap.down - l.up >= DOUBLE_TAP_MS);
            let on_marker = marker.is_some_and(|m| {
                (tap.at[0] - m[0]).hypot(tap.at[1] - m[1]) * tap.half_height <= MARKER_HIT_PX
            });
            if let Some(t) = self.trip
                && !double
                && late
                && on_marker
            {
                self.ended = Some((
                    TripEnd::Cancelled,
                    length(sub(t.destination, player.position)),
                ));
                self.trip = None;
                self.last_tap = None;
                marker = None;
                continue;
            }
            // A third tap starts afresh.
            self.last_tap = (!double).then_some(tap);
            marker = None;
            self.trip = destination.map(|destination| Trip {
                destination,
                sprint: double,
                closest: f32::INFINITY,
                progress_at: now,
            });
        }
        let mut toward = None;
        if let Some(t) = &mut self.trip {
            let off = sub(t.destination, player.position);
            let distance = length(off);
            let stuck = now.saturating_sub(t.progress_at) >= STUCK_MS;
            if distance < t.closest - PROGRESS {
                t.closest = distance;
                t.progress_at = now;
            }
            if distance <= ARRIVAL || stuck && t.progress_at != now {
                let how = if distance <= ARRIVAL {
                    TripEnd::Arrived
                } else {
                    TripEnd::Stuck
                };
                self.ended = Some((how, distance));
                self.trip = None;
            } else {
                // Right above or below: on along the view.
                let bearing = if off[0].hypot(off[2]) < 1e-3 {
                    player.yaw
                } else {
                    off[0].atan2(off[2])
                };
                // Within half a turn of the yaw, as the heading is.
                let mut turn = bearing - player.yaw;
                turn -= std::f32::consts::TAU * (turn / std::f32::consts::TAU).round();
                toward = Some(player.yaw + turn);
            }
        }
        let want = self.trip.is_some_and(|t| t.sprint);
        let press = if want && !self.owns_sprint && !player.sprint_requested {
            self.owns_sprint = true;
            true
        } else if !want && self.owns_sprint {
            self.owns_sprint = false;
            player.sprint_requested
        } else {
            false
        };
        (toward, press)
    }
}

/// The player's controls.
#[derive(Clone, Debug)]
pub struct Controls {
    pub bindings: Bindings,
    pub settings: Settings,
    /// The latched walk (`true`) or run choice of the gamepad's left stick
    /// and of touch (the touch stick; tap-to-move's trips, the low
    /// crouch), `None` until set (see the module docs).
    pub walk: [Option<bool>; 2],
    /// Each stick's peak deflection in the current push, 0 when released.
    peak: [f32; 2],
    pub sprint_lock: SprintLock,
    pub look: TouchLookDrag,
    pub tap_move: TapMove,
    /// While a screen over the level takes the mouse (the inventory
    /// screen): only keyboard keys reach the character, not these (the keys
    /// the screen uses); mouse, wheel, gamepad and touch do not
    /// (inventory-screen.md 1.3; the gamepad and touch part is our design,
    /// since the screen takes them).
    pub keys_only: Option<Vec<Control>>,
}

impl Controls {
    pub fn new(bindings: Bindings, settings: Settings) -> Self {
        Self {
            bindings,
            settings,
            walk: [None; 2],
            peak: [0.0; 2],
            sprint_lock: SprintLock::default(),
            look: TouchLookDrag::default(),
            tap_move: TapMove::default(),
            keys_only: None,
        }
    }

    /// The keys bound to `action`.
    pub fn keys(&self, action: MoveAction) -> Vec<Key> {
        self.bindings
            .controls(action)
            .filter_map(|c| match c {
                Control::Key(k) => Some(k),
                _ => None,
            })
            .collect()
    }

    /// Whether the player asked for the inventory screen this frame: a
    /// press of a keyboard key bound to `inventory` (inventory-screen.md
    /// 1.1), the touch Inventory button or a gamepad's north button (our
    /// design).
    pub fn inventory_pressed(&self, input: &Input) -> bool {
        let key = input.device_changes().iter().any(|&(c, pressed)| {
            pressed
                && matches!(c, Control::Key(_))
                && self
                    .bindings
                    .controls(MoveAction::Inventory)
                    .any(|b| b == c)
        });
        let touch = input.actions().iter().any(|a| {
            matches!(
                a,
                Action::Press {
                    control: Control::Touch(TouchButton::Inventory),
                    ..
                }
            )
        });
        key || touch || input.pad_changes().contains(&(GamepadButton::North, true))
    }

    /// The PDA tab asked for this frame, with no window open
    /// (pda-window.md 4.1): a press of a keyboard key bound to
    /// `active_jobs` (tab 0), `map` (1) or `contacts` (3), or, as
    /// `active_jobs`, the touch PDA button or a gamepad's d-pad left (our
    /// design). The first in that order when several are pressed.
    pub fn pda_pressed(&self, input: &Input) -> Option<usize> {
        let actions = [
            MoveAction::ActiveJobs,
            MoveAction::Map,
            MoveAction::Contacts,
        ];
        let key = actions
            .iter()
            .zip(crate::pda_window::ACTION_TABS)
            .find_map(|(&a, tab)| {
                input
                    .device_changes()
                    .iter()
                    .any(|&(c, pressed)| {
                        pressed
                            && matches!(c, Control::Key(_))
                            && self.bindings.controls(a).any(|b| b == c)
                    })
                    .then_some(tab)
            });
        let touch = input.actions().iter().any(|a| {
            matches!(
                a,
                Action::Press {
                    control: Control::Touch(TouchButton::Pda),
                    ..
                }
            )
        });
        let pad = input
            .pad_changes()
            .contains(&(GamepadButton::DpadLeft, true));
        key.or((touch || pad).then_some(crate::pda_window::ACTION_TABS[0]))
    }

    /// Whether the player pressed `torch` this frame (torch.md 8): a key
    /// bound to it, a gamepad's d-pad up or the touch Torch button (our
    /// design); while a screen takes the mouse, only keys it does not use.
    pub fn torch_pressed(&self, input: &Input) -> bool {
        self.command_pressed(
            input,
            MoveAction::Torch,
            GamepadButton::DpadUp,
            TouchButton::Torch,
        )
    }

    /// Whether the player pressed `night_vision` this frame (torch.md
    /// 15.3): a key bound to it, a gamepad's d-pad down or the touch Night
    /// vision button (our design).
    pub fn night_vision_pressed(&self, input: &Input) -> bool {
        self.command_pressed(
            input,
            MoveAction::NightVision,
            GamepadButton::DpadDown,
            TouchButton::NightVision,
        )
    }

    fn command_pressed(
        &self,
        input: &Input,
        action: MoveAction,
        pad: GamepadButton,
        touch: TouchButton,
    ) -> bool {
        let only = self.keys_only.as_ref();
        let key = input.device_changes().iter().any(|&(c, pressed)| {
            pressed
                && only.is_none_or(|kept| matches!(c, Control::Key(_)) && !kept.contains(&c))
                && self.bindings.controls(action).any(|b| b == c)
        });
        let touched = input.actions().iter().any(
            |a| matches!(a, Action::Press { control, .. } if *control == Control::Touch(touch)),
        );
        key || (only.is_none() && (input.pad_changes().contains(&(pad, true)) || touched))
    }
}

impl Controls {
    /// This frame's request from `input`, and whether the player asked for
    /// the menu with a gamepad's east button or the touch Menu button (the
    /// keyboard's key bound to `quit` is the app's: it opens the menu
    /// whatever has the keyboard, save-load-screens.md section 2). `player`
    /// is the player after the previous frame (its sprint request for the
    /// sprint lock and tap-to-move, its position for tap-to-move, its
    /// crouch toggle for the touch low crouch); `destination` gives a tap's destination for its place in
    /// the picture and its trip's marker there, from the view the tap saw
    /// ([`TapView`]).
    pub fn request(
        &mut self,
        input: &Input,
        player: &PlayerState,
        now: u64,
        view: &impl TapView,
    ) -> (Request, bool) {
        let only = self.keys_only.clone();
        let keys_only = only.is_some();
        let reaches = |c: Control| match &only {
            None => true,
            Some(kept) => matches!(c, Control::Key(_)) && !kept.contains(&c),
        };
        let held = |a| {
            self.bindings
                .controls(a)
                .any(|c| reaches(c) && input.device_held(c))
        };
        let changed = |a, pressed: bool| {
            input.device_changes().iter().any(|&(c, p)| {
                p == pressed && reaches(c) && self.bindings.controls(a).any(|b| b == c)
            })
        };
        let pad = |b: GamepadButton, pressed: bool| {
            !keys_only && input.pad_changes().contains(&(b, pressed))
        };
        let touch = |b: TouchButton, pressed: bool| {
            !keys_only
                && input.actions().iter().any(|a| match a {
                    Action::Press { control, .. } => pressed && *control == Control::Touch(b),
                    Action::Release { control } => !pressed && *control == Control::Touch(b),
                    Action::Cursor { .. } | Action::Wheel { .. } => false,
                })
        };
        let mut r = Request {
            forward: held(MoveAction::Forward),
            back: held(MoveAction::Back),
            left: held(MoveAction::LStrafe),
            right: held(MoveAction::RStrafe),
            accel: held(MoveAction::Accel),
            crouch: held(MoveAction::Crouch),
            crouch_released: changed(MoveAction::Crouch, false),
            crouch_toggle: changed(MoveAction::CrouchToggle, true)
                || pad(GamepadButton::West, true)
                || touch(TouchButton::Crouch, true),
            sprint_toggle: changed(MoveAction::SprintToggle, true)
                || pad(GamepadButton::LeftStick, true),
            jump_pressed: changed(MoveAction::Jump, true)
                || pad(GamepadButton::South, true)
                || touch(TouchButton::Jump, true),
            jump_released: changed(MoveAction::Jump, false)
                || pad(GamepadButton::South, false)
                || touch(TouchButton::Jump, false),
            fire: changed(MoveAction::WpnFire, true),
            mouse: if keys_only {
                [0.0; 2]
            } else {
                input.pointer_motion()
            },
            turn: [
                f32::from(u8::from(held(MoveAction::Right)))
                    - f32::from(u8::from(held(MoveAction::Left))),
                f32::from(u8::from(held(MoveAction::Down)))
                    - f32::from(u8::from(held(MoveAction::Up))),
            ],
            look: [0.0; 2],
            toward: None,
        };
        // The touch stick: its reach within the rim, or full forward while
        // the sprint lock holds it.
        // Under a screen the touch stick is gone (the page hides it): as
        // everything released, its lock ends.
        r.sprint_toggle ^= if keys_only {
            self.sprint_lock
                .frame(&[], true, player.sprint_requested, lock_target(None))
        } else {
            self.sprint_lock.frame(
                input.touch_stick_values(),
                input.sticks_released()[1],
                player.sprint_requested,
                lock_target(input.touch_lock_target()),
            )
        };
        // Crouch swiped down (our design): the low crouch, the crouch
        // toggle engaged with touch's latch walking (4.2), as a light push
        // of the touch stick let go would leave it. A lock holding the stick
        // forward lets go now, or its full push would latch running again
        // in this frame (the crouch cancels its sprint, 4.3).
        if touch(TouchButton::CrouchLow, true) {
            if !(player.crouch_toggled ^ r.crouch_toggle) {
                r.crouch_toggle ^= true;
            }
            self.sprint_lock.unlock();
            self.walk[1] = Some(true);
            self.peak[1] = 0.0;
        }
        let [x, y] = if keys_only {
            [0.0; 2]
        } else {
            input.touch_stick()
        };
        let reach = x.hypot(y).max(1.0);
        let touch_stick = if self.sprint_lock.locked() {
            [0.0, -1.0]
        } else {
            [x / reach, y / reach]
        };
        let axis = |a| if keys_only { 0.0 } else { input.axis(a) };
        let pads = [
            [axis(GamepadAxis::LeftX), axis(GamepadAxis::LeftY)],
            touch_stick,
        ];
        let keyboard = r.forward || r.back || r.left || r.right;
        if keyboard {
            // Keyboard movement: the latches give way.
            self.walk = [None; 2];
        }
        // The sticks: directions, and walking below the run deflection
        // (`accel` inverts `g_always_run`, 4.1). Released by a blur or lost
        // pointer lock, not let go (the gamepad's under a screen too), a
        // stick's latch and its push's peak stay for the value sent again.
        let released = input.sticks_released();
        let released = [keys_only || released[0], !keys_only && released[1]];
        let mut sticks_move = false;
        for (i, stick) in pads.into_iter().enumerate() {
            if released[i] {
                continue;
            }
            if let Some(([f, b, l, rt], _)) = stick_directions(stick) {
                r.forward |= f;
                r.back |= b;
                r.left |= l;
                r.right |= rt;
                sticks_move = true;
                self.peak[i] = self.peak[i].max(stick[0].hypot(stick[1]));
                self.walk[i] = Some(self.peak[i] < RUN_DEFLECTION);
            } else {
                self.peak[i] = 0.0;
            }
        }
        let dead = |v: f32| if v.abs() < DEAD_ZONE { 0.0 } else { v };
        r.turn[0] = (r.turn[0] + dead(axis(GamepadAxis::RightX))).clamp(-1.0, 1.0);
        r.turn[1] = (r.turn[1] + dead(axis(GamepadAxis::RightY))).clamp(-1.0, 1.0);
        // The standard touch look: mouse counts through the look curve.
        let moves: &[([f32; 2], f32)] = if keys_only {
            &[]
        } else {
            input.touch_look_moves()
        };
        for &(delta, speed) in moves {
            let gain = look_gain(speed);
            r.mouse[0] += delta[0] * gain;
            r.mouse[1] += delta[1] * gain;
        }
        // The experimental touch look: under a screen nothing looks and the
        // momentum ends.
        r.look = if keys_only {
            self.look
                .frame(&[], true, player.pitch, player.pitch_limits, now)
        } else {
            self.look.frame(
                input.touch_looks(),
                input.looks_released(),
                player.pitch,
                player.pitch_limits,
                now,
            )
        };
        // Tap-to-move: forward toward the trip's destination, running
        // (touch's latch, unless the low crouch holds it walking). A key or
        // a stick moving, a screen or a blur ends the trip.
        let taps: Vec<_> = self
            .look
            .taps()
            .iter()
            .map(|t| (*t, view.destination(t.at)))
            .collect();
        let marker = self
            .tap_move
            .trip()
            .and_then(|t| view.picture_point(t.destination));
        if taps.iter().any(|(_, d)| d.is_some()) {
            self.walk[0] = None;
            if !player.crouch_toggled {
                self.walk[1] = Some(false);
            }
        }
        let ended = keyboard || sticks_move || keys_only || input.looks_released();
        let (toward, sprint) = self.tap_move.frame(&taps, marker, ended, player, now);
        r.sprint_toggle ^= sprint;
        if toward.is_some() {
            r.forward = true;
            r.toward = toward;
        }
        for walk in self.walk.into_iter().flatten() {
            r.accel |= self.settings.always_run == walk;
        }
        let back = pad(GamepadButton::East, true) || touch(TouchButton::Menu, true);
        (r, back)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::input::Event;

    /// The movement and look lines of the shipped `default_controls.ltx`
    /// (section 2.2), with others around them.
    const DEFAULTS: &[u8] = b"bind left kLEFT\nbind right kRIGHT\nbind up kUP\nbind down kDOWN\n\
bind forward kW\nbind back kS\nbind lstrafe kA\nbind rstrafe kD\nbind llookout kQ\n\
bind rlookout kE\nbind jump kSPACE\nbind crouch kLCONTROL\nbind accel kLSHIFT\n\
bind sprint_toggle kX\nbind wpn_fire mouse1\nbind quit kESCAPE\n";

    fn controls() -> Controls {
        Controls::new(Bindings::read(&[DEFAULTS]), Settings::default())
    }

    /// A player standing, nothing requested.
    fn standing() -> PlayerState {
        PlayerState {
            position: [0.0; 3],
            velocity: [0.0; 3],
            airborne: false,
            flags: crate::core::movement::Flags::default(),
            posture: 0,
            sprint_requested: false,
            crouch_toggled: false,
            yaw: 0.0,
            pitch: 0.0,
            pitch_limits: [-1.5, 1.5],
        }
    }

    /// One frame's request for a standing player; taps meet nothing.
    fn req(c: &mut Controls, i: &Input) -> (Request, bool) {
        c.request(i, &standing(), 0, &())
    }

    fn key(input: &mut Input, key: Key, pressed: bool) {
        input.handle(Event::Key { key, pressed });
    }

    #[test]
    fn reads_bindings_with_unbindall_and_secondary_keys() {
        let b = Bindings::read(&[DEFAULTS]);
        let one = |a| b.controls(a).collect::<Vec<_>>();
        assert_eq!(one(MoveAction::Forward), [Control::Key(Key::W)]);
        assert_eq!(one(MoveAction::Crouch), [Control::Key(Key::LeftControl)]);
        assert_eq!(
            one(MoveAction::WpnFire),
            [Control::Mouse(MouseButton::Left)]
        );
        assert!(
            one(MoveAction::CrouchToggle).is_empty(),
            "unbound by default"
        );
        let user = b"unbindall\nbind jump kX\nbind_sec jump kSPACE\nbind forward kUNKNOWN\n";
        let b = Bindings::read(&[DEFAULTS, user]);
        assert_eq!(
            b.controls(MoveAction::Jump).collect::<Vec<_>>(),
            [Control::Key(Key::X), Control::Key(Key::Space)]
        );
        assert_eq!(b.controls(MoveAction::Forward).count(), 0);
        assert_eq!(b.controls(MoveAction::Back).count(), 0);
    }

    #[test]
    fn keyboard_and_mouse_follow_the_bindings() {
        let mut c = controls();
        let mut i = Input::new();
        key(&mut i, Key::W, true);
        key(&mut i, Key::LeftShift, true);
        key(&mut i, Key::Space, true);
        key(&mut i, Key::X, true);
        key(&mut i, Key::Left, true);
        i.handle(Event::PointerMotion { delta: [3.0, -2.0] });
        let (r, back) = req(&mut c, &i);
        assert!(r.forward && r.accel && r.jump_pressed && r.sprint_toggle && !back);
        assert!(!r.back && !r.left && !r.crouch);
        assert_eq!(r.turn, [-1.0, 0.0]);
        assert_eq!(r.mouse, [3.0, -2.0]);
        i.end_frame();
        key(&mut i, Key::Space, false);
        key(&mut i, Key::Escape, true);
        let (r, back) = req(&mut c, &i);
        assert!(r.jump_released && !r.jump_pressed && !r.sprint_toggle);
        assert!(!back, "the quit key is the app's");
        i.handle(Event::GamepadButton {
            button: GamepadButton::East,
            pressed: true,
        });
        assert!(req(&mut c, &i).1, "a gamepad's east button");
        // A gamepad's d-pad acts as the arrow keys in the menu, not as the
        // look keys here.
        let mut i = Input::new();
        i.handle(Event::GamepadButton {
            button: GamepadButton::DpadUp,
            pressed: true,
        });
        assert_eq!(req(&mut c, &i).0.turn, [0.0, 0.0]);
    }

    #[test]
    fn sticks_give_eight_directions_and_walk_when_pushed_a_little() {
        assert_eq!(stick_directions([0.1, 0.1]), None);
        assert_eq!(
            stick_directions([0.0, -1.0]),
            Some(([true, false, false, false], true))
        );
        assert_eq!(
            stick_directions([0.7, -0.7]),
            Some(([true, false, false, true], true))
        );
        assert_eq!(
            stick_directions([-0.3, 0.05]),
            Some(([false, false, true, false], false))
        );
        let mut c = controls();
        let mut i = Input::new();
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: -0.4,
        });
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::RightX,
            value: 0.5,
        });
        i.handle(Event::GamepadButton {
            button: GamepadButton::West,
            pressed: true,
        });
        i.handle(Event::GamepadButton {
            button: GamepadButton::South,
            pressed: true,
        });
        let (r, _) = req(&mut c, &i);
        // Walking with g_always_run on is `accel` held.
        assert!(r.forward && r.accel && r.crouch_toggle && r.jump_pressed);
        assert_eq!(r.turn, [0.5, 0.0]);
        // Released, the stick keeps walking: crouching now is the low
        // crouch, held without the stick.
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: 0.0,
        });
        let (r, _) = req(&mut c, &i);
        assert!(!r.forward && r.accel, "the walk choice is kept");
        // A full push returning gradually through the walk band stays a run.
        for value in [-1.0, -0.8, -0.5, -0.3, -0.1, 0.0] {
            i.handle(Event::GamepadAxis {
                axis: GamepadAxis::LeftY,
                value,
            });
            assert!(!req(&mut c, &i).0.accel, "running at {value}");
        }
        i.handle(Event::GamepadButton {
            button: GamepadButton::East,
            pressed: true,
        });
        assert!(req(&mut c, &i).1, "east goes back");
    }

    #[test]
    fn the_keyboard_walks_with_always_run_off() {
        let mut c = Controls::new(
            Bindings::read(&[DEFAULTS]),
            Settings {
                always_run: false,
                ..Settings::default()
            },
        );
        let mut i = Input::new();
        // A stick pushed fully once, then the keyboard: W alone walks
        // (`accel` not held), W with Shift runs, Control with W walks into
        // the low crouch.
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: -1.0,
        });
        assert!(req(&mut c, &i).0.accel, "the stick runs");
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: 0.0,
        });
        assert!(req(&mut c, &i).0.accel, "and stays latched");
        key(&mut i, Key::W, true);
        let (r, _) = req(&mut c, &i);
        assert!(r.forward && !r.accel, "W walks");
        assert_eq!(c.walk, [None; 2], "the keyboard cleared the latches");
        key(&mut i, Key::LeftShift, true);
        assert!(req(&mut c, &i).0.accel, "Shift runs");
        key(&mut i, Key::LeftShift, false);
        key(&mut i, Key::LeftControl, true);
        let (r, _) = req(&mut c, &i);
        assert!(r.crouch && !r.accel, "crouched walking: the low crouch");
        // Untouched sticks latch nothing.
        let mut c = Controls::new(
            Bindings::read(&[DEFAULTS]),
            Settings {
                always_run: false,
                ..Settings::default()
            },
        );
        assert_eq!(req(&mut c, &Input::new()).0, Request::default());
        assert_eq!(c.walk, [None; 2]);
    }

    #[test]
    fn a_release_of_everything_keeps_the_latch_and_presses_nothing_again() {
        let mut c = controls();
        let mut i = Input::new();
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: -1.0,
        });
        i.handle(Event::GamepadButton {
            button: GamepadButton::West,
            pressed: true,
        });
        assert!(req(&mut c, &i).0.crouch_toggle);
        i.end_frame();
        // A blur: everything released; the platform sends the stick again,
        // held back in the walk band, and the button state not at all.
        i.release_all();
        let (r, _) = req(&mut c, &i);
        assert!(!r.crouch_toggle && !r.accel && !r.forward);
        i.end_frame();
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: -0.4,
        });
        let (r, _) = req(&mut c, &i);
        assert!(r.forward && !r.accel, "the same push: still running");
        assert!(!r.crouch_toggle && !i.pad_held(GamepadButton::West));
        assert_eq!(c.walk[0], Some(false));
    }

    #[test]
    fn touch_controls() {
        let mut c = controls();
        let mut i = Input::new();
        for button in [TouchButton::Jump, TouchButton::Crouch] {
            i.handle(Event::TouchButton {
                button,
                pressed: true,
            });
        }
        let (r, back) = req(&mut c, &i);
        assert!(r.jump_pressed && r.crouch_toggle && !r.forward && !r.sprint_toggle);
        assert!(!r.crouch, "the touch crouch is the toggle");
        assert!(!back);
        i.end_frame();
        i.handle(Event::TouchButton {
            button: TouchButton::Jump,
            pressed: false,
        });
        i.handle(Event::TouchButton {
            button: TouchButton::Menu,
            pressed: true,
        });
        let (r, back) = req(&mut c, &i);
        assert!(r.jump_released && back);
    }

    #[test]
    fn the_standard_touch_controls() {
        let mut c = controls();
        let mut i = Input::new();
        i.handle(Event::TouchStick { value: [0.0, -0.9] });
        // A slow drag (190 px/s): the base gain, 3.25 counts per pixel.
        i.handle(Event::TouchLookDelta {
            delta: [3.0, 1.0],
            speed: 190.0,
        });
        for button in [TouchButton::Jump, TouchButton::Crouch] {
            i.handle(Event::TouchButton {
                button,
                pressed: true,
            });
        }
        let (r, back) = req(&mut c, &i);
        assert!(r.forward && !r.accel && r.jump_pressed && r.crouch_toggle);
        assert!(!r.sprint_toggle, "a full push within the rim runs");
        assert!(!r.crouch, "the touch crouch is the toggle");
        assert_eq!(r.mouse, [9.75, 3.25]);
        assert!(!back);
        i.end_frame();
        i.handle(Event::TouchButton {
            button: TouchButton::Jump,
            pressed: false,
        });
        i.handle(Event::TouchButton {
            button: TouchButton::Menu,
            pressed: true,
        });
        let (r, back) = req(&mut c, &i);
        assert!(r.jump_released && back && r.forward);
        // Released, then a light push and let go: the touch stick keeps
        // walking too.
        i.handle(Event::TouchStick { value: [0.0, 0.0] });
        req(&mut c, &i);
        i.handle(Event::TouchStick { value: [0.0, -0.3] });
        assert!(req(&mut c, &i).0.accel);
        i.handle(Event::TouchStick { value: [0.0, 0.0] });
        let (r, _) = req(&mut c, &i);
        assert!(r.accel && !r.forward);
    }

    #[test]
    fn the_look_curve_is_the_base_gain_when_slow_and_twice_it_when_fast() {
        assert_eq!(look_gain(0.0), LOOK_BASE_GAIN);
        assert_eq!(look_gain(LOOK_PRECISE_SPEED), LOOK_BASE_GAIN);
        assert!((look_gain(1150.0) - 4.875).abs() < 1e-6);
        assert_eq!(look_gain(LOOK_FAST_SPEED), LOOK_MAX_GAIN);
        assert_eq!(look_gain(10_000.0), LOOK_MAX_GAIN);
        assert_eq!(look_gain(f32::NAN), LOOK_BASE_GAIN);
        let mut last = 0.0;
        for speed in (0..3000).step_by(50) {
            let g = look_gain(speed as f32);
            assert!(
                g >= last && (LOOK_BASE_GAIN..=LOOK_MAX_GAIN).contains(&g),
                "{speed}: {g}"
            );
            last = g;
        }
        // Through the controls: a 40-pixel flick at 3600 px/s turns 6.5
        // counts a pixel, the same pixels slowly 3.25 each. The counts
        // are mouse counts: the movement layer scales them by `mouse_sens`
        // and `mouse_invert` (8.1).
        let mut c = controls();
        let mut i = Input::new();
        i.handle(Event::TouchLookDelta {
            delta: [0.0, 40.0],
            speed: 3600.0,
        });
        assert_eq!(req(&mut c, &i).0.mouse, [0.0, 260.0]);
        i.end_frame();
        i.handle(Event::TouchLookDelta {
            delta: [0.0, 40.0],
            speed: 40.0,
        });
        assert_eq!(req(&mut c, &i).0.mouse, [0.0, 130.0]);
    }

    #[test]
    fn uneven_event_batching_at_a_steady_drag_gives_a_steady_gain() {
        // A drag at a steady 1150 px/s, its events (10 px each) falling
        // into frames unevenly, as 120 Hz events into 60 Hz frames with
        // jitter do: every frame turns 4.875 counts per pixel.
        let mut c = controls();
        let mut i = Input::new();
        for batch in [1, 3, 0, 2, 1, 4, 2, 0, 1] {
            for _ in 0..batch {
                i.handle(Event::TouchLookDelta {
                    delta: [10.0, 0.0],
                    speed: 1150.0,
                });
            }
            let turned = req(&mut c, &i).0.mouse[0];
            let pixels = 10.0 * batch as f32;
            assert!((turned - 4.875 * pixels).abs() < 1e-3, "{batch}: {turned}");
            i.end_frame();
        }
    }

    /// The default lock target.
    const AHEAD: [f32; 2] = [0.0, -LOCK_REACH];

    /// One frame of the lock with the stick's `values` and the default
    /// target.
    fn lock(l: &mut SprintLock, values: &[[f32; 2]], requested: bool) -> bool {
        l.frame(&some(values), false, requested, AHEAD)
    }

    fn some(values: &[[f32; 2]]) -> Vec<Option<[f32; 2]>> {
        values.iter().copied().map(Some).collect()
    }

    #[test]
    fn past_the_rim_sprints_and_the_lock_target_keeps_it() {
        let mut l = SprintLock::default();
        // A full push within the rim, and past it aside: nothing.
        assert!(!lock(&mut l, &[[0.0, -1.0]], false));
        assert!(!lock(&mut l, &[[1.6, -0.2]], false) && !l.past());
        // Forward past the rim: sprint requested, once.
        assert!(lock(&mut l, &[[0.2, -1.4]], false) && l.past());
        assert!(
            !lock(&mut l, &[[0.0, -1.2]], true),
            "hysteresis: still past"
        );
        // Back within the rim: the sprint ends.
        assert!(lock(&mut l, &[[0.0, -1.1]], true) && !l.past());
        // Past again, then let go on the target within one frame: locked.
        assert!(lock(&mut l, &[[0.0, -1.5]], false));
        assert!(!lock(&mut l, &[[0.1, -2.0], [0.0, 0.0]], true));
        assert!(l.locked());
        assert!(!lock(&mut l, &[], true) && l.locked(), "hands-free");
        // A touch on the stick ends it and its sprint.
        assert!(lock(&mut l, &[[0.0, 0.0]], true) && !l.locked());
        // Let go past the rim but off the target: the sprint ends.
        assert!(lock(&mut l, &[[0.0, -1.5]], false));
        assert!(lock(&mut l, &[[0.0, 0.0]], true) && !l.past() && !l.locked());
        // Everything released (focus lost) while locked: the lock ends.
        lock(&mut l, &[[0.0, -2.1]], false);
        lock(&mut l, &[[0.0, 0.0]], true);
        assert!(l.locked());
        assert!(l.frame(&[], true, true, AHEAD) && !l.locked());
        // A cancelled touch on the target: no lock, the sprint ends.
        assert!(lock(&mut l, &[[0.0, -1.5]], false));
        let on_target = [Some([0.0, -2.1]), None];
        assert!(l.frame(&on_target, false, true, AHEAD) && !l.locked());
    }

    #[test]
    fn the_lock_target_moved_to_fit_the_screen_is_where_it_locks() {
        // The page's target, its distance kept from all past the rim to
        // the default; nonsense gives the default.
        assert_eq!(lock_target(None), AHEAD);
        assert_eq!(lock_target(Some([f32::NAN, 0.0])), AHEAD);
        assert_eq!(lock_target(Some([0.0, 0.0])), AHEAD);
        assert_eq!(lock_target(Some([0.0, -9.0])), AHEAD);
        assert_eq!(lock_target(Some([0.0, -0.5])), [0.0, -LOCK_MIN_REACH]);
        assert_eq!(lock_target(Some([0.0, -1.9])), [0.0, -1.9]);
        // A thumb landing high: the target closer. A let-go there locks;
        // at the default target's place it doesn't.
        let mut l = SprintLock::default();
        let low = [0.0, -LOCK_MIN_REACH];
        assert!(l.frame(&some(&[[0.0, -1.5]]), false, false, low));
        assert!(!l.frame(&some(&[low, [0.0, 0.0]]), false, true, low));
        assert!(l.locked());
        l.frame(&some(&[[0.0, 0.0]]), false, true, low);
        l.frame(&some(&[[0.0, -1.5]]), false, false, low);
        l.frame(&some(&[[0.0, -2.6], [0.0, 0.0]]), false, true, low);
        assert!(!l.locked());
        // A thumb at the very top: the target beside it, to the right.
        // Past the rim counts toward it, and a let-go on it locks; a push
        // straight ahead no longer reaches past the rim.
        let side = [LOCK_MIN_REACH, 0.0];
        let mut l = SprintLock::default();
        assert!(!l.frame(&some(&[[0.0, -1.6]]), false, false, side) && !l.past());
        l.frame(&some(&[[0.0, 0.0]]), false, false, side);
        assert!(l.frame(&some(&[[1.5, -0.1]]), false, false, side) && l.past());
        assert!(!l.frame(&some(&[side, [0.0, 0.0]]), false, true, side));
        assert!(l.locked());
        // Through the controls: the page's message moves the target, and
        // the lock still sprints forward.
        let mut w = Walk::new();
        w.event(Event::TouchLockTarget { centre: side });
        w.stick([0.0, 0.0]).stick([1.5, 0.0]).frames(2);
        w.stick(side).stick([0.0, 0.0]);
        let s = w.frames(10);
        assert!(w.c.sprint_lock.locked() && s.flags.sprint && s.flags.forward);
    }

    #[test]
    fn the_movement_layer_cancelling_the_sprint_ends_the_lock() {
        let mut l = SprintLock::default();
        assert!(lock(&mut l, &[[0.0, -1.5]], false));
        // Cancelled (e.g. a crouch) while held past the rim: no fresh
        // request until the stick returns within the rim and goes past
        // again, and a let-go on the target doesn't lock.
        assert!(!lock(&mut l, &[[0.0, -1.6]], false) && l.past());
        assert!(!lock(&mut l, &[[0.0, -2.1], [0.0, 0.0]], false) && !l.locked());
        assert!(
            lock(&mut l, &[[0.0, -0.5], [0.0, -1.5]], false),
            "past again"
        );
        // Locked, then cancelled: the lock lets the stick go.
        assert!(!lock(&mut l, &[[0.0, -2.1], [0.0, 0.0]], true) && l.locked());
        assert!(!lock(&mut l, &[], false) && !l.locked());
    }

    #[test]
    fn crouch_swiped_down_while_the_lock_holds_is_the_low_crouch() {
        // Locked, then Crouch swiped down in the lock's last held frame:
        // the lock lets go at once, so the latch stays walking.
        let mut w = Walk::new();
        w.stick([0.0, 0.0]).stick([0.0, -1.5]).frames(2);
        w.stick([0.0, -2.1]).stick([0.0, 0.0]);
        assert!(w.frames(10).flags.sprint && w.c.sprint_lock.locked());
        w.tap(TouchButton::CrouchLow);
        let s = w.frames(30);
        assert!(!w.c.sprint_lock.locked() && !s.sprint_requested);
        assert!(s.posture == 2 && s.crouch_toggled, "{s:?}");
    }

    #[test]
    fn the_sprint_lock_sprints_hands_free_until_the_movement_layer_ends_it() {
        let mut w = Walk::new();
        w.stick([0.0, 0.0]).stick([0.0, -1.0]);
        assert!(!w.frames(10).flags.sprint, "a full push runs");
        w.stick([0.0, -1.5]);
        let s = w.frames(10);
        assert!(s.flags.sprint && s.sprint_requested, "past the rim");
        // Let go on the lock target: still sprinting forward, nobody on
        // the stick.
        w.stick([0.0, -2.1]).stick([0.0, 0.0]);
        let s = w.frames(60);
        assert!(w.c.sprint_lock.locked() && s.flags.sprint && s.flags.forward);
        // A touch on the stick: it stops.
        w.stick([0.0, 0.0]);
        let s = w.frames(1);
        assert!(!w.c.sprint_lock.locked() && !s.sprint_requested);
        w.stick([0.0, 0.0]);
        assert!(!w.frames(60).flags.forward);
        // Locked again, then Crouch: the spec cancels the sprint (4.3) and
        // the lock lets go.
        w.stick([0.0, -2.1]).stick([0.0, 0.0]);
        assert!(w.frames(10).flags.sprint && w.c.sprint_lock.locked());
        w.tap(TouchButton::Crouch);
        let s = w.frames(2);
        assert!(!s.sprint_requested && !w.c.sprint_lock.locked());
        assert!(!w.frames(60).flags.forward, "stopped");
    }

    #[test]
    fn a_latched_walk_holds_the_low_crouch_without_the_stick() {
        let mut w = Walk::new();
        // A light push on the touch stick, let go; then Crouch.
        w.stick([0.0, -0.3]).frames(1);
        w.stick([0.0, 0.0]).frames(1);
        w.tap(TouchButton::Crouch);
        assert_eq!(w.frames(30).posture, 2, "the low crouch, held");
        // A full push runs: the crouch rises to box 1.
        w.stick([0.0, -1.0]);
        assert_eq!(w.frames(1).posture, 1);
        // Let go, then Crouch swiped down: the low crouch again, still
        // toggled.
        w.stick([0.0, 0.0]).frames(1);
        w.tap(TouchButton::CrouchLow);
        let s = w.frames(30);
        assert!(s.posture == 2 && s.crouch_toggled);
        // Standing, swiped down: straight to the low crouch.
        w.tap(TouchButton::Crouch);
        assert_eq!(w.frames(30).posture, 0);
        w.tap(TouchButton::CrouchLow);
        assert_eq!(w.frames(30).posture, 2);
    }

    /// The picture's half height in CSS pixels in the tests.
    const HALF: f32 = 220.0;

    fn look(finger: u32, phase: LookPhase, at: [f32; 2], time: f64) -> TouchLook {
        TouchLook {
            finger,
            phase,
            at,
            time,
            half_height: HALF,
        }
    }

    const LIMITS: [f32; 2] = [-1.5, 1.5];

    fn tan_half() -> f32 {
        (crate::core::camera::FOV_Y_DEGREES.to_radians() / 2.0).tan()
    }

    /// The world direction (yaw-0 frame turned by `yaw`) under a finger at
    /// `at` with the camera at `yaw`, `pitch`.
    fn under(at: [f32; 2], yaw: f32, pitch: f32) -> [f32; 3] {
        let t = tan_half();
        let c = [at[0] * t, -at[1] * t, 1.0f32];
        let l = (c[0] * c[0] + c[1] * c[1] + 1.0).sqrt();
        let c = c.map(|v| v / l);
        let (s, k) = pitch.sin_cos();
        let w = [c[0], c[1] * k - c[2] * s, c[1] * s + c[2] * k];
        let (s, k) = yaw.sin_cos();
        [w[0] * k + w[2] * s, w[1], -w[0] * s + w[2] * k]
    }

    #[test]
    fn the_scene_under_the_finger_stays_under_it() {
        // A finger wandering over the picture, in uneven steps and frames:
        // the direction first under it is under it at every step.
        let mut d = TouchLookDrag::default();
        let (mut yaw, mut pitch) = (0.3f32, 0.2f32);
        let start = [0.4, 0.3];
        let grabbed = under(start, yaw, pitch);
        let mut events = vec![look(1, LookPhase::Down, start, 0.0)];
        let path = [
            [0.1, 0.2],
            [-0.5, -0.1],
            [-0.9, 0.6],
            [0.7, -0.7],
            [0.0, 0.0],
        ];
        for (i, at) in path.into_iter().enumerate() {
            events.push(look(1, LookPhase::Move, at, 10.0 * (i + 1) as f64));
            let t = d.frame(&events, false, pitch, LIMITS, 16 * i as u64);
            events.clear();
            yaw += t[0];
            pitch += t[1];
            let now = under(at, yaw, pitch);
            for k in 0..3 {
                assert!(
                    (now[k] - grabbed[k]).abs() < 1e-4,
                    "{at:?}: {now:?} {grabbed:?}"
                );
            }
        }
        // Dragged right the scene goes right: the view turns left; dragged
        // down it looks up.
        let right = grab_turn([0.0, 0.0], [0.2, 0.0], 0.0, LIMITS, tan_half());
        assert!(right[0] < 0.0 && right[1].abs() < 1e-6);
        let down = grab_turn([0.0, 0.0], [0.0, 0.2], 0.0, LIMITS, tan_half());
        assert!(down[1] < 0.0 && down[0].abs() < 1e-6);
        // Near the middle a pixel turns tan_half / half a picture's height.
        let tiny = grab_turn([0.0, 0.0], [0.001, 0.0], 0.0, LIMITS, tan_half());
        assert!((tiny[0] + 0.001 * tan_half()).abs() < 1e-6);
        // At the pitch limit the pitch stops, the yaw still follows.
        let held = grab_turn([0.0, 0.0], [0.3, 0.9], -1.4, LIMITS, tan_half());
        assert!((held[1] - (-0.1)).abs() < 1e-6 && held[0] < 0.0, "{held:?}");
    }

    /// A flick: a finger at `speed` (half-heights a millisecond) across
    /// the middle, moving every 8 ms for 80 ms then lifted; the drag's turn.
    fn flick(d: &mut TouchLookDrag, speed: f32, now: &mut u64, frame: u64) -> f32 {
        let mut turned = 0.0;
        let mut events = vec![look(1, LookPhase::Down, [-0.32 * speed * 10.0, 0.0], 0.0)];
        for i in 1..=10 {
            let x = -0.32 * speed * 10.0 + speed * 8.0 * i as f32;
            events.push(look(1, LookPhase::Move, [x, 0.0], 8.0 * f64::from(i)));
            if i % 2 == 0 {
                *now += 16;
                turned += d.frame(&events, false, 0.0, LIMITS, *now)[0];
                events.clear();
            }
        }
        events.push(look(
            1,
            LookPhase::Up,
            [-0.32 * speed * 10.0 + speed * 80.0, 0.0],
            81.0,
        ));
        *now += frame;
        turned + d.frame(&events, false, 0.0, LIMITS, *now)[0]
    }

    /// Frames of `frame` ms (uneven when 0) for `ms` ms; the yaw turned.
    fn coast(d: &mut TouchLookDrag, now: &mut u64, frame: u64, ms: u64) -> f32 {
        let mut turned = 0.0;
        let end = *now + ms;
        let mut i = 0;
        while *now < end {
            let step = if frame == 0 {
                [5, 17, 33, 8, 41][i % 5]
            } else {
                frame
            };
            *now = (*now + step).min(end);
            turned += d.frame(&[], false, 0.0, LIMITS, *now)[0];
            i += 1;
        }
        turned
    }

    #[test]
    fn a_flick_coasts_slowing_as_a_scroll_view_does_at_any_frame_rate() {
        let speed = 0.004; // half-heights a millisecond, across the middle
        let mut results = Vec::new();
        for frame in [8, 16, 33, 0] {
            let mut d = TouchLookDrag::default();
            let mut now = 0;
            flick(&mut d, speed, &mut now, 16);
            let v = d.momentum()[0];
            // The release velocity: the finger's angular speed where it
            // was let go (moving right, so turning left).
            let x = 0.0;
            let expected = -speed * tan_half() / (1.0 + x * x * tan_half() * tan_half());
            assert!(
                (v - expected).abs() < 0.03 * expected.abs(),
                "{v} {expected}"
            );
            // After 500 ms: r^500 of it left, and v (1 − r^500) / −ln r
            // turned; in all, v / −ln r (the projection, v r / (1 − r) in
            // the WWDC 2018 form, within 0.1%).
            let half = coast(&mut d, &mut now, frame, 500);
            let ln = -(LOOK_DECELERATION.ln() as f32);
            let keep = LOOK_DECELERATION.powi(500) as f32;
            assert!((d.momentum()[0] - v * keep).abs() < 1e-6, "{frame}");
            assert!(
                (half - v * (1.0 - keep) / ln).abs() < 1e-4,
                "{frame}: {half}"
            );
            let total = half + coast(&mut d, &mut now, frame, 10_000);
            assert!((total - v / ln).abs() < LOOK_REST * 2.0, "{frame}: {total}");
            assert_eq!(d.momentum(), [0.0; 2], "it comes to rest");
            results.push(total);
        }
        for t in &results {
            assert!((t - results[0]).abs() < 1e-4, "{results:?}");
        }
    }

    #[test]
    fn a_touch_stops_the_momentum_and_a_stopped_finger_flings_nothing() {
        let mut d = TouchLookDrag::default();
        let mut now = 0;
        flick(&mut d, 0.004, &mut now, 16);
        coast(&mut d, &mut now, 16, 100);
        assert!(d.momentum()[0] != 0.0);
        now += 16;
        let t = d.frame(
            &[look(2, LookPhase::Down, [0.5, 0.0], 900.0)],
            false,
            0.0,
            LIMITS,
            now,
        );
        assert!(t[0] != 0.0, "this frame's share still turns");
        assert_eq!(d.momentum(), [0.0; 2]);
        assert_eq!(coast(&mut d, &mut now, 16, 200), 0.0, "held still: no turn");
        // Lifted after a pause: no fling.
        let hold = [
            look(2, LookPhase::Move, [0.6, 0.0], 910.0),
            look(2, LookPhase::Move, [0.7, 0.0], 920.0),
            look(2, LookPhase::Up, [0.7, 0.0], 1010.0),
        ];
        now += 16;
        d.frame(&hold, false, 0.0, LIMITS, now);
        assert_eq!(d.momentum(), [0.0; 2]);
        // Cancelled, released, or after a long gap: none either.
        for end in 0..3 {
            let mut d = TouchLookDrag::default();
            let mut now = 0;
            flick(&mut d, 0.004, &mut now, 16);
            let v = d.momentum();
            if end == 0 {
                let mut d2 = TouchLookDrag::default();
                let mut events = vec![look(1, LookPhase::Down, [0.0, 0.0], 0.0)];
                events.push(look(1, LookPhase::Move, [0.1, 0.0], 8.0));
                events.push(look(1, LookPhase::Cancel, [0.1, 0.0], 9.0));
                d2.frame(&events, false, 0.0, LIMITS, 16);
                assert_eq!(d2.momentum(), [0.0; 2]);
            } else if end == 1 {
                d.frame(&[], true, 0.0, LIMITS, now + 16);
                assert_eq!(d.momentum(), [0.0; 2]);
            } else {
                assert_eq!(
                    d.frame(&[], false, 0.0, LIMITS, now + LOOK_GAP_MS + 1),
                    [0.0; 2]
                );
                assert_eq!(d.momentum(), [0.0; 2], "{v:?}");
            }
        }
    }

    #[test]
    fn the_momentum_pitch_stops_at_the_limit() {
        let mut d = TouchLookDrag::default();
        // Flicked up from just below the lower limit's reach: the view
        // looks down, stops at the limit, keeps its yaw.
        let mut events = vec![look(1, LookPhase::Down, [0.0, 0.0], 0.0)];
        for i in 1..=10 {
            events.push(look(
                1,
                LookPhase::Move,
                [0.03 * i as f32, -0.03 * i as f32],
                8.0 * f64::from(i),
            ));
        }
        events.push(look(1, LookPhase::Up, [0.3, -0.3], 81.0));
        let mut pitch = 1.3;
        let t = d.frame(&events, false, pitch, LIMITS, 16);
        pitch += t[1];
        assert!(d.momentum()[1] > 0.0 && d.momentum()[0] < 0.0);
        let mut now = 16;
        for _ in 0..200 {
            now += 16;
            pitch += d.frame(&[], false, pitch, LIMITS, now)[1];
            assert!(pitch <= LIMITS[1] + 1e-6);
        }
        assert_eq!(pitch, LIMITS[1]);
        assert_eq!(d.momentum()[1], 0.0);
    }

    #[test]
    fn touch_look_through_the_controls_is_turned_as_it_is() {
        // Through the controls the turn is radians for the movement layer
        // as they are: no `mouse_sens`, no mouse counts.
        let mut c = controls();
        let mut i = Input::new();
        i.handle(Event::TouchLook(look(4, LookPhase::Down, [0.0, 0.0], 0.0)));
        i.handle(Event::TouchLook(look(4, LookPhase::Move, [0.0, 0.1], 10.0)));
        let (r, _) = req(&mut c, &i);
        assert_eq!(r.mouse, [0.0; 2]);
        assert!(
            (r.look[1] + (0.1 * tan_half()).atan()).abs() < 1e-6,
            "{:?}",
            r.look
        );
        // Under a screen: nothing, and the momentum ends.
        i.end_frame();
        i.handle(Event::TouchLook(look(4, LookPhase::Move, [0.0, 0.2], 20.0)));
        c.keys_only = Some(Vec::new());
        assert_eq!(req(&mut c, &i).0.look, [0.0; 2]);
        assert_eq!(c.look.momentum(), [0.0; 2]);
    }

    #[test]
    fn crouch_swiped_down_is_the_low_crouch() {
        let mut c = controls();
        let mut i = Input::new();
        let swipe = |i: &mut Input| {
            for pressed in [true, false] {
                i.handle(Event::TouchButton {
                    button: TouchButton::CrouchLow,
                    pressed,
                });
            }
        };
        // Standing: the toggle and walking.
        swipe(&mut i);
        let (r, _) = req(&mut c, &i);
        assert!(r.crouch_toggle && r.accel);
        assert_eq!(c.walk[1], Some(true));
        i.end_frame();
        // Crouch pressed first (toggled on), then swiped in the same frame,
        // or toggled already: no second toggle.
        i.handle(Event::TouchButton {
            button: TouchButton::Crouch,
            pressed: true,
        });
        swipe(&mut i);
        assert!(req(&mut c, &i).0.crouch_toggle);
        i.end_frame();
        swipe(&mut i);
        let toggled = PlayerState {
            crouch_toggled: true,
            ..standing()
        };
        let (r, _) = c.request(&i, &toggled, 0, &());
        assert!(!r.crouch_toggle && r.accel);
    }

    /// The material id of pass-through triangles in [`world_with`].
    const PASS: u32 = 7;

    /// A floor of the default material, facing up, with `extra` triangles
    /// (corners, material id: 0, or [`PASS`] for pass-through).
    fn world_with(extra: &[([[f32; 3]; 3], u32)]) -> crate::core::collision::CollisionWorld {
        use crate::core::collision::{Cform, CformTriangle, CollisionWorld};
        use volya_engine_formats::gamemtl::{GameMaterial, PASS_THROUGH};

        let mut vertices = vec![
            [-50.0, 0.0, -50.0],
            [-50.0, 0.0, 50.0],
            [50.0, 0.0, 50.0],
            [50.0, 0.0, -50.0],
        ];
        let mut triangles: Vec<_> = [[0, 1, 2], [0, 2, 3]]
            .map(|vertices| CformTriangle {
                vertices,
                attributes: 0,
            })
            .to_vec();
        for &(corners, material) in extra {
            let n = vertices.len() as u32;
            vertices.extend(corners);
            triangles.push(CformTriangle {
                vertices: [n, n + 1, n + 2],
                attributes: material,
            });
        }
        let cform = Cform {
            bbox_min: [-50.0, -1.0, -50.0],
            bbox_max: [50.0, 10.0, 50.0],
            vertices,
            triangles,
        };
        CollisionWorld::new(
            cform,
            vec![
                GameMaterial {
                    id: 0,
                    ..GameMaterial::default()
                },
                // For ladders (docs/behavior/climbing.md 1.2).
                GameMaterial {
                    id: 82,
                    name: "materials\\fake_ladders".into(),
                    ..GameMaterial::default()
                },
                GameMaterial {
                    id: PASS,
                    flags: PASS_THROUGH,
                    ..GameMaterial::default()
                },
            ],
        )
        .unwrap()
    }

    /// A floor of the default material, facing up, and the shipped
    /// movement configuration's values (player-movement.md section 1).
    fn floor() -> (
        crate::core::collision::CollisionWorld,
        crate::core::movement::MovementConfig,
    ) {
        use crate::core::movement::{Bobbing, MovementConfig};
        let world = world_with(&[]);
        let config = MovementConfig {
            walk_accel: 17.0,
            run_coef: 2.1,
            run_back_coef: 1.8,
            walk_back_coef: 0.8,
            crouch_coef: 0.45,
            climb_coef: 0.7,
            sprint_koef: 2.6,
            walk_strafe_coef: 1.0,
            run_strafe_coef: 1.0,
            jump_speed: 6.0,
            air_control_param: 0.1,
            camera_height_factor: 0.85,
            crash_speed_min: 14.0,
            crash_speed_max: 30.0,
            body: crate::core::collision::body::BodyConfig {
                boxes: [[0.35, 0.9, 0.35], [0.35, 0.75, 0.35], [0.35, 0.6, 0.35]],
                mass: 80.0,
            },
            lim_pitch: [-1.5, 1.5],
            rot_speed: [3.0, 3.0, 0.0],
            bobbing: Bobbing {
                run_amplitude: 0.0075,
                run_speed: 10.0,
                walk_amplitude: 0.005,
                walk_speed: 7.0,
                limp_amplitude: 0.011,
                limp_speed: 6.0,
            },
        };
        (world, config)
    }

    /// The controls driving a player on [`floor`], frame by frame.
    struct Walk {
        c: Controls,
        p: crate::core::movement::Player,
        world: crate::core::collision::CollisionWorld,
        i: Input,
        now: u64,
    }

    impl Walk {
        fn new() -> Self {
            let (world, config) = floor();
            let c = controls();
            let p = crate::core::movement::Player::new(
                config,
                c.settings,
                [0.0; 3],
                crate::core::camera::Angles::default(),
                0,
            );
            Walk {
                c,
                p,
                world,
                i: Input::new(),
                now: 0,
            }
        }

        fn event(&mut self, event: Event) -> &mut Self {
            self.i.handle(event);
            self
        }

        fn stick(&mut self, value: [f32; 2]) -> &mut Self {
            self.event(Event::TouchStick { value })
        }

        /// A tap where `point` shows, lifted 50 ms after going down at the
        /// frame clock (plus `later` ms).
        fn tap_at(&mut self, point: [f32; 3], later: f64) -> &mut Self {
            let at = picture_point(&self.p.view(), point).expect("in view");
            self.tap_picture(at, later)
        }

        /// A tap on the picture at `at`, as [`Walk::tap_at`].
        fn tap_picture(&mut self, at: [f32; 2], later: f64) -> &mut Self {
            let time = self.now as f64 + later;
            self.event(Event::TouchLook(look(9, LookPhase::Down, at, time)));
            self.event(Event::TouchLook(look(9, LookPhase::Up, at, time + 50.0)))
        }

        /// The feet's distance from `point` across the floor.
        fn from(&self, point: [f32; 3]) -> f32 {
            let p = self.p.state().position;
            (p[0] - point[0]).hypot(p[2] - point[2])
        }

        fn tap(&mut self, button: TouchButton) -> &mut Self {
            for pressed in [true, false] {
                self.event(Event::TouchButton { button, pressed });
            }
            self
        }

        fn frames(&mut self, n: usize) -> PlayerState {
            for _ in 0..n {
                self.now += 16;
                let (world, p) = (&self.world, &self.p);
                let view = LevelView {
                    world,
                    view: p.view(),
                };
                let (r, _) = self.c.request(&self.i, &p.state(), self.now, &view);
                self.p.frame(&self.world, self.now, &r);
                self.i.end_frame();
            }
            self.p.state()
        }
    }

    /// Climbing a ladder (docs/behavior/climbing.md) by the touch stick,
    /// look drag and Jump (standard), by a tap beyond it, look drag and
    /// Jump (experimental), and by the gamepad's sticks and south button:
    /// the same actions as the keys and the mouse.
    #[test]
    fn ladders_are_climbed_by_touch_and_gamepad() {
        use crate::core::camera::Angles;
        use crate::core::climbing::{ClimbState, Ladder};
        #[derive(Debug, PartialEq)]
        enum Path {
            Stick,
            Tap,
            Pad,
        }
        for path in [Path::Stick, Path::Tap, Path::Pad] {
            let pad = path == Path::Pad;
            let mut w = Walk::new();
            // A platform the ladder leads up to, behind it.
            let [a, b, c, d] = [
                [-2.0, 4.0, -0.5],
                [2.0, 4.0, -0.5],
                [2.0, 4.0, -5.0],
                [-2.0, 4.0, -5.0],
            ];
            w.world = world_with(&[([a, b, c], 0), ([a, c, d], 0)]);
            let ladder = Ladder::new("l", [0.0, 2.0, 0.0], [0.0; 3], [0.4, 2.0, 0.02]);
            w.world.set_ladders(vec![ladder]).unwrap();
            let facing = Angles {
                yaw: std::f32::consts::PI,
                ..Angles::default()
            };
            w.p.place([0.0, 0.0, 0.6], facing);
            w.frames(10);
            // Forward climbs (2.5).
            match path {
                Path::Pad => {
                    w.event(Event::GamepadAxis {
                        axis: GamepadAxis::LeftY,
                        value: -1.0,
                    });
                }
                Path::Stick => {
                    w.stick([0.0, -1.0]);
                }
                // A tap on the platform: the trip climbs toward it.
                Path::Tap => {
                    w.tap_at([0.0, 4.0, -1.0], 0.0);
                }
            }
            let s = w.frames(40);
            assert!(s.flags.climbing && s.position[1] > 0.5, "{path:?}: {s:?}");
            assert_eq!(w.p.climber().state(), ClimbState::ClimbingUp);
            // Looking down past 20° while still pushing climbs down.
            match path {
                Path::Pad => {
                    w.event(Event::GamepadAxis {
                        axis: GamepadAxis::RightY,
                        value: 1.0,
                    });
                    w.frames(12);
                    w.event(Event::GamepadAxis {
                        axis: GamepadAxis::RightY,
                        value: 0.0,
                    });
                }
                // A drag down: mouse counts, looking down.
                Path::Stick => {
                    w.event(Event::TouchLookDelta {
                        delta: [0.0, 300.0],
                        speed: 100.0,
                    });
                }
                // A finger dragged up: the scene with it, so the view
                // looks down.
                Path::Tap => {
                    for (phase, y, time) in
                        [(LookPhase::Down, 0.5, 0.0), (LookPhase::Move, -0.5, 500.0)]
                    {
                        w.event(Event::TouchLook(look(1, phase, [0.0, y], time)));
                    }
                }
            }
            let high = w.frames(1).position[1];
            let s = w.frames(20);
            assert!(w.p.angles().pitch > 0.35, "{path:?}: {:?}", w.p.angles());
            assert_eq!(w.p.climber().state(), ClimbState::ClimbingDown, "{path:?}");
            assert!(s.position[1] < high, "{path:?}: {s:?}");
            // Jump lets go (3.5).
            for pressed in [true, false] {
                w.event(if pad {
                    Event::GamepadButton {
                        button: GamepadButton::South,
                        pressed,
                    }
                } else {
                    Event::TouchButton {
                        button: TouchButton::Jump,
                        pressed,
                    }
                });
                w.frames(1);
            }
            let s = w.p.state();
            assert!(s.airborne && !s.flags.climbing, "{path:?}: {s:?}");
            assert_eq!(w.p.climber().state(), ClimbState::Leaving, "{path:?}");
        }
    }

    /// One frame of look events at `now`, from a level view: the turn.
    fn looks(d: &mut TouchLookDrag, events: &[TouchLook], now: u64) -> [f32; 2] {
        d.frame(events, false, 0.0, LIMITS, now)
    }

    #[test]
    fn a_tap_is_no_drag_and_a_drag_no_tap() {
        let px = 1.0 / HALF;
        let mut d = TouchLookDrag::default();
        // Down and up in place, quickly: a tap, nothing turned.
        let t = looks(
            &mut d,
            &[
                look(1, LookPhase::Down, [0.2, 0.1], 0.0),
                look(1, LookPhase::Up, [0.2, 0.1], 120.0),
            ],
            16,
        );
        assert_eq!(t, [0.0; 2]);
        assert_eq!(
            d.taps(),
            [Tap {
                at: [0.2, 0.1],
                down: 0.0,
                up: 120.0,
                half_height: HALF
            }]
        );
        // The next frame's taps are its own.
        looks(&mut d, &[], 32);
        assert!(d.taps().is_empty());
        // Wobbling within the slop: still a tap, where it went down, and
        // the view stays.
        let t = looks(
            &mut d,
            &[
                look(2, LookPhase::Down, [0.0, 0.0], 200.0),
                look(2, LookPhase::Move, [6.0 * px, 5.0 * px], 230.0),
                look(2, LookPhase::Up, [-4.0 * px, 7.0 * px], 260.0),
            ],
            48,
        );
        assert_eq!(t, [0.0; 2]);
        assert_eq!(d.taps().len(), 1);
        assert_eq!(d.taps()[0].at, [0.0, 0.0]);
        // Past the slop it drags: the scene from where it went down stays
        // under the finger, and the lift is no tap.
        let to = [12.0 * px, 0.0];
        let t = looks(
            &mut d,
            &[
                look(3, LookPhase::Down, [0.0, 0.0], 300.0),
                look(3, LookPhase::Move, [5.0 * px, 0.0], 310.0),
                look(3, LookPhase::Move, to, 320.0),
            ],
            64,
        );
        let want = grab_turn([0.0, 0.0], to, 0.0, LIMITS, tan_half());
        assert!(
            (t[0] - want[0]).abs() < 1e-6 && t[0] < 0.0,
            "{t:?} {want:?}"
        );
        // Back within the slop: still a drag.
        looks(&mut d, &[look(3, LookPhase::Move, [0.0, 0.0], 330.0)], 80);
        looks(&mut d, &[look(3, LookPhase::Up, [0.0, 0.0], 340.0)], 96);
        assert!(d.taps().is_empty());
        // Held past TAP_MS within the slop: neither (the drag's coast
        // aside).
        let mut d = TouchLookDrag::default();
        let t = looks(
            &mut d,
            &[
                look(4, LookPhase::Down, [0.0, 0.0], 400.0),
                look(4, LookPhase::Up, [0.0, 0.0], 400.0 + TAP_MS + 1.0),
            ],
            112,
        );
        assert!(d.taps().is_empty() && t == [0.0; 2]);
        // A drag from Jump or Crouch looks at once and is never a tap.
        let t = looks(
            &mut d,
            &[
                look(5, LookPhase::Drag, [0.5, 0.0], 1000.0),
                look(5, LookPhase::Move, [0.5 + 2.0 * px, 0.0], 1010.0),
                look(5, LookPhase::Up, [0.5 + 2.0 * px, 0.0], 1020.0),
            ],
            128,
        );
        assert!(t[0] < 0.0 && d.taps().is_empty(), "{t:?}");
    }

    /// A view from 1.5 m over the origin looking along +z, `pitch` down.
    fn view(pitch: f32) -> View {
        let angles = crate::core::camera::Angles {
            pitch,
            ..Default::default()
        };
        let (direction, up) = angles.direction_up();
        View {
            eye: [0.0, 1.5, 0.0],
            direction,
            up,
        }
    }

    #[test]
    fn a_tap_s_destination_is_where_its_ray_meets_the_level() {
        let world = world_with(&[]);
        // Straight down the middle at 45° down: the floor 1.5 m ahead.
        let v = view(std::f32::consts::FRAC_PI_4);
        let hit = tap_destination(&world, &v, [0.0, 0.0]).unwrap();
        for (k, want) in [0.0, 0.0, 1.5].into_iter().enumerate() {
            assert!((hit[k] - want).abs() < 1e-4, "{hit:?}");
        }
        // Any point of the picture: its ray meets the floor where that
        // point shows.
        for at in [[0.5, 0.3], [-0.9, 0.8], [0.2, -0.5]] {
            let hit = tap_destination(&world, &v, at).unwrap();
            assert!(hit[1].abs() < 1e-4, "{at:?}: {hit:?}");
            let back = picture_point(&v, hit).unwrap();
            assert!((back[0] - at[0]).abs() < 1e-4 && (back[1] - at[1]).abs() < 1e-4);
        }
        // Above the horizon: nothing within reach.
        assert_eq!(tap_destination(&world, &view(0.0), [0.0, -0.2]), None);
        // A wall in the way stops the ray, either side; a pass-through one
        // lets it by.
        let wall = |z: f32| [[-5.0, 0.0, z], [5.0, 0.0, z], [0.0, 5.0, z]];
        let world = world_with(&[(wall(1.0), PASS), (wall(1.2), 0)]);
        let hit = tap_destination(&world, &v, [0.0, 0.0]).unwrap();
        assert!(
            (hit[2] - 1.2).abs() < 1e-4 && (hit[1] - 0.3).abs() < 1e-4,
            "{hit:?}"
        );
        // Behind the eye nothing shows.
        assert_eq!(picture_point(&v, [0.0, 1.0, -1.0]), None);
    }

    #[test]
    fn a_tap_runs_there_whatever_the_view_and_stops_on_arrival() {
        let mut w = Walk::new();
        w.frames(10);
        let target = [3.0, 0.0, 4.0];
        w.tap_at(target, 0.0);
        let s = w.frames(2);
        assert!(s.flags.forward && w.c.tap_move.trip().is_some(), "{s:?}");
        assert!(!s.flags.sprint, "a single tap runs");
        assert!(w.c.settings.always_run != s.flags.accel, "running");
        // A drag turns the view away meanwhile; the trip goes on.
        w.event(Event::TouchLook(look(2, LookPhase::Down, [0.0, 0.0], 0.0)));
        w.event(Event::TouchLook(look(2, LookPhase::Move, [0.6, 0.0], 40.0)));
        w.frames(1);
        w.event(Event::TouchLook(look(2, LookPhase::Up, [0.6, 0.0], 400.0)));
        let yaw = w.p.angles().yaw;
        assert!(yaw < -0.3, "turned left: {yaw}");
        let mut frames = 0;
        while w.c.tap_move.trip().is_some() && frames < 300 {
            let p = w.p.state().position;
            w.frames(1);
            frames += 1;
            // Straight there: on the line from the start.
            assert!((p[0] * 4.0 - p[2] * 3.0).abs() < 0.05, "{p:?}");
        }
        assert!(w.c.tap_move.trip().is_none(), "arrived in {frames} frames");
        assert!(matches!(w.c.tap_move.ended(), Some((TripEnd::Arrived, d)) if d <= ARRIVAL));
        assert!(w.from(target) <= ARRIVAL, "{}", w.from(target));
        let s = w.frames(60);
        assert!(!s.flags.forward, "{s:?}");
        let rest = w.from(target);
        assert!(rest < 0.6, "came to rest {rest:.2} m away");
        assert_eq!(w.p.angles().yaw, yaw, "the view stayed where it was turned");
    }

    #[test]
    fn a_new_tap_sets_a_new_destination_and_a_tap_on_nothing_stops() {
        let mut w = Walk::new();
        w.p.place(
            [0.0; 3],
            crate::core::camera::Angles {
                pitch: 0.5,
                ..Default::default()
            },
        );
        w.frames(10);
        w.tap_at([0.0, 0.0, 6.0], 0.0);
        w.frames(30);
        // A second tap well after the first: a new destination aside, no
        // sprint.
        let second = [-3.0, 0.0, 3.0];
        w.tap_at(second, 0.0);
        w.frames(2);
        let trip = *w.c.tap_move.trip().unwrap();
        assert_eq!(trip.destination[0].round(), -3.0);
        assert!(!trip.sprint);
        for _ in 0..300 {
            if w.c.tap_move.trip().is_none() {
                break;
            }
            w.frames(1);
        }
        assert!(w.from(second) <= ARRIVAL, "{}", w.from(second));
        // Under way again, then a tap on the sky: it stops.
        w.tap_at([0.0, 0.0, 6.0], 0.0);
        w.frames(5);
        assert!(w.c.tap_move.trip().is_some());
        w.tap_picture([0.0, -0.95], 0.0);
        w.frames(1);
        assert!(w.c.tap_move.trip().is_none());
        // Under way again, then a key: the keyboard takes over.
        w.tap_at([0.0, 0.0, 6.0], 0.0);
        w.frames(5);
        key(&mut w.i, Key::A, true);
        let s = w.frames(1);
        assert!(w.c.tap_move.trip().is_none() && s.flags.left && !s.flags.forward);
    }

    #[test]
    fn a_trip_stuck_against_a_wall_stops() {
        let mut w = Walk::new();
        let [a, b, c, d] = [
            [-5.0, 0.0, 2.0],
            [5.0, 0.0, 2.0],
            [5.0, 5.0, 2.0],
            [-5.0, 5.0, 2.0],
        ];
        // Facing the player (-z).
        w.world = world_with(&[([a, c, b], 0), ([a, d, c], 0)]);
        w.frames(10);
        // Up the wall, out of reach: the trip runs into the wall and stops
        // half a second after its last progress.
        w.tap_at([0.0, 1.0, 2.0], 0.0).frames(1);
        assert!(w.c.tap_move.trip().is_some());
        let mut frames = 1;
        while w.c.tap_move.trip().is_some() && frames < 300 {
            w.frames(1);
            frames += 1;
        }
        let z = w.p.state().position[2];
        assert!(
            frames < 120 && z > 1.4 && z < 2.0,
            "{frames} frames, at {z}"
        );
        assert!(matches!(w.c.tap_move.ended(), Some((TripEnd::Stuck, d)) if d > ARRIVAL));
        assert!(!w.frames(60).flags.forward);
    }

    #[test]
    fn a_double_tap_sprints_that_trip_until_the_movement_layer_ends_it() {
        let mut w = Walk::new();
        w.frames(10);
        let target = [0.0, 0.0, 9.0];
        // Two taps 100 ms apart: sprinting.
        w.tap_at(target, 0.0).tap_at([0.05, 0.0, 9.0], 150.0);
        let s = w.frames(10);
        assert!(w.c.tap_move.trip().unwrap().sprint, "{s:?}");
        assert!(s.flags.sprint && s.sprint_requested, "{s:?}");
        // A single tap now: on at a run, the sprint ended.
        w.frames(30);
        w.tap_at([0.0, 0.0, 20.0], 0.0);
        let s = w.frames(5);
        assert!(!w.c.tap_move.trip().unwrap().sprint && !s.sprint_requested && s.flags.forward);
        // Double tapped again (aside, clear of the marker), then Crouch:
        // the spec cancels the sprint (4.3), the trip goes on crouched and
        // doesn't ask again.
        w.frames(30);
        w.tap_at([6.0, 0.0, 40.0], 0.0)
            .tap_at([6.0, 0.0, 40.0], 100.0);
        assert!(w.frames(5).flags.sprint);
        w.tap(TouchButton::Crouch);
        let s = w.frames(20);
        assert!(!s.sprint_requested && !s.flags.sprint && s.flags.forward && s.flags.crouch);
        assert!(!w.c.tap_move.trip().unwrap().sprint);
        // Taps far apart, or too late, are two single taps.
        let mut w = Walk::new();
        w.frames(10);
        w.tap_at([-2.0, 0.0, 6.0], 0.0)
            .tap_at([2.0, 0.0, 6.0], 150.0);
        assert!(!w.frames(10).sprint_requested, "apart");
        w.tap_at([0.0, 0.0, 6.0], 0.0)
            .tap_at([0.0, 0.0, 6.0], 50.0 + DOUBLE_TAP_MS + 1.0);
        assert!(!w.frames(10).sprint_requested, "too late");
    }

    #[test]
    fn a_tap_on_the_marker_cancels_the_trip_but_a_double_tap_there_sprints() {
        let mut w = Walk::new();
        w.frames(10);
        let target = [0.0, 0.0, 8.0];
        w.tap_at(target, 0.0);
        w.frames(30);
        assert!(w.c.tap_move.trip().is_some());
        // Near the marker (its ring plus the slop), past the double tap's
        // time: cancelled, the character stops.
        let at = picture_point(&w.p.view(), target).unwrap();
        let off = (MARKER_HIT_PX - 2.0) / HALF;
        w.tap_picture([at[0] + off, at[1]], 0.0);
        let s = w.frames(1);
        assert!(w.c.tap_move.trip().is_none(), "{s:?}");
        assert!(matches!(w.c.tap_move.ended(), Some((TripEnd::Cancelled, d)) if d > ARRIVAL));
        assert!(!w.frames(60).flags.forward, "stopped");
        // Under way again, a tap just beyond the marker's reach: a new
        // destination.
        w.tap_at(target, 0.0);
        w.frames(30);
        let at = picture_point(&w.p.view(), target).unwrap();
        w.tap_picture([at[0], at[1] + (MARKER_HIT_PX + 4.0) / HALF], 0.0);
        w.frames(1);
        assert!(w.c.tap_move.trip().is_some(), "beyond the marker");
        // A double tap elsewhere, its second tap on the new marker: that
        // sprints.
        w.frames(30);
        let far = [3.0, 0.0, 14.0];
        w.tap_at(far, 0.0).tap_at(far, 120.0);
        let s = w.frames(10);
        assert!(
            w.c.tap_move.trip().is_some_and(|t| t.sprint) && s.sprint_requested,
            "{s:?}"
        );
        // Then a single tap on the marker, after the window: cancelled, the
        // sprint with it.
        let at = picture_point(&w.p.view(), w.c.tap_move.trip().unwrap().destination).unwrap();
        w.tap_picture(at, 0.0);
        w.frames(2);
        assert!(w.c.tap_move.trip().is_none());
        assert!(!w.frames(60).flags.forward, "stopped");
    }

    #[test]
    fn a_trip_in_the_low_crouch_creeps_and_one_standing_runs() {
        let mut w = Walk::new();
        w.frames(10);
        w.tap(TouchButton::CrouchLow);
        assert_eq!(w.frames(30).posture, 2, "the low crouch");
        w.tap_at([0.0, 0.0, 5.0], 0.0);
        let s = w.frames(20);
        assert!(s.posture == 2 && s.flags.forward, "creeping: {s:?}");
        // Standing up, a trip runs.
        w.tap(TouchButton::Crouch);
        assert_eq!(w.frames(30).posture, 0);
        w.tap_at([0.0, 0.0, 8.0], 0.0);
        let s = w.frames(5);
        assert!(
            s.flags.forward && w.c.settings.always_run != s.flags.accel,
            "{s:?}"
        );
        // Crouch swiped down meanwhile: the low crouch, still on the way.
        w.tap(TouchButton::CrouchLow);
        let s = w.frames(30);
        assert!(
            s.posture == 2 && s.crouch_toggled && s.flags.forward,
            "{s:?}"
        );
    }

    #[test]
    fn under_a_screen_only_its_unused_keys_reach_the_character() {
        let mut c = controls();
        c.bindings = Bindings::read(&[DEFAULTS, b"bind inventory kI\nbind drop kG\n"]);
        assert_eq!(c.keys(MoveAction::Inventory), [Key::I]);
        c.keys_only = Some(vec![Control::Key(Key::I), Control::Key(Key::W)]);
        let mut i = Input::new();
        key(&mut i, Key::A, true);
        key(&mut i, Key::W, true);
        i.handle(Event::PointerMotion { delta: [5.0, 0.0] });
        i.handle(Event::PointerButton {
            at: None,
            button: MouseButton::Left,
            pressed: true,
            time: 0.0,
        });
        i.handle(Event::GamepadButton {
            button: GamepadButton::South,
            pressed: true,
        });
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: -1.0,
        });
        i.handle(Event::TouchButton {
            button: TouchButton::Menu,
            pressed: true,
        });
        let (r, back) = req(&mut c, &i);
        // A reaches it; W is the screen's; the mouse, the gamepad and touch
        // do not.
        assert!(r.left && !r.forward && !r.fire && !r.jump_pressed && !back);
        assert_eq!(r.mouse, [0.0, 0.0]);
        // Without the screen, all of it does.
        c.keys_only = None;
        let (r, back) = req(&mut c, &i);
        assert!(r.forward && r.fire && r.jump_pressed && back);
    }

    #[test]
    fn the_inventory_is_asked_for_by_key_touch_and_gamepad() {
        let mut c = controls();
        c.bindings = Bindings::read(&[DEFAULTS, b"bind inventory kI\n"]);
        let mut i = Input::new();
        assert!(!c.inventory_pressed(&i));
        key(&mut i, Key::I, true);
        assert!(c.inventory_pressed(&i));
        i.end_frame();
        assert!(!c.inventory_pressed(&i), "held, not pressed");
        i.handle(Event::TouchButton {
            button: TouchButton::Inventory,
            pressed: true,
        });
        assert!(c.inventory_pressed(&i));
        i.end_frame();
        i.handle(Event::GamepadButton {
            button: GamepadButton::North,
            pressed: true,
        });
        assert!(c.inventory_pressed(&i));
    }

    #[test]
    fn the_torch_commands_by_key_and_gamepad() {
        let mut c = controls();
        c.bindings = Bindings::read(&[DEFAULTS, b"bind torch kL\nbind night_vision kN\n"]);
        let mut i = Input::new();
        key(&mut i, Key::L, true);
        assert!(c.torch_pressed(&i) && !c.night_vision_pressed(&i));
        i.end_frame();
        assert!(!c.torch_pressed(&i), "held, not pressed");
        key(&mut i, Key::N, true);
        assert!(c.night_vision_pressed(&i) && !c.torch_pressed(&i));
        i.end_frame();
        for (button, torch) in [
            (GamepadButton::DpadUp, true),
            (GamepadButton::DpadDown, false),
        ] {
            i.handle(Event::GamepadButton {
                button,
                pressed: true,
            });
            assert_eq!(
                (c.torch_pressed(&i), c.night_vision_pressed(&i)),
                (torch, !torch)
            );
            i.end_frame();
        }
        // Under a screen that takes the mouse: its unused keys only.
        c.keys_only = Some(vec![Control::Key(Key::N)]);
        key(&mut i, Key::L, false);
        key(&mut i, Key::L, true);
        key(&mut i, Key::N, false);
        key(&mut i, Key::N, true);
        i.handle(Event::GamepadButton {
            button: GamepadButton::DpadUp,
            pressed: false,
        });
        i.handle(Event::GamepadButton {
            button: GamepadButton::DpadUp,
            pressed: true,
        });
        assert!(c.torch_pressed(&i) && !c.night_vision_pressed(&i));
        i.end_frame();
        i.handle(Event::GamepadButton {
            button: GamepadButton::DpadUp,
            pressed: false,
        });
        i.handle(Event::GamepadButton {
            button: GamepadButton::DpadUp,
            pressed: true,
        });
        assert!(!c.torch_pressed(&i));
    }

    #[test]
    fn the_torch_commands_by_touch() {
        let mut c = controls();
        let mut i = Input::new();
        let tap = |i: &mut Input, button| {
            for pressed in [true, false] {
                i.handle(Event::TouchButton { button, pressed });
            }
        };
        tap(&mut i, TouchButton::Torch);
        assert!(c.torch_pressed(&i) && !c.night_vision_pressed(&i));
        i.end_frame();
        assert!(!c.torch_pressed(&i), "one press, one toggle");
        tap(&mut i, TouchButton::NightVision);
        assert!(c.night_vision_pressed(&i) && !c.torch_pressed(&i));
        i.end_frame();
        // Under a screen that takes the mouse touch is the screen's.
        c.keys_only = Some(Vec::new());
        tap(&mut i, TouchButton::Torch);
        tap(&mut i, TouchButton::NightVision);
        assert!(!c.torch_pressed(&i) && !c.night_vision_pressed(&i));
    }
}
