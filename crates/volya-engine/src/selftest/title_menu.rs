// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The startup sequence and the title menu drawn from the game (once
//! `tests/gog.rs`'s drawing; docs/behavior/startup.md,
//! docs/behavior/title-menu.md, docs/formats/text-and-fonts.md section 5):
//! the THQ video's frame is not black, the menu's captions are black
//! glyphs on the slats; and each video's texture (startup.md 6.6), with no
//! pixel of the GSC item the green of its coded rows 360–367 (#35). Frame
//! `title-menu`.

use std::collections::BTreeSet;
use std::io::Cursor;

use super::{Context, Outcome, Run, capture, ensure, ensure_eq};
use crate::app::{App, AppState};
use crate::audio::Silent;
use crate::core::input::{Event, Input, Key};
use crate::core::vfs::db::DbVfs;
use crate::data::GameData;
use crate::present::Presenter;
use volya_engine_formats::ui::Resolution;
use volya_engine_formats::video::TheoraVideo;

pub const CHECKS: &[(&str, Run)] = &[
    ("startup and title menu from game data", |c| {
        Box::pin(startup_and_title_menu_from_game_data(c))
    }),
    ("video frames as textures from game data", |c| {
        Box::pin(video_frames_as_textures_from_game_data(c))
    }),
];

const XGA: Resolution = Resolution::new(1024, 768);

pub(super) fn key(input: &mut Input, key: Key) {
    input.handle(Event::Key { key, pressed: true });
    input.handle(Event::Key {
        key,
        pressed: false,
    });
}

pub(super) fn step(app: &mut App, input: &mut Input, t: f64) {
    // The startup sequence and the menu read nothing in updates.
    let empty = DbVfs::<std::io::Empty>::new();
    app.update(&empty, t, input, &mut Silent);
    input.end_frame();
}

fn app(cx: &Context<'_>) -> Result<App, String> {
    let data = GameData::load(cx.tree, None, XGA).map_err(|e| format!("game data: {e}"))?;
    App::new(data).map_err(|e| format!("app: {e}"))
}

pub async fn startup_and_title_menu_from_game_data(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let mut app = app(cx)?;
    let mut input = Input::new();
    let mut gpu = cx.renderer();
    let mut presenter = Presenter::new();

    // The THQ video, 2.5 s in.
    step(&mut app, &mut input, 1000.0);
    step(&mut app, &mut input, 3500.0);
    let s = app
        .scene(tree, XGA)
        .map_err(|e| format!("THQ scene: {e}"))?;
    let list = presenter.draw_list(&mut gpu, tree, &s, app.clock());
    ensure_eq!(list.quads.len(), 2, "THQ quads");
    let pixels = capture(&mut gpu, &list, 256, 192).await?;
    ensure!(
        pixels.chunks(4).any(|p| p[..3] != [0, 0, 0]),
        "THQ frame is black"
    );

    // As tests/gog.rs: Esc through the items to the menu, then Down to
    // item 1.
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 3600.0);
    key(&mut input, Key::Enter);
    step(&mut app, &mut input, 3700.0);
    key(&mut input, Key::Escape);
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 4000.0);
    step(&mut app, &mut input, 6000.0);
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 7000.0);
    step(&mut app, &mut input, 12_000.0);
    step(&mut app, &mut input, 12_600.0);
    key(&mut input, Key::Down);
    step(&mut app, &mut input, 13_000.0);
    step(&mut app, &mut input, 13_300.0);
    step(&mut app, &mut input, 13_700.0);
    let AppState::Menu { menu, .. } = app.state() else {
        return Err("not in the menu".into());
    };
    ensure_eq!(menu.menu.selected, 1, "the item selected");

    // Rendered: the captions are black glyphs on the slats.
    let s = app
        .scene(tree, XGA)
        .map_err(|e| format!("menu scene: {e}"))?;
    let list = presenter.draw_list(&mut gpu, tree, &s, app.clock());
    // Every sprite and the lens's map (title-menu.md 4.6).
    ensure!(list.lens.is_some(), "no lens");
    ensure_eq!(
        list.quads.len(),
        s.sprites.len() + 1,
        "{:?}",
        presenter.take_errors()
    );
    let pixels = capture(&mut gpu, &list, 1024, 768).await?;
    cx.dump("title-menu", 1024, 768, &pixels);
    let at = |x: usize, y: usize| &pixels[(y * 1024 + x) * 4..][..3];
    let luma = |p: &[u8]| p.iter().map(|&c| u32::from(c)).sum::<u32>();
    let caption = s.layer_bounds("caption 5").ok_or("no caption 5")?;
    let (mut dark, mut total) = (0, 0);
    for y in caption[1] as usize..caption[3] as usize {
        for x in caption[0] as usize..caption[2] as usize {
            total += 1;
            dark += usize::from(luma(at(x, y)) < 60);
        }
    }
    ensure!(dark * 10 > total, "{dark} of {total} caption pixels dark");
    let distinct: BTreeSet<&[u8]> = pixels.chunks(4).collect();
    ensure!(distinct.len() > 1000, "{} colours", distinct.len());
    Ok(format!(
        "menu: {} quads, {} colours",
        list.quads.len(),
        distinct.len()
    ))
}

/// The GSC item drawn: no pixel is the green of its coded rows 360–367
/// (#35); and each startup and title-menu video's texture size
/// (startup.md 6.6, table).
pub async fn video_frames_as_textures_from_game_data(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let mut gpu = cx.renderer();

    // The GSC item, rendered: skip THQ and GSC WP, then the item's 1 s delay.
    let mut app = app(cx)?;
    let mut input = Input::new();
    let mut presenter = Presenter::new();
    step(&mut app, &mut input, 1000.0);
    key(&mut input, Key::Escape);
    key(&mut input, Key::Escape);
    step(&mut app, &mut input, 1100.0);
    let AppState::Startup { startup, .. } = app.state() else {
        return Err("not in the startup".into());
    };
    ensure_eq!(startup.item, 2, "the GSC item");
    let mut drawn = 0;
    for t in (2150..2400).step_by(20) {
        step(&mut app, &mut input, f64::from(t));
        let s = app
            .scene(tree, XGA)
            .map_err(|e| format!("GSC scene: {e}"))?;
        let list = presenter.draw_list(&mut gpu, tree, &s, app.clock());
        ensure_eq!(list.quads.len(), 2, "{:?}", presenter.take_errors());
        let pixels = capture(&mut gpu, &list, 1024, 768).await?;
        let green = pixels
            .chunks(4)
            .filter(|p| p[0] < 8 && p[2] < 8 && (120..150).contains(&p[1]))
            .count();
        ensure_eq!(green, 0, "GSC at {t} ms: green pixels");
        drawn += usize::from(pixels.chunks(4).any(|p| p[..3] != [0, 0, 0]));
    }
    ensure!(drawn > 0, "GSC frames are black");

    // Video and its texture.
    for (name, texture) in [
        ("intro\\thq_logo_hyuv", (1024, 512)),
        ("intro\\intro_gsc-wp", (1024, 512)),
        ("intro\\intro_gsc", (1024, 512)),
        ("ui\\ui_mm_main_back_crop", (1024, 512)),
        ("ui\\ui_mm_fire", (128, 256)),
    ] {
        let path = format!("gamedata\\textures\\{name}.ogm");
        let bytes = crate::core::vfs::read(tree, &path).map_err(|e| format!("{path}: {e}"))?;
        let video = TheoraVideo::open(Cursor::new(bytes)).map_err(|e| format!("{path}: {e}"))?;
        let i = video.info();
        let size = gpu.create_video_texture(i.width, i.height).texture().size();
        ensure_eq!(size, texture, "{name}");
    }
    Ok(format!("{drawn} GSC frames drawn"))
}
