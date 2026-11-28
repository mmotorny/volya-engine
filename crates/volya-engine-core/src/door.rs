// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The door scheme `ph_door` of a physics object
//! (docs/behavior/trader-door.md, cited by section): the scheme's section
//! (2.1), its start (2.2) and per-step call (2.2, 2.3), the hint it sets
//! (5.1), and the door leaf's boxes the player's body meets (1, 3).
//!
//! The shipped scripts do not run yet: the scheme here is the spec's
//! statement of what the scripts do, standing in for them (TODO(#875)). The
//! leaf's shell is not simulated: how a shell is built for a model whose
//! root bone has no shape, and whether a shell held at rest sleeps and
//! still meets the player's body, are open (TODO(#873)). The simplest
//! behaviour consistent with 2.4 stands in: a closed door's leaf stays at
//! its placed pose, the hinge's closed stop and its motor (up to 10 000 of
//! torque, 2.4) holding it against the player, whose push is far weaker
//! (believed; how far it yields is unestablished, 3.3).

use std::io;

use volya_engine_formats::bone_physics::{self, UsedShape};
use volya_engine_formats::gamemtl::GameMaterial;
use volya_engine_formats::ltx::{Section, value};
use volya_engine_formats::model::{Mat4, Model, multiply, transform_point};

use crate::collision::body::BodyBox;
use crate::collision::geometry::{Vec3, normalize};

/// The door scheme's name: a logic section `ph_door` or `ph_door@…`.
pub const SCHEME: &str = "ph_door";
/// The locked hint id when the section has no `tip_open` (2.1).
pub const LOCKED_TIP: &str = "tip_door_locked";
/// The unlocked closed section's hint id (4.3).
pub const OPEN_TIP: &str = "tip_door_open";

/// The sections the trader's door switches to (4.1, 4.3): unlocked and
/// closed, then open once used.
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

/// What a use of the door did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Used {
    /// Nothing changed.
    Nothing,
    /// The stub unlocked it (TODO(#876)).
    Unlocked,
    /// It opened: its leaf leaves the way.
    Opened,
}

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
        }
    }
}

/// The boxes of a type-3 shell's bones (`physics-scope` 1.2) at the bind
/// pose under `transform`, the record's (2.3: the shell is believed built
/// in the bind pose): every bone with a used box shape (`bone` 2.2), with
/// its material's index in `materials`. Fixed bones (`fixed`,
/// comma-separated) must be bones of the model ("wrong fixed bone", fatal
/// in the original). Spheres and cylinders are not met (none on a door).
pub fn shell_boxes(
    model: &Model,
    transform: &Mat4,
    fixed: &str,
    materials: &[GameMaterial],
) -> io::Result<Vec<BodyBox>> {
    for name in fixed.split(',').map(str::trim).filter(|b| !b.is_empty()) {
        if model.bone(&name.to_lowercase()).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("wrong fixed bone '{name}'"),
            ));
        }
    }
    let pose = model.bind_pose()?;
    let mut boxes = Vec::new();
    for (bone, bind) in model.bones.iter().zip(&pose) {
        let Some(UsedShape::Box(b)) = bone.physics.shape.used() else {
            continue;
        };
        let m = multiply(bind, transform);
        let turn = |v: Vec3| -> Vec3 {
            let w: Vec3 = std::array::from_fn(|c| (0..3).map(|k| v[k] * m[k][c]).sum());
            normalize(w).unwrap_or(w)
        };
        boxes.push(BodyBox {
            centre: transform_point(&m, b.centre),
            axes: b.axes.map(turn),
            half_size: b.collision_half_size(),
            material: bone_physics::material(&bone.physics.material, materials)? as u32,
        });
    }
    Ok(boxes)
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
    tip: Option<String>,
    boxes: Vec<BodyBox>,
    /// The section the unlock switches to (4.3).
    unlocked: Option<DoorSection>,
    open: bool,
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
            tip: None,
            boxes,
            unlocked: None,
            open: false,
        }
    }

    /// The section the unlock switches to (4.3): the logic file's
    /// [`UNLOCKED_SECTION`].
    pub fn with_unlocked(mut self, section: DoorSection) -> Self {
        self.unlocked = Some(section);
        self
    }

    /// The use action on the door (2.5, 4.3), its scheme started.
    ///
    /// TODO(#876): a stub for the trader's dialog, which gives the info
    /// portion that unlocks the door (4.1): until it exists, using the
    /// locked door unlocks it, so the bunker can be left. The unlock then
    /// does what 4.3 says (the section unlocked and closed, closed again,
    /// the hint `tip_door_open`); its sound is not played. A use of the
    /// unlocked closed door opens it (4.3): the motor toward the open
    /// stop, and the leaf, whose swing is not simulated (#873), out of
    /// the way; its hint none (our choice: the open section's is PH-20's).
    pub fn use_action(&mut self) -> Used {
        if !self.started || self.open {
            return Used::Nothing;
        }
        if self.section.locked {
            let Some(next) = self.unlocked.take() else {
                return Used::Nothing;
            };
            self.section = next;
            if self.section.closed {
                self.motor = Some(CLOSING);
                self.to_block = true;
            }
            self.tip =
                (!self.section.locked && self.section.show_tips).then(|| OPEN_TIP.to_owned());
            return Used::Unlocked;
        }
        self.open = true;
        self.to_block = false;
        self.motor = Some(OPENING);
        self.tip = None;
        self.section.name = OPEN_SECTION.to_owned();
        Used::Opened
    }

    /// Whether it was opened.
    pub fn is_open(&self) -> bool {
        self.open
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
            self.motor = Some(CLOSING);
            self.to_block = true;
        }
        if self.section.locked && self.section.show_tips {
            self.tip = Some(self.section.locked_tip.clone());
        }
    }

    /// One physics step's per-step call (2.2): once the door is closed it
    /// is held. The leaf stays at its placed pose, angle 0, which the
    /// closed test (angle ≤ −0, 2.3) passes.
    pub fn physics_step(&mut self) {
        if std::mem::take(&mut self.to_block) {
            self.motor = Some(HOLDING);
        }
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

    /// The leaf's boxes the player's body meets (3.1).
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
