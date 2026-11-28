// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's body against a movable box in a world over the library
//! (physics-contacts.md 1, 2, 5, 7).

use super::*;
use crate::collision::body::Shapes;
use crate::physics::{LIBRARY_ITERATIONS, World};
use volya_engine_formats::gamemtl::GameMaterial;

/// A box on a body of its own, as a shell's leaf.
struct Leaf {
    group: GroupId,
    body: BodyId,
    half: Vec3,
    woken: bool,
}

impl Leaf {
    fn geom(&self, solver: &OdeSolver) -> Option<Geom> {
        Some(Geom {
            shape: Shape::Box {
                axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                centre: solver.position(self.body)?,
                half_size: self.half,
            },
            body: Some(self.body),
            material: 1,
            meets_level: false,
        })
    }
}

enum Thing {
    Leaf(Leaf, Option<Bounds>),
    Player(OdeCharacter),
}

impl Grouped for Thing {
    fn group(&self) -> GroupId {
        match self {
            Self::Leaf(l, _) => l.group,
            Self::Player(p) => p.group(),
        }
    }
    fn after_solve(&mut self, solver: &OdeSolver) {
        match self {
            Self::Leaf(l, b) => {
                *b = l
                    .geom(solver)
                    .map(|g| crate::physics::contacts::bounds(&g.shape))
            }
            Self::Player(p) => p.after_solve(solver),
        }
    }
    fn class(&self) -> Class {
        match self {
            Self::Leaf(..) => Class {
                word: 0x4,
                group: 0,
            },
            Self::Player(p) => p.class(),
        }
    }
    fn bounds(&self) -> Option<Bounds> {
        match self {
            Self::Leaf(_, b) => *b,
            Self::Player(p) => p.bounds(),
        }
    }
    fn geometries(&self, solver: &OdeSolver, out: &mut Vec<Geom>) {
        match self {
            Self::Leaf(l, _) => out.extend(l.geom(solver)),
            Self::Player(p) => p.geometries(solver, out),
        }
    }
    fn geometry_contact(&mut self, geom: usize, first: bool, d: &mut Draft) {
        if let Self::Player(p) = self {
            p.geometry_contact(geom, first, d);
        }
    }
    fn object_contact(&mut self, geom: usize, first: bool, d: &mut Draft) {
        if let Self::Player(p) = self {
            p.object_contact(geom, first, d);
        }
    }
    fn wake(&mut self, _: &OdeSolver) {
        if let Self::Leaf(l, _) = self {
            l.woken = true;
        }
    }
}

impl Object for Thing {
    fn kind(&self) -> Kind {
        match self {
            Self::Leaf(..) => Kind::Shell,
            Self::Player(_) => Kind::Character,
        }
    }
    fn has_geometry(&self) -> bool {
        true
    }
    fn height(&self) -> f32 {
        0.0
    }
    fn set_gravity(&mut self, _: bool) {}
    fn add_force(&mut self, _: Vec3) {}
    fn pre_step(&mut self, _: &Params) {}
    fn post_step(&mut self, _: &Params) -> bool {
        false
    }
}

fn materials() -> Vec<GameMaterial> {
    let m = |friction, spring| GameMaterial {
        friction,
        damping: 1.0,
        spring,
        ..GameMaterial::default()
    };
    // 0: the player's (`creatures\actor`'s factors), 1: the leaf's.
    vec![m(0.8, 0.8), m(1.0, 1.0)]
}

fn shapes() -> Shapes {
    Shapes::of([0.35, 0.9, 0.35])
}

/// The player's movement step: its centre at `x`, moving at `vx` along x.
fn step(x: f32, vx: f32) -> WorldStep {
    WorldStep {
        centre: [x, 0.35, 0.0],
        velocity: [vx, 0.0, 0.0],
        shapes: shapes(),
        mass: 80.0,
        material: 0,
        moving: true,
        direction: [1.0, 0.0, 0.0],
        step_assist: false,
        replacement: false,
        blocked: false,
        traction: 1.0,
        climbing: false,
    }
}

struct Setup {
    world: World<Thing, OdeSolver>,
    leaf: crate::physics::ObjectId,
    player: crate::physics::ObjectId,
    body: BodyId,
}

/// A leaf of 100 kg (half sizes 0.1, 1.3, 0.7) with its near face at
/// x = 0.5, resting (not active) or not; the player's body active.
fn setup(active_leaf: bool) -> Setup {
    let mut solver = OdeSolver::new();
    solver.set_materials(materials());
    let group = solver.add_group();
    let body = solver
        .add_sphere(group, [0.6, 1.3, 0.0], 100.0, 0.5)
        .unwrap();
    solver.set_gravity(body, false);
    let mut world = World::new(solver, 0.01, LIBRARY_ITERATIONS, -100.0);
    let character = world.solver_mut().add_character(80.0);
    let player = world.add(Thing::Player(character));
    world.activate(player).unwrap();
    let mut leaf = Leaf {
        group,
        body,
        half: [0.1, 1.3, 0.7],
        woken: false,
    };
    let b = leaf
        .geom(world.solver())
        .map(|g| crate::physics::contacts::bounds(&g.shape));
    leaf.woken = false;
    let leaf = world.add(Thing::Leaf(leaf, b));
    if active_leaf {
        world.activate(leaf).unwrap();
    }
    Setup {
        world,
        leaf,
        player,
        body,
    }
}

fn run(s: &mut Setup, step: WorldStep) -> (Vec3, bool) {
    let r = react(s, step);
    (r.velocity, r.blocked)
}

fn react(s: &mut Setup, step: WorldStep) -> Reaction {
    let (p, solver) = s.world.with_solver(s.player);
    let Some(Thing::Player(p)) = p else {
        unreachable!()
    };
    p.set_step(solver, step);
    s.world.step(0, 10);
    let Some(Thing::Player(p)) = s.world.get_mut(s.player) else {
        unreachable!()
    };
    p.take_reaction()
}

/// 1.2, 4.2, 4.3, 7: the player's shapes pressing into the resting leaf
/// wake it, push it along +x and are pushed back.
#[test]
fn the_player_pushes_and_wakes_the_leaf() {
    let mut s = setup(false);
    assert!(!s.world.is_active(s.leaf));
    // The bottom sphere (r 0.35) and torso reach x = 0.51 at 0.16.
    let (dv, blocked) = run(&mut s, step(0.16, 1.0));
    assert!(s.world.is_active(s.leaf), "woken by a kept contact");
    let Some(Thing::Leaf(l, _)) = s.world.get(s.leaf) else {
        unreachable!()
    };
    assert!(l.woken);
    // The outer sphere (radius 0.7 at 0.875 above the centre) overlaps it.
    assert!(blocked);
    assert!(dv[0] < -0.1, "pushed back: {dv:?}");
    let v = s.world.solver().linear_velocity(s.body).unwrap();
    assert!(v[0] > 0.05, "the leaf pushed: {v:?}");
    // Momentum passes between the two (the contacts' softness aside).
    let p = 80.0 * dv[0] + 100.0 * v[0];
    assert!(p.abs() < 2.0, "{p}");
}

/// 1.1: away from the leaf, no contacts; the outer sphere alone only sets
/// "blocked" and does not wake it (physics-contacts.md 1.2, 7 item 5).
#[test]
fn the_outer_sphere_only_blocks() {
    let mut s = setup(false);
    let (dv, blocked) = run(&mut s, step(-3.0, 1.0));
    assert_eq!((dv, blocked), ([0.0; 3], false));
    // At 0: the outer sphere's reach (0.7) meets the face at 0.5, the
    // shapes' (0.35, torso and head 0.29) do not.
    let (dv, blocked) = run(&mut s, step(0.0, 1.0));
    assert!(blocked);
    assert_eq!(dv, [0.0; 3]);
    assert!(!s.world.is_active(s.leaf));
}

/// step-assistance-movable-bodies.md 2.1, 3.3: every contact with the leaf
/// sets the touch flag, the outer sphere's alone too; only those the rules
/// handle as made are counted.
#[test]
fn contacts_with_the_leaf_set_the_touch_flag() {
    let mut s = setup(false);
    let r = react(&mut s, step(-3.0, 1.0));
    assert!(!r.touch && r.made == 0);
    let r = react(&mut s, step(0.0, 1.0));
    assert!(r.touch && r.blocked, "{r:?}");
    assert_eq!(r.made, 0, "the outer sphere's are not counted");
    let r = react(&mut s, step(0.16, 1.0));
    assert!(r.touch && r.made > 0, "{r:?}");
    let r = react(&mut s, step(-3.0, 1.0));
    assert!(!r.touch && r.made == 0, "taken: {r:?}");
}

/// level-cform.md 6.3 with step-assistance-movable-bodies.md 4: the step
/// replacement acts on a movable body's contacts as on the level's (here
/// with "blocked" clear so far): the bottom sphere's contact with the
/// leaf's face, at its centre's height and against the movement, is pushed
/// straight up; with "blocked" set it is kept.
#[test]
fn the_replacement_acts_on_the_leaf() {
    let assisted = |blocked| WorldStep {
        replacement: true,
        blocked,
        step_assist: true,
        ..step(0.16, 1.0)
    };
    let mut s = setup(false);
    let r = react(&mut s, assisted(false));
    assert!(r.made > 0);
    // The outer sphere's contact sets "blocked" too, which keeps the
    // contacts handled after it in the step unreplaced; the bottom
    // sphere's comes first (the player's shapes' order).
    assert!(r.velocity[1] > 0.1, "pushed up: {r:?}");
    let mut s = setup(false);
    let r = react(&mut s, assisted(true));
    assert!(r.velocity[1].abs() < 0.05 && r.velocity[0] < -0.1, "{r:?}");
}

/// 1.2: an active leaf later in the active objects is collided in the
/// player's pass once per step: its contacts are the same as at rest.
#[test]
fn an_active_leaf_is_met_once() {
    let mut s = setup(true);
    let (dv, _) = run(&mut s, step(0.16, 1.0));
    let mut r = setup(false);
    let (dv2, _) = run(&mut r, step(0.16, 1.0));
    assert!((dv[0] - dv2[0]).abs() < 1e-4, "{dv:?} {dv2:?}");
}

/// 4.2: a pair's contacts point from the second geometry towards the
/// first, either way round; cylinders get none yet (TODO(#1010)).
#[test]
fn pair_normals_point_to_the_first() {
    let mut s = OdeSolver::new();
    let sphere = Shape::Sphere {
        centre: [0.0, 0.45, 0.0],
        radius: 0.5,
    };
    let floor = Shape::Box {
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        centre: [0.0, -1.0, 0.0],
        half_size: [2.0, 1.0, 2.0],
    };
    let cyl = Shape::Cylinder {
        centre: [0.0, 0.48, 0.0],
        direction: [0.0, 1.0, 0.0],
        height: 1.0,
        radius: 0.3,
    };
    let mut out = Vec::new();
    for (a, b, up) in [(&sphere, &floor, 1.0), (&floor, &sphere, -1.0)] {
        s.pair_contacts(a, b, 809, &mut out);
        assert!(!out.is_empty(), "{a:?} {b:?}");
        for c in &out {
            assert!((c.normal[1] - up).abs() < 1e-5, "{c:?}");
            assert!(c.depth > 0.0 && c.depth < 0.06, "{c:?}");
        }
    }
    for (a, b) in [
        (&cyl, &floor),
        (&floor, &cyl),
        (&cyl, &sphere),
        (&cyl, &cyl),
    ] {
        s.pair_contacts(a, b, 809, &mut out);
        assert!(out.is_empty());
    }
}
