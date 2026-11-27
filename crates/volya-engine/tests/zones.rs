// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The first level's zones and weather effects from the Russian edition's
//! data (BYOA): the zones online at a New Game's start with their idle
//! effects and sounds (docs/behavior/zone-idle-effects.md section 8,
//! sound-anomalies.md section 9.1), the galantines switching, the start
//! weather's ambient effects (weather-ambient-effects.md 2.3, 3), the
//! hits of a fire, a radiation zone and a fall on the player
//! (environment-hits.md 3, 6), and the
//! frame cost of the zones and their effects (meaningful in release
//! builds). A campfire's effect drawn is the render-level suite's
//! (`selftest::zones`). Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use volya_engine::core::camera::Angles;
use volya_engine::core::input::Input;
use volya_engine::core::pause::GlobalClock;
use volya_engine::core::sound::system::{Listener, NoDevice, SoundSystem};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::core::zones::{Frame, NoColourAnimations};
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::level_sound::{LevelSounds, SoundRun};
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn root() -> Option<StdVfs> {
    let archives = gog_archives::archives()?;
    Some(StdVfs::new(archives[0].file.parent().unwrap()))
}

#[test]
fn first_level_zones_and_weather_effects_from_game_data() {
    let Some(root) = root() else {
        return;
    };
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();
    let report = level.zones_report();

    // P9 8, S15 1: every zone record is online and idle at the start.
    let mut sections: BTreeMap<&str, usize> = BTreeMap::new();
    for z in &report.zones {
        *sections.entry(z.section.as_str()).or_default() += 1;
        assert_eq!(z.state, "idle", "{}", z.name);
        assert!(!z.light, "{}: no idle light on the first level", z.name);
    }
    assert_eq!(
        sections.into_iter().collect::<Vec<_>>(),
        [
            ("zone_flame_small", 11),
            ("zone_mincer_weak_noart", 4),
            ("zone_mine_field", 49),
            ("zone_mosquito_bald_weak", 85),
            ("zone_mosquito_bald_weak_noart", 2),
            ("zone_radioactive_average", 1),
            ("zone_radioactive_killing", 17),
            ("zone_witches_galantine", 9),
        ]
    );
    // Their idle effects started (P9 8): 160, the campfires among them.
    let mut effects: BTreeMap<&str, usize> = BTreeMap::new();
    for z in &report.zones {
        if let Some(e) = &z.effect {
            assert!(z.playing, "{}: {e} not playing", z.name);
            *effects.entry(e.as_str()).or_default() += 1;
        }
    }
    assert_eq!(
        effects.into_iter().collect::<Vec<_>>(),
        [
            ("anomaly2\\bold_idle", 87),
            ("anomaly2\\electra_idle", 9),
            ("anomaly2\\mincer_idle", 4),
            ("explosions\\campfire_05", 11),
            ("static\\mine", 49),
        ]
    );
    // Their idle sounds (P9 8, S15 9.1): 111, none for the mine fields.
    let mut sounds: BTreeMap<&str, usize> = BTreeMap::new();
    for z in report.zones.iter().filter(|z| z.sound_on) {
        *sounds.entry(z.sound.as_deref().unwrap()).or_default() += 1;
    }
    assert_eq!(
        sounds.into_iter().collect::<Vec<_>>(),
        [
            ("ambient\\fire2", 11),
            ("anomaly\\electra_idle1", 9),
            ("anomaly\\gravi_idle01", 4),
            ("anomaly\\gravi_rumble1", 87),
        ]
    );
    // The library's shaders for them are resolved with the level's.
    let world = level.world().clone();
    let campfire = level
        .effects()
        .unwrap()
        .effects()
        .find(|p| p.name == "explosions\\campfire_05")
        .unwrap();
    for e in campfire.instance.effects() {
        let sprite = level.effects().unwrap().library().effect_at(e.record());
        if sprite.sprite.is_some() {
            assert!(
                world.particle_shaders[e.record()].is_some(),
                "{}",
                sprite.name
            );
        }
    }
    eprintln!(
        "the level's effect objects can hold {} particles",
        world.particle_capacity
    );

    // The level's sounds with the zones' (S15 2.1), played as the zones
    // stand; the first frames: the 5 galantines off at creation stop (P9 8).
    let mut sound = level.take_sound_data().unwrap();
    sound.pairs = None;
    let mut system = SoundSystem::new(24);
    system.set_volume(volya_engine::core::audio::Category::Effects, 1.0);
    let t = Instant::now();
    let mut level_sounds = LevelSounds::create(&mut system, &tree, &data.aliases, &sound).unwrap();
    eprintln!("created the level's sounds in {:.1?}", t.elapsed());
    level_sounds.zone_events(&mut system, &level.zone_sounds_playing());
    let idle = |system: &SoundSystem| {
        let mut n: BTreeMap<String, usize> = BTreeMap::new();
        for e in system.emitters() {
            if e.looped && (e.name.starts_with("anomaly\\") || e.name == "ambient\\fire2") {
                *n.entry(e.name).or_default() += 1;
            }
        }
        n
    };
    assert_eq!(idle(&system).values().sum::<usize>(), 111);
    let mut run = SoundRun::default();
    let mut clock = GlobalClock::default();
    let mut weather = Vec::new();
    // The start is in a bunker, indoors (S4 4.3): the weather's effects
    // start outdoors, 2 m above the first mine field.
    let field = (level.zones_report().zones.into_iter())
        .find(|z| z.section == "zone_mine_field")
        .unwrap()
        .position;
    let outdoors = [field[0], field[1] + 2.0, field[2]];
    let mut eye: Option<[f32; 3]> = None;
    let mut frame = |level: &mut LevelScreen,
                     system: &mut SoundSystem,
                     level_sounds: &mut LevelSounds,
                     run: &mut SoundRun,
                     eye: Option<[f32; 3]>,
                     ms: u64| {
        if let Some(eye) = eye {
            level.set_camera(eye, Angles::default());
        }
        level.update(ms, &Input::new());
        level_sounds.zone_events(system, level.zone_sound_events());
        let v = level.player().view();
        system.set_listener(Listener {
            position: v.eye,
            direction: v.direction,
            up: v.up,
        });
        let now = clock.frame(ms, false);
        let (inputs, mut particles) = level.sound_frame_parts();
        run.frame(system, level_sounds, &inputs, now, false, &mut particles)
            .unwrap();
        system.update(ms, &mut NoDevice);
    };
    frame(
        &mut level,
        &mut system,
        &mut level_sounds,
        &mut run,
        eye,
        16,
    );
    eye = Some(outdoors);
    let report = level.zones_report();
    let off: Vec<&str> = (report.zones.iter())
        .filter(|z| z.state == "disabled")
        .map(|z| z.name.as_str())
        .collect();
    assert_eq!(
        off,
        [
            "esc_zone_witches_galantine_0000",
            "esc_zone_witches_galantine_0001",
            "esc_zone_witches_galantine_0004",
            "esc_zone_witches_galantine_0005",
            "esc_zone_witches_galantine_0006",
        ]
    );
    assert!((report.zones.iter()).all(|z| z.state != "disabled" || !z.sound_on && !z.playing));
    assert_eq!(idle(&system)["anomaly\\electra_idle1"], 4);
    assert_eq!(idle(&system).values().sum::<usize>(), 106);

    // P11 2.3, 3: the start weather's first effect (S4's `tuman` or
    // `morning`): one of `nature\fog_foggy_00`, `nature\fog_stormy_01`,
    // placed at the eye once the view counts as outdoors (S4 4.3), playing;
    // run 31 s: it stops and is removed.
    let (mut started, mut ended, mut removed, mut most) = (None, None, None, 0);
    for k in 2..=1940u64 {
        let ms = 16 * k;
        frame(
            &mut level,
            &mut system,
            &mut level_sounds,
            &mut run,
            eye,
            ms,
        );
        let view = level.player().view().eye;
        match level.zones_report().weather {
            Some(w) if started.is_none() => {
                assert!(
                    ["nature\\fog_foggy_00", "nature\\fog_stormy_01"].contains(&w.name.as_str()),
                    "{}",
                    w.name
                );
                assert!(w.playing && !w.stopping);
                let p = level.weather_effect().unwrap();
                let e = p.instance.effects().next().unwrap();
                assert_eq!(e.position(), view, "placed at the eye, offset 0");
                weather.push(w.name.clone());
                started = Some(ms);
            }
            Some(w) => {
                most = most.max(w.particles);
                if w.stopping && ended.is_none() {
                    ended = Some(ms);
                }
            }
            None if started.is_some() && removed.is_none() => removed = Some(ms),
            None => {}
        }
    }
    eprintln!(
        "the ambient effect {weather:?}: started at {started:?} ms, up to {most} particles, \
         stopped at {ended:?} ms, removed at {removed:?} ms"
    );
    assert!(most > 0);
    let (started, ended, removed) = (started.unwrap(), ended.unwrap(), removed.unwrap());
    // Ended at its life time (`ae0_effect_4` 15 s, `ae0_effect_6` 7 s, S4
    // 5.3); removed once its particles ran out (5 s at most, P11 3.4).
    assert!(
        [15_000, 7_000]
            .iter()
            .any(|l| ended.abs_diff(started + l) <= 32),
        "{started} {ended}"
    );
    assert!(
        removed > ended && removed <= ended + 6_000,
        "{ended} {removed}"
    );
    // The galantines on the level's clock at 31 s, by P9 8's table.
    let on: Vec<String> = (level.zones_report().zones.into_iter())
        .filter(|z| z.section == "zone_witches_galantine" && z.state == "idle")
        .map(|z| z.name)
        .collect();
    assert_eq!(
        on,
        [
            "esc_zone_witches_galantine",
            "esc_zone_witches_galantine_0000",
            "esc_zone_witches_galantine_0003",
            "esc_zone_witches_galantine_0007",
        ]
    );
}

/// The first level loaded with the player's feet placed at `feet`, then
/// run for `ms` in 16 ms frames; the level is returned.
fn placed(
    tree: &dyn Vfs,
    data: &GameData,
    feet: impl Fn(&LevelScreen) -> [f32; 3],
    ms: u64,
) -> LevelScreen {
    let mut level = LevelScreen::load(tree, data, 0).unwrap();
    let s = level.player().state();
    let h = level.player().view().eye[1] - s.position[1];
    let p = feet(&level);
    level.set_camera([p[0], p[1] + h, p[2]], Angles::default());
    let mut now = 0;
    while now < ms {
        now += 16;
        level.update(now, &Input::new());
    }
    level
}

/// The position of the first zone of section `section`, and its maximum
/// power.
fn zone_at(level: &LevelScreen, section: &str) -> ([f32; 3], f32) {
    let z = (level.zones().zones().iter())
        .find(|z| z.section.name == section && z.maximum_power > 0.0)
        .unwrap();
    (z.position(), z.maximum_power)
}

#[test]
fn environment_hits_from_game_data() {
    let Some(root) = root() else {
        return;
    };
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    // environment-hits.md 9: the first level's fires have M 15; the New
    // Game state stands at none.
    let level = placed(&tree, &data, |l| l.player().state().position, 1_000);
    let c = level.character().unwrap();
    let (health, radiation) = (c.conditions.health, c.conditions.radiation);
    assert!(level.zones().zones().iter().all(|z| z.player.is_none()));
    // 6.1: in a fire, burnt to death within a blowout (P = 15).
    let level = placed(&tree, &data, |l| zone_at(l, "zone_flame_small").0, 500);
    assert_eq!(zone_at(&level, "zone_flame_small").1, 15.0);
    assert!(!level.character().unwrap().conditions.alive());
    // 6.3: in a killing radiation zone (M 0.05), radiation grows.
    let level = placed(
        &tree,
        &data,
        |l| zone_at(l, "zone_radioactive_killing").0,
        2_000,
    );
    let c = level.character().unwrap();
    assert!(
        c.conditions.radiation > radiation,
        "{}",
        c.conditions.radiation
    );
    // 3.2: a fall from 40 m above the start hits with `strike`.
    let level = placed(
        &tree,
        &data,
        |l| {
            let p = l.player().state().position;
            [p[0], p[1] + 40.0, p[2]]
        },
        4_000,
    );
    let c = level.character().unwrap();
    assert!(
        c.conditions.health < health - 0.1,
        "{}",
        c.conditions.health
    );
}

/// The zones' and their effects' frame cost with the camera at the start:
/// the zones' updates and the effect objects' scheduler pass, 600 frames.
#[test]
fn zones_frame_cost_from_game_data() {
    let Some(root) = root() else {
        return;
    };
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let level = LevelScreen::load(&tree, &data, 0).unwrap();
    let eye = level.player().view().eye;
    let mut effects = level.effects().unwrap().clone();
    let mut zones = level.zones().clone();
    let (mut total, mut worst) = (Duration::ZERO, Duration::ZERO);
    let mut times = Vec::with_capacity(600);
    for k in 1..=600u32 {
        let t = Instant::now();
        effects.frame(16, eye);
        let f = Frame {
            clock: effects.clock(),
            elapsed: 16,
            camera: eye,
            colours: &NoColourAnimations,
            player: None,
        };
        zones.frame(&f, &mut effects).unwrap();
        let e = t.elapsed();
        total += e;
        worst = worst.max(e);
        times.push(e);
        let _ = k;
    }
    times.sort();
    eprintln!(
        "zones and effect objects per frame: mean {:.1?}, 99th percentile {:.1?}, worst {worst:.1?}; {} live particles",
        total / 600,
        times[594],
        effects.particle_count()
    );
}
