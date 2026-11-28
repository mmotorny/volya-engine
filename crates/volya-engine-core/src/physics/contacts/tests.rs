// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The contacts' parts against physics-contacts.md, by section.

use super::level::{self, LevelContact, LevelState};
use super::*;
use crate::collision::test_world::Builder;

fn material(friction: f32, damping: f32, spring: f32, flags: u32) -> GameMaterial {
    GameMaterial {
        friction,
        damping,
        spring,
        flags,
        ..GameMaterial::default()
    }
}

/// 1.3: the filter's masks and the group numbers.
#[test]
fn the_filter() {
    let shell = Class {
        word: class::DYNAMIC,
        group: 0,
    };
    assert!(!shell.excludes(Class::CHARACTER));
    assert!(!Class::CHARACTER.excludes(shell));
    // 0x20 excludes characters, 0x8 dynamic objects, 0x200 ragdolls.
    let at_rest = Class {
        word: class::DYNAMIC | class::IGNORE_CHARACTERS,
        group: 0,
    };
    assert!(at_rest.excludes(Class::CHARACTER) && Class::CHARACTER.excludes(at_rest));
    assert!(!at_rest.excludes(shell));
    let ignoring = Class {
        word: class::DYNAMIC | class::IGNORE_DYNAMIC,
        group: 0,
    };
    assert!(ignoring.excludes(shell) && ignoring.excludes(Class::CHARACTER));
    let ragdoll = Class {
        word: class::DYNAMIC | class::RAGDOLL,
        group: 0,
    };
    let no_ragdolls = Class {
        word: class::DYNAMIC | class::IGNORE_RAGDOLL,
        group: 0,
    };
    assert!(no_ragdolls.excludes(ragdoll) && !no_ragdolls.excludes(shell));
    // Small-object bits change nothing.
    let small = Class {
        word: class::DYNAMIC | class::SMALL | class::IGNORE_SMALL,
        group: 0,
    };
    assert!(!small.excludes(small) && !small.excludes(shell));
    // Both with group numbers: only the numbers count.
    let g = |n, extra| Class {
        word: class::DYNAMIC | class::GROUP | extra,
        group: n,
    };
    assert!(g(1, 0).excludes(g(1, 0)));
    assert!(!g(1, class::IGNORE_DYNAMIC).excludes(g(2, 0)));
    // One with a number: the masks.
    assert!(!g(1, 0).excludes(shell));
}

/// 1.3: a shell's class when made.
#[test]
fn a_shells_class() {
    use crate::physics::shell::CollisionClass;
    let mut s = Shell {
        elements: Vec::new(),
        joints: Vec::new(),
        transform: crate::physics::shell::IDENTITY,
        switched_off: false,
        fixed: Vec::new(),
        exact_solver: false,
        ignore_static: true,
        collision_class: CollisionClass {
            small_object: true,
            ignore_small_objects: false,
            ignore_ragdoll: true,
        },
        collision_group: None,
    };
    let c = Class::shell(&s);
    assert_eq!(c.word, 0x4 | 0x2 | 0x40 | 0x200);
    assert!(c.ignores_static());
    s.collision_group = Some(7);
    s.ignore_static = false;
    let c = Class::shell(&s);
    assert_eq!((c.word & 0x3, c.group), (0x1, 7));
}

/// 2.2, and 7.4's figures: `creatures\actor` against
/// `objects\large_metal_trash` at h = 0.01.
#[test]
fn the_properties() {
    let actor = material(0.8, 1.0, 0.8, 0);
    let trash = material(1.0, 1.0, 1.0, BOUNCY);
    let p = properties(&actor, &trash, 0.01);
    assert_eq!(p.mode, mode::SOFT_ERP | mode::SOFT_CFM | mode::APPROX1);
    assert!((p.mu - 0.8).abs() < 1e-6);
    assert!((p.erp - 0.3243).abs() < 1e-4, "{}", p.erp);
    assert!((p.cfm - 1.689e-6).abs() < 1e-9, "{}", p.cfm);
    assert_eq!((p.bounce, p.bounce_speed), (0.01, 1.5));
    // Both bouncy: the lesser bounce, the greater start speed.
    let a = GameMaterial {
        bounce: 0.3,
        bounce_start_speed: 2.0,
        ..trash.clone()
    };
    let b = GameMaterial {
        bounce: 0.5,
        bounce_start_speed: 1.0,
        ..trash.clone()
    };
    let p = properties(&a, &b, 0.01);
    assert_eq!(p.mode & mode::BOUNCE, mode::BOUNCE);
    assert_eq!((p.bounce, p.bounce_speed), (0.3, 2.0));
    // All factors 1: the world's softness.
    let one = material(1.0, 1.0, 1.0, 0);
    let p = properties(&one, &one, 0.01);
    assert!((p.erp - 0.375).abs() < 1e-6 && (p.cfm - 1.5625e-6).abs() < 1e-12);
}

const X: Vec3 = [1.0, 0.0, 0.0];
const Y: Vec3 = [0.0, 1.0, 0.0];
const Z: Vec3 = [0.0, 0.0, 1.0];

/// 1.1, 4.1, 3.5: bounds and supports.
#[test]
fn bounds_and_supports() {
    let cyl = Shape::Cylinder {
        centre: [0.0; 3],
        direction: Y,
        height: 2.0,
        radius: 0.5,
    };
    assert_eq!(reach(&cyl), [0.5, 1.0, 0.5]);
    assert!((support(&cyl, Y) - 1.0).abs() < 1e-6);
    assert!((support(&cyl, X) - 0.5).abs() < 1e-6);
    let b = Shape::Box {
        axes: [X, Y, Z],
        centre: [1.0, 0.0, 0.0],
        half_size: [0.1, 0.2, 0.3],
    };
    assert_eq!(bounds(&b), ([0.9, -0.2, -0.3], [1.1, 0.2, 0.3]));
    let n = [0.6, 0.8, 0.0];
    assert!((support(&b, n) - (0.06 + 0.16)).abs() < 1e-6);
}

fn floor() -> crate::collision::CollisionWorld {
    let mut b = Builder::default();
    b.floor([-5.0, 5.0], [-5.0, 5.0], 0.0, true);
    b.world()
}

fn level_contacts(
    world: &crate::collision::CollisionWorld,
    shape: &Shape,
    state: &mut LevelState,
) -> Vec<LevelContact> {
    let mut out = Vec::new();
    level::collide(world, shape, Some([0.0; 3]), state, 809, &mut out);
    out
}

/// 3.4: a sphere on the floor: one contact per triangle at most, the
/// shared edge used once; normal into the mesh.
#[test]
fn a_sphere_on_the_floor() {
    let world = floor();
    let mut state = LevelState::default();
    let s = Shape::Sphere {
        centre: [0.3, 0.09, 0.2],
        radius: 0.1,
    };
    let c = level_contacts(&world, &s, &mut state);
    assert_eq!(c.len(), 1, "{c:?}");
    assert!((c[0].contact.depth - 0.01).abs() < 1e-6);
    assert_eq!(c[0].contact.normal, [0.0, -1.0, 0.0]);
    assert!((c[0].contact.position[1] + 0.01).abs() < 1e-6);
    assert_eq!(state.previous, Some([0.3, 0.09, 0.2]));
    // On the diagonal both triangles' faces contain the projection.
    let s = Shape::Sphere {
        centre: [0.0, 0.09, 0.0],
        radius: 0.1,
    };
    assert!(!level_contacts(&world, &s, &mut LevelState::default()).is_empty());
    // Above the reach: none.
    let s = Shape::Sphere {
        centre: [0.3, 0.2, 0.2],
        radius: 0.1,
    };
    assert!(level_contacts(&world, &s, &mut LevelState::default()).is_empty());
}

/// 3.4: a box resting on the floor: code 0's vertices.
#[test]
fn a_box_on_the_floor() {
    let world = floor();
    let b = Shape::Box {
        axes: [X, Y, Z],
        centre: [0.5, 0.19, 0.5],
        half_size: [0.2, 0.2, 0.2],
    };
    let c = level_contacts(&world, &b, &mut LevelState::default());
    assert!(!c.is_empty(), "{c:?}");
    for k in &c {
        assert_eq!(k.contact.normal, [0.0, -1.0, 0.0]);
        assert!((k.contact.depth - 0.01).abs() < 1e-5);
        assert!((k.contact.position[1] + 0.01).abs() < 1e-5);
    }
}

/// 3.3: a sphere whose centre crossed the floor is pushed back out, with
/// a record kept until its centre is in front again.
#[test]
fn a_crossing_is_pushed_out() {
    let world = floor();
    let mut state = LevelState::default();
    let above = Shape::Sphere {
        centre: [0.3, 0.05, 0.2],
        radius: 0.1,
    };
    level_contacts(&world, &above, &mut state);
    assert!(!state.inside());
    let below = Shape::Sphere {
        centre: [0.3, -0.02, 0.2],
        radius: 0.1,
    };
    let c = level_contacts(&world, &below, &mut state);
    assert!(state.inside());
    // Both of the floor's triangles are crossed: two records (the second
    // within 135° of the first), one contact each.
    assert_eq!(c.len(), 2, "{c:?}");
    for k in &c {
        assert!((k.contact.depth - 0.12).abs() < 1e-5);
        assert!((k.contact.normal[1] + 1.0).abs() < 1e-6);
    }
    // Still behind: kept, though the previous centre was behind too.
    let c = level_contacts(&world, &below, &mut state);
    assert!(state.inside() && c.len() == 2);
    // In front again: the record goes.
    level_contacts(&world, &above, &mut state);
    assert!(!state.inside());
    // Started behind with a previous centre behind and no record: ignored.
    let mut state = LevelState::default();
    level_contacts(&world, &below, &mut state);
    state.records.clear();
    let c = level_contacts(&world, &below, &mut state);
    assert!(c.is_empty() && !state.inside(), "{c:?}");
}

fn upright(centre: Vec3, height: f32, radius: f32) -> cylinder::Cylinder {
    cylinder::Cylinder::new(centre, Y, height, radius)
}

fn near(a: Vec3, b: Vec3) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < 1e-5)
}

/// 4.3, cylinder–sphere: the axis, the side and the rim; the normal from
/// the sphere towards the cylinder, the point on the sphere's surface.
#[test]
fn cylinder_and_sphere() {
    let c = upright([0.0; 3], 1.0, 0.5);
    // Above the top: axis 1.
    let k = cylinder::sphere(&c, [0.1, 0.7, 0.0], 0.25).unwrap();
    assert!(near(k.normal, [0.0, -1.0, 0.0]), "{k:?}");
    assert!((k.depth - 0.05).abs() < 1e-6, "{k:?}");
    assert!(near(k.position, [0.1, 0.45, 0.0]), "{k:?}");
    // Beside it: axis 2.
    let k = cylinder::sphere(&c, [0.0, 0.1, -0.7], 0.25).unwrap();
    assert!(near(k.normal, [0.0, 0.0, 1.0]), "{k:?}");
    assert!((k.depth - 0.05).abs() < 1e-6, "{k:?}");
    // Off the rim's edge: axis 3, along the line from the rim point.
    let k = cylinder::sphere(&c, [0.6, 0.6, 0.0], 0.2).unwrap();
    let s = std::f32::consts::FRAC_1_SQRT_2;
    assert!(near(k.normal, [-s, -s, 0.0]), "{k:?}");
    assert!((k.depth - (0.2 - 0.1 * 2f32.sqrt())).abs() < 1e-5, "{k:?}");
    // Apart: none.
    assert!(cylinder::sphere(&c, [0.0, 0.8, 0.0], 0.25).is_none());
    assert!(cylinder::sphere(&c, [0.7, 0.7, 0.0], 0.2).is_none());
}

/// 4.3, cylinder–box: an upright cylinder on a box ties the axis with the
/// face, so the axis (first) gives the box's vertex; tilted a little, the
/// face's three rim points; a lying one one; the player's torso against a
/// leaf beside it is pushed off it.
#[test]
fn cylinder_and_box() {
    let floor = ([0.0, -0.5, 0.0], [X, Y, Z], [2.0, 0.5, 2.0]);
    let mut out = Vec::new();
    let c = upright([0.0, 0.49, 0.0], 1.0, 0.3);
    cylinder::cuboid(&c, floor.0, floor.1, floor.2, &mut out);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(near(out[0].normal, Y) && (out[0].depth - 0.01).abs() < 1e-5);
    assert!(near(out[0].position, [2.0, 0.0, 2.0]), "{out:?}");
    out.clear();
    let (sin, cos) = (0.01f32, (1.0f32 - 1e-4).sqrt());
    let at = 0.5 * cos + 0.483 - 0.5;
    let c = cylinder::Cylinder::new([0.0, at, 0.0], [sin, cos, 0.0], 1.0, 0.3);
    cylinder::cuboid(&c, floor.0, floor.1, floor.2, &mut out);
    assert_eq!(out.len(), 3, "{out:?}");
    assert!((out[0].depth - 0.02).abs() < 1e-5, "{out:?}");
    for k in &out[1..] {
        // 1.5·r·sin θ higher on the rim.
        assert!((k.depth - (0.02 - 1.5 * 0.3 * sin)).abs() < 1e-5, "{k:?}");
    }
    for k in &out {
        assert!(near(k.normal, Y), "{k:?}");
        assert!((k.position[1] + k.depth).abs() < 1e-4, "{k:?}");
    }
    // Lying along x: one contact, at the rim of a cap.
    out.clear();
    let c = cylinder::Cylinder::new([0.0, 0.29, 0.0], X, 1.0, 0.3);
    cylinder::cuboid(&c, floor.0, floor.1, floor.2, &mut out);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(near(out[0].normal, Y) && (out[0].depth - 0.01).abs() < 1e-5);
    assert!((out[0].position[0].abs() - 0.5).abs() < 1e-5, "{out:?}");
    // Apart: none.
    out.clear();
    let c = upright([0.0, 0.51, 0.0], 1.0, 0.3);
    cylinder::cuboid(&c, floor.0, floor.1, floor.2, &mut out);
    assert!(out.is_empty());
    let c = upright([0.0, 1.0, 0.0], 1.0, 0.29);
    cylinder::cuboid(&c, [0.37, 1.0, 0.0], [X, Y, Z], [0.1, 1.3, 0.7], &mut out);
    assert!(!out.is_empty());
    for k in &out {
        assert!(near(k.normal, [-1.0, 0.0, 0.0]), "{k:?}");
        assert!((k.depth - 0.02).abs() < 1e-5, "{k:?}");
    }
}

/// physics-library.md 3.5: normalisation, the slow path below its least.
#[test]
fn normalisation() {
    assert_eq!(cylinder::normalise([3.0, 0.0, 4.0]), [0.6, 0.0, 0.8]);
    let v = cylinder::normalise([1e-3, -2e-3, 0.5e-3]);
    assert!(
        (crate::collision::geometry::length(v) - 1.0).abs() < 1e-6,
        "{v:?}"
    );
    assert_eq!(cylinder::normalise([0.0; 3]), [1.0, 0.0, 0.0]);
}

/// A rotation's columns from the axis: orthonormal and right-handed.
#[test]
fn cylinder_frames() {
    for axis in [
        Y,
        X,
        Z,
        [0.0, -1.0, 0.0],
        [0.6, 0.0, 0.8],
        [0.48, 0.6, -0.64],
    ] {
        let c = cylinder::Cylinder::new([0.0; 3], axis, 1.0, 1.0);
        for (u, v) in [(c.c0, c.a), (c.a, c.c2), (c.c2, c.c0)] {
            assert!(dot(u, v).abs() < 1e-6, "{axis:?}");
        }
        let x = crate::collision::geometry::cross(c.c0, c.a);
        assert!(near(x, c.c2), "{axis:?}: {c:?}");
    }
}

/// 3.4 (ours, open): an upright cylinder standing on the floor gets three
/// contacts over its two triangles; 3.3: a crossing pushes it out.
#[test]
fn a_cylinder_on_the_floor() {
    let world = floor();
    let cyl = |centre, direction| Shape::Cylinder {
        centre,
        direction,
        height: 1.0,
        radius: 0.25,
    };
    let c = level_contacts(
        &world,
        &cyl([0.3, 0.49, 0.2], Y),
        &mut LevelState::default(),
    );
    assert_eq!(c.len(), 3, "{c:?}");
    for k in &c {
        assert_eq!(k.contact.normal, [0.0, -1.0, 0.0]);
        assert!((k.contact.depth - 0.01).abs() < 1e-5, "{k:?}");
    }
    // Lying: one, at a cap's rim.
    let c = level_contacts(
        &world,
        &cyl([0.3, 0.24, 0.2], X),
        &mut LevelState::default(),
    );
    assert_eq!(c.len(), 1, "{c:?}");
    let mut state = LevelState::default();
    level_contacts(&world, &cyl([0.3, 0.6, 0.2], Y), &mut state);
    let c = level_contacts(&world, &cyl([0.3, -0.02, 0.2], Y), &mut state);
    assert!(state.inside());
    // Upright (|c| = 1): one per record, at depth s(f) − d.
    assert_eq!(c.len(), 2, "{c:?}");
    for k in &c {
        assert!((k.contact.depth - 0.52).abs() < 1e-5, "{k:?}");
    }
}
