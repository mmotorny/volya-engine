// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The PDA window (docs/behavior/pda-window.md) built from the game's
//! data and drawn without errors: on the tasks tab at the New Game's start,
//! the diary's new-content mark set and the cursor resting on the contacts
//! tab, its hint shown (pda-controls.md 6). Every texture id and string
//! key it draws exists. Frame `pda`.

use super::{Context, Outcome, Run, capture, ensure};
use crate::core::input::{Event, Input};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::pda_window::{Frame, Keys, PdaWindow};
use crate::present::Presenter;
use crate::scene::Scene;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("pda window draws", |c| Box::pin(draws(c)))];

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
