// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A [`Shell`] activated in an [`OdeSolver`] (docs/behavior/physics-shells.md,
//! cited by section; `joints` is docs/behavior/physics-joints.md, `resting`
//! docs/behavior/physics-resting.md): its elements' bodies placed and given
//! their masses (2.1, 2.3), the elements without geometry fixed (5.1), its
//! joints made after them (3.3), its fixed bones fixed again (4.4), switched
//! off when asked (`resting` 1), woken by a joint's motor (`resting` 2), and
//! the elements' motion parameters at each step (2.4); forces, fixing and
//! releasing as the scripts ask them (`script` is
//! docs/behavior/script-api-physics-vehicles-helicopter-and-lamps.md).

use std::ffi::c_int;

use super::collide::{Geom, bounds_of};
use super::{
    DMass, DxBody, DxJoint, DxWorld, GroupId, Grouped, JointId, OdeSolver, Real, STEPPING,
    dBodyCreate, dBodyGetPosition, dBodySetGravityMode, dBodySetMass, dBodySetPosition,
    dJointAttach,
};
use crate::collision::body::{WORLD_DAMPING, WORLD_SPRING};
use crate::collision::geometry::Vec3;
use crate::physics::contacts::level::LevelState;
use crate::physics::contacts::{Bounds, Class};
use crate::physics::shell::{IDENTITY, JointKind, MassPiece, Motion, Shape, Shell, turn};
use crate::physics::{Kind, Object, Params};
use volya_engine_formats::model::{Mat4, multiply, transform_point};

unsafe extern "C" {
    fn dBodySetRotation(b: *mut DxBody, r: *const Real);
    fn dBodyGetRotation(b: *mut DxBody) -> *const Real;
    fn dBodySetLinearVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetAngularVel(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyGetLinearVel(b: *mut DxBody) -> *const Real;
    fn dBodyGetAngularVel(b: *mut DxBody) -> *const Real;
    fn dBodySetForce(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodySetTorque(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyAddForce(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyAddTorque(b: *mut DxBody, x: Real, y: Real, z: Real);
    fn dBodyEnable(b: *mut DxBody);
    fn dBodyDisable(b: *mut DxBody);
    fn dBodyIsEnabled(b: *mut DxBody) -> c_int;
    fn dBodyGetMass(b: *mut DxBody, m: *mut DMass);

    fn dMassSetZero(m: *mut DMass);
    fn dMassSetSphere(m: *mut DMass, density: Real, radius: Real);
    fn dMassSetBox(m: *mut DMass, density: Real, lx: Real, ly: Real, lz: Real);
    fn dMassSetCylinder(m: *mut DMass, density: Real, direction: c_int, radius: Real, length: Real);
    fn dMassAdjust(m: *mut DMass, total: Real);
    fn dMassTranslate(m: *mut DMass, x: Real, y: Real, z: Real);
    fn dMassRotate(m: *mut DMass, r: *const Real);
    fn dMassAdd(a: *mut DMass, b: *const DMass);

    fn dJointCreateHinge(w: *mut DxWorld, g: *mut std::ffi::c_void) -> *mut DxJoint;
    fn dJointSetHingeAnchor(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetHingeAxis(j: *mut DxJoint, x: Real, y: Real, z: Real);
    fn dJointSetHingeParam(j: *mut DxJoint, parameter: c_int, value: Real);
    fn dJointGetHingeParam(j: *mut DxJoint, parameter: c_int) -> Real;
    fn dJointGetHingeAngle(j: *mut DxJoint) -> Real;
    fn dJointGetHingeAnchor(j: *mut DxJoint, result: *mut Real);
    fn dJointGetHingeAxis(j: *mut DxJoint, result: *mut Real);
}

/// The library's joint parameters (public 0.5.0's numbering, `library`
/// 5.1).
pub mod param {
    pub const LO_STOP: i32 = 0;
    pub const HI_STOP: i32 = 1;
    pub const VEL: i32 = 2;
    pub const F_MAX: i32 = 3;
    pub const CFM: i32 = 6;
    pub const STOP_ERP: i32 = 7;
    pub const STOP_CFM: i32 = 8;
}

/// A fixed element's mass (5.1).
pub const FIXED_MASS: f32 = 1e8;
/// The sphere a fixed element's mass is made from before it is adjusted:
/// its radius is open (TODO(#947)); nothing acts on a fixed body
/// (5.1), so it changes nothing seen.
const FIXED_SPHERE: (f32, f32) = (1.0, 1.0);

/// `bone` 3.3: a joint's (or its stops') error reduction and constraint
/// force mixing from its spring and damping factors at step `h`, for every
/// joint but the two-axis hinge.
pub fn stiffness(spring: f32, damping: f32, h: f32) -> (f32, f32) {
    let k = WORLD_SPRING * spring * h;
    let d = WORLD_DAMPING * damping;
    (k / (k + d), 1.0 / (k + d))
}

/// ODE's rotation (row-major `dMatrix3`, `world = R · local`) of a frame's
/// rotation in row vectors (`world = local · M`): `R = Mᵀ`.
fn rotation_of(m: &Mat4) -> [Real; 12] {
    let mut r = [0.0; 12];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 4 + j] = m[j][i];
        }
    }
    r
}

/// An element of an activated shell.
#[derive(Clone, Debug)]
struct ElementBody {
    body: *mut DxBody,
    /// Its mass centre in its frame (2.1).
    centre: Vec3,
    fixed: bool,
    motion: Motion,
    /// Its mass as set (2.3; 0 without geometry), and the library's mass
    /// it was given, which releasing it gives back (`script` 4.3).
    mass: f32,
    own: Option<DMass>,
}

/// A shell in the world: its group, bodies and joints.
#[derive(Debug)]
pub struct OdeShell {
    /// Its solver's serial and its group.
    serial: u64,
    group: GroupId,
    elements: Vec<ElementBody>,
    /// Its joints as made: the shell's joint index and the joint.
    joints: Vec<(usize, JointId)>,
    has_geometry: bool,
    /// The y of the first element's body, as last read (`world` 4.5).
    height: f32,
    /// All its bodies switched off, as last read (`world` 4.4).
    resting: bool,
    /// A gravity switch for its next solving (`world` 1.4).
    gravity: Option<bool>,
    /// Its geometries (2.2): the element, the shape in the element's frame
    /// and the element's material; their level states (physics-contacts.md
    /// 3.2, 3.3), its collision class (1.3) and its box as last updated
    /// (1.1).
    geoms: Vec<(usize, Shape, u32)>,
    level: Vec<LevelState>,
    class: Class,
    bounds: Option<Bounds>,
}

impl OdeSolver {
    /// Activates `shell` in a group of its own (2.1; 3.3, 4.4, `resting` 1):
    /// every element's body first, then the joints, then the fixed bones'
    /// elements fixed again, with the world's parameters `params` in force
    /// for the joints' defaults (`library` 5.2, item 3). The group requests
    /// the exact solver when the shell does (3.5).
    pub fn activate_shell(&mut self, params: &Params, shell: &Shell) -> OdeShell {
        let group = self.add_group();
        let world = self.group_mut(group.0).world;
        let _turn = STEPPING.lock().unwrap_or_else(|e| e.into_inner());
        Self::apply(params);
        let mut elements = Vec::with_capacity(shell.elements.len());
        let mut frames = Vec::with_capacity(shell.elements.len());
        for e in &shell.elements {
            let frame = multiply(&e.frame, &shell.transform);
            let centre = if e.has_geometry() {
                e.mass_centre().1
            } else {
                [0.0; 3]
            };
            let at = transform_point(&frame, centre);
            let r = rotation_of(&frame);
            let own = e.has_geometry().then(|| mass(&e.mass_pieces()));
            // SAFETY: the world is this solver's and live; `r` is a dMatrix3.
            let body = unsafe {
                let body = dBodyCreate(world);
                if let Some(m) = &own {
                    dBodySetMass(body, m);
                }
                dBodySetPosition(body, at[0], at[1], at[2]);
                dBodySetRotation(body, r.as_ptr());
                dBodySetLinearVel(body, 0.0, 0.0, 0.0);
                dBodySetAngularVel(body, 0.0, 0.0, 0.0);
                if shell.switched_off {
                    dBodyDisable(body);
                }
                body
            };
            let mut element = ElementBody {
                body,
                centre,
                fixed: false,
                motion: e.motion,
                mass: if e.has_geometry() {
                    e.mass_centre().0
                } else {
                    0.0
                },
                own,
            };
            if !e.has_geometry() {
                fix(&mut element);
            }
            elements.push(element);
            frames.push(frame);
        }
        let mut joints = Vec::new();
        for (index, j) in shell.joints.iter().enumerate() {
            let JointKind::Hinge { axis, limits } = j.kind else {
                // TODO(#750): the other joint kinds, as PH-7 specifies.
                continue;
            };
            let second = &elements[j.second];
            let frame = &frames[j.second];
            let anchor = [frame[3][0], frame[3][1], frame[3][2]];
            let mut dir = [0.0; 3];
            dir[axis as usize] = 1.0;
            let mut along = turn(frame, dir);
            // 3.3: a first element fixed (or without a body) gives no
            // first body and the negated axis.
            let first = &elements[j.first];
            let first = if first.fixed {
                along = along.map(|v| -v);
                std::ptr::null_mut()
            } else {
                first.body
            };
            let (stop_erp, stop_cfm) =
                stiffness(limits.spring_factor, limits.damping_factor, params.step);
            let (_, cfm) = stiffness(j.spring, j.damping, params.step);
            // SAFETY: the world and both bodies are this solver's and live.
            let joint = unsafe {
                let joint = dJointCreateHinge(world, std::ptr::null_mut());
                dJointAttach(joint, first, second.body);
                dJointSetHingeAnchor(joint, anchor[0], anchor[1], anchor[2]);
                dJointSetHingeAxis(joint, along[0], along[1], along[2]);
                if limits.limited() {
                    let (low, high) = limits.kept();
                    dJointSetHingeParam(joint, param::LO_STOP, low);
                    dJointSetHingeParam(joint, param::HI_STOP, high);
                }
                dJointSetHingeParam(joint, param::STOP_ERP, stop_erp);
                dJointSetHingeParam(joint, param::STOP_CFM, stop_cfm);
                // The joint's own error reduction is PH-7's (TODO(#750)).
                dJointSetHingeParam(joint, param::CFM, cfm);
                // `joints` 1: no motor at creation.
                dJointSetHingeParam(joint, param::F_MAX, 0.0);
                dJointSetHingeParam(joint, param::VEL, 0.0);
                joint
            };
            let id = JointId(joint);
            let g = self.group_mut(group.0);
            g.own_joints.push(joint);
            g.joints += 1;
            self.joints.insert(id, group);
            joints.push((index, id));
        }
        for &e in &shell.fixed {
            fix(&mut elements[e]);
        }
        let g = self.group_mut(group.0);
        g.own.extend(elements.iter().map(|e| e.body));
        g.bodies += elements.len();
        g.requests = shell.exact_solver;
        g.marks = shell.exact_solver;
        for e in &elements {
            self.bodies.insert(super::BodyId(e.body), group);
        }
        let geoms: Vec<(usize, Shape, u32)> = (shell.elements.iter().enumerate())
            .flat_map(|(i, e)| e.geometries.iter().map(move |g| (i, g.shape, e.material)))
            .collect();
        let mut s = OdeShell {
            serial: self.serial,
            group,
            elements,
            joints,
            has_geometry: shell.has_geometry(),
            height: 0.0,
            resting: shell.switched_off,
            gravity: None,
            level: vec![LevelState::default(); geoms.len()],
            geoms,
            class: Class::shell(shell),
            bounds: None,
        };
        s.read(self);
        s
    }

    /// Sets a hinge's motor on its axis (script API Y 5.3): its maximum force
    /// and target velocity.
    pub fn set_hinge_motor(&mut self, joint: JointId, force: f32, velocity: f32) {
        if self.joints.contains_key(&joint) {
            // SAFETY: the joint is this solver's.
            unsafe {
                dJointSetHingeParam(joint.0, param::F_MAX, force);
                dJointSetHingeParam(joint.0, param::VEL, velocity);
            }
        }
    }

    /// Sets a hinge's parameter (`script` 5.2 – 5.6), if the joint is this
    /// solver's.
    pub fn set_hinge_param(&mut self, joint: JointId, parameter: i32, value: f32) {
        if self.joints.contains_key(&joint) {
            // SAFETY: the joint is this solver's.
            unsafe { dJointSetHingeParam(joint.0, parameter, value) };
        }
    }

    /// A hinge's parameter, if the joint is this solver's.
    pub fn hinge_param(&self, joint: JointId, parameter: i32) -> Option<f32> {
        self.joints.contains_key(&joint).then(|| {
            // SAFETY: the joint is this solver's.
            unsafe { dJointGetHingeParam(joint.0, parameter) }
        })
    }

    /// A hinge's anchor and axis in the world, as the library's getters
    /// give them (on its body 1).
    pub fn hinge_anchor_axis(&self, joint: JointId) -> Option<(Vec3, Vec3)> {
        self.joints.contains_key(&joint).then(|| {
            let (mut p, mut a) = ([0.0; 4], [0.0; 4]);
            // SAFETY: the joint is this solver's; both are dVector3s.
            unsafe {
                dJointGetHingeAnchor(joint.0, p.as_mut_ptr());
                dJointGetHingeAxis(joint.0, a.as_mut_ptr());
            }
            ([p[0], p[1], p[2]], [a[0], a[1], a[2]])
        })
    }

    /// A hinge's angle (the library's getter, `joints` 2.2).
    pub fn hinge_angle(&self, joint: JointId) -> Option<f32> {
        self.joints.contains_key(&joint).then(|| {
            // SAFETY: the joint is this solver's.
            unsafe { dJointGetHingeAngle(joint.0) }
        })
    }

    /// Whether the exact solver solves a group marked for it this step.
    pub fn marked_exact(&self, group: GroupId) -> bool {
        self.group(group).is_some_and(|g| g.marks)
    }
}

/// The mass of an element's pieces (2.3): each geometry's volume at its
/// density, turned to the geometry's axes and moved to its offset from the
/// mass centre, added up.
fn mass(pieces: &[MassPiece]) -> DMass {
    let mut total = DMass::default();
    // SAFETY: the dMass values are valid and local; rotations are dMatrix3s.
    unsafe {
        dMassSetZero(&mut total);
        for p in pieces {
            let mut m = DMass::default();
            let axes = match p.shape {
                Shape::Box {
                    axes, half_size, ..
                } => {
                    let [x, y, z] = half_size.map(|h| 2.0 * h);
                    dMassSetBox(&mut m, p.density, x, y, z);
                    Some(axes)
                }
                Shape::Sphere { radius, .. } => {
                    dMassSetSphere(&mut m, p.density, radius);
                    None
                }
                Shape::Cylinder {
                    direction,
                    height,
                    radius,
                    ..
                } => {
                    // A flat-ended cylinder along z, turned to the
                    // direction (the volume of 2.3; believed).
                    dMassSetCylinder(&mut m, p.density, 3, radius, height);
                    Some(basis(direction))
                }
            };
            if let Some(axes) = axes {
                let mut r = [0.0; 12];
                for i in 0..3 {
                    for j in 0..3 {
                        r[i * 4 + j] = axes[j][i];
                    }
                }
                dMassRotate(&mut m, r.as_ptr());
            }
            dMassTranslate(&mut m, p.offset[0], p.offset[1], p.offset[2]);
            dMassAdd(&mut total, &m);
        }
    }
    total
}

/// Axes whose third is the unit `z`.
fn basis(z: Vec3) -> [Vec3; 3] {
    use crate::collision::geometry::{cross, normalize};
    let other = if z[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let x = normalize(cross(other, z)).unwrap_or([1.0, 0.0, 0.0]);
    [x, cross(z, x), z]
}

/// Fixes an element (5.1): a sphere mass adjusted to
/// [`FIXED_MASS`], gravity off, zero velocities, force and torque.
fn fix(e: &mut ElementBody) {
    e.fixed = true;
    let mut m = DMass::default();
    // SAFETY: the body is live; `m` is a valid dMass.
    unsafe {
        dMassSetSphere(&mut m, FIXED_SPHERE.0, FIXED_SPHERE.1);
        dMassAdjust(&mut m, FIXED_MASS);
        dBodySetMass(e.body, &m);
        dBodySetGravityMode(e.body, 0);
        dBodySetLinearVel(e.body, 0.0, 0.0, 0.0);
        dBodySetAngularVel(e.body, 0.0, 0.0, 0.0);
        dBodySetForce(e.body, 0.0, 0.0, 0.0);
        dBodySetTorque(e.body, 0.0, 0.0, 0.0);
    }
}

fn vec(p: *const Real) -> Vec3 {
    // SAFETY: ODE returns a live dVector3.
    let v = unsafe { std::slice::from_raw_parts(p, 3) };
    [v[0], v[1], v[2]]
}

impl OdeShell {
    pub fn group(&self) -> GroupId {
        self.group
    }

    pub fn elements(&self) -> usize {
        self.elements.len()
    }

    /// Its joints as made, by the shell's joint index.
    pub fn joints(&self) -> impl Iterator<Item = (usize, JointId)> + '_ {
        self.joints.iter().copied()
    }

    pub fn fixed(&self, element: usize) -> bool {
        self.elements[element].fixed
    }

    /// Reads what the world needs of its bodies.
    fn read(&mut self, solver: &OdeSolver) {
        if !solver.live(self.serial, self.group) {
            return;
        }
        // SAFETY: the bodies live while their group does.
        unsafe {
            self.height = self
                .elements
                .first()
                .map_or(0.0, |e| vec(dBodyGetPosition(e.body))[1]);
            self.resting = self.elements.iter().all(|e| dBodyIsEnabled(e.body) == 0);
        }
        let mut geoms = Vec::new();
        self.place(solver, &mut geoms);
        self.bounds = bounds_of(&geoms);
    }

    /// Its geometries where its bodies hold them.
    fn place(&self, solver: &OdeSolver, out: &mut Vec<Geom>) {
        for &(e, shape, material) in &self.geoms {
            let Some(frame) = self.frame(solver, e) else {
                continue;
            };
            out.push(Geom {
                shape: shape.moved(&frame),
                body: Some(super::BodyId(self.elements[e].body)),
                material,
                meets_level: true,
            });
        }
    }

    /// Its collision class (physics-contacts.md 1.3).
    pub fn class(&self) -> Class {
        self.class
    }

    /// An element's body: its position (the mass centre), rotation (rows
    /// of ODE's `R`), mass, and linear and angular velocity; `None`
    /// without its group in `solver`.
    pub fn body(&self, solver: &OdeSolver, element: usize) -> Option<BodyState> {
        if !solver.live(self.serial, self.group) {
            return None;
        }
        let e = self.elements.get(element)?;
        let mut m = DMass::default();
        // SAFETY: the body lives while its group does.
        unsafe {
            let r = std::slice::from_raw_parts(dBodyGetRotation(e.body), 12);
            dBodyGetMass(e.body, &mut m);
            Some(BodyState {
                position: vec(dBodyGetPosition(e.body)),
                rotation: std::array::from_fn(|i| [r[i * 4], r[i * 4 + 1], r[i * 4 + 2]]),
                mass: m.mass,
                inertia: std::array::from_fn(|i| [m.i[i * 4], m.i[i * 4 + 1], m.i[i * 4 + 2]]),
                linear: vec(dBodyGetLinearVel(e.body)),
                angular: vec(dBodyGetAngularVel(e.body)),
                enabled: dBodyIsEnabled(e.body) != 0,
                gravity: super::body_gravity(e.body),
            })
        }
    }

    /// An element's world frame (2.1): its body's pose moved back by the
    /// rotated mass centre.
    pub fn frame(&self, solver: &OdeSolver, element: usize) -> Option<Mat4> {
        let b = self.body(solver, element)?;
        let c = self.elements[element].centre;
        let mut m = IDENTITY;
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] = b.rotation[j][i];
            }
        }
        let off = turn(&m, c);
        for k in 0..3 {
            m[3][k] = b.position[k] - off[k];
        }
        Some(m)
    }

    /// Wakes it (`resting` 2): every body switched on. The world makes it
    /// active.
    pub fn wake(&mut self, solver: &OdeSolver) {
        if !solver.live(self.serial, self.group) {
            return;
        }
        for e in &self.elements {
            // SAFETY: the body lives while its group does.
            unsafe { dBodyEnable(e.body) };
        }
        self.resting = false;
    }

    /// `script` 4.1: a force on an element's body, only when it is not
    /// fixed: a switched-off body is switched on, and the shell no longer
    /// rests (believed), then the force is added to the body. How the
    /// body's accumulated force and torque are then limited is open
    /// (PH-9; TODO(#990)).
    pub fn apply_element_force(&mut self, solver: &OdeSolver, element: usize, force: Vec3) {
        if solver.live(self.serial, self.group) {
            self.element_force(element, force);
        }
    }

    /// [`OdeShell::apply_element_force`] on a live shell.
    fn element_force(&mut self, element: usize, force: Vec3) {
        let Some(e) = self.elements.get(element).filter(|e| !e.fixed) else {
            return;
        };
        // SAFETY: the caller checked the body lives.
        unsafe {
            if dBodyIsEnabled(e.body) == 0 {
                dBodyEnable(e.body);
                self.resting = false;
            }
            dBodyAddForce(e.body, force[0], force[1], force[2]);
        }
    }

    /// `script` 3.1: a force shared over the elements in proportion to
    /// their masses (as set, 2.3), each along its direction, then the shell
    /// woken; nothing below a size of 10⁻⁷.
    pub fn apply_force(&mut self, solver: &OdeSolver, force: Vec3) {
        if solver.live(self.serial, self.group) {
            self.shell_force(force);
        }
    }

    /// [`OdeShell::apply_force`] on a live shell.
    fn shell_force(&mut self, force: Vec3) {
        let f = (force[0] * force[0] + force[1] * force[1] + force[2] * force[2]).sqrt();
        if f < 1e-7 {
            return;
        }
        let total: f32 = self.elements.iter().map(|e| e.mass).sum();
        let direction = force.map(|c| c / f);
        for i in 0..self.elements.len() {
            let size = f * self.elements[i].mass / total;
            self.element_force(i, direction.map(|d| d * size));
        }
        for e in &self.elements {
            // SAFETY: the caller checked the bodies live.
            unsafe { dBodyEnable(e.body) };
        }
        self.resting = false;
    }

    /// `script` 4.3: fixes an element as 5.1 does.
    pub fn fix_element(&mut self, solver: &OdeSolver, element: usize) {
        if !solver.live(self.serial, self.group) {
            return;
        }
        if let Some(e) = self.elements.get_mut(element) {
            fix(e);
        }
    }

    /// `script` 4.3: a fixed element no longer fixed, its body given back
    /// its own mass; its gravity stays off.
    pub fn release_fixed(&mut self, solver: &OdeSolver, element: usize) {
        if !solver.live(self.serial, self.group) {
            return;
        }
        let Some(e) = self.elements.get_mut(element).filter(|e| e.fixed) else {
            return;
        };
        e.fixed = false;
        if let Some(m) = &e.own {
            // SAFETY: the body lives while its group does; `m` is a dMass.
            unsafe { dBodySetMass(e.body, m) };
        }
    }

    /// 2.4: a switched-on, unfixed body's velocities divided by their
    /// divisors, then a torque −(angular drag) × ω and a force −k × v with
    /// k = (linear drag) × speed, at most mass / h. That the division comes
    /// before the drags is our reading; the speed limits are not enforced,
    /// how being open (TODO(#947)).
    fn damp(&self, h: f32) {
        for e in self.elements.iter().filter(|e| !e.fixed) {
            let m = e.motion;
            // SAFETY: the body lives while its group does (checked by the
            // caller).
            unsafe {
                if dBodyIsEnabled(e.body) == 0 {
                    continue;
                }
                let v = vec(dBodyGetLinearVel(e.body)).map(|v| v / m.linear_divisor);
                let w = vec(dBodyGetAngularVel(e.body)).map(|w| w / m.angular_divisor);
                dBodySetLinearVel(e.body, v[0], v[1], v[2]);
                dBodySetAngularVel(e.body, w[0], w[1], w[2]);
                let t = w.map(|w| -m.angular_drag * w);
                dBodyAddTorque(e.body, t[0], t[1], t[2]);
                let mut mass = DMass::default();
                dBodyGetMass(e.body, &mut mass);
                let speed = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                let k = (m.linear_drag * speed).min(mass.mass / h);
                dBodyAddForce(e.body, -k * v[0], -k * v[1], -k * v[2]);
            }
        }
    }
}

/// An element's body as the library holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyState {
    pub position: Vec3,
    /// ODE's `R` (`world = R · local`), by rows.
    pub rotation: [Vec3; 3],
    pub mass: f32,
    pub inertia: [Vec3; 3],
    pub linear: Vec3,
    pub angular: Vec3,
    pub enabled: bool,
    pub gravity: bool,
}

impl Grouped for OdeShell {
    fn group(&self) -> GroupId {
        self.group
    }

    fn before_solve(&mut self, solver: &mut OdeSolver, params: &Params) {
        if !solver.live(self.serial, self.group) {
            return;
        }
        if let Some(on) = self.gravity.take() {
            for e in self.elements.iter().filter(|e| !e.fixed) {
                // SAFETY: the body lives while its group does.
                unsafe { dBodySetGravityMode(e.body, c_int::from(on)) };
            }
        }
        self.damp(params.step);
    }

    fn after_solve(&mut self, solver: &OdeSolver) {
        self.read(solver);
    }

    fn class(&self) -> Class {
        self.class
    }

    fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    fn geometries(&self, solver: &OdeSolver, out: &mut Vec<Geom>) {
        self.place(solver, out);
    }

    fn level_state(&mut self, geom: usize) -> Option<&mut LevelState> {
        self.level.get_mut(geom)
    }

    fn wake(&mut self, solver: &OdeSolver) {
        OdeShell::wake(self, solver);
    }
}

impl Object for OdeShell {
    fn kind(&self) -> Kind {
        Kind::Shell
    }

    fn has_geometry(&self) -> bool {
        self.has_geometry
    }

    fn height(&self) -> f32 {
        self.height
    }

    /// Takes effect at its next solving (a resting shell is not stepped
    /// before); a fixed element keeps its gravity off (5.1).
    fn set_gravity(&mut self, on: bool) {
        self.gravity = Some(on);
    }

    /// A constant force's step (`world` 4.3): the shell's own force
    /// application, [`OdeShell::apply_force`] (`script` 2.2). The world
    /// calls it on active shells, whose bodies live.
    fn add_force(&mut self, force: Vec3) {
        self.shell_force(force);
    }

    /// Its motion parameters act when the groups are solved
    /// ([`Grouped::before_solve`]: "before solving", 2.4).
    fn pre_step(&mut self, _: &Params) {}

    /// Once woken a shell does not rest again before PH-5 (TODO(#946)):
    /// the steppers switch every stepped body on (`library` 4.1).
    fn post_step(&mut self, _: &Params) -> bool {
        self.resting
    }

    fn clear_level_contacts(&mut self) {
        self.level.iter_mut().for_each(LevelState::clear);
    }
}

#[cfg(test)]
mod tests;
