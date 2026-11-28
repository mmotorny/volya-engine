// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's simulation registry and game graph through the
//! shipped scripts and data, checked against
//! docs/behavior/script-api-simulation-registry-and-game-graph.md (cited
//! by section): the story-id constants of the shipped configuration (4.2),
//! the story registry of a New Game (4.3), the shipped smart terrain's
//! level-name expression and default task (10, 11) and the members the
//! scripts call (14). Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::rc::Rc;

use shipped_scripts::{FSGAME, GameTrees};
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::simulation::graph::Graph;
use volya_engine_core::simulation::{AlifeConfig, Simulation};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, read};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_script::Value;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::GameObjects;
use volya_engine_script::loader::Prefix;
use volya_engine_script::services::EngineServices;
use volya_engine_script::simulation::AlifeSimulator;
use volya_engine_script::story::Story;

/// A started game state (spec D) whose simulation is a New Game's.
fn start() -> Option<(GameScripts, Rc<RefCell<AlifeSimulator>>)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let vfs = &trees.tree;
    let system = config::load_aliased(vfs, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(vfs, &aliases, "$game_config$", "game.ltx").unwrap();
    let alife_ltx = config::load_aliased(vfs, &aliases, "$game_config$", "alife.ltx").unwrap();
    let spawn_file = read(vfs, "gamedata/spawns/all.spawn").unwrap();
    let graph_file = read(vfs, "gamedata/game.graph").unwrap();
    let mut sim = Simulation::new(
        Graph::new(GameGraph::parse(&graph_file).unwrap()),
        PatrolPaths::parse(&spawn_file).unwrap(),
        AlifeConfig::read(&alife_ltx, &system).unwrap(),
    );
    sim.new_game(&spawn_file, &system, &mut |_, _| Ok(5), None)
        .unwrap();
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    let files = Rc::new(RefCell::new(FileSystem::new(
        trees.clone(),
        aliases.clone(),
        Rc::new(system),
        Rc::new(game),
    )));
    let alife = Rc::new(RefCell::new(AlifeSimulator::new()));
    let scripts = GameScripts::start(Startup {
        trees,
        aliases,
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::clone(&alife),
        window_files: Rc::default(),
        level: Rc::default(),
    })
    .unwrap();
    {
        let mut a = alife.borrow_mut();
        a.simulation = Some(sim);
        a.graph_file = Some(graph_file.into());
    }
    Some((scripts, alife))
}

fn values(s: &mut GameScripts, code: &str) -> Vec<Value> {
    s.scripts().exec("=check", code.as_bytes()).unwrap()
}

fn number(v: f64) -> Value {
    Value::Number(v)
}

#[test]
fn shipped_story_ids_and_registry() {
    let Some((mut s, alife)) = start() else {
        return;
    };
    // 4.2: the configuration's constants, the two the listing lacks among
    // them.
    let r = values(
        &mut s,
        "\
         return story_ids.Escape_blockpost_case, story_ids.Escape_Trader, story_ids.Invalid, \
         story_ids.escape_provodnik, story_ids.agr_doctor_spot, story_ids.INVALID_STORY_ID, \
         spawn_story_ids.INVALID_SPAWN_STORY_ID",
    );
    assert_eq!(r, [0.0, 3.0, 65535.0, 92.0, 350.0, -1.0, -1.0].map(number));
    // 4.3: 503 records with a story id; the trader has 3.
    let a = alife.borrow();
    let sim = a.simulation.as_ref().unwrap();
    assert_eq!(sim.story_registry().len(), 503);
    let trader = sim.object(sim.by_story_id(3).unwrap()).unwrap();
    assert_eq!(trader.record.header.name.as_str(), "escape_trader");
    drop(a);
    let r = values(
        &mut s,
        "return alife():story_object(story_ids.Escape_Trader) ~= nil, alife():story_object(19) ~= nil, \
         alife():story_object(-1)",
    );
    assert_eq!(r, [Value::Boolean(true), Value::Boolean(true), Value::Nil]);
}

#[test]
fn shipped_graph_and_default_task() {
    let Some((mut s, _)) = start() else {
        return;
    };
    // 11: `simulation_default_path`'s one point is on vertex 1,545, the
    // first of `l07_military`; 10.3: `smart_terrain.script`'s level name.
    let r = values(
        &mut s,
        "local t = CALifeSmartTerrainTask('simulation_default_path') \
         local v = t:game_vertex_id() \
         return v, alife():level_name(game_graph():vertex(v):level_id()), \
         game_graph():valid_vertex_id(3511), game_graph():valid_vertex_id(3512), \
         alife():level_name(1), alife():actor() ~= nil, alife():level_id()",
    );
    assert_eq!(
        r,
        [
            number(1545.0),
            Value::String(b"l07_military".to_vec()),
            Value::Boolean(true),
            Value::Boolean(false),
            Value::String(b"l01_escape".to_vec()),
            Value::Boolean(true),
            number(1.0),
        ]
    );
}

#[test]
fn members_the_scripts_call() {
    let Some((mut s, _)) = start() else {
        return;
    };
    // 14: every member the shipped scripts call on `alife()` is there.
    let r = values(
        &mut s,
        "local a = alife() local missing = {} \
         for _, m in ipairs({'create', 'story_object', 'object', 'set_switch_online', \
           'set_switch_offline', 'release', 'create_ammo', 'actor', 'has_info', 'level_id', \
           'level_name', 'set_interactive', 'kill_entity'}) do \
           if a[m] == nil then table.insert(missing, m) end end \
         return table.concat(missing, ' '), alife():has_info(0, 'no_such_portion')",
    );
    assert_eq!(r, [Value::String(Vec::new()), Value::Boolean(false)]);
}
