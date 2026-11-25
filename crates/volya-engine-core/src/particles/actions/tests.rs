// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The test vectors of particle-motion-actions.md section 6.1 and
//! particle-appearance-actions.md section 8.

use super::super::fixture::{point, region, sphere_03};
use super::*;
use volya_engine_formats::particles::region_kind as kind;

fn particle(position: Vec3, velocity: Vec3, age: f32) -> Particle {
    Particle {
        position,
        previous: position,
        velocity,
        age,
        ..Particle::default()
    }
}

/// The standard particles of P3 6.1.
fn standard() -> Vec<Particle> {
    vec![
        particle([1.0, 2.0, 3.0], [0.5, -1.0, 2.0], 0.25),
        particle([-2.0, 0.5, 1.0], [3.0, 0.0, -1.0], 1.0),
        particle([0.5, -1.0, -0.5], [0.0; 3], 0.0),
    ]
}

fn bounce_b() -> Vec<Particle> {
    vec![
        particle([0.5, 0.2, 0.5], [1.0, -1.0, 0.0], 0.0),
        particle([1.8, 0.1, 0.5], [0.0, -2.0, 0.0], 0.0),
        particle([0.1, 0.3, 0.1], [0.0, -1.0, 0.0], 0.0),
    ]
}

fn sphere_c() -> Vec<Particle> {
    vec![
        particle([1.2, 0.0, 0.0], [-2.0, 0.5, 0.0], 0.0),
        particle([0.0, 0.5, 0.0], [0.0, -1.0, 0.0], 0.0),
        particle([0.0, 0.0, 0.95], [0.0, 0.0, 1.0], 0.0),
    ]
}

/// Runs `kind` once with Δ `dt`, the generator at `state` and
/// `turbulence`'s time `time`; returns the particles and the notices.
fn step(
    kind: &ActionKind,
    mut particles: Vec<Particle>,
    dt: f32,
    state: u32,
    time: f32,
) -> (Vec<Particle>, Vec<Notice>) {
    let mut random = Random::new(state);
    let mut notices = Vec::new();
    let mut time = time;
    let mut s = Step {
        particles: &mut particles,
        max: 100,
        random: &mut random,
        notices: Some(&mut notices),
        dt,
    };
    run(kind, 0, &mut time, &mut s);
    (particles, notices)
}

fn close(got: Vec3, want: Vec3) -> bool {
    (0..3).all(|i| (got[i] - want[i]).abs() <= want[i].abs() * 2e-5 + 2e-6)
}

fn check(name: &str, got: &[Particle], want: &[(Vec3, Vec3)]) {
    for (i, (p, (x, w))) in got.iter().zip(want).enumerate() {
        assert!(
            close(p.position, *x) && close(p.velocity, *w),
            "{name} {i}: {:?} {:?}, want {x:?} {w:?}",
            p.position,
            p.velocity
        );
    }
}

/// Every case of P3 6.1's table.
#[test]
#[allow(clippy::type_complexity)]
fn motion_actions_give_p3s_results() {
    let d = 0.033;
    let x = |i: usize| standard()[i].position;
    let w = |i: usize| standard()[i].velocity;
    let same = |i: usize| (x(i), w(i));
    let cases: Vec<(
        &str,
        ActionKind,
        Vec<Particle>,
        f32,
        u32,
        f32,
        Vec<(Vec3, Vec3)>,
    )> = vec![
        (
            "move",
            ActionKind::Move,
            standard(),
            d,
            1,
            0.0,
            vec![
                ([1.0165, 1.967, 3.066], w(0)),
                ([-1.901, 0.5, 0.967], w(1)),
                ([0.5, -1.0, -0.5], w(2)),
            ],
        ),
        (
            "gravity",
            ActionKind::Gravity {
                acceleration: [0.0, -5.0, 0.0],
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.5, -1.165, 2.0]),
                (x(1), [3.0, -0.165, -1.0]),
                (x(2), [0.0, -0.165, 0.0]),
            ],
        ),
        (
            "gravitate",
            ActionKind::Gravitate {
                magnitude: 3.0,
                epsilon: 0.001,
                max_radius: 7.0,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.494517, -1.00547, 1.9932]),
                (x(1), [3.01201, -0.00171942, -1.00089]),
                (x(2), [-0.00652487, 0.00719188, 0.00768839]),
            ],
        ),
        (
            "orbitpoint",
            ActionKind::OrbitPoint {
                centre: [0.0; 3],
                magnitude: 1.0,
                epsilon: 0.001,
                max_radius: 1e16,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.49814, -1.00372, 1.99442]),
                (x(1), [3.00875, -0.00218767, -1.00438]),
                (x(2), [-0.00605339, 0.0121068, 0.00605339]),
            ],
        ),
        (
            "orbitpoint, bounded",
            ActionKind::OrbitPoint {
                centre: [0.0, 3.0, 0.0],
                magnitude: 12.0,
                epsilon: 0.01,
                max_radius: 3.4,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.472359, -0.972359, 1.91708]),
                (x(1), [3.05419, 0.0677428, -1.0271]),
                same(2),
            ],
        ),
        (
            "vortex",
            ActionKind::Vortex {
                centre: [0.0, 3.0, 0.0],
                axis: [0.0, 1.0, 0.0],
                magnitude: 70.0,
                epsilon: 50.0,
                max_radius: 170.0,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                ([1.11286, 2.0, 2.95999], w(0)),
                ([-1.96087, 0.5, 1.0747], w(1)),
                ([0.482333, -1.0, -0.517063], w(2)),
            ],
        ),
        (
            "vortex, bounded",
            ActionKind::Vortex {
                centre: [0.0; 3],
                axis: [0.707_106_77, 0.0, 0.707_106_77],
                magnitude: 21.0,
                epsilon: 0.001,
                max_radius: 3.0,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                same(0),
                ([-2.03348, 0.216503, 1.03348], w(1)),
                ([0.76264, -0.58031, -0.76264], w(2)),
            ],
        ),
        (
            "randomaccel, point",
            ActionKind::RandomAccel {
                region: point([0.0, 4.0, 0.0]),
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.5, -0.868, 2.0]),
                (x(1), [3.0, 0.132, -1.0]),
                (x(2), [0.0, 0.132, 0.0]),
            ],
        ),
        (
            "randomaccel, sphere",
            ActionKind::RandomAccel {
                region: sphere_03(),
            },
            standard(),
            d,
            12345,
            0.0,
            vec![
                (x(0), [0.504773, -0.99859, 1.99554]),
                (x(1), [3.00108, 0.000681665, -1.00102]),
                (x(2), [-0.000437152, 0.00215446, 0.00389386]),
            ],
        ),
        (
            "randomdisplace, sphere",
            ActionKind::RandomDisplace {
                region: sphere_03(),
            },
            standard(),
            d,
            12345,
            0.0,
            vec![
                ([1.00477, 2.00141, 2.99554], w(0)),
                ([-1.99892, 0.500682, 0.998975], w(1)),
                ([0.499563, -0.997846, -0.496106], w(2)),
            ],
        ),
        (
            "randomvelocity, point",
            ActionKind::RandomVelocity {
                region: point([1.0, 2.0, 3.0]),
            },
            standard(),
            d,
            1,
            0.0,
            (0..3).map(|i| (x(i), [1.0, 2.0, 3.0])).collect(),
        ),
        (
            "damping",
            ActionKind::Damping {
                factor: [-10.0; 3],
                lower: 1.0,
                upper: 1e32,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.3185, -0.637, 1.274]),
                (x(1), [1.911, 0.0, -0.637]),
                same(2),
            ],
        ),
        (
            "speedlimit",
            ActionKind::SpeedLimit {
                min: -1.0,
                max: 2.0,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.436436, -0.872872, 1.74574]),
                (x(1), [1.89737, 0.0, -0.632456]),
                same(2),
            ],
        ),
        (
            "speedlimit, minimum",
            ActionKind::SpeedLimit { min: 5.0, max: 8.0 },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [1.09109, -2.18218, 4.36436]),
                (x(1), [4.74342, 0.0, -1.58114]),
                same(2),
            ],
        ),
        (
            "jet",
            ActionKind::Jet {
                centre: [0.0, 2.0, 0.0],
                acceleration: point([0.0, 1.0, 0.0]),
                magnitude: 1.0,
                epsilon: 0.001,
                max_radius: 3.0,
            },
            standard(),
            d,
            1,
            0.0,
            vec![same(0), (x(1), [3.0, 0.0045511, -1.0]), same(2)],
        ),
        (
            "turbulence",
            ActionKind::Turbulence {
                frequency: 1.0,
                octaves: 2,
                magnitude: 4.0,
                offset: 0.01,
                drift: [1.0, 1.0, 1.0],
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.542374, -1.02518, 1.97606]),
                (x(1), [3.01198, 0.034908, -0.962697]),
                same(2),
            ],
        ),
        (
            "turbulence, 4 octaves",
            ActionKind::Turbulence {
                frequency: 0.5,
                octaves: 4,
                magnitude: 50.0,
                offset: 0.37,
                drift: [1.0, -3.0, 1.0],
            },
            standard(),
            d,
            1,
            2.0,
            vec![
                (x(0), [1.01284, -1.37607, -1.52662]),
                (x(1), [2.76068, 0.979295, -1.19147]),
                same(2),
            ],
        ),
        (
            "follow",
            ActionKind::Follow {
                magnitude: 1.0,
                epsilon: 0.001,
                max_radius: 1e16,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.498338, -1.00083, 1.99889]),
                (x(1), [3.00234, -0.00140427, -1.0014]),
                same(2),
            ],
        ),
        (
            "scatter",
            ActionKind::Scatter {
                centre: [0.0; 3],
                magnitude: 2000.0,
                epsilon: 22.0,
                max_radius: 3.0,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                same(0),
                (x(1), [0.885889, 0.528528, 0.0570555]),
                (x(2), [1.14657, -2.29314, -1.14657]),
            ],
        ),
        (
            "targetvelocity",
            ActionKind::TargetVelocity {
                velocity: [0.0, 1.0, 0.0],
                scale: 5.0,
            },
            standard(),
            d,
            1,
            0.0,
            vec![
                (x(0), [0.4175, -0.67, 1.67]),
                (x(1), [2.505, 0.165, -0.835]),
                (x(2), [0.0, 0.165, 0.0]),
            ],
        ),
    ];
    let plane = region(
        kind::PLANE,
        [0.0; 3],
        [0.0, 1.0, 0.0],
        [0.0; 3],
        [0.0; 3],
        [0.0; 4],
    );
    let disc = region(
        kind::DISC,
        [0.0; 3],
        [0.0, 1.0, 0.0],
        [0.0; 3],
        [0.0; 3],
        [1.0, 0.25, 0.0, 0.0],
    );
    let triangle = region(
        kind::TRIANGLE,
        [0.0; 3],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 2.0],
        [2.0, 0.0, 0.0],
        [0.0; 4],
    );
    let rectangle = Region {
        kind: kind::RECTANGLE,
        ..triangle
    };
    let sphere = region(
        kind::SPHERE,
        [0.0; 3],
        [0.0; 3],
        [0.0; 3],
        [0.0; 3],
        [1.0, 0.0, 1.0, 0.0],
    );
    let bounce = |surface, tangential, resilience, cutoff| ActionKind::Bounce {
        surface,
        tangential,
        resilience,
        cutoff,
    };
    let b = |i: usize| bounce_b()[i].position;
    let c = |i: usize| sphere_c()[i].position;
    let bounces: Vec<(&str, ActionKind, Vec<Particle>, Vec<(Vec3, Vec3)>)> = vec![
        (
            "bounce, plane",
            bounce(plane, 1.0, 0.45, 1.0),
            bounce_b(),
            vec![
                (b(0), [1.0, 0.45, 0.0]),
                (b(1), [0.0, 0.9, 0.0]),
                (b(2), [0.0, 0.45, 0.0]),
            ],
        ),
        (
            "bounce, disc",
            bounce(disc, 0.8, 0.7, 0.0),
            bounce_b(),
            vec![
                (b(0), [0.8, 0.7, 0.0]),
                (b(1), [0.0, -2.0, 0.0]),
                (b(2), [0.0, -1.0, 0.0]),
            ],
        ),
        (
            "bounce, triangle",
            bounce(triangle, 1.1, 0.3, 0.0),
            bounce_b(),
            vec![
                (b(0), [1.1, 0.3, 0.0]),
                (b(1), [0.0, -2.0, 0.0]),
                (b(2), [0.0, 0.3, 0.0]),
            ],
        ),
        (
            "bounce, rectangle",
            bounce(rectangle, 1.1, 0.3, 0.0),
            bounce_b(),
            vec![
                (b(0), [1.1, 0.3, 0.0]),
                (b(1), [0.0, 0.6, 0.0]),
                (b(2), [0.0, 0.3, 0.0]),
            ],
        ),
        (
            "bounce, sphere",
            bounce(sphere, 0.5, 0.5, 0.0),
            sphere_c(),
            vec![
                (c(0), [1.0, 0.25, 0.0]),
                (c(1), [0.0, 1.0, 0.0]),
                (c(2), [0.0, 0.0, 1.0]),
            ],
        ),
    ];
    for (name, kind, particles, dt, state, time, want) in cases {
        let (got, notices) = step(&kind, particles, dt, state, time);
        assert!(notices.is_empty());
        check(name, &got, &want);
    }
    for (name, kind, particles, want) in bounces {
        let (got, _) = step(&kind, particles, 0.5, 1, 0.0);
        check(name, &got, &want);
    }
    // `move`'s previous positions and ages.
    let (got, _) = step(&ActionKind::Move, standard(), d, 1, 0.0);
    for (p, (q, age)) in got.iter().zip(standard().iter().zip([0.283, 1.033, 0.033])) {
        assert_eq!(p.previous, q.position);
        assert!((p.age - age).abs() < 1e-6);
    }
}

/// The boundary cases of P3 6.1.
#[test]
fn motion_boundaries_are_p3s() {
    let one = |x: Vec3, w: Vec3| vec![particle(x, w, 0.0)];
    // At exactly the maximum radius only `vortex` changes a particle.
    let at = one([3.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let orbit = ActionKind::OrbitPoint {
        centre: [0.0; 3],
        magnitude: 1.0,
        epsilon: 0.1,
        max_radius: 3.0,
    };
    assert_eq!(step(&orbit, at.clone(), 0.033, 1, 0.0).0, at);
    let vortex = ActionKind::Vortex {
        centre: [0.0; 3],
        axis: [0.0, 1.0, 0.0],
        magnitude: 1.0,
        epsilon: 0.1,
        max_radius: 3.0,
    };
    assert_ne!(step(&vortex, at.clone(), 0.033, 1, 0.0).0, at);
    let limit = |min, max, w: Vec3| {
        step(
            &ActionKind::SpeedLimit { min, max },
            one([0.0; 3], w),
            0.033,
            1,
            0.0,
        )
        .0[0]
            .velocity
    };
    assert_eq!(limit(-1.0, 2.0, [0.5, 0.0, 0.0]), [-1.0, 0.0, 0.0]);
    assert_eq!(limit(-1.0, 2.0, [3.0, 0.0, 0.0]), [2.0, 0.0, 0.0]);
    assert_eq!(limit(5.0, 3.0, [0.0, 0.0, 4.0]), [0.0, 0.0, 5.0]);
    let damping = |speed2: f32| {
        let w = [speed2.sqrt(), 0.0, 0.0];
        let a = ActionKind::Damping {
            factor: [0.0; 3],
            lower: 4.0,
            upper: 16.0,
        };
        step(&a, one([0.0; 3], w), 0.033, 1, 0.0).0[0].velocity != w
    };
    assert!(damping(4.0) && damping(16.0) && !damping(3.99) && !damping(16.01));
    let plane = region(
        kind::PLANE,
        [0.0; 3],
        [0.0, 1.0, 0.0],
        [0.0; 3],
        [0.0; 3],
        [0.0; 4],
    );
    let bounce = |tangential, cutoff| {
        let a = ActionKind::Bounce {
            surface: plane,
            tangential,
            resilience: 0.5,
            cutoff,
        };
        step(&a, one([0.0, 0.4, 0.0], [1.0, -1.0, 0.0]), 0.5, 1, 0.0).0[0].velocity
    };
    assert!(close(bounce(1.0, 1.0), [1.0, 0.5, 0.0]));
    assert!(close(bounce(0.3, 0.5), [0.3, 0.5, 0.0]));
    // `follow` with no particle does nothing (our choice, P3 3.13).
    let follow = ActionKind::Follow {
        magnitude: 1.0,
        epsilon: 0.0,
        max_radius: 1e16,
    };
    assert!(step(&follow, Vec::new(), 0.033, 1, 0.0).0.is_empty());
}

fn aged(ages: &[f32]) -> Vec<Particle> {
    ages.iter()
        .map(|&a| particle([0.0; 3], [0.0; 3], a))
        .collect()
}

fn death_indices(notices: &[Notice]) -> Vec<usize> {
    notices
        .iter()
        .map(|n| match n {
            Notice::Death { index, .. } => *index,
            Notice::Birth { .. } => unreachable!(),
        })
        .collect()
}

#[test]
fn killold_is_p4s_v1() {
    let ages = [0.5, 2.0, 3.0, 1.0, 2.5];
    for (switch, deaths, kept) in [
        (0, vec![4, 2, 1], vec![0.5, 1.0]),
        (1, vec![3, 0], vec![2.5, 2.0, 3.0]),
    ] {
        let a = ActionKind::KillOld {
            age_limit: 2.0,
            kill_younger: switch,
        };
        let (got, notices) = step(&a, aged(&ages), 0.033, 1, 0.0);
        assert_eq!(death_indices(&notices), deaths);
        assert_eq!(got.iter().map(|p| p.age).collect::<Vec<_>>(), kept);
    }
    let other = ActionKind::KillOld {
        age_limit: 2.0,
        kill_younger: 2,
    };
    assert_eq!(step(&other, aged(&ages), 0.033, 1, 0.0).0.len(), 5);
}

#[test]
fn sink_is_p4s_v2() {
    let sphere = region(
        kind::SPHERE,
        [0.0; 3],
        [0.0; 3],
        [0.0; 3],
        [0.0; 3],
        [1.0, 0.0, 1.0, 0.0],
    );
    let positions = [
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [0.5, 0.5, 0.5],
        [1.0, 0.0, 0.0],
        [0.0, -3.0, 0.0],
    ];
    let particles: Vec<Particle> = (positions.iter().zip(0..))
        .map(|(x, i)| particle(*x, [0.0; 3], i as f32))
        .collect();
    for (switch, deaths, kept) in [
        (1, vec![3, 2, 0], vec![4.0, 1.0]),
        (0, vec![4, 1], vec![0.0, 3.0, 2.0]),
    ] {
        let a = ActionKind::Sink {
            kill_inside: switch,
            region: sphere,
        };
        let (got, notices) = step(&a, particles.clone(), 0.033, 1, 0.0);
        assert_eq!(death_indices(&notices), deaths);
        assert_eq!(got.iter().map(|p| p.age).collect::<Vec<_>>(), kept);
    }
}

fn coloured(colours: &[u32]) -> Vec<Particle> {
    colours
        .iter()
        .map(|&colour| Particle {
            colour,
            ..Particle::default()
        })
        .collect()
}

fn target_colour(colour: Vec3, alpha: f32, scale: f32, dt: f32, from: &[u32]) -> Vec<u32> {
    let a = ActionKind::TargetColor {
        colour,
        alpha,
        scale,
    };
    let (got, _) = step(&a, coloured(from), dt, 1, 0.0);
    got.iter().map(|p| p.colour).collect()
}

#[test]
fn targetcolor_is_p4s_v3() {
    let from = [0x80FF_4000, 0x0000_0000, 0xFFFF_FFFF];
    assert_eq!(
        target_colour([0.5; 3], 1.0, 2.0, 0.05, &from),
        [0x8CF2_460C, 0x190C_0C0C, 0xFFF2_F2F2]
    );
    assert_eq!(
        target_colour([1.0, 0.0, 0.25], 18.0, 22.0, 0.05, &from),
        [0xFFFF_0046, 0xFFFF_0046, 0xFFFF_002C]
    );
    let mut c = 0xFF00_0000;
    let mut steps = Vec::new();
    for _ in 0..6 {
        c = target_colour([1.0, 0.5, 0.0], 0.0, 3.0, 0.1, &[c])[0];
        steps.push(c);
    }
    assert_eq!(
        steps,
        [
            0xB24C_2600,
            0x7C81_4000,
            0x56A6_5300,
            0x3CC0_6000,
            0x2AD2_6900,
            0x1DDF_6F00
        ]
    );
}

#[test]
fn channel_conversion_is_p4s() {
    // P4 5.3.
    assert_eq!(to_byte(-0.0), 0);
    assert_eq!(to_byte(-3.0), 0);
    assert_eq!(to_byte(255.99), 255);
    assert_eq!(to_byte(300.0), 255);
    assert_eq!(to_byte(2_147_483_648.0), 0);
    assert_eq!(to_byte(4_000_000_000.0), 0);
    assert_eq!(to_byte(f32::INFINITY), 1);
    assert_eq!(to_byte(f32::from_bits(0x7FC0_0000)), 1);
    assert_eq!(to_byte(-2_147_483_648.0), 0);
    assert_eq!(to_byte(f32::NEG_INFINITY), 0);
    assert_eq!(to_byte(f32::from_bits(0xFFC0_0000)), 0);
    // 2³² ≤ z < 2³³: h = 31, (2³¹ + 256 m) / 2³¹.
    assert_eq!(to_byte(4_294_967_296.0), 1);
    // A huge negative with a fraction: h = 0, m ≠ 0.
    assert_eq!(to_byte(f32::from_bits(0xCF00_0001)), 255);
}

#[test]
fn targetsize_is_p4s_v4() {
    let a = ActionKind::TargetSize {
        size: [1.0, 2.0, 0.5],
        scale: [1.0, 2.0, 3.0],
    };
    let sized = |s: Vec3| Particle {
        size: s,
        ..Particle::default()
    };
    let (got, _) = step(
        &a,
        vec![sized([0.01, 0.01, 0.0]), sized([3.0, -1.0, 200.0])],
        0.02,
        1,
        0.0,
    );
    let bits = |p: &Particle| p.size.map(f32::to_bits);
    assert_eq!(bits(&got[0]), [0x3CF4_1F21, 0x3DB7_8034, 0x3CF5_C28F]);
    assert_eq!(bits(&got[1]), [0x403D_70A4, 0xBF61_47AE, 0x433C_07AE]);
}

#[test]
fn targetrotate_is_p4s_v5() {
    let a = ActionKind::TargetRotate {
        rotation: [-11.092_285, 0.0, 0.0],
        scale: 0.03,
    };
    let rotated = |r: Vec3| Particle {
        rotation: r,
        ..Particle::default()
    };
    let starts = [
        [100.0, 0.0, 0.0],
        [-100.0, 0.0, 0.0],
        [5.0, -2.0, 3.0],
        [-5.0, -2.0, 3.0],
    ];
    let want: [[[u32; 3]; 3]; 4] = [
        [
            [0x42C7_E4B0, 0, 0],
            [0x42C7_C964, 0, 0],
            [0x42C7_AE1C, 0, 0],
        ],
        [
            [0xC2C7_E4B0, 0, 0],
            [0xC2C7_C964, 0, 0],
            [0xC2C7_AE1C, 0, 0],
        ],
        [
            [0x40A0_1DF2, 0xC000_13A9, 0x403F_E282],
            [0x40A0_3BDF, 0xC000_2755, 0x403F_C509],
            [0x40A0_59C8, 0xC000_3B04, 0x403F_A794],
        ],
        [
            [0xC0A0_1DF2, 0xBFFF_D8AE, 0x4040_1D7E],
            [0xC0A0_3BDF, 0xBFFF_B162, 0x4040_3B00],
            [0xC0A0_59C8, 0xBFFF_8A1C, 0x4040_5887],
        ],
    ];
    for (start, want) in starts.iter().zip(want) {
        let mut p = vec![rotated(*start)];
        for w in want {
            p = step(&a, p, 0.02, 1, 0.0).0;
            assert_eq!(p[0].rotation.map(f32::to_bits), w, "{start:?}");
        }
    }
    // P4 7.2: the sign of rₓ picks the factor for y and z too.
    let half = ActionKind::TargetRotate {
        rotation: [0.0; 3],
        scale: 0.5,
    };
    let (got, _) = step(
        &half,
        vec![rotated([1.0, -1.0, 0.0]), rotated([-1.0, -1.0, 2.0])],
        1.0,
        1,
        0.0,
    );
    assert_eq!(got[0].rotation, [0.5, -1.5, 0.0]);
    assert_eq!(got[1].rotation, [-0.5, -0.5, 3.0]);
}
