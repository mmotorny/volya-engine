// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The talk window (docs/behavior/talk-window.md) built from the game's
//! data and drawn without errors with synthetic content: a talk between two
//! synthetic characters, phrases of both, an iconed line and choices,
//! enough phrases for the log to scroll. Every texture id and string key
//! it draws exists; the entries take the worked heights (6.3, 7) in the
//! game's fonts. Frame `talk` at 1024 × 768, frame `talk-16` on a wide
//! screen (`talk_16.xml`, 10).

use super::{Context, Outcome, Run, capture, ensure};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::present::Presenter;
use crate::scene::Scene;
use crate::talk_window::{Character, Icon, Standing, TalkWindow};
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[
    ("talk window draws", |c| {
        Box::pin(draws(c, Resolution::new(1024, 768), "talk"))
    }),
    ("talk window draws on a wide screen", |c| {
        Box::pin(draws(c, Resolution::new(1366, 768), "talk-16"))
    }),
];

fn character(id: u16, name: &str, icon: Option<Icon>) -> Character {
    Character {
        id,
        name: name.into(),
        rank: "novice".into(),
        community: "stalker".into(),
        reputation: "good".into(),
        icon,
    }
}

async fn draws(cx: &Context<'_>, res: Resolution, frame: &str) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, res).map_err(|e| format!("game data: {e}"))?;
    let mut assets = Assets::default();
    let fonts = assets
        .fonts(tree, &data, res.height)
        .map_err(|e| format!("fonts: {e}"))?;
    let mut window =
        TalkWindow::load(tree, &data, res, fonts).map_err(|e| format!("talk window: {e}"))?;
    let (textures, strings) = window.layout().references();
    for id in &textures {
        ensure!(data.descriptions.get(id).is_some(), "texture id {id}");
    }
    for key in &strings {
        ensure!(data.strings.get(key) != key.as_str(), "string {key}");
    }
    let picture = Icon {
        texture: "ui\\ui_inventory2".into(),
        rect: Some([0.0, 0.0, 210.0, 218.0]),
    };
    let player = character(0, "Player", Some(picture.clone()));
    let partner = character(1, "Partner", Some(picture));
    window.set_shown(true);
    window.start(&data, &player, &partner, false);
    let lookup = |id: u16| {
        Some(Standing {
            relation: i32::from(id),
            relation_name: "neutral".into(),
            health: 1.0,
        })
    };
    window.update(&data, 1, &lookup);
    let add = |w: &mut TalkWindow, speaker: &str, text: &str| {
        w.add_phrase(&data, speaker, text)
            .map_err(|e| format!("phrase: {e}"))
    };
    add(&mut window, "Partner", "Hello, stalker.")?;
    add(&mut window, "Player", "Hello.\\nWho are you?")?;
    let s = window.state();
    // 6.3: one line of the partner 45, two of the player character 68.
    let heights: Vec<f32> = s.answers.iter().map(|e| e.height).collect();
    ensure!(heights == [45.0, 68.0], "the phrases' heights: {heights:?}");
    let icon = Icon {
        texture: "ui\\ui_common".into(),
        rect: Some([0.0, 470.0, 157.0, 48.0]),
    };
    window
        .add_iconed_line(&data, "An iconed line.", icon, None)
        .map_err(|e| format!("iconed line: {e}"))?;
    for i in 0..7 {
        add(&mut window, "Partner", &format!("Phrase {i}."))?;
    }
    window
        .set_choices(&data, &["Goodbye.", "Tell me more.\\nAbout the Zone."])
        .map_err(|e| format!("choices: {e}"))?;
    let s = window.state();
    ensure!(s.answers_bar && s.answers_scroll > 0, "the log scrolled");
    // 7: two lines 36, but one line on a wide screen (10).
    let heights: Vec<f32> = s.questions.iter().map(|e| e.height).collect();
    let two = if res.is_wide() { 18.0 } else { 36.0 };
    ensure!(heights == [18.0, two], "the choices' heights: {heights:?}");
    ensure!(window.take_news().len() == 9, "a news record per phrase");
    let mut ctx = Ctx {
        tree,
        data: &data,
        assets: &mut assets,
        resolution: res,
        surface: crate::draw::Surface::screen(res),
        clock_ms: 1000,
    };
    let mut scene = Scene::new(res);
    window
        .draw(&mut ctx, &mut scene)
        .map_err(|e| format!("draw: {e}"))?;
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let list = presenter.draw_list(&mut r, tree, &scene, 1000);
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    let pixels = capture(&mut r, &list, res.width, res.height).await?;
    cx.dump(frame, res.width, res.height, &pixels);
    Ok(String::new())
}
