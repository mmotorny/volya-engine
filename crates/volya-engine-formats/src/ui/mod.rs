// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The data of the game's 2D user interface (docs/formats/ui-layout.md): the
//! 1024 × 768 UI space and its screen mapping, the wide-screen `_16` file
//! rule, colours, element descriptions read from layout XML ([`layout`]),
//! picture geometry ([`picture`]), texture descriptions ([`texture_desc`]),
//! frame sequences ([`seq`]) and the title menu's layout ([`title_menu`]).
//! Finding files is the caller's job (`volya-engine-core::ui`).

pub mod layout;
pub mod picture;
pub mod seq;
pub mod texture_desc;
pub mod title_menu;

use std::collections::HashMap;

use crate::ltx::value;
use crate::xml::{Document, Element};

/// Width of the virtual UI space in units (section 3).
pub const UI_WIDTH: f32 = 1024.0;
/// Height of the virtual UI space in units (section 3).
pub const UI_HEIGHT: f32 = 768.0;

/// Output resolution in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// The UI-to-pixel scale factors (sx, sy) = (W / 1024, H / 768)
    /// (section 3): each axis independently, no letterboxing.
    pub fn scale(self) -> (f32, f32) {
        (self.width as f32 / UI_WIDTH, self.height as f32 / UI_HEIGHT)
    }

    /// Whether W / H, in single precision, is greater than 1.3433334: the
    /// test of the `_16` rule (section 1.1) and of the rotated-picture
    /// correction (section 5.4).
    pub fn is_wide(self) -> bool {
        self.width as f32 / self.height as f32 > 1.343_333_4
    }
}

/// The names to try, in order, for a file `name` of the `ui` directory at
/// `resolution` (section 1.1): on wide screens first the `_16` variant (the
/// last extension, from the last `.`, replaced by `_16.xml`, or `_16`
/// appended when there is no `.`), then the requested name (with `.xml`
/// appended when it has no `.`).
pub fn ui_file_names(name: &str, resolution: Resolution) -> Vec<String> {
    let normal = if name.contains('.') {
        name.to_owned()
    } else {
        format!("{name}.xml")
    };
    if !resolution.is_wide() {
        return vec![normal];
    }
    let variant = match name.rfind('.') {
        Some(dot) => format!("{}_16.xml", &name[..dot]),
        None => format!("{name}_16"),
    };
    vec![variant, normal]
}

/// The file stem of a still image (section 6.3): a trailing `.tga`, `.dds`,
/// `.bmp` or `.ogm` (case-insensitive) removed; `.dds` is then appended to
/// find the file.
pub fn still_image_stem(path: &str) -> &str {
    let lower = path.to_ascii_lowercase();
    for ext in [".tga", ".dds", ".bmp", ".ogm"] {
        if lower.ends_with(ext) {
            return &path[..path.len() - ext.len()];
        }
    }
    path
}

/// An 8-bit ARGB colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const WHITE: Color = Color::rgba(255, 255, 255, 255);
    /// Transparent black, the colour of an undefined colour name (section 2).
    pub const NONE: Color = Color::rgba(0, 0, 0, 0);

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { a, r, g, b }
    }

    /// Packed as `0xAARRGGBB`.
    pub const fn argb(self) -> u32 {
        (self.a as u32) << 24 | (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }
}

/// The named colours of `config\ui\color_defs.xml` (section 2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorDefs {
    colors: HashMap<String, Color>,
}

impl ColorDefs {
    /// Reads the root's `color` children: `name`, `r`, `g`, `b` (default 0)
    /// and `a` (default 255). The spec does not cover a repeated name; the
    /// later definition wins here.
    pub fn read(doc: &Document) -> Self {
        let mut colors = HashMap::new();
        if let Some(root) = doc.root() {
            for e in root.children_named("color") {
                if let Some(name) = e.attr("name") {
                    colors.insert(name.to_owned(), argb_attributes(e, 0));
                }
            }
        }
        Self { colors }
    }

    /// The colour named exactly `name`.
    pub fn get(&self, name: &str) -> Option<Color> {
        self.colors.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
}

/// An integer attribute read like C `atoi` (section 1.4), or `default`.
pub fn int_attr(e: &Element, name: &str, default: i64) -> i64 {
    e.attr(name).map_or(default, value::int)
}

/// A float attribute read like C `atof` (section 1.4), or `default`.
pub fn float_attr(e: &Element, name: &str, default: f32) -> f32 {
    e.attr(name).map_or(default, value::float)
}

/// A flag attribute: on when its integer value is non-zero (section 1.4).
pub fn flag_attr(e: &Element, name: &str, default: bool) -> bool {
    e.attr(name).map_or(default, |v| value::int(v) != 0)
}

/// `a`, `r`, `g`, `b` attributes, modulo 256; `a` defaults to 255 and the
/// others to `rgb_default`.
fn argb_attributes(e: &Element, rgb_default: i64) -> Color {
    let c = |name, default| int_attr(e, name, default) as u8;
    Color::rgba(
        c("r", rgb_default),
        c("g", rgb_default),
        c("b", rgb_default),
        c("a", 255),
    )
}

/// An element's colour (section 2): the colour its `color` attribute names
/// (transparent black when undefined), else its `a r g b` attributes with
/// `rgb_default` for missing `r`, `g`, `b` (255 for text, font and texture
/// colours, 0 for list text-state colours).
pub fn read_color(e: &Element, defs: &ColorDefs, rgb_default: u8) -> Color {
    match e.attr("color") {
        Some(name) => defs.get(name).unwrap_or(Color::NONE),
        None => argb_attributes(e, i64::from(rgb_default)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    #[test]
    fn maps_ui_space_to_screen() {
        // Section 3 test vectors.
        assert_eq!(Resolution::new(1024, 768).scale(), (1.0, 1.0));
        assert_eq!(Resolution::new(800, 600).scale(), (0.78125, 0.78125));
        let (sx, sy) = Resolution::new(1280, 1024).scale();
        assert_eq!(sx, 1.25);
        assert!((sy - 1.333_333).abs() < 1e-5);
        assert_eq!(Resolution::new(1920, 1080).scale(), (1.875, 1.40625));
    }

    #[test]
    fn chooses_wide_screen_variants() {
        // Section 1.1 test vectors.
        for (w, h) in [(1024, 768), (1280, 1024), (1600, 1200)] {
            assert!(!Resolution::new(w, h).is_wide(), "{w}x{h}");
        }
        for (w, h) in [(1280, 800), (1680, 1050), (1920, 1080), (1366, 768)] {
            assert!(Resolution::new(w, h).is_wide(), "{w}x{h}");
        }
        let wide = Resolution::new(1920, 1080);
        assert_eq!(
            ui_file_names("maingame.xml", wide),
            ["maingame_16.xml", "maingame.xml"]
        );
        assert_eq!(
            ui_file_names("ui_mainmenu", wide),
            ["ui_mainmenu_16", "ui_mainmenu.xml"]
        );
        assert_eq!(
            ui_file_names("ui_mainmenu", Resolution::new(1024, 768)),
            ["ui_mainmenu.xml"]
        );
        assert_eq!(ui_file_names("a.b.xml", wide), ["a.b_16.xml", "a.b.xml"]);
    }

    #[test]
    fn strips_still_image_extensions() {
        assert_eq!(
            still_image_stem("fx\\fx_wood_fire_1.tga"),
            "fx\\fx_wood_fire_1"
        );
        assert_eq!(still_image_stem("a.DDS"), "a");
        assert_eq!(still_image_stem("ui\\ui_noise_00"), "ui\\ui_noise_00");
        assert_eq!(still_image_stem("a.png"), "a.png");
    }

    #[test]
    fn reads_colors() {
        let defs = ColorDefs::read(
            &parse(
                b"<colors><color name=\"ui_clr_1\" r=\"240\" g=\"217\" b=\"182\"/>\
                <color name=\"half\" r=\"1\" a=\"128\"/></colors>",
            )
            .unwrap(),
        );
        assert_eq!(defs.get("ui_clr_1"), Some(Color::rgba(240, 217, 182, 255)));
        assert_eq!(defs.get("half"), Some(Color::rgba(1, 0, 0, 128)));
        let doc = parse(
            b"<w><a color=\"ui_clr_1\" r=\"1\"/><b color=\"nope\"/><c r=\"300\" a=\"-1\"/>\
            <d/></w>",
        )
        .unwrap();
        let root = doc.root().unwrap();
        let get = |n, d| read_color(root.child(n).unwrap(), &defs, d);
        assert_eq!(get("a", 255), Color::rgba(240, 217, 182, 255));
        assert_eq!(get("b", 255), Color::NONE);
        assert_eq!(get("c", 255), Color::rgba(44, 255, 255, 255));
        assert_eq!(get("d", 255), Color::WHITE);
        assert_eq!(get("d", 0), Color::rgba(0, 0, 0, 255));
        assert_eq!(Color::rgba(1, 2, 3, 4).argb(), 0x0401_0203);
    }

    #[test]
    fn reads_numbers_like_c() {
        let doc = parse(b"<e i=\"1.5\" f=\"false\" s=\"0.25px\" n=\"2\"/>").unwrap();
        let e = doc.root().unwrap();
        assert_eq!(int_attr(e, "i", 7), 1);
        assert_eq!(int_attr(e, "f", 7), 0);
        assert_eq!(int_attr(e, "missing", 7), 7);
        assert_eq!(float_attr(e, "s", 0.0), 0.25);
        assert!(flag_attr(e, "n", false));
        assert!(!flag_attr(e, "f", true));
        assert!(flag_attr(e, "missing", true));
    }
}
