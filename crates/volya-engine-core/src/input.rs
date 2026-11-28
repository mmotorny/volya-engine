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
//! - **The on-screen stick and cursor buttons** wherever the game shows
//!   its cursor in the standard mobile mode (the user's design, #856): the
//!   page's stick ([`Event::CursorStick`]) moves the cursor as a gamepad's
//!   left stick does, through [`Input::pad_cursor_frame`]; its Click and
//!   Right-click buttons are the gamepad's south and west there, the mouse
//!   buttons at the cursor, and Back is its east ([`Event::CursorButton`]).
//!   Two fingers on the picture are wheel steps. A touch never clicks
//!   where it lands there.
//! - **Touch as direct clicks** there instead in the ergonomic mobile mode
//!   (the user's design, #807): touches arrive as
//!   [`Event::Touch`] (below), and while the platform says so
//!   ([`Event::TouchDirect`]) the game's cursor isn't drawn
//!   ([`Input::cursor_drawn`]) until a mouse moves it.
//! - **Touch** elsewhere (a primary touch pointer, [`Event::Touch`]): the
//!   contact point moves the cursor there, then presses the left mouse
//!   button; lifting releases it. Screens that react to keys only (the logo
//!   sequence, skipped with the quit key) treat a tap as that key
//!   ([`Input::touch_starts`]).
//! - **Gamepad as a mouse** wherever the game shows its cursor (the user's
//!   design, #835; [`Input::set_pad_cursor`]), as console ports of mouse
//!   screens do: the left stick moves the cursor through an analog curve
//!   ([`pad_cursor_speed`]) with friction over clickable elements, south
//!   (A / cross) is the left button, held for drags, west (X / square)
//!   the right one, east (B / circle) Escape, the d-pad nudges the cursor
//!   and the right stick and the shoulders are the wheel
//!   ([`Input::pad_cursor_frame`]).
//! - **Gamepad** elsewhere ([`GamepadButton`], [`GamepadAxis`]): south is
//!   Enter, east is Escape, the d-pad is the arrow keys, and the left stick
//!   acts as the arrow keys once pushed past [`STICK_PRESS`] (released
//!   below [`STICK_RELEASE`]).
//! - **Absolute pointers** (a mouse over the page without pointer lock, and
//!   [`Event::Touch`]): the cursor is placed under the pointer, inverting the screen
//!   mapping, so the drawn cursor and the system pointer agree. The
//!   sensitivity rule of §7 applies to relative motion only
//!   ([`Event::PointerMotion`], e.g. under pointer lock).
//! - Keyboard auto-repeat is not a key press: platforms report physical
//!   presses only.
//!
//! Beyond the menu, the player's controls in a level read raw state the
//! menu ignores: keys and mouse buttons of the keyboard and mouse alone
//! ([`Input::device_held`], [`Input::device_changes`]), mouse motion
//! ([`Input::pointer_delta`], relative under pointer lock), gamepad buttons
//! as themselves ([`Input::pad_held`], [`Input::pad_changes`]) and analog
//! axes ([`Input::axis`]: both sticks and the triggers), and the page's
//! on-screen touch controls ([`Event::TouchStick`],
//! [`Event::TouchLookDelta`], [`Event::TouchLook`],
//! [`Event::TouchButton`]). Every touch contact is also kept
//! ([`Input::contacts`], from [`Event::Contact`]; the primary one also
//! drives the cursor as above). Gamepad buttons without a key (shoulders,
//! stick click, west) are held as [`Control::Pad`].

use crate::console::Clipboard;

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

/// The gamepad cursor's radial dead zone, as a fraction of full deflection:
/// XInput's `XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE`, 7849 of 32767
/// (documented, Microsoft's "Getting Started With XInput").
pub const PAD_CURSOR_DEAD_ZONE: f32 = 0.24;
/// The gamepad cursor's speed at full deflection, UI units per second:
/// about half a screen's width a second, reached linearly from the dead
/// zone's edge (the user's choice, #856: the square curve before it was
/// either too slow or overshot).
pub const PAD_CURSOR_MAX_SPEED: f32 = 500.0;
/// The gamepad cursor's speed over a clickable element, as a fraction of
/// its speed elsewhere: friction (our value; Roblox's and the Xbox
/// browser's virtual cursors slow over interactable elements, documented
/// without a number).
pub const PAD_CURSOR_FRICTION: f32 = 0.4;
/// What a d-pad press moves the cursor, UI units; held, it repeats after
/// [`PAD_REPEAT_DELAY`] every [`PAD_REPEAT_INTERVAL`] (our values).
pub const PAD_NUDGE: f32 = 4.0;
/// Seconds before a held d-pad direction or shoulder repeats, and between
/// repeats (our values, near a keyboard's typematic defaults).
pub const PAD_REPEAT_DELAY: f32 = 0.4;
pub const PAD_REPEAT_INTERVAL: f32 = 0.05;
/// Seconds between a held shoulder's wheel steps after the delay.
pub const PAD_WHEEL_INTERVAL: f32 = 0.1;
/// The right stick's dead zone as the wheel: XInput's
/// `XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE`, 8689 of 32767 (documented).
pub const PAD_WHEEL_DEAD_ZONE: f32 = 0.265;
/// Wheel steps a second at the right stick's full deflection, rising
/// linearly from none past the dead zone (our value).
pub const PAD_WHEEL_MAX_RATE: f32 = 12.0;

/// The fraction of the way from the dead zone's edge to full deflection
/// for a stick deflected `d` (0 … 1): the scaled radial dead zone.
fn past_dead_zone(d: f32, dead: f32) -> f32 {
    if d.is_nan() {
        return 0.0;
    }
    ((d - dead) / (1.0 - dead)).clamp(0.0, 1.0)
}

/// The gamepad cursor's speed in UI units per second for a left stick
/// deflected `d` (its length, 0 … 1; our design): none within
/// [`PAD_CURSOR_DEAD_ZONE`], then [`PAD_CURSOR_MAX_SPEED`] times the
/// fraction of the way out (linear).
pub fn pad_cursor_speed(d: f32) -> f32 {
    PAD_CURSOR_MAX_SPEED * past_dead_zone(d, PAD_CURSOR_DEAD_ZONE)
}

/// The gamepad cursor's motion this frame in UI units: the left stick at
/// `stick` (+y down) for `dt` seconds, slowed by [`PAD_CURSOR_FRICTION`]
/// when `over` a clickable element.
pub fn pad_cursor_motion(stick: [f32; 2], dt: f32, over: bool) -> [f32; 2] {
    let [x, y] = stick.map(|v| if v.is_finite() { v } else { 0.0 });
    let d = x.hypot(y);
    if d <= 0.0 || dt.is_nan() || dt <= 0.0 {
        return [0.0; 2];
    }
    let friction = if over { PAD_CURSOR_FRICTION } else { 1.0 };
    let step = pad_cursor_speed(d.min(1.0)) * friction * dt;
    [x / d * step, y / d * step]
}

/// Wheel steps a second for a right stick deflected `d` vertically (+
/// down, toward the content's end): negative for wheel up.
pub fn pad_wheel_rate(d: f32) -> f32 {
    PAD_WHEEL_MAX_RATE * d.signum() * past_dead_zone(d.abs(), PAD_WHEEL_DEAD_ZONE)
}

/// Whether UI point `p` lies in any of `targets` (x, y, width, height).
fn over_target([px, py]: [f32; 2], targets: &[[f32; 4]]) -> bool {
    targets
        .iter()
        .any(|&[x, y, w, h]| px >= x && px <= x + w && py >= y && py <= y + h)
}

/// A held gamepad button's auto-repeat: how many times it acts this frame.
#[derive(Clone, Copy, Debug, Default)]
struct Repeat {
    /// Seconds held, when held.
    held: Option<f32>,
}

impl Repeat {
    /// Presses this frame of `dt` seconds: one at the press, then one each
    /// `interval` from [`PAD_REPEAT_DELAY`] on while held.
    fn frame(&mut self, pressed: bool, held: bool, dt: f32, interval: f32) -> u32 {
        let mut n = 0;
        if pressed {
            self.held = Some(0.0);
            n += 1;
        } else if held && let Some(t) = &mut self.held {
            let before = *t;
            *t += dt;
            let count = |t: f32| {
                if t < PAD_REPEAT_DELAY {
                    0
                } else {
                    1 + ((t - PAD_REPEAT_DELAY) / interval) as u32
                }
            };
            n += count(*t) - count(before);
        }
        if !held && !pressed {
            self.held = None;
        }
        n.min(64)
    }
}

fn clamp_unit(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(-1.0, 1.0) }
}

/// How far past its rim the touch stick reports, in stick radii per axis
/// (our choice: room for the sprint lock above it).
pub const STICK_REACH: f32 = 3.0;

macro_rules! keys {
    ($($variant:ident $name:literal $dik:literal,)*) => {
        /// Keyboard keys the engine distinguishes: those of the game's key
        /// list (docs/behavior/options-screens.md section 11.2) a keyboard
        /// can report; other keys are not reported.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Key { $($variant,)* }

        impl Key {
            pub const ALL: &'static [Key] = &[$(Key::$variant,)*];

            /// Stable name, used by platforms and the engine interface.
            pub fn name(self) -> &'static str {
                match self { $(Key::$variant => $name,)* }
            }

            /// The key's name in the game's key list (options-screens.md
            /// section 11.2), as `bind` lines write it.
            pub fn dik_name(self) -> &'static str {
                match self { $(Key::$variant => $dik,)* }
            }
        }
    };
}

// Which physical key each name means. The game's key names (section 11.2)
// carry DirectInput key codes, which Microsoft's public DirectInput
// documentation (the `DIK_*` keyboard device constants) defines as the
// keyboard's set-1 scan codes. Browsers name physical keys by the W3C UI
// Events `KeyboardEvent.code` values (a public standard), whose code tables
// give each code's position and its Windows scan code; the web platform maps
// codes to the names below (`static/input.js`) by that scan code. Every
// pairing here is backed by those two documents (documented, not measured
// on hardware). Keys of the list with no `code` value are not reported:
// kCIRCUMFLEX, kAT, kCOLON, kUNDERLINE, kKANJI, kSTOP, kAX, kUNLABELED
// (NEC PC-98 and Japanese keyboards); mouse1–mouse8 are mouse buttons.
keys! {
    Escape "escape" "kESCAPE",
    Digit1 "1" "k1",
    Digit2 "2" "k2",
    Digit3 "3" "k3",
    Digit4 "4" "k4",
    Digit5 "5" "k5",
    Digit6 "6" "k6",
    Digit7 "7" "k7",
    Digit8 "8" "k8",
    Digit9 "9" "k9",
    Digit0 "0" "k0",
    Minus "minus" "kMINUS",
    Equals "equals" "kEQUALS",
    Backspace "backspace" "kBACK",
    Tab "tab" "kTAB",
    Q "q" "kQ",
    W "w" "kW",
    E "e" "kE",
    R "r" "kR",
    T "t" "kT",
    Y "y" "kY",
    U "u" "kU",
    I "i" "kI",
    O "o" "kO",
    P "p" "kP",
    LeftBracket "left_bracket" "kLBRACKET",
    RightBracket "right_bracket" "kRBRACKET",
    Enter "enter" "kRETURN",
    LeftControl "left_control" "kLCONTROL",
    A "a" "kA",
    S "s" "kS",
    D "d" "kD",
    F "f" "kF",
    G "g" "kG",
    H "h" "kH",
    J "j" "kJ",
    K "k" "kK",
    L "l" "kL",
    Semicolon "semicolon" "kSEMICOLON",
    Apostrophe "apostrophe" "kAPOSTROPHE",
    Grave "grave" "kGRAVE",
    LeftShift "left_shift" "kLSHIFT",
    Backslash "backslash" "kBACKSLASH",
    Z "z" "kZ",
    X "x" "kX",
    C "c" "kC",
    V "v" "kV",
    B "b" "kB",
    N "n" "kN",
    M "m" "kM",
    Comma "comma" "kCOMMA",
    Period "period" "kPERIOD",
    Slash "slash" "kSLASH",
    RightShift "right_shift" "kRSHIFT",
    NumpadMultiply "numpad_multiply" "kMULTIPLY",
    LeftAlt "left_alt" "kLMENU",
    Space "space" "kSPACE",
    CapsLock "caps_lock" "kCAPITAL",
    F1 "f1" "kF1",
    F2 "f2" "kF2",
    F3 "f3" "kF3",
    F4 "f4" "kF4",
    F5 "f5" "kF5",
    F6 "f6" "kF6",
    F7 "f7" "kF7",
    F8 "f8" "kF8",
    F9 "f9" "kF9",
    F10 "f10" "kF10",
    NumLock "num_lock" "kNUMLOCK",
    ScrollLock "scroll_lock" "kSCROLL",
    Numpad7 "numpad7" "kNUMPAD7",
    Numpad8 "numpad8" "kNUMPAD8",
    Numpad9 "numpad9" "kNUMPAD9",
    NumpadSubtract "numpad_subtract" "kSUBTRACT",
    Numpad4 "numpad4" "kNUMPAD4",
    Numpad5 "numpad5" "kNUMPAD5",
    Numpad6 "numpad6" "kNUMPAD6",
    NumpadAdd "numpad_add" "kADD",
    Numpad1 "numpad1" "kNUMPAD1",
    Numpad2 "numpad2" "kNUMPAD2",
    Numpad3 "numpad3" "kNUMPAD3",
    Numpad0 "numpad0" "kNUMPAD0",
    NumpadDecimal "numpad_decimal" "kDECIMAL",
    F11 "f11" "kF11",
    F12 "f12" "kF12",
    F13 "f13" "kF13",
    F14 "f14" "kF14",
    F15 "f15" "kF15",
    Kana "kana" "kKANA",
    Convert "convert" "kCONVERT",
    NoConvert "no_convert" "kNOCONVERT",
    Yen "yen" "kYEN",
    NumpadEquals "numpad_equals" "kNUMPADEQUALS",
    NumpadEnter "numpad_enter" "kNUMPADENTER",
    RightControl "right_control" "kRCONTROL",
    NumpadComma "numpad_comma" "kNUMPADCOMMA",
    NumpadDivide "numpad_divide" "kDIVIDE",
    PrintScreen "print_screen" "kSYSRQ",
    RightAlt "right_alt" "kRMENU",
    Pause "pause" "kPAUSE",
    Home "home" "kHOME",
    Up "up" "kUP",
    PageUp "page_up" "kPRIOR",
    Left "left" "kLEFT",
    Right "right" "kRIGHT",
    End "end" "kEND",
    Down "down" "kDOWN",
    PageDown "page_down" "kNEXT",
    Insert "insert" "kINSERT",
    Delete "delete" "kDELETE",
    LeftWin "left_win" "kLWIN",
    RightWin "right_win" "kRWIN",
    Apps "apps" "kAPPS",
}

impl Key {
    /// The key's code in the game's key list (options-screens.md section
    /// 11.2).
    pub fn code(self) -> u16 {
        let name = self.dik_name();
        crate::console::KEYS
            .iter()
            .find(|(k, _)| *k == name)
            .map_or(0, |&(_, code)| code)
    }

    pub fn from_name(name: &str) -> Option<Key> {
        Key::ALL.iter().copied().find(|k| k.name() == name)
    }

    /// The key named `name` in the game's key list, compared
    /// case-insensitively (section 11.2).
    pub fn from_dik_name(name: &str) -> Option<Key> {
        Key::ALL
            .iter()
            .copied()
            .find(|k| k.dik_name().eq_ignore_ascii_case(name))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// The five further buttons the game reads, which reach only the
    /// level's key handling, as the keys `mouse4` … `mouse8`
    /// (console-key-bindings.md section 5), never the interface's mouse
    /// events (options-screens.md conventions).
    Mouse4,
    Mouse5,
    Mouse6,
    Mouse7,
    Mouse8,
}

impl MouseButton {
    pub const ALL: [MouseButton; 8] = [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Mouse4,
        MouseButton::Mouse5,
        MouseButton::Mouse6,
        MouseButton::Mouse7,
        MouseButton::Mouse8,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
            MouseButton::Mouse4 => "mouse4",
            MouseButton::Mouse5 => "mouse5",
            MouseButton::Mouse6 => "mouse6",
            MouseButton::Mouse7 => "mouse7",
            MouseButton::Mouse8 => "mouse8",
        }
    }

    /// The button's key name (options-screens.md section 11.2): `mouse1`
    /// left, `mouse2` right, `mouse3` middle, then `mouse4` … `mouse8`.
    pub fn key_name(self) -> &'static str {
        match self {
            MouseButton::Left => "mouse1",
            MouseButton::Right => "mouse2",
            MouseButton::Middle => "mouse3",
            other => other.name(),
        }
    }

    /// The button named `name` among the key names `mouse1` … `mouse8`,
    /// case-insensitively (section 11.2).
    pub fn from_key_name(name: &str) -> Option<MouseButton> {
        MouseButton::ALL
            .into_iter()
            .find(|b| b.key_name().eq_ignore_ascii_case(name))
    }

    /// The left, right and middle buttons: the ones the interface gets
    /// mouse events of (options-screens.md conventions).
    pub fn is_interface(self) -> bool {
        matches!(
            self,
            MouseButton::Left | MouseButton::Right | MouseButton::Middle
        )
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
    West,
    /// Y on Xbox, triangle on PlayStation: opens the inventory screen in a
    /// level (our design, #110).
    North,
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
    pub const ALL: [GamepadButton; 11] = [
        GamepadButton::South,
        GamepadButton::East,
        GamepadButton::West,
        GamepadButton::North,
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
            GamepadButton::West => "west",
            GamepadButton::North => "north",
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

    /// The control holding this button while it drives the game's cursor
    /// ([`Input::set_pad_cursor`]; our design, #835): south the left mouse
    /// button, west the right one, east Escape as ever; the rest as
    /// themselves (the d-pad nudges the cursor).
    pub fn cursor_control(self) -> Control {
        match self {
            GamepadButton::South => Control::Mouse(MouseButton::Left),
            GamepadButton::West => Control::Mouse(MouseButton::Right),
            GamepadButton::East => Control::Key(Key::Escape),
            b => Control::Pad(b),
        }
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

/// A look finger's part of its touch ([`Event::TouchLook`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookPhase {
    /// The finger was put down on the look area: a tap or a drag, which
    /// the engine tells apart.
    Down,
    /// The finger began looking as a drag already: dragged from Jump or
    /// Crouch past its threshold. Never a tap.
    Drag,
    Move,
    /// Lifted where it is: it may fling.
    Up,
    /// Ended without acting (by the system or the controls hiding).
    Cancel,
}

/// One look finger's event ([`Event::TouchLook`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchLook {
    pub finger: u32,
    pub phase: LookPhase,
    /// Where the finger is, relative to the picture's centre in units of
    /// half its height, +y down: the tangent of its direction's angle off
    /// the view axis is this times the tangent of half the vertical field
    /// of view.
    pub at: [f32; 2],
    /// The page's event time, milliseconds.
    pub time: f64,
    /// Half the picture's height in CSS pixels: what [`TouchLook::at`]
    /// is measured in, for the tap's slop.
    pub half_height: f32,
}

/// The buttons of the page's on-screen touch controls in a level (our
/// design; the original has no touch support).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TouchButton {
    Jump,
    Crouch,
    /// Crouch swiped down: the low crouch (a gesture, pressed and released
    /// at once).
    CrouchLow,
    /// Does what Escape does.
    Menu,
    /// Opens the inventory screen (#110).
    Inventory,
    /// The `torch` command (#363).
    Torch,
    /// The `night_vision` command (#363).
    NightVision,
    /// Opens the console as a key bound to `console` does (#642).
    Console,
    /// Opens and closes the PDA as the key bound to `active_jobs` does
    /// (#730).
    Pda,
    /// The `use` action on the object under the crosshair (#874).
    Use,
}

impl TouchButton {
    pub const ALL: [TouchButton; 10] = [
        TouchButton::Jump,
        TouchButton::Crouch,
        TouchButton::CrouchLow,
        TouchButton::Menu,
        TouchButton::Inventory,
        TouchButton::Torch,
        TouchButton::NightVision,
        TouchButton::Console,
        TouchButton::Pda,
        TouchButton::Use,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TouchButton::Jump => "jump",
            TouchButton::Crouch => "crouch",
            TouchButton::CrouchLow => "crouch_low",
            TouchButton::Menu => "menu",
            TouchButton::Inventory => "inventory",
            TouchButton::Torch => "torch",
            TouchButton::NightVision => "night_vision",
            TouchButton::Console => "console",
            TouchButton::Pda => "pda",
            TouchButton::Use => "use",
        }
    }

    pub fn from_name(name: &str) -> Option<TouchButton> {
        TouchButton::ALL.into_iter().find(|b| b.name() == name)
    }
}

/// The on-screen buttons wherever the game shows its cursor in the
/// standard mobile mode (the user's design, #856): those of a gamepad
/// driving the cursor ([`GamepadButton::cursor_control`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CursorButton {
    /// The left mouse button at the cursor, as a gamepad's south: held, it
    /// drags; a second press within [`DOUBLE_CLICK_MS`] double-clicks.
    Click,
    /// The right mouse button, as a gamepad's west.
    RightClick,
    /// A gamepad's east: Escape, and each screen's back.
    Back,
}

impl CursorButton {
    pub const ALL: [CursorButton; 3] = [
        CursorButton::Click,
        CursorButton::RightClick,
        CursorButton::Back,
    ];

    pub fn name(self) -> &'static str {
        match self {
            CursorButton::Click => "click",
            CursorButton::RightClick => "right_click",
            CursorButton::Back => "back",
        }
    }

    pub fn from_name(name: &str) -> Option<CursorButton> {
        CursorButton::ALL.into_iter().find(|b| b.name() == name)
    }

    /// The gamepad button it acts as.
    pub fn pad(self) -> GamepadButton {
        match self {
            CursorButton::Click => GamepadButton::South,
            CursorButton::RightClick => GamepadButton::West,
            CursorButton::Back => GamepadButton::East,
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
    /// The on-screen stick while the game shows its cursor (#856): its
    /// reach in stick radii (+x right, +y down; zero when let go), at most
    /// 1 long. It moves the cursor as a gamepad's left stick does
    /// ([`Input::pad_cursor_frame`]).
    CursorStick {
        value: [f32; 2],
    },
    /// An on-screen cursor button pressed or released while the game shows
    /// its cursor (#856), as the gamepad's ([`CursorButton`]).
    CursorButton {
        button: CursorButton,
        pressed: bool,
    },
    /// The touches down when the cursor appeared are dropped (#108): the
    /// left button they hold is released where the cursor is, their
    /// contacts end.
    TouchDropped,
    /// Touches click where they land while the game shows its cursor
    /// (`on`; the ergonomic mode, #807): the cursor isn't
    /// drawn until a mouse moves it.
    TouchDirect {
        on: bool,
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
    /// The on-screen move stick's reach in stick radii (+x right, +y
    /// down): within its rim at most 1 long, past it up to
    /// [`STICK_REACH`] per axis; zero when released (the standard touch
    /// controls).
    TouchStick {
        value: [f32; 2],
    },
    /// The on-screen move stick's touch was cancelled (by the system, or
    /// the controls hiding): released, without acting where it was.
    TouchStickCancel,
    /// Where the page draws the move stick's sprint lock target for the
    /// current touch: its centre relative to the stick's, in stick radii
    /// (+y down).
    TouchLockTarget {
        centre: [f32; 2],
    },
    /// A look drag on the standard touch controls, in screen units, at
    /// `speed` screen units per second (the page's smoothed speed of that
    /// drag).
    TouchLookDelta {
        delta: [f32; 2],
        speed: f32,
    },
    /// A look finger on the experimental touch controls.
    TouchLook(TouchLook),
    TouchButton {
        button: TouchButton,
        pressed: bool,
    },
    /// Mouse-wheel steps: negative toward the user's "up" (content toward
    /// its start), positive "down".
    Wheel {
        steps: i32,
    },
    /// The pointer lock the page held (mouse look in a level) was lost.
    PointerUnlocked,
    /// A character typed through the platform's text entry (a device's
    /// own keyboard on touch screens), not a key press: a Volya Engine
    /// addition for the save screen's name field (#94).
    Text {
        ch: char,
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
    Touch(TouchButton),
}

impl Control {
    pub fn name(self) -> &'static str {
        match self {
            Control::Key(k) => k.name(),
            Control::Mouse(MouseButton::Left) => "mouse_left",
            Control::Mouse(MouseButton::Right) => "mouse_right",
            Control::Mouse(MouseButton::Middle) => "mouse_middle",
            Control::Mouse(b) => b.name(),
            Control::Pad(b) => b.name(),
            Control::Touch(TouchButton::Jump) => "touch_jump",
            Control::Touch(TouchButton::Crouch) => "touch_crouch",
            Control::Touch(TouchButton::CrouchLow) => "touch_crouch_low",
            Control::Touch(TouchButton::Menu) => "touch_menu",
            Control::Touch(TouchButton::Inventory) => "touch_inventory",
            Control::Touch(TouchButton::Torch) => "touch_torch",
            Control::Touch(TouchButton::NightVision) => "touch_night_vision",
            Control::Touch(TouchButton::Console) => "touch_console",
            Control::Touch(TouchButton::Pda) => "touch_pda",
            Control::Touch(TouchButton::Use) => "touch_use",
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
    /// One mouse-wheel step: *wheel up* or *wheel down*.
    Wheel {
        up: bool,
    },
}

/// Where a held control comes from; a control is held while any source
/// holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Keyboard,
    Mouse,
    Touch,
    /// The on-screen cursor buttons (#856).
    CursorPad,
    Pad,
    Stick,
}

/// Input state: the cursor, held controls and this frame's actions.
#[derive(Debug)]
pub struct Input {
    screen: [f32; 2],
    sensitivity: f32,
    /// Normalised; the centre when the program starts (§7), moved by input
    /// and by UI sequences ([`Input::place_cursor`]).
    cursor: [f32; 2],
    held: Vec<(Source, Control)>,
    actions: Vec<Action>,
    /// Time of the last single (non-double) press per mouse button.
    last_press: [Option<f64>; MouseButton::ALL.len()],
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
    /// Presses (true) and releases of the keyboard and mouse this frame.
    device_changes: Vec<(Control, bool)>,
    /// Gamepad buttons held, and their presses and releases this frame.
    pad_held: Vec<GamepadButton>,
    pad_changes: Vec<(GamepadButton, bool)>,
    touch_stick: [f32; 2],
    /// The touch stick's values this frame, in order.
    touch_stick_values: Vec<Option<[f32; 2]>>,
    touch_lock_target: Option<[f32; 2]>,
    /// Look drags this frame with their speeds.
    touch_look_moves: Vec<([f32; 2], f32)>,
    /// The look fingers' events this frame.
    touch_looks: Vec<TouchLook>,
    /// [`Input::release_all`] ran this frame.
    looks_released: bool,
    /// Touches click where they land ([`Event::TouchDirect`]), and whether
    /// a mouse has moved the cursor since the latest touch.
    touch_direct: bool,
    mouse_since_touch: bool,
    /// Relative mouse motion this frame ([`Event::PointerMotion`]).
    motion: [f32; 2],
    /// Whether the gamepad's left stick and the touch stick read zero only
    /// because of [`Input::release_all`], until their next value.
    sticks_released: [bool; 2],
    /// The pointer lock was lost this frame.
    pointer_unlocked: bool,
    /// The gamepad drives the game's cursor ([`Input::set_pad_cursor`]).
    pad_cursor: bool,
    /// The on-screen stick's reach for the cursor ([`Event::CursorStick`]).
    cursor_stick: [f32; 2],
    /// The on-screen Back button holds the gamepad's east.
    cursor_back: bool,
    /// The platform time of the latest [`Input::pad_cursor_frame`], ms: a
    /// gamepad's presses are timed by it for double clicks.
    pad_clock: Option<f64>,
    /// The d-pad's nudges (up, down, left, right) and the shoulders'
    /// wheel steps (left, right) repeating while held.
    nudges: [Repeat; 4],
    shoulders: [Repeat; 2],
    /// The right stick's push as the wheel (−1 up, +1 down, 0 none) and
    /// its fraction of a step not yet made.
    wheel_push: i32,
    wheel_due: f32,
    /// Characters typed through text entry this frame ([`Event::Text`]).
    text: Vec<char>,
    /// What the clipboard held at this frame's Insert presses, in order
    /// ([`Input::clipboard_read`]).
    clipboard: std::collections::VecDeque<Clipboard>,
}

impl Default for Input {
    fn default() -> Self {
        Input {
            screen: [UI_WIDTH, UI_HEIGHT],
            sensitivity: DEFAULT_SENSITIVITY,
            cursor: [0.0, 0.0],
            held: Vec::new(),
            actions: Vec::new(),
            last_press: [None; MouseButton::ALL.len()],
            count: 0,
            touch_starts: 0,
            pointer_at: None,
            pointer_delta: [0.0; 2],
            drag_delta: [0.0; 2],
            contacts: Vec::new(),
            axes: [0.0; 6],
            device_changes: Vec::new(),
            pad_held: Vec::new(),
            pad_changes: Vec::new(),
            touch_stick: [0.0; 2],
            touch_stick_values: Vec::new(),
            touch_lock_target: None,
            touch_look_moves: Vec::new(),
            touch_looks: Vec::new(),
            looks_released: false,
            touch_direct: false,
            mouse_since_touch: false,
            motion: [0.0; 2],
            sticks_released: [false; 2],
            pointer_unlocked: false,
            pad_cursor: false,
            cursor_stick: [0.0; 2],
            cursor_back: false,
            pad_clock: None,
            nudges: Default::default(),
            shoulders: Default::default(),
            wheel_push: 0,
            wheel_due: 0.0,
            text: Vec::new(),
            clipboard: Default::default(),
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

    /// Whether the game's cursor is drawn: not while touches click where
    /// they land ([`Event::TouchDirect`]), unless a mouse has moved it since
    /// the latest touch.
    pub fn cursor_drawn(&self) -> bool {
        !self.touch_direct || self.mouse_since_touch
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

    /// Puts the cursor at normalised `to` (clamped to −1 … +1), as a UI
    /// sequence's image item does when it starts (title-menu.md §7): not
    /// input, so no action is recorded.
    pub fn place_cursor(&mut self, [x, y]: [f32; 2]) {
        self.cursor = [clamp_unit(x), clamp_unit(y)];
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

    /// Whether the keyboard or the mouse holds `control` (not a gamepad or
    /// touch acting as it).
    pub fn device_held(&self, control: Control) -> bool {
        self.held
            .iter()
            .any(|&(s, c)| c == control && matches!(s, Source::Keyboard | Source::Mouse))
    }

    /// Presses (`true`) and releases of keys and mouse buttons by the
    /// keyboard and mouse this frame, in order.
    pub fn device_changes(&self) -> &[(Control, bool)] {
        &self.device_changes
    }

    /// Uses up the keyboard's or mouse's first press of `control` this
    /// frame: whoever reads the frame after sees no such press, while the
    /// control stays held and its release stays (a press a bound console
    /// line took, docs/behavior/console-key-bindings.md section 5, rule 5).
    pub fn use_up_press(&mut self, control: Control) {
        if let Some(at) = self
            .device_changes
            .iter()
            .position(|&c| c == (control, true))
        {
            self.device_changes.remove(at);
        }
        if let Some(at) = self
            .actions
            .iter()
            .position(|a| matches!(a, Action::Press { control: c, .. } if *c == control))
        {
            self.actions.remove(at);
        }
    }

    /// The keyboard's key presses (`true`) and releases this frame by key
    /// code, in order: the key events of docs/behavior/console-input.md
    /// section 1.
    pub fn key_events(&self) -> Vec<(u16, bool)> {
        self.device_changes
            .iter()
            .filter_map(|&(c, pressed)| match c {
                Control::Key(k) => Some((k.code(), pressed)),
                _ => None,
            })
            .collect()
    }

    /// The codes of the keyboard keys that are down, ascending: the hold
    /// reports of console-input.md section 1.
    pub fn keys_down(&self) -> Vec<u16> {
        let mut down: Vec<u16> = self
            .held
            .iter()
            .filter_map(|&(s, c)| match c {
                Control::Key(k) if s == Source::Keyboard => Some(k.code()),
                _ => None,
            })
            .collect();
        down.sort_unstable();
        down
    }

    /// Whether gamepad button `button` is held.
    pub fn pad_held(&self, button: GamepadButton) -> bool {
        self.pad_held.contains(&button)
    }

    /// Presses (`true`) and releases of gamepad buttons this frame.
    pub fn pad_changes(&self) -> &[(GamepadButton, bool)] {
        &self.pad_changes
    }

    /// The on-screen move stick's reach ([`Event::TouchStick`]).
    pub fn touch_stick(&self) -> [f32; 2] {
        self.touch_stick
    }

    /// The latest [`Event::TouchLockTarget`], if any.
    pub fn touch_lock_target(&self) -> Option<[f32; 2]> {
        self.touch_lock_target
    }

    /// This frame's look drags, each with its speed
    /// ([`Event::TouchLookDelta`]).
    pub fn touch_look_moves(&self) -> &[([f32; 2], f32)] {
        &self.touch_look_moves
    }

    /// Every value the touch stick sent this frame, in order: a touch, a
    /// let-go (zero) and where it was let go show here even within one
    /// frame; `None` is a cancelled touch ([`Event::TouchStickCancel`]).
    pub fn touch_stick_values(&self) -> &[Option<[f32; 2]>] {
        &self.touch_stick_values
    }

    /// This frame's look finger events, in order ([`Event::TouchLook`]).
    pub fn touch_looks(&self) -> &[TouchLook] {
        &self.touch_looks
    }

    /// Whether everything was released this frame
    /// ([`Input::release_all`]): every look finger has ended, flinging
    /// nothing.
    pub fn looks_released(&self) -> bool {
        self.looks_released
    }

    /// Mouse motion this frame, in screen units (absolute moves and relative
    /// motion alike).
    pub fn pointer_delta(&self) -> [f32; 2] {
        self.pointer_delta
    }

    /// Relative mouse motion this frame (under pointer lock), in screen
    /// units; absolute moves are not included.
    pub fn pointer_motion(&self) -> [f32; 2] {
        self.motion
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
        self.pointer_unlocked = false;
        self.text.clear();
        self.clipboard.clear();
        self.touch_starts = 0;
        self.pointer_delta = [0.0; 2];
        self.drag_delta = [0.0; 2];
        self.device_changes.clear();
        self.pad_changes.clear();
        self.touch_stick_values.clear();
        self.touch_look_moves.clear();
        self.touch_looks.clear();
        self.looks_released = false;
        self.motion = [0.0; 2];
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
        for button in std::mem::take(&mut self.pad_held) {
            self.pad_changes.push((button, false));
        }
        self.touch_stick = [0.0; 2];
        self.cursor_stick = [0.0; 2];
        self.cursor_back = false;
        self.sticks_released = [true; 2];
        self.looks_released = true;
    }

    /// Whether the gamepad's left stick and the touch stick read zero only
    /// because everything was released, not because they were let go: the
    /// platform sends a stick still held again.
    pub fn sticks_released(&self) -> [bool; 2] {
        self.sticks_released
    }

    /// Whether the pointer lock was lost this frame
    /// ([`Event::PointerUnlocked`]).
    pub fn pointer_unlocked(&self) -> bool {
        self.pointer_unlocked
    }

    /// The characters typed through text entry this frame
    /// ([`Event::Text`]), in order.
    pub fn text(&self) -> &[char] {
        &self.text
    }

    /// The platform read the clipboard for an Insert press it reports next
    /// (docs/behavior/console-input.md section 6): the console takes it at
    /// that press ([`Input::take_clipboard`]). Kept for this frame only.
    pub fn clipboard_read(&mut self, clipboard: Clipboard) {
        self.clipboard.push_back(clipboard);
    }

    /// The clipboard as read for this frame's next Insert press;
    /// [`Clipboard::Unavailable`] where the platform read none.
    pub fn take_clipboard(&mut self) -> Clipboard {
        self.clipboard.pop_front().unwrap_or(Clipboard::Unavailable)
    }

    pub fn handle(&mut self, event: Event) {
        match event {
            Event::PointerUnlocked => self.pointer_unlocked = true,
            Event::Text { ch } => self.text.push(ch),
            Event::PointerMoved { at } => {
                self.mouse_since_touch = true;
                self.pointer_to(at);
                self.move_to(at)
            }
            Event::PointerMotion { delta } => {
                self.mouse_since_touch = true;
                self.motion[0] += delta[0];
                self.motion[1] += delta[1];
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
                    self.mouse_since_touch = true;
                    self.pointer_to(at);
                    self.move_to(at);
                }
                self.mouse_button(Source::Mouse, button, pressed, time);
            }
            Event::CursorStick { value } => {
                let [x, y] = value.map(|v| if v.is_finite() { v } else { 0.0 });
                let d = x.hypot(y);
                self.cursor_stick = if d > 1.0 { [x / d, y / d] } else { [x, y] };
            }
            Event::CursorButton {
                button: CursorButton::Back,
                pressed,
            } => {
                // The gamepad's east itself, so each screen's back is the
                // gamepad's; a gamepad's own east held stays held.
                if pressed != self.cursor_back {
                    self.cursor_back = pressed;
                    self.handle(Event::GamepadButton {
                        button: GamepadButton::East,
                        pressed,
                    });
                }
            }
            Event::CursorButton { button, pressed } => {
                let Control::Mouse(b) = button.pad().cursor_control() else {
                    return;
                };
                if !pressed {
                    self.set(Source::CursorPad, Control::Mouse(b), false, false);
                } else if self.pad_cursor {
                    // Timed by the frames, as a gamepad's presses are.
                    let time = self.pad_clock.unwrap_or(0.0);
                    self.mouse_button(Source::CursorPad, b, true, time);
                }
            }
            Event::TouchDropped => {
                self.set(
                    Source::Touch,
                    Control::Mouse(MouseButton::Left),
                    false,
                    false,
                );
                for c in &mut self.contacts {
                    c.ended = true;
                }
            }
            Event::TouchDirect { on } => {
                self.touch_direct = on;
                self.mouse_since_touch = false;
            }
            Event::Touch { at, phase, time } => {
                self.mouse_since_touch = false;
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
                let held = self.pad_held.contains(&button);
                if pressed != held {
                    if pressed {
                        self.pad_held.push(button);
                    } else {
                        self.pad_held.retain(|&b| b != button);
                    }
                    self.pad_changes.push((button, pressed));
                }
                if pressed {
                    match (self.pad_cursor, button.cursor_control()) {
                        (true, Control::Mouse(b)) => {
                            let time = self.pad_clock.unwrap_or(0.0);
                            self.mouse_button(Source::Pad, b, true, time)
                        }
                        (true, c) => self.set(Source::Pad, c, true, false),
                        (false, _) => self.set(Source::Pad, button.control(), true, false),
                    }
                } else {
                    // Whichever it was pressed as: the mode may have
                    // changed while it was held.
                    self.set(Source::Pad, button.control(), false, false);
                    self.set(Source::Pad, button.cursor_control(), false, false);
                }
            }
            Event::TouchStick { value } => {
                self.touch_stick = value.map(|v| {
                    if v.is_nan() {
                        0.0
                    } else {
                        v.clamp(-STICK_REACH, STICK_REACH)
                    }
                });
                self.touch_stick_values.push(Some(self.touch_stick));
                self.sticks_released[1] = false;
            }
            Event::TouchStickCancel => {
                self.touch_stick = [0.0; 2];
                self.touch_stick_values.push(None);
                self.sticks_released[1] = false;
            }
            Event::TouchLockTarget { centre } => self.touch_lock_target = Some(centre),
            Event::TouchLookDelta { delta, speed } => self.touch_look_moves.push((delta, speed)),
            Event::TouchLook(look) => self.touch_looks.push(look),
            Event::TouchButton { button, pressed } => {
                self.set(Source::Touch, Control::Touch(button), pressed, false)
            }
            Event::Contact { id, at, phase } => self.contact(id, at, phase),
            Event::Wheel { steps } => {
                for _ in 0..steps.unsigned_abs().min(64) {
                    self.push(Action::Wheel { up: steps < 0 });
                }
            }
            Event::GamepadAxis { axis, value } => {
                self.axes[axis as usize] = value;
                if matches!(axis, GamepadAxis::LeftX | GamepadAxis::LeftY) {
                    self.sticks_released[0] = false;
                }
                let Some(keys) = axis.keys() else {
                    return;
                };
                let [negative, positive] = keys.map(Control::Key);
                for (control, deflection) in [(negative, -value), (positive, value)] {
                    let held = self.held.contains(&(Source::Stick, control));
                    if !held && deflection >= STICK_PRESS && !self.pad_cursor {
                        self.set(Source::Stick, control, true, false);
                    } else if held && deflection < STICK_RELEASE {
                        self.set(Source::Stick, control, false, false);
                    }
                }
            }
        }
    }

    /// Whether the gamepad drives the game's cursor (our design, #835).
    pub fn pad_cursor(&self) -> bool {
        self.pad_cursor
    }

    /// Makes the gamepad drive the game's cursor (`on`) wherever the game
    /// shows it, as a mouse does: [`GamepadButton::cursor_control`], the
    /// left stick moving the cursor and the d-pad nudging it, the right
    /// stick and the shoulders the wheel ([`Input::pad_cursor_frame`]).
    /// Off, the buttons are [`GamepadButton::control`] and the left stick
    /// the arrow keys. Buttons held across a change are released as they
    /// were pressed.
    pub fn set_pad_cursor(&mut self, on: bool) {
        if on != self.pad_cursor {
            self.pad_cursor = on;
            self.nudges = Default::default();
            self.shoulders = Default::default();
            self.wheel_push = 0;
            self.wheel_due = 0.0;
            if on {
                for key in [Key::Left, Key::Right, Key::Up, Key::Down] {
                    self.set(Source::Stick, Control::Key(key), false, false);
                }
            }
        }
    }

    /// The gamepad cursor's frame at platform time `time_ms`, before the
    /// screens read the frame (our design, #835): the left stick, or the
    /// on-screen stick where it is pushed further ([`Event::CursorStick`],
    /// #856), moves the cursor by [`pad_cursor_motion`], slowed over any of `targets` (the
    /// clickable elements, UI rectangles x, y, width, height); each d-pad
    /// press nudges it [`PAD_NUDGE`] units, repeating while held; the
    /// shoulders are the wheel (left up, right down), repeating while
    /// held, and the right stick too (pushed up, wheel up) when
    /// `right_stick_scrolls`. Its motion is the cursor's as a mouse's is,
    /// and shows the cursor where touches hid it ([`Input::cursor_drawn`]).
    /// Nothing while the gamepad doesn't drive the cursor.
    pub fn pad_cursor_frame(
        &mut self,
        time_ms: f64,
        targets: &[[f32; 4]],
        right_stick_scrolls: bool,
    ) {
        let dt = match self.pad_clock {
            Some(last) if time_ms > last => ((time_ms - last) / 1000.0).min(0.1) as f32,
            _ => 0.0,
        };
        self.pad_clock = Some(time_ms);
        if !self.pad_cursor {
            return;
        }
        let [mut x, mut y] = self.cursor_ui();
        let pad = [self.axis(GamepadAxis::LeftX), self.axis(GamepadAxis::LeftY)];
        let length = |[x, y]: [f32; 2]| {
            if x.is_finite() && y.is_finite() {
                x.hypot(y)
            } else {
                0.0
            }
        };
        let stick = if length(self.cursor_stick) > length(pad) {
            self.cursor_stick
        } else {
            pad
        };
        let [dx, dy] = pad_cursor_motion(stick, dt, over_target([x, y], targets));
        x += dx;
        y += dy;
        let pressed = |s: &Self, b| s.pad_changes.contains(&(b, true));
        let dirs = [
            (GamepadButton::DpadUp, [0.0, -1.0]),
            (GamepadButton::DpadDown, [0.0, 1.0]),
            (GamepadButton::DpadLeft, [-1.0, 0.0]),
            (GamepadButton::DpadRight, [1.0, 0.0]),
        ];
        for (i, (b, [ux, uy])) in dirs.into_iter().enumerate() {
            let (p, h) = (pressed(self, b), self.pad_held(b));
            let n = self.nudges[i].frame(p, h, dt, PAD_REPEAT_INTERVAL) as f32;
            x += ux * PAD_NUDGE * n;
            y += uy * PAD_NUDGE * n;
        }
        let to = [
            clamp_unit(x * 2.0 / UI_WIDTH - 1.0),
            clamp_unit(y * 2.0 / UI_HEIGHT - 1.0),
        ];
        if to != self.cursor {
            self.mouse_since_touch = true;
            self.set_cursor(to);
        }
        let mut steps = 0i32;
        for (i, (b, sign)) in [
            (GamepadButton::LeftShoulder, -1),
            (GamepadButton::RightShoulder, 1),
        ]
        .into_iter()
        .enumerate()
        {
            let (p, h) = (pressed(self, b), self.pad_held(b));
            steps += sign * self.shoulders[i].frame(p, h, dt, PAD_WHEEL_INTERVAL) as i32;
        }
        let rate = if right_stick_scrolls {
            pad_wheel_rate(self.axis(GamepadAxis::RightY))
        } else {
            0.0
        };
        let sign = rate.signum() as i32;
        if rate == 0.0 {
            self.wheel_push = 0;
        } else if sign != self.wheel_push {
            // A new push (or one turned back) steps at once.
            self.wheel_push = sign;
            self.wheel_due = 0.0;
            steps += sign;
        } else {
            self.wheel_due += rate.abs() * dt;
            let whole = self.wheel_due.floor();
            self.wheel_due -= whole;
            steps += sign * whole as i32;
        }
        for _ in 0..steps.unsigned_abs().min(64) {
            self.push(Action::Wheel { up: steps < 0 });
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
            _ => return,
        }
        if matches!(source, Source::Keyboard | Source::Mouse) {
            self.device_changes.push((control, pressed));
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
    fn every_key_has_its_code() {
        for &key in Key::ALL {
            assert_ne!(key.code(), 0, "{key:?}");
        }
        assert_eq!(Key::Grave.code(), 41);
        assert_eq!(Key::Insert.code(), 210);
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
    fn the_pad_cursor_speed_has_a_dead_zone_and_is_linear_past_it() {
        assert_eq!(pad_cursor_speed(0.0), 0.0);
        assert_eq!(pad_cursor_speed(PAD_CURSOR_DEAD_ZONE), 0.0);
        assert_eq!(pad_cursor_speed(1.0), PAD_CURSOR_MAX_SPEED);
        assert_eq!(pad_cursor_speed(2.0), PAD_CURSOR_MAX_SPEED);
        assert_eq!(pad_cursor_speed(f32::NAN), 0.0);
        // A fifth of the way out: a fifth of the speed.
        let fifth = PAD_CURSOR_DEAD_ZONE + 0.2 * (1.0 - PAD_CURSOR_DEAD_ZONE);
        assert!((pad_cursor_speed(fifth) - 100.0).abs() < 0.01);
        let mut last = 0.0;
        for i in 0..=100 {
            let v = pad_cursor_speed(i as f32 / 100.0);
            assert!(v >= last);
            last = v;
        }
        // Radial: the direction kept, the length by the curve; friction
        // slows it over a target.
        let [x, y] = pad_cursor_motion([0.6, 0.8], 0.5, false);
        assert!((x - 150.0).abs() < 0.01 && (y - 200.0).abs() < 0.01);
        let [x, y] = pad_cursor_motion([0.6, 0.8], 0.5, true);
        assert!((x - 150.0 * PAD_CURSOR_FRICTION).abs() < 0.01);
        assert!((y - 200.0 * PAD_CURSOR_FRICTION).abs() < 0.01);
        assert_eq!(pad_cursor_motion([0.1, 0.1], 0.5, false), [0.0, 0.0]);
        assert_eq!(pad_cursor_motion([f32::NAN, 0.0], 0.5, false), [0.0, 0.0]);
        assert_eq!(pad_cursor_motion([1.0, 0.0], 0.0, false), [0.0, 0.0]);
        assert_eq!(pad_wheel_rate(0.2), 0.0);
        assert_eq!(pad_wheel_rate(-1.0), -PAD_WHEEL_MAX_RATE);
    }

    fn pad(input: &mut Input, button: GamepadButton, pressed: bool) {
        input.handle(Event::GamepadButton { button, pressed });
    }

    fn axis(input: &mut Input, axis: GamepadAxis, value: f32) {
        input.handle(Event::GamepadAxis { axis, value });
    }

    #[test]
    fn the_pad_cursor_moves_like_a_mouse_slowed_over_targets() {
        let mut input = Input::new();
        input.set_screen(SCREEN);
        input.set_pad_cursor(true);
        input.pad_cursor_frame(0.0, &[], true);
        assert_eq!(input.cursor_ui(), [512.0, 384.0]);
        // Full tilt right for 100 ms: 50 units.
        axis(&mut input, GamepadAxis::LeftX, 1.0);
        input.pad_cursor_frame(100.0, &[], true);
        assert!((input.cursor_ui()[0] - 562.0).abs() < 0.01);
        assert!(matches!(input.actions(), [Action::Cursor { .. }]));
        input.end_frame();
        // Over a target: 20.
        let target = [550.0, 300.0, 200.0, 200.0];
        input.pad_cursor_frame(200.0, &[target], true);
        assert!((input.cursor_ui()[0] - 582.0).abs() < 0.01);
        input.end_frame();
        // A stalled frame counts 100 ms at most; the screen clamps.
        input.pad_cursor_frame(5000.0, &[], true);
        assert!((input.cursor_ui()[0] - 632.0).abs() < 0.01);
        for t in 0..40 {
            input.pad_cursor_frame(5100.0 + 100.0 * t as f64, &[], true);
        }
        assert_eq!(input.cursor_ui()[0], UI_WIDTH);
        // The stick presses no arrow key here.
        assert!(!input.is_held(Control::Key(Key::Right)));
        axis(&mut input, GamepadAxis::LeftX, 0.0);
        input.end_frame();
        // Off: nothing moves; the stick is the arrows again.
        input.set_pad_cursor(false);
        axis(&mut input, GamepadAxis::LeftX, -1.0);
        input.pad_cursor_frame(8000.0, &[], true);
        assert_eq!(input.cursor_ui()[0], UI_WIDTH);
        assert!(input.is_held(Control::Key(Key::Left)));
        // Back on: the stick's arrow is let go.
        input.set_pad_cursor(true);
        assert!(!input.is_held(Control::Key(Key::Left)));
    }

    #[test]
    fn the_pad_cursor_shows_the_cursor_touches_hid() {
        let mut input = Input::new();
        input.handle(Event::TouchDirect { on: true });
        assert!(!input.cursor_drawn());
        input.set_pad_cursor(true);
        input.pad_cursor_frame(0.0, &[], true);
        axis(&mut input, GamepadAxis::LeftY, 1.0);
        input.pad_cursor_frame(50.0, &[], true);
        assert!(input.cursor_drawn());
    }

    #[test]
    fn pad_buttons_are_mouse_buttons_with_the_cursor_and_keys_without() {
        let mut input = Input::new();
        input.set_pad_cursor(true);
        input.pad_cursor_frame(1000.0, &[], true);
        pad(&mut input, GamepadButton::South, true);
        assert!(input.is_held(Control::Mouse(MouseButton::Left)));
        assert!(!input.device_held(Control::Mouse(MouseButton::Left)));
        pad(&mut input, GamepadButton::South, false);
        // A second press within 250 ms is a double click.
        input.pad_cursor_frame(1100.0, &[], true);
        pad(&mut input, GamepadButton::South, true);
        assert_eq!(
            input.actions(),
            [
                press(Control::Mouse(MouseButton::Left)),
                Action::Release {
                    control: Control::Mouse(MouseButton::Left)
                },
                Action::Press {
                    control: Control::Mouse(MouseButton::Left),
                    double: true
                },
            ]
        );
        pad(&mut input, GamepadButton::South, false);
        input.end_frame();
        pad(&mut input, GamepadButton::West, true);
        pad(&mut input, GamepadButton::East, true);
        pad(&mut input, GamepadButton::DpadUp, true);
        assert!(input.is_held(Control::Mouse(MouseButton::Right)));
        assert!(input.is_held(Control::Key(Key::Escape)));
        assert!(!input.is_held(Control::Key(Key::Up)));
        // Held across the change, released as pressed.
        input.set_pad_cursor(false);
        pad(&mut input, GamepadButton::West, false);
        pad(&mut input, GamepadButton::East, false);
        assert!(!input.is_held(Control::Mouse(MouseButton::Right)));
        assert!(!input.is_held(Control::Key(Key::Escape)));
        pad(&mut input, GamepadButton::South, true);
        assert!(input.is_held(Control::Key(Key::Enter)));
        input.set_pad_cursor(true);
        pad(&mut input, GamepadButton::South, false);
        assert!(!input.is_held(Control::Key(Key::Enter)));
    }

    #[test]
    fn the_d_pad_nudges_and_the_shoulders_and_right_stick_scroll() {
        let mut input = Input::new();
        input.set_pad_cursor(true);
        input.pad_cursor_frame(0.0, &[], true);
        pad(&mut input, GamepadButton::DpadRight, true);
        input.pad_cursor_frame(10.0, &[], true);
        assert_eq!(input.cursor_ui(), [512.0 + PAD_NUDGE, 384.0]);
        input.end_frame();
        // Held: repeats from 400 ms, every 50 ms (three by 520 ms).
        let mut t = 10.0;
        while t < 10.0 + 520.0 {
            t += 10.0;
            input.pad_cursor_frame(t, &[], true);
            input.end_frame();
        }
        let x = input.cursor_ui()[0];
        assert!((x - (512.0 + 4.0 * PAD_NUDGE)).abs() < 0.01, "{x}");
        pad(&mut input, GamepadButton::DpadRight, false);
        input.end_frame();
        let wheels = |input: &Input| {
            (input.actions().iter())
                .filter_map(|a| match a {
                    Action::Wheel { up } => Some(*up),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        pad(&mut input, GamepadButton::RightShoulder, true);
        input.pad_cursor_frame(t + 10.0, &[], true);
        assert_eq!(wheels(&input), [false]);
        pad(&mut input, GamepadButton::RightShoulder, false);
        input.end_frame();
        pad(&mut input, GamepadButton::LeftShoulder, true);
        input.pad_cursor_frame(t + 20.0, &[], true);
        assert_eq!(wheels(&input), [true]);
        pad(&mut input, GamepadButton::LeftShoulder, false);
        input.end_frame();
        // The right stick: a step at once, then at its rate (12 a second
        // at full tilt).
        axis(&mut input, GamepadAxis::RightY, -1.0);
        input.pad_cursor_frame(t + 30.0, &[], true);
        assert_eq!(wheels(&input), [true]);
        input.end_frame();
        let mut n = 0;
        for i in 1..=100 {
            input.pad_cursor_frame(t + 30.0 + 10.0 * i as f64, &[], true);
            n += wheels(&input).len();
            input.end_frame();
        }
        assert_eq!(n, 12);
        // Not where the screen takes the right stick itself.
        input.pad_cursor_frame(t + 2000.0, &[], false);
        assert!(wheels(&input).is_empty());
    }

    #[test]
    fn the_on_screen_stick_and_buttons_drive_the_cursor_as_a_gamepad_does() {
        let mut input = Input::new();
        input.set_screen(SCREEN);
        input.set_pad_cursor(true);
        input.pad_cursor_frame(0.0, &[], true);
        let start = input.cursor_ui();
        // Past its rim it reaches no further than a gamepad's full tilt.
        input.handle(Event::CursorStick { value: [3.0, 0.0] });
        input.pad_cursor_frame(100.0, &[], true);
        let moved = input.cursor_ui()[0] - start[0];
        assert!((moved - PAD_CURSOR_MAX_SPEED * 0.1).abs() < 0.5, "{moved}");
        // The same curve and friction as the gamepad's left stick.
        let target = [0.0, 0.0, UI_WIDTH, UI_HEIGHT];
        input.handle(Event::CursorStick { value: [0.0, 0.6] });
        let before = input.cursor_ui();
        input.pad_cursor_frame(200.0, &[target], true);
        let [_, dy] = pad_cursor_motion([0.0, 0.6], 0.1, true);
        assert!((input.cursor_ui()[1] - before[1] - dy).abs() < 0.01);
        // Within the dead zone, or let go, nothing.
        input.handle(Event::CursorStick { value: [0.1, 0.1] });
        let at = input.cursor_ui();
        input.pad_cursor_frame(300.0, &[], true);
        assert_eq!(input.cursor_ui(), at);
        input.handle(Event::CursorStick {
            value: [f32::NAN, 0.0],
        });
        input.pad_cursor_frame(400.0, &[], true);
        assert_eq!(input.cursor_ui(), at);
        input.end_frame();
        // Click is the left button at the cursor, a second press within the
        // double-click time a double click, timed by the frames.
        let left = Control::Mouse(MouseButton::Left);
        let click = |input: &mut Input, pressed| {
            input.handle(Event::CursorButton {
                button: CursorButton::Click,
                pressed,
            })
        };
        click(&mut input, true);
        click(&mut input, false);
        input.pad_cursor_frame(500.0, &[], true);
        click(&mut input, true);
        click(&mut input, false);
        input.pad_cursor_frame(900.0, &[], true);
        click(&mut input, true);
        assert_eq!(input.cursor_ui(), at);
        assert_eq!(
            input.actions(),
            [
                press(left),
                Action::Release { control: left },
                Action::Press {
                    control: left,
                    double: true
                },
                Action::Release { control: left },
                press(left),
            ]
        );
        // Held, it drags: the cursor moves with the button down.
        input.handle(Event::CursorStick { value: [1.0, 0.0] });
        input.pad_cursor_frame(1000.0, &[], true);
        assert!(input.is_held(left) && input.cursor_ui()[0] > at[0]);
        click(&mut input, false);
        assert!(!input.is_held(left));
        // Not the mouse's own: the level's controls don't see it.
        assert!(input.device_changes().is_empty());
        input.end_frame();
        // Right-click is the right button.
        input.handle(Event::CursorButton {
            button: CursorButton::RightClick,
            pressed: true,
        });
        assert!(input.is_held(Control::Mouse(MouseButton::Right)));
        input.release_all();
        assert!(input.held().is_empty());
        input.end_frame();
        // Back is the gamepad's east: Escape, and east among the gamepad's
        // presses, which the screens read as their back.
        for pressed in [true, false] {
            input.handle(Event::CursorButton {
                button: CursorButton::Back,
                pressed,
            });
        }
        assert_eq!(
            input.pad_changes(),
            [(GamepadButton::East, true), (GamepadButton::East, false)]
        );
        let escape = Control::Key(Key::Escape);
        assert_eq!(
            input.actions(),
            [press(escape), Action::Release { control: escape }]
        );
        input.end_frame();
        // Off (the on-screen keyboard takes the gamepad), the stick and the
        // buttons do nothing; a release still releases.
        click(&mut input, true);
        input.set_pad_cursor(false);
        click(&mut input, false);
        assert!(input.held().is_empty());
        click(&mut input, true);
        assert!(input.held().is_empty());
        input.handle(Event::CursorStick { value: [1.0, 0.0] });
        let at = input.cursor_ui();
        input.pad_cursor_frame(1100.0, &[], true);
        assert_eq!(input.cursor_ui(), at);
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
    fn direct_touch_hides_the_cursor_until_a_mouse_moves_it() {
        let mut input = Input::new();
        assert!(input.cursor_drawn());
        input.handle(Event::TouchDirect { on: true });
        assert!(!input.cursor_drawn(), "before any touch too");
        input.handle(Event::PointerMoved { at: [10.0, 10.0] });
        assert!(input.cursor_drawn(), "a mouse shows it");
        input.handle(Event::Touch {
            at: [20.0, 20.0],
            phase: TouchPhase::Start,
            time: 0.0,
        });
        assert!(!input.cursor_drawn(), "a touch hides it again");
        assert!(input.is_held(Control::Mouse(MouseButton::Left)));
        input.handle(Event::PointerMotion { delta: [1.0, 0.0] });
        assert!(input.cursor_drawn());
        input.handle(Event::TouchDirect { on: false });
        input.handle(Event::Touch {
            at: [20.0, 20.0],
            phase: TouchPhase::End,
            time: 10.0,
        });
        assert!(input.cursor_drawn(), "drawn outside the direct mode");
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
    fn a_lost_pointer_lock_shows_for_one_frame() {
        let mut input = Input::new();
        assert!(!input.pointer_unlocked());
        input.handle(Event::PointerUnlocked);
        assert!(input.pointer_unlocked());
        input.end_frame();
        assert!(!input.pointer_unlocked());
    }

    #[test]
    fn a_used_up_press_stays_held_and_keeps_its_release() {
        let mut input = Input::new();
        let w = Control::Key(Key::W);
        for pressed in [true, false, true] {
            input.handle(Event::Key {
                key: Key::W,
                pressed,
            });
        }
        input.use_up_press(w);
        assert_eq!(input.device_changes(), [(w, false), (w, true)]);
        assert_eq!(
            input
                .actions()
                .iter()
                .filter(|a| matches!(a, Action::Press { .. }))
                .count(),
            1
        );
        assert!(input.is_held(w));
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
        for &key in Key::ALL {
            assert_eq!(Key::from_name(key.name()), Some(key));
            assert_eq!(Key::from_dik_name(key.dik_name()), Some(key));
        }
        assert_eq!(Key::from_dik_name("klcontrol"), Some(Key::LeftControl));
        for button in MouseButton::ALL {
            assert_eq!(MouseButton::from_name(button.name()), Some(button));
            assert_eq!(MouseButton::from_key_name(button.key_name()), Some(button));
        }
        assert_eq!(
            MouseButton::from_key_name("MOUSE2"),
            Some(MouseButton::Right)
        );
        assert_eq!(
            MouseButton::from_key_name("mouse8"),
            Some(MouseButton::Mouse8)
        );
        for button in GamepadButton::ALL {
            assert_eq!(GamepadButton::from_name(button.name()), Some(button));
        }
        for axis in GamepadAxis::ALL {
            assert_eq!(GamepadAxis::from_name(axis.name()), Some(axis));
        }
        assert_eq!(Key::from_name("space"), Some(Key::Space));
        assert_eq!(Key::from_name("tab"), Some(Key::Tab));
        assert_eq!(Key::from_name("kTAB"), None);
    }

    #[test]
    fn raw_state_for_the_level_controls() {
        let mut input = Input::new();
        // Mouse motion accumulates per frame.
        input.handle(Event::PointerMoved { at: [10.0, 10.0] });
        input.handle(Event::PointerMoved { at: [15.0, 8.0] });
        input.handle(Event::PointerMotion { delta: [1.0, 1.0] });
        assert_eq!(input.pointer_delta(), [6.0, -1.0]);
        assert_eq!(input.pointer_motion(), [1.0, 1.0]);
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
        assert_eq!(
            input.pad_changes().last(),
            Some(&(GamepadButton::LeftShoulder, false))
        );
    }

    #[test]
    fn device_pad_and_touch_controls_are_told_apart() {
        let mut input = Input::new();
        let space = Control::Key(Key::Space);
        input.handle(Event::Key {
            key: Key::Space,
            pressed: true,
        });
        input.handle(Event::GamepadButton {
            button: GamepadButton::South,
            pressed: true,
        });
        input.handle(Event::GamepadButton {
            button: GamepadButton::South,
            pressed: true,
        });
        input.handle(Event::TouchButton {
            button: TouchButton::Jump,
            pressed: true,
        });
        assert!(input.device_held(space));
        // South holds Enter for the menu, and itself.
        assert!(!input.device_held(Control::Key(Key::Enter)));
        assert!(input.pad_held(GamepadButton::South));
        assert_eq!(input.device_changes(), [(space, true)]);
        // The console's key events and hold reports: the keyboard's only.
        assert_eq!(input.key_events(), [(57, true)]);
        assert_eq!(input.keys_down(), [57]);
        assert_eq!(input.pad_changes(), [(GamepadButton::South, true)]);
        assert!(input.is_held(Control::Touch(TouchButton::Jump)));
        input.handle(Event::TouchStick { value: [0.5, -2.0] });
        input.handle(Event::TouchStick { value: [0.5, -9.0] });
        input.handle(Event::TouchLookDelta {
            delta: [3.0, 1.0],
            speed: 100.0,
        });
        input.handle(Event::TouchLookDelta {
            delta: [1.0, 1.0],
            speed: 200.0,
        });
        assert_eq!(
            input.touch_look_moves(),
            [([3.0, 1.0], 100.0), ([1.0, 1.0], 200.0)]
        );
        let look = |phase, x: f32| TouchLook {
            finger: 3,
            phase,
            at: [x, 0.5],
            time: f64::from(x),
            half_height: 220.0,
        };
        input.handle(Event::TouchLook(look(LookPhase::Down, 1.0)));
        input.handle(Event::TouchLook(look(LookPhase::Move, 2.0)));
        assert_eq!(
            input.touch_looks(),
            [look(LookPhase::Down, 1.0), look(LookPhase::Move, 2.0)]
        );
        input.handle(Event::TouchStickCancel);
        assert_eq!(input.touch_stick(), [0.0, 0.0]);
        input.handle(Event::TouchStick { value: [0.5, -9.0] });
        assert_eq!(input.touch_stick(), [0.5, -STICK_REACH]);
        assert_eq!(
            input.touch_stick_values(),
            [
                Some([0.5, -2.0]),
                Some([0.5, -STICK_REACH]),
                None,
                Some([0.5, -STICK_REACH])
            ]
        );
        input.end_frame();
        assert!(input.device_changes().is_empty() && input.pad_changes().is_empty());
        assert_eq!(input.touch_stick(), [0.5, -STICK_REACH]);
        assert!(
            input.touch_stick_values().is_empty()
                && input.touch_look_moves().is_empty()
                && input.touch_looks().is_empty()
        );

        input.release_all();
        assert!(input.looks_released() && input.sticks_released() == [true; 2]);
        assert_eq!(input.touch_stick(), [0.0, 0.0]);
        assert!(!input.pad_held(GamepadButton::South));
        assert_eq!(input.device_changes(), [(space, false)]);
        for button in TouchButton::ALL {
            assert_eq!(TouchButton::from_name(button.name()), Some(button));
        }
    }

    #[test]
    fn typed_text_lasts_its_frame_and_presses_nothing() {
        let mut input = Input::new();
        for ch in "Ab".chars() {
            input.handle(Event::Text { ch });
        }
        assert_eq!(input.text(), ['A', 'b']);
        assert!(input.actions().is_empty() && input.device_changes().is_empty());
        input.end_frame();
        assert!(input.text().is_empty());
    }
}
