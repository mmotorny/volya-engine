// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's body sounds (docs/behavior/sound-player-body.md, "S17",
//! cited by section): the hit sounds by type (3), heavy breathing and the
//! heartbeat (4), the shock of an explosion that lowers the effects factor
//! while the tinnitus rings (5), and the death sounds (6), on the
//! positional sound system (S2).
//!
//! The pick of a hit's or a death's sound (3 item 3, 6 item 1) draws from
//! the generator the hit's wounds draw from, the character's; the level
//! makes it at the hit and leaves a [`BodyEvent`], which these sounds play.
//! The AI sound types these sounds are created with (2) are not kept: their
//! reports are S24's, and nothing hears them here.

use std::io;

use crate::audio::Category;
use crate::character::hits::{HitType, NAMES, TYPE_COUNT};
use crate::config::{Config, PathAliases, value};
use crate::sound::Fatal;
use crate::sound::system::{PlayFlags, SoundId, SoundSystem, Vec3};
use crate::vfs::Vfs;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Where breathing and the heartbeat play, relative to the listener (4).
pub const BODY_POSITION: Vec3 = [0.0, 1.75, 0.0];
/// The heartbeat starts above this total bleeding (4 item 2).
pub const HEARTBEAT_BLEEDING: f32 = 0.6;
/// The heartbeat's play volume is the total bleeding plus this (4 item 2).
pub const HEARTBEAT_VOLUME: f32 = 0.25;
/// The tinnitus's play volume on an explosion (3 item 5).
pub const TINNITUS_VOLUME: f32 = 10.0;
/// The number of death sounds (2 item 3).
pub const DEATHS: usize = 4;
/// The section of the shock's effects (5.1).
pub const SHOCK_SECTION: &str = "snd_shock_effector";

/// The names the player's section gives (1, 2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodySoundNames {
    /// Each type's list, by type (2 item 1).
    pub hits: Vec<Vec<String>>,
    /// `heavy_breath_snd` and `heavy_blood_snd` (2 item 2).
    pub breath: String,
    pub heartbeat: String,
    /// `<name>\die0` … `die3` (2 item 3).
    pub deaths: Vec<String>,
}

impl BodySoundNames {
    /// The player's section `actor` (its object's name is the section, 2
    /// item 3): `hit_sounds`, `heavy_breath_snd` and `heavy_blood_snd`
    /// read without a default.
    pub fn read(system: &Config, actor: &str) -> io::Result<Self> {
        let hit_sounds = value::unquote(system.read(actor, "hit_sounds")?.trim());
        let lists = system
            .section(hit_sounds)
            .ok_or_else(|| invalid(format!("no section [{hit_sounds}]")))?;
        let hits = NAMES
            .iter()
            .map(|t| {
                // TODO(#424): without the key the program reads the empty
                // text; whether that is an empty list is unestablished (every
                // type has a key in this edition). Ours: no sound.
                let v = lists.get(t).flatten().unwrap_or("");
                value::list(v)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .collect();
        let read = |key| {
            system
                .read(actor, key)
                .map(|v| value::unquote(v.trim()).to_owned())
        };
        Ok(Self {
            hits,
            breath: read("heavy_breath_snd")?,
            heartbeat: read("heavy_blood_snd")?,
            deaths: (0..DEATHS).map(|i| format!("{actor}\\die{i}")).collect(),
        })
    }

    /// The length of type `t`'s list.
    pub fn list_len(&self, t: HitType) -> usize {
        self.hits.get(t.index()).map_or(0, Vec::len)
    }
}

/// `[snd_shock_effector]` (5.1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShockEffector {
    /// `pp_eff_name`: the visual effect's file, if any.
    pub pp_eff_name: Option<String>,
    pub pp_eff_cyclic: bool,
    /// `cam_eff_name`: the camera animation's, if any.
    pub cam_eff_name: Option<String>,
}

impl ShockEffector {
    /// The section, empty when absent (our choice: the data has it).
    pub fn read(system: &Config) -> Self {
        let get = |key| {
            system
                .get(SHOCK_SECTION, key)
                .flatten()
                .map(|v| value::unquote(v.trim()).to_owned())
        };
        Self {
            pp_eff_name: get("pp_eff_name"),
            pp_eff_cyclic: get("pp_eff_cyclic").is_some_and(|v| value::bool(&v)),
            cam_eff_name: get("cam_eff_name"),
        }
    }
}

/// What the level leaves for the body sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BodyEvent {
    /// A hit of type `kind` and power `power` (3): `pick` the item of its
    /// list (None for an empty list or `telepatic`, which play nothing),
    /// `alive` with the health before it, `eye` the player's eye.
    Hit {
        kind: HitType,
        power: f32,
        pick: Option<usize>,
        alive: bool,
        eye: Vec3,
    },
    /// The player died (6): death sound `pick` at its feet.
    Death { pick: usize, feet: Vec3 },
}

/// The shock's visual effect (5.3), not drawn here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualEffect {
    pub cyclic: bool,
    /// Its intensity *I* after its latest step.
    pub intensity: f32,
}

/// A shock while it runs (5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shock {
    /// The length *L* (ms) and life ℓ (s).
    pub length: f32,
    pub life: f32,
    /// The end time *E* (s of the global clock).
    pub end: f32,
    /// The saved factor *F*₀.
    pub saved: f32,
    /// The elapsed time *e* (ms).
    pub elapsed: f32,
    pub visual: Option<VisualEffect>,
    /// The camera animation runs (none in this edition's data).
    pub camera: bool,
}

/// The lowest visual intensity, which ends the effect (5.3).
const MIN_INTENSITY: f32 = 0.0001;

impl Shock {
    /// A shock of length `length` ms and power `power` at global time `now`
    /// (s) with the effects factor `factor` (5.1); the factor it sets is
    /// 0.1 · *F*₀.
    pub fn start(length: f32, power: f32, now: f32, factor: f32, e: &ShockEffector) -> Self {
        let life = 4.0 * power.clamp(0.1, 1.5);
        Self {
            length,
            life,
            end: now + life,
            saved: factor,
            elapsed: 0.0,
            visual: e.pp_eff_name.as_ref().map(|_| VisualEffect {
                cyclic: e.pp_eff_cyclic,
                intensity: 1.0,
            }),
            camera: e.cam_eff_name.is_some(),
        }
    }

    /// The factor at the start (5.1).
    pub fn start_factor(&self) -> f32 {
        0.1 * self.saved
    }

    /// The per-frame update (5.2 items 1, 2) after `delta` ms: the factor
    /// to set, if it changes.
    pub fn frame(&mut self, delta: f32) -> Option<f32> {
        self.elapsed += delta;
        let x = 2.0 * self.elapsed / self.length - 1.0;
        let low = 0.1 * self.saved;
        (x > 0.0).then_some(low + (self.saved - low) * x)
    }

    /// Whether the shock ends at this frame (5.2 item 3): neither effect
    /// runs, or the player is not alive.
    pub fn ends(&self, alive: bool) -> bool {
        (self.visual.is_none() && !self.camera) || !alive
    }

    /// The camera effects' update (5.3) at global time `now` (s) with the
    /// frame time `dt` (s): the visual effect, no longer valid (*e* > *L*),
    /// is removed; otherwise it takes its step and is removed when the step
    /// ends it.
    pub fn camera_effects(&mut self, now: f32, dt: f32) {
        let Some(v) = &mut self.visual else {
            return;
        };
        if self.elapsed > self.length {
            self.visual = None;
            return;
        }
        let f = (((self.end - now) / self.life) * self.life * 0.125).clamp(0.0, 1.0);
        v.intensity = (f + (1.0 - f) * dt).clamp(MIN_INTENSITY, 1.0);
        if (v.intensity - MIN_INTENSITY).abs() < 1e-7 {
            self.visual = None;
        }
        // TODO(#663): the visual effect (`snd_shock.ppe`) is not drawn; it
        // waits for the post-process files' spec (#189).
    }
}

/// The player's body sounds in the sound system (2).
#[derive(Clone, Debug)]
pub struct BodySounds {
    hits: Vec<Vec<SoundId>>,
    breath: SoundId,
    heartbeat: SoundId,
    deaths: Vec<SoundId>,
    effector: ShockEffector,
    shock: Option<Shock>,
    /// The global clock at the previous per-frame update (ms).
    last: Option<u64>,
}

/// What the player's state gives the per-update sounds (4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyState {
    pub limping: bool,
    pub alive: bool,
    /// The total bleeding *B*.
    pub bleeding: f32,
}

impl BodySounds {
    /// Creates them in the effects category (2): fatal as creating any is.
    pub fn create(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        names: &BodySoundNames,
        effector: &ShockEffector,
    ) -> Result<Self, Fatal> {
        let mut create = |name: &str| system.create(tree, aliases, name, Category::Effects);
        let mut hits = Vec::with_capacity(TYPE_COUNT);
        for list in &names.hits {
            hits.push(list.iter().map(|n| create(n)).collect::<Result<_, _>>()?);
        }
        Ok(Self {
            hits,
            breath: create(&names.breath)?,
            heartbeat: create(&names.heartbeat)?,
            deaths: (names.deaths.iter())
                .map(|n| create(n))
                .collect::<Result<_, _>>()?,
            effector: effector.clone(),
            shock: None,
            last: None,
        })
    }

    /// Discards them, which stops their emitters (2); a running shock ends
    /// (the player object going offline, 5.4).
    pub fn discard(mut self, system: &mut SoundSystem) {
        self.end_shock(system);
        let ids = (self.hits.iter().flatten())
            .chain([&self.breath, &self.heartbeat])
            .chain(&self.deaths);
        for &id in ids {
            system.discard(id);
        }
    }

    /// The shock, while it runs.
    pub fn shock(&self) -> Option<&Shock> {
        self.shock.as_ref()
    }

    /// The breathing and heartbeat sounds.
    pub fn breath(&self) -> SoundId {
        self.breath
    }

    pub fn heartbeat(&self) -> SoundId {
        self.heartbeat
    }

    /// The sounds of type `t`'s list.
    pub fn hit_sounds(&self, t: HitType) -> &[SoundId] {
        self.hits.get(t.index()).map_or(&[], Vec::as_slice)
    }

    /// A level event at global time `now` (s).
    pub fn event(&mut self, system: &mut SoundSystem, event: BodyEvent, now: f32) {
        match event {
            BodyEvent::Hit {
                kind,
                power,
                pick,
                alive,
                eye,
            } => self.hit(system, kind, power, pick, alive, eye, now),
            BodyEvent::Death { pick, feet } => {
                // 6 items 1, 2, 4.
                // TODO(#664): item 3, the `game_over` sequence.
                if let Some(&id) = self.deaths.get(pick) {
                    system.play_at(id, feet, PlayFlags::default());
                }
                system.stop(self.breath);
                system.stop(self.heartbeat);
                self.end_shock(system);
            }
        }
    }

    /// 3 items 4 – 6 for a hit whose sound `pick` was drawn.
    #[allow(clippy::too_many_arguments)]
    fn hit(
        &mut self,
        system: &mut SoundSystem,
        kind: HitType,
        power: f32,
        pick: Option<usize>,
        alive: bool,
        eye: Vec3,
        now: f32,
    ) {
        let Some(list) = self.hits.get(kind.index()) else {
            return;
        };
        let Some(&s) = pick.and_then(|k| list.get(k)) else {
            return;
        };
        let busy = list.iter().any(|&id| system.has_emitter(id));
        // 5: the player is the controlled character here (believed in normal
        // play, S17 conventions).
        if kind == HitType::EXPLOSION {
            if system.has_emitter(s) {
                system.set_play_volume(s, TINNITUS_VOLUME);
            }
            if self.shock.is_none() {
                let length = system.duration_ms(s).unwrap_or(0) as f32;
                let shock =
                    Shock::start(length, power, now, system.effects_factor(), &self.effector);
                system.set_shock_factor(shock.start_factor());
                self.shock = Some(shock);
            }
        }
        if alive && !busy {
            system.play_at(s, eye, PlayFlags::default());
        }
    }

    /// The per-update sounds (4): breathing while limping and alive, the
    /// heartbeat above a total bleeding of 0.6 at play volume *B* + 0.25,
    /// both 2D at [`BODY_POSITION`] and looped.
    pub fn update(&mut self, system: &mut SoundSystem, state: BodyState) {
        let looped_2d = PlayFlags {
            looped: true,
            two_d: true,
            ..PlayFlags::default()
        };
        let keep = |system: &mut SoundSystem, id: SoundId| {
            if system.has_emitter(id) {
                system.set_position(id, BODY_POSITION);
            } else {
                system.play_at(id, BODY_POSITION, looped_2d);
            }
        };
        if state.limping && state.alive {
            keep(system, self.breath);
        } else {
            system.stop(self.breath);
        }
        if state.bleeding > HEARTBEAT_BLEEDING {
            keep(system, self.heartbeat);
            system.set_play_volume(self.heartbeat, state.bleeding + HEARTBEAT_VOLUME);
        } else {
            system.stop(self.heartbeat);
        }
        if !state.alive {
            system.stop(self.heartbeat);
        }
    }

    /// The player's per-frame update (5.2) after `delta` ms of the global
    /// clock, with the player `alive`; a paused frame runs none (the
    /// player's object is not processed then: believed).
    pub fn frame(&mut self, system: &mut SoundSystem, delta: f32, alive: bool) {
        let Some(shock) = &mut self.shock else {
            return;
        };
        if let Some(f) = shock.frame(delta) {
            system.set_shock_factor(f);
        }
        if shock.ends(alive) {
            self.end_shock(system);
        }
    }

    /// The per-frame update (5.2) at global clock `now` (ms): Δ is its
    /// advance since the previous one.
    pub fn frame_at(&mut self, system: &mut SoundSystem, now: u64, alive: bool) {
        let delta = self.last.map_or(0, |l| now.saturating_sub(l));
        self.last = Some(now);
        self.frame(system, delta as f32, alive);
    }

    /// The camera effects' update (5.3) at global time `now` (s) with the
    /// frame time `dt` (s), every frame of the level.
    // TODO(#424): its order against the per-frame update within a frame is
    // unestablished; ours runs after it.
    pub fn camera_effects(&mut self, now: f32, dt: f32) {
        if let Some(shock) = &mut self.shock {
            shock.camera_effects(now, dt);
        }
    }

    /// The shock ends (5.4): the saved factor is restored, the effects
    /// removed.
    fn end_shock(&mut self, system: &mut SoundSystem) {
        if let Some(shock) = self.shock.take() {
            system.set_shock_factor(shock.saved);
        }
    }
}

#[cfg(test)]
mod tests;
