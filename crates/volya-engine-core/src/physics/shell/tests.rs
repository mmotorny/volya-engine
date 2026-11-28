// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The build against physics-shells.md, by section, on synthetic models.

use volya_engine_formats::bone_physics::{
    AxisLimits, BonePhysics, BoxShape, Cylinder, JointType, Shape as BoneShape, ShapeType, Sphere,
    shape_flags,
};
use volya_engine_formats::gamemtl::{DYNAMIC, GameMaterial};
use volya_engine_formats::ltx;
use volya_engine_formats::model::{Bone, Header, transform};

use super::*;

const IDENTITY3: [Vec3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub(crate) fn physics(kind: ShapeType, joint: JointType, mass: f32) -> BonePhysics {
    BonePhysics {
        version: 1,
        material: "objects\\metal".into(),
        shape: BoneShape {
            kind,
            flags: 0,
            cuboid: BoxShape {
                axes: IDENTITY3,
                centre: [0.5, 0.0, 0.0],
                half_size: [0.5, 1.0, 0.1],
            },
            sphere: Sphere {
                centre: [0.0, 0.2, 0.0],
                radius: 0.3,
            },
            cylinder: Cylinder {
                centre: [0.0; 3],
                direction: [0.0, 0.0, 1.0],
                height: 2.0,
                radius: 0.5,
            },
        },
        joint,
        limits: [AxisLimits {
            low: 0.0,
            high: 0.0,
            spring_factor: 1.0,
            damping_factor: 1.0,
        }; 3],
        spring: 1.0,
        damping: 1.0,
        joint_flags: 0,
        break_force: 0.0,
        break_torque: 0.0,
        friction: None,
        mass,
        centre_of_mass: [0.4, 0.0, 0.0],
    }
}

pub(crate) fn bone(name: &str, parent: Option<usize>, at: Vec3, physics: BonePhysics) -> Bone {
    Bone {
        name: name.into(),
        parent,
        box_rotation: IDENTITY3,
        box_centre: [0.0; 3],
        box_half_size: [0.0; 3],
        bind_rotation: [0.0; 3],
        bind_position: at,
        physics,
    }
}

pub(crate) fn header(kind: u8) -> Header {
    Header {
        kind,
        shader: 0,
        box_min: [-1.0, 0.0, -0.5],
        box_max: [1.0, 2.0, 0.5],
        sphere_centre: [0.0; 3],
        sphere_radius: 0.0,
    }
}

/// A door: `frame` (root, no shape), `leaf` (a box hinged about its y
/// axis, limits −1.5 … 0, mass 50) 1 m above it, `latch` (no physics).
pub(crate) fn door() -> ShellModel {
    let mut leaf = physics(ShapeType::Box, JointType::Limited, 50.0);
    leaf.limits[1] = AxisLimits {
        low: -1.5,
        high: 0.0,
        spring_factor: 1.0,
        damping_factor: 1.0,
    };
    let mut latch = physics(ShapeType::Box, JointType::Rigid, 1.0);
    latch.shape.flags = shape_flags::NO_PHYSICS;
    ShellModel {
        header: header(10),
        bones: vec![
            bone(
                "frame",
                None,
                [0.0; 3],
                physics(ShapeType::None, JointType::Rigid, 1.0),
            ),
            bone("leaf", Some(0), [0.0, 1.0, 0.0], leaf),
            bone("latch", Some(1), [1.0, 0.0, 0.0], latch),
        ],
        user_data: None,
    }
}

pub(crate) fn materials() -> Vec<GameMaterial> {
    vec![
        GameMaterial {
            name: "default".into(),
            ..GameMaterial::default()
        },
        GameMaterial {
            name: "objects\\metal".into(),
            flags: DYNAMIC,
            ..GameMaterial::default()
        },
    ]
}

pub(crate) fn record<'a>(
    physics_type: u32,
    fixed: &'a str,
    custom: Option<&'a Config>,
) -> PhysicsRecord<'a> {
    PhysicsRecord {
        physics_type,
        mass: 10.0,
        fixed_bones: fixed,
        skeleton_flags: 0,
        startup_animation: "",
        custom_data: custom,
        transform: transform([10.0, 0.0, 5.0], [0.0, std::f32::consts::FRAC_PI_2, 0.0]),
        bone_state: BoneState::default(),
    }
}

fn close(a: Vec3, b: Vec3) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < 1e-5)
}

#[test]
fn geometries_2_2_boxes_have_a_minimum_half_size() {
    let mut p = physics(ShapeType::Box, JointType::None, 1.0);
    p.shape.cuboid.half_size = [0.0, 0.002, 0.3];
    let Shape::Box { half_size, .. } = Shape::of(&p.shape.used().unwrap()) else {
        panic!()
    };
    assert_eq!(half_size, [0.005, 0.005, 0.3]);
    let s = Shape::of(
        &physics(ShapeType::Sphere, JointType::None, 1.0)
            .shape
            .used()
            .unwrap(),
    );
    assert!((s.volume() - 4.0 / 3.0 * std::f32::consts::PI * 0.027).abs() < 1e-6);
    let c = Shape::of(
        &physics(ShapeType::Cylinder, JointType::None, 1.0)
            .shape
            .used()
            .unwrap(),
    );
    assert!((c.volume() - std::f32::consts::PI * 0.5).abs() < 1e-6);
}

#[test]
fn mass_2_3_a_mass_and_centre_spread_by_volume() {
    let mut e = Element::new(IDENTITY, None, None, 0);
    let small = Geometry {
        shape: Shape::Sphere {
            centre: [0.0; 3],
            radius: 1.0,
        },
        bone: None,
    };
    let big = Geometry {
        shape: Shape::Box {
            axes: IDENTITY3,
            centre: [2.0, 0.0, 0.0],
            half_size: [1.0; 3],
        },
        bone: None,
    };
    e.add([small, big], 12.0, Some([1.0, 0.0, 0.0]));
    let (m, c) = e.mass_centre();
    assert_eq!((m, c), (12.0, [1.0, 0.0, 0.0]));
    let pieces = e.mass_pieces();
    let v = 4.0 / 3.0 * std::f32::consts::PI + 8.0;
    assert!((pieces[0].density - 12.0 / v).abs() < 1e-6);
    assert_eq!(pieces[0].density, pieces[1].density);
    assert_eq!(
        (pieces[0].offset, pieces[1].offset),
        ([-1.0, 0.0, 0.0], [1.0, 0.0, 0.0])
    );
    // A mass alone: about the volume-weighted centre.
    let mut e = Element::new(IDENTITY, None, None, 0);
    e.add(
        [
            big,
            Geometry {
                shape: Shape::Box {
                    axes: IDENTITY3,
                    centre: [4.0, 0.0, 0.0],
                    half_size: [1.0; 3],
                },
                bone: None,
            },
        ],
        2.0,
        None,
    );
    assert_eq!(e.mass_centre(), (2.0, [3.0, 0.0, 0.0]));
    // A density alone.
    e.mass = Mass::Density(0.5);
    assert_eq!(e.mass_centre(), (8.0, [3.0, 0.0, 0.0]));
    // A second part (a rigid bone, `bone` 5): the mass-weighted mean.
    let mut e = Element::new(IDENTITY, None, None, 0);
    e.add([small], 1.0, Some([0.0; 3]));
    e.add([big], 3.0, Some([4.0, 0.0, 0.0]));
    assert_eq!(e.mass_centre(), (4.0, [3.0, 0.0, 0.0]));
}

#[test]
fn motion_2_4_starting_values_and_drags() {
    let m = Motion::default();
    assert_eq!(
        (
            m.linear_limit,
            m.angular_limit,
            m.linear_divisor,
            m.angular_divisor
        ),
        (150.0, 9.817_477, 1.01, 1.01)
    );
    assert_eq!((m.linear_drag, m.angular_drag), (0.0002, 0.05));
}

#[test]
fn bone_state_3_1_without_saved_state_all_visible_root_0() {
    let s = BoneState::default();
    assert_eq!(s.root, 0);
    assert!((0..70).all(|b| s.visible(b)));
    let hidden = BoneState {
        root: 0,
        visible: !0b10,
    };
    assert!(!hidden.visible(1) && hidden.visible(0));
}

#[test]
fn walk_3_2_the_door_gets_a_shapeless_root_and_its_leaf() {
    let model = door();
    let (s, ended) = from_bones(&model, BoneState::default(), &materials()).unwrap();
    // 3.5: two elements; the latch has no physics and adds nothing.
    assert_eq!(s.elements.len(), 2);
    assert_eq!(ended, [Some(0), Some(1), Some(1)]);
    let (frame, leaf) = (&s.elements[0], &s.elements[1]);
    assert!(!frame.has_geometry() && frame.bone == Some(0) && frame.above.is_none());
    assert_eq!(
        (leaf.bone, leaf.above, leaf.material),
        (Some(1), Some(0), 1)
    );
    // The leaf's frame is its bind transform below the root's.
    assert_eq!(leaf.frame[3], [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(leaf.geometries.len(), 1);
    assert_eq!(leaf.geometries[0].bone, Some(1));
    assert_eq!(leaf.mass_centre(), (50.0, [0.4, 0.0, 0.0]));
    // 3.3: the hinge from the frame's element (first) to the leaf's.
    assert_eq!(s.joints.len(), 1);
    let j = s.joints[0];
    assert_eq!((j.first, j.second, j.bone), (0, 1, 1));
    let JointKind::Hinge { axis, limits } = j.kind else {
        panic!("{:?}", j.kind)
    };
    assert_eq!((axis, limits.low, limits.high), (Axis::Y, -1.5, 0.0));
}

#[test]
fn walk_3_2_rigid_bones_join_above_in_its_frame_and_hidden_ones_add_nothing() {
    let mut model = door();
    // The latch with a box, rigid, 1 m along the leaf's x.
    model.bones[2].physics = physics(ShapeType::Box, JointType::Rigid, 50.0);
    let (s, ended) = from_bones(&model, BoneState::default(), &materials()).unwrap();
    assert_eq!(ended[2], Some(1));
    let leaf = &s.elements[1];
    assert_eq!(leaf.geometries.len(), 2);
    assert_eq!(leaf.geometries[1].bone, Some(2));
    assert!(close(leaf.geometries[1].shape.centre(), [1.5, 0.0, 0.0]));
    // Two parts of 50: the mean of 0.4 and 1.4.
    let (m, c) = leaf.mass_centre();
    assert_eq!(m, 100.0);
    assert!(close(c, [0.9, 0.0, 0.0]), "{c:?}");
    // Hidden, the latch adds nothing; its children would still be walked.
    let state = BoneState {
        root: 0,
        visible: !0b100,
    };
    let (s, _) = from_bones(&model, state, &materials()).unwrap();
    assert_eq!(s.elements[1].geometries.len(), 1);
    // A jointed bone with a used shape gets its own element and joint.
    model.bones[2].physics.joint = JointType::Ball;
    let (s, _) = from_bones(&model, BoneState::default(), &materials()).unwrap();
    assert_eq!(s.elements.len(), 3);
    assert_eq!(s.joints[1].kind, JointKind::Other(JointType::Ball));
    // Joint type 4: an element, no joint.
    model.bones[2].physics.joint = JointType::None;
    let (s, _) = from_bones(&model, BoneState::default(), &materials()).unwrap();
    assert_eq!((s.elements.len(), s.joints.len()), (3, 1));
}

#[test]
fn types_4_1_type_3_with_fixed_bones_requests_the_exact_solver() {
    let model = door();
    let s = physics_object(&model, &record(3, "frame", None), &materials())
        .unwrap()
        .unwrap();
    assert!(s.exact_solver && s.switched_off);
    assert_eq!(s.fixed, [0]);
    assert_eq!(s.transform, record(3, "", None).transform);
    // 4.1: the drags of every physics object.
    assert!(
        s.elements
            .iter()
            .all(|e| (e.motion.linear_drag, e.motion.angular_drag) == OBJECT_DRAGS)
    );
    // 4.5: the record's mass is not read; the bone data's is.
    assert_eq!(s.elements[1].mass_centre().0, 50.0);
    // Without fixed bones no request; flag 0x01 set: not switched off.
    let mut r = record(3, "", None);
    r.skeleton_flags = SKELETON_ACTIVE;
    let s = physics_object(&model, &r, &materials()).unwrap().unwrap();
    assert!(!s.exact_solver && !s.switched_off && s.fixed.is_empty());
    // 4.4: a fixed bone the model lacks is fatal.
    let e = physics_object(&model, &record(3, "frame, hinge", None), &materials()).unwrap_err();
    assert_eq!(e.to_string(), "wrong fixed bone 'hinge'");
    // 4.3: types 1 and 2 are not built here.
    assert_eq!(
        physics_object(&model, &record(1, "", None), &materials()).unwrap(),
        None
    );
}

#[test]
fn type_0_4_2_one_box_at_the_bounding_box_with_the_records_mass() {
    let mut model = door();
    model.bones[0].physics.material = "default".into();
    let s = physics_object(&model, &record(0, "frame", None), &materials());
    // The root's material must be dynamic (`bone` 6).
    assert!(s.is_err());
    let model = door();
    let s = physics_object(&model, &record(0, "", None), &materials())
        .unwrap()
        .unwrap();
    assert_eq!((s.elements.len(), s.joints.len()), (1, 0));
    let e = &s.elements[0];
    assert_eq!((e.frame, e.bone), (IDENTITY, None));
    let Shape::Box {
        centre, half_size, ..
    } = e.geometries[0].shape
    else {
        panic!()
    };
    assert_eq!((centre, half_size), ([0.0, 1.0, 0.0], [1.0, 1.0, 0.5]));
    assert_eq!(e.mass_centre(), (10.0, [0.0, 1.0, 0.0]));
    assert!(!s.exact_solver && s.fixed.is_empty() && s.switched_off);
}

#[test]
fn keys_5_2_custom_data_then_user_data() {
    let model = door();
    let custom = ltx::parse(b"[logic]\ncfg = x\n[collide]\nignore_static\nsmall_object\n").unwrap();
    let s = physics_object(&model, &record(3, "frame", Some(&custom)), &materials())
        .unwrap()
        .unwrap();
    assert!(s.ignore_static && s.collision_class.small_object);
    assert!(!s.collision_class.ignore_ragdoll);
    // Without fixed bones `ignore_static` is not read.
    let s = physics_object(&model, &record(3, "", Some(&custom)), &materials())
        .unwrap()
        .unwrap();
    assert!(!s.ignore_static && s.collision_class.small_object);
    // `[physics_common] fixed_bones` in the model's user data fixes, and a
    // name the model lacks is fatal.
    let mut m = door();
    m.user_data = Some(
        ltx::parse(b"[physics_common]\nfixed_bones = leaf\n[collide]\nignore_ragdoll\n").unwrap(),
    );
    let s = physics_object(&m, &record(3, "", None), &materials())
        .unwrap()
        .unwrap();
    assert_eq!(s.fixed, [1]);
    assert!(!s.exact_solver && s.collision_class.ignore_ragdoll);
    m.user_data = Some(ltx::parse(b"[physics_common]\nfixed_bones = hinge\n").unwrap());
    assert!(physics_object(&m, &record(3, "", None), &materials()).is_err());
    // The spawn's parts keys: a collision group from the counter.
    let mut m = door();
    m.user_data = Some(
        ltx::parse(b"[collide]\nnot_collide_parts\n[collide_parts]\nignore_small_objects\n")
            .unwrap(),
    );
    let mut s = physics_object(&m, &record(3, "", None), &materials())
        .unwrap()
        .unwrap();
    let mut groups = 7;
    apply_parts_keys(&mut s, &m, &mut groups);
    assert_eq!((s.collision_group, groups), (Some(7), 8));
    assert!(s.collision_class.ignore_small_objects);
}

#[test]
fn lamps_5_3_switched_off_their_root_fixed_without_a_list() {
    let model = door();
    let custom = ltx::parse(b"[collide]\nignore_static\n").unwrap();
    let r = LampRecord {
        light_flags: LAMP_PHYSICS,
        fixed_bones: "",
        custom_data: Some(&custom),
        transform: IDENTITY,
        bone_state: BoneState::default(),
    };
    let s = hanging_lamp(&model, &r, &materials()).unwrap().unwrap();
    assert!(s.switched_off && !s.exact_solver);
    assert_eq!(s.fixed, [0]);
    // "has fixed bones" is the field's text: none, so not ignore static.
    assert!(!s.ignore_static);
    let s = hanging_lamp(
        &model,
        &LampRecord {
            fixed_bones: "leaf",
            ..r
        },
        &materials(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(s.fixed, [1]);
    assert!(s.ignore_static);
    assert_eq!(s.elements[1].motion.linear_drag, OBJECT_DRAGS.0);
    let none = LampRecord {
        light_flags: 0x02,
        ..r
    };
    assert_eq!(hanging_lamp(&model, &none, &materials()).unwrap(), None);
}

#[test]
fn spawn_7_an_animated_model_needs_a_startup_animation() {
    let mut model = door();
    model.header = header(3);
    let e = physics_object(&model, &record(3, "", None), &materials()).unwrap_err();
    assert_eq!(e.to_string(), "no startup animation");
    let mut r = record(3, "", None);
    r.startup_animation = "idle";
    assert!(physics_object(&model, &r, &materials()).unwrap().is_some());
}

#[test]
fn skeleton_objects_1_plain_build_switched_off_by_their_flag() {
    let model = door();
    let s = skeleton_object(&model, 0, IDENTITY, BoneState::default(), &materials()).unwrap();
    assert!(s.switched_off && s.fixed.is_empty());
    let s = skeleton_object(
        &model,
        SKELETON_ACTIVE,
        IDENTITY,
        BoneState::default(),
        &materials(),
    )
    .unwrap();
    assert!(!s.switched_off);
}

#[test]
fn inverse_rigid_undoes_a_transform() {
    let m = transform([1.0, 2.0, 3.0], [0.3, -0.7, 1.1]);
    let p = transform_point(&multiply(&m, &inverse_rigid(&m)), [4.0, 5.0, 6.0]);
    assert!(close(p, [4.0, 5.0, 6.0]), "{p:?}");
}
