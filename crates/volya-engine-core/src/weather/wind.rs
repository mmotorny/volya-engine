// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The wind of the weather's ambient effects
//! (docs/behavior/weather-ambient-effects.md section 4): the gust in force
//! moves a one-dimensional gradient noise whose value gives the wind
//! strength *W*; and what *W* changes (section 5): a rain drop's mean
//! direction and the detail objects' sway values (drawn by
//! `crate::details`; no approved specification covers the rain's drawing).

use crate::particles::random::Generator;

/// φ per unit of gust (0x3CF5C28F, 4.3).
const GUST_SCALE: f32 = f32::from_bits(0x3CF5_C28F);
/// The amplitude *A* when made (0x3F2AAA3B, 4.2).
const AMPLITUDE: f32 = f32::from_bits(0x3F2A_AA3B);

/// The noise tables *P* and *G* (4.2).
#[derive(Clone, Debug, PartialEq)]
pub struct Tables {
    pub p: [u8; 256],
    pub g: [f32; 256],
}

impl Tables {
    /// The tables built with the C runtime's generator seeded with `seed`
    /// (4.2). The generator is a state of its own here: which other users
    /// share the original's is unestablished (disputed in review), and any
    /// uniform generator serves.
    pub fn build(seed: u32) -> Self {
        let mut rand = Generator::new(seed);
        let mut p = [0u8; 256];
        let mut g = [0f32; 256];
        for i in 0..256 {
            p[i] = i as u8;
            g[i] = ((rand.next_k() % 512) as f32 - 256.0) * 0.003_906_25;
        }
        for i in (1..256).rev() {
            let j = (rand.next_k() % 256) as usize;
            p.swap(i, j);
        }
        Self { p, g }
    }

    /// One noise value *n*(*x*) (4.2): the products but *u* in extended
    /// precision (here `f64`).
    pub fn noise(&self, x: f32) -> f32 {
        let y = x + 4096.0;
        let i = y as i32;
        let f = y - i as f32;
        let i = i.rem_euclid(256) as usize;
        let g0 = self.g[usize::from(self.p[i])];
        let g1 = self.g[usize::from(self.p[(i + 1) % 256])];
        let u = g0 * f;
        let (f, g1, u) = (f64::from(f), f64::from(g1), f64::from(u));
        let v = (f - 1.0) * g1;
        let h = (3.0 - 2.0 * f) * f * f;
        (u + h * (v - u)) as f32
    }
}

/// The environment's wind (4.2, 4.3): kept for the whole run.
#[derive(Clone, Debug, PartialEq)]
pub struct Wind {
    /// The tables of seed *s*, built when the wind is made rather than at
    /// the first evaluation
    /// (4.2): unobservable, and the block stays out of a level's heap.
    tables: Box<Tables>,
    /// *o*₀, *o*₁.
    offsets: [f32; 2],
    /// φ.
    frequency: f32,
    /// *A*.
    amplitude: f32,
    /// τ.
    last: f32,
    /// The wind strength *W*, 0 when made.
    pub strength: f32,
}

/// The draw the wind is made with: the original takes one from the shared
/// generator, whose start differs from run to run, so any seed serves
/// (4.2); a fixed one is our choice.
pub const SEED_DRAW: u32 = 1;

impl Default for Wind {
    fn default() -> Self {
        Self::new(SEED_DRAW)
    }
}

impl Wind {
    /// The wind made with one draw `r` of the shared generator (4.2): seed
    /// *s* = *r* mod 65,535.
    pub fn new(r: u32) -> Self {
        Self {
            tables: Box::new(Tables::build(r % 65_535)),
            offsets: [0.0; 2],
            frequency: 1.0,
            amplitude: AMPLITUDE,
            last: 0.0,
            strength: 0.0,
        }
    }

    pub fn offsets(&self) -> [f32; 2] {
        self.offsets
    }

    /// The continuous value *V*(*T*) at `t` seconds (4.2).
    fn value(&mut self, t: f32) -> f32 {
        let tables = &self.tables;
        let mut d = self.frequency * (t - self.last);
        self.last = t;
        let mut sum = 0f32;
        let mut a = self.amplitude;
        for o in &mut self.offsets {
            *o += d;
            sum = (f64::from(sum) + f64::from(a) * f64::from(tables.noise(*o))) as f32;
            d *= 2.0;
            a *= 0.5;
        }
        sum
    }

    /// The environment's update at global time `t` seconds with the gust
    /// in force `gust` (4.3): φ, then *W*.
    pub fn update(&mut self, gust: f32, t: f32) {
        self.frequency = gust * GUST_SCALE;
        self.strength = (self.value(t) + 0.5).clamp(0.0, 1.0);
    }
}

/// A rain drop's mean direction at its birth (5.1) from the wind strength
/// `w`, the blended `wind_velocity` and `wind_direction` (radians).
pub fn rain_direction(w: f32, velocity: f32, direction: f32) -> [f32; 3] {
    let sigma = (w * 0.1 * velocity * 0.05).clamp(0.0, 1.0);
    let a = 0.174_532_94 * sigma - 1.570_796_4;
    [
        -direction.sin() * a.cos(),
        a.sin(),
        direction.cos() * a.cos(),
    ]
}

/// A sway value of the detail objects (5.2): `normal` · (1 − *W*) + `fast`
/// · *W*.
pub fn sway(normal: f32, fast: f32, w: f32) -> f32 {
    normal * (1.0 - w) + fast * w
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn the_tables_for_seed_12345() {
        // Section 7.2 (M1).
        let t = Tables::build(12_345);
        assert_eq!(t.p[..8], [140, 206, 209, 109, 73, 1, 184, 167]);
        let g: Vec<f32> = (0..8).map(|i| t.g[usize::from(t.p[i])]).collect();
        assert_eq!(
            g,
            [
                -0.144_531_25,
                0.269_531_25,
                0.464_843_75,
                -0.9375,
                -0.90625,
                -0.140_625,
                0.984_375,
                0.285_156_25
            ]
        );
        let lo = t.g.iter().copied().fold(f32::MAX, f32::min);
        let hi = t.g.iter().copied().fold(f32::MIN, f32::max);
        assert_eq!((lo, hi), (-0.988_281_25, 0.984_375));
        let p: Vec<String> = t.p.iter().map(u8::to_string).collect();
        let g: Vec<String> = t.g.iter().map(f32::to_string).collect();
        let text = format!("{}|{}", p.join(","), g.join(","));
        let digest: String = Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            digest,
            "7337863be3df8ecbcab4c614324240af319d092fc0440393281b9dbdcbf5895a"
        );
    }

    #[test]
    fn noise_values() {
        // Section 7.2 (M2).
        let t = Tables::build(12_345);
        let want = [0.0, -0.062_073, -0.103_516, 0.0, -0.048_828, 0.252_228];
        for (x, w) in [0.0, 0.25, 0.5, 1.0, 1.5, 2.75].into_iter().zip(want) {
            assert!((t.noise(x) - w).abs() < 1e-6, "{x}: {}", t.noise(x));
        }
    }

    /// Frames every 20 ms from 100 s with `gusts` (frames, gust) in turn;
    /// the strength after each frame.
    fn run(wind: &mut Wind, gusts: &[(u32, f32)]) -> Vec<f32> {
        let mut out = Vec::new();
        let mut frame = 0u32;
        for &(n, gust) in gusts {
            for _ in 0..n {
                let t = (100_000 + 20 * frame) as f32 / 1000.0;
                wind.update(gust, t);
                out.push(wind.strength);
                frame += 1;
            }
        }
        out
    }

    #[test]
    fn a_short_gust_moves_the_wind_a_little() {
        // Section 7.2 (M3).
        let mut wind = Wind::new(12_345);
        let w = run(&mut wind, &[(50, 0.0), (750, 0.015), (250, 0.0)]);
        assert!(
            w[..51].iter().all(|&w| (w - 0.5).abs() < 5e-7),
            "{:?}",
            &w[..52]
        );
        assert!((w[799] - 0.498_609).abs() < 1e-6, "{}", w[799]);
        assert!(w[800..].iter().all(|&x| x == w[799]));
        let [o0, o1] = wind.offsets();
        assert!((o0 - 0.006_75).abs() < 1e-6 && (o1 - 0.0135).abs() < 1e-6);
    }

    #[test]
    fn an_hour_of_gust() {
        // Section 7.2 (M4).
        let mut wind = Wind::new(12_345);
        let w = run(&mut wind, &[(180_000, 0.03)]);
        let lo = w.iter().copied().fold(f32::MAX, f32::min);
        let hi = w.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            (lo - 0.4144).abs() < 1e-4 && (hi - 0.7338).abs() < 1e-4,
            "{lo} {hi}"
        );
        let [o0, o1] = wind.offsets();
        assert!(
            (o0 - 3.336).abs() < 1e-3 && (o1 - 6.672).abs() < 1e-3,
            "{o0} {o1}"
        );
    }

    #[test]
    fn the_wind_is_half_before_any_gust() {
        // Section 4.3: W = 0 when made, 0.5 after the first update.
        let mut wind = Wind::new(41);
        assert_eq!(wind.strength, 0.0);
        wind.update(0.0, 1.0);
        assert_eq!(wind.strength, 0.5);
    }

    #[test]
    fn rain_directions() {
        // Section 7.2 (M5).
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6);
        let d = rain_direction(0.5, 20.0, 0.0);
        assert!(close(d, [0.0, -0.999_962, 0.008_726]), "{d:?}");
        let d = rain_direction(1.0, 100.0, 90.0 * 0.017_453_292);
        assert!(close(d, [-0.087_156, -0.996_195, 0.0]), "{d:?}");
        let d = rain_direction(0.7, 0.0, 0.0);
        assert!(close(d, [0.0, -1.0, 0.0]), "{d:?}");
    }

    #[test]
    fn sway_values() {
        // Section 5.2's table at W = 0.5 (M6), and the sets at 0 and 1.
        for (normal, fast, half) in [
            (0.1, 0.35, 0.225),
            (0.05, 0.20, 0.125),
            (30.0, 5.0, 17.5),
            (1.0, 0.5, 0.75),
            (2.0, 0.5, 1.25),
        ] {
            assert!((sway(normal, fast, 0.5) - half).abs() < 1e-6);
            assert_eq!(sway(normal, fast, 0.0), normal);
            assert_eq!(sway(normal, fast, 1.0), fast);
        }
    }
}
