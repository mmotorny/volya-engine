// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use volya_engine_formats::particles::Library;

use super::*;
use crate::particles::fixture::muhi;
use crate::particles::placed::period;

fn section(text: &str, name: &str, behaviour: Behaviour) -> io::Result<ZoneSection> {
    let text = format!("{text}{TEST_HIT_KEYS}");
    let config = volya_engine_formats::ltx::parse(text.as_bytes()).unwrap();
    ZoneSection::read(config.section(name).unwrap(), behaviour)
}

const GALANTINE: &str = "[zone_witches_galantine]\nidle_particles = tests\\muhi\n\
     idle_sound = anomaly\\electra_idle1\nidle_light = off\nawaking_time = 100\n\
     blowout_time = 1000\naccamulate_time = 1000\n\
     visible_by_detector = on\nblowout_sound = anomaly\\electra_blast1\n\
     hit_sound = anomaly\\electra_hit\nentrance_sound = anomaly\\electra_hit\n";

fn record(e: u32, d: u32, s: u32) -> ZoneRecord {
    ZoneRecord {
        id: 7,
        name: "esc_zone_witches_galantine".into(),
        class_id: "ZS_MBALD".into(),
        position: [10.0, 2.0, -5.0],
        angles: [0.0; 3],
        shapes: vec![Shape::Sphere {
            centre: [0.0; 3],
            radius: 3.0,
        }],
        enabled_time: e,
        disabled_time: d,
        start_time_shift: s,
        always_fast: false,
        maximum_power: 15.0,
    }
}

fn effects() -> PlacedEffects {
    let library = Arc::new(Library::new(vec![muhi()], Vec::new()));
    PlacedEffects::new(library, &[], 0).unwrap()
}

/// Runs a zone created at clock 0 for `ms` in frames of `step` ms with the
/// camera at `camera`, and returns the clocks at which its idle sound
/// started and stopped.
fn run(zone: &ZoneRecord, ms: u32, step: u32, camera: Vec3) -> (Vec<u32>, Zones, PlacedEffects) {
    let mut fx = effects();
    let mut zones = Zones::default();
    let s = section(GALANTINE, "zone_witches_galantine", Behaviour::Mbald).unwrap();
    zones.create(zone, s, 0, &mut fx).unwrap();
    let mut changes = Vec::new();
    let mut clock = 0;
    while clock < ms {
        zones.clear_events();
        clock += step;
        fx.frame(u64::from(step), camera);
        let frame = Frame {
            clock,
            elapsed: step,
            camera,
            colours: &NoColourAnimations,
            player: None,
        };
        zones.frame(&frame, &mut fx).unwrap();
        for e in zones.sound_events() {
            assert!(matches!(
                e,
                SoundEvent::Play { .. } | SoundEvent::Stop { .. }
            ));
            changes.push(clock);
        }
    }
    (changes, zones, fx)
}

/// The times at which the zone of `(e, d, s)` turns on or off in its first
/// `seconds` (P9 8's table: its on windows), whole seconds.
fn switches(e: u32, d: u32, s: u32, seconds: u32) -> Vec<u32> {
    let r = record(e, d, s);
    let (changes, _, _) = run(&r, seconds * 1000, 16, r.position);
    // Each change at the first 16 ms frame at or past its second.
    changes.iter().map(|&c| c / 1000).collect()
}

#[test]
fn the_first_levels_galantines_switch_as_the_table_gives() {
    // P9 8 (M1, M2): on windows in the first 60 s; a zone that is off at
    // creation goes off at its first switching test (time 0 here).
    type Case = ((u32, u32, u32), &'static [u32]);
    let cases: [Case; 9] = [
        ((9, 15, 0), &[9, 24, 33, 48, 57]),
        ((9, 15, 18), &[0, 6, 15, 30, 39, 54]),
        ((9, 15, 12), &[0, 12, 21, 36, 45]),
        ((9, 15, 6), &[3, 18, 27, 42, 51]),
        ((9, 21, 6), &[3, 24, 33, 54]),
        ((9, 21, 12), &[0, 18, 27, 48, 57]),
        ((9, 21, 18), &[0, 12, 21, 42, 51]),
        ((9, 21, 24), &[0, 6, 15, 36, 45]),
        ((9, 21, 0), &[9, 30, 39]),
    ];
    for ((e, d, s), want) in cases {
        assert_eq!(switches(e, d, s, 60), want, "{e}, {d}, {s}");
    }
}

#[test]
fn the_idle_stop_and_start_follow_the_state() {
    let r = record(9, 15, 18);
    // P9 3.1: started on creation whatever its times.
    let mut fx = effects();
    let mut zones = Zones::default();
    let s = section(GALANTINE, "zone_witches_galantine", Behaviour::Mbald).unwrap();
    zones.create(&r, s, 0, &mut fx).unwrap();
    let z = &zones.zones()[0];
    assert_eq!(z.state, State::Idle);
    assert!(z.armed && z.fast && z.sound_on);
    assert_eq!(
        zones.sound_events(),
        [SoundEvent::Play {
            zone: 0,
            position: r.position
        }]
    );
    let o = z.effect.unwrap();
    assert!(fx.get(o).unwrap().instance.is_playing());
    // Its first switching test disables it: sound stopped, effect stopped
    // at once with every particle gone (P9 4.2), kept for the next start.
    let (_, zones, fx) = run(&r, 32, 16, r.position);
    let z = &zones.zones()[0];
    assert_eq!(z.state, State::Disabled);
    assert!(!z.sound_on);
    let p = fx.get(z.effect.unwrap()).unwrap();
    assert!(!p.instance.is_playing());
    assert_eq!(
        p.instance
            .effects()
            .map(|e| e.particles().len())
            .sum::<usize>(),
        0
    );
    // Enabled again at 6 s: played and placed by X again.
    let (_, zones, fx) = run(&r, 6_100, 16, r.position);
    let z = &zones.zones()[0];
    assert_eq!(z.state, State::Idle);
    assert!(z.sound_on);
    assert_eq!(z.effect, Some(o));
    let p = fx.get(o).unwrap();
    assert!(p.instance.is_playing());
    assert!(p.instance.effects().all(|e| e.position() == r.position));
}

#[test]
fn a_zone_without_times_stays_idle() {
    let r = record(0, 0, 0);
    let (changes, zones, _) = run(&r, 30_000, 16, r.position);
    assert!(changes.is_empty());
    assert!(!zones.zones()[0].armed);
    assert_eq!(zones.zones()[0].state, State::Idle);
}

#[test]
fn far_from_the_camera_the_zone_leaves_fast_mode() {
    // P9 6.2: fast mode while |E − c| − r ≤ 50 m.
    let r = record(0, 0, 0);
    let near = [r.position[0] + 52.9, r.position[1], r.position[2]];
    let far = [r.position[0] + 53.1, r.position[1], r.position[2]];
    let (_, zones, _) = run(&r, 100, 16, near);
    assert!(zones.zones()[0].fast);
    let (_, zones, _) = run(&r, 100, 16, far);
    assert!(!zones.zones()[0].fast);
}

#[test]
fn scheduled_periods_grow_with_distance() {
    // P9 9.2 (M5): zones, then effect objects (P8's periods).
    let zones: Vec<u32> = [0.0, 10.0, 50.0, 100.0, 199.0, 200.0, 500.0]
        .into_iter()
        .map(scheduled_period)
        .collect();
    assert_eq!(zones, [30, 78, 272, 515, 995, 1000, 1000]);
    let effects: Vec<u32> = [0.0, 50.0, 100.0, 200.0, 400.0]
        .into_iter()
        .map(period)
        .collect();
    assert_eq!(effects, [30, 153, 277, 525, 525]);
}

#[test]
fn the_light_range_spans_r_minus_to_plus_delta() {
    // P9 9.2 (M3).
    let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
    for (r, d, lo, hi) in [
        (3.0, 0.25, 2.75, 3.25),
        (15.0, 0.25, 14.75, 15.25),
        (15.0, 0.2, 14.8, 15.2),
    ] {
        assert!(close(light_range(r, d, 0), lo));
        assert!(close(light_range(r, d, 32_767), hi));
    }
}

#[test]
fn the_light_colour_is_the_entrys_red_green_blue_over_255() {
    // P9 9.2 (M4): `light_green_02` at T = 0, whose value is
    // 0x0099D538 (lanims-xr.md 5).
    let c = light_colour(0x0099_D538);
    let want = [0.6, 0.835_294, 0.219_608, 1.0];
    assert!((0..4).all(|i| (c[i] - want[i]).abs() < 1e-6), "{c:?}");
}

const BUZZ: &str = "[zone_buzz]\nidle_particles = tests\\muhi\nidle_light = on\n\
     idle_light_range = 3.0\nidle_light_range_delta = 0.25\nidle_light_anim = light_green_02\n\
     idle_light_height = 0.70\nawaking_time = 1\nblowout_time = 1\naccamulate_time = 1\n\
     visible_by_detector = on\n";

struct Green;

impl ColourAnimations for Green {
    fn colour_at(&self, name: &str, _: f32) -> Option<u32> {
        (name == "light_green_02").then_some(0x0099_D538)
    }
}

#[test]
fn the_idle_light_is_on_while_idle_and_steps_at_each_update() {
    let mut fx = effects();
    let mut zones = Zones::default();
    let s = section(BUZZ, "zone_buzz", Behaviour::Bfuzz).unwrap();
    let r = record(1, 1, 0);
    zones.create(&r, s, 0, &mut fx).unwrap();
    // P9 4.1: range R, at p + (0, h, 0), active; the colour not yet set.
    let l = zones.lights().next().unwrap();
    assert_eq!(l.range, 3.0);
    assert_eq!(l.position, [10.0, 2.7, -5.0]);
    assert!(l.colour.is_none() && l.shadow);
    // The first update: a colour from the animation and a range drawn from
    // the effects' generator.
    let frame = Frame {
        clock: 16,
        elapsed: 16,
        camera: r.position,
        colours: &Green,
        player: None,
    };
    let mut expected = fx.random_mut().uniform.get();
    zones.frame(&frame, &mut fx).unwrap();
    let l = zones.lights().next().unwrap();
    assert_eq!(l.colour, Some(light_colour(0x0099_D538)));
    assert_eq!(l.range, light_range(3.0, 0.25, expected.next_k()));
    // Disabled at 1 s: inactive; enabled at 2 s: active again.
    let mut clock = 16;
    while clock < 1_100 {
        clock += 16;
        let frame = Frame {
            clock,
            elapsed: 16,
            camera: r.position,
            colours: &Green,
            player: None,
        };
        zones.frame(&frame, &mut fx).unwrap();
    }
    assert!(zones.lights().next().is_none());
    assert!(zones.zones()[0].light.is_some());
}

#[test]
fn sections_are_read_as_the_zone_reads_them() {
    // P9 1.2: with `idle_light` off its keys are not read.
    let flame = "[zone_flame_small]\nidle_particles = explosions\\campfire_05\n\
         idle_sound = ambient\\fire2\nidle_light = off\nidle_light_anim = koster_00\n\
         awaking_time = 0\nblowout_time = 1000\naccamulate_time = 0\nvisible_by_detector = on\n\
         blowout_sound = ambient\\fire2\nartefact_born_sound = x\n";
    let s = section(flame, "zone_flame_small", Behaviour::Mbald).unwrap();
    assert_eq!(s.idle_particles.as_deref(), Some("explosions\\campfire_05"));
    assert!(s.idle_light.is_none());
    assert_eq!(s.sounds[IDLE].as_deref(), Some("ambient\\fire2"));
    // S15 2.1: the artefact sound only with `spawn_blowout_artefacts` on.
    assert!(s.sounds[6].is_none());
    assert_eq!(s.sounds[3].as_deref(), Some("ambient\\fire2"));
    // sound-detector.md 5.2: `visible_by_detector` is required.
    assert!(s.visible_by_detector);
    let hidden = flame.replace("visible_by_detector = on", "visible_by_detector = off");
    assert!(
        !section(&hidden, "zone_flame_small", Behaviour::Mbald)
            .unwrap()
            .visible_by_detector
    );
    let without = flame.replace("visible_by_detector = on\n", "");
    assert!(section(&without, "zone_flame_small", Behaviour::Mbald).is_err());
    // A missing `idle_light` is fatal.
    let none = "[z]\nawaking_time = 0\nblowout_time = 0\naccamulate_time = 0\n\
         visible_by_detector = on\n";
    assert!(section(none, "z", Behaviour::Mbald).is_err());
    // S15 2.1: the `ZS_MINCE` behaviour needs `body_tearing_sound` and
    // `tele_height`.
    let mincer = "[m]\nidle_light = off\nawaking_time = 50\nblowout_time = 10000\n\
         accamulate_time = 4000\nvisible_by_detector = on\n\
         idle_sound = anomaly\\gravi_idle01\n";
    assert!(section(mincer, "m", Behaviour::Mincer).is_err());
    let mincer = format!("{mincer}body_tearing_sound = anomaly\\anomaly_body_tear_1\n");
    assert!(section(&mincer, "m", Behaviour::Mincer).is_err());
    let mincer = format!("{mincer}tele_height = 10\n");
    let s = section(&mincer, "m", Behaviour::Mincer).unwrap();
    assert_eq!(s.tele_height, Some(10.0));
    assert_eq!(s.sounds[7].as_deref(), Some("anomaly\\anomaly_body_tear_1"));
    // The `Z_MBALD` behaviour does not read it.
    let s = section(&mincer, "m", Behaviour::Mbald).unwrap();
    assert!(s.sounds[7].is_none() && s.tele_height.is_none());
}

#[test]
fn an_unknown_effect_is_an_error() {
    let mut fx = effects();
    let mut zones = Zones::default();
    let text = GALANTINE.replace("tests\\muhi", "nope");
    let s = section(&text, "zone_witches_galantine", Behaviour::Mbald).unwrap();
    assert!(zones.create(&record(0, 0, 0), s, 0, &mut fx).is_err());
}

#[test]
fn shape_spheres() {
    let one = [Shape::Sphere {
        centre: [1.0, 2.0, 3.0],
        radius: 4.0,
    }];
    assert_eq!(shape_sphere(&one), ([1.0, 2.0, 3.0], 4.0));
    // A unit box at the origin: corners ± 0.5, half its diagonal.
    let cube = [Shape::Box([
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0; 3],
    ])];
    let (c, r) = shape_sphere(&cube);
    assert_eq!(c, [0.0; 3]);
    assert!((r - 3f32.sqrt() * 0.5).abs() < 1e-6);
}

#[test]
fn the_idle_light_is_a_dynamic_light_record() {
    // P9 4.4: a record of its own, shadow on, point; active while idle.
    let mut fx = effects();
    let mut zones = Zones::default();
    let mut lights = crate::lights::Lights::default();
    let s = section(BUZZ, "zone_buzz", Behaviour::Bfuzz).unwrap();
    zones.create(&record(1, 1, 0), s, 0, &mut fx).unwrap();
    zones.sync_lights(&mut lights);
    let id = zones.zones()[0].light.as_ref().unwrap().record.unwrap();
    let r = lights.get(id).unwrap();
    assert!(r.active && r.shadow);
    assert_eq!(r.kind, crate::lights::LightKind::Point);
    assert_eq!((r.range, r.position), (3.0, [10.0, 2.7, -5.0]));
    let mut clock = 0;
    while clock < 1_100 {
        clock += 16;
        let frame = Frame {
            clock,
            elapsed: 16,
            camera: [10.0, 2.0, -5.0],
            colours: &Green,
            player: None,
        };
        zones.frame(&frame, &mut fx).unwrap();
        zones.sync_lights(&mut lights);
        if clock == 16 {
            assert_eq!(lights.get(id).unwrap().colour, light_colour(0x0099_D538));
        }
    }
    // Disabled at 1 s: the same record, inactive.
    assert!(!lights.get(id).unwrap().active);
    assert_eq!(lights.len(), 1);
}

/// The section `text` as `behaviour` reads it, without the test hit keys.
fn exact(text: &str, behaviour: Behaviour) -> ZoneSection {
    let config = volya_engine_formats::ltx::parse(text.as_bytes()).unwrap();
    let name = config.sections().next().unwrap().name().to_owned();
    ZoneSection::read(config.section(&name).unwrap(), behaviour).unwrap()
}

/// Runs zone `record` of `section` for `ms` in 10 ms frames with the
/// player standing at `at` (alive, radius 1.3399 m, at rest; the camera
/// at its feet): the clocks and effects on the player, and the zones.
fn with_player(
    record: &ZoneRecord,
    section: ZoneSection,
    at: Vec3,
    ms: u32,
) -> (Vec<(u32, PlayerEffect)>, Zones) {
    let mut fx = effects();
    let mut zones = Zones::default();
    zones.create(record, section, 0, &mut fx).unwrap();
    let player = Target {
        position: at,
        radius: 1.3399,
        alive: true,
        speed: 0.0,
    };
    let mut out = Vec::new();
    let mut clock = 0;
    while clock < ms {
        clock += 10;
        fx.frame(10, at);
        let frame = Frame {
            clock,
            elapsed: 10,
            camera: at,
            colours: &NoColourAnimations,
            player: Some(player),
        };
        zones.frame(&frame, &mut fx).unwrap();
        out.extend(zones.take_player_effects().into_iter().map(|e| (clock, e)));
    }
    (out, zones)
}

const FLAME: &str = "[zone_flame_small]\nidle_light = off\nawaking_time = 0\n\
     blowout_time = 1000\naccamulate_time = 0\nvisible_by_detector = on\n\
     hit_type = burn\nattenuation = 1\neffective_radius = 1.3\nhit_impulse_scale = 0\n\
     ignore_nonalive = on\nignore_small = off\nignore_artefacts = off\n\
     disable_time = -1\ndisable_time_small = -1\n";

fn flame(at: Vec3) -> (Vec<(u32, PlayerEffect)>, Zones) {
    let mut r = record(0, 0, 0);
    r.shapes = vec![Shape::Sphere {
        centre: [0.0; 3],
        radius: 0.642,
    }];
    with_player(&r, exact(FLAME, Behaviour::Mbald), at, 3_100)
}

#[test]
fn a_fire_burns_the_player_inside_once_per_blowout() {
    // env 6.1 (M2, M7): P = 15 within 1.3399 m of the centre; with 10 ms
    // frames the hits come 20, 1030, 2040 ms after it starts awaking, here
    // at the first scheduled update (10 ms).
    let (hits, zones) = flame([10.0, 2.3, -5.0]);
    let clocks: Vec<u32> = hits.iter().map(|(c, _)| *c).collect();
    assert_eq!(clocks, [30, 1040, 2050, 3060]);
    for (_, e) in &hits {
        assert_eq!(e.zone, 7);
        assert!(e.draws_direction);
        let h = e.hit.unwrap();
        assert_eq!((h.kind, h.power, h.bone), (HitType::BURN, 15.0, Some(0)));
    }
    assert!(zones.zones()[0].counts());
}

#[test]
fn outside_its_shapes_a_zone_stays_idle() {
    // env 4.1: the player's feet 0.65 m from the centre, outside.
    let (hits, zones) = flame([10.65, 2.0, -5.0]);
    assert!(hits.is_empty());
    assert_eq!(zones.zones()[0].state, State::Idle);
    assert!(zones.zones()[0].player.is_none());
}

#[test]
fn relative_power_falls_to_zero_at_the_reach() {
    // env 4.3 (M1, M2): R = 0.642 × 1.3 = 0.8346; with a = 0.1 from 1 to
    // 0.9, then 0.
    let reach = 0.642 * 1.3;
    assert_eq!(relative_power(1.0, 0.0, reach), 1.0);
    assert!((relative_power(1.0, reach / 2.0, reach) * 15.0 - 11.25).abs() < 1e-5);
    assert_eq!(relative_power(1.0, reach * 1.001, reach), 0.0);
    assert!((relative_power(0.1, 1.0, 1.0) - 0.9).abs() < 1e-6);
    assert_eq!(relative_power(0.1, 1.001, 1.0), 0.0);
}

const RADIATION: &str = "[zone_radioactive_killing]\nidle_light = off\n\
     awaking_time = 100\nblowout_time = 100\naccamulate_time = 100\n\
     visible_by_detector = off\nattenuation = 0.1\neffective_radius = 1.0\nperiod = 1\n\
     hit_type = telepatic\nignore_nonalive = off\nignore_small = off\n\
     ignore_artefacts = off\ndisable_time = -1\ndisable_time_small = -1\n";

#[test]
fn a_radiation_zone_hits_with_radiation_once_per_blowout() {
    // env 6.3 (M3): M 0.05 gives 0.05 at the centre, of type radiation
    // whatever `hit_type` says, no bone; a blowout every 200 ms.
    let mut r = record(0, 0, 0);
    r.class_id = "Z_RADIO".into();
    r.maximum_power = 0.05;
    let (hits, _) = with_player(&r, exact(RADIATION, Behaviour::Radio), r.position, 1_000);
    assert!(hits.len() >= 4, "{hits:?}");
    for w in hits.windows(2) {
        assert_eq!(w[1].0 - w[0].0, 200);
    }
    let h = hits[0].1.hit.unwrap();
    assert_eq!((h.kind, h.power, h.bone), (HitType::RADIATION, 0.05, None));
    assert!(!hits[0].1.draws_direction);
    // 0.045 at the reach, r = 3.
    let at = [r.position[0] + 2.999, r.position[1], r.position[2]];
    let (hits, _) = with_player(&r, exact(RADIATION, Behaviour::Radio), at, 1_000);
    assert!((hits[0].1.hit.unwrap().power - 0.045).abs() < 1e-4);
}

const MINCER: &str = "[zone_mincer]\nidle_light = off\nawaking_time = 50\n\
     blowout_time = 10000\naccamulate_time = 4000\nvisible_by_detector = on\n\
     body_tearing_sound = x\ntele_height = 10\nhit_type = strike\nattenuation = 1\n\
     hit_impulse_scale = 0.1\nblowout_explosion_time = 5999\nignore_nonalive = off\n\
     ignore_small = off\nignore_artefacts = off\ndisable_time = -1\n\
     disable_time_small = -1\nthrow_in_atten = 1.5\nthrow_in_impulse_alive = 1000\n\
     blowout_radius_percent = 0.3\nactor_blowout_radius_percent = 0.3\n";

#[test]
fn a_mincer_hits_the_player_only_in_its_core_at_the_explosion() {
    // env 6.4 (M5): M 50, core 0.3 × r: 48 at the core's edge.
    let mut r = record(0, 0, 0);
    r.maximum_power = 50.0;
    let section = exact(MINCER, Behaviour::Mincer);
    let edge = [r.position[0] + 0.9, r.position[1], r.position[2]];
    let (hits, _) = with_player(&r, section.clone(), edge, 7_000);
    assert_eq!(hits.len(), 1);
    let (clock, e) = hits[0];
    assert!((6_000..6_100).contains(&clock), "{clock}");
    let h = e.hit.unwrap();
    assert_eq!((h.kind, h.bone), (HitType::STRIKE, Some(0)));
    assert!((h.power - 48.0).abs() < 1e-3, "{}", h.power);
    // Outside the core: pulled, never hit.
    let out = [r.position[0] + 1.0, r.position[1], r.position[2]];
    let (hits, zones) = with_player(&r, section, out, 7_000);
    assert!(hits.is_empty());
    assert_eq!(zones.zones()[0].state, State::Blowout);
}

#[test]
fn zone_hit_types_ignore_case_and_refuse_physic_strike() {
    // env 1.2.
    assert_eq!(
        zone_hit_type("Chemical_Burn").unwrap(),
        HitType::CHEMICAL_BURN
    );
    assert_eq!(zone_hit_type("wound_2").unwrap(), HitType::WOUND_2);
    assert!(zone_hit_type("physic_strike").is_err());
    assert!(zone_hit_type("nope").is_err());
}

#[test]
fn a_box_shape_holds_points_within_its_faces() {
    // env 4.1: faces at ± ½ of each axis, 0.001 m of tolerance; the
    // zone at (10, 2, −5).
    let mut r = record(0, 0, 0);
    r.shapes = vec![Shape::Box([
        [2.0, 0.0, 0.0],
        [0.0, 4.0, 0.0],
        [0.0, 0.0, 6.0],
        [0.0; 3],
    ])];
    let mut fx = effects();
    let mut zones = Zones::default();
    zones
        .create(&r, exact(FLAME, Behaviour::Mbald), 0, &mut fx)
        .unwrap();
    let z = &zones.zones()[0];
    assert!(z.contains_point([11.0005, 2.0, -5.0]));
    assert!(!z.contains_point([11.002, 2.0, -5.0]));
    assert!(z.contains_point([10.0, 3.9, -7.9]));
    assert!(!z.contains_point([10.0, 4.1, -5.0]));
}
