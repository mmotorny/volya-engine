// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Tap-to-move's routes on `l01_escape` (BYOA; our design,
//! docs/architecture/simulation.md, Navigation): targets around the start,
//! from just past the trader's locked door, whose straight line is blocked are planned and then followed by the
//! player's own movement on the level's collision world, and one of the
//! nearby ladders is climbed; the searches' cost is reported. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::Instant;

use volya_engine::core::collision::CollisionWorld;
use volya_engine::core::collision::geometry::{Vec3, length, sub};
use volya_engine::core::input::Input;
use volya_engine::core::movement::{Player, Request};
use volya_engine::core::navigation::follow::{Follow, Follower};
use volya_engine::core::navigation::{Body, Kind, Route, plan_among};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn horizontal(a: Vec3, b: Vec3) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// The route's length along its waypoints, horizontally.
fn route_length(r: &Route) -> f32 {
    r.waypoints
        .windows(2)
        .map(|w| horizontal(w[0].at, w[1].at))
        .sum()
}

/// Follows `route` with `player` for at most `seconds`: how it ended and
/// where the feet are.
fn follow(
    world: &CollisionWorld,
    mut player: Player,
    route: Route,
    seconds: f32,
) -> (Follow, Vec3) {
    let mut now = 1_000_000;
    let mut f = Follower::new(route, now);
    for _ in 0..(seconds / 0.016) as u32 {
        now += 16;
        let s = player.state();
        let steer = match f.frame(&s, now) {
            Follow::Steer(steer) => steer,
            end => return (end, s.position),
        };
        let mut r = Request::default();
        f.apply(&steer, &s, player.settings.always_run, 0.016, &mut r);
        player.frame(world, now, &r);
    }
    (Follow::Stuck, player.state().position)
}

#[test]
fn routes_on_the_start_level() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();
    // The trader's door is locked at the start (trader-door.md, #874): the
    // routes start 1 m past its leaf, on the side away from the start.
    let leaf = level.player().body().bodies()[0];
    let start = level.player().state().position;
    let k = (0..3)
        .min_by(|&a, &b| leaf.half_size[a].total_cmp(&leaf.half_size[b]))
        .unwrap();
    let n = leaf.axes[k];
    let side = n[0] * (start[0] - leaf.centre[0]) + n[2] * (start[2] - leaf.centre[2]);
    let out = -side.signum();
    let (x, z) = (leaf.centre[0] + out * n[0], leaf.centre[2] + out * n[2]);
    let (y, _) = (level.collision())
        .floor_below(x, z, start[1] + 1.0, start[1] - 2.0)
        .expect("a floor past the door");
    let p = level.player();
    let h = p.config.eye_height(p.body().box_index());
    let angles = p.angles();
    level.set_camera([x, y + h, z], angles);
    let input = Input::new();
    let mut now = 0;
    for _ in 0..30 {
        now += 16;
        level.set_global_clock(now);
        level.update(now, &input);
    }
    let world = level.collision();
    let player = level.player().clone();
    let body = Body::of(&player.config);
    // The door is in the way.
    let walls = player.body().bodies();
    let from = player.state().position;
    eprintln!("start {from:?}");

    // Floors on rings around the start whose straight line from the eye's
    // height is blocked.
    let mut around = Vec::new();
    let mut times = Vec::new();
    let mut most = (0, 0, 0);
    for radius in [8.0f32, 15.0, 25.0] {
        for k in 0..24 {
            let a = k as f32 * std::f32::consts::TAU / 24.0;
            let (x, z) = (from[0] + radius * a.sin(), from[2] + radius * a.cos());
            let Some((y, _)) = world.floor_below(x, z, from[1] + 10.0, from[1] - 10.0) else {
                continue;
            };
            let target = [x, y, z];
            let eye = [from[0], from[1] + 1.5, from[2]];
            let to = [x, y + 1.0, z];
            let d = sub(to, eye);
            let dir = d.map(|v| v / length(d));
            let blocked = world.ray_hits(eye, dir, length(d));
            let t = Instant::now();
            let route = plan_among(world, &body, walls, from, target);
            let ms = t.elapsed().as_secs_f64() * 1e3;
            times.push(ms);
            eprintln!(
                "  {target:?} blocked {blocked}: {ms:.1} ms {:?}",
                route
                    .as_ref()
                    .map(|r| (r.complete, r.stats, r.waypoints.len()))
            );
            if let Some(r) = &route {
                let s = r.stats;
                if s.bytes > most.2 {
                    most = (s.expanded, s.columns, s.bytes);
                }
                if blocked && r.complete {
                    around.push((target, r.clone()));
                }
            }
        }
    }
    times.sort_by(f64::total_cmp);
    eprintln!(
        "{} searches: median {:.1} ms, most {:.1} ms; the largest: {} expanded, {} columns, {} bytes",
        times.len(),
        times[times.len() / 2],
        times.last().unwrap(),
        most.0,
        most.1,
        most.2
    );
    eprintln!("{} blocked targets reached", around.len());
    assert!(around.len() >= 3, "{}", around.len());
    // The ones whose route most exceeds the straight line, followed.
    around.sort_by(|a, b| {
        let excess = |(t, r): &(Vec3, Route)| route_length(r) / horizontal(from, *t);
        excess(b).total_cmp(&excess(a))
    });
    for (target, route) in around.iter().take(4) {
        let straight = horizontal(from, *target);
        let kinds: Vec<Kind> = route.waypoints.iter().map(|w| w.kind).collect();
        eprintln!(
            "to {target:?}: {:.1} m along the route against {straight:.1} m straight, {:.1} s planned, {kinds:?}",
            route_length(route),
            route.cost
        );
        let (end, at) = follow(world, player.clone(), route.clone(), 40.0);
        let last = route.waypoints.last().unwrap().at;
        eprintln!(
            "  {end:?} at {at:?}, {:.2} m from the route's end",
            horizontal(at, last)
        );
        assert_eq!(end, Follow::Done, "to {target:?}: at {at:?}");
    }
    assert!(
        route_length(&around[0].1) > 1.3 * horizontal(from, around[0].0),
        "a route goes around something"
    );

    // Up the nearest ladder whose top has a floor beyond it.
    let mut ladders: Vec<_> = world.ladders().iter().collect();
    ladders.sort_by(|a, b| horizontal(a.lower, from).total_cmp(&horizontal(b.lower, from)));
    let mut climbed = false;
    for l in ladders.iter().take(4) {
        let n = l.normal_dir;
        let h = n[0].hypot(n[2]);
        let out = [n[0] / h, 0.0, n[2] / h];
        let beyond = [l.upper[0] - out[0] * 1.2, l.upper[2] - out[2] * 1.2];
        let Some((y, _)) =
            world.floor_below(beyond[0], beyond[1], l.upper[1] + 1.0, l.upper[1] - 1.0)
        else {
            continue;
        };
        let target = [beyond[0], y, beyond[1]];
        let t = Instant::now();
        let Some(route) = plan_among(world, &body, walls, from, target) else {
            continue;
        };
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let kinds: Vec<Kind> = route.waypoints.iter().map(|w| w.kind).collect();
        eprintln!(
            "{}: {:.1} m from the start, {ms:.1} ms, {:?}, complete {}, {kinds:?}",
            l.name,
            horizontal(l.lower, from),
            route.stats,
            route.complete
        );
        if !route.complete || !kinds.contains(&Kind::ClimbUp) {
            continue;
        }
        let (end, at) = follow(world, player.clone(), route, 60.0);
        eprintln!("  {end:?} at {at:?}");
        assert_eq!(end, Follow::Done, "{}: at {at:?}", l.name);
        assert!((at[1] - y).abs() < 0.5, "{}: at {at:?}", l.name);
        climbed = true;
        break;
    }
    assert!(climbed, "no ladder near the start climbed");
}
