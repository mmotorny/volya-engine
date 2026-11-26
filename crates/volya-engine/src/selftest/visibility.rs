// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The camera pass's draws (docs/behavior/visibility-scope.md 5,
//! portal-fade.md): the start view in the bunker drawn with only what the
//! traversal reaches against every draw that may show, with their draw and
//! WebGL call counts and frame times; and an outdoor view's portal fade.

use std::sync::Arc;

use super::{Context, Outcome, Run, ensure, now_ms};
use crate::core::camera::Angles;
use crate::core::input::Input;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::Renderer;
use crate::scene::Scene;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("the camera pass keeps the draws", |c| {
    Box::pin(the_camera_pass_keeps_the_draws(c))
})];

const SIZE: Resolution = Resolution::new(1024, 768);

/// Frames drawn per measure, the median kept.
const FRAMES: usize = 9;

/// `scene` drawn: its pixels, the draw calls, the WebGL calls and the
/// median time to encode and read back (ms).
async fn drawn(
    cx: &Context<'_>,
    r: &mut Renderer,
    presenter: &mut Presenter,
    scene: &Scene,
) -> Result<(Vec<u8>, u32, Option<u64>, f64), super::Problem> {
    let mut times = Vec::with_capacity(FRAMES);
    let mut pixels = Vec::new();
    let (mut draws, mut gl) = (0, None);
    for _ in 0..FRAMES {
        r.take_frame_stats();
        let start = now_ms();
        let (frame, counted) = cx.gl_calls(|| {
            let list = presenter.draw_list(r, cx.tree, scene, 0);
            r.capture(&list, SIZE.width, SIZE.height, crate::render::Color::BLACK)
        });
        pixels = frame.await.map_err(|e| format!("capture: {e}"))?;
        times.push(now_ms() - start);
        draws = r.take_frame_stats().total().draws;
        gl = counted.map(|c| c.iter().map(|(_, n)| n).sum());
    }
    times.sort_by(f64::total_cmp);
    Ok((pixels, draws, gl, times[FRAMES / 2]))
}

fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .count()
}

pub async fn the_camera_pass_keeps_the_draws(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    screen.set_effects_drawn(false);
    screen.update(16, &Input::new());
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let mut notes = Vec::new();

    // The start view: the traversal's visuals only, against every draw.
    let kept = screen.scene(SIZE, 0);
    let seen = kept
        .world
        .as_ref()
        .and_then(|w| w.seen.clone())
        .ok_or("the camera pass's draws")?;
    let items = seen.items.iter().filter(|&&k| k).count();
    let objects = seen.objects.iter().filter(|&&k| k).count();
    ensure!(
        items > 0 && items < seen.items.len() / 4,
        "{items} static draws kept"
    );
    // Every draw kept, the rest of the pass's frame (the projected
    // light's receivers and cells, the billboards) as it is.
    let mut all = kept.clone();
    if let Some(w) = all.world.as_mut() {
        let mut s = (*seen).clone();
        s.items.fill(true);
        s.objects.fill(true);
        w.seen = Some(Arc::new(s));
    }
    let (on, draws, gl, t) = drawn(cx, &mut r, &mut presenter, &kept).await?;
    let (every, draws_all, gl_all, t_all) = drawn(cx, &mut r, &mut presenter, &all).await?;
    let changed = differing(&on, &every);
    cx.dump("visibility-start", SIZE.width, SIZE.height, &on);
    notes.push(format!(
        "start: {items} of {} static draws and {objects} of {} objects kept; {draws} draw calls \
         against {draws_all}, {} WebGL calls against {}, {t:.1} ms against {t_all:.1} ms a frame \
         with readback; {changed} pixels differ",
        seen.items.len(),
        seen.objects.len(),
        gl.map_or("uncounted".into(), |n| n.to_string()),
        gl_all.map_or("uncounted".into(), |n| n.to_string()),
    ));
    ensure!(draws < draws_all, "{draws} draw calls against {draws_all}");
    // What the pass leaves out is behind the bunker's walls.
    ensure!(changed < 200, "{changed} pixels differ");

    // Outdoors, looking west: small portals far off, faded.
    let (x, z) = (-205.0, -112.0);
    let (ground, _) = screen
        .collision()
        .floor_below(x, z, 100.0, -100.0)
        .ok_or("ground")?;
    let angles = Angles {
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: 0.25,
        roll: 0.0,
    };
    screen.set_camera([x, ground + 1.7, z], angles);
    screen.paused(32);
    let faded = screen.scene(SIZE, 0);
    let seen = faded
        .world
        .as_ref()
        .and_then(|w| w.seen.clone())
        .ok_or("the camera pass's draws")?;
    ensure!(!seen.fade.is_empty(), "no portal fade");
    let mut plain = faded.clone();
    if let Some(w) = plain.world.as_mut() {
        let mut s = (*seen).clone();
        s.fade.clear();
        w.seen = Some(Arc::new(s));
    }
    let (with, ..) = drawn(cx, &mut r, &mut presenter, &faded).await?;
    let (without, ..) = drawn(cx, &mut r, &mut presenter, &plain).await?;
    let fade = differing(&with, &without);
    cx.dump("visibility-fade", SIZE.width, SIZE.height, &with);
    notes.push(format!(
        "outdoors: {} fade triangles; {fade} pixels darkened",
        seen.fade.len() / 4
    ));
    ensure!(fade > 0, "the fade draws nothing");
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    Ok(notes.join("; "))
}
