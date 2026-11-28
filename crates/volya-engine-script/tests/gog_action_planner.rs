// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's action planner, checked against
//! docs/behavior/script-api-action-planner.md (cited by section): on a
//! game state started with the shipped scripts over a New Game's
//! simulation, an `esc_lager` campfire stalker spawns bound by its
//! section's binding, `xr_motivator`'s, whose `reinit` and `net_spawn`
//! build the shipped scripts' planners on its motivation manager (10,
//! 12.1): `state_mgr`'s evaluators and actions and its own body-state
//! planner, the anomaly evaluator replaced by a constant, the script
//! themes' sound collections on the stalker's profile, and the camp
//! scheme's parts (its `add_to_binder` in place of the logic set-up the
//! scripts reach later, on other specs' names). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::rc::Rc;

use shipped_scripts::{FSGAME, GameTrees};
use volya_engine_core::character_identity::trader_like_mut;
use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::simulation::graph::Graph;
use volya_engine_core::simulation::{AlifeConfig, Simulation};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, read};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::object::Kind;
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_script::Value;
use volya_engine_script::action_planner::{Evaluation, Evaluator, WorldState};
use volya_engine_script::classes::EnginePart;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{GameObjects, ObjectClass, SpawnData};
use volya_engine_script::loader::{Prefix, Scripts};
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
/// spawning's result too.
fn started() -> Option<(GameScripts, u16, Result<(), String>)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let vfs = &trees.tree;
    let system = system(vfs, &aliases);
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
        story: Rc::new(RefCell::new(Story::new())),
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
    {
        let mut a = alife.borrow_mut();
        a.simulation = Some(sim);
        a.graph_file = Some(graph_file.into());
    }
    // The engine's reinitialisation sets the stalker's sound prefix to its
    // specific character's `snd_config` (creature-sounds.md 1.2 step 2):
    // the empty text without one, as this New Game leaves it.
    {
        let mut sounds = scripts.sounds().borrow_mut();
        let player = sounds.creatures.players.entry(stalker).or_default();
        player.prefix = Some(Vec::new());
    }
    let s = scripts.scripts();
    // What the motivator's `net_spawn` does after its planner part and its
    // sound loading stops on other specs' names: the logic set-up
    // (TODO(#1077)), in whose place what it does for the camp scheme's
    // first start (12.1), `xr_kamp.add_to_binder`.
    s.exec(
        "=fake_logic",
        b"xr_gulag.setup_gulag_and_logic_on_spawn = function(obj)\n\
          xr_kamp.add_to_binder(obj, nil, 'kamp', 'kamp1@esc_lager', {})\n\
          end",
    )
    .unwrap();
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

/// The ids, preconditions and effects of a planner's actions, as (id,
/// value) pairs.
type Conditions = Vec<(u32, Vec<(u32, bool)>, Vec<(u32, bool)>)>;

/// A planner's evaluators' ids and its actions' conditions.
fn conditions(s: &Scripts, planner: EnginePart) -> (Vec<u32>, Conditions) {
    let pairs = |w: &WorldState| -> Vec<(u32, bool)> {
        w.properties()
            .iter()
            .map(|p| (p.condition, p.value))
            .collect()
    };
    s.with_planner(planner, |p| {
        let evaluators = p.evaluators.iter().map(|e| e.0).collect();
        let actions = (p.actions.iter())
            .map(|a| {
                let action = a.action.borrow();
                (a.id, pairs(&action.preconditions), pairs(&action.effects))
            })
            .collect();
        (evaluators, actions)
    })
    .unwrap()
}

/// 10, 12.1: the campfire stalker's binder builds the shipped scripts'
/// planners on its motivation manager, and `state_mgr`'s own planner.
#[test]
fn a_campfire_stalker_builds_its_planners() {
    let Some((mut scripts, stalker, spawned)) = started() else {
        return;
    };
    let collections = (scripts.sounds().borrow().creatures.players)
        .get(&stalker)
        .map_or(0, |p| p.collections.len());
    let s = scripts.scripts();
    assert_eq!(spawned, Ok(()), "spawning; {:?}", s.take_failures());
    // `net_spawn`'s sound loading, on its profile (`profile_name()`):
    // the script themes' collections (creature-sounds.md 1.2 step 3).
    eprintln!("its sound collections: {collections}");
    assert!(collections > 0, "its sound collections");
    let manager = s
        .motivation_manager(stalker)
        .expect("its motivation manager");
    let (evaluators, actions) = conditions(s, manager);
    let ids: Vec<u32> = actions.iter().map(|a| a.0).collect();
    // The engine's (12.2), `state_mgr.bind_manager`'s at `reinit`
    // (`xr_evaluators_id.state_mgr` + 1, + 2 = 145, 146;
    // `xr_actions_id.state_mgr` + 1, + 2 = 162, 163) and the camp scheme's
    // (12.1).
    assert_eq!(evaluators, [0, 2, 4, 6, 7, 8, 42, 145, 146, 1065, 1066]);
    assert_eq!(ids, [2, 56, 57, 58, 59, 60, 162, 163, 202, 204]);
    let of = |id: u32| actions.iter().find(|a| a.0 == id).unwrap();
    // `state_mgr`'s preconditions on the engine's actions: the alife
    // planner and gather items need "idle in life", the combat, anomaly
    // and danger planners "idle in combat"; the camp scheme's on the alife
    // planner.
    let needs = [
        (57, 146),
        (2, 146),
        (58, 145),
        (59, 145),
        (60, 145),
        (57, 1065),
    ];
    for (action, condition) in needs {
        assert!(
            of(action).1.contains(&(condition, true)),
            "action {action} needs {condition}"
        );
    }
    let common = [(1075, false), (1164, false), (2054, false), (3144, false)];
    for (action, at, effect) in [(204, true, (1065, true)), (202, false, (1066, true))] {
        let mut want = vec![(0, true), (7, false), (8, false), (42, false), (1066, at)];
        want.extend(common);
        want.sort();
        assert_eq!(of(action).1, want, "action {action}'s preconditions");
        assert_eq!(of(action).2, [effect], "action {action}'s effect");
    }
    // `net_spawn`: the anomaly evaluator a constant false.
    let anomaly = (s.with_planner(manager, |p| p.evaluators[6])).unwrap();
    assert_eq!(anomaly.0, 42);
    let constant = s.with_part(anomaly.1, |e: &mut Evaluator| {
        matches!(e.evaluation, Evaluation::Constant(false))
    });
    assert_eq!(constant, Some(true), "the anomaly evaluator");
    // `state_mgr_goap`'s body-state planner, set up with the stalker.
    let code = format!(
        "local p = db.storage[{stalker}].state_mgr.planner\n\
         return p, p.object:id(), p:initialized()"
    );
    let got = s.exec("=body_state", code.as_bytes()).unwrap();
    let [
        Value::Object(body_state),
        Value::Number(object),
        Value::Boolean(false),
    ] = got[..]
    else {
        panic!("the body-state planner: {got:?}");
    };
    assert_eq!(object, f64::from(stalker));
    let (evaluators, actions) = conditions(s, body_state);
    eprintln!(
        "the body-state planner: {} evaluators, {} actions",
        evaluators.len(),
        actions.len()
    );
    assert!(!evaluators.is_empty() && !actions.is_empty());
    // One update (6.3), as the binder's `update` runs it (CH-17 4.2): it
    // plans and runs the plan's first action.
    let code = format!(
        "local p = db.storage[{stalker}].state_mgr.planner\n\
         p:update()\n\
         return p:initialized(), p:current_action_id()"
    );
    let got = s.exec("=update", code.as_bytes()).unwrap();
    let plan = s.with_planner(body_state, |p| p.plan.clone()).unwrap();
    eprintln!("its plan: {plan:?}");
    assert!(!plan.is_empty(), "a plan");
    assert_eq!(
        got,
        [Value::Boolean(true), Value::Number(f64::from(plan[0]))]
    );
}
