// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The inventory screen with a New Game's starting inventory (once
//! `tests/inventory_screen.rs`'s drawing; docs/behavior/inventory-screen.md
//! section 9), drawn without errors: with the outfit's context menu open,
//! and after the outfit is dragged to the backpack and bread eaten with a
//! double click. Frames `inventory` and `inventory-after`.

use super::{Context, Outcome, Run, capture, ensure};
use crate::audio::Silent;
use crate::core::character::CharacterState;
use crate::core::character::inventory::{InventoryItem, ItemConfig};
use crate::core::input::{Event, Input, Key, MouseButton};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::inventory_screen::layout::ListKind;
use crate::inventory_screen::{Frame, InventoryScreen, InventoryState, Keys};
use crate::level;
use crate::present::Presenter;
use crate::scene::Scene;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[
    ("inventory screen draws", |c| {
        Box::pin(draws(c, Stage::Menu))
    }),
    ("inventory screen draws after a drag and bread eaten", |c| {
        Box::pin(draws(c, Stage::After))
    }),
];

const XGA: Resolution = Resolution::new(1024, 768);

/// The states drawn, in the order the walk reaches them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// The outfit's context menu open.
    Menu,
    /// The outfit in the backpack and the bread eaten.
    After,
}

struct Walk<'a> {
    data: &'a GameData,
    screen: InventoryScreen,
    character: CharacterState,
    keys: Keys,
    input: Input,
    now: u64,
}

impl Walk<'_> {
    fn frame(&mut self) -> Result<InventoryState, String> {
        self.now += 16;
        let out = self
            .screen
            .update(Frame {
                data: self.data,
                character: &mut self.character,
                keys: &self.keys,
                time_of_day: 5.5 * 3600.0,
                now: self.now,
                input: &self.input,
                audio: &mut Silent,
                menu_font: None,
            })
            .map_err(|e| format!("update: {e}"))?;
        if out.closed {
            return Err("the screen closed".into());
        }
        self.input.end_frame();
        Ok(self.screen.state(&self.character))
    }

    fn button(&mut self, at: [f32; 2], button: MouseButton, time: f64) {
        for pressed in [true, false] {
            self.input.handle(Event::PointerButton {
                at: Some(at),
                button,
                pressed,
                time,
            });
        }
    }
}

/// The screen's scene drawn without errors, kept as frame `name`.
async fn draw(cx: &Context<'_>, walk: &Walk<'_>, name: &str) -> Outcome {
    let mut assets = Assets::default();
    let mut ctx = Ctx {
        tree: cx.tree,
        data: walk.data,
        assets: &mut assets,
        resolution: XGA,
        text_surface: crate::draw::Surface::screen(XGA),
        clock_ms: 1000,
    };
    let mut scene = Scene::new(XGA);
    walk.screen
        .draw(&mut ctx, &mut scene, &walk.character)
        .map_err(|e| format!("draw: {e}"))?;
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let list = presenter.draw_list(&mut r, cx.tree, &scene, 1000);
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    let pixels = capture(&mut r, &list, XGA.width, XGA.height).await?;
    cx.dump(name, XGA.width, XGA.height, &pixels);
    Ok(String::new())
}

/// A New Game's character on the screen: a right click on the character
/// picture opens the outfit's menu; then a key closes it, the outfit is
/// dragged to the backpack and bread eaten with a double click; `stage`
/// drawn.
async fn draws(cx: &Context<'_>, stage: Stage) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, XGA).map_err(|e| format!("game data: {e}"))?;
    let (_, character) =
        level::new_game_player(tree, &data).map_err(|e| format!("New Game player: {e}"))?;
    let mut screen =
        InventoryScreen::load(tree, &data, XGA).map_err(|e| format!("inventory screen: {e}"))?;
    screen
        .open(
            &data,
            &character,
            5.5 * 3600.0,
            0,
            [512.0, 384.0],
            &mut Silent,
        )
        .map_err(|e| format!("open: {e}"))?;
    let mut walk = Walk {
        data: &data,
        screen,
        character,
        keys: Keys {
            inventory: vec![Key::I],
            drop: vec![Key::G],
            quit: data.quit_keys.clone(),
        },
        input: Input::new(),
        now: 0,
    };
    let s = walk.frame()?;
    let outfit = s.lists[ListKind::Outfit as usize]
        .cells
        .first()
        .ok_or("the outfit")?
        .items[0];
    let at = [900.0, 330.0];
    walk.input.handle(Event::PointerMoved { at });
    walk.button(at, MouseButton::Right, 0.0);
    let s = walk.frame()?;
    ensure!(s.menu.is_some(), "the outfit's menu");
    if stage == Stage::Menu {
        return draw(cx, &walk, "inventory").await;
    }
    for pressed in [true, false] {
        walk.input.handle(Event::Key {
            key: Key::LeftShift,
            pressed,
        });
    }
    walk.frame()?;
    walk.input.handle(Event::PointerButton {
        at: Some(at),
        button: MouseButton::Left,
        pressed: true,
        time: 1000.0,
    });
    walk.frame()?;
    walk.input
        .handle(Event::PointerMoved { at: [905.0, 335.0] });
    walk.frame()?;
    let drop_at = [100.0, 300.0];
    walk.input.handle(Event::PointerMoved { at: drop_at });
    walk.input.handle(Event::PointerButton {
        at: Some(drop_at),
        button: MouseButton::Left,
        pressed: false,
        time: 1300.0,
    });
    walk.frame()?;
    ensure!(
        walk.character.inventory.worn_outfit().is_none(),
        "the outfit {outfit} is still worn"
    );
    let bread = InventoryItem::create(
        ItemConfig::read(&data.system, "bread").map_err(|e| format!("bread: {e}"))?,
        900,
        None,
        0,
        "actor",
    );
    walk.character.inventory.take(bread);
    let s = walk.frame()?;
    let cell = s.lists[ListKind::Bag as usize]
        .cells
        .iter()
        .find(|c| c.section == "bread")
        .ok_or("the bread rebuilt into the backpack")?;
    let at = [cell.rect[0] + 5.0, cell.rect[1] + 5.0];
    walk.input.handle(Event::PointerMoved { at });
    for time in [2000.0, 2100.0] {
        walk.button(at, MouseButton::Left, time);
    }
    walk.frame()?;
    ensure!(
        walk.character.inventory.item(900).is_none(),
        "the bread is eaten"
    );
    draw(cx, &walk, "inventory-after").await
}
