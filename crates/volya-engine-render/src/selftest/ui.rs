// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The UI layer's scenes: quads, textures by every path, the gamma ramp,
//! the scaled fit, GPU timing and video frames.

use volya_engine_formats::dds::Dds;
use volya_engine_formats::dds::build::{PixelFormat, file};
use volya_engine_formats::video::{Chroma, Frame, Plane};

use super::{
    Make, Outcome, Problem, capture, capture_scaled, ensure, ensure_eq, ensure_near, now_ms, pixel,
};
use crate::{DrawList, Quad, Rect, Renderer, TexturePath};

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255; 4];
const OPAQUE_BLACK: [u8; 4] = [0, 0, 0, 255];

/// An `n` × `n` RGBA image whose four quadrants are `colors` (top-left,
/// top-right, bottom-left, bottom-right).
fn quadrants(n: u32, colors: [[u8; 4]; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    for y in 0..n {
        for x in 0..n {
            let i = usize::from(x >= n / 2) + 2 * usize::from(y >= n / 2);
            out.extend_from_slice(&colors[i]);
        }
    }
    out
}

fn dds(bytes: &[u8]) -> Result<Dds<'_>, Problem> {
    Dds::parse(bytes).map_err(|e| Problem::Fail(format!("DDS: {e}")))
}

fn texture(renderer: &Renderer, bytes: &[u8]) -> Result<crate::Texture, Problem> {
    renderer
        .create_texture(&dds(bytes)?)
        .map_err(|e| Problem::Fail(format!("texture: {e}")))
}

pub async fn empty_list_clears(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    let image = capture(&mut renderer, &DrawList::default(), 8, 6).await?;
    ensure!(
        image.chunks(4).all(|p| p == OPAQUE_BLACK),
        "not black: {:?}",
        &image[..8]
    );
    Ok(String::new())
}

pub async fn texels_map_one_to_one_and_sub_rectangles_select(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    let texture = renderer.create_rgba_texture(8, 8, &quadrants(8, [RED, GREEN, BLUE, WHITE]));
    // Output 128 × 96: one pixel per 8 UI units on both axes.
    let mut list = DrawList::default();
    list.quads
        .push(Quad::new(Rect::new(0.0, 0.0, 64.0, 64.0), &texture));
    // The top-right quadrant only, stretched to 4 × 4 → 16 × 16 pixels.
    let mut sub = Quad::new(Rect::new(512.0, 384.0, 128.0, 128.0), &texture);
    sub.source = Rect::new(4.0, 0.0, 4.0, 4.0);
    list.quads.push(sub);
    let image = capture(&mut renderer, &list, 128, 96).await?;
    for (x, y, expected) in [(0, 0, RED), (7, 0, GREEN), (0, 7, BLUE), (7, 7, WHITE)] {
        ensure_eq!(pixel(&image, 128, x, y), expected, "texel ({x}, {y})");
    }
    ensure_eq!(pixel(&image, 128, 8, 8), OPAQUE_BLACK, "outside");
    // Bilinear magnification blends in the neighbouring texels at the
    // region's edge; the interior is the region's colour.
    for (x, y) in [(66, 50), (77, 50), (66, 61), (77, 61), (71, 55)] {
        ensure_eq!(
            pixel(&image, 128, x, y),
            GREEN,
            "sub-rectangle at ({x}, {y})"
        );
    }
    ensure_eq!(pixel(&image, 128, 80, 64), OPAQUE_BLACK, "beyond it");
    Ok(String::new())
}

pub async fn corners_place_a_rotated_quad(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    let texture = renderer.create_rgba_texture(2, 2, &quadrants(2, [RED, GREEN, BLUE, WHITE]));
    // A quarter turn counter-clockwise: the texture's top-left lands at the
    // bottom-left, its top-right at the top-left.
    let mut quad = Quad::new(Rect::SCREEN, &texture);
    quad.corners = Some([[0.0, 768.0], [0.0, 0.0], [1024.0, 0.0], [1024.0, 768.0]]);
    let mut list = DrawList::default();
    list.quads.push(quad);
    let image = capture(&mut renderer, &list, 16, 16).await?;
    for (x, y, expected) in [(1, 14, RED), (1, 1, GREEN), (14, 14, BLUE), (14, 1, WHITE)] {
        ensure_eq!(pixel(&image, 16, x, y), expected, "({x}, {y})");
    }
    Ok(String::new())
}

pub async fn axes_scale_independently(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    let texture = renderer.create_rgba_texture(1, 1, &RED);
    let mut list = DrawList::default();
    list.quads
        .push(Quad::new(Rect::new(512.0, 384.0, 512.0, 384.0), &texture));
    // 4× wider than tall: the UI space stretches, no letterboxing.
    let (w, h) = (256, 48);
    let image = capture(&mut renderer, &list, w, h).await?;
    for y in 0..h {
        for x in 0..w {
            let inside = x >= 128 && y >= 24;
            let expected = if inside { RED } else { OPAQUE_BLACK };
            ensure_eq!(pixel(&image, w, x, y), expected, "({x}, {y})");
        }
    }
    Ok(String::new())
}

pub async fn a_smaller_frame_fits_the_target_with_its_aspect_kept(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    let texture = renderer.create_rgba_texture(1, 1, &RED);
    let mut list = DrawList::default();
    list.quads.push(Quad::new(Rect::SCREEN, &texture));
    // A 4:3 frame of 32 × 24 in a 64 × 24 target: black bars of 16 at the
    // sides (#49).
    let (w, h) = (64, 24);
    let image = capture_scaled(&mut renderer, &list, (32, 24), w, h).await?;
    for y in 0..h {
        for x in 0..w {
            let expected = if (16..48).contains(&x) {
                RED
            } else {
                OPAQUE_BLACK
            };
            ensure_eq!(pixel(&image, w, x, y), expected, "({x}, {y})");
        }
    }
    // At the target's own size it draws straight into it.
    let image = capture_scaled(&mut renderer, &list, (w, h), w, h).await?;
    ensure!(image.chunks(4).all(|p| p == RED), "at its own size");
    Ok(String::new())
}

pub async fn the_gamma_ramp_maps_every_channel_of_the_whole_screen(make: Make<'_>) -> Outcome {
    // display-gamma.md 2.1: brightness 1.5 lifts 0 to 8192, 128 to 41088
    // (entries / 65535 shown on an 8-bit target: 32 and 160); 255 stays.
    // At gamma and contrast 1.0 the table is 257 i + 8192, clamped.
    let table: [u16; 256] = std::array::from_fn(|i| (257 * i as u32 + 8192).min(65535) as u16);
    let mut renderer = make();
    let texture = renderer.create_rgba_texture(2, 1, &[128, 0, 255, 255, 0, 64, 192, 255]);
    let mut list = DrawList {
        ramp: Some(table),
        ..Default::default()
    };
    list.quads.push(Quad::new(Rect::SCREEN, &texture));
    let shown = |v: u8| (f32::from(table[usize::from(v)]) / 65535.0 * 255.0).round() as u8;
    let left = [shown(128), shown(0), shown(255), 255];
    let right = [shown(0), shown(64), shown(192), 255];
    ensure_eq!(left, [160, 32, 255, 255], "the table");
    let image = capture(&mut renderer, &list, 8, 4).await?;
    ensure_eq!(pixel(&image, 8, 0, 0), left, "left");
    ensure_eq!(pixel(&image, 8, 7, 3), right, "right");
    // Scaled to fit, the black bars are shown through the ramp too: the
    // ramp acts on the entire screen (section 2).
    let image = capture_scaled(&mut renderer, &list, (8, 4), 16, 4).await?;
    ensure_eq!(pixel(&image, 16, 0, 0), [32, 32, 32, 255], "a bar");
    ensure_eq!(pixel(&image, 16, 4, 0), left, "scaled left");
    ensure_eq!(pixel(&image, 16, 11, 3), right, "scaled right");
    Ok(String::new())
}

pub async fn frames_are_timed_on_the_gpu_while_timing_is_on(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    if !renderer.can_time_gpu() {
        return Err(Problem::Skip("the renderer can't time the GPU here".into()));
    }
    let texture = renderer.create_rgba_texture(1, 1, &RED);
    let mut list = DrawList::default();
    list.quads.push(Quad::new(Rect::SCREEN, &texture));
    async fn frames(renderer: &mut Renderer, list: &DrawList) -> Result<Vec<f64>, Problem> {
        for _ in 0..8 {
            capture_scaled(renderer, list, (32, 24), 64, 48).await?;
        }
        // Read back once more: the GPU is done with the frames.
        capture(renderer, &DrawList::default(), 1, 1).await?;
        Ok(renderer.take_gpu_times())
    }
    ensure!(
        frames(&mut renderer, &list).await?.is_empty(),
        "off by default"
    );
    renderer.set_gpu_timing(true);
    // Not every frame: a GPU may leave some frames' last timestamp zero
    // (measured on Metal), and those are left out.
    let times = frames(&mut renderer, &list).await?;
    ensure!(
        !times.is_empty() && times.len() <= 9,
        "frames timed: {times:?}"
    );
    ensure!(times.iter().all(|&ms| ms > 0.0 && ms < 1000.0), "{times:?}");
    // A frame's time is its passes' (#441).
    let passes: f64 = renderer.take_gpu_pass_times().iter().map(|p| p.1).sum();
    let total: f64 = times.iter().sum();
    ensure!((total - passes).abs() < 1e-9, "{total} {passes}");
    renderer.set_gpu_timing(false);
    ensure!(frames(&mut renderer, &list).await?.is_empty(), "off again");
    Ok(format!("{} frames timed", times.len()))
}

pub async fn draws_in_order_with_tint_and_alpha(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    let red = renderer.create_rgba_texture(1, 1, &RED);
    let white = renderer.create_rgba_texture(1, 1, &[255, 255, 255, 128]);
    let mut list = DrawList::default();
    list.quads.push(Quad::new(Rect::SCREEN, &red));
    // Half-transparent texel, tinted blue at full alpha: 50 % blue over red.
    let mut blue = Quad::new(Rect::new(0.0, 0.0, 512.0, 768.0), &white);
    blue.color = [0, 0, 255, 255];
    list.quads.push(blue);
    // Opaque white, with the tint's alpha at 25 %.
    let mut faint = Quad::new(Rect::new(512.0, 0.0, 512.0, 768.0), &red);
    faint.color = [255, 255, 255, 64];
    list.quads.push(faint);
    let mut green = Quad::new(Rect::new(512.0, 0.0, 512.0, 768.0), &white);
    green.color = [0, 255, 0, 64];
    list.quads.push(green);
    let image = capture(&mut renderer, &list, 4, 4).await?;
    ensure_near!(
        pixel(&image, 4, 0, 0),
        [127, 0, 128, 255],
        1,
        "blue over red"
    );
    // Red at 25 % over red stays red; then green at 25 % × 50 % over it.
    ensure_near!(
        pixel(&image, 4, 3, 3),
        [223, 32, 0, 255],
        1,
        "faint green over red"
    );
    Ok(String::new())
}

/// A 4 × 4 BC1 block of one colour (RGB565 `color0`, every index 0).
fn bc1_block(color: u16) -> [u8; 8] {
    let [a, b] = color.to_le_bytes();
    [a, b, 0, 0, 0, 0, 0, 0]
}

/// BC textures by each path (#113): as stored where the device has BC,
/// transcoded to ETC2 where it has that, decoded to RGBA8 everywhere;
/// the same colours (exact, or within ETC2's error). A path the device
/// lacks is left out.
pub async fn bc_textures_by_every_path(make: Make<'_>) -> Outcome {
    let mut drawn = Vec::new();
    for (path, formats, tolerance) in [
        (TexturePath::Bc, ["bc1", "bc2", "bc3"], 0),
        (
            TexturePath::Etc2,
            ["etc2-rgb8", "etc2-rgba8", "etc2-rgba8"],
            4,
        ),
        (TexturePath::Rgba8, ["rgba8"; 3], 0),
    ] {
        let mut renderer = make();
        if !renderer.set_texture_path(path) {
            continue;
        }
        check_bc_formats(&mut renderer, formats, tolerance)
            .await
            .map_err(|e| match e {
                Problem::Fail(m) => Problem::Fail(format!("{}: {m}", path.name())),
                skip => skip,
            })?;
        drawn.push(path.name());
    }
    Ok(drawn.join(", "))
}

async fn check_bc_formats(
    renderer: &mut Renderer,
    formats: [&str; 3],
    tolerance: u8,
) -> Result<(), Problem> {
    // BC1: 8 × 8, 2 × 2 blocks: red, green / blue, white.
    let blocks: Vec<u8> = [0xf800, 0x07e0, 0x001f, 0xffff]
        .into_iter()
        .flat_map(bc1_block)
        .collect();
    let bc1 = file(8, 8, 0, false, PixelFormat::FourCc(b"DXT1"), &blocks);
    // BC2: explicit alpha 4 bits per texel (0x8 → 136), colour block red.
    let mut bc2 = vec![0x88; 8];
    bc2.extend_from_slice(&bc1_block(0xf800));
    let bc2 = file(4, 4, 0, false, PixelFormat::FourCc(b"DXT3"), &bc2);
    // BC3: alpha endpoints 64 and 0, every index 0 → 64; colour green.
    let mut bc3 = vec![64, 0, 0, 0, 0, 0, 0, 0];
    bc3.extend_from_slice(&bc1_block(0x07e0));
    let bc3 = file(4, 4, 0, false, PixelFormat::FourCc(b"DXT5"), &bc3);

    let mut list = DrawList::default();
    let rects = [
        Rect::new(0.0, 0.0, 512.0, 768.0),
        Rect::new(512.0, 0.0, 256.0, 768.0),
        Rect::new(768.0, 0.0, 256.0, 768.0),
    ];
    for ((bytes, format), rect) in [bc1, bc2, bc3].iter().zip(formats).zip(rects) {
        let texture = texture(renderer, bytes)?;
        ensure_eq!(texture.format_name(), format, "format");
        list.quads.push(Quad::new(rect, &texture));
    }
    let image = capture(renderer, &list, 16, 8).await?;
    for (x, y, expected) in [(0, 0, RED), (7, 0, GREEN), (0, 7, BLUE), (7, 7, WHITE)] {
        ensure_near!(
            pixel(&image, 16, x, y),
            expected,
            tolerance,
            "BC1 ({x}, {y})"
        );
    }
    ensure_near!(
        pixel(&image, 16, 9, 3),
        [136, 0, 0, 255],
        1 + tolerance,
        "BC2 red at 136/255"
    );
    ensure_near!(
        pixel(&image, 16, 13, 3),
        [0, 64, 0, 255],
        1 + tolerance,
        "BC3 green at 64/255"
    );
    Ok(())
}

/// A level larger than an upload piece is written in bands of rows (#357):
/// each band lands where it belongs.
pub async fn a_large_level_is_written_in_bands(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    // BC1, 4096 × 4096 (8 MiB): four horizontal stripes of 1024 rows.
    let colours = [0xf800, 0x07e0, 0x001f, 0xffff];
    let mut blocks = Vec::with_capacity(4096 * 4096 / 2);
    for row in 0..1024 {
        for _ in 0..1024 {
            blocks.extend_from_slice(&bc1_block(colours[row / 256]));
        }
    }
    let bc1 = file(4096, 4096, 0, false, PixelFormat::FourCc(b"DXT1"), &blocks);
    let texture = texture(&renderer, &bc1)?;
    let mut list = DrawList::default();
    list.quads
        .push(Quad::new(Rect::new(0.0, 0.0, 1024.0, 768.0), &texture));
    let image = capture(&mut renderer, &list, 16, 16).await?;
    for (y, expected) in [(2, RED), (6, GREEN), (10, BLUE), (14, WHITE)] {
        ensure_eq!(pixel(&image, 16, 8, y), expected, "row {y}");
    }
    Ok(renderer.texture_path().name().into())
}

pub async fn converts_uncompressed_formats_and_uploads_mips_and_cube_faces(
    make: Make<'_>,
) -> Outcome {
    let mut renderer = make();
    // RGB565, 2 × 2, mip 1 (1 × 1) a different colour; drawn at half size the
    // sampler picks mip 1.
    let mut data: Vec<u8> = [0xf800u16; 4]
        .iter()
        .flat_map(|t| t.to_le_bytes())
        .collect();
    data.extend_from_slice(&0x001fu16.to_le_bytes());
    let mips = file(2, 2, 2, false, PixelFormat::RGB565, &data);
    let mips = texture(&renderer, &mips)?;
    ensure_eq!(mips.mip_levels(), 2, "mip levels");
    // A8: alpha only, over the white background → grey.
    let a8 = file(1, 1, 0, false, PixelFormat::A8, &[128]);
    let a8 = texture(&renderer, &a8)?;
    // Cube map, BGRA8 1 × 1 per face: six layers. The game samples cube
    // maps only as such (every one is a sky or environment map, #492), so
    // their faces' texels are checked by world.rs's environment check.
    let faces: Vec<u8> = (0..6).flat_map(|i| [0, 0, 200 - 30 * i, 255]).collect();
    let cube = file(1, 1, 0, true, PixelFormat::BGRA8, &faces);
    let cube = texture(&renderer, &cube)?;
    ensure_eq!(cube.layers(), 6, "cube faces");
    let white = renderer.create_rgba_texture(1, 1, &WHITE);

    let mut list = DrawList::default();
    // 2 × 2 texels magnified into 4 × 4 pixels, and minified into 1 pixel.
    list.quads
        .push(Quad::new(Rect::new(0.0, 0.0, 256.0, 384.0), &mips));
    list.quads
        .push(Quad::new(Rect::new(256.0, 0.0, 64.0, 96.0), &mips));
    list.quads
        .push(Quad::new(Rect::new(512.0, 0.0, 256.0, 384.0), &white));
    list.quads
        .push(Quad::new(Rect::new(512.0, 0.0, 256.0, 384.0), &a8));
    let image = capture(&mut renderer, &list, 16, 8).await?;
    ensure_eq!(pixel(&image, 16, 1, 1), RED, "RGB565 mip 0");
    ensure_eq!(pixel(&image, 16, 4, 0), BLUE, "RGB565 mip 1");
    ensure_near!(
        pixel(&image, 16, 9, 1),
        [127, 127, 127, 255],
        1,
        "A8 over white"
    );
    Ok(String::new())
}

/// Reference conversion: the pixel-shader path of docs/behavior/startup.md
/// 6.6 (BT.601, limited range), on 0…1 values.
fn yuv_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 4] {
    let [y, u, v] = [y, cb, cr].map(|c| f32::from(c) / 255.0);
    let channel = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    [
        channel(1.16406 * y + 1.59765 * v - 0.86961),
        channel(1.16406 * y - 0.390625 * u - 0.8125 * v + 0.53076),
        channel(1.16406 * y + 2.01562 * u - 1.0786),
        255,
    ]
}

/// A synthetic 4:2:0 coded frame larger than its picture, with the
/// picture offset in the header pointing away from the top-left corner.
struct SyntheticFrame {
    width: usize,
    height: usize,
    planes: [Vec<u8>; 3],
}

impl SyntheticFrame {
    /// Extra coded columns and rows beyond the picture.
    const PAD: usize = 5;

    fn coded(&self) -> (usize, usize) {
        (self.width + Self::PAD, self.height + Self::PAD)
    }

    fn new(width: usize, height: usize, seed: u8) -> Self {
        let (fw, fh) = (width + Self::PAD, height + Self::PAD);
        let (cw, ch) = (fw.div_ceil(2), fh.div_ceil(2));
        let plane = |w: usize, h: usize, k: u8| -> Vec<u8> {
            (0..w * h)
                .map(|i| ((i % w) * 37 + (i / w) * 11) as u8 ^ k ^ seed)
                .collect()
        };
        Self {
            width,
            height,
            planes: [plane(fw, fh, 0), plane(cw, ch, 0x55), plane(cw, ch, 0xaa)],
        }
    }

    fn frame(&self) -> Frame<'_> {
        let (fw, fh) = self.coded();
        let (cw, ch) = (fw.div_ceil(2), fh.div_ceil(2));
        let sizes = [(fw, fh), (cw, ch), (cw, ch)];
        let coded = [0, 1, 2].map(|i| Plane {
            width: sizes[i].0,
            height: sizes[i].1,
            stride: sizes[i].0,
            data: &self.planes[i],
        });
        Frame {
            number: 0,
            coded,
            chroma: Chroma::Yuv420,
            width: self.width as u32,
            height: self.height as u32,
            pic_x: 2,
            pic_y: 4,
        }
    }

    /// Texel (x, y): coded luma (x, y), chroma (⌊x / 2⌋, ⌊y / 2⌋).
    fn expected(&self, x: usize, y: usize) -> [u8; 4] {
        let f = self.frame();
        let at = |p: &Plane, x: usize, y: usize| p.data[y * p.stride + x];
        yuv_to_rgb(
            at(&f.coded[0], x, y),
            at(&f.coded[1], x / 2, y / 2),
            at(&f.coded[2], x / 2, y / 2),
        )
    }
}

pub async fn video_frames_map_to_the_top_left_of_a_power_of_two_texture(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    let (w, h) = (6, 3);
    let mut video = renderer.create_video_texture(w as u32, h as u32);
    ensure_eq!(video.texture().size(), (8, 4), "texture size");
    ensure_eq!(
        video.frame_rect(),
        Rect::new(0.0, 0.0, 6.0, 3.0),
        "frame rectangle"
    );
    for seed in [0, 1] {
        let synthetic = SyntheticFrame::new(w, h, seed);
        renderer.update_video(&mut video, &synthetic.frame());
        // The whole texture onto an 8 × 4 target: one pixel per texel.
        let mut list = DrawList::default();
        list.quads.push(Quad::new(Rect::SCREEN, video.texture()));
        let image = capture(&mut renderer, &list, 8, 4).await?;
        for y in 0..4 {
            for x in 0..8 {
                let expected = if x < w && y < h {
                    synthetic.expected(x, y)
                } else {
                    // Never written: Y'CbCr black (our choice).
                    yuv_to_rgb(16, 128, 128)
                };
                let actual = pixel(&image, 8, x as u32, y as u32);
                ensure_near!(actual, expected, 1, "frame {seed} ({x}, {y})");
            }
        }
        // The quad's colour and alpha are not applied (startup.md 6.6).
        list.quads[0].color = [0, 0, 0, 0];
        ensure!(
            capture(&mut renderer, &list, 8, 4).await? == image,
            "frame {seed} tinted"
        );
    }
    Ok(String::new())
}

/// Cost of one title-menu-sized frame (876 × 400): GPU upload and
/// conversion against converting on the CPU, reported.
pub async fn video_conversion_cost(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    let synthetic = SyntheticFrame::new(876, 400, 7);
    let frame = synthetic.frame();
    let mut video = renderer.create_video_texture(876, 400);
    renderer.update_video(&mut video, &frame);
    // A readback waits until the GPU is done.
    capture(&mut renderer, &DrawList::default(), 1, 1).await?;
    const N: u32 = 20;
    let start = now_ms();
    for _ in 0..N {
        renderer.update_video(&mut video, &frame);
    }
    let submit = (now_ms() - start) / f64::from(N);
    capture(&mut renderer, &DrawList::default(), 1, 1).await?;
    let gpu = (now_ms() - start) / f64::from(N);
    let start = now_ms();
    let mut rgba = vec![0; 876 * 400 * 4];
    for _ in 0..N {
        let [luma, cb, cr] = &frame.texture_planes();
        for (y, out) in rgba.as_chunks_mut::<{ 876 * 4 }>().0.iter_mut().enumerate() {
            let (l, b, r) = (luma.row(y), cb.row(y / 2), cr.row(y / 2));
            for (x, pixel) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                pixel.copy_from_slice(&yuv_to_rgb(l[x], b[x / 2], r[x / 2]));
            }
        }
    }
    std::hint::black_box(&rgba);
    let cpu = (now_ms() - start) / f64::from(N);
    Ok(format!(
        "876×400 frame: GPU path {submit:.2} ms ({gpu:.2} ms with the GPU's wait), CPU conversion {cpu:.2} ms"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The older-hardware integer path of startup.md 6.6, which the spec
    /// calls equivalent: the reference must agree with it.
    fn yuv_to_rgb_integer(y: u8, cb: u8, cr: u8) -> [u8; 4] {
        let c = 298 * (i32::from(y) - 16);
        let (d, e) = (i32::from(cb) - 128, i32::from(cr) - 128);
        let channel = |v: i32| (v >> 8).clamp(0, 255) as u8;
        [
            channel(c + 409 * e + 128),
            channel(c - 100 * d - 208 * e + 128),
            channel(c + 516 * d + 128),
            255,
        ]
    }

    #[test]
    fn conversion_paths_agree_and_zero_is_green() {
        for y in (0..=255).step_by(5) {
            for cb in (0..=255).step_by(15) {
                for cr in (0..=255).step_by(15) {
                    let (a, b) = (yuv_to_rgb(y, cb, cr), yuv_to_rgb_integer(y, cb, cr));
                    assert!(super::super::near(a, b, 2), "({y}, {cb}, {cr})");
                }
            }
        }
        assert_eq!(yuv_to_rgb_integer(0, 0, 0), [0, 135, 0, 255]);
        assert_eq!(yuv_to_rgb(0, 0, 0), [0, 135, 0, 255]);
    }
}
