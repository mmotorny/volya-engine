// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's doors under the door scheme (docs/behavior/trader-door.md,
//! cited by section; [`crate::core::door`]): found among the objects online
//! at the start, their scheme updated with the level (2.1, 2.2), its motor
//! handed to the leaf's hinge in the level's physics (2.3), the leaves'
//! boxes from their shells for the pick and the route planner (the player's
//! body meets the shells in the physics world, physics-contacts.md 7), and the
//! player's hint from the one the crosshair picks (5.1), with when it came
//! under it (5.3). The hint text is the picked object's usable part's
//! (use-action.md 2.2, 10.1), which the scripts set (script API P 8,
//! `set_tip_text`); the door scheme's own hint stands in while no script
//! text is set (TODO(#875): the scheme run from the scripts sets it).

use std::collections::BTreeMap;
use std::io;

use serde::Serialize;

use crate::core::collision::body::BodyBox;
use crate::core::collision::geometry::Vec3;
use crate::core::config;
use crate::core::door::{self, Door, DoorSection, DoorSound};
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

/// The level's doors and the player's hint.
#[derive(Clone, Debug, Default)]
pub struct LevelDoors {
    doors: Vec<Door>,
    /// Each door's object id (its shell's in the physics).
    objects: Vec<u16>,
    /// `[inventory] take_dist` (5.1).
    take_dist: f32,
    /// The door the crosshair picks within `take_dist` and the engine
    /// clock when it came under it (5.3).
    picked: Option<(usize, u64)>,
    /// The sounds played since last taken (2.5).
    sounds: Vec<DoorSound>,
    /// The usable parts' hint texts the scripts set (script API P 8), by
    /// object id.
    tip_texts: BTreeMap<u16, String>,
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
    /// shell from the bone data whose logic starts in a door scheme section.
    pub fn load(tree: &dyn Vfs, data: &GameData, objects: &[LevelObject]) -> io::Result<Self> {
        let take_dist = value::float(data.system.read("inventory", "take_dist")?);
        let mut doors = Vec::new();
        let mut ids = Vec::new();
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
            let h = &o.record.header;
            doors.push(
                Door::new(h.name.to_string(), DoorSection::read(section), Vec::new())
                    .at(h.position),
            );
            ids.push(o.id);
        }
        Ok(Self {
            doors,
            objects: ids,
            take_dist,
            picked: None,
            sounds: Vec::new(),
            tip_texts: BTreeMap::new(),
        })
    }

    /// The sounds the doors' sections can play (2.5), each once: created
    /// with the level's sounds (our choice: when the scripts create a sound
    /// set's sounds is theirs, TODO(#875)).
    pub fn sound_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for s in self.doors.iter().flat_map(Door::sounds) {
            if !names.iter().any(|n| n == s) {
                names.push(s.to_owned());
            }
        }
        names
    }

    /// The sounds the doors played since last taken (2.5).
    pub fn take_sounds(&mut self) -> Vec<DoorSound> {
        std::mem::take(&mut self.sounds)
    }

    /// The leaves' boxes, where their shells hold them (3.1).
    pub fn boxes(&self) -> Vec<BodyBox> {
        (self.doors.iter())
            .flat_map(|d| d.boxes().iter().copied())
            .collect()
    }

    /// The motors the schemes set since last handed on, to their hinges
    /// (2.3: each wakes the shell), and the leaves' boxes from their shells.
    pub fn sync(&mut self, physics: &mut LevelPhysics) -> io::Result<()> {
        for (d, &id) in self.doors.iter_mut().zip(&self.objects) {
            if let Some(m) = d.take_motor() {
                physics.set_max_force_and_velocity(id, door::JOINT_BONE, m.force, m.velocity)?;
            }
            d.set_boxes(physics.boxes(id));
        }
        Ok(())
    }

    /// The objects' update (2.1).
    pub fn update(&mut self) {
        self.doors.iter_mut().for_each(Door::update);
    }

    /// The per-step calls of `steps` physics steps (2.2), each door closed
    /// when its hinge's angle is at most −0 (2.3; a door without a hinge
    /// stands closed).
    pub fn physics_steps(&mut self, steps: u32, physics: &LevelPhysics) {
        for (d, &id) in self.doors.iter_mut().zip(&self.objects) {
            let closed = physics
                .hinge_angle(id, door::JOINT_BONE)
                .is_none_or(|a| a <= -0.0);
            for _ in 0..steps {
                d.physics_step(closed);
            }
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
    /// crosshair (5.1, 5.3): the picked object's usable part's hint text
    /// (use-action.md 2.2), the scripts' when set (script API P 8), else
    /// the door scheme's stand-in (TODO(#875)).
    pub fn hint(&self) -> Option<(&str, u64)> {
        let (i, at) = self.picked?;
        let tip = match self.tip_texts.get(&self.objects[i]) {
            Some(text) => text.as_str(),
            None => self.doors[i].tip()?,
        };
        Some((tip, at))
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

    /// The use action on the door the crosshair picks, if any (2.5): the
    /// sound it plays is kept for [`Self::take_sounds`].
    pub fn use_picked(&mut self) {
        if let Some((i, _)) = self.picked {
            self.sounds.extend(self.doors[i].use_action());
        }
    }

    /// Whether object `id` is one of the doors.
    pub fn holds(&self, id: u16) -> bool {
        self.objects.contains(&id)
    }

    /// The object id of the door the crosshair picks.
    pub fn picked_object(&self) -> Option<u16> {
        self.picked.map(|(i, _)| self.objects[i])
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

#[cfg(test)]
mod tests {
    use super::*;

    const LOGIC: &[u8] = b"[ph_door@locked]\nlocked = true\n";
    const DOOR: u16 = 7;

    /// A level with the locked door `DOOR`, started and picked at clock 5.
    fn picked_door() -> LevelDoors {
        let config = ltx::parse(LOGIC).unwrap();
        let section = DoorSection::read(config.section("ph_door@locked").unwrap());
        let mut door = Door::new("door".into(), section, Vec::new());
        door.update();
        LevelDoors {
            doors: vec![door],
            objects: vec![DOOR],
            take_dist: 2.0,
            picked: Some((0, 5)),
            ..LevelDoors::default()
        }
    }

    #[test]
    fn the_scripts_tip_text_is_the_hint() {
        let mut doors = picked_door();
        // Without a script text: the door scheme's stand-in.
        assert_eq!(doors.hint(), Some(("tip_door_locked", 5)));
        // Script API P 8: set_tip_text stores the usable part's hint text.
        let mut texts = BTreeMap::new();
        texts.insert(DOOR, b"tip_door_open".to_vec());
        texts.insert(DOOR + 1, b"character_use".to_vec());
        doors.set_tip_texts(&texts);
        assert_eq!(doors.hint(), Some(("tip_door_open", 5)));
        // set_tip_text_default empties it.
        texts.remove(&DOOR);
        doors.set_tip_texts(&texts);
        assert_eq!(doors.hint(), Some(("tip_door_locked", 5)));
        // Nothing picked: no hint.
        doors.set_tip_texts(&BTreeMap::from([(DOOR, b"x".to_vec())]));
        doors.picked = None;
        assert_eq!(doors.hint(), None);
    }
}
