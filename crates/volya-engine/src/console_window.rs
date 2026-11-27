// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The console window (docs/behavior/console-window.md): whether the
//! console is open (sections 1, 2), its cursor blink (4.3) and its look
//! (section 4): the cleared area, the prompt line and the log's rows. What
//! it does to the receivers below is the app's ([`crate::app`]); typing is
//! the console's own ([`crate::core::console::Console::type_frame`]).

use std::io;

use crate::core::console::{self, Console, Typed};
use crate::core::input::{Action, Control, Event, Input, Key, TouchButton};
use crate::core::vfs::Vfs;
use crate::data::{Assets, GameData};
use crate::scene::{Scene, Sprite, TextureId};
use volya_engine_formats::cp1251;
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::text::layout::draw_line;
use volya_engine_formats::ui::Color;
use volya_engine_formats::ui::layout::HAlign;

/// The layer the console's area and glyphs are drawn in.
pub const LAYER: &str = "console";

/// The area's colour (4.1).
const AREA: Color = Color::rgba(32, 32, 32, 255);
/// The prompt line's colour (4.3).
const PROMPT: Color = Color::rgba(128, 128, 255, 255);
/// The cursor's phase toggles each time this much of the console's clock
/// has accumulated (4.3), in seconds.
const BLINK: f32 = 0.1;
/// The console's frame time is capped at this many seconds (4.3).
const FRAME_CAP: f32 = 0.06666;
/// A log line wider than this many pixels is broken (4.6).
const BREAK_WIDTH: f32 = 1024.0;
/// One row's step in normalised screen space (4.3, 4.4).
const ROW: f32 = 0.05;

/// The console's window state.
#[derive(Clone, Debug, Default)]
pub struct ConsoleWindow {
    open: bool,
    /// The console was open when the main menu opened: it opens again when
    /// the menu closes (sections 1, 2).
    remembered: bool,
    /// The cursor's phase: shown (`true`) or hidden.
    shown: bool,
    /// The blink's accumulated time, seconds.
    blink: f32,
    /// The time of the console's previous frame (ms).
    previous_frame: Option<u64>,
    /// Where the cursor of the receiver below was when the console took
    /// input: mouse movement reaches the console and is lost (3.2).
    cursor: [f32; 2],
}

impl ConsoleWindow {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the console (section 1, "What opening does"): `false` when it
    /// was open already, else the input line is emptied and the repeat
    /// timing restarts. The cursor's phase continues where it was.
    pub fn open(&mut self, console: &mut Console) -> bool {
        if self.open {
            return false;
        }
        self.open = true;
        console.opened();
        true
    }

    /// Closes the console (section 2, "What closing does"): `false` when it
    /// was closed already. The input line is kept.
    pub fn close(&mut self) -> bool {
        std::mem::replace(&mut self.open, false)
    }

    /// The main menu opens: whether the console was open is remembered,
    /// then it is closed (section 2).
    pub fn menu_opened(&mut self) {
        self.remembered = self.open;
        self.close();
    }

    /// The main menu closes: the console opens if it was open when the menu
    /// opened (section 1). Returns whether it opened.
    pub fn menu_closed(&mut self, console: &mut Console) -> bool {
        std::mem::take(&mut self.remembered) && self.open(console)
    }

    /// A level starts: the menu's remembered console is closed with it
    /// (section 2, `start`).
    pub fn level_started(&mut self) {
        self.remembered = false;
        self.close();
    }

    /// The open console's frame update at `now_ms` of the engine's
    /// unpaused clock: the cursor blink (4.3).
    pub fn frame(&mut self, now_ms: u64) {
        let dt = match self.previous_frame {
            Some(previous) => (now_ms.saturating_sub(previous) as f32 / 1000.0).min(FRAME_CAP),
            None => 0.0,
        };
        self.previous_frame = Some(now_ms);
        self.blink += dt;
        if self.blink > BLINK {
            self.blink -= BLINK;
            self.shown = !self.shown;
        }
    }

    /// Whether the cursor is in its visible phase.
    pub fn cursor_shown(&self) -> bool {
        self.shown
    }

    /// Adds the open console to `scene` (section 4): full height unless
    /// `half` (a level loaded or the main menu active, 4.1).
    pub fn draw(
        &self,
        tree: &dyn Vfs,
        data: &GameData,
        assets: &mut Assets,
        console: &Console,
        scene: &mut Scene,
        half: bool,
    ) -> io::Result<()> {
        if !self.open {
            return Ok(());
        }
        let (w, h) = (scene.resolution.width, scene.resolution.height);
        let mut font = assets.fonts(tree, data, h)?.get(UiFont::Di).clone();
        // Created with the screen-relative size flag only (4.2): one colour
        // per glyph; its height set every frame.
        font.gradient = false;
        font.height = h as f32 * 0.025;
        let path = Assets::font_texture(data, &font.texture)?;
        let size = assets.image_size(tree, &path)?;
        let area_height = if half { h / 2 } else { h };
        scene.sprites.push(Sprite {
            layer: LAYER.into(),
            texture: TextureId::White,
            corners: [
                [-0.5, -0.5],
                [w as f32 - 0.5, -0.5],
                [w as f32 - 0.5, area_height as f32 - 0.5],
                [-0.5, area_height as f32 - 0.5],
            ],
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            color: AREA,
            corner_colors: None,
        });
        let width = |text: &[u8]| font.width(text);
        let mut texts = vec![prompt_row(console.line(), self.shown, half)];
        texts.extend(log_rows(console.log(), console.scroll(), half, width));
        for row in texts {
            let pen = (0.0, pixel_y(row.y, h));
            for g in draw_line(&font, &row.text, pen, row.color, HAlign::Left) {
                scene.sprites.push(Sprite::glyph(
                    LAYER,
                    TextureId::Image { path: path.clone() },
                    &g,
                    size,
                ));
            }
        }
        Ok(())
    }
}

/// The input the receiver below gets when it loses input (3.3): a release
/// of every key and mouse button `input` holds.
fn lost(input: &Input) -> Input {
    let mut out = quiet(input);
    for c in input.held() {
        if input.device_held(c) {
            out.handle(press(c));
        }
    }
    out.end_frame();
    out.release_all();
    out
}

/// No input: what the receiver below gets while the console has input
/// (3.2), its cursor where it is.
fn quiet(input: &Input) -> Input {
    let mut out = Input::new();
    out.set_screen(input.screen());
    out.set_sensitivity(input.sensitivity());
    out.place_cursor(input.cursor());
    out
}

/// A press of key or mouse button `c` (no double click).
fn press(c: Control) -> Event {
    match c {
        Control::Key(key) => Event::Key { key, pressed: true },
        Control::Mouse(button) => Event::PointerButton {
            at: None,
            button,
            pressed: true,
            time: f64::NEG_INFINITY,
        },
        Control::Pad(_) | Control::Touch(_) => unreachable!("not a device control"),
    }
}

/// Whether this frame has a press of a keyboard key bound to `console`, or
/// of the touch screen's Console button, which does what such a key does
/// (our design, #642).
fn console_key_pressed(console: &Console, input: &Input) -> bool {
    let action = console::action_index("console").expect("console action");
    let keys: Vec<Key> = (console.keys(action).into_iter().flatten())
        .filter_map(Key::from_dik_name)
        .collect();
    let button = Control::Touch(TouchButton::Console);
    input
        .device_changes()
        .iter()
        .any(|&(c, pressed)| pressed && matches!(c, Control::Key(k) if keys.contains(&k)))
        || input
            .actions()
            .iter()
            .any(|a| matches!(a, Action::Press { control, .. } if *control == button))
}

impl ConsoleWindow {
    /// The console's part of a frame of the engine's unpaused clock `now`
    /// in which the receiver below it is the level or the main menu
    /// (sections 1 – 3). A key bound to `console` opens it when `may_open`;
    /// while it is open it takes every key (`input` is the frame's), the
    /// mouse is lost and Esc or key 41 closes it (Shift off by the
    /// releases of 3.4). Returns the input the receiver below gets this
    /// frame, `None` for the frame's own: the releases of 3.3 as the
    /// console opens, nothing while it is open, and as it closes a press of
    /// every held key or button of `resume` (3.4: the level's movement keys;
    /// the menu's none).
    pub fn receiver_frame(
        &mut self,
        console: &mut Console,
        input: &mut Input,
        now: u64,
        may_open: bool,
        resume: &[Control],
    ) -> Option<Input> {
        if !self.open {
            if !(may_open && console_key_pressed(console, input)) {
                return None;
            }
            self.open(console);
            self.cursor = input.cursor();
            return Some(lost(input));
        }
        input.place_cursor(self.cursor);
        let (events, down) = (input.key_events(), input.keys_down());
        let typed = console.type_frame(&events, &down, now, &mut || input.take_clipboard());
        self.frame(now);
        // A level or the main menu is below whenever the console is open
        // here, so Esc and key 41 always close it (section 2).
        if !typed.iter().any(|t| matches!(t, Typed::Close)) {
            return Some(quiet(input));
        }
        self.close();
        // Losing input: the releases of the held keys (3.4), which turn
        // Shift off (console-input.md section 2).
        let released: Vec<(u16, bool)> = down.iter().map(|&k| (k, false)).collect();
        console.type_frame(&released, &[], now, &mut || console::Clipboard::Unavailable);
        let mut out = quiet(input);
        for &c in resume {
            if input.device_held(c) {
                out.handle(press(c));
            }
        }
        Some(out)
    }
}

/// A row of text: its bytes in the font's encoding, colour and *y* in
/// normalised screen space.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub text: Vec<u8>,
    pub color: Color,
    pub y: f32,
}

/// The pixel row of normalised *y* on a screen `h` pixels high.
fn pixel_y(y: f32, h: u32) -> f32 {
    ((y + 1.0) * h as f32 / 2.0).floor()
}

/// The top's *y*₀ (conventions): 1 at full height, 0 at half.
fn top(half: bool) -> f32 {
    if half { 0.0 } else { 1.0 }
}

/// `text` in the font's encoding; a character Windows-1251 lacks is `?`
/// (our choice: the console's text is the original's bytes).
fn bytes(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| cp1251::encode_char(c).unwrap_or(b'?'))
        .collect()
}

/// The prompt line (4.3): `>>> `, the input line and `|` in the cursor's
/// visible phase.
pub fn prompt_row(line: &str, cursor: bool, half: bool) -> Row {
    let mut text = b">>> ".to_vec();
    text.extend(bytes(line));
    if cursor {
        text.push(b'|');
    }
    Row {
        text,
        color: PROMPT,
        y: top(half) - ROW,
    }
}

/// A log line's colour and the text drawn (4.5). A marked line shorter
/// than three characters draws nothing (our choice: a gap of the spec).
fn marked(line: &[u8]) -> (Color, &[u8]) {
    let color = match line.first() {
        Some(b'!') => Color::rgba(255, 0, 0, 255),
        Some(b'#') => Color::rgba(0, 222, 205, 145),
        Some(b'*') => Color::rgba(128, 128, 128, 255),
        Some(b'-') => Color::rgba(0, 255, 0, 255),
        Some(b'~') => Color::rgba(255, 255, 0, 255),
        _ => return (Color::rgba(255, 255, 255, 255), line),
    };
    (color, line.get(2..).unwrap_or_default())
}

/// The rows a log line's drawn text takes, top to bottom (4.6).
pub fn broken(text: &[u8], width: &dyn Fn(&[u8]) -> f32) -> Vec<Vec<u8>> {
    let mut rows = Vec::new();
    let mut rest = text;
    loop {
        if width(rest) <= BREAK_WIDTH {
            rows.push(rest.to_vec());
            return rows;
        }
        // The smallest k whose first k characters are wider than 1024;
        // the k-th starts the next row too.
        let k = (1..=rest.len())
            .find(|&k| width(&rest[..k]) > BREAK_WIDTH)
            .unwrap_or(rest.len());
        rows.push(rest[..k].to_vec());
        rest = &rest[k - 1..];
    }
}

/// The log's rows above the prompt (4.4): from the newest line less the
/// scroll offset backwards, the newest at *y*₀ − 0.15, each older 0.05
/// higher, while the line's row is not above −1 (single precision).
pub fn log_rows(
    log: &[String],
    scroll: usize,
    half: bool,
    width: impl Fn(&[u8]) -> f32,
) -> Vec<Row> {
    let mut out = Vec::new();
    let mut y = top(half) - 3.0 * ROW;
    for line in log.iter().rev().skip(scroll) {
        if y < -1.0 {
            break;
        }
        let line = bytes(line);
        let (color, text) = marked(&line);
        let rows = broken(text, &width);
        let n = rows.len();
        for (i, text) in rows.into_iter().enumerate() {
            out.push(Row {
                text,
                color,
                y: y - (n - 1 - i) as f32 * ROW,
            });
        }
        y -= n as f32 * ROW;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every byte 9 pixels wide, as the font's space and `a` (4.2).
    fn nine(text: &[u8]) -> f32 {
        9.0 * text.len() as f32
    }

    fn log(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("line {i}")).collect()
    }

    #[test]
    fn full_height_has_37_rows_and_half_17() {
        for (half, rows, first, last) in [(false, 37, 0.85, -0.95), (true, 17, -0.15, -0.95)] {
            let r = log_rows(&log(100), 0, half, nine);
            assert_eq!(r.len(), rows, "half {half}");
            assert!((r[0].y - first).abs() < 1e-5);
            assert!((r[rows - 1].y - last).abs() < 1e-5);
            assert_eq!(r[0].text, b"line 99");
        }
        // At 768 rows the topmost starts one glyph height (19.2) down.
        let r = log_rows(&log(100), 0, false, nine);
        assert_eq!(pixel_y(r[36].y, 768), 19.0);
        assert_eq!(pixel_y(prompt_row("", false, false).y, 768), 748.0);
    }

    #[test]
    fn the_scroll_offset_skips_the_newest_lines() {
        let r = log_rows(&log(5), 2, true, nine);
        let texts: Vec<_> = r.iter().map(|r| r.text.clone()).collect();
        assert_eq!(
            texts,
            [b"line 2".to_vec(), b"line 1".to_vec(), b"line 0".to_vec()]
        );
    }

    #[test]
    fn marked_lines_take_their_colour_and_lose_two_characters() {
        let lines = ["! bad", "~ typed", "* grey", "- ok", "# info", "plain", "!"];
        let r = log_rows(&lines.map(String::from), 0, false, nine);
        assert_eq!(r[6].text, b"bad");
        assert_eq!(r[6].color, Color::rgba(255, 0, 0, 255));
        assert_eq!(r[5].text, b"typed");
        assert_eq!(r[5].color, Color::rgba(255, 255, 0, 255));
        assert_eq!(r[2].color, Color::rgba(0, 222, 205, 145));
        assert_eq!(
            (r[1].text.as_slice(), r[1].color),
            (&b"plain"[..], Color::rgba(255, 255, 255, 255))
        );
        assert!(r[0].text.is_empty());
    }

    #[test]
    fn long_lines_break_with_the_breaking_character_twice() {
        let rows = |n: usize| -> Vec<usize> {
            broken(&vec![b'a'; n], &nine).iter().map(Vec::len).collect()
        };
        assert_eq!(rows(200), [114, 87]);
        assert_eq!(rows(300), [114, 114, 74]);
        assert_eq!(rows(113), [113]);
        // A broken newest line: its rows top to bottom, the older line above.
        let lines = ["old".to_string(), "a".repeat(200)];
        let r = log_rows(&lines, 0, true, nine);
        assert_eq!(r.len(), 3);
        assert!((r[1].y - -0.15).abs() < 1e-5 && (r[0].y - -0.20).abs() < 1e-5);
        assert_eq!((r[0].text.len(), r[1].text.len()), (114, 87));
        assert_eq!(r[2].text, b"old");
        assert!((r[2].y - -0.25).abs() < 1e-5);
    }

    #[test]
    fn the_prompt_shows_the_cursor_in_its_visible_phase() {
        assert_eq!(prompt_row("cfg", true, true).text, b">>> cfg|");
        assert_eq!(prompt_row("cfg", false, true).text, b">>> cfg");
        assert!((prompt_row("", false, true).y - -0.05).abs() < 1e-6);
    }

    #[test]
    fn the_cursor_toggles_every_tenth_of_a_second_of_capped_frames() {
        let mut w = ConsoleWindow::default();
        let phases: Vec<bool> = [0, 50, 101, 150, 202, 1000, 1067]
            .into_iter()
            .map(|t| {
                w.frame(t);
                w.cursor_shown()
            })
            .collect();
        // 0.05, 0.101 (toggle), 0.15, 0.202 (toggle), +0.06666 capped,
        // +0.06666 (toggle at 0.2353 – 0.2 … ).
        assert_eq!(phases, [false, false, true, true, false, false, true]);
    }

    #[test]
    fn the_menu_remembers_an_open_console() {
        let mut console = Console::new(Default::default());
        let mut w = ConsoleWindow::default();
        assert!(w.open(&mut console) && !w.open(&mut console));
        w.menu_opened();
        assert!(!w.is_open());
        assert!(w.menu_closed(&mut console) && w.is_open());
        w.menu_opened();
        w.level_started();
        assert!(!w.menu_closed(&mut console) && !w.is_open());
    }
}
