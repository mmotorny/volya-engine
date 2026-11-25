// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player character's state on New Game from the Russian edition
//! (BYOA): the verification facts and invariants of
//! docs/behavior/player-state.md section 7 (the player's record, its
//! profile, the starting inventory and its placement, the configuration,
//! the derived rates, a vodka and a medkit used). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeSet;
use std::ops::ControlFlow;

use volya_engine::core::character::condition::{Activity, ConditionConfig, Conditions};
use volya_engine::core::character::inventory::{
    InventoryConfig, InventoryItem, ItemConfig, ItemState, Place, Supplies, WeaponItem,
};
use volya_engine::core::character::{CharacterState, NOT_SET};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs, read};
use volya_engine::data::GameData;
use volya_engine::level;
use volya_engine::objects::Characters;
use volya_engine_formats::object::{self, Object, ObjectRecord};
use volya_engine_formats::spawn;
use volya_engine_formats::ui::Resolution;

fn with_game(test: impl FnOnce(&dyn Vfs, &GameData)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    test(&tree, &data);
}

#[track_caller]
fn near(got: f32, want: f32, tolerance: f32) {
    assert!((got - want).abs() <= tolerance, "{got} != {want}");
}

#[test]
fn new_game_player_state_from_game_data() {
    with_game(|tree, data| {
        let system = &data.system;
        let spawn_file = read(
            tree,
            &data.aliases.path("$game_spawn$", "all.spawn").unwrap(),
        )
        .unwrap();

        // Section 7: one O_ACTOR record, no record with a parent, and every
        // weapon record's ammunition.
        let mut actors = Vec::new();
        let mut parented = 0;
        let mut weapons = Vec::new();
        spawn::for_each_record(&spawn_file, |r| {
            let section = object::section(r.spawn)?;
            let class = system.get(&section, "class").flatten().unwrap().trim();
            let record = ObjectRecord::parse(r.spawn, r.update, class, true)?;
            if class == "O_ACTOR" {
                actors.push((r.index, record.header.clone()));
            }
            parented += usize::from(record.header.parent_id != 0xFFFF);
            let weapon = match &record.object {
                Object::Knife(w) => Some(&w.weapon),
                Object::MagazineWeapon(w) => Some(&w.weapon),
                Object::WeaponWithGrenadeLauncher(w) => Some(&w.weapon),
                Object::Shotgun(w) => Some(&w.weapon),
                _ => None,
            };
            if let Some(w) = weapon {
                weapons.push((
                    section,
                    w.ammo_in_reserve,
                    w.weapon_state,
                    w.ammo_in_magazine,
                ));
            }
            Ok(ControlFlow::Continue(()))
        })
        .unwrap();
        assert_eq!(actors.len(), 1);
        let (index, header) = &actors[0];
        assert_eq!(
            (
                *index,
                header.section.as_str(),
                header.name.as_str(),
                header.parent_id
            ),
            (63, "actor", "level_prefix_actor_0001", 0xFFFF)
        );
        assert_eq!(parented, 0);
        assert_eq!(weapons.len(), 34);
        assert!(weapons.iter().all(|w| w.1 == 90 && w.2 == 0));
        let loaded: Vec<_> = weapons.iter().filter(|w| w.3 == 1).collect();
        assert_eq!(loaded.len(), 2);
        assert!(loaded.iter().all(|w| w.0 == "wpn_rpg7"));
        assert_eq!(weapons.iter().filter(|w| w.3 == 0).count(), 32);

        // The player's record.
        let (record, state) = level::new_game_player(tree, data).unwrap();
        let Object::Player(p) = &record.object else {
            panic!("{:?}", record.kind())
        };
        let t = &p.trader;
        assert_eq!(p.creature.health, 1.0);
        assert_eq!(
            (t.money, t.specific_character.as_str(), t.trader_flags),
            (40, "", 0)
        );
        assert_eq!(
            (
                t.character_profile.as_str(),
                t.community,
                t.rank,
                t.reputation
            ),
            ("actor", -1, NOT_SET, NOT_SET)
        );
        assert_eq!(NOT_SET.to_le_bytes(), [0x01, 0, 0, 0x80]);
        assert_eq!(t.character_name, "");
        assert_eq!((p.update.creature.health, p.update.radiation), (1.0, 0.0));
        let custom = &p.creature.object.custom_data;
        assert_eq!(custom.len(), 99);
        assert!(custom.contains("\r\n"));
        let supplies = Supplies::parse(custom).unwrap();
        assert!(supplies.dont_spawn_character_supplies);
        let keys: Vec<_> = supplies
            .entries
            .iter()
            .map(|e| e.section.as_str())
            .collect();
        assert_eq!(
            keys,
            [
                "detector_simple",
                "device_torch",
                "novice_outfit",
                "wpn_binoc"
            ]
        );
        let file_order: Vec<_> = custom
            .lines()
            .map(str::trim)
            .skip_while(|l| *l != "[spawn]")
            .skip(1)
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(
            file_order,
            [
                "wpn_binoc",
                "detector_simple",
                "novice_outfit",
                "device_torch"
            ]
        );
        assert!(
            supplies
                .entries
                .iter()
                .all(|e| e.count == 1 && e.probability == 1.0)
        );

        // The specific character and profile `actor`.
        let mut characters = Characters::new(tree, data);
        let actor = characters.specific_character(t).unwrap().unwrap().clone();
        assert_eq!(
            (
                actor.id.as_str(),
                actor.community.as_str(),
                actor.rank,
                actor.reputation
            ),
            ("actor", "actor", 0, 0)
        );
        assert_eq!(actor.name, "actor_name");
        assert_eq!(actor.visual, "actors\\hero\\stalker_novice");
        let character_supplies = Supplies::parse(&actor.supplies).unwrap();
        let torch: Vec<_> = character_supplies
            .entries
            .iter()
            .map(|e| e.section.as_str())
            .collect();
        assert_eq!(torch, ["device_torch"]);
        assert!(actor.supplies.contains("[spawn]"));

        // Section 2.2: the profile resolved.
        let pr = &state.profile;
        assert_eq!(
            (pr.money, pr.specific_character.as_str(), pr.trader_flags),
            (40, "actor", 0)
        );
        assert_eq!(
            (
                pr.character_profile.as_str(),
                pr.community,
                pr.rank,
                pr.reputation
            ),
            ("actor", 0, 0, 0)
        );
        assert_eq!(pr.character_name, "Меченый");

        // Section 2.3.
        let c = &state.conditions;
        assert_eq!(
            (c.health, c.max_health, c.power, c.max_power, c.satiety),
            (1.0, 1.0, 1.0, 1.0, 1.0)
        );
        assert_eq!(
            (c.radiation, c.psy_health, c.morale, c.alcohol),
            (0.0, 1.0, 1.0, 0.0)
        );
        assert!(!c.limping && !c.cant_walk && !c.cant_sprint);

        check_inventory(&state);
        check_configuration(system, &state);
        check_rates(system, &state);
        check_eatables(system, &state);
    });
}

/// Section 7: the starting inventory, its places, weight and records.
fn check_inventory(state: &CharacterState) {
    let inv = &state.inventory;
    let items: Vec<_> = inv
        .items
        .iter()
        .map(|i| {
            (
                i.config.section.as_str(),
                i.config.class_id.as_str(),
                i.place,
                i.condition,
            )
        })
        .collect();
    assert_eq!(
        items,
        [
            ("detector_simple", "D_SIMDET", Place::Slot(8), 1.0),
            ("device_torch", "TORCH_S", Place::Slot(9), 1.0),
            ("novice_outfit", "E_STLK", Place::Slot(6), 1.0),
            ("wpn_binoc", "WP_BINOC", Place::Slot(4), 1.0),
            ("device_pda", "D_PDA", Place::Slot(7), 1.0),
            ("bolt", "II_BOLT", Place::Slot(5), 1.0),
        ]
    );
    let slots: BTreeSet<_> = inv.items.iter().map(|i| format!("{:?}", i.place)).collect();
    assert_eq!(slots.len(), 6);
    assert!(inv.belt.is_empty());
    assert_eq!(inv.active_slot, Some(4));
    near(inv.weight(), 4.3, 1e-6);
    let weights: Vec<f32> = inv.items.iter().map(|i| i.config.inv_weight).collect();
    assert_eq!(weights, [0.0, 0.0, 3.0, 1.0, 0.3, 0.0]);
    assert_eq!(
        inv.items[3].state,
        ItemState::Weapon(WeaponItem {
            ammo_in_reserve: 90,
            ammo_in_magazine: 0,
            weapon_state: 0,
            installed_addons: 0,
            ammo_type: 0,
            fire_mode: 0,
        })
    );
    assert_eq!(inv.items[1].state, ItemState::Torch { torch_flags: 0 });
    assert_eq!(
        inv.items[4].state,
        ItemState::Pda {
            original_owner_id: 0,
            specific_character: "actor".into(),
            info_portion: String::new()
        }
    );

    // The records a save writes, each read back as written.
    let records = state.item_records().unwrap();
    let models: Vec<_> = records
        .iter()
        .map(|r| {
            (
                r.header.name.clone(),
                r.object.visual().unwrap().model.clone(),
            )
        })
        .collect();
    let ids: Vec<u16> = records.iter().map(|r| r.header.id).collect();
    let name = |s: &str, id: u16| format!("{s}{id:04}");
    assert_eq!(
        models,
        [
            (
                name("detector_simple", ids[0]),
                "equipments\\item_datchik1".to_owned()
            ),
            (
                name("device_torch", ids[1]),
                "equipments\\light_night".to_owned()
            ),
            (
                name("novice_outfit", ids[2]),
                "equipments\\novice_suit".to_owned()
            ),
            (
                name("wpn_binoc", ids[3]),
                "weapons\\binoculars\\wpn_binoculars".to_owned()
            ),
            (
                name("device_pda", ids[4]),
                "equipments\\item_pda".to_owned()
            ),
        ]
    );
    for r in &records {
        assert_eq!(r.header.parent_id, 0);
        let (spawn, update) = r.write().unwrap();
        assert_eq!(
            &ObjectRecord::parse(&spawn, &update, r.class_id.as_str(), true).unwrap(),
            r
        );
    }
}

/// Section 7: item keys and the condition and inventory configuration.
fn check_configuration(system: &volya_engine::core::config::Config, state: &CharacterState) {
    let get = |s: &str, k: &str| system.get(s, k).flatten().map(str::to_owned);
    assert_eq!(
        get("detector_simple", "default_to_ruck").as_deref(),
        Some("false")
    );
    for s in [
        "device_torch",
        "novice_outfit",
        "wpn_binoc",
        "device_pda",
        "bolt",
    ] {
        assert_eq!(get(s, "default_to_ruck"), None, "{s}");
    }
    for s in [
        "detector_simple",
        "device_torch",
        "novice_outfit",
        "wpn_binoc",
        "device_pda",
        "bolt",
    ] {
        assert_eq!(get(s, "belt"), None, "{s}");
    }
    let outfit = ItemConfig::read(system, "novice_outfit").unwrap();
    assert_eq!(
        (
            outfit.additional_inventory_weight,
            outfit.additional_inventory_weight2,
            outfit.power_loss
        ),
        (0.0, 0.0, None)
    );
    let binoc = ItemConfig::read(system, "wpn_binoc").unwrap();
    assert_eq!(
        (
            binoc.ammo_mag_size,
            binoc.scope_status,
            binoc.silencer_status,
            binoc.grenade_launcher_status
        ),
        (0, 1, 0, 0)
    );
    let c = &state.condition_config;
    assert_eq!(
        c,
        &ConditionConfig::read(system, "actor_condition").unwrap()
    );
    assert_eq!(
        (
            c.stand_power,
            c.walk_power,
            c.walk_weight_power,
            c.overweight_walk_k
        ),
        (-0.005, 0.00001, 0.00005, 5.0)
    );
    assert_eq!(
        (c.accel_k, c.sprint_k, c.jump_power, c.jump_weight_power),
        (3.0, 75.0, 0.01, 0.05)
    );
    assert_eq!((c.overweight_jump_k, c.max_power_leak_speed), (5.0, 0.0));
    assert_eq!(
        (c.satiety_v, c.satiety_power_v, c.satiety_health_v),
        (0.000015, 0.000055, 0.0001)
    );
    assert_eq!(
        (
            c.radiation_v,
            c.radiation_health_v,
            c.psy_health_v,
            c.morale_v,
            c.alcohol_v
        ),
        (0.0001, 0.004, 0.001, 0.0001, -0.0003)
    );
    assert_eq!(
        (c.bleeding_v, c.health_restore_v, c.max_walk_weight),
        (0.0005, 0.0, 60.0)
    );
    // Each begin ≤ its end (the read checks it).
    for (h, want) in [
        (c.limping_health, (0.1, 0.2)),
        (c.limping_power, (0.1, 0.2)),
        (c.cant_walk_power, (0.01, 0.10)),
        (c.cant_sprint_power, (0.20, 0.30)),
    ] {
        assert_eq!((h.begin, h.end), want);
    }
    assert_eq!(
        InventoryConfig::read(system).unwrap(),
        InventoryConfig {
            max_weight: 50.0,
            max_belt: 5
        }
    );
    // Invariant: every [spawn] section exists and has a slot, distinct with
    // the PDA's.
    let mut slots = BTreeSet::new();
    for s in [
        "detector_simple",
        "device_torch",
        "novice_outfit",
        "wpn_binoc",
        "device_pda",
    ] {
        assert!(slots.insert(ItemConfig::read(system, s).unwrap().slot_number().unwrap()));
    }
    let load = state.load();
    assert_eq!(
        (load.carry_limit, load.walk_limit, load.power_loss),
        (50.0, 60.0, 1.0)
    );
}

/// Section 7: the derived rates with the New Game load.
fn check_rates(system: &volya_engine::core::config::Config, state: &CharacterState) {
    let c = ConditionConfig::read(system, "actor_condition").unwrap();
    let load = state.load();
    let rate = |moving, runs, sprints| {
        let mut s = Conditions::new_game(1.0);
        s.power = 0.5;
        s.update(
            &c,
            1.0,
            Activity {
                moving,
                runs,
                sprints,
            },
            &load,
        );
        s.power - 0.5
    };
    near(rate(false, false, false), 0.005055, 1e-7);
    near(rate(true, false, false), 0.0000407, 1e-7);
    near(rate(true, true, false), 0.0000121, 1e-7);
    near(rate(true, true, true), -0.0010175, 1e-7);
    let mut s = Conditions::new_game(1.0);
    s.jump(&c, &load);
    near(1.0 - s.power, 0.0143, 1e-6);
    // Sprint endurance from full power to below 0.2.
    let mut s = Conditions::new_game(1.0);
    let mut t = 0.0;
    while !s.cant_sprint {
        s.update(
            &c,
            0.1,
            Activity {
                moving: true,
                runs: true,
                sprints: true,
            },
            &load,
        );
        s.evaluate_thresholds(&c);
        t += 0.1;
    }
    near(t, 786.2, 0.2);
    // Satiety from 1 to 0 in 66,666.7 game seconds.
    let mut s = Conditions::new_game(1.0);
    s.update(&c, 66_666.0, Activity::default(), &load);
    assert!(s.satiety > 0.0);
    s.update(&c, 1.0, Activity::default(), &load);
    assert_eq!(s.satiety, 0.0);
}

/// Section 5's table, the invariant on portions, and section 7's vodka and
/// medkit.
fn check_eatables(system: &volya_engine::core::config::Config, state: &CharacterState) {
    type Row = (&'static str, &'static str, [f32; 5], i32, f32, Option<f32>);
    let table: [Row; 10] = [
        (
            "bread",
            "II_FOOD",
            [0.05, 0.0, 0.2, 0.0, 0.0],
            -1,
            0.0,
            None,
        ),
        (
            "kolbasa",
            "II_FOOD",
            [0.1, 0.0, 0.3, 0.0, 0.0],
            -1,
            0.0,
            None,
        ),
        (
            "conserva",
            "II_FOOD",
            [0.15, 0.0, 0.4, 0.0, 0.0],
            -1,
            0.0,
            None,
        ),
        (
            "vodka",
            "II_BOTTL",
            [0.01, 0.0, 0.01, -0.3, 0.0],
            -1,
            0.0,
            Some(0.1),
        ),
        (
            "energy_drink",
            "II_BOTTL",
            [0.0, 1.0, 0.0, 0.0, 0.0],
            1,
            0.2,
            Some(0.0),
        ),
        (
            "medkit",
            "II_MEDKI",
            [0.6, 0.0, 0.0, -0.1, 0.3],
            1,
            0.1,
            None,
        ),
        (
            "medkit_army",
            "II_MEDKI",
            [1.0, 0.0, 0.0, -0.1, 1.0],
            1,
            0.1,
            None,
        ),
        (
            "medkit_scientic",
            "II_MEDKI",
            [1.0, 0.0, 0.0, -1.0, 0.3],
            1,
            0.1,
            None,
        ),
        (
            "bandage",
            "II_BANDG",
            [0.1, 0.0, 0.0, 0.0, 1.0],
            1,
            0.0,
            None,
        ),
        (
            "antirad",
            "II_ANTIR",
            [0.0, 0.0, 0.0, -1.0, 0.0],
            1,
            0.0,
            None,
        ),
    ];
    for (section, class, [h, p, s, r, w], portions, max_power, alcohol) in table {
        let item = ItemConfig::read(system, section).unwrap();
        assert_eq!(item.class_id.as_str(), class, "{section}");
        let e = item.eatable.unwrap();
        assert_eq!(
            [
                e.eat_health,
                e.eat_power,
                e.eat_satiety,
                e.eat_radiation,
                e.wounds_heal_perc
            ],
            [h, p, s, r, w],
            "{section}"
        );
        assert_eq!(
            (e.eat_portions_num, e.eat_max_power, e.eat_alcohol),
            (portions, max_power, alcohol),
            "{section}"
        );
        assert!(e.eat_portions_num <= 1);
        // Food and drink go to the backpack, despite their slot.
        if matches!(class, "II_FOOD" | "II_BOTTL") {
            assert_eq!(item.slot, Some(4), "{section}");
        } else {
            assert_eq!(item.slot, None, "{section}");
        }
    }
    let c = ConditionConfig::read(system, "actor_condition").unwrap();
    let use_item = |section: &str, dt: f32| {
        let mut s = state.clone();
        let load = s.load();
        let id = 1000;
        let place = s.inventory.take(InventoryItem::create(
            ItemConfig::read(system, section).unwrap(),
            id,
            None,
            0,
            "actor",
        ));
        assert_eq!(place, Place::Backpack, "{section}");
        s.eat(id).unwrap();
        assert!(s.inventory.item(id).is_none(), "{section} used up");
        let before = s.conditions.clone();
        s.conditions.update(&c, dt, Activity::default(), &load);
        (before, s.conditions)
    };
    let (before, after) = use_item("vodka", 50.0);
    near(before.alcohol, 0.1, 1e-7);
    near(after.alcohol, 0.1 - 0.0003 * 50.0, 1e-6);
    let (_, after) = use_item("medkit", 0.0);
    assert_eq!(
        (
            after.health,
            after.radiation,
            after.max_power,
            after.satiety
        ),
        (1.0, 0.0, 1.0, 1.0)
    );
}
