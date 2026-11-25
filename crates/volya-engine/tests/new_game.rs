// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game from the Russian edition (BYOA): its loading screen
//! (docs/behavior/loading-screen.md), which level loads
//! (docs/behavior/level-loading.md section 2.2), the start view
//! (docs/behavior/start-position.md) rendered headless, with load and frame
//! times. `VOLYA_DUMP_LEVEL=<file.ppm>` writes the start frame,
//! `VOLYA_DUMP_LOADING=<file.ppm>` the fifth loading image and
//! `VOLYA_DUMP_INTRO=<file.ppm>` an intro frame locally for inspection.
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
use volya_engine::present::Presenter;
use volya_engine::render::{Renderer, TexturePath, wgpu};
use volya_engine::scene::TextureId;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn renderer() -> Renderer {
    renderer_with(wgpu::Features::all())
}

/// A renderer whose device has the renderer's features within `allowed`.
fn renderer_with(allowed: wgpu::Features) -> Renderer {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: volya_engine::render::device_features(&adapter) & allowed,
                ..Default::default()
            })
            .await
            .unwrap();
        Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm)
    })
}

/// Writes `pixels` (RGBA) as a PPM when the variable `var` is set.
fn dump(var: &str, pixels: &[u8], width: u32, height: u32) {
    if let Some(path) = std::env::var_os(var) {
        let mut ppm = format!("P6 {width} {height} 255\n").into_bytes();
        for p in pixels.chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
}

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

    let mut gpu = renderer();
    let mut presenter = Presenter::new();
    let scene = screen.scene(SIZE, 0);
    let start = Instant::now();
    let list = presenter.draw_list(&mut gpu, &tree, &scene, 0);
    let pixels =
        pollster::block_on(gpu.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
            .unwrap();
    let first = start.elapsed();
    let errors = presenter.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    let world = presenter.world().expect("world uploaded").clone();
    assert!(
        screen.world().objects.take_geometry().is_none(),
        "CPU copy of the objects released"
    );

    // Frame time: the same view again, the world already on the GPU.
    let start = Instant::now();
    const FRAMES: u32 = 10;
    for _ in 0..FRAMES {
        let list = presenter.draw_list(&mut gpu, &tree, &scene, 0);
        pollster::block_on(gpu.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
            .unwrap();
    }
    let frame = start.elapsed() / FRAMES;
    eprintln!(
        "level: loaded in {loaded:.1?}, first frame (upload) {first:.1?}, then {frame:.1?} per \
         frame with readback; GPU buffers {:.1} MiB, textures {:.1} MiB; {} draws, {} skipped",
        world.gpu_bytes() as f64 / (1 << 20) as f64,
        world.texture_bytes() as f64 / (1 << 20) as f64,
        world.draw_count(),
        world.skipped(),
    );
    let level = screen.world();
    eprintln!(
        "{} surfaces; not drawn: {:?}",
        level.surfaces.len(),
        level.not_drawn
    );
    assert_eq!(world.skipped(), 0);
    dump("VOLYA_DUMP_LEVEL", &pixels, SIZE.width, SIZE.height);

    // The start view is inside the trader's bunker: the world covers the
    // screen (the clear colour shows nowhere), in many colours.
    let clear = scene.world.as_ref().unwrap().environment.fog_rgb();
    let clear = clear.map(|c| (c * 255.0).round() as u8);
    let total = (SIZE.width * SIZE.height) as usize;
    let background = pixels
        .chunks(4)
        .filter(|p| p[..3].iter().zip(clear).all(|(a, b)| a.abs_diff(b) <= 1))
        .count();
    let distinct: std::collections::BTreeSet<&[u8]> = pixels.chunks(4).collect();
    eprintln!(
        "background {:.1} %, {} colours",
        100.0 * background as f64 / total as f64,
        distinct.len()
    );
    assert!(background * 100 < total, "world everywhere");
    assert!(distinct.len() > 500, "{} colours", distinct.len());

    // Outdoors, above the start (`VOLYA_OUTDOOR=x,y,z,yaw,pitch` picks
    // another view): terrain, trees, fog and the sky.
    let view: Vec<f32> = std::env::var("VOLYA_OUTDOOR")
        .unwrap_or_else(|_| "-180,5,-120,1.2,0.15".into())
        .split(',')
        .map(|v| v.trim().parse().unwrap())
        .collect();
    screen.set_camera(
        [view[0], view[1], view[2]],
        volya_engine::core::camera::Angles {
            yaw: view[3],
            pitch: view[4],
            roll: 0.0,
        },
    );
    let scene = screen.scene(SIZE, 5000);
    let list = presenter.draw_list(&mut gpu, &tree, &scene, 5000);
    let pixels =
        pollster::block_on(gpu.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
            .unwrap();
    if let Some(path) = std::env::var_os("VOLYA_DUMP_OUTDOOR") {
        let mut ppm = format!("P6 {} {} 255\n", SIZE.width, SIZE.height).into_bytes();
        for p in pixels.chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
    let distinct: std::collections::BTreeSet<&[u8]> = pixels.chunks(4).collect();
    assert!(distinct.len() > 500, "outdoors: {} colours", distinct.len());
    // The sky (docs/behavior/sky.md) covers the top rows: varied, and not
    // the clear colour.
    let top = &pixels[..(SIZE.width * 16 * 4) as usize];
    let cleared = top
        .chunks(4)
        .filter(|p| p[..3].iter().zip(clear).all(|(a, b)| a.abs_diff(b) <= 1))
        .count();
    let sky: std::collections::BTreeSet<&[u8]> = top.chunks(4).collect();
    assert!(
        cleared * 100 < top.len() / 4,
        "{cleared} cleared sky pixels"
    );
    assert!(sky.len() > 50, "{} sky colours", sky.len());

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
    screen.set_camera(
        [view[0], view[1], view[2]],
        volya_engine::core::camera::Angles {
            yaw: view[3],
            pitch: view[4],
            roll: 0.0,
        },
    );
    let mut scene = screen.scene(SIZE, 205_000);
    let mut top_rows = |scene: &volya_engine::scene::Scene| {
        let list = presenter.draw_list(&mut gpu, &tree, scene, 205_000);
        let pixels =
            pollster::block_on(gpu.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
                .unwrap();
        pixels[..(SIZE.width * 16 * 4) as usize].to_vec()
    };
    let at_six = top_rows(&scene);
    // The frame binds the cube maps of [default_weather_06] and _07 (index
    // 6 of the cycle in time order, from 00:00) ...
    let sky = scene.world.as_mut().unwrap().sky.as_mut().unwrap();
    assert_eq!(sky.section, 6);
    // ... and the same frame with the 05:30 pair bound instead (sky colour,
    // blend weight and view unchanged) shows another sky, so the drawn sky
    // comes from the next hour's cube maps.
    sky.section = 5;
    let with_five = top_rows(&scene);
    let differing = at_six
        .chunks(4)
        .zip(with_five.chunks(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
        .count();
    eprintln!(
        "06:04: {differing} of {} sky pixels differ",
        at_six.len() / 4
    );
    assert!(
        differing * 2 > at_six.len() / 4,
        "06:04: {differing} of {} sky pixels differ from the 05:00 pair's",
        at_six.len() / 4
    );
    assert!(presenter.take_errors().is_empty());

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
/// playing over the paused level until skipped (section 7). Its textures
/// and the intro's video render without errors.
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
    let mut gpu = renderer();
    let mut presenter = Presenter::new();
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
            let list = presenter.draw_list(&mut gpu, &tree, &scene, 0);
            let pixels =
                pollster::block_on(gpu.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
                    .unwrap();
            dump("VOLYA_DUMP_LOADING", &pixels, SIZE.width, SIZE.height);
            let errors = presenter.take_errors();
            assert!(errors.is_empty(), "{errors:?}");
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
    let list = presenter.draw_list(&mut gpu, &tree, &scene, app.clock());
    let pixels =
        pollster::block_on(gpu.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
            .unwrap();
    let errors = presenter.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    let distinct: std::collections::BTreeSet<&[u8]> = pixels.chunks(4).collect();
    assert!(
        distinct.len() > 100,
        "video frame: {} colours",
        distinct.len()
    );
    dump("VOLYA_DUMP_INTRO", &pixels, SIZE.width, SIZE.height);
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

/// The level's start view and an outdoor view drawn as on WebGL 2 (#113):
/// textures transcoded to ETC2 (where the GPU has no BC) or decoded to
/// RGBA8 (where it has neither), per-draw data from a texture, indices
/// rebased; each close to the WebGPU path's picture. Reports each path's
/// texture memory and the transcoding time.
#[test]
fn webgl_paths_match_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let views = |r: &mut Renderer| {
        let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();
        let mut presenter = Presenter::new();
        let start = Instant::now();
        let mut pictures = Vec::new();
        let mut first = None;
        for (eye, yaw, pitch) in [(None, 0.0, 0.0), (Some([-180.0, 5.0, -120.0]), 1.2, 0.15)] {
            if let Some(eye) = eye {
                screen.set_camera(
                    eye,
                    volya_engine::core::camera::Angles {
                        yaw,
                        pitch,
                        roll: 0.0,
                    },
                );
            }
            let scene = screen.scene(SIZE, 0);
            let list = presenter.draw_list(r, &tree, &scene, 0);
            pictures.push(
                pollster::block_on(r.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
                    .unwrap(),
            );
            first.get_or_insert(start.elapsed());
        }
        let errors = presenter.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
        let world = presenter.world().unwrap();
        (pictures, world.texture_bytes(), first.unwrap())
    };
    let psnr = |a: &[u8], b: &[u8]| {
        let se: f64 = a
            .chunks(4)
            .zip(b.chunks(4))
            .flat_map(|(x, y)| (0..3).map(move |c| (f64::from(x[c]) - f64::from(y[c])).powi(2)))
            .sum();
        10.0 * (255.0f64.powi(2) / (se / (a.len() / 4 * 3) as f64)).log10()
    };
    let mut webgpu = renderer();
    assert_eq!(webgpu.texture_path(), TexturePath::Bc);
    let (reference, bc_bytes, bc_first) = views(&mut webgpu);
    let mib = |b: u64| b as f64 / (1 << 20) as f64;
    eprintln!(
        "BC, storage buffer: textures {:.1} MiB, first frame {bc_first:.1?}",
        mib(bc_bytes)
    );
    for (allowed, path) in [
        (wgpu::Features::TEXTURE_COMPRESSION_ETC2, TexturePath::Etc2),
        (wgpu::Features::empty(), TexturePath::Rgba8),
    ] {
        let mut r = renderer_with(allowed);
        if r.texture_path() != path {
            eprintln!("skipped {path:?}: the GPU lacks it");
            continue;
        }
        r.set_compat(true);
        let (pictures, bytes, first) = views(&mut r);
        let quality: Vec<f64> = pictures
            .iter()
            .zip(&reference)
            .map(|(a, b)| psnr(a, b))
            .collect();
        eprintln!(
            "{path:?}, draws texture, rebased indices: textures {:.1} MiB, first frame (with \
             transcoding) {first:.1?}, PSNR against BC {quality:.1?} dB",
            mib(bytes)
        );
        for q in quality {
            assert!(q > 30.0, "{path:?}: {q:.1} dB");
        }
        match path {
            TexturePath::Etc2 => assert_eq!(bytes, bc_bytes, "ETC2 as large as BC"),
            _ => assert!(bytes > 4 * bc_bytes, "RGBA8 at least four times BC"),
        }
    }
}

/// With the platform preparing textures (#113), a New Game's loading waits
/// under `st_loading_textures`, its caption showing, until they are
/// prepared; the world then takes the prepared textures.
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
    while app.textures_to_prepare().is_none() {
        step(&mut app, &mut input, &mut audio);
        frames += 1;
        assert!(frames < 40, "never asked to prepare: {:?}", app.state());
    }
    let waiting = caption(&app);
    assert_eq!(waiting, Some((16, "Загрузка текстур...".to_owned())));
    for _ in 0..3 {
        step(&mut app, &mut input, &mut audio);
        assert_eq!(caption(&app), waiting, "held");
    }
    let paths = app.textures_to_prepare().unwrap().to_vec();
    assert!(paths.iter().any(|p| p.ends_with("_cube.dds")), "the sky");
    assert!(paths.iter().any(|p| p.contains("lmap#")), "light maps");
    let mut gpu = renderer();
    for p in &paths {
        let bytes = volya_engine::core::vfs::read(&tree, p).unwrap();
        let texture = gpu
            .create_texture(&volya_engine_formats::dds::Dds::parse(&bytes).unwrap())
            .unwrap();
        gpu.add_prepared(p, texture);
    }
    eprintln!("{} textures prepared", paths.len());
    app.textures_prepared();
    step(&mut app, &mut input, &mut audio);
    assert_eq!(caption(&app).map(|c| c.0), Some(17), "went on");
    // Then held under the last caption while the world is uploaded in
    // pieces (#184), its clocks not running meanwhile.
    step(&mut app, &mut input, &mut audio);
    let level = app
        .world_to_prepare()
        .expect("the world to prepare")
        .clone();
    let mut presenter = Presenter::new();
    let mut turns = 1;
    let start = Instant::now();
    while !presenter
        .prepare_world(&mut gpu, &tree, &level, 4 << 20)
        .unwrap()
    {
        turns += 1;
        step(&mut app, &mut input, &mut audio);
        assert_eq!(caption(&app).map(|c| c.0), Some(17), "held");
    }
    eprintln!("world prepared in {turns} turns, {:.1?}", start.elapsed());
    assert!(turns > 5, "{turns} turns");
    app.world_prepared();
    step(&mut app, &mut input, &mut audio);
    assert!(matches!(app.state(), AppState::Level { .. }));
    let scene = app.scene(&tree, SIZE).unwrap();
    presenter.draw_list(&mut gpu, &tree, &scene, 0);
    let errors = presenter.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        paths.iter().all(|p| gpu.take_prepared(p).is_none()),
        "taken or dropped"
    );
    assert!(presenter.world().is_some());
}

/// The first level's placed effect (`tests\muhi`, level-placed-effects.md
/// 8) drawn (docs/behavior/particle-drawing.md), on the WebGPU path and the
/// WebGL 2 one: a frame with the effects differs from one without them, and
/// only around where the effect's particles project. `VOLYA_DUMP_EFFECTS`
/// (and `_OFF`) write the frames locally.
#[test]
fn placed_effects_are_drawn_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    for compat in [false, true] {
        let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();
        let fx = screen.effects_report();
        let at = fx.records[0].effects[0].position;
        // 0.6 m in front of the effect, looking at it along +z.
        let eye = [at[0], at[1] + 0.1, at[2] - 0.6];
        let angles = volya_engine::core::camera::Angles {
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
        };
        // Two seconds: its 7 leaves born and drifting.
        for t in 0..120u64 {
            screen.set_camera(eye, angles);
            screen.update(16 * (t + 1), &Input::new());
        }
        screen.set_camera(eye, angles);
        // The placed effect alone: the zones' effects are zones.rs's.
        screen.set_effect_origins_drawn(&[volya_engine::core::particles::placed::Origin::Placed]);
        let mut r = renderer();
        r.set_compat(compat);
        let mut presenter = Presenter::new();
        let mut capture = |screen: &LevelScreen| {
            let scene = screen.scene(SIZE, 2000);
            let list = presenter.draw_list(&mut r, &tree, &scene, 2000);
            let pixels =
                pollster::block_on(r.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
                    .unwrap();
            (pixels, scene)
        };
        let (on, scene) = capture(&screen);
        let draws = &scene.world.as_ref().unwrap().particles;
        assert_eq!(draws.len(), 1, "{draws:?}");
        assert_eq!(draws[0].particles, 7);
        screen.set_effects_drawn(false);
        let (off, _) = capture(&screen);
        assert!(presenter.take_errors().is_empty());
        dump("VOLYA_DUMP_EFFECTS", &on, SIZE.width, SIZE.height);
        dump("VOLYA_DUMP_EFFECTS_OFF", &off, SIZE.width, SIZE.height);
        // Where the particles project: each quad's centre on screen.
        let (view, projection) = screen.view_projection(SIZE.width, SIZE.height);
        let vp = volya_engine::render::world::multiply(&view, &projection);
        let centres: Vec<[f32; 2]> = draws[0]
            .vertices
            .chunks(4)
            .filter_map(|q| {
                let p: Vec<f32> = (0..3)
                    .map(|i| q.iter().map(|v| v.position[i]).sum::<f32>() / 4.0)
                    .collect();
                let clip: Vec<f32> = (0..4)
                    .map(|c| p[0] * vp[0][c] + p[1] * vp[1][c] + p[2] * vp[2][c] + vp[3][c])
                    .collect();
                (clip[3] > 0.01).then(|| {
                    [
                        (clip[0] / clip[3] * 0.5 + 0.5) * SIZE.width as f32,
                        (0.5 - clip[1] / clip[3] * 0.5) * SIZE.height as f32,
                    ]
                })
            })
            .collect();
        let (mut inside, mut outside) = (0, 0);
        for (i, (a, b)) in on.chunks(4).zip(off.chunks(4)).enumerate() {
            if a[..3] != b[..3] {
                let (x, y) = (
                    (i as u32 % SIZE.width) as f32,
                    (i as u32 / SIZE.width) as f32,
                );
                // Within 30 px of a leaf's centre (a leaf is 1 cm across).
                if centres.iter().any(|c| (c[0] - x).hypot(c[1] - y) <= 30.0) {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
        eprintln!(
            "compat {compat}: {inside} pixels differ around the effect's {} leaves on screen, {outside} elsewhere",
            centres.len()
        );
        assert!(inside > 50, "compat {compat}: {inside} pixels differ");
        assert_eq!(outside, 0, "compat {compat}");
    }
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
    // The error, and the diagnostic log's end (#356) with its reason.
    let fatal = |(e, end): (Option<String>, Vec<(String, String)>), reason: &str| {
        let e = e.expect("fatal");
        assert!(e.contains("rain.ogg"), "{e}");
        assert_eq!(end, [(reason.to_owned(), e)]);
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
