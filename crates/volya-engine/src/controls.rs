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
//! | Action | Gamepad | Touch (the page's on-screen controls) |
//! |---|---|---|
//! | move | left stick | the move stick (left half) |
//! | walk instead of run | the stick pushed less than [`RUN_DEFLECTION`], kept after it is released | the same |
//! | look | right stick, at `rot_speed` at full deflection (8.2) | a drag on the right half, or one begun on Jump or Crouch, as mouse counts (8.1) through [`look_gain`] |
//! | jump | south (A / cross) | Jump |
//! | crouch | west (X / square), as `crouch_toggle` | Crouch, as `crouch_toggle` |
//! | sprint | left stick pressed, as `sprint_toggle` | the stick pushed forward past its rim, kept by the sprint lock ([`SprintLock`]) |
//! | back to the menu | east (B / circle), as Escape | Menu |
//!
//! Sticks give the spec's digital directions: eight sectors of 45°
//! ([`stick_directions`]). Crouch is a toggle on both, since holding it
//! while moving and looking would need a third thumb. Each stick's walk or
//! run choice is latched (our design): a push whose peak deflection stays
//! below [`RUN_DEFLECTION`] selects walking, one that reaches it running, and
//! a released stick keeps the last choice, so the low crouch (a crouch while
//! walking, 4.2) can be entered and held without touching the stick. A stick
//! never pushed latches nothing, and any keyboard movement key clears both
//! latches, so the keyboard's `accel` works as bound.
//!
//! Touch sprints as mobile shooters do (our design): the stick pushed
//! forward past its rim requests the sprint, and let go on the lock target
//! above it keeps running forward and sprinting hands-free ([`SprintLock`]).
//! The movement layer's rules still decide: whenever they cancel the
//! request (4.3: stopping, crouching, walking, firing), the lock ends.

use std::collections::HashMap;

use crate::core::input::{
    Action, Control, GamepadAxis, GamepadButton, Input, Key, MouseButton, TouchButton,
};
use crate::core::movement::{PlayerState, Request, Settings};
use volya_engine_formats::cp1251;

/// The actions of section 2.2 this engine acts on.
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
}

impl MoveAction {
    pub const ALL: [MoveAction; 14] = [
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
        }
    }
}

/// The control a key name of `bind` lines names (options-screens.md
/// section 11.2, case-insensitive), `None` for keys this engine does not
/// receive.
pub fn control_named(name: &str) -> Option<Control> {
    match name.to_ascii_lowercase().as_str() {
        "mouse1" => Some(Control::Mouse(MouseButton::Left)),
        "mouse2" => Some(Control::Mouse(MouseButton::Right)),
        "mouse3" => Some(Control::Mouse(MouseButton::Middle)),
        _ => Key::from_dik_name(name).map(Control::Key),
    }
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
/// forward (our choice: clearly beyond the full deflection, so a full push
/// alone runs).
pub const SPRINT_RIM: f32 = 1.3;
/// Back below this reach, it is within the rim again (our choice: a little
/// hysteresis, so a thumb resting on the rim doesn't flicker).
pub const SPRINT_RIM_EXIT: f32 = 1.15;
/// Past the rim counts within 45° of straight forward (our choice): the
/// cosine of that angle.
pub const SPRINT_CONE: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// How far straight ahead of the stick's centre the sprint lock target is
/// centred, in stick radii (our choice: above the rim, within a thumb's
/// reach). Where that would leave the screen or its safe area, the page
/// draws the target lower ([`Input::touch_lock_reach`]), down to
/// [`LOCK_MIN_REACH`].
pub const LOCK_REACH: f32 = 2.1;
/// The lock target's radius, in stick radii (our choice: 31 CSS pixels at
/// the page's 56-pixel stick radius, a 62-pixel target).
pub const LOCK_RADIUS: f32 = 0.55;
/// The lowest the lock target goes: all of it past the rim.
pub const LOCK_MIN_REACH: f32 = SPRINT_RIM + LOCK_RADIUS;

/// The lock target's reach for the page's `reach` (the default when the
/// page gave none or nonsense), kept between [`LOCK_MIN_REACH`] and
/// [`LOCK_REACH`].
pub fn lock_reach(reach: Option<f32>) -> f32 {
    match reach {
        Some(r) if r.is_finite() => r.clamp(LOCK_MIN_REACH, LOCK_REACH),
        _ => LOCK_REACH,
    }
}

/// The sprint lock of the touch stick (our design, after mobile shooters):
///
/// - pushed forward past the rim ([`SPRINT_RIM`], within [`SPRINT_CONE`]),
///   the stick requests the sprint;
/// - back within the rim ([`SPRINT_RIM_EXIT`]) or turned aside while held,
///   it ends the sprint it requested;
/// - let go on the lock target ([`LOCK_REACH`] ahead, [`LOCK_RADIUS`]) while
///   sprinting, it locks: the stick reads full forward with nobody on it;
/// - let go elsewhere, it ends the sprint it requested;
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

    /// One frame: the stick's `values` this frame in order
    /// ([`Input::touch_stick_values`]), whether everything was `released`
    /// since ([`Input::sticks_released`]), and whether the sprint is
    /// `requested` after the previous frame, with the lock target `reach`
    /// stick radii ahead ([`lock_reach`]). Returns whether to press
    /// `sprint_toggle`.
    pub fn frame(
        &mut self,
        values: &[[f32; 2]],
        released: bool,
        requested: bool,
        reach: f32,
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
        for &v in values {
            let len = v[0].hypot(v[1]);
            let ahead = -v[1] >= SPRINT_CONE * len;
            let let_go = v == [0.0; 2] && self.last != [0.0; 2];
            match self.phase {
                // Touched again.
                LockPhase::Locked => end(&mut self.phase),
                LockPhase::Past { sprinting } if let_go => {
                    let d = [self.last[0], self.last[1] + reach];
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

/// Below this speed of a touch look drag (CSS pixels per second) one pixel
/// turns as one mouse count (our choice: slow aiming stays as precise as
/// before).
pub const LOOK_PRECISE_SPEED: f32 = 300.0;
/// From this speed on a drag turns [`LOOK_MAX_GAIN`] times as far (our
/// choice: a quick flick).
pub const LOOK_FAST_SPEED: f32 = 1500.0;
/// The most a fast drag is multiplied by (our choice: modest, so turning
/// stays predictable).
pub const LOOK_MAX_GAIN: f32 = 1.6;

/// The touch look's acceleration (our design, as mobile shooters do): a
/// drag at `speed` CSS pixels per second gives this many mouse counts per
/// pixel: 1 up to [`LOOK_PRECISE_SPEED`], rising linearly to
/// [`LOOK_MAX_GAIN`] at [`LOOK_FAST_SPEED`], level beyond. The counts then
/// turn by `mouse_sens` and `mouse_invert` as any mouse's do (8.1).
pub fn look_gain(speed: f32) -> f32 {
    let t = (speed - LOOK_PRECISE_SPEED) / (LOOK_FAST_SPEED - LOOK_PRECISE_SPEED);
    if t.is_nan() {
        return 1.0;
    }
    1.0 + t.clamp(0.0, 1.0) * (LOOK_MAX_GAIN - 1.0)
}

/// The player's controls.
#[derive(Clone, Debug)]
pub struct Controls {
    pub bindings: Bindings,
    pub settings: Settings,
    /// The latched walk (`true`) or run choice of the gamepad's left stick
    /// and of the touch stick, `None` until pushed (see the module docs).
    pub walk: [Option<bool>; 2],
    /// Each stick's peak deflection in the current push, 0 when released.
    peak: [f32; 2],
    pub sprint_lock: SprintLock,
}

impl Controls {
    pub fn new(bindings: Bindings, settings: Settings) -> Self {
        Self {
            bindings,
            settings,
            walk: [None; 2],
            peak: [0.0; 2],
            sprint_lock: SprintLock::default(),
        }
    }
}

impl Controls {
    /// This frame's request from `input`, and whether the player asked to
    /// go back (Escape, a gamepad's east button, the touch Menu button).
    /// The in-game menu (a separate spec) is not made yet; going back
    /// returns to the title menu. `dt` is the frame's time in seconds (the
    /// touch look's speed), `player` the player after the previous frame
    /// (its sprint request for the sprint lock, its crouch toggle for the
    /// touch low crouch).
    pub fn request(&mut self, input: &Input, dt: f32, player: &PlayerState) -> (Request, bool) {
        let held = |a| self.bindings.controls(a).any(|c| input.device_held(c));
        let changed = |a, pressed: bool| {
            input
                .device_changes()
                .iter()
                .any(|&(c, p)| p == pressed && self.bindings.controls(a).any(|b| b == c))
        };
        let pad = |b: GamepadButton, pressed: bool| input.pad_changes().contains(&(b, pressed));
        let touch = |b: TouchButton, pressed: bool| {
            input.actions().iter().any(|a| match a {
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
            mouse: input.pointer_motion(),
            turn: [
                f32::from(u8::from(held(MoveAction::Right)))
                    - f32::from(u8::from(held(MoveAction::Left))),
                f32::from(u8::from(held(MoveAction::Down)))
                    - f32::from(u8::from(held(MoveAction::Up))),
            ],
        };
        // The touch stick: its reach within the rim, or full forward while
        // the sprint lock holds it.
        r.sprint_toggle ^= self.sprint_lock.frame(
            input.touch_stick_values(),
            input.sticks_released()[1],
            player.sprint_requested,
            lock_reach(input.touch_lock_reach()),
        );
        // Crouch swiped down (our design): the low crouch, the crouch
        // toggle engaged with the touch stick's latch walking (4.2), as a
        // light push let go would leave it.
        if touch(TouchButton::CrouchLow, true) {
            if !(player.crouch_toggled ^ r.crouch_toggle) {
                r.crouch_toggle ^= true;
            }
            self.walk[1] = Some(true);
            self.peak[1] = 0.0;
        }
        let [x, y] = input.touch_stick();
        let reach = x.hypot(y).max(1.0);
        let touch_stick = if self.sprint_lock.locked() {
            [0.0, -1.0]
        } else {
            [x / reach, y / reach]
        };
        let pads = [
            [
                input.axis(GamepadAxis::LeftX),
                input.axis(GamepadAxis::LeftY),
            ],
            touch_stick,
        ];
        // Sticks and the touch stick: directions, and walking below the
        // run deflection (`accel` inverts `g_always_run`, 4.1).
        if r.forward || r.back || r.left || r.right {
            // Keyboard movement: the sticks' latches give way.

            self.walk = [None; 2];
        }
        let released = input.sticks_released();
        for (i, stick) in pads.into_iter().enumerate() {
            if released[i] {
                // Released by a blur or lost pointer lock, not let go: the
                // latch and the push's peak stay for the value sent again.
            } else if let Some(([f, b, l, rt], _)) = stick_directions(stick) {
                r.forward |= f;
                r.back |= b;
                r.left |= l;
                r.right |= rt;
                self.peak[i] = self.peak[i].max(stick[0].hypot(stick[1]));
                self.walk[i] = Some(self.peak[i] < RUN_DEFLECTION);
            } else {
                self.peak[i] = 0.0;
            }
            if let Some(walk) = self.walk[i] {
                r.accel |= self.settings.always_run == walk;
            }
        }
        let dead = |v: f32| if v.abs() < DEAD_ZONE { 0.0 } else { v };
        r.turn[0] = (r.turn[0] + dead(input.axis(GamepadAxis::RightX))).clamp(-1.0, 1.0);
        r.turn[1] = (r.turn[1] + dead(input.axis(GamepadAxis::RightY))).clamp(-1.0, 1.0);
        let look = input.touch_look();
        let gain = if dt > 0.0 {
            look_gain(look[0].hypot(look[1]) / dt)
        } else {
            1.0
        };
        r.mouse[0] += look[0] * gain;
        r.mouse[1] += look[1] * gain;
        let back = input.actions().iter().any(|a| {
            matches!(
                a,
                Action::Press {
                    control: Control::Key(Key::Escape),
                    ..
                }
            )
        }) || touch(TouchButton::Menu, true);
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

    /// A 60 Hz frame.
    const DT: f32 = 1.0 / 60.0;

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
        }
    }

    /// One frame's request for a standing player.
    fn req(c: &mut Controls, i: &Input) -> (Request, bool) {
        c.request(i, DT, &standing())
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
        assert!(r.jump_released && !r.jump_pressed && !r.sprint_toggle && back);
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
        i.handle(Event::TouchStick { value: [0.0, -0.9] });
        // A slow drag (190 px/s): one count per pixel.
        i.handle(Event::TouchLook { delta: [3.0, 1.0] });
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
        assert_eq!(r.mouse, [3.0, 1.0]);
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
    fn the_look_curve_is_one_to_one_when_slow_and_modest_when_fast() {
        assert_eq!(look_gain(0.0), 1.0);
        assert_eq!(look_gain(LOOK_PRECISE_SPEED), 1.0);
        assert!((look_gain(900.0) - 1.3).abs() < 1e-6);
        assert_eq!(look_gain(LOOK_FAST_SPEED), LOOK_MAX_GAIN);
        assert_eq!(look_gain(10_000.0), LOOK_MAX_GAIN);
        assert_eq!(look_gain(f32::NAN), 1.0);
        let mut last = 0.0;
        for speed in (0..3000).step_by(50) {
            let g = look_gain(speed as f32);
            assert!(
                g >= last && (1.0..=LOOK_MAX_GAIN).contains(&g),
                "{speed}: {g}"
            );
            last = g;
        }
        // Through the controls: a 40-pixel flick within one frame (2400
        // px/s) turns 1.6 times as far; the same pixels over a second, one
        // count each. The counts are mouse counts: the movement layer
        // scales them by `mouse_sens` and `mouse_invert` (8.1).
        let mut c = controls();
        let mut i = Input::new();
        i.handle(Event::TouchLook { delta: [0.0, 40.0] });
        assert_eq!(req(&mut c, &i).0.mouse, [0.0, 64.0]);
        assert_eq!(c.request(&i, 1.0, &standing()).0.mouse, [0.0, 40.0]);
        assert_eq!(c.request(&i, 0.0, &standing()).0.mouse, [0.0, 40.0]);
    }

    /// One frame of the lock with the stick's `values`.
    fn lock(l: &mut SprintLock, values: &[[f32; 2]], requested: bool) -> bool {
        l.frame(values, false, requested, LOCK_REACH)
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
        assert!(l.frame(&[], true, true, LOCK_REACH) && !l.locked());
    }

    #[test]
    fn the_lock_target_lowered_to_fit_the_screen_is_where_it_locks() {
        // The page's reach, kept from below the rim to the default.
        assert_eq!(lock_reach(None), LOCK_REACH);
        assert_eq!(lock_reach(Some(f32::NAN)), LOCK_REACH);
        assert_eq!(lock_reach(Some(9.0)), LOCK_REACH);
        assert_eq!(lock_reach(Some(0.5)), LOCK_MIN_REACH);
        assert_eq!(lock_reach(Some(1.9)), 1.9);
        // A thumb landing high: the target drawn at the lowest reach. A
        // let-go there locks; at the default target's place it doesn't.
        let mut l = SprintLock::default();
        let low = LOCK_MIN_REACH;
        assert!(l.frame(&[[0.0, -1.5]], false, false, low));
        assert!(!l.frame(&[[0.0, -low], [0.0, 0.0]], false, true, low));
        assert!(l.locked());
        l.frame(&[[0.0, 0.0]], false, true, low);
        l.frame(&[[0.0, -1.5]], false, false, low);
        l.frame(&[[0.0, -2.6], [0.0, 0.0]], false, true, low);
        assert!(!l.locked());
        // Through the controls: the page's message moves the target.
        let mut w = Walk::new();
        w.event(Event::TouchLockReach { reach: low });
        w.stick([0.0, 0.0]).stick([0.0, -1.5]).frames(2);
        w.stick([0.0, -low]).stick([0.0, 0.0]);
        let s = w.frames(10);
        assert!(w.c.sprint_lock.locked() && s.flags.sprint);
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
        let (r, _) = c.request(&i, DT, &toggled);
        assert!(!r.crouch_toggle && r.accel);
    }

    /// A floor of the default material, facing up, and the shipped
    /// movement configuration's values (player-movement.md section 1).
    fn floor() -> (
        crate::core::collision::CollisionWorld,
        crate::core::movement::MovementConfig,
    ) {
        use crate::core::collision::{Cform, CformTriangle, CollisionWorld};
        use crate::core::movement::{Bobbing, MovementConfig};
        use volya_engine_formats::gamemtl::GameMaterial;

        let v = [
            [-50.0, 0.0, -50.0],
            [-50.0, 0.0, 50.0],
            [50.0, 0.0, 50.0],
            [50.0, 0.0, -50.0],
        ];
        let triangles = [[0, 1, 2], [0, 2, 3]].map(|vertices| CformTriangle {
            vertices,
            attributes: 0,
        });
        let cform = Cform {
            bbox_min: [-50.0, 0.0, -50.0],
            bbox_max: [50.0, 0.0, 50.0],
            vertices: v.to_vec(),
            triangles: triangles.to_vec(),
        };
        let world = CollisionWorld::new(
            cform,
            vec![GameMaterial {
                id: 0,
                ..GameMaterial::default()
            }],
        )
        .unwrap();
        let config = MovementConfig {
            walk_accel: 17.0,
            run_coef: 2.1,
            run_back_coef: 1.8,
            walk_back_coef: 0.8,
            crouch_coef: 0.45,
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

        fn tap(&mut self, button: TouchButton) -> &mut Self {
            for pressed in [true, false] {
                self.event(Event::TouchButton { button, pressed });
            }
            self
        }

        fn frames(&mut self, n: usize) -> PlayerState {
            for _ in 0..n {
                self.now += 16;
                let (r, _) = self.c.request(&self.i, DT, &self.p.state());
                self.p.frame(&self.world, self.now, &r);
                self.i.end_frame();
            }
            self.p.state()
        }
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
}
