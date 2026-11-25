// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Pictures of the game's UI textures for a platform's own interface (the
//! web page's touch controls, #105): a still image decoded to RGBA, whole
//! or a rectangle of it, named by its texture path, by a texture
//! description id (docs/formats/ui-layout.md sections 6.2–6.3) or by an
//! item section (its inventory icon), and the transforms our design
//! applies to them.

use std::io;

use crate::core::character::inventory::ItemConfig;
use crate::core::config::{Config, PathAliases};
use crate::core::ui;
use crate::core::vfs::{self, Vfs};
use crate::inventory_screen::layout::{ICON_PIXELS, ICONS};
use volya_engine_formats::dds::Dds;
use volya_engine_formats::ltx::value;
use volya_engine_formats::pe::Rgba;
use volya_engine_formats::ui::texture_desc::TextureDescriptions;

/// What picture to take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A texture path, relative to `textures\` without extension (e.g.
    /// `ui\ui_hud_frame_back`): the whole image.
    Path(String),
    /// A texture description id (e.g. `ui_hud_soldier_crouch`): its
    /// rectangle of its texture.
    Id(String),
    /// An item section (e.g. `device_torch`): its inventory icon
    /// (docs/behavior/inventory-screen.md 4.2).
    Item(String),
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The picture `source` names, cut to `region` (`[x, y, width, height]`
/// in texels of the texture, replacing an id's own rectangle) when given:
/// the top mip level of the still image, decoded to straight RGBA. The
/// rectangle is clipped to the image.
pub fn load(
    tree: &dyn Vfs,
    aliases: &PathAliases,
    descriptions: &TextureDescriptions,
    system: &Config,
    source: &Source,
    region: Option<[u32; 4]>,
) -> io::Result<Rgba> {
    let (path, rect) = match source {
        Source::Path(path) => (path.as_str(), None),
        Source::Item(section) => {
            let grid = ItemConfig::read(system, section)?
                .inv_grid
                .ok_or_else(|| invalid(format!("{section}: no inv_grid keys")))?;
            let pixels = ICON_PIXELS as u32;
            let rect = [grid.x, grid.y, grid.width, grid.height].map(|v| v * pixels);
            (ICONS, Some(rect))
        }
        Source::Id(id) => {
            let desc = descriptions
                .get(id)
                .ok_or_else(|| invalid(format!("no texture description {id}")))?;
            let r = desc.rect;
            let rect = [r.x, r.y, r.width, r.height].map(|v| v.max(0.0).round() as u32);
            (desc.file.as_str(), Some(rect))
        }
    };
    let file = ui::still_image(tree, aliases, path)?;
    let bytes = vfs::read(tree, &file)?;
    let dds = Dds::parse(&bytes).map_err(|e| invalid(format!("{file}: {e}")))?;
    let Some(rect) = region.or(rect) else {
        let pixels = dds.format.to_rgba8(dds.level(0, 0), dds.width, dds.height);
        return Ok(Rgba {
            width: dds.width,
            height: dds.height,
            pixels,
        });
    };
    // Only the rows of blocks the rectangle covers are decoded.
    let [x, y, width, height] = rect;
    let top = (y.min(dds.height) / 4 * 4).min(dds.height);
    let bottom = y
        .saturating_add(height)
        .min(dds.height)
        .next_multiple_of(4)
        .min(dds.height)
        .max(top);
    let rows = |count: u32| match count {
        0 => 0,
        _ => dds.format.image_bytes(dds.width, count),
    };
    let level = dds.level(0, 0);
    let band = &level[rows(top)..rows(top) + rows(bottom - top)];
    let image = Rgba {
        width: dds.width,
        height: bottom - top,
        pixels: dds.format.to_rgba8(band, dds.width, bottom - top),
    };
    Ok(crop(&image, [x, y.saturating_sub(top), width, height]))
}

/// The rectangle `[x, y, width, height]` of `image`, clipped to it.
pub fn crop(image: &Rgba, [x, y, width, height]: [u32; 4]) -> Rgba {
    let x0 = x.min(image.width);
    let y0 = y.min(image.height);
    let x1 = x.saturating_add(width).min(image.width);
    let y1 = y.saturating_add(height).min(image.height);
    let row = image.width as usize * 4;
    let mut pixels = Vec::with_capacity((x1 - x0) as usize * (y1 - y0) as usize * 4);
    for y in y0..y1 {
        let start = y as usize * row + x0 as usize * 4;
        pixels.extend_from_slice(&image.pixels[start..start + (x1 - x0) as usize * 4]);
    }
    Rgba {
        width: x1 - x0,
        height: y1 - y0,
        pixels,
    }
}

/// The mean colour of `image`'s texels, weighted by their alpha; `None`
/// when it is wholly transparent.
pub fn mean_color(image: &Rgba) -> Option<[u8; 3]> {
    let mut sum = [0u64; 3];
    let mut weight = 0u64;
    for p in image.pixels.as_chunks::<4>().0 {
        let a = u64::from(p[3]);
        for (s, &c) in sum.iter_mut().zip(p) {
            *s += u64::from(c) * a;
        }
        weight += a;
    }
    (weight > 0).then(|| sum.map(|s| ((s + weight / 2) / weight) as u8))
}

/// `image`'s alpha with gaps one row high closed: each texel takes the
/// largest alpha of its own row and the rows above and below (our design:
/// the HUD's posture icons are drawn in scanlines a texel apart, which
/// scaled to the screen's pixels would beat against them). Colours stay.
pub fn close_row_gaps(image: &Rgba) -> Rgba {
    let (w, h) = (image.width as usize, image.height as usize);
    let alpha = |x: usize, y: usize| image.pixels[(y * w + x) * 4 + 3];
    let mut pixels = image.pixels.clone();
    for y in 0..h {
        for x in 0..w {
            let above = y.checked_sub(1).map_or(0, |y| alpha(x, y));
            let below = if y + 1 < h { alpha(x, y + 1) } else { 0 };
            pixels[(y * w + x) * 4 + 3] = alpha(x, y).max(above).max(below);
        }
    }
    Rgba {
        width: image.width,
        height: image.height,
        pixels,
    }
}

/// `image`'s alpha filled with `color`: a silhouette.
pub fn recolor(image: &Rgba, [r, g, b]: [u8; 3]) -> Rgba {
    let mut pixels = image.pixels.clone();
    for p in pixels.as_chunks_mut::<4>().0 {
        p[..3].copy_from_slice(&[r, g, b]);
    }
    Rgba {
        width: image.width,
        height: image.height,
        pixels,
    }
}

/// The silhouette our design draws a HUD icon as: its alpha, row gaps
/// closed, filled with its own mean colour (returned too).
pub fn silhouette(image: &Rgba) -> (Rgba, Option<[u8; 3]>) {
    let mean = mean_color(image);
    let closed = close_row_gaps(image);
    let filled = match mean {
        Some(color) => recolor(&closed, color),
        None => closed,
    };
    (filled, mean)
}

/// The engraving our design draws a full-colour picture as (an item
/// icon, #363): white, each texel's alpha scaled by its brightness
/// relative to the picture's (the 99th percentile of the brightness of its
/// texels with any alpha, so a few glints don't dim the rest), so its
/// details stay where a silhouette would be one blot.
pub fn engraving(image: &Rgba) -> Rgba {
    let texels = image.pixels.as_chunks::<4>().0;
    // Rec. 601 luma, 0–255.
    let luma = |p: &[u8; 4]| {
        (299 * u32::from(p[0]) + 587 * u32::from(p[1]) + 114 * u32::from(p[2])) / 1000
    };
    let mut lit: Vec<u32> = texels.iter().filter(|p| p[3] > 0).map(luma).collect();
    lit.sort_unstable();
    let top = lit.get(lit.len() * 99 / 100).copied().unwrap_or(0).max(1);
    let mut pixels = Vec::with_capacity(image.pixels.len());
    for p in texels {
        let a = u32::from(p[3]) * luma(p).min(top) / top;
        pixels.extend_from_slice(&[255, 255, 255, a as u8]);
    }
    Rgba {
        width: image.width,
        height: image.height,
        pixels,
    }
}

/// The crosshair's colour, `[hud_cursor] cross_color` of `system`
/// (`config\system.ltx`): red, green, blue and alpha from 0 to 1; `None`
/// when absent.
pub fn cross_color(system: &Config) -> Option<[f32; 4]> {
    let text = system.get("hud_cursor", "cross_color")??;
    Some(value::floats::<4>(text).map(|v| v.clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, texel: impl Fn(u32, u32) -> [u8; 4]) -> Rgba {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&texel(x, y));
            }
        }
        Rgba {
            width,
            height,
            pixels,
        }
    }

    fn alphas(image: &Rgba) -> Vec<u8> {
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect()
    }

    #[test]
    fn closes_gaps_one_row_high() {
        // Scanlines: opaque on even rows of a 2-wide, 5-high icon.
        let lines = image(2, 5, |_, y| [10, 20, 30, if y % 2 == 0 { 200 } else { 0 }]);
        let closed = close_row_gaps(&lines);
        assert_eq!(alphas(&closed), vec![200; 10]);
        assert_eq!(&closed.pixels[..3], &[10, 20, 30]);
    }

    #[test]
    fn takes_the_largest_alpha_of_three_rows() {
        let column = image(1, 5, |_, y| [0, 0, 0, [0, 50, 0, 0, 90][y as usize]]);
        assert_eq!(alphas(&close_row_gaps(&column)), vec![50, 50, 50, 90, 90]);
    }

    #[test]
    fn leaves_wider_gaps_and_columns() {
        // Two empty rows stay apart by one; columns never spread.
        let column = image(2, 6, |x, y| {
            [0, 0, 0, if x == 0 && (y == 0 || y == 5) { 255 } else { 0 }]
        });
        let closed = close_row_gaps(&column);
        assert_eq!(
            alphas(&closed),
            vec![255, 0, 255, 0, 0, 0, 0, 0, 255, 0, 255, 0]
        );
    }

    #[test]
    fn recolours_keeping_alpha() {
        let icon = image(2, 1, |x, _| [1, 2, 3, x as u8 * 100]);
        let teal = recolor(&icon, [32, 180, 165]);
        assert_eq!(teal.pixels, vec![32, 180, 165, 0, 32, 180, 165, 100]);
    }

    #[test]
    fn mean_colour_weighs_by_alpha() {
        let icon = image(3, 1, |x, _| match x {
            0 => [0, 0, 0, 0],
            1 => [100, 200, 0, 255],
            _ => [200, 0, 100, 85],
        });
        assert_eq!(mean_color(&icon), Some([125, 150, 25]));
        assert_eq!(mean_color(&image(1, 1, |_, _| [9, 9, 9, 0])), None);
    }

    #[test]
    fn silhouette_is_closed_and_filled_with_the_mean() {
        let icon = image(
            1,
            3,
            |_, y| if y == 1 { [0; 4] } else { [40, 80, 120, 255] },
        );
        let (shape, mean) = silhouette(&icon);
        assert_eq!(mean, Some([40, 80, 120]));
        assert_eq!(shape.pixels, [40, 80, 120, 255].repeat(3));
    }

    #[test]
    fn engraving_is_white_with_alpha_by_brightness() {
        let picture = image(4, 1, |x, _| match x {
            0 => [200, 200, 200, 255],
            1 => [100, 100, 100, 255],
            2 => [200, 200, 200, 128],
            _ => [255, 255, 255, 0],
        });
        let out = engraving(&picture);
        let alphas: Vec<u8> = out.pixels.chunks(4).map(|p| p[3]).collect();
        assert_eq!(alphas, [255, 127, 128, 0]);
        assert!(out.pixels.chunks(4).all(|p| p[..3] == [255, 255, 255]));
    }

    #[test]
    fn crops_and_clips() {
        let picture = image(4, 3, |x, y| [x as u8, y as u8, 0, 255]);
        let part = crop(&picture, [1, 1, 2, 5]);
        assert_eq!((part.width, part.height), (2, 2));
        assert_eq!(&part.pixels[..8], &[1, 1, 0, 255, 2, 1, 0, 255]);
        assert_eq!(crop(&picture, [9, 0, 2, 2]).pixels, Vec::<u8>::new());
    }

    #[test]
    fn reads_the_crosshair_colour() {
        let system = volya_engine_formats::ltx::parse(
            b"[hud_cursor]\ncross_color = 0.7,0.7,0.7,1.0 ; grey\n",
        )
        .unwrap();
        assert_eq!(cross_color(&system), Some([0.7, 0.7, 0.7, 1.0]));
        assert_eq!(cross_color(&Config::default()), None);
    }
}
