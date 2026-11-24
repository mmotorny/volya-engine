// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Text to positioned glyph quads (docs/formats/text-and-fonts.md, sections
//! 3 and 4): one string drawn with a font ([`draw_line`]) and the text of a
//! UI element in simple or complex mode ([`layout_text`]). Pure functions;
//! the renderer samples [`TextLayout::texture`] for each quad.

use super::font::{Font, UiFont};
use crate::ltx::value;
use crate::ui::layout::{HAlign, Rect, StaticDesc, VAlign};
use crate::ui::{Color, ColorDefs};

/// Strings this long or longer are not drawn (section 3.1).
pub const MAX_LINE_BYTES: usize = 512;

/// One visible glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphQuad {
    /// The byte drawn.
    pub code: u8,
    /// Top-left and bottom-right corners in screen pixels: *w* × *f*.
    pub min: (f32, f32),
    pub max: (f32, f32),
    /// The glyph cell in texture pixels: (*gx*, *gy*, *w*, metric *h*).
    pub cell: Rect,
    /// Colour of the two top corners.
    pub top_color: Color,
    /// Colour of the two bottom corners (RGB halved for gradient fonts).
    pub bottom_color: Color,
}

impl GlyphQuad {
    /// Texture coordinates at `min` and `max` for a font texture of
    /// `width` × `height` texels (section 3.3): half-texel offset on the
    /// start, plus the cell size.
    pub fn uv(&self, width: f32, height: f32) -> ((f32, f32), (f32, f32)) {
        let u0 = (self.cell.x + 0.5) / width;
        let v0 = (self.cell.y + 0.5) / height;
        (
            (u0, v0),
            (u0 + self.cell.width / width, v0 + self.cell.height / height),
        )
    }

    /// The quad in UI units for scale (sx, sy).
    pub fn ui_rect(&self, (sx, sy): (f32, f32)) -> Rect {
        Rect::new(
            self.min.0 / sx,
            self.min.1 / sy,
            (self.max.0 - self.min.0) / sx,
            (self.max.1 - self.min.1) / sy,
        )
    }
}

/// The bottom colour of a gradient glyph: red, green, blue halved.
fn halved(c: Color) -> Color {
    Color::rgba(c.r / 2, c.g / 2, c.b / 2, c.a)
}

/// Draws `text` on one line at pen (`x`, `y`) in screen pixels (section 3):
/// no interpretation of any byte, ending at the first zero byte; nothing for
/// strings of [`MAX_LINE_BYTES`] or more. The pen *x* is floored and, for
/// right and centre alignment, moved left by floor(*S*) or floor(*S* / 2);
/// the pen *y* (floored) is the top of the glyph cells.
pub fn draw_line(
    font: &Font,
    text: &[u8],
    (x, y): (f32, f32),
    color: Color,
    align: HAlign,
) -> Vec<GlyphQuad> {
    let text = text
        .iter()
        .position(|&b| b == 0)
        .map_or(text, |nul| &text[..nul]);
    if text.len() >= MAX_LINE_BYTES {
        return Vec::new();
    }
    let s = font.width(text);
    let x0 = x.floor()
        - match align {
            HAlign::Left => 0.0,
            HAlign::Right => s.floor(),
            HAlign::Centre => (s / 2.0).floor(),
        };
    let y0 = y.floor();
    let bottom_color = if font.gradient { halved(color) } else { color };
    let mut quads = Vec::new();
    let mut advance = 0.0;
    for &code in text {
        let g = font.glyph(code);
        if g.width != 0.0 {
            let left = x0 + font.interval.0 * advance;
            quads.push(GlyphQuad {
                code,
                min: (left, y0),
                max: (left + g.width, y0 + font.height),
                cell: Rect::new(g.x, g.y, g.width, font.metrics.height),
                top_color: color,
                bottom_color,
            });
        }
        advance += g.width;
    }
    quads
}

/// Where and how a UI element draws its text (section 4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextBox {
    /// The element's absolute rectangle in UI units.
    pub rect: Rect,
    /// The text offset (*tx*, *ty*) in UI units.
    pub offset: (f32, f32),
    pub align: HAlign,
    pub vert_align: VAlign,
    /// Complex mode: colour codes, `\n` and wrapping (section 4.3).
    pub complex: bool,
    /// The element's text colour.
    pub color: Color,
    /// Extra line gap *g* in UI units for complex mode (0 by default).
    pub line_gap: f32,
}

impl TextBox {
    /// The text box of static `desc` with absolute rectangle `rect`, and its
    /// font: the text's font, else the default `letterica16`; defaults left,
    /// top, white.
    pub fn of_static(desc: &StaticDesc, rect: Rect) -> (Self, UiFont) {
        let text = desc.text.as_ref();
        (
            Self {
                rect,
                offset: text.map_or((0.0, 0.0), |t| t.offset),
                align: text.and_then(|t| t.align).unwrap_or_default(),
                vert_align: text.and_then(|t| t.vert_align).unwrap_or_default(),
                complex: desc.text_complex(),
                color: text.map_or(Color::WHITE, |t| t.color),
                line_gap: 0.0,
            },
            text.and_then(|t| t.font).unwrap_or(UiFont::DEFAULT),
        )
    }
}

/// Glyph quads of a text with the font texture to sample.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    /// The font's final texture name (`textures\<texture>.dds`).
    pub texture: String,
    pub quads: Vec<GlyphQuad>,
    /// Number of lines laid out (0 for an empty text).
    pub lines: usize,
}

/// A segment of complex-mode text (section 4.3): a run of bytes of one
/// colour containing neither a colour code nor `\n`.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub text: Vec<u8>,
    pub color: Color,
    /// The segment was ended by `\n` (always false in wrapped lines).
    pub ends_line: bool,
}

/// Cuts complex-mode text into segments (section 4.3): at every colour code
/// (`%c[default]`, `%c[<name>]` with an exact name of `color_defs.xml`, or
/// `%c[A,R,G,B]`; each sets the colour of the text after it, keeping
/// `base`'s alpha), then each coloured piece at every two-byte `\n`, which
/// is removed and ends the line. Empty segments are kept (an empty segment
/// ended by `\n` is an empty line). Choices where the spec is silent: a
/// `%c[` without a closing `]` is literal text; fewer than four numbers is
/// the spec's "missing comma" error.
pub fn split_complex(text: &[u8], base: Color, defs: &ColorDefs) -> Result<Vec<Segment>, String> {
    let mut segments = Vec::new();
    let mut color = base;
    let mut rest = text;
    loop {
        let code = find(rest, b"%c[").and_then(|start| {
            find(&rest[start + 3..], b"]").map(|len| (start, &rest[start + 3..start + 3 + len]))
        });
        let chunk = match code {
            Some((start, _)) => &rest[..start],
            None => rest,
        };
        let parts = split(chunk, b"\\n");
        let last = parts.len() - 1;
        for (i, part) in parts.into_iter().enumerate() {
            segments.push(Segment {
                text: part.to_vec(),
                color,
                ends_line: i < last,
            });
        }
        let Some((start, content)) = code else {
            break;
        };
        color = code_color(content, base, defs)?;
        rest = &rest[start + 3 + content.len() + 1..];
    }
    Ok(segments)
}

fn code_color(content: &[u8], base: Color, defs: &ColorDefs) -> Result<Color, String> {
    let name = crate::cp1251::decode(content);
    if name == "default" {
        return Ok(base);
    }
    let c = match defs.get(&name) {
        Some(c) => c,
        None => {
            let parts: Vec<&str> = name.split(',').collect();
            if parts.len() < 4 {
                return Err(format!("colour code %c[{name}]: missing comma"));
            }
            let n = |i: usize| value::int(parts[i]) as u8;
            Color::rgba(n(1), n(2), n(3), n(0))
        }
    };
    Ok(Color::rgba(c.r, c.g, c.b, base.a))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn split<'a>(text: &'a [u8], separator: &[u8]) -> Vec<&'a [u8]> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(i) = find(rest, separator) {
        parts.push(&rest[..i]);
        rest = &rest[i + separator.len()..];
    }
    parts.push(rest);
    parts
}

/// White space for break points (section 4.3): the ASCII white-space
/// characters space, tab, LF, VT, FF, CR. Whether bytes 0x80–0xFF count was
/// not established; they are not white space here.
fn is_wrap_space(b: u8) -> bool {
    matches!(b, 0x09..=0x0D | 0x20)
}

/// Wraps segments into lines of `width` UI units for horizontal scale `sx`
/// (section 4.3). With *L* the running width of the line (reset on every new
/// line, carried across segments of a line) and *r* the width of `o`:
///
/// - a character is accepted when *L* + its width + *r* ≤ `width`; its width
///   is added to *L* unless it is the last character of its segment;
/// - break points are white-space characters of the current segment at a
///   position other than 0, reached since the segment started or since the
///   last break in it (the tested character counts as reached);
/// - a rejected character that is not the last of its segment ends the line
///   after the most recent break point (filling resumes, re-testing, with the
///   character after it) or, without one, after itself (it stays on the
///   line; filling resumes with the next character);
/// - a rejected last character stays on the line with the rest of its
///   segment, and the line ends;
/// - a segment ended by `\n` ends the line, unless its last character was
///   rejected, which has ended it already (the spec lists the two cases
///   separately; we do not end a second, empty line).
///
/// A final empty line is kept only after a trailing `\n` (or when there is
/// no other line); the spec does not say whether it counts.
pub fn wrap(font: &Font, segments: &[Segment], width: f32, sx: f32) -> Vec<Vec<Segment>> {
    let units = |c: u8| font.advance(c) / sx;
    let r = units(b'o');
    let mut lines: Vec<Vec<Segment>> = Vec::new();
    let mut current: Vec<Segment> = Vec::new();
    let mut l = 0.0;
    let mut ended_by_newline = false;
    let piece = |text: &[u8], color| Segment {
        text: text.to_vec(),
        color,
        ends_line: false,
    };
    for seg in segments {
        let text = &seg.text;
        let n = text.len();
        let mut start = 0;
        let mut k = 0;
        let mut break_point: Option<usize> = None;
        let mut rejected_last = false;
        while k < n {
            let c = text[k];
            if k != 0 && is_wrap_space(c) {
                break_point = Some(k);
            }
            let last = k == n - 1;
            if l + units(c) + r <= width {
                if !last {
                    l += units(c);
                }
                k += 1;
                continue;
            }
            let end = if last {
                rejected_last = true;
                n - 1
            } else {
                break_point.unwrap_or(k)
            };
            current.push(piece(&text[start..=end], seg.color));
            lines.push(std::mem::take(&mut current));
            l = 0.0;
            break_point = None;
            start = end + 1;
            k = end + 1;
        }
        if start < n {
            current.push(piece(&text[start..], seg.color));
        }
        if n > 0 || seg.ends_line {
            ended_by_newline = seg.ends_line;
        }
        if seg.ends_line && !rejected_last {
            lines.push(std::mem::take(&mut current));
            l = 0.0;
        }
    }
    if !current.is_empty() || ended_by_newline || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Lays out the text of a UI element (section 4): `text` is the string
/// already looked up in the string table (Windows-1251 bytes), `scale` the
/// UI-to-pixel factors (sx, sy). An empty text draws nothing.
///
/// Simple mode draws the text as one string, colour codes and `\n`
/// literally; complex mode cuts it into segments, wraps them to the element
/// width and stacks the lines with pitch *t* + *g*, each segment drawn after
/// the previous one with the element's alignment applied to the segment
/// itself.
pub fn layout_text(
    font: &Font,
    text: &[u8],
    b: &TextBox,
    (sx, sy): (f32, f32),
    defs: &ColorDefs,
) -> Result<TextLayout, String> {
    let mut layout = TextLayout {
        texture: font.texture.clone(),
        quads: Vec::new(),
        lines: 0,
    };
    if text.is_empty() {
        return Ok(layout);
    }
    let t = font.line_height() / sy;
    let ax = match b.align {
        HAlign::Left => 0.0,
        HAlign::Right => b.rect.width,
        HAlign::Centre => b.rect.width / 2.0,
    };
    let x = b.rect.x + b.offset.0 + ax;
    let ay = |block: f32| match b.vert_align {
        VAlign::Top => 0.0,
        VAlign::Centre => (b.rect.height - block) / 2.0,
        VAlign::Bottom => b.rect.height - block,
    };
    if !b.complex {
        let y = b.rect.y + b.offset.1 + ay(t);
        layout.quads = draw_line(font, text, (sx * x, sy * y), b.color, b.align);
        layout.lines = 1;
        return Ok(layout);
    }
    let lines = wrap(font, &split_complex(text, b.color, defs)?, b.rect.width, sx);
    let n = lines.len() as f32;
    let pitch = t + b.line_gap;
    let top = b.rect.y + b.offset.1 + ay(n * pitch - b.line_gap);
    for (i, line) in lines.iter().enumerate() {
        let y = top + i as f32 * pitch;
        let mut seg_x = x;
        for seg in line {
            layout.quads.extend(draw_line(
                font,
                &seg.text,
                (sx * seg_x, sy * y),
                seg.color,
                b.align,
            ));
            seg_x += font.width(&seg.text) / sx;
        }
    }
    layout.lines = lines.len();
    Ok(layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cp1251;
    use crate::text::font::{FontDefinition, Glyph, GlyphMetrics};

    /// `ui_font_graff_22_1024` as far as section 2.7 gives it, other glyphs
    /// 8 wide in a dummy cell.
    fn graffiti22() -> Font {
        let mut glyphs = [Glyph {
            x: 0.0,
            y: 0.0,
            width: 8.0,
        }; 256];
        for glyph in &mut glyphs[..32] {
            glyph.width = 0.0;
        }
        let mut set = |c: char, x, y, w| {
            glyphs[usize::from(cp1251::encode_char(c).unwrap())] = Glyph { x, y, width: w };
        };
        set(' ', 0.0, 0.0, 10.0);
        set('.', 111.0, 0.0, 5.0);
        set('1', 134.0, 0.0, 7.0);
        set('Н', 12.0, 110.0, 10.0);
        set('а', 217.0, 110.0, 8.0);
        set('я', 246.0, 132.0, 9.0);
        set('о', 0.0, 0.0, 8.0);
        set('в', 0.0, 0.0, 8.0);
        set('и', 0.0, 0.0, 9.0);
        set('г', 0.0, 0.0, 7.0);
        set('р', 0.0, 0.0, 8.0);
        set('o', 0.0, 0.0, 5.0);
        let definition = FontDefinition {
            shader: Some("font".into()),
            texture: "ui\\ui_font_graff_22_1024".into(),
            texture800: None,
            texture1600: None,
            size: None,
            interval: (1.0, 1.0),
        };
        let metrics = GlyphMetrics {
            height: 22.0,
            glyphs,
        };
        Font::new(
            UiFont::Graffiti22,
            &definition,
            definition.texture.clone(),
            metrics,
            768,
        )
    }

    fn bytes(s: &str) -> Vec<u8> {
        cp1251::encode(s).unwrap()
    }

    const BLACK: Color = Color::rgba(0, 0, 0, 255);

    #[test]
    fn draws_one_line() {
        let font = graffiti22();
        let text = bytes("Новая игра");
        assert_eq!(font.width(&text), 85.0);
        // Section 5 derivation: pen (782, 423), centred.
        let quads = draw_line(&font, &text, (782.0, 423.0), BLACK, HAlign::Centre);
        assert_eq!(quads.len(), 10, "the space is visible too");
        assert_eq!(quads[0].min, (740.0, 423.0));
        assert_eq!(quads[0].max, (750.0, 445.0));
        assert_eq!(quads.last().unwrap().max, (825.0, 445.0));
        let (uv0, uv1) = quads[0].uv(256.0, 256.0);
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(
            close(uv0.0, 0.048_828) && close(uv1.0, 0.087_891),
            "{uv0:?} {uv1:?}"
        );
        assert!(
            close(uv0.1, 0.431_641) && close(uv1.1, 0.517_578),
            "{uv0:?} {uv1:?}"
        );
        assert_eq!(quads[0].bottom_color, BLACK);

        let right = draw_line(&font, &text, (100.7, 5.9), BLACK, HAlign::Right);
        assert_eq!(right[0].min, (15.0, 5.0));
        let zero = [b'a', 0, b'b'];
        assert_eq!(
            draw_line(&font, &zero, (0.0, 0.0), BLACK, HAlign::Left).len(),
            1
        );
        let control = [1u8, b'a'];
        let q = draw_line(&font, &control, (0.0, 0.0), BLACK, HAlign::Left);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].min.0, 0.0);
        assert!(draw_line(&font, &[b'a'; 512], (0.0, 0.0), BLACK, HAlign::Left).is_empty());
        assert_eq!(
            draw_line(&font, &[b'a'; 511], (0.0, 0.0), BLACK, HAlign::Left).len(),
            511
        );
        assert_eq!(
            draw_line(&font, b"\\n", (0.0, 0.0), BLACK, HAlign::Left).len(),
            2
        );
    }

    #[test]
    fn gradient_and_interval() {
        let mut font = graffiti22();
        font.gradient = true;
        font.interval = (0.5, 2.0);
        let q = draw_line(
            &font,
            b"ab",
            (0.0, 0.0),
            Color::rgba(255, 101, 2, 200),
            HAlign::Left,
        );
        assert_eq!(q[0].bottom_color, Color::rgba(127, 50, 1, 200));
        assert_eq!(q[1].min.0, 4.0);
        assert_eq!(q[1].max.0, 12.0, "the glyph keeps its width");
        assert_eq!(font.line_height(), 44.0);
    }

    fn menu_row(i: usize) -> TextBox {
        TextBox {
            rect: Rect::new(700.0, 419.0 + 30.0 * i as f32, 164.0, 30.0),
            offset: (0.0, 0.0),
            align: HAlign::Centre,
            vert_align: VAlign::Centre,
            complex: false,
            color: BLACK,
            line_gap: 0.0,
        }
    }

    #[test]
    fn simple_mode_centres_menu_captions() {
        let font = graffiti22();
        let l = layout_text(
            &font,
            &bytes("Новая игра"),
            &menu_row(0),
            (1.0, 1.0),
            &ColorDefs::default(),
        )
        .unwrap();
        assert_eq!(l.texture, "ui\\ui_font_graff_22_1024");
        assert_eq!(l.lines, 1);
        assert_eq!(l.quads[0].min, (740.0, 423.0));
        assert_eq!(l.quads.last().unwrap().max, (825.0, 445.0));
        let r = l.quads[0].ui_rect((1.0, 1.0));
        assert_eq!(r, Rect::new(740.0, 423.0, 10.0, 22.0));
        let empty =
            layout_text(&font, b"", &menu_row(0), (1.0, 1.0), &ColorDefs::default()).unwrap();
        assert!(empty.quads.is_empty() && empty.lines == 0);
        // Simple mode draws colour codes literally.
        let literal = layout_text(
            &font,
            b"%c[default]",
            &menu_row(0),
            (1.0, 1.0),
            &ColorDefs::default(),
        )
        .unwrap();
        assert_eq!(literal.quads.len(), 11);
    }

    #[test]
    fn splits_colour_codes_and_breaks() {
        let defs = ColorDefs::read(
            &crate::xml::parse(b"<c><color name=\"UI_orange\" r=\"238\" g=\"153\" b=\"26\"/></c>")
                .unwrap(),
        );
        let base = Color::rgba(1, 2, 3, 128);
        let segments = split_complex(
            b"a%c[UI_orange]b\\nc%c[255,192,10,20]d%c[default]e%c[nope",
            base,
            &defs,
        )
        .unwrap();
        let orange = Color::rgba(238, 153, 26, 128);
        let seg = |t: &str, color, ends_line| Segment {
            text: t.as_bytes().to_vec(),
            color,
            ends_line,
        };
        assert_eq!(
            segments,
            [
                seg("a", base, false),
                seg("b", orange, true),
                seg("c", orange, false),
                seg("d", Color::rgba(192, 10, 20, 128), false),
                seg("e%c[nope", base, false),
            ]
        );
        assert!(split_complex(b"%c[1,2,3]x", base, &defs).is_err());
        assert!(split_complex(b"%c[green]x", base, &defs).is_err());
    }

    /// Wraps `pieces` (text, ended by `\n`) to `width` at scale `sx`; the
    /// lines' texts with `|` between segments.
    fn wrapped(pieces: &[(&[u8], bool)], width: f32, sx: f32) -> Vec<String> {
        let segments: Vec<Segment> = pieces
            .iter()
            .map(|(t, ends_line)| Segment {
                text: t.to_vec(),
                color: BLACK,
                ends_line: *ends_line,
            })
            .collect();
        wrap(&graffiti22(), &segments, width, sx)
            .iter()
            .map(|line| {
                line.iter()
                    .map(|s| cp1251::decode(&s.text))
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect()
    }

    // Section 4.3 wrapping. Widths: `a` 8, space 10, control characters 0,
    // `o` (the reserve r) 5.

    #[test]
    fn wraps_after_the_most_recent_break_point() {
        // `aaa ` reaches 34; the next `a` needs 34 + 8 + 5 > 40. The space
        // (position 3) is the break point; filling resumes with the `a`
        // after it, tested again.
        assert_eq!(wrapped(&[(b"aaa aaa", false)], 40.0, 1.0), ["aaa ", "aaa"]);
        // The tested character counts as reached: a rejected space is its own
        // break point; break points are forgotten after each break.
        assert_eq!(
            wrapped(&[(b"aa aa aa", false)], 30.0, 1.0),
            ["aa ", "aa ", "aa"]
        );
        // Scale: widths in units are pixels / sx.
        assert_eq!(wrapped(&[(b"aaa aaa", false)], 20.0, 2.0), ["aaa ", "aaa"]);
    }

    #[test]
    fn wraps_after_the_rejected_character_without_break_point() {
        // 24 + 8 + 5 > 30: the fourth `a` is rejected and stays on the line.
        assert_eq!(wrapped(&[(b"aaaaaa", false)], 30.0, 1.0), ["aaaa", "aa"]);
        // White space at position 0 of the segment is never a break point.
        assert_eq!(wrapped(&[(b" aaaaa", false)], 40.0, 1.0), [" aaaa", "a"]);
    }

    #[test]
    fn break_points_only_in_the_current_segment() {
        // The space of `aa ` is not a break point for the second segment.
        // The space, last of its segment, adds no width, so L = 16 when the
        // second segment starts: 16 + 8 + 5, 24 + 8 + 5 fit, 32 + 8 + 5 not.
        assert_eq!(
            wrapped(&[(b"aa ", false), (b"aaaa", false)], 40.0, 1.0),
            ["aa |aaa", "a"]
        );
    }

    #[test]
    fn last_character_of_a_segment() {
        // Its width is not added: `aa` + `aa` fits 30 (8 + 8 + 5, then
        // 8 + 8 + 5 and 16 + 8 + 5), which counting it would not.
        assert_eq!(
            wrapped(&[(b"aa", false), (b"aa", false)], 30.0, 1.0),
            ["aa|aa"]
        );
        // A rejected last character stays with the rest of its segment and
        // ends the line; the break point (the space) is not used.
        assert_eq!(
            wrapped(&[(b"aa a", false), (b"b", false)], 35.0, 1.0),
            ["aa a", "b"]
        );
        // Ended by `\n` too: no second, empty line.
        assert_eq!(
            wrapped(&[(b"aaaa", true), (b"bb", false)], 30.0, 1.0),
            ["aaaa", "bb"]
        );
    }

    #[test]
    fn line_breaks_end_lines() {
        assert_eq!(
            wrapped(&[(b"ab", true), (b"", true), (b"cd", false)], 100.0, 1.0),
            ["ab", "", "cd"]
        );
        assert_eq!(
            wrapped(&[(b"ab", true), (b"", false)], 100.0, 1.0),
            ["ab", ""]
        );
        assert_eq!(
            wrapped(&[(b"ab", false), (b"cd", false)], 100.0, 1.0),
            ["ab|cd"]
        );
        assert_eq!(wrapped(&[], 40.0, 1.0), [""]);
    }

    #[test]
    fn white_space_is_ascii_only() {
        for space in [b'\t', b'\n', 0x0B, 0x0C, b'\r', b' '] {
            let text = [b'a', b'a', b'a', space, b'a', b'a', b'a'];
            let lines = wrapped(&[(&text, false)], 30.0, 1.0);
            assert_eq!(lines.len(), 2, "{space:#04x}");
            assert!(lines[0].ends_with(char::from(space)), "{space:#04x}");
        }
        // The no-break space 0xA0 (8 wide here) is not a break point.
        assert_eq!(
            wrapped(&[(b"aa\xa0aaa", false)], 30.0, 1.0),
            ["aa\u{a0}a", "aa"]
        );
    }

    #[test]
    fn complex_mode_stacks_lines() {
        let font = graffiti22();
        let b = TextBox {
            rect: Rect::new(10.0, 20.0, 100.0, 100.0),
            offset: (1.0, 2.0),
            align: HAlign::Left,
            vert_align: VAlign::Bottom,
            complex: true,
            color: BLACK,
            line_gap: 3.0,
        };
        let l = layout_text(&font, b"ab\\ncd", &b, (1.0, 1.0), &ColorDefs::default()).unwrap();
        assert_eq!(l.lines, 2);
        // Block 2 · (22 + 3) − 3 = 47 → top 20 + 2 + 53 = 75; second line 100.
        let tops: Vec<f32> = l.quads.iter().map(|q| q.min.1).collect();
        assert_eq!(tops, [75.0, 75.0, 100.0, 100.0]);
        assert_eq!(l.quads[1].min.0, 19.0);
        // Segments are placed one after another.
        let l = layout_text(
            &font,
            b"ab%c[default]cd",
            &b,
            (1.0, 1.0),
            &ColorDefs::default(),
        )
        .unwrap();
        let xs: Vec<f32> = l.quads.iter().map(|q| q.min.0).collect();
        assert_eq!(xs, [11.0, 19.0, 27.0, 35.0]);
    }
}
