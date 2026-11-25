// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A group instance (particle-groups.md): one member effect per member,
//! their per-particle and free children, the group time and its limit.

use volya_engine_formats::particles::{Library, Member, member_flags as flags};

use super::effect::Notice;
use super::{Bounds, EffectInstance, IDENTITY, Matrix, Mode, Random, Sphere, Vec3};

/// The message of a name that is not an effect of the library (P6 2.1).
pub const NO_EFFECT: &str = "Particle effect doesn't exist";
/// The fatal error of a looped birth or death child (P6 6.4).
pub const LOOPED_CHILD: &str = "Can't use looped effect '%s' as 'On Birth' child for group.";

/// 1 / 0.033 in `f32` (P6 6.3).
const PER_STEP: f32 = 1.0 / 0.033;

/// A member with its effect and children.
#[derive(Clone, Debug)]
struct MemberInstance {
    member: Member,
    effect: EffectInstance,
    per_particle: Vec<EffectInstance>,
    free: Vec<EffectInstance>,
}

/// A group instance.
#[derive(Clone, Debug)]
pub struct GroupInstance {
    record: usize,
    members: Vec<MemberInstance>,
    playing: bool,
    stopping: bool,
    /// The group time *t*, seconds.
    time: f32,
    time_limit: f32,
    position: Vec3,
    bounds: Bounds,
    sphere: Sphere,
    /// The fatal error the game ends at (P6 2.3): the owner ends the game
    /// with it; the group does nothing more.
    fatal: Option<String>,
}

impl GroupInstance {
    /// Makes the group at `record` of `library` (P6 2.1): a member effect
    /// for every member, whatever its flags. A member naming no effect of
    /// the library is the game's assertion failure: an error here.
    pub fn new(library: &Library, record: usize) -> Result<Self, String> {
        let group = &library.groups[record];
        let members = group
            .members
            .iter()
            .map(|m| {
                let mut effect = make(library, &m.effect)?;
                effect.notices = Some(Vec::new());
                Ok(MemberInstance {
                    member: m.clone(),
                    effect,
                    per_particle: Vec::new(),
                    free: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            record,
            members,
            playing: false,
            stopping: false,
            time: 0.0,
            time_limit: group.time_limit,
            position: [0.0; 3],
            bounds: Bounds::around([0.0; 3]),
            sphere: Bounds::around([0.0; 3]).sphere(),
            fatal: None,
        })
    }

    /// The group's index in the library.
    pub fn record(&self) -> usize {
        self.record
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn is_stopping(&self) -> bool {
        self.stopping
    }

    /// The group time, seconds.
    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub fn sphere(&self) -> Sphere {
        self.sphere
    }

    /// The fatal error met (P6 2.3), which ends the game.
    pub fn fatal(&self) -> Option<&str> {
        self.fatal.as_deref()
    }

    /// Member `i`'s effect.
    pub fn member_effect(&self, i: usize) -> &EffectInstance {
        &self.members[i].effect
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Every effect instance of the group: member effects, then each
    /// member's per-particle and free children.
    pub fn effects(&self) -> impl Iterator<Item = &EffectInstance> {
        self.members.iter().flat_map(|m| {
            std::iter::once(&m.effect)
                .chain(m.per_particle.iter())
                .chain(m.free.iter())
        })
    }

    /// Placement (P6 3): every member effect with the same request.
    pub fn place(&mut self, m: &Matrix, w: Vec3, mode: Mode) {
        self.position = [m[3][0], m[3][1], m[3][2]];
        for member in &mut self.members {
            member.effect.place(m, w, mode);
        }
    }

    /// Play (P6 3): the member effects start in the update.
    pub fn play(&mut self) {
        self.time = 0.0;
        self.stopping = false;
        self.playing = true;
    }

    pub fn stop_deferred(&mut self) {
        self.stopping = true;
        for m in &mut self.members {
            m.stop(false);
        }
    }

    pub fn stop_immediate(&mut self) {
        self.playing = false;
        for m in &mut self.members {
            m.stop(true);
        }
    }

    /// The update with `e` ms (P6 5).
    pub fn update(&mut self, e: u32, library: &Library, random: &mut Random) {
        if self.fatal.is_some() {
            return;
        }
        if !self.playing {
            self.bounds = Bounds::around(self.position);
            self.sphere = self.bounds.sphere();
            return;
        }
        let dt = (f64::from(e) * f64::from(0.001f32)) as f32;
        let (t, end) = (self.time, self.time + dt);
        for m in &mut self.members {
            if m.member.flags & flags::USED == 0 {
                continue;
            }
            if m.effect.is_playing() {
                if t <= m.member.stop_time && m.member.stop_time <= end {
                    m.stop(m.member.flags & flags::DEFERRED_STOP == 0);
                }
            } else if !self.stopping && t <= m.member.start_time && m.member.start_time <= end {
                m.effect.play();
            }
        }
        self.time = end;
        if self.time > self.time_limit && self.time_limit > 0.0 && !self.stopping {
            self.stop_deferred();
        }
        let mut collected = Bounds::EMPTY;
        let mut playing = false;
        for m in &mut self.members {
            match m.update(e, library, random, &mut collected) {
                Ok(p) => playing |= p,
                Err(fatal) => {
                    self.fatal = Some(fatal);
                    return;
                }
            }
        }
        if self.stopping && !playing {
            self.playing = false;
            self.stopping = false;
        }
        if collected.is_set() {
            self.bounds = collected;
            self.sphere = collected.sphere();
        }
    }
}

// TODO(#319): what destroying a group and giving an instance a record
// again do is not specified; dropping the instance stops and drops every
// member effect and child.

/// A new effect instance of the effect named `name` (P6 2.1, 6.3).
fn make(library: &Library, name: &str) -> Result<EffectInstance, String> {
    let i = library.effect(name).ok_or_else(|| NO_EFFECT.to_owned())?;
    Ok(EffectInstance::new(i, &library.effects[i]))
}

/// The placement of a child at a particle (P6 6.3): velocity and matrix.
fn at_particle(position: Vec3, previous: Vec3, local: Option<&Matrix>) -> (Matrix, Vec3) {
    let v = [0, 1, 2].map(|i| (position[i] - previous[i]) * PER_STEP);
    match local {
        None => {
            let mut m = IDENTITY;
            m[3] = [position[0], position[1], position[2], 1.0];
            (m, v)
        }
        Some(k) => {
            let v = super::place_direction(v, k);
            let x = super::place_point(position, k);
            let mut m = *k;
            m[3][..3].copy_from_slice(&x);
            (m, v)
        }
    }
}

impl MemberInstance {
    /// Stops the member effect and every child (P6 5.4).
    fn stop(&mut self, immediate: bool) {
        let all = std::iter::once(&mut self.effect)
            .chain(self.per_particle.iter_mut())
            .chain(self.free.iter_mut());
        for e in all {
            if immediate {
                e.stop_immediate();
            } else {
                e.stop_deferred();
            }
        }
        if immediate {
            self.per_particle.clear();
            self.free.clear();
        }
    }

    /// A child at a particle, played and placed (P6 6.3); a birth or death
    /// child is first tested for looping (6.4).
    fn child(
        &self,
        library: &Library,
        name: &str,
        position: Vec3,
        previous: Vec3,
        test_looped: bool,
    ) -> Result<EffectInstance, String> {
        let mut child = make(library, name)?;
        if test_looped && child.looped() {
            return Err(LOOPED_CHILD.replace("%s", name));
        }
        let (m, v) = at_particle(position, previous, self.effect.local());
        child.play();
        child.place(&m, v, Mode::World);
        Ok(child)
    }

    /// The member effect's births and deaths (P6 6.1, 6.2), in the order
    /// they happened.
    fn notices(&mut self, library: &Library) -> Result<(), String> {
        let Some(notices) = self.effect.notices.as_mut().map(std::mem::take) else {
            return Ok(());
        };
        let f = self.member.flags;
        for n in &notices {
            match *n {
                Notice::Birth { position, .. } => {
                    if f & flags::BIRTH_CHILD != 0 {
                        let name = self.member.birth_child.clone();
                        let c = self.child(library, &name, position, position, true)?;
                        self.free.push(c);
                    }
                    if f & flags::PER_PARTICLE_CHILD != 0 {
                        let name = self.member.per_particle_child.clone();
                        let c = self.child(library, &name, position, position, false)?;
                        self.per_particle.push(c);
                    }
                }
                Notice::Death {
                    index,
                    position,
                    previous,
                } => {
                    if f & flags::PER_PARTICLE_CHILD != 0 && index < self.per_particle.len() {
                        let mut c = self.per_particle.swap_remove(index);
                        c.stop_deferred();
                        self.free.push(c);
                    }
                    if f & flags::DEATH_CHILD != 0 {
                        let name = self.member.death_child.clone();
                        let c = self.child(library, &name, position, previous, true)?;
                        self.free.push(c);
                    }
                }
            }
        }
        // The buffer is kept for the next update.
        let mut notices = notices;
        notices.clear();
        self.effect.notices = Some(notices);
        Ok(())
    }

    /// The member's update (P6 5.3); whether anything of it is playing,
    /// or the fatal error a child met.
    fn update(
        &mut self,
        e: u32,
        library: &Library,
        random: &mut Random,
        collected: &mut Bounds,
    ) -> Result<bool, String> {
        let mut playing = false;
        self.effect.update(e, random);
        self.notices(library)?;
        if self.effect.is_playing() {
            playing = true;
            collected.merge(&self.effect.bounds());
            if self.member.flags & flags::PER_PARTICLE_CHILD != 0
                && !self.member.per_particle_child.is_empty()
            {
                for (c, p) in self.per_particle.iter_mut().zip(self.effect.particles()) {
                    let (m, v) = at_particle(p.position, p.previous, None);
                    c.place(&m, v, Mode::World);
                }
            }
        }
        for c in &mut self.per_particle {
            c.update(e, random);
            if c.is_playing() {
                playing = true;
                collected.merge(&c.bounds());
            } else if self.member.flags & flags::REPLAY_CHILD != 0 {
                c.play();
            }
        }
        self.free.retain_mut(|c| {
            c.update(e, random);
            if c.is_playing() {
                playing = true;
                collected.merge(&c.bounds());
            }
            c.is_playing()
        });
        Ok(playing)
    }
}

#[cfg(test)]
mod tests;
