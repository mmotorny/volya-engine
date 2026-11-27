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
pub use volya_engine_formats::text::layout::Surface;
use volya_engine_formats::text::layout::{TextBox, draw_line, layout_text_on_surface};
use volya_engine_formats::ui::layout::{HAlign, Rect, StaticDesc, TextureRef, WindowDesc};
use volya_engine_formats::ui::picture::{Picture, PicturePlacement};
use volya_engine_formats::ui::{Color, Resolution};

/// What a frame is drawn with.
pub struct Ctx<'a> {
    pub tree: &'a dyn Vfs,
    pub data: &'a GameData,
    pub assets: &'a mut Assets,
    pub resolution: Resolution,
    /// Where pictures and text are laid out (text-and-fonts.md 4.4): the
    /// screen, or the off-screen target while the main menu draws into it
    /// (frame-presentation.md 2.5). Sprites are still given in screen
    /// pixels, mapped from the surface's ([`Ctx::to_screen`]).
    pub surface: Surface,
    /// The engine clock, milliseconds.
    pub clock_ms: u64,
}

impl Ctx<'_> {
    /// The screen's UI-to-pixel factors.
    pub fn scale(&self) -> (f32, f32) {
        self.resolution.scale()
    }

    /// A position in pixels of the layout surface as the screen position
    /// the presenter maps to that same position of the target: the inverse
    /// of its mapping of screen pixels to UI units (both with pixel centres
    /// at whole numbers), so the sprite is drawn into the target exactly
    /// where the surface placed it.
    fn to_screen(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        let (s, w) = (self.surface.size, self.surface.screen);
        if s == w {
            return [x, y];
        }
        [
            (x + 0.5) * w.width as f32 / s.width as f32 - 0.5,
            (y + 0.5) * w.height as f32 / s.height as f32 - 0.5,
        ]
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
        let Some((id, mut placement)) = self.placement(desc, absolute, video_start_ms, looping)?
        else {
            return Ok(());
        };
        if let Some(color) = color {
            placement.color = color;
        }
        let scale = self.surface.scale();
        let mut sprite = if desc.heading {
            let q = placement.rotated(scale, angle, None, self.resolution.is_wide());
            Sprite::rotated(layer, id, &q)
        } else {
            Sprite::picture(layer, id, &placement.quad(scale))
        };
        sprite.corners = sprite.corners.map(|c| self.to_screen(c));
        scene.sprites.push(sprite);
        Ok(())
    }

    /// The texture of static `desc` and its picture placed at `absolute`
    /// (ui-layout.md 5.3), before mapping to pixels; `None` for a static
    /// without a texture.
    pub fn placement(
        &mut self,
        desc: &StaticDesc,
        absolute: Rect,
        video_start_ms: u64,
        looping: bool,
    ) -> io::Result<Option<(TextureId, PicturePlacement)>> {
        let Some(texture) = &desc.texture else {
            return Ok(None);
        };
        let picture = Picture::resolve(texture, &self.data.descriptions);
        let resolved =
            self.assets
                .texture(self.tree, self.data, &picture.texture, self.clock_ms)?;
        let placement = PicturePlacement::new(desc, absolute, &picture, resolved.size());
        Ok(Some((resolved.id(video_start_ms, looping), placement)))
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
        // The font variant still follows the screen height (4.4).
        let fonts = self
            .assets
            .fonts(self.tree, self.data, self.resolution.height)?;
        let font = fonts.get(font).clone();
        let mut layout =
            layout_text_on_surface(&font, text, text_box, self.surface, &self.data.colors)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        for g in &mut layout.quads {
            let [min, max] = [g.min, g.max].map(|(x, y)| self.to_screen([x, y]));
            (g.min, g.max) = ((min[0], min[1]), (max[0], max[1]));
        }
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

    /// The size in texture pixels (= units at pixel size) of texture id
    /// `id`, `None` when no description has it.
    pub fn part_size(&self, id: &str) -> Option<(f32, f32)> {
        let r = self.data.descriptions.get(id)?.rect;
        Some((r.width, r.height))
    }

    /// Adds texture id `id` at its pixel size with its top-left at (`x`,
    /// `y`) units, cut to `size` units from its top-left when smaller (the
    /// partial tile of a repeated part, options-screens.md section 4.9).
    /// `color` multiplies the texels. Nothing for an unknown id.
    #[allow(clippy::too_many_arguments)]
    pub fn part(
        &mut self,
        scene: &mut Scene,
        layer: &str,
        id: &str,
        (x, y): (f32, f32),
        size: Option<(f32, f32)>,
        color: Color,
    ) -> io::Result<()> {
        let Some(d) = self.data.descriptions.get(id) else {
            return Ok(());
        };
        let r = d.rect;
        let (w, h) = size.map_or((r.width, r.height), |(w, h)| {
            (w.min(r.width), h.min(r.height))
        });
        if w <= 0.0 || h <= 0.0 {
            return Ok(());
        }
        let desc = picture_desc(
            id,
            Rect::new(x, y, w, h),
            Some(Rect::new(r.x, r.y, w, h)),
            color,
        );
        self.picture(
            scene,
            layer,
            &desc,
            Rect::new(x, y, w, h),
            0.0,
            0,
            false,
            None,
        )
    }

    /// Adds texture id `id` stretched over `rect` (units).
    pub fn part_stretched(
        &mut self,
        scene: &mut Scene,
        layer: &str,
        id: &str,
        rect: Rect,
    ) -> io::Result<()> {
        if self.data.descriptions.get(id).is_none() {
            return Ok(());
        }
        let mut desc = picture_desc(id, rect, None, Color::WHITE);
        desc.stretch = true;
        self.picture(scene, layer, &desc, rect, 0.0, 0, false, None)
    }

    /// `id` repeated from `from` to `to` along x (or y when `vertical`) at
    /// the other coordinate `at`, the last tile cut (section 4.9).
    #[allow(clippy::too_many_arguments)]
    fn repeat(
        &mut self,
        scene: &mut Scene,
        layer: &str,
        id: &str,
        from: f32,
        to: f32,
        at: f32,
        across: Option<f32>,
        vertical: bool,
        color: Color,
    ) -> io::Result<()> {
        let Some((w, h)) = self.part_size(id) else {
            return Ok(());
        };
        let step = if vertical { h } else { w };
        if step <= 0.0 {
            return Ok(());
        }
        let mut p = from;
        while p < to - 0.001 {
            let len = (to - p).min(step);
            let (pos, size) = if vertical {
                ((at, p), (across.unwrap_or(w), len))
            } else {
                ((p, at), (len, across.unwrap_or(h)))
            };
            self.part(scene, layer, id, pos, Some(size), color)?;
            p += step;
        }
        Ok(())
    }

    /// A frame window with texture base `base` over `rect` (section 4.9):
    /// corners at pixel size, edges repeated between them and `_back` over
    /// the interior, each run ending with a cut tile.
    pub fn frame_window(
        &mut self,
        scene: &mut Scene,
        layer: &str,
        base: &str,
        rect: Rect,
    ) -> io::Result<()> {
        let c = Color::WHITE;
        let size =
            |s: &Self, part: &str| s.part_size(&format!("{base}_{part}")).unwrap_or_default();
        let (lt, rt, lb) = (size(self, "lt"), size(self, "rt"), size(self, "lb"));
        let rb = size(self, "rb");
        let (x0, y0) = (rect.x, rect.y);
        let (x1, y1) = (rect.x + rect.width, rect.y + rect.height);
        let id = |part: &str| format!("{base}_{part}");
        // The interior first, then edges and corners over it.
        let mut y = y0 + lt.1;
        let back = size(self, "back");
        while back.1 > 0.0 && y < y1 - lb.1 - 0.001 {
            let h = (y1 - lb.1 - y).min(back.1);
            self.repeat(
                scene,
                layer,
                &id("back"),
                x0 + lt.0,
                x1 - rt.0,
                y,
                Some(h),
                false,
                c,
            )?;
            y += back.1;
        }
        self.repeat(
            scene,
            layer,
            &id("t"),
            x0 + lt.0,
            x1 - rt.0,
            y0,
            None,
            false,
            c,
        )?;
        let b = size(self, "b");
        self.repeat(
            scene,
            layer,
            &id("b"),
            x0 + lb.0,
            x1 - rb.0,
            y1 - b.1,
            None,
            false,
            c,
        )?;
        self.repeat(
            scene,
            layer,
            &id("l"),
            y0 + lt.1,
            y1 - lb.1,
            x0,
            None,
            true,
            c,
        )?;
        let r = size(self, "r");
        self.repeat(
            scene,
            layer,
            &id("r"),
            y0 + rt.1,
            y1 - rb.1,
            x1 - r.0,
            None,
            true,
            c,
        )?;
        self.part(scene, layer, &id("lt"), (x0, y0), None, c)?;
        self.part(scene, layer, &id("rt"), (x1 - rt.0, y0), None, c)?;
        self.part(scene, layer, &id("lb"), (x0, y1 - lb.1), None, c)?;
        self.part(scene, layer, &id("rb"), (x1 - rb.0, y1 - rb.1), None, c)
    }

    /// A horizontal frame line with texture base `base` from (`x`, `y`),
    /// `width` long (section 4.9): `_b` at the left, `_e` at the right,
    /// `_back` repeated between.
    pub fn frame_line(
        &mut self,
        scene: &mut Scene,
        layer: &str,
        base: &str,
        at: (f32, f32),
        width: f32,
    ) -> io::Result<()> {
        self.frame_line_colored(scene, layer, base, at, width, Color::WHITE)
    }

    /// [`Ctx::frame_line`] with its three parts in colour `c`.
    pub fn frame_line_colored(
        &mut self,
        scene: &mut Scene,
        layer: &str,
        base: &str,
        (x, y): (f32, f32),
        width: f32,
        c: Color,
    ) -> io::Result<()> {
        let (b, e) = (format!("{base}_b"), format!("{base}_e"));
        let bw = self.part_size(&b).map_or(0.0, |s| s.0);
        let ew = self.part_size(&e).map_or(0.0, |s| s.0);
        self.repeat(
            scene,
            layer,
            &format!("{base}_back"),
            x + bw,
            x + width - ew,
            y,
            None,
            false,
            c,
        )?;
        self.part(scene, layer, &b, (x, y), None, c)?;
        self.part(scene, layer, &e, (x + width - ew, y), None, c)
    }

    /// Adds the glyphs of one string `text` (Windows-1251 bytes) drawn at
    /// pen `pen` in output pixels with font `font` (text-and-fonts.md
    /// section 3) as layer `layer`.
    #[allow(clippy::too_many_arguments)]
    pub fn line(
        &mut self,
        scene: &mut Scene,
        layer: &str,
        text: &[u8],
        pen: (f32, f32),
        color: Color,
        align: HAlign,
        font: UiFont,
    ) -> io::Result<()> {
        let fonts = self
            .assets
            .fonts(self.tree, self.data, self.resolution.height)?;
        let font = fonts.get(font).clone();
        let path = Assets::font_texture(self.data, &font.texture)?;
        let size = self.assets.image_size(self.tree, &path)?;
        let id = TextureId::Image { path };
        scene.sprites.extend(
            draw_line(&font, text, pen, color, align)
                .iter()
                .map(|g| Sprite::glyph(layer, id.clone(), g, size)),
        );
        Ok(())
    }
}

/// A static showing texture `name` (a texture id or path) with texture
/// rectangle `texture_rect` (pixels), at pixel size over `rect`.
pub fn picture_desc(
    name: &str,
    rect: Rect,
    texture_rect: Option<Rect>,
    color: Color,
) -> StaticDesc {
    StaticDesc {
        window: WindowDesc {
            name: String::new(),
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            centred: false,
            font: None,
            window_name: None,
            auto_statics: Vec::new(),
        },
        texture: Some(TextureRef {
            name: name.to_owned(),
            rect: texture_rect,
            color,
        }),
        stretch: false,
        texture_offset: (0.0, 0.0),
        heading: false,
        clipper: false,
        text: None,
        complex_mode: false,
        text_colors: None,
    }
}

/// The menu cursor (title-menu.md section 7): its frame at `ctx.clock_ms`
/// with its top-left at `at` (UI units), one texel per unit.
pub fn cursor(ctx: &mut Ctx, scene: &mut Scene, at: [f32; 2]) -> io::Result<()> {
    let frame = ctx.data.cursor.frame(ctx.clock_ms);
    let path = crate::core::ui::still_image(ctx.tree, &ctx.data.aliases, frame)?;
    let size = ctx.assets.image_size(ctx.tree, &path)?;
    let placement = PicturePlacement {
        position: (at[0], at[1]),
        size: (size.0 as f32, size.1 as f32),
        texture_rect: Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32),
        texture_size: (size.0 as f32, size.1 as f32),
        color: Color::WHITE,
    };
    scene.sprites.push(Sprite::picture(
        "cursor",
        TextureId::Image { path },
        &placement.quad(ctx.scale()),
    ));
    Ok(())
}
