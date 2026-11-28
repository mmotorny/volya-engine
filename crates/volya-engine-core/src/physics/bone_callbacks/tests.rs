// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;
use crate::physics::shell::{Element, IDENTITY, Mass, Motion};
use volya_engine_formats::model::{transform, transform_point};

fn element(bone: Option<usize>) -> Element {
    Element {
        frame: IDENTITY,
        bone,
        above: None,
        material: 0,
        geometries: Vec::new(),
        mass: Mass::Parts(Vec::new()),
        motion: Motion::default(),
    }
}

fn shell(bones: &[Option<usize>]) -> Shell {
    Shell {
        elements: bones.iter().map(|&b| element(b)).collect(),
        joints: Vec::new(),
        transform: IDENTITY,
        switched_off: false,
        fixed: Vec::new(),
        exact_solver: false,
        ignore_static: false,
        collision_class: Default::default(),
        collision_group: None,
    }
}

fn close(a: &Mat4, b: &Mat4) -> bool {
    (0..4).all(|r| (0..4).all(|c| (a[r][c] - b[r][c]).abs() < 1e-5))
}

#[test]
fn the_object_follows_its_root_body_and_the_bones_are_drawn_relative_to_it() {
    // A frame bone 0 and a leaf bone 1 below it, each making an element.
    let mut c = BoneCallbacks::of(&shell(&[Some(0), Some(1)]));
    assert!(c.from_bones());
    let x = transform([10.0, 2.0, -3.0], [0.3, 0.0, 0.0]);
    let pose = [
        transform([0.0, 1.9, 0.0], [0.0; 3]),
        transform([0.0, 1.3, 0.0], [0.0; 3]),
    ];
    // 8.1, first call: each body at its bone's pose followed by X.
    let placed = c.first_call(&pose, &x);
    assert_eq!(placed.len(), 2);
    assert!(close(&placed[1].1, &multiply(&pose[1], &x)));
    // 8.2: unmoved, the object's transform is X.
    assert!(close(&c.object_transform(&placed[0].1), &x));
    // The root body moved by 1 m along x: the object follows it.
    let moved = transform([1.0, 0.0, 0.0], [0.0; 3]);
    let root = multiply(&placed[0].1, &moved);
    let x2 = c.object_transform(&root);
    assert!(close(&x2, &multiply(&x, &moved)));
    // The leaf swung about y at its origin: its bone's pose relative to the
    // object turns, its origin stays.
    let swing = transform([0.0; 3], [0.0, 0.7, 0.0]);
    let leaf = multiply(&multiply(&swing, &pose[1]), &x2);
    let t = bone_pose(&leaf, &x2);
    assert!(close(&t, &multiply(&swing, &pose[1])));
    assert!((transform_point(&t, [0.0; 3])[1] - 1.3).abs() < 1e-5);
    // The root bone's pose stays as it was.
    assert!(close(&bone_pose(&root, &x2), &pose[0]));
}

#[test]
fn a_type_0_shell_draws_the_object_at_its_elements_frame() {
    let mut c = BoneCallbacks::of(&shell(&[None]));
    assert!(!c.from_bones());
    assert!(c.first_call(&[IDENTITY], &IDENTITY).is_empty());
    let f = transform([1.0, 2.0, 3.0], [0.1, 0.2, 0.3]);
    assert_eq!(c.object_transform(&f), f);
}

#[test]
fn bones_without_a_callback_follow_their_parent() {
    let binds = [
        transform([0.0, 1.0, 0.0], [0.0; 3]),
        transform([0.5, 1.0, 0.0], [0.0; 3]),
        transform([0.5, 1.5, 0.0], [0.0; 3]),
    ];
    // Bone 2 is the child of bone 1, which is the child of bone 0, listed
    // after it.
    let parents = [None, Some(0), Some(1)];
    let mut out = Vec::new();
    rigid_pose(&binds, &parents, |_| None, &mut out);
    assert!((0..3).all(|b| close(&out[b], &binds[b])));
    // Bone 1 follows its body: bone 2 is carried with it.
    let turned = multiply(&transform([0.0; 3], [0.0, 1.0, 0.0]), &binds[1]);
    rigid_pose(&binds, &parents, |b| (b == 1).then_some(turned), &mut out);
    assert!(close(&out[0], &binds[0]));
    assert!(close(&out[1], &turned));
    let local = multiply(&binds[2], &inverse_rigid(&binds[1]));
    assert!(close(&out[2], &multiply(&local, &turned)));
}
