// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The x87 instructions the shipped library computes its sines, cosines and
//! arc tangents with (docs/behavior/physics-library.md 7), emulated in
//! software for ODE's `sinf`, `cosf` and `atan2f` (csrc/ode/include/ode/
//! config.h routes them here, natively and on wasm32).
//!
//! The instructions give an extended (64-bit significand) result, which the
//! library stores as a float. Each function here computes the value in
//! double-double arithmetic (about 106 bits), rounds it to 64 bits and then
//! to the float, to nearest even both times, as the instruction and the
//! store do. `fsin` and `fcos` reduce their argument by multiples of π/2
//! with the instruction's 66-bit π (`PI66`), exactly; an operand of
//! magnitude 2^63 or more is returned unchanged (the instruction leaves it
//! and reports it out of range). Infinities give the instructions' default
//! NaN; a NaN is returned quieted.
//!
//! Square roots need nothing: at 24-bit precision `fsqrt` of a float is the
//! correctly rounded float square root, which `sqrtf` is.
//!
//! Exact where the hardware's extended result is the correctly rounded one
//! (Intel documents an error under one unit of the extended format, so the
//! float can differ in rare cases within that unit of a rounding boundary:
//! believed; the black-box comparison measures it, #770).

#![allow(unsafe_code)]

/// π rounded to 66 significant bits, as an integer: π ≈ `PI66` · 2⁻⁶⁴, π/2
/// ≈ `PI66` · 2⁻⁶⁵.
const PI66: i128 = 0x3_243F_6A88_85A3_08D3;

/// The x87's default NaN (the "indefinite"), stored as a float.
const INDEFINITE: u32 = 0xFFC0_0000;

/// sin(x) as `fsin` then a float store give it.
pub fn sin(x: f32) -> f32 {
    trig(x, false)
}

/// cos(x) as `fcos` then a float store give it.
pub fn cos(x: f32) -> f32 {
    trig(x, true)
}

/// atan2(y, x) as `fpatan` (of y over x) then a float store give it.
pub fn atan2(y: f32, x: f32) -> f32 {
    if y.is_nan() || x.is_nan() {
        let n = if y.is_nan() { y } else { x };
        return f32::from_bits(n.to_bits() | 0x0040_0000);
    }
    let neg = y.is_sign_negative();
    let a = if y == 0.0 {
        if x.is_sign_negative() { PI } else { ZERO }
    } else if x == 0.0 {
        PI_2
    } else if y.is_infinite() {
        if x.is_infinite() {
            if x < 0.0 { PI_3_4 } else { PI_4 }
        } else {
            PI_2
        }
    } else if x.is_infinite() {
        if x < 0.0 { PI } else { ZERO }
    } else {
        let (ay, ax) = (f64::from(y.abs()), f64::from(x.abs()));
        let a = if ay <= ax {
            atan01(Dd::div(Dd::from(ay), Dd::from(ax)))
        } else {
            PI_2.sub(atan01(Dd::div(Dd::from(ax), Dd::from(ay))))
        };
        if x < 0.0 { PI.sub(a) } else { a }
    };
    let r = round_once(extended(a));
    if neg { -r } else { r }
}

/// sin(x) times m as the shipped library computes it where its sine stays in
/// the x87 register (docs/behavior/physics-harness-2.md 2.2): `fsin`'s
/// extended result, not stored as a float, times the float m, rounded once
/// to a float: the multiplication at the game's 24-bit x87 precision
/// (docs/behavior/physics-library.md 7), which the comparison reproduces.
pub fn sin_times(x: f32, m: f32) -> f32 {
    match trig_extended(x, false) {
        Ok(s) => round_once(s.mul_f64(f64::from(m))),
        Err(s) => s * m,
    }
}

/// sin(x) over d, the same way (physics-harness-2.md 2.6): the extended
/// sine divided by the float d, rounded once to a float.
pub fn sin_over(x: f32, d: f32) -> f32 {
    match trig_extended(x, false) {
        Ok(s) if d != 0.0 && d.is_finite() => round_once(Dd::div(s, Dd::from(f64::from(d)))),
        Ok(s) => round_once(s) / d,
        Err(s) => s / d,
    }
}

fn trig(x: f32, cosine: bool) -> f32 {
    match trig_extended(x, cosine) {
        Ok(v) => round_once(v),
        Err(r) => r,
    }
}

/// The instruction's extended result, exactly; or, for the operands the
/// instruction returns as they are or as NaNs, that float.
fn trig_extended(x: f32, cosine: bool) -> Result<Dd, f32> {
    if x.is_nan() {
        return Err(f32::from_bits(x.to_bits() | 0x0040_0000));
    }
    if x.is_infinite() {
        return Err(f32::from_bits(INDEFINITE));
    }
    if x == 0.0 {
        // sin(±0) = ±0, cos(±0) = 1.
        return Err(if cosine { 1.0 } else { x });
    }
    if x.abs() >= 9.223_372e18 {
        // 2^63: out of the instruction's range.
        return Err(x);
    }
    let (k, r) = reduce(x);
    let (s, c) = (sin_series(r), cos_series(r));
    let q = (k + i64::from(cosine)).rem_euclid(4);
    let v = match q {
        0 => s,
        1 => c,
        2 => s.neg(),
        _ => c.neg(),
    };
    Ok(extended(v))
}

/// x = k·π₆₆/2 + r exactly, |r| ≤ π₆₆/4 (k the nearest multiple).
fn reduce(x: f32) -> (i64, Dd) {
    let xd = f64::from(x);
    let k = (xd / std::f64::consts::FRAC_PI_2).round() as i64;
    if k == 0 {
        return (0, Dd::from(xd));
    }
    // x = m·2^e with m < 2²⁴; |x| ≥ π/4 here, so e + 65 ≥ 0.
    let bits = x.to_bits();
    let (m, e) = (
        i128::from((bits & 0x7F_FFFF) | 0x80_0000),
        ((bits >> 23) & 0xFF) as i32 - 150,
    );
    let m = if x < 0.0 { -m } else { m };
    // In units of 2⁻⁶⁵; the products wrap, the difference is small.
    let xs = m.wrapping_shl((e + 65) as u32);
    let mut k = k;
    let mut r = xs.wrapping_sub(i128::from(k).wrapping_mul(PI66));
    while r > PI66 / 2 {
        r -= PI66;
        k += 1;
    }
    while r < -PI66 / 2 {
        r += PI66;
        k -= 1;
    }
    let hi = r as f64;
    let lo = (r - hi as i128) as f64;
    let scale = 2f64.powi(-65);
    (k, Dd::new(hi * scale, lo * scale))
}

fn sin_series(r: Dd) -> Dd {
    let r2 = r.mul(r);
    let mut t = r;
    let mut sum = r;
    for n in 1..40 {
        let d = f64::from(2 * n) * f64::from(2 * n + 1);
        t = t.mul(r2).div_f64(-d);
        sum = sum.add(t);
        if t.hi.abs() <= sum.hi.abs() * 1e-34 {
            break;
        }
    }
    sum
}

fn cos_series(r: Dd) -> Dd {
    let r2 = r.mul(r);
    let mut t = Dd::from(1.0);
    let mut sum = t;
    for n in 1..40 {
        let d = f64::from(2 * n - 1) * f64::from(2 * n);
        t = t.mul(r2).div_f64(-d);
        sum = sum.add(t);
        if t.hi.abs() <= 1e-34 {
            break;
        }
    }
    sum
}

/// atan(t) for 0 ≤ t ≤ 1.
fn atan01(t: Dd) -> Dd {
    // tan(π/8): above it, atan(t) = π/4 + atan((t − 1)/(t + 1)).
    if t.hi > 0.414_213_562_373_095_1 {
        let one = Dd::from(1.0);
        PI_4.add(atan_series(Dd::div(t.sub(one), t.add(one))))
    } else {
        atan_series(t)
    }
}

fn atan_series(u: Dd) -> Dd {
    let u2 = u.mul(u);
    let mut p = u;
    let mut sum = u;
    for n in 1..80 {
        p = p.mul(u2).neg();
        let term = p.div_f64(f64::from(2 * n + 1));
        sum = sum.add(term);
        if term.hi.abs() <= sum.hi.abs() * 1e-34 {
            break;
        }
    }
    sum
}

/// v rounded to a 64-bit significand, to nearest even (the extended
/// format's result), exactly.
fn extended(v: Dd) -> Dd {
    if v.hi == 0.0 {
        return v;
    }
    let neg = v.hi < 0.0;
    let (h, l) = if neg { (-v.hi, -v.lo) } else { (v.hi, v.lo) };
    // h = mh·2^(eh − 52), normal. Units: 2^(eh − 100), so h is mh·2⁴⁸.
    let hb = h.to_bits();
    let eh = ((hb >> 52) & 0x7FF) as i32 - 1023;
    let mh = i128::from((hb & 0xF_FFFF_FFFF_FFFF) | (1 << 52));
    let mut n = mh << 48;
    // The value is n + f in those units, 0 ≤ f < 1; `sticky` is f > 0.
    let mut sticky = false;
    if l != 0.0 {
        let lb = l.abs().to_bits();
        let el = ((lb >> 52) & 0x7FF) as i32 - 1023;
        let (ml, el) = if el == -1023 {
            (i128::from(lb & 0xF_FFFF_FFFF_FFFF), -1022)
        } else {
            (i128::from((lb & 0xF_FFFF_FFFF_FFFF) | (1 << 52)), el)
        };
        // |l| < ulp(h), so the shift is negative.
        let sh = el - 52 - (eh - 100);
        let down = (-sh) as u32;
        let (whole, frac) = if down >= 127 {
            (0, ml != 0)
        } else {
            (ml >> down, ml & ((1 << down) - 1) != 0)
        };
        if l > 0.0 {
            n += whole;
            sticky = frac;
        } else {
            n -= whole + i128::from(frac);
            sticky = frac;
        }
    }
    // To 64 significant bits.
    let len = 128 - n.leading_zeros() as i32;
    let mut d = len - 64;
    let (q, rem) = (n >> d, n & ((1 << d) - 1));
    let half = 1i128 << (d - 1);
    let up = rem > half || (rem == half && (sticky || q & 1 == 1));
    let mut q = q + i128::from(up);
    if q == 1 << 64 {
        q >>= 1;
        d += 1;
    }
    // The extended value q·2^e2 (e2 well within a double's exponents for
    // the values here): its top 53 bits and the rest.
    let e2 = d + eh - 100;
    let r = Dd::new(
        ((q >> 11) << 11) as f64 * pow2(e2),
        (q & 0x7FF) as f64 * pow2(e2),
    );
    if neg { r.neg() } else { r }
}

/// v rounded once to a float, to nearest even (a store of the extended
/// value, or an operation's result at 24-bit precision).
fn round_once(v: Dd) -> f32 {
    let f = v.hi as f32;
    let fd = f64::from(f);
    if v.lo == 0.0 || !v.hi.is_finite() || fd == v.hi {
        // Exact, or hi a float and lo far below half of its unit.
        return f;
    }
    // hi lies between f and its neighbour on hi's side; only when it is
    // their midpoint does lo decide.
    let away = fd.abs() < v.hi.abs();
    let other = f32::from_bits(if away {
        f.to_bits() + 1
    } else {
        f.to_bits() - 1
    });
    if (v.hi - fd).abs() != (f64::from(other) - v.hi).abs() {
        return f;
    }
    if (v.lo > 0.0) == (v.hi > fd) {
        other
    } else {
        f
    }
}

fn pow2(e: i32) -> f64 {
    f64::from_bits(((e + 1023) as u64) << 52)
}

/// A double-double: hi + lo, |lo| ≤ ulp(hi)/2.
#[derive(Clone, Copy, Debug)]
struct Dd {
    hi: f64,
    lo: f64,
}

const ZERO: Dd = Dd::new(0.0, 0.0);
const PI: Dd = Dd::new(std::f64::consts::PI, 1.224_646_799_147_353_2e-16);
const PI_2: Dd = Dd::new(std::f64::consts::FRAC_PI_2, 6.123_233_995_736_766e-17);
const PI_4: Dd = Dd::new(std::f64::consts::FRAC_PI_4, 3.061_616_997_868_383e-17);
const PI_3_4: Dd = Dd::new(2.356_194_490_192_345, 9.184_850_993_605_148e-17);

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

fn quick_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    Dd::new(s, b - (s - a))
}

fn split(a: f64) -> (f64, f64) {
    let c = 134_217_729.0 * a;
    let hi = c - (c - a);
    (hi, a - hi)
}

fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
}

impl Dd {
    const fn new(hi: f64, lo: f64) -> Self {
        Self { hi, lo }
    }

    fn from(v: f64) -> Self {
        Self::new(v, 0.0)
    }

    fn neg(self) -> Self {
        Self::new(-self.hi, -self.lo)
    }

    fn add(self, b: Self) -> Self {
        let (s, e) = two_sum(self.hi, b.hi);
        let (t, f) = two_sum(self.lo, b.lo);
        let r = quick_two_sum(s, e + t);
        quick_two_sum(r.hi, r.lo + f)
    }

    fn sub(self, b: Self) -> Self {
        self.add(b.neg())
    }

    fn mul(self, b: Self) -> Self {
        let (p, e) = two_prod(self.hi, b.hi);
        quick_two_sum(p, e + (self.hi * b.lo + self.lo * b.hi))
    }

    fn mul_f64(self, b: f64) -> Self {
        let (p, e) = two_prod(self.hi, b);
        quick_two_sum(p, e + self.lo * b)
    }

    fn div_f64(self, b: f64) -> Self {
        let q1 = self.hi / b;
        let r = self.sub(Self::from(b).mul_f64(q1));
        let q2 = r.hi / b;
        let r = r.sub(Self::from(b).mul_f64(q2));
        let q3 = r.hi / b;
        quick_two_sum(q1, q2).add(Self::from(q3))
    }

    fn div(a: Self, b: Self) -> Self {
        let q1 = a.hi / b.hi;
        let r = a.sub(b.mul_f64(q1));
        let q2 = r.hi / b.hi;
        let r = r.sub(b.mul_f64(q2));
        let q3 = r.hi / b.hi;
        quick_two_sum(q1, q2).add(Self::from(q3))
    }
}

/// ODE's `sinf` (config.h).
#[unsafe(no_mangle)]
pub extern "C" fn volya_x87_sinf(x: f32) -> f32 {
    sin(x)
}

/// ODE's `cosf` (config.h).
#[unsafe(no_mangle)]
pub extern "C" fn volya_x87_cosf(x: f32) -> f32 {
    cos(x)
}

/// ODE's `atan2f` (config.h).
#[unsafe(no_mangle)]
pub extern "C" fn volya_x87_atan2f(y: f32, x: f32) -> f32 {
    atan2(y, x)
}

/// sin(x) · m, the sine unrounded (config.h; [`sin_times`]).
#[unsafe(no_mangle)]
pub extern "C" fn volya_x87_sinf_times(x: f32, m: f32) -> f32 {
    sin_times(x, m)
}

/// sin(x) / d, the sine unrounded (config.h; [`sin_over`]).
#[unsafe(no_mangle)]
pub extern "C" fn volya_x87_sinf_over(x: f32, d: f32) -> f32 {
    sin_over(x, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Correctly rounded single-precision values (from f64, whose results
    /// are within half an ulp of 2⁻⁵³ relative, so rounding them is the
    /// float's correct rounding but in rare ties).
    fn reference(v: f64) -> f32 {
        v as f32
    }

    #[test]
    fn agree_with_correct_rounding_but_rarely() {
        let mut s: u32 = 12345;
        let mut next = || {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            s
        };
        let mut diff = [0; 3];
        let n = 200_000;
        for _ in 0..n {
            // Angles of a physics step: within ±8π.
            let x = (next() as f32 / u32::MAX as f32 - 0.5) * 50.0;
            let y = (next() as f32 / u32::MAX as f32 - 0.5) * 50.0;
            let xd = f64::from(x);
            diff[0] += usize::from(sin(x) != reference(xd.sin()));
            diff[1] += usize::from(cos(x) != reference(xd.cos()));
            diff[2] += usize::from(atan2(y, x) != reference(f64::from(y).atan2(xd)));
        }
        // Differences are 1-ulp ties of the double rounding or the 66-bit π
        // near zeros of sin and cos.
        for d in diff {
            assert!(d * 10_000 < n, "{diff:?}");
        }
    }

    #[test]
    fn known_values() {
        assert_eq!(sin(1.0).to_bits(), 0x3F57_6AA4);
        assert_eq!(cos(0.0), 1.0);
        assert_eq!(sin(-0.0).to_bits(), 0x8000_0000);
        assert_eq!(sin(1e-40), 1e-40);
        assert_eq!(atan2(1.0, 1.0), std::f32::consts::FRAC_PI_4);
        assert_eq!(atan2(0.0, -1.0), std::f32::consts::PI);
        assert_eq!(atan2(-0.0, -0.0), -std::f32::consts::PI);
        assert_eq!(atan2(1.0, 0.0), std::f32::consts::FRAC_PI_2);
        assert_eq!(atan2(1e-30, 1e10), 1e-40);
        assert_eq!(sin(f32::INFINITY).to_bits(), INDEFINITE);
        assert_eq!(sin(1e19), 1e19);
    }

    #[test]
    fn the_unrounded_sine_is_rounded_once_with_its_operand() {
        let mut s: u32 = 777;
        let mut next = || {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            s as f32 / u32::MAX as f32
        };
        let (mut off, mut times_differ, mut over_differ) = (0, 0, 0);
        let n = 100_000;
        for _ in 0..n {
            let x = (next() - 0.5) * 8.0;
            let m = 0.05 + next() * 3.0;
            // A factor of one leaves the stored sine.
            assert_eq!(sin_times(x, 1.0).to_bits(), sin(x).to_bits(), "{x}");
            let (xd, md) = (f64::from(x), f64::from(m));
            off += usize::from(sin_times(x, m) != reference(xd.sin() * md));
            off += usize::from(sin_over(x, m) != reference(xd.sin() / md));
            times_differ += usize::from(sin_times(x, m) != sin(x) * m);
            over_differ += usize::from(sin_over(x, m) != sin(x) / m);
        }
        // Double rounding in the reference, rarely.
        assert!(off * 10_000 < n, "{off}");
        // Rounding the sine first is another rule.
        assert!(
            times_differ * 20 > n && over_differ * 20 > n,
            "{times_differ} {over_differ}"
        );
        assert_eq!(sin_times(0.0, 2.0), 0.0);
        assert_eq!(sin_times(-0.0, 2.0).to_bits(), 0x8000_0000);
        assert!(sin_over(f32::NAN, 2.0).is_nan());
    }

    #[test]
    fn rounding_once_breaks_ties_by_the_low_part() {
        // 1 + 2^-24, the midpoint of 1 and its successor: a little above,
        // below, and exactly (to even).
        let mid = 1.0 + 2f64.powi(-24);
        let up = f32::from_bits(0x3F80_0001);
        assert_eq!(round_once(Dd::new(mid, 1e-30)), up);
        assert_eq!(round_once(Dd::new(mid, -1e-30)), 1.0);
        assert_eq!(round_once(Dd::new(mid, 0.0)), 1.0);
        assert_eq!(round_once(Dd::new(-mid, -1e-30)), -up);
    }

    #[test]
    fn the_66_bit_pi_shows_near_multiples_of_pi() {
        // sin(x) for x the float nearest π: π − x ≈ −8.74e-8, and the
        // 66-bit π moves the result by (π₆₆ − π)·1 relative to it, far
        // below a float's unit: the result is the correctly rounded one.
        let x = std::f32::consts::PI;
        assert_eq!(sin(x), reference(f64::from(x).sin()));
        // Near a large multiple the reduction still matches the integers'.
        let big = 1_000_000.0 * std::f32::consts::PI;
        assert!(sin(big).is_finite());
    }
}
