// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The static world's and the sky's passes on synthetic data: every
//! pipeline builds, and packed vertex attributes decode as
//! docs/formats/level-geom.md section 8 says, checked through rendered
//! pixels; objects, placed draws, details, particles, lights, glows and
//! the presentation's reduction likewise.

use std::sync::Arc;

use volya_engine_formats::dds::Dds;
use volya_engine_formats::dds::build::{PixelFormat, file};
use volya_engine_formats::level_geom::{GeomSource, Layout, LevelGeom, VertexBuffer};

use super::{Make, Outcome, Problem, capture, ensure, ensure_eq, pixel};
use crate::details::{DetailDraw, DetailFrame, DetailGeometry, STILL, SWAY_1};
use crate::particles::{
    ParticleBatch, ParticleBlend, ParticleCull, ParticlePass, ParticleProgram, ParticleQueue,
    ParticleVertex,
};
use crate::present::{Presentation, ScreenEffects};
use crate::sky::{SkyFrame, SkySection, SkyTextures};
use crate::world::{
    Blend, LightElement, LightPass, LightProgram, LightTextures, Mat4, ObjectGeometry, Program,
    Surface, TreeLight, WorldLight, model_vertex,
};
use crate::{DrawList, Renderer, Texture, World, WorldDraw, WorldFrame};

const SIZE: u32 = 16;
const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// A failure of `what`, with the error.
fn fail(what: &str) -> impl FnOnce(String) -> Problem + '_ {
    move |e| Problem::Fail(format!("{what}: {e}"))
}

/// Two draws of one index range from different first vertices: the second's
/// triangle (green, nearer) covers the first's (red). Without base vertices
/// the range is uploaded twice, rebased for each.
pub async fn shared_index_ranges_with_different_bases(make: Make<'_>) -> Outcome {
    let mut r = make();
    let white = r.create_rgba_texture(1, 1, &[255; 4]);
    // Vertex colour bytes b0 b1 b2 b3: red is (0, 0, 0x7F), green
    // (0, 0x7F, 0) (vert doubles them, level-geom.md 4).
    let corners = [[-3.0, -3.0], [-3.0, 9.0], [9.0, -3.0]];
    let mut data = Vec::new();
    for (z, colour) in [(1.0, [0, 0, 0x7F, 0xFF]), (0.5, [0, 0x7F, 0, 0xFF])] {
        let mut rest = [0u8; 8];
        rest[..4].copy_from_slice(&colour);
        for [x, y] in corners {
            data.extend(vertex([x, y, z], 0, 0, rest));
        }
    }
    let g = LevelGeom {
        header: None,
        vertex_buffers: vec![VertexBuffer {
            elements: Layout::VertexLit.elements(),
            layout: Some(Layout::VertexLit),
            stride: 32,
            vertex_count: 6,
            data,
        }],
        index_buffers: vec![vec![0, 1, 2]],
        progressive_meshes: Vec::new(),
    };
    let surfaces = [surface(Program::VertexLit, Some(white))];
    let red = draw(0, None);
    let green = WorldDraw {
        first_vertex: 3,
        ..red
    };
    let world = r
        .create_world(&g, &surfaces, &[red], None)
        .map_err(fail("world"))?;
    expect_near(
        centre(&mut r, frame(world, None)).await?,
        [0xFE, 0, 0],
        "red",
    )?;
    let world = r
        .create_world(&g, &surfaces, &[red, green], None)
        .map_err(fail("world"))?;
    expect_near(
        centre(&mut r, frame(world, None)).await?,
        [0, 0xFE, 0],
        "green over red",
    )?;
    // In steps (#184): one piece a step, the vertex buffer, the index
    // buffer, then the vertices' draw ids (#387); finished only when all is
    // uploaded, and for the same draws.
    let objects = ObjectGeometry::default();
    let draws = [red, green];
    let layout = g.source().layout().clone();
    let mut upload = r
        .begin_geometry(&layout, &objects, &draws)
        .map_err(fail("begin"))?;
    let mut steps = 1;
    while !upload
        .step(&r, &mut g.source(), &objects, 1)
        .map_err(fail("step"))?
    {
        steps += 1;
        ensure!(steps <= 100, "the upload never finishes");
    }
    ensure_eq!(steps, 3, "a piece a step");
    let other = r
        .begin_geometry(&layout, &objects, &[red])
        .map_err(fail("begin"))?;
    ensure!(
        r.finish_world(other, &surfaces, &[red], None).is_err(),
        "an unfinished upload finished"
    );
    ensure!(
        r.finish_world(upload, &surfaces, &[red], None).is_err(),
        "an upload finished for other draws"
    );
    let mut upload = r
        .begin_geometry(&layout, &objects, &draws)
        .map_err(fail("begin"))?;
    for (step, last) in [(1, false), (2, false), (3, true)] {
        let done = upload
            .step(&r, &mut g.source(), &objects, 1)
            .map_err(fail("step"))?;
        ensure_eq!(done, last, "finished after step {step}");
    }
    let world = r
        .finish_world(upload, &surfaces, &draws, None)
        .map_err(fail("finish"))?;
    expect_near(
        centre(&mut r, frame(world, None)).await?,
        [0, 0xFE, 0],
        "green over red, uploaded in steps",
    )?;
    Ok(String::new())
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
    for (o, v) in out.as_chunks_mut::<2>().0.iter_mut().zip(values) {
        *o = v.to_le_bytes();
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
        light: None,
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
fn frame(world: World, sky: Option<SkyFrame>) -> WorldFrame {
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
        particles: Vec::new(),
        lights: Vec::new(),
        presentation: Default::default(),
        effects: None,
        details: None,
        billboards: None,
        projected: None,
        filtering: Default::default(),
        geometry_lod: 1.0,
        drawn: None,
    }
}

/// The centre pixel of `frame` drawn at `SIZE` × `SIZE`.
async fn centre(renderer: &mut Renderer, frame: WorldFrame) -> Result<[u8; 4], Problem> {
    let image = pixels(renderer, frame, SIZE).await?;
    Ok(pixel(&image, SIZE, SIZE / 2, SIZE / 2))
}

/// Fails unless the first three channels of `actual` are within 2 of
/// `expected`.
fn expect_near(actual: [u8; 4], expected: [u8; 3], what: &str) -> Result<(), Problem> {
    ensure!(
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2),
        "{what}: {actual:?} != {expected:?}"
    );
    Ok(())
}

/// The colour channels of a pixel.
fn rgb(p: [u8; 4]) -> [u8; 3] {
    [p[0], p[1], p[2]]
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

pub async fn every_program_builds_and_base_coordinates_decode(make: Make<'_>) -> Outcome {
    let mut r = make();
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
        .map_err(fail("world"))?;
    ensure_eq!(
        (world.draw_count(), world.skipped()),
        (1, 0),
        "draws, skipped"
    );
    expect_near(
        centre(&mut r, frame(world, None)).await?,
        [255, 0, 0],
        "selflight",
    )?;
    // A draw whose layout does not suit its program is left out.
    let world = r
        .create_world(&g, &surfaces, &[draw(1, None)], None)
        .map_err(fail("world"))?;
    ensure_eq!(world.skipped(), 1, "skipped");
    Ok(String::new())
}

pub async fn vertex_colour_bytes_swap(make: Make<'_>) -> Outcome {
    let mut r = make();
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
        .map_err(fail("world"))?;
    expect_near(
        centre(&mut r, frame(world, None)).await?,
        [0x80, 0x40, 0x20],
        "vert",
    )?;
    Ok(String::new())
}

pub async fn tree_coordinates_and_constants(make: Make<'_>) -> Outcome {
    let mut r = make();
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
        .map_err(fail("world"))?;
    expect_near(
        centre(&mut r, frame(world, None)).await?,
        [0, 255, 0],
        "tree",
    )?;
    Ok(String::new())
}

fn dds_texture(r: &Renderer, cube: bool, bgra: [u8; 4]) -> Result<Texture, Problem> {
    let faces = if cube { 6 } else { 1 };
    let data: Vec<u8> = (0..faces).flat_map(|_| bgra).collect();
    let file = file(1, 1, 0, cube, PixelFormat::BGRA8, &data);
    let dds = Dds::parse(&file).map_err(|e| Problem::Fail(format!("dds: {e}")))?;
    r.create_texture(&dds)
        .map_err(|e| Problem::Fail(format!("texture: {e}")))
}

pub async fn sky_shows_where_the_world_does_not(make: Make<'_>) -> Outcome {
    let mut r = make();
    // Cube maps of rgb (100, 50, 26) and (200, 100, 52), blended half and
    // half (t byte 127 ≈ 0.5), times 2 × 127/255 (sky.md 5.4).
    // Section 1 blends with section 2; section 0 is not bound. Sections 0
    // and 1 have a white sun texture, section 2 a black one.
    let white = dds_texture(&r, false, [255; 4])?;
    let black = dds_texture(&r, false, [0, 0, 0, 255])?;
    let section = |cube, sun: &Texture| SkySection {
        cube,
        clouds: white.clone(),
        sun: Some(sun.clone()),
    };
    let sky = SkyTextures {
        sections: vec![
            section(dds_texture(&r, true, [0, 0, 0, 255])?, &white),
            section(dds_texture(&r, true, [26, 50, 100, 255])?, &white),
            section(dds_texture(&r, true, [52, 100, 200, 255])?, &black),
        ],
    };
    let g = geom(Layout::VertexLit, 0, 0, [0; 8]);
    let world = r
        .create_world(&g, &[], &[], Some(&sky))
        .map_err(fail("world"))?;
    ensure_eq!(world.sky_error(), None, "sky error");
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
    expect_near(
        centre(&mut r, frame(world.clone(), Some(sky_frame))).await?,
        expected,
        "sky",
    )?;
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
    expect_near(
        centre(&mut r, frame(world.clone(), Some(over))).await?,
        expected,
        "section 2's sun",
    )?;
    let white_sun = SkyFrame {
        sun_section: 1,
        ..over
    };
    expect_near(
        centre(&mut r, frame(world, Some(white_sun))).await?,
        [255; 3],
        "section 1's sun",
    )?;
    // A 1-face texture is not a cube map: no sky, said why.
    let mut flat = sky.clone();
    if let Some(s) = flat.sections.first_mut() {
        s.cube = white.clone();
    }
    let world = r
        .create_world(&g, &[], &[], Some(&flat))
        .map_err(fail("world"))?;
    ensure!(world.sky_error().is_some(), "a flat sky section accepted");
    Ok(String::new())
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
async fn object_pixel(
    r: &mut Renderer,
    objects: &ObjectGeometry,
    surface: Surface<Texture>,
    props: [f32; 4],
    light: impl FnOnce(&mut WorldLight),
) -> Result<[u8; 4], Problem> {
    object_pixel_at(r, objects, surface, props, light, 1.0).await
}

async fn object_pixel_at(
    r: &mut Renderer,
    objects: &ObjectGeometry,
    surface: Surface<Texture>,
    props: [f32; 4],
    light: impl FnOnce(&mut WorldLight),
    z: f32,
) -> Result<[u8; 4], Problem> {
    let g = geom(Layout::VertexLit, 0, 0, [0; 8]);
    let world = r
        .create_world_with(
            &g,
            objects,
            &[surface],
            &[object_draw_at(0, props, z)],
            None,
        )
        .map_err(fail("world"))?;
    ensure_eq!(
        (world.object_draw_count(), world.draw_count()),
        (1, 0),
        "object draws, draws"
    );
    ensure!(
        world.object_gpu_bytes() >= 48 + 8,
        "object bytes {}",
        world.object_gpu_bytes()
    );
    let mut f = frame(world, None);
    light(&mut f.light);
    centre(r, f).await
}

pub async fn objects_draw_quantized_models(make: Make<'_>) -> Outcome {
    let mut r = make();
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
    let px = object_pixel(&mut r, &up, model.clone(), [0.0, 0.0, 0.0, 0.5], hemi).await?;
    expect_near(px, [0, 153, 0], "model_def_lq, hemisphere")?;
    // The sun term: sunlight straight down on the upward normal, colour
    // 0.4, L_dynamic_props.xyz 0.5: Q = 0.5 · 0.4 = 0.2, C = 0.4 · B.
    let sun = |l: &mut WorldLight| {
        l.sun_color = [0.4; 3];
        l.sun_dir = [0.0, -1.0, 0.0];
    };
    let px = object_pixel(&mut r, &up, model.clone(), [0.5, 0.5, 0.5, 0.0], sun).await?;
    expect_near(px, [0, 102, 0], "model_def_lq, sun")?;
    // Lit from below, the sun adds nothing.
    let below = |l: &mut WorldLight| {
        l.sun_color = [0.4; 3];
        l.sun_dir = [0.0, 1.0, 0.0];
    };
    let px = object_pixel(&mut r, &up, model, [0.5, 0.5, 0.5, 0.0], below).await?;
    expect_near(px, [0, 0, 0], "model_def_lq, sun behind")?;
    // Self-lit: the texel.
    let px = object_pixel(
        &mut r,
        &up,
        surface(Program::ModelSelfLight, Some(base.clone())),
        [0.0; 4],
        |_| {},
    )
    .await?;
    expect_near(px, [0, 255, 0], "selflight")?;
    // Light planes, added (source alpha / one) onto black: B.rgb faded by
    // |view direction · normal| interpolated from the vertices, whose view
    // directions (−3, −3, 1), (−3, 9, 1), (9, −3, 1) meet the normal
    // facing the camera at 0.224, 0.107, 0.107; the centre pixel weighs
    // them 1/2, 1/4, 1/4: 0.166 · 255 = 42.
    let mut planes = surface(Program::ModelLightPlanes, Some(base));
    planes.blend = Blend::Add;
    let px = object_pixel(&mut r, &object_geometry(TOWARDS), planes, [0.0; 4], |_| {}).await?;
    expect_near(px, [0, 42, 0], "lightplanes")?;

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
        .map_err(fail("world"))?;
    ensure_eq!(world.skipped(), 2, "skipped");
    // An object draw beyond the objects' indices fails.
    let mut far = object_draw(1, [0.0; 4]);
    far.first_index = 3;
    ensure!(
        r.create_world_with(&g, &up, &surfaces, &[far], None)
            .is_err(),
        "an object draw beyond the indices accepted"
    );
    Ok(String::new())
}

/// Placing a draw after the world is made: hidden with a radius of 0,
/// shown again, moved (objects switching online and offline).
pub async fn placed_draws_hide_show_and_move(make: Make<'_>) -> Outcome {
    let mut r = make();
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
        .map_err(fail("world"))?;
    ensure_eq!(world.skipped(), 1, "skipped");
    let shown = draws[1];
    expect_near(
        centre(&mut r, frame(world.clone(), None)).await?,
        [0, 255, 0],
        "shown",
    )?;
    r.place_draw(&world, 1, &shown.transform, [0.0, 0.0, 1.0, 0.0]);
    expect_near(
        centre(&mut r, frame(world.clone(), None)).await?,
        [0, 0, 0],
        "hidden",
    )?;
    r.place_draw(&world, 1, &shown.transform, shown.sphere);
    expect_near(
        centre(&mut r, frame(world.clone(), None)).await?,
        [0, 255, 0],
        "shown again",
    )?;
    // Moved behind the eye: not in view.
    let mut behind = shown.transform;
    behind[3] = [0.0, 0.0, -5.0, 1.0];
    r.place_draw(&world, 1, &behind, [0.0, 0.0, -5.0, 10.0]);
    expect_near(
        centre(&mut r, frame(world.clone(), None)).await?,
        [0, 0, 0],
        "moved",
    )?;
    // A draw left out, or beyond the list, is ignored.
    r.place_draw(&world, 0, &IDENTITY, [0.0; 4]);
    r.place_draw(&world, 7, &IDENTITY, [0.0; 4]);
    Ok(String::new())
}

/// A cube map whose six 4 × 4 faces (+x, −x, +y, −y, +z, −z) each have
/// one colour (BGRA), so filtering near a face's centre stays on it.
fn faces_cube(r: &Renderer, faces: [[u8; 4]; 6]) -> Result<Texture, Problem> {
    let data: Vec<u8> = faces.iter().flat_map(|f| f.repeat(16)).collect();
    let file = file(4, 4, 0, true, PixelFormat::BGRA8, &data);
    let dds = Dds::parse(&file).map_err(|e| Problem::Fail(format!("dds: {e}")))?;
    r.create_texture(&dds)
        .map_err(|e| Problem::Fail(format!("texture: {e}")))
}

pub async fn environment_mapped_models(make: Make<'_>) -> Outcome {
    let mut r = make();
    // A base texel with alpha 0: the environment's texel shows:
    // C = 2 · Q · lerp(E, B, B.a) with Q = 0.5 (ambient).
    let base = r.create_rgba_texture(1, 1, &[255, 255, 255, 0]);
    let mut env = surface(Program::ModelEnv, Some(base));
    let colours: [[u8; 4]; 6] = std::array::from_fn(|i| [0, 40 * i as u8, 200, 255]);
    env.env = Some(faces_cube(&r, colours)?);
    let ambient = |l: &mut WorldLight| l.ambient = [0.5; 3];
    // At 40 m every view direction is near (0, 0, 1). Reflected about the
    // normal facing the camera it points back along −z: face 5. About the
    // upward normal it keeps going along +z: face 4. About a normal halfway
    // between the camera and ±x (±y) it points along ±x (±y): faces 0 – 3.
    // So every face's own texels show (on WebGL 2 too, where a cube can't
    // be drawn as a flat quad, #492).
    let normals = [
        ([255, 127, 0], 0, "+x"),
        ([0, 127, 0], 1, "−x"),
        ([127, 255, 0], 2, "+y"),
        ([127, 0, 0], 3, "−y"),
        (UP, 4, "past the upward normal"),
        (TOWARDS, 5, "towards the camera"),
    ];
    for (normal, face, what) in normals {
        let geometry = object_geometry(normal);
        let px = object_pixel_at(&mut r, &geometry, env.clone(), [0.0; 4], ambient, 40.0).await?;
        expect_near(px, [200, 40 * face, 0], &format!("reflected {what}"))?;
    }
    Ok(String::new())
}

/// A quad of particle-drawing.md 4.4 at depth `z` spanning `x0 … x1` and
/// `y0 … y1`, coloured `argb`: vertices X − A − B, X − A + B, X + A − B,
/// X + A + B (clockwise on screen, as a facing sprite).
fn sprite(x0: f32, x1: f32, y0: f32, y1: f32, z: f32, argb: u32) -> Vec<ParticleVertex> {
    [
        ([x0, y0, z], [0.0, 1.0]),
        ([x0, y1, z], [0.0, 0.0]),
        ([x1, y0, z], [1.0, 1.0]),
        ([x1, y1, z], [1.0, 0.0]),
    ]
    .into_iter()
    .map(|(position, uv)| ParticleVertex {
        position,
        colour: argb,
        uv,
    })
    .collect()
}

fn pass(program: ParticleProgram, blend: ParticleBlend, base: &Texture) -> ParticlePass {
    ParticlePass {
        program,
        blend,
        depth_write: program == ParticleProgram::Set,
        clamp: true,
        base: Some(base.clone()),
        distort: None,
    }
}

fn batch(
    pass: ParticlePass,
    queue: ParticleQueue,
    key: f32,
    vertices: Vec<ParticleVertex>,
) -> ParticleBatch {
    ParticleBatch {
        pass,
        queue,
        cull: ParticleCull::None,
        key,
        vertices,
    }
}

/// Pixels of `frame` at `size` × `size`.
async fn pixels(renderer: &mut Renderer, frame: WorldFrame, size: u32) -> Result<Vec<u8>, Problem> {
    let list = DrawList {
        world: Some(frame),
        ..Default::default()
    };
    let image = capture(renderer, &list, size, size).await?;
    ensure_eq!(image.len(), (size * size * 4) as usize, "captured bytes");
    Ok(image)
}

/// The colour channels at (`x`, `y`) of `pixels` `size` wide.
fn at(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 3] {
    rgb(pixel(pixels, size, x, y))
}

/// A black self-lit triangle at depth 1 behind everything.
fn black_world(r: &mut Renderer) -> Result<World, Problem> {
    let black = r.create_rgba_texture(1, 1, &[0, 0, 0, 255]);
    let g = geom(Layout::VertexLit, 0, 0, [0; 8]);
    r.create_world(
        &g,
        &[surface(Program::SelfLight, Some(black))],
        &[draw(0, None)],
        None,
    )
    .map_err(fail("world"))
}

/// detail-objects.md 6: an instance of a still model lit by the ambient
/// colour (texel × colour × 2), an instance whose texels fail the alpha
/// test cut away, both faces drawn, a swaying instance's colour within its
/// pulse's range, and an instance turned and scaled.
pub async fn detail_objects_are_cut_lit_and_swayed(make: Make<'_>) -> Outcome {
    let mut r = make();
    let world = black_world(&mut r)?;
    // Left texel opaque white, right texel white below the reference.
    let texture = r.create_rgba_texture(2, 1, &[255, 255, 255, 255, 255, 255, 255, 128]);
    // A unit quad standing on y = 0, x −0.5 … 0.5, its texels at u.
    let quad = |u: f32, clockwise: bool| {
        let v = [
            ([-0.5, 0.0, 0.0], [u, 0.5]),
            ([0.5, 0.0, 0.0], [u, 0.5]),
            ([-0.5, 1.0, 0.0], [u, 0.5]),
            ([0.5, 1.0, 0.0], [u, 0.5]),
        ];
        let i: &[u16] = if clockwise {
            &[0, 2, 1, 1, 2, 3]
        } else {
            &[0, 1, 2, 2, 1, 3]
        };
        DetailGeometry::new(&v, i)
    };
    r.set_details(
        &[quad(0.25, false), quad(0.75, false), quad(0.25, true)],
        &texture,
        true,
        16,
    );
    let size = 64;
    let at_ = |p: &[u8], x: u32, y: u32| at(p, size, x, y);
    let mut f = frame(world, None);
    f.light.ambient = [0.25; 3];
    // In front of the world's triangle (depth 1).
    let instance =
        |x: f32, scale: f32, turn: f32| [x, -0.2, 0.8, scale, turn.cos(), turn.sin(), 0.0, 0.0];
    let details = |instances: Vec<[f32; 8]>, draws: Vec<DetailDraw>| DetailFrame {
        instances: Arc::new(instances),
        draws,
        sway: [0.0, 0.0, 1.0, 1.0, 1.0],
        time: 0.0,
    };
    let one = |model, list, first| DetailDraw {
        model,
        list,
        first,
        count: 1,
    };
    // Model 0 at the centre, model 1 (cut) left, model 2 (wound the other
    // way: both faces drawn) right.
    f.details = Some(details(
        vec![
            instance(0.0, 0.4, 0.0),
            instance(-0.5, 0.4, 0.0),
            instance(0.5, 0.4, 0.0),
        ],
        vec![one(0, STILL, 0), one(1, STILL, 1), one(2, STILL, 2)],
    ));
    let p = pixels(&mut r, f.clone(), size).await?;
    expect_near(pixel(&p, size, 32, 30), [128, 128, 128], "ambient × 2")?;
    ensure_eq!(at_(&p, 8, 30), [0, 0, 0], "cut by the alpha test");
    expect_near(pixel(&p, size, 56, 30), [128, 128, 128], "the other face")?;
    // Turned a quarter about y, the quad is seen edge on: nothing at the
    // centre; scaled by 0.5 it no longer reaches the top.
    f.details = Some(details(
        vec![instance(0.0, 0.4, std::f32::consts::FRAC_PI_2)],
        vec![one(0, STILL, 0)],
    ));
    let p = pixels(&mut r, f.clone(), size).await?;
    ensure_eq!(at_(&p, 25, 30), [0, 0, 0], "edge on");
    f.details = Some(details(
        vec![instance(0.0, 0.2, 0.0)],
        vec![one(0, STILL, 0)],
    ));
    let p = pixels(&mut r, f.clone(), size).await?;
    ensure_eq!(at_(&p, 32, 28), [0, 0, 0], "half height");
    expect_near(pixel(&p, size, 32, 36), [128, 128, 128], "half height")?;
    // Sway 1 without amplitude: in place, the colour × (0.9 + 0.25
    // max(0, γ) φ).
    f.details = Some(details(
        vec![instance(0.0, 0.4, 0.0)],
        vec![one(0, SWAY_1, 0)],
    ));
    let p = pixels(&mut r, f, size).await?;
    let c = at_(&p, 32, 30)[0];
    ensure!((114..=147).contains(&c), "swaying colour {c}");
    Ok(String::new())
}

/// frame-presentation.md 1.1, 3.1, 3.5: at `r__supersample` 2 the world is
/// drawn into a target √2 times the screen per axis and reduced by one
/// bilinear sample per pixel; with the identity in force white stays white.
pub async fn the_presentation_reduces_a_supersampled_target(make: Make<'_>) -> Outcome {
    let mut r = make();
    let white = r.create_rgba_texture(1, 1, &[255; 4]);
    let world = black_world(&mut r)?;
    // A white sprite over the left half, its edge off the pixel grid.
    let left = batch(
        pass(ParticleProgram::Set, ParticleBlend::Off, &white),
        ParticleQueue::Early,
        1.0,
        sprite(-2.0, 0.013, -2.0, 2.0, 0.5, 0xFFFF_FFFF),
    );
    let size = 128;
    let mut f = frame(world.clone(), None);
    f.particles = vec![left.clone()];
    let straight = pixels(&mut r, f.clone(), size).await?;
    f.presentation = Presentation {
        supersample: 2,
        ..Default::default()
    };
    f.effects = Some(ScreenEffects::identity());
    let reduced = pixels(&mut r, f, size).await?;
    // 128 · √2 = 181.02: 181, made even.
    ensure_eq!(
        r.presentation_target(),
        Some(((180, 180), true)),
        "presentation target"
    );
    ensure_eq!(at(&reduced, size, 8, 64), [255, 255, 255], "left");
    ensure_eq!(at(&reduced, size, 120, 64), [0, 0, 0], "right");
    // The edge's pixel blends texels of both sides, where the screen's
    // own frame has one or the other.
    let edge: Vec<u8> = (60..72).map(|x| at(&reduced, size, x, 64)[0]).collect();
    ensure!(edge.iter().any(|&v| v > 0 && v < 255), "{edge:?}");
    let edge: Vec<u8> = (60..72).map(|x| at(&straight, size, x, 64)[0]).collect();
    ensure!(edge.iter().all(|&v| v == 0 || v == 255), "{edge:?}");
    Ok(String::new())
}

/// particle-drawing.md sections 4.5 – 7: blended effects back to front by
/// key, the `SET` alpha test, culling, and the distortion target composed
/// into the frame.
pub async fn particles_queue_blend_cull_and_distort(make: Make<'_>) -> Outcome {
    let mut r = make();
    let white = r.create_rgba_texture(1, 1, &[255; 4]);
    let world = black_world(&mut r)?;
    let blend = |key, argb| {
        batch(
            pass(ParticleProgram::Blended, ParticleBlend::Alpha, &white),
            ParticleQueue::Strict,
            key,
            sprite(-0.25, 0.25, -0.25, 0.25, 0.5, argb),
        )
    };
    // Half-transparent red farther (larger key) and green nearer, queued
    // nearer first: drawn back to front, green over red.
    let mut f = frame(world.clone(), None);
    f.particles = vec![blend(1.0, 0x8000_FF00), blend(2.0, 0x80FF_0000)];
    let p = pixels(&mut r, f, 64).await?;
    let c = pixel(&p, 64, 32, 32);
    // Red at half over black, then green at half over that: (64, 128, 0).
    expect_near(c, [64, 128, 0], "green over red")?;
    // `SET` passes alpha above 200 only (199 and 202 here: 200 itself rounds
    // either way in the unit); culling of clockwise faces drops
    // a facing sprite.
    let mut f = frame(world.clone(), None);
    let mut culled = blend(1.0, 0xFFFF_FFFF);
    culled.cull = ParticleCull::Clockwise;
    f.particles = vec![
        batch(
            pass(ParticleProgram::Set, ParticleBlend::Off, &white),
            ParticleQueue::Early,
            1.0,
            sprite(-0.25, 0.0, -0.25, 0.25, 0.5, 0xC7FF_FFFF),
        ),
        batch(
            pass(ParticleProgram::Set, ParticleBlend::Off, &white),
            ParticleQueue::Early,
            1.0,
            sprite(0.0, 0.25, -0.25, 0.25, 0.5, 0xCAFF_FFFF),
        ),
        culled,
    ];
    let p = pixels(&mut r, f, 64).await?;
    ensure_eq!(at(&p, 64, 28, 32), [0, 0, 0], "alpha 199 is cut");
    ensure_eq!(at(&p, 64, 36, 32), [255, 255, 255], "alpha 202 passes");
    let mut f = frame(world.clone(), None);
    let mut kept = blend(1.0, 0xFFFF_FFFF);
    kept.cull = ParticleCull::CounterClockwise;
    f.particles = vec![kept];
    let p = pixels(&mut r, f, 64).await?;
    ensure_eq!(
        at(&p, 64, 32, 32),
        [255, 255, 255],
        "counter-clockwise culling"
    );

    // Distortion: a white sprite over the left half; a distortion sprite
    // over the screen whose texel's red is 1 offsets every scene look-up
    // by (1 − 0.5) · 0.05 of the width to the right.
    let size = 256;
    let left = batch(
        pass(ParticleProgram::Set, ParticleBlend::Off, &white),
        ParticleQueue::Early,
        1.0,
        sprite(-2.0, 0.0, -2.0, 2.0, 0.5, 0xFFFF_FFFF),
    );
    let mut f = frame(world.clone(), None);
    f.particles = vec![left.clone()];
    let plain = pixels(&mut r, f, size).await?;
    ensure_eq!(at(&plain, size, 124, 128), [255, 255, 255], "plain");
    let red = r.create_rgba_texture(1, 1, &[255, 127, 0, 255]);
    let mut distort = pass(ParticleProgram::Distort, ParticleBlend::Alpha, &white);
    distort.distort = Some(red);
    let mut f = frame(world.clone(), None);
    f.particles = vec![
        left.clone(),
        batch(
            distort,
            ParticleQueue::Distortion,
            1.0,
            sprite(-2.0, 2.0, -2.0, 2.0, 0.4, 0xFFFF_FFFF),
        ),
    ];
    // Drawn straight to the screen (r__supersample 1, no effect), the
    // first frame's distortion is not shown; the next frame goes through
    // the target and is presented with postprocess_d
    // (frame-presentation.md 3.1, particle-drawing.md 6.3).
    let first = pixels(&mut r, f.clone(), size).await?;
    ensure_eq!(at(&first, size, 124, 128), [255, 255, 255], "not shown");
    ensure_eq!(
        r.presentation_target(),
        Some(((size, size), false)),
        "presentation target, first frame"
    );
    let shifted = pixels(&mut r, f, size).await?;
    ensure_eq!(
        r.presentation_target(),
        Some(((size, size), true)),
        "presentation target, next frame"
    );
    // About 6 pixels: at 124 the scene is read from 130, right of the edge.
    ensure_eq!(at(&shifted, size, 124, 128), [0, 0, 0], "shifted");
    // The renderer's own colour base (127, no hand-over) scales by
    // 254/255 (frame-presentation.md 3.5).
    ensure_eq!(at(&shifted, size, 112, 128), [254, 254, 254], "still white");
    // A frame after one with a distortion pass goes through the target;
    // the next goes straight to the screen again.
    let mut f = frame(world.clone(), None);
    f.particles = vec![left.clone()];
    pixels(&mut r, f.clone(), size).await?;
    ensure_eq!(
        r.presentation_target(),
        Some(((size, size), true)),
        "presentation target, the frame after"
    );
    let again = pixels(&mut r, f, size).await?;
    ensure_eq!(
        r.presentation_target(),
        Some(((size, size), false)),
        "presentation target, again"
    );
    ensure_eq!(at(&again, size, 124, 128), [255, 255, 255], "again");

    // A distortion list whose sprites all lie off the screen is presented
    // as one drawn that changes nothing (a black vertex colour: alpha 0),
    // without its pass (#433).
    let mut drawn: i64 = 0;
    let mut presented = Vec::new();
    for x in [-2.0, 3.0] {
        let mut distort = pass(ParticleProgram::Distort, ParticleBlend::Alpha, &white);
        distort.distort = Some(r.create_rgba_texture(1, 1, &[255, 127, 0, 255]));
        let mut f = frame(world.clone(), None);
        f.particles = vec![
            left.clone(),
            batch(
                distort,
                ParticleQueue::Distortion,
                1.0,
                sprite(x, x + 4.0, -2.0, 2.0, 0.4, 0xFF00_0000),
            ),
        ];
        pixels(&mut r, f.clone(), size).await?;
        r.take_frame_stats();
        presented.push(pixels(&mut r, f, size).await?);
        ensure_eq!(
            r.presentation_target(),
            Some(((size, size), true)),
            "presentation target, distortion at x {x}"
        );
        let passes = i64::from(r.take_frame_stats().passes);
        drawn = if x < 0.0 { passes } else { drawn - passes };
    }
    ensure_eq!(drawn, 1, "the distortion pass left out");
    ensure!(
        presented.first() == presented.get(1),
        "not the same picture"
    );
    Ok(String::new())
}

/// A light pass whose frustum holds everything (clip (0, 0, ½, 1)).
fn everywhere(spot: bool, position: [f32; 4], colour: [f32; 3]) -> LightPass {
    let mut frustum = [[0.0; 4]; 4];
    frustum[3] = [0.0, 0.0, 0.5, 1.0];
    LightPass {
        spot,
        position,
        colour: [colour[0], colour[1], colour[2], 1.0],
        xform: frustum,
        frustum,
    }
}

/// dynamic-lights.md 6: a light pass adds 2 · B.rgb · B.a · a₁ · a₂ · c
/// to the image, c the light's colour times n · −l; with white
/// attenuation textures a₁ = a₂ = 1. The alpha test, a hidden draw and a
/// frustum missing the draw leave the image as it is.
pub async fn light_passes_add_to_the_image(make: Make<'_>) -> Outcome {
    let mut r = make();
    let white = r.create_rgba_texture(1, 1, &[255; 4]);
    let half = r.create_rgba_texture(1, 1, &[255, 255, 255, 128]);
    // A black vertex-lit triangle at depth 1 with normal (0, 1, 0).
    let g = geom(Layout::VertexLit, 0, 0, [0; 8]);
    let lit = |base: &Texture, alpha_ref| Surface {
        light: Some(LightElement {
            program: LightProgram::VertexLit,
            alpha_ref,
            priority: 1,
        }),
        ..surface(Program::VertexLit, Some(base.clone()))
    };
    let surfaces = [
        lit(&white, 0),
        lit(&half, 0),
        lit(&half, 200),
        Surface {
            light: Some(LightElement {
                program: LightProgram::VertexLit,
                alpha_ref: 0,
                priority: 2,
            }),
            ..surface(Program::VertexLit, Some(white.clone()))
        },
    ];
    let world = |r: &mut Renderer, surface| {
        r.create_world(&g, &surfaces, &[draw(surface, None)], None)
            .map_err(fail("world"))
    };
    // A point light far above (lit per vertex): n · −l ≈ 1.
    let light = everywhere(false, [0.0, 1000.0, 1.0, 0.0625], [0.25, 0.125, 0.0]);
    let lit_frame = |w| {
        let mut f = frame(w, None);
        f.lights = vec![light];
        f
    };
    // Without the attenuation textures no light pass is drawn.
    let w = world(&mut r, 0)?;
    expect_near(
        centre(&mut r, lit_frame(w.clone())).await?,
        [0, 0, 0],
        "no textures",
    )?;
    r.set_light_textures(LightTextures {
        att: white.clone(),
        attclip: white.clone(),
        attpoint: white.clone(),
    });
    expect_near(
        centre(&mut r, frame(w.clone(), None)).await?,
        [0, 0, 0],
        "unlit",
    )?;
    expect_near(
        centre(&mut r, lit_frame(w.clone())).await?,
        [128, 64, 0],
        "point",
    )?;
    // Two passes add.
    let mut f = lit_frame(w.clone());
    f.lights.push(light);
    expect_near(centre(&mut r, f).await?, [255, 128, 0], "two passes")?;
    // B.a = ½ halves it; alpha 2 · ½ · c.a = 1 passes a reference of 200.
    let w = world(&mut r, 1)?;
    expect_near(
        centre(&mut r, lit_frame(w)).await?,
        [64, 32, 0],
        "half alpha",
    )?;
    let w = world(&mut r, 2)?;
    expect_near(
        centre(&mut r, lit_frame(w)).await?,
        [64, 32, 0],
        "alpha test passes",
    )?;
    // A light below the surface: n · −l < 0 adds nothing.
    let w = world(&mut r, 0)?;
    let mut f = frame(w.clone(), None);
    f.lights = vec![everywhere(false, [0.0, -1000.0, 1.0, 0.0625], [1.0; 3])];
    expect_near(centre(&mut r, f).await?, [0, 0, 0], "lit from behind")?;
    // A spot pass reads its projected coordinate: here (0, 0) / ½.
    let mut f = frame(w.clone(), None);
    f.lights = vec![everywhere(
        true,
        [0.0, 1000.0, 1.0, 0.125],
        [0.25, 0.125, 0.0],
    )];
    expect_near(centre(&mut r, f).await?, [128, 64, 0], "spot")?;
    // A frustum the draw's sphere misses.
    let mut away = light;
    away.frustum[3] = [0.0, 0.0, -1.0, 1.0];
    let mut f = frame(w.clone(), None);
    f.lights = vec![away];
    expect_near(
        centre(&mut r, f).await?,
        [0, 0, 0],
        "outside the light's frustum",
    )?;
    // A hidden draw (radius 0) is not lit.
    r.place_draw(&w, 0, &IDENTITY, [0.0, 0.0, 1.0, 0.0]);
    expect_near(centre(&mut r, lit_frame(w)).await?, [0, 0, 0], "hidden")?;
    // Priority 2 elements are never lit.
    let w = world(&mut r, 3)?;
    expect_near(centre(&mut r, lit_frame(w)).await?, [0, 0, 0], "priority 2")?;
    Ok(String::new())
}

/// glows.md 4, 6: glows add texel · b/255 · A after the strict-sorted
/// passes; `effects\glow` is hidden by nearer geometry, `effects\flare`
/// is not.
pub async fn glows_add_after_the_world(make: Make<'_>) -> Outcome {
    let mut r = make();
    let white = r.create_rgba_texture(1, 1, &[255; 4]);
    let world = black_world(&mut r)?;
    let glow = |program, z| ParticleBatch {
        pass: ParticlePass {
            program,
            blend: ParticleBlend::AlphaAdd,
            depth_write: false,
            clamp: true,
            base: Some(white.clone()),
            distort: None,
        },
        queue: ParticleQueue::Glow,
        cull: ParticleCull::CounterClockwise,
        key: 0.0,
        // b = 128 in all four channels: 128/255 · 128/255 added.
        vertices: sprite(-0.25, 0.25, -0.25, 0.25, z, 0x8080_8080),
    };
    let mut f = frame(world.clone(), None);
    f.particles = vec![glow(ParticleProgram::Glow, 0.5)];
    expect_near(centre(&mut r, f).await?, [64, 64, 64], "a glow in front")?;
    let mut f = frame(world.clone(), None);
    f.particles = vec![glow(ParticleProgram::Glow, 2.0)];
    expect_near(
        centre(&mut r, f).await?,
        [0, 0, 0],
        "a glow behind the triangle",
    )?;
    let mut f = frame(world, None);
    f.particles = vec![glow(ParticleProgram::Flare, 2.0)];
    expect_near(
        centre(&mut r, f).await?,
        [64, 64, 64],
        "a flare behind the triangle",
    )?;
    Ok(String::new())
}
