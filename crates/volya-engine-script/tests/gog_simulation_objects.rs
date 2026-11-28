// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's simulation objects, checked against
//! docs/behavior/script-api-simulation-objects.md. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::rc::Rc;

use shipped_scripts::{FSGAME, GameTrees};
use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::simulation::class_of;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, read};
use volya_engine_formats::object::{ObjectRecord, section};
use volya_engine_formats::spawn;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::GameObjects;
use volya_engine_script::loader::Prefix;
use volya_engine_script::object_factory::CreatedObject;
use volya_engine_script::services::EngineServices;
use volya_engine_script::simulation_objects::{CLASSES, classes, dispatched};
use volya_engine_script::story::Story;

/// A class of the listing (`scripts\lua_help.script`): its bases, member
/// names and constructors' parameter lists.
#[derive(Debug, Default)]
struct Listed {
    bases: Vec<String>,
    members: BTreeSet<String>,
    constructors: Vec<String>,
}

/// The listing's classes, by name.
fn listing(text: &str) -> BTreeMap<String, Listed> {
    let mut out = BTreeMap::new();
    let mut current: Option<(String, Listed)> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("C++ class ") {
            let head = rest.trim_end_matches('{').trim();
            let (name, bases) = match head.split_once(':') {
                Some((n, b)) => (
                    n.trim(),
                    b.split(',').map(|b| b.trim().to_owned()).collect(),
                ),
                None => (head, Vec::new()),
            };
            current = Some((
                name.to_owned(),
                Listed {
                    bases,
                    ..Listed::default()
                },
            ));
            continue;
        }
        let Some((name, class)) = current.as_mut() else {
            continue;
        };
        if line.starts_with("};") {
            let (name, class) = current.take().unwrap();
            out.insert(name, class);
            continue;
        }
        let word = |s: &str| {
            s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("")
                .to_owned()
        };
        if let Some(rest) = line.strip_prefix("function ") {
            class.members.insert(word(rest.trim()));
        } else if let Some(rest) = line.strip_prefix("property ") {
            class.members.insert(word(rest.trim()));
        } else if line.starts_with(name.as_str()) && line.contains('(') {
            let args = line.split_once('(').unwrap().1;
            class.constructors.push(
                args.trim_end_matches(';')
                    .trim_end_matches(')')
                    .trim()
                    .to_owned(),
            );
        } else if !line.is_empty() && !line.starts_with("const ") {
            class.members.insert(format!("? {line}"));
        }
    }
    out
}

/// 1, 2.1, 3.12: the classes, bases, constructors and members registered
/// against the listing.
#[test]
fn registrations_against_the_listing() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, None).unwrap();
    let text = read(&tree, "gamedata/scripts/lua_help.script").unwrap();
    let listed = listing(&String::from_utf8_lossy(&text));
    let mut failures = Vec::new();
    let mut members = 0;
    for class in &CLASSES {
        let Some(l) = listed.get(class.name) else {
            failures.push(format!("{} isn't listed", class.name));
            continue;
        };
        if l.bases != class.bases {
            failures.push(format!("{}: bases {:?}", class.name, l.bases));
        }
        let constructible = !l.constructors.is_empty();
        if constructible != class.constructible || l.constructors.len() > 1 {
            failures.push(format!("{}: constructors {:?}", class.name, l.constructors));
        }
        let mut want: BTreeSet<String> = l.members.clone();
        // 1: OnEvent is listed, not registered; m_smart_terrain_id the reverse.
        want.remove("OnEvent");
        if class.name == "cse_alife_monster_abstract"
            || classes::derives(class.name, "cse_alife_monster_abstract")
        {
            want.insert("m_smart_terrain_id".into());
        }
        let got: BTreeSet<String> = classes::members(class)
            .into_iter()
            .map(|(m, _)| m.to_owned())
            .collect();
        members += got.len();
        if want != got {
            failures.push(format!(
                "{}: missing {:?}, extra {:?}",
                class.name,
                want.difference(&got).collect::<Vec<_>>(),
                got.difference(&want).collect::<Vec<_>>()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(members > 0);
}

/// The engine state and startup over the shipped files.
fn started(trees: Rc<GameTrees>, aliases: &PathAliases) -> (GameScripts, Config) {
    let tree = &trees.tree;
    let system = config::load_aliased(tree, aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(tree, aliases, "$game_config$", "game.ltx").unwrap();
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    let files = Rc::new(RefCell::new(FileSystem::new(
        trees.clone(),
        aliases.clone(),
        Rc::new(system.clone()),
        Rc::new(game),
    )));
    let g = GameScripts::start(Startup {
        trees,
        aliases: aliases.clone(),
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::default(),
        files,
        window_files: Rc::default(),
    })
    .unwrap();
    (g, system)
}

/// 2.2, 6: the shipped server classes made through the object factory for
/// a record of each scripted class id of `spawns\all.spawn`, their engine
/// bases, `init`, and the engine's call their scripts answer without the
/// rest of the simulation (`keep_saved_data_anyway`); the server object a
/// record's online object gets.
#[test]
fn shipped_server_classes() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let file = read(&tree, "gamedata/spawns/all.spawn").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let (mut g, system) = started(trees, &aliases);
    // One record per class id, the first.
    let mut records: BTreeMap<String, ObjectRecord> = BTreeMap::new();
    spawn::for_each_record(&file, |r| {
        let section = section(r.spawn)?;
        let class = class_of(&system, &section)?.to_owned();
        if let std::collections::btree_map::Entry::Vacant(e) = records.entry(class) {
            let record = ObjectRecord::parse(r.spawn, r.update, e.key(), true)?;
            e.insert(record);
        }
        Ok(ControlFlow::Continue(()))
    })
    .unwrap();
    let mut bases = BTreeMap::new();
    let mut kept = BTreeMap::new();
    for (class, record) in &records {
        let s = g.scripts();
        let created = s
            .create_server_object(&system, &record.header.section)
            .unwrap();
        let CreatedObject::Script(part) = created.object else {
            continue;
        };
        let engine = s.with_simulation_object(part, |o| o.class).unwrap();
        bases.insert(class.as_str(), engine);
        s.set_simulation_state(part, record.clone()).unwrap();
        if dispatched(engine, "keep_saved_data_anyway") {
            kept.insert(class.as_str(), s.keep_saved_data_anyway(part).unwrap());
        }
        s.release_object(part).unwrap();
    }
    eprintln!(
        "failures {:?}\nengine bases {bases:?}\nkeep_saved_data_anyway {kept:?}",
        g.scripts().take_failures()
    );
    for (class, base) in [
        ("AI_STL_S", "cse_alife_human_stalker"),
        ("AI_TRD_S", "cse_alife_trader"),
        ("O_PHYS_S", "cse_alife_object_physic"),
        ("RE_SPAWN", "cse_alife_smart_zone"),
        ("SMRTTRRN", "cse_alife_smart_zone"),
        ("SPC_RS_S", "cse_alife_space_restrictor"),
        ("ZS_BFUZZ", "cse_zone_visual"),
        ("ZS_MBALD", "cse_anomalous_zone"),
        ("C_HLCP_S", "cse_alife_helicopter"),
        ("SCRPTCAR", "cse_alife_car"),
        ("SCRPTART", "cse_alife_item_artefact"),
        ("SM_FLESH", "cse_alife_monster_base"),
    ] {
        assert_eq!(bases.get(class), Some(&base), "{class}'s engine base (6)");
    }
    // sav.md 8: the scripts of these keep the client data.
    for class in ["SPC_RS_S", "O_PHYS_S", "AI_TRD_S", "C_HLCP_S"] {
        assert_eq!(
            kept.get(class),
            Some(&true),
            "{class} keeps its client data"
        );
    }
    assert_eq!(kept.get("AI_STL_S"), Some(&false), "a stalker doesn't");
    // The server object an online object spawns from (spec M 10.2).
    let door = &records["O_PHYS_S"];
    let s = g.scripts();
    let part = s.server_object_for(&system, door).unwrap();
    assert_eq!(s.server_object_for(&system, door).unwrap(), part, "held");
    let id = s
        .with_simulation_object(part, |o| o.record.as_ref().map(|r| r.header.id))
        .flatten();
    assert_eq!(id, Some(door.header.id));
    s.release_server_objects().unwrap();
}
