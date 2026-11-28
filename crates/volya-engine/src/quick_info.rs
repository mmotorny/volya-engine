// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The HUD's hint static `quick_info` (docs/behavior/hud-scope.md 13,
//! `hud-pickup-hint`, whose spec is TODO(#180)): `maingame.xml`'s element, shown exactly while the player has a
//! hint, its colour animation started again whenever the picked object
//! changes (docs/formats/colour-animations.md 4).

use std::io;

use crate::core::ui::load_ui_file;
use crate::draw::Ctx;
use crate::pda_window::hint::Animation;
use crate::scene::Scene;
use volya_engine_formats::lanims::ColourAnimations;
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::layout::{StaticDesc, require};
use volya_engine_formats::ui::{Color, ColorDefs, Resolution, flag_attr};
use volya_engine_formats::xml::Document;

/// The HUD's layout file (`hud-scope` 13; `_16` at wide resolutions, `ui-layout`).
pub const FILE: &str = "maingame.xml";
/// The sprites' layer.
pub const LAYER: &str = "hud quick info";
/// The single nearest to 0.001 (colour-animations.md 4.2).
const MS: f32 = f32::from_bits(0x3A83_126F);

/// The `quick_info` static and its colour animation.
#[derive(Clone, Debug, PartialEq)]
pub struct QuickInfo {
    pub desc: StaticDesc,
    pub animation: Option<Animation>,
    /// The layout's resolution class it was read for (wide or not).
    wide: bool,
}

impl QuickInfo {
    /// Reads the layout for `resolution`, its animation from `animations`
    /// (`lanims.xr`; a name not found: none, colour-animations.md 4.1).
    pub fn load(
        ctx: &Ctx,
        resolution: Resolution,
        animations: &ColourAnimations,
    ) -> io::Result<Self> {
        let doc = load_ui_file(ctx.tree, &ctx.data.aliases, FILE, resolution)?;
        Self::read(&doc, &ctx.data.colors, animations, resolution.is_wide())
    }

    pub fn read(
        doc: &Document,
        colors: &ColorDefs,
        animations: &ColourAnimations,
        wide: bool,
    ) -> io::Result<Self> {
        let e = require(doc, "quick_info", 0)?;
        let animation = e
            .attr("light_anim")
            .filter(|n| !n.is_empty())
            .and_then(|name| animations.find(&crate::data::encode(name)))
            .map(|i| Animation {
                entry: animations.entries[i].clone(),
                cyclic: flag_attr(e, "la_cyclic", true),
                text: flag_attr(e, "la_text", true),
                alpha: flag_attr(e, "la_alpha", false),
            });
        Ok(Self {
            desc: StaticDesc::read(e, colors)?,
            animation,
            wide,
        })
    }

    /// Whether it was read for `resolution`'s layout.
    pub fn fits(&self, resolution: Resolution) -> bool {
        self.wide == resolution.is_wide()
    }

    /// The text colour at interface clock `now_ms` for a colour start set
    /// at `start_ms` (colour-animations.md 4.2, 4.3; no delay). Played once
    /// and past its length it holds the value of the last update before
    /// the end: the last frame's, as when the frames come faster than the
    /// animation's (our reading: updates are once per drawn frame).
    pub fn color(&self, start_ms: u64, now_ms: u64) -> Color {
        let base = (self.desc.text.as_ref()).map_or(Color::WHITE, |t| t.color);
        let Some(a) = self.animation.as_ref().filter(|a| a.text) else {
            return base;
        };
        let start = start_ms as f32 * MS;
        let e = now_ms as f32 * MS - start;
        let v = if a.cyclic || e < a.entry.length() {
            a.entry.colour_at(e.max(0.0)).0
        } else {
            (a.entry.colour_at_frame(a.entry.frame_count - 1)).unwrap_or(0)
        };
        let [vb, vg, vr, va] = v.to_le_bytes();
        if a.alpha {
            Color::rgba(base.r, base.g, base.b, va)
        } else {
            Color::rgba(vr, vg, vb, va)
        }
    }

    /// Adds the static's text `text` (Windows-1251), its colour animation
    /// started at `start_ms`.
    pub fn draw(
        &self,
        ctx: &mut Ctx,
        scene: &mut Scene,
        text: &[u8],
        start_ms: u64,
    ) -> io::Result<()> {
        let rect = self.desc.window.rect();
        let (mut text_box, font) = TextBox::of_static(&self.desc, rect);
        text_box.color = self.color(start_ms, ctx.clock_ms);
        ctx.text(scene, LAYER, text, &text_box, font)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::lanims::Entry;

    const LAYOUT: &str = r#"<w><quick_info x="1" y="700" width="1022" height="50" complex_mode="1" light_anim="ui_btn_hint" la_cyclic="0" la_text="1" la_alpha="1" la_texture="0"><text font="graffiti22" r="225" g="225" b="250" a="255" align="c"/></quick_info></w>"#;

    fn quick_info() -> QuickInfo {
        let animations = ColourAnimations {
            version: 0,
            entries: vec![Entry {
                name: b"ui_btn_hint".to_vec(),
                frame_rate: 30.0,
                frame_count: 30,
                keys: vec![(0, 0), (29, 0xFF00_0000)],
            }],
        };
        let doc = volya_engine_formats::xml::parse(LAYOUT.as_bytes()).unwrap();
        QuickInfo::read(&doc, &ColorDefs::default(), &animations, false).unwrap()
    }

    #[test]
    fn reads_the_static_and_its_animation() {
        let q = quick_info();
        let r = q.desc.window.rect();
        assert_eq!([r.x, r.y, r.width, r.height], [1.0, 700.0, 1022.0, 50.0]);
        assert!(q.desc.text_complex());
        let a = q.animation.as_ref().unwrap();
        assert!(!a.cyclic && a.text && a.alpha);
        assert!(q.fits(Resolution::new(1024, 768)));
    }

    /// `hud-scope` 13 (TODO(#180)): the text fades in over a second from when the door came
    /// under the crosshair, and holds.
    #[test]
    fn fades_in_once_from_its_start() {
        let q = quick_info();
        let at = |ms: u64| q.color(10_000, 10_000 + ms);
        assert_eq!(at(0), Color::rgba(225, 225, 250, 0));
        let half = at(500).a;
        assert!(half > 100 && half < 160, "{half}");
        assert_eq!(at(980), Color::rgba(225, 225, 250, 255));
        assert_eq!(at(1000), Color::rgba(225, 225, 250, 255));
        assert_eq!(at(60_000), Color::rgba(225, 225, 250, 255));
    }
}
