// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game's supplies and ids on the Russian edition (BYOA;
//! docs/behavior/new-game-supplies.md): the ids of every created object
//! (3.2), the trader's supplies (3.2) and the novices of the beginners'
//! village (section 4). Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeMap;

use volya_engine::core::character_identity::trader_like;
use volya_engine::core::particles::random::Shared;
use volya_engine::core::simulation::{NO_ID, SimObject, Simulation};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level::new_game_offline_simulation;
use volya_engine_formats::object::Object;
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

/// The supplies `owner` carries, in increasing id.
fn supplies(sim: &Simulation, owner: u16) -> Vec<&SimObject> {
    sim.objects()
        .filter(|o| o.record.header.parent_id == owner && o.spawn_index.is_none())
        .collect()
}

/// The PDA's original owner and specific character.
fn pda(o: &SimObject) -> (u16, String) {
    let Object::Pda(p) = &o.record.object else {
        panic!("{}: not a PDA", o.record.header.name);
    };
    (p.original_owner_id, p.specific_character.to_string())
}

/// Section 4's table: the entries of the novices' specific characters in
/// the order they are read, with whether their probability is 1.
fn novice_entries(character: &str) -> Vec<(&'static str, bool)> {
    let n = character
        .strip_prefix("esc_stalker_novice_default")
        .unwrap_or_else(|| panic!("{character}: not a novice"));
    let (shotgun, drugs) = match n {
        "1" | "3" | "5" => (true, false),
        "2" | "4" => (false, false),
        "6" => (true, true),
        "9" | "10" => (false, true),
        _ => panic!("{character}: not one of the class's eight"),
    };
    let mut e = vec![(
        if shotgun {
            "ammo_12x70_buck"
        } else {
            "ammo_9x18_fmj"
        },
        true,
    )];
    if drugs {
        e.push(("bandage", false));
    }
    for s in [
        "bread",
        "device_torch",
        "energy_drink",
        "guitar_a",
        "harmonica_a",
        "kolbasa",
    ] {
        e.push((s, false));
    }
    if drugs {
        e.push(("medkit", false));
    }
    e.push(("vodka", false));
    e.push((if shotgun { "wpn_bm16" } else { "wpn_pm" }, true));
    e
}

#[test]
fn a_new_game_supplies_every_object() {
    with_game(|tree, data| {
        let sim = new_game_offline_simulation(tree, data, &Shared::new(1)).unwrap();

        // 3.2: ids 0 for the player, then 1, 2, … in creation order; the
        // player's items (held by its character state) take theirs right
        // after the objects of records 0 … 62.
        let actor_items = sim.actor_items();
        assert_eq!(actor_items.len(), 5);
        let ids: Vec<u16> = sim.objects().map(|o| o.id).collect();
        let expected: Vec<u16> = (0..ids.len() as u16 + 5)
            .filter(|id| !actor_items.contains(id))
            .collect();
        assert_eq!(ids, expected, "ids without gaps");
        let after = sim.object(actor_items.end).unwrap();
        assert_eq!(after.spawn_index, Some(64), "the record after the player");
        let mut last_record = None;
        for o in sim.objects() {
            match o.spawn_index {
                // Records in increasing index, the player (id 0) aside.
                Some(_) if o.id == 0 => {}
                Some(i) => {
                    assert!(last_record < Some(i), "{}", o.record.header.name);
                    last_record = Some(i);
                }
                // A supply right after its owner and its earlier supplies.
                None => {
                    let parent = o.record.header.parent_id;
                    assert_ne!(parent, NO_ID, "{}", o.record.header.name);
                    let owner = sim.object(parent).unwrap();
                    assert!(owner.id < o.id);
                    assert!(
                        (owner.id + 1..o.id).all(|id| sim
                            .object(id)
                            .unwrap()
                            .spawn_index
                            .is_none()),
                        "{}: after another record",
                        o.record.header.name
                    );
                }
            }
        }
        // Every stalker and trader has its PDA first, holding its specific
        // character.
        let mut characters = 0;
        for o in sim.objects() {
            let Some(t) = trader_like(&o.record) else {
                continue;
            };
            characters += 1;
            let what = o.record.header.name.as_str();
            let items = supplies(&sim, o.id);
            let pdas: Vec<_> = (items.iter())
                .filter(|i| i.record.header.section == "device_pda")
                .collect();
            assert_eq!(pdas.len(), 1, "{what}");
            assert_eq!(
                pda(pdas[0]),
                (o.id, t.specific_character.to_string()),
                "{what}"
            );
        }
        let records = sim.objects().filter(|o| o.spawn_index.is_some()).count();
        eprintln!(
            "{} objects: {records} records, {} supplies, {characters} characters",
            sim.len(),
            sim.len() - records
        );
        assert!(sim.len() <= 34_018, "3.1: at most 34,018 objects");

        // 3.2: the trader, record 84: his custom data's items in sorted
        // order, his PDA, his description's knife.
        let trader = sim.by_name("escape_trader").unwrap();
        assert_eq!(trader.spawn_index, Some(84));
        let t = trader.id;
        let sections: Vec<&str> = (1..=4)
            .map(|k| sim.object(t + k).unwrap().record.header.section.as_str())
            .collect();
        assert_eq!(
            sections,
            ["ammo_9x18_fmj", "wpn_pm", "device_pda", "wpn_knife"]
        );
        assert_eq!(pda(sim.object(t + 3).unwrap()), (t, "escape_trader".into()));
        assert_eq!(sim.object(t + 5).unwrap().spawn_index, Some(85));
        // A full box (script-api-inventory.md 7.5).
        let Object::Ammunition(a) = &sim.object(t + 1).unwrap().record.object else {
            panic!("the trader's ammunition");
        };
        assert_eq!(a.rounds_left, 20);

        // Section 4: the novices of the beginners' village.
        let custom = [
            Some("conserva"),
            Some("bandage"),
            Some("bread"),
            None,
            Some("kolbasa"),
            None,
            None,
        ];
        let mut guitars = 0;
        let mut drawn = BTreeMap::new();
        for (k, custom) in custom.into_iter().enumerate() {
            let name = format!("esc_lager{}", k + 1);
            let novice = sim.by_name(&name).unwrap();
            assert_eq!(novice.spawn_index, Some(93 + k as u32), "{name}");
            let n = novice.id;
            let items = supplies(&sim, n);
            // Consecutive ids from n + 1, the next record after them.
            for (i, item) in items.iter().enumerate() {
                assert_eq!(item.id, n + 1 + i as u16, "{name}");
            }
            let next = sim.object(n + 1 + items.len() as u16).unwrap();
            assert_eq!(next.spawn_index, Some(94 + k as u32), "{name}");
            // 1. Its PDA.
            let (owner, character) = pda(items[0]);
            assert_eq!(owner, n);
            // 2. Its description's entries in the table's order, those of
            // probability 1 all there.
            let mut sections: Vec<&str> = (items[1..].iter())
                .map(|i| i.record.header.section.as_str())
                .collect();
            // 3. Its custom data's item last.
            if let Some(c) = custom {
                assert_eq!(sections.pop(), Some(c), "{name}");
            }
            let entries = novice_entries(&character);
            let mut at = 0;
            for s in &sections {
                let k = at
                    + (entries[at..].iter().position(|(e, _)| e == s))
                        .unwrap_or_else(|| panic!("{name} ({character}): {s} out of order"));
                assert!(
                    entries[at..k].iter().all(|(_, certain)| !certain),
                    "{name} ({character}): {s} after {:?}",
                    &entries[at..k]
                );
                at = k + 1;
            }
            let rest = &entries[at..];
            assert!(
                rest.iter().all(|(_, c)| !c),
                "{name} ({character}): {rest:?}"
            );
            guitars += sections.iter().filter(|s| **s == "guitar_a").count();
            drawn.insert(name, (character, sections.join(" ")));
        }
        eprintln!("the novices' supplies, {guitars} guitars: {drawn:#?}");
    });
}
