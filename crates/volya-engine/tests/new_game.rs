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

use std::time::Instant;

use volya_engine::app::{App, AppState};
use volya_engine::audio::{AudioOut, Voice};
use volya_engine::core::audio::Playback;
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::{self, LevelScreen};
use volya_engine::present::Presenter;
use volya_engine::render::{Renderer, wgpu};
use volya_engine::scene::TextureId;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn renderer() -> Renderer {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: volya_engine::render::device_features(&adapter),
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
        screen.world().take_geometry().is_none(),
        "CPU copy released"
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

    // A later New Game skips prefetching (section 5); leaving the level
    // during its overlay frames un-mutes at once.
    for k in [Key::Escape, Key::Enter, Key::Enter] {
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
    assert!(matches!(app.state(), AppState::Menu { .. }));
    assert_eq!(audio.0, [0.0, 1.0, 0.0, 1.0]);
}

/// The home screen's cover: the loading screen's background, decoded as
/// loading-screen.md 8.1 measures it (mean RGB ≈ 31.7, 27.3, 21.4).
#[test]
fn cover_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let aliases = volya_engine::data::read_aliases(None).unwrap();
    let image = volya_engine::cover::read(&tree, &aliases).unwrap();
    assert_eq!((image.width, image.height), (512, 384));
    let n = (image.width * image.height) as f64;
    let mean = |c: usize| {
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| f64::from(p[c]))
            .sum::<f64>()
            / n
    };
    for (c, want) in [31.7, 27.3, 21.4].into_iter().enumerate() {
        assert!((mean(c) - want).abs() < 0.6, "channel {c}: {}", mean(c));
    }
    assert!(
        image.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255),
        "background is opaque"
    );
    dump("VOLYA_DUMP_COVER", &image.rgba, image.width, image.height);
}
