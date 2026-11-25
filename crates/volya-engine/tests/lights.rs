// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The first level's lights on a New Game (BYOA): its hanging lamps
//! (docs/behavior/hanging-lamps.md 9.3), their light records and the sun's
//! (docs/behavior/dynamic-lights.md 3.1, 3.3, 10.1), the shader entries
//! with light elements (10.1), the glows (docs/behavior/glows.md 7), and a
//! lamp's light drawn on both renderer paths against the same frame with
//! `r1_dlights` off, with frame times. `VOLYA_DUMP_LIGHTS=<file.ppm>` and
//! `VOLYA_DUMP_LIGHTS_OFF=<file.ppm>` write the frames locally. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use volya_engine::core::camera::Angles;
use volya_engine::core::config;
use volya_engine::core::input::Input;
use volya_engine::core::lights::{LightKind, LightSettings, sun_b_direction};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{self, StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::level_lights::animated_colour;
use volya_engine::present::Presenter;
use volya_engine::render::world::LightProgram;
use volya_engine::render::{Renderer, wgpu};
use volya_engine::shading::{self, ShaderLibrary};
use volya_engine_formats::level::Level;
use volya_engine_formats::shaders_xr;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);
/// The placement table's tolerance: its values are given to 4 decimals and
/// hold within 6·10⁻⁵ (hanging-lamps.md 9.3).
const TABLE: f32 = 6e-5 + 5e-5;

fn renderer() -> Renderer {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: volya_engine::render::device_features(&adapter),
                ..Default::default()
            })
            .await
            .unwrap();
        Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm)
    })
}

fn dump(var: &str, pixels: &[u8]) {
    if let Some(path) = std::env::var_os(var) {
        let mut ppm = format!("P6 {} {} 255\n", SIZE.width, SIZE.height).into_bytes();
        for p in pixels.chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
}

fn with_game(f: impl FnOnce(&dyn Vfs, &GameData)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    f(&tree, &data);
}

fn near3(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
}

/// hanging-lamps.md 9.3: record, brightness, animator, range, glow radius,
/// main light position and direction at rest.
#[allow(clippy::type_complexity)]
const LAMPS: [(&str, f32, &str, f32, f32, [f32; 3], [f32; 3]); 5] = [
    (
        "trader_light_0000",
        1.0,
        "light 40 hz",
        1.0,
        0.3,
        [-241.2864, -23.7312, -135.4352],
        [-0.4217, -0.7547, 0.5026],
    ),
    (
        "trader_light_0003",
        0.8,
        "light 40 hz",
        2.0,
        0.3,
        [-245.4443, -23.6726, -134.5719],
        [0.5545, -0.7547, 0.3506],
    ),
    (
        "trader_light_0005",
        0.6,
        "light 40 hz",
        1.5,
        0.3,
        [-140.7902, -28.3921, -351.9944],
        [0.0, -0.7547, -0.6561],
    ),
    (
        "trader_tv_0000",
        1.0,
        "tv light 1",
        3.0,
        1.0,
        [-245.2665, -22.9093, -133.0806],
        [0.6428, 0.0, -0.7660],
    ),
    (
        "trader_tv_0002",
        0.8,
        "tv light 1",
        3.0,
        1.0,
        [-119.8046, -29.1813, -390.8865],
        [-0.2406, 0.0, 0.9706],
    ),
];

#[test]
fn first_level_lights_from_game_data() {
    with_game(|tree, data| {
        let mut screen = LevelScreen::load(tree, data, 0).unwrap();
        let ll = screen.level_lights();
        // dynamic-lights.md 3.3: five lamps, seven records.
        assert_eq!(ll.lamps.len(), 5);
        let mut names: Vec<&str> = ll.lamps.iter().map(|l| l.name.as_str()).collect();
        names.sort();
        assert_eq!(names, LAMPS.map(|l| l.0));
        assert!(ll.lamps.iter().all(|l| l.on));
        let records = screen.lights();
        let drawable: Vec<_> = records
            .iter()
            .filter(|(_, r)| r.active && !r.excluded)
            .map(|(_, r)| r)
            .collect();
        assert_eq!(drawable.len(), 7);
        let mut spots: Vec<f32> = (drawable.iter())
            .filter(|r| r.kind == LightKind::Spot)
            .map(|r| r.range)
            .collect();
        spots.sort_by(f32::total_cmp);
        assert_eq!(spots, [1.0, 1.5, 2.0, 3.0, 3.0]);
        assert!(drawable.iter().all(|r| !r.shadow));
        let spot_cones = drawable.iter().filter(|r| r.kind == LightKind::Spot);
        assert!(
            spot_cones
                .clone()
                .all(|r| (r.cone - 120f32.to_radians()).abs() < 1e-5)
        );
        let ambient: Vec<_> = (drawable.iter())
            .filter(|r| r.kind == LightKind::Point)
            .collect();
        assert_eq!(ambient.len(), 2);
        assert!(ambient.iter().all(|r| r.range == 1.0));
        // The 40 chunk-6 point records, excluded, and the sun's two.
        assert_eq!(records.iter().filter(|(_, r)| r.excluded).count(), 42);

        // Glows: chunk 7's 22 and the five lamps' (glows.md 7).
        let glows: Vec<_> = ll.glows.iter().collect();
        assert_eq!(glows.len(), 27);
        let mut lamp_radii: Vec<f32> = glows
            .iter()
            .filter(|g| g.fade == 1.0)
            .map(|g| g.radius)
            .collect();
        lamp_radii.sort_by(f32::total_cmp);
        assert_eq!(lamp_radii, [0.3, 0.3, 0.3, 1.0, 1.0]);
        let world = screen.world().clone();
        let orange: Vec<_> = (world.glow_shaders.iter())
            .filter(|s| s.texture.contains("glow_orange"))
            .collect();
        assert_eq!(orange.len(), 1, "{:?}", world.glow_shaders);
        assert_eq!(world.glow_shaders.len(), 4);

        // Placed and coloured at each frame (4.2, 4.3), at global clock
        // times of the sample table.
        for (i, (t_ms, record, colour)) in [
            (350, "trader_light_0003", [0.800000, 0.696471, 0.536471]),
            (0, "trader_light_0005", [0.600000, 0.501176, 0.388235]),
            (500, "trader_tv_0000", [0.258824, 0.490196, 0.686275]),
            (2000, "trader_tv_0002", [0.250980, 0.549020, 0.793726]),
        ]
        .into_iter()
        .enumerate()
        {
            screen.set_global_clock(t_ms);
            screen.update(16 * (i as u64 + 1), &Input::new());
            let lamp = (screen.level_lights().lamps.iter())
                .find(|l| l.name == record)
                .unwrap();
            let r = screen.lights().get(lamp.main).unwrap();
            assert!(
                near3([r.colour[0], r.colour[1], r.colour[2]], colour, 1e-6),
                "{record}: {:?}",
                r.colour
            );
            if let Some(a) = lamp.ambient {
                let a = screen.lights().get(a).unwrap();
                assert!(near3(
                    [a.colour[0], a.colour[1], a.colour[2]],
                    colour.map(|c| c * 0.2),
                    1e-6
                ));
            }
        }
        assert_eq!(
            animated_colour(0x00FF_DEAB, 0.8)[1],
            222.0 * (0.8 * volya_engine_formats::lanims::K)
        );
        for (name, _, _, range, _, position, direction) in LAMPS {
            let lamp = (screen.level_lights().lamps.iter())
                .find(|l| l.name == name)
                .unwrap();
            let r = screen.lights().get(lamp.main).unwrap();
            assert_eq!(r.range, range, "{name}");
            assert!(
                near3(r.position, position, TABLE),
                "{name}: {:?}",
                r.position
            );
            assert!(
                near3(r.direction, direction, TABLE),
                "{name}: {:?}",
                r.direction
            );
            let g = screen.level_lights().glows.get(lamp.glow.unwrap()).unwrap();
            assert!(near3(g.position, position, TABLE), "{name}");
            if let Some(a) = lamp.ambient {
                let a = screen.lights().get(a).unwrap();
                assert!(near3(a.position, position, TABLE), "{name}");
            }
        }
        // The sun's record B (dynamic-lights.md 3.1).
        let (_, b) = screen.lights().sun().unwrap();
        let b = screen.lights().get(b).unwrap();
        let sun = screen.state();
        let _ = sun;
        assert!(b.excluded && !b.active);
        assert!((b.range - 600.0).abs() < 1e-3);
        let d = b.direction;
        assert!((d.iter().map(|c| c * c).sum::<f32>() - 1.0).abs() < 1e-5);
        assert!(
            d[1] < -0.5,
            "B points down more steeply than the sun: {d:?}"
        );

        // dynamic-lights.md 10.1: 380 of the 395 non-empty entries have
        // light elements.
        let level =
            Level::parse(&vfs::read(tree, "gamedata\\levels\\l01_escape\\level").unwrap()).unwrap();
        let aliases = &data.aliases;
        let definitions: HashMap<String, shaders_xr::Definition> = shaders_xr::parse(
            &vfs::read(tree, &aliases.path("$game_data$", "shaders.xr").unwrap()).unwrap(),
        )
        .unwrap()
        .into_iter()
        .map(|d| (d.name.clone(), d))
        .collect();
        let shader_dir = aliases.path("$game_shaders$", "r1").unwrap();
        let scripted = |module: &str| {
            vfs::read(tree, &format!("{shader_dir}\\{module}.s"))
                .is_ok_and(|t| shading::defines_normal_or_special(&String::from_utf8_lossy(&t)))
        };
        let textures_ltx = config::load(
            tree,
            &aliases.path("$game_textures$", "textures.ltx").unwrap(),
        )
        .unwrap();
        let associations = shading::associations(&textures_ltx);
        let library = ShaderLibrary {
            definitions: &definitions,
            scripted: &scripted,
            associations: &associations,
        };
        let entries: Vec<&String> = level.shaders.iter().filter(|e| !e.is_empty()).collect();
        assert_eq!(entries.len(), 395);
        let mut by: BTreeMap<(String, u8), usize> = BTreeMap::new();
        let mut unlit = 0;
        for e in &entries {
            let Ok(s) = library.resolve(e) else { continue };
            let Some(l) = s.light else { continue };
            // Priority 2 or 3: never lit, counted as without (5.5 rule 4):
            // `def_shaders\def_trans_v`.
            if l.priority > 1 {
                assert!(e.starts_with("def_shaders\\def_trans_v/"), "{e}");
                unlit += 1;
                continue;
            }
            let name = match (l.program, s.program) {
                (LightProgram::LightMapped, volya_engine::render::world::Program::Terrain) => {
                    "impl"
                }
                (LightProgram::LightMapped, _) => "lmap",
                (LightProgram::VertexLit, volya_engine::render::world::Program::SelfLight) => {
                    "simple"
                }
                (LightProgram::VertexLit, _) => "vert",
                (LightProgram::Tree, _) if s.object_lod => "tree_s",
                (LightProgram::Tree, _) => "tree_w",
                (LightProgram::Wallmark, _) => "wmark",
                (LightProgram::Model, _) => "model",
            };
            *by.entry((name.to_owned(), l.alpha_ref)).or_default() += 1;
        }
        let total: usize = by.values().sum();
        assert_eq!(
            by.into_iter().collect::<Vec<_>>(),
            [
                (("impl".to_owned(), 0), 4),
                (("lmap".to_owned(), 0), 250),
                (("lmap".to_owned(), 33), 4),
                (("simple".to_owned(), 0), 3),
                (("tree_s".to_owned(), 0), 37),
                (("tree_w".to_owned(), 0), 14),
                (("vert".to_owned(), 0), 54),
                (("vert".to_owned(), 33), 3),
                (("wmark".to_owned(), 0), 11),
            ]
        );
        assert_eq!((total, unlit), (380, 5));
        let d_b = sun_b_direction([0.9132, -0.2079, 0.3505]);
        assert!(near3(d_b, [0.6670, -0.6997, 0.2560], 1e-4));

        // glows.md 3, 5, 6: glow 0 (30.484, 19.756, 679.066) seen from
        // 6 m: tested, faded in and drawn as one sprite of its radius.
        let glow = [30.484, 19.756, 679.066];
        let eye = [glow[0], glow[1], glow[2] - 6.0];
        let mut now = 1_000;
        for _ in 0..30 {
            screen.set_camera(eye, look(eye, glow));
            now += 16;
            screen.update(now, &Input::new());
        }
        let ll = screen.level_lights();
        let g = ll
            .glows
            .iter()
            .find(|g| near3(g.position, glow, 1e-3))
            .unwrap();
        assert!(g.in_sector && !g.occluded && g.fade == 255.0, "{g:?}");
        let sprite = (ll.sprites().iter())
            .find(|s| near3(s.corners[0], [glow[0] - 1.0, glow[1] - 1.0, glow[2]], 0.05))
            .expect("glow 0's sprite");
        assert!(sprite.b > 200, "{sprite:?}");
    });
}

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

#[test]
fn a_lamp_lights_the_level_on_both_paths_from_game_data() {
    with_game(|tree, data| {
        // trader_tv_0000's light (range 3, cone 120°) on what is in front
        // of it, seen from above and behind the set.
        let (_, _, _, _, _, p, d) = LAMPS[3];
        let at = [p[0] + 1.5 * d[0], p[1] - 0.4, p[2] + 1.5 * d[2]];
        let eye = [p[0] - 0.6 * d[0], p[1] + 0.5, p[2] - 0.6 * d[2]];
        for compat in [false, true] {
            let mut screen = LevelScreen::load(tree, data, 0).unwrap();
            screen.set_effects_drawn(false);
            screen.set_camera(eye, look(eye, at));
            screen.update(16, &Input::new());
            let mut r = renderer();
            r.set_compat(compat);
            let mut presenter = Presenter::new();
            let mut capture = |screen: &LevelScreen| {
                let scene = screen.scene(SIZE, 16);
                let list = presenter.draw_list(&mut r, tree, &scene, 16);
                let start = Instant::now();
                let pixels = pollster::block_on(r.capture(
                    &list,
                    SIZE.width,
                    SIZE.height,
                    wgpu::Color::BLACK,
                ))
                .unwrap();
                (pixels, scene, start.elapsed(), r.light_draws())
            };
            let _ = capture(&screen);
            // The median of 9 frames' times.
            let median = |mut t: Vec<std::time::Duration>| {
                t.sort();
                t[t.len() / 2]
            };
            let mut times = Vec::new();
            for _ in 0..8 {
                times.push(capture(&screen).2);
            }
            let (on, scene, t, draws) = capture(&screen);
            times.push(t);
            let t_on = median(times);
            let passes = scene.world.as_ref().unwrap().lights.len();
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
                times.push(capture(&screen).2);
            }
            let (off, scene_off, t, _) = capture(&screen);
            times.push(t);
            let t_off = median(times);
            assert!(presenter.take_errors().is_empty());
            assert!(scene_off.world.as_ref().unwrap().lights.is_empty());
            dump("VOLYA_DUMP_LIGHTS", &on);
            dump("VOLYA_DUMP_LIGHTS_OFF", &off);
            let (mut brighter, mut darker) = (0, 0);
            for (a, b) in on.chunks(4).zip(off.chunks(4)) {
                let (sa, sb): (u32, u32) = (
                    a[..3].iter().map(|&c| u32::from(c)).sum(),
                    b[..3].iter().map(|&c| u32::from(c)).sum(),
                );
                brighter += usize::from(sa > sb + 3);
                darker += usize::from(sb > sa + 3);
            }
            eprintln!(
                "compat {compat}: {passes} light passes drawing {draws} draws, {brighter} pixels \
                 brighter, {darker} darker; frame {t_on:.1?} with lights, {t_off:.1?} without"
            );
            assert!(passes >= 1, "compat {compat}");
            assert!(brighter > 2_000, "compat {compat}: {brighter} pixels lit");
            assert!(darker < brighter / 50, "compat {compat}: {darker} darker");
        }
    });
}

/// The lights' cost per frame on the CPU: the lamps' updates, the sun's
/// records, the choice of lights and their passes, and the glows with
/// their occlusion rays, at the start position and beside the trader's
/// lamps (run with `--release` for meaningful times).
#[test]
fn lights_frame_cost_from_game_data() {
    with_game(|tree, data| {
        let mut screen = LevelScreen::load(tree, data, 0).unwrap();
        let (_, _, _, _, _, p, d) = LAMPS[3];
        let at = [p[0] + 1.5 * d[0], p[1] - 0.4, p[2] + 1.5 * d[2]];
        let eye = [p[0] - 0.6 * d[0], p[1] + 0.5, p[2] - 0.6 * d[2]];
        for (what, place) in [("start", None), ("trader", Some((eye, look(eye, at))))] {
            if let Some((eye, angles)) = place {
                screen.set_camera(eye, angles);
            }
            let (view, projection) = screen.view_projection(SIZE.width, SIZE.height);
            let camera = volya_engine::level_lights::LightCamera {
                eye: screen.player().view().eye,
                view,
                projection,
                far: 350.0,
            };
            let sun = ([0.9132, -0.2079, 0.3505], [0.759, 0.380, 0.102]);
            let collision = screen.shared_collision();
            let frames = 2_000u32;
            let started = Instant::now();
            for i in 0..frames {
                let lights = screen.level_lights_mut();
                lights.lamps_frame(i as f32 * 0.016, 0.016);
                lights.frame(&collision, &camera, sun, 0.016);
            }
            let per = started.elapsed() / frames;
            let report = screen.level_lights().report();
            eprintln!(
                "{what}: {per:.1?} a frame, {} passes, {} glow sprites",
                report.passes, report.sprites
            );
            assert!(per.as_micros() < 5_000, "{what}: {per:?}");
        }
    });
}
