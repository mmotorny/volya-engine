// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;

/// A spawn packet around `state`, with all.spawn's header values.
fn spawn(section: &str, script_version: u16, state: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u16(SPAWN_MESSAGE);
    w.string(section).unwrap();
    w.string("name").unwrap();
    w.u8(0);
    w.u8(254);
    w.float3([1.0, 2.0, 3.0]);
    w.float3([0.0, -1.5, 0.0]);
    w.u16(0);
    w.u16(0xFFFF);
    w.u16(0xFFFF);
    w.u16(0xFFFF);
    w.u16(0x21);
    w.u16(VERSION);
    w.u16(script_version);
    w.u16(2);
    w.bytes(b"cd");
    w.u16(17);
    w.u16(state.len() as u16 + 2);
    w.bytes(state);
    w.into_inner()
}

fn update(data: &[u8]) -> Vec<u8> {
    [&[0, 0][..], data].concat()
}

fn object_block(w: &mut Writer) {
    w.u16(4);
    w.float(0.7);
    w.u32(1);
    w.u32(11_713);
    w.u32(0x23F);
    w.string("[logic]").unwrap();
    w.s32(-1);
    w.s32(-1);
}

fn visual(w: &mut Writer) {
    w.string("model").unwrap();
    w.u8(1);
}

fn skeleton(w: &mut Writer) {
    w.string("idle").unwrap();
    w.u8(0);
    w.u16(0xFFFF);
}

fn creature(w: &mut Writer) {
    object_block(w);
    visual(w);
    w.bytes(&[1, 2, 3]);
    w.float(1.0);
    w.list(&[5u16], |w, &v| w.u16(v)).unwrap();
    w.u32(0);
    w.u16(0xFFFF);
    w.u64(0);
}

fn monster_like(w: &mut Writer) {
    creature(w);
    w.string("out").unwrap();
    w.string("").unwrap();
    w.u16(0xFFFF);
    w.u8(0);
}

fn trader_like(w: &mut Writer) {
    w.u32(5000);
    w.string("esc_wolf").unwrap();
    w.u32(0);
    w.string("profile").unwrap();
    w.s32(2);
    w.s32(100);
    w.s32(0);
    w.string("Волк").unwrap();
}

fn creature_update(w: &mut Writer) {
    w.float(1.0);
    w.u32(0);
    w.u8(0);
    w.float3([1.0, 2.0, 3.0]);
    (0..4).for_each(|_| w.float(0.25));
    w.bytes(&[1, 2, 3]);
}

fn monster_update(w: &mut Writer) {
    creature_update(w);
    w.u16(1);
    w.u16(2);
    w.float(0.5);
    w.float(1.5);
}

/// Parses, checks the packets are filled exactly, writes back the same bytes.
fn round_trip(class_id: &str, spawn: &[u8], update: &[u8], single_player: bool) -> ObjectRecord {
    let record = ObjectRecord::parse(spawn, update, class_id, single_player)
        .unwrap_or_else(|e| panic!("{class_id}: {e}"));
    let (s, u) = record.write().unwrap();
    assert_eq!(s, spawn, "{class_id}: spawn packet");
    assert_eq!(u, update, "{class_id}: update packet");
    record
}

fn bytes(f: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::new();
    f(&mut w);
    w.into_inner()
}

#[test]
fn player_round_trips_with_each_physics_form() {
    let state = bytes(|w| {
        creature(w);
        trader_like(w);
        skeleton(w);
        w.u16(0xFFFF);
    });
    for (m, physics) in [
        (0u16, vec![]),
        (
            1,
            bytes(|w| {
                w.u8(1);
                (0..19).for_each(|i| w.float(i as f32));
            }),
        ),
        (
            3,
            bytes(|w| {
                w.u8(2);
                w.bytes(&[9; 3 * 2 + 24]);
            }),
        ),
    ] {
        let upd = update(&bytes(|w| {
            creature_update(w);
            w.u16(0);
            w.bytes(&[0; 6]);
            w.bytes(&[0; 6]);
            w.float(0.0);
            w.u8(0xFF);
            w.u16(m);
            w.bytes(&physics);
        }));
        let r = round_trip("O_ACTOR", &spawn("actor", 6, &state), &upd, true);
        assert_eq!(r.kind(), Kind::Player);
        assert_eq!(r.header.client_data, b"cd");
        assert_eq!(r.header.spawn_id, 17);
        let Object::Player(p) = &r.object else {
            panic!()
        };
        assert_eq!(p.creature.object.level_graph_node, 11_713);
        assert_eq!(p.trader.character_name, "Волк");
        assert_eq!(p.creature.dynamic_out_restrictions, [5]);
        assert!(r.can_be_saved());
        let truncated = &upd[..upd.len() - 1];
        assert!(
            ObjectRecord::parse(&spawn("actor", 6, &state), truncated, "O_ACTOR", true).is_err()
        );
        let long = [&upd[..], &[0]].concat();
        assert!(ObjectRecord::parse(&spawn("actor", 6, &state), &long, "O_ACTOR", true).is_err());
    }
}

#[test]
fn scripted_stalker_fields_follow_the_script_version() {
    let engine = bytes(|w| {
        trader_like(w);
        monster_like(w);
        w.list(&[0u8, 1, 2, 0, 1], |w, &v| w.u8(v)).unwrap();
        w.list(&[2u8, 1, 0, 0], |w, &v| w.u8(v)).unwrap();
        skeleton(w);
    });
    let upd = update(&bytes(|w| {
        monster_update(w);
        w.string("dialog").unwrap();
    }));
    // Version 7: job online with a condition list, both bool8s.
    let state = [&engine[..], &[3], b"{+info} 1\0", &[1, 0]].concat();
    let r = round_trip("AI_STL_S", &spawn("stalker", 7, &state), &upd, true);
    let Object::Stalker(s) = &r.object else {
        panic!()
    };
    let script = s.script.as_ref().unwrap();
    assert_eq!(
        script
            .job_online
            .as_ref()
            .unwrap()
            .condition_list
            .as_deref(),
        Some("{+info} 1")
    );
    assert_eq!(
        (script.was_in_smart_terrain, script.death_items_dropped),
        (Some(true), Some(false))
    );
    assert_eq!(s.start_dialog, "dialog");
    // Version 4: only the job-online byte; version 2: nothing.
    let r = round_trip(
        "AI_STL_S",
        &spawn("stalker", 4, &[&engine[..], &[1]].concat()),
        &upd,
        true,
    );
    let Object::Stalker(s) = &r.object else {
        panic!()
    };
    assert_eq!(s.script.as_ref().unwrap().was_in_smart_terrain, None);
    round_trip("AI_STL_S", &spawn("stalker", 2, &engine), &upd, true);
    // The engine kind appends nothing.
    let r = round_trip("AI_STL", &spawn("stalker", 7, &engine), &upd, true);
    assert_eq!(r.kind(), Kind::Stalker);
    // A scripted monster at version 7: job byte and one bool8.
    let monster = bytes(|w| {
        monster_like(w);
        skeleton(w);
        w.u16(0xFFFF);
    });
    let r = round_trip(
        "SM_ZOMBI",
        &spawn("zombie", 7, &[&monster[..], &[0, 1]].concat()),
        &update(&bytes(monster_update)),
        true,
    );
    let Object::Monster(m) = &r.object else {
        panic!()
    };
    assert_eq!(m.script.as_ref().unwrap().death_items_dropped, None);
}

#[test]
fn smart_zones_anomalies_and_weapons() {
    let restrictor = bytes(|w| {
        object_block(w);
        write_shapes(
            w,
            &[Shape::Sphere {
                centre: [0.0; 3],
                radius: 2.0,
            }],
        )
        .unwrap();
        w.u8(3);
    });
    let empty = update(&[]);
    let r = round_trip(
        "SMRTTRRN",
        &spawn("smart_terrain", 6, &[&restrictor[..], &[0, 0, 0]].concat()),
        &empty,
        true,
    );
    let Object::SmartZone(z) = &r.object else {
        panic!()
    };
    assert_eq!(
        z.script,
        SmartZoneScript::SmartTerrain(SmartTerrain {
            duration_end: None,
            idle_end: None,
            working: None
        })
    );
    // Working: the rest is kept as stored.
    let working = [
        &restrictor[..],
        &[12, 5, 1, 5, 30, 0, 0, 0],
        &[0, 1],
        b"gulag",
    ]
    .concat();
    let r = round_trip(
        "SMRTTRRN",
        &spawn("smart_terrain", 7, &working),
        &empty,
        true,
    );
    let Object::SmartZone(z) = &r.object else {
        panic!()
    };
    let SmartZoneScript::SmartTerrain(t) = &z.script else {
        panic!()
    };
    assert_eq!(t.working.as_deref(), Some(&b"gulag"[..]));
    assert_eq!(t.duration_end.unwrap().minute, 30);
    round_trip(
        "RE_SPAWN",
        &spawn("respawn", 7, &[&restrictor[..], &[2, 1, 0, 2, 0]].concat()),
        &empty,
        true,
    );
    round_trip("SMRTZONE", &spawn("zone", 7, &restrictor), &empty, true);

    let anomaly = bytes(|w| {
        object_block(w);
        write_shapes(w, &[Shape::Box([[1.0; 3]; 4])]).unwrap();
        w.u8(0);
        w.float(1.0);
        w.u32(0xFFFF_FFFF);
        w.u32(0);
        w.u32(0);
        w.u32(0);
        w.float(10.0);
        w.u16(0);
        w.u32(0);
    });
    // Single-player: the ZS_ byte (here 1 and an unset time); otherwise none.
    let r = round_trip(
        "ZS_MBALD",
        &spawn("zone_mb", 6, &[&anomaly[..], &[1, 0]].concat()),
        &empty,
        true,
    );
    let Object::Anomaly(a) = &r.object else {
        panic!()
    };
    assert_eq!(a.script.as_ref().unwrap().last_spawn, Some(None));
    round_trip("ZS_MBALD", &spawn("zone_mb", 6, &anomaly), &empty, false);
    round_trip("Z_MBALD", &spawn("zone_mb", 6, &anomaly), &empty, true);

    let weapon = bytes(|w| {
        object_block(w);
        visual(w);
        w.float(1.0);
        w.u16(30);
        w.u16(10);
        w.u8(0);
        w.u8(0);
        w.u8(0);
    });
    let weapon_update = bytes(|w| {
        w.u8(0);
        w.bytes(&[255, 0]);
        w.u16(10);
        w.bytes(&[0, 0, 0, 0]);
    });
    let r = round_trip(
        "W_SHOTGN",
        &spawn("wpn_bm16", 6, &weapon),
        &update(&[&weapon_update[..], &[1, 2, 0, 1]].concat()),
        true,
    );
    let Object::Shotgun(s) = &r.object else {
        panic!()
    };
    assert_eq!(s.loaded_ammo_types, [0, 1]);
    round_trip(
        "W_AK74",
        &spawn("wpn_ak74", 6, &weapon),
        &update(&[&[1][..], &weapon_update, &[1]].concat()),
        true,
    );
    round_trip(
        "WP_PM",
        &spawn("wpn_pm", 6, &weapon),
        &update(&[&weapon_update[..], &[1]].concat()),
        true,
    );
    round_trip(
        "W_KNIFE",
        &spawn("wpn_knife", 6, &weapon),
        &update(&weapon_update),
        true,
    );
}

#[test]
fn class_ids_map_to_kinds() {
    for (id, kind) in [
        ("SM_POLTR", Kind::Monster),
        ("AF_MEDUSA", Kind::Item),
        ("II_BOLT", Kind::Item),
        ("WP_WALTH", Kind::MagazineWeapon),
        ("AI_FLE_G", Kind::MonsterGroup),
        ("ON_OFF_G", Kind::OnOffGroup),
        ("Z_TEAMBS", Kind::TeamBase),
    ] {
        assert_eq!(Kind::from_class_id(id).unwrap(), kind, "{id}");
    }
    for (id, _) in NOT_IN_SAVES {
        let e = Kind::from_class_id(id).unwrap_err().to_string();
        assert!(e.contains("cannot occur in a save"), "{e}");
    }
    assert!(
        Kind::from_class_id("NOPE")
            .unwrap_err()
            .to_string()
            .contains("unknown")
    );
}

#[test]
fn rejects_invalid_records() {
    let state = bytes(|w| {
        object_block(w);
        visual(w);
    });
    let empty = update(&[]);
    round_trip("O_INVBOX", &spawn("inventory_box", 6, &state), &empty, true);
    // Wrong update message, empty state, wrong version, a bolt can't be saved.
    assert!(ObjectRecord::parse(&spawn("box", 6, &state), &[1, 0], "O_INVBOX", true).is_err());
    assert!(ObjectRecord::parse(&spawn("box", 6, &[]), &empty, "O_INVBOX", true).is_err());
    let mut old = spawn("box", 6, &state);
    let at = 2 + 4 + 5 + 2 + 24 + 8 + 2;
    old[at] = 117;
    assert!(ObjectRecord::parse(&old, &empty, "O_INVBOX", true).is_err());
    assert!(ObjectRecord::parse(&spawn("box", 6, &state), &empty, "SPECT", true).is_err());
    assert_eq!(
        section(&spawn("inventory_box", 6, &state)).unwrap(),
        "inventory_box"
    );

    let item = [&state[..], &1.0f32.to_le_bytes()].concat();
    let bolt = round_trip("II_BOLT", &spawn("bolt", 6, &item), &update(&[0]), true);
    assert!(!bolt.can_be_saved());
    let medkit = round_trip("II_MEDKI", &spawn("medkit", 6, &item), &update(&[0]), true);
    assert!(medkit.can_be_saved());
}

/// Writes a record and parses it back to the same record.
fn round_trip_record(r: &ObjectRecord) {
    let (s, u) = r.write().unwrap();
    let back = ObjectRecord::parse(&s, &u, r.class_id.as_str(), r.single_player).unwrap();
    assert_eq!(&back, r);
}

#[test]
fn writer_rejects_records_no_reader_can_parse() {
    let fails = |r: &ObjectRecord| r.write().is_err();
    // Another kind than the class id's.
    let state = bytes(|w| {
        object_block(w);
        visual(w);
    });
    let mut r = round_trip("O_INVBOX", &spawn("box", 6, &state), &update(&[]), true);
    r.class_id = ClassId::new("O_BRKBL").unwrap();
    assert!(fails(&r));
    assert!(ClassId::new("SPECT").is_err());

    // Scripted stalker additions against class id and script version.
    let engine = bytes(|w| {
        trader_like(w);
        monster_like(w);
        w.list(&[0u8; 5], |w, &v| w.u8(v)).unwrap();
        w.list(&[0u8; 4], |w, &v| w.u8(v)).unwrap();
        skeleton(w);
    });
    let upd = update(&bytes(|w| {
        monster_update(w);
        w.string("").unwrap();
    }));
    let stalker = round_trip(
        "AI_STL_S",
        &spawn("stalker", 7, &[&engine[..], &[0, 1, 0]].concat()),
        &upd,
        true,
    );
    let mut r = stalker.clone();
    r.header.script_version = 4;
    assert!(fails(&r));
    let mut r = stalker.clone();
    r.class_id = ClassId::new("AI_STL").unwrap();
    assert!(fails(&r));
    // A later field without the earlier ones.
    let mut r = stalker.clone();
    let Object::Stalker(s) = &mut r.object else {
        panic!()
    };
    s.script.as_mut().unwrap().job_online = None;
    assert!(fails(&r));
    // Script version 2 has no scripted fields, but the class still has the
    // (empty) addition.
    let mut r = stalker.clone();
    r.header.script_version = 2;
    let Object::Stalker(s) = &mut r.object else {
        panic!()
    };
    s.script = Some(CharacterScript {
        job_online: None,
        was_in_smart_terrain: None,
        death_items_dropped: None,
    });
    round_trip_record(&r);
    // Skeleton bit 0x04 without bone state.
    let mut r = stalker.clone();
    let Object::Stalker(s) = &mut r.object else {
        panic!()
    };
    s.skeleton.flags |= SKELETON_BONE_STATE;
    assert!(fails(&r));

    // ZS_ addition against single-player, smart zones against class id.
    let anomaly = bytes(|w| {
        object_block(w);
        write_shapes(w, &[]).unwrap();
        w.bytes(&[0; 1 + 4 * 5 + 4 + 2 + 4]);
    });
    let zs = spawn("zone", 6, &[&anomaly[..], &[0]].concat());
    let mut r = round_trip("ZS_MBALD", &zs, &update(&[]), true);
    r.single_player = false;
    assert!(fails(&r));
    let restrictor = bytes(|w| {
        object_block(w);
        write_shapes(w, &[]).unwrap();
        w.u8(0);
    });
    let smart = spawn("smart", 6, &[&restrictor[..], &[0, 0, 0]].concat());
    let mut r = round_trip("SMRTTRRN", &smart, &update(&[]), true);
    r.class_id = ClassId::new("SMRTZONE").unwrap();
    assert!(fails(&r));
    r.class_id = ClassId::new("RE_SPAWN").unwrap();
    assert!(fails(&r));

    // The monster group's update flag repeats its state flag.
    let monster = bytes(|w| {
        monster_like(w);
        skeleton(w);
        w.u16(0xFFFF);
    });
    let group = [
        &monster[..],
        &1u32.to_le_bytes(),
        &[0, 0],
        &0u32.to_le_bytes(),
    ]
    .concat();
    let upd = [update(&bytes(monster_update)), 1u32.to_le_bytes().to_vec()].concat();
    let mut r = round_trip("AI_FLE_G", &spawn("flesh_group", 6, &group), &upd, true);
    let Object::MonsterGroup(g) = &mut r.object else {
        panic!()
    };
    g.update_flag = 0;
    assert!(fails(&r));
}
