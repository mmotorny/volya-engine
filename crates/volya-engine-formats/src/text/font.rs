// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Font definitions and glyph metric files (docs/formats/text-and-fonts.md,
//! section 2).

use std::io;

use crate::ltx::{Config, Section, value};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The fonts the UI creates (section 2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiFont {
    Graffiti19,
    Graffiti22,
    Graffiti32,
    Graffiti50,
    Arial14,
    Letterica16,
    Letterica18,
    Letterica25,
    Medium,
    Small,
    Di,
}

impl UiFont {
    pub const ALL: [UiFont; 11] = [
        Self::Graffiti19,
        Self::Graffiti22,
        Self::Graffiti32,
        Self::Graffiti50,
        Self::Arial14,
        Self::Letterica16,
        Self::Letterica18,
        Self::Letterica25,
        Self::Medium,
        Self::Small,
        Self::Di,
    ];

    /// The default font of a text (section 4.1).
    pub const DEFAULT: UiFont = Self::Letterica16;

    /// The name UI layout XML uses (`font=`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Graffiti19 => "graffiti19",
            Self::Graffiti22 => "graffiti22",
            Self::Graffiti32 => "graffiti32",
            Self::Graffiti50 => "graffiti50",
            Self::Arial14 => "arial_14",
            Self::Letterica16 => "letterica16",
            Self::Letterica18 => "letterica18",
            Self::Letterica25 => "letterica25",
            Self::Medium => "medium",
            Self::Small => "small",
            Self::Di => "di",
        }
    }

    /// The font named exactly `name` (case-sensitive).
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name() == name)
    }

    /// Its section of `config\fonts.ltx` (fixed, not language-dependent).
    pub fn section(self) -> &'static str {
        match self {
            Self::Graffiti19 => "ui_font_graffiti19_russian",
            Self::Graffiti22 => "ui_font_graffiti22_russian",
            Self::Graffiti32 => "ui_font_graff_32",
            Self::Graffiti50 => "ui_font_graff_50",
            Self::Arial14 => "ui_font_arial_14",
            Self::Letterica16 => "ui_font_letterica16_russian",
            Self::Letterica18 => "ui_font_letterica18_russian",
            Self::Letterica25 => "ui_font_letter_25",
            Self::Medium => "hud_font_medium",
            Self::Small => "stat_font",
            Self::Di => "hud_font_di",
        }
    }

    /// Bottom glyph edges drawn with red, green and blue halved.
    pub fn gradient(self) -> bool {
        self == Self::Di
    }

    /// `size` is a fraction of the screen height.
    pub fn screen_relative_size(self) -> bool {
        matches!(self, Self::Small | Self::Di)
    }
}

/// A section of `config\fonts.ltx` (section 2.2).
#[derive(Clone, Debug, PartialEq)]
pub struct FontDefinition {
    pub shader: Option<String>,
    pub texture: String,
    pub texture800: Option<String>,
    pub texture1600: Option<String>,
    /// Glyph height override: pixels, or × screen height for
    /// screen-relative fonts.
    pub size: Option<f32>,
    /// Advance (x) and line height (y) multipliers, default (1, 1).
    pub interval: (f32, f32),
}

impl FontDefinition {
    pub fn from_section(section: &Section) -> io::Result<Self> {
        let get = |k| section.get(k).flatten().map(str::to_owned);
        let texture = get("texture")
            .ok_or_else(|| invalid(format!("[{}]: font without texture", section.name())))?;
        let interval = section.get("interval").flatten().map_or((1.0, 1.0), |v| {
            let [x, y] = value::floats::<2>(v);
            (x, y)
        });
        Ok(Self {
            shader: get("shader"),
            texture,
            texture800: get("texture800"),
            texture1600: get("texture1600"),
            size: section.get("size").flatten().map(value::float),
            interval,
        })
    }

    /// The texture name for output height `height` (section 2.3): ≤ 600
    /// prefers `texture800`, 601–900 `texture`, ≥ 901 `texture1600`; an
    /// absent preferred key falls back to the next lower of `texture1600`,
    /// `texture`, `texture800`, and finally to `texture`.
    pub fn texture_for_height(&self, height: u32) -> &str {
        let order: &[Option<&String>] = match height {
            0..=600 => &[self.texture800.as_ref()],
            601..=900 => &[],
            _ => &[self.texture1600.as_ref()],
        };
        order
            .iter()
            .flatten()
            .next()
            .map_or(self.texture.as_str(), |t| t.as_str())
    }
}

/// Appends the language prefix (section 2.4) to texture name `texture`
/// unless the name contains `ui_font_hud_01`, `ui_font_hud_02` or
/// `ui_font_console_02`. `prefix` is the value of `[string_table]
/// font_prefix` (`None` or empty: no prefix).
pub fn with_prefix(texture: &str, prefix: Option<&str>) -> String {
    match prefix {
        Some(p)
            if !p.is_empty()
                && !["ui_font_hud_01", "ui_font_hud_02", "ui_font_console_02"]
                    .iter()
                    .any(|n| texture.contains(n)) =>
        {
            format!("{texture}{p}")
        }
        _ => texture.to_owned(),
    }
}

/// The metric file of final texture name `texture`, relative to
/// `textures\` (section 2.5): `<texture>.ini` after removing anything from
/// the last `.`.
pub fn metric_path(texture: &str) -> String {
    let stem = texture.rfind('.').map_or(texture, |dot| &texture[..dot]);
    format!("{stem}.ini")
}

/// One glyph's cell in the font texture, in texture pixels. Width 0 is an
/// invisible glyph with zero advance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Glyph {
    pub x: f32,
    pub y: f32,
    pub width: f32,
}

/// A metric file: the glyph height *h* and the 256 glyph cells (section
/// 2.6).
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphMetrics {
    pub height: f32,
    pub glyphs: [Glyph; 256],
}

impl GlyphMetrics {
    /// Reads the first present of `[mb_symbol_coords]`, `[symbol_coords]`,
    /// `[Char Widths]`, `[font_size]` (section 2.6; names case-insensitive).
    /// `[mb_symbol_coords]` (multi-byte text, unused by the edition) is
    /// rejected: this engine draws single-byte text only. Missing keys the
    /// spec calls required (`height`, all 256 codes of `[symbol_coords]`)
    /// are errors; the spec does not say whether `[Char Widths]` needs all
    /// 256 keys, and missing ones are errors too.
    pub fn parse(config: &Config) -> io::Result<Self> {
        let number = |s: &Section, key: &str| -> io::Result<f32> {
            s.get(key)
                .flatten()
                .map(value::float)
                .ok_or_else(|| invalid(format!("[{}]: missing {key}", s.name())))
        };
        let mut glyphs = [Glyph::default(); 256];
        if config.section("mb_symbol_coords").is_some() {
            Err(invalid(
                "[mb_symbol_coords] (multi-byte) fonts are not supported",
            ))
        } else if let Some(s) = config.section("symbol_coords") {
            let height = number(s, "height")?;
            for (code, glyph) in glyphs.iter_mut().enumerate() {
                let key = format!("{code:03}");
                let v = s
                    .get(&key)
                    .flatten()
                    .ok_or_else(|| invalid(format!("[symbol_coords]: missing {key}")))?;
                let [x1, y1, x2, _] = value::floats::<4>(v);
                *glyph = Glyph {
                    x: x1,
                    y: y1,
                    width: x2 - x1,
                };
            }
            Ok(Self { height, glyphs })
        } else if let Some(s) = config.section("char widths") {
            let height = number(s, "height")?;
            for (code, glyph) in glyphs.iter_mut().enumerate() {
                let i = code as f32;
                *glyph = Glyph {
                    x: (i % 16.0) * height,
                    y: (i / 16.0).floor() * height,
                    width: number(s, &code.to_string())?,
                };
            }
            Ok(Self { height, glyphs })
        } else if let Some(s) = config.section("font_size") {
            let height = number(s, "height")?;
            let width = number(s, "width")?;
            let cpl = number(s, "cpl")?;
            if cpl < 1.0 {
                return Err(invalid(format!("[font_size]: cpl {cpl}")));
            }
            for (code, glyph) in glyphs.iter_mut().enumerate() {
                let i = code as f32;
                *glyph = Glyph {
                    x: (i % cpl) * width,
                    y: (i / cpl).floor() * height,
                    width,
                };
            }
            Ok(Self { height, glyphs })
        } else {
            Err(invalid("font metric file without a known section"))
        }
    }
}

/// A font ready to draw: its final texture name (sample `textures\<name>`),
/// metrics and drawing parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub ui_font: UiFont,
    /// Final texture name *T* (relative to `textures\`, no extension).
    pub texture: String,
    pub metrics: GlyphMetrics,
    /// The drawn glyph height *f* in pixels: `size` (× screen height for
    /// screen-relative fonts) or the metric height.
    pub height: f32,
    pub interval: (f32, f32),
    pub gradient: bool,
}

impl Font {
    /// Builds `font` from its definition and metric file for output height
    /// `screen_height`. `texture` is the final texture name (see
    /// [`FontDefinition::texture_for_height`] and [`with_prefix`]).
    pub fn new(
        ui_font: UiFont,
        definition: &FontDefinition,
        texture: String,
        metrics: GlyphMetrics,
        screen_height: u32,
    ) -> Self {
        let height = match definition.size {
            Some(size) if ui_font.screen_relative_size() => size * screen_height as f32,
            Some(size) => size,
            None => metrics.height,
        };
        Self {
            ui_font,
            texture,
            metrics,
            height,
            interval: definition.interval,
            gradient: ui_font.gradient(),
        }
    }

    /// The glyph of byte `code`.
    pub fn glyph(&self, code: u8) -> Glyph {
        self.metrics.glyphs[usize::from(code)]
    }

    /// The pen advance of byte `code` in pixels: *w* · interval.*x*.
    pub fn advance(&self, code: u8) -> f32 {
        self.glyph(code).width * self.interval.0
    }

    /// The width *S* of a string in pixels: interval.*x* · Σ *w*
    /// (section 3.1).
    pub fn width(&self, text: &[u8]) -> f32 {
        self.interval.0 * text.iter().map(|&c| self.glyph(c).width).sum::<f32>()
    }

    /// The line height in pixels: *f* · interval.*y* (section 3.6).
    pub fn line_height(&self) -> f32 {
        self.height * self.interval.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ltx;

    #[test]
    fn names_fonts() {
        assert_eq!(UiFont::from_name("graffiti22"), Some(UiFont::Graffiti22));
        assert_eq!(UiFont::from_name("Graffiti22"), None);
        assert_eq!(UiFont::from_name("header"), None);
        assert_eq!(UiFont::from_name("di"), Some(UiFont::Di));
        assert_eq!(UiFont::Graffiti22.section(), "ui_font_graffiti22_russian");
        assert_eq!(UiFont::Small.section(), "stat_font");
        assert!(UiFont::Di.gradient() && UiFont::Di.screen_relative_size());
        assert!(UiFont::Small.screen_relative_size() && !UiFont::Small.gradient());
        for f in UiFont::ALL {
            assert_eq!(UiFont::from_name(f.name()), Some(f));
        }
    }

    fn definition(text: &[u8], section: &str) -> FontDefinition {
        FontDefinition::from_section(ltx::parse(text).unwrap().section(section).unwrap()).unwrap()
    }

    #[test]
    fn chooses_resolution_variants() {
        let d = definition(
            b"[f]\nshader = font\ntexture = ui\\ui_font_graff_22_1024\n\
            texture800 = ui\\ui_font_graff_22_800\ntexture1600 = ui\\ui_font_graff_22_1600\n",
            "f",
        );
        assert_eq!(d.shader.as_deref(), Some("font"));
        assert_eq!(d.interval, (1.0, 1.0));
        // Section 2.3 examples (by height only).
        for (h, t) in [
            (600, "ui\\ui_font_graff_22_800"),
            (768, "ui\\ui_font_graff_22_1024"),
            (720, "ui\\ui_font_graff_22_1024"),
            (900, "ui\\ui_font_graff_22_1024"),
            (901, "ui\\ui_font_graff_22_1600"),
            (1024, "ui\\ui_font_graff_22_1600"),
            (1080, "ui\\ui_font_graff_22_1600"),
        ] {
            assert_eq!(d.texture_for_height(h), t, "{h}");
        }
        let only = definition(b"[f]\ntexture = t\ninterval = 0.75,1\n", "f");
        assert_eq!(only.texture_for_height(480), "t");
        assert_eq!(only.texture_for_height(1200), "t");
        assert_eq!(only.interval, (0.75, 1.0));
        assert!(
            FontDefinition::from_section(
                ltx::parse(b"[f]\nshader = font\n")
                    .unwrap()
                    .section("f")
                    .unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn applies_prefix() {
        assert_eq!(
            with_prefix("ui\\ui_font_graff_22_1024", Some("_west")),
            "ui\\ui_font_graff_22_1024_west"
        );
        assert_eq!(
            with_prefix("ui\\ui_font_hud_01", Some("_west")),
            "ui\\ui_font_hud_01"
        );
        assert_eq!(
            with_prefix("ui\\ui_font_console_02", Some("_west")),
            "ui\\ui_font_console_02"
        );
        assert_eq!(
            with_prefix("ui\\ui_font_graff_22_1024", None),
            "ui\\ui_font_graff_22_1024"
        );
        assert_eq!(with_prefix("ui\\x", Some("")), "ui\\x");
        assert_eq!(
            metric_path("ui\\ui_font_graff_22_1024"),
            "ui\\ui_font_graff_22_1024.ini"
        );
        assert_eq!(metric_path("ui\\a.b"), "ui\\a.ini");
    }

    #[test]
    fn parses_metric_formats() {
        let mut coords = b"[symbol_coords]\nheight = 22\n".to_vec();
        for code in 0..256 {
            let line = match code {
                32 => "032 =    0,    0,   10,   22\n".to_owned(),
                205 => "205 =   12,  110,   22,  132\n".to_owned(),
                c => format!("{c:03} = 0, 0, 0, 0\n"),
            };
            coords.extend_from_slice(line.as_bytes());
        }
        let m = GlyphMetrics::parse(&ltx::parse(&coords).unwrap()).unwrap();
        assert_eq!(m.height, 22.0);
        assert_eq!(
            m.glyphs[32],
            Glyph {
                x: 0.0,
                y: 0.0,
                width: 10.0
            }
        );
        assert_eq!(
            m.glyphs[205],
            Glyph {
                x: 12.0,
                y: 110.0,
                width: 10.0
            }
        );
        coords.truncate(coords.len() - 20);
        assert!(GlyphMetrics::parse(&ltx::parse(&coords).unwrap()).is_err());

        let mut widths = b"[Char Widths]\nheight = 16\n".to_vec();
        for code in 0..256 {
            widths.extend_from_slice(format!("{code}={}\n", code % 7).as_bytes());
        }
        let m = GlyphMetrics::parse(&ltx::parse(&widths).unwrap()).unwrap();
        assert_eq!(
            m.glyphs[17],
            Glyph {
                x: 16.0,
                y: 16.0,
                width: 3.0
            }
        );
        assert_eq!(
            m.glyphs[255],
            Glyph {
                x: 240.0,
                y: 240.0,
                width: 3.0
            }
        );

        let grid = b"[font_size]\nheight = 8\nwidth = 8\ncpl = 16\n";
        let m = GlyphMetrics::parse(&ltx::parse(grid).unwrap()).unwrap();
        assert_eq!(
            m.glyphs[33],
            Glyph {
                x: 8.0,
                y: 16.0,
                width: 8.0
            }
        );

        assert!(
            GlyphMetrics::parse(&ltx::parse(b"[mb_symbol_coords]\nheight=1\n").unwrap()).is_err()
        );
        assert!(GlyphMetrics::parse(&ltx::parse(b"[other]\n").unwrap()).is_err());

        // Precedence when several sections are present: `[mb_symbol_coords]`,
        // `[symbol_coords]`, `[Char Widths]`, `[font_size]`.
        let both = [&widths[..], grid].concat();
        let m = GlyphMetrics::parse(&ltx::parse(&both).unwrap()).unwrap();
        assert_eq!((m.height, m.glyphs[17].width), (16.0, 3.0));
        let mut coords_full = b"[symbol_coords]\nheight = 5\n".to_vec();
        for code in 0..256 {
            coords_full.extend_from_slice(format!("{code:03} = 1, 2, 4, 7\n").as_bytes());
        }
        let three = [&widths[..], grid, &coords_full].concat();
        let m = GlyphMetrics::parse(&ltx::parse(&three).unwrap()).unwrap();
        assert_eq!((m.height, m.glyphs[17].width), (5.0, 3.0));
        assert_eq!(m.glyphs[17].x, 1.0);
        let mb = [&coords_full[..], b"[mb_symbol_coords]\nheight = 5\n"].concat();
        assert!(GlyphMetrics::parse(&ltx::parse(&mb).unwrap()).is_err());
    }

    #[test]
    fn sizes_fonts() {
        let m = GlyphMetrics::parse(
            &ltx::parse(b"[font_size]\nheight = 8\nwidth = 8\ncpl = 16\n").unwrap(),
        )
        .unwrap();
        let small = definition(
            b"[stat_font]\ntexture = ui\\ui_font_hud_01\ninterval = 0.75,1\n",
            "stat_font",
        );
        let f = Font::new(UiFont::Small, &small, small.texture.clone(), m.clone(), 768);
        assert_eq!((f.height, f.line_height()), (8.0, 8.0));
        assert_eq!(f.advance(b'a'), 6.0);
        assert_eq!(f.width(b"ab"), 12.0);
        let sized = definition(b"[f]\ntexture = t\nsize = 0.02\n", "f");
        let f = Font::new(UiFont::Di, &sized, "t".into(), m.clone(), 1000);
        assert!((f.height - 20.0).abs() < 1e-4 && f.gradient);
        let f = Font::new(UiFont::Medium, &sized, "t".into(), m, 1000);
        assert_eq!(f.height, 0.02);
    }
}
