// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The offline simulation (docs/behavior/offline-simulation.md): the
//! registry of every object with its location (section 5), the current
//! level and its list (6.1), the switch pass (6.2–6.4), the schedule and
//! offline travel on the game clock (section 7), and creation and
//! registration at a New Game and on load (section 8).
//!
//! Each object keeps its whole record ([`ObjectRecord`], docs/formats/sav.md
//! section 7), which the simulation changes in place (location, position,
//! smart terrain, client data), so a save writes it as it stands. What is
//! not saved (online state, travel state, the times of the last
//! smart-terrain look) is kept beside it.
//!
//! The smart terrains' scripts (acceptance, suitability, joining, tasks,
//! 7.2, 7.3) are behind [`SmartTerrains`]; without scripts no object joins
//! one, so nothing travels.

pub mod graph;
pub mod navigation;
mod travel;

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::ops::{ControlFlow, Range};

use crate::config::{Config, value};
use crate::switching::{Candidate, StartState, SwitchDistances, distance, switched};
use graph::{Graph, terrain_masks};
use navigation::Navigation;
pub use travel::{Target, Travel};
use volya_engine_formats::game_graph::TerrainMask;
use volya_engine_formats::ltx;
use volya_engine_formats::object::{FLAG_PLAYER, Kind, ObjectRecord};
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_formats::spawn;

/// No object (ids, parent ids, smart-terrain ids).
pub const NO_ID: u16 = 0xFFFF;
/// The actor's id at a New Game (8.1).
pub const ACTOR_ID: u16 = 0;
/// State flags bit: some kinds use the level graph (5.1).
const STATE_LEVEL_GRAPH: u32 = 0x80;
/// State flags bits a stalker's or monster's scheduled update needs (7.1).
const STATE_SCHEDULED: u32 = 0x08 | 0x10 | 0x20;
/// Health above which a stalker or monster is scheduled (7.1).
const SCHEDULE_HEALTH: f32 = 0.001;
/// Class ids whose scripts keep client data (sav.md 8).
const KEEPS_CLIENT_DATA: [&str; 4] = ["SPC_RS_S", "O_PHYS_S", "AI_TRD_S", "C_HLCP_S"];
/// The smart terrains' class id (7.2).
const SMART_TERRAIN: &str = "SMRTTRRN";
/// Game milliseconds per hour.
const HOUR_MS: u64 = 3_600_000;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The simulation's configuration: `[alife]` of `config\alife.ltx` and
/// `[monsters_common]` of the system configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlifeConfig {
    pub distances: SwitchDistances,
    /// Scheduled updates per simulation update (7.1).
    pub objects_per_update: usize,
    /// `stay_after_death_time_interval`, game ms (6.4).
    pub stay_after_death: u64,
}

impl AlifeConfig {
    /// Reads the keys; a missing `objects_per_update` or
    /// `stay_after_death_time_interval` takes the shipped value (our
    /// choice: 10 and 36 hours).
    pub fn read(alife: &Config, system: &Config) -> io::Result<Self> {
        Ok(Self {
            distances: SwitchDistances::read(alife)?,
            objects_per_update: alife
                .get("alife", "objects_per_update")
                .flatten()
                .map_or(10, |v| value::int(v).max(0) as usize),
            stay_after_death: (system
                .get("monsters_common", "stay_after_death_time_interval")
                .flatten()
                .map_or(36.0, value::float)
                .max(0.0) as f64
                * HOUR_MS as f64) as u64,
        })
    }
}

/// The smart terrains' scripts (7.2, 7.3; `scripts\smart_terrain.script`).
pub trait SmartTerrains {
    /// Whether smart terrain `terrain` accepts object `object`, and if so
    /// how suitable it is.
    fn suitability(&mut self, terrain: u16, object: u16) -> Option<f32>;
    /// Object `object` joins smart terrain `terrain`.
    fn join(&mut self, terrain: u16, object: u16);
    /// The patrol path smart terrain `terrain` gives object `object` as its
    /// task.
    fn task(&mut self, terrain: u16, object: u16) -> Option<String>;
}

/// No scripts: no smart terrain accepts anyone.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoScripts;

impl SmartTerrains for NoScripts {
    fn suitability(&mut self, _: u16, _: u16) -> Option<f32> {
        None
    }

    fn join(&mut self, _: u16, _: u16) {}

    fn task(&mut self, _: u16, _: u16) -> Option<String> {
        None
    }
}

/// An object of the simulation.
#[derive(Clone, Debug)]
pub struct SimObject {
    pub id: u16,
    /// Its object index in the spawn file, when a New Game created it.
    pub spawn_index: Option<u32>,
    /// Its state as a save writes it.
    pub record: ObjectRecord,
    /// Online state; not saved (6.1).
    pub online: bool,
    /// Uses the level graph (5.1).
    pub uses_level_graph: bool,
    /// The switch predicates' inputs that do not change.
    candidate: Candidate,
    /// Stalkers and monsters: travel (section 7).
    pub travel: Option<Box<Travel>>,
}

impl SimObject {
    fn kind(&self) -> Kind {
        self.record.kind()
    }

    fn parent(&self) -> u16 {
        self.record.header.parent_id
    }

    fn game_vertex(&self) -> u16 {
        self.record.object.object_block().game_graph_vertex
    }

    fn health(&self) -> Option<f32> {
        self.record.object.creature().map(|c| c.health)
    }

    fn direct_control(&self) -> bool {
        self.record.object.object_block().direct_control != 0
    }

    /// A stalker or monster (the kinds with offline travel, section 7).
    fn travels(&self) -> bool {
        matches!(self.kind(), Kind::Stalker | Kind::Monster)
    }

    /// 7.1: direct control, offline, health above 0.001.
    fn belongs_in_schedule(&self) -> bool {
        self.travels()
            && self.direct_control()
            && !self.online
            && self.health().is_some_and(|h| h > SCHEDULE_HEALTH)
    }

    fn keeps_client_data(&self) -> bool {
        KEEPS_CLIENT_DATA.contains(&self.record.class_id.as_str())
    }

    /// Whether *on* can hold for it: the engine's predicate
    /// (level-objects.md 2.3: the renderer match and state flags bit 0x02),
    /// or the electra artefacts' scripted rule, which does not test the
    /// engine's (2.5: true at night 50 m away or more).
    pub fn may_switch_online(&self) -> bool {
        self.candidate.is_electra()
            || self
                .candidate
                .renderer_match()
                .is_ok_and(|m| self.candidate.engine_on(m))
    }

    /// The predicates' inputs as they stand.
    fn candidate(&mut self) -> &Candidate {
        let c = &mut self.candidate;
        c.position = self.record.header.position;
        c.has_parent = self.record.header.parent_id != NO_ID;
        c.health = self.record.object.creature().map(|c| c.health);
        &self.candidate
    }
}

/// Whether `kind` is an item (has the Item block).
fn is_item(kind: Kind) -> bool {
    use Kind::*;
    matches!(
        kind,
        Item | Torch
            | Pda
            | Document
            | Outfit
            | Ammunition
            | Knife
            | MagazineWeapon
            | WeaponWithGrenadeLauncher
            | Shotgun
    )
}

/// Whether the object of `record` uses the level graph (5.1).
pub fn uses_level_graph(record: &ObjectRecord) -> bool {
    use Kind::*;
    let class = record.class_id.as_str();
    let flagged = record.object.object_block().state_flags & STATE_LEVEL_GRAPH != 0;
    match record.kind() {
        Player | Stalker | Monster | MonsterGroup | SmartZone => true,
        CrowOrPhantom | Vehicle | Helicopter | Zone | Anomaly | VisualAnomaly | TorridZone
        | LevelChanger | Restrictor | TeamBase | HangingLamp | PhysicsObject | Breakable
        | Climbable | SkeletonObject => false,
        Item if class == "II_BOLT" => false,
        PlainVisual if class == "O_SEARCH" => false,
        Trader
        | Item
        | Torch
        | Pda
        | Document
        | Outfit
        | Ammunition
        | Knife
        | MagazineWeapon
        | WeaponWithGrenadeLauncher
        | Shotgun
        | PlainVisual
        | StationaryGun
        | OnOffGroup => flagged,
    }
}

/// Game milliseconds of an `hours:minutes:seconds` value; `None` when
/// malformed.
fn interval_ms(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    let [h, m, s] = parts.as_slice() else {
        return None;
    };
    let n = |t: &str| t.trim().parse::<u64>().ok();
    Some(((n(h)? * 60 + n(m)?) * 60 + n(s)?) * 1000)
}

/// What changed in one update.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    /// Objects that switched online, in that order.
    pub online: Vec<u16>,
    /// Objects that switched offline.
    pub offline: Vec<u16>,
    /// Objects removed (6.4).
    pub removed: Vec<u16>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.online.is_empty() && self.offline.is_empty() && self.removed.is_empty()
    }
}

/// The objects by id: a dense list indexed by id (ids are handed out from
/// 1 upwards, 8.1), `None` where no object has that id.
#[derive(Clone, Debug, Default)]
struct Registry {
    slots: Vec<Option<SimObject>>,
    len: usize,
}

impl Registry {
    fn get(&self, id: &u16) -> Option<&SimObject> {
        self.slots.get(usize::from(*id))?.as_ref()
    }

    fn get_mut(&mut self, id: &u16) -> Option<&mut SimObject> {
        self.slots.get_mut(usize::from(*id))?.as_mut()
    }

    fn contains_key(&self, id: &u16) -> bool {
        self.get(id).is_some()
    }

    fn insert(&mut self, id: u16, object: SimObject) {
        let i = usize::from(id);
        if self.slots.len() <= i {
            self.slots.resize_with(i + 1, || None);
        }
        if self.slots[i].replace(object).is_none() {
            self.len += 1;
        }
    }

    fn remove(&mut self, id: &u16) -> Option<SimObject> {
        let o = self.slots.get_mut(usize::from(*id))?.take();
        self.len -= usize::from(o.is_some());
        o
    }

    fn keys(&self) -> impl Iterator<Item = &u16> + '_ {
        self.values().map(|o| &o.id)
    }

    fn values(&self) -> impl Iterator<Item = &SimObject> + '_ {
        self.slots.iter().flatten()
    }

    fn values_mut(&mut self) -> impl Iterator<Item = &mut SimObject> + '_ {
        self.slots.iter_mut().flatten()
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl std::ops::Index<&u16> for Registry {
    type Output = SimObject;

    fn index(&self, id: &u16) -> &SimObject {
        self.get(id).expect("no object with this id")
    }
}

/// The current level (6.1).
#[derive(Clone, Debug)]
struct CurrentLevel {
    id: u8,
    navigation: Option<Navigation>,
}

/// The offline simulation.
pub struct Simulation {
    pub graph: Graph,
    pub paths: PatrolPaths,
    pub config: AlifeConfig,
    objects: Registry,
    level: Option<CurrentLevel>,
    /// The current level's list, in increasing id (6.1).
    level_list: BTreeSet<u16>,
    /// Scheduled stalkers and monsters, in increasing id (7.1).
    schedule: BTreeSet<u16>,
    /// The last object the switch pass visited.
    switch_cursor: Option<u16>,
    /// The last scheduled object served.
    schedule_cursor: Option<u16>,
    /// Story id → object id.
    story_ids: BTreeMap<i32, u16>,
    /// The smart terrains' ids (7.2).
    smart_terrains: BTreeSet<u16>,
    /// Ids the actor's supplies took (8.1), held by its character state.
    actor_items: Range<u16>,
    /// The next id the pool hands out.
    next_id: u16,
    scripts: Box<dyn SmartTerrains>,
}

impl std::fmt::Debug for Simulation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Simulation")
            .field("objects", &self.objects.len())
            .field("level_list", &self.level_list.len())
            .field("schedule", &self.schedule.len())
            .finish()
    }
}

/// The class id of section `section` (its `class` key).
pub fn class_of<'a>(system: &'a Config, section: &str) -> io::Result<&'a str> {
    system
        .get(section, "class")
        .flatten()
        .map(str::trim)
        .ok_or_else(|| invalid(format!("section [{section}] has no class")))
}

impl Simulation {
    /// An empty simulation over `graph` and `paths`.
    pub fn new(graph: Graph, paths: PatrolPaths, config: AlifeConfig) -> Self {
        Self {
            graph,
            paths,
            config,
            objects: Registry::default(),
            level: None,
            level_list: BTreeSet::new(),
            schedule: BTreeSet::new(),
            switch_cursor: None,
            schedule_cursor: None,
            story_ids: BTreeMap::new(),
            smart_terrains: BTreeSet::new(),
            actor_items: 0..0,
            next_id: 1,
            scripts: Box::new(NoScripts),
        }
    }

    /// Sets the smart terrains' scripts.
    pub fn set_scripts(&mut self, scripts: Box<dyn SmartTerrains>) {
        self.scripts = scripts;
    }

    /// A New Game (8.1) from spawn file `spawn_file`, class ids from
    /// `system`: every record created in increasing object index, the
    /// actor with id 0, the others with the next id upwards from 1. The
    /// actor's supplies are created by its character state: `supplies` is
    /// called with the actor's record and the first id they take, and says
    /// how many ids they took. Other objects' supplies are not created
    /// (their rules are outside the spec). Then the post-registration step
    /// runs in increasing id.
    pub fn new_game(
        &mut self,
        spawn_file: &[u8],
        system: &Config,
        supplies: &mut dyn FnMut(&ObjectRecord, u16) -> io::Result<u16>,
    ) -> io::Result<()> {
        self.new_game_in(&mut io::Cursor::new(spawn_file), system, supplies)
    }

    /// As [`Self::new_game`], reading the spawn file from `spawn_file` an
    /// object at a time.
    pub fn new_game_in(
        &mut self,
        spawn_file: &mut (impl io::Read + io::Seek),
        system: &Config,
        supplies: &mut dyn FnMut(&ObjectRecord, u16) -> io::Result<u16>,
    ) -> io::Result<()> {
        let mut next = self.next_id;
        spawn::for_each_record_in(spawn_file, |r| {
            let section = volya_engine_formats::object::section(r.spawn)?;
            let class = class_of(system, &section)?;
            let mut record = ObjectRecord::parse(r.spawn, r.update, class, true)?;
            let actor = is_actor(&record);
            let id = if actor { ACTOR_ID } else { take(&mut next, 1)? };
            record.header.id = id;
            self.register(record, Some(r.index), system)?;
            if actor {
                let record = &self.objects[&id].record;
                let n = supplies(record, next)?;
                let first = take(&mut next, n)?;
                self.actor_items = first..first + n;
            }
            Ok(ControlFlow::Continue(()))
        })?;
        self.next_id = next;
        self.post_register();
        Ok(())
    }

    /// An id from the pool for an object created after the New Game's
    /// records (the bolt the online player receives, player-state.md 3.2;
    /// held by its owner's state, not registered here).
    pub fn take_id(&mut self) -> io::Result<u16> {
        take(&mut self.next_id, 1)
    }

    /// A load (8.2): `records` with their saved ids registered in
    /// increasing id, then the post-registration step.
    pub fn load(&mut self, mut records: Vec<ObjectRecord>, system: &Config) -> io::Result<()> {
        records.sort_by_key(|r| r.header.id);
        for r in records {
            if self.objects.contains_key(&r.header.id) {
                return Err(invalid(format!("object id {} saved twice", r.header.id)));
            }
            // Fresh ids come after every saved one (our choice: any unused
            // id serves).
            self.next_id = self.next_id.max(r.header.id.saturating_add(1));
            self.register(r, None, system)?;
        }
        self.post_register();
        Ok(())
    }

    /// Registers `record` under its id (8.1 step 3.3).
    fn register(
        &mut self,
        record: ObjectRecord,
        spawn_index: Option<u32>,
        system: &Config,
    ) -> io::Result<()> {
        let id = record.header.id;
        let actor = is_actor(&record);
        let travel = if matches!(record.kind(), Kind::Stalker | Kind::Monster) {
            Some(Box::new(Travel::new(
                &record,
                travel_config(&record, system)?,
            )))
        } else {
            None
        };
        let object = SimObject {
            id,
            spawn_index,
            uses_level_graph: uses_level_graph(&record),
            candidate: Candidate::of(&record),
            online: false,
            travel,
            record,
        };
        let block = object.record.object.object_block();
        if block.story_id != -1 {
            self.story_ids.insert(block.story_id, id);
        }
        if object.record.class_id.as_str() == SMART_TERRAIN {
            self.smart_terrains.insert(id);
        }
        let scheduled = object.belongs_in_schedule();
        self.objects.insert(id, object);
        if scheduled {
            self.schedule.insert(id);
        }
        if actor {
            self.fix_level(id)?;
        } else {
            self.enter_level_list(id);
        }
        Ok(())
    }

    /// The actor's registration fixes the current level (6.1): the level
    /// of its game vertex; the objects registered before it are entered in
    /// its list.
    fn fix_level(&mut self, actor: u16) -> io::Result<()> {
        let vertex = self.objects[&actor].game_vertex();
        let level = self.graph.graph.vertex_level(vertex)?.id;
        self.level = Some(CurrentLevel {
            id: level,
            navigation: None,
        });
        let ids: Vec<u16> = self.objects.keys().copied().collect();
        for id in ids {
            self.enter_level_list(id);
        }
        Ok(())
    }

    /// Enters `id` in the current level's list if it belongs there (6.1),
    /// takes it out otherwise.
    fn enter_level_list(&mut self, id: u16) {
        let Some(level) = &self.level else { return };
        let Some(o) = self.objects.get(&id) else {
            return;
        };
        let on_level = self
            .graph
            .graph
            .vertex(o.game_vertex())
            .is_some_and(|v| v.level_id == level.id);
        let held = is_item(o.kind()) && o.parent() != NO_ID;
        if o.direct_control() && !held && on_level {
            self.level_list.insert(id);
        } else {
            self.level_list.remove(&id);
        }
    }

    /// The post-registration step (8.1 step 4, sav.md 11.2 step 7): the
    /// client-data rule of sav.md 8 (the scripts' `on_register` does not
    /// run here).
    fn post_register(&mut self) {
        // Every object is registered: the list's spare room from growing
        // (up to half of it, 5.5 MB for a New Game) goes (#370).
        self.objects.slots.shrink_to_fit();
        let ids: Vec<u16> = self.objects.keys().copied().collect();
        for id in ids {
            let top = self.top_level(id);
            let on_level = self
                .objects
                .get(&top)
                .is_some_and(|o| self.on_current_level(o.game_vertex()));
            if let Some(o) = self.objects.get_mut(&id)
                && !on_level
                && !o.keeps_client_data()
            {
                o.record.header.client_data.clear();
            }
        }
    }

    /// The outermost object reached from `id` through parent ids.
    fn top_level(&self, mut id: u16) -> u16 {
        for _ in 0..self.objects.len() {
            match self.objects.get(&id).map(SimObject::parent) {
                Some(p) if p != NO_ID && self.objects.contains_key(&p) => id = p,
                _ => break,
            }
        }
        id
    }

    /// The current level's id, once the actor is registered.
    pub fn current_level(&self) -> Option<u8> {
        self.level.as_ref().map(|l| l.id)
    }

    /// Sets the current level's navigation data (loaded by the caller with
    /// the checks of 1.8).
    pub fn set_navigation(&mut self, navigation: Navigation) -> io::Result<()> {
        let level = self
            .level
            .as_mut()
            .ok_or_else(|| invalid("no current level"))?;
        if navigation.level_id != level.id {
            return Err(invalid("navigation data of another level"));
        }
        level.navigation = Some(navigation);
        Ok(())
    }

    pub fn navigation(&self) -> Option<&Navigation> {
        self.level.as_ref()?.navigation.as_ref()
    }

    /// The object with id `id`.
    pub fn object(&self, id: u16) -> Option<&SimObject> {
        self.objects.get(&id)
    }

    /// The object with id `id`, to change (tests, scripts).
    pub fn object_mut(&mut self, id: u16) -> Option<&mut SimObject> {
        self.objects.get_mut(&id)
    }

    /// Every object in increasing id.
    pub fn objects(&self) -> impl Iterator<Item = &SimObject> {
        self.objects.values()
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.len() == 0
    }

    /// The current level's list in increasing id.
    pub fn level_list(&self) -> impl Iterator<Item = u16> + '_ {
        self.level_list.iter().copied()
    }

    /// The scheduled objects in increasing id.
    pub fn schedule(&self) -> impl Iterator<Item = u16> + '_ {
        self.schedule.iter().copied()
    }

    /// The object with story id `story_id`.
    pub fn by_story_id(&self, story_id: i32) -> Option<u16> {
        self.story_ids.get(&story_id).copied()
    }

    /// The ids the actor's supplies took.
    pub fn actor_items(&self) -> Range<u16> {
        self.actor_items.clone()
    }

    /// The object with name `name` (tests, debugging).
    pub fn by_name(&self, name: &str) -> Option<&SimObject> {
        self.objects.values().find(|o| o.record.header.name == name)
    }

    /// Places the actor at `position` (the player's feet).
    pub fn set_actor_position(&mut self, position: [f32; 3]) {
        if let Some(a) = self.objects.get_mut(&ACTOR_ID) {
            a.record.header.position = position;
        }
    }

    /// The actor's position.
    fn actor_position(&self) -> [f32; 3] {
        self.objects
            .get(&ACTOR_ID)
            .map_or([0.0; 3], |a| a.record.header.position)
    }

    /// One simulation update at game time `now` (6.2): the switch pass,
    /// then the scheduled updates. `normal_time_factor` is the game-time
    /// chunk's second factor (7.5), `hour` the game hour (the scripts'
    /// switch rules, level-objects.md 2.5).
    pub fn update(&mut self, now: u64, normal_time_factor: f32, hour: u32) -> io::Result<Changes> {
        let mut changes = Changes::default();
        self.switch_pass(now, hour, &mut changes)?;
        self.scheduled_updates(now, normal_time_factor);
        Ok(changes)
    }

    /// The switch pass (6.2): every object of the current level's list
    /// once, in increasing id from after the one the previous pass stopped
    /// at, wrapping around. The pass's time limit (about 900 s with the
    /// shipped values) never ends it early here (our choice).
    pub fn switch_pass(&mut self, now: u64, hour: u32, changes: &mut Changes) -> io::Result<()> {
        let start = self.switch_cursor;
        let order: Vec<u16> = match start {
            Some(c) => self
                .level_list
                .range(c.saturating_add(1)..)
                .chain(self.level_list.range(..=c))
                .copied()
                .collect(),
            None => self.level_list.iter().copied().collect(),
        };
        let state = StartState {
            actor: self.actor_position(),
            hour,
        };
        for id in order {
            if !self.objects.contains_key(&id) {
                continue;
            }
            self.switch_cursor = Some(id);
            // Step 1.
            if self.removable(id, now) {
                self.remove(id, changes);
                continue;
            }
            // Step 2.
            let o = &self.objects[&id];
            if o.parent() == NO_ID && o.uses_level_graph {
                self.update_location(id);
            }
            // Step 3.
            let o = self.objects.get_mut(&id).expect("visited object");
            if o.parent() == NO_ID {
                let online = o.online;
                let node = self
                    .level
                    .as_ref()
                    .and_then(|l| l.navigation.as_ref())
                    .and_then(|n| {
                        n.ai.position(o.record.object.object_block().level_graph_node)
                    })
                    .unwrap_or(o.record.header.position);
                let d = distance(o.record.header.position, state.actor);
                if !online {
                    let scheduled = o.belongs_in_schedule();
                    if scheduled {
                        self.schedule.insert(id);
                    } else {
                        self.schedule.remove(&id);
                    }
                }
                let o = self.objects.get_mut(&id).expect("visited object");
                let predicates = o.candidate().predicates_in(&state, online, node)?;
                let now_online = switched(online, predicates, d, &self.config.distances);
                if !online && now_online {
                    self.switch_online(id, changes);
                } else if online && !now_online {
                    self.switch_offline(id, changes);
                } else if !online && !o.keeps_client_data() {
                    o.record.header.client_data.clear();
                }
            }
            // Step 4.
            if self.removable(id, now) {
                self.remove(id, changes);
            }
        }
        Ok(())
    }

    /// 6.4: a stalker or monster with health ≤ 0, offline, story id −1, a
    /// non-zero death time, and the game time at death time +
    /// `stay_after_death_time_interval` or later.
    fn removable(&self, id: u16, now: u64) -> bool {
        let Some(o) = self.objects.get(&id) else {
            return false;
        };
        let Some(c) = o.record.object.creature() else {
            return false;
        };
        o.travels()
            && c.health <= 0.0
            && !o.online
            && o.record.object.object_block().story_id == -1
            && c.death_time != 0
            && now >= c.death_time.saturating_add(self.config.stay_after_death)
    }

    /// Removes `id` and the objects it carries (6.2 step 1).
    /// Iterative, each object once, so a cycle of parent ids ends.
    fn remove(&mut self, id: u16, changes: &mut Changes) {
        let mut doomed = vec![id];
        let mut i = 0;
        while i < doomed.len() {
            let parent = doomed[i];
            for o in self.objects.values() {
                if o.parent() == parent && !doomed.contains(&o.id) {
                    doomed.push(o.id);
                }
            }
            i += 1;
        }
        // The carried first, deepest last found first.
        for &id in doomed.iter().rev() {
            self.remove_one(id, changes);
        }
    }

    fn remove_one(&mut self, id: u16, changes: &mut Changes) {
        if let Some(o) = self.objects.remove(&id) {
            let block = o.record.object.object_block();
            if block.story_id != -1 {
                self.story_ids.remove(&block.story_id);
            }
            self.smart_terrains.remove(&id);
            self.level_list.remove(&id);
            self.schedule.remove(&id);
            changes.removed.push(id);
        }
    }

    /// Switching online (6.3): with the items it carries, for objects with
    /// the Trader-like block; a stalker or monster leaves the schedule and
    /// its travel path is discarded.
    fn switch_online(&mut self, id: u16, changes: &mut Changes) {
        let Some(o) = self.objects.get_mut(&id) else {
            return;
        };
        o.online = true;
        let trader_like = matches!(o.kind(), Kind::Player | Kind::Stalker | Kind::Trader);
        if let Some(t) = &mut o.travel {
            t.discard_path();
        }
        self.schedule.remove(&id);
        changes.online.push(id);
        if trader_like {
            for c in self.objects.values_mut().filter(|c| c.parent() == id) {
                if !c.online {
                    c.online = true;
                    changes.online.push(c.id);
                }
            }
        }
    }

    /// Switching offline (6.3): the client data discarded unless its kind
    /// keeps it, a stalker or monster scheduled again if 7.1 says so and its
    /// travel path discarded. The items a trader-like object carries go
    /// offline with it (our choice, as they came online with it).
    fn switch_offline(&mut self, id: u16, changes: &mut Changes) {
        let Some(o) = self.objects.get_mut(&id) else {
            return;
        };
        o.online = false;
        if !o.keeps_client_data() {
            o.record.header.client_data.clear();
        }
        if let Some(t) = &mut o.travel {
            t.discard_path();
        }
        let trader_like = matches!(o.kind(), Kind::Player | Kind::Stalker | Kind::Trader);
        if o.belongs_in_schedule() {
            self.schedule.insert(id);
        }
        changes.offline.push(id);
        if trader_like {
            for c in self.objects.values_mut().filter(|c| c.parent() == id) {
                if c.online {
                    c.online = false;
                    if !c.keeps_client_data() {
                        c.record.header.client_data.clear();
                    }
                    changes.offline.push(c.id);
                }
            }
        }
    }

    /// Brings `id`'s location up to date (5.2).
    fn update_location(&mut self, id: u16) {
        let Some(nav) = self.level.as_ref().and_then(|l| l.navigation.as_ref()) else {
            return;
        };
        let o = &self.objects[&id];
        let block = o.record.object.object_block();
        let (online, lv, gv) = (o.online, block.level_graph_node, block.game_graph_vertex);
        let position = o.record.header.position;
        if !online && !nav.valid(lv) {
            return;
        }
        if nav.ai.packed_at(position).is_none() {
            return;
        }
        if nav.valid(lv) && nav.ai.in_cell(lv, position) {
            return;
        }
        let Some(k) = nav.level_vertex_at(position) else {
            return;
        };
        let Some((g, d)) = nav.cross.cell(k) else {
            return;
        };
        let k_contains_old = nav.ai.in_cell(k, position);
        if online || g == gv {
            let o = self.objects.get_mut(&id).expect("object");
            let block = o.record.object.object_block_mut();
            block.level_graph_node = k;
            block.distance = d;
            if online {
                block.game_graph_vertex = g;
            }
            return;
        }
        self.arrive(id, g);
        let own = self.graph.graph.vertex(g).map(|v| v.level_vertex);
        let d = self
            .level
            .as_ref()
            .and_then(|l| l.navigation.as_ref())
            .zip(own)
            .and_then(|(n, own)| n.cross.cell(own))
            .map(|c| c.1);
        let o = self.objects.get_mut(&id).expect("object");
        if k_contains_old {
            o.record.header.position = position;
        }
        if let Some(d) = d {
            o.record.object.object_block_mut().distance = d;
        }
    }

    /// An offline object arrives at game vertex `vertex` (5.3): its game
    /// vertex, its position the vertex's level point, its level vertex the
    /// vertex's own; the distance unchanged; the current level's list
    /// updated.
    fn arrive(&mut self, id: u16, vertex: u16) {
        let Some(v) = self.graph.graph.vertex(vertex).copied() else {
            return;
        };
        if let Some(o) = self.objects.get_mut(&id) {
            o.record.header.position = v.level_point;
            let block = o.record.object.object_block_mut();
            block.game_graph_vertex = vertex;
            block.level_graph_node = v.level_vertex;
        }
        self.enter_level_list(id);
    }

    /// The scheduled updates of one simulation update (7.1): at most
    /// `objects_per_update` scheduled objects in increasing id from after
    /// the last one served, wrapping around, each once.
    fn scheduled_updates(&mut self, now: u64, normal_time_factor: f32) {
        let n = self.config.objects_per_update;
        let order: Vec<u16> = match self.schedule_cursor {
            Some(c) => self
                .schedule
                .range(c.saturating_add(1)..)
                .chain(self.schedule.range(..=c))
                .take(n)
                .copied()
                .collect(),
            None => self.schedule.iter().take(n).copied().collect(),
        };
        for id in order {
            self.schedule_cursor = Some(id);
            self.scheduled_update(id, now, normal_time_factor);
        }
    }

    /// A stalker's or monster's scheduled update (7.1): with state flags
    /// 0x08, 0x10 and 0x20 and health above 0.001, the smart-terrain
    /// choice, then, with a smart terrain, its target and a movement update.
    fn scheduled_update(&mut self, id: u16, now: u64, normal_time_factor: f32) {
        let Some(o) = self.objects.get(&id) else {
            return;
        };
        let flags = o.record.object.object_block().state_flags;
        if flags & STATE_SCHEDULED != STATE_SCHEDULED
            || !o.health().is_some_and(|h| h > SCHEDULE_HEALTH)
        {
            return;
        }
        self.choose_smart_terrain(id, now);
        let Some(terrain) = self.smart_terrain(id).filter(|&t| t != NO_ID) else {
            return;
        };
        if let Some(target) = self
            .scripts
            .task(terrain, id)
            .and_then(|name| self.path_target(&name))
            && let Some(t) = self.objects.get_mut(&id).and_then(|o| o.travel.as_mut())
        {
            t.target = target;
        }
        self.movement_update(id, now, normal_time_factor);
    }

    /// The smart-terrain id of `id`'s Monster-like block.
    fn smart_terrain(&self, id: u16) -> Option<u16> {
        Some(
            self.objects
                .get(&id)?
                .record
                .object
                .monster_like()?
                .smart_terrain_id,
        )
    }

    /// 7.2: without a smart terrain, allowed to and with
    /// `smart_terrain_choose_interval` passed since the last look, the
    /// object asks every smart terrain in increasing id and joins the most
    /// suitable that accepts it (of equals the first).
    fn choose_smart_terrain(&mut self, id: u16, now: u64) {
        if self.smart_terrain(id) != Some(NO_ID) {
            return;
        }
        let Some(t) = self.objects.get_mut(&id).and_then(|o| o.travel.as_mut()) else {
            return;
        };
        if !t.can_choose || now.saturating_sub(t.last_look) < t.config.choose_interval {
            return;
        }
        t.last_look = now;
        let mut best: Option<(f32, u16)> = None;
        for &terrain in &self.smart_terrains {
            if let Some(s) = self.scripts.suitability(terrain, id)
                && best.is_none_or(|(b, _)| s > b)
            {
                best = Some((s, terrain));
            }
        }
        if let Some((_, terrain)) = best {
            let o = self.objects.get_mut(&id).expect("object");
            if let Some(m) = o.record.object.monster_like_mut() {
                m.smart_terrain_id = terrain;
            }
            if let Some(t) = o.travel.as_mut() {
                t.last_look = 0;
            }
            self.scripts.join(terrain, id);
        }
    }

    /// The target of a task naming path `name` (7.3): its point 0. A
    /// missing path or a point 0 with an invalid game vertex leaves the
    /// target as it was (our choice: unestablished).
    fn path_target(&self, name: &str) -> Option<Target> {
        let p = self.paths.get(name)?.points.first()?;
        self.graph.graph.vertex(p.game_vertex)?;
        Some(Target {
            game_vertex: p.game_vertex,
            level_vertex: p.level_vertex,
            position: p.position,
        })
    }

    /// Whether `vertex` is on the current level.
    fn on_current_level(&self, vertex: u16) -> bool {
        let level = self.current_level();
        self.graph
            .graph
            .vertex(vertex)
            .is_some_and(|v| Some(v.level_id) == level)
    }
}

/// `n` ids from the pool at `next`: the first of them.
fn take(next: &mut u16, n: u16) -> io::Result<u16> {
    let first = *next;
    *next = next
        .checked_add(n)
        .filter(|&n| n < NO_ID)
        .ok_or_else(|| invalid("the id pool is used up"))?;
    Ok(first)
}

/// Whether `record` is the actor's (6.1: spawn flags bit 0x08 and direct
/// control).
fn is_actor(record: &ObjectRecord) -> bool {
    record.header.flags & FLAG_PLAYER != 0 && record.object.object_block().direct_control != 0
}

/// A stalker's or monster's travel configuration (7.2, 7.4, 7.5) from its
/// section in `system` and its custom data.
fn travel_config(record: &ObjectRecord, system: &Config) -> io::Result<travel::TravelConfig> {
    let section = &record.header.section;
    let get = |key: &str| system.get(section, key).flatten();
    // `going_speed`, `smart_terrain_choose_interval` and (without the
    // custom data's) `terrain` are read: a missing one is fatal
    // (ltx-config.md 11); `current_level_going_speed` defaults to
    // `going_speed` (7.4).
    let read = |key: &str| system.read(section, key);
    let going_speed = value::float(read("going_speed")?);
    let current_level_going_speed =
        get("current_level_going_speed").map_or(going_speed, value::float);
    // A malformed interval is 0 (our choice; 7.2 gives only the format).
    let choose_interval = interval_ms(read("smart_terrain_choose_interval")?).unwrap_or(0);
    let custom = &record.object.object_block().custom_data;
    let custom = volya_engine_formats::cp1251::encode(custom)
        .and_then(|b| ltx::parse(&b).ok())
        .unwrap_or_default();
    let masks: Vec<TerrainMask> = match custom.get("alife", "terrain").flatten() {
        Some(t) => terrain_masks(t, &custom),
        None => terrain_masks(read("terrain")?, system),
    };
    Ok(travel::TravelConfig {
        going_speed,
        current_level_going_speed,
        choose_interval,
        masks,
    })
}

#[cfg(test)]
mod tests;
