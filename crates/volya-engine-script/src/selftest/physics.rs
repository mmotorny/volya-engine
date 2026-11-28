// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-physics-vehicles-helicopter-and-lamps.md on
//! every target, one check per section, over a synthetic door's shell
//! activated in the engine's physics world ([`OdeWorld`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::physics::ode::OdeSolver;
use volya_engine_core::physics::ode::shell::{OdeShell, param as library};
use volya_engine_core::physics::shell::{
    BoneState, PhysicsRecord, Shell, ShellModel, physics_object,
};
use volya_engine_core::physics::{INITIAL_STEP, LIBRARY_ITERATIONS, ObjectId, World};
use volya_engine_formats::bone_physics::{
    AxisLimits, BonePhysics, BoxShape, Cylinder, JointType, Shape as BoneShape, ShapeType, Sphere,
    shape_flags,
};
use volya_engine_formats::gamemtl::{DYNAMIC, GameMaterial};
use volya_engine_formats::ltx;
use volya_engine_formats::model::{Bone, Header, transform};

use super::classes::PRELUDE;
use super::ensure;
use super::files::{FSGAME, Tree};
use crate::Value;
use crate::files::FileSystem;
use crate::game_object::physics::{
    Body, PhysicsEvent, PhysicsShell, PhysicsState, PhysicsWorld, Vec3,
};
use crate::game_object::vehicles::CarWeapon;
use crate::game_object::{GameObjects, ObjectClass, SpawnData};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::class_id;
use crate::services::EngineServices;

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 game_object members", game_object_members),
    ("3 physics_shell", shell),
    ("4 physics_element", element),
    ("5 physics_joint", joint),
    ("6, 7 holder and CCar", car),
    ("8 CHelicopter", helicopter),
    ("9 hanging_lamp", lamp),
    ("11 the door scheme", door_scheme),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("physics {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// The engine's physics world over shells activated in it: what the
/// engine's level physics gives the members (TODO(#1056)), for checks.
pub struct OdeWorld {
    pub world: World<OdeShell, OdeSolver>,
    pub shells: HashMap<u16, ObjectId>,
}

impl Default for OdeWorld {
    fn default() -> Self {
        OdeWorld {
            world: World::new(OdeSolver::new(), INITIAL_STEP, LIBRARY_ITERATIONS, -100.0),
            shells: HashMap::new(),
        }
    }
}

impl OdeWorld {
    /// Activates object `object`'s shell (`shells` 2.1): active unless
    /// switched off.
    pub fn add(&mut self, object: u16, shell: &Shell) {
        let params = *self.world.params();
        let body = self.world.solver_mut().activate_shell(&params, shell);
        let id = self.world.add(body);
        if !shell.switched_off {
            self.world.activate(id).expect("the shell has geometry");
        }
        self.shells.insert(object, id);
    }

    fn shell(&self, object: u16) -> Option<&OdeShell> {
        self.world.get(*self.shells.get(&object)?)
    }

    fn with(&mut self, object: u16, f: impl FnOnce(&mut OdeShell, &mut OdeSolver)) {
        let Some(&id) = self.shells.get(&object) else {
            return;
        };
        if let (Some(s), solver) = self.world.with_solver(id) {
            f(s, solver);
        }
    }

    fn hinge(&self, object: u16, joint: usize) -> Option<volya_engine_core::physics::ode::JointId> {
        self.shell(object)?
            .joints()
            .find(|(j, _)| *j == joint)
            .map(|(_, id)| id)
    }
}

/// [`OdeWorld`] shared with the check that steps it.
#[derive(Clone, Default)]
pub struct SharedWorld(pub Rc<RefCell<OdeWorld>>);

impl PhysicsWorld for SharedWorld {
    fn step(&self) -> f32 {
        self.0.borrow().world.params().step
    }

    fn has_shell(&self, object: u16) -> bool {
        self.0.borrow().shells.contains_key(&object)
    }

    fn body(&self, object: u16, element: usize) -> Option<Body> {
        let w = self.0.borrow();
        let s = w.shell(object)?;
        let solver = w.world.solver();
        let b = s.body(solver, element)?;
        Some(Body {
            linear: b.linear,
            angular: b.angular,
            switched_on: b.enabled,
            frame: s.frame(solver, element)?,
        })
    }

    fn apply_force(&mut self, object: u16, force: Vec3) {
        self.0
            .borrow_mut()
            .with(object, |s, solver| s.apply_force(solver, force));
    }

    fn apply_element_force(&mut self, object: u16, element: usize, force: Vec3) {
        self.0.borrow_mut().with(object, |s, solver| {
            s.apply_element_force(solver, element, force)
        });
    }

    fn wake(&mut self, object: u16) {
        let mut w = self.0.borrow_mut();
        w.with(object, |s, solver| s.wake(solver));
        if let Some(&id) = w.shells.get(&object) {
            w.world.activate(id).expect("the shell has geometry");
        }
    }

    fn fix(&mut self, object: u16, element: usize) {
        self.0
            .borrow_mut()
            .with(object, |s, solver| s.fix_element(solver, element));
    }

    fn release_fixed(&mut self, object: u16, element: usize) {
        self.0
            .borrow_mut()
            .with(object, |s, solver| s.release_fixed(solver, element));
    }

    fn constant_force(&mut self, object: u16, direction: Vec3, size: f32, ms: u32) {
        let mut w = self.0.borrow_mut();
        if let Some(&id) = w.shells.get(&object) {
            w.world.constant_force(id, direction, size, ms);
        }
    }

    fn joint_made(&self, object: u16, joint: usize) -> bool {
        self.0.borrow().hinge(object, joint).is_some()
    }

    fn set_joint_param(&mut self, object: u16, joint: usize, parameter: i32, value: f32) {
        let mut w = self.0.borrow_mut();
        // Only hinges are made: their one axis's parameters.
        if let Some(id) = w.hinge(object, joint).filter(|_| parameter < 256) {
            w.world.solver_mut().set_hinge_param(id, parameter, value);
        }
    }

    fn joint_angle(&self, object: u16, joint: usize, _: i32) -> Option<f32> {
        let w = self.0.borrow();
        w.world.solver().hinge_angle(w.hinge(object, joint)?)
    }

    fn joint_axis(&self, object: u16, joint: usize, _: i32) -> Option<Vec3> {
        let w = self.0.borrow();
        let (_, axis) = w
            .world
            .solver()
            .hinge_anchor_axis(w.hinge(object, joint)?)?;
        Some(axis)
    }

    fn joint_anchor(&self, object: u16, joint: usize) -> Option<Vec3> {
        let w = self.0.borrow();
        let (anchor, _) = w
            .world
            .solver()
            .hinge_anchor_axis(w.hinge(object, joint)?)?;
        Some(anchor)
    }
}

const IDENTITY3: [Vec3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn bone_physics(kind: ShapeType, joint: JointType, mass: f32) -> BonePhysics {
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
                centre: [0.0; 3],
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
        centre_of_mass: [0.5, 0.0, 0.0],
    }
}

fn bone(name: &str, parent: Option<usize>, at: Vec3, physics: BonePhysics) -> Bone {
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

/// A door like the trader's (`joints` 3): `link` (root, no shape), `door`
/// (a box of 40 kg hinged about its y axis, limits −1.9199 … 0), `lock` (no
/// physics).
pub fn door_model() -> ShellModel {
    let mut door = bone_physics(ShapeType::Box, JointType::Limited, 40.0);
    door.limits[1] = AxisLimits {
        low: -1.9199,
        high: 0.0,
        spring_factor: 1.0,
        damping_factor: 1.0,
    };
    let mut lock = bone_physics(ShapeType::Box, JointType::Rigid, 1.0);
    lock.shape.flags = shape_flags::NO_PHYSICS;
    ShellModel {
        header: Header {
            kind: 10,
            shader: 0,
            box_min: [-1.0, 0.0, -0.5],
            box_max: [1.0, 2.0, 0.5],
            sphere_centre: [0.0; 3],
            sphere_radius: 0.0,
        },
        bones: vec![
            bone(
                "link",
                None,
                [0.0; 3],
                bone_physics(ShapeType::None, JointType::Rigid, 1.0),
            ),
            bone("door", Some(0), [0.0, 1.0, 0.0], door),
            bone("lock", Some(1), [1.0, 0.0, 0.0], lock),
        ],
        user_data: None,
    }
}

fn materials() -> Vec<GameMaterial> {
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

/// The door's shell as a physics object of type 3 at `position`,
/// switched off (`resting` 1).
pub fn door_shell(model: &ShellModel, position: Vec3) -> Result<Shell, String> {
    let record = PhysicsRecord {
        physics_type: 3,
        mass: 10.0,
        fixed_bones: "",
        skeleton_flags: 0,
        startup_animation: "",
        custom_data: None,
        transform: transform(position, [0.0; 3]),
        bone_state: BoneState::default(),
    };
    physics_object(model, &record, &materials())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no shell".to_owned())
}

const PLAYER: u16 = 0;
const DOOR: u16 = 90;
const CAR: u16 = 91;
const HELICOPTER: u16 = 92;
const LAMP: u16 = 93;
const GRENADE: u16 = 94;
const HELD_GRENADE: u16 = 95;
const STALKER: u16 = 10;

const SYSTEM: &str = "[helicopter]\r\nvelocity = 22\r\non_point_range_dist = 5\r\n\
    path_linear_acc_fw = 6\r\npath_linear_acc_bk = 4\r\nsafe_altitude = 30\r\n\
    use_rocket = true\r\nuse_mgun = false\r\nmin_rocket_attack_dist = 50\r\n\
    max_rocket_attack_dist = 300\r\nmin_mgun_attack_dist = 10\r\nmax_mgun_attack_dist = 200\r\n\
    time_between_rocket_attack = 2000\r\nsyncronize_rocket = true\r\nfire_trail_length = 12\r\n\
    use_fire_trail = true\r\nfire_dispersion_null = 2\r\nfire_dispersion_base = 10\r\n\
    barrel_dir_tolerance = 3\r\nsmoke_particle = vehiclefx\\smoke\r\nbroken_snd = heli\\broken\r\n";

/// A test state: the scripts with the game object basics and this spec
/// registered, `_G` loaded, a global `obj(id)` giving handles, the engine
/// objects and the physics world with the door's shell.
struct State {
    s: Scripts,
    objects: Rc<RefCell<GameObjects>>,
    services: Rc<RefCell<EngineServices>>,
    physics: Rc<RefCell<PhysicsState>>,
    world: SharedWorld,
}

fn data(id: u16, name: &str, section: &str, class: &str) -> SpawnData {
    SpawnData {
        id,
        name: name.as_bytes().to_vec(),
        section: section.as_bytes().to_vec(),
        class_id: class_id(class.as_bytes()),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: None,
    }
}

fn state() -> Result<State, String> {
    let tree = Rc::new(Tree(
        [("gamedata/scripts/_g.script", "")]
            .iter()
            .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
            .collect(),
    ));
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(|e| e.message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(SYSTEM.as_bytes()).map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(|e| e.message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .map_err(|e| e.message)?;
    s.register_physics_vehicles_and_lamps()
        .map_err(|e| e.message)?;
    let shared = Rc::clone(&objects);
    s.lua_mut()
        .register_object_function("obj", move |args| {
            let Some(Value::Number(id)) = args.first() else {
                return Ok(None);
            };
            Ok(shared.borrow().get(*id as u16).and_then(|o| o.handle()))
        })
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    let physics = s.physics_state().map_err(|e| e.message)?;

    s.spawn_engine_object(
        ObjectClass::Player,
        data(PLAYER, "actor", "actor", "O_ACTOR"),
    )
    .map_err(|e| e.to_string())?;
    s.spawn_engine_object(
        ObjectClass::Named("CAI_Stalker"),
        data(STALKER, "wolf", "stalker", "AI_STL_S"),
    )
    .map_err(|e| e.to_string())?;
    let spawned = [
        (
            "CPhysicObject",
            data(DOOR, "trader_door", "physic_object", "O_PHYS_S"),
        ),
        ("CCar", data(CAR, "niva", "car_niva", "SCRPTCAR")),
        (
            "CHelicopter",
            data(HELICOPTER, "heli", "helicopter", "C_HLCP_S"),
        ),
        (
            "hanging_lamp",
            data(LAMP, "lamp", "hanging_lamp", "O_HLAMP"),
        ),
        ("CF1", data(GRENADE, "f1", "grenade_f1", "G_F1")),
    ];
    for (class, mut d) in spawned {
        if d.id == DOOR {
            d.placement[3] = [4.0, 5.0, 6.0, 1.0];
        }
        if d.id == HELICOPTER {
            d.placement[3] = [0.0, 50.0, 0.0, 1.0];
        }
        // Kept in `clients` while the object lives.
        let code = format!(
            "clients = clients or {{}} clients[{id}] = {class}() return clients[{id}]",
            id = d.id
        );
        let client = s.exec("=client", code.as_bytes()).map_err(|e| e.message)?;
        let [Value::Object(client)] = client.as_slice() else {
            return Err(format!("{class}() gave {client:?}"));
        };
        if !s.spawn(*client, d).map_err(|e| e.to_string())? {
            return Err(format!("{class} didn't spawn"));
        }
    }
    let mut held = data(HELD_GRENADE, "f1b", "grenade_f1", "G_F1");
    held.parent = Some(STALKER);
    s.spawn_engine_object(ObjectClass::Named("CF1"), held)
        .map_err(|e| e.to_string())?;

    let world = SharedWorld::default();
    let model = door_model();
    let shell = door_shell(&model, [4.0, 5.0, 6.0])?;
    world.0.borrow_mut().add(DOOR, &shell);
    {
        let mut p = physics.borrow_mut();
        let h = world.0.borrow().world.params().step;
        p.shells
            .insert(DOOR, PhysicsShell::of_shell(&shell, Some(&model), h));
        p.world = Some(Box::new(world.clone()));
    }
    Ok(State {
        s,
        objects,
        services,
        physics,
        world,
    })
}

fn run(s: &mut Scripts, name: &str, body: &str) -> Result<(), String> {
    let code = format!("{PRELUDE}{body}\nreturn failures");
    let results = s
        .exec(&format!("={name}"), code.as_bytes())
        .map_err(|e| format!("{name}: {e}"))?;
    super::classes::failures_of(name, &results)
}

/// Runs `code`, which must stop the state with a fatal error (a failed
/// check, or a crash the spec believes); `what` names it.
fn fatal(s: &mut Scripts, code: &str, what: &str) -> Result<(), String> {
    let first = s.exec("=fatal", code.as_bytes());
    let after = s.exec("=after", b"return 1");
    ensure(
        first
            .as_ref()
            .is_err_and(|e| e.message.contains("fatal error"))
            && after.is_err(),
        || format!("{what}: {first:?}, then {after:?}"),
    )
}

/// Steps the world `n` frames of 0.02 s.
fn step(world: &SharedWorld, n: u32) {
    let mut w = world.0.borrow_mut();
    for i in 0..n {
        w.world.frame(0.02, u64::from(i) * 20);
    }
}

/// 1.2: the classes, without constructors for the physics ones, and the
/// constants.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "1",
        r#"
for _, n in ipairs({"physics_shell", "physics_element", "physics_joint", "holder", "CCar",
    "CHelicopter", "hanging_lamp"}) do
  eq(_G[n] ~= nil, true, n)
end
raises(function() return physics_shell() end, "", "physics_shell has no constructor")
eq(CCar.eWpnDesiredDir, 1, "eWpnDesiredDir")
eq(CCar.eWpnToDefaultDir, 6, "eWpnToDefaultDir")
eq(CHelicopter.eDead, 1, "eDead")
eq(CHelicopter.eMovTakeOff, 5, "eMovTakeOff")
eq(CHelicopter.eEnemyEntity, 2, "eEnemyEntity")
eq(CHelicopter.eBodyToPoint, 1, "eBodyToPoint")
eq(type(obj(91):get_car().id), "nil", "CCar's members are CGameObject's, not game_object's")
eq(type(obj(91):get_car().Visual), "function", "CCar derives from CGameObject")
"#,
    )
}

/// 2.
pub fn game_object_members() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "2",
        r#"
eq(obj(10):get_physics_shell(), nil, "a stalker without a shell")
eq(obj(91):get_physics_shell(), nil, "a car without a shell")
eq(type(obj(90):get_physics_shell()), "userdata", "the door's shell")
eq(obj(90):get_car(), nil, "the door is no car")
eq(obj(91):get_car(), clients[91], "the car as a CCar")
eq(obj(92):get_helicopter(), clients[92], "the helicopter")
eq(obj(91):get_helicopter(), nil, "the car is no helicopter")
eq(obj(93):get_hanging_lamp(), clients[93], "the lamp")
eq(obj(90):get_hanging_lamp(), nil, "the door is no lamp")
eq(type(obj(91):get_holder_class()), "userdata", "the car is a holder")
eq(obj(92):get_holder_class(), nil, "the helicopter is no holder")
eq(obj(0):get_current_holder(), nil, "the player in no vehicle")
obj(94):explode(0)
obj(95):explode(0)
obj(90):explode(0)
"#,
    )?;
    let events = t.physics.borrow_mut().take_events();
    ensure(
        events
            == [PhysicsEvent::Explode {
                object: GRENADE,
                initiator: GRENADE,
                position: [0.0; 3],
                normal: [0.0, 1.0, 0.0],
            }],
        || format!("2.1: only the grenade without a parent explodes: {events:?}"),
    )?;
    t.physics.borrow_mut().current_holder = Some(CAR);
    run(
        &mut t.s,
        "2 holder",
        r#"
eq(obj(0):get_current_holder():engaged(), false, "the player's holder")
eq(obj(10):get_current_holder(), nil, "only the player's object has one")
obj(10):set_const_force(vector():set(0, 1, 0), 1, 100)
obj(91):set_const_force(vector():set(0, 1, 0), 1, 100)
obj(90):set_const_force(vector():set(0, 0, 2), 400, 100)
"#,
    )?;
    // 2.2: the force acts while the shell is active; the door is switched
    // off, so it acts on nothing.
    step(&t.world, 5);
    let still = t.world.body(DOOR, 1).map(|b| b.linear);
    ensure(still == Some([0.0; 3]), || {
        format!("2.2: a switched-off shell isn't pushed: {still:?}")
    })?;
    t.s.exec(
        "=wake",
        b"obj(90):get_physics_shell():get_joint_by_order(0):set_max_force_and_velocity(0, 0, 0)",
    )
    .map_err(|e| e.message)?;
    t.s.exec(
        "=push",
        b"obj(90):set_const_force(vector():set(0, 0, 2), 400, 100)",
    )
    .map_err(|e| e.message)?;
    step(&t.world, 2);
    let moving = t.world.body(DOOR, 1).map(|b| b.linear);
    ensure(moving.is_some_and(|v| v[2] > 0.0), || {
        format!("2.2: the force pushes the woken door along z: {moving:?}")
    })
}

/// 3.
pub fn shell() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "3",
        r#"
local s = obj(90):get_physics_shell()
eq(s:get_elements_number(), 2, "elements")
eq(s:get_joints_number(), 1, "joints")
eq(s:get_element_by_order(1):get_mass(), 40, "element 1")
eq(s:get_element_by_bone_id(1):get_mass(), 40, "by bone id")
eq(s:get_element_by_bone_name("door"):get_mass(), 40, "by bone name")
eq(s:get_element_by_bone_name("lock"), nil, "a bone that joined an element")
eq(s:get_element_by_bone_name("none"), nil, "a name the model lacks")
eq(s:get_joint_by_bone_name("door"):get_bone_id(), 1, "the hinge by name")
eq(s:get_joint_by_bone_id(0), nil, "the root has no joint")
eq(s:get_joint_by_order(0):get_bone_id(), 1, "the hinge by order")
eq(s:is_breakable(), false, "no breaking data")
eq(s:is_breaking_blocked(), false, "no breaking data, not blocked")
s:block_breaking()
eq(s:is_breaking_blocked(), false, "nothing to block")
local v = vector():set(9, 9, 9)
s:get_linear_vel(v)
eq(v.x + v.y + v.z, 0, "a switched-off first element: zero")
s:apply_force(0, 0, 1e-8)
"#,
    )?;
    fatal(
        &mut t.s,
        "obj(90):get_physics_shell():get_element_by_order(2)",
        "3.2 order",
    )?;
    // 3.1: a force of 400 N along z, all on the one free element (the
    // fixed root has no mass of its own), wakes the shell.
    let mut t = state()?;
    t.s.exec(
        "=push",
        b"obj(90):get_physics_shell():apply_force(0, 0, 400)",
    )
    .map_err(|e| e.message)?;
    let on = t.world.body(DOOR, 1).map(|b| b.switched_on);
    ensure(on == Some(true), || format!("3.1: woken: {on:?}"))?;
    let w = t.world.0.borrow();
    let id = w.shells[&DOOR];
    let active = w.world.is_active(id);
    drop(w);
    ensure(!active, || {
        "3.1: woken, the shell is still not stepped (only the motor setter activates it)".into()
    })
}

/// 4.
pub fn element() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "4",
        r#"
local s = obj(90):get_physics_shell()
local root, door = s:get_element_by_order(0), s:get_element_by_order(1)
eq(root:is_fixed(), true, "the shapeless root is fixed")
eq(door:is_fixed(), false, "the door isn't")
eq(math.abs(door:get_volume() - 0.4) < 1e-6, true, "volume")
eq(math.abs(door:get_density() - 100) < 1e-3, true, "density")
eq(door:is_breakable(), false, "breakable")
local m = door:global_transform()
eq(m.c.x, 4, "frame origin x")
eq(m.c.y, 6, "frame origin y")
eq(m.c.z, 6, "frame origin z")
eq(m.j.y, 1, "frame axis j")
local v = vector():set(1, 1, 1)
door:get_angular_vel(v)
eq(v.x, 0, "a switched-off body's velocity")
door:fix()
eq(door:is_fixed(), true, "fixed")
door:apply_force(0, 0, 100)
door:release_fixed()
eq(door:is_fixed(), false, "released")
door:apply_force(0, 0, 100)
"#,
    )?;
    let b = t.world.body(DOOR, 1);
    ensure(b.is_some_and(|b| b.switched_on), || {
        format!("4.1: a force switches the body on: {b:?}")
    })?;
    let w = t.world.0.borrow();
    let s = w.shell(DOOR).ok_or("no shell")?;
    let body = s.body(w.world.solver(), 1).ok_or("no body")?;
    ensure((body.mass - 40.0).abs() < 1e-3 && !body.gravity, || {
        format!(
            "4.3: released, its own mass back, gravity still off: {} {}",
            body.mass, body.gravity
        )
    })
}

/// 5.
pub fn joint() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "5",
        r#"
local j = obj(90):get_physics_shell():get_joint_by_bone_name("door")
eq(j:get_axes_number(), 1, "a hinge's axes")
eq(j:get_first_element():is_fixed(), true, "the first element, the root")
eq(j:get_stcond_element():get_mass(), 40, "the second element")
eq(j:is_breakable(), false, "breakable")
-- Before a setter, the bone's limits; its first element is fixed, so they
-- come back negated and swapped (5.1, `joints` 3).
local lo, hi = j:get_limits(0, 0, 0)
eq(lo, 0, "low")
eq(math.abs(hi - 1.9199) < 1e-6, true, "high " .. tostring(hi))
raises(function() j:get_limits(nil, 0, 0) end, "number expected", "get_limits reads its arguments")
raises(function() j:get_max_force_and_velocity(0, 0, 0) end, "no overload", "references, not numbers")
eq(j:get_axis_angle(90), 0, "the hinge angle at the pose it was made in")
local a = vector():set(0, 0, 0)
j:get_axis_dir(5, a)
eq(a.y, -1, "the negated axis of a hinge with a fixed first element (`joints` 2.1)")
local p = vector()
j:get_anchor(p)
eq(p.y, 6, "the anchor, the door bone's origin")
j:set_limits(-1, 0.5, 1)
lo, hi = j:get_limits(0, 0, 7)
eq(lo, -0.5, "set_limits on axis 1 is axis 0's for a hinge")
eq(hi, 1, "set_limits high")
j:set_anchor_global(1, 2, 3)
j:set_axis_dir_vs_first_element(0, 1, 0, -1)
j:set_joint_spring_dumping_factors(1, 1)
j:set_axis_spring_dumping_factors(1, 1, -1)
j:set_max_force_and_velocity(2100, -3, 0)
"#,
    )?;
    let w = t.world.0.borrow();
    let id = w.hinge(DOOR, 0).ok_or("no hinge")?;
    let solver = w.world.solver();
    let read = |p| solver.hinge_param(id, p);
    let got = (
        read(library::F_MAX),
        read(library::VEL),
        read(library::LO_STOP),
        read(library::HI_STOP),
    );
    ensure(
        got == (Some(2100.0), Some(-3.0), Some(-1.0), Some(0.5)),
        || format!("5.2, 5.3: the library's parameters {got:?}"),
    )?;
    let active = w.world.is_active(w.shells[&DOOR]);
    drop(w);
    ensure(active, || {
        "5.3: the motor setter makes the shell active".into()
    })?;
    let stored = t.physics.borrow().shells[&DOOR].joints[0].clone();
    ensure(
        stored.anchor == Some((crate::game_object::physics::Frame::World, [1.0, 2.0, 3.0]))
            && stored.axes[0].direction
                == Some((
                    crate::game_object::physics::Frame::FirstElement,
                    [0.0, 1.0, 0.0],
                ))
            && (stored.axes[0].force, stored.axes[0].velocity) == (2100.0, -3.0),
        || format!("5.5: stored values {stored:?}"),
    )?;
    // The opening motor turns the door (`joints` 3: toward 1.9199).
    t.s.exec(
        "=open",
        b"obj(90):get_physics_shell():get_joint_by_order(0):set_limits(-1.9199, 0, 0)",
    )
    .map_err(|e| e.message)?;
    step(&t.world, 25);
    run(
        &mut t.s,
        "5 open",
        r#"
local a = obj(90):get_physics_shell():get_joint_by_order(0):get_axis_angle(0)
eq(a > 0.3, true, "the opening motor turned the door: " .. a)
"#,
    )
}

fn weapon() -> CarWeapon {
    CarWeapon {
        current_dir: [1.0, 0.0, 0.0],
        desired_dir: [0.0, 0.0, 1.0],
        default_dir: [0.0, 1.0, 0.0],
        fire_point: [0.0, 2.0, 0.0],
        ..CarWeapon::default()
    }
}

/// 6, 7.
pub fn car() -> Result<(), String> {
    let mut t = state()?;
    if let Some(o) = t.objects.borrow_mut().get_mut(CAR) {
        o.entity.as_mut().ok_or("no entity")?.health = 0.75;
    }
    run(
        &mut t.s,
        "7 without a weapon",
        r#"
local c = obj(91):get_car()
eq(c:HasWeapon(), false, "no weapon")
eq(c:CanHit(), false, "can't hit")
eq(c:FireDirDiff(), 0, "no direction difference")
c:Action(CCar.eWpnFire, 1)
eq(c:GetfHealth(), 0.75, "the entity's health")
eq(obj(91).health, -1, "spec M's health on an entity that isn't living")
c:SetfHealth(2)
eq(c:GetfHealth(), 2, "not clamped")
eq(c:ExplodeTime(), 0, "the timer isn't set")
c:SetExplodeTime(2600)
eq(c:ExplodeTime(), 3000, "whole seconds, in ms")
eq(c:IsObjectVisible(nil), false, "a null argument")
eq(c:IsObjectVisible(obj(90)), true, "nothing blocks without a level")
"#,
    )?;
    let log = t.services.borrow().console.log().join("\n");
    ensure(log.contains("IsObjectVisible"), || {
        format!("7: the null argument is logged: {log}")
    })?;
    fatal(
        &mut t.s,
        "obj(91):get_car():CurrentVel()",
        "7 CurrentVel without a shell",
    )?;
    let mut t = state()?;
    t.physics.borrow_mut().cars.entry(CAR).or_default().weapon = Some(weapon());
    run(
        &mut t.s,
        "6.2 the weapon",
        r#"
local c = obj(91):get_car()
local h = obj(91):get_holder_class()
eq(c:HasWeapon(), true, "a weapon")
eq(math.abs(c:FireDirDiff() - 90) < 1e-4, true, "degrees between the aims")
c:Action(CCar.eWpnActivate, 1)
h:Action(CCar.eWpnFire, 1)
c:Action(CCar.eWpnAutoFire, 1)
c:SetParam(CCar.eWpnDesiredPos, vector():set(0, 2, -5))
c:SetParam(CCar.eWpnDesiredDir, vector():set(1, 1, 1))
"#,
    )?;
    let w = t.physics.borrow().cars[&CAR]
        .weapon
        .clone()
        .ok_or("weapon")?;
    ensure(
        w.firing && w.activated && w.auto_fire && w.desired_dir == [0.0, 0.0, -1.0],
        || format!("6.2: {w:?}"),
    )?;
    run(
        &mut t.s,
        "6.2 off",
        r#"
local c = obj(91):get_car()
c:Action(CCar.eWpnFire, 0)
c:Action(CCar.eWpnToDefaultDir, 0)
c:CarExplode()
c:CarExplode()
"#,
    )?;
    let mut p = t.physics.borrow_mut();
    let w = p.cars[&CAR].weapon.clone().ok_or("weapon")?;
    ensure(
        !w.firing && !w.activated && w.desired_dir == [0.0, 1.0, 0.0],
        || format!("6.2 off: {w:?}"),
    )?;
    let events = p.take_events();
    ensure(
        events.len() == 2
            && matches!(
                events[0],
                PhysicsEvent::Explode {
                    object: CAR,
                    normal: [0.0, 1.0, 0.0],
                    ..
                }
            )
            && events[1] == PhysicsEvent::CarExploded { object: CAR },
        || format!("7: CarExplode once: {events:?}"),
    )
}

/// 8.
pub fn helicopter() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "8",
        r#"
local h = obj(92):get_helicopter()
eq(h:GetState(), CHelicopter.eAlive, "alive")
eq(h:GetMovementState(), CHelicopter.eMovNone, "no movement")
eq(h:GetMaxVelocity(), 22, "the section's velocity")
eq(h:GetOnPointRangeDist(), 5, "on_point_range_dist")
eq(h.m_use_rocket_on_attack, true, "use_rocket")
eq(h.m_max_mgun_dist, 200, "max_mgun_attack_dist")
eq(h.m_time_between_rocket_attack, 2000, "time_between_rocket_attack")
h.m_use_mgun_on_attack = true
eq(h.m_use_mgun_on_attack, true, "writable")
eq(h:UseFireTrail(), true, "use_fire_trail")
h:SetDestPosition(vector():set(0, 50, 30))
eq(h:GetMovementState(), CHelicopter.eMovToPoint, "to a point")
eq(h:GetDistanceToDestPosition(), 30, "distance")
h:GoPatrolByRoundPath(vector(), 100, true)
eq(h:GetMovementState(), CHelicopter.eMovRoundPath, "round path")
h:SetMaxVelocity(40)
eq(h:GetMaxVelocity(), 40, "max velocity")
h:SetSpeedInDestPoint(3)
eq(h:GetSpeedInDestPoint(0), 3, "speed in the point")
eq(h:GetSafeAltitude(), 30, "safe altitude without a level")
eq(h:GetRealAltitude(), 1000, "nothing below")
local d = h:GetCurrVelocityVec()
eq(d.z, 1, "heading 0, pitch 0")
h:SetEnemy(obj(10))
eq(h:GetHuntState(), CHelicopter.eEnemyEntity, "an enemy object")
h:SetEnemy(vector():set(1, 2, 3))
eq(h:GetHuntState(), CHelicopter.eEnemyPoint, "an enemy point")
h:ClearEnemy()
eq(h:GetHuntState(), CHelicopter.eEnemyNone, "no enemy")
h:LookAtPoint(vector(), true)
eq(h:GetBodyState(), CHelicopter.eBodyToPoint, "looking at a point")
eq(h:isVisible(obj(10)), true, "nothing blocks without a level")
h:TurnLighting(true)
eq(h.m_light_started, true, "lighting")
h:StartFlame()
h:StartFlame()
eq(h.m_flame_started, true, "flame")
h:Die()
h:Die()
eq(h:GetState(), CHelicopter.eDead, "dead")
eq(h.m_dead, true, "m_dead")
h:Explode()
eq(h.m_exploded, true, "exploded")
h:SetfHealth(0.5)
eq(h:GetfHealth(), 0.5, "health")
"#,
    )?;
    let p = t.physics.borrow();
    let h = &p.helicopters[&HELICOPTER];
    ensure(
        (h.fire_dispersion - 2.0 * 0.017_453_3).abs() < 1e-7
            && h.round_path
                .as_ref()
                .is_some_and(|r| r.radius == 100.0 && r.clockwise),
        || format!("8.2, 8.4: {h:?}"),
    )?;
    drop(p);
    t.s.exec(
        "=again",
        b"local h = obj(92):get_helicopter() h:GoPatrolByRoundPath(vector(), 50, true) h:UseFireTrail(false)",
    )
    .map_err(|e| e.message)?;
    let mut p = t.physics.borrow_mut();
    let h = &p.helicopters[&HELICOPTER];
    ensure(
        h.round_path.as_ref().is_some_and(|r| !r.clockwise)
            && (h.fire_dispersion - 10.0 * 0.017_453_3).abs() < 1e-6,
        || format!("8.2: a second round path inverts the direction: {h:?}"),
    )?;
    let events = p.take_events();
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            PhysicsEvent::Flame { .. } => "flame",
            PhysicsEvent::HelicopterDied { .. } => "died",
            PhysicsEvent::HelicopterExploded { .. } => "exploded",
            PhysicsEvent::Explode { .. } => "explode",
            _ => "other",
        })
        .collect();
    ensure(kinds == ["flame", "died", "exploded", "explode"], || {
        format!("8.5: {events:?}")
    })
}

/// 9.
pub fn lamp() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "9",
        "local l = obj(93):get_hanging_lamp() l:turn_off() l:turn_on()",
    )?;
    let events = t.physics.borrow_mut().take_events();
    ensure(
        events
            == [
                PhysicsEvent::LampSwitched {
                    object: LAMP,
                    on: false,
                },
                PhysicsEvent::LampSwitched {
                    object: LAMP,
                    on: true,
                },
            ],
        || format!("9: {events:?}"),
    )
}

/// 11: the door scheme's calls.
pub fn door_scheme() -> Result<(), String> {
    let mut t = state()?;
    run(
        &mut t.s,
        "11",
        r#"
local ph_shell = obj(90):get_physics_shell()
local joint = ph_shell:get_joint_by_bone_name("door")
local low, hi = 0, 0
low, hi = joint:get_limits(low, hi, 0)
joint:set_max_force_and_velocity(10000, 1, 0)
eq(joint:get_axis_angle(90) <= low, true, "closed at the pose the hinge was made in")
"#,
    )?;
    step(&t.world, 10);
    run(
        &mut t.s,
        "11 closed",
        r#"
local joint = obj(90):get_physics_shell():get_joint_by_bone_name("door")
local low = joint:get_limits(0, 0, 0)
eq(joint:get_axis_angle(90) < 0.01, true, "the closing motor holds it closed")
"#,
    )
}
