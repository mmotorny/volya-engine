// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-action-planner.md on every target, one check
//! per section, over a stalker and an item of our own and planners,
//! evaluators and actions of test script classes.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;

use super::ensure;
use super::files::{FSGAME, Tree};
use super::game_object::data;
use super::services::run;
use crate::action_planner::search::{self, Planning, Step};
use crate::action_planner::{STALKER_IDS, WorldProperty, WorldState};
use crate::files::FileSystem;
use crate::game_object::{GameObjects, ObjectClass};
use crate::loader::{Prefix, Scripts};
use crate::services::EngineServices;
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 world properties and states", world),
    ("3 property_storage", storage),
    ("4 property evaluators", evaluators),
    ("5 actions", actions),
    ("6.2 planner members", planner_members),
    ("6.3 update", update),
    ("7 planning", planning),
    ("7.4 the open states' order", open_order),
    ("8 planner_action", planner_action),
    ("9 stalker_ids", stalker_ids),
    ("10 motivation manager and ef_storage", motivation_manager),
    ("11 saving the store", saving),
    ("12 worked example", worked_example),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("action planner {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const STALKER: u16 = 10;
const RIFLE: u16 = 20;

struct State {
    s: Scripts,
    services: Rc<RefCell<EngineServices>>,
}

/// A state with the game object basics and the action planner
/// registered, `_G` loaded, a global `obj(id)` giving handles, a stalker
/// and a rifle online, and the test classes of [`CLASSES`].
fn state() -> Result<State, String> {
    let tree = Rc::new(Tree(
        [("gamedata/scripts/_g.script", "")]
            .iter()
            .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
            .collect(),
    ));
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
    s.register_action_planner().map_err(|e| e.message)?;
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
    stalker.health = Some(1.0);
    let spawned = [
        (ObjectClass::Named("CAI_Stalker"), stalker),
        (
            ObjectClass::Named("CWeaponAK74"),
            data(RIFLE, "", "wpn_ak74", "WP_AK74"),
        ),
    ];
    for (class, data) in spawned {
        s.spawn_engine_object(class, data)
            .map_err(|e| e.to_string())?;
    }
    s.exec("=classes", CLASSES.as_bytes())
        .map_err(|e| e.message)?;
    Ok(State { s, services })
}

/// Test classes: an evaluator giving `self.value` and an action noting
/// its engine calls in the global `calls`.
const CLASSES: &str = r##"
calls = {}
function note(text) table.insert(calls, text) end
function taken() local t = table.concat(calls, " ") calls = {} return t end

class "test_eval" (property_evaluator)
function test_eval:__init(name, value) super(nil, name) self.name = name self.value = value end
function test_eval:evaluate() note("e" .. self.name) return self.value end

class "test_action" (action_base)
function test_action:__init(name) super(nil, name) self.name = name end
function test_action:setup(object, storage)
  action_base.setup(self, object, storage)
  note("setup" .. self.name)
end
function test_action:initialize() action_base.initialize(self) note("i" .. self.name) end
function test_action:execute() action_base.execute(self) note("x" .. self.name) end
function test_action:finalize() action_base.finalize(self) note("f" .. self.name) end

-- An action with conditions {pre} and {eff}, each a list of {id, value}.
function make_action(name, pre, eff)
  local a = test_action(name)
  for _, p in ipairs(pre) do a:add_precondition(world_property(p[1], p[2])) end
  for _, p in ipairs(eff) do a:add_effect(world_property(p[1], p[2])) end
  return a
end

-- A planner set up with the stalker, its goal {goal}.
function make_planner(goal)
  local pl = action_planner()
  pl:setup(obj(10))
  local g = world_state()
  for _, p in ipairs(goal) do g:add_property(world_property(p[1], p[2])) end
  pl:set_goal_world_state(g)
  return pl
end
"##;

/// Section 1: the names registered, and the listed ones that aren't.
fn registered() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "1",
        r##"
for _, c in ipairs({"world_property", "world_state", "property_storage",
    "property_evaluator", "property_evaluator_const", "action_base",
    "action_planner", "planner_action", "stalker_ids"}) do
  eq(type(rawget(_G, c)), "userdata", c)
end
eq(type(ef_storage), "function", "ef_storage")
for _, n in ipairs({"cast_action_to_planner", "cast_planner_to_action", "debug_planner"}) do
  eq(rawget(_G, n), nil, n)
end
eq(obj(10).debug_planner, nil, "game_object:debug_planner")
eq(action_base().show, nil, "action_base:show")
eq(action_planner().show, nil, "action_planner:show")
eq(action_planner().reinit, nil, "action_planner:reinit")
"##,
    )
}

/// Section 2: world properties and world states.
fn world() -> Result<(), String> {
    // TODO(#1223): the spec's example has 103,320,731; its formula gives
    // this.
    ensure(WorldProperty::new(4, true).hash() == 103_340_187, || {
        format!("hash (4, true): {}", WorldProperty::new(4, true).hash())
    })?;
    ensure(WorldProperty::new(2, true).hash() == 2_637_066_335, || {
        format!("hash (2, true): {}", WorldProperty::new(2, true).hash())
    })?;
    let s = WorldState::of(&[WorldProperty::new(4, true), WorldProperty::new(2, true)]);
    ensure(s.hash() == 103_340_187 ^ 2_637_066_335, || {
        "a state's hash".into()
    })?;
    let mut t = state()?;
    run(
        &mut t.s,
        "2",
        r##"
local p = world_property(5.7, 0)
eq(p:condition(), 5, "condition made an id")
eq(p:value(), true, "0 is true")
eq(world_property(-1, nil):condition(), 4294967295, "an id is unsigned")
eq(world_property(3, false) == world_property(3, false), true, "==")
eq(world_property(3, false) == world_property(3, true), false, "== by value")
eq(world_property(3, false) < world_property(3, true), true, "false before true")
eq(world_property(2, true) < world_property(3, false), true, "by condition first")
local s = world_state()
s:add_property(world_property(5, true))
s:add_property(world_property(5, false))
s:add_property(world_property(2, true))
eq(s:property(3):condition(), 5, "property: the first at least c")
eq(s:property(3):value(), false, "both kept, sorted by value")
eq(s:property(6), nil, "property: nil past the end")
local c = world_state(s)
s:remove_property(3)
eq(s:property(5):value(), true, "remove_property removes the next")
eq(c:property(5):value(), false, "the copy is its own")
eq(c:includes(s), true, "includes")
eq(s:includes(c), false, "includes not")
local a, b = world_state(), world_state()
a:add_property(world_property(1, true)) a:add_property(world_property(2, true))
b:add_property(world_property(2, true)) b:add_property(world_property(1, true))
eq(a == b, true, "== by hash")
local d = world_state() d:add_property(world_property(1, true))
eq(d < a, true, "a beginning is the smaller")
eq(a < d, false, "not the larger")
raises(function() s:remove_property(6) end, "#1222", "remove past the end")
s:clear()
eq(s:property(0), nil, "cleared")
eq(s == world_state(), true, "cleared: hash 0")
"##,
    )
}

/// Section 3: the property store.
fn storage() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "3",
        r##"
local st = property_storage()
st:set_property(7, true)
st:set_property(8, false)
st:set_property(7, false)
eq(st:property(7), false, "replaced")
eq(st:property(8), false, "appended")
raises(function() st:property(9) end, "#1222", "an id not stored")
"##,
    )
}

/// Section 4: evaluators, their defaults and overrides.
fn evaluators() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "4",
        r##"
local e = property_evaluator()
eq(e.object, nil, "no object")
eq(e.storage, nil, "no store")
eq(e:evaluate(), false, "the default")
local st = property_storage()
st:set_property(1, true)
e:setup(obj(10), st)
eq(e.object:id(), 10, "setup's object")
eq(e.storage:property(1), true, "setup's store")
eq(property_evaluator(obj(10)).object:id(), 10, "(object)")
eq(property_evaluator(obj(10), "x").object:id(), 10, "(object, name)")
eq(property_evaluator_const(1):evaluate(), true, "const")
eq(property_evaluator_const(nil):evaluate(), false, "const nil")
local s = test_eval("a", 5)
eq(s:evaluate(), 5, "an override")
eq(property_evaluator.evaluate(s), false, "the base call")
"##,
    )
}

/// Section 5: actions.
fn actions() -> Result<(), String> {
    let mut t = state()?;
    t.services.borrow_mut().device.frame(1234, 0.0, false);
    run(
        &mut t.s,
        "5",
        r##"
local a = action_base(nil, "a")
eq(a.object, nil, "no object")
eq(a:weight(world_state(), world_state()), 1, "weight 1")
a:set_weight(65537)
eq(a:weight(), 1, "made a weight")
a:set_weight(-1)
eq(a:weight(), 65535, "unsigned")
a:add_precondition(world_property(3, true))
a:add_effect(world_property(4, true))
raises(function() a:remove_effect(5) end, "#1222", "remove_effect past the end")
a:remove_effect(1)
a:remove_precondition(3)
eq(action_base(obj(10)).object:id(), 10, "(object)")
a:initialize() a:execute() a:finalize()
probe = a
"##,
    )?;
    let part = match t.s.exec("=probe", b"return probe").map_err(|e| e.message)?[..] {
        [Value::Object(p)] => p,
        ref v => return Err(format!("probe {v:?}")),
    };
    let got = t.s.with_action(part, |a| {
        (
            a.start_time,
            a.first_execution,
            a.preconditions.properties().len(),
            a.effects.properties().len(),
        )
    });
    ensure(got == Some((1234, false, 0, 0)), || {
        format!("start time, flag, conditions: {got:?}")
    })
}

/// Section 6.2: the planner's members.
fn planner_members() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "6.2",
        r##"
local pl = action_planner()
eq(pl:initialized(), false, "not initialized")
raises(function() pl:current_action_id() end, "#1222", "before setup")
raises(function() pl:actual() end, "#1222", "actual before setup")
pl:setup(obj(10))
eq(pl.object:id(), 10, "object")
eq(pl:current_action_id(), 4294967295, "none")
eq(pl:actual(), true, "actual after setup")
pl.storage:set_property(3, true)
local a = test_action("a")
pl:add_action(5, a)
eq(taken(), "setupa", "add_action sets the action up")
eq(a.object:id(), 10, "with the planner's object")
eq(a.storage:property(3), true, "and store")
eq(pl:actual(), false, "add_action: not actual")
eq(pl:action(4), a, "action: the first id at least")
eq(pl:action(5), a, "action")
raises(function() pl:action(6) end, "#1222", "action past the end")
raises(function() pl:add_action(5, test_action("b")) end, "#1213", "an id already there")
local e = test_eval("e", true)
pl:add_evaluator(9, e)
eq(e.object:id(), 10, "add_evaluator sets the evaluator up")
eq(pl:evaluator(9), e, "evaluator")
raises(function() pl:evaluator(8) end, "#1222", "evaluator: exactly the id")
local e2 = property_evaluator_const(false)
pl:add_evaluator(9, e2)
eq(pl:evaluator(9), e2, "replaced")
raises(function() pl:remove_evaluator(8) end, "#1213", "remove_evaluator: the next")
raises(function() pl:remove_evaluator(10) end, "#1222", "remove_evaluator past the end")
pl:remove_evaluator(9)
raises(function() pl:clear() end, "#1213", "clear with parts")
pl:remove_action(4)
raises(function() pl:remove_action(4) end, "#1222", "remove_action past the end")
pl:clear()
-- set_goal_world_state's rule.
local q = action_planner() q:setup(obj(10))
local g = world_state() g:add_property(world_property(1, true))
q:set_goal_world_state(g)
eq(q:actual(), false, "a new goal")
q:setup(obj(10))
q:set_goal_world_state(world_state())
eq(q:actual(), true, "the same goal")
eq(action_planner.update ~= nil, true, "the default update")
"##,
    )
}

/// Section 6.3: an update runs one action, the plan's first.
fn update() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "6.3",
        r##"
-- 1 -> goal through A (2) then B (3).
local pl = make_planner({{1, true}})
local e1, e2 = test_eval("1", false), test_eval("2", false)
pl:add_evaluator(1, e1)
pl:add_evaluator(2, e2)
pl:add_action(2, make_action("A", {{2, false}}, {{2, true}}))
pl:add_action(3, make_action("B", {{2, true}}, {{1, true}}))
taken()
pl:update()
eq(taken(), "e1 e2 iA xA", "plans, initializes and executes A")
eq(pl:current_action_id(), 2, "current A")
eq(pl:initialized(), true, "initialized")
pl:update()
eq(taken(), "e1 e2 xA", "actual: the cached conditions only")
e2.value = true
pl:update()
eq(taken(), "e1 e2 e1 e2 fA iB xB", "replans, finalizes A, initializes B")
eq(pl:current_action():weight(), 1, "current_action")
e1.value = true
raises(function() pl:update() end, "#1222", "an empty plan")
"##,
    )
}

/// Section 7: planning's world values and costs.
fn planning() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "7",
        r##"
-- Two routes to 1: A (2) at weight 5, or B (3) then C (4) at 1 each.
class "heavy" (test_action)
function heavy:__init(name) super(name) end
function heavy:weight(s1, s2)
  note("w" .. tostring(s1:property(1) ~= nil) .. tostring(s2:property(0) == nil))
  return 5
end
local pl = make_planner({{1, true}})
pl:add_evaluator(1, test_eval("1", false))
pl:add_evaluator(5, test_eval("5", false))
local a = heavy("A")
a:add_effect(world_property(1, true))
pl:add_action(2, a)
pl:add_action(3, make_action("B", {}, {{5, true}}))
pl:add_action(4, make_action("C", {{5, true}}, {{1, true}}))
taken()
pl:update()
eq(taken(), "e1 wtruetrue e5 wtruefalse iB xB", "the cheaper route; weight's states")
-- A condition no evaluator gives.
local q = make_planner({{7, true}})
raises(function() q:add_action(1, make_action("D", {}, {{7, true}})) q:update() end,
  "#1222", "no evaluator")
-- No route: the previous plan stays.
local r = make_planner({{1, true}})
local e = test_eval("1", false)
r:add_evaluator(1, e)
r:add_evaluator(6, test_eval("6", false))
r:add_action(2, make_action("E", {{6, false}}, {{1, true}}))
r:update()
eq(r:current_action_id(), 2, "E")
r:evaluator(6).value = true
taken()
r:update()
eq(taken(), "e1 e6 e1 e6 xE", "no plan found: E stays")
"##,
    )
}

/// A search over fixed world values with unit costs.
struct Fixed;

impl Planning for Fixed {
    fn world_value(&mut self, _: u32) -> Result<bool, String> {
        Ok(false)
    }

    fn weight(&mut self, _: usize, _: &WorldState, _: &WorldState) -> Result<u16, String> {
        Ok(1)
    }
}

/// Section 7.4: of two states of equal f, the later added is taken first.
fn open_order() -> Result<(), String> {
    let goal = WorldState::of(&[WorldProperty::new(10, true)]);
    let none = WorldState::new();
    let a = WorldState::of(&[WorldProperty::new(10, true), WorldProperty::new(11, true)]);
    let b = WorldState::of(&[WorldProperty::new(10, true), WorldProperty::new(12, true)]);
    let steps = [
        Step {
            id: 1,
            preconditions: &none,
            effects: &a,
        },
        Step {
            id: 2,
            preconditions: &none,
            effects: &b,
        },
    ];
    let plan = search::search(&mut Fixed, &goal, &steps)?;
    ensure(plan == Some(vec![2]), || format!("plan {plan:?}"))?;
    let unreachable = WorldState::of(&[WorldProperty::new(13, true)]);
    let plan = search::search(&mut Fixed, &unreachable, &steps)?;
    ensure(plan.is_none(), || format!("no route: {plan:?}"))?;
    let plan = search::search(&mut Fixed, &none, &steps)?;
    ensure(plan == Some(Vec::new()), || {
        format!("the goal holds: {plan:?}")
    })
}

/// Section 8: a planner action runs its inner planner.
fn planner_action() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "8",
        r##"
local pl = make_planner({{1, true}})
pl:add_evaluator(1, test_eval("1", false))
local pa = planner_action(nil, "inner")
pa:add_effect(world_property(1, true))
pl:add_action(2, pa)
eq(pa.object:id(), 10, "setup: the object")
eq(pa:actual(), false, "setup: the goal is the effects")
pa:add_evaluator(1, test_eval("i", false))
pa:add_action(3, make_action("X", {}, {{1, true}}))
taken()
pl:update()
eq(taken(), "e1 ei iX xX", "the inner plan runs")
eq(pa:current_action_id(), 3, "inner current")
pa:finalize()
eq(taken(), "fX", "finalize: the inner current action")
eq(pa:initialized(), false, "finalize: not initialized")
eq(pa:weight(world_state(), world_state()), 1, "weight")
"##,
    )
}

/// Section 9: the constants.
fn stalker_ids() -> Result<(), String> {
    ensure(STALKER_IDS.len() == 119, || "119 constants".into())?;
    let mut t = state()?;
    run(
        &mut t.s,
        "9",
        r##"
eq(stalker_ids.action_script, 61, "action_script")
eq(stalker_ids.property_script, 44, "property_script")
eq(stalker_ids.action_alife_planner, 57, "action_alife_planner")
eq(stalker_ids.property_anomaly, 42, "property_anomaly")
eq(stalker_ids.sound_script, 23, "sound_script")
eq(stalker_ids.sound_enemy_killed_or_wounded, -805302272, "signed")
eq(stalker_ids.action_kill_enemy, 19, "action_kill_enemy")
"##,
    )
}

/// Section 10: the motivation manager.
fn motivation_manager() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "10",
        r##"
local m = obj(10):motivation_action_manager()
eq(m, obj(10):motivation_action_manager(), "the same planner")
eq(m.object:id(), 10, "set up with the stalker")
eq(m:initialized(), false, "not initialized")
eq(m:current_action_id(), 4294967295, "no current action")
raises(function() obj(20):motivation_action_manager() end, "#1222", "not a stalker")
for _, id in ipairs({2, 56, 57, 58, 59, 60}) do
  eq(m:action(id) ~= nil, true, "action " .. id)
end
m:action(57):add_precondition(world_property(1065, true))
for _, id in ipairs({0, 2, 4, 6, 7, 8, 42}) do
  eq(m:evaluator(id) ~= nil, true, "evaluator " .. id)
end
raises(function() m:evaluator(42):evaluate() end, "#1185", "the engine's evaluator")
m:remove_evaluator(stalker_ids.property_anomaly)
m:add_evaluator(stalker_ids.property_anomaly, property_evaluator_const(false))
eq(m:evaluator(42):evaluate(), false, "replaced by a constant")
eq(ef_storage(), ef_storage(), "ef_storage: one object")
"##,
    )
}

/// Section 11: the store as a planner saves it.
fn saving() -> Result<(), String> {
    let st = crate::action_planner::PropertyStorage::default();
    st.set(7, true);
    st.set(300, false);
    let mut out = Vec::new();
    st.save(&mut out);
    ensure(out == [2, 0, 0, 0, 7, 0, 0, 0, 1, 44, 1, 0, 0, 0], || {
        format!("saved {out:?}")
    })?;
    let loaded = crate::action_planner::PropertyStorage::default();
    loaded.set(300, true);
    ensure(loaded.load(&out) == Some(out.len()), || "loaded".into())?;
    ensure(*loaded.0.borrow() == [(300, false), (7, true)], || {
        format!("load sets each pair: {:?}", loaded.0.borrow())
    })
}

/// Section 12.3: the camp fire's plans on the motivation manager, with
/// constant evaluators for the engine's conditions.
fn worked_example() -> Result<(), String> {
    let mut t = state()?;
    let setup = r##"
local m = obj(10):motivation_action_manager()
for _, id in ipairs({0, 2, 4, 6, 7, 8, 42}) do
  m:remove_evaluator(id)
  m:add_evaluator(id, test_eval(tostring(id), id == 0))
end
kamp_end, on_position = test_eval("1065", false), test_eval("1066", false)
m:add_evaluator(1065, kamp_end)
m:add_evaluator(1066, on_position)
local common = {1075, 2054, 3144, 1164}
for _, id in ipairs(common) do m:add_evaluator(id, test_eval(tostring(id), false)) end
local function camp(name, at, effect)
  local pre = {{0, true}, {8, false}, {7, false}, {42, false}}
  for _, id in ipairs(common) do table.insert(pre, {id, false}) end
  table.insert(pre, {1066, at})
  return make_action(name, pre, {effect})
end
m:add_action(204, camp("wait", true, {1065, true}))
m:add_action(202, camp("go", false, {1066, true}))
m:action(57):add_precondition(world_property(1065, true))
taken()
"##;
    t.s.exec("=setup", setup.as_bytes())
        .map_err(|e| e.message)?;
    let plan = |t: &mut State| -> Result<Vec<u32>, String> {
        let m = t.s.motivation_manager(STALKER).ok_or("no manager")?;
        t.s.with_planner(m, |p| p.plan.clone())
            .ok_or_else(|| "no planner".into())
    };
    let cached = |t: &mut State| -> Result<Vec<u32>, String> {
        let m = t.s.motivation_manager(STALKER).ok_or("no manager")?;
        (t.s.with_planner(m, |p| p.cached.keys().copied().collect()))
            .ok_or_else(|| "no planner".into())
    };
    // 1: away from its camp place.
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    let got = plan(&mut t)?;
    ensure(got == [202, 204, 57], || format!("step 1 plan {got:?}"))?;
    let got = cached(&mut t)?;
    let want = [0, 4, 6, 7, 8, 42, 1065, 1066, 1075, 1164, 2054, 3144];
    ensure(got == want, || format!("step 1 cached {got:?}"))?;
    // 2: at the camp place.
    t.s.exec("=at", b"on_position.value = true taken()")
        .map_err(|e| e.message)?;
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    let got = plan(&mut t)?;
    ensure(got == [204, 57], || format!("step 2 plan {got:?}"))?;
    let calls =
        t.s.exec("=calls", b"return taken()")
            .map_err(|e| e.message)?;
    let calls = calls.first().and_then(Value::as_str).unwrap_or_default();
    ensure(calls.ends_with("fgo iwait xwait"), || {
        format!("step 2 calls {calls}")
    })?;
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    let calls =
        t.s.exec("=calls", b"return taken()")
            .map_err(|e| e.message)?;
    let calls = calls.first().and_then(Value::as_str).unwrap_or_default();
    ensure(calls.ends_with("xwait") && !calls.contains("iwait"), || {
        format!("step 2 again {calls}")
    })?;
    // 4: an enemy appears away from the camp place.
    t.s.exec(
        "=enemy",
        b"on_position.value = false obj(10):motivation_action_manager():evaluator(7).value = true",
    )
    .map_err(|e| e.message)?;
    let result = t.s.update_motivation_manager(STALKER);
    let got = plan(&mut t)?;
    ensure(got == [58, 202, 204, 57], || format!("step 4 plan {got:?}"))?;
    // The engine's combat planner is the stalker planner's.
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal && e.message.contains("#1185")),
        || format!("step 4: {result:?}"),
    )
}
