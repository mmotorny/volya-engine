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
use volya_engine::audio::SoundHost;
use volya_engine::core::audio::{Category, Mixer, VoiceId};
use volya_engine::core::collision::geometry::normalize;
use volya_engine::core::config;
use volya_engine::core::pause::GlobalClock;
use volya_engine::core::input::Input;
use volya_engine::core::object_light::SkyVisibility;
use volya_engine::core::sound::ambient::{AmbientParticles, NoParticles, list_items};
use volya_engine::core::sound::occlusion::Occlusion;
use volya_engine::core::sound::random::SharedRandom;
use volya_engine::core::sound::system::{
    Device, DeviceListener, DeviceSource, Listener, NoDevice, ROLLOFF, SoundSystem, Vec3,
    distance_factor, distance_gain, heard as heard_gain,
};
use volya_engine::core::sound::{Fatal, Sound};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::core::weather::Weather;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::level_sound::{LevelSoundData, LevelSounds, SoundRun};
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
    let mut clock = GlobalClock::default();
    let frame_ms = 16;
    run.frame_time(0.016, false);
    run.frame(
        &mut system,
        &mut sounds,
        &level.sound_inputs(),
        clock.frame(frame_ms, false),
        false,
        &mut NoParticles::default(),
    )
    .unwrap();
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
        run.frame_time(0.016, false);
        run.frame(
            &mut system,
            &mut sounds,
            &level.sound_inputs(),
            clock.frame((frame + 1) * frame_ms, false),
            false,
            &mut NoParticles::default(),
        )
        .unwrap();
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

    // The tape recorder (S3 5, source 2) at a New Game's start, on a mixer:
    // 30 s of 16 ms frames from the start eye, a block mixed every 3 frames
    // as the AudioWorklet asks. Its voice keeps playing the file on (S2 4.2:
    // looped data repeats without a gap): one mixer voice, advancing by
    // exactly its step per output frame, its gain ramp small.
    sounds.discard(&mut system);
    let mut host = SoundHost::new(16);
    host.system.set_volume(Category::Effects, 1.0);
    let mut sounds = LevelSounds::create(&mut host.system, &tree, &data.aliases, &sound).unwrap();
    host.system.occlusion_mut().level = Some(level.shared_collision());
    host.set_listener(Listener {
        position: v.eye,
        direction: v.direction,
        up: v.up,
    });
    let mut mixer = Mixer::new(48_000);
    let (mut run, mut clock) = (SoundRun::default(), GlobalClock::default());
    let tape = "scripts\\magnitofon\\magnitofon_2";
    let mut last: Option<(VoiceId, f64)> = None;
    let (mut switches, mut worst_jump) = (0, 0f32);
    for frame in 0..30_000 / frame_ms {
        let time = frame * frame_ms;
        run.frame_time(0.016, false);
        run.frame(
            &mut host.system,
            &mut sounds,
            &level.sound_inputs(),
            clock.frame(time, false),
            false,
            &mut NoParticles::default(),
        )
        .unwrap();
        host.update(time as f64, Some(&mut mixer), Some(&tree));
        if frame % 3 != 0 {
            continue;
        }
        let voice = host
            .system
            .emitters()
            .into_iter()
            .find(|e| e.name == tape)
            .and_then(|e| e.voice)
            .and_then(|v| host.mixer_voice(v));
        let before = voice.and_then(|id| mixer.probe(id));
        mixer.mix(2400);
        let (Some(id), Some(before)) = (voice, before) else {
            assert!(last.is_none(), "the tape recorder stopped at {time} ms");
            continue;
        };
        let after = mixer.probe(id).unwrap();
        match last {
            Some((prev, advanced)) if prev == id => {
                assert_eq!(before.advanced, advanced, "skipped at {time} ms")
            }
            Some(_) => switches += 1,
            None => {}
        }
        assert!((after.advanced - before.advanced - 2400.0 * after.step).abs() < 1e-6);
        // After its first block, which ramps from gain 0 (MixerDevice).
        if last.is_some() {
            worst_jump = worst_jump.max((after.gains.0 - before.gains.0).abs());
        }
        last = Some((id, after.advanced));
    }
    eprintln!(
        "tape recorder over 30 s: {switches} voice switches, worst gain change per block {worst_jump:.4}"
    );
    assert!(last.is_some(), "the tape recorder never played");
    assert_eq!(switches, 0, "the tape recorder's voice changed");
    assert!(worst_jump < 0.05, "gain jumps by {worst_jump}");
    host.system.stop_all();
    sounds.discard(&mut host.system);

    sky_visibility_and_heard_levels(&tree, &data, &sound, &mut level);
}

/// S4 4.3 (H2 – H6) and 6.1 (L1 – L9) on `level` at a New Game's start:
/// the player's sky visibility in the bunker and outdoors above it, and how
/// loud the random sounds are heard in each.
fn sky_visibility_and_heard_levels(
    tree: &dyn Vfs,
    data: &GameData,
    sound: &LevelSoundData,
    level: &mut LevelScreen,
) {
    // 4.3: the rays start at the model's header sphere centre raised by
    // 0.3 R, 1.35098 m above the feet (H2), 0.0228 m ahead of them.
    let feet = level.player().state().position;
    let origin = level.sky_ray_origin();
    assert!((origin[1] - feet[1] - 1.350_98).abs() < 1e-4, "{origin:?}");
    let ahead = ((origin[0] - feet[0]).powi(2) + (origin[2] - feet[2]).powi(2)).sqrt();
    assert!((ahead - 0.022_80).abs() < 1e-5, "{ahead}");
    // H3, H4: no ray is free in the bunker, so h = 0 from the first update
    // on (made with the level) and the view is not outdoors.
    assert_eq!(level.sky_visibility(), (0.0, 0.0));

    // L5, L7, L8: 30 s at the start in the bunker; the level runs, its
    // rules and the sound update every 16 ms, as the app runs them.
    let mut now = 0;
    let bunker = heard_levels(tree, data, sound, level, &mut now, 30_000, None);
    assert_eq!(bunker.effects, 0, "an ambient effect started in the bunker");
    assert_eq!(level.sky_visibility(), (0.0, 0.0));
    assert!(bunker.sounds >= 3, "{} random sounds", bunker.sounds);
    // Every placement is occluded: o = 0.4, so V = 0.4 from the start on,
    // heard at −20.4 … −41.2 dB, and the priority P = V · a.
    assert!(
        bunker.occlusion.iter().all(|&o| (o - 0.4).abs() < 1e-6),
        "{:?}",
        bunker.occlusion
    );
    assert!(bunker.db.0 >= -41.25 && bunker.db.1 <= -20.35, "{:?}", bunker.db);

    // H5: outdoors on the ground above the start (y −7.659), 13 of the 26
    // rays are free, 4 of the first 5: h = 0.8 at a first update.
    let ground = [EYE[0], -7.659, EYE[2]];
    let h = level.player().config.eye_height(0);
    let eye = [ground[0], ground[1] + h, ground[2]];
    level.set_camera(eye, level.player().angles());
    let mut fresh = SkyVisibility::default();
    for frame in 0..6 {
        fresh.update(frame, level.collision(), level.sky_ray_origin(), 1.0 / 60.0);
        if frame == 0 {
            assert!((fresh.value() - 0.8).abs() < 1e-6, "{}", fresh.value());
        }
    }
    assert_eq!(fresh.target(), 0.5);
    // H6: coming out of the bunker with h = 0, at 60 frames per second h
    // reaches 0.05 after 8 – 10 frames (the disputed range), so the view
    // turns outdoors. The player is held there: the body does not stand on
    // that ground (it falls through to a floor below).
    let angles = level.player().angles();
    let mut frames = 0;
    while level.read_sky_visibility() < 0.05 {
        frames += 1;
        now += if frames % 3 == 0 { 16 } else { 17 };
        level.update(now, &Input::new());
        level.set_camera(eye, angles);
        assert!(frames < 60, "never outdoors");
    }
    assert!((8..=10).contains(&frames), "outdoors after {frames} frames");

    // L6, L7: outdoors, unoccluded −12.5 … −33.2 dB, behind terrain as in
    // the bunker; an ambient effect starts.
    let outdoors = heard_levels(tree, data, sound, level, &mut now, 30_000, Some(eye));
    assert!(outdoors.effects >= 1, "no ambient effect outdoors");
    assert!(outdoors.sounds >= 3, "{} random sounds", outdoors.sounds);
    assert!(
        outdoors.occlusion.iter().all(|&o| o > 0.0 && o <= 1.0),
        "{:?}",
        outdoors.occlusion
    );
    assert!(
        outdoors.db.0 >= -41.25 && outdoors.db.1 <= -12.45,
        "{:?}",
        outdoors.db
    );
    let (h, target) = level.sky_visibility();
    assert!(h >= 0.05 && (target - 0.5).abs() < 1e-6, "{h} {target} {:?} {frames}", level.player().state());
}

/// What [`heard_levels`] found of the random sounds.
struct Heard {
    /// Random sounds started, effects started.
    sounds: usize,
    effects: usize,
    /// Each random sound's occlusion as it started.
    occlusion: Vec<f32>,
    /// The quietest and loudest heard gain of a playing random sound, dB.
    db: (f32, f32),
}

/// `ms` of the level from engine clock `now` in 16 ms frames, with fresh
/// level sounds and rules at the default settings (S4 6.1: effects volume
/// 1, 24 voices), the player held with its eye at `hold` if given: each
/// frame the level's update, its sound rules, and the
/// sound update on a device that keeps what it is sent. Checks that every
/// playing random sound (S4 4.5) is heard at the gain chain's level: the
/// output volume is its occlusion (V = b · v · c · φ · o with b, v, c, φ =
/// 1), the device's gain V · g with S2's distance gain g, and its priority
/// V · a.
fn heard_levels(
    tree: &dyn Vfs,
    data: &GameData,
    sound: &LevelSoundData,
    level: &mut LevelScreen,
    now: &mut u64,
    ms: u64,
    hold: Option<Vec3>,
) -> Heard {
    let mut system = SoundSystem::new(24);
    system.set_volume(Category::Effects, 1.0);
    let mut sounds = LevelSounds::create(&mut system, tree, &data.aliases, sound).unwrap();
    system.occlusion_mut().level = Some(level.shared_collision());
    let mut device = Recording::default();
    let mut particles = Counting::default();
    let (mut run, mut clock) = (SoundRun::default(), GlobalClock::default());
    let mut heard = Heard {
        sounds: 0,
        effects: 0,
        occlusion: Vec::new(),
        db: (f32::INFINITY, f32::NEG_INFINITY),
    };
    let mut seen = HashSet::new();
    let start = *now;
    let angles = level.player().angles();
    while *now < start + ms {
        *now += 16;
        level.update(*now, &Input::new());
        if let Some(eye) = hold {
            level.set_camera(eye, angles);
        }
        let v = level.player().view();
        let listener = Listener {
            position: v.eye,
            direction: v.direction,
            up: v.up,
        };
        system.set_listener(listener);
        run.frame_time(0.016, false);
        run.frame(
            &mut system,
            &mut sounds,
            &level.sound_inputs(),
            clock.frame(*now - start, false),
            false,
            &mut particles,
        )
        .unwrap();
        system.update(*now - start, &mut device);
        for e in system.emitters() {
            if !e.name.starts_with("ambient\\rnd_outdoor\\") || e.name.contains("rnd_wind_") {
                continue;
            }
            if seen.insert(e.id) {
                heard.sounds += 1;
                heard.occlusion.push(e.occlusion);
            }
            let Some(voice) = e.voice else { continue };
            let source = device.sources[voice].unwrap();
            let (gain, _) = heard_gain(&source, &device.listener.unwrap());
            let delta = distance(v.eye, e.position);
            let (m, max) = e.range;
            assert_eq!(max, 75.0, "{}", e.name);
            let g = distance_gain(m, max, ROLLOFF, delta);
            assert_eq!(e.fade, 1.0, "{} fades", e.name);
            assert!((e.out - e.occlusion).abs() < 1e-3, "{}: V {} o {}", e.name, e.out, e.occlusion);
            assert!((gain - e.out * g).abs() < 1e-6, "{}: {gain}", e.name);
            assert!(
                (e.priority - e.out * distance_factor(m, delta)).abs() < 1e-6,
                "{}: P {}",
                e.name,
                e.priority
            );
            let db = 20.0 * gain.log10();
            heard.db = (heard.db.0.min(db), heard.db.1.max(db));
        }
    }
    heard.effects = particles.started;
    system.stop_all();
    sounds.discard(&mut system);
    heard
}

/// A device that keeps the listener and each voice's source it was sent.
#[derive(Default)]
struct Recording {
    listener: Option<DeviceListener>,
    sources: Vec<Option<DeviceSource>>,
}

impl Device for Recording {
    fn start(&mut self, _: usize, _: &Sound, _: bool, _: u64) {}
    fn stop(&mut self, voice: usize) {
        if let Some(s) = self.sources.get_mut(voice) {
            *s = None;
        }
    }
    fn set_listener(&mut self, listener: &DeviceListener) {
        self.listener = Some(*listener);
    }
    fn set_source(&mut self, voice: usize, source: &DeviceSource) {
        if self.sources.len() <= voice {
            self.sources.resize(voice + 1, None);
        }
        self.sources[voice] = Some(*source);
    }
}

/// Effect objects counted as they start, gone at once when stopped.
#[derive(Default)]
struct Counting {
    started: usize,
    playing: bool,
}

impl AmbientParticles for Counting {
    fn start(&mut self, _: &str, _: Vec3) -> Result<(), Fatal> {
        self.started += 1;
        self.playing = true;
        Ok(())
    }
    fn stop(&mut self) {
        self.playing = false;
    }
    fn is_playing(&self) -> bool {
        self.playing
    }
    fn remove(&mut self) {}
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
