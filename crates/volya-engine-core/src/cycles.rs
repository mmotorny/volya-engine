// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The cycles of a model and how long they last
//! (docs/behavior/player-cycle-lengths.md, S10): a cycle found by name
//! (section 2), its length *L*, speed *s* and duration *D* (section 3), and
//! whether it stops at the end (section 4).
//!
//! No pose is drawn, so a running cycle's time (section 4) is not kept:
//! nothing reads it (the footsteps time their steps on their own clock,
//! docs/behavior/player-footsteps.md section 3).

use std::collections::HashMap;

use volya_engine_formats::skeletal_motions::{MotionSet, quantize};

/// f32(1/30): one key's time (S10 section 3).
pub const KEY_TIME: f32 = 1.0 / 30.0;
/// f32(1/655.35) (section 3).
pub const SPEED_UNIT: f32 = 0.001_525_902_2;
/// The program keeps the key count in 24 bits (section 3).
pub const KEY_COUNT_MASK: u32 = 0x00FF_FFFF;
/// Flags bit 1: the cycle stops at the end (section 4).
pub const STOPS_AT_END: u16 = 0x2;

/// A cycle's values (S10 section 3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cycle {
    /// *L* = f32(*N*′ · f32(1/30)), seconds.
    pub length: f32,
    /// *s* = f32(*q*(speed) · f32(1/655.35)).
    pub speed: f32,
    /// *D* = f32(*L* / *s*), seconds.
    pub duration: f32,
    /// Flags bit 1 (section 4).
    pub stops_at_end: bool,
}

impl Cycle {
    /// From a definition's key count *N*, flags and stored speed.
    pub fn new(keys: u32, flags: u16, speed: f32) -> Self {
        let length = (keys & KEY_COUNT_MASK) as f32 * KEY_TIME;
        let speed = f32::from(quantize(speed)) * SPEED_UNIT;
        Self {
            length,
            speed,
            duration: length / speed,
            stops_at_end: flags & STOPS_AT_END != 0,
        }
    }
}

/// A model's cycles by name (S10 section 2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cycles {
    by_name: HashMap<String, Cycle>,
}

impl Cycles {
    /// The cycles of a model's motion sets, in the order of its motion
    /// files: a name finds the cycle of the last file that has one of that
    /// name (fx motions are never found).
    pub fn new(sets: &[MotionSet]) -> Self {
        let mut by_name = HashMap::new();
        for set in sets {
            for (d, &keys) in set.params.definitions.iter().zip(&set.key_counts) {
                if !d.is_fx() {
                    by_name.insert(d.name.clone(), Cycle::new(keys, d.flags, d.speed));
                }
            }
        }
        Self { by_name }
    }

    /// The cycle named `name`, compared byte for byte with the stored
    /// (lower-cased) names; the name is not lower-cased.
    pub fn get(&self, name: &str) -> Option<&Cycle> {
        self.by_name.get(name)
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Cycle)> {
        self.by_name.iter().map(|(n, c)| (n.as_str(), c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::skeletal_motions::{MotionDefinition, MotionParams};

    fn set(definitions: &[(&str, u16, f32, u32)]) -> MotionSet {
        MotionSet {
            params: MotionParams {
                version: 3,
                partitions: Vec::new(),
                definitions: definitions
                    .iter()
                    .enumerate()
                    .map(|(k, &(name, flags, speed, _))| MotionDefinition {
                        name: name.into(),
                        flags,
                        bone_or_partition: 0xFFFF,
                        motion_index: k as u16,
                        speed,
                        power: 1.0,
                        accrue: 2.0,
                        falloff: 2.0,
                    })
                    .collect(),
            },
            key_counts: definitions.iter().map(|d| d.3).collect(),
        }
    }

    #[test]
    fn lengths_and_durations_of_s10_section_5() {
        // S10 5.1, in full precision for `norm_walk_fwd_0`.
        let walk = Cycle::new(26, 8, 1.0);
        assert_eq!(
            (f64::from(walk.length) * 1e10).round() / 1e10,
            0.866_666_734_2
        );
        assert_eq!(walk.speed, 0.999_465_94);
        assert_eq!(walk.duration, 0.867_129_8);
        assert!(!walk.stops_at_end);
        let round = |x: f32| (f64::from(x) * 1e6).round() / 1e6;
        // (N, flags, stored speed) → (L, D) of the table.
        let rows = [
            (20, 0, 1.0, 0.666667, 0.667023),
            (40, 0, 1.5, 1.333333, 0.888912),
            (25, 2, 1.0, 0.833333, 0.833779),
            (10, 0, 1.0, 0.333333, 0.333511),
            (55, 8, 1.0, 1.833333, 1.834313),
            (39, 8, 1.0, 1.3, 1.300695),
            (30, 0, 1.0, 1.0, 1.000534),
        ];
        for (n, flags, speed, l, d) in rows {
            let c = Cycle::new(n, flags, speed);
            assert_eq!((round(c.length), round(c.duration)), (l, d), "N = {n}");
        }
        assert!(Cycle::new(25, 2, 1.0).stops_at_end);
        // S11 3.3's durations, to 7 significant digits.
        let digits = |x: f32| (f64::from(x) * 1e7).round() / 1e7;
        assert_eq!(digits(Cycle::new(10, 0, 1.0).duration), 0.333_511_5);
        assert_eq!(digits(Cycle::new(20, 8, 1.0).duration), 0.667_022_9);
        assert_eq!(digits(Cycle::new(55, 8, 1.0).duration), 1.834_313);
        assert_eq!(digits(Cycle::new(39, 8, 1.0).duration), 1.300_694_7);
        // Only the low 24 bits of N count.
        assert_eq!(Cycle::new(0x0100_001A, 8, 1.0), walk);
    }

    #[test]
    fn a_name_finds_the_last_file_s_cycle_and_never_an_fx() {
        let first = set(&[("walk", 0, 1.0, 26), ("hit", 0, 1.0, 5), ("fx", 1, 1.0, 3)]);
        let second = set(&[("walk", 2, 1.0, 40), ("hit", 1, 1.0, 9)]);
        let cycles = Cycles::new(&[first, second]);
        assert_eq!(cycles.get("walk"), Some(&Cycle::new(40, 2, 1.0)));
        // An fx of a later file does not hide an earlier cycle.
        assert_eq!(cycles.get("hit"), Some(&Cycle::new(5, 0, 1.0)));
        assert_eq!(cycles.get("fx"), None);
        // Not lower-cased.
        assert_eq!(cycles.get("Walk"), None);
        assert_eq!(cycles.len(), 2);
    }
}
