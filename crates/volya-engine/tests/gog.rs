// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The startup sequence and the title menu built from the Russian edition
//! (BYOA), checked against docs/behavior/startup.md,
//! docs/behavior/title-menu.md and docs/formats/text-and-fonts.md section 5:
//! the scenes' geometry and textures, and the videos' coded frames. Their
//! drawing is the render-screens suite's (`selftest::title_menu`). Skips
//! when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::{self, Cursor};
use std::time::Instant;

use volya_engine::app::{self, App, AppState};
use volya_engine::audio::Silent;
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{DirEntry, StdVfs, Vfs, VfsFile};
use volya_engine::data::GameData;
use volya_engine::scene::{Scene, TextureId};
use volya_engine_formats::ui::Resolution;
use volya_engine_formats::video::TheoraVideo;

const XGA: Resolution = Resolution::new(1024, 768);
const SVGA: Resolution = Resolution::new(800, 600);
const HD720: Resolution = Resolution::new(1280, 720);
const SXGA: Resolution = Resolution::new(1280, 1024);
const HD: Resolution = Resolution::new(1920, 1080);

/// A VFS recording the paths opened.
struct Recording<'a> {
    inner: &'a dyn Vfs,
    opened: RefCell<BTreeSet<String>>,
}

impl Vfs for Recording<'_> {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        let file = self.inner.open(path)?;
        self.opened.borrow_mut().insert(path.to_ascii_lowercase());
        Ok(file)
    }
    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        self.inner.read_dir(path)
    }
}

/// `bin/XR_3DA.exe` from the installer, in a game-root VFS of its own.
fn game_root() -> StdVfs {
    let exe = gog_archives::game_files("splash", |p| p == app::SPLASH_EXE).unwrap();
    let file = &exe[0].file;
    StdVfs::new(
        file.ancestors()
            .nth(app::SPLASH_EXE.split('/').count())
            .unwrap(),
    )
}

fn key(input: &mut Input, key: Key) {
    input.handle(Event::Key { key, pressed: true });
    input.handle(Event::Key {
        key,
        pressed: false,
    });
}

fn step(app: &mut App, input: &mut Input, t: f64) {
    // The startup sequence and the menu read nothing in updates.
    let empty = DbVfs::<std::io::Empty>::new();
    app.update(&empty, t, input, &mut Silent);
    input.end_frame();
}

fn close(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
}

/// The glyph box of a caption: left/right of the text, top/bottom of the
/// cells, as text-and-fonts.md section 5 tabulates them.
fn glyph_box(scene: &Scene, layer: &str) -> [f32; 4] {
    scene
        .layer_bounds(layer)
        .unwrap_or_else(|| panic!("no {layer}"))
}

#[test]
fn startup_and_title_menu_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let cache = archives[0].file.parent().unwrap();
    let root = StdVfs::new(cache);
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let recording = Recording {
        inner: &tree,
        opened: RefCell::default(),
    };

    // Splash (startup.md section 3).
    let exe_root = game_root();
    let splash = app::splash(&exe_root).unwrap().unwrap();
    assert_eq!((splash.width, splash.height), (500, 268));
    let s = app::splash_scene(XGA, (500, 268));
    assert_eq!(s.sprites[0].bounds(), [261.5, 249.5, 761.5, 517.5]);

    let start = Instant::now();
    let data = GameData::load(&recording, None, XGA).unwrap();
    eprintln!("loaded the game data in {:.1?}", start.elapsed());
    // Section 6.3: delay + left sound.
    let lengths: Vec<f64> = data
        .intro_lengths
        .iter()
        .map(|l| (l * 1000.0).round() / 1000.0)
        .collect();
    assert_eq!(lengths, [6.833, 25.954, 13.839, 9.0, 5.0]);
    assert_eq!(data.quit_keys, [Key::Escape]);

    let mut app = App::new(data).unwrap();
    let mut input = Input::new();

    step(&mut app, &mut input, 1000.0);
    let AppState::Startup { startup, .. } = app.state() else {
        panic!("not in the startup")
    };
    assert_eq!((startup.item, startup.length_ms), (0, Some(7833)));
    // 1 s of black: only the background.
    let s = app.scene(&recording, XGA).unwrap();
    assert_eq!(s.sprites.len(), 1);
    assert_eq!(s.sprites[0].layer, "background");
    assert_eq!(
        s.sprites[0].texture,
        TextureId::Image {
            path: "gamedata\\textures\\intro\\intro_back.dds".into()
        }
    );
    // THQ video: 720 × 484 → 1024 × 688.4, centred.
    step(&mut app, &mut input, 3500.0);
    let s = app.scene(&recording, XGA).unwrap();
    let video = s.layer("video").next().unwrap();
    let height = 1024.0 * 484.0 / 720.0;
    assert!(
        close(video.bounds(), [-0.5, 38.5, 1023.5, 39.0 + height - 0.5]),
        "{:?}",
        video.bounds()
    );
    assert!(
        matches!(&video.texture, TextureId::Video { path, start_ms: 1000, looping: false }
        if path == "gamedata\\textures\\intro\\thq_logo_hyuv.ogm")
    );
    let wide = app.scene(&recording, HD).unwrap();
    let v = wide.layer("video").next().unwrap().bounds();
    // 688.4 × 1.328 = 914.2 units: taller than the screen.
    assert!(
        (v[3] - v[1] - height * 1.328 * 1.40625).abs() < 0.01,
        "{v:?}"
    );

    // Esc skips one item per press; the GSC WP item is current.
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 3600.0);
    let AppState::Startup { startup, .. } = app.state() else {
        panic!()
    };
    assert_eq!((startup.item, startup.skipped), (1, 1));
    // Other keys do nothing.
    key(&mut input, Key::Enter);
    step(&mut app, &mut input, 3700.0);
    let AppState::Startup { startup, .. } = app.state() else {
        panic!()
    };
    assert_eq!(startup.item, 1);
    // Skip to the ASUS item; 2 s in, `ui_asus_01` at full opacity.
    key(&mut input, Key::Escape);
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 4000.0);
    step(&mut app, &mut input, 6000.0);
    let s = app.scene(&recording, XGA).unwrap();
    let asus = s.layer("picture 0").next().unwrap();
    assert_eq!(asus.color.a, 255);
    assert!(close(asus.bounds(), [-0.5, -0.5, 1023.5, 767.5]));
    assert_eq!((asus.uv_min, asus.uv_max), ([0.0, 0.0], [0.5, 1.0]));
    assert!(s.layer("picture 1").next().is_none());
    // The NVIDIA item ends after 5 s on its own; then the menu.
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 7000.0);
    step(&mut app, &mut input, 12_000.0);
    let AppState::Menu { menu, .. } = app.state() else {
        panic!("not in the menu")
    };
    assert_eq!(menu.menu.list, "menu_main");
    let captions: Vec<&str> = menu.menu.items.iter().map(|i| i.caption.as_str()).collect();
    assert_eq!(
        captions,
        [
            "Новая игра",
            "Загрузить игру",
            "Сетевая игра",
            "Опции",
            "Титры",
            "Выйти в Windows"
        ]
    );
    assert_eq!(menu.menu.version, "ver. 1.0006");
    assert_eq!(menu.menu.music.track.as_deref(), Some("music\\wasteland2"));
    // The opening move: 22 units up in 556 ms.
    step(&mut app, &mut input, 12_600.0);
    let AppState::Menu { menu, .. } = app.state() else {
        panic!()
    };
    assert_eq!((menu.menu.bar_y, menu.menu.moving), (-22.0, false));
    assert_eq!(menu.menu.glass, [675.0, 419.0, 232.0, 36.0]);

    // Layers at 1024 × 768 (title-menu.md section 3, ui-layout.md 5.3).
    let s = app.scene(&recording, XGA).unwrap();
    let layers: Vec<&str> = s.sprites.iter().map(|s| s.layer.as_str()).collect();
    let order = [
        "back_movie",
        "background",
        "fire_movie",
        "caption 0",
        "bar",
        "left_grating",
        "right_grating",
        "left_anim",
        "right_anim",
        "version",
        "cursor",
    ];
    let firsts: Vec<usize> = order
        .iter()
        .map(|l| layers.iter().position(|x| x == l).unwrap())
        .collect();
    assert!(firsts.windows(2).all(|w| w[0] < w[1]), "{layers:?}");
    let one = |l: &str| s.first(l).unwrap();
    assert_eq!(one("back_movie").bounds(), [147.5, -0.5, 1023.5, 399.5]);
    assert!(close(
        [
            one("back_movie").uv_max[0],
            one("back_movie").uv_max[1],
            0.0,
            0.0
        ],
        [0.855_468_8, 0.78125, 0.0, 0.0]
    ));
    assert!(
        matches!(&one("back_movie").texture, TextureId::Video { path, looping: true, .. } if path == "gamedata\\textures\\ui\\ui_mm_main_back_crop.ogm")
    );
    assert_eq!(one("background").bounds(), [-0.5, -0.5, 1023.5, 767.5]);
    assert_eq!(
        (one("background").uv_min, one("background").uv_max),
        ([0.0, 0.0], [1.0, 0.75])
    );
    assert_eq!(
        one("background").texture,
        TextureId::Image {
            path: "gamedata\\textures\\ui\\ui_mainmenu.dds".into()
        }
    );
    assert_eq!(one("fire_movie").bounds(), [524.5, 458.5, 652.5, 714.5]);
    // The bar at rest on item 0: (599, 397), 392 × 73.
    assert_eq!(one("bar").bounds(), [598.5, 396.5, 990.5, 469.5]);
    assert_eq!(one("left_grating").bounds(), [598.5, 400.5, 644.5, 470.5]);
    // The wheels: 55 × 55 centred at bar + (43, 37) and (340, 37).
    let l = one("left_anim").bounds();
    assert!(
        (l[0] + l[2] - 2.0 * 642.0).abs() < 0.01 && (l[1] + l[3] - 2.0 * 434.0).abs() < 0.01,
        "{l:?}"
    );
    // Text-and-fonts section 5 glyph boxes.
    assert_eq!(glyph_box(&s, "caption 0"), [740.0, 423.0, 825.0, 445.0]);
    assert_eq!(glyph_box(&s, "caption 5"), [713.0, 573.0, 851.0, 595.0]);
    assert_eq!(glyph_box(&s, "version"), [15.0, 740.0, 94.0, 759.0]);
    assert!(
        matches!(&one("caption 0").texture, TextureId::Image { path } if path == "gamedata\\textures\\ui\\ui_font_graff_22_1024.dds")
    );
    assert_eq!(
        [one("caption 0").color.a, one("caption 0").color.r],
        [255, 0]
    );
    let v = one("version").color;
    assert_eq!([v.a, v.r, v.g, v.b], [255, 135, 123, 116]);
    assert!(
        matches!(&one("cursor").texture, TextureId::Image { path } if path.starts_with("gamedata\\textures\\ui\\ui_ani_cursor_0"))
    );
    for (r, item0, item5, version) in [
        (
            SVGA,
            [570.0, 328.0, 651.0, 349.0],
            [546.0, 445.0, 675.0, 466.0],
            [11.0, 578.0, 77.0, 595.0],
        ),
        (
            HD720,
            [935.0, 395.0, 1020.0, 417.0],
            [908.0, 536.0, 1046.0, 558.0],
            [18.0, 693.0, 97.0, 712.0],
        ),
        (
            SXGA,
            [916.0, 562.0, 1038.0, 594.0],
            [880.0, 762.0, 1075.0, 794.0],
            [18.0, 986.0, 133.0, 1016.0],
        ),
    ] {
        let s = app.scene(&recording, r).unwrap();
        assert_eq!(glyph_box(&s, "caption 0"), item0, "{r:?}");
        assert_eq!(glyph_box(&s, "caption 5"), item5, "{r:?}");
        assert_eq!(glyph_box(&s, "version"), version, "{r:?}");
    }

    // Down: item 1, a 30-unit move of 609 ms.
    key(&mut input, Key::Down);
    step(&mut app, &mut input, 13_000.0);
    step(&mut app, &mut input, 13_300.0);
    let AppState::Menu { menu, .. } = app.state() else {
        panic!()
    };
    assert_eq!(
        (menu.menu.selected, menu.menu.bar_target, menu.menu.moving),
        (1, 8.0, true)
    );
    assert!(menu.menu.wheel_angle != 0.0);
    step(&mut app, &mut input, 13_700.0);

    // Mouse: hover row 4, click in the glass.
    input.set_screen([1024.0, 768.0]);
    input.handle(Event::PointerMoved {
        at: [782.0, 419.0 + 30.0 * 4.0 + 15.0],
    });
    step(&mut app, &mut input, 14_000.0);
    step(&mut app, &mut input, 15_000.0);
    input.handle(Event::PointerButton {
        at: None,
        button: volya_engine::core::input::MouseButton::Left,
        pressed: true,
        time: 15_000.0,
    });
    step(&mut app, &mut input, 15_016.0);
    let AppState::Menu { menu, .. } = app.state() else {
        panic!()
    };
    assert_eq!(menu.menu.selected, 4);
    assert_eq!(menu.menu.activations.last().unwrap().item, "btn_credits");

    let mounted = volya_engine::core::vfs::db::game_archives(&root, Some("mods")).unwrap();
    let archives_used: BTreeSet<String> = recording
        .opened
        .borrow()
        .iter()
        .filter_map(|p| tree.archive_of(p).map(|i| mounted[i].path.clone()))
        .collect();
    eprintln!("archives used: {archives_used:?}");
}

/// Which coded rows and columns of each startup and title-menu video become
/// its texture (startup.md 6.6, table), and the GSC data evidence: its coded
/// rows 360–367 are zero in every frame and no shown texel is (#35). The
/// textures' sizes and the GSC item drawn without their green are the
/// render-screens suite's.
#[test]
fn video_frames_as_textures_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();

    // The GSC item: skip THQ and GSC WP, then the item's 1 s delay.
    let mut app = App::new(GameData::load(&tree, None, XGA).unwrap()).unwrap();
    let mut input = Input::new();
    step(&mut app, &mut input, 1000.0);
    key(&mut input, Key::Escape);
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 1100.0);
    let AppState::Startup { startup, .. } = app.state() else {
        panic!()
    };
    assert_eq!(startup.item, 2);
    for t in (2150..2400).step_by(20) {
        step(&mut app, &mut input, f64::from(t));
        let s = app.scene(&tree, XGA).unwrap();
        let video = s.layer("video").next().unwrap();
        assert!(matches!(&video.texture, TextureId::Video { path, .. }
            if path == "gamedata\\textures\\intro\\intro_gsc.ogm"));
        // Texture rectangle (0, 1, 800 × 358) of 1024 × 512.
        let uv = [video.uv_min, video.uv_max].concat();
        assert!(close(
            uv.try_into().unwrap(),
            [0.0, 1.0 / 512.0, 800.0 / 1024.0, 359.0 / 512.0]
        ));
    }

    // Video, coded frame, picture, header offset (y from the bottom),
    // shown rectangle (x, y, w, h).
    #[rustfmt::skip]
    let table = [
        ("intro\\thq_logo_hyuv", (720, 496), (720, 486), (0, 10), [0, 1, 720, 484]),
        ("intro\\intro_gsc-wp", (640, 288), (640, 288), (0, 0), [1, 1, 638, 286]),
        ("intro\\intro_gsc", (800, 368), (800, 360), (0, 0), [0, 1, 800, 358]),
        ("ui\\ui_mm_main_back_crop", (880, 400), (876, 400), (0, 0), [0, 0, 876, 400]),
        ("ui\\ui_mm_fire", (128, 256), (128, 256), (0, 0), [0, 0, 128, 256]),
    ];
    for (name, coded, picture, offset, shown) in table {
        let path = format!("gamedata\\textures\\{name}.ogm");
        let bytes = volya_engine::core::vfs::read(&tree, &path).unwrap();
        let mut video = TheoraVideo::open(Cursor::new(bytes)).unwrap();
        let i = video.info().clone();
        assert_eq!((i.frame_width, i.frame_height), coded, "{name}");
        assert_eq!((i.width, i.height), picture, "{name}");
        let from_bottom = i.frame_height - i.height - i.pic_y;
        assert_eq!((i.pic_x, from_bottom), offset, "{name}");
        let gsc = name == "intro\\intro_gsc";
        let mut frames = 0;
        while let Some(frame) = video.next_frame().unwrap() {
            frames += 1;
            let [y, cb, cr] = frame.texture_planes();
            let [cy, ccb, _] = frame.coded;
            // Texel (x, y) is coded column x, row y.
            assert_eq!((y.width as u32, y.height as u32), picture, "{name}");
            assert_eq!(y.data.as_ptr(), cy.data.as_ptr(), "{name}: luma origin");
            assert_eq!(cb.data.as_ptr(), ccb.data.as_ptr(), "{name}: chroma origin");
            if !gsc {
                continue;
            }
            for r in 360..368 {
                assert!(
                    cy.row(r).iter().all(|&v| v == 0),
                    "GSC frame {frames}: coded row {r}"
                );
            }
            let [x0, y0, w, h] = shown;
            for row in y0..y0 + h {
                for x in x0..x0 + w {
                    let (l, b, r) = (
                        y.row(row)[x],
                        cb.row(row / 2)[x / 2],
                        cr.row(row / 2)[x / 2],
                    );
                    assert!(
                        l > 4 || b > 4 || r > 4,
                        "GSC frame {frames}: texel ({x}, {row}) is ({l}, {b}, {r})"
                    );
                }
            }
        }
        if gsc {
            assert_eq!(frames, 301);
        }
    }
}
