// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The talk screen (docs/behavior/talk-window-input.md, cited by section;
//! DG-6): the in-game window holding the talk window ([`TalkWindow`]) and
//! the trade window, its showing with the talk's set-up (1.3), its input
//! (3), the refill mark (4.3), its closing (5) and the information portions
//! it gives (6). What the talk is, its sides and dialogs, is the level's
//! ([`crate::level_talk`], DG-3); the app runs the screen's update of 4 with
//! them.
//!
//! **Input.** A mouse window drawing the game's cursor: the key bound to
//! `use` and the key bound to `quit` close it (3.1, 3.2), a click on a
//! question takes its choice (message 46), a click on the trade button asks
//! for trade (message 45), the wheel, a drag and the scroll bars scroll the
//! two lists (3.4, 3.5); no key reaches the character (3.1 item 5). Touch
//! and the gamepad are our design, as for the PDA ([`crate::pda_window`]):
//! the standard touch controls' stick and Click, or the gamepad's left
//! stick and south, move the cursor and click (#856, #835); the touch
//! controls' Menu and the gamepad's east are the key bound to `quit`, and
//! the gamepad's d-pad right, its `use` in the level, the key bound to
//! `use`. So each choice of section 9 has both paths.
//!
//! **What waits for other systems**: the trade window and what a switch to
//! trade does (DG-10, #908): the trade button's message is reported and
//! nothing more happens; the trader's sounds (`set_trader_sound`, R 6).

use std::io;

use crate::core::input::Key;
use crate::data::GameData;
use crate::dialog::{self, Ev};
use crate::draw::Ctx;
use crate::options::widgets::{ARROW, REPEAT_MS};
use crate::scene::Scene;
use volya_engine_formats::ui::layout::Rect;

use super::{Character, TalkState, TalkWindow};

/// The portion given when the talk window is shown (6).
pub const TALK_SHOWN: &str = "ui_talk";
/// The portion given when the trade window is hidden (6).
pub const TRADE_HIDDEN: &str = "ui_trade_hide";
/// A frame counts at most this, seconds (as the inventory screen's).
const MAX_FRAME: f32 = 0.1;

/// The keys the screen reads (3.1, 3.2), from the effective bindings.
#[derive(Clone, Debug, Default)]
pub struct Keys {
    pub use_keys: Vec<Key>,
    pub quit: Vec<Key>,
}

/// A phrase said, for the answer log (talk-window.md 6.1): the speaker's
/// name and the phrase's text (a string id or a literal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub speaker: String,
    pub text: String,
}

/// The talk start the level made (docs/behavior/talk.md 3 steps 1–2): the
/// speakers, the partner's special mode, the phrases said.
#[derive(Clone, Debug, PartialEq)]
pub struct TalkStart {
    pub player: Character,
    pub partner: Character,
    pub special_mode: bool,
    pub said: Vec<Said>,
}

/// What a frame's input asks of the talk (3.4): message 46 with the
/// choice's index, message 45.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Asked {
    pub choice: Option<usize>,
    pub trade: bool,
    /// The screen closed (3.1, 3.2).
    pub closed: bool,
}

/// An element a left press armed (3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Armed {
    Question(usize),
    Trade,
}

/// What the held left button does to a list (options-screens.md 4.7, 4.8).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    /// A drag over list `i`'s view, from the cursor's y.
    View(usize, f32),
    /// List `i`'s box, held this far below its top.
    Box(usize, f32),
    /// List `i`'s arrow (`true` up), the last step at this clock.
    Arrow(usize, bool, u64),
}

/// The talk screen.
pub struct TalkScreen {
    pub window: TalkWindow,
    open: bool,
    enabled: bool,
    /// The partner of the last talk start.
    partner: Option<u16>,
    /// The question list is marked for a refill (4.3).
    refill: bool,
    cursor: [f32; 2],
    armed: Option<Armed>,
    grab: Option<Grab>,
    last_update: Option<u64>,
    portions: Vec<&'static str>,
}

impl TalkScreen {
    pub fn new(window: TalkWindow) -> Self {
        Self {
            window,
            open: false,
            enabled: false,
            partner: None,
            refill: false,
            cursor: [512.0, 384.0],
            armed: None,
            grab: None,
            last_update: None,
            portions: Vec::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The partner of the talk the screen was set up for.
    pub fn partner(&self) -> Option<u16> {
        self.partner
    }

    /// Whether the partner has the special mode (talk-window.md 9).
    pub fn special_mode(&self) -> bool {
        self.window.special_mode
    }

    /// The information portions given since the last call (6).
    pub fn take_portions(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.portions)
    }

    /// Showing the screen (1.3) with the cursor at `cursor`: with the
    /// level's talk start (the player character talking), the talk window's
    /// start, the phrases said, the refill mark, the talk window shown
    /// (`ui_talk`) and the trade window hidden (`ui_trade_hide`); then the
    /// screen shown and enabled. The caller runs one update at once.
    pub fn show(
        &mut self,
        data: &GameData,
        start: Option<TalkStart>,
        partner: u16,
        cursor: [f32; 2],
    ) -> io::Result<()> {
        if let Some(t) = start {
            self.partner = Some(partner);
            self.window
                .start(data, &t.player, &t.partner, t.special_mode);
            self.say(data, &t.said)?;
            self.refill = true;
            self.window.set_shown(true);
            self.portions.extend([TALK_SHOWN, TRADE_HIDDEN]);
        }
        self.open = true;
        self.enabled = true;
        self.cursor = cursor;
        self.armed = None;
        self.grab = None;
        self.last_update = None;
        self.window.set_looks(None, "e");
        Ok(())
    }

    /// Closing (5 items 2, 4): the screen disabled and hidden, the trade
    /// window hidden (`ui_trade_hide`). The talk window stays shown (no
    /// `ui_talk_hide`). The caller forgets the dialog and stops the player
    /// character's side (item 3).
    pub fn close(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        self.enabled = false;
        self.armed = None;
        self.grab = None;
        self.portions.push(TRADE_HIDDEN);
    }

    /// The phrases said, added to the answer log (talk-window.md 6.1): an
    /// empty text adds nothing (dialog-files.md 7).
    pub fn say(&mut self, data: &GameData, said: &[Said]) -> io::Result<()> {
        for s in said.iter().filter(|s| !s.text.is_empty()) {
            self.window.add_phrase(data, &s.speaker, &s.text)?;
        }
        Ok(())
    }

    /// Marks the question list for a refill (4.3).
    pub fn mark_refill(&mut self) {
        self.refill = true;
    }

    /// Takes the refill mark (4.2): the caller refills the list.
    pub fn take_refill(&mut self) -> bool {
        std::mem::take(&mut self.refill)
    }

    /// The frame time of this update at engine clock `now`, seconds.
    pub fn frame_time(&mut self, now: u64) -> f32 {
        let dt = now.saturating_sub(self.last_update.unwrap_or(now)) as f32 / 1000.0;
        self.last_update = Some(now);
        dt.min(MAX_FRAME)
    }

    /// The gamepad cursor's targets: the question captions shown and the
    /// trade button.
    pub fn cursor_targets(&self) -> Vec<Rect> {
        let w = &self.window;
        (0..w.question_count())
            .filter_map(|i| w.question_rect(i))
            .chain(w.button_rect())
            .collect()
    }

    /// One frame's input while the screen is open (3): `paused` the game's
    /// pause, `use_pressed` a press of the key bound to `use` (or the
    /// gamepad's d-pad right), `now` the engine clock for the arrows'
    /// repeat.
    pub fn input(
        &mut self,
        data: &GameData,
        input: &crate::core::input::Input,
        keys: &Keys,
        use_pressed: bool,
        paused: bool,
        now: u64,
    ) -> Asked {
        let mut asked = Asked::default();
        if !self.open {
            return asked;
        }
        let special = self.window.special_mode;
        // 3.1 item 1: `use` closes the screen, also while paused; with a
        // special-mode partner it does nothing. Either way it is taken.
        if use_pressed && !special {
            self.close();
            asked.closed = true;
            return asked;
        }
        for e in dialog::events(data, input) {
            match e {
                // 3.2: not taken by the screen (always, but with a
                // special-mode partner while the game runs), `quit` closes
                // it; the menu does not open.
                Ev::Quit { pressed: true } => {
                    if paused || !special {
                        self.close();
                        asked.closed = true;
                        return asked;
                    }
                }
                Ev::Key { key, pressed: true } if keys.use_keys.contains(&key) => {}
                // 3.1 item 2, 3.4: nothing else while paused or disabled.
                _ if paused || !self.enabled => {}
                Ev::Move(p) => self.moved(p),
                Ev::Press { .. } => self.pressed(),
                Ev::Release => self.released(&mut asked),
                Ev::Wheel { up } => self.wheel(up),
                _ => {}
            }
        }
        let held = dialog::left_held(input);
        if !paused && held {
            self.held(now);
        }
        self.update_looks(held);
        asked
    }

    /// What the cursor is over: a question caption or the trade button.
    fn target_at(&self, p: [f32; 2]) -> Option<Armed> {
        let w = &self.window;
        if let Some(r) = w.button_rect()
            && dialog::contains(r, p)
        {
            return Some(Armed::Trade);
        }
        (0..w.question_count())
            .find(|&i| w.question_rect(i).is_some_and(|r| dialog::contains(r, p)))
            .map(Armed::Question)
    }

    /// List `i`'s scroll bar, while shown (talk-window.md 6, 7).
    fn bar_rect(&self, i: usize) -> Option<Rect> {
        let (list, _, content) = self.window.list(i)?;
        if !list.desc.shows_scroll_bar(content) {
            return None;
        }
        let (r, w) = (list.rect, list.profile.width);
        Some(Rect::new(r.x + r.width - w, r.y, w, r.height))
    }

    fn moved(&mut self, p: [f32; 2]) {
        let dy = p[1] - self.cursor[1];
        self.cursor = p;
        match self.grab {
            // options-screens.md 4.7: a drag scrolls the content by the
            // cursor's vertical movement.
            Some(Grab::View(i, _)) => {
                let b = self.window.bar_mut(i);
                b.set(b.pos - dy as i32);
            }
            Some(Grab::Box(i, offset)) => {
                if let Some(r) = self.bar_rect(i) {
                    self.window.bar_mut(i).drag_to(p[1] - r.y - offset);
                }
            }
            _ => {}
        }
    }

    fn pressed(&mut self) {
        let p = self.cursor;
        if let Some(t) = self.target_at(p) {
            self.armed = Some(t);
        }
        for i in 0..2 {
            if let Some(r) = self.bar_rect(i)
                && dialog::contains(r, p)
            {
                let b = *self.window.bar_mut(i);
                let y = p[1] - r.y;
                if y < ARROW || y > r.height - ARROW {
                    let up = y < ARROW;
                    self.window.bar_mut(i).scroll(if up { -1 } else { 1 });
                    self.grab = Some(Grab::Arrow(i, up, u64::MAX));
                } else if (b.box_top()..=b.box_top() + b.box_length()).contains(&y) {
                    self.grab = Some(Grab::Box(i, y - b.box_top()));
                }
                return;
            }
            if let Some((list, _, _)) = self.window.list(i)
                && dialog::contains(super::view(list), p)
            {
                self.grab = Some(Grab::View(i, p[1]));
            }
        }
    }

    /// A left release: an armed button under the cursor is clicked (3.4,
    /// save-load-screens.md 3.2).
    fn released(&mut self, asked: &mut Asked) {
        self.grab = None;
        let Some(armed) = self.armed.take() else {
            return;
        };
        if self.target_at(self.cursor) != Some(armed) {
            return;
        }
        match armed {
            // Message 46: ignored while a choice waits for the refill;
            // otherwise the choice is taken and the list marked.
            Armed::Question(i) => {
                if !self.refill && asked.choice.is_none() {
                    asked.choice = Some(i);
                    self.refill = true;
                }
            }
            // Message 45 (DG-10's what follows).
            Armed::Trade => asked.trade = true,
        }
    }

    fn wheel(&mut self, up: bool) {
        for i in 0..2 {
            if let Some((list, _, _)) = self.window.list(i)
                && dialog::contains(list.rect, self.cursor)
            {
                self.window.bar_mut(i).scroll(if up { -1 } else { 1 });
            }
        }
    }

    /// The held left button over an arrow repeats it every > 100 ms
    /// (options-screens.md 4.8); the first repeat counts from the press.
    fn held(&mut self, now: u64) {
        let Some(Grab::Arrow(i, up, last)) = self.grab else {
            return;
        };
        if last == u64::MAX {
            self.grab = Some(Grab::Arrow(i, up, now));
            return;
        }
        let over = self.bar_rect(i).is_some_and(|r| {
            let y = self.cursor[1] - r.y;
            dialog::contains(r, self.cursor) && if up { y < ARROW } else { y > r.height - ARROW }
        });
        if over && now.saturating_sub(last) > REPEAT_MS {
            self.window.bar_mut(i).scroll(if up { -1 } else { 1 });
            self.grab = Some(Grab::Arrow(i, up, now));
        }
    }

    /// pda-controls.md 4: under the cursor `h`, pressed `t`.
    fn update_looks(&mut self, held: bool) {
        let over = self.target_at(self.cursor);
        let look = |t: Armed| -> Option<&'static str> {
            match (over == Some(t), held && self.armed == Some(t)) {
                (true, true) => Some("t"),
                (true, false) => Some("h"),
                _ => None,
            }
        };
        let question = (0..self.window.question_count())
            .find_map(|i| look(Armed::Question(i)).map(|l| (i, l)));
        let button = look(Armed::Trade).unwrap_or("e");
        self.window.set_looks(question, button);
    }

    /// The open screen over the level (talk-window.md 2), then the cursor.
    pub fn draw(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        if !self.open {
            return Ok(());
        }
        self.window.draw(ctx, s)?;
        crate::draw::cursor(ctx, s, self.cursor)
    }

    /// What tests observe.
    pub fn state(&self) -> TalkScreenState {
        TalkScreenState {
            open: self.open,
            partner: self.partner,
            refill: self.refill,
            cursor: self.cursor,
            window: self.window.state(),
        }
    }
}

/// The screen as tests see it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct TalkScreenState {
    pub open: bool,
    pub partner: Option<u16>,
    pub refill: bool,
    pub cursor: [f32; 2],
    pub window: TalkState,
}
