// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The anomaly detector's sounds (docs/behavior/sound-detector.md, "S21"):
//! a detector section's radius and table of zone classes with their
//! frequencies and sound lists (2), on and off by where the inventory puts
//! it and its holder (3), the scheduled range test that keeps its range and
//! zone lists (4), the per-frame counters at the rate of 5.2 (5) and the
//! ticks they make (6).
//!
//! The sounds are played by the level's sound side from the
//! [`SoundCommand`]s a detector returns; the zones are the level's
//! ([`crate::zones::Zones`]), indexed as there.
//!
//! What is left to other issues:
//! - TODO(#398): S21's review gaps. A saved game does not exist yet; a
//!   detector starts as created (off, no lists) with every level, as from
//!   its place in the inventory and the next range test. Zones are never
//!   destroyed while their level runs, so a zone of the zone list always
//!   exists at the per-frame update.
//! - TODO(#147): the sounds' AI sound type `item` (2.2) is for the reports
//!   to the AI (S24), which do not exist.

use std::io;

use crate::character::inventory::Place;
use crate::config::{Config, Section, value};
use crate::zones::{self, State, Zones};

/// A detector's class id (1).
pub const CLASS_ID: &str = "D_SIMDET";

/// The factor of the zone's shape-sphere radius within which the rate is
/// highest (5.2 step 2): the `f32` nearest to 0.8.
pub const NEAR: f32 = 0.8;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// One sound of a sound list (2.2).
#[derive(Clone, Debug, PartialEq)]
pub struct ListSound {
    pub name: String,
    /// The play volume.
    pub volume: f32,
    /// The delay in seconds.
    pub delay: f32,
}

/// An entry of the table of zone classes (2.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The zone class id: the `class` of the section `zone_class_<n>` names.
    pub zone_class: String,
    /// *f*min and *f*max (ticks per second).
    pub min_freq: f32,
    pub max_freq: f32,
    pub sounds: Vec<ListSound>,
}

/// What a detector reads from its section (2.1).
#[derive(Clone, Debug, PartialEq)]
pub struct DetectorSection {
    pub name: String,
    /// *ρ* (m).
    pub radius: f32,
    /// The entries, by zone class id: one per class id.
    pub entries: Vec<Entry>,
}

/// A list item, `None` when absent or empty (2.2).
fn item<'a>(items: &[&'a str], i: usize) -> Option<&'a str> {
    items.get(i).copied().filter(|v| !v.is_empty())
}

/// The sound list of key prefix `prefix` in `section` (2.2): the keys
/// `prefix`, `prefix1`, `prefix2`, … while each exists. An empty list is
/// the game's assertion failure: an error (what follows one is
/// unestablished, S21 10; it ends the game here as a fatal error does).
pub fn sound_list(section: Section<'_>, prefix: &str) -> io::Result<Vec<ListSound>> {
    let mut out = Vec::new();
    let mut key = prefix.to_owned();
    while let Some(v) = section.get(&key) {
        let items: Vec<&str> = v.unwrap_or("").split(',').map(str::trim).collect();
        out.push(ListSound {
            name: item(&items, 0).unwrap_or("").to_owned(),
            volume: item(&items, 1).map_or(1.0, value::float),
            delay: item(&items, 2).map_or(0.0, value::float),
        });
        key = format!("{prefix}{}", out.len());
    }
    if out.is_empty() {
        return Err(invalid(format!(
            "[{}]: assertion failed: no sound {prefix}",
            section.name()
        )));
    }
    Ok(out)
}

impl DetectorSection {
    /// Reads detector section `name` of `system` (2.1). A missing required
    /// key is fatal (ltx-config.md), and so is an assertion failure:
    /// *f*min ≥ *f*max or an empty sound list (S21 10: what follows one is
    /// unestablished).
    pub fn read(system: &Config, name: &str) -> io::Result<Self> {
        let s = system
            .section(name)
            .ok_or_else(|| invalid(format!("no detector section [{name}]")))?;
        let at = |e: io::Error| invalid(format!("[{name}]: {e}"));
        let float = |key: &str| s.read(key).map(value::float).map_err(at);
        let radius = float("radius")?;
        let mut entries: Vec<Entry> = Vec::new();
        for n in 1.. {
            let Some(class_section) = s.get(&format!("zone_class_{n}")) else {
                break;
            };
            let class_section = class_section.unwrap_or("").trim();
            let zone_class = (system.read(class_section, "class"))
                .map_err(at)?
                .trim()
                .to_owned();
            let min_freq = float(&format!("zone_min_freq_{n}"))?;
            let max_freq = float(&format!("zone_max_freq_{n}"))?;
            if min_freq >= max_freq {
                return Err(invalid(format!(
                    "[{name}]: assertion failed: zone_min_freq_{n} {min_freq} is not below \
                     zone_max_freq_{n} {max_freq}"
                )));
            }
            let sounds = sound_list(s, &format!("zone_sound_{n}_"))?;
            let entry = Entry {
                zone_class,
                min_freq,
                max_freq,
                sounds,
            };
            // 2.1: a class id named again replaces the earlier entry's
            // frequencies and sound list.
            match entries
                .iter_mut()
                .find(|e| e.zone_class == entry.zone_class)
            {
                Some(e) => *e = entry,
                None => entries.push(entry),
            }
        }
        // Required, not sound (9).
        s.read("ef_detector_type").map_err(at)?;
        Ok(Self {
            name: name.to_owned(),
            radius,
            entries,
        })
    }

    /// The entry of zone class id `class_id`.
    pub fn entry(&self, class_id: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.zone_class == class_id)
    }
}

/// The rate *f* (ticks per second) of `entry` for a holder at `holder` and
/// a zone at `zone` with shape-sphere radius `zone_radius`, for a detector
/// of radius `radius` (5.2), in `f32`.
pub fn frequency(
    entry: &Entry,
    radius: f32,
    holder: [f32; 3],
    zone: [f32; 3],
    zone_radius: f32,
) -> f32 {
    let d = |k: usize| holder[k] - zone[k];
    let s = (d(0) * d(0) + d(1) * d(1) + d(2) * d(2)).sqrt();
    let d = (s - zone_radius * NEAR).max(0.0);
    let u = (1.0 - d / radius).clamp(0.0, 1.0);
    ((((entry.max_freq - entry.min_freq) * u) * u) * u) * u + entry.min_freq
}

/// *T* = 1000 / *f* (ms, 5.3).
pub fn period(frequency: f32) -> f32 {
    1000.0 / frequency
}

/// Whether a bounding sphere (`centre`, `radius`) is found by a range test
/// at `q` with the detector radius `rho` (4.2 step 1), in `f32`.
pub fn in_range(q: [f32; 3], centre: [f32; 3], radius: f32, rho: f32) -> bool {
    let d = |k: usize| q[k] - centre[k];
    let r = radius + rho;
    (d(0) * d(0) + d(2) * d(2)) + d(1) * d(1) < r * r
}

/// What the level's sound side does with a detector's sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundCommand {
    /// A tick of entry `entry` (6): every sound of its list stopped at
    /// once, one chosen at random played 2D at (0, 0, 0), not looped, with
    /// its delay and volume. `zone` is the ticking zone's index in
    /// [`Zones::zones`].
    Tick { entry: usize, zone: usize },
    /// Every sound of every list stopped at once (3: the parent lost).
    StopAll,
}

/// A zone of the zone list with its counter *c* (ms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Listed {
    /// The zone's index in [`Zones::zones`].
    pub zone: usize,
    pub counter: u32,
}

/// One detector object's state (3, 4).
#[derive(Clone, Debug, Default)]
pub struct Detector {
    pub on: bool,
    /// It has a parent; `holder` when that parent is the player's
    /// character (3).
    pub parent: bool,
    pub holder: bool,
    /// Its position: its parent's at its latest scheduled update (4.1).
    pub position: [f32; 3],
    /// The range list (zone indices) and the zone list (4.2).
    pub range: Vec<usize>,
    pub zones: Vec<Listed>,
    /// The next scheduled update's clock; `None` before the first, which
    /// is due at once.
    due: Option<u32>,
    /// Where the inventory last put it.
    place: Option<Place>,
}

impl Detector {
    /// A detector as it is created: off, without a parent (3).
    pub fn new() -> Self {
        Self::default()
    }

    /// It gets a parent; `player` when the parent is the player's
    /// character, which makes that its holder (3).
    pub fn set_parent(&mut self, player: bool, position: [f32; 3]) {
        self.parent = true;
        self.holder = player;
        self.position = position;
    }

    /// It loses its parent (3): the holder cleared, its lists emptied and
    /// every sound of its lists stopped at once.
    pub fn lose_parent(&mut self) -> SoundCommand {
        self.parent = false;
        self.holder = false;
        self.range.clear();
        self.zones.clear();
        self.place = None;
        SoundCommand::StopAll
    }

    /// The inventory has it at `place` (3): put into a slot or onto the
    /// belt it is switched on, into the backpack off; no place changes
    /// nothing. Only a change of place is a putting.
    pub fn placed(&mut self, place: Place) {
        if self.place == Some(place) {
            return;
        }
        self.place = Some(place);
        match place {
            Place::Slot(_) | Place::Belt => self.on = true,
            Place::Backpack => self.on = false,
            Place::None => {}
        }
    }

    /// The range test at the detector's position (4.2): every zone whose
    /// placed shape sphere is in range is found (each zone is enabled and
    /// has a collision shape, and none is ever marked as being destroyed);
    /// those found that are not in the range list join it and, unless
    /// disabled, the zone list with *c* = 0; those not found leave both.
    pub fn range_test(&mut self, section: &DetectorSection, zones: &Zones) {
        let found: Vec<usize> = (zones.zones().iter().enumerate())
            .filter(|(_, z)| {
                let (c, r) = z.placed_sphere();
                in_range(self.position, c, r, section.radius)
            })
            .map(|(i, _)| i)
            .collect();
        for &i in &found {
            if self.range.contains(&i) {
                continue;
            }
            self.range.push(i);
            if zones.zones()[i].state != State::Disabled {
                self.zones.push(Listed {
                    zone: i,
                    counter: 0,
                });
            }
        }
        self.range.retain(|i| found.contains(i));
        self.zones.retain(|l| found.contains(&l.zone));
    }

    /// A scheduled update at clock `clock` if one is due (4.1, P9 6.2's
    /// general schedule: next due by the distance from the camera `camera`
    /// to the detector's position after the update): while it is on and has a parent, its
    /// position becomes the parent's `parent_position`, and when the
    /// parent is the player's character (`controlled`; believed to be the
    /// object the player controls, S21 10) the range test runs.
    pub fn scheduled(
        &mut self,
        clock: u32,
        camera: [f32; 3],
        parent_position: [f32; 3],
        controlled: bool,
        section: &DetectorSection,
        zones: &Zones,
    ) {
        if self
            .due
            .is_some_and(|due| (clock.wrapping_sub(due) as i32) < 0)
        {
            return;
        }
        if self.on && self.parent {
            self.position = parent_position;
            if controlled {
                self.range_test(section, zones);
            }
        }
        // TODO(#398): whether the next due time is reckoned from the
        // position before or after the update is unestablished; after.
        let p = self.position;
        let d = (0..3)
            .map(|k| f64::from(camera[k] - p[k]).powi(2))
            .sum::<f64>()
            .sqrt();
        self.due = Some(clock.wrapping_add(zones::scheduled_period(d)));
    }

    /// The per-frame update with the frame's `elapsed` ms and the holder at
    /// `holder` (5): while on, with a parent and a holder, each zone of the
    /// zone list in turn (in the order they joined it; S21 10: the order is
    /// unestablished) is skipped when the table has no entry for its class
    /// id or it is not visible by the detector; otherwise its counter
    /// passing *T* makes a tick, else grows by `elapsed`. Returns the ticks
    /// in order.
    pub fn frame(
        &mut self,
        elapsed: u32,
        holder: [f32; 3],
        section: &DetectorSection,
        zones: &Zones,
        out: &mut Vec<SoundCommand>,
    ) {
        if !(self.on && self.parent && self.holder) {
            return;
        }
        for l in &mut self.zones {
            let Some(z) = zones.zones().get(l.zone) else {
                continue;
            };
            if !z.section.visible_by_detector {
                continue;
            }
            let Some(entry) = section.entry(&z.class_id) else {
                continue;
            };
            let (_, r) = z.placed_sphere();
            let f = frequency(
                &section.entries[entry],
                section.radius,
                holder,
                z.position(),
                r,
            );
            if f64::from(l.counter) > f64::from(period(f)) {
                l.counter = 0;
                out.push(SoundCommand::Tick {
                    entry,
                    zone: l.zone,
                });
            } else {
                l.counter = l.counter.wrapping_add(elapsed);
            }
        }
    }
}

#[cfg(test)]
mod tests;
