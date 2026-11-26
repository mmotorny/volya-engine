// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's particle objects. The effects a level places through
//! `level.ps_static` (docs/behavior/level-placed-effects.md): one instance
//! per record, placed once, played and updated once at the load, then
//! updated by the engine's scheduler with a period growing with the
//! camera's distance. The zones' idle effects
//! (docs/behavior/zone-idle-effects.md section 4.5: made at a zone's first
//! idle start, stopped and played again with its state) and the weather's
//! ambient effects (docs/behavior/weather-ambient-effects.md section 3: made
//! by the ambient rule, removed once they no longer play) are particle
//! objects of the same kind, scheduled and drawn as P8's (P9 4.5, P11 3.3).
//!
//! The drawing update of section 5.3 is [`PlacedEffects::draw_update`],
//! which the level runs for the instances it draws. The scheduler's
//! per-frame budget
//! is unestablished (5.2): every due instance runs in the frame (our
//! choice).

use std::sync::Arc;

use volya_engine_formats::particles::{Library, PlacedRecord};

use super::random::Shared;
use super::{EffectInstance, GroupInstance, Matrix, Mode, Random, STEP_MS, Sphere, Vec3, sub};
use crate::collision::CollisionWorld;

/// What a record made (section 2).
#[derive(Clone, Debug)]
pub enum Instance {
    Effect(EffectInstance),
    Group(GroupInstance),
}

impl Instance {
    /// The instance of `name`: among the library's effects first, its
    /// groups second (P8 2, P9 4.5, P11 3.1). A name that is neither, or a
    /// group a member of which names no effect, is the game's assertion
    /// failure: an error here.
    pub fn make(library: &Library, name: &str) -> Result<Self, String> {
        if let Some(i) = library.effect(name) {
            Ok(Instance::Effect(EffectInstance::new(
                i,
                library.effect_at(i),
            )))
        } else if let Some(g) = library.group(name) {
            Ok(Instance::Group(
                GroupInstance::new(library, g).map_err(|e| format!("{name}: {e}"))?,
            ))
        } else {
            Err(format!("{name}: Particle effect or group doesn't exist"))
        }
    }

    /// Places it by `m` with velocity `w` (P2 5, P6 3).
    pub fn place(&mut self, m: &Matrix, w: Vec3, mode: Mode) {
        match self {
            Instance::Effect(i) => i.place(m, w, mode),
            Instance::Group(g) => g.place(m, w, mode),
        }
    }

    /// Play (P2 7, P6 3).
    pub fn play(&mut self) {
        match self {
            Instance::Effect(i) => i.play(),
            Instance::Group(g) => g.play(),
        }
    }

    /// Deferred stop (P2 7, P6 3).
    pub fn stop_deferred(&mut self) {
        match self {
            Instance::Effect(i) => i.stop_deferred(),
            Instance::Group(g) => g.stop_deferred(),
        }
    }

    /// Immediate stop (P2 7, P6 3).
    pub fn stop_immediate(&mut self) {
        match self {
            Instance::Effect(i) => i.stop_immediate(),
            Instance::Group(g) => g.stop_immediate(),
        }
    }

    fn update(
        &mut self,
        e: u32,
        library: &Library,
        random: &mut Random,
        world: Option<&CollisionWorld>,
    ) {
        match self {
            Instance::Effect(i) => i.update_in(e, random, world),
            Instance::Group(g) => g.update_in(e, library, random, world),
        }
    }

    pub fn sphere(&self) -> Sphere {
        match self {
            Instance::Effect(i) => i.sphere(),
            Instance::Group(g) => g.sphere(),
        }
    }

    pub fn is_playing(&self) -> bool {
        match self {
            Instance::Effect(i) => i.is_playing(),
            Instance::Group(g) => g.is_playing(),
        }
    }

    /// Every effect instance it holds.
    pub fn effects(&self) -> impl Iterator<Item = &EffectInstance> + '_ {
        // Not boxed: no allocation in a running frame.
        let (one, group) = match self {
            Instance::Effect(i) => (Some(i), None),
            Instance::Group(g) => (None, Some(g.effects())),
        };
        one.into_iter().chain(group.into_iter().flatten())
    }
}

/// The most particles the instance of `name` can hold at once: an
/// effect's maximum, or the sum over a group's members and the children
/// their member flags use (P1 6.1). `None` for a name of neither.
pub fn particle_capacity(library: &Library, name: &str) -> Option<usize> {
    use volya_engine_formats::particles::member_flags as m;
    let max = |n: &str| {
        library
            .effect(n)
            .map_or(0, |i| library.effect_at(i).max_particles as usize)
    };
    if let Some(i) = library.effect(name) {
        return Some(library.effect_at(i).max_particles as usize);
    }
    let g = library.group(name)?;
    Some(
        library
            .group_at(g)
            .members
            .iter()
            .map(|member| {
                let mut n = max(member.effect);
                for (bit, child) in [
                    (m::PER_PARTICLE_CHILD, member.per_particle_child),
                    (m::BIRTH_CHILD, member.birth_child),
                    (m::DEATH_CHILD, member.death_child),
                ] {
                    if member.flags & bit != 0 {
                        n += max(child);
                    }
                }
                n
            })
            .sum(),
    )
}

/// What made a particle object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A record of `level.ps_static` (P8).
    Placed,
    /// A zone's idle effect (P9 4.5).
    Zone,
    /// The weather's ambient effect (P11 3).
    Weather,
    /// An artefact's idle effect (P10 4).
    Artefact,
}

/// One particle object: its instance with its update times.
#[derive(Clone, Debug)]
pub struct PlacedEffect {
    pub name: String,
    pub origin: Origin,
    pub instance: Instance,
    /// The last-update time (clock, ms).
    last: u32,
    /// When the scheduler next runs it (clock, ms).
    due: u32,
    /// The latest period (ms).
    period: u32,
    /// The sphere handed to the renderer's spatial index (5.4).
    registered: Option<Sphere>,
    /// Updates it has had.
    updates: u64,
}

impl PlacedEffect {
    /// The visibility sphere as registered (section 5.4).
    pub fn registered(&self) -> Option<Sphere> {
        self.registered
    }

    pub fn period(&self) -> u32 {
        self.period
    }

    pub fn updates(&self) -> u64 {
        self.updates
    }

    /// The elapsed-time update of section 5.1 at `clock`.
    fn update(
        &mut self,
        clock: u32,
        library: &Library,
        random: &mut Random,
        world: Option<&CollisionWorld>,
    ) {
        let e = clock.wrapping_sub(self.last);
        if e != 0 {
            self.instance.update(e, library, random, world);
            self.last = clock;
            self.updates += 1;
        }
    }

    /// The visibility sphere after an update (section 5.4).
    fn register(&mut self) {
        let s = self.instance.sphere();
        let fine = |v: f32| v.is_normal() || v == 0.0;
        if !(s.centre.iter().all(|&c| fine(c)) && fine(s.radius)) {
            return;
        }
        match &mut self.registered {
            None => self.registered = Some(s),
            Some(r) => {
                let moved = (0..3)
                    .any(|i| (s.centre[i] - r.centre[i]).abs() >= f32::from_bits(0x3C23_D70B));
                if moved || (s.radius - r.radius).abs() >= f32::from_bits(0x3E19_999A) {
                    *r = s;
                }
            }
        }
    }
}

/// The 0.01 added to the transform's y (section 3).
const LIFT: f32 = f32::from_bits(0x3C23_D70A);
/// The scale per metre of the camera's distance (section 5.2).
const SCALE_PER_METRE: f32 = f32::from_bits(0x3BA3_D70A);

/// The scheduled period at `distance` metres from the camera (section
/// 5.2): *lo* = 30, *hi* = 525. The distance and *s* are formed in
/// extended precision and only (*hi* − *lo*) · *s* is rounded to `f32`
/// (the review gap of P8).
pub fn period(distance: f64) -> u32 {
    let (lo, hi) = (30i64, 525i64);
    let s = distance * f64::from(SCALE_PER_METRE);
    let product = ((hi - lo) as f64 * s) as f32;
    (lo + product.trunc() as i64).clamp(lo, hi) as u32
}

/// The level's particle objects with the library they come from, their
/// generators and the world clock they run on.
#[derive(Clone, Debug)]
pub struct PlacedEffects {
    library: Arc<Library>,
    /// The level's collision mesh the effects' collision rays are cast
    /// against (particle-collision.md); none in checks without a level.
    world: Option<Arc<CollisionWorld>>,
    random: Random,
    /// The objects; a deleted one leaves its slot to the next made.
    effects: Vec<Option<PlacedEffect>>,
    /// Objects asked to be destroyed, deleted at the next frame's start
    /// (P11 3.5).
    queued: Vec<usize>,
    /// The world clock (autosave-timing.md section 1), ms: it advances
    /// only by [`PlacedEffects::advance`], so it stands still while the
    /// game is paused or the page hidden.
    clock: u32,
}

impl PlacedEffects {
    /// Makes, places and starts every record in file order (sections 2 –
    /// 4) at world clock `clock`. A name that is neither an effect nor a
    /// group, or a group a member of which names no effect, is the game's
    /// assertion failure: an error here.
    pub fn new(
        library: Arc<Library>,
        records: &[PlacedRecord],
        clock: u32,
    ) -> Result<Self, String> {
        Self::new_in(library, records, clock, None, Shared::new(1))
    }

    /// As [`PlacedEffects::new`], the effects colliding with `world`
    /// (particle-collision.md), drawing from the shared generator `random`.
    pub fn new_in(
        library: Arc<Library>,
        records: &[PlacedRecord],
        clock: u32,
        world: Option<Arc<CollisionWorld>>,
        random: Shared,
    ) -> Result<Self, String> {
        let mut effects = Self {
            library,
            world,
            random: Random::sharing(random),
            effects: Vec::with_capacity(records.len()),
            queued: Vec::new(),
            clock,
        };
        for r in records {
            let i = effects.add(&r.name, Origin::Placed)?;
            let mut m = r.transform;
            m[3][1] += LIFT;
            effects.start(i, &m, [0.0; 3]);
        }
        Ok(effects)
    }

    /// Makes the object of `name` (P8 2, P9 4.5, P11 3.1), not yet placed
    /// or playing, and returns its index; errors as [`Instance::make`].
    pub fn add(&mut self, name: &str, origin: Origin) -> Result<usize, String> {
        let instance = Instance::make(&self.library, name)?;
        let object = PlacedEffect {
            name: name.to_owned(),
            origin,
            instance,
            last: self.clock,
            due: self.clock,
            period: 0,
            registered: None,
            updates: 0,
        };
        Ok(match self.effects.iter().position(Option::is_none) {
            Some(i) => {
                self.effects[i] = Some(object);
                i
            }
            None => {
                self.effects.push(Some(object));
                self.effects.len() - 1
            }
        })
    }

    /// Places object `i` by `m` with velocity `w` in world mode, plays it,
    /// updates it at once with 33 ms (its last-update time the clock minus
    /// 33) and registers its visibility sphere (P8 3, 4, 5.1, 5.4; P9 4.1,
    /// 4.5; P11 3.2).
    pub fn start(&mut self, i: usize, m: &Matrix, w: Vec3) {
        let clock = self.clock;
        let Some(Some(p)) = self.effects.get_mut(i) else {
            return;
        };
        p.instance.place(m, w, Mode::World);
        p.instance.play();
        p.last = clock.wrapping_sub(STEP_MS);
        p.update(
            clock,
            &self.library,
            &mut self.random,
            self.world.as_deref(),
        );
        p.register();
    }

    /// Places object `i` by `m` with velocity `w` in world mode and
    /// registers its visibility sphere (P10 4.2 step 5, 4.4 step 1).
    pub fn place_registered(&mut self, i: usize, m: &Matrix, w: Vec3) {
        if let Some(Some(p)) = self.effects.get_mut(i) {
            p.instance.place(m, w, Mode::World);
            p.register();
        }
    }

    /// Plays object `i` with its immediate 33 ms update, its last-update
    /// time the clock minus 33, and registers its visibility sphere (P9
    /// 4.5, P8 5.4).
    pub fn play(&mut self, i: usize) {
        let clock = self.clock;
        let Some(Some(p)) = self.effects.get_mut(i) else {
            return;
        };
        p.instance.play();
        p.last = clock.wrapping_sub(STEP_MS);
        p.update(
            clock,
            &self.library,
            &mut self.random,
            self.world.as_deref(),
        );
        p.register();
    }

    /// Places object `i` again (the moving zone, P9 5).
    pub fn place(&mut self, i: usize, m: &Matrix, w: Vec3) {
        if let Some(Some(p)) = self.effects.get_mut(i) {
            p.instance.place(m, w, Mode::World);
        }
    }

    /// Stops object `i` immediately (P9 4.2).
    pub fn stop_immediate(&mut self, i: usize) {
        if let Some(Some(p)) = self.effects.get_mut(i) {
            p.instance.stop_immediate();
        }
    }

    /// Stops object `i` with a deferred stop (P11 3.4).
    pub fn stop_deferred(&mut self, i: usize) {
        if let Some(Some(p)) = self.effects.get_mut(i) {
            p.instance.stop_deferred();
        }
    }

    /// Asks object `i` to be destroyed (P11 3.5): the scheduler no longer
    /// updates it, and it is deleted at the start of the next frame.
    pub fn remove(&mut self, i: usize) {
        if !self.queued.contains(&i) {
            self.queued.push(i);
        }
    }

    /// Deletes the objects asked to be destroyed (P11 3.5).
    pub fn delete_queued(&mut self) {
        for i in std::mem::take(&mut self.queued) {
            if let Some(slot) = self.effects.get_mut(i) {
                *slot = None;
            }
        }
    }

    /// The level's generators (P2 2.1), which the zones' lights draw from
    /// too (P9 4.4).
    pub fn random_mut(&mut self) -> &mut Random {
        &mut self.random
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    /// The library, shared.
    pub fn shared_library(&self) -> Arc<Library> {
        self.library.clone()
    }

    /// Object `i`.
    pub fn get(&self, i: usize) -> Option<&PlacedEffect> {
        self.effects.get(i).and_then(Option::as_ref)
    }

    /// Every object.
    pub fn effects(&self) -> impl Iterator<Item = &PlacedEffect> {
        self.effects.iter().flatten()
    }

    /// Every object with its index.
    pub fn indexed(&self) -> impl Iterator<Item = (usize, &PlacedEffect)> {
        (self.effects.iter().enumerate()).filter_map(|(i, p)| p.as_ref().map(|p| (i, p)))
    }

    /// The world clock, ms.
    pub fn clock(&self) -> u32 {
        self.clock
    }

    /// The world clock moves on by `ms`.
    pub fn advance(&mut self, ms: u64) {
        self.clock = self.clock.wrapping_add(ms as u32);
    }

    /// The scheduler's pass of a frame (section 5.2) with the camera at
    /// `camera`: every instance whose due time the clock is past runs, but
    /// those asked to be destroyed.
    pub fn schedule(&mut self, camera: Vec3) {
        let clock = self.clock;
        for (i, p) in self.effects.iter_mut().enumerate() {
            let Some(p) = p else { continue };
            if (clock.wrapping_sub(p.due) as i32) <= 0 || self.queued.contains(&i) {
                continue;
            }
            let c = p.instance.sphere().centre;
            let d = sub(c, camera).map(f64::from);
            p.period = period((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt());
            p.update(
                clock,
                &self.library,
                &mut self.random,
                self.world.as_deref(),
            );
            p.register();
            p.due = clock.wrapping_add(p.period);
        }
    }

    /// The drawing update of object `i` (section 5.3): advanced to the
    /// clock before it is drawn; the visibility sphere is left as it was
    /// (5.1, as disputed in review).
    pub fn draw_update(&mut self, i: usize) {
        let clock = self.clock;
        if let Some(Some(p)) = self.effects.get_mut(i) {
            p.update(
                clock,
                &self.library,
                &mut self.random,
                self.world.as_deref(),
            );
        }
    }

    /// One frame: the clock moves on by `elapsed` ms, the objects asked to
    /// be destroyed are deleted (P11 3.6), then the scheduler's pass.
    pub fn frame(&mut self, elapsed: u64, camera: Vec3) {
        self.advance(elapsed);
        self.delete_queued();
        self.schedule(camera);
    }

    /// The fatal error a group met (particle-groups.md 2.3): the game ends
    /// with it.
    pub fn fatal(&self) -> Option<&str> {
        self.effects().find_map(|p| match &p.instance {
            Instance::Group(g) => g.fatal(),
            Instance::Effect(_) => None,
        })
    }

    /// Every effect instance of every object, for drawing.
    pub fn instances(&self) -> impl Iterator<Item = &EffectInstance> {
        self.effects().flat_map(|p| p.instance.effects())
    }

    /// A copy holding only the objects of `origin` (tests and benches).
    pub fn copy_of(&self, origin: Origin) -> Self {
        Self {
            library: self.library.clone(),
            world: self.world.clone(),
            random: self.random.detached(),
            effects: (self.effects())
                .filter(|p| p.origin == origin)
                .cloned()
                .map(Some)
                .collect(),
            queued: Vec::new(),
            clock: self.clock,
        }
    }

    /// Live particles of every instance.
    pub fn particle_count(&self) -> usize {
        self.instances().map(|i| i.particles().len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture::muhi;
    use super::*;

    #[test]
    fn periods_grow_with_distance() {
        // P8 8.
        let got: Vec<u32> = [0.0, 10.0, 50.0, 100.0, 150.0, 200.0, 1000.0]
            .into_iter()
            .map(period)
            .collect();
        assert_eq!(got, [30, 54, 153, 277, 401, 525, 525]);
    }

    /// Steps per second of an effect updated only by the schedule at a
    /// fixed `period`, over `seconds` (P8 8).
    fn rate(period: u32, seconds: u32) -> f64 {
        let mut accumulator = 0u32;
        let mut steps = 0u64;
        let updates = seconds * 1000 / period;
        for _ in 0..updates {
            accumulator += period;
            let n = accumulator / 33;
            accumulator %= 33;
            steps += u64::from(n.min(3));
        }
        steps as f64 / f64::from(seconds)
    }

    #[test]
    fn scheduled_steps_per_second_are_p8s() {
        assert!((rate(30, 10) - 30.2).abs() < 0.05, "{}", rate(30, 10));
        // Disputed in review: the long-run rate at 30 ms is 30.3.
        assert!((rate(30, 10_000) - 30.3).abs() < 0.01);
        assert!((rate(99, 1000) - 30.3).abs() < 0.01);
        assert!((rate(153, 1000) - 19.6).abs() < 0.01);
        assert!((rate(525, 1000) - 5.71).abs() < 0.01);
    }

    fn level() -> PlacedEffects {
        let library = Arc::new(Library::new(vec![muhi()], Vec::new()));
        let mut transform = super::super::IDENTITY;
        transform[3] = [
            f32::from_bits(0xC37F_B655),
            f32::from_bits(0xC196_A9EA),
            f32::from_bits(0xC2FE_7DA2),
            1.0,
        ];
        let records = [PlacedRecord {
            name: "tests\\muhi".into(),
            transform,
        }];
        PlacedEffects::new(library, &records, 1000).unwrap()
    }

    #[test]
    fn the_first_levels_effect_is_lifted_placed_and_started() {
        let p = level();
        let Instance::Effect(e) = &p.get(0).unwrap().instance else {
            panic!()
        };
        // P8 8: placed with y bits 0xC196956F.
        assert_eq!(e.position()[1].to_bits(), 0xC196_956F);
        assert!(e.is_playing());
        // The start update ran one step: the source's first births.
        assert_eq!(p.get(0).unwrap().updates, 1);
        assert!(p.get(0).unwrap().registered.is_some());
    }

    #[test]
    fn the_schedule_runs_past_due_and_stands_still_with_the_clock() {
        let mut p = level();
        let camera = p.get(0).unwrap().instance.sphere().centre;
        // Due at the load clock: not run until the clock is past it.
        p.schedule(camera);
        assert_eq!(p.get(0).unwrap().updates, 1);
        p.frame(16, camera);
        assert_eq!(p.get(0).unwrap().updates, 2);
        assert_eq!(p.get(0).unwrap().period, 30);
        // A paused frame does not move the clock: nothing runs.
        for _ in 0..10 {
            p.frame(0, camera);
        }
        assert_eq!(p.get(0).unwrap().updates, 2);
        for _ in 0..120 {
            p.frame(16, camera);
        }
        assert!(p.particle_count() > 0);
        assert!(p.get(0).unwrap().instance.is_playing());
    }

    #[test]
    fn a_far_camera_updates_less_often() {
        let mut near = level();
        let mut far = level();
        let c = near.get(0).unwrap().instance.sphere().centre;
        for _ in 0..600 {
            near.frame(16, c);
            far.frame(16, [c[0] + 500.0, c[1], c[2]]);
        }
        assert_eq!(far.get(0).unwrap().period, 525);
        assert!(far.get(0).unwrap().updates * 10 < near.get(0).unwrap().updates);
    }

    #[test]
    fn an_unknown_name_is_an_error() {
        let library = Arc::new(Library::new(vec![muhi()], Vec::new()));
        let records = [PlacedRecord {
            name: "nope".into(),
            transform: super::super::IDENTITY,
        }];
        assert!(PlacedEffects::new(library, &records, 0).is_err());
    }
}
