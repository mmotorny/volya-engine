// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The characters' identities after a New Game on the Russian edition
//! (BYOA; docs/behavior/character-identity.md): the trader's worked example
//! (section 8) value by value, and every stalker's and trader's identity
//! against the rules of sections 2–4 and the counts of section 9. Skips
//! when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet};

use volya_engine::characters::{self, Characters};
use volya_engine::core::character_identity::{GENERATE_NAME, model_name, trader_like};
use volya_engine::core::particles::random::Shared;
use volya_engine::core::simulation::Simulation;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level::new_game_offline_simulation;
use volya_engine_formats::character_descriptions::NOT_SET;
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

/// Each character's (name, specific character, character name, money).
fn identities(sim: &Simulation) -> Vec<(String, String, String, u32)> {
    sim.objects()
        .filter_map(|o| {
            let t = trader_like(&o.record)?;
            Some((
                o.record.header.name.to_string(),
                t.specific_character.to_string(),
                t.character_name.to_string(),
                t.money,
            ))
        })
        .collect()
}

#[test]
fn a_new_game_chooses_every_character_s_identity() {
    with_game(|tree, data| {
        let sim = new_game_offline_simulation(tree, data, &Shared::new(1)).unwrap();
        let communities = characters::communities(tree, data).unwrap();
        let mut characters = Characters::new(tree, data);

        // Section 8: the trader, record 84.
        let trader = sim.by_name("escape_trader").unwrap();
        assert_eq!(trader.spawn_index, Some(84));
        let Object::Trader(o) = &trader.record.object else {
            panic!("the trader's kind");
        };
        let t = &o.trader;
        assert_eq!(t.character_profile.as_str(), "escape_trader");
        assert_eq!(t.specific_character.as_str(), "escape_trader");
        assert_eq!(communities[13].0, "trader");
        assert_eq!((t.community, t.rank, t.reputation), (13, 330, 23));
        assert_eq!(
            t.character_name.as_str(),
            data.strings.get("escape_trader_name")
        );
        assert_ne!(t.character_name.as_str(), "escape_trader_name");
        assert_eq!(t.money, 100_000);
        assert_eq!(o.visual.model.as_str(), "actors\\trader\\trader");
        let d = characters.specific_character("escape_trader").unwrap();
        assert_eq!(d.icon, "ui_npc_u_trader");

        // Sections 4.2 and 9: every stalker and trader.
        let in_use = sim.in_use();
        let mut profiles: BTreeMap<String, usize> = BTreeMap::new();
        let mut explicit = 0;
        let mut generated = 0;
        let mut distinct = BTreeSet::new();
        let mut kinds = (0, 0);
        for o in sim.objects() {
            let Some(t) = trader_like(&o.record) else {
                continue;
            };
            let what = o.record.header.name.as_str();
            *profiles.entry(t.character_profile.to_string()).or_default() += 1;
            let id = t.specific_character.as_str();
            assert!(in_use.contains(id), "{what}: {id} not in use");
            distinct.insert(id.to_owned());
            assert!(t.community >= 0 && (t.community as usize) < communities.len());
            assert!(t.rank != NOT_SET && t.reputation != NOT_SET, "{what}");
            assert!(!t.character_name.contains(GENERATE_NAME), "{what}");
            assert!(!t.character_name.is_empty(), "{what}");
            let profile = characters.profile(&t.character_profile).unwrap().clone();
            let d = characters.specific_character(id).unwrap().clone();
            match &profile.specific_character {
                Some(s) => {
                    assert_eq!(s, id, "{what}");
                    explicit += 1;
                }
                None => {
                    let class = profile.class.as_deref().unwrap();
                    assert!(!d.no_random, "{what}: {id}");
                    assert!(d.classes.iter().any(|c| c == class), "{what}: {id}");
                }
            }
            // The supplies choice's description gives the community, the
            // model (3 steps 2, 3) and a name from its `name`.
            assert_eq!(communities[t.community as usize].0, d.community, "{what}");
            let model = o.record.object.visual().unwrap().model.as_str();
            assert_eq!(model, model_name(&d.visual), "{what}");
            let text = data.strings.get(&d.name);
            if text.contains(GENERATE_NAME) {
                generated += 1;
            } else {
                assert_eq!(t.character_name.as_str(), text, "{what}");
            }
            if d.money.min != 0 && d.money.max != 0 {
                let range = d.money.min..d.money.max.max(d.money.min + 1);
                assert!(range.contains(&t.money), "{what}: {}", t.money);
            }
            match &o.record.object {
                Object::Stalker(s) => {
                    kinds.0 += 1;
                    let team = communities[t.community as usize].1 as u8;
                    assert_eq!(s.monster.creature.team, team, "{what}");
                    // The section's `terrain` mask first (D18), then the
                    // description's `terrain_sect`.
                    let masks = &o.travel.as_ref().unwrap().config.masks;
                    assert_eq!(masks[0], [255; 4], "{what}");
                }
                _ => kinds.1 += 1,
            }
        }
        assert_eq!(kinds, (1_073, 1));
        assert_eq!(profiles.values().sum::<usize>(), 1_074);
        assert_eq!(profiles.len(), 205);
        assert_eq!(explicit, 14);
        assert!(generated > 0);
        // 4.2 step 4: the registry lists every id chosen, the spawn-file
        // choices' included.
        assert!(in_use.len() >= distinct.len());
        assert!(in_use.entries().all(|(_, v)| v == 1));

        // The same generator state gives the same identities; another,
        // others.
        let again = new_game_offline_simulation(tree, data, &Shared::new(1)).unwrap();
        assert_eq!(identities(&again), identities(&sim));
        let other = new_game_offline_simulation(tree, data, &Shared::new(2)).unwrap();
        assert_ne!(identities(&other), identities(&sim));
        let trader = |s: &Simulation| {
            identities(s)
                .into_iter()
                .find(|i| i.0 == "escape_trader")
                .unwrap()
        };
        assert_eq!(trader(&other), trader(&sim));
    });
}
