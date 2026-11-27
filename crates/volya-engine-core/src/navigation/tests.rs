// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Routes on synthetic worlds, planned and then followed by the real
//! movement (player-movement.md, climbing.md).

// Numbers are written as the data writes them (`rot_speed 3.14`).
#![allow(clippy::approx_constant)]

use super::follow::{Follow, Follower};
use super::*;
use crate::camera::Angles;
use crate::climbing::Ladder;
use crate::collision::body::BodyConfig;
use crate::collision::geometry::{cross, dot};
use crate::collision::test_world::Builder;
use crate::movement::{Bobbing, Player, PlayerState, Request, Settings};

/// The shipped values (player-movement.md section 12).
fn config() -> MovementConfig {
    MovementConfig {
        walk_accel: 17.0,
        run_coef: 2.1,
        run_back_coef: 1.8,
        walk_back_coef: 0.8,
        crouch_coef: 0.45,
        climb_coef: 0.7,
        sprint_koef: 2.6,
        walk_strafe_coef: 1.0,
        run_strafe_coef: 1.0,
        jump_speed: 6.0,
        air_control_param: 0.1,
        camera_height_factor: 0.85,
        crash_speed_min: 14.0,
        crash_speed_max: 30.0,
        body: BodyConfig {
            boxes: [[0.35, 0.9, 0.35], [0.35, 0.75, 0.35], [0.35, 0.6, 0.35]],
            mass: 80.0,
        },
        lim_pitch: [-1.5, 1.5],
        rot_speed: [3.14, 3.14, 0.0],
        bobbing: Bobbing {
            run_amplitude: 0.0075,
            run_speed: 10.0,
            walk_amplitude: 0.005,
            walk_speed: 7.0,
            limp_amplitude: 0.011,
            limp_speed: 6.0,
        },
    }
}

fn body() -> Body {
    Body::of(&config())
}

/// An axis-aligned box from `lo` to `hi`, its faces facing out.
fn solid(b: &mut Builder, lo: Vec3, hi: Vec3) {
    let c = [
        0.5 * (lo[0] + hi[0]),
        0.5 * (lo[1] + hi[1]),
        0.5 * (lo[2] + hi[2]),
    ];
    for k in 0..3 {
        let (i, j) = ((k + 1) % 3, (k + 2) % 3);
        for side in [lo[k], hi[k]] {
            let at = |a: f32, b: f32| {
                let mut p = [0.0; 3];
                p[k] = side;
                p[i] = a;
                p[j] = b;
                p
            };
            let mut q = [
                at(lo[i], lo[j]),
                at(hi[i], lo[j]),
                at(hi[i], hi[j]),
                at(lo[i], hi[j]),
            ];
            let n = cross(sub(q[1], q[0]), sub(q[2], q[1]));
            let out = sub(q[0], c);
            if dot(n, out) < 0.0 {
                q.reverse();
            }
            b.quad(q, 1);
        }
    }
}

fn open_floor() -> Builder {
    let mut b = Builder::default();
    b.floor([-20.0, 20.0], [-20.0, 20.0], 0.0, true);
    b
}

/// A corridor along z, 2 m wide (x from −1 to 1), 3 m high walls.
fn corridor() -> Builder {
    let mut b = Builder::default();
    b.floor([-1.0, 1.0], [-10.0, 10.0], 0.0, true);
    solid(&mut b, [-1.5, 0.0, -10.0], [-1.0, 3.0, 10.0]);
    solid(&mut b, [1.0, 0.0, -10.0], [1.5, 3.0, 10.0]);
    b
}

fn kinds(r: &Route) -> Vec<Kind> {
    r.waypoints.iter().map(|w| w.kind).collect()
}

/// Follows `route` with a player standing at `from` facing +z; frames of
/// 16 ms for at most `seconds`. Returns how it ended and the player.
fn follow(world: &CollisionWorld, from: Vec3, route: Route, seconds: f32) -> (Follow, PlayerState) {
    let settings = Settings::read(&[b"g_always_run on\ncam_inert 0.\n"]);
    let mut p = Player::new(config(), settings, from, Angles::default(), 0);
    let mut now = 0;
    for _ in 0..20 {
        now += 16;
        p.frame(world, now, &Request::default());
    }
    let mut f = Follower::new(route, now);
    for _ in 0..(seconds / 0.016) as u32 {
        now += 16;
        let s = p.state();
        let steer = match f.frame(&s, now) {
            Follow::Steer(steer) => steer,
            end => return (end, s),
        };
        let mut r = Request::default();
        f.apply(&steer, &s, true, 0.016, &mut r);
        p.frame(world, now, &r);
    }
    (Follow::Stuck, p.state())
}

fn assert_arrives(world: &CollisionWorld, from: Vec3, to: Vec3, seconds: f32) -> Route {
    let route = plan(world, &body(), from, to).expect("a route");
    assert!(route.complete, "{route:?}");
    let (end, s) = follow(world, from, route.clone(), seconds);
    assert_eq!(end, Follow::Done, "{s:?} {:?}", kinds(&route));
    let last = route.waypoints.last().unwrap().at;
    assert!(horizontal_distance(s.position, to) < 0.8, "{s:?} {last:?}");
    route
}

#[test]
fn the_body_from_the_movement_values() {
    let b = body();
    assert_eq!(b.heights, [1.8, 1.5, 1.2]);
    assert!((b.speeds[0] - 3.57).abs() < 1e-3 && (b.speeds[2] - 0.765).abs() < 1e-3);
    assert!((b.apex - 0.9174).abs() < 1e-3, "{b:?}");
    assert!((b.climb_speed - 2.499).abs() < 1e-3);
}

#[test]
fn an_open_floor_is_one_straight_leg() {
    let world = open_floor().world();
    let r = plan(&world, &body(), [0.0, 0.0, 0.0], [3.0, 0.0, 8.0]).unwrap();
    assert!(r.complete);
    assert_eq!(kinds(&r), [Kind::Walk, Kind::Walk], "{r:?}");
    assert_arrives(&world, [0.0; 3], [3.0, 0.0, 8.0], 5.0);
}

#[test]
fn goes_around_a_wall() {
    let mut b = open_floor();
    solid(&mut b, [-4.0, 0.0, 2.0], [4.0, 3.0, 2.3]);
    let world = b.world();
    let r = assert_arrives(&world, [0.0; 3], [0.0, 0.0, 5.0], 8.0);
    assert!(r.waypoints.iter().all(|w| w.kind == Kind::Walk));
    // Round an end of the wall.
    assert!(r.waypoints.iter().any(|w| w.at[0].abs() > 4.0), "{r:?}");
    assert!(r.cost < 12.0 / 3.57, "{}", r.cost);
}

/// A movable body's box (a locked door's leaf, trader-door.md) is a wall.
#[test]
fn goes_around_a_movable_body() {
    let world = open_floor().world();
    let leaf = BodyBox {
        centre: [0.0, 1.5, 2.15],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        half_size: [4.0, 1.5, 0.15],
        material: 0,
    };
    let (from, to) = ([0.0; 3], [0.0, 0.0, 5.0]);
    let straight = plan(&world, &body(), from, to).unwrap();
    assert!(
        straight.waypoints.iter().all(|w| w.at[0].abs() < 1.0),
        "{straight:?}"
    );
    let r = plan_among(&world, &body(), &[leaf], from, to).unwrap();
    assert!(r.complete, "{r:?}");
    assert!(r.waypoints.iter().any(|w| w.at[0].abs() > 4.0), "{r:?}");
}

#[test]
fn jumps_up_onto_a_box_across_the_way() {
    let mut b = corridor();
    solid(&mut b, [-1.0, 0.0, 3.0], [1.0, 0.65, 4.5]);
    let world = b.world();
    let r = assert_arrives(&world, [0.0; 3], [0.0, 0.0, 7.0], 8.0);
    assert!(kinds(&r).contains(&Kind::JumpUp), "{r:?}");
}

#[test]
fn jumps_over_a_low_wall_across_the_way() {
    let mut b = corridor();
    solid(&mut b, [-1.0, 0.0, 3.0], [1.0, 0.55, 3.1]);
    let world = b.world();
    let r = assert_arrives(&world, [0.0; 3], [0.0, 0.0, 7.0], 8.0);
    assert!(kinds(&r).contains(&Kind::JumpOver), "{r:?}");
}

#[test]
fn jumps_across_a_gap() {
    let mut b = Builder::default();
    b.floor([-1.0, 1.0], [-10.0, 3.0], 0.0, true);
    b.floor([-1.0, 1.0], [3.9, 10.0], 0.0, true);
    b.floor([-1.0, 1.0], [3.0, 3.9], -6.0, true);
    solid(&mut b, [-1.5, -6.0, -10.0], [-1.0, 3.0, 10.0]);
    solid(&mut b, [1.0, -6.0, -10.0], [1.5, 3.0, 10.0]);
    let world = b.world();
    let r = assert_arrives(&world, [0.0; 3], [0.0, 0.0, 7.0], 8.0);
    assert!(kinds(&r).contains(&Kind::JumpOver), "{r:?}");
}

#[test]
fn crawls_under_a_low_ceiling() {
    let mut b = corridor();
    // A slab 1.3 m over the floor: only the low crouch fits.
    solid(&mut b, [-1.0, 1.3, 3.0], [1.0, 3.0, 5.0]);
    let world = b.world();
    let r = assert_arrives(&world, [0.0; 3], [0.0, 0.0, 8.0], 15.0);
    let k = kinds(&r);
    assert!(k.contains(&Kind::Crawl), "{r:?}");
    assert_eq!(k.last(), Some(&Kind::Walk), "stands up after: {r:?}");
}

#[test]
fn crouches_under_a_lower_ceiling() {
    let mut b = corridor();
    solid(&mut b, [-1.0, 1.6, 3.0], [1.0, 3.0, 5.0]);
    let world = b.world();
    let r = assert_arrives(&world, [0.0; 3], [0.0, 0.0, 8.0], 10.0);
    assert!(kinds(&r).contains(&Kind::Crouch), "{r:?}");
    assert!(!kinds(&r).contains(&Kind::Crawl), "{r:?}");
}

/// A floor, a thin upright ladder facing +z from (0, 0, 0) to (0, 4, 0)
/// and a platform at its top behind it (as the movement's tests).
fn ladder_world() -> CollisionWorld {
    let mut b = Builder::default();
    b.floor([-20.0, 20.0], [0.0, 20.0], 0.0, true);
    b.floor([-20.0, 20.0], [-20.0, 0.0], 4.0, true);
    // The platform's front face.
    let q = [
        [-20.0, 0.0, 0.0],
        [-20.0, 4.0, 0.0],
        [20.0, 4.0, 0.0],
        [20.0, 0.0, 0.0],
    ];
    b.quad(q, 1);
    let mut w = b.world();
    w.set_ladders(vec![Ladder::new(
        "test",
        [0.0, 2.0, 0.0],
        [0.0; 3],
        [0.4, 2.0, 0.02],
    )])
    .unwrap();
    w
}

#[test]
fn climbs_up_a_ladder() {
    let world = ladder_world();
    let r = assert_arrives(&world, [1.0, 0.0, 3.0], [-2.0, 4.0, -3.0], 12.0);
    assert!(kinds(&r).contains(&Kind::ClimbUp), "{r:?}");
}

#[test]
fn climbs_down_a_ladder() {
    let world = ladder_world();
    let r = assert_arrives(&world, [-2.0, 4.0, -3.0], [1.0, 0.0, 3.0], 12.0);
    assert!(kinds(&r).contains(&Kind::ClimbDown), "{r:?}");
}

#[test]
fn an_unreachable_target_gets_the_nearest_floor() {
    let mut b = open_floor();
    // A pillar too tall to jump onto, its top the target.
    solid(&mut b, [2.0, 0.0, 2.0], [3.0, 3.0, 3.0]);
    let world = b.world();
    let r = plan(&world, &body(), [0.0; 3], [2.5, 3.0, 2.5]).unwrap();
    assert!(!r.complete, "{r:?}");
    let end = r.waypoints.last().unwrap().at;
    assert!(
        end[1].abs() < 0.01 && horizontal_distance(end, [2.5, 0.0, 2.5]) < 1.1,
        "{end:?}"
    );
    // A target walled in: no floor near it is reached, so no route (the
    // trip runs straight); one on the wall's top, the floor at its foot.
    let mut b = open_floor();
    for (lo, hi) in [
        ([4.0, 0.0, 4.0], [8.0, 3.0, 4.3]),
        ([4.0, 0.0, 7.7], [8.0, 3.0, 8.0]),
        ([4.0, 0.0, 4.0], [4.3, 3.0, 8.0]),
        ([7.7, 0.0, 4.0], [8.0, 3.0, 8.0]),
    ] {
        solid(&mut b, lo, hi);
    }
    let world = b.world();
    assert_eq!(plan(&world, &body(), [0.0; 3], [6.0, 0.0, 6.0]), None);
    let r = plan(&world, &body(), [0.0; 3], [6.0, 3.0, 4.15]).unwrap();
    assert!(!r.complete);
    let end = r.waypoints.last().unwrap().at;
    assert!(
        end[2] < 4.0 && horizontal_distance(end, [6.0, 0.0, 4.15]) < 1.5,
        "{end:?} {:?}",
        r.stats
    );
    // A start with no floor: none.
    assert!(plan(&world, &body(), [0.0, 10.0, 0.0], [1.0, 0.0, 1.0]).is_none());
}

#[test]
fn a_search_spreads_over_parts_to_the_same_route() {
    let mut b = open_floor();
    solid(&mut b, [-4.0, 0.0, 2.0], [4.0, 3.0, 2.3]);
    let world = b.world();
    let whole = plan(&world, &body(), [0.0; 3], [0.0, 0.0, 5.0]).unwrap();
    let mut p = Planning::new(&world, &body(), [0.0; 3], [0.0, 0.0, 5.0]).unwrap();
    let mut parts = 1;
    let route = loop {
        if let Some(r) = p.run(&world, 10) {
            break r.unwrap();
        }
        parts += 1;
    };
    assert!(parts > 3, "{parts}");
    assert_eq!(route.waypoints, whole.waypoints);
}

fn straight(points: &[(Vec3, Kind)]) -> Route {
    Route {
        waypoints: points
            .iter()
            .map(|&(at, kind)| Waypoint {
                at,
                kind,
                ladder: None,
            })
            .collect(),
        cost: 0.0,
        complete: true,
        stats: Stats::default(),
    }
}

/// The body's limits hold in the movement, with margin: a ledge a little
/// over the step is walked up (measured up to 0.7 m), one at the jump's
/// height jumped up onto, from a run and from standing before it (measured
/// up to 0.9 m), and a gap a little over the jump's length jumped across
/// (measured 1.8 m).
#[test]
fn the_limits_hold_in_the_movement() {
    let b = body();
    let ledge = |h: f32| {
        let mut w = corridor();
        solid(&mut w, [-1.0, 0.0, 3.0], [1.0, h, 10.0]);
        w.world()
    };
    let h = b.step + 0.1;
    let route = straight(&[([0.0; 3], Kind::Walk), ([0.0, h, 5.0], Kind::Walk)]);
    let (end, s) = follow(&ledge(h), [0.0; 3], route, 5.0);
    assert_eq!(end, Follow::Done, "walked up {h}: {s:?}");
    let h = b.jump_up;
    for from in [0.0, 2.6] {
        let route = straight(&[
            ([0.0, 0.0, from], Kind::Walk),
            ([0.0, 0.0, 2.7], Kind::Walk),
            ([0.0, h, 3.3], Kind::JumpUp),
            ([0.0, h, 5.0], Kind::Walk),
        ]);
        let (end, s) = follow(&ledge(h), [0.0, 0.0, from], route, 5.0);
        assert_eq!(end, Follow::Done, "jumped up {h} from {from}: {s:?}");
    }
    let gap = b.jump_length + 0.1;
    let mut w = Builder::default();
    w.floor([-1.0, 1.0], [-10.0, 3.0], 0.0, true);
    w.floor([-1.0, 1.0], [3.0 + gap, 10.0], 0.0, true);
    w.floor([-1.0, 1.0], [3.0, 3.0 + gap], -6.0, true);
    solid(&mut w, [-1.5, -6.0, -10.0], [-1.0, 3.0, 10.0]);
    solid(&mut w, [1.0, -6.0, -10.0], [1.5, 3.0, 10.0]);
    let world = w.world();
    let route = straight(&[
        ([0.0; 3], Kind::Walk),
        ([0.0, 0.0, 3.0], Kind::Walk),
        ([0.0, 0.0, 3.0 + gap + 0.3], Kind::JumpOver),
        ([0.0, 0.0, 7.0], Kind::Walk),
    ]);
    let (end, s) = follow(&world, [0.0; 3], route, 5.0);
    assert_eq!(end, Follow::Done, "jumped across {gap}: {s:?}");
    // A thin wall a little higher than a jump clears: up onto it and off.
    let mut w = corridor();
    solid(&mut w, [-1.0, 0.0, 3.0], [1.0, b.jump_over + 0.05, 3.1]);
    let world = w.world();
    let r = assert_arrives(&world, [0.0; 3], [0.0, 0.0, 7.0], 8.0);
    assert!(kinds(&r).contains(&Kind::JumpUp), "{r:?}");
}
