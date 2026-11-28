// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-object-binders.md on every target, one check
//! per section, over binders of a script of our own.

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
use crate::game_object::object_binder::{SaveRelevance, update_delta};
use crate::game_object::{GameObjects, ObjectClass, SpawnData};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::class_id;
use crate::services::{EngineServices, GameType};
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1, 2 object_binder", class),
    ("3 binding", binding),
    ("4 bind_object and binded_object", bind_object),
    ("5.1 spawning", spawning),
    ("5.2, 5.3 update and saving", update_and_saving),
    ("5.5, 5.6 release and destroying", destroying),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("object binders {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// The sections objects spawn with: binding functions found, inherited,
/// not found, empty, failing and the engine's own binder.
const SYSTEM: &str = "[bound]\r\nscript_binding = binders.bind\r\n\
    [child]:bound\r\n\
    [missing]\r\nscript_binding = binders.nowhere\r\n\
    [empty]\r\nscript_binding =\r\n\
    [failing]\r\nscript_binding = binders.fail\r\n\
    [plain]\r\nscript_binding = binders.plain\r\n\
    [unbound]\r\n";

/// A binder class noting the engine's calls in `calls`.
const BINDERS: &str = r##"
calls = {}
function note(...)
  local t = {}
  for i = 1, select("#", ...) do t[i] = tostring((select(i, ...))) end
  table.insert(calls, table.concat(t, " "))
end
function taken() local c = table.concat(calls, ", ") calls = {} return c end
class "test_binder" (object_binder)
function test_binder:__init(obj) super(obj) self.mark = obj:id() end
function test_binder:reload(section) object_binder.reload(self, section) note("reload", section) end
function test_binder:reinit() object_binder.reinit(self) note("reinit") end
function test_binder:load(reader) note("load", reader) end
function test_binder:net_spawn(data)
  note("net_spawn", type(data))
  if spawn_result ~= nil then return spawn_result end
  return object_binder.net_spawn(self, data)
end
function test_binder:update(delta) note("update", delta) end
function test_binder:save(packet) note("save", packet) end
function test_binder:net_save_relevant() note("net_save_relevant") return relevant end
function test_binder:net_Relcase(obj) note("net_Relcase", self.object:id(), obj:id()) end
function test_binder:net_destroy() note("net_destroy", self.object:id()) end
function bind(obj) note("bind", obj:id()) obj:bind_object(test_binder(obj)) end
function fail(obj) error("binding failed") end
function plain(obj) note("plain", obj:id()) obj:bind_object(object_binder(obj)) end
"##;

/// A test state: the scripts, the engine objects, the services.
struct State {
    s: Scripts,
    objects: Rc<RefCell<GameObjects>>,
    services: Rc<RefCell<EngineServices>>,
}

fn data(id: u16, section: &str) -> SpawnData {
    SpawnData {
        id,
        name: Vec::new(),
        section: section.as_bytes().to_vec(),
        class_id: class_id(b"SPC_RS_S"),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: None,
    }
}

/// A state with the game object basics and binders registered, `_G`
/// loaded, `binders.script` in the scripts and a global `obj(id)` giving
/// handles.
fn state() -> Result<State, String> {
    let tree = Tree(
        [
            ("gamedata/scripts/_g.script", ""),
            ("gamedata/scripts/binders.script", BINDERS),
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
    let system = ltx::parse(SYSTEM.as_bytes()).map_err(|e| e.to_string())?;
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
    s.load_namespace("binders").map_err(|e| e.message)?;
    Ok(State {
        s,
        objects,
        services,
    })
}

fn restrictor(t: &mut State, id: u16, section: &str) -> Result<(), String> {
    t.s.spawn_engine_object(ObjectClass::Named("CSpaceRestrictor"), data(id, section))
        .map(drop)
        .map_err(|e| e.to_string())
}

/// A new client object of engine class `class`.
fn client(t: &mut State, class: &str) -> Result<EnginePart, String> {
    let results =
        t.s.exec("=client", format!("return {class}()").as_bytes())
            .map_err(|e| e.message)?;
    match results.as_slice() {
        [Value::Object(part)] => Ok(*part),
        other => Err(format!("{class}(): {other:?}")),
    }
}

/// Sections 1, 2.
pub fn class() -> Result<(), String> {
    let mut t = state()?;
    restrictor(&mut t, 1, "unbound")?;
    run(
        &mut t.s,
        "2",
        r##"
for _, m in ipairs({"save", "update", "reload", "net_export", "net_save_relevant", "load",
    "net_destroy", "reinit", "net_Relcase", "net_spawn", "net_import"}) do
  eq(type(object_binder[m]), "function", "object_binder." .. m)
end
eq(type(game_object.bind_object), "function", "bind_object")
eq(type(game_object.binded_object), "function", "binded_object")
local b = binders.test_binder(obj(1))
eq(rawequal(b.object, obj(1)), true, "object is the constructor's argument")
eq(b.mark, 1, "the script class's own field")
eq(object_binder.net_spawn(b, 0), true, "net_spawn's default")
eq(object_binder.net_save_relevant(b), false, "net_save_relevant's default")
eq(select("#", object_binder.reinit(b)), 0, "reinit's default returns nothing")
eq(object_binder(nil).object, nil, "nil: no object")
raises(function() return object_binder(1) end, "game object expected", "not an object")
"##,
    )
}

/// Section 3.
pub fn binding() -> Result<(), String> {
    let mut t = state()?;
    for (id, section) in [(1, "bound"), (2, "child"), (3, "missing"), (4, "empty")] {
        restrictor(&mut t, id, section)?;
    }
    restrictor(&mut t, 5, "unbound")?;
    restrictor(&mut t, 6, "plain")?;
    run(
        &mut t.s,
        "3",
        r#"
eq(binders.taken(), "bind 1, reload bound, reinit, net_spawn number, bind 2, reload child, "
  .. "reinit, net_spawn number, plain 6", "the binding, then the binder's calls")
eq(obj(2):binded_object().mark, 2, "an inherited key binds")
for id = 3, 5 do eq(obj(id):binded_object(), nil, "no binder for " .. id) end
eq(type(obj(6):binded_object()), "userdata", "the engine's own binder")
"#,
    )?;
    // A failing binding function is fatal (D 1.2).
    let result =
        t.s.spawn_engine_object(ObjectClass::Named("CSpaceRestrictor"), data(7, "failing"));
    ensure(
        matches!(&result, Err(f) if f.error.kind == ErrorKind::Fatal
            && f.error.message.starts_with("LUA error: ")
            && f.error.message.ends_with("binding failed")),
        || format!("a failing binding: {result:?}"),
    )
}

/// Section 4.
pub fn bind_object() -> Result<(), String> {
    let mut t = state()?;
    restrictor(&mut t, 1, "bound")?;
    restrictor(&mut t, 2, "unbound")?;
    run(
        &mut t.s,
        "4",
        r#"
binders.taken()
collectgarbage("collect")
local b = obj(1):binded_object()
eq(b.mark, 1, "the engine keeps the binder from the collector")
eq(rawequal(b, obj(1):binded_object()), true, "the same binder each call")
eq(rawequal(b.object, obj(1)), true, "its object")
local other = binders.test_binder(obj(1))
obj(1):bind_object(other)
eq(rawequal(obj(1):binded_object(), other), true, "a second binder replaces the first")
eq(binders.taken(), "", "binding calls no binder method")
raises(function() obj(2):bind_object({}) end, "object_binder expected", "not a binder")
"#,
    )?;
    t.services.borrow_mut().game_type = GameType::Deathmatch;
    run(
        &mut t.s,
        "4 multiplayer",
        r#"
obj(2):bind_object(binders.test_binder(obj(2)))
eq(obj(2):binded_object(), nil, "nothing attached outside single-player")
"#,
    )
}

/// Section 5.1.
pub fn spawning() -> Result<(), String> {
    let mut t = state()?;
    let first = client(&mut t, "CSpaceRestrictor")?;
    let saved = client(&mut t, "CSpaceRestrictor")?;
    let spawned =
        t.s.spawn(first, data(10, "bound"))
            .map_err(|e| e.to_string())?;
    let loaded =
        t.s.spawn_saved(saved, data(11, "bound"), Value::from("reader"))
            .map_err(|e| e.to_string())?;
    ensure(spawned && loaded, || format!("spawns {spawned} {loaded}"))?;
    run(
        &mut t.s,
        "5.1",
        r#"
eq(binders.taken(), "bind 10, reload bound, reinit, net_spawn number, "
  .. "bind 11, reload bound, reinit, load reader, net_spawn number", "spawning's calls")
binders.spawn_result = false
"#,
    )?;
    // A false `net_spawn` fails the spawning: destroyed and logged.
    let failing = client(&mut t, "CSpaceRestrictor")?;
    let physics = client(&mut t, "CPhysicObject")?;
    let failed =
        t.s.spawn(failing, data(12, "bound"))
            .map_err(|e| e.to_string())?;
    let kept =
        t.s.spawn(physics, data(13, "bound"))
            .map_err(|e| e.to_string())?;
    ensure(!failed && kept, || format!("spawns {failed} {kept}"))?;
    let objects = t.objects.borrow();
    ensure(
        objects.get(12).is_none() && objects.get(13).is_some(),
        || "a failed spawning destroys the object, CPhysicObject's never fails".into(),
    )?;
    drop(objects);
    let log = t.services.borrow().console.log().join("\n");
    ensure(log.contains("! Failed to spawn entity 'bound'"), || {
        format!("no failed spawning in the log:\n{log}")
    })?;
    run(
        &mut t.s,
        "5.1 failed",
        r#"
eq(binders.taken(), "bind 12, reload bound, reinit, net_spawn number, net_destroy 12, "
  .. "bind 13, reload bound, reinit, net_spawn number", "the failed spawning's calls")
"#,
    )
}

/// Sections 5.2, 5.3.
pub fn update_and_saving() -> Result<(), String> {
    ensure(
        [
            update_delta(0, 0),
            update_delta(50, 0),
            update_delta(5000, 0),
            update_delta(5000, 3000),
        ] == [1, 50, 1000, 3000],
        || "the update interval".into(),
    )?;
    ensure(
        SaveRelevance::of(ObjectClass::Player) == SaveRelevance::Always
            && SaveRelevance::of(ObjectClass::Named("CAI_Stalker")) == SaveRelevance::Always
            && SaveRelevance::of(ObjectClass::Named("CPhysicObject")) == SaveRelevance::Always
            && SaveRelevance::of(ObjectClass::Named("CMincer")) == SaveRelevance::Binder
            && SaveRelevance::of(ObjectClass::Named("CHelicopter"))
                == SaveRelevance::BinderOrFurtherPart,
        || "save relevance by class".into(),
    )?;
    let mut t = state()?;
    restrictor(&mut t, 1, "bound")?;
    restrictor(&mut t, 2, "unbound")?;
    let physics = client(&mut t, "CPhysicObject")?;
    t.s.spawn(physics, data(3, "bound"))
        .map_err(|e| e.to_string())?;
    t.s.exec("=taken", b"binders.taken() binders.relevant = true")
        .map_err(|e| e.message)?;
    t.s.scheduled_update(1, 50).map_err(|e| e.to_string())?;
    t.s.scheduled_update(2, 50).map_err(|e| e.to_string())?;
    let mut relevant = Vec::new();
    for (id, further) in [(1, false), (2, false), (3, false)] {
        relevant.push(t.s.save_relevant(id, further).map_err(|e| e.to_string())?);
    }
    t.s.exec("=irrelevant", b"binders.relevant = false")
        .map_err(|e| e.message)?;
    relevant.push(t.s.save_relevant(1, false).map_err(|e| e.to_string())?);
    t.objects
        .borrow_mut()
        .get_mut(3)
        .expect("spawned")
        .destroying = true;
    relevant.push(t.s.save_relevant(3, false).map_err(|e| e.to_string())?);
    ensure(relevant == [true, false, true, false, false], || {
        format!("save relevance {relevant:?}")
    })?;
    t.s.save_binder(1, Value::from("packet"))
        .map_err(|e| e.to_string())?;
    run(
        &mut t.s,
        "5.2, 5.3",
        r#"
eq(binders.taken(), "update 50, net_save_relevant, net_save_relevant, save packet",
  "update, the relevance asked, save")
"#,
    )
}

/// Sections 5.5, 5.6.
pub fn destroying() -> Result<(), String> {
    let mut t = state()?;
    for id in [1, 2, 3] {
        restrictor(&mut t, id, "bound")?;
    }
    restrictor(&mut t, 4, "unbound")?;
    t.s.exec("=keep", b"binders.taken() kept = obj(2):binded_object()")
        .map_err(|e| e.message)?;
    t.s.release_destroyed(&[1, 2]).map_err(|e| e.to_string())?;
    let left: Vec<u16> = [1, 2, 3, 4]
        .into_iter()
        .filter(|id| t.objects.borrow().get(*id).is_some())
        .collect();
    ensure(left == [3, 4], || format!("objects left {left:?}"))?;
    run(
        &mut t.s,
        "5.5, 5.6",
        r#"
eq(binders.taken(), "net_Relcase 1 2, net_Relcase 1 1, net_Relcase 2 2, net_Relcase 2 1, "
  .. "net_Relcase 3 2, net_Relcase 3 1, net_destroy 1, net_destroy 2",
  "the release notices, then destroying")
eq(kept.mark, 2, "a deleted binder a script kept stays reachable")
"#,
    )?;
    let destroyed = t.s.destroy_object(3).map_err(|e| e.to_string())?;
    let again = t.s.destroy_object(3).map_err(|e| e.to_string())?;
    ensure(destroyed && !again, || {
        format!("destroys {destroyed} {again}")
    })?;
    run(
        &mut t.s,
        "5.6",
        r#"eq(binders.taken(), "net_destroy 3", "destroying alone")"#,
    )
}
