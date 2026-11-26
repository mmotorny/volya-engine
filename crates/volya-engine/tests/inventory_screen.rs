// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The inventory screen built from the Russian edition (BYOA), checked
//! against docs/behavior/inventory-screen.md section 9: both layouts'
//! rectangles, the strings, textures, texture ids and sounds resolve, every
//! item section's icon region, and a New Game's starting inventory drawn in
//! its places (the outfit's picture, the weight line, money, bars and
//! protection lines, the outfit's context menu), then a drag of the outfit
//! to the backpack and bread eaten with a double click. Its drawing is the
//! render-screens suite's (`selftest::inventory_screen`). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::audio::{AudioOut, Voice};
use volya_engine::core::audio::{Category, Playback};
use volya_engine::core::character::CharacterState;
use volya_engine::core::character::inventory::{InventoryItem, ItemConfig, Place};
use volya_engine::core::input::{Event, Input, MouseButton};
use volya_engine::core::ui;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::data::{Assets, GameData};
use volya_engine::draw::Ctx;
use volya_engine::inventory_screen::layout::{Layout, ListKind};
use volya_engine::inventory_screen::{Frame, InventoryScreen, InventoryState, Keys};
use volya_engine::level;
use volya_engine::scene::Scene;
use volya_engine_formats::ui::Resolution;

const XGA: Resolution = Resolution::new(1024, 768);
const WIDE: Resolution = Resolution::new(1280, 720);

/// Records the sounds played.
#[derive(Default)]
struct Sounds(Vec<String>);

impl AudioOut for Sounds {
    fn play(&mut self, path: &str, _: Playback) -> Option<Voice> {
        self.0.push(path.to_owned());
        Some(self.0.len() as Voice)
    }
    fn stop(&mut self, _: Voice) {}
    fn is_playing(&self, _: Voice) -> bool {
        false
    }
    fn set_master_gain(&mut self, _: f32) {}
    fn set_volume(&mut self, _: Category, _: f32) {}
}

fn rect(l: &Layout, k: ListKind) -> [f32; 4] {
    let r = l.list(k).rect;
    [r.x, r.y, r.width, r.height]
}

#[test]
fn inventory_screen_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let tree: &dyn Vfs = &tree;
    let data = GameData::read(tree, None, XGA).unwrap();

    // Section 9: the lists' absolute rectangles and grids, both variants.
    let normal = Layout::load(tree, &data, XGA).unwrap();
    assert_eq!(rect(&normal, ListKind::Bag), [37.0, 273.0, 310.0, 442.0]);
    assert_eq!(rect(&normal, ListKind::Belt), [645.0, 136.0, 410.0, 60.0]);
    assert_eq!(
        rect(&normal, ListKind::Outfit),
        [858.0, 290.0, 104.0, 260.0]
    );
    assert_eq!(rect(&normal, ListKind::Pistol), [38.0, 118.0, 156.0, 104.0]);
    assert_eq!(
        rect(&normal, ListKind::Automatic),
        [237.0, 118.0, 312.0, 104.0]
    );
    let bag = normal.list(ListKind::Bag);
    assert_eq!(
        (
            bag.cols,
            bag.rows,
            bag.cell,
            bag.unlimited,
            bag.group_similar
        ),
        (7, 10, (41.0, 41.0), true, true)
    );
    assert!(!normal.list(ListKind::Outfit).custom_placement);
    assert!(normal.list(ListKind::Belt).custom_placement);
    // Section 3.1: parents.
    assert_eq!(normal.bars[0].rect.x, 747.0);
    assert_eq!(normal.bars[0].rect.y, 362.0);
    assert_eq!(
        (normal.time_str.rect.x, normal.time_str.rect.y),
        (130.0, 737.0)
    );
    assert_eq!(
        (normal.frame_title.rect.x, normal.frame_title.rect.y),
        (810.0, 252.0)
    );
    assert_eq!((normal.outfit_view.x, normal.outfit_view.y), (750.0, 583.0));
    assert_eq!(normal.exit.rect.x, 870.0);
    let wide = Layout::load(tree, &data, WIDE).unwrap();
    assert_eq!(rect(&wide, ListKind::Bag), [33.0, 286.0, 295.0, 415.0]);
    let wbag = wide.list(ListKind::Bag);
    assert_eq!((wbag.cols, wbag.rows, wbag.cell), (9, 10, (31.0, 41.0)));
    assert_eq!(rect(&wide, ListKind::Belt), [600.0, 136.0, 225.0, 60.0]);
    assert_eq!(wide.exit.rect.x, 850.0);

    // Every texture, id and string the screen draws resolves; the sounds
    // exist (section 7).
    for layout in [&normal, &wide] {
        let (textures, strings) = layout.references();
        for id in &textures {
            let file = match data.descriptions.get(id) {
                Some(d) => d.file.clone(),
                None => id.clone(),
            };
            ui::texture_source(tree, &data.aliases, &file)
                .unwrap_or_else(|e| panic!("texture {id}: {e}"));
        }
        let missing: Vec<&String> = strings
            .iter()
            .filter(|k| !data.strings.contains(k))
            .collect();
        assert!(missing.is_empty(), "missing strings {missing:?}");
        for sound in layout.sounds.all() {
            let path = data
                .aliases
                .path("$game_sounds$", &format!("{sound}.ogg"))
                .unwrap();
            tree.open(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        }
    }
    for (key, text) in [
        ("ui_inv_weight", "Вес"),
        ("st_condition", "состояние"),
        ("ui_inv_exit", "Выход"),
        ("st_drop_all", "выбросить все"),
        ("st_undress_outfit", "снять костюм"),
        ("st_drop", "выбросить"),
        ("st_eat", "съесть"),
    ] {
        assert_eq!(data.strings.get(key), text, "{key}");
    }
    // Texture ids of section 9.
    for (id, file, r) in [
        (
            "ui_slots_belt",
            "ui\\ui_inventory2",
            [0.0, 749.0, 1024.0, 172.0],
        ),
        (
            "ui_inventory_main",
            "ui\\ui_inventory",
            [0.0, 0.0, 1005.0, 483.0],
        ),
        (
            "ui_scale_red_1",
            "ui\\ui_inventory2",
            [307.0, 41.0, 9.0, 128.0],
        ),
        (
            "ui_button_main02_e",
            "ui\\ui_common",
            [0.0, 662.0, 157.0, 48.0],
        ),
        (
            "ui_mg_progress_efficiency_full",
            "ui\\ui_hud",
            [161.0, 381.0, 120.0, 17.0],
        ),
        (
            "npc_icon_without_outfit",
            "ui\\ui_icons_npc",
            [384.0, 384.0, 128.0, 320.0],
        ),
        (
            "npc_icon_novice_outfit",
            "ui\\ui_icons_npc",
            [0.0, 704.0, 128.0, 320.0],
        ),
    ] {
        let d = data.descriptions.get(id).unwrap_or_else(|| panic!("{id}"));
        assert!(d.file.eq_ignore_ascii_case(file), "{id}: {}", d.file);
        assert_eq!([d.rect.x, d.rect.y, d.rect.width, d.rect.height], r, "{id}");
    }

    // Section 9: the icon regions of every item section with the grid keys
    // lie in the 1024 × 2048 icon texture but the Colt's.
    let mut assets = Assets::default();
    let icons = assets
        .texture(tree, &data, "ui\\ui_icon_equipment", 0)
        .unwrap();
    assert_eq!(icons.size(), (1024, 2048));
    // Every section with the four keys as integers counts, whether or not
    // the rest of it reads as an item; and the item reader agrees.
    let mut sections = 0;
    let mut outside = Vec::new();
    let int = |s: volya_engine::core::config::Section<'_>, key: &str| {
        s.get(key)
            .flatten()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .map(|v| v as u32)
    };
    for s in data.system.sections() {
        let (Some(x), Some(y), Some(w), Some(h)) = (
            int(s, "inv_grid_x"),
            int(s, "inv_grid_y"),
            int(s, "inv_grid_width"),
            int(s, "inv_grid_height"),
        ) else {
            continue;
        };
        sections += 1;
        if (x + w) * 50 > 1024 || (y + h) * 50 > 2048 {
            outside.push(s.name().to_owned());
        }
        if let Ok(c) = ItemConfig::read(&data.system, s.name()) {
            let g = c.inv_grid.expect("the grid keys read");
            assert_eq!((g.x, g.y, g.width, g.height), (x, y, w, h), "{}", s.name());
        }
    }
    outside.sort();
    assert_eq!(sections, 309, "sections with the four grid keys");
    assert_eq!(outside, ["mp_wpn_colt1911", "wpn_colt1911"]);

    // A New Game's character (player-state.md) on the screen.
    let (_, mut character) = level::new_game_player(tree, &data).unwrap();
    let mut screen = InventoryScreen::load(tree, &data, XGA).unwrap();
    let mut audio = Sounds::default();
    screen
        .open(
            &data,
            &character,
            5.5 * 3600.0,
            0,
            [512.0, 384.0],
            &mut audio,
        )
        .unwrap();
    assert!(
        audio.0[0].ends_with("interface\\inv_open.ogg"),
        "{:?}",
        audio.0
    );
    let keys = Keys {
        inventory: vec![volya_engine::core::input::Key::I],
        drop: vec![volya_engine::core::input::Key::G],
        quit: data.quit_keys.clone(),
    };
    let mut input = Input::new();
    let mut now = 0;
    let mut frame = |screen: &mut InventoryScreen,
                     character: &mut CharacterState,
                     input: &mut Input,
                     audio: &mut Sounds|
     -> InventoryState {
        now += 16;
        let out = screen
            .update(Frame {
                data: &data,
                character,
                keys: &keys,
                time_of_day: 5.5 * 3600.0,
                now,
                input,
                audio,
                menu_font: None,
            })
            .unwrap();
        assert!(!out.closed);
        input.end_frame();
        screen.state(character)
    };
    let s = frame(&mut screen, &mut character, &mut input, &mut audio);
    // Section 9, New Game: lists empty but the outfit's (at cell 0, 0,
    // 114 × 114 units); the weight line, money, bars, protections.
    let cells = |s: &InventoryState, k: ListKind| s.lists[k as usize].cells.clone();
    for k in [
        ListKind::Bag,
        ListKind::Belt,
        ListKind::Pistol,
        ListKind::Automatic,
    ] {
        assert!(cells(&s, k).is_empty(), "{k:?}");
    }
    let outfit = &cells(&s, ListKind::Outfit)[0];
    assert_eq!(outfit.section, "novice_outfit");
    assert_eq!(outfit.rect, [858.0, 290.0, 114.0, 114.0]);
    assert_eq!(
        s.weight_line,
        "%c[default]Вес %c[UI_orange]4.3 %c[UI_orange]/ 50.0"
    );
    assert_eq!(s.money, "40 RU");
    assert_eq!(s.time, "05:30");
    assert_eq!(s.bars, [100.0, 100.0, 0.0]);
    assert_eq!(
        s.protections,
        [
            "Ожог %c[green] +10%",
            "Электрошок %c[green] +10%",
            "Удар %c[green] +10%",
            "Разрыв %c[green] +10%",
            "Хим. ожог %c[green] +10%",
            "Взрыв %c[green] +10%",
            "Пулестойкость %c[green] +10%",
        ]
    );
    // A right click on the character picture: the outfit's menu, two rows.
    let at = [900.0, 330.0];
    input.handle(Event::PointerMoved { at });
    for pressed in [true, false] {
        input.handle(Event::PointerButton {
            at: Some(at),
            button: MouseButton::Right,
            pressed,
            time: 0.0,
        });
    }
    let s = frame(&mut screen, &mut character, &mut input, &mut audio);
    let menu = s.menu.clone().expect("the outfit's menu");
    assert_eq!(menu.entries, ["st_undress_outfit", "st_drop"]);
    assert_eq!(menu.rect[3], 50.0);
    assert!(
        audio
            .0
            .last()
            .unwrap()
            .ends_with("interface\\inv_properties.ogg")
    );
    let panel = s.panel.expect("the outfit selected");
    assert_eq!(panel.weight, "3.00 kg");

    // The screen's scene.
    let mut draw = |screen: &InventoryScreen, character: &CharacterState| -> Scene {
        let mut ctx = Ctx {
            tree,
            data: &data,
            assets: &mut assets,
            resolution: XGA,
            surface: volya_engine::draw::Surface::screen(XGA),
            clock_ms: 1000,
        };
        let mut scene = Scene::new(XGA);
        screen.draw(&mut ctx, &mut scene, character).unwrap();
        scene
    };
    let scene = draw(&screen, &character);
    for layer in [
        "belt_slots",
        "back",
        "bag_static text",
        "dragdrop_outfit picture",
        "dragdrop_bag cell",
        "menu",
        "exit_button text",
        "cursor",
        "panel image",
    ] {
        assert!(scene.first(layer).is_some(), "{layer}");
    }
    // Section 3.2: 70 bag cells, 5 belt cells, 6 + 12 slot cells, none for
    // the outfit.
    assert_eq!(scene.layer("dragdrop_bag cell").count(), 70);
    assert_eq!(scene.layer("dragdrop_belt cell").count(), 5);
    assert_eq!(scene.layer("dragdrop_pistol cell").count(), 6);
    assert_eq!(scene.layer("dragdrop_automatic cell").count(), 12);
    assert_eq!(scene.layer("dragdrop_outfit cell").count(), 0);

    // Escape the menu (a key closes it), then drag the outfit to the
    // backpack: undressed, the bare picture.
    let key = |input: &mut Input, key| {
        for pressed in [true, false] {
            input.handle(Event::Key { key, pressed });
        }
    };
    key(&mut input, volya_engine::core::input::Key::LeftShift);
    let s = frame(&mut screen, &mut character, &mut input, &mut audio);
    assert!(s.menu.is_none());
    input.handle(Event::PointerButton {
        at: Some(at),
        button: MouseButton::Left,
        pressed: true,
        time: 1000.0,
    });
    let s = frame(&mut screen, &mut character, &mut input, &mut audio);
    assert_eq!(s.selected, Some(outfit.items[0]));
    let drop_at = [100.0, 300.0];
    input.handle(Event::PointerMoved { at: [905.0, 335.0] });
    let s = frame(&mut screen, &mut character, &mut input, &mut audio);
    assert_eq!(s.dragging, Some(outfit.items[0]));
    input.handle(Event::PointerMoved { at: drop_at });
    input.handle(Event::PointerButton {
        at: Some(drop_at),
        button: MouseButton::Left,
        pressed: false,
        time: 1300.0,
    });
    let s = frame(&mut screen, &mut character, &mut input, &mut audio);
    let id = outfit.items[0];
    assert_eq!(character.inventory.item(id).unwrap().place, Place::Backpack);
    assert!(character.inventory.worn_outfit().is_none());
    assert_eq!(cells(&s, ListKind::Bag)[0].items, [id]);
    assert!(cells(&s, ListKind::Outfit).is_empty());
    assert!(audio.0.last().unwrap().ends_with("interface\\inv_ruck.ogg"));
    assert!(s.protections.is_empty(), "no outfit worn");

    // Bread eaten with a double click: satiety rises, the bread is gone.
    let bread = InventoryItem::create(
        ItemConfig::read(&data.system, "bread").unwrap(),
        900,
        None,
        0,
        "actor",
    );
    character.inventory.take(bread);
    let s = frame(&mut screen, &mut character, &mut input, &mut audio);
    let cell = cells(&s, ListKind::Bag)
        .into_iter()
        .find(|c| c.section == "bread")
        .expect("the bread rebuilt into the backpack");
    let at = [cell.rect[0] + 5.0, cell.rect[1] + 5.0];
    input.handle(Event::PointerMoved { at });
    for time in [2000.0, 2100.0] {
        for pressed in [true, false] {
            input.handle(Event::PointerButton {
                at: Some(at),
                button: MouseButton::Left,
                pressed,
                time,
            });
        }
    }
    let before = character.conditions.satiety;
    frame(&mut screen, &mut character, &mut input, &mut audio);
    assert!(character.inventory.item(900).is_none());
    assert!((character.conditions.satiety - (before + 0.2).min(1.0)).abs() < 1e-6);
    assert!(audio.0.last().unwrap().ends_with("interface\\inv_slot.ogg"));
    draw(&screen, &character);
}
