// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's doors under the door scheme `ph_door`: the physics objects
//! whose logic starts in a section of the scheme
//! (docs/behavior/script-api-configuration-and-files.md 9), whose scheme
//! the shipped scripts run (`bind_physic_object.init`,
//! docs/behavior/script-api-object-binders.md 8). Here: their leaves' boxes
//! from their shells (docs/behavior/physics-shells.md 10) for the route
//! planner (the player's body meets the shells in the physics world,
//! physics-contacts.md 7). The crosshair picks a door by its shell's boxes
//! like any object with a shell ([`crate::crosshair`], use-action.md 2.1).

use std::collections::BTreeMap;
use std::io;

use serde::Serialize;

use crate::core::collision::body::BodyBox;
use crate::core::config;
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::level_physics::LevelPhysics;
use crate::objects::LevelObject;
use volya_engine_formats::ltx::{self, value};
use volya_engine_formats::object::Object;

/// The physics type whose shell comes from the model's bone data
/// (`physics-scope` 1.2).
const BONE_DATA_SHELL: u32 = 3;

/// The class id of `CPhysicObject` (spec C's factory table).
const PHYSICS_OBJECT_CLASS: &str = "O_PHYS_S";

/// The door scheme's name: a logic section `ph_door` or `ph_door@…`.
const SCHEME: &str = "ph_door";

/// The bone whose hinge the scheme drives (script API Y 11).
pub const JOINT_BONE: &str = "door";

/// A door: its record's name, its object id (its shell's in the physics)
/// and its leaf's boxes where the shell holds them.
#[derive(Clone, Debug, Default)]
struct Door {
    name: String,
    object: u16,
    boxes: Vec<BodyBox>,
}

/// The level's doors.
#[derive(Clone, Debug, Default)]
pub struct LevelDoors {
    doors: Vec<Door>,
}

/// The doors as the engine interface reports them.
#[derive(Clone, Debug, Serialize)]
pub struct DoorReport {
    pub name: String,
    /// The leaf's hinge motor: maximum force and target velocity.
    pub motor: Option<[f32; 2]>,
    /// The leaf's hinge angle (physics-joints.md 2.2).
    pub angle: Option<f32>,
    /// The usable part's hint text the scripts set.
    pub tip: Option<String>,
}

/// Whether a logic file whose `[logic] active` is `active` starts in a
/// section of the door scheme (F 9).
fn starts_door_scheme(active: Option<&str>) -> bool {
    active.is_some_and(|s| value::unquote(s.trim()).split('@').next() == Some(SCHEME))
}

impl LevelDoors {
    /// The doors among `objects`: online physics objects with a shell from
    /// the bone data, built in `physics`, whose logic starts in a door
    /// scheme section, of the physics objects' class. One whose shell isn't
    /// built is left out (the scheme's start stops the game without a
    /// shell, Y 11 step 1).
    pub fn load(
        tree: &dyn Vfs,
        data: &GameData,
        objects: &[LevelObject],
        physics: &LevelPhysics,
    ) -> io::Result<Self> {
        let mut doors = Vec::new();
        for o in objects.iter().filter(|o| o.online) {
            let Object::PhysicsObject(p) = &o.record.object else {
                continue;
            };
            // Only the physics objects' own class, `CPhysicObject`, is
            // physical to the scripts (script API M 2.2), so only its
            // objects have the shell the scheme's start needs (script API Y
            // 11 step 1). The destroyable objects under the scheme are left
            // out (TODO(#1123)).
            let section = o.record.header.section.as_str();
            let class = data.system.get(section, "class").flatten();
            if p.physics_type != BONE_DATA_SHELL
                || class.map(str::trim) != Some(PHYSICS_OBJECT_CLASS)
            {
                continue;
            }
            // How the scripts read malformed custom data is theirs; such a
            // record has no door here.
            let Ok(custom) = ltx::parse(p.object.custom_data.as_bytes()) else {
                continue;
            };
            let Some(cfg) = custom.get("logic", "cfg").flatten() else {
                continue;
            };
            let path = data
                .aliases
                .path("$game_config$", value::unquote(cfg.trim()))?;
            if !tree.exists(&path) {
                continue;
            }
            let logic = config::load(tree, &path)?;
            if !starts_door_scheme(logic.get("logic", "active").flatten()) {
                continue;
            }
            if physics.shell_of(o.id).is_none() {
                continue;
            }
            doors.push(Door {
                name: o.record.header.name.to_string(),
                object: o.id,
                boxes: Vec::new(),
            });
        }
        Ok(Self { doors })
    }

    /// The leaves' boxes, where their shells hold them.
    pub fn boxes(&self) -> Vec<BodyBox> {
        (self.doors.iter())
            .flat_map(|d| d.boxes.iter().copied())
            .collect()
    }

    /// The leaves' boxes from their shells.
    pub fn sync(&mut self, physics: &LevelPhysics) {
        for d in &mut self.doors {
            d.boxes = physics.boxes(d.object);
        }
    }

    /// Whether object `id` is one of the doors.
    pub fn holds(&self, id: u16) -> bool {
        self.doors.iter().any(|d| d.object == id)
    }

    /// The doors' report, with the usable parts' hint texts `tips`.
    pub fn report(&self, physics: &LevelPhysics, tips: &BTreeMap<u16, String>) -> Vec<DoorReport> {
        (self.doors.iter())
            .map(|d| DoorReport {
                name: d.name.clone(),
                motor: physics.hinge_motor(d.object, JOINT_BONE),
                angle: physics.hinge_angle(d.object, JOINT_BONE),
                tip: tips.get(&d.object).cloned(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_logic_file_of_the_door_scheme_starts_in_one_of_its_sections() {
        assert!(starts_door_scheme(Some("ph_door@locked")));
        assert!(starts_door_scheme(Some(" ph_door ")));
        assert!(!starts_door_scheme(Some("ph_idle@x")));
        assert!(!starts_door_scheme(Some("ph_doors")));
        assert!(!starts_door_scheme(None));
    }
}
