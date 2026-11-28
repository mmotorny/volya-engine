// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use volya_engine_formats::bone_physics::{
    AxisLimits, BonePhysics, BoxShape, Cylinder, JointType, Shape, ShapeType, Sphere, shape_flags,
};
use volya_engine_formats::gamemtl::DYNAMIC;
use volya_engine_formats::ltx;
use volya_engine_formats::model::{Bone, Header, ModelType, transform};

use super::*;

fn physics(kind: ShapeType, flags: u16, cuboid: BoxShape) -> BonePhysics {
    BonePhysics {
        version: 1,
        material: "objects\\large_metal_trash".into(),
        shape: Shape {
            kind,
            flags,
            cuboid,
            sphere: Sphere::default(),
            cylinder: Cylinder::default(),
        },
        joint: JointType::Rigid,
        limits: [AxisLimits::default(); 3],
        spring: 1.0,
        damping: 1.0,
        joint_flags: 0,
        break_force: 0.0,
        break_torque: 0.0,
        friction: None,
        mass: 100.0,
        centre_of_mass: [0.0; 3],
    }
}

fn bone(name: &str, parent: Option<usize>, at: [f32; 3], physics: BonePhysics) -> Bone {
    Bone {
        name: name.into(),
        parent,
        box_rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        box_centre: [0.0; 3],
        box_half_size: [0.0; 3],
        bind_rotation: [0.0; 3],
        bind_position: at,
        physics,
    }
}

/// The trader's door's model as section 1 gives it: `link` (no shape),
/// `door` (the leaf's box, here 1 m above `link`), `lock` (no physics).
fn door_model() -> Model {
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let leaf = BoxShape {
        axes: identity,
        centre: [0.703, 0.0, -0.013],
        half_size: [0.690, 1.322, 0.125],
    };
    Model {
        header: Header {
            kind: 10,
            shader: 0,
            box_min: [0.0; 3],
            box_max: [0.0; 3],
            sphere_centre: [0.0; 3],
            sphere_radius: 0.0,
        },
        kind: ModelType::Skeleton,
        meshes: Vec::new(),
        bones: vec![
            bone("link", None, [0.0; 3], physics(ShapeType::None, 0, leaf)),
            bone(
                "door",
                Some(0),
                [0.0, 1.0, 0.0],
                physics(ShapeType::Box, 0, leaf),
            ),
            bone(
                "lock",
                Some(1),
                [0.5, 0.0, 0.0],
                physics(ShapeType::Box, shape_flags::NO_PHYSICS, leaf),
            ),
        ],
    }
}

fn materials() -> Vec<GameMaterial> {
    vec![
        GameMaterial {
            name: "default".into(),
            ..GameMaterial::default()
        },
        GameMaterial {
            name: "objects\\large_metal_trash".into(),
            flags: DYNAMIC,
            ..GameMaterial::default()
        },
    ]
}

const LOGIC: &[u8] = b"[ph_door@locked]\nlocked = true\nsnd_open_start = trader_door_unlock\n\
    on_info = {+esc_trader_can_leave} ph_door@closed %=play_snd(device\\door_servomotor)%\n\
    [ph_door@closed]\nlocked = false\n[ph_door@tips]\nlocked = true\nshow_tips = false\n\
    tip_open = tip_other\nclosed = false\n";

fn section(name: &str) -> DoorSection {
    let config = ltx::parse(LOGIC).unwrap();
    DoorSection::read(config.section(name).unwrap())
}

#[test]
fn the_logic_starts_in_the_locked_section() {
    assert_eq!(
        start_section("scripts\\ESC_trader_door.ltx"),
        Some("ph_door@locked")
    );
    assert_eq!(start_section("scripts\\esc_trader.ltx"), None);
    assert!(is_door_section("ph_door@locked") && is_door_section("ph_door"));
    assert!(!is_door_section("ph_doors@x") && !is_door_section("sr_idle@ph_door"));
    // 2.1: closed and show_tips default to true, the locked hint id to
    // tip_door_locked.
    assert_eq!(
        section("ph_door@locked"),
        DoorSection {
            name: "ph_door@locked".into(),
            locked: true,
            closed: true,
            show_tips: true,
            locked_tip: LOCKED_TIP.into(),
        }
    );
    let s = section("ph_door@tips");
    assert!(s.locked && !s.closed && !s.show_tips && s.locked_tip == "tip_other");
}

#[test]
fn starting_closes_the_door_holds_it_and_sets_the_hint() {
    let mut door = Door::new("trader_door".into(), section("ph_door@locked"), Vec::new());
    // 2.3: no motor and no hint before the first update.
    assert_eq!(
        (door.motor(), door.tip(), door.started()),
        (None, None, false)
    );
    door.update();
    assert!(door.started());
    assert_eq!(door.motor(), Some(CLOSING));
    assert_eq!(door.tip(), Some("tip_door_locked"));
    // The next physics step finds it closed and holds it with 10000 / 1.
    door.physics_step();
    assert_eq!(door.motor(), Some(HOLDING));
    door.update();
    door.physics_step();
    assert_eq!(door.motor(), Some(HOLDING));
    // Unlocked: no hint; not closed: no motor.
    let mut open = Door::new("d".into(), section("ph_door@closed"), Vec::new());
    open.update();
    assert_eq!((open.motor(), open.tip()), (Some(CLOSING), None));
    let mut quiet = Door::new("d".into(), section("ph_door@tips"), Vec::new());
    quiet.update();
    quiet.physics_step();
    assert_eq!((quiet.motor(), quiet.tip()), (None, None));
}

#[test]
fn the_leaf_box_is_placed_by_its_bone_and_the_record() {
    let model = door_model();
    // The record turned a quarter turn about y and moved.
    let x = transform([10.0, 0.0, 5.0], [0.0, std::f32::consts::FRAC_PI_2, 0.0]);
    let boxes = shell_boxes(&model, &x, "link", &materials()).unwrap();
    assert_eq!(
        boxes.len(),
        1,
        "the no-physics bone and the shapeless one have none"
    );
    let b = boxes[0];
    assert_eq!((b.half_size, b.material), ([0.690, 1.322, 0.125], 1));
    let expected = transform_point(&x, [0.703, 1.0, -0.013]);
    for k in 0..3 {
        assert!((b.centre[k] - expected[k]).abs() < 1e-5, "{:?}", b.centre);
    }
    let y = b.axes[1];
    assert!((y[1] - 1.0).abs() < 1e-6, "{y:?}");
    // A fixed bone the model lacks is fatal.
    let e = shell_boxes(&model, &x, "link,frame", &materials()).unwrap_err();
    assert_eq!(e.to_string(), "wrong fixed bone 'frame'");
}

#[test]
fn the_crosshair_picks_the_leaf() {
    let model = door_model();
    let x = transform([0.0; 3], [0.0; 3]);
    let boxes = shell_boxes(&model, &x, "link", &materials()).unwrap();
    let door = Door::new("trader_door".into(), section("ph_door@locked"), boxes);
    // The leaf spans x 0.013 … 1.393, z −0.138 … 0.112; looking along +z
    // from 1.9 m before its near face.
    let eye = [0.7, 1.5, -2.038];
    let t = door.pick(eye, [0.0, 0.0, 1.0]).unwrap();
    assert!((t - 1.9).abs() < 1e-5, "{t}");
    assert_eq!(door.pick(eye, [0.0, 0.0, -1.0]), None);
    assert_eq!(door.pick([3.0, 1.5, -2.038], [0.0, 0.0, 1.0]), None);
}

/// TODO(#876): the stub's unlock by a use of the locked door, then the
/// opening by the next (4.3).
#[test]
fn a_use_unlocks_the_locked_door_and_the_next_opens_it() {
    let mut door = Door::new("trader_door".into(), section("ph_door@locked"), Vec::new())
        .with_unlocked(section("ph_door@closed"));
    assert_eq!(door.use_action(), Used::Nothing, "not started");
    door.update();
    door.physics_step();
    assert_eq!(door.use_action(), Used::Unlocked);
    assert!(!door.section.locked && door.section.name == "ph_door@closed");
    // Closed again: 200 / 3, then held; the hint tip_door_open.
    assert_eq!(
        (door.motor(), door.tip()),
        (Some(CLOSING), Some("tip_door_open"))
    );
    door.physics_step();
    assert_eq!(door.motor(), Some(HOLDING));
    assert!(!door.is_open());
    assert_eq!(door.use_action(), Used::Opened);
    assert!(door.is_open());
    assert_eq!((door.motor(), door.tip()), (Some(OPENING), None));
    door.physics_step();
    assert_eq!(door.motor(), Some(OPENING));
    assert_eq!(door.use_action(), Used::Nothing);
    // Without a section to switch to, the locked door stays locked.
    let mut locked = Door::new("d".into(), section("ph_door@locked"), Vec::new());
    locked.update();
    assert_eq!(locked.use_action(), Used::Nothing);
    assert_eq!(locked.tip(), Some("tip_door_locked"));
}
