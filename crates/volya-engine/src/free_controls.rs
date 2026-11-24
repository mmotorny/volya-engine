// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! How every input path drives the level view's free camera. Our design,
//! not the game's (the free camera is a viewing tool; docs/architecture.md
//! asks for the same reach from touch, keyboard and mouse, and gamepads):
//!
//! | | Keyboard and mouse | Gamepad | Touch |
//! |---|---|---|---|
//! | move | W A S D or the arrows | left stick, d-pad | a contact begun on the left half: a virtual stick around where it began |
//! | look | drag with a mouse button held | right stick | a contact begun on the right half: drag |
//! | up / down | Space / C | right / left trigger, right / left shoulder | the up and down buttons at the right edge |
//! | faster | Shift | left stick pressed | the virtual stick pulled past 1.5 radii |
//! | back to the menu | Escape | east (B / circle) | the button at the top left |
//!
//! Touch controls are laid out in fractions of the screen's shorter side, so
//! the input (CSS pixels) and the drawn overlay (output pixels) agree.

use crate::core::camera::FreeMove;
use crate::core::input::{Action, Control, GamepadAxis, GamepadButton, Input, Key};

/// Mouse look, radians per screen unit (CSS pixel) dragged.
pub const MOUSE_LOOK: f32 = 0.004;
/// Touch look, radians per screen unit dragged.
pub const TOUCH_LOOK: f32 = 0.006;
/// Right-stick turn rates at full deflection, radians per second.
pub const PAD_YAW: f32 = 2.5;
pub const PAD_PITCH: f32 = 2.0;
/// Stick deflection ignored around the centre.
pub const DEAD_ZONE: f32 = 0.15;
/// Virtual stick deflection, in radii, beyond which touch moves faster.
pub const TOUCH_FAST: f32 = 1.5;

/// An axis-aligned rectangle: min x, min y, max x, max y.
pub type Rect = [f32; 4];

fn inside(r: Rect, [x, y]: [f32; 2]) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

/// Where the touch controls are on a `width` × `height` screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchLayout {
    pub width: f32,
    pub height: f32,
    pub back: Rect,
    pub up: Rect,
    pub down: Rect,
    /// Virtual stick radius.
    pub stick_radius: f32,
}

/// What a contact does, by where it began.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Back,
    Up,
    Down,
    Stick,
    Look,
}

impl TouchLayout {
    pub fn new(width: f32, height: f32) -> Self {
        let unit = width.min(height);
        let (size, margin) = (0.14 * unit, 0.04 * unit);
        let right = width - margin;
        let middle = height / 2.0;
        Self {
            width,
            height,
            back: [margin, margin, margin + size, margin + size],
            up: [
                right - size,
                middle - size - margin / 2.0,
                right,
                middle - margin / 2.0,
            ],
            down: [
                right - size,
                middle + margin / 2.0,
                right,
                middle + size + margin / 2.0,
            ],
            stick_radius: 0.15 * unit,
        }
    }

    pub fn role(&self, start: [f32; 2]) -> Role {
        if inside(self.back, start) {
            Role::Back
        } else if inside(self.up, start) {
            Role::Up
        } else if inside(self.down, start) {
            Role::Down
        } else if start[0] < self.width / 2.0 {
            Role::Stick
        } else {
            Role::Look
        }
    }
}

fn dead_zone(v: f32) -> f32 {
    if v.abs() < DEAD_ZONE { 0.0 } else { v }
}

fn axis(negative: bool, positive: bool) -> f32 {
    f32::from(u8::from(positive)) - f32::from(u8::from(negative))
}

/// This frame's free-camera request from `input` over `dt` seconds, and
/// whether the player asked to go back to the menu.
pub fn free_move(input: &Input, dt: f32) -> (FreeMove, bool) {
    let key = |k| input.key_held(k);
    let pad = |b| input.is_held(Control::Pad(b));
    let value = |a| input.axis(a);
    let mut m = FreeMove {
        forward: axis(key(Key::S) || key(Key::Down), key(Key::W) || key(Key::Up))
            - dead_zone(value(GamepadAxis::LeftY)),
        right: axis(
            key(Key::A) || key(Key::Left),
            key(Key::D) || key(Key::Right),
        ) + dead_zone(value(GamepadAxis::LeftX)),
        up: axis(key(Key::C), key(Key::Space))
            + axis(
                pad(GamepadButton::LeftShoulder),
                pad(GamepadButton::RightShoulder),
            )
            + value(GamepadAxis::RightTrigger)
            - value(GamepadAxis::LeftTrigger),
        fast: key(Key::Shift) || pad(GamepadButton::LeftStick),
        yaw: dead_zone(value(GamepadAxis::RightX)) * PAD_YAW * dt,
        pitch: dead_zone(value(GamepadAxis::RightY)) * PAD_PITCH * dt,
    };
    let [dx, dy] = input.drag_delta();
    m.yaw += dx * MOUSE_LOOK;
    m.pitch += dy * MOUSE_LOOK;
    let mut back = input.actions().iter().any(|a| {
        matches!(
            a,
            Action::Press {
                control: Control::Key(Key::Escape),
                ..
            }
        )
    });
    let [w, h] = input.screen();
    let layout = TouchLayout::new(w, h);
    for c in input.contacts() {
        match layout.role(c.start) {
            Role::Back => back |= c.new,
            Role::Up => m.up += 1.0,
            Role::Down => m.up -= 1.0,
            Role::Stick => {
                let r = layout.stick_radius;
                let (x, y) = ((c.at[0] - c.start[0]) / r, (c.at[1] - c.start[1]) / r);
                let len = x.hypot(y);
                let scale = if len > 1.0 { 1.0 / len } else { 1.0 };
                m.right += x * scale;
                m.forward -= y * scale;
                m.fast |= len > TOUCH_FAST;
            }
            Role::Look => {
                let [dx, dy] = c.delta();
                m.yaw += dx * TOUCH_LOOK;
                m.pitch += dy * TOUCH_LOOK;
            }
        }
    }
    m.forward = m.forward.clamp(-1.0, 1.0);
    m.right = m.right.clamp(-1.0, 1.0);
    m.up = m.up.clamp(-1.0, 1.0);
    (m, back)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::input::{Event, MouseButton, TouchPhase};

    fn input() -> Input {
        let mut input = Input::new();
        input.set_screen([1000.0, 500.0]);
        input
    }

    fn key(input: &mut Input, key: Key, pressed: bool) {
        input.handle(Event::Key { key, pressed });
    }

    #[test]
    fn keyboard_and_mouse() {
        let mut i = input();
        key(&mut i, Key::W, true);
        key(&mut i, Key::A, true);
        key(&mut i, Key::Space, true);
        key(&mut i, Key::Shift, true);
        let (m, back) = free_move(&i, 0.1);
        assert_eq!(
            (m.forward, m.right, m.up, m.fast, back),
            (1.0, -1.0, 1.0, true, false)
        );
        // Moving the mouse without a button does not look; dragging does.
        i.handle(Event::PointerMoved { at: [100.0, 100.0] });
        i.handle(Event::PointerMoved { at: [110.0, 95.0] });
        assert_eq!(free_move(&i, 0.1).0.yaw, 0.0);
        i.handle(Event::PointerButton {
            at: Some([110.0, 95.0]),
            button: MouseButton::Left,
            pressed: true,
            time: 0.0,
        });
        i.handle(Event::PointerMoved { at: [120.0, 90.0] });
        let (m, _) = free_move(&i, 0.1);
        assert_eq!((m.yaw, m.pitch), (10.0 * MOUSE_LOOK, -5.0 * MOUSE_LOOK));
        key(&mut i, Key::Escape, true);
        assert!(free_move(&i, 0.1).1);
    }

    #[test]
    fn gamepad() {
        let mut i = input();
        for (axis, value) in [
            (GamepadAxis::LeftY, -0.6),
            (GamepadAxis::LeftX, 0.1),
            (GamepadAxis::RightX, 1.0),
            (GamepadAxis::RightTrigger, 0.5),
        ] {
            i.handle(Event::GamepadAxis { axis, value });
        }
        i.handle(Event::GamepadButton {
            button: GamepadButton::LeftStick,
            pressed: true,
        });
        let (m, _) = free_move(&i, 0.5);
        // The stick's arrow keys do not add to its analog value.
        assert_eq!((m.forward, m.right, m.up, m.fast), (0.6, 0.0, 0.5, true));
        assert_eq!(m.yaw, PAD_YAW * 0.5);
        i.handle(Event::GamepadButton {
            button: GamepadButton::East,
            pressed: true,
        });
        assert!(free_move(&i, 0.5).1, "east acts as Escape");
    }

    #[test]
    fn touch() {
        let mut i = input();
        let layout = TouchLayout::new(1000.0, 500.0);
        let contact = |id, at, phase| Event::Contact { id, at, phase };
        // Left half: a stick pulled up by one radius moves forward.
        i.handle(contact(1, [200.0, 300.0], TouchPhase::Start));
        i.handle(contact(
            1,
            [200.0, 300.0 - layout.stick_radius],
            TouchPhase::Move,
        ));
        // Right half: a drag looks.
        i.handle(contact(2, [700.0, 300.0], TouchPhase::Start));
        i.end_frame();
        i.handle(contact(2, [720.0, 290.0], TouchPhase::Move));
        let (m, back) = free_move(&i, 0.1);
        assert!((m.forward - 1.0).abs() < 1e-6 && m.right.abs() < 1e-6);
        assert!(!m.fast && !back);
        assert_eq!((m.yaw, m.pitch), (20.0 * TOUCH_LOOK, -10.0 * TOUCH_LOOK));
        // Pulled further: faster, still at most 1.
        i.handle(contact(
            1,
            [200.0, 300.0 - 2.0 * layout.stick_radius],
            TouchPhase::Move,
        ));
        let (m, _) = free_move(&i, 0.1);
        assert!(m.fast && (m.forward - 1.0).abs() < 1e-6);
        // The up button, then the back button.
        let centre = |r: Rect| [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
        assert_eq!(layout.role(centre(layout.up)), Role::Up);
        assert_eq!(layout.role(centre(layout.down)), Role::Down);
        i.handle(contact(3, centre(layout.up), TouchPhase::Start));
        assert_eq!(free_move(&i, 0.1).0.up, 1.0);
        i.handle(contact(4, centre(layout.back), TouchPhase::Start));
        assert!(free_move(&i, 0.1).1);
    }
}
