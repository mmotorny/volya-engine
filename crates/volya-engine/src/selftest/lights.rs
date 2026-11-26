// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A lamp's light drawn on the first level (once part of
//! `tests/lights.rs`; docs/behavior/dynamic-lights.md,
//! hanging-lamps.md 9.3): `trader_tv_0000`'s light against the same frame
//! with `r1_dlights` off, with frame times (frames `lights` and
//! `lights-off`); and the caged lamp by the bunker's stairs, its glass and
//! its glow (glows.md; frame `lamp`).

use super::{Context, Outcome, Problem, Run, capture, ensure, now_ms};
use crate::core::camera::Angles;
use crate::core::input::Input;
use crate::core::lights::LightSettings;
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::Renderer;
use crate::scene::Scene;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[
    ("a lamp lights the level", |c| {
        Box::pin(a_lamp_lights_the_level(c))
    }),
    ("a hanging lamp and its glow", |c| {
        Box::pin(a_hanging_lamp_and_its_glow(c))
    }),
];

const SIZE: Resolution = Resolution::new(1024, 768);

/// hanging-lamps.md 9.3: `trader_tv_0000`'s main light position and
/// direction at rest.
const TV_0000: ([f32; 3], [f32; 3]) = ([-245.2665, -22.9093, -133.0806], [0.6428, 0.0, -0.7660]);

/// The view from `eye` towards `at`.
fn look(eye: [f32; 3], at: [f32; 3]) -> Angles {
    let d: Vec<f32> = (0..3).map(|i| at[i] - eye[i]).collect();
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    Angles {
        yaw: d[0].atan2(d[2]),
        pitch: (-d[1] / l).asin(),
        roll: 0.0,
    }
}

/// The median of `t`.
fn median(mut t: Vec<f64>) -> f64 {
    t.sort_by(f64::total_cmp);
    t[t.len() / 2]
}

/// A frame of `screen`: its pixels, scene, time to draw and read back
/// (ms), and light draws.
async fn frame(
    r: &mut Renderer,
    presenter: &mut Presenter,
    tree: &dyn Vfs,
    screen: &LevelScreen,
) -> Result<(Vec<u8>, Scene, f64, usize), Problem> {
    let scene = screen.scene(SIZE, 16);
    let list = presenter.draw_list(r, tree, &scene, 16);
    let start = now_ms();
    let pixels = capture(r, &list, SIZE.width, SIZE.height).await?;
    Ok((pixels, scene, now_ms() - start, r.light_draws()))
}

pub async fn a_lamp_lights_the_level(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    // trader_tv_0000's light (range 3, cone 120°) on what is in front
    // of it, seen from above and behind the set.
    let (p, d) = TV_0000;
    let at = [p[0] + 1.5 * d[0], p[1] - 0.4, p[2] + 1.5 * d[2]];
    let eye = [p[0] - 0.6 * d[0], p[1] + 0.5, p[2] - 0.6 * d[2]];
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    screen.set_effects_drawn(false);
    screen.set_camera(eye, look(eye, at));
    screen.update(16, &Input::new());
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let _ = frame(&mut r, &mut presenter, tree, &screen).await?;
    // The median of 9 frames' times.
    let mut times = Vec::new();
    for _ in 0..8 {
        times.push(frame(&mut r, &mut presenter, tree, &screen).await?.2);
    }
    let (on, scene, t, draws) = frame(&mut r, &mut presenter, tree, &screen).await?;
    times.push(t);
    let t_on = median(times);
    let passes = scene.world.as_ref().ok_or("the world")?.lights.len();
    let settings = LightSettings {
        dlights: false,
        ..LightSettings::default()
    };
    screen.set_light_settings(settings, 16);
    // The same frame again (no time passes: the glows keep their
    // fade), its light passes made anew.
    screen.paused(16);
    let mut times = Vec::new();
    for _ in 0..8 {
        times.push(frame(&mut r, &mut presenter, tree, &screen).await?.2);
    }
    let (off, scene_off, t, _) = frame(&mut r, &mut presenter, tree, &screen).await?;
    times.push(t);
    let t_off = median(times);
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    ensure!(
        scene_off
            .world
            .as_ref()
            .ok_or("the world")?
            .lights
            .is_empty()
    );
    cx.dump("lights", SIZE.width, SIZE.height, &on);
    cx.dump("lights-off", SIZE.width, SIZE.height, &off);
    let (mut brighter, mut darker) = (0, 0);
    for (a, b) in on.chunks(4).zip(off.chunks(4)) {
        let (sa, sb): (u32, u32) = (
            a[..3].iter().map(|&c| u32::from(c)).sum(),
            b[..3].iter().map(|&c| u32::from(c)).sum(),
        );
        brighter += usize::from(sa > sb + 3);
        darker += usize::from(sb > sa + 3);
    }
    let note = format!(
        "{passes} light passes drawing {draws} draws, {brighter} pixels \
         brighter, {darker} darker; frame {t_on:.1} ms with lights, {t_off:.1} ms without"
    );
    ensure!(passes >= 1);
    ensure!(brighter > 2_000, "{brighter} pixels lit");
    ensure!(darker < brighter / 50, "{darker} darker");
    Ok(note)
}

/// glows.md 7: `l01_escape`'s level glow 5 (`glow\\glow_red2`, radius
/// 0.5), at the caged lamp under the cross vault by the bunker's stairs.
const GLOW_5: [f32; 3] = [-255.72, -18.48, -127.23];

/// The texel colour of `prop\\prop_lampa_g_red2` (an 8 x 8 BC1 texture
/// of one colour), the lamp's glass with the `selflight` shader.
const GLASS: [u8; 3] = [247, 156, 57];

/// The caged lamp under the bunker's cross vault seen from below after
/// half a second (#523): its glass drawn in its texture's colour (it was
/// black on WebKit with wgpu), and its glow faded in and drawn (glows.md
/// 3 – 6), against the same frame without glows. Frame `lamp`.
pub async fn a_hanging_lamp_and_its_glow(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let p = GLOW_5;
    let eye = [p[0] + 1.3, p[1] - 1.0, p[2]];
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    screen.set_effects_drawn(false);
    screen.set_camera(eye, look(eye, p));
    let input = Input::new();
    for i in 1..=32 {
        screen.update(16 * i, &input);
    }
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let (pixels, scene, _, _) = frame(&mut r, &mut presenter, tree, &screen).await?;
    let mut bare = scene.clone();
    if let Some(w) = bare.world.as_mut() {
        w.glows.clear();
    }
    let list = presenter.draw_list(&mut r, tree, &bare, 16);
    let without = capture(&mut r, &list, SIZE.width, SIZE.height).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    cx.dump("lamp", SIZE.width, SIZE.height, &pixels);
    let world = scene.world.as_ref().ok_or("the world")?;
    let glowing = (pixels.chunks(4).zip(without.chunks(4)))
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    let brightness: Vec<u8> = world.glows.iter().map(|g| g.b).collect();
    // Points on the glass, inside the cage.
    let at = |x: usize, y: usize| {
        let i = (y * SIZE.width as usize + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let glass = [at(512, 400), at(505, 380), at(520, 410)];
    let note = format!(
        "glow sprites at brightness {brightness:?} change {glowing} pixels; glass {glass:?}"
    );
    ensure!(!brightness.is_empty(), "{note}");
    ensure!(glowing > 5_000, "{note}");
    ensure!(
        glass
            .iter()
            .all(|c| (0..3).all(|k| c[k].abs_diff(GLASS[k]) <= 8)),
        "{note}"
    );
    Ok(note)
}
