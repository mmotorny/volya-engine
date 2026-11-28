// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The door scheme `ph_door` of a physics object
//! (docs/behavior/trader-door.md, cited by section): the scheme's section
//! (2.1), its start (2.2) and per-step call (2.2, 2.3), the hint it sets
//! (5.1), the use action's sound (2.5), the motor it sets on the leaf's
//! hinge (2.2, 2.3), and the leaf's boxes the crosshair picks (1, 5.1).
//!
//! The shipped scripts do not run yet: the scheme here is the spec's
//! statement of what the scripts do, standing in for them (TODO(#875)). The
//! leaf is its shell's (docs/behavior/physics-shells.md 10): the level
//! hands the scheme's motor to the hinge and the shell's boxes, where its
//! bodies hold them, to the door. The player's body meets the shell itself
//! through the physics world's contacts (docs/behavior/physics-contacts.md
//! 7), at rest from the start as when active (7.1).

use volya_engine_formats::ltx::{Section, value};

use crate::collision::body::BodyBox;
use crate::collision::geometry::Vec3;

/// The door scheme's name: a logic section `ph_door` or `ph_door@…`.
pub const SCHEME: &str = "ph_door";
/// The locked hint id when the section has no `tip_open` (2.1).
pub const LOCKED_TIP: &str = "tip_door_locked";
/// The unlocked closed section's hint id (4.3).
pub const OPEN_TIP: &str = "tip_door_open";

/// The sections the trader's door switches to (4.1, 4.3): unlocked and
/// closed, then open once used (TODO(#876): the unlock by
/// `esc_trader_can_leave`, once the trader's dialog gives it).
pub const UNLOCKED_SECTION: &str = "ph_door@closed";
pub const OPEN_SECTION: &str = "ph_door@open";

/// Where the shipped scripts start a logic file (2.1): the logic file a
/// record's `[logic] cfg` names (relative to the configuration's folder)
/// and its first section. Only the spec's own case: the scripts choose it
/// (TODO(#875)).
pub const START_SECTIONS: [(&str, &str); 1] = [("scripts\\esc_trader_door.ltx", "ph_door@locked")];

/// The start section of logic file `cfg`, as [`START_SECTIONS`] has it.
pub fn start_section(cfg: &str) -> Option<&'static str> {
    START_SECTIONS
        .iter()
        .find(|(file, _)| file.eq_ignore_ascii_case(cfg.trim()))
        .map(|&(_, section)| section)
}

/// Whether logic section `name` is of the door scheme.
pub fn is_door_section(name: &str) -> bool {
    name.split('@').next() == Some(SCHEME)
}

/// The bone whose joint the scheme drives (2.2).
pub const JOINT_BONE: &str = "door";

/// The sound sets the door scheme's sections name and their sounds (2.5):
/// only the spec's own. The sets are the shipped scripts' (TODO(#875)).
pub const SOUND_SETS: [(&str, &str); 1] = [("trader_door_unlock", "device\\door_servomotor")];

/// The sound of sound set `name`, as [`SOUND_SETS`] has it.
pub fn sound_set(name: &str) -> Option<&'static str> {
    SOUND_SETS
        .iter()
        .find(|(set, _)| set.eq_ignore_ascii_case(name.trim()))
        .map(|&(_, sound)| sound)
}

/// A sound the scheme plays at a position (2.5).
#[derive(Clone, Debug, PartialEq)]
pub struct DoorSound {
    /// The sound's name (S1's name rules).
    pub sound: &'static str,
    pub position: Vec3,
}

/// The hinge motor's values on axis 0 (2.3): maximum force (a torque) and
/// target velocity, rad/s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motor {
    pub force: f32,
    pub velocity: f32,
}

/// The motor the closing sets (2.2, item 1).
pub const CLOSING: Motor = Motor {
    force: 200.0,
    velocity: 3.0,
};
/// The motor the per-step call sets once the door is closed (2.2).
pub const HOLDING: Motor = Motor {
    force: 10_000.0,
    velocity: 1.0,
};
/// The motor the opening sets, toward the open stop (4.3).
pub const OPENING: Motor = Motor {
    force: 2100.0,
    velocity: -3.0,
};

/// A door scheme section (2.1).
#[derive(Clone, Debug, PartialEq)]
pub struct DoorSection {
    pub name: String,
    pub locked: bool,
    /// Default true.
    pub closed: bool,
    /// Default true.
    pub show_tips: bool,
    /// The locked hint id: `tip_open`, default [`LOCKED_TIP`].
    pub locked_tip: String,
    /// The sound set `snd_open_start` (2.1), if any.
    pub snd_open_start: Option<String>,
}

impl DoorSection {
    pub fn read(section: Section<'_>) -> Self {
        let flag =
            |key: &str, default: bool| section.get(key).flatten().map_or(default, value::bool);
        Self {
            name: section.name().to_owned(),
            locked: flag("locked", false),
            closed: flag("closed", true),
            show_tips: flag("show_tips", true),
            locked_tip: section
                .get("tip_open")
                .flatten()
                .map_or(LOCKED_TIP, value::unquote)
                .to_owned(),
            snd_open_start: (section.get("snd_open_start").flatten())
                .map(|v| value::unquote(v).to_owned()),
        }
    }
}

/// A physics object under the door scheme.
#[derive(Clone, Debug, PartialEq)]
pub struct Door {
    /// The record's name.
    pub name: String,
    pub section: DoorSection,
    started: bool,
    /// Set by the closing until the per-step call acts (2.2).
    to_block: bool,
    motor: Option<Motor>,
    /// Set since last taken (2.3: each setting wakes the shell).
    motor_set: bool,
    tip: Option<String>,
    boxes: Vec<BodyBox>,
    /// The object's position (the record's).
    position: Vec3,
}

impl Door {
    /// The door before its first update: no motor (2.3, believed), no hint.
    pub fn new(name: String, section: DoorSection, boxes: Vec<BodyBox>) -> Self {
        Self {
            name,
            section,
            started: false,
            to_block: false,
            motor: None,
            motor_set: false,
            tip: None,
            boxes,
            position: [0.0; 3],
        }
    }

    /// The door object at `position`.
    pub fn at(mut self, position: Vec3) -> Self {
        self.position = position;
        self
    }

    /// The use action on the door (2.5), its scheme started (the scheme's
    /// use callback exists from its start, 2.1): a locked section plays its
    /// `snd_open_start` sound set at the door's position, returned here; a
    /// set [`SOUND_SETS`] lacks plays nothing (TODO(#875)). It switches
    /// section only by an `on_use` condition, which the sections here lack,
    /// so the door does not move and nothing else changes.
    pub fn use_action(&mut self) -> Option<DoorSound> {
        if !self.started || !self.section.locked {
            return None;
        }
        let set = self.section.snd_open_start.as_deref()?;
        Some(DoorSound {
            sound: sound_set(set)?,
            position: self.position,
        })
    }

    /// The sounds its sections can play (2.5), for creating them with the
    /// level's.
    pub fn sounds(&self) -> impl Iterator<Item = &'static str> {
        (self.section.snd_open_start.as_deref())
            .and_then(sound_set)
            .into_iter()
    }

    /// The object's update: the first starts the scheme (2.1; believed
    /// the first frame of play).
    pub fn update(&mut self) {
        if std::mem::replace(&mut self.started, true) {
            return;
        }
        // 2.2: a closed section closes the door; a locked one sets the
        // locked hint.
        if self.section.closed {
            self.set_motor(CLOSING);
            self.to_block = true;
        }
        if self.section.locked && self.section.show_tips {
            self.tip = Some(self.section.locked_tip.clone());
        }
    }

    /// One physics step's per-step call (2.2): once the door is `closed`
    /// it is held. Closed is the hinge angle at most the low limit the
    /// script receives, −0 for a hinge whose first element is fixed (2.3;
    /// physics-shell-shapeless-root.md 2.2).
    pub fn physics_step(&mut self, closed: bool) {
        if self.to_block && closed {
            self.to_block = false;
            self.set_motor(HOLDING);
        }
    }

    fn set_motor(&mut self, motor: Motor) {
        self.motor = Some(motor);
        self.motor_set = true;
    }

    /// The motor set since last taken, for the hinge (2.3).
    pub fn take_motor(&mut self) -> Option<Motor> {
        std::mem::take(&mut self.motor_set)
            .then_some(self.motor)
            .flatten()
    }

    /// The leaf's boxes where its shell holds them.
    pub fn set_boxes(&mut self, boxes: Vec<BodyBox>) {
        self.boxes = boxes;
    }

    pub fn started(&self) -> bool {
        self.started
    }

    pub fn motor(&self) -> Option<Motor> {
        self.motor
    }

    /// The object's hint id (5.1), if any.
    pub fn tip(&self) -> Option<&str> {
        self.tip.as_deref()
    }

    /// The leaf's boxes where its shell holds them (1).
    pub fn boxes(&self) -> &[BodyBox] {
        &self.boxes
    }

    /// The distance along the unit `dir` from `origin` at which the ray
    /// meets the object's boxes, if it does. Which shapes the crosshair's
    /// pick tests is unestablished (5.1, `hud-scope` 14): the boxes here
    /// (TODO(#181)).
    pub fn pick(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        self.boxes
            .iter()
            .filter_map(|b| b.ray(origin, dir))
            .min_by(f32::total_cmp)
    }
}

#[cfg(test)]
mod tests;
