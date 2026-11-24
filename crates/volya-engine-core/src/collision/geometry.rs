// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Contact generation between the player's shapes and one triangle, and the
//! step probe's box test (docs/formats/level-cform.md sections 2.3, 4.1,
//! 5.1, 6.3). The spec gives the shapes and the front-side rule, not how
//! the original computes a contact's normal and depth; the methods here are
//! the usual ones (closest points, separating axes) and our choice.

pub type Vec3 = [f32; 3];

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: Vec3, k: f32) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The spec's cross product (level-cform.md, conventions).
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

/// `a / |a|`, or `None` for a (near) zero vector.
pub fn normalize(a: Vec3) -> Option<Vec3> {
    let l = length(a);
    (l > 1e-12).then(|| scale(a, 1.0 / l))
}

/// The front normal `normalize((v1 − v0) × (v2 − v1))` (section 2.3), or
/// `None` for a zero-area triangle (skipped: section 2.4's natural choice).
pub fn front_normal(t: &[Vec3; 3]) -> Option<Vec3> {
    normalize(cross(sub(t[1], t[0]), sub(t[2], t[1])))
}

/// The point of triangle `t` closest to `p` (the standard Voronoi-region
/// method).
pub fn closest_point(p: Vec3, t: &[Vec3; 3]) -> Vec3 {
    let [a, b, c] = *t;
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, scale(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return add(b, scale(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    add(a, add(scale(ab, vb * denom), scale(ac, vc * denom)))
}

/// A contact of a shape with a triangle: `normal` points from the surface
/// towards the body, `depth` is the penetration along it, `point` is on the
/// triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawContact {
    pub normal: Vec3,
    pub depth: f32,
    pub point: Vec3,
}

/// Where the shape's centre lies against the triangle's plane (section
/// 4.1): `Some(true)` in front, `Some(false)` crossed from front to back
/// since `previous`, `None` behind now and before (no contact).
fn side(n: Vec3, t: &[Vec3; 3], centre: Vec3, previous: Vec3) -> Option<(bool, f32, f32)> {
    let s = dot(n, sub(centre, t[0]));
    let s_prev = dot(n, sub(previous, t[0]));
    if s >= 0.0 {
        Some((true, s, s_prev))
    } else if s_prev >= 0.0 {
        Some((false, s, s_prev))
    } else {
        None
    }
}

/// Whether the segment from `previous` (in front, `s_prev ≥ 0`) to
/// `centre` (behind, `s < 0`) passes through the triangle.
fn crosses(t: &[Vec3; 3], previous: Vec3, centre: Vec3, s: f32, s_prev: f32) -> bool {
    let k = s_prev / (s_prev - s);
    let x = add(previous, scale(sub(centre, previous), k));
    length(sub(closest_point(x, t), x)) < 1e-4
}

/// Contact of a sphere (centre `centre`, radius `r`; centre `previous` at
/// the end of the previous step) with triangle `t`, front side only.
///
/// A centre that crossed the plane to the back during the step is pushed
/// back out along the front normal only when it crossed inside the face;
/// otherwise only an edge or corner within reach gives a contact, as from
/// the front (our choice, as are the normal and depth).
pub fn sphere(centre: Vec3, r: f32, previous: Vec3, t: &[Vec3; 3]) -> Option<RawContact> {
    let n = front_normal(t)?;
    let (front, s, s_prev) = side(n, t, centre, previous)?;
    let q = closest_point(centre, t);
    if !front && crosses(t, previous, centre, s, s_prev) {
        return Some(RawContact {
            normal: n,
            depth: r - s,
            point: q,
        });
    }
    let d = length(sub(centre, q));
    if d >= r {
        return None;
    }
    // In the face's interior the face normal, exactly (so a wall at the
    // centre's height is not tilted by rounding); else from the nearest
    // edge or corner to the centre.
    let interior = (d - s.abs()).abs() <= 1e-5;
    let normal = if !interior && d > 1e-6 {
        scale(sub(centre, q), 1.0 / d)
    } else if front {
        n
    } else {
        // Behind the face's interior without crossing it: no contact.
        return None;
    };
    Some(RawContact {
        normal,
        depth: r - d,
        point: q,
    })
}

/// A convex polygon of at most six corners, on the stack.
#[derive(Clone, Copy)]
struct Polygon {
    corners: [Vec3; 6],
    len: usize,
}

impl Polygon {
    fn as_slice(&self) -> &[Vec3] {
        &self.corners[..self.len]
    }

    /// The part where `keep(p) ≥ 0`.
    fn clip(&self, keep: impl Fn(Vec3) -> f32) -> Self {
        let mut out = Self {
            corners: [[0.0; 3]; 6],
            len: 0,
        };
        let poly = self.as_slice();
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (da, db) = (keep(a), keep(b));
            if da >= 0.0 && out.len < 6 {
                out.corners[out.len] = a;
                out.len += 1;
            }
            if (da >= 0.0) != (db >= 0.0) && out.len < 6 {
                out.corners[out.len] = add(a, scale(sub(b, a), da / (da - db)));
                out.len += 1;
            }
        }
        out
    }
}

/// The part of triangle `t` between heights `y0` and `y1` (at most five
/// corners).
fn clip_slab(t: &[Vec3; 3], y0: f32, y1: f32) -> Polygon {
    let mut corners = [[0.0; 3]; 6];
    corners[..3].copy_from_slice(t);
    Polygon { corners, len: 3 }
        .clip(|p| p[1] - y0)
        .clip(|p| y1 - p[1])
}

/// The point of the (convex, planar) polygon closest to the vertical line
/// through `o` in the xz plane, and its horizontal distance (0 when the
/// line passes through the polygon).
fn closest_to_axis(poly: &[Vec3], o: Vec3, plane: Vec3, a: Vec3) -> (Vec3, f32) {
    let cross2 =
        |p: Vec3, q: Vec3, r: Vec3| (q[0] - p[0]) * (r[2] - p[2]) - (q[2] - p[2]) * (r[0] - p[0]);
    let n = poly.len();
    let mut sign = 0.0f32;
    let mut inside = n >= 3;
    for i in 0..n {
        let c = cross2(poly[i], poly[(i + 1) % n], o);
        if c.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            inside = false;
        }
    }
    if inside && sign != 0.0 && plane[1].abs() > 1e-6 {
        // The plane's height at the axis.
        let y = a[1] - (plane[0] * (o[0] - a[0]) + plane[2] * (o[2] - a[2])) / plane[1];
        return ([o[0], y, o[2]], 0.0);
    }
    let mut best = (poly[0], f32::INFINITY);
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let e = [q[0] - p[0], q[2] - p[2]];
        let len2 = e[0] * e[0] + e[1] * e[1];
        let k = if len2 > 0.0 {
            (((o[0] - p[0]) * e[0] + (o[2] - p[2]) * e[1]) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let x = add(p, scale(sub(q, p), k));
        let d = ((o[0] - x[0]).powi(2) + (o[2] - x[2]).powi(2)).sqrt();
        if d < best.1 {
            best = (x, d);
        }
    }
    best
}

/// Contact of a flat-ended cylinder with axis +y (centre `centre`, radius
/// `r`, half length `l`) with triangle `t`, front side only. The overlap is
/// exact (the triangle's part within the cylinder's height range against
/// its disc); the contact's normal is the separating direction of least
/// penetration among the triangle's normal, the horizontal direction from
/// the triangle to the axis and the caps (our choice). A centre that crossed
/// to the back is treated as [`sphere`] says: pushed out along the front
/// normal only when it crossed inside the face, otherwise only the
/// radial and cap directions apply (our choice).
pub fn cylinder(centre: Vec3, r: f32, l: f32, previous: Vec3, t: &[Vec3; 3]) -> Option<RawContact> {
    let n = front_normal(t)?;
    let (front, s, s_prev) = side(n, t, centre, previous)?;
    let face_depth = r * n[0].hypot(n[2]) + l * n[1].abs() - s;
    let (y0, y1) = (centre[1] - l, centre[1] + l);
    if !front && crosses(t, previous, centre, s, s_prev) {
        return Some(RawContact {
            normal: n,
            depth: face_depth,
            point: closest_point(centre, t),
        });
    }
    let clipped = clip_slab(t, y0, y1);
    let poly = clipped.as_slice();
    if poly.is_empty() {
        return None;
    }
    let (near_axis, d) = closest_to_axis(poly, centre, n, t[0]);
    if d >= r {
        return None;
    }
    let mut best = (
        if front { n } else { [0.0; 3] },
        if front { face_depth } else { f32::INFINITY },
    );
    if d > 1e-6 {
        let radial = [
            (centre[0] - near_axis[0]) / d,
            0.0,
            (centre[2] - near_axis[2]) / d,
        ];
        if r - d < best.1 {
            best = (radial, r - d);
        }
    }
    let (lo, hi) = poly
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
    if n[1] > 0.0 && hi - y0 < best.1 {
        best = ([0.0, 1.0, 0.0], hi - y0);
    }
    if n[1] < 0.0 && y1 - lo < best.1 {
        best = ([0.0, -1.0, 0.0], y1 - lo);
    }
    if !best.1.is_finite() {
        return None;
    }
    // The contact point: the overlapping part's point nearest the
    // cylinder's centre (our choice).
    let point = (1..poly.len() - 1)
        .map(|i| closest_point(centre, &[poly[0], poly[i], poly[i + 1]]))
        .min_by(|a, b| length(sub(*a, centre)).total_cmp(&length(sub(*b, centre))))
        .unwrap_or(near_axis);
    Some(RawContact {
        normal: best.0,
        depth: best.1,
        point,
    })
}

/// Whether triangle `t` intersects the box with centre `centre`,
/// orthonormal axes `axes` and half-sizes `half` (separating-axis test).
pub fn box_intersects(centre: Vec3, axes: [Vec3; 3], half: Vec3, t: &[Vec3; 3]) -> bool {
    let v = t.map(|p| {
        let d = sub(p, centre);
        [dot(d, axes[0]), dot(d, axes[1]), dot(d, axes[2])]
    });
    // The box's own axes.
    for k in 0..3 {
        let lo = v[0][k].min(v[1][k]).min(v[2][k]);
        let hi = v[0][k].max(v[1][k]).max(v[2][k]);
        if lo > half[k] || hi < -half[k] {
            return false;
        }
    }
    let separated = |a: Vec3| {
        let p = v.map(|q| dot(q, a));
        let rad = half[0] * a[0].abs() + half[1] * a[1].abs() + half[2] * a[2].abs();
        p[0].min(p[1]).min(p[2]) > rad || p[0].max(p[1]).max(p[2]) < -rad
    };
    let edges = [sub(v[1], v[0]), sub(v[2], v[1]), sub(v[0], v[2])];
    // The triangle's plane.
    if separated(cross(edges[0], edges[1])) {
        return false;
    }
    let units = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for u in units {
        for e in edges {
            if separated(cross(u, e)) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A floor square's first half at height 0, facing up.
    const FLOOR: [Vec3; 3] = [[-5.0, 0.0, -5.0], [-5.0, 0.0, 5.0], [5.0, 0.0, 5.0]];

    #[test]
    fn front_normal_follows_the_spec_winding() {
        assert_eq!(front_normal(&FLOOR), Some([0.0, 1.0, 0.0]));
        let flipped = [FLOOR[0], FLOOR[2], FLOOR[1]];
        assert_eq!(front_normal(&flipped), Some([0.0, -1.0, 0.0]));
        assert_eq!(front_normal(&[FLOOR[0], FLOOR[0], FLOOR[1]]), None);
    }

    #[test]
    fn sphere_contacts_only_from_the_front() {
        let c = sphere([-2.0, 0.3, 2.0], 0.35, [-2.0, 0.3, 2.0], &FLOOR).unwrap();
        assert_eq!(c.normal, [0.0, 1.0, 0.0]);
        assert!((c.depth - 0.05).abs() < 1e-6);
        assert!(sphere([-2.0, 0.4, 2.0], 0.35, [-2.0, 0.4, 2.0], &FLOOR).is_none());
        // Behind now and before: none.
        assert!(sphere([-2.0, -0.1, 2.0], 0.35, [-2.0, -0.1, 2.0], &FLOOR).is_none());
        // Crossed during the step: pushed back out.
        let c = sphere([-2.0, -0.1, 2.0], 0.35, [-2.0, 0.2, 2.0], &FLOOR).unwrap();
        assert_eq!(c.normal, [0.0, 1.0, 0.0]);
        assert!((c.depth - 0.45).abs() < 1e-6);
        // Crossed beside the face: no push-out; within reach of the edge,
        // the edge's contact.
        assert!(sphere([-5.5, -0.1, 0.0], 0.35, [-5.5, 0.2, 0.0], &FLOOR).is_none());
        let c = sphere([-5.1, -0.05, 0.0], 0.35, [-5.3, 0.2, 0.0], &FLOOR).unwrap();
        assert!(c.normal[0] < -0.5 && c.normal[1] < 0.0, "{c:?}");
        // At an edge the normal points from the edge to the centre.
        let c = sphere([-5.2, 0.2, 0.0], 0.35, [-5.2, 0.2, 0.0], &FLOOR).unwrap();
        assert!(c.normal[0] < -0.5 && c.normal[1] > 0.5);
    }

    #[test]
    fn cylinder_contacts_walls_floors_and_ceilings() {
        // A wall at x = 1 facing −x.
        let wall = [[1.0, 0.0, 5.0], [1.0, 5.0, 0.0], [1.0, 0.0, -5.0]];
        assert_eq!(front_normal(&wall).map(|n| n[0] < -0.99), Some(true));
        let at = [0.8, 1.0, 0.0];
        let c = cylinder(at, 0.29, 0.6, at, &wall).unwrap();
        assert!((c.normal[0] + 1.0).abs() < 1e-5 && c.normal[1].abs() < 1e-5);
        assert!((c.depth - 0.09).abs() < 1e-5);
        assert!(cylinder([0.6, 1.0, 0.0], 0.29, 0.6, [0.6, 1.0, 0.0], &wall).is_none());
        // Standing through a floor: the cap is pushed up.
        let at = [0.0, 0.5, 2.0];
        let c = cylinder(at, 0.29, 0.6, at, &FLOOR).unwrap();
        assert_eq!(c.normal, [0.0, 1.0, 0.0]);
        assert!((c.depth - 0.1).abs() < 1e-5);
        assert!(cylinder([0.0, 0.7, 2.0], 0.29, 0.6, [0.0, 0.7, 2.0], &FLOOR).is_none());
    }

    #[test]
    fn box_test_separates_and_overlaps() {
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(box_intersects(
            [0.0, 0.2, 2.0],
            axes,
            [0.2, 0.3, 0.2],
            &FLOOR
        ));
        assert!(!box_intersects(
            [0.0, 0.4, 2.0],
            axes,
            [0.2, 0.3, 0.2],
            &FLOOR
        ));
        // Beyond the floor's diagonal edge, off the triangle.
        assert!(!box_intersects(
            [3.0, 0.0, -3.0],
            axes,
            [0.5, 0.5, 0.5],
            &FLOOR
        ));
        let s = 0.5f32.sqrt();
        let turned = [[s, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, s]];
        assert!(box_intersects(
            [0.0, 0.0, 0.0],
            turned,
            [0.1, 0.1, 0.1],
            &FLOOR
        ));
    }
}
