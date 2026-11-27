// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use volya_engine_formats::object::Shape;
use volya_engine_formats::particles::Library;

use super::*;
use crate::particles::placed::PlacedEffects;
use crate::zones::{Behaviour, ZoneRecord, ZoneSection};

/// `detector_simple` as S21 7.1 gives it, with the zone classes' sections.
const SIMPLE: &str = "[zone_mosquito_bald]\nclass = ZS_MBALD\n\
     [zone_mincer]\nclass = ZS_MINCE\n\
     [zone_radioactive]\nclass = Z_RADIO\n\
     [zone_gravi_zone]\nclass = ZS_GALAN\n\
     [detector_simple]\nclass = D_SIMDET\nradius = 10\nslot = 8\nef_detector_type = 1\n\
     noise = zone_mosquito_bald\nbuzzer = zone_mosquito_bald\nbuzzer_radius = 1.5\n\
     zone_class_1 = zone_mosquito_bald\nzone_min_freq_1 = 0.1\nzone_max_freq_1 = 40\n\
     zone_sound_1_ = detectors\\DA-2_beep1\nzone_sound_1_1 = detectors\\DA-2_beep1\n\
     zone_sound_1_2 = detectors\\DA-2_beep1\nzone_sound_1_3 = detectors\\DA-2_beep1\n\
     zone_class_2 = zone_mincer\nzone_min_freq_2 = 0.1\nzone_max_freq_2 = 20\n\
     zone_sound_2_ = detectors\\DA-2_beep1\n\
     zone_class_3 = zone_radioactive\nzone_min_freq_3 = 1\nzone_max_freq_3 = 20\n\
     zone_sound_3_ = detectors\\geiger_1\nzone_sound_3_1 = detectors\\geiger_2\n\
     zone_sound_3_2 = detectors\\geiger_3\nzone_sound_3_3 = detectors\\geiger_4\n\
     zone_sound_3_4 = detectors\\geiger_5\nzone_sound_3_5 = detectors\\geiger_6\n\
     zone_sound_3_6 = detectors\\geiger_1\nzone_sound_3_7 = detectors\\geiger_1\n\
     zone_sound_3_8 = detectors\\geiger_1\n\
     zone_class_4 = zone_gravi_zone\nzone_min_freq_4 = 0.5\nzone_max_freq_4 = 30\n\
     zone_sound_4_ = detectors\\DA-2_beep1\n\
     zone_class_6 = zone_mincer\nzone_min_freq_6 = 5\nzone_max_freq_6 = 6\n\
     zone_sound_6_ = x\n";

fn config(text: &str) -> Config {
    volya_engine_formats::ltx::parse(text.as_bytes()).unwrap()
}

fn simple() -> DetectorSection {
    DetectorSection::read(&config(SIMPLE), "detector_simple").unwrap()
}

#[test]
fn the_table_is_read_as_7_1_gives_it() {
    let s = simple();
    assert_eq!(s.radius, 10.0);
    // 2.1: the first missing number ends the table (entry 6 is not read).
    let classes: Vec<_> = s.entries.iter().map(|e| e.zone_class.as_str()).collect();
    assert_eq!(classes, ["ZS_MBALD", "ZS_MINCE", "Z_RADIO", "ZS_GALAN"]);
    let freqs: Vec<_> = s.entries.iter().map(|e| (e.min_freq, e.max_freq)).collect();
    assert_eq!(freqs, [(0.1, 40.0), (0.1, 20.0), (1.0, 20.0), (0.5, 30.0)]);
    let counts: Vec<_> = s.entries.iter().map(|e| e.sounds.len()).collect();
    assert_eq!(counts, [4, 1, 9, 1]);
    assert!(
        s.entries
            .iter()
            .flat_map(|e| &e.sounds)
            .all(|x| x.volume == 1.0 && x.delay == 0.0)
    );
    let geiger: Vec<_> = s.entries[2]
        .sounds
        .iter()
        .map(|x| x.name.as_str())
        .collect();
    assert_eq!(
        geiger,
        [
            "detectors\\geiger_1",
            "detectors\\geiger_2",
            "detectors\\geiger_3",
            "detectors\\geiger_4",
            "detectors\\geiger_5",
            "detectors\\geiger_6",
            "detectors\\geiger_1",
            "detectors\\geiger_1",
            "detectors\\geiger_1",
        ]
    );
    assert_eq!(s.entry("Z_RADIO"), Some(2));
    assert_eq!(s.entry("Z_MBALD"), None);
}

#[test]
fn sound_items_default_and_lists_end_at_a_gap() {
    let c = config("[s]\nk_ = a, 0.5, 2\nk_1 = b,,\nk_2 = c, , 1.5\nk_4 = d\n[t]\nother = x\n");
    let list = sound_list(c.section("s").unwrap(), "k_").unwrap();
    assert_eq!(
        list,
        [
            ListSound {
                name: "a".into(),
                volume: 0.5,
                delay: 2.0
            },
            ListSound {
                name: "b".into(),
                volume: 1.0,
                delay: 0.0
            },
            ListSound {
                name: "c".into(),
                volume: 1.0,
                delay: 1.5
            },
        ]
    );
    // 2.2: an empty list is an assertion failure.
    assert!(sound_list(c.section("t").unwrap(), "k_").is_err());
}

#[test]
fn assertions_and_missing_keys_are_fatal() {
    let base = "[z]\nclass = ZS_MBALD\n[d]\nradius = 10\nef_detector_type = 1\n\
         zone_class_1 = z\nzone_min_freq_1 = 1\nzone_max_freq_1 = 2\nzone_sound_1_ = a\n";
    assert!(DetectorSection::read(&config(base), "d").is_ok());
    // fmin < fmax is asserted.
    let equal = base.replace("zone_max_freq_1 = 2", "zone_max_freq_1 = 1");
    assert!(DetectorSection::read(&config(&equal), "d").is_err());
    for key in [
        "radius = 10\n",
        "ef_detector_type = 1\n",
        "zone_min_freq_1 = 1\n",
        "zone_sound_1_ = a\n",
        "class = ZS_MBALD\n",
    ] {
        let without = base.replace(key, "");
        assert!(
            DetectorSection::read(&config(&without), "d").is_err(),
            "{key}"
        );
    }
    // A section the table names that does not exist.
    let missing = base.replace("zone_class_1 = z", "zone_class_1 = y");
    assert!(DetectorSection::read(&config(&missing), "d").is_err());
    // 2.1: a class id named again replaces the earlier entry.
    let again = format!(
        "{base}zone_class_2 = z\nzone_min_freq_2 = 3\nzone_max_freq_2 = 4\nzone_sound_2_ = b\n"
    );
    let s = DetectorSection::read(&config(&again), "d").unwrap();
    assert_eq!(s.entries.len(), 1);
    assert_eq!((s.entries[0].min_freq, s.entries[0].max_freq), (3.0, 4.0));
    assert_eq!(s.entries[0].sounds[0].name, "b");
}

#[test]
fn the_rate_follows_8_2() {
    // M1: entry 1, R = 2.7388 (a `zone_mosquito_bald_weak`).
    let s = simple();
    let e = &s.entries[0];
    let f = |d: f32| frequency(e, 10.0, [d, 0.0, 0.0], [0.0; 3], 2.7388);
    assert_eq!(f(0.0), 40.0);
    assert_eq!(f(2.191), 40.0);
    assert!((f(5.0) - 10.769).abs() < 5e-4, "{}", f(5.0));
    assert!((f(10.0) - 0.19195).abs() < 5e-6, "{}", f(10.0));
    assert_eq!(f(12.192), 0.1);
    assert_eq!(f(100.0), 0.1);
    assert!((period(f(5.0)) - 92.856).abs() < 5e-3, "{}", period(f(5.0)));
    assert!(
        (period(f(10.0)) - 5209.6).abs() < 0.05,
        "{}",
        period(f(10.0))
    );
    assert_eq!(period(0.1), 10_000.0);
    // M3: entry 3, R = 6.0904, s = 3.
    let r = frequency(&s.entries[2], 10.0, [0.0, 3.0, 0.0], [0.0; 3], 6.0904);
    assert_eq!((r, period(r)), (20.0, 50.0));
}

#[test]
fn the_range_test_is_against_r_plus_rho() {
    assert!(in_range([12.9, 0.0, 0.0], [0.0; 3], 3.0, 10.0));
    assert!(!in_range([13.0, 0.0, 0.0], [0.0; 3], 3.0, 10.0));
    assert!(in_range([0.0, 0.0, -12.99], [0.0; 3], 3.0, 10.0));
}

/// A zone section of class `class_id` with `visible_by_detector`.
fn zone_section(visible: bool) -> ZoneSection {
    let text = format!(
        "[z]\nidle_light = off\nawaking_time = 0\nblowout_time = 0\naccamulate_time = 0\n\
         visible_by_detector = {}\n{}",
        if visible { "on" } else { "off" },
        crate::zones::TEST_HIT_KEYS
    );
    ZoneSection::read(config(&text).section("z").unwrap(), Behaviour::Mbald).unwrap()
}

/// Zones of (class id, position, sphere radius, visible).
fn zones(list: &[(&str, [f32; 3], f32, bool)]) -> Zones {
    let library = Arc::new(Library::new(Vec::new(), Vec::new()));
    let mut fx = PlacedEffects::new(library, &[], 0).unwrap();
    let mut zones = Zones::default();
    for (i, &(class_id, position, radius, visible)) in list.iter().enumerate() {
        let record = ZoneRecord {
            id: i as u16,
            name: format!("zone_{i}"),
            class_id: class_id.into(),
            position,
            angles: [0.0; 3],
            shapes: vec![Shape::Sphere {
                centre: [0.0; 3],
                radius,
            }],
            enabled_time: 0,
            disabled_time: 0,
            start_time_shift: 0,
            always_fast: false,
            maximum_power: 0.0,
        };
        zones
            .create(&record, zone_section(visible), 0, &mut fx)
            .unwrap();
    }
    zones
}

/// A detector carried by the player, on (in slot 8), at `at`.
fn carried(at: [f32; 3]) -> Detector {
    let mut d = Detector::new();
    assert!(!d.on);
    d.set_parent(true, at);
    d.placed(Place::Slot(8));
    assert!(d.on);
    d
}

/// The frames (the first after the range test counting as 0) at which a
/// detector held at `at` ticks for `frames` frames of `step` ms.
fn ticks(zones: &Zones, at: [f32; 3], step: u32, frames: usize) -> Vec<usize> {
    let s = simple();
    let mut d = carried(at);
    d.range_test(&s, zones);
    let mut out = Vec::new();
    let mut at_frames = Vec::new();
    for frame in 0..frames {
        out.clear();
        d.frame(step, at, &s, zones, &mut out);
        if !out.is_empty() {
            at_frames.push(frame);
        }
    }
    at_frames
}

/// The first `n` entries of `v`.
fn first(v: &[usize], n: usize) -> &[usize] {
    &v[..n.min(v.len())]
}

#[test]
fn ticks_come_at_the_frames_of_8_2() {
    // M2: entry 1, R = 2.7388, frames of 16 ms.
    let z = zones(&[("ZS_MBALD", [0.0; 3], 2.7388, true)]);
    assert_eq!(first(&ticks(&z, [0.0; 3], 16, 20), 3), [2, 5, 8]);
    assert_eq!(first(&ticks(&z, [5.0, 0.0, 0.0], 16, 30), 3), [6, 13, 20]);
    assert_eq!(first(&ticks(&z, [10.0, 0.0, 0.0], 16, 700), 2), [326, 653]);
    // At fmin (12.5 m, within R + ρ = 12.7388 of the sphere's centre).
    assert_eq!(
        first(&ticks(&z, [12.5, 0.0, 0.0], 16, 1300), 2),
        [626, 1253]
    );
    // M3: entry 3, R = 6.0904, s = 3: f = 20, T = 50.
    let r = zones(&[("Z_RADIO", [0.0; 3], 6.0904, true)]);
    assert_eq!(first(&ticks(&r, [3.0, 0.0, 0.0], 16, 20), 3), [4, 9, 14]);
    assert_eq!(first(&ticks(&r, [3.0, 0.0, 0.0], 33, 20), 3), [2, 5, 8]);
    // M4: f = 40, T = 25: c = T does not tick.
    assert_eq!(first(&ticks(&z, [0.0; 3], 26, 10), 3), [1, 3, 5]);
    assert_eq!(first(&ticks(&z, [0.0; 3], 25, 10), 3), [2, 5, 8]);
}

#[test]
fn a_tick_names_its_entry_and_zones_count_independently() {
    let s = simple();
    let z = zones(&[
        ("ZS_MBALD", [0.0; 3], 2.0, true),
        ("Z_RADIO", [1.0, 0.0, 0.0], 2.0, true),
        // Not in the table, and not visible: never tick.
        ("Z_MBALD", [0.0; 3], 2.0, true),
        ("ZS_MINCE", [0.0; 3], 2.0, false),
    ]);
    let mut d = carried([0.0; 3]);
    d.range_test(&s, &z);
    assert_eq!(d.range, [0, 1, 2, 3]);
    assert_eq!(d.zones.len(), 4);
    let mut out = Vec::new();
    for _ in 0..200 {
        d.frame(16, [0.0; 3], &s, &z, &mut out);
    }
    assert!(out.contains(&SoundCommand::Tick { entry: 0, zone: 0 }));
    assert!(out.contains(&SoundCommand::Tick { entry: 2, zone: 1 }));
    assert!(
        out.iter()
            .all(|c| matches!(c, SoundCommand::Tick { entry: 0 | 2, .. }))
    );
    // The skipped zones' counters are unchanged.
    assert_eq!(d.zones[2].counter, 0);
    assert_eq!(d.zones[3].counter, 0);
}

#[test]
fn the_zone_list_follows_the_range_test() {
    let s = simple();
    let mut z = zones(&[("ZS_MBALD", [0.0; 3], 2.0, true)]);
    let mut d = carried([20.0, 0.0, 0.0]);
    d.range_test(&s, &z);
    assert!(d.range.is_empty() && d.zones.is_empty());
    // 4.2: a zone disabled as it comes into range joins the range list
    // only, and not the zone list while it stays in range, even enabled.
    z.zones_mut()[0].state = State::Disabled;
    d.position = [11.0, 0.0, 0.0];
    d.range_test(&s, &z);
    assert_eq!((d.range.len(), d.zones.len()), (1, 0));
    z.zones_mut()[0].state = State::Idle;
    d.range_test(&s, &z);
    assert_eq!((d.range.len(), d.zones.len()), (1, 0));
    // Out of range and back: it joins both.
    d.position = [13.0, 0.0, 0.0];
    d.range_test(&s, &z);
    assert!(d.range.is_empty() && d.zones.is_empty());
    d.position = [5.0, 0.0, 0.0];
    d.range_test(&s, &z);
    assert_eq!(
        d.zones,
        [Listed {
            zone: 0,
            counter: 0
        }]
    );
    // A zone of the zone list that becomes disabled stays there.
    z.zones_mut()[0].state = State::Disabled;
    d.range_test(&s, &z);
    assert_eq!(d.zones.len(), 1);
}

#[test]
fn the_scheduled_update_follows_the_parent_while_on() {
    let s = simple();
    let z = zones(&[("ZS_MBALD", [0.0; 3], 2.0, true)]);
    let mut d = carried([50.0, 0.0, 0.0]);
    // Due at once; the position becomes the parent's and the test runs.
    d.scheduled(0, [0.0; 3], [1.0, 0.0, 0.0], true, &s, &z);
    assert_eq!(d.position, [1.0, 0.0, 0.0]);
    assert_eq!(d.zones.len(), 1);
    // The next is due 34 ms on (the detector 1 m from the camera, P9 6.2).
    d.scheduled(33, [0.0; 3], [40.0, 0.0, 0.0], true, &s, &z);
    assert_eq!(d.position, [1.0, 0.0, 0.0]);
    d.scheduled(34, [0.0; 3], [40.0, 0.0, 0.0], true, &s, &z);
    assert!(d.zones.is_empty());
    // Not the controlled object's: the position follows, no test runs.
    d.scheduled(300, [0.0; 3], [1.0, 0.0, 0.0], false, &s, &z);
    assert_eq!(d.position, [1.0, 0.0, 0.0]);
    assert!(d.zones.is_empty());
    // In the backpack it is off: nothing follows or is tested.
    d.placed(Place::Backpack);
    assert!(!d.on);
    d.scheduled(1000, [0.0; 3], [7.0, 0.0, 0.0], true, &s, &z);
    assert_eq!(d.position, [1.0, 0.0, 0.0]);
    // On the belt it is on again.
    d.placed(Place::Belt);
    assert!(d.on);
}

#[test]
fn no_ticks_without_holder_or_while_off_and_losing_the_parent_empties_the_lists() {
    let s = simple();
    let z = zones(&[("ZS_MBALD", [0.0; 3], 2.0, true)]);
    let mut out = Vec::new();
    // Another character's detector has no holder.
    let mut d = Detector::new();
    d.set_parent(false, [0.0; 3]);
    d.placed(Place::Slot(8));
    d.range_test(&s, &z);
    for _ in 0..100 {
        d.frame(16, [0.0; 3], &s, &z, &mut out);
    }
    assert!(out.is_empty());
    // Off: no ticks; the counters keep.
    let mut d = carried([0.0; 3]);
    d.range_test(&s, &z);
    d.placed(Place::Backpack);
    for _ in 0..100 {
        d.frame(16, [0.0; 3], &s, &z, &mut out);
    }
    assert!(out.is_empty());
    assert_eq!(d.zones[0].counter, 0);
    // 3: losing the parent stops its sounds and empties its lists.
    assert_eq!(d.lose_parent(), SoundCommand::StopAll);
    assert!(d.range.is_empty() && d.zones.is_empty() && !d.holder && !d.parent);
}
