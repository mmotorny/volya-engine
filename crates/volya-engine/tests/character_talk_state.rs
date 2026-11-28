// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The characters' talk state on the Russian edition after a New Game
//! (BYOA; docs/behavior/character-talk-state.md, cited by section): the
//! class ids that can talk (1.2), the start dialog each character takes as
//! it comes online (6.1), the trader's worked example (8) and the start
//! dialogs no dialog file defines (6.3). The talk state's own rules are
//! the script crate's checks (`selftest::character_talk_state`). Skips
//! when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeMap;

use volya_engine::characters::Characters;
use volya_engine::core::character_identity::trader_like;
use volya_engine::core::particles::random::Shared;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level::new_game_offline_simulation;
use volya_engine::scripts::{ClientObject, engine_state};
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

#[test]
fn the_characters_talk_state_after_a_new_game() {
    with_game(|tree, data| {
        let sim = new_game_offline_simulation(tree, data, &Shared::new(1)).unwrap();
        let mut characters = Characters::new(tree, data);
        let class = |section: &str| {
            let class = data.system.get(section, "class").flatten();
            class.map(|c| c.trim().to_owned()).unwrap_or_default()
        };

        // 1.2: the records of the class ids that can talk.
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for o in sim.objects() {
            *counts.entry(class(&o.record.header.section)).or_default() += 1;
        }
        let count = |id: &str| counts.get(id).copied().unwrap_or(0);
        assert_eq!(count("AI_STL_S"), 1_073);
        assert_eq!(count("AI_TRD_S"), 1);
        assert_eq!(count("AI_STL"), 0);
        assert_eq!(count("AI_TRADE"), 0);
        assert_eq!(class("m_izgoy"), "AI_STL");

        // 6.1: every stalker and trader comes online with its specific
        // character's start dialog, an undefined one included (6.3).
        let mut with_dialog = 0;
        for o in sim.objects() {
            let Some(t) = trader_like(&o.record) else {
                continue;
            };
            let object = ClientObject {
                id: o.id,
                record: o.record.clone(),
                model: None,
                physics: None,
                shell: None,
                door: false,
            };
            let state = engine_state(&mut characters, &object).unwrap();
            let id = t.specific_character.to_string();
            let d = characters.specific_character(&id).unwrap();
            assert_eq!(state.start_dialog, d.start_dialog.as_bytes(), "{id}");
            with_dialog += usize::from(!state.start_dialog.is_empty());
        }
        assert!(with_dialog > 0);

        // 8: the trader.
        let trader = sim.by_name("escape_trader").unwrap();
        assert_eq!(class(&trader.record.header.section), "AI_TRD_S");
        let object = ClientObject {
            id: trader.id,
            record: trader.record.clone(),
            model: None,
            physics: None,
            shell: None,
            door: false,
        };
        let state = engine_state(&mut characters, &object).unwrap();
        assert_eq!(state.start_dialog, b"escape_trader_start_dialog");

        // 6.3: start dialogs no listed dialog file defines, kept as read.
        for (id, dialog) in [
            ("bar_psih", "bar_bar_psih_start_pain"),
            ("bar_svoboda_attack_commander", "bar_svoboda_volkodav_start"),
            ("gar_bandit_novice_wounded", "gar_wounded_bandit"),
        ] {
            let d = characters.specific_character(id).unwrap();
            assert_eq!(d.start_dialog, dialog, "{id}");
        }
    });
}
