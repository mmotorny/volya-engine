// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The shells of `l01_escape`'s objects at a New Game on the Russian
//! edition's data (BYOA), against docs/behavior/physics-shells.md (10, the
//! trader's door) and physics-shell-shapeless-root.md: the door's two
//! bodies, its hinge and masses, resting until its scheme's motor wakes it
//! and held closed after. Measures the heap the level's shells take (the
//! engine's and the library's) and the cost of a physics step with them.
//! Its own test binary, so no other test allocates meanwhile. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]
// A counting allocator and the library's allocation handlers need `unsafe`;
// both only forward to the system allocator.
#![allow(unsafe_code)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

use volya_engine::core::input::Input;
use volya_engine::core::physics::ode::shell::param;
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::level_physics::{LevelPhysics, PhysicsSettings};
use volya_engine::objects::level_objects;
use volya_engine_formats::object::Object;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

struct Counting;

static RUST: AtomicUsize = AtomicUsize::new(0);
static ODE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            RUST.fetch_add(layout.size(), Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        RUST.fetch_sub(layout.size(), Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            RUST.fetch_add(size, Relaxed);
            RUST.fetch_sub(layout.size(), Relaxed);
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

// The library's memory handlers (physics-library.md 3.4): every block it
// allocates, reallocates and frees, with its size.
unsafe extern "C" {
    fn dSetAllocHandler(f: Option<unsafe extern "C" fn(usize) -> *mut c_void>);
    fn dSetReallocHandler(
        f: Option<unsafe extern "C" fn(*mut c_void, usize, usize) -> *mut c_void>,
    );
    fn dSetFreeHandler(f: Option<unsafe extern "C" fn(*mut c_void, usize)>);
}

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size.max(1), 16).unwrap()
}

unsafe extern "C" fn ode_alloc(size: usize) -> *mut c_void {
    ODE.fetch_add(size, Relaxed);
    unsafe { System.alloc(layout(size)) as *mut c_void }
}

unsafe extern "C" fn ode_realloc(p: *mut c_void, old: usize, size: usize) -> *mut c_void {
    ODE.fetch_add(size, Relaxed);
    ODE.fetch_sub(old, Relaxed);
    unsafe { System.realloc(p as *mut u8, layout(old), size.max(1)) as *mut c_void }
}

unsafe extern "C" fn ode_free(p: *mut c_void, size: usize) {
    ODE.fetch_sub(size, Relaxed);
    unsafe { System.dealloc(p as *mut u8, layout(size)) };
}

fn close(a: [f32; 3], b: [f32; 3], e: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < e)
}

#[test]
fn the_bunkers_shells_and_the_trader_door() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    // SAFETY: set before the library allocates anything; the handlers
    // allocate as the library's own defaults would.
    unsafe {
        dSetAllocHandler(Some(ode_alloc));
        dSetReallocHandler(Some(ode_realloc));
        dSetFreeHandler(Some(ode_free));
    }
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();
    let objects = level_objects(level.simulation().unwrap());
    let door = objects
        .iter()
        .find(|o| o.record.header.name.as_str() == "trader_door")
        .expect("the trader's door");
    let Object::PhysicsObject(p) = &door.record.object else {
        panic!("{:?}", door.record.class_id)
    };
    assert_eq!(
        (
            p.physics_type,
            p.fixed_bones.as_str(),
            p.skeleton.flags,
            p.mass
        ),
        (3, "link", 0, 10.0)
    );

    // The level's shells, built as the level loaded.
    let report = level.state().physics;
    eprintln!(
        "{} shells of {} online objects, {} active at the start",
        report.shells,
        objects.iter().filter(|o| o.online).count(),
        report.active
    );
    let physics = level.physics();
    let (shell, active) = physics.shell_of(door.id).expect("the door's shell");
    let solver = physics.world().solver();
    // 10, item 2 (`root` 1.2, 1.3): two bodies, the frame's fixed.
    assert_eq!(shell.elements(), 2);
    assert!(shell.fixed(0) && !shell.fixed(1));
    let frame = shell.body(solver, 0).unwrap();
    let leaf = shell.body(solver, 1).unwrap();
    assert_eq!((frame.mass, frame.gravity), (1e8, false));
    // `root` 6: the leaf has the bone data's mass, not the record's.
    assert!((leaf.mass - 100.0).abs() < 1e-3, "{}", leaf.mass);
    assert!(leaf.gravity);
    // 10, item 3: switched off, resting, its leaf's origin at its centre of
    // mass; the hinge at the door bone's origin.
    assert!(!active && !leaf.enabled && !frame.enabled);
    eprintln!("leaf body at {:?}", leaf.position);
    assert!(
        close(leaf.position, [-248.3419, -23.3206, -134.3756], 1e-3),
        "{:?}",
        leaf.position
    );
    let joint = physics.joint_of(door.id, "door").expect("the hinge");
    let (anchor, axis) = solver.hinge_anchor_axis(joint).unwrap();
    eprintln!("hinge anchor {anchor:?}, axis {axis:?}");
    assert!(
        close(anchor, [-248.3039, -23.3206, -133.7155], 1e-3),
        "{anchor:?}"
    );
    // About the door bone's y axis, unrotated: vertical in the world.
    assert!(axis[1].abs() > 0.999, "{axis:?}");
    let hinge = |n| solver.hinge_param(joint, n).unwrap();
    assert!((hinge(param::LO_STOP) + 1.9199).abs() < 1e-4);
    assert_eq!(hinge(param::HI_STOP), 0.0);
    // `root` 2.3: no motor before the scheme.
    assert_eq!((hinge(param::F_MAX), hinge(param::VEL)), (0.0, 0.0));
    // 10, item 4: the exact solver requested.
    assert!(solver.marked_exact(shell.group()));
    // The player meets the leaf's box (trader-door.md 1, 3).
    let boxes = level.doors().boxes();
    assert_eq!(boxes.len(), 1);
    for (got, want) in boxes[0].half_size.into_iter().zip([0.6902, 1.3225, 0.1254]) {
        assert!((got - want).abs() < 1e-3, "{:?}", boxes[0].half_size);
    }
    let start = boxes[0].centre;

    // The scheme's start wakes it (`root` 3.1); held closed after.
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
    frames(&mut level, 120);
    let timers = level.take_physics_timers();
    let physics = level.physics();
    let (shell, active) = physics.shell_of(door.id).unwrap();
    let solver = physics.world().solver();
    let joint = physics.joint_of(door.id, "door").unwrap();
    assert!(active);
    assert_eq!(
        (
            solver.hinge_param(joint, param::F_MAX),
            solver.hinge_param(joint, param::VEL)
        ),
        (Some(10_000.0), Some(1.0))
    );
    let angle = solver.hinge_angle(joint).unwrap();
    let leaf_after = shell.body(solver, 1).unwrap();
    let moved = boxes_moved(&level, start);
    eprintln!(
        "after 120 frames: angle {angle:e}, leaf body moved {:?}, box moved {moved:.2e} m",
        [0, 1, 2].map(|k| leaf_after.position[k] - leaf.position[k])
    );
    assert!(angle.abs() < 1e-3, "{angle}");
    assert!(moved < 1e-3, "{moved}");
    eprintln!(
        "level physics over 120 frames: {} steps, {:.3} ms each (collider {:.3}, solver {:.3}), {} shells active",
        timers.steps,
        timers.physics / f64::from(timers.steps.max(1)),
        timers.collider / f64::from(timers.steps.max(1)),
        timers.solver / f64::from(timers.steps.max(1)),
        level.state().physics.active
    );

    // The bunker's shells again in a world of their own: their heap, and a
    // step's cost (meaningful in an optimised build).
    let (rust, ode) = (RUST.load(Relaxed), ODE.load(Relaxed));
    let mut own = LevelPhysics::new(PhysicsSettings::default(), level.collision().bbox_min[1]);
    // Their contacts with the level's mesh (physics-contacts.md 3).
    own.set_level(level.shared_collision());
    let materials = level.collision().materials();
    let dir = data.aliases.path("$game_levels$", "l01_escape").unwrap();
    own.load_shells(&tree, &data, &dir, &objects, materials)
        .unwrap();
    let (rust, ode) = (
        RUST.load(Relaxed) as f64 - rust as f64,
        ODE.load(Relaxed) as f64 - ode as f64,
    );
    let r = own.report();
    eprintln!(
        "shells: {} ({} active); heap kept {:.1} KB of the engine, {:.1} KB of the library",
        r.shells,
        r.active,
        rust / 1e3,
        ode / 1e3
    );
    let cost = |own: &mut LevelPhysics, steps: u32| {
        let t = Instant::now();
        for _ in 0..steps {
            own.frame(0.01, 0);
        }
        t.elapsed().as_secs_f64() * 1e3 / f64::from(steps)
    };
    let first = cost(&mut own, 100);
    let active_after = own.report().active;
    let later = cost(&mut own, 1000);
    eprintln!(
        "a step: {first:.4} ms over the first 100 ({active_after} active after), {later:.4} ms over the next 1000 ({} active)",
        own.report().active
    );
    // The two awake at the start are buckets whose bodies are cylinders,
    // which get no contacts yet (TODO(#1010)): `vedro_0001` falls out of
    // the world; `vedro_0007`'s handle, a box on a body of its own, meets
    // the floor while the bucket falls on, which tears their joint apart
    // and throws the shell far off, where it meets nothing and stays
    // active.
    let name = |id| {
        let o = objects.iter().find(|o| o.id == id).unwrap();
        o.record.header.name.as_str().to_owned()
    };
    let active: Vec<String> = (objects.iter())
        .filter(|o| own.shell_of(o.id).is_some_and(|(_, a)| a))
        .map(|o| name(o.id))
        .collect();
    assert_eq!(active, ["vedro_0007"]);
}

/// How far the leaf's box the player meets moved from `start`, m.
fn boxes_moved(level: &LevelScreen, start: [f32; 3]) -> f32 {
    let c = level.doors().boxes()[0].centre;
    (0..3)
        .map(|k| (c[k] - start[k]).powi(2))
        .sum::<f32>()
        .sqrt()
}
