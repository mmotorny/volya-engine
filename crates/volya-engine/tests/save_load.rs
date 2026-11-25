// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The menu during a game and its save and load screens built from the
//! Russian edition (BYOA), checked against docs/behavior/title-menu.md
//! (section 4.1) and docs/behavior/save-load-screens.md (sections 3–6,
//! 13): the in-game list's captions, the screens' and boxes' layouts,
//! strings and textures resolve, and the screens draw.
//! `VOLYA_DUMP_SAVE=<file.ppm>` and `VOLYA_DUMP_LOAD=<file.ppm>` write the
//! screens' frames locally, `VOLYA_DUMP_KEYBOARD=<file.ppm>` the save screen
//! with the on-screen keyboard (#94). Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::audio::Silent;
use volya_engine::core::input::{Event, GamepadButton, Input, Key, MouseButton};
use volya_engine::core::ui;
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::{Assets, GameData};
use volya_engine::draw::Ctx;
use volya_engine::menu_screens::{Frame, MenuScreens, MenuState};
use volya_engine::present::Presenter;
use volya_engine::render::{Renderer, wgpu};
use volya_engine::save_load::{SaveInfo, SaveLoadData, display_time};
use volya_engine::scene::Scene;
use volya_engine::title_menu::{Opening, TitleMenu};
use volya_engine_formats::ui::Resolution;

const XGA: Resolution = Resolution::new(1024, 768);

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

#[test]
fn in_game_menu_and_save_load_screens_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let cache = archives[0].file.parent().unwrap();
    let root = StdVfs::new(cache);
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, XGA).unwrap();

    // Title-menu section 4.1: the in-game list and its captions.
    let single = data.menu.menu("menu_main_single").unwrap();
    let items: Vec<(&str, &str)> = single
        .buttons
        .iter()
        .map(|b| (b.name.as_str(), data.strings.get(&b.caption)))
        .collect();
    assert_eq!(items[0], ("btn_ret", "Возврат к игре"));
    assert_eq!(items[3], ("btn_save", "Сохранить игру"));
    assert_eq!(items[6], ("btn_quit_to_mm", "Завершить игру"));
    assert_eq!(
        items.iter().map(|i| i.0).collect::<Vec<_>>(),
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

    // The layouts, strings and textures resolve.
    let sl = SaveLoadData::load(&tree, &data, XGA).unwrap();
    let (textures, strings) = sl.references(&data).unwrap();
    for id in &textures {
        let file = match data.descriptions.get(id) {
            Some(d) => d.file.clone(),
            None => id.clone(),
        };
        ui::texture_source(&tree, &data.aliases, &file)
            .unwrap_or_else(|e| panic!("texture {id}: {e}"));
    }
    let missing: Vec<&String> = strings
        .iter()
        .filter(|k| !data.strings.contains(k))
        .collect();
    assert!(missing.is_empty(), "missing strings {missing:?}");
    // Section 13 fact 4 and the boxes' texts (section 6).
    for (key, text) in [
        ("ui_mm_save", "Сохранить"),
        ("ui_mm_load", "Загрузить"),
        ("ui_mm_delete", "Удалить"),
        ("ui_mm_cancel", "Отмена"),
        ("ui_st_btn_yes", "Да"),
        ("ui_st_btn_no", "Нет"),
        ("ui_st_file_name_is_empty", "Имя файла пустое"),
        (
            "ui_mm_quit_windows_message",
            "Действительно ли вы хотите выйти в Windows?",
        ),
    ] {
        assert_eq!(data.strings.get(key), text, "{key}");
    }
    eprintln!("{} textures, {} strings", textures.len(), strings.len());

    // Section 6: the boxes' rectangles.
    for name in [
        "message_box_quit_windows",
        "message_box_quit_game",
        "message_box_empty_file_name",
        "message_box_file_already_exist",
        "message_box_delete_file_name",
        "message_box_confirm_load_save",
        "message_box_invalid_saved_game",
    ] {
        let b = sl.message_box(&data, name).unwrap();
        assert_eq!(b.frame.window.absolute(0.0, 0.0).x, 200.0, "{name}");
        assert_eq!(b.button_rect(0).y, 404.0, "{name}");
    }
    let quit = sl.message_box(&data, "message_box_quit_game").unwrap();
    assert_eq!(
        (quit.button_rect(0).x, quit.button_rect(1).x),
        (450.0, 570.0)
    );
    // The quit boxes' own types (Yes and No, our reading), and the data's
    // text for the quit-game box, which the menu replaces (title-menu.md
    // 4.5).
    let windows = sl.message_box(&data, "message_box_quit_windows").unwrap();
    assert_eq!(windows.kind.as_deref(), Some("quit_windows"));
    assert_eq!(quit.kind.as_deref(), Some("quit_game"));
    assert_eq!(
        quit.text.text.as_ref().and_then(|t| t.key.as_deref()),
        Some("ui_st_want_to_leave_game_q")
    );

    // Section 5.2's defaults for a file that is not valid: the New Game
    // start and the first level.
    let defaults = SaveInfo::defaults(&tree, &data).unwrap();
    assert_eq!(display_time(defaults.game_time), "05:30 05/01/2012");
    assert_eq!(defaults.health, 1.0);
    let text = SaveInfo::read(&tree, &data, b"not a save")
        .unwrap()
        .text(&data);
    assert!(
        text.contains("Время: 05:30 05/01/2012\\nЗдоровье 100%"),
        "{text}"
    );
    // A valid container whose body cannot be read is fatal (sav.md 3, 7.1).
    let mut broken = vec![0xFF; 4];
    broken.extend(3u32.to_le_bytes());
    broken.extend(100u32.to_le_bytes());
    broken.extend([0; 8]);
    assert!(SaveInfo::read(&tree, &data, &broken).is_err());

    // The screens through the menu during a game: Save (item 3), then
    // Load (item 2).
    let mut console = data.console();
    let mut screens = MenuScreens::default();
    let mut input = Input::new();
    let opening = Opening {
        in_game: true,
        last_save: false,
    };
    let mut menu = TitleMenu::open(&data, 0, &input, &mut Silent, opening);
    let mut now = 0;
    let mut frame = |keys: &[Key],
                     menu: &mut TitleMenu,
                     screens: &mut MenuScreens,
                     input: &mut Input|
     -> MenuState {
        for &key in keys {
            for pressed in [true, false] {
                input.handle(Event::Key { key, pressed });
            }
        }
        now += 16;
        screens.update(
            menu,
            Frame {
                tree: &tree,
                data: &data,
                console: &mut console,
                native: XGA,
                video: XGA,
                now,
                input,
                audio: &mut Silent,
                field_font: None,
            },
        );
        input.end_frame();
        screens.state(menu, &data, now)
    };
    let mut gpu = renderer();
    let mut presenter = Presenter::new();
    let mut assets = Assets::default();
    let mut draw = |menu: &TitleMenu, screens: &MenuScreens, var: &str| -> Scene {
        let mut ctx = Ctx {
            tree: &tree,
            data: &data,
            assets: &mut assets,
            resolution: XGA,
            clock_ms: 1000,
        };
        let scene = screens.scene(menu, &mut ctx).unwrap();
        let list = presenter.draw_list(&mut gpu, &tree, &scene, 1000);
        let errors = presenter.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
        if let Some(path) = std::env::var_os(var) {
            let pixels =
                pollster::block_on(gpu.capture(&list, 1024, 768, wgpu::Color::BLACK)).unwrap();
            let mut ppm = b"P6 1024 768 255\n".to_vec();
            for p in pixels.chunks(4) {
                ppm.extend_from_slice(&p[..3]);
            }
            std::fs::write(path, ppm).unwrap();
        }
        scene
    };
    frame(
        &[Key::Down, Key::Down, Key::Down],
        &mut menu,
        &mut screens,
        &mut input,
    );
    let s = frame(&[Key::Enter], &mut menu, &mut screens, &mut input);
    let save = s.save_screen.expect("the save screen");
    // Table 4.1 and section 13 fact 2.
    assert_eq!(save.screen.list.rect, [490.0, 274.0, 429.0, 287.0]);
    assert_eq!(save.screen.list.visible, 15);
    let rects: Vec<[f32; 4]> = save.screen.buttons.iter().map(|b| b.rect).collect();
    assert_eq!(
        rects,
        [
            [480.0, 595.0, 157.0, 48.0],
            [636.0, 595.0, 157.0, 48.0],
            [792.0, 595.0, 157.0, 48.0]
        ]
    );
    let scene = draw(&menu, &screens, "VOLYA_DUMP_SAVE");
    assert!(scene.first("edit").is_some() && scene.first("caption").is_some());
    // The on-screen keyboard (#94): a gamepad's south on the name field
    // opens it; it draws with the field's font; east closes it, the screen
    // stays.
    let [x, y, w, h] = save.field;
    input.handle(Event::PointerMoved {
        at: [x + w / 2.0, y + h / 2.0],
    });
    let pad = |input: &mut Input, button: GamepadButton| {
        for pressed in [true, false] {
            input.handle(Event::GamepadButton { button, pressed });
        }
    };
    pad(&mut input, GamepadButton::South);
    let s = frame(&[], &mut menu, &mut screens, &mut input);
    let save = s.save_screen.expect("the save screen");
    assert!(
        save.focused && save.keyboard.is_some(),
        "the keyboard opens"
    );
    pad(&mut input, GamepadButton::South);
    let s = frame(&[], &mut menu, &mut screens, &mut input);
    assert_eq!(s.save_screen.expect("the save screen").name, "1");
    // A click on Cancel while it shows doesn't reach the screen.
    let cancel = [792.0 + 140.0, 595.0 + 24.0];
    input.handle(Event::PointerMoved { at: cancel });
    for pressed in [true, false] {
        input.handle(Event::PointerButton {
            at: Some(cancel),
            button: MouseButton::Left,
            pressed,
            time: 0.0,
        });
    }
    let s = frame(&[], &mut menu, &mut screens, &mut input);
    assert!(
        s.save_screen.is_some_and(|s| s.keyboard.is_some()),
        "the click is swallowed"
    );
    let scene = draw(&menu, &screens, "VOLYA_DUMP_KEYBOARD");
    assert_eq!(scene.layer("keyboard key").count(), 50);
    // 46 character keys with labels; Shift, Space, Backspace, Done drawn
    // as symbols.
    assert!(scene.layer("keyboard label").count() >= 46);
    assert_eq!(scene.layer("keyboard symbol").count(), 7 + 3 + 3 + 2);
    pad(&mut input, GamepadButton::East);
    let s = frame(&[], &mut menu, &mut screens, &mut input);
    let save = s.save_screen.expect("east's release leaves the screen");
    assert!(
        !save.focused && save.keyboard.is_none(),
        "the keyboard closes"
    );
    let s = frame(&[Key::Escape], &mut menu, &mut screens, &mut input);
    assert!(s.save_screen.is_none());
    let s = frame(&[Key::Up, Key::Enter], &mut menu, &mut screens, &mut input);
    let load = s.load_screen.expect("the load screen");
    assert_eq!(load.screen.list.rect, [490.0, 399.0, 429.0, 180.0]);
    assert_eq!(load.screen.list.visible, 10);
    assert!(load.screen.list.rows.is_empty());
    let scene = draw(&menu, &screens, "VOLYA_DUMP_LOAD");
    assert!(scene.first("picture").is_some() && scene.first("file_info").is_some());
    let s = frame(&[Key::Escape], &mut menu, &mut screens, &mut input);
    assert!(s.load_screen.is_none());
    // The quit box over the menu draws too.
    for _ in 0..4 {
        frame(&[Key::Down], &mut menu, &mut screens, &mut input);
    }
    let s = frame(&[Key::Enter], &mut menu, &mut screens, &mut input);
    let message = s.message.unwrap();
    assert_eq!(message.name, "message_box_quit_game");
    // Section 4.5: the single-player text is shown.
    assert_eq!(message.text, "ui_mm_quit_game_message");
    assert_eq!(
        data.strings.get(&message.text),
        "Действительно ли вы желаете покинуть игру?"
    );
    let scene = draw(&menu, &screens, "VOLYA_DUMP_QUIT");
    assert!(scene.first("message").is_some());
}
