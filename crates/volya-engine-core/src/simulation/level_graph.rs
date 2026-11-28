// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level graph at run time (docs/behavior/level-graph.md, cited by
//! section): the neighbours of a vertex (2.1), the accessibility bits
//! (2.2), the straight-line tests over linked cells (3.1–3.4) and
//! the two searches (4.1, 4.2), over the current level's `level.ai`
//! positions and links ([`super::navigation::Navigation`]). The lookup of
//! a vertex for a point (2.3) is [`super::navigation::Navigation::vertex_for_point`].

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use volya_engine_formats::level_ai::{LevelAi, LevelAiLinks};

#[cfg(test)]
mod tests;

/// The widening of a cell's sides in the crossing test (3.1).
const CROSSING_SLACK: f32 = 1e-5;
/// The step test's reach, in cells (3.4).
const STEP_TEST_CELLS: f32 = 4.0;

/// The accessibility bits of the loaded level's vertices (2.2): all set
/// when the graph is created, cleared for a creature's restriction list
/// just before a test made for it and set again just after it. Kept as
/// the cleared bits, allocated on the first clearing (ours: until then
/// every vertex is accessible and nothing is held).
#[derive(Clone, Debug, Default)]
pub struct Accessibility {
    cleared: Vec<u64>,
}

impl Accessibility {
    /// Whether vertex `k`'s bit is set.
    pub fn is_set(&self, k: u32) -> bool {
        let (word, bit) = (k as usize / 64, k % 64);
        self.cleared.get(word).is_none_or(|w| w & (1 << bit) == 0)
    }

    /// Clears the bits of `vertices`.
    pub fn clear(&mut self, vertices: &[u32]) {
        for &k in vertices {
            let (word, bit) = (k as usize / 64, k % 64);
            if self.cleared.len() <= word {
                self.cleared.resize(word + 1, 0);
            }
            self.cleared[word] |= 1 << bit;
        }
    }

    /// Sets the bits of `vertices` again.
    pub fn set(&mut self, vertices: &[u32]) {
        for &k in vertices {
            if let Some(w) = self.cleared.get_mut(k as usize / 64) {
                *w &= !(1 << (k % 64));
            }
        }
    }
}

/// The limits of a search (4.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SearchLimits {
    /// The largest estimated length: the search fails when the best
    /// waiting vertex's sum is not below it.
    pub length: f32,
    /// The largest number of steps.
    pub steps: u32,
    /// The largest number of search entries (every vertex ever reached,
    /// the start included).
    pub entries: u32,
}

impl SearchLimits {
    /// The limits for creatures' movement (4.1).
    pub const CREATURE_MOVEMENT: Self = Self {
        length: f32::MAX,
        steps: u32::MAX,
        entries: 65_500,
    };

    /// The nearest-vertex search's one caller's limits (4.2): no length
    /// limit.
    pub const NEAREST_VERTEX: Self = Self {
        length: f32::INFINITY,
        steps: u32::MAX,
        entries: u32::MAX,
    };
}

/// What a level path search (4.1) gives.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelPathSearch {
    /// The level path from the start to the goal, each vertex linked from
    /// the one before; `None` on failure.
    pub path: Option<Vec<u32>>,
    /// The search entries made.
    pub entries: u32,
    /// The steps made.
    pub steps: u32,
}

/// The level graph of the loaded level: its vertices' positions and
/// links and their accessibility bits.
#[derive(Clone, Copy)]
pub struct LevelGraph<'a> {
    pub ai: &'a LevelAi,
    pub links: &'a LevelAiLinks,
    pub accessibility: &'a Accessibility,
}

impl<'a> LevelGraph<'a> {
    /// The vertex count *N*.
    pub fn len(&self) -> u32 {
        self.ai.len().min(self.links.len()) as u32
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether `k` is a valid vertex (below *N*).
    pub fn valid(&self, k: u32) -> bool {
        k < self.len()
    }

    /// Whether `k` is accessible (2.2): valid and its bit set.
    pub fn accessible(&self, k: u32) -> bool {
        self.valid(k) && self.accessibility.is_set(k)
    }

    /// The neighbours of `k` (2.1): the valid vertices its four links name,
    /// in link order (−x, +z, +x, −z). None for an invalid `k`.
    pub fn neighbours(&self, k: u32) -> impl Iterator<Item = u32> + 'a {
        let n = self.len();
        let links = if k < n { self.links.links(k) } else { None };
        links.into_iter().flatten().filter(move |&l| l < n)
    }

    /// The column and row of `k` (its packed value's quotient and
    /// remainder by the row length *r*).
    fn column_row(&self, k: u32) -> (i64, i64) {
        let p = self.ai.packed(k).unwrap_or(0);
        let r = self.ai.header.row_length().max(1);
        (i64::from(p / r), i64::from(p % r))
    }

    /// The x and z of `k`'s centre.
    fn centre(&self, k: u32) -> [f32; 2] {
        let v = self.ai.position(k).unwrap_or([f32::NAN; 3]);
        [v[0], v[2]]
    }

    /// Whether the line through `a` with direction `d` (both x–z) crosses
    /// `k`'s cell (3.1).
    pub fn crosses(&self, k: u32, a: [f32; 2], d: [f32; 2]) -> bool {
        let [x, z] = self.centre(k);
        let half = self.ai.header.cell_size / 2.0;
        let (x0, x1) = (x - half - a[0], x + half - a[0]);
        let (z0, z1) = (z - half - a[1], z + half - a[1]);
        let within =
            |v: f32, lo: f32, hi: f32| v >= lo - CROSSING_SLACK && v <= hi + CROSSING_SLACK;
        if d[0] != 0.0 && [x0, x1].iter().any(|&s| within(s * d[1] / d[0], z0, z1)) {
            return true;
        }
        d[1] != 0.0 && [z0, z1].iter().any(|&s| within(s * d[0] / d[1], x0, x1))
    }

    /// The walk to a point (3.2) from vertex `k0` and point `a` to point
    /// `b` (x and z used): the vertex whose cell contains `b`, or `None`.
    /// `k0`'s own cell is not compared with `b`'s.
    pub fn walk_to_point(&self, k0: u32, a: [f32; 3], b: [f32; 3]) -> Option<u32> {
        self.walk_to_point_visiting(k0, a, b, |_| {})
    }

    /// [`Self::walk_to_point`], calling `visit` with each vertex the walk
    /// is at, `k0` first. The walk moves at most *N* times (our guard).
    pub fn walk_to_point_visiting(
        &self,
        k0: u32,
        a: [f32; 3],
        b: [f32; 3],
        visit: impl FnMut(u32),
    ) -> Option<u32> {
        // 1.
        let goal_cell = self.ai.packed_at(b)?;
        self.walk(
            k0,
            [a[0], a[2]],
            [b[0], b[2]],
            |next| self.ai.packed(next) == Some(goal_cell),
            visit,
        )
    }

    /// The walk to a vertex (3.3) from vertex `k0` and point `a` (x and z
    /// used) to vertex `ke`: whether it reaches `ke`, accessible. `k0` is
    /// not compared with `ke`.
    pub fn walk_to_vertex(&self, k0: u32, a: [f32; 3], ke: u32) -> bool {
        self.walk_to_vertex_visiting(k0, a, ke, |_| {})
    }

    /// [`Self::walk_to_vertex`], calling `visit` with each vertex the walk
    /// is at, `k0` first.
    pub fn walk_to_vertex_visiting(
        &self,
        k0: u32,
        a: [f32; 3],
        ke: u32,
        visit: impl FnMut(u32),
    ) -> bool {
        if !self.valid(ke) {
            return false;
        }
        self.walk(k0, [a[0], a[2]], self.centre(ke), |next| next == ke, visit)
            .is_some()
    }

    /// The walk of 3.2 (steps 2 and 3) from vertex `k0` along the line
    /// from `a` through `b` (x–z), distances measured to `b`, until a
    /// crossed neighbour `ends` it: that neighbour if accessible.
    fn walk(
        &self,
        k0: u32,
        a: [f32; 2],
        b: [f32; 2],
        ends: impl Fn(u32) -> bool,
        mut visit: impl FnMut(u32),
    ) -> Option<u32> {
        if !self.valid(k0) {
            return None;
        }
        let d = [b[0] - a[0], b[1] - a[1]];
        let to_b = |c: [f32; 2]| {
            let (dx, dz) = (c[0] - b[0], c[1] - b[1]);
            dx * dx + dz * dz
        };
        // 2.
        let (mut at, mut came_from) = (k0, None);
        let mut best = to_b(self.centre(k0));
        visit(at);
        // 3.
        for _ in 0..self.len() {
            let mut taken = None;
            for next in self.neighbours(at) {
                if Some(next) == came_from || !self.crosses(next, a, d) {
                    continue;
                }
                if ends(next) {
                    if !self.accessible(next) {
                        return None;
                    }
                    visit(next);
                    return Some(next);
                }
                let distance = to_b(self.centre(next));
                if distance > best {
                    continue;
                }
                taken = Some((next, distance));
                break;
            }
            let (next, distance) = taken?;
            if !self.accessible(next) {
                return None;
            }
            came_from = Some(at);
            at = next;
            best = distance;
            visit(at);
        }
        None
    }

    /// The step test in direction `direction` (x and z used) from vertex
    /// `k` (3.4).
    pub fn step_test(&self, k: u32, direction: [f32; 3]) -> bool {
        if !self.valid(k) {
            return false;
        }
        let mut d = [direction[0], direction[2]];
        let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if length > 0.0 {
            d = [d[0] / length, d[1] / length];
        }
        let c = self.centre(k);
        let reach = STEP_TEST_CELLS * self.ai.header.cell_size;
        let e = [c[0] + d[0] * reach, c[1] + d[1] * reach];
        let to_e = |p: [f32; 2]| {
            let (dx, dz) = (p[0] - e[0], p[1] - e[1]);
            dx * dx + dz * dz
        };
        let own = to_e(c);
        self.neighbours(k).any(|next| {
            self.accessible(next) && self.crosses(next, c, d) && to_e(self.centre(next)) <= own
        })
    }

    /// The level path search (4.1) from `start` to `goal`. An invalid
    /// start or goal fails without searching (our guard).
    pub fn level_path(&self, start: u32, goal: u32, limits: SearchLimits) -> LevelPathSearch {
        if !self.valid(start) || !self.valid(goal) {
            return LevelPathSearch {
                path: None,
                entries: 0,
                steps: 0,
            };
        }
        let (gc, gr) = self.column_row(goal);
        let cell = self.ai.header.cell_size;
        let estimate = |k: u32| {
            let (c, r) = self.column_row(k);
            2.0 * cell * ((c - gc).abs() + (r - gr).abs()) as f32
        };
        let mut search = Search::new(start, estimate(start));
        let found = search.run(self, limits, Some(goal), |_| true, estimate, |_| {});
        let path = found.then(|| search.chain(goal));
        LevelPathSearch {
            path,
            entries: search.entries(),
            steps: search.steps,
        }
    }

    /// The nearest-vertex search (4.2) from `start` within `radius`
    /// metres for `target` (x and z used): the vertex nearest the target
    /// of those it reaches. `None` only for an invalid `start`.
    pub fn nearest_vertex(
        &self,
        start: u32,
        radius: f32,
        target: [f32; 3],
        limits: SearchLimits,
    ) -> Option<u32> {
        if !self.valid(start) {
            return None;
        }
        let cell = self.ai.header.cell_size;
        let bound = (radius * radius / (cell * cell) + 0.5) as i64;
        let (sc, sr) = self.column_row(start);
        let within = |k: u32| {
            let (c, r) = self.column_row(k);
            let (dc, dr) = (c - sc, r - sr);
            dc * dc + dr * dr <= bound
        };
        let to_target = |k: u32| {
            let [x, z] = self.centre(k);
            let (dx, dz) = (x - target[0], z - target[2]);
            dx * dx + dz * dz
        };
        let mut best: Option<(f32, u32)> = None;
        let mut search = Search::new(start, 0.0);
        search.run(
            self,
            limits,
            None,
            within,
            |_| 0.0,
            |k| {
                let d = to_target(k);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, k));
                }
            },
        );
        best.map(|(_, k)| k)
    }
}

/// A vertex reached by a search.
struct Reached {
    /// The length *g* from the start.
    length: f32,
    /// The estimate *e*, given when first reached.
    estimate: f32,
    /// The vertex it was last reached through (the start: itself).
    through: u32,
    /// The number of its last entry into the waiting list; `None` once
    /// done.
    entry: Option<u64>,
}

/// A waiting list entry: least sum first, of equal sums the last entered.
#[derive(PartialEq)]
struct Waiting {
    sum: f32,
    entry: u64,
    vertex: u32,
}

impl Eq for Waiting {}

impl Ord for Waiting {
    fn cmp(&self, other: &Self) -> Ordering {
        (other.sum.total_cmp(&self.sum)).then(self.entry.cmp(&other.entry))
    }
}

impl PartialOrd for Waiting {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The best-first search of 4.1, shared by 4.2.
struct Search {
    reached: HashMap<u32, Reached>,
    waiting: BinaryHeap<Waiting>,
    entered: u64,
    steps: u32,
}

impl Search {
    fn new(start: u32, estimate: f32) -> Self {
        let mut search = Self {
            reached: HashMap::new(),
            waiting: BinaryHeap::new(),
            entered: 0,
            steps: 0,
        };
        search.reached.insert(
            start,
            Reached {
                length: 0.0,
                estimate,
                through: start,
                entry: None,
            },
        );
        search.enter(start);
        search
    }

    fn entries(&self) -> u32 {
        self.reached.len() as u32
    }

    /// Puts `k` (reached) into the waiting list again.
    fn enter(&mut self, k: u32) {
        let r = self.reached.get_mut(&k).expect("reached");
        r.entry = Some(self.entered);
        self.waiting.push(Waiting {
            sum: r.length + r.estimate,
            entry: self.entered,
            vertex: k,
        });
        self.entered += 1;
    }

    /// The best waiting vertex and its sum, dropping stale entries.
    fn best(&mut self) -> Option<(u32, f32)> {
        while let Some(w) = self.waiting.peek() {
            let current = self.reached.get(&w.vertex).and_then(|r| r.entry);
            if current == Some(w.entry) {
                return Some((w.vertex, w.sum));
            }
            self.waiting.pop();
        }
        None
    }

    /// Runs until `goal` is taken (true) or the search fails (false);
    /// without a goal, until no vertex is waiting or a limit is reached.
    /// A neighbour is reached only when accessible and `admit`ted; its
    /// estimate is `estimate` of the vertex being expanded; `take` sees each
    /// vertex as it is taken.
    fn run(
        &mut self,
        graph: &LevelGraph,
        limits: SearchLimits,
        goal: Option<u32>,
        admit: impl Fn(u32) -> bool,
        estimate: impl Fn(u32) -> f32,
        mut take: impl FnMut(u32),
    ) -> bool {
        let cell = graph.ai.header.cell_size;
        loop {
            let Some((k, sum)) = self.best() else {
                return false;
            };
            if sum >= limits.length
                || self.steps >= limits.steps
                || self.entries() >= limits.entries
            {
                return false;
            }
            take(k);
            if Some(k) == goal {
                return true;
            }
            self.waiting.pop();
            let length = {
                let r = self.reached.get_mut(&k).expect("reached");
                r.entry = None;
                r.length + cell
            };
            self.steps += 1;
            for next in graph.neighbours(k) {
                if !graph.accessible(next) || !admit(next) {
                    continue;
                }
                match self.reached.get_mut(&next) {
                    None => {
                        let e = estimate(k);
                        self.reached.insert(
                            next,
                            Reached {
                                length,
                                estimate: e,
                                through: k,
                                entry: None,
                            },
                        );
                        self.enter(next);
                    }
                    Some(r) if r.entry.is_some() && length < r.length => {
                        r.length = length;
                        r.through = k;
                        self.enter(next);
                    }
                    Some(_) => {}
                }
            }
        }
    }

    /// The chain of vertices from the start through which `goal` was last
    /// reached.
    fn chain(&self, goal: u32) -> Vec<u32> {
        let mut path = vec![goal];
        let mut at = goal;
        while let Some(r) = self.reached.get(&at)
            && r.through != at
        {
            at = r.through;
            path.push(at);
        }
        path.reverse();
        path
    }
}

/// The movement's level-path step (4.1): the level path search with the
/// limits for creatures' movement, remembering the start and goal of the
/// last search that failed. A new one is made when the creature's
/// movement is reinitialised.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelPathStep {
    last_failure: Option<(u32, u32)>,
}

impl LevelPathStep {
    /// The level path from `start` to `goal`, or `None`: without
    /// searching when the last search that failed had the same start and
    /// goal.
    pub fn search(&mut self, graph: &LevelGraph, start: u32, goal: u32) -> Option<Vec<u32>> {
        if self.last_failure == Some((start, goal)) {
            return None;
        }
        let path = graph
            .level_path(start, goal, SearchLimits::CREATURE_MOVEMENT)
            .path;
        if path.is_none() {
            self.last_failure = Some((start, goal));
        }
        path
    }
}
