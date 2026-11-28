// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The current level's navigation data (docs/behavior/offline-simulation.md
//! sections 2, 3, 6.1): its `level.ai` positions and its cross table, with
//! the checks of 1.8, the level vertex at a position (5.2) and the one an
//! object coming online takes (docs/behavior/object-visual-and-transform.md
//! 4.4). Both are
//! held in the file cache, standing for the files the original maps
//! (docs/architecture/memory.md, File-backed data), each a leaner
//! structure under its file's path: the positions, links and cover values
//! under `level.ai`'s, the cross table's distinct cells and their indices
//! under `level.gct`'s.

use std::io::{self, BufReader};
use std::sync::Arc;

use crate::file_cache;
use crate::vfs::Vfs;
use volya_engine_formats::cross_table::CrossTable;
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::level_ai::{LevelAi, LevelAiLinks};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A level's navigation data.
#[derive(Debug)]
pub struct Navigation {
    pub level_id: u8,
    /// Shared with the scripts (script-api-level.md 3).
    pub ai: Arc<LevelAi>,
    /// Each vertex's links and cover values, as [`Navigation::read`] reads
    /// them, for the scripts (script-api-level.md 3.1); `None` otherwise.
    pub links: Option<Arc<LevelAiLinks>>,
    pub cross: CrossTable,
    /// Byte 11 of each `level.ai` vertex, as [`Navigation::read`] reads
    /// it, for the scripts (script-api-game-object-basics.md 4); empty
    /// otherwise, or once taken.
    pub vertex_lights: Vec<u8>,
    /// Level vertices in increasing packed column and row, when the file
    /// does not store them so; `None` when it does.
    order: Option<Vec<u32>>,
}

impl Navigation {
    /// The navigation data of level `level_id` with the checks of 1.8 and
    /// section 2: the cross table's game identifier is the graph's, its
    /// level identifier and the graph's level record's are the AI map's,
    /// and its level vertex count is the AI map's (our check).
    pub fn new(
        graph: &GameGraph,
        level_id: u8,
        ai: LevelAi,
        cross: CrossTable,
    ) -> io::Result<Self> {
        if cross.game_identifier != graph.identifier {
            return Err(invalid("graph doesn't correspond to the cross table"));
        }
        if cross.level_identifier != ai.header.identifier {
            return Err(invalid("cross_table doesn't correspond to the AI-map"));
        }
        let level = graph
            .level(level_id)
            .ok_or_else(|| invalid("Graph point level ID not found!"))?;
        if level.identifier != ai.header.identifier {
            return Err(invalid("graph doesn't correspond to the AI-map"));
        }
        if cross.len() != ai.len() {
            return Err(invalid(format!(
                "cross table has {} level vertices, level.ai {}",
                cross.len(),
                ai.len()
            )));
        }
        let sorted = (ai.packed_all())
            .zip(ai.packed_all().skip(1))
            .all(|(a, b)| a <= b);
        let order = (!sorted).then(|| {
            let mut order: Vec<u32> = (0..ai.len() as u32).collect();
            order.sort_by_key(|&k| ai.packed(k));
            order
        });
        Ok(Self {
            level_id,
            ai: Arc::new(ai),
            links: None,
            cross,
            vertex_lights: Vec::new(),
            order,
        })
    }

    /// Reads level `level_id`'s `level.ai` and `level.gct` (each as a
    /// stream, what is kept of it held in the file cache under its path)
    /// from its directory `dir` in `tree`, with
    /// [`Navigation::new`]'s checks. A missing cross table is the game's
    /// "Can't open cross table!".
    pub fn read(tree: &dyn Vfs, dir: &str, graph: &GameGraph, level_id: u8) -> io::Result<Self> {
        let file =
            |name: &str, e: io::Error| io::Error::new(e.kind(), format!("{dir}\\{name}: {e}"));
        let path = format!("{dir}\\level.ai");
        let f = tree.open(&path).map_err(|e| file("level.ai", e))?;
        let (ai, vertex_lights, links) = LevelAi::read_in_with_lights_and_links(
            BufReader::new(f),
            |len| file_cache::hold(&path, len),
            |len| file_cache::hold(&path, len),
        )
        .map_err(|e| file("level.ai", e))?;
        let path = format!("{dir}\\level.gct");
        let cross = match tree.open(&path) {
            Ok(f) => CrossTable::read_in(BufReader::new(f), |len| file_cache::hold(&path, len))
                .map_err(|e| file("level.gct", e))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(invalid("Can't open cross table!"));
            }
            Err(e) => return Err(file("level.gct", e)),
        };
        let mut navigation = Self::new(graph, level_id, ai, cross)?;
        navigation.vertex_lights = vertex_lights;
        navigation.links = Some(Arc::new(links));
        Ok(navigation)
    }

    /// Whether `k` is a valid level vertex (below *N*).
    pub fn valid(&self, k: u32) -> bool {
        (k as usize) < self.ai.len()
    }

    /// The level vertices stored at packed column and row `p`.
    fn stacked(&self, p: u32) -> impl Iterator<Item = u32> + '_ {
        let n = self.ai.len();
        let key = |i: usize| match &self.order {
            Some(o) => self.ai.packed(o[i]),
            None => self.ai.packed(i as u32),
        };
        // The first index whose key is not below p.
        let (mut lo, mut hi) = (0, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if key(mid) < Some(p) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        (lo..n)
            .take_while(move |&i| key(i) == Some(p))
            .map(move |i| match &self.order {
                Some(o) => o[i],
                None => i as u32,
            })
    }

    /// A level vertex whose cell contains `position` (5.2), `None` when the
    /// position is off the grid or no level vertex lies there. Of several
    /// stacked at different heights, the one whose height is nearest the
    /// position's (our choice: the spec leaves it unestablished; of equals
    /// the first stored). Without one at the position the game falls back
    /// to a level vertex near the old one (unestablished): we give none.
    pub fn level_vertex_at(&self, position: [f32; 3]) -> Option<u32> {
        let p = self.ai.packed_at(position)?;
        let mut best: Option<(f32, u32)> = None;
        for k in self.stacked(p) {
            let y = self.ai.position(k)?[1];
            let d = (y - position[1]).abs();
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, k));
            }
        }
        best.map(|(_, k)| k)
    }

    /// The level vertex for point `q` from start vertex `k0`
    /// (object-visual-and-transform.md 4.4); `None` only on a level
    /// without vertices.
    pub fn vertex_for_point(&self, k0: u32, q: [f32; 3]) -> Option<u32> {
        let ai = &self.ai;
        let [x, y, z] = q;
        let height = |k: u32| ai.plane_height(k, x, z).unwrap_or(f32::NAN);
        if let Some(p) = ai.packed_at(q) {
            // Step 2.
            if self.valid(k0) && ai.cell_contains(k0, q) {
                return Some(k0);
            }
            // Step 3: the cell vertex.
            let mut below: Option<(f32, u32)> = None;
            let mut lowest: Option<(f32, u32)> = None;
            for k in self.stacked(p) {
                let h = height(k);
                if h <= y && below.is_none_or(|(b, _)| h > b) {
                    below = Some((h, k));
                }
                if lowest.is_none_or(|(l, _)| h < l) {
                    lowest = Some((h, k));
                }
            }
            if let Some((h1, k1)) = below.or(lowest) {
                if !self.valid(k0) {
                    return Some(k1);
                }
                let h0 = height(k0);
                if (y <= h0 && y > h1) || (y > h0 && y > h1 && (y - h1) - (y - h0) <= 1.0) {
                    return Some(k1);
                }
            }
        }
        // Step 4: the fallback.
        let mut best: Option<(f32, u32)> = None;
        let mut consider = |k: u32| {
            let d = self.outline_distance(k, q);
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, k));
            }
        };
        if self.valid(k0) {
            consider(k0);
            let links = self.links.as_ref().and_then(|l| l.links(k0));
            for k in links.into_iter().flatten().filter(|&k| self.valid(k)) {
                consider(k);
            }
        } else {
            (0..self.ai.len() as u32).for_each(consider);
        }
        best.map(|(_, k)| k)
    }

    /// The distance from `q` to level vertex `k`'s cell outline: its four
    /// corners placed on its plane, joined by four edges
    /// (object-visual-and-transform.md 4.4 step 4, believed).
    fn outline_distance(&self, k: u32, q: [f32; 3]) -> f32 {
        let (Some(v), half) = (self.ai.position(k), self.ai.header.cell_size / 2.0) else {
            return f32::INFINITY;
        };
        let corner = |dx: f32, dz: f32| {
            let (x, z) = (v[0] + dx, v[2] + dz);
            [x, self.ai.plane_height(k, x, z).unwrap_or(v[1]), z]
        };
        let corners = [
            corner(-half, -half),
            corner(half, -half),
            corner(half, half),
            corner(-half, half),
        ];
        (0..4)
            .map(|i| segment_distance(q, corners[i], corners[(i + 1) % 4]))
            .fold(f32::INFINITY, f32::min)
    }
}

/// The distance from `q` to the segment `a`–`b`.
fn segment_distance(q: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let sub = |u: [f32; 3], v: [f32; 3]| [u[0] - v[0], u[1] - v[1], u[2] - v[2]];
    let dot = |u: [f32; 3], v: [f32; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let (ab, aq) = (sub(b, a), sub(q, a));
    let len = dot(ab, ab);
    let t = if len > 0.0 {
        (dot(aq, ab) / len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let nearest = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
    let d = sub(q, nearest);
    dot(d, d).sqrt()
}
