// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The shipped library's arithmetic that the comparison's third round
//! established (docs/behavior/physics-harness-3.md 8), as `harness_rules_2`
//! does for the second: each rule's result recomputed here in plain single
//! precision from synthetic inputs, bit for bit, through the library's own
//! functions, and the rule it replaced (public 0.5.0's, as our build had
//! it) shown to round differently on some of them. The models follow the
//! library's code line by line (its loops too); `shipped` switches between
//! the two rules.

#![allow(clippy::needless_range_loop)]

use super::harness_rules::{ContactGeom, DxGeom, Inputs, bits, lock, normalise, normalise_slow};
use super::harness_rules_2::{
    EXACT, M3, Orders, QUICKSTEP, SOURCE, Sum, Xyz, Xzy, Yzx, mul331, normalise4, read, rotation,
    sum3, unit_quaternion, world_frame,
};
use super::{
    DContact, DMass, DxBody, DxJoint, DxJointGroup, DxWorld, Real, Surface, msvc_rand, x87,
};
use std::ffi::{c_int, c_void};

unsafe extern "C" {
    fn dCreateSphere(space: *mut c_void, radius: Real) -> *mut DxGeom;
    fn dCreateBox(space: *mut c_void, lx: Real, ly: Real, lz: Real) -> *mut DxGeom;
    fn dCreateCCylinder(space: *mut c_void, radius: Real, length: Real) -> *mut DxGeom;
    fn dCreatePlane(space: *mut c_void, a: Real, b: Real, c: Real, d: Real) -> *mut DxGeom;
    fn dCreateRay(space: *mut c_void, length: Real) -> *mut DxGeom;
    fn dGeomRaySet(g: *mut DxGeom, px: Real, py: Real, pz: Real, dx: Real, dy: Real, dz: Real);
    fn dGeomPlaneGetParams(g: *mut DxGeom, result: *mut Real);
    fn dGeomDestroy(g: *mut DxGeom);
    fn dGeomSetPosition(g: *mut DxGeom, x: Real, y: Real, z: Real);
    fn dGeomSetRotation(g: *mut DxGeom, r: *const Real);
    fn dGeomGetRotation(g: *mut DxGeom) -> *const Real;
    fn dGeomGetQuaternion(g: *mut DxGeom, q: *mut Real);
    fn dCollide(
        o1: *mut DxGeom,
        o2: *mut DxGeom,
        flags: c_int,
        contact: *mut ContactGeom,
        skip: c_int,
    ) -> c_int;
}

const NO_CONTACT: ContactGeom = ContactGeom {
    pos: [0.0; 4],
    normal: [0.0; 4],
    depth: 0.0,
    g1: std::ptr::null_mut(),
    g2: std::ptr::null_mut(),
};

pub(super) type V3 = [f32; 3];

pub(super) fn dot(o: Sum, a: V3, b: V3) -> f32 {
    sum3(o, a[0] * b[0], a[1] * b[1], a[2] * b[2])
}

fn col(r: &M3, j: usize) -> V3 {
    [r[j], r[4 + j], r[8 + j]]
}

fn row(r: &M3, i: usize) -> V3 {
    [r[4 * i], r[4 * i + 1], r[4 * i + 2]]
}

/// Rᵀ v, component i summed in order `o[i]`.
fn mul_t(r: &M3, v: V3, o: [Sum; 3]) -> V3 {
    std::array::from_fn(|i| dot(o[i], col(r, i), v))
}

pub(super) fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}

pub(super) fn sub(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] - b[i])
}

pub(super) fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn three(v: [f32; 4]) -> V3 {
    [v[0], v[1], v[2]]
}

/// A shipped grouping or public 0.5.0's source order.
pub(super) fn pick(shipped: bool, o: Sum) -> Sum {
    if shipped { o } else { Xyz }
}

fn picks(shipped: bool, o: [Sum; 3]) -> [Sum; 3] {
    if shipped { o } else { SOURCE }
}

/// A contact as compared: position, normal, depth.
type Con = (V3, V3, f32);

fn con_bits(c: &Con) -> ([u32; 3], [u32; 3], u32) {
    (bits(c.0), bits(c.1), c.2.to_bits())
}

fn got(c: &ContactGeom) -> Con {
    (three(c.pos), three(c.normal), c.depth)
}

/// The library's colliders through `dCollide`, up to `max` contacts.
fn collide(a: *mut DxGeom, b: *mut DxGeom, max: usize) -> Vec<Con> {
    let mut c = [NO_CONTACT; 8];
    // SAFETY: room for 8 contacts (every collider here makes at most 7).
    let n = unsafe {
        dCollide(
            a,
            b,
            max as c_int,
            c.as_mut_ptr(),
            size_of::<ContactGeom>() as c_int,
        )
    };
    c[..n as usize].iter().map(got).collect()
}

fn same(a: &[Con], b: &[Con]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| con_bits(x) == con_bits(y))
}

fn assert_same(got: &[Con], want: &[Con], what: &dyn std::fmt::Debug) {
    assert!(same(got, want), "{what:?}\ngot  {got:?}\nwant {want:?}");
}

/// A random rotation as the library stores it.
fn random_rotation(inputs: &mut Inputs) -> M3 {
    rotation(unit_quaternion(inputs))
}

/// A geometry placed at `p` with rotation `r`; the rotation as stored.
fn place(g: *mut DxGeom, p: V3, r: &M3) -> M3 {
    // SAFETY: a geometry made by the caller.
    unsafe {
        dGeomSetPosition(g, p[0], p[1], p[2]);
        dGeomSetRotation(g, r.as_ptr());
        read::<12>(dGeomGetRotation(g))
    }
}

/// The two spheres' contact (`dCollideSpheres`), the distance's squares
/// summed in order `o`.
fn spheres(p1: V3, r1: f32, p2: V3, r2: f32, o: Sum) -> Option<Con> {
    let q = sub(p1, p2);
    let d = dot(o, q, q).sqrt();
    if d > r1 + r2 {
        return None;
    }
    if d <= 0.0 {
        return Some((p1, [1.0, 0.0, 0.0], r1 + r2));
    }
    let d1 = 1.0 / d;
    let n = q.map(|x| x * d1);
    let k = 0.5 * (r2 - r1 - d);
    Some((std::array::from_fn(|i| p1[i] + n[i] * k), n, r1 + r2 - d))
}

/// 2.1: sphere (centre `c`, radius `radius`) against box (`p`, `r`, sides
/// `side`).
fn sphere_box(c: V3, radius: f32, p: V3, r: &M3, side: V3, shipped: bool) -> Option<Con> {
    let o = pick(shipped, Yzx);
    let d = sub(c, p);
    let l = side.map(|s| s * 0.5);
    let mut t = [0.0; 3];
    let mut onborder = false;
    for i in 0..3 {
        t[i] = dot(o, d, col(r, i));
        if t[i] < -l[i] {
            t[i] = -l[i];
            onborder = true;
        }
        if t[i] > l[i] {
            t[i] = l[i];
            onborder = true;
        }
    }
    if !onborder {
        let mut min_distance = l[0] - t[0].abs();
        let mut mini = 0;
        for i in 1..3 {
            let face_distance = l[i] - t[i].abs();
            if face_distance < min_distance {
                min_distance = face_distance;
                mini = i;
            }
        }
        let mut tmp = [0.0; 3];
        tmp[mini] = if t[mini] > 0.0 { 1.0 } else { -1.0 };
        return Some((c, mul331(r, tmp, [o; 3]), min_distance + radius));
    }
    let q = mul331(r, t, [o; 3]);
    let rr = sub(d, q);
    let depth = radius - dot(o, rr, rr).sqrt();
    if depth < 0.0 {
        return None;
    }
    Some((add(q, p), normalise(rr), depth))
}

/// The test of a pair kind: `make` builds the two geometries from the
/// inputs and returns them with the shipped and public models' contacts.
fn pairs(
    n: usize,
    max: usize,
    mut make: impl FnMut(&mut Inputs, usize) -> (*mut DxGeom, *mut DxGeom, Vec<Con>, Vec<Con>),
) -> (usize, usize) {
    let mut inputs = Inputs::new();
    let (mut hits, mut differ) = (0, 0);
    for i in 0..n {
        let (a, b, shipped, public) = make(&mut inputs, i);
        let c = collide(a, b, max);
        assert_same(&c, &shipped, &i);
        hits += usize::from(!c.is_empty());
        differ += usize::from(!same(&shipped, &public));
        // SAFETY: made by `make`.
        unsafe {
            dGeomDestroy(a);
            dGeomDestroy(b);
        }
    }
    (hits, differ)
}

/// 2.1 and 8 item 1.
#[test]
fn sphere_box_in_the_shipped_groupings() {
    let (hits, differ) = pairs(4000, 1, |inputs, i| {
        let side = inputs.vec3(0.2, 3.0);
        let radius = inputs.next(0.1, 1.5);
        let p = inputs.vec3(-2.0, 2.0);
        // Every fourth centre inside the box.
        let c = if i % 4 == 0 {
            add(p, inputs.vec3(-0.1, 0.1))
        } else {
            add(p, inputs.vec3(-2.5, 2.5))
        };
        // SAFETY: geometries outside any space, destroyed by `pairs`.
        let (s, b) = unsafe {
            (
                dCreateSphere(std::ptr::null_mut(), radius),
                dCreateBox(std::ptr::null_mut(), side[0], side[1], side[2]),
            )
        };
        place(s, c, &rotation([1.0, 0.0, 0.0, 0.0]));
        let r = place(b, p, &random_rotation(inputs));
        let model = |shipped| Vec::from_iter(sphere_box(c, radius, p, &r, side, shipped));
        (s, b, model(true), model(false))
    });
    assert!(hits > 1000, "{hits}");
    assert!(differ > 0, "the source order rounds the same");
}

/// Public 0.5.0's polygon clipping of box–box's face case
/// (`intersectRectQuad`): the quadrilateral `p` clipped to the rectangle of
/// half sides `h`.
fn intersect_rect_quad(h: [f32; 2], p: [f32; 8]) -> Vec<f32> {
    let mut q = p.to_vec();
    for dir in 0..2 {
        for sign in [-1.0f32, 1.0] {
            let nq = q.len() / 2;
            let mut r = Vec::new();
            for i in 0..nq {
                let pq = [q[2 * i], q[2 * i + 1]];
                let k = if i + 1 < nq { i + 1 } else { 0 };
                let nextq = [q[2 * k], q[2 * k + 1]];
                let inside = sign * pq[dir] < h[dir];
                if inside {
                    r.extend(pq);
                    if r.len() == 16 {
                        return r;
                    }
                }
                if inside ^ (sign * nextq[dir] < h[dir]) {
                    let mut pr = [0.0; 2];
                    pr[1 - dir] = pq[1 - dir]
                        + (nextq[1 - dir] - pq[1 - dir]) / (nextq[dir] - pq[dir])
                            * (sign * h[dir] - pq[dir]);
                    pr[dir] = sign * h[dir];
                    r.extend(pr);
                    if r.len() == 16 {
                        return r;
                    }
                }
            }
            q = r;
        }
    }
    q
}

/// Public 0.5.0's closest approach of two lines (`dLineClosestApproach`).
fn line_closest_approach(pa: V3, ua: V3, pb: V3, ub: V3) -> (f32, f32) {
    let p = sub(pb, pa);
    let uaub = dot(Xyz, ua, ub);
    let q1 = dot(Xyz, ua, p);
    let q2 = -dot(Xyz, ub, p);
    let d = 1.0 - uaub * uaub;
    if d <= 0.0001 {
        return (0.0, 0.0);
    }
    let d = 1.0 / d;
    ((q1 + uaub * q2) * d, (uaub * q1 + q2) * d)
}

/// A box: centre, rotation, sides.
#[derive(Clone, Copy, Debug)]
struct Box3 {
    p: V3,
    r: M3,
    side: V3,
}

/// The fifteen separation tests' sums (2.4's table): the shipped
/// library's, or public 0.5.0's.
#[rustfmt::skip]
fn separation_sums(a: V3, b: V3, q: [[f32; 3]; 3], shipped: bool) -> [f32; 15] {
    let [a0, a1, a2] = a;
    let [b0, b1, b2] = b;
    let [[q11, q12, q13], [q21, q22, q23], [q31, q32, q33]] = q;
    if shipped {
        [
            (b0 * q11 + (b1 * q12 + b2 * q13)) + a0,
            ((b2 * q23 + b1 * q22) + b0 * q21) + a1,
            (b0 * q31 + (b1 * q32 + b2 * q33)) + a2,
            ((a1 * q21 + a2 * q31) + a0 * q11) + b0,
            ((a2 * q32 + a1 * q22) + a0 * q12) + b1,
            ((a1 * q23 + a2 * q33) + a0 * q13) + b2,
            ((b2 * q12 + b1 * q13) + a1 * q31) + a2 * q21,
            (a1 * q32 + (b2 * q11 + b0 * q13)) + a2 * q22,
            ((a1 * q33 + b1 * q11) + a2 * q23) + b0 * q12,
            ((b2 * q22 + b1 * q23) + a0 * q31) + a2 * q11,
            ((b0 * q23 + b2 * q21) + a0 * q32) + a2 * q12,
            a2 * q13 + ((b1 * q21 + b0 * q22) + a0 * q33),
            a1 * q11 + ((b2 * q32 + b1 * q33) + a0 * q21),
            ((b2 * q31 + b0 * q33) + a0 * q22) + a1 * q12,
            (a0 * q23 + (b0 * q32 + b1 * q31)) + a1 * q13,
        ]
    } else {
        [
            a0 + b0 * q11 + b1 * q12 + b2 * q13,
            a1 + b0 * q21 + b1 * q22 + b2 * q23,
            a2 + b0 * q31 + b1 * q32 + b2 * q33,
            a0 * q11 + a1 * q21 + a2 * q31 + b0,
            a0 * q12 + a1 * q22 + a2 * q32 + b1,
            a0 * q13 + a1 * q23 + a2 * q33 + b2,
            a1 * q31 + a2 * q21 + b1 * q13 + b2 * q12,
            a1 * q32 + a2 * q22 + b0 * q13 + b2 * q11,
            a1 * q33 + a2 * q23 + b0 * q12 + b1 * q11,
            a0 * q31 + a2 * q11 + b1 * q23 + b2 * q22,
            a0 * q32 + a2 * q12 + b0 * q23 + b2 * q21,
            a0 * q33 + a2 * q13 + b0 * q22 + b1 * q21,
            a0 * q21 + a1 * q11 + b1 * q33 + b2 * q32,
            a0 * q22 + a1 * q12 + b0 * q33 + b2 * q31,
            a0 * q23 + a1 * q13 + b0 * q32 + b1 * q31,
        ]
    }
}

/// 2.4: the box–box collider (`dCollideBoxBox`, at most 8 contacts asked
/// for, so that none is culled), with the shipped library's edge–edge
/// contacts (physics-library.md 6.2) and this round's groupings.
fn box_box(b1: &Box3, b2: &Box3, shipped: bool) -> (Vec<Con>, usize) {
    let y = pick(shipped, Yzx);
    let (p1, r1, p2, r2) = (b1.p, &b1.r, b2.p, &b2.r);
    let p = sub(p2, p1);
    let pp = mul_t(r1, p, picks(shipped, [Xyz, Yzx, Yzx]));
    let a = b1.side.map(|s| s * 0.5);
    let b = b2.side.map(|s| s * 0.5);
    let rr: [[f32; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| dot(if i + j == 0 { Xyz } else { y }, col(r1, i), col(r2, j)))
    });
    let q = rr.map(|row| row.map(f32::abs));
    let sums = separation_sums(a, b, q, shipped);
    let [[r11, r12, r13], [r21, r22, r23], [r31, r32, r33]] = rr;

    // The face tests: box 1's axes (1 – 3), box 2's (4 – 6).
    let mut s = f32::NEG_INFINITY;
    let mut normal_r: Option<V3> = None;
    let mut normal_c = [0.0; 3];
    let mut invert = false;
    let mut code = 0;
    for k in 0..6 {
        let (e, axis) = if k < 3 {
            (pp[k], col(r1, k))
        } else {
            (dot(y, col(r2, k - 3), p), col(r2, k - 3))
        };
        let s2 = e.abs() - sums[k];
        if s2 > 0.0 {
            return (vec![], 0);
        }
        if s2 > s {
            s = s2;
            normal_r = Some(axis);
            invert = e < 0.0;
            code = k + 1;
        }
    }
    // The edge tests: the expression and the normal relative to box 1.
    let edges: [(f32, V3); 9] = [
        (pp[2] * r21 - pp[1] * r31, [0.0, -r31, r21]),
        (pp[2] * r22 - pp[1] * r32, [0.0, -r32, r22]),
        (pp[2] * r23 - pp[1] * r33, [0.0, -r33, r23]),
        (pp[0] * r31 - pp[2] * r11, [r31, 0.0, -r11]),
        (pp[0] * r32 - pp[2] * r12, [r32, 0.0, -r12]),
        (pp[0] * r33 - pp[2] * r13, [r33, 0.0, -r13]),
        (pp[1] * r11 - pp[0] * r21, [-r21, r11, 0.0]),
        (pp[1] * r12 - pp[0] * r22, [-r22, r12, 0.0]),
        (pp[1] * r13 - pp[0] * r23, [-r23, r13, 0.0]),
    ];
    for (k, (e, n)) in edges.into_iter().enumerate() {
        let mut s2 = e.abs() - sums[6 + k];
        if s2 > 0.0 {
            return (vec![], 0);
        }
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if l > 0.0 {
            let il = 1.0 / l;
            if shipped {
                s2 *= il;
            } else {
                s2 /= l;
            }
            if s2 * 1.05 > s {
                s = s2;
                normal_r = None;
                normal_c = if shipped {
                    n.map(|x| x * il)
                } else {
                    n.map(|x| x / l)
                };
                invert = e < 0.0;
                code = 7 + k;
            }
        }
    }
    if code == 0 {
        return (vec![], 0);
    }
    let mut normal = match normal_r {
        Some(axis) => axis,
        None => mul331(r1, normal_c, picks(shipped, [Xyz, Yzx, Yzx])),
    };
    if invert {
        normal = normal.map(|x| -x);
    }
    let depth = -s;
    // The collider's contacts carry the normal negated.
    let contacts = |c: Vec<(V3, f32)>| -> Vec<Con> {
        c.into_iter()
            .map(|(pos, d)| (pos, normal.map(|x| -x), d))
            .collect()
    };

    if code > 6 {
        let xz = pick(shipped, Xzy);
        let mut pa = p1;
        for j in 0..3 {
            let sign = if dot(xz, normal, col(r1, j)) > 0.0 {
                1.0
            } else {
                -1.0
            };
            for i in 0..3 {
                pa[i] += sign * a[j] * r1[i * 4 + j];
            }
        }
        let mut pb = p2;
        for j in 0..3 {
            let sign = if dot(xz, normal, col(r2, j)) > 0.0 {
                -1.0
            } else {
                1.0
            };
            for i in 0..3 {
                pb[i] += sign * b[j] * r2[i * 4 + j];
            }
        }
        let (k1, k2) = ((code - 7) / 3, (code - 7) % 3);
        let (ua, ub) = (col(r1, k1), col(r2, k2));
        let (alpha, beta) = line_closest_approach(pa, ua, pb, ub);
        for i in 0..3 {
            pa[i] += ua[i] * alpha;
            pb[i] += ub[i] * beta;
        }
        let mut c = vec![(std::array::from_fn(|i| 0.5 * (pa[i] + pb[i])), depth)];
        // The further edge–edge contacts (physics-library.md 6.2 item 2).
        let axes = |r: &M3, k: usize| {
            let (i, j) = ((k + 1) % 3, (k + 2) % 3);
            let ni = dot(y, normal, col(r, i)).abs();
            let nj = dot(y, normal, col(r, j)).abs();
            if ni > nj { (i, j) } else { (j, i) }
        };
        let (m1, s1) = axes(r1, k1);
        let (m2, s2) = axes(r2, k2);
        for cand in 0..6 {
            let second = cand < 3;
            let (bp, r, h, lp, u, m, sa, k) = if second {
                (p2, r2, b, pa, ua, m2, s2, k2)
            } else {
                (p1, r1, a, pb, ub, m1, s1, k1)
            };
            let (j, sign) = if cand % 3 == 0 {
                let d = sub(bp, lp);
                (
                    sa,
                    if dot(y, d, col(r, sa)) > 0.0 {
                        1.0
                    } else {
                        -1.0
                    },
                )
            } else {
                (k, if cand % 3 == 1 { 1.0 } else { -1.0 })
            };
            // Box 1's faces on its edge axis take box 2's centre.
            let pc = if shipped && cand >= 4 { p2 } else { bp };
            let f: V3 = std::array::from_fn(|i| pc[i] + sign * h[j] * r[i * 4 + j]);
            let t = (dot(y, lp, col(r, j)) - dot(y, f, col(r, j))) / dot(y, u, col(r, j));
            let q: V3 = std::array::from_fn(|i| lp[i] - t * u[i]);
            let (ce, ee) = ((j + 1) % 3, (j + 2) % 3);
            let within = (dot(y, q, col(r, ce)) - dot(y, f, col(r, ce))).abs() <= h[ce]
                && (dot(y, q, col(r, ee)) - dot(y, f, col(r, ee))).abs() <= h[ee];
            if within {
                let qp = sub(q, lp);
                let d = depth - (dot(y, normal, col(r, m)) * dot(y, qp, col(r, m))).abs();
                if d > 0.0 {
                    c.push((q, d));
                }
            }
        }
        return (contacts(c), code);
    }

    // A face–something case: face a the reference face, b the incident.
    let (ra, rb, pa, pb, sa, sb) = if code <= 3 {
        (r1, r2, p1, p2, a, b)
    } else {
        (r2, r1, p2, p1, b, a)
    };
    let normal2 = if code <= 3 {
        normal
    } else {
        normal.map(|x| -x)
    };
    let nr = mul_t(rb, normal2, [y; 3]);
    let anr = nr.map(f32::abs);
    let (lanr, a1, a2) = if anr[1] > anr[0] {
        if anr[1] > anr[2] {
            (1, 0, 2)
        } else {
            (2, 0, 1)
        }
    } else if anr[0] > anr[2] {
        (0, 1, 2)
    } else {
        (2, 0, 1)
    };
    let center: V3 = std::array::from_fn(|i| {
        if nr[lanr] < 0.0 {
            pb[i] - pa[i] + sb[lanr] * rb[i * 4 + lanr]
        } else {
            pb[i] - pa[i] - sb[lanr] * rb[i * 4 + lanr]
        }
    });
    let code_n = if code <= 3 { code - 1 } else { code - 4 };
    let (code1, code2) = match code_n {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let c1 = dot(y, center, col(ra, code1));
    let c2 = dot(y, center, col(ra, code2));
    let mut m11 = dot(y, col(ra, code1), col(rb, a1));
    let mut m12 = dot(y, col(ra, code1), col(rb, a2));
    let mut m21 = dot(y, col(ra, code2), col(rb, a1));
    let mut m22 = dot(y, col(ra, code2), col(rb, a2));
    let (k1, k2, k3, k4) = (m11 * sb[a1], m21 * sb[a1], m12 * sb[a2], m22 * sb[a2]);
    let quad = [
        c1 - k1 - k3,
        c2 - k2 - k4,
        c1 - k1 + k3,
        c2 - k2 + k4,
        if shipped {
            (k3 + k1) + c1
        } else {
            c1 + k1 + k3
        },
        if shipped {
            (k4 + k2) + c2
        } else {
            c2 + k2 + k4
        },
        c1 + k1 - k3,
        c2 + k2 - k4,
    ];
    let ret = intersect_rect_quad([sa[code1], sa[code2]], quad);
    if ret.is_empty() {
        return (vec![], 0);
    }
    let det1 = 1.0 / (m11 * m22 - m12 * m21);
    m11 *= det1;
    m12 *= det1;
    m21 *= det1;
    m22 *= det1;
    let mut c = Vec::new();
    for j in 0..ret.len() / 2 {
        let k1 = m22 * (ret[j * 2] - c1) - m12 * (ret[j * 2 + 1] - c2);
        let k2 = -m21 * (ret[j * 2] - c1) + m11 * (ret[j * 2 + 1] - c2);
        let point: V3 = std::array::from_fn(|i| {
            if shipped {
                (k1 * rb[i * 4 + a1] + k2 * rb[i * 4 + a2]) + center[i]
            } else {
                center[i] + k1 * rb[i * 4 + a1] + k2 * rb[i * 4 + a2]
            }
        });
        let dep = sa[code_n] - dot(pick(shipped, Xzy), normal2, point);
        if dep >= 0.0 {
            c.push((add(point, pa), dep));
        }
    }
    (contacts(c), code)
}

/// 2.4 and 8 item 4: face and edge–edge cases, at most 8 contacts asked for.
#[test]
fn box_box_in_the_shipped_groupings() {
    let (mut edge, mut face, mut added) = (0, 0, 0);
    let (hits, differ) = pairs(6000, 8, |inputs, _| {
        let side1 = inputs.vec3(0.3, 2.0);
        let side2 = inputs.vec3(0.3, 2.0);
        // SAFETY: geometries outside any space, destroyed by `pairs`.
        let (g1, g2) = unsafe {
            (
                dCreateBox(std::ptr::null_mut(), side1[0], side1[1], side1[2]),
                dCreateBox(std::ptr::null_mut(), side2[0], side2[1], side2[2]),
            )
        };
        let p1 = inputs.vec3(-1.0, 1.0);
        let p2 = add(p1, inputs.vec3(-1.5, 1.5));
        let b1 = Box3 {
            p: p1,
            r: place(g1, p1, &random_rotation(inputs)),
            side: side1,
        };
        let b2 = Box3 {
            p: p2,
            r: place(g2, p2, &random_rotation(inputs)),
            side: side2,
        };
        let (shipped, code) = box_box(&b1, &b2, true);
        edge += usize::from(code > 6);
        face += usize::from(code > 0 && code <= 6);
        added += usize::from(code > 6 && shipped.len() > 1);
        (g1, g2, shipped, box_box(&b1, &b2, false).0)
    });
    assert!(
        hits > 1500 && edge > 100 && added > 20 && face > 500,
        "{hits} {edge} {added} {face}"
    );
    assert!(differ > 0, "the public rule rounds the same");
}

/// 2.2: the closest points of the segment p1–p2 and the box (`c`, `r`,
/// `side`), `dClosestLineBoxPoints`.
fn closest_line_box(p1: V3, p2: V3, c: V3, r: &M3, side: V3, shipped: bool) -> (V3, V3) {
    let o = picks(shipped, [Xyz, Yzx, Yzx]);
    let mut s = mul_t(r, sub(p1, c), o);
    let tmp = sub(p2, p1);
    let mut v = mul_t(r, tmp, o);
    let mut sign = [1.0f32; 3];
    for i in 0..3 {
        if v[i] < 0.0 {
            s[i] = -s[i];
            v[i] = -v[i];
            sign[i] = -1.0;
        }
    }
    let v2 = v.map(|x| x * x);
    let h = side.map(|x| 0.5 * x);
    let mut region = [0i32; 3];
    let mut tanchor = [0.0f32; 3];
    for i in 0..3 {
        if v[i] > 0.0 {
            if s[i] < -h[i] {
                region[i] = -1;
                tanchor[i] = (-h[i] - s[i]) / v[i];
            } else {
                region[i] = i32::from(s[i] > h[i]);
                tanchor[i] = (h[i] - s[i]) / v[i];
            }
        } else {
            region[i] = 0;
            tanchor[i] = 2.0;
        }
    }
    let term = |region: &[i32; 3], i: usize, x: f32| (if region[i] != 0 { v2[i] } else { 0.0 }) * x;
    let mut t = 0.0f32;
    let mut dd2dt = if shipped { -0.0f32 } else { 0.0 };
    for i in 0..3 {
        dd2dt -= term(&region, i, tanchor[i]);
    }
    'answer: {
        if dd2dt >= 0.0 {
            break 'answer;
        }
        loop {
            let mut next_t = 1.0f32;
            for i in 0..3 {
                if tanchor[i] > t && tanchor[i] < 1.0 && tanchor[i] < next_t {
                    next_t = tanchor[i];
                }
            }
            let terms: V3 = std::array::from_fn(|i| term(&region, i, next_t - tanchor[i]));
            let next_dd2dt = if shipped {
                terms[0] + terms[1] + terms[2]
            } else {
                0.0 + terms[0] + terms[1] + terms[2]
            };
            if next_dd2dt >= 0.0 {
                let m = (next_dd2dt - dd2dt) / (next_t - t);
                t -= dd2dt / m;
                break 'answer;
            }
            for i in 0..3 {
                if tanchor[i] == next_t {
                    tanchor[i] = (h[i] - s[i]) / v[i];
                    region[i] += 1;
                }
            }
            t = next_t;
            dd2dt = next_dd2dt;
            if t >= 1.0 {
                break;
            }
        }
        t = 1.0;
    }
    let lret = std::array::from_fn(|i| p1[i] + t * tmp[i]);
    let clamped: V3 = std::array::from_fn(|i| (sign[i] * (s[i] + t * v[i])).clamp(-h[i], h[i]));
    (lret, add(mul331(r, clamped, o), c))
}

/// 2.2 and 8 item 2: capped cylinder (`p`, `r`, radius, length) against
/// box.
fn capped_cylinder_box(cyl: (V3, &M3, f32, f32), bx: &Box3, shipped: bool) -> Option<Con> {
    let (p, r, radius, length) = cyl;
    let clen = length * 0.5;
    let axis = col(r, 2);
    let p1 = std::array::from_fn(|i| p[i] + clen * axis[i]);
    let p2 = std::array::from_fn(|i| p[i] - clen * axis[i]);
    let (pl, pb) = closest_line_box(p1, p2, bx.p, &bx.r, bx.side, shipped);
    spheres(pl, radius, pb, 0.0, Xyz)
}

#[test]
fn capped_cylinder_box_in_the_shipped_groupings() {
    let (hits, differ) = pairs(6000, 1, |inputs, _| {
        let (radius, length) = (inputs.next(0.05, 0.8), inputs.next(0.1, 2.0));
        let side = inputs.vec3(0.3, 2.0);
        // SAFETY: geometries outside any space, destroyed by `pairs`.
        let (gc, gb) = unsafe {
            (
                dCreateCCylinder(std::ptr::null_mut(), radius, length),
                dCreateBox(std::ptr::null_mut(), side[0], side[1], side[2]),
            )
        };
        let pc = inputs.vec3(-1.0, 1.0);
        let pb = add(pc, inputs.vec3(-1.5, 1.5));
        let rc = place(gc, pc, &random_rotation(inputs));
        let bx = Box3 {
            p: pb,
            r: place(gb, pb, &random_rotation(inputs)),
            side,
        };
        let model =
            |shipped| Vec::from_iter(capped_cylinder_box((pc, &rc, radius, length), &bx, shipped));
        (gc, gb, model(true), model(false))
    });
    assert!(hits > 1500, "{hits}");
    assert!(differ > 0, "the public rule rounds the same");
}

/// 2.3: the closest points of the segments a1–a2 and b1–b2
/// (`dClosestLineSegmentPoints`), every dot product summed in order `o`.
fn closest_segments(a1: V3, a2: V3, b1: V3, b2: V3, o: Sum) -> (V3, V3) {
    let d = |a: V3, b: V3| dot(o, a, b);
    let scaled = |a: V3, k: f32| a.map(|x| k * x);
    let a1a2 = sub(a2, a1);
    let b1b2 = sub(b2, b1);
    let a1b1 = sub(b1, a1);
    let (da1, db1) = (d(a1a2, a1b1), d(b1b2, a1b1));
    if da1 <= 0.0 && db1 >= 0.0 {
        return (a1, b1);
    }
    let a1b2 = sub(b2, a1);
    let (da2, db2) = (d(a1a2, a1b2), d(b1b2, a1b2));
    if da2 <= 0.0 && db2 <= 0.0 {
        return (a1, b2);
    }
    let a2b1 = sub(b1, a2);
    let (da3, db3) = (d(a1a2, a2b1), d(b1b2, a2b1));
    if da3 >= 0.0 && db3 >= 0.0 {
        return (a2, b1);
    }
    let a2b2 = sub(b2, a2);
    let (da4, db4) = (d(a1a2, a2b2), d(b1b2, a2b2));
    if da4 >= 0.0 && db4 <= 0.0 {
        return (a2, b2);
    }
    let la = d(a1a2, a1a2);
    if da1 >= 0.0 && da3 <= 0.0 {
        let k = da1 / la;
        let n = sub(a1b1, scaled(a1a2, k));
        if d(b1b2, n) >= 0.0 {
            return (add(a1, scaled(a1a2, k)), b1);
        }
    }
    if da2 >= 0.0 && da4 <= 0.0 {
        let k = da2 / la;
        let n = sub(a1b2, scaled(a1a2, k));
        if d(b1b2, n) <= 0.0 {
            return (add(a1, scaled(a1a2, k)), b2);
        }
    }
    let lb = d(b1b2, b1b2);
    if db1 <= 0.0 && db2 >= 0.0 {
        let k = -db1 / lb;
        let n = sub(a1b1.map(|x| -x), scaled(b1b2, k));
        if d(a1a2, n) >= 0.0 {
            return (a1, add(b1, scaled(b1b2, k)));
        }
    }
    if db3 <= 0.0 && db4 >= 0.0 {
        let k = -db3 / lb;
        let n = sub(a2b1.map(|x| -x), scaled(b1b2, k));
        if d(a1a2, n) <= 0.0 {
            return (a2, add(b1, scaled(b1b2, k)));
        }
    }
    let k = d(a1a2, b1b2);
    let det = la * lb - k * k;
    if det <= 0.0 {
        return (a1, b1);
    }
    let det = 1.0 / det;
    let alpha = (lb * da1 - k * db1) * det;
    let beta = (k * da1 - la * db1) * det;
    (add(a1, scaled(a1a2, alpha)), add(b1, scaled(b1b2, beta)))
}

/// A capped cylinder: centre, rotation, radius, length.
#[derive(Clone, Copy)]
struct Cyl {
    p: V3,
    r: M3,
    radius: f32,
    length: f32,
}

/// 2.3 and 8 item 3: two capped cylinders, at most `max` contacts.
fn two_capped_cylinders(c1: &Cyl, c2: &Cyl, max: usize, shipped: bool) -> Vec<Con> {
    let y = pick(shipped, Yzx);
    let (lz1, lz2) = (c1.length * 0.5, c2.length * 0.5);
    let axis1 = col(&c1.r, 2);
    let mut axis2 = col(&c2.r, 2);
    let a1a2 = dot(y, axis1, axis2);
    let det = 1.0 - a1a2 * a1a2;
    let at = |p: V3, axis: V3, alpha: f32| std::array::from_fn(|i| p[i] + alpha * axis[i]);
    if det < 1e-5 {
        if a1a2 < 0.0 {
            axis2 = axis2.map(|x| -x);
        }
        let q = sub(c1.p, c2.p);
        let k = dot(y, axis1, q);
        let lo = if -lz1 > -lz2 - k { -lz1 } else { -lz2 - k };
        let hi = if lz1 < lz2 - k { lz1 } else { lz2 - k };
        if lo <= hi {
            if max >= 2 && lo < hi {
                let first = spheres(
                    at(c1.p, axis1, lo),
                    c1.radius,
                    at(c2.p, axis2, lo + k),
                    c2.radius,
                    Xyz,
                );
                if let Some(first) = first {
                    let second = spheres(
                        at(c1.p, axis1, hi),
                        c1.radius,
                        at(c2.p, axis2, hi + k),
                        c2.radius,
                        y,
                    );
                    if let Some(second) = second {
                        return vec![first, second];
                    }
                }
            }
            let alpha1 = (lo + hi) * 0.5;
            return Vec::from_iter(spheres(
                at(c1.p, axis1, alpha1),
                c1.radius,
                at(c2.p, axis2, alpha1 + k),
                c2.radius,
                y,
            ));
        }
    }
    let ends = |c: &Cyl, axis: V3, lz: f32| {
        (
            std::array::from_fn(|i| c.p[i] + axis[i] * lz),
            std::array::from_fn(|i| c.p[i] - axis[i] * lz),
        )
    };
    let (a1, a2) = ends(c1, axis1, lz1);
    let (b1, b2) = ends(c2, axis2, lz2);
    let (s1, s2) = closest_segments(a1, a2, b1, b2, y);
    Vec::from_iter(spheres(s1, c1.radius, s2, c2.radius, Xyz))
}

#[test]
fn two_capped_cylinders_in_the_shipped_groupings() {
    let mut parallel = 0;
    let (hits, differ) = pairs(8000, 2, |inputs, i| {
        let mut c1 = Cyl {
            p: [0.0; 3],
            r: [0.0; 12],
            radius: inputs.next(0.05, 0.8),
            length: inputs.next(0.1, 2.0),
        };
        let mut c2 = Cyl {
            radius: inputs.next(0.05, 0.8),
            length: inputs.next(0.1, 2.0),
            ..c1
        };
        // SAFETY: geometries outside any space, destroyed by `pairs`.
        let (g1, g2) = unsafe {
            (
                dCreateCCylinder(std::ptr::null_mut(), c1.radius, c1.length),
                dCreateCCylinder(std::ptr::null_mut(), c2.radius, c2.length),
            )
        };
        c1.p = inputs.vec3(-1.0, 1.0);
        c2.p = add(c1.p, inputs.vec3(-1.2, 1.2));
        let q = unit_quaternion(inputs);
        c1.r = place(g1, c1.p, &rotation(q));
        // Every fourth pair near-parallel (2.3; static only in the spec).
        let q2 = if i % 4 == 0 {
            let mut q2 = q.map(|x| x + inputs.next(-2e-4, 2e-4));
            if i % 8 == 0 {
                // Antiparallel: then a half turn about the body's x axis.
                q2 = [-q2[1], q2[0], q2[3], -q2[2]];
            }
            normalise4(q2)
        } else {
            unit_quaternion(inputs)
        };
        c2.r = place(g2, c2.p, &rotation(q2));
        let det = 1.0 - dot(Yzx, col(&c1.r, 2), col(&c2.r, 2)).powi(2);
        parallel += usize::from(det < 1e-5);
        (
            g1,
            g2,
            two_capped_cylinders(&c1, &c2, 2, true),
            two_capped_cylinders(&c1, &c2, 2, false),
        )
    });
    assert!(hits > 2000 && parallel > 500, "{hits} {parallel}");
    assert!(differ > 0, "the public rule rounds the same");
}

/// 2.5 and 8 item 5: box–plane (round 2's depth, physics-harness-2.md
/// 2.7, with this round's dots), sphere–plane, capped cylinder–plane and
/// capped cylinder–sphere.
#[test]
fn planes_and_capped_cylinder_sphere_in_the_shipped_groupings() {
    let plane = |inputs: &mut Inputs| {
        let n = inputs.vec3(-1.0, 1.0);
        // SAFETY: a plane outside any space, destroyed by `pairs`.
        unsafe {
            let g = dCreatePlane(
                std::ptr::null_mut(),
                n[0],
                n[1],
                n[2],
                inputs.next(-1.0, 1.0),
            );
            let mut pl = [0.0; 4];
            dGeomPlaneGetParams(g, pl.as_mut_ptr());
            (g, pl)
        }
    };
    let pn = |pl: [f32; 4]| [pl[0], pl[1], pl[2]];
    let (box_hits, box_differ) = pairs(4000, 3, |inputs, _| {
        let side = inputs.vec3(0.2, 3.0);
        // SAFETY: as above.
        let b = unsafe { dCreateBox(std::ptr::null_mut(), side[0], side[1], side[2]) };
        let p = inputs.vec3(-2.0, 2.0);
        let r = place(b, p, &random_rotation(inputs));
        let (g, pl) = plane(inputs);
        let model = |shipped: bool| {
            let y = pick(shipped, Yzx);
            let n = pn(pl);
            let q = [
                dot(Xyz, n, col(&r, 0)),
                dot(y, n, col(&r, 1)),
                dot(y, n, col(&r, 2)),
            ];
            let a: V3 = std::array::from_fn(|i| side[i] * q[i]);
            let bb = a.map(f32::abs);
            let mut depth = pl[3] + 0.5 * ((bb[1] + bb[2]) + bb[0]) - dot(y, n, p);
            if depth < 0.0 {
                return vec![];
            }
            let mut deepest = p;
            for i in 0..3 {
                for k in 0..3 {
                    let d = 0.5 * side[i] * r[k * 4 + i];
                    deepest[k] = if a[i] > 0.0 {
                        deepest[k] - d
                    } else {
                        deepest[k] + d
                    };
                }
            }
            let mut c = vec![(deepest, n, depth)];
            // The second and third contacts along the two shortest
            // projections (public 0.5.0's order of the cases).
            let next = |depth: &mut f32, side_i: usize| {
                *depth -= bb[side_i];
                if *depth < 0.0 {
                    return None;
                }
                let pos = std::array::from_fn(|k| {
                    let d = side[side_i] * r[k * 4 + side_i];
                    if a[side_i] > 0.0 {
                        deepest[k] + d
                    } else {
                        deepest[k] - d
                    }
                });
                Some((pos, n, *depth))
            };
            let order = if bb[0] < bb[1] {
                if bb[2] < bb[0] {
                    [2, if bb[0] < bb[1] { 0 } else { 1 }]
                } else {
                    [0, if bb[1] < bb[2] { 1 } else { 2 }]
                }
            } else if bb[2] < bb[1] {
                [2, if bb[0] < bb[1] { 0 } else { 1 }]
            } else {
                [1, if bb[0] < bb[2] { 0 } else { 2 }]
            };
            for side_i in order {
                match next(&mut depth, side_i) {
                    Some(con) => c.push(con),
                    None => break,
                }
            }
            c
        };
        (b, g, model(true), model(false))
    });
    let (sphere_hits, sphere_differ) = pairs(4000, 1, |inputs, _| {
        let radius = inputs.next(0.1, 1.5);
        // SAFETY: as above.
        let s = unsafe { dCreateSphere(std::ptr::null_mut(), radius) };
        let p = inputs.vec3(-2.0, 2.0);
        place(s, p, &rotation([1.0, 0.0, 0.0, 0.0]));
        let (g, pl) = plane(inputs);
        let model = |shipped| {
            let n = pn(pl);
            let depth = pl[3] - dot(pick(shipped, Yzx), p, n) + radius;
            if depth >= 0.0 {
                vec![(std::array::from_fn(|i| p[i] - n[i] * radius), n, depth)]
            } else {
                vec![]
            }
        };
        (s, g, model(true), model(false))
    });
    let (cyl_hits, cyl_differ) = pairs(4000, 2, |inputs, _| {
        let (radius, length) = (inputs.next(0.05, 0.8), inputs.next(0.1, 2.0));
        // SAFETY: as above.
        let c = unsafe { dCreateCCylinder(std::ptr::null_mut(), radius, length) };
        let p = inputs.vec3(-2.0, 2.0);
        let r = place(c, p, &random_rotation(inputs));
        let (g, pl) = plane(inputs);
        let model = |shipped| {
            let y = pick(shipped, Yzx);
            let n = pn(pl);
            let axis = col(&r, 2);
            let sign = if dot(y, n, axis) > 0.0 { -1.0 } else { 1.0 };
            let cap = |s: f32| -> V3 { std::array::from_fn(|i| p[i] + axis[i] * length * 0.5 * s) };
            let contact = |q: V3| {
                let depth = pl[3] - dot(y, q, n) + radius;
                (depth >= 0.0).then(|| (std::array::from_fn(|i| q[i] - n[i] * radius), n, depth))
            };
            let mut c = Vec::from_iter(contact(cap(sign)));
            if !c.is_empty() {
                let p2: V3 = std::array::from_fn(|i| p[i] - axis[i] * length * 0.5 * sign);
                c.extend(contact(p2));
            }
            c
        };
        (c, g, model(true), model(false))
    });
    let (cs_hits, cs_differ) = pairs(4000, 1, |inputs, _| {
        let (radius, length) = (inputs.next(0.05, 0.8), inputs.next(0.1, 2.0));
        let sr = inputs.next(0.1, 1.0);
        // SAFETY: as above.
        let (c, s) = unsafe {
            (
                dCreateCCylinder(std::ptr::null_mut(), radius, length),
                dCreateSphere(std::ptr::null_mut(), sr),
            )
        };
        let p = inputs.vec3(-2.0, 2.0);
        let r = place(c, p, &random_rotation(inputs));
        let ps = add(p, inputs.vec3(-1.5, 1.5));
        place(s, ps, &rotation([1.0, 0.0, 0.0, 0.0]));
        let model = |shipped| {
            let axis = col(&r, 2);
            let lz2 = length * 0.5;
            let alpha = dot(pick(shipped, Yzx), axis, sub(ps, p)).clamp(-lz2, lz2);
            let q = std::array::from_fn(|i| p[i] + alpha * axis[i]);
            Vec::from_iter(spheres(q, radius, ps, sr, Xyz))
        };
        (c, s, model(true), model(false))
    });
    for (hits, differ, what) in [
        (box_hits, box_differ, "box"),
        (sphere_hits, sphere_differ, "sphere"),
        (cyl_hits, cyl_differ, "capped cylinder"),
        (cs_hits, cs_differ, "capped cylinder-sphere"),
    ] {
        assert!(hits > 800, "{what}: {hits}");
        assert!(differ > 0, "{what}: the source order rounds the same");
    }
}

/// 2.6 and 8 item 6: rays against boxes and capped cylinders.
#[test]
fn rays_in_the_shipped_groupings() {
    let ray = |inputs: &mut Inputs, at: V3| {
        let length = inputs.next(0.5, 6.0);
        let start = add(at, inputs.vec3(-2.5, 2.5));
        let d = sub(add(at, inputs.vec3(-0.8, 0.8)), start);
        // SAFETY: a ray outside any space, destroyed by `pairs`.
        unsafe {
            let g = dCreateRay(std::ptr::null_mut(), length);
            dGeomRaySet(g, start[0], start[1], start[2], d[0], d[1], d[2]);
            (g, start, read::<12>(dGeomGetRotation(g)), length)
        }
    };
    let (box_hits, box_differ) = pairs(6000, 1, |inputs, _| {
        let side = inputs.vec3(0.2, 3.0);
        // SAFETY: as above.
        let b = unsafe { dCreateBox(std::ptr::null_mut(), side[0], side[1], side[2]) };
        let p = inputs.vec3(-2.0, 2.0);
        let r = place(b, p, &random_rotation(inputs));
        let (g, start, rr, length) = ray(inputs, p);
        let model = |shipped: bool| {
            let o = picks(shipped, [Yzx; 3]);
            let dir = col(&rr, 2);
            let mut s = mul_t(&r, sub(start, p), o);
            let mut v = mul_t(&r, dir, o);
            let mut sign = [-1.0f32; 3];
            for i in 0..3 {
                if v[i] < 0.0 {
                    s[i] = -s[i];
                    v[i] = -v[i];
                    sign[i] = 1.0;
                }
            }
            let h = side.map(|x| 0.5 * x);
            if (0..3).any(|i| (s[i] < -h[i] && v[i] <= 0.0) || s[i] > h[i]) || v == [0.0; 3] {
                return vec![];
            }
            let (mut lo, mut hi) = (f32::NEG_INFINITY, f32::INFINITY);
            let (mut nlo, mut nhi) = (0, 0);
            for i in 0..3 {
                if v[i] != 0.0 {
                    let iv = 1.0 / v[i];
                    let slab = |x: f32| if shipped { x * iv } else { x / v[i] };
                    let k = slab(-h[i] - s[i]);
                    if k > lo {
                        lo = k;
                        nlo = i;
                    }
                    let k = slab(h[i] - s[i]);
                    if k < hi {
                        hi = k;
                        nhi = i;
                    }
                }
            }
            if lo > hi {
                return vec![];
            }
            let (alpha, n) = if lo >= 0.0 { (lo, nlo) } else { (hi, nhi) };
            if alpha < 0.0 || alpha > length {
                return vec![];
            }
            let pos = std::array::from_fn(|i| start[i] + alpha * dir[i]);
            vec![(pos, col(&r, n).map(|x| x * sign[n]), alpha)]
        };
        (g, b, model(true), model(false))
    });
    let (cyl_hits, cyl_differ) = pairs(6000, 1, |inputs, _| {
        let (radius, length) = (inputs.next(0.05, 0.8), inputs.next(0.1, 2.0));
        // SAFETY: as above.
        let c = unsafe { dCreateCCylinder(std::ptr::null_mut(), radius, length) };
        let p = inputs.vec3(-2.0, 2.0);
        let r = place(c, p, &random_rotation(inputs));
        let (g, start, rr, ray_length) = ray(inputs, p);
        let model =
            |shipped| ray_capped_cylinder(start, &rr, ray_length, (p, &r, radius, length), shipped);
        (g, c, model(true), model(false))
    });
    for (hits, differ, what) in [
        (box_hits, box_differ, "box"),
        (cyl_hits, cyl_differ, "capped cylinder"),
    ] {
        assert!(hits > 1000, "{what}: {hits}");
        assert!(differ > 0, "{what}: the source order rounds the same");
    }
}

/// Round 1's ray–sphere helper (physics-harness.md 5.2), which the capped
/// cylinder's caps use; `exit` for its mode 1.
fn ray_sphere(start: V3, dir: V3, length: f32, c: V3, radius: f32, exit: bool) -> Vec<Con> {
    let q = sub(start, c);
    let b = dot(Yzx, q, dir);
    let cc = dot(Yzx, q, q) - radius * radius;
    let k = b * b - cc;
    if k < 0.0 {
        return vec![];
    }
    let k = k.sqrt();
    let alpha = if (exit && cc >= 0.0) || -b - k < 0.0 {
        -b + k
    } else {
        -b - k
    };
    if alpha < 0.0 || alpha > length {
        return vec![];
    }
    let pos: V3 = std::array::from_fn(|i| start[i] + alpha * dir[i]);
    let nsign = if cc < 0.0 || exit { -1.0 } else { 1.0 };
    vec![(pos, normalise(sub(pos, c).map(|x| nsign * x)), alpha)]
}

/// 2.6: a ray (start, rotation, length) against a capped cylinder.
fn ray_capped_cylinder(
    start: V3,
    rr: &M3,
    ray_length: f32,
    cyl: (V3, &M3, f32, f32),
    shipped: bool,
) -> Vec<Con> {
    let (p, r, radius, length) = cyl;
    let y = pick(shipped, Yzx);
    let lz2 = length * 0.5;
    let axis = col(r, 2);
    let dir = col(rr, 2);
    let cs = sub(start, p);
    let mut k = dot(y, axis, cs);
    let q: V3 = std::array::from_fn(|i| k * axis[i] - cs[i]);
    let c = dot(y, q, q) - radius * radius;
    let mut inside = false;
    if c < 0.0 {
        k = k.clamp(-lz2, lz2);
        let rp: V3 = std::array::from_fn(|i| p[i] + k * axis[i]);
        let d = sub(start, rp);
        inside = d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < radius * radius;
    }
    if !inside && c < 0.0 {
        k = if k < 0.0 { -lz2 } else { lz2 };
    } else {
        let uv = dot(y, axis, dir);
        let rv: V3 = std::array::from_fn(|i| uv * axis[i] - dir[i]);
        let a = dot(y, rv, rv);
        let b = 2.0 * dot(y, q, rv);
        let kk = b * b - 4.0 * a * c;
        if kk < 0.0 {
            if !inside {
                return vec![];
            }
            k = if uv < 0.0 { -lz2 } else { lz2 };
        } else {
            let kk = kk.sqrt();
            let a = 1.0 / (2.0 * a);
            let mut alpha = (-b - kk) * a;
            if alpha < 0.0 {
                alpha = (-b + kk) * a;
                if alpha < 0.0 {
                    return vec![];
                }
            }
            if alpha > ray_length {
                return vec![];
            }
            let pos: V3 = std::array::from_fn(|i| start[i] + alpha * dir[i]);
            let qq = sub(pos, p);
            k = dot(y, qq, axis);
            let nsign = if inside { -1.0 } else { 1.0 };
            if k >= -lz2 && k <= lz2 {
                let n: V3 = std::array::from_fn(|i| nsign * (pos[i] - (p[i] + k * axis[i])));
                return vec![(pos, normalise(n), alpha)];
            }
            k = if k < 0.0 { -lz2 } else { lz2 };
        }
    }
    let cap = std::array::from_fn(|i| p[i] + k * axis[i]);
    ray_sphere(start, dir, ray_length, cap, radius, inside)
}

unsafe extern "C" {
    fn dWorldCreate() -> *mut DxWorld;
    fn dWorldDestroy(w: *mut DxWorld);
    fn dWorldSetGravity(w: *mut DxWorld, x: Real, y: Real, z: Real);
    fn dWorldQuickStep(w: *mut DxWorld, stepsize: Real);
    fn dWorldStep(w: *mut DxWorld, stepsize: Real);
    fn dBodyCreate(w: *mut DxWorld) -> *mut DxBody;
    fn dBodyDestroy(b: *mut DxBody);
    fn dBodySetMass(b: *mut DxBody, mass: *const DMass);
    fn dBodySetPosition(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetPosition(b: *mut DxBody) -> *const Real;
    fn dBodySetLinearVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetLinearVel(b: *mut DxBody) -> *const Real;
    fn dBodySetAngularVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetAngularVel(b: *mut DxBody) -> *const Real;
    fn dBodySetQuaternion(b: *mut DxBody, q: *const Real);
    fn dBodyGetQuaternion(b: *mut DxBody) -> *const Real;
    fn dBodyGetRotation(b: *mut DxBody) -> *const Real;
    fn dBodySetForce(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetTorque(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetForce(b: *mut DxBody) -> *const Real;
    fn dBodyGetTorque(b: *mut DxBody) -> *const Real;
    fn dBodyAddTorque(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyAddRelForce(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyAddRelTorque(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyAddForceAtRelPos(
        b: *mut DxBody,
        fx: Real,
        fy: Real,
        fz: Real,
        px: Real,
        py: Real,
        pz: Real,
    );
    fn dBodyAddRelForceAtPos(
        b: *mut DxBody,
        fx: Real,
        fy: Real,
        fz: Real,
        px: Real,
        py: Real,
        pz: Real,
    );
    fn dBodySetFiniteRotationMode(b: *mut DxBody, mode: c_int);
    fn dBodySetFiniteRotationAxis(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetFiniteRotationAxis(b: *mut DxBody, result: *mut Real);
    fn dxStepBody(b: *mut DxBody, h: Real);
    fn dMassSetCappedCylinder(m: *mut DMass, density: Real, direction: c_int, r: Real, l: Real);
    fn dMassSetCappedCylinderTotal(m: *mut DMass, total: Real, direction: c_int, r: Real, l: Real);
    fn dMassSetCylinder(m: *mut DMass, density: Real, direction: c_int, r: Real, l: Real);
    fn dMassSetCylinderTotal(m: *mut DMass, total: Real, direction: c_int, r: Real, l: Real);
    fn dMassSetSphere(m: *mut DMass, density: Real, radius: Real);
    fn dMassSetBox(m: *mut DMass, density: Real, lx: Real, ly: Real, lz: Real);
    fn dMassSetBoxTotal(m: *mut DMass, total: Real, lx: Real, ly: Real, lz: Real);
    fn dMassRotate(m: *mut DMass, r: *const Real);
    fn dNormalize4(a: *mut Real);
    fn dQfromR(q: *mut Real, r: *const Real);
    fn dQFromAxisAndAngle(q: *mut Real, ax: Real, ay: Real, az: Real, angle: Real);
}

/// Round 1's infinitesimal update's derivative (physics-harness.md 4.2):
/// `½ (w ⊗ q)`, the third component's terms first and last first.
fn derivative(w: V3, q: [f32; 4]) -> [f32; 4] {
    [
        0.5 * (-w[0] * q[1] - w[1] * q[2] - w[2] * q[3]),
        0.5 * (w[0] * q[0] + w[1] * q[3] - w[2] * q[2]),
        0.5 * (-w[0] * q[3] + w[2] * q[1] + w[1] * q[0]),
        0.5 * (w[0] * q[2] - w[1] * q[1] + w[2] * q[0]),
    ]
}

/// q p, components 1 to 3 summed first, third, second, then the fourth term
/// (the shipped library's), or as `dQMultiply0`.
fn finite_product(q: [f32; 4], p: [f32; 4], shipped: bool) -> [f32; 4] {
    let first = q[0] * p[0] - q[1] * p[1] - q[2] * p[2] - q[3] * p[3];
    if shipped {
        [
            first,
            q[0] * p[1] + q[2] * p[3] + q[1] * p[0] - q[3] * p[2],
            q[0] * p[2] + q[3] * p[1] + q[2] * p[0] - q[1] * p[3],
            q[0] * p[3] + q[1] * p[2] + q[3] * p[0] - q[2] * p[1],
        ]
    } else {
        [
            first,
            q[0] * p[1] + q[1] * p[0] + q[2] * p[3] - q[3] * p[2],
            q[0] * p[2] + q[2] * p[0] + q[3] * p[1] - q[1] * p[3],
            q[0] * p[3] + q[3] * p[0] + q[1] * p[2] - q[2] * p[1],
        ]
    }
}

/// sin θ / θ: the sine unrounded (the shipped library's) or rounded first;
/// the series below 10⁻⁴.
fn sinc(theta: f32, shipped: bool) -> f32 {
    if f64::from(theta.abs()) < 1e-4 {
        1.0 - theta * theta * (1.0f32 / 6.0)
    } else if shipped {
        x87::sin_over(theta, theta)
    } else {
        x87::sin(theta) / theta
    }
}

/// 7.1: `dxStepBody`'s finite rotation about the unit `axis` (as stored),
/// the quaternion after the step.
fn finite_rotation_about(axis: V3, w: V3, p: [f32; 4], h: f32, shipped: bool) -> [f32; 4] {
    let k = dot(pick(shipped, Yzx), axis, w);
    let frv = axis.map(|a| a * k);
    let irv = sub(w, frv);
    let hh = h * 0.5;
    let theta = k * hh;
    let s = sinc(theta, shipped) * hh;
    let q = [x87::cos(theta), frv[0] * s, frv[1] * s, frv[2] * s];
    let q2 = finite_product(q, p, shipped);
    let dq = derivative(irv, q2);
    normalise4(std::array::from_fn(|j| q2[j] + hh * dq[j]))
}

/// 7.1 without an axis: the squared length summed (x + z) + y (the shipped
/// library's) or in index order, then round 2's rotation.
fn finite_rotation_without(w: V3, p: [f32; 4], h: f32, xzy: bool) -> [f32; 4] {
    let l = if xzy {
        w[0] * w[0] + w[2] * w[2] + w[1] * w[1]
    } else {
        w[0] * w[0] + w[1] * w[1] + w[2] * w[2]
    };
    let hh = h * 0.5;
    let theta = l.sqrt() * hh;
    let s = sinc(theta, true) * hh;
    let q = [x87::cos(theta), w[0] * s, w[1] * s, w[2] * s];
    normalise4(finite_product(q, p, true))
}

/// 7.1 and 8 item 26.
#[test]
fn finite_rotation_in_the_shipped_groupings() {
    let mut inputs = Inputs::new();
    let (mut about, mut series, mut without) = (0, 0, 0);
    // SAFETY: bodies outside any world, made, stepped and destroyed here.
    unsafe {
        let b = dBodyCreate(std::ptr::null_mut());
        for i in 0..3000 {
            let q = unit_quaternion(&mut inputs);
            dBodySetQuaternion(b, q.as_ptr());
            let p = read::<4>(dBodyGetQuaternion(b));
            // Every eighth angular velocity small: angles below 10⁻⁴.
            let scale = if i % 8 == 0 { 1e-3 } else { 10.0 };
            let w = inputs.vec3(-scale, scale);
            let h = inputs.next(0.005, 0.05);
            dBodySetAngularVel(b, w[0], w[1], w[2]);
            if i % 2 == 0 {
                let a = inputs.vec3(-1.0, 1.0);
                dBodySetFiniteRotationMode(b, 1);
                dBodySetFiniteRotationAxis(b, a[0], a[1], a[2]);
                let mut axis = [0.0; 4];
                dBodyGetFiniteRotationAxis(b, axis.as_mut_ptr());
                let axis = three(axis);
                dxStepBody(b, h);
                let rule = finite_rotation_about(axis, w, p, h, true);
                assert_eq!(
                    bits(read::<4>(dBodyGetQuaternion(b))),
                    bits(rule),
                    "{axis:?} {w:?} {p:?} {h}"
                );
                about +=
                    usize::from(bits(rule) != bits(finite_rotation_about(axis, w, p, h, false)));
                series += usize::from((dot(Yzx, axis, w) * h * 0.5).abs() < 1e-4);
            } else {
                dBodySetFiniteRotationMode(b, 1);
                dBodySetFiniteRotationAxis(b, 0.0, 0.0, 0.0);
                dxStepBody(b, h);
                let rule = finite_rotation_without(w, p, h, true);
                assert_eq!(
                    bits(read::<4>(dBodyGetQuaternion(b))),
                    bits(rule),
                    "{w:?} {p:?} {h}"
                );
                without += usize::from(bits(rule) != bits(finite_rotation_without(w, p, h, false)));
            }
            dBodySetFiniteRotationMode(b, 0);
        }
        dBodyDestroy(b);
    }
    assert!(series > 50, "{series}");
    assert!(about > 0, "about an axis: the public rule rounds the same");
    assert!(without > 0, "without an axis: index order rounds the same");
}

/// 7.2: `dMassSetCappedCylinder`'s mass and inertia (the shipped
/// library's groupings, or public 0.5.0's), then `dMassAdjust`'s scale for
/// the total form.
fn capped_cylinder_mass(density: f32, direction: usize, r: f32, l: f32, shipped: bool) -> DMass {
    let pi = std::f32::consts::PI;
    let k = 1.0f32 / 12.0;
    let (mass, ia, ib) = if shipped {
        let p = density * r * r;
        let m1 = p * l * pi;
        let m2 = p * r * 4.188_790_3;
        let ia = m2 * ((0.4 * r + 0.375 * l) * r + l * l * 0.25) + m1 * (r * r * 0.25 + l * l * k);
        (m2 + m1, ia, (m2 * 0.4 + m1 * 0.5) * r * r)
    } else {
        let m1 = pi * r * r * l * density;
        let m2 = (4.0f32 / 3.0) * pi * r * r * r * density;
        let ia =
            m1 * (0.25 * r * r + k * l * l) + m2 * (0.4 * r * r + 0.375 * r * l + 0.25 * l * l);
        (m1 + m2, ia, (m1 * 0.5 + m2 * 0.4) * r * r)
    };
    let mut m = DMass {
        mass,
        ..DMass::default()
    };
    for i in 0..3 {
        m.i[i * 5] = if i + 1 == direction { ib } else { ia };
    }
    m
}

fn adjust(mut m: DMass, total: f32) -> DMass {
    let scale = total / m.mass;
    m.mass = total;
    for i in 0..3 {
        for j in 0..3 {
            m.i[i * 4 + j] *= scale;
        }
    }
    m
}

fn mass_bits(m: &DMass) -> (u32, [u32; 4], [u32; 12]) {
    (m.mass.to_bits(), bits(m.c), bits(m.i))
}

/// 7.2, 7.3 and 8 items 27 and 28: the mass functions.
#[test]
fn mass_functions_in_the_shipped_groupings() {
    assert_eq!(
        4.188_790_3f32.to_bits(),
        ((4.0f32 / 3.0) * std::f32::consts::PI).to_bits()
    );
    let mut inputs = Inputs::new();
    let mut differ = [0; 6];
    let pi = std::f32::consts::PI;
    let k = 1.0f32 / 12.0;
    for _ in 0..3000 {
        let d = inputs.next(0.1, 50.0);
        let t = inputs.next(0.1, 200.0);
        let (r, l) = (inputs.next(0.02, 2.0), inputs.next(0.02, 3.0));
        let dir = 1 + (inputs.next(0.0, 3.0) as usize).min(2);
        let s = inputs.vec3(0.05, 5.0);
        let mut m = DMass::default();
        // SAFETY: masses.
        unsafe {
            let check = |m: &DMass, want: &DMass, old: &DMass, n: &mut usize| {
                assert_eq!(mass_bits(m), mass_bits(want));
                *n += usize::from(mass_bits(want) != mass_bits(old));
            };
            dMassSetCappedCylinder(&mut m, d, dir as c_int, r, l);
            let want = capped_cylinder_mass(d, dir, r, l, true);
            check(
                &m,
                &want,
                &capped_cylinder_mass(d, dir, r, l, false),
                &mut differ[0],
            );
            dMassSetCappedCylinderTotal(&mut m, t, dir as c_int, r, l);
            let want = adjust(capped_cylinder_mass(1.0, dir, r, l, true), t);
            let old = adjust(capped_cylinder_mass(1.0, dir, r, l, false), t);
            check(&m, &want, &old, &mut differ[1]);

            // The sphere: the density times the radius three times, then
            // 4/3 π; the inertia as round 2's total form.
            let sphere = |mass: f32| {
                let ii = mass * r * r * 0.4;
                DMass {
                    mass,
                    c: [0.0; 4],
                    i: [ii, 0.0, 0.0, 0.0, 0.0, ii, 0.0, 0.0, 0.0, 0.0, ii, 0.0],
                }
            };
            dMassSetSphere(&mut m, d, r);
            let old = sphere((4.0f32 / 3.0) * pi * r * r * r * d);
            check(
                &m,
                &sphere(d * r * r * r * 4.188_790_3),
                &old,
                &mut differ[2],
            );

            // The box: the density times the sides.
            let cuboid = |mass: f32| {
                let sq = |a: f32, b: f32| a * a + b * b;
                let mut want = DMass {
                    mass,
                    ..DMass::default()
                };
                want.i[0] = sq(s[1], s[2]) * mass * k;
                want.i[5] = sq(s[0], s[2]) * mass * k;
                want.i[10] = sq(s[0], s[1]) * mass * k;
                want
            };
            dMassSetBox(&mut m, d, s[0], s[1], s[2]);
            check(
                &m,
                &cuboid(d * s[0] * s[1] * s[2]),
                &cuboid(s[0] * s[1] * s[2] * d),
                &mut differ[3],
            );

            // The cylinder.
            let cylinder = |mass: f32, shipped: bool| {
                let r2 = r * r;
                let (ia, ib) = if shipped {
                    ((l * l * k + r2 * 0.25) * mass, r2 * mass * 0.5)
                } else {
                    (mass * (0.25 * r2 + k * l * l), mass * 0.5 * r2)
                };
                let mut want = DMass {
                    mass,
                    ..DMass::default()
                };
                for i in 0..3 {
                    want.i[i * 5] = if i + 1 == dir { ib } else { ia };
                }
                want
            };
            dMassSetCylinderTotal(&mut m, t, dir as c_int, r, l);
            check(&m, &cylinder(t, true), &cylinder(t, false), &mut differ[4]);
            dMassSetCylinder(&mut m, d, dir as c_int, r, l);
            let old = cylinder(pi * r * r * l * d, false);
            check(
                &m,
                &cylinder(d * r * r * l * pi, true),
                &old,
                &mut differ[5],
            );
        }
    }
    assert!(differ.iter().all(|&n| n > 0), "{differ:?}");
}

/// 7.3: `dMassRotate`'s inertia R (I Rᵀ) and centre R c.
fn rotate_mass(m: &DMass, r: &M3, shipped: bool) -> DMass {
    let mut out = DMass {
        mass: m.mass,
        ..DMass::default()
    };
    out.i = if shipped {
        world_frame(r, &m.i, [Yzx; 3])
    } else {
        let mut tmp = [0.0; 12];
        for k in 0..3 {
            for j in 0..3 {
                tmp[k * 4 + j] = dot(Xyz, row(&m.i, k), row(r, j));
            }
        }
        std::array::from_fn(|e| {
            let (i, j) = (e / 4, e % 4);
            if j == 3 {
                0.0
            } else {
                dot(Xyz, row(r, i), col(&tmp, j))
            }
        })
    };
    // The upper entries copied to the lower ones.
    out.i[4] = out.i[1];
    out.i[8] = out.i[2];
    out.i[9] = out.i[6];
    let c = mul331(r, three(m.c), picks(shipped, [Yzx; 3]));
    out.c = [c[0], c[1], c[2], 0.0];
    out
}

/// 7.3 and 8 item 28: `dMassRotate`.
#[test]
fn mass_rotation_in_the_shipped_groupings() {
    let mut inputs = Inputs::new();
    let mut differ = 0;
    for _ in 0..3000 {
        let s = inputs.vec3(0.1, 3.0);
        let mut m = DMass::default();
        // SAFETY: masses.
        unsafe {
            dMassSetBoxTotal(&mut m, inputs.next(0.5, 50.0), s[0], s[1], s[2]);
            // A rotated mass rotated again: a full inertia matrix.
            let r1 = random_rotation(&mut inputs);
            dMassRotate(&mut m, r1.as_ptr());
            let c = inputs.vec3(-0.5, 0.5);
            m.c = [c[0], c[1], c[2], 0.0];
            let before = DMass {
                mass: m.mass,
                c: m.c,
                i: m.i,
            };
            let r = random_rotation(&mut inputs);
            dMassRotate(&mut m, r.as_ptr());
            let want = rotate_mass(&before, &r, true);
            assert_eq!(mass_bits(&m), mass_bits(&want));
            differ += usize::from(mass_bits(&want) != mass_bits(&rotate_mass(&before, &r, false)));
        }
    }
    assert!(differ > 0, "the source order rounds the same");
}

/// The inverse of a positive definite 3×3 matrix as `dInvertPDMatrix`
/// computes it (public 0.5.0's Cholesky factorisation and solves).
fn invert_pd(a: &M3) -> M3 {
    let mut l = *a;
    let mut recip = [0.0f32; 3];
    for i in 0..3 {
        for j in 0..i {
            let mut sum = l[i * 4 + j];
            for k in 0..j {
                sum -= l[i * 4 + k] * l[j * 4 + k];
            }
            l[i * 4 + j] = sum * recip[j];
        }
        let mut sum = l[i * 4 + i];
        for k in 0..i {
            sum -= l[i * 4 + k] * l[i * 4 + k];
        }
        l[i * 4 + i] = sum.sqrt();
        recip[i] = 1.0 / l[i * 4 + i];
    }
    let mut inv = [0.0; 12];
    for c in 0..3 {
        let mut b = [0.0f32; 3];
        b[c] = 1.0;
        let mut y = [0.0f32; 3];
        for i in 0..3 {
            let mut sum = 0.0f32;
            for k in 0..i {
                sum += l[i * 4 + k] * y[k];
            }
            y[i] = (b[i] - sum) / l[i * 4 + i];
        }
        for i in (0..3).rev() {
            let mut sum = 0.0f32;
            for k in i + 1..3 {
                sum += l[k * 4 + i] * b[k];
            }
            b[i] = (y[i] - sum) / l[i * 4 + i];
        }
        for j in 0..3 {
            inv[j * 4 + c] = b[j];
        }
    }
    inv
}

/// R D Rᵀ with public 0.5.0's body-frame product (source order).
fn world_frame_public(r: &M3, d: &M3, o: [Sum; 3]) -> M3 {
    let mut tmp = [0.0; 12];
    for k in 0..3 {
        for j in 0..3 {
            tmp[k * 4 + j] = dot(Xyz, row(d, k), row(r, j));
        }
    }
    std::array::from_fn(|e| {
        let (i, j) = (e / 4, e % 4);
        if j == 3 {
            0.0
        } else {
            dot(o[i], row(r, i), col(&tmp, j))
        }
    })
}

/// 7.4: a lone body's angular velocity after a step with a general
/// inertia (round 2's 2.4 with this round's body-frame products).
fn angular_step_general(
    o: &Orders,
    r: &M3,
    inertia: &M3,
    w: V3,
    t: V3,
    h: f32,
    shipped: bool,
) -> V3 {
    let frame = if shipped {
        world_frame
    } else {
        world_frame_public
    };
    let inv = invert_pd(inertia);
    let m = frame(r, &inv, o.inv);
    let j = frame(r, inertia, o.inertia);
    let l = mul331(&j, w, o.l);
    let t = sub(t, cross(w, l));
    let dw = mul331(&m, t.map(|x| h * x), o.update);
    add(w, dw)
}

fn angular_update_general(step: unsafe extern "C" fn(*mut DxWorld, Real), o: &Orders) {
    let _turn = lock();
    let mut inputs = Inputs::new();
    let mut differ = 0;
    // SAFETY: a world and a body made, stepped and destroyed here, under the
    // lock.
    unsafe {
        let world = dWorldCreate();
        dWorldSetGravity(world, 0.0, 0.0, 0.0);
        let b = dBodyCreate(world);
        for _ in 0..2000 {
            let s = inputs.vec3(0.1, 3.0);
            let mut mass = DMass::default();
            dMassSetBoxTotal(&mut mass, inputs.next(0.5, 50.0), s[0], s[1], s[2]);
            let rm = random_rotation(&mut inputs);
            dMassRotate(&mut mass, rm.as_ptr());
            mass.c = [0.0; 4];
            dBodySetMass(b, &mass);
            let q = unit_quaternion(&mut inputs);
            dBodySetQuaternion(b, q.as_ptr());
            let r = read::<12>(dBodyGetRotation(b));
            let w = inputs.vec3(-8.0, 8.0);
            let t = inputs.vec3(-20.0, 20.0);
            let h = inputs.next(0.005, 0.05);
            dBodySetLinearVel(b, 0.0, 0.0, 0.0);
            dBodySetAngularVel(b, w[0], w[1], w[2]);
            dBodyAddTorque(b, t[0], t[1], t[2]);
            step(world, h);
            let got = read::<3>(dBodyGetAngularVel(b));
            let rule = angular_step_general(o, &r, &mass.i, w, t, h, true);
            assert_eq!(bits(got), bits(rule), "{r:?} {w:?} {t:?} {h}");
            differ += usize::from(
                bits(rule) != bits(angular_step_general(o, &r, &mass.i, w, t, h, false)),
            );
        }
        dBodyDestroy(b);
        dWorldDestroy(world);
    }
    assert!(differ > 0, "the source order rounds the same");
}

/// 7.4 and 8 item 29, `dWorldQuickStep`.
#[test]
fn quickstep_angular_update_with_a_general_inertia() {
    angular_update_general(dWorldQuickStep, &QUICKSTEP);
}

/// 7.4 and 8 item 29, `dWorldStep`.
#[test]
fn exact_step_angular_update_with_a_general_inertia() {
    angular_update_general(dWorldStep, &EXACT);
}

/// 7.5: the rotation-to-quaternion conversion (`dQfromR`), the trace summed
/// R00 + (R11 + R22) (the shipped library's) or in source order.
fn quaternion_from_rotation(m: &M3, shipped: bool) -> [f32; 4] {
    let r = |i: usize, j: usize| m[i * 4 + j];
    let tr = if shipped {
        r(0, 0) + (r(1, 1) + r(2, 2))
    } else {
        r(0, 0) + r(1, 1) + r(2, 2)
    };
    if tr >= 0.0 {
        let s = (tr + 1.0).sqrt();
        let k = 0.5 * (1.0 / s);
        return [
            0.5 * s,
            (r(2, 1) - r(1, 2)) * k,
            (r(0, 2) - r(2, 0)) * k,
            (r(1, 0) - r(0, 1)) * k,
        ];
    }
    let case = if r(1, 1) > r(0, 0) {
        if r(2, 2) > r(1, 1) { 2 } else { 1 }
    } else if r(2, 2) > r(0, 0) {
        2
    } else {
        0
    };
    let mut q = [0.0; 4];
    let (i, j, k) = [(0, 1, 2), (1, 2, 0), (2, 0, 1)][case];
    let s = ((r(i, i) - (r(j, j) + r(k, k))) + 1.0).sqrt();
    q[1 + i] = 0.5 * s;
    let s = 0.5 * (1.0 / s);
    q[1 + j] = (r(i, j) + r(j, i)) * s;
    q[1 + k] = (r(k, i) + r(i, k)) * s;
    q[0] = (r(k, j) - r(j, k)) * s;
    q
}

/// 7.5 and 8 item 30: the conversion in `dQfromR` and `dGeomGetQuaternion`,
/// and `dQFromAxisAndAngle`.
#[test]
fn rotation_conversions_in_the_shipped_groupings() {
    let mut inputs = Inputs::new();
    let (mut positive, mut differ, mut axis_differ) = (0, 0, 0);
    // SAFETY: a geometry outside any space and plain functions.
    unsafe {
        let g = dCreateSphere(std::ptr::null_mut(), 1.0);
        for i in 0..4000 {
            let mut q = unit_quaternion(&mut inputs);
            if i % 2 == 0 {
                // Near the identity: traces near 3.
                q[0] = 4.0;
                dNormalize4(q.as_mut_ptr());
            }
            let m = rotation(q);
            let want = quaternion_from_rotation(&m, true);
            let mut got = [0.0; 4];
            dQfromR(got.as_mut_ptr(), m.as_ptr());
            assert_eq!(bits(got), bits(want), "{m:?}");
            dGeomSetRotation(g, m.as_ptr());
            dGeomGetQuaternion(g, got.as_mut_ptr());
            assert_eq!(bits(got), bits(want), "{m:?}");
            positive += usize::from(m[0] + (m[5] + m[10]) >= 0.0);
            differ += usize::from(bits(want) != bits(quaternion_from_rotation(&m, false)));

            let a = inputs.vec3(-3.0, 3.0);
            let angle = inputs.next(-7.0, 7.0);
            dQFromAxisAndAngle(got.as_mut_ptr(), a[0], a[1], a[2], angle);
            let l = a[0] * a[0] + a[1] * a[1] + a[2] * a[2];
            let half = angle * 0.5;
            let k = x87::sin_times(half, 1.0 / l.sqrt());
            let want = [x87::cos(half), a[0] * k, a[1] * k, a[2] * k];
            assert_eq!(bits(got), bits(want), "{a:?} {angle}");
            let k = x87::sin(half) * (1.0 / l.sqrt());
            axis_differ += usize::from(bits(want) != bits([want[0], a[0] * k, a[1] * k, a[2] * k]));
        }
        dGeomDestroy(g);
    }
    assert!(positive > 1500 && positive < 3800, "{positive}");
    assert!(differ > 0, "the source order rounds the same");
    assert!(axis_differ > 0, "rounding the sine first rounds the same");
}

/// x · y on the x87 at 24-bit precision, stored: rounded to 24 bits with the
/// wider exponent, then to the float's grid.
fn x87_mul(x: f32, y: f32) -> f32 {
    let p = f64::from(x) * f64::from(y);
    if p.abs() >= f64::from(f32::MIN_POSITIVE) {
        return p as f32;
    }
    let scale = 2f64.powi(200);
    (f64::from((p * scale) as f32) / scale) as f32
}

/// 7.5 and 8 item 31: `dNormalize4`'s products on the x87, not flushed: a
/// product below the normal range is a denormal rounded twice.
#[test]
fn quaternion_normalisation_is_not_flushed() {
    let mut inputs = Inputs::new();
    let (mut denormal, mut twice) = (0, 0);
    for _ in 0..4000 {
        // Two components near 1 and two just above 2^-126, whose products
        // with the reciprocal length (below 0.89) fall below the normal
        // range.
        let tiny = f32::MIN_POSITIVE * inputs.next(1.0, 1.1);
        let mut q = [inputs.next(0.8, 1.0), inputs.next(0.8, 1.0), tiny, -tiny];
        let l = q[0] * q[0] + q[1] * q[1];
        let s = 1.0 / l.sqrt();
        let want = q.map(|x| x87_mul(x, s));
        // SAFETY: a quaternion.
        unsafe { dNormalize4(q.as_mut_ptr()) };
        assert_eq!(bits(q), bits(want), "{tiny:e}");
        denormal += usize::from(want[2] != 0.0 && want[2].abs() < f32::MIN_POSITIVE);
        // Rounded once to the denormal grid instead.
        twice += usize::from(want[2] != (f64::from(tiny) * f64::from(s)) as f32);
    }
    assert!(denormal > 1000, "{denormal}");
    assert!(twice > 0, "rounding once rounds the same");
}

/// 7.6 and 8 item 32: the force and torque functions' rotations.
#[test]
fn force_functions_rotate_last_two_first() {
    let mut inputs = Inputs::new();
    let mut differ = [0; 4];
    // SAFETY: a body outside any world, made and destroyed here.
    unsafe {
        let b = dBodyCreate(std::ptr::null_mut());
        for _ in 0..2000 {
            let q = unit_quaternion(&mut inputs);
            dBodySetQuaternion(b, q.as_ptr());
            let pos = inputs.vec3(-5.0, 5.0);
            dBodySetPosition(b, pos[0], pos[1], pos[2]);
            let r = read::<12>(dBodyGetRotation(b));
            let f0 = inputs.vec3(-10.0, 10.0);
            let t0 = inputs.vec3(-10.0, 10.0);
            let f = inputs.vec3(-100.0, 100.0);
            let p = inputs.vec3(-6.0, 6.0);
            let rot = |v: V3, shipped: bool| mul331(&r, v, picks(shipped, [Yzx; 3]));
            let reset = || {
                dBodySetForce(b, f0[0], f0[1], f0[2]);
                dBodySetTorque(b, t0[0], t0[1], t0[2]);
            };
            let check = |want: (V3, V3), old: (V3, V3), n: &mut usize| {
                let got = (read::<3>(dBodyGetForce(b)), read::<3>(dBodyGetTorque(b)));
                assert_eq!((bits(got.0), bits(got.1)), (bits(want.0), bits(want.1)));
                *n += usize::from((bits(want.0), bits(want.1)) != (bits(old.0), bits(old.1)));
            };
            reset();
            dBodyAddRelForce(b, f[0], f[1], f[2]);
            let model = |s| (add(f0, rot(f, s)), t0);
            check(model(true), model(false), &mut differ[0]);
            reset();
            dBodyAddRelTorque(b, f[0], f[1], f[2]);
            let model = |s| (f0, add(t0, rot(f, s)));
            check(model(true), model(false), &mut differ[1]);
            reset();
            dBodyAddForceAtRelPos(b, f[0], f[1], f[2], p[0], p[1], p[2]);
            let model = |s| (add(f0, f), add(t0, cross(rot(p, s), f)));
            check(model(true), model(false), &mut differ[2]);
            reset();
            dBodyAddRelForceAtPos(b, f[0], f[1], f[2], p[0], p[1], p[2]);
            let model = |s| {
                let fw = rot(f, s);
                (add(f0, fw), add(t0, cross(sub(p, pos), fw)))
            };
            check(model(true), model(false), &mut differ[3]);
        }
        dBodyDestroy(b);
    }
    assert!(differ.iter().all(|&n| n > 0), "{differ:?}");
}

/// ODE's `dJointFeedback`: f1, t1, f2, t2.
type Feedback = [[Real; 4]; 4];

unsafe extern "C" {
    fn dWorldGetERP(w: *mut DxWorld) -> Real;
    fn dWorldGetCFM(w: *mut DxWorld) -> Real;
    fn dWorldGetQuickStepW(w: *mut DxWorld) -> Real;
    fn dWorldGetQuickStepNumIterations(w: *mut DxWorld) -> c_int;
    fn dWorldGetContactSurfaceLayer(w: *mut DxWorld) -> Real;
    fn dWorldGetContactMaxCorrectingVel(w: *mut DxWorld) -> Real;
    fn dWorldAddJoint(w: *mut DxWorld, j: *mut DxJoint);
    fn dBodyGetMass(b: *mut DxBody, mass: *mut DMass);
    fn dBodyAddForce(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dJointCreateContact(
        w: *mut DxWorld,
        g: *mut DxJointGroup,
        c: *const DContact,
    ) -> *mut DxJoint;
    fn dJointAttach(j: *mut DxJoint, b1: *mut DxBody, b2: *mut DxBody);
    fn dJointSetFeedback(j: *mut DxJoint, f: *mut Feedback);
    fn dJointDestroy(j: *mut DxJoint);
}

/// The library-wide parameters a step reads.
struct Params {
    erp: f32,
    cfm: f32,
    w: f32,
    iterations: usize,
    layer: f32,
    max_vel: f32,
    gravity: V3,
}

fn params(world: *mut DxWorld, gravity: V3) -> Params {
    // SAFETY: the library's getters.
    unsafe {
        Params {
            erp: dWorldGetERP(world),
            cfm: dWorldGetCFM(world),
            w: dWorldGetQuickStepW(world),
            iterations: dWorldGetQuickStepNumIterations(world) as usize,
            layer: dWorldGetContactSurfaceLayer(world),
            max_vel: dWorldGetContactMaxCorrectingVel(world),
            gravity,
        }
    }
}

/// A body as the steppers read it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Body {
    mass: f32,
    inv_mass: f32,
    i: M3,
    inv_i: M3,
    pub(super) r: M3,
    pub(super) q: [f32; 4],
    pub(super) pos: V3,
    pub(super) lvel: V3,
    pub(super) avel: V3,
    facc: V3,
    tacc: V3,
}

/// A body of the library's, read back.
pub(super) fn body_of(b: *mut DxBody, facc: V3, tacc: V3) -> Body {
    let mut m = DMass::default();
    // SAFETY: a body.
    unsafe {
        dBodyGetMass(b, &mut m);
        Body {
            mass: m.mass,
            inv_mass: 1.0 / m.mass,
            i: m.i,
            inv_i: invert_pd(&m.i),
            r: read::<12>(dBodyGetRotation(b)),
            q: read::<4>(dBodyGetQuaternion(b)),
            pos: read::<3>(dBodyGetPosition(b)),
            lvel: read::<3>(dBodyGetLinearVel(b)),
            avel: read::<3>(dBodyGetAngularVel(b)),
            facc,
            tacc,
        }
    }
}

/// A random body of a world: a box mass rotated (a full inertia), a pose,
/// velocities and accumulated force and torque.
pub(super) fn random_body(world: *mut DxWorld, inputs: &mut Inputs) -> *mut DxBody {
    // SAFETY: a body made in the world.
    unsafe {
        let b = dBodyCreate(world);
        let s = inputs.vec3(0.2, 2.0);
        let mut m = DMass::default();
        dMassSetBoxTotal(&mut m, inputs.next(0.5, 50.0), s[0], s[1], s[2]);
        let r = random_rotation(inputs);
        dMassRotate(&mut m, r.as_ptr());
        m.c = [0.0; 4];
        dBodySetMass(b, &m);
        let q = unit_quaternion(inputs);
        dBodySetQuaternion(b, q.as_ptr());
        let p = inputs.vec3(-3.0, 3.0);
        dBodySetPosition(b, p[0], p[1], p[2]);
        let v = inputs.vec3(-3.0, 3.0);
        dBodySetLinearVel(b, v[0], v[1], v[2]);
        let w = inputs.vec3(-4.0, 4.0);
        dBodySetAngularVel(b, w[0], w[1], w[2]);
        let f = inputs.vec3(-50.0, 50.0);
        dBodyAddForce(b, f[0], f[1], f[2]);
        let t = inputs.vec3(-20.0, 20.0);
        dBodyAddTorque(b, t[0], t[1], t[2]);
        b
    }
}

/// A constraint row: QuickStep's layout (body 1's linear and angular
/// Jacobian, then body 2's), right-hand side, CFM, bounds and friction
/// index (relative to the joint's first row).
#[derive(Clone, Copy, Debug)]
struct Row {
    j: [f32; 12],
    c: f32,
    cfm: f32,
    lo: f32,
    hi: f32,
    findex: i32,
}

impl Row {
    fn new(cfm: f32) -> Self {
        Self {
            j: [0.0; 12],
            c: 0.0,
            cfm,
            lo: f32::NEG_INFINITY,
            hi: f32::INFINITY,
            findex: -1,
        }
    }
}

/// A joint's rows as its getInfo2 gives them: the bodies (indexes in the
/// stepper's list) and whether it records feedback.
struct Rows {
    b1: usize,
    b2: Option<usize>,
    rows: Vec<Row>,
    feedback: bool,
}

/// The run time's generator, from a state.
fn msvc_next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
    (*state >> 16) & 0x7FFF
}

/// 3: `dWorldQuickStep` of the bodies (the stepper's list order) and the
/// joints' rows, with round 2's rules and this round's (`shipped`) or
/// public 0.5.0's groupings for 3's items 1, 3, 5 and 8 and 7.4. The bodies'
/// velocities after the step, and the feedback of each joint that records
/// it (f1, t1, f2, t2).
fn quickstep(
    bodies: &mut [Body],
    joints: &[Rows],
    h: f32,
    p: &Params,
    rand: &mut u32,
    shipped: bool,
) -> Vec<[V3; 4]> {
    let frame = if shipped {
        world_frame
    } else {
        world_frame_public
    };
    let h1 = 1.0 / h;
    let nb = bodies.len();
    let mut iw = vec![[0.0f32; 12]; nb];
    let mut invw = vec![[0.0f32; 12]; nb];
    for (k, b) in bodies.iter_mut().enumerate() {
        iw[k] = frame(&b.r, &b.i, QUICKSTEP.inertia);
        invw[k] = frame(&b.r, &b.inv_i, QUICKSTEP.inv);
        let tmp = mul331(&iw[k], b.avel, QUICKSTEP.l);
        b.tacc = sub(b.tacc, cross(b.avel, tmp));
        for i in 0..3 {
            b.facc[i] += b.mass * p.gravity[i];
        }
    }
    let mut rows = Vec::new();
    let mut jb = Vec::new();
    for joint in joints {
        let ofs = rows.len() as i32;
        for r in &joint.rows {
            let mut r = *r;
            if r.findex >= 0 {
                r.findex += ofs;
            }
            rows.push(r);
            jb.push((joint.b1, joint.b2));
        }
    }
    let m = rows.len();
    let tmp1: Vec<[f32; 6]> = (0..nb)
        .map(|k| {
            let b = &bodies[k];
            let ang = mul331(&invw[k], b.tacc, picks(shipped, [Xyz, Xzy, Xyz]));
            std::array::from_fn(|i| {
                if i < 3 {
                    b.facc[i] * b.inv_mass + b.lvel[i] * h1
                } else {
                    ang[i - 3] + b.avel[i - 3] * h1
                }
            })
        })
        .collect();
    let mut b = vec![0.0f32; m];
    let mut cfm = vec![0.0f32; m];
    for i in 0..m {
        let (b1, b2) = jb[i];
        let mut sum = 0.0f32;
        for k in 0..6 {
            sum += rows[i].j[k] * tmp1[b1][k];
        }
        if let Some(b2) = b2 {
            for k in 0..6 {
                sum += rows[i].j[6 + k] * tmp1[b2][k];
            }
        }
        b[i] = rows[i].c * h1 - sum;
        cfm[i] = rows[i].cfm * h1;
    }

    // SOR_LCP, from multipliers of 0 (fresh joints).
    let mut lambda = vec![0.0f32; m];
    let mut imj = vec![[0.0f32; 12]; m];
    for i in 0..m {
        let (b1, b2) = jb[i];
        let ang = picks(shipped, [Yzx; 3]);
        let a = mul331(&invw[b1], [rows[i].j[3], rows[i].j[4], rows[i].j[5]], ang);
        for k in 0..3 {
            imj[i][k] = bodies[b1].inv_mass * rows[i].j[k];
            imj[i][3 + k] = a[k];
        }
        if let Some(b2) = b2 {
            let a = mul331(&invw[b2], [rows[i].j[9], rows[i].j[10], rows[i].j[11]], ang);
            for k in 0..3 {
                imj[i][6 + k] = bodies[b2].inv_mass * rows[i].j[6 + k];
                imj[i][9 + k] = a[k];
            }
        }
    }
    let mut fc = vec![[0.0f32; 6]; nb];
    let mut ad = vec![0.0f32; m];
    let mut jj: Vec<[f32; 12]> = rows.iter().map(|r| r.j).collect();
    for i in 0..m {
        let mut sum = 0.0f32;
        for k in 0..6 {
            sum += imj[i][k] * jj[i][k];
        }
        if jb[i].1.is_some() {
            for k in 6..12 {
                sum += imj[i][k] * jj[i][k];
            }
        }
        ad[i] = p.w / (sum + cfm[i]);
    }
    for i in 0..m {
        for k in 0..12 {
            jj[i][k] *= ad[i];
        }
        b[i] *= ad[i];
        ad[i] *= cfm[i];
    }
    let mut lo: Vec<f32> = rows.iter().map(|r| r.lo).collect();
    let mut hi: Vec<f32> = rows.iter().map(|r| r.hi).collect();
    let hicopy = hi.clone();
    let mut order: Vec<usize> = (0..m).filter(|&i| rows[i].findex < 0).collect();
    order.extend((0..m).filter(|&i| rows[i].findex >= 0));
    let s6 = |fc: &[f32; 6], j: &[f32]| {
        if shipped {
            fc[5] * j[5] + fc[4] * j[4] + fc[3] * j[3] + fc[2] * j[2] + fc[1] * j[1] + fc[0] * j[0]
        } else {
            fc[0] * j[0] + fc[1] * j[1] + fc[2] * j[2] + fc[3] * j[3] + fc[4] * j[4] + fc[5] * j[5]
        }
    };
    for iteration in 0..p.iterations {
        if iteration & 3 == 0 {
            for i in 1..m {
                let swapi = msvc_next(rand) as usize % (i + 1);
                order.swap(i, swapi);
            }
        }
        for &index in &order {
            let f = rows[index].findex;
            if f >= 0 {
                hi[index] = (hicopy[index] * lambda[f as usize]).abs();
                lo[index] = -hi[index];
            }
            let (b1, b2) = jb[index];
            let mut delta = b[index] - lambda[index] * ad[index];
            delta -= s6(&fc[b1], &jj[index][..6]);
            if let Some(b2) = b2 {
                delta -= s6(&fc[b2], &jj[index][6..]);
            }
            let new = lambda[index] + delta;
            if new >= lo[index] && new <= hi[index] {
                lambda[index] = new;
            } else if new >= lo[index] {
                delta = hi[index] - lambda[index];
                lambda[index] = hi[index];
            } else {
                delta = lo[index] - lambda[index];
                lambda[index] = lo[index];
            }
            for k in 0..6 {
                fc[b1][k] += delta * imj[index][k];
            }
            if let Some(b2) = b2 {
                for k in 0..6 {
                    fc[b2][k] += delta * imj[index][6 + k];
                }
            }
        }
    }
    for (k, body) in bodies.iter_mut().enumerate() {
        for i in 0..3 {
            body.lvel[i] += h * fc[k][i];
            body.avel[i] += h * fc[k][3 + i];
        }
    }
    let mut feedback = Vec::new();
    if joints.iter().any(|j| j.feedback) {
        let nb4 = if nb >= 4 { nb & !3 } else { 0 };
        let force: Vec<(V3, V3)> = (0..nb)
            .map(|k| {
                let lin = std::array::from_fn(|i| fc[k][i] * bodies[k].mass);
                let o = if !shipped || (k < nb4 && k % 4 == 0) {
                    SOURCE
                } else if k < nb4 && k % 4 >= 2 {
                    [Yzx; 3]
                } else {
                    [Xyz, Xzy, Xyz]
                };
                (lin, mul331(&iw[k], [fc[k][3], fc[k][4], fc[k][5]], o))
            })
            .collect();
        for joint in joints.iter().filter(|j| j.feedback) {
            let (f1, t1) = force[joint.b1];
            let (f2, t2) = joint.b2.map_or(([0.0; 3], [0.0; 3]), |b2| force[b2]);
            feedback.push([f1, t1, f2, t2]);
        }
    }
    for (k, body) in bodies.iter_mut().enumerate() {
        for i in 0..3 {
            body.lvel[i] += h * (body.inv_mass * body.facc[i]);
        }
        let t = body.tacc.map(|x| x * h);
        body.avel = add(body.avel, mul331(&invw[k], t, QUICKSTEP.update));
    }
    feedback
}

/// `dPlaneSpace` (public 0.5.0's): two unit vectors across n.
fn plane_space(n: V3) -> (V3, V3) {
    if n[2].abs() > std::f32::consts::FRAC_1_SQRT_2 {
        let a = n[1] * n[1] + n[2] * n[2];
        let k = 1.0 / a.sqrt();
        let p = [0.0, -n[2] * k, n[1] * k];
        (p, [a * k, -n[0] * p[2], n[0] * p[1]])
    } else {
        let a = n[0] * n[0] + n[1] * n[1];
        let k = 1.0 / a.sqrt();
        let p = [-n[1] * k, n[0] * k, 0.0];
        (p, [-n[2] * p[1], n[2] * p[0], a * k])
    }
}

const BOUNCE: c_int = 0x004;
const APPROX1: c_int = 0x3000;

/// A contact joint's rows (`contactGetInfo2`, the bounce's dots summed
/// (y + z) + x (the shipped library's) or in source order; 3 item 7 and
/// 4.1 rule 4), QuickStep's layout.
fn contact_rows(
    c: &DContact,
    bodies: &[Body],
    b1: usize,
    b2: Option<usize>,
    h1: f32,
    p: &Params,
    shipped: bool,
) -> Vec<Row> {
    let s = &c.surface;
    let n = three(c.geom.normal);
    let pos = three(c.geom.pos);
    let m = if s.mu > 0.0 { 3 } else { 1 };
    let mut rows = vec![Row::new(p.cfm); m];
    let c1 = sub(pos, bodies[b1].pos);
    let c2 = b2.map(|b2| sub(pos, bodies[b2].pos));
    let jac = |row: &mut Row, d: V3| {
        let a1 = cross(c1, d);
        row.j[..3].copy_from_slice(&d);
        row.j[3..6].copy_from_slice(&a1);
        if let Some(c2) = c2 {
            let a2 = cross(c2, d);
            for i in 0..3 {
                row.j[6 + i] = -d[i];
                row.j[9 + i] = -a2[i];
            }
        }
    };
    jac(&mut rows[0], n);
    let k = h1 * p.erp;
    let mut depth = c.geom.depth - p.layer;
    if depth < 0.0 {
        depth = 0.0;
    }
    rows[0].c = if k * depth > p.max_vel {
        p.max_vel
    } else {
        k * depth
    };
    if s.mode & BOUNCE != 0 {
        let o = pick(shipped, Yzx);
        let j = rows[0].j;
        let part = |b: &Body, l: usize| {
            dot(o, [j[l], j[l + 1], j[l + 2]], b.lvel)
                + dot(o, [j[l + 3], j[l + 4], j[l + 5]], b.avel)
        };
        let mut outgoing = part(&bodies[b1], 0);
        if let Some(b2) = b2 {
            outgoing += part(&bodies[b2], 6);
        }
        if s.bounce_vel >= 0.0 && -outgoing > s.bounce_vel {
            let newc = -s.bounce * outgoing;
            if newc > rows[0].c {
                rows[0].c = newc;
            }
        }
    }
    rows[0].lo = 0.0;
    rows[0].hi = f32::INFINITY;
    if m == 3 {
        let (t1, t2) = plane_space(n);
        for (r, t) in [(1, t1), (2, t2)] {
            jac(&mut rows[r], t);
            rows[r].lo = -s.mu;
            rows[r].hi = s.mu;
            if s.mode & APPROX1 != 0 {
                rows[r].findex = 0;
            }
        }
    }
    rows
}

/// A random contact for body `b1` (and `b2`): its position near the body,
/// a unit normal, a depth, friction on most, bounce on some.
fn random_contact(inputs: &mut Inputs, at: V3) -> DContact {
    let mut c = DContact::default();
    let n = normalise(inputs.vec3(-1.0, 1.0));
    c.geom.normal = [n[0], n[1], n[2], 0.0];
    let p = add(at, inputs.vec3(-0.5, 0.5));
    c.geom.pos = [p[0], p[1], p[2], 0.0];
    c.geom.depth = inputs.next(0.0, 0.05);
    let kind = inputs.next(0.0, 4.0) as i32;
    c.surface = Surface {
        mode: if kind >= 1 { APPROX1 } else { 0 } | if kind >= 2 { BOUNCE } else { 0 },
        mu: if kind == 0 {
            0.0
        } else {
            inputs.next(0.1, 1.0)
        },
        bounce: 0.4,
        bounce_vel: 0.1,
        ..Surface::default()
    };
    c
}

/// 3 and 8 items 7 – 11: QuickStep with contact joints between bodies and
/// with the world, every joint recording feedback.
#[test]
fn quickstep_in_the_shipped_groupings() {
    let _turn = lock();
    let mut inputs = Inputs::new();
    let (mut steps, mut differ) = (0, 0);
    for _ in 0..150 {
        // SAFETY: a world, its bodies and joints made, stepped and
        // destroyed here, under the lock.
        unsafe {
            let world = dWorldCreate();
            let g = [0.0, -9.81, 0.0];
            dWorldSetGravity(world, g[0], g[1], g[2]);
            let p = params(world, g);
            let nb = 3 + (inputs.next(0.0, 4.0) as usize);
            let made: Vec<*mut DxBody> = (0..nb).map(|_| random_body(world, &mut inputs)).collect();
            // The stepper's list: the last made first.
            let list: Vec<*mut DxBody> = made.iter().rev().copied().collect();
            let bodies: Vec<Body> = list
                .iter()
                .map(|&b| body_of(b, read::<3>(dBodyGetForce(b)), read::<3>(dBodyGetTorque(b))))
                .collect();
            // Each body's contact with the world, and contacts between
            // neighbours.
            let mut contacts = Vec::new();
            for k in 0..nb {
                contacts.push((k, None, random_contact(&mut inputs, bodies[k].pos)));
                if k + 1 < nb && k % 2 == 0 {
                    contacts.push((k, Some(k + 1), random_contact(&mut inputs, bodies[k].pos)));
                }
            }
            let mut feedback: Vec<Feedback> = vec![[[0.0; 4]; 4]; contacts.len()];
            let mut joints = Vec::new();
            for (n, (b1, b2, c)) in contacts.iter().enumerate() {
                let j = dJointCreateContact(std::ptr::null_mut(), std::ptr::null_mut(), c);
                dWorldAddJoint(world, j);
                dJointAttach(j, list[*b1], b2.map_or(std::ptr::null_mut(), |b2| list[b2]));
                dJointSetFeedback(j, &mut feedback[n]);
                joints.push(j);
            }
            let h = inputs.next(0.005, 0.03);
            // The world's joint list: the last added first.
            let model = |shipped: bool| {
                let mut bodies = bodies.clone();
                let rows: Vec<Rows> = contacts
                    .iter()
                    .rev()
                    .map(|(b1, b2, c)| Rows {
                        b1: *b1,
                        b2: *b2,
                        rows: contact_rows(c, &bodies, *b1, *b2, 1.0 / h, &p, shipped),
                        feedback: true,
                    })
                    .collect();
                let mut rand = msvc_rand::state();
                let fb = quickstep(&mut bodies, &rows, h, &p, &mut rand, shipped);
                (bodies, fb, rand)
            };
            let (want, want_fb, want_rand) = model(true);
            let (old, old_fb, _) = model(false);
            dWorldQuickStep(world, h);
            assert_eq!(msvc_rand::state(), want_rand);
            for (k, &b) in list.iter().enumerate() {
                let got = (
                    read::<3>(dBodyGetLinearVel(b)),
                    read::<3>(dBodyGetAngularVel(b)),
                );
                assert_eq!(
                    (bits(got.0), bits(got.1)),
                    (bits(want[k].lvel), bits(want[k].avel)),
                    "body {k}"
                );
            }
            for (n, fb) in want_fb.iter().enumerate() {
                let got = &feedback[contacts.len() - 1 - n];
                for i in 0..4 {
                    assert_eq!(bits(three(got[i])), bits(fb[i]), "joint {n}, vector {i}");
                }
            }
            steps += 1;
            let vel = |b: &[Body]| {
                b.iter()
                    .map(|b| (bits(b.lvel), bits(b.avel)))
                    .collect::<Vec<_>>()
            };
            let fbs = |f: &[[V3; 4]]| f.iter().map(|f| f.map(bits)).collect::<Vec<_>>();
            differ += usize::from(vel(&want) != vel(&old) || fbs(&want_fb) != fbs(&old_fb));
            for j in joints {
                dJointDestroy(j);
            }
            for b in made {
                dBodyDestroy(b);
            }
            dWorldDestroy(world);
        }
    }
    assert!(differ * 2 > steps, "{differ} of {steps}");
}

/// 4: `dWorldStep` of one body against one frictionless contact with the
/// world (a single row: the LCP takes one step to make its w zero), with
/// round 2's rules and this round's (`shipped`) or public 0.5.0's rules 1
/// – 3 and 7.4. The body's velocities and the joint's feedback (f1, t1).
fn exact_step_one_row(body: &mut Body, row: &Row, h: f32, p: &Params, shipped: bool) -> (V3, V3) {
    let frame = if shipped {
        world_frame
    } else {
        world_frame_public
    };
    let h1 = 1.0 / h;
    let iw = frame(&body.r, &body.i, EXACT.inertia);
    let invw = frame(&body.r, &body.inv_i, EXACT.inv);
    let tmp = mul331(&iw, body.avel, EXACT.l);
    body.tacc = sub(body.tacc, cross(body.avel, tmp));
    for i in 0..3 {
        body.facc[i] += body.mass * p.gravity[i];
    }
    // J and J M⁻¹ in the exact stepper's layout (4 linear, 4 angular).
    let j = [
        row.j[0], row.j[1], row.j[2], 0.0, row.j[3], row.j[4], row.j[5], 0.0,
    ];
    let mut jm = [0.0f32; 8];
    for k in 0..3 {
        jm[k] = j[k] * body.inv_mass;
        // Entry k of the angular part: (a0 M0k + a2 M2k) + a1 M1k.
        jm[4 + k] = sum3(
            pick(shipped, Xzy),
            j[4] * invw[k],
            j[5] * invw[4 + k],
            j[6] * invw[8 + k],
        );
    }
    let six = |a: &[f32; 8], b: &[f32; 8]| {
        let mut sum = a[0] * b[0];
        for k in [1, 2, 4, 5, 6] {
            sum += a[k] * b[k];
        }
        sum
    };
    let a = six(&jm, &j) + row.cfm * h1;
    let ang = mul331(&invw, body.tacc, picks(shipped, [Xzy; 3]));
    let mut t = [0.0f32; 8];
    for k in 0..3 {
        t[k] = body.facc[k] * body.inv_mass + body.lvel[k] * h1;
        t[4 + k] = ang[k] + body.avel[k] * h1;
    }
    let rhs = row.c * h1 - six(&j, &t);
    // dSolveLCP: w = 0 + 0 − b; at w < 0 the step that makes it zero.
    let w = 0.0 + 0.0 - rhs;
    let lambda = if w >= 0.0 {
        0.0
    } else if shipped {
        0.0 + (-1.0 / a) * w * 1.0
    } else {
        0.0 + -w / a * 1.0
    };
    let mut cforce = [0.0f32; 8];
    let mut fb = [0.0f32; 8];
    for k in [0, 1, 2, 4, 5, 6] {
        fb[k] = 0.0 + j[k] * lambda;
        cforce[k] += fb[k];
    }
    for k in 0..3 {
        cforce[k] += body.facc[k];
        cforce[4 + k] += body.tacc[k];
    }
    let cforce = cforce.map(|x| x * h);
    for k in 0..3 {
        body.lvel[k] += body.inv_mass * cforce[k];
    }
    body.avel = add(
        body.avel,
        mul331(&invw, [cforce[4], cforce[5], cforce[6]], EXACT.update),
    );
    ([fb[0], fb[1], fb[2]], [fb[4], fb[5], fb[6]])
}

/// 4 and 8 items 12 – 15: the exact stepper, one body on one frictionless
/// contact, the bounce on half of them.
#[test]
fn exact_step_in_the_shipped_groupings() {
    let _turn = lock();
    let mut inputs = Inputs::new();
    let (mut pushed, mut differ) = (0, 0);
    for i in 0..1500 {
        // SAFETY: a world, its body and joint made, stepped and destroyed
        // here, under the lock.
        unsafe {
            let world = dWorldCreate();
            let g = [0.0, -9.81, 0.0];
            dWorldSetGravity(world, g[0], g[1], g[2]);
            let p = params(world, g);
            let b = random_body(world, &mut inputs);
            let body = body_of(b, read::<3>(dBodyGetForce(b)), read::<3>(dBodyGetTorque(b)));
            let mut c = random_contact(&mut inputs, body.pos);
            c.surface.mu = 0.0;
            c.surface.mode = if i % 2 == 0 { BOUNCE } else { 0 };
            // Contacts that push: a normal against the velocity.
            if dot(Xyz, three(c.geom.normal), body.lvel) > 0.0 {
                c.geom.normal = c.geom.normal.map(|x| -x);
            }
            let mut feedback: Feedback = [[0.0; 4]; 4];
            let j = dJointCreateContact(std::ptr::null_mut(), std::ptr::null_mut(), &c);
            dWorldAddJoint(world, j);
            dJointAttach(j, b, std::ptr::null_mut());
            dJointSetFeedback(j, &mut feedback);
            let h = inputs.next(0.005, 0.03);
            let model = |shipped: bool| {
                let mut body = body;
                let row = contact_rows(&c, &[body], 0, None, 1.0 / h, &p, shipped)[0];
                let fb = exact_step_one_row(&mut body, &row, h, &p, shipped);
                (body, fb)
            };
            let (want, want_fb) = model(true);
            let (old, old_fb) = model(false);
            dWorldStep(world, h);
            let got = (
                read::<3>(dBodyGetLinearVel(b)),
                read::<3>(dBodyGetAngularVel(b)),
            );
            assert_eq!(
                (bits(got.0), bits(got.1)),
                (bits(want.lvel), bits(want.avel)),
                "{i}"
            );
            assert_eq!(
                (bits(three(feedback[0])), bits(three(feedback[1]))),
                (bits(want_fb.0), bits(want_fb.1))
            );
            pushed += usize::from(want_fb.0 != [0.0; 3]);
            differ += usize::from(
                (bits(want.lvel), bits(want.avel), bits(want_fb.0))
                    != (bits(old.lvel), bits(old.avel), bits(old_fb.0)),
            );
            dJointDestroy(j);
            dBodyDestroy(b);
            dWorldDestroy(world);
        }
    }
    assert!(pushed > 500, "{pushed}");
    assert!(differ > 0, "the public rules round the same");
}

unsafe extern "C" {
    fn dJointCreateBall(w: *mut DxWorld, g: *mut c_void) -> *mut DxJoint;
    fn dJointCreateHinge(w: *mut DxWorld, g: *mut c_void) -> *mut DxJoint;
    fn dJointCreateSlider(w: *mut DxWorld, g: *mut c_void) -> *mut DxJoint;
    fn dJointCreateUniversal(w: *mut DxWorld, g: *mut c_void) -> *mut DxJoint;
    fn dJointCreateHinge2(w: *mut DxWorld, g: *mut c_void) -> *mut DxJoint;
    fn dJointCreateFixed(w: *mut DxWorld, g: *mut c_void) -> *mut DxJoint;
    fn dJointSetBallAnchor(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointGetBallAnchor(j: *mut DxJoint, result: *mut Real);
    fn dJointGetBallAnchor2(j: *mut DxJoint, result: *mut Real);
    fn dJointSetHingeAnchor(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetHingeAxis(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetHingeParam(j: *mut DxJoint, parameter: c_int, value: Real);
    fn dJointGetHingeAxis(j: *mut DxJoint, result: *mut Real);
    fn dJointGetHingeAngle(j: *mut DxJoint) -> Real;
    fn dJointGetHingeAngleRate(j: *mut DxJoint) -> Real;
    fn dJointSetSliderAxis(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetSliderParam(j: *mut DxJoint, parameter: c_int, value: Real);
    fn dJointGetSliderPosition(j: *mut DxJoint) -> Real;
    fn dJointGetSliderPositionRate(j: *mut DxJoint) -> Real;
    fn dJointSetUniversalAnchor(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetUniversalAxis1(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetUniversalAxis2(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetHinge2Anchor(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetHinge2Axis1(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetHinge2Axis2(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetFixed(j: *mut DxJoint);
}

const LO_STOP: c_int = 0;
const HI_STOP: c_int = 1;
const BOUNCE_PARAM: c_int = 5;

/// A rotation of a stored vector into the world frame, R v, and back, Rᵀ
/// v: every row (y + z) + x (5.1 rule 1), or in source order.
pub(super) fn rot(r: &M3, v: V3, shipped: bool) -> V3 {
    mul331(r, v, picks(shipped, [Yzx; 3]))
}

pub(super) fn rot_back(r: &M3, v: V3, shipped: bool) -> V3 {
    mul_t(r, v, picks(shipped, [Yzx; 3]))
}

/// The relative rotation q1⁻¹ q2 (`dQMultiply1`), its scalar part summing
/// the products of components `order` in that order.
pub(super) fn relative(q1: [f32; 4], q2: [f32; 4], order: [usize; 4]) -> [f32; 4] {
    let [a, b, c, d] = order;
    [
        q1[a] * q2[a] + q1[b] * q2[b] + q1[c] * q2[c] + q1[d] * q2[d],
        q1[0] * q2[1] - q1[1] * q2[0] - q1[2] * q2[3] + q1[3] * q2[2],
        q1[0] * q2[2] - q1[2] * q2[0] - q1[3] * q2[1] + q1[1] * q2[3],
        q1[0] * q2[3] - q1[3] * q2[0] - q1[1] * q2[2] + q1[2] * q2[1],
    ]
}

/// qb qc⁻¹ (`dQMultiply2`), its scalar part summing the products of
/// components `order` in that order.
pub(super) fn times_inverse(qb: [f32; 4], qc: [f32; 4], order: [usize; 4]) -> [f32; 4] {
    let [a, b, c, d] = order;
    [
        qb[a] * qc[a] + qb[b] * qc[b] + qb[c] * qc[c] + qb[d] * qc[d],
        -qb[0] * qc[1] + qb[1] * qc[0] - qb[2] * qc[3] + qb[3] * qc[2],
        -qb[0] * qc[2] + qb[2] * qc[0] - qb[3] * qc[1] + qb[1] * qc[3],
        -qb[0] * qc[3] + qb[3] * qc[0] - qb[1] * qc[2] + qb[2] * qc[1],
    ]
}

pub(super) const INDEX_ORDER: [usize; 4] = [0, 1, 2, 3];

/// A joint's state as its setters store it.
#[derive(Clone, Copy, Debug, Default)]
struct Stored {
    anchor1: V3,
    anchor2: V3,
    axis1: V3,
    axis2: V3,
    qrel: [f32; 4],
    offset: V3,
    s0: f32,
    c0: f32,
    lostop: f32,
    histop: f32,
    bounce: f32,
}

/// `setAnchors`: the anchor in each body's frame.
fn set_anchors(a: V3, b1: &Body, b2: Option<&Body>, shipped: bool) -> (V3, V3) {
    let a1 = rot_back(&b1.r, sub(a, b1.pos), shipped);
    let a2 = b2.map_or(a, |b2| rot_back(&b2.r, sub(a, b2.pos), shipped));
    (a1, a2)
}

/// `setAxes`: the normalised axis in each body's frame.
fn set_axes(a: V3, b1: &Body, b2: Option<&Body>, shipped: bool) -> (V3, V3) {
    let q = normalise(a);
    let a1 = rot_back(&b1.r, q, shipped);
    let a2 = b2.map_or(a, |b2| rot_back(&b2.r, q, shipped));
    (a1, a2)
}

/// The rows' context: the bodies, the joint's two (indexes), 1/h and the
/// parameters.
struct Ctx<'a> {
    bodies: &'a [Body],
    b1: usize,
    b2: Option<usize>,
    h1: f32,
    p: &'a Params,
    shipped: bool,
}

impl Ctx<'_> {
    fn body1(&self) -> &Body {
        &self.bodies[self.b1]
    }

    fn body2(&self) -> Option<&Body> {
        self.b2.map(|b2| &self.bodies[b2])
    }

    fn k(&self) -> f32 {
        self.h1 * self.p.erp
    }
}

/// `setBall`: rows 0 – 2 (5.1 rule 2).
fn ball_rows(rows: &mut [Row], s: &Stored, x: &Ctx) {
    let a1 = rot(&x.body1().r, s.anchor1, x.shipped);
    for i in 0..3 {
        rows[i].j[i] = 1.0;
    }
    // dCROSSMAT (J1a,a1,s,-,+): the cross matrix of −a1.
    rows[0].j[3..6].copy_from_slice(&[0.0, a1[2], -a1[1]]);
    rows[1].j[3..6].copy_from_slice(&[-a1[2], 0.0, a1[0]]);
    rows[2].j[3..6].copy_from_slice(&[a1[1], -a1[0], 0.0]);
    let k = x.k();
    let p1 = x.body1().pos;
    match x.body2() {
        Some(b2) => {
            let a2 = rot(&b2.r, s.anchor2, x.shipped);
            for i in 0..3 {
                rows[i].j[6 + i] = -1.0;
            }
            // dCROSSMAT (J2a,a2,s,+,-): the cross matrix of a2.
            rows[0].j[9..12].copy_from_slice(&[0.0, -a2[2], a2[1]]);
            rows[1].j[9..12].copy_from_slice(&[a2[2], 0.0, -a2[0]]);
            rows[2].j[9..12].copy_from_slice(&[-a2[1], a2[0], 0.0]);
            for i in 0..3 {
                rows[i].c = k * (a2[i] + b2.pos[i] - a1[i] - p1[i]);
            }
        }
        None => {
            for i in 0..3 {
                rows[i].c = k * (s.anchor2[i] - a1[i] - p1[i]);
            }
        }
    }
}

/// `setFixedOrientation`: rows `start` – `start` + 2 (5.1 rule 5).
fn fixed_orientation_rows(rows: &mut [Row], qrel: [f32; 4], start: usize, x: &Ctx) {
    let q1 = x.body1().q;
    for i in 0..3 {
        rows[start + i].j[3 + i] = 1.0;
        if x.b2.is_some() {
            rows[start + i].j[9 + i] = -1.0;
        }
    }
    let mut qerr = match x.body2() {
        Some(b2) => times_inverse(relative(q1, b2.q, INDEX_ORDER), qrel, INDEX_ORDER),
        None => [
            q1[0] * qrel[0] - q1[1] * qrel[1] - q1[2] * qrel[2] - q1[3] * qrel[3],
            -q1[0] * qrel[1] - q1[1] * qrel[0] + q1[2] * qrel[3] - q1[3] * qrel[2],
            -q1[0] * qrel[2] - q1[2] * qrel[0] + q1[3] * qrel[1] - q1[1] * qrel[3],
            -q1[0] * qrel[3] - q1[3] * qrel[0] + q1[1] * qrel[2] - q1[2] * qrel[1],
        ],
    };
    if qerr[0] < 0.0 {
        for v in &mut qerr[1..] {
            *v = -*v;
        }
    }
    let e = rot(&x.body1().r, [qerr[1], qerr[2], qerr[3]], x.shipped);
    let k = x.k();
    for i in 0..3 {
        rows[start + i].c = if x.shipped {
            e[i] * k * 2.0
        } else {
            2.0 * k * e[i]
        };
    }
}

/// `getHingeAngle` (5.2 rule 12; with one body, physics-harness-4.md 3).
fn hinge_angle(s: &Stored, q1: [f32; 4], q2: Option<[f32; 4]>, shipped: bool) -> f32 {
    let qrel = match q2 {
        Some(q2) => {
            let qq = relative(q1, q2, INDEX_ORDER);
            let order = if shipped { [0, 3, 1, 2] } else { INDEX_ORDER };
            times_inverse(qq, s.qrel, order)
        }
        // dQMultiply3 (qrel,q1,qrel stored).
        None => {
            let q = s.qrel;
            [
                q1[0] * q[0] - q1[1] * q[1] - q1[2] * q[2] - q1[3] * q[3],
                -q1[0] * q[1] - q1[1] * q[0] + q1[2] * q[3] - q1[3] * q[2],
                -q1[0] * q[2] - q1[2] * q[0] + q1[3] * q[1] - q1[1] * q[3],
                -q1[0] * q[3] - q1[3] * q[0] + q1[1] * q[2] - q1[2] * q[1],
            ]
        }
    };
    let cost2 = qrel[0];
    let sint2 = if shipped {
        (qrel[3] * qrel[3] + qrel[2] * qrel[2] + qrel[1] * qrel[1]).sqrt()
    } else {
        (qrel[1] * qrel[1] + qrel[2] * qrel[2] + qrel[3] * qrel[3]).sqrt()
    };
    let v = [qrel[1], qrel[2], qrel[3]];
    let mut theta = if dot(pick(shipped, Yzx), v, s.axis1) >= 0.0 {
        2.0 * x87::atan2(sint2, cost2)
    } else {
        2.0 * x87::atan2(sint2, -cost2)
    };
    if theta > std::f32::consts::PI {
        theta -= 2.0 * std::f32::consts::PI;
    }
    -theta
}

/// A limit's state (`testRotationalLimit` and the slider's test): 0 none, 1
/// low, 2 high, and the error.
fn limit_of(value: f32, s: &Stored) -> (u8, f32) {
    if value <= s.lostop {
        (1, value - s.lostop)
    } else if value >= s.histop {
        (2, value - s.histop)
    } else {
        (0, 0.0)
    }
}

/// `addLimot`'s limit row (no motor): row `r` along `ax1` (5.1 rule 9).
fn limit_row(row: &mut Row, ax1: V3, limit: (u8, f32), rotational: bool, s: &Stored, x: &Ctx) {
    let (l1, l2) = if rotational { (3, 9) } else { (0, 6) };
    row.j[l1..l1 + 3].copy_from_slice(&ax1);
    if x.b2.is_some() {
        row.j[l2..l2 + 3].copy_from_slice(&ax1.map(|v| -v));
    }
    if !rotational && let Some(b2) = x.body2() {
        let c: V3 = std::array::from_fn(|i| 0.5 * (b2.pos[i] - x.body1().pos[i]));
        let ltd = cross(c, ax1);
        row.j[3..6].copy_from_slice(&ltd);
        row.j[9..12].copy_from_slice(&ltd);
    }
    let k = x.h1 * x.p.erp;
    row.c = -k * limit.1;
    row.cfm = x.p.cfm;
    if limit.0 == 1 {
        row.lo = 0.0;
        row.hi = f32::INFINITY;
    } else {
        row.lo = f32::NEG_INFINITY;
        row.hi = 0.0;
    }
    if s.bounce > 0.0 {
        let o = pick(x.shipped, Yzx);
        let v = |b: &Body| if rotational { b.avel } else { b.lvel };
        let mut vel = dot(o, v(x.body1()), ax1);
        if let Some(b2) = x.body2() {
            vel -= dot(o, v(b2), ax1);
        }
        let newc = -s.bounce * vel;
        if (limit.0 == 1 && vel < 0.0 && newc > row.c)
            || (limit.0 == 2 && vel > 0.0 && newc < row.c)
        {
            row.c = newc;
        }
    }
}

/// `dJointGetSliderPosition` (5.2 rule 14; with one body,
/// physics-harness-4.md 4 item 1).
fn slider_position(s: &Stored, b1: &Body, b2: Option<&Body>, shipped: bool) -> f32 {
    let ax1 = rot(&b1.r, s.axis1, shipped);
    let c: V3 = match b2 {
        Some(b2) => {
            let q = rot(&b2.r, s.offset, shipped);
            std::array::from_fn(|i| b1.pos[i] - q[i] - b2.pos[i])
        }
        None => sub(b1.pos, s.offset),
    };
    dot(pick(shipped, Yzx), c, ax1)
}

/// `dJointGetSliderPositionRate` (physics-harness-4.md 4 item 2).
fn slider_position_rate(s: &Stored, b1: &Body, b2: Option<&Body>, shipped: bool) -> f32 {
    let ax1 = rot(&b1.r, s.axis1, shipped);
    let o = pick(shipped, Yzx);
    let rate = dot(o, ax1, b1.lvel);
    b2.map_or(rate, |b2| rate - dot(o, ax1, b2.lvel))
}

/// normalise a given its squared length (physics-library.md 3.5).
pub(super) fn normalise_with(a: V3, l: f32) -> V3 {
    if l >= f32::from_bits(0x3748_0000) {
        let k = 1.0 / l.sqrt();
        a.map(|x| x * k)
    } else {
        normalise_slow(a, false)
    }
}

/// The joint kinds of the joints scenario.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Ball,
    Hinge,
    Slider,
    Universal,
    Hinge2,
    Fixed,
}

/// A joint's rows (getInfo1 and getInfo2) from its stored state.
fn joint_rows(kind: Kind, s: &Stored, x: &Ctx) -> Vec<Row> {
    let b1 = x.body1();
    let k = x.k();
    let shipped = x.shipped;
    let y = pick(shipped, Yzx);
    match kind {
        Kind::Ball => {
            let mut rows = vec![Row::new(x.p.cfm); 3];
            ball_rows(&mut rows, s, x);
            rows
        }
        Kind::Hinge => {
            let b2 = x.body2();
            let limit = limit_of(hinge_angle(s, b1.q, b2.map(|b| b.q), shipped), s);
            let mut rows = vec![Row::new(x.p.cfm); if limit.0 != 0 { 6 } else { 5 }];
            ball_rows(&mut rows, s, x);
            let ax1 = rot(&b1.r, s.axis1, shipped);
            let (p, q) = plane_space(ax1);
            rows[3].j[3..6].copy_from_slice(&p);
            rows[4].j[3..6].copy_from_slice(&q);
            if b2.is_some() {
                rows[3].j[9..12].copy_from_slice(&p.map(|v| -v));
                rows[4].j[9..12].copy_from_slice(&q.map(|v| -v));
            }
            let ax2 = b2.map_or(s.axis2, |b2| rot(&b2.r, s.axis2, shipped));
            let b = cross(ax1, ax2);
            rows[3].c = k * dot(y, b, p);
            rows[4].c = k * dot(y, b, q);
            if limit.0 != 0 {
                limit_row(&mut rows[5], ax1, limit, true, s, x);
            }
            rows
        }
        Kind::Slider => {
            let b2 = x.body2();
            let limit = limit_of(slider_position(s, b1, b2, shipped), s);
            let mut rows = vec![Row::new(x.p.cfm); if limit.0 != 0 { 6 } else { 5 }];
            fixed_orientation_rows(&mut rows, s.qrel, 0, x);
            let ax1 = rot(&b1.r, s.axis1, shipped);
            let (p, q) = plane_space(ax1);
            rows[3].j[..3].copy_from_slice(&p);
            rows[4].j[..3].copy_from_slice(&q);
            match b2 {
                Some(b2) => {
                    let mut c = sub(b2.pos, b1.pos);
                    let half = |v: V3| v.map(|e| 0.5 * e);
                    rows[3].j[9..12].copy_from_slice(&half(cross(c, p)));
                    rows[4].j[9..12].copy_from_slice(&half(cross(c, q)));
                    rows[3].j[6..9].copy_from_slice(&p.map(|v| -v));
                    rows[4].j[6..9].copy_from_slice(&q.map(|v| -v));
                    c = add(c, rot(&b2.r, s.offset, shipped));
                    let xz = pick(shipped, Xzy);
                    rows[3].c = k * dot(xz, p, c);
                    rows[4].c = k * dot(xz, q, c);
                }
                // physics-harness-4.md 4 item 3: p's dot in source order,
                // q's (y + z) + x.
                None => {
                    let o = sub(s.offset, b1.pos);
                    rows[3].c = k * dot(Xyz, p, o);
                    rows[4].c = k * dot(y, q, o);
                }
            }
            if limit.0 != 0 {
                limit_row(&mut rows[5], ax1, limit, false, s, x);
            }
            rows
        }
        Kind::Universal => {
            let b2 = x.body2().unwrap();
            let mut rows = vec![Row::new(x.p.cfm); 4];
            ball_rows(&mut rows, s, x);
            let ax1 = rot(&b1.r, s.axis1, shipped);
            let ax2 = rot(&b2.r, s.axis2, shipped);
            let d = dot(y, ax1, ax2);
            let t: V3 = std::array::from_fn(|i| ax2[i] - d * ax1[i]);
            let p = cross(ax1, t);
            let p = if shipped {
                normalise_with(p, dot(Yzx, p, p))
            } else {
                normalise(p)
            };
            rows[3].j[3..6].copy_from_slice(&p);
            rows[3].j[9..12].copy_from_slice(&p.map(|v| -v));
            rows[3].c = if shipped {
                -(x.p.erp * d * x.h1)
            } else {
                x.h1 * x.p.erp * -dot(Xyz, ax1, ax2)
            };
            rows
        }
        Kind::Hinge2 => {
            let b2 = x.body2().unwrap();
            let ax1 = rot(&b1.r, s.axis1, shipped);
            let ax2 = rot(&b2.r, s.axis2, shipped);
            let q = cross(ax1, ax2);
            let l = dot(y, q, q);
            let sin = l.sqrt();
            let cos = dot(pick(shipped, Xzy), ax1, ax2);
            let q = if shipped {
                normalise_with(q, l)
            } else {
                normalise(q)
            };
            let mut rows = vec![Row::new(x.p.cfm); 4];
            // setBall2 with the suspension axis ax1 and its ERP (the
            // library-wide one here).
            let (q1, q2) = plane_space(ax1);
            let a1 = rot(&b1.r, s.anchor1, shipped);
            let a2 = rot(&b2.r, s.anchor2, shipped);
            for (r, d) in [(0, ax1), (1, q1), (2, q2)] {
                rows[r].j[..3].copy_from_slice(&d);
                rows[r].j[3..6].copy_from_slice(&cross(a1, d));
                rows[r].j[6..9].copy_from_slice(&d.map(|v| -v));
                rows[r].j[9..12].copy_from_slice(&cross(a2, d).map(|v| -v));
            }
            let a1 = add(a1, b1.pos);
            let a2 = add(a2, b2.pos);
            let xz = pick(shipped, Xzy);
            rows[0].c = k * (dot(Xyz, ax1, a2) - dot(Xyz, ax1, a1));
            rows[1].c = k * (dot(Xyz, q1, a2) - dot(xz, q1, a1));
            rows[2].c = k * (dot(Xyz, q2, a2) - dot(xz, q2, a1));
            rows[3].j[3..6].copy_from_slice(&q);
            rows[3].j[9..12].copy_from_slice(&q.map(|v| -v));
            rows[3].c = k * (s.c0 * sin - s.s0 * cos);
            rows[0].cfm = x.p.cfm;
            rows
        }
        Kind::Fixed => {
            let mut rows = vec![Row::new(x.p.cfm); 6];
            fixed_orientation_rows(&mut rows, s.qrel, 3, x);
            for i in 0..3 {
                rows[i].j[i] = 1.0;
            }
            let ofs = rot(&b1.r, s.offset, shipped);
            match x.body2() {
                Some(b2) => {
                    // dCROSSMAT (J1a,ofs,s,+,-).
                    rows[0].j[3..6].copy_from_slice(&[0.0, -ofs[2], ofs[1]]);
                    rows[1].j[3..6].copy_from_slice(&[ofs[2], 0.0, -ofs[0]]);
                    rows[2].j[3..6].copy_from_slice(&[-ofs[1], ofs[0], 0.0]);
                    for i in 0..3 {
                        rows[i].j[6 + i] = -1.0;
                        rows[i].c = k * (b2.pos[i] - b1.pos[i] + ofs[i]);
                    }
                }
                None => {
                    for i in 0..3 {
                        rows[i].c = k * (s.offset[i] - b1.pos[i]);
                    }
                }
            }
            rows
        }
    }
}

/// A random joint between the bodies `b1` and `b2` (the world for none),
/// set up through the library's setters; with the state they store as the
/// models compute it (the shipped library's and public 0.5.0's).
fn random_joint(
    kind: Kind,
    inputs: &mut Inputs,
    (j1, j2): (*mut DxBody, *mut DxBody),
    (b1, b2): (&Body, Option<&Body>),
) -> (*mut DxJoint, Stored, Stored) {
    let a = add(b1.pos, inputs.vec3(-1.0, 1.0));
    let axis = inputs.vec3(-1.0, 1.0);
    let axis2 = inputs.vec3(-1.0, 1.0);
    let lo = inputs.next(-0.5, 0.0);
    let hi = inputs.next(0.0, 0.5);
    let bounce = inputs.next(0.0, 1.0);
    // SAFETY: a joint made without a world (added to one by the caller),
    // attached and set up.
    let j = unsafe {
        let j = match kind {
            Kind::Ball => dJointCreateBall(std::ptr::null_mut(), std::ptr::null_mut()),
            Kind::Hinge => dJointCreateHinge(std::ptr::null_mut(), std::ptr::null_mut()),
            Kind::Slider => dJointCreateSlider(std::ptr::null_mut(), std::ptr::null_mut()),
            Kind::Universal => dJointCreateUniversal(std::ptr::null_mut(), std::ptr::null_mut()),
            Kind::Hinge2 => dJointCreateHinge2(std::ptr::null_mut(), std::ptr::null_mut()),
            Kind::Fixed => dJointCreateFixed(std::ptr::null_mut(), std::ptr::null_mut()),
        };
        dJointAttach(j, j1, j2);
        match kind {
            Kind::Ball => dJointSetBallAnchor(j, a[0], a[1], a[2]),
            Kind::Hinge => {
                dJointSetHingeAnchor(j, a[0], a[1], a[2]);
                dJointSetHingeAxis(j, axis[0], axis[1], axis[2]);
                dJointSetHingeParam(j, LO_STOP, lo);
                dJointSetHingeParam(j, HI_STOP, hi);
                dJointSetHingeParam(j, BOUNCE_PARAM, bounce);
            }
            Kind::Slider => {
                dJointSetSliderAxis(j, axis[0], axis[1], axis[2]);
                dJointSetSliderParam(j, LO_STOP, lo);
                dJointSetSliderParam(j, HI_STOP, hi);
                dJointSetSliderParam(j, BOUNCE_PARAM, bounce);
            }
            Kind::Universal => {
                dJointSetUniversalAnchor(j, a[0], a[1], a[2]);
                dJointSetUniversalAxis1(j, axis[0], axis[1], axis[2]);
                dJointSetUniversalAxis2(j, axis2[0], axis2[1], axis2[2]);
            }
            Kind::Hinge2 => {
                dJointSetHinge2Anchor(j, a[0], a[1], a[2]);
                dJointSetHinge2Axis1(j, axis[0], axis[1], axis[2]);
                dJointSetHinge2Axis2(j, axis2[0], axis2[1], axis2[2]);
            }
            Kind::Fixed => dJointSetFixed(j),
        }
        j
    };
    let stored = |shipped: bool| {
        let mut s = Stored {
            lostop: lo,
            histop: hi,
            bounce,
            ..Stored::default()
        };
        if matches!(
            kind,
            Kind::Ball | Kind::Hinge | Kind::Universal | Kind::Hinge2
        ) {
            (s.anchor1, s.anchor2) = set_anchors(a, b1, b2, shipped);
        }
        match kind {
            Kind::Hinge => {
                (s.axis1, s.axis2) = set_axes(axis, b1, b2, shipped);
                let order = if shipped { [3, 0, 2, 1] } else { INDEX_ORDER };
                s.qrel = match b2 {
                    Some(b2) => relative(b1.q, b2.q, order),
                    None => [b1.q[0], -b1.q[1], -b1.q[2], -b1.q[3]],
                };
            }
            Kind::Slider => {
                s.axis1 = set_axes(axis, b1, b2, shipped).0;
                match b2 {
                    Some(b2) => {
                        let order = if shipped { [0, 3, 2, 1] } else { INDEX_ORDER };
                        s.qrel = relative(b1.q, b2.q, order);
                        s.offset = rot_back(&b2.r, sub(b1.pos, b2.pos), shipped);
                    }
                    None => {
                        s.qrel = [b1.q[0], -b1.q[1], -b1.q[2], -b1.q[3]];
                        s.offset = b1.pos;
                    }
                }
            }
            Kind::Universal => {
                s.axis1 = set_axes(axis, b1, b2, shipped).0;
                s.axis2 = set_axes(axis2, b1, b2, shipped).1;
            }
            Kind::Hinge2 => {
                s.axis1 = rot_back(&b1.r, normalise(axis), shipped);
                s.axis2 = rot_back(&b2.unwrap().r, normalise(axis2), shipped);
                // The axis setters' stored sine and cosine
                // (physics-harness-4.md 6 item 1).
                let ax1 = rot(&b1.r, s.axis1, shipped);
                let ax2 = rot(&b2.unwrap().r, s.axis2, shipped);
                let q = cross(ax1, ax2);
                s.s0 = dot(pick(shipped, Yzx), q, q).sqrt();
                s.c0 = dot(pick(shipped, Xzy), ax1, ax2);
            }
            Kind::Fixed => match b2 {
                Some(b2) => {
                    let order = if shipped { [3, 0, 2, 1] } else { INDEX_ORDER };
                    s.qrel = relative(b1.q, b2.q, order);
                    s.offset = rot_back(&b1.r, sub(b1.pos, b2.pos), shipped);
                }
                None => {
                    s.qrel = [b1.q[0], -b1.q[1], -b1.q[2], -b1.q[3]];
                    s.offset = b1.pos;
                }
            },
            Kind::Ball => {}
        }
        s
    };
    (j, stored(true), stored(false))
}

/// 5 and 8 items 17 – 25: QuickStep with each joint kind of the joints
/// scenario, the hinge and the slider at their limits with bounce, and
/// the joints' getters.
#[test]
fn joints_in_the_shipped_groupings() {
    let _turn = lock();
    let mut inputs = Inputs::new();
    let kinds = [
        Kind::Ball,
        Kind::Hinge,
        Kind::Slider,
        Kind::Universal,
        Kind::Hinge2,
        Kind::Fixed,
    ];
    let mut steps = [0; 9];
    let mut differ = [0; 9];
    let mut limits = 0;
    let mut getters_differ = 0;
    let mut rates_differ = 0;
    for i in 0..1800 {
        // The kind, and the fixed joint, the hinge and the slider held to
        // the world too (physics-harness-4.md 3 and 4).
        let slot = i % 9;
        let kind = match slot {
            6 => Kind::Fixed,
            7 => Kind::Hinge,
            8 => Kind::Slider,
            _ => kinds[slot],
        };
        let one_body = slot >= 6;
        // SAFETY: a world, its bodies and joint made, stepped and destroyed
        // here, under the lock.
        unsafe {
            let world = dWorldCreate();
            let g = [0.0, -9.81, 0.0];
            dWorldSetGravity(world, g[0], g[1], g[2]);
            let p = params(world, g);
            let made = [
                random_body(world, &mut inputs),
                random_body(world, &mut inputs),
            ];
            if one_body {
                dBodyDestroy(made[1]);
            }
            let n = if one_body { 1 } else { 2 };
            // The stepper's list: the last made first; the joint from
            // made[0] (body 1) to made[1].
            let list: Vec<*mut DxBody> = made[..n].iter().rev().copied().collect();
            let bodies: Vec<Body> = list
                .iter()
                .map(|&b| body_of(b, read::<3>(dBodyGetForce(b)), read::<3>(dBodyGetTorque(b))))
                .collect();
            let (i1, i2) = if one_body { (0, None) } else { (1, Some(0)) };
            let (j, shipped, public) = random_joint(
                kind,
                &mut inputs,
                (
                    made[0],
                    if one_body {
                        std::ptr::null_mut()
                    } else {
                        made[1]
                    },
                ),
                (&bodies[i1], i2.map(|i| &bodies[i])),
            );
            dWorldAddJoint(world, j);
            let h = inputs.next(0.005, 0.03);
            // The getters.
            {
                let b1 = &bodies[i1];
                let b2o = i2.map(|i| &bodies[i]);
                let mut got = [0.0; 4];
                match kind {
                    Kind::Ball => {
                        let b2 = b2o.unwrap();
                        dJointGetBallAnchor(j, got.as_mut_ptr());
                        let want = add(rot(&b1.r, shipped.anchor1, true), b1.pos);
                        assert_eq!(bits(three(got)), bits(want));
                        dJointGetBallAnchor2(j, got.as_mut_ptr());
                        let want2 = add(rot(&b2.r, shipped.anchor2, true), b2.pos);
                        assert_eq!(bits(three(got)), bits(want2));
                        let old = add(rot(&b1.r, public.anchor1, false), b1.pos);
                        getters_differ += usize::from(bits(want) != bits(old));
                    }
                    Kind::Hinge => {
                        dJointGetHingeAxis(j, got.as_mut_ptr());
                        assert_eq!(bits(three(got)), bits(rot(&b1.r, shipped.axis1, true)));
                        let q2 = b2o.map(|b| b.q);
                        let want = hinge_angle(&shipped, b1.q, q2, true);
                        assert_eq!(dJointGetHingeAngle(j).to_bits(), want.to_bits());
                        let old = hinge_angle(&public, b1.q, q2, false);
                        getters_differ += usize::from(want.to_bits() != old.to_bits());
                        let axis = rot(&b1.r, shipped.axis1, true);
                        let mut rate = dot(Yzx, axis, b1.avel);
                        if let Some(b2) = b2o {
                            rate -= dot(Yzx, axis, b2.avel);
                        }
                        assert_eq!(dJointGetHingeAngleRate(j).to_bits(), rate.to_bits());
                    }
                    Kind::Slider => {
                        let want = slider_position(&shipped, b1, b2o, true);
                        assert_eq!(dJointGetSliderPosition(j).to_bits(), want.to_bits());
                        let old = slider_position(&public, b1, b2o, false);
                        getters_differ += usize::from(want.to_bits() != old.to_bits());
                        let want = slider_position_rate(&shipped, b1, b2o, true);
                        assert_eq!(dJointGetSliderPositionRate(j).to_bits(), want.to_bits());
                        let old = slider_position_rate(&public, b1, b2o, false);
                        rates_differ += usize::from(want.to_bits() != old.to_bits());
                    }
                    _ => {}
                }
            }
            let model = |s: &Stored, shipped: bool| {
                let mut bodies = bodies.clone();
                let x = Ctx {
                    bodies: &bodies,
                    b1: i1,
                    b2: i2,
                    h1: 1.0 / h,
                    p: &p,
                    shipped,
                };
                let rows = vec![Rows {
                    b1: i1,
                    b2: i2,
                    rows: joint_rows(kind, s, &x),
                    feedback: false,
                }];
                let mut rand = msvc_rand::state();
                quickstep(&mut bodies, &rows, h, &p, &mut rand, shipped);
                (bodies, rows[0].rows.len(), rand)
            };
            let (want, m, rand_after) = model(&shipped, true);
            let (old, _, _) = model(&public, false);

            limits += usize::from(m == 6);
            dWorldQuickStep(world, h);
            assert_eq!(msvc_rand::state(), rand_after);
            for (k, &b) in list.iter().enumerate() {
                let got = (
                    read::<3>(dBodyGetLinearVel(b)),
                    read::<3>(dBodyGetAngularVel(b)),
                );
                assert_eq!(
                    (bits(got.0), bits(got.1)),
                    (bits(want[k].lvel), bits(want[k].avel)),
                    "{kind:?} {one_body} body {k}"
                );
            }
            steps[slot] += 1;
            let vel = |b: &[Body]| {
                b.iter()
                    .map(|b| (bits(b.lvel), bits(b.avel)))
                    .collect::<Vec<_>>()
            };
            differ[slot] += usize::from(vel(&want) != vel(&old));
            dJointDestroy(j);
            for &b in &list {
                dBodyDestroy(b);
            }
            dWorldDestroy(world);
        }
    }
    assert!(limits > 50, "{limits}");
    assert!(
        getters_differ > 0,
        "the getters' public rules round the same"
    );
    assert!(
        rates_differ > 0,
        "the slider rate's public rule rounds the same"
    );
    for slot in 0..9 {
        assert!(
            differ[slot] > 0,
            "{slot}: the public rules round the same ({} steps)",
            steps[slot]
        );
    }
}
