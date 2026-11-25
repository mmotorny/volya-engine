// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The inventory screen's layout (docs/behavior/inventory-screen.md,
//! sections 2, 3, 4.6–4.8, 5.6 and 7): `inventory_new.xml` with the
//! information panel of `inventory_item.xml`, each element at its absolute
//! rectangle under the parent the spec gives it (section 3.1), whatever the
//! file's order.

use std::io;

use crate::core::ui::load_ui_file;
use crate::core::vfs::Vfs;
use crate::data::GameData;
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::ui::layout::{Rect, StaticDesc, font_attr, require};
use volya_engine_formats::ui::options::ScrollBarProfile;
use volya_engine_formats::ui::{Color, ColorDefs, Resolution, flag_attr, float_attr, int_attr};
use volya_engine_formats::xml::{Document, Element};

fn invalid(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

/// The layout file (section 2; the `_16` rule picks the variant).
pub const LAYOUT_FILE: &str = "inventory_new.xml";
/// The information panel's file (section 4.6).
pub const ITEM_FILE: &str = "inventory_item.xml";
/// The item icons (section 4.2): 50 pixels per grid unit.
pub const ICONS: &str = "ui\\ui_icon_equipment";
pub const ICON_PIXELS: f32 = 50.0;
/// The cell background (section 3.2): its left half.
pub const GRID: &str = "ui\\ui_grid";
/// The character picture without an outfit (section 4.4).
pub const WITHOUT_OUTFIT: &str = "npc_icon_without_outfit";
/// The context menu's list sits this far into the menu (section 5.6).
pub const MENU_LIST_OFFSET: (f32, f32) = (5.0, 5.0);
/// Context menu rows (section 5.6).
pub const MENU_ROW: f32 = 20.0;
/// The context menu's highlight line (section 5.6).
pub const MENU_LINE: &str = "ui_listline";

/// A static at its absolute rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub desc: StaticDesc,
    pub rect: Rect,
}

impl Placed {
    fn read(e: &Element, colors: &ColorDefs, parent: (f32, f32)) -> io::Result<Self> {
        let desc = StaticDesc::read(e, colors)?;
        let rect = desc.window.absolute(parent.0, parent.1);
        Ok(Self { desc, rect })
    }

    fn origin(&self) -> (f32, f32) {
        (self.rect.x, self.rect.y)
    }
}

/// Which item list (section 3.2), in the order of [`Layout::lists`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ListKind {
    Bag,
    Belt,
    Outfit,
    Pistol,
    Automatic,
}

impl ListKind {
    pub const ALL: [ListKind; 5] = [
        ListKind::Bag,
        ListKind::Belt,
        ListKind::Outfit,
        ListKind::Pistol,
        ListKind::Automatic,
    ];

    pub fn element(self) -> &'static str {
        match self {
            ListKind::Bag => "dragdrop_bag",
            ListKind::Belt => "dragdrop_belt",
            ListKind::Outfit => "dragdrop_outfit",
            ListKind::Pistol => "dragdrop_pistol",
            ListKind::Automatic => "dragdrop_automatic",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The list a slot's item shows in (sections 4.1, 5.3): 1 pistol, 2
    /// automatic, 6 outfit, 3 (grenades) the backpack.
    pub fn of_slot(slot: u8) -> Option<ListKind> {
        match slot {
            1 => Some(ListKind::Pistol),
            2 => Some(ListKind::Automatic),
            6 => Some(ListKind::Outfit),
            3 => Some(ListKind::Bag),
            _ => None,
        }
    }

    /// The classes of section 5.3.
    pub fn is_slot(self) -> bool {
        matches!(
            self,
            ListKind::Outfit | ListKind::Pistol | ListKind::Automatic
        )
    }
}

/// An item list's attributes (section 3.2) at its absolute rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListDesc {
    pub kind: ListKind,
    pub rect: Rect,
    pub cell: (f32, f32),
    pub cols: usize,
    pub rows: usize,
    pub unlimited: bool,
    pub group_similar: bool,
    pub custom_placement: bool,
}

impl ListDesc {
    fn read(kind: ListKind, e: &Element, parent: (f32, f32)) -> Self {
        let r = Rect::new(
            float_attr(e, "x", 0.0),
            float_attr(e, "y", 0.0),
            float_attr(e, "width", 0.0),
            float_attr(e, "height", 0.0),
        );
        let count = |name| int_attr(e, name, 0).max(0) as usize;
        Self {
            kind,
            rect: Rect::new(parent.0 + r.x, parent.1 + r.y, r.width, r.height),
            cell: (
                float_attr(e, "cell_width", 0.0),
                float_attr(e, "cell_height", 0.0),
            ),
            cols: count("cols_num"),
            rows: count("rows_num"),
            unlimited: flag_attr(e, "unlimited", false),
            group_similar: flag_attr(e, "group_similar", false),
            custom_placement: flag_attr(e, "custom_placement", true),
        }
    }
}

/// A progress bar (section 4.7) at its absolute rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct BarDesc {
    pub rect: Rect,
    pub min: f32,
    pub max: f32,
    pub horizontal: bool,
    pub inertion: f32,
    /// The `progress` child: its picture, sized to the whole bar.
    pub progress: StaticDesc,
    pub min_color: Option<Color>,
    pub max_color: Option<Color>,
}

impl BarDesc {
    fn read(e: &Element, colors: &ColorDefs, parent: (f32, f32)) -> io::Result<Self> {
        let window = StaticDesc::read(e, colors)?.window;
        let rect = window.absolute(parent.0, parent.1);
        let mut progress = StaticDesc::read(
            e.child("progress")
                .ok_or_else(|| invalid(format!("<{}> has no progress", e.name)))?,
            colors,
        )?;
        progress.window.x = 0.0;
        progress.window.y = 0.0;
        progress.window.width = rect.width;
        progress.window.height = rect.height;
        let color = |name| {
            e.child(name)
                .map(|c| volya_engine_formats::ui::read_color(c, colors, 255))
        };
        Ok(Self {
            rect,
            min: float_attr(e, "min", 0.0),
            max: float_attr(e, "max", 0.0),
            horizontal: int_attr(e, "horz", 0) == 1,
            inertion: float_attr(e, "inertion", 0.0),
            progress,
            min_color: color("min_color"),
            max_color: color("max_color"),
        })
    }
}

/// The screen's sounds (section 7): files of `sounds\`, without `.ogg`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sounds {
    pub open: String,
    pub close: String,
    pub item_to_slot: String,
    pub item_to_belt: String,
    pub item_to_ruck: String,
    pub properties: String,
    pub drop_item: String,
    pub attach_addon: String,
    pub detach_addon: String,
    pub item_use: String,
}

impl Sounds {
    fn read(doc: &Document) -> io::Result<Self> {
        let s = |name: &str| -> io::Result<String> {
            Ok(require(doc, &format!("action_sounds:{name}"), 0)?
                .value()
                .unwrap_or_default()
                .to_owned())
        };
        Ok(Self {
            open: s("snd_open")?,
            close: s("snd_close")?,
            item_to_slot: s("snd_item_to_slot")?,
            item_to_belt: s("snd_item_to_belt")?,
            item_to_ruck: s("snd_item_to_ruck")?,
            properties: s("snd_properties")?,
            drop_item: s("snd_drop_item")?,
            attach_addon: s("snd_attach_addon")?,
            detach_addon: s("snd_detach_addon")?,
            item_use: s("snd_item_use")?,
        })
    }

    /// Every sound, for checks.
    pub fn all(&self) -> [&str; 10] {
        [
            &self.open,
            &self.close,
            &self.item_to_slot,
            &self.item_to_belt,
            &self.item_to_ruck,
            &self.properties,
            &self.drop_item,
            &self.attach_addon,
            &self.detach_addon,
            &self.item_use,
        ]
    }
}

/// The context menu's look (section 5.6, `properties_box`).
#[derive(Clone, Debug, PartialEq)]
pub struct MenuDesc {
    /// The frame window's texture base.
    pub texture: String,
    pub font: UiFont,
    pub color: Color,
    /// The selected row's text colour (`font_s`).
    pub selected_color: Color,
    pub left_ident: f32,
    pub right_ident: f32,
    pub top_indent: f32,
    pub bottom_indent: f32,
}

impl MenuDesc {
    fn read(doc: &Document, colors: &ColorDefs) -> io::Result<Self> {
        let list = require(doc, "properties_box:list", 0)?;
        let font = require(doc, "properties_box:list:font", 0)?;
        let selected = require(doc, "properties_box:list:font_s", 0)?;
        Ok(Self {
            texture: require(doc, "properties_box:texture", 0)?
                .value()
                .unwrap_or_default()
                .to_owned(),
            font: font_attr(font)?.unwrap_or(UiFont::DEFAULT),
            color: volya_engine_formats::ui::read_color(font, colors, 255),
            selected_color: volya_engine_formats::ui::read_color(selected, colors, 255),
            left_ident: float_attr(list, "left_ident", 0.0),
            right_ident: float_attr(list, "right_ident", 0.0),
            top_indent: float_attr(list, "top_indent", 0.0),
            bottom_indent: float_attr(list, "bottom_indent", 0.0),
        })
    }
}

/// The information panel (section 4.6), relative to `descr_static`'s
/// top-left made absolute.
#[derive(Clone, Debug, PartialEq)]
pub struct InfoDesc {
    pub name: Placed,
    pub weight: Placed,
    pub cost: Placed,
    pub condition: Placed,
    pub condition_bar: BarDesc,
    /// The description scroll view and its `left_ident`.
    pub descr_list: Rect,
    pub descr_left_ident: f32,
    pub descr_font: UiFont,
    pub descr_color: Color,
    /// The icon's centre (absolute) and largest size.
    pub image_centre: (f32, f32),
    pub image_max: (f32, f32),
}

impl InfoDesc {
    fn read(doc: &Document, colors: &ColorDefs, origin: (f32, f32)) -> io::Result<Self> {
        let p = |name: &str| Placed::read(require(doc, name, 0)?, colors, origin);
        let list = require(doc, "descr_list", 0)?;
        let list_rect = StaticDesc::read(list, colors)?
            .window
            .absolute(origin.0, origin.1);
        let font = list.child("font");
        let image = require(doc, "image_static", 0)?;
        Ok(Self {
            name: p("static_name")?,
            weight: p("static_weight")?,
            cost: p("static_cost")?,
            condition: p("static_condition")?,
            condition_bar: BarDesc::read(require(doc, "condition_progress", 0)?, colors, origin)?,
            descr_list: list_rect,
            descr_left_ident: float_attr(list, "left_ident", 0.0),
            descr_font: font
                .map(font_attr)
                .transpose()?
                .flatten()
                .unwrap_or(UiFont::DEFAULT),
            descr_color: font.map_or(Color::WHITE, |f| {
                volya_engine_formats::ui::read_color(f, colors, 255)
            }),
            image_centre: (
                origin.0 + float_attr(image, "x", 0.0),
                origin.1 + float_attr(image, "y", 0.0),
            ),
            image_max: (
                float_attr(image, "width", 0.0),
                float_attr(image, "height", 0.0),
            ),
        })
    }
}

/// The outfit protection lines' statics (section 4.8), by line *k*.
pub const PROTECTION_STATICS: [&str; 9] = [
    "static_burn_immunity",
    "static_strike_immunity",
    "static_shock_immunity",
    "static_wound_immunity",
    "static_radiation_immunity",
    "static_telepatic_immunity",
    "static_chemical_burn_immunity",
    "static_explosion_immunity",
    "static_fire_wound_immunity",
];

/// Line *k*'s label key and outfit key (section 4.8; lines 1 and 2 use
/// each other's statics, so their labels and keys are swapped against
/// [`PROTECTION_STATICS`]).
pub const PROTECTIONS: [(&str, &str); 9] = [
    ("ui_inv_outfit_burn_protection", "burn_protection"),
    ("ui_inv_outfit_shock_protection", "shock_protection"),
    ("ui_inv_outfit_strike_protection", "strike_protection"),
    ("ui_inv_outfit_wound_protection", "wound_protection"),
    ("ui_inv_outfit_radiation_protection", "radiation_protection"),
    ("ui_inv_outfit_telepatic_protection", "telepatic_protection"),
    (
        "ui_inv_outfit_chemical_burn_protection",
        "chemical_burn_protection",
    ),
    ("ui_inv_outfit_explosion_protection", "explosion_protection"),
    (
        "ui_inv_outfit_fire_wound_protection",
        "fire_wound_protection",
    ),
];

/// The whole screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub belt_slots: Placed,
    pub back: Placed,
    pub bottom_static: Placed,
    pub bag_static: Placed,
    pub money_static: Placed,
    pub descr_static: Placed,
    pub frame_title: Placed,
    pub progress_background: Placed,
    /// Health, psy, radiation.
    pub bars: [BarDesc; 3],
    pub outfit_caption: Placed,
    /// The protection lines' scroll view (absolute).
    pub outfit_view: Rect,
    /// Per line *k*: its static, at its size (placed by the view).
    pub protections: Vec<StaticDesc>,
    /// In [`ListKind::ALL`] order.
    pub lists: [ListDesc; 5],
    pub time_static: Placed,
    pub time_str: Placed,
    pub exit: Placed,
    pub sounds: Sounds,
    pub menu: MenuDesc,
    pub info: InfoDesc,
    pub scroll: ScrollBarProfile,
}

impl Layout {
    /// Loads the layout at `resolution` (the `_16` rule at that moment,
    /// section 2).
    pub fn load(tree: &dyn Vfs, data: &GameData, resolution: Resolution) -> io::Result<Self> {
        let ui = |name: &str| load_ui_file(tree, &data.aliases, name, resolution);
        Self::read(
            &ui(LAYOUT_FILE)?,
            &ui(ITEM_FILE)?,
            &ui("scroll_bar.xml")?,
            &data.colors,
        )
    }

    pub fn read(
        doc: &Document,
        item: &Document,
        scroll: &Document,
        colors: &ColorDefs,
    ) -> io::Result<Self> {
        let at =
            |name: &str, parent: (f32, f32)| Placed::read(require(doc, name, 0)?, colors, parent);
        let top = (0.0, 0.0);
        let bag_static = at("bag_static", top)?;
        let frame = at("character_frame_window", top)?;
        let progress_background = at("progress_background", top)?;
        let bar = |name: &str| {
            BarDesc::read(require(doc, name, 0)?, colors, progress_background.origin())
        };
        let outfit = at("outfit_info", top)?;
        let caption = outfit
            .desc
            .window
            .auto_statics
            .first()
            .cloned()
            .ok_or_else(|| invalid("outfit_info has no caption static"))?;
        let caption_rect = caption.window.absolute(outfit.rect.x, outfit.rect.y);
        let view = require(doc, "outfit_info:scroll_view", 0)?;
        let outfit_view = StaticDesc::read(view, colors)?
            .window
            .absolute(outfit.rect.x, outfit.rect.y);
        let protections = PROTECTION_STATICS
            .iter()
            .map(|name| StaticDesc::read(require(doc, &format!("outfit_info:{name}"), 0)?, colors))
            .collect::<io::Result<Vec<_>>>()?;
        let list = |kind: ListKind| -> io::Result<ListDesc> {
            let parent = if kind == ListKind::Bag {
                bag_static.origin()
            } else {
                top
            };
            Ok(ListDesc::read(
                kind,
                require(doc, kind.element(), 0)?,
                parent,
            ))
        };
        let time_static = at("time_static", top)?;
        let descr_static = at("descr_static", top)?;
        Ok(Self {
            belt_slots: at("belt_slots", top)?,
            back: at("back", top)?,
            bottom_static: at("bottom_static", top)?,
            money_static: at("money_static", top)?,
            frame_title: at("character_frame_window:title", frame.origin())?,
            bars: [
                bar("progress_bar_health")?,
                bar("progress_bar_psy")?,
                bar("progress_bar_radiation")?,
            ],
            outfit_caption: Placed {
                desc: caption,
                rect: caption_rect,
            },
            outfit_view,
            protections,
            lists: [
                list(ListKind::Bag)?,
                list(ListKind::Belt)?,
                list(ListKind::Outfit)?,
                list(ListKind::Pistol)?,
                list(ListKind::Automatic)?,
            ],
            time_str: at("time_static_str", time_static.origin())?,
            time_static,
            exit: at("exit_button", top)?,
            sounds: Sounds::read(doc)?,
            menu: MenuDesc::read(doc, colors)?,
            info: InfoDesc::read(item, colors, descr_static.origin())?,
            scroll: ScrollBarProfile::read(scroll, "default")?,
            bag_static,
            descr_static,
            progress_background,
        })
    }

    pub fn list(&self, kind: ListKind) -> &ListDesc {
        &self.lists[kind.index()]
    }

    /// Every texture id or path the screen draws from its layout, and its
    /// string keys, for checks.
    pub fn references(&self) -> (Vec<String>, Vec<String>) {
        let mut textures = vec![ICONS.to_owned(), GRID.to_owned(), WITHOUT_OUTFIT.to_owned()];
        let mut strings = vec!["ui_inv_weight".to_owned(), "st_condition".to_owned()];
        let statics = [
            &self.belt_slots,
            &self.back,
            &self.bottom_static,
            &self.bag_static,
            &self.money_static,
            &self.descr_static,
            &self.frame_title,
            &self.progress_background,
            &self.outfit_caption,
            &self.time_static,
            &self.time_str,
        ];
        for p in statics {
            textures.extend(p.desc.texture.iter().map(|t| t.name.clone()));
            strings.extend(
                p.desc
                    .text
                    .iter()
                    .flat_map(|t| t.key.clone())
                    // The weight line replaces this one (section 3.1).
                    .filter(|k| k != "ui_inv_bag"),
            );
        }
        for b in self.bars.iter().chain([&self.info.condition_bar]) {
            textures.extend(b.progress.texture.iter().map(|t| t.name.clone()));
        }
        for s in &self.protections {
            textures.extend(s.texture.iter().map(|t| t.name.clone()));
        }
        strings.extend(PROTECTIONS.iter().map(|(label, _)| (*label).to_owned()));
        textures.extend(crate::dialog::button_textures(&self.exit.desc));
        strings.extend(self.exit.desc.text.iter().flat_map(|t| t.key.clone()));
        for p in ["lt", "t", "rt", "l", "back", "r", "lb", "b", "rb"] {
            textures.push(format!("{}_{p}", self.menu.texture));
        }
        textures.extend(["b", "back", "e"].map(|p| format!("{MENU_LINE}_{p}")));
        strings.extend(super::menu::ALL_KEYS.iter().map(|k| (*k).to_owned()));
        let s = &self.scroll;
        textures.extend([
            s.up.clone(),
            s.down.clone(),
            s.track.clone(),
            s.thumb.clone(),
        ]);
        textures.sort();
        textures.dedup();
        strings.sort();
        strings.dedup();
        (textures, strings)
    }
}
