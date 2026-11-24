// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Building sprites from UI elements: a static's picture
//! (docs/formats/ui-layout.md section 5) and text
//! (docs/formats/text-and-fonts.md section 4), shared by the screens.

use std::io;

use crate::core::vfs::Vfs;
use crate::data::{Assets, GameData};
use crate::scene::{Scene, Sprite, TextureId};
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::text::layout::{TextBox, layout_text};
use volya_engine_formats::ui::layout::{Rect, StaticDesc};
use volya_engine_formats::ui::picture::{Picture, PicturePlacement};
use volya_engine_formats::ui::{Color, Resolution};

/// What a frame is drawn with.
pub struct Ctx<'a> {
    pub tree: &'a dyn Vfs,
    pub data: &'a GameData,
    pub assets: &'a mut Assets,
    pub resolution: Resolution,
    /// The engine clock, milliseconds.
    pub clock_ms: u64,
}

impl Ctx<'_> {
    pub fn scale(&self) -> (f32, f32) {
        self.resolution.scale()
    }

    /// Adds the picture of static `desc`, whose absolute rectangle is
    /// `absolute`, as layer `layer`: screen-aligned (section 5.3), or
    /// rotated by `angle` when the static is rotatable (section 5.4).
    /// Videos play from `video_start_ms`, looping when `looping`. `color`,
    /// when given, replaces the texture colour (colour animations). Nothing
    /// is added for a static without a texture.
    #[allow(clippy::too_many_arguments)]
    pub fn picture(
        &mut self,
        scene: &mut Scene,
        layer: &str,
        desc: &StaticDesc,
        absolute: Rect,
        angle: f32,
        video_start_ms: u64,
        looping: bool,
        color: Option<Color>,
    ) -> io::Result<()> {
        let Some(texture) = &desc.texture else {
            return Ok(());
        };
        let picture = Picture::resolve(texture, &self.data.descriptions);
        let resolved =
            self.assets
                .texture(self.tree, self.data, &picture.texture, self.clock_ms)?;
        let mut placement = PicturePlacement::new(desc, absolute, &picture, resolved.size());
        if let Some(color) = color {
            placement.color = color;
        }
        let id = resolved.id(video_start_ms, looping);
        let sprite = if desc.heading {
            let q = placement.rotated(self.scale(), angle, None, self.resolution.is_wide());
            Sprite::rotated(layer, id, &q)
        } else {
            Sprite::picture(layer, id, &placement.quad(self.scale()))
        };
        scene.sprites.push(sprite);
        Ok(())
    }

    /// Adds the glyphs of `text` (Windows-1251 bytes) in box `text_box` with
    /// font `font` as layer `layer`.
    pub fn text(
        &mut self,
        scene: &mut Scene,
        layer: &str,
        text: &[u8],
        text_box: &TextBox,
        font: UiFont,
    ) -> io::Result<()> {
        let scale = self.scale();
        let fonts = self
            .assets
            .fonts(self.tree, self.data, self.resolution.height)?;
        let font = fonts.get(font).clone();
        let layout = layout_text(&font, text, text_box, scale, &self.data.colors)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let path = Assets::font_texture(self.data, &layout.texture)?;
        let size = self.assets.image_size(self.tree, &path)?;
        let id = TextureId::Image { path };
        scene.sprites.extend(
            layout
                .quads
                .iter()
                .map(|g| Sprite::glyph(layer, id.clone(), g, size)),
        );
        Ok(())
    }
}
