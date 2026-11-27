// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Page input in the engine worker: decodes the page's input messages into
//! [`volya_engine::core::input`] events and keeps the engine's input state.
//!
//! The page forwards input only while the engine owns the screen
//! ([`capture`], off by default, so the DOM UI keeps its input). Messages
//! are arrays, positions in CSS pixels relative to the canvas, times in
//! page milliseconds (`Event.timeStamp`):
//!
//! | Message | Meaning |
//! |---|---|
//! | `["move", x, y]` | mouse moved |
//! | `["motion", dx, dy]` | relative mouse motion (pointer lock) |
//! | `["button", x, y, name, pressed, time]` | mouse button ([`MouseButton::name`]) |
//! | `["touch", x, y, "start"\|"move"\|"end", time, id, primary]` | a touch contact (`id`, `primary` optional: a primary contact without id), while the game shows no cursor |
//! | `["cursor_motion", dx, dy, speed]` | a touchpad swipe's motion while the game shows its cursor (#108), and its smoothed speed in CSS pixels per second |
//! | `["cursor_button", "left"\|"right", pressed, time, double]` | a touchpad gesture pressing or releasing a mouse button at the cursor (#108); `double` for a double click's second press |
//! | `["touch_drop"]` | the touches down when the cursor appeared are dropped: their left button released where the cursor is (#108) |
//! | `["key", name, pressed]` | key ([`Key::name`]), no auto-repeat |
//! | `["key", "insert", true, read, text]` | an Insert press while the console is open, with the clipboard the page read for it (#639): `read` is `"text"` (and `text` its text), `"none"` (no text) or `"unavailable"` |
//! | `["pad", name, pressed]` | gamepad button ([`GamepadButton::name`]) |
//! | `["axis", name, value]` | gamepad axis ([`GamepadAxis::name`]) |
//! | `["stick", x, y]` | the on-screen move stick's reach in stick radii (+y down): at most 1 long within its rim, up to 3 per axis past it; `0, 0` when touched and when let go |
//! | `["stick_cancel"]` | the stick's touch cancelled: released without acting where it was |
//! | `["stick_lock", x, y]` | the centre of the stick's sprint lock target for this touch, relative to the stick's, in stick radii (sent before the touch's first `stick`) |
//! | `["look", dx, dy, speed]` | a look drag on the on-screen controls, and its smoothed speed in CSS pixels per second |

//! | `["tbutton", name, pressed]` | an on-screen button ([`TouchButton::name`]) |
//! | `["wheel", steps]` | mouse-wheel steps, negative up |
//! | `["blur"]` | the page lost focus: release everything |
//! | `["unlock"]` | the page lost its pointer lock (sent after `["blur"]`) |
//! | `["text", s]` | the characters of `s`, typed through the page's text entry (the device's keyboard, #94) |
//! | `["typed", s]` | the characters of `s`, typed into the open console through the device's keyboard (#642): each as the presses of the keys that type it, Shift around those that need it (console-input.md section 2) |

use std::cell::RefCell;
use std::collections::VecDeque;

use js_sys::Array;
use serde::Serialize;
use volya_engine::core::console::{Clipboard, typing_key};
use volya_engine::core::input::{
    Action, Event, GamepadAxis, GamepadButton, Input, Key, LookPhase, MouseButton, TouchButton,
    TouchLook, TouchPhase,
};
use volya_engine::formats::ui::Resolution;
use wasm_bindgen::JsValue;

/// Actions kept for [`state`], across frames.
const RECENT: usize = 32;

#[derive(Default)]
struct State {
    input: Input,
    capture: bool,
    recent: VecDeque<(u64, Action)>,
    /// The canvas's size in CSS pixels.
    css: [f32; 2],
    /// The size the engine draws at, shown scaled to fit the canvas with its
    /// aspect kept (#49); `None` when it fills the canvas.
    render: Option<Resolution>,
    /// The top-left of the drawn area in the canvas, CSS pixels.
    offset: [f32; 2],
    /// A primary touch began on the drawn area and has not ended.
    touch_down: bool,
}

impl State {
    /// Event positions are relative to the drawn area: the engine's screen
    /// is the letterboxed picture, not the canvas.
    fn fit(&mut self) {
        let [w, h] = self.css;
        let [x, y, fw, fh] = match self.render {
            Some(r) => r.letterbox((w, h)),
            None => [0.0, 0.0, w, h],
        };
        self.offset = [x, y];
        self.input.set_screen([fw, fh]);
    }

    fn place(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        [x - self.offset[0], y - self.offset[1]]
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Whether the page should forward input to the engine.
pub fn capture() -> bool {
    STATE.with(|s| s.borrow().capture)
}

pub fn set_capture(on: bool) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.capture && !on {
            // No more releases will arrive.
            with_recent(&mut s, Input::release_all);
            s.touch_down = false;
        }
        s.capture = on;
    });
}

/// Screen size in CSS pixels, the units of event positions.
pub fn resize(width: f64, height: f64) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.css = [width as f32, height as f32];
        s.fit();
    });
}

/// The size the engine draws at (`None`: the canvas's own).
pub fn set_render(render: Option<Resolution>) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.render != render {
            s.render = render;
            s.fit();
        }
    });
}

/// UI rectangle `r` (1024 × 768 over the drawn area) in canvas CSS pixels:
/// x, y, width, height.
pub fn css_from_ui(r: volya_engine::formats::ui::layout::Rect) -> [f32; 4] {
    STATE.with(|s| {
        let s = s.borrow();
        let [w, h] = s.input.screen();
        let (sx, sy) = (w / 1024.0, h / 768.0);
        [
            s.offset[0] + r.x * sx,
            s.offset[1] + r.y * sy,
            r.width * sx,
            r.height * sy,
        ]
    })
}

/// Runs `f` on the input state (this frame's actions, the cursor).
pub fn with_input<T>(f: impl FnOnce(&Input) -> T) -> T {
    STATE.with(|s| f(&s.borrow().input))
}

/// Runs `f` on the input state, which it may change: the engine's frame,
/// which UI sequences let place the cursor.
pub fn with_input_mut<T>(f: impl FnOnce(&mut Input) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut().input))
}

/// Ends the frame: this frame's actions have been consumed.
pub fn end_frame() {
    STATE.with(|s| s.borrow_mut().input.end_frame());
}

/// Handles one input message from the page; ignored unless capturing.
pub fn handle(message: &Array) -> Result<(), JsValue> {
    let event = decode(message)?;
    let clipboard = clipboard(message);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.capture {
            if let Some(clipboard) = clipboard {
                s.input.clipboard_read(clipboard);
            }
            match event {
                Some(events) => {
                    let events: Vec<Event> =
                        events.into_iter().filter_map(|e| s.placed(e)).collect();
                    with_recent(&mut s, |input| {
                        for event in events {
                            input.handle(event);
                        }
                    })
                }
                None => {
                    with_recent(&mut s, Input::release_all);
                    s.touch_down = false;
                }
            }
        }
    });
    Ok(())
}

impl State {
    /// Whether `at` (relative to the drawn area) is on it.
    fn on_picture(&self, [x, y]: [f32; 2]) -> bool {
        let [w, h] = self.input.screen();
        (0.0..w).contains(&x) && (0.0..h).contains(&y)
    }

    /// `e` with its position relative to the drawn area; `None` for a press
    /// or a touch begun in the black bars around it, which are ignored
    /// rather than taken to the picture's edge (our choice). Moves and
    /// releases outside it still count (the cursor stops at the edge).
    fn placed(&mut self, e: Event) -> Option<Event> {
        Some(match e {
            Event::PointerMoved { at } => Event::PointerMoved { at: self.place(at) },
            Event::PointerButton {
                at,
                button,
                pressed,
                time,
            } => {
                let at = at.map(|a| self.place(a));
                if pressed && at.is_some_and(|a| !self.on_picture(a)) {
                    return None;
                }
                Event::PointerButton {
                    at,
                    button,
                    pressed,
                    time,
                }
            }
            Event::Touch { at, phase, time } => {
                let at = self.place(at);
                match phase {
                    TouchPhase::Start if !self.on_picture(at) => return None,
                    TouchPhase::Start => self.touch_down = true,
                    _ if !self.touch_down => return None,
                    TouchPhase::End => self.touch_down = false,
                    TouchPhase::Move => {}
                }
                Event::Touch { at, phase, time }
            }
            Event::TouchDropped => {
                self.touch_down = false;
                Event::TouchDropped
            }
            Event::Contact { id, at, phase } => Event::Contact {
                id,
                at: self.place(at),
                phase,
            },
            e => e,
        })
    }
}

/// Runs `f` on the input and records the actions it adds.
fn with_recent(s: &mut State, f: impl FnOnce(&mut Input)) {
    let before = s.input.action_count();
    f(&mut s.input);
    let added = (s.input.action_count() - before) as usize;
    let actions = s.input.actions();
    for (i, &action) in actions[actions.len() - added..].iter().enumerate() {
        if s.recent.len() == RECENT {
            s.recent.pop_front();
        }
        s.recent.push_back((before + i as u64 + 1, action));
    }
}

/// The clipboard a key message carries, if any.
fn clipboard(message: &Array) -> Option<Clipboard> {
    if message.get(0).as_string()? != "key" {
        return None;
    }
    Some(match message.get(3).as_string()?.as_str() {
        "text" => Clipboard::Text(message.get(4).as_string().unwrap_or_default()),
        "none" => Clipboard::NoText,
        _ => Clipboard::Unavailable,
    })
}

/// The key presses and releases that type `s` in the console, in order
/// (#642): each character's key ([`typing_key`]), within a press and
/// release of left Shift where it needs one; characters no key types are
/// dropped.
fn typed_keys(s: &str) -> Vec<Event> {
    let key = |key, pressed| Event::Key { key, pressed };
    let mut events = Vec::new();
    for (code, shift) in s.chars().filter_map(typing_key) {
        let Some(k) = Key::ALL.iter().copied().find(|k| k.code() == code) else {
            continue;
        };
        if shift {
            events.push(key(Key::LeftShift, true));
        }
        events.extend([key(k, true), key(k, false)]);
        if shift {
            events.push(key(Key::LeftShift, false));
        }
    }
    events
}

/// `None` for `blur`.
fn decode(message: &Array) -> Result<Option<Vec<Event>>, JsValue> {
    let bad = || JsValue::from_str(&format!("bad input message {message:?}"));
    let num = |i: u32| message.get(i).as_f64().ok_or_else(bad);
    let text = |i: u32| message.get(i).as_string().ok_or_else(bad);
    let flag = |i: u32| message.get(i).as_bool().ok_or_else(bad);
    let at = || -> Result<[f32; 2], JsValue> { Ok([num(1)? as f32, num(2)? as f32]) };
    let event = match text(0)?.as_str() {
        "move" => Event::PointerMoved { at: at()? },
        "motion" => Event::PointerMotion { delta: at()? },
        "button" => Event::PointerButton {
            at: Some(at()?),
            button: MouseButton::from_name(&text(3)?).ok_or_else(bad)?,
            pressed: flag(4)?,
            time: num(5)?,
        },
        "touch" => {
            let phase = match text(3)?.as_str() {
                "start" => TouchPhase::Start,
                "move" => TouchPhase::Move,
                "end" => TouchPhase::End,
                _ => return Err(bad()),
            };
            let id = message.get(5).as_f64().map_or(0, |id| id as u32);
            let primary = message.get(6).as_bool().unwrap_or(true);
            let mut events = vec![Event::Contact {
                id,
                at: at()?,
                phase,
            }];
            if primary {
                events.push(Event::Touch {
                    at: at()?,
                    phase,
                    time: num(4)?,
                });
            }
            return Ok(Some(events));
        }
        "cursor_motion" => Event::TouchpadMotion {
            delta: at()?,
            speed: num(3)? as f32,
        },
        "cursor_button" => Event::TouchpadButton {
            button: MouseButton::from_name(&text(1)?).ok_or_else(bad)?,
            pressed: flag(2)?,
            double: message.get(4).as_bool().unwrap_or(false),
            time: num(3)?,
        },
        "touch_drop" => Event::TouchDropped,
        "key" => Event::Key {
            key: Key::from_name(&text(1)?).ok_or_else(bad)?,
            pressed: flag(2)?,
        },
        "pad" => Event::GamepadButton {
            button: GamepadButton::from_name(&text(1)?).ok_or_else(bad)?,
            pressed: flag(2)?,
        },
        "axis" => Event::GamepadAxis {
            axis: GamepadAxis::from_name(&text(1)?).ok_or_else(bad)?,
            value: num(2)? as f32,
        },
        "stick" => Event::TouchStick { value: at()? },
        "stick_cancel" => Event::TouchStickCancel,
        "stick_lock" => Event::TouchLockTarget { centre: at()? },
        "look" => Event::TouchLook(TouchLook {
            finger: num(5)? as u32,
            phase: match text(3)?.as_str() {
                "down" => LookPhase::Down,
                "move" => LookPhase::Move,
                "up" => LookPhase::Up,
                "cancel" => LookPhase::Cancel,
                _ => return Err(bad()),
            },
            at: at()?,
            time: num(4)?,
        }),
        "tbutton" => Event::TouchButton {
            button: TouchButton::from_name(&text(1)?).ok_or_else(bad)?,
            pressed: flag(2)?,
        },
        "wheel" => Event::Wheel {
            steps: num(1)? as i32,
        },
        "blur" => return Ok(None),
        "unlock" => Event::PointerUnlocked,
        "text" => {
            let s = text(1)?;
            return Ok(Some(s.chars().map(|ch| Event::Text { ch }).collect()));
        }
        "typed" => return Ok(Some(typed_keys(&text(1)?))),
        _ => return Err(bad()),
    };
    Ok(Some(vec![event]))
}

#[derive(Serialize)]
pub struct Snapshot {
    capture: bool,
    screen: [f32; 2],
    sensitivity: f32,
    /// Normalised, −1 … +1.
    cursor: [f32; 2],
    /// UI units (1024 × 768).
    ui: [f32; 2],
    held: Vec<&'static str>,
    /// Actions since start.
    count: u64,
    /// The latest actions, oldest first.
    recent: Vec<RecentAction>,
}

#[derive(Serialize)]
pub struct RecentAction {
    seq: u64,
    action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    control: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    double: bool,
}

/// The `input_state` op.
pub fn state() -> Snapshot {
    STATE.with(|s| {
        let s = s.borrow();
        let input = &s.input;
        Snapshot {
            capture: s.capture,
            screen: input.screen(),
            sensitivity: input.sensitivity(),
            cursor: input.cursor(),
            ui: input.cursor_ui(),
            held: input.held().into_iter().map(|c| c.name()).collect(),
            count: input.action_count(),
            recent: s
                .recent
                .iter()
                .map(|&(seq, action)| {
                    let (action, control, to, double) = match action {
                        Action::Cursor { to } => ("cursor", None, Some(to), false),
                        Action::Press { control, double } => {
                            ("press", Some(control.name()), None, double)
                        }
                        Action::Release { control } => {
                            ("release", Some(control.name()), None, false)
                        }
                        Action::Wheel { up } => (
                            if up { "wheel_up" } else { "wheel_down" },
                            None,
                            None,
                            false,
                        ),
                    };
                    RecentAction {
                        seq,
                        action,
                        control,
                        to,
                        double,
                    }
                })
                .collect(),
        }
    })
}
