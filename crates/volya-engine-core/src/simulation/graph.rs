// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game graph as the simulation uses it
//! (docs/behavior/offline-simulation.md 1.6, 7.4): accessibility flags,
//! terrain masks and the path search.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::config::{Config, value};
use volya_engine_formats::game_graph::{GameGraph, TerrainMask, matches_mask};

/// The mask that matches every vertex.
pub const ANY_TERRAIN: TerrainMask = [255; 4];

/// The game graph with its per-vertex accessibility (7.4).
#[derive(Clone, Debug)]
pub struct Graph {
    pub graph: GameGraph,
    /// Every vertex is accessible when the graph is loaded (7.4).
    accessible: Vec<bool>,
}

/// An open vertex in the search: smallest `f` first, and among equal `f`
/// the most recently opened or improved (the largest `order`).
#[derive(Clone, Copy, PartialEq)]
struct Open {
    f: f32,
    order: u32,
    vertex: u16,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .total_cmp(&self.f)
            .then(self.order.cmp(&other.order))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A vertex's state during one search.
#[derive(Clone, Copy)]
struct Seen {
    g: f32,
    predecessor: u16,
    /// The order of its latest opening or improvement; stale heap entries
    /// carry another.
    order: u32,
    closed: bool,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl Graph {
    pub fn new(graph: GameGraph) -> Self {
        let accessible = vec![true; graph.vertices.len()];
        Self { graph, accessible }
    }

    /// Whether `vertex` is accessible.
    pub fn accessible(&self, vertex: u16) -> bool {
        self.accessible
            .get(usize::from(vertex))
            .copied()
            .unwrap_or(false)
    }

    /// Sets `vertex` accessible or not (scripts'
    /// `game_graph():accessible(vertex, flag)`, 7.4).
    pub fn set_accessible(&mut self, vertex: u16, accessible: bool) {
        if let Some(a) = self.accessible.get_mut(usize::from(vertex)) {
            *a = accessible;
        }
    }

    /// Whether `vertex` matches one of `masks`.
    fn matches(&self, vertex: u16, masks: &[TerrainMask]) -> bool {
        self.graph
            .vertex(vertex)
            .is_some_and(|v| masks.iter().any(|&m| matches_mask(v.types, m)))
    }

    /// The path search of 7.4 from `start` to `target` for an object with
    /// terrain masks `masks`: A* over the directed edges, cost the edge
    /// distances, estimate the straight line between global points, all in
    /// single precision. The path runs from `start` to `target`, both
    /// included; `None` when none is found.
    ///
    /// The masks apply when the start vertex is accessible; when it also
    /// matches none of them, whether they apply is unestablished, and we
    /// apply them (our choice; the start vertex itself is never tested).
    pub fn path(&self, start: u16, target: u16, masks: &[TerrainMask]) -> Option<Vec<u16>> {
        let vertices = &self.graph.vertices;
        let target_point = vertices.get(usize::from(target))?.global_point;
        vertices.get(usize::from(start))?;
        let masks_apply = self.accessible(start);
        let admitted = |v: u16| self.accessible(v) && (!masks_apply || self.matches(v, masks));
        let estimate = |v: u16| distance(vertices[usize::from(v)].global_point, target_point);
        let mut seen: Vec<Option<Seen>> = vec![None; vertices.len()];
        let mut open = BinaryHeap::new();
        let mut order = 0u32;
        seen[usize::from(start)] = Some(Seen {
            g: 0.0,
            predecessor: start,
            order,
            closed: false,
        });
        open.push(Open {
            f: estimate(start),
            order,
            vertex: start,
        });
        while let Some(Open {
            order: o, vertex, ..
        }) = open.pop()
        {
            let s = seen[usize::from(vertex)].as_mut()?;
            if s.closed || s.order != o {
                continue;
            }
            if vertex == target {
                let mut path = vec![target];
                let mut v = target;
                while v != start {
                    v = seen[usize::from(v)]?.predecessor;
                    path.push(v);
                }
                path.reverse();
                return Some(path);
            }
            s.closed = true;
            let g = s.g;
            for e in self.graph.edges_of(vertex) {
                let t = e.target;
                if !admitted(t) {
                    continue;
                }
                let new_g = g + e.distance;
                match &mut seen[usize::from(t)] {
                    Some(n) if n.closed => continue,
                    Some(n) if new_g >= n.g => continue,
                    slot => {
                        order += 1;
                        *slot = Some(Seen {
                            g: new_g,
                            predecessor: vertex,
                            order,
                            closed: false,
                        });
                        open.push(Open {
                            f: new_g + estimate(t),
                            order,
                            vertex: t,
                        });
                    }
                }
            }
        }
        None
    }

    /// The length of `path`: the sum of the first stored edge between each
    /// pair of consecutive vertices; `None` if a pair has no edge.
    pub fn path_length(&self, path: &[u16]) -> Option<f32> {
        path.windows(2)
            .map(|w| {
                self.graph
                    .edges_of(w[0])
                    .iter()
                    .find(|e| e.target == w[1])
                    .map(|e| e.distance)
            })
            .sum()
    }
}

/// A key that is exactly four comma-separated numbers, as a mask: each
/// number (decimal digits, our reading) taken modulo 256 (also
/// character-identity.md 3 step 4).
pub fn mask_key(key: &str) -> Option<TerrainMask> {
    let parts: Vec<&str> = key.split(',').map(str::trim).collect();
    if parts.len() != 4
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let mut mask = [0u8; 4];
    for (m, p) in mask.iter_mut().zip(parts) {
        *m = value::int(p).rem_euclid(256) as u8;
    }
    Some(mask)
}

/// The terrain masks of a `terrain` value (7.4): the masks of the section
/// it names in `sections` if that has at least one line, else its numbers
/// four at a time (a final group of fewer ignored); the single mask
/// 255,255,255,255 when either gives none.
pub fn terrain_masks(terrain: &str, sections: &Config) -> Vec<TerrainMask> {
    let masks: Vec<TerrainMask> = match sections.section(terrain.trim()) {
        Some(section) if !section.is_empty() => section
            .entries()
            .filter_map(|e| mask_key(e.key()))
            .collect(),
        _ => {
            let numbers: Vec<u8> = value::list(terrain)
                .map(|n| value::int(n).rem_euclid(256) as u8)
                .collect();
            numbers.as_chunks::<4>().0.to_vec()
        }
    };
    if masks.is_empty() {
        vec![ANY_TERRAIN]
    } else {
        masks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::ltx;

    #[test]
    fn masks_from_sections_and_lists() {
        let c = ltx::parse(
            b"[flesh_terrain]\n255,000,255,255\n[snork_terrain]\n255,000,255,255\n010,001,255,255\nnote = 1\n[empty_lines]\nx\n",
        )
        .unwrap();
        assert_eq!(terrain_masks("flesh_terrain", &c), [[255, 0, 255, 255]]);
        let mut snork = terrain_masks("snork_terrain", &c);
        snork.sort();
        assert_eq!(snork, [[10, 1, 255, 255], [255, 0, 255, 255]]);
        assert_eq!(terrain_masks("empty_lines", &c), [ANY_TERRAIN]);
        assert_eq!(terrain_masks("255,255,255,255,0,1", &c), [ANY_TERRAIN]);
        assert_eq!(terrain_masks("1,2,3,260", &c), [[1, 2, 3, 4]]);
        assert_eq!(terrain_masks("", &c), [ANY_TERRAIN]);
    }
}
