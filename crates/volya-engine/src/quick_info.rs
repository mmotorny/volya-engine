// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The HUD's hint line, the static `quick_info`
//! (docs/behavior/hud-pickup-hint.md 2, cited by section):
//! `maingame.xml`'s element (2.1), updated from the player's hint after
//! the HUD is drawn (2.2), its colour animation started again whenever
//! the crosshair record's object changes (2.3;
//! docs/formats/colour-animations.md 4).

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

/// The line's state (2.2): shown or hidden, its text, its colour start
/// and the object remembered from its last update, which is one value for
/// the whole run of the game (2.2: never reset by a new HUD, a level
/// change or a load).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HintLine {
    pub shown: bool,
    /// The text, as the string table gives it.
    pub text: String,
    /// The colour start on the interface clock, ms; none before the
    /// line's first update.
    pub start_ms: Option<u64>,
    remembered: Option<u16>,
}

impl HintLine {
    /// The update at the end of the HUD's drawing (2.2) from the player's
    /// hint `hint` (as [`PlayerHint::resolve`] gives it; none: none) and the
    /// crosshair record's object `object`, at interface clock `now_ms`;
    /// `lookup` the string table's (`TF` 1.4). The text is compared
    /// ignoring ASCII letter case (our reading of 2.2 step 2).
    ///
    /// [`PlayerHint::resolve`]: crate::player_hint::PlayerHint::resolve
    pub fn update<'a>(
        &mut self,
        hint: Option<&'a str>,
        object: Option<u16>,
        now_ms: u64,
        lookup: impl Fn(&'a str) -> &'a str,
    ) {
        // CA 4.2: the first update sets the colour start.
        self.start_ms.get_or_insert(now_ms);
        self.shown = hint.is_some();
        if let Some(h) = hint
            && !h.eq_ignore_ascii_case(&self.text)
        {
            self.text = lookup(h).to_owned();
        }
        if object != self.remembered {
            self.text = hint.map_or("", &lookup).to_owned();
            self.start_ms = Some(now_ms);
            self.remembered = object;
        }
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

    /// 2.1: the text fades in over a second from the line's colour start,
    /// and holds.
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

    fn lookup(key: &str) -> &str {
        match key {
            "character_use" => "Talk (F)",
            "tip_door_locked" => "Locked",
            k => k,
        }
    }

    #[test]
    fn the_line_shows_the_hint_and_restarts_its_fade_on_another_object() {
        let mut l = HintLine::default();
        l.update(None, None, 100, lookup);
        assert_eq!(
            (l.shown, l.text.as_str(), l.start_ms),
            (false, "", Some(100))
        );
        // A stalker within reach: shown, faded in from now.
        l.update(Some("character_use"), Some(3), 200, lookup);
        assert_eq!(
            (l.shown, l.text.as_str(), l.start_ms),
            (true, "Talk (F)", Some(200))
        );
        // Another stalker, the same hint: the fade starts again.
        l.update(Some("character_use"), Some(4), 300, lookup);
        assert_eq!(l.start_ms, Some(300));
        // The same object, another hint: the text changes, the fade not.
        l.update(Some("Locked"), Some(4), 400, lookup);
        assert_eq!((l.text.as_str(), l.start_ms), ("Locked", Some(300)));
        // Compared ignoring case: kept.
        l.update(Some("LOCKED"), Some(4), 450, lookup);
        assert_eq!(l.text, "Locked");
        // An empty hint shows the line with no text.
        l.update(Some(""), Some(5), 500, lookup);
        assert_eq!((l.shown, l.text.as_str()), (true, ""));
        // None: hidden, the text emptied as the object changes.
        l.update(None, None, 600, lookup);
        assert_eq!(
            (l.shown, l.text.as_str(), l.start_ms),
            (false, "", Some(600))
        );
        // The same none again: no restart.
        l.update(None, None, 700, lookup);
        assert_eq!(l.start_ms, Some(600));
    }
}
