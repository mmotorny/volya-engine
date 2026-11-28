// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's body as an object of the physics world
//! (docs/behavior/physics-contacts.md 5, 7): class 0x4 | 0x10, its four
//! shapes (bottom sphere, torso cylinder, head sphere, outer sphere;
//! level-cform.md 5.1) meeting the movable bodies, the outer sphere's
//! contacts only setting "blocked" (player-body-contacts.md 6), the others
//! under the player's rules (player-body-contacts.md 2.2) made joints of the player's body
//! and the movable one; every contact with a movable body reaching the rules
//! sets the touch flag (step-assistance-movable-bodies.md 2.1); the torso
//! cylinder's by the game's cylinder–box and cylinder–sphere colliders
//! (physics-contacts.md 4.3).
//!
//! The player's body is the movement's own solver
//! (docs/architecture/simulation.md), so in the library it is a stand-in
//! body of the player's mass that never turns, placed at each world step
//! where the movement step began, at the velocity it left, without
//! gravity: the movable body receives the reaction, and the change the
//! contacts made to the stand-in's velocity goes back to the player's body
//! ([`crate::collision::body::PlayerBody::react`]). The player's level
//! contacts stay the movement's (none here meets the level).

use super::collide::{Draft, Geom, bounds_of};
use super::{BodyId, DMass, DxBody, GroupId, Grouped, OdeSolver, Real, STEPPING};
use crate::collision::body::{
    self, OUTER_HEIGHT, OUTER_RADIUS, Reaction, Shape as Part, WorldStep,
};
use crate::collision::geometry::{Vec3, add, scale, sub};
use crate::physics::contacts::{Bounds, Class};
use crate::physics::shell::Shape;
use crate::physics::{Kind, Object, Params};

unsafe extern "C" {
    fn dBodyCreate(w: *mut super::DxWorld) -> *mut DxBody;
    fn dBodySetMass(b: *mut DxBody, mass: *const DMass);
    fn dBodySetPosition(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetRotation(b: *mut DxBody, r: *const Real);
    fn dBodySetLinearVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetAngularVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetGravityMode(b: *mut DxBody, mode: std::ffi::c_int);
    fn dMassSetSphereTotal(m: *mut DMass, total: Real, radius: Real);
}

/// The stand-in's inertia is its sphere's times this: it does not turn.
const STIFF_INERTIA: f32 = 1e8;
/// The outer sphere's position among the shapes.
const OUTER: usize = 3;
const PARTS: [Part; 3] = [Part::Bottom, Part::Torso, Part::Head];

/// The player's body in the world.
#[derive(Debug)]
pub struct OdeCharacter {
    serial: u64,
    group: GroupId,
    body: BodyId,
    /// The movement step this world step meets, taken by its solving.
    step: Option<WorldStep>,
    geoms: Vec<Geom>,
    bounds: Option<Bounds>,
    /// The solving's change to the stand-in's velocity, whether a contact
    /// set "blocked", the touch flag and the contacts the rules handled as
    /// made, since last taken.
    reaction: Reaction,
    /// The contacts its rules kept, since it was made.
    kept: u64,
}

impl OdeSolver {
    /// The player's body of mass `mass` in a group of its own.
    pub fn add_character(&mut self, mass: f32) -> OdeCharacter {
        let group = self.add_group();
        let g = self.group_mut(group.0);
        let mut m = DMass::default();
        // SAFETY: the world is live; `m` is a valid dMass.
        let body = unsafe {
            let b = dBodyCreate(g.world);
            dMassSetSphereTotal(&mut m, mass, 0.5);
            for k in [0, 5, 10] {
                m.i[k] *= STIFF_INERTIA;
            }
            dBodySetMass(b, &m);
            dBodySetGravityMode(b, 0);
            b
        };
        g.own.push(body);
        g.bodies += 1;
        let id = BodyId(body);
        self.bodies.insert(id, group);
        OdeCharacter {
            serial: self.serial,
            group,
            body: id,
            step: None,
            geoms: Vec::new(),
            bounds: None,
            reaction: Reaction::default(),
            kept: 0,
        }
    }
}

impl OdeCharacter {
    pub fn body(&self) -> BodyId {
        self.body
    }

    /// The movement step the next world step meets: the stand-in placed
    /// where it began, at the velocity it left, and the shapes there.
    pub fn set_step(&mut self, solver: &mut OdeSolver, step: WorldStep) {
        if !solver.live(self.serial, self.group) {
            return;
        }
        let c = step.centre;
        let v = step.velocity;
        let identity = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        {
            let _turn = STEPPING.lock().unwrap_or_else(|e| e.into_inner());
            // SAFETY: the body lives while its group does.
            unsafe {
                dBodySetPosition(self.body.0, c[0], c[1], c[2]);
                dBodySetRotation(self.body.0, identity.as_ptr());
                dBodySetLinearVel(self.body.0, v[0], v[1], v[2]);
                dBodySetAngularVel(self.body.0, 0.0, 0.0, 0.0);
            }
        }
        let s = step.shapes;
        let (torso_at, torso_half) = s.torso();
        let up = [0.0, 1.0, 0.0];
        let geom = |shape| Geom {
            shape,
            body: Some(self.body),
            material: step.material,
            meets_level: false,
        };
        self.geoms = vec![
            geom(Shape::Sphere {
                centre: c,
                radius: s.r,
            }),
            geom(Shape::Cylinder {
                centre: add(c, [0.0, torso_at, 0.0]),
                direction: up,
                height: 2.0 * torso_half,
                radius: s.small_radius(),
            }),
            geom(Shape::Sphere {
                centre: add(c, [0.0, s.h, 0.0]),
                radius: s.small_radius(),
            }),
            geom(Shape::Sphere {
                centre: add(c, scale(up, OUTER_HEIGHT * s.r)),
                radius: OUTER_RADIUS * s.r,
            }),
        ];
        self.bounds = bounds_of(&self.geoms);
        self.step = Some(step);
    }

    /// The contacts with movable bodies its rules kept since it was made
    /// (the group limits may still have left some unmade).
    pub fn kept(&self) -> u64 {
        self.kept
    }

    /// What the world's contacts left since last taken: the change to the
    /// velocity, "blocked", the touch flag and the count of contacts made.
    pub fn take_reaction(&mut self) -> Reaction {
        std::mem::take(&mut self.reaction)
    }
}

impl Grouped for OdeCharacter {
    fn group(&self) -> GroupId {
        self.group
    }

    fn after_solve(&mut self, solver: &OdeSolver) {
        let Some(step) = self.step.take() else {
            return;
        };
        if let Some(v) = solver.linear_velocity(self.body) {
            self.reaction.velocity = add(self.reaction.velocity, sub(v, step.velocity));
        }
        self.geoms.clear();
        self.bounds = None;
    }

    fn class(&self) -> Class {
        Class::CHARACTER
    }

    fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    fn geometries(&self, _: &OdeSolver, out: &mut Vec<Geom>) {
        out.extend_from_slice(&self.geoms);
    }

    /// The outer sphere's callback (player-body-contacts.md 6): no contact, the
    /// step "blocked".
    fn geometry_contact(&mut self, geom: usize, _: bool, d: &mut Draft) {
        if geom == OUTER {
            d.made = false;
            self.reaction.blocked = true;
        }
    }

    /// The player's rules (player-body-contacts.md 2.2; physics-contacts.md 5: the
    /// normal points towards the player's shape when it is the first
    /// geometry), every contact of its shapes, made or not, reaching them.
    fn object_contact(&mut self, geom: usize, first: bool, d: &mut Draft) {
        let Some(step) = &self.step else {
            return;
        };
        // step-assistance-movable-bodies.md 2.1: the touch flag, unless the
        // other shape's material is pass-through and the contact not made.
        let other_passes = if first {
            d.pass_through.1
        } else {
            d.pass_through.0
        };
        if d.bodies && !(other_passes && !d.made) {
            self.reaction.touch = true;
        }
        let Some(&part) = PARTS.get(geom) else {
            return;
        };
        if !d.made {
            return;
        }
        let n = d.contact.normal;
        let towards = if first { n } else { scale(n, -1.0) };
        let blocked = step.blocked || self.reaction.blocked;
        let s = &mut d.contact.surface;
        let pair = (s.soft_erp, s.soft_cfm, s.mu);
        let at = (towards, d.contact.position, first);
        match body::movable_contact(part, at, pair, blocked, step) {
            None => d.made = false,
            Some(c) => {
                s.soft_erp = c.erp;
                s.soft_cfm = c.cfm;
                if s.mu.is_finite() {
                    s.mu = c.friction;
                }
                if let Some((normal, depth)) = c.replaced {
                    d.contact.normal = if first { normal } else { scale(normal, -1.0) };
                    d.contact.depth = depth;
                }
                self.reaction.blocked |= c.blocked;
                self.reaction.made += 1;
                self.kept += 1;
            }
        }
    }
}

impl Object for OdeCharacter {
    fn kind(&self) -> Kind {
        Kind::Character
    }

    fn has_geometry(&self) -> bool {
        true
    }

    fn height(&self) -> f32 {
        self.step.map_or(f32::INFINITY, |s| s.centre[1])
    }

    fn set_gravity(&mut self, _: bool) {}

    fn add_force(&mut self, _: Vec3) {}

    fn pre_step(&mut self, _: &Params) {}

    fn post_step(&mut self, _: &Params) -> bool {
        false
    }
}

#[cfg(test)]
mod tests;
