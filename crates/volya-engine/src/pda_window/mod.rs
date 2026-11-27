// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The PDA window (docs/behavior/pda-window.md, cited by section) with its
//! seven tabs empty: built with the level's in-game interface (1), its
//! layout and drawing (2, [`layout`]), the clock (2.3), the new-content
//! marks (2.4), the tab bar and switching tabs (3), opening and closing
//! (4, 5), its input while open (6), the information portions it gives
//! (8), and the buttons' hint window (docs/behavior/pda-controls.md 6,
//! [`hint`]). The tabs' contents are their own specs' (pda-scope.md PD-4 …
//! PD-17): the map tab's is [`map`]'s.
//!
//! **Input.** A mouse window drawing the game's cursor, as the inventory
//! screen: keys it does not take reach the character. The keys bound to
//! `active_jobs`, `map` and `contacts` switch to their tab and open or
//! close it (4.1), the key bound to `quit` closes it (5). Touch and the
//! gamepad are our design, as for the inventory screen
//! ([`crate::controls`]): touch reaches the open window through the
//! touchpad of #108 (a tap clicks a tab), and the touch PDA button acts as
//! the key bound to `active_jobs`; a gamepad's d-pad left does too in the
//! level, and in the window the d-pad moves the cursor to the nearest tab
//! button, the left stick moves it freely, south (A / cross) is the left
//! button and east (B / circle) the key bound to `quit`.
//!
//! **What waits for other systems**: the information portions are
//! collected ([`PdaWindow::take_portions`]) until the character can
//! receive them (#738); the tutorials' `pda_section` (#739); the HUD's
//! flashing icon and the HUD hidden while open (#740).

pub mod hint;
pub mod layout;
pub mod map;
#[cfg(test)]
pub(crate) mod map_tests;
#[cfg(test)]
pub(crate) mod tests;

use std::io;

use serde::Serialize;

use crate::core::input::{Action, Control, Input, Key, TouchButton};
use crate::core::vfs::Vfs;
use crate::data::{GameData, encode};
use crate::dialog::{self, Ev};
use crate::draw::{Ctx, picture_desc};
use crate::scene::Scene;
use hint::{Hint, HintDesc};
use layout::{Layout, Placed, SEPARATOR, SEPARATOR_COLOR, TABS};
use map::{MapFrame, MapPlayer, MapState, MapWindow};
use volya_engine_core::text::Fonts;
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::layout::{HAlign, Rect, VAlign};
use volya_engine_formats::ui::{Color, Resolution};

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

/// The tabs the actions select (4.1): `active_jobs`, `map`, `contacts`.
pub const ACTION_TABS: [usize; 3] = [0, MAP_TAB, 3];

/// The keys the window reads (4.1, 5), from the effective bindings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    /// Bound to `active_jobs`, `map` and `contacts`, in [`ACTION_TABS`]
    /// order.
    pub actions: [Vec<Key>; 3],
    pub quit: Vec<Key>,
}

impl Keys {
    /// Every key the window uses: kept from the character's controls while
    /// it is open (6).
    pub fn used(&self) -> Vec<Control> {
        (self.actions.iter().flatten())
            .chain(&self.quit)
            .map(|&k| Control::Key(k))
            .collect()
    }

    /// The tab whose action key `key` is bound to (4.1).
    pub fn tab_of(&self, key: Key) -> Option<usize> {
        (self.actions.iter())
            .position(|keys| keys.contains(&key))
            .map(|i| ACTION_TABS[i])
    }
}

/// One frame of the open window.
pub struct Frame<'a> {
    pub data: &'a GameData,
    pub keys: &'a Keys,
    /// The game's current time, game milliseconds (2.3).
    pub game_time: u64,
    /// The engine clock, milliseconds: the interface clock of the hint's
    /// animation (pda-controls.md 6).
    pub now: u64,
    /// The global clock, milliseconds: the hint's delay (pda-controls.md
    /// 6).
    pub global_ms: u64,
    pub input: &'a Input,
    /// The game is paused: the window takes no input but `quit` (5, 6).
    pub paused: bool,
    /// The character's health is above 0 (5 item 3).
    pub alive: bool,
    /// The player, for the map (pda-map.md 5.2, 6.7).
    pub player: Option<MapPlayer<'a>>,
}

/// What a frame did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The window closed this frame.
    pub closed: bool,
}

/// The PDA window of a level.
pub struct PdaWindow {
    hint_desc: HintDesc,
    /// Each button's hint text and its width in units, if it has one: the
    /// tab buttons, then the map's tool buttons.
    hints: Vec<Option<(String, f32)>>,
    /// The hint window, while a button holds it.
    hint: Option<Hint>,
    /// When the cursor entered each button, global clock milliseconds.
    entered: Vec<Option<u64>>,
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
    /// The cursor over each button (3.3), as [`PdaWindow::hints`].
    over: Vec<bool>,
    last_update: Option<u64>,
    /// The information portions given and not yet taken (8).
    portions: Vec<&'static str>,
    /// The map tab's window (pda-map.md).
    map: Option<MapWindow>,
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
        let layout = Layout::load(tree, data, resolution, &width)?;
        let hint = HintDesc::load(tree, data, resolution)?;
        let map = MapWindow::load(tree, data, resolution, fonts)?;
        let text = |k: &str| data.strings.get(k).to_owned();
        Ok(Self::new(layout, hint, &text, &width).with_map(map, &text, &width))
    }

    /// The window of `layout` with the hint window `hint_desc`, the hints'
    /// texts by `text` (pda-controls.md 4: through the string table) and
    /// their widths by `width` (6 step 2), measured once.
    pub fn new(
        layout: Layout,
        hint_desc: HintDesc,
        text: &dyn Fn(&str) -> String,
        width: &dyn Fn(UiFont, &[u8]) -> f32,
    ) -> Self {
        let font = (hint_desc.description.text.as_ref())
            .and_then(|t| t.font)
            .unwrap_or(UiFont::DEFAULT);
        let hints = (layout.buttons.iter())
            .map(|b| {
                let t = text(b.hint.as_deref()?);
                (!t.is_empty()).then(|| {
                    let w = width(font, &encode(&t));
                    (t, w)
                })
            })
            .collect();
        Self {
            hint_desc,
            hints,
            hint: None,
            entered: vec![None; TABS],
            layout,
            open: false,
            active: None,
            selected: 0,
            new_content: [false; TABS],
            clock: String::new(),
            cursor: [512.0, 384.0],
            over: vec![false; TABS],
            last_update: None,
            portions: Vec::new(),
            map: None,
        }
    }

    /// The window with the map tab's window `map`, its tool buttons' hints
    /// as the tab buttons' (pda-controls.md 6).
    pub fn with_map(
        mut self,
        map: MapWindow,
        text: &dyn Fn(&str) -> String,
        width: &dyn Fn(UiFont, &[u8]) -> f32,
    ) -> Self {
        let font = (self.hint_desc.description.text.as_ref())
            .and_then(|t| t.font)
            .unwrap_or(UiFont::DEFAULT);
        for t in &map.layout().tools {
            let hint = t.hint.as_deref().map(text).filter(|t| !t.is_empty());
            self.hints
                .push(hint.map(|t| (t.clone(), width(font, &encode(&t)))));
        }
        let n = self.hints.len();
        self.entered.resize(n, None);
        self.over.resize(n, false);
        self.map = Some(map);
        self
    }

    pub fn map(&self) -> Option<&MapWindow> {
        self.map.as_ref()
    }

    /// The map tab's window while it shows.
    fn shown_map(&mut self) -> Option<&mut MapWindow> {
        if self.open && self.active == Some(MAP_TAB) {
            self.map.as_mut()
        } else {
            None
        }
    }

    /// The keys kept from the character's controls (6): the window's, and
    /// the map's keypad zoom while it shows (pda-map.md 5.7).
    pub fn kept(&self, keys: &Keys) -> Vec<Control> {
        let mut kept = keys.used();
        if self.active == Some(MAP_TAB) && self.map.is_some() {
            kept.extend([Key::NumpadAdd, Key::NumpadSubtract].map(Control::Key));
        }
        kept
    }

    /// The log lines written since the last call (pda-map.md 5.2).
    pub fn take_log(&mut self) -> Vec<String> {
        self.map
            .as_mut()
            .map(MapWindow::take_log)
            .unwrap_or_default()
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Every texture id the window draws and its string keys, the hint
    /// window's included, for checks.
    pub fn references(&self) -> (Vec<String>, Vec<String>) {
        let (mut textures, strings) = self.layout.references();
        let line = &self.hint_desc.line;
        textures.extend(["b", "back", "e"].map(|p| format!("{line}_{p}")));
        (textures, strings)
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
        // pda-map.md 3: the map window is shown and hidden with its tab.
        if let Some(map) = &mut self.map {
            if self.active == Some(MAP_TAB) {
                map.hide();
            } else if tab == MAP_TAB {
                map.show();
            }
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
        self.over.fill(false);
        self.entered.fill(None);
        self.hint = None;
        self.portions.push(OPENED);
    }

    /// Closes the window (5): `ui_pda_hide`; the active tab is kept.
    pub fn close(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        self.hint = None;
        self.portions.push(CLOSED);
    }

    /// An action of 4.1 while the window is the topmost one, or none is
    /// open: switch to `tab`, then open the window (the cursor at
    /// `cursor`) if it was closed, close it if it was open.
    pub fn act(&mut self, tab: usize, cursor: [f32; 2]) {
        self.switch_to(tab);
        if self.open {
            self.close();
        } else {
            self.open(cursor);
        }
    }

    /// One frame of the open window: the character's death (5 item 3), its
    /// input (3.3, 4.1, 5, 6), the captions' cursor flags and the hint
    /// (pda-controls.md 6), then the clock (2.3).
    pub fn update(&mut self, f: Frame) -> Outcome {
        let mut out = Outcome::default();
        if !self.open {
            return out;
        }
        if !f.alive {
            self.close();
            out.closed = true;
            return out;
        }
        // A frame counts 100 ms at most (as the inventory screen's).
        let dt = f
            .now
            .saturating_sub(self.last_update.unwrap_or(f.now))
            .min(100) as f32
            / 1000.0;
        self.last_update = Some(f.now);
        // The touch PDA button acts as the key bound to `active_jobs` (our
        // design).
        let touched = (f.input.actions().iter()).any(|a| {
            matches!(
                a,
                Action::Press {
                    control: Control::Touch(TouchButton::Pda),
                    ..
                }
            )
        });
        if touched && !f.paused {
            self.act(ACTION_TABS[0], self.cursor);
            out.closed = true;
            return out;
        }
        for e in dialog::events(f.data, f.input, self.cursor, dt) {
            // 6: while paused the window takes no input; the key bound to
            // `quit` still closes it (5 item 2).
            if f.paused && !matches!(e, Ev::Quit { pressed: true }) {
                continue;
            }
            if self.event(e, f.keys, f.input, f.global_ms) {
                out.closed = true;
                return out;
            }
        }
        let (player, cursor) = (f.player, self.cursor);
        if !f.paused
            && let Some(map) = self.shown_map()
        {
            map.update(MapFrame {
                data: f.data,
                input: f.input,
                player,
                cursor,
                global_ms: f.global_ms,
                dt,
            });
        }
        let over = self.over_flags(self.cursor);
        for (i, &now_over) in over.iter().enumerate() {
            if now_over && !self.over[i] {
                self.entered[i] = Some(f.global_ms);
            } else if !now_over && self.over[i] {
                self.entered[i] = None;
                if self.hint.as_ref().is_some_and(|h| h.owner == i) {
                    self.hint = None;
                }
            }
        }
        self.over = over;
        self.update_hint(f.now, f.global_ms);
        self.clock = clock_text(f.game_time);
        out
    }

    /// pda-controls.md 6: a caption under the cursor for over 500 ms of
    /// the global clock takes the free hint window; the shown one's
    /// animation runs.
    fn update_hint(&mut self, now: u64, global_ms: u64) {
        if let Some(h) = &mut self.hint {
            h.update(&self.hint_desc, now);
            return;
        }
        let ready = (0..self.hints.len()).find(|&i| {
            self.over[i]
                && self.hints[i].is_some()
                && self.entered[i].is_some_and(|t| global_ms.saturating_sub(t) > hint::DELAY_MS)
        });
        if let Some(i) = ready
            && let Some((text, width)) = self.hints[i].clone()
        {
            let h = Hint::take(&self.hint_desc, i, text, width, self.cursor, now);
            self.hint = Some(h);
        }
    }

    /// One event; returns whether the window closed.
    fn event(&mut self, e: Ev, keys: &Keys, input: &Input, global_ms: u64) -> bool {
        let cursor = match e {
            Ev::Move(p) => p,
            _ => self.cursor,
        };
        // pda-window.md 6: the active tab is offered the events first (the
        // map takes of the keys only the keypad's plus and minus,
        // pda-map.md 5.7); a press on a tab button is the tab bar's.
        let on_tab = matches!(e, Ev::Press { .. }) && self.button_at(cursor).is_some();
        if !on_tab
            && !matches!(e, Ev::Quit { .. } | Ev::Navigate(_))
            && let Some(map) = self.shown_map()
            && map.event(e, cursor, dialog::left_held(input), global_ms)
        {
            if let Ev::Move(p) = e {
                self.cursor = p;
            }
            return false;
        }
        match e {
            // 4.1: an action's key switches to its tab, then closes the
            // window; 5 item 2: `quit` closes it (a gamepad's east too).
            Ev::Key { key, pressed: true } => {
                if let Some(tab) = keys.tab_of(key) {
                    self.act(tab, self.cursor);
                    return true;
                }
            }
            Ev::Quit { pressed: true } => {
                self.close();
                return true;
            }
            Ev::Move(p) => self.cursor = p,
            Ev::Navigate(d) => {
                let mut targets: Vec<Rect> = self.layout.buttons.iter().map(|b| b.rect).collect();
                if let Some(map) = self.shown_map() {
                    targets.extend(map.tool_rects());
                }
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
        false
    }

    /// The tab whose caption holds `p`.
    fn button_at(&self, p: [f32; 2]) -> Option<usize> {
        (self.layout.buttons.iter()).position(|b| dialog::contains(b.rect, p))
    }

    /// The cursor over each button: the map's tools only while it shows.
    fn over_flags(&self, p: [f32; 2]) -> Vec<bool> {
        let tools = match &self.map {
            Some(m) if self.active == Some(MAP_TAB) => m.tool_rects(),
            _ => Vec::new(),
        };
        (0..self.hints.len())
            .map(|i| match i.checked_sub(TABS) {
                None => dialog::contains(self.layout.buttons[i].rect, p),
                Some(t) => tools.get(t).is_some_and(|&r| dialog::contains(r, p)),
            })
            .collect()
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
        // The active tab's window (each tab's own spec): the map's.
        if self.active == Some(MAP_TAB)
            && let Some(map) = &self.map
        {
            map.draw(ctx, s)?;
        }
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
        self.draw_hint(ctx, s)?;
        crate::draw::cursor(ctx, s, self.cursor)
    }

    /// pda-controls.md 6: the hint window over the interface, its frame
    /// line white with the text's alpha.
    fn draw_hint(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let Some(h) = &self.hint else {
            return Ok(());
        };
        let d = &self.hint_desc;
        let line = Color::rgba(255, 255, 255, h.color.a);
        let r = h.rect;
        ctx.frame_line_colored(s, "pda hint line", &d.line, (r.x, r.y), r.width, line)?;
        let w = &d.description.window;
        let rect = Rect::new(r.x + w.x, r.y + w.y, h.text_width, w.height);
        let (mut tb, font) = TextBox::of_static(&d.description, rect);
        tb.complex = false;
        tb.color = h.color;
        ctx.text(s, "pda hint", &encode(&h.text), &tb, font)
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
            map: self.map.as_ref().map(MapWindow::state),
            hint: self.hint.as_ref().map(|h| HintState {
                tab: h.owner,
                text: h.text.clone(),
                rect: [h.rect.x, h.rect.y, h.rect.width, h.rect.height],
                alpha: h.color.a,
            }),
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
    pub map: Option<MapState>,
    pub hint: Option<HintState>,
}

/// The hint window as tests see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HintState {
    /// The button holding it: a tab, or [`TABS`] + a map tool.
    pub tab: usize,
    pub text: String,
    pub rect: [f32; 4],
    /// The text's alpha as last animated.
    pub alpha: u8,
}
