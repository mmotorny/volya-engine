// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The noise *N* and fractal sum *F* of `turbulence`
//! (particle-motion-actions.md sections 4.1, 4.2).

use super::Vec3;
use super::random::Generator;

const ENTRIES: usize = 514;

/// The gradient and index tables.
#[derive(Clone, Debug)]
pub struct Noise {
    pub(crate) gradients: [Vec3; ENTRIES],
    pub(crate) indices: [i32; ENTRIES],
}

/// One coordinate's lattice values.
struct Axis {
    b0: usize,
    b1: usize,
    rho: f32,
    sigma: f32,
}

/// *L*(*s*, *a*, *b*) = *a* + *s* (*b* − *a*).
fn lerp(s: f32, a: f32, b: f32) -> f32 {
    a + s * (b - a)
}

impl Noise {
    /// Builds the tables from `crt`, already seeded (P3 4.1).
    pub fn build(crt: &mut Generator) -> Self {
        let mut gradients = [[0.0f32; 3]; ENTRIES];
        for g in gradients.iter_mut().take(256) {
            *g = loop {
                let a = [0, 1, 2].map(|_| ((crt.next_k() % 512) as f32 - 256.0) / 256.0);
                let l2 = a[0] * a[0] + a[1] * a[1] + a[2] * a[2];
                if l2 > 1.0 {
                    continue;
                }
                let inv = 1.0 / l2.sqrt();
                break a.map(|c| c * inv);
            };
        }
        let mut indices = [0i32; ENTRIES];
        for (k, p) in indices.iter_mut().enumerate().take(256) {
            *p = k as i32;
        }
        indices[256] = 0;
        for k in (2..=256).rev().step_by(2) {
            let j = (crt.next_k() % 256) as usize;
            indices.swap(k, j);
        }
        for k in 0..=257 {
            gradients[256 + k] = gradients[k];
            indices[256 + k] = indices[k];
        }
        Self { gradients, indices }
    }

    fn axis(c: f32) -> Option<Axis> {
        let t = c + 10000.0;
        // The rule holds for −2³¹ ≤ t < 2³¹; the program gives NaN at and
        // beyond 2³¹ and at −2³² and below. Between −2³² and −2³¹ it is
        // unestablished: NaN here too (our choice).
        if !(-2_147_483_648.0..2_147_483_648.0).contains(&t) {
            return None;
        }
        let b = t.floor();
        let rho = t - b;
        let b0 = (b as i64).rem_euclid(256) as usize;
        Some(Axis {
            b0,
            b1: (b0 + 1) % 256,
            rho,
            sigma: rho * rho * (3.0 - 2.0 * rho),
        })
    }

    /// *N* at `p`.
    pub fn value(&self, p: Vec3) -> f32 {
        let (Some(x), Some(y), Some(z)) = (Self::axis(p[0]), Self::axis(p[1]), Self::axis(p[2]))
        else {
            return f32::NAN;
        };
        let pick = |a: &Axis, i: usize| if i == 0 { a.b0 } else { a.b1 };
        let corner = |i: usize, j: usize, k: usize| {
            let a = self.indices[pick(&x, i)] as usize + pick(&y, j);
            let g = self.gradients[self.indices[a] as usize + pick(&z, k)];
            let d = [x.rho - i as f32, y.rho - j as f32, z.rho - k as f32];
            g[0] * d[0] + g[1] * d[1] + g[2] * d[2]
        };
        let near = lerp(
            y.sigma,
            lerp(x.sigma, corner(0, 0, 0), corner(1, 0, 0)),
            lerp(x.sigma, corner(0, 1, 0), corner(1, 1, 0)),
        );
        let far = lerp(
            y.sigma,
            lerp(x.sigma, corner(0, 0, 1), corner(1, 0, 1)),
            lerp(x.sigma, corner(0, 1, 1), corner(1, 1, 1)),
        );
        1.5 * lerp(z.sigma, near, far)
    }

    /// The fractal sum *F*(**q**) with frequency `f0` and `octaves` (4.2).
    pub fn fractal(&self, q: Vec3, f0: f32, octaves: i32) -> f32 {
        let mut sum = 0.0f32;
        let mut f = f0;
        for _ in 0..octaves.max(0) {
            sum += self.value(q.map(|c| f * c)) / f;
            f *= 2.059;
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md5(bytes: &[u8]) -> String {
        use md5::{Digest, Md5};
        Md5::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn noise() -> Noise {
        Noise::build(&mut Generator::new(1))
    }

    #[test]
    fn the_tables_are_p3s() {
        // P3 4.1.
        let mut crt = Generator::new(1);
        let n = Noise::build(&mut crt);
        let mut count = Generator::new(1);
        (0..1571).for_each(|_| {
            count.next_k();
        });
        assert_eq!(crt, count, "1,571 values taken");
        assert_eq!(&n.indices[..8], &[0, 1, 109, 3, 175, 5, 24, 7]);
        assert_eq!(n.indices[253], 0);
        assert!(!n.indices[..256].contains(&253));
        for (got, want) in n.gradients[0]
            .iter()
            .zip([0.761_481, -0.178_833, 0.623_030])
        {
            assert!((got - want).abs() < 1e-6);
        }
        let p: Vec<u8> = n.indices.iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(md5(&p), "b636928c5f7a237aa0fbaf21637389f3");
        let g: Vec<u8> = (n.gradients.iter().flatten())
            .flat_map(|v| v.to_le_bytes())
            .collect();
        assert_eq!(md5(&g), "44b86fb2ff90d4fa8903df93ef531f24");
    }

    #[test]
    fn values_at_points_are_p3s() {
        let n = noise();
        for (p, want) in [
            ([0.3, 0.2, 0.1], -0.006_784_7),
            ([0.5, 0.5, 0.5], -0.101_803_3),
            ([1.25, -2.5, 3.75], 0.311_547_7),
            ([10.1, 20.2, 30.3], -0.399_265_9),
            ([-7.3, 0.6, 2.2], 0.451_740_9),
            ([-10000.5, 0.25, 0.5], 0.076_217_9),
            ([123.456, -78.9, 0.001], -0.219_859),
        ] {
            let got = n.value(p);
            assert!((got - want).abs() < 2e-7, "{p:?}: {got} vs {want}");
        }
        assert_eq!(n.value([3.0, -4.0, 7.0]), 0.0);
        assert!(n.value([3e9, 0.0, 0.0]).is_nan());
        assert!(n.value([f32::INFINITY, 0.0, 0.0]).is_nan());
        assert_eq!(n.fractal([0.3, 0.2, 0.1], 1.0, 0), 0.0);
    }
}
