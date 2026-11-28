// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's collision body and its movement against the level
//! (docs/formats/level-cform.md sections 4–6).
//!
//! The body never rotates (section 5.1), so it is simulated as a point mass
//! carrying its shapes. Each fixed step of `h` seconds (section 6.1) has two
//! halves, so a movement layer (`player-movement`) can compute its forces
//! from the step's collision results:
//!
//! 1. [`PlayerBody::begin_step`] takes the requested acceleration, runs the
//!    step probe (6.3), generates the contacts and applies the player's
//!    contact rules (4, 5.4, 6.3), and reports ground, obstacle, traction
//!    and step assistance ([`StepContacts`]).
//! 2. [`PlayerBody::finish_step`] takes the total external force (gravity
//!    included: its value belongs to `player-movement`), solves the
//!    contacts and moves the body, and reports the airborne state and
//!    landing (6.4).
//!
//! The spec gives each contact's parameters (error reduction, constraint-
//! force mixing, friction pyramid, bounce, section 4.2) and the solver's
//! iteration count (6.1), not the solver. Ours is our interpretation of
//! those terms: soft velocity constraints solved by projected Gauss–Seidel
//! over `ph_iterations` sweeps, friction bounded by the coefficient times
//! the normal impulse (Coulomb's law).

use std::io;

use volya_engine_formats::gamemtl::{ACTOR_OBSTACLE, BOUNCY, PASS_THROUGH};

use super::CollisionWorld;
use super::geometry::{self, RawContact, Vec3, add, cross, dot, length, normalize, scale, sub};
use crate::config::{Section, value};

/// The physics step `h` of `ph_frequency = 100` (section 6.1; the shipped
/// `user.ltx` and the game's own default agree). User settings are not
/// read for it (our choice).
pub const STEP: f32 = 0.01;
/// `ph_iterations` of the shipped `user.ltx` (section 6.1).
pub const ITERATIONS: u32 = 38;
/// The world's spring and damping `S`, `D` (section 4.2).
pub const WORLD_SPRING: f32 = 24_000_002.0;
pub const WORLD_DAMPING: f32 = 400_000.0;
/// Movement is requested above this `|a|`, m/s² (section 6.2).
pub const MOVE_THRESHOLD: f32 = 0.1;
/// Ground steeper than 45° is not walkable (section 6.2).
pub const WALKABLE_NORMAL_Y: f32 = 0.707_106_77;
/// Ground ending the airborne state (section 6.4).
pub const LANDING_NORMAL_Y: f32 = 0.353_553_38;
/// Squared speed counted as at rest, ending the airborne state (6.4).
pub const REST_SPEED_SQUARED: f32 = 1e-7;
/// Departure distances making the body airborne (6.4).
pub const AIRBORNE_DISTANCE: f32 = 0.5;
pub const AIRBORNE_HEIGHT: f32 = 0.1;
/// Side force coefficients of `player-movement`, normal and during step
/// assistance (section 6.2).
pub const SIDE_FORCE_COEFFICIENT: f32 = 500.0;
pub const STEP_ASSIST_SIDE_FORCE_COEFFICIENT: f32 = 700.0;
/// The drive along an obstacle during step assistance is this × mass ×
/// traction (6.2), and every assisted drive and the resting friction are
/// multiplied by [`STEP_ASSIST_FACTOR`] (5.4, 6.2).
pub const STEP_ASSIST_OBSTACLE_DRIVE: f32 = 25.0;
pub const STEP_ASSIST_FACTOR: f32 = 4.0;
/// The step probe (6.3): distance ahead of the feet, the upper box's
/// centre height above the feet, and the lower box's lift above `r`.
pub const PROBE_AHEAD: f32 = 0.4;
pub const PROBE_UPPER_HEIGHT: f32 = 1.5;
pub const PROBE_LOWER_LIFT: f32 = 0.05;
/// A refused posture change is not retried for this long (ms) unless the
/// body has moved this far (m) (section 5.3).
pub const POSTURE_RETRY_MS: u64 = 500;
pub const POSTURE_RETRY_DISTANCE: f32 = 0.05;
/// The posture change test's growth steps, settling steps per growth step
/// and the penetration left that refuses the change: unestablished in the
/// spec (5.3), our choice.
pub const POSTURE_GROWTH_STEPS: u32 = 5;
pub const POSTURE_SETTLE_STEPS: u32 = 4;
pub const POSTURE_PENETRATION: f32 = 0.01;
/// The material the player's shapes get when the body is made: index 0,
/// `default` (section 5.1); the actor's spawn gives them its section's
/// (docs/behavior/physics-contacts.md 5, [`PlayerBody::set_material`]).
pub const PLAYER_MATERIAL: u32 = 0;
/// A contact with ladder `i`'s triangle `k` records triangle
/// `LADDER_TRIANGLES + 12·i + k` (our numbering, apart from the level's).
pub const LADDER_TRIANGLES: u32 = 0x8000_0000;
/// The outer sphere's radius and its centre's height above the body
/// centre, in `r` (section 5.1).
pub const OUTER_RADIUS: f32 = 2.0;
pub const OUTER_HEIGHT: f32 = 2.5;
/// The constant of the normal-direction softening (5.4): 0.5 for the
/// level, 5 for a movable body (docs/behavior/trader-door.md 3.3).
pub const LEVEL_SOFTENING: f32 = 0.5;
pub const BODY_SOFTENING: f32 = 5.0;
/// The world's error-reduction parameter `E` and constraint-force mixing
/// `C` at the step `h` (section 4.2): `K·h / (K·h + D₀)` and
/// `1 / (K·h + D₀)`.
pub fn world_erp_cfm() -> (f32, f32) {
    let k = WORLD_SPRING * STEP;
    (k / (k + WORLD_DAMPING), 1.0 / (k + WORLD_DAMPING))
}

const UP: Vec3 = [0.0, 1.0, 0.0];

/// The player's shapes from a box's half-extents `ph_boxN_size`
/// (section 5.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shapes {
    pub r: f32,
    pub h: f32,
    pub s: f32,
}

impl Shapes {
    pub fn of(size: Vec3) -> Self {
        let e = scale(size, 2.0);
        let r = 0.5 * e[0].min(e[2]);
        let h = e[1] - 2.0 * r;
        let h = if h < 0.0 { 0.01 } else { h };
        Self {
            r,
            h,
            s: 0.5 * r * 0.305_555_6f32.sqrt(),
        }
    }

    /// The torso cylinder's and head sphere's radius.
    pub fn small_radius(&self) -> f32 {
        0.833_333_3 * self.r
    }

    /// The torso cylinder's centre height above the body centre and its
    /// half length.
    pub fn torso(&self) -> (f32, f32) {
        (0.5 * self.h - self.s, 0.5 * (self.h + self.s))
    }

    /// Height of the top of the head sphere above the body centre.
    pub fn top(&self) -> f32 {
        self.h + self.small_radius()
    }
}

/// A box of a movable body (docs/behavior/trader-door.md 3.1) in world
/// space, as its shell holds it: what the crosshair picks and the route
/// planner walls off (the player's body meets the shell itself, through the
/// physics world's contacts).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyBox {
    pub centre: Vec3,
    /// Unit axes.
    pub axes: [Vec3; 3],
    pub half_size: Vec3,
    /// Its game material's index (section 3.3).
    pub material: u32,
}

impl BodyBox {
    /// The corner on side `s` (each ±1) of each axis.
    pub fn corner(&self, s: [f32; 3]) -> Vec3 {
        (0..3).fold(self.centre, |p, k| {
            add(p, scale(self.axes[k], s[k] * self.half_size[k]))
        })
    }

    /// Its six faces as twelve triangles, each facing out (section 2.3's
    /// winding).
    pub fn triangles(&self) -> [[Vec3; 3]; 12] {
        let mut out = [[[0.0; 3]; 3]; 12];
        for k in 0..3 {
            let (i, j) = ((k + 1) % 3, (k + 2) % 3);
            for (f, side) in [-1.0f32, 1.0].into_iter().enumerate() {
                let at = |a: f32, b: f32| {
                    let mut s = [0.0; 3];
                    (s[k], s[i], s[j]) = (side, a, b);
                    self.corner(s)
                };
                let mut q = [at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)];
                let outward = scale(self.axes[k], side);
                if geometry::front_normal(&[q[0], q[1], q[2]])
                    .is_some_and(|n| dot(n, outward) < 0.0)
                {
                    q.reverse();
                }
                let n = 4 * k + 2 * f;
                out[n] = [q[0], q[1], q[2]];
                out[n + 1] = [q[0], q[2], q[3]];
            }
        }
        out
    }

    /// Its world-space bounding box.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let reach: Vec3 = std::array::from_fn(|c| {
            (0..3)
                .map(|k| (self.axes[k][c] * self.half_size[k]).abs())
                .sum()
        });
        (sub(self.centre, reach), add(self.centre, reach))
    }

    /// The distance from `p` to the box, 0 inside it.
    pub fn distance(&self, p: Vec3) -> f32 {
        let d = sub(p, self.centre);
        (0..3)
            .map(|k| (dot(d, self.axes[k]).abs() - self.half_size[k]).max(0.0))
            .map(|e| e * e)
            .sum::<f32>()
            .sqrt()
    }

    /// The distance along the unit direction `dir` from `origin` at which
    /// the ray enters the box, if it meets it (0 from inside).
    pub fn ray(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        let d = sub(origin, self.centre);
        let (mut near, mut far) = (0.0f32, f32::INFINITY);
        for k in 0..3 {
            let (o, v, h) = (
                dot(d, self.axes[k]),
                dot(dir, self.axes[k]),
                self.half_size[k],
            );
            if v.abs() < 1e-12 {
                if o.abs() > h {
                    return None;
                }
                continue;
            }
            let (a, b) = ((-h - o) / v, (h - o) / v);
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
        Some(near)
    }
}

/// The shapes colliding with the level (the outer sphere does not).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Bottom,
    Torso,
    Head,
}

/// The actor's collision configuration (section 5.2): `ph_box0_size` …
/// `ph_box2_size` and `ph_mass`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyConfig {
    /// Box 0 standing, 1 crouching, 2 low crouch.
    pub boxes: [Vec3; 3],
    pub mass: f32,
}

impl BodyConfig {
    /// From the actor's section (`actor`); a missing key is an error (the
    /// spec does not say what the game does then).
    pub fn from_section(section: Section<'_>) -> io::Result<Self> {
        let get = |key: &str| {
            section.get(key).flatten().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("[{}] has no {key}", section.name()),
                )
            })
        };
        Ok(Self {
            boxes: [
                value::floats(get("ph_box0_size")?),
                value::floats(get("ph_box1_size")?),
                value::floats(get("ph_box2_size")?),
            ],
            mass: value::float(get("ph_mass")?),
        })
    }
}

/// What the movement layer hands the body for a step (section 6.2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepInput {
    /// The requested acceleration `a`, world space, m/s².
    pub acceleration: Vec3,
    /// A jump has been requested and not yet started (6.3).
    pub jump_requested: bool,
}

/// One contact of a step after the player's rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub shape: Shape,
    pub triangle: u32,
    /// The triangle's material index (section 3.3).
    pub material: u32,
    /// From the surface towards the body.
    pub normal: Vec3,
    /// The normal as generated, before the step replacement.
    pub generated_normal: Vec3,
    pub depth: f32,
    pub point: Vec3,
    /// Replaced by the step rule (6.3).
    pub replaced: bool,
    /// Error-reduction parameter and constraint-force mixing (4.2, 5.4).
    pub erp: f32,
    pub cfm: f32,
    pub friction: f32,
    /// Bounce coefficient and the approach speed above which it applies.
    pub bounce: Option<(f32, f32)>,
}

/// The collision results of a step for the movement layer (6.2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepContacts {
    /// `|a| > 0.1`.
    pub moving: bool,
    /// `â`, zero when not moving.
    pub direction: Vec3,
    /// The ground normal: the contact normal with the largest y.
    pub ground: Option<Vec3>,
    /// Whether the ground normal's y exceeds [`WALKABLE_NORMAL_Y`].
    pub walkable: bool,
    /// The normal most opposed to the requested horizontal movement.
    pub obstacle: Option<Vec3>,
    /// The step's traction `T` (5.4): `player-movement`'s friction factor.
    pub traction: f32,
    pub step_assist: bool,
    /// The step's "blocked" flag (6.3).
    pub blocked: bool,
    /// The step's impact speed (4.4).
    pub impact_speed: f32,
}

/// What a step did (6.4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepResult {
    /// The actor's position (feet) after the step.
    pub position: Vec3,
    pub velocity: Vec3,
    pub airborne: bool,
    /// The airborne state ended this step.
    pub landed: bool,
}

/// The player's collision body.
#[derive(Clone, Debug)]
pub struct PlayerBody {
    pub config: BodyConfig,
    box_index: usize,
    shapes: Shapes,
    centre: Vec3,
    /// The centre at the end of the previous step (front-side test, 4.1).
    previous: Vec3,
    velocity: Vec3,
    /// Step assistance of the last probe (6.3). Kept while no movement is
    /// requested and the probe does not run (our reading), so resting
    /// friction is ×4 after stopping at a step (5.4).
    step_assist: bool,
    blocked_previous: bool,
    had_contact: bool,
    departure: Option<Vec3>,
    airborne: bool,
    jump_in_progress: bool,
    /// The material most recently touched (4.4).
    ground_material: Option<u32>,
    /// The last refused posture change: target box, time, centre.
    refused: Option<(usize, u64, Vec3)>,
    /// The player is climbing (docs/behavior/climbing.md 3.1, 3.4): not
    /// airborne, and its contacts while moving take the world's softness.
    climbing: bool,
    /// Its shapes' material index (physics-contacts.md 5).
    material: u32,
    contacts: Vec<Contact>,
    step: StepContacts,
    scratch: Vec<u32>,
    rows: Vec<Row>,
}

/// The contact rules' state while a step's contacts are processed.
struct Rules {
    moving: bool,
    direction: Vec3,
    step_assist: bool,
    blocked: bool,
    /// The replacement's conditions other than "no head contact so far".
    replacement: bool,
}

impl PlayerBody {
    /// A body standing (box 0) with its feet at `position`, at rest.
    pub fn new(config: BodyConfig, position: Vec3) -> Self {
        let shapes = Shapes::of(config.boxes[0]);
        let centre = add(position, [0.0, shapes.r, 0.0]);
        Self {
            config,
            box_index: 0,
            shapes,
            centre,
            previous: centre,
            velocity: [0.0; 3],
            step_assist: false,
            blocked_previous: false,
            had_contact: false,
            departure: None,
            airborne: false,
            jump_in_progress: false,
            ground_material: None,
            refused: None,
            climbing: false,
            material: PLAYER_MATERIAL,
            contacts: Vec::new(),
            step: StepContacts::default(),
            scratch: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// The actor's position: the body centre − `(0, r, 0)` (5.1).
    pub fn position(&self) -> Vec3 {
        sub(self.centre, [0.0, self.shapes.r, 0.0])
    }

    /// Places the feet at `position`, as a teleport (no crossing).
    pub fn set_position(&mut self, position: Vec3) {
        self.centre = add(position, [0.0, self.shapes.r, 0.0]);
        self.previous = self.centre;
    }

    /// Moves the body by `delta` within the step just finished (the speed
    /// cap's position corrections, `player-movement` 6.3): unlike
    /// [`PlayerBody::set_position`] the previous centre is kept, so the
    /// front-side test still sees the step's crossing.
    pub fn move_by(&mut self, delta: Vec3) {
        self.centre = add(self.centre, delta);
    }

    pub fn centre(&self) -> Vec3 {
        self.centre
    }

    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }

    /// For the movement layer's speed limits and jumps.
    pub fn set_velocity(&mut self, velocity: Vec3) {
        self.velocity = velocity;
    }

    pub fn box_index(&self) -> usize {
        self.box_index
    }

    pub fn shapes(&self) -> Shapes {
        self.shapes
    }

    pub fn airborne(&self) -> bool {
        self.airborne
    }

    /// Sets whether the player is climbing (climbing.md 3.1, 3.4).
    pub fn set_climbing(&mut self, climbing: bool) {
        self.climbing = climbing;
    }

    /// The material most recently touched, a pass-through one taking
    /// precedence over a solid one within a step (4.4); its deadly flag and
    /// damage rate are for the health system.
    pub fn ground_material(&self) -> Option<u32> {
        self.ground_material
    }

    /// Its shapes' material index: the actor's spawn gives them its
    /// section's `material` (physics-contacts.md 5).
    pub fn set_material(&mut self, material: u32) {
        self.material = material;
    }

    pub fn material(&self) -> u32 {
        self.material
    }

    /// What the physics world's step takes of the movement step just run
    /// (docs/behavior/physics-contacts.md 5): the body's centre at the
    /// step's start, its velocity after it, its shapes and the step's
    /// rules.
    pub fn world_step(&self) -> WorldStep {
        WorldStep {
            centre: self.previous,
            velocity: self.velocity,
            shapes: self.shapes,
            mass: self.config.mass,
            material: self.material,
            moving: self.step.moving,
            step_assist: self.step.step_assist,
            traction: self.step.traction,
            climbing: self.climbing,
        }
    }

    /// The world step's reaction on the body (physics-contacts.md 5, 7):
    /// the change `dv` its contacts with movable bodies made to the
    /// velocity, which moves the body for the step too, and whether they set
    /// the "blocked" flag (trader-door.md 3.1, 3.3), which the next step's
    /// replacement reads (6.3).
    pub fn react(&mut self, dv: Vec3, blocked: bool) {
        self.velocity = add(self.velocity, dv);
        self.centre = add(self.centre, scale(dv, STEP));
        self.blocked_previous |= blocked;
    }

    /// The contacts of the current step, after the player's rules.
    pub fn contacts(&self) -> &[Contact] {
        &self.contacts
    }

    /// The first half of a step: probe, contacts and rules.
    pub fn begin_step(&mut self, world: &CollisionWorld, input: &StepInput) -> StepContacts {
        let a = input.acceleration;
        let moving = length(a) > MOVE_THRESHOLD;
        let direction = if moving {
            scale(a, 1.0 / length(a))
        } else {
            [0.0; 3]
        };
        let mut blocked = false;
        if moving {
            let (lower, upper) = self.probe(world, direction);
            self.step_assist = lower && !upper;
            blocked = upper;
        }
        let rules = Rules {
            moving,
            direction,
            step_assist: self.step_assist,
            blocked,
            replacement: moving
                && self.step_assist
                && !self.blocked_previous
                && !input.jump_requested
                && !self.jump_in_progress,
        };
        let mut contacts = std::mem::take(&mut self.contacts);
        let mut scratch = std::mem::take(&mut self.scratch);
        let out = self.collide(world, &self.shapes, rules, &mut contacts, &mut scratch);
        self.contacts = contacts;
        self.scratch = scratch;
        if let Some(m) = out.material {
            self.ground_material = Some(m);
        }
        // 6.2: ground and obstacle among the contacts with force, by their
        // normals as generated, before the step replacement (our reading;
        // replaced normals all point up).
        let ground = self
            .contacts
            .iter()
            .map(|c| c.generated_normal)
            .max_by(|a, b| a[1].total_cmp(&b[1]));
        let obstacle = moving
            .then(|| {
                self.contacts
                    .iter()
                    .map(|c| c.generated_normal)
                    .min_by(|p, q| horizontal_dot(*p, a).total_cmp(&horizontal_dot(*q, a)))
            })
            .flatten();
        self.step = StepContacts {
            moving,
            direction,
            ground,
            walkable: ground.is_some_and(|n| n[1] > WALKABLE_NORMAL_Y),
            obstacle,
            traction: out.traction,
            step_assist: self.step_assist,
            blocked: out.blocked,
            impact_speed: out.impact,
        };
        self.step
    }

    /// The second half of a step: `force` (newtons, gravity included) acts
    /// for `h`, the contacts are solved and the body moves. `jump_applied`
    /// says the movement layer applied a jump this step (6.3, 6.4).
    pub fn finish_step(&mut self, force: Vec3, jump_applied: bool) -> StepResult {
        let mut v = add(self.velocity, scale(force, STEP / self.config.mass));
        solve(
            &self.contacts,
            &mut v,
            self.config.mass,
            ITERATIONS,
            &mut self.rows,
        );
        let start = self.centre;
        self.previous = self.centre;
        self.centre = add(self.centre, scale(v, STEP));
        self.velocity = v;
        self.blocked_previous = self.step.blocked;
        // 6.4: contacts here are those with collision force (our reading of
        // "no contact at all": pass-through contacts do not count).
        let had_contact = !self.contacts.is_empty();
        if !had_contact && self.had_contact {
            self.departure = Some(start);
        }
        self.had_contact = had_contact;
        let mut landed = false;
        if jump_applied {
            self.airborne = true;
            self.jump_in_progress = true;
        } else if self.climbing {
            // climbing.md 3.1: the airborne state ends on the first
            // climbing step and is not entered while climbing; a climbing
            // contact ends the jump too (player-movement.md 6.6, our
            // reading of its jump assistance). The departure point of 6.4
            // follows the body, so leaving the ladder counts from where it
            // is let go (our choice).
            landed = self.airborne;
            self.airborne = false;
            self.jump_in_progress = false;
            self.departure = Some(self.centre);
        } else {
            if self.step.walkable {
                self.jump_in_progress = false;
            }
            if self.airborne {
                let ground = self.step.ground.is_some_and(|n| n[1] > LANDING_NORMAL_Y);
                if ground || dot(v, v) < REST_SPEED_SQUARED {
                    self.airborne = false;
                    landed = true;
                }
            } else if self.step.ground.is_none()
                && let Some(d) = self.departure
            {
                let off = sub(self.centre, d);
                if length(off) > AIRBORNE_DISTANCE && off[1].abs() > AIRBORNE_HEIGHT {
                    self.airborne = true;
                }
            }
        }
        StepResult {
            position: self.position(),
            velocity: v,
            airborne: self.airborne,
            landed,
        }
    }

    /// A whole step: `forces` computes the force from the step's contacts.
    pub fn step(
        &mut self,
        world: &CollisionWorld,
        input: &StepInput,
        forces: impl FnOnce(&StepContacts) -> (Vec3, bool),
    ) -> StepResult {
        let contacts = self.begin_step(world, input);
        let (force, jump) = forces(&contacts);
        self.finish_step(force, jump)
    }

    /// Changes to box `target` (5.3) at engine clock `now` (ms). A lower
    /// box always succeeds (believed in the spec); a taller one only if
    /// the taller body fits: the shapes grow in steps while the body
    /// settles, and the change is refused if penetration remains. A refused
    /// target is not retried within 500 ms unless the body moved 0.05 m.
    /// Returns whether the body now has `target`; the caller keeps asking
    /// while the posture is requested.
    pub fn request_box(&mut self, world: &CollisionWorld, target: usize, now: u64) -> bool {
        if target == self.box_index {
            return true;
        }
        let from = self.config.boxes[self.box_index];
        let to = self.config.boxes[target];
        if to[1] <= from[1] {
            self.set_box(target);
            return true;
        }
        if let Some((t, at, centre)) = self.refused
            && t == target
            && now.saturating_sub(at) < POSTURE_RETRY_MS
            && length(sub(self.centre, centre)) < POSTURE_RETRY_DISTANCE
        {
            return false;
        }
        let (centre, previous) = (self.centre, self.previous);
        let mut contacts = Vec::new();
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut rows = std::mem::take(&mut self.rows);
        let rules = || Rules {
            moving: false,
            direction: [0.0; 3],
            step_assist: false,
            blocked: false,
            replacement: false,
        };
        // Only penetration that is new counts: the torso and head contacts
        // before growing, by shape and triangle, are subtracted. The bottom
        // sphere does not change with the box and is left out (our choice).
        let counted = |c: &Contact| c.shape != Shape::Bottom;
        self.collide(world, &self.shapes, rules(), &mut contacts, &mut scratch);
        let before: Vec<(Shape, u32, f32)> = contacts
            .iter()
            .filter(|c| counted(c))
            .map(|c| (c.shape, c.triangle, c.depth))
            .collect();
        let mut penetration = 0.0f32;
        for k in 1..=POSTURE_GROWTH_STEPS {
            let f = k as f32 / POSTURE_GROWTH_STEPS as f32;
            let shapes = Shapes::of(add(from, scale(sub(to, from), f)));
            for settle in 0..=POSTURE_SETTLE_STEPS {
                self.collide(world, &shapes, rules(), &mut contacts, &mut scratch);
                if k == POSTURE_GROWTH_STEPS && settle == POSTURE_SETTLE_STEPS {
                    penetration = contacts
                        .iter()
                        .filter(|c| counted(c))
                        .map(|c| {
                            let old = before
                                .iter()
                                .find(|b| b.0 == c.shape && b.1 == c.triangle)
                                .map_or(0.0, |b| b.2);
                            c.depth - old
                        })
                        .fold(0.0, f32::max);
                    break;
                }
                let mut v = [0.0; 3];
                solve(&contacts, &mut v, self.config.mass, ITERATIONS, &mut rows);
                self.previous = self.centre;
                self.centre = add(self.centre, scale(v, STEP));
            }
        }
        self.scratch = scratch;
        self.rows = rows;
        if penetration > POSTURE_PENETRATION {
            (self.centre, self.previous) = (centre, previous);
            self.refused = Some((target, now, centre));
            return false;
        }
        self.refused = None;
        self.set_box(target);
        true
    }

    fn set_box(&mut self, index: usize) {
        // `r` stays, so the feet stay (5.2).
        self.box_index = index;
        self.shapes = Shapes::of(self.config.boxes[index]);
    }

    /// The step probe (6.3): whether a solid triangle meets the lower and
    /// the upper box ahead of the feet along `direction` (its horizontal
    /// part; none when it has none, our choice).
    fn probe(&mut self, world: &CollisionWorld, direction: Vec3) -> (bool, bool) {
        let Some(along) = normalize([direction[0], 0.0, direction[2]]) else {
            return (false, false);
        };
        let across = cross(UP, along);
        let r = self.shapes.r;
        let feet = self.position();
        let centre = add(feet, scale(along, PROBE_AHEAD));
        let reach = (0.7 * r).hypot(0.5 * r);
        self.scratch.clear();
        world.query(
            [
                centre[0] - reach,
                feet[1] + PROBE_LOWER_LIFT,
                centre[2] - reach,
            ],
            [
                centre[0] + reach,
                feet[1] + PROBE_UPPER_HEIGHT + 2.0 * r,
                centre[2] + reach,
            ],
            &mut self.scratch,
        );
        // Half-sizes 0.7·r along â and 0.5·r across it: the spec believes
        // this assignment (6.3). The step probe is not skipped in the
        // spec's one unestablished movement state, which is believed not
        // to occur on foot (our choice).
        let axes = [along, UP, across];
        let upper = (
            add(centre, [0.0, PROBE_UPPER_HEIGHT, 0.0]),
            [0.7 * r, 2.0 * r, 0.5 * r],
        );
        let lower = (
            add(centre, [0.0, r + PROBE_LOWER_LIFT, 0.0]),
            [0.7 * r, r, 0.5 * r],
        );
        let (mut lo, mut hi) = (false, false);
        for &t in &self.scratch {
            if !world.is_solid(t) {
                continue;
            }
            let c = world.corners(t);
            lo = lo || geometry::box_intersects(lower.0, axes, lower.1, &c);
            hi = hi || geometry::box_intersects(upper.0, axes, upper.1, &c);
            if lo && hi {
                break;
            }
        }
        (lo, hi)
    }

    /// Generates the contacts of `shapes` at the current centre and applies
    /// the rules of sections 4.1, 4.2, 5.4 and 6.3, in this order: head
    /// sphere, bottom sphere, torso cylinder, each by triangle index (our
    /// choice; the original's order is not specified, and head contacts
    /// first make "no head contact so far" cover the whole step).
    fn collide(
        &self,
        world: &CollisionWorld,
        shapes: &Shapes,
        mut rules: Rules,
        out: &mut Vec<Contact>,
        found: &mut Vec<u32>,
    ) -> Collided {
        out.clear();
        found.clear();
        let c = self.centre;
        let p = self.previous;
        let margin = shapes.r + 0.05;
        let min = [
            c[0].min(p[0]) - margin,
            c[1].min(p[1]) - margin,
            c[2].min(p[2]) - margin,
        ];
        let max = [
            c[0].max(p[0]) + margin,
            c[1].max(p[1]) + shapes.top() + 0.05,
            c[2].max(p[2]) + margin,
        ];
        world.query(min, max, found);
        // climbing.md 1.3: a ladder's box keeps its contacts with the player
        // only while the body is in front of it by front(−0.1); its faces
        // follow the level's triangles here, by ladder and triangle.
        let ladders: Vec<usize> = (world.ladders().iter().enumerate())
            .filter(|(_, l)| {
                (0..3).all(|k| l.min[k] <= max[k] && l.max[k] >= min[k])
                    && l.front(c, shapes.r, -0.1)
            })
            .map(|(i, _)| i)
            .collect();
        let ladder_triangles = ladders.iter().flat_map(|&i| {
            let base = LADDER_TRIANGLES + 12 * i as u32;
            (world.ladders()[i].triangles.iter().enumerate())
                .map(move |(k, t)| (base + k as u32, *t))
        });
        let mut result = Collided {
            traction: 0.0,
            blocked: rules.blocked,
            impact: 0.0,
            material: None,
        };
        // A world with triangles has materials (its mapping found them).
        let Some(player) = world.materials().get(self.material as usize) else {
            return result;
        };
        let small = shapes.small_radius();
        let (torso_at, torso_half) = shapes.torso();
        let head = [0.0, shapes.h, 0.0];
        let torso = [0.0, torso_at, 0.0];
        let (mut solid, mut passing) = (None, None);
        let mut head_seen = false;
        let (world_erp, world_cfm) = world_erp_cfm();
        for shape in [Shape::Head, Shape::Bottom, Shape::Torso] {
            let level = found.iter().map(|&t| (t, world.corners(t)));
            let all = level.chain(ladder_triangles.clone());
            for (t, corners) in all {
                let ladder = t >= LADDER_TRIANGLES;
                let raw = match shape {
                    Shape::Head => geometry::sphere(add(c, head), small, add(p, head), &corners),
                    Shape::Bottom => geometry::sphere(c, shapes.r, p, &corners),
                    Shape::Torso => geometry::cylinder(
                        add(c, torso),
                        small,
                        torso_half,
                        add(p, torso),
                        &corners,
                    ),
                };
                let Some(RawContact {
                    mut normal,
                    mut depth,
                    point,
                }) = raw
                else {
                    continue;
                };
                let generated_normal = normal;
                let material = if ladder {
                    world.ladder_material()
                } else {
                    world.material_index(t)
                };
                let m = &world.materials()[material as usize];
                let pass = m.has(PASS_THROUGH);
                // 4.4: impact speed and the material touched, over the
                // contacts kept (a dropped one is no contact; our reading).
                let mut touch = |normal: Vec3| {
                    // TODO(#391): whether a ladder's contacts give the impact
                    // speed or set the ground material is unestablished
                    // (climbing.md 3.7, 4); here they do neither.
                    if ladder {
                        return;
                    }
                    let v = self.velocity;
                    let speed = if pass {
                        length(v)
                    } else {
                        let vn = dot(v, normal);
                        let tangent = sub(v, scale(normal, vn));
                        vn.abs().max(m.friction * length(tangent))
                    };
                    result.impact = result.impact.max(m.impact_factor() * speed);
                    if pass {
                        passing = Some(material);
                    } else {
                        solid = Some(material);
                    }
                };
                // 4.1: pass-through exerts no force, except actor obstacles.
                if pass && !m.has(ACTOR_OBSTACLE) {
                    touch(normal);
                    continue;
                }
                let against = horizontal_dot(normal, rules.direction) <= 0.0;
                let active = rules.replacement && !head_seen;
                let mut replaced = false;
                match shape {
                    Shape::Head => {
                        head_seen = true;
                        rules.blocked = true;
                    }
                    Shape::Bottom if active && normal[1] <= 0.0 && against => replaced = true,
                    Shape::Bottom if !active && normal[1] < 0.0 => continue,
                    Shape::Torso if active && point[1] < c[1] && against => replaced = true,
                    _ => {}
                }
                touch(normal);
                if replaced {
                    normal = UP;
                    depth = shapes.r + (point[1] - c[1]);
                    if depth <= 0.0 {
                        continue;
                    }
                }
                // 4.2: the pair's parameters.
                let mu = player.friction * m.friction;
                let k = STEP * player.spring * m.spring * WORLD_SPRING;
                let d = player.damping * m.damping * WORLD_DAMPING;
                let mut erp = k / (k + d);
                let mut cfm = 1.0 / (k + d);
                // climbing.md 3.4: climbing and moving, the world's error
                // reduction and twice its constraint-force mixing (friction
                // is zero while moving anyway).
                if self.climbing && rules.moving {
                    (erp, cfm) = (world_erp, 2.0 * world_cfm);
                }
                let soften = |erp: &mut f32, cfm: &mut f32, q: f32, k: f32| {
                    let f = 1.0 / ((1.0 - *erp) * q + *erp * k);
                    *cfm *= f;
                    *erp *= f * k;
                };
                if shape == Shape::Head {
                    soften(&mut erp, &mut cfm, 20.1, LEVEL_SOFTENING);
                }
                soften(&mut erp, &mut cfm, 20.1 + 19.1 * normal[1], LEVEL_SOFTENING);
                // 5.4: traction and friction.
                let value = if shape == Shape::Head || rules.blocked {
                    1.0
                } else {
                    mu
                };
                result.traction = result.traction.max(value);
                let friction = if shape == Shape::Head || rules.moving {
                    0.0
                } else {
                    let assist = if rules.step_assist {
                        STEP_ASSIST_FACTOR
                    } else {
                        1.0
                    };
                    mu * result.traction * assist
                };
                let bounce = (player.has(BOUNCY) && m.has(BOUNCY)).then(|| {
                    (
                        player.bounce.min(m.bounce),
                        player.bounce_start_speed.max(m.bounce_start_speed),
                    )
                });
                out.push(Contact {
                    shape,
                    triangle: t,
                    material,
                    normal,
                    generated_normal,
                    depth,
                    point,
                    replaced,
                    erp,
                    cfm,
                    friction,
                    bounce,
                });
            }
        }
        result.blocked = rules.blocked;
        result.material = passing.or(solid);
        result
    }
}

/// What the physics world's step takes of a movement step
/// ([`PlayerBody::world_step`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldStep {
    /// The body centre at the step's start.
    pub centre: Vec3,
    /// The velocity the step left.
    pub velocity: Vec3,
    pub shapes: Shapes,
    pub mass: f32,
    pub material: u32,
    /// The step's rules (6.2, 5.4): movement requested, step assistance,
    /// the traction `T`, climbing (climbing.md 3.4).
    pub moving: bool,
    pub step_assist: bool,
    pub traction: f32,
    pub climbing: bool,
}

/// A contact of the player's shape with a movable body after the
/// player's rules ([`movable_contact`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovableContact {
    pub erp: f32,
    pub cfm: f32,
    pub friction: f32,
    /// A head contact sets the step's "blocked" flag (5.4, 6.3).
    pub blocked: bool,
}

/// The player's contact rules for a contact of its `shape` with a movable
/// body (level-cform.md 5.4 with docs/behavior/trader-door.md 3.3's
/// differences): `normal` points towards the player's shape, `library_y`
/// is the y of the library's normal as generated for the pair, and `erp`,
/// `cfm` and `mu` are the pair's (physics-contacts.md 2.2). `None` when the
/// rules drop it. The step replacement does not act on these contacts:
/// step assistance against movable bodies is its own spec (TODO(#898)).
pub fn movable_contact(
    shape: Shape,
    normal: Vec3,
    library_y: f32,
    (mut erp, mut cfm, mu): (f32, f32, f32),
    step: &WorldStep,
) -> Option<MovableContact> {
    if shape == Shape::Bottom && normal[1] < 0.0 {
        return None;
    }
    if step.climbing && step.moving {
        let (world_erp, world_cfm) = world_erp_cfm();
        (erp, cfm) = (world_erp, 2.0 * world_cfm);
    }
    let soften = |erp: &mut f32, cfm: &mut f32, q: f32, k: f32| {
        let f = 1.0 / ((1.0 - *erp) * q + *erp * k);
        *cfm *= f;
        *erp *= f * k;
    };
    if shape == Shape::Head {
        soften(&mut erp, &mut cfm, 20.1, LEVEL_SOFTENING);
    }
    soften(&mut erp, &mut cfm, 20.1 - 19.1 * library_y, BODY_SOFTENING);
    // 5.4: a movable body's traction value is 1.
    let traction = step.traction.max(1.0);
    let friction = if shape == Shape::Head || step.moving {
        0.0
    } else {
        let assist = if step.step_assist {
            STEP_ASSIST_FACTOR
        } else {
            1.0
        };
        mu * traction * assist
    };
    Some(MovableContact {
        erp,
        cfm,
        friction,
        blocked: shape == Shape::Head,
    })
}

struct Collided {
    traction: f32,
    blocked: bool,
    impact: f32,
    material: Option<u32>,
}

fn horizontal_dot(n: Vec3, a: Vec3) -> f32 {
    n[0] * a[0] + n[2] * a[2]
}

/// One contact's rows in [`solve`]: normal, two tangents, and the impulses
/// so far.
#[derive(Clone, Debug)]
struct Row {
    n: Vec3,
    t: [Vec3; 2],
    target: f32,
    soft: f32,
    friction: f32,
    normal: f32,
    tangent: [f32; 2],
}

/// Solves the contacts for the velocity `v` (already including the step's
/// external force) of a point of mass `mass`: each contact's normal row
/// drives the approach velocity to `erp·depth/h` (or the bounce), softened
/// by `cfm`, with a non-negative impulse; two friction rows per contact
/// are bounded by `friction ×` the normal impulse.
fn solve(contacts: &[Contact], v: &mut Vec3, mass: f32, iterations: u32, rows: &mut Vec<Row>) {
    rows.clear();
    if contacts.is_empty() {
        return;
    }
    let inv_mass = 1.0 / mass;
    rows.extend(contacts.iter().map(|c| {
        let n = c.normal;
        let seed = if n[1].abs() < 0.9 {
            UP
        } else {
            [1.0, 0.0, 0.0]
        };
        let t0 = normalize(cross(n, seed)).unwrap_or([0.0, 0.0, 1.0]);
        let t1 = cross(n, t0);
        let mut target = c.erp * c.depth / STEP;
        if let Some((k, threshold)) = c.bounce {
            let approach = -dot(n, *v);
            if approach > threshold {
                target = target.max(k * approach);
            }
        }
        Row {
            n,
            t: [t0, t1],
            target,
            // Our interpretation: constraint-force mixing relates
            // force to velocity error, so per impulse it is cfm / h.
            soft: c.cfm / STEP,
            friction: c.friction,
            normal: 0.0,
            tangent: [0.0; 2],
        }
    }));
    for _ in 0..iterations {
        for row in rows.iter_mut() {
            let vn = dot(row.n, *v);
            let delta = (row.target - vn - row.soft * row.normal) / (inv_mass + row.soft);
            let total = (row.normal + delta).max(0.0);
            *v = add(*v, scale(row.n, (total - row.normal) * inv_mass));
            row.normal = total;
            if row.friction <= 0.0 {
                continue;
            }
            let bound = row.friction * row.normal;
            for k in 0..2 {
                let vt = dot(row.t[k], *v);
                let total = (row.tangent[k] - vt * mass).clamp(-bound, bound);
                *v = add(*v, scale(row.t[k], (total - row.tangent[k]) * inv_mass));
                row.tangent[k] = total;
            }
        }
    }
}

/// `direction` turned into the plane with normal `normal`:
/// `(up × direction) × normal`, normalised (6.2).
pub fn plane_direction(direction: Vec3, normal: Vec3) -> Option<Vec3> {
    normalize(cross(cross(UP, direction), normal))
}

/// The drive force's direction (6.2): along the ground plane on walkable
/// ground, otherwise horizontal.
pub fn drive_direction(contacts: &StepContacts) -> Vec3 {
    let d = contacts.direction;
    let horizontal = normalize([d[0], 0.0, d[2]]).unwrap_or([0.0; 3]);
    match contacts.ground {
        Some(n) if contacts.walkable => plane_direction(d, n).unwrap_or(horizontal),
        _ => horizontal,
    }
}

/// The drive force during step assistance (6.2), from the drive `drive`
/// the movement layer computed by its own rule: along the obstacle's plane
/// with magnitude `25·m·T` when there is an obstacle contact; then × 4, its
/// vertical component made non-negative and each horizontal component
/// given the sign of `â`'s (zero where `â`'s is zero, our choice).
pub fn step_assisted_drive(drive: Vec3, contacts: &StepContacts, mass: f32) -> Vec3 {
    let d = contacts.direction;
    let f = match contacts.obstacle.and_then(|n| plane_direction(d, n)) {
        Some(along) => scale(along, STEP_ASSIST_OBSTACLE_DRIVE * mass * contacts.traction),
        None => drive,
    };
    let f = scale(f, STEP_ASSIST_FACTOR);
    let sign = |c: f32, s: f32| if s == 0.0 { 0.0 } else { c.abs().copysign(s) };
    [sign(f[0], d[0]), f[1].max(0.0), sign(f[2], d[2])]
}

/// The side force's coefficient (6.2).
pub fn side_force_coefficient(contacts: &StepContacts) -> f32 {
    if contacts.step_assist {
        STEP_ASSIST_SIDE_FORCE_COEFFICIENT
    } else {
        SIDE_FORCE_COEFFICIENT
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_world::Builder;
    use super::*;

    /// Stand-ins for `player-movement`'s values, for these tests only.
    const TEST_GRAVITY: f32 = 9.8;
    const TEST_DRIVE: f32 = 20.0;

    fn config() -> BodyConfig {
        BodyConfig {
            boxes: [[0.35, 0.9, 0.35], [0.35, 0.75, 0.35], [0.35, 0.6, 0.35]],
            mass: 80.0,
        }
    }

    /// A minimal movement layer: a drive force along [`drive_direction`]
    /// (assisted on steps), gravity, no speed cap.
    fn walk(body: &mut PlayerBody, world: &CollisionWorld, a: Vec3, steps: u32) -> StepResult {
        let mass = body.config.mass;
        let mut last = StepResult::default();
        for _ in 0..steps {
            let input = StepInput {
                acceleration: a,
                jump_requested: false,
            };
            last = body.step(world, &input, |c| {
                let mut drive = if c.moving {
                    scale(drive_direction(c), TEST_DRIVE * mass * c.traction)
                } else {
                    [0.0; 3]
                };
                if c.moving && c.step_assist {
                    drive = step_assisted_drive(drive, c, mass);
                }
                (add(drive, [0.0, -TEST_GRAVITY * mass, 0.0]), false)
            });
            // A crude speed cap standing in for player-movement's.
            let v = body.velocity();
            let h = v[0].hypot(v[2]);
            if h > 3.0 {
                body.set_velocity([v[0] * 3.0 / h, v[1], v[2] * 3.0 / h]);
            }
        }
        last
    }

    #[test]
    fn shapes_follow_the_boxes() {
        let s = Shapes::of([0.35, 0.9, 0.35]);
        assert!((s.r - 0.35).abs() < 1e-6 && (s.h - 1.1).abs() < 1e-6);
        let (at, half) = s.torso();
        // 7.4: torso length 1.196735 centred 0.803265 above the feet.
        assert!((2.0 * half - 1.196_735).abs() < 1e-5);
        assert!((at + s.r - 0.803_265).abs() < 1e-5);
        assert!((s.small_radius() - 0.291_667).abs() < 1e-5);
        assert!((s.top() + s.r - 1.741_667).abs() < 1e-5);
        assert!((Shapes::of([0.35, 0.6, 0.35]).top() + 0.35 - 1.141_667).abs() < 1e-5);
        assert_eq!(Shapes::of([0.35, 0.2, 0.35]).h, 0.01);
    }

    #[test]
    fn contact_parameters_match_the_spec() {
        let mut b = Builder::default();
        b.floor([-5.0, 5.0], [-5.0, 5.0], 0.0, true);
        let world = b.world();
        let mut body = PlayerBody::new(config(), [0.0, -0.01, 0.0]);
        let c = body.begin_step(&world, &StepInput::default());
        assert!(c.walkable && c.traction == 1.0 && !c.moving);
        // Section 4.2's example: error reduction 0.375 and constraint-force
        // mixing 1.5625e-6 with all factors 1 and h = 0.01; the floor
        // softening of 5.4, q = 39.2, then gives f·0.5·e.
        let e = 0.375f32;
        let f = 1.0 / ((1.0 - e) * 39.2 + e * 0.5);
        let contact = body.contacts()[0];
        assert!((contact.erp - f * 0.5 * e).abs() < 1e-6);
        assert!((contact.cfm - 1.5625e-6 * f).abs() < 1e-10);
        assert_eq!(contact.friction, 1.0);
    }

    #[test]
    fn rests_on_a_floor_and_slides_along_walls() {
        let mut b = Builder::default();
        b.floor([-10.0, 10.0], [-10.0, 10.0], 0.0, true);
        b.wall_x(1.0, [0.0, 3.0], [-10.0, 10.0], true);
        let world = b.world();
        let mut body = PlayerBody::new(config(), [0.0, 0.0, 0.0]);
        let r = walk(&mut body, &world, [0.0; 3], 100);
        assert!(r.position[1].abs() < 0.01, "{:?}", r.position);
        assert!(!r.airborne);
        // Diagonally into the wall: blocked in x, sliding in z.
        let r = walk(&mut body, &world, [1.0, 0.0, 1.0], 200);
        assert!(
            r.position[0] < 1.0 - 0.34 && r.position[0] > 0.5,
            "{:?}",
            r.position
        );
        assert!(r.position[2] > 2.0, "{:?}", r.position);
        assert!(r.position[1].abs() < 0.02, "{:?}", r.position);
    }

    /// A step of `height` rising at x = 1 across the whole z range.
    fn stair(height: f32) -> CollisionWorld {
        let mut b = Builder::default();
        b.floor([-10.0, 1.0], [-10.0, 10.0], 0.0, true);
        b.wall_x(1.0, [0.0, height], [-10.0, 10.0], true);
        b.floor([1.0, 10.0], [-10.0, 10.0], height, true);
        b.world()
    }

    #[test]
    fn climbs_a_step_within_the_limit() {
        for height in [0.2, 0.4] {
            let world = stair(height);
            let mut body = PlayerBody::new(config(), [0.0, 0.0, 0.0]);
            walk(&mut body, &world, [0.0; 3], 20);
            let r = walk(&mut body, &world, [1.0, 0.0, 0.0], 300);
            assert!(r.position[0] > 2.0, "{height}: {:?}", r.position);
            assert!(
                (r.position[1] - height).abs() < 0.03,
                "{height}: {:?}",
                r.position
            );
        }
    }

    #[test]
    fn stops_at_a_step_above_the_limit() {
        let world = stair(1.0);
        let mut body = PlayerBody::new(config(), [0.0, 0.0, 0.0]);
        walk(&mut body, &world, [0.0; 3], 20);
        let c = body.begin_step(
            &world,
            &StepInput {
                acceleration: [1.0, 0.0, 0.0],
                ..StepInput::default()
            },
        );
        body.finish_step([0.0, -TEST_GRAVITY * 80.0, 0.0], false);
        assert!(!c.step_assist);
        let r = walk(&mut body, &world, [1.0, 0.0, 0.0], 300);
        assert!(r.position[0] < 1.0 - 0.34, "{:?}", r.position);
        assert!(r.position[1] < 0.05, "{:?}", r.position);
    }

    /// A slope rising along +x at `degrees`.
    fn slope(degrees: f32) -> CollisionWorld {
        let rise = 20.0 * degrees.to_radians().tan();
        let mut b = Builder::default();
        b.quad(
            [
                [-10.0, -rise / 2.0, -10.0],
                [-10.0, -rise / 2.0, 10.0],
                [10.0, rise / 2.0, 10.0],
                [10.0, rise / 2.0, -10.0],
            ],
            1,
        );
        b.world()
    }

    #[test]
    fn steep_slopes_are_not_walkable_and_slide() {
        for (degrees, walkable) in [(30.0f32, true), (60.0, false)] {
            let world = slope(degrees);
            // Touching the slope: the centre r along its normal.
            let (sin, cos) = degrees.to_radians().sin_cos();
            let r = 0.35 - 0.001;
            let mut body = PlayerBody::new(config(), [-sin * r, cos * r - 0.35, 0.0]);
            let c = body.begin_step(&world, &StepInput::default());
            body.finish_step([0.0; 3], false);
            assert_eq!(c.walkable, walkable, "{degrees}");
            let n = c.ground.unwrap();
            assert!(
                (n[1] - degrees.to_radians().cos()).abs() < 1e-3,
                "{degrees}: {n:?}"
            );
            let start = body.position();
            let r = walk(&mut body, &world, [0.0; 3], 100);
            let moved = length(sub(r.position, start));
            if walkable {
                // Held by friction (tan 30° < μ = 1).
                assert!(moved < 0.02, "{degrees}: moved {moved}");
            } else {
                assert!(
                    moved > 0.3 && r.position[0] < start[0],
                    "{degrees}: {:?}",
                    r.position
                );
            }
        }
    }

    #[test]
    fn a_ceiling_blocks_standing_up() {
        for (ceiling, fits) in [(1.6, false), (2.0, true)] {
            let mut b = Builder::default();
            b.floor([-5.0, 5.0], [-5.0, 5.0], 0.0, true);
            b.floor([-5.0, 5.0], [-5.0, 5.0], ceiling, false);
            let world = b.world();
            let mut body = PlayerBody::new(config(), [0.0, 0.0, 0.0]);
            assert!(body.request_box(&world, 1, 0));
            assert_eq!(body.box_index(), 1);
            walk(&mut body, &world, [0.0; 3], 10);
            let feet = body.position();
            assert_eq!(body.request_box(&world, 0, 100), fits, "ceiling {ceiling}");
            assert_eq!(body.box_index(), if fits { 0 } else { 1 });
            assert!(length(sub(body.position(), feet)) < 0.02);
            if !fits {
                // A refused target is not tested again within 500 ms unless
                // the body moved 0.05 m: in a world where it would fit now,
                // still refused at 300 ms; tested again at 600 ms, or after
                // moving 0.06 m.
                let mut open = Builder::default();
                open.floor([-5.0, 5.0], [-5.0, 5.0], 0.0, true);
                let open = open.world();
                let mut moved = body.clone();
                assert!(!body.request_box(&open, 0, 300));
                assert!(body.request_box(&open, 0, 600));
                assert_eq!(body.box_index(), 0);
                let feet = moved.position();
                moved.set_position([feet[0] + 0.06, feet[1], feet[2]]);
                assert!(moved.request_box(&open, 0, 300));
                // A lower box always.
                let mut low = PlayerBody::new(config(), [0.0, 0.0, 0.0]);
                assert!(low.request_box(&world, 1, 0));
                assert!(low.request_box(&world, 2, 0));
                assert_eq!(low.box_index(), 2);
            }
        }
    }

    #[test]
    fn falls_and_lands() {
        let mut b = Builder::default();
        b.floor([-5.0, 5.0], [-5.0, 5.0], 0.0, true);
        b.floor([-5.0, -1.0], [-5.0, 5.0], 2.0, true);
        let world = b.world();
        // Walk off a 2 m ledge.
        let mut body = PlayerBody::new(config(), [-1.5, 2.0, 0.0]);
        walk(&mut body, &world, [0.0; 3], 10);
        let mut airborne = false;
        let mut landed = 0;
        let mut impact = 0.0f32;
        for _ in 0..300 {
            let a = if body.position()[0] < 0.0 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0; 3]
            };
            let mass = body.config.mass;
            let r = body.step(
                &world,
                &StepInput {
                    acceleration: a,
                    ..StepInput::default()
                },
                |c| {
                    impact = impact.max(c.impact_speed);
                    let drive = if c.moving && c.walkable {
                        scale(drive_direction(c), TEST_DRIVE * mass)
                    } else {
                        [0.0; 3]
                    };
                    (add(drive, [0.0, -TEST_GRAVITY * mass, 0.0]), false)
                },
            );
            airborne |= r.airborne;
            landed += u32::from(r.landed);
        }
        assert!(airborne, "became airborne");
        assert_eq!(landed, 1);
        assert!(!body.airborne());
        assert!(body.position()[1].abs() < 0.05, "{:?}", body.position());
        // Factor 1 is 1 in the test library; the fall is ~6 m/s.
        assert!(impact > 5.0 && impact < 7.5, "{impact}");
        assert_eq!(body.ground_material(), Some(1));
    }

    #[test]
    fn passes_through_pass_through_walls_but_not_actor_obstacles() {
        // Material ids of the test library: 2 pass-through, 3 also an actor
        // obstacle (section 3.2).
        for (material, blocks) in [(2, false), (3, true)] {
            let mut b = Builder::default();
            b.floor([-10.0, 10.0], [-10.0, 10.0], 0.0, true);
            let (y, z) = ([0.0, 3.0], [-10.0, 10.0]);
            b.quad(
                [
                    [1.0, y[0], z[1]],
                    [1.0, y[1], z[1]],
                    [1.0, y[1], z[0]],
                    [1.0, y[0], z[0]],
                ],
                material,
            );
            let world = b.world();
            let mut body = PlayerBody::new(config(), [0.0, 0.0, 0.0]);
            let r = walk(&mut body, &world, [1.0, 0.0, 0.0], 150);
            assert_eq!(
                r.position[0] < 1.0,
                blocks,
                "material {material}: {:?}",
                r.position
            );
            // The pass-through material is the one touched (4.4).
            if !blocks {
                let mut body = PlayerBody::new(config(), [0.8, 0.0, 0.0]);
                body.begin_step(&world, &StepInput::default());
                assert_eq!(body.ground_material(), Some(2));
            }
        }
    }

    #[test]
    fn step_assisted_drive_follows_the_obstacle() {
        let contacts = StepContacts {
            moving: true,
            direction: [1.0, 0.0, 0.0],
            obstacle: Some([-1.0, 0.0, 0.0]),
            traction: 1.0,
            step_assist: true,
            ..StepContacts::default()
        };
        let f = step_assisted_drive([5.0, -3.0, 0.0], &contacts, 80.0);
        assert!(
            f[0].abs() < 1e-3 && (f[1] - 4.0 * 25.0 * 80.0).abs() < 1e-2,
            "{f:?}"
        );
        let f = step_assisted_drive(
            [-5.0, -3.0, 2.0],
            &StepContacts {
                obstacle: None,
                ..contacts
            },
            80.0,
        );
        assert_eq!(f, [20.0, 0.0, 0.0]);
        assert_eq!(side_force_coefficient(&contacts), 700.0);
        assert_eq!(
            plane_direction([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            Some([1.0, 0.0, 0.0])
        );
    }

    /// A door leaf's box standing across x = `x` (its near face there),
    /// turned `angle` about y, of material index 4.
    fn leaf(x: f32, angle: f32) -> BodyBox {
        let (s, c) = angle.sin_cos();
        BodyBox {
            centre: [x + 0.125, 1.322, 0.0],
            axes: [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]],
            half_size: [0.125, 1.322, 0.69],
            material: 4,
        }
    }

    #[test]
    fn a_body_box_faces_out_and_is_met_by_rays() {
        let b = leaf(1.0, 0.3);
        for t in b.triangles() {
            let n = geometry::front_normal(&t).unwrap();
            let mid = scale(add(add(t[0], t[1]), t[2]), 1.0 / 3.0);
            assert!(dot(n, sub(mid, b.centre)) > 0.0, "{t:?}");
        }
        let b = leaf(1.0, 0.0);
        assert_eq!(b.bounds(), ([1.0, 0.0, -0.69], [1.25, 2.644, 0.69]));
        assert_eq!(b.ray([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]), Some(1.0));
        assert_eq!(b.ray([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]), None);
        assert_eq!(b.ray([0.0, 1.0, 2.0], [1.0, 0.0, 0.0]), None);
        assert_eq!(b.ray([1.1, 1.0, 0.0], [1.0, 0.0, 0.0]), Some(0.0));
        assert!((b.distance([0.5, 1.0, 0.0]) - 0.5).abs() < 1e-6);
        assert_eq!(b.distance([1.1, 1.0, 0.0]), 0.0);
    }
}
