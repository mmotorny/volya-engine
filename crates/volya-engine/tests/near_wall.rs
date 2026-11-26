// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The camera against a wall of `l01_escape` on a wide screen (#454): the
//! player's body keeps the eye about 0.34 m from walls, and on a screen
//! wider than about 16:9 the corners of a 0.2 m near plane reach farther,
//! so the near plane would cut the wall away in the corner aimed at it;
//! the frame takes the close near plane instead and draws the wall whole.
//! `VOLYA_DUMP_NEAR_WALL=<file.ppm>` writes the frame. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::core::camera::{self, Angles};
use volya_engine::core::collision::geometry::{
    cross, dot, front_normal, normalize, ray_triangle_distance,
};
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::present::Presenter;
use volya_engine::render::{Renderer, wgpu};
use volya_engine_formats::ui::Resolution;

/// An iPhone's landscape screen (aspect 2.17).
const WIDE: Resolution = Resolution::new(2576, 1186);
const NARROW: Resolution = Resolution::new(1024, 768);

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

#[test]
fn a_wall_beside_the_camera_is_drawn_whole_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, WIDE).unwrap();
    let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();
    let start = screen.state().eye;

    // The bunker's wall ahead (+z) of the start eye, and its normal toward
    // the eye.
    let dir = [0.0, 0.0, 1.0];
    let collision = screen.collision();
    let (d, t) = collision
        .ray_nearest(start, dir, 8.0, |t| {
            ray_triangle_distance(start, dir, &collision.corners(t)).filter(|&d| d >= 0.0)
        })
        .expect("a wall ahead of the start");
    let mut normal = front_normal(&collision.corners(t)).unwrap();
    if dot(normal, dir) > 0.0 {
        normal = normal.map(|v| -v);
    }
    assert!(normal[1].abs() < 0.2, "a wall: {normal:?}");

    // Walk into it until the body stops.
    let point = [0, 1, 2].map(|i| start[i] + dir[i] * d);
    screen.set_camera(
        [0, 1, 2].map(|i| point[i] + normal[i] * 0.8),
        Angles {
            yaw: (-normal[0]).atan2(-normal[2]),
            pitch: 0.0,
            roll: 0.0,
        },
    );
    let mut input = Input::new();
    input.handle(Event::Key {
        key: Key::W,
        pressed: true,
    });
    for frame in 1..=120 {
        screen.update(frame * 16, &input);
        input.end_frame();
    }
    let eye = screen.state().eye;

    // Turn so that the screen's upper-left corner points into the wall.
    let t = (camera::FOV_Y_DEGREES.to_radians() / 2.0).tan();
    let a = camera::aspect_factor(WIDE.width, WIDE.height);
    let corner = |angles: &Angles| {
        let (d, u) = angles.direction_up();
        let r = cross(u, d);
        normalize([0, 1, 2].map(|i| -t / a * r[i] + t * u[i] + d[i])).unwrap()
    };
    let angles = (0..360)
        .flat_map(|y| {
            (-50..=50).map(move |p| Angles {
                yaw: (y as f32).to_radians(),
                pitch: (p as f32).to_radians(),
                roll: 0.0,
            })
        })
        .max_by(|a, b| dot(corner(b), normal).total_cmp(&dot(corner(a), normal)))
        .unwrap();
    screen.set_camera(eye, angles);

    let mut gpu = renderer();
    let mut presenter = Presenter::new();
    let mut frame = |screen: &LevelScreen, size: Resolution| {
        let scene = screen.scene(size, 0);
        let list = presenter.draw_list(&mut gpu, &tree, &scene, 0);
        pollster::block_on(gpu.capture(&list, size.width, size.height, wgpu::Color::BLACK)).unwrap()
    };

    // At 4:3 the near plane's corners stay nearer than the wall: section
    // 7's near plane.
    frame(&screen, NARROW);
    assert_eq!(screen.state().near_plane, camera::NEAR, "eye {eye:?}");

    // On the wide screen they would reach through it: the close one.
    let pixels = frame(&screen, WIDE);
    if let Some(path) = std::env::var_os("VOLYA_DUMP_NEAR_WALL") {
        let mut ppm = format!("P6 {} {} 255\n", WIDE.width, WIDE.height).into_bytes();
        for p in pixels.chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
    assert_eq!(screen.state().near_plane, camera::NEAR_CLOSE, "eye {eye:?}");
}
