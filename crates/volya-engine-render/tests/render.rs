// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Offscreen renders of synthetic textures, read back and compared pixel by
//! pixel (docs/testing.md, Rendering). Runs on the Mac's Metal adapter.

// Needs a native GPU.
#![cfg(not(target_arch = "wasm32"))]

use std::sync::OnceLock;
use std::time::Instant;

use volya_engine_formats::dds::Dds;
use volya_engine_formats::dds::build::{PixelFormat, file};
use volya_engine_formats::video::{Chroma, Frame, Plane};
use volya_engine_render::wgpu;
use volya_engine_render::{DrawList, Quad, Rect, Renderer, TexturePath};

const BLACK: wgpu::Color = wgpu::Color::BLACK;

/// The device (with BC support), shared by the tests.
fn gpu() -> &'static (wgpu::Device, wgpu::Queue) {
    static GPU: OnceLock<(wgpu::Device, wgpu::Queue)> = OnceLock::new();
    GPU.get_or_init(|| device_with(wgpu::Features::all()))
}

/// A device with the renderer's features within `allowed`.
fn device_with(allowed: wgpu::Features) -> (wgpu::Device, wgpu::Queue) {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&Default::default())
            .await
            .expect("a GPU adapter (Metal on the Mac)");
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: volya_engine_render::device_features(&adapter) & allowed,
                ..Default::default()
            })
            .await
            .unwrap()
    })
}

fn renderer() -> Renderer {
    let (device, queue) = gpu();
    Renderer::new(device, queue, wgpu::TextureFormat::Bgra8Unorm)
}

fn capture(renderer: &mut Renderer, list: &DrawList, width: u32, height: u32) -> Vec<u8> {
    pollster::block_on(renderer.capture(list, width, height, BLACK)).unwrap()
}

fn pixel(image: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    image[i..i + 4].try_into().unwrap()
}

fn assert_near(actual: [u8; 4], expected: [u8; 4], tolerance: u8, what: &str) {
    let close = actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= tolerance);
    assert!(close, "{what}: {actual:?} != {expected:?} (±{tolerance})");
}

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

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255; 4];

#[test]
fn empty_list_clears() {
    let mut renderer = renderer();
    let image = capture(&mut renderer, &DrawList::default(), 8, 6);
    assert!(image.chunks(4).all(|p| p == [0, 0, 0, 255]));
}

#[test]
fn texels_map_one_to_one_and_sub_rectangles_select() {
    let mut renderer = renderer();
    let texture = renderer.create_rgba_texture(8, 8, &quadrants(8, [RED, GREEN, BLUE, WHITE]));
    // Output 128 × 96: one pixel per 8 UI units on both axes.
    let mut list = DrawList::default();
    list.quads
        .push(Quad::new(Rect::new(0.0, 0.0, 64.0, 64.0), &texture));
    // The top-right quadrant only, stretched to 4 × 4 → 16 × 16 pixels.
    let mut sub = Quad::new(Rect::new(512.0, 384.0, 128.0, 128.0), &texture);
    sub.source = Rect::new(4.0, 0.0, 4.0, 4.0);
    list.quads.push(sub);
    let image = capture(&mut renderer, &list, 128, 96);
    for (x, y, expected) in [(0, 0, RED), (7, 0, GREEN), (0, 7, BLUE), (7, 7, WHITE)] {
        assert_eq!(pixel(&image, 128, x, y), expected, "texel ({x}, {y})");
    }
    assert_eq!(pixel(&image, 128, 8, 8), [0, 0, 0, 255], "outside");
    // Bilinear magnification blends in the neighbouring texels at the
    // region's edge; the interior is the region's colour.
    for (x, y) in [(66, 50), (77, 50), (66, 61), (77, 61), (71, 55)] {
        assert_eq!(
            pixel(&image, 128, x, y),
            GREEN,
            "sub-rectangle at ({x}, {y})"
        );
    }
    assert_eq!(pixel(&image, 128, 80, 64), [0, 0, 0, 255]);
}

#[test]
fn corners_place_a_rotated_quad() {
    let mut renderer = renderer();
    let texture = renderer.create_rgba_texture(2, 2, &quadrants(2, [RED, GREEN, BLUE, WHITE]));
    // A quarter turn counter-clockwise: the texture's top-left lands at the
    // bottom-left, its top-right at the top-left.
    let mut quad = Quad::new(Rect::SCREEN, &texture);
    quad.corners = Some([[0.0, 768.0], [0.0, 0.0], [1024.0, 0.0], [1024.0, 768.0]]);
    let mut list = DrawList::default();
    list.quads.push(quad);
    let image = capture(&mut renderer, &list, 16, 16);
    for (x, y, expected) in [(1, 14, RED), (1, 1, GREEN), (14, 14, BLUE), (14, 1, WHITE)] {
        assert_eq!(pixel(&image, 16, x, y), expected, "({x}, {y})");
    }
}

#[test]
fn axes_scale_independently() {
    let mut renderer = renderer();
    let texture = renderer.create_rgba_texture(1, 1, &RED);
    let mut list = DrawList::default();
    list.quads
        .push(Quad::new(Rect::new(512.0, 384.0, 512.0, 384.0), &texture));
    // 4× wider than tall: the UI space stretches, no letterboxing.
    let (w, h) = (256, 48);
    let image = capture(&mut renderer, &list, w, h);
    for y in 0..h {
        for x in 0..w {
            let inside = x >= 128 && y >= 24;
            let expected = if inside { RED } else { [0, 0, 0, 255] };
            assert_eq!(pixel(&image, w, x, y), expected, "({x}, {y})");
        }
    }
}

#[test]
fn a_smaller_frame_fits_the_target_with_its_aspect_kept() {
    let mut renderer = renderer();
    let texture = renderer.create_rgba_texture(1, 1, &RED);
    let mut list = DrawList::default();
    list.quads.push(Quad::new(Rect::SCREEN, &texture));
    // A 4:3 frame of 32 × 24 in a 64 × 24 target: black bars of 16 at the
    // sides (#49).
    let (w, h) = (64, 24);
    let image = pollster::block_on(renderer.capture_scaled(&list, (32, 24), w, h, BLACK)).unwrap();
    for y in 0..h {
        for x in 0..w {
            let expected = if (16..48).contains(&x) {
                RED
            } else {
                [0, 0, 0, 255]
            };
            assert_eq!(pixel(&image, w, x, y), expected, "({x}, {y})");
        }
    }
    // At the target's own size it draws straight into it.
    let image = pollster::block_on(renderer.capture_scaled(&list, (w, h), w, h, BLACK)).unwrap();
    assert!(image.chunks(4).all(|p| p == RED));
}

#[test]
fn the_gamma_ramp_maps_every_channel_of_the_whole_screen() {
    // display-gamma.md 2.1: brightness 1.5 lifts 0 to 8192, 128 to 41088
    // (entries / 65535 shown on an 8-bit target: 32 and 160); 255 stays.
    // At gamma and contrast 1.0 the table is 257 i + 8192, clamped.
    let table: [u16; 256] = std::array::from_fn(|i| (257 * i as u32 + 8192).min(65535) as u16);
    let mut renderer = renderer();
    let texture = renderer.create_rgba_texture(2, 1, &[128, 0, 255, 255, 0, 64, 192, 255]);
    let mut list = DrawList {
        ramp: Some(table),
        ..Default::default()
    };
    list.quads.push(Quad::new(Rect::SCREEN, &texture));
    let shown = |v: u8| (f32::from(table[usize::from(v)]) / 65535.0 * 255.0).round() as u8;
    let left = [shown(128), shown(0), shown(255), 255];
    let right = [shown(0), shown(64), shown(192), 255];
    assert_eq!(left, [160, 32, 255, 255]);
    let image = capture(&mut renderer, &list, 8, 4);
    assert_eq!(pixel(&image, 8, 0, 0), left);
    assert_eq!(pixel(&image, 8, 7, 3), right);
    // Scaled to fit, the black bars are shown through the ramp too: the
    // ramp acts on the entire screen (section 2).
    let image = pollster::block_on(renderer.capture_scaled(&list, (8, 4), 16, 4, BLACK)).unwrap();
    assert_eq!(pixel(&image, 16, 0, 0), [32, 32, 32, 255]);
    assert_eq!(pixel(&image, 16, 4, 0), left);
    assert_eq!(pixel(&image, 16, 11, 3), right);
}

#[test]
fn draws_in_order_with_tint_and_alpha() {
    let mut renderer = renderer();
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
    let image = capture(&mut renderer, &list, 4, 4);
    assert_near(
        pixel(&image, 4, 0, 0),
        [127, 0, 128, 255],
        1,
        "blue over red",
    );
    // Red at 25 % over red stays red; then green at 25 % × 50 % over it.
    assert_near(
        pixel(&image, 4, 3, 3),
        [223, 32, 0, 255],
        1,
        "faint green over red",
    );
}

/// A 4 × 4 BC1 block of one colour (RGB565 `color0`, every index 0).
fn bc1_block(color: u16) -> [u8; 8] {
    let [a, b] = color.to_le_bytes();
    [a, b, 0, 0, 0, 0, 0, 0]
}

/// BC textures by each path (#113): as stored where the device has BC,
/// transcoded to ETC2 where it has only that, decoded to RGBA8 where it has
/// neither; the same colours (exact, or within ETC2's error).
#[test]
fn bc_textures_by_every_path() {
    use wgpu::TextureFormat as F;
    for (allowed, path, formats, tolerance) in [
        (
            wgpu::Features::all(),
            TexturePath::Bc,
            [F::Bc1RgbaUnorm, F::Bc2RgbaUnorm, F::Bc3RgbaUnorm],
            0,
        ),
        (
            wgpu::Features::TEXTURE_COMPRESSION_ETC2,
            TexturePath::Etc2,
            [F::Etc2Rgb8Unorm, F::Etc2Rgba8Unorm, F::Etc2Rgba8Unorm],
            4,
        ),
        (
            wgpu::Features::empty(),
            TexturePath::Rgba8,
            [F::Rgba8Unorm; 3],
            0,
        ),
    ] {
        let (device, queue) = device_with(allowed);
        let mut renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Bgra8Unorm);
        if renderer.texture_path() != path {
            assert_ne!(path, TexturePath::Bc, "the Mac's GPU supports BC");
            eprintln!("skipped {path:?}: the GPU lacks it");
            continue;
        }
        check_bc_formats(&mut renderer, formats, tolerance);
    }
}

fn check_bc_formats(renderer: &mut Renderer, formats: [wgpu::TextureFormat; 3], tolerance: u8) {
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
    let mut rects = [
        Rect::new(0.0, 0.0, 512.0, 768.0),
        Rect::new(512.0, 0.0, 256.0, 768.0),
        Rect::new(768.0, 0.0, 256.0, 768.0),
    ]
    .into_iter();
    for (bytes, format) in [bc1, bc2, bc3].iter().zip(formats) {
        let texture = renderer
            .create_texture(&Dds::parse(bytes).unwrap())
            .unwrap();
        assert_eq!(texture.wgpu().format(), format);
        list.quads.push(Quad::new(rects.next().unwrap(), &texture));
    }
    let image = capture(renderer, &list, 16, 8);
    for (x, y, expected) in [(0, 0, RED), (7, 0, GREEN), (0, 7, BLUE), (7, 7, WHITE)] {
        assert_near(
            pixel(&image, 16, x, y),
            expected,
            tolerance,
            &format!("BC1 ({x}, {y})"),
        );
    }
    assert_near(
        pixel(&image, 16, 9, 3),
        [136, 0, 0, 255],
        1 + tolerance,
        "BC2 red at 136/255",
    );
    assert_near(
        pixel(&image, 16, 13, 3),
        [0, 64, 0, 255],
        1 + tolerance,
        "BC3 green at 64/255",
    );
}

/// A level larger than an upload piece is written in bands of rows (#357):
/// each band lands where it belongs.
#[test]
fn a_large_level_is_written_in_bands() {
    let mut renderer = renderer();
    // BC1, 4096 × 4096 (8 MiB): four horizontal stripes of 1024 rows.
    let colours = [0xf800, 0x07e0, 0x001f, 0xffff];
    let mut blocks = Vec::with_capacity(4096 * 4096 / 2);
    for row in 0..1024 {
        for _ in 0..1024 {
            blocks.extend_from_slice(&bc1_block(colours[row / 256]));
        }
    }
    let bc1 = file(4096, 4096, 0, false, PixelFormat::FourCc(b"DXT1"), &blocks);
    let texture = renderer.create_texture(&Dds::parse(&bc1).unwrap()).unwrap();
    let mut list = DrawList::default();
    list.quads
        .push(Quad::new(Rect::new(0.0, 0.0, 1024.0, 768.0), &texture));
    let image = capture(&mut renderer, &list, 16, 16);
    for (y, expected) in [(2, RED), (6, GREEN), (10, BLUE), (14, WHITE)] {
        assert_eq!(pixel(&image, 16, 8, y), expected, "row {y}");
    }
}

#[test]
fn converts_uncompressed_formats_and_uploads_mips_and_cube_faces() {
    let mut renderer = renderer();
    // RGB565, 2 × 2, mip 1 (1 × 1) a different colour; drawn at half size the
    // sampler picks mip 1.
    let mut data: Vec<u8> = [0xf800u16; 4]
        .iter()
        .flat_map(|t| t.to_le_bytes())
        .collect();
    data.extend_from_slice(&0x001fu16.to_le_bytes());
    let mips = file(2, 2, 2, false, PixelFormat::RGB565, &data);
    let mips = renderer
        .create_texture(&Dds::parse(&mips).unwrap())
        .unwrap();
    assert_eq!(mips.wgpu().mip_level_count(), 2);
    // A8: alpha only, over the white background → grey.
    let a8 = file(1, 1, 0, false, PixelFormat::A8, &[128]);
    let a8 = renderer.create_texture(&Dds::parse(&a8).unwrap()).unwrap();
    // Cube map, BGRA8 1 × 1 per face, face i has red = 200 - 30 × i; draws
    // face 0.
    let faces: Vec<u8> = (0..6).flat_map(|i| [0, 0, 200 - 30 * i, 255]).collect();
    let cube = file(1, 1, 0, true, PixelFormat::BGRA8, &faces);
    let cube = renderer
        .create_texture(&Dds::parse(&cube).unwrap())
        .unwrap();
    assert_eq!(cube.wgpu().depth_or_array_layers(), 6);
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
    list.quads
        .push(Quad::new(Rect::new(768.0, 0.0, 256.0, 384.0), &cube));
    let image = capture(&mut renderer, &list, 16, 8);
    assert_eq!(pixel(&image, 16, 1, 1), RED, "RGB565 mip 0");
    assert_eq!(pixel(&image, 16, 4, 0), BLUE, "RGB565 mip 1");
    assert_near(
        pixel(&image, 16, 9, 1),
        [127, 127, 127, 255],
        1,
        "A8 over white",
    );
    assert_eq!(pixel(&image, 16, 13, 1), [200, 0, 0, 255], "cube face 0");
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

/// The older-hardware integer path of startup.md 6.6, which the spec calls
/// equivalent: the reference must agree with it.
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
                assert_near(a, b, 2, &format!("({y}, {cb}, {cr})"));
            }
        }
    }
    assert_eq!(yuv_to_rgb_integer(0, 0, 0), [0, 135, 0, 255]);
    assert_eq!(yuv_to_rgb(0, 0, 0), [0, 135, 0, 255]);
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

#[test]
fn video_frames_map_to_the_top_left_of_a_power_of_two_texture() {
    let mut renderer = renderer();
    let (w, h) = (6, 3);
    let mut video = renderer.create_video_texture(w as u32, h as u32);
    assert_eq!(video.texture().size(), (8, 4));
    assert_eq!(video.frame_rect(), Rect::new(0.0, 0.0, 6.0, 3.0));
    for seed in [0, 1] {
        let synthetic = SyntheticFrame::new(w, h, seed);
        renderer.update_video(&mut video, &synthetic.frame());
        // The whole texture onto an 8 × 4 target: one pixel per texel.
        let mut list = DrawList::default();
        list.quads.push(Quad::new(Rect::SCREEN, video.texture()));
        let image = capture(&mut renderer, &list, 8, 4);
        for y in 0..4 {
            for x in 0..8 {
                let expected = if x < w && y < h {
                    synthetic.expected(x, y)
                } else {
                    // Never written: Y'CbCr black (our choice).
                    yuv_to_rgb(16, 128, 128)
                };
                let actual = pixel(&image, 8, x as u32, y as u32);
                assert_near(actual, expected, 1, &format!("frame {seed} ({x}, {y})"));
            }
        }
        // The quad's colour and alpha are not applied (startup.md 6.6).
        list.quads[0].color = [0, 0, 0, 0];
        assert_eq!(capture(&mut renderer, &list, 8, 4), image, "tinted");
    }
}

/// Cost of one title-menu-sized frame (876 × 400): GPU upload and conversion
/// against converting on the CPU. Prints the timings; `--release` for
/// representative numbers.
#[test]
fn video_conversion_cost() {
    let renderer = renderer();
    let synthetic = SyntheticFrame::new(876, 400, 7);
    let frame = synthetic.frame();
    let mut video = renderer.create_video_texture(876, 400);
    renderer.update_video(&mut video, &frame);
    let _ = renderer.device().poll(wgpu::PollType::wait_indefinitely());
    const N: u32 = 20;
    let start = Instant::now();
    for _ in 0..N {
        renderer.update_video(&mut video, &frame);
    }
    let submit = start.elapsed() / N;
    let _ = renderer.device().poll(wgpu::PollType::wait_indefinitely());
    let gpu = start.elapsed() / N;
    let start = Instant::now();
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
    let cpu = start.elapsed() / N;
    eprintln!(
        "876×400 frame: GPU path {submit:?} CPU per frame ({gpu:?} incl. GPU wait); CPU conversion {cpu:?}"
    );
}
