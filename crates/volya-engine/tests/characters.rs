// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The character descriptions of the Russian edition (BYOA): every profile
//! and specific character read as the program reads them, against the
//! counts docs/formats/character-descriptions.md measured, and its worked
//! example `escape_trader` (section 9) field by field. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, HashSet};

use volya_engine::characters::Characters;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine_formats::character_descriptions::{DEFAULT_ICON, Money, NOT_SET};
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

/// How many of `items` give each key.
fn tally<'a>(items: impl IntoIterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for i in items {
        *counts.entry(i).or_default() += 1;
    }
    counts
}

#[test]
fn every_profile_and_specific_character_reads() {
    with_game(|tree, data| {
        let files = &data.characters;
        // Section 1: the lists.
        assert_eq!(files.profiles.files(), ["npc_profile"]);
        assert_eq!(
            files.specific_characters.files(),
            [
                "character_desc_general",
                "character_desc_simulation",
                "character_desc_escape",
                "character_desc_garbage",
                "character_desc_agroprom",
                "character_desc_darkvalley",
                "character_desc_deadcity",
                "character_desc_bar",
                "character_desc_yantar",
                "character_desc_military",
                "character_desc_radar",
                "character_desc_pripyat",
                "character_desc_aes",
                "character_desc_kishka",
                "character_desc_sarcofag",
            ]
        );

        // Section 2: entries per file.
        let profiles = &files.profiles;
        let specifics = &files.specific_characters;
        assert_eq!(profiles.len(), 474);
        let mut per_file = vec![0; specifics.files().len()];
        for i in 0..specifics.len() {
            per_file[specifics.entry(i).file] += 1;
        }
        assert_eq!(
            per_file,
            [4, 251, 33, 23, 24, 49, 20, 80, 26, 26, 68, 28, 13, 79, 19]
        );
        assert_eq!(specifics.len(), 743);

        // Section 3.1: ids.
        let capitals = |r: &volya_engine_formats::character_descriptions::Registry| {
            (0..r.len())
                .filter(|&i| r.id(i).bytes().any(|b| b.is_ascii_uppercase()))
                .count()
        };
        assert_eq!((capitals(profiles), capitals(specifics)), (55, 51));
        let specific_ids: HashSet<&str> = (0..specifics.len()).map(|i| specifics.id(i)).collect();
        let both = (0..profiles.len())
            .filter(|&i| specific_ids.contains(profiles.id(i)))
            .count();
        assert_eq!(both, 353);

        let mut characters = Characters::new(tree, data);

        // Section 4: every profile.
        let mut named = Vec::new();
        let mut classes = Vec::new();
        for i in 0..profiles.len() {
            let p = characters.profile(profiles.id(i)).unwrap().clone();
            assert_eq!(p.id, profiles.id(i));
            match &p.specific_character {
                Some(s) => {
                    assert_eq!(
                        (p.class.as_deref(), p.rank, p.reputation),
                        (None, NOT_SET, NOT_SET)
                    );
                    named.push((p.id.clone(), s.clone()));
                }
                None => {
                    assert_eq!((p.rank, p.reputation), (NOT_SET, NOT_SET));
                    classes.push(p.class.clone().expect("every profile has a class"));
                }
            }
        }
        assert_eq!((named.len(), classes.len()), (18, 456));
        // The spec says 16 (#1011); every one is the profile's own id.
        assert_eq!(named.iter().filter(|(p, s)| p == s).count(), 18);
        for (_, s) in &named {
            characters.specific_character(s).unwrap();
        }
        assert!(
            classes
                .iter()
                .all(|c| !c.bytes().any(|b| b.is_ascii_uppercase()))
        );

        // Section 5: every specific character.
        let all: Vec<_> = (0..specifics.len())
            .map(|i| characters.specific_character_at(i).unwrap().clone())
            .collect();
        let specific_classes: HashSet<&str> = all
            .iter()
            .flat_map(|c| c.classes.iter().map(String::as_str))
            .collect();
        let classless: Vec<&str> = tally(classes.iter().map(String::as_str))
            .into_keys()
            .filter(|c| !specific_classes.contains(c))
            .collect();
        assert_eq!(classless, ["esc_plemiannik", "gar_newbie_in_tonnel"]);

        // 5.1.
        let mut no_random: Vec<&str> = all
            .iter()
            .filter(|c| c.no_random)
            .map(|c| c.id.as_str())
            .collect();
        no_random.sort_unstable();
        assert_eq!(
            no_random,
            ["actor", "bar_barman", "escape_trader", "mp_actor"]
        );
        assert_eq!(all.iter().filter(|c| c.team_default).count(), 738);
        let val = characters
            .specific_character("val_monolith_regular_default1")
            .unwrap();
        assert!(!val.team_default);

        // 5.2: one class each.
        assert!(all.iter().all(|c| c.classes.len() == 1));

        // 5.3.
        assert_eq!(all.iter().filter(|c| c.supplies.is_empty()).count(), 118);
        assert!(
            all.iter()
                .filter(|c| !c.supplies.is_empty())
                .all(|c| c.supplies.starts_with("[spawn]") && c.supplies.contains('\n'))
        );

        // 5.4.
        let generated = |kind: &str| {
            let name = format!("GENERATE_NAME_{kind}");
            all.iter().filter(|c| c.name == name).count()
        };
        assert_eq!(
            all.iter()
                .filter(|c| c.name.starts_with("GENERATE_NAME_"))
                .count(),
            520
        );
        assert_eq!(
            [
                "stalker",
                "sergeant",
                "bandit",
                "private",
                "lieutenant",
                "captain",
                "science"
            ]
            .map(generated),
            [344, 96, 39, 17, 17, 5, 2]
        );
        assert_eq!(all.iter().filter(|c| c.name == "Test").count(), 115);
        // Missing in two, empty in 115.
        assert_eq!(all.iter().filter(|c| c.bio.is_empty()).count(), 117);
        for id in ["actor", "mp_actor"] {
            assert_eq!(characters.specific_character(id).unwrap().bio, "");
        }
        // Not in the table: kept as written.
        let kruglov = all.iter().filter(|c| c.bio == "yan_kruglov_bio").count();
        let soldier = all
            .iter()
            .filter(|c| c.bio == "val_soldier_regular_bio.")
            .count();
        assert!(kruglov >= 1 && soldier >= 1);

        // 5.5.
        assert_eq!(
            tally(all.iter().map(|c| c.community.as_str())),
            BTreeMap::from([
                ("stalker", 334),
                ("monolith", 87),
                ("bandit", 68),
                ("military", 57),
                ("zombied", 53),
                ("dolg", 49),
                ("killer", 33),
                ("freedom", 23),
                ("arena_enemy", 18),
                ("ecolog", 15),
                ("actor", 2),
                ("actor_dolg", 2),
                ("trader", 2),
            ])
        );
        assert_eq!(all.iter().map(|c| c.rank).min(), Some(0));
        assert_eq!(all.iter().map(|c| c.rank).max(), Some(1000));
        assert_eq!(all.iter().map(|c| c.reputation).min(), Some(-1000));
        assert_eq!(all.iter().map(|c| c.reputation).max(), Some(198));

        // 5.6.
        let money = |m: Money| (m.min, m.max, m.infinitive);
        let with_money: Vec<_> = all.iter().filter(|c| c.money != Money::default()).collect();
        assert_eq!(with_money.len(), 226);
        let mut amounts = BTreeMap::new();
        for c in &with_money {
            *amounts.entry(money(c.money)).or_insert(0) += 1;
        }
        assert_eq!(
            amounts,
            BTreeMap::from([
                ((600, 2000, false), 112),
                ((200, 600, false), 47),
                ((2000, 5000, false), 40),
                ((5000, 10000, false), 22),
                ((100000, 110000, true), 3),
                ((100000, 100000, true), 1),
                ((0, 200, false), 1),
            ])
        );
        let infinitive: Vec<&str> = with_money
            .iter()
            .filter(|c| c.money.infinitive)
            .map(|c| c.id.as_str())
            .collect();
        let mut infinitive = infinitive;
        infinitive.sort_unstable();
        assert_eq!(
            infinitive,
            [
                "bar_barman",
                "bar_dolg_petrenko",
                "escape_trader",
                "yan_general_ecolog"
            ]
        );

        // 5.7.
        assert_eq!(all.iter().filter(|c| c.icon == DEFAULT_ICON).count(), 101);
        assert_eq!(tally(all.iter().map(|c| c.icon.as_str())).len(), 113);
        let crouch = tally(all.iter().map(|c| match c.crouch_type {
            -1 => "-1",
            0 => "0",
            1 => "1",
            _ => "other",
        }));
        assert_eq!(crouch, BTreeMap::from([("-1", 236), ("0", 436), ("1", 71)]));
        assert!(all.iter().all(|c| c.panic_threshold == 0.0));
        assert_eq!(
            all.iter()
                .filter(|c| c.hit_probability_factor == 1.5)
                .count(),
            1
        );
        assert!(
            all.iter()
                .all(|c| [1.0, 1.5].contains(&c.hit_probability_factor))
        );
        assert_eq!(
            tally(all.iter().map(|c| c.critical_wound_weights.as_str())),
            BTreeMap::from([("55,30,15", 519), ("75,25,25", 1), ("1", 223)])
        );
        assert_eq!(
            characters
                .specific_character("sim_soldier_specnaz_default2")
                .unwrap()
                .critical_wound_weights,
            "75,25,25"
        );
        assert_eq!(
            tally(all.iter().map(|c| c.terrain_sect.as_str())),
            BTreeMap::from([
                ("", 91),
                ("stalker_terrain", 267),
                ("stalker_military_terrain", 137),
                ("stalker_bandit_terrain", 74),
                ("stalker_zombied_terrain", 53),
                ("stalker_dolg_terrain", 50),
                ("stalker_killer_terrain", 33),
                ("stalker_freedom_terrain", 23),
                ("stalker_ecolog_terrain", 15),
            ])
        );
        let voices = tally(all.iter().map(|c| c.snd_config.as_str()));
        assert_eq!(voices.get(""), Some(&8));
        assert_eq!(voices.len(), 33);
        assert_eq!(
            voices.get("characters_voice\\human_01\\ military\\"),
            Some(&78)
        );
        assert_eq!(tally(all.iter().map(|c| c.visual.as_str())).len(), 119);
        assert!(all.iter().all(|c| c.npc_config.is_empty()));

        // Section 6.
        let starts = tally(all.iter().map(|c| c.start_dialog.as_str()));
        assert_eq!(
            (
                starts.get(""),
                starts.get("hello_dialog"),
                starts.get("dm_hello_dialog")
            ),
            (Some(&143), Some(&295), Some(&257))
        );
        assert_eq!(
            all.iter().filter(|c| !c.start_dialog.is_empty()).count(),
            600
        );
        assert_eq!(
            all.iter().filter(|c| !c.actor_dialogs.is_empty()).count(),
            394
        );
        assert_eq!(all.iter().map(|c| c.actor_dialogs.len()).max(), Some(14));
        let actor = tally(
            all.iter()
                .flat_map(|c| c.actor_dialogs.iter().map(String::as_str)),
        );
        assert_eq!(actor.len(), 82);
        assert_eq!(actor.get("dm_help_wounded_medkit_dialog"), Some(&364));
        assert_eq!(actor.get("dm_cool_info_dialog"), Some(&359));
        assert!(all.iter().all(|c| {
            let ids: HashSet<&String> = c.actor_dialogs.iter().collect();
            ids.len() == c.actor_dialogs.len() && !ids.contains(&String::new())
        }));
        let psih = characters.specific_character("bar_psih").unwrap();
        assert_eq!(psih.start_dialog, "bar_bar_psih_start_pain");

        // Section 7: indented include lines stay text.
        let krot = characters.specific_character("agr_krot").unwrap();
        assert!(krot.supplies.contains("#include \"gameplay\\character_"));
    });
}

#[test]
fn the_traders_profile_and_specific_character() {
    with_game(|tree, data| {
        let mut characters = Characters::new(tree, data);
        // Section 9: the profile.
        let p = characters.profile("escape_trader").unwrap().clone();
        assert_eq!(p.specific_character.as_deref(), Some("escape_trader"));
        assert_eq!((p.class, p.rank, p.reputation), (None, NOT_SET, NOT_SET));

        // The specific character, the first entry of `character_desc_escape.xml`.
        let at = data
            .characters
            .specific_characters
            .find("escape_trader")
            .unwrap();
        assert_eq!(
            data.characters.specific_characters.files()[at.file],
            "character_desc_escape"
        );
        assert_eq!(at.position, 0);
        let c = characters
            .specific_character("escape_trader")
            .unwrap()
            .clone();
        assert_eq!(c.id, "escape_trader");
        assert!(c.no_random && !c.team_default);
        assert_eq!(c.name, "escape_trader_name");
        assert_eq!(data.strings.get(&c.name), "Сидорович");
        assert_eq!(c.icon, "ui_npc_u_trader");
        assert_eq!(c.bio, data.strings.get("escape_trader_bio"));
        assert_ne!(c.bio, "escape_trader_bio");
        assert_eq!(c.classes, ["trader"]);
        assert_eq!(c.community, "trader");
        assert_eq!(c.visual, "actors\\trader\\trader");
        assert_eq!((c.rank, c.reputation), (330, 23));
        assert_eq!(
            c.money,
            Money {
                min: 100_000,
                max: 100_000,
                infinitive: true
            }
        );
        assert_eq!(c.supplies, "[spawn] \n wpn_knife \n");
        assert_eq!(c.start_dialog, "escape_trader_start_dialog");
        assert_eq!(
            c.actor_dialogs,
            [
                "escape_trader_talk_info",
                "escape_trader_jobs",
                "tm_trader_dialog",
                "tm_trader_reward",
                "escape_trader_done_blockpost_box",
            ]
        );
        // Not written: defaults.
        assert!(c.snd_config.is_empty() && c.npc_config.is_empty() && c.terrain_sect.is_empty());
        assert_eq!(c.crouch_type, 0);
        assert_eq!((c.panic_threshold, c.hit_probability_factor), (0.0, 1.0));
        assert_eq!(c.critical_wound_weights, "1");
    });
}

#[test]
fn an_undefined_id_is_a_fatal_error() {
    with_game(|tree, data| {
        let mut characters = Characters::new(tree, data);
        let e = characters
            .specific_character("no_such_character")
            .unwrap_err();
        let check = volya_engine_formats::character_descriptions::FailedCheck::of(&e).unwrap();
        assert_eq!(check.description, "item not found, id");
        assert_eq!(check.log.len(), 743);
        assert_eq!(
            check.log[0],
            format!("[0]=[{}]", data.characters.specific_characters.id(0))
        );
    });
}
