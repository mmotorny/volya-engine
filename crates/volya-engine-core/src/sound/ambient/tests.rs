// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The lists, sets and rule of docs/behavior/sound-weather-ambient.md with
//! the worked examples of section 7.

use std::io;

use super::*;
use crate::sound::Sound;
use crate::vfs::db::DbVfs;
use volya_engine_formats::sound_params::SoundParams;

#[test]
fn list_items_follow_section_2_1() {
    assert!(list_items(None).is_empty());
    assert!(list_items(Some("")).is_empty());
    assert_eq!(list_items(Some("a")), ["a"]);
    assert_eq!(list_items(Some("a,b,")), ["a", "b"]);
    assert_eq!(list_items(Some("a,,b")), ["a", ""]);
    assert_eq!(list_items(Some("a,b,c")), ["a", "b", "c"]);
    assert_eq!(list_items(Some("a,a")), ["a", "a"]);
    assert_eq!(list_items(Some(",a")), ["", "a"]);
}

fn config(text: &str) -> Config {
    volya_engine_formats::ltx::parse(text.as_bytes()).unwrap()
}

/// Sets like `ambient_env_tuman` and `ambient_env_morning` (section 5.2):
/// 33 and 30 sounds, periods and distances as listed, effects 4, 6 and 4.
fn tuman_and_morning() -> Config {
    let names = |prefix: &str, n: usize| {
        (0..n)
            .map(|i| format!("{prefix}{i}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    config(&format!(
        "[ambient_env_tuman]\nsounds = {},\nsound_period = 5, 10\nsound_dist = 60, 30\n\
         effects = ae0_effect_4, ae0_effect_6\neffect_period = 5, 10\n\
         [ambient_env_morning]\nsounds = {},\nsound_period = 5, 10\nsound_dist = 30, 60\n\
         effects = ae0_effect_4\neffect_period = 30, 60\n\
         [ambient_env_evening]\nsounds = x\nsound_period = 5, 10\nsound_dist = 30, 60\n\
         effects =\neffect_period = 90, 120\n\
         [ae0_effect_4]\nlife_time = 15\nparticles = nature\\fog_foggy_00\n\
         sound = ambient\\rnd_outdoor\\rnd_wind_2\noffset = 0, 0, 0\nwind_gust_factor = 0.015\n\
         [ae0_effect_6]\nlife_time = 7\nparticles = nature\\fog_stormy_01\n\
         sound = ambient\\rnd_outdoor\\rnd_wind_1\noffset = 0, 0, 0\nwind_gust_factor = 0.02\n",
        names("t", 33),
        names("m", 30)
    ))
}

#[test]
fn sets_read_as_section_2_2() {
    let c = tuman_and_morning();
    let tuman = SetData::read(&c, "ambient_env_tuman").unwrap();
    // Section 7: P = 5,000 … 10,000 ms, d 30 … 60 m (swapped here), Q =
    // 5,000 … 10,000 ms.
    assert_eq!(tuman.sounds.len(), 33);
    assert_eq!(tuman.sound_period, Some((5_000, 10_000)));
    assert_eq!(tuman.sound_dist, Some((30.0, 60.0)));
    assert_eq!(tuman.effect_period, Some((5_000, 10_000)));
    assert_eq!(tuman.effects[0].life, 15_000);
    assert_eq!(tuman.effects[1].wind_gust_factor, 0.02);
    let evening = SetData::read(&c, "ambient_env_evening").unwrap();
    assert!(evening.effects.is_empty());
    assert_eq!(evening.effect_period, None);
}

/// ltx-config.md 11: a period or distance read (the set has sounds or
/// effects, 2.2) and missing is fatal; without sounds it is not read.
#[test]
fn a_missing_period_is_fatal() {
    let c = config(
        "[with_sounds]\nsounds = x\nsound_period = 5, 10\n\
         [without]\neffects =\n",
    );
    let err = SetData::read(&c, "with_sounds").unwrap_err();
    assert!(err.to_string().contains("sound_dist"), "{err}");
    assert!(SetData::read(&c, "without").is_ok());
}

fn sound(name: &str) -> Sound {
    Sound {
        name: name.into(),
        path: format!("gamedata\\sounds\\{name}.ogg"),
        params: SoundParams {
            min_distance: 5.0,
            max_distance: 75.0,
            base_volume: 1.0,
            ai_sound_type: 0,
            max_ai_distance: 75.0,
        },
        samples: 220_500,
    }
}

fn create(system: &mut SoundSystem, data: SetData) -> AmbientSet {
    for n in data
        .sounds
        .iter()
        .chain(data.effects.iter().filter_map(|e| e.sound.as_ref()))
    {
        system.preload(sound(&resolved(n)));
    }
    let tree = DbVfs::<io::Empty>::new();
    AmbientSet::create(system, &tree, &PathAliases::default(), data).unwrap()
}

fn resolved(name: &str) -> String {
    crate::sound::resolved_name(name)
}

/// The start eye (section 7).
const EYE: Vec3 = [-246.726_3, -23.263_26, -134.433_87];

fn close(a: Vec3, b: Vec3, tolerance: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tolerance)
}

#[test]
fn placement_of_a_random_sound() {
    // Section 7 (M2): r_angle 8,192, r_dist 16,384 → a = 1.570844,
    // d = 45.00046.
    let a = 8_192.0f32 * ANGLE;
    assert!((a - 1.570_844).abs() < 1e-6, "{a}");
    let d = 30.0 + 16_384.0f32 * 30.0 / MAX as f32;
    assert!((d - 45.000_46).abs() < 1e-4, "{d}");
    let p = [EYE[0] + d * a.cos(), EYE[1] + HEIGHT, EYE[2] + d * a.sin()];
    assert!(close(p, [-246.728, -13.263, -89.433], 1e-3), "{p:?}");
}

#[test]
fn set_choice_by_the_blend_weight() {
    // Section 7 (M1).
    let b = |t: f32, r: u32| 1.0 - t <= r as f32 * (1.0 / MAX as f32);
    assert!(!b(0.5, 16_383));
    assert!(b(0.5, 16_384));
    assert!(!b(0.0, 32_766));
    assert!(b(0.0, 32_767));
    assert!(b(1.0, 0));
}

/// Section 7's run (M3): seed 1, `tuman` (A) and `morning` (B) at t = 0.5,
/// frames every 20 ms to 60,000, outdoors at the start eye, particles gone
/// in the frame they are told to stop.
fn run(outdoors: bool) -> Vec<(u32, &'static str, AmbientFrame)> {
    let c = tuman_and_morning();
    let mut s = SoundSystem::new(16);
    let tuman = create(&mut s, SetData::read(&c, "ambient_env_tuman").unwrap());
    let morning = create(&mut s, SetData::read(&c, "ambient_env_morning").unwrap());
    let mut state = AmbientState::default();
    let mut events = Vec::new();
    for k in 1..=3_000 {
        let now = 20 * k;
        let f = state
            .frame(
                &mut s,
                now,
                (Some(&tuman), Some(&morning), 0.5),
                outdoors,
                EYE,
            )
            .unwrap();
        let set = if f.chose_b { "morning" } else { "tuman" };
        if f.sound.is_some() || f.effect.is_some() || f.effect_ended {
            events.push((now, set, f));
        }
    }
    events
}

#[test]
fn the_worked_run_of_section_7() {
    let events = run(true);
    let at = |now: u32| events.iter().find(|e| e.0 == now).unwrap();
    // 20: tuman, item 20 (18,467 mod 33), placed at (−229.572, −13.263,
    // −178.782); effect item 0 (`ae0_effect_4`), period 6,478, end 15,020.
    let (_, set, f) = at(20);
    assert_eq!(*set, "tuman");
    let (item, position) = f.sound.unwrap();
    assert_eq!(item, 20);
    assert!(
        close(position, [-229.572, -13.263, -178.782], 2e-3),
        "{position:?}"
    );
    assert_eq!(f.effect, Some(0));
    let sounds: Vec<(u32, &str)> = events
        .iter()
        .filter(|e| e.2.sound.is_some())
        .map(|e| (e.0, e.1))
        .collect();
    assert_eq!(
        sounds,
        [
            (20, "tuman"),
            (6_360, "tuman"),
            (14_560, "tuman"),
            (22_440, "tuman"),
            (31_420, "tuman"),
            (38_940, "morning"),
            (47_760, "tuman"),
            (56_840, "morning"),
        ]
    );
    let effects: Vec<(u32, &str)> = events
        .iter()
        .filter(|e| e.2.effect.is_some())
        .map(|e| (e.0, e.1))
        .collect();
    assert_eq!(effects, [(20, "tuman"), (15_040, "morning")]);
    let ended: Vec<u32> = events
        .iter()
        .filter(|e| e.2.effect_ended)
        .map(|e| e.0)
        .collect();
    assert_eq!(ended, [15_020, 30_040]);
}

#[test]
fn indoors_random_sounds_go_on_and_no_effect_starts() {
    // Section 7 (M4).
    let events = run(false);
    assert!(events.iter().all(|e| e.2.effect.is_none()));
    let sounds: Vec<u32> = events.iter().map(|e| e.0).collect();
    assert_eq!(sounds[..2], [20, 6_360]);
}

#[test]
fn without_sets_nothing_plays_but_a_number_is_drawn() {
    // Section 7 (M5).
    let mut s = SoundSystem::new(16);
    let mut state = AmbientState::default();
    let f = state
        .frame(&mut s, 20, (None, None, 0.5), true, EYE)
        .unwrap();
    assert_eq!(f, AmbientFrame::default());
    // The first draw (41) was taken: the next is 18,467.
    assert_eq!(s.random().draw(), 18_467);
}

#[test]
fn an_effect_ends_at_its_time_without_a_set() {
    // Section 7 (M6).
    let c = tuman_and_morning();
    let mut s = SoundSystem::new(16);
    let morning = create(&mut s, SetData::read(&c, "ambient_env_morning").unwrap());
    let mut state = AmbientState::default();
    let f = state
        .frame(&mut s, 20, (Some(&morning), Some(&morning), 0.5), true, EYE)
        .unwrap();
    assert_eq!(f.effect, Some(0));
    assert_eq!(state.wind_gust_factor, 0.015);
    let mut ended = None;
    for k in 2..1_000 {
        let f = state
            .frame(&mut s, 20 * k, (None, None, 0.5), true, EYE)
            .unwrap();
        if f.effect_ended {
            ended = Some(20 * k);
            break;
        }
    }
    assert_eq!(ended, Some(15_020));
    assert_eq!(state.wind_gust_factor, 0.0);
    // Its sound was played at E + offset and is not stopped.
    let wind = s.emitters();
    assert!(
        wind.iter()
            .any(|e| e.name.ends_with("rnd_wind_2") && !e.stopping)
    );
}

#[test]
fn a_period_with_equal_bounds_is_fatal() {
    // 4.7: the original faults at the set's first random sound.
    let c = tuman_and_morning();
    let mut s = SoundSystem::new(16);
    let mut data = SetData::read(&c, "ambient_env_tuman").unwrap();
    data.sound_period = Some((5_000, 5_000));
    let tuman = create(&mut s, data);
    let mut state = AmbientState::default();
    assert!(
        state
            .frame(&mut s, 20, (Some(&tuman), Some(&tuman), 0.5), false, EYE)
            .is_err()
    );
}
