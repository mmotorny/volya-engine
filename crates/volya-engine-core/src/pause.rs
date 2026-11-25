// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The engine's global clock and the pause
//! (docs/behavior/sound-weather-ambient.md section 3.1): the clock of the
//! level's sound rules (S3 4.1, S4 3.1, S5 4.1, S8 3.1), not S2's sound
//! clock.
//!
//! The pause is one on/off state, not nested. Turning it on takes effect
//! only while the game type is none or single-player; Volya Engine starts
//! only single-player games, so here it always does. Of the table's events
//! this module has the main menu, the interface sequences' items and the
//! input focus.
//! TODO(#334): the `pause` action, the level-change window, loading a
//! saved game during a game, the scripts' device pause and `demo_record`'s
//! Pause key.

/// An item's `pause_state` (section 3.1): `on`, `off`, or anything else
/// (missing, empty, another value), which leaves the pause unchanged.
fn pause_state(value: Option<&str>) -> Option<bool> {
    match value {
        Some("on") => Some(true),
        Some("off") => Some(false),
        _ => None,
    }
}

/// The pause and what each event pair remembered when it began.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pause {
    /// Off when the program starts: the spec doesn't say (TODO(#335): its
    /// review gap; the simplest reading).
    on: bool,
    /// While the main menu is shown: whether the pause was on when it
    /// opened.
    menu: Option<bool>,
    /// While an interface sequence's item with `pause_state` `on` or `off`
    /// plays: that state, and whether the pause was on when it started.
    item: Option<(bool, bool)>,
    /// While the input focus is lost: whether the pause was on then.
    focus: Option<bool>,
}

impl Pause {
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// The main menu opens (`true`; on the title screen or during a game)
    /// or closes. Opening it while it is shown, or closing it while it
    /// isn't, is no event.
    pub fn main_menu(&mut self, shown: bool) {
        if shown {
            if self.menu.is_none() {
                self.menu = Some(self.on);
                self.on = true;
            }
        } else if let Some(was_on) = self.menu.take()
            && !was_on
        {
            self.on = false;
        }
    }

    /// An item of an interface sequence with `pause_state` `value` starts.
    pub fn item_started(&mut self, value: Option<&str>) {
        self.item = pause_state(value).map(|state| {
            let was_on = self.on;
            self.on = state;
            (state, was_on)
        });
    }

    /// The item that last started ends.
    pub fn item_ended(&mut self) {
        match self.item.take() {
            Some((true, false)) => self.on = false,
            Some((false, true)) => self.on = true,
            _ => {}
        }
    }

    /// The program loses (`false`) or regains the input focus.
    pub fn focus(&mut self, has: bool) {
        if !has {
            if self.focus.is_none() {
                self.focus = Some(self.on);
                self.on = true;
            }
        } else if let Some(was_on) = self.focus.take()
            && !was_on
        {
            self.on = false;
        }
    }
}

/// "Now" of section 3.1: milliseconds of a real-time timer that starts with
/// the engine and stands still while the pause is on, taken once per frame
/// at its start. It is the timer's elapsed time, not a sum of frame steps,
/// so time with no frame (a level loading) counts in full. Its rate is 1
/// and it has no 20 ms fixed-step mode: the initial values, which nothing
/// in Volya Engine changes (what sets the device flag 0x100 is
/// unestablished).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlobalClock {
    now: u64,
    /// The real time at the previous frame's start.
    last: Option<u64>,
}

impl GlobalClock {
    /// The start of a frame at real time `real_ms` (since the engine
    /// started), the pause `paused` since the previous frame: the real time
    /// since then counts unless paused. Returns now.
    pub fn frame(&mut self, real_ms: u64, paused: bool) -> u64 {
        let elapsed = self.last.map_or(real_ms, |l| real_ms.saturating_sub(l));
        self.last = Some(real_ms);
        if !paused {
            self.now += elapsed;
        }
        self.now
    }

    /// Now, milliseconds. The spec's is unsigned 32-bit, so a rule's
    /// comparisons take it modulo 2³² (`as u32`).
    pub fn now(&self) -> u64 {
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_counts_real_time_not_frames() {
        let mut c = GlobalClock::default();
        assert_eq!(c.frame(0, false), 0);
        assert_eq!(c.frame(16, false), 16);
        // A frame that took 5 s (a level loading) counts in full.
        assert_eq!(c.frame(5016, false), 5016);
    }

    #[test]
    fn the_clock_stands_still_while_paused_and_goes_on_from_there() {
        let mut c = GlobalClock::default();
        c.frame(0, false);
        c.frame(100, false);
        assert_eq!(c.frame(1100, true), 100);
        assert_eq!(c.frame(2100, true), 100);
        assert_eq!(c.frame(2116, false), 116);
        assert_eq!(c.now(), 116);
    }

    #[test]
    fn the_pause_is_not_nested() {
        let mut p = Pause::default();
        assert!(!p.is_on());
        // An `on` item starts while the pause is off, the menu opens over it.
        p.item_started(Some("on"));
        assert!(p.is_on());
        p.main_menu(true);
        // The item ends: it was off when the item started, so off, though
        // the menu is still shown.
        p.item_ended();
        assert!(!p.is_on());
        // The menu remembered on: closing it leaves the pause as it is.
        p.main_menu(false);
        assert!(!p.is_on());
    }

    #[test]
    fn the_main_menu() {
        let mut p = Pause::default();
        p.main_menu(true);
        assert!(p.is_on());
        // Shown again (title menu after leaving a game): no event.
        p.main_menu(true);
        p.main_menu(false);
        assert!(!p.is_on());
        // Closed while not shown: no event.
        p.focus(false);
        p.main_menu(false);
        assert!(p.is_on());
    }

    #[test]
    fn sequence_items() {
        let mut p = Pause::default();
        // `on`: on, then off at its end.
        p.item_started(Some("on"));
        assert!(p.is_on());
        p.item_ended();
        assert!(!p.is_on());
        // `off` while on: off, then on again.
        p.main_menu(true);
        p.item_started(Some("off"));
        assert!(!p.is_on());
        p.item_ended();
        assert!(p.is_on());
        p.main_menu(false);
        // `off` while off: its end leaves it off.
        p.item_started(Some("off"));
        p.item_ended();
        assert!(!p.is_on());
        // Missing, empty or another value: unchanged.
        for v in [None, Some(""), Some("yes")] {
            p.item_started(v);
            assert!(!p.is_on());
            p.item_ended();
            assert!(!p.is_on());
        }
        // An `on` item while on: its end leaves it on.
        p.main_menu(true);
        p.item_started(Some("on"));
        p.item_ended();
        assert!(p.is_on());
    }

    #[test]
    fn focus() {
        let mut p = Pause::default();
        p.focus(false);
        assert!(p.is_on());
        p.focus(true);
        assert!(!p.is_on());
        // Lost while on (the menu shown): regaining it leaves it on.
        p.main_menu(true);
        p.focus(false);
        p.focus(true);
        assert!(p.is_on());
        p.main_menu(false);
        assert!(!p.is_on());
    }
}
