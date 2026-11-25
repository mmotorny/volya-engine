// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The static world's and the sky's passes on synthetic data, without game
//! data: every pipeline builds (world.wgsl, sky.wgsl validate), and packed
//! vertex attributes decode as docs/formats/level-geom.md section 8 says,
//! checked through rendered pixels.

// Needs a native GPU.
#![cfg(not(target_arch = "wasm32"))]

use volya_engine_formats::dds::Dds;
use volya_engine_formats::dds::build::{PixelFormat, file};
use volya_engine_formats::level_geom::{Layout, LevelGeom, VertexBuffer};
use volya_engine_render::sky::{SkyFrame, SkySection, SkyTextures};
use volya_engine_render::world::{
    Blend, Mat4, ObjectGeometry, Program, Surface, TreeLight, WorldLight, model_vertex,
};
use volya_engine_render::{DrawList, Renderer, Texture, WorldDraw, WorldFrame, wgpu};

const SIZE: u32 = 16;
const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn renderer() -> Renderer {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: volya_engine_render::device_features(&adapter),
                ..Default::default()
            })
            .await
            .unwrap();
        Renderer::new(&device, &queue, wgpu::TextureFormat::Bgra8Unorm)
    })
}

/// A vertex of 32 bytes: position, normal, tangent, binormal (D3DCOLOR
/// bytes b0 … b3), then the layout's last 8 bytes.
fn vertex(p: [f32; 3], tangent_alpha: u8, binormal_alpha: u8, rest: [u8; 8]) -> Vec<u8> {
    let mut v: Vec<u8> = p.iter().flat_map(|c| c.to_le_bytes()).collect();
    v.extend([128, 255, 128, 255]); // normal (0, 1, 0), hemi 1
    v.extend([128, 128, 255, tangent_alpha]);
    v.extend([255, 128, 128, binormal_alpha]);
    v.extend(rest);
    v
}

fn i16s(values: &[i16]) -> [u8; 8] {
    let mut out = [0; 8];
    for (i, v) in values.iter().enumerate() {
        out[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// One screen-covering triangle (clockwise on screen: front-facing) at
/// view depth 1, every vertex with the same last 8 bytes.
fn geom(layout: Layout, tangent_alpha: u8, binormal_alpha: u8, rest: [u8; 8]) -> LevelGeom {
    let data = [[-3.0, -3.0, 1.0], [-3.0, 9.0, 1.0], [9.0, -3.0, 1.0]]
        .iter()
        .flat_map(|&p| vertex(p, tangent_alpha, binormal_alpha, rest))
        .collect();
    LevelGeom {
        header: None,
        vertex_buffers: vec![VertexBuffer {
            elements: layout.elements(),
            layout: Some(layout),
            stride: 32,
            vertex_count: 3,
            data,
        }],
        index_buffers: vec![vec![0, 1, 2]],
        progressive_meshes: Vec::new(),
    }
}

fn surface(program: Program, base: Option<Texture>) -> Surface<Texture> {
    Surface {
        program,
        blend: Blend::Off,
        depth_write: true,
        fog: false,
        alpha_ref: None,
        detail_scale: None,
        priority: 1,
        strict: false,
        object_lod: true,
        textures: [base, None, None, None],
        env: None,
    }
}

fn draw(surface: u32, tree: Option<TreeLight>) -> WorldDraw {
    WorldDraw {
        surface,
        vertex_buffer: 0,
        first_vertex: 0,
        index_buffer: 0,
        first_index: 0,
        index_count: 3,
        transform: IDENTITY,
        sphere: [0.0, 0.0, 1.0, 10.0],
        tree,
        object: None,
    }
}

/// Looking along +z from the origin, near 0.1, far 100, square.
fn frame(world: volya_engine_render::World, sky: Option<SkyFrame>) -> WorldFrame {
    let (n, z) = (0.1, 100.0);
    let mut projection = IDENTITY;
    projection[2] = [0.0, 0.0, z / (z - n), 1.0];
    projection[3] = [0.0, 0.0, -n * z / (z - n), 0.0];
    WorldFrame {
        world,
        view: IDENTITY,
        projection,
        eye: [0.0; 3],
        forward: [0.0, 0.0, 1.0],
        fov_y_degrees: 90.0,
        light: WorldLight {
            ambient: [0.0; 3],
            hemi: [0.0; 3],
            sun_color: [0.0; 3],
            sun_dir: [0.0, -1.0, 0.0],
            fog_color: [0.0; 3],
            fog_near: 10.0,
            fog_far: 90.0,
        },
        time: 0.0,
        sky,
        clear: [0.0; 3],
    }
}

fn centre(renderer: &mut Renderer, frame: WorldFrame) -> [u8; 4] {
    let list = DrawList {
        world: Some(frame),
        quads: Vec::new(),
    };
    let pixels =
        pollster::block_on(renderer.capture(&list, SIZE, SIZE, wgpu::Color::BLACK)).unwrap();
    let i = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
    pixels[i..i + 4].try_into().unwrap()
}

#[track_caller]
fn assert_near(actual: [u8; 4], expected: [u8; 3], what: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2),
        "{what}: {actual:?} != {expected:?}"
    );
}

/// 2 × 2 quadrants: red, green (top right), blue, white.
fn quadrants(renderer: &Renderer) -> Texture {
    let mut rgba = Vec::new();
    for c in [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255; 4],
    ] {
        rgba.extend(c);
    }
    renderer.create_rgba_texture(2, 2, &rgba)
}

#[test]
fn every_program_builds_and_base_coordinates_decode() {
    let mut r = renderer();
    // 1024 × 2 texels: odd columns of row 0 red, even ones black.
    let mut rgba = Vec::new();
    for y in 0..2 {
        for x in 0..1024 {
            let red = if y == 0 && x % 2 == 1 { 255 } else { 0 };
            rgba.extend([red, 0, 0, 255]);
        }
    }
    let base = r.create_rgba_texture(1024, 2, &rgba);
    // Layout B base coordinate (s + a/255) × 32/32768 (level-geom.md 8.3):
    // s = 601 with a tangent alpha of 128 is 601.502 texels, the centre of
    // the red column 601 (without the alpha fraction it would be the edge
    // between columns 600 and 601, half red); s = 256 is row 0's centre.
    let mut rest = [0u8; 8];
    rest[4..].copy_from_slice(&i16s(&[601, 256])[..4]);
    let g = geom(Layout::VertexLit, 128, 0, rest);
    // A surface of every program: each pipeline is built.
    let surfaces: Vec<Surface<Texture>> = [
        Program::SelfLight,
        Program::LightMapped,
        Program::Terrain,
        Program::VertexLit,
        Program::Tree,
        Program::Wallmark,
        Program::LightPlanes,
    ]
    .into_iter()
    .map(|p| surface(p, Some(base.clone())))
    .collect();
    let world = r
        .create_world(&g, &surfaces, &[draw(0, None)], None)
        .unwrap();
    assert_eq!((world.draw_count(), world.skipped()), (1, 0));
    assert_near(centre(&mut r, frame(world, None)), [255, 0, 0], "selflight");
    // A draw whose layout does not suit its program is left out.
    let world = r
        .create_world(&g, &surfaces, &[draw(1, None)], None)
        .unwrap();
    assert_eq!(world.skipped(), 1);
}

#[test]
fn vertex_colour_bytes_swap() {
    let mut r = renderer();
    let white = r.create_rgba_texture(1, 1, &[255; 4]);
    // Colour bytes b0 b1 b2 b3 = 0x10 0x20 0x40 0xFF: rgb = (0x40, 0x20,
    // 0x10)/255 (level-geom.md 4); vert gives C = 2 · B · rgb with no
    // weather light (static-shading.md 7.2).
    let mut rest = [0u8; 8];
    rest[..4].copy_from_slice(&[0x10, 0x20, 0x40, 0xFF]);
    let g = geom(Layout::VertexLit, 0, 0, rest);
    let world = r
        .create_world(
            &g,
            &[surface(Program::VertexLit, Some(white))],
            &[draw(0, None)],
            None,
        )
        .unwrap();
    assert_near(
        centre(&mut r, frame(world, None)),
        [0x80, 0x40, 0x20],
        "vert",
    );
}

#[test]
fn tree_coordinates_and_constants() {
    let mut r = renderer();
    let base = quadrants(&r);
    // Layout A: (u, v) = (s0, s1)/2048 = (0.75, 0.25): green
    // (level-geom.md 8.4); the bias 2/3 × 0.75 = 0.5 makes c0 0.5, so
    // C = 2 · B · 0.5 = B (static-shading.md 7.3).
    let g = geom(Layout::Tree, 0, 0, i16s(&[1536, 512, 1024, 0]));
    let light = TreeLight {
        scale: [0.0; 5],
        bias: [2.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0, 0.0, 0.0],
    };
    let world = r
        .create_world(
            &g,
            &[surface(Program::Tree, Some(base))],
            &[draw(0, Some(light))],
            None,
        )
        .unwrap();
    assert_near(centre(&mut r, frame(world, None)), [0, 255, 0], "tree");
}

fn dds_texture(r: &Renderer, cube: bool, bgra: [u8; 4]) -> Texture {
    let faces = if cube { 6 } else { 1 };
    let data: Vec<u8> = (0..faces).flat_map(|_| bgra).collect();
    let file = file(1, 1, 0, cube, PixelFormat::BGRA8, &data);
    r.create_texture(&Dds::parse(&file).unwrap()).unwrap()
}

#[test]
fn sky_shows_where_the_world_does_not() {
    let mut r = renderer();
    // Cube maps of rgb (100, 50, 26) and (200, 100, 52), blended half and
    // half (t byte 127 ≈ 0.5), times 2 × 127/255 (sky.md 5.4).
    // Section 1 blends with section 2; section 0 is not bound. Sections 0
    // and 1 have a white sun texture, section 2 a black one.
    let white = dds_texture(&r, false, [255; 4]);
    let black = dds_texture(&r, false, [0, 0, 0, 255]);
    let section = |cube, sun: &Texture| SkySection {
        cube,
        clouds: white.clone(),
        sun: Some(sun.clone()),
    };
    let sky = SkyTextures {
        sections: vec![
            section(dds_texture(&r, true, [0, 0, 0, 255]), &white),
            section(dds_texture(&r, true, [26, 50, 100, 255]), &white),
            section(dds_texture(&r, true, [52, 100, 200, 255]), &black),
        ],
    };
    let g = geom(Layout::VertexLit, 0, 0, [0; 8]);
    let world = r.create_world(&g, &[], &[], Some(&sky)).unwrap();
    assert_eq!(world.sky_error(), None);
    let sky_frame = SkyFrame {
        section: 1,
        sun_section: 2,
        color: [127, 127, 127, 127],
        rotation: 0.0,
        clouds: Some(([37, 217, 176, 9], [113, 84, 80, 127])),
        // A sprite beside the view: drawn, but not over the centre.
        sun: Some((
            [
                [5.0, 1.0, 10.0],
                [5.0, 0.0, 10.0],
                [4.0, 1.0, 10.0],
                [4.0, 0.0, 10.0],
            ],
            [255, 255, 255, 255],
        )),
    };
    let blend = |a: f32, b: f32| (2.0 * 127.0 / 255.0 * (a + (b - a) * 127.0 / 255.0)) as u8;
    let expected = [blend(100.0, 200.0), blend(50.0, 100.0), blend(26.0, 52.0)];
    assert_near(
        centre(&mut r, frame(world.clone(), Some(sky_frame))),
        expected,
        "sky",
    );
    // A sprite over the centre (added: source alpha / one, sky.md 7.5):
    // section 2's black sun adds nothing, section 1's white one saturates.
    // Axes X = r = +x, Y = r × F = −y (sky.md 7.4).
    let over = SkyFrame {
        sun: Some((
            [
                [1.0, 1.0, 10.0],
                [1.0, -1.0, 10.0],
                [-1.0, 1.0, 10.0],
                [-1.0, -1.0, 10.0],
            ],
            [255, 255, 255, 255],
        )),
        ..sky_frame
    };
    assert_near(
        centre(&mut r, frame(world.clone(), Some(over))),
        expected,
        "section 2's sun",
    );
    let white_sun = SkyFrame {
        sun_section: 1,
        ..over
    };
    assert_near(
        centre(&mut r, frame(world, Some(white_sun))),
        [255; 3],
        "section 1's sun",
    );
    // A 1-face texture is not a cube map: no sky, said why.
    let mut flat = sky.clone();
    flat.sections[0].cube = white.clone();
    let world = r.create_world(&g, &[], &[], Some(&flat)).unwrap();
    assert!(world.sky_error().is_some());
}

/// One object: a screen-covering triangle in model space at z = 0, moved
/// to z = 1 by its transform, with the stored normal bytes `normal` and
/// texture coordinate (0.75, 0.25), quantized as level-objects.md section
/// 4.7 stores them.
fn object_geometry(normal: [u8; 3]) -> ObjectGeometry {
    let q = |v: f32| (v * 32767.0 / 12.0).floor() as i16;
    let uv = [
        (0.75f32 * 32767.0 / 16.0) as i16,
        (0.25f32 * 32767.0 / 16.0) as i16,
    ];
    let vertices = [[-3.0, -3.0], [-3.0, 9.0], [9.0, -3.0]]
        .iter()
        .flat_map(|&[x, y]| model_vertex([q(x), q(y), 0], normal, uv))
        .collect();
    ObjectGeometry {
        vertices,
        indices: vec![0, 1, 2],
    }
}

/// Normal bytes of (0, 1, 0) and of (0, 0, −1), towards the camera.
const UP: [u8; 3] = [127, 255, 127];
const TOWARDS: [u8; 3] = [127, 127, 0];

fn object_draw(surface: u32, props: [f32; 4]) -> WorldDraw {
    object_draw_at(surface, props, 1.0)
}

/// [`object_draw`] moved to depth `z`.
fn object_draw_at(surface: u32, props: [f32; 4], z: f32) -> WorldDraw {
    let mut transform = IDENTITY;
    transform[3] = [0.0, 0.0, z, 1.0];
    WorldDraw {
        transform,
        object: Some(props),
        ..draw(surface, None)
    }
}

/// The centre pixel of `objects` drawn with `surface` under `light`.
fn object_pixel(
    r: &mut Renderer,
    objects: &ObjectGeometry,
    surface: Surface<Texture>,
    props: [f32; 4],
    light: impl FnOnce(&mut WorldLight),
) -> [u8; 4] {
    object_pixel_at(r, objects, surface, props, light, 1.0)
}

fn object_pixel_at(
    r: &mut Renderer,
    objects: &ObjectGeometry,
    surface: Surface<Texture>,
    props: [f32; 4],
    light: impl FnOnce(&mut WorldLight),
    z: f32,
) -> [u8; 4] {
    let g = geom(Layout::VertexLit, 0, 0, [0; 8]);
    let world = r
        .create_world_with(
            &g,
            objects,
            &[surface],
            &[object_draw_at(0, props, z)],
            None,
        )
        .unwrap();
    assert_eq!((world.object_draw_count(), world.draw_count()), (1, 0));
    assert!(world.object_gpu_bytes() >= 48 + 8);
    let mut f = frame(world, None);
    light(&mut f.light);
    centre(r, f)
}

#[test]
fn objects_draw_quantized_models() {
    let mut r = renderer();
    let base = quadrants(&r);
    let up = object_geometry(UP);
    // Q = (n.y · 0.5 + 0.5) · L_dynamic_props.w · hemi + ambient
    // + L_dynamic_props.xyz · S(n) = 1 · 0.5 · 0.4 + 0.1 = 0.3 without sun;
    // C = 2 · Q · B with B green at (0.75, 0.25) (sections 5.4, 5.5).
    let hemi = |l: &mut WorldLight| {
        l.ambient = [0.1; 3];
        l.hemi = [0.4; 3];
    };
    let model = surface(Program::Model, Some(base.clone()));
    let px = object_pixel(&mut r, &up, model.clone(), [0.0, 0.0, 0.0, 0.5], hemi);
    assert_near(px, [0, 153, 0], "model_def_lq, hemisphere");
    // The sun term: sunlight straight down on the upward normal, colour
    // 0.4, L_dynamic_props.xyz 0.5: Q = 0.5 · 0.4 = 0.2, C = 0.4 · B.
    let sun = |l: &mut WorldLight| {
        l.sun_color = [0.4; 3];
        l.sun_dir = [0.0, -1.0, 0.0];
    };
    let px = object_pixel(&mut r, &up, model.clone(), [0.5, 0.5, 0.5, 0.0], sun);
    assert_near(px, [0, 102, 0], "model_def_lq, sun");
    // Lit from below, the sun adds nothing.
    let below = |l: &mut WorldLight| {
        l.sun_color = [0.4; 3];
        l.sun_dir = [0.0, 1.0, 0.0];
    };
    let px = object_pixel(&mut r, &up, model, [0.5, 0.5, 0.5, 0.0], below);
    assert_near(px, [0, 0, 0], "model_def_lq, sun behind");
    // Self-lit: the texel.
    let px = object_pixel(
        &mut r,
        &up,
        surface(Program::ModelSelfLight, Some(base.clone())),
        [0.0; 4],
        |_| {},
    );
    assert_near(px, [0, 255, 0], "selflight");
    // Light planes, added (source alpha / one) onto black: B.rgb faded by
    // |view direction · normal| interpolated from the vertices, whose view
    // directions (−3, −3, 1), (−3, 9, 1), (9, −3, 1) meet the normal
    // facing the camera at 0.224, 0.107, 0.107; the centre pixel weighs
    // them 1/2, 1/4, 1/4: 0.166 · 255 = 42.
    let mut planes = surface(Program::ModelLightPlanes, Some(base));
    planes.blend = Blend::Add;
    let px = object_pixel(&mut r, &object_geometry(TOWARDS), planes, [0.0; 4], |_| {});
    assert_near(px, [0, 42, 0], "lightplanes");

    // A level surface on an object's draw, and a model surface on a level
    // draw, are left out.
    let g = geom(Layout::VertexLit, 0, 0, [0; 8]);
    let surfaces = [
        surface(Program::VertexLit, None),
        surface(Program::Model, None),
    ];
    let world = r
        .create_world_with(
            &g,
            &up,
            &surfaces,
            &[object_draw(0, [0.0; 4]), draw(1, None)],
            None,
        )
        .unwrap();
    assert_eq!(world.skipped(), 2);
    // An object draw beyond the objects' indices fails.
    let mut far = object_draw(1, [0.0; 4]);
    far.first_index = 3;
    assert!(
        r.create_world_with(&g, &up, &surfaces, &[far], None)
            .is_err()
    );
}

/// Placing a draw after the world is made: hidden with a radius of 0,
/// shown again, moved (objects switching online and offline).
#[test]
fn placed_draws_hide_show_and_move() {
    let mut r = renderer();
    let base = quadrants(&r);
    let g = geom(Layout::VertexLit, 0, 0, [0; 8]);
    let surfaces = [
        surface(Program::ModelSelfLight, Some(base)),
        surface(Program::Model, None),
    ];
    // Draw 0 is left out (a model surface on a level draw), so the object
    // is draw 1 given, the first kept.
    let draws = [draw(1, None), object_draw(0, [0.0; 4])];
    let world = r
        .create_world_with(&g, &object_geometry(UP), &surfaces, &draws, None)
        .unwrap();
    assert_eq!(world.skipped(), 1);
    let shown = draws[1];
    assert_near(
        centre(&mut r, frame(world.clone(), None)),
        [0, 255, 0],
        "shown",
    );
    r.place_draw(&world, 1, &shown.transform, [0.0, 0.0, 1.0, 0.0]);
    assert_near(
        centre(&mut r, frame(world.clone(), None)),
        [0, 0, 0],
        "hidden",
    );
    r.place_draw(&world, 1, &shown.transform, shown.sphere);
    assert_near(
        centre(&mut r, frame(world.clone(), None)),
        [0, 255, 0],
        "shown again",
    );
    // Moved behind the eye: not in view.
    let mut behind = shown.transform;
    behind[3] = [0.0, 0.0, -5.0, 1.0];
    r.place_draw(&world, 1, &behind, [0.0, 0.0, -5.0, 10.0]);
    assert_near(
        centre(&mut r, frame(world.clone(), None)),
        [0, 0, 0],
        "moved",
    );
    // A draw left out, or beyond the list, is ignored.
    r.place_draw(&world, 0, &IDENTITY, [0.0; 4]);
    r.place_draw(&world, 7, &IDENTITY, [0.0; 4]);
}

/// A cube map whose six 4 × 4 faces (+x, −x, +y, −y, +z, −z) each have
/// one colour (BGRA), so filtering near a face's centre stays on it.
fn faces_cube(r: &Renderer, faces: [[u8; 4]; 6]) -> Texture {
    let data: Vec<u8> = faces.iter().flat_map(|f| f.repeat(16)).collect();
    let file = file(4, 4, 0, true, PixelFormat::BGRA8, &data);
    r.create_texture(&Dds::parse(&file).unwrap()).unwrap()
}

#[test]
fn environment_mapped_models() {
    let mut r = renderer();
    // A base texel with alpha 0: the environment's texel shows:
    // C = 2 · Q · lerp(E, B, B.a) with Q = 0.5 (ambient).
    let base = r.create_rgba_texture(1, 1, &[255, 255, 255, 0]);
    let mut env = surface(Program::ModelEnv, Some(base));
    let colours: [[u8; 4]; 6] = std::array::from_fn(|i| [0, 40 * i as u8, 200, 255]);
    env.env = Some(faces_cube(&r, colours));
    let ambient = |l: &mut WorldLight| l.ambient = [0.5; 3];
    // At 40 m every view direction is near (0, 0, 1). Reflected about the
    // normal facing the camera it points back along −z: face 5. About the
    // upward normal it keeps going along +z: face 4.
    let at = |r: &mut Renderer, normal, env| {
        object_pixel_at(r, &object_geometry(normal), env, [0.0; 4], ambient, 40.0)
    };
    let px = at(&mut r, TOWARDS, env.clone());
    assert_near(px, [200, 200, 0], "reflected towards the camera");
    let px = at(&mut r, UP, env);
    assert_near(px, [200, 160, 0], "reflected past the upward normal");
}
