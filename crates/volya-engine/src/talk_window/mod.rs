// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The talk window (docs/behavior/talk-window.md, cited by section): its
//! layout ([`layout`]; 1, 2, 10), the backgrounds and character pictures
//! (3), the vertical frame lines with their titles (4), the character
//! panels with their fill and refresh (5), the answer log with its phrases
//! and iconed lines and its scrolling to the end (6), the question list (7),
//! the trade button (8) and the partners with the special mode (9).
//!
//! **What waits for other systems**: building the window with the level's
//! in-game interface (1) and opening, closing and every input of it,
//! the question list's and the trade button's looks under the cursor and
//! the lists' wheel and dragging included, are DG-6's (#904): until then
//! the window is reachable only from tests, its captions drawn in their
//! normal look `e`. Which phrases and choices a talk offers are DG-3's and
//! DG-4's; the lines scripts add DG-7's; the panels' values (names, icons,
//! ranks, communities, relations, health) the characters scope's (#279)
//! and their texts PD-14's; what shows the news records `pda-scope`'s.

pub mod layout;
#[cfg(test)]
pub(crate) mod tests;

use std::io;

use serde::Serialize;

use crate::core::config::value;
use crate::data::{GameData, encode};
use crate::draw::{Ctx, picture_desc, pixels};
use crate::options::widgets::ScrollBar;
use crate::scene::Scene;
use layout::{
    ACTOR_ANSWER, ICONED_ANSWER, Layout, List, OTHER_ANSWER, PanelLayout, Placed, QUESTION_ITEM,
    Template,
};
use volya_engine_core::text::Fonts;
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::text::layout::{TextBox, split_complex, wrap};
use volya_engine_formats::ui::layout::{HAlign, Rect, VAlign};
use volya_engine_formats::ui::{Color, Resolution};

/// A panel's picture colour while its character's value is 0 or less (5).
pub const TINT: Color = Color::rgba(255, 160, 160, 255);
/// A panel is refreshed on every frame whose number is a multiple of this
/// (5).
pub const REFRESH_FRAMES: u64 = 100;
/// The object-section key of the special mode (9).
pub const SPECIAL_MODE_KEY: &str = "need_osoznanie_mode";
/// Between the speaker's name and the phrase in a news record (6.1).
pub const NEWS_SEPARATOR: &str = " >> ";

/// A text's height in units (6.3, 7): with font `font`, wrapped to `width`
/// units in complex mode (`true`), *n* · *t* for *n* lines; one line (*t*)
/// otherwise.
pub type Measure = Box<dyn Fn(UiFont, &[u8], f32, bool) -> f32>;

/// Whether the object section `section` gives the special mode (9): the
/// boolean key [`SPECIAL_MODE_KEY`], absent off.
pub fn has_special_mode(data: &GameData, section: &str) -> bool {
    (data.system.get(section, SPECIAL_MODE_KEY))
        .flatten()
        .is_some_and(value::bool)
}

/// The colour of `relation_static` for relation kind `kind` (5).
pub fn relation_color(kind: i32) -> Color {
    match kind {
        0 => Color::rgba(0, 255, 0, 255),
        1 => Color::rgba(192, 192, 192, 255),
        _ => Color::rgba(255, 0, 0, 255),
    }
}

/// A picture: a texture (id or path) and its rectangle in texture pixels.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Icon {
    pub texture: String,
    pub rect: Option<[f32; 4]>,
}

/// A character as a panel is filled with it (5); the values are the
/// characters scope's (#279).
#[derive(Clone, Debug, PartialEq)]
pub struct Character {
    pub id: u16,
    /// Its name, shown as it is.
    pub name: String,
    /// The string keys of its rank, community and reputation names.
    pub rank: String,
    pub community: String,
    pub reputation: String,
    /// Its icon texture.
    pub icon: Option<Icon>,
}

/// What a panel's refresh reads of a character that still exists (5).
#[derive(Clone, Debug, PartialEq)]
pub struct Standing {
    /// Its relation's kind to the player character and the kind's name's
    /// string key.
    pub relation: i32,
    pub relation_name: String,
    /// The value whose being 0 or less tints the picture (believed its
    /// health).
    pub health: f32,
}

/// A record of the player character's news (6.1 item 4).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NewsRecord {
    pub text: String,
    /// The speaker's panel's `icon_static` picture.
    pub icon: Option<Icon>,
}

/// A character panel's state (5).
#[derive(Clone, Debug, Default, PartialEq)]
struct Panel {
    character: Option<u16>,
    /// Filled and not yet refreshed: the next update refreshes it.
    fresh: bool,
    name: Option<String>,
    rank: Option<String>,
    community: Option<String>,
    reputation: Option<String>,
    icon: Option<Icon>,
    /// The picture's texture colour once tinted.
    icon_color: Option<Color>,
    /// `relation_caption` and `relation_static` shown.
    relation_hidden: bool,
    relation: Option<String>,
    relation_color: Option<Color>,
}

impl Panel {
    /// A fill (5): the name, the translated rank, community and reputation
    /// names, the icon. The picture's colour is kept (whether it returns to
    /// white is unestablished).
    fn fill(&mut self, data: &GameData, c: &Character) {
        let text = |key: &str| Some(data.strings.get(key).to_owned());
        self.character = Some(c.id);
        self.fresh = true;
        self.name = Some(c.name.clone());
        self.rank = text(&c.rank);
        self.community = text(&c.community);
        self.reputation = text(&c.reputation);
        self.icon = c.icon.clone();
    }

    /// A refresh (5): the character looked up first; gone, the panel
    /// forgets it.
    fn refresh(
        &mut self,
        data: &GameData,
        player: Option<u16>,
        lookup: &dyn Fn(u16) -> Option<Standing>,
    ) {
        let Some(id) = self.character else { return };
        let Some(s) = lookup(id) else {
            self.character = None;
            return;
        };
        self.relation_hidden = Some(id) == player;
        if !self.relation_hidden {
            self.relation = Some(data.strings.get(&s.relation_name).to_owned());
            self.relation_color = Some(relation_color(s.relation));
        }
        if s.health <= 0.0 {
            self.icon_color = Some(TINT);
        }
    }

    /// `icon_static`'s picture: the filled icon, else the file's.
    fn picture(&self, l: &PanelLayout) -> Option<Icon> {
        self.icon.clone().or_else(|| {
            let t = l.icon.as_ref()?.desc.texture.as_ref()?;
            Some(Icon {
                texture: t.name.clone(),
                rect: t.rect.map(|r| [r.x, r.y, r.width, r.height]),
            })
        })
    }
}

/// An answer log entry (6.3).
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    template_name: String,
    template: Template,
    /// `content_text`'s height and the entry's.
    content_height: f32,
    height: f32,
    name: String,
    text: String,
    icon: Option<Icon>,
}

/// A question list entry (7).
#[derive(Clone, Debug, PartialEq)]
struct Question {
    template: Template,
    caption_height: f32,
    height: f32,
    text: String,
}

/// A list's scroll state: the value model of options-screens.md 4.8.
fn bar(list: &List, heights: &[f32]) -> ScrollBar {
    let (_, content) = list.desc.layout(heights);
    ScrollBar::new(content, list.rect.height, list.profile.width)
}

/// The talk window.
pub struct TalkWindow {
    layout: Layout,
    measure: Measure,
    shown: bool,
    /// The player character of the last talk start.
    player: Option<u16>,
    player_name: String,
    /// Frame line 0's and 1's titles (4).
    titles: [String; 2],
    /// The left panel (the player character) and the right (the partner).
    panels: [Panel; 2],
    answers: Vec<Entry>,
    answers_bar: ScrollBar,
    questions: Vec<Question>,
    questions_bar: ScrollBar,
    /// The partner has the special mode (9).
    special_mode: bool,
    button_shown: bool,
    news: Vec<NewsRecord>,
}

impl TalkWindow {
    /// The window of a level, its texts measured in `fonts`, the fonts of
    /// `resolution` (1).
    pub fn load(
        tree: &dyn crate::core::vfs::Vfs,
        data: &GameData,
        resolution: Resolution,
        fonts: &Fonts,
    ) -> io::Result<Self> {
        let layout = Layout::load(tree, data, resolution)?;
        let (sx, sy) = resolution.scale();
        let (fonts, colors) = (fonts.clone(), data.colors.clone());
        let measure: Measure = Box::new(move |font, text, width, complex| {
            let f = fonts.get(font);
            let t = f.line_height() / sy;
            if !complex {
                return t;
            }
            // A colour code in error fails the text's drawing
            // (text-and-fonts.md 4.3); measured, it counts one line.
            let lines = split_complex(text, Color::WHITE, &colors)
                .map_or(1, |segments| wrap(f, &segments, width, sx).len());
            lines as f32 * t
        });
        Ok(Self::new(layout, measure))
    }

    /// The window of `layout`, texts measured by `measure`.
    pub fn new(layout: Layout, measure: Measure) -> Self {
        let answers_bar = bar(&layout.answers, &[]);
        let questions_bar = bar(&layout.questions, &[]);
        Self {
            layout,
            measure,
            shown: false,
            player: None,
            player_name: String::new(),
            titles: Default::default(),
            panels: Default::default(),
            answers: Vec::new(),
            answers_bar,
            questions: Vec::new(),
            questions_bar,
            special_mode: false,
            button_shown: true,
            news: Vec::new(),
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Shows or hides the window (when is DG-6's).
    pub fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
    }

    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// The talk start between `player`, the player character, and
    /// `partner`: both lists emptied and scrolled to their beginning (6,
    /// 7), the titles (4), the panels filled (5), the elements shown or
    /// hidden by the partner's special mode (8, 9).
    pub fn start(
        &mut self,
        data: &GameData,
        player: &Character,
        partner: &Character,
        special_mode: bool,
    ) {
        self.answers.clear();
        self.answers_bar = bar(&self.layout.answers, &[]);
        self.questions.clear();
        self.questions_bar = bar(&self.layout.questions, &[]);
        self.titles = [partner.name.clone(), player.name.clone()];
        self.player = Some(player.id);
        self.player_name = player.name.clone();
        self.panels[0].fill(data, player);
        self.panels[1].fill(data, partner);
        self.special_mode = special_mode;
        self.button_shown = !special_mode;
    }

    /// A phrase said by `speaker` (6.1): the template by the speaker's
    /// name, the entry appended with the translation of `text_key`, the log
    /// scrolled to its end, and a news record.
    pub fn add_phrase(&mut self, data: &GameData, speaker: &str, text_key: &str) -> io::Result<()> {
        let actor = encode(speaker) == encode(&self.player_name);
        let name = if actor { ACTOR_ANSWER } else { OTHER_ANSWER };
        let text = data.strings.get(text_key).to_owned();
        self.append(data, name, speaker.to_owned(), text.clone(), None)?;
        // The panel of the template's speaker; the record's text holds the
        // phrase as the entry shows it (our reading of "phrase text").
        let panel = usize::from(!actor);
        let icon = self.panels[panel].picture(self.layout_panel(panel));
        self.news.push(NewsRecord {
            text: format!("{speaker}{NEWS_SEPARATOR}{text}"),
            icon,
        });
        Ok(())
    }

    /// An iconed line (6.2), added only while the window is shown: from
    /// `template` (`iconed_answer_item` when none), its name empty, `text`
    /// as given, `msg_icon` showing `icon`. No news record.
    pub fn add_iconed_line(
        &mut self,
        data: &GameData,
        text: &str,
        icon: Icon,
        template: Option<&str>,
    ) -> io::Result<()> {
        if !self.shown {
            return Ok(());
        }
        let name = template.unwrap_or(ICONED_ANSWER);
        self.append(data, name, String::new(), text.to_owned(), Some(icon))
    }

    /// The choices refilled (7): the list emptied and scrolled to its
    /// beginning, one entry per choice in order, its caption the
    /// translation of the choice's text key. Adding does not scroll.
    pub fn set_choices(&mut self, data: &GameData, keys: &[&str]) -> io::Result<()> {
        self.questions.clear();
        self.questions_bar = bar(&self.layout.questions, &[]);
        for key in keys {
            let template = self.layout.template(QUESTION_ITEM, &data.colors)?;
            let text = data.strings.get(key).to_owned();
            let ct = &template.content_text;
            let font = font_of(&ct.text);
            let caption_height =
                (self.measure)(font, &encode(&text), ct.window.width, ct.text_complex());
            let height = (ct.window.y + caption_height).max(template.min_height);
            self.questions.push(Question {
                template,
                caption_height,
                height,
                text,
            });
        }
        // At its beginning still: the content grown, the position kept.
        self.questions_bar = bar(&self.layout.questions, &self.question_heights());
        Ok(())
    }

    /// The panels' per-frame update (5): a panel showing a character is
    /// refreshed at its first update after a fill, then on every frame
    /// whose number `frame` is a multiple of [`REFRESH_FRAMES`]; `lookup`
    /// finds a character.
    pub fn update(
        &mut self,
        data: &GameData,
        frame: u64,
        lookup: &dyn Fn(u16) -> Option<Standing>,
    ) {
        for p in &mut self.panels {
            if p.character.is_some() && (p.fresh || frame.is_multiple_of(REFRESH_FRAMES)) {
                p.fresh = false;
                p.refresh(data, self.player, lookup);
            }
        }
    }

    /// The news records made since the last call (6.1 item 4).
    pub fn take_news(&mut self) -> Vec<NewsRecord> {
        std::mem::take(&mut self.news)
    }

    fn layout_panel(&self, i: usize) -> &PanelLayout {
        if i == 0 {
            &self.layout.left_panel
        } else {
            &self.layout.right_panel
        }
    }

    fn answer_heights(&self) -> Vec<f32> {
        self.answers.iter().map(|e| e.height).collect()
    }

    fn question_heights(&self) -> Vec<f32> {
        self.questions.iter().map(|q| q.height).collect()
    }

    /// An entry from template `name` (6.3), appended, the log scrolled to
    /// its end (6).
    fn append(
        &mut self,
        data: &GameData,
        name: &str,
        speaker: String,
        text: String,
        icon: Option<Icon>,
    ) -> io::Result<()> {
        let template = self.layout.template(name, &data.colors)?;
        let ct = &template.content_text;
        let font = font_of(&ct.text);
        let content_height =
            (self.measure)(font, &encode(&text), ct.window.width, ct.text_complex());
        let height =
            template.bottom_footer + (ct.window.y + content_height).max(template.min_height);
        self.answers.push(Entry {
            template_name: name.to_owned(),
            template,
            content_height,
            height,
            name: speaker,
            text,
            icon,
        });
        self.answers_bar = bar(&self.layout.answers, &self.answer_heights());
        let end = self.answers_bar.max - self.answers_bar.page() + 1;
        self.answers_bar.set(end);
        Ok(())
    }

    /// Draws the shown window: its elements in the order of 2, each frame
    /// line's list after its band and title.
    pub fn draw(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        if !self.shown {
            return Ok(());
        }
        let l = &self.layout;
        draw_static(ctx, s, "talk top_background", &l.top_background, None, None)?;
        draw_static(
            ctx,
            s,
            "talk bottom_background",
            &l.bottom_background,
            None,
            None,
        )?;
        if !self.special_mode {
            let icons = [&l.left_icon, &l.right_icon];
            for (i, side) in ["left", "right"].iter().enumerate() {
                let layer = format!("talk {side}_character_icon");
                draw_static(ctx, s, &layer, icons[i], None, None)?;
                self.draw_panel(ctx, s, i, &format!("talk {side} panel"))?;
            }
            self.draw_frame_line(ctx, s, 0)?;
            draw_answers(ctx, s, &l.answers, &self.answers, &self.answers_bar)?;
        }
        self.draw_frame_line(ctx, s, 1)?;
        draw_questions(ctx, s, &l.questions, &self.questions, &self.questions_bar)?;
        if self.button_shown {
            let b = &l.button;
            crate::dialog::draw_button(ctx, s, "talk button", &b.desc, b.rect, "e")?;
        }
        Ok(())
    }

    /// Frame line `i` (4): its band, then its title.
    fn draw_frame_line(&self, ctx: &mut Ctx, s: &mut Scene, i: usize) -> io::Result<()> {
        let line = &self.layout.frame_lines[i];
        let layer = format!("talk frame line {i}");
        if let Some(t) = &line.desc.texture {
            let at = (line.rect.x, line.rect.y);
            if line.vertical {
                ctx.frame_line_vertical(s, &layer, &t.name, at, line.rect.height, t.color)?;
            } else {
                ctx.frame_line_colored(s, &layer, &t.name, at, line.rect.width, t.color)?;
            }
        }
        if let Some(title) = &line.title {
            let layer = format!("talk frame line {i} title");
            draw_static(ctx, s, &layer, title, Some(&self.titles[i]), None)?;
        }
        Ok(())
    }

    /// Panel `i` (5): its elements in reading order, the relation line
    /// while shown.
    fn draw_panel(&self, ctx: &mut Ctx, s: &mut Scene, i: usize, layer: &str) -> io::Result<()> {
        let (l, p) = (self.layout_panel(i), &self.panels[i]);
        if let Some(icon) = &l.icon {
            let start = s.sprites.len();
            match &p.icon {
                Some(c) => {
                    let rect = c.rect.map(|[x, y, w, h]| Rect::new(x, y, w, h));
                    let color = p.icon_color.unwrap_or(Color::WHITE);
                    let mut desc = picture_desc(&c.texture, icon.rect, rect, color);
                    desc.stretch = true;
                    ctx.picture(
                        s,
                        &format!("{layer} icon"),
                        &desc,
                        icon.rect,
                        0.0,
                        0,
                        false,
                        None,
                    )?;
                }
                None => {
                    let c = p.icon_color;
                    ctx.picture(
                        s,
                        &format!("{layer} icon"),
                        &icon.desc,
                        icon.rect,
                        0.0,
                        0,
                        false,
                        c,
                    )?;
                }
            }
            // `icon_static` clips its picture (ui-layout.md 4.3).
            s.clip_from(start, pixels(icon.rect, ctx.scale()));
        }
        let fields = [
            ("name", &l.name, &p.name, None),
            ("rank", &l.rank, &p.rank, None),
            ("rank_caption", &l.rank_caption, &None, None),
            ("community", &l.community, &p.community, None),
            ("community_caption", &l.community_caption, &None, None),
            ("reputation", &l.reputation, &p.reputation, None),
            ("reputation_caption", &l.reputation_caption, &None, None),
            ("relation", &l.relation, &p.relation, p.relation_color),
            ("relation_caption", &l.relation_caption, &None, None),
        ];
        for (name, placed, text, color) in fields {
            let Some(placed) = placed else { continue };
            if name.starts_with("relation") && p.relation_hidden {
                continue;
            }
            let layer = format!("{layer} {name}");
            draw_static(ctx, s, &layer, placed, text.as_deref(), color)?;
        }
        Ok(())
    }

    /// What tests observe.
    pub fn state(&self) -> TalkState {
        let l = &self.layout;
        let (answers_at, answers_content) = l.answers.desc.layout(&self.answer_heights());
        let (questions_at, questions_content) = l.questions.desc.layout(&self.question_heights());
        TalkState {
            shown: self.shown,
            special_mode: self.special_mode,
            button_shown: self.button_shown,
            titles: self.titles.clone(),
            panels: self.panels.iter().map(PanelState::of).collect(),
            answers: (self.answers.iter().zip(answers_at))
                .map(|(e, (x, y))| EntryState {
                    template: e.template_name.clone(),
                    at: [l.answers.rect.x + x, l.answers.rect.y + y],
                    height: e.height,
                    name: e.name.clone(),
                    text: e.text.clone(),
                })
                .collect(),
            answers_scroll: self.answers_bar.pos,
            answers_bar: l.answers.desc.shows_scroll_bar(answers_content),
            questions: (self.questions.iter().zip(questions_at))
                .map(|(q, (x, y))| EntryState {
                    template: QUESTION_ITEM.to_owned(),
                    at: [l.questions.rect.x + x, l.questions.rect.y + y],
                    height: q.height,
                    name: String::new(),
                    text: q.text.clone(),
                })
                .collect(),
            questions_scroll: self.questions_bar.pos,
            questions_bar: l.questions.desc.shows_scroll_bar(questions_content),
        }
    }
}

fn font_of(text: &Option<volya_engine_formats::ui::layout::TextDesc>) -> UiFont {
    text.as_ref()
        .and_then(|t| t.font)
        .unwrap_or(UiFont::DEFAULT)
}

/// A static's picture and its text: `text` (raw) in place of its key's
/// translation when given, in `color` when given.
fn draw_static(
    ctx: &mut Ctx,
    s: &mut Scene,
    layer: &str,
    p: &Placed,
    text: Option<&str>,
    color: Option<Color>,
) -> io::Result<()> {
    ctx.picture(s, layer, &p.desc, p.rect, 0.0, 0, false, None)?;
    let bytes = match text {
        Some(t) => encode(t),
        None => match p.desc.text.as_ref().and_then(|t| t.key.as_deref()) {
            Some(key) => ctx.data.text(key),
            None => return Ok(()),
        },
    };
    if bytes.is_empty() {
        return Ok(());
    }
    let (mut b, font) = TextBox::of_static(&p.desc, p.rect);
    if let Some(c) = color {
        b.color = c;
    }
    ctx.text(s, &format!("{layer} text"), &bytes, &b, font)
}

/// A list's view (ui-layout.md 9.1), absolute: what its entries are
/// clipped to.
fn view(list: &List) -> Rect {
    let c = list.desc.clip();
    Rect::new(list.rect.x + c.x, list.rect.y + c.y, c.width, c.height)
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// A list's scroll bar while its entries are taller than the view (6, 7).
fn draw_bar(
    ctx: &mut Ctx,
    s: &mut Scene,
    layer: &str,
    list: &List,
    content: f32,
    bar: &ScrollBar,
) -> io::Result<()> {
    if !list.desc.shows_scroll_bar(content) {
        return Ok(());
    }
    let (r, w) = (list.rect, list.profile.width);
    let rect = Rect::new(r.x + r.width - w, r.y, w, r.height);
    ctx.scroll_bar(s, layer, &list.profile, bar, rect)
}

/// The answer log (6): its entries shifted up by the scroll position and
/// clipped to the view, then its scroll bar.
fn draw_answers(
    ctx: &mut Ctx,
    s: &mut Scene,
    list: &List,
    entries: &[Entry],
    bar: &ScrollBar,
) -> io::Result<()> {
    let heights: Vec<f32> = entries.iter().map(|e| e.height).collect();
    let (at, content) = list.desc.layout(&heights);
    let clip = view(list);
    let start = s.sprites.len();
    for (i, (e, (x, y))) in entries.iter().zip(at).enumerate() {
        let (left, top) = (list.rect.x + x, list.rect.y + y - bar.pos as f32);
        let t = &e.template;
        if !intersects(Rect::new(left, top, t.window.width, e.height), clip) {
            continue;
        }
        let layer = format!("talk answer {i}");
        let ct = &t.content_text;
        let mut rect = ct.window.absolute(left, top);
        rect.height = e.content_height;
        let (b, font) = TextBox::of_static(ct, rect);
        if !e.text.is_empty() {
            ctx.text(
                s,
                &format!("{layer} content_text"),
                &encode(&e.text),
                &b,
                font,
            )?;
        }
        if let Some(nc) = &t.name_caption {
            let p = Placed {
                desc: nc.clone(),
                rect: nc.window.absolute(left, top),
            };
            draw_static(
                ctx,
                s,
                &format!("{layer} name_caption"),
                &p,
                Some(&e.name),
                None,
            )?;
        }
        if let (Some(mi), Some(icon)) = (&t.msg_icon, &e.icon) {
            let r = mi.window.absolute(left, top);
            let texture_rect = icon.rect.map(|[x, y, w, h]| Rect::new(x, y, w, h));
            let mut desc = picture_desc(&icon.texture, r, texture_rect, Color::WHITE);
            desc.stretch = true;
            ctx.picture(
                s,
                &format!("{layer} msg_icon"),
                &desc,
                r,
                0.0,
                0,
                false,
                None,
            )?;
        }
    }
    s.clip_from(start, pixels(clip, ctx.scale()));
    draw_bar(ctx, s, "talk answers bar", list, content, bar)
}

/// The question list (7): each entry's caption as a button's
/// (pda-controls.md 4: aligned by its `text` child horizontally, centred
/// vertically in its own height), clipped to the view, then its scroll bar.
fn draw_questions(
    ctx: &mut Ctx,
    s: &mut Scene,
    list: &List,
    entries: &[Question],
    bar: &ScrollBar,
) -> io::Result<()> {
    let heights: Vec<f32> = entries.iter().map(|q| q.height).collect();
    let (at, content) = list.desc.layout(&heights);
    let clip = view(list);
    let start = s.sprites.len();
    for (i, (q, (x, y))) in entries.iter().zip(at).enumerate() {
        let (left, top) = (list.rect.x + x, list.rect.y + y - bar.pos as f32);
        let t = &q.template;
        if !intersects(Rect::new(left, top, t.window.width, q.height), clip) {
            continue;
        }
        let ct = &t.content_text;
        let mut rect = ct.window.absolute(left, top);
        rect.height = q.caption_height;
        let (mut b, font) = TextBox::of_static(ct, rect);
        b.align = (ct.text.as_ref())
            .and_then(|t| t.align)
            .unwrap_or(HAlign::Centre);
        b.vert_align = VAlign::Centre;
        b.color = crate::dialog::look_color(ct, "e", b.color);
        if !q.text.is_empty() {
            let layer = format!("talk question {i}");
            ctx.text(s, &layer, &encode(&q.text), &b, font)?;
        }
    }
    s.clip_from(start, pixels(clip, ctx.scale()));
    draw_bar(ctx, s, "talk questions bar", list, content, bar)
}

/// A panel as tests see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PanelState {
    pub character: Option<u16>,
    pub name: Option<String>,
    pub rank: Option<String>,
    pub community: Option<String>,
    pub relation: Option<String>,
    pub relation_shown: bool,
    pub relation_color: Option<[u8; 4]>,
    pub icon: Option<Icon>,
    pub icon_color: Option<[u8; 4]>,
}

impl PanelState {
    fn of(p: &Panel) -> Self {
        let rgba = |c: Color| [c.r, c.g, c.b, c.a];
        Self {
            character: p.character,
            name: p.name.clone(),
            rank: p.rank.clone(),
            community: p.community.clone(),
            relation: p.relation.clone(),
            relation_shown: !p.relation_hidden,
            relation_color: p.relation_color.map(rgba),
            icon: p.icon.clone(),
            icon_color: p.icon_color.map(rgba),
        }
    }
}

/// A list entry as tests see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EntryState {
    pub template: String,
    /// Its top-left, absolute, before scrolling.
    pub at: [f32; 2],
    pub height: f32,
    pub name: String,
    pub text: String,
}

/// The window as tests see it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TalkState {
    pub shown: bool,
    pub special_mode: bool,
    pub button_shown: bool,
    pub titles: [String; 2],
    pub panels: Vec<PanelState>,
    pub answers: Vec<EntryState>,
    pub answers_scroll: i32,
    pub answers_bar: bool,
    pub questions: Vec<EntryState>,
    pub questions_scroll: i32,
    pub questions_bar: bool,
}
