// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's torch drawn on a New Game (once part of `tests/torch.rs`;
//! docs/behavior/torch.md): the first level's start lit by it, switched on
//! with its key, against the same view unlit. Frames `torch` and
//! `torch-off`.

use super::{Context, Outcome, Problem, Run, capture, ensure, ensure_eq};
use crate::core::input::{Event, Input, Key};
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::Renderer;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("the torch lights the level", |c| {
    Box::pin(the_torch_lights_the_level(c))
})];

const SIZE: Resolution = Resolution::new(1024, 768);

fn press(level: &mut LevelScreen, now: &mut u64, key: Key) {
    let mut input = Input::new();
    input.handle(Event::Key { key, pressed: true });
    *now += 16;
    level.update(*now, &input);
}

/// Frames without input until `now` + `ms`.
fn run(level: &mut LevelScreen, now: &mut u64, ms: u64) {
    let end = *now + ms;
    while *now < end {
        *now += 16;
        level.update(*now, &Input::new());
    }
}

/// A frame of `level` at `now`: its pixels and light passes.
async fn frame(
    r: &mut Renderer,
    presenter: &mut Presenter,
    tree: &dyn Vfs,
    level: &LevelScreen,
    now: u64,
) -> Result<(Vec<u8>, usize), Problem> {
    let scene = level.scene(SIZE);
    let list = presenter.draw_list(r, tree, &scene, now);
    let pixels = capture(r, &list, SIZE.width, SIZE.height).await?;
    Ok((
        pixels,
        scene.world.as_ref().ok_or("the world")?.lights.len(),
    ))
}

pub async fn the_torch_lights_the_level(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let mut level = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    level.set_effects_drawn(false);
    let mut now = 0;
    press(&mut level, &mut now, Key::L);
    run(&mut level, &mut now, 1000);
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let _ = frame(&mut r, &mut presenter, tree, &level, now).await?;
    let (lit, lit_passes) = frame(&mut r, &mut presenter, tree, &level, now).await?;
    // Off with L: one frame later, the same view.
    press(&mut level, &mut now, Key::L);
    ensure!(!level.state().torch.ok_or("the torch")?.on);
    let _ = frame(&mut r, &mut presenter, tree, &level, now).await?;
    let (unlit, unlit_passes) = frame(&mut r, &mut presenter, tree, &level, now).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    cx.dump("torch", SIZE.width, SIZE.height, &lit);
    cx.dump("torch-off", SIZE.width, SIZE.height, &unlit);
    let (mut brighter, mut darker) = (0, 0);
    for (a, b) in lit.chunks(4).zip(unlit.chunks(4)) {
        let (sa, sb): (u32, u32) = (
            a[..3].iter().map(|&c| u32::from(c)).sum(),
            b[..3].iter().map(|&c| u32::from(c)).sum(),
        );
        brighter += usize::from(sa > sb + 3);
        darker += usize::from(sb > sa + 3);
    }
    let note = format!(
        "{lit_passes} light passes lit, {unlit_passes} unlit; \
         {brighter} pixels brighter, {darker} darker"
    );
    ensure_eq!(lit_passes, unlit_passes + 1);
    ensure!(brighter > 20_000, "{brighter} pixels lit");
    ensure!(darker < brighter / 50, "{darker} darker");
    Ok(note)
}
