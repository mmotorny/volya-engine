// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A game state started with the shipped scripts over a New Game's
//! simulation and `l01_escape` (its game clock, patrol paths and level
//! vertices), with an `esc_lager` campfire stalker spawned bound by its
//! section's binding, `xr_motivator`'s (script API W 12.1; CH-17 8).
//! Included by the tests with `#[path]`, beside `gog_archives` and
//! `shipped_scripts`.

// Each test uses one of the two set-ups.
#![allow(dead_code)]

use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::Arc;

use super::gog_archives;
use super::shipped_scripts::{FSGAME, GameTrees};
use volya_engine_core::character_identity::trader_like_mut;
use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::info_portions::{Declarations, InfoPortions};
use volya_engine_core::simulation::graph::Graph;
use volya_engine_core::simulation::{AlifeConfig, Simulation};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, VfsFile, read};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::level_ai::LevelAi;
use volya_engine_formats::level_cform::CformReader;
use volya_engine_formats::object::{Kind, ObjectRecord};
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{EngineState, GameObjects, ObjectClass, SpawnData};
use volya_engine_script::level::{Level, LevelVertices};
use volya_engine_script::loader::Prefix;
use volya_engine_script::services::EngineServices;
use volya_engine_script::simulation::AlifeSimulator;
use volya_engine_script::story::Story;

/// The system configuration as the engine loads it (`system.ltx`).
fn system(
    vfs: &DbVfs<Box<dyn volya_engine_core::vfs::VfsFile + 'static>>,
    aliases: &PathAliases,
) -> Config {
    config::load_aliased(vfs, aliases, "$game_config$", "system.ltx").unwrap()
}

/// A started game state over a New Game's simulation, and the id of an
/// `esc_lager` stalker spawned bound by its section's binding: the
/// spawning's result too. With `logic`, the stalker's custom data is it
/// and the shipped logic set-up runs; without, the logic set-up is, in its
/// place, what it does for the camp scheme's first start (12.1),
/// `xr_kamp.add_to_binder` (the job's logic is CH-28's, TODO(#1215)).
pub fn started(logic: Option<&[u8]>) -> Option<(GameScripts, u16, Result<(), String>)> {
    started_with(logic, |_, _| EngineState::default())
}

/// [`started`], the stalker given what `prepare` makes from the game's
/// tree and its record as the engine's state as it spawns (spec M 3).
pub fn started_with(
    logic: Option<&[u8]>,
    prepare: impl FnOnce(&DbVfs<Box<dyn VfsFile + 'static>>, &ObjectRecord) -> EngineState,
) -> Option<(GameScripts, u16, Result<(), String>)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let game_trees = Rc::clone(&trees);
    let vfs = &trees.tree;
    let system = system(vfs, &aliases);
    // The game's information portions, as a New Game sets them (the
    // scripts may give some as the stalker spawns: measured, when it is
    // the process's second spawning).
    let declarations = Declarations::read(vfs, &aliases, &system).unwrap();
    let story = Rc::new(RefCell::new(Story::new()));
    story.borrow_mut().info_portions = Some(Rc::new(RefCell::new(InfoPortions::new(
        Arc::new(declarations),
        0,
    ))));
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
    let ai = read(vfs, "gamedata/levels/l01_escape/level.ai").unwrap();
    let (ai, lights, links) = LevelAi::read_in_with_lights_and_links(
        Cursor::new(&ai[..]),
        |len| Ok(vec![0; len]),
        |len| Ok(vec![0; len]),
    )
    .unwrap();
    let cform = read(vfs, "gamedata/levels/l01_escape/level.cform").unwrap();
    let header = *CformReader::new(Cursor::new(&cform[..])).unwrap().header();
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
    let alife = Rc::new(RefCell::new(AlifeSimulator::new()));
    let mut scripts = GameScripts::start(Startup {
        trees,
        aliases,
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::clone(&story),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::clone(&alife),
        window_files: Rc::default(),
        level: Rc::default(),
        effects_and_weather: Rc::default(),
    })
    .unwrap();
    let stalker = sim
        .objects()
        .find(|o| {
            o.record.kind() == Kind::Stalker
                && o.record.header.name.as_str().starts_with("esc_lager")
        })
        .map(|o| o.id)
        .unwrap();
    // The New Game without identities leaves the record's community unset
    // (character-identity.md 4.2), which `xr_motivator` reads: its
    // position in `[game_relations] communities` as identities set it.
    let communities = system
        .get("game_relations", "communities")
        .flatten()
        .unwrap();
    let names: Vec<&str> = communities.split(',').map(str::trim).step_by(2).collect();
    let position = names.iter().position(|&n| n == "stalker").unwrap();
    let object = sim.object_mut(stalker).unwrap();
    trader_like_mut(&mut object.record).unwrap().community = position as i32;
    let record = sim.object(stalker).unwrap().record.clone();
    let mut data = SpawnData::from_record(stalker, &record, &system).unwrap();
    let state = prepare(&game_trees.tree, &record);
    scripts.objects().borrow_mut().prepare(stalker, state);
    if let Some(logic) = logic {
        data.custom_data = logic.to_vec();
    }
    {
        let mut a = alife.borrow_mut();
        a.simulation = Some(sim);
        a.graph_file = Some(graph_file.into());
    }
    // The game clock at its start (our choice: the logic set-up reads it,
    // nothing here depends on its value).
    scripts.times().borrow_mut().level_frame(0, 1.0);
    // The level's vertex count, which `set_dest_level_vertex_id` reads.
    scripts.objects().borrow_mut().level_vertex_lights = lights;
    scripts.level().borrow_mut().patrol_paths =
        Some(Rc::new(PatrolPaths::parse(&spawn_file).unwrap()));
    scripts
        .scripts()
        .start_level(Level {
            name: Some(b"l01_escape".to_vec()),
            bounding_volume: (header.bbox_min, header.bbox_max),
            vertices: Some(LevelVertices {
                ai: Arc::new(ai),
                links: Arc::new(links),
            }),
        })
        .unwrap();
    // The engine's reinitialisation sets the stalker's sound prefix to its
    // specific character's `snd_config` (creature-sounds.md 1.2 step 2):
    // the empty text without one, as this New Game leaves it.
    {
        let mut sounds = scripts.sounds().borrow_mut();
        let player = sounds.creatures.players.entry(stalker).or_default();
        player.prefix = Some(Vec::new());
    }
    let s = scripts.scripts();
    // Without `logic`, what the motivator's `net_spawn` does after its
    // planner part and its sound loading stops on other specs' names: the
    // logic set-up (TODO(#1077)), in whose place what it does for the camp
    // scheme's first start (12.1), `xr_kamp.add_to_binder`.
    if logic.is_none() {
        s.exec(
            "=fake_logic",
            b"xr_gulag.setup_gulag_and_logic_on_spawn = function(obj)\n\
              xr_kamp.add_to_binder(obj, nil, 'kamp', 'kamp1@esc_lager', {})\n\
              end",
        )
        .unwrap();
    }
    let server = s
        .server_object_for(&system, &record)
        .map_err(|e| e.to_string());
    let spawned = server.and_then(|server| {
        data.server = Some(server);
        s.spawn_engine_object(ObjectClass::Named("CAI_Stalker"), data)
            .map(drop)
            .map_err(|e| e.to_string())
    });
    Some((scripts, stalker, spawned))
}
