// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The shipped library's arithmetic that the comparison's second round
//! established (docs/behavior/physics-harness-2.md 5), as `harness_rules`
//! does for the first: each rule's result recomputed here in plain single
//! precision from synthetic inputs, bit for bit, through the library's own
//! functions, and the rule it replaced shown to round differently on some
//! of them. The x87 sine's single rounding is `x87`'s, tested there.

use super::harness_rules::{ContactGeom, DxGeom, Inputs, bits, lock, normalise_slow};
use super::x87;
use super::{DMass, DxBody, DxWorld, Real};
use std::ffi::{c_int, c_void};

unsafe extern "C" {
    fn dWorldCreate() -> *mut DxWorld;
    fn dWorldDestroy(w: *mut DxWorld);
    fn dWorldSetGravity(w: *mut DxWorld, x: Real, y: Real, z: Real);
    fn dWorldQuickStep(w: *mut DxWorld, stepsize: Real);
    fn dWorldStep(w: *mut DxWorld, stepsize: Real);
    fn dBodyCreate(w: *mut DxWorld) -> *mut DxBody;
    fn dBodyDestroy(b: *mut DxBody);
    fn dBodySetMass(b: *mut DxBody, mass: *const DMass);
    fn dBodyGetMass(b: *mut DxBody, mass: *mut DMass);
    fn dBodySetLinearVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetAngularVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetAngularVel(b: *mut DxBody) -> *const Real;
    fn dBodySetQuaternion(b: *mut DxBody, q: *const Real);
    fn dBodyGetQuaternion(b: *mut DxBody) -> *const Real;
    fn dBodySetRotation(b: *mut DxBody, r: *const Real);
    fn dBodyGetRotation(b: *mut DxBody) -> *const Real;
    fn dBodySetForce(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetTorque(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyAddTorque(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetTorque(b: *mut DxBody) -> *const Real;
    fn dBodyAddRelForceAtRelPos(
        b: *mut DxBody,
        fx: Real,
        fy: Real,
        fz: Real,
        px: Real,
        py: Real,
        pz: Real,
    );
    fn dBodySetFiniteRotationMode(b: *mut DxBody, mode: c_int);
    fn dMassSetSphereTotal(m: *mut DMass, total: Real, radius: Real);
    fn dMassSetBoxTotal(m: *mut DMass, total: Real, lx: Real, ly: Real, lz: Real);
    fn dxStepBody(b: *mut DxBody, h: Real);
    fn dNormalize3_slow(a: *mut Real);
    fn dNormalize4(a: *mut Real);
    fn dQfromR(q: *mut Real, r: *const Real);
    fn dRfromQ(r: *mut Real, q: *const Real);
    fn dRFromAxisAndAngle(r: *mut Real, ax: Real, ay: Real, az: Real, angle: Real);
    fn dCreateBox(space: *mut c_void, lx: Real, ly: Real, lz: Real) -> *mut DxGeom;
    fn dCreatePlane(space: *mut c_void, a: Real, b: Real, c: Real, d: Real) -> *mut DxGeom;
    fn dGeomPlaneGetParams(g: *mut DxGeom, result: *mut Real);
    fn dGeomDestroy(g: *mut DxGeom);
    fn dGeomSetPosition(g: *mut DxGeom, x: Real, y: Real, z: Real);
    fn dGeomSetRotation(g: *mut DxGeom, r: *const Real);
    fn dGeomGetRotation(g: *mut DxGeom) -> *const Real;
    fn dCollide(
        o1: *mut DxGeom,
        o2: *mut DxGeom,
        flags: c_int,
        contact: *mut ContactGeom,
        skip: c_int,
    ) -> c_int;
}

/// A 3-term sum's grouping (physics-harness-2.md's conventions).
#[derive(Clone, Copy)]
enum Sum {
    /// Source order, `(x + y) + z`.
    Xyz,
    /// `(x + z) + y`.
    Xzy,
    /// `(y + z) + x`.
    Yzx,
}

use Sum::{Xyz, Xzy, Yzx};

const SOURCE: [Sum; 3] = [Xyz; 3];

fn sum3(o: Sum, x: f32, y: f32, z: f32) -> f32 {
    match o {
        Xyz => x + y + z,
        Xzy => x + z + y,
        Yzx => y + z + x,
    }
}

/// A 3×3 matrix as ODE stores it, rows four apart.
type M3 = [f32; 12];

fn read<const N: usize>(p: *const Real) -> [f32; N] {
    // SAFETY: the library's arrays of at least N values.
    let s = unsafe { std::slice::from_raw_parts(p, N) };
    std::array::from_fn(|i| s[i])
}

fn unit_quaternion(inputs: &mut Inputs) -> [f32; 4] {
    let mut q = [0.0; 4].map(|_: f32| inputs.next(-1.0, 1.0));
    // SAFETY: a quaternion.
    unsafe { dNormalize4(q.as_mut_ptr()) };
    q
}

/// `dRfromQ`.
fn rotation(q: [f32; 4]) -> M3 {
    let (qq1, qq2, qq3) = (2.0 * q[1] * q[1], 2.0 * q[2] * q[2], 2.0 * q[3] * q[3]);
    [
        1.0 - qq2 - qq3,
        2.0 * (q[1] * q[2] - q[0] * q[3]),
        2.0 * (q[1] * q[3] + q[0] * q[2]),
        0.0,
        2.0 * (q[1] * q[2] + q[0] * q[3]),
        1.0 - qq1 - qq3,
        2.0 * (q[2] * q[3] - q[0] * q[1]),
        0.0,
        2.0 * (q[1] * q[3] - q[0] * q[2]),
        2.0 * (q[2] * q[3] + q[0] * q[1]),
        1.0 - qq1 - qq2,
        0.0,
    ]
}

fn mul331(b: &M3, c: [f32; 3], o: [Sum; 3]) -> [f32; 3] {
    std::array::from_fn(|i| {
        sum3(
            o[i],
            b[i * 4] * c[0],
            b[i * 4 + 1] * c[1],
            b[i * 4 + 2] * c[2],
        )
    })
}

/// The library's normalisation of a quaternion: the length summed in index
/// order, times its reciprocal square root.
fn normalise4(q: [f32; 4]) -> [f32; 4] {
    let l = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
    let s = 1.0 / l.sqrt();
    q.map(|x| x * s)
}

/// 2.1 and 5.1: the sphere's and the box's diagonal inertia.
#[test]
fn mass_inertias_in_the_shipped_order() {
    let mut inputs = Inputs::new();
    let (mut sphere_differ, mut box_differ) = (0, 0);
    for _ in 0..2000 {
        let t = inputs.next(0.1, 200.0);
        let r = inputs.next(0.05, 3.0);
        let mut m = DMass::default();
        // SAFETY: a mass.
        unsafe { dMassSetSphereTotal(&mut m, t, r) };
        let rule = t * r * r * 0.4;
        let old = 0.4 * t * r * r;
        assert_eq!(bits([m.i[0], m.i[5], m.i[10]]), bits([rule; 3]), "{t} {r}");
        assert_eq!(m.mass, t);
        sphere_differ += usize::from(rule.to_bits() != old.to_bits());

        let s = inputs.vec3(0.05, 5.0);
        // SAFETY: a mass.
        unsafe { dMassSetBoxTotal(&mut m, t, s[0], s[1], s[2]) };
        let k = 1.0f32 / 12.0;
        let sq = |a: f32, b: f32| a * a + b * b;
        let sums = [sq(s[1], s[2]), sq(s[0], s[2]), sq(s[0], s[1])];
        let rule = sums.map(|x| x * t * k);
        let old = sums.map(|x| t / 12.0 * x);
        assert_eq!(bits([m.i[0], m.i[5], m.i[10]]), bits(rule), "{t} {s:?}");
        box_differ += usize::from(bits(rule) != bits(old));
    }
    assert!(
        sphere_differ > 0 && box_differ > 0,
        "{sphere_differ} {box_differ}"
    );
}

/// 2.2 and 5.2: `dRFromAxisAndAngle`'s vector part scaled by the unrounded
/// sine times the axis's reciprocal length, rounded once.
#[test]
fn rotation_from_axis_and_angle_rounds_the_sine_once() {
    let mut inputs = Inputs::new();
    let mut differ = 0;
    let expect = |a: [f32; 3], angle: f32, once: bool| {
        let l = a[0] * a[0] + a[1] * a[1] + a[2] * a[2];
        let r = 1.0 / l.sqrt();
        let half = angle * 0.5;
        let k = if once {
            x87::sin_times(half, r)
        } else {
            x87::sin(half) * r
        };
        rotation([x87::cos(half), a[0] * k, a[1] * k, a[2] * k])
    };
    for _ in 0..4000 {
        let a = inputs.vec3(-3.0, 3.0);
        let angle = inputs.next(-7.0, 7.0);
        let mut m = [0.0; 12];
        // SAFETY: a rotation.
        unsafe { dRFromAxisAndAngle(m.as_mut_ptr(), a[0], a[1], a[2], angle) };
        let rule = expect(a, angle, true);
        assert_eq!(bits(m), bits(rule), "{a:?} {angle}");
        differ += usize::from(bits(rule) != bits(expect(a, angle, false)));
    }
    assert!(differ > 0, "rounding the sine first rounds the same");
}

/// 2.3 and 5.3: `dBodySetRotation`'s trace summed `(R11 + R22) + R00`; for a
/// negative trace, `dQfromR`'s quaternion. The normalisation's double
/// rounding is shown only without flushing, which our build never runs
/// (the review of 2.3).
#[test]
fn set_rotation_sums_the_trace_last_two_first() {
    let mut inputs = Inputs::new();
    let (mut positive, mut differ) = (0, 0);
    let expect = |m: &M3, tr: f32| {
        let s = (tr + 1.0).sqrt();
        let q0 = 0.5 * s;
        let s = 0.5 * (1.0 / s);
        normalise4([q0, (m[9] - m[6]) * s, (m[2] - m[8]) * s, (m[4] - m[1]) * s])
    };
    // SAFETY: a body outside any world, made, set and destroyed here.
    unsafe {
        let b = dBodyCreate(std::ptr::null_mut());
        for i in 0..4000 {
            // Every other rotation near the identity: traces near 3.
            let mut q = unit_quaternion(&mut inputs);
            if i % 2 == 0 {
                q[0] = 4.0;
                dNormalize4(q.as_mut_ptr());
            }
            let m = rotation(q);
            dBodySetRotation(b, m.as_ptr());
            let got = read::<4>(dBodyGetQuaternion(b));
            let tr = m[5] + m[10] + m[0];
            if tr >= 0.0 {
                let rule = expect(&m, tr);
                assert_eq!(bits(got), bits(rule), "{m:?}");
                positive += 1;
                differ += usize::from(bits(rule) != bits(expect(&m, m[0] + m[5] + m[10])));
            } else {
                let mut want = [0.0; 4];
                dQfromR(want.as_mut_ptr(), m.as_ptr());
                dNormalize4(want.as_mut_ptr());
                assert_eq!(bits(got), bits(want), "{m:?}");
            }
        }
        dBodyDestroy(b);
    }
    assert!(positive > 1000, "{positive}");
    assert!(differ > 0, "the source order rounds the same");
}

/// 2.4's orders for one stepper: the world-frame inverse inertia, the
/// world-frame inertia, L = J w, and the update.
struct Orders {
    inv: [Sum; 3],
    inertia: [Sum; 3],
    l: [Sum; 3],
    update: [Sum; 3],
}

const QUICKSTEP: Orders = Orders {
    inv: [Yzx, Yzx, Yzx],
    inertia: [Xzy, Yzx, Yzx],
    l: [Xzy, Xzy, Yzx],
    update: [Xyz, Xzy, Xyz],
};

const EXACT: Orders = Orders {
    inv: [Xzy, Yzx, Yzx],
    inertia: [Yzx, Yzx, Yzx],
    l: SOURCE,
    update: [Yzx, Yzx, Yzx],
};

const PUBLIC: Orders = Orders {
    inv: SOURCE,
    inertia: SOURCE,
    l: SOURCE,
    update: SOURCE,
};

/// R D Rᵀ: D Rᵀ in source order, then row i summed over k in order `o[i]`.
fn world_frame(r: &M3, d: &M3, o: [Sum; 3]) -> M3 {
    let mut tmp = [0.0; 12];
    for k in 0..3 {
        for j in 0..3 {
            tmp[k * 4 + j] =
                d[k * 4] * r[j * 4] + d[k * 4 + 1] * r[j * 4 + 1] + d[k * 4 + 2] * r[j * 4 + 2];
        }
    }
    let mut a = [0.0; 12];
    for i in 0..3 {
        for j in 0..3 {
            a[i * 4 + j] = sum3(
                o[i],
                r[i * 4] * tmp[j],
                r[i * 4 + 1] * tmp[4 + j],
                r[i * 4 + 2] * tmp[8 + j],
            );
        }
    }
    a
}

/// A lone body's angular velocity after one step (2.4 steps 1 to 4).
fn angular_step(o: &Orders, r: &M3, inertia: &M3, w: [f32; 3], t: [f32; 3], h: f32) -> [f32; 3] {
    let mut inv = [0.0; 12];
    for k in 0..3 {
        let c = inertia[k * 5].sqrt();
        inv[k * 5] = 1.0 / c / c;
    }
    let m = world_frame(r, &inv, o.inv);
    let j = world_frame(r, inertia, o.inertia);
    let l = mul331(&j, w, o.l);
    let t = [
        t[0] - (w[1] * l[2] - w[2] * l[1]),
        t[1] - (w[2] * l[0] - w[0] * l[2]),
        t[2] - (w[0] * l[1] - w[1] * l[0]),
    ];
    let dw = mul331(&m, t.map(|x| h * x), o.update);
    std::array::from_fn(|i| w[i] + dw[i])
}

fn angular_update(step: unsafe extern "C" fn(*mut DxWorld, Real), o: &Orders) {
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
            dBodySetMass(b, &mass);
            dBodyGetMass(b, &mut mass);
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
            let rule = angular_step(o, &r, &mass.i, w, t, h);
            assert_eq!(bits(got), bits(rule), "{r:?} {w:?} {t:?} {h}");
            differ += usize::from(bits(rule) != bits(angular_step(&PUBLIC, &r, &mass.i, w, t, h)));
        }
        dBodyDestroy(b);
        dWorldDestroy(world);
    }
    assert!(differ > 0, "the source orders round the same");
}

/// 2.4 and 5.4, `dWorldQuickStep`.
#[test]
fn quickstep_angular_update_in_the_shipped_orders() {
    angular_update(dWorldQuickStep, &QUICKSTEP);
}

/// 2.4 and 5.4, `dWorldStep`.
#[test]
fn exact_step_angular_update_in_the_shipped_orders() {
    angular_update(dWorldStep, &EXACT);
}

/// 2.5 and 5.5: `dBodyAddRelForceAtRelPos` rotates the point with each row
/// summed `(y + z) + x`.
#[test]
fn relative_point_rotated_last_two_first() {
    let mut inputs = Inputs::new();
    let mut differ = 0;
    let expect = |r: &M3, t0: [f32; 3], f: [f32; 3], p: [f32; 3], o: [Sum; 3]| {
        let f = mul331(r, f, SOURCE);
        let p = mul331(r, p, o);
        [
            t0[0] + (p[1] * f[2] - p[2] * f[1]),
            t0[1] + (p[2] * f[0] - p[0] * f[2]),
            t0[2] + (p[0] * f[1] - p[1] * f[0]),
        ]
    };
    // SAFETY: a body outside any world, made and destroyed here.
    unsafe {
        let b = dBodyCreate(std::ptr::null_mut());
        for _ in 0..2000 {
            let q = unit_quaternion(&mut inputs);
            dBodySetQuaternion(b, q.as_ptr());
            let r = read::<12>(dBodyGetRotation(b));
            let t0 = inputs.vec3(-10.0, 10.0);
            let f = inputs.vec3(-100.0, 100.0);
            let p = inputs.vec3(-2.0, 2.0);
            dBodySetForce(b, 0.0, 0.0, 0.0);
            dBodySetTorque(b, 0.0, 0.0, 0.0);
            dBodyAddTorque(b, t0[0], t0[1], t0[2]);
            dBodyAddRelForceAtRelPos(b, f[0], f[1], f[2], p[0], p[1], p[2]);
            let got = read::<3>(dBodyGetTorque(b));
            let rule = expect(&r, t0, f, p, [Yzx; 3]);
            assert_eq!(bits(got), bits(rule), "{r:?} {f:?} {p:?}");
            differ += usize::from(bits(rule) != bits(expect(&r, t0, f, p, SOURCE)));
        }
        dBodyDestroy(b);
    }
    assert!(differ > 0, "the source order rounds the same");
}

/// 2.6: the finite rotation without an axis, `(cos θ, w k)` with k = (sin θ
/// / θ) (h / 2), the sine unrounded, times the quaternion, normalised.
fn finite_rotation(w: [f32; 3], p: [f32; 4], h: f32, shipped: bool) -> [f32; 4] {
    let wlen = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
    let h = h * 0.5;
    let th = wlen * h;
    let k = if shipped {
        x87::sin_over(th, th)
    } else {
        x87::sin(th) / th
    } * h;
    let q = [x87::cos(th), w[0] * k, w[1] * k, w[2] * k];
    let q2 = if shipped {
        [
            q[0] * p[0] - q[1] * p[1] - q[2] * p[2] - q[3] * p[3],
            q[0] * p[1] + q[2] * p[3] + q[1] * p[0] - q[3] * p[2],
            q[0] * p[2] + q[3] * p[1] + q[2] * p[0] - q[1] * p[3],
            q[0] * p[3] + q[1] * p[2] + q[3] * p[0] - q[2] * p[1],
        ]
    } else {
        [
            q[0] * p[0] - q[1] * p[1] - q[2] * p[2] - q[3] * p[3],
            q[0] * p[1] + q[1] * p[0] + q[2] * p[3] - q[3] * p[2],
            q[0] * p[2] + q[2] * p[0] + q[3] * p[1] - q[1] * p[3],
            q[0] * p[3] + q[3] * p[0] + q[1] * p[2] - q[2] * p[1],
        ]
    };
    normalise4(q2)
}

/// 2.6 and 5.6.
#[test]
fn finite_rotation_without_an_axis() {
    let mut inputs = Inputs::new();
    let (mut stepped, mut differ) = (0, 0);
    // SAFETY: a body outside any world, made, stepped and destroyed here.
    unsafe {
        let b = dBodyCreate(std::ptr::null_mut());
        dBodySetFiniteRotationMode(b, 1);
        for _ in 0..2000 {
            let q = unit_quaternion(&mut inputs);
            dBodySetQuaternion(b, q.as_ptr());
            let p = read::<4>(dBodyGetQuaternion(b));
            let w = inputs.vec3(-10.0, 10.0);
            let h = inputs.next(0.005, 0.05);
            // Small angles keep the series (not established).
            let wlen = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
            if wlen * (h * 0.5) < 1e-3 {
                continue;
            }
            dBodySetAngularVel(b, w[0], w[1], w[2]);
            dxStepBody(b, h);
            let got = read::<4>(dBodyGetQuaternion(b));
            let rule = finite_rotation(w, p, h, true);
            assert_eq!(bits(got), bits(rule), "{w:?} {p:?} {h}");
            assert_eq!(bits(read::<12>(dBodyGetRotation(b))), bits(rotation(rule)));
            stepped += 1;
            differ += usize::from(bits(rule) != bits(finite_rotation(w, p, h, false)));
        }
        dBodyDestroy(b);
    }
    assert!(stepped > 1500, "{stepped}");
    assert!(differ > 0, "the public rule rounds the same");
}

/// 2.7 and 5.7: box against plane, the deepest contact's depth with the
/// half-extents summed `(a1 + a2) + a0`.
#[test]
fn box_plane_depth_last_two_first() {
    let mut inputs = Inputs::new();
    let (mut hits, mut differ) = (0, 0);
    let expect = |pl: [f32; 4], r: &M3, side: [f32; 3], pos: [f32; 3], shipped: bool| {
        let b: [f32; 3] = std::array::from_fn(|i| {
            let q = pl[0] * r[i] + pl[1] * r[4 + i] + pl[2] * r[8 + i];
            (side[i] * q).abs()
        });
        let half = if shipped {
            b[1] + b[2] + b[0]
        } else {
            b[0] + b[1] + b[2]
        };
        pl[3] + 0.5 * half - (pl[0] * pos[0] + pl[1] * pos[1] + pl[2] * pos[2])
    };
    // SAFETY: geometries made, collided and destroyed here.
    unsafe {
        for i in 0..4000 {
            let side = inputs.vec3(0.1, 4.0);
            let bx = dCreateBox(std::ptr::null_mut(), side[0], side[1], side[2]);
            // Every other box at the origin (the comparison's case).
            let pos = if i % 2 == 0 {
                [0.0; 3]
            } else {
                inputs.vec3(-2.0, 2.0)
            };
            dGeomSetPosition(bx, pos[0], pos[1], pos[2]);
            let a = inputs.vec3(-1.0, 1.0);
            let mut m = [0.0; 12];
            dRFromAxisAndAngle(m.as_mut_ptr(), a[0], a[1], a[2], inputs.next(-4.0, 4.0));
            dGeomSetRotation(bx, m.as_ptr());
            let r = read::<12>(dGeomGetRotation(bx));
            // Normals along a coordinate axis (the comparison's), or any.
            let n = if i % 4 < 2 {
                let mut n = [0.0; 3];
                n[i % 3] = if i % 8 < 4 { 1.0 } else { -1.0 };
                n
            } else {
                inputs.vec3(-1.0, 1.0)
            };
            let plane = dCreatePlane(
                std::ptr::null_mut(),
                n[0],
                n[1],
                n[2],
                inputs.next(-1.5, 1.5),
            );
            let mut pl = [0.0; 4];
            dGeomPlaneGetParams(plane, pl.as_mut_ptr());
            let mut con = ContactGeom {
                pos: [0.0; 4],
                normal: [0.0; 4],
                depth: 0.0,
                g1: std::ptr::null_mut(),
                g2: std::ptr::null_mut(),
            };
            let got = dCollide(bx, plane, 1, &mut con, size_of::<ContactGeom>() as c_int);
            let rule = expect(pl, &r, side, pos, true);
            if rule < 0.0 {
                assert_eq!(got, 0, "{rule}");
            } else {
                assert_eq!(got, 1, "{rule}");
                assert_eq!(con.depth.to_bits(), rule.to_bits(), "{side:?} {r:?} {pl:?}");
                hits += 1;
                differ += usize::from(rule.to_bits() != expect(pl, &r, side, pos, false).to_bits());
            }
            dGeomDestroy(plane);
            dGeomDestroy(bx);
        }
    }
    assert!(hits > 1000, "{hits}");
    assert!(differ > 0, "the source order rounds the same");
}

/// 2.8 and 5.8: among equal largest magnitudes `dNormalize3_slow` takes the
/// lowest index (round 1's five steps, `harness_rules`).
#[test]
fn normalise_slow_ties_go_to_the_lowest_index() {
    let mut inputs = Inputs::new();
    for i in 0..3000 {
        let a = inputs.next(-50.0, 50.0);
        let b = inputs.next(-0.9, 0.9) * a;
        let s = if i % 2 == 0 { 1.0 } else { -1.0 };
        let v = match i % 4 {
            0 => [a, s * a, b],
            1 => [a, b, s * a],
            2 => [b, a, s * a],
            _ => [a, s * a, -a],
        };
        let mut got = [v[0], v[1], v[2], 0.0];
        // SAFETY: a 3-vector (with its padding word).
        unsafe { dNormalize3_slow(got.as_mut_ptr()) };
        assert_eq!(
            bits([got[0], got[1], got[2]]),
            bits(normalise_slow(v, false)),
            "{v:?}"
        );
    }
}

/// `dRfromQ` itself, which the rules above recompute (round 1's [Q2]).
#[test]
fn rotation_from_quaternion_as_recomputed() {
    let mut inputs = Inputs::new();
    for _ in 0..1000 {
        let q = unit_quaternion(&mut inputs);
        let mut m = [0.0; 12];
        // SAFETY: a rotation and a quaternion.
        unsafe { dRfromQ(m.as_mut_ptr(), q.as_ptr()) };
        assert_eq!(bits(m), bits(rotation(q)));
    }
}
