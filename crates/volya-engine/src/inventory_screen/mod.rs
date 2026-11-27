// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The inventory screen (docs/behavior/inventory-screen.md, cited by
//! section): opened over the running level with the key bound to
//! `inventory` (1), its layout (2, 3, [`layout`]), the item lists with the
//! player's items (3.2, 4.1–4.4, [`list`]), the weight line (4.5), the
//! information panel (4.6), the values updated every frame (4.7, 4.8),
//! selection, double click, drag and drop and the context menu (5,
//! [`menu`]) acting on the player character's inventory, and the sounds (7).
//!
//! **Input.** A mouse screen (6.2) drawing the game's cursor. Touch reaches
//! it through the standard touch controls wherever the cursor shows (#856):
//! the stick moves the cursor, Click clicks and held drags, Right-click
//! opens the context menu; nothing here reads touches directly.
//! A gamepad is a mouse here (#835, [`Input::set_pad_cursor`]): its left
//! stick moves the cursor, south (A / cross) is the left button, held for
//! drags, west (X / square) the right one; east (B / circle) is the key
//! bound to `quit` and north (Y / triangle) the inventory key, which opens
//! the screen in the level (our design).
//!
//! **What waits for other systems** (#111): dropped items are released from
//! the inventory and handed to the caller, which keeps them until the world
//! can place them; weapon addons, unloading and the weapon and artefact
//! parameter blocks need weapons and artefacts; the information portions
//! `ui_inventory` / `ui_inventory_hide` need scripts; the HUD it hides does
//! not exist yet; the game has no pause yet (1.4).

pub mod layout;
pub mod list;
pub mod menu;

use std::io;

use serde::Serialize;

use crate::audio::AudioOut;
use crate::core::audio::Playback;
use crate::core::character::CharacterState;
use crate::core::character::inventory::{Inventory, InventoryItem, Place, SlotMove};
use crate::core::input::{
    Action, Control, GamepadButton, Input, Key, MouseButton, ui_from_normalized,
};
use crate::core::vfs::Vfs;
use crate::data::{GameData, encode};
use crate::dialog::{self, PushButton, Pushed};
use crate::draw::{Ctx, picture_desc};
use crate::save_load::FieldFont;
use crate::scene::{Scene, Sprite};
use layout::{
    BarDesc, GRID, ICON_PIXELS, ICONS, Layout, ListKind, MENU_LINE, PROTECTIONS, Placed,
    WITHOUT_OUTFIT,
};
use list::{ItemList, is_ammunition};
use menu::{ContextMenu, Entry};
use volya_engine_formats::text::layout::TextBox;
use volya_engine_formats::ui::layout::{HAlign, Rect, VAlign};
use volya_engine_formats::ui::picture::Quad;
use volya_engine_formats::ui::{Color, Resolution};

/// Alpha of the dragged picture (5.3).
const DRAG_ALPHA: u8 = 0xAA;
/// The protection lines' height in the scroll view (4.8).
const LINE_HEIGHT: f32 = 15.0;
/// Values closer than this count as equal (4.8).
const EPSILON: f32 = 0.000_01;
/// The image static's width factor on wide screens (4.6).
const WIDE_IMAGE: f32 = 0.753_012_06;
/// The grid texture's left half (3.2): its pixel rectangle.
const GRID_RECT: Rect = Rect::new(0.0, 0.0, 64.0, 64.0);

/// The keys the screen reads (6.1), from the effective bindings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub inventory: Vec<Key>,
    pub drop: Vec<Key>,
    pub quit: Vec<Key>,
}

impl Keys {
    /// Every key the screen uses: kept from the character's controls while
    /// it is open (1.3).
    pub fn used(&self) -> Vec<Control> {
        self.inventory
            .iter()
            .chain(&self.drop)
            .chain(&self.quit)
            .map(|&k| Control::Key(k))
            .collect()
    }
}

/// One frame of the open screen.
pub struct Frame<'a> {
    pub data: &'a GameData,
    pub character: &'a mut CharacterState,
    pub keys: &'a Keys,
    /// The game's time of day, seconds (4.7).
    pub time_of_day: f32,
    pub now: u64,
    pub input: &'a Input,
    pub audio: &'a mut dyn AudioOut,
    /// The context menu's font at the size drawn at, for its width (5.6);
    /// `None` measures every text as 0 wide.
    pub menu_font: Option<&'a FieldFont>,
}

/// What a frame asks of the level.
#[derive(Debug, Default)]
pub struct Outcome {
    /// The screen closed this frame.
    pub closed: bool,
    /// Items dropped this frame, released from the inventory (5.7). Placing
    /// them in the world is dependent.
    pub dropped: Vec<InventoryItem>,
}

/// What the events act with.
struct Act<'a> {
    data: &'a GameData,
    character: &'a mut CharacterState,
    keys: &'a Keys,
    audio: &'a mut dyn AudioOut,
    menu_font: Option<&'a FieldFont>,
    out: Outcome,
}

/// The screen's input events (5.2, 6).
#[derive(Clone, Copy, Debug, PartialEq)]
enum In {
    Move([f32; 2]),
    Press {
        right: bool,
        double: bool,
    },
    Release {
        right: bool,
    },
    Wheel {
        up: bool,
    },
    Key {
        key: Key,
        pressed: bool,
    },
    /// A gamepad's east button: as the key bound to `quit`.
    PadQuit,
    /// A gamepad's north button: as the key bound to `inventory`.
    PadInventory,
}

/// A bar's drawn value (4.7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Bar {
    /// `None` when it was not drawn in the previous frame.
    drawn: Option<f32>,
}

impl Bar {
    /// Sets value `v` of `d` after `dt` seconds: moves towards it by at most
    /// (1 − `inertion`) · range · `dt` when drawn the frame before, else
    /// jumps to it. A bar at 0 or below drew nothing (its length was not
    /// positive), so it jumps.
    fn set(&mut self, d: &BarDesc, v: f32, dt: f32) {
        let v = v.clamp(d.min.min(d.max), d.max.max(d.min));
        self.drawn = Some(match self.drawn {
            Some(old) if (d.max - d.min) * old > 0.0 => {
                let most = (1.0 - d.inertion) * (d.max - d.min).abs() * dt;
                old + (v - old).clamp(-most, most)
            }
            _ => v,
        });
    }
}

/// A drag (5.3).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    list: ListKind,
    item: u16,
    /// From the cursor to the cell item's top-left when it began, whole
    /// units.
    offset: [f32; 2],
}

/// What the information panel shows of the selected item (4.6), filled
/// when the selection changes.
#[derive(Clone, Debug, PartialEq)]
struct Panel {
    item: u16,
    name: String,
    weight: String,
    cost: String,
    /// 0 … 1.
    condition: f32,
    description: String,
    icon: Option<(Rect, (f32, f32))>,
}

/// The inventory screen of a level.
pub struct InventoryScreen {
    layout: Layout,
    open: bool,
    /// In [`ListKind::ALL`] order.
    lists: Vec<ItemList>,
    /// The inventory's item ids at the last rebuild or move (4.3:
    /// ownership changes rebuild).
    known: Vec<u16>,
    selected: Option<u16>,
    panel: Option<Panel>,
    condition_bar: Bar,
    drag: Option<Drag>,
    menu: Option<ContextMenu>,
    exit: PushButton,
    cursor: [f32; 2],
    left_held: bool,
    /// Health, psy, radiation (4.7).
    bars: [Bar; 3],
    /// The protection lines shown, in order (4.8), with their texts.
    lines: Vec<(usize, String)>,
    weight_line: String,
    money: String,
    time: String,
    last_update: Option<u64>,
}

/// Item `id`'s grid icon: its texture rectangle in pixels (4.2).
fn icon_rect(item: &InventoryItem) -> Option<Rect> {
    let g = item.config.inv_grid?;
    Some(Rect::new(
        g.x as f32 * ICON_PIXELS,
        g.y as f32 * ICON_PIXELS,
        g.width as f32 * ICON_PIXELS,
        g.height as f32 * ICON_PIXELS,
    ))
}

/// A sound file of the layout (section 7).
fn play(data: &GameData, audio: &mut dyn AudioOut, sound: &str) {
    if let Ok(path) = data.aliases.path("$game_sounds$", &format!("{sound}.ogg")) {
        audio.play(&path, Playback::default());
    }
}

/// The list item `item` shows in (4.1), if any.
fn list_of(item: &InventoryItem) -> Option<ListKind> {
    match item.place {
        Place::Slot(s) => ListKind::of_slot(s),
        Place::Belt => Some(ListKind::Belt),
        Place::Backpack => Some(ListKind::Bag),
        Place::None => None,
    }
}

fn sorted_ids(inv: &Inventory) -> Vec<u16> {
    let mut ids: Vec<u16> = inv.items.iter().map(|i| i.id).collect();
    ids.sort_unstable();
    ids
}

/// The weight line (4.5).
fn weight_line(data: &GameData, c: &CharacterState) -> String {
    let load = c.load();
    let (m, w) = (load.weight, load.carry_limit);
    let colour = if m > w { "%c[red]" } else { "%c[UI_orange]" };
    format!(
        "%c[default]{} {colour}{m:3.1} %c[UI_orange]/{w:5.1}",
        data.strings.get("ui_inv_weight")
    )
}

/// A float key of a section; missing, it is fatal (ltx-config.md 11).
fn section_float(data: &GameData, section: &str, key: &str) -> io::Result<f32> {
    data.system
        .read(section, key)
        .map(crate::core::config::value::float)
}

/// A text key of a section, unquoted; missing, it is fatal (ltx-config.md
/// 11).
fn read_text<'a>(data: &'a GameData, section: &str, key: &str) -> io::Result<&'a str> {
    data.system
        .read(section, key)
        .map(|v| crate::core::config::value::unquote(v).trim())
}

fn section_text<'a>(data: &'a GameData, section: &str, key: &str) -> Option<&'a str> {
    data.system
        .get(section, key)
        .flatten()
        .map(|v| crate::core::config::value::unquote(v).trim())
}

impl InventoryScreen {
    /// The screen of a level, its layout loaded at `resolution` (section 2).
    pub fn load(tree: &dyn Vfs, data: &GameData, resolution: Resolution) -> io::Result<Self> {
        Ok(Self::new(Layout::load(tree, data, resolution)?))
    }

    pub fn new(layout: Layout) -> Self {
        Self {
            lists: layout.lists.iter().map(|d| ItemList::new(*d)).collect(),
            layout,
            open: false,
            known: Vec::new(),
            selected: None,
            panel: None,
            condition_bar: Bar::default(),
            drag: None,
            menu: None,
            exit: PushButton::default(),
            cursor: [512.0, 384.0],
            left_held: false,
            bars: [Bar::default(); 3],
            lines: Vec::new(),
            weight_line: String::new(),
            money: String::new(),
            time: String::new(),
            last_update: None,
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn list(&self, k: ListKind) -> &ItemList {
        &self.lists[k.index()]
    }

    fn list_mut(&mut self, k: ListKind) -> &mut ItemList {
        &mut self.lists[k.index()]
    }

    /// Opens the screen with the cursor at `cursor` (1.3): its content
    /// rebuilt, the selection cleared, the per-frame values set at once,
    /// `snd_open`.
    pub fn open(
        &mut self,
        data: &GameData,
        character: &CharacterState,
        time_of_day: f32,
        now: u64,
        cursor: [f32; 2],
        audio: &mut dyn AudioOut,
    ) -> io::Result<()> {
        self.rebuild(data, character)?;
        self.open = true;
        self.cursor = cursor;
        self.left_held = false;
        self.drag = None;
        self.menu = None;
        self.exit = PushButton::default();
        self.bars = [Bar::default(); 3];
        // The protection lines keep their order of first appearance for the
        // screen's whole life, the level's (4.8).
        self.last_update = Some(now);
        self.per_frame(data, character, time_of_day, 0.0)?;
        self.exit.frame(self.layout.exit.rect, cursor);
        // TODO(#111): the information portion `ui_inventory` (scripts), the
        // HUD hidden (#101), the pause key (1.4).
        play(data, audio, &self.layout.sounds.open);
        Ok(())
    }

    /// Closes the screen (1.2): `snd_close`.
    pub fn close(&mut self, data: &GameData, audio: &mut dyn AudioOut) {
        if !self.open {
            return;
        }
        self.open = false;
        self.drag = None;
        self.menu = None;
        self.left_held = false;
        // TODO(#111): `ui_inventory_hide`, the HUD restored, the slot of a
        // weapon an addon was attached to made active again (1.3).
        play(data, audio, &self.layout.sounds.close);
    }

    /// Every list emptied and filled from the inventory (4.1, 4.3), the
    /// selection cleared, the weight line set (4.5).
    fn rebuild(&mut self, data: &GameData, c: &CharacterState) -> io::Result<()> {
        let inv = &c.inventory;
        for l in &mut self.lists {
            l.clear();
        }
        let slot = |s: usize| inv.slots[s];
        if let Some(id) = slot(1) {
            self.list_mut(ListKind::Pistol).place(inv, id, None)?;
        }
        if let Some(id) = slot(2) {
            self.list_mut(ListKind::Automatic).place(inv, id, None)?;
        }
        if let Some(id) = slot(6) {
            self.list_mut(ListKind::Outfit).place(inv, id, None)?;
        }
        for &id in &inv.belt {
            self.list_mut(ListKind::Belt).place(inv, id, None)?;
        }
        let mut backpack: Vec<&InventoryItem> = inv
            .items
            .iter()
            .filter(|i| i.place == Place::Backpack)
            .collect();
        backpack.sort_by(|a, b| crate::core::character::inventory::placement_order(a, b));
        for i in backpack {
            self.list_mut(ListKind::Bag).place(inv, i.id, None)?;
        }
        if let Some(id) = slot(3) {
            self.list_mut(ListKind::Bag).place(inv, id, None)?;
        }
        self.known = sorted_ids(inv);
        self.deselect();
        self.weight_line = weight_line(data, c);
        Ok(())
    }

    /// The lists follow the inventory after a move (5.4): items whose place
    /// changed leave their list and are placed in their new one, `moved` at
    /// its drop cell when given; the others stay where they are.
    fn sync(
        &mut self,
        inv: &Inventory,
        moved: Option<(u16, Option<(usize, usize)>)>,
    ) -> io::Result<()> {
        for k in ListKind::ALL {
            let stale: Vec<u16> = self
                .list(k)
                .items()
                .filter(|&id| inv.item(id).and_then(list_of) != Some(k))
                .collect();
            for id in stale {
                self.list_mut(k).remove_item(id);
            }
        }
        for item in &inv.items {
            let Some(k) = list_of(item) else { continue };
            if self.list(k).find(item.id).is_some() {
                continue;
            }
            let at = moved
                .filter(|(id, _)| *id == item.id)
                .and_then(|(_, at)| at);
            self.list_mut(k).place(inv, item.id, at)?;
        }
        self.known = sorted_ids(inv);
        Ok(())
    }

    /// Selects `id` (5.1), filling the panel when the selection changes.
    /// Selects item `id`, or none; filling the panel can fail (4.6).
    fn select(&mut self, data: &GameData, inv: &Inventory, id: Option<u16>) -> io::Result<()> {
        if self.selected == id && (id.is_none() || self.panel.is_some()) {
            return Ok(());
        }
        self.selected = id;
        self.panel = id
            .and_then(|id| inv.item(id))
            .map(|i| self.fill_panel(data, i))
            .transpose()?;
        Ok(())
    }

    /// Clears the selection, which cannot fail.
    fn deselect(&mut self) {
        self.selected = None;
        self.panel = None;
    }

    /// 4.6: `inv_name` and `cost` are read, so a missing one is fatal
    /// (ltx-config.md 11); an absent `description` is an empty text.
    fn fill_panel(&self, data: &GameData, item: &InventoryItem) -> io::Result<Panel> {
        let section = &item.config.section;
        let name = read_text(data, section, "inv_name")?;
        let description = section_text(data, section, "description");
        let cost = crate::core::config::value::int(data.system.read(section, "cost")?);
        Ok(Panel {
            item: item.id,
            name: data.strings.get(name).to_owned(),
            // TODO(#111): a weapon's addons and magazine (4.6).
            weight: format!("{:3.2} kg", item.config.inv_weight),
            cost: format!("{cost} RU"),
            condition: item.condition,
            description: description.map_or(String::new(), |k| data.strings.get(k).to_owned()),
            icon: icon_rect(item).map(|r| (r, (r.width, r.height))),
        })
    }

    /// One frame of the open screen.
    pub fn update(&mut self, f: Frame) -> io::Result<Outcome> {
        let Frame {
            data,
            character,
            keys,
            time_of_day,
            now,
            input,
            audio,
            menu_font,
        } = f;
        let mut a = Act {
            data,
            character,
            keys,
            audio,
            menu_font,
            out: Outcome::default(),
        };
        if !self.open {
            return Ok(a.out);
        }
        // A frame counts 100 ms at most (our choice: a stalled frame doesn't
        // jump the bars' animation).
        let dt = now.saturating_sub(self.last_update.unwrap_or(now)).min(100) as f32 / 1000.0;
        self.last_update = Some(now);
        // 1.2 item 4: the character's death closes it.
        if !a.character.conditions.alive() {
            self.close(a.data, a.audio);
            a.out.closed = true;
            return Ok(a.out);
        }
        // 4.3: an item entered or left the inventory. The rebuild also ends
        // a drag and closes the menu (our choice: their items may be gone).
        if sorted_ids(&a.character.inventory) != self.known {
            self.drag = None;
            self.menu = None;
            self.rebuild(a.data, a.character)?;
        }
        for e in self.events(input) {
            if self.event(e, &mut a)? {
                self.close(a.data, a.audio);
                a.out.closed = true;
                return Ok(a.out);
            }
        }
        self.per_frame(a.data, a.character, time_of_day, dt)?;
        self.exit.frame(self.layout.exit.rect, self.cursor);
        Ok(a.out)
    }

    /// This frame's events (5.2, 6; the gamepad's mapping is our design).
    fn events(&self, input: &Input) -> Vec<In> {
        let mut out = Vec::new();
        for a in input.actions() {
            match *a {
                Action::Cursor { to } => out.push(In::Move(ui_from_normalized(to))),
                Action::Press {
                    control: Control::Mouse(b @ (MouseButton::Left | MouseButton::Right)),
                    double,
                } => out.push(In::Press {
                    right: b == MouseButton::Right,
                    double,
                }),
                Action::Release {
                    control: Control::Mouse(b @ (MouseButton::Left | MouseButton::Right)),
                } => out.push(In::Release {
                    right: b == MouseButton::Right,
                }),
                Action::Wheel { up } => out.push(In::Wheel { up }),
                _ => {}
            }
        }
        for &(c, pressed) in input.device_changes() {
            if let Control::Key(key) = c {
                out.push(In::Key { key, pressed });
            }
        }
        for &(b, pressed) in input.pad_changes() {
            out.extend(match b {
                GamepadButton::East => pressed.then_some(In::PadQuit),
                GamepadButton::North => pressed.then_some(In::PadInventory),
                _ => None,
            });
        }
        out
    }

    /// The gamepad cursor's targets ([`Input::pad_cursor_frame`]): the
    /// menu's rows while it is open, else every cell item shown and the
    /// exit button.
    pub fn cursor_targets(&self) -> Vec<Rect> {
        if let Some(m) = &self.menu {
            return (0..m.entries.len())
                .map(|i| m.row_rect(i, self.layout.menu.top_indent))
                .collect();
        }
        let mut t = Vec::new();
        for l in &self.lists {
            for i in 0..l.cells.len() {
                let r = l.cell_rect(i);
                let lr = l.desc.rect;
                // Its part inside the list, if any.
                let x0 = r.x.max(lr.x);
                let y0 = r.y.max(lr.y);
                let x1 = (r.x + r.width).min(lr.x + lr.width);
                let y1 = (r.y + r.height).min(lr.y + lr.height);
                if x1 > x0 && y1 > y0 {
                    t.push(Rect::new(x0, y0, x1 - x0, y1 - y0));
                }
            }
        }
        t.push(self.layout.exit.rect);
        t
    }

    /// The list and cell item under `p`.
    fn cell_at(&self, p: [f32; 2]) -> Option<(ListKind, usize)> {
        ListKind::ALL
            .into_iter()
            .find_map(|k| self.list(k).cell_at(p).map(|i| (k, i)))
    }

    /// The list whose rectangle holds `p` (5.3: the target list).
    fn list_at(&self, p: [f32; 2]) -> Option<ListKind> {
        ListKind::ALL
            .into_iter()
            .find(|&k| self.list(k).contains(p))
    }

    /// One event; returns whether the screen closes.
    fn event(&mut self, e: In, a: &mut Act) -> io::Result<bool> {
        match e {
            In::Key { key, pressed } => {
                // 5.6: any key press or release closes the menu; the key is
                // then handled as usual (6.1).
                self.menu = None;
                if !pressed {
                    return Ok(false);
                }
                if a.keys.inventory.contains(&key) || a.keys.quit.contains(&key) {
                    return Ok(true);
                }
                if a.keys.drop.contains(&key)
                    && let Some(id) = self.selected
                {
                    self.drop(a, id);
                }
            }
            In::PadQuit | In::PadInventory => return Ok(true),
            In::Move(p) => self.moved(p, a)?,
            // `double`: the input's rule (title-menu.md 4.4): a second press
            // of the button within 250 ms of the previous single press; a
            // third starts a new pair.
            In::Press { right, double } => return self.press(right, double, a),
            In::Release { right } => return self.release(right, a),
            In::Wheel { up } => {
                if self.menu.is_none()
                    && let Some(k) = self.list_at(self.cursor)
                    && self.list(k).bar_shown()
                {
                    self.list_mut(k).scroll_by(if up { -1 } else { 1 });
                }
            }
        }
        Ok(false)
    }

    fn moved(&mut self, p: [f32; 2], a: &mut Act) -> io::Result<()> {
        self.cursor = p;
        if let Some(m) = &mut self.menu {
            m.hovered = m.row_at(p, self.layout.menu.top_indent).or(m.hovered);
            return Ok(());
        }
        let r = self.layout.exit.rect;
        if self.exit.receives(r, p) {
            self.exit.frame(r, p);
            self.exit.event(dialog::Ev::Move(p));
        }
        // 5.2: moving with the left button held over a cell item selects it
        // and starts dragging it.
        if self.left_held
            && self.drag.is_none()
            && let Some((k, i)) = self.cell_at(p)
        {
            let id = self.list(k).cells[i].item();
            let r = self.list(k).cell_rect(i);
            self.select(a.data, &a.character.inventory, Some(id))?;
            self.drag = Some(Drag {
                list: k,
                item: id,
                offset: [(r.x - p[0]).trunc(), (r.y - p[1]).trunc()],
            });
        }
        Ok(())
    }

    fn press(&mut self, right: bool, double: bool, a: &mut Act) -> io::Result<bool> {
        let p = self.cursor;
        if let Some(m) = self.menu.take() {
            // 5.6: the menu takes all mouse input. A press inside it but on
            // no row (its frame's margin) closes it like one outside (our
            // choice: the spec names presses on rows and outside).
            if !right && let Some(row) = m.row_at(p, self.layout.menu.top_indent) {
                self.choose(a, m.item, m.entries[row])?;
            }
            return Ok(false);
        }
        if !right {
            self.left_held = true;
            let r = self.layout.exit.rect;
            // As for the boxes (our choice, for taps).
            self.exit.frame(r, p);
            if self.exit.receives(r, p) {
                self.exit.event(dialog::Ev::Press { double });
            }
        }
        let Some((k, i)) = self.cell_at(p) else {
            return Ok(false);
        };
        let cell = &self.list(k).cells[i];
        let (id, grouped) = (cell.item(), cell.items.len());
        self.select(a.data, &a.character.inventory, Some(id))?;
        if right {
            self.open_menu(a, id, grouped);
        } else if double {
            self.double_click(a, k, id)?;
        }
        Ok(false)
    }

    fn release(&mut self, right: bool, a: &mut Act) -> io::Result<bool> {
        if right {
            return Ok(false);
        }
        self.left_held = false;
        if let Some(d) = self.drag.take() {
            self.end_drag(a, d)?;
        }
        let r = self.layout.exit.rect;
        // 6.2: activated on release over it.
        Ok(self.exit.receives(r, self.cursor)
            && self.exit.event(dialog::Ev::Release) == Pushed::Activated)
    }

    /// The context menu for item `id` (5.6), when any entry applies.
    fn open_menu(&mut self, a: &mut Act, id: u16, grouped: usize) {
        let entries = menu::entries(&a.character.inventory, id, grouped);
        if entries.is_empty() {
            return;
        }
        let widest = entries
            .iter()
            .map(|e| {
                a.menu_font
                    .map_or(0.0, |f| f.width(a.data.strings.get(e.key())))
            })
            .fold(0.0, f32::max);
        let (w, h) = menu::size(&self.layout.menu, entries.len(), widest);
        let (x, y) = menu::position((self.cursor[0], self.cursor[1]), (w, h));
        let mut m = ContextMenu {
            item: id,
            entries,
            rect: Rect::new(x, y, w, h),
            hovered: None,
        };
        m.hovered = m.row_at(self.cursor, self.layout.menu.top_indent);
        self.menu = Some(m);
        play(a.data, a.audio, &self.layout.sounds.properties);
    }

    /// A chosen menu row's action (5.6) on item `id`.
    fn choose(&mut self, a: &mut Act, id: u16, entry: Entry) -> io::Result<()> {
        match entry {
            Entry::MoveToSlot | Entry::DressOutfit => {
                self.move_to_slot(a, id, true)?;
            }
            Entry::MoveOnBelt => {
                self.move_to_belt(a, id, None)?;
            }
            Entry::MoveToBag | Entry::UndressOutfit => {
                self.move_to_backpack(a, id, None)?;
            }
            Entry::Use | Entry::Drink | Entry::Eat => self.use_item(a, id)?,
            Entry::Drop => self.drop(a, id),
            Entry::DropAll => {
                // 5.7: every item grouped in the cell, then its own.
                let items = ListKind::ALL
                    .iter()
                    .find_map(|&k| {
                        let l = self.list(k);
                        l.find(id).map(|i| l.cells[i].items.clone())
                    })
                    .unwrap_or_else(|| vec![id]);
                for &other in items.iter().skip(1).chain(items.first()) {
                    self.drop(a, other);
                }
            }
        }
        Ok(())
    }

    /// A double click on item `id` in list `k` (5.5).
    fn double_click(&mut self, a: &mut Act, k: ListKind, id: u16) -> io::Result<()> {
        let usable = a
            .character
            .inventory
            .item(id)
            .is_some_and(|i| i.config.is_eatable());
        if usable {
            return self.use_item(a, id);
        }
        if k == ListKind::Bag {
            if !self.move_to_slot(a, id, false)?.succeeded() && !self.move_to_belt(a, id, None)? {
                self.move_to_slot(a, id, true)?;
            }
            return Ok(());
        }
        self.move_to_backpack(a, id, None)?;
        Ok(())
    }

    /// A drag ends with a left release (5.3).
    fn end_drag(&mut self, a: &mut Act, d: Drag) -> io::Result<()> {
        let p = self.cursor;
        let Some(t) = self.list_at(p) else {
            return Ok(());
        };
        let s = d.list;
        let picture = [p[0] + d.offset[0], p[1] + d.offset[1]];
        let at = self.list(t).cell_of(picture);
        if t == s {
            if self.list(t).desc.custom_placement
                && let Some(i) = self.list(t).find(d.item)
            {
                let inv = &a.character.inventory;
                self.lists[t.index()].move_cell(inv, i, at)?;
            }
            return Ok(());
        }
        if s.is_slot() && t.is_slot() {
            return Ok(());
        }
        let inv = &a.character.inventory;
        match t {
            ListKind::Bag => {
                self.move_to_backpack(a, d.item, at)?;
            }
            ListKind::Belt => {
                self.move_to_belt(a, d.item, at)?;
            }
            _ => {
                let own = inv
                    .item(d.item)
                    .and_then(|i| i.config.slot_number())
                    .filter(|&s| !inv.is_persistent(s))
                    .and_then(ListKind::of_slot);
                if own == Some(t) {
                    self.move_to_slot(a, d.item, true)?;
                }
            }
        }
        // Cases 4–6: then the further effect.
        let eatable = a
            .character
            .inventory
            .item(d.item)
            .is_some_and(|i| i.config.is_eatable());
        if t == ListKind::Outfit && eatable {
            self.use_item(a, d.item)?;
        }
        // TODO(#111): otherwise an addon dropped on a weapon that can take
        // it is attached (5.8).
        Ok(())
    }

    /// **To backpack** (5.4), at cell `at` when given.
    fn move_to_backpack(
        &mut self,
        a: &mut Act,
        id: u16,
        at: Option<(usize, usize)>,
    ) -> io::Result<bool> {
        if !a.character.inventory.move_to_backpack(id) {
            return Ok(false);
        }
        self.sync(&a.character.inventory, Some((id, at)))?;
        play(a.data, a.audio, &self.layout.sounds.item_to_ruck);
        Ok(true)
    }

    /// **To belt** (5.4), at cell `at` when given.
    fn move_to_belt(
        &mut self,
        a: &mut Act,
        id: u16,
        at: Option<(usize, usize)>,
    ) -> io::Result<bool> {
        if !a.character.inventory.move_to_belt(id) {
            return Ok(false);
        }
        self.sync(&a.character.inventory, Some((id, at)))?;
        play(a.data, a.audio, &self.layout.sounds.item_to_belt);
        Ok(true)
    }

    /// **To slot**, with swap when `swap` (5.4).
    fn move_to_slot(&mut self, a: &mut Act, id: u16, swap: bool) -> io::Result<SlotMove> {
        let inv = &mut a.character.inventory;
        let old = inv
            .item(id)
            .and_then(|i| i.config.slot_number())
            .and_then(|s| inv.slots[usize::from(s)])
            .filter(|&o| o != id);
        let moved = if swap {
            inv.move_to_slot_with_swap(id)
        } else {
            inv.move_to_slot(id)
        };
        if moved != SlotMove::Moved {
            return Ok(moved);
        }
        // The swap moved the slot's item to the backpack first.
        let swapped = old.is_some_and(|o| inv.item(o).is_some_and(|i| i.place == Place::Backpack));
        self.sync(&a.character.inventory, Some((id, None)))?;
        if swapped {
            play(a.data, a.audio, &self.layout.sounds.item_to_ruck);
        }
        play(a.data, a.audio, &self.layout.sounds.item_to_slot);
        Ok(moved)
    }

    /// **Use** (5.7): the selection cleared; a usable item is eaten
    /// (player-state.md section 5) with `snd_item_use`.
    fn use_item(&mut self, a: &mut Act, id: u16) -> io::Result<()> {
        self.deselect();
        if a.character
            .inventory
            .item(id)
            .is_some_and(InventoryItem::usable)
        {
            a.character.eat(id)?;
            play(a.data, a.audio, &self.layout.sounds.item_use);
            if a.character.inventory.item(id).is_none() {
                self.forget(id);
            }
        }
        Ok(())
    }

    /// An item gone from the inventory leaves the lists at once; the next
    /// frame rebuilds them (4.3).
    fn forget(&mut self, id: u16) {
        for l in &mut self.lists {
            l.remove_item(id);
        }
    }

    /// **Drop** (5.7): released from the inventory, `snd_drop_item`, the
    /// selection cleared and the weight line updated; nothing for a quest
    /// item.
    fn drop(&mut self, a: &mut Act, id: u16) {
        self.menu = None;
        let Some(item) = a.character.drop_item(id) else {
            return;
        };
        a.out.dropped.push(item);
        self.forget(id);
        play(a.data, a.audio, &self.layout.sounds.drop_item);
        self.deselect();
        self.weight_line = weight_line(a.data, a.character);
    }

    /// The per-frame values (4.7, 4.8).
    /// The outfit's protection keys are read (4.8): a missing one is fatal
    /// (ltx-config.md 11).
    fn per_frame(
        &mut self,
        data: &GameData,
        c: &CharacterState,
        time_of_day: f32,
        dt: f32,
    ) -> io::Result<()> {
        let cond = &c.conditions;
        let values = [cond.health, cond.psy_health, cond.radiation];
        for (i, v) in values.into_iter().enumerate() {
            let d = self.layout.bars[i].clone();
            self.bars[i].set(&d, v * 100.0, dt);
        }
        self.money = format!("{} RU", c.profile.money);
        let t = time_of_day.max(0.0) as u64;
        self.time = format!("{:02}:{:02}", (t / 3600) % 24, (t / 60) % 60);
        // 4.8.
        let outfit = c.inventory.worn_outfit();
        for (k, (label, key)) in PROTECTIONS.iter().enumerate() {
            let p = match outfit {
                Some(o) => section_float(data, &o.config.section, key)? * o.condition,
                None => 0.0,
            };
            // TODO(#111): belt artefacts' protection factor; 1 without them.
            let a = 1.0_f32;
            let shown = p.abs() >= EPSILON || (1.0 - a).abs() >= EPSILON;
            let at = self.lines.iter().position(|(l, _)| *l == k);
            if !shown {
                if let Some(i) = at {
                    self.lines.remove(i);
                }
                continue;
            }
            let sign = |v: f32| if v > 0.0 { "%c[green]" } else { "%c[red]" };
            let mut text = format!(
                "{} {} {:+3.0}%",
                data.strings.get(label),
                sign(p),
                p * 100.0
            );
            if (1.0 - a).abs() >= EPSILON {
                text.push_str(&format!(" {} {:+3.0}%", sign(1.0 - a), (1.0 - a) * 100.0));
            }
            match at {
                Some(i) => self.lines[i].1 = text,
                None => self.lines.push((k, text)),
            }
        }
        // The panel's condition bar (4.6): drawn while an item is selected.
        match &self.panel {
            Some(p) => {
                let d = self.layout.info.condition_bar.clone();
                self.condition_bar
                    .set(&d, p.condition * 100.0 + 1.0 - 0.000_01, dt);
            }
            None => self.condition_bar = Bar::default(),
        }
        Ok(())
    }

    /// Draws the open screen over the level (3.1: its elements in their
    /// attachment order, the dragged picture over them, then the cursor).
    pub fn draw(&self, ctx: &mut Ctx, s: &mut Scene, c: &CharacterState) -> io::Result<()> {
        if !self.open {
            return Ok(());
        }
        let l = &self.layout;
        draw_static(ctx, s, "belt_slots", &l.belt_slots, None)?;
        draw_static(ctx, s, "back", &l.back, None)?;
        draw_static(ctx, s, "bottom_static", &l.bottom_static, None)?;
        draw_static(ctx, s, "bag_static", &l.bag_static, Some(&self.weight_line))?;
        draw_static(ctx, s, "money_static", &l.money_static, Some(&self.money))?;
        draw_static(ctx, s, "descr_static", &l.descr_static, None)?;
        self.draw_panel(ctx, s, c)?;
        draw_static(ctx, s, "character_frame_window title", &l.frame_title, None)?;
        draw_static(ctx, s, "progress_background", &l.progress_background, None)?;
        for (i, name) in ["health", "psy", "radiation"].iter().enumerate() {
            draw_bar(
                ctx,
                s,
                &format!("progress_bar_{name}"),
                &l.bars[i],
                self.bars[i].drawn,
            )?;
        }
        self.draw_outfit_info(ctx, s)?;
        for k in ListKind::ALL {
            self.draw_list(ctx, s, c, k)?;
        }
        self.draw_menu(ctx, s)?;
        draw_static(ctx, s, "time_static", &l.time_static, None)?;
        draw_static(ctx, s, "time_static_str", &l.time_str, Some(&self.time))?;
        dialog::draw_button(
            ctx,
            s,
            "exit_button",
            &l.exit.desc,
            l.exit.rect,
            self.exit.look(),
        )?;
        // 5.3: the dragged picture follows the cursor (believed: above the
        // screen); the cell it left stays drawn (unestablished; our choice).
        if let Some(d) = &self.drag
            && let Some(item) = c.inventory.item(d.item)
            && let Some(i) = self.list(d.list).find(d.item)
        {
            let r = self.list(d.list).cell_rect(i);
            let at = Rect::new(
                self.cursor[0] + d.offset[0],
                self.cursor[1] + d.offset[1],
                r.width,
                r.height,
            );
            draw_icon(
                ctx,
                s,
                "drag",
                item,
                at,
                Color::rgba(255, 255, 255, DRAG_ALPHA),
            )?;
        }
        crate::draw::cursor(ctx, s, self.cursor)
    }

    /// The information panel (4.6) while an item is selected.
    fn draw_panel(&self, ctx: &mut Ctx, s: &mut Scene, c: &CharacterState) -> io::Result<()> {
        let Some(p) = &self.panel else {
            return Ok(());
        };
        let i = &self.layout.info;
        draw_static(ctx, s, "panel name", &i.name, Some(&p.name))?;
        draw_static(ctx, s, "panel weight", &i.weight, Some(&p.weight))?;
        draw_static(ctx, s, "panel cost", &i.cost, Some(&p.cost))?;
        draw_static(ctx, s, "panel condition", &i.condition, None)?;
        draw_bar(
            ctx,
            s,
            "panel condition_progress",
            &i.condition_bar,
            self.condition_bar.drawn,
        )?;
        // The description at x 0 of its scroll view, wrapped at the view's
        // width less `left_ident`. TODO(#111): its scroll bar (a longer text
        // is cut), and the weapon and artefact parameter blocks above it.
        if !p.description.is_empty() {
            let r = i.descr_list;
            let start = s.sprites.len();
            let b = TextBox {
                rect: Rect::new(r.x, r.y, r.width - i.descr_left_ident, 10_000.0),
                offset: (0.0, 0.0),
                align: HAlign::Left,
                vert_align: VAlign::Top,
                complex: true,
                color: i.descr_color,
                line_gap: 0.0,
            };
            ctx.text(
                s,
                "panel description",
                &encode(&p.description),
                &b,
                i.descr_font,
            )?;
            s.clip_from(start, pixels(r, ctx.scale()));
        }
        if let (Some((_, (w, h))), Some(item)) = (p.icon, c.inventory.item(p.item)) {
            let w = if ctx.resolution.is_wide() {
                w * WIDE_IMAGE
            } else {
                w
            };
            let (w, h) = (w.min(i.image_max.0), h.min(i.image_max.1));
            let (cx, cy) = i.image_centre;
            let r = Rect::new(cx - w / 2.0, cy - h / 2.0, w, h);
            draw_icon(ctx, s, "panel image", item, r, Color::WHITE)?;
        }
        Ok(())
    }

    /// `outfit_info` (4.8): the caption and the protection lines stacked
    /// from the top of the scroll view.
    fn draw_outfit_info(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let l = &self.layout;
        draw_static(ctx, s, "outfit_info caption", &l.outfit_caption, None)?;
        let v = l.outfit_view;
        let start = s.sprites.len();
        for (row, (k, text)) in self.lines.iter().enumerate() {
            let desc = &l.protections[*k];
            let rect = Rect::new(
                v.x,
                v.y + row as f32 * LINE_HEIGHT,
                desc.window.width,
                desc.window.height,
            );
            let placed = Placed {
                desc: desc.clone(),
                rect,
            };
            draw_static(
                ctx,
                s,
                &format!("outfit_info line {k}"),
                &placed,
                Some(text),
            )?;
        }
        s.clip_from(start, pixels(v, ctx.scale()));
        Ok(())
    }

    /// An item list (3.2, 4.2, 4.4): its cells' backgrounds and cell items,
    /// clipped to it, and its scroll bar while shown.
    fn draw_list(
        &self,
        ctx: &mut Ctx,
        s: &mut Scene,
        c: &CharacterState,
        k: ListKind,
    ) -> io::Result<()> {
        let list = self.list(k);
        let name = k.element();
        let start = s.sprites.len();
        let rect = list.desc.rect;
        if k == ListKind::Outfit {
            // 4.4: the character picture, nothing else.
            // The worn outfit's `full_icon_name` is read: missing, it is
            // fatal (ltx-config.md 11).
            let id = match c.inventory.worn_outfit() {
                Some(o) => read_text(ctx.data, &o.config.section, "full_icon_name")?,
                None => WITHOUT_OUTFIT,
            }
            .to_owned();
            ctx.part_stretched(s, &format!("{name} picture"), &id, rect)?;
            return Ok(());
        }
        // Cell backgrounds over the visible part of the grid block.
        let grid = ctx.assets.texture(ctx.tree, ctx.data, GRID, ctx.clock_ms)?;
        let (gw, gh) = grid.size();
        let texture = grid.id(0, false);
        let (ox, oy) = list.origin();
        let (cw, ch) = list.desc.cell;
        let (sx, sy) = ctx.scale();
        for r in 0..list.rows {
            let y = oy + r as f32 * ch;
            if y + ch <= rect.y || y >= rect.y + rect.height {
                continue;
            }
            for col in 0..list.desc.cols {
                let x = ox + col as f32 * cw;
                let q = Quad {
                    min: ((x * sx).round() - 0.5, (y * sy).round() - 0.5),
                    max: (((x + cw) * sx).round() - 0.5, ((y + ch) * sy).round() - 0.5),
                    uv_min: (GRID_RECT.x / gw as f32, GRID_RECT.y / gh as f32),
                    uv_max: (
                        (GRID_RECT.x + GRID_RECT.width) / gw as f32,
                        (GRID_RECT.y + GRID_RECT.height) / gh as f32,
                    ),
                    color: Color::WHITE,
                };
                s.sprites
                    .push(Sprite::picture(format!("{name} cell"), texture.clone(), &q));
            }
        }
        for (i, cell) in list.cells.iter().enumerate() {
            let Some(item) = c.inventory.item(cell.item()) else {
                continue;
            };
            let r = list.cell_rect(i);
            draw_icon(ctx, s, &format!("{name} item"), item, r, Color::WHITE)?;
            draw_addons(ctx, s, &format!("{name} addon"), item, r)?;
            let count = if is_ammunition(item) {
                // Each box's rounds start at its `box_size` (how they change
                // is dependent).
                let rounds: i64 = cell
                    .items
                    .iter()
                    .filter_map(|&id| c.inventory.item(id))
                    .map(|b| {
                        ctx.data
                            .system
                            .get(&b.config.section, "box_size")
                            .flatten()
                            .map_or(0, crate::core::config::value::int)
                    })
                    .sum();
                Some(rounds.to_string())
            } else if cell.items.len() >= 2 {
                Some(format!("x{}", cell.items.len()))
            } else {
                None
            };
            if let Some(text) = count {
                let b = TextBox {
                    rect: r,
                    offset: (0.0, 0.0),
                    align: HAlign::Left,
                    vert_align: VAlign::Top,
                    complex: false,
                    color: Color::WHITE,
                    line_gap: 0.0,
                };
                ctx.text(
                    s,
                    &format!("{name} count"),
                    text.as_bytes(),
                    &b,
                    volya_engine_formats::text::font::UiFont::DEFAULT,
                )?;
            }
        }
        s.clip_from(start, pixels(rect, ctx.scale()));
        // TODO(#111): the bar's arrows and box; the wheel scrolls (3.2).
        if list.bar_shown() {
            let p = &self.layout.scroll;
            let mut bar =
                crate::options::widgets::ScrollBar::new(list.block_height(), rect.height, p.width);
            bar.set(list.scroll as i32);
            let br = Rect::new(rect.x + rect.width - p.width, rect.y, p.width, rect.height);
            let layer = format!("{name} bar");
            ctx.part_stretched(s, &layer, &p.track, br)?;
            let b = Rect::new(br.x, br.y + bar.box_top(), br.width, bar.box_length());
            ctx.part_stretched(s, &layer, &p.thumb, b)?;
            ctx.part(s, &layer, &p.up, (br.x, br.y), None, Color::WHITE)?;
            let arrow = crate::options::widgets::ARROW;
            ctx.part(
                s,
                &layer,
                &p.down,
                (br.x, br.y + br.height - arrow),
                None,
                Color::WHITE,
            )?;
        }
        Ok(())
    }

    /// The context menu (5.6): a frame window, its rows, the hovered one
    /// highlighted in `font_s`'s colour.
    fn draw_menu(&self, ctx: &mut Ctx, s: &mut Scene) -> io::Result<()> {
        let Some(m) = &self.menu else {
            return Ok(());
        };
        let d = &self.layout.menu;
        ctx.frame_window(s, "menu", &d.texture, m.rect)?;
        for (i, e) in m.entries.iter().enumerate() {
            let r = m.row_rect(i, d.top_indent);
            let hovered = m.hovered == Some(i);
            // The text and highlight placement within the row are
            // unestablished (believed as in the save list): our choice is
            // the highlight inside the frame, the text at `right_ident` (the
            // items' x, ui-layout.md 9.1), centred vertically.
            if hovered {
                ctx.frame_line(
                    s,
                    "menu selected",
                    MENU_LINE,
                    (r.x, r.y + 1.0),
                    m.rect.width - 2.0 * layout::MENU_LIST_OFFSET.0,
                )?;
            }
            let b = TextBox {
                rect: Rect::new(r.x + d.right_ident, r.y, r.width - d.right_ident, r.height),
                offset: (0.0, 0.0),
                align: HAlign::Left,
                vert_align: VAlign::Centre,
                complex: false,
                color: if hovered { d.selected_color } else { d.color },
                line_gap: 0.0,
            };
            ctx.text(
                s,
                &format!("menu row {i}"),
                &ctx.data.text(e.key()),
                &b,
                d.font,
            )?;
        }
        Ok(())
    }

    /// The screen as the engine interface reports it.
    pub fn state(&self, c: &CharacterState) -> InventoryState {
        InventoryState {
            cursor: self.cursor,
            lists: ListKind::ALL
                .iter()
                .map(|&k| {
                    let l = self.list(k);
                    ListState {
                        list: k,
                        rect: rect4(l.desc.rect),
                        rows: l.rows,
                        scroll: l.scroll,
                        scroll_bar: l.bar_shown(),
                        cells: (0..l.cells.len())
                            .map(|i| {
                                let cell = &l.cells[i];
                                CellState {
                                    items: cell.items.clone(),
                                    section: c
                                        .inventory
                                        .item(cell.item())
                                        .map(|i| i.config.section.clone())
                                        .unwrap_or_default(),
                                    cell: [cell.cell.0, cell.cell.1],
                                    rect: rect4(l.cell_rect(i)),
                                }
                            })
                            .collect(),
                    }
                })
                .collect(),
            selected: self.selected,
            dragging: self.drag.map(|d| d.item),
            menu: self.menu.as_ref().map(|m| MenuState {
                item: m.item,
                entries: m.entries.iter().map(|e| e.key()).collect(),
                rect: rect4(m.rect),
                rows: (0..m.entries.len())
                    .map(|i| rect4(m.row_rect(i, self.layout.menu.top_indent)))
                    .collect(),
                hovered: m.hovered,
            }),
            weight_line: self.weight_line.clone(),
            money: self.money.clone(),
            time: self.time.clone(),
            bars: self.bars.map(|b| b.drawn.unwrap_or(0.0)),
            protections: self.lines.iter().map(|(_, t)| t.clone()).collect(),
            panel: self.panel.as_ref().map(|p| PanelState {
                item: p.item,
                name: p.name.clone(),
                weight: p.weight.clone(),
                cost: p.cost.clone(),
            }),
            exit: rect4(self.layout.exit.rect),
            exit_look: self.exit.look(),
        }
    }
}

fn rect4(r: Rect) -> [f32; 4] {
    [r.x, r.y, r.width, r.height]
}

/// A UI rectangle in output pixels, in the scene's convention.
fn pixels(r: Rect, (sx, sy): (f32, f32)) -> [f32; 4] {
    [
        (r.x * sx).floor() - 0.5,
        (r.y * sy).floor() - 0.5,
        ((r.x + r.width) * sx).floor() - 0.5,
        ((r.y + r.height) * sy).floor() - 0.5,
    ]
}

/// A static's picture and its text: `text` (raw, complex codes allowed)
/// in place of its key's translation when given.
fn draw_static(
    ctx: &mut Ctx,
    s: &mut Scene,
    layer: &str,
    p: &Placed,
    text: Option<&str>,
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
    let (b, font) = TextBox::of_static(&p.desc, p.rect);
    ctx.text(s, &format!("{layer} text"), &bytes, &b, font)
}

/// A bar (4.7) at drawn value `v`.
fn draw_bar(
    ctx: &mut Ctx,
    s: &mut Scene,
    layer: &str,
    d: &BarDesc,
    v: Option<f32>,
) -> io::Result<()> {
    let Some(v) = v else {
        return Ok(());
    };
    let mut range = d.max - d.min;
    if range < 0.000_01 {
        range += 0.000_01;
    }
    let r = d.rect;
    let total = if d.horizontal { r.width } else { r.height };
    let len = total * v / range;
    if len <= 0.0 {
        return Ok(());
    }
    let clip = if d.horizontal {
        Rect::new(r.x, r.y, len, r.height)
    } else {
        Rect::new(r.x, r.y + r.height - len, r.width, len)
    };
    let color = match (d.min_color, d.max_color) {
        (Some(a), Some(b)) => {
            let t = (v / range).clamp(0.0, 1.0);
            let mix =
                |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
            Some(Color::rgba(
                mix(a.r, b.r),
                mix(a.g, b.g),
                mix(a.b, b.b),
                mix(a.a, b.a),
            ))
        }
        _ => None,
    };
    let start = s.sprites.len();
    ctx.picture(s, layer, &d.progress, r, 0.0, 0, false, color)?;
    s.clip_from(start, pixels(clip, ctx.scale()));
    Ok(())
}

/// Item `item`'s icon (4.2) stretched over `rect`.
fn draw_icon(
    ctx: &mut Ctx,
    s: &mut Scene,
    layer: &str,
    item: &InventoryItem,
    rect: Rect,
    color: Color,
) -> io::Result<()> {
    let Some(tr) = icon_rect(item) else {
        return Ok(());
    };
    let mut desc = picture_desc(ICONS, rect, Some(tr), color);
    desc.stretch = true;
    ctx.picture(s, layer, &desc, rect, 0.0, 0, false, None)
}

/// A weapon's attachable addons that are installed, drawn over its icon
/// (4.2): silencer, scope, grenade launcher, each the icon of the section
/// its `*_name` names (believed) at its `*_x`, `*_y` (icon pixels, scaled to
/// the cell item).
fn draw_addons(
    ctx: &mut Ctx,
    s: &mut Scene,
    layer: &str,
    item: &InventoryItem,
    rect: Rect,
) -> io::Result<()> {
    use crate::core::character::inventory::{ADDON_LAUNCHER, ADDON_SCOPE, ADDON_SILENCER};
    if !item.config.is_weapon() {
        return Ok(());
    }
    let Some(icon) = icon_rect(item) else {
        return Ok(());
    };
    let installed = list::addons(item);
    let c = &item.config;
    let (kx, ky) = (rect.width / icon.width, rect.height / icon.height);
    for (status, bit, name) in [
        (c.silencer_status, ADDON_SILENCER, "silencer"),
        (c.scope_status, ADDON_SCOPE, "scope"),
        (
            c.grenade_launcher_status,
            ADDON_LAUNCHER,
            "grenade_launcher",
        ),
    ] {
        if status != 2 || installed & bit == 0 {
            continue;
        }
        let section = &c.section;
        // 4.2: the weapon's `*_name`, `*_x` and `*_y` are read for an
        // installed attachable addon: a missing one is fatal (ltx-config.md
        // 11).
        let addon = read_text(ctx.data, section, &format!("{name}_name"))?;
        // 4.2: every item section has the grid keys; one missing is fatal.
        let grid = |key: &str| {
            ctx.data
                .system
                .get(addon, key)
                .flatten()
                .map(|v| crate::core::config::value::int(v) as u32 as f32)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("[{addon}] lacks {key}"))
                })
        };
        let (gx, gy, gw, gh) = (
            grid("inv_grid_x")?,
            grid("inv_grid_y")?,
            grid("inv_grid_width")?,
            grid("inv_grid_height")?,
        );
        let x = section_float(ctx.data, section, &format!("{name}_x"))?;
        let y = section_float(ctx.data, section, &format!("{name}_y"))?;
        let tr = Rect::new(
            gx * ICON_PIXELS,
            gy * ICON_PIXELS,
            gw * ICON_PIXELS,
            gh * ICON_PIXELS,
        );
        let at = Rect::new(
            rect.x + x * kx,
            rect.y + y * ky,
            tr.width * kx,
            tr.height * ky,
        );
        let mut desc = picture_desc(ICONS, at, Some(tr), Color::WHITE);
        desc.stretch = true;
        ctx.picture(s, layer, &desc, at, 0.0, 0, false, None)?;
    }
    Ok(())
}

/// One cell item as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct CellState {
    /// Its own item first, then the grouped ones.
    pub items: Vec<u16>,
    pub section: String,
    pub cell: [usize; 2],
    /// Absolute (x, y, width, height).
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, Serialize)]
pub struct ListState {
    pub list: ListKind,
    pub rect: [f32; 4],
    pub rows: usize,
    pub scroll: f32,
    pub scroll_bar: bool,
    pub cells: Vec<CellState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MenuState {
    pub item: u16,
    /// The entries' string keys.
    pub entries: Vec<&'static str>,
    pub rect: [f32; 4],
    pub rows: Vec<[f32; 4]>,
    pub hovered: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PanelState {
    pub item: u16,
    pub name: String,
    pub weight: String,
    pub cost: String,
}

/// The open screen as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct InventoryState {
    /// UI units.
    pub cursor: [f32; 2],
    pub lists: Vec<ListState>,
    pub selected: Option<u16>,
    pub dragging: Option<u16>,
    pub menu: Option<MenuState>,
    /// The text of `bag_static` (4.5), colour codes included.
    pub weight_line: String,
    pub money: String,
    pub time: String,
    /// Health, psy and radiation bars' drawn values, 0–100.
    pub bars: [f32; 3],
    pub protections: Vec<String>,
    pub panel: Option<PanelState>,
    pub exit: [f32; 4],
    pub exit_look: &'static str,
}

#[cfg(test)]
pub(crate) mod tests;
