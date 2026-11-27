// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The worked examples of docs/behavior/sound-player-body.md section 7 and
//! the rules of sections 3 – 6.

use super::*;
use crate::sound::system::NoDevice;
use crate::sound::{Sound, SoundParams};
use crate::vfs::db::DbVfs;

/// A sound of `ms` at 44,100 Hz with min distance 1.
fn sound(name: &str, ms: u64) -> Sound {
    Sound {
        name: name.into(),
        path: format!("gamedata\\sounds\\{name}.ogg"),
        params: SoundParams {
            min_distance: 1.0,
            max_distance: 50.0,
            base_volume: 1.0,
            ai_sound_type: 0,
            max_ai_distance: 25.0,
        },
        samples: ms * 441 / 10,
    }
}

const EFFECTOR: ShockEffector = ShockEffector {
    pp_eff_name: None,
    pp_eff_cyclic: true,
    cam_eff_name: None,
};

fn effector() -> ShockEffector {
    ShockEffector {
        pp_eff_name: Some("snd_shock.ppe".into()),
        ..EFFECTOR
    }
}

/// The sounds of section 1 (`pain` lists for every type but radiation,
/// explosion, fire_wound and wound_2) with their durations.
fn names() -> BodySoundNames {
    let pain = ["actor\\pain1", "actor\\pain2", "actor\\pain3"].map(str::to_owned);
    let mut hits: Vec<Vec<String>> = vec![pain.to_vec(); TYPE_COUNT];
    hits[4] = vec!["monsters\\biting\\def_0".into()];
    hits[7] = vec!["affects\\tinnitus3a".into()];
    hits[8] = (1..=4).map(|i| format!("actor\\bullet_hit_{i}")).collect();
    hits[9] = (1..=4).map(|i| format!("actor\\hurt{i}")).collect();
    BodySoundNames {
        hits,
        breath: "actor\\breath_1".into(),
        heartbeat: "affects\\heartbeat".into(),
        deaths: (0..4).map(|i| format!("actor\\die{i}")).collect(),
    }
}

fn system() -> (SoundSystem, BodySounds) {
    let mut s = SoundSystem::new(16);
    let durations = [
        ("actor\\pain1", 354),
        ("actor\\pain2", 459),
        ("actor\\pain3", 334),
        ("monsters\\biting\\def_0", 128),
        ("affects\\tinnitus3a", 6000),
        ("actor\\bullet_hit_1", 169),
        ("actor\\bullet_hit_2", 160),
        ("actor\\bullet_hit_3", 176),
        ("actor\\bullet_hit_4", 217),
        ("actor\\hurt1", 623),
        ("actor\\hurt2", 264),
        ("actor\\hurt3", 244),
        ("actor\\hurt4", 366),
        ("actor\\breath_1", 2202),
        ("affects\\heartbeat", 3376),
        ("actor\\die0", 1359),
        ("actor\\die1", 1826),
        ("actor\\die2", 1526),
        ("actor\\die3", 808),
    ];
    for (name, ms) in durations {
        s.preload(sound(name, ms));
    }
    let tree = DbVfs::<std::io::Empty>::new();
    let body = BodySounds::create(
        &mut s,
        &tree,
        &PathAliases::default(),
        &names(),
        &effector(),
    )
    .unwrap();
    (s, body)
}

fn playing(s: &SoundSystem) -> Vec<String> {
    s.emitters()
        .into_iter()
        .filter(|e| e.phase != crate::sound::system::Phase::Ended)
        .map(|e| e.name)
        .collect()
}

const EYE: Vec3 = [1.0, 1.53, 2.0];

fn hit(kind: HitType, pick: usize, alive: bool) -> BodyEvent {
    BodyEvent::Hit {
        kind,
        power: 0.5,
        pick: Some(pick),
        alive,
        eye: EYE,
    }
}

#[track_caller]
fn near(got: f32, want: f32, tolerance: f32) {
    assert!((got - want).abs() <= tolerance, "{got} != {want}");
}

#[test]
fn the_shock_factor_over_the_tinnitus() {
    // 5.5 (M1): L = 6,000 ms, F0 = 1, frames of 20 ms.
    let mut shock = Shock::start(6000.0, 1.0, 0.0, 1.0, &effector());
    assert_eq!(shock.start_factor(), 0.1);
    let mut f = shock.start_factor();
    let mut at = Vec::new();
    for k in 1..=301 {
        if let Some(v) = shock.frame(20.0) {
            f = v;
        }
        at.push((k * 20, f));
        shock.camera_effects(k as f32 * 0.02, 0.02);
    }
    let factor = |ms: u32| at.iter().find(|(t, _)| *t == ms).unwrap().1;
    assert_eq!(factor(3000), 0.1);
    near(factor(3020), 0.106, 1e-5);
    near(factor(4500), 0.55, 1e-5);
    near(factor(6000), 1.0, 1e-5);
    near(factor(6020), 1.006, 1e-5);
    // The frame past L made the visual effect invalid: the shock ends.
    assert!(shock.visual.is_none() && shock.ends(true));
    // With F0 = 0.8: from 0.08 to 0.8.
    let mut shock = Shock::start(6000.0, 1.0, 0.0, 0.8, &effector());
    near(shock.start_factor(), 0.08, 1e-7);
    near(shock.frame(6000.0).unwrap(), 0.8, 1e-6);
}

#[test]
fn the_shock_life_sets_the_visual_intensity() {
    // 5.5 (M2): ℓ = 0.4, 2, 4, 6 s for P = 0.05, 0.5, 1, 2; f at the start.
    for (p, life, f) in [
        (0.05, 0.4, 0.05),
        (0.5, 2.0, 0.25),
        (1.0, 4.0, 0.5),
        (2.0, 6.0, 0.75),
    ] {
        let mut shock = Shock::start(6000.0, p, 10.0, 1.0, &effector());
        near(shock.life, life, 1e-6);
        shock.camera_effects(10.0, 0.0);
        near(shock.visual.unwrap().intensity, f, 1e-6);
    }
    // After the life, I = dt, which does not end the effect.
    let mut shock = Shock::start(6000.0, 1.0, 0.0, 1.0, &effector());
    shock.camera_effects(5.0, 0.016);
    near(shock.visual.unwrap().intensity, 0.016, 1e-7);
    // A paused frame (dt 0) brings I to its floor and ends it early.
    shock.camera_effects(5.0, 0.0);
    assert!(shock.visual.is_none());
    // Without a visual effect or camera animation the shock ends at once.
    assert!(Shock::start(6000.0, 1.0, 0.0, 1.0, &EFFECTOR).ends(true));
    assert!(Shock::start(6000.0, 1.0, 0.0, 1.0, &effector()).ends(false));
}

#[test]
fn hit_sounds_by_type_one_at_a_time() {
    let (mut s, mut body) = system();
    // 3 item 6: played at the eye, 3D, once.
    body.event(&mut s, hit(HitType::FIRE_WOUND, 2, true), 0.0);
    let e = &s.emitters()[0];
    assert_eq!(e.name, "actor\\bullet_hit_3");
    assert!(!e.two_d && !e.looped && e.position == EYE);
    // The list busy: a second fire_wound is silent; another list plays.
    body.event(&mut s, hit(HitType::FIRE_WOUND, 0, true), 0.0);
    body.event(&mut s, hit(HitType::BURN, 1, true), 0.0);
    body.event(&mut s, hit(HitType::STRIKE, 0, true), 0.0);
    assert_eq!(
        playing(&s),
        ["actor\\bullet_hit_3", "actor\\pain2", "actor\\pain1"]
    );
    // A dead player hears none; a telepathic hit has no pick.
    let (mut s, mut body) = system();
    body.event(&mut s, hit(HitType::WOUND, 0, false), 0.0);
    body.event(
        &mut s,
        BodyEvent::Hit {
            kind: HitType::TELEPATIC,
            power: 1.0,
            pick: None,
            alive: true,
            eye: EYE,
        },
        0.0,
    );
    assert!(playing(&s).is_empty());
    // The pick from the shared generator's state 0 is 38 (3, M7).
    assert_eq!(38 % 4, 2);
    assert_eq!(38 % 3, 2);
}

#[test]
fn an_explosion_rings_and_shocks() {
    // 3 item 5, 5.1, 5.5 (M5): the tinnitus plays, quieted by the shock it
    // starts (F 0.1); a second explosion sets its volume to 10 and does not
    // restart the shock.
    let (mut s, mut body) = system();
    body.event(&mut s, hit(HitType::EXPLOSION, 0, true), 3.0);
    let shock = *body.shock().unwrap();
    assert_eq!((shock.length, shock.life, shock.end), (6000.0, 2.0, 5.0));
    assert_eq!(s.effects_factor(), 0.1);
    let e = &s.emitters()[0];
    assert_eq!((e.name.as_str(), e.volume), ("affects\\tinnitus3a", 1.0));
    body.frame(&mut s, 1000.0, true);
    body.event(&mut s, hit(HitType::EXPLOSION, 0, true), 4.0);
    assert_eq!(s.emitters()[0].volume, TINNITUS_VOLUME);
    assert_eq!(body.shock().unwrap().elapsed, 1000.0);
    // 0.1 · 10 = 1.0 heard (b · v · c).
    near(s.emitters()[0].volume * s.effects_factor(), 1.0, 1e-6);
    // The shock ends past L: F restored.
    for _ in 0..6 {
        body.frame(&mut s, 1000.0, true);
        body.camera_effects(4.0, 0.016);
    }
    body.frame(&mut s, 20.0, true);
    assert!(body.shock().is_none());
    assert_eq!(s.effects_factor(), 1.0);
    // A dead player's explosion starts a shock that ends at the next frame.
    let (mut s, mut body) = system();
    body.event(&mut s, hit(HitType::EXPLOSION, 0, false), 0.0);
    assert!(playing(&s).is_empty() && body.shock().is_some());
    body.frame(&mut s, 16.0, false);
    assert!(body.shock().is_none());
    assert_eq!(s.effects_factor(), 1.0);
}

#[test]
fn breathing_and_heartbeat() {
    let (mut s, mut body) = system();
    let state = |limping, alive, bleeding| BodyState {
        limping,
        alive,
        bleeding,
    };
    body.update(&mut s, state(false, true, 0.0));
    assert!(playing(&s).is_empty());
    // 4 item 1: limping, breathing 2D and looped at (0, 1.75, 0).
    body.update(&mut s, state(true, true, 0.0));
    let e = &s.emitters()[0];
    assert_eq!(e.name, "actor\\breath_1");
    assert!(e.two_d && e.looped && e.position == BODY_POSITION);
    body.update(&mut s, state(true, true, 0.0));
    assert_eq!(playing(&s).len(), 1, "kept, not restarted");
    body.update(&mut s, state(false, true, 0.0));
    assert!(playing(&s).is_empty());
    // 4 item 2 (M4): B 0.7 → v 0.95; 0.55 silent; 0.7 (1.0 and 0.4) 0.95;
    // 1.2 → 1.45; exactly 0.6 silent.
    for (b, v) in [
        (0.7, Some(0.95)),
        (0.55, None),
        (1.2, Some(1.45)),
        (0.6, None),
    ] {
        body.update(&mut s, state(false, true, b));
        let beat = s.emitters().into_iter().find(|e| {
            e.name == "affects\\heartbeat" && e.phase != crate::sound::system::Phase::Ended
        });
        match v {
            Some(v) => {
                let e = beat.expect("the heartbeat plays");
                near(e.volume, v, 1e-6);
                assert!(e.two_d && e.looped);
            }
            None => assert!(beat.is_none(), "{b}"),
        }
    }
    // 4 item 3: dead, both stop.
    body.update(&mut s, state(true, true, 1.0));
    assert_eq!(playing(&s).len(), 2);
    body.update(&mut s, state(true, false, 1.0));
    assert!(playing(&s).is_empty());
    // The 2D sounds' distance factor at 1.75 m (M3).
    let g = 1.0 / (1.0 + 0.75 * 0.75);
    near(g, 0.64, 1e-6);
}

#[test]
fn death_plays_its_sound_and_stops_the_rest() {
    let (mut s, mut body) = system();
    body.update(
        &mut s,
        BodyState {
            limping: true,
            alive: true,
            bleeding: 1.0,
        },
    );
    body.event(&mut s, hit(HitType::EXPLOSION, 0, true), 0.0);
    s.update(16, &mut NoDevice);
    body.event(
        &mut s,
        BodyEvent::Death {
            pick: 38 % 4,
            feet: [1.0, 0.0, 2.0],
        },
        0.0,
    );
    let names = playing(&s);
    assert!(names.contains(&"actor\\die2".to_owned()), "{names:?}");
    assert!(
        !names
            .iter()
            .any(|n| n.contains("breath") || n.contains("heartbeat"))
    );
    assert!(body.shock().is_none());
    assert_eq!(s.effects_factor(), 1.0);
}
