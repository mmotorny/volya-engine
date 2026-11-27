// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game from the Russian edition (BYOA): its loading screen
//! (docs/behavior/loading-screen.md), which level loads
//! (docs/behavior/level-loading.md section 2.2), the start view
//! (docs/behavior/start-position.md) with the load time, the game clock,
//! the player character and the placed effect, the intro and the menu
//! during the game, a missing level sound and the loading stages' times.
//! Its drawing (the start and outdoor views, the loading screen's and the
//! intro's frames, the texture paths, the prepared textures taken, the
//! placed effect drawn) is the render-level suite's (`selftest::new_game`).
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::cell::Cell;
use std::io;
use std::time::Instant;

use volya_engine::app::{App, AppState};
use volya_engine::audio::{AudioOut, Voice};
use volya_engine::core::audio::Playback;
use volya_engine::core::diagnostics::{self, Diagnostics};
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::sound::system::SoundSystem;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{self, DirEntry, StdVfs, Vfs, VfsFile};
use volya_engine::data::GameData;
use volya_engine::level::{self, LevelScreen};
use volya_engine::loading_screen::Stage;
use volya_engine::scene::TextureId;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

#[test]
fn new_game_start_view_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();

    let (name, actor) = level::new_game_level(&tree, &data).unwrap();
    assert_eq!((name.as_str(), actor.game_graph_vertex), ("l01_escape", 4));

    let start = Instant::now();
    let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();
    let loaded = start.elapsed();
    let state = screen.state();
    assert_eq!(state.draws, 7_782);
    for (got, want) in state.eye.iter().zip([-246.7263, -23.26326, -134.43387]) {
        assert!((got - want).abs() < 1e-4, "eye {:?}", state.eye);
    }
    // start-position.md 7.1: two modifiers apply at the start eye.
    assert!(
        (state.far_plane - 254.478).abs() < 1e-3,
        "{}",
        state.far_plane
    );
    eprintln!("level: loaded in {loaded:.1?}");

    // Outdoors, above the start (the view the render-level suite draws
    // there; the placed effect's updates below depend on it).
    let outdoors = || {
        (
            [-180.0, 5.0, -120.0],
            volya_engine::core::camera::Angles {
                yaw: 1.2,
                pitch: 0.15,
                roll: 0.0,
            },
        )
    };
    let (eye, angles) = outdoors();
    screen.set_camera(eye, angles);

    // The game clock (sav.md sections 5, 15): 205 real seconds at ×10
    // later it is 06:04:10, and the sky blends the next hour's pair.
    assert_eq!(screen.state().game_time, 63_471_533_400_000);
    screen.update(205_000, &Input::new());
    let state = screen.state();
    assert_eq!(state.game_time, 63_471_533_400_000 + 2_050_000);
    assert_eq!(state.time_of_day, 6.0 * 3600.0 + 250.0);
    assert_eq!(
        state.weather_sections,
        ["default_weather_06", "default_weather_07"]
    );
    assert!((state.weather_t - 250.0 / 3600.0).abs() < 1e-6);
    // The frame binds the cube maps of [default_weather_06] and _07 (index
    // 6 of the cycle in time order, from 00:00).
    let (eye, angles) = outdoors();
    screen.set_camera(eye, angles);
    let scene = screen.scene(SIZE);
    assert_eq!(scene.world.unwrap().sky.unwrap().section, 6);

    // The player character (player-state.md 4.1, 4.2). The update above was
    // its first and advanced the game clock 2,050 game seconds, yet satiety
    // is still 1 (it would be 0.969 otherwise): the first update has dt 0.
    // 10 real seconds standing later (100 game seconds) satiety is 0.0015
    // lower, and paused time counts nothing.
    assert_eq!(state.game_time, 63_471_533_400_000 + 2_050_000);
    let c = state.character.as_ref().unwrap();
    assert_eq!((c.health, c.power, c.satiety, c.money), (1.0, 1.0, 1.0, 40));
    assert_eq!(c.active_slot, Some(4));
    // The level's placed effect (level-placed-effects.md 8): started at
    // the load, on a world clock that stands still while paused.
    let fx = screen.effects_report();
    assert_eq!(fx.records.len(), 1);
    assert_eq!(fx.records[0].name, "tests\\muhi");
    assert!(fx.records[0].playing && !fx.records[0].group);
    let clock = fx.clock;
    screen.update(215_000, &Input::new());
    let satiety = screen.state().character.unwrap().satiety;
    assert!((satiety - (1.0 - 0.0015)).abs() < 1e-5, "{satiety}");
    let fx = screen.effects_report();
    assert!(fx.clock > clock && fx.particles > 0, "{fx:?}");
    screen.paused(300_000);
    screen.update(300_000, &Input::new());
    assert_eq!(screen.state().character.unwrap().satiety, satiety);
    assert_eq!(screen.effects_report().clock, fx.clock);
    for t in 0..120 {
        screen.update(300_016 + t * 16, &Input::new());
    }
    let fx = screen.effects_report();
    assert_eq!(fx.records[0].effects[0].count, 7, "{fx:?}");
}

/// Records the master gains set; plays nothing.
#[derive(Default)]
struct Gains(Vec<f32>);

impl AudioOut for Gains {
    fn play(&mut self, _: &str, _: Playback) -> Option<Voice> {
        None
    }
    fn stop(&mut self, _: Voice) {}
    fn is_playing(&self, _: Voice) -> bool {
        false
    }
    fn set_master_gain(&mut self, gain: f32) {
        self.0.push(gain);
    }
    fn set_volume(&mut self, _: volya_engine::core::audio::Category, _: f32) {}
}

fn press(input: &mut Input, key: Key) {
    input.handle(Event::Key { key, pressed: true });
    input.handle(Event::Key {
        key,
        pressed: false,
    });
}

/// Section 5's texts with section 10.2's width S and left edge x0 at
/// 800 × 600, 1024 × 768 and 1920 × 1080.
type Widths = [(u32, f32); 3];
const CAPTIONS: [(&str, Widths); 17] = [
    (
        "Кэширование объектов...",
        [(154, 323.), (176, 424.), (257, 832.)],
    ),
    ("Сервер: Старт...", [(98, 351.), (113, 456.), (165, 878.)]),
    (
        "Сервер: Загрузка симуляции жизни...",
        [(230, 285.), (265, 380.), (388, 766.)],
    ),
    (
        "Создание новой игры...",
        [(145, 328.), (168, 428.), (241, 840.)],
    ),
    (
        "Сервер: Соединяемся...",
        [(142, 329.), (165, 430.), (239, 841.)],
    ),
    (
        "Клиент: Соединение с localhost...",
        [(208, 296.), (240, 392.), (344, 788.)],
    ),
    (
        "Открытие потока...",
        [(116, 342.), (129, 448.), (194, 863.)],
    ),
    (
        "Загрузка формы объектов...",
        [(175, 313.), (198, 413.), (291, 815.)],
    ),
    (
        "Загрузка шейдеров...",
        [(130, 335.), (148, 438.), (218, 851.)],
    ),
    (
        "Загрузка геометрии...",
        [(134, 333.), (150, 437.), (222, 849.)],
    ),
    (
        "Загрузка базы пространств...",
        [(181, 310.), (200, 412.), (299, 811.)],
    ),
    (
        "Загрузка детальных объектов...",
        [(198, 301.), (223, 401.), (330, 795.)],
    ),
    (
        "Загрузка секторов и порталов...",
        [(202, 299.), (228, 398.), (334, 793.)],
    ),
    (
        "Загрузка ИИ объектов...",
        [(151, 325.), (171, 427.), (249, 836.)],
    ),
    (
        "Клиент: Создание...",
        [(120, 340.), (138, 443.), (200, 860.)],
    ),
    (
        "Загрузка текстур...",
        [(117, 342.), (129, 448.), (195, 863.)],
    ),
    (
        "Клиент: Синхронизация...",
        [(154, 323.), (177, 424.), (258, 831.)],
    ),
];

/// docs/behavior/loading-screen.md: a first New Game from the title menu
/// shows the 17 captions in order with their texts, widths and positions
/// (sections 5, 10.2), the level's picture from caption 5 (4.5), then the
/// loading image over 30 frames of the running level with sound muted
/// (sections 2, 6), the intro starting under the last two of them and
/// playing over the paused level until skipped (section 7). Their drawing
/// is the render-level suite's (`selftest::new_game`).
#[test]
fn new_game_loading_screen_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let mut data = GameData::read(&tree, None, SIZE).unwrap();
    data.measure_intro(&tree).unwrap();
    // The logo sequence is skipped; its lengths do not matter here.
    data.intro_lengths = vec![0.0; data.intro.items.len()];
    let mut app = App::new(data).unwrap();
    let (mut input, mut audio) = (Input::new(), Gains::default());
    let mut t = 0.0;
    let mut step = |app: &mut App, input: &mut Input, audio: &mut Gains| {
        t += 16.0;
        app.update(&tree, t, input, audio);
        input.end_frame();
    };
    for _ in 0..20 {
        if matches!(app.state(), AppState::Menu { .. }) {
            break;
        }
        press(&mut input, Key::Escape);
        step(&mut app, &mut input, &mut audio);
    }
    assert!(matches!(app.state(), AppState::Menu { .. }));
    for _ in 0..2 {
        press(&mut input, Key::Enter);
        step(&mut app, &mut input, &mut audio);
    }
    assert!(matches!(
        app.state(),
        AppState::Loading { loading: None, .. }
    ));

    let resolutions = [
        (Resolution::new(800, 600), 544.0, 17.0),
        (SIZE, 696.0, 19.0),
        (Resolution::new(1920, 1080), 980.0, 30.0),
    ];
    let picture = "gamedata\\textures\\intro\\intro_l01_escape.dds";
    let start = Instant::now();
    for (i, (text, widths)) in CAPTIONS.iter().enumerate() {
        step(&mut app, &mut input, &mut audio);
        let AppState::Loading {
            loading: Some(l), ..
        } = app.state()
        else {
            panic!("caption {}: {:?}", i + 1, app.state())
        };
        assert_eq!((l.captions, l.caption.as_str()), (i as u32 + 1, *text));
        assert_eq!(l.picture.as_deref(), (i >= 4).then_some(picture), "{text}");
        for ((resolution, y, height), (s, x0)) in resolutions.iter().zip(widths) {
            let scene = app.scene(&tree, *resolution).unwrap();
            let b = scene.layer_bounds("loading caption").unwrap();
            assert_eq!(
                b,
                [*x0, *y, x0 + *s as f32, y + height],
                "{text} at {resolution:?}"
            );
        }
        if i == 4 {
            let scene = app.scene(&tree, SIZE).unwrap();
            let bg = scene.first("loading background").unwrap();
            assert_eq!(
                bg.texture,
                TextureId::Image {
                    path: "gamedata\\textures\\ui\\ui_load.dds".into()
                }
            );
            assert_eq!(bg.uv_max, [1.0, 0.75], "1024 × 1024");
        }
    }

    // The last caption's work: the level runs under the loading image.
    step(&mut app, &mut input, &mut audio);
    eprintln!("loading screen: 17 captions in {:.1?}", start.elapsed());
    let AppState::Level {
        level,
        overlay_frames,
        ..
    } = app.state()
    else {
        panic!("not in the level: {:?}", app.state())
    };
    assert_eq!((level.name.as_str(), overlay_frames), ("l01_escape", 30));
    assert_eq!(audio.0, [0.0], "muted");
    let scene = app.scene(&tree, SIZE).unwrap();
    assert!(
        !scene.world.as_ref().unwrap().covered,
        "drawn under the loading image"
    );
    assert_eq!(
        scene.layer_bounds("loading caption").unwrap()[0],
        424.0,
        "the last caption"
    );
    assert!(scene.first("loading picture").is_some());
    let level_state = |app: &App| match app.state() {
        AppState::Level {
            overlay_frames,
            intro,
            ..
        } => (overlay_frames, intro),
        s => panic!("not in the level: {s:?}"),
    };
    for _ in 0..27 {
        step(&mut app, &mut input, &mut audio);
    }
    assert!(matches!(level_state(&app), (3, None)));
    // Section 7: the intro starts once at most 2 overlay frames remain.
    step(&mut app, &mut input, &mut audio);
    let (left, intro) = level_state(&app);
    let intro = intro.expect("the intro started");
    assert_eq!((left, intro.items, intro.kind), (2, 1, Some("video")));
    // intro_stalker: 182.2 s (startup.md section 5).
    let length = intro.length_ms.unwrap();
    assert!((180_000..190_000).contains(&length), "{length} ms");
    assert_eq!(audio.0, [0.0], "still muted");
    step(&mut app, &mut input, &mut audio);
    step(&mut app, &mut input, &mut audio);
    let (left, intro) = level_state(&app);
    assert_eq!((left, intro.is_some()), (0, true));
    assert_eq!(audio.0, [0.0, 1.0], "un-muted after the 30th frame");

    // It plays over the level, game time paused (`pause_state on`).
    let time = |app: &mut App| app.scene(&tree, SIZE).unwrap().world.unwrap().time;
    let paused_at = time(&mut app);
    for _ in 0..100 {
        step(&mut app, &mut input, &mut audio);
    }
    assert_eq!(time(&mut app), paused_at, "game time paused");
    let scene = app.scene(&tree, SIZE).unwrap();
    assert!(
        scene.world.as_ref().unwrap().covered,
        "not drawn under the intro"
    );
    assert!(
        scene
            .sprites
            .iter()
            .all(|s| !s.layer.starts_with("loading"))
    );
    let video = scene.first("video").expect("the intro video");
    assert!(
        matches!(&video.texture, TextureId::Video { path, .. }
            if path == "gamedata\\textures\\intro\\intro_stalker.ogm"),
        "{:?}",
        video.texture
    );
    // Escape skips it (startup.md section 6.5): the level is shown and game
    // time runs again.
    press(&mut input, Key::Escape);
    step(&mut app, &mut input, &mut audio);
    assert!(matches!(level_state(&app), (0, None)));
    step(&mut app, &mut input, &mut audio);
    assert!(time(&mut app) > paused_at);
    let scene = app.scene(&tree, SIZE).unwrap();
    assert!(scene.first("video").is_none());
    assert!(!scene.world.unwrap().covered);

    // The menu during the game (save-load-screens.md section 2): Escape
    // opens it with the in-game list, game time stops, the level is not
    // drawn but stays loaded; Escape on its main page returns.
    press(&mut input, Key::Escape);
    step(&mut app, &mut input, &mut audio);
    let menu = |app: &App| match app.state() {
        AppState::Level { menu, .. } => menu,
        s => panic!("not in the level: {s:?}"),
    };
    let m = menu(&app).expect("the menu opened");
    assert_eq!(m.menu.list, "menu_main_single");
    let items: Vec<&str> = m.menu.items.iter().map(|i| i.name.as_str()).collect();
    // The data's list (title-menu.md 4.1), the last-save item included.
    assert_eq!(
        items,
        [
            "btn_ret",
            "btn_lastsave",
            "btn_load",
            "btn_save",
            "btn_options",
            "btn_credits",
            "btn_quit_to_mm",
            "btn_quit"
        ]
    );
    let game_time = |app: &App| match app.state() {
        AppState::Level { level, .. } => level.game_time,
        s => panic!("{s:?}"),
    };
    let stopped = game_time(&app);
    for _ in 0..10 {
        step(&mut app, &mut input, &mut audio);
    }
    assert_eq!(game_time(&app), stopped, "game time paused");
    let scene = app.scene(&tree, SIZE).unwrap();
    assert!(scene.world.as_ref().unwrap().covered, "the level not drawn");
    assert!(scene.first("background").is_some(), "the menu drawn");
    assert!(app.level_controls().is_none(), "the controls stop");
    press(&mut input, Key::Escape);
    step(&mut app, &mut input, &mut audio);
    assert!(menu(&app).is_none(), "returned to the game");
    step(&mut app, &mut input, &mut audio);
    assert!(game_time(&app) > stopped, "game time runs again");

    // Load (item 2) shows the load screen with no saves; Escape returns to
    // the menu. Quit game (item 6) asks first; "Yes" ends the game.
    press(&mut input, Key::Escape);
    step(&mut app, &mut input, &mut audio);
    for k in [Key::Down, Key::Down, Key::Enter] {
        press(&mut input, k);
        step(&mut app, &mut input, &mut audio);
    }
    let load = menu(&app).unwrap().load_screen.expect("the load screen");
    assert!(load.screen.list.rows.is_empty());
    press(&mut input, Key::Escape);
    step(&mut app, &mut input, &mut audio);
    let m = menu(&app).unwrap();
    assert!(m.load_screen.is_none() && m.menu.selected == 2);
    for k in [Key::Down, Key::Down, Key::Down, Key::Down, Key::Enter] {
        press(&mut input, k);
        step(&mut app, &mut input, &mut audio);
    }
    let message = menu(&app).unwrap().message.expect("the quit box");
    assert_eq!(message.name, "message_box_quit_game");
    let yes = message.buttons[0].rect;
    let at = [yes[0] + yes[2] / 2.0, yes[1] + yes[3] / 2.0];
    input.handle(Event::PointerMoved { at });
    for pressed in [true, false] {
        input.handle(Event::PointerButton {
            at: None,
            button: volya_engine::core::input::MouseButton::Left,
            pressed,
            time: 1.0e9,
        });
    }
    step(&mut app, &mut input, &mut audio);
    let AppState::Menu { menu: m, .. } = app.state() else {
        panic!("not on the title screen: {:?}", app.state())
    };
    assert_eq!(m.menu.list, "menu_main");

    // A later New Game skips prefetching (section 5); the menu does not
    // open under the loading image (our choice).
    for k in [Key::Up, Key::Enter, Key::Enter] {
        press(&mut input, k);
        step(&mut app, &mut input, &mut audio);
    }
    step(&mut app, &mut input, &mut audio);
    let AppState::Loading {
        loading: Some(l), ..
    } = app.state()
    else {
        panic!("not loading: {:?}", app.state())
    };
    assert_eq!((l.captions, l.caption.as_str()), (1, CAPTIONS[1].0));
    for _ in 0..16 {
        step(&mut app, &mut input, &mut audio);
    }
    assert!(matches!(
        app.state(),
        AppState::Level {
            overlay_frames: 30,
            ..
        }
    ));
    press(&mut input, Key::Escape);
    step(&mut app, &mut input, &mut audio);
    assert!(menu(&app).is_none());
    assert_eq!(audio.0, [0.0, 1.0, 0.0], "still muted");
}

/// Each loading stage's work, timed: the loading screen does one stage per
/// frame (loading-screen.md section 5), so each must fit under a frame the
/// player sees. `st_loading_cform` loads the collision world.
#[test]
fn loading_stage_times() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut load = level::NewGameLoad::default();
    for stage in Stage::ALL {
        let start = Instant::now();
        load.run(stage, &tree, &data).unwrap();
        eprintln!("{stage:?}: {:.1?}", start.elapsed());
    }
    let screen = load.finish(0).unwrap();
    let collision = screen.collision();
    assert_eq!(collision.triangle_count(), 1_375_470);
    eprintln!(
        "collision world: {:.1} MB",
        collision.memory_bytes() as f64 / 1e6
    );
}

/// The level's objects take their models from the prefetched ones
/// (console-diagnostics.md 2.4.1) exactly as their files would lay them
/// out: the same models and meshes as a load without the prefetching (a
/// later New Game of the game type given none), each prefetched mesh's
/// vertices and indices in its kept chunk those of its file, the indices
/// counted from the chunk's first vertex; the others' in the level's own
/// chunks (#788) as in that load's, and the rest of the geometry the same.
#[test]
fn prefetched_models_lay_out_as_their_files() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let load = |stages: &[Stage]| {
        let mut load = level::NewGameLoad::default();
        for &stage in stages {
            load.run(stage, &tree, &data).unwrap();
        }
        let prefetched = load.prefetched().cloned();
        let screen = load.finish(0).unwrap();
        let objects = &screen.world().objects;
        let names = &objects.names;
        let models: Vec<_> = (objects.models.iter())
            .map(|m| {
                let meshes: Vec<_> = (m.meshes.iter())
                    .map(|k| {
                        (
                            *k,
                            names.get(k.texture).to_owned(),
                            names.get(k.shader).to_owned(),
                        )
                    })
                    .collect();
                (names.get(m.path).to_owned(), meshes)
            })
            .collect();
        let own = objects.take_chunks();
        assert!(own.iter().all(|c| c.set == objects.set));
        (prefetched, models, objects.take_geometry().unwrap(), own)
    };
    let (prefetched, with, with_geometry, with_own) = load(&Stage::ALL);
    let (none, without, without_geometry, without_own) = load(&Stage::ALL[1..]);
    let p = prefetched.unwrap();
    let chunks = p.take_pending();
    let (count, vertices, indices) = p.geometry();
    eprintln!(
        "{} models prefetched, {} texture files, their geometry {count} chunks, {vertices} bytes of vertices and {indices} of indices; the level's {} models, {} of them prefetched; its own geometry {} chunks",
        p.len(),
        p.textures.len(),
        with.len(),
        with.iter().filter(|(path, _)| p.holds(path)).count(),
        with_own.len()
    );
    assert!(!p.is_empty() && none.is_none());
    assert_eq!(chunks.len(), count as usize);
    assert!(chunks.iter().all(|c| c.set == p.set()));
    assert_eq!(with.len(), without.len());
    const V: usize = volya_engine::render::world::MODEL_VERTEX_BYTES;
    const LEVEL: u32 = volya_engine::render::world::LEVEL_CHUNK;
    let index = |bytes: &[u8], i: usize| u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]);
    // A mesh's vertices and its indices counted from its first vertex,
    // wherever they are.
    let layout = |m: &volya_engine::objects::MeshRange,
                  geometry: &volya_engine::render::world::ObjectGeometry,
                  own: &[volya_engine::objects::GeometryChunk]| {
        let (v, i) = (m.first_vertex as usize, m.first_index as usize);
        let (n, k) = (m.vertex_count as usize, m.index_count as usize);
        match m.kept {
            Some(c) => {
                let c = if c & LEVEL != 0 {
                    &own[(c & !LEVEL) as usize]
                } else {
                    &chunks[c as usize]
                };
                let indices: Vec<u16> = (0..k)
                    .map(|j| index(&c.indices, i + j) - v as u16)
                    .collect();
                (c.vertices[v * V..(v + n) * V].to_vec(), indices)
            }
            None => (
                geometry.vertices[v * V..(v + n) * V].to_vec(),
                geometry.indices[i..i + k].to_vec(),
            ),
        }
    };
    for ((path, meshes), (other, file)) in with.iter().zip(&without) {
        assert_eq!(path, other);
        assert_eq!(meshes.len(), file.len(), "{path}");
        for ((m, texture, shader), (f, file_texture, file_shader)) in meshes.iter().zip(file) {
            let ranges = |m: &volya_engine::objects::MeshRange| {
                (m.vertex_count, m.index_count, m.windows, m.sphere)
            };
            assert_eq!(ranges(m), ranges(f), "{path}");
            assert_eq!((texture, shader), (file_texture, file_shader), "{path}");
            let prefetched = m.kept.is_some_and(|c| c & LEVEL == 0);
            assert_eq!(prefetched, p.holds(path), "{path}");
            assert!(f.kept.is_none_or(|c| c & LEVEL != 0), "{path}");
            assert!(
                layout(m, &with_geometry, &with_own) == layout(f, &without_geometry, &without_own),
                "{path}"
            );
        }
    }
    assert_eq!(
        with_geometry.windows, without_geometry.windows,
        "the detail windows"
    );
}

/// With the platform preparing textures (#113), a New Game's loading waits
/// under `st_loading_textures`, its caption showing, until they are
/// prepared. The world taking them is the render-level suite's
/// (`selftest::new_game`).
#[test]
fn loading_waits_for_prepared_textures() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let mut data = GameData::read(&tree, None, SIZE).unwrap();
    data.intro_lengths = vec![0.0; data.intro.items.len()];
    let mut app = App::new(data).unwrap();
    app.set_prepare_textures(true);
    let (mut input, mut audio) = (Input::new(), Gains::default());
    let mut t = 0.0;
    let mut step = |app: &mut App, input: &mut Input, audio: &mut Gains| {
        t += 16.0;
        app.update(&tree, t, input, audio);
        input.end_frame();
    };
    for _ in 0..20 {
        if matches!(app.state(), AppState::Menu { .. }) {
            break;
        }
        press(&mut input, Key::Escape);
        step(&mut app, &mut input, &mut audio);
    }
    for _ in 0..2 {
        press(&mut input, Key::Enter);
        step(&mut app, &mut input, &mut audio);
    }
    let caption = |app: &App| match app.state() {
        AppState::Loading {
            loading: Some(l), ..
        } => Some((l.captions, l.caption.clone())),
        _ => None,
    };
    let mut frames = 0;
    // First the prefetched models, under the first caption, their geometry
    // handed over a chunk at a time as it is made (the platform's renderer
    // keeps it).
    while !app.models_to_prefetch() {
        step(&mut app, &mut input, &mut audio);
        frames += 1;
        assert!(frames < 40, "never asked to prefetch: {:?}", app.state());
    }
    let prefetching = Some((1, "Кэширование объектов...".to_owned()));
    assert_eq!(caption(&app), prefetching);
    step(&mut app, &mut input, &mut audio);
    assert!(app.models_to_prefetch(), "held");
    let mut chunks = Vec::new();
    loop {
        let before = chunks.len();
        let more = app.prefetch_models(&tree, &mut |c| chunks.push((c.index, c.vertices.len())));
        if !more {
            break;
        }
        assert_eq!(chunks.len(), before + 1, "a chunk a step");
    }
    assert!(chunks.len() > 10, "{} chunks", chunks.len());
    assert!(
        (chunks.iter()).all(|&(_, v)| v <= volya_engine::objects::CHUNK_VERTICES * 16),
        "chunks within 16 bits"
    );
    // Then their textures (texture-quality-filtering.md 2), kept for the
    // game type.
    assert_eq!(caption(&app), prefetching);
    assert!(app.prefetching_textures());
    let prefetched = app.textures_to_prepare().unwrap().len();
    assert!(prefetched > 200, "{prefetched} textures prefetched");
    step(&mut app, &mut input, &mut audio);
    assert_eq!(caption(&app).map(|c| c.0), Some(1), "held");
    app.textures_prepared();
    step(&mut app, &mut input, &mut audio);
    while app.textures_to_prepare().is_none() {
        step(&mut app, &mut input, &mut audio);
        frames += 1;
        assert!(frames < 40, "never asked to prepare: {:?}", app.state());
    }
    assert!(!app.prefetching_textures());
    let waiting = caption(&app);
    assert_eq!(waiting, Some((16, "Загрузка текстур...".to_owned())));
    for _ in 0..3 {
        step(&mut app, &mut input, &mut audio);
        assert_eq!(caption(&app), waiting, "held");
    }
    let paths = app.textures_to_prepare().unwrap().to_vec();
    assert!(paths.iter().any(|p| p.ends_with("_cube.dds")), "the sky");
    assert!(paths.iter().any(|p| p.contains("lmap#")), "light maps");
}

/// A game tree that, while `hidden` is set, lacks the file of
/// `ambient\rain` (sound-rain-thunder.md 4.1), a sound every level creates.
struct WithoutRain<'a> {
    inner: &'a dyn Vfs,
    hidden: Cell<bool>,
}

impl WithoutRain<'_> {
    fn hides(&self, path: &str) -> bool {
        self.hidden.get()
            && vfs::normalize(path)
                .is_ok_and(|p| p.to_ascii_lowercase().ends_with("sounds/ambient/rain.ogg"))
    }
}

impl Vfs for WithoutRain<'_> {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        if self.hides(path) {
            return Err(io::ErrorKind::NotFound.into());
        }
        self.inner.open(path)
    }
    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        self.inner.read_dir(path)
    }
    fn exists(&self, path: &str) -> bool {
        !self.hides(path) && self.inner.exists(path)
    }
}

/// An audio output with the positional sound system, or without one.
#[derive(Default)]
struct Output {
    gains: Gains,
    sounds: Option<SoundSystem>,
}

impl AudioOut for Output {
    fn play(&mut self, path: &str, playback: Playback) -> Option<Voice> {
        self.gains.play(path, playback)
    }
    fn stop(&mut self, voice: Voice) {
        self.gains.stop(voice);
    }
    fn is_playing(&self, voice: Voice) -> bool {
        self.gains.is_playing(voice)
    }
    fn set_master_gain(&mut self, gain: f32) {
        self.gains.set_master_gain(gain);
    }
    fn set_volume(&mut self, category: volya_engine::core::audio::Category, volume: f32) {
        self.gains.set_volume(category, volume);
    }
    fn sounds(&mut self) -> Option<&mut SoundSystem> {
        self.sounds.as_mut()
    }
}

/// The title menu's error once a New Game is chosen on `tree` with
/// `audio` and its level has run a few frames, `None` while it runs.
/// `running` is called at the first frame the level runs.
fn new_game_error(
    tree: &dyn Vfs,
    data: GameData,
    audio: &mut Output,
    running: impl FnOnce(&mut Output),
) -> Option<String> {
    let mut app = App::new(data).unwrap();
    let mut input = Input::new();
    let mut t = 0.0;
    let mut step = |app: &mut App, input: &mut Input, audio: &mut Output| {
        t += 16.0;
        app.update(tree, t, input, audio);
        input.end_frame();
    };
    for _ in 0..20 {
        if matches!(app.state(), AppState::Menu { .. }) {
            break;
        }
        press(&mut input, Key::Escape);
        step(&mut app, &mut input, audio);
    }
    for _ in 0..2 {
        press(&mut input, Key::Enter);
        step(&mut app, &mut input, audio);
    }
    let mut running = Some(running);
    for _ in 0..40 {
        step(&mut app, &mut input, audio);
        match app.state() {
            AppState::Menu { menu, .. } => return menu.error,
            AppState::Level { .. } => {
                if let Some(f) = running.take() {
                    f(audio);
                }
            }
            _ => {}
        }
    }
    assert!(matches!(app.state(), AppState::Level { .. }));
    None
}

/// sound-ogg-params.md section 5: a level sound's file that cannot be
/// opened is fatal at level load, with or without audio output (the
/// level's sounds are created only with it): the game ends and the title
/// menu says why. Gone once the level runs, when the sound system appears
/// and the sounds are created, it is fatal there too (`App::fatal`).
#[test]
fn a_missing_level_sound_file_is_fatal() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let inner = DbVfs::open_game(&root, Some("mods")).unwrap();
    let tree = WithoutRain {
        inner: &inner,
        hidden: Cell::new(false),
    };
    let data = || {
        let mut data = GameData::read(&tree, None, SIZE).unwrap();
        data.intro_lengths = vec![0.0; data.intro.items.len()];
        data
    };
    // The error, and the diagnostic log's end (#356) with its reason and
    // the message, but a fatal error's: the game's log has its report
    // (#646).
    let fatal = |(e, end): (Option<String>, Vec<(String, String)>), reason: &str| {
        let e = e.expect("fatal");
        assert!(e.contains("rain.ogg"), "{e}");
        let message = if reason == "fatal" { String::new() } else { e };
        assert_eq!(end, [(reason.to_owned(), message)]);
    };
    tree.hidden.set(true);
    for sounds in [None, Some(SoundSystem::new(32))] {
        let mut audio = Output {
            sounds,
            ..Default::default()
        };
        let failed = ends(|| new_game_error(&tree, data(), &mut audio, |_| {}));
        fatal(failed, "load_failed");
    }
    // Loaded without audio output, the file present; then the output
    // appears and the file is gone: `App::fatal`.
    tree.hidden.set(false);
    let failed = ends(|| {
        new_game_error(&tree, data(), &mut Output::default(), |audio| {
            tree.hidden.set(true);
            audio.sounds = Some(SoundSystem::new(32));
        })
    });
    fatal(failed, "fatal");
}

/// `f`'s result and the `engine.end` events (reason, message) the
/// diagnostic log records while it runs.
fn ends<T>(f: impl FnOnce() -> T) -> (T, Vec<(String, String)>) {
    use tracing_subscriber::layer::SubscriberExt;
    let d = Diagnostics::new(|| 0.0, "test", 1, "test");
    let subscriber = tracing_subscriber::registry().with(d.layer());
    let out = tracing::subscriber::with_default(subscriber, f);
    let ends = d
        .take_pending()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|e| e["name"] == diagnostics::END)
        .map(|e| {
            let text = |k: &str| e["args"][k].as_str().unwrap_or_default().to_owned();
            (text("reason"), text("message"))
        })
        .collect();
    (out, ends)
}
