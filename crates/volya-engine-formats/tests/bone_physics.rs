// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Every skeleton model of the Russian edition's archives (`gamedata.db*`,
//! merged) read with its bone physics data: the counts of
//! docs/formats/bone-physics.md sections 1 – 8, its examples, and every
//! bone's material found dynamic in `gamemtl.xr`. Skips without game data.

#![cfg(not(target_arch = "wasm32"))]

#[path = "support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::BufReader;

use volya_engine_formats::bone_physics::{
    self, Axis, JointType, LimitedJoint, ShapeType, shape_flags,
};
use volya_engine_formats::chunks::Chunks;
use volya_engine_formats::db::{Archive, Entry, Scrambling};
use volya_engine_formats::gamemtl;
use volya_engine_formats::model::{self, Model};

#[test]
fn reads_every_models_bone_physics() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let mut opened = Vec::new();
    // Lower-case path → (archive, entry); later archives replace earlier.
    let mut files: BTreeMap<String, (usize, Entry)> = BTreeMap::new();
    for a in archives
        .iter()
        .filter(|a| a.path.starts_with("gamedata.db"))
    {
        let archive = Archive::open(
            BufReader::new(File::open(&a.file).unwrap()),
            Scrambling::for_path(&a.path),
        )
        .unwrap();
        for e in archive.entries().iter().filter(|e| !e.is_dir()) {
            files.insert(e.path.to_lowercase(), (opened.len(), e.clone()));
        }
        opened.push(archive);
    }
    let mut read = |path: &str| {
        let (a, e) = &files[path];
        opened[*a].read(e).unwrap()
    };
    let library = gamemtl::parse(&read("gamemtl.xr")).unwrap();

    let models: Vec<String> = files
        .keys()
        .filter(|p| p.ends_with(".ogf"))
        .cloned()
        .collect();
    let (mut skeletons, mut records, mut used, mut roots_unused) = (0, 0, 0, 0);
    let mut shapes = BTreeMap::new();
    let mut flags = BTreeMap::new();
    let mut joints = BTreeMap::new();
    let mut limited = BTreeMap::new();
    let mut versions = BTreeSet::new();
    let mut materials: BTreeMap<String, usize> = BTreeMap::new();
    let (mut raw_breakable, mut raw_models) = (0, 0);
    let mut kept = BTreeMap::new();
    let mut cleared = Vec::new();
    let (mut friction_0, mut friction_1) = (0, 0);
    let (mut min_mass, mut max_mass) = (f32::INFINITY, 0f32);
    for path in &models {
        let file = read(path);
        let header = Chunks::parse(&file).unwrap();
        let header =
            model::Header::parse(&header.require(1, "model").unwrap().content().unwrap()).unwrap();
        if !matches!(header.kind, 3 | 10) {
            continue;
        }
        skeletons += 1;
        let m = Model::parse(&file).unwrap_or_else(|e| panic!("{path}: {e}"));
        let (_, stored) = model::skeleton(&file).unwrap();
        let root = m.bones.iter().position(|b| b.parent.is_none()).unwrap();
        if m.bones[root].physics.shape.used().is_none() {
            roots_unused += 1;
        }
        let breakable =
            |b: &model::Bone| b.physics.breakable() && b.physics.joint != JointType::None;
        let raw = stored.iter().filter(|b| breakable(b)).count();
        if raw > 0 {
            raw_breakable += raw;
            raw_models += 1;
            if !m.bones.iter().any(|b| b.physics.breakable()) {
                cleared.push(path.clone());
            }
        }
        for b in &m.bones {
            let p = &b.physics;
            records += 1;
            versions.insert(p.version);
            *shapes.entry(p.shape.kind).or_insert(0) += 1;
            for bit in [1, 2, 4] {
                if p.shape.has(bit) {
                    *flags.entry(bit).or_insert(0) += 1;
                }
            }
            if p.shape.used().is_some() {
                used += 1;
            }
            *joints.entry(p.joint).or_insert(0) += 1;
            if let Some(l) = p.limited_joint() {
                *limited.entry(format!("{l:?}")).or_insert(0) += 1;
            }
            if breakable(b) {
                *kept.entry(p.joint).or_insert(0) += 1;
            }
            *materials.entry(p.material.clone()).or_insert(0) += 1;
            match p.friction {
                Some(0.0) => friction_0 += 1,
                Some(1.0) => friction_1 += 1,
                _ => {}
            }
            assert!(p.mass.is_finite() && p.mass > 0.0, "{path} {}", b.name);
            min_mass = min_mass.min(p.mass);
            max_mass = max_mass.max(p.mass);
        }
    }
    let count = |m: &BTreeMap<_, usize>| m.values().sum::<usize>();
    println!(
        "{skeletons} skeleton models, {records} bones; shapes {shapes:?}; flags {flags:?}; \
         joints {joints:?}; type 2 {limited:?}; breakable {raw_breakable} in {raw_models} \
         models, kept {kept:?}, cleared in {cleared:?}; roots without a used shape \
         {roots_unused}; mass {min_mass} … {max_mass}; {} materials",
        materials.len()
    );

    // Section 1.
    assert_eq!((skeletons, records), (1_579, 20_685));
    assert_eq!(versions, BTreeSet::from([1]));
    // Section 2.
    use ShapeType::*;
    assert_eq!(
        shapes,
        BTreeMap::from([
            (None, 7_343),
            (Box, 8_976),
            (Sphere, 2_490),
            (Cylinder, 1_876)
        ])
    );
    assert_eq!(
        flags,
        BTreeMap::from([
            (shape_flags::NO_HIT_MARKS, 474),
            (shape_flags::REMOVABLE, 4_801),
            (shape_flags::NO_PHYSICS, 3_593)
        ])
    );
    assert_eq!(used, 11_327);
    // Section 3.
    assert_eq!(
        joints,
        BTreeMap::from([
            (JointType::Rigid, 12_982),
            (JointType::Ball, 48),
            (JointType::Limited, 2_854),
            (JointType::TwoAxisHinge, 70),
            (JointType::None, 4_710),
            (JointType::Slider, 21),
        ])
    );
    assert_eq!(count(&limited), 2_854);
    let hinge = |a: Axis| format!("{:?}", LimitedJoint::Hinge(a));
    assert_eq!(limited[&hinge(Axis::X)], 76);
    assert_eq!(limited[&hinge(Axis::Y)], 164);
    assert_eq!(limited[&hinge(Axis::Z)], 28 + 13);
    let three = |a: [Axis; 3]| format!("{:?}", LimitedJoint::ThreeAxis(a));
    use Axis::{X, Y, Z};
    // Only z; only y; none or only x.
    assert_eq!(limited[&three([X, Z, Y])], 1_322);
    assert_eq!(limited[&three([Z, Y, X])], 51);
    assert_eq!(limited[&three([Z, X, Y])], 1_081 + 119);
    assert_eq!((raw_breakable, raw_models), (201, 30));
    assert_eq!(
        kept,
        BTreeMap::from([
            (JointType::Rigid, 173),
            (JointType::Ball, 2),
            (JointType::Limited, 7)
        ])
    );
    assert_eq!(
        cleared,
        [
            "meshes\\physics\\decor\\kozel_01.ogf",
            "meshes\\physics\\rope\\krane_box_escape.ogf",
            "meshes\\physics\\vehicles\\niva\\include_test_niva.ogf",
        ]
    );
    // Section 4.
    assert_eq!((friction_0, friction_1), (14_310, 2_581));
    // Section 5.
    assert!(
        min_mass < 8e-5 && max_mass == 8_000.0,
        "{min_mass} {max_mass}"
    );
    // Section 6: 22 names, every one dynamic.
    assert_eq!(materials.len(), 22);
    assert_eq!(materials["objects\\dead_body"], 9_683);
    assert_eq!(materials["default_object"], 2_048);
    for name in materials.keys() {
        bone_physics::material(name, &library).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    // Section 8.
    assert_eq!(roots_unused, 284);

    // The examples of section 3.2.
    let trader = Model::parse(&read("meshes\\physics\\door\\door_trader.ogf")).unwrap();
    let names: Vec<_> = trader.bones.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["link", "door", "lock"]);
    let door = &trader.bones[1].physics;
    assert_eq!(door.limited_joint(), Some(LimitedJoint::Hinge(Axis::Y)));
    let y = door.limits(Axis::Y);
    assert!((y.low - -1.9199).abs() < 1e-4 && y.high == 0.0, "{y:?}");
    let garbage = Model::parse(&read("meshes\\physics\\door\\door_garbage_01.ogf")).unwrap();
    let slider = &garbage.bones[1].physics;
    assert_eq!(slider.joint, JointType::Slider);
    let x = slider.limits(Axis::X);
    assert!((x.low - -2.0).abs() < 1e-4 && x.high == 0.0, "{x:?}");
    let yantar = Model::parse(&read("meshes\\physics\\door\\door_yantar_u_01.ogf")).unwrap();
    let door = &yantar.bones[yantar.bone("door").unwrap()].physics;
    assert_eq!(door.joint, JointType::Slider);
    let (low, high) = door.limits(Axis::X).kept();
    assert!(
        low == -std::f32::consts::PI && (high - 0.4584).abs() < 1e-4,
        "{low} {high}"
    );
    let bandit = Model::parse(&read("meshes\\actors\\bandit\\stalker_bandit_1.ogf")).unwrap();
    let root = bandit.bones.iter().find(|b| b.parent.is_none()).unwrap();
    assert!(root.physics.shape.used().is_none());
}
