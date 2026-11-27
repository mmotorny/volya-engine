// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's spawned objects on New Game, drawn (once part of
//! `tests/objects.rs`; docs/behavior/level-objects.md): the start view
//! with and without them, the changes where section 3 places them, with
//! frame times; and an offline stalker drawn once the player walks within
//! reach (offline-simulation.md 6.2). Frame `objects`.

use super::{Context, Outcome, Problem, Run, capture, ensure, ensure_eq, now_ms};
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::Renderer;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[
    ("start view shows the objects", |c| {
        Box::pin(start_view_shows_the_objects(c))
    }),
    ("walking draws an offline stalker", |c| {
        Box::pin(walking_draws_an_offline_stalker(c))
    }),
];

const SIZE: Resolution = Resolution::new(1024, 768);

/// Walking (offline-simulation.md 6.2): the stalker
/// `esc_novice_attacker1`, loaded but offline, is drawn once the player is
/// within 135 m and no longer beyond 165 m: what the presenter has the
/// renderer draw (a draw's bounding radius of 0 hides it).
pub async fn walking_draws_an_offline_stalker(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    let sim = screen.simulation().ok_or("the simulation")?;
    let o = sim
        .by_name("esc_novice_attacker1")
        .ok_or("esc_novice_attacker1")?;
    let (id, at) = (o.id, o.record.header.position);
    let objects = &screen.world().objects;
    let k = objects
        .instances
        .iter()
        .position(|i| i.id == id)
        .ok_or("esc_novice_attacker1's instance")?;
    ensure!(!objects.instances[k].online);
    let draws: Vec<(usize, usize)> = objects
        .draws
        .iter()
        .enumerate()
        .map(|(j, d)| (j, d.instance))
        .collect();
    let mut gpu = cx.renderer();
    let mut presenter = Presenter::new();
    // Per instance: whether the renderer draws any of its draws.
    let mut drawn = |screen: &LevelScreen| -> Result<Vec<bool>, Problem> {
        let list = presenter.draw_list(&mut gpu, tree, &screen.scene(SIZE), 0);
        let world = list.world.ok_or("the world")?.world;
        let mut shown = vec![false; screen.world().objects.instances.len()];
        for &(j, i) in &draws {
            let sphere = world.draw_sphere(presenter.object_draw_index(j));
            shown[i] |= sphere.is_some_and(|s| s[3] > 0.0);
        }
        Ok(shown)
    };
    // From the first frame, only the online instances are drawn.
    let first = drawn(&screen)?;
    let with_draws: Vec<usize> = draws.iter().map(|d| d.1).collect();
    for (i, instance) in screen.world().objects.instances.iter().enumerate() {
        if with_draws.contains(&i) {
            ensure_eq!(first[i], instance.online, "{}", instance.name);
        }
    }
    ensure!(!first[k]);
    let mut now = 0;
    let mut walk = |screen: &mut LevelScreen, d: f32| {
        screen.set_camera([at[0] - d, at[1] + 1.6, at[2]], Default::default());
        now += 16;
        screen.update(now, &Default::default());
    };
    walk(&mut screen, 130.0);
    ensure!(drawn(&screen)?[k], "not drawn at 130 m");
    walk(&mut screen, 160.0);
    ensure!(drawn(&screen)?[k], "not drawn at 160 m");
    walk(&mut screen, 170.0);
    ensure!(!drawn(&screen)?[k], "drawn at 170 m");
    Ok(String::new())
}

/// Projects world point `p` to pixel coordinates of a `SIZE` frame, if in
/// front of the camera.
fn project(view: &[[f32; 4]; 4], projection: &[[f32; 4]; 4], p: [f32; 3]) -> Option<(f32, f32)> {
    let row = |v: [f32; 4], m: &[[f32; 4]; 4]| -> [f32; 4] {
        std::array::from_fn(|c| (0..4).map(|k| v[k] * m[k][c]).sum())
    };
    let clip = row(row([p[0], p[1], p[2], 1.0], view), projection);
    if clip[3] <= 0.0 {
        return None;
    }
    let (x, y) = (clip[0] / clip[3], clip[1] / clip[3]);
    Some((
        (x + 1.0) / 2.0 * SIZE.width as f32,
        (1.0 - y) / 2.0 * SIZE.height as f32,
    ))
}

/// The start view, its objects hidden or not: the screen, the frame and a
/// note of the frame time and the objects' GPU memory.
async fn start_view(
    cx: &Context<'_>,
    data: &GameData,
    hide: bool,
) -> Result<(LevelScreen, Vec<u8>, String), Problem> {
    let tree = cx.tree;
    let mut screen = LevelScreen::load(tree, data, 0).map_err(|e| format!("level: {e}"))?;
    if hide {
        ensure!(screen.hide_objects());
    }
    let mut gpu: Renderer = cx.renderer();
    let mut presenter = Presenter::new();
    let scene = screen.scene(SIZE);
    let list = presenter.draw_list(&mut gpu, tree, &scene, 0);
    let pixels = capture(&mut gpu, &list, SIZE.width, SIZE.height).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    let world = presenter.world().ok_or("the world")?.clone();
    const FRAMES: u32 = 10;
    let start = now_ms();
    for _ in 0..FRAMES {
        let list = presenter.draw_list(&mut gpu, tree, &scene, 0);
        capture(&mut gpu, &list, SIZE.width, SIZE.height).await?;
    }
    let frame = (now_ms() - start) / f64::from(FRAMES);
    let mib = |b: u64| b as f64 / f64::from(1 << 20);
    let note = format!(
        "objects {}: {frame:.1} ms per frame with readback; object draws {}, GPU buffers \
         {:.2} MiB, textures {:.2} MiB (all: {:.1} / {:.1} MiB)",
        if hide { "hidden" } else { "shown" },
        world.object_draw_count(),
        mib(world.object_gpu_bytes()),
        mib(world.object_texture_bytes()),
        mib(world.gpu_bytes()),
        mib(world.texture_bytes()),
    );
    Ok((screen, pixels, note))
}

/// The start view with and without the objects: they show where section 3
/// places them. Frame times are reported, not checked.
pub async fn start_view_shows_the_objects(cx: &Context<'_>) -> Outcome {
    let data = GameData::read(cx.tree, None, SIZE).map_err(|e| format!("game data: {e}"))?;
    let (screen, with, shown) = start_view(cx, &data, false).await?;
    let (_, without, hidden) = start_view(cx, &data, true).await?;
    cx.dump("objects", SIZE.width, SIZE.height, &with);
    let differs = |x: usize, y: usize| {
        let i = (y * SIZE.width as usize + x) * 4;
        with[i..i + 3]
            .iter()
            .zip(&without[i..i + 3])
            .any(|(a, b)| a.abs_diff(*b) > 8)
    };
    let changed = (0..SIZE.height as usize)
        .flat_map(|y| (0..SIZE.width as usize).map(move |x| (x, y)))
        .filter(|&(x, y)| differs(x, y))
        .count();
    let mut note = format!(
        "{shown}; {hidden}; {:.1} % of the start view changed by the objects",
        100.0 * changed as f64 / f64::from(SIZE.width * SIZE.height)
    );
    // The changes lie where the objects are: within the screen square of
    // each one's world sphere. The trader (behind his counter, his upper
    // body in view) and the lamp on the counter, of section 7.2, show;
    // every changed pixel is covered by the square of an object within
    // the view.
    let (view, projection) = screen.view_projection(SIZE.width, SIZE.height);
    let eye = screen.state().eye;
    let focal = projection[1][1] * SIZE.height as f32 / 2.0;
    let mut squares = Vec::new();
    for i in &screen.world().objects.instances {
        let c = [i.sphere[0], i.sphere[1], i.sphere[2]];
        let d =
            ((c[0] - eye[0]).powi(2) + (c[1] - eye[1]).powi(2) + (c[2] - eye[2]).powi(2)).sqrt();
        let Some((x, y)) = project(&view, &projection, c) else {
            continue;
        };
        // Conservative: the sphere's radius at its centre's distance,
        // less its radius, plus a pixel.
        let r = focal * i.sphere[3] / (d - i.sphere[3]).max(0.1) + 1.0;
        squares.push((i.name.clone(), x - r, y - r, x + r, y + r));
    }
    let changed_in = |s: &(String, f32, f32, f32, f32)| {
        let (x0, y0) = (s.1.max(0.0) as usize, s.2.max(0.0) as usize);
        let x1 = (s.3.max(0.0) as usize).min(SIZE.width as usize - 1);
        let y1 = (s.4.max(0.0) as usize).min(SIZE.height as usize - 1);
        (y0..=y1)
            .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
            .filter(|&(x, y)| differs(x, y))
            .count()
    };
    for name in ["escape_trader", "trader_light_0003"] {
        let s = squares.iter().find(|s| s.0 == name).ok_or(name)?;
        let n = changed_in(s);
        note += &format!("; {name}: {n} pixels changed in its square");
        ensure!(n > 500, "{name}: {n} pixels changed");
    }
    let outside = (0..SIZE.height as usize)
        .flat_map(|y| (0..SIZE.width as usize).map(move |x| (x, y)))
        .filter(|&(x, y)| differs(x, y))
        .filter(|&(x, y)| {
            let (x, y) = (x as f32, y as f32);
            !squares
                .iter()
                .any(|s| x >= s.1 && x <= s.3 && y >= s.2 && y <= s.4)
        })
        .count();
    ensure_eq!(outside, 0, "changed pixels outside every object");
    ensure!(changed > 1000, "{changed} pixels changed");
    Ok(note)
}
