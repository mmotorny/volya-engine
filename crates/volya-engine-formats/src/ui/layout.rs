// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Element descriptions read from UI layout XML (docs/formats/ui-layout.md,
//! sections 4, 5, 7 and 9). Which element is read as what is the screen
//! builder's decision; these readers interpret an element once chosen.

use std::io;

use super::{Color, ColorDefs, flag_attr, float_attr, int_attr, read_color};
use crate::text::font::FontChoice;
use crate::xml::{Document, Element};

/// A rectangle in UI units (or, for texture rectangles, texture pixels).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// The element at `path`/`index` of `doc` (ui-layout.md section 1.4), which a
/// builder requires: missing is an error.
pub fn require<'a>(doc: &'a Document, path: &str, index: usize) -> io::Result<&'a Element> {
    doc.find(path, index).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("UI element {path} (index {index}) not found"),
        )
    })
}

/// A font reference with its colour: a window's `font` child (section 4.2).
#[derive(Clone, Debug, PartialEq)]
pub struct FontDesc {
    pub font: Option<FontChoice>,
    pub color: Color,
}

/// The `font` attribute of `e`: absent or empty is `None` (section 7.2).
pub fn font_attr(e: &Element) -> Option<FontChoice> {
    e.attr("font")
        .filter(|f| !f.is_empty())
        .map(FontChoice::from_name)
}

impl FontDesc {
    pub fn read(e: &Element, defs: &ColorDefs) -> Self {
        Self {
            font: font_attr(e),
            color: read_color(e, defs, 255),
        }
    }
}

/// What every window reads (section 4).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowDesc {
    /// The element's name.
    pub name: String,
    /// `x`, `y`, `width`, `height` as written, relative to the parent.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// `alignment` contains `c`: (`x`, `y`) is the centre.
    pub centred: bool,
    /// `font` child (section 4.2).
    pub font: Option<FontDesc>,
    /// `window_name` child value.
    pub window_name: Option<String>,
    /// `auto_static` children, in document order (section 4.2).
    pub auto_statics: Vec<StaticDesc>,
}

impl WindowDesc {
    pub fn read(e: &Element, defs: &ColorDefs) -> Self {
        Self {
            name: e.name.clone(),
            x: float_attr(e, "x", 0.0),
            y: float_attr(e, "y", 0.0),
            width: float_attr(e, "width", 0.0),
            height: float_attr(e, "height", 0.0),
            centred: e.attr("alignment").is_some_and(|a| a.contains('c')),
            font: e.child("font").map(|f| FontDesc::read(f, defs)),
            window_name: e
                .child("window_name")
                .and_then(|w| w.value().map(str::to_owned)),
            auto_statics: e
                .children_named("auto_static")
                .map(|s| StaticDesc::read(s, defs))
                .collect(),
        }
    }

    /// The rectangle relative to the parent's top-left, centre-corrected
    /// (section 4.1). Uses the current `width`/`height`, so a centred
    /// window whose width code changes stays centred.
    pub fn rect(&self) -> Rect {
        let (x, y) = if self.centred {
            (self.x - self.width / 2.0, self.y - self.height / 2.0)
        } else {
            (self.x, self.y)
        };
        Rect::new(x, y, self.width, self.height)
    }

    /// The absolute rectangle for a parent whose absolute top-left is
    /// (`px`, `py`) (section 4.1).
    pub fn absolute(&self, px: f32, py: f32) -> Rect {
        let r = self.rect();
        Rect::new(px + r.x, py + r.y, r.width, r.height)
    }
}

/// A `texture` child (section 5.1): the value (texture id or path), the
/// texture rectangle when both `width` and `height` are non-zero (section
/// 5.2), and the texture colour (default opaque white).
#[derive(Clone, Debug, PartialEq)]
pub struct TextureRef {
    pub name: String,
    pub rect: Option<Rect>,
    pub color: Color,
}

impl TextureRef {
    /// `None` when the child has no value: the data then sets no picture
    /// (the spec only describes `texture` children with a value).
    pub fn read(e: &Element, defs: &ColorDefs) -> Option<Self> {
        let name = e.value()?.to_owned();
        let rect = Rect::new(
            float_attr(e, "x", 0.0),
            float_attr(e, "y", 0.0),
            float_attr(e, "width", 0.0),
            float_attr(e, "height", 0.0),
        );
        Some(Self {
            name,
            rect: (rect.width != 0.0 && rect.height != 0.0).then_some(rect),
            color: read_color(e, defs, 255),
        })
    }
}

/// Horizontal text alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HAlign {
    #[default]
    Left,
    Right,
    Centre,
}

impl HAlign {
    /// `l`, `r`, `c` exactly; anything else is `None` (keeps the default).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "l" => Some(Self::Left),
            "r" => Some(Self::Right),
            "c" => Some(Self::Centre),
            _ => None,
        }
    }
}

/// Vertical text alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VAlign {
    #[default]
    Top,
    Centre,
    Bottom,
}

impl VAlign {
    /// `t`, `c`, `b` exactly; anything else is `None` (keeps the default).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "t" => Some(Self::Top),
            "c" => Some(Self::Centre),
            "b" => Some(Self::Bottom),
            _ => None,
        }
    }
}

/// A `text` child (section 7.1; text-and-fonts.md section 4.1).
#[derive(Clone, Debug, PartialEq)]
pub struct TextDesc {
    /// The value: a string-table key; `None` when the element has none.
    pub key: Option<String>,
    /// `None`: the text keeps its current font (default `letterica16`).
    pub font: Option<FontChoice>,
    /// Always applied; default opaque white.
    pub color: Color,
    /// `None`: unchanged (default left).
    pub align: Option<HAlign>,
    /// `None`: unchanged (default top).
    pub vert_align: Option<VAlign>,
    /// `x`, `y`: offset in units from the element's top-left.
    pub offset: (f32, f32),
    /// `complex_mode` when present: non-zero for multi-line text.
    pub complex_mode: Option<bool>,
}

impl TextDesc {
    pub fn read(e: &Element, defs: &ColorDefs) -> Self {
        Self {
            key: e.value().map(str::to_owned),
            font: font_attr(e),
            color: read_color(e, defs, 255),
            align: e.attr("align").and_then(HAlign::parse),
            vert_align: e.attr("vert_align").and_then(VAlign::parse),
            offset: (float_attr(e, "x", 0.0), float_attr(e, "y", 0.0)),
            complex_mode: e
                .attr("complex_mode")
                .map(|_| flag_attr(e, "complex_mode", false)),
        }
    }
}

/// Button state text colours: a `text_color` child with `e`, `d`, `t`, `h`
/// children (section 7.1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextStateColors {
    pub enabled: Option<Color>,
    pub disabled: Option<Color>,
    pub touched: Option<Color>,
    pub highlighted: Option<Color>,
}

impl TextStateColors {
    /// `rgb_default` is the default of missing `r`, `g`, `b` (section 2: 0
    /// for list text-state colours; the spec gives no other default for
    /// state colours, so callers pass 0 unless they know better).
    pub fn read(e: &Element, defs: &ColorDefs, rgb_default: u8) -> Self {
        let c = |n| e.child(n).map(|c| read_color(c, defs, rgb_default));
        Self {
            enabled: c("e"),
            disabled: c("d"),
            touched: c("t"),
            highlighted: c("h"),
        }
    }
}

/// A static element: a window with an optional picture and text (section 5).
#[derive(Clone, Debug, PartialEq)]
pub struct StaticDesc {
    pub window: WindowDesc,
    pub texture: Option<TextureRef>,
    /// `stretch` (on the element; one on the `texture` child is ignored).
    pub stretch: bool,
    /// `texture_offset` child `x`, `y`.
    pub texture_offset: (f32, f32),
    pub heading: bool,
    pub clipper: bool,
    pub text: Option<TextDesc>,
    /// Element-level `complex_mode`: non-zero forces multi-line text; 0 does
    /// not switch it off.
    pub complex_mode: bool,
    pub text_colors: Option<TextStateColors>,
}

impl StaticDesc {
    pub fn read(e: &Element, defs: &ColorDefs) -> Self {
        Self {
            window: WindowDesc::read(e, defs),
            texture: e.child("texture").and_then(|t| TextureRef::read(t, defs)),
            stretch: flag_attr(e, "stretch", false),
            texture_offset: e.child("texture_offset").map_or((0.0, 0.0), |o| {
                (float_attr(o, "x", 0.0), float_attr(o, "y", 0.0))
            }),
            heading: flag_attr(e, "heading", false),
            clipper: flag_attr(e, "clipper", false),
            text: e.child("text").map(|t| TextDesc::read(t, defs)),
            complex_mode: flag_attr(e, "complex_mode", false),
            text_colors: e
                .child("text_color")
                .map(|c| TextStateColors::read(c, defs, 0)),
        }
    }

    /// Whether the text is multi-line (sections 5.1 and 7.1).
    pub fn text_complex(&self) -> bool {
        self.complex_mode || self.text.as_ref().and_then(|t| t.complex_mode) == Some(true)
    }
}

/// A vertical scroll list (section 9.1).
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollListDesc {
    pub window: WindowDesc,
    /// x of every item (sic: the left offset despite the name).
    pub right_ident: f32,
    /// Read; no layout effect was established.
    pub left_ident: f32,
    pub top_indent: f32,
    pub bottom_indent: f32,
    pub vert_interval: f32,
    pub inverse_dir: bool,
    pub flip_vert: bool,
    /// `always_show_scroll` exactly 1 (the default): the scroll bar is
    /// always shown; otherwise only when the items overflow the view.
    pub always_show_scroll: bool,
    pub can_select: bool,
    pub scroll_profile: String,
    /// `text` children: multi-line text items.
    pub items: Vec<TextDesc>,
}

impl ScrollListDesc {
    pub fn read(e: &Element, defs: &ColorDefs) -> Self {
        Self {
            window: WindowDesc::read(e, defs),
            right_ident: float_attr(e, "right_ident", 0.0),
            left_ident: float_attr(e, "left_ident", 0.0),
            top_indent: float_attr(e, "top_indent", 0.0),
            bottom_indent: float_attr(e, "bottom_indent", 0.0),
            vert_interval: float_attr(e, "vert_interval", 0.0),
            inverse_dir: flag_attr(e, "inverse_dir", false),
            flip_vert: flag_attr(e, "flip_vert", false),
            always_show_scroll: int_attr(e, "always_show_scroll", 1) == 1,
            can_select: flag_attr(e, "can_select", false),
            scroll_profile: e.attr("scroll_profile").unwrap_or("default").to_owned(),
            items: e
                .children_named("text")
                .map(|t| TextDesc::read(t, defs))
                .collect(),
        }
    }

    /// Lays out items of the given heights (section 9.1): the top-left of
    /// each, relative to the list, in list order, and the content height.
    /// Item *j* is at x = `right_ident`, y = `top_indent` + Σ previous
    /// (height + `vert_interval`) + `vert_interval`. With `flip_vert` the
    /// items are stacked in reverse order; with `inverse_dir` the block is
    /// moved so that its bottom meets the bottom of the view (the spec gives
    /// no formula for these two; this is the plain reading).
    pub fn layout(&self, heights: &[f32]) -> (Vec<(f32, f32)>, f32) {
        let content = self.top_indent
            + self.bottom_indent
            + heights.iter().map(|h| h + self.vert_interval).sum::<f32>();
        let shift = if self.inverse_dir {
            self.window.height - content
        } else {
            0.0
        };
        let mut positions = vec![(0.0, 0.0); heights.len()];
        let mut y = self.top_indent;
        let order: Box<dyn Iterator<Item = usize>> = if self.flip_vert {
            Box::new((0..heights.len()).rev())
        } else {
            Box::new(0..heights.len())
        };
        for j in order {
            y += self.vert_interval;
            positions[j] = (self.right_ident, y + shift);
            y += heights[j];
        }
        (positions, content)
    }

    /// Whether the scroll bar shows for `content` height (section 9.1).
    pub fn shows_scroll_bar(&self, content: f32) -> bool {
        self.always_show_scroll || content > self.window.height
    }

    /// The clip rectangle of the items, relative to the list: the view
    /// shortened by `top_indent` and `bottom_indent` (section 9.1).
    pub fn clip(&self) -> Rect {
        Rect::new(
            0.0,
            self.top_indent,
            self.window.width,
            self.window.height - self.top_indent - self.bottom_indent,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::font::UiFont;
    use crate::xml::parse;

    fn defs() -> ColorDefs {
        ColorDefs::read(
            &parse(b"<c><color name=\"ui_clr_3\" r=\"135\" g=\"123\" b=\"116\"/></c>").unwrap(),
        )
    }

    #[test]
    fn reads_statics() {
        let doc = parse(
            b"<w><s x=\"43\" y=\"37\" width=\"55\" height=\"55\" stretch=\"1\" heading=\"1\" \
            alignment=\"c\" clipper=\"0\" complex_mode=\"1\">\
            <texture x=\"0\" y=\"0\" width=\"876\" height=\"0\" color=\"ui_clr_3\">ui_menu_bigroll_l</texture>\
            <texture_offset x=\"2\" y=\"3.5\"/>\
            <window_name> win </window_name>\
            <font font=\"medium\" r=\"1\"/>\
            <text font=\"graffiti19\" align=\"l\" vert_align=\"x\" x=\"1\" complex_mode=\"0\">key</text>\
            <auto_static x=\"3\" y=\"12\" width=\"19\" height=\"18\"><texture>ui_hud_icon_car</texture></auto_static>\
            <text_color><e r=\"10\"/><h color=\"ui_clr_3\"/></text_color>\
            </s><bare><texture/><text font=\"header\"/></bare></w>",
        )
        .unwrap();
        let s = StaticDesc::read(require(&doc, "s", 0).unwrap(), &defs());
        assert_eq!(s.window.rect(), Rect::new(15.5, 9.5, 55.0, 55.0));
        assert_eq!(
            s.window.absolute(599.0, 419.0),
            Rect::new(614.5, 428.5, 55.0, 55.0)
        );
        assert!(s.stretch && s.heading && !s.clipper && s.complex_mode && s.text_complex());
        let t = s.texture.as_ref().unwrap();
        assert_eq!(t.name, "ui_menu_bigroll_l");
        assert_eq!(t.rect, None, "height 0 ignores the rectangle");
        assert_eq!(t.color, Color::rgba(135, 123, 116, 255));
        assert_eq!(s.texture_offset, (2.0, 3.5));
        assert_eq!(s.window.window_name.as_deref(), Some("win"));
        let f = s.window.font.as_ref().unwrap();
        assert_eq!(f.font, Some(FontChoice::Font(UiFont::Medium)));
        assert_eq!(f.color, Color::rgba(1, 255, 255, 255));
        let text = s.text.as_ref().unwrap();
        assert_eq!(text.key.as_deref(), Some("key"));
        assert_eq!(text.font, Some(FontChoice::Font(UiFont::Graffiti19)));
        assert_eq!(text.align, Some(HAlign::Left));
        assert_eq!(text.vert_align, None);
        assert_eq!(text.offset, (1.0, 0.0));
        assert_eq!(text.complex_mode, Some(false));
        assert_eq!(s.window.auto_statics.len(), 1);
        assert_eq!(
            s.window.auto_statics[0].texture.as_ref().unwrap().name,
            "ui_hud_icon_car"
        );
        let colors = s.text_colors.as_ref().unwrap();
        assert_eq!(colors.enabled, Some(Color::rgba(10, 0, 0, 255)));
        assert_eq!(colors.highlighted, Some(Color::rgba(135, 123, 116, 255)));
        assert_eq!(colors.disabled, None);

        let bare = StaticDesc::read(require(&doc, "bare", 0).unwrap(), &defs());
        assert_eq!(bare.texture, None);
        assert!(!bare.text_complex());
        let text = bare.text.unwrap();
        assert_eq!(text.key, None);
        assert_eq!(text.color, Color::WHITE);
        assert_eq!(text.font, Some(FontChoice::Unknown("header".into())));
        assert!(require(&doc, "missing", 0).is_err());
    }

    #[test]
    fn lays_out_scroll_lists() {
        // Section 9.1: the title menu's region.
        let doc = parse(
            b"<w><buttons_region x=\"110\" y=\"0\" width=\"164\" height=\"280\" \
            always_show_scroll=\"false\"/>\
            <l height=\"100\" right_ident=\"5\" top_indent=\"2\" bottom_indent=\"3\" vert_interval=\"4\">\
            <text>a</text><text>b</text></l></w>",
        )
        .unwrap();
        let list = ScrollListDesc::read(require(&doc, "buttons_region", 0).unwrap(), &defs());
        assert!(!list.always_show_scroll);
        assert_eq!(list.scroll_profile, "default");
        let (pos, content) = list.layout(&[30.0; 6]);
        assert_eq!(
            pos,
            (0..6).map(|j| (0.0, 30.0 * j as f32)).collect::<Vec<_>>()
        );
        assert_eq!(content, 180.0);
        assert!(!list.shows_scroll_bar(content));
        assert!(list.shows_scroll_bar(300.0));

        let list = ScrollListDesc::read(require(&doc, "l", 0).unwrap(), &defs());
        assert!(list.always_show_scroll);
        assert_eq!(list.items.len(), 2);
        let (pos, content) = list.layout(&[10.0, 20.0]);
        assert_eq!(pos, [(5.0, 6.0), (5.0, 20.0)]);
        assert_eq!(content, 2.0 + 3.0 + 14.0 + 24.0);
        assert_eq!(list.clip(), Rect::new(0.0, 2.0, 0.0, 95.0));
    }
}
