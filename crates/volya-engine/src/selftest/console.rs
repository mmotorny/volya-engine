// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The console window over the title menu from game data
//! (docs/behavior/console-window.md section 4): the top half cleared to
//! (32, 32, 32), the prompt in (128, 128, 255) at *y* −0.05, the entered
//! line in yellow and the unknown command's line in red above it, the menu
//! below. Frame `console`.

use super::title_menu::{key, step};
use super::{Context, Outcome, Run, capture, ensure, ensure_eq};
use crate::app::{App, AppState};
use crate::console_window::LAYER;
use crate::core::input::{Input, Key};
use crate::data::GameData;
use crate::present::Presenter;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("the console over the title menu from game data", |c| {
    Box::pin(console_over_the_title_menu(c))
})];

const XGA: Resolution = Resolution::new(1024, 768);

async fn console_over_the_title_menu(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::load(tree, None, XGA).map_err(|e| format!("game data: {e}"))?;
    let mut app = App::new(data).map_err(|e| format!("app: {e}"))?;
    let mut input = Input::new();
    // As the title menu's check: Esc through the items to the menu.
    step(&mut app, &mut input, 1000.0);
    step(&mut app, &mut input, 3500.0);
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
    let t = 13_000.0;
    ensure!(
        matches!(app.state(), AppState::Menu { .. }),
        "not in the menu"
    );
    app.exec("bind console kGRAVE");
    key(&mut input, Key::Grave);
    step(&mut app, &mut input, t + 100.0);
    ensure!(app.console_open(), "the console key opened nothing");
    for k in [Key::X, Key::Y, Key::Z, Key::Enter] {
        key(&mut input, k);
        step(&mut app, &mut input, t + 200.0);
    }
    ensure_eq!(
        app.console().log().last().map(String::as_str),
        Some("! Unknown command:  xyz")
    );
    let s = app.scene(tree, XGA).map_err(|e| format!("scene: {e}"))?;
    let glyphs = s.layer(LAYER).count();
    let mut gpu = cx.renderer();
    let mut presenter = Presenter::new();
    let list = presenter.draw_list(&mut gpu, tree, &s, app.clock());
    let pixels = capture(&mut gpu, &list, 1024, 768).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    cx.dump("console", 1024, 768, &pixels);
    let at = |x: usize, y: usize| -> [u8; 3] {
        let p = &pixels[(y * 1024 + x) * 4..][..3];
        [p[0], p[1], p[2]]
    };
    let rows = |y0: usize, y1: usize| (y0..y1).flat_map(|y| (0..1024).map(move |x| at(x, y)));
    // The area (through the display's ramp) fills the top half.
    let area = at(1020, 4);
    ensure!(
        area.iter().all(|&c| (20..=48).contains(&c)) && area[0] == area[1] && area[1] == area[2],
        "area colour {area:?}"
    );
    let same = rows(0, 300).filter(|&p| p == area).count();
    ensure!(same > 1024 * 290, "{same} pixels of the area's colour");
    // The prompt row (364 … 383) has blue glyphs, the red line's (326 …)
    // red ones, the yellow line's (307 …) yellow ones.
    let blue = rows(364, 384).filter(|p| p[2] > 150 && p[0] < 150).count();
    let red = rows(326, 345).filter(|p| p[0] > 150 && p[1] < 80).count();
    let yellow = rows(307, 326)
        .filter(|p| p[0] > 150 && p[1] > 150 && p[2] < 80)
        .count();
    ensure!(
        blue > 50 && red > 50 && yellow > 20,
        "blue {blue}, red {red}, yellow {yellow}"
    );
    // The menu shows below it.
    let below = rows(400, 768).filter(|&p| p != area).count();
    ensure!(below > 1024 * 300, "{below} pixels of the menu below");
    Ok(format!(
        "{glyphs} sprites; blue {blue}, red {red}, yellow {yellow}"
    ))
}
