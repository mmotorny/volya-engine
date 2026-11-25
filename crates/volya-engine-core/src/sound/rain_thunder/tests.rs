// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The rain and thunder rules of docs/behavior/sound-rain-thunder.md with
//! the worked examples of section 8.

use std::collections::VecDeque;
use std::io;

use super::*;
use crate::sound::Sound;
use crate::sound::system::NoDevice;
use crate::vfs::db::DbVfs;
use volya_engine_formats::sound_params::SoundParams;

fn sound(name: &str, min: f32, max: f32) -> Sound {
    Sound {
        name: name.into(),
        path: format!("gamedata\\sounds\\{name}.ogg"),
        params: SoundParams {
            min_distance: min,
            max_distance: max,
            base_volume: 1.0,
            ai_sound_type: 0,
            max_ai_distance: max,
        },
        samples: 264_209,
    }
}

const EYE: Vec3 = [-246.726_3, -23.263_26, -134.433_87];

#[test]
fn rain_volume_follows_density_and_sky() {
    // Section 8.1 (M3).
    let v = |density: f32, h: f32| sky_factor(h) * density * 1.1;
    let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
    assert!(close(v(0.85, 0.3), 0.935));
    assert!(close(v(0.85, 0.5), 0.935));
    assert!(close(v(0.85, 0.2), 0.748));
    assert!(close(v(0.85, 0.1), 0.561));
    assert!(close(v(0.85, 0.0), 0.374));
    assert!(close(v(0.2, 0.4), 0.22));
    assert!(close(v(0.05, 0.4), 0.055));
}

#[test]
fn rain_plays_looped_above_the_camera_while_dense_enough() {
    let mut s = SoundSystem::new(16);
    s.preload(sound(RAIN, 300.0, 300.0));
    let tree = DbVfs::<io::Empty>::new();
    let id = s
        .create(&tree, &PathAliases::default(), RAIN, Category::Effects)
        .unwrap();
    let mut rain = Rain::default();
    rain.frame(&mut s, id, 0.0005, 1.0, EYE);
    assert!(!rain.on && !s.has_emitter(id));
    rain.frame(&mut s, id, 0.85, 1.0, EYE);
    assert!(rain.on);
    let e = &s.emitters()[0];
    assert!(e.looped && !e.two_d);
    assert_eq!(e.range, (40.0, 80.0));
    assert_eq!(e.position, [EYE[0], EYE[1] + 40.0, EYE[2]]);
    assert!((e.volume - 0.935).abs() < 1e-6);
    rain.frame(&mut s, id, 0.0, 1.0, EYE);
    assert!(!rain.on && !s.has_emitter(id));
    // On without an emitter (a level change): silent until it turns off
    // and on again (4.4).
    rain.frame(&mut s, id, 0.5, 1.0, EYE);
    s.stop_all();
    s.update(16, &mut NoDevice);
    rain.frame(&mut s, id, 0.5, 1.0, EYE);
    assert!(rain.on && !s.has_emitter(id));
}

#[test]
fn the_sun_heading_points_bolts_towards_the_sun() {
    // Section 8.2 (M5): at 12:00 the section's `sun_dir` is altitude −25°,
    // longitude 291° (the disputed wording aside, this is the data): β =
    // −69° (291° modulo 360°).
    let (alt, lon) = ((-25.0f32).to_radians(), 291.0f32.to_radians());
    let sun = [-lon.sin() * alt.cos(), alt.sin(), lon.cos() * alt.cos()];
    let beta = sun_heading(sun).to_degrees();
    assert!((beta + 69.0).abs() < 1e-3, "{beta}");
    // Headings β + 180° ± 17°: 94° … 128°.
    assert!((beta + 180.0 - 17.0 - 94.0).abs() < 1e-3);
    // Heading 111°, altitude 15°.
    let (h, a) = (111.0f32.to_radians(), 15.0f32.to_radians());
    let d = [-h.sin() * a.cos(), a.sin(), h.cos() * a.cos()];
    for (x, y) in d.iter().zip([-0.901_77, 0.258_82, -0.346_16]) {
        assert!((x - y).abs() < 1e-5, "{d:?}");
    }
    assert_eq!(sun_heading([0.0, -1.0, 0.0]), 0.0);
    assert_eq!(sun_heading([-1.0, 0.0, 0.0]), std::f32::consts::FRAC_PI_2);
}

#[test]
fn thunder_is_heard_at_a_fixed_fraction_of_full_level() {
    // Section 5.5 (M4): range s/2 … 2s at distance s: 1/1.75.
    let s = 330.0;
    let g = crate::sound::system::distance_gain(s / 2.0, 2.0 * s, 0.75, s);
    assert!((g - 0.571_429).abs() < 1e-6, "{g}");
}

/// Draws scripted as fractions *u* of 32,767.
struct Script(VecDeque<f32>);

impl Draws for Script {
    fn draw(&mut self) -> u32 {
        let u = self.0.pop_front().expect("a scripted draw");
        (u * MAX as f32).round() as u32
    }
}

#[test]
fn the_worked_schedule_of_section_8_2() {
    // M6: P = 5, D = 0.25, F = 350, dt = 0.02, frames from T = 100.
    let collection = Collection {
        name: "thunderbolt_collection_default".into(),
        descriptions: vec![None; 4],
    };
    let weather = ThunderWeather {
        collection: Some(&collection),
        period: 5.0,
        duration: 0.25,
        far_plane: 350.0,
        sun_dir: [0.0, -1.0, 0.0],
    };
    let common = BoltCommon {
        altitude: (10.0, 20.0),
        delta_longitude: 17.0,
        min_dist_factor: 0.94,
        second_probability: 0.5,
    };
    // Per bolt: life, description, α, φ, s, three for the picture, the
    // sound decision, then (with sound) the jitter.
    let bolt = |s: f32, decision: f32, jitter: Option<f32>| {
        let mut d = vec![0.5, 0.0, 0.5, 0.5, s, 0.0, 0.0, 0.0, decision];
        d.extend(jitter);
        d
    };
    let mut script = vec![0.5];
    script.extend(bolt(0.0, 0.25, None));
    script.extend(bolt(1.0, 0.75, Some(0.5)));
    script.extend(bolt(0.5, 0.75, Some(0.5)));
    let mut draws = Script(script.into());
    let mut state = ThunderState::default();
    let mut bolts = Vec::new();
    let mut stopped = Vec::new();
    for k in 0..520 {
        let ms = 100_000 + 20 * k;
        let t = ms as f32 / 1000.0;
        let was = state.running;
        let f = state.frame(&mut draws, t, 0.02, &weather, &common, EYE);
        if k == 0 {
            assert!((state.next - 105.0).abs() < 1e-3);
        }
        if let Some(thunder) = f.bolt {
            bolts.push((ms, thunder, state.next));
        } else if was && !state.running {
            stopped.push(ms);
        }
    }
    assert_eq!(stopped[0], 105_280);
    assert_eq!(bolts.len(), 3);
    let (ms, thunder, next) = &bolts[0];
    assert_eq!(*ms, 105_020);
    // Silent: no description sound (the descriptions here have none).
    assert!(thunder.is_none());
    assert!((next - 105.271).abs() < 1e-3, "{next}");
    let (ms, _, next) = &bolts[1];
    assert_eq!(*ms, 105_300);
    assert!((next - 110.30).abs() < 1e-3, "{next}");
    assert_eq!(bolts[2].0, 110_320);
}

#[test]
fn a_bolt_with_sound_plays_its_thunder_delayed_at_its_point() {
    let mut s = SoundSystem::new(16);
    s.preload(sound("nature\\thunder-0", 1.0, 300.0));
    let tree = DbVfs::<io::Empty>::new();
    let collection = Collection::create(
        &mut s,
        &tree,
        &PathAliases::default(),
        "c",
        &[Some("nature\\thunder-0".into())],
    )
    .unwrap();
    let thunder = Thunder {
        description: 0,
        position: [1.0, 2.0, 3.0],
        distance: 332.5,
        delay: 332.5 / 300.0,
    };
    play_thunder(&mut s, &collection, &thunder);
    let e = &s.emitters()[0];
    assert_eq!(e.phase, crate::sound::system::Phase::Waiting);
    assert_eq!(e.position, [1.0, 2.0, 3.0]);
    assert_eq!(e.range, (166.25, 665.0));
    assert!(!e.looped && !e.two_d);
}

#[test]
fn common_values_are_capped() {
    let c = volya_engine_formats::ltx::parse(
        b"[thunderbolt_common]\naltitude = 10, 20\ndelta_longitude = 17\n\
          min_dist_factor = 0.99\nsecond_propability = 1.5\n\
          tilt = 15\nsky_color = 1, 1, 1\nsun_color = 1, 1, 1\nfog_color = 1, 1, 1\n",
    )
    .unwrap();
    let common = BoltCommon::read(&c).unwrap();
    assert_eq!(common.altitude, (10.0, 20.0));
    assert_eq!(common.min_dist_factor, 0.95);
    assert_eq!(common.second_probability, 1.0);
}

/// ltx-config.md 11: the keys of 2.2 are read unconditionally, so a
/// missing one is fatal.
#[test]
fn a_missing_bolt_key_is_fatal() {
    let c = volya_engine_formats::ltx::parse(
        b"[thunderbolt_common]\naltitude = 10, 20\ndelta_longitude = 17\n\
          min_dist_factor = 0.5\ntilt = 15\nsky_color = 1\nsun_color = 1\nfog_color = 1\n\
          [storm]\nbolt_1\nbolt_2\n[bolt_1]\nsound = thunder\n[bolt_2]\nsound =\n\
          [quiet]\nbolt_3\n[bolt_3]\nlightning_model = m\n",
    )
    .unwrap();
    let err = BoltCommon::read(&c).unwrap_err();
    assert!(err.to_string().contains("second_propability"), "{err}");
    assert_eq!(
        collection_sounds(&c, "storm").unwrap(),
        [Some("thunder".to_owned()), None]
    );
    let err = collection_sounds(&c, "quiet").unwrap_err();
    assert!(err.to_string().contains("sound"), "{err}");
}

#[test]
fn smoothed_frame_time() {
    let mut s = ThunderState::default();
    assert!((s.smooth_frame_time(0.02) - 0.018).abs() < 1e-7);
    assert!((s.smooth_frame_time(0.02) - 0.0198).abs() < 1e-7);
    assert_eq!(s.smooth_frame_time(1.0), 0.1);
}
