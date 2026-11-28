// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The PDA's map window (docs/behavior/pda-map.md, cited by section): the
//! global picture with the level pictures on it (1.2, 2), shown and hidden
//! with its tab (3), its transitions (4), the tool bar, the wheel, the
//! drag, the arrow keys, the scroll bars and the keypad zoom (5), the
//! player's mark (6.7), the label and the hint window (7). The level-map
//! data is [`crate::level_maps`]'s.
//!
//! **What waits**: the other map locations, their marks, borders and
//! pointers (6.1–6.6) wait for the map locations (#178); the tasks tab's
//! map pane for that tab (pda-scope.md PD-7). The level pictures are drawn
//! in the list's order: the spec leaves the order unestablished (8).
//!
//! **Touch and gamepad** are our design (proposed on #743): touch through
//! the standard touch controls (#856: Click presses a tool, held it drags
//! the map, two fingers zoom: pinched apart or swiped up zoom in, as the
//! wheel); a gamepad is a mouse, as everywhere in the window (#835): its
//! left stick moves the cursor, south (A / cross) is the left button and
//! the shoulders are the wheel (right in, left out); here the right stick
//! moves the map ([`PAN_SPEED`]) instead of scrolling.

use std::io;

use serde::Serialize;

use crate::core::input::{GamepadAxis, Input, Key};
use crate::core::ui::load_ui_file;
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::dialog::{self, ButtonState, Ev, PushButton, Pushed};
use crate::draw::{Ctx, picture_desc};
use crate::level_maps::LevelMaps;
use crate::scene::Scene;
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::layout::{Rect, StaticDesc, require};
use volya_engine_formats::ui::options::ScrollBarProfile;
use volya_engine_formats::ui::{Color, ColorDefs, Resolution};
use volya_engine_formats::xml::Document;

/// The map tab's layout file and element (1.1).
pub const FILE: &str = "pda_map.xml";
pub const ELEMENT: &str = "map_wnd";
/// The map spots' file (1.3) and the hint window's (1.4).
pub const SPOTS_FILE: &str = "map_spots.xml";
pub const HINT_FILE: &str = "hint_item.xml";
/// The scroll-bar profile (5.6).
pub const SCROLL_PROFILE: &str = "pda";
/// The tool buttons, in their order (1.1).
pub const TOOLS: [&str; 4] = ["global_map_btn", "actor_btn", "zoom_in_btn", "zoom_out_btn"];
pub const GLOBAL_BUTTON: usize = 0;
pub const ACTOR_BUTTON: usize = 1;
pub const ZOOM_IN: usize = 2;
pub const ZOOM_OUT: usize = 3;
/// The player's two locations: their types and level-map spots (1.3, 6.7).
pub const PLAYER_SPOTS: [(&str, &str); 2] = [
    ("actor_location", "actor_level_spot"),
    ("actor_location_p", "actor_level_spot_p"),
];
/// The hint id of a type without `hint` (7.2).
pub const NO_HINT: &str = "no hint";
/// The label's text over the global picture (7.1).
pub const GLOBAL_NAME: &str = "global_map";

/// Zoom factors (5.4, 5.5, 5.7).
const ZOOM_UP: f32 = 1.5;
const ZOOM_DOWN: f32 = 2.0 / 3.0;
/// "At minimum zoom" (2.1) and a zoom change that counts (4.2, 5.4).
const AT_MIN: f32 = 0.001;
const ZOOM_EPS: f32 = 1e-5;
/// Transitions (4.2, 4.3): seconds, global units per second.
const APPROACH_S: f64 = 0.5;
const SPEED: f32 = 350.0;
const MIN_MOVE_S: f32 = 0.25;
const MIN_ZOOM_S: f32 = 0.5;
/// The hint window (7.2).
const HINT_DELAY_MS: u64 = 500;
const HINT_WIDTH: f32 = 210.0;
const HINT_MIN_HEIGHT: f32 = 64.0;
const HINT_BELOW: f32 = 45.0;
/// The scroll bars' distance from the frame (5.6).
const BAR_GAP: f32 = 5.0;
/// A held arrow repeats every more than this, milliseconds
/// (options-screens.md 4.8).
const REPEAT_MS: u64 = 100;
/// Our design: the right stick moves the map this many units a second at
/// full deflection, past this dead zone.
pub const PAN_SPEED: f32 = 600.0;
const PAN_DEAD_ZONE: f32 = 0.2;

/// A tool-bar button (1.1; pda-controls.md 4).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolDesc {
    pub desc: StaticDesc,
    pub rect: Rect,
    /// `check_mode="1"`: a check button.
    pub check: bool,
    /// The looks' texture ids, `e`, `t`, `h`, `d`: a `texture_<look>`
    /// child, else the `texture` base with `_<look>` (pda-controls.md 4).
    pub looks: [Option<String>; 4],
    /// The hint's string key.
    pub hint: Option<String>,
}

/// A spot of `map_spots.xml` (1.3, 6.3).
#[derive(Clone, Debug, PartialEq)]
pub struct SpotDesc {
    pub desc: StaticDesc,
    /// `scale="1"`: sized with the zoom (6.3).
    pub scale: bool,
    /// The location type's hint id (7.2).
    pub hint: String,
}

/// `hint_item` (1.4).
#[derive(Clone, Debug, PartialEq)]
pub struct MapHintDesc {
    /// The frame's texture base.
    pub frame: String,
    pub description: StaticDesc,
}

/// What the map window reads (1.1, 1.3, 1.4, 5.6).
#[derive(Clone, Debug, PartialEq)]
pub struct MapLayout {
    /// `main_wnd`: the map window, absolute.
    pub main: Rect,
    /// `main_map_frame`'s rectangle and texture base.
    pub frame_window: Rect,
    pub frame_base: Option<String>,
    /// `level_frame`: the frame, absolute.
    pub frame: Rect,
    /// `map_header_frame_line`, absolute.
    pub header: Rect,
    pub header_base: Option<String>,
    pub tools: Vec<ToolDesc>,
    /// `text_hint`: a screen rectangle (1.1, 7.1).
    pub label: StaticDesc,
    pub hint: MapHintDesc,
    pub spots: Vec<SpotDesc>,
    pub scroll: ScrollBarProfile,
    /// The arrows' sizes: their elements' (pda-controls.md 11).
    pub up_arrow: (f32, f32),
    pub down_arrow: (f32, f32),
}

impl MapLayout {
    /// Reads `root` of `map` (1.1), the spots of `spots` (1.3), `hint_item`
    /// of `hint` (1.4) and the `pda` profile of `scroll` (5.6).
    pub fn read(
        map: &Document,
        root: &str,
        spots: &Document,
        hint: &Document,
        scroll: &Document,
        colors: &ColorDefs,
    ) -> io::Result<Self> {
        let path = |p: &str| format!("{root}:main_wnd{p}");
        let read = |p: &str| StaticDesc::read(require(map, &path(p), 0)?, colors);
        let main = read("")?.window.absolute(0.0, 0.0);
        let frame_window = read(":main_map_frame")?;
        let fw = frame_window.window.absolute(main.x, main.y);
        let frame = read(":main_map_frame:level_frame")?
            .window
            .absolute(fw.x, fw.y);
        let header = read(":map_header_frame_line")?;
        let hr = header.window.absolute(main.x, main.y);
        let bar = read(":map_header_frame_line:tool_bar")?
            .window
            .absolute(hr.x, hr.y);
        let tools = TOOLS
            .iter()
            .map(|name| {
                let e = require(
                    map,
                    &path(&format!(":map_header_frame_line:tool_bar:{name}")),
                    0,
                )?;
                let desc = StaticDesc::read(e, colors)?;
                let looks = ["e", "t", "h", "d"].map(|l| {
                    (e.child(&format!("texture_{l}")).and_then(|t| t.value()))
                        .map(str::to_owned)
                        .or_else(|| desc.texture.as_ref().map(|t| format!("{}_{l}", t.name)))
                });
                Ok(ToolDesc {
                    looks,
                    rect: desc.window.absolute(bar.x, bar.y),
                    check: e.attr("check_mode") == Some("1"),
                    hint: e.attr("hint").map(str::to_owned),
                    desc,
                })
            })
            .collect::<io::Result<_>>()?;
        let spots = PLAYER_SPOTS
            .iter()
            .map(|(kind, spot)| {
                let e = require(spots, spot, 0)?;
                let hint = require(spots, kind, 0)?
                    .attr("hint")
                    .unwrap_or(NO_HINT)
                    .to_owned();
                Ok(SpotDesc {
                    desc: StaticDesc::read(e, colors)?,
                    scale: e.attr("scale") == Some("1"),
                    hint,
                })
            })
            .collect::<io::Result<_>>()?;
        let frame_desc = StaticDesc::read(require(hint, "hint_item:frame", 0)?, colors)?;
        let profile = ScrollBarProfile::read(scroll, SCROLL_PROFILE)?;
        let arrow = |name: &str| -> io::Result<(f32, f32)> {
            let e = require(scroll, &format!("{SCROLL_PROFILE}:{name}"), 0)?;
            let w = StaticDesc::read(e, colors)?.window;
            Ok((w.width, w.height))
        };
        Ok(Self {
            main,
            frame_window: fw,
            frame_base: frame_window.texture.map(|t| t.name),
            frame,
            header: hr,
            header_base: header.texture.map(|t| t.name),
            tools,
            label: read(":text_hint")?,
            hint: MapHintDesc {
                frame: frame_desc.texture.map(|t| t.name).unwrap_or_default(),
                description: StaticDesc::read(require(hint, "hint_item:description", 0)?, colors)?,
            },
            spots,
            up_arrow: arrow("up_arrow")?,
            down_arrow: arrow("down_arrow")?,
            scroll: profile,
        })
    }

    /// Every texture id the window draws and its string keys, for checks.
    pub fn references(&self) -> (Vec<String>, Vec<String>) {
        let mut textures = Vec::new();
        let mut strings = Vec::new();
        for base in self.frame_base.iter().chain(&self.hint.frame_base()) {
            textures.extend(
                ["lt", "t", "rt", "l", "back", "r", "lb", "b", "rb"].map(|p| format!("{base}_{p}")),
            );
        }
        if let Some(base) = &self.header_base {
            textures.extend(["b", "back", "e"].map(|p| format!("{base}_{p}")));
        }
        for t in &self.tools {
            // No tool is ever disabled: its `d` look is not drawn.
            textures.extend(t.looks[..3].iter().flatten().cloned());
            strings.extend(t.hint.clone());
        }
        let s = &self.scroll;
        textures.extend([&s.up, &s.down, &s.track, &s.thumb].map(|t| t.clone()));
        strings.extend(self.spots.iter().map(|s| s.hint.clone()));
        strings.push(GLOBAL_NAME.to_owned());
        (textures, strings)
    }
}

impl MapHintDesc {
    fn frame_base(&self) -> Option<String> {
        (!self.frame.is_empty()).then(|| self.frame.clone())
    }
}

/// The player as the map sees it (5.2, 6.7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapPlayer<'a> {
    /// The current level's name.
    pub level: &'a str,
    /// World x and z.
    pub position: [f32; 2],
    /// The camera's yaw (world-coordinates.md 2).
    pub yaw: f32,
}

/// A transition's phase (4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Approach,
    Settle,
}

/// A running phase: its target rectangle and end time (4.2).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Run {
    phase: Phase,
    target: [f32; 4],
    /// Seconds of the global clock.
    end: f64,
    /// The approach before its first step (4.2).
    first: bool,
}

/// A transition requested and not yet reached (4.2).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Request {
    /// The near test, stored once true (4.2).
    near: bool,
    run: Option<Run>,
}

/// What the hint window shows (7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HintSource {
    /// Level picture `i` (at minimum zoom).
    Level(usize),
    /// The player's mark `i` ([`PLAYER_SPOTS`]).
    Mark(usize),
}

/// The open hint window (7.2).
#[derive(Clone, Debug, PartialEq)]
struct MapHint {
    source: HintSource,
    text: String,
    rect: Rect,
}

/// A scroll bar (5.6): which, and what the left button holds of it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Arrow {
        vertical: bool,
        up: bool,
    },
    /// The box, grabbed this far from its start.
    Box {
        vertical: bool,
        offset: f32,
    },
}

/// One scroll bar's values (5.6, options-screens.md 4.8).
#[derive(Clone, Copy, Debug, PartialEq)]
struct BarValues {
    pos: i32,
    max: i32,
    page: i32,
    step: i32,
    /// The bar's length *H*.
    length: f32,
    /// Its thickness *t*.
    t: f32,
}

impl BarValues {
    fn limit(&self) -> i32 {
        (self.max - self.page + 1).max(0)
    }

    fn n(&self) -> i32 {
        (self.max - self.page + 1).max(1)
    }

    fn work(&self) -> i32 {
        (self.length - 2.0 * self.t) as i32
    }

    /// The box length (options-screens.md 4.8, with the profile's *t*).
    fn box_length(&self) -> f32 {
        let room = self.length - 2.0 * self.t;
        let l = if self.max > 0 {
            self.work() as f32 * self.page as f32 / self.max as f32
        } else {
            room
        };
        let low = self.t.min(room);
        l.clamp(low, room.max(low))
    }

    /// The box's start along the bar.
    fn box_start(&self) -> f32 {
        let l = self.box_length();
        let n = self.n();
        let t = self.t as i32;
        let top = (self.pos * (self.work() - l as i32) + n * t) / n;
        (top as f32).clamp(self.t, (self.length - self.t - l).max(self.t))
    }

    /// The position for the box's start at `y`.
    fn drag_to(&self, y: f32) -> i32 {
        let room = self.work() as f32 - self.box_length();
        let pos = if room > 0.0 {
            (self.n() as f32 * (y - self.t) / room) as i32
        } else {
            0
        };
        pos.clamp(0, self.limit())
    }
}

/// A text's height in units, laid out in the hint's description.
pub type TextHeight = Box<dyn Fn(&[u8]) -> f32>;

/// The map window (1.1).
pub struct MapWindow {
    layout: MapLayout,
    maps: LevelMaps,
    /// A text's height in units laid out in the hint's description (7.2).
    text_height: TextHeight,
    /// The zoom limits (2.1).
    min_zoom: f32,
    max_zoom: f32,
    /// The wanted zoom *Z* and target point *T* (2.1).
    zoom: f32,
    target: [f32; 2],
    /// The global picture's rectangle relative to the frame: left, top,
    /// right, bottom (2.1).
    picture: [f32; 4],
    request: Option<Request>,
    shown: bool,
    /// The first showing's actor-button action is due (3).
    first_pending: bool,
    shown_before: bool,
    /// The zoom tool on (5.4): [`ZOOM_IN`] or [`ZOOM_OUT`].
    tool: Option<usize>,
    /// The push buttons' states, the check buttons' cursor flags.
    buttons: [PushButton; 4],
    /// The label's text (7.1), the string key.
    label: Option<String>,
    hint: Option<MapHint>,
    /// What the cursor is over and since when, global clock ms (7.2).
    hover: Option<(HintSource, u64)>,
    grab: Option<Grab>,
    last_repeat: u64,
    left_was_held: bool,
    cursor: [f32; 2],
    /// The player as last seen, for the actor button and the marks.
    player: Option<(String, [f32; 2], f32)>,
    log: Vec<String>,
}

/// One frame of the map window.
pub struct MapFrame<'a> {
    pub data: &'a GameData,
    pub input: &'a Input,
    pub player: Option<MapPlayer<'a>>,
    /// The window's cursor after this frame's events.
    pub cursor: [f32; 2],
    /// The global clock, milliseconds.
    pub global_ms: u64,
    /// The frame's time step, seconds.
    pub dt: f32,
}

impl MapWindow {
    /// The map tab's window (1.1) with the level maps of `game.ltx`, the
    /// hint's text measured in `fonts` at `resolution`.
    pub fn load(
        tree: &dyn Vfs,
        data: &GameData,
        resolution: Resolution,
        fonts: &crate::core::text::Fonts,
    ) -> io::Result<Self> {
        let ui = |name| load_ui_file(tree, &data.aliases, name, resolution);
        let layout = MapLayout::read(
            &ui(FILE)?,
            ELEMENT,
            &ui(SPOTS_FILE)?,
            &ui(HINT_FILE)?,
            &ui("scroll_bar.xml")?,
            &data.colors,
        )?;
        let maps = LevelMaps::load(tree, data)?;
        let d = &layout.hint.description;
        let font = (d.text.as_ref())
            .and_then(|t| t.font)
            .unwrap_or(volya_engine_formats::text::font::UiFont::DEFAULT);
        let font = fonts.get(font).clone();
        let colors = data.colors.clone();
        let (sx, sy) = resolution.scale();
        let width = d.window.width;
        let text_height = move |text: &[u8]| {
            use volya_engine_formats::text::layout::{split_complex, wrap};
            let lines = split_complex(text, Color::WHITE, &colors)
                .map(|s| wrap(&font, &s, width, sx).len())
                .unwrap_or(1);
            lines as f32 * font.line_height() / sy
        };
        Ok(Self::new(layout, maps, Box::new(text_height)))
    }

    /// The window of `layout` and `maps`, hint texts `text_height` high.
    pub fn new(layout: MapLayout, maps: LevelMaps, text_height: TextHeight) -> Self {
        let f = layout.frame;
        let b = maps.global.picture.bound;
        let (bw, bh) = (b[2] - b[0], b[3] - b[1]);
        // 2.1: the set-up size covers the frame.
        let (w, h) = if bh / f.height < bw / f.width {
            (f.height * bw / bh, f.height)
        } else {
            (f.width, f.width * bh / bw)
        };
        let min_zoom = w / bw;
        Self {
            max_zoom: maps.global.max_zoom,
            layout,
            maps,
            text_height,
            min_zoom,
            zoom: min_zoom,
            target: [0.0, 0.0],
            picture: [0.0, 0.0, w, h],
            request: None,
            shown: false,
            first_pending: false,
            shown_before: false,
            tool: None,
            buttons: [PushButton::default(); 4],
            label: None,
            hint: None,
            hover: None,
            grab: None,
            last_repeat: 0,
            left_was_held: false,
            cursor: [0.0, 0.0],
            player: None,
            log: Vec::new(),
        }
    }

    pub fn layout(&self) -> &MapLayout {
        &self.layout
    }

    pub fn maps(&self) -> &LevelMaps {
        &self.maps
    }

    /// The log lines written since the last call (5.2).
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    /// Shows the window (3): the hint window closed; the first time, the
    /// actor button's action is due at the next update.
    pub fn show(&mut self) {
        self.shown = true;
        self.hint = None;
        self.hover = None;
        if !self.shown_before {
            self.shown_before = true;
            self.first_pending = true;
        }
    }

    /// Hides it (3): the hint window closed.
    pub fn hide(&mut self) {
        self.shown = false;
        self.hint = None;
        self.hover = None;
        self.grab = None;
    }

    /// The tool buttons' rectangles, for the gamepad cursor and the button
    /// hints.
    pub fn tool_rects(&self) -> Vec<Rect> {
        self.layout.tools.iter().map(|t| t.rect).collect()
    }

    /// The current scale *s* (2.1).
    fn scale(&self) -> f32 {
        (self.picture[2] - self.picture[0]) / self.bound_width()
    }

    fn bound_width(&self) -> f32 {
        let b = self.maps.global.picture.bound;
        b[2] - b[0]
    }

    fn bound_height(&self) -> f32 {
        let b = self.maps.global.picture.bound;
        b[3] - b[1]
    }

    fn at_min_zoom(&self) -> bool {
        (self.scale() - self.min_zoom).abs() < AT_MIN
    }

    /// Frame position of global point `g` (2.1).
    fn frame_pos(&self, g: [f32; 2]) -> [f32; 2] {
        let b = self.maps.global.picture.bound;
        let s = self.scale();
        [
            self.picture[0] + s * (g[0] - b[0]),
            self.picture[1] + s * (g[1] - b[1]),
        ]
    }

    /// The global point at screen point `p`.
    fn global_at(&self, p: [f32; 2]) -> [f32; 2] {
        let b = self.maps.global.picture.bound;
        let s = self.scale();
        let f = self.layout.frame;
        [
            (p[0] - f.x - self.picture[0]) / s + b[0],
            (p[1] - f.y - self.picture[1]) / s + b[1],
        ]
    }

    /// The clamp of 2.1 for a rectangle `w` × `h` at (`x`, `y`).
    fn clamped(&self, mut x: f32, mut y: f32, w: f32, h: f32) -> [f32; 4] {
        let f = self.layout.frame;
        if x + w < f.width {
            x = f.width - w;
        }
        if y + h < f.height {
            y = f.height - h;
        }
        x = x.min(0.0);
        y = y.min(0.0);
        [x, y, x + w, y + h]
    }

    fn clamp_picture(&mut self) {
        let [l, t, r, b] = self.picture;
        self.picture = self.clamped(l, t, r - l, b - t);
    }

    /// Moves the picture by (`dx`, `dy`) and clamps it (5.6).
    fn move_by(&mut self, dx: f32, dy: f32) {
        let p = &mut self.picture;
        p[0] += dx;
        p[2] += dx;
        p[1] += dy;
        p[3] += dy;
        self.clamp_picture();
    }

    /// The rectangle showing *T* at zoom *Z* (4.1).
    fn target_rect(&self, zoom: f32) -> [f32; 4] {
        let f = self.layout.frame;
        let b = self.maps.global.picture.bound;
        let (w, h) = (self.bound_width() * zoom, self.bound_height() * zoom);
        let x = f.width / 2.0 - (self.target[0] - b[0]) * zoom;
        let y = f.height / 2.0 - (self.target[1] - b[1]) * zoom;
        self.clamped(x, y, w, h)
    }

    /// The duration towards `target` at zoom `zoom` (4.3), seconds.
    fn duration(&self, target: [f32; 4], zoom: f32) -> f64 {
        let s = self.scale();
        let (px, py) = (self.picture[0], self.picture[1]);
        let d = ((px / s - target[0] / zoom).powi(2) + (py / s - target[1] / zoom).powi(2)).sqrt();
        let t = if (zoom - s).abs() < AT_MIN {
            if d < 0.001 {
                0.0
            } else {
                (d / SPEED).max(MIN_MOVE_S)
            }
        } else if d < 0.001 {
            MIN_ZOOM_S
        } else {
            (d / SPEED).max(MIN_ZOOM_S)
        };
        f64::from(t)
    }

    /// A phase started from the current rectangle towards (*Z*, *T*) with
    /// the duration rule (4.2).
    fn towards(&self, phase: Phase, now: f64) -> Run {
        let target = self.target_rect(self.zoom);
        Run {
            phase,
            target,
            end: now + self.duration(target, self.zoom),
            first: false,
        }
    }

    /// Asks for a transition after *Z* (was `old`) and *T* were set (4.2).
    fn request_transition(&mut self, old: f32, now: f64) {
        let changed = (self.zoom - old).abs() >= ZOOM_EPS;
        match self.request {
            Some(r) => {
                if changed && let Some(run) = r.run {
                    let run = self.towards(run.phase, now);
                    self.request = Some(Request {
                        run: Some(run),
                        ..r
                    });
                }
            }
            None => {
                self.request = Some(Request {
                    near: false,
                    run: None,
                });
            }
        }
    }

    /// Sets *Z* clamped (2.1), returning the old one.
    fn set_zoom(&mut self, z: f32) -> f32 {
        let old = self.zoom;
        self.zoom = z.clamp(self.min_zoom, self.max_zoom);
        old
    }

    /// Show-a-point on level map `level` at world (`x`, `z`) (5.1).
    fn show_level_point(&mut self, level: usize, [x, z]: [f32; 2], zoom_up: bool, now: f64) {
        self.target = self.maps.levels[level].to_global(x, z);
        let old = self.zoom;
        if zoom_up && self.at_min_zoom() {
            self.set_zoom(self.max_zoom);
        }
        self.request_transition(old, now);
    }

    /// Show-a-point on the global picture (5.1, 5.3).
    fn show_global(&mut self, now: f64) {
        let f = self.layout.frame;
        self.target = self.global_at([f.x + f.width / 2.0, f.y + f.height / 2.0]);
        let old = self.set_zoom(self.min_zoom);
        self.request_transition(old, now);
    }

    /// The actor button (5.2).
    fn actor(&mut self, now: f64) {
        let Some((level, position, _)) = self.player.clone() else {
            return;
        };
        match self.maps.level(&level) {
            Some(i) => self.show_level_point(i, position, true, now),
            None => {
                self.log
                    .push(format!("~ Level Map '{level}' not registered"));
                self.show_global(now);
            }
        }
    }

    /// A zoom by `factor` at the cursor (5.4, 5.5): *T* under the cursor,
    /// the hint closed, a transition, when *Z* changed.
    fn zoom_at(&mut self, factor: f32, now: f64) {
        let old = self.set_zoom(self.zoom * factor);
        if (self.zoom - old).abs() < ZOOM_EPS {
            return;
        }
        self.target = self.global_at(self.cursor);
        self.hint = None;
        self.request_transition(old, now);
    }

    /// The keypad zoom (5.7): *T* kept, the hint kept, a transition asked
    /// for even when *Z* did not change.
    fn key_zoom(&mut self, factor: f32, now: f64) {
        let old = self.set_zoom(self.zoom * factor);
        self.request_transition(old, now);
    }

    /// The near test (4.2): *T* within the frame grown by its size on
    /// every side.
    fn near(&self) -> bool {
        let f = self.layout.frame;
        let [x, y] = self.frame_pos(self.target);
        x >= -f.width && x <= 2.0 * f.width && y >= -f.height && y <= 2.0 * f.height
    }

    /// A transition's frame (4.2, 4.4) at `now` seconds, `dt` after the
    /// previous.
    fn step(&mut self, now: f64, dt: f32) {
        let Some(mut req) = self.request else { return };
        if !req.near {
            req.near = self.near();
        }
        let phase = if req.near {
            Phase::Settle
        } else {
            Phase::Approach
        };
        let mut run = match req.run {
            Some(r) if r.phase == phase => r,
            _ if phase == Phase::Approach => Run {
                phase,
                target: self.target_rect(self.min_zoom),
                end: now + APPROACH_S,
                first: true,
            },
            _ => self.towards(phase, now),
        };
        if run.first {
            run.first = false;
            if (self.zoom - self.min_zoom).abs() >= ZOOM_EPS {
                run = self.towards(Phase::Approach, now);
            }
        }
        let r = run.end - now;
        if r <= 0.0 {
            self.picture = run.target;
            self.request = None;
            return;
        }
        let k = (f64::from(dt).min(r) / r) as f32;
        for (p, t) in self.picture.iter_mut().zip(run.target) {
            *p += (t - *p) * k;
        }
        req.run = Some(run);
        self.request = Some(req);
    }

    /// Level picture `i`'s rectangle relative to the frame (2.2).
    fn level_rect(&self, i: usize) -> [f32; 4] {
        let g = self.maps.levels[i].global;
        let [l, t] = self.frame_pos([g[0], g[1]]);
        let [r, b] = self.frame_pos([g[2], g[3]]);
        [l, t, r, b]
    }

    fn frame_overlaps(&self, [l, t, r, b]: [f32; 4]) -> bool {
        let f = self.layout.frame;
        r > 0.0 && l < f.width && b > 0.0 && t < f.height
    }

    /// The player's marks (6.1, 6.3, 6.7): each spot's absolute rectangle
    /// and angle, when drawn.
    fn marks(&self) -> Vec<(usize, Rect, f32)> {
        let Some((level, [x, z], yaw)) = &self.player else {
            return Vec::new();
        };
        let Some(i) = self.maps.level(level) else {
            return Vec::new();
        };
        let r = self.level_rect(i);
        if self.at_min_zoom() || !self.frame_overlaps(r) {
            return Vec::new();
        }
        // 2.3: the width ratio on both axes.
        let [u, v] = self.maps.levels[i].picture.point(*x, *z, r[2] - r[0]);
        let f = self.layout.frame;
        let (cx, cy) = (f.x + r[0] + u, f.y + r[1] + v);
        let s = self.scale();
        let angle = heading([yaw.sin(), yaw.cos()]);
        (self.layout.spots.iter().enumerate())
            .filter_map(|(j, spot)| {
                let w = &spot.desc.window;
                let (mut sw, mut sh) = (w.width, w.height);
                if spot.scale {
                    sw *= s;
                    sh *= s;
                }
                let rect = if w.centred {
                    Rect::new(cx - sw / 2.0, cy - sh / 2.0, sw, sh)
                } else {
                    Rect::new(cx, cy, sw, sh)
                };
                let rel = [
                    rect.x - f.x,
                    rect.y - f.y,
                    rect.x - f.x + sw,
                    rect.y - f.y + sh,
                ];
                self.frame_overlaps(rel).then_some((j, rect, angle))
            })
            .collect()
    }

    /// The two scroll bars' values and rectangles (5.6): horizontal, then
    /// vertical.
    fn bars(&self) -> [(Rect, BarValues); 2] {
        let f = self.layout.frame;
        let t = self.layout.scroll.width;
        let p = self.picture;
        let values = |extent: f32, frame: f32, offset: f32, length: f32| {
            let mut v = BarValues {
                pos: 0,
                max: extent as i32,
                page: frame as i32,
                step: ((0.1 * frame) as i32).max(1),
                length,
                t,
            };
            v.pos = ((-offset) as i32).clamp(0, v.limit());
            v
        };
        let h = Rect::new(f.x, f.y + f.height + BAR_GAP, f.width, t);
        let vlen = f.height + BAR_GAP + t;
        let v = Rect::new(f.x + f.width + BAR_GAP, f.y, t, vlen);
        [
            (h, values(p[2] - p[0], f.width, p[0], f.width)),
            (v, values(p[3] - p[1], f.height, p[1], vlen)),
        ]
    }

    /// Bar `vertical`'s arrow `up` rectangle (absolute).
    fn arrow_rect(&self, vertical: bool, up: bool) -> Rect {
        let (r, v) = self.bars()[usize::from(vertical)];
        let (w, h) = if up {
            self.layout.up_arrow
        } else {
            self.layout.down_arrow
        };
        let along = if up { 0.0 } else { v.length - v.t };
        if vertical {
            Rect::new(r.x, r.y + along, w, h)
        } else {
            Rect::new(r.x + along, r.y, h, w)
        }
    }

    /// Bar `vertical`'s box rectangle (absolute).
    fn box_rect(&self, vertical: bool) -> Rect {
        let (r, v) = self.bars()[usize::from(vertical)];
        let (start, len) = (v.box_start(), v.box_length());
        if vertical {
            Rect::new(r.x, r.y + start, r.width, len)
        } else {
            Rect::new(r.x + start, r.y, len, r.height)
        }
    }

    /// Sets bar `vertical`'s position: the picture follows, unclamped
    /// (5.6).
    fn set_bar(&mut self, vertical: bool, pos: i32) {
        let v = self.bars()[usize::from(vertical)].1;
        let pos = pos.clamp(0, v.limit());
        let (a, b) = if vertical { (1, 3) } else { (0, 2) };
        let size = self.picture[b] - self.picture[a];
        self.picture[a] = -(pos as f32);
        self.picture[b] = self.picture[a] + size;
    }

    fn bar_step(&mut self, vertical: bool, steps: i32) {
        let v = self.bars()[usize::from(vertical)].1;
        self.set_bar(vertical, v.pos + steps * v.step);
    }

    /// The bar under `p`, if any.
    fn bar_at(&self, p: [f32; 2]) -> Option<bool> {
        let bars = self.bars();
        [false, true].into_iter().find(|&vertical| {
            dialog::contains(bars[usize::from(vertical)].0, p)
                || dialog::contains(self.arrow_rect(vertical, false), p)
        })
    }

    /// A left press on a bar (options-screens.md 4.8): an arrow steps, the
    /// box is grabbed.
    fn bar_press(&mut self, vertical: bool, p: [f32; 2]) {
        for up in [true, false] {
            if dialog::contains(self.arrow_rect(vertical, up), p) {
                self.bar_step(vertical, if up { -1 } else { 1 });
                self.grab = Some(Grab::Arrow { vertical, up });
                return;
            }
        }
        let b = self.box_rect(vertical);
        if dialog::contains(b, p) {
            let offset = if vertical { p[1] - b.y } else { p[0] - b.x };
            self.grab = Some(Grab::Box { vertical, offset });
        }
    }

    /// Whether the window takes key `key` (5.7).
    pub fn takes(key: Key) -> bool {
        matches!(key, Key::NumpadAdd | Key::NumpadSubtract)
    }

    /// One event while shown, the cursor at `cursor` after it; returns
    /// whether it was taken.
    pub fn event(&mut self, e: Ev, cursor: [f32; 2], held: bool, global_ms: u64) -> bool {
        let now = global_ms as f64 / 1000.0;
        let previous = self.cursor;
        self.cursor = cursor;
        match e {
            Ev::Move(p) => {
                self.frame_buttons();
                if let Some(Grab::Box { vertical, offset }) = self.grab {
                    let (r, v) = self.bars()[usize::from(vertical)];
                    let along = if vertical { p[1] - r.y } else { p[0] - r.x };
                    self.set_bar(vertical, v.drag_to(along - offset));
                    return true;
                }
                for (i, b) in self.buttons.iter_mut().enumerate() {
                    if !self.layout.tools[i].check {
                        b.event(e);
                    }
                }
                let pressed = self.buttons.iter().any(|b| b.state != ButtonState::Idle);
                if held {
                    // 5.6: a drag over the global picture's whole rectangle.
                    if !pressed && self.grab.is_none() && self.in_picture(p) {
                        self.move_by(p[0] - previous[0], p[1] - previous[1]);
                        self.hint = None;
                    }
                } else {
                    self.update_label(p);
                }
                true
            }
            Ev::Press { double } => {
                self.frame_buttons();
                if let Some(i) = self.tool_at(cursor) {
                    if self.layout.tools[i].check {
                        // pda-controls.md 4: a left press toggles; one
                        // counted as a double click does not.
                        if !double {
                            self.toggle(i);
                        }
                    } else {
                        self.buttons[i].event(e);
                    }
                    return true;
                }
                if let Some(vertical) = self.bar_at(cursor) {
                    self.bar_press(vertical, cursor);
                    return true;
                }
                // 5.4: with a tool on, a press in the map window zooms; a
                // double click does not.
                if dialog::contains(self.layout.main, cursor) {
                    if !double && let Some(tool) = self.tool {
                        let f = if tool == ZOOM_IN { ZOOM_UP } else { ZOOM_DOWN };
                        self.zoom_at(f, now);
                    }
                    return true;
                }
                false
            }
            Ev::Release => {
                self.frame_buttons();
                if let Some(Grab::Arrow { vertical, up }) = self.grab
                    && dialog::contains(self.arrow_rect(vertical, up), cursor)
                {
                    // options-screens.md 4.8: the click steps again.
                    self.bar_step(vertical, if up { -1 } else { 1 });
                }
                self.grab = None;
                for i in 0..self.buttons.len() {
                    if !self.layout.tools[i].check && self.buttons[i].event(e) == Pushed::Activated
                    {
                        match i {
                            GLOBAL_BUTTON => self.show_global(now),
                            ACTOR_BUTTON => self.actor(now),
                            _ => {}
                        }
                    }
                }
                true
            }
            Ev::Wheel { up } => {
                if let Some(vertical) = self.bar_at(cursor) {
                    self.bar_step(vertical, if up { -1 } else { 1 });
                    return true;
                }
                if dialog::contains(self.layout.main, cursor) {
                    self.zoom_at(if up { ZOOM_DOWN } else { ZOOM_UP }, now);
                    return true;
                }
                false
            }
            // 5.7: every event of the key, press and release alike.
            Ev::Key { key, .. } if Self::takes(key) => {
                let f = if key == Key::NumpadAdd {
                    ZOOM_UP
                } else {
                    ZOOM_DOWN
                };
                self.key_zoom(f, now);
                true
            }
            _ => false,
        }
    }

    /// 5.4: pressing zoom button `i` turns its tool on when it is now
    /// checked, the other off.
    fn toggle(&mut self, i: usize) {
        self.tool = if self.tool == Some(i) { None } else { Some(i) };
    }

    fn tool_at(&self, p: [f32; 2]) -> Option<usize> {
        (self.layout.tools.iter()).position(|t| dialog::contains(t.rect, p))
    }

    fn frame_buttons(&mut self) {
        for (b, t) in self.buttons.iter_mut().zip(&self.layout.tools) {
            b.frame(t.rect, self.cursor);
        }
    }

    fn in_picture(&self, p: [f32; 2]) -> bool {
        let f = self.layout.frame;
        let r = self.picture;
        dialog::contains(self.layout.main, p)
            && dialog::contains(
                Rect::new(f.x + r[0], f.y + r[1], r[2] - r[0], r[3] - r[1]),
                p,
            )
    }

    fn in_frame(&self, p: [f32; 2]) -> bool {
        dialog::contains(self.layout.frame, p)
    }

    /// The topmost level picture under `p`.
    fn level_at(&self, p: [f32; 2]) -> Option<usize> {
        let f = self.layout.frame;
        (0..self.maps.levels.len()).rev().find(|&i| {
            let r = self.level_rect(i);
            dialog::contains(
                Rect::new(f.x + r[0], f.y + r[1], r[2] - r[0], r[3] - r[1]),
                p,
            )
        })
    }

    /// 7.1: a move without a button over the frame names the level picture
    /// under the cursor, else the global map.
    fn update_label(&mut self, p: [f32; 2]) {
        if !self.in_frame(p) {
            return;
        }
        if let Some(i) = self.level_at(p) {
            self.label = Some(self.maps.levels[i].name.clone());
        } else if self.in_picture(p) {
            self.label = Some(GLOBAL_NAME.to_owned());
        }
    }

    /// One frame while shown (3, 4.4, 5.6, 7.2), after the events.
    pub fn update(&mut self, f: MapFrame) {
        if !self.shown {
            return;
        }
        let now = f.global_ms as f64 / 1000.0;
        self.cursor = f.cursor;
        self.player = f.player.map(|p| (p.level.to_owned(), p.position, p.yaw));
        if std::mem::take(&mut self.first_pending) {
            self.actor(now);
        }
        self.frame_buttons();
        // 5.6: each held arrow key moves the picture a unit, the way of
        // the arrow; the keyboard's alone (the d-pad nudges the cursor).
        let mut moved = false;
        for (key, dx, dy) in [
            (Key::Up, 0.0, 1.0),
            (Key::Down, 0.0, -1.0),
            (Key::Left, 1.0, 0.0),
            (Key::Right, -1.0, 0.0),
        ] {
            if f.input.device_held(crate::core::input::Control::Key(key)) {
                self.move_by(dx, dy);
                moved = true;
            }
        }
        // Our design: the right stick moves the map.
        let stick = [
            f.input.axis(GamepadAxis::RightX),
            f.input.axis(GamepadAxis::RightY),
        ];
        if stick[0].hypot(stick[1]) >= PAN_DEAD_ZONE {
            self.move_by(-stick[0] * PAN_SPEED * f.dt, -stick[1] * PAN_SPEED * f.dt);
            moved = true;
        }
        if moved {
            self.hint = None;
        }
        // options-screens.md 4.8: a held arrow repeats.
        let held = dialog::left_held(f.input);
        if held
            && self.left_was_held
            && let Some(Grab::Arrow { vertical, up }) = self.grab
            && dialog::contains(self.arrow_rect(vertical, up), self.cursor)
            && f.global_ms.saturating_sub(self.last_repeat) > REPEAT_MS
        {
            self.last_repeat = f.global_ms;
            self.bar_step(vertical, if up { -1 } else { 1 });
        }
        self.left_was_held = held;
        self.step(now, f.dt);
        self.update_hint(f.data, f.global_ms);
    }

    /// 7.2: what the cursor is over, the hint opened after 500 ms of it,
    /// closed when the cursor leaves its source or a level's away from the
    /// minimum zoom.
    fn update_hint(&mut self, data: &GameData, global_ms: u64) {
        let p = self.cursor;
        let marks = self.marks();
        let over = if !self.in_frame(p) {
            None
        } else if let Some((j, _, _)) = marks.iter().rev().find(|(_, r, _)| dialog::contains(*r, p))
        {
            Some(HintSource::Mark(*j))
        } else {
            self.level_at(p).map(HintSource::Level)
        };
        if self.hover.map(|(s, _)| s) != over {
            self.hover = over.map(|s| (s, global_ms));
        }
        if let Some(h) = &self.hint {
            let away = match h.source {
                HintSource::Level(_) => !self.at_min_zoom(),
                HintSource::Mark(_) => false,
            };
            // Leaving a level picture closes its hint too: our choice, the
            // spec leaves it open (#742).
            if away || over != Some(h.source) {
                self.hint = None;
            }
            return;
        }
        let Some((source, since)) = self.hover else {
            return;
        };
        if global_ms.saturating_sub(since) <= HINT_DELAY_MS {
            return;
        }
        let key = match source {
            HintSource::Level(i) if self.at_min_zoom() => self.maps.levels[i].name.clone(),
            HintSource::Level(_) => return,
            HintSource::Mark(j) => self.layout.spots[j].hint.clone(),
        };
        let text = data.strings.get(&key).to_owned();
        let height =
            HINT_MIN_HEIGHT.max(4.0 + (self.text_height)(&crate::data::encode(&text)) + 20.0);
        let rect = self.hint_place(height);
        self.hint = Some(MapHint { source, text, rect });
    }

    /// 7.2: the first of four places strictly inside the frame, else the
    /// last.
    fn hint_place(&self, h: f32) -> Rect {
        let [cx, cy] = self.cursor;
        let w = HINT_WIDTH;
        let f = self.layout.frame;
        let inside =
            |x: f32, y: f32| x > f.x && x + w < f.x + f.width && y > f.y && y + h < f.y + f.height;
        let at = [(cx, cy - h), (cx - w, cy - h), (cx - w, cy)]
            .into_iter()
            .find(|&(x, y)| inside(x, y))
            .unwrap_or((cx, cy + HINT_BELOW));
        Rect::new(at.0, at.1, w, h)
    }

    /// Tool button `i`'s look (pda-controls.md 4): `e`, `t` or `h`.
    fn tool_look(&self, i: usize) -> &'static str {
        if self.layout.tools[i].check {
            if self.tool == Some(i) {
                "t"
            } else if self.buttons[i].over {
                "h"
            } else {
                "e"
            }
        } else {
            self.buttons[i].look()
        }
    }

    /// The window (1.1): the frame window, the pictures and marks clipped
    /// to the frame, the header line with the tools, the scroll bars, the
    /// label, the hint window.
    pub fn draw(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let l = &self.layout;
        if let Some(base) = &l.frame_base {
            ctx.frame_window(s, "pda map frame", base, l.frame_window)?;
        }
        let f = l.frame;
        let start = s.sprites.len();
        let abs = |r: [f32; 4]| Rect::new(f.x + r[0], f.y + r[1], r[2] - r[0], r[3] - r[1]);
        let g = &self.maps.global.picture;
        stretched(ctx, s, "pda map global", &g.texture, abs(self.picture))?;
        for (i, level) in self.maps.levels.iter().enumerate() {
            let layer = format!("pda map level {}", level.name);
            stretched(
                ctx,
                s,
                &layer,
                &level.picture.texture,
                abs(self.level_rect(i)),
            )?;
        }
        for (j, rect, angle) in self.marks() {
            let spot = &l.spots[j];
            let mut desc = spot.desc.clone();
            desc.window.width = rect.width;
            desc.window.height = rect.height;
            ctx.picture(
                s,
                &format!("pda map mark {j}"),
                &desc,
                rect,
                angle,
                0,
                false,
                None,
            )?;
        }
        s.clip_from(start, pixels(f, ctx.scale()));
        if let Some(base) = &l.header_base {
            ctx.frame_line(
                s,
                "pda map header",
                base,
                (l.header.x, l.header.y),
                l.header.width,
            )?;
        }
        for (i, t) in l.tools.iter().enumerate() {
            let look = ["e", "t", "h"].iter().position(|&l| l == self.tool_look(i));
            if let Some(id) = look.and_then(|l| t.looks[l].as_ref()) {
                ctx.part(
                    s,
                    &format!("pda map tool {i}"),
                    id,
                    (t.rect.x, t.rect.y),
                    None,
                    Color::WHITE,
                )?;
            }
        }
        self.draw_bars(ctx, s)?;
        if let Some(key) = &self.label {
            let r = l.label.window.absolute(0.0, 0.0);
            let (b, font) = TextBox::of_static(&l.label, r);
            ctx.text(s, "pda map label", &ctx.data.text(key), &b, font)?;
        }
        if let Some(h) = &self.hint {
            ctx.frame_window(s, "pda map hint", &l.hint.frame, h.rect)?;
            let d = &l.hint.description;
            let r = d.window.absolute(h.rect.x, h.rect.y);
            let (b, font) = TextBox::of_static(d, r);
            ctx.text(
                s,
                "pda map hint text",
                &crate::data::encode(&h.text),
                &b,
                font,
            )?;
        }
        Ok(())
    }

    /// The scroll bars (5.6): the vertical bar as `options-screens` 4.8
    /// draws it with the `pda` profile; the horizontal one the same turned
    /// a quarter counter-clockwise, its up arrow at the left (the spec's
    /// belief; the profile's pictures are a vertical bar's).
    fn draw_bars(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let p = &self.layout.scroll;
        for vertical in [false, true] {
            let r = self.bars()[usize::from(vertical)].0;
            let layer = if vertical {
                "pda map bar v"
            } else {
                "pda map bar h"
            };
            let parts = [
                (&p.track, r),
                (&p.thumb, self.box_rect(vertical)),
                (&p.up, self.arrow_rect(vertical, true)),
                (&p.down, self.arrow_rect(vertical, false)),
            ];
            for (id, rect) in parts {
                let n = s.sprites.len();
                ctx.part_stretched(s, layer, id, rect)?;
                if !vertical {
                    for sprite in &mut s.sprites[n..] {
                        // Texture top to the left: the texture's corners
                        // (top-left, top-right, bottom-right, bottom-left)
                        // land on the screen's bottom-left, top-left,
                        // top-right, bottom-right.
                        let c = sprite.corners;
                        sprite.corners = [c[3], c[0], c[1], c[2]];
                    }
                }
            }
        }
        Ok(())
    }

    /// What tests observe.
    pub fn state(&self) -> MapState {
        MapState {
            shown: self.shown,
            main: {
                let m = self.layout.main;
                [m.x, m.y, m.width, m.height]
            },
            zoom: self.zoom,
            scale: self.scale(),
            min_zoom: self.min_zoom,
            target: self.target,
            picture: self.picture,
            phase: self.request.and_then(|r| r.run.map(|r| r.phase)),
            moving: self.request.is_some() || self.first_pending,
            tool: self.tool,
            looks: (0..self.layout.tools.len())
                .map(|i| self.tool_look(i))
                .collect(),
            label: self.label.clone(),
            hint: self.hint.as_ref().map(|h| MapHintState {
                text: h.text.clone(),
                rect: [h.rect.x, h.rect.y, h.rect.width, h.rect.height],
            }),
            marks: (self.marks().into_iter())
                .map(|(j, r, a)| MarkState {
                    spot: j,
                    rect: [r.x, r.y, r.width, r.height],
                    angle: a,
                })
                .collect(),
            bars: self.bars().map(|(_, v)| [v.pos, v.max, v.page, v.step]),
        }
    }
}

/// The heading of direction (`x`, `z`) (6.3).
pub fn heading([x, z]: [f32; 2]) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    if z.abs() < 1e-7 {
        if x.abs() < 1e-7 {
            0.0
        } else if x > 0.0 {
            -FRAC_PI_2
        } else {
            FRAC_PI_2
        }
    } else {
        let a = -(x / z).atan();
        if z < 0.0 { a - PI } else { a }
    }
}

/// Texture path `path` stretched over `rect` (1.2).
fn stretched(ctx: &mut Ctx, s: &mut Scene, layer: &str, path: &str, rect: Rect) -> io::Result<()> {
    let mut desc = picture_desc(path, rect, None, Color::WHITE);
    desc.stretch = true;
    ctx.picture(s, layer, &desc, rect, 0.0, 0, false, None)
}

/// `r` in output pixels: min x, min y, max x, max y.
fn pixels(r: Rect, (sx, sy): (f32, f32)) -> [f32; 4] {
    [
        r.x * sx - 0.5,
        r.y * sy - 0.5,
        (r.x + r.width) * sx - 0.5,
        (r.y + r.height) * sy - 0.5,
    ]
}

/// A mark as tests see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MarkState {
    /// Index into [`PLAYER_SPOTS`].
    pub spot: usize,
    pub rect: [f32; 4],
    pub angle: f32,
}

/// The hint window as tests see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MapHintState {
    pub text: String,
    pub rect: [f32; 4],
}

/// The map window as tests see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MapState {
    pub shown: bool,
    /// The map window, absolute (x, y, width, height).
    pub main: [f32; 4],
    /// The wanted zoom *Z*.
    pub zoom: f32,
    /// The current scale *s*.
    pub scale: f32,
    pub min_zoom: f32,
    pub target: [f32; 2],
    /// Left, top, right, bottom relative to the frame.
    pub picture: [f32; 4],
    pub phase: Option<Phase>,
    /// A transition is under way or due (the first showing's).
    pub moving: bool,
    pub tool: Option<usize>,
    pub looks: Vec<&'static str>,
    pub label: Option<String>,
    pub hint: Option<MapHintState>,
    pub marks: Vec<MarkState>,
    /// Each bar's position, maximum, page and step.
    pub bars: [[i32; 4]; 2],
}
