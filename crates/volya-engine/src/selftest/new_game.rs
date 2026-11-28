// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game's drawing (once in `tests/new_game.rs`, which keeps what
//! needs none): the start view (docs/behavior/view-projection.md 2.3) with the
//! frame times, an outdoor view with the sky (docs/behavior/sky.md), the
//! loading screen's and the intro's frames (docs/behavior/loading-screen.md),
//! the texture paths against each other, the loading taking prefetched and prepared
//! textures (#113, #184), and the level's placed effect drawn
//! (docs/behavior/particle-drawing.md). Frames `level`, `outdoor`,
//! `loading`, `intro`, `effects` and `effects-off`.

use std::collections::BTreeSet;

use super::{Context, Outcome, Problem, Run, capture, ensure, ensure_eq, now_ms};
use crate::app::{App, AppState};
use crate::audio::Silent;
use crate::core::camera::Angles;
use crate::core::input::{Event, Input, Key};
use crate::core::particles::placed::Origin;
use crate::core::vfs::{self, Vfs};
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::present::Presenter;
use crate::render::gl::Owner;
use crate::render::{Renderer, TexturePath};
use crate::scene::Scene;
use volya_engine_formats::dds::Dds;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[
    ("new game start view", |c| Box::pin(start_view(c))),
    ("loading screen and intro frames", |c| {
        Box::pin(loading_screen_frames(c))
    }),
    ("texture paths match", |c| Box::pin(texture_paths_match(c))),
    ("loading takes prepared textures", |c| {
        Box::pin(loading_takes_prepared_textures(c))
    }),
    ("placed effects are drawn", |c| {
        Box::pin(placed_effects_are_drawn(c))
    }),
];

const SIZE: Resolution = Resolution::new(1024, 768);

fn game_data(tree: &dyn Vfs) -> Result<GameData, Problem> {
    Ok(GameData::read(tree, None, SIZE).map_err(|e| format!("game data: {e}"))?)
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / f64::from(1 << 20)
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let se: f64 = a
        .chunks(4)
        .zip(b.chunks(4))
        .flat_map(|(x, y)| (0..3).map(move |c| (f64::from(x[c]) - f64::from(y[c])).powi(2)))
        .sum();
    10.0 * (255.0f64.powi(2) / (se / (a.len() / 4 * 3) as f64)).log10()
}

/// `scene` drawn and read back.
async fn draw(
    r: &mut Renderer,
    presenter: &mut Presenter,
    tree: &dyn Vfs,
    scene: &Scene,
    now: u64,
) -> Result<Vec<u8>, Problem> {
    let list = presenter.draw_list(r, tree, scene, now);
    capture(r, &list, SIZE.width, SIZE.height).await
}

/// The start view: the world uploaded and covering the screen in many
/// colours; an outdoor view above the start with terrain, trees, fog and
/// the sky; and at 06:04 the sky drawn from the next hour's cube maps.
pub async fn start_view(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = game_data(tree)?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;

    let mut gpu = cx.renderer();
    let mut presenter = Presenter::new();
    let scene = screen.scene(SIZE);
    let start = now_ms();
    let pixels = draw(&mut gpu, &mut presenter, tree, &scene, 0).await?;
    let first = now_ms() - start;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    let world = presenter.world().ok_or("world uploaded")?.clone();
    ensure!(
        screen.world().objects.take_geometry().is_none(),
        "CPU copy of the objects released"
    );

    // Frame time: the same view again, the world already on the GPU.
    let start = now_ms();
    const FRAMES: u32 = 10;
    for _ in 0..FRAMES {
        draw(&mut gpu, &mut presenter, tree, &scene, 0).await?;
    }
    let frame = (now_ms() - start) / f64::from(FRAMES);
    let level = screen.world();
    let mut note = format!(
        "first frame (upload) {first:.1} ms, then {frame:.1} ms per frame with readback; \
         GPU buffers {:.1} MiB, textures {:.1} MiB; {} draws, {} skipped; {} surfaces; \
         not drawn: {:?}",
        mib(world.gpu_bytes()),
        mib(world.texture_bytes()),
        world.draw_count(),
        world.skipped(),
        level.surfaces.len(),
        level.not_drawn
    );
    ensure_eq!(world.skipped(), 0);
    cx.dump("level", SIZE.width, SIZE.height, &pixels);

    // The start view is inside the trader's bunker: the world covers the
    // screen (the clear colour shows nowhere), in many colours.
    let clear = scene.world.as_ref().ok_or("world")?.environment.fog_rgb();
    let clear = clear.map(|c| (c * 255.0).round() as u8);
    let total = (SIZE.width * SIZE.height) as usize;
    let background = pixels
        .chunks(4)
        .filter(|p| p[..3].iter().zip(clear).all(|(a, b)| a.abs_diff(b) <= 1))
        .count();
    let distinct: BTreeSet<&[u8]> = pixels.chunks(4).collect();
    note += &format!(
        "; background {:.1} %, {} colours",
        100.0 * background as f64 / total as f64,
        distinct.len()
    );
    ensure!(background * 100 < total, "world everywhere");
    ensure!(distinct.len() > 500, "{} colours", distinct.len());

    // Outdoors, above the start: terrain, trees, fog and the sky.
    let (eye, angles) = (
        [-180.0, 5.0, -120.0],
        Angles {
            yaw: 1.2,
            pitch: 0.15,
            roll: 0.0,
        },
    );
    screen.set_camera(eye, angles);
    let scene = screen.scene(SIZE);
    let pixels = draw(&mut gpu, &mut presenter, tree, &scene, 5000).await?;
    // Its blended trees differ by a few pixels from run to run (#435):
    // compared, not held.
    cx.dump_compared("outdoor", SIZE.width, SIZE.height, &pixels, false);
    let distinct: BTreeSet<&[u8]> = pixels.chunks(4).collect();
    ensure!(distinct.len() > 500, "outdoors: {} colours", distinct.len());
    // The sky (docs/behavior/sky.md) covers the top rows: varied, and not
    // the clear colour.
    let top = &pixels[..(SIZE.width * 16 * 4) as usize];
    let cleared = top
        .chunks(4)
        .filter(|p| p[..3].iter().zip(clear).all(|(a, b)| a.abs_diff(b) <= 1))
        .count();
    let sky: BTreeSet<&[u8]> = top.chunks(4).collect();
    ensure!(
        cleared * 100 < top.len() / 4,
        "{cleared} cleared sky pixels"
    );
    ensure!(sky.len() > 50, "{} sky colours", sky.len());

    // The game clock (sav.md sections 5, 15): 205 real seconds at ×10
    // later it is 06:04:10, and the sky blends the next hour's pair.
    screen.update(205_000, &Input::new());
    screen.set_camera(eye, angles);
    let mut scene = screen.scene(SIZE);
    let rows = (SIZE.width * 16 * 4) as usize;
    let at_six = draw(&mut gpu, &mut presenter, tree, &scene, 205_000).await?[..rows].to_vec();
    // The frame binds the cube maps of [default_weather_06] and _07 (index
    // 6 of the cycle in time order, from 00:00) ...
    let sky = scene
        .world
        .as_mut()
        .and_then(|w| w.sky.as_mut())
        .ok_or("sky")?;
    ensure_eq!(sky.section, 6);
    // ... and the same frame with the 05:30 pair bound instead (sky colour,
    // blend weight and view unchanged) shows another sky, so the drawn sky
    // comes from the next hour's cube maps.
    sky.section = 5;
    let with_five = draw(&mut gpu, &mut presenter, tree, &scene, 205_000).await?[..rows].to_vec();
    let differing = at_six
        .chunks(4)
        .zip(with_five.chunks(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
        .count();
    note += &format!(
        "; 06:04: {differing} of {} sky pixels differ",
        at_six.len() / 4
    );
    ensure!(
        differing * 2 > at_six.len() / 4,
        "06:04: {differing} of {} sky pixels differ from the 05:00 pair's",
        at_six.len() / 4
    );
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    Ok(note)
}

fn press(input: &mut Input, key: Key) {
    input.handle(Event::Key { key, pressed: true });
    input.handle(Event::Key {
        key,
        pressed: false,
    });
}

/// An app from the title screen's start to a New Game chosen, and its
/// clock.
fn new_game(tree: &dyn Vfs, data: GameData, prepare_textures: bool) -> Result<Game, Problem> {
    let mut game = Game {
        app: App::new(data).map_err(|e| format!("app: {e}"))?,
        input: Input::new(),
        t: 0.0,
    };
    game.app.set_prepare_textures(prepare_textures);
    for _ in 0..20 {
        if matches!(game.app.state(), AppState::Menu { .. }) {
            break;
        }
        press(&mut game.input, Key::Escape);
        game.step(tree);
    }
    ensure!(matches!(game.app.state(), AppState::Menu { .. }));
    for _ in 0..2 {
        press(&mut game.input, Key::Enter);
        game.step(tree);
    }
    Ok(game)
}

struct Game {
    app: App,
    input: Input,
    t: f64,
}

impl Game {
    /// One 16 ms frame, sound off.
    fn step(&mut self, tree: &dyn Vfs) {
        self.t += 16.0;
        self.app.update(tree, self.t, &mut self.input, &mut Silent);
        self.input.end_frame();
    }
}

/// docs/behavior/loading-screen.md: a first New Game's loading screen at
/// its fifth caption, with the level's picture (4.5), and the intro's
/// video over the paused level (section 7) render without errors.
pub async fn loading_screen_frames(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let mut data = game_data(tree)?;
    data.measure_intro(tree)
        .map_err(|e| format!("intro: {e}"))?;
    // The logo sequence is skipped; its lengths do not matter here.
    data.intro_lengths = vec![0.0; data.intro.items.len()];
    let mut game = new_game(tree, data, false)?;
    let mut gpu = cx.renderer();
    let mut presenter = Presenter::new();
    // The 17 captions, one a frame; the fifth drawn.
    for i in 0..17 {
        game.step(tree);
        ensure!(
            matches!(
                game.app.state(),
                AppState::Loading {
                    loading: Some(_),
                    ..
                }
            ),
            "caption {}: {:?}",
            i + 1,
            game.app.state()
        );
        if i == 4 {
            let scene = game
                .app
                .scene(tree, SIZE)
                .map_err(|e| format!("scene: {e}"))?;
            let pixels = draw(&mut gpu, &mut presenter, tree, &scene, 0).await?;
            cx.dump("loading", SIZE.width, SIZE.height, &pixels);
            let errors = presenter.take_errors();
            ensure!(errors.is_empty(), "{errors:?}");
        }
    }
    // The level under the loading image's 30 frames, the intro starting
    // under the last two, then 100 frames of it.
    for _ in 0..(1 + 27 + 1 + 2 + 100) {
        game.step(tree);
    }
    ensure!(
        matches!(game.app.state(), AppState::Level { intro: Some(_), .. }),
        "the intro playing: {:?}",
        game.app.state()
    );
    let scene = game
        .app
        .scene(tree, SIZE)
        .map_err(|e| format!("scene: {e}"))?;
    ensure!(scene.first("video").is_some(), "the intro video");
    let pixels = draw(&mut gpu, &mut presenter, tree, &scene, game.app.clock()).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    let distinct: BTreeSet<&[u8]> = pixels.chunks(4).collect();
    ensure!(
        distinct.len() > 100,
        "video frame: {} colours",
        distinct.len()
    );
    cx.dump("intro", SIZE.width, SIZE.height, &pixels);
    Ok(String::new())
}

/// The level's start view and an outdoor view, their pictures, texture
/// bytes and first frame's time on `r`.
async fn views(
    r: &mut Renderer,
    tree: &dyn Vfs,
    data: &GameData,
) -> Result<(Vec<Vec<u8>>, u64, f64), Problem> {
    let mut screen = LevelScreen::load(tree, data, 0).map_err(|e| format!("level: {e}"))?;
    let mut presenter = Presenter::new();
    let start = now_ms();
    let mut pictures = Vec::new();
    let mut first = None;
    for (eye, yaw, pitch) in [(None, 0.0, 0.0), (Some([-180.0, 5.0, -120.0]), 1.2, 0.15)] {
        if let Some(eye) = eye {
            screen.set_camera(
                eye,
                Angles {
                    yaw,
                    pitch,
                    roll: 0.0,
                },
            );
        }
        let scene = screen.scene(SIZE);
        pictures.push(draw(r, &mut presenter, tree, &scene, 0).await?);
        first.get_or_insert(now_ms() - start);
    }
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    let world = presenter.world().ok_or("world uploaded")?;
    Ok((pictures, world.texture_bytes(), first.unwrap_or_default()))
}

/// The level's start view and an outdoor view drawn with textures
/// transcoded to ETC2 or decoded to RGBA8, each close to the picture with
/// BC as stored. Reports each path's texture memory and the transcoding
/// time; a path the device lacks is left out.
pub async fn texture_paths_match(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = game_data(tree)?;
    let mut bc = cx.renderer();
    if !bc.set_texture_path(TexturePath::Bc) {
        return Err(Problem::Skip("the device has no BC".into()));
    }
    let (reference, bc_bytes, bc_first) = views(&mut bc, tree, &data).await?;
    drop(bc);
    let mut note = format!(
        "BC: textures {:.1} MiB, first frame {bc_first:.1} ms",
        mib(bc_bytes)
    );
    for path in [TexturePath::Etc2, TexturePath::Rgba8] {
        let mut r = cx.renderer();
        if !r.set_texture_path(path) {
            note += &format!("; skipped {path:?}: the device lacks it");
            continue;
        }
        let (pictures, bytes, first) = views(&mut r, tree, &data).await?;
        let quality: Vec<f64> = pictures
            .iter()
            .zip(&reference)
            .map(|(a, b)| psnr(a, b))
            .collect();
        note += &format!(
            "; {path:?}: textures {:.1} MiB, first frame (with transcoding) {first:.1} ms, \
             PSNR against BC {quality:.1?} dB",
            mib(bytes)
        );
        for q in quality {
            ensure!(q > 30.0, "{path:?}: {q:.1} dB");
        }
        match path {
            TexturePath::Etc2 => ensure_eq!(bytes, bc_bytes, "ETC2 as large as BC"),
            _ => ensure!(bytes > 4 * bc_bytes, "RGBA8 at least four times BC"),
        }
    }
    Ok(note)
}

/// With the platform preparing textures (#113), the world takes the
/// textures prepared under `st_loading_textures`; then held under the last
/// caption while the world is uploaded in pieces (#184), it draws without
/// errors. The waiting itself is the native `tests/new_game.rs`'s.
pub async fn loading_takes_prepared_textures(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let mut data = game_data(tree)?;
    data.intro_lengths = vec![0.0; data.intro.items.len()];
    let mut game = new_game(tree, data, true)?;
    let caption = |app: &App| match app.state() {
        AppState::Loading {
            loading: Some(l), ..
        } => Some(l.captions),
        _ => None,
    };
    let mut frames = 0;
    let mut gpu = cx.renderer();
    let texture = |gpu: &Renderer, p: &str| {
        let bytes = vfs::read(tree, p).map_err(|e| format!("{p}: {e}"))?;
        let dds = Dds::parse(&bytes).map_err(|e| format!("{p}: {e:?}"))?;
        gpu.create_texture(p, &dds)
            .map_err(|e| format!("{p}: {e:?}"))
    };
    // First the prefetched models, under the first caption, their
    // geometry kept by the renderer a chunk at a time as it is made.
    while !game.app.models_to_prefetch() {
        game.step(tree);
        frames += 1;
        ensure!(
            frames < 40,
            "never asked to prefetch: {:?}",
            game.app.state()
        );
    }
    ensure_eq!(caption(&game.app), Some(1), "prefetching");
    while game.app.prefetch_models(tree, &mut |c| {
        gpu.keep_geometry(c.set, c.index, &c.vertices, &c.indices)
    }) {}
    let (chunks, chunk_bytes) = gpu.kept_geometry();
    // Then their textures, under the same caption, kept for the game type
    // (the platform makes them the prefetched owner's).
    while game.app.textures_to_prepare().is_none() {
        game.step(tree);
        frames += 1;
        ensure!(
            frames < 40,
            "never asked to prefetch: {:?}",
            game.app.state()
        );
    }
    ensure_eq!(caption(&game.app), Some(1), "prefetching");
    ensure!(game.app.prefetching_textures(), "the prefetched textures");
    let kept = game.app.textures_to_prepare().ok_or("paths")?.to_vec();
    for p in &kept {
        let t = gpu.owning(Owner::Prefetched, || texture(&gpu, p))?;
        gpu.keep_texture(p, t);
    }
    game.app.textures_prepared();
    game.step(tree);
    while game.app.textures_to_prepare().is_none() {
        game.step(tree);
        frames += 1;
        ensure!(
            frames < 40,
            "never asked to prepare: {:?}",
            game.app.state()
        );
    }
    // Held under `st_loading_textures` (the native test checks it).
    for _ in 0..3 {
        game.step(tree);
        ensure_eq!(caption(&game.app), Some(16), "held");
    }
    // The level's, but for those kept.
    let all = game.app.textures_to_prepare().ok_or("paths")?.to_vec();
    let paths: Vec<String> = (all.iter())
        .filter(|p| gpu.kept_texture(p).is_none())
        .cloned()
        .collect();
    for p in &paths {
        let t = texture(&gpu, p)?;
        gpu.add_prepared(p, t);
    }
    game.app.textures_prepared();
    game.step(tree);
    ensure_eq!(caption(&game.app), Some(17), "went on");
    // Then held under the last caption while the world is uploaded in
    // pieces (#184), its clocks not running meanwhile.
    game.step(tree);
    let level = game
        .app
        .world_to_prepare()
        .ok_or("the world to prepare")?
        .clone();
    let mut presenter = Presenter::new();
    let mut turns = 1;
    let start = now_ms();
    while !presenter.prepare_world(&mut gpu, tree, &level, 4 << 20)? {
        turns += 1;
        game.step(tree);
        ensure_eq!(caption(&game.app), Some(17), "held");
    }
    let note = format!(
        "{chunks} chunks of geometry kept ({:.1} MiB), {} textures prefetched ({} of them the level's), {} prepared; world prepared in {turns} turns, {:.1} ms",
        mib(chunk_bytes),
        kept.len(),
        all.len() - paths.len(),
        paths.len(),
        now_ms() - start
    );
    ensure!(turns > 5, "{turns} turns");
    game.app.world_prepared();
    game.step(tree);
    ensure!(matches!(game.app.state(), AppState::Level { .. }));
    let scene = game
        .app
        .scene(tree, SIZE)
        .map_err(|e| format!("scene: {e}"))?;
    presenter.draw_list(&mut gpu, tree, &scene, 0);
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    ensure!(
        paths.iter().all(|p| gpu.take_prepared(p).is_none()),
        "taken or dropped"
    );
    ensure_eq!(gpu.kept_count(), kept.len(), "the prefetched textures kept");
    ensure!(presenter.world().is_some());
    Ok(note)
}

/// The first level's placed effect (`tests\muhi`, level-placed-effects.md
/// 8) drawn (docs/behavior/particle-drawing.md): a frame with the effects
/// differs from one without them, and only around where the effect's
/// particles project.
pub async fn placed_effects_are_drawn(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let data = game_data(tree)?;
    let mut screen = LevelScreen::load(tree, &data, 0).map_err(|e| format!("level: {e}"))?;
    let fx = screen.effects_report();
    let at = fx
        .records
        .first()
        .and_then(|r| r.effects.first())
        .ok_or("the placed effect")?
        .position;
    // 0.6 m in front of the effect, looking at it along +z.
    let eye = [at[0], at[1] + 0.1, at[2] - 0.6];
    let angles = Angles {
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
    };
    // Two seconds: its 7 leaves born and drifting.
    for t in 0..120u64 {
        screen.set_camera(eye, angles);
        screen.update(16 * (t + 1), &Input::new());
    }
    screen.set_camera(eye, angles);
    // The placed effect alone: the zones' effects are the zones check's.
    screen.set_effect_origins_drawn(&[Origin::Placed]);
    let mut r = cx.renderer();
    let mut presenter = Presenter::new();
    let scene = screen.scene(SIZE);
    let on = draw(&mut r, &mut presenter, tree, &scene, 2000).await?;
    let draws = &scene.world.as_ref().ok_or("world")?.particles;
    ensure_eq!(draws.len(), 1, "{draws:?}");
    ensure_eq!(draws[0].particles, 7);
    screen.set_effects_drawn(false);
    let off_scene = screen.scene(SIZE);
    let off = draw(&mut r, &mut presenter, tree, &off_scene, 2000).await?;
    let errors = presenter.take_errors();
    ensure!(errors.is_empty(), "{errors:?}");
    cx.dump("effects", SIZE.width, SIZE.height, &on);
    cx.dump("effects-off", SIZE.width, SIZE.height, &off);
    // Where the particles project: each quad's centre on screen.
    let (view, projection) = screen.view_projection(SIZE.width, SIZE.height);
    let vp = crate::render::world::multiply(&view, &projection);
    let range = draws[0].vertices.start as usize..draws[0].vertices.end as usize;
    let vertices = &scene.world.as_ref().ok_or("world")?.particle_vertices[range];
    let centres: Vec<[f32; 2]> = vertices
        .chunks(4)
        .filter_map(|q| {
            let p: Vec<f32> = (0..3)
                .map(|i| q.iter().map(|v| v.position[i]).sum::<f32>() / 4.0)
                .collect();
            let clip: Vec<f32> = (0..4)
                .map(|c| p[0] * vp[0][c] + p[1] * vp[1][c] + p[2] * vp[2][c] + vp[3][c])
                .collect();
            (clip[3] > 0.01).then(|| {
                [
                    (clip[0] / clip[3] * 0.5 + 0.5) * SIZE.width as f32,
                    (0.5 - clip[1] / clip[3] * 0.5) * SIZE.height as f32,
                ]
            })
        })
        .collect();
    let (mut inside, mut outside) = (0, 0);
    for (i, (a, b)) in on.chunks(4).zip(off.chunks(4)).enumerate() {
        if a[..3] != b[..3] {
            let (x, y) = (
                (i as u32 % SIZE.width) as f32,
                (i as u32 / SIZE.width) as f32,
            );
            // Within 30 px of a leaf's centre (a leaf is 1 cm across).
            if centres.iter().any(|c| (c[0] - x).hypot(c[1] - y) <= 30.0) {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    let note = format!(
        "{inside} pixels differ around the effect's {} leaves on screen, {outside} elsewhere",
        centres.len()
    );
    ensure!(inside > 50, "{inside} pixels differ");
    ensure_eq!(outside, 0);
    Ok(note)
}
