// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Game object basics (script API M,
//! docs/behavior/script-api-game-object-basics.md; section numbers are
//! its): the handle `game_object` with its identity, place, condition,
//! zone, use and trade members and its constants ([`members`]), the class
//! `hit`, the class `CGameObject` with the overrides the engine calls, the
//! engine client classes (with the unlisted `CPhysicObject`) and
//! `explosive` ([`engine_classes`]); object binders (script API L,
//! [`object_binder`]); script API Y's physics, vehicles, helicopter
//! and lamps ([`physics`], [`vehicles`]).
//!
//! What scripts read and change is [`GameObjects`]: the engine objects as
//! scripts see them ([`GameObject`], made from their spawn data,
//! [`SpawnData`]), and the events members raise for the engine to apply
//! ([`Event`]). [`Scripts::register_game_object_basics`] registers the
//! names; the engine spawns, uses and destroys objects through
//! [`Scripts::spawn`], [`Scripts::spawn_engine_object`],
//! [`Scripts::use_object`], [`Scripts::player_use`] and
//! [`Scripts::destroy_object`]. See docs/architecture/scripts.md, Game
//! objects.

pub mod callbacks;
mod engine_classes;
pub mod members;
pub mod monsters;
pub mod movement;
pub mod object_binder;
pub mod physics;
pub mod sight;
pub mod vehicles;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::rc::{Rc, Weak};

use volya_engine_core::config::Config;
use volya_engine_core::physics::shell::Shell;
use volya_engine_core::simulation::{NO_ID, class_of};
use volya_engine_formats::cp1251;
use volya_engine_formats::model::{self, Mat4};
use volya_engine_formats::object::ObjectRecord;

use crate::classes::{Conversion, EnginePart, OverrideFailure};
use crate::dialogs::talk::{Talk, has_start_dialog};
use crate::dialogs::trade::TradeParameters;
use crate::files::{FileSystem, IniFile, config_from_text};
use crate::loader::Scripts;
use crate::object_factory::{ObjectFactory, class_id};
use crate::scripted_actions::{ScriptControl, is_scripted};
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::{Error, ErrorKind, Value};

pub use engine_classes::{ClientObject, Explosive, Hit, HitData};

/// The kinds an engine object answers (2.2): each a kind query that is
/// "none" for an object without it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Kinds(u16);

impl Kinds {
    pub const NONE: Kinds = Kinds(0);
    pub const ENTITY: Kinds = Kinds(1);
    pub const LIVING: Kinds = Kinds(2);
    pub const INVENTORY_OWNER: Kinds = Kinds(4);
    pub const FIGHTER: Kinds = Kinds(8);
    pub const STALKER: Kinds = Kinds(16);
    pub const PLAYER: Kinds = Kinds(32);
    pub const ITEM: Kinds = Kinds(64);
    pub const PHYSICAL: Kinds = Kinds(128);
    pub const ANOMALOUS_ZONE: Kinds = Kinds(256);
    pub const RESTRICTOR: Kinds = Kinds(512);
    /// Script API Y 2: the holder and explosive kinds.
    pub const HOLDER: Kinds = Kinds(1024);
    pub const EXPLOSIVE: Kinds = Kinds(2048);
    /// Script API X 2: the monster, scripted and weapon kinds.
    pub const MONSTER: Kinds = Kinds(4096);
    pub const SCRIPTED: Kinds = Kinds(8192);
    pub const WEAPON: Kinds = Kinds(16384);

    pub const fn with(self, other: Kinds) -> Kinds {
        Kinds(self.0 | other.0)
    }

    pub const fn has(self, kind: Kinds) -> bool {
        self.0 & kind.0 == kind.0 && kind.0 != 0
    }

    /// The kinds of the 19 creature classes (2.2); without fighter for
    /// `CAI_Trader`.
    const CREATURE: Kinds = Kinds::ENTITY
        .with(Kinds::LIVING)
        .with(Kinds::INVENTORY_OWNER)
        .with(Kinds::FIGHTER)
        .with(Kinds::PHYSICAL);
    const TRADER: Kinds = Kinds::ENTITY
        .with(Kinds::LIVING)
        .with(Kinds::INVENTORY_OWNER)
        .with(Kinds::PHYSICAL);
    const ITEM_KINDS: Kinds = Kinds::ITEM.with(Kinds::PHYSICAL);
    const ZONE: Kinds = Kinds::ANOMALOUS_ZONE.with(Kinds::RESTRICTOR);
    /// The player's object (2.2).
    pub const OF_PLAYER: Kinds = Kinds::ENTITY
        .with(Kinds::LIVING)
        .with(Kinds::INVENTORY_OWNER)
        .with(Kinds::PLAYER)
        .with(Kinds::PHYSICAL);
}

/// The 19 creature classes (2.2).
pub const CREATURE_CLASSES: [&str; 19] = [
    "CAI_Bloodsucker",
    "CAI_Boar",
    "CAI_Dog",
    "CAI_Flesh",
    "CAI_PseudoDog",
    "CAI_Stalker",
    "CAI_Trader",
    "CBurer",
    "CCat",
    "CChimera",
    "CController",
    "CFracture",
    "CPoltergeist",
    "CPseudoGigant",
    "CPsyDog",
    "CPsyDogPhantom",
    "CSnork",
    "CTushkano",
    "CZombie",
];

/// The 36 item classes (2.2).
pub const ITEM_CLASSES: [&str; 36] = [
    "CWeaponAK74",
    "CWeaponBM16",
    "CWeaponBinoculars",
    "CWeaponFN2000",
    "CWeaponFORT",
    "CWeaponGroza",
    "CWeaponHPSA",
    "CWeaponKnife",
    "CWeaponLR300",
    "CWeaponPM",
    "CWeaponRG6",
    "CWeaponRPG7",
    "CWeaponSVD",
    "CWeaponSVU",
    "CWeaponShotgun",
    "CWeaponUSP45",
    "CWeaponVal",
    "CWeaponVintorez",
    "CWeaponWalther",
    "CF1",
    "CRGD5",
    "CScope",
    "CStalkerOutfit",
    "CTorch",
    "CBastArtefact",
    "CBlackDrops",
    "CBlackGraviArtefact",
    "CDummyArtefact",
    "CElectricBall",
    "CFadedBall",
    "CGalantineArtefact",
    "CGraviArtefact",
    "CMercuryBall",
    "CRustyHairArtefact",
    "CThornArtefact",
    "CZudaArtefact",
];

/// The 3 anomalous-zone classes (2.2).
pub const ZONE_CLASSES: [&str; 3] = ["CHairsZone", "CMincer", "CMosquitoBald"];

/// The client classes registered with `CGameObject` as their base (10.3):
/// the 59 listed (creatures, items, zones and `CSpaceRestrictor`) and the
/// unlisted `CPhysicObject`.
pub fn client_classes() -> impl Iterator<Item = &'static str> {
    CREATURE_CLASSES
        .into_iter()
        .chain(ITEM_CLASSES)
        .chain(ZONE_CLASSES)
        .chain(["CSpaceRestrictor", "CPhysicObject"])
}

/// The engine class the client classes derive from (10.1).
pub const CLIENT_BASE: &str = "CGameObject";

/// The client classes registered with `DLL_Pure` as their base (10.3).
pub const SCRIPT_ZONE_CLASSES: [&str; 2] = ["ce_script_zone", "ce_smart_zone"];

/// The class `active_zone_contact` tests for (6.4).
pub const SCRIPT_ZONE: &str = "ce_script_zone";

/// The class `set_invisible` and `set_manual_invisibility` test for (6.5).
pub const BLOODSUCKER: &str = "CAI_Bloodsucker";

/// The class of an engine object: one scripts can name, or the player's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectClass {
    Named(&'static str),
    /// The player's object, whose class scripts can't name (2.2).
    Player,
    /// An object of an engine entry's kind (spec C 2.2, 6.1) other than
    /// the player's, whose class scripts can't name: its entry's class id.
    /// Which kinds such an object has is outside the specs: none
    /// (TODO(#1070)).
    Unnamed(u64),
}

impl ObjectClass {
    /// The kinds its objects have (2.2). `CCar` and `CHelicopter` (spec Y's
    /// classes) are entities with a physics part; a class the table lacks
    /// has none.
    pub fn kinds(self) -> Kinds {
        let name = match self {
            ObjectClass::Named(name) => name,
            ObjectClass::Player => return Kinds::OF_PLAYER,
            ObjectClass::Unnamed(_) => return Kinds::NONE,
        };
        // Script API X 2: every creature class is scripted, the 17 but the
        // stalker and the trader monsters, the 19 `CWeapon` classes weapons.
        if name == "CAI_Trader" {
            Kinds::TRADER.with(Kinds::SCRIPTED)
        } else if name == "CAI_Stalker" {
            Kinds::CREATURE.with(Kinds::STALKER).with(Kinds::SCRIPTED)
        } else if CREATURE_CLASSES.contains(&name) {
            Kinds::CREATURE.with(Kinds::MONSTER).with(Kinds::SCRIPTED)
        } else if name.starts_with("CWeapon") && ITEM_CLASSES.contains(&name) {
            Kinds::ITEM_KINDS.with(Kinds::WEAPON)
        } else if name == "CF1" || name == "CRGD5" {
            // Script API Y 2: the grenade classes are explosive.
            Kinds::ITEM_KINDS.with(Kinds::EXPLOSIVE)
        } else if ITEM_CLASSES.contains(&name) {
            Kinds::ITEM_KINDS
        } else if ZONE_CLASSES.contains(&name) {
            Kinds::ZONE
        } else if name == "CSpaceRestrictor" || SCRIPT_ZONE_CLASSES.contains(&name) {
            Kinds::RESTRICTOR
        } else if name == "CPhysicObject" {
            Kinds::PHYSICAL
        } else if name == physics::CAR {
            // Script API Y 2: the car is a holder; it and the helicopter
            // are explosive, the lamps physical. Script API X 2: the car is
            // scripted.
            Kinds::ENTITY
                .with(Kinds::PHYSICAL)
                .with(Kinds::HOLDER)
                .with(Kinds::EXPLOSIVE)
                .with(Kinds::SCRIPTED)
        } else if name == physics::HELICOPTER {
            Kinds::ENTITY.with(Kinds::PHYSICAL).with(Kinds::EXPLOSIVE)
        } else if name == physics::HANGING_LAMP {
            Kinds::PHYSICAL
        } else {
            Kinds::NONE
        }
    }

    fn is(self, class: &str) -> bool {
        matches!(self, ObjectClass::Named(name) if name == class)
    }
}

/// What an engine object spawns from (3): its spawn data, the simulation
/// object or spawn record it is created from.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnData {
    pub id: u16,
    /// The object name (may be empty), the game's bytes.
    pub name: Vec<u8>,
    pub section: Vec<u8>,
    /// The class id of the section's `class` (spec C 1).
    pub class_id: u64,
    /// The simulation part's story id; −1 for none.
    pub story_id: i32,
    /// The simulation part's custom data (3, `spawn_ini`).
    pub custom_data: Vec<u8>,
    pub parent: Option<u16>,
    /// The placement: rows i, j, k (axes) and c (origin) (spec E).
    pub placement: Mat4,
    pub level_vertex: u32,
    pub game_vertex: u16,
    /// An entity's health at spawn.
    pub health: Option<f32>,
    /// The simulation object it spawns from (spec J), which `net_Spawn`
    /// and the binder's `net_spawn` get as their `data`; nil without one.
    pub server: Option<EnginePart>,
}

/// The game's bytes of `text` (Windows-1251).
pub(crate) fn game_bytes(text: &str) -> Vec<u8> {
    cp1251::encode(text).unwrap_or_else(|| text.as_bytes().to_vec())
}

impl SpawnData {
    /// The spawn data of simulation object `id` whose record is `record`,
    /// its section's class read from `system` (`config\system.ltx`).
    pub fn from_record(id: u16, record: &ObjectRecord, system: &Config) -> io::Result<Self> {
        let h = &record.header;
        let block = record.object.object_block();
        let class = class_of(system, &h.section)?;
        Ok(SpawnData {
            id,
            name: game_bytes(&h.name),
            section: game_bytes(&h.section),
            class_id: class_id(&game_bytes(class)),
            story_id: block.story_id,
            custom_data: game_bytes(&block.custom_data),
            parent: (h.parent_id != NO_ID).then_some(h.parent_id),
            placement: model::transform(h.position, h.angles),
            level_vertex: block.level_graph_node,
            game_vertex: block.game_graph_vertex,
            health: record.object.creature().map(|c| c.health),
            server: None,
        })
    }
}

/// The visual of an engine object, as the place members read it (4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Visual {
    /// Its `IRender_Visual` (spec B 5.2, [`Scripts::new_visual`]), which
    /// `CGameObject:Visual()` returns.
    pub part: Option<EnginePart>,
    /// The centre of its bounding sphere, in the object's space.
    pub centre: [f32; 3],
    /// Its bones: name and current transform, in the object's space.
    pub bones: Vec<(Vec<u8>, Mat4)>,
    /// The root bone's index.
    pub root: usize,
}

impl Visual {
    /// The index of bone `name`.
    pub fn bone(&self, name: &[u8]) -> Option<usize> {
        self.bones.iter().position(|(n, _)| n == name)
    }
}

/// An entity's part (5.2).
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    /// The health `alive()` tests, which the living's `health` reads too
    /// (believed the same, 5.2).
    pub health: f32,
    pub max_health: f32,
    /// The device's global time it died at, ms; 0 while it hasn't.
    pub death_time: u32,
    pub killer: Option<u16>,
    /// The game time recorded at its death.
    pub death_game_time: u64,
}

impl Default for Entity {
    fn default() -> Self {
        Entity {
            health: 1.0,
            max_health: 1.0,
            death_time: 0,
            killer: None,
            death_game_time: 0,
        }
    }
}

/// The conditions of a living object (5.1, 5.3), its health the entity's.
#[derive(Clone, Debug, PartialEq)]
pub struct Conditions {
    pub psy_health: f32,
    pub power: f32,
    pub radiation: f32,
    pub morale: f32,
    /// Each wound's 11 per-hit-type amounts.
    pub wounds: Vec<[f32; 11]>,
    /// The object recorded as having hit it last.
    pub who_hit: Option<u16>,
    /// Whether it may be harmed: what lets a zero or negative `health`
    /// assignment through (5.1): the engine's per-object flag and its
    /// network test as one value (which network states pass is
    /// unestablished; the engine sets it).
    pub may_be_harmed: bool,
}

impl Default for Conditions {
    fn default() -> Self {
        Conditions {
            psy_health: 1.0,
            power: 1.0,
            radiation: 0.0,
            morale: 1.0,
            wounds: Vec::new(),
            who_hit: None,
            may_be_harmed: true,
        }
    }
}

/// An inventory owner's part (7).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InventoryOwner {
    /// Its own trade parameters (script API R 5.2, 5.3).
    pub trade: TradeParameters,
    /// The character profile id, if any.
    pub profile: Option<Vec<u8>>,
    pub character_name: Vec<u8>,
    /// Its talk state (script API R 4).
    pub talk: Talk,
}

/// A fighter's part (5.5).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fighter {
    pub invulnerable: bool,
    /// A critical wound is recorded (its state isn't "none").
    pub critically_wounded: bool,
    /// The corpse its memory has selected.
    pub corpse: Option<u16>,
    /// The body's current and target headings, radians.
    pub body_heading: (f32, f32),
}

/// A stalker's part (5.5).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stalker {
    pub wounded: bool,
    /// Its second current and target headings (believed its head's).
    pub head_heading: (f32, f32),
    /// Its memory's choices, which its planner reads
    /// (docs/behavior/stalker-planner.md 1.2): nothing chooses them yet
    /// (CH-15, TODO(#1131)).
    pub choices: Choices,
    /// The objects its touch sense touches (stalker-planner.md 1.2, 5.1
    /// step 4): nothing touches yet (TODO(#1131)).
    pub touching: Vec<u16>,
}

/// A stalker's memory choices (CH-15; script API X 8, 9).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choices {
    /// The chosen enemy (`best_enemy`).
    pub enemy: Option<u16>,
    /// The clock (`time_global()`) at the last update of the enemy choice
    /// that had an enemy; `None` before any.
    pub enemy_chosen_at: Option<u32>,
    /// Whether the danger choice (`best_danger`) has a danger.
    pub danger: bool,
    /// The chosen item.
    pub item: Option<u16>,
}

/// An item's part (5.4).
#[derive(Clone, Debug, PartialEq)]
pub struct ItemState {
    /// 0 … 1.
    pub condition: f32,
    pub cost: u32,
}

impl Default for ItemState {
    fn default() -> Self {
        ItemState {
            condition: 1.0,
            cost: 0,
        }
    }
}

/// A physics part (5.4: `mass`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Physics {
    pub mass: f32,
}

impl Physics {
    /// The physics part of an object whose shell is `shell` (#940): its
    /// elements' masses summed.
    pub fn of_shell(shell: &Shell) -> Self {
        Physics {
            mass: shell.elements.iter().map(|e| e.mass_centre().0).sum(),
        }
    }
}

/// An anomalous zone's part (6.1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Anomaly {
    /// 0 is believed active, 4 disabled.
    pub state: u32,
    pub power: f32,
}

/// A plane of a restrictor's box: unit normal and offset (6.2).
pub type Plane = ([f32; 3], f32);

/// A restrictor's shape in level space (6.2): how it is derived from the
/// spawn data is unestablished, so the engine sets it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Restrictor {
    /// The bounding sphere: centre and radius.
    pub bound: ([f32; 3], f32),
    pub spheres: Vec<([f32; 3], f32)>,
    pub boxes: Vec<[Plane; 6]>,
}

/// An engine object as scripts see it (2): made when it spawns (3), its
/// kind parts by its class's kinds (2.2), each with its starting state
/// (the engine sets the rest).
#[derive(Clone, Debug, PartialEq)]
pub struct GameObject {
    pub id: u16,
    pub name: Option<Vec<u8>>,
    pub section: Option<Vec<u8>>,
    /// `clsid()`: the number of its class id (spec C 2.4).
    pub clsid: usize,
    /// The class id it spawned with (spec C 2.4's eight-character name).
    pub class_id: u64,
    pub story_id: i32,
    pub parent: Option<u16>,
    pub class: ObjectClass,
    pub kinds: Kinds,
    pub placement: Mat4,
    pub visual: Option<Visual>,
    pub level_vertex: u32,
    pub game_vertex: u16,
    /// The use flag (6.3): true when created.
    pub usable: bool,
    /// `getEnabled()`, `getVisible()` (10.1).
    pub enabled: bool,
    pub visible: bool,
    /// Marked for destruction (the engine's "destroy" state, 5.2).
    pub destroying: bool,
    /// The server time of its previous scheduled update (spec L 5.2), its
    /// spawning's before the first (our choice).
    pub updated_at: u32,
    /// The simulation part's custom data (3).
    pub custom_data: Vec<u8>,
    pub entity: Option<Entity>,
    pub conditions: Option<Conditions>,
    pub owner: Option<InventoryOwner>,
    pub fighter: Option<Fighter>,
    pub stalker: Option<Stalker>,
    pub item: Option<ItemState>,
    pub physics: Option<Physics>,
    pub anomaly: Option<Anomaly>,
    pub restrictor: Option<Restrictor>,
    /// Script API X's parts: movement, sight, memory, monster state.
    pub movement: movement::Parts,
    /// A `ce_script_zone`'s contacts (6.4).
    pub contacts: Option<Vec<u16>>,
    /// A bloodsucker's invisibility flag (6.5).
    pub invisible: Option<bool>,
    /// A scripted object's script control (script API V 2.1).
    pub scripted: Option<ScriptControl>,
    /// Script API Q's values: team, community, rank, reputation, relation
    /// flags, set as it comes online.
    pub relations: crate::relations::ObjectRelations,
    /// Its handle, made at spawn ([`Scripts::spawn`]).
    handle: Option<EnginePart>,
    /// `spawn_ini()`'s configuration object, made at spawn.
    spawn_ini: Option<EnginePart>,
    /// Its `CGameObject`, for an object spawned through one.
    client: Option<EnginePart>,
    /// Its attached binder (spec L 4).
    binder: Option<EnginePart>,
}

impl GameObject {
    /// An object of `class` spawned from `data` (3), its class id's number
    /// `clsid`. Its level vertex is the spawn data's (the level's range is
    /// the engine's to check).
    pub fn new(class: ObjectClass, data: &SpawnData, clsid: usize) -> Self {
        let kinds = class.kinds();
        let name = if data.name.is_empty() {
            &data.section
        } else {
            &data.name
        };
        let entity = kinds.has(Kinds::ENTITY).then(|| {
            let mut e = Entity::default();
            if let Some(health) = data.health {
                e.health = health;
            }
            e
        });
        GameObject {
            id: data.id,
            name: Some(name.clone()),
            section: Some(data.section.clone()),
            clsid,
            class_id: data.class_id,
            story_id: data.story_id,
            parent: data.parent,
            class,
            kinds,
            placement: data.placement,
            visual: None,
            level_vertex: data.level_vertex,
            game_vertex: data.game_vertex,
            usable: true,
            enabled: true,
            visible: true,
            destroying: false,
            updated_at: 0,
            custom_data: data.custom_data.clone(),
            entity,
            conditions: kinds.has(Kinds::LIVING).then(Conditions::default),
            owner: kinds
                .has(Kinds::INVENTORY_OWNER)
                .then(InventoryOwner::default),
            fighter: kinds.has(Kinds::FIGHTER).then(Fighter::default),
            stalker: kinds.has(Kinds::STALKER).then(Stalker::default),
            item: kinds.has(Kinds::ITEM).then(ItemState::default),
            physics: kinds.has(Kinds::PHYSICAL).then(Physics::default),
            anomaly: kinds.has(Kinds::ANOMALOUS_ZONE).then(Anomaly::default),
            restrictor: kinds.has(Kinds::RESTRICTOR).then(Restrictor::default),
            movement: movement::Parts::of(class, kinds),
            contacts: class.is(SCRIPT_ZONE).then(Vec::new),
            invisible: class.is(BLOODSUCKER).then_some(false),
            scripted: is_scripted(class).then(ScriptControl::new),
            relations: Default::default(),
            handle: None,
            spawn_ini: None,
            client: None,
            binder: None,
        }
    }

    /// Its handle, once spawned.
    pub fn handle(&self) -> Option<EnginePart> {
        self.handle
    }
}

/// The hit event `game_object:hit` sends (5.6).
#[derive(Clone, Debug, PartialEq)]
pub struct HitEvent {
    /// The server time.
    pub time: u32,
    pub target: u16,
    pub power: f32,
    pub direction: [f32; 3],
    pub hitter: u16,
    pub weapon: u16,
    /// The bone in the target's visual; 0 when none was named.
    pub bone: u16,
    /// In the bone's space.
    pub position: [f32; 3],
    pub impulse: f32,
    pub hit_type: i32,
    /// For `fire_wound` (8) only: the armour-piercing value.
    pub armour_piercing: Option<f32>,
    pub aimed_bullet: bool,
}

/// What members ask of the engine, in order ([`GameObjects::take_events`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// An entity's death event (5.2), carrying its killer's id.
    Death {
        object: u16,
        killer: u16,
    },
    Hit(HitEvent),
    /// The player's forced placement (4, `set_actor_position`).
    ForcePlacement {
        object: u16,
        placement: Mat4,
    },
    /// The player's active camera given heading, pitch and roll (4,
    /// `set_actor_direction`).
    CameraAngles {
        object: u16,
        angles: [f32; 3],
    },
    /// The engine's visibility switch (6.5).
    Visibility {
        object: u16,
        hidden: bool,
    },
    /// A stalker's wounded flag changed: its behaviour is updated (5.5).
    Wounded {
        object: u16,
        wounded: bool,
    },
    /// `explosive:explode()` (10.4).
    Explode {
        object: u16,
    },
    /// Script API X 10.4: the game event to the player that hides
    /// (`hide`) or restores its weapon, with its slot argument.
    HideWeapon {
        player: u16,
        hide: bool,
        slot: i32,
    },
}

/// The engine objects scripts see, and what their members ask of the
/// engine.
#[derive(Default)]
pub struct GameObjects {
    objects: BTreeMap<u16, GameObject>,
    /// Handle part -> object id.
    handles: HashMap<EnginePart, u16>,
    events: Vec<Event>,
    /// The server time hit events carry, ms.
    pub server_time: u32,
    /// The game time a death records.
    pub game_time: u64,
    /// Byte 11 of each level vertex's record (4, `level_vertex_light`).
    pub level_vertex_lights: Vec<u8>,
    /// Spawn data awaiting `net_Spawn`, by token.
    pending: HashMap<u32, (SpawnData, Option<u16>)>,
    next_token: u32,
    /// The token of the spawning in progress, by the client object
    /// spawning ([`Scripts::spawn`]).
    spawning: HashMap<EnginePart, u32>,
    /// The engine's state for objects about to spawn, by id.
    prepared: HashMap<u16, EngineState>,
    /// Script API S's inventories, living as long as the objects.
    inventories: Rc<RefCell<crate::inventory::Inventories>>,
}

/// What the engine gives an object as it spawns beyond its spawn data
/// ([`GameObjects::prepare`]): its visual (4), its physics part (5.4),
/// its restrictor's shape (6.2) and its character information's start
/// dialog (docs/behavior/character-talk-state.md 6.2), the last three kept
/// only where its kinds have the part (the start dialog: stalkers and
/// traders, 6.1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EngineState {
    pub visual: Option<Visual>,
    pub physics: Option<Physics>,
    pub restrictor: Option<Restrictor>,
    /// Empty for an object without one.
    pub start_dialog: Vec<u8>,
}

impl GameObjects {
    pub fn new() -> Self {
        Self::default()
    }

    /// The inventories of the objects (script API S).
    pub fn inventories(&self) -> Rc<RefCell<crate::inventory::Inventories>> {
        Rc::clone(&self.inventories)
    }

    pub fn get(&self, id: u16) -> Option<&GameObject> {
        self.objects.get(&id)
    }

    /// The object, for the engine to set its state.
    pub fn get_mut(&mut self, id: u16) -> Option<&mut GameObject> {
        self.objects.get_mut(&id)
    }

    /// Adds `object`, made by the engine outside the spawning (tests):
    /// without a handle, replacing one of its id.
    pub fn insert(&mut self, object: GameObject) {
        self.objects.insert(object.id, object);
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// The objects, by increasing id.
    pub fn iter(&self) -> impl Iterator<Item = &GameObject> {
        self.objects.values()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// The object of handle `part`.
    pub fn of_handle(&self, part: EnginePart) -> Option<u16> {
        self.handles.get(&part).copied()
    }

    /// The handle of object `id` as a Lua value: nil without one.
    pub(crate) fn handle_value(&self, id: Option<u16>) -> Value {
        id.and_then(|id| self.objects.get(&id))
            .and_then(|o| o.handle)
            .map_or(Value::Nil, Value::Object)
    }

    pub(crate) fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Gives object `id`, when it next spawns, the engine's `state`: set as
    /// the object is made, before its binding and binder calls (spec L 5.1
    /// step 1).
    pub fn prepare(&mut self, id: u16, state: EngineState) {
        self.prepared.insert(id, state);
    }

    /// The token of the spawning of client object `client` in progress.
    pub(crate) fn spawning(&self, client: EnginePart) -> Option<u32> {
        self.spawning.get(&client).copied()
    }

    fn pend(&mut self, data: SpawnData) -> u32 {
        self.next_token = self.next_token.wrapping_add(1);
        self.pending.insert(self.next_token, (data, None));
        self.next_token
    }

    /// Spawns the object of `class` from the pending data `token` (10.1,
    /// `net_Spawn`): `false` when there is no such data, or (our choice)
    /// an object of its id exists.
    pub(crate) fn spawn_pending(
        &mut self,
        token: u32,
        class: ObjectClass,
        factory: &RefCell<Option<ObjectFactory>>,
        client: Option<EnginePart>,
    ) -> Option<u16> {
        let (data, spawned) = self.pending.get_mut(&token)?;
        if spawned.is_some() || self.objects.contains_key(&data.id) {
            return None;
        }
        let clsid = factory
            .borrow_mut()
            .get_or_insert_with(ObjectFactory::new)
            .number(data.class_id);
        let mut object = GameObject::new(class, data, clsid);
        object.client = client;
        object.updated_at = self.server_time;
        if let Some(state) = self.prepared.remove(&data.id) {
            object.visual = state.visual;
            if object.physics.is_some() {
                object.physics = state.physics.or(object.physics);
            }
            if object.restrictor.is_some() {
                object.restrictor = state.restrictor.or(object.restrictor);
            }
            if has_start_dialog(object.class)
                && let Some(owner) = object.owner.as_mut()
            {
                owner.talk.come_online(&state.start_dialog);
            }
        }
        let id = object.id;
        *spawned = Some(id);
        self.objects.insert(id, object);
        Some(id)
    }
}

/// What the members share: the engine objects, the services (whose console
/// logs fatal errors and whose device gives the time), the file system
/// (`spawn_ini`'s configurations), the state's fatal error and its object
/// factory.
#[derive(Clone)]
pub(crate) struct Context {
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub fatal: Rc<RefCell<Option<String>>>,
    pub factory: Rc<RefCell<Option<ObjectFactory>>>,
    /// The `hit` objects alive, by part, for the copy constructor.
    pub hits: Rc<RefCell<HashMap<EnginePart, Weak<RefCell<HitData>>>>>,
    /// Script API Y's physics, vehicles and lamps.
    pub physics: Rc<RefCell<physics::PhysicsState>>,
}

impl Context {
    pub fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A fatal error (CL 3.8) standing for a fault or crash the spec
    /// believes, or a failed check: reported, and the state stopped. The
    /// error to raise.
    pub fn fatal(&self, function: &str, line: u32, description: &str, argument: &str) -> String {
        let arguments: &[&str] = if argument.is_empty() {
            &[]
        } else {
            &[argument]
        };
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression: "fatal error",
                description,
                arguments,
            },
        )
    }
}

/// What a state with the game object basics registered keeps for the
/// engine's calls.
pub(crate) struct Registered {
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub files: Rc<RefCell<FileSystem>>,
    physics: Rc<RefCell<physics::PhysicsState>>,
}

/// What the player's use action does after the targeted object's scripted
/// use (6.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseAction {
    /// The object's use flag is false: nothing more.
    ScriptedOnly,
    /// The engine's own handling of the use follows (outside this spec).
    EngineHandling,
}

fn fatal_error(message: String) -> Error {
    Error {
        kind: ErrorKind::Fatal,
        message,
    }
}

impl Scripts {
    /// Registers the game object basics (1) over `objects`, fatal errors
    /// reported to the console of `services`, `spawn_ini` read through
    /// `files`: the engine registers them after the engine services
    /// (`CGameObject` derives from `DLL_Pure`) and before
    /// [`Scripts::load_globals`].
    pub fn register_game_object_basics(
        &mut self,
        objects: Rc<RefCell<GameObjects>>,
        services: Rc<RefCell<EngineServices>>,
        files: Rc<RefCell<FileSystem>>,
    ) -> Result<(), Error> {
        let ctx = Context {
            objects,
            services,
            fatal: self.fatal_flag(),
            factory: Rc::clone(&self.factory),
            hits: Rc::default(),
            physics: Rc::default(),
        };
        ctx.physics.borrow_mut().system = Some(Rc::clone(&files.borrow().system));
        self.register_class(members::game_object_class(&ctx))?;
        for class in engine_classes::classes(&ctx)
            .into_iter()
            .chain(movement::classes(&ctx))
        {
            self.register_class(class)?;
        }
        object_binder::install(self, &ctx)?;
        callbacks::register(self, &ctx)?;
        sight::install(self, &ctx)?;
        self.game_object_state = Some(Registered {
            objects: ctx.objects,
            services: ctx.services,
            files,
            physics: ctx.physics,
        });
        Ok(())
    }

    /// Registers script API Y's classes (1.2 of
    /// docs/behavior/script-api-physics-vehicles-helicopter-and-lamps.md):
    /// `physics_shell`, `physics_element`, `physics_joint`, `holder`,
    /// `CCar`, `CHelicopter` and `hanging_lamp`, after the game object
    /// basics (whose `game_object` has its members of section 2) and before
    /// [`Scripts::load_globals`].
    pub fn register_physics_vehicles_and_lamps(&mut self) -> Result<(), Error> {
        let r = self.registered()?;
        let ctx = Context {
            objects: Rc::clone(&r.objects),
            services: Rc::clone(&r.services),
            fatal: self.fatal_flag(),
            factory: Rc::clone(&self.factory),
            hits: Rc::default(),
            physics: Rc::clone(&r.physics),
        };
        for class in physics::classes(&ctx)
            .into_iter()
            .chain(vehicles::classes(&ctx))
        {
            self.register_class(class)?;
        }
        Ok(())
    }

    /// Script API Y's state, for the engine to add shells, give the world
    /// and take the events.
    pub fn physics_state(&self) -> Result<Rc<RefCell<physics::PhysicsState>>, Error> {
        self.registered().map(|r| Rc::clone(&r.physics))
    }

    fn registered(&self) -> Result<&Registered, Error> {
        self.game_object_state.as_ref().ok_or_else(|| Error {
            kind: ErrorKind::Api,
            message: "the game object basics aren't registered".into(),
        })
    }

    fn game_objects(&self) -> Result<Rc<RefCell<GameObjects>>, Error> {
        self.registered().map(|r| Rc::clone(&r.objects))
    }

    /// The handle of engine object `id` (2.1), for other specs' members to
    /// hand out.
    pub fn game_object(&self, id: u16) -> Option<EnginePart> {
        self.game_object_state
            .as_ref()?
            .objects
            .borrow()
            .get(id)
            .and_then(|o| o.handle)
    }

    /// Spawns the engine object `client` stands for (a `CGameObject` the
    /// object factory created, spec C 6) from `data` (10.2): the engine
    /// calls `net_Spawn`, a script class's override or the engine's own,
    /// and takes its result as success. The engine's own makes the engine
    /// object (3) with its handle and, for non-empty custom data, its
    /// `spawn_ini()` configuration. `data`'s simulation object
    /// ([`SpawnData::server`]) is `net_Spawn`'s argument; the engine's own
    /// spawns from `data` whatever argument it is given.
    ///
    /// Then the binder's part of spawning (spec L 5.1,
    /// [`object_binder`]): the binding and the attached binder's calls,
    /// after `net_Spawn` returns (so an override's code after its base call
    /// runs before them, TODO(#1053)). Its `net_spawn` result is the
    /// spawning's, except for `CPhysicObject`, which always succeeds (L 6).
    /// A spawning that fails after making the object destroys it and logs
    /// `! Failed to spawn entity '<section>'` (L 5.1).
    pub fn spawn(&mut self, client: EnginePart, data: SpawnData) -> Result<bool, OverrideFailure> {
        self.spawn_with(client, data, None)
    }

    /// [`Scripts::spawn`] from spawn data carrying a client-data block
    /// (sav.md 8): after the object reads its own state from the block,
    /// its binder's `load(reader)` reads on (spec L 5.1 step 5). `reader`
    /// is spec I's reader over the block.
    pub fn spawn_saved(
        &mut self,
        client: EnginePart,
        data: SpawnData,
        reader: Value,
    ) -> Result<bool, OverrideFailure> {
        self.spawn_with(client, data, Some(reader))
    }

    fn spawn_with(
        &mut self,
        client: EnginePart,
        data: SpawnData,
        reader: Option<Value>,
    ) -> Result<bool, OverrideFailure> {
        let objects = self.game_objects()?;
        let server = data.server.map_or(Value::Nil, Value::Object);
        let token = {
            let mut o = objects.borrow_mut();
            let token = o.pend(data);
            o.spawning.insert(client, token);
            token
        };
        let result = self.dispatch(
            client,
            "net_Spawn",
            std::slice::from_ref(&server),
            Conversion::Value,
        );
        let spawned = {
            let mut o = objects.borrow_mut();
            o.spawning.remove(&client);
            o.pending.remove(&token).and_then(|(_, id)| id)
        };
        let result = result?;
        let mut ok = !matches!(result, Value::Nil | Value::Boolean(false));
        let Some(id) = spawned else {
            return Ok(ok);
        };
        self.finish_spawn(id)?;
        let bound = self.spawn_binder(id, server, reader, None)?;
        let class = objects.borrow().get(id).map(|o| o.class);
        if class != Some(ObjectClass::Named("CPhysicObject")) {
            // TODO(#1019): gap 1, the simplest reading: the other client
            // classes pass the result on, as `CGameObject` does.
            ok &= bound;
        }
        if !ok {
            let section = objects
                .borrow()
                .get(id)
                .and_then(|o| o.section.clone())
                .unwrap_or_default();
            self.destroy_object(id)?;
            let services = Rc::clone(&self.registered()?.services);
            services.borrow_mut().console.log_mut().message(&format!(
                "! Failed to spawn entity '{}'",
                cp1251::decode(&section)
            ));
        }
        Ok(ok)
    }

    /// Spawns an engine object of `class` that no `CGameObject` stands for
    /// (the engine's own kinds, as the player's object): the engine object
    /// (3) with its handle, which is returned, then the binder's part of
    /// spawning (spec L 5.1), whose result it ignores (TODO(#1019): gap 1,
    /// the simplest reading: these kinds' spawning reaches the general one;
    /// whether a false result fails it is their spec's). The binder's
    /// `net_spawn` gets the spawn data's simulation object.
    pub fn spawn_engine_object(
        &mut self,
        class: ObjectClass,
        data: SpawnData,
    ) -> Result<EnginePart, OverrideFailure> {
        self.spawn_engine_object_with_binding(class, data, None)
    }

    /// [`Scripts::spawn_engine_object`] bound by the function `binding`
    /// names instead of its section's `script_binding`, when given
    /// (TODO(#1120): the fake of the player's binding, until
    /// `bind_stalker.actor_init` runs).
    pub fn spawn_engine_object_with_binding(
        &mut self,
        class: ObjectClass,
        data: SpawnData,
        binding: Option<&[u8]>,
    ) -> Result<EnginePart, OverrideFailure> {
        let objects = self.game_objects()?;
        let server = data.server.map_or(Value::Nil, Value::Object);
        let id = {
            let mut o = objects.borrow_mut();
            let token = o.pend(data);
            let id = o.spawn_pending(token, class, &self.factory, None);
            o.pending.remove(&token);
            id
        };
        let Some(id) = id else {
            return Err(Error {
                kind: ErrorKind::Api,
                message: "an object of that id exists".into(),
            }
            .into());
        };
        let handle = self.finish_spawn(id)?;
        self.spawn_binder(id, server, None, binding)?;
        Ok(handle)
    }

    /// The handle and `spawn_ini()` object of a new engine object. The
    /// handle is made at spawn rather than on first need (2.1): scripts
    /// can't tell. A custom data that doesn't load is a fatal error (LTX
    /// 11).
    fn finish_spawn(&mut self, id: u16) -> Result<EnginePart, Error> {
        let objects = self.game_objects()?;
        let custom = objects.borrow().get(id).map(|o| o.custom_data.clone());
        let Some(custom) = custom else {
            return Err(Error {
                kind: ErrorKind::Api,
                message: format!("no object {id}"),
            });
        };
        // Script API X: what the object reads from its section as it loads
        // (our placement: as it spawns), a missing key fatal (11.3).
        let system = self.physics_state()?.borrow().system.clone();
        let loaded = (objects.borrow_mut().get_mut(id))
            .map_or(Ok(()), |o| movement::load(o, system.as_deref()));
        if let Err(description) = loaded {
            let services = Rc::clone(&self.registered()?.services);
            let message = report_fatal(
                &services,
                &self.fatal_flag(),
                Fatal {
                    function: "CInifile::r_float",
                    file: file!(),
                    line: line!(),
                    expression: "fatal error",
                    description: &description,
                    arguments: &[],
                },
            );
            return Err(fatal_error(message));
        }
        // Script API Q 2.1: what it copies from its simulation object and
        // reads from its section as it comes online.
        self.relations_come_online(id)?;
        let ini = if custom.is_empty() {
            None
        } else {
            let (files, services) = {
                let r = self.registered()?;
                (Rc::clone(&r.files), Rc::clone(&r.services))
            };
            let loaded = config_from_text(&files.borrow(), &custom);
            match loaded {
                Ok(config) => Some(
                    self.lua_mut()
                        .new_engine_object("ini_file", |_| IniFile(Rc::new(config)))?,
                ),
                Err(e) => {
                    let message = report_fatal(
                        &services,
                        &self.fatal_flag(),
                        Fatal {
                            function: "CInifile::Load",
                            file: file!(),
                            line: line!(),
                            expression: "fatal error",
                            description: "<no expression>",
                            arguments: &[&e.to_string()],
                        },
                    );
                    return Err(fatal_error(message));
                }
            }
        };
        let handle = self
            .lua_mut()
            .new_engine_object("game_object", |part| members::Handle { id, part })?;
        {
            let mut o = objects.borrow_mut();
            o.handles.insert(handle, id);
            if let Some(object) = o.get_mut(id) {
                object.handle = Some(handle);
                object.spawn_ini = ini;
            }
        }
        // Script API W 10: a stalker's motivation manager, before its
        // binder's calls.
        self.motivation_manager_comes_online(id, handle)?;
        Ok(handle)
    }

    /// Destroys engine object `id` (it goes offline or is removed, 2.1):
    /// its attached binder gets `net_destroy()` and is deleted (spec L
    /// 5.6), its queued sounds are stopped and its sound player goes
    /// (CH-25 1.3), then its handle and its `spawn_ini()` object are
    /// released, so
    /// members called on a handle scripts still hold fail (our choice: the
    /// spec has it unestablished), and its `CGameObject` no longer stands
    /// for it. `false` if there is no such object.
    pub fn destroy_object(&mut self, id: u16) -> Result<bool, OverrideFailure> {
        let objects = self.game_objects()?;
        if objects.borrow().get(id).is_none() {
            return Ok(false);
        }
        self.destroy_binder(id)?;
        let Some(object) = objects.borrow_mut().objects.remove(&id) else {
            return Ok(false);
        };
        self.forget_callbacks(id, object.kinds)?;
        self.forget_inventory_object(id);
        self.forget_sound_player(id);
        self.motivation_manager_goes_offline(id)?;
        if let Some(handle) = object.handle {
            objects.borrow_mut().handles.remove(&handle);
            self.lua_mut().release_engine_object(handle)?;
        }
        if let Some(ini) = object.spawn_ini {
            // TODO(#1001): gap 2, the simplest reading: the configuration
            // object is the engine object's alone, released only here.
            self.lua_mut().release_engine_object(ini)?;
        }
        if let Some(client) = object.client {
            self.with_part(client, |c: &mut ClientObject| c.object = None);
        }
        let physics = self.physics_state()?;
        let mut p = physics.borrow_mut();
        p.shells.remove(&id);
        p.cars.remove(&id);
        p.helicopters.remove(&id);
        p.exploding.remove(&id);
        Ok(true)
    }

    /// The engine's export or import call on object `id` (10.2,
    /// `net_Export`, `net_Import`): a script class's override or the
    /// engine's own, which does nothing. `args` are the packet (spec I's).
    pub fn net_call(
        &mut self,
        id: u16,
        method: NetCall,
        args: &[Value],
    ) -> Result<(), OverrideFailure> {
        let client = self.game_objects()?.borrow().get(id).and_then(|o| o.client);
        if let Some(client) = client {
            let name = match method {
                NetCall::Export => "net_Export",
                NetCall::Import => "net_Import",
            };
            self.dispatch(client, name, args, Conversion::Nothing)?;
        }
        Ok(())
    }

    /// Object `id`'s scripted use by `user` (10.1, 10.2): a script class's
    /// `use` override or the engine's own, which calls the object's
    /// `use_object` callback (spec O, [`callbacks::ObjectEvent::UseObject`])
    /// and returns true.
    pub fn use_object(&mut self, id: u16, user: Option<u16>) -> Result<bool, OverrideFailure> {
        let objects = self.game_objects()?;
        let (client, who) = {
            let o = objects.borrow();
            let Some(object) = o.get(id) else {
                return Err(Error {
                    kind: ErrorKind::Api,
                    message: format!("no object {id}"),
                }
                .into());
            };
            (object.client, o.handle_value(user))
        };
        match client {
            Some(client) => {
                let result = self.dispatch(client, "use", &[who], Conversion::Value)?;
                Ok(!matches!(result, Value::Nil | Value::Boolean(false)))
            }
            None => {
                self.raise(id, &callbacks::ObjectEvent::UseObject { user })?;
                Ok(true)
            }
        }
    }

    /// The player `player`'s use action on `target` (6.3): its scripted
    /// use first, then, if its use flag is false, nothing more. The further
    /// object tested first (disputed in review) and the player in a vehicle
    /// (TODO(#1001): gap 1, the simplest reading: the caller's, before
    /// this) are not covered.
    pub fn player_use(&mut self, target: u16, player: u16) -> Result<UseAction, OverrideFailure> {
        self.use_object(target, Some(player))?;
        let usable = self
            .game_objects()?
            .borrow()
            .get(target)
            .is_some_and(|o| o.usable);
        Ok(if usable {
            UseAction::EngineHandling
        } else {
            UseAction::ScriptedOnly
        })
    }
}

/// The engine's network calls of 10.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetCall {
    Export,
    Import,
}
