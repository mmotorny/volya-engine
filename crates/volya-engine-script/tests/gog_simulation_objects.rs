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
use volya_engine_core::character_identity::trader_like_mut;
use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::simulation::graph::Graph;
use volya_engine_core::simulation::{AlifeConfig, Simulation, class_of};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, read};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::object::{Kind, ObjectRecord, section};
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_formats::spawn;
use volya_engine_script::Value;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::GameObjects;
use volya_engine_script::loader::Prefix;
use volya_engine_script::object_factory::CreatedObject;
use volya_engine_script::services::EngineServices;
use volya_engine_script::simulation::AlifeSimulator;
use volya_engine_script::simulation_objects::{CLASSES, Creation, classes, dispatched};
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
    started_over(trees, aliases, Rc::default())
}

/// [`started`] over script API K's simulation state `simulation`.
fn started_over(
    trees: Rc<GameTrees>,
    aliases: &PathAliases,
    simulation: Rc<RefCell<AlifeSimulator>>,
) -> (GameScripts, Config) {
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
        simulation,
        level: Rc::default(),
        effects_and_weather: Rc::default(),
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

/// What `alife():object` gives for the `esc_lager` smart terrain, one of
/// its stalkers and a box of ammunition: the script objects' fields their
/// constructors set (`se_smart_terrain`, `se_stalker`), their class ids,
/// the same object twice, and the engine object's section.
fn objects_as_scripts_get_them(g: &mut GameScripts, ids: [u16; 3]) -> Vec<Value> {
    let [terrain, stalker, ammo] = ids;
    let code = format!(
        "local sim = alife()\n\
         local t, s, a = sim:object({terrain}), sim:object({stalker}), sim:object({ammo})\n\
         return t.registred, t.initialized, t:clsid() == clsid.smart_terrain, \
         s.was_in_smart_terrain, s:clsid() == clsid.script_stalker, sim:object({stalker}) == s, \
         a:section_name()"
    );
    g.scripts().exec("=check", code.as_bytes()).unwrap()
}

/// Spec C 6.1 and 2.2 at a New Game and a load (offline-simulation.md 8.1,
/// 8.2): every object of the simulation gets its server object through the
/// object factory, scripted ones their script objects, which `alife()`
/// hands out; `release` and the simulation's end release them.
#[test]
fn a_new_games_and_a_loads_objects_through_the_factory() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let spawn_file = read(&tree, "gamedata/spawns/all.spawn").unwrap();
    let graph_file = read(&tree, "gamedata/game.graph").unwrap();
    let alife_ltx = config::load_aliased(&tree, &aliases, "$game_config$", "alife.ltx").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let alife = Rc::new(RefCell::new(AlifeSimulator::new()));
    let (mut g, system) = started_over(trees, &aliases, Rc::clone(&alife));
    let new_simulation = || {
        Simulation::new(
            Graph::new(GameGraph::parse(&graph_file).unwrap()),
            PatrolPaths::parse(&spawn_file).unwrap(),
            AlifeConfig::read(&alife_ltx, &system).unwrap(),
        )
    };
    let mut sim = new_simulation();
    sim.new_game(&spawn_file, &system, &mut |_, _| Ok(5), None)
        .unwrap();
    // The New Game without identities leaves the characters' communities
    // unset (character-identity.md 4.2), which their registration reads:
    // `stalker`'s position in `[game_relations] communities` instead.
    let communities = system
        .get("game_relations", "communities")
        .flatten()
        .unwrap();
    let names: Vec<&str> = communities.split(',').map(str::trim).step_by(2).collect();
    let position = names.iter().position(|&n| n == "stalker").unwrap() as i32;
    let ids: Vec<u16> = sim.objects().map(|o| o.id).collect();
    for id in ids {
        let record = &mut sim.object_mut(id).unwrap().record;
        if let Some(t) = trader_like_mut(record)
            && !(0..names.len() as i32).contains(&t.community)
        {
            t.community = position;
        }
    }
    let find = |sim: &Simulation, f: &dyn Fn(&ObjectRecord) -> bool| {
        sim.objects().find(|o| f(&o.record)).map(|o| o.id).unwrap()
    };
    let terrain = find(&sim, &|r| {
        r.class_id.as_str() == "SMRTTRRN" && r.header.name.as_str() == "esc_lager"
    });
    let stalker = find(&sim, &|r| {
        r.kind() == Kind::Stalker && r.header.name.as_str().starts_with("esc_lager")
    });
    let ammo = find(&sim, &|r| r.kind() == Kind::Ammunition);
    let ammo_section = sim.object(ammo).unwrap().record.header.section.to_string();
    let records: Vec<ObjectRecord> = sim.objects().map(|o| o.record.clone()).collect();
    let count = records.len();
    // The game's patrol paths, as the app gives them at a New Game.
    g.level().borrow_mut().patrol_paths = Some(Rc::new(sim.paths.clone()));
    alife.borrow_mut().simulation = Some(sim);

    let held = g.scripts().simulation_objects().unwrap();
    let objects = || held.clone();
    let start = std::time::Instant::now();
    g.scripts()
        .create_simulation_objects(&system, Creation::NewGame)
        .unwrap();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(objects().borrow().held_count(), count, "one per object");
    let terrain_part = objects().borrow().held(terrain).unwrap();
    // The smart terrain registered and set up (its `STATE_Read` read its
    // configuration, its `on_register` ran).
    let want = [
        Value::Boolean(true),
        Value::Boolean(true),
        Value::Boolean(true),
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Boolean(true),
        Value::from(ammo_section.as_str()),
    ];
    let ids = [terrain, stalker, ammo];
    assert_eq!(objects_as_scripts_get_them(&mut g, ids), want, "New Game");
    // Made once.
    g.scripts()
        .create_simulation_objects(&system, Creation::NewGame)
        .unwrap();
    assert_eq!(objects().borrow().held(terrain), Some(terrain_part));

    // Script API K's `release` of the (offline) stalker: their server objects
    // goes with the engine's next release.
    let code = format!("alife():release(alife():object({stalker}), true)");
    g.scripts().exec("=release", code.as_bytes()).unwrap();
    assert_eq!(objects().borrow().held_count(), count);
    g.scripts().release_simulation_objects(&[]).unwrap();
    // The stalker and the items it carries (its supplies).
    let left = alife.borrow().simulation.as_ref().unwrap().len();
    assert!(left < count, "released");
    assert_eq!(objects().borrow().held_count(), left, "released");
    assert_eq!(objects().borrow().held(stalker), None);

    // A load of the New Game's records: the same objects again, in
    // increasing id.
    g.scripts().release_server_objects().unwrap();
    assert_eq!(objects().borrow().held_count(), 0, "the simulation's end");
    let mut loaded = new_simulation();
    loaded.load(records, &system).unwrap();
    alife.borrow_mut().simulation = Some(loaded);
    g.scripts()
        .create_simulation_objects(&system, Creation::Load)
        .unwrap();
    assert_eq!(objects().borrow().held_count(), count, "loaded");
    assert_eq!(objects_as_scripts_get_them(&mut g, ids), want, "load");
    eprintln!(
        "{count} objects made through the factory in {ms:.1} ms; failures {:?}",
        g.scripts().take_failures()
    );
    g.scripts().release_server_objects().unwrap();
}
