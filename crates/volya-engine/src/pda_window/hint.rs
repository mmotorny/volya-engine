// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The hint window of buttons (docs/behavior/pda-controls.md section 6):
//! `hint_item.xml`'s `button_hint`, a frame line and a description whose
//! colour animation fades the text in once, placed by the cursor when a
//! button with a hint has had the cursor on it for over 500 ms of the
//! global clock.

use std::io;

use crate::core::ui::load_ui_file;
use crate::core::vfs::{self, Vfs};
use crate::data::GameData;
use volya_engine_formats::lanims::{ColourAnimations, Entry};
use volya_engine_formats::ui::layout::{Rect, StaticDesc, require};
use volya_engine_formats::ui::{Color, ColorDefs, Resolution, flag_attr};
use volya_engine_formats::xml::Document;

/// The hint window's file (section 6).
pub const FILE: &str = "hint_item.xml";
/// The cursor rests this long on a button before its hint shows, global
/// clock milliseconds (section 6).
pub const DELAY_MS: u64 = 500;
/// The hint window's least width, and its width beyond the description's
/// (section 6).
const MIN_WIDTH: f32 = 80.0;
const MARGIN: f32 = 30.0;
/// How far below the cursor the last placement puts it (section 6).
const BELOW: f32 = 45.0;
/// The single nearest to 0.001 (colour-animations.md 4.2).
const MS: f32 = f32::from_bits(0x3A83_126F);

/// The description's colour animation (colour-animations.md 4.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub entry: Entry,
    /// `la_cyclic`.
    pub cyclic: bool,
    /// `la_text`.
    pub text: bool,
    /// `la_alpha`.
    pub alpha: bool,
}

/// `button_hint` (section 6).
#[derive(Clone, Debug, PartialEq)]
pub struct HintDesc {
    /// The window's height (its width follows the text).
    pub height: f32,
    /// The frame line's texture base.
    pub line: String,
    pub description: StaticDesc,
    pub animation: Option<Animation>,
}

impl HintDesc {
    /// Reads `hint_item.xml` with the colour animation library
    /// `lanims.xr` (without it, no animation: colour-animations.md 4.1).
    pub fn load(tree: &dyn Vfs, data: &GameData, resolution: Resolution) -> io::Result<Self> {
        let doc = load_ui_file(tree, &data.aliases, FILE, resolution)?;
        let path = data.aliases.path("$game_data$", "lanims.xr")?;
        let animations = match vfs::read(tree, &path) {
            Ok(bytes) => ColourAnimations::parse(&bytes)
                .map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => ColourAnimations::default(),
            Err(e) => return Err(e),
        };
        Self::read(&doc, &data.colors, &animations)
    }

    pub fn read(
        doc: &Document,
        colors: &ColorDefs,
        animations: &ColourAnimations,
    ) -> io::Result<Self> {
        let window = StaticDesc::read(require(doc, "button_hint", 0)?, colors)?;
        let line = StaticDesc::read(require(doc, "button_hint:frame_line", 0)?, colors)?;
        let e = require(doc, "button_hint:description", 0)?;
        let animation = e
            .attr("light_anim")
            .and_then(|name| animations.find(&crate::data::encode(name)))
            .map(|i| Animation {
                entry: animations.entries[i].clone(),
                cyclic: flag_attr(e, "la_cyclic", true),
                text: flag_attr(e, "la_text", true),
                alpha: flag_attr(e, "la_alpha", false),
            });
        Ok(Self {
            height: window.window.height,
            line: line.texture.map(|t| t.name).unwrap_or_default(),
            description: StaticDesc::read(e, colors)?,
            animation,
        })
    }

    /// The description's text colour: the layout's (7.1).
    pub fn text_color(&self) -> Color {
        (self.description.text.as_ref()).map_or(Color::WHITE, |t| t.color)
    }
}

/// The hint window while a button holds it (section 6).
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    /// The button holding it.
    pub owner: usize,
    pub text: String,
    /// The window's rectangle, UI units.
    pub rect: Rect,
    /// The description's width (the text's).
    pub text_width: f32,
    /// The animation's start, seconds of the interface clock.
    start: f32,
    /// The text colour as the last update left it.
    pub color: Color,
}

/// Seconds of the interface clock at `ms` (colour-animations.md 4.2).
fn seconds(ms: u64) -> f32 {
    ms as f32 * MS
}

impl Hint {
    /// The hint window taken by button `owner` with text `text`
    /// `text_width` units wide, the cursor at `cursor` and the interface
    /// clock at `now` (section 6 steps 1–5).
    pub fn take(
        desc: &HintDesc,
        owner: usize,
        text: String,
        text_width: f32,
        cursor: [f32; 2],
        now: u64,
    ) -> Self {
        let w = MIN_WIDTH.max(text_width + MARGIN);
        let h = desc.height;
        let [cx, cy] = cursor;
        let off = |x: f32, y: f32| x <= 0.0 || x + w >= 1024.0 || y <= 0.0 || y + h >= 768.0;
        let mut at = (cx, cy - h);
        for next in [(cx - w, cy - h), (cx - w, cy), (cx, cy + BELOW)] {
            if !off(at.0, at.1) {
                break;
            }
            at = next;
        }
        let mut hint = Self {
            owner,
            text,
            rect: Rect::new(at.0, at.1, w, h),
            text_width,
            start: seconds(now),
            color: desc.text_color(),
        };
        hint.update(desc, now);
        hint
    }

    /// The description's colour animation at `now` (colour-animations.md
    /// 4.3); played once, it holds its last value after its length.
    pub fn update(&mut self, desc: &HintDesc, now: u64) {
        let Some(a) = &desc.animation else { return };
        if !a.text {
            return;
        }
        let e = seconds(now) - self.start;
        if e < 0.0 || !a.cyclic && e >= a.entry.length() {
            return;
        }
        let v = a.entry.colour_at(e).0;
        let [vb, vg, vr, va] = v.to_le_bytes();
        self.color = if a.alpha {
            Color::rgba(self.color.r, self.color.g, self.color.b, va)
        } else {
            Color::rgba(vr, vg, vb, va)
        };
    }
}
