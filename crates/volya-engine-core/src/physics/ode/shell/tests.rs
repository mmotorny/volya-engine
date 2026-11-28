// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Activated shells against physics-shells.md 2, 3.3, 3.5 and 5.1,
//! physics-joints.md and physics-resting.md, on a synthetic door.

use volya_engine_formats::model::transform;

use super::*;
use crate::physics::shell::tests::{door, materials, record};
use crate::physics::shell::{SKELETON_ACTIVE, physics_object};
use crate::physics::{INITIAL_STEP, LIBRARY_ITERATIONS, World};

fn params() -> Params {
    Params::new(INITIAL_STEP, LIBRARY_ITERATIONS)
}

fn door_shell(flags: u8) -> Shell {
    let mut r = record(3, "frame", None);
    r.skeleton_flags = flags;
    r.transform = transform([10.0, 0.0, 5.0], [0.0, std::f32::consts::FRAC_PI_2, 0.0]);
    physics_object(&door(), &r, &materials()).unwrap().unwrap()
}

fn close(a: Vec3, b: Vec3, e: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < e)
}

#[test]
fn activation_2_1_places_bodies_at_their_mass_centres() {
    let shell = door_shell(0);
    let mut solver = OdeSolver::new();
    let s = solver.activate_shell(&params(), &shell);
    assert_eq!(s.elements(), 2);
    // The leaf's body: its mass centre (0.4, 0, 0) of its frame, 1 m above
    // the record's position, the record turned a quarter about y.
    let x = &shell.transform;
    let leaf = s.body(&solver, 1).unwrap();
    let want = transform_point(&multiply(&shell.elements[1].frame, x), [0.4, 0.0, 0.0]);
    assert!(
        close(leaf.position, want, 1e-5),
        "{:?} {want:?}",
        leaf.position
    );
    assert!((leaf.mass - 50.0).abs() < 1e-3, "{}", leaf.mass);
    assert_eq!((leaf.linear, leaf.angular), ([0.0; 3], [0.0; 3]));
    // Reading back, the frame is the element's (the bone's origin).
    let frame = s.frame(&solver, 1).unwrap();
    let want = multiply(&shell.elements[1].frame, x);
    for r in 0..4 {
        assert!(close(
            [frame[r][0], frame[r][1], frame[r][2]],
            [want[r][0], want[r][1], want[r][2]],
            1e-5
        ));
    }
    // physics-shells.md 5.1: the shapeless frame is fixed, 10⁸, gravity off.
    let f = s.body(&solver, 0).unwrap();
    assert!(s.fixed(0) && !s.fixed(1));
    assert_eq!((f.mass, f.gravity), (FIXED_MASS, false));
    assert!(leaf.gravity);
    // physics-resting.md 1: skeleton flags 0, switched off.
    assert!(!leaf.enabled && !f.enabled && s.resting);
}

#[test]
fn hinge_root_2_joins_the_leaf_to_the_static_environment() {
    let shell = door_shell(0);
    let mut solver = OdeSolver::new();
    let s = solver.activate_shell(&params(), &shell);
    let joints: Vec<_> = s.joints().collect();
    assert_eq!(joints.len(), 1);
    let j = joints[0].1;
    // 2.1: the stops as the bone's (−1.5 … 0); 2.3: no motor at creation.
    let p = |n| solver.hinge_param(j, n).unwrap();
    assert_eq!((p(param::LO_STOP), p(param::HI_STOP)), (-1.5, 0.0));
    assert_eq!((p(param::F_MAX), p(param::VEL)), (0.0, 0.0));
    // `bone` 3.3 with factors 1: as stiff as the world.
    let world = params();
    assert!((p(param::STOP_ERP) - world.erp).abs() < 1e-6);
    assert!((p(param::CFM) / world.cfm - 1.0).abs() < 1e-5);
    assert_eq!(solver.hinge_angle(j), Some(0.0));
    // physics-shells.md 3.5: naming a fixed bone requests the exact solver.
    assert!(solver.marked_exact(s.group()));
}

#[test]
fn waking_root_3_1_and_the_motor_root_2_2() {
    let shell = door_shell(0);
    let mut world: World<OdeShell, OdeSolver> =
        World::new(OdeSolver::new(), INITIAL_STEP, LIBRARY_ITERATIONS, -100.0);
    let params = *world.params();
    let s = world.solver_mut().activate_shell(&params, &shell);
    let joint = s.joints().next().unwrap().1;
    let id = world.add(s);
    assert!(!world.is_active(id));
    let before = world.get(id).unwrap().frame(world.solver(), 1).unwrap();
    // Resting, it is not stepped.
    for _ in 0..10 {
        world.step(0, 0);
    }
    // Woken by the closing motor, held at its closed stop.
    let wake = |world: &mut World<OdeShell, OdeSolver>, force, velocity| {
        let (s, solver) = world.with_solver(id);
        s.unwrap().wake(solver);
        solver.set_hinge_motor(joint, force, velocity);
        world.activate(id).unwrap();
    };
    wake(&mut world, 10_000.0, 1.0);
    for _ in 0..200 {
        world.step(0, 0);
    }
    let solver = world.solver();
    let s = world.get(id).unwrap();
    let after = s.frame(solver, 1).unwrap();
    let angle = solver.hinge_angle(joint).unwrap();
    assert!(angle.abs() < 1e-3, "{angle}");
    for r in 0..4 {
        let (a, b) = (after[r], before[r]);
        assert!(
            close([a[0], a[1], a[2]], [b[0], b[1], b[2]], 2e-3),
            "{after:?} {before:?}"
        );
    }
    assert!(s.body(solver, 1).unwrap().enabled && !s.resting);
    // The frame's body does not move.
    let f = s.body(solver, 0).unwrap();
    assert_eq!(f.linear, [0.0; 3]);
    // The opening motor (velocity −3) turns it toward its open stop: the
    // angle φ grows to 1.5.
    wake(&mut world, 2100.0, -3.0);
    for _ in 0..200 {
        world.step(0, 0);
    }
    let solver = world.solver();
    let angle = solver.hinge_angle(joint).unwrap();
    assert!((angle - 1.5).abs() < 0.05, "{angle}");
}

#[test]
fn motion_2_4_divisors_and_drags_slow_a_falling_shell() {
    // A free leaf (no fixed bone, awake): it falls, slower than freely.
    let mut r = record(3, "", None);
    r.skeleton_flags = SKELETON_ACTIVE;
    let mut model = door();
    model.bones[1].physics.joint = volya_engine_formats::bone_physics::JointType::None;
    let shell = physics_object(&model, &r, &materials()).unwrap().unwrap();
    let mut world: World<OdeShell, OdeSolver> =
        World::new(OdeSolver::new(), INITIAL_STEP, LIBRARY_ITERATIONS, -100.0);
    let params = *world.params();
    let s = world.solver_mut().activate_shell(&params, &shell);
    assert!(!world.solver_mut().marked_exact(s.group()));
    let id = world.add(s);
    world.activate(id).unwrap();
    for _ in 0..10 {
        world.step(0, 0);
    }
    let solver = world.solver();
    let v = world.get(id).unwrap().body(solver, 1).unwrap().linear[1];
    let free = -19.62 * 0.01 * 10.0;
    assert!(v < 0.0 && v > free + 1e-3, "{v} vs {free}");
    // Each step divides the last velocity by 1.01 before gravity's 0.1962.
    let mut want = 0.0f32;
    for _ in 0..10 {
        want = want / 1.01 - 0.1962;
    }
    assert!((v - want).abs() < 0.01, "{v} vs {want}");
}

#[test]
fn drawing_8_2_blends_the_last_two_steps() {
    // A falling leaf, as above.
    let mut r = record(3, "", None);
    r.skeleton_flags = SKELETON_ACTIVE;
    let mut model = door();
    model.bones[1].physics.joint = volya_engine_formats::bone_physics::JointType::None;
    let shell = physics_object(&model, &r, &materials()).unwrap().unwrap();
    let mut world: World<OdeShell, OdeSolver> =
        World::new(OdeSolver::new(), INITIAL_STEP, LIBRARY_ITERATIONS, -100.0);
    let params = *world.params();
    let s = world.solver_mut().activate_shell(&params, &shell);
    let id = world.add(s);
    world.activate(id).unwrap();
    // No step yet: read from the body.
    let (s, solver) = world.with_solver(id);
    let s = s.unwrap();
    let start = s.drawn_frame(solver, 1, 0.5).unwrap();
    assert_eq!(start, s.frame(solver, 1).unwrap());
    world.step(0, 0);
    let first = world.get(id).unwrap().frame(world.solver(), 1).unwrap();
    world.step(0, 0);
    let (s, solver) = world.with_solver(id);
    let s = s.unwrap();
    let last = s.frame(solver, 1).unwrap();
    // Steps ran: halfway between the last two steps' states.
    let blended = s.drawn_frame(solver, 1, 0.5).unwrap();
    let mid = 0.5 * (first[3][1] + last[3][1]);
    assert!((blended[3][1] - mid).abs() < 1e-5, "{blended:?}");
    assert!(blended[3][1] > last[3][1] && blended[3][1] < first[3][1]);
    // Read since: from the body again.
    assert_eq!(s.drawn_frame(solver, 1, 0.5).unwrap(), last);
}

#[test]
fn the_blend_turns_along_the_shorter_arc() {
    let near =
        |x: &Mat4, y: &Mat4| (0..4).all(|r| (0..4).all(|c| (x[r][c] - y[r][c]).abs() < 1e-5));
    let a = transform([0.0; 3], [0.0, 0.2, 0.0]);
    let b = transform([2.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    assert!(near(&blend(&a, &b, 0.0), &a));
    assert!(near(&blend(&a, &b, 1.0), &b));
    let half = transform([1.0, 0.0, 0.0], [0.0, 0.6, 0.0]);
    assert!(near(&blend(&a, &b, 0.5), &half), "{:?}", blend(&a, &b, 0.5));
    // Every branch of the rotation's reading.
    for angles in [
        [2.5, -1.2, 3.0],
        [3.1, 0.0, 0.0],
        [0.0, 3.1, 0.0],
        [0.0, 0.0, 3.1],
    ] {
        let c = transform([0.0; 3], angles);
        assert!(near(&blend(&c, &c, 0.3), &c), "{angles:?}");
    }
}
