// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The title menu (docs/behavior/title-menu.md) on the title screen: its
//! layers, the item list with the sliding bar and cog wheels, the version
//! caption, the cursor, the music, and navigation and activation.
//!
//! New game and options open their screens (the app shows them). Items
//! that open other screens (load, multiplayer, credits, last save) are not
//! implemented: activating them only records what the spec says they do
//! ([`TitleMenuState::activations`]); nothing visible happens. `btn_quit` should open a confirmation box first; that box is
//! another screen, so it quits at once, like the Q key, which here means
//! leaving the engine's screen for the app's own interface.

use std::f32::consts::PI;
use std::io;

use serde::Serialize;

use crate::audio::{AudioOut, Voice};
use crate::core::audio::{MUSIC_PLAYBACK, music_files};
use crate::core::input::{Action, Control, Input, Key, MouseButton, ui_from_normalized};
use crate::data::GameData;
use crate::draw::Ctx;
use crate::scene::Scene;
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::Resolution;
use volya_engine_formats::ui::layout::{Rect, StaticDesc};
use volya_engine_formats::ui::title_menu::MenuList;

/// The list shown on the title screen without a valid last save
/// (section 4.1). This platform has no saved games, so there is never a
/// valid last save and `menu_main_last_save` is not used.
pub const MAIN_LIST: &str = "menu_main";
/// The difficulty page (section 4.5).
pub const NEW_GAME_LIST: &str = "menu_new_game";
/// Shortest bar move, milliseconds (section 4.3).
const MIN_MOVE_MS: f64 = 100.0;
/// Cog-wheel width factor on wide screens after a resize (section 8).
const WIDE_WHEEL: f32 = 0.751_879_7;

/// A move of the bar (section 4.3).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Move {
    from: f32,
    to: f32,
    start: u64,
    duration: f64,
}

impl Move {
    /// The bar y `t` ms into the move: *y₀* + (*y₁* − *y₀*) ·
    /// ln(1 + 10·*t*/*T*) / ln 11; `None` once *t* exceeds *T*.
    fn y_at(&self, now: u64) -> Option<f32> {
        let t = now.saturating_sub(self.start) as f64;
        (t <= self.duration).then(|| {
            let k = (1.0 + 10.0 * t / self.duration).ln() / 11f64.ln();
            self.from + (self.to - self.from) * k as f32
        })
    }
}

/// The duration *T* of a move over `d` units in a window `h` units high:
/// round(1000 · ln(*d* + 1) / ln *h*) ms, at least 100 ms (section 4.3).
pub fn move_duration(d: f32, h: f32) -> f64 {
    (1000.0 * (f64::from(d) + 1.0).ln() / f64::from(h).ln())
        .round()
        .max(MIN_MOVE_MS)
}

/// The cog-wheel angle for bar y `y` and wheel height `h`:
/// 2π · frac(*y* / (π·*h*)) radians (section 4.3).
pub fn wheel_angle(y: f32, h: f32) -> f32 {
    let turns = y / (PI * h);
    2.0 * PI * (turns - turns.floor())
}

/// What activating an item did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Activation {
    pub item: String,
    /// What the spec says happens (section 4.5), and whether it is done.
    pub effect: String,
}

/// The menu.
#[derive(Clone, Debug)]
pub struct TitleMenu {
    /// The list shown: [`MAIN_LIST`] or [`NEW_GAME_LIST`].
    list: &'static str,
    selected: usize,
    /// The bar's y relative to `shniaga_wnd` when not moving.
    bar_y: f32,
    motion: Option<Move>,
    /// The right wheel's angle; the left one turns the other way.
    angle: f32,
    /// The row under the cursor.
    hovered: Option<usize>,
    /// The cursor in UI units.
    cursor: [f32; 2],
    /// The cog wheels' width (section 8).
    wheel_width: f32,
    /// When the menu opened: its videos play from then.
    opened: u64,
    music: Option<[Voice; 2]>,
    track: Option<String>,
    /// Times the music was started.
    music_starts: u32,
    /// State of the pseudo-random track pick.
    seed: u32,
    activations: Vec<Activation>,
    quit: bool,
    /// The difficulty of a new game the player chose
    /// (level-loading.md section 2.1), until the app takes it.
    new_game: Option<&'static str>,
    /// The options screen was asked for (section 4.5), until the app
    /// takes it.
    options: bool,
}

impl TitleMenu {
    /// Opens the menu at `now` (section 2): the main list, the bar at its
    /// XML position sliding to item 0, the music started.
    pub fn open(data: &GameData, now: u64, input: &Input, audio: &mut dyn AudioOut) -> Self {
        let wheel_width = data.menu.left_anim.window.width;
        let mut menu = Self {
            list: MAIN_LIST,
            selected: 0,
            bar_y: data.menu.shniaga.window.y,
            motion: None,
            angle: 0.0,
            hovered: None,
            cursor: input.cursor_ui(),
            wheel_width,
            opened: now,
            music: None,
            track: None,
            music_starts: 0,
            seed: 1,
            activations: Vec::new(),
            quit: false,
            new_game: None,
            options: false,
        };
        // The cursor already over a row does not select it; only entering
        // a row does (the spec does not cover the cursor's position at
        // opening).
        menu.hovered = menu.row_at(data, menu.cursor);
        menu.select(data, 0, now);
        menu.start_music(data, audio);
        menu
    }

    fn list<'a>(&self, data: &'a GameData) -> &'a MenuList {
        data.menu
            .menu(self.list)
            .unwrap_or_else(|| panic!("ui_mm_main.xml has no {}", self.list))
    }

    /// Whether `ui_mm_main.xml` has the lists this menu shows.
    pub fn check(data: &GameData) -> io::Result<()> {
        for list in [MAIN_LIST, NEW_GAME_LIST] {
            if data.menu.menu(list).is_none_or(|m| m.buttons.is_empty()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("ui_mm_main.xml: no {list} items"),
                ));
            }
        }
        if data.menu.menu_sound.music.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ui_mm_main.xml: no menu_music",
            ));
        }
        Ok(())
    }

    /// The bar's y at `now`.
    pub fn bar_y(&self, now: u64) -> f32 {
        match self.motion {
            Some(m) => m.y_at(now).unwrap_or(m.to),
            None => self.bar_y,
        }
    }

    /// The bar y that puts the glass over item `i`: row y − `magnifire` y +
    /// `y_offset` (section 4.3).
    fn target(&self, data: &GameData, i: usize) -> f32 {
        let m = &data.menu;
        let list = self.list(data);
        let (positions, _) = m
            .buttons_region
            .layout(&vec![list.btn_height; list.buttons.len()]);
        m.buttons_region.window.rect().y + positions[i].1 - m.magnifire.window.y + m.y_offset
    }

    /// Selects item `i` at `now`: the bar moves to it from where it is.
    fn select(&mut self, data: &GameData, i: usize, now: u64) {
        let from = self.bar_y(now);
        self.bar_y = from;
        let to = self.target(data, i);
        let duration = move_duration((to - from).abs(), data.menu.shniaga_wnd.height);
        self.selected = i;
        self.motion = Some(Move {
            from,
            to,
            start: now,
            duration,
        });
    }

    /// The absolute rectangle of `shniaga_wnd` and of the bar at `now`.
    fn bar_rect(&self, data: &GameData, now: u64) -> (Rect, Rect) {
        let m = &data.menu;
        let wnd = m.shniaga_wnd.absolute(0.0, 0.0);
        let r = m.shniaga.window.rect();
        let bar = Rect::new(wnd.x + r.x, wnd.y + self.bar_y(now), r.width, r.height);
        (wnd, bar)
    }

    /// The glass (`magnifire`) at `now`, absolute: the click area (4.4).
    pub fn glass(&self, data: &GameData, now: u64) -> Rect {
        let (_, bar) = self.bar_rect(data, now);
        data.menu.magnifire.window.absolute(bar.x, bar.y)
    }

    fn row_at(&self, data: &GameData, [x, y]: [f32; 2]) -> Option<usize> {
        data.menu
            .caption_rows(self.list(data))
            .iter()
            .position(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
    }

    fn set_list(&mut self, data: &GameData, list: &'static str, now: u64) {
        self.list = list;
        self.hovered = self.row_at(data, self.cursor);
        self.select(data, 0, now);
    }

    fn activate(&mut self, data: &GameData, now: u64) {
        let item = self.list(data).buttons[self.selected].name.clone();
        let effect = match item.as_str() {
            "btn_new_game" => {
                self.set_list(data, NEW_GAME_LIST, now);
                "switches to the difficulty page"
            }
            "btn_new_back" => {
                self.set_list(data, MAIN_LIST, now);
                "switches back to the main page"
            }
            "btn_novice" | "btn_stalker" | "btn_veteran" | "btn_master" => {
                // The console commands' `g_game_difficulty` value
                // (level-loading.md section 2.1).
                self.new_game = Some(match item.as_str() {
                    "btn_novice" => "gd_novice",
                    "btn_stalker" => "gd_stalker",
                    "btn_veteran" => "gd_veteran",
                    _ => "gd_master",
                });
                "sets the difficulty and starts a new game"
            }
            "btn_load" => "opens the load-game screen (not implemented)",
            "btn_lastsave" => "loads the last save (not implemented)",
            "btn_multiplayer" => "opens the multiplayer screen (not implemented)",
            "btn_options" => {
                self.options = true;
                "opens the options screen"
            }
            "btn_credits" => "starts the credits sequence (not implemented)",
            "btn_quit" => {
                self.quit = true;
                "quits (without the confirmation box, which is not implemented)"
            }
            _ => "no action is bound to this item",
        };
        self.activations.push(Activation {
            item,
            effect: effect.to_owned(),
        });
    }

    /// One frame at `now`: this frame's input in order (section 4.4), then
    /// the bar, the wheels and the music.
    pub fn update(&mut self, data: &GameData, now: u64, input: &Input, audio: &mut dyn AudioOut) {
        for action in input.actions() {
            if self.quit || self.new_game.is_some() || self.options {
                break;
            }
            match *action {
                Action::Cursor { to } => {
                    self.cursor = ui_from_normalized(to);
                    let row = self.row_at(data, self.cursor);
                    if row != self.hovered {
                        self.hovered = row;
                        if let Some(i) = row {
                            self.select(data, i, now);
                        }
                    }
                }
                Action::Press {
                    control: Control::Mouse(MouseButton::Left),
                    double: false,
                } => {
                    let g = self.glass(data, now);
                    let [x, y] = self.cursor;
                    if x >= g.x && x < g.x + g.width && y >= g.y && y < g.y + g.height {
                        self.activate(data, now);
                    }
                }
                Action::Press {
                    control: Control::Key(key),
                    ..
                } => match key {
                    Key::Up if self.selected > 0 => self.select(data, self.selected - 1, now),
                    Key::Down if self.selected + 1 < self.list(data).buttons.len() => {
                        self.select(data, self.selected + 1, now)
                    }
                    // The numeric keypad's Enter acts as Enter here (our
                    // choice; it is bindable as kNUMPADENTER).
                    Key::Enter | Key::NumpadEnter => self.activate(data, now),
                    Key::Escape if self.list == NEW_GAME_LIST => {
                        self.set_list(data, MAIN_LIST, now)
                    }
                    Key::Q => {
                        self.quit = true;
                        self.activations.push(Activation {
                            item: "key q".into(),
                            effect: "quits".into(),
                        });
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        if let Some(m) = self.motion {
            let y = m.y_at(now).unwrap_or(m.to);
            if m.y_at(now).is_none() {
                self.motion = None;
            }
            self.bar_y = y;
            self.angle = wheel_angle(y, data.menu.left_anim.window.height);
        }
        if self.quit {
            self.stop(audio);
            return;
        }
        // Section 6: on the title screen the game is never paused, so a
        // stopped channel restarts the pair.
        if let Some([l, r]) = self.music
            && !(audio.is_playing(l) && audio.is_playing(r))
        {
            self.start_music(data, audio);
        }
    }

    /// Picks a track (pseudo-randomly, whatever `random` says) and starts
    /// both channels. When a channel cannot play (no audio output), the
    /// music stays off rather than being retried every frame.
    fn start_music(&mut self, data: &GameData, audio: &mut dyn AudioOut) {
        if let Some(voices) = self.music.take() {
            for v in voices {
                audio.stop(v);
            }
        }
        let tracks = &data.menu.menu_sound.music;
        if tracks.is_empty() {
            return;
        }
        // A linear congruential generator (our choice of pseudo-random).
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let track = &tracks[(self.seed >> 16) as usize % tracks.len()];
        let [l, r] =
            music_files(track).map(|f| data.aliases.path("$game_data$", &f).unwrap_or_default());
        let left = audio.play(&l, MUSIC_PLAYBACK[0]);
        let right = audio.play(&r, MUSIC_PLAYBACK[1]);
        self.track = Some(track.clone());
        match (left, right) {
            (Some(l), Some(r)) => {
                self.music = Some([l, r]);
                self.music_starts += 1;
            }
            (l, r) => {
                l.into_iter().chain(r).for_each(|v| audio.stop(v));
            }
        }
    }

    /// Stops the music (the menu screen is discarded).
    pub fn stop(&mut self, audio: &mut dyn AudioOut) {
        if let Some(voices) = self.music.take() {
            for v in voices {
                audio.stop(v);
            }
        }
    }

    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    /// Whether the options screen was asked for, once.
    pub fn take_options(&mut self) -> bool {
        std::mem::take(&mut self.options)
    }

    /// The difficulty of the new game the player started, once.
    pub fn take_new_game(&mut self) -> Option<&'static str> {
        self.new_game.take()
    }

    /// The video device was reset at `resolution` (section 8): the wheels
    /// narrow on wide screens.
    pub fn resized(&mut self, data: &GameData, resolution: Resolution) {
        let width = data.menu.left_anim.window.width;
        self.wheel_width = if resolution.is_wide() {
            width * WIDE_WHEEL
        } else {
            width
        };
    }

    /// The frame at `ctx.clock_ms` (section 3, drawn in order).
    pub fn scene(&self, ctx: &mut Ctx) -> io::Result<Scene> {
        let mut scene = Scene::new(ctx.resolution);
        let data = ctx.data;
        let m = &data.menu;
        let now = ctx.clock_ms;
        let top = |s: &StaticDesc| s.window.absolute(0.0, 0.0);
        ctx.picture(
            &mut scene,
            "back_movie",
            &m.back_movie,
            top(&m.back_movie),
            0.0,
            self.opened,
            true,
            None,
        )?;
        ctx.picture(
            &mut scene,
            "background",
            &m.background,
            top(&m.background),
            0.0,
            0,
            false,
            None,
        )?;
        ctx.picture(
            &mut scene,
            "fire_movie",
            &m.fire_movie,
            top(&m.fire_movie),
            0.0,
            self.opened,
            true,
            None,
        )?;

        // `shniaga_wnd`: the captions, then the bar over them.
        let list = self.list(data);
        let font = list.font.unwrap_or(UiFont::DEFAULT);
        for (i, (b, text_box)) in list.buttons.iter().zip(m.caption_boxes(list)).enumerate() {
            ctx.text(
                &mut scene,
                &format!("caption {i}"),
                &data.text(&b.caption),
                &text_box,
                font,
            )?;
        }
        let (_, bar) = self.bar_rect(data, now);
        ctx.picture(&mut scene, "bar", &m.shniaga, bar, 0.0, 0, false, None)?;
        // `magnifire` is never drawn.
        for (layer, s) in [
            ("left_grating", &m.left_grating),
            ("right_grating", &m.right_grating),
        ] {
            ctx.picture(
                &mut scene,
                layer,
                s,
                s.window.absolute(bar.x, bar.y),
                0.0,
                0,
                false,
                None,
            )?;
        }
        for (layer, s, angle) in [
            ("left_anim", &m.left_anim, -self.angle),
            ("right_anim", &m.right_anim, self.angle),
        ] {
            let mut s = s.clone();
            s.window.width = self.wheel_width;
            let abs = s.window.absolute(bar.x, bar.y);
            ctx.picture(&mut scene, layer, &s, abs, angle, 0, false, None)?;
        }

        // Section 3.4: "ver. " and the version, as the code sets it.
        let v = &m.static_version;
        let (text_box, font): (TextBox, UiFont) = TextBox::of_static(v, top(v));
        ctx.picture(&mut scene, "static_version", v, top(v), 0.0, 0, false, None)?;
        let text = crate::data::encode(&format!("ver. {}", data.version));
        ctx.text(&mut scene, "version", &text, &text_box, font)?;

        // Section 7: the cursor's top-left at the cursor, 64 × 64 units from
        // a 64 × 64 frame (one texel per unit, as unstretched pictures).
        crate::draw::cursor(ctx, &mut scene, self.cursor)?;
        Ok(scene)
    }

    pub fn state(&self, data: &GameData, now: u64) -> TitleMenuState {
        let list = self.list(data);
        TitleMenuState {
            list: self.list,
            items: list
                .buttons
                .iter()
                .map(|b| MenuItem {
                    name: b.name.clone(),
                    caption: data.strings.get(&b.caption).to_owned(),
                })
                .collect(),
            selected: self.selected,
            bar_y: self.bar_y(now),
            bar_target: self.motion.map_or(self.bar_y, |m| m.to),
            moving: self.motion.is_some_and(|m| m.y_at(now).is_some()),
            wheel_angle: self.angle,
            wheel_width: self.wheel_width,
            hovered: self.hovered,
            cursor: self.cursor,
            cursor_frame: data.cursor.frame_index(now),
            glass: {
                let g = self.glass(data, now);
                [g.x, g.y, g.width, g.height]
            },
            version: format!("ver. {}", data.version),
            music: MusicState {
                track: self.track.clone(),
                voices: self.music,
                starts: self.music_starts,
            },
            activations: self.activations.clone(),
            quit: self.quit,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MenuItem {
    pub name: String,
    pub caption: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MusicState {
    pub track: Option<String>,
    /// Left and right voices while playing.
    pub voices: Option<[Voice; 2]>,
    pub starts: u32,
}

/// The menu as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct TitleMenuState {
    pub list: &'static str,
    pub items: Vec<MenuItem>,
    pub selected: usize,
    pub bar_y: f32,
    pub bar_target: f32,
    pub moving: bool,
    pub wheel_angle: f32,
    pub wheel_width: f32,
    pub hovered: Option<usize>,
    pub cursor: [f32; 2],
    pub cursor_frame: usize,
    /// The click area, absolute (x, y, width, height).
    pub glass: [f32; 4],
    pub version: String,
    pub music: MusicState,
    pub activations: Vec<Activation>,
    pub quit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::audio::Category;
    use crate::core::input::{Event, TouchPhase};
    use crate::fixture::{self, FakeAudio, GLYPH};
    use crate::scene::TextureId;

    const XGA: Resolution = Resolution::new(1024, 768);
    const HD: Resolution = Resolution::new(1920, 1080);

    struct Run {
        data: GameData,
        tree: fixture::MemVfs,
        menu: TitleMenu,
        input: Input,
        audio: FakeAudio,
    }

    impl Run {
        fn open(now: u64) -> Self {
            let tree = fixture::tree();
            let data = fixture::data(&tree);
            let input = Input::new();
            let mut audio = FakeAudio::default();
            let menu = TitleMenu::open(&data, now, &input, &mut audio);
            Self {
                data,
                tree,
                menu,
                input,
                audio,
            }
        }

        fn frame(&mut self, now: u64) {
            self.menu
                .update(&self.data, now, &self.input, &mut self.audio);
            self.input.end_frame();
        }

        fn key(&mut self, key: Key, now: u64) {
            self.input.handle(Event::Key { key, pressed: true });
            self.input.handle(Event::Key {
                key,
                pressed: false,
            });
            self.frame(now);
        }

        /// Moves the (absolute) pointer to UI point (x, y).
        fn point(&mut self, x: f32, y: f32, now: u64) {
            self.input.handle(Event::PointerMoved { at: [x, y] });
            self.frame(now);
        }

        fn click(&mut self, now: u64) {
            for pressed in [true, false] {
                self.input.handle(Event::PointerButton {
                    at: None,
                    button: MouseButton::Left,
                    pressed,
                    time: now as f64,
                });
            }
            self.frame(now);
        }

        fn state(&self, now: u64) -> TitleMenuState {
            self.menu.state(&self.data, now)
        }

        fn scene(&self, r: Resolution, now: u64) -> Scene {
            let mut assets = fixture::assets();
            let mut ctx = Ctx {
                tree: &self.tree,
                data: &self.data,
                assets: &mut assets,
                resolution: r,
                clock_ms: now,
            };
            self.menu.scene(&mut ctx).unwrap()
        }
    }

    #[test]
    fn move_durations() {
        // Section 4.3 examples.
        for (d, t) in [
            (22.0, 556.0),
            (30.0, 609.0),
            (60.0, 730.0),
            (90.0, 801.0),
            (120.0, 851.0),
            (150.0, 890.0),
            (180.0, 923.0),
        ] {
            assert_eq!(move_duration(d, 280.0), t, "d = {d}");
        }
        assert_eq!(move_duration(0.0, 280.0), 100.0);
    }

    #[test]
    fn opening_slides_onto_item_zero() {
        let mut r = Run::open(1000);
        let s = r.state(1000);
        assert_eq!(
            (s.list, s.selected, s.bar_y, s.bar_target, s.moving),
            ("menu_main", 0, 0.0, -22.0, true)
        );
        // Logarithmic ease: y₀ + (y₁ − y₀) · ln(1 + 10t/T) / ln 11.
        let t = 200.0;
        let expect = -22.0 * ((1.0 + 10.0 * t / 556.0f64).ln() / 11f64.ln()) as f32;
        assert!((r.state(1200).bar_y - expect).abs() < 1e-4);
        r.frame(1556);
        assert!((r.state(1556).bar_y - -22.0).abs() < 1e-4);
        r.frame(1557);
        let s = r.state(1557);
        assert_eq!((s.bar_y, s.moving), (-22.0, false));
        assert_eq!(s.glass, [675.0, 419.0, 232.0, 36.0]);
        // The wheels turned with the bar: 2π · frac(y / (π·55)), right +, left −.
        assert!((s.wheel_angle - wheel_angle(-22.0, 55.0)).abs() < 1e-5);
        assert!((wheel_angle(-22.0, 55.0) - (2.0 * PI - 0.8)).abs() < 1e-4);
        // At rest they keep their angle.
        r.frame(5000);
        assert_eq!(r.state(5000).wheel_angle, s.wheel_angle);
    }

    #[test]
    fn keys_navigate_without_wrapping() {
        let mut r = Run::open(0);
        r.key(Key::Up, 1000);
        assert_eq!(r.state(1000).selected, 0);
        r.key(Key::Down, 1000);
        let s = r.state(1000);
        assert_eq!((s.selected, s.bar_target), (1, 8.0));
        // A new selection during a move starts from where the bar is.
        let mid = r.state(1300).bar_y;
        r.key(Key::Down, 1300);
        let s = r.state(1300);
        assert_eq!((s.selected, s.bar_y, s.bar_target), (2, mid, 38.0));
        for _ in 0..10 {
            r.key(Key::Down, 1400);
        }
        assert_eq!(r.state(1400).selected, 5);
    }

    #[test]
    fn activation() {
        let mut r = Run::open(0);
        // Enter (the keypad's here) on New game: the difficulty page.
        r.key(Key::NumpadEnter, 1000);
        let s = r.state(1000);
        assert_eq!((s.list, s.selected, s.items.len()), ("menu_new_game", 0, 5));
        assert_eq!(s.items[4].caption, "Назад");
        r.key(Key::Down, 2000);
        r.key(Key::Enter, 3000);
        let s = r.state(3000);
        assert_eq!(s.activations.last().unwrap().item, "btn_stalker");
        assert_eq!(s.list, "menu_new_game");
        // The app takes the new game; the menu reacts again after.
        assert_eq!(r.menu.take_new_game(), Some("gd_stalker"));
        assert_eq!(r.menu.take_new_game(), None);
        // Escape returns to the main page; there it does nothing.
        r.key(Key::Escape, 4000);
        assert_eq!(
            (r.state(4000).list, r.state(4000).selected),
            ("menu_main", 0)
        );
        r.key(Key::Escape, 5000);
        assert!(!r.menu.quit_requested());
        // Назад returns too.
        r.key(Key::Enter, 6000);
        for _ in 0..4 {
            r.key(Key::Down, 6000);
        }
        r.key(Key::Enter, 7000);
        assert_eq!(r.state(7000).list, "menu_main");
        // Quit item: quits, music stopped.
        for _ in 0..5 {
            r.key(Key::Down, 8000);
        }
        assert!(!r.audio.playing.is_empty());
        r.key(Key::Enter, 9000);
        assert!(r.menu.quit_requested());
        assert!(r.audio.playing.is_empty());
        // Q quits at once.
        let mut r = Run::open(0);
        r.key(Key::Q, 10);
        assert!(r.menu.quit_requested());
    }

    #[test]
    fn mouse_hover_selects_and_glass_click_activates() {
        let mut r = Run::open(0);
        r.frame(1000);
        // Entering row 3 selects it; moving inside it does nothing more.
        r.point(782.0, 419.0 + 90.0 + 5.0, 1000);
        assert_eq!(r.state(1000).selected, 3);
        r.point(790.0, 419.0 + 90.0 + 20.0, 1500);
        assert_eq!(r.state(1500).bar_target, 68.0);
        assert!(r.state(1500).moving, "not restarted");
        // Leaving and re-entering row 3 selects it again: a 100 ms move.
        r.point(100.0, 100.0, 3000);
        r.point(782.0, 419.0 + 95.0, 3000);
        assert!(r.state(3050).moving && !r.state(3101).moving);
        // A press inside the glass activates the selected item.
        r.frame(3200);
        r.click(3200);
        assert_eq!(r.state(3200).activations.len(), 1);
        assert_eq!(r.state(3200).activations[0].item, "btn_options");
        // The second press of a double click does not.
        r.click(3300);
        assert_eq!(r.state(3300).activations.len(), 1);
        // Outside the glass nothing happens.
        r.point(100.0, 100.0, 4000);
        r.click(4000);
        assert_eq!(r.state(4000).activations.len(), 1);
    }

    #[test]
    fn touch_taps_select_then_activate() {
        let mut r = Run::open(0);
        r.frame(1000);
        let tap = |r: &mut Run, now: u64| {
            for phase in [TouchPhase::Start, TouchPhase::End] {
                r.input.handle(Event::Touch {
                    at: [782.0, 419.0 + 120.0 + 15.0],
                    phase,
                    time: now as f64,
                });
            }
            r.frame(now);
        };
        tap(&mut r, 1000);
        let s = r.state(1000);
        assert_eq!((s.selected, s.activations.len()), (4, 0));
        tap(&mut r, 2000);
        assert_eq!(r.state(2000).activations[0].item, "btn_credits");
    }

    #[test]
    fn music_repeats_as_a_stereo_pair() {
        let mut r = Run::open(0);
        let started: Vec<(&str, f32, Category, bool)> = r
            .audio
            .started
            .iter()
            .map(|(_, p, b)| (p.as_str(), b.pan, b.category, b.looping))
            .collect();
        assert_eq!(
            started,
            [
                (
                    "gamedata\\sounds\\music\\wasteland2_l.ogg",
                    -1.0,
                    Category::Music,
                    false
                ),
                (
                    "gamedata\\sounds\\music\\wasteland2_r.ogg",
                    1.0,
                    Category::Music,
                    false
                ),
            ]
        );
        r.frame(100);
        assert_eq!(r.audio.started.len(), 2);
        // Either channel stopping restarts both.
        let right = r.state(100).music.voices.unwrap()[1];
        r.audio.end(right);
        r.frame(200);
        let s = r.state(200);
        assert_eq!(r.audio.started.len(), 4);
        assert_eq!(s.music.starts, 2);
        assert_eq!(r.audio.playing.len(), 2);
        assert_eq!(s.music.track.as_deref(), Some("music\\wasteland2"));
        assert!(r.audio.started[2].1.ends_with("wasteland2_l.ogg"));
    }

    #[test]
    fn cursor_frames_follow_the_clock() {
        let r = Run::open(0);
        for (t, frame) in [(0, 0), (51, 0), (52, 1), (415, 7), (416, 0), (1000, 3)] {
            assert_eq!(r.state(t).cursor_frame, frame, "t = {t}");
            let s = r.scene(XGA, t);
            let TextureId::Image { path } = &s.first("cursor").unwrap().texture else {
                panic!()
            };
            assert!(
                path.ends_with(&format!("ui_ani_cursor_{:02}.dds", frame + 1)),
                "{path}"
            );
        }
    }

    #[test]
    fn scene_at_1024x768() {
        let mut r = Run::open(0);
        r.frame(600);
        let s = r.scene(XGA, 600);
        let layers: Vec<&str> = s
            .sprites
            .iter()
            .map(|s| s.layer.as_str())
            .filter(|l| !l.starts_with("caption") && *l != "version")
            .collect();
        assert_eq!(
            layers,
            [
                "back_movie",
                "background",
                "fire_movie",
                "bar",
                "left_grating",
                "right_grating",
                "left_anim",
                "right_anim",
                "cursor"
            ]
        );
        let one = |l: &str| s.first(l).unwrap();
        assert_eq!(one("back_movie").bounds(), [147.5, -0.5, 1023.5, 399.5]);
        assert_eq!(
            one("back_movie").texture,
            TextureId::Video {
                path: "gamedata\\textures\\ui\\ui_mm_main_back_crop.ogm".into(),
                start_ms: 0,
                looping: true
            }
        );
        assert_eq!(one("background").bounds(), [-0.5, -0.5, 1023.5, 767.5]);
        assert_eq!(one("background").uv_max, [1.0, 0.75]);
        assert_eq!(one("fire_movie").bounds(), [524.5, 458.5, 652.5, 714.5]);
        assert_eq!(one("bar").bounds(), [598.5, 396.5, 990.5, 469.5]);
        assert_eq!(one("left_grating").bounds(), [598.5, 400.5, 644.5, 470.5]);
        assert_eq!(one("right_grating").bounds(), [935.5, 400.5, 990.5, 467.5]);
        // The cursor at the screen centre (its initial position), 64 × 64.
        assert_eq!(one("cursor").bounds(), [511.5, 383.5, 575.5, 447.5]);
        // Captions: centred in their rows with the fixture's 8-pixel glyphs.
        for i in 0..6 {
            let n = r.state(600).items[i].caption.chars().count();
            assert_eq!(s.layer(&format!("caption {i}")).count(), n);
        }
        let w = 10.0 * GLYPH as f32;
        let b = s.layer_bounds("caption 0").unwrap();
        assert_eq!(b, [782.0 - w / 2.0, 423.0, 782.0 + w / 2.0, 445.0]);
        assert_eq!(
            one("caption 0").color,
            volya_engine_formats::ui::Color::rgba(0, 0, 0, 255)
        );
        // "ver. 1.0006": 11 glyphs from (15, 740) in the version colour.
        assert_eq!(
            s.layer_bounds("version").unwrap(),
            [15.0, 740.0, 15.0 + 11.0 * GLYPH as f32, 762.0]
        );
        assert_eq!(
            one("version").color,
            volya_engine_formats::ui::Color::rgba(135, 123, 116, 255)
        );
    }

    #[test]
    fn cog_wheels_rotate_and_narrow_on_wide_screens() {
        let mut r = Run::open(0);
        r.frame(600);
        let a = r.state(600).wheel_angle;
        let s = r.scene(XGA, 600);
        let left = s.first("left_anim").unwrap();
        let right = s.first("right_anim").unwrap();
        // Centres at bar + (43, 37) and (340, 37); rotated by −a and +a.
        let centre = |c: &[[f32; 2]; 4]| [(c[0][0] + c[2][0]) / 2.0, (c[0][1] + c[2][1]) / 2.0];
        assert_eq!(centre(&left.corners), [642.0, 434.0]);
        assert_eq!(centre(&right.corners), [939.0, 434.0]);
        // Section 5.4: corner (0, 0) of a picture turned by +a.
        let turned = |angle: f32| {
            let (sin, cos) = angle.sin_cos();
            let (dx, dy) = (-27.5f32, -27.5f32);
            [dx * cos + dy * sin, dy * cos - dx * sin]
        };
        for (sprite, angle, cx) in [(left, -a, 642.0), (right, a, 939.0)] {
            let t = turned(angle);
            assert!(
                (sprite.corners[0][0] - (cx + t[0])).abs() < 1e-3
                    && (sprite.corners[0][1] - (434.0 + t[1])).abs() < 1e-3
            );
        }
        // Wide: narrowed by 0.8333 around the centre at every angle; after a
        // resize also only 55 × 0.7518797 wide.
        let narrow = |r: &Run| {
            let s = r.scene(HD, 600);
            let c = s.first("right_anim").unwrap().corners;
            ((c[1][0] - c[0][0]).powi(2) + (c[1][1] - c[0][1]).powi(2)).sqrt()
        };
        let before = narrow(&r);
        r.menu.resized(&r.data, HD);
        assert_eq!(r.state(600).wheel_width, 55.0 * WIDE_WHEEL);
        assert!(narrow(&r) < before);
        r.menu.resized(&r.data, XGA);
        assert_eq!(r.state(600).wheel_width, 55.0);
    }

    #[test]
    fn scene_at_1920x1080() {
        let mut r = Run::open(0);
        r.frame(600);
        let s = r.scene(HD, 600);
        assert_eq!(
            s.first("background").unwrap().bounds(),
            [-0.5, -0.5, 1919.5, 1079.5]
        );
        // Bar at (599, 397): floor(599 · 1.875) = 1123, floor(397 · 1.40625) = 558.
        assert_eq!(
            s.first("bar").unwrap().bounds(),
            [
                1122.5,
                557.5,
                1122.5 + 392.0 * 1.875,
                557.5 + 73.0 * 1.40625
            ]
        );
        // Glyphs keep their pixel size; the pen is at the scaled anchor.
        let w = 10.0 * GLYPH as f32;
        let b = s.layer_bounds("caption 0").unwrap();
        let x0 = (782.0f32 * 1.875).floor() - (w / 2.0).floor();
        let y0 = (1.40625f32 * (419.0 + (30.0 - 22.0 / 1.40625) / 2.0)).floor();
        assert_eq!(b, [x0, y0, x0 + w, y0 + 22.0]);
    }
}
