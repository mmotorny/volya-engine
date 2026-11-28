// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Choosing a patrol point (docs/behavior/character-paths.md 6.5, cited
//! by section): the start point of a patrol path by start type, the
//! fatal errors of the choice, the start point reached at once, and the
//! next point chosen among a reached point's links. Accessibility is the
//! caller's (3.4, [`super::restrictions::Restrictions::accessible_position`]).

use volya_engine_formats::patrol_path::PatrolPath;

/// The start types (N's `patrol` constants, 6.5).
pub const START: i32 = 0;
pub const CONTINUE: i32 = 1;
pub const NEAREST: i32 = 2;
pub const CUSTOM: i32 = 3;
pub const NEXT: i32 = 4;

/// The route type `patrol.stop` (6.5 step 3).
pub const STOP: i32 = 0;

/// The largest draw of the shared generator (6.5 step 2).
pub const DRAW_MAX: u32 = 32_767;

/// How near the start point a character counts as on it, per axis (6.5).
const AT_ONCE: f32 = 0.1;

/// A fatal error of the start point's choice (6.5; console-log 3.8).
#[derive(Clone, Debug, PartialEq)]
pub enum ChoiceError {
    /// No point: the log lines listed before the report, and the message.
    Inaccessible { log: Vec<String>, message: String },
    /// The point's level vertex number is not below the vertex count.
    InvalidVertex { message: String },
}

/// The restriction texts the inaccessible error lists (6.5).
#[derive(Clone, Copy, Debug, Default)]
pub struct Listed<'a> {
    pub default_out: &'a str,
    pub default_in: &'a str,
    pub out: &'a str,
    pub inside: &'a str,
}

fn squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

/// The nearest point to `position` whose position is `accessible`; of
/// equals the first.
fn nearest(
    path: &PatrolPath,
    position: [f32; 3],
    accessible: &dyn Fn([f32; 3]) -> bool,
) -> Option<u32> {
    let mut best: Option<(f32, u32)> = None;
    for (i, p) in path.points.iter().enumerate() {
        if !accessible(p.position) {
            continue;
        }
        let d = squared(p.position, position);
        if best.is_none_or(|(b, _)| d < b) {
            best = Some((d, i as u32));
        }
    }
    best.map(|(_, i)| i)
}

/// The linked points of `point`, in stored order.
fn linked(path: &PatrolPath, point: u32) -> impl Iterator<Item = (u32, f32)> + '_ {
    let n = path.points.len() as u32;
    (path.links.iter())
        .filter(move |l| l.from == point && l.to < n)
        .map(|l| (l.to, l.weight))
}

/// What the start point's choice reads (6.5).
pub struct StartChoice<'a> {
    pub path_name: &'a str,
    pub object_name: &'a str,
    pub start_type: i32,
    /// `set_start_point`'s point.
    pub custom: Option<u32>,
    /// The previous point, for `patrol.next`.
    pub previous: Option<u32>,
    /// The random flag, for `patrol.next`'s link choice.
    pub random: bool,
    /// The character's position.
    pub position: [f32; 3],
    /// The level graph's vertex count.
    pub vertex_count: u32,
    pub listed: Listed<'a>,
}

/// The start point of `path` (6.5) and whether the character is on it
/// already (within 0.1 m on each axis); `accessible` tests a position
/// (3.4), `draw` is the shared generator for `patrol.next`'s link choice.
pub fn start_point(
    path: &PatrolPath,
    choice: &StartChoice,
    accessible: &dyn Fn([f32; 3]) -> bool,
    draw: &mut dyn FnMut() -> u32,
) -> Result<(u32, bool), ChoiceError> {
    let n = path.points.len() as u32;
    let nearest = || nearest(path, choice.position, accessible);
    let point = match choice.start_type {
        START => (n > 0).then_some(0),
        CONTINUE => n.checked_sub(1),
        CUSTOM => choice.custom.filter(|&p| p < n),
        NEXT => match choice.previous.filter(|&p| p < n) {
            Some(p) if p + 1 < n => Some(p + 1),
            Some(p) => choose_link(path, p, None, choice.random, accessible, draw).or_else(nearest),
            None => nearest(),
        },
        // `patrol.nearest`, and a start type the spec doesn't name (our
        // choice).
        _ => nearest(),
    };
    let Some(point) = point else {
        let l = choice.listed;
        return Err(ChoiceError::Inaccessible {
            log: vec![
                format!("DEFAULT OUT RESTRICTIONS :{}", l.default_out),
                format!("DEFAULT IN RESTRICTIONS  :{}", l.default_in),
                format!("OUT RESTRICTIONS         :{}", l.out),
                format!("IN RESTRICTIONS          :{}", l.inside),
            ],
            message: format!(
                "any vertex in patrol path [{}] in inaccessible for object [{}]",
                choice.path_name, choice.object_name
            ),
        });
    };
    let p = &path.points[point as usize];
    if p.level_vertex >= choice.vertex_count {
        return Err(ChoiceError::InvalidVertex {
            message: format!(
                "patrol path[{}], point on path [{}],object [{}]",
                choice.path_name, p.name, choice.object_name
            ),
        });
    }
    let on = (0..3).all(|i| (p.position[i] - choice.position[i]).abs() <= AT_ONCE);
    Ok((point, on))
}

/// The choice among `point`'s links (6.5 step 2): the candidates are its
/// linked points with accessible positions, the `previous` point left out;
/// with `random` and more than one, the first at which the running sum of
/// weights reaches a draw scaled to their sum; otherwise the first.
fn choose_link(
    path: &PatrolPath,
    point: u32,
    previous: Option<u32>,
    random: bool,
    accessible: &dyn Fn([f32; 3]) -> bool,
    draw: &mut dyn FnMut() -> u32,
) -> Option<u32> {
    let candidates: Vec<(u32, f32)> = linked(path, point)
        .filter(|&(to, _)| Some(to) != previous && accessible(path.points[to as usize].position))
        .collect();
    if random && candidates.len() > 1 {
        let sum: f32 = candidates.iter().map(|c| c.1).sum();
        let r = draw() as f32 * sum / DRAW_MAX as f32;
        let mut running = 0.0;
        for &(to, weight) in &candidates {
            running += weight;
            if running >= r {
                return Some(to);
            }
        }
        // Rounding left the draw above the sum: the last (our guard).
        return candidates.last().map(|c| c.0);
    }
    candidates.first().map(|c| c.0)
}

/// The next point after `reached` (6.5 steps 2 and 3), `previous` the
/// point before it: `None` when the path is finished.
pub fn next_point(
    path: &PatrolPath,
    reached: u32,
    previous: Option<u32>,
    random: bool,
    route_type: i32,
    accessible: &dyn Fn([f32; 3]) -> bool,
    draw: &mut dyn FnMut() -> u32,
) -> Option<u32> {
    if let Some(next) = choose_link(path, reached, previous, random, accessible, draw) {
        return Some(next);
    }
    if route_type == STOP {
        return None;
    }
    linked(path, reached)
        .map(|(to, _)| to)
        .find(|&to| accessible(path.points[to as usize].position))
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::patrol_path::{PathLink, PathPoint};

    /// Points 0 … 3 along x at 1 m, links 0→1, 1→0, 1→2 (weight 3),
    /// 1→3 (weight 1), 2→1.
    fn path() -> PatrolPath {
        let point = |i: u32| PathPoint {
            name: format!("p{i}"),
            position: [i as f32, 0.0, 0.0],
            flags: 0,
            level_vertex: i,
            game_vertex: 0,
        };
        let link = |from, to, weight| PathLink { from, to, weight };
        PatrolPath {
            name: "walk".into(),
            points: (0..4).map(point).collect(),
            links: vec![
                link(0, 1, 1.0),
                link(1, 0, 1.0),
                link(1, 2, 3.0),
                link(1, 3, 1.0),
                link(2, 1, 1.0),
            ],
        }
    }

    fn choice(start_type: i32, position: [f32; 3]) -> StartChoice<'static> {
        StartChoice {
            path_name: "walk",
            object_name: "novice",
            start_type,
            custom: Some(2),
            previous: None,
            random: false,
            position,
            vertex_count: 10,
            listed: Listed::default(),
        }
    }

    #[test]
    fn start_points() {
        let p = path();
        let all = |_: [f32; 3]| true;
        let mut draw = || 0;
        let start = |c: &StartChoice, a: &dyn Fn([f32; 3]) -> bool, d: &mut dyn FnMut() -> u32| {
            start_point(&p, c, a, d)
        };
        assert_eq!(
            start(&choice(START, [5.0; 3]), &all, &mut draw),
            Ok((0, false))
        );
        assert_eq!(
            start(&choice(CONTINUE, [5.0; 3]), &all, &mut draw),
            Ok((3, false))
        );
        assert_eq!(
            start(&choice(CUSTOM, [5.0; 3]), &all, &mut draw),
            Ok((2, false))
        );
        let near_two = choice(NEAREST, [2.05, 0.0, -0.05]);
        assert_eq!(
            start(&near_two, &all, &mut draw),
            Ok((2, true)),
            "on it at once"
        );
        let not_two = |q: [f32; 3]| q[0] != 2.0;
        assert_eq!(start(&near_two, &not_two, &mut draw), Ok((3, false)));
        let mut next = choice(NEXT, [0.0; 3]);
        next.previous = Some(1);
        assert_eq!(start(&next, &all, &mut draw), Ok((2, false)));
        next.previous = Some(3);
        next.position = [3.0, 0.0, 0.0];
        assert_eq!(
            start(&next, &all, &mut draw),
            Ok((3, true)),
            "no link: nearest"
        );
        next.position = [0.0; 3];
        next.previous = None;
        assert_eq!(start(&next, &all, &mut draw), Ok((0, true)));
        let none = |_: [f32; 3]| false;
        let Err(ChoiceError::Inaccessible { log, message }) = start(&near_two, &none, &mut draw)
        else {
            panic!("no point");
        };
        assert_eq!(log[3], "IN RESTRICTIONS          :");
        assert_eq!(
            message,
            "any vertex in patrol path [walk] in inaccessible for object [novice]"
        );
        let mut small = choice(CONTINUE, [0.0; 3]);
        small.vertex_count = 3;
        assert_eq!(
            start(&small, &all, &mut draw),
            Err(ChoiceError::InvalidVertex {
                message: "patrol path[walk], point on path [p3],object [novice]".into()
            })
        );
    }

    #[test]
    fn next_points() {
        let p = path();
        let all = |_: [f32; 3]| true;
        let next = |reached, previous, random, route, a: &dyn Fn([f32; 3]) -> bool, d: u32| {
            next_point(&p, reached, previous, random, route, a, &mut || d)
        };
        // Not back to the previous point: the first candidate.
        assert_eq!(next(1, Some(0), false, STOP, &all, 0), Some(2));
        // Random: weights 3 and 1 of a sum of 4.
        assert_eq!(next(1, Some(0), true, STOP, &all, 0), Some(2));
        assert_eq!(next(1, Some(0), true, STOP, &all, 24_575), Some(2));
        assert_eq!(next(1, Some(0), true, STOP, &all, 24_576), Some(3));
        assert_eq!(next(1, Some(0), true, STOP, &all, DRAW_MAX), Some(3));
        // At an end: back along its only link unless the route stops.
        assert_eq!(next(2, Some(1), false, CONTINUE, &all, 0), Some(1));
        assert_eq!(next(2, Some(1), false, STOP, &all, 0), None);
        let only_zero = |q: [f32; 3]| q[0] == 0.0;
        assert_eq!(next(1, Some(0), false, CONTINUE, &only_zero, 0), Some(0));
        assert_eq!(next(3, None, false, CONTINUE, &all, 0), None, "no links");
    }
}
