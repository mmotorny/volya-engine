// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The weather's ambient sounds (docs/behavior/sound-weather-ambient.md):
//! ambient sets and effects read from the configuration (section 2) and
//! the per-frame rule choosing the set, starting random sounds around the
//! listener and starting and ending effects (section 4).
//!
//! An effect's particles and wind are P11's
//! (docs/behavior/particles-and-lights-scope.md); here the active effect is
//! kept as the data P11 will need (its particle effect, where it started,
//! whether it was told to stop) and the wind gust factor in force.
//!
//! Section numbers below are the spec's.

use std::io;

use super::Fatal;
use super::random::{MAX, range};
use super::system::{PlayFlags, SoundId, SoundSystem, Vec3};
use crate::audio::Category;
use crate::config::{Config, PathAliases, value};
use crate::vfs::Vfs;

/// The angle factor 2π / 32,767 as the 32-bit float the program stores
/// (3.2).
const ANGLE: f32 = 0.000_191_753_45;
/// The height of a random sound above the listener (4.5).
const HEIGHT: f32 = 10.0;
/// The view is outdoors from this sky visibility on (4.3).
pub const OUTDOORS: f32 = 0.05;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The items of a list value (2.1): `None` (no value) or empty gives none;
/// otherwise a `,` directly followed by another ends the count, a trailing
/// `,` adds no item, and items are trimmed of bytes 0x00 – 0x20.
pub fn list_items(value: Option<&str>) -> Vec<&str> {
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return Vec::new();
    };
    let bytes = value.as_bytes();
    let mut count = 0;
    let mut last = None;
    let mut from = 0;
    while let Some(i) = bytes[from..].iter().position(|&b| b == b',') {
        let i = from + i;
        count += 1;
        last = Some(i);
        if bytes.get(i + 1) == Some(&b',') {
            break;
        }
        from = i + 1;
    }
    let rest = last.map_or(0, |i| i + 1);
    if last.is_none() || rest < bytes.len() {
        count += 1;
    }
    value
        .split(',')
        .take(count)
        .map(|s| s.trim_matches(|c: char| c <= ' '))
        .collect()
}

/// Seconds to milliseconds, rounded down (2.2, 2.3).
fn ms(seconds: f32) -> u32 {
    (seconds * 1000.0).floor() as i64 as u32
}

/// An ambient effect section's values (2.3).
#[derive(Clone, Debug, PartialEq)]
pub struct EffectData {
    pub section: String,
    /// *T*_life (ms).
    pub life: u32,
    pub particles: String,
    pub offset: Vec3,
    pub wind_gust_factor: f32,
    pub sound: Option<String>,
}

impl EffectData {
    pub fn read(config: &Config, name: &str) -> io::Result<Self> {
        let section = config
            .section(name)
            .ok_or_else(|| invalid(format!("no ambient effect section [{name}]")))?;
        // TODO(docs/behavior/sound-weather-ambient.md section 2.3): what a
        // missing `life_time`, `particles`, `offset` or `wind_gust_factor`
        // does is unestablished; we read it as 0 or empty (every shipped
        // section has them).
        let get = |key: &str| section.get(key).flatten().unwrap_or("");
        Ok(Self {
            section: name.to_owned(),
            life: ms(value::float(get("life_time"))),
            particles: get("particles").to_owned(),
            offset: value::floats::<3>(get("offset")),
            wind_gust_factor: value::float(get("wind_gust_factor")),
            sound: section
                .get("sound")
                .flatten()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        })
    }
}

/// An ambient set's values (2.2).
#[derive(Clone, Debug, PartialEq)]
pub struct SetData {
    pub section: String,
    /// The random sounds, duplicates kept.
    pub sounds: Vec<String>,
    /// *P*₀, *P*₁ (ms); read only with sounds.
    pub sound_period: Option<(u32, u32)>,
    /// *d*₀ ≤ *d*₁ (m); read only with sounds.
    pub sound_dist: Option<(f32, f32)>,
    pub effects: Vec<EffectData>,
    /// *Q*₀, *Q*₁ (ms); read only with effects.
    pub effect_period: Option<(u32, u32)>,
}

impl SetData {
    /// Reads the set `name` of `config` (2.2): its effects each read as
    /// listed.
    pub fn read(config: &Config, name: &str) -> io::Result<Self> {
        let section = config
            .section(name)
            .ok_or_else(|| invalid(format!("no ambient set [{name}]")))?;
        let get = |key: &str| section.get(key).flatten();
        let pair = |key: &str| value::floats::<2>(get(key).unwrap_or(""));
        let sounds: Vec<String> = match section.get("sounds") {
            Some(v) => list_items(v).into_iter().map(str::to_owned).collect(),
            None => Vec::new(),
        };
        let has_sounds = section.contains("sounds");
        let (sound_period, sound_dist) = if has_sounds {
            let [p0, p1] = pair("sound_period");
            let [d0, d1] = pair("sound_dist");
            (
                Some((ms(p0), ms(p1))),
                Some(if d0 > d1 { (d1, d0) } else { (d0, d1) }),
            )
        } else {
            (None, None)
        };
        let effect_names = list_items(get("effects"));
        let effect_period = (!effect_names.is_empty()).then(|| {
            let [q0, q1] = pair("effect_period");
            (ms(q0), ms(q1))
        });
        let effects = effect_names
            .iter()
            .map(|e| EffectData::read(config, e))
            .collect::<io::Result<_>>()?;
        Ok(Self {
            section: section.name().to_owned(),
            sounds,
            sound_period,
            sound_dist,
            effects,
            effect_period,
        })
    }
}

/// A set with its sounds created (2.2, 2.3: effects category, the file's
/// AI sound type).
#[derive(Clone, Debug)]
pub struct AmbientSet {
    pub data: SetData,
    pub sounds: Vec<SoundId>,
    /// Each effect's sound, if it has one.
    pub effect_sounds: Vec<Option<SoundId>>,
}

impl AmbientSet {
    pub fn create(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        data: SetData,
    ) -> Result<Self, Fatal> {
        let mut create = |name: &str| system.create(tree, aliases, name, Category::Effects);
        let sounds = data
            .sounds
            .iter()
            .map(|n| create(n))
            .collect::<Result<_, _>>()?;
        let effect_sounds = data
            .effects
            .iter()
            .map(|e| e.sound.as_deref().map(&mut create).transpose())
            .collect::<Result<_, _>>()?;
        Ok(Self {
            data,
            sounds,
            effect_sounds,
        })
    }

    pub fn discard(&self, system: &mut SoundSystem) {
        for &s in self
            .sounds
            .iter()
            .chain(self.effect_sounds.iter().flatten())
        {
            system.discard(s);
        }
    }
}

/// The active effect's particles (4.2), as P11 will need them.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectParticles {
    pub particles: String,
    pub position: Vec3,
    /// Told to stop (4.6, End).
    pub stopping: bool,
}

/// What a frame did, for tests and diagnostics.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmbientFrame {
    /// Whether B's set was taken (else A's).
    pub chose_b: bool,
    /// The random sound started: the set's item index and its position.
    pub sound: Option<(usize, Vec3)>,
    /// The effect started: the set's item index.
    pub effect: Option<usize>,
    /// The active effect ended this frame.
    pub effect_ended: bool,
}

/// The state kept for the whole run (4.2): not reset when a level is
/// loaded or left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmbientState {
    /// *N*_s, *N*_e, *N*_end (ms of the global clock).
    pub next_sound: u32,
    pub next_effect: u32,
    pub effect_end: u32,
    pub particles: Option<EffectParticles>,
    /// The wind gust factor in force (P11).
    pub wind_gust_factor: f32,
}

/// A period from bounds `a`, `b` (3.2). With equal bounds the original
/// faults (believed, 4.7); no shipped set has them, and we take *a* (our
/// choice).
fn period(system: &mut SoundSystem, (a, b): (u32, u32)) -> u32 {
    let r = system.random().draw();
    range(r, a, b).unwrap_or(a)
}

impl AmbientState {
    /// The level was left (4.2): the active effect's particles are removed
    /// at once (every sound stops by S2's stop all).
    pub fn level_left(&mut self) {
        self.particles = None;
    }

    /// The rule of one frame (4.4 – 4.6) at global clock `now` (ms): the
    /// current sections' sets `a` and `b` with blend weight `t`, whether
    /// the view is `outdoors` (4.3), and the camera's eye `eye`. Run once
    /// per frame while a level runs and the game is not paused (4.1).
    pub fn frame(
        &mut self,
        system: &mut SoundSystem,
        now: u32,
        (a, b, t): (Option<&AmbientSet>, Option<&AmbientSet>, f32),
        outdoors: bool,
        eye: Vec3,
    ) -> AmbientFrame {
        let mut out = AmbientFrame::default();
        // 4.4: both sides in 32-bit float.
        let r = system.random().draw();
        out.chose_b = 1.0 - t <= r as f32 * (1.0 / MAX as f32);
        let set = if out.chose_b { b } else { a };
        if let Some(set) = set {
            self.random_sound(system, now, set, eye, &mut out);
            self.start_effect(system, now, set, outdoors, eye, &mut out);
        }
        // End (4.6), whether or not the frame has a set.
        if !(outdoors && now < self.effect_end) {
            if let Some(p) = &mut self.particles
                && !p.stopping
            {
                p.stopping = true;
                out.effect_ended = true;
            }
            self.wind_gust_factor = 0.0;
        }
        // Removal: stopped particles that no longer play are removed at the
        // end of the frame.
        // TODO(#320, P11, docs/behavior/particles-and-lights-scope.md): when stopped
        // particles count as no longer playing is P11's; until particles
        // exist they are gone in the frame they are told to stop.
        if self.particles.as_ref().is_some_and(|p| p.stopping) {
            self.particles = None;
        }
        out
    }

    /// 4.5.
    fn random_sound(
        &mut self,
        system: &mut SoundSystem,
        now: u32,
        set: &AmbientSet,
        eye: Vec3,
        out: &mut AmbientFrame,
    ) {
        if self.next_sound >= now {
            return;
        }
        let n = set.sounds.len();
        let item = (n > 0).then(|| system.random().draw() as usize % n);
        // TODO(docs/behavior/sound-weather-ambient.md section 4.7): a set
        // without `sounds` draws its period from bounds never read, which
        // is unestablished; we draw none and leave N_s (no shipped set).
        let Some(bounds) = set.data.sound_period else {
            return;
        };
        self.next_sound = now.wrapping_add(period(system, bounds));
        let Some(item) = item else {
            return;
        };
        let angle = system.random().draw() as f32 * ANGLE;
        let (d0, d1) = set.data.sound_dist.unwrap_or((0.0, 0.0));
        let d = d0 + system.random().draw() as f32 * (d1 - d0) / MAX as f32;
        let position = [
            eye[0] + d * angle.cos(),
            eye[1] + HEIGHT,
            eye[2] + d * angle.sin(),
        ];
        system.play_at(set.sounds[item], position, PlayFlags::default());
        out.sound = Some((item, position));
    }

    /// 4.6, Start.
    fn start_effect(
        &mut self,
        system: &mut SoundSystem,
        now: u32,
        set: &AmbientSet,
        outdoors: bool,
        eye: Vec3,
        out: &mut AmbientFrame,
    ) {
        let n = set.data.effects.len();
        if !outdoors || self.particles.is_some() || self.next_effect >= now || n == 0 {
            return;
        }
        let item = system.random().draw() as usize % n;
        let effect = &set.data.effects[item];
        self.wind_gust_factor = effect.wind_gust_factor;
        let bounds = set.data.effect_period.unwrap_or((0, 0));
        self.next_effect = now.wrapping_add(period(system, bounds));
        self.effect_end = now.wrapping_add(effect.life);
        let o = effect.offset;
        let position = [eye[0] + o[0], eye[1] + o[1], eye[2] + o[2]];
        self.particles = Some(EffectParticles {
            particles: effect.particles.clone(),
            position,
            stopping: false,
        });
        if let Some(sound) = set.effect_sounds[item] {
            system.play_at(sound, position, PlayFlags::default());
        }
        out.effect = Some(item);
    }
}

#[cfg(test)]
mod tests;
