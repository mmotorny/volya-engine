// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Which of the level's objects are online at the first frame of a New
//! Game (docs/behavior/level-objects.md section 2): the switch distances of
//! `[alife]`, each object's switch capability (the engine's predicates and
//! the shipped scripts' replacements) and the rule, evaluated once at the
//! start positions (section 2.4).
//!
//! No scripts run here. The scripts' inputs at a New Game are taken as
//! section 2.5 states them: no job-online value is set, the actor holds no
//! information portion, so no `[spawner]` condition holds.

use std::io;

use crate::config::{Config, value};
use volya_engine_formats::ltx;
use volya_engine_formats::object::{Kind, Object, ObjectRecord};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// State flags bit: the object can switch online (section 2.3).
pub const STATE_SWITCH_ONLINE: u32 = 0x02;
/// State flags bit: the object can switch offline.
pub const STATE_SWITCH_OFFLINE: u32 = 0x04;
/// Hanging-lamp light flags: the lamp exists for the first renderer.
pub const LAMP_FIRST_RENDERER: u16 = 0x04;
/// … for the second renderer.
pub const LAMP_SECOND_RENDERER: u16 = 0x08;
/// Job-online mode meaning "not set" (section 2.5: 2 in every record).
const JOB_ONLINE_NOT_SET: u8 = 2;
/// Job-online mode followed by a condition list.
const JOB_ONLINE_CONDITION_LIST: u8 = 3;
/// The sections of the scripted artefacts with their own rule (2.5).
const ELECTRA_ARTEFACTS: [&str; 3] = [
    "af_electra_flash",
    "af_electra_moonlight",
    "af_electra_sparkler",
];
/// The electra artefacts' distance threshold, metres (2.5).
const ELECTRA_DISTANCE: f32 = 50.0;

/// The online and offline distances (section 2.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwitchDistances {
    pub online: f32,
    pub offline: f32,
}

impl SwitchDistances {
    /// `d · (1 − f)` and `d · (1 + f)` from `[alife] switch_distance` and
    /// `switch_factor` of `alife`.
    pub fn read(alife: &Config) -> io::Result<Self> {
        let get = |key: &str| {
            alife
                .get("alife", key)
                .flatten()
                .map(value::float)
                .ok_or_else(|| invalid(format!("[alife] has no {key}")))
        };
        let d = get("switch_distance")?;
        let f = get("switch_factor")?;
        Ok(Self {
            online: d * (1.0 - f),
            offline: d * (1.0 + f),
        })
    }
}

/// What the scripted replacements read at the start (section 2.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartState {
    /// The actor's position.
    pub actor: [f32; 3],
    /// The game hour, 0 … 23.
    pub hour: u32,
}

/// What the switch predicates read from a record.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub kind: Kind,
    pub class_id: String,
    pub section: String,
    pub name: String,
    pub position: [f32; 3],
    pub has_parent: bool,
    /// The Object block's state flags.
    pub state_flags: u32,
    /// The Creature block's health, for creatures.
    pub health: Option<f32>,
    /// Ammunition's rounds left.
    pub rounds_left: Option<u16>,
    /// A hanging lamp's light flags.
    pub lamp_flags: Option<u16>,
    /// A stalker's or monster's job-online value; `None` when not set.
    pub job_online: Option<bool>,
    /// Whether the custom data has a `[spawner]` section.
    pub spawner: bool,
}

impl Candidate {
    pub fn of(record: &ObjectRecord) -> Self {
        let o = &record.object;
        Self {
            kind: record.kind(),
            class_id: record.class_id.as_str().to_owned(),
            section: record.header.section.clone(),
            name: record.header.name.clone(),
            position: record.header.position,
            has_parent: record.header.parent_id != 0xFFFF,
            state_flags: o.object_block().state_flags,
            health: o.creature().map(|c| c.health),
            rounds_left: match o {
                Object::Ammunition(a) => Some(a.rounds_left),
                _ => None,
            },
            lamp_flags: match o {
                Object::HangingLamp(l) => Some(l.light_flags),
                _ => None,
            },
            job_online: job_online(o),
            spawner: has_spawner(&o.object_block().custom_data),
        }
    }

    /// The renderer match `m` of section 2.3 with the first renderer in
    /// use: true except for a hanging lamp whose light flags lack bit 0x04.
    /// A lamp with neither renderer bit is the game's fatal error.
    pub fn renderer_match(&self) -> io::Result<bool> {
        match self.lamp_flags {
            Some(flags) if flags & (LAMP_FIRST_RENDERER | LAMP_SECOND_RENDERER) == 0 => {
                Err(invalid(format!(
                    "{}: no renderer type set for hanging-lamp",
                    self.name
                )))
            }
            Some(flags) => Ok(flags & LAMP_FIRST_RENDERER != 0),
            None => Ok(true),
        }
    }

    /// The engine's "can switch online" (section 2.3).
    pub fn engine_on(&self, m: bool) -> bool {
        m && self.state_flags & STATE_SWITCH_ONLINE != 0
    }

    /// The engine's "can switch offline" (section 2.3).
    pub fn engine_off(&self, m: bool) -> bool {
        use Kind::*;
        let base = !m || self.state_flags & STATE_SWITCH_OFFLINE != 0;
        match self.kind {
            Zone | Anomaly | VisualAnomaly | TorridZone | LevelChanger | Restrictor | SmartZone
            | TeamBase | Breakable | Climbable => false,
            Player | Stalker | Monster | CrowOrPhantom | MonsterGroup => {
                base && self.health.is_some_and(|h| h > 0.0)
            }
            Ammunition => base && self.rounds_left.is_some_and(|r| r != 0),
            _ => base,
        }
    }

    /// Both predicates `(on, off)` of an offline object at a New Game: the
    /// engine's, or the shipped scripts' replacements (section 2.5), with
    /// the scripts' distances from the record's position.
    pub fn predicates(&self, start: &StartState) -> io::Result<(bool, bool)> {
        self.predicates_in(start, false, self.position)
    }

    /// Both predicates `(on, off)` of the object, `online` or not, the
    /// scripts' distances measured from `node`, the position of its
    /// level-graph node (section 2.5). A `[spawner]` condition never holds
    /// (section 2.5: each requires an information portion the actor lacks
    /// at the start; we do not evaluate condition lists, and take any for
    /// false). The actor exists and is alive (our choice: the player's
    /// death is not modelled).
    pub fn predicates_in(
        &self,
        state: &StartState,
        online: bool,
        node: [f32; 3],
    ) -> io::Result<(bool, bool)> {
        let m = self.renderer_match()?;
        let on = self.engine_on(m);
        let off = self.engine_off(m);
        let class = self.class_id.as_str();
        let job = self.job_online;
        let off_scripted = job.map_or(off, |j| !j);
        Ok(if class == "AI_STL_S" {
            // The job value if set; else the engine's without a spawner;
            // else the condition (false), which while offline also needs
            // the engine's. A stalker's `dont_spawn_online` field is a
            // script variable no shipped script sets (section 2.5), so it
            // is taken as false (our omission: no scripts run here).
            let on = match job {
                Some(j) => j,
                None if !self.spawner => on,
                None => false,
            };
            (on, off_scripted)
        } else if class.starts_with("SM_") {
            // As the stalker's, except that an online monster with a
            // spawner whose condition fails keeps the engine's `on` (no
            // record sets `check_distance`).
            let on = match job {
                Some(j) => j,
                None if !self.spawner || online => on,
                None => false,
            };
            (on, off_scripted)
        } else if class == "SCRPTART" && ELECTRA_ARTEFACTS.contains(&self.section.as_str()) {
            let night = state.hour <= 4;
            let d = distance(node, state.actor);
            let on = if self.has_parent {
                true
            } else if online {
                night || d < ELECTRA_DISTANCE
            } else {
                night && d >= ELECTRA_DISTANCE
            };
            (on, off)
        } else if class == "SCRPTCAR" {
            (on && !self.spawner, off)
        } else {
            (on, off)
        })
    }

    /// Whether the object, of the actor's level and offline at the start,
    /// is online at the first visible frame: one evaluation of section 2.4
    /// at the stored positions, `on` and (not `off` or distance ≤ the
    /// online distance).
    pub fn online_at_start(
        &self,
        start: &StartState,
        distances: &SwitchDistances,
    ) -> io::Result<bool> {
        let (on, off) = self.predicates(start)?;
        Ok(on && (!off || distance(self.position, start.actor) <= distances.online))
    }
}

/// The rule of section 2.4: whether an object, `online` or not, is online
/// after one evaluation with predicates `on` and `off` at `distance` from
/// the actor.
pub fn switched(
    online: bool,
    (on, off): (bool, bool),
    distance: f32,
    distances: &SwitchDistances,
) -> bool {
    if online {
        !off || (on && distance <= distances.offline)
    } else {
        on && (!off || distance <= distances.online)
    }
}

/// A stalker's or monster's job-online value: `None` when not set. Modes
/// 0 and 1 are taken for false and true, and a condition list for false
/// (our reading and choice: section 2.5 names the values, not their codes,
/// and no record at a New Game has one set).
fn job_online(object: &Object) -> Option<bool> {
    let script = match object {
        Object::Stalker(s) => s.script.as_ref(),
        Object::Monster(m) => m.script.as_ref(),
        _ => None,
    }?;
    match script.job_online.as_ref()?.mode {
        JOB_ONLINE_NOT_SET => None,
        JOB_ONLINE_CONDITION_LIST => Some(false),
        m => Some(m == 1),
    }
}

/// Whether custom data `text` has a `[spawner]` section. Text that is not
/// valid LTX counts as none (our choice).
fn has_spawner(text: &str) -> bool {
    let bytes = volya_engine_formats::cp1251::encode(text).unwrap_or_default();
    ltx::parse(&bytes).is_ok_and(|c| c.section("spawner").is_some())
}

/// The Euclidean distance between two points.
pub fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(class: &str, flags: u32, position: [f32; 3]) -> Candidate {
        Candidate {
            kind: Kind::from_class_id(class).unwrap(),
            class_id: class.into(),
            section: "s".into(),
            name: "x".into(),
            position,
            has_parent: false,
            state_flags: flags,
            health: None,
            rounds_left: None,
            lamp_flags: None,
            job_online: None,
            spawner: false,
        }
    }

    fn lamp(flags: u32, light_flags: u16, position: [f32; 3]) -> Candidate {
        Candidate {
            lamp_flags: Some(light_flags),
            ..candidate("O_HLAMP", flags, position)
        }
    }

    fn stalker(flags: u32, health: f32, position: [f32; 3]) -> Candidate {
        Candidate {
            health: Some(health),
            ..candidate("AI_STL_S", flags, position)
        }
    }

    const START: StartState = StartState {
        actor: [0.0; 3],
        hour: 5,
    };
    const DISTANCES: SwitchDistances = SwitchDistances {
        online: 135.0,
        offline: 165.0,
    };

    fn online(c: &Candidate) -> bool {
        c.online_at_start(&START, &DISTANCES).unwrap()
    }

    #[test]
    fn distances_from_alife() {
        let c = ltx::parse(b"[alife]\nswitch_distance = 150\nswitch_factor = 0.1\n").unwrap();
        let d = SwitchDistances::read(&c).unwrap();
        assert!((d.online - 135.0).abs() < 1e-4 && (d.offline - 165.0).abs() < 1e-4);
        assert!(SwitchDistances::read(&ltx::parse(b"[alife]\n").unwrap()).is_err());
        assert!(has_spawner("[spawner]\ncond = {+esc_dog_spawn}\n"));
        assert!(!has_spawner("[logic]\nactive = walker\n"));
    }

    #[test]
    fn lamps_need_the_first_renderer() {
        let far = [500.0, 0.0, 0.0];
        // 0xB55C: first and second renderer; flags without 0x04 (offline
        // switching): online at any distance.
        assert!(online(&lamp(0x03, 0xB55C, far)));
        // Second renderer only: m false, never online.
        assert!(!online(&lamp(0x03, 0x0008, far)));
        assert!(lamp(0x03, 0x0010, far).renderer_match().is_err());
        // Bit 0x04 set: online only within 135 m.
        assert!(!online(&lamp(0x07, 0x04, far)));
        assert!(online(&lamp(0x07, 0x04, [135.0, 0.0, 0.0])));
        // Without bit 0x02 never.
        assert!(!online(&lamp(0x05, 0x04, [1.0, 0.0, 0.0])));
    }

    #[test]
    fn kinds_and_distances() {
        let far = [250.0, 0.0, 0.0];
        let near = [25.0, 0.0, 0.0];
        // Breakables and restrictors never switch offline.
        assert!(online(&candidate("O_BRKBL", 0x07, far)));
        assert!(online(&candidate("SPC_RS_S", 0x07, far)));
        // Physics objects with 0x04 switch by distance.
        assert!(!online(&candidate("O_PHYS_S", 0x07, far)));
        assert!(online(&candidate("O_PHYS_S", 0x07, near)));
        // Ammunition with no rounds never switches offline.
        let empty = Candidate {
            rounds_left: Some(0),
            ..candidate("AMMO", 0x07, far)
        };
        assert!(online(&empty));
        let full = Candidate {
            rounds_left: Some(10),
            ..empty
        };
        assert!(!online(&full));
    }

    #[test]
    fn creatures_and_scripts() {
        let near = [25.0, 0.0, 0.0];
        let far = [250.0, 0.0, 0.0];
        // Camp stalkers (0xFFFFFFBF) online by distance.
        assert!(online(&stalker(0xFFFF_FFBF, 1.0, near)));
        assert!(!online(&stalker(0xFFFF_FFBF, 1.0, far)));
        // Soldiers (0xFFFFFFBB: no 0x04) and the dead at any distance.
        assert!(online(&stalker(0xFFFF_FFBB, 1.0, far)));
        assert!(online(&stalker(0xFFFF_FFBF, 0.0, far)));
        // A [spawner] section keeps them out.
        let spawner = Candidate {
            spawner: true,
            ..stalker(0xFFFF_FFBB, 1.0, near)
        };
        assert!(!online(&spawner));
        // A set job-online value decides both predicates.
        let job = |j| Candidate {
            job_online: Some(j),
            ..spawner.clone()
        };
        assert!(online(&Candidate {
            position: far,
            ..job(true)
        }));
        assert!(!online(&job(false)));
        // The electra artefacts: not at 05:30.
        let electra = Candidate {
            section: "af_electra_flash".into(),
            ..candidate("SCRPTART", 0x03, [60.0, 0.0, 0.0])
        };
        assert!(!online(&electra));
        let night = StartState { hour: 2, ..START };
        assert!(electra.online_at_start(&night, &DISTANCES).unwrap());
    }
}
