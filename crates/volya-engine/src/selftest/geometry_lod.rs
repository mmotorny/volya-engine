// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Object detail (docs/behavior/object-detail-level.md 1): the first
//! level seen from above (the draw-calls suite's overview) at
//! `r__geometry_lod` 1 and at its lowest, 0.1, which raises every size
//! threshold tenfold. Frame `geometry-lod-0.1`. Then the cost of drawing
//! detail windows new to the renderer (ogf.md 8.3, object-detail-level.md
//! 4): a sweep of the renderer's `r__geometry_lod` draws other windows at
//! each step, index ranges WebGL implementations may check on their first
//! use; the same sweep again draws only ranges drawn before.

use super::{Context, Outcome, Run, capture, ensure, now_ms};
use crate::core::camera::Angles;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use volya_engine_formats::level::{SSA_LOD_A, SSA_LOD_B};
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("object detail level", |c| Box::pin(geometry_lod(c)))];

const SIZE: Resolution = Resolution::new(1024, 768);

async fn geometry_lod(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let mut frames = Vec::new();
    let (x, z) = (-205.0, -112.0);
    let (ground, _) = screen
        .collision()
        .floor_below(x, z, 100.0, -100.0)
        .ok_or("ground")?;
    let eye = [x, ground + 60.0, z];
    let angles = Angles {
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: 0.5,
        roll: 0.0,
    };
    for lod in [1.0, 0.1] {
        // The camera pass again with the thresholds.
        screen.set_geometry_lod(lod, SSA_LOD_A, SSA_LOD_B);
        screen.set_camera(eye, angles);
        let mut scene = screen.scene(SIZE);
        scene.geometry_lod = lod;
        let list = presenter.draw_list(&mut r, tree, &scene, 0);
        frames.push(capture(&mut r, &list, SIZE.width, SIZE.height).await?);
    }
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    cx.dump("geometry-lod-0.1", SIZE.width, SIZE.height, &frames[1]);
    let differing = (frames[0].chunks(4))
        .zip(frames[1].chunks(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    ensure!(differing > 0, "r__geometry_lod 0.1 changes nothing");
    screen.set_geometry_lod(1.0, SSA_LOD_A, SSA_LOD_B);
    screen.set_camera(eye, angles);
    const STEPS: u32 = 24;
    let mut sweeps = [0.0; 2];
    for sweep in &mut sweeps {
        let start = now_ms();
        for i in 0..STEPS {
            let mut scene = screen.scene(SIZE);
            scene.geometry_lod = 0.15 + 1.05 * i as f32 / (STEPS - 1) as f32;
            let list = presenter.draw_list(&mut r, tree, &scene, 0);
            capture(&mut r, &list, SIZE.width, SIZE.height).await?;
        }
        *sweep = (now_ms() - start) / f64::from(STEPS);
    }
    let [first, again] = sweeps;
    Ok(format!(
        "r__geometry_lod 0.1 changes {differing} pixels of the overview; a sweep of \
         r__geometry_lod 0.15 … 1.2 in {STEPS} frames: {first:.1} ms per frame with readback, \
         {again:.1} ms again"
    ))
}
