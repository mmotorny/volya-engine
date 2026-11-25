// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The first level's sounds from the Russian edition's data (BYOA): the
//! verification facts of docs/behavior/sound-static-sources.md (S3),
//! sound-weather-ambient.md (S4), sound-level-music.md (S5),
//! sound-occlusion.md (S6) and sound-rain-thunder.md (S8) for
//! `l01_escape`, what plays at a New Game's start, and the cost of the
//! sound update with occlusion on the level (meaningful in release
//! builds). Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use volya_engine::core::collision::geometry::normalize;
use volya_engine::core::config;
use volya_engine::core::sound::ambient::list_items;
use volya_engine::core::sound::occlusion::Occlusion;
use volya_engine::core::sound::random::SharedRandom;
use volya_engine::core::sound::system::{Listener, NoDevice, SoundSystem, Vec3};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::weather::Weather;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::level_sound::{LevelSounds, SoundRun};
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);
/// The start eye (start-position.md).
const EYE: Vec3 = [-246.726_3, -23.263_26, -134.433_87];

fn distance(a: Vec3, b: Vec3) -> f32 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt()
}

#[test]
fn first_level_sounds_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();
    let sound = level.take_sound_data().unwrap();

    // S3 section 5: five sources; distances from the start eye (6.1).
    let names: Vec<&str> = sound.statics.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "nature\\new_flies1",
            "ambient\\sparks1",
            "scripts\\magnitofon\\magnitofon_2",
            "script_replics\\radio\\message\\radio_noise_1",
            "ambient\\indoors\\small_fan",
        ]
    );
    let distances: Vec<f32> = sound
        .statics
        .iter()
        .map(|s| distance(EYE, s.position))
        .collect();
    for (d, want) in distances
        .iter()
        .zip([12.3405, 2.0298, 1.8484, 1.7373, 3.0158])
    {
        assert!((d - want).abs() < 1e-3, "{distances:?}");
    }
    let s3 = &sound.statics[3];
    assert_eq!((s3.active_from, s3.active_to), (7_200_000, 10_800_000));

    // S5 section 7: `amb01` by day, `amb20` never.
    let tracks: Vec<&str> = sound.music.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(tracks, ["music\\amb01", "music\\amb20"]);
    let day = &sound.music[0].1;
    assert_eq!(
        (day.start, day.end, day.volume.to_bits()),
        (21_600_000, 79_200_000, 0x3E4C_CCCD)
    );
    assert_eq!((day.pause_min, day.pause_max), (20_000, 40_000));

    // S6 section 7: `level.som`'s 31 one-sided faces, 20 vertices; k = 0.4.
    let mesh = sound.occlusion_mesh.clone().unwrap();
    assert_eq!((mesh.faces.len(), mesh.vertices.len()), (31, 20));
    assert!(mesh.faces.iter().all(|f| f.1 == 0.6));
    assert_eq!(sound.occlusion_scale, 0.4);

    // S4 5.1: the default weather uses 7 sets, at 05:30 `tuman` (A) and
    // `morning` (B) at t = 0.5.
    let mut sets: Vec<&str> = sound.sets.keys().map(String::as_str).collect();
    sets.sort_unstable();
    assert_eq!(
        sets,
        [
            "ambient_env_0",
            "ambient_env_day",
            "ambient_env_evening",
            "ambient_env_morning",
            "ambient_env_night",
            "ambient_env_rain",
            "ambient_env_tuman",
        ]
    );
    let inputs = level.sound_inputs();
    assert_eq!(inputs.tau, 19_800_000);
    let ambient: Vec<_> = inputs
        .sections
        .iter()
        .map(|s| s.env_ambient.clone().unwrap())
        .collect();
    assert_eq!(ambient, ["ambient_env_tuman", "ambient_env_morning"]);
    assert!((inputs.t - 0.5).abs() < 1e-6);
    // Section 5.2: the counts the data gives (the disputed hand count
    // aside): items and distinct names per set.
    for (set, items, distinct) in [
        ("ambient_env_0", 38, 36),
        ("ambient_env_day", 43, 41),
        ("ambient_env_evening", 35, 24),
        ("ambient_env_morning", 30, 27),
        ("ambient_env_night", 35, 24),
        ("ambient_env_rain", 38, 29),
        ("ambient_env_tuman", 33, 33),
    ] {
        let s = &sound.sets[set];
        let names: HashSet<String> = s.sounds.iter().map(|n| n.to_lowercase()).collect();
        assert_eq!((s.sounds.len(), names.len()), (items, distinct), "{set}");
    }
    let tuman = &sound.sets["ambient_env_tuman"];
    assert_eq!(
        tuman.sounds[20].to_lowercase(),
        "ambient\\rnd_outdoor\\rnd_shooting_6"
    );
    let effects: Vec<&str> = tuman.effects.iter().map(|e| e.section.as_str()).collect();
    assert_eq!(effects, ["ae0_effect_4", "ae0_effect_6"]);
    // Section 5.2's dispute, read from the data: in `rain`, `rnd_drone1`,
    // `rnd_drone2` and `rnd_wind_tree` thrice, `rnd_dark`, `rnd_dark3`,
    // `rnd_dark5` twice.
    let rain = &sound.sets["ambient_env_rain"];
    let count = |n: &str| rain.sounds.iter().filter(|s| s.ends_with(n)).count();
    for (n, times) in [
        ("\\rnd_drone1", 3),
        ("\\rnd_drone2", 3),
        ("\\rnd_wind_tree", 3),
        ("\\rnd_dark", 2),
        ("\\rnd_dark3", 2),
        ("\\rnd_dark5", 2),
    ] {
        assert_eq!(count(n), times, "{n}");
    }
    eprintln!("ambient_env_rain repeats: {:?}", {
        let mut seen: Vec<(String, usize)> = Vec::new();
        for s in &rain.sounds {
            match seen.iter_mut().find(|(n, _)| n == s) {
                Some(e) => e.1 += 1,
                None => seen.push((s.clone(), 1)),
            }
        }
        seen.into_iter().filter(|e| e.1 > 1).collect::<Vec<_>>()
    });

    // S4 section 7: the digest of all 11 sets of `env_ambient.ltx`.
    let env = config::load(
        &tree,
        &data
            .aliases
            .path("$game_config$", "weathers\\env_ambient.ltx")
            .unwrap(),
    )
    .unwrap();
    let mut text = String::new();
    let mut lines = 0;
    let mut set_count = 0;
    for section in env.sections().filter(|s| s.contains("sounds")) {
        set_count += 1;
        let get = |k: &str| section.get(k).flatten().unwrap_or("");
        let effects = list_items(section.get("effects").flatten()).join(",");
        text += &format!(
            "{}|{}|{}|{}|{}\n",
            section.name(),
            get("sound_period"),
            get("sound_dist"),
            get("effect_period"),
            effects
        );
        lines += 1;
        for (i, item) in list_items(section.get("sounds").flatten())
            .iter()
            .enumerate()
        {
            text += &format!("{}|{i}|{}\n", section.name(), item.to_lowercase());
            lines += 1;
        }
    }
    assert_eq!((set_count, lines), (11, 313));
    assert_eq!(
        Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        "6e444a3ceb57e2ae0acc31578eefb05ad21c3cd7362247fc90209216edc18846"
    );

    // S8 7.4: the default cycle's rain and thunder.
    let weather = Weather::load(&data.system, "default").unwrap();
    for (section, density, collection, period, duration) in [
        (
            "default_weather_01",
            0.2,
            "thunderbolt_collection_default",
            4.5,
            0.35,
        ),
        (
            "default_weather_12",
            0.85,
            "thunderbolt_collection_default",
            5.0,
            0.25,
        ),
        (
            "default_weather_16",
            0.85,
            "thunderbolt_collection_default",
            5.0,
            0.25,
        ),
        (
            "default_weather_17",
            0.05,
            "thunderbolt_collection_default",
            7.0,
            0.25,
        ),
        ("default_weather_05", 0.0, "", 0.0, 0.0),
    ] {
        let s = &weather
            .sections
            .iter()
            .find(|(_, s)| s.name == section)
            .unwrap()
            .1;
        assert_eq!(
            (
                s.rain_density,
                s.thunderbolt.as_str(),
                s.bolt_period,
                s.bolt_duration
            ),
            (density, collection, period, duration),
            "{section}"
        );
    }
    let others = weather
        .sections
        .iter()
        .filter(|(_, s)| s.thunderbolt.is_empty() && s.rain_density == 0.0)
        .count();
    assert_eq!(others, 20);
    // S8 8.1 (M1): the rain is on, to the whole second, 00:00:18 – 01:59:41,
    // 11:00:05 – 12:59:55, 15:00:05 – 17:58:47; 8.2 (M2): a collection is in
    // effect 00:30:00 – 01:29:59, 11:30:00 – 12:29:59, 15:30:00 – 17:29:59.
    let lerp = |x: f32, y: f32, t: f32| x * (1.0 - t) + y * t;
    let spans = |on: &dyn Fn(f32) -> bool| {
        let mut spans = Vec::new();
        let mut start = None;
        for second in 0..=86_400u32 {
            let is = second < 86_400 && on(second as f32);
            match (is, start) {
                (true, None) => start = Some(second),
                (false, Some(s)) => {
                    spans.push((s, second - 1));
                    start = None;
                }
                _ => {}
            }
        }
        spans
    };
    let hms = |h: u32, m: u32, s: u32| h * 3600 + m * 60 + s;
    let rain_on = spans(&|tau| {
        let (a, b, t) = weather.bracket(tau);
        lerp(a.rain_density, b.rain_density, t) >= 0.001
    });
    assert_eq!(
        rain_on,
        [
            (hms(0, 0, 18), hms(1, 59, 41)),
            (hms(11, 0, 5), hms(12, 59, 55)),
            (hms(15, 0, 5), hms(17, 58, 47)),
        ]
    );
    let thunder_on = spans(&|tau| {
        let (a, b, t) = weather.bracket(tau);
        !(if t >= 0.5 { b } else { a }).thunderbolt.is_empty()
    });
    assert_eq!(
        thunder_on,
        [
            (hms(0, 30, 0), hms(1, 29, 59)),
            (hms(11, 30, 0), hms(12, 29, 59)),
            (hms(15, 30, 0), hms(17, 29, 59)),
        ]
    );
    for (tau, period, duration) in [
        (hms(0, 30, 0), 2.25, 0.175),
        (hms(1, 0, 0), 4.5, 0.35),
        (hms(16, 30, 0), 6.0, 0.25),
        (hms(17, 0, 0), 7.0, 0.25),
    ] {
        let (a, b, t) = weather.bracket(tau as f32);
        let p = lerp(a.bolt_period, b.bolt_period, t);
        let d = lerp(a.bolt_duration, b.bolt_duration, t);
        assert!(
            (p - period).abs() < 1e-4 && (d - duration).abs() < 1e-4,
            "{tau}: {p} {d}"
        );
    }
    assert_eq!(
        sound.collections["thunderbolt_collection_default"],
        (0..4)
            .map(|i| Some(format!("nature\\thunder-{i}")))
            .collect::<Vec<_>>()
    );
    let common = sound.bolt_common.clone().unwrap();
    assert_eq!(common.altitude, (10.0, 20.0));
    assert_eq!(
        (
            common.delta_longitude,
            common.min_dist_factor,
            common.second_probability
        ),
        (17.0, 0.94, 0.5)
    );

    // S6 section 7 (M2 – M4), jitter (0, 0, 0.2).
    let occlusion = Occlusion {
        level: Some(level.shared_collision()),
        sound: Some(mesh),
        scale: 0.4,
    };
    let sparks1 = sound.statics[1].position;
    let along = |from: Vec3, to: Vec3| {
        let x = [to[0], to[1], to[2] + 0.2];
        let v = [x[0] - from[0], x[1] - from[1], x[2] - from[2]];
        let d = distance(from, x);
        occlusion.target_along(from, normalize(v).unwrap(), d)
    };
    let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
    assert!(close(along(EYE, sparks1), 0.4));
    assert!(close(along([-246.7, 0.0, -134.4], sparks1), 0.24));
    assert!(close(
        along([-246.7, 10.0, -134.4], [-246.7, 0.0, -200.0]),
        1.0
    ));
    assert!(close(
        along([-200.0, -40.0, -110.0], [-260.0, -40.0, -110.0]),
        0.6
    ));
    assert!(close(
        along([-260.0, -40.0, -110.0], [-200.0, -40.0, -110.0]),
        1.0
    ));
    let mut zero = ZeroFirst::default();
    assert!(close(
        occlusion.hearer_factor([-200.0, -40.0, -110.0], [-260.0, -40.0, -110.0], zero.get()),
        0.6
    ));

    // A New Game's start: the sounds created, the rules' first frame
    // (after the loading image), the sound update with occlusion.
    let mut system = SoundSystem::new(24);
    system.set_volume(volya_engine::core::audio::Category::Effects, 1.0);
    let t = Instant::now();
    let mut sounds = LevelSounds::create(&mut system, &tree, &data.aliases, &sound).unwrap();
    eprintln!(
        "created the level's {} sounds in {:.1?}",
        system.files().len(),
        t.elapsed()
    );
    system.occlusion_mut().level = Some(level.shared_collision());
    let v = level.player().view();
    system.set_listener(Listener {
        position: v.eye,
        direction: v.direction,
        up: v.up,
    });
    let mut run = SoundRun::default();
    let frame_ms = 16;
    run.advance(frame_ms);
    run.frame(&mut system, &mut sounds, &level.sound_inputs(), 0.016);
    system.update(16, &mut NoDevice);
    let playing: Vec<String> = system.emitters().iter().map(|e| e.name.clone()).collect();
    eprintln!("first frame: {playing:?}");
    // S3 5: sources 0, 1, 2 and 4 start; the radio does not.
    for name in [
        "nature\\new_flies1",
        "ambient\\sparks1",
        "scripts\\magnitofon\\magnitofon_2",
        "ambient\\indoors\\small_fan",
    ] {
        assert!(playing.iter().any(|p| p == name), "{name}");
    }
    assert!(!playing.iter().any(|p| p.contains("radio_noise")));
    // S5 7: no music at 05:30.
    assert!(sounds.music.current.is_none());
    // S4 4.2: the first frame starts a random sound of `tuman` or `morning`.
    let random = system.emitters().len() - 4;
    assert!(random >= 1, "{playing:?}");
    // sparks1 heard from the start eye through 2 level triangles (S6 M2).
    let sparks = system
        .emitters()
        .into_iter()
        .find(|e| e.name == "ambient\\sparks1")
        .unwrap();
    eprintln!("sparks1 occlusion at the start: {}", sparks.occlusion);
    // The radio's window: 02:00 of the next day (S3 5).
    assert!(!sounds.statics.sources[3].active(level.sound_inputs().tau));

    // The update's cost with occlusion: 600 frames of 16 ms with the
    // level's rules and the sound update, the listener walking 1 m/s.
    let (mut rules, mut update, mut worst) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let mut most = 0;
    for frame in 1..=600u64 {
        let mut eye = v.eye;
        eye[2] += frame as f32 * 0.016;
        system.set_listener(Listener {
            position: eye,
            direction: v.direction,
            up: v.up,
        });
        let t = Instant::now();
        run.advance(frame_ms);
        run.frame(&mut system, &mut sounds, &level.sound_inputs(), 0.016);
        rules += t.elapsed();
        let t = Instant::now();
        system.update(16 + frame * frame_ms, &mut NoDevice);
        let u = t.elapsed();
        update += u;
        worst = worst.max(u);
        most = most.max(system.emitters().len());
    }
    eprintln!(
        "per frame: rules {:.1?}, sound update with occlusion {:.1?} (worst {worst:.1?}), \
         up to {most} emitters",
        rules / 600,
        update / 600
    );
}

/// Draws 0 first (the jitter's θ = 0), then the shared generator.
#[derive(Default)]
struct ZeroFirst(SharedRandom);

impl ZeroFirst {
    fn get(&mut self) -> &mut SharedRandom {
        // SharedRandom seeded so that its first draw is 0: state s with
        // (s · 214,013 + 2,531,011) >> 16 & 32,767 = 0.
        let seed = (0..u32::MAX)
            .find(|&s| (s.wrapping_mul(214_013).wrapping_add(2_531_011) >> 16) & 32_767 == 0)
            .unwrap();
        self.0 = SharedRandom::new(seed);
        &mut self.0
    }
}
