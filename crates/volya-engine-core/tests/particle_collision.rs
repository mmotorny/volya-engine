// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Particle collision on the Russian edition (BYOA): the data facts of
//! docs/behavior/particle-collision.md section 7.1, its test vectors on
//! `l01_escape`'s `level.cform` (7.2), and every colliding effect of the
//! library run against that level at the player's start, with the cost of
//! the collision step reported (meaningful in release builds). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]
// Numbers are written as the spec writes them.
#![allow(clippy::excessive_precision)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::{Duration, Instant};

use volya_engine_core::collision::CollisionWorld;
use volya_engine_core::particles::collision::{Hit, MASK_STATIC, ray, step};
use volya_engine_core::particles::{
    EffectInstance, GroupInstance, IDENTITY, Matrix, Mode, Particle, Random, STEP,
};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, Vfs, read};
use volya_engine_formats::gamemtl;
use volya_engine_formats::particles::{Collision, Library, effect_flags as f};

struct Data {
    library: Library,
    world: CollisionWorld,
}

fn data() -> Option<Data> {
    let archives = gog_archives::archives()?;
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let library = Library::parse(&read(&tree, "gamedata/particles.xr").unwrap()).unwrap();
    let materials = gamemtl::parse(&read(&tree, "gamedata/gamemtl.xr").unwrap()).unwrap();
    let cform = tree.open("gamedata/levels/l01_escape/level.cform").unwrap();
    let world = CollisionWorld::read(cform, materials).unwrap();
    Some(Data { library, world })
}

/// The actor's start position 1 m above the stored one (P5 7.2).
const START: [f32; 3] = [-246.72630, -23.793261, -134.43387];
const FLOOR: u32 = 424_895;
const CEILING: u32 = 424_977;

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 1e-4 * b[i].abs().max(1.0))
}

fn particle(velocity: [f32; 3]) -> Particle {
    Particle {
        position: [0, 1, 2].map(|i| START[i] + velocity[i] * STEP),
        previous: START,
        velocity,
        ..Particle::default()
    }
}

fn first_ray(world: &CollisionWorld, p: &Particle) -> Option<Hit> {
    let d = [0, 1, 2].map(|i| p.position[i] - p.previous[i]);
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    ray(world, p.previous, d.map(|c| c * (1.0 / l)), l, MASK_STATIC)
}

fn values(tangential: f32, resilience: f32, cutoff: f32) -> Collision {
    Collision {
        tangential,
        resilience,
        cutoff,
    }
}

#[test]
fn the_library_facts_are_p5s() {
    let Some(Data { library, .. }) = data() else {
        return;
    };
    let fx = &library.effects;
    let has = |e: &&volya_engine_formats::particles::Effect, b| e.flags & b != 0;
    let colliding: Vec<_> = fx.iter().filter(|e| has(e, f::COLLIDE)).collect();
    // D1.
    assert_eq!(colliding.len(), 71);
    assert_eq!(colliding.iter().filter(|e| has(e, f::SPRITE)).count(), 70);
    let undrawn: Vec<_> = colliding.iter().filter(|e| !has(e, f::SPRITE)).collect();
    assert_eq!(undrawn[0].name, "alexmx\\test1_old00");
    // D2.
    let destroy = colliding.iter().filter(|e| has(e, f::COLLIDE_DESTROY));
    assert_eq!(destroy.count(), 31);
    let idle = fx
        .iter()
        .filter(|e| has(e, f::COLLIDE_DESTROY) && !has(e, f::COLLIDE));
    assert_eq!(idle.count(), 9);
    let mut query: Vec<_> = fx
        .iter()
        .filter(|e| has(e, f::COLLIDE_QUERY))
        .map(|e| (e.name.as_str(), e.flags))
        .collect();
    query.sort();
    assert_eq!(
        query,
        [
            ("anomaly2\\group_items\\studen_idle_00", 0xF1001),
            ("explosions\\group_items\\expl_mushroom_fire_small", 0x94C01),
        ]
    );
    let by_name = |n: &str| fx[library.effect(n).unwrap()].collision;
    assert_eq!(
        by_name("explosions\\group_items\\expl_mushroom_fire_small"),
        values(0.72, 0.0, 0.0)
    );
    // D3.
    let bouncing: Vec<_> = colliding
        .iter()
        .filter(|e| !has(e, f::COLLIDE_DESTROY))
        .collect();
    assert_eq!(bouncing.len(), 40);
    let count = |c: Collision| bouncing.iter().filter(|e| e.collision == c).count();
    assert_eq!(count(values(1.0, 0.0, 0.0)), 8);
    assert_eq!(count(values(1.0, 0.2, 0.0)), 3);
    assert_eq!(count(values(1.0, 0.5, 0.0)), 3);
    let cut: Vec<_> = bouncing
        .iter()
        .filter(|e| e.collision.cutoff > 0.0)
        .map(|e| {
            let c = e.collision;
            // The values as P5 writes them, to two decimals.
            let r = |v: f32| (v * 100.0).round() / 100.0;
            (
                e.name.as_str(),
                [c.tangential, c.resilience, c.cutoff].map(r),
            )
        })
        .collect();
    assert_eq!(
        cut,
        [
            ("alexmx\\test1_old00", [0.91, 1.0, 1.0]),
            (
                "anomaly2\\group_items\\body_tear_blood_03",
                [0.5, 0.3, 0.25]
            ),
        ]
    );
    assert!(colliding.iter().all(|e| {
        let c = e.collision;
        (0.0..=1.0).contains(&c.tangential)
            && (0.0..=1.0).contains(&c.resilience)
            && c.cutoff >= 0.0
    }));
    // D6: the stormy fog's members.
    let fog = |n: &str| &fx[library.effect(n).unwrap()];
    assert_eq!(
        fog("nature\\group_items\\fog_stormy_dust_02").flags,
        0x31001
    );
    let dust = fog("nature\\group_items\\fog_stormy_dust");
    assert_eq!(
        (dust.flags, dust.collision),
        (0x11401, values(0.95, 0.25, 0.0))
    );
}

#[test]
fn the_level_vectors_are_p5s() {
    let Some(Data { world, .. }) = data() else {
        return;
    };
    let w = &world;
    let run = |flags, c: Collision, velocity| {
        let mut ps = [particle(velocity)];
        let n = step(&mut ps, 1, flags, &c, Some(w), None);
        (n, ps[0])
    };
    // V1.
    let v1 = particle([0.0, -40.0, 0.0]);
    let Some(Hit::Triangle { index, t }) = first_ray(w, &v1) else {
        panic!("V1: no hit")
    };
    assert_eq!(index, FLOOR);
    assert!((t - 0.999868).abs() < 1e-4, "{t}");
    let (n, p) = run(0x10000, values(1.0, 0.5, 0.0), [0.0, -40.0, 0.0]);
    assert_eq!(n, 1);
    assert!(close(p.velocity, [-0.26231, 19.99886, -0.0044]), "{p:?}");
    assert!(
        close(p.position, [-246.73495, -23.1333, -134.43402]),
        "{p:?}"
    );
    // V2.
    assert_eq!(run(0x30000, values(1.0, 0.5, 0.0), [0.0, -40.0, 0.0]).0, 0);
    // V3, with the distance as corrected in review.
    let v3 = particle([3.0, 100.0, 0.0]);
    let Some(Hit::Triangle { index, t }) = first_ray(w, &v3) else {
        panic!("V3: no hit")
    };
    assert_eq!(index, CEILING);
    assert!((t - 2.178059).abs() < 1e-4 * 2.178059, "{t}");
    let (_, p) = run(0x10000, values(0.5, 0.5, 0.0), [3.0, 100.0, 0.0]);
    assert!(close(p.velocity, [0.51653, 24.35004, 5.68958]), "{p:?}");
    assert!(
        close(p.position, [-246.70926, -22.98971, -134.24611]),
        "{p:?}"
    );
    // The second test hit the floor: after the ceiling's bounce the
    // particle went down, after the floor's it goes up.
    assert!(p.velocity[1] > 0.0);
    // V4.
    let v4 = particle([0.0, -20.0, 0.0]);
    assert_eq!(first_ray(w, &v4), None);
    assert_eq!(run(0x10000, values(1.0, 0.5, 0.0), [0.0, -20.0, 0.0]).1, v4);
    // V5.
    let (_, p) = run(0x10000, values(1.0, 0.5, 0.0), [0.0, -0.0001, 0.0]);
    assert_eq!(p.position, START);
}

/// A colliding effect, alone or in a group, playing at the start.
enum Playing {
    Effect(EffectInstance),
    Group(GroupInstance),
}

impl Playing {
    fn update(&mut self, library: &Library, random: &mut Random, world: Option<&CollisionWorld>) {
        match self {
            Playing::Effect(e) => e.update_in(33, random, world),
            Playing::Group(g) => g.update_in(33, library, random, world),
        }
    }

    fn particles(&self) -> Vec<Particle> {
        match self {
            Playing::Effect(e) => e.particles().to_vec(),
            Playing::Group(g) => g.effects().flat_map(|e| e.particles().to_vec()).collect(),
        }
    }
}

/// The matrix placing an effect at `p`.
fn at(p: [f32; 3]) -> Matrix {
    let mut m = IDENTITY;
    m[3] = [p[0], p[1], p[2], 1.0];
    m
}

/// `steps` updates of 33 ms of the effect `e` placed at `p`, with or
/// without the level, and the time the updates took.
fn play(
    library: &Library,
    e: usize,
    p: [f32; 3],
    steps: usize,
    world: Option<&CollisionWorld>,
) -> (Vec<Particle>, Duration) {
    let mut i = EffectInstance::new(e, &library.effects[e]);
    i.place(&at(p), [0.0; 3], Mode::World);
    i.play();
    let mut playing = Playing::Effect(i);
    let mut random = Random::default();
    let start = Instant::now();
    for _ in 0..steps {
        playing.update(library, &mut random, world);
    }
    (playing.particles(), start.elapsed())
}

#[test]
fn the_colliding_effects_collide_with_the_first_level() {
    let Some(Data { library, world }) = data() else {
        return;
    };
    // 2 m above the floor at the start, for 3 s.
    let p = [START[0], START[1] + 1.0, START[2]];
    let steps = 90;
    let mut changed = Vec::new();
    let (mut with, mut without) = (Duration::ZERO, Duration::ZERO);
    let mut particles = 0;
    for (e, effect) in library.effects.iter().enumerate() {
        if effect.flags & f::COLLIDE == 0 {
            continue;
        }
        let (a, ta) = play(&library, e, p, steps, Some(&world));
        let (b, tb) = play(&library, e, p, steps, None);
        with += ta;
        without += tb;
        particles += a.len();
        assert!(
            a.iter().all(|q| q.position.iter().all(|c| c.is_finite())),
            "{}",
            effect.name
        );
        if a != b {
            changed.push(effect.name.as_str());
        }
    }
    eprintln!(
        "{} of 71 colliding effects changed by the level in {steps} steps at the start; \
         {particles} live particles at the end; all 71 updates: {with:.1?} with collision, \
         {without:.1?} without",
        changed.len()
    );
    assert!(changed.len() > 10, "{changed:?}");
    // The stormy fog's falling dust is removed on hitting the level.
    assert!(changed.contains(&"nature\\group_items\\fog_stormy_dust_02"));
}

/// The weather's stormy fog at the start (P5 7.1 D6), the colliding effect
/// the first level plays: what its collision step costs per update.
#[test]
fn the_cost_of_the_stormy_fogs_collision() {
    let Some(Data { library, world }) = data() else {
        return;
    };
    let g = library.group("nature\\fog_stormy_01").unwrap();
    let p = [START[0], START[1] + 1.0, START[2]];
    let timing = |world: Option<&CollisionWorld>| {
        let mut group = GroupInstance::new(&library, g).unwrap();
        group.place(&at(p), [0.0; 3], Mode::World);
        group.play();
        let mut playing = Playing::Group(group);
        let mut random = Random::default();
        // Warm up 10 s, then time 10 s of updates.
        for _ in 0..300 {
            playing.update(&library, &mut random, world);
        }
        let start = Instant::now();
        for _ in 0..300 {
            playing.update(&library, &mut random, world);
        }
        (start.elapsed() / 300, playing.particles().len())
    };
    let (with, n_with) = timing(Some(&world));
    let (without, n_without) = timing(None);
    eprintln!(
        "nature\\fog_stormy_01 at the start: an update {with:.1?} with collision ({n_with} \
         particles), {without:.1?} without ({n_without} particles)"
    );
}
