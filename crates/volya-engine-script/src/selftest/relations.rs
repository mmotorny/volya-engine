// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-relations-community-and-ranking.md on every
//! target, one check per section, over a simulation and relation tables
//! of our own: the player, a stalker, the trader, a dog, a flesh and a
//! monster without a species from simulation records, a stalker and an
//! item without one.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::simulation::graph::Graph;
use volya_engine_core::simulation::{AlifeConfig, NO_ID, Simulation};
use volya_engine_core::switching::SwitchDistances;
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;
use volya_engine_formats::object::{
    ClassId, CompressedVector, Creature, CreatureUpdate, Monster, MonsterLike, MonsterLikeUpdate,
    MonsterState, Object, ObjectBlock, ObjectRecord, Player, PlayerPhysics, PlayerUpdate, Skeleton,
    SpawnHeader, Stalker, Trader, TraderLike, Visual,
};

use super::files::{FSGAME, Tree};
use super::services::run;
use super::simulation::{graph, paths};
use crate::files::FileSystem;
use crate::game_object::{GameObjects, ObjectClass, SpawnData};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::class_id;
use crate::relations::{ACTOR_STATS, MEMBERS, RELATION_REGISTRY};
use crate::services::EngineServices;
use crate::simulation::AlifeSimulator;
use crate::story::Story;
use crate::{Error, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2.1 values at spawn", values_at_spawn),
    ("2.3 lists and tables", lists_and_tables),
    ("3 change_team", change_team),
    ("4.1 relation", relation),
    ("4.2 personal goodwill", personal_goodwill),
    ("4.3 4.4 the attitude", attitude),
    ("5.1 rank", rank),
    ("5.2 reputation", reputation),
    ("5.3 community", community),
    ("6 relation flags", relation_flags),
    ("7 relation_registry", relation_registry),
    ("8.1 the ranking", ranking),
    ("8.2 the player's points", points),
    ("9 worked example", worked_example),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("relations {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const PLAYER: u16 = 0;
const STALKER: u16 = 1;
const TRADER: u16 = 2;
const DOG: u16 = 3;
const FLESH: u16 = 4;
const CAT: u16 = 5;
/// A stalker and an item the simulation has no object for.
const LONE: u16 = 6;
const ITEM: u16 = 7;

/// The bandit's number is missing: 0 (2.3). The `human` row of
/// `[monster_relations]` lacks a comma, as the shipped one does.
const SYSTEM: &str = "[game_relations]\r\n\
communities = actor, 0, stalker, 5, trader, 15, bandit\r\n\
rating = novice, 300, experienced, 600, master\r\n\
reputation = bad, -50, neutral, 50, good\r\n\
attitude_neutal_threshold = -400\r\nattitude_friend_threshold = 500\r\n\
goodwill_friend = 1000\r\ngoodwill_neutal = 0\r\ngoodwill_enemy = -1000\r\n\
[action_points]\r\npersonal_goodwill_limits = -1000, 1000\r\n\
community_goodwill_limits = -3000, 1000\r\n\
[communities_relations]\r\nactor = 0, 0, 0, -5000\r\nstalker = 0, 100, 0, -1000\r\n\
trader = 0, 0, 0, 0\r\nbandit = -5000, -1000, 0, 1000\r\n\
[rank_relations]\r\nnovice = 0, 10, 20\r\nexperienced = 0, 0, 0\r\nmaster = 30, 0, 0\r\n\
[reputation_relations]\r\nbad = 0, 0, 0\r\nneutral = 0, 7, 0\r\ngood = 0, 3, 0\r\n\
[monster_communities]\r\ncommunities = actor, 255, human, 255, dog, 40, flesh, 41\r\n\
[monster_relations]\r\nactor = 1, 0, -1, -2\r\nhuman = 0 \t 1, 0, -1\r\n\
dog = -1, -1, 1, 5\r\nflesh = 0, 0, 0, 0\r\n\
[travel]\r\ngoing_speed = 2\r\nsmart_terrain_choose_interval = 00:15:00\r\n\
terrain = 255,255,255,255\r\npanic_threshold = 0.1\r\n\
[actor]\r\nspecies = actor\r\n\
[stalker]:travel\r\nspecies = human\r\n\
[trader]\r\nspecies = human\r\n\
[dog]:travel\r\nspecies = dog\r\nrank = 401\r\n\
[flesh]:travel\r\nspecies = flesh\r\n\
[cat]:travel\r\n\
[wpn_knife]\r\nclass = W_KNIFE\r\n";

fn creature(team: [u8; 3]) -> Creature {
    Creature {
        object: ObjectBlock {
            game_graph_vertex: 0,
            distance: 0.0,
            direct_control: 1,
            level_graph_node: 5,
            state_flags: 0x07,
            custom_data: String::new().into(),
            story_id: -1,
            spawn_story_id: -1,
        },
        visual: visual(),
        team: team[0],
        squad: team[1],
        group: team[2],
        health: 1.0,
        dynamic_out_restrictions: Vec::new(),
        dynamic_in_restrictions: Vec::new(),
        killer_id: NO_ID,
        death_time: 0,
    }
}

fn visual() -> Visual {
    Visual {
        model: String::new().into(),
        flags: 0,
    }
}

fn skeleton() -> Skeleton {
    Skeleton {
        startup_animation: String::new().into(),
        flags: 0,
        source_id: NO_ID,
        bone_state: None,
    }
}

fn creature_update() -> CreatureUpdate {
    CreatureUpdate {
        health: 1.0,
        timestamp: 0,
        flags: 0,
        position: [0.0; 3],
        model_yaw: 0.0,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        team: 0,
        squad: 0,
        group: 0,
    }
}

fn monster_like(team: [u8; 3]) -> MonsterLike {
    MonsterLike {
        creature: creature(team),
        out_restrictions: String::new().into(),
        in_restrictions: String::new().into(),
        smart_terrain_id: NO_ID,
        smart_terrain_task_reached: 0,
    }
}

fn monster_update() -> MonsterLikeUpdate {
    MonsterLikeUpdate {
        creature: creature_update(),
        next_game_vertex: NO_ID,
        previous_game_vertex: NO_ID,
        distance_from_previous: 0.0,
        distance_to_next: 0.0,
    }
}

/// A Trader-like block of community position `community`.
fn trader_like(community: i32, rank: i32, reputation: i32) -> TraderLike {
    TraderLike {
        money: 0,
        specific_character: String::new().into(),
        trader_flags: 0,
        character_profile: String::new().into(),
        community,
        rank,
        reputation,
        character_name: String::new().into(),
    }
}

fn record(id: u16, section: &str, class: &str, object: Object) -> ObjectRecord {
    ObjectRecord {
        header: SpawnHeader {
            section: section.into(),
            name: format!("{section}_{id}").into(),
            game_type: 0,
            after_game_type: 0,
            position: [0.0; 3],
            angles: [0.0; 3],
            respawn_time: 0,
            id,
            parent_id: NO_ID,
            phantom_id: NO_ID,
            flags: 0,
            version: 118,
            script_version: 7,
            client_data: Vec::new(),
            spawn_id: id,
        },
        class_id: ClassId::new(class).expect("a class id"),
        single_player: true,
        object,
    }
}

fn monster(id: u16, section: &str, team: [u8; 3]) -> ObjectRecord {
    let object = Object::Monster(Box::new(Monster {
        state: MonsterState {
            monster: monster_like(team),
            skeleton: skeleton(),
            special_object_id: NO_ID,
        },
        script: None,
        update: monster_update(),
    }));
    record(id, section, "AI_DOG_R", object)
}

/// The simulation's records: the player (actor, rank 100, reputation 10),
/// the stalker (stalker, 650, 60; team 1, 2, 3), the trader (trader, 330,
/// 23), the dog (team 7, 8, 9), the flesh and the cat.
fn records() -> Vec<ObjectRecord> {
    let player = Object::Player(Box::new(Player {
        creature: creature([0; 3]),
        trader: trader_like(0, 100, 10),
        skeleton: skeleton(),
        holder_id: NO_ID,
        update: PlayerUpdate {
            creature: creature_update(),
            movement_state: 0,
            acceleration: CompressedVector {
                direction: 0,
                magnitude: 0.0,
            },
            velocity: CompressedVector {
                direction: 0,
                magnitude: 0.0,
            },
            radiation: 0.0,
            active_weapon: 0,
            physics: PlayerPhysics::None,
        },
    }));
    let stalker = Object::Stalker(Box::new(Stalker {
        trader: trader_like(1, 650, 60),
        monster: monster_like([1, 2, 3]),
        equipment_preferences: Vec::new(),
        main_weapon_preferences: Vec::new(),
        skeleton: skeleton(),
        script: None,
        update: monster_update(),
        start_dialog: String::new().into(),
    }));
    let trader = Object::Trader(Box::new(Trader {
        object: creature([0; 3]).object,
        visual: visual(),
        trader: trader_like(2, 330, 23),
    }));
    vec![
        record(PLAYER, "actor", "O_ACTOR", player),
        record(STALKER, "stalker", "AI_STL_S", stalker),
        record(TRADER, "trader", "AI_TRADE", trader),
        monster(DOG, "dog", [7, 8, 9]),
        monster(FLESH, "flesh", [1, 1, 1]),
        monster(CAT, "cat", [2, 2, 2]),
    ]
}

/// A test state.
struct State {
    s: Scripts,
    objects: Rc<RefCell<GameObjects>>,
    alife: Rc<RefCell<AlifeSimulator>>,
}

impl State {
    fn lua(&mut self, name: &str, body: &str) -> Result<(), String> {
        run(&mut self.s, name, &format!("{PRELUDE}{body}"))
    }

    /// The simulation record `id`'s Trader-like (community, rank,
    /// reputation).
    fn simulation_values(&self, id: u16) -> Option<(i32, i32, i32)> {
        let alife = self.alife.borrow();
        let record = &alife.simulation.as_ref()?.object(id)?.record;
        let t = match &record.object {
            Object::Player(o) => &o.trader,
            Object::Stalker(o) => &o.trader,
            Object::Trader(o) => &o.trader,
            _ => return None,
        };
        Some((t.community, t.rank, t.reputation))
    }
}

const PRELUDE: &str = "local actor, stalker, trader, dog, flesh, cat, lone, item = \
obj(0), obj(1), obj(2), obj(3), obj(4), obj(5), obj(6), obj(7)\n";

fn data(id: u16, section: &str, class: &str) -> SpawnData {
    SpawnData {
        id,
        name: format!("{section}{id}").into_bytes(),
        section: section.as_bytes().to_vec(),
        class_id: class_id(class.as_bytes()),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: Some(1.0),
        server: None,
    }
}

fn message(e: Error) -> String {
    e.message
}

/// A state with everything up to the relations registered, `_G` loaded, a
/// global `obj(id)` giving handles, the simulation running with
/// [`records`] and the objects online.
fn state() -> Result<State, String> {
    let tree = Rc::new(Tree(
        [("gamedata/scripts/_g.script".to_owned(), Vec::new())]
            .into_iter()
            .collect(),
    ));
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(SYSTEM.as_bytes()).map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(message)?;
    let game = ltx::parse(b"[story_ids]\r\n[spawn_story_ids]\r\n").map_err(|e| e.to_string())?;
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(game),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .map_err(message)?;
    s.register_story(Rc::new(RefCell::new(Story::new())))
        .map_err(message)?;
    let alife = Rc::new(RefCell::new(AlifeSimulator::new()));
    s.register_simulation(Rc::clone(&alife)).map_err(message)?;
    s.register_relations().map_err(message)?;
    let shared = Rc::clone(&objects);
    s.lua_mut()
        .register_object_function("obj", move |args| {
            let Some(Value::Number(id)) = args.first() else {
                return Ok(None);
            };
            Ok(shared.borrow().get(*id as u16).and_then(|o| o.handle()))
        })
        .map_err(message)?;
    s.load_globals().map_err(message)?;
    let mut sim = Simulation::new(
        Graph::new(graph()),
        paths(),
        AlifeConfig {
            distances: SwitchDistances::of(150.0, 0.1),
            objects_per_update: 10,
            stay_after_death: 0,
        },
    );
    sim.load(records(), &system).map_err(|e| e.to_string())?;
    alife.borrow_mut().simulation = Some(sim);
    let spawned = [
        (ObjectClass::Player, data(PLAYER, "actor", "O_ACTOR")),
        (
            ObjectClass::Named("CAI_Stalker"),
            data(STALKER, "stalker", "AI_STL_S"),
        ),
        (
            ObjectClass::Named("CAI_Trader"),
            data(TRADER, "trader", "AI_TRADE"),
        ),
        (ObjectClass::Named("CAI_Dog"), data(DOG, "dog", "AI_DOG_R")),
        (
            ObjectClass::Named("CAI_Flesh"),
            data(FLESH, "flesh", "AI_FLESH"),
        ),
        (ObjectClass::Named("CCat"), data(CAT, "cat", "AI_CAT")),
        (
            ObjectClass::Named("CAI_Stalker"),
            data(LONE, "stalker", "AI_STL_S"),
        ),
        (
            ObjectClass::Named("CWeaponKnife"),
            data(ITEM, "wpn_knife", "W_KNIFE"),
        ),
    ];
    for (class, data) in spawned {
        s.spawn_engine_object(class, data)
            .map_err(|e| e.error.message)?;
    }
    Ok(State { s, objects, alife })
}

/// Runs `code` on a new state, which must stop it with a fatal error
/// containing `text`.
fn fatal(code: &str, text: &str) -> Result<(), String> {
    let mut t = state()?;
    let code = format!("{PRELUDE}{code}");
    match t.s.exec("=fatal", code.as_bytes()) {
        Err(e) if e.message.contains(text) => Ok(()),
        other => Err(format!("{code}: {other:?}, want a fatal error with {text}")),
    }
}

/// 1.1, 1.2: the 19 members, the two namespaces with exactly their
/// functions; `set_character_reputation` and the goodwill methods on game
/// objects are not registered.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    let names = |list: &[&str]| {
        list.iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    t.lua(
        "registered",
        &format!(
            "for _, n in ipairs({{{}}}) do eq(type(game_object[n]), 'function', n) end\n\
             local function count(t) local n = 0 for _ in pairs(t) do n = n + 1 end return n end\n\
             for _, n in ipairs({{{}}}) do eq(type(relation_registry[n]), 'function', n) end\n\
             eq(count(relation_registry), 3, 'relation_registry')\n\
             for _, n in ipairs({{{}}}) do eq(type(actor_stats[n]), 'function', n) end\n\
             eq(count(actor_stats), 6, 'actor_stats')\n\
             eq(game_object.set_character_reputation, nil, 'set_character_reputation')\n\
             eq(game_object.community_goodwill, nil, 'community_goodwill')\n\
             eq(game_object.set_community_goodwill, nil, 'set_community_goodwill')\n\
             raises(function() stalker:set_community_goodwill(-5000, actor) end, '', \
             'a call of a member game_object lacks')",
            names(&MEMBERS),
            names(&RELATION_REGISTRY),
            names(&ACTOR_STATS),
        ),
    )
}

/// 2.1: team, squad and group from the Creature block (0 without one), a
/// monster's team its species' number; the monster rank; nothing known
/// without a simulation object.
pub fn values_at_spawn() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "values at spawn",
        "local function tsg(o) return o:team() .. ' ' .. o:squad() .. ' ' .. o:group() end\n\
         eq(tsg(stalker), '1 2 3', 'the stalker')\n\
         eq(tsg(actor), '0 0 0', 'the player')\n\
         eq(tsg(trader), '0 0 0', 'no Creature block')\n\
         eq(tsg(dog), '40 8 9', \"the dog's species\")\n\
         eq(tsg(flesh), '41 1 1', \"the flesh's species\")\n\
         eq(tsg(cat), '2 2 2', 'no species')\n\
         eq(tsg(item), '-1 -1 -1', 'not an entity')\n\
         raises(function() return lone:team() end, '#1198', 'no simulation object')\n\
         eq(dog:character_rank(), 401, \"the section's rank\")\n\
         eq(flesh:character_rank(), 0, 'no rank key')",
    )
}

/// 2.3: a name without a number gets 0; a value's category; the shifted
/// row; a table's key the list lacks and a table's size are fatal.
pub fn lists_and_tables() -> Result<(), String> {
    let mut t = state()?;
    // The bandit (number 0): its team after the change (5.3).
    t.lua(
        "lists",
        "stalker:set_character_community('bandit', 4, 5)\n\
         eq(stalker:team(), 0, \"the bandit's number\")\n\
         eq(stalker:relation(dog), 2, 'human -> dog -1 (shifted)')\n\
         eq(stalker:relation(flesh), 1, 'human -> flesh: the missing last item 0')",
    )?;
    let wrong = SYSTEM.replace("good = 0, 3, 0", "great = 0, 3, 0");
    with_system(
        &wrong,
        "wrong community great in section [reputation_relations]",
    )?;
    let short = SYSTEM.replace("good = 0, 3, 0\r\n", "");
    with_system(&short, "wrong size for table in section")
}

/// A state over `system`, where the stalker's attitude towards the
/// player is fatal with `text`.
fn with_system(system: &str, text: &str) -> Result<(), String> {
    let mut t = state()?;
    let config = ltx::parse(system.as_bytes()).map_err(|e| e.to_string())?;
    // The tables are read when first needed: a fresh registration reads
    // them from the new configuration.
    let files = Rc::clone(&t.s.game_object_state.as_ref().ok_or("registered")?.files);
    files.borrow_mut().system = Rc::new(config);
    let code = format!("{PRELUDE}return stalker:general_goodwill(actor)");
    match t.s.exec("=fatal", code.as_bytes()) {
        Err(e) if e.message.contains(text) => Ok(()),
        other => Err(format!("{other:?}, want a fatal error with {text}")),
    }
}

/// 3: fighters only; bytes; equal values change nothing.
pub fn change_team() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "change_team",
        "local function tsg(o) return o:team() .. ' ' .. o:squad() .. ' ' .. o:group() end\n\
         stalker:change_team(257, 2, 3)\n\
         eq(tsg(stalker), '1 2 3', 'the low 8 bits, equal')\n\
         stalker:change_team(4, 5, 6)\n\
         eq(tsg(stalker), '4 5 6', 'changed')\n\
         dog:change_team(1, 1, 1)\n\
         eq(tsg(dog), '1 1 1', 'a monster')\n\
         trader:change_team(1, 1, 1)\n\
         eq(tsg(trader), '0 0 0', 'the trader is no fighter')\n\
         actor:change_team(1, 1, 1)\n\
         eq(tsg(actor), '0 0 0', 'the player is no fighter')\n\
         item:change_team(1, 1, 1)",
    )
}

/// 4.1: by the attitude for a stalker or the trader towards a character,
/// by `[monster_relations]` otherwise.
pub fn relation() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "relation",
        "eq(stalker:relation(actor), 1, 'attitude 33: neutral')\n\
         eq(trader:relation(actor), 1, 'attitude 7: neutral')\n\
         stalker:set_goodwill(470, actor)\n\
         eq(stalker:relation(actor), 0, 'attitude 503: friend')\n\
         relation_registry.set_community_goodwill('stalker', actor:id(), -1000)\n\
         eq(stalker:relation(actor), 2, 'attitude -497: enemy')\n\
         eq(actor:relation(actor), 0, 'actor -> actor 1')\n\
         eq(actor:relation(stalker), 1, 'actor -> human 0')\n\
         eq(actor:relation(dog), 2, 'actor -> dog -1')\n\
         eq(actor:relation(flesh), 3, 'actor -> flesh -2')\n\
         eq(dog:relation(actor), 2, 'dog -> actor -1')\n\
         eq(dog:relation(flesh), -1, 'dog -> flesh 5')\n\
         eq(stalker:relation(dog), 2, 'a stalker to a monster: the table')\n\
         eq(item:relation(actor), -1, 'not living')\n\
         eq(actor:relation(item), -1, 'not living')\n\
         raises(function() return dog:relation(cat) end, '#1198', 'no species')",
    )
}

/// 4.2: inventory owners, clamped to the personal limits; `set_relation`'s
/// three values.
pub fn personal_goodwill() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "personal goodwill",
        "eq(stalker:goodwill(actor), 0, 'none stored')\n\
         stalker:set_goodwill(5000, actor)\n\
         eq(stalker:goodwill(actor), 1000, 'clamped')\n\
         stalker:change_goodwill(-2500, actor)\n\
         eq(stalker:goodwill(actor), -1000, 'changed, clamped')\n\
         eq(actor:goodwill(stalker), 0, 'personal: one way')\n\
         dog:set_goodwill(10.9, actor)\n\
         eq(dog:goodwill(actor), 10, 'a monster is an owner; an integer')\n\
         item:set_goodwill(10, actor)\n\
         eq(item:goodwill(actor), 0, 'not an owner')\n\
         stalker:set_goodwill(5, item)\n\
         eq(stalker:goodwill(item), 5, 'any other object')\n\
         stalker:set_relation(0, trader)\n\
         eq(stalker:goodwill(trader), 1000, 'friend')\n\
         stalker:set_relation(1, trader)\n\
         eq(stalker:goodwill(trader), 0, 'neutral')\n\
         stalker:set_relation(-1, trader)\n\
         eq(stalker:goodwill(trader), -1000, 'dummy: enemy')\n\
         stalker:set_relation(0, item)\n\
         eq(stalker:goodwill(item), 5, 'not an owner')",
    )
}

/// 4.3, 4.4: the five terms; a non-owner and a monster.
pub fn attitude() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "attitude",
        "eq(stalker:general_goodwill(actor), 33, 'reputation 3 + rank 30')\n\
         stalker:set_goodwill(100, actor)\n\
         relation_registry.set_community_goodwill('stalker', actor:id(), 50)\n\
         eq(stalker:general_goodwill(actor), 183, 'personal and community')\n\
         eq(stalker:general_goodwill(stalker), 100, 'communities 100')\n\
         eq(actor:general_goodwill(stalker), 20, 'rank 20')\n\
         raises(function() return dog:general_goodwill(actor) end, '#1198', 'a monster')",
    )?;
    fatal(
        "return stalker:general_goodwill(item)",
        "not an inventory owner",
    )
}

/// 5.1: `rank` of stalkers, `character_rank` of monsters and owners;
/// `set_character_rank` under the simulation-object condition (5.4).
pub fn rank() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "rank",
        "eq(stalker:rank(), 650, 'a stalker')\n\
         eq(trader:rank(), 0, 'not a stalker')\n\
         eq(dog:rank(), 0, 'a monster')\n\
         eq(trader:character_rank(), 330, 'the trader')\n\
         eq(actor:character_rank(), 100, 'the player')\n\
         eq(item:character_rank(), 0, 'not an owner')\n\
         actor:set_character_rank(actor:character_rank() + 50.7)\n\
         eq(actor:character_rank(), 150, 'set')\n\
         dog:set_character_rank(5)\n\
         eq(dog:character_rank(), 401, 'no Trader-like block')\n\
         lone:set_character_rank(5)\n\
         raises(function() return lone:character_rank() end, '#1198', 'no simulation object')",
    )?;
    let got = t.simulation_values(PLAYER).map(|v| v.1);
    if got != Some(150) {
        return Err(format!("the player's simulation rank {got:?}"));
    }
    Ok(())
}

/// 5.2: reputation and its change; a monster's is open.
pub fn reputation() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "reputation",
        "eq(actor:character_reputation(), 10, 'the player')\n\
         actor:change_character_reputation(actor:character_reputation() + 5)\n\
         eq(actor:character_reputation(), 25, 'twice the old plus the reward')\n\
         eq(item:character_reputation(), 0, 'not an owner')\n\
         dog:change_character_reputation(5)\n\
         raises(function() return dog:character_reputation() end, '#1198', 'a monster')",
    )?;
    let got = t.simulation_values(PLAYER).map(|v| v.2);
    if got != Some(25) {
        return Err(format!("the player's simulation reputation {got:?}"));
    }
    Ok(())
}

/// 5.3: the community's name; its change with the team; the team alone
/// when the condition of 5.4 fails; an unknown name and a position outside
/// the list are fatal.
pub fn community() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "community",
        "local function tsg(o) return o:team() .. ' ' .. o:squad() .. ' ' .. o:group() end\n\
         eq(stalker:character_community(), 'stalker', 'the stalker')\n\
         eq(trader:character_community(), 'trader', 'the trader')\n\
         eq(actor:character_community(), 'actor', 'the player')\n\
         eq(item:character_community(), nil, 'not an owner')\n\
         raises(function() return dog:character_community() end, '#1198', 'a monster')\n\
         stalker:set_character_community('bandit', 4, 5)\n\
         eq(stalker:character_community(), 'bandit', 'set')\n\
         eq(tsg(stalker), '0 4 5', \"the bandit's number, squad and group\")\n\
         trader:set_character_community('stalker', 1, 1)\n\
         eq(trader:character_community(), 'stalker', \"the trader's\")\n\
         eq(tsg(trader), '0 0 0', 'the trader is no fighter')\n\
         dog:set_character_community('stalker', 7, 7)\n\
         eq(tsg(dog), '5 7 7', 'the team changes without the community')\n\
         item:set_character_community('stalker', 7, 7)",
    )?;
    let got = t.simulation_values(STALKER).map(|v| v.0);
    if got != Some(3) {
        return Err(format!("the stalker's simulation community {got:?}"));
    }
    fatal(
        "stalker:set_character_community('nobody', 0, 0)",
        "item not found, id nobody",
    )?;
    let mut t = state()?;
    if let Some(c) =
        (t.objects.borrow_mut().get_mut(TRADER)).and_then(|o| o.relations.character.as_mut())
    {
        c.community = 9;
    }
    let code = format!("{PRELUDE}return trader:character_community()");
    match t.s.exec("=fatal", code.as_bytes()) {
        Err(e) if e.message.contains("item by index not found") => Ok(()),
        other => Err(format!("a position outside the list: {other:?}")),
    }
}

/// 6: a stalker's flags, 0 at spawn; on another object fatal.
pub fn relation_flags() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "relation flags",
        "eq(stalker:get_actor_relation_flags():get(), 0, 'at spawn')\n\
         local f = flags32() f:assign(6)\n\
         stalker:set_actor_relation_flags(f)\n\
         f:assign(1)\n\
         eq(stalker:get_actor_relation_flags():get(), 6, 'set, a copy')",
    )?;
    fatal("return dog:get_actor_relation_flags()", "not a stalker")?;
    fatal("dog:set_actor_relation_flags(flags32())", "not a stalker")
}

/// 7: by community name and object id, clamped to the community limits;
/// an unknown community is fatal.
pub fn relation_registry() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "relation_registry",
        "eq(relation_registry.community_goodwill('bandit', 0), 0, 'none stored')\n\
         relation_registry.set_community_goodwill('bandit', 0, -5000)\n\
         eq(relation_registry.community_goodwill('bandit', 0), -3000, 'the minimum')\n\
         relation_registry.change_community_goodwill('bandit', 0, 3500)\n\
         eq(relation_registry.community_goodwill('bandit', 0), 500, 'changed')\n\
         relation_registry.set_community_goodwill('trader', 999, 5)\n\
         eq(relation_registry.community_goodwill('trader', 999), 5, 'no such object')",
    )?;
    fatal(
        "relation_registry.community_goodwill('nobody', 0)",
        "item not found, id nobody",
    )
}

/// 8.1: the list, sorted by rank, the player's position; 1 without its
/// entry; an entry without a Trader-like part is fatal; equal ranks are
/// open.
pub fn ranking() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "ranking",
        "eq(actor_stats.get_actor_ranking(), 1, 'no entry')\n\
         actor_stats.add_to_ranking(0) actor_stats.add_to_ranking(1) actor_stats.add_to_ranking(2)\n\
         eq(actor_stats.get_actor_ranking(), 2, 'behind 650 and 330')\n\
         actor_stats.remove_from_ranking(1)\n\
         eq(actor_stats.get_actor_ranking(), 1, 'behind 330')\n\
         actor_stats.add_to_ranking(0)\n\
         eq(actor_stats.get_actor_ranking(), 1, 'added again, once')\n\
         actor:set_character_rank(400)\n\
         eq(actor_stats.get_actor_ranking(), 0, 'first')\n\
         trader:set_character_rank(400)\n\
         raises(function() return actor_stats.get_actor_ranking() end, '#1198', 'equal ranks')",
    )?;
    let ids: Vec<u16> = t
        .alife
        .borrow()
        .relations
        .ranking
        .iter()
        .map(|e| e.id)
        .collect();
    if ids != [PLAYER, TRADER] {
        return Err(format!("sorted {ids:?}"));
    }
    fatal(
        "actor_stats.add_to_ranking(6) return actor_stats.get_actor_ranking()",
        "without a Trader-like part",
    )?;
    let mut t = state()?;
    t.s.destroy_object(PLAYER).map_err(|e| e.error.message)?;
    t.lua(
        "no player",
        "raises(function() return actor_stats.get_actor_ranking() end, '#1198', 'no player')",
    )
}

/// 8.2: count and count × points; −1 with a text; `total`.
pub fn points() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "points",
        "eq(actor_stats.get_points('total'), -1, 'no section')\n\
         eq(actor_stats.get_points('empty'), 0, 'made')\n\
         eq(actor_stats.get_points('total'), 0, 'the empty one')\n\
         actor_stats.add_points('kills', 'dog', 1, 10)\n\
         actor_stats.add_points('kills', 'dog', 1, 10)\n\
         eq(actor_stats.get_points('kills'), 40, '4p')\n\
         actor_stats.add_points('quests', 'a', 3, 5)\n\
         eq(actor_stats.get_points('quests'), 45, 'count 3, points 15')\n\
         actor_stats.add_points_str('names', 'x', 'text')\n\
         eq(actor_stats.get_points('names'), -1, 'a text')\n\
         eq(actor_stats.get_points('total'), 85, 'every section but -1')",
    )?;
    let details = (t.alife.borrow().relations.statistics.get(&PLAYER))
        .and_then(|s| s.iter().find(|s| s.key == b"kills").cloned())
        .map(|s| (s.details[0].count, s.details[0].points));
    if details != Some((2, 20)) {
        return Err(format!("the detail {details:?}"));
    }
    let mut t = state()?;
    t.s.destroy_object(PLAYER).map_err(|e| e.error.message)?;
    let code = "actor_stats.add_points('kills', 'dog', 1, 10)";
    match t.s.exec("=fatal", code.as_bytes()) {
        Err(e) if e.message.contains("there is no player") => Ok(()),
        other => Err(format!("no player: {other:?}")),
    }
}

/// 9: the storyline reward's three steps, and `dialogs_military`'s
/// goodwill below the minimum.
pub fn worked_example() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "worked example",
        "local a = actor\n\
         a:change_character_reputation(a:character_reputation() + 5)\n\
         eq(a:character_reputation(), 25, 'step 1: 2p + r')\n\
         relation_registry.change_community_goodwill('stalker', a:id(), 1500)\n\
         eq(relation_registry.community_goodwill('stalker', a:id()), 1000, 'step 2, clamped')\n\
         eq(stalker:general_goodwill(a), 1033, 'the attitude follows')\n\
         eq(stalker:relation(a), 0, 'now a friend')\n\
         actor_stats.add_points('quests', 'task', 1, 250)\n\
         a:set_character_rank(a:character_rank() + 250)\n\
         eq(actor_stats.get_points('quests'), 250, 'step 3: the points')\n\
         eq(a:character_rank(), 350, 'step 3: the rank')\n\
         eq(stalker:general_goodwill(a), 1003, 'experienced: the rank term 0')\n\
         relation_registry.set_community_goodwill('bandit', a:id(), -5000)\n\
         eq(relation_registry.community_goodwill('bandit', a:id()), -3000, 'dialogs_military')",
    )
}
