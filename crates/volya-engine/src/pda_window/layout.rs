// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The PDA window's layout (docs/behavior/pda-window.md, cited by section):
//! `pda.xml` read once when the window is built (1), each element at its
//! absolute rectangle under the parent of 2.1, and the tab bar laid out
//! again from its captions' widths (3.2).

use std::io;

use crate::core::ui::load_ui_file;
use crate::core::vfs::Vfs;
use crate::data::GameData;
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::ui::layout::{Rect, StaticDesc, require};
use volya_engine_formats::ui::{Color, ColorDefs, Resolution};
use volya_engine_formats::xml::Document;

fn invalid(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

/// The window's layout file (1; it has no `_16` variant).
pub const LAYOUT_FILE: &str = "pda.xml";
/// The number of tabs (3.1).
pub const TABS: usize = 7;
/// The separator between two tab captions (3.2).
pub const SEPARATOR: &str = "//";
/// The separator's colour (3.2).
pub const SEPARATOR_COLOR: Color = Color::rgba(90, 90, 90, 255);
/// The space before a caption, after its mark, and after a caption (3.2).
const MARK_SPACE: f32 = 12.0;
const AFTER_CAPTION: f32 = 3.0;
/// A new-content mark's height (3.2: the mark point is centred on it).
const MARK_HEIGHT: f32 = 11.0;

/// A static at its absolute rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub desc: StaticDesc,
    pub rect: Rect,
}

/// One tab button after the tab bar's layout (3.2): its caption rectangle
/// and mark point in absolute units.
#[derive(Clone, Debug, PartialEq)]
pub struct TabButton {
    pub desc: StaticDesc,
    /// The caption's rectangle: the button moved and sized to its text.
    pub rect: Rect,
    /// Where the tab's new-content mark is drawn (top-left).
    pub mark: (f32, f32),
    /// The hint's string key (`hint`), if any.
    pub hint: Option<String>,
}

/// A separator `//` before every tab but the first (3.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Separator {
    pub rect: Rect,
    pub font: UiFont,
}

/// What the window draws (2.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub background: Placed,
    pub mb_line: Placed,
    pub timer_line: Placed,
    /// `timer_frame_line`'s `title`: the clock (2.3).
    pub timer_title: Placed,
    /// The tab bar's absolute rectangle.
    pub tab_bar: Rect,
    pub buttons: Vec<TabButton>,
    pub separators: Vec<Separator>,
    /// `updated_section_static`: a tab with new content (2.4).
    pub updated_mark: StaticDesc,
    /// `old_section_static`: a tab without (2.4).
    pub old_mark: StaticDesc,
}

impl Layout {
    /// Loads `pda.xml` and lays the tab bar out with `width`, a text's
    /// width in UI units in a font (the string width *S* divided by the
    /// UI's horizontal factor, 3.2), the captions' texts from the string
    /// table.
    pub fn load(
        tree: &dyn Vfs,
        data: &GameData,
        resolution: Resolution,
        width: &dyn Fn(UiFont, &[u8]) -> f32,
    ) -> io::Result<Self> {
        let doc = load_ui_file(tree, &data.aliases, LAYOUT_FILE, resolution)?;
        Self::read(&doc, &data.colors, &|key| data.text(key), width)
    }

    /// Reads the elements of 1 in their order, `text` translating a string
    /// key to its Windows-1251 text.
    pub fn read(
        doc: &Document,
        colors: &ColorDefs,
        text: &dyn Fn(&str) -> Vec<u8>,
        width: &dyn Fn(UiFont, &[u8]) -> f32,
    ) -> io::Result<Self> {
        let read = |name: &str| StaticDesc::read(require(doc, name, 0)?, colors);
        let at = |name: &str, parent: (f32, f32)| -> io::Result<Placed> {
            let desc = read(name)?;
            let rect = desc.window.absolute(parent.0, parent.1);
            Ok(Placed { desc, rect })
        };
        // 2.1: the window is the whole UI space; the rest is the
        // background's.
        let main = read("main")?.window.absolute(0.0, 0.0);
        let background = at("background_static", (main.x, main.y))?;
        let bg = (background.rect.x, background.rect.y);
        let mb_line = at("mbbackground_frame_line", bg)?;
        let timer_line = at("timer_frame_line", bg)?;
        let timer_title = at(
            "timer_frame_line:title",
            (timer_line.rect.x, timer_line.rect.y),
        )?;
        let tab = require(doc, "tab", 0)?;
        let tab_bar = StaticDesc::read(tab, colors)?.window.absolute(bg.0, bg.1);
        let buttons: Vec<StaticDesc> = tab
            .children_named("button")
            .map(|b| StaticDesc::read(b, colors))
            .collect::<io::Result<_>>()?;
        if buttons.len() != TABS {
            return Err(invalid(format!(
                "pda.xml: the tab bar has {} buttons, not {TABS}",
                buttons.len()
            )));
        }
        let hints: Vec<Option<String>> = tab
            .children_named("button")
            .map(|b| b.attr("hint").map(str::to_owned))
            .collect();
        let (buttons, separators) = lay_out(tab_bar, buttons, hints, text, width);
        Ok(Self {
            background,
            mb_line,
            timer_line,
            timer_title,
            tab_bar,
            buttons,
            separators,
            updated_mark: read("updated_section_static")?,
            old_mark: read("old_section_static")?,
        })
    }

    /// Every texture id the window draws and its string keys, for checks.
    pub fn references(&self) -> (Vec<String>, Vec<String>) {
        let mut textures: Vec<String> = [&self.background.desc, &self.updated_mark, &self.old_mark]
            .iter()
            .flat_map(|d| d.texture.iter().map(|t| t.name.clone()))
            .collect();
        for line in [&self.mb_line, &self.timer_line] {
            if let Some(t) = &line.desc.texture {
                textures.extend(["b", "back", "e"].map(|p| format!("{}_{p}", t.name)));
            }
        }
        let mut strings: Vec<String> = self
            .buttons
            .iter()
            .flat_map(|b| {
                b.desc
                    .text
                    .iter()
                    .flat_map(|t| t.key.clone())
                    .chain(b.hint.clone())
            })
            .collect();
        textures.sort();
        textures.dedup();
        strings.sort();
        strings.dedup();
        (textures, strings)
    }
}

/// The tab bar laid out again from left to right (3.2): the buttons'
/// `x` and `width` overridden, a separator before every button but the
/// first, each tab's mark point.
fn lay_out(
    bar: Rect,
    buttons: Vec<StaticDesc>,
    hints: Vec<Option<String>>,
    text: &dyn Fn(&str) -> Vec<u8>,
    width: &dyn Fn(UiFont, &[u8]) -> f32,
) -> (Vec<TabButton>, Vec<Separator>) {
    let first = buttons[0].window.rect();
    let (mut x, y) = (first.x, first.y);
    let mut out = Vec::with_capacity(buttons.len());
    let mut separators = Vec::new();
    for (i, (desc, hint)) in buttons.into_iter().zip(hints).enumerate() {
        let h = desc.window.height;
        let font = desc
            .text
            .as_ref()
            .and_then(|t| t.font)
            .unwrap_or(UiFont::DEFAULT);
        if i > 0 {
            let w = width(font, SEPARATOR.as_bytes());
            separators.push(Separator {
                rect: Rect::new(bar.x + x, bar.y + y, w, h),
                font,
            });
            x += w;
        }
        let mark = (bar.x + x, bar.y + y + ((h - MARK_HEIGHT) / 2.0).floor());
        x += MARK_SPACE;
        let caption = desc
            .text
            .as_ref()
            .and_then(|t| t.key.as_deref())
            .map(text)
            .unwrap_or_default();
        let w = width(font, &caption);
        out.push(TabButton {
            rect: Rect::new(bar.x + x, bar.y + y, w, h),
            mark,
            hint,
            desc,
        });
        x += w + AFTER_CAPTION;
    }
    (out, separators)
}
