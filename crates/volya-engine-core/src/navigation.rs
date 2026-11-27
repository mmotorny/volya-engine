// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Route planning for tap-to-move (our design, not the original's AI; see
//! docs/architecture/simulation.md, Navigation): an A* search over a
//! grid of [`CELL`]-metre columns probed lazily from the level's collision
//! world while the search runs, so only the area a search explores is
//! ever built, and dropped when it ends.
//!
//! A column's **floors** are where its vertical line meets the front side
//! of a solid walkable triangle (normal y ≥ [`WALKABLE_NORMAL_Y`]); a
//! floor's **room** is the tallest of the player's three postures whose
//! box (standing, crouch, low crouch, from [`MovementConfig`]'s boxes)
//! fits over it, from [`CLEAR_BASE`] above the floor, [`CLEAR_HALF`] to
//! each side; the ladders' boxes count as walls. Neighbouring floors join
//! by edges of the kinds of [`Kind`], costed by the time the movement
//! takes at that posture's speed: a step within [`Body::step`], a drop to
//! within [`Body::drop`], a jump up to a ledge within [`Body::jump_up`], a
//! jump over a gap or an obstacle lower than [`Body::jump_over`], and a
//! ladder's bottom and top joined both ways. The search ends at the first
//! floor within [`GOAL_REACH`] of the target, or else at the floor nearest
//! the target it reached (a tap on a wall goes to its foot).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::collision::CollisionWorld;
use crate::collision::body::WALKABLE_NORMAL_Y;
use crate::collision::geometry::{Vec3, box_intersects, closest_point, front_normal, sub};
use crate::movement::{GRAVITY, MovementConfig};

#[cfg(test)]
mod tests;

/// The grid's spacing, metres (our choice): under twice [`CLEAR_HALF`], so
/// a wall between two columns meets one of their boxes.
pub const CELL: f32 = 0.3;
/// The half width of a floor's clearance boxes (our choice: between the
/// torso's radius 0.29 and the bottom sphere's 0.35).
pub const CLEAR_HALF: f32 = 0.28;
/// Clearance is tested from this height above a floor (our choice): lower
/// things are steps the movement climbs.
pub const CLEAR_BASE: f32 = 0.5;
/// Headroom over a posture's box height (our choice).
pub const HEAD_MARGIN: f32 = 0.05;
/// A floor within this of the target (vertically, beyond a standing body's
/// span) ends the search (our choice).
pub const GOAL_REACH: f32 = 0.45;
/// The target is taken this far back toward the start, horizontally, so
/// a target on a wall is met on its near side (our choice).
pub const GOAL_PULL: f32 = 0.3;
/// A search that doesn't reach the target goes to the floor nearest it
/// only within this many metres of it and in at most this factor of the
/// straight run's time, plus this many seconds (our choices: a tap high on
/// a wall goes to its foot, not around to its far side or a long way).
pub const NEAREST_REACH: f32 = 1.5;
pub const DETOUR_FACTOR: f32 = 2.0;
pub const DETOUR_SLACK: f32 = 3.0;
/// Floors a search may expand at most (our choice; measured in
/// docs/architecture/simulation.md).
pub const MAX_EXPANSIONS: usize = 20_000;
/// A search that came no nearer the target over this many expansions
/// ends there (our choice: a target nothing leads to, as a roof, isn't
/// searched for at the full cost).
pub const STALL_EXPANSIONS: usize = 8_000;
/// The heuristic's weight (our choice): above 1 trades a little length for
/// a much smaller search.
pub const HEURISTIC_WEIGHT: f32 = 1.3;
/// The search keeps within this many metres, horizontally, of the box
/// spanned by the start and the target, and within this many metres of
/// their heights (our choice).
pub const WINDOW_MARGIN: f32 = 30.0;
pub const WINDOW_HEIGHT: f32 = 8.0;
/// Extra seconds a jump, a drop and a ladder's ends cost (our choice: they
/// are slower and riskier than their length says).
pub const JUMP_PENALTY: f32 = 0.6;
pub const DROP_PENALTY: f32 = 0.3;
pub const LADDER_PENALTY: f32 = 1.5;
/// The longest straight leg [`Route`]'s waypoints are merged into
/// (our choice).
pub const MAX_LEG: f32 = 12.0;

/// How a waypoint is reached from the one before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// On foot standing, in the crouch, in the low crouch.
    Walk,
    Crouch,
    Crawl,
    /// Walking off an edge to a floor below.
    Drop,
    /// A running jump up onto a ledge.
    JumpUp,
    /// A running jump over a gap or a low obstacle.
    JumpOver,
    /// Up or down a ladder, from the floor in front of its foot to the
    /// floor beyond its top.
    ClimbUp,
    ClimbDown,
}

impl Kind {
    /// The posture the movement needs: 0 standing, 1 crouch, 2 low crouch.
    pub fn posture(self) -> usize {
        match self {
            Kind::Crouch => 1,
            Kind::Crawl => 2,
            _ => 0,
        }
    }

    fn of_posture(posture: u8) -> Self {
        match posture {
            0 => Kind::Walk,
            1 => Kind::Crouch,
            _ => Kind::Crawl,
        }
    }
}

/// What the planner takes from the player's movement (player-movement.md
/// sections 1, 4.4, 6.6 and 7, and level-cform.md 6.3; the margins are our
/// choices).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Box heights: standing, crouch, low crouch.
    pub heights: [f32; 3],
    /// Speeds on foot, m/s: running standing, running crouched, walking in
    /// the low crouch (0.1 × the acceleration, section 6.3).
    pub speeds: [f32; 3],
    pub climb_speed: f32,
    /// The highest step walked up (step assistance acts on edges up to
    /// about 0.7 m, level-cform.md 6.3; our margin).
    pub step: f32,
    /// The deepest drop walked off: the fall's speed stays well under
    /// `ph_crash_speed_min`.
    pub drop: f32,
    /// The jump's apex, `jump_speed² / 2g`, and with margins the highest
    /// ledge jumped up onto and obstacle top jumped over.
    pub apex: f32,
    pub jump_up: f32,
    pub jump_over: f32,
    /// The longest jump across, metres.
    pub jump_length: f32,
}

impl Body {
    pub fn of(c: &MovementConfig) -> Self {
        let heights = c.body.boxes.map(|b| 2.0 * b[1]);
        let run = 0.1 * c.walk_accel * c.run_coef;
        let speeds = [run, run * c.crouch_coef, 0.1 * c.walk_accel * c.crouch_coef];
        let apex = c.jump_speed * c.jump_speed / (2.0 * GRAVITY);
        let crash_drop = c.crash_speed_min * c.crash_speed_min / (2.0 * GRAVITY);
        // A running jump keeps 1.2 × the speed (6.6) over the flight back
        // to the take-off height, with half of it to spare.
        let flight = 2.0 * c.jump_speed / GRAVITY;
        Self {
            heights,
            speeds,
            climb_speed: run * c.climb_coef,
            step: 0.5,
            drop: (0.5 * crash_drop).min(3.0),
            apex,
            jump_up: 0.9 * apex,
            jump_over: 0.7 * apex,
            jump_length: 0.7 * 1.2 * run * flight,
        }
    }
}

/// A ladder's ends as a route needs them: its foot **B** and top **T**
/// (climbing.md 1.2) and the horizontal direction of its front, **N̂**.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LadderEnds {
    pub index: usize,
    pub lower: Vec3,
    pub upper: Vec3,
    pub out: Vec3,
}

/// A point of a route: the feet's place on a floor, how it is reached from
/// the waypoint before, and the ladder of a climb.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waypoint {
    pub at: Vec3,
    pub kind: Kind,
    pub ladder: Option<LadderEnds>,
}

/// A planned route: from the start floor (the first waypoint, kind
/// [`Kind::Walk`]) to the floor nearest the target the search reached.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub waypoints: Vec<Waypoint>,
    /// The planned time, seconds.
    pub cost: f32,
    /// The last waypoint is within [`GOAL_REACH`] of the target.
    pub complete: bool,
    pub stats: Stats,
}

/// What a search cost.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub expanded: usize,
    pub nodes: usize,
    pub columns: usize,
    /// Bytes the search held at its end (columns, nodes, keys, queue).
    pub bytes: usize,
    /// Box queries for clearance made (each column's floors, jumps).
    pub queries: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Floor {
    y: f32,
    /// The tallest posture that fits: 0 standing, 1 crouch, 2 low crouch,
    /// 3 none.
    room: u8,
}

const BLOCKED: u8 = 3;

type Key = (i32, i32, u8);

#[derive(Clone, Copy, Debug)]
struct Node {
    key: Key,
    y: f32,
    room: u8,
    g: f32,
    parent: u32,
    kind: Kind,
    ladder: Option<u16>,
    closed: bool,
    /// Room for a jump over the standing body: unknown, yes, no.
    headroom: Option<bool>,
}

#[derive(Clone, Copy, PartialEq)]
struct Open {
    f: f32,
    node: u32,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.total_cmp(&self.f).then(o.node.cmp(&self.node))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// An edge out of a floor: to the floor `to`, its kind, seconds, ladder.
#[derive(Clone, Copy, Debug)]
struct Link {
    to: Key,
    kind: Kind,
    cost: f32,
    ladder: Option<u16>,
}

/// The columns probed so far.
#[derive(Clone)]
struct Grid {
    body: Body,
    /// The vertical span columns are probed over.
    y: [f32; 2],
    columns: HashMap<(i32, i32), Vec<Floor>>,
    scratch: Vec<u32>,
    queries: usize,
}

fn cell_of(v: f32) -> i32 {
    (v / CELL).round() as i32
}

fn centre(ix: i32, iz: i32) -> (f32, f32) {
    (ix as f32 * CELL, iz as f32 * CELL)
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

impl Grid {
    fn new(body: Body, y: [f32; 2]) -> Self {
        Self {
            body,
            y,
            columns: HashMap::new(),
            scratch: Vec::new(),
            queries: 0,
        }
    }

    fn floors(&mut self, w: &CollisionWorld, ix: i32, iz: i32) -> &[Floor] {
        if !self.columns.contains_key(&(ix, iz)) {
            let floors = self.probe(w, ix, iz);
            self.columns.insert((ix, iz), floors);
        }
        &self.columns[&(ix, iz)]
    }

    fn floor(&mut self, w: &CollisionWorld, key: Key) -> Option<Floor> {
        self.floors(w, key.0, key.1).get(key.2 as usize).copied()
    }

    /// The floors of column (`ix`, `iz`), highest first, from one query
    /// of the box over the column's whole span.
    fn probe(&mut self, w: &CollisionWorld, ix: i32, iz: i32) -> Vec<Floor> {
        let (x, z) = centre(ix, iz);
        let mut found = std::mem::take(&mut self.scratch);
        found.clear();
        self.queries += 1;
        let lo = [x - CLEAR_HALF, self.y[0], z - CLEAR_HALF];
        let hi = [
            x + CLEAR_HALF,
            self.y[1] + self.body.heights[0],
            z + CLEAR_HALF,
        ];
        w.query(lo, hi, &mut found);
        found.retain(|&t| w.is_solid(t));
        let mut heights: Vec<f32> = Vec::new();
        for &t in &found {
            let c = w.corners(t);
            let Some(n) = front_normal(&c) else {
                continue;
            };
            if n[1] < WALKABLE_NORMAL_Y {
                continue;
            }
            let y = c[0][1] - (n[0] * (x - c[0][0]) + n[2] * (z - c[0][2])) / n[1];
            let on = closest_point([x, y, z], &c);
            if (on[0] - x).abs() < 1e-4
                && (on[2] - z).abs() < 1e-4
                && (self.y[0]..=self.y[1]).contains(&y)
            {
                heights.push(y);
            }
        }
        heights.sort_by(|a, b| b.total_cmp(a));
        heights.dedup_by(|a, b| (*b - *a).abs() < 0.05);
        let floors = heights
            .into_iter()
            .map(|y| Floor {
                y,
                room: room(w, &found, self.body.heights, x, y, z),
            })
            .collect();
        self.scratch = found;
        floors
    }

    /// Whether nothing solid lies in the box over (`x`, `z`) from height
    /// `span[0]` to `span[1]`.
    fn clear(&mut self, w: &CollisionWorld, x: f32, z: f32, span: [f32; 2]) -> bool {
        self.queries += 1;
        let lo = [x - CLEAR_HALF, span[0], z - CLEAR_HALF];
        let hi = [x + CLEAR_HALF, span[1], z + CLEAR_HALF];
        let mut found = std::mem::take(&mut self.scratch);
        found.clear();
        w.query(lo, hi, &mut found);
        let clear = !found
            .iter()
            .any(|&t| w.is_solid(t) && box_meets(x, z, span, &w.corners(t)))
            && !ladder_faces(w, lo, hi).any(|t| box_meets(x, z, span, t));
        self.scratch = found;
        clear
    }
}

/// The faces of the ladders' boxes whose bounds meet `lo`…`hi`.
fn ladder_faces(w: &CollisionWorld, lo: Vec3, hi: Vec3) -> impl Iterator<Item = &[Vec3; 3]> {
    w.ladders()
        .iter()
        .filter(move |l| (0..3).all(|k| l.min[k] <= hi[k] && l.max[k] >= lo[k]))
        .flat_map(|l| l.triangles.iter())
}

/// The tallest posture whose box fits over the floor at (`x`, `y`, `z`),
/// among the solid triangles `found` and the ladders' boxes.
fn room(w: &CollisionWorld, found: &[u32], heights: [f32; 3], x: f32, y: f32, z: f32) -> u8 {
    let base = y + CLEAR_BASE;
    let top = y + heights[0] + HEAD_MARGIN;
    let lo = [x - CLEAR_HALF, base, z - CLEAR_HALF];
    let hi = [x + CLEAR_HALF, top, z + CLEAR_HALF];
    let mut room = 0u8;
    let mut test = |t: &[Vec3; 3]| {
        // Lowest box first: once one is hit the taller ones are.
        for (posture, &h) in heights.iter().enumerate().rev() {
            if box_meets(x, z, [base, y + h + HEAD_MARGIN], t) {
                room = room.max(posture as u8 + 1);
                break;
            }
        }
        room == BLOCKED
    };
    for &t in found {
        let c = w.corners(t);
        let (tlo, thi) = crate::collision::bvh::triangle_box(&c);
        if thi[1] < base || tlo[1] > top {
            continue;
        }
        if test(&c) {
            return BLOCKED;
        }
    }
    for t in ladder_faces(w, lo, hi) {
        if test(t) {
            return BLOCKED;
        }
    }
    room
}

fn box_meets(x: f32, z: f32, span: [f32; 2], t: &[Vec3; 3]) -> bool {
    let centre = [x, 0.5 * (span[0] + span[1]), z];
    let half = [CLEAR_HALF, 0.5 * (span[1] - span[0]), CLEAR_HALF];
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    box_intersects(centre, axes, half, t)
}

const DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// A search from one start toward one target.
#[derive(Clone)]
struct Search {
    grid: Grid,
    target: Vec3,
    nodes: Vec<Node>,
    index: HashMap<Key, u32>,
    open: BinaryHeap<Open>,
    ladders: Vec<LadderEnds>,
    ladder_links: HashMap<Key, Vec<Link>>,
    window: [f32; 4],
}

impl Search {
    /// The node of floor `key`, made on first sight.
    fn node(&mut self, key: Key, floor: Floor) -> u32 {
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        let i = self.nodes.len() as u32;
        self.nodes.push(Node {
            key,
            y: floor.y,
            room: floor.room,
            g: f32::INFINITY,
            parent: u32::MAX,
            kind: Kind::Walk,
            ladder: None,
            closed: false,
            headroom: None,
        });
        self.index.insert(key, i);
        i
    }

    fn heuristic(&self, key: Key, y: f32) -> f32 {
        let (x, z) = centre(key.0, key.1);
        HEURISTIC_WEIGHT * horizontal_distance([x, y, z], self.target) / self.grid.body.speeds[0]
    }

    /// How far floor `key` at `y` is from the target: horizontally, and
    /// vertically beyond a standing body over it.
    fn goal_distance(&self, key: Key, y: f32) -> f32 {
        let (x, z) = centre(key.0, key.1);
        let t = self.target;
        let h = self.grid.body.heights[0];
        let v = if t[1] < y - 0.1 {
            y - 0.1 - t[1]
        } else if t[1] > y + h {
            t[1] - y - h
        } else {
            0.0
        };
        horizontal_distance([x, y, z], t).hypot(v)
    }

    fn in_window(&self, ix: i32, iz: i32) -> bool {
        let (x, z) = centre(ix, iz);
        let w = self.window;
        x >= w[0] && x <= w[1] && z >= w[2] && z <= w[3]
    }

    /// The floor of column (`ix`, `iz`) nearest `y` within `below` under
    /// and `above` over it that some posture fits, with its layer.
    fn floor_near(
        &mut self,
        w: &CollisionWorld,
        ix: i32,
        iz: i32,
        y: f32,
        below: f32,
        above: f32,
    ) -> Option<(u8, Floor)> {
        self.grid
            .floors(w, ix, iz)
            .iter()
            .enumerate()
            .filter(|(_, f)| f.room < BLOCKED && f.y >= y - below && f.y <= y + above)
            .min_by(|a, b| (a.1.y - y).abs().total_cmp(&(b.1.y - y).abs()))
            .map(|(i, f)| (i as u8, *f))
    }

    /// Whether a standing body over node `n` has room for a jump's rise.
    fn headroom(&mut self, w: &CollisionWorld, n: u32) -> bool {
        if let Some(h) = self.nodes[n as usize].headroom {
            return h;
        }
        let node = self.nodes[n as usize];
        let (x, z) = centre(node.key.0, node.key.1);
        let b = self.grid.body;
        let base = node.y + b.heights[0];
        let h = self.grid.clear(w, x, z, [base, base + b.apex]);
        self.nodes[n as usize].headroom = Some(h);
        h
    }

    /// The edges out of node `n`.
    fn links(&mut self, w: &CollisionWorld, n: u32, out: &mut Vec<Link>) {
        out.clear();
        let node = self.nodes[n as usize];
        let (ix, iz, _) = node.key;
        let y = node.y;
        let b = self.grid.body;
        for (dx, dz) in DIRECTIONS {
            let (jx, jz) = (ix + dx, iz + dz);
            if !self.in_window(jx, jz) {
                continue;
            }
            let diagonal = dx != 0 && dz != 0;
            let length = CELL
                * if diagonal {
                    std::f32::consts::SQRT_2
                } else {
                    1.0
                };
            let floors = self.grid.floors(w, jx, jz).to_vec();
            let mut level = false;
            for (layer, f) in floors.iter().enumerate() {
                if f.room == BLOCKED {
                    continue;
                }
                let to = (jx, jz, layer as u8);
                let dy = f.y - y;
                let posture = node.room.max(f.room);
                if (-b.step..=b.step).contains(&dy) {
                    // Diagonals don't cut corners: both sides pass too.
                    if diagonal
                        && [(ix + dx, iz), (ix, iz + dz)].iter().any(|&(cx, cz)| {
                            self.floor_near(w, cx, cz, y.max(f.y), b.step, b.step)
                                .is_none_or(|(_, s)| s.room > posture)
                        })
                    {
                        continue;
                    }
                    level = true;
                    out.push(Link {
                        to,
                        kind: Kind::of_posture(posture),
                        cost: length / b.speeds[posture as usize],
                        ladder: None,
                    });
                } else if dy < -b.step && dy >= -b.drop && !diagonal {
                    let fall = (2.0 * -dy / GRAVITY).sqrt();
                    out.push(Link {
                        to,
                        kind: Kind::Drop,
                        cost: length / b.speeds[posture as usize] + fall + DROP_PENALTY,
                        ladder: None,
                    });
                } else if dy > b.step
                    && dy <= b.jump_up
                    && node.room == 0
                    && f.room == 0
                    && !diagonal
                    && self.headroom(w, n)
                {
                    out.push(Link {
                        to,
                        kind: Kind::JumpUp,
                        cost: length / b.speeds[0] + JUMP_PENALTY,
                        ladder: None,
                    });
                }
            }
            if !level && node.room == 0 && self.headroom(w, n) {
                self.jump_over(w, node, (dx, dz), out);
            }
        }
        if let Some(l) = self.ladder_links.get(&node.key) {
            out.extend(l.iter().copied());
        }
    }

    /// A running jump from `node` along (`dx`, `dz`) over the columns
    /// between to a floor at most a step above and a drop below, within
    /// [`Body::jump_length`].
    fn jump_over(
        &mut self,
        w: &CollisionWorld,
        node: Node,
        (dx, dz): (i32, i32),
        out: &mut Vec<Link>,
    ) {
        let b = self.grid.body;
        let (ix, iz, _) = node.key;
        let step = CELL * (dx as f32).hypot(dz as f32);
        let span = [node.y + b.jump_over, node.y + b.jump_over + b.heights[0]];
        for k in 2.. {
            if k as f32 * step > b.jump_length {
                return;
            }
            let (jx, jz) = (ix + dx * k, iz + dz * k);
            // The column passed over before this one is clear above the
            // jump's height.
            let (px, pz) = centre(ix + dx * (k - 1), iz + dz * (k - 1));
            if !self.grid.clear(w, px, pz, span) || !self.in_window(jx, jz) {
                return;
            }
            let landing = self
                .grid
                .floors(w, jx, jz)
                .iter()
                .enumerate()
                .find(|(_, f)| f.room == 0 && (-b.drop..=b.step).contains(&(f.y - node.y)))
                .map(|(i, f)| (i as u8, *f));
            if let Some((layer, _)) = landing {
                out.push(Link {
                    to: (jx, jz, layer),
                    kind: Kind::JumpOver,
                    cost: k as f32 * step / (1.2 * b.speeds[0]) + JUMP_PENALTY,
                    ladder: None,
                });
                return;
            }
        }
    }

    /// Joins the floors at the ends of the ladders near the window.
    fn join_ladders(&mut self, w: &CollisionWorld) {
        let b = self.grid.body;
        for (index, l) in w.ladders().iter().enumerate() {
            let v = self.window;
            if l.max[0] < v[0] || l.min[0] > v[1] || l.max[2] < v[2] || l.min[2] > v[3] {
                continue;
            }
            let n = l.normal_dir;
            let h = n[0].hypot(n[2]);
            if h < 1e-3 {
                continue;
            }
            let out = [n[0] / h, 0.0, n[2] / h];
            // The foot: the floor in front of B.
            let at = |p: Vec3, d: f32| (cell_of(p[0] + out[0] * d), cell_of(p[2] + out[2] * d));
            let bottom = [0.5f32, 0.7, 0.9].into_iter().find_map(|d| {
                let (cx, cz) = at(l.lower, d);
                self.floor_near(w, cx, cz, l.lower[1], 0.6, 0.6)
                    .filter(|(_, f)| f.room == 0)
                    .map(|(layer, _)| (cx, cz, layer))
            });
            // The top: a floor beyond T, behind the ladder (or in front).
            let top = [-0.6f32, -0.9, -1.2, 0.6, 0.9].into_iter().find_map(|d| {
                let (cx, cz) = at(l.upper, d);
                self.floor_near(w, cx, cz, l.upper[1], 0.7, 0.4)
                    .filter(|(_, f)| f.room == 0)
                    .map(|(layer, _)| (cx, cz, layer))
            });
            let (Some(bottom), Some(top)) = (bottom, top) else {
                continue;
            };
            let k = self.ladders.len() as u16;
            self.ladders.push(LadderEnds {
                index,
                lower: l.lower,
                upper: l.upper,
                out,
            });
            let cost =
                2.0 * crate::collision::geometry::length(l.axis) / b.climb_speed + LADDER_PENALTY;
            for (from, to, kind) in [(bottom, top, Kind::ClimbUp), (top, bottom, Kind::ClimbDown)] {
                self.ladder_links.entry(from).or_default().push(Link {
                    to,
                    kind,
                    cost,
                    ladder: Some(k),
                });
            }
        }
    }

    fn bytes(&self) -> usize {
        let floors: usize = self.grid.columns.values().map(|v| v.capacity()).sum();
        self.grid.columns.capacity() * (std::mem::size_of::<((i32, i32), Vec<Floor>)>() + 1)
            + floors * std::mem::size_of::<Floor>()
            + self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.index.capacity() * (std::mem::size_of::<(Key, u32)>() + 1)
            + self.open.capacity() * std::mem::size_of::<Open>()
    }
}

/// A search under way: [`Planning::run`] expands it by parts, so a long
/// one spreads over frames.
#[derive(Clone)]
pub struct Planning {
    s: Search,
    from: Vec3,
    target: Vec3,
    first: u32,
    best: (f32, u32),
    best_at: usize,
    expanded: usize,
    links: Vec<Link>,
}

impl std::fmt::Debug for Planning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Planning")
            .field("from", &self.from)
            .field("target", &self.target)
            .field("expanded", &self.expanded)
            .finish()
    }
}

impl PartialEq for Planning {
    /// Searches compare by where they go: they are never equal in their
    /// state.
    fn eq(&self, o: &Self) -> bool {
        std::ptr::eq(self, o)
    }
}

impl Planning {
    /// A search in `world` for `body` from the feet at `from` toward the
    /// reachable floor nearest `target`; `None` with no floor under or
    /// beside the feet.
    pub fn new(world: &CollisionWorld, body: &Body, from: Vec3, target: Vec3) -> Option<Self> {
        let y = [
            from[1].min(target[1]) - WINDOW_HEIGHT,
            from[1].max(target[1]) + WINDOW_HEIGHT,
        ];
        let window = [
            from[0].min(target[0]) - WINDOW_MARGIN,
            from[0].max(target[0]) + WINDOW_MARGIN,
            from[2].min(target[2]) - WINDOW_MARGIN,
            from[2].max(target[2]) + WINDOW_MARGIN,
        ];
        // A target on a wall is met from the side it was seen from.
        let back = [from[0] - target[0], from[2] - target[2]];
        let l = back[0].hypot(back[1]);
        let seen = if l > GOAL_PULL {
            [
                target[0] + back[0] / l * GOAL_PULL,
                target[1],
                target[2] + back[1] / l * GOAL_PULL,
            ]
        } else {
            target
        };
        let mut s = Search {
            grid: Grid::new(*body, y),
            target: seen,
            nodes: Vec::new(),
            index: HashMap::new(),
            open: BinaryHeap::new(),
            ladders: Vec::new(),
            ladder_links: HashMap::new(),
            window,
        };
        // The start: the floor under the feet, or beside them.
        let (sx, sz) = (cell_of(from[0]), cell_of(from[2]));
        let start = std::iter::once((0, 0))
            .chain(DIRECTIONS)
            .find_map(|(dx, dz)| {
                s.floor_near(world, sx + dx, sz + dz, from[1], 1.2, 0.5)
                    .map(|(layer, f)| ((sx + dx, sz + dz, layer), f))
            })?;
        s.join_ladders(world);
        let first = s.node(start.0, start.1);
        s.nodes[first as usize].g = 0.0;
        let h = s.heuristic(start.0, start.1.y);
        s.open.push(Open { f: h, node: first });
        let best = (s.goal_distance(start.0, start.1.y), first);
        Some(Self {
            s,
            from,
            target,
            first,
            best,
            best_at: 0,
            expanded: 0,
            links: Vec::new(),
        })
    }

    /// Expands at most `budget` more floors in `world` (the one the search
    /// began in). `None` while the search goes on; else the route, `None`
    /// when nothing nearer the target than the start is reachable.
    pub fn run(&mut self, world: &CollisionWorld, budget: usize) -> Option<Option<Route>> {
        let s = &mut self.s;
        let mut complete = false;
        let mut left = budget;
        while let Some(Open { node: n, .. }) = s.open.pop() {
            if s.nodes[n as usize].closed {
                continue;
            }
            s.nodes[n as usize].closed = true;
            let node = s.nodes[n as usize];
            let d = s.goal_distance(node.key, node.y);
            if d < self.best.0 {
                self.best = (d, n);
                self.best_at = self.expanded;
            }
            if d <= GOAL_REACH {
                complete = true;
                break;
            }
            self.expanded += 1;
            if self.expanded >= MAX_EXPANSIONS || self.expanded - self.best_at > STALL_EXPANSIONS {
                break;
            }
            s.links(world, n, &mut self.links);
            for link in &self.links {
                let Some(floor) = s.grid.floor(world, link.to) else {
                    continue;
                };
                let m = s.node(link.to, floor);
                let g = node.g + link.cost;
                let to = &mut s.nodes[m as usize];
                if to.closed || g >= to.g {
                    continue;
                }
                to.g = g;
                to.parent = n;
                to.kind = link.kind;
                to.ladder = link.ladder;
                let f = g + s.heuristic(link.to, floor.y);
                s.open.push(Open { f, node: m });
            }
            left -= 1;
            if left == 0 {
                return None;
            }
        }
        Some(self.route(world, complete))
    }

    /// The expansions made so far.
    pub fn expanded(&self) -> usize {
        self.expanded
    }

    fn route(&mut self, world: &CollisionWorld, complete: bool) -> Option<Route> {
        let s = &mut self.s;
        let stats = Stats {
            expanded: self.expanded,
            nodes: s.nodes.len(),
            columns: s.grid.columns.len(),
            bytes: s.bytes(),
            queries: s.grid.queries,
        };
        let goal = self.best.1;
        if goal == self.first {
            return None;
        }
        // Short of the target, only a floor near it and no detour.
        let straight = horizontal_distance(self.from, self.target) / s.grid.body.speeds[0];
        if !complete
            && (self.best.0 > NEAREST_REACH
                || s.nodes[goal as usize].g > DETOUR_FACTOR * straight + DETOUR_SLACK)
        {
            return None;
        }
        let mut chain = Vec::new();
        let mut n = goal;
        while n != u32::MAX {
            chain.push(s.nodes[n as usize]);
            n = s.nodes[n as usize].parent;
        }
        chain.reverse();
        let cost = chain.last().map_or(0.0, |n| n.g);
        let mut waypoints: Vec<Waypoint> = chain
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let (x, z) = centre(n.key.0, n.key.1);
                Waypoint {
                    at: [x, n.y, z],
                    kind: if i == 0 { Kind::Walk } else { n.kind },
                    ladder: n.ladder.map(|k| s.ladders[k as usize]),
                }
            })
            .collect();
        // The ends exactly: the feet, and the target where it is on a floor
        // the last floor leads straight to.
        waypoints[0].at = self.from;
        let last = *waypoints.last().unwrap();
        let target = self.target;
        if complete && matches!(last.kind, Kind::Walk | Kind::Crouch | Kind::Crawl) {
            let floor = s
                .grid
                .floors(world, cell_of(target[0]), cell_of(target[2]))
                .iter()
                .find(|f| (f.y - target[1]).abs() <= 0.3)
                .copied();
            if let Some(f) = floor {
                let at = [target[0], f.y, target[2]];
                if line_passes(world, &mut s.grid, last.at, at, last.kind.posture() as u8) {
                    waypoints.push(Waypoint { at, ..last });
                }
            }
        }
        let waypoints = straighten(world, &mut s.grid, waypoints);
        Some(Route {
            waypoints,
            cost,
            complete,
            stats,
        })
    }
}

/// A route from the feet at `from` to the reachable floor nearest
/// `target` in `world` for `body`, searched at once; `None` when no floor
/// is under the feet or nothing nearer the target is reachable.
pub fn plan(world: &CollisionWorld, body: &Body, from: Vec3, target: Vec3) -> Option<Route> {
    Planning::new(world, body, from, target)?
        .run(world, usize::MAX)
        .flatten()
}

/// Merges runs of waypoints reached on foot at one posture into straight
/// legs where the line between passes over floors of that posture's room
/// with no step above [`Body::step`] (within [`MAX_LEG`]).
fn straighten(world: &CollisionWorld, grid: &mut Grid, w: Vec<Waypoint>) -> Vec<Waypoint> {
    let on_foot = |k: Kind| matches!(k, Kind::Walk | Kind::Crouch | Kind::Crawl);
    let mut out = vec![w[0]];
    let mut i = 0;
    while i + 1 < w.len() {
        let mut j = i + 1;
        if on_foot(w[j].kind) {
            let posture = w[j].kind.posture();
            while j + 1 < w.len()
                && on_foot(w[j + 1].kind)
                && w[j + 1].kind.posture() == posture
                && horizontal_distance(w[i].at, w[j + 1].at) <= MAX_LEG
                && line_passes(world, grid, w[i].at, w[j + 1].at, posture as u8)
            {
                j += 1;
            }
        }
        out.push(w[j]);
        i = j;
    }
    out
}

/// Whether the straight line from `a` to `b` passes over floors with room
/// for `posture`, each within [`Body::step`] of the one before, and so do
/// the lines half a cell to either side, so the body's width passes.
fn line_passes(world: &CollisionWorld, grid: &mut Grid, a: Vec3, b: Vec3, posture: u8) -> bool {
    let d = sub(b, a);
    let length = d[0].hypot(d[2]);
    let samples = (length / (0.5 * CELL)).ceil().max(1.0) as usize;
    let side = if length > 0.0 {
        [-d[2] / length * 0.5 * CELL, d[0] / length * 0.5 * CELL]
    } else {
        [0.0; 2]
    };
    let step = grid.body.step;
    for offset in [0.0f32, 1.0, -1.0] {
        let mut y = a[1];
        for i in 0..=samples {
            let t = i as f32 / samples as f32;
            let x = a[0] + d[0] * t + side[0] * offset;
            let z = a[2] + d[2] * t + side[1] * offset;
            let Some(f) = grid
                .floors(world, cell_of(x), cell_of(z))
                .iter()
                .filter(|f| (f.y - y).abs() <= step)
                .min_by(|p, q| (p.y - y).abs().total_cmp(&(q.y - y).abs()))
                .copied()
            else {
                return false;
            };
            if f.room > posture {
                return false;
            }
            y = f.y;
        }
        if (y - b[1]).abs() > step {
            return false;
        }
    }
    true
}

pub mod follow;
