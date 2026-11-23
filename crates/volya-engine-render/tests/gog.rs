// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's textures and videos on the GPU: a sample of every
//! kind of texture (format, cube map, mip chain) uploads and draws, and the
//! title menu's fire video plays into a video texture. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

// Needs a native GPU.
#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::time::Instant;

use volya_engine_formats::db::{Archive, Entry, Scrambling};
use volya_engine_formats::dds::{Dds, Format};
use volya_engine_formats::video::TheoraVideo;
use volya_engine_render::wgpu;
use volya_engine_render::{DrawList, Quad, Rect, Renderer};

/// Samples per kind (format × cube × mip chain).
const SAMPLES: usize = 3;

fn renderer() -> Renderer {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: volya_engine_render::device_features(&adapter),
                ..Default::default()
            })
            .await
            .unwrap();
        Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm)
    })
}

#[test]
fn game_textures_and_videos_upload_and_draw() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    // The game's own archives (without `mods/`).
    let mut opened: Vec<_> = archives
        .iter()
        .filter(|a| !a.path.starts_with("mods/"))
        .map(|a| {
            let file = BufReader::new(File::open(&a.file).unwrap());
            Archive::open(file, Scrambling::for_path(&a.path)).unwrap()
        })
        .collect();
    let mut files: BTreeMap<String, (usize, Entry)> = BTreeMap::new();
    for (i, archive) in opened.iter().enumerate() {
        for entry in archive.entries() {
            let path = entry.path.to_ascii_lowercase();
            if path.ends_with(".dds") || path.ends_with(".ogm") {
                files.insert(path, (i, entry.clone()));
            }
        }
    }

    let mut renderer = renderer();
    let mut kinds: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let start = Instant::now();
    for (path, (i, entry)) in files.iter().filter(|(p, _)| p.ends_with(".dds")) {
        let bytes = opened[*i].read(entry).unwrap();
        let dds = Dds::parse(&bytes).unwrap();
        let format = match dds.format {
            Format::Masked(m) => format!("{}-bit", m.bits),
            f => format!("{f:?}"),
        };
        // The one upload precondition samples could miss.
        if dds.format.block_bytes().is_some() {
            assert!(
                dds.width.is_multiple_of(4) && dds.height.is_multiple_of(4),
                "{path}: block size"
            );
        }
        let kind = format!("{format} cube={} mips={}", dds.cube, dds.mip_levels > 1);
        let samples = kinds.entry(kind).or_default();
        if samples.len() == SAMPLES {
            continue;
        }
        samples.push(path.clone());
        let texture = renderer
            .create_texture(&dds)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut list = DrawList::default();
        list.quads.push(Quad::new(Rect::SCREEN, &texture));
        pollster::block_on(renderer.capture(&list, 64, 48, wgpu::Color::BLACK)).unwrap();
    }
    eprintln!("uploaded and drew {kinds:#?} in {:.1?}", start.elapsed());
    // 15 kinds in this edition.
    assert_eq!(kinds.len(), 15, "kinds: {:?}", kinds.keys());

    // The title menu's camp fire: 128 × 256 frames into a texture of that
    // size (already powers of two).
    let (i, entry) = &files["textures\\ui\\ui_mm_fire.ogm"];
    let bytes = opened[*i].read(entry).unwrap();
    let mut video = TheoraVideo::open(Cursor::new(bytes)).unwrap();
    let (w, h) = (video.info().width, video.info().height);
    let mut texture = renderer.create_video_texture(w, h);
    assert_eq!(texture.texture().size(), (128, 256));
    let mut frames = 0;
    let mut lit = 0;
    while let Some(frame) = video.next_frame().unwrap() {
        renderer.update_video(&mut texture, &frame);
        frames += 1;
        if frames % 30 == 1 {
            let mut list = DrawList::default();
            list.quads.push(Quad::new(Rect::SCREEN, texture.texture()));
            let image =
                pollster::block_on(renderer.capture(&list, 32, 64, wgpu::Color::BLACK)).unwrap();
            // A fire: plenty of bright, red-dominant pixels.
            lit += image
                .chunks(4)
                .filter(|p| p[0] > 128 && p[0] >= p[2])
                .count();
        }
    }
    assert_eq!(frames, 118);
    assert!(lit > 100, "{lit} bright pixels");
}
