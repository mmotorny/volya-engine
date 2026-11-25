// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The shared pseudo-random generator the level's sound rules draw from
//! (docs/behavior/sound-weather-ambient.md section 3.2; the same generator
//! in docs/behavior/sound-static-sources.md 4.2, sound-level-music.md 4.2,
//! sound-occlusion.md 4.2 and sound-rain-thunder.md section 6).
//!
//! The specs state the original's linear congruential generator and that
//! its sequence cannot be reproduced (it is seeded from the processor's
//! time-stamp counter and shared with other uses), so any uniform
//! generator of 0 … 32,767 serves. We use the stated one, seeded with 1:
//! the seed S4's worked examples use.

/// The largest draw.
pub const MAX: u32 = 32_767;

/// The generator: state *s* ← *s* · 214,013 + 2,531,011 mod 2³², draws
/// (*s* ≫ 16) AND 32,767.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedRandom {
    state: u32,
}

impl Default for SharedRandom {
    fn default() -> Self {
        Self::new(1)
    }
}

impl SharedRandom {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// The next draw *r*, 0 … 32,767.
    pub fn draw(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.state >> 16) & MAX
    }

    /// U(*a*, *b*) = *a* + *r* · (*b* − *a*) / 32,767 in 32-bit float
    /// (sound-rain-thunder.md conventions; S4's distance, 3.2).
    pub fn uniform(&mut self, a: f32, b: f32) -> f32 {
        a + self.draw() as f32 * (b - a) / MAX as f32
    }
}

/// *a* + (*r* mod (*b* − *a*)) with C's truncating signed remainder
/// (S3's R(*a*, *b*), S4's period, S5's pause): `None` for *b* = *a*, where
/// the original divides by zero (S3 4.2, S4 4.7).
pub fn range(r: u32, a: u32, b: u32) -> Option<u32> {
    let span = (b as i32).wrapping_sub(a as i32);
    if span == 0 {
        return None;
    }
    // Truncating remainder of a non-negative r: the sign of r, so r mod |span|.
    let m = (r as i32).wrapping_rem(span);
    Some(a.wrapping_add(m as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_draws_with_seed_1() {
        // S4 section 7: "first draws 41, 18467, 6334, 26500, 19169, 15724,
        // 11478".
        let mut r = SharedRandom::new(1);
        let draws: Vec<u32> = (0..7).map(|_| r.draw()).collect();
        assert_eq!(draws, [41, 18467, 6334, 26500, 19169, 15724, 11478]);
    }

    #[test]
    fn ranges() {
        // S4 section 7: periods from 5,000 / 10,000 ms.
        assert_eq!(range(12_345, 5_000, 10_000), Some(7_345));
        assert_eq!(range(32_767, 5_000, 10_000), Some(7_767));
        assert_eq!(range(0, 5_000, 10_000), Some(5_000));
        // `ambient_env_indoor`, 10 … 30 s.
        assert_eq!(range(12_345, 10_000, 30_000), Some(22_345));
        // S5 8.3: 20,000 … 40,000.
        assert_eq!(range(19_999, 20_000, 40_000), Some(39_999));
        assert_eq!(range(20_000, 20_000, 40_000), Some(20_000));
        assert_eq!(range(32_767, 20_000, 40_000), Some(32_767));
        // Reversed bounds: a + (r mod |b − a|) (S3 4.2).
        assert_eq!(range(7, 10, 5), Some(12));
        assert_eq!(range(1, 3, 3), None);
    }
}
