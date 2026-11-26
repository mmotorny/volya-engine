// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Frame presentation from the Russian edition (BYOA): the verification
//! facts of docs/behavior/frame-presentation.md section 5, and the first
//! level's start view drawn through the supersampled target, against the
//! same view at `r__supersample` 1, with frame times. `VOLYA_DUMP_SUPERSAMPLE=<file.ppm>` writes the supersampled frame
//! locally. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::fs::File;
use std::io::BufReader;
use std::time::Instant;

use volya_engine::core::console::Console;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{self, StdVfs};
use volya_engine::data::{DEFAULT_USER_LTX, GameData};
use volya_engine::level::LevelScreen;
use volya_engine::present::Presenter;
use volya_engine::render::present::Presentation;
use volya_engine::render::{Renderer, wgpu};
use volya_engine_formats::dds::{Dds, Format, Masks};
use volya_engine_formats::ui::Resolution;
use volya_engine_installer::{Installer, extract};

const SIZE: Resolution = Resolution::new(1024, 768);

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

/// The installer's `__support/userdocs/user.ltx`.
fn shipped_user_ltx() -> Vec<u8> {
    let exe = std::fs::read_dir(std::env::var_os("VOLYA_GOG_INSTALLER").unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.contains("(russian)") && n.ends_with(".exe")
        })
        .unwrap();
    let bin = exe.with_file_name(format!(
        "{}-1.bin",
        exe.file_stem().unwrap().to_string_lossy()
    ));
    let installer = Installer::parse(BufReader::new(File::open(&exe).unwrap())).unwrap();
    let file = installer
        .game_files("ru-RU")
        .find(|f| f.path == DEFAULT_USER_LTX)
        .unwrap();
    let mut data = Vec::new();
    extract(
        file,
        &mut BufReader::new(File::open(&bin).unwrap()),
        &mut data,
    )
    .unwrap();
    data
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let se: f64 = a
        .chunks(4)
        .zip(b.chunks(4))
        .flat_map(|(x, y)| (0..3).map(move |c| (f64::from(x[c]) - f64::from(y[c])).powi(2)))
        .sum();
    10.0 * (255.0f64.powi(2) / (se / (a.len() / 4 * 3) as f64)).log10()
}

#[test]
fn presentation_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();

    // Section 5: the shipped user.ltx.
    let user = String::from_utf8_lossy(&shipped_user_ltx()).into_owned();
    for line in ["r__supersample 2", "r1_pps_u 0.", "r1_pps_v 0."] {
        assert!(user.lines().any(|l| l.trim() == line), "{line}");
    }
    let console = Console::startup(Default::default(), user.as_bytes());
    assert_eq!(console.number("r__supersample"), 2.0);
    // `shaders\r1\.s`'s texture names.
    let s = vfs::read(
        &tree,
        &data.aliases.path("$game_shaders$", "r1\\.s").unwrap(),
    )
    .unwrap();
    // Whitespace collapsed (the file aligns with tabs).
    let s: String = String::from_utf8_lossy(&s)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for name in [
        "t_rt = \"$user$rendertarget\"",
        "t_distort = \"$user$distort\"",
        "t_noise = \"fx\\\\fx_noise2\"",
    ] {
        assert!(s.contains(name), "{name}");
    }
    // `fx_noise2.dds`: 174,890 bytes, 256 × 256, 9 levels, 5:6:5.
    let noise = data
        .aliases
        .path("$game_textures$", "fx\\fx_noise2.dds")
        .unwrap();
    let bytes = vfs::read(&tree, &noise).unwrap();
    assert_eq!(bytes.len(), 174_890);
    let dds = Dds::parse(&bytes).unwrap();
    assert_eq!((dds.width, dds.height, dds.mip_levels), (256, 256, 9));
    assert_eq!(
        dds.format,
        Format::Masked(Masks {
            bits: 16,
            r: 0xF800,
            g: 0x07E0,
            b: 0x001F,
            a: 0,
        })
    );

    // The start view at r__supersample 2 against 1.
    let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();
    assert_eq!(
        screen.world().presentation_noise.as_deref(),
        Some(noise.as_str())
    );
    let mut r = renderer();
    let mut presenter = Presenter::new();
    let mut frames = Vec::new();
    for supersample in [1, 2] {
        screen.set_supersample(supersample);
        let mut scene = screen.scene(SIZE, 0);
        scene.presentation = Presentation {
            supersample,
            ..Default::default()
        };
        let draw = |r: &mut Renderer, presenter: &mut Presenter| {
            let list = presenter.draw_list(r, &tree, &scene, 0);
            pollster::block_on(r.capture(&list, SIZE.width, SIZE.height, wgpu::Color::BLACK))
                .unwrap()
        };
        draw(&mut r, &mut presenter);
        let start = Instant::now();
        const FRAMES: u32 = 10;
        let mut pixels = Vec::new();
        for _ in 0..FRAMES {
            pixels = draw(&mut r, &mut presenter);
        }
        let time = start.elapsed() / FRAMES;
        let target = r.presentation_target();
        eprintln!(
            "r__supersample {supersample}: target {target:?}, {time:.1?} \
             per frame with readback"
        );
        // At 1 the frame goes through the target only after a distortion
        // pass (3.1): the start view has the level's distorting effect.
        let want = if supersample == 1 {
            (1024, 768)
        } else {
            (1448, 1086)
        };
        assert_eq!(target.map(|t| t.0), Some(want));
        assert!(supersample == 1 || target.is_some_and(|t| t.1));
        frames.push(pixels);
    }
    assert!(presenter.take_errors().is_empty());
    if let Some(path) = std::env::var_os("VOLYA_DUMP_SUPERSAMPLE") {
        let mut ppm = format!("P6 {} {} 255\n", SIZE.width, SIZE.height).into_bytes();
        for p in frames[1].chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path, ppm).unwrap();
    }
    // The same picture, resampled: close, not equal.
    let q = psnr(&frames[0], &frames[1]);
    eprintln!("PSNR of r__supersample 2 against 1: {q:.1} dB");
    assert!(q > 22.0 && frames[0] != frames[1], "{q:.1} dB");
}
