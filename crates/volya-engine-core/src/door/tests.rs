// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use volya_engine_formats::ltx;

use super::*;

const LOGIC: &[u8] = b"[ph_door@locked]\nlocked = true\nsnd_open_start = trader_door_unlock\n\
    on_info = {+esc_trader_can_leave} ph_door@closed %=play_snd(device\\door_servomotor)%\n\
    [ph_door@closed]\nlocked = false\n[ph_door@tips]\nlocked = true\nshow_tips = false\n\
    tip_open = tip_other\nclosed = false\n";

fn section(name: &str) -> DoorSection {
    let config = ltx::parse(LOGIC).unwrap();
    DoorSection::read(config.section(name).unwrap())
}

#[test]
fn the_logic_starts_in_the_locked_section() {
    assert_eq!(
        start_section("scripts\\ESC_trader_door.ltx"),
        Some("ph_door@locked")
    );
    assert_eq!(start_section("scripts\\esc_trader.ltx"), None);
    assert!(is_door_section("ph_door@locked") && is_door_section("ph_door"));
    assert!(!is_door_section("ph_doors@x") && !is_door_section("sr_idle@ph_door"));
    // 2.1: closed and show_tips default to true, the locked hint id to
    // tip_door_locked.
    assert_eq!(
        section("ph_door@locked"),
        DoorSection {
            name: "ph_door@locked".into(),
            locked: true,
            closed: true,
            show_tips: true,
            locked_tip: LOCKED_TIP.into(),
            snd_open_start: Some("trader_door_unlock".into()),
        }
    );
    let s = section("ph_door@tips");
    assert!(s.locked && !s.closed && !s.show_tips && s.locked_tip == "tip_other");
    assert_eq!(s.snd_open_start, None);
}

#[test]
fn starting_closes_the_door_holds_it_and_sets_the_hint() {
    let mut door = Door::new("trader_door".into(), section("ph_door@locked"), Vec::new());
    // 2.3: no motor and no hint before the first update.
    assert_eq!(
        (door.motor(), door.tip(), door.started()),
        (None, None, false)
    );
    door.update();
    assert!(door.started());
    assert_eq!(door.motor(), Some(CLOSING));
    assert_eq!(door.tip(), Some("tip_door_locked"));
    // Each setting is taken once, for the hinge (2.3).
    assert_eq!(
        (door.take_motor(), door.take_motor()),
        (Some(CLOSING), None)
    );
    // A step that finds it open waits; the next that finds it closed holds
    // it with 10000 / 1.
    door.physics_step(false);
    assert_eq!((door.motor(), door.take_motor()), (Some(CLOSING), None));
    door.physics_step(true);
    assert_eq!(door.motor(), Some(HOLDING));
    assert_eq!(door.take_motor(), Some(HOLDING));
    door.update();
    door.physics_step(true);
    assert_eq!((door.motor(), door.take_motor()), (Some(HOLDING), None));
    // Unlocked: no hint; not closed: no motor.
    let mut open = Door::new("d".into(), section("ph_door@closed"), Vec::new());
    open.update();
    assert_eq!((open.motor(), open.tip()), (Some(CLOSING), None));
    let mut quiet = Door::new("d".into(), section("ph_door@tips"), Vec::new());
    quiet.update();
    quiet.physics_step(true);
    assert_eq!((quiet.motor(), quiet.tip()), (None, None));
}

#[test]
fn the_crosshair_picks_the_leaf() {
    let leaf = BodyBox {
        centre: [0.703, 1.0, -0.013],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        half_size: [0.690, 1.322, 0.125],
        material: 1,
    };
    let mut door = Door::new("trader_door".into(), section("ph_door@locked"), Vec::new());
    assert_eq!(door.pick([0.7, 1.5, -2.038], [0.0, 0.0, 1.0]), None);
    door.set_boxes(vec![leaf]);
    // The leaf spans x 0.013 … 1.393, z −0.138 … 0.112; looking along +z
    // from 1.9 m before its near face.
    let eye = [0.7, 1.5, -2.038];
    let t = door.pick(eye, [0.0, 0.0, 1.0]).unwrap();
    assert!((t - 1.9).abs() < 1e-5, "{t}");
    assert_eq!(door.pick(eye, [0.0, 0.0, -1.0]), None);
    assert_eq!(door.pick([3.0, 1.5, -2.038], [0.0, 0.0, 1.0]), None);
}

/// 2.5: a use of the locked door plays `trader_door_unlock`,
/// `device\door_servomotor`, at the door's position, each time, and changes
/// nothing else.
#[test]
fn a_use_of_the_locked_door_plays_its_sound_and_leaves_it_locked() {
    let at = [10.0, 2.0, -5.0];
    let mut door = Door::new("trader_door".into(), section("ph_door@locked"), Vec::new()).at(at);
    assert_eq!(
        door.sounds().collect::<Vec<_>>(),
        ["device\\door_servomotor"]
    );
    assert_eq!(door.use_action(), None, "the scheme not started");
    door.update();
    door.physics_step(true);
    let before = door.clone();
    let played = DoorSound {
        sound: "device\\door_servomotor",
        position: at,
    };
    for _ in 0..2 {
        assert_eq!(door.use_action(), Some(played.clone()));
    }
    assert_eq!(door, before);
    assert_eq!(door.tip(), Some("tip_door_locked"));
    // Unlocked, or locked without the key or with a set not known: none.
    let mut unlocked = Door::new("d".into(), section("ph_door@closed"), Vec::new());
    let mut quiet = Door::new("d".into(), section("ph_door@tips"), Vec::new());
    let mut other = section("ph_door@locked");
    other.snd_open_start = Some("some_set".into());
    let mut other = Door::new("d".into(), other, Vec::new());
    for d in [&mut unlocked, &mut quiet, &mut other] {
        d.update();
        assert_eq!(d.use_action(), None, "{}", d.section.name);
        assert_eq!(d.sounds().count(), 0);
    }
    assert_eq!(
        sound_set("TRADER_door_unlock"),
        Some("device\\door_servomotor")
    );
}
