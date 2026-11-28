// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-class-ids-and-the-object-factory.md on every
//! target, one check per section, with scripts and test engine classes of
//! our own (no engine class of the game exists here).

use super::classes::{PRELUDE, failures_of};
use super::ensure;
use super::loading::scripts_of;
use crate::classes::EngineClass;
use crate::loader::{Common, Prefix, Scripts};
use crate::object_factory::{
    Created, CreatedObject, ENGINE_ENTRIES, Entry, Form, GameTypeKind, ObjectFactory, Sides,
    Source, class_id, id_text, keeps_lua,
};
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 class ids", class_ids),
    ("2 the object factory", factory),
    ("3 register", register),
    ("4 class_registrators", registrators),
    ("5 clsid", clsid),
    ("6 creation", creation),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("object factory {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Section 1: the id of a text, its reverse, the order.
pub fn class_ids() -> Result<(), String> {
    let cases: &[(&[u8], u64)] = &[
        (b"AI_STL", 0x4149_5F53_544C_2020),
        (b"", 0x2020_2020_2020_2020),
        (b"P_SKELET", u64::from_be_bytes(*b"P_SKELET")),
        (b"LONGER_THAN_8", u64::from_be_bytes(*b"LONGER_T")),
        // A byte of 80-FF makes every more significant byte FF.
        (b"ABCDEFG\x80", 0xFFFF_FFFF_FFFF_FF80),
        (b"AB\xC0", 0xFFFF_C020_2020_2020),
    ];
    for &(text, id) in cases {
        let got = class_id(text);
        ensure(got == id, || {
            format!("{}: {got:#x}, want {id:#x}", String::from_utf8_lossy(text))
        })?;
    }
    ensure(id_text(class_id(b"AI_STL")) == *b"AI_STL  ", || {
        "the reverse keeps the padding".into()
    })?;
    // A space sorts below the other characters: AI_STL < AI_STL_S.
    ensure(class_id(b"AI_STL") < class_id(b"AI_STL_S"), || {
        "AI_STL sorts before AI_STL_S".into()
    })?;
    ensure(
        (class_id(b"Z_ZONE") < class_id(b"ZS_BFUZZ")) == (b'_' < b'S'),
        || "ids order as their padded texts".into(),
    )?;
    ensure(
        keeps_lua("XR_3DA.exe -KEEP_LUA") && keeps_lua("-x-keep_luax") && !keeps_lua("-keep"),
        || "-keep_lua is a substring of the lowercased command line".into(),
    )
}

/// Section 2: the engine's entries, adding, order, lookup and numbering.
pub fn factory() -> Result<(), String> {
    let mut f = ObjectFactory::new();
    let count = |s: Sides| ENGINE_ENTRIES.iter().filter(|e| e.2 == s).count();
    ensure(
        (
            f.entries().len(),
            count(Sides::ClientAndServer),
            count(Sides::ByGameType),
            count(Sides::ClientOnly),
            count(Sides::ServerOnly),
        ) == (134, 115, 1, 15, 3),
        || "134 engine entries: 115 both sides, 1 by game type, 15 client, 3 server".into(),
    )?;
    let ids: Vec<u64> = f.entries().iter().map(|e| e.id).collect();
    ensure(ids.windows(2).all(|w| w[0] < w[1]), || {
        "distinct ids in ascending order".into()
    })?;
    ensure(
        GameTypeKind::of(0) == GameTypeKind::ZeroOrOne
            && GameTypeKind::of(1) == GameTypeKind::ZeroOrOne
            && GameTypeKind::of(2) == GameTypeKind::Other,
        || "O_ACTOR's kinds: game type 0 or 1, and the others".into(),
    )?;
    // Numbers: a registered id's own position; an unregistered id's the
    // next larger's; past the end, the count.
    let stalker = f.number(class_id(b"AI_STL"));
    ensure(stalker == 32, || format!("AI_STL is {stalker}, want 32"))?;
    let rat = class_id(b"AI_RAT");
    let next = f.entry_for(rat).map(|e| e.id_text());
    ensure(next.as_deref() == Some("AI_SNORK"), || {
        format!("AI_RAT's entry: {next:?}")
    })?;
    ensure(f.number(rat) == f.number(class_id(b"AI_SNORK")), || {
        "an unregistered id has the next larger id's number".into()
    })?;
    let last = class_id(b"\x7F");
    ensure(f.number(last) == 134 && f.entry_for(last).is_none(), || {
        "past the end: the entry count, no entry".into()
    })?;
    // Adding marks the table unsorted; an existing id is kept beside the
    // first, after it.
    let again = |slot| Entry {
        id: class_id(b"AI_STL"),
        name: b"again".to_vec(),
        source: Source::Script {
            form: Form::ThreeArguments,
            slot,
        },
    };
    f.add(again(1));
    f.add(Entry {
        id: class_id(b"AAA"),
        name: b"first".to_vec(),
        source: Source::Engine(Sides::ClientOnly),
    });
    let names: Vec<&[u8]> = f.entries().iter().map(|e| e.name.as_slice()).collect();
    ensure(
        names.len() == 136
            && names[0] == b"first"
            && names[33] == b"stalker"
            && names[34] == b"again",
        || "added entries are sorted in, a repeated id after the first".into(),
    )?;
    ensure(
        f.entry_for(class_id(b"AI_STL")).map(|e| e.name.as_slice()) == Some(b"stalker"),
        || "the entry for a repeated id is the first".into(),
    )
}

/// Test engine classes: `test_client` (a constructor of no arguments) and
/// `test_server` (one of one), whose parts record their arguments.
fn test_classes() -> Vec<EngineClass> {
    let made = |args: &[Value]| -> Vec<Value> { args.to_vec() };
    vec![
        EngineClass::new("test_client")
            .constructor(0, |_| Some(0), move |_, args| Ok(made(args)))
            .method("args", |p: &mut Vec<Value>, _| {
                Ok(vec![(p.len() as f64).into()])
            }),
        EngineClass::new("test_server").constructor(1, |_| Some(0), move |_, args| Ok(made(args))),
    ]
}

/// The registrators and the classes they name.
const SCRIPTS: &[(&str, &str)] = &[
    (
        "class_registrator.script",
        r#"
calls = {}
function register(f)
  seen_clsid = clsid
  -- 4-argument form: an engine client class, a script server class.
  f:register("test_client", "srv.server_obj", "TS_SRV", "test_srv")
  -- 3-argument form: a script class as both sides.
  f:register("cl.client_obj", "CL_ONLY", "client_only")
  -- Failed resolutions add nothing.
  f:register("missing_ns.thing", "srv.server_obj", "TS_MISS1", "miss1")
  f:register("test_client", "srv.nothing", "TS_MISS2", "miss2")
  f:register("cl.not_class", "TS_MISS3", "miss3")
  f:register("cl.test_client", "TS_MISS4", "miss4")
  f:register("cl.no_part", "srv.no_part", "TS_NOPRT", "no_part")
  f:register("cl.raises", "srv.raises", "TS_RAISE", "raises")
  f:register("cl.number", "srv.number", "TS_NUMBR", "number")
  table.insert(calls, "class_registrator")
end
"#,
    ),
    (
        "game_registrator.script",
        r#"
function register(f)
  table.insert(class_registrator.calls, "game_registrator")
  local ok, e = pcall(f.register, f, 1, "TS_NUM", "num")
  class_registrator.number_argument = ok
  ok = pcall(f.register, f, "test_client", "TS_TWO")
  class_registrator.two_arguments = ok
  ok = pcall(f.register, {}, "test_client", "TS_SELF", "self")
  class_registrator.other_receiver = ok
  f:register("test_server", "SV_ENGIN", "server_engine")
end
not_a_function = 5
"#,
    ),
    (
        "cl.script",
        r#"
class "client_obj" (test_client)
function client_obj:__init(...) super(...) self.made = select('#', ...) end
class "no_part" (test_client)
function no_part:__init() end
class "raises" (test_client)
function raises:__init() error("raised in __init", 0) end
not_class = 5
number = newproxy()
"#,
    ),
    (
        "srv.script",
        r#"
class "server_obj" (test_server)
function server_obj:__init(section) super(section) self.section = section end
class "no_part" (test_server)
function no_part:__init(section) end
class "raises" (test_server)
function raises:__init(section) error("raised in __init", 0) end
-- A callable value that isn't a class: its result isn't an object.
number = newproxy(true)
getmetatable(number).__call = function() return 5 end
"#,
    ),
    (
        "bad.script",
        "function register(f) error('bad registrator', 0) end",
    ),
];

/// A state with the test engine classes, `_G` loaded and the registrators
/// of `names` called.
fn started(names: &[&str]) -> Result<Scripts, String> {
    let (mut s, _) = scripts_of(SCRIPTS, Prefix::Default)?;
    for class in test_classes() {
        s.register_class(class).map_err(|e| e.message)?;
    }
    s.load_globals().map_err(|e| e.message)?;
    let common = Common {
        class_registrators: names.iter().map(|n| (*n).to_owned()).collect(),
        scripts: Vec::new(),
        game_type_function: None,
    };
    s.call_class_registrators(&common).map_err(|e| e.message)?;
    Ok(s)
}

const SHIPPED: &[&str] = &["class_registrator.register", "game_registrator.register"];

/// Runs `body` after the prelude on `s`.
fn run(s: &mut Scripts, name: &str, body: &str) -> Result<(), String> {
    let code = format!("{PRELUDE}{body}\nreturn failures");
    let results = s
        .exec(&format!("={name}"), code.as_bytes())
        .map_err(|e| format!("{name}: {e}"))?;
    failures_of(name, &results)
}

/// Section 3: `register` in both forms, resolution, failures.
pub fn register() -> Result<(), String> {
    let mut s = started(SHIPPED)?;
    let entries: Vec<(String, Vec<u8>, Source)> = s
        .object_factory()
        .entries()
        .iter()
        .filter(|e| matches!(e.source, Source::Script { .. }))
        .map(|e| (e.id_text(), e.name.clone(), e.source))
        .collect();
    let got: Vec<(&str, &[u8], Form)> = entries
        .iter()
        .map(|(id, name, source)| {
            let Source::Script { form, .. } = source else {
                unreachable!()
            };
            (id.as_str(), name.as_slice(), *form)
        })
        .collect();
    let want: Vec<(&str, &[u8], Form)> = vec![
        ("CL_ONLY", b"client_only", Form::ThreeArguments),
        ("SV_ENGIN", b"server_engine", Form::ThreeArguments),
        ("TS_NOPRT", b"no_part", Form::FourArguments),
        ("TS_NUMBR", b"number", Form::FourArguments),
        ("TS_RAISE", b"raises", Form::FourArguments),
        ("TS_SRV", b"test_srv", Form::FourArguments),
    ];
    ensure(got == want, || format!("script entries: {got:?}"))?;
    run(
        &mut s,
        "3",
        r#"
eq(class_registrator.number_argument, false, "a number in place of a string raises")
eq(class_registrator.two_arguments, false, "two arguments match no form")
eq(class_registrator.other_receiver, false, "another receiver raises")
eq(type(object_factory), "userdata", "object_factory is a class")
raises(function() object_factory() end, "", "object_factory has no constructor")
eq(rawget(_G, "missing_ns"), nil, "a missing namespace file loads nothing")
eq(type(rawget(_G, "srv")), "table", "a server class's namespace is loaded")
"#,
    )
}

/// Section 4: the registrators' order, skipping, and the fatal error.
pub fn registrators() -> Result<(), String> {
    let mut s = started(&[
        "missing.register",
        "game_registrator.not_a_function",
        "class_registrator.register",
        "game_registrator.register",
        "no_dot_at_all",
    ])?;
    run(
        &mut s,
        "4",
        r#"
local calls = class_registrator.calls
eq(#calls, 2, "both registrators called")
eq(calls[1], "class_registrator", "in the list's order")
eq(calls[2], "game_registrator", "in the list's order")
"#,
    )?;
    let (mut s, _) = scripts_of(SCRIPTS, Prefix::Default).map_err(|e| e.to_string())?;
    s.load_globals().map_err(|e| e.message)?;
    let common = Common {
        class_registrators: vec!["bad.register".into()],
        scripts: Vec::new(),
        game_type_function: None,
    };
    let failed = s.call_class_registrators(&common);
    ensure(
        matches!(&failed, Err(e) if e.kind == ErrorKind::Fatal && e.message == "LUA error: bad registrator"),
        || format!("a script error: {failed:?}"),
    )?;
    let after = s.exec("=after", b"return 1");
    ensure(
        matches!(&after, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("the state is stopped: {after:?}"),
    )
}

/// Section 5: `clsid`, after the registrators, numbered in order.
pub fn clsid() -> Result<(), String> {
    let mut s = started(SHIPPED)?;
    let number = |s: &mut Scripts, text: &[u8]| s.object_factory().number(class_id(text));
    let srv = number(&mut s, b"TS_SRV");
    let only = number(&mut s, b"CL_ONLY");
    let zone = number(&mut s, b"Z_ZONE");
    ensure(zone == 134 + 6 - 1, || format!("Z_ZONE is {zone}"))?;
    run(
        &mut s,
        "5",
        &format!(
            r#"
eq(class_registrator.seen_clsid, nil, "clsid doesn't exist during registration")
eq(clsid.test_srv, {srv}, "a registered name's value is its position")
eq(clsid.client_only, {only}, "a 3-argument entry's name")
eq(clsid.zone, {zone}, "the last entry")
eq(clsid.art_bast_artefact, 0, "the first entry")
eq(clsid.miss1, nil, "a failed registration has no name")
eq(clsid.ZONE, nil, "names compare exactly")
eq(clsid.no_such_name, nil, "a missing name reads as nil")
raises(function() clsid() end, "", "clsid has no constructor")
"#
        ),
    )
}

fn created_part(c: &Result<Created, crate::Error>) -> Option<crate::classes::EnginePart> {
    match c {
        Ok(Created {
            object: CreatedObject::Script(part),
            ..
        }) => Some(*part),
        _ => None,
    }
}

/// Section 6: client and server objects, engine and script entries.
pub fn creation() -> Result<(), String> {
    let mut s = started(SHIPPED)?;
    // Engine entries: the kind for the caller to make; a missing side is
    // fatal.
    let stalker = s.create_client_object(class_id(b"AI_STL"));
    ensure(
        stalker
            == Ok(Created {
                class_id: class_id(b"AI_STL"),
                object: CreatedObject::Engine {
                    entry_id: class_id(b"AI_STL"),
                    sides: Sides::ClientAndServer,
                },
            }),
        || format!("an engine entry: {stalker:?}"),
    )?;
    let rat = s.create_client_object(class_id(b"AI_RAT"));
    ensure(
        matches!(&rat, Ok(Created { class_id: id, object: CreatedObject::Engine { entry_id, .. } })
            if *id == class_id(b"AI_RAT") && *entry_id == class_id(b"AI_SNORK")),
        || format!("an unregistered id uses the next entry, keeping its id: {rat:?}"),
    )?;

    // A script client class: constructed without arguments.
    let client = s.create_client_object(class_id(b"CL_ONLY"));
    let part = created_part(&client).ok_or_else(|| format!("CL_ONLY: {client:?}"))?;
    let args = s.with_part(part, |p: &mut Vec<Value>| p.len());
    ensure(args == Some(0), || format!("no arguments: {args:?}"))?;
    // TODO(#935): gap 1; an engine client class constructs its engine
    // object with its constructor of no arguments.
    let engine = s.create_client_object(class_id(b"TS_SRV"));
    let engine_part = created_part(&engine).ok_or_else(|| format!("TS_SRV: {engine:?}"))?;
    ensure(
        s.with_part(engine_part, |p: &mut Vec<Value>| p.len()) == Some(0),
        || "an engine client class's object".into(),
    )?;

    // A script server class: called with the section.
    let system = volya_engine_formats::ltx::parse(
        b"[srv_section]\r\nclass = TS_SRV\r\n[engine_section]\r\nclass = AI_STL\r\n\
          [client_only]\r\nclass = HUD_MNGR\r\n[server_only]\r\nclass = AI_GRAPH\r\n\
          [no_part]\r\nclass = TS_NOPRT\r\n[raises]\r\nclass = TS_RAISE\r\n\
          [number]\r\nclass = TS_NUMBR\r\n[no_class]\r\n",
    )
    .map_err(|e| e.to_string())?;
    let server = s.create_server_object(&system, "srv_section");
    let part = created_part(&server).ok_or_else(|| format!("srv_section: {server:?}"))?;
    let args = s.with_part(part, |p: &mut Vec<Value>| p.clone());
    ensure(args == Some(vec!["srv_section".into()]), || {
        format!("the section is the argument: {args:?}")
    })?;
    // The engine holds the object, whatever scripts drop.
    s.exec("=gc", b"collectgarbage() collectgarbage()")
        .map_err(|e| e.message)?;
    ensure(s.with_part(part, |_: &mut Vec<Value>| ()).is_some(), || {
        "the engine part outlives the script's references".into()
    })?;
    ensure(s.release_object(part) == Ok(true), || "released".into())?;
    ensure(s.with_part(part, |_: &mut Vec<Value>| ()).is_none(), || {
        "the released part is gone".into()
    })?;
    ensure(s.release_object(part) == Ok(false), || {
        "released once".into()
    })?;
    let engine_server = s.create_server_object(&system, "engine_section");
    ensure(
        matches!(
            &engine_server,
            Ok(Created {
                object: CreatedObject::Engine { .. },
                ..
            })
        ),
        || format!("an engine server entry: {engine_server:?}"),
    )?;
    let no_class = s.create_server_object(&system, "no_class");
    ensure(
        matches!(&no_class, Err(e) if e.kind == ErrorKind::Api),
        || format!("a section without a class: {no_class:?}"),
    )?;

    // The fatal outcomes, each on a fresh state.
    let fatal_cases: &[(&str, &str, bool)] = &[
        (
            "client_only",
            "Cannot instantiate server object, because server class is not declared!",
            true,
        ),
        (
            "server_only",
            "Cannot instantiate client object, because client class is not declared!",
            false,
        ),
        (
            "no_part",
            "the server class of TS_NOPRT made no object",
            true,
        ),
        ("raises", "LUA error: raised in __init", true),
        ("number", "LUA error: cannot cast lua value to ?", true),
    ];
    for &(section, message, server_side) in fatal_cases {
        let mut s = started(SHIPPED)?;
        let result = if server_side {
            s.create_server_object(&system, section)
        } else {
            let class = system.get(section, "class").flatten().unwrap_or("");
            s.create_client_object(class_id(class.as_bytes()))
        };
        ensure(
            matches!(&result, Err(e) if e.kind == ErrorKind::Fatal && e.message.contains(message)),
            || format!("{section}: {result:?}"),
        )?;
        ensure(s.exec("=after", b"return 1").is_err(), || {
            format!("{section}: the state is stopped")
        })?;
    }
    Ok(())
}
