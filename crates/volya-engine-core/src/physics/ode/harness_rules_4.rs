// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The shipped library's arithmetic that the comparison's fourth round
//! established (docs/behavior/physics-harness-4.md 9) for the universal
//! joint, hinge-2, the angular motor and `dRFrom2Axes`, as `harness_rules_3`
//! does for the third: each rule's result recomputed here in plain single
//! precision from synthetic inputs at general rotations, bit for bit,
//! through the library's own functions, and the rule it replaced (our
//! build's before, public 0.5.0's) shown to round differently on some of
//! them. The one-body hinge and slider rules (items 1 – 4) are in
//! `harness_rules_3`'s joints test, whose models have the rows.

use super::harness_rules::{Inputs, bits, lock, normalise};
use super::harness_rules_2::{M3, Yzx, unit_quaternion};
use super::harness_rules_3::{
    Body, INDEX_ORDER, V3, body_of, cross, dot, normalise_with, pick, random_body, relative, rot,
    rot_back, times_inverse,
};
use super::{DxBody, DxJoint, DxWorld, Real, x87};
use std::ffi::{c_int, c_void};

unsafe extern "C" {
    fn dWorldCreate() -> *mut DxWorld;
    fn dWorldDestroy(w: *mut DxWorld);
    fn dWorldQuickStep(w: *mut DxWorld, stepsize: Real);
    fn dWorldAddJoint(w: *mut DxWorld, j: *mut DxJoint);
    fn dBodyDestroy(b: *mut DxBody);
    fn dBodySetQuaternion(b: *mut DxBody, q: *const Real);
    fn dJointAttach(j: *mut DxJoint, b1: *mut DxBody, b2: *mut DxBody);
    fn dJointDestroy(j: *mut DxJoint);
    fn dJointCreateUniversal(w: *mut DxWorld, g: *mut c_void) -> *mut DxJoint;
    fn dJointSetUniversalAnchor(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetUniversalAxis1(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetUniversalAxis2(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointGetUniversalAngle1(j: *mut DxJoint) -> Real;
    fn dJointGetUniversalAngle2(j: *mut DxJoint) -> Real;
    fn dJointGetUniversalAngle1Rate(j: *mut DxJoint) -> Real;
    fn dJointGetUniversalAngle2Rate(j: *mut DxJoint) -> Real;
    fn dJointCreateHinge2(w: *mut DxWorld, g: *mut c_void) -> *mut DxJoint;
    fn dJointSetHinge2Anchor(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetHinge2Axis1(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetHinge2Axis2(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointGetHinge2Angle1(j: *mut DxJoint) -> Real;
    fn dJointGetHinge2Angle1Rate(j: *mut DxJoint) -> Real;
    fn dJointGetHinge2Angle2Rate(j: *mut DxJoint) -> Real;
    fn dJointCreateAMotor(w: *mut DxWorld, g: *mut c_void) -> *mut DxJoint;
    fn dJointSetAMotorMode(j: *mut DxJoint, mode: c_int);
    fn dJointSetAMotorNumAxes(j: *mut DxJoint, num: c_int);
    fn dJointSetAMotorAxis(j: *mut DxJoint, anum: c_int, rel: c_int, x: Real, y: Real, z: Real);
    fn dJointGetAMotorAxis(j: *mut DxJoint, anum: c_int, result: *mut Real);
    fn dJointGetAMotorAngle(j: *mut DxJoint, anum: c_int) -> Real;
    fn dRFrom2Axes(r: *mut Real, ax: Real, ay: Real, az: Real, bx: Real, by: Real, bz: Real);
    fn dQfromR(q: *mut Real, r: *const Real);
}

const AMOTOR_USER: c_int = 0;
const AMOTOR_EULER: c_int = 1;

/// Two random bodies of a world (the second destroyed for one body), read
/// back: (made, bodies).
fn two_bodies(
    world: *mut DxWorld,
    inputs: &mut Inputs,
    one_body: bool,
) -> ([*mut DxBody; 2], Body, Option<Body>) {
    let made = [random_body(world, inputs), random_body(world, inputs)];
    let b1 = body_of(made[0], [0.0; 3], [0.0; 3]);
    if one_body {
        // SAFETY: a body of the world.
        unsafe { dBodyDestroy(made[1]) };
        return ([made[0], std::ptr::null_mut()], b1, None);
    }
    (made, b1, Some(body_of(made[1], [0.0; 3], [0.0; 3])))
}

/// The bodies turned to new random orientations, read back.
fn turned(made: [*mut DxBody; 2], inputs: &mut Inputs) -> (Body, Option<Body>) {
    let mut turn = |b: *mut DxBody| {
        let q = unit_quaternion(inputs);
        // SAFETY: a body of the world.
        unsafe { dBodySetQuaternion(b, q.as_ptr()) };
        body_of(b, [0.0; 3], [0.0; 3])
    };
    let b1 = turn(made[0]);
    let b2 = (!made[1].is_null()).then(|| turn(made[1]));
    (b1, b2)
}

/// `dRFrom2Axes` (physics-harness-4.md 5 item 2): both lengths the square
/// root of the squares summed in index order (the x87's at 24 bits is the
/// correctly rounded float square root), the rest as public 0.5.0.
fn r_from_2_axes(a: V3, b: V3) -> M3 {
    let l = 1.0 / (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let a = a.map(|x| x * l);
    let k = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let b: V3 = std::array::from_fn(|i| b[i] - k * a[i]);
    let l = 1.0 / (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    let b = b.map(|x| x * l);
    let mut r = [0.0; 12];
    for i in 0..3 {
        r[i * 4] = a[i];
        r[i * 4 + 1] = b[i];
    }
    r[2] = -b[1] * a[2] + a[1] * b[2];
    r[6] = -b[2] * a[0] + a[2] * b[0];
    r[10] = -b[0] * a[1] + a[0] * b[1];
    r
}

/// The cross's rotation from two axes as a quaternion: `dRFrom2Axes` then
/// the library's `dQfromR` (round 3 7.5).
fn cross_rotation(a: V3, b: V3) -> [f32; 4] {
    let r = r_from_2_axes(a, b);
    let mut q = [0.0; 4];
    // SAFETY: a rotation and room for a quaternion.
    unsafe { dQfromR(q.as_mut_ptr(), r.as_ptr()) };
    q
}

#[test]
fn rotation_from_two_axes_in_the_shipped_groupings() {
    let _turn = lock();
    let mut inputs = Inputs::new();
    for _ in 0..2000 {
        let a = inputs.vec3(-2.0, 2.0);
        let b = inputs.vec3(-2.0, 2.0);
        let mut got = [0.0; 12];
        // SAFETY: room for a rotation.
        unsafe { dRFrom2Axes(got.as_mut_ptr(), a[0], a[1], a[2], b[0], b[1], b[2]) };
        assert_eq!(bits(got), bits(r_from_2_axes(a, b)), "{a:?} {b:?}");
    }
}

/// The universal joint's stored state (5 item 1): the axes in their bodies'
/// frames and the two relative rotations.
struct Universal {
    axis1: V3,
    axis2: V3,
    qrel1: [f32; 4],
    qrel2: [f32; 4],
}

/// The axes in world coordinates (`getUniversalAxes`, round 3's rows).
fn universal_axes(u: &Universal, b1: &Body, b2: Option<&Body>) -> (V3, V3) {
    let ax1 = rot(&b1.r, u.axis1, true);
    let ax2 = b2.map_or(u.axis2, |b2| rot(&b2.r, u.axis2, true));
    (ax1, ax2)
}

fn universal_stored(axis: V3, axis2: V3, b1: &Body, b2: Option<&Body>, shipped: bool) -> Universal {
    let mut u = Universal {
        axis1: rot_back(&b1.r, normalise(axis), true),
        // setAxes: without a body, the axis as given.
        axis2: b2.map_or(axis2, |b2| rot_back(&b2.r, normalise(axis2), true)),
        qrel1: [0.0; 4],
        qrel2: [0.0; 4],
    };
    let (ax1, ax2) = universal_axes(&u, b1, b2);
    let order1 = if shipped { [0, 1, 3, 2] } else { INDEX_ORDER };
    let order2 = if shipped { [1, 3, 2, 0] } else { INDEX_ORDER };
    u.qrel1 = relative(b1.q, cross_rotation(ax1, ax2), order1);
    let q2 = cross_rotation(ax2, ax1);
    u.qrel2 = b2.map_or(q2, |b2| relative(b2.q, q2, order2));
    u
}

/// The angle from a relative rotation (5 item 3): the half-angle sine's
/// squares in index order, the sign dot (y + z) + x, round 3's tail.
fn universal_tail(qrel: [f32; 4], axis: V3, shipped: bool) -> f32 {
    let sint2 = ((qrel[1] * qrel[1] + qrel[2] * qrel[2]) + qrel[3] * qrel[3]).sqrt();
    let v = [qrel[1], qrel[2], qrel[3]];
    let mut theta = if dot(pick(shipped, Yzx), v, axis) >= 0.0 {
        2.0 * x87::atan2(sint2, qrel[0])
    } else {
        2.0 * x87::atan2(sint2, -qrel[0])
    };
    if theta > std::f32::consts::PI {
        theta -= 2.0 * std::f32::consts::PI;
    }
    -theta
}

fn universal_angles(u: &Universal, b1: &Body, b2: Option<&Body>, shipped: bool) -> [f32; 2] {
    let (ax1, ax2) = universal_axes(u, b1, b2);
    let qq = relative(b1.q, cross_rotation(ax1, ax2), INDEX_ORDER);
    let angle1 = universal_tail(times_inverse(qq, u.qrel1, INDEX_ORDER), u.axis1, shipped);
    let qc = cross_rotation(ax2, ax1);
    let qq = b2.map_or(qc, |b2| relative(b2.q, qc, INDEX_ORDER));
    let angle2 = -universal_tail(times_inverse(qq, u.qrel2, INDEX_ORDER), u.axis2, shipped);
    [angle1, angle2]
}

/// An angle rate: the world axis dotted with each body's angular velocity,
/// body 2's subtracted (5 item 4, 6 item 4).
fn rate(axis: V3, b1: &Body, b2: Option<&Body>, shipped: bool) -> f32 {
    let o = pick(shipped, Yzx);
    let r = dot(o, axis, b1.avel);
    b2.map_or(r, |b2| r - dot(o, axis, b2.avel))
}

/// 9 items 5 – 7: the universal joint's setters' relative rotations, its
/// angles and its rates, with two bodies and with one.
#[test]
fn universal_joint_in_the_shipped_groupings() {
    let _turn = lock();
    let mut inputs = Inputs::new();
    let (mut angles_differ, mut rates_differ) = (0, 0);
    for i in 0..1200 {
        let one_body = i % 3 == 2;
        // SAFETY: a world, its bodies and a joint made, read and destroyed
        // here, under the lock.
        unsafe {
            let world = dWorldCreate();
            let (made, b1, b2) = two_bodies(world, &mut inputs, one_body);
            let a = inputs.vec3(-3.0, 3.0);
            let axis = inputs.vec3(-1.0, 1.0);
            let axis2 = inputs.vec3(-1.0, 1.0);
            let j = dJointCreateUniversal(std::ptr::null_mut(), std::ptr::null_mut());
            dJointAttach(j, made[0], made[1]);
            dJointSetUniversalAnchor(j, a[0], a[1], a[2]);
            dJointSetUniversalAxis1(j, axis[0], axis[1], axis[2]);
            dJointSetUniversalAxis2(j, axis2[0], axis2[1], axis2[2]);
            let shipped = universal_stored(axis, axis2, &b1, b2.as_ref(), true);
            let public = universal_stored(axis, axis2, &b1, b2.as_ref(), false);
            for turn in 0..2 {
                let (b1, b2) = if turn == 0 {
                    (b1, b2)
                } else {
                    turned(made, &mut inputs)
                };
                let got = [dJointGetUniversalAngle1(j), dJointGetUniversalAngle2(j)];
                let want = universal_angles(&shipped, &b1, b2.as_ref(), true);
                assert_eq!(bits(got), bits(want), "{i} {turn}");
                let old = universal_angles(&public, &b1, b2.as_ref(), false);
                angles_differ += usize::from(bits(want) != bits(old));
                let (ax1, ax2) = universal_axes(&shipped, &b1, b2.as_ref());
                let got = [
                    dJointGetUniversalAngle1Rate(j),
                    dJointGetUniversalAngle2Rate(j),
                ];
                let want = [
                    rate(ax1, &b1, b2.as_ref(), true),
                    rate(ax2, &b1, b2.as_ref(), true),
                ];
                assert_eq!(bits(got), bits(want), "{i} {turn}");
                let old = [
                    rate(ax1, &b1, b2.as_ref(), false),
                    rate(ax2, &b1, b2.as_ref(), false),
                ];
                rates_differ += usize::from(bits(want) != bits(old));
            }
            dJointDestroy(j);
            dWorldDestroy(world);
        }
    }
    assert!(
        angles_differ > 0,
        "the public relative rotations round the same"
    );
    assert!(rates_differ > 0, "the public rates round the same");
}

/// Hinge-2's stored state: the axes in their bodies' frames and the
/// reference vectors v1, v2 (6 item 2).
struct Hinge2 {
    axis1: V3,
    axis2: V3,
    v1: V3,
    v2: V3,
}

fn hinge2_stored(axis: V3, axis2: V3, b1: &Body, b2: &Body, shipped: bool) -> Hinge2 {
    let axis1 = rot_back(&b1.r, normalise(axis), true);
    let axis2 = rot_back(&b2.r, normalise(axis2), true);
    let ax1 = rot(&b1.r, axis1, shipped);
    let ax2 = rot(&b2.r, axis2, shipped);
    let o = pick(shipped, Yzx);
    let k = dot(o, ax1, ax2);
    let t: V3 = std::array::from_fn(|i| ax2[i] - k * ax1[i]);
    let t = if shipped {
        normalise_with(t, dot(Yzx, t, t))
    } else {
        normalise(t)
    };
    let v = cross(ax1, t);
    Hinge2 {
        axis1,
        axis2,
        v1: rot_back(&b1.r, t, shipped),
        v2: rot_back(&b1.r, v, shipped),
    }
}

/// `dJointGetHinge2Angle1` (6 item 3).
fn hinge2_angle(h: &Hinge2, b1: &Body, b2: &Body, shipped: bool) -> f32 {
    let a = rot_back(&b1.r, rot(&b2.r, h.axis2, shipped), shipped);
    let o = pick(shipped, Yzx);
    -x87::atan2(dot(o, h.v2, a), dot(o, h.v1, a))
}

/// 9 items 9 – 11: hinge-2's reference vectors, angle and rates (the
/// setters' sine and cosine, item 8, are in `harness_rules_3`'s rows).
#[test]
fn hinge2_in_the_shipped_groupings() {
    let _turn = lock();
    let mut inputs = Inputs::new();
    let (mut angles_differ, mut rates_differ) = (0, 0);
    for i in 0..800 {
        // SAFETY: a world, its bodies and a joint made, read and destroyed
        // here, under the lock.
        unsafe {
            let world = dWorldCreate();
            let (made, b1, b2) = two_bodies(world, &mut inputs, false);
            let b2 = b2.unwrap();
            let a = inputs.vec3(-3.0, 3.0);
            let axis = inputs.vec3(-1.0, 1.0);
            let axis2 = inputs.vec3(-1.0, 1.0);
            let j = dJointCreateHinge2(std::ptr::null_mut(), std::ptr::null_mut());
            dJointAttach(j, made[0], made[1]);
            dJointSetHinge2Anchor(j, a[0], a[1], a[2]);
            dJointSetHinge2Axis1(j, axis[0], axis[1], axis[2]);
            dJointSetHinge2Axis2(j, axis2[0], axis2[1], axis2[2]);
            let shipped = hinge2_stored(axis, axis2, &b1, &b2, true);
            let public = hinge2_stored(axis, axis2, &b1, &b2, false);
            for turn in 0..2 {
                let (b1, b2) = if turn == 0 {
                    (b1, b2)
                } else {
                    let (b1, b2) = turned(made, &mut inputs);
                    (b1, b2.unwrap())
                };
                let want = hinge2_angle(&shipped, &b1, &b2, true);
                assert_eq!(dJointGetHinge2Angle1(j).to_bits(), want.to_bits(), "{i}");
                let old = hinge2_angle(&public, &b1, &b2, false);
                angles_differ += usize::from(want.to_bits() != old.to_bits());
                let ax1 = rot(&b1.r, shipped.axis1, true);
                let ax2 = rot(&b2.r, shipped.axis2, true);
                let got = [dJointGetHinge2Angle1Rate(j), dJointGetHinge2Angle2Rate(j)];
                let want = [
                    rate(ax1, &b1, Some(&b2), true),
                    rate(ax2, &b1, Some(&b2), true),
                ];
                assert_eq!(bits(got), bits(want), "{i}");
                let old = [
                    rate(ax1, &b1, Some(&b2), false),
                    rate(ax2, &b1, Some(&b2), false),
                ];
                rates_differ += usize::from(bits(want) != bits(old));
            }
            dJointDestroy(j);
            dWorldDestroy(world);
        }
    }
    assert!(angles_differ > 0, "the public angle rounds the same");
    assert!(rates_differ > 0, "the public rates round the same");
}

/// `dJointSetAMotorAxis`'s stored axis (7.1 item 1): relative to a body,
/// taken into its frame; then normalised.
fn amotor_axis(v: V3, body: Option<&Body>, shipped: bool) -> V3 {
    normalise(body.map_or(v, |b| rot_back(&b.r, v, shipped)))
}

/// `dJointGetAMotorAxis` (7.1 item 2).
fn amotor_get_axis(axis: V3, body: Option<&Body>, shipped: bool) -> V3 {
    body.map_or(axis, |b| rot(&b.r, axis, shipped))
}

/// Euler mode's reference vectors (7.2 item 1).
fn euler_references(axis0: V3, axis2: V3, b1: &Body, b2: Option<&Body>, shipped: bool) -> (V3, V3) {
    match b2 {
        Some(b2) => (
            rot_back(&b1.r, rot(&b2.r, axis2, shipped), shipped),
            rot_back(&b2.r, rot(&b1.r, axis0, shipped), shipped),
        ),
        None => (rot_back(&b1.r, axis2, shipped), rot(&b1.r, axis0, shipped)),
    }
}

/// Euler mode's angles (7.1 item 3, 7.2 item 2).
fn euler_angles(
    axis0: V3,
    axis2: V3,
    (ref1, ref2): (V3, V3),
    b1: &Body,
    b2: Option<&Body>,
    shipped: bool,
) -> [f32; 3] {
    let ax0 = rot(&b1.r, axis0, shipped);
    let ax2 = b2.map_or(axis2, |b2| rot(&b2.r, axis2, shipped));
    let ax1 = normalise(cross(ax2, ax0));
    let ref1 = rot(&b1.r, ref1, shipped);
    let ref2 = b2.map_or(ref2, |b2| rot(&b2.r, ref2, shipped));
    let o = pick(shipped, Yzx);
    let q = cross(ax0, ref1);
    let a0 = -x87::atan2(dot(o, ax2, q), dot(o, ax2, ref1));
    let q = cross(ax0, ax1);
    let a1 = -x87::atan2(dot(o, ax2, ax0), dot(o, ax2, q));
    let q = cross(ax1, ax2);
    let a2 = -x87::atan2(dot(o, ref2, ax1), dot(o, ref2, q));
    [a0, a1, a2]
}

/// 9 items 12, 13 and 15: the angular motor's axes in user mode (each
/// relative to the world, body 1 or body 2), and in Euler mode its axes,
/// reference vectors and the angles a step computes, with two bodies and
/// with one.
#[test]
fn angular_motor_in_the_shipped_groupings() {
    let _turn = lock();
    let mut inputs = Inputs::new();
    let (mut axes_differ, mut angles_differ) = (0, 0);
    for i in 0..1200 {
        let one_body = i % 3 == 2;
        let euler = i % 2 == 1;
        // SAFETY: a world, its bodies and a joint made, stepped, read and
        // destroyed here, under the lock.
        unsafe {
            let world = dWorldCreate();
            let (made, b1, b2) = two_bodies(world, &mut inputs, one_body);
            let j = dJointCreateAMotor(std::ptr::null_mut(), std::ptr::null_mut());
            dJointAttach(j, made[0], made[1]);
            dWorldAddJoint(world, j);
            let v: [V3; 3] = std::array::from_fn(|_| inputs.vec3(-1.0, 1.0));
            // Each axis's body: rel 0 the world, 1 body 1, 2 body 2 (or
            // body 1 without one).
            let body = |rel: c_int| match rel {
                0 => None,
                2 if !one_body => b2.as_ref(),
                _ => Some(&b1),
            };
            let rels: [c_int; 3] = if euler {
                [1, 0, if one_body { 1 } else { 2 }]
            } else {
                std::array::from_fn(|_| {
                    let r = inputs.next(0.0, 3.0) as c_int;
                    if one_body { r.min(1) } else { r }
                })
            };
            if euler {
                dJointSetAMotorMode(j, AMOTOR_EULER);
                for anum in [0, 2] {
                    let [x, y, z] = v[anum];
                    dJointSetAMotorAxis(j, anum as c_int, rels[anum], x, y, z);
                }
            } else {
                dJointSetAMotorMode(j, AMOTOR_USER);
                dJointSetAMotorNumAxes(j, 3);
                for anum in 0..3 {
                    let [x, y, z] = v[anum];
                    dJointSetAMotorAxis(j, anum as c_int, rels[anum], x, y, z);
                }
            }
            let stored = |shipped: bool| -> [V3; 3] {
                std::array::from_fn(|a| amotor_axis(v[a], body(rels[a]), shipped))
            };
            let (shipped, public) = (stored(true), stored(false));
            let refs = |s: &[V3; 3], shipped: bool| {
                euler_references(s[0], s[2], &b1, b2.as_ref(), shipped)
            };
            let (refs_shipped, refs_public) = (refs(&shipped, true), refs(&public, false));
            let (b1, b2) = turned(made, &mut inputs);
            let body = |rel: c_int| match rel {
                0 => None,
                2 if !one_body => b2.as_ref(),
                _ => Some(&b1),
            };
            let anums: &[usize] = if euler { &[0, 2] } else { &[0, 1, 2] };
            for &a in anums {
                let mut got = [0.0; 4];
                dJointGetAMotorAxis(j, a as c_int, got.as_mut_ptr());
                let want = amotor_get_axis(shipped[a], body(rels[a]), true);
                assert_eq!(bits([got[0], got[1], got[2]]), bits(want), "{i} axis {a}");
                let old = amotor_get_axis(public[a], body(rels[a]), false);
                axes_differ += usize::from(bits(want) != bits(old));
            }
            if euler {
                // The step computes the angles from the bodies before it.
                dWorldQuickStep(world, 0.01);
                let got: [f32; 3] = std::array::from_fn(|a| dJointGetAMotorAngle(j, a as c_int));
                let want =
                    euler_angles(shipped[0], shipped[2], refs_shipped, &b1, b2.as_ref(), true);
                assert_eq!(bits(got), bits(want), "{i} angles");
                let old = euler_angles(public[0], public[2], refs_public, &b1, b2.as_ref(), false);
                angles_differ += usize::from(bits(want) != bits(old));
            }
            dJointDestroy(j);
            dWorldDestroy(world);
        }
    }
    assert!(axes_differ > 0, "the public axes round the same");
    assert!(angles_differ > 0, "the public Euler angles round the same");
}
