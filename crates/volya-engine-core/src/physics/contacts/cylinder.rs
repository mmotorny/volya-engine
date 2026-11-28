// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's cylinder colliders (docs/behavior/physics-contacts.md 4.3):
//! cylinder–sphere and cylinder–box, in single precision with the stated
//! groupings of every sum (`(y + z) + x` is [`dot`], source order
//! [`dot_src`]). An x87 operation equals the single-precision one in the
//! normal range (physics-harness-3.md's definition), so it is computed as
//! one: below the normal range our results may differ from the game's in
//! the last place. Cylinder–cylinder and cylinder–plane are open
//! (TODO(#957)): none.
//!
//! Also the cylinder's rim points the level's contacts use (3.3).

use crate::collision::geometry::Vec3;

use super::RawContact;

/// The rim's extra points: below this `ρ` (4.3) or above this `|c|` (3.3).
pub const RIM_LIMIT: f32 = 0.707_106_77;
/// The ±120° turn's sine (bit pattern 0x3f5db3d7).
const SIN_120: f32 = 0.866_025_4;
/// `normalise`'s least squared length (bit pattern 0x37480000).
const NORMALISE_LEAST: f32 = 1.192_092_9e-5;

/// A cylinder in the world: centre `p`, the rotation's columns `c0`, `a`
/// (the axis) and `c2`, radius and length (4.3's notation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cylinder {
    pub p: Vec3,
    pub c0: Vec3,
    pub a: Vec3,
    pub c2: Vec3,
    pub r: f32,
    pub l: f32,
}

impl Cylinder {
    /// A cylinder about the unit `axis`, its rotation the least turn taking
    /// +y to the axis (our choice: how the game builds a shell cylinder's
    /// rotation from the bone data's direction is open, TODO(#1054); the
    /// player's torso, along +y, is unrotated).
    pub fn new(p: Vec3, axis: Vec3, l: f32, r: f32) -> Self {
        let [x, y, z] = axis;
        let (c0, c2) = if axis == [0.0, 1.0, 0.0] {
            ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
        } else if 1.0 + y < 1e-6 {
            // Half a turn about x.
            ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0])
        } else {
            let k = 1.0 / (1.0 + y);
            (
                [1.0 - x * x * k, -x, -x * z * k],
                [-x * z * k, -z, 1.0 - z * z * k],
            )
        };
        Self {
            p,
            c0,
            a: axis,
            c2,
            r,
            l,
        }
    }

    /// `C_ij`: row i of column j.
    fn m(&self, i: usize, j: usize) -> f32 {
        [self.c0, self.a, self.c2][j][i]
    }

    /// The rim point furthest along `n` (4.3, codes 1 – 3 and 5 – 7: the
    /// projection's length exactly 0 taken as 1) and its cap's centre.
    pub fn rim(&self, n: Vec3) -> (Vec3, Vec3, f32, f32) {
        let (r, h) = (self.r, self.l * 0.5);
        let g0 = dot_src(self.c0, n);
        let g2 = dot(self.c2, n);
        let mut s = (g2 * g2 + g0 * g0).sqrt();
        if s == 0.0 {
            s = 1.0;
        }
        let (h0, h2) = ((1.0 / s) * g0, (1.0 / s) * g2);
        let sigma = side(dot(self.a, n));
        let e: Vec3 = std::array::from_fn(|i| sigma * self.a[i] * h + self.p[i]);
        let m = |i, j| self.m(i, j);
        let k = [
            ((h2 * m(0, 2)) * r) + ((m(0, 0) * h0) * r + e[0]),
            ((m(1, 2) * h2) * r) + ((m(1, 0) * h0) * r + e[1]),
            ((h2 * r) * m(2, 2)) + ((m(2, 0) * h0) * r + e[2]),
        ];
        (k, e, h0, h2)
    }

    /// The two rim points turned by +120° and −120° from the one of
    /// [`Cylinder::rim`] (4.3, codes 1 – 3), with their cap-plane
    /// coordinates `(X, Z)`.
    pub fn turned(&self, e: Vec3, h0: f32, h2: f32) -> [(Vec3, f32, f32); 2] {
        let r = self.r;
        let m = |i, j| self.m(i, j);
        let at = |x: f32, z: f32| {
            let p = [
                ((x * m(0, 0)) + (z * m(0, 2))) + e[0],
                ((z * m(1, 2)) + (m(1, 0) * x)) + e[1],
                ((z * m(2, 2)) + (m(2, 0) * x)) + e[2],
            ];
            (p, x, z)
        };
        [
            at(
                ((h0 * -0.5) - (h2 * SIN_120)) * r,
                ((h0 * SIN_120) - (h2 * 0.5)) * r,
            ),
            at(
                ((h2 * SIN_120) - (h0 * 0.5)) * r,
                ((h2 * -0.5) - (h0 * SIN_120)) * r,
            ),
        ]
    }
}

/// `u·v` summed `(y + z) + x`.
pub fn dot(u: Vec3, v: Vec3) -> f32 {
    (u[1] * v[1] + u[2] * v[2]) + u[0] * v[0]
}

/// `u·v` in source order, `(x + y) + z`.
pub fn dot_src(u: Vec3, v: Vec3) -> f32 {
    (u[0] * v[0] + u[1] * v[1]) + u[2] * v[2]
}

/// +1 when `x > 0`, else −1 (a NaN gives −1).
fn side(x: f32) -> f32 {
    if x > 0.0 { 1.0 } else { -1.0 }
}

/// +1 when `x ≤ 0` (a NaN gives +1), else −1.
fn towards(x: f32) -> f32 {
    if x > 0.0 { -1.0 } else { 1.0 }
}

fn cross(k: Vec3, b: Vec3) -> Vec3 {
    [
        k[1] * b[2] - k[2] * b[1],
        k[2] * b[0] - k[0] * b[2],
        k[0] * b[1] - k[1] * b[0],
    ]
}

/// physics-library.md 3.5's normalisation.
pub fn normalise(v: Vec3) -> Vec3 {
    let l = (v[1] * v[1] + v[2] * v[2]) + v[0] * v[0];
    if l >= NORMALISE_LEAST || l.is_nan() {
        let k = 1.0 / l.sqrt();
        return v.map(|x| x * k);
    }
    normalise_slow(v)
}

/// `dNormalize3_slow` (physics-library.md 3.5): by the largest magnitude
/// (the lowest index among equals), then by the reciprocal square root.
fn normalise_slow(a: Vec3) -> Vec3 {
    let m = a.map(f32::abs);
    let k = if m[1] > m[0] {
        if m[2] > m[1] { 2 } else { 1 }
    } else if m[2] > m[0] {
        2
    } else {
        0
    };
    if m[k] <= 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let (o1, o2) = ((k + 1) % 3, (k + 2) % 3);
    let mut v = a;
    let inv = 1.0 / m[k];
    v[o1] *= inv;
    v[o2] *= inv;
    v[k] = if a[k] < 0.0 { -1.0 } else { 1.0 };
    let s = 1.0 / (v[o1] * v[o1] + v[o2] * v[o2] + 1.0).sqrt();
    v.map(|x| x * s)
}

/// The largest separation so far and its axis (4.3's "best").
struct Best {
    sep: f32,
    vec: Option<Vec3>,
    code: usize,
}

impl Best {
    fn new() -> Self {
        Self {
            sep: f32::NEG_INFINITY,
            vec: None,
            code: 0,
        }
    }

    /// An axis: `false` when it separates (the collider ends).
    fn axis(&mut self, code: usize, sep: f32, vec: Vec3, test: f32) -> bool {
        if sep > 0.0 {
            return false;
        }
        if sep > self.sep {
            self.sep = sep;
            self.vec = Some(if test < 0.0 { vec.map(|x| -x) } else { vec });
            self.code = code;
        }
        true
    }
}

/// 4.3, cylinder–sphere: at most one contact, the cylinder first, the
/// sphere (centre `q`, radius `rs`) second.
pub fn sphere(c: &Cylinder, q: Vec3, rs: f32) -> Option<RawContact> {
    let (p, a, r) = (c.p, c.a, c.r);
    let hl = c.l * 0.5;
    let d: Vec3 = std::array::from_fn(|i| q[i] - p[i]);
    let mut best = Best::new();
    // 1. The axis.
    let s1 = dot(a, d);
    if !best.axis(1, s1.abs() - (rs + hl), a, s1) {
        return None;
    }
    // 2. From the axis to the centre.
    let t = dot(a, q) - dot(a, p);
    let n2 = normalise(std::array::from_fn(|i| d[i] - a[i] * t));
    let s2 = dot(n2, d);
    if !best.axis(2, s2.abs() - (rs + r), n2, s2) {
        return None;
    }
    // 3. From the rim point to the centre.
    let u = normalise(d);
    let g0 = dot_src(c.c0, u);
    let g2 = dot(c.c2, u);
    let k = 1.0 / (g2 * g2 + g0 * g0).sqrt();
    let (h0, h2) = (g0 * k, g2 * k);
    let sigma = side(dot(a, u));
    let m = |i, j| c.m(i, j);
    let rim = [
        ((h2 * r) * m(0, 2)) + ((sigma * m(0, 1)) * hl + ((h0 * m(0, 0)) * r + p[0])),
        ((m(1, 2) * h2) * r) + ((sigma * m(1, 1)) * hl + ((h0 * m(1, 0)) * r + p[1])),
        ((m(2, 2) * h2) * r) + ((sigma * m(2, 1)) * hl + ((h0 * m(2, 0)) * r + p[2])),
    ];
    let n3 = normalise(std::array::from_fn(|i| q[i] - rim[i]));
    let s3 = dot(n3, d);
    let (f0, f2, f) = (dot(c.c0, n3), dot(c.c2, n3), dot(a, n3));
    let sep = s3.abs() - ((((f2 * f2 + f0 * f0).sqrt() * r) + (f.abs() * hl)) + rs);
    if !best.axis(3, sep, n3, s3) {
        return None;
    }
    let m = best.vec?;
    Some(RawContact {
        position: std::array::from_fn(|i| q[i] - m[i] * rs),
        normal: m.map(|x| -x),
        depth: -best.sep,
    })
}

/// 4.3, cylinder–box: 1 to 3 contacts, the cylinder first, the box (centre
/// `q`, axes `b`, half sides `e`) second; appended to `out`.
pub fn cuboid(c: &Cylinder, q: Vec3, b: [Vec3; 3], e: Vec3, out: &mut Vec<RawContact>) {
    let (p, a, r) = (c.p, c.a, c.r);
    let h = c.l * 0.5;
    let d: Vec3 = std::array::from_fn(|i| q[i] - p[i]);
    let d0 = dot_src(c.c0, d);
    let da = dot(a, d);
    let d2 = dot(c.c2, d);
    let x0: Vec3 = std::array::from_fn(|k| {
        if k == 0 {
            dot_src(c.c0, b[k])
        } else {
            dot(c.c0, b[k])
        }
    });
    let x2: Vec3 = std::array::from_fn(|k| dot(c.c2, b[k]));
    let xa: Vec3 = std::array::from_fn(|k| dot(a, b[k]));
    let ak = xa.map(f32::abs);
    let rho = [
        (ak[1] * ak[1] + ak[2] * ak[2]).sqrt(),
        (ak[0] * ak[0] + ak[2] * ak[2]).sqrt(),
        (ak[0] * ak[0] + ak[1] * ak[1]).sqrt(),
    ];
    let beta_sum = |u: Vec3| {
        let bk: Vec3 = std::array::from_fn(|k| dot(b[k], u) * e[k]);
        bk[0].abs() + (bk[1].abs() + bk[2].abs())
    };
    // The box's vertex towards `n`'s far side: σk = +1 when b_k·n ≤ 0.
    let vertex = |n: Vec3| -> Vec3 {
        let s: Vec3 = std::array::from_fn(|k| towards(dot(b[k], n)));
        std::array::from_fn(|i| {
            ((q[i] + s[0] * (e[0] * b[0][i])) + s[1] * (e[1] * b[1][i])) + s[2] * (e[2] * b[2][i])
        })
    };
    let mut best = Best::new();
    // 0: the axis.
    let sep = da.abs() - ((((ak[2] * e[2]) + (ak[1] * e[1])) + (ak[0] * e[0])) + h);
    if !best.axis(0, sep, a, da) {
        return;
    }
    // 1 – 3: a box face.
    for k in 0..3 {
        let sk = dot(b[k], d);
        let sep = sk.abs() - (((ak[k] * h) + (rho[k] * r)) + e[k]);
        if !best.axis(k + 1, sep, b[k], sk) {
            return;
        }
    }
    // 4: a box vertex against the side.
    let t = dot(a, q) - dot(a, p);
    let u4 = normalise(std::array::from_fn(|i| d[i] - a[i] * t));
    let v4 = vertex(u4);
    let t = dot(a, v4) - dot(a, p);
    let u = normalise(std::array::from_fn(|i| (v4[i] - p[i]) - a[i] * t));
    let s = dot(u, d);
    if !best.axis(4, s.abs() - (beta_sum(u) + r), u, s) {
        return;
    }
    // 5 – 7: a box edge against the rim.
    let tangent = |n: Vec3| -> Vec3 {
        let (alpha, beta) = (dot(c.c0, n), dot(c.c2, n));
        std::array::from_fn(|i| alpha * c.m(i, 2) - beta * c.m(i, 0))
    };
    let edge_normal = |k: usize, dots: fn(Vec3, Vec3) -> f32| -> Vec3 {
        let t = dots(b[k], p) - dots(b[k], q);
        normalise(std::array::from_fn(|i| (b[k][i] * t - p[i]) + q[i]))
    };
    let n5 = edge_normal(0, dot_src);
    let n6 = edge_normal(1, dot);
    let t6 = tangent(n6);
    let n7 = normalise(t6);
    for (code, (tn, bk)) in [(tangent(n5), b[0]), (t6, b[1]), (tangent(n7), b[2])]
        .into_iter()
        .enumerate()
    {
        let code = code + 5;
        let u = normalise(cross(tn, bk));
        let s = dot(u, d);
        let g0 = if code == 7 {
            dot_src(c.c0, u)
        } else {
            dot(c.c0, u)
        };
        let (g2, ga) = (dot(c.c2, u), dot(a, u));
        let sep = s.abs() - (((g2 * g2 + g0 * g0).sqrt() * r) + ((ga.abs() * h) + beta_sum(u)));
        if !best.axis(code, sep, u, s) {
            return;
        }
    }
    // 8 – 10: a box edge against the side.
    for (k, (i, j)) in [(1, 2), (0, 2), (0, 1)].into_iter().enumerate() {
        let big_e = (d0 * x2[k]) - (d2 * x0[k]);
        let test = big_e.abs() - (((ak[i] * e[j]) + (ak[j] * e[i])) + r);
        if test > 0.0 {
            return;
        }
        let lambda = ((-x0[k]) * (-x0[k]) + x2[k] * x2[k]).sqrt();
        if lambda > 0.0 {
            let inv = 1.0 / lambda;
            let v = [inv * x2[k], inv * 0.0, inv * (-x0[k])];
            let m = |i, j| c.m(i, j);
            let n = [
                (m(0, 0) * v[0] + m(0, 1) * v[1]) + m(0, 2) * v[2],
                (m(1, 1) * v[1] + m(1, 2) * v[2]) + m(1, 0) * v[0],
                (m(2, 1) * v[1] + m(2, 2) * v[2]) + m(2, 0) * v[0],
            ];
            if !best.axis(8 + k, inv * test, n, big_e) {
                return;
            }
        }
    }
    let Some(n) = best.vec else {
        return;
    };
    let depth = -best.sep;
    let normal = n.map(|x| -x);
    let contact = |position, depth| RawContact {
        position,
        normal,
        depth,
    };
    match best.code {
        0 => out.push(contact(vertex(n), depth)),
        4 => out.push(contact(v4, depth)),
        1..=7 => {
            let (k_point, cap, h0, h2) = c.rim(n);
            out.push(contact(k_point, depth));
            let k = best.code.wrapping_sub(1);
            if k < 3 && rho[k] < RIM_LIMIT {
                let delta = -best.sep - (rho[k] * r);
                let (w0, w2) = (x0[k].abs(), x2[k].abs());
                for (point, x, z) in c.turned(cap, h0, h2) {
                    let depth = ((z * w2) + (x * w0)) + delta;
                    if depth > 0.0 {
                        out.push(contact(point, depth));
                    }
                }
            }
        }
        code => {
            // The midpoint of the closest points of the rim's line along
            // the axis and the box's edge through code 0's vertex.
            let bk = b[code - 8];
            let g0 = dot_src(c.c0, n);
            let g2 = dot(c.c2, n);
            let s = (g2 * g2 + g0 * g0).sqrt();
            let (h0, h2) = ((1.0 / s) * g0, (1.0 / s) * g2);
            let sigma = side(dot(a, n));
            let m = |i, j| c.m(i, j);
            let kp = [
                ((h2 * r) * m(0, 2)) + (sigma * m(0, 1) * h + ((m(0, 0) * h0) * r + p[0])),
                ((m(1, 2) * h2) * r) + (sigma * m(1, 1) * h + ((m(1, 0) * h0) * r + p[1])),
                ((h2 * r) * m(2, 2)) + (sigma * m(2, 1) * h + ((m(2, 0) * h0) * r + p[2])),
            ];
            let w = vertex(n);
            let dd: Vec3 = std::array::from_fn(|i| w[i] - kp[i]);
            let cc = dot(a, bk);
            let q1 = dot(a, dd);
            let q2 = -dot(bk, dd);
            let den = 1.0 - cc * cc;
            let (t1, t2) = if den > 0.0 || den.is_nan() {
                (
                    ((q2 * cc) + q1) * (1.0 / den),
                    ((q1 * cc) + q2) * (1.0 / den),
                )
            } else {
                (0.0, 0.0)
            };
            let point =
                std::array::from_fn(|i| (((t1 * a[i]) + kp[i]) + ((t2 * bk[i]) + w[i])) * 0.5);
            out.push(contact(point, depth));
        }
    }
}
