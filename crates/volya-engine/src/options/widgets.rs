// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The value rules of the options screen's controls
//! (docs/behavior/options-screens.md section 4): track bars, drop-down
//! lists, scroll bars, the key table's capture and conflicts, and the
//! resolution list (section 9). Pure models; the screen draws them and
//! routes input to them.

use crate::core::console::{self, ACTIONS, KEYS};
use volya_engine_formats::ui::Resolution;

/// A track bar's value model (section 4.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub integer: bool,
    pub invert: bool,
    /// The bar's width *w*.
    pub width: f32,
}

/// Width of the thumb, units.
pub const THUMB: f32 = 24.0;

impl Track {
    /// The thumb's x relative to the bar for value `v`:
    /// (*v* − *m*) · (*w* − 24) / (*M* − *m*), mirrored when inverted.
    pub fn thumb_x(&self, v: f32) -> f32 {
        let range = self.max - self.min;
        let p = if range > 0.0 {
            (v - self.min) * (self.width - THUMB) / range
        } else {
            0.0
        };
        if self.invert {
            (self.width - THUMB) - p
        } else {
            p
        }
    }

    /// The value for the cursor at `x` relative to the bar: *x* (mirrored
    /// when inverted) clamped to [12, *w* − 12], *r* = (*x* − 12) ·
    /// (*M* − *m*) / (*w* − 24), rounded to the step, clamped, truncated for
    /// integer bars.
    pub fn value_at(&self, x: f32) -> f32 {
        let x = if self.invert { self.width - x } else { x };
        let x = x.clamp(THUMB / 2.0, self.width - THUMB / 2.0);
        let r = (x - THUMB / 2.0) * (self.max - self.min) / (self.width - THUMB);
        let s = self.step;
        let mut q = if s > 0.0 { s * (r / s).trunc() } else { r };
        if s > 0.0 && r - q > s / 2.0 {
            q += s;
        }
        let v = (q + self.min).clamp(self.min, self.max);
        if self.integer { v.trunc() } else { v }
    }

    /// The value as the console command writes it (section 5.3): integers
    /// in decimal, reals with six decimals.
    pub fn format(&self, v: f32) -> String {
        if self.integer {
            format!("{}", v as i64)
        } else {
            format!("{v:.6}")
        }
    }
}

/// A drop-down list's entries and selection (section 4.6).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropDown {
    /// Entry names (the variable's values) and texts (through the string
    /// table), in list order; entry *k* has the value *k*.
    pub entries: Vec<(String, String)>,
    pub selected: Option<usize>,
    /// The control's stored current value.
    pub stored: usize,
    /// The closed line's text.
    pub line: String,
}

impl DropDown {
    /// Refreshes from the variable (section 4.6): `entries` rebuilt, the
    /// closed line set to `text` (the variable's status through the string
    /// table), the entry whose text equals it selected and its value
    /// stored; with no match nothing is selected and the stored value is 1.
    pub fn refresh(&mut self, entries: Vec<(String, String)>, text: String) {
        self.entries = entries;
        self.selected = self.entries.iter().position(|(_, t)| *t == text);
        self.stored = self.selected.unwrap_or(1);
        self.line = text;
    }

    /// A click on entry `k`: selected, its text on the line; returns the
    /// entry's name when its value differs from the stored one (it is then
    /// stored, and the variable is to be set).
    pub fn choose(&mut self, k: usize) -> Option<String> {
        let (name, text) = self.entries.get(k)?.clone();
        self.selected = Some(k);
        self.line = text;
        (k != self.stored).then(|| {
            self.stored = k;
            name
        })
    }

    /// Shows the entry of value `k` (Cancel's undo, section 5.5); its name.
    pub fn show(&mut self, k: usize) -> Option<String> {
        let (name, text) = self.entries.get(k)?.clone();
        self.selected = Some(k);
        self.stored = k;
        self.line = text;
        Some(name)
    }
}

/// Repeat interval of a held arrow, milliseconds (section 4.8).
pub const REPEAT_MS: u64 = 100;
/// Arrow length, units.
pub const ARROW: f32 = 16.0;

/// A vertical scroll bar's value model (section 4.8).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollBar {
    pub pos: i32,
    /// trunc(content height).
    pub max: i32,
    /// The view height *H*.
    pub height: f32,
    /// The bar's width *t*.
    pub width: f32,
}

impl ScrollBar {
    pub fn new(content: f32, height: f32, width: f32) -> Self {
        Self {
            pos: 0,
            max: content as i32,
            height,
            width,
        }
    }

    /// Page *pg* = trunc(*H*).
    pub fn page(&self) -> i32 {
        self.height as i32
    }

    /// Step *st* = max(1, trunc(0.1 · *H*)).
    pub fn step(&self) -> i32 {
        ((0.1 * self.height) as i32).max(1)
    }

    /// The largest position, *max* − *pg* + 1 (at least 0).
    pub fn limit(&self) -> i32 {
        (self.max - self.page() + 1).max(0)
    }

    fn n(&self) -> i32 {
        (self.max - self.page() + 1).max(1)
    }

    pub fn set(&mut self, pos: i32) {
        self.pos = pos.clamp(0, self.limit());
    }

    /// Moves by `steps` steps (negative up).
    pub fn scroll(&mut self, steps: i32) {
        self.set(self.pos + steps * self.step());
    }

    /// The work length *W*ₐ = trunc(*H* − 32).
    fn work(&self) -> i32 {
        (self.height - 2.0 * ARROW) as i32
    }

    /// The box length *L* = clamp(*W*ₐ · *pg* / *max*, min(*t*, *H* − 32),
    /// *H* − 32).
    pub fn box_length(&self) -> f32 {
        let room = self.height - 2.0 * ARROW;
        let l = if self.max > 0 {
            self.work() as f32 * self.page() as f32 / self.max as f32
        } else {
            room
        };
        l.clamp(self.width.min(room), room.max(self.width.min(room)))
    }

    /// The box's top relative to the bar: trunc((*pos* · (*W*ₐ −
    /// trunc(*L*)) + *n* · 16) / *n*), clamped between the arrows.
    pub fn box_top(&self) -> f32 {
        let l = self.box_length();
        let n = self.n();
        let top = (self.pos * (self.work() - l as i32) + n * ARROW as i32) / n;
        (top as f32).clamp(ARROW, (self.height - ARROW - l).max(ARROW))
    }

    /// Drags the box's top to `y` (relative to the bar): *pos* =
    /// trunc(*n* · (*y* − 16) / (*W*ₐ − *L*)), clamped.
    pub fn drag_to(&mut self, y: f32) {
        let l = self.box_length();
        let room = self.work() as f32 - l;
        let pos = if room > 0.0 {
            (self.n() as f32 * (y - ARROW) / room) as i32
        } else {
            0
        };
        self.set(pos);
    }
}

/// One key cell of the key table (sections 11.3–11.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// Index into [`ACTIONS`].
    pub action: usize,
    /// 0 primary, 1 secondary.
    pub slot: usize,
    /// Index into [`KEYS`], `None` when empty.
    pub key: Option<usize>,
    /// A key was assigned since the last save.
    pub changed: bool,
    /// Emptied by a conflict: shows `---` (section 11.4).
    pub cleared: bool,
}

/// Assigns key `key` to cell `i` of `cells` (section 11.4): shown and
/// marked changed; every other cell showing the key is cleared (not
/// marked), except cells of the same action and of an action that may
/// share it.
pub fn assign(cells: &mut [Cell], i: usize, key: usize) {
    cells[i].key = Some(key);
    cells[i].changed = true;
    cells[i].cleared = false;
    let action = cells[i].action;
    for (j, c) in cells.iter_mut().enumerate() {
        if j != i
            && c.key == Some(key)
            && c.action != action
            && !console::may_share(action, c.action)
        {
            c.key = None;
            c.cleared = true;
        }
    }
}

/// The console lines that save the changed cells, in table order (section
/// 11.5): `unbind`/`unbind_sec`, then `bind`/`bind_sec` when the cell holds
/// a key; the cells are then unchanged.
pub fn save_cells(cells: &mut [Cell]) -> Vec<String> {
    let mut lines = Vec::new();
    for c in cells.iter_mut().filter(|c| c.changed) {
        let action = ACTIONS[c.action].0;
        let sec = if c.slot == 1 { "_sec" } else { "" };
        lines.push(format!("unbind{sec} {action}"));
        if let Some(k) = c.key {
            lines.push(format!("bind{sec} {action} {}", KEYS[k].0));
        }
        c.changed = false;
    }
    lines
}

/// Display sizes offered besides the screen's own (our choice, #49: the
/// browser exposes no display modes): common 4:3, 5:4, 16:10 and 16:9
/// sizes.
pub const COMMON_MODES: &[(u32, u32)] = &[
    (800, 600),
    (1024, 768),
    (1152, 864),
    (1280, 720),
    (1280, 800),
    (1280, 960),
    (1280, 1024),
    (1366, 768),
    (1440, 900),
    (1600, 900),
    (1600, 1200),
    (1680, 1050),
    (1920, 1080),
    (1920, 1200),
    (2560, 1440),
    (2560, 1600),
    (3840, 2160),
];

/// The resolution list (section 9) for a screen of `native` pixels: the
/// common sizes that fit it and the screen's own size (our choice, #49),
/// without those narrower than 800 pixels or repeated, by width then
/// height (our choice: the order of a list the browser does not give).
/// When nothing is 800 wide the screen's own size is offered alone (our
/// choice, so the list is never empty).
pub fn resolution_list(native: Resolution) -> Vec<String> {
    let mut modes: Vec<(u32, u32)> = COMMON_MODES
        .iter()
        .copied()
        .filter(|&(w, h)| w <= native.width && h <= native.height)
        .chain([(native.width, native.height)])
        .filter(|&(w, _)| w >= 800)
        .collect();
    modes.sort_unstable();
    modes.dedup();
    if modes.is_empty() {
        modes.push((native.width, native.height));
    }
    modes.into_iter().map(|(w, h)| format!("{w}x{h}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::console::{action_index, key_index};

    #[test]
    fn track_bars_place_the_thumb_and_round_to_the_step() {
        // track_gamma: 0.5–1.5, step 0.1, 261 wide.
        let t = Track {
            min: 0.5,
            max: 1.5,
            step: 0.1,
            integer: false,
            invert: false,
            width: 261.0,
        };
        assert_eq!(t.thumb_x(0.5), 0.0);
        assert_eq!(t.thumb_x(1.5), 237.0);
        assert!((t.thumb_x(1.0) - 118.5).abs() < 1e-4);
        // The ends are reached 12 units in; the middle rounds to the step.
        assert_eq!(t.value_at(0.0), 0.5);
        assert_eq!(t.value_at(300.0), 1.5);
        assert!((t.value_at(12.0 + 118.5) - 1.0).abs() < 1e-5);
        // r = 0.33 → q = 0.3; r = 0.36 → q = 0.4 (over half a step).
        let x = |r: f32| 12.0 + r * 237.0;
        assert!((t.value_at(x(0.33)) - 0.8).abs() < 1e-5);
        assert!((t.value_at(x(0.36)) - 0.9).abs() < 1e-5);
        assert_eq!(t.format(0.8), "0.800000");
        // texture_lod: 0–4 integer, inverted: 4 at the left.
        let lod = Track {
            min: 0.0,
            max: 4.0,
            step: 1.0,
            integer: true,
            invert: true,
            width: 245.0,
        };
        assert_eq!(lod.thumb_x(4.0), 0.0);
        assert_eq!(lod.thumb_x(0.0), 221.0);
        assert_eq!(lod.value_at(0.0), 4.0);
        assert_eq!(lod.value_at(245.0), 0.0);
        assert_eq!(lod.value_at(122.5), 2.0);
        assert_eq!(lod.format(2.0), "2");
    }

    #[test]
    fn drop_downs_select_by_text_and_store_one_without_a_match() {
        let entries = || {
            ["800x600", "1024x768", "1280x720"]
                .map(|m| (m.to_owned(), m.to_owned()))
                .to_vec()
        };
        let mut d = DropDown::default();
        d.refresh(entries(), "1280x720".into());
        assert_eq!((d.selected, d.stored), (Some(2), 2));
        assert_eq!(d.choose(2), None, "the same value: nothing to set");
        assert_eq!(d.choose(0).as_deref(), Some("800x600"));
        assert_eq!((d.stored, d.line.as_str()), (0, "800x600"));
        // A mode not offered: no selection, stored value 1; choosing the
        // entry of value 1 only changes the line.
        d.refresh(entries(), "640x480".into());
        assert_eq!(
            (d.selected, d.stored, d.line.as_str()),
            (None, 1, "640x480")
        );
        assert_eq!(d.choose(1), None);
        assert_eq!(d.line, "1024x768");
    }

    #[test]
    fn scroll_bars_follow_the_spec() {
        // The advanced page: 390 units of content in a 313-unit view.
        let mut b = ScrollBar::new(390.0, 313.0, 16.0);
        assert_eq!((b.page(), b.step(), b.limit()), (313, 31, 78));
        // W = 281; L = 281 · 313 / 390 = 225.52.
        assert!((b.box_length() - 225.520_51).abs() < 1e-3);
        assert_eq!(b.box_top(), 16.0);
        b.scroll(1);
        assert_eq!(b.pos, 31);
        // trunc((31 · (281 − 225) + 78 · 16) / 78) = 38.
        assert_eq!(b.box_top(), 38.0);
        b.scroll(10);
        assert_eq!(b.pos, 78);
        b.scroll(-10);
        assert_eq!(b.pos, 0);
        b.drag_to(16.0 + 0.5 * (281.0 - 225.520_51));
        assert_eq!(b.pos, 39);
        // The key table: 1407 units in 181; step 18.
        let k = ScrollBar::new(1407.0, 181.0, 16.0);
        assert_eq!((k.step(), k.limit()), (18, 1227));
        // L = 149 · 181 / 1407; never shorter than the bar is wide.
        assert!((k.box_length() - 19.167_73).abs() < 1e-3);
        assert_eq!(ScrollBar::new(10_000.0, 181.0, 16.0).box_length(), 16.0);
    }

    #[test]
    fn capturing_a_key_clears_it_elsewhere_except_shared_kinds() {
        let a = |n| action_index(n).unwrap();
        let k = |n| key_index(n).unwrap();
        let mut cells = vec![
            Cell {
                action: a("forward"),
                slot: 0,
                key: Some(k("kW")),
                changed: false,
                cleared: false,
            },
            Cell {
                action: a("forward"),
                slot: 1,
                key: None,
                changed: false,
                cleared: false,
            },
            Cell {
                action: a("jump"),
                slot: 0,
                key: Some(k("kSPACE")),
                changed: false,
                cleared: false,
            },
            Cell {
                action: a("vote"),
                slot: 0,
                key: Some(k("kF6")),
                changed: false,
                cleared: false,
            },
            Cell {
                action: a("quick_save"),
                slot: 0,
                key: None,
                changed: false,
                cleared: false,
            },
            Cell {
                action: a("llookout"),
                slot: 0,
                key: Some(k("kF6")),
                changed: false,
                cleared: false,
            },
        ];
        assign(&mut cells, 1, k("kSPACE"));
        assert_eq!(cells[2].key, None, "jump lost Space");
        assert!(!cells[2].changed, "cleared without the changed mark");
        assert!(cells[2].cleared, "shown as ---");
        assign(&mut cells, 0, k("kSPACE"));
        assert_eq!(cells[1].key, Some(k("kSPACE")), "the same action keeps it");
        // quick_save (single player) shares with vote (multiplayer) but not
        // with llookout.
        assign(&mut cells, 4, k("kF6"));
        assert_eq!(cells[3].key, Some(k("kF6")));
        assert_eq!(cells[5].key, None);
        let lines = save_cells(&mut cells);
        assert_eq!(
            lines,
            [
                "unbind forward",
                "bind forward kSPACE",
                "unbind_sec forward",
                "bind_sec forward kSPACE",
                "unbind quick_save",
                "bind quick_save kF6"
            ]
        );
        assert!(cells.iter().all(|c| !c.changed));
        assert!(save_cells(&mut cells).is_empty());
    }

    #[test]
    fn the_resolution_list_offers_what_fits() {
        assert_eq!(
            resolution_list(Resolution::new(1366, 768)),
            ["800x600", "1024x768", "1280x720", "1366x768"]
        );
        // An iPhone 17 Pro Max in landscape: 956 × 440 points at 3×.
        let phone = resolution_list(Resolution::new(2868, 1320));
        assert_eq!(phone.first().unwrap(), "800x600");
        assert_eq!(phone.last().unwrap(), "2868x1320");
        assert!(!phone.contains(&"2560x1440".to_owned()));
        assert!(phone.contains(&"1920x1080".to_owned()));
        // Narrower than 800: the screen alone.
        assert_eq!(resolution_list(Resolution::new(700, 500)), ["700x500"]);
    }
}
