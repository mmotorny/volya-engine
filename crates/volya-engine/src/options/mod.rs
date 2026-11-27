// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The options screen (docs/behavior/options-screens.md): its layers, the
//! four tabs and the advanced-video page, every control bound to its
//! console variable, Apply/Cancel/Esc, the restart marks and the restart
//! message, the key table with capture and conflicts.
//!
//! The screen reads and sets the settings through the console
//! ([`crate::core::console`]); what a setting then does is the app's
//! business ([`crate::app`]).
//!
//! **Input.** The screen is the game's own mouse and keyboard interface
//! (section 13). Touch and gamepads reach every control through the same
//! mapping as the title menu (our design, like `volya-engine-core::input`):
//!
//! - touch is a touchpad for the cursor (#108): a tap clicks at the cursor,
//!   a double-tap-and-hold drags a slider or a scroll bar, two fingers
//!   scroll a list;
//! - a gamepad's d-pad moves the cursor to the nearest control in that
//!   direction (scrolling a list to show it), the left stick moves it
//!   freely ([`STICK_SPEED`]), south (A / cross) is the left button and
//!   east (B / circle) is Escape.
//!
//! Key rebinding (section 11.4) waits for the next key or mouse button; a
//! gamepad's south button or a tap with the cursor on the waiting cell is a
//! left press over it and binds `mouse1` (what the spec allows without a keyboard). Gamepad
//! buttons are not bindable: the gamepad's own mapping is fixed
//! ([`crate::controls`]).

pub mod widgets;

use std::io;

use serde::Serialize;

use crate::core::console::{self, ACTIONS, Console, KEYS, Kind as VarKind};
use crate::core::input::{Action, Control, GamepadAxis, GamepadButton, Input, Key, MouseButton};
use crate::core::ui::load_ui_file;
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::draw::Ctx;
use crate::scene::Scene;
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::layout::{HAlign, Rect, StaticDesc, VAlign, require};
use volya_engine_formats::ui::options::{ControlDesc, KeyGroup, ScrollBarProfile, key_groups};
use volya_engine_formats::ui::{Color, Resolution};
use widgets::{Cell, DropDown, ScrollBar, THUMB, Track};

/// Variables whose saving marks a video restart, and the one marking a
/// sound restart (section 5.4).
const VIDEO_RESTART: [&str; 7] = [
    "vid_mode",
    "_preset",
    "rs_fullscreen",
    "r__supersample",
    "rs_refresh_60hz",
    "rs_no_v_sync",
    "texture_lod",
];
const SOUND_RESTART: &str = "snd_efx";
/// The never-written backup byte of `check_mouseinvert` (section 5.2): not
/// established (section 17). Our choice is a value that is neither 0 nor
/// 1, so every toggle of the box is applied.
const MOUSE_INVERT_BACKUP: u8 = 0xcd;
/// The renderer list's dynamic entries, `renderer_r2a` and `renderer_r2`:
/// disabled while Volya Engine draws only the static renderer (decided by
/// the user, a deviation). A stored value naming one is kept as written and
/// drawn as `renderer_r1`, which the list shows.
// TODO(#385): enable them once the dynamic renderers exist.
const DYNAMIC_RENDERERS: [usize; 2] = [1, 2];
/// Real values differ when they are this far apart (section 5.2).
const CHANGED_EPSILON: f32 = 0.000_01;
/// The page windows' top-left, absolute (section 3.2).
const PAGE: (f32, f32) = (470.0, 242.0);
/// The dialog's top-left, absolute.
const DIALOG: (f32, f32) = (415.0, 168.0);
/// Heights the controls are forced to (sections 4.5, 4.6), list rows.
const TRACK_HEIGHT: f32 = 21.0;
const LINE_HEIGHT: f32 = 23.0;
const ROW_HEIGHT: f32 = 18.0;
/// The check-box image's size and the caption's offset (section 4.4).
const CHECK_BOX: (f32, f32) = (32.0, 23.0);
/// The advanced page's rows (section 6.2).
const ADV_ROW: f32 = 30.0;
/// Cursor speed of a gamepad's left stick at full deflection, units per
/// second (our choice), and its dead zone.
pub const STICK_SPEED: f32 = 600.0;
const STICK_DEAD_ZONE: f32 = 0.2;
/// Period of a waiting key cell's blink, ms: the colour animation
/// `ui_map_area_anim`'s timing is not established (section 17), so this is
/// our choice.
const BLINK_MS: f32 = 1000.0;
/// Units the cursor moves in an open list before a press counts as a drag
/// (our choice).
const DRAG_SLOP: f32 = 4.0;

/// The screen's pages (section 3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    Video,
    Advanced,
    Sound,
    Game,
    Controls,
}

impl Page {
    /// The page a tab shows (section 3.2).
    fn of_tab(tab: usize) -> Self {
        [Page::Video, Page::Sound, Page::Game, Page::Controls][tab.min(3)]
    }

    /// The tab that is active while the page shows.
    fn tab(self) -> usize {
        match self {
            Page::Video | Page::Advanced => 0,
            Page::Sound => 1,
            Page::Game => 2,
            Page::Controls => 3,
        }
    }
}

/// Option groups (section 5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Preset,
    Video,
    VideoAdv,
    Gameplay,
    Sound,
    Controls,
}

impl Group {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "mm_opt_video_preset" => Self::Preset,
            "mm_opt_video" => Self::Video,
            "mm_opt_video_adv" => Self::VideoAdv,
            "mm_opt_gameplay" => Self::Gameplay,
            "mm_opt_sound" => Self::Sound,
            "mm_opt_controls" => Self::Controls,
            _ => return None,
        })
    }
}

/// Groups in refresh order (section 5.2); key cells follow.
const REFRESH: [Group; 6] = [
    Group::Preset,
    Group::Video,
    Group::VideoAdv,
    Group::Gameplay,
    Group::Sound,
    Group::Controls,
];
/// Groups whose shown values are backed up (section 5.2).
const BACKED_UP: [Group; 5] = [
    Group::Preset,
    Group::Video,
    Group::VideoAdv,
    Group::Gameplay,
    Group::Sound,
];
/// Groups Cancel undoes, in order (section 2.2).
const UNDONE: [Group; 4] = [Group::Preset, Group::Video, Group::VideoAdv, Group::Sound];

/// What a button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Accept,
    Cancel,
    Advanced,
    Simply,
    CheckUpdates,
    Defaults,
}

/// What an item is.
#[derive(Clone, Debug)]
enum Kind {
    /// A static: picture and/or text.
    Static,
    /// A frame window with this texture base.
    Frame(String),
    /// A frame line with this texture base, and the static's text.
    Label(String),
    Button(Act),
    Tab(usize),
    Check {
        shown: bool,
        backup: u8,
    },
    Track {
        model: Track,
        value: f32,
        backup: f32,
    },
    List(Box<ListState>),
    /// Index into the key cells.
    Cell(usize),
    /// The start and end of a scroll view's content (`ViewEnd` names it).
    ViewBegin,
    ViewEnd(usize),
}

#[derive(Clone, Debug)]
struct ListState {
    dd: DropDown,
    backup: usize,
    /// The variable's value when the backup was taken, for an undo whose
    /// backed-up entry does not exist.
    backup_status: String,
    open: bool,
    bar: ScrollBar,
}

/// One element of the screen, in attachment (drawing) order.
#[derive(Clone, Debug)]
struct Item {
    name: String,
    /// `None`: on the dialog itself (tabs, Apply, Cancel), always shown.
    page: Option<Page>,
    /// The scroll view it is in.
    view: Option<usize>,
    /// Absolute rectangle with the view scrolled to the top.
    rect: Rect,
    desc: ControlDesc,
    /// The variable and group of an option control.
    var: Option<(String, Group)>,
    enabled: bool,
    kind: Kind,
}

/// A scroll view (section 4.7).
#[derive(Clone, Debug)]
struct View {
    page: Page,
    rect: Rect,
    bar: ScrollBar,
    /// Whether the bar shows.
    shown: bool,
}

/// The restart message box (section 10).
#[derive(Clone, Debug)]
struct MessageBox {
    frame: StaticDesc,
    picture: StaticDesc,
    text: StaticDesc,
    ok: StaticDesc,
}

/// Which arrow of which bar is pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bar {
    View(usize),
    List(usize),
}

/// What the screen holds while the left button drags.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    /// A scroll bar's box, at this offset from its top.
    Box(Bar, f32),
    /// An arrow (up when `true`).
    Arrow(Bar, bool),
}

/// What closing the screen asks of the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Closed {
    /// Apply or Cancel ran the restart step with a video restart marked
    /// (section 5.4): the video mode is to be applied.
    pub video_restart: bool,
    pub sound_restart: bool,
}

/// The layout data the screen is built from.
pub struct OptionsData {
    opt: volya_engine_formats::xml::Document,
    keys: Vec<KeyGroup>,
    message: MessageBox,
    scroll: ScrollBarProfile,
}

impl OptionsData {
    /// Loads `ui_mm_opt.xml`, `ui_keybinding.xml`, `message_box.xml` and
    /// `scroll_bar.xml` (section 18) at `resolution` (the `_16` rule; none
    /// has a variant).
    pub fn load(tree: &dyn Vfs, data: &GameData, resolution: Resolution) -> io::Result<Self> {
        let ui = |name: &str| load_ui_file(tree, &data.aliases, name, resolution);
        let opt = ui("ui_mm_opt.xml")?;
        let keys = key_groups(&ui("ui_keybinding.xml")?);
        let boxes = ui("message_box.xml")?;
        let colors = &data.colors;
        let part = |p: &str| {
            StaticDesc::read(
                require(&boxes, &format!("message_box_restart_game{p}"), 0)?,
                colors,
            )
        };
        let message = MessageBox {
            frame: part("")?,
            picture: part(":picture")?,
            text: part(":message_text")?,
            ok: part(":button_ok")?,
        };
        let scroll = ScrollBarProfile::read(&ui("scroll_bar.xml")?, "default")?;
        Ok(Self {
            opt,
            keys,
            message,
            scroll,
        })
    }
}

/// The options screen.
pub struct OptionsScreen {
    /// The page can't take the browser's full screen: "Полный экран" is
    /// disabled (our deviation, docs/architecture/web-platform.md).
    no_full_screen: bool,
    items: Vec<Item>,
    cells: Vec<Cell>,
    views: Vec<View>,
    message_box: MessageBox,
    scroll: ScrollBarProfile,
    back_video: StaticDesc,
    background: StaticDesc,
    rug_video: StaticDesc,
    dialog: StaticDesc,
    page: Page,
    /// The cursor, UI units.
    cursor: [f32; 2],
    /// The item the cursor was over at the latest mouse event.
    hovered: Option<usize>,
    /// The waiting key cell (item index) and since when.
    waiting: Option<(usize, u64)>,
    /// Buttons pressed (armed) by the left button.
    armed: Option<usize>,
    /// The track bar the left button holds.
    sliding: Option<usize>,
    grab: Option<Grab>,
    /// A left press in the open list's rows: the entry under it, where it
    /// began, and whether the cursor has moved since (a drag scrolls the
    /// list instead of choosing).
    list_press: Option<(Option<usize>, [f32; 2], bool)>,
    /// The open drop-down (item index): it holds the mouse capture.
    open_list: Option<usize>,
    message: bool,
    /// Whether the restart message showed in this screen's lifetime.
    message_shown: bool,
    video_restart: bool,
    sound_restart: bool,
    /// The shared last-repeat time of the scroll bars (section 4.8).
    last_repeat: u64,
    left_was_held: bool,
    opened: u64,
    last_update: Option<u64>,
    closed: Option<Closed>,
    /// Clicks of buttons without an effect here, for tests.
    clicks: Vec<String>,
    /// The item under the cursor while a frame is drawn, found once.
    drawn_hover: std::cell::Cell<Option<Option<usize>>>,
}

fn invalid(m: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m)
}

impl OptionsScreen {
    /// Builds the screen from its layout (sections 3 and 6): every element
    /// the screen creates, in attachment order.
    pub fn new(data: &GameData, opt: &OptionsData) -> io::Result<Self> {
        let doc = &opt.opt;
        let colors = &data.colors;
        let read = |path: &str| ControlDesc::require(doc, path, colors);
        let stat = |path: &str| StaticDesc::read(require(doc, path, 0)?, colors);
        let mut b = Builder {
            items: Vec::new(),
            views: Vec::new(),
            cells: Vec::new(),
        };
        let at = |d: &ControlDesc, (x, y): (f32, f32)| d.desc.window.absolute(x, y);

        // Dialog children: the caption, then the pages, Apply, Cancel, tabs.
        let cap = read("main_dialog:cap_options")?;
        b.push("cap_options", None, at(&cap, DIALOG), cap, Kind::Static);

        let frame = |b: &mut Builder, page: Page, name: &str| -> io::Result<()> {
            let d = read(name)?;
            let base = d
                .desc
                .texture
                .as_ref()
                .map(|t| t.name.clone())
                .unwrap_or_default();
            b.push(name, Some(page), at(&d, PAGE), d, Kind::Frame(base));
            Ok(())
        };
        // Section 6.1, in attachment order.
        frame(&mut b, Page::Video, "frame")?;
        for name in [
            "cap_fullscreen",
            "check_fullscreen",
            "cap_brightness",
            "track_brightness",
            "cap_contrast",
            "track_contrast",
            "cap_gamma",
            "track_gamma",
            "cap_resolution",
            "list_resolution",
            "cap_preset",
            "list_presets",
            "cap_renderer",
            "list_renderer",
        ] {
            b.control(read(&format!("tab_video:{name}"))?, Page::Video, PAGE, None)?;
        }
        b.button(read("tab_video:btn_advanced")?, Page::Video, Act::Advanced);

        // Section 6.2: the scroll view of 13 rows, caption and control each.
        frame(&mut b, Page::Advanced, "frame_videoadv")?;
        let sv = read("video_adv:scroll_v")?;
        let view_rect = at(&sv, PAGE);
        let rows = [
            "vis_dist",
            "geometry_lod",
            "detail_density",
            "texture_lod",
            "aniso",
            "ssample",
            "r2_sun",
            "r2_sun_details",
            "light_distance",
            "npc_torch",
            "particles_distance",
            "vsync",
            "60hz",
        ];
        let v = b.view(
            Page::Advanced,
            view_rect,
            rows.len() as f32 * ADV_ROW,
            false,
            opt,
        );
        for (j, row) in rows.iter().enumerate() {
            let origin = (view_rect.x, view_rect.y + ADV_ROW * j as f32);
            for name in [
                format!("cap_{row}"),
                format!("{}_{row}", control_prefix(row)),
            ] {
                b.control(
                    read(&format!("video_adv:{name}"))?,
                    Page::Advanced,
                    origin,
                    Some(v),
                )?;
            }
        }
        b.end_view(v);
        b.button(
            read("video_adv:btn_to_simply")?,
            Page::Advanced,
            Act::Simply,
        );

        // Section 6.4.
        frame(&mut b, Page::Sound, "frame")?;
        for name in [
            "cap_mastervolume",
            "cap_musicvolume",
            "check_eax",
            "track_musicvolume",
            "track_mastervolume",
        ] {
            b.control(read(&format!("tab_sound:{name}"))?, Page::Sound, PAGE, None)?;
        }

        // Section 6.5.
        frame(&mut b, Page::Game, "frame_gameplay")?;
        for name in [
            "cap_difficulty",
            "check_tips",
            "check_crosshair",
            "check_dyn_crosshair",
            "check_show_weapon",
            "check_dist",
            "list_difficulty",
        ] {
            b.control(
                read(&format!("tab_gameplay:{name}"))?,
                Page::Game,
                PAGE,
                None,
            )?;
        }
        b.button(
            read("tab_gameplay:btn_check_updates")?,
            Page::Game,
            Act::CheckUpdates,
        );

        // Section 6.6: the keyboard label is created twice.
        frame(&mut b, Page::Controls, "frame")?;
        b.control(
            read("tab_controls:cap_mousesens")?,
            Page::Controls,
            PAGE,
            None,
        )?;
        for _ in 0..2 {
            let d = read("tab_controls:cap_keyboardsetup")?;
            let base = d
                .desc
                .texture
                .as_ref()
                .map(|t| t.name.clone())
                .unwrap_or_default();
            b.push(
                "cap_keyboardsetup",
                Some(Page::Controls),
                at(&d, PAGE),
                d,
                Kind::Label(base),
            );
        }
        b.control(
            read("tab_controls:track_mousesens")?,
            Page::Controls,
            PAGE,
            None,
        )?;
        b.control(
            read("tab_controls:check_mouseinvert")?,
            Page::Controls,
            PAGE,
            None,
        )?;
        // Section 11.3: the key table.
        let kb = read("tab_controls:key_binding")?;
        let table = at(&kb, PAGE);
        let origin = (table.x, table.y);
        for header in ["header_1", "header_2", "header_3"] {
            let d = read(&format!("tab_controls:key_binding:{header}"))?;
            let base = d
                .desc
                .texture
                .as_ref()
                .map(|t| t.name.clone())
                .unwrap_or_default();
            b.push(
                header,
                Some(Page::Controls),
                at(&d, origin),
                d,
                Kind::Label(base),
            );
        }
        let fd = read("tab_controls:key_binding:frame")?;
        let base = fd
            .desc
            .texture
            .as_ref()
            .map(|t| t.name.clone())
            .unwrap_or_default();
        b.push(
            "key_frame",
            Some(Page::Controls),
            at(&fd, origin),
            fd,
            Kind::Frame(base),
        );
        let list = read("tab_controls:key_binding:scroll_view")?;
        let interval = volya_engine_formats::ui::float_attr(
            require(doc, "tab_controls:key_binding:scroll_view", 0)?,
            "vert_interval",
            0.0,
        );
        let group_row = read("tab_controls:key_binding:scroll_view:item_group")?;
        let key_row = read("tab_controls:key_binding:scroll_view:item_key")?;
        let list_rect = at(&list, origin);
        let mut heights = Vec::new();
        for g in &opt.keys {
            heights.push(group_row.desc.window.height);
            heights.extend(g.commands.iter().map(|_| key_row.desc.window.height));
        }
        let content: f32 = heights.iter().map(|h| h + interval).sum();
        let v = b.view(
            Page::Controls,
            list_rect,
            content,
            list.always_show_scroll,
            opt,
        );
        let mut y = list_rect.y;
        for g in &opt.keys {
            y += interval;
            let mut row = group_row.clone();
            if let Some(t) = &mut row.desc.text {
                t.key = Some(g.name.clone());
            }
            let rect = Rect::new(
                list_rect.x,
                y,
                row.desc.window.width,
                row.desc.window.height,
            );
            b.push_in(&g.name, Page::Controls, v, rect, row, Kind::Static);
            y += group_row.desc.window.height;
            for c in &g.commands {
                y += interval;
                let mut row = key_row.clone();
                if let Some(t) = &mut row.desc.text {
                    t.key = Some(c.id.clone());
                }
                let rect = Rect::new(
                    list_rect.x,
                    y,
                    row.desc.window.width,
                    row.desc.window.height,
                );
                b.push_in(&c.id, Page::Controls, v, rect, row, Kind::Static);
                let action = console::action_index(&c.exe).ok_or_else(|| {
                    invalid(format!("ui_keybinding.xml: unknown action {}", c.exe))
                })?;
                // The cells at the headers' x, 3 units narrower (11.3).
                for (slot, hx) in [(0, 229.0), (1, 330.0)] {
                    let cell = Rect::new(list_rect.x + hx, y, 99.0, key_row.desc.window.height);
                    b.cells.push(Cell {
                        action,
                        slot,
                        ..Cell::default()
                    });
                    let i = b.cells.len() - 1;
                    b.push_in(
                        &format!("{}:{slot}", c.exe),
                        Page::Controls,
                        v,
                        cell,
                        key_row.clone(),
                        Kind::Cell(i),
                    );
                }
                y += key_row.desc.window.height;
            }
        }
        b.end_view(v);
        b.button(
            read("tab_controls:btn_default")?,
            Page::Controls,
            Act::Defaults,
        );

        // Apply, Cancel, then the tabs.
        let accept = read("main_dialog:btn_accept")?;
        b.push(
            "btn_accept",
            None,
            at(&accept, DIALOG),
            accept,
            Kind::Button(Act::Accept),
        );
        let cancel = read("main_dialog:btn_cancel")?;
        b.push(
            "btn_cancel",
            None,
            at(&cancel, DIALOG),
            cancel,
            Kind::Button(Act::Cancel),
        );
        let tab = read("main_dialog:tab")?;
        let tab_origin = tab.desc.window.absolute(DIALOG.0, DIALOG.1);
        let tab_el = require(doc, "main_dialog:tab", 0)?;
        for (i, e) in tab_el.children_named("button").enumerate() {
            let d = ControlDesc::read(e, colors)?;
            let rect = d.desc.window.absolute(tab_origin.x, tab_origin.y);
            b.push(&format!("tab {i}"), None, rect, d, Kind::Tab(i));
        }
        if b.items
            .iter()
            .filter(|i| matches!(i.kind, Kind::Tab(_)))
            .count()
            != 4
        {
            return Err(invalid(
                "ui_mm_opt.xml: the tab control has no four buttons".into(),
            ));
        }

        Ok(Self {
            items: b.items,
            cells: b.cells,
            views: b.views,
            message_box: opt.message.clone(),
            scroll: opt.scroll.clone(),
            back_video: stat("back_video")?,
            background: stat("background")?,
            rug_video: stat("rug_video")?,
            dialog: stat("main_dialog:dialog")?,
            page: Page::Video,
            cursor: [512.0, 384.0],
            hovered: None,
            waiting: None,
            armed: None,
            sliding: None,
            grab: None,
            list_press: None,
            open_list: None,
            message: false,
            message_shown: false,
            video_restart: false,
            sound_restart: false,
            last_repeat: 0,
            left_was_held: false,
            opened: 0,
            last_update: None,
            closed: None,
            clicks: Vec::new(),
            drawn_hover: std::cell::Cell::new(None),
            no_full_screen: false,
        })
    }

    /// Opens the screen at `now` (section 2.1): every control refreshed
    /// from its variable and the backups taken; the tab and page shown when
    /// it last closed stay. The renderer list stays enabled, without the
    /// hardware test (our choice; its criteria are not established, section
    /// 17); its dynamic entries are disabled instead ([`DYNAMIC_RENDERERS`]).
    pub fn open(
        &mut self,
        data: &GameData,
        console: &mut Console,
        native: Resolution,
        now: u64,
        cursor: [f32; 2],
    ) {
        self.opened = now;
        self.closed = None;
        self.cursor = cursor;
        self.hovered = self.item_at(cursor);
        self.waiting = None;
        self.armed = None;
        self.sliding = None;
        self.grab = None;
        self.list_press = None;
        self.open_list = None;
        for it in &mut self.items {
            if let Kind::List(l) = &mut it.kind {
                l.open = false;
            }
        }
        self.last_update = Some(now);
        for g in REFRESH {
            self.refresh_group(data, console, native, g);
        }
        self.refresh_cells(console);
        for i in 0..self.items.len() {
            let backed = matches!(&self.items[i].var, Some((_, g)) if BACKED_UP.contains(g));
            if backed {
                self.back_up(i);
            }
        }
        self.dependent();
    }

    fn refresh_group(
        &mut self,
        data: &GameData,
        console: &mut Console,
        native: Resolution,
        group: Group,
    ) {
        for i in 0..self.items.len() {
            if matches!(&self.items[i].var, Some((_, g)) if *g == group) {
                self.refresh(data, console, native, i);
            }
        }
    }

    /// Refreshes item `i` from its variable (sections 4.4–4.6); a track
    /// bar then sets its variable from its value.
    fn refresh(&mut self, data: &GameData, console: &mut Console, native: Resolution, i: usize) {
        let Some((var, _)) = self.items[i].var.clone() else {
            return;
        };
        match &mut self.items[i].kind {
            Kind::Check { shown, .. } => *shown = console.on(&var),
            Kind::Track { model, value, .. } => {
                *value = console.number(&var).clamp(model.min, model.max);
                self.save(console, i);
            }
            Kind::List(l) => {
                let names: Vec<String> = match Console::def(&var).map(|d| d.kind) {
                    Some(VarKind::List(names)) => {
                        names.iter().map(|&(n, _)| n.to_owned()).collect()
                    }
                    Some(VarKind::Mode) => widgets::resolution_list(native),
                    _ => Vec::new(),
                };
                let entries = names
                    .into_iter()
                    .map(|n| {
                        let t = data.strings.get(&n).to_owned();
                        (n, t)
                    })
                    .collect();
                let status = console.status(&var).unwrap_or_default();
                l.dd.refresh(entries, data.strings.get(&status).to_owned());
                let content = l.dd.entries.len() as f32 * ROW_HEIGHT;
                let height = self.items[i].desc.list_length as f32 * ROW_HEIGHT;
                if let Kind::List(l) = &mut self.items[i].kind {
                    let pos = l.bar.pos;
                    l.bar = ScrollBar::new(content, height, self.scroll.width);
                    l.bar.set(pos);
                }
            }
            _ => {}
        }
    }

    fn refresh_cells(&mut self, console: &Console) {
        for c in &mut self.cells {
            c.key = console.keys(c.action)[c.slot].and_then(console::key_index);
            c.cleared = false;
        }
    }

    fn back_up(&mut self, i: usize) {
        match &mut self.items[i].kind {
            Kind::Check { shown, backup } => *backup = u8::from(*shown),
            Kind::Track { value, backup, .. } => *backup = *value,
            Kind::List(l) => {
                l.backup = l.dd.stored;
                l.backup_status =
                    l.dd.entries
                        .get(l.dd.stored)
                        .map_or_else(|| l.dd.line.clone(), |(n, _)| n.clone());
            }
            _ => {}
        }
    }

    fn changed(&self, i: usize) -> bool {
        match &self.items[i].kind {
            Kind::Check { shown, backup } => u8::from(*shown) != *backup,
            Kind::Track { value, backup, .. } => (value - backup).abs() >= CHANGED_EPSILON,
            Kind::List(l) => l.dd.stored != l.backup,
            _ => false,
        }
    }

    /// Saves item `i`: sets its variable from the shown value (section
    /// 5.3) and marks the restarts its variable needs (section 5.4).
    fn save(&mut self, console: &mut Console, i: usize) {
        let Some((var, _)) = &self.items[i].var else {
            return;
        };
        let value = match &self.items[i].kind {
            Kind::Check { shown, .. } => Some(if *shown { "on" } else { "off" }.to_owned()),
            Kind::Track { model, value, .. } => Some(model.format(*value)),
            Kind::List(l) => l.dd.entries.get(l.dd.stored).map(|(n, _)| n.clone()),
            _ => None,
        };
        let Some(value) = value else { return };
        console.exec(&format!("{var} {value}"));
        if VIDEO_RESTART.contains(&var.as_str()) {
            self.video_restart = true;
        }
        if var == SOUND_RESTART {
            self.sound_restart = true;
        }
    }

    /// Enables the controls that depend on the renderer (section 6.3).
    fn dependent(&mut self) {
        let renderer = self
            .items
            .iter()
            .find_map(|it| match (&it.var, &it.kind) {
                (Some((v, _)), Kind::List(l)) if v == "renderer" => Some(l.dd.selected),
                _ => None,
            })
            .flatten();
        for it in &mut self.items {
            match it.name.as_str() {
                "check_fullscreen" => it.enabled = !self.no_full_screen,
                "check_r2_sun" | "check_r2_sun_details" => it.enabled = renderer == Some(2),
                "track_light_distance" | "track_particles_distance" => {
                    it.enabled = matches!(renderer, Some(1 | 2))
                }
                _ => {}
            }
        }
    }

    /// Whether the page can take the browser's full screen: without it
    /// the full-screen check box is disabled, its value kept.
    pub fn set_full_screen_supported(&mut self, supported: bool) {
        self.no_full_screen = !supported;
        self.dependent();
    }

    /// Whether the screen closed in the latest update, and what it asks.
    pub fn take_closed(&mut self) -> Option<Closed> {
        self.closed.take()
    }

    /// One frame at `now` with this frame's input.
    pub fn update(
        &mut self,
        data: &GameData,
        console: &mut Console,
        native: Resolution,
        now: u64,
        input: &Input,
    ) {
        let dt = now.saturating_sub(self.last_update.unwrap_or(now)).min(100) as f32 / 1000.0;
        self.last_update = Some(now);
        // The menu takes the console key and F12 first (section 13).
        let console_keys: Vec<Key> = console
            .keys(console::action_index("console").expect("console action"))
            .into_iter()
            .flatten()
            .filter_map(Key::from_dik_name)
            .chain([Key::F12])
            .collect();
        let mut events = Vec::new();
        for a in input.actions() {
            match *a {
                Action::Cursor { to } => {
                    events.push(Ev::Move(crate::core::input::ui_from_normalized(to)))
                }
                // Only these three are the interface's mouse events
                // (conventions).
                Action::Press {
                    control: Control::Mouse(b),
                    ..
                } if b.is_interface() => events.push(Ev::Press(b)),
                Action::Release {
                    control: Control::Mouse(b),
                } if b.is_interface() => events.push(Ev::Release(b)),
                Action::Wheel { up } => events.push(Ev::Wheel(up)),
                _ => {}
            }
        }
        for &(c, pressed) in input.device_changes() {
            if let (Control::Key(k), true) = (c, pressed)
                && !console_keys.contains(&k)
            {
                events.push(Ev::Key(k));
            }
        }
        for &(b, pressed) in input.pad_changes() {
            match (b, pressed) {
                (GamepadButton::South, true) => events.push(Ev::Press(MouseButton::Left)),
                (GamepadButton::South, false) => events.push(Ev::Release(MouseButton::Left)),
                (GamepadButton::East, true) => events.push(Ev::PadEscape),
                (GamepadButton::DpadUp, true) => events.push(Ev::Navigate(0.0, -1.0)),
                (GamepadButton::DpadDown, true) => events.push(Ev::Navigate(0.0, 1.0)),
                (GamepadButton::DpadLeft, true) => events.push(Ev::Navigate(-1.0, 0.0)),
                (GamepadButton::DpadRight, true) => events.push(Ev::Navigate(1.0, 0.0)),
                _ => {}
            }
        }
        let stick = [
            input.axis(GamepadAxis::LeftX),
            input.axis(GamepadAxis::LeftY),
        ];
        if stick[0].hypot(stick[1]) >= STICK_DEAD_ZONE {
            let [x, y] = self.cursor;
            events.push(Ev::Move([
                (x + stick[0] * STICK_SPEED * dt).clamp(0.0, 1023.0),
                (y + stick[1] * STICK_SPEED * dt).clamp(0.0, 767.0),
            ]));
        }
        for e in events {
            if self.closed.is_some() {
                break;
            }
            self.event(data, console, native, now, e);
        }
        // Section 4.8: the held left button repeats a held arrow.
        let held = input.is_held(Control::Mouse(MouseButton::Left))
            || input.pad_held(GamepadButton::South);
        if held && self.left_was_held && self.closed.is_none() {
            self.held(now);
        }
        self.left_was_held = held;
    }

    fn event(
        &mut self,
        data: &GameData,
        console: &mut Console,
        native: Resolution,
        now: u64,
        e: Ev,
    ) {
        match e {
            Ev::Move(p) => self.moved(console, p),
            Ev::Navigate(dx, dy) => {
                if let Some(p) = self.navigate(dx, dy) {
                    self.moved(console, p);
                }
            }
            Ev::Press(b) => self.pressed(console, now, b),
            Ev::Release(MouseButton::Left) => self.released(data, console, native),
            Ev::Release(_) => {}
            Ev::Wheel(up) => self.wheel(up),
            Ev::Key(k) => self.key(k),
            Ev::PadEscape => {
                // A gamepad's east button ends a cell's waiting without
                // binding anything (our design: Escape from a keyboard is
                // captured, section 11.4); otherwise it is Escape.
                if self.waiting.is_some() && !self.message {
                    self.waiting = None;
                } else {
                    self.key(Key::Escape);
                }
            }
        }
    }

    fn moved(&mut self, console: &mut Console, p: [f32; 2]) {
        let dy = p[1] - self.cursor[1];
        self.cursor = p;
        if self.message {
            return;
        }
        match self.grab {
            Some(Grab::Box(bar, offset)) => {
                let top = self.bar_rect(bar).map_or(0.0, |r| r.y);
                if let Some(b) = self.bar_mut(bar) {
                    b.drag_to(p[1] - top - offset);
                }
                return;
            }
            Some(Grab::Arrow(..)) => return,
            None => {}
        }
        if let Some(l) = self.open_list {
            // A drag in the open list scrolls it, as a drag in a scroll
            // view does (our design: a touch reaches every entry, not only
            // through the arrows).
            if let Some((_, start, dragged)) = &mut self.list_press {
                if (p[1] - start[1]).abs() > DRAG_SLOP || (p[0] - start[0]).abs() > DRAG_SLOP {
                    *dragged = true;
                }
                if *dragged && let Kind::List(s) = &mut self.items[l].kind {
                    s.bar.set(s.bar.pos - dy as i32);
                }
            }
            return;
        }
        let at = self.item_at(p);
        if at != self.hovered {
            // The item left notices (section 4.1): a waiting cell stops.
            if let Some((w, _)) = self.waiting
                && Some(w) == self.hovered
            {
                self.waiting = None;
            }
            self.hovered = at;
        }
        if self.left_was_held || self.sliding.is_some() {
            // Track bars take every move with the button held over them;
            // otherwise a drag over a scroll view scrolls it.
            match at {
                Some(i) if matches!(self.items[i].kind, Kind::Track { .. }) => {
                    self.slide(console, i);
                }
                _ if self.sliding.is_none() => {
                    if let Some(v) = self.view_at(p) {
                        let bar = &mut self.views[v].bar;
                        bar.set(bar.pos - dy as i32);
                    }
                }
                _ => {}
            }
        }
    }

    fn slide(&mut self, console: &mut Console, i: usize) {
        let x = self.cursor[0] - self.shown_rect(i).x;
        if let Kind::Track { model, value, .. } = &mut self.items[i].kind {
            *value = model.value_at(x);
        }
        self.save(console, i);
        self.sliding = Some(i);
    }

    fn pressed(&mut self, console: &mut Console, now: u64, b: MouseButton) {
        let p = self.cursor;
        if self.message {
            if b == MouseButton::Left && contains(self.message_ok(), p) {
                self.armed = Some(usize::MAX);
            }
            return;
        }
        if let Some(l) = self.open_list {
            if b != MouseButton::Left {
                return;
            }
            self.press_in_list(l, p);
            return;
        }
        let at = self.item_at(p);
        // A waiting cell takes a mouse button pressed over it (11.4).
        if let Some((w, _)) = self.waiting {
            if at == Some(w) {
                self.assign(w, console::key_index(b.key_name()).expect("mouse key"));
                return;
            }
            self.waiting = None;
        }
        if b != MouseButton::Left {
            return;
        }
        if let Some(bar) = self.bar_at(p) {
            self.bar_press(bar, p);
            return;
        }
        let Some(i) = at else { return };
        match self.items[i].kind.clone() {
            Kind::Button(_) => self.armed = Some(i),
            Kind::Tab(t) => {
                if t != self.page.tab() {
                    self.page = Page::of_tab(t);
                }
            }
            Kind::Check { .. } => {
                if let Kind::Check { shown, .. } = &mut self.items[i].kind {
                    *shown = !*shown;
                }
            }
            Kind::Track { .. } => self.slide(console, i),
            Kind::List(_) => {
                self.open_list = Some(i);
                if let Kind::List(l) = &mut self.items[i].kind {
                    l.open = true;
                }
            }
            Kind::Cell(_) => self.waiting = Some((i, now)),
            _ => {}
        }
    }

    /// A left press while list `l` is open: on an entry it is chosen, on
    /// the list's scroll bar the bar takes it, elsewhere the list closes.
    fn press_in_list(&mut self, l: usize, p: [f32; 2]) {
        if let Some(bar) = self.bar_at(p) {
            self.bar_press(bar, p);
            return;
        }
        let entry = self.list_entry_at(l, p);
        if entry.is_some() || self.list_rect(l).is_some_and(|r| contains(r, p)) {
            // In the rows: chosen on release unless dragged (our design,
            // so that a drag can scroll; the spec says "a click").
            self.list_press = Some((entry, p, false));
            return;
        }
        self.close_list(l);
    }

    fn close_list(&mut self, l: usize) {
        if let Kind::List(s) = &mut self.items[l].kind {
            s.open = false;
        }
        self.open_list = None;
        self.list_press = None;
    }

    /// The left button released over the open list's rows.
    fn list_release(
        &mut self,
        data: &GameData,
        console: &mut Console,
        native: Resolution,
        l: usize,
    ) {
        let Some((entry, _, dragged)) = self.list_press.take() else {
            return;
        };
        if dragged {
            return;
        }
        let Some(k) = entry.filter(|&k| self.list_entry_at(l, self.cursor) == Some(k)) else {
            return;
        };
        self.close_list(l);
        let chosen = match &mut self.items[l].kind {
            Kind::List(s) => s.dd.choose(k),
            _ => None,
        };
        if chosen.is_some() {
            self.save(console, l);
            self.chosen(data, console, native, l);
        }
    }

    /// The screen is told a list's value changed (sections 6.1, 10).
    fn chosen(&mut self, data: &GameData, console: &mut Console, native: Resolution, l: usize) {
        match self.items[l].var.as_ref().map(|(v, _)| v.as_str()) {
            Some("_preset") => {
                // Section 8: the preset's script ran; the video controls
                // show its values.
                for g in [Group::Video, Group::VideoAdv] {
                    self.refresh_group(data, console, native, g);
                }
            }
            Some("renderer") => {
                self.dependent();
                if !self.message_shown {
                    self.message_shown = true;
                    self.message = true;
                }
            }
            _ => {}
        }
    }

    fn released(&mut self, data: &GameData, console: &mut Console, native: Resolution) {
        let p = self.cursor;
        self.sliding = None;
        if let Some(l) = self.open_list {
            self.grab = None;
            self.list_release(data, console, native, l);
            return;
        }
        if let Some(Grab::Arrow(bar, up)) = self.grab.take()
            && self.arrow_rect(bar, up).is_some_and(|r| contains(r, p))
        {
            // The click steps again (section 4.8).
            if let Some(b) = self.bar_mut(bar) {
                b.scroll(if up { -1 } else { 1 });
            }
        }
        let Some(armed) = self.armed.take() else {
            return;
        };
        if self.message {
            if armed == usize::MAX && contains(self.message_ok(), p) {
                self.message = false;
            }
            return;
        }
        if self.item_at(p) != Some(armed) {
            return;
        }
        if let Kind::Button(act) = self.items[armed].kind {
            self.click(data, console, native, act);
        }
    }

    fn click(&mut self, data: &GameData, console: &mut Console, native: Resolution, act: Act) {
        match act {
            Act::Accept => self.apply(console),
            Act::Cancel => self.cancel(data, console, native),
            Act::Advanced => self.page = Page::Advanced,
            Act::Simply => self.page = Page::Video,
            // An online service Volya Engine does not have (section 3.4).
            Act::CheckUpdates => self.clicks.push("btn_check_updates".into()),
            Act::Defaults => {
                // Section 11.6.
                console.exec("unbindall");
                for i in 0..self.items.len() {
                    if matches!(&self.items[i].var, Some((_, Group::Controls))) {
                        self.refresh(data, console, native, i);
                    }
                }
                self.refresh_cells(console);
            }
        }
    }

    /// Apply (section 2.2).
    fn apply(&mut self, console: &mut Console) {
        let order = [
            Some(Group::Preset),
            None,
            Some(Group::Video),
            Some(Group::VideoAdv),
            Some(Group::Gameplay),
            Some(Group::Sound),
            Some(Group::Controls),
        ];
        for g in order {
            match g {
                None => {
                    for line in widgets::save_cells(&mut self.cells) {
                        console.exec(&line);
                    }
                }
                Some(g) => {
                    for i in 0..self.items.len() {
                        if matches!(&self.items[i].var, Some((_, ig)) if *ig == g)
                            && self.changed(i)
                        {
                            self.save(console, i);
                        }
                    }
                }
            }
        }
        self.close_with_restart();
    }

    /// Cancel (sections 2.2, 5.5).
    fn cancel(&mut self, data: &GameData, console: &mut Console, native: Resolution) {
        for g in UNDONE {
            for i in 0..self.items.len() {
                if !matches!(&self.items[i].var, Some((_, ig)) if *ig == g) || !self.changed(i) {
                    continue;
                }
                let mut restored = None;
                match &mut self.items[i].kind {
                    Kind::Check { shown, backup } => *shown = *backup == 1,
                    Kind::Track { value, backup, .. } => *value = *backup,
                    Kind::List(l) => {
                        let k = l.backup;
                        if l.dd.show(k).is_none() {
                            // The backed-up value 1 of a variable no entry
                            // matched (4.6) names no entry when the list has
                            // fewer than two: the variable's backed-up value
                            // is set instead (our choice, so Cancel undoes).
                            restored = Some(l.backup_status.clone());
                        }
                    }
                    _ => {}
                }
                match restored {
                    Some(value) => {
                        let var = self.items[i]
                            .var
                            .as_ref()
                            .map(|(v, _)| v.clone())
                            .unwrap_or_default();
                        console.exec(&format!("{var} {value}"));
                        if VIDEO_RESTART.contains(&var.as_str()) {
                            self.video_restart = true;
                        }
                    }
                    None => self.save(console, i),
                }
                if matches!(self.items[i].kind, Kind::Track { .. } | Kind::List(_)) {
                    self.refresh(data, console, native, i);
                }
            }
        }
        self.close_with_restart();
    }

    /// The restart step (section 5.4), then the screen closes.
    fn close_with_restart(&mut self) {
        self.closed = Some(Closed {
            video_restart: self.video_restart,
            sound_restart: self.sound_restart,
        });
        self.video_restart = false;
        self.sound_restart = false;
    }

    fn wheel(&mut self, up: bool) {
        if self.message {
            return;
        }
        let steps = if up { -1 } else { 1 };
        if let Some(l) = self.open_list {
            if let Kind::List(s) = &mut self.items[l].kind {
                s.bar.scroll(steps);
            }
            return;
        }
        if let Some(v) = self.view_at(self.cursor) {
            self.views[v].bar.scroll(steps);
        }
    }

    fn key(&mut self, k: Key) {
        if self.message {
            return;
        }
        if let Some((w, _)) = self.waiting {
            // Every key of the list is captured, Escape too (11.4).
            if let Some(key) = console::key_index(k.dik_name()) {
                self.assign(w, key);
            }
            return;
        }
        if k == Key::Escape {
            if self.page == Page::Advanced {
                self.page = Page::Video;
            } else {
                // Nothing saved or undone; restart marks stay (2.2).
                self.closed = Some(Closed::default());
            }
        }
    }

    fn assign(&mut self, item: usize, key: usize) {
        if let Kind::Cell(c) = self.items[item].kind {
            widgets::assign(&mut self.cells, c, key);
        }
        self.waiting = None;
    }

    /// The held left button over an arrow repeats it every > 100 ms
    /// (section 4.8).
    fn held(&mut self, now: u64) {
        let Some(Grab::Arrow(bar, up)) = self.grab else {
            return;
        };
        if !self
            .arrow_rect(bar, up)
            .is_some_and(|r| contains(r, self.cursor))
        {
            return;
        }
        if now.saturating_sub(self.last_repeat) > widgets::REPEAT_MS {
            self.last_repeat = now;
            if let Some(b) = self.bar_mut(bar) {
                b.scroll(if up { -1 } else { 1 });
            }
        }
    }

    fn bar_press(&mut self, bar: Bar, p: [f32; 2]) {
        let Some(r) = self.bar_rect(bar) else { return };
        for up in [true, false] {
            if self.arrow_rect(bar, up).is_some_and(|a| contains(a, p)) {
                if let Some(b) = self.bar_mut(bar) {
                    b.scroll(if up { -1 } else { 1 });
                }
                self.grab = Some(Grab::Arrow(bar, up));
                return;
            }
        }
        let Some(b) = self.bar_mut(bar) else { return };
        let (top, len) = (b.box_top(), b.box_length());
        let y = p[1] - r.y;
        if y >= top && y < top + len {
            self.grab = Some(Grab::Box(bar, y - top));
        }
    }

    fn bar_mut(&mut self, bar: Bar) -> Option<&mut ScrollBar> {
        match bar {
            Bar::View(v) => self.views.get_mut(v).map(|v| &mut v.bar),
            Bar::List(i) => match &mut self.items.get_mut(i)?.kind {
                Kind::List(l) => Some(&mut l.bar),
                _ => None,
            },
        }
    }

    fn bar_ref(&self, bar: Bar) -> Option<&ScrollBar> {
        match bar {
            Bar::View(v) => self.views.get(v).map(|v| &v.bar),
            Bar::List(i) => match &self.items.get(i)?.kind {
                Kind::List(l) => Some(&l.bar),
                _ => None,
            },
        }
    }

    /// A scroll bar's rectangle, absolute.
    fn bar_rect(&self, bar: Bar) -> Option<Rect> {
        let t = self.scroll.width;
        match bar {
            Bar::View(v) => {
                let view = self.views.get(v)?;
                let r = view.rect;
                view.shown
                    .then(|| Rect::new(r.x + r.width - t, r.y, t, r.height))
            }
            Bar::List(i) => {
                let r = self.list_rect(i)?;
                Some(Rect::new(r.x + r.width - t, r.y, t, r.height))
            }
        }
    }

    fn arrow_rect(&self, bar: Bar, up: bool) -> Option<Rect> {
        let r = self.bar_rect(bar)?;
        let a = widgets::ARROW;
        Some(if up {
            Rect::new(r.x, r.y, a, a)
        } else {
            Rect::new(r.x, r.y + r.height - a, a, a)
        })
    }

    /// The bar under `p` on the current page or of the open list.
    fn bar_at(&self, p: [f32; 2]) -> Option<Bar> {
        if let Some(l) = self.open_list {
            let bar = Bar::List(l);
            return self.bar_rect(bar).filter(|r| contains(*r, p)).map(|_| bar);
        }
        (0..self.views.len())
            .filter(|&v| self.views[v].page == self.page)
            .map(Bar::View)
            .find(|&b| self.bar_rect(b).is_some_and(|r| contains(r, p)))
    }

    /// An open list's rows area, absolute (section 4.6).
    fn list_rect(&self, i: usize) -> Option<Rect> {
        let it = self.items.get(i)?;
        let r = self.shown_rect(i);
        let Kind::List(_) = &it.kind else { return None };
        Some(Rect::new(
            r.x,
            r.y + LINE_HEIGHT,
            r.width,
            it.desc.list_length as f32 * ROW_HEIGHT,
        ))
    }

    fn list_entry_at(&self, i: usize, p: [f32; 2]) -> Option<usize> {
        let r = self.list_rect(i)?;
        let Kind::List(l) = &self.items[i].kind else {
            return None;
        };
        let row_width = r.width - self.scroll.width - 5.0;
        if p[0] < r.x || p[0] >= r.x + row_width || p[1] < r.y || p[1] >= r.y + r.height {
            return None;
        }
        let k = ((p[1] - r.y + l.bar.pos as f32) / ROW_HEIGHT) as usize;
        (k < l.dd.entries.len() && l.dd.enabled(k)).then_some(k)
    }

    /// Item `i`'s rectangle as shown: scrolled with its view.
    fn shown_rect(&self, i: usize) -> Rect {
        let it = &self.items[i];
        let mut r = it.rect;
        if let Some(v) = it.view {
            r.y -= self.views[v].bar.pos as f32;
        }
        match it.kind {
            Kind::Track { .. } => r.height = TRACK_HEIGHT,
            Kind::List(_) => r.height = LINE_HEIGHT,
            _ => {}
        }
        r
    }

    fn view_at(&self, p: [f32; 2]) -> Option<usize> {
        (0..self.views.len())
            .find(|&v| self.views[v].page == self.page && contains(self.views[v].rect, p))
    }

    /// Whether item `i` takes input now (sections 2.2, 4.1).
    fn live(&self, i: usize) -> bool {
        let it = &self.items[i];
        it.enabled
            && it.page.is_none_or(|p| p == self.page)
            && matches!(
                it.kind,
                Kind::Button(_)
                    | Kind::Tab(_)
                    | Kind::Check { .. }
                    | Kind::Track { .. }
                    | Kind::List(_)
                    | Kind::Cell(_)
            )
    }

    /// The topmost live item under `p`: later items first (section 4.1),
    /// those in a scroll view only inside it.
    fn item_at(&self, p: [f32; 2]) -> Option<usize> {
        (0..self.items.len()).rev().find(|&i| {
            self.live(i)
                && contains(self.shown_rect(i), p)
                && self.items[i]
                    .view
                    .is_none_or(|v| contains(self.views[v].rect, p))
        })
    }

    /// Where the d-pad moves the cursor (our design): to the centre of the
    /// nearest target in direction (`dx`, `dy`), scrolling a view to show
    /// it.
    fn navigate(&mut self, dx: f32, dy: f32) -> Option<[f32; 2]> {
        let targets = self.targets();
        let [cx, cy] = self.cursor;
        let best = targets
            .iter()
            .filter_map(|&(t, r)| {
                let (x, y) = (r.x + r.width / 2.0, r.y + r.height / 2.0);
                let along = (x - cx) * dx + (y - cy) * dy;
                let across = ((x - cx) * dy - (y - cy) * dx).abs();
                (along > 2.0).then_some((along + 3.0 * across, t, r))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))?;
        let (_, target, mut r) = best;
        if let Target::Item(i) = target
            && let Some(v) = self.items[i].view
        {
            let view = self.views[v].rect;
            let bar = &mut self.views[v].bar;
            if r.y < view.y {
                bar.set(bar.pos - (view.y - r.y).ceil() as i32);
            } else if r.y + r.height > view.y + view.height {
                bar.set(bar.pos + (r.y + r.height - view.y - view.height).ceil() as i32);
            }
            r = self.shown_rect(i);
        }
        if let Target::Entry(l, k) = target
            && let Some(lr) = self.list_rect(l)
            && let Kind::List(s) = &mut self.items[l].kind
        {
            if r.y < lr.y {
                s.bar.set(s.bar.pos - (lr.y - r.y).ceil() as i32);
            } else if r.y + r.height > lr.y + lr.height {
                s.bar
                    .set(s.bar.pos + (r.y + r.height - lr.y - lr.height).ceil() as i32);
            }
            r.y = lr.y + k as f32 * ROW_HEIGHT - s.bar.pos as f32;
        }
        Some([r.x + r.width / 2.0, r.y + r.height / 2.0])
    }

    /// What the d-pad can reach: the message box's button; an open list's
    /// entries; else the current page's live controls and the dialog's.
    fn targets(&self) -> Vec<(Target, Rect)> {
        if self.message {
            return vec![(Target::Ok, self.message_ok())];
        }
        if let Some(l) = self.open_list {
            let Some(r) = self.list_rect(l) else {
                return Vec::new();
            };
            let Kind::List(s) = &self.items[l].kind else {
                return Vec::new();
            };
            let w = r.width - self.scroll.width - 5.0;
            return (0..s.dd.entries.len())
                .filter(|&k| s.dd.enabled(k))
                .map(|k| {
                    let y = r.y + k as f32 * ROW_HEIGHT - s.bar.pos as f32;
                    (Target::Entry(l, k), Rect::new(r.x, y, w, ROW_HEIGHT))
                })
                .collect();
        }
        (0..self.items.len())
            .filter(|&i| self.live(i))
            .map(|i| {
                let mut r = self.shown_rect(i);
                if let Kind::Track { model, value, .. } = &self.items[i].kind {
                    // The thumb.
                    r = Rect::new(r.x + model.thumb_x(*value), r.y, THUMB, r.height);
                }
                (Target::Item(i), r)
            })
            .collect()
    }

    fn message_ok(&self) -> Rect {
        let f = self.message_box.frame.window.absolute(0.0, 0.0);
        self.message_box.ok.window.absolute(f.x, f.y)
    }

    /// The frame at `ctx.clock_ms` (section 3, drawn in order).
    pub fn scene(&self, ctx: &mut Ctx) -> io::Result<Scene> {
        let mut scene = Scene::new(ctx.resolution);
        let s = &mut scene;
        let top = |d: &StaticDesc| d.window.absolute(0.0, 0.0);
        ctx.picture(
            s,
            "back_video",
            &self.back_video,
            top(&self.back_video),
            0.0,
            self.opened,
            true,
            None,
        )?;
        ctx.picture(
            s,
            "background",
            &self.background,
            top(&self.background),
            0.0,
            0,
            false,
            None,
        )?;
        ctx.picture(
            s,
            "rug_video",
            &self.rug_video,
            top(&self.rug_video),
            0.0,
            self.opened,
            true,
            None,
        )?;
        ctx.picture(
            s,
            "dialog",
            &self.dialog,
            top(&self.dialog),
            0.0,
            0,
            false,
            None,
        )?;
        self.drawn_hover.set(Some(self.item_at(self.cursor)));
        let drawn = self.draw_layers(ctx, s);
        self.drawn_hover.set(None);
        drawn?;
        crate::draw::cursor(ctx, s, self.cursor)?;
        Ok(scene)
    }

    fn draw_layers(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let mut view_start = Vec::new();
        for i in 0..self.items.len() {
            let it = &self.items[i];
            if it.page.is_some_and(|p| p != self.page) {
                continue;
            }
            match &it.kind {
                Kind::ViewBegin => view_start.push(s.sprites.len()),
                Kind::ViewEnd(v) => {
                    let start = view_start.pop().unwrap_or(s.sprites.len());
                    let view = &self.views[*v];
                    s.clip_from(start, pixels(view.rect, ctx.scale()));
                    if view.shown {
                        self.draw_bar(ctx, s, &format!("{} bar", it.name), Bar::View(*v))?;
                    }
                }
                _ => {
                    // Rows a scroll view hides are not laid out.
                    if let Some(v) = it.view
                        && !overlaps(self.shown_rect(i), self.views[v].rect)
                    {
                        continue;
                    }
                    self.draw_item(ctx, s, i)?
                }
            }
        }
        if self.message {
            self.draw_message(ctx, s)?;
        }
        Ok(())
    }

    /// The item under the cursor.
    fn hover(&self) -> Option<usize> {
        self.drawn_hover
            .get()
            .unwrap_or_else(|| self.item_at(self.cursor))
    }

    fn state_suffix(&self, i: usize, pressed: bool) -> &'static str {
        let it = &self.items[i];
        let hovered = self.open_list.is_none() && !self.message && self.hover() == Some(i);
        if !it.enabled {
            "d"
        } else if pressed {
            "t"
        } else if hovered {
            "h"
        } else {
            "e"
        }
    }

    fn draw_item(&self, ctx: &mut Ctx, s: &mut Scene, i: usize) -> io::Result<()> {
        let it = &self.items[i];
        let r = self.shown_rect(i);
        let d = &it.desc.desc;
        let layer = it.name.as_str();
        let text = |ctx: &mut Ctx, s: &mut Scene, b: TextBox, font: UiFont| -> io::Result<()> {
            match d.text.as_ref().and_then(|t| t.key.as_deref()) {
                Some(key) => ctx.text(s, &format!("{layer} text"), &ctx.data.text(key), &b, font),
                None => Ok(()),
            }
        };
        match &it.kind {
            Kind::Static => {
                ctx.picture(s, layer, d, r, 0.0, 0, false, None)?;
                let (b, font) = TextBox::of_static(d, r);
                text(ctx, s, b, font)
            }
            Kind::Frame(base) => ctx.frame_window(s, layer, base, r),
            Kind::Label(base) => {
                ctx.frame_line(s, layer, base, (r.x, r.y), r.width)?;
                let (b, font) = TextBox::of_static(d, r);
                text(ctx, s, b, font)
            }
            Kind::Button(_) | Kind::Tab(_) => {
                let pressed = match it.kind {
                    Kind::Tab(t) => t == self.page.tab(),
                    _ => self.armed == Some(i) && self.hover() == Some(i),
                };
                let state = self.state_suffix(i, pressed);
                let state = if matches!(it.kind, Kind::Tab(_)) && state == "d" {
                    "e"
                } else {
                    state
                };
                if let Some(t) = &d.texture {
                    let at = (r.x + d.texture_offset.0, r.y + d.texture_offset.1);
                    ctx.part(
                        s,
                        layer,
                        &format!("{}_{state}", t.name),
                        at,
                        None,
                        Color::WHITE,
                    )?;
                }
                // Section 4.2: centred unless the data says otherwise.
                let (mut b, font) = TextBox::of_static(d, r);
                let t = d.text.as_ref();
                b.align = t.and_then(|t| t.align).unwrap_or(HAlign::Centre);
                b.vert_align = t.and_then(|t| t.vert_align).unwrap_or(VAlign::Centre);
                b.color = state_color(d, state, b.color);
                text(ctx, s, b, font)
            }
            Kind::Check { shown, .. } => {
                let state = if !it.enabled {
                    "d"
                } else if *shown {
                    "t"
                } else {
                    self.state_suffix(i, false)
                };
                ctx.part(
                    s,
                    layer,
                    &format!("ui_checker_{state}"),
                    (r.x, r.y),
                    None,
                    Color::WHITE,
                )?;
                let (mut b, font) =
                    TextBox::of_static(d, Rect::new(r.x + CHECK_BOX.0, r.y, r.width, CHECK_BOX.1));
                b.align = HAlign::Left;
                b.vert_align = VAlign::Centre;
                b.color = state_color(d, state, b.color);
                text(ctx, s, b, font)
            }
            Kind::Track { model, value, .. } => {
                let base = if it.enabled {
                    "ui_slider_e"
                } else {
                    "ui_slider_d"
                };
                ctx.frame_line(s, layer, base, (r.x, r.y + 2.0), r.width)?;
                let thumb = Rect::new(r.x + model.thumb_x(*value), r.y + 2.0, THUMB, 17.0);
                let state = if !it.enabled {
                    "d"
                } else if self.sliding == Some(i) {
                    "t"
                } else if contains(thumb, self.cursor) && self.open_list.is_none() {
                    "h"
                } else {
                    "e"
                };
                ctx.part(
                    s,
                    &format!("{layer} thumb"),
                    &format!("ui_slider_button_{state}"),
                    (thumb.x, thumb.y),
                    None,
                    Color::WHITE,
                )
            }
            Kind::List(l) => self.draw_list(ctx, s, i, l, r),
            Kind::Cell(c) => {
                ctx.frame_line(s, layer, "ui_options_string", (r.x, r.y), r.width)?;
                let cell = &self.cells[*c];
                // The key-list name (section 11.2's fallback; our use of it:
                // the browser reports no operating-system key names).
                let label = match cell.key {
                    Some(k) => KEYS[k].0,
                    None if cell.cleared => "---",
                    None => "",
                };
                let mut color = Color::WHITE;
                if let Some((w, since)) = self.waiting
                    && w == i
                {
                    let t = (ctx.clock_ms.saturating_sub(since)) as f32 / BLINK_MS;
                    color.a = (255.0 * (0.5 + 0.5 * (std::f32::consts::TAU * t).cos())) as u8;
                }
                let b = TextBox {
                    rect: r,
                    offset: (0.0, 0.0),
                    align: HAlign::Left,
                    vert_align: VAlign::Centre,
                    complex: false,
                    color,
                    line_gap: 0.0,
                };
                ctx.text(
                    s,
                    &format!("{layer} text"),
                    &crate::data::encode(label),
                    &b,
                    UiFont::DEFAULT,
                )
            }
            Kind::ViewBegin | Kind::ViewEnd(_) => Ok(()),
        }
    }

    fn draw_list(
        &self,
        ctx: &mut Ctx,
        s: &mut Scene,
        i: usize,
        l: &ListState,
        r: Rect,
    ) -> io::Result<()> {
        let it = &self.items[i];
        let layer = it.name.as_str();
        let hovered = self.open_list.is_none() && !self.message && self.hover() == Some(i);
        let base = if hovered {
            "ui_cb_linetext_h"
        } else {
            "ui_cb_linetext_e"
        };
        ctx.frame_line(s, layer, base, (r.x, r.y), r.width)?;
        let colors = it.desc.desc.text_colors.clone().unwrap_or_default();
        let line_color = if it.enabled {
            colors.enabled
        } else {
            colors.disabled.or(colors.enabled)
        }
        .unwrap_or(Color::rgba(0, 0, 0, 255));
        let font = it
            .desc
            .list_font
            .as_ref()
            .and_then(|f| f.font)
            .unwrap_or(UiFont::DEFAULT);
        let line = TextBox {
            rect: Rect::new(r.x, r.y, r.width, LINE_HEIGHT),
            offset: (0.0, 0.0),
            align: HAlign::Left,
            vert_align: VAlign::Centre,
            complex: false,
            color: line_color,
            line_gap: 0.0,
        };
        ctx.text(
            s,
            &format!("{layer} line"),
            &crate::data::encode(&l.dd.line),
            &line,
            UiFont::DEFAULT,
        )?;
        if !l.open {
            return Ok(());
        }
        let Some(lr) = self.list_rect(i) else {
            return Ok(());
        };
        ctx.frame_window(s, &format!("{layer} list"), "ui_cb_listbox", lr)?;
        let start = s.sprites.len();
        let row_w = lr.width - self.scroll.width - 5.0;
        let row_color = it.desc.list_font.as_ref().map_or(Color::WHITE, |f| f.color);
        let sel_color = it.desc.list_font_s.unwrap_or(row_color);
        for (k, (_, text)) in l.dd.entries.iter().enumerate() {
            let y = lr.y + k as f32 * ROW_HEIGHT - l.bar.pos as f32;
            if y + ROW_HEIGHT < lr.y || y > lr.y + lr.height {
                continue;
            }
            let selected = l.dd.selected == Some(k);
            if selected {
                ctx.frame_line(
                    s,
                    &format!("{layer} selected"),
                    "ui_cb_listline",
                    (lr.x, y),
                    row_w,
                )?;
            }
            let b = TextBox {
                rect: Rect::new(lr.x, y, row_w, ROW_HEIGHT),
                offset: (0.0, 0.0),
                align: HAlign::Left,
                vert_align: VAlign::Centre,
                complex: false,
                color: match (selected, l.dd.enabled(k)) {
                    (true, _) => sel_color,
                    (false, true) => row_color,
                    // Section 4.6: a disabled row's text alpha is halved.
                    (false, false) => Color {
                        a: 128,
                        ..row_color
                    },
                },
                line_gap: 0.0,
            };
            ctx.text(
                s,
                &format!("{layer} entry {k}"),
                &crate::data::encode(text),
                &b,
                font,
            )?;
        }
        s.clip_from(start, pixels(lr, ctx.scale()));
        self.draw_bar(ctx, s, &format!("{layer} bar"), Bar::List(i))
    }

    fn draw_bar(&self, ctx: &mut Ctx, s: &mut Scene, layer: &str, bar: Bar) -> io::Result<()> {
        let (Some(r), Some(b)) = (self.bar_rect(bar), self.bar_ref(bar)) else {
            return Ok(());
        };
        let p = &self.scroll;
        ctx.part_stretched(s, layer, &p.track, r)?;
        let box_rect = Rect::new(r.x, r.y + b.box_top(), r.width, b.box_length());
        ctx.part_stretched(s, layer, &p.thumb, box_rect)?;
        ctx.part(s, layer, &p.up, (r.x, r.y), None, Color::WHITE)?;
        ctx.part(
            s,
            layer,
            &p.down,
            (r.x, r.y + r.height - widgets::ARROW),
            None,
            Color::WHITE,
        )
    }

    fn draw_message(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let m = &self.message_box;
        let f = m.frame.window.absolute(0.0, 0.0);
        ctx.picture(s, "message", &m.frame, f, 0.0, 0, false, None)?;
        let pic = m.picture.window.absolute(f.x, f.y);
        ctx.picture(s, "message picture", &m.picture, pic, 0.0, 0, false, None)?;
        let tr = m.text.window.absolute(f.x, f.y);
        let (b, font) = TextBox::of_static(&m.text, tr);
        if let Some(key) = m.text.text.as_ref().and_then(|t| t.key.as_deref()) {
            ctx.text(s, "message text", &ctx.data.text(key), &b, font)?;
        }
        let ok = self.message_ok();
        let pressed = self.armed == Some(usize::MAX) && contains(ok, self.cursor);
        let state = if pressed {
            "t"
        } else if contains(ok, self.cursor) {
            "h"
        } else {
            "e"
        };
        if let Some(t) = &m.ok.texture {
            ctx.part(
                s,
                "message ok",
                &format!("{}_{state}", t.name),
                (ok.x, ok.y),
                None,
                Color::WHITE,
            )?;
        }
        let (mut b, font) = TextBox::of_static(&m.ok, ok);
        let t = m.ok.text.as_ref();
        b.align = t.and_then(|t| t.align).unwrap_or(HAlign::Centre);
        b.vert_align = t.and_then(|t| t.vert_align).unwrap_or(VAlign::Centre);
        if let Some(key) = t.and_then(|t| t.key.as_deref()) {
            ctx.text(s, "message ok text", &ctx.data.text(key), &b, font)?;
        }
        Ok(())
    }

    /// Every texture id and string key the screen draws, for checks.
    pub fn references(&self) -> (Vec<String>, Vec<String>) {
        let mut textures: Vec<String> = Vec::new();
        let mut strings: Vec<String> = Vec::new();
        fn parts(textures: &mut Vec<String>, base: &str, suffixes: &[&str]) {
            for s in suffixes {
                textures.push(format!("{base}_{s}"));
            }
        }
        const STATES: [&str; 4] = ["e", "d", "t", "h"];
        const FRAME: [&str; 9] = ["lt", "t", "rt", "l", "back", "r", "lb", "b", "rb"];
        const LINE: [&str; 3] = ["b", "back", "e"];
        parts(&mut textures, "ui_checker", &STATES);
        parts(&mut textures, "ui_slider_button", &STATES);
        parts(&mut textures, "ui_slider_e", &LINE);
        parts(&mut textures, "ui_slider_d", &LINE);
        parts(&mut textures, "ui_cb_linetext_e", &LINE);
        parts(&mut textures, "ui_cb_linetext_h", &LINE);
        parts(&mut textures, "ui_cb_listline", &LINE);
        parts(&mut textures, "ui_cb_listbox", &FRAME);
        parts(&mut textures, "ui_options_string", &LINE);
        for it in &self.items {
            let d = &it.desc.desc;
            if let Some(key) = d.text.as_ref().and_then(|t| t.key.clone()) {
                strings.push(key);
            }
            let Some(t) = d.texture.as_ref().filter(|t| !t.name.is_empty()) else {
                continue;
            };
            match it.kind {
                Kind::Frame(_) => parts(&mut textures, &t.name, &FRAME),
                Kind::Label(_) => parts(&mut textures, &t.name, &LINE),
                Kind::Button(_) | Kind::Tab(_) => parts(&mut textures, &t.name, &STATES),
                _ => textures.push(t.name.clone()),
            }
        }
        let m = &self.message_box;
        for d in [&m.frame, &m.picture] {
            textures.extend(d.texture.iter().map(|t| t.name.clone()));
        }
        if let Some(t) = &m.ok.texture {
            parts(&mut textures, &t.name, &STATES);
        }
        for d in [&m.text, &m.ok] {
            strings.extend(d.text.as_ref().and_then(|t| t.key.clone()));
        }
        let p = &self.scroll;
        textures.extend([
            p.up.clone(),
            p.down.clone(),
            p.track.clone(),
            p.thumb.clone(),
        ]);
        for d in [
            &self.dialog,
            &self.back_video,
            &self.background,
            &self.rug_video,
        ] {
            textures.extend(d.texture.iter().map(|t| t.name.clone()));
        }
        // The lists of the options screen's drop-downs.
        for names in [console::RENDERERS, console::PRESETS, console::DIFFICULTIES] {
            strings.extend(names.iter().map(|&(n, _)| n.to_owned()));
        }
        textures.sort();
        textures.dedup();
        strings.sort();
        strings.dedup();
        (textures, strings)
    }

    pub fn state(&self, data: &GameData) -> OptionsState {
        let controls = (0..self.items.len())
            .filter(|&i| self.items[i].var.is_some())
            .map(|i| {
                let it = &self.items[i];
                let r = self.shown_rect(i);
                let (kind, value) = match &it.kind {
                    Kind::Check { shown, .. } => {
                        ("check", if *shown { "on" } else { "off" }.to_owned())
                    }
                    Kind::Track { model, value, .. } => ("track", model.format(*value)),
                    Kind::List(l) => ("list", l.dd.line.clone()),
                    _ => ("other", String::new()),
                };
                ControlState {
                    name: it.name.clone(),
                    var: it.var.as_ref().map(|(v, _)| v.clone()).unwrap_or_default(),
                    kind,
                    page: it.page,
                    value,
                    entries: match &it.kind {
                        Kind::List(l) => l.dd.entries.iter().map(|(_, t)| t.clone()).collect(),
                        _ => Vec::new(),
                    },
                    enabled: it.enabled,
                    changed: self.changed(i),
                    rect: [r.x, r.y, r.width, r.height],
                }
            })
            .collect();
        let buttons = (0..self.items.len())
            .filter(|&i| matches!(self.items[i].kind, Kind::Button(_) | Kind::Tab(_)))
            .map(|i| {
                let r = self.shown_rect(i);
                ButtonState {
                    name: self.items[i].name.clone(),
                    page: self.items[i].page,
                    caption: self.items[i]
                        .desc
                        .desc
                        .text
                        .as_ref()
                        .and_then(|t| t.key.as_deref())
                        .map(|k| data.strings.get(k).to_owned())
                        .unwrap_or_default(),
                    rect: [r.x, r.y, r.width, r.height],
                }
            })
            .collect();
        let cells = (0..self.items.len())
            .filter_map(|i| match self.items[i].kind {
                Kind::Cell(c) => Some((i, c)),
                _ => None,
            })
            .map(|(i, c)| {
                let cell = &self.cells[c];
                let r = self.shown_rect(i);
                CellState {
                    action: ACTIONS[cell.action].0,
                    slot: cell.slot,
                    key: cell.key.map(|k| KEYS[k].0),
                    changed: cell.changed,
                    rect: [r.x, r.y, r.width, r.height],
                }
            })
            .collect();
        OptionsState {
            page: self.page,
            tab: self.page.tab(),
            controls,
            buttons,
            cells,
            waiting: self.waiting.and_then(|(i, _)| match self.items[i].kind {
                Kind::Cell(c) => Some(format!(
                    "{}:{}",
                    ACTIONS[self.cells[c].action].0, self.cells[c].slot
                )),
                _ => None,
            }),
            open_list: self.open_list.map(|i| self.items[i].name.clone()),
            message: self.message,
            video_restart: self.video_restart,
            sound_restart: self.sound_restart,
            scroll: self.views.iter().map(|v| v.bar.pos).collect(),
            cursor: self.cursor,
            clicks: self.clicks.clone(),
        }
    }
}

/// A d-pad target.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Item(usize),
    Entry(usize, usize),
    Ok,
}

/// One input event, in the screen's vocabulary.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ev {
    Move([f32; 2]),
    Press(MouseButton),
    Release(MouseButton),
    Wheel(bool),
    Key(Key),
    /// The d-pad (our design).
    Navigate(f32, f32),
    /// A gamepad's east button (our design).
    PadEscape,
}

struct Builder {
    items: Vec<Item>,
    views: Vec<View>,
    cells: Vec<Cell>,
}

impl Builder {
    fn push(&mut self, name: &str, page: Option<Page>, rect: Rect, desc: ControlDesc, kind: Kind) {
        self.items.push(Item {
            name: name.to_owned(),
            page,
            view: None,
            rect,
            var: None,
            enabled: true,
            desc,
            kind,
        });
    }

    fn push_in(
        &mut self,
        name: &str,
        page: Page,
        view: usize,
        rect: Rect,
        desc: ControlDesc,
        kind: Kind,
    ) {
        self.push(name, Some(page), rect, desc, kind);
        self.items.last_mut().expect("pushed").view = Some(view);
    }

    fn button(&mut self, d: ControlDesc, page: Page, act: Act) {
        let rect = d.desc.window.absolute(PAGE.0, PAGE.1);
        let name = d.desc.window.name.clone();
        self.push(&name, Some(page), rect, d, Kind::Button(act));
    }

    fn view(
        &mut self,
        page: Page,
        rect: Rect,
        content: f32,
        always: bool,
        opt: &OptionsData,
    ) -> usize {
        let t = opt.scroll.width;
        self.views.push(View {
            page,
            rect,
            bar: ScrollBar::new(content, rect.height, t),
            shown: always || content > rect.height,
        });
        let v = self.views.len() - 1;
        self.push(
            &format!("view {v}"),
            Some(page),
            rect,
            empty_control(),
            Kind::ViewBegin,
        );
        v
    }

    fn end_view(&mut self, v: usize) {
        let (page, rect) = (self.views[v].page, self.views[v].rect);
        self.push(
            &format!("view {v}"),
            Some(page),
            rect,
            empty_control(),
            Kind::ViewEnd(v),
        );
    }

    /// A caption, check box, track bar or drop-down from its element, at
    /// `origin` (the page's or a scroll-view row's top-left).
    fn control(
        &mut self,
        d: ControlDesc,
        page: Page,
        origin: (f32, f32),
        view: Option<usize>,
    ) -> io::Result<()> {
        let name = d.desc.window.name.clone();
        let rect = d.desc.window.absolute(origin.0, origin.1);
        let var = match &d.item {
            Some(item) => {
                let group = Group::parse(&item.group)
                    .ok_or_else(|| invalid(format!("{name}: unknown group {}", item.group)))?;
                let def = Console::def(&item.entry)
                    .ok_or_else(|| invalid(format!("{name}: unknown variable {}", item.entry)))?;
                Some((item.entry.clone(), group, def))
            }
            None => None,
        };
        let kind = match (&var, name.split('_').next()) {
            (None, _) => Kind::Static,
            (Some(_), Some("check")) => Kind::Check {
                shown: false,
                backup: if name == "check_mouseinvert" {
                    MOUSE_INVERT_BACKUP
                } else {
                    0
                },
            },
            (Some((_, _, def)), Some("track")) => {
                let (min, max) = match def.kind {
                    VarKind::Real { min, max } => (min, max),
                    VarKind::Int { min, max } => (min as f32, max as f32),
                    _ => return Err(invalid(format!("{name}: not a number variable"))),
                };
                Kind::Track {
                    model: Track {
                        min,
                        max,
                        step: d.step,
                        integer: d.is_integer,
                        invert: d.invert,
                        width: rect.width,
                    },
                    value: min,
                    // Set to 0 when the bar is created (section 5.2).
                    backup: 0.0,
                }
            }
            (Some((v, _, _)), Some("list")) => Kind::List(Box::new(ListState {
                dd: DropDown {
                    disabled: if v == "renderer" {
                        DYNAMIC_RENDERERS.to_vec()
                    } else {
                        Vec::new()
                    },
                    ..DropDown::default()
                },
                backup: 0,
                backup_status: String::new(),
                open: false,
                bar: ScrollBar::default(),
            })),
            _ => return Err(invalid(format!("{name}: unknown control type"))),
        };
        self.push(&name, Some(page), rect, d, kind);
        let it = self.items.last_mut().expect("pushed");
        it.view = view;
        it.var = var.map(|(v, g, _)| (v, g));
        Ok(())
    }
}

/// The control prefix of an advanced row (section 6.2).
fn control_prefix(row: &str) -> &'static str {
    match row {
        "r2_sun" | "r2_sun_details" | "npc_torch" | "vsync" | "60hz" => "check",
        _ => "track",
    }
}

fn empty_control() -> ControlDesc {
    ControlDesc {
        desc: crate::draw::picture_desc("", Rect::default(), None, Color::WHITE),
        item: None,
        list_length: 4,
        always_show_scroll: true,
        list_font: None,
        list_font_s: None,
        step: 0.1,
        is_integer: false,
        invert: false,
    }
}

/// A state's caption colour (section 4.1): its `text_color` entry, else
/// the `e` entry, else the text's own colour.
fn state_color(d: &StaticDesc, state: &str, fallback: Color) -> Color {
    let Some(c) = &d.text_colors else {
        return fallback;
    };
    let pick = match state {
        "d" => c.disabled,
        "t" => c.touched,
        "h" => c.highlighted,
        _ => None,
    };
    pick.or(c.enabled).unwrap_or(fallback)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

fn contains(r: Rect, [x, y]: [f32; 2]) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

/// A UI rectangle in output pixels (min x, min y, max x, max y), in the
/// scene's convention.
fn pixels(r: Rect, (sx, sy): (f32, f32)) -> [f32; 4] {
    [
        (r.x * sx).floor() - 0.5,
        (r.y * sy).floor() - 0.5,
        ((r.x + r.width) * sx).floor() - 0.5,
        ((r.y + r.height) * sy).floor() - 0.5,
    ]
}

#[derive(Clone, Debug, Serialize)]
pub struct ControlState {
    pub name: String,
    pub var: String,
    pub kind: &'static str,
    pub page: Option<Page>,
    /// As shown: `on`/`off`, the bar's value as the console writes it, the
    /// list's line.
    pub value: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    pub enabled: bool,
    pub changed: bool,
    /// Absolute, as shown (x, y, width, height).
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, Serialize)]
pub struct ButtonState {
    pub name: String,
    pub page: Option<Page>,
    pub caption: String,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, Serialize)]
pub struct CellState {
    pub action: &'static str,
    pub slot: usize,
    pub key: Option<&'static str>,
    pub changed: bool,
    pub rect: [f32; 4],
}

/// The screen as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct OptionsState {
    pub page: Page,
    pub tab: usize,
    pub controls: Vec<ControlState>,
    pub buttons: Vec<ButtonState>,
    pub cells: Vec<CellState>,
    /// The waiting cell as `action:slot`.
    pub waiting: Option<String>,
    pub open_list: Option<String>,
    /// The restart message shows.
    pub message: bool,
    pub video_restart: bool,
    pub sound_restart: bool,
    /// Scroll positions of the advanced page and the key table.
    pub scroll: Vec<i32>,
    pub cursor: [f32; 2],
    pub clicks: Vec<String>,
}
