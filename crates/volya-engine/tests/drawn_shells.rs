// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What is drawn of `l01_escape`'s shells at a New Game on the Russian
//! edition's data (BYOA), against docs/behavior/physics-shells.md 7, 8 and
//! 10, item 8: the trader's door drawn at its record's transform, its leaf's
//! bone (and the lock below it) turning with the leaf's body as its hinge's
//! motor opens it; a bucket awake at the start drawn where its body rests.
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::core::input::Input;
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::objects::level_objects;
use volya_engine_formats::model::{Mat4, transform, transform_point};
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn close(a: [f32; 3], b: [f32; 3], e: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < e)
}

fn origin(m: &Mat4) -> [f32; 3] {
    [m[3][0], m[3][1], m[3][2]]
}

/// The render matrix of bone `bone` of instance `k`, as rows (row
/// vectors), if it is posed.
fn render_matrix(level: &LevelScreen, k: usize, bone: usize) -> Option<Mat4> {
    let poses = level.motions().poses();
    let first = poses.first[k]? as usize;
    let m = poses.matrices[first + bone];
    Some(std::array::from_fn(|r| {
        std::array::from_fn(|c| if c < 3 { m[c][r] } else { f32::from(r == 3) })
    }))
}

#[test]
fn shells_are_drawn_from_their_bodies() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();
    let objects = level_objects(&level.simulation().unwrap());
    let named = |name: &str| {
        let o = (objects.iter())
            .find(|o| o.record.header.name.as_str() == name)
            .unwrap_or_else(|| panic!("{name}"));
        let h = &o.record.header;
        (o.id, transform(h.position, h.angles))
    };
    let (door, x) = named("trader_door");
    let k = level.instance_of(door).expect("the door's instance");
    let bone = |name: &str| {
        let objects = &level.world().objects;
        let model = &objects.models[objects.instances[k].model];
        (0..model.bone_count())
            .find(|&b| objects.names.get(model.bone_name(b)) == name)
            .unwrap()
    };
    let (leaf, lock) = (bone("door"), bone("lock"));
    let mut input = Input::new();
    let mut now = 0;
    let mut frames = |level: &mut LevelScreen, n: u32| {
        for _ in 0..n {
            now += 16;
            level.set_global_clock(now);
            level.update(now, &input);
            input.end_frame();
        }
    };
    // 7, 10 item 8: drawn at X, its bones at their bind poses, from the
    // first frame's bone computation.
    frames(&mut level, 1);
    let at = level.placement(k).expect("drawn");
    assert!((0..4).all(|r| close(
        [at[r][0], at[r][1], at[r][2]],
        [x[r][0], x[r][1], x[r][2]],
        1e-4
    )));
    let k0 = render_matrix(&level, k, leaf).expect("the door is posed");
    assert!(close(origin(&k0), [0.0; 3], 1e-4), "{k0:?}");

    // The opening motor (script API Y 5.3; the scheme's open section's
    // values are the scripts'), then two seconds.
    (level.physics_mut())
        .set_max_force_and_velocity(door, "door", 2100.0, -3.0)
        .unwrap();
    frames(&mut level, 120);
    let angle = level.physics().hinge_angle(door, "door").unwrap();
    eprintln!("the door's hinge at {angle} rad after 120 frames");
    assert!(angle.abs() > 0.5, "{angle}");
    // 8.2: the frame's body has not moved: the object's transform is X.
    let at = level.placement(k).unwrap();
    assert!(close(origin(&at), origin(&x), 1e-3), "{at:?}");
    // 8.1: the leaf's bone turned about its vertical hinge by the hinge's
    // angle, its origin (the hinge's anchor) where it was; the lock, with
    // no callback, follows it.
    let kd = render_matrix(&level, k, leaf).unwrap();
    let turned = kd[0][0].clamp(-1.0, 1.0).acos();
    eprintln!("the leaf's bone turned by {turned} rad");
    assert!((turned - angle.abs()).abs() < 0.05, "{turned} vs {angle}");
    assert!((kd[1][1] - 1.0).abs() < 1e-3, "{kd:?}");
    let objects_ = &level.world().objects;
    let model = &objects_.models[objects_.instances[k].model];
    let hinge = origin(&model.bind(leaf));
    assert!(close(transform_point(&kd, hinge), hinge, 1e-3), "{kd:?}");
    let kl = render_matrix(&level, k, lock).unwrap();
    assert!((0..4).all(|r| close(
        [kl[r][0], kl[r][1], kl[r][2]],
        [kd[r][0], kd[r][1], kd[r][2]],
        1e-4
    )));
    // The leaf drawn where its body is (8.2: the blend of the last two
    // steps, within a step's motion).
    let physics = level.physics();
    let (shell, _) = physics.shell_of(door).unwrap();
    let body = shell.frame(physics.world().solver(), 1).unwrap();
    let drawn = transform_point(&at, transform_point(&kd, hinge));
    assert!(close(drawn, origin(&body), 1e-2), "{drawn:?} vs {body:?}");
    drop(physics);

    // A bucket awake at the start fell onto the floor: drawn where its
    // body rests (8.2).
    let (bucket, record) = named("vedro_0007");
    let b = level.instance_of(bucket).unwrap();
    let at = level.placement(b).unwrap();
    let physics = level.physics();
    let (shell, _) = physics.shell_of(bucket).unwrap();
    let root = shell.frame(physics.world().solver(), 0).unwrap();
    let objects_ = &level.world().objects;
    let model = &objects_.models[objects_.instances[b].model];
    let root_bone = origin(&model.bind(0));
    eprintln!(
        "the bucket drawn {:?} from its record",
        [0, 1, 2].map(|i| at[3][i] - record[3][i])
    );
    assert!(close(transform_point(&at, root_bone), origin(&root), 1e-3));
}
