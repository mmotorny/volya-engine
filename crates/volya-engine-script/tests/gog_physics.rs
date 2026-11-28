// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's use of script API Y, checked against
//! docs/behavior/script-api-physics-vehicles-helicopter-and-lamps.md: the
//! 250 loadable scripts' uses of the most used names (12), the names they
//! use that aren't registered (10), every loadable script loaded with the
//! names registered and the class registrators naming `CCar` and
//! `CHelicopter`, the helicopter's section (8), and the door scheme's calls
//! on the trader door's shell, built from its record and model and
//! activated in the physics world (11). Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

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
use volya_engine_core::physics::shell::{BoneState, PhysicsRecord, ShellModel, physics_object};
use volya_engine_core::simulation::class_of;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::gamemtl;
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;
use volya_engine_formats::object::{Object, ObjectRecord, section};
use volya_engine_formats::spawn;
use volya_engine_script::Value;
use volya_engine_script::classes::EngineClass;
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::physics::PhysicsShell;
use volya_engine_script::game_object::vehicles::Helicopter;
use volya_engine_script::game_object::{GameObjects, ObjectClass, SpawnData};
use volya_engine_script::loader::{Common, FailureKind, Prefix, Scripts};
use volya_engine_script::object_factory::{CreatedObject, class_id};
use volya_engine_script::selftest::physics::SharedWorld;
use volya_engine_script::services::EngineServices;

use shipped_scripts::{ENGINE_BASES, GameTrees, Token, tokens};

/// The aliases scripts use.
const FSGAME: &[u8] = b"$app_data_root$ = true | false| {userdocs}\\Stalker-SHOC\r\n\
    $game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n\
    $game_saves$ =    true | false| $app_data_root$| savedgames\\\r\n";

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

/// The words of `bytes`, lowercase (script-loading.md 3).
fn words(bytes: &[u8], out: &mut BTreeSet<String>) {
    for run in bytes.split(|b| !(b.is_ascii_alphanumeric() || *b == b'_')) {
        if run.first().is_some_and(|b| !b.is_ascii_digit()) {
            out.insert(String::from_utf8_lossy(run).to_ascii_lowercase());
        }
    }
}

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

/// The 250 loadable scripts (script-loading.md 3), every script's tokens
/// by name, and xrGame.dll's bytes.
fn loadable(base: &dyn Vfs) -> (BTreeSet<String>, BTreeMap<String, Vec<Token>>, Vec<u8>) {
    let mut files = Vec::new();
    walk(base, "", &mut files);
    let lower: Vec<(String, &String)> = files.iter().map(|f| (f.to_ascii_lowercase(), f)).collect();
    let ext = |f: &str, exts: &[&str]| exts.iter().any(|e| f.ends_with(e));
    let mut roots = BTreeSet::from(["_g".to_owned()]);
    for (l, path) in &lower {
        let root = (l.starts_with("gamedata/config/") && ext(l, &[".ltx", ".xml"]))
            || (!l.starts_with("gamedata/config/")
                && !l.starts_with("gamedata/scripts/")
                && ext(l, &[".ltx", ".ini", ".spawn", ".game"]));
        if root {
            words(&read(base, path).unwrap(), &mut roots);
        }
    }
    let binaries = gog_archives::game_files("binaries", |p| {
        let p = p.to_ascii_lowercase();
        p == "bin/xrgame.dll" || p == "bin/xr_3da.exe"
    })
    .unwrap();
    let mut game_library = Vec::new();
    for binary in &binaries {
        let bytes = fs::read(&binary.file).unwrap();
        words(&bytes, &mut roots);
        let name = binary.file.to_string_lossy().to_ascii_lowercase();
        if name.ends_with("xrgame.dll") {
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

/// Whether a script calls `name` as a method (`:name(`).
fn calls(tokens: &[Token], name: &str) -> bool {
    tokens.windows(3).any(|w| {
        w[0] == Token::Punct(b':')
            && w[1] == Token::Name(name.to_owned())
            && w[2] == Token::Punct(b'(')
    })
}

fn string_in(bytes: &[u8], text: &[u8]) -> bool {
    let needle = [b"\0", text, b"\0"].concat();
    bytes.windows(needle.len()).any(|w| w == needle)
}

/// The trader door's record (trader-door.md 1) and its object index.
fn door_record(tree: &dyn Vfs, system: &Config) -> (u32, ObjectRecord) {
    let file = read(tree, "gamedata/spawns/all.spawn").unwrap();
    let mut door = None;
    spawn::for_each_record(&file, |r| {
        let name = section(r.spawn)?;
        if name == "physic_object" {
            let class = class_of(system, &name)?.to_owned();
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

/// The game object basics' classes and this spec's.
fn ours() -> BTreeSet<&'static str> {
    let mut out: BTreeSet<&str> = [
        "game_object",
        "hit",
        "CGameObject",
        "explosive",
        "ce_script_zone",
        "ce_smart_zone",
        "physics_shell",
        "physics_element",
        "physics_joint",
        "holder",
        "CCar",
        "CHelicopter",
        "hanging_lamp",
    ]
    .into_iter()
    .collect();
    out.extend(volya_engine_script::game_object::client_classes());
    out
}

#[test]
fn shipped_uses_of_script_api_y() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let base = DbVfs::open_game(root, None).unwrap();
    let (reached, scripts, game_library) = loadable(&base);

    // 12: the most used names, by loadable scripts calling them.
    let using = |name: &str| {
        reached
            .iter()
            .filter(|s| s.as_str() != "lua_help")
            .filter(|s| scripts.get(s.as_str()).is_some_and(|t| calls(t, name)))
            .count()
    };
    let counts: Vec<(&str, usize)> = [
        "get_physics_shell",
        "get_joint_by_bone_name",
        "set_max_force_and_velocity",
        "get_axis_angle",
        "get_helicopter",
        "GetfHealth",
        "set_const_force",
    ]
    .into_iter()
    .map(|n| (n, using(n)))
    .collect();
    assert_eq!(
        counts,
        [
            ("get_physics_shell", 6),
            ("get_joint_by_bone_name", 5),
            ("set_max_force_and_velocity", 4),
            ("get_axis_angle", 3),
            ("get_helicopter", 3),
            ("GetfHealth", 3),
            ("set_const_force", 3),
        ],
        "loadable scripts calling the most used names"
    );
    // 10: names the scripts use that the game library lacks.
    for name in ["air_attack", "air_attack_active"] {
        let who: Vec<&str> = scripts
            .iter()
            .filter(|(s, t)| s.as_str() != "lua_help" && calls(t, name))
            .map(|(s, _)| s.as_str())
            .collect();
        assert_eq!(who, ["h", "test_heli"], "scripts calling {name}");
        assert!(
            !string_in(&game_library, name.as_bytes()),
            "{name} in xrGame.dll"
        );
    }
    assert!(reached.contains("h"), "h is loadable");
    // 11: the door scheme's calls.
    for name in [
        "get_physics_shell",
        "get_joint_by_bone_name",
        "get_limits",
        "set_max_force_and_velocity",
        "get_axis_angle",
    ] {
        assert!(calls(&scripts["ph_door"], name), "ph_door calls {name}");
    }
    for name in [
        "set_limits",
        "get_axis_angle",
        "block_breaking",
        "unblock_breaking",
    ] {
        assert!(calls(&scripts["ph_gate"], name), "ph_gate calls {name}");
    }

    // Startup with the names registered.
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let system = config::load_aliased(&tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(&tree, &aliases, "$game_config$", "game.ltx").unwrap();
    // 8.4: the helicopter's section, every key it reads.
    let heli = Helicopter::from_section(&system, "helicopter", [0.0; 3]).unwrap();
    assert!(heli.max_velocity > 0.0, "{heli:?}");
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
        trees.clone(),
        aliases,
        Rc::new(system.clone()),
        Rc::new(game),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .unwrap();
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), services, files)
        .unwrap();
    s.register_physics_vehicles_and_lamps().unwrap();
    let ours = ours();
    let registrator = &scripts["class_registrator"];
    let named: BTreeSet<String> = registrator
        .iter()
        .filter_map(|t| match t {
            Token::String(n) => Some(String::from_utf8_lossy(n).into_owned()),
            _ => None,
        })
        .filter(|n| n.starts_with('C') && !n.contains('.'))
        .collect();
    assert!(
        named.contains("CCar") && named.contains("CHelicopter"),
        "the registrators name CCar and CHelicopter"
    );
    let mut stand_ins: BTreeMap<&str, Vec<usize>> = ENGINE_BASES
        .iter()
        .map(|(name, counts)| (*name, counts.to_vec()))
        .collect();
    for name in &named {
        stand_ins.entry(name).or_insert_with(|| vec![0]);
    }
    for (name, counts) in stand_ins {
        // `object_binder` is spec L's, registered with the game object
        // basics.
        if ours.contains(name) || name == "object_binder" {
            continue;
        }
        let mut class = EngineClass::new(name);
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
    for name in &reached {
        let name = if name == "_g" { "_G" } else { name.as_str() };
        s.load_namespace(name).unwrap();
    }
    for failure in s.take_failures() {
        assert_ne!(failure.kind, FailureKind::Fatal, "{failure:?}");
        assert!(
            !ours
                .iter()
                .any(|n| failure.message.contains(&format!("'{n}'"))),
            "{failure:?}"
        );
    }
    // The factory's `CCar` and `CHelicopter` are this spec's classes.
    for (id, class) in [("SCRPTCAR", "CCar"), ("C_HLCP_S", "CHelicopter")] {
        let created = s.create_client_object(class_id(id.as_bytes())).unwrap();
        let CreatedObject::Script(client) = created.object else {
            panic!("{id}: {created:?}");
        };
        let made = s.with_part(
            client,
            |c: &mut volya_engine_script::game_object::ClientObject| c.class,
        );
        assert_eq!(made, Some(class), "{id}'s client");
    }

    // 11: the trader door, its shell built from its record and model and
    // activated, and the door scheme's calls.
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
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: Some(1.0),
    };
    s.spawn_engine_object(ObjectClass::Player, player).unwrap();
    let (index, door) = door_record(&trees.tree, &system);
    let id = u16::try_from(index).unwrap();
    let data = SpawnData::from_record(id, &door, &system).unwrap();
    let created = s.create_client_object(data.class_id).unwrap();
    let CreatedObject::Script(client) = created.object else {
        panic!("the door's client: {created:?}");
    };
    assert!(s.spawn(client, data).unwrap(), "the door spawns");
    let Object::PhysicsObject(p) = &door.object else {
        panic!("the door is a physics object");
    };
    let path = format!("gamedata/meshes/{}.ogf", p.visual.model.as_str()).replace('\\', "/");
    let model = ShellModel::read(&read(&trees.tree, &path).unwrap()).unwrap();
    let materials = gamemtl::parse(&read(&trees.tree, "gamedata/gamemtl.xr").unwrap()).unwrap();
    let custom = ltx::parse(p.object.custom_data.as_bytes()).ok();
    let h = &door.header;
    let record = PhysicsRecord {
        physics_type: p.physics_type,
        mass: p.mass,
        fixed_bones: &p.fixed_bones,
        skeleton_flags: p.skeleton.flags,
        startup_animation: &p.skeleton.startup_animation,
        custom_data: custom.as_ref(),
        transform: transform(h.position, h.angles),
        bone_state: BoneState::default(),
    };
    let shell = physics_object(&model, &record, &materials)
        .unwrap()
        .expect("the door's shell");
    let world = SharedWorld::default();
    world.0.borrow_mut().add(id, &shell);
    let physics = s.physics_state().unwrap();
    {
        let mut ph = physics.borrow_mut();
        let step = world.0.borrow().world.params().step;
        ph.shells
            .insert(id, PhysicsShell::of_shell(&shell, Some(&model), step));
        ph.world = Some(Box::new(world.clone()));
    }
    let handle = s.game_object(id).unwrap();
    s.exec(
        "=door",
        br#"
function door_calls(d)
  local ph_shell = d:get_physics_shell()
  local joint = ph_shell:get_joint_by_bone_name("door")
  local low, hi = 0, 0
  low, hi = joint:get_limits(low, hi, 0)
  local closed = joint:get_axis_angle(90) <= low
  joint:set_max_force_and_velocity(2100, -3, 0)
  return low, hi, closed
end
function door_angle(d)
  return d:get_physics_shell():get_joint_by_bone_name("door"):get_axis_angle(90)
end
"#,
    )
    .unwrap();
    let results = s.call("door_calls", &[Value::Object(handle)]).unwrap();
    // 5.1, `joints` 3: the limits before a setter, (−0, 1.9199),
    // closed at the pose the hinge was made in.
    let [
        Value::Number(low),
        Value::Number(high),
        Value::Boolean(closed),
    ] = results[..]
    else {
        panic!("the door scheme's reads {results:?}");
    };
    assert_eq!(low, 0.0, "low");
    assert!((high - 1.9199).abs() < 1e-4, "high {high}");
    assert!(closed, "closed at the start");
    // The opening motor turns the door toward 1.9199.
    {
        let mut w = world.0.borrow_mut();
        for i in 0..50 {
            w.world.frame(0.02, i * 20);
        }
    }
    let angle = s.call("door_angle", &[Value::Object(handle)]).unwrap();
    let [Value::Number(angle)] = angle[..] else {
        panic!("{angle:?}");
    };
    assert!(angle > 0.5 && angle < 1.95, "the door opened to {angle}");
}
