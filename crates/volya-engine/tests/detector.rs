// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The detector's sounds on the first level from the Russian edition's
//! data (BYOA; docs/behavior/sound-detector.md, "S21"): the starting
//! inventory's `detector_simple` and its table (7.1), the zones it ticks
//! for (7.2), none in range at a New Game's start, and the player walking
//! to an anomaly: the zone joining the zone list within R + ρ, every tick
//! at the frame sections 5.2 and 5.3 give for the player's positions, the
//! ticks every 48 ms at the zone (8.2), each played 2D at the listener
//! (6); then standing by a radioactive zone, a tick every 5 frames (8.2).
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeMap;

use volya_engine::core::audio::Category;
use volya_engine::core::camera::Angles;
use volya_engine::core::character::inventory::Place;
use volya_engine::core::detector::{self, DetectorSection, SoundCommand};
use volya_engine::core::input::Input;
use volya_engine::core::sound::system::{NoDevice, Phase, SoundSystem};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::core::zones::Zone;
use volya_engine::data::GameData;
use volya_engine::detector::{DetectorCommand, DetectorReport, DetectorSound};
use volya_engine::level::{self, LevelScreen};
use volya_engine::level_sound::LevelSounds;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);
/// The frame time (ms).
const STEP: u64 = 16;

fn root() -> Option<StdVfs> {
    let archives = gog_archives::archives()?;
    Some(StdVfs::new(archives[0].file.parent().unwrap()))
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt()
}

/// The level, its sounds and the sound system, run frame by frame as the
/// app runs them, with an independent model of S21 5 checked each frame.
struct Run<'a> {
    tree: &'a dyn Vfs,
    data: &'a GameData,
    level: LevelScreen,
    system: SoundSystem,
    sounds: LevelSounds,
    section: DetectorSection,
    now: u64,
    /// The model's zone list: zone object id → counter.
    model: BTreeMap<u16, u32>,
    /// The ticks seen: (frame, entry, zone object id).
    ticks: Vec<(u64, usize, u16)>,
    /// The zone ids seen joining the zone list, with the player's distance
    /// to the zone's shape sphere's centre at that frame.
    joined: Vec<(u16, f32)>,
}

impl Run<'_> {
    fn report(&self) -> DetectorReport {
        self.level.detectors().report(self.level.zones()).remove(0)
    }

    /// One frame with the player's feet held at `feet`, if given.
    fn frame(&mut self, feet: Option<[f32; 3]>) {
        if let Some(f) = feet {
            let h = self.level.player().config.eye_height(0);
            self.level
                .set_camera([f[0], f[1] + h, f[2]], Angles::default());
        }
        self.now += STEP;
        self.level.update(self.now, &Input::new());
        let frame = self.now / STEP;
        let holder = self.level.player().state().position;
        let on = self.report().on;
        // The model (S21 5.2, 5.3): while on, the zone list as the previous
        // frame left it, each zone's counter against T at the holder's
        // position.
        let mut expected = Vec::new();
        let zones = self.level.zones().zones();
        for (&id, c) in self.model.iter_mut().filter(|_| on) {
            let z = zones.iter().find(|z| z.id == id).unwrap();
            let Some(entry) = self.section.entry(&z.class_id) else {
                continue;
            };
            if !z.section.visible_by_detector {
                continue;
            }
            let (_, r) = z.placed_sphere();
            let f = detector::frequency(
                &self.section.entries[entry],
                self.section.radius,
                holder,
                z.position(),
                r,
            );
            if f64::from(*c) > f64::from(detector::period(f)) {
                *c = 0;
                expected.push((entry, id));
            } else {
                *c += STEP as u32;
            }
        }
        // The detector's sound commands, played as the app plays them.
        let mut seen = Vec::new();
        for sound in self.level.take_detector_sounds() {
            if let DetectorCommand::Sound(SoundCommand::Tick { entry, zone }) = sound.command {
                let id = self.level.zones().zones()[zone].id;
                seen.push((entry, id));
                self.ticks.push((frame, entry, id));
            }
            let section = self.level.detectors().section(sound.detector);
            (self.sounds)
                .detector(
                    &mut self.system,
                    self.tree,
                    &self.data.aliases,
                    sound,
                    section,
                )
                .unwrap();
            self.check_tick(sound);
        }
        self.system.update(self.now, &mut NoDevice);
        // The order of the zone list's updates is unestablished (S21 5.1).
        seen.sort();
        expected.sort();
        assert_eq!(seen, expected, "frame {frame}");
        // The range test's changes, then the counters as the model has them.
        let listed: BTreeMap<u16, u32> = self.report().zones.into_iter().collect();
        self.model.retain(|id, _| listed.contains_key(id));
        let zones = self.level.zones().zones();
        for (&id, &c) in &listed {
            if !self.model.contains_key(&id) {
                assert_eq!(c, 0, "zone {id} joined with a count");
                let z = zones.iter().find(|z| z.id == id).unwrap();
                self.joined
                    .push((id, distance(holder, z.placed_sphere().0)));
                self.model.insert(id, 0);
            }
        }
        assert_eq!(self.model, listed, "frame {frame}");
    }

    /// After a tick: exactly one sound of its entry's list has an emitter,
    /// 2D at (0, 0, 0), not looped, at the sound's volume (S21 6).
    fn check_tick(&self, sound: DetectorSound) {
        let DetectorCommand::Sound(SoundCommand::Tick { entry, .. }) = sound.command else {
            return;
        };
        let list = &self.section.entries[entry].sounds;
        let ids: Vec<_> = self.sounds.detectors[&sound.detector].ids().collect();
        let offset: usize = (self.section.entries[..entry].iter())
            .map(|e| e.sounds.len())
            .sum();
        let playing = (ids[offset..offset + list.len()].iter())
            .filter(|&&id| self.system.has_emitter(id))
            .count();
        assert_eq!(playing, 1, "entry {entry}");
        let emitters = self.system.emitters();
        let e = (emitters.iter().rev())
            .filter(|e| e.phase != Phase::Ended)
            .find(|e| list.iter().any(|s| s.name.eq_ignore_ascii_case(&e.name)))
            .unwrap();
        assert!(e.two_d && !e.looped, "{e:?}");
        assert_eq!(e.position, [0.0; 3]);
        assert_eq!(e.volume, 1.0);
    }
}

#[test]
fn the_detector_ticks_near_anomalies_from_game_data() {
    let Some(root) = root() else {
        return;
    };
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let (_, actor) = level::new_game_level(&tree, &data).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();

    // 7.1: `detector_simple`, from the starting inventory, in slot 8.
    let section = level
        .detectors()
        .section_named("detector_simple")
        .unwrap()
        .clone();
    assert_eq!(section.radius, 10.0);
    let table: Vec<_> = (section.entries.iter())
        .map(|e| {
            (
                e.zone_class.as_str(),
                e.min_freq,
                e.max_freq,
                e.sounds.len(),
            )
        })
        .collect();
    assert_eq!(
        table,
        [
            ("ZS_MBALD", 0.1, 40.0, 4),
            ("ZS_MINCE", 0.1, 20.0, 1),
            ("Z_RADIO", 1.0, 20.0, 9),
            ("ZS_GALAN", 0.5, 30.0, 1),
        ]
    );
    let sounds = section.entries.iter().flat_map(|e| &e.sounds);
    assert!(sounds.clone().all(|s| s.volume == 1.0 && s.delay == 0.0));
    let names: Vec<&str> = sounds.map(|s| s.name.as_str()).collect();
    assert_eq!(names.len(), 15);
    assert_eq!(names[..4], ["detectors\\DA-2_beep1"; 4]);
    assert_eq!(names[5], "detectors\\geiger_1");
    let character = level.character().unwrap();
    let item = (character.inventory.items.iter())
        .find(|i| i.config.section == "detector_simple")
        .unwrap();
    assert_eq!(item.place, Place::Slot(8));

    // 7.2: 118 of the 178 zones tick (96 ZS_MBALD, 4 ZS_MINCE, 18
    // Z_RADIO), the 60 Z_MBALD never; each one sphere at its origin, radii
    // 1.086 … 7.71 m; the nearest 88.10 m from the actor's record.
    let zones = level.zones().zones();
    assert_eq!(zones.len(), 178);
    let ticking: Vec<_> = (zones.iter())
        .filter(|z| z.section.visible_by_detector && section.entry(&z.class_id).is_some())
        .collect();
    let mut by_class: BTreeMap<&str, usize> = BTreeMap::new();
    for z in &ticking {
        *by_class.entry(z.class_id.as_str()).or_default() += 1;
    }
    assert_eq!(
        by_class.into_iter().collect::<Vec<_>>(),
        [("ZS_MBALD", 96), ("ZS_MINCE", 4), ("Z_RADIO", 18)]
    );
    let z_mbald = zones.iter().filter(|z| z.class_id == "Z_MBALD");
    assert!(z_mbald.clone().all(|z| !z.section.visible_by_detector));
    assert_eq!(z_mbald.count(), 60);
    for z in &ticking {
        assert!(
            distance(z.placed_sphere().0, z.position()) < 1e-4,
            "{}",
            z.name
        );
    }
    let radii = ticking.iter().map(|z| z.placed_sphere().1);
    let (lo, hi) = radii.fold((f32::MAX, 0.0f32), |(lo, hi), r| (lo.min(r), hi.max(r)));
    assert!(
        (lo - 1.086).abs() < 5e-4 && (hi - 7.71).abs() < 5e-3,
        "{lo} … {hi}"
    );
    let from_actor = |z: &Zone| distance(actor.position, z.position());
    let nearest = (ticking.iter().copied())
        .min_by(|a, b| from_actor(a).total_cmp(&from_actor(b)))
        .unwrap();
    assert_eq!(nearest.section.name, "zone_mincer_weak_noart");
    let d = distance(actor.position, nearest.position());
    assert!((d - 88.10).abs() < 0.005, "{d}");

    // An entry-1 zone far from every other ticking zone, and a radioactive
    // one, to walk to.
    let isolation = |z: &Zone| {
        (ticking.iter())
            .filter(|o| o.id != z.id)
            .map(|o| distance(o.position(), z.position()) - o.placed_sphere().1)
            .fold(f32::MAX, f32::min)
    };
    let pick = |class_id: &str| {
        let z = (ticking.iter())
            .filter(|z| z.class_id == class_id && !z.name.contains("galantine"))
            .max_by(|a, b| isolation(a).total_cmp(&isolation(b)))
            .unwrap();
        (z.id, z.position(), z.placed_sphere().1, isolation(z))
    };
    let bald = pick("ZS_MBALD");
    let radio = pick("Z_RADIO");
    eprintln!(
        "walking to zone {} (R {:.4}, {:.1} m clear) and standing by zone {} (R {:.4}, {:.1} m clear)",
        bald.0, bald.2, bald.3, radio.0, radio.2, radio.3
    );
    // Near the first no other ticking zone is in range; the radioactive
    // zones lie close together, so others tick by the second.
    assert!(bald.3 > 30.0, "{bald:?}");

    let mut sound = level.take_sound_data().unwrap();
    sound.pairs = None;
    let mut system = SoundSystem::new(24);
    system.set_volume(Category::Effects, 1.0);
    let sounds = LevelSounds::create(&mut system, &tree, &data.aliases, &sound).unwrap();
    let mut run = Run {
        tree: &tree,
        data: &data,
        level,
        system,
        sounds,
        section,
        now: 0,
        model: BTreeMap::new(),
        ticks: Vec::new(),
        joined: Vec::new(),
    };

    // 7.2: no zone in range at the start; the detector is on (3).
    for _ in 0..120 {
        run.frame(None);
    }
    assert_eq!(run.level.state().detectors.len(), 1);
    let report = run.report();
    assert!(report.on && report.range.is_empty());
    assert!(run.ticks.is_empty());
    let created: usize = run.sounds.detectors.values().map(|d| d.ids().count()).sum();
    assert_eq!(created, 15);

    // Walking to the zone at 2 m/s from 16 m along x, then standing at
    // its centre for 2 s.
    let (id, centre, r, _) = bald;
    let start = run.now / STEP;
    let frames = (16.0 / 2.0 * 1000.0 / STEP as f32) as u32;
    for k in 0..=frames {
        let x = 16.0 * (1.0 - k as f32 / frames as f32);
        run.frame(Some([centre[0] + x, centre[1], centre[2]]));
    }
    let arrived = run.now / STEP;
    for _ in 0..125 {
        run.frame(Some(centre));
    }
    // 4: it joined the zone list once, within R + ρ of the sphere's centre
    // and no more than a scheduled period's walk later.
    assert_eq!(run.joined.len(), 1, "{:?}", run.joined);
    let (joined, at) = run.joined[0];
    assert_eq!(joined, id);
    assert!(
        at < r + 10.0 && at > r + 10.0 - 0.15,
        "joined at {at} m (R {r})"
    );
    let walk: Vec<_> = run.ticks.iter().filter(|t| t.0 > start).collect();
    assert!(walk.iter().all(|t| t.1 == 0 && t.2 == id));
    // The ticks quicken as the player nears: the intervals never grow.
    let frames_of: Vec<u64> = walk.iter().map(|t| t.0).collect();
    let gaps: Vec<u64> = frames_of.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps.windows(2).all(|g| g[1] <= g[0]), "{gaps:?}");
    // 8.2: at the zone, a tick every 3 frames of 16 ms (48 ms).
    let standing: Vec<u64> = frames_of
        .iter()
        .copied()
        .filter(|&f| f > arrived + 3)
        .collect();
    assert!(standing.len() > 30);
    assert!(
        standing.windows(2).all(|w| w[1] - w[0] == 3),
        "{standing:?}"
    );
    eprintln!(
        "walk: joined at {at:.3} m, {} ticks, first gap {} frames, then every 3",
        walk.len(),
        gaps.first().unwrap()
    );

    // Standing at the radioactive zone's centre (s ≤ 0.8 R): the bald zone
    // leaves the lists, the radioactive one joins (with its neighbours);
    // 8.2: f = 20, T = 50, a tick every 5 frames of 16 ms, each a sound of
    // its list.
    let (id, centre, _, _) = radio;
    let before = run.now / STEP;
    for _ in 0..125 {
        run.frame(Some(centre));
    }
    assert!(run.joined.iter().any(|j| j.0 == id));
    let after: Vec<_> = run.ticks.iter().filter(|t| t.0 > before).collect();
    assert!(after.iter().all(|t| t.1 == 2), "{after:?}");
    let geiger: Vec<u64> = (after.iter()).filter(|t| t.2 == id).map(|t| t.0).collect();
    assert!(geiger.len() > 20);
    assert!(geiger.windows(2).all(|w| w[1] - w[0] == 5), "{geiger:?}");
    let report = run.report();
    assert!(report.range.contains(&id) && !report.range.contains(&bald.0));

    // The detector to the backpack: off, no ticks (3, 5.1); back to its
    // slot: on, ticking again.
    let detector = report.id;
    let c = run.level.character_mut().unwrap();
    assert!(c.inventory.move_to_backpack(detector));
    let ticks = run.ticks.len();
    for _ in 0..60 {
        run.frame(Some(centre));
    }
    assert!(!run.report().on);
    assert_eq!(run.ticks.len(), ticks);
    let c = run.level.character_mut().unwrap();
    assert!(c.inventory.move_to_slot(detector).succeeded());
    for _ in 0..60 {
        run.frame(Some(centre));
    }
    assert!(run.report().on);
    assert!(run.ticks.len() > ticks);
}
