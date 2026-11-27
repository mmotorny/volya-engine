// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player character's New Game and its time rates on a configuration
//! with the shipped values (docs/behavior/player-state.md sections 1–7).

use volya_engine_formats::ltx;
use volya_engine_formats::object::{
    ClassId, CompressedVector, Creature, CreatureUpdate, ObjectBlock, Player, PlayerPhysics,
    PlayerUpdate, Skeleton, Visual,
};

use super::inventory::Place;
use super::*;

const SYSTEM: &str = concat!(
    "\
    [actor]\nclass = O_ACTOR\ncondition_sect = actor_condition\nmoney = 40\n\
    immunities_sect = actor_immunities_gd_master\ndamage = actor_damage\n\
    [actor_immunities_gd_master]\nburn_immunity = 1.0\nstrike_immunity = 1.0\n\
    shock_immunity = 1.0\nwound_immunity = 1.0\nradiation_immunity = 1.0\n\
    telepatic_immunity = 1.0\nchemical_burn_immunity = 1.0\nexplosion_immunity = 1.0\n\
    fire_wound_immunity = 1.0\n\
    [actor_immunities_gd_novice]\nburn_immunity = 0.5\nstrike_immunity = 0.5\n\
    shock_immunity = 0.5\nwound_immunity = 0.5\nradiation_immunity = 0.5\n\
    telepatic_immunity = 0.5\nchemical_burn_immunity = 0.5\nexplosion_immunity = 0.1\n\
    fire_wound_immunity = 0.5\n\
    [actor_immunities_gd_stalker]\nburn_immunity = 0.65\nstrike_immunity = 0.65\n\
    shock_immunity = 0.65\nwound_immunity = 0.65\nradiation_immunity = 0.65\n\
    telepatic_immunity = 0.65\nchemical_burn_immunity = 0.65\nexplosion_immunity = 0.3\n\
    fire_wound_immunity = 0.65\n\
    [actor_damage]\ndefault = 1.0, -1, 1.0\nbip01_spine = 1.0, 0, 1.5\n\
    bip01_head = 0.2, 0, 1.5\nbip01_r_hand = 1.0, 2, 0.0\njaw_1 = 0.0, 0, 0.0\n\
    [inventory]\nmax_weight = 50\nmax_belt = 5\n\
    [detector_simple]\nclass = D_SIMDET\nslot = 8\ndefault_to_ruck = false\ninv_weight = 0\n\
    visual = equipments\\item_datchik1.ogf\n\
    [device_torch]\nclass = TORCH_S\nslot = 9\ninv_weight = 0\nvisual = equipments\\light_night\n\
    [novice_outfit]\nclass = E_STLK\nslot = 6\ninv_weight = 3.0\n\
    visual = equipments\\novice_suit.ogf\nadditional_inventory_weight = 0\n\
    additional_inventory_weight2 = 0\n",
    crate::character::hits::test_outfit_keys!(),
    crate::character::hits::test_outfit_sections!(),
    "[wpn_binoc]\nclass = WP_BINOC\nslot = 4\ninv_weight = 1.0\nammo_mag_size = 0\n\
    scope_status = 1\nsilencer_status = 0\ngrenade_launcher_status = 0\n\
    visual = weapons\\binoculars\\wpn_binoculars.ogf\n\
    [device_pda]\nclass = D_PDA\nslot = 7\ninv_weight = 0.3\nvisual = equipments\\item_pda\n\
    [bolt]\nclass = II_BOLT\nvisual = v\nslot = 5\ninv_weight = 0\n\
    [medkit]\nclass = II_MEDKI\nvisual = v\ninv_weight = 0.1\neat_health = 0.6\neat_power = 0.0\n\
    eat_satiety = 0\neat_radiation = -0.1\nwounds_heal_perc = 0.3\neat_portions_num = 1\n\
    eat_max_power = 0.1\n\
    [heavy]\nclass = II_MEDKI\nvisual = v\ninv_weight = 60\n"
);

/// The player's custom data (2.1), CR LF line ends.
const CUSTOM_DATA: &str = "[dont_spawn_character_supplies]\r\n\r\n[spawn]\r\nwpn_binoc\r\n\
    detector_simple\r\nnovice_outfit\r\ndevice_torch\r\n";

pub(crate) fn system() -> Config {
    let text = format!("{SYSTEM}{}", super::condition::tests::ACTOR_CONDITION);
    ltx::parse(text.as_bytes()).unwrap()
}

/// A player's record as `all.spawn` holds it (section 7).
pub(crate) fn record() -> ObjectRecord {
    let creature = Creature {
        object: ObjectBlock {
            game_graph_vertex: 12,
            distance: 0.0,
            direct_control: 1,
            level_graph_node: 3456,
            state_flags: 0,
            custom_data: CUSTOM_DATA.into(),
            story_id: -1,
            spawn_story_id: -1,
        },
        visual: Visual {
            model: "actors\\hero\\stalker_novice".into(),
            flags: 0,
        },
        team: 0,
        squad: 0,
        group: 0,
        health: 1.0,
        dynamic_out_restrictions: Vec::new(),
        dynamic_in_restrictions: Vec::new(),
        killer_id: 0xFFFF,
        death_time: 0,
    };
    ObjectRecord {
        header: volya_engine_formats::object::SpawnHeader {
            section: "actor".into(),
            name: "level_prefix_actor_0001".into(),
            game_type: 0,
            after_game_type: 254,
            position: [1.0, 2.0, 3.0],
            angles: [0.0; 3],
            respawn_time: 0,
            id: 0xFFFF,
            parent_id: 0xFFFF,
            phantom_id: 0xFFFF,
            flags: 0x29,
            version: 118,
            script_version: 6,
            client_data: Vec::new(),
            spawn_id: 63,
        },
        class_id: ClassId::new("O_ACTOR").unwrap(),
        single_player: true,
        object: Object::Player(Box::new(Player {
            creature,
            trader: TraderLike {
                money: 40,
                specific_character: String::new(),
                trader_flags: 0,
                character_profile: "actor".into(),
                community: -1,
                rank: NOT_SET,
                reputation: NOT_SET,
                character_name: String::new(),
            },
            skeleton: Skeleton {
                startup_animation: String::new(),
                flags: 0,
                source_id: 0xFFFF,
                bone_state: None,
            },
            holder_id: 0xFFFF,
            update: PlayerUpdate {
                creature: CreatureUpdate {
                    health: 1.0,
                    timestamp: 0,
                    flags: 0,
                    position: [1.0, 2.0, 3.0],
                    model_yaw: 0.0,
                    yaw: 0.0,
                    pitch: 0.0,
                    roll: 0.0,
                    team: 0,
                    squad: 0,
                    group: 0,
                },
                movement_state: 0,
                acceleration: CompressedVector {
                    direction: 0,
                    magnitude: 0.0,
                },
                velocity: CompressedVector {
                    direction: 0,
                    magnitude: 0.0,
                },
                radiation: 0.0,
                active_weapon: 0,
                physics: PlayerPhysics::None,
            },
        })),
    }
}

fn actor() -> SpecificCharacter {
    SpecificCharacter {
        id: "actor".into(),
        community: "actor".into(),
        rank: 0,
        reputation: 0,
        name: "actor_name".into(),
        visual: "actors\\hero\\stalker_novice".into(),
        supplies: "[spawn] \n device_torch \n".into(),
    }
}

pub(crate) fn new_game(record: &ObjectRecord, system: &Config) -> CharacterState {
    let communities = communities("actor, 0, actor_dolg, 1, actor_freedom, 2");
    CharacterState::new_game(&NewGame {
        record,
        system,
        character: &actor(),
        communities: &communities,
        character_name: "Меченый",
        seed: 1,
        first_id: 1,
    })
    .and_then(|mut s| {
        // The next id after the five supplies.
        s.receive_bolt(system, 6)?;
        Ok(s)
    })
    .unwrap()
}

#[test]
fn reads_communities() {
    assert_eq!(
        communities("actor, 0, actor_dolg, 1,stalker,4"),
        [
            ("actor".to_owned(), 0),
            ("actor_dolg".to_owned(), 1),
            ("stalker".to_owned(), 4)
        ]
    );
}

#[test]
fn a_new_game_resolves_the_profile() {
    let system = system();
    let s = new_game(&record(), &system);
    let p = &s.profile;
    assert_eq!(
        (p.money, p.specific_character.as_str(), p.trader_flags),
        (40, "actor", 0)
    );
    assert_eq!(
        (
            p.character_profile.as_str(),
            p.community,
            p.rank,
            p.reputation
        ),
        ("actor", 0, 0, 0)
    );
    assert_eq!(p.character_name, "Меченый");
    // Values already set are kept.
    let mut set = Profile {
        community: 3,
        rank: 7,
        character_name: "x".into(),
        ..p.clone()
    };
    set.resolve(&actor(), &communities("actor, 0"), "y");
    assert_eq!(
        (set.community, set.rank, set.character_name.as_str()),
        (3, 7, "x")
    );
}

#[test]
fn a_new_game_starts_the_conditions() {
    let system = system();
    let s = new_game(&record(), &system);
    let c = &s.conditions;
    assert_eq!(
        (c.health, c.max_health, c.power, c.max_power, c.satiety),
        (1.0, 1.0, 1.0, 1.0, 1.0)
    );
    assert_eq!(
        (c.radiation, c.psy_health, c.morale, c.alcohol),
        (0.0, 1.0, 1.0, 0.0)
    );
    assert!(!c.limping && !c.cant_walk && !c.cant_sprint);
    // The record's health wins.
    let mut hurt = record();
    if let Object::Player(p) = &mut hurt.object {
        p.creature.health = 0.5;
    }
    assert_eq!(new_game(&hurt, &system).conditions.health, 0.5);
}

#[test]
fn a_new_game_supplies_the_inventory() {
    let system = system();
    let s = new_game(&record(), &system);
    let inv = &s.inventory;
    let items: Vec<_> = inv
        .items
        .iter()
        .map(|i| (i.config.section.as_str(), i.id, i.place, i.condition))
        .collect();
    // Section 3.1's order (the [spawn] keys sorted, then the PDA), the
    // character supplies not created, then the bolt; section 3.3's places.
    assert_eq!(
        items,
        [
            ("detector_simple", 1, Place::Slot(8), 1.0),
            ("device_torch", 2, Place::Slot(9), 1.0),
            ("novice_outfit", 3, Place::Slot(6), 1.0),
            ("wpn_binoc", 4, Place::Slot(4), 1.0),
            ("device_pda", 5, Place::Slot(7), 1.0),
            ("bolt", 6, Place::Slot(5), 1.0),
        ]
    );
    assert_eq!(inv.active_slot, Some(4));
    assert_eq!(s.active_slot_byte(), 4);
    assert!(inv.belt.is_empty());
    let load = s.load();
    assert!((load.weight - 4.3).abs() < 1e-6);
    assert_eq!(
        (load.carry_limit, load.walk_limit, load.power_loss),
        (50.0, 60.0, 1.0)
    );
    // Without [dont_spawn_character_supplies], the character's torch too.
    let mut supplied = record();
    if let Object::Player(p) = &mut supplied.object {
        p.creature.object.custom_data = "[spawn]\nwpn_binoc\n".into();
    }
    let s = new_game(&supplied, &system);
    let sections: Vec<_> = s
        .inventory
        .items
        .iter()
        .map(|i| i.config.section.as_str())
        .collect();
    assert_eq!(
        sections,
        ["wpn_binoc", "device_pda", "device_torch", "bolt"]
    );
}

#[test]
fn records_for_a_save() {
    let system = system();
    let s = new_game(&record(), &system);
    let records = s.item_records().unwrap();
    // The bolt is never saved.
    let names: Vec<_> = records.iter().map(|r| r.header.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "detector_simple0001",
            "device_torch0002",
            "novice_outfit0003",
            "wpn_binoc0004",
            "device_pda0005"
        ]
    );
    assert!(records.iter().all(|r| r.header.parent_id == 0
        && r.header.position == [1.0, 2.0, 3.0]
        && r.object.object_block().game_graph_vertex == 12));
    let Object::Pda(pda) = &records[4].object else {
        panic!()
    };
    assert_eq!(
        (pda.original_owner_id, pda.specific_character.as_str()),
        (0, "actor")
    );
    let mut player = record();
    s.apply_to_record(&mut player).unwrap();
    assert_eq!(player.header.id, 0);
    let Object::Player(p) = &player.object else {
        panic!()
    };
    assert_eq!((p.trader.community, p.trader.rank), (0, 0));
    assert_eq!(p.trader.character_name, "Меченый");
    let (spawn, update) = player.write().unwrap();
    assert_eq!(
        ObjectRecord::parse(&spawn, &update, "O_ACTOR", true).unwrap(),
        player
    );
}

#[test]
fn client_data_blocks() {
    let system = system();
    let mut s = new_game(&record(), &system);
    let mut w = Writer::new();
    s.player_condition_block(&mut w);
    let mut want = vec![1u8];
    for v in [1.0f32, 0.0, 1.0, 1.0] {
        want.extend(v.to_le_bytes());
    }
    want.push(0);
    want.extend(0.0f32.to_le_bytes());
    want.extend(0u16.to_le_bytes());
    want.extend(1.0f32.to_le_bytes());
    assert_eq!(w.into_inner(), want);
    let mut w = Writer::new();
    s.inventory.items[3].item_block(&mut w);
    let mut want = vec![1u8];
    want.extend(1.0f32.to_le_bytes());
    want.push(0);
    assert_eq!(w.into_inner(), want);
    // Dead: only the flag.
    s.conditions.health = 0.0;
    let mut w = Writer::new();
    s.condition_block(&mut w);
    assert_eq!(w.into_inner(), [0]);
}

const STAND: Activity = Activity {
    moving: false,
    runs: false,
    sprints: false,
};
const SPRINT: Activity = Activity {
    moving: true,
    runs: true,
    sprints: true,
};

/// Updates every `step` game ms from `start` for `minutes` game minutes.
fn advance(s: &mut CharacterState, start: u64, minutes: u64, step: u64, activity: Activity) -> u64 {
    let end = start + minutes * 60_000;
    let mut t = start;
    while t < end {
        t = (t + step).min(end);
        s.update(t, activity);
    }
    t
}

#[test]
fn advances_by_game_time_at_the_spec_rates() {
    let system = system();
    let mut s = new_game(&record(), &system);
    s.conditions.power = 0.2;
    s.conditions.health = 0.5;
    // The first update has dt 0.
    s.update(5_000_000, STAND);
    assert_eq!((s.conditions.power, s.conditions.satiety), (0.2, 1.0));
    // 2 game minutes standing, frames of 160 game ms (16 real ms × 10).
    let t = advance(&mut s, 5_000_000, 2, 160, STAND);
    let c = &s.conditions;
    let near =
        |got: f32, want: f32, tol: f32| assert!((got - want).abs() <= tol, "{got} != {want}");
    near(c.power, 0.2 + 0.005055 * 120.0, 1e-4);
    // Single precision over 750 updates.
    near(c.satiety, 1.0 - 0.000015 * 120.0, 5e-5);
    near(c.health, 0.5 + 0.0001 * 120.0, 5e-5);
    // 10 game minutes sprinting: 0.0010175 per game second (from full
    // power, the cap no longer holding it).
    s.conditions.power = 1.0;
    let t = advance(&mut s, t, 10, 160, SPRINT);
    near(s.conditions.power, 1.0 - 0.0010175 * 600.0, 1e-3);
    // The same game time in one update gives the same result.
    let mut once = new_game(&record(), &system);
    once.update(0, STAND);
    once.update(600_000, SPRINT);
    near(once.conditions.power, 1.0 - 0.0010175 * 600.0, 1e-6);
    // A stopped clock changes nothing.
    let before = s.conditions.clone();
    s.update(t, STAND);
    assert_eq!(s.conditions, before);
}

#[test]
fn limits_movement_and_jumps() {
    let system = system();
    let mut s = new_game(&record(), &system);
    assert_eq!(s.movement_limits(), Limits::default());
    s.jump();
    assert!((1.0 - s.conditions.power - 0.0143).abs() < 1e-6);
    s.conditions.power = 0.15;
    assert_eq!(
        s.movement_limits(),
        Limits {
            limping: false,
            cant_sprint: true,
            unable_to_move: false
        }
    );
    s.conditions.power = 0.005;
    assert_eq!(
        s.movement_limits(),
        Limits {
            limping: true,
            cant_sprint: true,
            unable_to_move: true
        }
    );
    // Too heavy to walk: above 60 with the novice outfit.
    let mut s = new_game(&record(), &system);
    s.inventory.take(InventoryItem::create(
        ItemConfig::read(&system, "heavy").unwrap(),
        100,
        None,
        0,
        "actor",
    ));
    assert!(s.movement_limits().unable_to_move);
}

#[test]
fn eats_a_medkit() {
    let system = system();
    let mut s = new_game(&record(), &system);
    s.inventory.take(InventoryItem::create(
        ItemConfig::read(&system, "medkit").unwrap(),
        50,
        None,
        0,
        "actor",
    ));
    assert_eq!(s.inventory.item(50).unwrap().place, Place::Backpack);
    s.eat(50).unwrap();
    s.update(0, STAND);
    let c = &s.conditions;
    assert_eq!(
        (c.health, c.radiation, c.max_power, c.satiety),
        (1.0, 0.0, 1.0, 1.0)
    );
    assert!(s.inventory.item(50).is_none());
    assert!(s.eat(4).is_err());
}
