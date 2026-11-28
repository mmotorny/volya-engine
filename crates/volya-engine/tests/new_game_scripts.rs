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
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::memory_manager::GAME_SCRIPTS;
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
    loop {
        let calls = app.script_calls().len();
        let ms = step(&mut app, &mut input);
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

    // No call into scripts in the level's first frames (3.4; no binder
    // yet, #1027), and none of the tutorial or weapon functions.
    let calls = app.script_calls().len();
    for _ in 0..40 {
        step(&mut app, &mut input);
    }
    assert!(matches!(app.state(), AppState::Level { .. }));
    assert_eq!(app.script_calls().len(), calls);

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
