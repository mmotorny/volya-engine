// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The bone physics data of skeleton models (docs/formats/bone-physics.md):
//! chunk 16's record per bone (section 1) with its shape (2), joint (3),
//! friction (4), mass and centre of mass (5) and material (6); the
//! load-time check of breakable bones (3.4) and the material lookup (6).
//! How bodies, shapes and joints are built from it is the physics' own.

use std::f32::consts::PI;
use std::io;

use crate::gamemtl::{self, GameMaterial};
use crate::packet::{Reader, invalid};

/// Shape-flag bits (section 2.2).
pub mod shape_flags {
    /// The marks that hits leave on the model skip this bone.
    pub const NO_HIT_MARKS: u16 = 1;
    /// Removable (when every visible bone of an object carries it).
    pub const REMOVABLE: u16 = 2;
    /// The bone is treated as having shape type 0.
    pub const NO_PHYSICS: u16 = 4;
}

/// Joint-flag bit 1: the bone is breakable (section 3.4).
pub const BREAKABLE: u32 = 1;

/// The smallest half size of a box's collision geometry (section 2.3).
pub const MIN_HALF_SIZE: f32 = 0.005;

/// Limits of joint type 2 closer than this are equal (section 3.2).
pub const EQUAL_LIMITS: f32 = 1e-5;

/// The material of an empty name (section 6).
pub const DEFAULT_MATERIAL: &str = "default_object";

/// Shape types (section 2.1). Other values are an error: what they would do
/// is unestablished and none occurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShapeType {
    /// 0: no body, shape or mass of its own.
    None = 0,
    Box = 1,
    Sphere = 2,
    Cylinder = 3,
}

/// A box in the bone's own space (section 2.3).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxShape {
    /// The box's x, y and z directions.
    pub axes: [[f32; 3]; 3],
    pub centre: [f32; 3],
    pub half_size: [f32; 3],
}

impl BoxShape {
    /// The half sizes of the collision geometry: each at least
    /// [`MIN_HALF_SIZE`].
    pub fn collision_half_size(&self) -> [f32; 3] {
        self.half_size.map(|h| h.max(MIN_HALF_SIZE))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sphere {
    pub centre: [f32; 3],
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cylinder {
    pub centre: [f32; 3],
    /// Unit direction of its axis.
    pub direction: [f32; 3],
    /// Length along the axis.
    pub height: f32,
    pub radius: f32,
}

/// The shape of a bone: its type and flags, and all three parts as stored
/// whatever the type (section 1; only the type's part is used).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub kind: ShapeType,
    /// Bits of [`shape_flags`], as stored.
    pub flags: u16,
    pub cuboid: BoxShape,
    pub sphere: Sphere,
    pub cylinder: Cylinder,
}

/// The part of a used shape (section 2.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UsedShape {
    Box(BoxShape),
    Sphere(Sphere),
    Cylinder(Cylinder),
}

impl UsedShape {
    /// The volume that shares a body's mass between its shapes (section
    /// 2.3): a box with its raised half sizes.
    pub fn volume(&self) -> f32 {
        match self {
            Self::Box(b) => {
                let h = b.collision_half_size();
                8.0 * h[0] * h[1] * h[2]
            }
            Self::Sphere(s) => 4.0 / 3.0 * PI * s.radius.powi(3),
            Self::Cylinder(c) => PI * c.radius * c.radius * c.height,
        }
    }
}

impl Shape {
    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    /// The shape if it is used: types 1 – 3 without the no-physics bit.
    pub fn used(&self) -> Option<UsedShape> {
        if self.has(shape_flags::NO_PHYSICS) {
            return None;
        }
        match self.kind {
            ShapeType::None => None,
            ShapeType::Box => Some(UsedShape::Box(self.cuboid)),
            ShapeType::Sphere => Some(UsedShape::Sphere(self.sphere)),
            ShapeType::Cylinder => Some(UsedShape::Cylinder(self.cylinder)),
        }
    }
}

/// Joint types (section 3.1). Other values are an error: what they would do
/// is unestablished and none occurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JointType {
    /// 0: no body of its own; its used shape and mass join the body above.
    Rigid = 0,
    /// 1: free rotation about the anchor.
    Ball = 1,
    /// 2: a hinge or a three-axis joint, by which axes have equal limits.
    Limited = 2,
    /// 3: rotation about the bone's x axis (limited) and z axis.
    TwoAxisHinge = 3,
    /// 4: a body of its own but no joint.
    None = 4,
    /// 5: sliding along one axis (x limits) with rotation about it (y).
    Slider = 5,
}

/// A bone axis (section 3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Axis {
    X = 0,
    Y = 1,
    Z = 2,
}

/// One axis record of the limits (section 3.2): angles in radians, except
/// the x limits of the slider.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisLimits {
    pub low: f32,
    pub high: f32,
    pub spring_factor: f32,
    pub damping_factor: f32,
}

impl AxisLimits {
    /// The two limits are equal (joint type 2: the axis is locked).
    pub fn equal(&self) -> bool {
        (self.low - self.high).abs() < EQUAL_LIMITS
    }

    /// Whether the axis is given its limits: a range below 2π (a range of
    /// exactly 2π leaves it free).
    pub fn limited(&self) -> bool {
        self.high - self.low < 2.0 * PI
    }

    /// The limits as handed on, kept within −π … 0 (low) and 0 … π (high):
    /// a bound outside its range is moved inside and the other shifted by
    /// the same amount.
    pub fn kept(&self) -> (f32, f32) {
        let (mut low, mut high) = (self.low, self.high);
        let kept = low.clamp(-PI, 0.0);
        high += kept - low;
        low = kept;
        let kept = high.clamp(0.0, PI);
        low += kept - high;
        high = kept;
        // The order of the two moves, which matters only when both bounds
        // are out (none in the data), is our reading.
        (low.clamp(-PI, 0.0), high)
    }
}

/// How joint type 2 is built (section 3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitedJoint {
    /// A hinge about this bone axis.
    Hinge(Axis),
    /// A three-axis joint whose axes 1, 2 and 3 are these bone axes.
    ThreeAxis([Axis; 3]),
}

/// A bone's chunk-16 record (section 1), bind pose aside (`model::Bone`).
#[derive(Clone, Debug, PartialEq)]
pub struct BonePhysics {
    /// 1 in every shipped record.
    pub version: u32,
    /// The game material's name, as stored.
    pub material: String,
    pub shape: Shape,
    pub joint: JointType,
    /// Axis records x, y, z.
    pub limits: [AxisLimits; 3],
    /// Joint stiffness and damping factors (section 3.3).
    pub spring: f32,
    pub damping: f32,
    /// Bits as stored; only [`BREAKABLE`] is tested.
    pub joint_flags: u32,
    pub break_force: f32,
    pub break_torque: f32,
    /// Present only if the version's low 16 bits are not 0; not used.
    pub friction: Option<f32>,
    pub mass: f32,
    /// In the bone's own space.
    pub centre_of_mass: [f32; 3],
}

impl BonePhysics {
    pub fn breakable(&self) -> bool {
        self.joint_flags & BREAKABLE != 0
    }

    pub fn limits(&self, axis: Axis) -> &AxisLimits {
        &self.limits[axis as usize]
    }

    /// For joint type 2, which joint the equal limits make (section 3.2).
    pub fn limited_joint(&self) -> Option<LimitedJoint> {
        use Axis::*;
        if self.joint != JointType::Limited {
            return None;
        }
        let [x, y, z] = self.limits.map(|l| l.equal());
        Some(match (x, y, z) {
            (_, true, true) if !x => LimitedJoint::Hinge(X),
            (true, false, true) => LimitedJoint::Hinge(Y),
            (true, true, _) => LimitedJoint::Hinge(Z),
            (false, false, true) => LimitedJoint::ThreeAxis([X, Z, Y]),
            (false, true, false) => LimitedJoint::ThreeAxis([Z, Y, X]),
            _ => LimitedJoint::ThreeAxis([Z, X, Y]),
        })
    }
}

/// Reads one record (section 1): its physics data, bind rotation and bind
/// position.
pub(crate) fn read_record(d: &mut Reader) -> io::Result<(BonePhysics, [f32; 3], [f32; 3])> {
    let version = d.u32()?;
    let material = d.string()?;
    let kind = match d.u16()? {
        0 => ShapeType::None,
        1 => ShapeType::Box,
        2 => ShapeType::Sphere,
        3 => ShapeType::Cylinder,
        t => return Err(invalid(format!("bone shape type {t}"))),
    };
    let flags = d.u16()?;
    let cuboid = BoxShape {
        axes: [d.float3()?, d.float3()?, d.float3()?],
        centre: d.float3()?,
        half_size: d.float3()?,
    };
    let sphere = Sphere {
        centre: d.float3()?,
        radius: d.float()?,
    };
    let cylinder = Cylinder {
        centre: d.float3()?,
        direction: d.float3()?,
        height: d.float()?,
        radius: d.float()?,
    };
    let joint = match d.u32()? {
        0 => JointType::Rigid,
        1 => JointType::Ball,
        2 => JointType::Limited,
        3 => JointType::TwoAxisHinge,
        4 => JointType::None,
        5 => JointType::Slider,
        t => return Err(invalid(format!("bone joint type {t}"))),
    };
    let mut limits = [AxisLimits::default(); 3];
    for l in &mut limits {
        *l = AxisLimits {
            low: d.float()?,
            high: d.float()?,
            spring_factor: d.float()?,
            damping_factor: d.float()?,
        };
    }
    let spring = d.float()?;
    let damping = d.float()?;
    let joint_flags = d.u32()?;
    let break_force = d.float()?;
    let break_torque = d.float()?;
    let friction = if version & 0xFFFF != 0 {
        Some(d.float()?)
    } else {
        None
    };
    let bind_rotation = d.float3()?;
    let bind_position = d.float3()?;
    let mass = d.float()?;
    let centre_of_mass = d.float3()?;
    let physics = BonePhysics {
        version,
        material,
        shape: Shape {
            kind,
            flags,
            cuboid,
            sphere,
            cylinder,
        },
        joint,
        limits,
        spring,
        damping,
        joint_flags,
        break_force,
        break_torque,
        friction,
        mass,
        centre_of_mass,
    };
    Ok((physics, bind_rotation, bind_position))
}

/// The load-time check of breakable bones (section 3.4), for bones with
/// `parents`, their `physics` and the child meshes' triangles (`meshes`,
/// each yielding the bones its triangles' vertices name): if any bone is
/// breakable with a joint type other than 4 and a mesh's bones lie in more
/// than one part, the breakable bit is cleared on every bone. Returns
/// whether it was.
pub fn check_breakable(
    parents: &[Option<usize>],
    physics: &mut [&mut BonePhysics],
    meshes: impl IntoIterator<Item = impl IntoIterator<Item = usize>>,
) -> bool {
    if !physics
        .iter()
        .any(|p| p.breakable() && p.joint != JointType::None)
    {
        return false;
    }
    let part = parts(parents, |i| physics[i].breakable());
    let split = meshes.into_iter().any(|bones| {
        let mut first = None;
        bones.into_iter().any(|b| {
            let p = part.get(b).copied().flatten();
            *first.get_or_insert(p) != p
        })
    });
    if split {
        for p in physics.iter_mut() {
            p.joint_flags &= !BREAKABLE;
        }
    }
    split
}

/// Each bone's part (section 3.4 step 1): depth-first from the root,
/// children in bone order, the root in part 0, each `starts` bone in a new
/// part, every other bone in its parent's; `None` for a bone the root does
/// not reach.
fn parts(parents: &[Option<usize>], starts: impl Fn(usize) -> bool) -> Vec<Option<usize>> {
    let mut part = vec![None; parents.len()];
    let mut next = 0;
    let mut stack: Vec<usize> = (0..parents.len())
        .filter(|&i| parents[i].is_none())
        .take(1)
        .collect();
    while let Some(i) = stack.pop() {
        part[i] = Some(match parents[i] {
            None => 0,
            Some(_) if starts(i) => {
                next += 1;
                next
            }
            Some(p) => part[p].expect("parent visited first"),
        });
        // Pushed in reverse so that the first child is visited first.
        let children = (0..parents.len()).filter(|&c| parents[c] == Some(i) && part[c].is_none());
        stack.extend(children.collect::<Vec<_>>().into_iter().rev());
    }
    part
}

/// The index in `library` of a bone's material (section 6): looked up by
/// name ignoring case, an empty name giving [`DEFAULT_MATERIAL`]. A
/// material without the dynamic flag is the game's fatal error; so (our
/// reading: unestablished, none occurs) is a name the library lacks.
pub fn material(name: &str, library: &[GameMaterial]) -> io::Result<usize> {
    let name = if name.is_empty() {
        DEFAULT_MATERIAL
    } else {
        name
    };
    let i = library
        .iter()
        .position(|m| m.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| invalid(format!("no game material {name}")))?;
    if !library[i].has(gamemtl::DYNAMIC) {
        return Err(invalid(if name == DEFAULT_MATERIAL {
            "'default_object' - must be dynamic".to_owned()
        } else {
            format!("Required dynamic game material: {name}")
        }));
    }
    Ok(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn record(physics: &BonePhysics) -> Vec<u8> {
        let mut b = physics.version.to_le_bytes().to_vec();
        b.extend(physics.material.as_bytes());
        b.push(0);
        b.extend((physics.shape.kind as u16).to_le_bytes());
        b.extend(physics.shape.flags.to_le_bytes());
        let s = &physics.shape;
        let mut f: Vec<f32> = s.cuboid.axes.concat();
        f.extend(s.cuboid.centre);
        f.extend(s.cuboid.half_size);
        f.extend(s.sphere.centre);
        f.push(s.sphere.radius);
        f.extend(s.cylinder.centre);
        f.extend(s.cylinder.direction);
        f.extend([s.cylinder.height, s.cylinder.radius]);
        b.extend(f.iter().flat_map(|v| v.to_le_bytes()));
        b.extend((physics.joint as u32).to_le_bytes());
        for l in &physics.limits {
            for v in [l.low, l.high, l.spring_factor, l.damping_factor] {
                b.extend(v.to_le_bytes());
            }
        }
        for v in [physics.spring, physics.damping] {
            b.extend(v.to_le_bytes());
        }
        b.extend(physics.joint_flags.to_le_bytes());
        for v in [physics.break_force, physics.break_torque] {
            b.extend(v.to_le_bytes());
        }
        b.extend(physics.friction.iter().flat_map(|v| v.to_le_bytes()));
        for v in [0.1f32, 0.2, 0.3, 1.0, 2.0, 3.0, physics.mass] {
            b.extend(v.to_le_bytes());
        }
        b.extend(physics.centre_of_mass.iter().flat_map(|v| v.to_le_bytes()));
        b
    }

    fn limits(x: (f32, f32), y: (f32, f32), z: (f32, f32)) -> [AxisLimits; 3] {
        [x, y, z].map(|(low, high)| AxisLimits {
            low,
            high,
            spring_factor: 1.0,
            damping_factor: 1.0,
        })
    }

    pub(crate) fn sample() -> BonePhysics {
        BonePhysics {
            version: 1,
            material: "objects\\large_furniture".into(),
            shape: Shape {
                kind: ShapeType::Box,
                flags: shape_flags::REMOVABLE,
                cuboid: BoxShape {
                    axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    centre: [0.0, 1.0, 0.5],
                    half_size: [0.5, 1.0, 0.002],
                },
                sphere: Sphere {
                    centre: [0.0; 3],
                    radius: 0.25,
                },
                cylinder: Cylinder {
                    centre: [0.0; 3],
                    direction: [0.0, 1.0, 0.0],
                    height: 2.0,
                    radius: 0.5,
                },
            },
            joint: JointType::Limited,
            // The door of door_trader (section 3.2): x and z limits 0.
            limits: limits((0.0, 0.0), (-1.9199, 0.0), (0.0, 0.0)),
            spring: 1.0,
            damping: 2.0,
            joint_flags: 0,
            break_force: 0.0,
            break_torque: 0.0,
            friction: Some(1.0),
            mass: 10.0,
            centre_of_mass: [0.0, 1.0, 0.5],
        }
    }

    #[test]
    fn reads_records() {
        let want = sample();
        let bytes = record(&want);
        // 4 + n + 228 with friction (section 1).
        assert_eq!(bytes.len(), 4 + want.material.len() + 1 + 228);
        let mut r = Reader::new(&bytes);
        let (got, rotation, position) = read_record(&mut r).unwrap();
        r.finish("record").unwrap();
        assert_eq!(
            (got, rotation, position),
            (want, [0.1, 0.2, 0.3], [1.0, 2.0, 3.0])
        );

        // Low 16 bits 0: no friction, whatever the high bits.
        let mut without = sample();
        without.version = 0x0001_0000;
        without.friction = None;
        let bytes = record(&without);
        let mut r = Reader::new(&bytes);
        assert_eq!(read_record(&mut r).unwrap().0, without);
        r.finish("record").unwrap();
    }

    #[test]
    fn rejects_unknown_types() {
        let mut bytes = record(&sample());
        let shape = 4 + "objects\\large_furniture".len() + 1;
        bytes[shape..shape + 2].copy_from_slice(&4u16.to_le_bytes());
        assert!(read_record(&mut Reader::new(&bytes)).is_err(), "shape 4");
        let mut bytes = record(&sample());
        bytes[shape + 112..shape + 116].copy_from_slice(&6u32.to_le_bytes());
        assert!(read_record(&mut Reader::new(&bytes)).is_err(), "joint 6");
    }

    #[test]
    fn used_shapes_and_volumes() {
        let mut p = sample();
        let Some(UsedShape::Box(b)) = p.shape.used() else {
            panic!("box");
        };
        // The half size 0.002 raised to 0.005 (section 2.3).
        assert_eq!(b.collision_half_size(), [0.5, 1.0, 0.005]);
        assert!((UsedShape::Box(b).volume() - 8.0 * 0.5 * 0.005).abs() < 1e-7);
        p.shape.kind = ShapeType::Sphere;
        let v = p.shape.used().unwrap().volume();
        assert!((v - 4.0 / 3.0 * PI / 64.0).abs() < 1e-6);
        p.shape.kind = ShapeType::Cylinder;
        assert!((p.shape.used().unwrap().volume() - PI * 0.5).abs() < 1e-6);
        p.shape.flags |= shape_flags::NO_PHYSICS;
        assert_eq!(p.shape.used(), None);
        p.shape.flags = 0;
        p.shape.kind = ShapeType::None;
        assert_eq!(p.shape.used(), None);
    }

    #[test]
    fn limited_joints() {
        use Axis::*;
        let mut p = sample();
        // door_trader's door: a hinge about y, −1.9199 … 0.
        assert_eq!(p.limited_joint(), Some(LimitedJoint::Hinge(Y)));
        assert_eq!(p.limits(Y).kept(), (-1.9199, 0.0));
        let open = (-0.5, 0.5);
        let shut = (0.0, 0.0);
        for (l, want) in [
            (limits(open, shut, shut), LimitedJoint::Hinge(X)),
            (limits(shut, shut, open), LimitedJoint::Hinge(Z)),
            (limits(shut, shut, shut), LimitedJoint::Hinge(Z)),
            (limits(open, open, shut), LimitedJoint::ThreeAxis([X, Z, Y])),
            (limits(open, shut, open), LimitedJoint::ThreeAxis([Z, Y, X])),
            (limits(open, open, open), LimitedJoint::ThreeAxis([Z, X, Y])),
            (limits(shut, open, open), LimitedJoint::ThreeAxis([Z, X, Y])),
        ] {
            p.limits = l;
            assert_eq!(p.limited_joint(), Some(want), "{l:?}");
        }
        // Within 10⁻⁵ is equal.
        p.limits = limits((0.0, 0.000_009), open, (0.0, 0.0));
        assert_eq!(p.limited_joint(), Some(LimitedJoint::Hinge(Y)));
        p.joint = JointType::Ball;
        assert_eq!(p.limited_joint(), None);
    }

    #[test]
    fn kept_limits() {
        let l = |low, high| AxisLimits {
            low,
            high,
            spring_factor: 1.0,
            damping_factor: 1.0,
        };
        // door_yantar_u_01's slide, −3.6 … 0 (section 3.2).
        let (low, high) = l(-3.6, 0.0).kept();
        assert_eq!(low, -PI);
        assert!((high - 0.4584).abs() < 1e-4, "{high}");
        assert_eq!(l(-1.0, 4.0).kept(), (-1.0 - (4.0 - PI), PI));
        assert_eq!(l(-2.0, 0.0).kept(), (-2.0, 0.0));
        // Exactly 2π (−π … π) leaves the axis free.
        assert!(!l(-PI, PI).limited());
        assert!(l(-3.0, 3.0).limited());
    }

    #[test]
    fn breakable_check() {
        // root → a (breakable) → b; root → c.
        let parents = [None, Some(0), Some(1), Some(0)];
        let make = |breakable: [bool; 4], joint: JointType| -> Vec<BonePhysics> {
            breakable
                .iter()
                .map(|&b| BonePhysics {
                    joint,
                    joint_flags: u32::from(b),
                    ..sample()
                })
                .collect()
        };
        let run = |mut bones: Vec<BonePhysics>, meshes: &[&[usize]]| {
            let mut refs: Vec<&mut BonePhysics> = bones.iter_mut().collect();
            let cleared = check_breakable(
                &parents,
                &mut refs,
                meshes.iter().map(|m| m.iter().copied()),
            );
            (
                cleared,
                bones.iter().map(BonePhysics::breakable).collect::<Vec<_>>(),
            )
        };
        let flags = [false, true, false, false];
        // a and b are one part, root and c another: meshes within a part
        // keep the bit.
        assert_eq!(
            run(make(flags, JointType::Rigid), &[&[1, 2], &[0, 3]]),
            (false, vec![false, true, false, false])
        );
        // A mesh over root and a: every bit cleared.
        assert_eq!(
            run(
                make([true, true, false, false], JointType::Rigid),
                &[&[1, 2], &[0, 1]]
            ),
            (true, vec![false; 4])
        );
        // Joint type 4 on every breakable bone: no check.
        assert_eq!(
            run(make(flags, JointType::None), &[&[0, 1]]),
            (false, vec![false, true, false, false])
        );
    }

    #[test]
    fn materials() {
        let m = |name: &str, flags| GameMaterial {
            name: name.into(),
            flags,
            ..GameMaterial::default()
        };
        let lib = [
            m("materials\\concrete", 0),
            m("default_object", gamemtl::DYNAMIC),
            m("objects\\glass", gamemtl::DYNAMIC),
        ];
        assert_eq!(material("", &lib).unwrap(), 1);
        assert_eq!(material("Objects\\Glass", &lib).unwrap(), 2);
        let e = material("materials\\concrete", &lib).unwrap_err();
        assert!(
            e.to_string().contains("Required dynamic game material"),
            "{e}"
        );
        assert!(material("objects\\none", &lib).is_err());
        let lib = [m("default_object", 0)];
        let e = material("", &lib).unwrap_err();
        assert_eq!(e.to_string(), "'default_object' - must be dynamic");
    }
}
