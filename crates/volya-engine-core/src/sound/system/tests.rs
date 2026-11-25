// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The worked examples of docs/behavior/sound-positional.md section 10 and
//! the rules of sections 3 – 7.

use std::io;

use super::*;
use crate::sound::SoundParams;
use crate::vfs::db::DbVfs;

/// `sounds\ambient\sparks1.ogg` (section 10).
fn sparks1() -> Sound {
    Sound {
        name: "ambient\\sparks1".into(),
        path: "gamedata\\sounds\\ambient\\sparks1.ogg".into(),
        params: SoundParams {
            min_distance: 0.2,
            max_distance: 10.0,
            base_volume: 1.0,
            ai_sound_type: 0,
            max_ai_distance: 10.0,
        },
        samples: 176_953,
    }
}

/// `sounds\music\wasteland2_l.ogg` (section 10).
fn wasteland2() -> Sound {
    Sound {
        name: "music\\wasteland2_l".into(),
        path: "gamedata\\sounds\\music\\wasteland2_l.ogg".into(),
        params: SoundParams {
            min_distance: 1.0,
            max_distance: 300.0,
            base_volume: 1.0,
            ai_sound_type: 0,
            max_ai_distance: 300.0,
        },
        samples: 5_518_127,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Start {
        voice: usize,
        name: String,
        looped: bool,
        offset: u64,
    },
    Stop(usize),
    Listener(DeviceListener),
    Source(usize, DeviceSource),
}

#[derive(Default)]
struct Recorder(Vec<Event>);

impl Recorder {
    fn starts(&self) -> Vec<&Event> {
        self.0
            .iter()
            .filter(|e| matches!(e, Event::Start { .. }))
            .collect()
    }
}

impl Device for Recorder {
    fn start(&mut self, voice: usize, sound: &Sound, looped: bool, offset: u64) {
        self.0.push(Event::Start {
            voice,
            name: sound.name.clone(),
            looped,
            offset,
        });
    }
    fn stop(&mut self, voice: usize) {
        self.0.push(Event::Stop(voice));
    }
    fn set_listener(&mut self, listener: &DeviceListener) {
        self.0.push(Event::Listener(*listener));
    }
    fn set_source(&mut self, voice: usize, source: &DeviceSource) {
        self.0.push(Event::Source(voice, *source));
    }
}

/// A system with `voices` voices and `sounds` loaded (no files needed: the
/// loaded sounds are found by name first).
fn system(voices: usize, sounds: &[Sound]) -> SoundSystem {
    let mut s = SoundSystem::new(voices);
    s.set_volume(Category::Effects, 1.0);
    for sound in sounds {
        s.files
            .loaded
            .insert(sound.name.clone(), Rc::new(sound.clone()));
    }
    s
}

fn create(s: &mut SoundSystem, name: &str, category: Category) -> SoundId {
    let tree = DbVfs::<io::Empty>::new();
    s.create(&tree, &PathAliases::default(), name, category)
        .unwrap()
}

fn listener_at(position: Vec3) -> Listener {
    Listener {
        position,
        ..Listener::default()
    }
}

fn state(s: &SoundSystem) -> EmitterState {
    s.emitters()[0].clone()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 5e-6 * b.abs().max(1.0)
}

/// Updates every 16 ms from `*now`, `n` times.
fn run(s: &mut SoundSystem, now: &mut u64, n: usize, device: &mut dyn Device) {
    for _ in 0..n {
        *now += 16;
        s.update(*now, device);
    }
}

#[test]
fn distance_gain_and_factor_of_sparks1() {
    // Section 10: g and a at δ = 0.2 / 1 / 3 / 5 / 10 m.
    for (delta, g, a) in [
        (0.2, 1.0, 1.0),
        (1.0, 0.25, 0.266_667),
        (3.0, 0.086_957, 0.088_889),
        (5.0, 0.052_632, 0.053_333),
        (10.0, 0.026_49, 0.026_667),
    ] {
        let got = distance_gain(0.2, 10.0, ROLLOFF, delta);
        assert!((got - g).abs() < 5e-6, "g at {delta}: {got}");
        let got = distance_factor(0.2, delta);
        assert!((got - a).abs() < 5e-6, "a at {delta}: {got}");
    }
    // Half level at m + m / 0.75.
    assert!(close(
        distance_gain(0.2, 10.0, ROLLOFF, 0.2 + 0.2 / 0.75),
        0.5
    ));
    // Clamped beyond M, full within m, a = 1 at the emitter.
    assert!(close(distance_gain(0.2, 10.0, ROLLOFF, 20.0), 0.026_49));
    assert_eq!(distance_gain(0.2, 10.0, ROLLOFF, 0.0), 1.0);
    assert_eq!(distance_factor(0.2, 0.0), 1.0);
}

#[test]
fn sparks1_is_steady_within_its_max_distance_and_inaudible_beyond() {
    for delta in [0.2, 1.0, 3.0, 5.0, 10.0] {
        let mut s = system(16, &[sparks1()]);
        let id = create(&mut s, "ambient\\sparks1", Category::Effects);
        s.set_listener(listener_at([delta, 0.0, 0.0]));
        let (mut now, mut device) = (0, Recorder::default());
        s.play(id, PlayFlags::default());
        run(&mut s, &mut now, 10, &mut device);
        let e = state(&s);
        assert!(close(e.out, 1.0), "V at {delta}: {}", e.out);
        assert_eq!(e.phase, Phase::Playing);
    }
    let mut s = system(16, &[sparks1()]);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    s.set_listener(listener_at([10.5, 0.0, 0.0]));
    let (mut now, mut device) = (0, Recorder::default());
    s.play(id, PlayFlags::default());
    run(&mut s, &mut now, 3, &mut device);
    let e = state(&s);
    assert_eq!((e.out, e.phase, e.voice), (0.0, Phase::Silent, None));
    assert!(device.starts().is_empty());
}

#[test]
fn priority_and_stop_with_fade_at_3_m() {
    let mut s = system(16, &[sparks1()]);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    s.set_listener(listener_at([3.0, 0.0, 0.0]));
    let (mut now, mut device) = (0, Recorder::default());
    s.play(id, PlayFlags::default());
    run(&mut s, &mut now, 3, &mut device);
    assert!(
        close(state(&s).priority, 0.088_889),
        "{}",
        state(&s).priority
    );

    s.stop_with_fade(id);
    let fades = [0.84, 0.68, 0.52, 0.36, 0.2, 0.04, 0.0];
    let volumes = [
        0.984, 0.9536, 0.910_24, 0.855_216, 0.789_694, 0.714_725, 0.643_252,
    ];
    for (n, (fade, volume)) in fades.iter().zip(volumes).enumerate() {
        assert!(s.has_emitter(id), "ended before update {}", n + 1);
        let before = state(&s);
        run(&mut s, &mut now, 1, &mut device);
        if n < 6 {
            let e = state(&s);
            assert!((e.fade - fade).abs() < 1e-5, "φ {}: {}", n + 1, e.fade);
            assert!((e.out - volume).abs() < 1e-5, "V {}: {}", n + 1, e.out);
        } else {
            // Update 7 ends it.
            assert!(before.fade > 0.0);
            assert!(s.emitters().is_empty());
            assert!(!s.has_emitter(id));
        }
    }
    assert!(device.0.contains(&Event::Stop(0)));
}

#[test]
fn a_listener_walking_in_hears_the_volume_rise() {
    let mut s = system(16, &[sparks1()]);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    s.set_listener(listener_at([11.0, 0.0, 0.0]));
    let (mut now, mut device) = (0, Recorder::default());
    s.play(id, PlayFlags::default());
    run(&mut s, &mut now, 2, &mut device);
    assert_eq!(state(&s).out, 0.0);
    s.set_listener(listener_at([5.0, 0.0, 0.0]));
    let mut seen = Vec::new();
    for _ in 0..44 {
        run(&mut s, &mut now, 1, &mut device);
        seen.push(state(&s).out);
    }
    assert!(close(seen[0], 0.1) && close(seen[1], 0.19) && close(seen[2], 0.271));
    assert!(seen[43] >= 0.99 && seen[42] < 0.99, "{seen:?}");
}

#[test]
fn menu_music_in_2d() {
    let mut s = system(16, &[wasteland2()]);
    s.set_volume(Category::Music, 0.7);
    let id = create(&mut s, "music\\wasteland2_l", Category::Music);
    let (mut now, mut device) = (0, Recorder::default());
    s.play_at(
        id,
        [-0.5, 0.0, 0.3],
        PlayFlags {
            two_d: true,
            ..PlayFlags::default()
        },
    );
    run(&mut s, &mut now, 1, &mut device);
    assert!(close(state(&s).out, 0.7));
    run(&mut s, &mut now, 5, &mut device);
    assert!(close(state(&s).out, 0.7));
    let Some(Event::Source(0, source)) = device.0.last().cloned() else {
        panic!("{:?}", device.0.last());
    };
    assert!(source.relative);
    assert_eq!(source.position, [-0.5, 0.0, -0.3]);
    let listener = device
        .0
        .iter()
        .rev()
        .find_map(|e| match e {
            Event::Listener(l) => Some(*l),
            _ => None,
        })
        .unwrap();
    let (gain, pan) = heard(&source, &listener);
    assert!(close(gain, 0.7), "{gain}");
    assert!(pan < 0.0, "heard on the left: {pan}");
}

#[test]
fn a_source_on_the_right_is_heard_on_the_right() {
    // Listener at the origin looking along +z, up +y; source at (1, 0, 0).
    let listener = DeviceListener {
        position: to_device([0.0; 3]),
        at: to_device([0.0, 0.0, 1.0]),
        up: to_device([0.0, 1.0, 0.0]),
    };
    let source = |p| DeviceSource {
        reference_distance: 1.0,
        max_distance: 100.0,
        rolloff: ROLLOFF,
        position: to_device(p),
        relative: false,
        gain: 1.0,
        pitch: 1.0,
    };
    assert_eq!(cross(listener.at, listener.up), [1.0, 0.0, 0.0]);
    assert!(close(heard(&source([1.0, 0.0, 0.0]), &listener).1, 1.0));
    assert!(close(heard(&source([-2.0, 0.0, 0.0]), &listener).1, -1.0));
    assert!(close(heard(&source([0.0, 0.0, 5.0]), &listener).1, 0.0));
    // Turned to look along +x, the source ahead is centred.
    let turned = DeviceListener {
        at: to_device([1.0, 0.0, 0.0]),
        ..listener
    };
    assert!(close(heard(&source([1.0, 0.0, 0.0]), &turned).1, 0.0));
}

#[test]
fn device_parameters_of_a_voice() {
    let mut s = system(16, &[sparks1()]);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    let (mut now, mut device) = (0, Recorder::default());
    s.set_listener(Listener {
        position: [1.0, 2.0, 3.0],
        direction: [0.0, 0.0, 1.0],
        up: [0.0, 1.0, 0.0],
    });
    s.play_at(id, [1.0, 2.0, 4.0], PlayFlags::default());
    s.set_frequency(id, 3.0);
    run(&mut s, &mut now, 1, &mut device);
    assert_eq!(
        device.0,
        [
            Event::Start {
                voice: 0,
                name: "ambient\\sparks1".into(),
                looped: false,
                offset: 0
            },
            Event::Listener(DeviceListener {
                position: [1.0, 2.0, -3.0],
                at: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
            }),
            Event::Source(
                0,
                DeviceSource {
                    reference_distance: 0.2,
                    max_distance: 10.0,
                    rolloff: 0.75,
                    position: [1.0, 2.0, -4.0],
                    relative: false,
                    gain: 1.0,
                    pitch: 2.0,
                }
            ),
        ]
    );
}

/// Silent from 1,000 to 6,000 ms after its start (the listener beyond its
/// max distance meanwhile).
fn silent_between(looped: bool) -> (SoundSystem, Recorder, u64) {
    let mut s = system(16, &[sparks1()]);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    let mut device = Recorder::default();
    s.update(0, &mut device);
    s.play(
        id,
        PlayFlags {
            looped,
            ..PlayFlags::default()
        },
    );
    s.update(10, &mut device);
    assert_eq!(state(&s).phase, Phase::Playing);
    s.set_listener(listener_at([20.0, 0.0, 0.0]));
    let mut now = 1_010;
    s.update(now, &mut device);
    assert_eq!(state(&s).phase, Phase::Silent);
    while now < 6_000 {
        now += 10;
        s.update(now, &mut device);
        if s.emitters().is_empty() {
            break;
        }
    }
    s.set_listener(listener_at([0.0; 3]));
    now += 10;
    s.update(now, &mut device);
    (s, device, now)
}

#[test]
fn a_silent_emitter_ends_or_resumes_on_its_timeline() {
    // Non-looped: ended at 4,012 ms after its start (at 10 ms).
    let mut s = system(16, &[sparks1()]);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    let mut device = Recorder::default();
    s.update(0, &mut device);
    s.play(id, PlayFlags::default());
    s.update(10, &mut device);
    s.set_listener(listener_at([20.0, 0.0, 0.0]));
    s.update(4_021, &mut device);
    assert!(s.has_emitter(id));
    s.update(4_022, &mut device);
    assert!(!s.has_emitter(id));

    let (s, _, _) = silent_between(false);
    assert!(s.emitters().is_empty());

    // Looped: resumes at sample 44 · ((now − T_s) mod D).
    let (s, device, now) = silent_between(true);
    let e = state(&s);
    assert_eq!(e.phase, Phase::Playing);
    let Some(Event::Start { offset, looped, .. }) = device.starts().last().map(|e| (*e).clone())
    else {
        unreachable!()
    };
    assert!(looped);
    assert_eq!(offset, 44 * ((now - 10) % 4_012));
    // The spec's example: silent until 6,000 ms after its start.
    assert_eq!(44 * (6_000 % 4_012), 87_472);
}

#[test]
fn delay_waits_then_starts() {
    let mut s = system(16, &[sparks1()]);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    let mut device = Recorder::default();
    s.update(0, &mut device);
    s.play(
        id,
        PlayFlags {
            delay: 0.05,
            ..PlayFlags::default()
        },
    );
    for (now, phase) in [
        (16, Phase::Waiting),
        (32, Phase::Waiting),
        (48, Phase::Waiting),
        (64, Phase::Starting),
        (80, Phase::Playing),
    ] {
        s.update(now, &mut device);
        assert_eq!(state(&s).phase, phase, "at {now}");
    }
    assert!(close(s.play_time(id) as f32, 0.0));
    s.update(100, &mut device);
    assert!(close(s.play_time(id) as f32, 20.0));
}

#[test]
fn play_again_restarts_the_same_emitter() {
    let mut s = system(16, &[sparks1()]);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    let mut device = Recorder::default();
    s.update(0, &mut device);
    s.play(id, PlayFlags::default());
    s.update(16, &mut device);
    s.update(1_016, &mut device);
    assert_eq!(s.play_time(id), 1_000.0);
    s.play(id, PlayFlags::default());
    assert_eq!(s.play_time(id), 0.0);
    s.update(1_032, &mut device);
    assert_eq!(s.emitters().len(), 1);
    assert_eq!(device.starts().len(), 2, "rewound at the next update");
    assert!(matches!(device.starts()[1], Event::Start { offset: 0, .. }));
    // The end moved with the start: 4,012 ms from the restart.
    s.update(1_016 + 4_011, &mut device);
    assert!(s.has_emitter(id));
    s.update(1_016 + 4_012, &mut device);
    assert!(!s.has_emitter(id));
}

#[test]
fn voices_go_to_the_highest_priorities() {
    let mut s = system(4, &[sparks1()]);
    let mut device = Recorder::default();
    s.update(0, &mut device);
    // Five emitters at 1 … 5 m: the farthest has the lowest priority.
    let ids: Vec<_> = (1..=5)
        .map(|d| {
            let id = create(&mut s, "ambient\\sparks1", Category::Effects);
            s.play_at(id, [d as f32, 0.0, 0.0], PlayFlags::default());
            id
        })
        .collect();
    s.update(16, &mut device);
    let phases: Vec<_> = s.emitters().iter().map(|e| e.phase).collect();
    assert_eq!(phases[..4], [Phase::Playing; 4]);
    assert_eq!(phases[4], Phase::Silent);
    // The nearest walks away: the silent one takes its voice.
    s.set_position(ids[0], [9.0, 0.0, 0.0]);
    s.update(32, &mut device);
    let e = s.emitters();
    assert_eq!(e[0].phase, Phase::Silent);
    assert_eq!(e[4].phase, Phase::Playing);
    assert_eq!(e[4].voice, Some(0));
}

#[test]
fn a_voice_is_taken_only_by_a_strictly_higher_priority() {
    let mut s = system(4, &[sparks1()]);
    let mut device = Recorder::default();
    s.update(0, &mut device);
    for _ in 0..5 {
        let id = create(&mut s, "ambient\\sparks1", Category::Effects);
        s.play_at(id, [2.0, 0.0, 0.0], PlayFlags::default());
    }
    for now in [16, 32, 48] {
        s.update(now, &mut device);
    }
    let e = s.emitters();
    assert_eq!(e[4].phase, Phase::Silent);
    assert!(e[..4].iter().all(|e| e.phase == Phase::Playing));
}

#[test]
fn pauses_nest_and_freeze_the_timeline() {
    let mut s = system(16, &[sparks1()]);
    let a = create(&mut s, "ambient\\sparks1", Category::Effects);
    let mut device = Recorder::default();
    s.update(0, &mut device);
    s.play(a, PlayFlags::default());
    s.update(10, &mut device);
    s.pause();
    let b = create(&mut s, "ambient\\sparks1", Category::Effects);
    s.play(b, PlayFlags::default());
    s.update(20, &mut device);
    assert!(device.0.contains(&Event::Stop(0)), "paused: voice released");
    let e = s.emitters();
    assert_eq!((e[0].phase, e[0].paused), (Phase::Silent, true));
    // Created after the pause: plays.
    assert_eq!((e[1].phase, e[1].paused), (Phase::Playing, false));
    // A second pause marks b too: both frozen.
    s.pause();
    for now in (30..=10_000).step_by(10) {
        s.update(now, &mut device);
    }
    assert!(
        s.has_emitter(a) && s.has_emitter(b),
        "frozen past their ends"
    );
    // Paused before the update at 20: none of its time counts.
    assert_eq!(s.play_time(a), 0.0);
    // Leaving the second level resumes b only; b ends 4,012 ms after its
    // start, counting its unpaused time.
    s.resume();
    for now in (10_010..=14_100).step_by(10) {
        s.update(now, &mut device);
    }
    assert!(!s.has_emitter(b), "b ended");
    assert!(state(&s).paused);
    s.resume();
    assert_eq!(s.pause_level(), 0);
    s.update(14_110, &mut device);
    assert_eq!(state(&s).phase, Phase::Playing);
    // Resumes where it was paused: this update's 10 ms.
    assert_eq!(s.play_time(a), 10.0);
    let Some(Event::Start { offset, .. }) = device.starts().last().map(|e| (*e).clone()) else {
        unreachable!()
    };
    assert_eq!(offset, 44 * 10);
}

#[test]
fn stop_and_stop_all_end_at_once() {
    let mut s = system(16, &[sparks1()]);
    let a = create(&mut s, "ambient\\sparks1", Category::Effects);
    let b = create(&mut s, "ambient\\sparks1", Category::Effects);
    let mut device = Recorder::default();
    s.play(a, PlayFlags::default());
    s.play(b, PlayFlags::default());
    s.update(0, &mut device);
    s.stop(a);
    assert!(!s.has_emitter(a));
    s.update(16, &mut device);
    assert_eq!(s.emitters().len(), 1);
    assert!(device.0.contains(&Event::Stop(0)));
    s.stop_all();
    s.update(32, &mut device);
    assert!(s.emitters().is_empty());
    assert!(device.0.contains(&Event::Stop(1)));
    // Independent plays run on their own.
    s.play_independent(
        a,
        PlayFlags::default(),
        Settings {
            volume: Some(0.5),
            ..Settings::default()
        },
    );
    s.update(48, &mut device);
    assert!(!s.has_emitter(a));
    assert!(close(state(&s).out, 0.5));
}

#[test]
fn category_volumes_apply_at_once_through_the_smoothing() {
    let mut s = system(16, &[sparks1()]);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    let mut device = Recorder::default();
    s.play(id, PlayFlags::default());
    s.update(0, &mut device);
    s.set_volume(Category::Effects, 0.5);
    s.update(16, &mut device);
    assert!(close(state(&s).out, 0.95));
    s.set_effects_factor(2.0);
    assert_eq!(s.effects_factor(), 1.0);
    s.set_effects_factor(0.0);
    let mut now = 16;
    run(&mut s, &mut now, 60, &mut device);
    assert!(s.emitters()[0].out < AUDIBLE);
    assert_eq!(state(&s).phase, Phase::Silent);
}

#[test]
fn a_restart_stops_everything_and_changes_the_voices() {
    let mut s = system(16, &[sparks1()]);
    assert_eq!(s.voice_count(), 16);
    let id = create(&mut s, "ambient\\sparks1", Category::Effects);
    let mut device = Recorder::default();
    s.play(id, PlayFlags::default());
    s.update(0, &mut device);
    s.restart(100);
    assert_eq!(s.voice_count(), MAX_VOICES);
    s.update(16, &mut device);
    assert!(s.emitters().is_empty());
    assert!(device.0.contains(&Event::Stop(0)));
    assert_eq!(SoundSystem::new(1).voice_count(), MIN_VOICES);
}

#[test]
fn a_failed_creation_leaves_no_sound_behind() {
    let mut s = system(16, &[sparks1()]);
    let kept = create(&mut s, "ambient\\sparks1", Category::Effects);
    let tree = DbVfs::<io::Empty>::new();
    let aliases = PathAliases::default();
    let created = s.all_or_none(|s| {
        s.create(&tree, &aliases, "ambient\\sparks1", Category::Effects)?;
        s.create(&tree, &aliases, "ambient\\missing", Category::Effects)
    });
    assert!(created.is_err());
    assert_eq!(s.sound_ids().collect::<Vec<_>>(), [kept]);
}
