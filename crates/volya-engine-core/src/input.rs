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
//! - **Touch as a touchpad** wherever the game shows its cursor (a Volya
//!   Engine addition, decided by the user, #108): the platform recognises
//!   the gestures (swipe, tap, double-tap-and-hold, long press, two-finger
//!   swipe) and sends relative motion ([`Event::TouchpadMotion`], through
//!   [`touchpad_gain`] and then the sensitivity rule of §7), button presses
//!   at the cursor ([`Event::TouchpadButton`]) and wheel steps. A touch
//!   never clicks where it lands there.
//! - **Touch** elsewhere (a primary touch pointer, [`Event::Touch`]): the
//!   contact point moves the cursor there, then presses the left mouse
//!   button; lifting releases it. Screens that react to keys only (the logo
//!   sequence, skipped with the quit key) treat a tap as that key
//!   ([`Input::touch_starts`]).
//! - **Gamepad** ([`GamepadButton`], [`GamepadAxis`]): south (A / cross) is
//!   Enter, east (B / circle) is Escape, the d-pad is the arrow keys, and the
//!   left stick acts as the arrow keys once pushed past [`STICK_PRESS`]
//!   (released below [`STICK_RELEASE`]). The stick does not move the cursor.
//! - **Absolute pointers** (a mouse over the page without pointer lock, and
//!   [`Event::Touch`]): the cursor is placed under the pointer, inverting the screen
//!   mapping, so the drawn cursor and the system pointer agree. The
//!   sensitivity rule of §7 applies to relative motion only
//!   ([`Event::PointerMotion`], e.g. under pointer lock, and
//!   [`Event::TouchpadMotion`]).
//! - Keyboard auto-repeat is not a key press: platforms report physical
//!   presses only.
//!
//! Beyond the menu, the player's controls in a level read raw state the
//! menu ignores: keys and mouse buttons of the keyboard and mouse alone
//! ([`Input::device_held`], [`Input::device_changes`]), mouse motion
//! ([`Input::pointer_delta`], relative under pointer lock), gamepad buttons
//! as themselves ([`Input::pad_held`], [`Input::pad_changes`]) and analog
//! axes ([`Input::axis`]: both sticks and the triggers), and the page's
//! on-screen touch controls ([`Event::TouchLook`],
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

/// Below this speed of a touchpad swipe (screen units per second) the
/// cursor follows the finger one to one before the sensitivity rule (§7).
/// Our design (#108).
pub const TOUCHPAD_PRECISE_SPEED: f32 = 200.0;
/// From this speed on a swipe moves the cursor [`TOUCHPAD_MAX_GAIN`] times
/// as far.
pub const TOUCHPAD_FAST_SPEED: f32 = 1200.0;
pub const TOUCHPAD_MAX_GAIN: f32 = 2.5;

/// The touchpad's acceleration (our design, #108): the gain on a swipe's
/// motion at `speed` (the platform's smoothed speed of that swipe): 1 up
/// to [`TOUCHPAD_PRECISE_SPEED`], precise when slow; rising linearly to
/// [`TOUCHPAD_MAX_GAIN`] at [`TOUCHPAD_FAST_SPEED`], so a quick swipe
/// crosses the screen; level beyond.
pub fn touchpad_gain(speed: f32) -> f32 {
    let t = (speed - TOUCHPAD_PRECISE_SPEED) / (TOUCHPAD_FAST_SPEED - TOUCHPAD_PRECISE_SPEED);
    if t.is_nan() {
        return 1.0;
    }
    1.0 + t.clamp(0.0, 1.0) * (TOUCHPAD_MAX_GAIN - 1.0)
}

fn clamp_unit(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(-1.0, 1.0) }
}

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
}

impl TouchButton {
    pub const ALL: [TouchButton; 9] = [
        TouchButton::Jump,
        TouchButton::Crouch,
        TouchButton::CrouchLow,
        TouchButton::Menu,
        TouchButton::Inventory,
        TouchButton::Torch,
        TouchButton::NightVision,
        TouchButton::Console,
        TouchButton::Pda,
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
        }
    }

    pub fn from_name(name: &str) -> Option<TouchButton> {
        TouchButton::ALL.into_iter().find(|b| b.name() == name)
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
    /// Touchpad motion of the cursor (#108): a swipe's motion in screen
    /// units at `speed` screen units per second (the platform's smoothed
    /// speed of that swipe).
    TouchpadMotion {
        delta: [f32; 2],
        speed: f32,
    },
    /// A touchpad gesture pressed or released a mouse button at the cursor
    /// (#108) at `time` ms. The platform's recognizer decides a double
    /// click (`double`, for a press): a drag's press never is one.
    TouchpadButton {
        button: MouseButton,
        pressed: bool,
        double: bool,
        time: f64,
    },
    /// The touches down when the cursor appeared are dropped (#108): the
    /// left button they hold is released where the cursor is, their
    /// contacts end.
    TouchDropped,
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
    /// A look finger on the on-screen controls.
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
    Touchpad,
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
    /// The look fingers' events this frame.
    touch_looks: Vec<TouchLook>,
    /// [`Input::release_all`] ran this frame.
    looks_released: bool,
    /// Relative mouse motion this frame ([`Event::PointerMotion`]).
    motion: [f32; 2],
    /// Whether the gamepad's left stick reads zero only because of
    /// [`Input::release_all`], until its next value.
    stick_released: bool,
    /// The pointer lock was lost this frame.
    pointer_unlocked: bool,
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
            touch_looks: Vec::new(),
            looks_released: false,
            motion: [0.0; 2],
            stick_released: false,
            pointer_unlocked: false,
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
        self.stick_released = true;
        self.looks_released = true;
    }

    /// Whether the gamepad's left stick reads zero only because everything
    /// was released, not because it was let go: the platform sends a stick
    /// still held again.
    pub fn stick_released(&self) -> bool {
        self.stick_released
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
                self.pointer_to(at);
                self.move_to(at)
            }
            Event::PointerMotion { delta } => {
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
                    self.pointer_to(at);
                    self.move_to(at);
                }
                self.mouse_button(Source::Mouse, button, pressed, time);
            }
            Event::TouchpadMotion { delta, speed } => {
                let gain = touchpad_gain(speed);
                let delta = delta.map(|d| if d.is_finite() { d * gain } else { 0.0 });
                let to = apply_motion(self.cursor, delta, self.screen, self.sensitivity);
                self.set_cursor(to);
            }
            Event::TouchpadButton {
                button,
                pressed,
                double,
                time: _,
            } => {
                // The recognizer's double click, not the press times'.
                self.last_press[button as usize] = None;
                self.set(
                    Source::Touchpad,
                    Control::Mouse(button),
                    pressed,
                    double && pressed,
                )
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
                let held = self.pad_held.contains(&button);
                if pressed != held {
                    if pressed {
                        self.pad_held.push(button);
                    } else {
                        self.pad_held.retain(|&b| b != button);
                    }
                    self.pad_changes.push((button, pressed));
                }
                self.set(Source::Pad, button.control(), pressed, false)
            }
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
                    self.stick_released = false;
                }
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
    fn the_touchpad_curve_is_one_to_one_when_slow_and_rises_when_fast() {
        assert_eq!(touchpad_gain(0.0), 1.0);
        assert_eq!(touchpad_gain(TOUCHPAD_PRECISE_SPEED), 1.0);
        assert!((touchpad_gain(700.0) - 1.75).abs() < 1e-6);
        assert_eq!(touchpad_gain(TOUCHPAD_FAST_SPEED), TOUCHPAD_MAX_GAIN);
        assert_eq!(touchpad_gain(1e9), TOUCHPAD_MAX_GAIN);
        assert_eq!(touchpad_gain(f32::NAN), 1.0);
        assert_eq!(touchpad_gain(-5.0), 1.0);
        let mut last = 1.0;
        for speed in (0..3000).step_by(10) {
            let g = touchpad_gain(speed as f32);
            assert!(g >= last && (1.0..=TOUCHPAD_MAX_GAIN).contains(&g));
            last = g;
        }
    }

    #[test]
    fn touchpad_motion_follows_section_7_after_the_curve_and_clicks_at_the_cursor() {
        let mut input = Input::new();
        input.set_screen(SCREEN);
        // Slow: as relative mouse motion (§7: Δ · s / W).
        input.handle(Event::TouchpadMotion {
            delta: [192.0, 108.0],
            speed: 100.0,
        });
        assert_eq!(input.cursor(), [0.15, 0.15]);
        // Fast: the same swipe moves 2.5 times as far.
        input.handle(Event::TouchpadMotion {
            delta: [-192.0, -108.0],
            speed: 5000.0,
        });
        assert!((input.cursor()[0] - (0.15 - 0.375)).abs() < 1e-6);
        // The sensitivity applies; the screen clamps.
        input.set_sensitivity(1.0);
        input.handle(Event::TouchpadMotion {
            delta: [1e6, f32::NAN],
            speed: 100.0,
        });
        assert_eq!(input.cursor()[0], 1.0);
        input.end_frame();
        let at = input.cursor();
        // A click at the cursor: the left button, double as the recognizer
        // says, whatever the times (a drag's quick press is single).
        for (pressed, double, time) in [
            (true, false, 0.0),
            (false, false, 0.0),
            (true, true, 900.0),
            (false, true, 900.0),
            (true, false, 950.0),
            (false, false, 950.0),
        ] {
            input.handle(Event::TouchpadButton {
                button: MouseButton::Left,
                pressed,
                double,
                time,
            });
        }
        assert_eq!(input.cursor(), at);
        let left = Control::Mouse(MouseButton::Left);
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
                Action::Release { control: left },
            ]
        );
        // Not the mouse's own: the level's controls don't see it.
        assert!(input.device_changes().is_empty());
        input.handle(Event::TouchpadButton {
            button: MouseButton::Right,
            pressed: true,
            double: false,
            time: 500.0,
        });
        input.release_all();
        assert!(input.held().is_empty());
        // Held apart from direct touch: dropping the touches down when the
        // cursor appeared releases theirs only, where the cursor is.
        input.handle(Event::Touch {
            at: [0.0, 0.0],
            phase: TouchPhase::Start,
            time: 1000.0,
        });
        input.handle(Event::TouchpadButton {
            button: MouseButton::Left,
            pressed: true,
            double: false,
            time: 1000.0,
        });
        let at = input.cursor();
        input.handle(Event::TouchDropped);
        assert_eq!(input.cursor(), at);
        assert_eq!(input.held(), [left], "the touchpad's press kept");
        input.handle(Event::TouchpadButton {
            button: MouseButton::Left,
            pressed: false,
            double: false,
            time: 1100.0,
        });
        assert!(input.held().is_empty());
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
        input.end_frame();
        assert!(input.device_changes().is_empty() && input.pad_changes().is_empty());
        assert!(input.touch_looks().is_empty());

        input.release_all();
        assert!(input.looks_released() && input.stick_released());
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
