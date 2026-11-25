// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The on-screen keyboard: typing the save screen's name with a gamepad.
//! A Volya Engine addition, decided by the user (#94): the original's name
//! field takes only a keyboard (docs/behavior/save-load-screens.md section
//! 4.4) and its screens no gamepad (section 3.1).
//!
//! It opens when a gamepad's south button presses inside the field (which
//! gives the field focus) and closes when the field loses it. While open it
//! takes the gamepad: the d-pad moves the highlight (wrapping), south
//! presses the highlighted key, west is Backspace and east is Done. Its
//! keys are the field's own (section 4.4: the character keys, Space and
//! Shift, here a toggle) plus Backspace and Done (Enter), and each acts as
//! the field's key, so the field's rules hold: the characters it types and
//! its width limit. Drawn plainly by the engine over the screen's bottom:
//! flat keys, the highlighted one in the field's text colour; the four
//! special keys carry line symbols, not words (the game's language is not
//! ours to add to): an arrow up (Shift), a space bar's bracket (Space), an
//! arrow left (Backspace) and a check mark (Done).

use std::io;

use serde::Serialize;

use crate::core::input::{GamepadButton, Key};
use crate::draw::Ctx;
use crate::save_load::typed;
use crate::scene::{Scene, Sprite, TextureId};
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::Color;
use volya_engine_formats::ui::layout::{HAlign, Rect, VAlign};
use volya_engine_formats::ui::picture::Quad;

/// What a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// The field's character key, shifted or not.
    Char(Key, bool),
    Backspace,
    /// Enter: the field loses focus, the keyboard closes.
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Char(Key),
    Shift,
    Backspace,
    Done,
}

/// A key: its row, its first column and its width in columns.
#[derive(Clone, Copy, Debug)]
struct KeyCap {
    row: usize,
    col: f32,
    span: f32,
    kind: Kind,
}

/// The character rows, as the keys stand on a US keyboard (the field's
/// characters, section 4.4).
const ROWS: [&[Key]; 4] = [
    &[
        Key::Digit1,
        Key::Digit2,
        Key::Digit3,
        Key::Digit4,
        Key::Digit5,
        Key::Digit6,
        Key::Digit7,
        Key::Digit8,
        Key::Digit9,
        Key::Digit0,
        Key::Minus,
        Key::Equals,
    ],
    &[
        Key::Q,
        Key::W,
        Key::E,
        Key::R,
        Key::T,
        Key::Y,
        Key::U,
        Key::I,
        Key::O,
        Key::P,
        Key::LeftBracket,
        Key::RightBracket,
    ],
    &[
        Key::A,
        Key::S,
        Key::D,
        Key::F,
        Key::G,
        Key::H,
        Key::J,
        Key::K,
        Key::L,
        Key::Semicolon,
        Key::Apostrophe,
        Key::Backslash,
    ],
    &[
        Key::Z,
        Key::X,
        Key::C,
        Key::V,
        Key::B,
        Key::N,
        Key::M,
        Key::Comma,
        Key::Period,
        Key::Slash,
    ],
];

/// Columns of the widest row.
const COLUMNS: f32 = 12.0;
/// Key size and the space between keys, UI units.
const KEY: (f32, f32) = (48.0, 36.0);
const GAP: f32 = 4.0;
/// The panel's padding, UI units, and its distance from the screen's
/// bottom.
const PAD: f32 = 10.0;
const BOTTOM: f32 = 24.0;
/// The field's text colour (section 4.4).
const TEXT: Color = Color::rgba(240, 217, 182, 255);

fn keys() -> Vec<KeyCap> {
    let mut out = Vec::new();
    for (row, keys) in ROWS.iter().enumerate() {
        // Shorter rows centred.
        let first = (COLUMNS - keys.len() as f32) / 2.0;
        for (i, &key) in keys.iter().enumerate() {
            out.push(KeyCap {
                row,
                col: first + i as f32,
                span: 1.0,
                kind: Kind::Char(key),
            });
        }
    }
    let row = ROWS.len();
    let mut col = 0.0;
    for (span, kind) in [
        (2.0, Kind::Shift),
        (6.0, Kind::Char(Key::Space)),
        (2.0, Kind::Backspace),
        (2.0, Kind::Done),
    ] {
        out.push(KeyCap {
            row,
            col,
            span,
            kind,
        });
        col += span;
    }
    out
}

/// The panel behind the keys, UI units.
fn panel() -> Rect {
    let width = COLUMNS * (KEY.0 + GAP) - GAP + 2.0 * PAD;
    let rows = ROWS.len() as f32 + 1.0;
    let height = rows * (KEY.1 + GAP) - GAP + 2.0 * PAD;
    Rect::new(
        (1024.0 - width) / 2.0,
        768.0 - BOTTOM - height,
        width,
        height,
    )
}

fn key_rect(k: &KeyCap) -> Rect {
    let p = panel();
    Rect::new(
        p.x + PAD + k.col * (KEY.0 + GAP),
        p.y + PAD + k.row as f32 * (KEY.1 + GAP),
        k.span * (KEY.0 + GAP) - GAP,
        KEY.1,
    )
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyboardState {
    /// The highlighted key's label.
    pub selected: String,
    pub shift: bool,
}

/// The keyboard while open.
#[derive(Clone, Debug)]
pub struct OnScreenKeyboard {
    keys: Vec<KeyCap>,
    selected: usize,
    shift: bool,
}

impl Default for OnScreenKeyboard {
    fn default() -> Self {
        Self {
            keys: keys(),
            selected: 0,
            shift: false,
        }
    }
}

impl OnScreenKeyboard {
    /// A gamepad button pressed: what the field gets, if anything.
    pub fn press(&mut self, button: GamepadButton) -> Option<Press> {
        match button {
            GamepadButton::DpadLeft => self.step(-1),
            GamepadButton::DpadRight => self.step(1),
            GamepadButton::DpadUp => self.row(-1),
            GamepadButton::DpadDown => self.row(1),
            GamepadButton::South => return self.activate(),
            GamepadButton::West => return Some(Press::Backspace),
            GamepadButton::East => return Some(Press::Done),
            _ => {}
        }
        None
    }

    fn activate(&mut self) -> Option<Press> {
        match self.keys[self.selected].kind {
            Kind::Char(key) => Some(Press::Char(key, self.shift)),
            Kind::Shift => {
                self.shift = !self.shift;
                None
            }
            Kind::Backspace => Some(Press::Backspace),
            Kind::Done => Some(Press::Done),
        }
    }

    /// The next or previous key in the row, wrapping.
    fn step(&mut self, by: isize) {
        let row = self.keys[self.selected].row;
        let in_row: Vec<usize> = (0..self.keys.len())
            .filter(|&i| self.keys[i].row == row)
            .collect();
        let at = in_row.iter().position(|&i| i == self.selected).unwrap_or(0) as isize;
        let n = in_row.len() as isize;
        self.selected = in_row[(at + by).rem_euclid(n) as usize];
    }

    /// The key above or below, wrapping: the one nearest the highlighted
    /// key's centre.
    fn row(&mut self, by: isize) {
        let k = self.keys[self.selected];
        let rows = ROWS.len() as isize + 1;
        let row = (k.row as isize + by).rem_euclid(rows) as usize;
        let centre = k.col + k.span / 2.0;
        let distance = |c: &KeyCap| {
            if (c.col..c.col + c.span).contains(&centre) {
                0.0
            } else {
                (c.col + c.span / 2.0 - centre).abs()
            }
        };
        if let Some(i) = (0..self.keys.len())
            .filter(|&i| self.keys[i].row == row)
            .min_by(|&a, &b| distance(&self.keys[a]).total_cmp(&distance(&self.keys[b])))
        {
            self.selected = i;
        }
    }

    /// The key's name in the state (tests, debugging); the special keys
    /// draw symbols instead.
    fn label(&self, k: &KeyCap) -> String {
        match k.kind {
            Kind::Char(Key::Space) => "Space".into(),
            Kind::Char(key) => typed(key, self.shift).map(String::from).unwrap_or_default(),
            Kind::Shift => "Shift".into(),
            Kind::Backspace => "Backspace".into(),
            Kind::Done => "Done".into(),
        }
    }

    pub fn state(&self) -> KeyboardState {
        KeyboardState {
            selected: self.label(&self.keys[self.selected]),
            shift: self.shift,
        }
    }

    /// Adds the keyboard to `scene`, its labels in `font`.
    pub fn draw(&self, ctx: &mut Ctx, scene: &mut Scene, font: UiFont) -> io::Result<()> {
        let (sx, sy) = ctx.scale();
        let flat = |scene: &mut Scene, layer: &str, r: Rect, color: Color| {
            scene.sprites.push(Sprite::picture(
                layer,
                TextureId::White,
                &Quad {
                    min: ((r.x * sx).floor() - 0.5, (r.y * sy).floor() - 0.5),
                    max: (
                        ((r.x + r.width) * sx).floor() - 0.5,
                        ((r.y + r.height) * sy).floor() - 0.5,
                    ),
                    uv_min: (0.0, 0.0),
                    uv_max: (1.0, 1.0),
                    color,
                },
            ));
        };
        flat(scene, "keyboard", panel(), Color::rgba(0, 0, 0, 240));
        for (i, k) in self.keys.iter().enumerate() {
            let r = key_rect(k);
            let selected = i == self.selected;
            let back = if selected {
                TEXT
            } else if k.kind == Kind::Shift && self.shift {
                Color::rgba(255, 255, 255, 90)
            } else {
                Color::rgba(255, 255, 255, 36)
            };
            flat(scene, "keyboard key", r, back);
            let text = TextBox {
                rect: r,
                offset: (0.0, 0.0),
                align: HAlign::Centre,
                vert_align: VAlign::Centre,
                complex: false,
                color: if selected {
                    Color::rgba(0, 0, 0, 255)
                } else {
                    TEXT
                },
                line_gap: 0.0,
            };
            if let Kind::Char(key) = k.kind
                && key != Key::Space
            {
                let label = crate::data::encode(&self.label(k));
                ctx.text(scene, "keyboard label", &label, &text, font)?;
            } else {
                for [a, b] in symbol(k.kind, r) {
                    scene.sprites.push(segment(a, b, (sx, sy), text.color));
                }
            }
        }
        Ok(())
    }
}

/// The line segments (UI units) of a special key's symbol, centred in `r`.
fn symbol(kind: Kind, r: Rect) -> Vec<[[f32; 2]; 2]> {
    let (cx, cy) = (r.x + r.width / 2.0, r.y + r.height / 2.0);
    let p = |x: f32, y: f32| [cx + x, cy + y];
    let path = |points: &[[f32; 2]]| -> Vec<[[f32; 2]; 2]> {
        points.windows(2).map(|w| [w[0], w[1]]).collect()
    };
    match kind {
        // An outlined arrow up.
        Kind::Shift => path(&[
            p(0.0, -9.0),
            p(9.0, 0.0),
            p(4.0, 0.0),
            p(4.0, 8.0),
            p(-4.0, 8.0),
            p(-4.0, 0.0),
            p(-9.0, 0.0),
            p(0.0, -9.0),
        ]),
        // A space bar's bracket.
        Kind::Char(_) => path(&[p(-14.0, -3.0), p(-14.0, 4.0), p(14.0, 4.0), p(14.0, -3.0)]),
        // An arrow left.
        Kind::Backspace => {
            let mut s = path(&[p(-7.0, -6.0), p(-13.0, 0.0), p(-7.0, 6.0)]);
            s.push([p(-13.0, 0.0), p(12.0, 0.0)]);
            s
        }
        // A check mark.
        Kind::Done => path(&[p(-9.0, 0.0), p(-3.0, 6.0), p(9.0, -7.0)]),
    }
}

/// Line width of the symbols, UI units.
const STROKE: f32 = 2.0;

/// A line from `a` to `b` (UI units), [`STROKE`] wide with square caps,
/// as a quad in output pixels.
fn segment(a: [f32; 2], b: [f32; 2], (sx, sy): (f32, f32), color: Color) -> Sprite {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy).max(f32::EPSILON);
    let h = STROKE / 2.0;
    let (ux, uy) = (dx / len * h, dy / len * h);
    let (nx, ny) = (-uy, ux);
    let at = |x: f32, y: f32| [x * sx - 0.5, y * sy - 0.5];
    let (a0, a1) = (a[0] - ux, a[1] - uy);
    let (b0, b1) = (b[0] + ux, b[1] + uy);
    Sprite {
        layer: "keyboard symbol".into(),
        texture: TextureId::White,
        corners: [
            at(a0 + nx, a1 + ny),
            at(b0 + nx, b1 + ny),
            at(b0 - nx, b1 - ny),
            at(a0 - nx, a1 - ny),
        ],
        uv_min: [0.0, 0.0],
        uv_max: [1.0, 1.0],
        color,
        corner_colors: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_d_pad_moves_the_highlight_and_south_presses_it() {
        let mut k = OnScreenKeyboard::default();
        assert_eq!(k.state().selected, "1");
        // Down twice: the third row's first key; left wraps to its last.
        k.press(GamepadButton::DpadDown);
        k.press(GamepadButton::DpadDown);
        assert_eq!(k.state().selected, "a");
        assert_eq!(
            k.press(GamepadButton::South),
            Some(Press::Char(Key::A, false))
        );
        k.press(GamepadButton::DpadLeft);
        assert_eq!(k.state().selected, "\\");
        // The fourth row is centred: below `a` (column 0) its nearest key
        // is `z` (column 1).
        k.press(GamepadButton::DpadRight);
        k.press(GamepadButton::DpadDown);
        assert_eq!(k.state().selected, "z");
        // Down to the bottom row: Shift toggles, then keys type shifted.
        k.press(GamepadButton::DpadDown);
        assert_eq!(k.state().selected, "Shift");
        assert_eq!(k.press(GamepadButton::South), None);
        assert!(k.state().shift);
        k.press(GamepadButton::DpadUp);
        assert_eq!(k.state().selected, "Z");
        assert_eq!(
            k.press(GamepadButton::South),
            Some(Press::Char(Key::Z, true))
        );
        // Up from the top row wraps to the bottom: Space below the middle.
        for _ in 0..3 {
            k.press(GamepadButton::DpadUp);
        }
        k.press(GamepadButton::DpadRight);
        k.press(GamepadButton::DpadRight);
        k.press(GamepadButton::DpadRight);
        k.press(GamepadButton::DpadRight);
        k.press(GamepadButton::DpadUp);
        assert_eq!(k.state().selected, "Space");
        k.press(GamepadButton::DpadRight);
        assert_eq!(k.state().selected, "Backspace");
        assert_eq!(k.press(GamepadButton::South), Some(Press::Backspace));
        k.press(GamepadButton::DpadRight);
        assert_eq!(k.press(GamepadButton::South), Some(Press::Done));
        // West and east are Backspace and Done anywhere.
        assert_eq!(k.press(GamepadButton::West), Some(Press::Backspace));
        assert_eq!(k.press(GamepadButton::East), Some(Press::Done));
    }

    #[test]
    fn the_keys_fit_the_screen_and_every_field_character_has_one() {
        let p = panel();
        assert!(p.x >= 0.0 && p.x + p.width <= 1024.0 && p.y >= 0.0);
        for k in keys() {
            let r = key_rect(&k);
            assert!(r.x >= p.x && r.x + r.width <= p.x + p.width + 0.01);
        }
        for &key in Key::ALL {
            for shift in [false, true] {
                if typed(key, shift).is_some() {
                    assert!(
                        keys().iter().any(|k| k.kind == Kind::Char(key)),
                        "{key:?} has a key"
                    );
                }
            }
        }
    }
}
