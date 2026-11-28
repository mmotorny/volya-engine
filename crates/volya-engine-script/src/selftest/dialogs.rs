// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The dialog files' functions as entry points
//! (docs/behavior/script-api-dialogs-and-trade.md 3.1 to 3.3,
//! docs/behavior/dialog-conditions.md 3 and 4.2) on every target, one
//! check per section, over a script of our own.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;

use super::ensure;
use super::files::{FSGAME, Tree};
use super::services::run;
use crate::entry_points;
use crate::files::FileSystem;
use crate::game_object::{GameObjects, ObjectClass, SpawnData};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::class_id;
use crate::services::EngineServices;
use crate::{Error, ErrorKind};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("R 3.2 preconditions", preconditions),
    ("R 3.3 actions", actions),
    ("R 3.1 failures", failures),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("dialog functions {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const PLAYER: u16 = 0;
const TRADER: u16 = 7;

/// Dialog functions noting their arguments in `calls`.
const TALK: &str = r##"
calls = {}
local function note(...)
  local t = {}
  for i = 1, select("#", ...) do
    local v = (select(i, ...))
    if type(v) == "userdata" then v = "obj" .. v:id() end
    t[i] = tostring(v)
  end
  table.insert(calls, table.concat(t, " "))
end
function taken() local c = table.concat(calls, ", ") calls = {} return c end
function noted(...) note(...) return true end
function two(first, second) note(first, second) return true end
function gives(value) return function() return value end end
yes, zero, empty, tbl = gives(true), gives(0), gives(""), gives({})
no, null = gives(false), gives(nil)
function nothing() end
function many(...) note(...) return 1, 2 end
function act(...) note("act", ...) return false end
function fails() error("dialog failed") end
"##;

fn data(id: u16) -> SpawnData {
    SpawnData {
        id,
        name: Vec::new(),
        section: b"unbound".to_vec(),
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

/// A state with the game object basics registered, `_G` loaded,
/// `talk.script` in the scripts and objects [`PLAYER`] and [`TRADER`]
/// spawned.
fn state() -> Result<Scripts, String> {
    let tree = Tree(
        [
            (
                "gamedata/scripts/_g.script",
                "function global_yes() return 1 end",
            ),
            ("gamedata/scripts/talk.script", TALK),
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
    let system = ltx::parse(b"[unbound]\r\n").map_err(|e| e.to_string())?;
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
    s.register_game_object_basics(objects, services, files)
        .map_err(|e| e.message)?;
    entry_points::install(s.lua_mut()).map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    for id in [PLAYER, TRADER] {
        s.spawn_engine_object(ObjectClass::Named("CSpaceRestrictor"), data(id))
            .map_err(|e| e.to_string())?;
    }
    Ok(s)
}

fn message(e: Error) -> String {
    e.message
}

/// R 3.2, `DG-2` 3 step 3: five arguments, the result's truth value.
pub fn preconditions() -> Result<(), String> {
    let mut s = state()?;
    let texts = ["talk_dialog", "0", "12"];
    let truth = s
        .call_precondition("talk.noted", PLAYER, TRADER, texts)
        .map_err(message)?;
    ensure(truth, || "a true result".into())?;
    s.call_precondition("talk.noted", TRADER, PLAYER, ["d", "", ""])
        .map_err(message)?;
    // A function declaring fewer parameters ignores the rest.
    s.call_precondition("talk.two", PLAYER, TRADER, texts)
        .map_err(message)?;
    for (function, expected) in [
        ("talk.yes", true),
        ("talk.zero", true),
        ("talk.empty", true),
        ("talk.tbl", true),
        ("talk.many", true),
        ("talk.no", false),
        ("talk.null", false),
        ("talk.nothing", false),
        ("_G.global_yes", true),
    ] {
        let truth = s
            .call_precondition(function, PLAYER, TRADER, texts)
            .map_err(message)?;
        ensure(truth == expected, || format!("{function}: {truth}"))?;
    }
    run(
        &mut s,
        "preconditions",
        r#"
eq(talk.taken(), "obj0 obj7 talk_dialog 0 12, obj7 obj0 d  , obj0 obj7, "
  .. "obj0 obj7 talk_dialog 0 12", "the handles, then the three texts")
"#,
    )
}

/// R 3.3, `DG-2` 4.2 step 2.3: four arguments, the result ignored.
pub fn actions() -> Result<(), String> {
    let mut s = state()?;
    s.call_action("talk.act", TRADER, PLAYER, "talk_dialog", "321")
        .map_err(message)?;
    s.call_action("talk.nothing", PLAYER, TRADER, "talk_dialog", "3")
        .map_err(message)?;
    run(
        &mut s,
        "actions",
        r#"eq(talk.taken(), "act obj7 obj0 talk_dialog 321", "the handles and the two texts")"#,
    )
}

/// R 3.1 (as disputed in its review), `files` 8: an error raised is fatal,
/// and so is a function no script defines (our reading of the believed
/// access fault).
pub fn failures() -> Result<(), String> {
    let mut s = state()?;
    let e = s
        .call_precondition("talk.fails", PLAYER, TRADER, ["d", "", ""])
        .unwrap_err();
    ensure(
        e.kind == ErrorKind::Fatal
            && e.message.starts_with("LUA error: ")
            && e.message.ends_with("dialog failed"),
        || format!("a failing precondition: {e:?}"),
    )?;
    let mut s = state()?;
    let e = s
        .call_action("talk.fails", PLAYER, TRADER, "d", "0")
        .unwrap_err();
    ensure(
        e.kind == ErrorKind::Fatal && e.message.ends_with("dialog failed"),
        || format!("a failing action: {e:?}"),
    )?;
    for missing in ["talk.nowhere", "nowhere.f", ""] {
        let mut s = state()?;
        let e = s
            .call_action(missing, PLAYER, TRADER, "d", "0")
            .unwrap_err();
        ensure(
            e.kind == ErrorKind::Fatal
                && e.message == format!("the dialog function {missing} is missing"),
            || format!("{missing:?}: {e:?}"),
        )?;
    }
    Ok(())
}
