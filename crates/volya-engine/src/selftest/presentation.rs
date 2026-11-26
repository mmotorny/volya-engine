// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Frame presentation (once in `tests/presentation.rs`, which keeps
//! section 5's verification facts): the first level's start view drawn
//! through the supersampled target, against the same view at
//! `r__supersample` 1, with frame times
//! (docs/behavior/frame-presentation.md). Frame `supersample`.

use super::{Context, Outcome, Run, capture, ensure, ensure_eq, now_ms};
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::present::Presentation;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("start view supersampled", |c| {
    Box::pin(start_view_supersampled(c))
})];

const SIZE: Resolution = Resolution::new(1024, 768);

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let se: f64 = a
        .chunks(4)
        .zip(b.chunks(4))
        .flat_map(|(x, y)| (0..3).map(move |c| (f64::from(x[c]) - f64::from(y[c])).powi(2)))
        .sum();
    10.0 * (255.0f64.powi(2) / (se / (a.len() / 4 * 3) as f64)).log10()
}

/// The start view at r__supersample 2 against 1.
pub async fn start_view_supersampled(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let mut frames = Vec::new();
    let mut notes = Vec::new();
    for supersample in [1, 2] {
        screen.set_supersample(supersample);
        let mut scene = screen.scene(SIZE, 0);
        scene.presentation = Presentation {
            supersample,
            ..Default::default()
        };
        let list = presenter.draw_list(&mut r, tree, &scene, 0);
        capture(&mut r, &list, SIZE.width, SIZE.height).await?;
        let start = now_ms();
        const FRAMES: u32 = 10;
        let mut pixels = Vec::new();
        for _ in 0..FRAMES {
            let list = presenter.draw_list(&mut r, tree, &scene, 0);
            pixels = capture(&mut r, &list, SIZE.width, SIZE.height).await?;
        }
        let time = (now_ms() - start) / f64::from(FRAMES);
        let target = r.presentation_target();
        notes.push(format!(
            "r__supersample {supersample}: target {target:?}, {time:.1} ms per frame with \
             readback"
        ));
        // At 1 the frame goes through the target only after a distortion
        // pass (3.1): the start view has the level's distorting effect.
        let want = if supersample == 1 {
            (1024, 768)
        } else {
            (1448, 1086)
        };
        ensure_eq!(target.map(|t| t.0), Some(want));
        ensure!(supersample == 1 || target.is_some_and(|t| t.1));
        frames.push(pixels);
    }
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    cx.dump("supersample", SIZE.width, SIZE.height, &frames[1]);
    // The same picture, resampled: close, not equal.
    let q = psnr(&frames[0], &frames[1]);
    notes.push(format!("PSNR of r__supersample 2 against 1: {q:.1} dB"));
    ensure!(q > 22.0 && frames[0] != frames[1], "{q:.1} dB");
    Ok(notes.join("; "))
}
