// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's textures and videos on the GPU (once the render crate's
//! `tests/gog.rs`): a sample of every kind of texture (format, cube map,
//! mip chain) uploads and draws, and the title menu's fire video plays
//! into a video texture.

use std::collections::BTreeMap;
use std::io::Cursor;

use super::{Context, Outcome, Run, capture, ensure, ensure_eq, now_ms};
use crate::core::vfs::{self, EntryKind, Vfs};
use crate::render::{DrawList, Quad, Rect};
use volya_engine_formats::dds::{Dds, Format};
use volya_engine_formats::video::TheoraVideo;

pub const CHECKS: &[(&str, Run)] = &[("game textures and videos upload and draw", |c| {
    Box::pin(game_textures_and_videos_upload_and_draw(c))
})];

/// Samples per kind (format × cube × mip chain).
const SAMPLES: usize = 3;

/// The `.dds` files under `dir` of `tree`, recursively, into `out`.
fn textures(tree: &dyn Vfs, dir: &str, out: &mut Vec<String>) -> Result<(), String> {
    for entry in tree.read_dir(dir).map_err(|e| format!("{dir}: {e}"))? {
        let path = if dir.is_empty() {
            entry.name
        } else {
            format!("{dir}/{}", entry.name)
        };
        match entry.kind {
            EntryKind::Dir => textures(tree, &path, out)?,
            EntryKind::File if path.to_ascii_lowercase().ends_with(".dds") => out.push(path),
            EntryKind::File => {}
        }
    }
    Ok(())
}

pub async fn game_textures_and_videos_upload_and_draw(cx: &Context<'_>) -> Outcome {
    let tree = cx.tree;
    let mut files = Vec::new();
    textures(tree, "", &mut files)?;
    // In the archives' order of paths (`\`-separated, lower case).
    files.sort_by_key(|p| p.to_ascii_lowercase().replace('/', "\\"));

    let mut renderer = cx.renderer();
    let mut kinds: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let start = now_ms();
    for path in &files {
        let bytes = vfs::read(tree, path).map_err(|e| format!("{path}: {e}"))?;
        let dds = Dds::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
        let format = match dds.format {
            Format::Masked(m) => format!("{}-bit", m.bits),
            f => format!("{f:?}"),
        };
        // The one upload precondition samples could miss.
        if dds.format.block_bytes().is_some() {
            ensure!(
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
            .map_err(|e| format!("{path}: {e}"))?;
        let mut list = DrawList::default();
        list.quads.push(Quad::new(Rect::SCREEN, &texture));
        capture(&mut renderer, &list, 64, 48).await?;
    }
    let elapsed = now_ms() - start;
    // 15 kinds in this edition.
    ensure_eq!(kinds.len(), 15, "kinds: {:?}", kinds.keys());

    // The title menu's camp fire: 128 × 256 frames into a texture of that
    // size (already powers of two).
    let path = "gamedata/textures/ui/ui_mm_fire.ogm";
    let bytes = vfs::read(tree, path).map_err(|e| format!("{path}: {e}"))?;
    let mut video = TheoraVideo::open(Cursor::new(bytes)).map_err(|e| format!("{path}: {e}"))?;
    let (w, h) = (video.info().width, video.info().height);
    let mut texture = renderer.create_video_texture(w, h);
    ensure_eq!(texture.texture().size(), (128, 256));
    let mut frames = 0;
    let mut lit = 0;
    while let Some(frame) = video.next_frame().map_err(|e| format!("{path}: {e}"))? {
        renderer.update_video(&mut texture, &frame);
        frames += 1;
        if frames % 30 == 1 {
            let mut list = DrawList::default();
            list.quads.push(Quad::new(Rect::SCREEN, texture.texture()));
            let image = capture(&mut renderer, &list, 32, 64).await?;
            // A fire: plenty of bright, red-dominant pixels.
            lit += image
                .chunks(4)
                .filter(|p| p[0] > 128 && p[0] >= p[2])
                .count();
        }
    }
    ensure_eq!(frames, 118);
    ensure!(lit > 100, "{lit} bright pixels");
    let kinds: Vec<String> = kinds
        .iter()
        .map(|(k, s)| format!("{k}: {}", s.len()))
        .collect();
    Ok(format!(
        "uploaded and drew {} kinds ({}) in {elapsed:.0} ms",
        kinds.len(),
        kinds.join("; ")
    ))
}
