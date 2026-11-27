// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Hits on the player character from the Russian edition (BYOA): the
//! verification facts of docs/behavior/player-hits.md section 8; a New
//! Game hit with each type, its health, conditions, outfit and wounds
//! against section 8's table; the level's hit path (difficulty, slowdown,
//! bleeding, healing). Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeSet;

use volya_engine::core::character::condition::Activity;
use volya_engine::core::character::hits::{BoneProtection, Hit, HitType, NAMES, TYPE_COUNT};
use volya_engine::core::character::inventory::ItemConfig;
use volya_engine::core::input::Input;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine_formats::object::Kind;
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

fn float(data: &GameData, section: &str, key: &str) -> f32 {
    let v = data.system.read(section, key).unwrap();
    volya_engine::core::config::value::float(v)
}

#[test]
fn hit_data_from_game_data() {
    with_game(|_, data| {
        let system = &data.system;
        let read = |s: &str, k: &str| system.read(s, k).unwrap().trim().to_owned();
        // PS-1 8 [D1, D2].
        assert_eq!(
            read("actor", "immunities_sect"),
            "actor_immunities_gd_master"
        );
        assert_eq!(read("actor", "damage"), "actor_damage");
        assert_eq!(read("actor", "condition_sect"), "actor_condition");
        for (d, most, explosion) in [
            ("gd_novice", 0.5, 0.1),
            ("gd_stalker", 0.65, 0.3),
            ("gd_veteran", 0.8, 0.6),
            ("gd_master", 1.0, 1.0),
        ] {
            let s = format!("actor_immunities_{d}");
            for t in NAMES.iter().filter(|t| !t.ends_with("wound_2")) {
                let want = if *t == "explosion" { explosion } else { most };
                if *t == "physic_strike" {
                    assert!(system.get(&s, "physic_strike_wound_immunity").is_none());
                    continue;
                }
                near(float(data, &s, &format!("{t}_immunity")), want, 0.0);
            }
        }
        for (k, v) in [
            ("health_hit_part", 1.0),
            ("power_hit_part", 0.1),
            ("min_wound_size", 0.0256),
            ("bleeding_v", 0.0005),
            ("wound_incarnation_v", 0.003),
        ] {
            near(float(data, "actor_condition", k), v, 0.0);
        }
        // [D4]: [actor_damage].
        let damage = system.section("actor_damage").unwrap();
        assert_eq!(damage.len(), 45);
        let entry = |k: &str| damage.get(k).flatten().unwrap().replace(' ', "");
        assert_eq!(entry("default"), "1.0,-1,1.0");
        assert_eq!(entry("bip01_head"), "0.2,0,1.5");
        assert_eq!(entry("bip01_l_hand"), "1.0,4,0.5");
        assert_eq!(entry("bip01_r_hand"), "1.0,2,0.0");
        let zero = (damage.entries())
            .filter(|e| {
                let v = e.value().unwrap().replace(' ', "");
                v.starts_with("0.0,") && v.ends_with(",0.0")
            })
            .count();
        assert_eq!(zero, 22, "18 finger bones and 4 of the face");
        // [D5, D6]: the outfits.
        let outfits: Vec<_> = (system.sections())
            .filter(|s| s.get("slot").flatten().map(str::trim) == Some("6"))
            .collect();
        assert_eq!(outfits.len(), 30);
        for s in &outfits {
            assert!(s.contains("immunities_sect") && s.contains("bones_koeff_protection"));
            assert!(!s.contains("physic_strike_protection"), "{}", s.name());
            let c = ItemConfig::read(system, s.name()).unwrap();
            assert_eq!(c.class_id.kind(), Kind::Outfit);
            let p = c.protection.unwrap();
            let mp = s.name().starts_with("mp_");
            if mp {
                continue;
            }
            assert_eq!(p.immunities[10], 1.0, "{}", s.name());
            let table = read(s.name(), "bones_koeff_protection");
            let b = &p.bones;
            assert_eq!(b.bones.len(), 24, "{table}");
            assert!(system.get(&table, "default").is_none());
            assert!(b.bones.iter().all(|(_, a)| a.factor == 1.0));
            let f = if table == "armor_helmet_damage" || table == "exo_helmet_damage" {
                0.05
            } else {
                0.1
            };
            assert_eq!(b.hit_fraction, f, "{table}");
            // Invariant: K × p_t ≤ 1.
            for (_, a) in &b.bones {
                assert!(p.protections.iter().all(|&pt| a.factor * pt <= 1.0));
            }
        }
        let mp_immunities = (outfits.iter())
            .filter(|s| s.name().starts_with("mp_"))
            .filter(|s| {
                let i = read(s.name(), "immunities_sect");
                system.get(&i, "physic_strike_wound_immunity").is_some()
            })
            .count();
        assert_eq!(mp_immunities, 3);
        // The novice outfit.
        let novice = ItemConfig::read(system, "novice_outfit").unwrap();
        let p = novice.protection.unwrap();
        assert_eq!(
            p.protections,
            [0.1, 0.1, 0.1, 0.1, 0.0, 0.0, 0.1, 0.1, 0.1, 1.0, 0.0]
        );
        assert_eq!(
            p.immunities,
            [
                0.04, 0.04, 0.01, 0.025, 0.0, 0.0, 0.04, 0.04, 0.015, 1.0, 1.0
            ]
        );
        let b: &BoneProtection = &p.bones;
        assert_eq!(
            read("novice_outfit", "bones_koeff_protection"),
            "kurtka_mask_damage"
        );
        assert_eq!((b.hit_fraction, b.bones.len()), (0.1, 24));
        let armour: Vec<f32> = b.bones.iter().map(|(_, a)| a.armour).collect();
        assert_eq!(armour.iter().filter(|&&a| a == 0.15).count(), 18);
        assert_eq!(armour.iter().filter(|&&a| a == 0.05).count(), 6);
    });
}

const STAND: Activity = Activity {
    moving: false,
    runs: false,
    sprints: false,
};

/// Frames without input until `now` + `ms`.
fn run(level: &mut LevelScreen, now: &mut u64, ms: u64) {
    let end = *now + ms;
    while *now < end {
        *now += 16;
        level.update(*now, &Input::new());
    }
}

/// PS-1 8's table on the New Game state, by difficulty: (type, power,
/// bone, ap, difficulty, health, power change, wound size, outfit).
#[allow(clippy::type_complexity)]
const TABLE: &[(
    HitType,
    f32,
    Option<&str>,
    f32,
    &str,
    f32,
    f32,
    Option<f32>,
    f32,
)] = &[
    (
        HitType::FIRE_WOUND,
        0.5,
        Some("bip01_spine"),
        0.0,
        "gd_master",
        -0.35,
        -0.035,
        Some(0.525),
        0.9925,
    ),
    (
        HitType::FIRE_WOUND,
        0.5,
        Some("bip01_spine"),
        0.0,
        "gd_novice",
        -0.175,
        -0.0175,
        Some(0.2625),
        0.9925,
    ),
    (
        HitType::FIRE_WOUND,
        0.5,
        Some("bip01_head"),
        0.0,
        "gd_master",
        -0.09,
        -0.045,
        Some(0.675),
        0.9925,
    ),
    (
        HitType::FIRE_WOUND,
        0.1,
        Some("bip01_spine"),
        0.0,
        "gd_master",
        -0.01,
        -0.001,
        Some(0.015),
        0.9985,
    ),
    (
        HitType::FIRE_WOUND,
        0.5,
        Some("bip01_spine"),
        1.0,
        "gd_master",
        -0.5,
        -0.05,
        Some(0.75),
        0.9925,
    ),
    (
        HitType::STRIKE,
        0.2,
        None,
        0.0,
        "gd_master",
        -0.18,
        -0.018,
        Some(0.18),
        0.998,
    ),
    (
        HitType::BURN,
        0.2,
        None,
        0.0,
        "gd_master",
        -0.18,
        -0.018,
        None,
        0.992,
    ),
    (
        HitType::CHEMICAL_BURN,
        0.2,
        None,
        0.0,
        "gd_stalker",
        0.0,
        0.0,
        Some(0.117),
        0.992,
    ),
    (
        HitType::RADIATION,
        0.1,
        None,
        0.0,
        "gd_novice",
        0.0,
        0.0,
        None,
        1.0,
    ),
    (
        HitType::TELEPATIC,
        0.1,
        None,
        0.0,
        "gd_novice",
        0.0,
        0.0,
        None,
        1.0,
    ),
    (
        HitType::SHOCK,
        0.2,
        None,
        0.0,
        "gd_master",
        -0.18,
        -0.018,
        None,
        0.992,
    ),
    (
        HitType::EXPLOSION,
        0.2,
        None,
        0.0,
        "gd_master",
        -0.18,
        -0.018,
        Some(0.18),
        0.992,
    ),
    (
        HitType::WOUND,
        0.2,
        Some("bip01_spine"),
        0.0,
        "gd_master",
        -0.18,
        -0.018,
        Some(0.27),
        0.995,
    ),
    (
        HitType::WOUND_2,
        0.2,
        Some("bip01_spine"),
        0.0,
        "gd_master",
        -0.18,
        -0.018,
        Some(0.27),
        0.995,
    ),
    (
        HitType::PHYSIC_STRIKE,
        0.2,
        None,
        0.0,
        "gd_master",
        -0.2,
        -0.02,
        Some(0.2),
        // No `physic_strike_wound_immunity`: 1.0 (2).
        0.8,
    ),
];

#[test]
fn hits_on_a_new_game_from_game_data() {
    with_game(|tree, data| {
        let mut level = LevelScreen::load(tree, data, 0).unwrap();
        let mut now = 0;
        run(&mut level, &mut now, 160);
        let online = level.character().unwrap().clone();
        let at = level.clock().time();
        // The model's bones resolve the damage table (PS-1 4.3).
        assert!(online.bone("bip01_spine").is_some() && online.bone("jaw_1").is_some());
        assert_eq!(
            online.inventory.worn_outfit().unwrap().config.section,
            "novice_outfit"
        );
        // PS-1 8, by type, from the New Game state (an update of dt 0 after
        // each hit applies it).
        let mut types = BTreeSet::new();
        for &(kind, p, bone, ap, d, health, power, z, outfit) in TABLE {
            let mut s = online.clone();
            s.set_difficulty(d).unwrap();
            let hit = Hit {
                kind,
                power: p,
                bone: bone.map(|b| s.bone(b).unwrap()),
                armour_piercing: ap,
            };
            s.hit(&hit).unwrap();
            let what = format!("{} {p} {bone:?} {d}", kind.name());
            near(s.inventory.worn_outfit().unwrap().condition, outfit, 1e-6);
            let size = s.conditions.wounds.0.first().map(|w| w.size());
            match (z, size) {
                (Some(z), Some(size)) => {
                    assert!(
                        (0.5 * z - 1e-6..=1.5 * z + 1e-6).contains(&size),
                        "{what}: {size}"
                    );
                }
                (None, None) => {}
                _ => panic!("{what}: wound {size:?}, want {z:?}"),
            }
            s.update(at, STAND);
            let c = &s.conditions;
            near(c.health - 1.0, health, 2e-6);
            near(c.power - 1.0, power, 2e-6);
            if kind == HitType::RADIATION {
                near(c.radiation, 0.1, 1e-7);
            }
            if kind == HitType::TELEPATIC {
                near(c.psy_health, 1.0 - 0.025, 1e-7);
            }
            types.insert(kind.index());
        }
        assert_eq!(types.len(), TYPE_COUNT);

        // The level's own hit path: the difficulty as the console sets it,
        // the slowdown, the bleeding and its healing by a bandage.
        level.set_difficulty("gd_novice");
        near(level.character().unwrap().hits.immunities[7], 0.1, 0.0);
        level.set_difficulty("gd_master");
        let spine = level.character().unwrap().bone("bip01_spine");
        let hit = |level: &mut LevelScreen, kind, power| {
            let bone = spine;
            level
                .hit(Hit {
                    kind,
                    power,
                    bone,
                    armour_piercing: 0.0,
                })
                .unwrap();
        };
        hit(&mut level, HitType::STRIKE, 0.5);
        near(level.player().slowdown(), 0.5, 0.0);
        run(&mut level, &mut now, 16);
        assert!(level.player().slowdown() < 0.5);
        hit(&mut level, HitType::CHEMICAL_BURN, 0.4);
        run(&mut level, &mut now, 16);
        let c = &level.character().unwrap().conditions;
        assert_eq!(c.wounds.0.len(), 1);
        // P2 0.36, z 1.5 × 0.36: amount 0.27 … 0.81.
        assert!(c.total_bleeding() > 0.25, "{}", c.total_bleeding());
        let health = c.health;
        run(&mut level, &mut now, 1000);
        let c = &level.character().unwrap().conditions;
        assert!(c.health < health && c.bleeding(), "it bleeds");
        let bandage = data.system.section("bandage").unwrap();
        let e = volya_engine::core::character::condition::Eatable::read(bandage, "II_BANDG")
            .unwrap()
            .unwrap();
        let character = level.character_mut().unwrap();
        let config = character.condition_config;
        character.conditions.eat(&config, &e);
        run(&mut level, &mut now, 16);
        assert!(level.character().unwrap().conditions.wounds.0.is_empty());
    });
}
