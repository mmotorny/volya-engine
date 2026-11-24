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
//! | `["button", x, y, "left"\|"right"\|"middle", pressed, time]` | mouse button |
//! | `["touch", x, y, "start"\|"move"\|"end", time]` | primary touch |
//! | `["key", name, pressed]` | key ([`Key::name`]), no auto-repeat |
//! | `["pad", name, pressed]` | gamepad button ([`GamepadButton::name`]) |
//! | `["axis", name, value]` | gamepad axis ([`GamepadAxis::name`]) |
//! | `["blur"]` | the page lost focus: release everything |

use std::cell::RefCell;
use std::collections::VecDeque;

use js_sys::Array;
use serde::Serialize;
use volya_engine::core::input::{
    Action, Event, GamepadAxis, GamepadButton, Input, Key, MouseButton, TouchPhase,
};
use wasm_bindgen::JsValue;

/// Actions kept for [`state`], across frames.
const RECENT: usize = 32;

#[derive(Default)]
struct State {
    input: Input,
    capture: bool,
    recent: VecDeque<(u64, Action)>,
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
        }
        s.capture = on;
    });
}

/// Screen size in CSS pixels, the units of event positions.
pub fn resize(width: f64, height: f64) {
    STATE.with(|s| {
        s.borrow_mut()
            .input
            .set_screen([width as f32, height as f32])
    });
}

/// Ends the frame: this frame's actions have been consumed.
pub fn end_frame() {
    STATE.with(|s| s.borrow_mut().input.end_frame());
}

/// Handles one input message from the page; ignored unless capturing.
pub fn handle(message: &Array) -> Result<(), JsValue> {
    let event = decode(message)?;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.capture {
            match event {
                Some(event) => with_recent(&mut s, |input| input.handle(event)),
                None => with_recent(&mut s, Input::release_all),
            }
        }
    });
    Ok(())
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

/// `None` for `blur`.
fn decode(message: &Array) -> Result<Option<Event>, JsValue> {
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
        "touch" => Event::Touch {
            at: at()?,
            phase: match text(3)?.as_str() {
                "start" => TouchPhase::Start,
                "move" => TouchPhase::Move,
                "end" => TouchPhase::End,
                _ => return Err(bad()),
            },
            time: num(4)?,
        },
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
        "blur" => return Ok(None),
        _ => return Err(bad()),
    };
    Ok(Some(event))
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
