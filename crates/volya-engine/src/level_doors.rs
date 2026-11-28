// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's doors under the door scheme `ph_door`: the physics objects
//! whose logic starts in a section of the scheme
//! (docs/behavior/script-api-configuration-and-files.md 9), whose scheme
//! the shipped scripts run (`bind_physic_object.init`,
//! docs/behavior/script-api-object-binders.md 8). Here: their leaves' boxes
//! from their shells (docs/behavior/physics-shells.md 10) for the
//! crosshair's pick and the route planner (the player's body meets the
//! shells in the physics world, physics-contacts.md 7), and the player's
//! hint from the one the crosshair picks (use-action.md 2.2), with when it
//! came under it (`hud-scope` 13). The hint text is the picked object's
//! usable part's (use-action.md 2.2, 10.1), which the scripts set (script
//! API P 8, `set_tip_text`).
//!
//! Which objects the pick tests is unestablished (use-action.md 2.1,
//! TODO(#181)): the doors' leaves only.

use std::collections::BTreeMap;
use std::io;

use serde::Serialize;

use crate::core::collision::body::BodyBox;
use crate::core::collision::geometry::Vec3;
use crate::core::config;
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::level_physics::LevelPhysics;
use crate::objects::LevelObject;
use volya_engine_formats::cp1251;
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

impl Door {
    /// The distance along the unit `dir` from `origin` at which the ray
    /// meets the leaf's boxes, if it does.
    fn pick(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        (self.boxes.iter())
            .filter_map(|b| b.ray(origin, dir))
            .min_by(f32::total_cmp)
    }
}

/// The level's doors and the player's hint.
#[derive(Clone, Debug, Default)]
pub struct LevelDoors {
    doors: Vec<Door>,
    /// `[inventory] take_dist` (use-action.md 2.2).
    take_dist: f32,
    /// The door the crosshair picks within `take_dist` and the engine
    /// clock when it came under it.
    picked: Option<(usize, u64)>,
    /// The usable parts' hint texts the scripts set (script API P 8), by
    /// object id.
    tip_texts: BTreeMap<u16, String>,
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
        let take_dist = value::float(data.system.read("inventory", "take_dist")?);
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
        Ok(Self {
            doors,
            take_dist,
            ..Self::default()
        })
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

    /// The player's update's pick (use-action.md 2.1, 2.2) from `eye`
    /// along the unit `dir`, at engine clock `now`: the nearest door the
    /// ray meets closer than `take_dist`. Whether the level hides an object
    /// behind it is not tested (TODO(#181)).
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
    /// crosshair: the picked object's usable part's hint text
    /// (use-action.md 2.2), as the scripts set it (script API P 8); none
    /// while it is empty (our reading).
    pub fn hint(&self) -> Option<(&str, u64)> {
        let (i, at) = self.picked?;
        let tip = self.tip_texts.get(&self.doors[i].object)?;
        (!tip.is_empty()).then_some((tip.as_str(), at))
    }

    /// The usable parts' hint texts the scripts hold (script API P 8, in
    /// the scripts' Windows-1251), when they changed.
    pub fn set_tip_texts(&mut self, texts: &BTreeMap<u16, Vec<u8>>) {
        let same = texts.len() == self.tip_texts.len()
            && (texts.iter().zip(&self.tip_texts))
                .all(|((a, t), (b, u))| a == b && cp1251::decode(t) == *u);
        if !same {
            self.tip_texts = (texts.iter())
                .map(|(&id, t)| (id, cp1251::decode(t)))
                .collect();
        }
    }

    /// Whether object `id` is one of the doors.
    pub fn holds(&self, id: u16) -> bool {
        self.doors.iter().any(|d| d.object == id)
    }

    /// The object id of the door the crosshair picks.
    pub fn picked_object(&self) -> Option<u16> {
        self.picked.map(|(i, _)| self.doors[i].object)
    }

    pub fn report(&self, physics: &LevelPhysics) -> Vec<DoorReport> {
        (self.doors.iter())
            .map(|d| DoorReport {
                name: d.name.clone(),
                motor: physics.hinge_motor(d.object, JOINT_BONE),
                angle: physics.hinge_angle(d.object, JOINT_BONE),
                tip: self.tip_texts.get(&d.object).cloned(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOOR: u16 = 7;

    /// A level with the door `DOOR`, picked at clock 5.
    fn picked_door() -> LevelDoors {
        LevelDoors {
            doors: vec![Door {
                name: "door".into(),
                object: DOOR,
                boxes: Vec::new(),
            }],
            take_dist: 2.0,
            picked: Some((0, 5)),
            ..LevelDoors::default()
        }
    }

    #[test]
    fn the_scripts_tip_text_is_the_hint() {
        let mut doors = picked_door();
        // Without a script text: none.
        assert_eq!(doors.hint(), None);
        // Script API P 8: set_tip_text stores the usable part's hint text.
        let mut texts = BTreeMap::new();
        texts.insert(DOOR, b"tip_door_locked".to_vec());
        texts.insert(DOOR + 1, b"character_use".to_vec());
        doors.set_tip_texts(&texts);
        assert_eq!(doors.hint(), Some(("tip_door_locked", 5)));
        texts.insert(DOOR, b"tip_door_open".to_vec());
        doors.set_tip_texts(&texts);
        assert_eq!(doors.hint(), Some(("tip_door_open", 5)));
        // An empty text (the scheme's deactivation): none.
        texts.insert(DOOR, Vec::new());
        doors.set_tip_texts(&texts);
        assert_eq!(doors.hint(), None);
        // set_tip_text_default empties it.
        texts.remove(&DOOR);
        doors.set_tip_texts(&texts);
        assert_eq!(doors.hint(), None);
        // Nothing picked: no hint.
        doors.set_tip_texts(&BTreeMap::from([(DOOR, b"x".to_vec())]));
        doors.picked = None;
        assert_eq!(doors.hint(), None);
    }

    #[test]
    fn a_logic_file_of_the_door_scheme_starts_in_one_of_its_sections() {
        assert!(starts_door_scheme(Some("ph_door@locked")));
        assert!(starts_door_scheme(Some(" ph_door ")));
        assert!(!starts_door_scheme(Some("ph_idle@x")));
        assert!(!starts_door_scheme(Some("ph_doors")));
        assert!(!starts_door_scheme(None));
    }
}
