// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The screen on the fixture's layout (the spec's normal values) with items
//! of a synthetic configuration: placement, grouping and growth (3.2, 4.3),
//! the drag rules (5.3), double click (5.5), the context menu and its
//! actions (5.6, 5.7), keys (6.1) and the gamepad (our design).

use super::*;
use crate::core::character::condition::{ConditionConfig, Conditions, Hysteresis};
use crate::core::character::inventory::{InventoryConfig, ItemConfig, Owner};
use crate::core::character::{NO_COMMUNITY, NOT_SET, Profile};
use crate::core::config::Config;
use crate::core::input::{Event, GamepadButton};
use crate::fixture::{self, FakeAudio, MemVfs};
use volya_engine_formats::ltx;

const XGA: Resolution = Resolution::new(1024, 768);

pub(crate) const ITEMS: &str = "\
    [inventory]\nmax_weight = 50\nmax_belt = 5\nslot_persistent_1 = true\n\
    slot_persistent_4 = true\nslot_persistent_5 = true\nslot_persistent_6 = true\n\
    slot_persistent_8 = true\nslot_persistent_9 = true\nslot_persistent_10 = true\n\
    [wpn_ak74]\nclass = WP_AK74\ninv_name = wpn_ak74\ncost = 10\nvisual = v\nscope_status = 0\nsilencer_status = 0\ngrenade_launcher_status = 0\nslot = 2\ninv_weight = 3\ninv_grid_x = 5\ninv_grid_y = 5\n\
    inv_grid_width = 5\ninv_grid_height = 2\n\
    [wpn_pm]\nclass = WP_PM\ninv_name = wpn_pm\ncost = 10\nvisual = v\nscope_status = 0\nsilencer_status = 0\ngrenade_launcher_status = 0\nslot = 1\ninv_weight = 1\ninv_grid_x = 0\ninv_grid_y = 0\n\
    inv_grid_width = 2\ninv_grid_height = 1\n\
    [medkit]\nclass = II_MEDKI\ninv_name = medkit\ncost = 10\nvisual = v\ninv_weight = 0.3\ninv_grid_x = 6\ninv_grid_y = 14\n\
    inv_grid_width = 2\ninv_grid_height = 1\neat_health = 0.6\neat_power = 0\neat_satiety = 0\n\
    eat_radiation = -0.1\nwounds_heal_perc = 0.3\neat_portions_num = 1\n\
    [bread]\nclass = II_FOOD\ninv_name = bread\ncost = 10\nvisual = v\nslot = 4\ninv_weight = 0.3\ninv_grid_x = 0\ninv_grid_y = 2\n\
    inv_grid_width = 1\ninv_grid_height = 1\neat_health = 0.05\neat_power = 0\n\
    eat_satiety = 0.2\neat_radiation = 0\nwounds_heal_perc = 0\neat_portions_num = -1\n\
    [novice_outfit]\nclass = E_STLK\ninv_name = novice_outfit\ncost = 10\nfull_icon_name = npc_icon_without_outfit\nburn_protection = 0\nshock_protection = 0\nstrike_protection = 0\nwound_protection = 0\nradiation_protection = 0\ntelepatic_protection = 0\nchemical_burn_protection = 0\nexplosion_protection = 0\nfire_wound_protection = 0\nvisual = v\nadditional_inventory_weight = 0\nadditional_inventory_weight2 = 0\nslot = 6\ninv_weight = 3\ninv_grid_x = 0\n\
    inv_grid_y = 18\ninv_grid_width = 2\ninv_grid_height = 2\n\
    [af_medusa]\nclass = ARTEFACT\ninv_name = af_medusa\ncost = 10\nvisual = v\nbelt = true\ninv_weight = 0.5\ninv_grid_x = 1\n\
    inv_grid_y = 1\ninv_grid_width = 1\ninv_grid_height = 1\n\
    [quest_doc]\nclass = II_DOC\ninv_name = quest_doc\ncost = 10\nvisual = v\nquest_item = true\ninv_weight = 0.1\ninv_grid_x = 1\n\
    inv_grid_y = 3\ninv_grid_width = 1\ninv_grid_height = 1\n\
    [grenade_f1]\nclass = G_F1\ninv_name = grenade_f1\ncost = 10\nvisual = v\nslot = 3\ninv_weight = 0.6\ninv_grid_x = 0\ninv_grid_y = 4\n\
    inv_grid_width = 1\ninv_grid_height = 1\n";

pub(crate) fn items() -> Config {
    ltx::parse(ITEMS.as_bytes()).unwrap()
}

fn condition_config() -> ConditionConfig {
    let h = Hysteresis {
        begin: 0.0,
        end: 0.0,
    };
    ConditionConfig {
        stand_power: 0.0,
        walk_power: 0.0,
        walk_weight_power: 0.0,
        overweight_walk_k: 0.0,
        accel_k: 0.0,
        sprint_k: 0.0,
        jump_power: 0.0,
        jump_weight_power: 0.0,
        overweight_jump_k: 0.0,
        max_power_leak_speed: 0.0,
        satiety_v: 0.0,
        satiety_power_v: 0.0,
        satiety_health_v: 0.0,
        radiation_v: 0.0,
        radiation_health_v: 0.0,
        psy_health_v: 0.0,
        morale_v: 0.0,
        alcohol_v: 0.0,
        bleeding_v: 0.0,
        health_restore_v: 0.0,
        max_walk_weight: 60.0,
        limping_health: h,
        limping_power: h,
        cant_walk_power: h,
        cant_sprint_power: h,
    }
}

/// A character holding `stored` items: (section, id, stored place code).
pub(crate) fn character(stored: &[(&str, u16, u8)]) -> CharacterState {
    let system = items();
    let mut inv = Inventory::new(InventoryConfig::read(&system).unwrap());
    for &(section, id, code) in stored {
        let item = InventoryItem::create(
            ItemConfig::read(&system, section).unwrap(),
            id,
            None,
            0,
            "actor",
        );
        inv.take_stored(item, code);
    }
    let profile = Profile {
        money: 40,
        specific_character: "actor".into(),
        trader_flags: 0,
        character_profile: "actor".into(),
        community: NO_COMMUNITY,
        rank: NOT_SET,
        reputation: NOT_SET,
        character_name: String::new(),
    };
    let owner = Owner {
        id: 0,
        position: [0.0; 3],
        game_graph_vertex: 0,
        level_graph_node: 0,
    };
    let mut c = CharacterState::new(
        owner,
        profile,
        Conditions::new_game(1.0),
        condition_config(),
        inv,
    );
    c.conditions.satiety = 0.5;
    c
}

struct Run {
    tree: MemVfs,
    data: GameData,
    screen: InventoryScreen,
    character: CharacterState,
    input: Input,
    audio: FakeAudio,
    now: u64,
    keys: Keys,
    dropped: Vec<InventoryItem>,
    closed: bool,
}

impl Run {
    fn open(stored: &[(&str, u16, u8)]) -> Self {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let mut screen = InventoryScreen::load(&tree, &data, XGA).unwrap();
        let character = character(stored);
        let mut audio = FakeAudio::default();
        screen
            .open(
                &data,
                &character,
                3600.0 * 7.25,
                0,
                [512.0, 384.0],
                &mut audio,
            )
            .unwrap();
        Self {
            tree,
            data,
            screen,
            character,
            input: Input::new(),
            audio,
            now: 0,
            keys: Keys {
                inventory: vec![Key::I],
                drop: vec![Key::G],
                quit: vec![Key::Escape],
            },
            dropped: Vec::new(),
            closed: false,
        }
    }

    fn frame(&mut self) -> InventoryState {
        self.now += 16;
        let out = self
            .screen
            .update(Frame {
                data: &self.data,
                character: &mut self.character,
                keys: &self.keys,
                time_of_day: 3600.0 * 7.25,
                now: self.now,
                input: &self.input,
                audio: &mut self.audio,
                menu_font: None,
            })
            .unwrap();
        self.input.end_frame();
        self.dropped.extend(out.dropped);
        self.closed |= out.closed;
        self.screen.state(&self.character)
    }

    fn sounds(&self) -> Vec<&str> {
        self.audio
            .started
            .iter()
            .map(|(_, p, _)| p.rsplit('\\').next().unwrap())
            .collect()
    }

    fn move_to(&mut self, at: [f32; 2]) {
        self.input.handle(Event::PointerMoved { at });
    }

    /// A button press or release; presses of different frames are more
    /// than 250 ms apart.
    fn button(&mut self, button: MouseButton, pressed: bool) {
        self.input.handle(Event::PointerButton {
            at: None,
            button,
            pressed,
            time: self.now as f64 * 100.0,
        });
    }

    fn click(&mut self, at: [f32; 2], button: MouseButton) -> InventoryState {
        self.move_to(at);
        self.button(button, true);
        self.button(button, false);
        self.frame()
    }

    fn double_click(&mut self, at: [f32; 2]) -> InventoryState {
        self.move_to(at);
        for _ in 0..2 {
            self.button(MouseButton::Left, true);
            self.button(MouseButton::Left, false);
        }
        self.frame()
    }

    /// A drag from `from` to `to`, released there.
    fn drag(&mut self, from: [f32; 2], to: [f32; 2]) -> InventoryState {
        self.move_to(from);
        self.button(MouseButton::Left, true);
        self.frame();
        self.move_to([from[0] + 3.0, from[1] + 3.0]);
        let s = self.frame();
        assert!(
            s.dragging.is_some(),
            "the drag began at {from:?}: {:?}",
            s.lists
        );
        self.move_to(to);
        self.button(MouseButton::Left, false);
        self.frame()
    }

    fn key(&mut self, key: Key) -> InventoryState {
        for pressed in [true, false] {
            self.input.handle(Event::Key { key, pressed });
        }
        self.frame()
    }

    fn pad(&mut self, button: GamepadButton) -> InventoryState {
        for pressed in [true, false] {
            self.input.handle(Event::GamepadButton { button, pressed });
        }
        self.frame()
    }

    fn place(&self, id: u16) -> Place {
        self.character.inventory.item(id).unwrap().place
    }
}

fn cells(s: &InventoryState, k: ListKind) -> Vec<CellState> {
    s.lists[k.index()].cells.clone()
}

/// The centre of the cell item holding `id`.
fn centre(s: &InventoryState, id: u16) -> [f32; 2] {
    for l in &s.lists {
        if let Some(c) = l.cells.iter().find(|c| c.items.contains(&id)) {
            let [x, y, w, h] = c.rect;
            let [lx, ly, lw, lh] = l.rect;
            // Within the list (the outfit's cell item is wider than it).
            return [
                (x + w.min(lx + lw - x) / 2.0).min(lx + lw - 1.0),
                (y + h.min(ly + lh - y) / 2.0).min(ly + lh - 1.0),
            ];
        }
    }
    panic!("no cell item holds {id}");
}

#[test]
fn rebuild_sorts_groups_and_places_the_backpack() {
    // Section 4.3's example: a rifle, two similar medkits, bread.
    let mut r = Run::open(&[
        ("wpn_ak74", 100, 3),
        ("medkit", 101, 3),
        ("medkit", 102, 3),
        ("bread", 103, 3),
    ]);
    let s = r.frame();
    let bag = cells(&s, ListKind::Bag);
    let got: Vec<(Vec<u16>, [f32; 2])> = bag
        .iter()
        .map(|c| (c.items.clone(), [c.rect[0], c.rect[1]]))
        .collect();
    assert_eq!(
        got,
        [
            (vec![100], [37.0, 273.0]),
            (vec![102, 101], [242.0, 273.0]),
            (vec![103], [242.0, 314.0]),
        ]
    );
    // Sizes in units: 5 × 41 by 2 × 41.
    assert_eq!(bag[0].rect[2..], [205.0, 82.0]);
    assert!(!s.lists[0].scroll_bar);
    // The opening's sound and per-frame values (4.7).
    assert_eq!(r.sounds(), ["inv_open.ogg"]);
    assert_eq!((s.money.as_str(), s.time.as_str()), ("40 RU", "07:15"));
    assert_eq!(s.bars, [100.0, 100.0, 0.0]);
    // 4.5: no outfit, the carry limit is max_weight.
    assert_eq!(
        s.weight_line,
        "%c[default]ui_inv_weight %c[UI_orange]3.9 %c[UI_orange]/ 50.0"
    );
}

#[test]
fn lists_grow_and_take_drop_positions() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let layout = Layout::load(&tree, &data, XGA).unwrap();
    let mut c = character(&[]);
    for id in 1..=80 {
        let mut item = InventoryItem::create(
            ItemConfig::read(&items(), "af_medusa").unwrap(),
            id,
            None,
            0,
            "actor",
        );
        // Conditions apart: none similar.
        item.condition = f32::from(id) * 0.011;
        c.inventory.take_stored(item, 3);
    }
    // The bag is unlimited: 80 items need 12 rows of 7, and it scrolls.
    let mut bag = ItemList::new(*layout.list(ListKind::Bag));
    for id in 1..=80 {
        bag.place(&c.inventory, id, None).unwrap();
    }
    assert_eq!(bag.rows, 12);
    assert!(bag.bar_shown());
    assert_eq!(bag.scroll_step(), 13.0);
    bag.scroll_by(100);
    assert_eq!(bag.scroll, 12.0 * 41.0 - 442.0);
    assert_eq!(bag.cell_of([37.0, 273.0]), Some((0, 1)));
    // A drop position: the cell under the picture's top-left when free.
    let mut belt = ItemList::new(*layout.list(ListKind::Belt));
    belt.place(&c.inventory, 1, Some((3, 0))).unwrap();
    assert_eq!(belt.cells[0].cell, (3, 0));
    belt.place(&c.inventory, 2, Some((3, 0))).unwrap();
    assert_eq!(belt.cells[1].cell, (0, 0), "taken: first free");
    belt.place(&c.inventory, 3, None).unwrap();
    belt.place(&c.inventory, 4, None).unwrap();
    belt.place(&c.inventory, 5, None).unwrap();
    // A sixth does not fit and the belt may not grow: placed again, still
    // no room, an error (the original's fatal error).
    assert!(belt.place(&c.inventory, 6, None).is_err());
    // A moved cell item takes its drop cell, its own room free.
    let mut belt = ItemList::new(*layout.list(ListKind::Belt));
    belt.place(&c.inventory, 1, None).unwrap();
    belt.move_cell(&c.inventory, 0, Some((4, 0))).unwrap();
    assert_eq!(belt.cells[0].cell, (4, 0));
    assert_eq!(belt.cell_of([645.0 + 299.0, 136.0]), Some((4, 0)));
    assert_eq!(belt.cell_of([645.0 + 301.0, 136.0]), None);
}

#[test]
fn similar_items_group_but_weapons_need_their_place_and_addons() {
    let system = items();
    let item = |section, id| {
        InventoryItem::create(
            ItemConfig::read(&system, section).unwrap(),
            id,
            None,
            0,
            "actor",
        )
    };
    let (a, mut b) = (item("medkit", 1), item("medkit", 2));
    assert!(list::similar(&a, &b));
    b.condition = 0.98;
    assert!(!list::similar(&a, &b));
    let (mut x, y) = (item("wpn_pm", 3), item("wpn_pm", 4));
    assert!(list::similar(&x, &y));
    x.place = Place::Backpack;
    assert!(!list::similar(&x, &y), "different places");
    assert!(!list::similar(&a, &y));
}

#[test]
fn dragging_moves_items_between_places() {
    let mut r = Run::open(&[
        ("af_medusa", 1, 3),
        ("wpn_ak74", 2, 3),
        ("wpn_ak74", 3, 3),
        ("novice_outfit", 4, 1),
        ("bread", 5, 3),
        ("wpn_pm", 6, 3),
    ]);
    let s = r.frame();
    // Case 5: backpack to belt at the drop cell (the picture's top-left
    // over cell 2).
    let from = centre(&s, 1);
    let s = r.drag(from, [645.0 + 2.0 * 60.0 + 25.0, 160.0]);
    assert_eq!(r.place(1), Place::Belt);
    assert_eq!(cells(&s, ListKind::Belt)[0].cell, [2, 0]);
    assert_eq!(r.sounds().last(), Some(&"inv_belt.ogg"));
    // Case 2: within the belt (custom placement) to cell 4 (the picture,
    // grabbed 33 units into the cell, has its top-left there); the inventory
    // does not change.
    let from = centre(&s, 1);
    let s = r.drag(from, [645.0 + 4.0 * 60.0 + 40.0, 160.0]);
    assert_eq!(cells(&s, ListKind::Belt)[0].cell, [4, 0]);
    assert_eq!(r.place(1), Place::Belt);
    // The two rifles in the backpack share a cell (similar, 4.3), the
    // higher id its own item.
    let s = r.frame();
    let rifles = cells(&s, ListKind::Bag)
        .into_iter()
        .find(|c| c.section == "wpn_ak74")
        .unwrap();
    assert_eq!(rifles.items, [3, 2]);
    // Case 6: a rifle onto the automatic list: to its slot, in the hands;
    // the other stays in the cell.
    let from = centre(&s, 2);
    let s = r.drag(from, [300.0, 150.0]);
    assert_eq!(r.place(3), Place::Slot(2));
    assert_eq!(r.character.inventory.in_hands(), Some(3));
    assert_eq!(cells(&s, ListKind::Automatic)[0].items, [3]);
    assert_eq!(r.sounds().last(), Some(&"inv_slot.ogg"));
    // The second rifle there: with swap, the first goes to the backpack.
    let from = centre(&s, 2);
    let s = r.drag(from, [300.0, 150.0]);
    assert_eq!((r.place(2), r.place(3)), (Place::Slot(2), Place::Backpack));
    assert!(cells(&s, ListKind::Bag).iter().any(|c| c.items == [3]));
    let n = r.sounds().len();
    assert_eq!(r.sounds()[n - 2..], ["inv_ruck.ogg", "inv_slot.ogg"]);
    // Case 3: slot list to slot list: nothing.
    let from = centre(&s, 2);
    let s = r.drag(from, [60.0, 150.0]);
    assert_eq!(r.place(2), Place::Slot(2));
    // A pistol onto the automatic list (not its slot's): nothing.
    let from = centre(&s, 6);
    let s = r.drag(from, [300.0, 150.0]);
    assert_eq!(r.place(6), Place::Backpack);
    // Food dropped on the character picture is eaten (the further effect).
    let satiety = r.character.conditions.satiety;
    let from = centre(&s, 5);
    let s = r.drag(from, [900.0, 450.0]);
    assert!(r.character.inventory.item(5).is_none());
    assert!((r.character.conditions.satiety - (satiety + 0.2)).abs() < 1e-6);
    assert_eq!(r.sounds().last(), Some(&"inv_slot.ogg"));
    assert_eq!(s.selected, None, "use clears the selection");
    // The outfit to the backpack: undressed.
    let s = r.frame();
    let s = r.drag(centre(&s, 4), [100.0, 600.0]);
    assert_eq!(r.place(4), Place::Backpack);
    assert!(cells(&s, ListKind::Outfit).is_empty());
    // Released outside every list: nothing.
    let from = centre(&s, 4);
    r.drag(from, [600.0, 40.0]);
    assert_eq!(r.place(4), Place::Backpack);
}

#[test]
fn double_click_uses_moves_to_slot_belt_or_backpack() {
    let mut r = Run::open(&[
        ("wpn_pm", 1, 3),
        ("af_medusa", 2, 3),
        ("medkit", 3, 3),
        ("af_medusa", 4, 1),
    ]);
    let s = r.frame();
    // From the bag: to its slot first.
    let s = r.double_click(centre(&s, 1));
    assert_eq!(r.place(1), Place::Slot(1));
    assert_eq!(cells(&s, ListKind::Pistol)[0].items, [1]);
    // No slot: to the belt.
    let s = r.double_click(centre(&s, 2));
    assert_eq!(r.place(2), Place::Belt);
    // From a slot or the belt: to the backpack.
    let s = r.double_click(centre(&s, 1));
    assert_eq!(r.place(1), Place::Backpack);
    let s = r.double_click(centre(&s, 2));
    assert_eq!(r.place(2), Place::Backpack);
    // Eatable: used (its effects wait for the condition update,
    // player-state.md section 5).
    let s = r.double_click(centre(&s, 3));
    assert!(r.character.inventory.item(3).is_none());
    assert_eq!(r.sounds().last(), Some(&"inv_slot.ogg"));
    // The next frame rebuilds the lists (4.3).
    let s2 = r.frame();
    assert!(
        cells(&s2, ListKind::Bag)
            .iter()
            .all(|c| !c.items.contains(&3))
    );
    let _ = s;
}

#[test]
fn the_context_menu_offers_and_acts() {
    let mut r = Run::open(&[
        ("wpn_pm", 1, 3),
        ("medkit", 2, 3),
        ("medkit", 3, 3),
        ("quest_doc", 4, 3),
        ("novice_outfit", 5, 3),
        ("bread", 6, 3),
        ("grenade_f1", 7, 1),
    ]);
    let s = r.frame();
    let menu = |s: &InventoryState| s.menu.clone().expect("a menu").entries;
    // A pistol in the backpack, its slot empty.
    let s = r.click(centre(&s, 1), MouseButton::Right);
    assert_eq!(menu(&s), ["st_move_to_slot", "st_drop"]);
    assert_eq!(r.sounds().last(), Some(&"inv_properties.ogg"));
    assert_eq!(s.selected, Some(1));
    // Section 5.6: 20 units a row, 10 below; it opens left of the cursor.
    let m = s.menu.clone().unwrap();
    assert_eq!(m.rect[3], 50.0);
    assert_eq!(m.rect[0] + m.rect[2], centre(&s, 1)[0]);
    // A press outside closes it without action.
    let s = r.click([600.0, 60.0], MouseButton::Left);
    assert!(s.menu.is_none());
    assert_eq!(r.place(1), Place::Backpack);
    // Grouped medkits: use, drop, drop all.
    let s = r.click(centre(&s, 2), MouseButton::Right);
    assert_eq!(menu(&s), ["st_use", "st_drop", "st_drop_all"]);
    // Hover selects a row; a press on it chooses it.
    let row = s.menu.clone().unwrap().rows[2];
    let at = [row[0] + 4.0, row[1] + 10.0];
    r.move_to(at);
    let s = r.frame();
    assert_eq!(s.menu.clone().unwrap().hovered, Some(2));
    let s = r.click(at, MouseButton::Left);
    assert!(s.menu.is_none());
    let ids: Vec<u16> = r.dropped.iter().map(|i| i.id).collect();
    assert_eq!(ids, [2, 3], "the grouped one, then the cell's own");
    assert_eq!(
        r.sounds()[r.sounds().len() - 2..],
        ["inv_drop.ogg", "inv_drop.ogg"]
    );
    assert!(r.character.inventory.item(2).is_none());
    // The weight line follows the drop (4.5): 1 + 0.1 + 3 + 0.3 + 0.6.
    assert!(s.weight_line.contains("5.0 "), "{}", s.weight_line);
    // A quest item: no drop, no menu at all.
    let s = r.frame();
    let s = r.click(centre(&s, 4), MouseButton::Right);
    assert!(s.menu.is_none());
    assert_eq!(s.selected, Some(4));
    let s = r.key(Key::G);
    assert!(r.character.inventory.item(4).is_some(), "never dropped");
    // An outfit in the backpack: dress it.
    let s = r.click(centre(&s, 5), MouseButton::Right);
    assert_eq!(menu(&s), ["st_dress_outfit", "st_drop"]);
    let row = s.menu.clone().unwrap().rows[0];
    let s = r.click([row[0] + 4.0, row[1] + 10.0], MouseButton::Left);
    assert_eq!(r.place(5), Place::Slot(6));
    assert_eq!(cells(&s, ListKind::Outfit)[0].items, [5]);
    // Worn: undress it.
    let s = r.click([900.0, 330.0], MouseButton::Right);
    assert_eq!(menu(&s), ["st_undress_outfit", "st_drop"]);
    // Any key closes the menu.
    let s = r.key(Key::LeftShift);
    assert!(s.menu.is_none() && !r.closed);
    // Food: eat; the grenade, shown in the bag but in its persistent slot:
    // drop only.
    let s = r.click(centre(&s, 6), MouseButton::Right);
    assert_eq!(menu(&s), ["st_eat", "st_drop"]);
    r.key(Key::LeftShift);
    let s = r.frame();
    let s = r.click(centre(&s, 7), MouseButton::Right);
    assert_eq!(menu(&s), ["st_drop"]);
    // The drop key drops the selected item, closing the menu.
    let s = r.key(Key::G);
    assert!(s.menu.is_none());
    assert!(r.character.inventory.item(7).is_none());
    assert_eq!(r.dropped.last().map(|i| i.id), Some(7));
}

#[test]
fn keys_close_it_and_the_exit_button_does() {
    let mut r = Run::open(&[]);
    r.frame();
    r.key(Key::I);
    assert!(r.closed && !r.screen.is_open());
    assert_eq!(r.sounds().last(), Some(&"inv_close.ogg"));
    let mut r = Run::open(&[]);
    r.key(Key::Escape);
    assert!(r.closed);
    // The exit button: activated on release over it.
    let mut r = Run::open(&[]);
    let s = r.frame();
    let [x, y, w, h] = s.exit;
    r.click([x + w / 2.0, y + h / 2.0], MouseButton::Left);
    assert!(r.closed);
    // Other keys do not close it.
    let mut r = Run::open(&[]);
    r.key(Key::W);
    assert!(!r.closed);
}

#[test]
fn death_closes_it_and_items_entering_rebuild() {
    let mut r = Run::open(&[]);
    r.frame();
    let bread = InventoryItem::create(
        ItemConfig::read(&items(), "bread").unwrap(),
        9,
        None,
        0,
        "actor",
    );
    r.character.inventory.take(bread);
    let s = r.frame();
    assert_eq!(cells(&s, ListKind::Bag)[0].items, [9]);
    r.character.conditions.health = 0.0;
    r.frame();
    assert!(r.closed);
}

#[test]
fn a_gamepad_navigates_selects_and_opens_the_menu() {
    let mut r = Run::open(&[("wpn_pm", 1, 3), ("bread", 2, 3)]);
    let s = r.frame();
    // The d-pad goes to the nearest target that way: from the screen's
    // centre left, the bag's cell items.
    let s = {
        let _ = s;
        r.pad(GamepadButton::DpadLeft)
    };
    let id = s
        .lists
        .iter()
        .flat_map(|l| &l.cells)
        .find(|c| {
            let [x, y, w, h] = c.rect;
            s.cursor[0] >= x && s.cursor[0] <= x + w && s.cursor[1] >= y && s.cursor[1] <= y + h
        })
        .map(|c| c.items[0]);
    assert!(id.is_some(), "on a cell item: {:?}", s.cursor);
    // South selects (the left button), west opens the menu (the right).
    let s = r.pad(GamepadButton::South);
    assert_eq!(s.selected, id);
    let s = r.pad(GamepadButton::West);
    assert!(s.menu.is_some());
    // The d-pad moves within the menu's rows; east closes the screen.
    let s = r.pad(GamepadButton::DpadDown);
    assert!(s.menu.is_some());
    r.pad(GamepadButton::East);
    assert!(r.closed);
    // North, the inventory button, closes it too.
    let mut r = Run::open(&[]);
    r.pad(GamepadButton::North);
    assert!(r.closed);
}

#[test]
fn it_draws_the_layout_and_items() {
    let mut r = Run::open(&[("medkit", 1, 3), ("medkit", 2, 3), ("af_medusa", 3, 2)]);
    let s = r.frame();
    r.click(centre(&s, 3), MouseButton::Left);
    let mut assets = fixture::assets();
    let mut ctx = Ctx {
        tree: &r.tree,
        data: &r.data,
        assets: &mut assets,
        resolution: XGA,
        text_surface: crate::draw::Surface::screen(XGA),
        clock_ms: 0,
    };
    let mut scene = Scene::new(XGA);
    r.screen.draw(&mut ctx, &mut scene, &r.character).unwrap();
    // 70 bag cells, 5 belt, 6 + 12 in the slot lists, none for the outfit.
    assert_eq!(scene.layer("dragdrop_bag cell").count(), 70);
    assert_eq!(scene.layer("dragdrop_belt cell").count(), 5);
    assert_eq!(scene.layer("dragdrop_pistol cell").count(), 6);
    assert_eq!(scene.layer("dragdrop_automatic cell").count(), 12);
    assert_eq!(scene.layer("dragdrop_outfit cell").count(), 0);
    // The cells' corners rounded, less half a pixel, the grid's left half.
    let cell = scene.first("dragdrop_bag cell").unwrap();
    assert_eq!(cell.corners[0], [36.5, 272.5]);
    assert_eq!(cell.corners[2], [77.5, 313.5]);
    assert_eq!((cell.uv_min, cell.uv_max), ([0.0, 0.0], [0.5, 1.0]));
    // The grouped medkits: one icon, `x2`; its icon region (4.2).
    let icon = scene.first("dragdrop_bag item").unwrap();
    assert_eq!(icon.uv_min, [300.0 / 1024.0, 700.0 / 2048.0]);
    assert_eq!(icon.uv_max, [400.0 / 1024.0, 750.0 / 2048.0]);
    assert_eq!(scene.layer("dragdrop_bag item").count(), 1);
    assert_eq!(scene.layer("dragdrop_bag count").count(), 2, "x2");
    // The selected item's panel and the character picture without an
    // outfit; the cursor last.
    assert!(scene.first("panel image").is_some());
    assert!(scene.first("dragdrop_outfit picture").is_some());
    assert_eq!(scene.sprites.last().unwrap().layer, "cursor");
}

#[test]
fn bars_follow_their_values_and_jump_from_nothing_drawn() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let layout = Layout::load(&tree, &data, XGA).unwrap();
    let d = &layout.bars[0];
    let mut bar = Bar::default();
    // Not drawn before: jumps.
    bar.set(d, 80.0, 0.1);
    assert_eq!(bar.drawn, Some(80.0));
    // Drawn: at most the whole range per second (inertion 0).
    bar.set(d, 0.0, 0.1);
    assert!((bar.drawn.unwrap() - 70.0).abs() < 1e-4);
    // At 0 it drew nothing, so the next value jumps.
    bar.drawn = Some(0.0);
    bar.set(d, 50.0, 0.1);
    assert_eq!(bar.drawn, Some(50.0));
    // Clamped to the range.
    bar.set(d, 500.0, 10.0);
    assert_eq!(bar.drawn, Some(100.0));
}

#[test]
fn only_class_ammo_counts_rounds() {
    let system = ltx::parse(
        b"[ammo_5.45x39_fmj]\nclass = AMMO\ninv_weight = 0.3\nvisual = v\n\
          [ammo_vog-25]\nclass = A_VOG25\ninv_weight = 0.3\nvisual = v\n",
    )
    .unwrap();
    let item =
        |s| InventoryItem::create(ItemConfig::read(&system, s).unwrap(), 1, None, 0, "actor");
    assert!(list::is_ammunition(&item("ammo_5.45x39_fmj")));
    assert!(!list::is_ammunition(&item("ammo_vog-25")));
}

/// ltx-config.md 11: the panel's `inv_name` and `cost` (4.6) and the worn
/// outfit's `full_icon_name` (4.4) are read, so a missing one is fatal.
#[test]
fn a_missing_panel_key_is_fatal() {
    let mut run = Run::open(&[("bread", 5, 3)]);
    let without = ITEMS.replace("inv_name = bread\n", "");
    assert_ne!(without, ITEMS);
    run.data.system = ltx::parse(without.as_bytes()).unwrap();
    let err = run
        .screen
        .select(&run.data, &run.character.inventory, Some(5))
        .unwrap_err();
    assert!(err.to_string().contains("inv_name"), "{err}");
    let without = ITEMS.replace("cost = 10\n", "");
    run.data.system = ltx::parse(without.as_bytes()).unwrap();
    let err = run
        .screen
        .select(&run.data, &run.character.inventory, Some(5))
        .unwrap_err();
    assert!(err.to_string().contains("cost"), "{err}");
}

/// 4.2: an installed attachable addon is drawn with the icon of the section
/// the weapon's `scope_name` names; that section lacking a grid key is
/// fatal.
#[test]
fn an_addon_section_without_its_grid_keys_is_fatal() {
    use crate::core::character::inventory::{ADDON_SCOPE, ItemState};
    let mut r = Run::open(&[("wpn_pm", 1, 3)]);
    let item = &mut r.character.inventory.items[0];
    item.config.scope_status = 2;
    let ItemState::Weapon(w) = &mut item.state else {
        panic!("{:?}", item.state)
    };
    w.installed_addons |= ADDON_SCOPE;
    let draw = |r: &mut Run, scope: &str| {
        r.data.system = ltx::parse(
            format!(
                "{}{scope}",
                ITEMS.replace(
                    "[wpn_pm]\n",
                    "[wpn_pm]\nscope_name = wpn_scope\nscope_x = 10\nscope_y = 5\n"
                )
            )
            .as_bytes(),
        )
        .unwrap();
        let mut assets = fixture::assets();
        let mut ctx = Ctx {
            tree: &r.tree,
            data: &r.data,
            assets: &mut assets,
            resolution: XGA,
            text_surface: crate::draw::Surface::screen(XGA),
            clock_ms: 0,
        };
        let mut scene = Scene::new(XGA);
        r.screen
            .draw(&mut ctx, &mut scene, &r.character)
            .map(|()| scene.layer("dragdrop_bag addon").count())
    };
    let grid = "[wpn_scope]\ninv_grid_x = 0\ninv_grid_y = 0\ninv_grid_width = 1\n";
    assert_eq!(
        draw(&mut r, &format!("{grid}inv_grid_height = 1\n")).unwrap(),
        1
    );
    let err = draw(&mut r, grid).unwrap_err();
    assert!(
        err.to_string()
            .contains("[wpn_scope] lacks inv_grid_height"),
        "{err}"
    );
}
