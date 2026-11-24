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
use volya_engine_formats::video::{Frame, Plane};
use volya_engine_render::wgpu;
use volya_engine_render::{DrawList, Quad, Rect, Renderer, TextureError};

const BLACK: wgpu::Color = wgpu::Color::BLACK;

/// The device (with BC support), shared by the tests.
fn gpu() -> &'static (wgpu::Device, wgpu::Queue) {
    static GPU: OnceLock<(wgpu::Device, wgpu::Queue)> = OnceLock::new();
    GPU.get_or_init(|| device(true))
}

fn device(bc: bool) -> (wgpu::Device, wgpu::Queue) {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&Default::default())
            .await
            .expect("a GPU adapter (Metal on the Mac)");
        let mut features = volya_engine_render::device_features(&adapter);
        if bc {
            assert!(
                features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
                "the Mac's GPU supports BC"
            );
        } else {
            features.remove(wgpu::Features::TEXTURE_COMPRESSION_BC);
        }
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: features,
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

#[test]
fn uploads_bc_formats_as_stored() {
    let mut renderer = renderer();
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
    let texture = renderer.create_texture(&Dds::parse(&bc1).unwrap()).unwrap();
    list.quads
        .push(Quad::new(Rect::new(0.0, 0.0, 512.0, 768.0), &texture));
    let texture = renderer.create_texture(&Dds::parse(&bc2).unwrap()).unwrap();
    assert_eq!(texture.wgpu().format(), wgpu::TextureFormat::Bc2RgbaUnorm);
    list.quads
        .push(Quad::new(Rect::new(512.0, 0.0, 256.0, 768.0), &texture));
    let texture = renderer.create_texture(&Dds::parse(&bc3).unwrap()).unwrap();
    list.quads
        .push(Quad::new(Rect::new(768.0, 0.0, 256.0, 768.0), &texture));
    let image = capture(&mut renderer, &list, 16, 8);
    for (x, y, expected) in [(0, 0, RED), (7, 0, GREEN), (0, 7, BLUE), (7, 7, WHITE)] {
        assert_eq!(pixel(&image, 16, x, y), expected, "BC1 ({x}, {y})");
    }
    assert_near(
        pixel(&image, 16, 9, 3),
        [136, 0, 0, 255],
        1,
        "BC2 red at 136/255",
    );
    assert_near(
        pixel(&image, 16, 13, 3),
        [0, 64, 0, 255],
        1,
        "BC3 green at 64/255",
    );
}

#[test]
fn bc_without_device_support_is_a_clear_error() {
    let (device, queue) = device(false);
    let renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Bgra8Unorm);
    let bc1 = file(4, 4, 0, false, PixelFormat::FourCc(b"DXT1"), &[0; 8]);
    let error = renderer
        .create_texture(&Dds::parse(&bc1).unwrap())
        .unwrap_err();
    assert_eq!(
        error,
        TextureError::NoBcSupport {
            format: "BC1 (DXT1)"
        }
    );
    assert_eq!(
        error.to_string(),
        "BC1 (DXT1) texture needs the GPU feature texture-compression-bc, which this device lacks"
    );
    // Uncompressed textures still work.
    let rgba = file(1, 1, 0, false, PixelFormat::BGRA8, &[0, 0, 255, 255]);
    renderer
        .create_texture(&Dds::parse(&rgba).unwrap())
        .unwrap();
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

/// Reference conversion: BT.601, video range.
fn yuv_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 4] {
    let y = (f32::from(y) - 16.0) * 1.164383;
    let (cb, cr) = (f32::from(cb) - 128.0, f32::from(cr) - 128.0);
    let channel = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    [
        channel(y + 1.596027 * cr),
        channel(y - 0.391762 * cb - 0.812968 * cr),
        channel(y + 2.017232 * cb),
        255,
    ]
}

/// A synthetic 4:2:0 frame: planes with a row stride wider than the picture.
struct SyntheticFrame {
    width: usize,
    height: usize,
    planes: [Vec<u8>; 3],
}

impl SyntheticFrame {
    const PAD: usize = 5;

    fn new(width: usize, height: usize, seed: u8) -> Self {
        let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
        let plane = |w: usize, h: usize, k: u8| -> Vec<u8> {
            (0..(w + Self::PAD) * h)
                .map(|i| ((i % (w + Self::PAD)) * 37 + (i / (w + Self::PAD)) * 11) as u8 ^ k ^ seed)
                .collect()
        };
        Self {
            width,
            height,
            planes: [
                plane(width, height, 0),
                plane(cw, ch, 0x55),
                plane(cw, ch, 0xaa),
            ],
        }
    }

    fn frame(&self) -> Frame<'_> {
        let (cw, ch) = (self.width.div_ceil(2), self.height.div_ceil(2));
        let sizes = [(self.width, self.height), (cw, ch), (cw, ch)];
        let planes = [0, 1, 2].map(|i| Plane {
            width: sizes[i].0,
            height: sizes[i].1,
            stride: sizes[i].0 + Self::PAD,
            data: &self.planes[i],
        });
        Frame { number: 0, planes }
    }

    fn expected(&self, x: usize, y: usize) -> [u8; 4] {
        let f = self.frame();
        let at = |p: &Plane, x: usize, y: usize| p.data[y * p.stride + x];
        yuv_to_rgb(
            at(&f.planes[0], x, y),
            at(&f.planes[1], x / 2, y / 2),
            at(&f.planes[2], x / 2, y / 2),
        )
    }
}

#[test]
fn video_frames_convert_on_the_gpu_into_a_power_of_two_texture() {
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
                    // Transparent black over the black background.
                    [0, 0, 0, 255]
                };
                let actual = pixel(&image, 8, x as u32, y as u32);
                assert_near(actual, expected, 1, &format!("frame {seed} ({x}, {y})"));
            }
        }
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
        let [luma, cb, cr] = &frame.planes;
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
