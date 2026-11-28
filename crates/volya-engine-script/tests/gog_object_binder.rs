// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's object binders (BYOA), checked against
//! docs/behavior/script-api-object-binders.md: the `script_binding` keys of
//! `config\system.ltx` (3.2), the binder classes, their overrides and the
//! `bind_object` calls of the scripts (3.3, 7, 9), and the trader door
//! spawned with its binding function, updated, saved and destroyed (8).
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::BTreeMap;
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
use volya_engine_script::game_object::{GameObjects, SpawnData};
use volya_engine_script::loader::{Common, Prefix, Scripts};
use volya_engine_script::services::EngineServices;

use shipped_scripts::{ENGINE_BASES, FSGAME, GameTrees, Token, tokens};

/// The scripts, by name (lower case, without `.script`).
fn scripts(vfs: &dyn Vfs) -> BTreeMap<String, Vec<Token>> {
    vfs.read_dir("gamedata/scripts")
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EntryKind::File)
        .filter_map(|e| {
            let name = e.name.to_ascii_lowercase();
            let stem = name.strip_suffix(".script")?.to_owned();
            let text = read(vfs, &format!("gamedata/scripts/{}", e.name)).unwrap();
            Some((stem, tokens(&text)))
        })
        .collect()
}

/// The script classes deriving from `object_binder`, and each one's
/// methods: `(name, calls object_binder.name)`.
fn binder_classes(tokens: &[Token]) -> Vec<(String, Vec<(String, bool)>)> {
    let mut out: Vec<(String, Vec<(String, bool)>)> = Vec::new();
    for w in tokens.windows(5) {
        if let [
            Token::Name(class),
            Token::String(n),
            Token::Punct(b'('),
            Token::Name(base),
            Token::Punct(b')'),
        ] = w
            && class == "class"
            && base == "object_binder"
        {
            out.push((String::from_utf8_lossy(n).into_owned(), Vec::new()));
        }
    }
    // `function C:m(` … up to the next `function`: whether it has
    // `object_binder.m`.
    let mut i = 0;
    while i + 4 < tokens.len() {
        if let [
            Token::Name(f),
            Token::Name(class),
            Token::Punct(b':'),
            Token::Name(method),
            ..,
        ] = &tokens[i..i + 4]
            && f == "function"
            && let Some((_, methods)) = out.iter_mut().find(|(c, _)| c == class)
        {
            let end = tokens[i + 1..]
                .iter()
                .position(|t| *t == Token::Name("function".into()))
                .map_or(tokens.len(), |p| i + 1 + p);
            let base = tokens[i..end].windows(3).any(|w| {
                w[0] == Token::Name("object_binder".into())
                    && w[1] == Token::Punct(b'.')
                    && w[2] == Token::Name(method.clone())
            });
            methods.push((method.clone(), base));
        }
        i += 1;
    }
    out
}

/// How often `:name(` occurs.
fn calls_of(tokens: &[Token], name: &str) -> usize {
    tokens
        .windows(3)
        .filter(|w| {
            w[0] == Token::Punct(b':')
                && w[1] == Token::Name(name.to_owned())
                && w[2] == Token::Punct(b'(')
        })
        .count()
}

/// The trader door's record in `spawns\all.spawn` (trader-door.md 1), with
/// its object index.
fn door_record(tree: &dyn Vfs, system: &Config) -> (u32, ObjectRecord) {
    let file = read(tree, "gamedata/spawns/all.spawn").unwrap();
    let mut door = None;
    spawn::for_each_record(&file, |r| {
        if section(r.spawn)? == "physic_object" {
            let class = class_of(system, "physic_object")?.to_owned();
            let record = ObjectRecord::parse(r.spawn, r.update, &class, true)?;
            let custom = &record.object.object_block().custom_data;
            if record.header.name.as_str() == "trader_door"
                && custom.contains("scripts\\esc_trader_door.ltx")
            {
                door = Some((r.index, record));
            }
        }
        Ok(ControlFlow::Continue(()))
    })
    .unwrap();
    door.expect("the trader door's record")
}

#[test]
fn shipped_object_binders() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let base = DbVfs::open_game(root, None).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();

    // 3.2: the keys, inherited ones counted.
    let system = config::load_aliased(&base, &aliases, "$game_config$", "system.ltx").unwrap();
    assert_eq!(system.len(), 1245, "sections of system.ltx");
    let mut values: BTreeMap<String, usize> = BTreeMap::new();
    for s in system.sections() {
        if let Some(value) = s.get("script_binding") {
            *values
                .entry(value.unwrap_or("").trim().to_owned())
                .or_default() += 1;
        }
    }
    let expected: BTreeMap<String, usize> = [
        ("", 1),
        ("bind_heli.bind", 1),
        ("bind_monster.bind", 68),
        ("bind_physic_object.init", 6),
        ("bind_respawn.bind", 1),
        ("bind_restrictor.bind", 1),
        ("bind_smart_terrain.bind", 1),
        ("bind_stalker.actor_init", 1),
        ("bind_stalker.init", 95),
        ("phantom_manager.bind", 1),
        ("xr_zones.bind", 1),
    ]
    .into_iter()
    .map(|(k, n)| (k.to_owned(), n))
    .collect();
    assert_eq!(values, expected, "script_binding values");
    assert_eq!(values.values().sum::<usize>(), 177);
    assert_eq!(
        system.get("online_offline_group", "script_binding"),
        Some(None),
        "the empty value"
    );
    assert_eq!(
        system
            .get("physic_object", "script_binding")
            .flatten()
            .map(str::trim),
        Some("bind_physic_object.init")
    );

    // 3.3, 7, 9: the binder classes, their overrides and the calls.
    let scripts = scripts(&base);
    for name in values.keys().filter(|v| !v.is_empty()) {
        let (namespace, function) = name.rsplit_once('.').unwrap();
        let t = &scripts[namespace];
        assert!(
            t.windows(4).any(|w| matches!(w,
                [Token::Name(f), Token::Name(n), Token::Punct(b'('), Token::Name(_)]
                    if f == "function" && n == function)),
            "{name} is defined with a parameter"
        );
    }
    let classes: BTreeMap<String, (String, Vec<(String, bool)>)> = scripts
        .iter()
        .flat_map(|(s, t)| {
            binder_classes(t)
                .into_iter()
                .map(move |(c, m)| (c, (s.clone(), m)))
        })
        .collect();
    let overrides = |class: &str| -> Vec<String> {
        let (_, methods) = &classes[class];
        let mut out: Vec<String> = methods
            .iter()
            .filter(|(m, _)| METHODS.contains(&m.as_str()))
            .map(|(m, base)| format!("{m}{}", if *base { "+" } else { " o" }))
            .collect();
        // A method defined twice (as `reload` in bind_monster.script)
        // counts once.
        out.sort();
        out.dedup();
        out
    };
    let table: Vec<(&str, &str, &str)> = vec![
        (
            "heli_binder",
            "bind_heli",
            "reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o update+ save+ load+",
        ),
        (
            "generic_object_binder",
            "bind_monster",
            "reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o update+ save+ load+",
        ),
        (
            "generic_physics_binder",
            "bind_physic_object",
            "reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o update+ save+ load+",
        ),
        (
            "respawn_binder",
            "bind_respawn",
            "net_spawn+ net_destroy+ update+",
        ),
        (
            "restrictor_binder",
            "bind_restrictor",
            "reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o save+ load+",
        ),
        (
            "smart_terrain_binder",
            "bind_smart_terrain",
            "net_spawn+ net_destroy+ net_Relcase o update+",
        ),
        (
            "actor_binder",
            "bind_stalker",
            "reinit+ net_spawn+ net_destroy+ update+ save+ load+",
        ),
        (
            "stalker_binder",
            "bug_test",
            "reinit+ reload+ net_spawn+ net_destroy+ net_import+ net_export+",
        ),
        ("Phantom", "phantom_manager", "net_destroy o"),
        (
            "motivator_binder",
            "xr_motivator",
            "reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o update+ save+ load+",
        ),
        (
            "arena_zone_binder",
            "xr_zones",
            "net_spawn+ net_destroy+ save+ load+",
        ),
    ];
    assert_eq!(classes.len(), table.len(), "binder classes: {classes:?}");
    for (class, script, expected) in table {
        assert_eq!(classes[class].0, script, "{class}'s script");
        assert_eq!(overrides(class), items(expected), "{class}'s overrides");
        assert!(
            classes[class].1.iter().any(|(m, _)| m == "__init"),
            "{class} defines __init"
        );
    }
    let binding: Vec<&str> = scripts
        .iter()
        .filter(|(_, t)| calls_of(t, "bind_object") > 0)
        .map(|(s, _)| s.as_str())
        .collect();
    assert_eq!(
        binding.len(),
        11,
        "scripts calling bind_object: {binding:?}"
    );
    assert!(binding.contains(&"bug_test") && binding.contains(&"xr_motivator"));
    let total: usize = scripts.values().map(|t| calls_of(t, "bind_object")).sum();
    assert_eq!(total, 11, "bind_object calls");
    assert!(
        scripts.values().all(|t| calls_of(t, "binded_object") == 0),
        "no binded_object"
    );

    // 8: the trader door with its binding function.
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let system = config::load_aliased(&tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(&tree, &aliases, "$game_config$", "game.ltx").unwrap();
    let (door_index, door) = door_record(&tree, &system);
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
    // Stand-ins for the engine bases and client classes of other specs
    // the class registrators name (for `clsid`).
    let mut stand_ins: BTreeMap<String, Vec<usize>> = ENGINE_BASES
        .iter()
        .map(|(name, counts)| ((*name).to_owned(), counts.to_vec()))
        .collect();
    for t in &scripts["class_registrator"] {
        if let Token::String(n) = t {
            let n = String::from_utf8_lossy(n).into_owned();
            if n.starts_with('C') && !n.contains('.') {
                stand_ins.entry(n).or_insert_with(|| vec![0]);
            }
        }
    }
    for (name, counts) in stand_ins {
        let registered = s
            .exec(
                "=registered",
                format!("return rawget(_G, '{name}') ~= nil").as_bytes(),
            )
            .unwrap();
        if registered == [Value::Boolean(true)] {
            continue;
        }
        let mut class = EngineClass::new(&name);
        for count in counts {
            class = class.constructor(count, |_| Some(0), |_, _| Ok(()));
        }
        s.register_class(class).unwrap();
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
    let id = u16::try_from(door_index).unwrap();
    let data = SpawnData::from_record(id, &door, &system).unwrap();
    let client = match s.exec("=client", b"return CPhysicObject()").unwrap()[..] {
        [Value::Object(part)] => part,
        ref other => panic!("CPhysicObject(): {other:?}"),
    };
    assert!(s.spawn(client, data).unwrap(), "the door spawns");
    assert!(s.attached_binder(id).is_some(), "the door's binder");
    let handle = s.game_object(id).unwrap();
    s.exec(
        "=door",
        br#"
function door_binder(d)
  local b = d:binded_object()
  return b.initialized, b.loaded, rawequal(b.object, d), rawequal(b.st, db.storage[d:id()]),
    type(b.st)
end
"#,
    )
    .unwrap();
    let state = s.call("door_binder", &[Value::Object(handle)]).unwrap();
    assert_eq!(
        state,
        [
            Value::Boolean(false),
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::from("table"),
        ],
        "generic_physics_binder after reinit"
    );
    // Without the player's script object the update starts no logic.
    s.scheduled_update(id, 100).unwrap();
    assert_eq!(
        s.call("door_binder", &[Value::Object(handle)]).unwrap()[0],
        Value::Boolean(false),
        "not initialized without db.actor"
    );
    // A physics object is always save-relevant, without asking (5.3, 8).
    assert!(s.save_relevant(id, false).unwrap());
    assert!(s.destroy_object(id).unwrap());
    assert_eq!(s.attached_binder(id), None, "deleted with the door");
}

/// The 11 methods of `object_binder` (1).
const METHODS: [&str; 11] = [
    "save",
    "update",
    "reload",
    "net_export",
    "net_save_relevant",
    "load",
    "net_destroy",
    "reinit",
    "net_Relcase",
    "net_spawn",
    "net_import",
];

/// The overrides of a row of the table of 7 (`m+`, `m o`), sorted.
fn items(row: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in row.split_whitespace() {
        match (word, out.last_mut()) {
            ("o", Some(last)) => last.push_str(" o"),
            _ => out.push(word.to_owned()),
        }
    }
    out.sort();
    out
}
