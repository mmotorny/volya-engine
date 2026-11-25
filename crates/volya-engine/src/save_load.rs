// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The menu's save-game and load-game screens
//! (docs/behavior/save-load-screens.md sections 3–7): their layouts, the
//! save list with its scroll bar, the name field, the information area,
//! Delete, and their message boxes; what the load screen reads from a save
//! (docs/formats/sav.md section 11.4).
//!
//! Writing a save and loading one are not built yet (#75): the save
//! screen's last step (`save N`, section 8) and loading a valid save
//! (sections 5.3, 9.3) do nothing ([`Stubbed`]). The save list comes from
//! the platform's saved-games folder ([`SavedGames`]).
//!
//! Volya Engine shows no save picture yet: the load screen's picture is
//! always `ui\ui_noise` (the saved-games folder is not a texture source
//! here).

use std::io;

use serde::Serialize;

use crate::core::clock::GameClock;
use crate::core::config::{self, Config};
use crate::core::input::{GamepadButton, Input, Key};
use crate::core::ui::load_ui_file;
use crate::core::vfs::{self, Vfs};
use crate::data::GameData;
use crate::dialog::{
    self, Answer, Ev, MessageBox, MessageBoxDesc, MessageBoxState, Over, PushButton, Pushed,
    contains,
};
use crate::draw::Ctx;
use crate::on_screen_keyboard::{KeyboardState, OnScreenKeyboard, Press};
use crate::saved_games::{SaveRow, SavedGames};
use crate::scene::Scene;
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::object::{self, Object, ObjectRecord};
use volya_engine_formats::sav::Save;
use volya_engine_formats::text::font::{Font, UiFont};
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::layout::{HAlign, Rect, StaticDesc, VAlign, require};
use volya_engine_formats::ui::options::ScrollBarProfile;
use volya_engine_formats::ui::{Color, Resolution};
use volya_engine_formats::xml::Document;

/// The boxes the screens show (section 6).
pub const EMPTY_FILE_NAME: &str = "message_box_empty_file_name";
pub const FILE_ALREADY_EXIST: &str = "message_box_file_already_exist";
pub const DELETE_FILE_NAME: &str = "message_box_delete_file_name";
pub const CONFIRM_LOAD_SAVE: &str = "message_box_confirm_load_save";
pub const INVALID_SAVED_GAME: &str = "message_box_invalid_saved_game";
/// The menu's own boxes (title-menu.md section 4.5).
pub const QUIT_WINDOWS: &str = "message_box_quit_windows";
pub const QUIT_GAME: &str = "message_box_quit_game";
/// The quit-game box's text in single player (title-menu.md 4.5).
pub const QUIT_GAME_TEXT: &str = "ui_mm_quit_game_message";

/// Row texts (section 3.4): the name and the time, each at its position
/// in the row, in its field.
const ROW_NAME: (f32, f32, f32, f32) = (0.0, 0.0, 230.0, 22.0);
const ROW_TIME: (f32, f32, f32, f32) = (250.0, 0.0, 150.0, 22.0);
const ROW_COLOR: Color = Color::rgba(216, 186, 140, 255);
/// The highlight line's base (section 3.4).
const LIST_LINE: &str = "ui_listline";
/// The scroll bar's arrows and the repeated track and thumb copies
/// (section 3.5).
const ARROW: f32 = 16.0;
const COPY: (f32, f32) = (15.0, 16.0);
const THUMB_MIN: f32 = 16.0;
/// A held arrow steps once more than this many ms after the last hold step.
const HOLD_MS: u64 = 100;
/// A dragged thumb follows the cursor within this many units of it.
const DRAG_REACH: f32 = 10.0;
/// The name field: text 3 units right of its left edge, accepted while the
/// text is at most this wide (section 4.4).
const FIELD_INDENT: f32 = 3.0;
const FIELD_MAX_WIDTH: f32 = 256.0;
/// Key repeat of the name field, ms (section 4.4).
const REPEAT_DELAY: u64 = 400;
const REPEAT_PERIOD: u64 = 100;
/// Characters a save name may not hold (section 8.1).
const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

fn invalid(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

/// One screen's layout (tables 4.1 and 5.1), rectangles as written
/// (`form`'s children relative to `form`).
#[derive(Clone, Debug)]
struct ScreenDesc {
    back_video: StaticDesc,
    background: StaticDesc,
    newspaper_video: StaticDesc,
    form: StaticDesc,
    caption: StaticDesc,
    list_frame: StaticDesc,
    list: StaticDesc,
    item_height: f32,
    /// `btn_save` or `btn_load`.
    action: StaticDesc,
    delete: StaticDesc,
    cancel: StaticDesc,
    /// The save screen's name field.
    edit: Option<StaticDesc>,
    /// The load screen's information area.
    info: Option<InfoDesc>,
}

#[derive(Clone, Debug)]
struct InfoDesc {
    picture: StaticDesc,
    file_info: StaticDesc,
    file_caption: StaticDesc,
    file_data: StaticDesc,
}

impl ScreenDesc {
    fn read(doc: &Document, data: &GameData, action: &str, save: bool) -> io::Result<Self> {
        let s = |p: &str| StaticDesc::read(require(doc, p, 0)?, &data.colors);
        let list = require(doc, "form:list", 0)?;
        let item_height = list
            .attr("item_height")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|h| *h > 0.0)
            .ok_or_else(|| invalid("form:list without item_height"))?;
        Ok(Self {
            back_video: s("back_video")?,
            background: s("background")?,
            newspaper_video: s("newspaper_video")?,
            form: s("form")?,
            caption: s("form:caption")?,
            list_frame: s("form:list_frame")?,
            list: StaticDesc::read(list, &data.colors)?,
            item_height,
            action: s(&format!("form:{action}"))?,
            delete: s("form:btn_delete")?,
            cancel: s("form:btn_cancel")?,
            edit: if save { Some(s("form:edit")?) } else { None },
            info: if save {
                None
            } else {
                Some(InfoDesc {
                    picture: s("form:picture")?,
                    file_info: s("form:file_info")?,
                    file_caption: s("form:file_caption")?,
                    file_data: s("form:file_data")?,
                })
            },
        })
    }

    fn form(&self) -> Rect {
        self.form.window.absolute(0.0, 0.0)
    }

    /// The absolute rectangle of form child `d`.
    fn abs(&self, d: &StaticDesc) -> Rect {
        let f = self.form();
        d.window.absolute(f.x, f.y)
    }

    fn buttons(&self) -> [&StaticDesc; 3] {
        [&self.action, &self.delete, &self.cancel]
    }
}

/// The screens' layouts, `message_box.xml` and the scroll-bar profile.
pub struct SaveLoadData {
    boxes: Document,
    save: ScreenDesc,
    load: ScreenDesc,
    scroll: ScrollBarProfile,
}

impl SaveLoadData {
    /// Loads `ui_mm_save_dlg.xml`, `ui_mm_load_dlg.xml`, `message_box.xml`
    /// and `scroll_bar.xml` at `resolution` (the `_16` rule).
    pub fn load(tree: &dyn Vfs, data: &GameData, resolution: Resolution) -> io::Result<Self> {
        let ui = |name: &str| load_ui_file(tree, &data.aliases, name, resolution);
        Ok(Self {
            save: ScreenDesc::read(&ui("ui_mm_save_dlg.xml")?, data, "btn_save", true)?,
            load: ScreenDesc::read(&ui("ui_mm_load_dlg.xml")?, data, "btn_load", false)?,
            boxes: ui("message_box.xml")?,
            scroll: ScrollBarProfile::read(&ui("scroll_bar.xml")?, "default")?,
        })
    }

    /// Box `name` of `message_box.xml`.
    pub fn message_box(&self, data: &GameData, name: &str) -> io::Result<MessageBoxDesc> {
        MessageBoxDesc::read(&self.boxes, name, &data.colors)
    }

    /// Every texture id or path and string key the screens and their boxes
    /// draw, for checks.
    pub fn references(&self, data: &GameData) -> io::Result<(Vec<String>, Vec<String>)> {
        let mut textures = Vec::new();
        let mut strings = Vec::new();
        for d in [&self.save, &self.load] {
            for s in [&d.back_video, &d.background, &d.newspaper_video, &d.form] {
                textures.extend(s.texture.iter().map(|t| t.name.clone()));
            }
            if let Some(t) = &d.list_frame.texture {
                for p in ["lt", "t", "rt", "l", "back", "r", "lb", "b", "rb"] {
                    textures.push(format!("{}_{p}", t.name));
                }
            }
            for b in d.buttons() {
                textures.extend(dialog::button_textures(b));
                strings.extend(b.text.iter().flat_map(|t| t.key.clone()));
            }
            strings.extend(d.caption.text.iter().flat_map(|t| t.key.clone()));
            if let Some(e) = &d.edit
                && let Some(t) = &e.texture
            {
                textures.extend(["b", "back", "e"].map(|p| format!("{}_{p}", t.name)));
            }
            if let Some(i) = &d.info {
                for s in [&i.picture, &i.file_info] {
                    textures.extend(s.texture.iter().map(|t| t.name.clone()));
                }
            }
        }
        textures.extend(["b", "back", "e"].map(|p| format!("{LIST_LINE}_{p}")));
        let p = &self.scroll;
        textures.extend([
            p.up.clone(),
            p.down.clone(),
            p.track.clone(),
            p.thumb.clone(),
        ]);
        strings.extend(
            ["st_level", "ui_inv_time", "ui_inv_health", QUIT_GAME_TEXT]
                .iter()
                .map(|s| s.to_string()),
        );
        for name in [
            EMPTY_FILE_NAME,
            FILE_ALREADY_EXIST,
            DELETE_FILE_NAME,
            CONFIRM_LOAD_SAVE,
            INVALID_SAVED_GAME,
            QUIT_WINDOWS,
            QUIT_GAME,
        ] {
            let (t, s) = self.message_box(data, name)?.references();
            textures.extend(t);
            strings.extend(s);
        }
        textures.sort();
        textures.dedup();
        strings.sort();
        strings.dedup();
        Ok((textures, strings))
    }
}

/// What the screens leave undone until #75, for the engine interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "action", content = "name", rename_all = "snake_case")]
pub enum Stubbed {
    /// `save N` (section 8): nothing is written.
    Save(String),
    /// Loading save N (sections 5.3, 9.3): nothing is loaded.
    Load(String),
}

/// What a screen's frame asks of the menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The screen returns to the menu.
    pub closed: bool,
    /// Files the screen deleted from the saved-games folder.
    pub deleted: Vec<String>,
}

/// The save list (section 3.4) and its scroll bar (3.5).
#[derive(Clone, Debug)]
struct SaveList {
    rect: Rect,
    item_height: f32,
    visible: usize,
    bar_width: f32,
    rows: Vec<(SaveRow, PushButton)>,
    selected: Option<usize>,
    focused: Option<usize>,
    /// The first visible row.
    first: usize,
    up: PushButton,
    down: PushButton,
    /// A row reported *cursor left*: a movement is re-delivered next frame.
    redeliver: bool,
    last_cursor: [f32; 2],
    /// The dragged thumb's top, units below the bar's top.
    drag: Option<f32>,
}

/// What a list event did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListEvent {
    Nothing,
    /// Row `i` was clicked (activated): it is selected.
    Clicked(usize),
    /// The second press of a double click over the list.
    Double,
}

impl SaveList {
    fn new(rect: Rect, item_height: f32, bar_width: f32, rows: Vec<SaveRow>) -> Self {
        Self {
            rect,
            item_height,
            visible: (rect.height / item_height).floor() as usize,
            bar_width,
            rows: rows
                .into_iter()
                .map(|r| (r, PushButton::default()))
                .collect(),
            selected: None,
            focused: None,
            first: 0,
            up: PushButton::default(),
            down: PushButton::default(),
            redeliver: false,
            last_cursor: [0.0; 2],
            drag: None,
        }
    }

    fn bar_shown(&self) -> bool {
        self.rows.len() > self.visible
    }

    fn max_first(&self) -> usize {
        self.rows.len().saturating_sub(self.visible)
    }

    fn scroll(&mut self, by: isize) {
        self.first = self.first.saturating_add_signed(by).min(self.max_first());
    }

    fn row_rect(&self, i: usize) -> Option<Rect> {
        (i >= self.first && i < self.first + self.visible && i < self.rows.len()).then(|| {
            Rect::new(
                self.rect.x,
                self.rect.y + (i - self.first) as f32 * self.item_height,
                self.rect.width - self.bar_width,
                self.item_height,
            )
        })
    }

    fn bar_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + self.rect.width - self.bar_width,
            self.rect.y,
            self.bar_width,
            self.rect.height,
        )
    }

    fn up_rect(&self) -> Rect {
        let b = self.bar_rect();
        Rect::new(b.x, b.y, b.width, ARROW)
    }

    fn down_rect(&self) -> Rect {
        let b = self.bar_rect();
        Rect::new(b.x, b.y + b.height - ARROW, b.width, ARROW)
    }

    /// The track length *T*, the thumb's length and its top below the
    /// bar's top (section 3.5, integer arithmetic).
    fn thumb(&self) -> (f32, f32, f32) {
        let track = self.rect.height - 2.0 * ARROW;
        let count = self.rows.len();
        let len = (track * self.visible as f32 / count.saturating_sub(1).max(1) as f32)
            .clamp(THUMB_MIN, track.max(THUMB_MIN))
            .trunc();
        let range = (count - self.visible.min(count)).max(1) as f32;
        let top = match self.drag {
            Some(t) => t,
            None => ARROW + (self.first as f32 * (track - len) / range).floor(),
        };
        (track, len, top)
    }

    fn thumb_rect(&self) -> Rect {
        let b = self.bar_rect();
        let (_, len, top) = self.thumb();
        Rect::new(b.x, b.y + top, b.width, len)
    }

    /// The per-frame update: the re-delivered movement, then the visible
    /// rows' and the arrows' flags.
    fn frame(&mut self, cursor: [f32; 2]) {
        if std::mem::take(&mut self.redeliver) {
            let at = self.last_cursor;
            self.event(Ev::Move(at), at);
        }
        for i in 0..self.rows.len() {
            let Some(r) = self.row_rect(i) else { continue };
            match self.rows[i].1.frame(r, cursor) {
                Some(Over::Entered) => self.focused = Some(i),
                Some(Over::Left) => {
                    if self.focused == Some(i) {
                        self.focused = None;
                    }
                    self.redeliver = true;
                }
                None => {}
            }
        }
        if self.bar_shown() {
            let (u, d) = (self.up_rect(), self.down_rect());
            self.up.frame(u, cursor);
            self.down.frame(d, cursor);
        }
    }

    fn event(&mut self, e: Ev, cursor: [f32; 2]) -> ListEvent {
        let previous = self.last_cursor;
        if matches!(e, Ev::Move(_) | Ev::Press { .. } | Ev::Release) {
            self.last_cursor = cursor;
        }
        if let (Some(top), Ev::Move(_)) = (self.drag, e) {
            // The thumb takes every movement while dragged, moving by the
            // cursor's vertical movement.
            let t = self.thumb_rect();
            let dy = cursor[1] - previous[1];
            let near = cursor[0] >= t.x - DRAG_REACH
                && cursor[0] < t.x + t.width + DRAG_REACH
                && cursor[1] >= t.y - DRAG_REACH
                && cursor[1] < t.y + t.height + DRAG_REACH;
            if near {
                let (track, len, _) = self.thumb();
                let top = (top + dy).clamp(ARROW, ARROW + track - len);
                self.drag = Some(top);
                // Our rounding (section 14).
                let range = (track - len).max(1.0);
                self.first = (((top - ARROW) / range) * self.max_first() as f32).round() as usize;
            }
            return ListEvent::Nothing;
        }
        if matches!(e, Ev::Release) && self.drag.take().is_some() {
            return ListEvent::Nothing;
        }
        let mut out = ListEvent::Nothing;
        if let Ev::Wheel { up } = e {
            if contains(self.rect, cursor) {
                self.scroll(if up { -1 } else { 1 });
            }
            return out;
        }
        if let Ev::Press { .. } = e {
            for i in 0..self.rows.len() {
                if let Some(r) = self.row_rect(i) {
                    // A press finds the flags, and the focused row, as the
                    // cursor stands (our choice, so a tap, which moves,
                    // presses and releases in one frame, focuses its row).
                    if self.rows[i].1.frame(r, cursor) == Some(Over::Entered) {
                        self.focused = Some(i);
                    }
                }
            }
        }
        for i in 0..self.rows.len() {
            let Some(r) = self.row_rect(i) else { continue };
            let b = &mut self.rows[i].1;
            if b.receives(r, cursor) && b.event(e) == Pushed::Activated {
                self.selected = Some(i);
                out = ListEvent::Clicked(i);
            }
        }
        if self.bar_shown() {
            for up in [true, false] {
                let r = if up { self.up_rect() } else { self.down_rect() };
                let b = if up { &mut self.up } else { &mut self.down };
                if matches!(e, Ev::Press { .. }) {
                    b.frame(r, cursor);
                }
                if b.receives(r, cursor) && b.event(e) != Pushed::Nothing {
                    self.scroll(if up { -1 } else { 1 });
                }
            }
            if e == (Ev::Press { double: false }) && contains(self.thumb_rect(), cursor) {
                self.drag = Some(self.thumb().2);
            }
        }
        if matches!(e, Ev::Press { double: true }) && contains(self.rect, cursor) {
            out = ListEvent::Double;
        }
        out
    }

    /// A hold step (section 3.5, item 3) at `now`, sharing `last` with every
    /// scroll bar.
    fn held(&mut self, now: u64, last: &mut u64, cursor: [f32; 2]) {
        if !self.bar_shown() || now.saturating_sub(*last) <= HOLD_MS {
            return;
        }
        if contains(self.up_rect(), cursor) {
            self.scroll(-1);
            *last = now;
        } else if contains(self.down_rect(), cursor) {
            self.scroll(1);
            *last = now;
        }
    }

    /// Removes the selected row (section 3.4, with the deviation of 3.6: a
    /// selection past the end is cleared).
    fn remove_selected(&mut self) {
        let Some(i) = self.selected else { return };
        if i < self.rows.len() {
            self.rows.remove(i);
        }
        if i >= self.rows.len() {
            self.selected = None;
        }
        self.first = self.first.min(self.max_first());
    }

    fn selected_row(&self) -> Option<&SaveRow> {
        self.selected.and_then(|i| self.rows.get(i)).map(|(r, _)| r)
    }

    fn selected_name(&self) -> Option<&str> {
        self.selected_row().map(|r| r.name.as_str())
    }

    fn targets(&self) -> Vec<Rect> {
        let mut t: Vec<Rect> = (0..self.rows.len())
            .filter_map(|i| self.row_rect(i))
            .collect();
        if self.bar_shown() {
            t.extend([self.up_rect(), self.down_rect()]);
        }
        t
    }

    fn draw(&self, ctx: &mut Ctx, s: &mut Scene, profile: &ScrollBarProfile) -> io::Result<()> {
        let start = s.sprites.len();
        if let Some(i) = self.selected
            && let Some(r) = self.row_rect(i)
        {
            let width = if self.bar_shown() {
                self.rect.width - self.bar_width
            } else {
                self.rect.width
            };
            ctx.frame_line(s, "list selected", LIST_LINE, (r.x, r.y), width)?;
        }
        s.clip_from(start, pixels(self.rect, ctx.scale()));
        for i in 0..self.rows.len() {
            let Some(r) = self.row_rect(i) else { continue };
            let row = &self.rows[i].0;
            for ((x, y, w, h), text, font, what) in [
                (
                    ROW_NAME,
                    crate::data::encode(&row.name),
                    UiFont::Letterica18,
                    "name",
                ),
                (
                    ROW_TIME,
                    crate::data::encode(&row.time_text()),
                    UiFont::Letterica16,
                    "time",
                ),
            ] {
                let b = text_box(Rect::new(r.x + x, r.y + y, w, h), ROW_COLOR);
                ctx.text(s, &format!("row {i} {what}"), &text, &b, font)?;
            }
        }
        if self.bar_shown() {
            let b = self.bar_rect();
            let (track, len, top) = self.thumb();
            repeat(
                ctx,
                s,
                "list bar",
                &profile.track,
                (b.x, b.y + ARROW),
                track,
            )?;
            repeat(ctx, s, "list bar", &profile.thumb, (b.x, b.y + top), len)?;
            ctx.part(s, "list bar", &profile.up, (b.x, b.y), None, Color::WHITE)?;
            let d = self.down_rect();
            ctx.part(s, "list bar", &profile.down, (d.x, d.y), None, Color::WHITE)?;
        }
        Ok(())
    }

    fn state(&self) -> ListState {
        ListState {
            rows: self
                .rows
                .iter()
                .map(|(r, _)| RowState {
                    name: r.name.clone(),
                    time: r.time_text(),
                })
                .collect(),
            visible: self.visible,
            first: self.first,
            selected: self.selected,
            focused: self.focused,
            scroll_bar: self.bar_shown(),
            rect: [self.rect.x, self.rect.y, self.rect.width, self.rect.height],
        }
    }
}

/// `id` repeated unstretched along a bar from `at`, `length` units, the last
/// copy cut (section 3.5).
fn repeat(
    ctx: &mut Ctx,
    s: &mut Scene,
    layer: &str,
    id: &str,
    (x, y): (f32, f32),
    length: f32,
) -> io::Result<()> {
    let mut p = 0.0;
    while p < length {
        let h = (length - p).min(COPY.1);
        ctx.part(s, layer, id, (x, y + p), Some((COPY.0, h)), Color::WHITE)?;
        p += COPY.1;
    }
    Ok(())
}

/// A single-line, left/top text box.
fn text_box(rect: Rect, color: Color) -> TextBox {
    TextBox {
        rect,
        offset: (0.0, 0.0),
        align: HAlign::Left,
        vert_align: VAlign::Top,
        complex: false,
        color,
        line_gap: 0.0,
    }
}

/// A UI rectangle in output pixels, in the scene's convention.
fn pixels(r: Rect, (sx, sy): (f32, f32)) -> [f32; 4] {
    [
        (r.x * sx).floor() - 0.5,
        (r.y * sy).floor() - 0.5,
        ((r.x + r.width) * sx).floor() - 0.5,
        ((r.y + r.height) * sy).floor() - 0.5,
    ]
}

/// The character key `key` types (section 4.4), lower or shifted.
pub(crate) fn typed(key: Key, shift: bool) -> Option<char> {
    let letters = [
        (Key::A, 'a'),
        (Key::B, 'b'),
        (Key::C, 'c'),
        (Key::D, 'd'),
        (Key::E, 'e'),
        (Key::F, 'f'),
        (Key::G, 'g'),
        (Key::H, 'h'),
        (Key::I, 'i'),
        (Key::J, 'j'),
        (Key::K, 'k'),
        (Key::L, 'l'),
        (Key::M, 'm'),
        (Key::N, 'n'),
        (Key::O, 'o'),
        (Key::P, 'p'),
        (Key::Q, 'q'),
        (Key::R, 'r'),
        (Key::S, 's'),
        (Key::T, 't'),
        (Key::U, 'u'),
        (Key::V, 'v'),
        (Key::W, 'w'),
        (Key::X, 'x'),
        (Key::Y, 'y'),
        (Key::Z, 'z'),
    ];
    if let Some(&(_, c)) = letters.iter().find(|(k, _)| *k == key) {
        return Some(if shift { c.to_ascii_uppercase() } else { c });
    }
    let pairs = [
        (Key::Digit1, '1', '!'),
        (Key::Digit2, '2', '@'),
        (Key::Digit3, '3', '#'),
        (Key::Digit4, '4', '$'),
        (Key::Digit5, '5', '%'),
        (Key::Digit6, '6', '^'),
        (Key::Digit7, '7', '&'),
        (Key::Digit8, '8', '*'),
        (Key::Digit9, '9', '('),
        (Key::Digit0, '0', ')'),
        (Key::Minus, '-', '_'),
        (Key::Equals, '=', '+'),
        (Key::LeftBracket, '[', '{'),
        (Key::RightBracket, ']', '}'),
        (Key::Semicolon, ';', ':'),
        (Key::Apostrophe, '\'', '"'),
        (Key::Backslash, '\\', '|'),
        (Key::Comma, ',', '<'),
        (Key::Period, '.', '>'),
        (Key::Slash, '/', '?'),
        (Key::Space, ' ', ' '),
    ];
    pairs
        .iter()
        .find(|(k, ..)| *k == key)
        .map(|&(_, plain, shifted)| if shift { shifted } else { plain })
}

/// Widths of name-field text in UI units (the field's font at the output
/// size drawn at).
pub struct FieldFont {
    pub font: Font,
    /// Output pixels per UI unit along x.
    pub scale_x: f32,
}

impl FieldFont {
    /// The width of `text` in UI units.
    pub fn width(&self, text: &str) -> f32 {
        self.font.width(&crate::data::encode(text)) / self.scale_x
    }
}

/// The save screen's name field (section 4.4).
#[derive(Clone, Debug, Default)]
struct NameField {
    text: String,
    focused: bool,
    /// The caret, in characters.
    caret: usize,
    shift: bool,
    /// The repeat key and whether it is repeating (past its delay).
    repeat: Option<(Key, bool)>,
    mark: u64,
}

impl NameField {
    /// The text's length in characters.
    fn chars(&self) -> usize {
        self.text.chars().count()
    }

    /// The byte offset of character `i`.
    fn byte(&self, i: usize) -> usize {
        self.text
            .char_indices()
            .nth(i)
            .map_or(self.text.len(), |(b, _)| b)
    }

    /// The text before the caret.
    fn before_caret(&self) -> &str {
        &self.text[..self.byte(self.caret)]
    }

    /// Applies key `key` once.
    fn apply(&mut self, key: Key, font: Option<&FieldFont>) {
        match key {
            Key::LeftShift | Key::RightShift => self.shift = true,
            Key::Backspace if self.caret > 0 => {
                self.caret -= 1;
                let at = self.byte(self.caret);
                self.text.remove(at);
            }
            Key::Delete if self.caret < self.chars() => {
                let at = self.byte(self.caret);
                self.text.remove(at);
            }
            Key::Left if self.caret > 0 => self.caret -= 1,
            Key::Right if self.caret < self.chars() => self.caret += 1,
            _ => {
                if let Some(c) = typed(key, self.shift) {
                    let fits = font.is_none_or(|f| f.width(&self.text) <= FIELD_MAX_WIDTH);
                    if fits {
                        let at = self.byte(self.caret);
                        self.text.insert(at, c);
                        self.caret += 1;
                    }
                }
            }
        }
    }

    /// A key event while focused. Returns nothing; Enter and Esc on an
    /// empty field end focus.
    fn key(&mut self, key: Key, pressed: bool, font: Option<&FieldFont>) {
        if !self.focused {
            return;
        }
        if !pressed {
            if matches!(key, Key::LeftShift | Key::RightShift) {
                self.shift = false;
            }
            if self.repeat.is_some_and(|(k, _)| k == key) {
                self.repeat = None;
            }
            return;
        }
        match key {
            Key::Enter | Key::NumpadEnter => {
                self.focused = false;
                self.repeat = None;
            }
            Key::Escape if self.text.is_empty() => {
                self.focused = false;
                self.repeat = None;
            }
            Key::Escape => {
                self.text.clear();
                self.caret = 0;
                self.repeat = Some((key, false));
            }
            _ => {
                self.apply(key, font);
                self.repeat = Some((key, false));
            }
        }
    }

    /// The per-frame key repeat at `now`.
    fn frame(&mut self, now: u64, font: Option<&FieldFont>) {
        if !self.focused {
            return;
        }
        match self.repeat {
            None => self.mark = now,
            Some((key, false)) => {
                if now.saturating_sub(self.mark) > REPEAT_DELAY {
                    self.repeat = Some((key, true));
                    self.mark = now;
                }
            }
            Some((key, true)) => {
                if now.saturating_sub(self.mark) > REPEAT_PERIOD {
                    match key {
                        // Repeating Esc clears what is there (nothing).
                        Key::Escape => {}
                        _ => self.apply(key, font),
                    }
                    self.mark = now;
                }
            }
        }
    }

    /// Character `c` typed through text entry (our design, #94): typed as
    /// a key typing it would, if one does (section 4.4) and it fits.
    fn insert(&mut self, c: char, font: Option<&FieldFont>) {
        // Typographic quotes (a device keyboard's smart punctuation) as
        // the plain ones the field's keys type.
        let c = match c {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            c => c,
        };
        let typeable = Key::ALL
            .iter()
            .any(|&k| typed(k, false) == Some(c) || typed(k, true) == Some(c));
        if !self.focused || !typeable {
            return;
        }
        if font.is_none_or(|f| f.width(&self.text) <= FIELD_MAX_WIDTH) {
            let at = self.byte(self.caret);
            self.text.insert(at, c);
            self.caret += 1;
        }
        self.repeat = None;
    }

    /// A press on the on-screen keyboard (our design, #94): its character
    /// key with its own shift, Backspace, or Done (Enter).
    fn keyboard(&mut self, press: Press, font: Option<&FieldFont>) {
        match press {
            Press::Char(key, shift) => {
                if let Some(c) = typed(key, shift) {
                    self.insert(c, font);
                }
            }
            Press::Backspace => {
                self.key(Key::Backspace, true, font);
                self.key(Key::Backspace, false, font);
            }
            Press::Done => {
                self.key(Key::Enter, true, font);
                self.key(Key::Enter, false, font);
            }
        }
    }

    fn focus(&mut self) {
        if !self.focused {
            self.focused = true;
            self.caret = self.chars();
            self.repeat = None;
        }
    }

    fn set(&mut self, text: &str) {
        self.text = text.to_owned();
        self.caret = self.caret.min(self.chars());
    }
}

/// What a box shown by a screen is for.
#[derive(Clone, Debug, PartialEq)]
enum BoxFor {
    /// OK only closes it.
    Notice,
    Overwrite(String),
    Delete,
    ConfirmLoad(String),
}

/// What the screens share.
#[derive(Clone, Debug)]
struct Screen {
    desc: ScreenDesc,
    scroll: ScrollBarProfile,
    list: SaveList,
    /// Action, Delete, Cancel.
    buttons: [PushButton; 3],
    message: Option<(MessageBox, BoxFor)>,
    opened: u64,
    cursor: [f32; 2],
    last_update: Option<u64>,
    left_was_held: bool,
    stubbed: Vec<Stubbed>,
    /// The save screen's name field, a d-pad target (our design, #94).
    field: Option<Rect>,
}

impl Screen {
    fn new(desc: &ScreenDesc, sl: &SaveLoadData, rows: Vec<SaveRow>) -> Self {
        let list_rect = desc.abs(&desc.list);
        Self {
            list: SaveList::new(list_rect, desc.item_height, sl.scroll.width, rows),
            desc: desc.clone(),
            scroll: sl.scroll.clone(),
            buttons: [PushButton::default(); 3],
            message: None,
            opened: 0,
            cursor: [512.0, 384.0],
            last_update: None,
            left_was_held: false,
            stubbed: Vec::new(),
            field: None,
        }
    }

    fn open(&mut self, now: u64, cursor: [f32; 2]) {
        self.opened = now;
        self.cursor = cursor;
        self.last_update = Some(now);
        self.left_was_held = false;
    }

    fn button_rect(&self, i: usize) -> Rect {
        self.desc.abs(self.desc.buttons()[i])
    }

    /// The per-frame update of every shown element (section 3.1).
    fn frame(&mut self) {
        let c = self.cursor;
        for i in 0..3 {
            let r = self.button_rect(i);
            self.buttons[i].frame(r, c);
        }
        self.list.frame(c);
        if let Some((m, _)) = &mut self.message {
            m.frame(c);
        }
    }

    /// The buttons' share of event `e`: the activated one.
    fn buttons_event(&mut self, e: Ev) -> Option<usize> {
        let c = self.cursor;
        let mut activated = None;
        for i in 0..3 {
            let r = self.button_rect(i);
            if matches!(e, Ev::Press { .. }) {
                // As for the box (our choice, for taps).
                self.buttons[i].frame(r, c);
            }
            if self.buttons[i].receives(r, c) && self.buttons[i].event(e) == Pushed::Activated {
                activated = Some(i);
            }
        }
        activated
    }

    fn show(&mut self, data: &GameData, sl: &SaveLoadData, name: &str, purpose: BoxFor) {
        // A box the data lacks is not shown (the real-data test checks
        // they all exist).
        if let Ok(desc) = sl.message_box(data, name) {
            self.message = Some((MessageBox::new(desc, self.cursor), purpose));
        }
    }

    fn targets(&self) -> Vec<Rect> {
        if let Some((m, _)) = &self.message {
            return m.targets();
        }
        let mut t = self.list.targets();
        t.extend(self.field);
        t.extend((0..3).map(|i| self.button_rect(i)));
        t
    }

    fn draw_common(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let d = &self.desc;
        let top = |x: &StaticDesc| x.window.absolute(0.0, 0.0);
        ctx.picture(
            s,
            "back_video",
            &d.back_video,
            top(&d.back_video),
            0.0,
            self.opened,
            true,
            None,
        )?;
        ctx.picture(
            s,
            "background",
            &d.background,
            top(&d.background),
            0.0,
            0,
            false,
            None,
        )?;
        ctx.picture(
            s,
            "newspaper_video",
            &d.newspaper_video,
            top(&d.newspaper_video),
            0.0,
            self.opened,
            true,
            None,
        )?;
        ctx.picture(s, "form", &d.form, d.form(), 0.0, 0, false, None)?;
        let cr = d.abs(&d.caption);
        let (b, font) = TextBox::of_static(&d.caption, cr);
        if let Some(key) = d.caption.text.as_ref().and_then(|t| t.key.as_deref()) {
            ctx.text(s, "caption", &ctx.data.text(key), &b, font)?;
        }
        Ok(())
    }

    fn draw_list_and_buttons(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let d = &self.desc;
        if let Some(t) = &d.list_frame.texture {
            ctx.frame_window(s, "list_frame", &t.name, d.abs(&d.list_frame))?;
        }
        self.list.draw(ctx, s, &self.scroll)?;
        for (i, (b, layer)) in d
            .buttons()
            .into_iter()
            .zip(["btn action", "btn_delete", "btn_cancel"])
            .enumerate()
        {
            dialog::draw_button(
                ctx,
                s,
                layer,
                b,
                self.button_rect(i),
                self.buttons[i].look(),
            )?;
        }
        Ok(())
    }

    fn draw_top(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        if let Some((m, _)) = &self.message {
            m.draw(ctx, s)?;
        }
        crate::draw::cursor(ctx, s, self.cursor)
    }

    /// This frame's events; the cursor moves with them.
    /// This frame's events, the gamepad's only when `pad`.
    fn events(&mut self, data: &GameData, now: u64, input: &Input, pad: bool) -> Vec<Ev> {
        let dt = now.saturating_sub(self.last_update.unwrap_or(now)).min(100) as f32 / 1000.0;
        self.last_update = Some(now);
        dialog::events_from(data, input, self.cursor, dt, pad)
    }

    /// Moves the cursor for `e`; a d-pad press goes to the nearest target.
    fn follow(&mut self, e: &mut Ev) {
        match *e {
            Ev::Move(p) => self.cursor = p,
            Ev::Navigate(d) => {
                if let Some(p) = dialog::navigate(self.cursor, d, &self.targets()) {
                    self.cursor = p;
                    *e = Ev::Move(p);
                }
            }
            _ => {}
        }
    }

    /// Hold steps (section 3.5) at `now`; `last_hold` is the time of the
    /// last one, shared by every scroll bar.
    fn hold(&mut self, now: u64, input: &Input, last_hold: &mut u64) {
        let held = dialog::left_held(input);
        if held && self.left_was_held && self.message.is_none() {
            let c = self.cursor;
            self.list.held(now, last_hold, c);
        }
        self.left_was_held = held;
    }

    fn state(&self) -> ScreenState {
        ScreenState {
            list: self.list.state(),
            buttons: (0..3)
                .map(|i| {
                    let r = self.button_rect(i);
                    ScreenButtonState {
                        name: self.desc.buttons()[i].window.name.clone(),
                        rect: [r.x, r.y, r.width, r.height],
                        look: self.buttons[i].look(),
                    }
                })
                .collect(),
            message: self.message.as_ref().map(|(m, _)| m.state()),
            cursor: self.cursor,
            stubbed: self.stubbed.clone(),
        }
    }
}

/// Deletes the selected save (section 3.6) once confirmed.
fn delete_selected(list: &mut SaveList, saves: &mut SavedGames, out: &mut Outcome) {
    let Some(path) = list.selected_row().map(|r| r.path.clone()) else {
        return;
    };
    out.deleted.extend(saves.delete(&path));
    list.remove_selected();
}

/// The save-game screen (section 4).
pub struct SaveScreen {
    screen: Screen,
    field: NameField,
    /// The on-screen keyboard, while a gamepad types the name (#94).
    keyboard: Option<OnScreenKeyboard>,
    /// Gamepad buttons held as the keyboard closed are not yet released.
    pad_swallowed: bool,
}

impl SaveScreen {
    /// Creates the screen (section 4.2): the list filled from `saves`.
    pub fn new(sl: &SaveLoadData, saves: &SavedGames) -> Self {
        let mut screen = Screen::new(&sl.save, sl, saves.rows(true));
        let d = &sl.save;
        screen.field = d.edit.as_ref().map(|e| d.abs(e));
        Self {
            screen,
            field: NameField::default(),
            keyboard: None,
            pad_swallowed: false,
        }
    }

    /// Shows the screen at `now` with the cursor at `cursor`, keeping its
    /// state from an earlier visit (section 12).
    pub fn open(&mut self, now: u64, cursor: [f32; 2]) {
        self.screen.open(now, cursor);
        self.keyboard = None;
        self.pad_swallowed = false;
    }

    pub fn cursor(&self) -> [f32; 2] {
        self.screen.cursor
    }

    /// The name field's rectangle (UI units) and whether it has focus, for
    /// a platform's text entry over it (the device's keyboard on touch
    /// screens, #94); `None` while a box covers the screen.
    pub fn text_field(&self) -> Option<(Rect, bool)> {
        if self.screen.message.is_some() {
            return None;
        }
        Some((self.field_rect(), self.field.focused))
    }

    /// The name field's font (its `text` child's).
    pub fn field_font(&self) -> UiFont {
        self.screen
            .desc
            .edit
            .as_ref()
            .and_then(|e| e.text.as_ref())
            .and_then(|t| t.font)
            .unwrap_or(UiFont::DEFAULT)
    }

    fn field_rect(&self) -> Rect {
        let d = &self.screen.desc;
        d.abs(d.edit.as_ref().expect("save screen"))
    }

    /// One frame at `now`.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        data: &GameData,
        sl: &SaveLoadData,
        saves: &mut SavedGames,
        font: Option<&FieldFont>,
        now: u64,
        input: &Input,
        last_hold: &mut u64,
    ) -> Outcome {
        let mut out = Outcome::default();
        // While the on-screen keyboard shows it takes the gamepad (#94).
        let typing = self.keyboard.is_some();
        let pad = !typing && !self.pad_swallowed;
        if let Some(k) = &mut self.keyboard {
            let presses = input.pad_changes().iter().filter(|c| c.1);
            for p in presses.filter_map(|&(b, _)| k.press(b)) {
                self.field.keyboard(p, font);
            }
        }
        for mut e in self.screen.events(data, now, input, pad) {
            if out.closed {
                break;
            }
            // The keyboard over the screen: the mouse and touch don't reach
            // what is under it (our choice); keys still do.
            if typing && !matches!(e, Ev::Key { .. } | Ev::Text(_) | Ev::Quit { .. }) {
                continue;
            }
            self.screen.follow(&mut e);
            let c = self.screen.cursor;
            if let Some((m, purpose)) = &mut self.screen.message {
                if let Some(answer) = m.event(e, c) {
                    let purpose = purpose.clone();
                    self.screen.message = None;
                    match (purpose, answer) {
                        (BoxFor::Overwrite(n), Answer::Yes) => self.save(&n, &mut out),
                        (BoxFor::Delete, Answer::Yes) => {
                            delete_selected(&mut self.screen.list, saves, &mut out);
                            // Section 4.3: the name rule of a click.
                            if !self.screen.list.rows.is_empty() {
                                self.take_focused_name();
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }
            match e {
                Ev::Key { key, pressed } => self.field.key(key, pressed, font),
                Ev::Text(c) => self.field.insert(c, font),
                Ev::Quit { .. } => {
                    // Section 3.7: a press or a release.
                    out.closed = true;
                }
                _ => {
                    if matches!(e, Ev::Press { double: false }) && contains(self.field_rect(), c) {
                        self.field.focus();
                    }
                    if let ListEvent::Clicked(_) = self.screen.list.event(e, c) {
                        self.take_focused_name();
                    }
                    match self.screen.buttons_event(e) {
                        Some(0) => self.save_pressed(data, sl, saves, &mut out),
                        Some(1) => {
                            if self.screen.list.selected_name().is_some() {
                                self.screen.show(data, sl, DELETE_FILE_NAME, BoxFor::Delete);
                            }
                        }
                        Some(_) => out.closed = true,
                        None => {}
                    }
                }
            }
        }
        // A gamepad's south pressing inside the field (giving it focus)
        // opens the on-screen keyboard; it closes with the focus (#94).
        let south = input.pad_changes().contains(&(GamepadButton::South, true));
        if !typing && south && self.field.focused && contains(self.field_rect(), self.screen.cursor)
        {
            self.keyboard = Some(OnScreenKeyboard::default());
        }
        if !self.field.focused || out.closed || self.screen.message.is_some() {
            self.keyboard = None;
        }
        // The buttons held as it closes stay its until released: Done's
        // release would otherwise close the screen (section 3.7).
        self.pad_swallowed = (typing || self.pad_swallowed)
            && self.keyboard.is_none()
            && GamepadButton::ALL.iter().any(|&b| input.pad_held(b));
        self.field.frame(now, font);
        if !typing {
            self.screen.hold(now, input, last_hold);
        }
        self.screen.frame();
        out
    }

    /// Section 4.3: the name of the row at the focused index, if any.
    fn take_focused_name(&mut self) {
        let list = &self.screen.list;
        if let Some((row, _)) = list.focused.and_then(|i| list.rows.get(i)) {
            let name = row.name.clone();
            self.field.set(&name);
        }
    }

    fn save_pressed(
        &mut self,
        data: &GameData,
        sl: &SaveLoadData,
        saves: &SavedGames,
        out: &mut Outcome,
    ) {
        let n = self.field.text.clone();
        if n.is_empty() {
            self.screen.show(data, sl, EMPTY_FILE_NAME, BoxFor::Notice);
        } else if saves.exists(&n) {
            self.screen
                .show(data, sl, FILE_ALREADY_EXIST, BoxFor::Overwrite(n));
        } else {
            self.save(&n, out);
        }
    }

    /// `save N` and the return to the menu (sections 4.3, 8.1).
    fn save(&mut self, n: &str, out: &mut Outcome) {
        if n.contains(FORBIDDEN) {
            // Refused: back to the menu, nothing saved or shown.
            out.closed = true;
            return;
        }
        // TODO(#75): write `<N>.sav` (and its picture) and return to the
        // menu. Until a save the original game loads can be written,
        // nothing is written and the screen stays.
        self.screen.stubbed.push(Stubbed::Save(n.to_owned()));
    }

    pub fn scene(&self, ctx: &mut Ctx) -> io::Result<Scene> {
        let mut scene = Scene::new(ctx.resolution);
        let s = &mut scene;
        self.screen.draw_common(ctx, s)?;
        let f = self.field_rect();
        let edit = self.screen.desc.edit.as_ref().expect("save screen");
        if let Some(t) = &edit.texture {
            ctx.frame_line(s, "edit", &t.name, (f.x, f.y), f.width)?;
        }
        let (mut b, font) = TextBox::of_static(edit, f);
        b.rect = Rect::new(f.x + FIELD_INDENT, f.y, f.width - FIELD_INDENT, f.height);
        b.offset = (0.0, 0.0);
        b.align = HAlign::Left;
        b.vert_align = VAlign::Centre;
        b.complex = false;
        ctx.text(
            s,
            "edit text",
            &crate::data::encode(&self.field.text),
            &b,
            font,
        )?;
        if self.field.focused {
            let (sx, _) = ctx.scale();
            let before = crate::data::encode(self.field.before_caret());
            let width = ctx
                .assets
                .fonts(ctx.tree, ctx.data, ctx.resolution.height)?
                .get(font)
                .width(&before)
                / sx;
            let mut caret = b;
            caret.rect.x += width;
            ctx.text(s, "edit caret", b"_", &caret, font)?;
        }
        self.screen.draw_list_and_buttons(ctx, s)?;
        if let Some(k) = &self.keyboard {
            k.draw(ctx, s, self.field_font())?;
        }
        self.screen.draw_top(ctx, s)?;
        Ok(scene)
    }

    pub fn state(&self) -> SaveScreenState {
        let f = self.field_rect();
        SaveScreenState {
            screen: self.screen.state(),
            name: self.field.text.clone(),
            focused: self.field.focused,
            caret: self.field.caret,
            field: [f.x, f.y, f.width, f.height],
            keyboard: self.keyboard.as_ref().map(OnScreenKeyboard::state),
        }
    }
}

/// What the load screen's information area shows of a save (section 5.2,
/// sav.md 11.4).
#[derive(Clone, Debug, PartialEq)]
pub struct SaveInfo {
    /// The level's directory name.
    pub level: String,
    /// Game milliseconds.
    pub game_time: u64,
    pub health: f32,
}

/// `config\alife.ltx`, or the system configuration where it holds
/// `[alife]` already.
fn alife(tree: &dyn Vfs, data: &GameData) -> io::Result<Config> {
    match data.system.section("alife") {
        Some(_) => Ok(data.system.clone()),
        None => config::load(tree, &data.aliases.path("$game_config$", "alife.ltx")?),
    }
}

fn game_graph(tree: &dyn Vfs, data: &GameData) -> io::Result<GameGraph> {
    GameGraph::parse(&vfs::read(
        tree,
        &data.aliases.path("$game_data$", "game.graph")?,
    )?)
}

impl SaveInfo {
    /// What sav.md section 11.4 reads from save file `file`: chunk 5's game
    /// time, the first object record's (the player's) health and the level
    /// of its game-graph vertex; for a file that is not valid, or one whose
    /// first record cannot be read (our choice), the defaults
    /// ([`SaveInfo::defaults`]).
    pub fn read(tree: &dyn Vfs, data: &GameData, file: &[u8]) -> io::Result<Self> {
        match Self::read_valid(tree, data, file) {
            Ok(info) => Ok(info),
            Err(_) => Self::defaults(tree, data),
        }
    }

    fn read_valid(tree: &dyn Vfs, data: &GameData, file: &[u8]) -> io::Result<Self> {
        let save = Save::read(file)?;
        let first = save
            .objects
            .first()
            .ok_or_else(|| invalid("saved game without objects"))?;
        let section = object::section(&first.spawn)?;
        let class = data
            .system
            .get(&section, "class")
            .flatten()
            .ok_or_else(|| invalid(format!("[{section}] without class")))?;
        let record = ObjectRecord::parse(&first.spawn, &first.update, class, true)?;
        let Object::Player(player) = record.object else {
            return Err(invalid("the first record is not the player's"));
        };
        let graph = game_graph(tree, data)?;
        let level = graph.vertex_level(player.creature.object.game_graph_vertex)?;
        Ok(Self {
            level: level.name.clone(),
            game_time: save.game_time.time,
            health: player.creature.health,
        })
    }

    /// The defaults for a file that is not valid: the configured New Game
    /// start time, health 1.0, and the level of the game graph's first
    /// level-table entry, taken as the smallest level id (believed, sav.md
    /// 11.4).
    pub fn defaults(tree: &dyn Vfs, data: &GameData) -> io::Result<Self> {
        let start = GameClock::new_game(&alife(tree, data)?)?.time();
        let graph = game_graph(tree, data)?;
        let level = graph
            .levels
            .iter()
            .min_by_key(|l| l.id)
            .ok_or_else(|| invalid("game.graph without levels"))?;
        Ok(Self {
            level: level.name.clone(),
            game_time: start,
            health: 1.0,
        })
    }

    /// `file_data`'s three lines (section 5.2), joined by `\n` for complex
    /// mode.
    pub fn text(&self, data: &GameData) -> String {
        let s = |k: &str| data.strings.get(k).to_owned();
        format!(
            "{}: {}\\n{}: {}\\n{} {}%",
            s("st_level"),
            s(&self.level),
            s("ui_inv_time"),
            display_time(self.game_time),
            s("ui_inv_health"),
            (self.health * 100.0).trunc() as i64,
        )
    }
}

/// Game time `t` as the load screen shows it, `HH:MM MM/DD/YYYY`, from the
/// game's split of the value (section 5.2), which is not an exact inverse
/// of sav.md section 12.
pub fn display_time(t: u64) -> String {
    let t = t / 1000;
    let t = t / 60;
    let minutes = t % 60;
    let t = t / 60;
    let hours = t % 24;
    let days = t / 24;
    let year = days * 400 / 146_097 + 1;
    let mut r = (days + year / 100 + 1).saturating_sub(365 * (year - 1) + year / 4 + year / 400);
    let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
    ];
    let mut month = 1;
    for len in lengths {
        if r > len {
            r -= len;
            month += 1;
        } else {
            break;
        }
    }
    format!("{hours:02}:{minutes:02} {month:02}/{r:02}/{year:04}")
}

/// The load-game screen (section 5).
pub struct LoadScreen {
    screen: Screen,
    /// A single-player game runs (section 5.3).
    in_game: bool,
    caption: String,
    data_text: String,
}

impl LoadScreen {
    pub fn new(sl: &SaveLoadData, saves: &SavedGames, in_game: bool) -> Self {
        Self {
            screen: Screen::new(&sl.load, sl, saves.rows(false)),
            in_game,
            caption: String::new(),
            data_text: String::new(),
        }
    }

    pub fn open(&mut self, now: u64, cursor: [f32; 2]) {
        self.screen.open(now, cursor);
    }

    pub fn cursor(&self) -> [f32; 2] {
        self.screen.cursor
    }

    /// One frame at `now`; reads saves' information from `saves` and the
    /// game data from `tree`.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        sl: &SaveLoadData,
        saves: &mut SavedGames,
        now: u64,
        input: &Input,
        last_hold: &mut u64,
    ) -> Outcome {
        let mut out = Outcome::default();
        for mut e in self.screen.events(data, now, input, true) {
            if out.closed {
                break;
            }
            self.screen.follow(&mut e);
            let c = self.screen.cursor;
            if let Some((m, purpose)) = &mut self.screen.message {
                if let Some(answer) = m.event(e, c) {
                    let purpose = purpose.clone();
                    self.screen.message = None;
                    match (purpose, answer) {
                        (BoxFor::ConfirmLoad(n), Answer::Yes) => self.load(&n),
                        (BoxFor::Delete, Answer::Yes) => {
                            delete_selected(&mut self.screen.list, saves, &mut out);
                            if self.screen.list.rows.is_empty() {
                                // The area keeps showing the deleted save.
                            } else if self.screen.list.selected.is_some() {
                                self.fill(tree, data, saves);
                            } else {
                                self.clear();
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }
            match e {
                Ev::Quit { .. } => out.closed = true,
                Ev::Key { .. } => {}
                _ => {
                    let list = self.screen.list.event(e, c);
                    if let ListEvent::Clicked(_) = list {
                        self.fill(tree, data, saves);
                    }
                    let double = list == ListEvent::Double;
                    match self.screen.buttons_event(e) {
                        Some(0) => self.load_pressed(data, sl, saves),
                        Some(1) => {
                            if self.screen.list.selected_name().is_some() {
                                self.screen.show(data, sl, DELETE_FILE_NAME, BoxFor::Delete);
                            }
                        }
                        Some(_) => out.closed = true,
                        None if double => self.load_pressed(data, sl, saves),
                        None => {}
                    }
                }
            }
        }
        self.screen.hold(now, input, last_hold);
        self.screen.frame();
        out
    }

    /// Section 5.2 for the selected row.
    fn fill(&mut self, tree: &dyn Vfs, data: &GameData, saves: &SavedGames) {
        let Some(name) = self.screen.list.selected_name().map(str::to_owned) else {
            return;
        };
        self.caption = name.clone();
        let path = self
            .screen
            .list
            .selected_row()
            .map(|r| r.path.clone())
            .unwrap_or_default();
        match saves.read(&path) {
            None => {
                // The file is gone: its row goes too.
                self.data_text = "no file data".to_owned();
                self.screen.list.remove_selected();
            }
            Some(file) => {
                // A file that cannot be read shows what a file that is not
                // valid shows (our choice).
                let file = file.unwrap_or_default();
                self.data_text = SaveInfo::read(tree, data, &file)
                    .map(|i| i.text(data))
                    .unwrap_or_default();
            }
        }
    }

    fn clear(&mut self) {
        self.caption.clear();
        self.data_text.clear();
    }

    fn load_pressed(&mut self, data: &GameData, sl: &SaveLoadData, saves: &SavedGames) {
        let Some(row) = self.screen.list.selected_row().cloned() else {
            return;
        };
        let n = row.name;
        if !saves.is_valid(&row.path) {
            self.screen
                .show(data, sl, INVALID_SAVED_GAME, BoxFor::Notice);
        } else if self.in_game {
            self.screen
                .show(data, sl, CONFIRM_LOAD_SAVE, BoxFor::ConfirmLoad(n));
        } else {
            self.load(&n);
        }
    }

    /// Loads save `n` (sections 5.3, 9.3).
    fn load(&mut self, n: &str) {
        // TODO(#75): load the save (sav.md 11.2) and close the menu as the
        // level starts loading. Nothing is loaded until then.
        self.screen.stubbed.push(Stubbed::Load(n.to_owned()));
    }

    pub fn scene(&self, ctx: &mut Ctx) -> io::Result<Scene> {
        let mut scene = Scene::new(ctx.resolution);
        let s = &mut scene;
        self.screen.draw_common(ctx, s)?;
        let d = &self.screen.desc;
        let info = d.info.as_ref().expect("load screen");
        let pr = d.abs(&info.picture);
        let start = s.sprites.len();
        ctx.picture(
            s,
            "picture",
            &info.picture,
            pr,
            0.0,
            self.screen.opened,
            true,
            None,
        )?;
        if info.picture.clipper {
            s.clip_from(start, pixels(pr, ctx.scale()));
        }
        ctx.picture(
            s,
            "file_info",
            &info.file_info,
            d.abs(&info.file_info),
            0.0,
            0,
            false,
            None,
        )?;
        for (desc, text, layer) in [
            (&info.file_caption, &self.caption, "file_caption"),
            (&info.file_data, &self.data_text, "file_data"),
        ] {
            let (b, font) = TextBox::of_static(desc, d.abs(desc));
            ctx.text(s, layer, &crate::data::encode(text), &b, font)?;
        }
        self.screen.draw_list_and_buttons(ctx, s)?;
        self.screen.draw_top(ctx, s)?;
        Ok(scene)
    }

    pub fn state(&self) -> LoadScreenState {
        LoadScreenState {
            screen: self.screen.state(),
            file_caption: self.caption.clone(),
            file_data: self.data_text.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RowState {
    pub name: String,
    pub time: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ListState {
    pub rows: Vec<RowState>,
    pub visible: usize,
    pub first: usize,
    pub selected: Option<usize>,
    pub focused: Option<usize>,
    pub scroll_bar: bool,
    /// Absolute (x, y, width, height).
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, Serialize)]
pub struct ScreenButtonState {
    pub name: String,
    pub rect: [f32; 4],
    pub look: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScreenState {
    pub list: ListState,
    pub buttons: Vec<ScreenButtonState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<MessageBoxState>,
    pub cursor: [f32; 2],
    /// What the screen left undone until #75.
    pub stubbed: Vec<Stubbed>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SaveScreenState {
    #[serde(flatten)]
    pub screen: ScreenState,
    pub name: String,
    pub focused: bool,
    pub caret: usize,
    /// The name field, UI units.
    pub field: [f32; 4],
    /// The on-screen keyboard while it shows (#94).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyboard: Option<KeyboardState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LoadScreenState {
    #[serde(flatten)]
    pub screen: ScreenState,
    pub file_caption: String,
    pub file_data: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saved_games::LocalTime;

    #[test]
    fn game_time_as_the_load_screen_shows_it() {
        // Section 13 fact 7 and section 5.2's consequences.
        assert_eq!(display_time(63_471_533_400_000), "05:30 05/01/2012");
        assert_eq!(display_time(63_492_595_200_000), "00:00 01/01/2013");
        assert_eq!(display_time(63_524_131_200_000), "00:00 12/32/2013");
        let t = volya_engine_formats::sav::game_time(2011, 1, 1, 0, 0, 0, 0).unwrap();
        assert_eq!(display_time(t), "00:00 12/32/2010");
        let t = volya_engine_formats::sav::game_time(2012, 2, 29, 13, 7, 0, 0).unwrap();
        assert_eq!(display_time(t), "13:07 02/29/2012");
    }

    fn rows(n: usize) -> Vec<SaveRow> {
        (0..n)
            .map(|i| SaveRow {
                name: format!("s{i}"),
                path: format!("s{i}.sav"),
                modified: LocalTime::default(),
            })
            .collect()
    }

    fn list(n: usize) -> SaveList {
        SaveList::new(Rect::new(490.0, 399.0, 429.0, 180.0), 18.0, 16.0, rows(n))
    }

    fn click(l: &mut SaveList, at: [f32; 2]) -> ListEvent {
        l.event(Ev::Move(at), at);
        l.event(Ev::Press { double: false }, at);
        l.event(Ev::Release, at)
    }

    #[test]
    fn the_load_list_shows_ten_rows_and_scrolls() {
        let mut l = list(11);
        assert_eq!(l.visible, 10);
        assert!(l.bar_shown());
        // Section 13 fact 2: 11 saves on the load screen, the thumb fills
        // the track.
        assert_eq!(l.thumb(), (148.0, 148.0, 16.0));
        assert_eq!(l.up_rect(), Rect::new(903.0, 399.0, 16.0, 16.0));
        assert_eq!(l.down_rect().y, 563.0);
        // A click on the down arrow: a step on the press, one on the
        // release (the range ends after the first).
        let down = [910.0, 570.0];
        click(&mut l, down);
        assert_eq!(l.first, 1);
        l.event(Ev::Wheel { up: true }, [600.0, 450.0]);
        assert_eq!(l.first, 0);
        // A row click selects it; no selection before.
        assert_eq!(l.selected, None);
        assert_eq!(
            click(&mut l, [600.0, 399.0 + 18.0 * 2.0 + 5.0]),
            ListEvent::Clicked(2)
        );
        assert_eq!(l.selected_name(), Some("s2"));
        // A double click over the list.
        assert_eq!(
            l.event(Ev::Press { double: true }, [600.0, 450.0]),
            ListEvent::Double
        );
        // Removing the last selected row clears the selection (3.6).
        let mut l = list(2);
        click(&mut l, [600.0, 399.0 + 18.0 + 5.0]);
        l.remove_selected();
        assert_eq!((l.rows.len(), l.selected), (1, None));
        assert!(list(0).targets().is_empty());
    }

    #[test]
    fn the_save_list_thumb_follows_the_spec() {
        // Section 13 fact 2: 30 saves on the save screen.
        let mut l = SaveList::new(Rect::new(490.0, 274.0, 429.0, 287.0), 18.0, 16.0, rows(30));
        assert_eq!(l.visible, 15);
        assert_eq!(l.thumb(), (255.0, 131.0, 16.0));
        l.first = 15;
        assert_eq!(l.thumb().2, 140.0);
    }

    #[test]
    fn a_dragged_thumb_moves_with_the_cursor() {
        let mut l = SaveList::new(Rect::new(490.0, 274.0, 429.0, 287.0), 18.0, 16.0, rows(30));
        // The thumb: 131 units from 16 below the bar's top (y 290).
        let at = [910.0, 300.0];
        l.event(Ev::Move(at), at);
        l.event(Ev::Press { double: false }, at);
        assert_eq!(l.drag, Some(16.0));
        // Down by 62 units of cursor movement, wherever on the thumb.
        let to = [910.0, 362.0];
        l.event(Ev::Move(to), to);
        assert_eq!(l.drag, Some(78.0));
        // Half the range: (78 − 16) / (255 − 131) · 15 rows, rounded.
        assert_eq!(l.first, 8);
        // Far from the thumb, a movement does nothing.
        let away = [600.0, 700.0];
        l.event(Ev::Move(away), away);
        assert_eq!(l.drag, Some(78.0));
        l.event(Ev::Release, away);
        assert_eq!(l.drag, None);
    }

    #[test]
    fn the_caret_counts_characters() {
        let mut f = NameField::default();
        f.set("сейв");
        f.focus();
        assert_eq!(f.caret, 4);
        assert_eq!(f.before_caret(), "сейв");
        f.key(Key::Left, true, None);
        f.key(Key::Backspace, true, None);
        assert_eq!((f.text.as_str(), f.caret), ("сев", 2));
        assert_eq!(f.before_caret(), "се");
        f.key(Key::A, true, None);
        f.key(Key::Delete, true, None);
        assert_eq!(f.text, "сеa");
        f.key(Key::Right, true, None);
        assert_eq!(f.caret, 3);
    }

    #[test]
    fn the_name_field_types_by_key_with_shift_and_repeat() {
        let mut f = NameField::default();
        f.key(Key::A, true, None);
        assert_eq!(f.text, "", "no focus, no typing");
        f.focus();
        f.frame(1000, None);
        for (key, pressed) in [
            (Key::LeftShift, true),
            (Key::M, true),
            (Key::M, false),
            (Key::LeftShift, false),
            (Key::O, true),
            (Key::O, false),
            (Key::Digit1, true),
            (Key::Digit1, false),
        ] {
            f.key(key, pressed, None);
        }
        assert_eq!(f.text, "Mo1");
        f.key(Key::Left, true, None);
        f.key(Key::Left, false, None);
        f.key(Key::Backspace, true, None);
        f.key(Key::Backspace, false, None);
        assert_eq!((f.text.as_str(), f.caret), ("M1", 1));
        // Held `a`: once, then from a little over 500 ms after the last
        // frame before the press, every ~100 ms.
        f.frame(2000, None);
        f.key(Key::A, true, None);
        f.frame(2016, None);
        f.frame(2401, None);
        assert_eq!(f.text, "Ma1");
        f.frame(2501, None);
        assert_eq!(f.text, "Ma1");
        f.frame(2502, None);
        assert_eq!(f.text, "Maa1");
        // Pressing and releasing `b` ends all repeating.
        f.key(Key::B, true, None);
        f.key(Key::B, false, None);
        f.frame(4000, None);
        f.frame(5000, None);
        assert_eq!(f.text, "Maab1");
        // Esc clears a filled field; on an empty one it ends focus.
        f.key(Key::Escape, true, None);
        assert!(f.text.is_empty() && f.focused);
        f.key(Key::Escape, true, None);
        assert!(!f.focused);
        assert_eq!(typed(Key::Slash, true), Some('?'));
        assert_eq!(typed(Key::Numpad1, false), None);
    }

    #[test]
    fn text_entry_and_the_on_screen_keyboard_type_the_fields_characters_only() {
        let mut f = NameField::default();
        f.insert('a', None);
        assert_eq!(f.text, "", "no focus, no typing");
        f.focus();
        // The device keyboard's characters: those a key types, others
        // (Cyrillic, a tab, an accent) dropped.
        for c in "Ab 1жé\t_?".chars() {
            f.insert(c, None);
        }
        assert_eq!((f.text.as_str(), f.caret), ("Ab 1_?", 6));
        // Typographic quotes as the plain ones.
        for c in "\u{2019}\u{201C}".chars() {
            f.insert(c, None);
        }
        assert_eq!(f.text, "Ab 1_?'\"");
        f.key(Key::Backspace, true, None);
        f.key(Key::Backspace, false, None);
        f.key(Key::Backspace, true, None);
        f.key(Key::Backspace, false, None);
        // The on-screen keyboard: its own shift, Backspace, Done.
        f.keyboard(Press::Char(Key::Digit1, true), None);
        f.keyboard(Press::Char(Key::Q, false), None);
        assert_eq!(f.text, "Ab 1_?!q");
        f.keyboard(Press::Backspace, None);
        assert_eq!(f.text, "Ab 1_?!");
        assert!(f.focused);
        f.keyboard(Press::Done, None);
        assert!(!f.focused, "Done is Enter: the focus ends");
        // The field's shift flag is untouched by either.
        assert!(!f.shift);
    }
}
