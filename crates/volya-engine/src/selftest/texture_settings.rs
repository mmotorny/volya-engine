// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Texture quality and filtering (docs/behavior/texture-quality-
//! filtering.md): the first level's start view at the shipped settings,
//! with the engine's values a level starts with (maximum anisotropy 4, mip
//! bias −0.5, 8.2), and at `texture_lod` 4. Frame `texture-lod-4`.

use super::{Context, Outcome, Run, capture, ensure};
use crate::core::texture_quality::TextureQuality;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::world::Filtering;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[("texture quality and filtering", |c| {
    Box::pin(texture_settings(c))
})];

const SIZE: Resolution = Resolution::new(1024, 768);

fn mib(bytes: u64) -> f64 {
    bytes as f64 / f64::from(1 << 20)
}

/// Pixels whose colour differs between `a` and `b`.
fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(x, y)| x[..3] != y[..3])
        .count()
}

async fn texture_settings(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let list = TextureQuality::read_list(&data.system);
    let mut r = cx.renderer();
    let mut frames = Vec::new();
    let mut bytes = Vec::new();
    // (texture_lod, filtering): the shipped values, a level's start
    // (8.2), the lowest quality.
    let views = [
        (0, Filtering::default()),
        (
            0,
            Filtering {
                anisotropy: 4,
                mip_bias: -0.5,
            },
        ),
        (4, Filtering::default()),
    ];
    for (lod, filtering) in views {
        // A level and presenter of its own: the level's textures load
        // again.
        let screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
        let mut presenter = Presenter::new();
        let mut scene = screen.scene(SIZE, 0);
        scene.filtering = filtering;
        scene.texture_quality = TextureQuality::new(lod, list.clone());
        let list = presenter.draw_list(&mut r, tree, &scene, 0);
        let pixels = capture(&mut r, &list, SIZE.width, SIZE.height).await?;
        let errors = presenter.take_errors();
        ensure!(errors.is_empty(), "{errors:?}");
        bytes.push(presenter.world().map_or(0, |w| w.texture_bytes()));
        frames.push(pixels);
    }
    cx.dump("texture-lod-4", SIZE.width, SIZE.height, &frames[2]);
    let filtered = differing(&frames[0], &frames[1]);
    let reduced = differing(&frames[0], &frames[2]);
    ensure!(filtered > 0, "anisotropy 4 and bias −0.5 change nothing");
    ensure!(reduced > 0, "texture_lod 4 changes nothing");
    ensure!(bytes[2] < bytes[0], "texture_lod 4: {:?}", bytes);
    Ok(format!(
        "anisotropy 4 and mip bias −0.5 change {filtered} pixels; texture_lod 4 changes \
         {reduced}, textures {:.1} MiB against {:.1}",
        mib(bytes[2]),
        mib(bytes[0])
    ))
}
