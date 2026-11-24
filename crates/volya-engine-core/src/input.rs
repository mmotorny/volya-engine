// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Platform-free input for menus: platforms feed [`Event`]s, menu code reads
//! the per-frame [`Action`]s and held state of [`Input`].
//!
//! The menu specification (docs/behavior/title-menu.md §4.4, §7, §8) speaks
//! of mouse and keyboard only, so every source is reduced to that
//! vocabulary: a cursor in normalised coordinates, mouse buttons and a small
//! set of [`Key`]s. How touch and gamepads map onto it is **our design**, not
//! the original game's (it has no touch or gamepad support), chosen to give
//! every input path the same reach:
//!
//! - **Touch** (a primary touch pointer): the contact point moves the cursor
//!   there, then presses the left mouse button; lifting releases it. Moving
//!   while touching moves the cursor. On the title menu a tap on a row
//!   selects it (hover), and a tap on the selected row, now under the glass,
//!   activates it. Screens that react to keys only (the logo sequence,
//!   skipped with the quit key) treat a tap as that key
//!   ([`Input::touch_starts`]).
//! - **Gamepad** ([`GamepadButton`], [`GamepadAxis`]): south (A / cross) is
//!   Enter, east (B / circle) is Escape, the d-pad is the arrow keys, and the
//!   left stick acts as the arrow keys once pushed past [`STICK_PRESS`]
//!   (released below [`STICK_RELEASE`]). The stick does not move the cursor.
//! - **Absolute pointers** (a mouse over the page without pointer lock, and
//!   touch): the cursor is placed under the pointer, inverting the screen
//!   mapping, so the drawn cursor and the system pointer agree. The
//!   sensitivity rule of §7 applies to relative motion only
//!   ([`Event::PointerMotion`], e.g. under pointer lock).
//! - Keyboard auto-repeat is not a key press: platforms report physical
//!   presses only.
//!
//! Beyond the menu, the free camera of the level view (our design, not the
//! game's) reads raw state the menu ignores: held movement keys
//! ([`Input::key_held`]), mouse motion ([`Input::pointer_delta`]), every
//! touch contact ([`Input::contacts`], from [`Event::Contact`]; the primary
//! one also drives the cursor as above) and analog gamepad axes
//! ([`Input::axis`]: both sticks and the triggers). Gamepad buttons without
//! a key (shoulders, stick click) are held as [`Control::Pad`].

/// Virtual UI space (§1): 1024 × 768 units, origin top-left, y downward.
pub const UI_WIDTH: f32 = 1024.0;
pub const UI_HEIGHT: f32 = 768.0;

/// Default mouse sensitivity *s* (§7).
pub const DEFAULT_SENSITIVITY: f32 = 1.5;

/// A press of a mouse button less than this many milliseconds after the
/// previous press of the same button is a double click (§4.4).
pub const DOUBLE_CLICK_MS: f64 = 250.0;

/// Stick deflection (−1 … +1) at which a stick direction presses its key,
/// and below which it releases it again. Our design (see module docs).
pub const STICK_PRESS: f32 = 0.5;
pub const STICK_RELEASE: f32 = 0.25;

/// UI position of normalised cursor coordinates (§7):
/// ((*n*ₓ + 1) · 512, (*n*ᵧ + 1) · 384).
pub fn ui_from_normalized([nx, ny]: [f32; 2]) -> [f32; 2] {
    [(nx + 1.0) * UI_WIDTH / 2.0, (ny + 1.0) * UI_HEIGHT / 2.0]
}

/// Screen position of a UI position on a `screen` of W × H (§8): each axis
/// scaled independently, no letterboxing.
pub fn screen_from_ui([x, y]: [f32; 2], [w, h]: [f32; 2]) -> [f32; 2] {
    [x * w / UI_WIDTH, y * h / UI_HEIGHT]
}

/// Inverse of [`screen_from_ui`].
pub fn ui_from_screen([x, y]: [f32; 2], [w, h]: [f32; 2]) -> [f32; 2] {
    [x * UI_WIDTH / w, y * UI_HEIGHT / h]
}

/// Normalised cursor coordinates that put the cursor at screen position
/// `at` (the inverse of §8 after §7), clamped to −1 … +1.
pub fn normalized_from_screen([x, y]: [f32; 2], [w, h]: [f32; 2]) -> [f32; 2] {
    [clamp_unit(2.0 * x / w - 1.0), clamp_unit(2.0 * y / h - 1.0)]
}

/// Moves normalised cursor `n` by relative pointer motion `delta` in screen
/// pixels on a `screen` of W × H with sensitivity `s` (§7):
/// *n* += (Δx · *s* / W, Δy · *s* / H), clamped to −1 … +1.
pub fn apply_motion([nx, ny]: [f32; 2], [dx, dy]: [f32; 2], [w, h]: [f32; 2], s: f32) -> [f32; 2] {
    [clamp_unit(nx + dx * s / w), clamp_unit(ny + dy * s / h)]
}

fn clamp_unit(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(-1.0, 1.0) }
}

/// Keys the engine distinguishes; the rest are not reported. Those the menu
/// uses (§4.4) plus the console key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    Q,
    F12,
    /// The key left of 1 (`` ` `` / `~`), for the console.
    Grave,
    W,
    A,
    S,
    D,
    Space,
    C,
    /// Either Shift key.
    Shift,
}

impl Key {
    pub const ALL: [Key; 16] = [
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Enter,
        Key::Escape,
        Key::Q,
        Key::F12,
        Key::Grave,
        Key::W,
        Key::A,
        Key::S,
        Key::D,
        Key::Space,
        Key::C,
        Key::Shift,
    ];

    /// Stable name, used by platforms and the engine interface.
    pub fn name(self) -> &'static str {
        match self {
            Key::Up => "up",
            Key::Down => "down",
            Key::Left => "left",
            Key::Right => "right",
            Key::Enter => "enter",
            Key::Escape => "escape",
            Key::Q => "q",
            Key::F12 => "f12",
            Key::Grave => "grave",
            Key::W => "w",
            Key::A => "a",
            Key::S => "s",
            Key::D => "d",
            Key::Space => "space",
            Key::C => "c",
            Key::Shift => "shift",
        }
    }

    pub fn from_name(name: &str) -> Option<Key> {
        Key::ALL.into_iter().find(|k| k.name() == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub const ALL: [MouseButton; 3] = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];

    pub fn name(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
        }
    }

    pub fn from_name(name: &str) -> Option<MouseButton> {
        MouseButton::ALL.into_iter().find(|b| b.name() == name)
    }
}

/// Gamepad buttons by position, in the common layout (south = A on Xbox,
/// cross on PlayStation). Only those with a mapping are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    South,
    East,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    LeftShoulder,
    RightShoulder,
    /// The left stick pressed in.
    LeftStick,
}

impl GamepadButton {
    pub const ALL: [GamepadButton; 9] = [
        GamepadButton::South,
        GamepadButton::East,
        GamepadButton::DpadUp,
        GamepadButton::DpadDown,
        GamepadButton::DpadLeft,
        GamepadButton::DpadRight,
        GamepadButton::LeftShoulder,
        GamepadButton::RightShoulder,
        GamepadButton::LeftStick,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GamepadButton::South => "south",
            GamepadButton::East => "east",
            GamepadButton::DpadUp => "dpad_up",
            GamepadButton::DpadDown => "dpad_down",
            GamepadButton::DpadLeft => "dpad_left",
            GamepadButton::DpadRight => "dpad_right",
            GamepadButton::LeftShoulder => "left_shoulder",
            GamepadButton::RightShoulder => "right_shoulder",
            GamepadButton::LeftStick => "left_stick",
        }
    }

    pub fn from_name(name: &str) -> Option<GamepadButton> {
        GamepadButton::ALL.into_iter().find(|b| b.name() == name)
    }

    /// The key this button acts as (our design, see module docs); `None`
    /// for buttons held as themselves ([`Control::Pad`]).
    pub fn key(self) -> Option<Key> {
        match self {
            GamepadButton::South => Some(Key::Enter),
            GamepadButton::East => Some(Key::Escape),
            GamepadButton::DpadUp => Some(Key::Up),
            GamepadButton::DpadDown => Some(Key::Down),
            GamepadButton::DpadLeft => Some(Key::Left),
            GamepadButton::DpadRight => Some(Key::Right),
            _ => None,
        }
    }

    /// The control holding this button.
    pub fn control(self) -> Control {
        self.key().map_or(Control::Pad(self), Control::Key)
    }
}

/// Analog gamepad axes. Sticks: −1 is left / up, +1 right / down.
/// Triggers: 0 released … 1 fully pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamepadAxis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
}

impl GamepadAxis {
    pub const ALL: [GamepadAxis; 6] = [
        GamepadAxis::LeftX,
        GamepadAxis::LeftY,
        GamepadAxis::RightX,
        GamepadAxis::RightY,
        GamepadAxis::LeftTrigger,
        GamepadAxis::RightTrigger,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GamepadAxis::LeftX => "left_x",
            GamepadAxis::LeftY => "left_y",
            GamepadAxis::RightX => "right_x",
            GamepadAxis::RightY => "right_y",
            GamepadAxis::LeftTrigger => "left_trigger",
            GamepadAxis::RightTrigger => "right_trigger",
        }
    }

    pub fn from_name(name: &str) -> Option<GamepadAxis> {
        GamepadAxis::ALL.into_iter().find(|a| a.name() == name)
    }

    /// Keys for the negative and positive directions (the left stick only).
    fn keys(self) -> Option<[Key; 2]> {
        match self {
            GamepadAxis::LeftX => Some([Key::Left, Key::Right]),
            GamepadAxis::LeftY => Some([Key::Up, Key::Down]),
            _ => None,
        }
    }
}

/// What a platform reports. Screen positions are in the same units as the
/// screen size given to [`Input::set_screen`], origin top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// An absolute pointer (mouse without pointer lock) is at `at`.
    PointerMoved {
        at: [f32; 2],
    },
    /// Relative pointer motion in screen pixels (e.g. under pointer lock).
    PointerMotion {
        delta: [f32; 2],
    },
    /// A mouse button changed at `at` (absolute pointers) at `time` ms.
    PointerButton {
        at: Option<[f32; 2]>,
        button: MouseButton,
        pressed: bool,
        time: f64,
    },
    /// A primary touch contact began, moved or ended at `at`.
    Touch {
        at: [f32; 2],
        phase: TouchPhase,
        time: f64,
    },
    /// A physical key press or release (no auto-repeat).
    Key {
        key: Key,
        pressed: bool,
    },
    GamepadButton {
        button: GamepadButton,
        pressed: bool,
    },
    GamepadAxis {
        axis: GamepadAxis,
        value: f32,
    },
    /// Any touch contact (the primary one included) began, moved or ended
    /// at `at`; `id` tells simultaneous contacts apart.
    Contact {
        id: u32,
        at: [f32; 2],
        phase: TouchPhase,
    },
}

/// A touch contact, in screen units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub id: u32,
    /// Where it began.
    pub start: [f32; 2],
    pub at: [f32; 2],
    /// Where it was at the end of the previous frame (or where it began).
    pub previous: [f32; 2],
    /// Began this frame.
    pub new: bool,
    /// Lifted this frame (gone after [`Input::end_frame`]).
    pub ended: bool,
}

impl Contact {
    /// Motion this frame.
    pub fn delta(&self) -> [f32; 2] {
        [self.at[0] - self.previous[0], self.at[1] - self.previous[1]]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Start,
    Move,
    /// Lifted, or cancelled by the platform.
    End,
}

/// A key or mouse button: what the menu reacts to, whatever the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Control {
    Key(Key),
    Mouse(MouseButton),
    /// A gamepad button that acts as no key.
    Pad(GamepadButton),
}

impl Control {
    pub fn name(self) -> &'static str {
        match self {
            Control::Key(k) => k.name(),
            Control::Mouse(MouseButton::Left) => "mouse_left",
            Control::Mouse(MouseButton::Right) => "mouse_right",
            Control::Mouse(MouseButton::Middle) => "mouse_middle",
            Control::Pad(b) => b.name(),
        }
    }
}

/// What happened, in order, reduced to the menu's vocabulary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// The cursor moved to normalised position `to`.
    Cursor {
        to: [f32; 2],
    },
    /// `double` for the second press of a double click (mouse only).
    Press {
        control: Control,
        double: bool,
    },
    Release {
        control: Control,
    },
}

/// Where a held control comes from; a control is held while any source
/// holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Keyboard,
    Mouse,
    Touch,
    Pad,
    Stick,
}

/// Input state: the cursor, held controls and this frame's actions.
#[derive(Debug)]
pub struct Input {
    screen: [f32; 2],
    sensitivity: f32,
    /// Normalised; the initial position is not established by the spec
    /// (§7), the centre is our choice.
    cursor: [f32; 2],
    held: Vec<(Source, Control)>,
    actions: Vec<Action>,
    /// Time of the last single (non-double) press per mouse button.
    last_press: [Option<f64>; 3],
    /// Actions since start, including earlier frames.
    count: u64,
    /// Touch contacts begun this frame.
    touch_starts: u32,
    /// The absolute pointer's latest position.
    pointer_at: Option<[f32; 2]>,
    /// Pointer motion this frame.
    pointer_delta: [f32; 2],
    /// Pointer motion this frame while a mouse button was held.
    drag_delta: [f32; 2],
    contacts: Vec<Contact>,
    axes: [f32; 6],
}

impl Default for Input {
    fn default() -> Self {
        Input {
            screen: [UI_WIDTH, UI_HEIGHT],
            sensitivity: DEFAULT_SENSITIVITY,
            cursor: [0.0, 0.0],
            held: Vec::new(),
            actions: Vec::new(),
            last_press: [None; 3],
            count: 0,
            touch_starts: 0,
            pointer_at: None,
            pointer_delta: [0.0; 2],
            drag_delta: [0.0; 2],
            contacts: Vec::new(),
            axes: [0.0; 6],
        }
    }
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    /// The output size W × H, in the units of event positions. Ignored
    /// unless both are positive.
    pub fn set_screen(&mut self, screen: [f32; 2]) {
        if screen[0] > 0.0 && screen[1] > 0.0 {
            self.screen = screen;
        }
    }

    pub fn screen(&self) -> [f32; 2] {
        self.screen
    }

    pub fn set_sensitivity(&mut self, s: f32) {
        self.sensitivity = s;
    }

    pub fn sensitivity(&self) -> f32 {
        self.sensitivity
    }

    /// Normalised cursor position, −1 … +1 per axis.
    pub fn cursor(&self) -> [f32; 2] {
        self.cursor
    }

    /// Cursor position in UI units.
    pub fn cursor_ui(&self) -> [f32; 2] {
        ui_from_normalized(self.cursor)
    }

    /// Whether any source holds `control`.
    pub fn is_held(&self, control: Control) -> bool {
        self.held.iter().any(|&(_, c)| c == control)
    }

    /// Whether `key` is held by the keyboard or a gamepad button (not by the
    /// left stick acting as arrow keys; read [`Input::axis`] for it).
    pub fn key_held(&self, key: Key) -> bool {
        self.held
            .iter()
            .any(|&(s, c)| c == Control::Key(key) && s != Source::Stick)
    }

    /// Mouse motion this frame, in screen units (absolute moves and relative
    /// motion alike).
    pub fn pointer_delta(&self) -> [f32; 2] {
        self.pointer_delta
    }

    /// Mouse motion this frame made while a mouse button was held (a drag,
    /// even one pressed and released within the frame).
    pub fn drag_delta(&self) -> [f32; 2] {
        self.drag_delta
    }

    /// Touch contacts down this frame (and those lifted this frame).
    pub fn contacts(&self) -> &[Contact] {
        &self.contacts
    }

    /// The latest value of gamepad axis `axis`.
    pub fn axis(&self, axis: GamepadAxis) -> f32 {
        self.axes[axis as usize]
    }

    /// Held controls, each once, in the order first pressed.
    pub fn held(&self) -> Vec<Control> {
        let mut out: Vec<Control> = Vec::new();
        for &(_, c) in &self.held {
            if !out.contains(&c) {
                out.push(c);
            }
        }
        out
    }

    /// This frame's actions, in order.
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// Number of actions since start (for tests and debugging).
    pub fn action_count(&self) -> u64 {
        self.count
    }

    /// Touch contacts begun this frame. They also press the left mouse
    /// button; screens that need touch to reach a key the touch path lacks
    /// (e.g. skipping the logo sequence, which only keys do) read this.
    pub fn touch_starts(&self) -> u32 {
        self.touch_starts
    }

    /// Clears this frame's actions; call once per frame after the consumers.
    pub fn end_frame(&mut self) {
        self.actions.clear();
        self.touch_starts = 0;
        self.pointer_delta = [0.0; 2];
        self.drag_delta = [0.0; 2];
        self.contacts.retain(|c| !c.ended);
        for c in &mut self.contacts {
            c.previous = c.at;
            c.new = false;
        }
    }

    /// Releases every held control, e.g. when the platform stops delivering
    /// input (focus lost) and releases would otherwise never arrive.
    pub fn release_all(&mut self) {
        while let Some(&(source, control)) = self.held.last() {
            self.set(source, control, false, false);
        }
        for c in &mut self.contacts {
            c.ended = true;
        }
        self.axes = [0.0; 6];
    }

    pub fn handle(&mut self, event: Event) {
        match event {
            Event::PointerMoved { at } => {
                self.pointer_to(at);
                self.move_to(at)
            }
            Event::PointerMotion { delta } => {
                self.add_motion(delta);
                let to = apply_motion(self.cursor, delta, self.screen, self.sensitivity);
                self.set_cursor(to);
            }
            Event::PointerButton {
                at,
                button,
                pressed,
                time,
            } => {
                if let Some(at) = at {
                    self.pointer_to(at);
                    self.move_to(at);
                }
                self.mouse_button(Source::Mouse, button, pressed, time);
            }
            Event::Touch { at, phase, time } => {
                self.move_to(at);
                match phase {
                    TouchPhase::Start => {
                        self.touch_starts += 1;
                        self.mouse_button(Source::Touch, MouseButton::Left, true, time)
                    }
                    TouchPhase::End => {
                        self.mouse_button(Source::Touch, MouseButton::Left, false, time)
                    }
                    TouchPhase::Move => {}
                }
            }
            Event::Key { key, pressed } => {
                self.set(Source::Keyboard, Control::Key(key), pressed, false)
            }
            Event::GamepadButton { button, pressed } => {
                self.set(Source::Pad, button.control(), pressed, false)
            }
            Event::Contact { id, at, phase } => self.contact(id, at, phase),
            Event::GamepadAxis { axis, value } => {
                self.axes[axis as usize] = value;
                let Some(keys) = axis.keys() else {
                    return;
                };
                let [negative, positive] = keys.map(Control::Key);
                for (control, deflection) in [(negative, -value), (positive, value)] {
                    let held = self.held.contains(&(Source::Stick, control));
                    if !held && deflection >= STICK_PRESS {
                        self.set(Source::Stick, control, true, false);
                    } else if held && deflection < STICK_RELEASE {
                        self.set(Source::Stick, control, false, false);
                    }
                }
            }
        }
    }

    fn pointer_to(&mut self, at: [f32; 2]) {
        if let Some(last) = self.pointer_at {
            self.add_motion([at[0] - last[0], at[1] - last[1]]);
        }
        self.pointer_at = Some(at);
    }

    fn add_motion(&mut self, delta: [f32; 2]) {
        let dragging = self.held.iter().any(|&(s, _)| s == Source::Mouse);
        for i in 0..2 {
            self.pointer_delta[i] += delta[i];
            if dragging {
                self.drag_delta[i] += delta[i];
            }
        }
    }

    fn contact(&mut self, id: u32, at: [f32; 2], phase: TouchPhase) {
        let existing = self.contacts.iter().position(|c| c.id == id && !c.ended);
        match (phase, existing) {
            (TouchPhase::Start, None) => self.contacts.push(Contact {
                id,
                start: at,
                at,
                previous: at,
                new: true,
                ended: false,
            }),
            (TouchPhase::Start | TouchPhase::Move, Some(i)) => self.contacts[i].at = at,
            (TouchPhase::End, Some(i)) => {
                self.contacts[i].at = at;
                self.contacts[i].ended = true;
            }
            // A move or end of a contact never begun: ignored.
            (TouchPhase::Move | TouchPhase::End, None) => {}
        }
    }

    fn move_to(&mut self, at: [f32; 2]) {
        self.set_cursor(normalized_from_screen(at, self.screen));
    }

    fn set_cursor(&mut self, to: [f32; 2]) {
        if to != self.cursor {
            self.cursor = to;
            self.push(Action::Cursor { to });
        }
    }

    fn mouse_button(&mut self, source: Source, button: MouseButton, pressed: bool, time: f64) {
        let mut double = false;
        if pressed && !self.held.contains(&(source, Control::Mouse(button))) {
            let last = &mut self.last_press[button as usize];
            // The second press of a pair is the double click; a third starts
            // a new pair.
            double = last.is_some_and(|t| time - t < DOUBLE_CLICK_MS);
            *last = if double { None } else { Some(time) };
        }
        self.set(source, Control::Mouse(button), pressed, double);
    }

    /// Records `source` pressing or releasing `control`; repeated presses
    /// or releases from the same source are ignored.
    fn set(&mut self, source: Source, control: Control, pressed: bool, double: bool) {
        let index = self.held.iter().position(|&h| h == (source, control));
        match (pressed, index) {
            (true, None) => {
                self.held.push((source, control));
                self.push(Action::Press { control, double });
            }
            (false, Some(i)) => {
                self.held.remove(i);
                self.push(Action::Release { control });
            }
            _ => {}
        }
    }

    fn push(&mut self, action: Action) {
        self.actions.push(action);
        self.count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: [f32; 2] = [1920.0, 1080.0];

    fn press(control: Control) -> Action {
        Action::Press {
            control,
            double: false,
        }
    }

    #[test]
    fn normalized_cursor_maps_to_ui_space() {
        assert_eq!(ui_from_normalized([-1.0, -1.0]), [0.0, 0.0]);
        assert_eq!(ui_from_normalized([0.0, 0.0]), [512.0, 384.0]);
        assert_eq!(ui_from_normalized([1.0, 1.0]), [1024.0, 768.0]);
        assert_eq!(ui_from_normalized([0.5, -0.5]), [768.0, 192.0]);
    }

    #[test]
    fn ui_space_stretches_to_the_screen() {
        assert_eq!(screen_from_ui([512.0, 384.0], SCREEN), [960.0, 540.0]);
        assert_eq!(
            screen_from_ui([1024.0, 768.0], [800.0, 600.0]),
            [800.0, 600.0]
        );
        assert_eq!(ui_from_screen([960.0, 270.0], SCREEN), [512.0, 192.0]);
    }

    #[test]
    fn screen_position_inverts_the_mapping() {
        for at in [[0.0, 0.0], [960.0, 540.0], [480.0, 810.0], [1920.0, 1080.0]] {
            let n = normalized_from_screen(at, SCREEN);
            assert_eq!(screen_from_ui(ui_from_normalized(n), SCREEN), at);
        }
        // Outside the screen clamps to its edge.
        assert_eq!(normalized_from_screen([-10.0, 2000.0], SCREEN), [-1.0, 1.0]);
    }

    #[test]
    fn motion_scales_by_sensitivity_and_clamps() {
        let s = DEFAULT_SENSITIVITY;
        // A full screen width of motion moves 1.5 normalised units.
        assert_eq!(
            apply_motion([-1.0, 0.0], [1920.0, 0.0], SCREEN, s),
            [0.5, 0.0]
        );
        assert_eq!(
            apply_motion([0.0, 0.0], [192.0, -108.0], SCREEN, s),
            [0.15, -0.15]
        );
        assert_eq!(
            apply_motion([0.9, -0.9], [960.0, -540.0], SCREEN, s),
            [1.0, -1.0]
        );
        assert_eq!(
            apply_motion([0.0, 0.0], [192.0, 0.0], SCREEN, 1.0),
            [0.1, 0.0]
        );
    }

    #[test]
    fn relative_motion_uses_the_input_sensitivity() {
        let mut input = Input::new();
        input.set_screen(SCREEN);
        assert_eq!(input.sensitivity(), 1.5);
        assert_eq!(input.cursor(), [0.0, 0.0]);
        input.handle(Event::PointerMotion {
            delta: [192.0, 108.0],
        });
        assert_eq!(input.cursor(), [0.15, 0.15]);
        input.handle(Event::PointerMotion { delta: [1e6, -1e6] });
        assert_eq!(input.cursor(), [1.0, -1.0]);
        assert_eq!(input.cursor_ui(), [1024.0, 0.0]);
    }

    #[test]
    fn mouse_moves_cursor_and_clicks() {
        let mut input = Input::new();
        input.set_screen(SCREEN);
        input.handle(Event::PointerMoved { at: [480.0, 270.0] });
        input.handle(Event::PointerMoved { at: [480.0, 270.0] }); // No change.
        input.handle(Event::PointerButton {
            at: Some([480.0, 270.0]),
            button: MouseButton::Left,
            pressed: true,
            time: 0.0,
        });
        assert!(input.is_held(Control::Mouse(MouseButton::Left)));
        input.handle(Event::PointerButton {
            at: None,
            button: MouseButton::Left,
            pressed: false,
            time: 10.0,
        });
        let left = Control::Mouse(MouseButton::Left);
        assert_eq!(
            input.actions(),
            [
                Action::Cursor { to: [-0.5, -0.5] },
                press(left),
                Action::Release { control: left },
            ]
        );
        assert_eq!(input.cursor_ui(), [256.0, 192.0]);
        assert!(input.held().is_empty());
        input.end_frame();
        assert!(input.actions().is_empty());
        assert_eq!(input.action_count(), 3);
    }

    #[test]
    fn double_click_is_the_second_quick_press() {
        let mut input = Input::new();
        let left = Control::Mouse(MouseButton::Left);
        let mut doubles = Vec::new();
        for time in [0.0, 249.0, 400.0, 700.0, 800.0, 900.0] {
            for pressed in [true, false] {
                input.handle(Event::PointerButton {
                    at: None,
                    button: MouseButton::Left,
                    pressed,
                    time,
                });
            }
            let Some(&Action::Press { control, double }) = input.actions().first() else {
                panic!("no press");
            };
            assert_eq!(control, left);
            doubles.push(double);
            input.end_frame();
        }
        assert_eq!(doubles, [false, true, false, false, true, false]);
    }

    #[test]
    fn touch_moves_cursor_then_presses() {
        let mut input = Input::new();
        input.set_screen([400.0, 800.0]);
        let left = Control::Mouse(MouseButton::Left);
        input.handle(Event::Touch {
            at: [300.0, 200.0],
            phase: TouchPhase::Start,
            time: 0.0,
        });
        input.handle(Event::Touch {
            at: [300.0, 600.0],
            phase: TouchPhase::Move,
            time: 50.0,
        });
        assert!(input.is_held(left));
        input.handle(Event::Touch {
            at: [300.0, 600.0],
            phase: TouchPhase::End,
            time: 100.0,
        });
        assert_eq!(
            input.actions(),
            [
                Action::Cursor { to: [0.5, -0.5] },
                press(left),
                Action::Cursor { to: [0.5, 0.5] },
                Action::Release { control: left },
            ]
        );
        assert_eq!(input.touch_starts(), 1);
        input.end_frame();
        assert_eq!(input.touch_starts(), 0);
        // A second tap soon after is a double click, as with the mouse.
        input.handle(Event::Touch {
            at: [300.0, 600.0],
            phase: TouchPhase::Start,
            time: 200.0,
        });
        assert_eq!(
            input.actions().last(),
            Some(&Action::Press {
                control: left,
                double: true
            })
        );
    }

    #[test]
    fn keys_press_once_until_released() {
        let mut input = Input::new();
        let up = Control::Key(Key::Up);
        input.handle(Event::Key {
            key: Key::Up,
            pressed: true,
        });
        input.handle(Event::Key {
            key: Key::Up,
            pressed: true,
        });
        assert!(input.is_held(up));
        input.handle(Event::Key {
            key: Key::Up,
            pressed: false,
        });
        input.handle(Event::Key {
            key: Key::Up,
            pressed: false,
        });
        assert_eq!(
            input.actions(),
            [press(up), Action::Release { control: up }]
        );
    }

    #[test]
    fn gamepad_buttons_act_as_keys() {
        let expected = [
            (GamepadButton::South, Key::Enter),
            (GamepadButton::East, Key::Escape),
            (GamepadButton::DpadUp, Key::Up),
            (GamepadButton::DpadDown, Key::Down),
            (GamepadButton::DpadLeft, Key::Left),
            (GamepadButton::DpadRight, Key::Right),
        ];
        for (button, key) in expected {
            let mut input = Input::new();
            input.handle(Event::GamepadButton {
                button,
                pressed: true,
            });
            assert!(input.is_held(Control::Key(key)), "{button:?}");
            assert!(input.key_held(key), "{button:?}");
            input.handle(Event::GamepadButton {
                button,
                pressed: false,
            });
            assert_eq!(
                input.actions(),
                [
                    press(Control::Key(key)),
                    Action::Release {
                        control: Control::Key(key)
                    }
                ],
                "{button:?}"
            );
        }
    }

    #[test]
    fn stick_acts_as_arrow_keys_with_hysteresis() {
        let mut input = Input::new();
        let (up, down) = (Control::Key(Key::Up), Control::Key(Key::Down));
        for value in [-0.3, -0.6, -0.9, -0.4, -0.2, 0.0, 0.7, 1.0, 0.1] {
            input.handle(Event::GamepadAxis {
                axis: GamepadAxis::LeftY,
                value,
            });
        }
        assert_eq!(
            input.actions(),
            [
                press(up),
                Action::Release { control: up },
                press(down),
                Action::Release { control: down },
            ]
        );
        input.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftX,
            value: 0.8,
        });
        assert!(input.is_held(Control::Key(Key::Right)));
    }

    #[test]
    fn a_control_is_held_while_any_source_holds_it() {
        let mut input = Input::new();
        let enter = Control::Key(Key::Enter);
        input.handle(Event::Key {
            key: Key::Enter,
            pressed: true,
        });
        input.handle(Event::GamepadButton {
            button: GamepadButton::South,
            pressed: true,
        });
        input.handle(Event::Key {
            key: Key::Enter,
            pressed: false,
        });
        assert!(input.is_held(enter));
        assert_eq!(input.held(), [enter]);
        input.handle(Event::GamepadButton {
            button: GamepadButton::South,
            pressed: false,
        });
        assert!(!input.is_held(enter));
    }

    #[test]
    fn release_all_releases_every_source() {
        let mut input = Input::new();
        input.handle(Event::Key {
            key: Key::Up,
            pressed: true,
        });
        input.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: -1.0,
        });
        input.handle(Event::Touch {
            at: [0.0, 0.0],
            phase: TouchPhase::Start,
            time: 0.0,
        });
        input.end_frame();
        input.release_all();
        assert!(input.held().is_empty());
        assert_eq!(input.actions().len(), 3);
        assert!(
            input
                .actions()
                .iter()
                .all(|a| matches!(a, Action::Release { .. }))
        );
    }

    #[test]
    fn names_round_trip() {
        for key in Key::ALL {
            assert_eq!(Key::from_name(key.name()), Some(key));
        }
        for button in MouseButton::ALL {
            assert_eq!(MouseButton::from_name(button.name()), Some(button));
        }
        for button in GamepadButton::ALL {
            assert_eq!(GamepadButton::from_name(button.name()), Some(button));
        }
        for axis in GamepadAxis::ALL {
            assert_eq!(GamepadAxis::from_name(axis.name()), Some(axis));
        }
        assert_eq!(Key::from_name("space"), Some(Key::Space));
        assert_eq!(Key::from_name("tab"), None);
    }

    #[test]
    fn raw_state_for_the_free_camera() {
        let mut input = Input::new();
        // Mouse motion accumulates per frame.
        input.handle(Event::PointerMoved { at: [10.0, 10.0] });
        input.handle(Event::PointerMoved { at: [15.0, 8.0] });
        input.handle(Event::PointerMotion { delta: [1.0, 1.0] });
        assert_eq!(input.pointer_delta(), [6.0, -1.0]);
        assert_eq!(input.drag_delta(), [0.0, 0.0]);
        // A drag within one frame still counts.
        for (pressed, at) in [(true, [15.0, 8.0]), (false, [25.0, 8.0])] {
            input.handle(Event::PointerButton {
                at: Some(at),
                button: MouseButton::Right,
                pressed,
                time: 0.0,
            });
        }
        assert_eq!(input.drag_delta(), [10.0, 0.0]);
        input.end_frame();
        assert_eq!(input.pointer_delta(), [0.0, 0.0]);
        // Analog axes; the stick also holds arrow keys, which key_held
        // leaves out.
        input.handle(Event::GamepadAxis {
            axis: GamepadAxis::LeftY,
            value: -0.8,
        });
        input.handle(Event::GamepadAxis {
            axis: GamepadAxis::RightTrigger,
            value: 0.6,
        });
        assert_eq!(input.axis(GamepadAxis::LeftY), -0.8);
        assert_eq!(input.axis(GamepadAxis::RightTrigger), 0.6);
        assert!(input.is_held(Control::Key(Key::Up)));
        assert!(!input.key_held(Key::Up));
        input.handle(Event::GamepadButton {
            button: GamepadButton::LeftShoulder,
            pressed: true,
        });
        assert!(input.is_held(Control::Pad(GamepadButton::LeftShoulder)));
        // Contacts: begun, moved, deltas per frame, ended.
        let contact = |id, at, phase| Event::Contact { id, at, phase };
        input.handle(contact(1, [0.0, 0.0], TouchPhase::Start));
        input.handle(contact(2, [50.0, 0.0], TouchPhase::Start));
        input.handle(contact(1, [3.0, 4.0], TouchPhase::Move));
        assert_eq!(input.contacts().len(), 2);
        assert_eq!(input.contacts()[0].delta(), [3.0, 4.0]);
        assert!(input.contacts()[0].new);
        input.end_frame();
        input.handle(contact(1, [4.0, 4.0], TouchPhase::End));
        assert_eq!(input.contacts()[0].delta(), [1.0, 0.0]);
        assert!(input.contacts()[0].ended);
        assert_eq!(input.contacts()[0].start, [0.0, 0.0]);
        input.end_frame();
        assert_eq!(input.contacts().len(), 1);
        input.release_all();
        assert!(input.contacts()[0].ended);
        assert_eq!(input.axis(GamepadAxis::RightTrigger), 0.0);
    }
}
