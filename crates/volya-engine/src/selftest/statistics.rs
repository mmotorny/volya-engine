// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The statistics overlay (docs/behavior/display-mode.md part B) over the
//! startup's first video with game data: `stat_font`'s glyphs at 0.01 × H,
//! block 2 from the top left, block 3 at x 200, the camera line at
//! (10, 600). Frame `statistics`.

use super::{Context, Outcome, Run, capture, ensure, ensure_eq};
use crate::app::App;
use crate::audio::Silent;
use crate::core::input::Input;
use crate::core::vfs::db::DbVfs;
use crate::data::GameData;
use crate::present::Presenter;
use crate::statistics_overlay::LAYER;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("the statistics overlay from game data", |c| {
    Box::pin(statistics_overlay(c))
})];

const XGA: Resolution = Resolution::new(1024, 768);

async fn statistics_overlay(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::load(tree, None, XGA).map_err(|e| format!("game data: {e}"))?;
    let mut app = App::new(data).map_err(|e| format!("app: {e}"))?;
    let empty = DbVfs::<std::io::Empty>::new();
    let input = Input::new();
    let mut gpu = cx.renderer();
    let mut presenter = Presenter::new();
    let mut frames = Vec::new();
    let mut glyphs = 0;
    // The THQ video, 2.5 s in, drawn without the overlay and with it.
    app.update(&empty, 1000.0, &input, &mut Silent);
    app.update(&empty, 3500.0, &input, &mut Silent);
    for on in [false, true] {
        app.exec(if on { "rs_stats on" } else { "rs_stats off" });
        let s = app.scene(tree, XGA).map_err(|e| format!("scene: {e}"))?;
        glyphs = s.layer(LAYER).count();
        let list = presenter.draw_list(&mut gpu, tree, &s, app.clock());
        frames.push(capture(&mut gpu, &list, 1024, 768).await?);
        let errors = presenter.take_errors();
        ensure!(errors.is_empty(), "{errors:?}");
        if on {
            // The top-left glyph: 8 pixels wide, 7.68 high (B.2).
            let first = s.first(LAYER).ok_or("no glyph")?;
            let b = first.bounds();
            ensure_eq!([b[0], b[1], b[2] - b[0]], [0.0, 0.0, 8.0]);
            ensure!((b[3] - b[1] - 7.68).abs() < 1e-3, "height {}", b[3] - b[1]);
        }
    }
    cx.dump("statistics", 1024, 768, &frames[1]);
    let changed = (frames[0].chunks(4))
        .zip(frames[1].chunks(4))
        .take(1024 * 200)
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    ensure!(changed > 0, "the overlay changes nothing at the top");
    Ok(format!(
        "{glyphs} glyphs; {changed} pixels of the top 200 rows changed"
    ))
}
