// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The context menu (docs/behavior/inventory-screen.md section 5.6): its
//! entries for an item, its size and where it opens.

use serde::Serialize;

use super::layout::{MENU_ROW, MenuDesc};
use crate::core::character::inventory::{Inventory, Place};
use volya_engine_formats::object::Kind;
use volya_engine_formats::ui::layout::Rect;

/// A context menu entry (section 5.6), in the menu's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    MoveToSlot,
    MoveOnBelt,
    MoveToBag,
    UndressOutfit,
    DressOutfit,
    Use,
    Drink,
    Eat,
    Drop,
    DropAll,
}

/// Every entry's string key, for checks.
pub const ALL_KEYS: [&str; 10] = [
    "st_move_to_slot",
    "st_move_on_belt",
    "st_move_to_bag",
    "st_undress_outfit",
    "st_dress_outfit",
    "st_use",
    "st_drink",
    "st_eat",
    "st_drop",
    "st_drop_all",
];

impl Entry {
    /// The string key of its text.
    pub fn key(self) -> &'static str {
        ALL_KEYS[self as usize]
    }
}

/// The entries for item `id` shown in a cell item grouping `grouped` items
/// (section 5.6), in order; empty when none applies (no menu then).
///
/// Entries 5–10″ (detaching and attaching weapon addons, unloading a
/// magazine) wait for weapons: which weapons can take which addons, the
/// addon items and the ammunition are dependent (section 5.8).
pub fn entries(inv: &Inventory, id: u16, grouped: usize) -> Vec<Entry> {
    let Some(item) = inv.item(id) else {
        return Vec::new();
    };
    let c = &item.config;
    let outfit = c.class_id.kind() == Kind::Outfit;
    let slot = c.slot_number();
    let mut out = Vec::new();
    // 1: not an outfit; a slot that is not persistent, and it can go there.
    if !outfit && slot.is_some_and(|s| !inv.is_persistent(s)) && inv.can_go_to_slot(id) {
        out.push(Entry::MoveToSlot);
    }
    // 2.
    if c.belt && inv.can_go_on_belt(id) {
        out.push(Entry::MoveOnBelt);
    }
    // 3: not in the backpack, not the bolt, no persistent slot.
    let to_bag = item.place != Place::Backpack
        && !c.is_bolt()
        && !slot.is_some_and(|s| inv.is_persistent(s));
    if to_bag {
        out.push(if outfit {
            Entry::UndressOutfit
        } else {
            Entry::MoveToBag
        });
    }
    // 4: an outfit for which entry 3 was not added.
    if outfit && !to_bag {
        out.push(Entry::DressOutfit);
    }
    // TODO(#111): entries 5–10″ once weapons exist (section 5.8).
    // 11: by the class.
    if c.is_eatable() {
        out.push(match c.class_id.as_str() {
            "II_MEDKI" | "II_BANDG" | "II_ANTIR" => Entry::Use,
            "II_BOTTL" => Entry::Drink,
            _ => Entry::Eat,
        });
    }
    // 12, 13.
    if !c.quest_item {
        out.push(Entry::Drop);
        if grouped > 1 {
            out.push(Entry::DropAll);
        }
    }
    out
}

/// The menu's size for `n` entries whose widest text is `widest` units
/// (section 5.6): *n* · 20 + `top_indent` + `bottom_indent` high, max(20,
/// `left_ident` + `right_ident` + *T* + 2) wide.
pub fn size(d: &MenuDesc, n: usize, widest: f32) -> (f32, f32) {
    let width = (d.left_ident + d.right_ident + widest + 2.0).max(20.0);
    (width, n as f32 * MENU_ROW + d.top_indent + d.bottom_indent)
}

/// The menu's top-left for the cursor at `(x, y)` and size `(w, h)`
/// (section 5.6): to the left of the cursor when it fits, else above it,
/// else above and right, else down and right.
pub fn position((x, y): (f32, f32), (w, h): (f32, f32)) -> (f32, f32) {
    const W: f32 = 1024.0;
    const H: f32 = 768.0;
    if x - w > 0.0 && y + h < H {
        (x - w, y)
    } else if x - w > 0.0 && y - h > 0.0 {
        (x - w, y - h)
    } else if x + w < W && y - h > 0.0 {
        (x, y - h)
    } else {
        (x, y)
    }
}

/// A shown context menu.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextMenu {
    /// The item it acts on (the selected one).
    pub item: u16,
    pub entries: Vec<Entry>,
    /// The menu (frame window), absolute.
    pub rect: Rect,
    /// The row under the cursor (section 5.6: hover selects it).
    pub hovered: Option<usize>,
}

impl ContextMenu {
    /// The list's absolute top-left (section 5.6: (5, 5) into the menu).
    pub fn list_origin(&self) -> (f32, f32) {
        let (dx, dy) = super::layout::MENU_LIST_OFFSET;
        (self.rect.x + dx, self.rect.y + dy)
    }

    /// Row `i`'s absolute rectangle: 20 units high from the list's top
    /// (`top_indent` 0), as wide as the list (the menu's width).
    pub fn row_rect(&self, i: usize, top_indent: f32) -> Rect {
        let (x, y) = self.list_origin();
        Rect::new(
            x,
            y + top_indent + i as f32 * MENU_ROW,
            self.rect.width,
            MENU_ROW,
        )
    }

    /// The row under `p`.
    pub fn row_at(&self, p: [f32; 2], top_indent: f32) -> Option<usize> {
        (0..self.entries.len()).find(|&i| {
            let r = self.row_rect(i, top_indent);
            p[0] >= r.x && p[0] < r.x + r.width && p[1] >= r.y && p[1] < r.y + r.height
        })
    }

    /// Whether `p` is on the menu (the frame window).
    pub fn contains(&self, p: [f32; 2]) -> bool {
        crate::dialog::contains(self.rect, p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_left_of_the_cursor_when_it_fits() {
        assert_eq!(position((500.0, 300.0), (100.0, 50.0)), (400.0, 300.0));
        // Too low: above it, still left.
        assert_eq!(position((500.0, 740.0), (100.0, 50.0)), (400.0, 690.0));
        // At the left edge: to the right, above.
        assert_eq!(position((50.0, 300.0), (100.0, 50.0)), (50.0, 250.0));
        // In the top-left corner: down and right.
        assert_eq!(position((50.0, 20.0), (100.0, 50.0)), (50.0, 20.0));
    }

    #[test]
    fn its_size_follows_the_entries() {
        let d = MenuDesc {
            texture: "ui_cb_listbox".into(),
            font: volya_engine_formats::text::font::UiFont::Letterica16,
            color: volya_engine_formats::ui::Color::WHITE,
            selected_color: volya_engine_formats::ui::Color::WHITE,
            left_ident: 20.0,
            right_ident: 10.0,
            top_indent: 0.0,
            bottom_indent: 10.0,
        };
        // Section 9: two rows are 50 units high.
        assert_eq!(size(&d, 2, 100.0), (132.0, 50.0));
        assert_eq!(size(&d, 1, 0.0), (32.0, 30.0));
        assert_eq!(size(&d, 1, -40.0).0, 20.0);
    }
}
