// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Contacts of a shape with the level's static mesh
//! (docs/behavior/physics-contacts.md section 3): the triangles near it and
//! their cache (3.2), the front side, the crossed-triangle records and the
//! push-out contacts (3.3), the ordinary contacts of spheres and boxes
//! (3.4) and the supports (3.5); cylinders get none yet (TODO(#1010)).
//! Every contact's normal points from the shape towards the surface: the
//! level is its first geometry (3.1).

use std::collections::HashSet;

use crate::collision::CollisionWorld;
use crate::collision::geometry::{self, Vec3, add, cross, dot, length, scale, sub};
use crate::physics::shell::Shape;

use super::{Bounds, RawContact, support};

/// A geometry's velocity term per axis (3.2).
const VELOCITY_REACH: f32 = 0.04;
/// A box's margin (3.2).
const BOX_MARGIN: f32 = 0.01;
/// The fetched box is the query box grown by this (3.2).
const FETCH_GROWTH: f32 = 1.3;
/// Ordinary contacts need room for this many more (3.3).
const ORDINARY_ROOM: usize = 10;
/// A second record's normal makes an angle below 135° with the first's.
const SECOND_RECORD_DOT: f32 = -0.707_106_77;
/// Cross axes must be this much better to be chosen (3.4).
const CROSS_AXIS_FACTOR: f32 = 1.05;
/// Beyond this a shape meets no level (m).
const FAR: f32 = 1e6;

/// A crossed-triangle record (3.3): the triangle the centre crossed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub triangle: u32,
}

/// A geometry's state between its collisions with the level (3.2, 3.3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelState {
    /// The centre at the end of its last collision with the level; none
    /// when made (stored as −∞).
    pub previous: Option<Vec3>,
    /// The box the kept triangles were fetched with, and the triangles.
    fetched: Option<Bounds>,
    triangles: Vec<u32>,
    /// The first and second record.
    pub records: Vec<Record>,
}

impl LevelState {
    /// Whether it holds a crossed-triangle record (2.3, item 6).
    pub fn inside(&self) -> bool {
        !self.records.is_empty()
    }

    /// The rest pass's emptying of the cached data (physics-world.md 4.4):
    /// the kept triangles (our reading of "level-contact data").
    pub fn clear(&mut self) {
        self.fetched = None;
        self.triangles = Vec::new();
    }
}

/// A contact with the level: the library's contact (the level first) and
/// its triangle, whose material it carries (2.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelContact {
    pub contact: RawContact,
    pub triangle: u32,
}

/// 3.2: the query box's half extents of `shape` moving at `velocity` (none
/// for a geometry without a body).
pub fn query_reach(shape: &Shape, velocity: Option<Vec3>) -> Vec3 {
    let v = velocity.unwrap_or([0.0; 3]);
    let mut e = super::reach(shape);
    if let Shape::Box { .. } = shape {
        e = e.map(|x| x + BOX_MARGIN);
    }
    std::array::from_fn(|i| e[i] + VELOCITY_REACH * v[i].abs())
}

/// 3.2 – 3.4: the contacts of `shape` (world space) moving at `velocity`
/// with the level's mesh, at most `room` of them, its `state` kept from
/// collision to collision; appended to `out`.
pub fn collide(
    world: &CollisionWorld,
    shape: &Shape,
    velocity: Option<Vec3>,
    state: &mut LevelState,
    room: usize,
    out: &mut Vec<LevelContact>,
) {
    // A cylinder's contacts wait for its colliders (TODO(#1010)).
    if let Shape::Cylinder { .. } = shape {
        return;
    }
    let centre = shape.centre();
    let e = query_reach(shape, velocity);
    // A shape the solving has thrown out of any sensible place (a body at
    // 10¹⁷ m, or not finite) would query the whole mesh at every step: it
    // meets nothing (our guard; the level's extent is far below this).
    if !(0..3).all(|k| centre[k].abs() < FAR && e[k].is_finite() && e[k] < FAR) {
        return;
    }
    let query = (sub(centre, e), add(centre, e));
    // 3.2: fetch again when there is no previous centre, or the query box
    // is not inside the box last fetched.
    let inside_fetched = state
        .fetched
        .is_some_and(|(lo, hi)| (0..3).all(|k| lo[k] <= query.0[k] && query.1[k] <= hi[k]));
    if state.previous.is_none() || !inside_fetched {
        let grown = scale(e, FETCH_GROWTH);
        let fetched = (sub(centre, grown), add(centre, grown));
        state.triangles.clear();
        world.query(fetched.0, fetched.1, &mut state.triangles);
        state.fetched = Some(fetched);
    }
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    // A record whose triangle no longer has the centre behind it goes.
    state.records.retain(|r| {
        let t = world.corners(r.triangle);
        geometry::front_normal(&t).is_some_and(|f| dot(sub(centre, t[0]), f) < 0.0)
    });
    let start = out.len();
    let mut candidates: Vec<(f32, u32, Vec3)> = Vec::new();
    let mut marks = Marks::default();
    for &t in &state.triangles {
        let c = world.corners(t);
        // The overlap test with the query box (believed a box–triangle
        // test, TODO(#957)).
        if !geometry::box_intersects(centre, axes, e, &c) {
            continue;
        }
        let Some(f) = geometry::front_normal(&c) else {
            continue;
        };
        let d = dot(sub(centre, c[0]), f);
        let inside = !state.records.is_empty();
        if d >= 0.0 {
            // While a crossing is found in the pass and there is a
            // previous centre, no ordinary contact either.
            let crossing = !candidates.is_empty() && state.previous.is_some();
            if inside || crossing || out.len() - start + ORDINARY_ROOM > room {
                continue;
            }
            let found = ordinary(shape, &c, f, d, world.triangle(t), &mut marks);
            out.extend(found.into_iter().map(|contact| LevelContact {
                contact,
                triangle: t,
            }));
            continue;
        }
        if let Some(p) = state.previous
            && dot(sub(p, c[0]), f) < 0.0
            && !inside
        {
            continue;
        }
        if overlaps(shape, &c) {
            candidates.push((support(shape, f) - d, t, f));
        }
    }
    // The records from the candidates: the least push-out depth first, a
    // second at an angle below 135° from the first (the further rules are
    // open, TODO(#957)).
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let normal_of = |r: &Record| geometry::front_normal(&world.corners(r.triangle));
    for &(_, t, f) in &candidates {
        if state.records.len() >= 2 || state.records.iter().any(|r| r.triangle == t) {
            continue;
        }
        if state
            .records
            .iter()
            .all(|r| normal_of(r).is_some_and(|g| dot(f, g) > SECOND_RECORD_DOT))
        {
            state.records.push(Record { triangle: t });
        }
    }
    for r in state.records.clone() {
        let c = world.corners(r.triangle);
        let Some(f) = geometry::front_normal(&c) else {
            continue;
        };
        let d = dot(sub(centre, c[0]), f);
        for contact in push_out(shape, f, d) {
            if out.len() - start >= room {
                break;
            }
            out.push(LevelContact {
                contact,
                triangle: r.triangle,
            });
        }
    }
    state.previous = Some(centre);
}

/// The second, closer overlap test of a crossing candidate (3.3): a
/// sphere's distance, a box's separating axes.
fn overlaps(shape: &Shape, t: &[Vec3; 3]) -> bool {
    match *shape {
        Shape::Sphere { centre, radius } => {
            length(sub(geometry::closest_point(centre, t), centre)) <= radius
        }
        Shape::Box {
            axes,
            centre,
            half_size,
        } => geometry::box_intersects(centre, axes, half_size, t),
        Shape::Cylinder { .. } => false,
    }
}

/// 3.3: the push-out contacts of a record with front normal `f`, the
/// centre at `d` from its plane: normal −f, depth s(f) − d when not
/// negative.
fn push_out(shape: &Shape, f: Vec3, d: f32) -> Vec<RawContact> {
    let depth = support(shape, f) - d;
    if depth < 0.0 {
        return Vec::new();
    }
    let normal = scale(f, -1.0);
    let at = |position| RawContact {
        position,
        normal,
        depth,
    };
    match *shape {
        Shape::Sphere { centre, radius } => vec![at(sub(centre, scale(f, radius)))],
        Shape::Box {
            axes,
            centre,
            half_size,
        } => {
            let mut vertices: Vec<Vec3> = (0..8)
                .map(|i| {
                    (0..3).fold(centre, |p, k| {
                        let s = if i >> k & 1 == 0 { -1.0 } else { 1.0 };
                        add(p, scale(axes[k], s * half_size[k]))
                    })
                })
                .collect();
            vertices.sort_by(|a, b| dot(*b, normal).total_cmp(&dot(*a, normal)));
            vertices.into_iter().take(3).map(at).collect()
        }
        Shape::Cylinder { .. } => Vec::new(),
    }
}

/// The edges and vertices a sphere's contacts have used (3.4).
#[derive(Default)]
struct Marks {
    edges: HashSet<(u32, u32)>,
    vertices: HashSet<u32>,
}

/// 3.4: a shape's ordinary contacts with a triangle `t` (vertex indices
/// `v`) with front normal `f`, its centre at `d ≥ 0` in front.
fn ordinary(
    shape: &Shape,
    t: &[Vec3; 3],
    f: Vec3,
    d: f32,
    v: [u32; 3],
    marks: &mut Marks,
) -> Vec<RawContact> {
    match *shape {
        Shape::Sphere { centre, radius } => sphere(centre, radius, t, f, d, v, marks)
            .into_iter()
            .collect(),
        Shape::Box {
            axes,
            centre,
            half_size,
        } => cuboid(centre, axes, half_size, t, f, d),
        Shape::Cylinder { .. } => Vec::new(),
    }
}

fn sphere(
    centre: Vec3,
    r: f32,
    t: &[Vec3; 3],
    f: Vec3,
    d: f32,
    v: [u32; 3],
    marks: &mut Marks,
) -> Option<RawContact> {
    if r - d < 0.0 {
        return None;
    }
    let contact = |u: Vec3, depth: f32| RawContact {
        position: sub(centre, scale(u, r)),
        normal: scale(u, -1.0),
        depth,
    };
    let projected = sub(centre, scale(f, d));
    if inside_triangle(projected, t, f) {
        return Some(contact(f, r - d));
    }
    let edges = [(0, 1), (1, 2), (2, 0)];
    // A marked edge: its neighbour of consistent winding used it.
    if edges
        .iter()
        .any(|&(a, b)| marks.edges.contains(&(v[b], v[a])))
    {
        return None;
    }
    let towards = |q: Vec3| {
        let off = sub(centre, q);
        let l = length(off);
        let u = if l > 0.0 {
            scale(off, 1.0 / l)
        } else {
            [0.0, 1.0, 0.0]
        };
        (u, r - l)
    };
    for &(a, b) in &edges {
        let q = closest_on_segment(centre, t[a], t[b]);
        let (u, depth) = towards(q);
        if depth >= 0.0 {
            marks.edges.insert((v[a], v[b]));
            return Some(contact(u, depth));
        }
    }
    if v.iter().any(|i| marks.vertices.contains(i)) {
        return None;
    }
    for k in 0..3 {
        let (u, depth) = towards(t[k]);
        if depth >= 0.0 {
            marks.vertices.insert(v[k]);
            return Some(contact(u, depth));
        }
    }
    None
}

/// Whether `p`, in the plane of `t` (normal `f`), lies inside it.
fn inside_triangle(p: Vec3, t: &[Vec3; 3], f: Vec3) -> bool {
    (0..3).all(|k| dot(cross(sub(t[(k + 1) % 3], t[k]), sub(p, t[k])), f) >= 0.0)
}

fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    if l2 <= 0.0 {
        return a;
    }
    let s = (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0);
    add(a, scale(ab, s))
}

/// 3.4: a box's separating-axis test against a triangle and its contacts.
fn cuboid(
    centre: Vec3,
    axes: [Vec3; 3],
    h: Vec3,
    t: &[Vec3; 3],
    f: Vec3,
    d: f32,
) -> Vec<RawContact> {
    // Code 0: the front normal.
    let mut best = (h[0] * dot(axes[0], f).abs()
        + h[1] * dot(axes[1], f).abs()
        + h[2] * dot(axes[2], f).abs())
        - d;
    if best < 0.0 {
        return Vec::new();
    }
    let mut normal = scale(f, -1.0);
    let mut code = 0;
    let mut point = None;
    let rel = t.map(|p| sub(p, centre));
    // Codes 1 – 9: a box axis with the triangle's vertex nearest the centre
    // along it, when all three lie on one side.
    for k in 0..3 {
        let p = rel.map(|q| dot(q, axes[k]));
        if !(p.iter().all(|&x| x > 0.0) || p.iter().all(|&x| x < 0.0)) {
            continue;
        }
        let j = (0..3)
            .min_by(|&a, &b| p[a].abs().total_cmp(&p[b].abs()))
            .unwrap_or(0);
        let depth = h[k] - p[j].abs();
        if depth < 0.0 {
            return Vec::new();
        }
        if depth < best {
            best = depth;
            normal = scale(axes[k], p[j].signum());
            code = 1 + 3 * k + j;
            point = Some(t[j]);
        }
    }
    // Codes 10 – 18: a box axis crossed with an edge.
    for k in 0..3 {
        for m in 0..3 {
            let e = sub(t[(m + 1) % 3], t[m]);
            let Some(a) = geometry::normalize(cross(axes[k], e)) else {
                continue;
            };
            let p = rel.map(|q| dot(q, a));
            let radius: f32 = (0..3).map(|i| h[i] * dot(axes[i], a).abs()).sum();
            let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
            if lo > radius || hi < -radius {
                return Vec::new();
            }
            // The deeper side's overlap, the axis pointing from the box
            // towards the triangle.
            let (depth, sign) = if radius - lo < hi + radius {
                (radius - lo, 1.0)
            } else {
                (hi + radius, -1.0)
            };
            if depth * CROSS_AXIS_FACTOR < best {
                best = depth;
                normal = scale(a, sign);
                code = 10 + 3 * k + m;
                // Its point is open (TODO(#957)): the edge's point nearest
                // the box's centre.
                point = Some(closest_on_segment(centre, t[m], t[(m + 1) % 3]));
            }
        }
    }
    if dot(normal, f) > 0.0 {
        return Vec::new();
    }
    let contact = |position| RawContact {
        position,
        normal,
        depth: best,
    };
    if code > 0 {
        return point.map(contact).into_iter().collect();
    }
    // Code 0: the vertex deepest along the normal and those along its
    // edges, each behind the plane (their own depths are open, TODO(#957):
    // each carries the axis's).
    let deepest: [f32; 3] = std::array::from_fn(|k| {
        if dot(axes[k], normal) > 0.0 {
            1.0
        } else {
            -1.0
        }
    });
    let vertex = |s: [f32; 3]| (0..3).fold(centre, |p, k| add(p, scale(axes[k], s[k] * h[k])));
    let mut out = Vec::new();
    let mut signs = vec![deepest];
    for k in 0..3 {
        let mut s = deepest;
        s[k] = -s[k];
        signs.push(s);
    }
    for s in signs {
        let p = vertex(s);
        if dot(sub(p, t[0]), f) <= 0.0 {
            out.push(contact(p));
        }
    }
    out
}
