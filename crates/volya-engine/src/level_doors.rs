// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's doors under the door scheme (docs/behavior/trader-door.md,
//! cited by section; [`crate::core::door`]): found among the objects online
//! at the start, their leaves' boxes handed to the player's body (3), their
//! scheme updated with the level (2.1, 2.2), and the player's hint from the
//! one the crosshair picks (5.1), with when it came under the crosshair
//! (5.3).

use std::io;

use serde::Serialize;

use crate::core::collision::body::BodyBox;
use crate::core::collision::geometry::Vec3;
use crate::core::config;
use crate::core::door::{self, Door, DoorSection};
use crate::core::vfs::{self, Vfs};
use crate::data::GameData;
use crate::objects::{LevelObject, locate_model};
use volya_engine_formats::gamemtl::GameMaterial;
use volya_engine_formats::ltx::{self, value};
use volya_engine_formats::model::{Model, transform};
use volya_engine_formats::object::Object;

/// The physics type whose shell comes from the model's bone data
/// (`physics-scope` 1.2).
const BONE_DATA_SHELL: u32 = 3;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The level's doors and the player's hint.
#[derive(Clone, Debug, Default)]
pub struct LevelDoors {
    doors: Vec<Door>,
    /// `[inventory] take_dist` (5.1).
    take_dist: f32,
    /// The door the crosshair picks within `take_dist` and the engine
    /// clock when it came under it (5.3).
    picked: Option<(usize, u64)>,
}

/// The doors as the engine interface reports them.
#[derive(Clone, Debug, Serialize)]
pub struct DoorReport {
    pub name: String,
    pub section: String,
    pub locked: bool,
    /// The hinge motor's maximum force and velocity (2.3), once set.
    pub motor: Option<[f32; 2]>,
    pub tip: Option<String>,
}

impl LevelDoors {
    /// The doors among `objects` (1, 2.1): online physics objects with a
    /// shell from the bone data whose logic starts in a door scheme section,
    /// `materials` the game materials (the collision world's).
    pub fn load(
        tree: &dyn Vfs,
        data: &GameData,
        dir: &str,
        objects: &[LevelObject],
        materials: &[GameMaterial],
    ) -> io::Result<Self> {
        let take_dist = value::float(data.system.read("inventory", "take_dist")?);
        let mut doors = Vec::new();
        for o in objects.iter().filter(|o| o.online) {
            let Object::PhysicsObject(p) = &o.record.object else {
                continue;
            };
            if p.physics_type != BONE_DATA_SHELL {
                continue;
            }
            // How the scripts read malformed custom data is theirs
            // (TODO(#875)); such a record has no door here.
            let Ok(custom) = ltx::parse(p.object.custom_data.as_bytes()) else {
                continue;
            };
            let Some(cfg) = custom.get("logic", "cfg").flatten() else {
                continue;
            };
            let Some(start) = door::start_section(cfg) else {
                continue;
            };
            let path = data
                .aliases
                .path("$game_config$", value::unquote(cfg.trim()))?;
            let logic = config::load(tree, &path)?;
            let Some(section) = logic
                .section(start)
                .filter(|s| door::is_door_section(s.name()))
            else {
                continue;
            };
            let model_path = locate_model(tree, data, dir, &p.visual.model)?;
            let model = Model::parse(&vfs::read(tree, &model_path)?)
                .map_err(|e| invalid(format!("{model_path}: {e}")))?;
            let h = &o.record.header;
            let boxes = door::shell_boxes(
                &model,
                &transform(h.position, h.angles),
                &p.fixed_bones,
                materials,
            )
            .map_err(|e| invalid(format!("{}: {e}", h.name.as_str())))?;
            doors.push(Door::new(
                h.name.to_string(),
                DoorSection::read(section),
                boxes,
            ));
        }
        Ok(Self {
            doors,
            take_dist,
            picked: None,
        })
    }

    /// The boxes of the closed doors' leaves, held at their placed pose
    /// (2.4; [`crate::core::door`]: an open door's leaf would move, which
    /// is not simulated, so it meets nothing here).
    pub fn boxes(&self) -> Vec<BodyBox> {
        (self.doors.iter())
            .filter(|d| d.section.closed)
            .flat_map(|d| d.boxes().iter().copied())
            .collect()
    }

    /// The objects' update (2.1).
    pub fn update(&mut self) {
        self.doors.iter_mut().for_each(Door::update);
    }

    /// The per-step calls of `steps` physics steps (2.2).
    pub fn physics_steps(&mut self, steps: u32) {
        for _ in 0..steps {
            self.doors.iter_mut().for_each(Door::physics_step);
        }
    }

    /// The player's update's pick (5.1) from `eye` along the unit `dir`,
    /// at engine clock `now`: the nearest door the ray meets closer than
    /// `take_dist`. Which shapes the pick tests, and whether the level
    /// hides an object behind it, are unestablished (5.1, TODO(#181)): the
    /// doors' boxes, nothing hidden.
    pub fn pick(&mut self, eye: Vec3, dir: Vec3, now: u64) {
        let picked = (self.doors.iter().enumerate())
            .filter_map(|(i, d)| Some((d.pick(eye, dir)?, i)))
            .filter(|&(t, _)| t < self.take_dist)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, i)| i);
        self.picked = match (self.picked, picked) {
            (Some((was, at)), Some(i)) if was == i => Some((i, at)),
            (_, Some(i)) => Some((i, now)),
            (_, None) => None,
        };
    }

    /// The player's hint id and the engine clock its object came under the
    /// crosshair (5.1, 5.3): a picked door's hint.
    pub fn hint(&self) -> Option<(&str, u64)> {
        let (i, at) = self.picked?;
        Some((self.doors[i].tip()?, at))
    }

    /// The use action on the door the crosshair picks, if any (2.5).
    pub fn use_picked(&mut self) {
        if let Some((i, _)) = self.picked {
            self.doors[i].use_action();
        }
    }

    pub fn report(&self) -> Vec<DoorReport> {
        (self.doors.iter())
            .map(|d| DoorReport {
                name: d.name.clone(),
                section: d.section.name.clone(),
                locked: d.section.locked,
                motor: d.motor().map(|m| [m.force, m.velocity]),
                tip: d.tip().map(str::to_owned),
            })
            .collect()
    }
}
