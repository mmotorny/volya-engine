// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's use of the game object basics (BYOA), checked
//! against docs/behavior/script-api-game-object-basics.md: the 250
//! loadable scripts (docs/behavior/script-loading.md 3, recomputed here)
//! and their uses of this spec's names (13), the one script class deriving
//! from `CGameObject` and its class id (10.2), the names the scripts use
//! that aren't registered (11), `hit.dummy` (9), every loadable script
//! loaded with the names registered, and the door scheme's calls on the
//! trader door's handle, spawned through the object factory from its spawn
//! record (12). Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::ops::ControlFlow;
use std::rc::Rc;

use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::simulation::class_of;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::object::{ObjectRecord, section};
use volya_engine_formats::spawn;
use volya_engine_script::Value;
use volya_engine_script::classes::EngineClass;
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{
    ClientObject, GameObjects, ObjectClass, SpawnData, UseAction,
};
use volya_engine_script::loader::{Common, FailureKind, Prefix, Scripts};
use volya_engine_script::object_factory::{CreatedObject, class_id};
use volya_engine_script::services::EngineServices;

use shipped_scripts::{ENGINE_BASES, GameTrees, Token, tokens};

/// The aliases scripts use, with the saved-games folder.
const FSGAME: &[u8] = b"$app_data_root$ = true | false| {userdocs}\\Stalker-SHOC\r\n\
    $game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n\
    $game_saves$ =    true | false| $app_data_root$| savedgames\\\r\n";

/// Every file under `dir`, `/`-separated.
fn walk(vfs: &dyn Vfs, dir: &str, out: &mut Vec<String>) {
    for e in vfs.read_dir(dir).unwrap() {
        let path = if dir.is_empty() {
            e.name.clone()
        } else {
            format!("{dir}/{}", e.name)
        };
        match e.kind {
            EntryKind::Dir => walk(vfs, &path, out),
            EntryKind::File => out.push(path),
        }
    }
}

/// The words of `bytes`: runs of letters, digits and `_` not starting with
/// a digit, lowercase (script-loading.md 3).
fn words(bytes: &[u8], out: &mut BTreeSet<String>) {
    for run in bytes.split(|b| !(b.is_ascii_alphanumeric() || *b == b'_')) {
        if run.first().is_some_and(|b| !b.is_ascii_digit()) {
            out.insert(String::from_utf8_lossy(run).to_ascii_lowercase());
        }
    }
}

/// A script's words after comment removal, string contents kept.
fn script_words(tokens: &[Token]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in tokens {
        match t {
            Token::Name(n) => {
                out.insert(n.to_ascii_lowercase());
            }
            Token::String(s) => words(s, &mut out),
            _ => {}
        }
    }
    out
}

/// The 250 loadable scripts of script-loading.md 3, every script's tokens
/// by name, and xrGame.dll's bytes.
fn loadable(base: &dyn Vfs) -> (BTreeSet<String>, BTreeMap<String, Vec<Token>>, Vec<u8>) {
    let mut files = Vec::new();
    walk(base, "", &mut files);
    let lower: Vec<(String, &String)> = files.iter().map(|f| (f.to_ascii_lowercase(), f)).collect();
    let ext = |f: &str, exts: &[&str]| exts.iter().any(|e| f.ends_with(e));
    let roots_files: Vec<&String> = lower
        .iter()
        .filter(|(l, _)| {
            (l.starts_with("gamedata/config/") && ext(l, &[".ltx", ".xml"]))
                || (!l.starts_with("gamedata/config/")
                    && !l.starts_with("gamedata/scripts/")
                    && ext(l, &[".ltx", ".ini", ".spawn", ".game"]))
        })
        .map(|(_, p)| *p)
        .collect();
    assert_eq!(roots_files.len(), 893 + 195, "root files");
    let mut roots = BTreeSet::from(["_g".to_owned()]);
    for path in roots_files {
        words(&read(base, path).unwrap(), &mut roots);
    }
    let binaries = gog_archives::game_files("binaries", |p| {
        let p = p.to_ascii_lowercase();
        p == "bin/xrgame.dll" || p == "bin/xr_3da.exe"
    })
    .unwrap();
    assert_eq!(binaries.len(), 2, "xrGame.dll and XR_3DA.exe");
    let mut game_library = Vec::new();
    for binary in &binaries {
        let bytes = fs::read(&binary.file).unwrap();
        words(&bytes, &mut roots);
        if binary
            .file
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with("xrgame.dll")
        {
            game_library = bytes;
        }
    }
    assert_eq!(game_library.len(), 6_246_400, "xrGame.dll");
    let mut scripts = BTreeMap::new();
    for (l, path) in &lower {
        if let Some(name) = l
            .strip_prefix("gamedata/scripts/")
            .and_then(|n| n.strip_suffix(".script"))
        {
            scripts.insert(name.to_owned(), tokens(&read(base, path).unwrap()));
        }
    }
    let mut reached = BTreeSet::new();
    let mut pending: Vec<String> = scripts
        .keys()
        .filter(|n| roots.contains(*n))
        .cloned()
        .collect();
    while let Some(name) = pending.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let found = script_words(&scripts[&name]);
        pending.extend(
            scripts
                .keys()
                .filter(|n| found.contains(*n) && !reached.contains(*n))
                .cloned(),
        );
    }
    assert_eq!(reached.len(), 250, "loadable scripts");
    (reached, scripts, game_library)
}

/// This spec's classes (1.1, 10.3).
fn classes() -> Vec<&'static str> {
    let mut out = vec!["game_object", "hit", "CGameObject", "explosive"];
    out.extend(volya_engine_script::game_object::client_classes());
    out.extend(["ce_script_zone", "ce_smart_zone"]);
    out
}

/// The methods of this spec's classes: `game_object`'s 46 (3–7), `hit`'s
/// `bone`, `CGameObject`'s 7, `explode`.
const METHODS: &[&str] = &[
    "id",
    "name",
    "section",
    "clsid",
    "story_id",
    "spawn_ini",
    "parent",
    "position",
    "direction",
    "center",
    "bone_position",
    "level_vertex_id",
    "game_vertex_id",
    "level_vertex_light",
    "set_actor_position",
    "set_actor_direction",
    "alive",
    "max_health",
    "death_time",
    "kill",
    "get_bleeding",
    "who_hit_name",
    "who_hit_section_name",
    "condition",
    "set_condition",
    "cost",
    "mass",
    "critically_wounded",
    "get_corpse",
    "is_body_turning",
    "wounded",
    "hit",
    "enable_anomaly",
    "disable_anomaly",
    "get_anomaly_power",
    "set_anomaly_power",
    "inside",
    "set_nonscript_usable",
    "active_zone_contact",
    "set_invisible",
    "set_manual_invisibility",
    "sell_condition",
    "buy_condition",
    "show_condition",
    "profile_name",
    "character_name",
    "bone",
    "Visual",
    "getEnabled",
    "getVisible",
    "net_Export",
    "net_Import",
    "net_Spawn",
    "use",
    "explode",
];

/// The properties of `game_object` (5.1) and `hit` (9).
const PROPERTIES: &[&str] = &[
    "health",
    "psy_health",
    "power",
    "radiation",
    "morale",
    "direction",
    "draftsman",
    "impulse",
    "type",
];

/// A script's uses of this spec's names: classes by bare name, methods
/// after `:`, properties after `.` (not called), constants `Class.name`
/// after `game_object` or `hit`.
fn uses(tokens: &[Token], classes: &[&str]) -> BTreeSet<String> {
    let name = |i: usize| match tokens.get(i) {
        Some(Token::Name(n)) => Some(n.as_str()),
        _ => None,
    };
    let punct = |i: usize, c: u8| tokens.get(i) == Some(&Token::Punct(c));
    let mut out = BTreeSet::new();
    for i in 0..tokens.len() {
        let Some(n) = name(i) else { continue };
        let after = |c: u8| i > 0 && punct(i - 1, c);
        let bare = !after(b'.') && !after(b':');
        if bare && classes.contains(&n) {
            out.insert(n.to_owned());
        }
        if after(b':') && METHODS.contains(&n) && punct(i + 1, b'(') {
            out.insert(n.to_owned());
        }
        if after(b'.') && PROPERTIES.contains(&n) && !punct(i + 1, b'(') {
            out.insert(format!(".{n}"));
        }
        if (n == "game_object" || n == "hit")
            && bare
            && punct(i + 1, b'.')
            && let Some(constant) = name(i + 2)
        {
            out.insert(format!("{n}.{constant}"));
        }
    }
    out
}

/// The script classes declared `class "N" (Base)`, with their bases.
fn declared(tokens: &[Token]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for w in tokens.windows(5) {
        if let [
            Token::Name(class),
            Token::String(n),
            Token::Punct(b'('),
            Token::Name(base),
            Token::Punct(b')'),
        ] = w
            && class == "class"
        {
            out.push((String::from_utf8_lossy(n).into_owned(), base.clone()));
        }
    }
    out
}

/// Whether `text` occurs in `bytes` as a string of its own: a zero byte on
/// either side.
fn string_in(bytes: &[u8], text: &[u8]) -> bool {
    let needle = [b"\0", text, b"\0"].concat();
    bytes.windows(needle.len()).any(|w| w == needle)
}

/// The trader door's record in `spawns\all.spawn` (trader-door.md 1: the
/// `trader_door` whose logic file is `esc_trader_door.ltx`), with its object
/// index, and how many records have section `trigger`.
fn spawn_records(tree: &dyn Vfs, system: &Config) -> ((u32, ObjectRecord), usize) {
    let file = read(tree, "gamedata/spawns/all.spawn").unwrap();
    let mut door = None;
    let mut triggers = 0;
    spawn::for_each_record(&file, |r| {
        let name = section(r.spawn)?;
        if name == "trigger" {
            triggers += 1;
        }
        if name == "physic_object" {
            let class = class_of(system, &name)?.to_owned();
            let record = ObjectRecord::parse(r.spawn, r.update, &class, true)?;
            let custom = &record.object.object_block().custom_data;
            if record.header.name.as_str() == "trader_door"
                && custom.contains("scripts\\esc_trader_door.ltx")
            {
                assert!(door.is_none(), "one trader door");
                door = Some((r.index, record));
            }
        }
        Ok(ControlFlow::Continue(()))
    })
    .unwrap();
    (door.expect("the trader door's record"), triggers)
}

#[test]
fn shipped_uses_of_game_object_basics() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let base = DbVfs::open_game(root, None).unwrap();
    let (reached, scripts, game_library) = loadable(&base);
    let classes = classes();

    // 13: the scripts' uses.
    let used: BTreeMap<&String, BTreeSet<String>> = scripts
        .iter()
        .filter(|(n, _)| n.as_str() != "lua_help")
        .map(|(n, t)| (n, uses(t, &classes)))
        .collect();
    let loadable_using = |n: &str| {
        used.iter()
            .filter(|(s, names)| reached.contains(s.as_str()) && names.contains(n))
            .count()
    };
    let counts: Vec<(&str, usize)> = [
        "id",
        "name",
        "position",
        "alive",
        ".type",
        "spawn_ini",
        "level_vertex_id",
        "inside",
    ]
    .into_iter()
    .map(|n| (n, loadable_using(n)))
    .collect();
    let all = used.values().filter(|n| !n.is_empty()).count();
    let loadable_all = used
        .iter()
        .filter(|(s, n)| reached.contains(s.as_str()) && !n.is_empty())
        .count();
    // TODO(#1036): the spec counts `id` 79, `name` 78, `type` 20 and 139
    // (212) scripts; this scan, without the scoping spec's local-scope and
    // receiver rules, finds one more of each and 5 more scripts (held at
    // that until the spec states its exclusions). The others agree.
    assert_eq!(
        counts,
        [
            ("id", 80),
            ("name", 79),
            ("position", 53),
            ("alive", 31),
            (".type", 21),
            ("spawn_ini", 18),
            ("level_vertex_id", 17),
            ("inside", 15),
        ],
        "loadable scripts using the most used names"
    );
    assert_eq!(
        (loadable_all, all),
        (144, 217),
        "scripts using a name of this spec"
    );
    // 9: nobody uses `hit.dummy`.
    assert_eq!(loadable_using("hit.dummy"), 0, "hit.dummy");

    // 10.2: the only script class deriving from `CGameObject`.
    let deriving: Vec<(&String, String)> = scripts
        .iter()
        .flat_map(|(s, t)| declared(t).into_iter().map(move |d| (s, d)))
        .filter(|(_, (_, base))| base == "CGameObject")
        .map(|(s, (class, _))| (s, class))
        .collect();
    assert_eq!(
        deriving,
        [(&"ce_switcher".to_owned(), "ce_switcher".to_owned())],
        "script classes deriving from CGameObject"
    );
    assert!(reached.contains("ce_switcher"), "ce_switcher is loadable");

    // 11: names the scripts use that aren't registered.
    let who = |n: &str| -> Vec<&str> {
        used.iter()
            .filter(|(s, _)| reached.contains(s.as_str()))
            .filter(|(s, _)| {
                let t = &scripts[s.as_str()];
                t.windows(3).any(|w| {
                    w[0] == Token::Punct(b':')
                        && w[1] == Token::Name(n.to_owned())
                        && w[2] == Token::Punct(b'(')
                })
            })
            .map(|(s, _)| s.as_str())
            .collect()
    };
    assert_eq!(who("actor_sleep"), ["xr_effects"]);
    assert_eq!(who("is_actor_sleeping"), ["xr_conditions"]);
    assert_eq!(who("setVisible"), ["ce_switcher"]);
    assert_eq!(who("setEnabled"), ["ce_switcher"]);
    // As strings of their own: `actor_sleep` only as the callback
    // constant, the others not at all. (`setVisible` and `setEnabled` occur
    // in the decorated names of engine functions the game library imports,
    // which the spec doesn't count, TODO(#1038).)
    for (name, occurs) in [
        ("actor_sleep", true),
        ("is_actor_sleeping", false),
        ("setVisible", false),
        ("setEnabled", false),
    ] {
        assert_eq!(
            string_in(&game_library, name.as_bytes()),
            occurs,
            "{name} as a string of xrGame.dll"
        );
    }
    // 12: the door scheme's calls of this spec's names.
    let calls = |script: &str| -> BTreeSet<String> {
        uses(&scripts[script], &classes)
            .into_iter()
            .filter(|n| METHODS.contains(&n.as_str()))
            .collect()
    };
    let binder = calls("bind_physic_object");
    for name in [
        "name",
        "spawn_ini",
        "clsid",
        "id",
        "game_vertex_id",
        "position",
    ] {
        assert!(binder.contains(name), "bind_physic_object calls {name}");
    }
    let door_scheme = calls("ph_door");
    for name in ["name", "position", "set_nonscript_usable"] {
        assert!(door_scheme.contains(name), "ph_door calls {name}");
    }

    // Startup with the names registered: the services, configuration and
    // files and this spec, then stand-ins for the engine bases and client
    // classes of other specs the class registrators name.
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let system = config::load_aliased(&tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(&tree, &aliases, "$game_config$", "game.ltx").unwrap();
    // 10.2: `O_SWITCH`'s only section, which no spawn record has.
    let switches: Vec<&str> = system
        .sections()
        .filter(|s| s.get("class").flatten().map(str::trim) == Some("O_SWITCH"))
        .map(|s| s.name())
        .collect();
    assert_eq!(switches, ["trigger"], "sections of class O_SWITCH");
    let ((door_index, door), triggers) = spawn_records(&tree, &system);
    assert_eq!(triggers, 0, "spawn records of section trigger");
    let common = Common::read(&tree, &aliases).unwrap();
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    let trees = Rc::new(GameTrees { tree, root });
    let mut s = Scripts::new(trees.clone(), &aliases, Prefix::Default).unwrap();
    s.register_engine_services(Rc::clone(&services)).unwrap();
    let files = Rc::new(RefCell::new(FileSystem::new(
        trees,
        aliases,
        Rc::new(system.clone()),
        Rc::new(game),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .unwrap();
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), services, files)
        .unwrap();
    let ours: BTreeSet<&str> = classes.iter().copied().collect();
    let registrator = &scripts["class_registrator"];
    let named: BTreeSet<String> = registrator
        .iter()
        .filter_map(|t| match t {
            Token::String(n) => Some(String::from_utf8_lossy(n).into_owned()),
            _ => None,
        })
        .filter(|n| n.starts_with('C') && !n.contains('.'))
        .collect();
    let mut stand_ins: BTreeMap<&str, Vec<usize>> = ENGINE_BASES
        .iter()
        .map(|(name, counts)| (*name, counts.to_vec()))
        .collect();
    for name in &named {
        stand_ins.entry(name).or_insert_with(|| vec![0]);
    }
    for (name, counts) in stand_ins {
        // `object_binder` is spec L's, registered with this spec's names.
        if ours.contains(name) || name == "object_binder" {
            continue;
        }
        let mut class = EngineClass::new(name);
        for count in counts {
            class = class.constructor(count, |_| Some(0), |_, _| Ok(()));
        }
        s.register_class(class).unwrap();
    }
    // 10.3: the registrators name 44 of the 59 listed client classes and
    // `CPhysicObject`; the 15 they don't are 11 artefact classes, `CF1`,
    // `CRGD5`, `CWeaponFN2000` and `CWeaponFORT`.
    let unnamed: Vec<&str> = volya_engine_script::game_object::client_classes()
        .filter(|c| !named.contains(*c))
        .collect();
    assert_eq!(
        unnamed.len(),
        15,
        "client classes no registrator names: {unnamed:?}"
    );
    assert!(named.contains("CPhysicObject"), "CPhysicObject is named");
    for c in ["CF1", "CRGD5", "CWeaponFN2000", "CWeaponFORT"] {
        assert!(unnamed.contains(&c), "{c} isn't named");
    }
    s.exec(
        "=stand_ins",
        b"function editor() return false end
          local any = {}
          setmetatable(any, {__index = function() return any end, __call = function() return any end})
          game = any",
    )
    .unwrap();
    s.load_globals().unwrap();
    s.call_class_registrators(&common).unwrap();
    for name in &reached {
        let name = if name == "_g" { "_G" } else { name.as_str() };
        s.load_namespace(name).unwrap();
    }
    // Scripts using engine classes of other specs fail to load; none fails
    // on a name of this spec, and none stops the state.
    for failure in s.take_failures() {
        assert_ne!(failure.kind, FailureKind::Fatal, "{failure:?}");
        assert!(
            !ours
                .iter()
                .any(|n| failure.message.contains(&format!("'{n}'"))),
            "{failure:?}"
        );
    }

    // 12: the trader door, spawned from its record through the factory's
    // `CPhysicObject`, and the door scheme's calls on its handle.
    let player = SpawnData {
        id: 0,
        name: b"actor".to_vec(),
        // A section without `script_binding`: the player's binder (spec L)
        // needs names of specs this test doesn't register.
        section: b"test_actor_without_binding".to_vec(),
        class_id: class_id(b"O_ACTOR"),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: volya_engine_formats::model::transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: Some(1.0),
    };
    s.spawn_engine_object(ObjectClass::Player, player).unwrap();
    let id = u16::try_from(door_index).unwrap();
    let data = SpawnData::from_record(id, &door, &system).unwrap();
    let created = s.create_client_object(data.class_id).unwrap();
    let CreatedObject::Script(client) = created.object else {
        panic!("the door's client: {created:?}");
    };
    let class = s.with_part(client, |c: &mut ClientObject| c.class);
    assert_eq!(class, Some("CPhysicObject"), "the door's client class");
    assert!(s.spawn(client, data.clone()).unwrap(), "the door spawns");
    let number = s.object_factory().number(data.class_id);
    let handle = s.game_object(id).unwrap();
    s.exec(
        "=door",
        br#"
function door_calls(d)
  local ini = d:spawn_ini()
  local p = d:position()
  local results = {d:name(), ini:section_exist("logic"), ini:r_string("logic", "cfg"),
    d:clsid() == clsid.projector, d:clsid(), d:id(), d:game_vertex_id(), p.x, p.y, p.z}
  d:set_nonscript_usable(false)
  return unpack(results)
end
"#,
    )
    .unwrap();
    let results = s.call("door_calls", &[Value::Object(handle)]).unwrap();
    let h = &door.header;
    let block = door.object.object_block();
    assert_eq!(
        results,
        [
            Value::from("trader_door"),
            Value::Boolean(true),
            Value::from("scripts\\esc_trader_door.ltx"),
            Value::Boolean(false),
            Value::Number(number as f64),
            Value::Number(f64::from(id)),
            Value::Number(f64::from(block.game_graph_vertex)),
            Value::Number(f64::from(h.position[0])),
            Value::Number(f64::from(h.position[1])),
            Value::Number(f64::from(h.position[2])),
        ],
        "the door scheme's reads"
    );
    // 6.3, 12: the player's use then does only the scripted use.
    assert_eq!(s.player_use(id, 0).unwrap(), UseAction::ScriptedOnly);
    assert!(s.destroy_object(id).unwrap());
    assert_eq!(objects.borrow().len(), 1, "the player alone");
}
