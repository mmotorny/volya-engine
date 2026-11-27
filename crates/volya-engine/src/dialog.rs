// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the menu's save and load screens and its message boxes share
//! (docs/behavior/save-load-screens.md sections 3.1, 3.2 and 6): their
//! input events, the push button, and the message box.
//!
//! **Input.** These screens are mouse screens (section 3.1). Touch reaches
//! them through the standard touch controls' stick and cursor buttons
//! (#856, [`crate::core::input`]) or directly (#807), a gamepad as a mouse (#835, [`Input::set_pad_cursor`]): its left stick
//! moves the cursor, south (A / cross) is the left button; east (B /
//! circle) is the key bound to `quit`.

use std::io;

use serde::Serialize;

use crate::core::input::{
    Action, Control, GamepadButton, Input, Key, MouseButton, ui_from_normalized,
};
use crate::data::GameData;
use crate::draw::Ctx;
use crate::scene::Scene;
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::layout::{HAlign, Rect, StaticDesc, VAlign, require};
use volya_engine_formats::ui::{Color, ColorDefs};
use volya_engine_formats::xml::Document;

/// One input event of a frame, in the screens' vocabulary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ev {
    /// The cursor moved here (UI units).
    Move([f32; 2]),
    /// A left-button press; `double` for the second press of a double click
    /// (section 3.1).
    Press {
        double: bool,
    },
    Release,
    Wheel {
        up: bool,
    },
    /// A key press or release (the keyboard's own keys).
    Key {
        key: Key,
        pressed: bool,
    },
    /// A press or release of the key bound to `quit` (section 3.7), or of a
    /// gamepad's east button.
    Quit {
        pressed: bool,
    },
    /// A character typed through the platform's text entry (the device's
    /// own keyboard on touch screens; our design, #94).
    Text(char),
}

/// This frame's events. The console key and F12 are not the screens'
/// (section 3.1); nothing here uses them.
pub fn events(data: &GameData, input: &Input) -> Vec<Ev> {
    events_from(data, input, true)
}

/// [`events`], without the gamepad's east button unless `pad` (the save
/// screen's on-screen keyboard takes it while open).
pub fn events_from(data: &GameData, input: &Input, pad: bool) -> Vec<Ev> {
    let mut out = Vec::new();
    for a in input.actions() {
        match *a {
            Action::Cursor { to } => out.push(Ev::Move(ui_from_normalized(to))),
            Action::Press {
                control: Control::Mouse(MouseButton::Left),
                double,
            } => out.push(Ev::Press { double }),
            Action::Release {
                control: Control::Mouse(MouseButton::Left),
            } => out.push(Ev::Release),
            Action::Wheel { up } => out.push(Ev::Wheel { up }),
            _ => {}
        }
    }
    for &(c, pressed) in input.device_changes() {
        if let Control::Key(key) = c {
            out.push(Ev::Key { key, pressed });
            if data.quit_keys.contains(&key) {
                out.push(Ev::Quit { pressed });
            }
        }
    }
    out.extend(input.text().iter().map(|&c| Ev::Text(c)));
    if pad {
        for &(b, pressed) in input.pad_changes() {
            if b == GamepadButton::East {
                out.push(Ev::Quit { pressed });
            }
        }
    }
    out
}

/// Whether the left button is held (mouse, touch or a gamepad's south).
pub fn left_held(input: &Input) -> bool {
    input.is_held(Control::Mouse(MouseButton::Left))
}

/// Rectangles as the gamepad cursor's targets ([`Input::pad_cursor_frame`]).
pub fn cursor_targets(rects: &[Rect]) -> Vec<[f32; 4]> {
    rects
        .iter()
        .map(|r| [r.x, r.y, r.width, r.height])
        .collect()
}

/// Whether `r` holds `p`, edges included (section 3.2).
pub fn contains(r: Rect, [x, y]: [f32; 2]) -> bool {
    x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
}

/// A push button's state (section 3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonState {
    #[default]
    Idle,
    Pressed,
    Returning,
}

/// What an event did to a push button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pushed {
    Nothing,
    /// It became pressed.
    Pressed,
    /// It was activated: its action runs.
    Activated,
}

/// A push button (section 3.2): its cursor-over flag, set in the per-frame
/// update, and its state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct PushButton {
    pub over: bool,
    pub state: ButtonState,
}

/// A change of a push button's cursor-over flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Over {
    Entered,
    Left,
}

impl PushButton {
    /// The per-frame update: the flag from the cursor (edges included).
    pub fn frame(&mut self, rect: Rect, cursor: [f32; 2]) -> Option<Over> {
        let over = contains(rect, cursor);
        let change = match (self.over, over) {
            (false, true) => Some(Over::Entered),
            (true, false) => Some(Over::Left),
            _ => None,
        };
        self.over = over;
        change
    }

    /// Whether the button receives a mouse event at `cursor`: inside it, or
    /// with its flag still set.
    pub fn receives(&self, rect: Rect, cursor: [f32; 2]) -> bool {
        self.over || contains(rect, cursor)
    }

    /// The transitions of section 3.2 for an event the button receives.
    pub fn event(&mut self, e: Ev) -> Pushed {
        match (self.state, e) {
            (ButtonState::Idle, Ev::Press { .. }) => {
                self.state = ButtonState::Pressed;
                Pushed::Pressed
            }
            (ButtonState::Pressed, Ev::Release) => {
                self.state = ButtonState::Idle;
                if self.over {
                    Pushed::Activated
                } else {
                    Pushed::Nothing
                }
            }
            (ButtonState::Pressed, Ev::Move(_)) if !self.over => {
                self.state = ButtonState::Returning;
                Pushed::Nothing
            }
            (ButtonState::Returning, Ev::Move(_)) if self.over => {
                self.state = ButtonState::Pressed;
                Pushed::Nothing
            }
            (ButtonState::Returning, Ev::Release) => {
                self.state = ButtonState::Idle;
                Pushed::Nothing
            }
            _ => Pushed::Nothing,
        }
    }

    /// The look: `t` while pressed, else `h` with the flag set, else `e`.
    pub fn look(&self) -> &'static str {
        match self.state {
            ButtonState::Pressed => "t",
            _ if self.over => "h",
            _ => "e",
        }
    }
}

/// A look's caption colour (section 3.2): its `text_color` entry, else the
/// normal one, else the `text` child's colour.
pub fn look_color(d: &StaticDesc, look: &str, fallback: Color) -> Color {
    let Some(c) = &d.text_colors else {
        return fallback;
    };
    let pick = match look {
        "d" => c.disabled,
        "t" => c.touched,
        "h" => c.highlighted,
        _ => None,
    };
    pick.or(c.enabled).unwrap_or(fallback)
}

/// Draws button `desc` at `rect` with look `look`: the look's texture id at
/// its pixel size from the top-left, the caption single-line and centred
/// both ways.
pub fn draw_button(
    ctx: &mut Ctx,
    s: &mut Scene,
    layer: &str,
    desc: &StaticDesc,
    rect: Rect,
    look: &str,
) -> io::Result<()> {
    if let Some(t) = &desc.texture {
        ctx.part(
            s,
            layer,
            &format!("{}_{look}", t.name),
            (rect.x, rect.y),
            None,
            Color::WHITE,
        )?;
    }
    let (mut b, font) = TextBox::of_static(desc, rect);
    b.align = HAlign::Centre;
    b.vert_align = VAlign::Centre;
    b.complex = false;
    b.color = look_color(desc, look, b.color);
    if let Some(key) = desc.text.as_ref().and_then(|t| t.key.as_deref()) {
        ctx.text(s, &format!("{layer} text"), &ctx.data.text(key), &b, font)?;
    }
    Ok(())
}

/// The texture ids of button `desc`'s four looks.
pub fn button_textures(desc: &StaticDesc) -> Vec<String> {
    desc.texture
        .iter()
        .flat_map(|t| ["e", "h", "t", "d"].map(|l| format!("{}_{l}", t.name)))
        .collect()
}

/// A message box's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Yes,
    No,
    Ok,
}

/// A box of `config\ui\message_box.xml` (section 6): the box, its sign, its
/// text and its buttons, relative to the box.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageBoxDesc {
    pub name: String,
    pub frame: StaticDesc,
    pub picture: StaticDesc,
    pub text: StaticDesc,
    /// The box's `type`.
    pub kind: Option<String>,
    /// By the box's `type`: `ok` has OK, others Yes and No.
    pub buttons: Vec<(Answer, StaticDesc)>,
}

impl MessageBoxDesc {
    /// Box `name` of `doc` (`message_box.xml`).
    pub fn read(doc: &Document, name: &str, colors: &ColorDefs) -> io::Result<Self> {
        let e = require(doc, name, 0)?;
        let part = |p: &str| StaticDesc::read(require(doc, &format!("{name}:{p}"), 0)?, colors);
        // Section 6 gives `ok` OK and `yes_no` Yes and No. Every other type
        // (the quit boxes' own) also gets Yes and No: our choice, from the
        // buttons title-menu.md 4.5 names for those boxes.
        let answers: &[(Answer, &str)] = match e.attr("type") {
            Some("ok") => &[(Answer::Ok, "button_ok")],
            _ => &[(Answer::Yes, "button_yes"), (Answer::No, "button_no")],
        };
        let buttons = answers
            .iter()
            .map(|&(a, p)| Ok((a, part(p)?)))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            name: name.to_owned(),
            frame: StaticDesc::read(e, colors)?,
            picture: part("picture")?,
            text: part("message_text")?,
            kind: e.attr("type").map(str::to_owned),
            buttons,
        })
    }

    /// The box with text key `key` in place of the data's (the menu sets a
    /// box's text, title-menu.md 4.5).
    pub fn with_text(mut self, key: &str) -> Self {
        if let Some(t) = &mut self.text.text {
            t.key = Some(key.to_owned());
        }
        self
    }

    fn origin(&self) -> Rect {
        self.frame.window.absolute(0.0, 0.0)
    }

    /// Button `i`'s absolute rectangle.
    pub fn button_rect(&self, i: usize) -> Rect {
        let f = self.origin();
        self.buttons[i].1.window.absolute(f.x, f.y)
    }

    /// Every texture id or path and string key the box draws.
    pub fn references(&self) -> (Vec<String>, Vec<String>) {
        let mut textures: Vec<String> = [&self.frame, &self.picture]
            .iter()
            .flat_map(|d| d.texture.iter().map(|t| t.name.clone()))
            .collect();
        let mut strings: Vec<String> = self.text.text.iter().flat_map(|t| t.key.clone()).collect();
        for (_, b) in &self.buttons {
            textures.extend(button_textures(b));
            strings.extend(b.text.iter().flat_map(|t| t.key.clone()));
        }
        (textures, strings)
    }
}

/// A shown message box. Its window covers the whole screen; it reacts to
/// the mouse only (section 6).
#[derive(Clone, Debug)]
pub struct MessageBox {
    pub desc: MessageBoxDesc,
    buttons: Vec<PushButton>,
}

impl MessageBox {
    /// Shows `desc` with the cursor at `cursor`.
    pub fn new(desc: MessageBoxDesc, cursor: [f32; 2]) -> Self {
        let mut b = Self {
            buttons: vec![PushButton::default(); desc.buttons.len()],
            desc,
        };
        b.frame(cursor);
        b
    }

    /// The per-frame update of the buttons' flags.
    pub fn frame(&mut self, cursor: [f32; 2]) {
        for i in 0..self.buttons.len() {
            let r = self.desc.button_rect(i);
            self.buttons[i].frame(r, cursor);
        }
    }

    /// One event at `cursor` (after the event's move); an activated button
    /// closes the box with its answer. Keys do nothing.
    pub fn event(&mut self, e: Ev, cursor: [f32; 2]) -> Option<Answer> {
        if matches!(e, Ev::Press { .. }) {
            // A press finds the flags as the cursor stands (our choice, so
            // a tap, which moves, presses and releases in one frame, works).
            self.frame(cursor);
        }
        if !matches!(e, Ev::Move(_) | Ev::Press { .. } | Ev::Release) {
            return None;
        }
        for i in 0..self.buttons.len() {
            let r = self.desc.button_rect(i);
            if self.buttons[i].receives(r, cursor) && self.buttons[i].event(e) == Pushed::Activated
            {
                return Some(self.desc.buttons[i].0);
            }
        }
        None
    }

    /// The gamepad cursor's targets: the buttons.
    pub fn targets(&self) -> Vec<Rect> {
        (0..self.buttons.len())
            .map(|i| self.desc.button_rect(i))
            .collect()
    }

    pub fn draw(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let d = &self.desc;
        let f = d.origin();
        ctx.picture(s, "message", &d.frame, f, 0.0, 0, false, None)?;
        let pic = d.picture.window.absolute(f.x, f.y);
        ctx.picture(s, "message picture", &d.picture, pic, 0.0, 0, false, None)?;
        let tr = d.text.window.absolute(f.x, f.y);
        let (b, font) = TextBox::of_static(&d.text, tr);
        if let Some(key) = d.text.text.as_ref().and_then(|t| t.key.as_deref()) {
            ctx.text(s, "message text", &ctx.data.text(key), &b, font)?;
        }
        for (i, (answer, desc)) in d.buttons.iter().enumerate() {
            let layer = format!("message {}", answer_name(*answer));
            draw_button(
                ctx,
                s,
                &layer,
                desc,
                d.button_rect(i),
                self.buttons[i].look(),
            )?;
        }
        Ok(())
    }

    pub fn state(&self) -> MessageBoxState {
        MessageBoxState {
            name: self.desc.name.clone(),
            text: self
                .desc
                .text
                .text
                .as_ref()
                .and_then(|t| t.key.clone())
                .unwrap_or_default(),
            buttons: self
                .desc
                .buttons
                .iter()
                .enumerate()
                .map(|(i, (a, _))| {
                    let r = self.desc.button_rect(i);
                    BoxButtonState {
                        answer: *a,
                        rect: [r.x, r.y, r.width, r.height],
                        look: self.buttons[i].look(),
                    }
                })
                .collect(),
        }
    }
}

fn answer_name(a: Answer) -> &'static str {
    match a {
        Answer::Yes => "yes",
        Answer::No => "no",
        Answer::Ok => "ok",
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BoxButtonState {
    pub answer: Answer,
    /// Absolute (x, y, width, height).
    pub rect: [f32; 4],
    pub look: &'static str,
}

/// A message box as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct MessageBoxState {
    pub name: String,
    /// The text's string key.
    pub text: String,
    pub buttons: Vec<BoxButtonState>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rect = Rect::new(10.0, 10.0, 20.0, 10.0);

    #[test]
    fn a_click_activates_a_press_moved_off_does_not() {
        let mut b = PushButton::default();
        b.frame(R, [15.0, 15.0]);
        assert_eq!(b.look(), "h");
        assert_eq!(b.event(Ev::Press { double: false }), Pushed::Pressed);
        assert_eq!(b.look(), "t");
        assert_eq!(b.event(Ev::Release), Pushed::Activated);
        // Pressed, moved off, released elsewhere: the release does not reach
        // it, so it stays pressed (look t).
        b.event(Ev::Press { double: false });
        b.frame(R, [100.0, 100.0]);
        assert!(!b.receives(R, [100.0, 100.0]));
        assert_eq!(b.look(), "t");
        // Back over it: the first movement makes it returning, the next
        // pressed again; a release then activates it.
        assert!(b.receives(R, [12.0, 12.0]));
        b.event(Ev::Move([12.0, 12.0]));
        assert_eq!(b.state, ButtonState::Returning);
        b.frame(R, [12.0, 12.0]);
        assert_eq!(b.look(), "h");
        b.event(Ev::Move([13.0, 12.0]));
        assert_eq!(b.state, ButtonState::Pressed);
        assert_eq!(b.event(Ev::Release), Pushed::Activated);
        // Edges are inside.
        assert!(contains(R, [30.0, 20.0]));
    }
}
