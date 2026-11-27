// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The PDA window (docs/behavior/pda-window.md, cited by section) with its
//! seven tabs empty: built with the level's in-game interface (1), its
//! layout and drawing (2, [`layout`]), the clock (2.3), the new-content
//! marks (2.4), the tab bar and switching tabs (3), opening and closing
//! (4.2, 5) and the information portions it gives (8). The tabs' contents
//! are their own specs' (pda-scope.md PD-4 … PD-17).
//!
//! **Input.** A mouse window drawing the game's cursor, as the inventory
//! screen: touch reaches it through the touchpad of #108 (a tap clicks a
//! tab); a gamepad's d-pad moves the cursor to the nearest tab button, the
//! left stick moves it freely and south (A / cross) is the left button
//! (our design, the options screen's mapping).
//!
//! **What waits for other systems**: the information portions are
//! collected ([`PdaWindow::take_portions`]) until the character can
//! receive them (#738); the tutorials' `pda_section` (#739); the HUD's
//! flashing icon and the HUD hidden while open (#740).

pub mod layout;
#[cfg(test)]
pub(crate) mod tests;

use std::io;

use serde::Serialize;

use crate::core::input::Input;
use crate::core::vfs::Vfs;
use crate::data::{GameData, encode};
use crate::dialog::{self, Ev};
use crate::draw::{Ctx, picture_desc};
use crate::scene::Scene;
use layout::{Layout, Placed, SEPARATOR, SEPARATOR_COLOR, TABS};
use volya_engine_core::text::Fonts;
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::Resolution;
use volya_engine_formats::ui::layout::{HAlign, Rect, VAlign};

/// The portion given when the window opens (4.2, 8).
pub const OPENED: &str = "ui_pda";
/// The portion given when it closes (5, 8).
pub const CLOSED: &str = "ui_pda_hide";
/// The portion given when a PDA map is shown: tab 1 becoming active (8).
pub const MAP_SHOWN: &str = "ui_pda_map_local";
/// The portion each tab gives when it becomes active (3.1).
pub const TAB_PORTIONS: [Option<&str>; TABS] = [
    None,
    Some(MAP_SHOWN),
    Some("ui_pda_events"),
    Some("ui_pda_contacts"),
    Some("ui_pda_ranking"),
    Some("ui_pda_actor_info"),
    Some("ui_pda_encyclopedia"),
];
/// The map tab (3.1).
pub const MAP_TAB: usize = 1;

/// One frame of the open window.
pub struct Frame<'a> {
    pub data: &'a GameData,
    /// The game's current time, game milliseconds (2.3).
    pub game_time: u64,
    /// The engine clock, milliseconds.
    pub now: u64,
    pub input: &'a Input,
    /// The game is paused: the window takes no input (6).
    pub paused: bool,
}

/// The PDA window of a level.
pub struct PdaWindow {
    layout: Layout,
    open: bool,
    /// The tab shown, none until the first switch (3.3).
    active: Option<usize>,
    /// The tab bar's selected button: tab 0 when built (3.3).
    selected: usize,
    /// One new-content flag per tab (2.4).
    new_content: [bool; TABS],
    /// The clock's text (2.3), empty before the first update.
    clock: String,
    cursor: [f32; 2],
    /// The cursor over each tab's caption (3.3).
    over: [bool; TABS],
    last_update: Option<u64>,
    /// The information portions given and not yet taken (8).
    portions: Vec<&'static str>,
}

impl PdaWindow {
    /// The window of a level, its tab bar laid out in `fonts`, the fonts
    /// of `resolution` (3.2).
    pub fn load(
        tree: &dyn Vfs,
        data: &GameData,
        resolution: Resolution,
        fonts: &Fonts,
    ) -> io::Result<Self> {
        let sx = resolution.scale().0;
        let width = |font, text: &[u8]| fonts.get(font).width(text) / sx;
        Ok(Self::new(Layout::load(tree, data, resolution, &width)?))
    }

    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            open: false,
            active: None,
            selected: 0,
            new_content: [false; TABS],
            clock: String::new(),
            cursor: [512.0, 384.0],
            over: [false; TABS],
            last_update: None,
            portions: Vec::new(),
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The tab shown, if any (3.3).
    pub fn active_tab(&self) -> Option<usize> {
        self.active
    }

    /// Sets tab `tab`'s new-content flag (2.4: a task or news event).
    pub fn set_new_content(&mut self, tab: usize) {
        self.new_content[tab] = true;
    }

    /// The information portions given since the last call (8).
    pub fn take_portions(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.portions)
    }

    /// Switches to tab `tab` (3.3), from any source; nothing when it is
    /// already the active tab.
    pub fn switch_to(&mut self, tab: usize) {
        if self.active == Some(tab) {
            return;
        }
        self.active = Some(tab);
        self.selected = tab;
        self.new_content[tab] = false;
        self.portions.extend(TAB_PORTIONS[tab]);
    }

    /// Opens the window with the cursor at `cursor` (4.2): `ui_pda`. A tab
    /// is selected first by every opening (4.1, 7).
    pub fn open(&mut self, cursor: [f32; 2]) {
        if self.open {
            return;
        }
        self.open = true;
        self.cursor = cursor;
        self.last_update = None;
        self.over = [false; TABS];
        self.portions.push(OPENED);
    }

    /// Closes the window (5): `ui_pda_hide`; the active tab is kept.
    pub fn close(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        self.portions.push(CLOSED);
    }

    /// One frame of the open window: its input (3.3, 6), then the clock
    /// (2.3).
    pub fn update(&mut self, f: Frame) {
        if !self.open {
            return;
        }
        // A frame counts 100 ms at most (as the inventory screen's).
        let dt = f
            .now
            .saturating_sub(self.last_update.unwrap_or(f.now))
            .min(100) as f32
            / 1000.0;
        self.last_update = Some(f.now);
        if !f.paused {
            for e in dialog::events(f.data, f.input, self.cursor, dt) {
                self.event(e);
            }
        }
        self.over = self.over_flags(self.cursor);
        self.clock = clock_text(f.game_time);
    }

    fn event(&mut self, e: Ev) {
        match e {
            Ev::Move(p) => self.cursor = p,
            Ev::Navigate(d) => {
                let targets: Vec<Rect> = self.layout.buttons.iter().map(|b| b.rect).collect();
                if let Some(p) = dialog::navigate(self.cursor, d, &targets) {
                    self.cursor = p;
                }
            }
            // 3.3, pda-controls.md 5: a left press on a tab button selects
            // it at once; a press counted as a double click does not.
            Ev::Press { double: false } => {
                if let Some(i) = self.button_at(self.cursor)
                    && i != self.selected
                {
                    self.switch_to(i);
                }
            }
            _ => {}
        }
    }

    /// The tab whose caption holds `p`.
    fn button_at(&self, p: [f32; 2]) -> Option<usize> {
        (self.layout.buttons.iter()).position(|b| dialog::contains(b.rect, p))
    }

    fn over_flags(&self, p: [f32; 2]) -> [bool; TABS] {
        std::array::from_fn(|i| dialog::contains(self.layout.buttons[i].rect, p))
    }

    /// Tab `i`'s caption look (3.3): selected `t`, under the cursor `h`,
    /// else `e`.
    fn look(&self, i: usize) -> &'static str {
        if i == self.selected {
            "t"
        } else if self.over[i] {
            "h"
        } else {
            "e"
        }
    }

    /// The window over the level (2.2), then the cursor.
    pub fn draw(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        if !self.open {
            return Ok(());
        }
        let l = &self.layout;
        ctx.picture(
            s,
            "pda background",
            &l.background.desc,
            l.background.rect,
            0.0,
            0,
            false,
            None,
        )?;
        frame_line(ctx, s, "pda mbbackground_frame_line", &l.mb_line)?;
        frame_line(ctx, s, "pda timer_frame_line", &l.timer_line)?;
        if !self.clock.is_empty() {
            let (b, font) = TextBox::of_static(&l.timer_title.desc, l.timer_title.rect);
            ctx.text(s, "pda clock", &encode(&self.clock), &b, font)?;
        }
        for (i, b) in l.buttons.iter().enumerate() {
            let (mut tb, font) = TextBox::of_static(&b.desc, b.rect);
            tb.align = b
                .desc
                .text
                .as_ref()
                .and_then(|t| t.align)
                .unwrap_or(HAlign::Centre);
            tb.vert_align = VAlign::Centre;
            tb.complex = false;
            tb.color = dialog::look_color(&b.desc, self.look(i), tb.color);
            if let Some(key) = b.desc.text.as_ref().and_then(|t| t.key.as_deref()) {
                ctx.text(s, &format!("pda tab {i}"), &ctx.data.text(key), &tb, font)?;
            }
        }
        for (i, sep) in l.separators.iter().enumerate() {
            let tb = TextBox {
                rect: sep.rect,
                offset: (0.0, 0.0),
                align: HAlign::Left,
                vert_align: VAlign::Top,
                complex: false,
                color: SEPARATOR_COLOR,
                line_gap: 0.0,
            };
            ctx.text(
                s,
                &format!("pda separator {i}"),
                SEPARATOR.as_bytes(),
                &tb,
                sep.font,
            )?;
        }
        // TODO(#723): the active tab's window (each tab's own spec).
        for (i, b) in l.buttons.iter().enumerate() {
            let mark = if self.new_content[i] {
                &l.updated_mark
            } else {
                &l.old_mark
            };
            let Some(t) = &mark.texture else { continue };
            let r = Rect::new(b.mark.0, b.mark.1, mark.window.width, mark.window.height);
            let desc = picture_desc(&t.name, r, t.rect, t.color);
            ctx.picture(s, &format!("pda mark {i}"), &desc, r, 0.0, 0, false, None)?;
        }
        crate::draw::cursor(ctx, s, self.cursor)
    }

    /// What tests observe.
    pub fn state(&self) -> PdaState {
        PdaState {
            tab: self.active,
            selected: self.selected,
            clock: self.clock.clone(),
            cursor: self.cursor,
            tabs: (self.layout.buttons.iter().enumerate())
                .map(|(i, b)| TabState {
                    rect: [b.rect.x, b.rect.y, b.rect.width, b.rect.height],
                    mark: [b.mark.0, b.mark.1],
                    look: self.look(i),
                    new_content: self.new_content[i],
                })
                .collect(),
        }
    }
}

/// A frame line (pda-controls.md 3): its band at the line's top-left.
fn frame_line(ctx: &mut Ctx, s: &mut Scene, layer: &str, line: &Placed) -> io::Result<()> {
    let Some(t) = &line.desc.texture else {
        return Ok(());
    };
    ctx.frame_line_colored(
        s,
        layer,
        &t.name,
        (line.rect.x, line.rect.y),
        line.rect.width,
        t.color,
    )
}

/// The clock's text at game time `ms` (2.3): `HH:MM DD/MM/YYYY`. The date
/// is the one whose game time (sav.md 12's formula) is the latest not
/// after `ms`: the game's own inverse conversion is unestablished (#741).
pub fn clock_text(ms: u64) -> String {
    const DAY: u64 = 24 * 60 * 60 * 1000;
    const MONTH_DAYS: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = |y: u64| (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    // The formula's days before 1 January of year `y`.
    let start = |y: u64| (y - 1) * 365 + y / 4 - y / 100 + y / 400;
    let days = ms / DAY;
    let mut year = (days * 400 / 146_097).max(1);
    while year > 1 && start(year) > days {
        year -= 1;
    }
    while start(year + 1) <= days {
        year += 1;
    }
    let mut day = days - start(year);
    let mut month = 0;
    while month < 11 {
        let n = MONTH_DAYS[month] + u64::from(month == 1 && leap(year));
        if day < n {
            break;
        }
        day -= n;
        month += 1;
    }
    let minutes = ms % DAY / 60_000;
    format!(
        "{:02}:{:02} {:02}/{:02}/{:04}",
        minutes / 60,
        minutes % 60,
        day + 1,
        month + 1,
        year
    )
}

/// One tab as tests see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TabState {
    /// The caption's rectangle, UI units.
    pub rect: [f32; 4],
    pub mark: [f32; 2],
    pub look: &'static str,
    pub new_content: bool,
}

/// The open window as tests see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PdaState {
    pub tab: Option<usize>,
    pub selected: usize,
    pub clock: String,
    pub cursor: [f32; 2],
    pub tabs: Vec<TabState>,
}
