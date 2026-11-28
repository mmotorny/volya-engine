// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-scripted-actions.md on every target, one check
//! per section, over objects of each scripted kind and sound files of our
//! own.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;

use super::dialogs_and_trade::data;
use super::ensure;
use super::files::{FSGAME, Tree};
use super::services::run;
use crate::dialogs::DialogsAndTrade;
use crate::files::FileSystem;
use crate::game_object::{GameObjects, ObjectClass};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::class_id;
use crate::scripted_actions::{
    ACT_CONSTANTS, ANIM_CONSTANTS, COND_CONSTANTS, LOOK_CONSTANTS, MOVE_CONSTANTS,
    OBJECT_CONSTANTS, SOUND_CONSTANTS,
};
use crate::services::EngineServices;
use crate::sound::{Request, ScriptSounds, SoundFile, SoundFiles};
use crate::story::Story;
use crate::{Error, ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2.1 scripted objects", scripted_objects),
    ("2.2 script control", script_control),
    ("2.3 the queue", queue),
    ("2.4 running the queue", running),
    ("3 entity_action", entity_action),
    ("3.3 completeness", completeness),
    ("4 move", movement),
    ("5 look", look),
    ("6 anim", anim),
    ("7 sound", sound),
    ("8 particle", particle),
    ("9 object", object),
    ("10 act", act),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("scripted actions {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const PLAYER: u16 = 0;
const STALKER: u16 = 30;
const DOG: u16 = 40;
const TRADER: u16 = 60;
const BOX: u16 = 70;
const CAR: u16 = 80;
const LIGHT: u16 = 90;

/// Our sound files: name and length.
const FILES: &[(&str, u64)] = &[("hello", 1000), ("door\\open", 1500)];

struct OurFiles;

impl SoundFiles for OurFiles {
    fn exists(&self, name: &[u8]) -> bool {
        FILES.iter().any(|(n, _)| n.as_bytes() == name)
    }

    fn load(&mut self, name: &[u8], _: Option<&str>) -> Result<SoundFile, String> {
        let name = String::from_utf8_lossy(name);
        match FILES.iter().find(|(n, _)| *n == name) {
            Some(&(_, ms)) => Ok(SoundFile {
                duration_ms: ms,
                min_distance: 2.0,
                max_distance: 40.0,
            }),
            None => Err(format!("{name}.ogg: cannot open sound file")),
        }
    }
}

const SYSTEM: &str = "[trade]\r\nbuy_price_factor_hostile = 1\r\nbuy_price_factor_friendly = 1\r\n\
sell_price_factor_hostile = 1\r\nsell_price_factor_friendly = 1\r\n";

struct State {
    s: Scripts,
    sounds: Rc<RefCell<ScriptSounds>>,
    services: Rc<RefCell<EngineServices>>,
    objects: Rc<RefCell<GameObjects>>,
}

fn message(e: Error) -> String {
    e.message
}

/// A state with everything up to the scripted actions registered, `_G`
/// loaded, a global `obj(id)` giving handles, and the player, a stalker,
/// a dog, the trader, a box, a car and a searchlight spawned.
fn state() -> Result<State, String> {
    let tree = Tree(
        [("gamedata/scripts/_g.script", "")]
            .iter()
            .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
            .collect::<BTreeMap<_, _>>(),
    );
    let tree = Rc::new(tree);
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
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree.clone(),
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .map_err(message)?;
    s.register_physics_vehicles_and_lamps().map_err(message)?;
    s.register_story(Rc::new(RefCell::new(Story::new())))
        .map_err(message)?;
    s.register_dialogs_and_trade(Rc::new(RefCell::new(DialogsAndTrade::new())))
        .map_err(message)?;
    let sounds = Rc::new(RefCell::new(ScriptSounds::new()));
    sounds.borrow_mut().files = Some(Box::new(OurFiles));
    s.register_sound(Rc::clone(&sounds), tree)
        .map_err(message)?;
    s.register_scripted_actions().map_err(message)?;
    let shared = Rc::clone(&objects);
    s.lua_mut()
        .register_object_function("obj", move |args| {
            let Some(Value::Number(id)) = args.first() else {
                return Ok(None);
            };
            Ok(shared.borrow().get(*id as u16).and_then(|o| o.handle()))
        })
        .map_err(message)?;
    crate::entry_points::install(s.lua_mut()).map_err(message)?;
    s.load_globals().map_err(message)?;
    for (class, mut data) in [
        (ObjectClass::Player, data(PLAYER, "actor", "O_ACTOR")),
        (
            ObjectClass::Named("CAI_Stalker"),
            data(STALKER, "stalker", "AI_STL_S"),
        ),
        (ObjectClass::Named("CAI_Dog"), data(DOG, "dog", "AI_DOG_R")),
        (
            ObjectClass::Named("CAI_Trader"),
            data(TRADER, "trader", "AI_TRADE"),
        ),
        (
            ObjectClass::Named("CPhysicObject"),
            data(BOX, "box", "O_PHYS_S"),
        ),
        (ObjectClass::Named("CCar"), data(CAR, "car", "SCRPTCAR")),
        (
            ObjectClass::Unnamed(class_id(b"O_SEARCH")),
            data(LIGHT, "projector", "O_SEARCH"),
        ),
    ] {
        if data.id == STALKER {
            data.placement = transform([1.0, 2.0, 3.0], [0.0; 3]);
        }
        s.spawn_engine_object(class, data)
            .map_err(|e| e.error.message)?;
    }
    Ok(State {
        s,
        sounds,
        services,
        objects,
    })
}

const PRELUDE: &str = "local actor, stalker, dog, trader, box, car, light = \
    obj(0), obj(30), obj(40), obj(60), obj(70), obj(80), obj(90)\n";

fn check_lua(t: &mut State, name: &str, body: &str) -> Result<(), String> {
    run(&mut t.s, name, &format!("{PRELUDE}{body}"))
}

/// The device's time set to `ms`.
fn at(t: &State, ms: u64) {
    t.services.borrow_mut().device.frame(ms, 0.0, false);
}

/// Object `id`'s queue run by the engine.
fn run_queue(t: &mut State, id: u16) -> Result<(), String> {
    t.s.run_action_queue(id).map_err(message)
}

/// `body` stops the state with a fatal error whose message holds `text`.
fn fatal(body: &str, text: &str) -> Result<(), String> {
    let mut t = state()?;
    let e =
        t.s.exec("=fatal", format!("{PRELUDE}{body}").as_bytes())
            .err();
    ensure(
        e.as_ref()
            .is_some_and(|e| e.kind == ErrorKind::Fatal && e.message.contains(text)),
        || format!("{body}: {e:?}"),
    )
}

/// 1.1, 1.2: the nine classes, their 88 constants and the nine members.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    let mut body = String::new();
    for (class, list) in [
        ("move", &MOVE_CONSTANTS[..]),
        ("look", &LOOK_CONSTANTS),
        ("anim", &ANIM_CONSTANTS),
        ("sound", &SOUND_CONSTANTS),
        ("object", &OBJECT_CONSTANTS),
        ("cond", &COND_CONSTANTS),
        ("act", &ACT_CONSTANTS),
    ] {
        for (name, value) in list {
            body.push_str(&format!(
                "eq({class}.{name}, {value}, \"{class}.{name}\")\n"
            ));
        }
    }
    body.push_str(
        r#"
        for _, c in ipairs({ "move", "look", "anim", "sound", "particle", "object", "cond",
          "act", "entity_action" }) do
          eq(type(_G[c]()), "userdata", c .. "()")
        end
        for _, n in ipairs({ "script", "get_script", "get_script_name", "can_script_capture",
          "command", "action", "action_count", "action_by_index", "reset_action_queue" }) do
          eq(type(game_object[n]), "function", n)
        end
        "#,
    );
    let count = [
        MOVE_CONSTANTS.len(),
        LOOK_CONSTANTS.len(),
        ANIM_CONSTANTS.len(),
        SOUND_CONSTANTS.len(),
        OBJECT_CONSTANTS.len(),
        COND_CONSTANTS.len(),
        ACT_CONSTANTS.len(),
    ]
    .iter()
    .sum::<usize>();
    ensure(count == 88, || format!("{count} constants"))?;
    check_lua(&mut t, "1", &body)
}

/// 2.1: the scripted kind, and the fallbacks of the members on others.
pub fn scripted_objects() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.1",
        r#"
        for _, o in ipairs({ actor, box }) do
          eq(o:get_script(), false, "get_script") eq(o:can_script_capture(), false, "capture")
          eq(o:get_script_name(), "", "name") eq(o:action(), nil, "action")
          eq(o:action_count(), 0, "count") eq(o:action_by_index(0), nil, "by index")
          o:script(true, "s") eq(o:get_script(), false, "not taken")
          o:command(entity_action(), false) eq(o:action_count(), 0, "not queued")
          o:reset_action_queue()
        end
        for _, o in ipairs({ stalker, dog, trader, car, light }) do
          eq(o:can_script_capture(), true, "capture at spawn")
          o:script(true, "s") eq(o:get_script(), true, "taken")
        end
        "#,
    )
}

/// 2.2: taking control, releasing it, and the invalid sequences.
pub fn script_control() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.2",
        r#"
        eq(dog:get_script(), false, "not at first") eq(dog:get_script_name(), "", "empty name")
        dog:script(false, "a") eq(dog:get_script(), false, "release without control")
        dog:script(true, "a") eq(dog:get_script(), true, "taken")
        eq(dog:get_script_name(), "a", "its name")
        dog:command(entity_action(), false) dog:command(entity_action(), false)
        dog:script(true, "b") eq(dog:get_script_name(), "a", "taken twice")
        dog:script(false, "b") eq(dog:get_script(), true, "another name")
        eq(dog:action_count(), 2, "queue kept")
        dog:script(false, "a") eq(dog:get_script(), false, "released")
        eq(dog:get_script_name(), "", "name emptied") eq(dog:action_count(), 0, "queue emptied")
        dog:script(true, "") dog:script(false, "") eq(dog:get_script(), true, "empty name")
        raises(function() dog:script(1, "a") end, "boolean expected", "on a number")
        "#,
    )?;
    // Without the capture flag, nothing happens.
    if let Some(o) = t.objects.borrow_mut().get_mut(STALKER)
        && let Some(c) = o.scripted.as_mut()
    {
        c.capture = false;
    }
    check_lua(
        &mut t,
        "2.2 capture",
        r#"
        stalker:script(true, "a") eq(stalker:get_script(), false, "no capture")
        eq(stalker:can_script_capture(), false, "the flag")
        "#,
    )
}

/// 2.3: `command`'s copies and interruptions, the readers, the reset.
pub fn queue() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.3",
        r#"
        trader:script(true, "q")
        local a = entity_action() a:set_action(cond(cond.time_end, 1000))
        trader:command(a, false)
        a:set_action(anim("idle"))
        eq(trader:action():anim(), true, "a copy queued")
        local b = entity_action() b:set_action(anim("talk"))
        trader:command(b, false) eq(trader:action_count(), 2, "added at the end")
        eq(trader:action_by_index(1):anim(), false, "the second")
        local c = entity_action() c:set_action(look(look.point))
        trader:command(c, true) eq(trader:action_count(), 3, "put first")
        eq(trader:action_by_index(0):look(), false, "the new current")
        eq(trader:action_by_index(1):anim(), true, "the interrupted one after it")
        local first = trader:action_by_index(0)
        first:set_action(look())
        eq(trader:action_by_index(0):look(), true, "itself, not a copy")
        local copy = trader:action() copy:set_action(look(look.point))
        eq(trader:action_by_index(0):look(), true, "action() a copy")
        trader:reset_action_queue()
        eq(trader:action_count(), 0, "reset") eq(trader:action(), nil, "none")
        eq(trader:get_script(), true, "control kept")
        "#,
    )?;
    fatal(
        "dog:script(true, \"q\") dog:command(entity_action(), false) dog:action_by_index(1)",
        "outside the queue",
    )
}

/// 2.4, 2.5: the engine's runs, the first test, the callbacks, monsters
/// only alive.
pub fn running() -> Result<(), String> {
    let mut t = state()?;
    at(&t, 5000);
    check_lua(
        &mut t,
        "2.4",
        r#"
        calls = {}
        trader:set_callback(callback.action_particle, function(o, n)
          calls[#calls + 1] = n end)
        trader:script(true, "r")
        local a = entity_action()
        trader:command(a, false)
        eq(#calls, 1, "run at once: the default particle part done")
        eq(calls[1], 4, "its number")
        eq(trader:action_count(), 1, "stays a run")
        eq(trader:action():time(), false, "no time limit")
        "#,
    )?;
    run_queue(&mut t, TRADER)?;
    check_lua(
        &mut t,
        "2.4 removed",
        r#"eq(trader:action_count(), 0, "complete on the second test") eq(#calls, 1, "once")"#,
    )?;
    // A time condition: the start time is now when it becomes current.
    check_lua(
        &mut t,
        "2.4 time",
        r#"
        local a = entity_action() a:set_action(cond(cond.time_end, 1000))
        trader:command(a, false)
        "#,
    )?;
    at(&t, 5500);
    run_queue(&mut t, TRADER)?;
    check_lua(
        &mut t,
        "2.4 not yet",
        r#"eq(trader:action_count(), 1, "before the limit") eq(trader:action():time(), false, "time")"#,
    )?;
    at(&t, 6001);
    check_lua(
        &mut t,
        "2.4 passed",
        r#"eq(trader:action_by_index(0):time(), true, "past the limit")"#,
    )?;
    run_queue(&mut t, TRADER)?;
    check_lua(
        &mut t,
        "2.4 done",
        r#"eq(trader:action_count(), 0, "removed once the limit passed")"#,
    )?;
    // Not under control: not run. A dead monster: not run.
    check_lua(
        &mut t,
        "2.4 control",
        r#"
        dog:script(true, "r") dog:command(entity_action(), false)
        eq(dog:action():particle(), true, "run at once")
        "#,
    )?;
    if let Some(o) = t.objects.borrow_mut().get_mut(DOG)
        && let Some(e) = o.entity.as_mut()
    {
        e.health = 0.0;
    }
    run_queue(&mut t, DOG)?;
    check_lua(
        &mut t,
        "2.4 dead",
        r#"eq(dog:action_count(), 1, "a dead monster doesn't run it")"#,
    )
}

/// 3.1, 3.2: construction, copies, the parts' done flags, `time`.
pub fn entity_action() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "3",
        r#"
        local a = entity_action()
        eq(a:move(), true, "move") eq(a:look(), true, "look") eq(a:anim(), true, "anim")
        eq(a:sound(), true, "sound") eq(a:particle(), false, "particle")
        eq(a:object(), true, "object") eq(a:time(), false, "time")
        a:set_action(move(move.fwd)) a:set_action(look(look.point)) a:set_action(anim("x"))
        a:set_action(object(object.idle)) a:set_action(act(act.rest))
        eq(a:move(), false, "move set") eq(a:look(), false, "look set")
        eq(a:anim(), false, "anim set") eq(a:object(), false, "object set")
        local b = entity_action(a)
        eq(b:move(), false, "copied")
        b:set_action(move())
        eq(a:move(), false, "a copy") eq(b:move(), true, "set twice keeps the last")
        local c = entity_action(nil)
        eq(c:move(), true, "nil gives a default one")
        raises(function() a:set_action(1) end, "", "not a part")
        "#,
    )
}

/// 3.3: the completeness test.
pub fn completeness() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "3.3",
        r#"
        local a = entity_action()
        eq(a:completed(), false, "first test") eq(a:all(), true, "set 0: the second")
        local b = entity_action() b:set_action(cond(cond.move_end + cond.look_end))
        b:set_action(move(move.fwd))
        eq(b:completed(), false, "first") eq(b:completed(), false, "move not done")
        b:set_action(move()) eq(b:completed(), true, "both hold")
        local c = entity_action() c:set_action(cond(256))
        c:completed() eq(c:completed(), false, "a bit above 128 never")
        local d = entity_action() d:set_action(cond(16))
        d:completed() eq(d:completed(), false, "bit 16: the particle part")
        "#,
    )
}

/// 4: `move`'s forms and setters.
pub fn movement() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "4",
        r#"
        local v = vector():set(1, 2, 3)
        eq(move():completed(), true, "default done")
        for i, m in ipairs({ move(move.fwd), move(move.fwd, 2), move(move.standing, move.walk,
          move.line, dog), move(move.crouch, move.run, move.line, nil, 1),
          move(move.standing, move.walk, move.line, v), move(move.standing, move.walk, move.line, v, 1),
          move(v, 3), move(move.walk_fwd, v), move(move.walk_fwd, dog), move(move.run_fwd, v, 2),
          move(move.run_fwd, dog, 2), move(move.drag, 5, v), move(move.drag, 5, v, 1),
          move(move.steal, v, 1, move.force), move(move.steal, dog, 1, move.force) }) do
          eq(m:completed(), false, "form " .. i)
        end
        for _, s in ipairs({ "body", "move", "path", "input" }) do
          local m = move() m[s](m, 1) eq(m:completed(), false, s)
        end
        local m = move() m:object(dog) eq(m:completed(), false, "object")
        m = move() m:position(v) eq(m:completed(), false, "position")
        raises(function() move():patrol(nil, "x") end, "", "patrol")
        raises(function() return move("x") end, "", "no form")
        "#,
    )?;
    // Applying it: a stalker's input keys stop it, done; the trader's
    // never becomes done.
    check_lua(
        &mut t,
        "4.4",
        r#"
        for _, o in ipairs({ stalker, trader }) do
          o:script(true, "m")
          local a = entity_action() a:set_action(move(move.fwd))
          a:set_action(cond(cond.move_end)) o:command(a, false)
        end
        eq(stalker:action():move(), true, "stalker: input keys done")
        eq(trader:action():move(), false, "trader: never done")
        "#,
    )
}

/// 5: `look`'s forms and setters.
pub fn look() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "5",
        r#"
        local v = vector():set(1, 2, 3)
        eq(look():completed(), true, "default done")
        for i, l in ipairs({ look(look.point), look(look.direction, v), look(look.point, dog),
          look(look.point, dog, "bip01_head"), look(v, 1, 2), look(dog, 1, 2), look(nil, 1, 2) }) do
          eq(l:completed(), false, "form " .. i)
        end
        for _, s in ipairs({ "type", "bone" }) do
          local l = look() l[s](l, s == "type" and 1 or "b") eq(l:completed(), false, s)
        end
        local l = look() l:object(dog) eq(l:completed(), false, "object")
        l = look() l:direct(v) eq(l:completed(), false, "direct")
        "#,
    )
}

/// 6: `anim`'s forms, a mental state's part done.
pub fn anim() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "6",
        r#"
        eq(anim():completed(), true, "default done")
        eq(anim("a"):completed(), false, "a name")
        eq(anim("a", true):completed(), false, "a name and the flag")
        eq(anim(anim.free):completed(), true, "a mental state: done")
        eq(anim(anim.stand_idle, 0):completed(), false, "a monster animation")
        local a = anim() a:anim("b") eq(a:completed(), false, "anim clears")
        a:type(anim.danger) eq(a:completed(), true, "type sets")
        "#,
    )
}

/// 7: `sound`'s forms, the missing file, applying it (7.2), finishing.
pub fn sound() -> Result<(), String> {
    let mut t = state()?;
    let v = "vector():set(1, 2, 3)";
    check_lua(
        &mut t,
        "7.1",
        &format!(
            r#"
            local v = {v}
            eq(sound():completed(), true, "default done")
            for i, s in ipairs({{ sound("hello", "bip01_head"), sound("hello", "b", v),
              sound("hello", "b", v, v), sound("hello", "b", v, v, true), sound("hello", v),
              sound("hello", v, v), sound("hello", v, v, true), sound("hello", "b", 1),
              sound(sound.idle), sound(sound.attack, 100) }}) do
              eq(s:completed(), false, "form " .. i)
            end
            eq(sound("missing", v):completed(), true, "a missing file: done")
            local s = sound("hello", v) s:set_sound("missing") eq(s:completed(), true, "set_sound")
            raises(function() return sound(sound_object("hello"), v) end, "sound_object",
              "the sound_object forms")
            "#
        ),
    )?;
    // 7.1's sound object made a sound.
    t.sounds.borrow_mut().take_requests();
    at(&t, 1000);
    check_lua(
        &mut t,
        "7.2",
        &format!(
            r#"
            calls = {{}}
            stalker:set_callback(callback.action_sound, function(o, n) calls[#calls + 1] = n end)
            stalker:script(true, "s")
            local a = entity_action() a:set_action(sound("hello", {v}, {v}, true))
            a:set_action(particle("p", "b"))
            a:set_action(cond(cond.sound_end)) stalker:command(a, false)
            "#
        ),
    )?;
    let made: Vec<_> = t.sounds.borrow_mut().take_requests();
    ensure(
        matches!(&made[..], [Request::Create { name, .. }] if name == "hello"),
        || format!("the sound made first: {made:?}"),
    )?;
    run_queue(&mut t, STALKER)?;
    let played = t.sounds.borrow_mut().take_requests();
    ensure(
        matches!(&played[..], [Request::Play { flags, position: Some([1.0, 2.0, 3.0]), owner, .. }]
            if flags.looped && *owner == STALKER),
        || format!("played at the position: {played:?}"),
    )?;
    run_queue(&mut t, STALKER)?;
    ensure(t.sounds.borrow_mut().take_requests().is_empty(), || {
        "playing: nothing".into()
    })?;
    // The sound ends; done on the next run, removed on the one after,
    // which releases the sound.
    t.sounds.borrow_mut().frame_without_system();
    run_queue(&mut t, STALKER)?;
    check_lua(
        &mut t,
        "7.2 done",
        r#"eq(stalker:action():sound(), true, "done") eq(#calls, 1, "callback 21")
           eq(calls[1], 3, "its number")"#,
    )?;
    run_queue(&mut t, STALKER)?;
    let released = t.sounds.borrow_mut().take_requests();
    ensure(matches!(&released[..], [Request::Discard(_)]), || {
        format!("released when finished: {released:?}")
    })?;
    check_lua(
        &mut t,
        "7.2 removed",
        r#"eq(stalker:action_count(), 0, "removed")"#,
    )
}

/// 8: `particle`'s forms and setters, done the run after it starts.
pub fn particle() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "8",
        r#"
        eq(particle():completed(), false, "default not done")
        eq(particle("p", "b"):completed(), false, "a name and a bone")
        for _, s in ipairs({ "set_bone", "set_position", "set_angles", "set_velocity" }) do
          local p = particle()
          if s == "set_bone" then p[s](p, "b") else p[s](p, vector()) end
          eq(p:completed(), false, s)
        end
        local p = particle() p:set_particle("p", false) eq(p:completed(), false, "set_particle")
        dog:script(true, "p")
        local a = entity_action() a:set_action(particle("p", "b"))
        a:set_action(cond(cond.time_end, 100000)) dog:command(a, false)
        eq(dog:action():particle(), false, "started")
        "#,
    )?;
    run_queue(&mut t, DOG)?;
    check_lua(
        &mut t,
        "8 done",
        r#"eq(dog:action():particle(), true, "done the run after")"#,
    )
}

/// 9: `object`'s forms and setters; the car's part with no name done.
pub fn object() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "9",
        r#"
        eq(object():completed(), true, "default done")
        for i, o in ipairs({ object(box, object.use), object(box, object.fire1, 2),
          object(object.idle), object("phy_door_left", object.activate), object(nil, 1) }) do
          eq(o:completed(), false, "form " .. i)
        end
        local o = object() o:object("x") eq(o:completed(), false, "object(name)")
        o = object() o:object(box) eq(o:completed(), false, "object(obj)")
        o = object() o:action(object.use) eq(o:completed(), false, "action")
        car:script(true, "c")
        local a = entity_action() a:set_action(object(object.use))
        a:set_action(cond(cond.object_end)) car:command(a, false)
        eq(car:action():object(), true, "the car: an empty name done")
        "#,
    )
}

/// 10: `act`'s forms; applying it never makes it done.
pub fn act() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "10",
        r#"
        local a = entity_action() a:set_action(act()) a:set_action(cond(cond.act_end))
        a:completed() eq(a:completed(), true, "act() done")
        local b = entity_action() b:set_action(act(act.attack, actor))
        b:set_action(cond(cond.act_end)) b:completed() eq(b:completed(), false, "act(a, obj)")
        local c = entity_action() c:set_action(act(act.rest)) c:set_action(cond(cond.act_end))
        c:completed() eq(c:completed(), false, "act(a)")
        "#,
    )
}
