// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Hits on the player character against the worked examples of
//! docs/behavior/player-hits.md section 8, on a configuration with the
//! shipped values it gives.

use volya_engine_formats::ltx;
use volya_engine_formats::packet::Writer;

use super::*;
use crate::character::condition::Activity;
use crate::character::tests::{new_game, record, system};
use crate::character::{CharacterState, HitMovement};

const STAND: Activity = Activity {
    moving: false,
    runs: false,
    sprints: false,
};

/// The test model's bones.
const BONES: [&str; 5] = [
    "bip01",
    "bip01_spine",
    "bip01_head",
    "bip01_r_hand",
    "jaw_1",
];
const SPINE: Option<u16> = Some(1);
const HEAD: Option<u16> = Some(2);

#[track_caller]
fn near(got: f32, want: f32) {
    assert!((got - want).abs() <= 2e-6, "{got} != {want}");
}

/// The New Game state online (its first update run) at `difficulty`:
/// `novice_outfit` worn at condition 1, health and power 1, no wounds.
fn online(difficulty: &str) -> CharacterState {
    let system = system();
    let mut s = new_game(&record(), &system);
    s.set_model(BONES.iter().map(|b| (*b).to_owned()).collect())
        .unwrap();
    s.set_difficulty(difficulty).unwrap();
    s.update(0, STAND);
    s
}

fn hit(kind: HitType, power: f32, bone: Option<u16>, ap: f32) -> Hit {
    Hit {
        kind,
        power,
        bone,
        armour_piercing: ap,
    }
}

fn outfit_condition(s: &CharacterState) -> f32 {
    s.inventory.worn_outfit().unwrap().condition
}

/// What a hit does to the New Game state: health, power, radiation and psy
/// health changes after the next update (`dt` 0), the wound amount of type
/// `t` with the draw it took (as the hit leaves it: the update's healing
/// clears an amount below `min_wound_size`), the outfit's condition.
struct After {
    health: f32,
    power: f32,
    radiation: f32,
    psy_health: f32,
    wound: Option<(f32, f32)>,
    outfit: f32,
}

fn apply(difficulty: &str, h: Hit) -> After {
    let mut s = online(difficulty);
    let mut random = s.random;
    s.hit(&h).unwrap();
    let outfit = outfit_condition(&s);
    let wounds = &s.conditions.wounds.0;
    let wound = wounds.first().map(|w| {
        assert_eq!(wounds.len(), 1);
        assert_eq!(w.bone, h.bone);
        let t = h.kind.as_applied().index();
        let factor = 0.5 + f32::from(random.rand15()) / 32767.0;
        (w.amounts[t], factor)
    });
    s.update(0, STAND);
    let c = &s.conditions;
    After {
        health: c.health - 1.0,
        power: c.power - 1.0,
        radiation: c.radiation,
        psy_health: c.psy_health - 1.0,
        wound,
        outfit,
    }
}

/// The wound of size `z` (factor 0.5 … 1.5) grew by its draw.
#[track_caller]
fn wound(a: &After, z: f32) {
    let (amount, factor) = a.wound.expect("a wound");
    near(amount, factor * z);
    assert!((0.5 * z..=1.5 * z + 1e-6).contains(&amount), "{amount}");
}

#[test]
fn section_8_hits_on_the_new_game_state() {
    use HitType as T;
    // fire_wound 0.5, ap 0, bip01_spine at gd_master: P2 0.35.
    let a = apply("gd_master", hit(T::FIRE_WOUND, 0.5, SPINE, 0.0));
    near(a.health, -0.35);
    near(a.power, -0.035);
    wound(&a, 0.525);
    near(a.outfit, 0.9925);
    // The same at gd_novice.
    let a = apply("gd_novice", hit(T::FIRE_WOUND, 0.5, SPINE, 0.0));
    near(a.health, -0.175);
    near(a.power, -0.0175);
    wound(&a, 0.2625);
    near(a.outfit, 0.9925);
    // On the head: P2 0.45, its hit scale 0.2.
    let a = apply("gd_master", hit(T::FIRE_WOUND, 0.5, HEAD, 0.0));
    near(a.health, -0.09);
    near(a.power, -0.045);
    wound(&a, 0.675);
    near(a.outfit, 0.9925);
    // Power 0.1: the hit_fraction floor, P2 0.01.
    let a = apply("gd_master", hit(T::FIRE_WOUND, 0.1, SPINE, 0.0));
    near(a.health, -0.01);
    near(a.power, -0.001);
    wound(&a, 0.015);
    near(a.outfit, 0.9985);
    // Below min_wound_size, the next update clears it (5.3).
    let mut s = online("gd_master");
    s.hit(&hit(T::FIRE_WOUND, 0.1, SPINE, 0.0)).unwrap();
    s.update(0, STAND);
    assert!(s.conditions.wounds.0.is_empty());
    // Armour piercing 1: the armour does nothing.
    let a = apply("gd_master", hit(T::FIRE_WOUND, 0.5, SPINE, 1.0));
    near(a.health, -0.5);
    near(a.power, -0.05);
    wound(&a, 0.75);
    near(a.outfit, 0.9925);
    // strike 0.2, no bone.
    let a = apply("gd_master", hit(T::STRIKE, 0.2, None, 0.0));
    near(a.health, -0.18);
    near(a.power, -0.018);
    wound(&a, 0.18);
    near(a.outfit, 0.998);
    // burn 0.2, no bone: no wound.
    let a = apply("gd_master", hit(T::BURN, 0.2, None, 0.0));
    near(a.health, -0.18);
    near(a.power, -0.018);
    assert!(a.wound.is_none());
    near(a.outfit, 0.992);
    // chemical_burn 0.2 at gd_stalker: only a wound.
    let a = apply("gd_stalker", hit(T::CHEMICAL_BURN, 0.2, None, 0.0));
    assert_eq!((a.health, a.power), (0.0, 0.0));
    wound(&a, 0.117);
    near(a.outfit, 0.992);
    // radiation 0.1 at gd_novice: radiation +0.1, no immunity.
    let a = apply("gd_novice", hit(T::RADIATION, 0.1, None, 0.0));
    assert_eq!((a.health, a.power, a.outfit), (0.0, 0.0, 1.0));
    near(a.radiation, 0.1);
    assert!(a.wound.is_none());
    // telepatic 0.1 at gd_novice: psy health −0.5 × 0.5 × 0.1.
    let a = apply("gd_novice", hit(T::TELEPATIC, 0.1, None, 0.0));
    assert_eq!((a.health, a.power, a.outfit), (0.0, 0.0, 1.0));
    near(a.psy_health, -0.025);
}

#[test]
fn an_explosion_without_an_outfit_by_difficulty() {
    // M11: explosion 0.5, no bone, no outfit.
    for (difficulty, want) in [("gd_novice", -0.05), ("gd_master", -0.5)] {
        let mut s = online(difficulty);
        let id = s.inventory.worn_outfit().unwrap().id;
        s.inventory.remove(id);
        s.hit(&hit(HitType::EXPLOSION, 0.5, None, 0.0)).unwrap();
        s.update(0, STAND);
        near(s.conditions.health - 1.0, want);
    }
}

#[test]
fn wound_2_is_a_wound_from_the_power_on() {
    // 4.2 item 3: wound_2 takes the wound's keys and amount, but not its
    // slowdown (4.1 item 2).
    let a = apply("gd_master", hit(HitType::WOUND_2, 0.2, None, 0.0));
    near(a.health, -0.18);
    wound(&a, 0.18);
    near(a.outfit, 1.0 - 0.025 * 0.2);
    assert_eq!(HitMovement::of(HitType::WOUND_2, 0.5).slowdown, 0.0);
    assert_eq!(HitMovement::of(HitType::WOUND, 0.5).slowdown, 0.5);
    assert_eq!(HitMovement::of(HitType::STRIKE, 2.0).slowdown, 1.0);
    assert!(!HitMovement::of(HitType::RADIATION, 1.0).cancels_sprint);
    assert!(!HitMovement::of(HitType::TELEPATIC, 1.0).cancels_sprint);
    assert!(HitMovement::of(HitType::BURN, 1.0).cancels_sprint);
}

#[test]
fn bone_factors_scale_health_and_wounds() {
    // gd_master, no outfit: 4.5's table on bones of the damage table.
    let mut s = online("gd_master");
    let id = s.inventory.worn_outfit().unwrap().id;
    s.inventory.remove(id);
    // The right hand: wound scale 0, so no wound; health by the power.
    s.hit(&hit(HitType::WOUND, 0.1, Some(3), 0.0)).unwrap();
    assert_eq!(s.conditions.wounds.0.len(), 1);
    assert_eq!(s.conditions.wounds.0[0].size(), 0.0);
    s.update(0, STAND);
    near(s.conditions.health, 0.9);
    // Of size 0, it is removed after the update (5.3).
    assert!(s.conditions.wounds.0.is_empty());
    // The jaw: hit scale 0, so no health; shock ignores the hit scale.
    s.hit(&hit(HitType::FIRE_WOUND, 0.1, Some(4), 0.0)).unwrap();
    s.hit(&hit(HitType::SHOCK, 0.1, Some(4), 0.0)).unwrap();
    s.update(0, STAND);
    near(s.conditions.health, 0.8);
    // M16: with no outfit at gd_master, types 2, 3, 7, 8 and 10 on a bone
    // with h 1 lower health by exactly their power.
    for t in [2, 3, 7, 8, 10] {
        let mut s = online("gd_master");
        let id = s.inventory.worn_outfit().unwrap().id;
        s.inventory.remove(id);
        s.hit(&hit(HitType::new(t).unwrap(), 0.3, SPINE, 0.0))
            .unwrap();
        s.update(0, STAND);
        near(s.conditions.health, 0.7);
    }
}

#[test]
fn hits_take_effect_at_the_next_update_and_not_before_coming_online() {
    let system = system();
    let mut s = new_game(&record(), &system);
    s.set_model(BONES.iter().map(|b| (*b).to_owned()).collect())
        .unwrap();
    // 4.7, as corrected in review: before the first update after coming
    // online, the health, power and radiation changes are discarded and the
    // psy health's applied.
    s.hit(&hit(HitType::BURN, 0.2, None, 0.0)).unwrap();
    s.hit(&hit(HitType::RADIATION, 0.2, None, 0.0)).unwrap();
    s.hit(&hit(HitType::TELEPATIC, 0.2, None, 0.0)).unwrap();
    s.update(0, STAND);
    let c = &s.conditions;
    assert_eq!((c.health, c.power, c.radiation), (1.0, 1.0, 0.0));
    near(c.psy_health, 0.8);
    // The outfit lost its condition at once (4.7): 1 − 0.04 × 0.2.
    near(outfit_condition(&s), 0.992);
    // Online, a hit waits for the next update.
    s.hit(&hit(HitType::BURN, 0.2, None, 0.0)).unwrap();
    assert_eq!(s.conditions.health, 1.0);
    s.update(0, STAND);
    near(s.conditions.health, 1.0 - 0.2 * (1.0 - 0.1 * 0.992));
    // A hit taking health to 0 kills at that update; on the dead nothing
    // later applies.
    s.hit(&hit(HitType::SHOCK, 5.0, None, 0.0)).unwrap();
    s.update(0, STAND);
    assert!(!s.conditions.alive());
    assert_eq!(s.conditions.health, crate::character::condition::MIN_HEALTH);
    let wounds = s.conditions.wounds.clone();
    s.hit(&hit(HitType::STRIKE, 0.1, None, 0.0)).unwrap();
    assert_eq!(s.conditions.wounds, wounds, "no wound opens on the dead");
}

#[test]
fn reads_immunities_and_the_difficulty() {
    let system = system();
    let mut s = new_game(&record(), &system);
    // immunities_sect first (gd_master's).
    assert_eq!(s.hits.immunities, [1.0; TYPE_COUNT]);
    s.set_difficulty("gd_novice").unwrap();
    let i = s.hits.immunities;
    assert_eq!((i[0], i[7], i[9], i[10]), (0.5, 0.1, 1.0, 1.0));
    // A difficulty whose section is missing is an error.
    assert!(s.set_difficulty("gd_veteran").is_err());
    assert_eq!((s.hits.health_hit_part, s.hits.power_hit_part), (1.0, 0.1));
    // Hit types by number and name (2).
    assert_eq!(HitType::named("fire_wound").unwrap(), HitType::FIRE_WOUND);
    assert_eq!(HitType::new(10).unwrap().name(), "physic_strike");
    assert!(HitType::new(11).is_err());
    assert_eq!(HitType::WOUND_2.as_applied(), HitType::WOUND);
    // A protection or immunity key missing is an error; physic_strike's are
    // optional.
    let config = ltx::parse(b"[i]\nburn_immunity = 0.5\n").unwrap();
    assert!(read_immunities(config.section("i").unwrap()).is_err());
}

#[test]
fn the_damage_table_against_the_model() {
    let config = ltx::parse(
        b"[d]\ndefault = 0.5, -1, 0.7\nbip01_head = 0.2, 0, 1.5\n\
          [no_default]\nbip01_head = 0.2, 0, 1.5\n\
          [zero_root]\nbip01 = 0.0, 0, 1.0\n",
    )
    .unwrap();
    let bones: Vec<String> = ["bip01", "bip01_head"].map(str::to_owned).to_vec();
    let mut d = DamageTable::read(config.section("d").unwrap());
    d.set_model(&bones).unwrap();
    assert_eq!(
        d.scales(Some(1)),
        BoneScales {
            hit: 0.2,
            wound: 1.5
        }
    );
    assert_eq!(
        d.scales(Some(0)),
        BoneScales {
            hit: 0.5,
            wound: 0.7
        }
    );
    assert_eq!(
        d.scales(None),
        BoneScales {
            hit: 0.5,
            wound: 0.7
        }
    );
    let mut d = DamageTable::read(config.section("no_default").unwrap());
    d.set_model(&bones).unwrap();
    assert_eq!(
        d.scales(None),
        BoneScales {
            hit: 1.0,
            wound: 1.0
        }
    );
    // A bone the model lacks, a first bone with a factor 0: errors.
    assert!(d.set_model(&["bip01".to_owned()]).is_err());
    let mut d = DamageTable::read(config.section("zero_root").unwrap());
    assert!(d.set_model(&bones).is_err());
}

#[test]
fn the_outfit_bone_table_checks_the_model() {
    let mut s = online("gd_master");
    s.bones = vec!["bip01".to_owned(), "bip01_spine".to_owned()];
    assert!(s.hit(&hit(HitType::BURN, 0.1, None, 0.0)).is_err());
}

#[test]
fn wounds_bleed_and_heal_with_time() {
    // M12: one wound of size 0.525, updates of 1 game second.
    let mut w = Wounds::default();
    w.grow(None, HitType::FIRE_WOUND, 0.525, 16383);
    near(w.0[0].size(), 0.525 * (0.5 + 16383.0 / 32767.0));
    w.0[0].amounts[8] = 0.525;
    let mut bled = 0.0f32;
    let mut updates = 0;
    while w.0[0].size() > 0.0 {
        bled += w.total_bleeding() * 0.0005;
        w.heal(0.003, 0.0256);
        updates += 1;
        if updates == 166 {
            assert!(w.0[0].amounts[8] > 0.0256);
        }
    }
    assert_eq!(updates, 167);
    assert!((bled - 0.023046).abs() < 1e-5, "{bled}");
    // The same through the condition update: removed after the 167th.
    let mut s = online("gd_master");
    let c = s.condition_config;
    s.conditions.wounds.0.push(Wound {
        bone: None,
        amounts: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.525, 0.0, 0.0],
    });
    let load = s.load();
    for k in 1..=167 {
        s.conditions.update(&c, 1.0, STAND, &load);
        assert_eq!(s.conditions.wounds.0.len(), usize::from(k < 167), "{k}");
    }
    // M13: sizes 0.4 and 0.2: total bleeding 0.3, −0.00015 per game second.
    let mut s = online("gd_master");
    for a in [0.4, 0.2] {
        let mut amounts = [0.0; TYPE_COUNT];
        amounts[2] = a;
        s.conditions.wounds.0.push(Wound {
            bone: None,
            amounts,
        });
    }
    near(s.conditions.total_bleeding(), 0.3);
    s.conditions.satiety = 0.0;
    s.conditions.update(&c, 1.0, STAND, &load);
    // Hunger's −0.0001 too (not yet bleeding at the previous update).
    near(s.conditions.health, 1.0 - 0.00015 - 0.0001);
    assert!(s.conditions.bleeding());
}

#[test]
fn eatables_heal_wounds() {
    // M14: amounts 0.525 (type 8) and 0.2 (type 2).
    let wound = || {
        let mut amounts = [0.0; TYPE_COUNT];
        amounts[8] = 0.525;
        amounts[2] = 0.2;
        Wound {
            bone: SPINE,
            amounts,
        }
    };
    let mut w = Wounds(vec![wound()]);
    w.heal(0.3, 0.0256);
    near(w.0[0].amounts[8], 0.225);
    assert_eq!(w.0[0].amounts[2], 0.0);
    // A bandage through eating: both 0, removed at the next update.
    let mut s = online("gd_master");
    s.conditions.wounds = Wounds(vec![wound()]);
    let bandage = crate::character::condition::Eatable {
        wounds_heal_perc: 1.0,
        eat_portions_num: 1,
        ..Default::default()
    };
    let c = s.condition_config;
    s.conditions.eat(&c, &bandage);
    assert_eq!(s.conditions.wounds.0[0].size(), 0.0);
    s.update(0, STAND);
    assert!(s.conditions.wounds.0.is_empty());
}

#[test]
fn wound_amounts_stay_within_range() {
    let mut w = Wounds::default();
    for r in [0, 32767, 100, 30000] {
        w.grow(HEAD, HitType::EXPLOSION, 4.0, r);
        w.grow(HEAD, HitType::STRIKE, -4.0, r);
    }
    assert_eq!(w.0.len(), 1);
    assert_eq!(w.0[0].amounts[7], MAX_AMOUNT);
    assert_eq!(w.0[0].amounts[2], 0.0);
    // A new bone's wound goes at the end.
    w.grow(None, HitType::WOUND, 0.1, 0);
    assert_eq!(w.0[1].bone, None);
    // The outfit's condition stays within 0 … 1.
    let mut s = online("gd_master");
    for _ in 0..200 {
        s.hit(&hit(HitType::BURN, 1.0, None, 0.0)).unwrap();
    }
    assert_eq!(outfit_condition(&s), 0.0);
}

#[test]
fn a_save_holds_the_wounds() {
    // 5.5: count, then bone (255: no bone) and 11 × q8(amount, 0, 10).
    let mut s = online("gd_master");
    let mut amounts = [0.0; TYPE_COUNT];
    amounts[8] = 10.0;
    amounts[2] = 5.0;
    s.conditions.wounds = Wounds(vec![
        Wound {
            bone: Some(258),
            amounts,
        },
        Wound {
            bone: None,
            amounts: [0.0; TYPE_COUNT],
        },
    ]);
    let mut w = Writer::new();
    s.condition_block(&mut w);
    let bytes = w.into_inner();
    let tail = &bytes[1 + 16..];
    let mut want = vec![2u8, 2];
    want.extend([0, 0, 128, 0, 0, 0, 0, 0, 255, 0, 0]);
    want.push(255);
    want.extend([0; 11]);
    assert_eq!(tail, want);
}
