// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The item lists (docs/behavior/inventory-screen.md sections 3.2 and 4.3):
//! grids of cells holding cell items, their placement, grouping, growth and
//! scrolling.

use std::io;

use super::layout::ListDesc;
use crate::core::character::inventory::{Inventory, InventoryItem};
use crate::dialog::contains;
use volya_engine_formats::ui::layout::Rect;

/// Items whose conditions differ by less than this are similar (4.3).
const SIMILAR_CONDITION: f32 = 0.01;

/// A cell item (4.2): the item it shows first, then the items grouped in
/// it, at its top-left cell, `size` cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellItem {
    pub items: Vec<u16>,
    pub cell: (usize, usize),
    pub size: (usize, usize),
}

impl CellItem {
    /// The cell item's own item.
    pub fn item(&self) -> u16 {
        self.items[0]
    }
}

/// Whether `a` and `b` are similar (4.3): the same section and conditions
/// within 0.01; weapons with the same installed addons and in the same
/// place. The further test of ammunition boxes is unestablished (believed
/// always true for boxes of one section) and not made.
pub fn similar(a: &InventoryItem, b: &InventoryItem) -> bool {
    if a.config.section != b.config.section
        || (a.condition - b.condition).abs() >= SIMILAR_CONDITION
    {
        return false;
    }
    if a.config.is_weapon() {
        return addons(a) == addons(b) && a.place == b.place;
    }
    true
}

/// A weapon's installed-addon byte (0 for other items).
pub fn addons(i: &InventoryItem) -> u8 {
    match &i.state {
        crate::core::character::inventory::ItemState::Weapon(w) => w.installed_addons,
        _ => 0,
    }
}

/// Whether the item is of class `AMMO` (4.2: the count text of its
/// boxes; grenade-launcher ammunition is of other classes).
pub fn is_ammunition(i: &InventoryItem) -> bool {
    i.config.class_id.as_str() == "AMMO"
}

/// An item list.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemList {
    pub desc: ListDesc,
    /// Rows of the grid block (`rows_num` at a rebuild, growing in an
    /// `unlimited` list).
    pub rows: usize,
    /// In placement order.
    pub cells: Vec<CellItem>,
    /// How far the grid block is scrolled up, units.
    pub scroll: f32,
}

impl ItemList {
    pub fn new(desc: ListDesc) -> Self {
        Self {
            rows: desc.rows,
            desc,
            cells: Vec::new(),
            scroll: 0.0,
        }
    }

    /// Emptied for a rebuild (4.1).
    pub fn clear(&mut self) {
        self.cells.clear();
        self.rows = self.desc.rows;
        self.scroll = 0.0;
    }

    /// The grid block's height, units (3.2).
    pub fn block_height(&self) -> f32 {
        self.rows as f32 * self.desc.cell.1
    }

    /// The scroll bar shows while the block is taller than the list (3.2).
    pub fn bar_shown(&self) -> bool {
        self.block_height() > self.desc.rect.height
    }

    /// The grid block's absolute top-left, scrolling included.
    pub fn origin(&self) -> (f32, f32) {
        (self.desc.rect.x, self.desc.rect.y - self.scroll)
    }

    /// The scroll step (3.2, believed): `cell_height` / 3, integer
    /// division.
    pub fn scroll_step(&self) -> f32 {
        (self.desc.cell.1 as i64 / 3) as f32
    }

    /// Scrolls by `steps` steps (negative up), within the block.
    pub fn scroll_by(&mut self, steps: i32) {
        let most = (self.block_height() - self.desc.rect.height).max(0.0);
        self.scroll = (self.scroll + steps as f32 * self.scroll_step()).clamp(0.0, most);
    }

    /// Cell item `i`'s absolute rectangle (its *w* · `cell_width` × *h* ·
    /// `cell_height` units).
    pub fn cell_rect(&self, i: usize) -> Rect {
        let c = &self.cells[i];
        let (x, y) = self.origin();
        let (cw, ch) = self.desc.cell;
        Rect::new(
            x + c.cell.0 as f32 * cw,
            y + c.cell.1 as f32 * ch,
            c.size.0 as f32 * cw,
            c.size.1 as f32 * ch,
        )
    }

    /// Whether the list's rectangle holds `p`.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        contains(self.desc.rect, p)
    }

    /// The cell item under `p`, within the list's rectangle (where it is
    /// drawn, our reading), the last placed on top.
    pub fn cell_at(&self, p: [f32; 2]) -> Option<usize> {
        if !self.contains(p) {
            return None;
        }
        (0..self.cells.len())
            .rev()
            .find(|&i| contains(self.cell_rect(i), p))
    }

    /// The cell item holding item `id`.
    pub fn find(&self, id: u16) -> Option<usize> {
        self.cells.iter().position(|c| c.items.contains(&id))
    }

    /// Which cells are taken, but those of cell item `except`.
    fn taken(&self, except: Option<usize>) -> Vec<bool> {
        let cols = self.desc.cols;
        let mut t = vec![false; cols * self.rows];
        for (i, c) in self.cells.iter().enumerate() {
            if Some(i) == except {
                continue;
            }
            for r in c.cell.1..(c.cell.1 + c.size.1).min(self.rows) {
                for col in c.cell.0..(c.cell.0 + c.size.0).min(cols) {
                    t[r * cols + col] = true;
                }
            }
        }
        t
    }

    /// Whether `size` cells are free at `cell`.
    fn free(&self, taken: &[bool], (c, r): (usize, usize), (w, h): (usize, usize)) -> bool {
        let cols = self.desc.cols;
        c + w <= cols
            && r + h <= self.rows
            && (r..r + h).all(|row| (c..c + w).all(|col| !taken[row * cols + col]))
    }

    /// The first free room for `size` (4.3): rows from the top, in each
    /// the columns from the left.
    fn first_free(&self, size: (usize, usize), except: Option<usize>) -> Option<(usize, usize)> {
        let taken = self.taken(except);
        (0..self.rows)
            .flat_map(|r| (0..self.desc.cols).map(move |c| (c, r)))
            .find(|&at| self.free(&taken, at, size))
    }

    /// The cell a similar cell item `id` joins, when the list groups.
    fn similar_cell(&self, inv: &Inventory, id: u16, except: Option<usize>) -> Option<usize> {
        if !self.desc.group_similar {
            return None;
        }
        let item = inv.item(id)?;
        self.cells.iter().enumerate().position(|(i, c)| {
            Some(i) != except && inv.item(c.item()).is_some_and(|o| similar(o, item))
        })
    }

    /// The cell containing the dragged picture's top-left `at` (absolute),
    /// when inside the grid (4.3).
    pub fn cell_of(&self, at: [f32; 2]) -> Option<(usize, usize)> {
        let (x, y) = self.origin();
        let (cw, ch) = self.desc.cell;
        let c = ((at[0] - x) / cw).trunc();
        let r = ((at[1] - y) / ch).trunc();
        (c >= 0.0 && r >= 0.0 && (c as usize) < self.desc.cols && (r as usize) < self.rows)
            .then_some((c as usize, r as usize))
    }

    /// Places `items` (one cell item's) of grid `size` (4.3): joining a
    /// similar cell item when the list groups; else at cell `at` when given
    /// and free there; else in the first free room, growing the list when
    /// there is none (3.2). `except` is a cell item being moved (its own
    /// room counts as free); it is replaced. A list that may not grow and
    /// has no room even after placing everything again is an error, as in
    /// the original (fatal).
    fn place_items(
        &mut self,
        inv: &Inventory,
        items: Vec<u16>,
        size: (usize, usize),
        at: Option<(usize, usize)>,
        except: Option<usize>,
    ) -> io::Result<()> {
        if let Some(i) = self.similar_cell(inv, items[0], except) {
            self.cells[i].items.extend(items);
            if let Some(e) = except {
                self.cells.remove(e);
            }
            return Ok(());
        }
        let taken = self.taken(except);
        let cell = match at.filter(|&a| self.free(&taken, a, size)) {
            Some(a) => a,
            None => loop {
                if let Some(a) = self.first_free(size, except) {
                    break a;
                }
                if self.desc.unlimited {
                    self.rows += 1;
                    continue;
                }
                // Everything again, one by one in the current order (the
                // original's order there is unestablished).
                let mut all: Vec<(Vec<u16>, (usize, usize))> = Vec::new();
                for (i, c) in std::mem::take(&mut self.cells).into_iter().enumerate() {
                    if Some(i) != except {
                        all.push((c.items, c.size));
                    }
                }
                all.push((items, size));
                for (items, size) in all {
                    let Some(a) = self.first_free(size, None) else {
                        return Err(io::Error::other(format!(
                            "{}: no room for item {}",
                            self.desc.kind.element(),
                            items[0]
                        )));
                    };
                    self.cells.push(CellItem {
                        items,
                        cell: a,
                        size,
                    });
                }
                return Ok(());
            },
        };
        let new = CellItem { items, cell, size };
        match except {
            Some(e) => {
                self.cells.remove(e);
                self.cells.push(new);
            }
            None => self.cells.push(new),
        }
        Ok(())
    }

    /// Places item `id` (4.3), at cell `at` when given.
    pub fn place(
        &mut self,
        inv: &Inventory,
        id: u16,
        at: Option<(usize, usize)>,
    ) -> io::Result<()> {
        let size = grid_cells(inv, id)?;
        self.place_items(inv, vec![id], size, at, None)
    }

    /// Moves cell item `i`, with every item grouped in it, to cell `at` of
    /// this list (5.3 case 2: placement at a position, 4.3).
    pub fn move_cell(
        &mut self,
        inv: &Inventory,
        i: usize,
        at: Option<(usize, usize)>,
    ) -> io::Result<()> {
        let c = self.cells[i].clone();
        self.place_items(inv, c.items, c.size, at, Some(i))
    }

    /// Removes item `id` from its cell item: the cell item goes when it
    /// held only it; otherwise the next grouped item stays in its place
    /// (our choice: one item leaves a group).
    pub fn remove_item(&mut self, id: u16) -> bool {
        let Some(i) = self.find(id) else {
            return false;
        };
        let c = &mut self.cells[i];
        c.items.retain(|&x| x != id);
        if c.items.is_empty() {
            self.cells.remove(i);
        }
        true
    }

    /// Every item shown, in placement order.
    pub fn items(&self) -> impl Iterator<Item = u16> + '_ {
        self.cells.iter().flat_map(|c| c.items.iter().copied())
    }
}

/// Item `id`'s grid size in cells (4.2); an item without the grid keys is
/// an error, as in the original (fatal).
pub fn grid_cells(inv: &Inventory, id: u16) -> io::Result<(usize, usize)> {
    let item = inv
        .item(id)
        .ok_or_else(|| io::Error::other(format!("no item {id} in the inventory")))?;
    let g = item.config.inv_grid.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("[{}] lacks an inv_grid key", item.config.section),
        )
    })?;
    Ok((g.width as usize, g.height as usize))
}
