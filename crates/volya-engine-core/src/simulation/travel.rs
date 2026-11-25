// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Offline travel (docs/behavior/offline-simulation.md 7.4, 7.5): a
//! stalker's or monster's travel state and its movement update.

use super::Simulation;
use volya_engine_formats::game_graph::TerrainMask;
use volya_engine_formats::object::ObjectRecord;

/// A travel target: a game vertex, a level vertex and a position (7.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub game_vertex: u16,
    pub level_vertex: u32,
    pub position: [f32; 3],
}

/// What a stalker's or monster's travel reads from its configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct TravelConfig {
    /// Metres per real second at the default speed of game time, off the
    /// current level.
    pub going_speed: f32,
    /// … on the current level.
    pub current_level_going_speed: f32,
    /// `smart_terrain_choose_interval`, game ms.
    pub choose_interval: u64,
    pub masks: Vec<TerrainMask>,
}

/// The travel state (7.5) and the smart-terrain look (7.2); not saved.
#[derive(Clone, Debug, PartialEq)]
pub struct Travel {
    pub config: TravelConfig,
    /// From the vertex the object is at to the target's game vertex; empty
    /// without a path.
    pub path: Vec<u16>,
    /// Metres along the path's current edge.
    pub progress: f32,
    pub target: Target,
    /// Game time of the previous movement update; 0 before the first.
    pub clock: u64,
    /// The per-object setting that allows choosing a smart terrain (on at
    /// creation; scripts can turn it off).
    pub can_choose: bool,
    /// Game time of the last smart-terrain look; 0 at creation.
    pub last_look: u64,
}

impl Travel {
    /// The state at creation (New Game or load): no path, progress and
    /// clock 0, the target the object's own location.
    pub fn new(record: &ObjectRecord, config: TravelConfig) -> Self {
        let block = record.object.object_block();
        Self {
            config,
            path: Vec::new(),
            progress: 0.0,
            target: Target {
                game_vertex: block.game_graph_vertex,
                level_vertex: block.level_graph_node,
                position: record.header.position,
            },
            clock: 0,
            can_choose: true,
            last_look: 0,
        }
    }

    /// Discards the path (6.3).
    pub fn discard_path(&mut self) {
        self.path.clear();
        self.progress = 0.0;
    }
}

impl Simulation {
    /// A movement update of `id` at game time `now` (7.5).
    pub(super) fn movement_update(&mut self, id: u16, now: u64, normal_time_factor: f32) {
        let Some(t) = self.objects.get_mut(&id).and_then(|o| o.travel.as_mut()) else {
            return;
        };
        // Step 1.
        if t.clock >= now {
            return;
        }
        let delta = now - t.clock;
        let started = t.clock != 0;
        t.clock = now;
        if !started {
            return;
        }
        // Step 2: the target test and the path search (7.4).
        let o = &self.objects[&id];
        let t = o.travel.as_ref().expect("travel");
        let block = o.record.object.object_block();
        let (gv, lv) = (block.game_graph_vertex, block.level_graph_node);
        let target = t.target;
        if target.game_vertex == gv && target.level_vertex == lv {
            return;
        }
        // A path that does not start at the object's vertex (it was moved
        // by the location update, 5.2) is searched again (our choice).
        let leads = t.path.first() == Some(&gv) && t.path.last() == Some(&target.game_vertex);
        if !leads {
            let path = self.graph.path(gv, target.game_vertex, &t.config.masks);
            let t = self
                .objects
                .get_mut(&id)
                .and_then(|o| o.travel.as_mut())
                .expect("travel");
            match path {
                None => {
                    t.path.clear();
                    return;
                }
                Some(p) => {
                    if p.len() > 1 {
                        t.progress = 0.0;
                    }
                    t.path = p;
                }
            }
        }
        // Step 3.
        let o = self.objects.get_mut(&id).expect("object");
        let t = o.travel.as_mut().expect("travel");
        if t.path.len() == 1 {
            t.progress = 0.0;
            o.record.header.position = target.position;
            o.record.object.object_block_mut().level_graph_node = target.level_vertex;
            return;
        }
        // Step 4: t / normal_time_factor · speed metres, t in game seconds.
        let mut seconds = delta as f32 * 0.001;
        loop {
            let o = &self.objects[&id];
            let t = o.travel.as_ref().expect("travel");
            let gv = o.record.object.object_block().game_graph_vertex;
            let speed = if self.on_current_level(gv) {
                t.config.current_level_going_speed
            } else {
                t.config.going_speed
            };
            if speed <= 0.0 || normal_time_factor <= 0.0 || t.path.len() < 2 {
                return;
            }
            let covered = seconds / normal_time_factor * speed;
            let next = t.path[1];
            let Some(edge) = self
                .graph
                .graph
                .edges_of(gv)
                .iter()
                .find(|e| e.target == next)
            else {
                // No edge to the path's next vertex: the path is discarded
                // (our choice; the search only follows edges).
                let t = self.travel_mut(id);
                t.discard_path();
                return;
            };
            let edge = edge.distance;
            let reached = t.progress + covered;
            if reached < edge {
                self.travel_mut(id).progress = reached;
                return;
            }
            self.arrive(id, next);
            let t = self.travel_mut(id);
            t.progress = 0.0;
            t.path.remove(0);
            if t.path.len() < 2 {
                return;
            }
            seconds = (reached - edge) * normal_time_factor / speed;
        }
    }

    fn travel_mut(&mut self, id: u16) -> &mut Travel {
        self.objects
            .get_mut(&id)
            .and_then(|o| o.travel.as_deref_mut())
            .expect("travel")
    }
}
