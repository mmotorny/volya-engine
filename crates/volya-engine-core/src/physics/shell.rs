// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Shells from models and level records (docs/behavior/physics-shells.md,
//! cited by section; `resting` is docs/behavior/physics-resting.md,
//! `bone` docs/formats/bone-physics.md): what a shell is made of (2), the
//! build from the bone data (3), the physics types of physics objects (4),
//! fixed bones and the physics keys (5), the hanging lamps' shells (5.3) and
//! the spawn's checks (7, 9). A [`Shell`] is the description an activation
//! turns into bodies and joints (`physics::ode::shell`): its elements' frames
//! in model space, the shell's transform, and what activating it does.

use std::io;
use std::ops::Range;

use volya_engine_formats::bone_physics::{
    self, Axis, AxisLimits, JointType, LimitedJoint, UsedShape,
};
use volya_engine_formats::gamemtl::GameMaterial;
use volya_engine_formats::ltx::Config;
use volya_engine_formats::model::{Bone, Header, Mat4, multiply, transform, transform_point};

use crate::collision::geometry::Vec3;

/// Skeleton flags bit 0x01: when clear, the shell is activated switched
/// off (1, 4.4; `resting` 1).
pub const SKELETON_ACTIVE: u8 = 0x01;
/// The hanging lamp's light flag giving it a shell (5.3; `lamps` 3.4).
pub const LAMP_PHYSICS: u16 = 0x01;

/// An element's starting motion parameters (2.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub linear_limit: f32,
    pub angular_limit: f32,
    pub linear_divisor: f32,
    pub angular_divisor: f32,
    pub linear_drag: f32,
    pub angular_drag: f32,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            linear_limit: 150.0,
            angular_limit: 9.817_477,
            linear_divisor: 1.01,
            angular_divisor: 1.01,
            linear_drag: 0.0002,
            angular_drag: 0.05,
        }
    }
}

/// The drags after a build from the bone data (3.4) and of a lamp's first
/// activation (5.3).
pub const BUILD_DRAGS: (f32, f32) = (0.0002, 0.05);
/// The drags every physics object's shell ends with (4.1), and a lamp's (5.3).
pub const OBJECT_DRAGS: (f32, f32) = (0.001, 0.02);

/// A collision geometry in its element's frame (2.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// The box's x, y and z directions, its centre and its half sizes, each
    /// at least [`bone_physics::MIN_HALF_SIZE`].
    Box {
        axes: [Vec3; 3],
        centre: Vec3,
        half_size: Vec3,
    },
    Sphere {
        centre: Vec3,
        radius: f32,
    },
    Cylinder {
        centre: Vec3,
        direction: Vec3,
        height: f32,
        radius: f32,
    },
}

impl Shape {
    /// `bone`'s used shape in its own space. Spheres and cylinders get no
    /// minimum size: whether they do is open (TODO(#939)).
    fn of(shape: &UsedShape) -> Self {
        match shape {
            UsedShape::Box(b) => Self::Box {
                axes: b.axes,
                centre: b.centre,
                half_size: b.collision_half_size(),
            },
            UsedShape::Sphere(s) => Self::Sphere {
                centre: s.centre,
                radius: s.radius,
            },
            UsedShape::Cylinder(c) => Self::Cylinder {
                centre: c.centre,
                direction: c.direction,
                height: c.height,
                radius: c.radius,
            },
        }
    }

    pub fn centre(&self) -> Vec3 {
        match *self {
            Self::Box { centre, .. }
            | Self::Sphere { centre, .. }
            | Self::Cylinder { centre, .. } => centre,
        }
    }

    /// The volume that shares a mass between geometries (2.3, `bone` 2.3).
    pub fn volume(&self) -> f32 {
        match *self {
            Self::Box { half_size: h, .. } => 8.0 * h[0] * h[1] * h[2],
            Self::Sphere { radius: r, .. } => 4.0 / 3.0 * std::f32::consts::PI * r.powi(3),
            Self::Cylinder { height, radius, .. } => {
                std::f32::consts::PI * radius * radius * height
            }
        }
    }

    /// The shape carried by the rigid transform `m` (row vectors).
    pub fn moved(&self, m: &Mat4) -> Self {
        match *self {
            Self::Box {
                axes,
                centre,
                half_size,
            } => Self::Box {
                axes: axes.map(|a| turn(m, a)),
                centre: transform_point(m, centre),
                half_size,
            },
            Self::Sphere { centre, radius } => Self::Sphere {
                centre: transform_point(m, centre),
                radius,
            },
            Self::Cylinder {
                centre,
                direction,
                height,
                radius,
            } => Self::Cylinder {
                centre: transform_point(m, centre),
                direction: turn(m, direction),
                height,
                radius,
            },
        }
    }
}

/// `v` turned by `m`'s rotation (row vectors).
pub fn turn(m: &Mat4, v: Vec3) -> Vec3 {
    std::array::from_fn(|c| (0..3).map(|k| v[k] * m[k][c]).sum())
}

/// The inverse of a rigid transform (row vectors: `[[R, 0], [p, 1]]`).
pub fn inverse_rigid(m: &Mat4) -> Mat4 {
    let mut r = [[0.0; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = m[j][i];
        }
    }
    for j in 0..3 {
        r[3][j] = -(0..3).map(|k| m[3][k] * r[k][j]).sum::<f32>();
    }
    r[3][3] = 1.0;
    r
}

/// The identity transform.
pub const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// A geometry of an element, with the bone it was made from (2.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub shape: Shape,
    pub bone: Option<usize>,
}

/// A mass spread over some of an element's geometries in proportion to
/// their volumes (2.3): about `centre` (a bone's centre of mass, in the
/// element's frame) or, without one, about their volume-weighted centre.
#[derive(Clone, Debug, PartialEq)]
pub struct MassPart {
    pub total: f32,
    pub centre: Option<Vec3>,
    pub geometries: Range<usize>,
}

/// How an element's mass is set (2.3).
#[derive(Clone, Debug, PartialEq)]
pub enum Mass {
    /// Masses on its geometries: the bone's own, then those of the rigid
    /// bones joined to it (`bone` 5).
    Parts(Vec<MassPart>),
    /// A density on every geometry.
    Density(f32),
}

/// One geometry's share of an element's mass (2.3): the density of its
/// volume, at its offset from the element's mass centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassPiece {
    pub shape: Shape,
    pub density: f32,
    /// The geometry's centre less the element's mass centre.
    pub offset: Vec3,
}

/// One body of a shell with its geometries (conventions).
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// Its frame in model space (the bone's transform, 3.2).
    pub frame: Mat4,
    /// The bone that made it.
    pub bone: Option<usize>,
    /// The element above it.
    pub above: Option<usize>,
    /// Its material's index in the game materials (`bone` 6).
    pub material: u32,
    pub geometries: Vec<Geometry>,
    pub mass: Mass,
    pub motion: Motion,
}

impl Element {
    fn new(frame: Mat4, bone: Option<usize>, above: Option<usize>, material: u32) -> Self {
        Self {
            frame,
            bone,
            above,
            material,
            geometries: Vec::new(),
            mass: Mass::Parts(Vec::new()),
            motion: Motion::default(),
        }
    }

    /// Adds geometries and the mass they carry.
    fn add(
        &mut self,
        shapes: impl IntoIterator<Item = Geometry>,
        total: f32,
        centre: Option<Vec3>,
    ) {
        let start = self.geometries.len();
        self.geometries.extend(shapes);
        let part = MassPart {
            total,
            centre,
            geometries: start..self.geometries.len(),
        };
        match &mut self.mass {
            Mass::Parts(parts) => parts.push(part),
            Mass::Density(_) => self.mass = Mass::Parts(vec![part]),
        }
    }

    pub fn has_geometry(&self) -> bool {
        !self.geometries.is_empty()
    }

    fn volume_centre(&self, range: Range<usize>) -> (f32, Vec3) {
        let mut volume = 0.0;
        let mut sum = [0.0; 3];
        for g in &self.geometries[range] {
            let v = g.shape.volume();
            volume += v;
            let c = g.shape.centre();
            for k in 0..3 {
                sum[k] += v * c[k];
            }
        }
        (volume, sum.map(|s| s / volume))
    }

    /// The element's mass (2.3): its total and its mass centre in its frame.
    pub fn mass_centre(&self) -> (f32, Vec3) {
        match &self.mass {
            Mass::Density(d) => {
                let (v, c) = self.volume_centre(0..self.geometries.len());
                (v * d, c)
            }
            Mass::Parts(parts) => {
                let mut total = 0.0;
                let mut sum = [0.0; 3];
                for p in parts {
                    let c = p
                        .centre
                        .unwrap_or_else(|| self.volume_centre(p.geometries.clone()).1);
                    total += p.total;
                    for k in 0..3 {
                        sum[k] += p.total * c[k];
                    }
                }
                (total, sum.map(|s| s / total))
            }
        }
    }

    /// Each geometry's share of the mass, about the mass centre (2.3).
    pub fn mass_pieces(&self) -> Vec<MassPiece> {
        let (_, centre) = self.mass_centre();
        let piece = |g: &Geometry, density: f32| MassPiece {
            shape: g.shape,
            density,
            offset: std::array::from_fn(|k| g.shape.centre()[k] - centre[k]),
        };
        match &self.mass {
            Mass::Density(d) => self.geometries.iter().map(|g| piece(g, *d)).collect(),
            Mass::Parts(parts) => parts
                .iter()
                .flat_map(|p| {
                    let (v, _) = self.volume_centre(p.geometries.clone());
                    let density = p.total / v;
                    self.geometries[p.geometries.clone()]
                        .iter()
                        .map(move |g| piece(g, density))
                })
                .collect(),
        }
    }
}

/// A joint between an element and the one above it (3.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    /// The element above (first) and the bone's (second).
    pub first: usize,
    pub second: usize,
    /// The bone it was made from.
    pub bone: usize,
    pub kind: JointKind,
    /// The bone data's stiffness factors (`bone` 3.3).
    pub spring: f32,
    pub damping: f32,
}

/// The joints of `bone` 3.1 – 3.2.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JointKind {
    /// A hinge about a bone axis of the second element, with that axis's
    /// limit record.
    Hinge { axis: Axis, limits: AxisLimits },
    /// Ball, three-axis, two-axis hinge and slider: their simulation is
    /// PH-7's (TODO(#750)), and they are not made.
    Other(JointType),
}

/// The marks of a shell's collision class (5.2; their effect is PH-4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollisionClass {
    pub small_object: bool,
    pub ignore_small_objects: bool,
    pub ignore_ragdoll: bool,
}

/// A shell as built, before it is activated.
#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    pub elements: Vec<Element>,
    pub joints: Vec<Joint>,
    /// The object's transform as the shell holds it (conventions).
    pub transform: Mat4,
    /// Activated switched off (`resting` 1).
    pub switched_off: bool,
    /// Elements fixed after activation (4.4, 5.1, 5.2), in order.
    pub fixed: Vec<usize>,
    /// The request for the exact solver (3.5).
    pub exact_solver: bool,
    /// No contacts with the level's static mesh (5.2).
    pub ignore_static: bool,
    pub collision_class: CollisionClass,
    /// A collision group of its own (5.2, `not_collide_parts`).
    pub collision_group: Option<u32>,
}

impl Shell {
    fn new(elements: Vec<Element>, joints: Vec<Joint>) -> Self {
        Self {
            elements,
            joints,
            transform: IDENTITY,
            switched_off: false,
            fixed: Vec::new(),
            exact_solver: false,
            ignore_static: false,
            collision_class: CollisionClass::default(),
            collision_group: None,
        }
    }

    /// Sets both drags of every element (2.4).
    pub fn set_drags(&mut self, (linear, angular): (f32, f32)) {
        for e in &mut self.elements {
            e.motion.linear_drag = linear;
            e.motion.angular_drag = angular;
        }
    }

    pub fn has_geometry(&self) -> bool {
        self.elements.iter().any(Element::has_geometry)
    }

    /// Fixes an element after activation (once per naming).
    fn fix(&mut self, element: usize) {
        self.fixed.push(element);
    }
}

fn fatal(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The bones of a model a shell is built from, with what the build reads
/// of the rest of its file.
#[derive(Clone, Debug)]
pub struct ShellModel {
    pub header: Header,
    pub bones: Vec<Bone>,
    /// Its user data, read as LTX text (`scope` 1.3).
    pub user_data: Option<Config>,
}

impl ShellModel {
    /// A model file's header, bones and user data. User data that is not
    /// LTX text is read as none (our choice: none in the data, `scope` 1.3).
    pub fn read(file: &[u8]) -> io::Result<Self> {
        let (header, bones) = volya_engine_formats::model::skeleton(file)?;
        let user_data = volya_engine_formats::model::user_data(file)?
            .and_then(|text| volya_engine_formats::ltx::parse(&text).ok());
        Ok(Self {
            header,
            bones,
            user_data,
        })
    }

    /// The bone a name names, ignoring case.
    pub fn bone_named(&self, name: &str) -> Option<usize> {
        self.bone(name)
    }

    /// Whether the model has motions: an animated skeleton (visual type 3,
    /// docs/formats/skeleton-ogf.md 1; believed the condition of section 7).
    pub fn has_motions(&self) -> bool {
        self.header.kind == 3
    }

    fn bone(&self, name: &str) -> Option<usize> {
        self.bones
            .iter()
            .position(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// The bones named by a comma-separated list; one the model lacks is
    /// fatal (4.4, 5.2, 5.3).
    pub fn named_bones(&self, list: &str) -> io::Result<Vec<usize>> {
        list.split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(|n| {
                self.bone(n)
                    .ok_or_else(|| fatal(format!("wrong fixed bone '{n}'")))
            })
            .collect()
    }

    fn material(&self, bone: usize, materials: &[GameMaterial]) -> io::Result<u32> {
        Ok(bone_physics::material(&self.bones[bone].physics.material, materials)? as u32)
    }
}

/// The model's root bone and visible bones from a record's saved bone state
/// (3.1): with none, root 0 and every bone visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoneState {
    pub root: usize,
    /// Bit *i*: bone *i* visible.
    pub visible: u64,
}

impl Default for BoneState {
    fn default() -> Self {
        Self {
            root: 0,
            visible: u64::MAX,
        }
    }
}

impl BoneState {
    pub fn visible(&self, bone: usize) -> bool {
        bone >= 64 || self.visible & (1 << bone) != 0
    }
}

/// The plain build from the bone data (3.2, 3.3, 3.5): the shell
/// and, for each bone walked, the element it ended in.
pub fn from_bones(
    model: &ShellModel,
    state: BoneState,
    materials: &[GameMaterial],
) -> io::Result<(Shell, Vec<Option<usize>>)> {
    let bones = &model.bones;
    let mut elements: Vec<Element> = Vec::new();
    let mut joints = Vec::new();
    let mut ended = vec![None; bones.len()];
    let local = |i: usize| {
        let b = &bones[i];
        transform(b.bind_position, b.bind_rotation.map(|r| -r))
    };
    // (bone, the transform passed down, the element above).
    let mut stack = vec![(state.root, IDENTITY, None::<usize>)];
    while let Some((i, down, above)) = stack.pop() {
        let at = multiply(&local(i), &down);
        let physics = &bones[i].physics;
        let used = physics.shape.used();
        let mut below = above;
        if !state.visible(i) {
            // 1: nothing of its own.
        } else if above.is_none() || (used.is_some() && physics.joint != JointType::Rigid) {
            // 2: an element of its own (3.5: always for the root).
            let index = elements.len();
            let mut e = Element::new(at, Some(i), above, model.material(i, materials)?);
            if let Some(shape) = &used {
                let g = Geometry {
                    shape: Shape::of(shape),
                    bone: Some(i),
                };
                e.add([g], physics.mass, Some(physics.centre_of_mass));
            }
            elements.push(e);
            if let Some(first) = above {
                joints.extend(joint(first, index, i, physics));
            }
            below = Some(index);
        } else if let (Some(shape), Some(a)) = (&used, above) {
            // 3: a rigid bone's shape and mass join the element above, in
            // its frame. A rigid breakable bone's split is PH-13's.
            let into = multiply(&at, &inverse_rigid(&elements[a].frame));
            let g = Geometry {
                shape: Shape::of(shape).moved(&into),
                bone: Some(i),
            };
            let centre = transform_point(&into, physics.centre_of_mass);
            if !(physics.mass.is_finite() && physics.mass > 0.0) {
                return Err(fatal("bad bone mass params"));
            }
            elements[a].add([g], physics.mass, Some(centre));
        }
        // 4: no used shape below a body adds nothing.
        ended[i] = below;
        // Children in bone order: pushed in reverse.
        let children: Vec<usize> = (0..bones.len())
            .filter(|&c| bones[c].parent == Some(i))
            .collect();
        for &c in children.iter().rev() {
            stack.push((c, at, below));
        }
    }
    Ok((Shell::new(elements, joints), ended))
}

/// The joint a bone of `physics` makes between `first` and `second`
/// (`bone` 3.1, 3.2); joint type 4 makes none.
fn joint(
    first: usize,
    second: usize,
    bone: usize,
    physics: &bone_physics::BonePhysics,
) -> Option<Joint> {
    let kind = match physics.joint {
        JointType::None | JointType::Rigid => return None,
        JointType::Limited => match physics.limited_joint() {
            Some(LimitedJoint::Hinge(axis)) => JointKind::Hinge {
                axis,
                limits: *physics.limits(axis),
            },
            _ => JointKind::Other(JointType::Limited),
        },
        t => JointKind::Other(t),
    };
    Some(Joint {
        first,
        second,
        bone,
        kind,
        spring: physics.spring,
        damping: physics.damping,
    })
}

/// What a physics object's build reads of its record (4, 7).
#[derive(Clone, Copy, Debug)]
pub struct PhysicsRecord<'a> {
    pub physics_type: u32,
    pub mass: f32,
    pub fixed_bones: &'a str,
    pub skeleton_flags: u8,
    pub startup_animation: &'a str,
    /// The record's custom data, read as LTX text (4.4).
    pub custom_data: Option<&'a Config>,
    /// The object's transform *X*.
    pub transform: Mat4,
    pub bone_state: BoneState,
}

/// The type-0 shell and the items' box shell (4.2, 6): one unrotated box at
/// the centre of the model's bounding box, half sizes 0.5 × its extent,
/// with `mass` (a total about the box's centre, or a density).
pub fn simple_box(model: &ShellModel, material: u32, mass: Mass) -> Shell {
    let (min, max) = (model.header.box_min, model.header.box_max);
    let shape = Shape::Box {
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        centre: std::array::from_fn(|k| 0.5 * (min[k] + max[k])),
        half_size: std::array::from_fn(|k| {
            (0.5 * (max[k] - min[k])).max(bone_physics::MIN_HALF_SIZE)
        }),
    };
    let mut e = Element::new(IDENTITY, None, None, material);
    e.geometries.push(Geometry { shape, bone: None });
    e.mass = mass;
    Shell::new(vec![e], Vec::new())
}

/// A physics object's shell at spawn by its physics type (4, 7): `None`
/// for types 1 and 2, unused in the data, whose activation is open
/// (4.3, TODO(#947)), and for other types.
pub fn physics_object(
    model: &ShellModel,
    record: &PhysicsRecord<'_>,
    materials: &[GameMaterial],
) -> io::Result<Option<Shell>> {
    let switched_off = record.skeleton_flags & SKELETON_ACTIVE == 0;
    let mut shell = match record.physics_type {
        0 => {
            // 4.2: the record's mass over the box; no bone, so no custom
            // data read. The root bone's material (`bone` 6, believed).
            let material = model.material(record.bone_state.root, materials)?;
            let mass = Mass::Parts(vec![MassPart {
                total: record.mass,
                centre: None,
                geometries: 0..1,
            }]);
            let mut s = simple_box(model, material, mass);
            s.switched_off = switched_off;
            s
        }
        3 => {
            let named = model.named_bones(record.fixed_bones)?;
            let (mut s, ended) = from_bones(model, record.bone_state, materials)?;
            s.set_drags(BUILD_DRAGS);
            s.switched_off = switched_off;
            if !named.is_empty() {
                s.exact_solver = true;
                for b in named {
                    s.fix(element_of(&ended, b)?);
                }
            }
            let has_fixed = !record.fixed_bones.is_empty();
            if let Some(custom) = record.custom_data {
                apply_keys(&mut s, model, &ended, custom, has_fixed)?;
            }
            if let Some(user) = &model.user_data {
                apply_keys(&mut s, model, &ended, user, has_fixed)?;
            }
            s
        }
        _ => return Ok(None),
    };
    // 4.1, whatever the type. The `[disable]` rest parameters are PH-5's
    // (TODO(#946)).
    shell.transform = record.transform;
    shell.set_drags(OBJECT_DRAGS);
    // 7: the startup animation of a model with motions, played at spawn
    // (`LevelScreen::spawn_shells`); one the model lacks is open
    // (TODO(#939)).
    if model.has_motions() && record.startup_animation.is_empty() {
        return Err(fatal("no startup animation"));
    }
    Ok(Some(shell))
}

fn element_of(ended: &[Option<usize>], bone: usize) -> io::Result<usize> {
    // A bone the walk did not reach (none in the data) has no element.
    ended[bone].ok_or_else(|| fatal("wrong fixed bone"))
}

/// The physics keys of a custom data or a model's user data (5.2), in
/// order; `has_fixed` whether the shell has fixed bones.
pub fn apply_keys(
    shell: &mut Shell,
    model: &ShellModel,
    ended: &[Option<usize>],
    keys: &Config,
    has_fixed: bool,
) -> io::Result<()> {
    if let Some(s) = keys.section("physics_common") {
        // Without the key: unestablished, none in the data; nothing.
        if let Some(Some(list)) = s.get("fixed_bones") {
            for b in model.named_bones(list)? {
                shell.fix(element_of(ended, b)?);
            }
        }
    }
    if let Some(s) = keys.section("collide") {
        if has_fixed && s.contains("ignore_static") {
            shell.ignore_static = true;
        }
        set_class(&mut shell.collision_class, |k| s.contains(k));
        shell.collision_class.ignore_ragdoll |= s.contains("ignore_ragdoll");
    }
    Ok(())
}

fn set_class(class: &mut CollisionClass, has: impl Fn(&str) -> bool) {
    class.small_object |= has("small_object");
    class.ignore_small_objects |= has("ignore_small_objects");
}

/// The spawn's reading of the model's user data after the shell is built
/// (5.2, section 7): `not_collide_parts` gives the shell the next number of
/// `groups`; `[collide_parts]` sets the class's marks.
pub fn apply_parts_keys(shell: &mut Shell, model: &ShellModel, groups: &mut u32) {
    let Some(user) = &model.user_data else {
        return;
    };
    if user
        .section("collide")
        .is_some_and(|s| s.contains("not_collide_parts"))
    {
        shell.collision_group = Some(*groups);
        *groups += 1;
    }
    if let Some(s) = user.section("collide_parts") {
        set_class(&mut shell.collision_class, |k| s.contains(k));
    }
}

/// What a hanging lamp's build reads of its record (5.3).
#[derive(Clone, Copy, Debug)]
pub struct LampRecord<'a> {
    pub light_flags: u16,
    pub fixed_bones: &'a str,
    pub custom_data: Option<&'a Config>,
    pub transform: Mat4,
    pub bone_state: BoneState,
}

/// A hanging lamp's shell (5.3): `None` without flag 0x01.
pub fn hanging_lamp(
    model: &ShellModel,
    record: &LampRecord<'_>,
    materials: &[GameMaterial],
) -> io::Result<Option<Shell>> {
    if record.light_flags & LAMP_PHYSICS == 0 {
        return Ok(None);
    }
    let (mut s, ended) = from_bones(model, record.bone_state, materials)?;
    s.transform = record.transform;
    s.switched_off = true;
    s.set_drags(BUILD_DRAGS);
    // The root when the field holds no text; whether an empty field is no
    // text is open (TODO(#947)): here it is.
    let named = model.named_bones(record.fixed_bones)?;
    if named.is_empty() {
        s.fix(element_of(&ended, record.bone_state.root)?);
    }
    for b in named {
        s.fix(element_of(&ended, b)?);
    }
    // The `[disable]` rest parameters are PH-5's (TODO(#946)).
    s.set_drags(OBJECT_DRAGS);
    if let Some(custom) = record.custom_data {
        apply_keys(
            &mut s,
            model,
            &ended,
            custom,
            !record.fixed_bones.is_empty(),
        )?;
    }
    Ok(Some(s))
}

/// A skeleton object's shell at its spawn (1, 3.4): the plain build, no
/// fixed bones, switched off when skeleton flag 0x01 is clear.
pub fn skeleton_object(
    model: &ShellModel,
    skeleton_flags: u8,
    transform: Mat4,
    bone_state: BoneState,
    materials: &[GameMaterial],
) -> io::Result<Shell> {
    let (mut s, _) = from_bones(model, bone_state, materials)?;
    s.transform = transform;
    s.switched_off = skeleton_flags & SKELETON_ACTIVE == 0;
    s.set_drags(BUILD_DRAGS);
    Ok(s)
}

#[cfg(test)]
pub(crate) mod tests;
