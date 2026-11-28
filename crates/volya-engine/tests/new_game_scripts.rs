// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game of the Russian edition (BYOA) with its scripts: the engine's
//! calls into the shipped scripts from program start to the level's first
//! frames, recorded by the app, in the order and under the captions of
//! docs/behavior/script-api-engine-entry-points.md section 3 (its worked
//! example, 3.6, up to the level's client objects), with the time and the
//! game scripts' pool they take. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::io;
use std::rc::Rc;
use std::time::Instant;

use volya_engine::app::{App, AppState};
use volya_engine::audio::Silent;
use volya_engine::controls::MoveAction;
use volya_engine::core::camera::Angles;
use volya_engine::core::dialogs::conditions::DialogWorld;
use volya_engine::core::input::{Control, Event, Input, Key};
use volya_engine::core::memory_manager::GAME_SCRIPTS;
use volya_engine::core::simulation::NO_ID;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs, VfsFile};
use volya_engine::data::GameData;
use volya_engine::script::Value;
use volya_engine::script::entry_points::EngineCall;
use volya_engine::script::loader::Trees;
use volya_engine::script::services::GameType;
use volya_engine_formats::ui::Resolution;

/// The game's trees, owned for the test's lifetime.
struct GameTrees {
    tree: DbVfs<Box<dyn VfsFile + 'static>>,
    root: &'static StdVfs,
}

impl Trees for GameTrees {
    fn with_trees(&self, f: &mut dyn FnMut(&dyn Vfs, &dyn Vfs)) -> io::Result<()> {
        f(&self.tree, self.root);
        Ok(())
    }
}

fn press(input: &mut Input, key: Key) {
    input.handle(Event::Key { key, pressed: true });
    input.handle(Event::Key {
        key,
        pressed: false,
    });
}

#[test]
fn a_new_games_calls_into_scripts_before_the_first_frame() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let mut data = GameData::read(&trees.tree, None, Resolution::new(1024, 768)).unwrap();
    data.measure_intro(&trees.tree).unwrap();
    data.intro_lengths = vec![0.0; data.intro.items.len()];
    let system = data.system.clone();
    let mut app = App::new(data).unwrap();

    // Program start (3.1 step 2): startup.
    let pool = GAME_SCRIPTS.in_use_bytes();
    let start = Instant::now();
    app.start_scripts(trees.clone()).unwrap();
    let startup_ms = start.elapsed().as_secs_f64() * 1000.0;
    let startup_pool = GAME_SCRIPTS.in_use_bytes() - pool;
    let startup = EngineCall::Startup {
        registrators: vec![
            "class_registrator.register".into(),
            "game_registrator.register".into(),
        ],
        initialized: Vec::new(),
    };
    assert_eq!(app.script_calls(), std::slice::from_ref(&startup));

    let (mut input, mut audio) = (Input::new(), Silent);
    let mut t = 0.0;
    let mut step = |app: &mut App, input: &mut Input| {
        t += 16.0;
        let start = Instant::now();
        app.update(&trees.tree, t, input, &mut audio);
        input.end_frame();
        start.elapsed().as_secs_f64() * 1000.0
    };
    // The logos and the title menu call nothing.
    for _ in 0..20 {
        if matches!(app.state(), AppState::Menu { .. }) {
            break;
        }
        press(&mut input, Key::Escape);
        step(&mut app, &mut input);
    }
    assert!(matches!(app.state(), AppState::Menu { .. }));
    assert_eq!(app.script_calls().len(), 1);
    for _ in 0..2 {
        press(&mut input, Key::Enter);
        step(&mut app, &mut input);
    }

    // The captions, one per frame: each frame runs the work of the caption
    // shown before it, the engine's calls into scripts first.
    let mut by_caption: Vec<(String, Vec<EngineCall>)> = Vec::new();
    let mut caption = String::new();
    let mut script_ms = 0.0;
    let mut spawning = (0.0, 0);
    loop {
        let calls = app.script_calls().len();
        let before = GAME_SCRIPTS.in_use_bytes();
        let ms = step(&mut app, &mut input);
        if caption == "st_client_spawning" {
            spawning = (ms, GAME_SCRIPTS.in_use_bytes() - before);
        }
        let new = app.script_calls()[calls..].to_vec();
        if !new.is_empty() {
            script_ms += ms;
            by_caption.push((caption.clone(), new));
        }
        match app.state() {
            AppState::Loading {
                loading: Some(l), ..
            } => caption = l.stage.map(|s| s.caption_id().to_owned()).unwrap(),
            AppState::Loading { .. } => {}
            AppState::Level { .. } => break,
            other => panic!("left the load: {other:?}"),
        }
    }
    let new_game_pool = GAME_SCRIPTS.in_use_bytes() - pool;
    let game_type = |server: bool, result: &str| EngineCall::GameType {
        function: "game_registrator.get_game_clsid".into(),
        game_type: "single".into(),
        server,
        result: result.into(),
    };
    let want = vec![
        (
            "st_server_starting".to_owned(),
            vec![game_type(true, "SV_SINGL")],
        ),
        (
            "st_loading_alife_simulator".to_owned(),
            vec![
                EngineCall::Rebuild,
                startup,
                EngineCall::StartGameCallback("_G.start_game_callback".into()),
            ],
        ),
        (
            "st_client_connecting_to".to_owned(),
            vec![game_type(false, "CL_SINGL")],
        ),
    ];
    assert_eq!(by_caption, want);

    // No entry point called in the level's first frames (3.4), none of the
    // tutorial or weapon functions.
    let calls = app.script_calls().len();
    for _ in 0..40 {
        step(&mut app, &mut input);
    }
    assert!(matches!(app.state(), AppState::Level { .. }));
    assert_eq!(app.script_calls().len(), calls);

    // 3.3 step 11.4: the level's client objects online at the start are in
    // the scripts, through the factory, with their binding functions
    // called: the trader door's `bind_physic_object.init` attached its
    // `generic_physics_binder` (script API L 8).
    let level = app.level().unwrap();
    let sim = level.simulation().unwrap();
    let online: Vec<(u16, Option<String>)> = (sim.level_list())
        .filter_map(|id| sim.object(id))
        .filter(|o| o.online && o.record.header.parent_id == NO_ID)
        .map(|o| {
            let binding = system.get(o.record.header.section.as_str(), "script_binding");
            let binding = binding.flatten().map(|b| b.trim().to_owned());
            // The doors, and the player under the fake of its binding
            // (#1120).
            let runs = level.doors().holds(o.id) || o.id == 0;
            (o.id, binding.filter(|b| !b.is_empty() && !runs))
        })
        .collect();
    let door = (sim.level_list())
        .filter_map(|id| sim.object(id))
        .find(|o| {
            o.record.header.name.as_str() == "trader_door"
                && (o.record.object.object_block().custom_data)
                    .contains("scripts\\esc_trader_door.ltx")
        })
        .map(|o| o.id)
        .expect("the trader door");
    let vertices = sim.navigation().unwrap().ai.len();
    drop(sim);
    let scripts = app.game_scripts().unwrap();
    // TODO(#1077): every one, once the interface their binders call is
    // implemented; now those without a binding function, the doors and the
    // player.
    let expected: Vec<u16> = (online.iter())
        .filter(|(_, b)| b.is_none())
        .map(|&(id, _)| id)
        .collect();
    let spawned: Vec<u16> = {
        let objects = scripts.objects().borrow();
        (online.iter())
            .map(|&(id, _)| id)
            .filter(|&id| objects.get(id).is_some())
            .collect()
    };
    let bound = (spawned.iter())
        .filter(|&&id| scripts.scripts().attached_binder(id).is_some())
        .count();
    let left_out = online.iter().filter(|(_, b)| b.is_some()).count();
    eprintln!(
        "client objects: {} online ({left_out} left out for their binders), {} spawned, {bound} bound",
        online.len(),
        spawned.len()
    );
    // Script API K: `alife()` is the level's simulation in the game, the
    // trader (story id 3) and the player in it.
    let alife = scripts.scripts().exec(
        "=alife",
        b"local a = alife() return a ~= nil and a:story_object(3) ~= nil, \
          a ~= nil and a:actor() ~= nil, a ~= nil and a:level_name(a:level_id())",
    );
    assert_eq!(
        alife.unwrap(),
        [
            Value::Boolean(true),
            Value::Boolean(true),
            Value::String(b"l01_escape".to_vec())
        ],
        "alife() in the game"
    );
    let lights = scripts.objects().borrow().level_vertex_lights.len();
    assert_eq!(lights, vertices, "a light byte per level vertex");
    eprintln!(
        "st_client_spawning: {:.1} ms with the scripts' spawning, the game scripts' pool +{} K, \
         {lights} light bytes",
        spawning.0,
        spawning.1 / 1024
    );
    assert_eq!(spawned, expected, "the online objects spawned");
    assert!(
        scripts.scripts().attached_binder(door).is_some(),
        "the door's binder"
    );
    let handle = scripts.scripts().game_object(door).unwrap();
    scripts
        .scripts()
        .exec(
            "=door",
            br#"
function door_binder(d)
  local b = d:binded_object()
  return d:name(), d:section(), d:clsid() == clsid.script_phys, rawequal(b.st, db.storage[d:id()]),
    d:mass() > 0, d:level_vertex_light(d:level_vertex_id()) <= 1
end
"#,
        )
        .unwrap();
    let state = scripts
        .scripts()
        .call("door_binder", &[Value::Object(handle)])
        .unwrap();
    assert_eq!(
        state,
        [
            Value::from("trader_door"),
            Value::from("physic_object"),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(true),
        ],
        "the door's handle and binder"
    );

    // Escape skips the intro (startup.md 6.5): the level runs.
    press(&mut input, Key::Escape);
    step(&mut app, &mut input);
    assert!(matches!(app.state(), AppState::Level { intro: None, .. }));
    let scripts = app.game_scripts().unwrap();

    // Spec L 5.2, 8: each level frame runs the door's scheduled update,
    // its binder's `update(delta)`, `delta` the frame's 16 ms.
    scripts
        .scripts()
        .exec(
            "=count",
            br#"
updates, last_delta = 0, nil
local binder = bind_physic_object.generic_physics_binder
local update = binder.update
binder.update = function(self, delta)
  if self.object:name() == "trader_door" then
    updates, last_delta = updates + 1, delta
  end
  return update(self, delta)
end
"#,
        )
        .unwrap();
    for _ in 0..10 {
        step(&mut app, &mut input);
    }
    let scripts = app.game_scripts().unwrap();
    let counted = scripts
        .scripts()
        .exec("=counted", b"return updates, last_delta")
        .unwrap();
    assert_eq!(
        counted,
        [Value::Number(10.0), Value::Number(16.0)],
        "the door's updates"
    );

    // Script API O 5, 6.1 and 10 in the app's level frames: the door
    // scheme's fast call (`close_door`) ran until the door was closed; the
    // player's use action on the door raises its `use_object` on the
    // binder's `use_callback`, as its update sets it (here wrapped to
    // record the arguments).
    assert_eq!(scripts.scripts().level_calls().unwrap(), 0);
    scripts
        .scripts()
        .exec(
            "=door_calls",
            br#"
local binder = bind_physic_object.generic_physics_binder
local use = binder.use_callback
binder.use_callback = function(self, obj, who)
  if obj:name() == "trader_door" then
    door_used = { rawequal(self, obj:binded_object()), obj:id(), who and who:id() }
  end
  return use(self, obj, who)
end
function door_calls_read()
  if not door_used then return nil end
  return door_used[1], door_used[2], door_used[3]
end
"#,
        )
        .unwrap();

    // Facing the leaf from 0.6 m in front of it, a use.
    let level = app.level_mut().unwrap();
    let leaf = level.doors().boxes()[0];
    let start = level.player().state().position;
    let k = (0..3)
        .min_by(|&a, &b| leaf.half_size[a].total_cmp(&leaf.half_size[b]))
        .unwrap();
    let mut n = leaf.axes[k];
    let side =
        |p: [f32; 3], n: [f32; 3]| (0..3).map(|i| (p[i] - leaf.centre[i]) * n[i]).sum::<f32>();
    if side(start, n) < 0.0 {
        n = n.map(|v| -v);
    }
    let d = leaf.half_size[k] + 0.6;
    let feet = [
        leaf.centre[0] + n[0] * d,
        start[1],
        leaf.centre[2] + n[2] * d,
    ];
    let h = (level.player().config).eye_height(level.player().body().box_index());
    let angles = Angles {
        yaw: (-n[0]).atan2(-n[2]),
        pitch: 0.0,
        roll: 0.0,
    };
    level.set_camera([feet[0], feet[1] + h, feet[2]], angles);
    let Some(Control::Key(use_key)) = (level.controls().bindings)
        .controls(MoveAction::Use)
        .find(|c| matches!(c, Control::Key(_)))
    else {
        panic!("a key bound to use");
    };
    step(&mut app, &mut input);
    step(&mut app, &mut input);
    assert_eq!(app.level().unwrap().doors().picked_object(), Some(door));
    press(&mut input, use_key);
    step(&mut app, &mut input);
    let scripts = app.game_scripts().unwrap();
    assert_eq!(
        scripts.scripts().call("door_calls_read", &[]).unwrap(),
        [
            Value::Boolean(true),
            Value::Number(f64::from(door)),
            Value::Number(0.0),
        ],
        "the door's use callback: the binder, the door, the user"
    );

    // Script API Y (11): the door scheme's calls on the door's shell act on
    // the level's door. Its limits come back as the hinge's, seen from its
    // fixed frame element; the opening motor of its sections, (2100, −3),
    // turns the leaf from the pose it was made in.
    scripts
        .scripts()
        .exec(
            "=door_scheme",
            br#"
function door_joint(d)
  local ph_shell = d:get_physics_shell()
  local joint = ph_shell:get_joint_by_bone_name("door")
  local low, hi = 0, 0
  low, hi = joint:get_limits(low, hi, 0)
  return joint, low, hi
end
function door_scheme(d, force, velocity)
  local joint, low, hi = door_joint(d)
  joint:set_max_force_and_velocity(force, velocity, 0)
  return low, hi, joint:get_axis_angle(90)
end
function door_angle(d)
  return (door_joint(d)):get_axis_angle(90)
end
"#,
        )
        .unwrap();
    let before = app
        .level()
        .unwrap()
        .physics()
        .hinge_angle(door, "door")
        .unwrap();
    let scripts = app.game_scripts().unwrap();
    let handle = scripts.scripts().game_object(door).unwrap();
    let opened = scripts
        .scripts()
        .call(
            "door_scheme",
            &[Value::Object(handle), 2100.0.into(), (-3.0).into()],
        )
        .unwrap();
    let number = |v: &Value| match v {
        Value::Number(n) => *n as f32,
        other => panic!("not a number: {other:?}"),
    };
    let (low, high) = (number(&opened[0]), number(&opened[1]));
    assert!(
        low <= 0.0 && high > 1.0,
        "the door's limits ({low}, {high})"
    );
    // The opening motor acts as the level runs.
    for _ in 0..30 {
        step(&mut app, &mut input);
    }
    let after = app
        .level()
        .unwrap()
        .physics()
        .hinge_angle(door, "door")
        .unwrap();
    let scripts = app.game_scripts().unwrap();
    let angle = scripts
        .scripts()
        .call("door_angle", &[Value::Object(handle)])
        .unwrap();
    let angle = number(&angle[0]);
    eprintln!(
        "the door scheme: limits ({low}, {high}), the hinge from {before} to {after}, the \
         script's angle {angle}"
    );
    assert!(
        (after - before).abs() > 0.1,
        "the scripts' motor turns the door: {before} -> {after}"
    );
    assert_eq!(angle, after, "the script reads the level's hinge");

    // Script API P over the game's own portions: a give to the player
    // through the scripts' story (as a dialog's effect gives it) is the
    // level's, and the scripts see it a frame later.
    let scripts = app.game_scripts().unwrap();
    (scripts.scripts().give_to_player("esc_trader_can_leave")).unwrap();
    step(&mut app, &mut input);
    let held = {
        let portions = app.level().unwrap().info_portions().unwrap().borrow();
        portions.holds(portions.player(), "esc_trader_can_leave")
    };
    assert!(held, "the level's portions have the scripts' give");
    let scripts = app.game_scripts().unwrap();
    assert!(
        scripts.scripts().player_holds("esc_trader_can_leave"),
        "the scripts see it"
    );

    // The services as the load left them: the game type and app_ready().
    let scripts = app.game_scripts().unwrap();
    {
        let services = scripts.services().borrow();
        assert_eq!(services.game_type, GameType::Single);
        assert!(services.app_ready());
        assert_eq!(services.device.precache_frame(), 0, "the phase ended");
    }
    // The start callback's namespaces are loaded in the new state.
    let loaded = scripts
        .scripts()
        .exec(
            "=t",
            b"return type(rawget(_G, 'task_manager')), type(rawget(_G, 'dialog_manager'))",
        )
        .unwrap();
    assert_eq!(loaded, [Value::from("table"), "table".into()]);
    eprintln!(
        "scripts: startup {startup_ms:.1} ms, game scripts' pool {} K; the New Game's frames \
         with calls into scripts {script_ms:.1} ms, the pool after it {} K",
        startup_pool / 1024,
        new_game_pool / 1024
    );
}
