// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The current level's navigation data (docs/behavior/offline-simulation.md
//! sections 2, 3, 6.1): its `level.ai` positions and its cross table, with
//! the checks of 1.8, and the level vertex at a position (5.2). Both are
//! held in the file cache, standing for the files the original maps
//! (docs/architecture/memory.md, File-backed data): the positions, a
//! leaner structure, under `level.ai`'s path; `level.gct` whole, read in
//! place.

use std::io::{self, BufReader};

use crate::file_cache;
use crate::vfs::Vfs;
use volya_engine_formats::cross_table::CrossTable;
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::level_ai::LevelAi;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A level's navigation data.
#[derive(Debug)]
pub struct Navigation {
    pub level_id: u8,
    pub ai: LevelAi,
    pub cross: CrossTable,
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
            ai,
            cross,
            order,
        })
    }

    /// Reads level `level_id`'s `level.ai` (as a stream, its positions
    /// held in the file cache under its path) and `level.gct` (held in the
    /// file cache, read in place) from its directory `dir` in `tree`, with
    /// [`Navigation::new`]'s checks. A missing cross table is the game's
    /// "Can't open cross table!".
    pub fn read(tree: &dyn Vfs, dir: &str, graph: &GameGraph, level_id: u8) -> io::Result<Self> {
        let file =
            |name: &str, e: io::Error| io::Error::new(e.kind(), format!("{dir}\\{name}: {e}"));
        let path = format!("{dir}\\level.ai");
        let f = tree.open(&path).map_err(|e| file("level.ai", e))?;
        let ai = LevelAi::read_in(BufReader::new(f), |len| file_cache::hold(&path, len))
            .map_err(|e| file("level.ai", e))?;
        let cross = match file_cache::read(tree, &format!("{dir}\\level.gct")) {
            Ok(f) => CrossTable::in_place(f).map_err(|e| file("level.gct", e))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(invalid("Can't open cross table!"));
            }
            Err(e) => return Err(file("level.gct", e)),
        };
        Self::new(graph, level_id, ai, cross)
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
}
