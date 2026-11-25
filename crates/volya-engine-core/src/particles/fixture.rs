// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Test records written from the specs' own vectors.

use volya_engine_formats::particles::{
    Action, ActionKind, Collision, Effect, Frames, MINUS_HALF_PI, Region, Source, region_kind,
};

/// A region of `kind` with the fields given.
pub fn region(
    kind: u32,
    p1: [f32; 3],
    p2: [f32; 3],
    u: [f32; 3],
    v: [f32; 3],
    r: [f32; 4],
) -> Region {
    Region {
        kind,
        p1,
        p2,
        u,
        v,
        r,
    }
}

pub fn point(p: [f32; 3]) -> Region {
    region(
        region_kind::POINT,
        p,
        [0.0; 3],
        [0.0; 3],
        [0.0; 3],
        [0.0; 4],
    )
}

/// "sphere 0.3" of P3 6.1.
pub fn sphere_03() -> Region {
    region(
        region_kind::SPHERE,
        [0.0; 3],
        [0.0; 3],
        [0.0; 3],
        [0.0; 3],
        [0.3, 0.0, 0.09, 0.0],
    )
}

pub fn action(type_id: u32, flags: u32, kind: ActionKind) -> Action {
    Action {
        type_id,
        flags,
        kind,
    }
}

/// `tests\muhi`'s `source` (P1 8).
pub fn muhi_source() -> Source {
    Source {
        position: region(
            region_kind::DISC,
            [0.0; 3],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            [1.0, 0.0, 0.0, 0.0],
        ),
        velocity: region(
            region_kind::CONE,
            [0.0; 3],
            [0.0, 0.1, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            [0.3, 0.0, 0.09, 99.999_99],
        ),
        rotation: region(
            region_kind::LINE,
            [-137.0, -1.0, -1.0],
            [274.0, 2.0, 2.0],
            [0.0; 3],
            [0.0; 3],
            [0.0; 4],
        ),
        size: point([0.01, 0.01, 0.0]),
        colour: point([0.5, 0.5, 0.5]),
        alpha: 1.0,
        rate: 17.0,
        age: 0.0,
        age_spread: 0.0,
        parent_velocity: [0.0; 3],
        parent_motion: 0.0,
    }
}

pub fn orbit(centre: [f32; 3]) -> ActionKind {
    ActionKind::OrbitPoint {
        centre,
        magnitude: 1.0,
        epsilon: 0.001,
        max_radius: 1.0e16,
    }
}

pub fn muhi_vortex() -> ActionKind {
    ActionKind::Vortex {
        centre: [0.0, 3.0, 0.0],
        axis: [0.0, 1.0, 0.0],
        magnitude: 70.0,
        epsilon: 50.0,
        max_radius: 170.0,
    }
}

/// `tests\muhi` (P1 8) with every action record.
pub fn muhi() -> Effect {
    Effect {
        id: 934,
        name: "tests\\muhi".into(),
        max_particles: 7,
        flags: 0x1401,
        actions: vec![
            action(21, 0x8000_0002, ActionKind::Source(Box::new(muhi_source()))),
            action(
                25,
                0,
                ActionKind::TargetRotate {
                    rotation: [-11.092_285, 0.0, 0.0],
                    scale: 0.03,
                },
            ),
            action(14, 2, orbit([0.0; 3])),
            action(14, 2, orbit([0.3, 0.0, 0.3])),
            action(
                16,
                2,
                ActionKind::RandomDisplace {
                    region: sphere_03(),
                },
            ),
            action(29, 2, muhi_vortex()),
            action(
                10,
                0,
                ActionKind::KillOld {
                    age_limit: 2.0,
                    kill_younger: 0,
                },
            ),
            action(12, 0, ActionKind::Move),
        ],
        sprite: None,
        frames: Frames {
            width: 0.5,
            height: 0.5,
            per_row: 2,
            count: 4,
            rate: 24.0,
        },
        time_limit: 0.0,
        collision: Collision {
            tangential: 1.0,
            resilience: 0.0,
            cutoff: 0.0,
        },
        velocity_scale: [0.0; 3],
        alignment: [MINUS_HALF_PI, 0.0, 0.0],
    }
}

/// An effect with `actions`, room for `max` particles, `flags` and a time
/// limit.
pub fn effect(name: &str, max: u32, flags: u32, time_limit: f32, actions: Vec<Action>) -> Effect {
    Effect {
        name: name.into(),
        max_particles: max,
        flags,
        actions,
        time_limit,
        ..muhi()
    }
}
