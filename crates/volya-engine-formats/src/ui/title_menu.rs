// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The title menu's layout file `config\ui\ui_mm_main.xml` read into typed
//! elements, as docs/behavior/title-menu.md (sections 3, 4, 6 and 10) and
//! docs/formats/ui-layout.md (section 10) describe it.

use std::io;

use super::layout::{
    FontDesc, HAlign, Rect, ScrollListDesc, StaticDesc, VAlign, WindowDesc, require,
};
use super::{Color, ColorDefs, float_attr};
use crate::text::font::UiFont;
use crate::text::layout::TextBox;
use crate::xml::{Document, Element};

/// One item of a menu list: the name actions bind to and the caption key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuButton {
    pub name: String,
    pub caption: String,
}

/// A `menu_*` list (title-menu.md section 4.1).
#[derive(Clone, Debug, PartialEq)]
pub struct MenuList {
    pub name: String,
    pub btn_height: f32,
    pub font: Option<UiFont>,
    /// `a r g b` (the spec does not say which defaults apply; those of text
    /// colours, 255, are used; every list sets all four).
    pub color: Color,
    pub buttons: Vec<MenuButton>,
}

impl MenuList {
    fn read(e: &Element, defs: &ColorDefs) -> io::Result<Self> {
        let font = FontDesc::read(e, defs)?;
        let buttons = e
            .children_named("btn")
            .map(|b| {
                let attr = |n: &str| {
                    b.attr(n).map(str::to_owned).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("{}: btn without {n}", e.name),
                        )
                    })
                };
                Ok(MenuButton {
                    name: attr("name")?,
                    caption: attr("caption")?,
                })
            })
            .collect::<io::Result<_>>()?;
        Ok(Self {
            name: e.name.clone(),
            btn_height: float_attr(e, "btn_height", 0.0),
            font: font.font,
            color: font.color,
            buttons,
        })
    }
}

/// `menu_sound` (title-menu.md section 6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuSound {
    pub random: bool,
    /// `menu_music` values in document order: track names *N*, played from
    /// `sounds\N_l.ogg` and `sounds\N_r.ogg`.
    pub music: Vec<String>,
    pub whell_sound: Option<String>,
    pub whell_click: Option<String>,
}

impl MenuSound {
    fn read(e: &Element) -> Self {
        let value = |n| e.child(n).and_then(Element::value).map(str::to_owned);
        Self {
            random: super::flag_attr(e, "random", false),
            music: e
                .children_named("menu_music")
                .filter_map(Element::value)
                .map(str::to_owned)
                .collect(),
            whell_sound: value("whell_sound"),
            whell_click: value("whell_click"),
        }
    }
}

/// Names of the menu lists, in the order of title-menu.md section 4.1.
pub const MENU_LISTS: [&str; 6] = [
    "menu_main_last_save",
    "menu_main",
    "menu_main_single_dead",
    "menu_main_single",
    "menu_main_mm",
    "menu_new_game",
];

/// The title menu's elements. Rectangles are as written (relative to the
/// parent named in each field's description).
#[derive(Clone, Debug, PartialEq)]
pub struct TitleMenuLayout {
    /// Layers relative to the UI space.
    pub back_movie: StaticDesc,
    pub background: StaticDesc,
    pub fire_movie: StaticDesc,
    /// `shniaga_wnd`, relative to the UI space.
    pub shniaga_wnd: WindowDesc,
    /// `shniaga_wnd:buttons_region`, relative to `shniaga_wnd`.
    pub buttons_region: ScrollListDesc,
    /// `shniaga_wnd:shniaga` (the bar), relative to `shniaga_wnd`.
    pub shniaga: StaticDesc,
    /// The bar's children, relative to the bar.
    pub magnifire: StaticDesc,
    /// `magnifire`'s `y_offset` value (0 when absent).
    pub y_offset: f32,
    pub left_anim: StaticDesc,
    pub right_anim: StaticDesc,
    pub left_grating: StaticDesc,
    pub right_grating: StaticDesc,
    /// The `menu_*` lists present, in [`MENU_LISTS`] order.
    pub menus: Vec<MenuList>,
    pub menu_sound: MenuSound,
    pub static_version: StaticDesc,
}

impl TitleMenuLayout {
    /// Reads the elements from `ui_mm_main.xml`. All except the menu lists
    /// are required; a missing `menu_sound` gives no music.
    pub fn read(doc: &Document, defs: &ColorDefs) -> io::Result<Self> {
        let s = |path: &str| StaticDesc::read(require(doc, path, 0)?, defs);
        let magnifire = require(doc, "shniaga_wnd:shniaga:magnifire", 0)?;
        let menus = MENU_LISTS
            .iter()
            .filter_map(|name| doc.find(name, 0))
            .map(|e| MenuList::read(e, defs))
            .collect::<io::Result<_>>()?;
        Ok(Self {
            back_movie: s("back_movie")?,
            background: s("background")?,
            fire_movie: s("fire_movie")?,
            shniaga_wnd: WindowDesc::read(require(doc, "shniaga_wnd", 0)?, defs)?,
            buttons_region: ScrollListDesc::read(
                require(doc, "shniaga_wnd:buttons_region", 0)?,
                defs,
            )?,
            shniaga: s("shniaga_wnd:shniaga")?,
            y_offset: magnifire
                .child("y_offset")
                .and_then(Element::value)
                .map_or(0.0, crate::ltx::value::float),
            magnifire: StaticDesc::read(magnifire, defs)?,
            left_anim: s("shniaga_wnd:shniaga:left_anim")?,
            right_anim: s("shniaga_wnd:shniaga:right_anim")?,
            left_grating: s("shniaga_wnd:shniaga:left_grating")?,
            right_grating: s("shniaga_wnd:shniaga:right_grating")?,
            menus,
            menu_sound: doc
                .find("menu_sound", 0)
                .map(MenuSound::read)
                .unwrap_or_default(),
            static_version: s("static_version")?,
        })
    }

    /// The menu list named `name`.
    pub fn menu(&self, name: &str) -> Option<&MenuList> {
        self.menus.iter().find(|m| m.name == name)
    }

    /// The absolute rows of `menu`'s items (title-menu.md section 4.2):
    /// `buttons_region` (inside `shniaga_wnd`) stacks items `btn_height`
    /// high and as wide as the region; at 1024 × 768 units item *i* is
    /// (700, 419 + 30·*i*, 164 × 30).
    pub fn caption_rows(&self, menu: &MenuList) -> Vec<Rect> {
        let wnd = self.shniaga_wnd.absolute(0.0, 0.0);
        let region = self.buttons_region.window.absolute(wnd.x, wnd.y);
        let heights = vec![menu.btn_height; menu.buttons.len()];
        let (positions, _) = self.buttons_region.layout(&heights);
        positions
            .into_iter()
            .map(|(x, y)| Rect::new(region.x + x, region.y + y, region.width, menu.btn_height))
            .collect()
    }

    /// The text boxes of `menu`'s captions: the menu code draws every
    /// caption single-line, centred horizontally and vertically in its row,
    /// in the list's colour (ui-layout.md section 7.3, text-and-fonts.md
    /// section 5); the font is [`MenuList::font`].
    pub fn caption_boxes(&self, menu: &MenuList) -> Vec<TextBox> {
        self.caption_rows(menu)
            .into_iter()
            .map(|rect| TextBox {
                rect,
                offset: (0.0, 0.0),
                align: HAlign::Centre,
                vert_align: VAlign::Centre,
                complex: false,
                color: menu.color,
                line_gap: 0.0,
            })
            .collect()
    }
}
