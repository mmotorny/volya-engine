// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The rain sound and the thunder sounds of the weather
//! (docs/behavior/sound-rain-thunder.md): the thunderbolt collections and
//! `[thunderbolt_common]` (section 2.2), the rain rule (section 4) and the
//! thunder rule (section 5). The bolts' pictures and light are P21's
//! (docs/behavior/particles-and-lights-scope.md); a bolt here is only what
//! its sound needs, its three picture draws taken so the sequence of draws
//! is the spec's (section 6).
//!
//! Section numbers below are the spec's.

use std::io;

use super::Fatal;
use super::random::{MAX, SharedRandom};
use super::system::{PlayFlags, Settings, SoundId, SoundSystem, Vec3};
use crate::audio::Category;
use crate::config::{Config, PathAliases, value};
use crate::vfs::Vfs;

/// The rain sound's name (4.1).
pub const RAIN: &str = "ambient\\rain";
/// Rain plays from this blended density on (4.2).
const RAIN_DENSITY: f32 = 0.001;
/// The rain emitter's range and height above the camera (4.2).
const RAIN_MIN: f32 = 40.0;
const RAIN_MAX: f32 = 80.0;
const RAIN_HEIGHT: f32 = 40.0;
/// The speed of sound for the thunder's delay, m/s (5.5).
const SOUND_SPEED: f32 = 300.0;
/// The cap of `min_dist_factor` and the far end of a bolt's distance (2.2,
/// 5.3).
const FAR: f32 = 0.95;
/// The longest smoothed frame time, seconds (3.1).
const MAX_FRAME: f32 = 0.1;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A source of draws *r*, 0 … 32,767: the shared generator, or a script in
/// tests.
pub trait Draws {
    fn draw(&mut self) -> u32;

    /// U(*a*, *b*) in 32-bit float.
    fn uniform(&mut self, a: f32, b: f32) -> f32 {
        a + self.draw() as f32 * (b - a) / MAX as f32
    }
}

impl Draws for SharedRandom {
    fn draw(&mut self) -> u32 {
        SharedRandom::draw(self)
    }
}

/// The values of `[thunderbolt_common]` that sound uses (2.2).
#[derive(Clone, Debug, PartialEq)]
pub struct BoltCommon {
    /// *a*₀, *a*₁ (degrees).
    pub altitude: (f32, f32),
    /// Δ (degrees).
    pub delta_longitude: f32,
    /// *f*, capped at 0.95.
    pub min_dist_factor: f32,
    /// *q*, clamped to 0 … 1.
    pub second_probability: f32,
}

impl BoltCommon {
    /// `[thunderbolt_common]` of `config` (`config\weathers\thunderbolt.ltx`,
    /// included by `system.ltx`).
    pub fn read(config: &Config) -> io::Result<Self> {
        let section = config
            .section("thunderbolt_common")
            .ok_or_else(|| invalid("no section [thunderbolt_common]"))?;
        let get = |key: &str| section.get(key).flatten().unwrap_or("");
        let [a0, a1] = value::floats::<2>(get("altitude"));
        Ok(Self {
            altitude: (a0, a1),
            delta_longitude: value::float(get("delta_longitude")),
            min_dist_factor: value::float(get("min_dist_factor")).min(FAR),
            second_probability: value::float(get("second_propability")).clamp(0.0, 1.0),
        })
    }
}

/// A thunderbolt collection's descriptions' sound names, in line order
/// (2.2); `None` for a description with an empty `sound`.
pub fn collection_sounds(config: &Config, name: &str) -> io::Result<Vec<Option<String>>> {
    let section = config
        .section(name)
        .ok_or_else(|| invalid(format!("no thunderbolt collection [{name}]")))?;
    // TODO(#320, P21): `lightning_model` and the picture keys are the bolt's
    // picture; a missing model is an error there, not checked here.
    section
        .entries()
        .map(|e| {
            let bolt = config
                .section(e.key())
                .ok_or_else(|| invalid(format!("[{name}]: no bolt section [{}]", e.key())))?;
            Ok(bolt
                .get("sound")
                .flatten()
                .filter(|s| !s.is_empty())
                .map(str::to_owned))
        })
        .collect()
}

/// A collection with its sounds created (effects category; AI sound type 0,
/// so no report: 5.5).
#[derive(Clone, Debug)]
pub struct Collection {
    pub name: String,
    pub descriptions: Vec<Option<SoundId>>,
}

impl Collection {
    pub fn create(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        name: &str,
        sounds: &[Option<String>],
    ) -> Result<Self, Fatal> {
        let descriptions = sounds
            .iter()
            .map(|s| {
                s.as_deref()
                    .map(|s| system.create(tree, aliases, s, Category::Effects))
                    .transpose()
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            name: name.to_owned(),
            descriptions,
        })
    }

    pub fn discard(&self, system: &mut SoundSystem) {
        for &s in self.descriptions.iter().flatten() {
            system.discard(s);
        }
    }
}

/// The rain's sky factor *k* from the view object's sky visibility *h*
/// (4.3): 1 − 2 · (0.3 − min(*h*, 0.3)).
pub fn sky_factor(h: f32) -> f32 {
    1.0 - 2.0 * (0.3 - h.min(0.3))
}

/// The rain (section 4): on or off, kept for the whole run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rain {
    pub on: bool,
}

impl Rain {
    /// The rule of 4.2 with blended rain density `density`, sky factor `k`
    /// and the camera's eye `eye`, for the rain sound `sound`.
    pub fn frame(
        &mut self,
        system: &mut SoundSystem,
        sound: SoundId,
        density: f32,
        k: f32,
        eye: Vec3,
    ) {
        if !self.on {
            if density < RAIN_DENSITY {
                return;
            }
            self.on = true;
            let flags = PlayFlags {
                looped: true,
                two_d: false,
                delay: 0.0,
            };
            system.play(sound, flags);
            system.set_range(sound, RAIN_MIN, RAIN_MAX);
        } else if density < RAIN_DENSITY {
            self.on = false;
            system.stop(sound);
            return;
        }
        if system.has_emitter(sound) {
            system.set_position(sound, [eye[0], eye[1] + RAIN_HEIGHT, eye[2]]);
            system.set_play_volume(sound, k * density * 1.1);
        }
    }
}

/// The sun heading β of the blended sun direction `sun` (5.3).
pub fn sun_heading(sun: Vec3) -> f32 {
    let (x, z) = (sun[0], sun[2]);
    const TINY: f32 = 1e-7;
    if x.abs() < TINY && z.abs() < TINY {
        0.0
    } else if z.abs() < TINY {
        if x <= 0.0 {
            std::f32::consts::FRAC_PI_2
        } else {
            -std::f32::consts::FRAC_PI_2
        }
    } else {
        let b = -(x / z).atan();
        if z < 0.0 { b + std::f32::consts::PI } else { b }
    }
}

/// The blended values thunder takes from the environment of a frame (2.3).
#[derive(Clone, Copy, Debug)]
pub struct ThunderWeather<'a> {
    /// The collection in effect.
    pub collection: Option<&'a Collection>,
    /// *P*, *D* (s).
    pub period: f32,
    pub duration: f32,
    /// *F* (m).
    pub far_plane: f32,
    /// L_sun_dir.
    pub sun_dir: Vec3,
}

/// A bolt's sound (5.5).
#[derive(Clone, Debug, PartialEq)]
pub struct Thunder {
    /// The description's index in its collection.
    pub description: usize,
    pub position: Vec3,
    /// *s* (m).
    pub distance: f32,
    /// *s* / 300 (s).
    pub delay: f32,
}

/// What a frame did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThunderFrame {
    /// A bolt started; with its thunder, or none for a silent bolt.
    pub bolt: Option<Option<Thunder>>,
}

/// The thunder's state (5.1), kept for the whole run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThunderState {
    pub enabled: bool,
    /// *N* (s of the global clock).
    pub next: f32,
    pub running: bool,
    /// λ and *e* (s).
    pub life: f32,
    pub elapsed: f32,
    /// The smoothed frame time *dt* (s; 3.1).
    pub frame_time: f32,
}

impl ThunderState {
    /// The smoothed frame time of a frame `real` seconds after the previous
    /// one (3.1): 0.9 · real + 0.1 · *dt*, at most 0.1.
    pub fn smooth_frame_time(&mut self, real: f32) -> f32 {
        self.frame_time = (0.9 * real + 0.1 * self.frame_time).min(MAX_FRAME);
        self.frame_time
    }

    /// The rule of 5.2 at global clock `t` (s) with this frame's `weather`,
    /// smoothed frame time `dt`, the camera's eye `eye` and `common`.
    pub fn frame(
        &mut self,
        draws: &mut impl Draws,
        t: f32,
        dt: f32,
        weather: &ThunderWeather,
        common: &BoltCommon,
        eye: Vec3,
    ) -> ThunderFrame {
        let mut out = ThunderFrame::default();
        let p = weather.period;
        let in_effect = weather.collection.is_some();
        if self.enabled != in_effect {
            self.enabled = in_effect;
            self.next = t + p + draws.uniform(-0.5 * p, 0.5 * p);
        } else if let Some(collection) = weather.collection
            && self.enabled
            && t > self.next
            && !self.running
        {
            out.bolt = Some(self.bolt(draws, t, weather, collection, common, eye));
        }
        if self.running {
            if self.elapsed > self.life {
                self.running = false;
            }
            self.elapsed += dt;
        }
        out
    }

    /// A bolt (5.3, 5.4).
    fn bolt(
        &mut self,
        draws: &mut impl Draws,
        t: f32,
        weather: &ThunderWeather,
        collection: &Collection,
        common: &BoltCommon,
        eye: Vec3,
    ) -> Option<Thunder> {
        let (p, d) = (weather.period, weather.duration);
        self.running = true;
        self.elapsed = 0.0;
        self.life = d + draws.uniform(-0.5 * d, 0.5 * d);
        let n = collection.descriptions.len();
        // TODO(docs/behavior/sound-rain-thunder.md, Gaps found in review): a
        // collection without descriptions is unestablished; we draw and take
        // none (no shipped collection is empty).
        let r = draws.draw() as usize;
        let description = (n > 0).then(|| r % n);
        let altitude = draws
            .uniform(common.altitude.0, common.altitude.1)
            .to_radians();
        let beta = sun_heading(weather.sun_dir);
        let delta = common.delta_longitude.to_radians();
        let pi = std::f32::consts::PI;
        let heading = draws.uniform(beta - delta + pi, beta + delta + pi);
        let dir = [
            -heading.sin() * altitude.cos(),
            altitude.sin(),
            heading.cos() * altitude.cos(),
        ];
        let f = weather.far_plane;
        let s = draws.uniform(common.min_dist_factor * f, FAR * f);
        let position = [
            eye[0] + s * dir[0],
            eye[1] + s * dir[1],
            eye[2] + s * dir[2],
        ];
        // The three draws of the bolt's picture (P21).
        for _ in 0..3 {
            draws.draw();
        }
        if common.second_probability > draws.draw() as f32 / MAX as f32 {
            self.next = t + d + 0.001;
            return None;
        }
        self.next = t + p + draws.uniform(-0.3 * p, 0.3 * p);
        Some(Thunder {
            description: description?,
            position,
            distance: s,
            delay: s / SOUND_SPEED,
        })
    }
}

/// Plays a bolt's thunder (5.5): play independently, 3D, not looped,
/// delayed by *s* / 300 at its point, with the range *s* / 2 … 2 *s*.
pub fn play_thunder(system: &mut SoundSystem, collection: &Collection, thunder: &Thunder) {
    let Some(Some(sound)) = collection.descriptions.get(thunder.description) else {
        return;
    };
    let flags = PlayFlags {
        looped: false,
        two_d: false,
        delay: thunder.delay,
    };
    let s = thunder.distance;
    let settings = Settings {
        position: Some(thunder.position),
        frequency: None,
        range: Some((s / 2.0, 2.0 * s)),
        volume: None,
    };
    system.play_independent(*sound, flags, settings);
}

#[cfg(test)]
mod tests;
