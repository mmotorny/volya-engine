// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The first level's grass and detail objects (once
//! `tests/details.rs`): drawn through the supersampled target at the
//! shipped `r__detail_density` and `r__supersample`, against the same view
//! without them (docs/behavior/detail-objects.md), with the pass's and the
//! frame's times and the GPU memory. Frames `grass` and `grass-off`.

use super::{Context, Outcome, Run, capture, ensure, now_ms};
use crate::core::camera::Angles;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::present::Presentation;
use crate::scene::Scene;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("grass from game data", |c| {
    Box::pin(grass_from_game_data(c))
})];

const SIZE: Resolution = Resolution::new(1024, 768);

pub async fn grass_from_game_data(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    let world = screen.world().clone();
    let d = world
        .details
        .as_ref()
        .ok_or("l01_escape has detail objects")?;
    ensure!(d.models.len() == 21, "{} models", d.models.len());
    ensure!(d.alpha_test, "details\\blend has Alpha-blend");
    ensure!(
        d.texture.ends_with("l01_escape\\build_details.dds"),
        "{}",
        d.texture
    );
    // The shipped user.ltx: r__detail_density 0.2, r__supersample 2.
    screen.set_detail_settings(0.2, 0.5);
    screen.set_supersample(2);
    // Outdoors west of the start, on the ground, looking along +x and
    // a little down.
    let (x, z) = (-205.0, -112.0);
    let (ground, _) = screen
        .collision()
        .floor_below(x, z, 100.0, -100.0)
        .ok_or("ground")?;
    screen.set_camera(
        [x, ground + 1.7, z],
        Angles {
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.25,
            roll: 0.0,
        },
    );
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let presentation = Presentation {
        supersample: 2,
        ..Default::default()
    };
    let instances_of = |scene: &Scene| {
        scene
            .world
            .as_ref()
            .and_then(|w| w.details.as_ref())
            .map(|d| (d.frame.instances.len(), d.frame.draws.len()))
    };
    // The first frame's pass draws nothing (TODO(#407)); the next
    // builds the whole cache and selects.
    let mut scene = screen.scene(SIZE, 0);
    ensure!(
        instances_of(&scene).is_some_and(|(i, _)| i == 0),
        "the first frame's instances: {:?}",
        instances_of(&scene)
    );
    scene.presentation = presentation;
    let list = presenter.draw_list(&mut r, tree, &scene, 0);
    capture(&mut r, &list, SIZE.width, SIZE.height).await?;
    let mut pass_times = Vec::new();
    let mut frames = Vec::new();
    for f in 0..20 {
        // The pass runs in the frame's update, before its camera pass.
        let start = now_ms();
        screen.paused(16 * (f + 1));
        pass_times.push(now_ms() - start);
        let mut scene = screen.scene(SIZE, 0);
        scene.presentation = presentation;
        frames.push(scene);
    }
    let scene = frames.pop().ok_or("frames")?;
    let (instances, draws) = instances_of(&scene).ok_or("no details drawn")?;
    ensure!(instances > 1000, "{instances} instances drawn");
    let start = now_ms();
    let list = presenter.draw_list(&mut r, tree, &scene, 0);
    let grass = capture(&mut r, &list, SIZE.width, SIZE.height).await?;
    let with = now_ms() - start;
    let mut bare = scene.clone();
    if let Some(w) = bare.world.as_mut() {
        w.details = None;
    }
    let start = now_ms();
    let list = presenter.draw_list(&mut r, tree, &bare, 0);
    let without = capture(&mut r, &list, SIZE.width, SIZE.height).await?;
    let without_t = now_ms() - start;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    // Their blended trees differ by a few pixels from run to run (#435):
    // compared, not held.
    cx.dump_compared("grass", SIZE.width, SIZE.height, &grass, false);
    cx.dump_compared("grass-off", SIZE.width, SIZE.height, &without, false);
    let changed = grass
        .chunks(4)
        .zip(without.chunks(4))
        .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(x, y)| x.abs_diff(*y) > 24))
        .count();
    let share = changed as f64 / f64::from(SIZE.width * SIZE.height);
    let cache = screen.details().ok_or("no detail cache")?;
    let note = format!(
        "{} instances in the cache ({:.1} MB held), {instances} drawn in \
         {draws} draws; the first update with the pass (whole cache) {:.1} ms, later ones {:.2} ms on average; \
         frame with readback {with:.1} ms, without {without_t:.1} ms; details' GPU buffers \
         {:.1} MiB; {:.1} % of the pixels changed",
        cache.cache.instances().count(),
        cache.cache.memory_bytes() as f64 / 1e6,
        pass_times[0],
        pass_times[1..].iter().sum::<f64>() / (pass_times.len() - 1) as f64,
        r.details_gpu_bytes() as f64 / f64::from(1 << 20),
        share * 100.0,
    );
    ensure!(
        share > 0.02,
        "grass changes {:.2} % of the picture",
        share * 100.0
    );
    Ok(note)
}
