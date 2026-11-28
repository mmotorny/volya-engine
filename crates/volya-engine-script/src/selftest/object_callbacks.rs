// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-object-event-callbacks.md on every target, one
//! check per section, over the game object basics' test state.

use super::ensure;
use super::game_object::{PLAYER, RIFLE, SCRIPT_ZONE, STALKER, TRADER, data, state};
use super::services::run;
use crate::ErrorKind;
use crate::Value;
use crate::game_object::UseAction;
use crate::game_object::callbacks::{ActionPart, CALLBACK_IDS, ObjectEvent, PerceivedSound};
use crate::loader::Scripts;

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 the event ids", ids),
    ("3 set_callback", set_callback),
    ("4 the events", events),
    ("5 set_fastcall", fastcall),
    ("6 the level's call list", level_calls),
    ("7, 8 the fighters' callbacks", fighter_callbacks),
    ("9 set_use_callback", use_callback),
    ("10 the trader's door", door),
    ("D 1.2 failures", failures),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("object callbacks {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn err(e: impl ToString) -> String {
    e.to_string()
}

/// Section 1: the names registered.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "1",
        &format!(
            r#"
for _, m in ipairs({{"set_callback", "set_fastcall", "set_enemy_callback",
    "set_patrol_extrapolate_callback"}}) do
  eq(type(obj({PLAYER})[m]), "function", m)
  eq(type(game_object[m]), "function", "game_object." .. m)
end
for _, f in ipairs({{"add_call", "remove_call", "remove_calls_for_object"}}) do
  eq(type(level[f]), "function", "level." .. f)
end
eq(type(callback), "userdata", "callback is a class")
raises(function() return callback() end, "", "callback has no constructor")
eq(__volya_callback_object, nil, "the Rust side is hidden")
"#
        ),
    )
}

/// Section 2: the 34 constants.
pub fn ids() -> Result<(), String> {
    let mut t = state()?;
    ensure(CALLBACK_IDS.len() == 34, || "34 constants".into())?;
    let checks: String = CALLBACK_IDS
        .iter()
        .enumerate()
        .map(|(i, (name, value))| {
            format!("eq(callback.{name}, {value}, \"{name}\") eq({value}, {i}, \"in order\")\n")
        })
        .collect();
    run(&mut t.s, "2", &checks)
}

/// Raises `event` on `receiver`.
fn raise(s: &mut Scripts, receiver: u16, event: ObjectEvent) -> Result<(), String> {
    s.raise(receiver, &event).map_err(err)
}

/// Section 3: each object's table and how the engine calls an entry.
pub fn set_callback() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "3 set",
        &format!(
            r##"
calls = {{}}
local function record(...) calls[#calls + 1] = {{n = select("#", ...), ...}} end
binder = {{}}
function binder:on_death(victim, killer) calls[#calls + 1] = {{self = self, victim, killer}} end
local s = obj({STALKER})
s:set_callback(callback.death, record)
s:set_callback(callback.hit, binder.on_death, binder)
-- Any number is an id; a later call replaces an earlier one.
s:set_callback(99, record)
s:set_callback(callback.use_object, record)
s:set_callback(callback.use_object, function() calls[#calls + 1] = "replaced" end)
s:set_callback(callback.zone_enter, record)
s:set_callback(callback.zone_enter, nil)
obj({RIFLE}):set_callback(callback.on_item_take, record)
obj({RIFLE}):set_callback(callback.on_item_take)
raises(function() s:set_callback("death", record) end, "set_callback", "a text id")
raises(function() s:set_callback(8, 5) end, "set_callback", "a number for f")
raises(function() s.set_callback({{}}, 8, record) end, "game object", "not a handle")
"##
        ),
    )?;
    raise(&mut t.s, STALKER, ObjectEvent::Death { killer: None })?;
    raise(
        &mut t.s,
        STALKER,
        ObjectEvent::Death {
            killer: Some(PLAYER),
        },
    )?;
    raise(&mut t.s, STALKER, ObjectEvent::UseObject { user: None })?;
    raise(&mut t.s, STALKER, ObjectEvent::ZoneEnter { object: None })?;
    raise(&mut t.s, RIFLE, ObjectEvent::OnItemTake { item: None })?;
    raise(
        &mut t.s,
        STALKER,
        ObjectEvent::Hit {
            power: 0.5,
            direction: [1.0, 2.0, 3.0],
            hitter: Some(PLAYER),
            bone: 4.0,
        },
    )?;
    // No such object: nothing.
    raise(&mut t.s, 999, ObjectEvent::Death { killer: None })?;
    run(
        &mut t.s,
        "3 calls",
        &format!(
            r#"
eq(#calls, 4, "calls")
eq(calls[1].n, 2, "own handle and killer") eq(calls[1][1]:id(), {STALKER}, "own handle")
eq(calls[1][2], nil, "no killer")
eq(rawequal(calls[1][1], obj({STALKER})), true, "the receiver's handle")
eq(calls[2][2]:id(), {PLAYER}, "the killer's handle")
eq(calls[3], "replaced", "the replacing function")
eq(rawequal(calls[4].self, binder), true, "the bound value first")
eq(calls[4][1]:id(), {STALKER}, "then the event's arguments")
"#
        ),
    )
}

/// Section 4: the arguments of each event the engine raises.
pub fn events() -> Result<(), String> {
    let mut t = state()?;
    let (task, objective) = {
        let lua = t.s.lua_mut();
        let task = lua
            .exec("=task", b"return vector()")
            .map_err(|e| e.message)?;
        let objective = lua
            .exec("=objective", b"return vector()")
            .map_err(|e| e.message)?;
        match (task.as_slice(), objective.as_slice()) {
            ([Value::Object(a)], [Value::Object(b)]) => (*a, *b),
            other => return Err(format!("stand-ins {other:?}")),
        }
    };
    run(
        &mut t.s,
        "4 set",
        &format!(
            r##"
seen = {{}}
local function describe(v)
  if type(v) == "userdata" and v.id then return "h" .. v:id() end
  if type(v) == "userdata" and v.x then return "v" .. v.x .. "," .. v.y .. "," .. v.z end
  return tostring(v)
end
for id = 0, 33 do
  for _, o in ipairs({{{PLAYER}, {STALKER}}}) do
    obj(o):set_callback(id, function(...)
      local t = {{}}
      for i = 1, select("#", ...) do t[i] = describe((select(i, ...))) end
      seen[#seen + 1] = id .. "@" .. o .. ":" .. table.concat(t, " ")
    end)
  end
end
"##
        ),
    )?;
    use ObjectEvent::*;
    let p = PLAYER;
    let s = STALKER;
    let events: Vec<(u16, ObjectEvent, String)> = vec![
        (s, TradeStart, format!("0@{s}:")),
        (s, TradeStop, format!("1@{s}:")),
        (
            p,
            TradeSellBuyItem {
                item: Some(RIFLE),
                sold: true,
                money: 300.0,
            },
            format!("2@{p}:h{RIFLE} true 300"),
        ),
        (s, TradePerformOperation(1.0, 2.0), format!("3@{s}:1 2")),
        (
            s,
            ZoneEnter {
                object: Some(PLAYER),
            },
            format!("4@{s}:h{s} h{p}"),
        ),
        (s, ZoneExit { object: None }, format!("5@{s}:h{s} nil")),
        (p, LevelBorderExit, format!("6@{p}:h{p}")),
        (p, LevelBorderEnter, format!("7@{p}:h{p}")),
        (s, Death { killer: Some(p) }, format!("8@{s}:h{s} h{p}")),
        (
            s,
            BreakableDeath { last_hitter: None },
            format!("8@{s}:h{s} nil"),
        ),
        (s, PatrolPathInPoint { point: 3 }, format!("9@{s}:h{s} 0 3")),
        (
            p,
            InventoryInfo {
                portion: b"info".to_vec(),
            },
            format!("11@{p}:h{p} info"),
        ),
        (
            p,
            ArticleInfo {
                group: b"g".to_vec(),
                name: b"n".to_vec(),
                article_type: 1.0,
            },
            format!("12@{p}:h{p} g n 1"),
        ),
        (
            p,
            TaskState {
                task,
                objective,
                state: 2.0,
            },
            format!("13@{p}:v0,0,0 v0,0,0 2"),
        ),
        (
            p,
            MapLocationAdded {
                location_type: b"spot".to_vec(),
                object: 7,
            },
            format!("14@{p}:spot 7"),
        ),
        (s, UseObject { user: Some(p) }, format!("15@{s}:h{s} h{p}")),
        (p, UseItem { item: Some(RIFLE) }, format!("15@{p}:h{RIFLE}")),
        (
            s,
            Hit {
                power: 0.25,
                direction: [0.0, 1.0, 0.0],
                hitter: None,
                bone: 2.0,
            },
            format!("16@{s}:h{s} 0.25 v0,1,0 nil 2"),
        ),
        (
            s,
            ActionMovement {
                object: Some(s),
                first: 0.0,
                second: 1.0,
            },
            format!("18@{s}:h{s} 0 1"),
        ),
        (
            s,
            ActionDone {
                object: Some(s),
                part: ActionPart::Watch,
            },
            format!("19@{s}:h{s} 1"),
        ),
        (
            s,
            ActionDone {
                object: Some(s),
                part: ActionPart::Object,
            },
            format!("23@{s}:h{s} 5"),
        ),
        (
            s,
            HelicopterOnPoint {
                distance: 5.0,
                position: [1.0, 2.0, 3.0],
                point: 4.0,
            },
            format!("25@{s}:5 v1,2,3 4"),
        ),
        (
            s,
            HelicopterOnHit {
                power: 1.0,
                impulse: 2.0,
                hit_type: 3.0,
                hitter: 4.0,
            },
            format!("26@{s}:1 2 3 4"),
        ),
        (
            s,
            OnItemTake { item: Some(RIFLE) },
            format!("27@{s}:h{RIFLE}"),
        ),
        (
            s,
            OnItemDrop { item: Some(RIFLE) },
            format!("28@{s}:h{RIFLE}"),
        ),
        (s, ScriptAnimation, format!("29@{s}:")),
        (s, TraderGlobalAnimRequest, format!("30@{s}:")),
        (s, TraderHeadAnimRequest, format!("31@{s}:")),
        (s, TraderSoundEnd, format!("32@{s}:")),
        (
            p,
            TakeItemFromBox {
                inventory_box: s,
                item: Some(RIFLE),
            },
            format!("33@{p}:h{s} h{RIFLE}"),
        ),
    ];
    let mut checks = String::new();
    for (i, (receiver, event, want)) in events.into_iter().enumerate() {
        raise(&mut t.s, receiver, event)?;
        checks.push_str(&format!("eq(seen[{}], {want:?}, {want:?})\n", i + 1));
    }
    // 17: queued while entry 17 has a function and the sound a source, then
    // delivered in the object's update.
    let sound = |source| PerceivedSound {
        source,
        sound_type: 1.0,
        position: [1.0, 0.0, 0.0],
        power: 0.5,
    };
    let queued = t.s.perceive_sound(s, &sound(Some(p))).map_err(err)?;
    let sourceless = t.s.perceive_sound(s, &sound(None)).map_err(err)?;
    let unset = t.s.perceive_sound(RIFLE, &sound(Some(p))).map_err(err)?;
    ensure(queued && !sourceless && !unset, || {
        format!("queued {queued} {sourceless} {unset}")
    })?;
    t.s.deliver_sounds(s).map_err(err)?;
    t.s.deliver_sounds(s).map_err(err)?;
    checks.push_str(&format!(
        "eq(seen[31], \"17@{s}:h{s} h{p} 1 v1,0,0 0.5\", \"sound\")\neq(#seen, 31, \"delivered once\")\n"
    ));
    run(&mut t.s, "4 seen", &checks)
}

/// Section 5: fast calls.
pub fn fastcall() -> Result<(), String> {
    let mut t = state()?;
    // A physical object for 5's removal on destruction.
    let client = match t
        .s
        .exec("=door", b"return CPhysicObject()")
        .map_err(|e| e.message)?
        .as_slice()
    {
        [Value::Object(c)] => *c,
        other => return Err(format!("the door client {other:?}")),
    };
    t.s.spawn(client, data(90, "door", "physic_object", "O_PHYS_S"))
        .map_err(err)?;
    run(
        &mut t.s,
        "5 set",
        &format!(
            r#"
counts = {{first = 0, second = 0, door = 0, zone = 0}}
local actor = {{}}
function actor:first() counts.first = counts.first + 1 return true end
function actor:second() counts.second = counts.second + 1 return counts.second == 3 end
local o = obj({PLAYER})
o:set_fastcall(actor.first, actor)
o:set_fastcall(actor.second, actor)
obj(90):set_fastcall(function() counts.door = counts.door + 1 end, nil)
obj({SCRIPT_ZONE}):set_fastcall(function() counts.zone = counts.zone + 1 end, nil)
raises(function() o:set_fastcall(actor.first) end, "set_fastcall", "one form")
level.remove_call(actor, actor.second, actor.second)
level.remove_calls_for_object(actor)
"#
        ),
    )?;
    let entries = t.s.level_calls().map_err(err)?;
    ensure(entries == 3, || {
        format!("one fast call per object: {entries}")
    })?;
    for _ in 0..4 {
        t.s.update_level_calls().map_err(err)?;
    }
    // The door is physical: destroying it removes its fast call; the
    // script zone's (a restrictor only) stays after it is destroyed.
    t.s.destroy_object(90).map_err(err)?;
    t.s.destroy_object(SCRIPT_ZONE).map_err(err)?;
    t.s.update_level_calls().map_err(err)?;
    run(
        &mut t.s,
        "5 calls",
        r#"
eq(counts.first, 0, "replaced before called")
eq(counts.second, 3, "called once per update until true")
eq(counts.door, 4, "the door's until destroyed")
eq(counts.zone, 5, "a script zone's after it is destroyed")
"#,
    )?;
    let entries = t.s.level_calls().map_err(err)?;
    ensure(entries == 1, || {
        format!("the script zone's remains: {entries}")
    })
}

/// Section 6: `level.add_call`, `remove_call`, `remove_calls_for_object`
/// and the update.
pub fn level_calls() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "6 add",
        r#"
order = {}
local function note(x) order[#order + 1] = x end
local ready = false
local function c() note("c") return ready end
local function a() note("a") end
level.add_call(c, a)
level.remove_call(c, a)
obj_a = {name = "A"}
function obj_a:cond() note("cond " .. self.name) return true end
function obj_a:act() note("act " .. self.name)
  level.add_call(function() note("late") return true end, function() end)
end
level.add_call(obj_a, "cond", "act")
level.add_call(obj_a, "cond", "act")
local obj_b = {name = "B"}
level.add_call(obj_b, obj_a.cond, obj_a.act)
level.add_call(obj_b, obj_a.cond, obj_a.act)
level.remove_call(obj_b, "cond", "act")
level.remove_call(obj_b, obj_a.cond, obj_a.act)
local obj_c = {name = "C"}
level.add_call(obj_c, function() return false end, function() end)
level.add_call(nil, function(v) note("nil " .. tostring(v)) return true end, function() end)
raises(function() level.add_call(c) end, "add_call", "one argument")
raises(function() level.remove_call(c) end, "remove_call", "one argument")
"#,
    )?;
    let entries = t.s.level_calls().map_err(err)?;
    // c/a, obj_a (once), obj_c, nil: obj_b's three went.
    ensure(entries == 4, || format!("entries after adding {entries}"))?;
    t.s.update_level_calls().map_err(err)?;
    run(
        &mut t.s,
        "6 update",
        r#"
eq(table.concat(order, ","), "c,cond A,act A,nil nil,late", "one update in order, the late entry reached")
level.remove_calls_for_object(nil)
level.remove_calls_for_object(obj_a)
"#,
    )?;
    let entries = t.s.level_calls().map_err(err)?;
    ensure(entries == 2, || format!("c/a and obj_c remain: {entries}"))?;
    t.s.exec("=remove", b"level.remove_calls_for_object(level)")
        .map_err(|e| e.message)?;
    t.s.clear_level_calls().map_err(err)?;
    t.s.update_level_calls().map_err(err)?;
    let entries = t.s.level_calls().map_err(err)?;
    ensure(entries == 0, || format!("cleared: {entries}"))?;
    run(
        &mut t.s,
        "6 cleared",
        r#"eq(table.concat(order, ","), "c,cond A,act A,nil nil,late", "nothing called when cleared")"#,
    )
}

/// Sections 7 and 8: the enemy and patrol callbacks of fighters.
pub fn fighter_callbacks() -> Result<(), String> {
    let mut t = state()?;
    let default = (
        t.s.enemy_callback(STALKER, PLAYER).map_err(err)?,
        t.s.patrol_extrapolate_callback(STALKER, 2).map_err(err)?,
    );
    ensure(default == (true, true), || format!("defaults {default:?}"))?;
    run(
        &mut t.s,
        "7 set",
        &format!(
            r#"
asked = {{}}
local ignore = {{}}
function ignore:enemy(o, enemy) asked.enemy = {{self = self, o:id(), enemy:id()}} return nil end
obj({STALKER}):set_enemy_callback(ignore.enemy, ignore)
obj({STALKER}):set_patrol_extrapolate_callback(function(point) asked.point = point return 1 end)
-- Not a fighter: nothing.
obj({TRADER}):set_enemy_callback(function() return false end)
obj({SCRIPT_ZONE}):set_patrol_extrapolate_callback(function() return false end)
raises(function() obj({STALKER}):set_enemy_callback(5) end, "set_enemy_callback", "a number")
"#
        ),
    )?;
    let enemy = t.s.enemy_callback(STALKER, PLAYER).map_err(err)?;
    let patrol = t.s.patrol_extrapolate_callback(STALKER, 2).map_err(err)?;
    let trader = t.s.enemy_callback(TRADER, PLAYER).map_err(err)?;
    let zone =
        t.s.patrol_extrapolate_callback(SCRIPT_ZONE, 2)
            .map_err(err)?;
    ensure(!enemy && patrol && trader && zone, || {
        format!("results {enemy} {patrol} {trader} {zone}")
    })?;
    run(
        &mut t.s,
        "7 asked",
        &format!(
            r#"
eq(asked.enemy[1], {STALKER}, "the fighter") eq(asked.enemy[2], {PLAYER}, "the candidate")
eq(asked.enemy.self ~= nil, true, "after the bound value")
eq(asked.point, 2, "the point's index")
obj({STALKER}):set_enemy_callback()
obj({STALKER}):set_patrol_extrapolate_callback(nil)
"#
        ),
    )?;
    let cleared = (
        t.s.enemy_callback(STALKER, PLAYER).map_err(err)?,
        t.s.patrol_extrapolate_callback(STALKER, 2).map_err(err)?,
    );
    ensure(cleared == (true, true), || format!("cleared {cleared:?}"))
}

/// Section 9: `game_object` has no `set_use_callback`.
pub fn use_callback() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "9",
        &format!(r#"eq(obj({PLAYER}).set_use_callback, nil, "set_use_callback")"#),
    )
}

/// Section 10: the binder's three callbacks and the door scheme's fast
/// calls on a `CPhysicObject`, used by the player.
pub fn door() -> Result<(), String> {
    let mut t = state()?;
    let client = match t
        .s
        .exec("=door", b"return CPhysicObject()")
        .map_err(|e| e.message)?
        .as_slice()
    {
        [Value::Object(c)] => *c,
        other => return Err(format!("the door client {other:?}")),
    };
    t.s.spawn(client, data(90, "trader_door", "physic_object", "O_PHYS_S"))
        .map_err(err)?;
    run(
        &mut t.s,
        "10 set",
        r#"
uses = {}
binder = {}
function binder:use_callback(door, who) uses[#uses + 1] = {self = self, door:id(), who:id()} end
function binder:hit_callback() end
function binder:death_callback() end
local door = obj(90)
for _ = 1, 2 do
  door:set_callback(callback.hit, binder.hit_callback, binder)
  door:set_callback(callback.death, binder.death_callback, binder)
  door:set_callback(callback.use_object, binder.use_callback, binder)
end
action = {closed = false}
function action:fastcall() return self.closed end
for _ = 1, 3 do door:set_fastcall(action.fastcall, action) end
"#,
    )?;
    let used = t.s.player_use(90, PLAYER).map_err(err)?;
    ensure(used == UseAction::EngineHandling, || format!("{used:?}"))?;
    let entries = t.s.level_calls().map_err(err)?;
    ensure(entries == 1, || format!("one fast call: {entries}"))?;
    t.s.update_level_calls().map_err(err)?;
    t.s.exec("=close", b"action.closed = true")
        .map_err(|e| e.message)?;
    t.s.update_level_calls().map_err(err)?;
    let entries = t.s.level_calls().map_err(err)?;
    ensure(entries == 0, || format!("closed: {entries}"))?;
    run(
        &mut t.s,
        "10 used",
        &format!(
            r#"
eq(#uses, 1, "used once")
eq(rawequal(uses[1].self, binder), true, "the binder")
eq(uses[1][1], 90, "the door") eq(uses[1][2], {PLAYER}, "the player")
"#
        ),
    )
}

/// A fresh state where `setup` runs, then `trigger`: it must stop the state
/// with `LUA error: boom`, logged.
fn fails(
    what: &str,
    setup: &str,
    trigger: impl FnOnce(&mut Scripts) -> Result<(), crate::Error>,
) -> Result<(), String> {
    let mut t = state()?;
    t.s.exec("=setup", setup.as_bytes())
        .map_err(|e| format!("{what}: {}", e.message))?;
    let result = trigger(&mut t.s);
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal && e.message.ends_with("boom")
            && e.message.starts_with("LUA error: ")),
        || format!("{what}: {result:?}"),
    )?;
    let after = t.s.update_level_calls();
    ensure(
        matches!(&after, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{what}: the state goes on: {after:?}"),
    )?;
    let log = t.services.borrow().console.log().join("\n");
    ensure(log.contains("boom"), || {
        format!("{what}: not logged:\n{log}")
    })
}

/// Every engine call of a script function is fatal as D 1.2.
pub fn failures() -> Result<(), String> {
    fails(
        "a callback",
        &format!("obj({STALKER}):set_callback(callback.death, function() error('boom') end)"),
        |s| s.raise(STALKER, &ObjectEvent::Death { killer: None }),
    )?;
    fails(
        "the engine's own use",
        &format!("obj({RIFLE}):set_callback(callback.use_object, function() error('boom') end)"),
        |s| {
            s.use_object(RIFLE, Some(PLAYER))
                .map(drop)
                .map_err(|f| f.error)
        },
    )?;
    fails(
        "a condition",
        "level.add_call(function() error('boom') end, function() end)",
        |s| s.update_level_calls(),
    )?;
    fails(
        "an action",
        "level.add_call({}, function() return true end, function() error('boom') end)",
        |s| s.update_level_calls(),
    )?;
    fails(
        "a fast call",
        &format!("obj({PLAYER}):set_fastcall(function() error('boom') end, nil)"),
        |s| s.update_level_calls(),
    )?;
    fails(
        "an enemy callback",
        &format!("obj({STALKER}):set_enemy_callback(function() error('boom') end)"),
        |s| s.enemy_callback(STALKER, PLAYER).map(drop),
    )?;
    fails(
        "a patrol callback",
        &format!("obj({STALKER}):set_patrol_extrapolate_callback(function() error('boom') end)"),
        |s| s.patrol_extrapolate_callback(STALKER, 0).map(drop),
    )
}
