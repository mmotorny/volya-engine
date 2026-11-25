// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The effects a level places through `level.ps_static`
//! (docs/behavior/level-placed-effects.md): one instance per record,
//! placed once, played and updated once at the load, then updated by the
//! engine's scheduler with a period growing with the camera's distance.
//!
//! The drawing update of section 5.3 comes with drawing (P7, #322); until
//! then the schedule alone updates them. The scheduler's per-frame budget
//! is unestablished (5.2): every due instance runs in the frame (our
//! choice).

use std::sync::Arc;

use volya_engine_formats::particles::{Library, PlacedRecord};

use super::{EffectInstance, GroupInstance, Mode, Random, STEP_MS, Sphere, Vec3, sub};

/// What a record made (section 2).
#[derive(Clone, Debug)]
pub enum Instance {
    Effect(EffectInstance),
    Group(GroupInstance),
}

impl Instance {
    fn update(&mut self, e: u32, library: &Library, random: &mut Random) {
        match self {
            Instance::Effect(i) => i.update(e, random),
            Instance::Group(g) => g.update(e, library, random),
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
    pub fn effects(&self) -> Box<dyn Iterator<Item = &EffectInstance> + '_> {
        match self {
            Instance::Effect(i) => Box::new(std::iter::once(i)),
            Instance::Group(g) => Box::new(g.effects()),
        }
    }
}

/// One record's instance with its update times.
#[derive(Clone, Debug)]
pub struct PlacedEffect {
    pub name: String,
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
    fn update(&mut self, clock: u32, library: &Library, random: &mut Random) {
        let e = clock.wrapping_sub(self.last);
        if e != 0 {
            self.instance.update(e, library, random);
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

/// The level's placed effects with the library they come from, their
/// generators and the world clock they run on.
#[derive(Clone, Debug)]
pub struct PlacedEffects {
    library: Arc<Library>,
    random: Random,
    effects: Vec<PlacedEffect>,
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
        let mut random = Random::default();
        let mut effects = Vec::with_capacity(records.len());
        for r in records {
            let mut instance = if let Some(i) = library.effect(&r.name) {
                Instance::Effect(EffectInstance::new(i, &library.effects[i]))
            } else if let Some(g) = library.group(&r.name) {
                Instance::Group(
                    GroupInstance::new(&library, g).map_err(|e| format!("{}: {e}", r.name))?,
                )
            } else {
                return Err(format!(
                    "{}: Particle effect or group doesn't exist",
                    r.name
                ));
            };
            let mut m = r.transform;
            m[3][1] += LIFT;
            match &mut instance {
                Instance::Effect(i) => {
                    i.place(&m, [0.0; 3], Mode::World);
                    i.play();
                }
                Instance::Group(g) => {
                    g.place(&m, [0.0; 3], Mode::World);
                    g.play();
                }
            }
            let mut placed = PlacedEffect {
                name: r.name.clone(),
                instance,
                last: clock.wrapping_sub(STEP_MS),
                due: clock,
                period: 0,
                registered: None,
                updates: 0,
            };
            placed.update(clock, &library, &mut random);
            placed.register();
            effects.push(placed);
        }
        Ok(Self {
            library,
            random,
            effects,
            clock,
        })
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn effects(&self) -> &[PlacedEffect] {
        &self.effects
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
    /// `camera`: every instance whose due time the clock is past runs.
    pub fn schedule(&mut self, camera: Vec3) {
        let clock = self.clock;
        for p in &mut self.effects {
            if (clock.wrapping_sub(p.due) as i32) <= 0 {
                continue;
            }
            let c = p.instance.sphere().centre;
            let d = sub(c, camera).map(f64::from);
            p.period = period((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt());
            p.update(clock, &self.library, &mut self.random);
            p.register();
            p.due = clock.wrapping_add(p.period);
        }
    }

    /// One frame: the clock moves on by `elapsed` ms, then the scheduler's
    /// pass.
    pub fn frame(&mut self, elapsed: u64, camera: Vec3) {
        self.advance(elapsed);
        self.schedule(camera);
    }

    /// The fatal error a group met (particle-groups.md 2.3): the game ends
    /// with it.
    pub fn fatal(&self) -> Option<&str> {
        self.effects.iter().find_map(|p| match &p.instance {
            Instance::Group(g) => g.fatal(),
            Instance::Effect(_) => None,
        })
    }

    /// Every effect instance of every record, for drawing.
    pub fn instances(&self) -> impl Iterator<Item = &EffectInstance> {
        self.effects.iter().flat_map(|p| p.instance.effects())
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
        let Instance::Effect(e) = &p.effects[0].instance else {
            panic!()
        };
        // P8 8: placed with y bits 0xC196956F.
        assert_eq!(e.position()[1].to_bits(), 0xC196_956F);
        assert!(e.is_playing());
        // The start update ran one step: the source's first births.
        assert_eq!(p.effects[0].updates, 1);
        assert!(p.effects[0].registered.is_some());
    }

    #[test]
    fn the_schedule_runs_past_due_and_stands_still_with_the_clock() {
        let mut p = level();
        let camera = p.effects[0].instance.sphere().centre;
        // Due at the load clock: not run until the clock is past it.
        p.schedule(camera);
        assert_eq!(p.effects[0].updates, 1);
        p.frame(16, camera);
        assert_eq!(p.effects[0].updates, 2);
        assert_eq!(p.effects[0].period, 30);
        // A paused frame does not move the clock: nothing runs.
        for _ in 0..10 {
            p.frame(0, camera);
        }
        assert_eq!(p.effects[0].updates, 2);
        for _ in 0..120 {
            p.frame(16, camera);
        }
        assert!(p.particle_count() > 0);
        assert!(p.effects[0].instance.is_playing());
    }

    #[test]
    fn a_far_camera_updates_less_often() {
        let mut near = level();
        let mut far = level();
        let c = near.effects[0].instance.sphere().centre;
        for _ in 0..600 {
            near.frame(16, c);
            far.frame(16, [c[0] + 500.0, c[1], c[2]]);
        }
        assert_eq!(far.effects[0].period, 525);
        assert!(far.effects[0].updates * 10 < near.effects[0].updates);
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
