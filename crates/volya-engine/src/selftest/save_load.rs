// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The save and load screens and the quit box reached through the menu
//! during a game (once `tests/save_load.rs`'s drawing;
//! docs/behavior/save-load-screens.md, title-menu.md 4.5), each drawn
//! without errors: the save screen, the save screen with the on-screen
//! keyboard (#94), the load screen and the quit box over the menu. Frames
//! `save`, `keyboard`, `load` and `quit`.

use super::{Context, Outcome, Run, capture, ensure};
use crate::audio::Silent;
use crate::core::console::Console;
use crate::core::input::{Event, GamepadButton, Input, Key, MouseButton};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::menu_screens::{Frame, MenuScreens, MenuState};
use crate::present::Presenter;
use crate::title_menu::{Opening, TitleMenu};
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[
    ("save screen draws", |c| Box::pin(draws(c, Stage::Save))),
    ("on-screen keyboard draws", |c| {
        Box::pin(draws(c, Stage::Keyboard))
    }),
    ("load screen draws", |c| Box::pin(draws(c, Stage::Load))),
    ("quit box draws", |c| Box::pin(draws(c, Stage::Quit))),
];

const XGA: Resolution = Resolution::new(1024, 768);

/// The states drawn, in the order the walk reaches them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Save,
    Keyboard,
    Load,
    Quit,
}

/// The menu during a game with its screens, frame by frame.
struct Walk<'a> {
    cx: &'a Context<'a>,
    data: &'a GameData,
    console: Console,
    screens: MenuScreens,
    input: Input,
    menu: TitleMenu,
    now: u64,
}

impl Walk<'_> {
    fn frame(&mut self, keys: &[Key]) -> MenuState {
        for &key in keys {
            for pressed in [true, false] {
                self.input.handle(Event::Key { key, pressed });
            }
        }
        self.now += 16;
        self.screens.update(
            &mut self.menu,
            Frame {
                tree: self.cx.tree,
                data: self.data,
                console: &mut self.console,
                native: XGA,
                video: XGA,
                now: self.now,
                input: &self.input,
                audio: &mut Silent,
                field_font: None,
            },
        );
        self.input.end_frame();
        self.screens.state(&self.menu, self.data, self.now)
    }

    fn pad(&mut self, button: GamepadButton) {
        for pressed in [true, false] {
            self.input.handle(Event::GamepadButton { button, pressed });
        }
    }

    /// The screens' scene drawn without errors, kept as frame `name`.
    async fn draw(&self, name: &str) -> Outcome {
        let mut assets = Assets::default();
        let mut ctx = Ctx {
            tree: self.cx.tree,
            data: self.data,
            assets: &mut assets,
            resolution: XGA,
            text_surface: crate::draw::Surface::screen(XGA),
            clock_ms: 1000,
        };
        let scene = self
            .screens
            .scene(&self.menu, &mut ctx)
            .map_err(|e| format!("scene: {e}"))?;
        let mut r = self.cx.renderer();
        let mut presenter = Presenter::new();
        let list = presenter.draw_list(&mut r, self.cx.tree, &scene, 1000);
        let errors = presenter.take_errors();
        ensure!(errors.is_empty(), "{errors:?}");
        let pixels = capture(&mut r, &list, XGA.width, XGA.height).await?;
        self.cx.dump(name, XGA.width, XGA.height, &pixels);
        Ok(String::new())
    }
}

/// The screens through the menu during a game: Save (item 3), its
/// keyboard, then Load (item 2), then the quit box; `stage` drawn.
async fn draws(cx: &Context<'_>, stage: Stage) -> Outcome {
    let data = GameData::read(cx.tree, None, XGA).map_err(|e| format!("game data: {e}"))?;
    let input = Input::new();
    let opening = Opening {
        in_game: true,
        last_save: false,
    };
    let menu = TitleMenu::open(&data, 0, &input, &mut Silent, opening);
    let mut walk = Walk {
        cx,
        data: &data,
        console: data.console(),
        screens: MenuScreens::default(),
        input,
        menu,
        now: 0,
    };
    walk.frame(&[Key::Down, Key::Down, Key::Down]);
    let s = walk.frame(&[Key::Enter]);
    let save = s.save_screen.ok_or("the save screen")?;
    if stage == Stage::Save {
        return walk.draw("save").await;
    }
    // A gamepad's south on the name field opens the keyboard; a click on
    // Cancel while it shows doesn't reach the screen.
    let [x, y, w, h] = save.field;
    walk.input.handle(Event::PointerMoved {
        at: [x + w / 2.0, y + h / 2.0],
    });
    walk.pad(GamepadButton::South);
    let s = walk.frame(&[]);
    ensure!(
        s.save_screen
            .is_some_and(|s| s.focused && s.keyboard.is_some()),
        "the keyboard opens"
    );
    walk.pad(GamepadButton::South);
    walk.frame(&[]);
    let cancel = [792.0 + 140.0, 595.0 + 24.0];
    walk.input.handle(Event::PointerMoved { at: cancel });
    for pressed in [true, false] {
        walk.input.handle(Event::PointerButton {
            at: Some(cancel),
            button: MouseButton::Left,
            pressed,
            time: 0.0,
        });
    }
    let s = walk.frame(&[]);
    ensure!(
        s.save_screen.is_some_and(|s| s.keyboard.is_some()),
        "the click is swallowed"
    );
    if stage == Stage::Keyboard {
        return walk.draw("keyboard").await;
    }
    walk.pad(GamepadButton::East);
    walk.frame(&[]);
    walk.frame(&[Key::Escape]);
    let s = walk.frame(&[Key::Up, Key::Enter]);
    ensure!(s.load_screen.is_some(), "the load screen");
    if stage == Stage::Load {
        return walk.draw("load").await;
    }
    walk.frame(&[Key::Escape]);
    for _ in 0..4 {
        walk.frame(&[Key::Down]);
    }
    let s = walk.frame(&[Key::Enter]);
    let message = s.message.ok_or("the quit box")?;
    ensure!(message.name == "message_box_quit_game", "{}", message.name);
    walk.draw("quit").await
}
