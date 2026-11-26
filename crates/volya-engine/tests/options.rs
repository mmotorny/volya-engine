// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The options screen built from the Russian edition (BYOA), checked
//! against docs/behavior/options-screens.md: its layout, strings and
//! textures resolve, the controls sit where the spec puts them, the
//! shipped user configuration file round-trips through the console, and a
//! few controls work on the real data. Its drawing is the render-screens
//! suite's (`selftest::options`). Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::fs::File;
use std::io::BufReader;

use volya_engine::core::console::{Console, action_index};
use volya_engine::core::input::{Event, GamepadButton, Input, Key, MouseButton};
use volya_engine::core::ui;
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::{Assets, DEFAULT_USER_LTX, GameData};
use volya_engine::draw::Ctx;
use volya_engine::options::{OptionsData, OptionsScreen, OptionsState, Page};
use volya_engine_formats::ui::Resolution;
use volya_engine_installer::{Installer, extract};

const XGA: Resolution = Resolution::new(1024, 768);

/// The installer's `__support/userdocs/user.ltx`.
fn shipped_user_ltx() -> Vec<u8> {
    let exe = std::fs::read_dir(std::env::var_os("VOLYA_GOG_INSTALLER").unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.contains("(russian)") && n.ends_with(".exe")
        })
        .unwrap();
    let bin = exe.with_file_name(format!(
        "{}-1.bin",
        exe.file_stem().unwrap().to_string_lossy()
    ));
    let installer = Installer::parse(BufReader::new(File::open(&exe).unwrap())).unwrap();
    let file = installer
        .game_files("ru-RU")
        .find(|f| f.path == DEFAULT_USER_LTX)
        .unwrap();
    let mut data = Vec::new();
    extract(
        file,
        &mut BufReader::new(File::open(&bin).unwrap()),
        &mut data,
    )
    .unwrap();
    data
}

struct Run<'a> {
    data: &'a GameData,
    screen: OptionsScreen,
    console: Console,
    input: Input,
    now: u64,
}

impl Run<'_> {
    fn frame(&mut self) {
        self.now += 16;
        self.screen
            .update(self.data, &mut self.console, XGA, self.now, &self.input);
        self.input.end_frame();
    }

    fn click(&mut self, [x, y]: [f32; 2]) {
        self.input.handle(Event::PointerMoved { at: [x, y] });
        for pressed in [true, false] {
            self.input.handle(Event::PointerButton {
                at: None,
                button: MouseButton::Left,
                pressed,
                time: self.now as f64 * 10.0,
            });
            self.frame();
        }
    }

    fn state(&self) -> OptionsState {
        self.screen.state(self.data)
    }

    fn control(&self, name: &str) -> volya_engine::options::ControlState {
        self.state()
            .controls
            .into_iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no {name}"))
    }

    fn button(&self, name: &str) -> [f32; 2] {
        let b = self
            .state()
            .buttons
            .into_iter()
            .find(|b| b.name == name)
            .unwrap();
        [b.rect[0] + b.rect[2] / 2.0, b.rect[1] + b.rect[3] / 2.0]
    }
}

fn centre(r: [f32; 4]) -> [f32; 2] {
    [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]
}

#[test]
fn options_screen_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let cache = archives[0].file.parent().unwrap();
    let root = StdVfs::new(cache);
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let mut data = GameData::read(&tree, None, XGA).unwrap();
    let shipped = shipped_user_ltx();

    // Section 12.4: the shipped file, read at startup and saved again, is
    // written as shipped: the same lines in the same order.
    data.set_user_config(shipped.clone());
    let console = data.console();
    let saved = console.save();
    let text = String::from_utf8_lossy(&saved);
    if let Some(out) = std::env::var_os("VOLYA_DUMP_USER") {
        std::fs::write(&out, &saved).unwrap();
        std::fs::write(
            std::path::Path::new(&out).with_extension("shipped"),
            &shipped,
        )
        .unwrap();
    }
    assert_eq!(text.lines().count(), 239, "lines");
    assert_eq!(
        String::from_utf8_lossy(&saved),
        String::from_utf8_lossy(&shipped),
        "saved like the shipped file"
    );
    assert_eq!(console.video_mode(), (1024, 768));
    assert_eq!(console.status("_preset").unwrap(), "Extreme");
    // The bindings are the defaults (section 11.6: 58 `bind` lines).
    assert_eq!(text.lines().filter(|l| l.starts_with("bind ")).count(), 58);

    // The layout, strings and textures resolve.
    let opt = OptionsData::load(&tree, &data, XGA).unwrap();
    let screen = OptionsScreen::new(&data, &opt).unwrap();
    let (textures, strings) = screen.references();
    for id in &textures {
        let file = match data.descriptions.get(id) {
            Some(d) => d.file.clone(),
            // A texture path (the backgrounds and videos).
            None => id.clone(),
        };
        ui::texture_source(&tree, &data.aliases, &file)
            .unwrap_or_else(|e| panic!("texture {id}: {e}"));
    }
    let missing: Vec<&String> = strings
        .iter()
        .filter(|k| !data.strings.contains(k))
        .collect();
    // Sections 6.1 and 16: the entries without a string are the list
    // values with texts of their own; every caption key resolves.
    assert!(
        missing.iter().all(|k| k.starts_with("gd_")
            || k.starts_with("renderer_")
            || ["Minimum", "Low", "Default", "High", "Extreme"].contains(&k.as_str())),
        "missing strings {missing:?}"
    );
    eprintln!(
        "{} textures, {} strings; without a string: {missing:?}",
        textures.len(),
        strings.len()
    );

    let mut run = Run {
        data: &data,
        screen,
        console,
        input: Input::new(),
        now: 1000,
    };
    run.screen
        .open(run.data, &mut run.console, XGA, run.now, [512.0, 384.0]);
    let s = run.state();
    assert_eq!(s.page, Page::Video);
    assert_eq!(s.controls.len(), 31, "option controls (section 16)");
    assert_eq!(s.cells.len(), 118, "key cells");
    // Section 16's absolute rectangles.
    assert_eq!(
        run.control("list_renderer").rect,
        [664.0, 262.0, 261.0, 23.0]
    );
    assert_eq!(run.control("track_gamma").rect, [664.0, 352.0, 261.0, 21.0]);
    let b = |name: &str| {
        run.state()
            .buttons
            .into_iter()
            .find(|b| b.name == name)
            .unwrap()
            .rect
    };
    assert_eq!(b("btn_accept"), [634.0, 597.0, 157.0, 48.0]);
    assert_eq!(b("btn_cancel"), [790.0, 597.0, 157.0, 48.0]);
    assert_eq!(b("tab 3")[..2], [818.0, 216.0]);
    assert_eq!(b("btn_advanced"), [812.0, 567.0, 111.0, 29.0]);
    let forward = s
        .cells
        .iter()
        .find(|c| c.action == "left" && c.slot == 0)
        .unwrap();
    assert_eq!(forward.rect[..2], [709.0, 413.0]);
    assert_eq!(forward.key, Some("kLEFT"));
    let last = s.cells.last().unwrap();
    assert_eq!(last.action, "speech_menu_1");
    assert_eq!(last.rect[1], 1771.0);
    assert_eq!(run.control("list_presets").value, "максимальные");
    assert_eq!(run.control("list_renderer").value, "Статичное освещение");
    assert_eq!(run.control("list_resolution").entries[1], "1024x768");
    assert!(
        !run.control("check_r2_sun").enabled,
        "only with the full renderer"
    );
    // Opening refreshed texture_lod's bar: a video restart is marked
    // (section 5.4).
    assert!(s.video_restart);

    // The screen's scenes.
    let mut assets = Assets::default();
    let scene = |run: &Run, assets: &mut Assets| {
        let mut ctx = Ctx {
            tree: &tree,
            data: run.data,
            assets,
            resolution: XGA,
            clock_ms: run.now,
        };
        run.screen.scene(&mut ctx).unwrap()
    };
    let sc = scene(&run, &mut assets);
    assert!(sc.first("dialog").is_some() && sc.first("cursor").is_some());

    // The preset list: choosing "низкие" runs rspec_low.ltx at once
    // (section 8) and the advanced bars show it.
    run.click(centre(run.control("list_presets").rect));
    assert_eq!(run.state().open_list.as_deref(), Some("list_presets"));
    let r = run.control("list_presets").rect;
    run.click([r[0] + 20.0, r[1] + 23.0 + 18.0 + 9.0]);
    assert_eq!(run.console.status("_preset").unwrap(), "Low");
    assert_eq!(run.console.status("r__tf_aniso").unwrap(), "2");
    assert_eq!(run.control("list_presets").value, "низкие");
    run.click(run.button("btn_advanced"));
    assert_eq!(run.state().page, Page::Advanced);
    assert_eq!(run.control("track_aniso").value, "2");
    // Section 6.2 and 16: rows at y = 252 + 30 j, controls at x 664.
    assert_eq!(
        run.control("track_vis_dist").rect,
        [664.0, 252.0, 245.0, 21.0]
    );
    assert_eq!(run.control("check_r2_sun").rect[..2], [664.0, 432.0]);
    assert_eq!(run.control("check_60hz").rect[..2], [664.0, 612.0]);
    // Scrolled to the bottom (78 units, section 4.8), row 12 shows at 534.
    for _ in 0..3 {
        run.input.handle(Event::PointerMoved { at: [700.0, 400.0] });
        run.input.handle(Event::Wheel { steps: 1 });
        run.frame();
    }
    assert_eq!(run.state().scroll[0], 78);
    assert_eq!(run.control("check_60hz").rect[1], 534.0);
    scene(&run, &mut assets);
    // Escape returns to the basic page; Cancel undoes the preset.
    run.input.handle(Event::Key {
        key: Key::Escape,
        pressed: true,
    });
    run.frame();
    assert_eq!(run.state().page, Page::Video);
    run.click(run.button("btn_cancel"));
    assert_eq!(run.console.status("_preset").unwrap(), "Extreme");
    assert_eq!(run.console.status("r__tf_aniso").unwrap(), "16");
    let closed = run.screen.take_closed().unwrap();
    assert!(closed.video_restart);

    // The controls tab: a key captured into kb_forward, applied.
    run.screen
        .open(run.data, &mut run.console, XGA, run.now, [512.0, 384.0]);
    run.click(run.button("tab 3"));
    scene(&run, &mut assets);
    let cell = run
        .state()
        .cells
        .into_iter()
        .find(|c| c.action == "forward" && c.slot == 0)
        .unwrap();
    run.click(centre(cell.rect));
    assert_eq!(run.state().waiting.as_deref(), Some("forward:0"));
    run.input.handle(Event::Key {
        key: Key::T,
        pressed: true,
    });
    run.frame();
    run.click(run.button("btn_accept"));
    let forward = action_index("forward").unwrap();
    assert_eq!(run.console.keys(forward), [Some("kT"), None]);
    assert!(
        String::from_utf8(run.console.save())
            .unwrap()
            .contains("bind forward kT\r\n")
    );
    // Section 8.8's cost of the controls tab's frames is the
    // render-screens suite's.
    run.screen
        .open(run.data, &mut run.console, XGA, run.now, [512.0, 384.0]);
    run.click(run.button("tab 3"));
    run.input.handle(Event::Key {
        key: Key::Escape,
        pressed: true,
    });
    run.frame();
    run.screen.take_closed();

    // A drag in an open list scrolls it (our design); a press and release
    // without moving chooses.
    run.screen
        .open(run.data, &mut run.console, XGA, run.now, [512.0, 384.0]);
    run.click(run.button("tab 0"));
    let small = Resolution::new(2868, 1320);
    run.screen
        .open(run.data, &mut run.console, small, run.now, [512.0, 384.0]);
    let r = run.control("list_resolution").rect;
    run.click(centre(r));
    assert_eq!(run.state().open_list.as_deref(), Some("list_resolution"));
    let rows = [r[0] + 40.0, r[1] + 23.0 + 60.0];
    run.input.handle(Event::PointerMoved { at: rows });
    run.input.handle(Event::PointerButton {
        at: None,
        button: MouseButton::Left,
        pressed: true,
        time: 0.0,
    });
    run.frame();
    run.input.handle(Event::PointerMoved {
        at: [rows[0], rows[1] - 40.0],
    });
    run.frame();
    run.input.handle(Event::PointerButton {
        at: None,
        button: MouseButton::Left,
        pressed: false,
        time: 0.0,
    });
    run.frame();
    let s = run.state();
    assert_eq!(
        s.open_list.as_deref(),
        Some("list_resolution"),
        "still open"
    );
    assert_eq!(run.console.video_mode(), (1024, 768), "nothing chosen");
    run.input.handle(Event::Key {
        key: Key::Escape,
        pressed: true,
    });
    run.frame();
    run.screen.take_closed();

    // Cancel undoes a list whose backed-up entry does not exist: one
    // resolution offered (a 700 × 500 screen), vid_mode 1024x768 not in it.
    let tiny = Resolution::new(700, 500);
    run.screen
        .open(run.data, &mut run.console, tiny, run.now, [512.0, 384.0]);
    assert_eq!(run.control("list_resolution").entries, ["700x500"]);
    let r = run.control("list_resolution").rect;
    run.screen
        .update(run.data, &mut run.console, tiny, run.now, &run.input);
    for at in [centre(r), [r[0] + 20.0, r[1] + 23.0 + 9.0]] {
        run.input.handle(Event::PointerMoved { at });
        for pressed in [true, false] {
            run.input.handle(Event::PointerButton {
                at: None,
                button: MouseButton::Left,
                pressed,
                time: 0.0,
            });
            run.now += 16;
            run.screen
                .update(run.data, &mut run.console, tiny, run.now, &run.input);
            run.input.end_frame();
        }
    }
    assert_eq!(run.console.video_mode(), (700, 500), "chosen at once");
    let cancel = run.button("btn_cancel");
    run.input.handle(Event::PointerMoved { at: cancel });
    for pressed in [true, false] {
        run.input.handle(Event::PointerButton {
            at: None,
            button: MouseButton::Left,
            pressed,
            time: 0.0,
        });
        run.now += 16;
        run.screen
            .update(run.data, &mut run.console, tiny, run.now, &run.input);
        run.input.end_frame();
    }
    assert_eq!(run.console.video_mode(), (1024, 768), "Cancel undid it");
    assert!(run.screen.take_closed().unwrap().video_restart);

    // Touch and gamepad (our design): the touchpad (#108) moves the cursor
    // and a tap clicks at it; the d-pad moves the cursor to the nearest
    // control, south clicks, east is Escape.
    run.screen
        .open(run.data, &mut run.console, XGA, run.now, [512.0, 384.0]);
    let tap = |run: &mut Run, at: [f32; 2]| {
        let ([x, y], s) = (run.input.cursor_ui(), run.input.sensitivity());
        run.input.handle(Event::TouchpadMotion {
            delta: [2.0 * (at[0] - x) / s, 2.0 * (at[1] - y) / s],
            speed: 0.0,
        });
        for pressed in [true, false] {
            run.input.handle(Event::TouchpadButton {
                button: MouseButton::Left,
                pressed,
                double: false,
                time: run.now as f64 * 10.0,
            });
            run.frame();
        }
    };
    let tab1 = run.button("tab 1");
    tap(&mut run, tab1);
    assert_eq!(run.state().page, Page::Sound);
    assert_eq!(run.control("check_eax").value, "on");
    let eax = centre(run.control("check_eax").rect);
    tap(&mut run, eax);
    assert_eq!(run.control("check_eax").value, "off");
    let pad = |run: &mut Run, button: GamepadButton| {
        for pressed in [true, false] {
            run.input.handle(Event::GamepadButton { button, pressed });
            run.frame();
        }
    };
    tap(&mut run, tab1);
    pad(&mut run, GamepadButton::DpadDown);
    let eax = run.control("check_eax").rect;
    let c = run.state().cursor;
    assert!(
        c[0] >= eax[0] && c[0] < eax[0] + eax[2] && c[1] >= eax[1] && c[1] < eax[1] + eax[3],
        "the d-pad reached the check box: {c:?}"
    );
    pad(&mut run, GamepadButton::South);
    assert_eq!(run.control("check_eax").value, "on");
    // East while a key cell waits ends the waiting, binding nothing (our
    // design).
    let tab3 = run.button("tab 3");
    tap(&mut run, tab3);
    let cell = run
        .state()
        .cells
        .into_iter()
        .find(|c| c.action == "forward" && c.slot == 1)
        .unwrap();
    tap(&mut run, centre(cell.rect));
    assert_eq!(run.state().waiting.as_deref(), Some("forward:1"));
    pad(&mut run, GamepadButton::East);
    let s = run.state();
    assert_eq!(s.waiting, None);
    let key = |s: &OptionsState| {
        s.cells
            .iter()
            .find(|c| c.action == "forward" && c.slot == 1)
            .unwrap()
            .key
    };
    assert_eq!(key(&s), None);
    assert_eq!(run.screen.take_closed(), None, "still open");
    pad(&mut run, GamepadButton::East);
    assert_eq!(
        run.screen.take_closed(),
        Some(Default::default()),
        "east is Escape"
    );

    // The renderer list's dynamic entries are disabled (#364, a deviation
    // decided by the user): no input chooses them, so the restart message
    // (section 10) never opens.
    run.screen
        .open(run.data, &mut run.console, XGA, run.now, [512.0, 384.0]);
    run.click(run.button("tab 0"));
    let r = run.control("list_renderer").rect;
    let entry = |k: f32| [r[0] + 20.0, r[1] + 23.0 + k * 18.0 + 9.0];
    run.click(centre(r));
    assert_eq!(run.state().open_list.as_deref(), Some("list_renderer"));
    let sc = scene(&run, &mut assets);
    let alpha = |k: usize| {
        sc.first(&format!("list_renderer entry {k}"))
            .unwrap()
            .color
            .a
    };
    assert_eq!(
        (alpha(1), alpha(2)),
        (128, 128),
        "section 4.6's disabled rows"
    );
    assert_eq!(alpha(0), 255);
    run.click(entry(2.0));
    tap(&mut run, entry(1.0));
    assert_eq!(run.state().open_list.as_deref(), Some("list_renderer"));
    // The d-pad reaches only the enabled entry: from it, nothing below.
    pad(&mut run, GamepadButton::DpadUp);
    let before = run.state().cursor;
    assert!((before[1] - entry(0.0)[1]).abs() < 1.0, "{before:?}");
    pad(&mut run, GamepadButton::DpadDown);
    assert_eq!(run.state().cursor, before);
    pad(&mut run, GamepadButton::South);
    assert_eq!(run.state().open_list, None);
    assert_eq!(run.console.status("renderer").unwrap(), "renderer_r1");
    assert_eq!(run.control("list_renderer").value, "Статичное освещение");
    assert!(!run.state().message);
    assert!(!run.control("check_r2_sun").enabled);
    run.click(run.button("btn_cancel"));

    // A dynamic renderer stored by the original's user.ltx: kept as
    // written, shown as the static renderer it is drawn with.
    run.console.exec("renderer renderer_r2");
    run.screen
        .open(run.data, &mut run.console, XGA, run.now, [512.0, 384.0]);
    let c = run.control("list_renderer");
    assert_eq!(
        (c.value.as_str(), c.changed),
        ("Статичное освещение", false)
    );
    for name in [
        "check_r2_sun",
        "check_r2_sun_details",
        "track_light_distance",
        "track_particles_distance",
    ] {
        assert!(!run.control(name).enabled, "{name}");
    }
    run.click(run.button("btn_accept"));
    assert_eq!(run.console.status("renderer").unwrap(), "renderer_r2");
}

/// The app opens the screen from the title menu's options item.
#[test]
fn the_title_menu_opens_the_options_screen() {
    use volya_engine::app::{App, AppState};
    use volya_engine::audio::Silent;
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let cache = archives[0].file.parent().unwrap();
    let root = StdVfs::new(cache);
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let mut data = GameData::load(&tree, None, XGA).unwrap();
    data.set_user_config(shipped_user_ltx());
    let mut app = App::new(data).unwrap();
    let mut input = Input::new();
    let mut t = 0.0;
    let mut step = |app: &mut App, input: &mut Input, keys: &[Key]| {
        for &key in keys {
            input.handle(Event::Key { key, pressed: true });
            input.handle(Event::Key {
                key,
                pressed: false,
            });
        }
        t += 100.0;
        app.update(&tree, t, input, &mut Silent);
        input.end_frame();
    };
    step(&mut app, &mut input, &[]);
    for _ in 0..5 {
        step(&mut app, &mut input, &[Key::Escape]);
    }
    for _ in 0..3 {
        step(&mut app, &mut input, &[Key::Down]);
    }
    step(&mut app, &mut input, &[Key::Enter]);
    step(&mut app, &mut input, &[]);
    let AppState::Menu { menu, .. } = app.state() else {
        panic!("not in the menu: {:?}", app.state())
    };
    assert_eq!(menu.error, None);
    let options = menu.options.expect("the options screen");
    assert_eq!(options.page, Page::Video);
    // A key captured and applied: the next level's controls have it.
    let click = |app: &mut App, input: &mut Input, at: [f32; 2], t: f64| {
        input.handle(Event::PointerMoved { at });
        for pressed in [true, false] {
            input.handle(Event::PointerButton {
                at: None,
                button: MouseButton::Left,
                pressed,
                time: t,
            });
        }
        app.update(&tree, t, input, &mut Silent);
        input.end_frame();
    };
    // 800 × 600 from the resolution list (the screen is 1024 × 768 here).
    click(&mut app, &mut input, [794.0, 333.0], 1500.0);
    click(&mut app, &mut input, [684.0, 354.0], 1600.0);
    assert_eq!(app.video_mode(), XGA, "applied by the video restart only");
    click(&mut app, &mut input, [876.0, 229.0], 2000.0);
    click(&mut app, &mut input, [758.0, 534.0], 3000.0);
    input.handle(Event::Key {
        key: Key::T,
        pressed: true,
    });
    app.update(&tree, 3100.0, &mut input, &mut Silent);
    input.end_frame();
    click(&mut app, &mut input, [712.0, 621.0], 4000.0);
    let AppState::Menu { menu, .. } = app.state() else {
        panic!()
    };
    assert!(menu.options.is_none(), "Apply closed the screen");
    assert_eq!(
        app.video_mode(),
        Resolution::new(800, 600),
        "the video restart"
    );
    let forward = app
        .data()
        .controls
        .bindings
        .controls(volya_engine::controls::MoveAction::Forward)
        .collect::<Vec<_>>();
    assert_eq!(forward, [volya_engine::core::input::Control::Key(Key::T)]);
    assert!(
        String::from_utf8(app.take_save().unwrap())
            .unwrap()
            .contains("bind forward kT\r\n")
    );
}
