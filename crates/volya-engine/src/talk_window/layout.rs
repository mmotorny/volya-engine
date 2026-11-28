// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The talk window's layout (docs/behavior/talk-window.md, cited by
//! section): `talk.xml` (`talk_16.xml` on wide screens) read once (1), its
//! elements in reading order at their absolute rectangles (2), the frame
//! lines (4), the character panels of `trade_character.xml` (5), the two
//! lists (6, 7) and the entry templates, read from the kept document each
//! time an entry is added (6.3, 7).

use std::io;

use crate::core::ui::load_ui_file;
use crate::core::vfs::Vfs;
use crate::data::GameData;
pub use crate::pda_window::layout::Placed;
use volya_engine_formats::ui::layout::{Rect, ScrollListDesc, StaticDesc, WindowDesc, require};
use volya_engine_formats::ui::options::ScrollBarProfile;
use volya_engine_formats::ui::{ColorDefs, Resolution, float_attr, int_attr};
use volya_engine_formats::xml::{Document, Element};

/// The window's layout file (1).
pub const LAYOUT_FILE: &str = "talk.xml";
/// The character panels' layout file (1).
pub const PANEL_FILE: &str = "trade_character.xml";
/// The template of the player character's phrases (6.1).
pub const ACTOR_ANSWER: &str = "actor_answer_item";
/// The template of other speakers' phrases (6.1).
pub const OTHER_ANSWER: &str = "other_answer_item";
/// The template of an iconed line that names none (6.2).
pub const ICONED_ANSWER: &str = "iconed_answer_item";
/// The question list's entry template (7).
pub const QUESTION_ITEM: &str = "question_item";
/// The window's rectangle (2).
pub const WINDOW: Rect = Rect::new(0.0, 0.0, 1024.0, 768.0);
/// `min_height` and `bottom_footer` when a template gives none (6.3).
const MIN_HEIGHT: f32 = 15.0;
const BOTTOM_FOOTER: f32 = 0.0;

fn invalid(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

/// A frame line (4; pda-controls.md 3): its rectangle, the `texture`
/// child's value as the base and its colour, `vertical`, and its title.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameLine {
    pub desc: StaticDesc,
    pub rect: Rect,
    pub vertical: bool,
    pub title: Option<Placed>,
}

impl FrameLine {
    fn read(e: &Element, colors: &ColorDefs, parent: Rect) -> io::Result<Self> {
        let desc = StaticDesc::read(e, colors)?;
        let rect = desc.window.absolute(parent.x, parent.y);
        let title = e
            .child("title")
            .map(|t| -> io::Result<Placed> {
                let desc = StaticDesc::read(t, colors)?;
                let rect = desc.window.absolute(rect.x, rect.y);
                Ok(Placed { desc, rect })
            })
            .transpose()?;
        Ok(Self {
            vertical: int_attr(e, "vertical", 0) != 0,
            desc,
            rect,
            title,
        })
    }
}

/// A scroll view (6, 7; ui-layout.md 9.1) at its absolute rectangle, with
/// its scroll bar's profile (options-screens.md 4.8).
#[derive(Clone, Debug, PartialEq)]
pub struct List {
    pub desc: ScrollListDesc,
    pub rect: Rect,
    pub profile: ScrollBarProfile,
}

impl List {
    fn read(e: &Element, colors: &ColorDefs, parent: Rect, scroll: &Document) -> io::Result<Self> {
        let desc = ScrollListDesc::read(e, colors)?;
        let rect = desc.window.absolute(parent.x, parent.y);
        let profile = ScrollBarProfile::read(scroll, &desc.scroll_profile)?;
        Ok(Self {
            desc,
            rect,
            profile,
        })
    }
}

/// A character panel (5): a window at the picture element's top-left of
/// its width and height, and the elements of `trade_character.xml` it
/// reads, each only if the file has it.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelLayout {
    pub rect: Rect,
    pub icon: Option<Placed>,
    pub name: Option<Placed>,
    pub rank: Option<Placed>,
    pub rank_caption: Option<Placed>,
    pub community: Option<Placed>,
    pub community_caption: Option<Placed>,
    pub reputation: Option<Placed>,
    pub reputation_caption: Option<Placed>,
    pub relation: Option<Placed>,
    pub relation_caption: Option<Placed>,
    pub biography: Option<List>,
}

impl PanelLayout {
    /// The panel of picture element `picture` (absolute `at`), from `doc`
    /// (`trade_character.xml`).
    pub fn read(
        doc: &Document,
        colors: &ColorDefs,
        scroll: &Document,
        picture: &StaticDesc,
        at: Rect,
    ) -> io::Result<Self> {
        let rect = Rect::new(at.x, at.y, picture.window.width, picture.window.height);
        let at = |name: &str| -> io::Result<Option<Placed>> {
            doc.find(name, 0)
                .map(|e| {
                    let desc = StaticDesc::read(e, colors)?;
                    let rect = desc.window.absolute(rect.x, rect.y);
                    Ok(Placed { desc, rect })
                })
                .transpose()
        };
        Ok(Self {
            icon: at("icon_static")?,
            name: at("name_static")?,
            rank: at("rank_static")?,
            rank_caption: at("rank_caption")?,
            community: at("community_static")?,
            community_caption: at("community_caption")?,
            reputation: at("reputation_static")?,
            reputation_caption: at("reputation_caption")?,
            relation: at("relation_static")?,
            relation_caption: at("relation_caption")?,
            biography: doc
                .find("biography_list", 0)
                .map(|e| List::read(e, colors, rect, scroll))
                .transpose()?,
            rect,
        })
    }

    /// The captions, for checks.
    fn captions(&self) -> impl Iterator<Item = &Placed> {
        [
            &self.rank_caption,
            &self.community_caption,
            &self.reputation_caption,
            &self.relation_caption,
        ]
        .into_iter()
        .flatten()
    }
}

/// What the window reads (2), in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub top_background: Placed,
    pub bottom_background: Placed,
    pub left_icon: Placed,
    pub left_panel: PanelLayout,
    pub right_icon: Placed,
    pub right_panel: PanelLayout,
    /// Frame line 0, the answer log's, and frame line 1, the question
    /// list's.
    pub frame_lines: [FrameLine; 2],
    /// `answers_list`, attached to frame line 0.
    pub answers: List,
    /// `questions_list`, attached to frame line 1.
    pub questions: List,
    /// `button`, the trade button (8).
    pub button: Placed,
    /// The loaded document, kept for the entry templates (1).
    pub doc: Document,
}

impl Layout {
    /// Loads `talk.xml` and `trade_character.xml`, each with the `_16` rule
    /// of ui-layout.md 1.1 (1). A missing file is an error.
    pub fn load(tree: &dyn Vfs, data: &GameData, resolution: Resolution) -> io::Result<Self> {
        let doc = load_ui_file(tree, &data.aliases, LAYOUT_FILE, resolution)?;
        let panel = load_ui_file(tree, &data.aliases, PANEL_FILE, resolution)?;
        let scroll = load_ui_file(tree, &data.aliases, "scroll_bar.xml", resolution)?;
        Self::read(doc, &panel, &scroll, &data.colors)
    }

    /// Reads the elements of 2 in their order; `scroll` is
    /// `scroll_bar.xml`.
    pub fn read(
        doc: Document,
        panel: &Document,
        scroll: &Document,
        colors: &ColorDefs,
    ) -> io::Result<Self> {
        let at = |name: &str| -> io::Result<Placed> {
            let desc = StaticDesc::read(require(&doc, name, 0)?, colors)?;
            let rect = desc.window.absolute(WINDOW.x, WINDOW.y);
            Ok(Placed { desc, rect })
        };
        let top_background = at("top_background")?;
        let bottom_background = at("bottom_background")?;
        let left_icon = at("left_character_icon")?;
        let left_panel = PanelLayout::read(panel, colors, scroll, &left_icon.desc, left_icon.rect)?;
        let right_icon = at("right_character_icon")?;
        let right_panel =
            PanelLayout::read(panel, colors, scroll, &right_icon.desc, right_icon.rect)?;
        let line = |i| FrameLine::read(require(&doc, "frame_line_window", i)?, colors, WINDOW);
        let frame_lines = [line(0)?, line(1)?];
        let list =
            |name: &str, parent: Rect| List::read(require(&doc, name, 0)?, colors, parent, scroll);
        let answers = list("answers_list", frame_lines[0].rect)?;
        let questions = list("questions_list", frame_lines[1].rect)?;
        let button = at("button")?;
        Ok(Self {
            top_background,
            bottom_background,
            left_icon,
            left_panel,
            right_icon,
            right_panel,
            frame_lines,
            answers,
            questions,
            button,
            doc,
        })
    }

    /// The entry template `name` (6.3, 7), read from the kept document.
    pub fn template(&self, name: &str, colors: &ColorDefs) -> io::Result<Template> {
        Template::read(&self.doc, name, colors)
    }

    /// Every texture id the window draws and its string keys, for checks.
    pub fn references(&self) -> (Vec<String>, Vec<String>) {
        let mut textures: Vec<String> = [&self.left_icon, &self.right_icon]
            .iter()
            .flat_map(|p| p.desc.texture.iter().map(|t| t.name.clone()))
            .collect();
        for line in &self.frame_lines {
            if let Some(t) = &line.desc.texture {
                textures.extend(["b", "back", "e"].map(|p| format!("{}_{p}", t.name)));
            }
        }
        textures.extend(crate::dialog::button_textures(&self.button.desc));
        let mut strings: Vec<String> = (self.button.desc.text.iter())
            .flat_map(|t| t.key.clone())
            .collect();
        for c in self
            .left_panel
            .captions()
            .chain(self.right_panel.captions())
        {
            strings.extend(c.desc.text.iter().flat_map(|t| t.key.clone()));
        }
        textures.sort();
        textures.dedup();
        strings.sort();
        strings.dedup();
        (textures, strings)
    }
}

/// An entry template (6.3, 7): the window, `min_height`, `bottom_footer`,
/// and its `content_text`, `name_caption` and `msg_icon` children, each
/// only if the template has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub window: WindowDesc,
    pub min_height: f32,
    pub bottom_footer: f32,
    pub content_text: StaticDesc,
    pub name_caption: Option<StaticDesc>,
    pub msg_icon: Option<StaticDesc>,
}

impl Template {
    fn read(doc: &Document, name: &str, colors: &ColorDefs) -> io::Result<Self> {
        let e = require(doc, name, 0)?;
        let child = |c: &str| e.child(c).map(|c| StaticDesc::read(c, colors)).transpose();
        Ok(Self {
            window: WindowDesc::read(e, colors)?,
            min_height: float_attr(e, "min_height", MIN_HEIGHT),
            bottom_footer: float_attr(e, "bottom_footer", BOTTOM_FOOTER),
            content_text: child("content_text")?
                .ok_or_else(|| invalid(format!("talk.xml: {name} has no content_text")))?,
            name_caption: child("name_caption")?,
            msg_icon: child("msg_icon")?,
        })
    }
}
