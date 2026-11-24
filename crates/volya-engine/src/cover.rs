// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Art from the player's own game for the app's home screen (our design):
//! the loading screen's background, `ui\ui_load` (docs/behavior/
//! loading-screen.md 4.1: the top 1024 × 768 texels of its texture), decoded
//! and halved to keep the page's memory small.

use std::io;

use volya_engine_core::config::PathAliases;
use volya_engine_core::vfs::{self, Vfs};
use volya_engine_formats::dds::Dds;

use crate::loading_screen::BACKGROUND;

/// An RGBA image, 8 bits per channel, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The background part of the loading screen's texture, as the spec maps it.
const BACKGROUND_TEXELS: (u32, u32) = (1024, 768);

/// Reads and decodes the cover from `tree`, using `aliases` to find it.
pub fn read(tree: &dyn Vfs, aliases: &PathAliases) -> io::Result<Image> {
    let path = aliases.path("$game_textures$", &format!("{BACKGROUND}.dds"))?;
    let bytes = vfs::read(tree, &path)?;
    let dds = Dds::parse(&bytes)?;
    let rgba = dds.format.to_rgba8(dds.level(0, 0), dds.width, dds.height);
    // The background's share of the texture: 1024 × 768 of 1024 × 1024 (u 0 … 1024 / Tw, v 0 … 768 / Th).
    let w = dds.width.min(BACKGROUND_TEXELS.0 * dds.width / 1024);
    let h = dds.height.min(BACKGROUND_TEXELS.1 * dds.height / 1024);
    Ok(halve(&rgba, dds.width, w, h))
}

/// The top-left `w` × `h` texels of a `stride`-wide image, halved by
/// averaging 2 × 2 texels (our choice of filter).
fn halve(rgba: &[u8], stride: u32, w: u32, h: u32) -> Image {
    let (out_w, out_h) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = Vec::with_capacity((out_w * out_h * 4) as usize);
    for y in 0..out_h {
        for x in 0..out_w {
            for c in 0..4 {
                let at = |dx: u32, dy: u32| {
                    let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                    u32::from(rgba[((sy * stride + sx) * 4 + c) as usize])
                };
                out.push(((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8);
            }
        }
    }
    Image {
        width: out_w,
        height: out_h,
        rgba: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_the_top_left_part() {
        // 4 × 2 image (stride 4); take the left 2 × 2.
        let rgba: Vec<u8> = [10, 20, 99, 99, 30, 40, 99, 99]
            .iter()
            .flat_map(|&v| [v, v, v, 255])
            .collect();
        let image = halve(&rgba, 4, 2, 2);
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(image.rgba, vec![25, 25, 25, 255]);
    }
}
