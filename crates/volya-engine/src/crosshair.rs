// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What is under the crosshair (docs/behavior/use-action.md, cited by
//! section): the crosshair's pick (2.1), a ray from the camera against
//! the objects and the level mesh, and the crosshair record the player's
//! update keeps from it (2.2), with when its object came under the
//! crosshair (trader-door.md 5.3), for the HUD's hint and the use action.
//!
//! Which shapes of an object the pick tests is not stated by 2.1; the
//! engine's object ray query is glows.md 5.1's (our reading: the same
//! query): an object's skeleton collision form, hit when the ray passes
//! its bounding sphere and one of its bones' shapes (box, sphere or
//! cylinder, in the current pose) from either side. The objects with a
//! physics shell are tested by their shell's boxes where its bodies hold
//! them ([`PickShape::Box`]).

use crate::core::collision::CollisionWorld;
use crate::core::collision::body::BodyBox;
use crate::core::collision::geometry::{self, Vec3, dot, scale, sub};
use volya_engine_formats::bone_physics::UsedShape;
use volya_engine_formats::gamemtl::PASS_THROUGH;
use volya_engine_formats::model::{Mat4, transform_point};

/// The ray's length over the environment's far value `Z` (2.1).
pub const RAY_LENGTH: f32 = 0.99;

/// The shortest distance a hit is recorded at (2.1).
pub const NEAREST: f32 = 0.5;

/// A shape of an object's collision form in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PickShape {
    Box(BodyBox),
    Sphere {
        centre: Vec3,
        radius: f32,
    },
    /// A cylinder with flat ends: its centre, unit axis direction, length
    /// along the axis and radius.
    Cylinder {
        centre: Vec3,
        direction: Vec3,
        height: f32,
        radius: f32,
    },
}

impl PickShape {
    /// Bone shape `shape` (in its bone's own space) placed by the bone's
    /// model-space matrix `bone` and the model's world matrix `model`.
    pub fn of_bone(shape: &UsedShape, bone: &Mat4, model: &Mat4) -> Self {
        let point = |p: Vec3| transform_point(model, transform_point(bone, p));
        let dir = |v: Vec3| {
            let w = sub(point(v), point([0.0; 3]));
            geometry::normalize(w).unwrap_or(w)
        };
        match shape {
            UsedShape::Box(b) => Self::Box(BodyBox {
                centre: point(b.centre),
                axes: b.axes.map(dir),
                half_size: b.collision_half_size(),
                material: 0,
            }),
            UsedShape::Sphere(s) => Self::Sphere {
                centre: point(s.centre),
                radius: s.radius,
            },
            UsedShape::Cylinder(c) => Self::Cylinder {
                centre: point(c.centre),
                direction: dir(c.direction),
                height: c.height,
                radius: c.radius,
            },
        }
    }

    /// The distance along the unit `dir` from `origin` at which the ray
    /// first meets the shape, 0 from inside.
    pub fn ray(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        match *self {
            Self::Box(b) => b.ray(origin, dir),
            Self::Sphere { centre, radius } => ray_sphere(origin, dir, centre, radius),
            Self::Cylinder {
                centre,
                direction,
                height,
                radius,
            } => ray_cylinder(origin, dir, centre, direction, height, radius),
        }
    }
}

/// The distance at which the ray meets the sphere, 0 from inside.
fn ray_sphere(origin: Vec3, dir: Vec3, centre: Vec3, radius: f32) -> Option<f32> {
    let d = sub(origin, centre);
    let c = dot(d, d) - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let b = dot(d, dir);
    let disc = b * b - c;
    if b > 0.0 || disc < 0.0 {
        return None;
    }
    Some(-b - disc.sqrt())
}

/// The distance at which the ray meets the cylinder with flat ends, 0
/// from inside.
fn ray_cylinder(
    origin: Vec3,
    dir: Vec3,
    centre: Vec3,
    axis: Vec3,
    height: f32,
    radius: f32,
) -> Option<f32> {
    let half = 0.5 * height;
    let d = sub(origin, centre);
    // Along the axis: the slab |a| ≤ half.
    let (a0, av) = (dot(d, axis), dot(dir, axis));
    let (mut near, mut far) = (0.0f32, f32::INFINITY);
    if av.abs() < 1e-12 {
        if a0.abs() > half {
            return None;
        }
    } else {
        let (t0, t1) = ((-half - a0) / av, (half - a0) / av);
        near = near.max(t0.min(t1));
        far = far.min(t0.max(t1));
    }
    // Across it: the infinite cylinder.
    let p = sub(d, scale(axis, a0));
    let v = sub(dir, scale(axis, av));
    let (a, b, c) = (dot(v, v), dot(p, v), dot(p, p) - radius * radius);
    if a < 1e-12 {
        if c > 0.0 {
            return None;
        }
    } else {
        let disc = b * b - a * c;
        if disc < 0.0 {
            return None;
        }
        let s = disc.sqrt();
        near = near.max((-b - s) / a);
        far = far.min((-b + s) / a);
    }
    (near <= far).then_some(near)
}

/// An object the pick tests: its id, its bounding sphere (centre, radius)
/// if its form has one, and its shapes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PickObject {
    pub id: u16,
    pub sphere: Option<[f32; 4]>,
    pub shapes: Vec<PickShape>,
}

impl PickObject {
    /// The distance at which the ray meets it: within its bounding sphere,
    /// the nearest of its shapes.
    fn ray(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        if let Some([x, y, z, r]) = self.sphere {
            ray_sphere(origin, dir, [x, y, z], r)?;
        }
        (self.shapes.iter())
            .filter_map(|s| s.ray(origin, dir))
            .min_by(f32::total_cmp)
    }
}

/// The crosshair's pick (2.1): the object hit (none for the level mesh or
/// nothing) and its distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    pub object: Option<u16>,
    pub distance: f32,
}

/// The crosshair's pick (2.1) from `eye` along the unit `dir` for
/// `length` metres against `level` (none: no level mesh) and `objects`
/// (the controlled object left out by the caller): the nearest hit, a
/// level-mesh triangle of a pass-through material (flag 0x80) passed
/// through; at a distance of at least [`NEAREST`]. A level triangle is
/// hit from either side (our choice: 2.1 does not say), and a tie goes to
/// the level mesh, then to the earlier object (our choice).
pub fn pick(
    level: Option<&CollisionWorld>,
    eye: Vec3,
    dir: Vec3,
    length: f32,
    objects: impl IntoIterator<Item = PickObject>,
) -> Pick {
    let mesh = level.and_then(|w| {
        w.ray_nearest(eye, dir, length, |t| {
            if w.material(t).has(PASS_THROUGH) {
                return None;
            }
            geometry::ray_triangle_distance(eye, dir, &w.corners(t))
                .filter(|d| (0.0..=length).contains(d))
        })
    });
    let mut best = Pick {
        object: None,
        distance: mesh.map_or(length, |(d, _)| d),
    };
    for o in objects {
        if let Some(d) = o.ray(eye, dir).filter(|&d| d < best.distance) {
            best = Pick {
                object: Some(o.id),
                distance: d,
            };
        }
    }
    best.distance = best.distance.max(NEAREST);
    best
}

/// The object of the crosshair record (2.2): its id, whether it is a
/// character, and the engine clock when it came under the crosshair
/// (trader-door.md 5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordObject {
    pub id: u16,
    pub character: bool,
    pub since: u64,
}

/// The crosshair record (2.2): the object under the crosshair, which
/// exists only closer than `[inventory] take_dist`. Its parts as an
/// inventory box, an item and a holder have no reader here yet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CrosshairRecord {
    pub object: Option<RecordObject>,
}

impl CrosshairRecord {
    /// The record from this frame's `pick` at engine clock `now`: cleared
    /// when the pick has no object or its distance is at least
    /// `take_dist`; `character` tells whether an object is a character.
    /// An object that stays under the crosshair keeps its clock.
    pub fn update(
        &mut self,
        pick: &Pick,
        take_dist: f32,
        now: u64,
        character: impl FnOnce(u16) -> bool,
    ) {
        self.object = match pick.object.filter(|_| pick.distance < take_dist) {
            None => None,
            Some(id) => Some(match self.object {
                Some(o) if o.id == id => o,
                _ => RecordObject {
                    id,
                    character: character(id),
                    since: now,
                },
            }),
        };
    }

    /// The id of the object under the crosshair.
    pub fn id(&self) -> Option<u16> {
        self.object.map(|o| o.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::bone_physics::{BoxShape, Cylinder, Sphere};
    use volya_engine_formats::level::IDENTITY;

    const EYE: Vec3 = [0.0, 1.5, 0.0];
    const AHEAD: Vec3 = [0.0, 0.0, 1.0];

    fn sphere_at(id: u16, z: f32) -> PickObject {
        PickObject {
            id,
            sphere: None,
            shapes: vec![PickShape::Sphere {
                centre: [0.0, 1.5, z],
                radius: 0.25,
            }],
        }
    }

    #[test]
    fn the_nearest_object_is_picked() {
        let p = pick(
            None,
            EYE,
            AHEAD,
            100.0,
            [sphere_at(3, 4.0), sphere_at(2, 1.5)],
        );
        assert_eq!(p.object, Some(2));
        assert!((p.distance - 1.25).abs() < 1e-5, "{p:?}");
        // Behind the eye or off the ray: nothing.
        let p = pick(None, EYE, AHEAD, 100.0, [sphere_at(3, -2.0)]);
        assert_eq!(
            p,
            Pick {
                object: None,
                distance: 100.0
            }
        );
        // Beyond the ray's length: nothing.
        let p = pick(None, EYE, AHEAD, 3.0, [sphere_at(3, 4.0)]);
        assert_eq!(p.object, None);
    }

    /// Two walls across the ray, z = 1 of a pass-through material, z = 3
    /// of a solid one.
    fn walls() -> CollisionWorld {
        use crate::core::collision::{Cform, CformTriangle};
        use volya_engine_formats::gamemtl::GameMaterial;
        let wall = |z: f32| [[-5.0, -5.0, z], [5.0, -5.0, z], [0.0, 10.0, z]];
        let mut vertices = wall(1.0).to_vec();
        vertices.extend(wall(3.0));
        let triangles = vec![
            CformTriangle {
                vertices: [0, 1, 2],
                attributes: 1,
            },
            CformTriangle {
                vertices: [3, 4, 5],
                attributes: 0,
            },
        ];
        let cform = Cform {
            bbox_min: [-5.0, -5.0, 0.0],
            bbox_max: [5.0, 10.0, 4.0],
            vertices,
            triangles,
        };
        let material = |id, flags| GameMaterial {
            id,
            flags,
            ..GameMaterial::default()
        };
        CollisionWorld::new(cform, vec![material(0, 0), material(1, PASS_THROUGH)]).unwrap()
    }

    #[test]
    fn the_level_mesh_ends_the_search_unless_passed_through() {
        let w = walls();
        let p = pick(Some(&w), EYE, AHEAD, 100.0, [sphere_at(3, 4.0)]);
        assert_eq!(p.object, None);
        assert!((p.distance - 3.0).abs() < 1e-5, "{p:?}");
        // Behind the pass-through wall, in front of the solid one.
        let p = pick(Some(&w), EYE, AHEAD, 100.0, [sphere_at(3, 2.0)]);
        assert_eq!(p.object, Some(3));
    }

    #[test]
    fn a_hit_closer_than_half_a_metre_is_recorded_there() {
        let p = pick(None, EYE, AHEAD, 100.0, [sphere_at(5, 0.3)]);
        assert_eq!(
            p,
            Pick {
                object: Some(5),
                distance: NEAREST
            }
        );
    }

    #[test]
    fn the_bounding_sphere_gates_the_shapes() {
        let mut o = sphere_at(4, 2.0);
        o.sphere = Some([5.0, 1.5, 2.0, 1.0]);
        assert_eq!(pick(None, EYE, AHEAD, 10.0, [o.clone()]).object, None);
        o.sphere = Some([0.0, 1.5, 2.0, 1.0]);
        assert_eq!(pick(None, EYE, AHEAD, 10.0, [o]).object, Some(4));
    }

    #[test]
    fn bone_shapes_are_placed_by_the_bone_and_the_model() {
        let mut model = IDENTITY;
        model[3] = [0.0, 0.0, 3.0, 1.0];
        let mut bone = IDENTITY;
        bone[3] = [0.0, 1.5, 0.0, 1.0];
        let b = UsedShape::Box(BoxShape {
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            centre: [0.0, 0.0, 0.0],
            half_size: [0.2, 0.2, 0.2],
        });
        let s = PickShape::of_bone(&b, &bone, &model);
        assert!((s.ray(EYE, AHEAD).unwrap() - 2.8).abs() < 1e-5);
        let c = UsedShape::Cylinder(Cylinder {
            centre: [0.0, 0.0, 0.0],
            direction: [0.0, 1.0, 0.0],
            height: 1.0,
            radius: 0.3,
        });
        let s = PickShape::of_bone(&c, &bone, &model);
        assert!((s.ray(EYE, AHEAD).unwrap() - 2.7).abs() < 1e-5);
        // Past its end along the axis: missed.
        assert_eq!(s.ray([0.0, 2.1, 0.0], AHEAD), None);
        // Along its axis from below: its end.
        let t = s.ray([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]).unwrap();
        assert!((t - 1.0).abs() < 1e-5, "{t}");
        let sp = UsedShape::Sphere(Sphere {
            centre: [0.0, 0.0, 0.5],
            radius: 0.1,
        });
        let s = PickShape::of_bone(&sp, &bone, &model);
        assert!((s.ray(EYE, AHEAD).unwrap() - 3.4).abs() < 1e-5);
        // From inside: 0.
        assert_eq!(s.ray([0.0, 1.5, 3.5], AHEAD), Some(0.0));
    }

    #[test]
    fn the_record_holds_an_object_closer_than_take_dist() {
        let mut r = CrosshairRecord::default();
        let at = |object, distance| Pick { object, distance };
        r.update(&at(Some(7), 1.0), 2.0, 5, |id| id == 7);
        assert_eq!(
            r.object,
            Some(RecordObject {
                id: 7,
                character: true,
                since: 5
            })
        );
        // Still under it: its clock kept.
        r.update(&at(Some(7), 1.5), 2.0, 9, |_| false);
        assert_eq!(r.object.unwrap().since, 5);
        // Another object: its own clock.
        r.update(&at(Some(8), 1.5), 2.0, 11, |_| false);
        assert_eq!(
            r.object,
            Some(RecordObject {
                id: 8,
                character: false,
                since: 11
            })
        );
        // At take_dist or with no object: cleared.
        r.update(&at(Some(8), 2.0), 2.0, 12, |_| false);
        assert_eq!(r.id(), None);
        r.update(&at(Some(8), 1.0), 2.0, 13, |_| false);
        r.update(&at(None, 1.0), 2.0, 14, |_| false);
        assert_eq!(r.id(), None);
    }
}
