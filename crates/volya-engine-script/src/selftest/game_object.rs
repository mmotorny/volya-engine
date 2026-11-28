// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-game-object-basics.md on every target, one
//! check per section, over [`GameObjects`] holding objects of our own.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;

use super::ensure;
use super::files::{FSGAME, Tree};
use super::services::run;
use crate::classes::EnginePart;
use crate::files::FileSystem;
use crate::game_object::members::CONSTANTS;
use crate::game_object::{
    Anomaly, Event, GameObjects, HitEvent, Kinds, ObjectClass, Physics, Restrictor, SpawnData,
    UseAction, Visual, client_classes,
};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::{ObjectFactory, class_id};
use crate::services::EngineServices;
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 the handle and kinds", handle),
    ("3 identity", identity),
    ("4 place", place),
    ("5 condition", condition),
    ("6 zones, restrictors, use and visibility", zones),
    ("7 trade parameters and names", trade),
    ("8 constants", constants),
    ("9 hit", hit),
    ("10 CGameObject, client classes, explosive", engine_classes),
    ("11 unregistered names", unregistered),
    ("12 the door scheme", door),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("game object {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

pub(super) const STALKER: u16 = 10;
pub(super) const PLAYER: u16 = 0;
pub(super) const RIFLE: u16 = 20;
pub(super) const ZONE: u16 = 30;
pub(super) const BLOODSUCKER: u16 = 40;
pub(super) const SCRIPT_ZONE: u16 = 50;
pub(super) const TRADER: u16 = 60;

/// A test state: the scripts, the engine objects, the services.
pub(super) struct State {
    pub s: Scripts,
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
}

pub(super) fn data(id: u16, name: &str, section: &str, class: &str) -> SpawnData {
    SpawnData {
        id,
        name: name.as_bytes().to_vec(),
        section: section.as_bytes().to_vec(),
        class_id: class_id(class.as_bytes()),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: None,
    }
}

/// A state with the services, configuration and files and game object
/// basics registered, `_G` loaded, a global `obj(id)` giving handles, and
/// objects of several kinds.
pub(super) fn state() -> Result<State, String> {
    let tree = Tree(
        [
            ("gamedata/scripts/_g.script", ""),
            ("gamedata/config/inc/part.ltx", "[part]\r\ny = 2\r\n"),
        ]
        .iter()
        .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
        .collect(),
    );
    let tree = Rc::new(tree);
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(|e| e.message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(b"[sys]\r\n").map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(|e| e.message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .map_err(|e| e.message)?;
    let shared = Rc::clone(&objects);
    s.lua_mut()
        .register_object_function("obj", move |args| {
            let Some(Value::Number(id)) = args.first() else {
                return Ok(None);
            };
            Ok(shared.borrow().get(*id as u16).and_then(|o| o.handle()))
        })
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;

    let mut stalker = data(STALKER, "wolf", "stalker", "AI_STL_S");
    stalker.story_id = 5;
    stalker.custom_data = b"[logic]\r\nactive = walker\r\n#include \"inc\\part.ltx\"\r\n".to_vec();
    stalker.placement = [
        [0.0, 0.0, -1.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [1.0, 2.0, 3.0, 1.0],
    ];
    stalker.level_vertex = 7;
    stalker.game_vertex = 9;
    stalker.health = Some(0.5);
    let mut rifle = data(RIFLE, "", "wpn_ak74", "WP_AK74");
    rifle.parent = Some(STALKER);
    let spawned: [(ObjectClass, SpawnData); 7] = [
        (
            ObjectClass::Player,
            data(PLAYER, "actor", "actor", "O_ACTOR"),
        ),
        (ObjectClass::Named("CAI_Stalker"), stalker),
        (ObjectClass::Named("CWeaponAK74"), rifle),
        (
            ObjectClass::Named("CMincer"),
            data(ZONE, "mincer", "zone_mincer", "ZS_MINCE"),
        ),
        (
            ObjectClass::Named("CAI_Bloodsucker"),
            data(BLOODSUCKER, "sucker", "bloodsucker_weak", "SM_BLOOD"),
        ),
        (
            ObjectClass::Named("ce_script_zone"),
            data(SCRIPT_ZONE, "zone", "script_zone", "SCRIPTZN"),
        ),
        (
            ObjectClass::Named("CAI_Trader"),
            data(TRADER, "trader", "trader", "AI_TRADE"),
        ),
    ];
    for (class, data) in spawned {
        s.spawn_engine_object(class, data)
            .map_err(|e| e.to_string())?;
    }
    Ok(State {
        s,
        objects,
        services,
    })
}

/// `code` must stop the state with a fatal error whose report includes
/// `row`; `prepare` sets the objects up first.
fn fatal(code: &str, row: &str, prepare: impl FnOnce(&mut GameObjects)) -> Result<(), String> {
    let mut t = state()?;
    prepare(&mut t.objects.borrow_mut());
    let result =
        t.s.exec("=fatal", format!("pcall(function() {code} end)").as_bytes());
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: {result:?}, not fatal"),
    )?;
    let log = t.services.borrow().console.log().join("\n");
    ensure(log.contains(row), || {
        format!("{code}: no {row:?} in the log:\n{log}")
    })
}

/// The 46 functions of 3–7.
const FUNCTIONS: &str = r#"{"id", "name", "section", "clsid", "story_id", "spawn_ini", "parent",
  "position", "direction", "center", "bone_position", "level_vertex_id", "game_vertex_id",
  "level_vertex_light", "set_actor_position", "set_actor_direction", "alive", "max_health",
  "death_time", "kill", "get_bleeding", "who_hit_name", "who_hit_section_name", "condition",
  "set_condition", "cost", "mass", "critically_wounded", "get_corpse", "is_body_turning",
  "wounded", "hit", "enable_anomaly", "disable_anomaly", "get_anomaly_power",
  "set_anomaly_power", "inside", "set_nonscript_usable", "active_zone_contact",
  "set_invisible", "set_manual_invisibility", "sell_condition", "buy_condition",
  "show_condition", "profile_name", "character_name"}"#;

/// Section 1 (with 10.3's classes): what is registered.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    let mut clients = String::new();
    for name in client_classes() {
        clients.push_str(&format!("{name:?}, "));
    }
    run(
        &mut t.s,
        "1",
        &format!(
            r#"
local functions = {FUNCTIONS}
eq(#functions, 46, "46 functions")
local o = obj({STALKER})
for _, m in ipairs(functions) do eq(type(o[m]), "function", m) end
eq(type(o.invulnerable), "function", "the unlisted invulnerable")
for _, p in ipairs({{"health", "psy_health", "power", "radiation", "morale"}}) do
  eq(type(o[p]), "number", p)
end
local class_mt = getmetatable(game_object)
for _, c in ipairs({{"game_object", "hit", "CGameObject", "explosive", {clients}
    "ce_script_zone", "ce_smart_zone"}}) do
  eq(getmetatable(_G[c]) == class_mt, true, c .. " is a class")
end
raises(function() return game_object() end, "", "game_object has no constructor")
raises(function() return explosive() end, "", "explosive has no constructor")
"#
        ),
    )?;
    ensure(client_classes().count() == 60, || {
        format!("{} client classes", client_classes().count())
    })
}

/// Section 2: one handle per object, freed with it; the kinds (2.2) and
/// the fallbacks on the wrong kind (2.3).
pub fn handle() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "2",
        &format!(
            r#"
eq(rawequal(obj({STALKER}), obj({STALKER})), true, "the same handle")
eq(rawequal(obj({STALKER}), obj({RIFLE})), false, "a handle per object")
held = obj({RIFLE})
-- 2.3 on the wrong kind.
local z = obj({ZONE})
eq(z:alive(), false, "alive")
eq(z:max_health(), -1, "max_health")
eq(z:death_time(), 0, "death_time")
eq(z:get_bleeding(), -1, "get_bleeding")
eq(z:who_hit_name(), nil, "who_hit_name")
eq(z:condition(), 0, "condition")
eq(z:cost(), 0, "cost")
eq(z:mass(), -1, "mass")
eq(z:invulnerable(), false, "invulnerable")
eq(z:critically_wounded(), false, "critically_wounded")
eq(z:get_corpse(), nil, "get_corpse")
eq(z:is_body_turning(), false, "is_body_turning")
eq(z:wounded(), false, "wounded")
eq(z.health, -1, "health")
z.power = 3
eq(z.power, -1, "power assigned to an object that isn't living")
eq(z:profile_name(), nil, "profile_name")
eq(z:character_name(), nil, "character_name")
eq(obj({STALKER}):inside(vector():set(0, 0, 0)), false, "inside of a non-restrictor")
eq(obj({STALKER}):active_zone_contact(1), false, "active_zone_contact")
"#
        ),
    )?;
    ensure(t.s.destroy_object(RIFLE) == Ok(true), || "destroy".into())?;
    run(
        &mut t.s,
        "2 destroyed",
        &format!(
            r#"
eq(obj({RIFLE}), nil, "no handle for a destroyed object")
raises(function() return held:id() end, "no engine part", "a member on a freed handle")
"#
        ),
    )?;
    ensure(
        t.objects
            .borrow()
            .of_handle(EnginePart::from_value(0.0))
            .is_none(),
        || "no handle 0".into(),
    )?;
    // The kinds (2.2): counts by kind over the client classes.
    let classes: Vec<ObjectClass> = client_classes()
        .chain(["ce_script_zone", "ce_smart_zone"])
        .map(ObjectClass::Named)
        .collect();
    let count = |k: Kinds| classes.iter().filter(|c| c.kinds().has(k)).count();
    let counts = [
        count(Kinds::ENTITY),
        count(Kinds::LIVING),
        count(Kinds::INVENTORY_OWNER),
        count(Kinds::FIGHTER),
        count(Kinds::STALKER),
        count(Kinds::ITEM),
        count(Kinds::PHYSICAL),
        count(Kinds::ANOMALOUS_ZONE),
        count(Kinds::RESTRICTOR),
        count(Kinds::PLAYER),
    ];
    ensure(counts == [19, 19, 19, 18, 1, 36, 56, 3, 6, 0], || {
        format!("kind counts {counts:?}")
    })?;
    ensure(
        ObjectClass::Named("CPhysicObject").kinds() == Kinds::PHYSICAL,
        || "CPhysicObject is physical only".into(),
    )?;
    let player = ObjectClass::Player.kinds();
    ensure(
        player == Kinds::OF_PLAYER && !player.has(Kinds::FIGHTER) && !player.has(Kinds::ITEM),
        || format!("the player's kinds {player:?}"),
    )?;
    let car = ObjectClass::Named("CCar").kinds();
    ensure(
        car.has(Kinds::ENTITY) && car.has(Kinds::PHYSICAL) && !car.has(Kinds::LIVING),
        || format!("CCar's kinds {car:?}"),
    )
}

/// Section 3.
pub fn identity() -> Result<(), String> {
    let mut t = state()?;
    let number = |text: &[u8]| ObjectFactory::new().number(class_id(text));
    let stalker = number(b"AI_STL_S");
    run(
        &mut t.s,
        "3",
        &format!(
            r#"
local o, rifle = obj({STALKER}), obj({RIFLE})
eq(o:id(), {STALKER}, "id")
eq(o:name(), "wolf", "name")
eq(rifle:name(), "wpn_ak74", "an empty name is the section")
eq(o:section(), "stalker", "section")
eq(o:clsid(), {stalker}, "clsid: the class id's number")
eq(o:story_id(), 5, "story_id")
eq(rifle:story_id(), -1, "no story id")
local ini = o:spawn_ini()
eq(ini:r_string("logic", "active"), "walker", "spawn_ini reads the custom data")
eq(ini:r_string("part", "y"), "2", "its includes against $game_config$")
eq(rawequal(o:spawn_ini(), ini), true, "the same object every call")
eq(rifle:spawn_ini(), nil, "nil for empty custom data")
eq(rawequal(rifle:parent(), o), true, "parent: the handle of the owner")
eq(o:parent(), nil, "no parent")
"#
        ),
    )
}

/// Section 4.
pub fn place() -> Result<(), String> {
    let mut t = state()?;
    {
        let mut objects = t.objects.borrow_mut();
        objects.level_vertex_lights = vec![0xF0, 0x7A, 0x0F];
        let stalker = objects.get_mut(STALKER).expect("spawned");
        let mut head = transform([0.0, 1.5, 0.0], [0.0; 3]);
        head[3] = [0.0, 1.5, 0.25, 1.0];
        stalker.visual = Some(Visual {
            part: None,
            centre: [0.0, 1.0, 0.0],
            bones: vec![
                (b"root".to_vec(), transform([0.0; 3], [0.0; 3])),
                (b"head".to_vec(), head),
            ],
            root: 0,
        });
    }
    run(
        &mut t.s,
        "4",
        &format!(
            r#"
local function v3(v) return v.x .. "," .. v.y .. "," .. v.z end
local o = obj({STALKER})
eq(v3(o:position()), "1,2,3", "position")
eq(rawequal(o:position(), o:position()), false, "a new vector each call")
eq(v3(o:direction()), "1,0,0", "direction: the k row")
eq(v3(o:center()), "1,3,3", "center: c + y j")
eq(v3(o:bone_position("head")), "1.25,3.5,3", "bone_position")
eq(v3(o:bone_position("")), "1,2,3", "the root bone")
eq(o:level_vertex_id(), 7, "level_vertex_id")
eq(o:game_vertex_id(), 9, "game_vertex_id")
eq(o:level_vertex_light(0), 1, "light: upper 4 bits / 15")
eq(math.abs(o:level_vertex_light(1) - 7 / 15) < 1e-6, true, "light 7/15")
eq(obj({ZONE}):level_vertex_light(2), 0, "light 0, the object unused")
o:set_actor_position(vector():set(5, 5, 5))
eq(v3(o:position()), "1,2,3", "set_actor_position: player only")
local p = obj({PLAYER})
p:set_actor_position(vector():set(5, 6, 7))
eq(v3(p:position()), "5,6,7", "the player placed")
p:set_actor_direction(0.5)
o:set_actor_direction(0.5)
"#
        ),
    )?;
    let events = t.objects.borrow_mut().take_events();
    let mut placement = transform([0.0; 3], [0.0; 3]);
    placement[3] = [5.0, 6.0, 7.0, 1.0];
    ensure(
        events
            == [
                Event::ForcePlacement {
                    object: PLAYER,
                    placement,
                },
                Event::CameraAngles {
                    object: PLAYER,
                    angles: [0.5, 0.0, 0.0],
                },
            ],
        || format!("events {events:?}"),
    )?;
    fatal(
        &format!("obj({RIFLE}):center()"),
        "the object has no visual",
        |_| {},
    )?;
    fatal(
        &format!("obj({STALKER}):level_vertex_light(3)"),
        "no level vertex 3",
        |_| {},
    )
}

/// Section 5.
pub fn condition() -> Result<(), String> {
    let mut t = state()?;
    t.services.borrow_mut().device.frame(4321, 0.0, false);
    {
        let mut objects = t.objects.borrow_mut();
        objects.game_time = 77;
        let stalker = objects.get_mut(STALKER).expect("spawned");
        let conditions = stalker.conditions.as_mut().expect("living");
        conditions.wounds = vec![[1.0; 11], [0.5; 11]];
        conditions.who_hit = Some(PLAYER);
        let fighter = stalker.fighter.as_mut().expect("fighter");
        fighter.critically_wounded = true;
        fighter.corpse = Some(BLOODSUCKER);
        fighter.body_heading = (0.0, 0.00001);
        stalker.physics = Some(Physics { mass: 80.0 });
        let item = objects
            .get_mut(RIFLE)
            .expect("spawned")
            .item
            .as_mut()
            .expect("item");
        item.condition = 0.5;
        item.cost = 1500;
        let sucker = objects.get_mut(BLOODSUCKER).expect("spawned");
        sucker.conditions.as_mut().expect("living").may_be_harmed = false;
    }
    run(
        &mut t.s,
        "5",
        &format!(
            r#"
local o, rifle, sucker = obj({STALKER}), obj({RIFLE}), obj({BLOODSUCKER})
-- 5.1
eq(o.health, 0.5, "health")
eq(o.power, 1, "power")
o.power = 2
eq(o.power, 3, "assigning adds")
o.radiation = -0.25
eq(o.radiation, -0.25, "not clamped")
o.health = -0.25
eq(o.health, 0.25, "a negative health while it may be harmed")
sucker.health = -0.5
eq(sucker.health, 1, "not while it may not")
sucker.health = 0.5
eq(sucker.health, 1.5, "a positive health always")
-- 5.2
eq(o:alive(), true, "alive")
eq(o:max_health(), 1, "max_health")
eq(o:death_time(), 0, "not dead")
o:kill(obj({PLAYER}))
eq(o:death_time(), 4321, "the device's time")
o:kill(nil)
sucker:kill(nil)
sucker:kill(nil)
-- 5.3
eq(o:get_bleeding(), 8.25, "the mean of the wounds' totals")
eq(sucker:get_bleeding(), 0, "no wound")
eq(o:who_hit_name(), "actor", "who_hit_name")
eq(o:who_hit_section_name(), "actor", "who_hit_section_name")
eq(sucker:who_hit_name(), nil, "none recorded")
-- 5.4
eq(rifle:condition(), 0.5, "condition")
rifle:set_condition(2)
eq(rifle:condition(), 1, "set_condition clamps")
rifle:set_condition(-1)
eq(rifle:condition(), 0, "to 0 … 1")
eq(rifle:cost(), 1500, "cost")
eq(o:mass(), 80, "mass of a physical object")
eq(rifle:mass(), 0, "an item's physics part")
-- 5.5
eq(o:invulnerable(), false, "invulnerable")
o:invulnerable(true)
eq(o:invulnerable(), true, "invulnerable(on)")
eq(o:critically_wounded(), true, "critically_wounded")
eq(rawequal(o:get_corpse(), sucker), true, "get_corpse")
eq(o:is_body_turning(), true, "a difference of 0.00001 turns")
eq(sucker:is_body_turning(), false, "no difference")
eq(o:wounded(), false, "wounded")
o:wounded(true)
o:wounded(true)
eq(o:wounded(), true, "wounded(on)")
sucker:wounded(true)
eq(sucker:wounded(), false, "a stalker's only")
"#
        ),
    )?;
    let events = t.objects.borrow_mut().take_events();
    ensure(
        events
            == [
                Event::Death {
                    object: STALKER,
                    killer: PLAYER,
                },
                Event::Death {
                    object: BLOODSUCKER,
                    killer: BLOODSUCKER,
                },
                Event::Wounded {
                    object: STALKER,
                    wounded: true,
                },
            ],
        || format!("events {events:?}"),
    )?;
    {
        let objects = t.objects.borrow();
        let e = objects.get(STALKER).and_then(|o| o.entity.clone());
        ensure(
            e.as_ref()
                .is_some_and(|e| e.killer == Some(PLAYER) && e.death_game_time == 77),
            || format!("the killer and game time {e:?}"),
        )?;
    }
    // A self-kill after a killer was recorded, and a destroying entity.
    {
        let mut objects = t.objects.borrow_mut();
        let trader = objects.get_mut(TRADER).expect("spawned");
        trader.entity.as_mut().expect("entity").killer = Some(PLAYER);
        let player = objects.get_mut(PLAYER).expect("spawned");
        player.destroying = true;
    }
    run(
        &mut t.s,
        "5 kill",
        &format!(
            r#"
obj({TRADER}):kill(nil)
eq(obj({TRADER}):death_time(), 0, "a self-kill after a killer: nothing")
obj({PLAYER}):kill(nil)
eq(obj({PLAYER}):death_time(), 4321, "marked for destruction: recorded")
"#
        ),
    )?;
    let events = t.objects.borrow_mut().take_events();
    ensure(events.is_empty(), || format!("no death event: {events:?}"))
}

/// Section 5.6 and 9: the hit event.
pub fn hit() -> Result<(), String> {
    let mut t = state()?;
    t.objects.borrow_mut().server_time = 900;
    t.objects
        .borrow_mut()
        .get_mut(STALKER)
        .expect("spawned")
        .visual = Some(Visual {
        bones: vec![
            (b"root".to_vec(), transform([0.0; 3], [0.0; 3])),
            (b"head".to_vec(), transform([0.0; 3], [0.0; 3])),
        ],
        ..Visual::default()
    });
    run(
        &mut t.s,
        "9",
        &format!(
            r#"
local h = hit()
eq(h.power, 100, "power")
eq(h.impulse, 100, "impulse")
eq(h.type, hit.wound, "type wound")
eq(h.direction.x .. h.direction.y .. h.direction.z, "100", "direction (1, 0, 0)")
eq(rawequal(h.direction, h.direction), false, "a new vector each read")
eq(h.draftsman, nil, "no draftsman")
local consts = {{burn = 0, shock = 1, strike = 2, wound = 3, radiation = 4, telepatic = 5,
  chemical_burn = 6, explosion = 7, fire_wound = 8, dummy = 11}}
for k, v in pairs(consts) do eq(hit[k], v, "hit." .. k) end
h.power = 2
h.impulse = 3
h.direction = vector():set(0, 0, 1)
h.draftsman = obj({PLAYER})
eq(rawequal(h.draftsman, obj({PLAYER})), true, "draftsman")
h:bone("head")
h.type = hit.fire_wound
local copy = hit(h)
h.power = 5
eq(copy.power, 2, "a copy")
obj({STALKER}):hit(copy)
local plain = hit()
plain.draftsman = obj({STALKER})
obj({RIFLE}):hit(plain)
"#
        ),
    )?;
    let events = t.objects.borrow_mut().take_events();
    let expected = [
        Event::Hit(HitEvent {
            time: 900,
            target: STALKER,
            power: 2.0,
            direction: [0.0, 0.0, 1.0],
            hitter: PLAYER,
            weapon: 0,
            bone: 1,
            position: [0.0; 3],
            impulse: 3.0,
            hit_type: 8,
            armour_piercing: Some(0.0),
            aimed_bullet: false,
        }),
        Event::Hit(HitEvent {
            time: 900,
            target: RIFLE,
            power: 100.0,
            direction: [1.0, 0.0, 0.0],
            hitter: STALKER,
            weapon: 0,
            bone: 0,
            position: [0.0; 3],
            impulse: 100.0,
            hit_type: 3,
            armour_piercing: None,
            aimed_bullet: false,
        }),
    ];
    ensure(events == expected, || format!("hit events {events:?}"))?;
    fatal(
        &format!("obj({STALKER}):hit(hit())"),
        "the hit has no draftsman",
        |_| {},
    )?;
    // TODO(#1001): gap 3, the simplest reading.
    fatal(
        &format!("local h = hit() h.draftsman = obj({PLAYER}) h:bone('x') obj({RIFLE}):hit(h)"),
        "the object has no visual",
        |_| {},
    )
}

/// Section 6.
pub fn zones() -> Result<(), String> {
    let mut t = state()?;
    {
        let mut objects = t.objects.borrow_mut();
        let zone = objects.get_mut(ZONE).expect("spawned");
        zone.anomaly = Some(Anomaly {
            state: 1,
            power: 1.5,
        });
        let mut box_planes = [([1.0, 0.0, 0.0], -1.0); 6];
        // The unit cube 0 … 1 on every axis, offset 10 on x.
        box_planes[0] = ([1.0, 0.0, 0.0], -11.0);
        box_planes[1] = ([-1.0, 0.0, 0.0], 10.0);
        box_planes[2] = ([0.0, 1.0, 0.0], -1.0);
        box_planes[3] = ([0.0, -1.0, 0.0], 0.0);
        box_planes[4] = ([0.0, 0.0, 1.0], -1.0);
        box_planes[5] = ([0.0, 0.0, -1.0], 0.0);
        zone.restrictor = Some(Restrictor {
            bound: ([5.0, 0.0, 0.0], 10.0),
            spheres: vec![([0.0, 0.0, 0.0], 1.0)],
            boxes: vec![box_planes],
        });
        objects
            .get_mut(SCRIPT_ZONE)
            .expect("spawned")
            .contacts
            .as_mut()
            .expect("a script zone")
            .push(STALKER);
    }
    run(
        &mut t.s,
        "6",
        &format!(
            r#"
local z = obj({ZONE})
eq(z:get_anomaly_power(), 1.5, "get_anomaly_power")
z:set_anomaly_power(2.5)
eq(z:get_anomaly_power(), 2.5, "set_anomaly_power")
local p = vector()
eq(z:inside(p:set(0.5, 0, 0)), true, "in the sphere")
eq(z:inside(p:set(1.0005, 0, 0)), true, "within the default radius")
eq(z:inside(p:set(1.01, 0, 0)), false, "outside it")
eq(z:inside(p:set(1.5, 0, 0), 0.6), true, "with a radius")
eq(z:inside(p:set(10.5, 0.5, 0.5)), true, "in the box")
eq(z:inside(p:set(14.9, 0.5, 0.5)), false, "outside the box")
eq(z:inside(p:set(16, 0, 0)), false, "outside the bounding sphere")
eq(obj({SCRIPT_ZONE}):active_zone_contact({STALKER}), true, "a contact")
eq(obj({SCRIPT_ZONE}):active_zone_contact({RIFLE}), false, "not a contact")
eq(obj({SCRIPT_ZONE}):inside(p:set(0, 0, 0)), false, "an empty shape")
z:disable_anomaly()
obj({BLOODSUCKER}):set_invisible(true)
obj({STALKER}):set_invisible(true)
obj({BLOODSUCKER}):set_manual_invisibility(true)
obj({RIFLE}):set_nonscript_usable(false)
"#
        ),
    )?;
    {
        let objects = t.objects.borrow();
        let zone = objects.get(ZONE).and_then(|o| o.anomaly);
        ensure(zone.is_some_and(|a| a.state == 4), || {
            format!("the zone's state {zone:?}")
        })?;
        ensure(
            objects.get(BLOODSUCKER).and_then(|o| o.invisible) == Some(true),
            || "the bloodsucker's flag".into(),
        )?;
    }
    t.s.exec(
        "=enable",
        format!("obj({ZONE}):enable_anomaly()").as_bytes(),
    )
    .map_err(|e| e.message)?;
    ensure(
        t.objects
            .borrow()
            .get(ZONE)
            .and_then(|o| o.anomaly)
            .map(|a| a.state)
            == Some(0),
        || "enable_anomaly: state 0".into(),
    )?;
    let events = t.objects.borrow_mut().take_events();
    ensure(
        events
            == [Event::Visibility {
                object: BLOODSUCKER,
                hidden: true,
            }],
        || format!("events {events:?}"),
    )?;
    // 6.3: the use flag and the player's use action, the scripted use (its
    // callback, spec O) first.
    run(
        &mut t.s,
        "6.3",
        &format!(
            r#"
used = {{}}
for _, id in ipairs({{{RIFLE}, {STALKER}}}) do
  obj(id):set_callback(callback.use_object, function(o, who)
    used[#used + 1] = {{o:id(), who:id()}}
  end)
end
"#
        ),
    )?;
    let used = t.s.player_use(RIFLE, PLAYER).map_err(|e| e.to_string())?;
    let other = t.s.player_use(STALKER, PLAYER).map_err(|e| e.to_string())?;
    ensure(
        used == UseAction::ScriptedOnly && other == UseAction::EngineHandling,
        || format!("use actions {used:?} {other:?}"),
    )?;
    run(
        &mut t.s,
        "6.3",
        &format!(
            r#"
eq(#used, 2, "both used")
eq(used[1][1], {RIFLE}, "the rifle") eq(used[1][2], {PLAYER}, "by the player")
eq(used[2][1], {STALKER}, "the stalker") eq(used[2][2], {PLAYER}, "by the player")
"#
        ),
    )?;
    fatal(
        &format!("obj({STALKER}):enable_anomaly()"),
        "the object is not an anomalous zone",
        |_| {},
    )
}

/// Section 7.
pub fn trade() -> Result<(), String> {
    let mut t = state()?;
    {
        let mut objects = t.objects.borrow_mut();
        let owner = objects
            .get_mut(TRADER)
            .and_then(|o| o.owner.as_mut())
            .expect("an inventory owner");
        owner.profile = Some(b"esc_trader".to_vec());
        owner.character_name = b"Sidorovich".to_vec();
    }
    run(
        &mut t.s,
        "7",
        &format!(
            r#"
local ini = create_ini_file("[sell]\r\nwpn_pm = 0.5, 1.5\r\nlisted\r\n[show]\r\nwpn_pm\r\nbread = 1\r\n")
local tr = obj({TRADER})
tr:sell_condition(ini, "sell")
tr:buy_condition(0.25, 0.75)
tr:show_condition(ini, "show")
obj({ZONE}):sell_condition(ini, "absent")
eq(tr:profile_name(), "esc_trader", "profile_name")
eq(tr:character_name(), "Sidorovich", "character_name")
eq(obj({STALKER}):profile_name(), nil, "no profile")
"#
        ),
    )?;
    {
        let objects = t.objects.borrow();
        let owner = objects.get(TRADER).and_then(|o| o.owner.clone());
        let owner = owner.ok_or("no owner")?;
        ensure(
            owner.sell.items == [(b"wpn_pm".to_vec(), 0.5, 1.5)]
                && owner.buy.default == (0.25, 0.75)
                && owner.show == [b"wpn_pm".to_vec()],
            || format!("the owner's factors {owner:?}"),
        )?;
    }
    fatal(
        &format!("obj({TRADER}):sell_condition(create_ini_file(''), 'absent')"),
        "cannot find section absent",
        |_| {},
    )
}

/// Section 8.
pub fn constants() -> Result<(), String> {
    let mut t = state()?;
    let mut code = String::new();
    for (name, value) in CONSTANTS {
        code.push_str(&format!("eq(game_object.{name}, {value}, \"{name}\")\n"));
    }
    run(&mut t.s, "8", &code)
}

/// Section 10.
pub fn engine_classes() -> Result<(), String> {
    let mut t = state()?;
    let results =
        t.s.exec(
            "=classes",
            br#"
class "switch" (CGameObject)
function switch:__init() super() end
function switch:net_Spawn(data)
  spawned_with = data
  return CGameObject.net_Spawn(self, data)
end
function switch:use(who) used_by = who return false end
function switch:net_Export(p) exported = p end
function switch:Load() loaded = true end
function switch:net_Destroy() destroyed = true end
sw = switch()
plain = CPhysicObject()
return sw, plain
"#,
        )
        .map_err(|e| e.message)?;
    let [Value::Object(sw), Value::Object(plain)] = results.as_slice() else {
        return Err(format!("the clients {results:?}"));
    };
    let mut door = data(70, "door", "physic_object", "O_PHYS_S");
    door.custom_data = b"[logic]\r\ncfg = door.ltx\r\n".to_vec();
    let switched =
        t.s.spawn(*sw, data(80, "switch", "trigger", "O_SWITCH"))
            .map_err(|e| e.to_string())?;
    let spawned = t.s.spawn(*plain, door.clone()).map_err(|e| e.to_string())?;
    let again = t.s.spawn(*plain, door).map_err(|e| e.to_string())?;
    ensure(switched && spawned && !again, || {
        format!("spawns {switched} {spawned} {again}")
    })?;
    let visual = t.s.new_visual(None).map_err(|e| e.message)?;
    t.objects.borrow_mut().get_mut(70).expect("spawned").visual = Some(Visual {
        part: Some(visual),
        ..Visual::default()
    });
    let used = t.s.player_use(80, PLAYER).map_err(|e| e.to_string())?;
    t.s.net_call(
        80,
        crate::game_object::NetCall::Export,
        &[Value::Number(3.0)],
    )
    .map_err(|e| e.to_string())?;
    t.s.net_call(70, crate::game_object::NetCall::Import, &[])
        .map_err(|e| e.to_string())?;
    ensure(used == UseAction::EngineHandling, || format!("{used:?}"))?;
    run(
        &mut t.s,
        "10",
        &format!(
            r#"
eq(type(spawned_with), "number", "net_Spawn's override gets the data")
eq(rawequal(used_by, obj({PLAYER})), true, "use's override gets the user")
eq(exported, 3, "net_Export's override")
eq(loaded, nil, "Load is never called")
eq(destroyed, nil, "net_Destroy is never called")
for _, m in ipairs({{"_construct", "Visual", "getEnabled", "getVisible", "net_Spawn",
    "net_Export", "net_Import", "use"}}) do
  eq(type(CGameObject[m]), "function", "CGameObject." .. m)
  eq(type(CPhysicObject[m]), "function", "CPhysicObject." .. m)
end
eq(rawequal(plain:_construct(), plain), true, "_construct returns the object")
eq(plain:getEnabled(), 1, "getEnabled")
eq(plain:getVisible(), 1, "getVisible")
eq(type(plain:Visual()), "userdata", "Visual")
eq(type(plain:Visual().dcast_PKinematicsAnimated), "function", "an IRender_Visual")
eq(sw:Visual(), nil, "no visual")
obj(70):set_callback(callback.use_object, function(o, who) own_use = {{o:id(), who:id()}} end)
eq(plain:use(obj({PLAYER})), true, "use returns true")
eq(obj(70):spawn_ini():r_string("logic", "cfg"), "door.ltx", "spawned with its data")
eq(CPhysicObject():getEnabled(), 0, "a new client object isn't spawned")
eq(type(ce_script_zone():_construct()), "userdata", "ce_script_zone constructs")
eq(type(explosive.explode), "function", "explosive:explode")
raises(function() return CPhysicObject(1) end, "", "one constructor taking no arguments")
"#
        ),
    )?;
    run(
        &mut t.s,
        "10",
        &format!(
            r#"
eq(own_use and own_use[1], 70, "the engine's own use calls the use_object callback")
eq(own_use and own_use[2], {PLAYER}, "with the user")
"#
        ),
    )?;
    ensure(t.s.destroy_object(70) == Ok(true), || "destroy".into())?;
    let unbound =
        t.s.with_part(*plain, |c: &mut crate::game_object::ClientObject| c.object);
    ensure(unbound == Some(None), || {
        format!("the client unbound {unbound:?}")
    })
}

/// Section 11.
pub fn unregistered() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "11",
        &format!(
            r#"
for _, m in ipairs({{"actor_sleep", "is_actor_sleeping", "debug_planner",
    "set_character_reputation", "setVisible", "setEnabled"}}) do
  eq(obj({PLAYER})[m], nil, m)
end
for _, m in ipairs({{"Load", "shedule_Update", "net_Destroy"}}) do
  eq(CGameObject[m], nil, "CGameObject." .. m)
end
raises(function() CGameObject.Load(nil) end, "attempt to call", "a base call of Load")
"#
        ),
    )
}

/// Section 12: the door scheme's calls on a `CPhysicObject` handle.
pub fn door() -> Result<(), String> {
    let mut t = state()?;
    let client =
        t.s.exec("=door", b"door_client = CPhysicObject() return door_client")
            .map_err(|e| e.message)?;
    let [Value::Object(client)] = client.as_slice() else {
        return Err(format!("the client {client:?}"));
    };
    let mut door = data(90, "trader_door", "physic_object", "O_PHYS_S");
    door.custom_data = b"[logic]\r\ncfg = scripts\\door.ltx\r\n".to_vec();
    door.placement[3] = [4.0, 5.0, 6.0, 1.0];
    t.s.spawn(*client, door).map_err(|e| e.to_string())?;
    run(
        &mut t.s,
        "12",
        r#"
local d = obj(90)
eq(d:name(), "trader_door", "name")
eq(d:spawn_ini():section_exist("logic"), true, "spawn_ini")
eq(type(d:clsid()), "number", "clsid")
eq(d:id(), 90, "id")
eq(d:game_vertex_id(), 0, "game_vertex_id")
eq(d:position().y, 5, "position")
d:set_nonscript_usable(false)
"#,
    )?;
    let used = t.s.player_use(90, PLAYER).map_err(|e| e.to_string())?;
    ensure(used == UseAction::ScriptedOnly, || {
        format!("the engine's taking hold is skipped: {used:?}")
    })
}
