// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! An effect instance (particle-lifecycle.md): its own copy of the record's
//! action list, its live particles, placement (section 5), play and stop
//! (section 7) and the update in 33 ms steps (section 8).

use volya_engine_formats::particles::{ActionKind, Effect, action_flags, effect_flags};

use super::actions::{Step, run};
use super::region::place;
use super::{
    Bounds, IDENTITY, Matrix, Particle, Random, STEP, STEP_MS, Sphere, Vec3, place_direction,
    place_point, scale, translation_only, without_translation,
};

/// How an owner places an instance (P2 5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    World,
    Local,
}

/// A particle birth or death reported to the owner (P2 3.2), for groups.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Notice {
    /// After the particle is written, before the count grows (its index is
    /// the new last one).
    Birth { index: usize, position: Vec3 },
    /// Before the last particle moves into place `index`.
    Death {
        index: usize,
        position: Vec3,
        previous: Vec3,
    },
}

/// An action of the instance's list: its stored values, the placed ones
/// the steps read, its flag word (with the run-time `source` switch) and
/// `turbulence`'s time.
#[derive(Clone, Debug)]
struct InstanceAction {
    stored: ActionKind,
    placed: ActionKind,
    flags: u32,
    time: f32,
}

/// An effect instance.
#[derive(Clone, Debug)]
pub struct EffectInstance {
    /// The effect's index in the library.
    record: usize,
    flags: u32,
    max: usize,
    time_limit: f32,
    frames: volya_engine_formats::particles::Frames,
    actions: Vec<InstanceAction>,
    particles: Vec<Particle>,
    playing: bool,
    stopping: bool,
    local: bool,
    /// The kept matrix (local mode).
    matrix: Matrix,
    position: Vec3,
    remaining: f32,
    accumulator: u32,
    bounds: Bounds,
    sphere: Sphere,
    /// Births and deaths for the owner, when it listens (groups).
    pub(crate) notices: Option<Vec<Notice>>,
}

impl EffectInstance {
    /// A new instance given `effect`, its record at `record` in the library
    /// (P2 8.1): not playing, remaining time *T* when time-limited,
    /// accumulator 0, at (0, 0, 0) with the identity kept and the list as
    /// stored.
    pub fn new(record: usize, effect: &Effect) -> Self {
        let actions = effect
            .actions
            .iter()
            .map(|a| InstanceAction {
                stored: a.kind.clone(),
                placed: a.kind.clone(),
                flags: a.flags,
                time: 0.0,
            })
            .collect();
        let timed = effect.flags & effect_flags::TIME_LIMIT != 0;
        Self {
            record,
            flags: effect.flags,
            max: effect.max_particles as usize,
            time_limit: effect.time_limit,
            frames: effect.frames,
            actions,
            particles: Vec::new(),
            playing: false,
            stopping: false,
            local: false,
            matrix: IDENTITY,
            position: [0.0; 3],
            remaining: if timed { effect.time_limit } else { 0.0 },
            accumulator: 0,
            bounds: Bounds::around([0.0; 3]),
            sphere: Bounds::around([0.0; 3]).sphere(),
            notices: None,
        }
    }

    /// The effect's index in the library.
    pub fn record(&self) -> usize {
        self.record
    }

    /// The live particles, in list order (not birth order).
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    #[cfg(test)]
    pub(crate) fn particles_mut(&mut self) -> &mut Vec<Particle> {
        &mut self.particles
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn is_stopping(&self) -> bool {
        self.stopping
    }

    /// Local mode, with the kept matrix the particles are drawn by.
    pub fn local(&self) -> Option<&Matrix> {
        self.local.then_some(&self.matrix)
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub fn sphere(&self) -> Sphere {
        self.sphere
    }

    /// Looped: its length is −1, no time limit (P2 7, P6 4).
    pub fn looped(&self) -> bool {
        self.flags & effect_flags::TIME_LIMIT == 0
    }

    /// Places the instance (P2 5.1, 5.2).
    pub fn place(&mut self, m: &Matrix, w: Vec3, mode: Mode) {
        if mode == Mode::Local {
            self.local = true;
            self.matrix = *m;
            return;
        }
        self.local = false;
        self.position = [m[3][0], m[3][1], m[3][2]];
        let full = *m;
        let moved = translation_only(m);
        for a in &mut self.actions {
            let m = if a.flags & action_flags::TRANSFORM != 0 {
                &full
            } else {
                &moved
            };
            a.placed = place_action(&a.stored, m, w);
        }
    }

    /// Play (P2 7): sources on, `turbulence`'s time 0.
    pub fn play(&mut self) {
        self.stopping = false;
        self.playing = true;
        for a in &mut self.actions {
            match a.placed {
                ActionKind::Source(_) => a.flags &= !action_flags::SILENT,
                ActionKind::Turbulence { .. } => a.time = 0.0,
                _ => {}
            }
        }
    }

    fn silence(&mut self) {
        for a in &mut self.actions {
            if matches!(a.placed, ActionKind::Source(_)) {
                a.flags |= action_flags::SILENT;
            }
        }
    }

    /// Deferred stop: the particles live on until none is left.
    pub fn stop_deferred(&mut self) {
        self.silence();
        self.stopping = true;
    }

    /// Immediate stop: every particle removed at once, without notices.
    pub fn stop_immediate(&mut self) {
        self.silence();
        self.particles.clear();
        self.playing = false;
    }

    /// The update with `e` ms of elapsed real time (P2 8.2).
    pub fn update(&mut self, e: u32, random: &mut Random) {
        if !self.playing {
            self.bounds = Bounds::around(self.position);
            self.sphere = self.bounds.sphere();
            return;
        }
        self.accumulator = self.accumulator.wrapping_add(e);
        if self.accumulator < STEP_MS {
            return;
        }
        let n = (self.accumulator / STEP_MS).min(3);
        self.accumulator %= STEP_MS;
        for _ in 0..n {
            if !self.step(random) {
                break;
            }
        }
    }

    /// One step (P2 8.3); `false` when it ends the update.
    fn step(&mut self, random: &mut Random) -> bool {
        if self.flags & effect_flags::TIME_LIMIT != 0 && !self.stopping {
            self.remaining -= STEP;
            if self.remaining < 0.0 {
                self.remaining = self.time_limit;
                self.stop_deferred();
                return false;
            }
        }
        let mut s = Step {
            particles: &mut self.particles,
            max: self.max,
            effect_flags: self.flags,
            frame_count: self.frames.count,
            random,
            notices: self.notices.as_mut(),
            dt: STEP,
        };
        for a in &mut self.actions {
            run(&a.placed, a.flags, &mut a.time, &mut s);
        }
        // The frame animation (particle-drawing.md 3.3).
        let animated = effect_flags::FRAMES | effect_flags::ANIMATED;
        if self.flags & animated == animated {
            super::actions::animate_frames(
                &mut self.particles,
                self.frames.rate,
                self.frames.count,
                STEP,
            );
        }
        // TODO(#152): collision (P5) when it has 0x10000.
        let count = self.particles.len();
        if count > 0 {
            let mut b = Bounds::EMPTY;
            let mut grow = 0.0f32;
            for p in &self.particles {
                for i in 0..3 {
                    b.min[i] = b.min[i].min(p.position[i]);
                    b.max[i] = b.max[i].max(p.position[i]);
                }
                grow = grow.max(p.size[0]).max(p.size[1]).max(p.size[2]);
            }
            for i in 0..3 {
                b.min[i] -= grow;
                b.max[i] += grow;
            }
            self.bounds = b;
            self.sphere = b.sphere();
        }
        if self.stopping && count == 0 {
            self.playing = false;
            self.stopping = false;
            return false;
        }
        true
    }
}

/// The steps a time-limited effect with limit `t` runs before its time
/// limit stops it in step *k* (P2 8.3: *k* − 1), and *k*.
pub fn time_limit_step(t: f32) -> u64 {
    let mut remaining = t;
    let mut k = 0;
    loop {
        k += 1;
        remaining -= STEP;
        if remaining < 0.0 {
            return k;
        }
    }
}

/// An action's fields placed by `m` and the effect's velocity `w`
/// (P2 5.2).
fn place_action(stored: &ActionKind, m: &Matrix, w: Vec3) -> ActionKind {
    let mut a = stored.clone();
    let turned = without_translation(m);
    match &mut a {
        ActionKind::Bounce { surface, .. } => *surface = place(surface, m),
        ActionKind::Jet {
            centre,
            acceleration,
            ..
        } => {
            *centre = place_point(*centre, m);
            *acceleration = place(acceleration, &turned);
        }
        ActionKind::OrbitPoint { centre, .. } | ActionKind::Scatter { centre, .. } => {
            *centre = place_point(*centre, m);
        }
        ActionKind::RandomAccel { region }
        | ActionKind::RandomDisplace { region }
        | ActionKind::RandomVelocity { region } => *region = place(region, &turned),
        ActionKind::Sink { region, .. } => *region = place(region, m),
        ActionKind::Source(source) => {
            source.position = place(&source.position, m);
            source.velocity = place(&source.velocity, &turned);
            source.parent_velocity = scale(w, source.parent_motion);
        }
        ActionKind::TargetVelocity { velocity, .. } => {
            *velocity = place_direction(*velocity, m);
        }
        ActionKind::Vortex { centre, axis, .. } => {
            *centre = place_point(*centre, m);
            *axis = place_direction(*axis, m);
        }
        _ => {}
    }
    a
}

#[cfg(test)]
mod tests;
