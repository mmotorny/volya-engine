// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The menu with what it opens, on the title screen and during a game
//! (docs/behavior/title-menu.md sections 2 and 4.5,
//! docs/behavior/save-load-screens.md sections 1, 2 and 12): the item list,
//! its sub-screens (options, save, load; one shows at a time, the list is
//! hidden meanwhile) and its own message boxes (the quit boxes, the last
//! save's confirmation).
//!
//! Each sub-screen is made on its first opening after the menu opened and
//! kept until the menu closes ([`MenuScreens::menu_opened`]).

use std::io;

use serde::Serialize;

use crate::audio::AudioOut;
use crate::core::console::Console;
use crate::core::input::{Control, Input};
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::dialog::{self, Answer, Ev, MessageBox, MessageBoxState};
use crate::draw::Ctx;
use crate::options::{OptionsData, OptionsScreen, OptionsState};
use crate::save_load::{
    CONFIRM_LOAD_SAVE, FieldFont, LoadScreen, LoadScreenState, QUIT_GAME, QUIT_GAME_TEXT,
    QUIT_WINDOWS, SaveLoadData, SaveScreen, SaveScreenState,
};
use crate::saved_games::SavedGames;
use crate::scene::Scene;
use crate::title_menu::{Request, TitleMenu, TitleMenuState};
use volya_engine_formats::ui::Resolution;

/// The sub-screen shown instead of the item list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Sub {
    Options,
    Save,
    Load,
}

/// What a box over the item list is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuBox {
    QuitWindows,
    QuitGame,
    LastSave,
}

/// What the menu's frame asks of the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuFrame {
    /// A new game of this difficulty (title-menu.md section 4.5).
    pub new_game: Option<&'static str>,
    /// `main_menu off`: back to the game.
    pub return_to_game: bool,
    /// `disconnect`: the game ends, the title screen's menu opens.
    pub disconnect: bool,
    /// The options screen closed with a video restart marked.
    pub video_restart: bool,
    /// The options screen closed: the settings are to be stored.
    pub store_config: bool,
}

/// What a frame needs besides the menu.
pub struct Frame<'a> {
    pub tree: &'a dyn Vfs,
    pub data: &'a GameData,
    pub console: &'a mut Console,
    /// The screen's own size (the resolution list).
    pub native: Resolution,
    /// The resolution drawn at (layouts' `_16` rule).
    pub video: Resolution,
    pub now: u64,
    pub input: &'a Input,
    pub audio: &'a mut dyn AudioOut,
    /// The save screen's name-field font, while it shows.
    pub field_font: Option<&'a FieldFont>,
}

#[derive(Default)]
pub struct MenuScreens {
    options_data: Option<OptionsData>,
    options: Option<OptionsScreen>,
    data: Option<SaveLoadData>,
    save: Option<SaveScreen>,
    load: Option<LoadScreen>,
    sub: Option<Sub>,
    message: Option<(MessageBox, MenuBox)>,
    saves: SavedGames,
    /// Files deleted from the saved-games folder, for the platform.
    deleted: Vec<String>,
    /// Why the latest screen could not open.
    error: Option<String>,
    /// The previous frame's time (the left stick's cursor speed).
    last_now: Option<u64>,
    /// The time of the last hold step of any scroll bar, 0 at program
    /// start (save-load-screens.md section 3.5).
    last_hold: u64,
}

impl MenuScreens {
    pub fn saves(&self) -> &SavedGames {
        &self.saves
    }

    pub fn set_saves(&mut self, saves: SavedGames) {
        self.saves = saves;
    }

    /// The files deleted since the last call.
    pub fn take_deleted(&mut self) -> Vec<String> {
        std::mem::take(&mut self.deleted)
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    /// The menu (re)opened: its screens are made afresh when next opened
    /// (save-load-screens.md section 12).
    pub fn menu_opened(&mut self) {
        self.options = None;
        self.save = None;
        self.load = None;
        self.sub = None;
        self.message = None;
    }

    pub fn sub(&self) -> Option<Sub> {
        self.sub
    }

    /// The save screen's name-field font, while it shows.
    pub fn field_font(&self) -> Option<volya_engine_formats::text::font::UiFont> {
        match self.sub {
            Some(Sub::Save) => self.save.as_ref().map(SaveScreen::field_font),
            _ => None,
        }
    }

    fn save_load(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        video: Resolution,
    ) -> io::Result<&SaveLoadData> {
        if self.data.is_none() {
            self.data = Some(SaveLoadData::load(tree, data, video)?);
        }
        Ok(self.data.as_ref().expect("loaded"))
    }

    /// One frame of `menu` and what shows over or instead of it.
    pub fn update(&mut self, menu: &mut TitleMenu, f: Frame) -> MenuFrame {
        let mut out = MenuFrame::default();
        let Frame {
            tree,
            data,
            console,
            native,
            video,
            now,
            input,
            audio,
            field_font,
        } = f;
        let none = Input::new();
        match self.sub {
            Some(Sub::Options) => {
                // The menu is hidden meanwhile (title-menu.md 4.5); its music
                // plays on, not restarted (section 6).
                let screen = self.options.as_mut().expect("open options");
                screen.update(data, console, native, now, input);
                menu.update_hidden(data, now);
                if let Some(closed) = screen.take_closed() {
                    self.sub = None;
                    menu.set_cursor(data, input.cursor_ui());
                    out.video_restart = closed.video_restart;
                    out.store_config = true;
                }
                return out;
            }
            Some(Sub::Save) => {
                let (Some(sl), Some(screen)) = (self.data.as_ref(), self.save.as_mut()) else {
                    self.sub = None;
                    return out;
                };
                let o = screen.update(
                    data,
                    sl,
                    &mut self.saves,
                    field_font,
                    now,
                    input,
                    &mut self.last_hold,
                );
                self.deleted.extend(o.deleted);
                menu.update_hidden(data, now);
                if o.closed {
                    self.sub = None;
                    menu.set_cursor(data, screen.cursor());
                }
                return out;
            }
            Some(Sub::Load) => {
                let (Some(sl), Some(screen)) = (self.data.as_ref(), self.load.as_mut()) else {
                    self.sub = None;
                    return out;
                };
                let o = screen.update(
                    tree,
                    data,
                    sl,
                    &mut self.saves,
                    now,
                    input,
                    &mut self.last_hold,
                );
                self.deleted.extend(o.deleted);
                menu.update_hidden(data, now);
                if o.closed {
                    self.sub = None;
                    menu.set_cursor(data, screen.cursor());
                }
                return out;
            }
            None => {}
        }
        let dt = now.saturating_sub(self.last_now.unwrap_or(now)).min(100) as f32 / 1000.0;
        self.last_now = Some(now);
        if let Some((m, purpose)) = &mut self.message {
            // The box takes the input; the menu behind it animates.
            let mut cursor = menu.cursor();
            let mut answer = None;
            for mut e in dialog::events(data, input, cursor, dt) {
                if let Ev::Navigate(d) = e
                    && let Some(p) = dialog::navigate(cursor, d, &m.targets())
                {
                    e = Ev::Move(p);
                }
                if let Ev::Move(p) = e {
                    cursor = p;
                }
                if let Some(a) = m.event(e, cursor) {
                    answer = Some(a);
                    break;
                }
            }
            m.frame(cursor);
            let purpose = *purpose;
            menu.set_cursor(data, cursor);
            menu.update(data, now, &none, audio);
            if let Some(a) = answer {
                self.message = None;
                if a == Answer::Yes {
                    match purpose {
                        // `quit`: the game exits.
                        MenuBox::QuitWindows => menu.quit(audio),
                        MenuBox::QuitGame => out.disconnect = true,
                        // TODO(#75): `main_menu off`, then `load_last_save`.
                        MenuBox::LastSave => {}
                    }
                }
            }
            return out;
        }
        // During a game the key bound to `quit` toggles the menu
        // (save-load-screens.md section 2): on its main page it closes it.
        let quit_key = input.device_changes().iter().any(|&(c, pressed)| {
            pressed && matches!(c, Control::Key(k) if data.quit_keys.contains(&k))
        });
        if menu.in_game() && quit_key {
            out.return_to_game = true;
            return out;
        }
        menu.update(data, now, input, audio);
        out.new_game = menu.take_new_game();
        let Some(request) = menu.take_request() else {
            return out;
        };
        let cursor = menu.cursor();
        let opened = match request {
            Request::Return => {
                out.return_to_game = true;
                Ok(())
            }
            Request::Options => self.open_options(tree, data, console, native, video, now, cursor),
            Request::Save | Request::Load => self.open_save_load(
                tree,
                data,
                video,
                now,
                cursor,
                request == Request::Save,
                menu.in_game(),
            ),
            Request::QuitWindows => self.show_box(
                (tree, data, video),
                QUIT_WINDOWS,
                None,
                MenuBox::QuitWindows,
                cursor,
            ),
            // Title-menu 4.5: the single-player text.
            Request::QuitGame => self.show_box(
                (tree, data, video),
                QUIT_GAME,
                Some(QUIT_GAME_TEXT),
                MenuBox::QuitGame,
                cursor,
            ),
            Request::LastSave if menu.in_game() => self.show_box(
                (tree, data, video),
                CONFIRM_LOAD_SAVE,
                None,
                MenuBox::LastSave,
                cursor,
            ),
            // TODO(#75): `main_menu off`, then `load_last_save`.
            Request::LastSave => Ok(()),
        };
        match opened {
            Ok(()) => self.error = None,
            // Back on the menu, saying why (our choice).
            Err(e) => self.error = Some(e.to_string()),
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn open_options(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        console: &mut Console,
        native: Resolution,
        video: Resolution,
        now: u64,
        cursor: [f32; 2],
    ) -> io::Result<()> {
        let context = |e: io::Error| io::Error::new(e.kind(), format!("options: {e}"));
        if self.options_data.is_none() {
            self.options_data = Some(OptionsData::load(tree, data, video).map_err(context)?);
        }
        if self.options.is_none() {
            let opt = self.options_data.as_ref().expect("loaded");
            self.options = Some(OptionsScreen::new(data, opt).map_err(context)?);
        }
        let screen = self.options.as_mut().expect("made");
        screen.open(data, console, native, now, cursor);
        self.sub = Some(Sub::Options);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn open_save_load(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        video: Resolution,
        now: u64,
        cursor: [f32; 2],
        save: bool,
        in_game: bool,
    ) -> io::Result<()> {
        let context = |e: io::Error| {
            let what = if save { "save" } else { "load" };
            io::Error::new(e.kind(), format!("{what}: {e}"))
        };
        self.save_load(tree, data, video).map_err(context)?;
        let sl = self.data.as_ref().expect("loaded");
        if save {
            let saves = &self.saves;
            let s = self.save.get_or_insert_with(|| SaveScreen::new(sl, saves));
            s.open(now, cursor);
            self.sub = Some(Sub::Save);
        } else {
            let saves = &self.saves;
            let s = self
                .load
                .get_or_insert_with(|| LoadScreen::new(sl, saves, in_game));
            s.open(now, cursor);
            self.sub = Some(Sub::Load);
        }
        Ok(())
    }

    /// Shows box `name`, with text key `text` in place of the data's.
    fn show_box(
        &mut self,
        (tree, data, video): (&dyn Vfs, &GameData, Resolution),
        name: &str,
        text: Option<&str>,
        purpose: MenuBox,
        cursor: [f32; 2],
    ) -> io::Result<()> {
        let mut desc = self.save_load(tree, data, video)?.message_box(data, name)?;
        if let Some(key) = text {
            desc = desc.with_text(key);
        }
        self.message = Some((MessageBox::new(desc, cursor), purpose));
        Ok(())
    }

    /// The frame: the shown sub-screen, or the menu with its box over it.
    pub fn scene(&self, menu: &TitleMenu, ctx: &mut Ctx) -> io::Result<Scene> {
        match self.sub {
            Some(Sub::Options) => self.options.as_ref().expect("open options").scene(ctx),
            Some(Sub::Save) => self.save.as_ref().expect("open save").scene(ctx),
            Some(Sub::Load) => self.load.as_ref().expect("open load").scene(ctx),
            None => {
                let mut scene = menu.scene(ctx)?;
                if let Some((m, _)) = &self.message {
                    // The cursor stays on top.
                    let cursor = scene.sprites.iter().rposition(|s| s.layer == "cursor");
                    let tail = cursor.map(|i| scene.sprites.split_off(i));
                    m.draw(ctx, &mut scene)?;
                    scene.sprites.extend(tail.into_iter().flatten());
                }
                Ok(scene)
            }
        }
    }

    pub fn state(&self, menu: &TitleMenu, data: &GameData, now: u64) -> MenuState {
        MenuState {
            menu: menu.state(data, now),
            options: self
                .options
                .as_ref()
                .filter(|_| self.sub == Some(Sub::Options))
                .map(|o| Box::new(o.state(data))),
            save_screen: self
                .save
                .as_ref()
                .filter(|_| self.sub == Some(Sub::Save))
                .map(|s| Box::new(s.state())),
            load_screen: self
                .load
                .as_ref()
                .filter(|_| self.sub == Some(Sub::Load))
                .map(|s| Box::new(s.state())),
            message: self.message.as_ref().map(|(m, _)| m.state()),
            error: self.error.clone(),
        }
    }
}

/// The menu as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct MenuState {
    /// The item list.
    pub menu: TitleMenuState,
    /// The options screen while it shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Box<OptionsState>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_screen: Option<Box<SaveScreenState>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_screen: Option<Box<LoadScreenState>>,
    /// The box over the item list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<MessageBoxState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::input::{Event, GamepadButton, Key, MouseButton, TouchPhase};
    use crate::fixture::{self, FakeAudio};
    use crate::save_load::Stubbed;
    use crate::saved_games::{LocalTime, SavedFile};
    use crate::title_menu::Opening;

    const XGA: Resolution = Resolution::new(1024, 768);

    struct Run {
        tree: fixture::MemVfs,
        data: GameData,
        console: Console,
        screens: MenuScreens,
        menu: TitleMenu,
        input: Input,
        audio: FakeAudio,
        now: u64,
    }

    impl Run {
        fn new(in_game: bool) -> Self {
            let tree = fixture::tree();
            let data = fixture::data(&tree);
            let mut audio = FakeAudio::default();
            let opening = Opening {
                in_game,
                last_save: false,
            };
            let menu = TitleMenu::open(&data, 0, &Input::new(), &mut audio, opening);
            Self {
                console: data.console(),
                tree,
                data,
                screens: MenuScreens::default(),
                menu,
                input: Input::new(),
                audio,
                now: 0,
            }
        }

        fn frame(&mut self) -> MenuFrame {
            self.now += 16;
            let out = self.screens.update(
                &mut self.menu,
                Frame {
                    tree: &self.tree,
                    data: &self.data,
                    console: &mut self.console,
                    native: XGA,
                    video: XGA,
                    now: self.now,
                    input: &self.input,
                    audio: &mut self.audio,
                    field_font: None,
                },
            );
            self.input.end_frame();
            out
        }

        fn key(&mut self, key: Key) -> MenuFrame {
            for pressed in [true, false] {
                self.input.handle(Event::Key { key, pressed });
            }
            self.frame()
        }

        fn item(&mut self, i: usize) -> MenuFrame {
            for _ in 0..i {
                self.key(Key::Down);
            }
            self.key(Key::Enter)
        }

        /// A tap at UI point `at`.
        fn tap(&mut self, at: [f32; 2]) -> MenuFrame {
            for phase in [TouchPhase::Start, TouchPhase::End] {
                self.input.handle(Event::Touch {
                    at,
                    phase,
                    time: self.now as f64 * 100.0,
                });
            }
            self.frame()
        }

        fn pad(&mut self, button: GamepadButton) {
            for pressed in [true, false] {
                self.input.handle(Event::GamepadButton { button, pressed });
                self.frame();
            }
        }

        fn state(&self) -> MenuState {
            self.screens.state(&self.menu, &self.data, self.now)
        }

        fn scene(&self) -> Scene {
            let mut assets = fixture::assets();
            let mut ctx = Ctx {
                tree: &self.tree,
                data: &self.data,
                assets: &mut assets,
                resolution: XGA,
                clock_ms: self.now,
            };
            self.screens.scene(&self.menu, &mut ctx).unwrap()
        }
    }

    fn centre([x, y, w, h]: [f32; 4]) -> [f32; 2] {
        [x + w / 2.0, y + h / 2.0]
    }

    #[test]
    fn return_and_escape_close_the_in_game_menu() {
        let mut r = Run::new(true);
        assert!(r.item(0).return_to_game, "Возврат к игре");
        let mut r = Run::new(true);
        assert!(r.key(Key::Escape).return_to_game, "Escape on the main page");
    }

    #[test]
    fn quitting_the_game_asks_first() {
        let mut r = Run::new(true);
        assert_eq!(r.item(6), MenuFrame::default());
        let message = r.state().message.expect("the box");
        assert_eq!(message.name, "message_box_quit_game");
        assert_eq!(message.text, "ui_mm_quit_game_message");
        assert_eq!(message.buttons[0].rect, [450.0, 404.0, 117.0, 29.0]);
        assert_eq!(message.buttons[1].rect, [570.0, 404.0, 117.0, 29.0]);
        let scene = r.scene();
        let layers: Vec<&str> = scene.sprites.iter().map(|s| s.layer.as_str()).collect();
        assert!(layers.contains(&"message") && layers.contains(&"message yes"));
        assert_eq!(layers.last(), Some(&"cursor"), "the cursor on top");
        // Keys do nothing to a box; the menu behind gets none.
        r.key(Key::Enter);
        r.key(Key::Escape);
        assert!(r.state().message.is_some());
        assert_eq!(r.state().menu.selected, 6);
        // "Нет" closes it; the menu stays.
        assert_eq!(r.tap(centre(message.buttons[1].rect)), MenuFrame::default());
        assert!(r.state().message.is_none());
        // "Да" disconnects.
        r.key(Key::Enter);
        let yes = centre(r.state().message.unwrap().buttons[0].rect);
        assert!(r.tap(yes).disconnect);
    }

    #[test]
    fn quitting_to_windows_asks_first_on_the_title_screen_too() {
        let mut r = Run::new(false);
        r.item(5);
        let message = r.state().message.expect("the box");
        assert_eq!(message.name, "message_box_quit_windows");
        assert!(!r.menu.quit_requested());
        // A gamepad reaches the buttons: the d-pad moves the cursor to the
        // nearest one that way, south clicks it.
        r.pad(GamepadButton::DpadDown);
        r.pad(GamepadButton::South);
        assert!(r.menu.quit_requested(), "{:?}", r.state().message);
    }

    #[test]
    fn the_load_screen_lists_no_saves_and_escape_returns() {
        let mut r = Run::new(true);
        r.item(2);
        let load = r.state().load_screen.expect("the load screen");
        assert!(load.screen.list.rows.is_empty());
        assert!(!load.screen.list.scroll_bar);
        assert_eq!(load.screen.list.rect, [490.0, 399.0, 429.0, 180.0]);
        assert_eq!(load.screen.list.visible, 10);
        let scene = r.scene();
        for layer in [
            "back_video",
            "background",
            "newspaper_video",
            "form",
            "picture",
            "file_info",
            "caption",
            "btn action",
            "cursor",
        ] {
            assert!(scene.first(layer).is_some(), "{layer}");
        }
        assert!(scene.first("bar").is_none(), "the menu is hidden");
        // Load and Delete do nothing without a selection.
        r.tap(centre(load.screen.buttons[0].rect));
        r.tap(centre(load.screen.buttons[1].rect));
        let s = r.state().load_screen.unwrap();
        assert!(s.screen.message.is_none() && s.screen.stubbed.is_empty());
        // Esc returns to the menu, which kept its selection.
        r.key(Key::Escape);
        let s = r.state();
        assert!(s.load_screen.is_none());
        assert_eq!(s.menu.selected, 2);
        // Cancel too.
        r.key(Key::Enter);
        let cancel = centre(r.state().load_screen.unwrap().screen.buttons[2].rect);
        r.tap(cancel);
        assert!(r.state().load_screen.is_none());
    }

    #[test]
    fn the_save_screen_asks_for_a_name_and_saving_is_stubbed() {
        let mut r = Run::new(true);
        r.item(3);
        let s = r.state().save_screen.expect("the save screen");
        assert_eq!(s.screen.list.rect, [490.0, 274.0, 429.0, 287.0]);
        assert_eq!(s.screen.list.visible, 15);
        let save = centre(s.screen.buttons[0].rect);
        // An empty name: "Имя файла пустое" (OK).
        r.tap(save);
        let message = r.state().save_screen.unwrap().screen.message.unwrap();
        assert_eq!(message.name, "message_box_empty_file_name");
        r.tap(centre(message.buttons[0].rect));
        assert!(r.state().save_screen.unwrap().screen.message.is_none());
        // Typing needs the field's focus.
        r.key(Key::A);
        assert_eq!(r.state().save_screen.unwrap().name, "");
        r.tap([500.0, 234.0]);
        for k in [Key::M, Key::Y, Key::Digit1] {
            r.key(k);
        }
        let s = r.state().save_screen.unwrap();
        assert!(s.focused);
        assert_eq!(s.name, "my1");
        // Saving is not built yet (#75): nothing is written, the screen
        // stays.
        r.tap(save);
        let s = r.state().save_screen.unwrap();
        assert_eq!(s.screen.stubbed, [Stubbed::Save("my1".into())]);
        assert!(r.screens.take_deleted().is_empty());
        // Esc clears the focused field and returns to the menu.
        r.key(Key::Escape);
        assert!(r.state().save_screen.is_none());
        // The screen is kept within one menu opening (section 12) and made
        // anew at the next; the field was cleared.
        r.key(Key::Enter);
        let s = r.state().save_screen.unwrap();
        assert_eq!(s.screen.stubbed.len(), 1);
        assert_eq!(s.name, "", "Esc cleared the field");
        r.key(Key::Escape);
        r.screens.menu_opened();
        r.key(Key::Enter);
        assert!(r.state().save_screen.unwrap().screen.stubbed.is_empty());
    }

    #[test]
    fn a_name_with_a_forbidden_character_returns_without_saving() {
        let mut r = Run::new(true);
        r.item(3);
        r.tap([500.0, 234.0]);
        for (key, pressed) in [
            (Key::LeftShift, true),
            (Key::Slash, true),
            (Key::Slash, false),
            (Key::LeftShift, false),
        ] {
            r.input.handle(Event::Key { key, pressed });
        }
        r.frame();
        assert_eq!(r.state().save_screen.unwrap().name, "?");
        let save = centre(r.state().save_screen.unwrap().screen.buttons[0].rect);
        r.tap(save);
        assert!(r.state().save_screen.is_none(), "back to the menu");
    }

    #[test]
    fn deleting_and_overwriting_ask_first() {
        let mut r = Run::new(true);
        let file = |path: &str, head: Vec<u8>| SavedFile {
            path: path.into(),
            modified_ms: 2.0,
            modified: LocalTime::default(),
            head,
        };
        r.screens.set_saves(SavedGames::new(
            vec![
                file("Alpha.sav", vec![0xff, 0xff, 0xff, 0xff, 3, 0, 0, 0]),
                file("alpha.dds", Vec::new()),
            ],
            None,
        ));
        r.item(3);
        // A row click (the mouse over it first) puts its name in the field.
        r.input.handle(Event::PointerMoved {
            at: [600.0, 274.0 + 9.0],
        });
        r.frame();
        for pressed in [true, false] {
            r.input.handle(Event::PointerButton {
                at: None,
                button: MouseButton::Left,
                pressed,
                time: 0.0,
            });
        }
        r.frame();
        let s = r.state().save_screen.unwrap();
        assert_eq!(s.screen.list.rows[0].time, "[00:00:0000 00:00]");
        assert_eq!(
            (s.name.as_str(), s.screen.list.selected),
            ("alpha", Some(0))
        );
        // Saving under an existing name asks to overwrite; "Нет" closes.
        r.tap(centre(s.screen.buttons[0].rect));
        let m = r.state().save_screen.unwrap().screen.message.unwrap();
        assert_eq!(m.name, "message_box_file_already_exist");
        r.tap(centre(m.buttons[1].rect));
        // Delete asks; "Да" deletes the save and its picture.
        r.tap(centre(s.screen.buttons[1].rect));
        let m = r.state().save_screen.unwrap().screen.message.unwrap();
        assert_eq!(m.name, "message_box_delete_file_name");
        r.tap(centre(m.buttons[0].rect));
        let s = r.state().save_screen.unwrap();
        assert!(s.screen.list.rows.is_empty() && s.screen.list.selected.is_none());
        assert_eq!(r.screens.take_deleted(), ["Alpha.sav", "alpha.dds"]);
        assert!(r.screens.saves().files().is_empty());
    }

    #[test]
    fn a_tapped_row_puts_its_name_in_the_field() {
        let mut r = Run::new(true);
        let file = |path: &str, ms: f64| SavedFile {
            path: path.into(),
            modified_ms: ms,
            modified: LocalTime::default(),
            head: Vec::new(),
        };
        r.screens.set_saves(SavedGames::new(
            vec![file("first.sav", 2.0), file("second.sav", 1.0)],
            None,
        ));
        r.item(3);
        // No hover before: the tap moves, presses and releases at once.
        r.tap([600.0, 274.0 + 18.0 + 9.0]);
        let s = r.state().save_screen.unwrap();
        assert_eq!(
            (s.name.as_str(), s.screen.list.selected),
            ("second", Some(1))
        );
        assert_eq!(s.screen.list.focused, Some(1));
    }

    #[test]
    fn the_quit_key_closes_the_in_game_menu_even_rebound() {
        let mut r = Run::new(true);
        r.data.quit_keys = vec![Key::F10];
        assert!(r.key(Key::F10).return_to_game);
        // On the title screen it does nothing.
        let mut r = Run::new(false);
        r.data.quit_keys = vec![Key::F10];
        assert_eq!(r.key(Key::F10), MenuFrame::default());
    }

    #[test]
    fn the_in_game_last_save_item_asks_then_does_nothing_yet() {
        let mut r = Run::new(true);
        assert_eq!(r.state().menu.items[1].name, "btn_lastsave");
        r.item(1);
        let m = r.state().message.expect("the box");
        assert_eq!(m.name, "message_box_confirm_load_save");
        // TODO(#75): "Да" loads the last save; nothing happens yet.
        assert_eq!(r.tap(centre(m.buttons[0].rect)), MenuFrame::default());
        assert!(r.state().message.is_none());
    }

    #[test]
    fn title_music_does_not_restart_under_a_sub_screen() {
        let mut r = Run::new(false);
        r.item(1);
        assert!(r.state().load_screen.is_some());
        let voices = r.state().menu.music.voices.unwrap();
        r.audio.end(voices[0]);
        r.frame();
        assert_eq!(r.state().menu.music.starts, 1, "not under the load screen");
        r.key(Key::Escape);
        r.frame();
        assert_eq!(r.state().menu.music.starts, 2, "again on the menu");
    }

    #[test]
    fn options_open_from_the_in_game_menu() {
        // The fixture has no options layout: the menu stays, saying why.
        let mut r = Run::new(true);
        r.item(4);
        let s = r.state();
        assert!(s.options.is_none());
        assert!(s.error.unwrap().contains("ui_mm_opt.xml"));
    }
}
