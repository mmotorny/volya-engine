// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A zone's idle effect drawn on the first level (once part of
//! `tests/zones.rs`; docs/behavior/zone-idle-effects.md,
//! particle-drawing.md): a campfire's effect against the same view without
//! the zones' effects. Frames `campfire` and `campfire-off`.

use super::{Context, Outcome, Problem, Run, capture, ensure};
use crate::core::camera::Angles;
use crate::core::input::Input;
use crate::core::particles::placed::Origin;
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::Renderer;
use crate::scene::Scene;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("a campfire is drawn", |c| Box::pin(a_campfire_is_drawn(c)))];

const SIZE: Resolution = Resolution::new(1024, 768);

/// A frame of `screen` at 2 s: its pixels and scene.
async fn frame(
    r: &mut Renderer,
    presenter: &mut Presenter,
    tree: &dyn Vfs,
    screen: &LevelScreen,
) -> Result<(Vec<u8>, Scene), Problem> {
    let scene = screen.scene(SIZE, 2000);
    let list = presenter.draw_list(r, tree, &scene, 2000);
    let pixels = capture(r, &list, SIZE.width, SIZE.height).await?;
    Ok((pixels, scene))
}

/// A campfire's effect (`zone_flame_small`, `explosions\campfire_05`)
/// drawn at its zone's position: a frame with the zones' effects differs
/// from one without them around the fire.
pub async fn a_campfire_is_drawn(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    let fire = (screen.zones_report().zones.into_iter())
        .find(|z| z.section == "zone_flame_small")
        .ok_or("a zone_flame_small")?;
    let at = fire.position;
    // 3 m above the fire, 1 m to its −z, looking down at it.
    let eye = [at[0], at[1] + 3.0, at[2] - 1.0];
    let angles = Angles {
        yaw: 0.0,
        pitch: 1.25,
        roll: 0.0,
    };
    for t in 0..120u64 {
        screen.set_camera(eye, angles);
        screen.update(16 * (t + 1), &Input::new());
    }
    screen.set_camera(eye, angles);
    screen.set_effect_origins_drawn(&[Origin::Zone]);
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let (on, scene) = frame(&mut r, &mut presenter, tree, &screen).await?;
    let draws = &scene.world.as_ref().ok_or("the world")?.particles;
    ensure!(!draws.is_empty());
    screen.set_effects_drawn(false);
    let (off, _) = frame(&mut r, &mut presenter, tree, &screen).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    ensure!(r.particle_buffer_size() >= screen.world().particle_capacity as u64 * 144);
    cx.dump("campfire", SIZE.width, SIZE.height, &on);
    cx.dump("campfire-off", SIZE.width, SIZE.height, &off);
    // Where the fire projects: within 200 px of its position on screen.
    let (view, projection) = screen.view_projection(SIZE.width, SIZE.height);
    let vp = crate::render::world::multiply(&view, &projection);
    let clip: Vec<f32> = (0..4)
        .map(|c| at[0] * vp[0][c] + at[1] * vp[1][c] + at[2] * vp[2][c] + vp[3][c])
        .collect();
    let centre = [
        (clip[0] / clip[3] * 0.5 + 0.5) * SIZE.width as f32,
        (0.5 - clip[1] / clip[3] * 0.5) * SIZE.height as f32,
    ];
    let (mut inside, mut outside) = (0, 0);
    for (i, (a, b)) in on.chunks(4).zip(off.chunks(4)).enumerate() {
        // The fire's heat haze recomposes the whole frame through its
        // distortion target (particle-drawing.md 6.3), moving every
        // pixel by a little: what the sprites draw differs by more.
        if (0..3).any(|c| a[c].abs_diff(b[c]) > 24) {
            let (x, y) = (
                (i as u32 % SIZE.width) as f32,
                (i as u32 / SIZE.width) as f32,
            );
            if (centre[0] - x).hypot(centre[1] - y) <= 250.0 {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    let particles: usize = draws.iter().map(|d| d.particles).sum();
    let note = format!(
        "{} draws, {particles} sprites; {inside} pixels differ around the fire at {centre:?}, \
         {outside} elsewhere",
        draws.len()
    );
    ensure!(inside > 500, "{inside} pixels differ");
    ensure!(outside * 10 < inside, "{outside} elsewhere");
    Ok(note)
}
