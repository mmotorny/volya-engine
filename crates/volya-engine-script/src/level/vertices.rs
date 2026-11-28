// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Level vertices (docs/behavior/script-api-level.md section 3): the walk
//! of `level.vertex_in_direction` (3.3) and the cover interpolation of
//! `level.cover_in_direction` (3.4), over a level's `level.ai` positions,
//! links and cover values (3.1).

use std::f32::consts::{FRAC_2_PI, FRAC_PI_2, PI};

use volya_engine_formats::level_ai::{LevelAi, LevelAiLinks};

type Vec3 = [f32; 3];

/// The slack of the walk's comparisons (3.3).
const SLACK: f32 = 0.001;

/// The heading's threshold (3.4).
const EPSILON: f32 = 1e-7;

/// The x–z distance from `a` to `b`.
fn distance_xz(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    (dx * dx + dz * dz).sqrt()
}

fn xz(p: Vec3) -> [f32; 2] {
    [p[0], p[2]]
}

/// Where the segment from `a` to `b` first meets the square of side
/// 2 · `half` centred on `centre` in the x–z plane, if it does.
fn crossing(a: Vec3, b: Vec3, centre: Vec3, half: f32) -> Option<[f32; 2]> {
    let (ax, az) = (a[0], a[2]);
    let (dx, dz) = (b[0] - ax, b[2] - az);
    let (mut enter, mut leave) = (0.0f32, 1.0f32);
    let sides = [
        (-dx, ax - (centre[0] - half)),
        (dx, centre[0] + half - ax),
        (-dz, az - (centre[2] - half)),
        (dz, centre[2] + half - az),
    ];
    for (p, q) in sides {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                enter = enter.max(r);
            } else {
                leave = leave.min(r);
            }
        }
    }
    (enter <= leave).then_some([ax + enter * dx, az + enter * dz])
}

/// `level.vertex_in_direction(vertex, direction, distance)` (3.3): the
/// vertex where the straight line from `vertex` toward the target leaves
/// the connected area of the level graph, or the target's vertex when the
/// line reaches it. `None` when `vertex` is not a vertex of the level.
///
/// The crossing point of a neighbour is where the segment from the start
/// to the target enters its cell (our reading: the spec names the cells
/// the segment crosses and their crossing points; edge and corner cases
/// are believed to follow the same rule). The walk never visits more
/// vertices than the level has (our guard).
pub fn vertex_in_direction(
    ai: &LevelAi,
    links: &LevelAiLinks,
    vertex: u32,
    direction: Vec3,
    distance: f32,
) -> Option<u32> {
    let n = ai.len().min(links.len()) as u32;
    if vertex >= n {
        return None;
    }
    let start = ai.position(vertex)?;
    // 1: normalised unless its squared length is tiny.
    let square =
        direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2];
    let d = if square > f32::MIN_POSITIVE {
        let length = square.sqrt();
        direction.map(|v| v / length)
    } else {
        direction
    };
    // 2.
    let target = [
        start[0] + d[0] * distance,
        start[1] + d[1] * distance,
        start[2] + d[2] * distance,
    ];
    let span = {
        let (x, y, z) = (
            target[0] - start[0],
            target[1] - start[1],
            target[2] - start[2],
        );
        (x * x + y * y + z * z).sqrt()
    };
    let half = ai.header.cell_size / 2.0;
    let target_cell = ai.packed_at(target);
    // 3.
    let mut current = vertex;
    let mut came_from = None;
    for _ in 0..n {
        if target_cell.is_some() && target_cell == ai.packed(current) {
            break;
        }
        // The first crossing is compared with the start; a later one
        // replaces the best when less than the slack farther from the
        // target.
        let mut best = None;
        let mut best_distance = distance_xz(xz(start), xz(target));
        for link in links.links(current)? {
            if link >= n || Some(link) == came_from {
                continue;
            }
            let Some(centre) = ai.position(link) else {
                continue;
            };
            let Some(point) = crossing(start, target, centre, half) else {
                continue;
            };
            let to_target = distance_xz(point, xz(target));
            if to_target < best_distance + SLACK {
                best = Some((link, point));
                best_distance = to_target;
            }
        }
        let Some((next, point)) = best else {
            break;
        };
        came_from = Some(current);
        current = next;
        if distance_xz(xz(start), point) >= span + SLACK {
            break;
        }
    }
    // 4.
    Some(current)
}

/// `level.cover_in_direction(vertex, direction)` (3.4) for a vertex's
/// cover values `cover` (*c*0 … *c*3): each divided by 15, interpolated by
/// the heading of `direction`'s x and z. The heading's arctangent is taken
/// in double precision and rounded to single (our choice: the review
/// disputes which precision it has); the rest is single precision.
pub fn cover_in_direction(cover: [u8; 4], direction: Vec3) -> f32 {
    let (x, z) = (direction[0], direction[2]);
    let a: f32 = if x.abs() < EPSILON && z.abs() < EPSILON {
        0.0
    } else if z.abs() < EPSILON {
        if x > 0.0 { -FRAC_PI_2 } else { FRAC_PI_2 }
    } else if z < 0.0 {
        (std::f64::consts::PI - f64::from(x / z).atan()) as f32
    } else {
        (-f64::from(x / z).atan()) as f32
    };
    let n = cover.map(|c| f32::from(c) / 15.0);
    if a < FRAC_PI_2 {
        n[1] + (n[0] - n[1]) * a * FRAC_2_PI
    } else if a < PI {
        n[0] + (n[3] - n[0]) * (a - FRAC_PI_2) * FRAC_2_PI
    } else if a < 3.0 * FRAC_PI_2 {
        n[3] + (n[2] - n[3]) * (a - PI) * FRAC_2_PI
    } else {
        n[2] + (n[1] - n[2]) * (a - 3.0 * FRAC_PI_2) * FRAC_2_PI
    }
}
