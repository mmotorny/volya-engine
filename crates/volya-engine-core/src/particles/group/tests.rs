// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Group timelines (particle-groups.md section 9.1) on records written
//! from P1 8 and P6 9.1.

use volya_engine_formats::particles::{ActionKind, Group, Library, Member};

use super::super::fixture::{action, effect, muhi_source};
use super::*;

fn member(effect: &str, start: f32, stop: f32, flags: u32) -> Member {
    Member {
        effect: effect.into(),
        per_particle_child: String::new(),
        birth_child: String::new(),
        death_child: String::new(),
        start_time: start,
        stop_time: stop,
        flags,
    }
}

/// An effect making particles that live 2 s, with a time limit or not.
fn emitter(name: &str, time_limit: Option<f32>) -> volya_engine_formats::particles::Effect {
    let actions = vec![
        action(21, 0, ActionKind::Source(Box::new(muhi_source()))),
        action(
            10,
            0,
            ActionKind::KillOld {
                age_limit: 2.0,
                kill_younger: 0,
            },
        ),
        action(12, 0, ActionKind::Move),
    ];
    let (flags, t) = time_limit.map_or((0, 0.0), |t| (0x4000, t));
    effect(name, 7, flags, t, actions)
}

fn group(name: &str, time_limit: f32, members: Vec<Member>) -> Group {
    Group {
        id: 0,
        name: name.into(),
        flags: 0,
        members,
        time_limit,
    }
}

/// Member effects first played, by update number (from 1).
fn starts(g: &mut GroupInstance, library: &Library, es: &[u32]) -> Vec<Vec<usize>> {
    let mut random = Random::default();
    let mut was = vec![false; g.member_count()];
    let mut out = Vec::new();
    for &e in es {
        g.update(e, library, &mut random);
        let mut now = Vec::new();
        for (i, w) in was.iter_mut().enumerate() {
            if !*w && g.member_effect(i).is_playing() {
                *w = true;
                now.push(i);
            }
        }
        out.push(now);
    }
    out
}

#[test]
fn the_group_step_is_e_times_0_001_in_f32() {
    let dt = |e: u32| (f64::from(e) * f64::from(0.001f32)) as f32;
    assert_eq!(f64::from(dt(33)), 0.032_999_999_821_186_066);
    assert_eq!(f64::from(dt(16)), 0.016_000_000_759_959_22);
}

#[test]
fn campfire_05s_members_start_as_p6_says() {
    // P1 8's members; P6 9.1's timeline.
    let names = [
        "explosions\\group_items\\campfire_05_flame00",
        "explosions\\group_items\\campfire_03_smoke",
        "explosions\\group_items\\campfire_02_sparks",
        "static\\fire_distort",
        "explosions\\group_items\\campfire_05_flame01",
        "explosions\\group_items\\campfire_05_vacuum",
        "explosions\\group_items\\campfire_glow_05",
    ];
    let members = vec![
        member(names[0], 0.0, 0.0, 0x4),
        member(names[1], 0.0, 0.0, 0x0),
        member(names[2], 0.5, 0.0, 0x4),
        member(names[3], 0.0, 0.0, 0x4),
        member(names[4], 0.0, 0.0, 0x4),
        member(names[5], 0.0, 0.0, 0x4),
        member(names[6], 0.0, 0.0, 0x4),
    ];
    let effects = names.iter().map(|n| emitter(n, None)).collect();
    let library = Library::new(
        effects,
        vec![group("explosions\\campfire_05", 0.0, members)],
    );
    let mut g = GroupInstance::new(&library, 0).unwrap();
    g.place(&IDENTITY, [0.0; 3], Mode::World);
    g.play();
    let got = starts(&mut g, &library, &[33; 40]);
    assert_eq!(got[0], [0, 3, 4, 5, 6]);
    assert_eq!(got[15], [2]);
    for (i, s) in got.iter().enumerate() {
        if i != 0 && i != 15 {
            assert!(s.is_empty(), "update {}: {s:?}", i + 1);
        }
    }
    assert!(!g.member_effect(1).is_playing());
    assert!(g.is_playing());
    assert!(g.bounds().is_set());
}

#[test]
fn stop_times_of_0_stop_only_while_t_is_0() {
    // `destroy_fx\destroy_small_box_00` (P6 9.1): time limit 1; member 0
    // 0x23E004 with stop time 4.48 × 10⁻⁴⁴, member 1 stop time 0; their
    // effects with time limits 1 and 1.5.
    let library = Library::new(
        vec![emitter("a", Some(1.0)), emitter("b", Some(1.5))],
        vec![group(
            "destroy_fx\\destroy_small_box_00",
            1.0,
            vec![
                member("a", 0.0, 4.48e-44, 0x23E004),
                member("b", 0.0, 0.0, 0x4),
            ],
        )],
    );
    let mut random = Random::default();
    let mut g = GroupInstance::new(&library, 0).unwrap();
    g.play();
    g.update(0, &library, &mut random);
    assert!(g.member_effect(0).is_playing() && g.member_effect(1).is_playing());
    g.update(33, &library, &mut random);
    assert!(!g.member_effect(0).is_playing() && !g.member_effect(1).is_playing());

    let mut g = GroupInstance::new(&library, 0).unwrap();
    g.play();
    let mut stopped = None;
    for k in 1..=40 {
        g.update(33, &library, &mut random);
        if k == 1 {
            assert!(g.member_effect(0).is_playing() && g.member_effect(1).is_playing());
        }
        if g.is_stopping() && stopped.is_none() {
            stopped = Some(k);
        }
    }
    assert_eq!(stopped, Some(31));
}

#[test]
fn birth_and_per_particle_children_follow_their_particles() {
    let mut parent = emitter("parent", None);
    parent.max_particles = 3;
    let mut m = member("parent", 0.0, 0.0, 0x26);
    m.per_particle_child = "child".into();
    m.birth_child = "burst".into();
    let library = Library::new(
        vec![parent, emitter("child", None), emitter("burst", Some(0.1))],
        vec![group("g", 0.0, vec![m])],
    );
    let mut random = Random::default();
    let mut g = GroupInstance::new(&library, 0).unwrap();
    g.play();
    for _ in 0..10 {
        g.update(33, &library, &mut random);
    }
    let member = &g.members[0];
    assert_eq!(member.per_particle.len(), member.effect.particles().len());
    for (c, p) in member.per_particle.iter().zip(member.effect.particles()) {
        assert_eq!(c.position(), p.position);
    }
    // 3 births, each with a burst child of 0.1 s that has ended by now.
    assert!(g.errors().is_empty());
    // Particles die after 2 s; their per-particle children become free
    // and run until they end.
    for _ in 0..70 {
        g.update(33, &library, &mut random);
    }
    assert!(!g.members[0].free.is_empty());
    // An immediate stop drops every child.
    g.stop_immediate();
    assert!(g.members[0].per_particle.is_empty() && g.members[0].free.is_empty());
}

#[test]
fn a_looped_birth_child_is_the_fatal_error() {
    let mut m = member("parent", 0.0, 0.0, 0x24);
    m.birth_child = "looped".into();
    let library = Library::new(
        vec![emitter("parent", None), emitter("looped", None)],
        vec![group("g", 0.0, vec![m])],
    );
    let mut g = GroupInstance::new(&library, 0).unwrap();
    g.play();
    g.update(33, &library, &mut Random::default());
    assert_eq!(
        g.errors()[0],
        "Can't use looped effect 'looped' as 'On Birth' child for group."
    );
}

#[test]
fn a_missing_member_effect_fails_the_group() {
    let library = Library::new(
        Vec::new(),
        vec![group("g", 0.0, vec![member("nope", 0.0, 0.0, 4)])],
    );
    assert_eq!(GroupInstance::new(&library, 0).unwrap_err(), NO_EFFECT);
}

#[test]
fn a_time_limit_stops_the_group_deferred_and_it_ends_when_nothing_plays() {
    let library = Library::new(
        vec![emitter("e", Some(0.2))],
        vec![group("g", 0.5, vec![member("e", 0.0, 0.0, 4)])],
    );
    let mut random = Random::default();
    let mut g = GroupInstance::new(&library, 0).unwrap();
    g.play();
    for _ in 0..200 {
        g.update(33, &library, &mut random);
    }
    assert!(!g.is_playing() && !g.is_stopping());
}
