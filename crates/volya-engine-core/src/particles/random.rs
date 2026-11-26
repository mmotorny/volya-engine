// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The particles' random numbers (particle-lifecycle.md section 2): the
//! shared uniform generator, the C runtime's generator the normal deviate
//! and the turbulence noise use (P2 2.2; particle-motion-actions.md 4.1),
//! and the noise tables built from the latter.

use std::cell::Cell;
use std::rc::Rc;

use super::noise::Noise;

/// A generator advancing *s* ← *s* · 214013 + 2531011 (mod 2³²) and giving
/// *k* = (*s* >> 16) & 0x7FFF (P2 2.1; the C runtime's, P3 4.1, is the
/// same rule on a state of its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Generator {
    pub state: u32,
}

impl Generator {
    pub fn new(state: u32) -> Self {
        Self { state }
    }

    /// The next *k*, 0 … 32767.
    pub fn next_k(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.state >> 16) & 0x7FFF
    }
}

/// 1/32767 as the nearest `f32`.
const UNIT: f32 = (1.0 / 32767.0) as f32;
/// 2π/32767 as an `f32` constant.
const TURN: f32 = (std::f64::consts::TAU / 32767.0) as f32;

/// The engine's one shared generator (P2 2.1; occlusion-grid.md 4.1,
/// occlusion-tests.md 5, detail-objects.md 5.2): a handle, its clones
/// drawing from the same state, so the level's particles, occlusion and
/// detail objects take their values from it in the order they draw.
#[derive(Clone, Debug)]
pub struct Shared(Rc<Cell<Generator>>);

impl Shared {
    pub fn new(state: u32) -> Self {
        Self(Rc::new(Cell::new(Generator::new(state))))
    }

    /// The next *k*.
    pub fn next_k(&self) -> u32 {
        self.with(Generator::next_k)
    }

    /// The generator as it is now.
    pub fn get(&self) -> Generator {
        self.0.get()
    }

    /// Runs `f` with the generator; `f` must not draw through another
    /// handle.
    pub fn with<R>(&self, f: impl FnOnce(&mut Generator) -> R) -> R {
        let mut g = self.0.get();
        let r = f(&mut g);
        self.0.set(g);
        r
    }

    /// A generator of its own at the same state (copies for tests).
    pub fn detached(&self) -> Self {
        Self::new(self.get().state)
    }
}

/// At state 1 (our choice: the seed is unestablished, occlusion-tests.md 5).
impl Default for Shared {
    fn default() -> Self {
        Self::new(1)
    }
}

impl PartialEq<Generator> for Shared {
    fn eq(&self, other: &Generator) -> bool {
        self.get() == *other
    }
}

/// The particles' generators.
///
/// Our choice where P2 2.1 leaves it unestablished: both generators start
/// at state 1 (the C runtime's documented start) when a [`Random`] is
/// made.
#[derive(Clone, Debug)]
pub struct Random {
    /// The shared generator of P2 2.1.
    pub uniform: Shared,
    /// The C runtime's `rand()` (P2 2.2, P3 4.1).
    pub crt: Generator,
    noise: Option<Box<Noise>>,
}

impl Default for Random {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Random {
    /// Generators with the shared one at `state`, the C runtime's at 1.
    pub fn new(state: u32) -> Self {
        Self::sharing(Shared::new(state))
    }

    /// Generators drawing the shared values from `uniform`, the C
    /// runtime's at 1.
    pub fn sharing(uniform: Shared) -> Self {
        Self {
            uniform,
            crt: Generator::new(1),
            noise: None,
        }
    }

    /// A copy whose shared generator is its own, at the same state.
    pub fn detached(&self) -> Self {
        Self {
            uniform: self.uniform.detached(),
            ..self.clone()
        }
    }

    /// U = *k* · (1/32767), 0 … 1.
    pub fn u(&mut self) -> f32 {
        self.uniform.next_k() as f32 * UNIT
    }

    /// θ = *k* · (2π/32767), 0 … 2π.
    pub fn angle(&mut self) -> f32 {
        self.uniform.next_k() as f32 * TURN
    }

    /// The normal deviate g(σ) (P2 2.2).
    pub fn normal(&mut self, sigma: f32) -> f32 {
        if sigma == 0.0 {
            return 0.0;
        }
        let y = loop {
            let y = -self.u().ln();
            let u2 = self.u();
            let d = y - 1.0;
            // Repeated while U₂ > exp(−(y − 1)²/2).
            let repeat = u2 > (-(d * d) / 2.0).exp();
            if !repeat {
                break y;
            }
        };
        let g = 1.253_918_5 * sigma * y;
        if self.crt.next_k() & 1 == 1 { g } else { -g }
    }

    /// The noise tables, built at the first evaluation (P3 4.1): the C
    /// runtime's generator is seeded with 1 and gives 1,571 values.
    pub(crate) fn noise(&mut self) -> &Noise {
        if self.noise.is_none() {
            self.crt = Generator::new(1);
            self.noise = Some(Box::new(Noise::build(&mut self.crt)));
        }
        self.noise.as_deref().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generator_from_state_1_gives_p2s_values() {
        // P2 9.2.
        let mut g = Generator::new(1);
        let k: Vec<u32> = (0..5).map(|_| g.next_k()).collect();
        assert_eq!(k, [41, 18467, 6334, 26500, 19169]);
    }

    #[test]
    fn uniform_numbers_span_0_to_1() {
        assert_eq!(32767.0 * UNIT, 1.0);
        let mut r = Random::new(1);
        assert_eq!(r.u(), 41.0 * UNIT);
    }

    #[test]
    fn the_shared_generators_handles_draw_in_turn() {
        let a = Shared::new(1);
        let mut r = Random::sharing(a.clone());
        assert_eq!(r.uniform.next_k(), 41);
        assert_eq!(a.with(Generator::next_k), 18467);
        assert_eq!(r.uniform.next_k(), 6334);
        let d = r.detached();
        assert_eq!(d.uniform.next_k(), 26500);
        assert_eq!(a.next_k(), 26500);
    }

    #[test]
    fn a_zero_sigma_draws_nothing() {
        let mut r = Random::new(7);
        assert_eq!(r.normal(0.0), 0.0);
        assert_eq!(r.uniform.get().state, 7);
        assert_eq!(r.crt.state, 1);
    }

    #[test]
    fn the_normal_deviate_is_symmetric_with_its_spread() {
        let mut r = Random::new(12345);
        let n = 20_000;
        let (mut sum, mut sq) = (0.0f64, 0.0f64);
        for _ in 0..n {
            let g = f64::from(r.normal(2.0));
            sum += g;
            sq += g * g;
        }
        let mean = sum / f64::from(n);
        let sd = (sq / f64::from(n) - mean * mean).sqrt();
        assert!(mean.abs() < 0.08, "{mean}");
        // Believed 1.2539185 σ (P2 2.2).
        assert!((sd - 2.0 * 1.2539185).abs() < 0.08, "{sd}");
    }
}
