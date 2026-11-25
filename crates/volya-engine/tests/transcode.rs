// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The ETC2 transcoding of a New Game level's textures (#113, #183): time
//! and quality per kind of block, every face and mip level, against the BC
//! texels (decoded by the decoder written apart from the encoder). Run it
//! with `--release` for meaningful times. Skips when `VOLYA_GOG_INSTALLER`
//! is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::{Duration, Instant};

use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine_formats::dds::{Dds, Format};
use volya_engine_formats::etc2::{self, Etc2Format, decode};
use volya_engine_formats::ui::Resolution;

/// One kind of block: time, texels and squared error.
#[derive(Default)]
struct Class {
    time: Duration,
    texels: u64,
    squared: f64,
    samples: u64,
}

impl Class {
    fn psnr(&self) -> f64 {
        10.0 * (255.0f64.powi(2) / (self.squared / self.samples as f64)).log10()
    }
}

fn squared(a: u8, b: u8) -> f64 {
    (f64::from(a) - f64::from(b)).powi(2)
}

#[test]
fn level_textures_transcode() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    let screen = LevelScreen::load(&tree, &data, 0).unwrap();
    let paths = screen.world().texture_paths();
    // [RGB8, RGB8A1, RGBA8's colour, RGBA8's alpha]
    let mut classes: [Class; 4] = Default::default();
    let mut textures = 0;
    for path in &paths {
        let bytes = volya_engine::core::vfs::read(&tree, path).unwrap();
        let dds = Dds::parse(&bytes).unwrap();
        let Some(block) = dds.format.block_bytes() else {
            continue;
        };
        textures += 1;
        let levels: Vec<&[u8]> = (0..dds.faces)
            .flat_map(|f| (0..dds.mip_levels).map(move |m| (f, m)))
            .map(|(f, m)| dds.level(f, m))
            .collect();
        let target = match dds.format {
            Format::Bc1 => {
                if levels
                    .iter()
                    .any(|l| Etc2Format::for_bc(dds.format, l) == Some(Etc2Format::Rgb8A1))
                {
                    Etc2Format::Rgb8A1
                } else {
                    Etc2Format::Rgb8
                }
            }
            _ => Etc2Format::Rgba8,
        };
        let start = Instant::now();
        let out: Vec<Vec<u8>> = levels
            .iter()
            .map(|l| etc2::transcode_bc(dds.format, l, target))
            .collect();
        let time = start.elapsed();
        let blocks: u64 = levels.iter().map(|l| (l.len() / block) as u64).sum();
        let colour = match target {
            Etc2Format::Rgb8 => 0,
            Etc2Format::Rgb8A1 => 1,
            Etc2Format::Rgba8 => 2,
        };
        classes[colour].time += time;
        classes[colour].texels += blocks * 16;
        for (source, encoded) in levels.iter().zip(&out) {
            let encoded_block = target.block_bytes();
            for (s, e) in source
                .chunks_exact(block)
                .zip(encoded.chunks_exact(encoded_block))
            {
                let texels = dds.format.decode_block(s);
                let (alpha, rgb) = if target == Etc2Format::Rgba8 {
                    (Some(decode::alpha(e[..8].try_into().unwrap())), &e[8..])
                } else {
                    (None, e)
                };
                let punch = target == Etc2Format::Rgb8A1;
                let decoded = decode::rgb(rgb.try_into().unwrap(), punch);
                for (t, d) in texels.iter().zip(&decoded) {
                    if punch {
                        assert_eq!(d[3], if t[3] < 128 { 0 } else { 255 }, "{path}");
                        if t[3] < 128 {
                            continue;
                        }
                    }
                    let c = &mut classes[colour];
                    c.squared += (0..3).map(|k| squared(t[k], d[k])).sum::<f64>();
                    c.samples += 3;
                }
                if let Some(alpha) = alpha {
                    let c = &mut classes[3];
                    c.texels += 16;
                    for (t, a) in texels.iter().zip(alpha) {
                        c.squared += squared(t[3], a);
                        c.samples += 1;
                    }
                }
            }
        }
    }
    let total: Duration = classes.iter().map(|c| c.time).sum();
    let mpx = (classes[0].texels + classes[1].texels + classes[2].texels) as f64 / 1e6;
    eprintln!(
        "{textures} BC textures, {mpx:.1} Mpx: {total:.2?} ({:.1} Mpx/s)",
        mpx / total.as_secs_f64()
    );
    for (name, c) in ["RGB8", "RGB8A1", "RGBA8 colour", "RGBA8 alpha"]
        .iter()
        .zip(&classes)
    {
        eprintln!(
            "{name}: {:.1} Mpx, {:.2?}, PSNR {:.2} dB",
            c.texels as f64 / 1e6,
            c.time,
            c.psnr()
        );
    }
    // #113's encoder measured 43.38, 35.71, 38.91 and 47.77 dB; #183's may
    // lose up to about 0.5 dB a kind for its speed.
    for (c, floor) in classes.iter().zip([42.88, 35.21, 38.41, 47.27]) {
        assert!(c.psnr() > floor, "{:.2} dB, below {floor}", c.psnr());
    }
}
