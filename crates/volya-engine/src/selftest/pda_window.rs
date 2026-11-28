// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The PDA window (docs/behavior/pda-window.md) built from the game's
//! data and drawn without errors: on the tasks tab at the New Game's start,
//! the diary's new-content mark set and the cursor resting on the contacts
//! tab, its hint shown (pda-controls.md 6). Every texture id and string
//! key it draws exists. Frame `pda`. The map tab (pda-map.md) after its
//! first showing's flight to the New Game's start, the cursor resting on
//! the player's mark (frame `pda-map`), then at the global map with the
//! cursor on Escape's picture (frame `pda-map-global`).

use super::{Context, Outcome, Run, capture, ensure};
use crate::core::input::{Event, Input, MouseButton};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::pda_window::map::{GLOBAL_BUTTON, MapPlayer};
use crate::pda_window::{Frame, Keys, MAP_TAB, PdaWindow};
use crate::present::Presenter;
use crate::scene::Scene;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[
    ("pda window draws", |c| Box::pin(draws(c))),
    ("pda map draws", |c| Box::pin(map_draws(c))),
];

const XGA: Resolution = Resolution::new(1024, 768);
/// The New Game's start, 01.05.2012 05:30 (sav.md 12).
const NEW_GAME: u64 = 63_471_533_400_000;

async fn draws(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, XGA).map_err(|e| format!("game data: {e}"))?;
    let mut assets = Assets::default();
    let fonts = assets
        .fonts(tree, &data, XGA.height)
        .map_err(|e| format!("fonts: {e}"))?;
    let mut window =
        PdaWindow::load(tree, &data, XGA, fonts).map_err(|e| format!("pda window: {e}"))?;
    let (textures, strings) = window.references();
    for id in &textures {
        ensure!(data.descriptions.get(id).is_some(), "texture id {id}");
    }
    for key in &strings {
        ensure!(data.strings.get(key) != key.as_str(), "string {key}");
    }
    window.set_new_content(2);
    window.switch_to(0);
    window.open([512.0, 384.0]);
    let r = window.layout().buttons[3].rect;
    let mut input = Input::new();
    input.handle(Event::PointerMoved {
        at: [r.x + r.width / 2.0, r.y + r.height / 2.0],
    });
    let keys = Keys::default();
    // The cursor enters and rests 3.6 s, a frame every 16 ms: the hint
    // shows after 500 ms and its animation, played once, has ended.
    for now in (16..=3616).step_by(16) {
        let out = window.update(Frame {
            data: &data,
            keys: &keys,
            game_time: NEW_GAME,
            now,
            global_ms: now,
            input: &input,
            paused: false,
            alive: true,
            player: None,
        });
        ensure!(!out.closed, "closed");
        input.end_frame();
    }
    let s = window.state();
    ensure!(s.clock == "05:30 01/05/2012", "the clock: {}", s.clock);
    ensure!(s.tabs[3].look == "h", "the contacts tab under the cursor");
    let hint = s.hint.ok_or("the contacts tab's hint")?;
    ensure!(
        hint.text == data.strings.get("pda_btn_contacts_hint"),
        "the hint: {}",
        hint.text
    );
    ensure!(hint.alpha > 0, "the hint faded in: alpha {}", hint.alpha);
    let mut ctx = Ctx {
        tree,
        data: &data,
        assets: &mut assets,
        resolution: XGA,
        surface: crate::draw::Surface::screen(XGA),
        clock_ms: 1000,
    };
    let mut scene = Scene::new(XGA);
    window
        .draw(&mut ctx, &mut scene)
        .map_err(|e| format!("draw: {e}"))?;
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let list = presenter.draw_list(&mut r, tree, &scene, 1000);
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    let pixels = capture(&mut r, &list, XGA.width, XGA.height).await?;
    cx.dump("pda", XGA.width, XGA.height, &pixels);
    Ok(String::new())
}

/// The New Game's start (sav.md 10.2, world-coordinates.md 4) on `l01_escape`.
const START: [f32; 2] = [-246.7263, -134.43387];
const START_YAW: f32 = 1.3962544;

/// Runs `window` for `ms` from `*now`, a frame every 16 ms, the cursor at
/// `cursor`, clicking there first when `click`.
fn run(
    window: &mut PdaWindow,
    data: &GameData,
    now: &mut u64,
    ms: u64,
    cursor: [f32; 2],
    click: bool,
) -> Result<(), super::Problem> {
    let mut input = Input::new();
    input.handle(Event::PointerMoved { at: cursor });
    let keys = Keys::default();
    let end = *now + ms;
    let mut frames = 0;
    while *now < end {
        *now += 16;
        frames += 1;
        if click && frames == 2 {
            for pressed in [true, false] {
                input.handle(Event::PointerButton {
                    at: None,
                    button: MouseButton::Left,
                    pressed,
                    time: *now as f64,
                });
            }
        }
        let out = window.update(Frame {
            data,
            keys: &keys,
            game_time: NEW_GAME,
            now: *now,
            global_ms: *now,
            input: &input,
            paused: false,
            alive: true,
            player: Some(MapPlayer {
                level: "l01_escape",
                position: START,
                yaw: START_YAW,
            }),
        });
        ensure!(!out.closed, "closed");
        input.end_frame();
    }
    Ok(())
}

/// Draws `window` and keeps the frame as `name`.
async fn draw_frame(
    cx: &Context<'_>,
    window: &PdaWindow,
    data: &GameData,
    assets: &mut Assets,
    name: &str,
) -> Outcome {
    let mut ctx = Ctx {
        tree: cx.tree,
        data,
        assets,
        resolution: XGA,
        surface: crate::draw::Surface::screen(XGA),
        clock_ms: 1000,
    };
    let mut scene = Scene::new(XGA);
    window
        .draw(&mut ctx, &mut scene)
        .map_err(|e| format!("draw: {e}"))?;
    ensure!(
        scene.first("pda map global").is_some(),
        "{name}: the global picture drawn"
    );
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let list = presenter.draw_list(&mut r, cx.tree, &scene, 1000);
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{name}: {errors:?}");
    let pixels = capture(&mut r, &list, XGA.width, XGA.height).await?;
    cx.dump(name, XGA.width, XGA.height, &pixels);
    Ok(String::new())
}

async fn map_draws(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, XGA).map_err(|e| format!("game data: {e}"))?;
    let mut assets = Assets::default();
    let fonts = assets
        .fonts(tree, &data, XGA.height)
        .map_err(|e| format!("fonts: {e}"))?;
    let mut window =
        PdaWindow::load(tree, &data, XGA, fonts).map_err(|e| format!("pda window: {e}"))?;
    let map = window.map().ok_or("no map")?;
    let levels = map.maps().levels.len();
    ensure!(levels == 17, "17 level maps: {levels}");
    let (textures, strings) = map.layout().references();
    for id in &textures {
        ensure!(data.descriptions.get(id).is_some(), "texture id {id}");
    }
    for key in &strings {
        ensure!(data.strings.get(key) != key.as_str(), "string {key}");
    }
    let frame = map.layout().frame;
    let tool = map.layout().tools[GLOBAL_BUTTON].rect;
    let escape = map.maps().levels[map.maps().level("l01_escape").ok_or("no Escape")?].global;
    let centre = [frame.x + frame.width / 2.0, frame.y + frame.height / 2.0];
    window.switch_to(MAP_TAB);
    window.open(centre);
    // The flight and more, the cursor resting on the player's mark.
    let mut now = 0;
    run(&mut window, &data, &mut now, 9000, centre, false)?;
    // A move over the mark names its level (pda-map.md 7.1).
    let nudged = [centre[0] + 1.0, centre[1]];
    run(&mut window, &data, &mut now, 100, nudged, false)?;
    let s = window.state().map.ok_or("no map state")?;
    ensure!(!s.moving, "still moving: {s:?}");
    ensure!((s.scale - 6.0).abs() < 1e-3, "zoom {}", s.scale);
    ensure!(s.marks.len() == 2, "the player's marks: {:?}", s.marks);
    let hint = s.hint.ok_or("the mark's hint")?;
    ensure!(
        hint.text == data.strings.get("actor"),
        "the hint: {}",
        hint.text
    );
    ensure!(
        s.label.as_deref() == Some("l01_escape"),
        "label {:?}",
        s.label
    );
    draw_frame(cx, &window, &data, &mut assets, "pda-map").await?;
    // The global map button, then the cursor on Escape's picture.
    let at = [tool.x + tool.width / 2.0, tool.y + tool.height / 2.0];
    run(&mut window, &data, &mut now, 2000, at, true)?;
    let s = window.state().map.ok_or("no map state")?;
    ensure!((s.scale - s.min_zoom).abs() < 1e-3, "zoomed out: {s:?}");
    let k = s.scale;
    let on = [
        frame.x + s.picture[0] + k * (escape[0] + escape[2]) / 2.0,
        frame.y + s.picture[1] + k * (escape[1] + escape[3]) / 2.0,
    ];
    run(&mut window, &data, &mut now, 1000, on, false)?;
    let s = window.state().map.ok_or("no map state")?;
    let hint = s.hint.ok_or("the level's hint")?;
    ensure!(
        hint.text == data.strings.get("l01_escape"),
        "the hint: {}",
        hint.text
    );
    draw_frame(cx, &window, &data, &mut assets, "pda-map-global").await
}
