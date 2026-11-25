// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Artefacts by themselves (docs/behavior/artefact-idle-effects.md, "P10"):
//! what an artefact object reads from its section (section 1.2), its
//! states (2), its creation and destruction (3), its idle effect at bone 0
//! of its model (4), its light (5) and its updates (6).
//!
//! The effect objects are the level's particle objects
//! ([`crate::particles::placed`]); the light is kept here as its record's
//! values and given to its record among the level's dynamic lights (P16)
//! by [`Artefacts::sync_lights`].
//!
//! What waits for other specifications:
//! - Activation (section 7): the player never activates an artefact (7.1),
//!   and its phases, light and phase effects are TODO(#409).
//! - Physics (6.1 step 1, 6.4): an artefact stays where its record put it,
//!   is processed every frame (the rule by which the physics simulation
//!   makes it sleep is unestablished) and the jumping behaviour's impulse
//!   moves nothing (TODO(#278)).
//! - Motions (3.1 step 5, 4.1): the models are not animated, so bone 0
//!   stays at rest; under `idle` only `artefact_rusty_hairs`' bone 0 is
//!   elsewhere, by 7 mm (TODO(#234)).
//! - Carried artefacts follow no parent's transform (6.4): in a
//!   single-player game they have no effect and no light, so nothing shows
//!   it (TODO(#255)).

use std::io;

use volya_engine_formats::ltx::{Section, value};

use crate::lights::{LightId, Lights};
use crate::particles::placed::{Origin, PlacedEffects, period};
use crate::particles::{Matrix, Vec3, place_point};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The two in-game behaviours (P10 1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behaviour {
    /// `ARTEFACT`.
    Jumping,
    /// `SCRPTART`.
    Plain,
}

impl Behaviour {
    /// The behaviour of `class_id`; `None` for other class ids.
    pub fn of(class_id: &str) -> Option<Self> {
        match class_id {
            "ARTEFACT" => Some(Self::Jumping),
            "SCRPTART" => Some(Self::Plain),
            _ => None,
        }
    }
}

/// The light's keys (P10 1.2).
#[derive(Clone, Debug, PartialEq)]
pub struct TrailLight {
    /// `trail_light_color`: red, green, blue.
    pub rgb: Vec3,
    /// `trail_light_range` (m).
    pub range: f32,
}

/// What an artefact object reads from its section (P10 1.2).
#[derive(Clone, Debug, PartialEq)]
pub struct ArtefactSection {
    pub name: String,
    pub behaviour: Behaviour,
    /// `particles`: the idle effect.
    pub particles: Option<String>,
    /// `lights_enabled` on, with its keys.
    pub light: Option<TrailLight>,
    /// `jump_height` (m): `ARTEFACT` only, 0 when absent.
    pub jump_height: f32,
}

impl ArtefactSection {
    /// Reads `section` as an artefact of `behaviour` reads it. A required
    /// key that is missing is fatal (ltx-config.md): an error.
    pub fn read(section: &Section, behaviour: Behaviour) -> io::Result<Self> {
        let name = section.name();
        let at = |e: io::Error| invalid(format!("[{name}]: {e}"));
        let light = if value::bool(section.read("lights_enabled").map_err(at)?) {
            Some(TrailLight {
                rgb: value::floats::<3>(section.read("trail_light_color").map_err(at)?),
                range: value::float(section.read("trail_light_range").map_err(at)?),
            })
        } else {
            None
        };
        // An empty `particles`, which no shipped section has, names no
        // effect (4.2: "if the effect name is not empty").
        let particles = (section.get("particles").flatten())
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        let jump_height = match behaviour {
            Behaviour::Jumping => section
                .get("jump_height")
                .flatten()
                .map_or(0.0, value::float),
            Behaviour::Plain => 0.0,
        };
        Ok(Self {
            name: name.to_owned(),
            behaviour,
            particles,
            light,
            jump_height,
        })
    }
}

/// The start matrix *S* of a direction **d** (P10 4.2 step 1).
///
/// TODO(#409): the rule for |*d*ₓ| ≥ |*d*ᵧ| is not given (a review gap);
/// only activation effects may need it, and the same rule is used.
pub fn start_matrix(d: Vec3) -> Matrix {
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let z = [d[0] / l, d[1] / l, d[2] / l];
    let m = (z[1] * z[1] + z[2] * z[2]).sqrt();
    let y = [0.0, z[2] / m, -z[1] / m];
    let x = [
        y[1] * z[2] - y[2] * z[1],
        y[2] * z[0] - y[0] * z[2],
        y[0] * z[1] - y[1] * z[0],
    ];
    [
        [x[0], x[1], x[2], 0.0],
        [y[0], y[1], y[2], 0.0],
        [z[0], z[1], z[2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// The angles (*h*, *p*, *b*) kept from a start matrix *S* (P10 4.3).
///
/// TODO(#409): the general case (*w* above 1.9073486 × 10⁻⁶) is not
/// given (a review gap) and is never reached by an idle start; it is taken
/// as the special one.
pub fn kept_angles(s: &Matrix) -> Vec3 {
    let w = (s[0][1] * s[0][1] + s[1][1] * s[1][1]).sqrt();
    let h = -(-s[0][2]).atan2(s[0][0]);
    let p = -(-s[2][1]).atan2(w);
    [h, p, 0.0]
}

/// The update rotation *U* of kept angles (*h*, *p*, *b*) (P10 4.3): the
/// rotation of level-objects.md 3.4 with *x* = *p*, *y* = *h*, *z* = *b*,
/// as a matrix with row 3 (0, 0, 0, 1).
pub fn update_rotation(angles: Vec3) -> Matrix {
    let [h, p, b] = angles;
    volya_engine_formats::model::transform([0.0; 3], [p, h, b])
}

/// The idle effect's direction (P10 4.2).
const UP: Vec3 = [0.0, 1.0, 0.0];
/// The fast flag's distance from the model's sphere (m, P10 6.2).
const FAST_DISTANCE: f32 = 50.0;

/// What the artefact object takes from its record and model.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtefactRecord {
    pub id: u16,
    pub name: String,
    /// The object transform *X* (level-objects.md 3.4).
    pub transform: Matrix,
    /// It is created with a parent (carried).
    pub parent: bool,
    /// Row 3 of bone 0's model-space matrix *T*₀ at rest (P10 4.1).
    pub bone: Vec3,
    /// The model's bounding sphere in model space: centre, radius (6.2).
    pub sphere: (Vec3, f32),
}

/// An effect object at the effect list's point (P10 4.1, 4.2).
#[derive(Clone, Debug, PartialEq)]
pub struct ListEffect {
    pub name: String,
    pub object: usize,
    /// The kept angles (4.3).
    pub angles: Vec3,
}

/// The artefact's states (P10 2) while it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Lying,
    Carried,
}

/// One artefact object.
#[derive(Clone, Debug)]
pub struct Artefact {
    pub id: u16,
    pub name: String,
    pub section: ArtefactSection,
    /// *X*; row 3 is **p**.
    pub transform: Matrix,
    pub state: State,
    bone: Vec3,
    sphere: (Vec3, f32),
    /// The effect list's one point's effect objects (4.1).
    pub effects: Vec<ListEffect>,
    /// A start or an earlier update found something playing (4.4).
    list_active: bool,
    /// The list's stored velocity (6.3).
    velocity: Vec3,
    pub fast: bool,
    /// Its scheduled update: next due (clock, ms).
    due: u32,
    /// The light is on (5.2); only with `lights_enabled`.
    pub light_on: bool,
    /// The light record among the level's dynamic lights, once made.
    pub record: Option<LightId>,
}

impl Artefact {
    /// **p**, row 3 of *X*.
    pub fn position(&self) -> Vec3 {
        [
            self.transform[3][0],
            self.transform[3][1],
            self.transform[3][2],
        ]
    }

    /// The effect list's point **q** = **o** · *T*₀ · *X* (4.1).
    pub fn point(&self) -> Vec3 {
        place_point(self.bone, &self.transform)
    }

    /// The model's sphere placed by *X* (6.2).
    pub fn placed_sphere(&self) -> (Vec3, f32) {
        (place_point(self.sphere.0, &self.transform), self.sphere.1)
    }

    /// The effect start (4.2) of `name` with direction `d`; life −1 and
    /// auto-remove off (TODO(#409): lives are the activation's).
    fn effect_start(
        &mut self,
        name: &str,
        d: Vec3,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        let s = start_matrix(d);
        let object = match self.effects.iter().position(|e| e.name == name) {
            Some(k) => self.effects[k].object,
            None => {
                let object = effects.add(name, Origin::Artefact)?;
                self.effects.push(ListEffect {
                    name: name.to_owned(),
                    object,
                    angles: [0.0; 3],
                });
                object
            }
        };
        let k = self.effects.iter().position(|e| e.object == object);
        if let Some(k) = k {
            self.effects[k].angles = kept_angles(&s);
        }
        let q = self.point();
        let mut m = s;
        m[3] = [q[0], q[1], q[2], 1.0];
        effects.place_registered(object, &m, [0.0; 3]);
        if !effects.get(object).is_some_and(|p| p.instance.is_playing()) {
            effects.play(object);
        }
        self.list_active = true;
        Ok(())
    }

    /// The idle effect's start (4.2, 4.7).
    fn idle_start(&mut self, effects: &mut PlacedEffects) -> Result<(), String> {
        match self.section.particles.clone() {
            Some(name) => self.effect_start(&name, UP, effects),
            None => Ok(()),
        }
    }

    /// The effect list's update (4.4).
    fn list_update(&mut self, effects: &mut PlacedEffects) {
        if !self.list_active {
            return;
        }
        let q = self.point();
        let mut playing = false;
        let velocity = self.velocity;
        self.effects.retain(|e| {
            let mut m = update_rotation(e.angles);
            m[3] = [q[0], q[1], q[2], 1.0];
            effects.place_registered(e.object, &m, velocity);
            if effects
                .get(e.object)
                .is_some_and(|p| p.instance.is_playing())
            {
                playing = true;
                true
            } else {
                effects.remove(e.object);
                false
            }
        });
        self.list_active = playing;
    }

    /// Destroys every effect object named `name` (4.6), then the list's
    /// update.
    fn destroy_effects(&mut self, name: &str, effects: &mut PlacedEffects) {
        self.effects.retain(|e| {
            if e.name == name {
                effects.remove(e.object);
                false
            } else {
                true
            }
        });
        self.list_update(effects);
    }

    /// Lights on (5.2).
    fn lights_on(&mut self) {
        self.light_on = self.section.light.is_some();
    }

    /// Lights off (5.2).
    fn lights_off(&mut self) {
        self.light_on = false;
    }

    /// The workload (6.3): the stored velocity (the parent's velocity when
    /// carried: TODO(#255), no parent's is known, 0); the light follows
    /// **p** at [`Artefacts::sync_lights`]; the behaviour's own work.
    fn workload(&mut self) {
        self.velocity = [0.0; 3];
        // TODO(#278): the jumping behaviour's ray and impulse (6.4) move a
        // physics shell, which does not exist yet; a carried artefact
        // would take its parent's transform (TODO(#255)).
    }
}

/// What a frame's updates read.
pub struct Frame {
    /// The clock *C* (ms).
    pub clock: u32,
    /// The camera position *E*.
    pub camera: Vec3,
}

/// The level's artefact objects.
#[derive(Clone, Debug, Default)]
pub struct Artefacts {
    artefacts: Vec<Artefact>,
    /// Light records of destroyed artefacts, to release.
    released: Vec<LightId>,
}

impl Artefacts {
    pub fn artefacts(&self) -> &[Artefact] {
        &self.artefacts
    }

    /// The artefact of object `id`.
    pub fn get(&self, id: u16) -> Option<&Artefact> {
        self.artefacts.iter().find(|a| a.id == id)
    }

    pub fn contains(&self, id: u16) -> bool {
        self.get(id).is_some()
    }

    /// Creates the artefact of `record`, read with `section`, as it comes
    /// online (P10 3.1): the effect start, the light record (made by
    /// [`Artefacts::sync_lights`]), lights on, the fast flag cleared. One
    /// created with a parent is then taken at once (believed, P10 10). An
    /// effect name that is neither an effect nor a group is the game's
    /// assertion failure: an error.
    pub fn create(
        &mut self,
        record: &ArtefactRecord,
        section: ArtefactSection,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        let mut a = Artefact {
            id: record.id,
            name: record.name.clone(),
            section,
            transform: record.transform,
            state: State::Lying,
            bone: record.bone,
            sphere: record.sphere,
            effects: Vec::new(),
            list_active: false,
            velocity: [0.0; 3],
            fast: false,
            due: effects.clock(),
            light_on: false,
            record: None,
        };
        a.idle_start(effects)?;
        a.lights_on();
        self.artefacts.push(a);
        if record.parent {
            self.take(record.id, effects);
        }
        Ok(())
    }

    /// Destroys the artefact of object `id` (P10 3.2): its effects, lights
    /// off, its light record released.
    pub fn destroy(&mut self, id: u16, effects: &mut PlacedEffects) {
        let Some(k) = self.artefacts.iter().position(|a| a.id == id) else {
            return;
        };
        let a = self.artefacts.remove(k);
        for e in &a.effects {
            effects.remove(e.object);
        }
        self.released.extend(a.record);
    }

    /// The artefact of object `id` is taken by a parent (P10 4.6) in a
    /// single-player game: lights off, its idle effect destroyed.
    pub fn take(&mut self, id: u16, effects: &mut PlacedEffects) {
        let Some(a) = self.artefacts.iter_mut().find(|a| a.id == id) else {
            return;
        };
        a.state = State::Carried;
        a.lights_off();
        if let Some(name) = a.section.particles.clone() {
            a.destroy_effects(&name, effects);
        }
    }

    /// The artefact of object `id` leaves its parent at transform
    /// `transform` (P10 4.7): lights on, its idle effect started again.
    pub fn release(
        &mut self,
        id: u16,
        transform: Matrix,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        let Some(a) = self.artefacts.iter_mut().find(|a| a.id == id) else {
            return Ok(());
        };
        a.state = State::Lying;
        a.transform = transform;
        a.lights_on();
        a.idle_start(effects)
    }

    /// One frame of the artefacts: the pass over the game's objects, in
    /// which every artefact has its client update (6.1), then the
    /// scheduler's pass over those due (6.2; every due one, the
    /// scheduler's budget being unestablished). Runs only while the game
    /// is not paused.
    pub fn frame(&mut self, frame: &Frame, effects: &mut PlacedEffects) {
        for a in &mut self.artefacts {
            a.list_update(effects);
            if a.fast {
                a.workload();
            }
        }
        for a in &mut self.artefacts {
            if (frame.clock.wrapping_sub(a.due) as i32) < 0 {
                continue;
            }
            let p = a.position();
            let d = (0..3)
                .map(|k| f64::from(frame.camera[k] - p[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            a.due = frame.clock.wrapping_add(period(d));
            // The kept frame number's test (6.2) is left out: believed to
            // hold only in the device's frame 0, before any level runs.
            let (c, r) = a.placed_sphere();
            let e: f32 = (0..3)
                .map(|k| (frame.camera[k] - c[k]).powi(2))
                .sum::<f32>()
                .sqrt();
            a.fast = a.state == State::Lying && e - r < FAST_DISTANCE;
            if !a.fast {
                a.workload();
            }
        }
    }

    /// Gives the lights' values to their records among the level's dynamic
    /// lights (docs/behavior/dynamic-lights.md), making each record the
    /// first time (P10 3.1, 5.1: shadow on, the type the default point),
    /// and releases the destroyed artefacts' records. With
    /// `lights_enabled` on, lights on sets the colour, the range and the
    /// position **p** (5.2), which then follows **p** (5.3); unchanged
    /// values change nothing, so it runs after every frame's updates.
    pub fn sync_lights(&mut self, lights: &mut Lights) {
        for id in self.released.drain(..) {
            lights.set_active(id, false);
            lights.destroy(id);
        }
        for a in &mut self.artefacts {
            let id = *a.record.get_or_insert_with(|| {
                let id = lights.create();
                lights.set_shadow(id, true);
                id
            });
            if let (Some(l), true) = (&a.section.light, a.light_on) {
                lights.set_rgb(id, l.rgb);
                lights.set_range(id, l.range);
                lights.set_position(id, a.position());
            }
            lights.set_active(id, a.light_on);
        }
    }
}

#[cfg(test)]
mod tests;
