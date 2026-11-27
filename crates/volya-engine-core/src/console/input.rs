// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Typing into the open console (docs/behavior/console-input.md): the key
//! events and hold reports it receives (section 1), the characters (2), the
//! editing keys (3), the length (4), key repeat (5), pasting (6), the
//! history (7), completion (8) and scrolling (9).
//!
//! When the console opens and closes, and which receiver gets the keys, is
//! the console window's (docs/behavior/console-window.md): its owner calls
//! [`Console::opened`] and feeds [`Console::type_frame`] only while the
//! console is open. Keys are the key codes of options-screens.md 11.2.

use super::{COMMANDS, Console, Exec};

/// The line holds at most this many characters after any key (section 4).
pub const LINE_LENGTH: usize = 1015;

/// A paste keeps at most this many characters of the clipboard (section 6).
const PASTE_LENGTH: usize = 1023;

/// A repeat comes due each time this much accumulated time has passed
/// (section 5), in seconds.
const REPEAT_PERIOD: f32 = 0.2;
/// The rate grows by this much with every due repeat.
const RATE_STEP: f32 = 0.2;
/// The console's frame time is capped at this many seconds.
pub const FRAME_CAP: f32 = 0.06666;

const ESCAPE: u16 = 1;
const BACKSPACE: u16 = 14;
const TAB: u16 = 15;
const ENTER: u16 = 28;
const GRAVE: u16 = 41;
const LEFT_SHIFT: u16 = 42;
const RIGHT_SHIFT: u16 = 54;
const UP: u16 = 200;
const PAGE_UP: u16 = 201;
const DOWN: u16 = 208;
const PAGE_DOWN: u16 = 209;
const INSERT: u16 = 210;

/// What the clipboard held when Insert read it (section 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clipboard {
    /// It could not be opened: nothing happens.
    Unavailable,
    /// It holds no text: the line is unchanged.
    NoText,
    /// Its plain text.
    Text(String),
}

/// What a key did that is the console's owner's to carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Typed {
    /// A first press of Esc, or of key 41 without Shift: the console closes
    /// if console-window.md section 2's condition holds.
    Close,
    /// Enter executed `line` (the line as typed) with `result`; an
    /// [`Exec::Action`] waits in [`Console::take_actions`] for the owner
    /// to run.
    Entered { line: String, result: Exec },
}

/// The console's typing state.
#[derive(Clone, Debug, Default)]
pub(super) struct InputLine {
    /// The input line.
    line: String,
    shift: bool,
    /// Section 5's accumulated time, rate and "repeat due" mark.
    accumulated: f32,
    rate: f32,
    due: bool,
    /// The time of the console's previous frame (ms).
    previous_frame: Option<u64>,
    /// Section 7's position and last found position (0 or negative).
    position: i64,
    last_found: i64,
    /// Section 9's scroll offset.
    scroll: usize,
}

/// The character key `code` types (section 2's table), with and without
/// Shift; key 41 without Shift is Esc's.
fn character(code: u16, shift: bool) -> Option<char> {
    const DIGITS: &[u8; 10] = b"1234567890";
    const SHIFTED: &[u8; 10] = b"!@#$%^&*()";
    let pair = |plain: u8, shifted: u8| Some(char::from(if shift { shifted } else { plain }));
    match code {
        2..=11 => pair(DIGITS[code as usize - 2], SHIFTED[code as usize - 2]),
        12 => pair(b'-', b'_'),
        13 => pair(b'=', b'+'),
        26 => pair(b'[', b'{'),
        27 => pair(b']', b'}'),
        39 => pair(b';', b':'),
        40 => pair(b'\'', b'"'),
        GRAVE if shift => Some('~'),
        43 => pair(b'\\', b'|'),
        51 => pair(b',', b'<'),
        52 => pair(b'.', b'>'),
        53 => pair(b'/', b'?'),
        16..=25 => Some(char::from(b"qwertyuiop"[code as usize - 16])),
        30..=38 => Some(char::from(b"asdfghjkl"[code as usize - 30])),
        44..=50 => Some(char::from(b"zxcvbnm"[code as usize - 44])),
        57 => Some(' '),
        _ => None,
    }
}

/// Cuts `line` to its first [`LINE_LENGTH`] characters (section 1).
fn cut(line: &mut String) {
    if let Some((at, _)) = line.char_indices().nth(LINE_LENGTH) {
        line.truncate(at);
    }
}

/// A pasted text (section 6): its first 1023 characters, every control
/// character a space, the letters `A` … `Z` lower case. Other characters
/// are kept as they are: what the original does with bytes 0x80 … 0xFF is
/// unestablished (section 10).
fn pasted(text: &str) -> String {
    text.chars()
        .take(PASTE_LENGTH)
        .map(|c| {
            if c.is_control() {
                ' '
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

impl InputLine {
    /// Opening (section 5): the repeat timing starts afresh; the line is
    /// emptied (console-window.md section 1).
    fn opened(&mut self) {
        self.line.clear();
        self.accumulated = 0.0;
        self.rate = 1.0;
    }

    /// A line was executed (section 3): browsing starts again.
    pub(super) fn executing(&mut self) {
        self.scroll = 0;
        self.position = 0;
        self.last_found = 0;
    }

    /// The executed line leaves the line empty (section 3).
    pub(super) fn executed(&mut self) {
        self.line.clear();
    }

    /// The console's frame update (section 5): its frame time is the real
    /// time since its previous frame, capped.
    fn frame(&mut self, now_ms: u64) {
        let dt = match self.previous_frame {
            Some(previous) => (now_ms.saturating_sub(previous) as f32 / 1000.0).min(FRAME_CAP),
            None => 0.0,
        };
        self.previous_frame = Some(now_ms);
        self.accumulated += dt * self.rate;
        if self.accumulated > REPEAT_PERIOD {
            self.accumulated -= REPEAT_PERIOD;
            self.due = true;
            self.rate += RATE_STEP;
        }
    }
}

impl Console {
    /// The input line.
    pub fn line(&self) -> &str {
        &self.input.line
    }

    /// The scroll offset (section 9).
    pub fn scroll(&self) -> usize {
        self.input.scroll
    }

    /// The console opened (console-window.md section 1).
    pub fn opened(&mut self) {
        self.input.opened();
    }

    /// One frame of the open console (section 1): the key presses
    /// (`true`) and releases since the previous frame in order, then a
    /// hold report for every key that is down (`down`, ascending codes),
    /// then the console's frame update at `now_ms` (the real-time clock).
    /// Insert reads `clipboard`. Returns what the keys did that is the
    /// owner's, in order.
    ///
    /// The frame update comes after the hold reports; which comes first in
    /// the original is unestablished (section 10: at most a frame's delay).
    pub fn type_frame(
        &mut self,
        events: &[(u16, bool)],
        down: &[u16],
        now_ms: u64,
        clipboard: &mut dyn FnMut() -> Clipboard,
    ) -> Vec<Typed> {
        let mut out = Vec::new();
        for &(code, pressed) in events {
            if pressed {
                self.input.rate = 1.0;
                out.extend(self.key_action(code, false, clipboard));
                self.input.accumulated = 0.0;
            } else {
                self.input.rate = 1.0;
                self.input.accumulated = 0.0;
                if matches!(code, LEFT_SHIFT | RIGHT_SHIFT) {
                    self.input.shift = false;
                }
            }
        }
        for &code in down {
            if self.input.due {
                out.extend(self.key_action(code, true, clipboard));
            }
        }
        self.input.frame(now_ms);
        out
    }

    /// Key `code`'s action, as a first press or a `repeat` (sections 2, 3);
    /// it clears the "repeat due" mark and ends with the length cut.
    fn key_action(
        &mut self,
        code: u16,
        repeat: bool,
        clipboard: &mut dyn FnMut() -> Clipboard,
    ) -> Option<Typed> {
        let mut out = None;
        match code {
            LEFT_SHIFT | RIGHT_SHIFT => self.input.shift = true,
            ESCAPE => out = (!repeat).then_some(Typed::Close),
            GRAVE if !self.input.shift => out = (!repeat).then_some(Typed::Close),
            BACKSPACE => {
                self.input.line.pop();
            }
            ENTER => {
                let line = self.input.line.clone();
                let result = self.enter(&line);
                out = Some(Typed::Entered { line, result });
            }
            TAB => self.complete(),
            INSERT => match clipboard() {
                Clipboard::Text(text) => self.input.line = pasted(&text),
                Clipboard::Unavailable | Clipboard::NoText => {}
            },
            UP => self.browse(-1),
            DOWN => self.browse(1),
            PAGE_UP => {
                let most = self.log().len().saturating_sub(1);
                self.input.scroll = (self.input.scroll + 1).min(most);
            }
            PAGE_DOWN => self.input.scroll = self.input.scroll.saturating_sub(1),
            code => {
                if let Some(c) = character(code, self.input.shift) {
                    self.input.line.push(c);
                }
            }
        }
        self.input.due = false;
        cut(&mut self.input.line);
        out
    }

    /// Up (`-1`) or Down (`1`) through the history (section 7).
    fn browse(&mut self, step: i64) {
        let position = self.input.position + step;
        let entry = (position < 0)
            .then(|| {
                self.log()
                    .iter()
                    .rev()
                    .filter(|l| l.starts_with('~'))
                    .nth((-position - 1) as usize)
            })
            .flatten();
        match entry {
            Some(entry) => {
                self.input.line = entry.chars().skip(2).collect();
                self.input.position = position;
                self.input.last_found = position;
            }
            None => {
                if position > self.input.last_found {
                    self.input.line.clear();
                }
                self.input.position = self.input.last_found;
            }
        }
    }

    /// Tab (section 8): the first registered name, in byte order, not less
    /// than the text, and a space; after `ra ` only the rest is completed.
    fn complete(&mut self) {
        let keep = if self.input.line.starts_with("ra ") {
            3
        } else {
            0
        };
        let text = &self.input.line.as_bytes()[keep..];
        let name = COMMANDS
            .iter()
            .filter(|d| !d.designer())
            .find(|d| d.name.as_bytes() >= text);
        if let Some(def) = name {
            self.input.line.truncate(keep);
            self.input.line.push_str(def.name);
            self.input.line.push(' ');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ConsoleFiles;
    use super::*;

    const Q: u16 = 16;
    const W: u16 = 17;
    const Z: u16 = 44;
    const C: u16 = 46;
    const F: u16 = 33;
    const G: u16 = 34;

    /// An open console and its clock.
    struct Open {
        c: Console,
        now: u64,
        clipboard: Clipboard,
        down: Vec<u16>,
    }

    impl Open {
        fn new() -> Self {
            let mut c = Console::new(ConsoleFiles::default());
            c.opened();
            Self {
                c,
                now: 0,
                clipboard: Clipboard::Unavailable,
                down: Vec::new(),
            }
        }

        /// One frame `ms` after the previous with `events`.
        fn frame(&mut self, ms: u64, events: &[(u16, bool)]) -> Vec<Typed> {
            self.now += ms;
            for &(code, pressed) in events {
                self.down.retain(|&d| d != code);
                if pressed {
                    self.down.push(code);
                }
            }
            self.down.sort_unstable();
            let clipboard = self.clipboard.clone();
            self.c
                .type_frame(events, &self.down, self.now, &mut || clipboard.clone())
        }

        /// Presses and releases each key in turn, a frame each.
        fn tap(&mut self, codes: &[u16]) -> Vec<Typed> {
            let mut out = Vec::new();
            for &code in codes {
                out.extend(self.frame(10, &[(code, true)]));
                out.extend(self.frame(10, &[(code, false)]));
            }
            out
        }

        fn shifted(&mut self, codes: &[u16]) {
            self.frame(10, &[(LEFT_SHIFT, true)]);
            self.tap(codes);
            self.frame(10, &[(LEFT_SHIFT, false)]);
        }

        /// Types `text` (lower-case letters, digits, space) and Enter.
        fn enter(&mut self, text: &str) -> Vec<Typed> {
            let codes: Vec<u16> = text.bytes().map(code_of).collect();
            self.tap(&codes);
            self.tap(&[ENTER])
        }

        fn line(&self) -> &str {
            self.c.line()
        }
    }

    fn code_of(b: u8) -> u16 {
        (0..256)
            .find(|&code| character(code, false) == Some(char::from(b)))
            .unwrap()
    }

    fn entered(typed: &[Typed]) -> Vec<String> {
        typed
            .iter()
            .filter_map(|t| match t {
                Typed::Entered { line, .. } => Some(line.clone()),
                Typed::Close => None,
            })
            .collect()
    }

    #[test]
    fn the_character_table() {
        let mut o = Open::new();
        // Measured: `Cfg` gave `cfg`, `~QW` gave `~qw` (section 2).
        o.frame(10, &[(LEFT_SHIFT, true)]);
        o.tap(&[C]);
        o.frame(10, &[(LEFT_SHIFT, false)]);
        o.tap(&[F, G]);
        assert_eq!(o.line(), "cfg");
        o.tap(&[BACKSPACE, BACKSPACE, BACKSPACE, BACKSPACE]);
        o.shifted(&[GRAVE, Q, W]);
        assert_eq!(o.line(), "~qw");
        o.tap(&[BACKSPACE; 3]);
        let all: Vec<u16> = (2..=13).chain([26, 27, 39, 40, 43, 51, 52, 53]).collect();
        o.tap(&all);
        o.shifted(&all);
        assert_eq!(o.line(), "1234567890-=[];'\\,./!@#$%^&*()_+{}:\"|<>?");
        o.tap(&[BACKSPACE; 40]);
        assert_eq!(o.line(), "");
        let letters: Vec<u16> = (16..=25)
            .chain(30..=38)
            .chain(44..=50)
            .chain([57])
            .collect();
        o.shifted(&letters);
        assert_eq!(o.line(), "qwertyuiopasdfghjklzxcvbnm ");
    }

    #[test]
    fn keys_without_an_action_do_nothing() {
        // Measured: `y`, keypad 1, keypad Enter, F1, Home, Left, Delete,
        // Ctrl, Alt and Caps Lock twice, `x`, Enter executed `yx`.
        let mut o = Open::new();
        let mut keys = vec![21, 79, 156, 59, 199, 203, 211, 29, 56, 58, 58, 45];
        keys.extend([ENTER]);
        let out = o.tap(&keys);
        assert_eq!(entered(&out), ["yx"]);
        assert_eq!(o.c.log().last().unwrap(), "! Unknown command:  yx");
    }

    #[test]
    fn shift_follows_either_key() {
        let mut o = Open::new();
        o.frame(10, &[(LEFT_SHIFT, true), (RIGHT_SHIFT, true)]);
        o.frame(10, &[(RIGHT_SHIFT, false)]);
        o.tap(&[2]);
        assert_eq!(o.line(), "1", "releasing one turns it off");
        // Opening and closing leave it as it is.
        o.frame(10, &[(RIGHT_SHIFT, true)]);
        o.c.opened();
        o.tap(&[2]);
        assert_eq!(o.line(), "!");
    }

    #[test]
    fn escape_and_grave_close_on_a_first_press_only() {
        let mut o = Open::new();
        assert_eq!(o.tap(&[ESCAPE, GRAVE]), [Typed::Close, Typed::Close]);
        // Held: repeats do not close.
        let mut out = o.frame(10, &[(ESCAPE, true)]);
        for _ in 0..40 {
            out.extend(o.frame(30, &[]));
        }
        assert_eq!(out, [Typed::Close]);
        o.frame(10, &[(ESCAPE, false)]);
        assert_eq!(o.line(), "");
        // With Shift, key 41 types `~` and repeats it.
        o.frame(10, &[(RIGHT_SHIFT, true)]);
        assert!(o.frame(10, &[(GRAVE, true)]).is_empty());
        for _ in 0..20 {
            o.frame(30, &[]);
        }
        assert!(o.line().len() > 2 && o.line().bytes().all(|b| b == b'~'));
    }

    #[test]
    fn the_line_is_cut_after_every_key() {
        let mut o = Open::new();
        o.c.input.line = "a".repeat(1100);
        o.tap(&[29]);
        assert_eq!(o.line().len(), LINE_LENGTH, "a key without an action too");
        o.tap(&[Q]);
        assert_eq!(
            o.line(),
            "a".repeat(LINE_LENGTH),
            "a full line takes nothing"
        );
    }

    #[test]
    fn enter_executes_and_empties_the_line() {
        let mut o = Open::new();
        let out = o.enter("mouse sens 0.5");
        assert_eq!(entered(&out), ["mouse sens 0.5"]);
        assert_eq!(o.line(), "");
        assert_eq!(o.c.log()[o.c.log().len() - 2], "~ mouse sens 0.5");
        // An executed line of the programs shares the line and resets the
        // browsing (section 3).
        o.tap(&[Q, PAGE_UP, UP]);
        assert_eq!(o.c.scroll(), 1);
        o.c.exec("flush");
        assert_eq!(o.line(), "");
        assert_eq!(o.c.scroll(), 0);
        assert_eq!(o.c.input.position, 0);
    }

    /// Repeats of a key held from frame 0, frames of `ms`: the frame
    /// numbers its character was typed in.
    fn repeats(ms: u64, frames: usize) -> Vec<usize> {
        let mut o = Open::new();
        let mut at = Vec::new();
        o.frame(ms, &[(Q, true)]);
        for f in 1..=frames {
            let before = o.line().len();
            o.frame(ms, &[]);
            if o.line().len() > before {
                at.push(f);
            }
        }
        at
    }

    #[test]
    fn repeats_speed_up() {
        // 15 ms frames: due when the accumulated time exceeds 0.2 s, used
        // by the next frame's hold report; then 0.2 / 1.2 s, 0.2 / 1.4 s, …
        let at = repeats(15, 80);
        let gaps: Vec<usize> = at.windows(2).map(|w| w[1] - w[0]).collect();
        assert_eq!(at[0], 15);
        assert_eq!(&gaps[..3], [11, 10, 8]);
        // Under 15 frames a second the cap slows them.
        let slow = repeats(200, 10);
        assert_eq!(&slow[..2], [5, 7]);
    }

    #[test]
    fn the_lowest_held_key_takes_the_repeat() {
        // Measured: with left Shift held, a held `z` typed one `z` and a
        // held `q` repeated.
        let mut o = Open::new();
        o.frame(10, &[(LEFT_SHIFT, true)]);
        o.frame(10, &[(Z, true)]);
        for _ in 0..60 {
            o.frame(10, &[]);
        }
        assert_eq!(o.line(), "z");
        o.frame(10, &[(Z, false)]);
        o.frame(10, &[(Q, true)]);
        for _ in 0..60 {
            o.frame(10, &[]);
        }
        assert!(o.line().len() > 3 && o.line()[1..].bytes().all(|b| b == b'q'));
    }

    #[test]
    fn a_due_repeat_survives_closing() {
        let mut o = Open::new();
        o.frame(15, &[(Q, true)]);
        // Due after 14 frames, not yet used: the console closes.
        for _ in 0..13 {
            o.frame(15, &[]);
        }
        o.now += 15;
        o.c.input.frame(o.now);
        assert!(o.c.input.due);
        o.c.opened();
        assert_eq!(o.line(), "", "opening empties the line");
        o.frame(10, &[]);
        assert_eq!(o.line(), "q", "used by the first hold report");
        // A key press first clears it.
        o.c.input.due = true;
        o.frame(10, &[(W, true)]);
        assert_eq!(o.line(), "qw");
    }

    #[test]
    fn paste_replaces_the_line() {
        // Measured: `AbC`, tab, `x`, CR, LF, `Y~`; `zzz`, Insert, Enter
        // executed `abc x y~`.
        let mut o = Open::new();
        o.clipboard = Clipboard::Text("AbC\tx\r\nY~".to_owned());
        o.tap(&[Z, Z, Z, INSERT]);
        assert_eq!(o.line(), "abc x  y~");
        let out = o.tap(&[ENTER]);
        assert_eq!(o.c.log().last().unwrap(), "! Unknown command:  abc");
        assert_eq!(entered(&out), ["abc x  y~"]);
        // No text, or no clipboard: unchanged.
        o.tap(&[Q]);
        o.clipboard = Clipboard::NoText;
        o.tap(&[INSERT]);
        o.clipboard = Clipboard::Unavailable;
        o.tap(&[INSERT]);
        assert_eq!(o.line(), "q");
        // At most 1023 characters, then the cut.
        o.clipboard = Clipboard::Text("Я".repeat(2000));
        o.tap(&[INSERT]);
        assert_eq!(o.line().chars().count(), LINE_LENGTH);
    }

    #[test]
    fn history_browses_the_tilde_lines() {
        let mut o = Open::new();
        // With no entries Up keeps the line and Down empties it.
        o.tap(&[Q, UP]);
        assert_eq!(o.line(), "q");
        o.tap(&[DOWN]);
        assert_eq!(o.line(), "");
        o.enter("a");
        o.enter("b");
        o.c.log_mut().write("not an entry");
        o.enter("c");
        // Up past the oldest keeps it.
        o.tap(&[UP]);
        assert_eq!(o.line(), "c");
        o.tap(&[UP, UP, UP, UP]);
        assert_eq!(o.line(), "a");
        // Measured: Up, Down, Up gives the second newest.
        o.tap(&[ENTER]);
        o.tap(&[UP, DOWN]);
        assert_eq!(o.line(), "");
        o.tap(&[UP]);
        assert_eq!(o.line(), "c", "the newest after the second Enter is `a`");
        // Down from the line being typed loses it.
        o.tap(&[ENTER, Q, DOWN]);
        assert_eq!(o.line(), "");
        // Closing and opening keep the position (measured: Up, Esc, key
        // 41, Up gives the second newest).
        o.tap(&[UP, ESCAPE]);
        o.c.opened();
        o.tap(&[UP]);
        assert_eq!(o.line(), "a");
        // Other `~` lines are entries; their text is from the third
        // character, as written.
        o.c.exec("mouse_sens x");
        o.tap(&[UP]);
        assert_eq!(
            o.line(),
            "Valid arguments: float value in range [0.050,0.600]"
        );
        o.tap(&[UP]);
        assert_eq!(o.line(), "Invalid syntax in call to 'mouse_sens'");
    }

    #[test]
    fn tab_completes_by_byte_order() {
        let mut o = Open::new();
        let tab = |o: &mut Open, line: &str| {
            o.c.input.line = line.to_owned();
            o.tap(&[TAB]);
            o.line().to_owned()
        };
        assert_eq!(tab(&mut o, ""), "_preset ");
        assert_eq!(tab(&mut o, "cfg"), "cfg_load ");
        assert_eq!(tab(&mut o, "zz"), "zz");
        assert_eq!(tab(&mut o, "bind q"), "bind_console ");
        assert_eq!(tab(&mut o, "hud_crosshair"), "hud_crosshair ");
        assert_eq!(tab(&mut o, "hud_crosshair "), "hud_crosshair_dist ");
        assert_eq!(tab(&mut o, "ra cfg"), "ra cfg_load ");
    }

    #[test]
    fn scrolling_stays_within_the_log() {
        let mut o = Open::new();
        o.tap(&[PAGE_DOWN]);
        assert_eq!(o.c.scroll(), 0);
        o.c.log_mut().write("a\nb");
        let lines = o.c.log().len();
        o.tap(&vec![PAGE_UP; lines + 3]);
        assert_eq!(o.c.scroll(), lines - 1);
        o.tap(&[PAGE_DOWN]);
        assert_eq!(o.c.scroll(), lines - 2);
        o.tap(&[PAGE_UP]);
        o.enter("");
        assert_eq!(o.c.scroll(), 0, "any executed line resets it");
    }
}
