// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! An artefact's idle effect drawn on the first level (once part of
//! `tests/artefacts.rs`; docs/behavior/artefact-idle-effects.md):
//! `esc_af_medusa`'s effect against the same view without the artefacts'
//! effects. Frames `artefact` and `artefact-off`.

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

pub const CHECKS: &[(&str, Run)] = &[("an artefact's effect is drawn", |c| {
    Box::pin(an_artefacts_effect_is_drawn(c))
})];

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

/// `esc_af_medusa`'s effect (`anomaly2\artefact\artefact_gravi`) drawn at
/// its effect point: a frame with the artefacts' effects differs from one
/// without them around it.
pub async fn an_artefacts_effect_is_drawn(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    let at = screen
        .artefacts()
        .artefacts()
        .first()
        .ok_or("an artefact online")?
        .point();
    // 1.5 m above it, 0.5 m to its −z, looking down at it.
    let eye = [at[0], at[1] + 1.5, at[2] - 0.5];
    let angles = Angles {
        yaw: 0.0,
        pitch: 1.25,
        roll: 0.0,
    };
    for t in 0..120u64 {
        screen.set_camera(eye, angles);
        screen.update(16 * (t + 1), &Input::new());
    }
    // How bright the sprites are at a moment follows the shared generator,
    // which the camera pass and the detail objects draw from too (86 –
    // 20,662 pixels over revisions, 0 in some): the first of up to 60
    // frames where they show.
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let mut t = 120u64;
    let (on, off, scene, centre, inside, outside) = loop {
        screen.set_camera(eye, angles);
        screen.set_effect_origins_drawn(&[Origin::Artefact]);
        let (on, scene) = frame(&mut r, &mut presenter, tree, &screen).await?;
        ensure!(
            !scene
                .world
                .as_ref()
                .ok_or("the world")?
                .particles
                .is_empty()
        );
        screen.set_effects_drawn(false);
        let (off, _) = frame(&mut r, &mut presenter, tree, &screen).await?;
        let errors = presenter.take_errors();
        ensure!(errors.is_empty(), "{errors:?}");
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
            // Faint differences spread over the whole frame (about 6,000
            // pixels by more than 8, measured): what the sprites draw
            // differs by more.
            if (0..3).any(|c| a[c].abs_diff(b[c]) > 24) {
                let (x, y) = (
                    (i as u32 % SIZE.width) as f32,
                    (i as u32 / SIZE.width) as f32,
                );
                if (centre[0] - x).hypot(centre[1] - y) <= 300.0 {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
        if inside > 40 || t >= 180 {
            break (on, off, scene, centre, inside, outside);
        }
        t += 1;
        screen.set_effects_drawn(true);
        screen.update(16 * t, &Input::new());
    };
    cx.dump("artefact", SIZE.width, SIZE.height, &on);
    cx.dump("artefact-off", SIZE.width, SIZE.height, &off);
    let draws = &scene.world.as_ref().ok_or("the world")?.particles;
    let particles: usize = draws.iter().map(|d| d.particles).sum();
    let note = format!(
        "{} draws, {particles} sprites; {inside} pixels differ around the artefact at \
         {centre:?}, {outside} elsewhere, frame {t}",
        draws.len()
    );
    // No frame of the original can check the brightness: from its first
    // frame the original pushes this jumping artefact off its record
    // position, to where is not established (artefact-idle-effects.md 6.4,
    // 8; #514).
    ensure!(inside > 40, "{inside} pixels differ");
    ensure!(outside * 10 < inside, "{outside} elsewhere");
    Ok(note)
}
