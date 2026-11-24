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
//! | look | right stick, at `rot_speed` at full deflection (8.2) | a drag on the right half, as mouse counts (8.1) |
//! | jump | south (A / cross) | Jump |
//! | crouch | west (X / square), as `crouch_toggle` | Crouch, as `crouch_toggle` |
//! | sprint | left stick pressed, as `sprint_toggle` | Sprint, as `sprint_toggle` |
//! | back to the menu | east (B / circle), as Escape | Menu |
//!
//! Sticks give the spec's digital directions: eight sectors of 45°
//! ([`stick_directions`]). Crouch is a toggle on both, since holding it
//! while moving and looking would need a third thumb. Each stick's walk or
//! run choice is latched: a light push selects walking, a full push running,
//! and a released stick keeps the last choice, so the low crouch (a crouch
//! while walking, 4.2) can be entered and held without touching the stick.

use std::collections::HashMap;

use crate::core::input::{
    Action, Control, GamepadAxis, GamepadButton, Input, Key, MouseButton, TouchButton,
};
use crate::core::movement::{Request, Settings};
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

/// Key names of section 2.1 this engine receives.
const KEY_NAMES: [(&str, Control); 15] = [
    ("kW", Control::Key(Key::W)),
    ("kS", Control::Key(Key::S)),
    ("kA", Control::Key(Key::A)),
    ("kD", Control::Key(Key::D)),
    ("kX", Control::Key(Key::X)),
    ("kQ", Control::Key(Key::Q)),
    ("kSPACE", Control::Key(Key::Space)),
    ("kLSHIFT", Control::Key(Key::LeftShift)),
    ("kLCONTROL", Control::Key(Key::LeftControl)),
    ("kUP", Control::Key(Key::Up)),
    ("kDOWN", Control::Key(Key::Down)),
    ("kLEFT", Control::Key(Key::Left)),
    ("kRIGHT", Control::Key(Key::Right)),
    ("mouse1", Control::Mouse(MouseButton::Left)),
    ("mouse2", Control::Mouse(MouseButton::Right)),
];

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
    /// engine does not receive leaves the slot empty. Whether binding a key
    /// to one action unbinds it from another is not in the spec; it does
    /// not here (our choice).
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
                let control = KEY_NAMES.iter().find(|(n, _)| n == key).map(|(_, c)| *c);
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

/// The player's controls.
#[derive(Clone, Debug)]
pub struct Controls {
    pub bindings: Bindings,
    pub settings: Settings,
    /// The latched walk choice of the gamepad's left stick and of the touch
    /// stick (see the module docs).
    pub walk: [bool; 2],
}

impl Controls {
    pub fn new(bindings: Bindings, settings: Settings) -> Self {
        Self {
            bindings,
            settings,
            walk: [false; 2],
        }
    }
}

impl Controls {
    /// This frame's request from `input`, and whether the player asked to
    /// go back (Escape, a gamepad's east button, the touch Menu button).
    /// The in-game menu (a separate spec) is not made yet; going back
    /// returns to the title menu.
    pub fn request(&mut self, input: &Input) -> (Request, bool) {
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
                Action::Cursor { .. } => false,
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
                || pad(GamepadButton::LeftStick, true)
                || touch(TouchButton::Sprint, true),
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
        // Sticks and the touch stick: directions, and walking below the
        // run deflection (`accel` inverts `g_always_run`, 4.1).
        let pads = [
            [
                input.axis(GamepadAxis::LeftX),
                input.axis(GamepadAxis::LeftY),
            ],
            input.touch_stick(),
        ];
        for (stick, walk) in pads.into_iter().zip(&mut self.walk) {
            if let Some(([f, b, l, rt], run)) = stick_directions(stick) {
                r.forward |= f;
                r.back |= b;
                r.left |= l;
                r.right |= rt;
                *walk = !run;
            }
            r.accel |= self.settings.always_run == *walk;
        }
        let dead = |v: f32| if v.abs() < DEAD_ZONE { 0.0 } else { v };
        r.turn[0] = (r.turn[0] + dead(input.axis(GamepadAxis::RightX))).clamp(-1.0, 1.0);
        r.turn[1] = (r.turn[1] + dead(input.axis(GamepadAxis::RightY))).clamp(-1.0, 1.0);
        let look = input.touch_look();
        r.mouse[0] += look[0];
        r.mouse[1] += look[1];
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
        let (r, back) = c.request(&i);
        assert!(r.forward && r.accel && r.jump_pressed && r.sprint_toggle && !back);
        assert!(!r.back && !r.left && !r.crouch);
        assert_eq!(r.turn, [-1.0, 0.0]);
        assert_eq!(r.mouse, [3.0, -2.0]);
        i.end_frame();
        key(&mut i, Key::Space, false);
        key(&mut i, Key::Escape, true);
        let (r, back) = c.request(&i);
        assert!(r.jump_released && !r.jump_pressed && !r.sprint_toggle && back);
        // A gamepad's d-pad acts as the arrow keys in the menu, not as the
        // look keys here.
        let mut i = Input::new();
        i.handle(Event::GamepadButton {
            button: GamepadButton::DpadUp,
            pressed: true,
        });
        assert_eq!(c.request(&i).0.turn, [0.0, 0.0]);
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
        let (r, _) = c.request(&i);
        // Walking with g_always_run on is `accel` held.
        assert!(r.forward && r.accel && r.crouch_toggle && r.jump_pressed);
        assert_eq!(r.turn, [0.5, 0.0]);
        // Released, the stick keeps walking: crouching now is the low
        // crouch, held without the stick.
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: 0.0,
        });
        let (r, _) = c.request(&i);
        assert!(!r.forward && r.accel, "the walk choice is kept");
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: -1.0,
        });
        assert!(!c.request(&i).0.accel, "a full push runs");
        i.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: 0.0,
        });
        assert!(!c.request(&i).0.accel, "and stays running");
        i.handle(Event::GamepadButton {
            button: GamepadButton::East,
            pressed: true,
        });
        assert!(c.request(&i).1, "east goes back");
    }

    #[test]
    fn touch_controls() {
        let mut c = controls();
        let mut i = Input::new();
        i.handle(Event::TouchStick { value: [0.0, -0.9] });
        i.handle(Event::TouchLook { delta: [12.0, 4.0] });
        for button in [TouchButton::Jump, TouchButton::Sprint, TouchButton::Crouch] {
            i.handle(Event::TouchButton {
                button,
                pressed: true,
            });
        }
        let (r, back) = c.request(&i);
        assert!(r.forward && !r.accel && r.jump_pressed && r.sprint_toggle && r.crouch_toggle);
        assert!(!r.crouch, "the touch crouch is the toggle");
        assert_eq!(r.mouse, [12.0, 4.0]);
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
        let (r, back) = c.request(&i);
        assert!(r.jump_released && back && r.forward);
        // A light push, then let go: the touch stick keeps walking too.
        i.handle(Event::TouchStick { value: [0.0, -0.3] });
        assert!(c.request(&i).0.accel);
        i.handle(Event::TouchStick { value: [0.0, 0.0] });
        let (r, _) = c.request(&i);
        assert!(r.accel && !r.forward);
    }

    #[test]
    fn a_latched_walk_holds_the_low_crouch_without_the_stick() {
        use crate::core::camera::Angles;
        use crate::core::collision::{Cform, CformTriangle, CollisionWorld};
        use crate::core::movement::{Bobbing, MovementConfig, Player};
        use volya_engine_formats::gamemtl::GameMaterial;

        // A floor of the default material, facing up.
        let v = [
            [-10.0, 0.0, -10.0],
            [-10.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            [10.0, 0.0, -10.0],
        ];
        let triangles = [[0, 1, 2], [0, 2, 3]].map(|vertices| CformTriangle {
            vertices,
            attributes: 0,
        });
        let cform = Cform {
            bbox_min: [-10.0, 0.0, -10.0],
            bbox_max: [10.0, 0.0, 10.0],
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
        let mut c = controls();
        let mut p = Player::new(config, c.settings, [0.0; 3], Angles::default(), 0);
        let mut now = 0;
        let mut frame = |c: &mut Controls, p: &mut Player, i: &mut Input| {
            now += 16;
            let (r, _) = c.request(i);
            p.frame(&world, now, &r);
            i.end_frame();
        };
        let mut i = Input::new();
        // A light push on the touch stick, let go; then Crouch.
        i.handle(Event::TouchStick { value: [0.0, -0.3] });
        frame(&mut c, &mut p, &mut i);
        i.handle(Event::TouchStick { value: [0.0, 0.0] });
        frame(&mut c, &mut p, &mut i);
        i.handle(Event::TouchButton {
            button: TouchButton::Crouch,
            pressed: true,
        });
        frame(&mut c, &mut p, &mut i);
        i.handle(Event::TouchButton {
            button: TouchButton::Crouch,
            pressed: false,
        });
        for _ in 0..30 {
            frame(&mut c, &mut p, &mut i);
        }
        assert_eq!(p.state().posture, 2, "the low crouch, held");
        // A full push runs: the crouch rises to box 1.
        i.handle(Event::TouchStick { value: [0.0, -1.0] });
        frame(&mut c, &mut p, &mut i);
        assert_eq!(p.state().posture, 1);
    }
}
