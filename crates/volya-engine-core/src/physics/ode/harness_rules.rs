// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The shipped library's arithmetic that the black-box comparison
//! established (docs/behavior/physics-harness.md 3 – 5, 7; section 2 is
//! `comparison/ode_load_time.rs`, a process of its own), through the
//! library's own functions: each rule's result recomputed here in plain
//! single precision from synthetic inputs, bit for bit, and the rule it
//! replaced shown to round differently on some of them.

use super::{DMass, DxBody, DxWorld, Real, STEPPING};
use std::ffi::{c_int, c_void};

#[repr(C)]
struct DxGeom {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ContactGeom {
    pos: [Real; 4],
    normal: [Real; 4],
    depth: Real,
    g1: *mut DxGeom,
    g2: *mut DxGeom,
}

unsafe extern "C" {
    fn dWorldCreate() -> *mut DxWorld;
    fn dWorldDestroy(w: *mut DxWorld);
    fn dWorldSetGravity(w: *mut DxWorld, x: Real, y: Real, z: Real);
    fn dWorldQuickStep(w: *mut DxWorld, stepsize: Real);
    fn dWorldGetAutoDisableLinearThreshold(w: *mut DxWorld) -> Real;
    fn dWorldSetAutoDisableLinearThreshold(w: *mut DxWorld, t: Real);
    fn dBodyCreate(w: *mut DxWorld) -> *mut DxBody;
    fn dBodyDestroy(b: *mut DxBody);
    fn dBodySetMass(b: *mut DxBody, mass: *const DMass);
    fn dBodySetLinearVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetAngularVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetQuaternion(b: *mut DxBody, q: *const Real);
    fn dBodyGetLinearVel(b: *mut DxBody) -> *const Real;
    fn dBodyGetQuaternion(b: *mut DxBody) -> *const Real;
    fn dBodyAddForce(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dMassSetSphereTotal(m: *mut DMass, total: Real, radius: Real);
    fn dMassAdjust(m: *mut DMass, newmass: Real);
    fn dxStepBody(b: *mut DxBody, h: Real);
    fn dNormalize3_slow(a: *mut Real);
    fn dRfromQ(r: *mut Real, q: *const Real);
    fn dCreateSphere(space: *mut c_void, radius: Real) -> *mut DxGeom;
    fn dCreateRay(space: *mut c_void, length: Real) -> *mut DxGeom;
    fn dGeomDestroy(g: *mut DxGeom);
    fn dGeomSetPosition(g: *mut DxGeom, x: Real, y: Real, z: Real);
    fn dGeomGetRotation(g: *mut DxGeom) -> *const Real;
    fn dGeomRaySet(g: *mut DxGeom, px: Real, py: Real, pz: Real, dx: Real, dy: Real, dz: Real);
    fn dCollide(
        o1: *mut DxGeom,
        o2: *mut DxGeom,
        flags: c_int,
        contact: *mut ContactGeom,
        skip: c_int,
    ) -> c_int;
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    STEPPING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Synthetic inputs: a fixed xorshift sequence, uniform in [lo, hi).
struct Inputs(u64);

impl Inputs {
    fn new() -> Self {
        Self(0x9E37_79B9_7F4A_7C15)
    }

    fn next(&mut self, lo: f32, hi: f32) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        let unit = (self.0 >> 40) as f32 / (1u64 << 24) as f32;
        lo + (hi - lo) * unit
    }

    fn vec3(&mut self, lo: f32, hi: f32) -> [f32; 3] {
        [self.next(lo, hi), self.next(lo, hi), self.next(lo, hi)]
    }
}

fn bits<const N: usize>(v: [f32; N]) -> [u32; N] {
    v.map(f32::to_bits)
}

fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// Section 3: `dNormalize3_slow`'s five steps.
fn normalise_slow(a: [f32; 3], divide: bool) -> [f32; 3] {
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
    if divide {
        // public 0.5.0's: divided by the magnitude
        v[o1] /= m[k];
        v[o2] /= m[k];
    } else {
        let inv = 1.0 / m[k];
        v[o1] *= inv;
        v[o2] *= inv;
    }
    v[k] = if a[k] < 0.0 { -1.0 } else { 1.0 };
    let s = 1.0 / (v[o1] * v[o1] + v[o2] * v[o2] + 1.0).sqrt();
    v.map(|x| x * s)
}

#[test]
fn normalise_slow_multiplies_by_the_reciprocal() {
    let mut inputs = Inputs::new();
    let mut differ = 0;
    for _ in 0..2000 {
        let a = inputs.vec3(-1e3, 1e3);
        let mut ours = [a[0], a[1], a[2], 0.0];
        // SAFETY: a 3-vector (with its padding word).
        unsafe { dNormalize3_slow(ours.as_mut_ptr()) };
        let ours = [ours[0], ours[1], ours[2]];
        assert_eq!(bits(ours), bits(normalise_slow(a, false)), "{a:?}");
        differ += usize::from(bits(ours) != bits(normalise_slow(a, true)));
    }
    assert!(differ > 0, "division rounds the same on every input");
}

#[test]
fn quickstep_rounds_inverse_mass_times_force_first() {
    let _turn = lock();
    let mut inputs = Inputs::new();
    let mut differ = 0;
    // SAFETY: a world and a body made, stepped and destroyed here.
    unsafe {
        let w = dWorldCreate();
        let b = dBodyCreate(w);
        for _ in 0..1000 {
            let total = inputs.next(0.5, 50.0);
            let g = inputs.vec3(-20.0, 20.0);
            let force = inputs.vec3(-100.0, 100.0);
            let v0 = inputs.vec3(-5.0, 5.0);
            let h = inputs.next(0.005, 0.05);
            let mut m = DMass::default();
            dMassSetSphereTotal(&mut m, total, 0.5);
            dBodySetMass(b, &m);
            dWorldSetGravity(w, g[0], g[1], g[2]);
            dBodySetLinearVel(b, v0[0], v0[1], v0[2]);
            dBodySetAngularVel(b, 0.0, 0.0, 0.0);
            dBodyAddForce(b, force[0], force[1], force[2]);
            dWorldQuickStep(w, h);
            let v = std::slice::from_raw_parts(dBodyGetLinearVel(b), 3);
            let inv = 1.0 / total;
            let f = [0, 1, 2].map(|j| force[j] + total * g[j]);
            let rule = [0, 1, 2].map(|j| v0[j] + h * (inv * f[j]));
            let old = [0, 1, 2].map(|j| v0[j] + h * inv * f[j]);
            assert_eq!(bits([v[0], v[1], v[2]]), bits(rule));
            differ += usize::from(bits(rule) != bits(old));
        }
        dBodyDestroy(b);
        dWorldDestroy(w);
    }
    assert!(
        differ > 0,
        "(h × invMass) × f rounds the same on every input"
    );
}

/// Section 4.2: the infinitesimal quaternion update, `dq = ½ (w ⊗ q)`, `q
/// += h × dq`, renormalised (the length summed in index order).
fn step_quaternion(w: [f32; 3], q: [f32; 4], h: f32, source_order: bool) -> [f32; 4] {
    let third = if source_order {
        -w[0] * q[3] + w[1] * q[0] + w[2] * q[1]
    } else {
        -w[0] * q[3] + w[2] * q[1] + w[1] * q[0]
    };
    let dq = [
        0.5 * (-w[0] * q[1] - w[1] * q[2] - w[2] * q[3]),
        0.5 * (w[0] * q[0] + w[1] * q[3] - w[2] * q[2]),
        0.5 * third,
        0.5 * (w[0] * q[2] - w[1] * q[1] + w[2] * q[0]),
    ];
    let q = [0, 1, 2, 3].map(|j| q[j] + h * dq[j]);
    let l = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
    let s = 1.0 / l.sqrt();
    q.map(|x| x * s)
}

#[test]
fn step_body_sums_the_third_derivative_out_of_order() {
    let mut inputs = Inputs::new();
    let mut differ = 0;
    // SAFETY: a body outside any world, made, stepped and destroyed here.
    unsafe {
        let b = dBodyCreate(std::ptr::null_mut());
        for _ in 0..2000 {
            let w = inputs.vec3(-10.0, 10.0);
            let q0 = [
                inputs.next(-1.0, 1.0),
                inputs.next(-1.0, 1.0),
                inputs.next(-1.0, 1.0),
                inputs.next(-1.0, 1.0),
            ];
            let h = inputs.next(0.005, 0.05);
            dBodySetQuaternion(b, q0.as_ptr());
            let q = std::slice::from_raw_parts(dBodyGetQuaternion(b), 4);
            let q = [q[0], q[1], q[2], q[3]];
            dBodySetAngularVel(b, w[0], w[1], w[2]);
            dxStepBody(b, h);
            let after = std::slice::from_raw_parts(dBodyGetQuaternion(b), 4);
            let rule = step_quaternion(w, q, h, false);
            assert_eq!(bits([after[0], after[1], after[2], after[3]]), bits(rule));
            differ += usize::from(bits(rule) != bits(step_quaternion(w, q, h, true)));
        }
        dBodyDestroy(b);
    }
    assert!(
        differ > 0,
        "the source order rounds the same on every input"
    );
}

/// The library's normalisation of a 3-vector (physics-library.md 3.5).
fn normalise(a: [f32; 3]) -> [f32; 3] {
    let l = a[0] * a[0] + a[1] * a[1] + a[2] * a[2];
    if l >= f(0x3748_0000) {
        let s = 1.0 / l.sqrt();
        a.map(|x| x * s)
    } else {
        normalise_slow(a, false)
    }
}

/// Section 5.2: a ray from `p` along the unit `d` against a sphere at `c`
/// of radius `r`: (depth, point, normal), or None.
fn ray_sphere(
    p: [f32; 3],
    d: [f32; 3],
    length: f32,
    c: [f32; 3],
    r: f32,
    source_order: bool,
) -> Option<(f32, [f32; 3], [f32; 3])> {
    let q = [0, 1, 2].map(|j| p[j] - c[j]);
    let (b, qq) = if source_order {
        (
            q[0] * d[0] + q[1] * d[1] + q[2] * d[2],
            q[0] * q[0] + q[1] * q[1] + q[2] * q[2],
        )
    } else {
        (
            q[1] * d[1] + q[2] * d[2] + q[0] * d[0],
            q[1] * q[1] + q[2] * q[2] + q[0] * q[0],
        )
    };
    let cc = qq - r * r;
    let k = b * b - cc;
    if k < 0.0 {
        return None;
    }
    let k = k.sqrt();
    let mut alpha = -b - k;
    if alpha < 0.0 {
        alpha = -b + k;
        if alpha < 0.0 {
            return None;
        }
    }
    if alpha > length {
        return None;
    }
    let pos = [0, 1, 2].map(|j| p[j] + alpha * d[j]);
    let sign = if cc < 0.0 { -1.0 } else { 1.0 };
    let n = normalise([0, 1, 2].map(|j| sign * (pos[j] - c[j])));
    Some((alpha, pos, n))
}

#[test]
fn ray_sphere_sums_the_first_component_last() {
    let mut inputs = Inputs::new();
    let (mut hits, mut differ) = (0, 0);
    // SAFETY: geometries made, collided and destroyed here.
    unsafe {
        let ray = dCreateRay(std::ptr::null_mut(), 100.0);
        for i in 0..2000 {
            let c = inputs.vec3(-20.0, 20.0);
            let r = inputs.next(0.5, 5.0);
            let sphere = {
                let s = dCreateSphere(std::ptr::null_mut(), r);
                dGeomSetPosition(s, c[0], c[1], c[2]);
                s
            };
            // Every fourth ray starts inside the sphere.
            let p = if i % 4 == 0 {
                [0, 1, 2].map(|j| c[j] + inputs.next(-0.5, 0.5) * r)
            } else {
                inputs.vec3(-20.0, 20.0)
            };
            // Aimed at a point near the sphere.
            let at = [0, 1, 2].map(|j| c[j] + inputs.next(-1.2, 1.2) * r);
            dGeomRaySet(
                ray,
                p[0],
                p[1],
                p[2],
                at[0] - p[0],
                at[1] - p[1],
                at[2] - p[2],
            );
            let m = std::slice::from_raw_parts(dGeomGetRotation(ray), 12);
            let d = [m[2], m[6], m[10]];
            let mut con = ContactGeom {
                pos: [0.0; 4],
                normal: [0.0; 4],
                depth: 0.0,
                g1: std::ptr::null_mut(),
                g2: std::ptr::null_mut(),
            };
            let n = dCollide(ray, sphere, 1, &mut con, size_of::<ContactGeom>() as c_int);
            let rule = ray_sphere(p, d, 100.0, c, r, false);
            match rule {
                None => assert_eq!(n, 0, "{p:?} {d:?}"),
                Some((depth, pos, normal)) => {
                    assert_eq!(n, 1, "{p:?} {d:?}");
                    assert_eq!(con.depth.to_bits(), depth.to_bits());
                    assert_eq!(bits([con.pos[0], con.pos[1], con.pos[2]]), bits(pos));
                    assert_eq!(
                        bits([con.normal[0], con.normal[1], con.normal[2]]),
                        bits(normal)
                    );
                    hits += 1;
                }
            }
            let old = ray_sphere(p, d, 100.0, c, r, true);
            differ += usize::from(
                rule.map(|(a, p, n)| (a.to_bits(), bits(p), bits(n)))
                    != old.map(|(a, p, n)| (a.to_bits(), bits(p), bits(n))),
            );
            dGeomDestroy(sphere);
        }
        dGeomDestroy(ray);
    }
    assert!(hits > 500, "{hits} hits");
    assert!(
        differ > 0,
        "the source order rounds the same on every input"
    );
}

/// Section 7, through `dRfromQ`: R(0,1) = 2 (q1 q2 − q0 q3).
fn r01(q: [f32; 4]) -> u32 {
    let mut r = [0.0; 12];
    // SAFETY: a rotation and a quaternion.
    unsafe { dRfromQ(r.as_mut_ptr(), q.as_ptr()) };
    r[1].to_bits()
}

#[test]
fn tiny_products_and_differences_flush_to_zero_of_their_sign() {
    // 2^-70 · 2^-70 = 2^-140, a denormal: 0, then 2 · (0 − 0).
    let tiny = f(0x1C80_0000);
    assert_eq!(r01([0.0, tiny, tiny, 0.0]), 0x0000_0000);
    // −0 − 0 = −0: the flushed product keeps its sign.
    assert_eq!(r01([0.0, -tiny, tiny, 0.0]), 0x8000_0000);
    // (2^-126 + 2^-149) − 2^-126 = 2^-149, a denormal difference.
    assert_eq!(r01([1.0, 1.0, f(0x0080_0001), f(0x0080_0000)]), 0x0000_0000);
    // A normal result is untouched: 2 · (1 · 2^-126 − 0).
    assert_eq!(r01([0.0, 1.0, f(0x0080_0000), 0.0]), 0x0100_0000);
}

#[test]
fn tininess_is_detected_after_rounding() {
    // (1 − 2^-24) · 2^-126: rounds to 2^-126 among denormals, but at 24
    // bits with an unbounded exponent stays below it: flushed (as Rosetta
    // 2's x86 SSE does under flush-to-zero; ARM's flush-to-zero, which
    // detects tininess before rounding, flushes it too).
    assert_eq!(r01([0.0, f(0x3F7F_FFFF), f(0x0080_0000), 0.0]), 0x0000_0000);
    // (1 − 2^-23) · (2^-126 + 2^-149) = 2^-126 (1 − 2^-46): rounds to 2^-126
    // at 24 bits too, so not tiny: kept (detected before rounding it would
    // be flushed).
    assert_eq!(r01([0.0, f(0x3F7F_FFFE), f(0x0080_0001), 0.0]), 0x0100_0000);

    // The same for a quotient, through dMassAdjust's newmass / mass.
    let adjust = |mass: f32, newmass: f32| {
        let mut m = DMass {
            mass,
            ..DMass::default()
        };
        m.i[0] = 1.0;
        // SAFETY: a mass.
        unsafe { dMassAdjust(&mut m, newmass) };
        m.i[0].to_bits()
    };
    // (1 − 2^-24) / 2^126, then 1 × that.
    assert_eq!(adjust(f(0x7E80_0000), f(0x3F7F_FFFF)), 0x0000_0000);
    assert_eq!(adjust(f(0x7E80_0000), 2.0), 0x0100_0000);
}

#[test]
fn denormal_operands_read_as_zero() {
    // 2^-127 · 2^100: 2^-27 unless the denormal reads as 0.
    assert_eq!(r01([0.0, f(0x0040_0000), f(0x7180_0000), 0.0]), 0x0000_0000);
    // A quotient: 2^-20 / 2^-138 would be 2^118; by 0 it is infinite.
    let mut m = DMass {
        mass: f(0x0000_0800),
        ..DMass::default()
    };
    m.i[0] = 1.0;
    // SAFETY: a mass.
    unsafe { dMassAdjust(&mut m, f(0x3580_0000)) };
    assert_eq!(m.i[0], f32::INFINITY);
    // A comparison: a vector of one negative denormal is a zero vector to
    // dNormalize3_slow, (1, 0, 0), not (−1, 0, 0).
    let mut a = [f(0x8000_0800), 0.0, 0.0, 0.0];
    // SAFETY: a 3-vector.
    unsafe { dNormalize3_slow(a.as_mut_ptr()) };
    assert_eq!(bits([a[0], a[1], a[2]]), bits([1.0, 0.0, 0.0]));
}

#[test]
fn world_parameters_flush_too() {
    let _turn = lock();
    // SAFETY: the library-wide parameters, under the lock.
    unsafe {
        let w = dWorldCreate();
        // 2^-64 squared is 2^-128, a denormal: stored as 0.
        dWorldSetAutoDisableLinearThreshold(w, f(0x1F80_0000));
        assert_eq!(dWorldGetAutoDisableLinearThreshold(w).to_bits(), 0);
        dWorldSetAutoDisableLinearThreshold(w, 0.001);
        dWorldDestroy(w);
    }
}
