// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The HUD's pick-up icon, the static `pick_up_item`
//! (docs/behavior/hud-pickup-hint.md 3, cited by section): the inventory
//! icon of the pick-up target (console-misc-settings.md 5.1), centred in
//! the element's layout rectangle and never larger than it. The pick-up
//! target itself is TODO(#627); until it exists the icon is hidden.

use std::io;

use crate::core::character::inventory::InvGrid;
use crate::core::ui::load_ui_file;
use crate::draw::{Ctx, picture_desc};
use crate::inventory_screen::layout::{ICON_PIXELS, ICONS};
use crate::quick_info::FILE;
use crate::scene::Scene;
use volya_engine_formats::ui::layout::{Rect, WindowDesc, require};
use volya_engine_formats::ui::{Color, ColorDefs, Resolution};
use volya_engine_formats::xml::Document;

/// The sprites' layer.
pub const LAYER: &str = "hud pick-up icon";
/// The picture's colour (3.2 step 4): white, alpha 192.
pub const COLOR: Color = Color::rgba(255, 255, 255, 192);

/// The element's icon area (3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickUpIcon {
    pub area: Rect,
    /// The layout's resolution class it was read for (wide or not).
    wide: bool,
}

impl PickUpIcon {
    /// Reads the layout for `resolution`.
    pub fn load(ctx: &Ctx, resolution: Resolution) -> io::Result<Self> {
        let doc = load_ui_file(ctx.tree, &ctx.data.aliases, FILE, resolution)?;
        Self::read(&doc, resolution.is_wide())
    }

    /// 3.1: the layout rectangle of `pick_up_item` (with the default
    /// alignment, not shifted).
    pub fn read(doc: &Document, wide: bool) -> io::Result<Self> {
        let e = require(doc, "pick_up_item", 0)?;
        let w = WindowDesc::read(e, &ColorDefs::default())?;
        Ok(Self {
            area: Rect::new(w.x, w.y, w.width, w.height),
            wide,
        })
    }

    /// Whether it was read for `resolution`'s layout.
    pub fn fits(&self, resolution: Resolution) -> bool {
        self.wide == resolution.is_wide()
    }

    /// The icon of an item of grid `grid` (3.2 steps 1 to 3): its texture
    /// rectangle in pixels and where it is drawn, scaled down to fit the
    /// area (single precision) and centred in it.
    pub fn place(&self, grid: InvGrid) -> (Rect, Rect) {
        let (iw, ih) = (
            grid.width as f32 * ICON_PIXELS,
            grid.height as f32 * ICON_PIXELS,
        );
        let texture = Rect::new(
            grid.x as f32 * ICON_PIXELS,
            grid.y as f32 * ICON_PIXELS,
            iw,
            ih,
        );
        let a = self.area;
        let s = (1.0f32.min(a.width / iw)).min(1.0f32.min(a.height / ih));
        let (w, h) = (iw * s, ih * s);
        let at = Rect::new(a.x + (a.width - w) * 0.5, a.y + (a.height - h) * 0.5, w, h);
        (texture, at)
    }

    /// Adds the icon of an item of grid `grid` (3.2), stretched, clipped
    /// to its rectangle.
    pub fn draw(&self, ctx: &mut Ctx, scene: &mut Scene, grid: InvGrid) -> io::Result<()> {
        let (texture, at) = self.place(grid);
        let mut desc = picture_desc(ICONS, at, Some(texture), COLOR);
        desc.stretch = true;
        desc.clipper = true;
        ctx.picture(scene, LAYER, &desc, at, 0.0, 0, false, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon() -> PickUpIcon {
        let layout = r#"<w><pick_up_item x="437" y="400" width="150" height="100"/></w>"#;
        let doc = volya_engine_formats::xml::parse(layout.as_bytes()).unwrap();
        PickUpIcon::read(&doc, false).unwrap()
    }

    /// 3.2's examples.
    #[test]
    fn the_icon_is_centred_and_never_larger_than_its_area() {
        let i = icon();
        assert_eq!(i.area, Rect::new(437.0, 400.0, 150.0, 100.0));
        let grid = |x, y, width, height| InvGrid {
            x,
            y,
            width,
            height,
        };
        // medkit: s = 1.
        let (t, at) = i.place(grid(6, 14, 2, 1));
        assert_eq!(t, Rect::new(300.0, 700.0, 100.0, 50.0));
        assert_eq!(at, Rect::new(462.0, 425.0, 100.0, 50.0));
        // wpn_ak74: s = 0.6.
        let (t, at) = i.place(grid(5, 5, 5, 2));
        assert_eq!(t, Rect::new(250.0, 250.0, 250.0, 100.0));
        assert!(
            (at.x - 437.0).abs() < 1e-4 && (at.y - 420.0).abs() < 1e-4,
            "{at:?}"
        );
        assert!(
            (at.width - 150.0).abs() < 1e-4 && (at.height - 60.0).abs() < 1e-4,
            "{at:?}"
        );
    }
}
