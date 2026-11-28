// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Relations, community and ranking as scripts reach them (script API Q,
//! docs/behavior/script-api-relations-community-and-ranking.md; section
//! numbers are its): the `game_object` members for team, squad and group
//! (3), relations and goodwill (4), rank, reputation and community (5)
//! and relation flags (6), the namespace `relation_registry` (7) and the
//! namespace `actor_stats` (8).
//!
//! Each online object's values are its [`ObjectRelations`], copied from
//! its simulation object as it comes online ([`Scripts::spawn`]). The
//! game's own state, the relations registry, the ranking list and the
//! player's statistics, is the simulation's [`Relations`]
//! ([`crate::simulation::AlifeSimulator::relations`]), which lives as long
//! as the game. `game_relations.ltx`'s lists and tables are read from the
//! system configuration, each once, when first needed (2.3). The members
//! and namespaces are defined by the Lua side (`relations.lua`) over one
//! hidden Rust function, as the inventory's are. See
//! docs/architecture/scripts.md, Relations, community and ranking.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use volya_engine_core::config::{Config, value};
use volya_engine_formats::cp1251;
use volya_engine_formats::object::{Object, ObjectRecord, TraderLike};

use crate::classes::ObjectMaker;
use crate::files::FileSystem;
use crate::game_object::members::Handle;
use crate::game_object::{GameObject, GameObjects, Kinds, ObjectClass};
use crate::loader::Scripts;
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::simulation::AlifeSimulator;
use crate::values::{Flags32, to_bits32};
use crate::{Error, ErrorKind, Value};

/// The 19 registered members of `game_object` (1.2).
pub const MEMBERS: [&str; 19] = [
    "team",
    "squad",
    "group",
    "change_team",
    "relation",
    "set_relation",
    "goodwill",
    "set_goodwill",
    "change_goodwill",
    "general_goodwill",
    "rank",
    "character_rank",
    "set_character_rank",
    "character_reputation",
    "change_character_reputation",
    "character_community",
    "set_character_community",
    "get_actor_relation_flags",
    "set_actor_relation_flags",
];

/// The functions of `relation_registry` (1.2).
pub const RELATION_REGISTRY: [&str; 3] = [
    "change_community_goodwill",
    "set_community_goodwill",
    "community_goodwill",
];

/// The functions of `actor_stats` (1.2).
pub const ACTOR_STATS: [&str; 6] = [
    "get_actor_ranking",
    "remove_from_ranking",
    "add_to_ranking",
    "get_points",
    "add_points_str",
    "add_points",
];

/// The relation values (4.1; spec M 8's constants).
pub const FRIEND: i32 = 0;
pub const NEUTRAL: i32 = 1;
pub const ENEMY: i32 = 2;
/// The unnamed relation value 3 (4.1).
pub const UNNAMED: i32 = 3;
pub const DUMMY: i32 = -1;

/// The attitude that gives `neutral` whatever the thresholds (4.1).
const NEUTRAL_ATTITUDE: i32 = -2_147_483_647;

/// The section `get_points` sums every other over (8.2).
const TOTAL: &[u8] = b"total";

/// The spec gap of the outcomes it leaves open: a member reaching one
/// raises a script error naming it.
const GAP: &str = "#1198";

/// An entity's team, squad and group (2.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Team {
    pub team: u8,
    pub squad: u8,
    pub group: u8,
}

/// A living object's monster community (2.1): the position of its
/// section's `species` in `[monster_communities] communities`, −1 when the
/// list lacks it. Kept as the name, its position found when first needed
/// (the configuration doesn't change, so it is the position at load).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MonsterCommunity {
    Species(String),
    /// Its section has no `species` (TODO(#1198): item 4).
    NoSpecies,
}

/// An inventory owner's community, rank and reputation (2.1, 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharacterValues {
    /// A community's position (`identity` 1.3); −1 none.
    pub community: i32,
    pub rank: i32,
    pub reputation: i32,
}

/// An online object's values (2.1), by its kinds; `None` where the object
/// lacks the kind, or where what it holds is open (TODO(#1198)).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectRelations {
    /// An entity's; `None` too when the simulation had no object of its id
    /// as it came online (TODO(#1198): item 7).
    pub team: Option<Team>,
    /// A living object's.
    pub monster_community: Option<MonsterCommunity>,
    /// An inventory owner's, copied from its simulation object's
    /// Trader-like block; `None` for a monster (unestablished, 5.4) or
    /// without that block.
    pub character: Option<CharacterValues>,
    /// A monster's rank: its section's `rank`, 0 when absent (2.1).
    pub monster_rank: i32,
    /// A stalker's relation flags (6), 0 as it comes online.
    pub relation_flags: u32,
}

/// One entry of the ranking list (8.1): an object id and whether the
/// simulation had its Trader-like part when it was added (the rank is
/// read from that part when the list is sorted).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RankingEntry {
    pub id: u16,
    pub trader_like: bool,
}

/// A detail of the player's statistics (8.2; `sav` 9.10).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detail {
    pub key: Vec<u8>,
    pub count: i32,
    pub points: i32,
    pub text: Vec<u8>,
}

/// A section of the player's statistics (8.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatisticsSection {
    pub key: Vec<u8>,
    pub details: Vec<Detail>,
}

impl StatisticsSection {
    /// The sum over its details of count × points; −1 when any detail
    /// has a text (8.2).
    pub fn points(&self) -> i32 {
        if self.details.iter().any(|d| !d.text.is_empty()) {
            return -1;
        }
        (self.details.iter()).fold(0i32, |sum, d| {
            sum.wrapping_add(d.count.wrapping_mul(d.points))
        })
    }
}

/// The simulation's relations state (2.2, 8): the relations registry, the
/// ranking list and the player's statistics. TODO(#1199): saved and
/// loaded with the game's chunk 9.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Relations {
    /// Per object id, its personal table: another object's id → goodwill.
    pub personal: BTreeMap<u16, BTreeMap<u16, i32>>,
    /// Per object id, its community table: a community's position →
    /// goodwill.
    pub community: BTreeMap<u16, BTreeMap<i32, i32>>,
    pub ranking: Vec<RankingEntry>,
    /// The statistics, keyed by the player's id: its sections in the order
    /// made.
    pub statistics: BTreeMap<u16, Vec<StatisticsSection>>,
}

impl Relations {
    /// The personal goodwill of `from` towards `to` (2.2): 0 when none was
    /// stored.
    pub fn personal_goodwill(&self, from: u16, to: u16) -> i32 {
        (self.personal.get(&from))
            .and_then(|t| t.get(&to))
            .copied()
            .unwrap_or(0)
    }

    /// The community goodwill of the community at `position` towards
    /// object `to` (2.2): 0 when none was stored.
    pub fn community_goodwill(&self, position: i32, to: u16) -> i32 {
        (self.community.get(&to))
            .and_then(|t| t.get(&position))
            .copied()
            .unwrap_or(0)
    }
}

/// A list of `game_relations.ltx` (2.3): names with their numbers.
type List = Vec<(String, i32)>;

/// A table of `game_relations.ltx` (2.3): rows by position.
type Table = Vec<Vec<i32>>;

/// The lists of 2.3, by section and key.
const COMMUNITIES: (&str, &str) = ("game_relations", "communities");
const RATING: (&str, &str) = ("game_relations", "rating");
const REPUTATION: (&str, &str) = ("game_relations", "reputation");
const MONSTER_COMMUNITIES: (&str, &str) = ("monster_communities", "communities");

/// The tables of 2.3 and the list giving their rows and columns.
const COMMUNITIES_RELATIONS: (&str, (&str, &str)) = ("communities_relations", COMMUNITIES);
const RANK_RELATIONS: (&str, (&str, &str)) = ("rank_relations", RATING);
const REPUTATION_RELATIONS: (&str, (&str, &str)) = ("reputation_relations", REPUTATION);
const MONSTER_RELATIONS: (&str, (&str, &str)) = ("monster_relations", MONSTER_COMMUNITIES);

/// What the system configuration gives, each read once on first use
/// (2.2, 2.3, 4.1, 4.2).
#[derive(Default)]
struct Readings {
    lists: HashMap<&'static str, Rc<List>>,
    tables: HashMap<&'static str, Rc<Table>>,
    numbers: HashMap<(&'static str, &'static str), Rc<Vec<i32>>>,
}

/// What the raw function shares.
#[derive(Clone)]
pub(crate) struct Context {
    pub alife: Rc<RefCell<AlifeSimulator>>,
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub files: Rc<RefCell<FileSystem>>,
    pub fatal: Rc<RefCell<Option<String>>>,
    readings: Rc<RefCell<Readings>>,
}

type Results = Result<Vec<Value>, String>;

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    fn report(&self, function: &str, line: u32, description: &str, arguments: &[&str]) -> String {
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

    /// A fatal error standing for a fault the spec believes (our wording).
    fn crash(&self, function: &str, text: &str) -> String {
        self.report(function, line!(), "<no expression>", &[text])
    }

    /// A failed check (`console-log` 3.8): fatal.
    fn failed_check(&self, function: &str, description: &str, arguments: &[&str]) -> String {
        self.report(function, line!(), description, arguments)
    }

    fn system(&self) -> Rc<Config> {
        Rc::clone(&self.files.borrow().system)
    }

    /// The configuration's text of `key` in `section`; a missing section
    /// or key is the reader's fatal error (`ltx-config` 11).
    fn read(&self, section: &str, key: &str) -> Result<String, String> {
        let system = self.system();
        match system.read(section, key) {
            Ok(v) => Ok(v.to_owned()),
            Err(e) => Err(self.failed_check("game_relations", &e.to_string(), &[])),
        }
    }

    /// The list `(section, key)` (2.3).
    fn list(&self, (section, key): (&'static str, &'static str)) -> Result<Rc<List>, String> {
        let name = list_name((section, key));
        if let Some(l) = self.readings.borrow().lists.get(name) {
            return Ok(Rc::clone(l));
        }
        let list = Rc::new(parse_list(&self.read(section, key)?));
        (self.readings.borrow_mut().lists).insert(name, Rc::clone(&list));
        Ok(list)
    }

    /// The table `section` with its rows and columns from `rows` (2.3).
    fn table(
        &self,
        (section, rows): (&'static str, (&'static str, &'static str)),
    ) -> Result<Rc<Table>, String> {
        if let Some(t) = self.readings.borrow().tables.get(section) {
            return Ok(Rc::clone(t));
        }
        let rows = self.list(rows)?;
        let system = self.system();
        let Some(s) = system.section(section) else {
            let text = format!("no section [{section}]");
            return Err(self.failed_check("game_relations", &text, &[]));
        };
        let mut table: Table = vec![Vec::new(); rows.len()];
        for entry in s.entries() {
            let Some(row) = rows.iter().position(|(n, _)| n == entry.key()) else {
                let description = format!("wrong community {} in section [{section}]", entry.key());
                return Err(self.failed_check("game_relations", &description, &[]));
            };
            table[row] = (entry.value().unwrap_or("").split(','))
                .map(|item| value::int(item) as i32)
                .collect();
        }
        if s.len() != rows.len() {
            let description = "wrong size for table in section";
            return Err(self.failed_check("game_relations", description, &[section]));
        }
        let table = Rc::new(table);
        (self.readings.borrow_mut().tables).insert(section, Rc::clone(&table));
        Ok(table)
    }

    /// The comma-separated integers of `(section, key)`, read once.
    fn numbers(&self, section: &'static str, key: &'static str) -> Result<Rc<Vec<i32>>, String> {
        if let Some(n) = self.readings.borrow().numbers.get(&(section, key)) {
            return Ok(Rc::clone(n));
        }
        let text = self.read(section, key)?;
        let numbers = Rc::new(text.split(',').map(|i| value::int(i) as i32).collect());
        (self.readings.borrow_mut().numbers).insert((section, key), Rc::clone(&numbers));
        Ok(numbers)
    }

    fn number(&self, section: &'static str, key: &'static str) -> Result<i32, String> {
        Ok(self.numbers(section, key)?.first().copied().unwrap_or(0))
    }

    /// The limits `(section, key)`: minimum, maximum (2.2).
    fn limits(&self, key: &'static str) -> Result<(i32, i32), String> {
        let n = self.numbers("action_points", key)?;
        let at = |i: usize| n.get(i).copied().unwrap_or(0);
        Ok((at(0), at(1)))
    }
}

fn list_name((section, key): (&str, &str)) -> &'static str {
    match (section, key) {
        ("game_relations", "communities") => "communities",
        ("game_relations", "rating") => "rating",
        ("game_relations", "reputation") => "reputation",
        _ => "monster_communities",
    }
}

/// A list's items split at commas and taken as pairs (name, number), a
/// name without a number after it getting 0 (2.3).
fn parse_list(text: &str) -> List {
    let items: Vec<&str> = text.split(',').collect();
    items
        .chunks(2)
        .map(|pair| {
            let name = pair[0].trim().to_owned();
            (name, pair.get(1).map_or(0, |n| value::int(n) as i32))
        })
        .collect()
}

/// The category of `value` in `list` (2.3): the first entry whose number
/// is greater than it, else the last.
fn category(list: &List, value: i32) -> Result<usize, String> {
    if list.is_empty() {
        return Err(unestablished("a category in an empty list"));
    }
    Ok((list.iter())
        .position(|(_, n)| *n > value)
        .unwrap_or(list.len() - 1))
}

/// T[`a`][`b`] (2.3): a missing item 0.
fn cell(table: &[Vec<i32>], a: usize, b: usize) -> i32 {
    table
        .get(a)
        .and_then(|row| row.get(b))
        .copied()
        .unwrap_or(0)
}

/// The script error of an outcome the spec leaves open.
fn unestablished(what: &str) -> String {
    format!("{what} is unestablished ({GAP})")
}

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

fn number(args: &[Value], i: usize, what: &str) -> Result<f64, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(*n),
        _ => Err(format!("{what}: number expected")),
    }
}

/// A number argument made an integer (spec E 2.2's conversion).
fn integer(args: &[Value], i: usize, what: &str) -> Result<i32, String> {
    number(args, i, what).map(to_bits32)
}

/// An object id argument.
fn id_arg(args: &[Value], i: usize, what: &str) -> Result<u16, String> {
    integer(args, i, what).map(|n| n as u16)
}

fn text(args: &[Value], i: usize, what: &str) -> Result<Vec<u8>, String> {
    match arg(args, i) {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!("{what}: text expected")),
    }
}

/// A handle argument's object id (our wording for a mismatch, depends on
/// A).
fn handle(maker: &ObjectMaker, args: &[Value], i: usize, what: &str) -> Result<u16, String> {
    maker
        .get::<Handle>(arg(args, i))
        .map(|h| h.id)
        .ok_or_else(|| format!("{what}: game object expected"))
}

fn whole(n: i32) -> Results {
    Ok(vec![Value::Number(f64::from(n))])
}

fn nothing() -> Results {
    Ok(Vec::new())
}

/// The simulation object's Trader-like block, for kinds that have one
/// (`sav` 7.4: a stalker, the trader, the player).
fn trader_like(record: &ObjectRecord) -> Option<&TraderLike> {
    match &record.object {
        Object::Player(o) => Some(&o.trader),
        Object::Stalker(o) => Some(&o.trader),
        Object::Trader(o) => Some(&o.trader),
        _ => None,
    }
}

fn trader_like_mut(record: &mut ObjectRecord) -> Option<&mut TraderLike> {
    match &mut record.object {
        Object::Player(o) => Some(&mut o.trader),
        Object::Stalker(o) => Some(&mut o.trader),
        Object::Trader(o) => Some(&mut o.trader),
        _ => None,
    }
}

fn is_character(o: &GameObject) -> bool {
    o.kinds.has(Kinds::INVENTORY_OWNER) && !o.kinds.has(Kinds::MONSTER)
}

fn is_trader(o: &GameObject) -> bool {
    o.class == ObjectClass::Named("CAI_Trader")
}

/// An inventory owner's values: open for a monster (TODO(#1198): item 1)
/// or a character the simulation had no object for (item 7).
fn character(o: &GameObject) -> Result<CharacterValues, String> {
    o.relations.character.ok_or_else(|| {
        if o.kinds.has(Kinds::MONSTER) {
            unestablished("a monster's inventory-owner community, rank and reputation")
        } else {
            unestablished("the values of a character without a simulation object")
        }
    })
}

fn team(o: &GameObject) -> Result<Team, String> {
    (o.relations.team)
        .ok_or_else(|| unestablished("the team of an object without a simulation object"))
}

/// The state the raw function works on, borrowed once.
struct Call<'a> {
    ctx: &'a Context,
    objects: &'a mut GameObjects,
    alife: &'a mut AlifeSimulator,
}

impl Call<'_> {
    fn object(&self, id: u16) -> Result<&GameObject, String> {
        self.objects
            .get(id)
            .ok_or_else(|| format!("game object {id} no longer exists"))
    }

    fn object_mut(&mut self, id: u16) -> Result<&mut GameObject, String> {
        self.objects
            .get_mut(id)
            .ok_or_else(|| format!("game object {id} no longer exists"))
    }

    /// The simulation object `id`'s Trader-like block, if the simulation
    /// has the object and its state has one (5.4).
    fn simulation_trader(&mut self, id: u16) -> Option<&mut TraderLike> {
        let sim = self.alife.simulation.as_mut()?;
        trader_like_mut(&mut sim.object_mut(id)?.record)
    }

    fn has_simulation_trader(&mut self, id: u16) -> bool {
        self.simulation_trader(id).is_some()
    }

    /// The player's id (conventions: the program's one reference to it).
    fn player(&self) -> Option<u16> {
        (self.objects.iter())
            .find(|o| o.kinds.has(Kinds::PLAYER))
            .map(|o| o.id)
    }

    /// Stores the personal goodwill of `from` towards `to`, clamped (2.2).
    fn store_personal(&mut self, from: u16, to: u16, value: i32) -> Result<(), String> {
        let (min, max) = self.ctx.limits("personal_goodwill_limits")?;
        let value = value.min(max).max(min);
        let table = self.alife.relations.personal.entry(from).or_default();
        table.insert(to, value);
        Ok(())
    }

    /// Stores the community goodwill of the community at `position`
    /// towards `to`, clamped (2.2).
    fn store_community(&mut self, position: i32, to: u16, value: i32) -> Result<(), String> {
        let (min, max) = self.ctx.limits("community_goodwill_limits")?;
        let value = value.min(max).max(min);
        let table = self.alife.relations.community.entry(to).or_default();
        table.insert(position, value);
        Ok(())
    }

    /// The position of community `name` in the community list; a name the
    /// list lacks fails a check (5.3, 7).
    fn community(&self, op: &str, name: &[u8]) -> Result<(usize, i32), String> {
        let list = self.ctx.list(COMMUNITIES)?;
        let name = cp1251::decode(name);
        match list.iter().position(|(n, _)| *n == name) {
            Some(p) => Ok((p, list[p].1)),
            None => Err(self.ctx.failed_check(op, "item not found, id", &[&name])),
        }
    }

    /// `change_team(team, squad, group)` on object `id` (3).
    fn change_team(&mut self, id: u16, new: Team) -> Result<(), String> {
        let o = self.object_mut(id)?;
        if !o.kinds.has(Kinds::FIGHTER) {
            return Ok(());
        }
        if team(o)? != new {
            o.relations.team = Some(new);
        }
        Ok(())
    }

    /// The attitude of character `a` towards character `b` (4.4).
    fn attitude(&self, a: u16, b: u16) -> Result<i32, String> {
        let (oa, ob) = (self.object(a)?, self.object(b)?);
        let (ca, cb) = (character(oa)?, character(ob)?);
        let ctx = self.ctx;
        let communities = ctx.list(COMMUNITIES)?;
        let position = |c: i32| {
            usize::try_from(c)
                .ok()
                .filter(|&p| p < communities.len())
                .ok_or_else(|| unestablished("a community of -1 in the relations tables"))
        };
        let (pa, pb) = (position(ca.community)?, position(cb.community)?);
        let reputations = ctx.list(REPUTATION)?;
        let ranks = ctx.list(RATING)?;
        let terms = [
            self.alife.relations.personal_goodwill(a, b),
            cell(
                &ctx.table(REPUTATION_RELATIONS)?,
                category(&reputations, ca.reputation)?,
                category(&reputations, cb.reputation)?,
            ),
            cell(
                &ctx.table(RANK_RELATIONS)?,
                category(&ranks, ca.rank)?,
                category(&ranks, cb.rank)?,
            ),
            self.alife.relations.community_goodwill(ca.community, b),
            cell(&ctx.table(COMMUNITIES_RELATIONS)?, pa, pb),
        ];
        Ok(terms.into_iter().fold(0i32, i32::wrapping_add))
    }

    /// `obj:relation(other)` (4.1).
    fn relation(&self, a: u16, b: u16) -> Result<i32, String> {
        let (oa, ob) = (self.object(a)?, self.object(b)?);
        if !oa.kinds.has(Kinds::LIVING) || !ob.kinds.has(Kinds::LIVING) {
            return Ok(DUMMY);
        }
        if (oa.kinds.has(Kinds::STALKER) || is_trader(oa)) && is_character(ob) {
            let attitude = self.attitude(a, b)?;
            let neutral = self
                .ctx
                .number("game_relations", "attitude_neutal_threshold")?;
            let friend = self
                .ctx
                .number("game_relations", "attitude_friend_threshold")?;
            return Ok(if attitude == NEUTRAL_ATTITUDE {
                NEUTRAL
            } else if attitude < neutral {
                ENEMY
            } else if attitude < friend {
                NEUTRAL
            } else {
                FRIEND
            });
        }
        let communities = self.ctx.list(MONSTER_COMMUNITIES)?;
        let position = |o: &GameObject| match &o.relations.monster_community {
            Some(MonsterCommunity::Species(species)) => (communities.iter())
                .position(|(n, _)| n == species)
                .ok_or_else(|| unestablished("a monster community of -1 in [monster_relations]")),
            _ => Err(unestablished(
                "the monster community of a section without species",
            )),
        };
        let (pa, pb) = (position(oa)?, position(ob)?);
        Ok(match cell(&self.ctx.table(MONSTER_RELATIONS)?, pa, pb) {
            1 => FRIEND,
            0 => NEUTRAL,
            -1 => ENEMY,
            -2 => UNNAMED,
            _ => DUMMY,
        })
    }

    /// The player's statistics section `key`, made if missing (8.2).
    fn section(&mut self, op: &str, key: &[u8]) -> Result<&mut StatisticsSection, String> {
        let player = self
            .player()
            .ok_or_else(|| self.ctx.crash(op, "there is no player"))?;
        let sections = self.alife.relations.statistics.entry(player).or_default();
        let i = match sections.iter().position(|s| s.key == key) {
            Some(i) => i,
            None => {
                sections.push(StatisticsSection {
                    key: key.to_vec(),
                    details: Vec::new(),
                });
                sections.len() - 1
            }
        };
        Ok(&mut sections[i])
    }
}

/// The player's statistics section's detail `key`, made if missing.
fn detail<'a>(section: &'a mut StatisticsSection, key: &[u8]) -> &'a mut Detail {
    let i = match section.details.iter().position(|d| d.key == key) {
        Some(i) => i,
        None => {
            section.details.push(Detail {
                key: key.to_vec(),
                ..Detail::default()
            });
            section.details.len() - 1
        }
    };
    &mut section.details[i]
}

fn register(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    let lua = scripts.lua_mut();
    let c = ctx.clone();
    lua.register_with_objects("__volya_relations", move |maker, args| raw(&c, maker, args))?;
    lua.exec("=relations", include_bytes!("relations.lua"))
        .map(drop)
}

/// The engine part of the Lua side's names: `args[0]` names it; a
/// member's `args[1]` is its object.
fn raw(ctx: &Context, maker: &ObjectMaker, args: &[Value]) -> Results {
    ctx.stopped()?;
    let op = match arg(args, 0) {
        Value::String(op) => String::from_utf8_lossy(op).into_owned(),
        _ => return Err("relations: operation expected".into()),
    };
    let op = op.as_str();
    let mut objects = (ctx.objects.try_borrow_mut()).map_err(|_| "the game objects are in use")?;
    let mut alife = (ctx.alife.try_borrow_mut()).map_err(|_| "the simulation is in use")?;
    let mut call = Call {
        ctx,
        objects: &mut objects,
        alife: &mut alife,
    };
    let c = &mut call;
    if let Some(name) = op.strip_prefix("relation_registry.") {
        return relation_registry(c, name, &args[1..]);
    }
    if let Some(name) = op.strip_prefix("actor_stats.") {
        return actor_stats(c, name, &args[1..]);
    }
    let id = handle(maker, args, 1, op)?;
    let args = args.get(2..).unwrap_or(&[]);
    let o = c.object(id)?;
    let kinds = o.kinds;
    let owner = kinds.has(Kinds::INVENTORY_OWNER);
    match op {
        // 3.
        "team" | "squad" | "group" => {
            if !kinds.has(Kinds::ENTITY) {
                return whole(-1);
            }
            let t = team(o)?;
            let n = match op {
                "team" => t.team,
                "squad" => t.squad,
                _ => t.group,
            };
            whole(i32::from(n))
        }
        "change_team" => {
            // Each made a byte (believed: the integer's low 8 bits).
            let byte = |i: usize| integer(args, i, op).map(|n| n as u8);
            let new = Team {
                team: byte(0)?,
                squad: byte(1)?,
                group: byte(2)?,
            };
            c.change_team(id, new)?;
            nothing()
        }
        // 4.1.
        "relation" => {
            let other = handle(maker, args, 0, op)?;
            whole(c.relation(id, other)?)
        }
        // 4.2.
        "goodwill" => {
            let other = handle(maker, args, 0, op)?;
            whole(if owner {
                c.alife.relations.personal_goodwill(id, other)
            } else {
                0
            })
        }
        "set_goodwill" | "change_goodwill" => {
            let value = integer(args, 0, op)?;
            let other = handle(maker, args, 1, op)?;
            if owner {
                let value = if op == "change_goodwill" {
                    (c.alife.relations.personal_goodwill(id, other)).wrapping_add(value)
                } else {
                    value
                };
                c.store_personal(id, other, value)?;
            }
            nothing()
        }
        "set_relation" => {
            let relation = integer(args, 0, op)?;
            let other = handle(maker, args, 1, op)?;
            if owner && c.object(other)?.kinds.has(Kinds::INVENTORY_OWNER) {
                let key = match relation {
                    FRIEND => "goodwill_friend",
                    NEUTRAL => "goodwill_neutal",
                    _ => "goodwill_enemy",
                };
                let value = c.ctx.number("game_relations", key)?;
                c.store_personal(id, other, value)?;
            }
            nothing()
        }
        // 4.3.
        "general_goodwill" => {
            let other = handle(maker, args, 0, op)?;
            let owners = owner && c.object(other)?.kinds.has(Kinds::INVENTORY_OWNER);
            if !owners {
                return Err(c.ctx.crash(op, "an object that is not an inventory owner"));
            }
            whole(c.attitude(id, other)?)
        }
        // 5.1.
        "rank" => {
            if !kinds.has(Kinds::STALKER) {
                return whole(0);
            }
            whole(character(o)?.rank)
        }
        "character_rank" => {
            if kinds.has(Kinds::MONSTER) {
                return whole(o.relations.monster_rank);
            }
            if !owner {
                return whole(0);
            }
            whole(character(o)?.rank)
        }
        "set_character_rank" => {
            let rank = integer(args, 0, op)?;
            if owner && let Some(t) = c.simulation_trader(id) {
                t.rank = rank;
                let values = character_from(t);
                let o = c.object_mut(id)?;
                o.relations.character.get_or_insert(values).rank = rank;
            }
            nothing()
        }
        // 5.2.
        "character_reputation" => {
            if !owner {
                return whole(0);
            }
            whole(character(o)?.reputation)
        }
        "change_character_reputation" => {
            let delta = integer(args, 0, op)?;
            if owner && c.has_simulation_trader(id) {
                let reputation = character(c.object(id)?)?.reputation.wrapping_add(delta);
                if let Some(t) = c.simulation_trader(id) {
                    t.reputation = reputation;
                }
                if let Some(v) = &mut c.object_mut(id)?.relations.character {
                    v.reputation = reputation;
                }
            }
            nothing()
        }
        // 5.3.
        "character_community" => {
            if !owner {
                return Ok(vec![Value::Nil]);
            }
            let position = character(o)?.community;
            let list = c.ctx.list(COMMUNITIES)?;
            match usize::try_from(position).ok().and_then(|p| list.get(p)) {
                Some((name, _)) => Ok(vec![Value::String(
                    cp1251::encode(name).unwrap_or_else(|| name.as_bytes().to_vec()),
                )]),
                None => Err(c.ctx.failed_check(
                    op,
                    "item by index not found in section %s, line %s",
                    &["game_relations", &position.to_string()],
                )),
            }
        }
        "set_character_community" => {
            let name = text(args, 0, op)?;
            let byte = |i: usize| integer(args, i, op).map(|n| n as u8);
            let (squad, group) = (byte(1)?, byte(2)?);
            if !(owner && kinds.has(Kinds::ENTITY)) {
                return nothing();
            }
            let (position, number) = c.community(op, &name)?;
            let number = number as u8;
            if let Some(t) = c.simulation_trader(id) {
                t.community = position as i32;
                let values = character_from(t);
                let o = c.object_mut(id)?;
                o.relations.character.get_or_insert(values).community = position as i32;
                if o.kinds.has(Kinds::FIGHTER) {
                    let current = team(o)?;
                    c.change_team(
                        id,
                        Team {
                            team: number,
                            ..current
                        },
                    )?;
                }
            }
            c.change_team(
                id,
                Team {
                    team: number,
                    squad,
                    group,
                },
            )?;
            nothing()
        }
        // 6.
        "get_actor_relation_flags" => {
            if !kinds.has(Kinds::STALKER) {
                return Err(c.ctx.crash(op, "an object that is not a stalker"));
            }
            let flags = Flags32(o.relations.relation_flags);
            Ok(vec![maker.new_object("flags32", flags)])
        }
        "set_actor_relation_flags" => {
            let Some(flags) = maker.get::<Flags32>(arg(args, 0)) else {
                return Err(format!("{op}: flags32 expected"));
            };
            if !kinds.has(Kinds::STALKER) {
                return Err(c.ctx.crash(op, "an object that is not a stalker"));
            }
            c.object_mut(id)?.relations.relation_flags = flags.0;
            nothing()
        }
        _ => Err(format!("relations: no operation {op}")),
    }
}

fn character_from(t: &TraderLike) -> CharacterValues {
    CharacterValues {
        community: t.community,
        rank: t.rank,
        reputation: t.reputation,
    }
}

/// Section 7.
fn relation_registry(c: &mut Call, name: &str, args: &[Value]) -> Results {
    let what = format!("relation_registry.{name}");
    let community = text(args, 0, &what)?;
    let to = id_arg(args, 1, &what)?;
    let (position, _) = c.community(&what, &community)?;
    let position = position as i32;
    match name {
        "community_goodwill" => whole(c.alife.relations.community_goodwill(position, to)),
        "set_community_goodwill" => {
            let value = integer(args, 2, &what)?;
            c.store_community(position, to, value)?;
            nothing()
        }
        "change_community_goodwill" => {
            let delta = integer(args, 2, &what)?;
            let current = c.alife.relations.community_goodwill(position, to);
            c.store_community(position, to, current.wrapping_add(delta))?;
            nothing()
        }
        _ => Err(format!("relation_registry: no function {name}")),
    }
}

/// Section 8.
fn actor_stats(c: &mut Call, name: &str, args: &[Value]) -> Results {
    let what = format!("actor_stats.{name}");
    match name {
        "add_to_ranking" => {
            let id = id_arg(args, 0, &what)?;
            let trader_like = c.has_simulation_trader(id);
            let ranking = &mut c.alife.relations.ranking;
            ranking.retain(|e| e.id != id);
            ranking.push(RankingEntry { id, trader_like });
            nothing()
        }
        "remove_from_ranking" => {
            let id = id_arg(args, 0, &what)?;
            c.alife.relations.ranking.retain(|e| e.id != id);
            nothing()
        }
        "get_actor_ranking" => {
            let player = c
                .player()
                .ok_or_else(|| unestablished("the ranking without a player"))?;
            let entries = c.alife.relations.ranking.clone();
            let mut ranked = Vec::with_capacity(entries.len());
            for e in entries {
                if !e.trader_like {
                    return Err(c.ctx.crash(&what, "an entry without a Trader-like part"));
                }
                let rank = (c.simulation_trader(e.id)).map(|t| t.rank).ok_or_else(|| {
                    unestablished("a ranking entry whose simulation object is gone")
                })?;
                ranked.push((e, rank));
            }
            ranked.sort_by_key(|&(_, rank)| std::cmp::Reverse(rank));
            c.alife.relations.ranking = ranked.iter().map(|&(e, _)| e).collect();
            let Some(position) = ranked.iter().position(|(e, _)| e.id == player) else {
                return whole(1);
            };
            let rank = ranked[position].1;
            if ranked.iter().filter(|&&(_, r)| r == rank).count() > 1 {
                return Err(unestablished("the order of equal ranks"));
            }
            whole(position as i32)
        }
        "add_points" => {
            let section = text(args, 0, &what)?;
            let key = text(args, 1, &what)?;
            let count = integer(args, 2, &what)?;
            let points = integer(args, 3, &what)?;
            let d = detail(c.section(&what, &section)?, &key);
            d.count = d.count.wrapping_add(count);
            d.points = d.points.wrapping_add(count.wrapping_mul(points));
            nothing()
        }
        "add_points_str" => {
            let section = text(args, 0, &what)?;
            let key = text(args, 1, &what)?;
            let value = text(args, 2, &what)?;
            detail(c.section(&what, &section)?, &key).text = value;
            nothing()
        }
        "get_points" => {
            let section = text(args, 0, &what)?;
            if section != TOTAL {
                return whole(c.section(&what, &section)?.points());
            }
            let player = c
                .player()
                .ok_or_else(|| c.ctx.crash(&what, "there is no player"))?;
            let sections = c.alife.relations.statistics.get(&player);
            let values: Vec<i32> = (sections.into_iter().flatten())
                .map(StatisticsSection::points)
                .filter(|&p| p != -1)
                .collect();
            if values.is_empty() {
                return whole(-1);
            }
            whole(values.into_iter().fold(0i32, i32::wrapping_add))
        }
        _ => Err(format!("actor_stats: no function {name}")),
    }
}

/// An object's values as it comes online (2.1): its team, squad and group
/// from its simulation object's Creature block (0 without one), a
/// monster's team the number of its species when not 255; a living
/// object's monster community; an inventory owner's community, rank and
/// reputation from the Trader-like block; a monster's rank from its
/// section; a stalker's relation flags 0.
fn come_online(ctx: &Context, id: u16) -> Result<(), String> {
    let system = ctx.system();
    let mut objects = (ctx.objects.try_borrow_mut()).map_err(|_| "the game objects are in use")?;
    let alife = (ctx.alife.try_borrow()).map_err(|_| "the simulation is in use")?;
    let Some(o) = objects.get_mut(id) else {
        return Ok(());
    };
    let record = (alife.simulation.as_ref())
        .and_then(|s| s.object(id))
        .map(|s| &s.record);
    let section = cp1251::decode(o.section.as_deref().unwrap_or_default());
    let kinds = o.kinds;
    let mut r = ObjectRelations::default();
    if kinds.has(Kinds::ENTITY) {
        r.team = record.map(|record| {
            record.object.creature().map_or(Team::default(), |c| Team {
                team: c.team,
                squad: c.squad,
                group: c.group,
            })
        });
    }
    if kinds.has(Kinds::LIVING) {
        let community = match system.get(&section, "species") {
            Some(species) => {
                let species = species.unwrap_or("").trim().to_owned();
                if let (true, Some(t)) = (kinds.has(Kinds::MONSTER), &mut r.team) {
                    let list = ctx.list(MONSTER_COMMUNITIES)?;
                    if let Some((_, number)) = list.iter().find(|(n, _)| *n == species)
                        && *number as u8 != 255
                    {
                        t.team = *number as u8;
                    }
                }
                MonsterCommunity::Species(species)
            }
            None => MonsterCommunity::NoSpecies,
        };
        r.monster_community = Some(community);
    }
    if kinds.has(Kinds::INVENTORY_OWNER) && !kinds.has(Kinds::MONSTER) {
        r.character = record.and_then(trader_like).map(character_from);
    }
    if kinds.has(Kinds::MONSTER) {
        r.monster_rank =
            (system.get(&section, "rank").flatten()).map_or(0, |v| value::int(v) as i32);
    }
    o.relations = r;
    Ok(())
}

/// What a state with the relations registered keeps for the engine's
/// calls.
pub(crate) struct Registered {
    ctx: Context,
}

impl Scripts {
    /// Registers script API Q's names (1): after the game object basics
    /// and the simulation registry (whose objects, services, files and
    /// simulation it shares), before [`Scripts::load_globals`].
    pub fn register_relations(&mut self) -> Result<(), Error> {
        let api = |message: &str| Error {
            kind: ErrorKind::Api,
            message: message.into(),
        };
        let basics = (self.game_object_state.as_ref())
            .ok_or_else(|| api("the game object basics aren't registered"))?;
        let simulation = (self.simulation_state.as_ref())
            .ok_or_else(|| api("the simulation registry isn't registered"))?;
        let ctx = Context {
            alife: Rc::clone(&simulation.ctx.state),
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            files: Rc::clone(&basics.files),
            fatal: self.fatal_flag(),
            readings: Rc::default(),
        };
        register(self, &ctx)?;
        self.relations_state = Some(Registered { ctx });
        Ok(())
    }

    /// Object `id` comes online (2.1): its values copied from its
    /// simulation object and section, before its binder's calls. Nothing
    /// when the relations aren't registered.
    pub(crate) fn relations_come_online(&mut self, id: u16) -> Result<(), Error> {
        let Some(r) = &self.relations_state else {
            return Ok(());
        };
        let ctx = r.ctx.clone();
        come_online(&ctx, id).map_err(|message| {
            let kind = if self.fatal_flag().borrow().is_some() {
                ErrorKind::Fatal
            } else {
                ErrorKind::Api
            };
            Error { kind, message }
        })
    }
}
