// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Space restrictions (docs/behavior/character-paths.md, cited by
//! section): restriction texts and a character's combined restrictions
//! (2.2), a restrictor's shape (3.1), the cell tests (3.2), accessible
//! vertices and positions (3.3, 3.4), borders (3.6),
//! `accessible_nearest` (section 4) and the level path searched under
//! them (section 5), over the level graph ([`LevelGraph`]).
//!
//! The level's vertices are taken in their stored order, which is by
//! packed column and row (level-graph.md 1.3).

use std::ops::RangeInclusive;

use super::level_graph::{LevelGraph, LevelPathStep};
use volya_engine_formats::level_ai::LevelAi;
use volya_engine_formats::model;
use volya_engine_formats::object::Shape;

/// The radius of a test point's sphere and of `accessible(pos)` (3.2,
/// 3.4).
pub const TEST_RADIUS: f32 = 0.001;
/// How far the cell's corners are shrunk (3.2).
const CORNER_SHRINK: f32 = 0.001;
/// The height within which a border vertex in a position's cell column
/// makes it inaccessible (3.4).
const BORDER_HEIGHT: f32 = 2.0;

/// The restrictor types (1.1).
pub const RESTRICTOR: u8 = 0;
pub const DEFAULT_OUT_RESTRICTOR: u8 = 1;
pub const DEFAULT_IN_RESTRICTOR: u8 = 2;

/// Whether a restrictor of `restrictor_type` is registered as it comes
/// online (2.1): types 0, 1 and 2.
pub fn registered(restrictor_type: u8) -> bool {
    restrictor_type <= DEFAULT_IN_RESTRICTOR
}

/// The items of restriction text `text` (conventions): its names
/// separated by commas, each without surrounding spaces; an empty name is
/// no item (our reading).
pub fn items(text: &[u8]) -> impl Iterator<Item = &[u8]> {
    text.split(|&b| b == b',')
        .map(|n| n.trim_ascii())
        .filter(|n| !n.is_empty())
}

fn joined<'a>(items: impl IntoIterator<Item = &'a [u8]>) -> Vec<u8> {
    let items: Vec<&[u8]> = items.into_iter().collect();
    items.join(&b","[..])
}

/// The union of texts `a` and `b` (conventions): `a` followed by each item
/// of `b` that `a` does not contain.
pub fn union(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out: Vec<&[u8]> = items(a).collect();
    for item in items(b) {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    joined(out)
}

/// The difference `a` − `b` (conventions): the items of `a` that `b` does
/// not contain.
pub fn difference(a: &[u8], b: &[u8]) -> Vec<u8> {
    let b: Vec<&[u8]> = items(b).collect();
    joined(items(a).filter(|i| !b.contains(i)))
}

/// `text` normalised (2.2): with more than one item, its items sorted in
/// ascending byte order, duplicates kept, joined with commas.
pub fn normalised(text: &[u8]) -> Vec<u8> {
    let mut all: Vec<&[u8]> = items(text).collect();
    if all.len() < 2 {
        return joined(all);
    }
    all.sort();
    joined(all)
}

/// A character's combined out- and in-restrictions (2.2) from its own
/// texts and the level's default lists, each normalised.
pub fn combined(
    own_out: &[u8],
    own_in: &[u8],
    default_out: &[u8],
    default_in: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    let out = union(own_out, &difference(default_out, own_in));
    let inside = union(own_in, &difference(default_in, own_out));
    (normalised(&out), normalised(&inside))
}

/// A box face plane: unit normal and offset, the signed distance of *p*
/// being normal · *p* + offset, positive outside (3.1, believed).
pub type Plane = ([f32; 3], f32);

/// What a Shape element is in the world (3.1).
#[derive(Clone, Debug, PartialEq)]
pub enum ElementKind {
    Sphere { centre: [f32; 3], radius: f32 },
    Box { planes: [Plane; 6] },
}

/// A Shape element of a restrictor in the world, with its horizontal
/// bounding box (3.6): low and high x and z.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeElement {
    pub kind: ElementKind,
    pub low: [f32; 2],
    pub high: [f32; 2],
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn squared_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    dot(d, d)
}

impl ShapeElement {
    /// Whether the element meets the sphere (`p`, `r`) (3.1).
    pub fn meets(&self, p: [f32; 3], r: f32) -> bool {
        match &self.kind {
            ElementKind::Sphere { centre, radius } => {
                let s = radius + r;
                squared_distance(p, *centre) < s * s
            }
            ElementKind::Box { planes } => planes.iter().all(|(n, d)| dot(*n, p) + d <= r),
        }
    }
}

/// A restrictor's shape in the world (3.1): its Shape elements.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RestrictorShape {
    pub elements: Vec<ShapeElement>,
}

impl RestrictorShape {
    /// The shape of a restrictor at `position` with `angles` whose Shape
    /// block holds `shapes` (3.1, 3.6): a sphere's centre transformed by
    /// the restrictor's transform (objects 4, believed), its bounding box
    /// the position plus its centre, not rotated, ± its radius; a box's
    /// corners **o** ± ½**a₁** ± ½**a₂** ± ½**a₃** transformed, its faces
    /// the planes about its transformed centre, its bounding box that of
    /// its eight corners.
    pub fn placed(shapes: &[Shape], position: [f32; 3], angles: [f32; 3]) -> Self {
        let x = model::transform(position, angles);
        let point = |p: [f32; 3]| model::transform_point(&x, p);
        let direction = |d: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|c| d[0] * x[0][c] + d[1] * x[1][c] + d[2] * x[2][c])
        };
        let elements = shapes
            .iter()
            .map(|shape| match *shape {
                Shape::Sphere { centre, radius } => {
                    let (cx, cz) = (position[0] + centre[0], position[2] + centre[2]);
                    ShapeElement {
                        kind: ElementKind::Sphere {
                            centre: point(centre),
                            radius,
                        },
                        low: [cx - radius, cz - radius],
                        high: [cx + radius, cz + radius],
                    }
                }
                Shape::Box([a, b, c, o]) => {
                    let centre = point(o);
                    let mut planes = [([0.0; 3], 0.0); 6];
                    for (k, axis) in [a, b, c].into_iter().enumerate() {
                        let u = direction(axis);
                        let l = dot(u, u).sqrt();
                        let n = if l > 0.0 { u.map(|v| v / l) } else { [0.0; 3] };
                        let m = dot(n, centre);
                        planes[2 * k] = (n, -m - l * 0.5);
                        planes[2 * k + 1] = (n.map(|v| -v), m - l * 0.5);
                    }
                    let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
                    for i in 0..8 {
                        let s = |bit: usize| if i & bit == 0 { -0.5 } else { 0.5 };
                        let corner: [f32; 3] =
                            std::array::from_fn(|j| o[j] + s(1) * a[j] + s(2) * b[j] + s(4) * c[j]);
                        let w = point(corner);
                        low = [low[0].min(w[0]), low[1].min(w[2])];
                        high = [high[0].max(w[0]), high[1].max(w[2])];
                    }
                    ShapeElement {
                        kind: ElementKind::Box { planes },
                        low,
                        high,
                    }
                }
            })
            .collect();
        Self { elements }
    }

    /// Whether the restrictor contains the sphere (`p`, `r`): one of its
    /// elements meets it (3.1).
    pub fn meets(&self, p: [f32; 3], r: f32) -> bool {
        self.elements.iter().any(|e| e.meets(p, r))
    }
}

/// The first vertex whose packed column and row is not below `packed`.
fn first_at_or_after(ai: &LevelAi, n: u32, packed: u32) -> u32 {
    let (mut lo, mut hi) = (0, n);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if ai.packed(mid).is_some_and(|p| p < packed) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The vertices in the cell of packed column and row `packed`.
fn cell_vertices(g: &LevelGraph, packed: u32) -> std::ops::Range<u32> {
    let n = g.len();
    let first = first_at_or_after(g.ai, n, packed);
    let end = first_at_or_after(g.ai, n, packed + 1);
    first..end
}

/// The packed cell of `p` when it is a valid vertex position (3.4: inside
/// the level graph's bounds, in a cell that has a vertex).
pub fn valid_position(g: &LevelGraph, p: [f32; 3]) -> Option<u32> {
    let packed = g.ai.packed_at(p)?;
    (!cell_vertices(g, packed).is_empty()).then_some(packed)
}

/// The level vertex of position `p`: of its cell's vertices, the highest
/// whose plane lies at or below `p`, else the lowest (level-graph.md 2.3,
/// the cell vertex).
pub fn level_vertex_of(g: &LevelGraph, p: [f32; 3]) -> Option<u32> {
    let packed = valid_position(g, p)?;
    let mut below: Option<(f32, u32)> = None;
    let mut lowest: Option<(f32, u32)> = None;
    for k in cell_vertices(g, packed) {
        let h = g.ai.plane_height(k, p[0], p[2]).unwrap_or(f32::NAN);
        if h <= p[1] && below.is_none_or(|(b, _)| h > b) {
            below = Some((h, k));
        }
        if lowest.is_none_or(|(l, _)| h < l) {
            lowest = Some((h, k));
        }
    }
    below.or(lowest).map(|(_, k)| k)
}

/// Vertex `k`'s five test points (3.2): its cell's four corners shrunk by
/// 0.001, on its plane, then its position; `None` for an invalid `k`.
pub fn test_points(g: &LevelGraph, k: u32) -> Option<[[f32; 3]; 5]> {
    if !g.valid(k) {
        return None;
    }
    let v = g.ai.position(k)?;
    let d = g.ai.header.cell_size / 2.0 - CORNER_SHRINK;
    let corner = |dx: f32, dz: f32| {
        let (x, z) = (v[0] + dx, v[2] + dz);
        [x, g.ai.plane_height(k, x, z).unwrap_or(v[1]), z]
    };
    Some([
        corner(-d, -d),
        corner(-d, d),
        corner(d, -d),
        corner(d, d),
        v,
    ])
}

/// How many of `k`'s test points `inside` contains: `None` for an invalid
/// `k`.
fn points_inside(g: &LevelGraph, k: u32, inside: impl Fn([f32; 3]) -> bool) -> Option<usize> {
    Some(
        test_points(g, k)?
            .into_iter()
            .filter(|&p| inside(p))
            .count(),
    )
}

/// An area: the union of restrictors' shapes (a composition, 3.1).
#[derive(Clone, Debug, Default)]
pub struct Area<'a> {
    pub members: Vec<&'a RestrictorShape>,
}

impl<'a> Area<'a> {
    /// Whether any member contains the sphere (`p`, `r`).
    pub fn meets(&self, p: [f32; 3], r: f32) -> bool {
        self.members.iter().any(|m| m.meets(p, r))
    }

    fn count(&self, g: &LevelGraph, k: u32) -> usize {
        points_inside(g, k, |p| self.meets(p, TEST_RADIUS)).unwrap_or(0)
    }

    /// Whether vertex `k` is wholly inside the area (3.2).
    pub fn wholly_inside(&self, g: &LevelGraph, k: u32) -> bool {
        g.valid(k) && self.count(g, k) == 5
    }

    /// Whether vertex `k` is partly inside the area (3.2).
    pub fn partly_inside(&self, g: &LevelGraph, k: u32) -> bool {
        self.count(g, k) > 0
    }

    /// The vertices an element's packed range covers (3.6): from the first
    /// vertex at or after the low corner to the first vertex after the
    /// high corner, a corner that is not a valid vertex position standing
    /// for the first or the last vertex.
    fn range(g: &LevelGraph, e: &ShapeElement) -> RangeInclusive<u32> {
        let n = g.len();
        let last = n.saturating_sub(1);
        let lo = match valid_position(g, [e.low[0], 0.0, e.low[1]]) {
            Some(p) => first_at_or_after(g.ai, n, p),
            None => 0,
        };
        let hi = match valid_position(g, [e.high[0], 0.0, e.high[1]]) {
            Some(p) => first_at_or_after(g.ai, n, p + 1).min(last),
            None => last,
        };
        lo..=hi
    }

    /// Whether `k` is on member `m`'s border (3.6).
    fn on_member_border(g: &LevelGraph, m: &RestrictorShape, k: u32) -> bool {
        let in_range = m.elements.iter().any(|e| Self::range(g, e).contains(&k));
        in_range && {
            let c = points_inside(g, k, |p| m.meets(p, TEST_RADIUS)).unwrap_or(0);
            c > 0 && c < 5
        }
    }

    /// Whether `k` is on the area's border (3.6): on a member's border and
    /// not wholly inside the area.
    pub fn border_contains(&self, g: &LevelGraph, k: u32) -> bool {
        self.members.iter().any(|m| Self::on_member_border(g, m, k)) && !self.wholly_inside(g, k)
    }

    /// The area's border (3.6), in ascending vertex order.
    pub fn border(&self, g: &LevelGraph) -> Vec<u32> {
        let mut found = Vec::new();
        for m in &self.members {
            for e in &m.elements {
                for k in Self::range(g, e) {
                    let c = points_inside(g, k, |p| m.meets(p, TEST_RADIUS)).unwrap_or(0);
                    if c > 0 && c < 5 {
                        found.push(k);
                    }
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found.retain(|&k| !self.wholly_inside(g, k));
        found
    }
}

/// A character's restrictions in effect (2.2, 3.5): its out area and in
/// area, each `None` without items. Uninitialised restrictions (a named
/// restrictor not registered) make everything accessible and apply no
/// border.
#[derive(Clone, Debug, Default)]
pub struct Restrictions<'a> {
    pub initialised: bool,
    pub out: Option<Area<'a>>,
    pub inside: Option<Area<'a>>,
}

impl<'a> Restrictions<'a> {
    /// The restrictions of combined texts `out` and `inside` (2.2), each
    /// name looked up as given by `shape` among the registered
    /// restrictors: initialised only when every name is registered (3.5;
    /// tested at each use, our reading).
    pub fn of(
        out: &[u8],
        inside: &[u8],
        shape: impl Fn(&[u8]) -> Option<&'a RestrictorShape>,
    ) -> Self {
        let area = |text: &[u8]| -> Option<Option<Area<'a>>> {
            let members = items(text).map(&shape).collect::<Option<Vec<_>>>()?;
            Some((!members.is_empty()).then_some(Area { members }))
        };
        match (area(out), area(inside)) {
            (Some(out), Some(inside)) => Self {
                initialised: true,
                out,
                inside,
            },
            _ => Self::default(),
        }
    }

    /// Whether any restriction is in effect.
    pub fn any(&self) -> bool {
        self.initialised && (self.out.is_some() || self.inside.is_some())
    }

    /// Whether vertex `k` is accessible (3.3).
    pub fn accessible_vertex(&self, g: &LevelGraph, k: u32) -> bool {
        if !self.any() {
            return true;
        }
        if !g.valid(k) {
            return false;
        }
        let out_ok = self.out.as_ref().is_none_or(|a| a.wholly_inside(g, k));
        out_ok && self.inside.as_ref().is_none_or(|a| !a.partly_inside(g, k))
    }

    /// Whether position `p` is accessible: the sphere (`p`, 0.001) (3.4).
    pub fn accessible_position(&self, g: &LevelGraph, p: [f32; 3]) -> bool {
        if !self.any() {
            return true;
        }
        let Some(packed) = valid_position(g, p) else {
            return false;
        };
        let near_border = |a: &Area| {
            cell_vertices(g, packed).any(|k| {
                let y = g.ai.position(k).map_or(f32::NAN, |v| v[1]);
                (y - p[1]).abs() < BORDER_HEIGHT && a.border_contains(g, k)
            })
        };
        if let Some(out) = &self.out {
            if !out.meets(p, TEST_RADIUS) || near_border(out) {
                return false;
            }
            let Some(k) = level_vertex_of(g, p) else {
                return false;
            };
            let Some(h) = g.ai.plane_height(k, p[0], p[2]) else {
                return false;
            };
            if !out.meets([p[0], h, p[2]], TEST_RADIUS) {
                return false;
            }
        }
        if let Some(inside) = &self.inside
            && (inside.meets(p, TEST_RADIUS) || near_border(inside))
        {
            return false;
        }
        true
    }

    /// The character's border (3.6), in ascending vertex order: empty
    /// while uninitialised.
    pub fn border(&self, g: &LevelGraph) -> Vec<u32> {
        if !self.initialised {
            return Vec::new();
        }
        match (&self.out, &self.inside) {
            (None, None) => Vec::new(),
            (None, Some(inside)) => inside.border(g),
            (Some(out), None) => out.border(g),
            (Some(out), Some(inside)) => {
                let mut border: Vec<u32> = (out.border(g).into_iter())
                    .filter(|&k| !inside.wholly_inside(g, k))
                    .collect();
                border.extend((inside.border(g).into_iter()).filter(|&k| out.partly_inside(g, k)));
                border.sort_unstable();
                border.dedup();
                border
            }
        }
    }

    /// `accessible_nearest(pos, out)` (section 4) over the character's
    /// `border`: the vertex and the test point written to `out`. `None`
    /// without a candidate, and without restrictions in effect (both
    /// unestablished).
    pub fn accessible_nearest(
        &self,
        g: &LevelGraph,
        border: &[u32],
        pos: [f32; 3],
    ) -> Option<(u32, [f32; 3])> {
        if !self.any() {
            return None;
        }
        let at = |k: u32| {
            g.ai.position(k)
                .map_or(f32::INFINITY, |v| squared_distance(v, pos))
        };
        let accessible_links = |k: u32| g.neighbours(k).filter(|&l| self.accessible_vertex(g, l));
        // 1, 2: the first in ascending order among equals.
        let mut b: Option<(f32, u32)> = None;
        for &k in border {
            if accessible_links(k).next().is_none() {
                continue;
            }
            let d = at(k);
            if b.is_none_or(|(bd, _)| d < bd) {
                b = Some((d, k));
            }
        }
        let (_, b) = b?;
        // 3: of equals the first in link order.
        let mut result: Option<(f32, u32)> = None;
        for l in accessible_links(b) {
            let d = at(l);
            if result.is_none_or(|(rd, _)| d < rd) {
                result = Some((d, l));
            }
        }
        let (_, k) = result?;
        // 4: of equals the last in 3.2's order (unestablished: our reading,
        // which gives section 9's point).
        let point = test_points(g, k)?
            .into_iter()
            .rev()
            .min_by(|a, b| squared_distance(*a, pos).total_cmp(&squared_distance(*b, pos)))?;
        Some((k, point))
    }

    /// The level path from `start` to `goal` (section 5) with `step`'s
    /// memory of the last failure: the character's `border` disabled for
    /// the search when `start` is accessible.
    pub fn level_path(
        &self,
        g: &LevelGraph,
        border: &[u32],
        step: &mut LevelPathStep,
        start: u32,
        goal: u32,
    ) -> Option<Vec<u32>> {
        let mut disabled = g.accessibility.clone();
        if self.accessible_vertex(g, start) {
            disabled.clear(border);
        }
        let graph = LevelGraph {
            accessibility: &disabled,
            ..*g
        };
        step.search(&graph, start, goal)
    }
}

/// A destination made accessible as the level path type's reset does
/// (6.1): the destination vertex and the detail path's end position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Destination {
    pub vertex: u32,
    pub end_position: [f32; 3],
}

impl Restrictions<'_> {
    /// 6.1's reset of destination `vertex` with desired end position
    /// `end_position`: an inaccessible vertex replaced by
    /// `accessible_nearest` of its position and the end position by that
    /// function's point; otherwise an inaccessible end position replaced by
    /// the vertex's position. `None` when `accessible_nearest` gives none
    /// (unestablished).
    pub fn accessible_destination(
        &self,
        g: &LevelGraph,
        border: &[u32],
        vertex: u32,
        end_position: [f32; 3],
    ) -> Option<Destination> {
        let position = g.ai.position(vertex)?;
        if !self.accessible_vertex(g, vertex) {
            let (vertex, end_position) = self.accessible_nearest(g, border, position)?;
            return Some(Destination {
                vertex,
                end_position,
            });
        }
        let end_position = if self.accessible_position(g, end_position) {
            end_position
        } else {
            position
        };
        Some(Destination {
            vertex,
            end_position,
        })
    }
}

#[cfg(test)]
mod tests;
