// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's footsteps and landings in a level
//! (docs/behavior/player-footsteps.md, S11), on the material pairs of
//! `gamemtl.xr` (docs/formats/material-pairs.md, S9) and the lengths of
//! the player's cycles (docs/behavior/player-cycle-lengths.md, S10).
//!
//! [`MaterialPairs`] are read with the materials; [`PairSounds`] are their
//! lists' sounds in the sound system, created with the level's sounds;
//! [`PlayerFootsteps`] chooses the leg cycle and times the steps on each
//! frame of the level while the pause is off, and [`PairSounds::play`]
//! plays what they give.
//!
//! The step clock (S11 3.1) is the engine's global clock of
//! sound-weather-ambient.md 3.1 (`volya-engine-core::pause`), which the
//! app keeps and gives to each frame.

use std::io;
use std::sync::Arc;

use crate::core::audio::Category;
use crate::core::config::PathAliases;
use crate::core::cycles::Cycles;
use crate::core::footsteps::{Footsteps, LegState, Step, StepTable, leg_cycle, own_material};
use crate::core::movement::Player;
use crate::core::sound::Fatal;
use crate::core::sound::system::{PlayFlags, Settings, SoundId, SoundSystem, Vec3};
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::objects;
use volya_engine_formats::gamemtl::{self, GameMaterial, PairTable};
use volya_engine_formats::skeletal_motions::{self, ModelMotions};

/// The step sound plays this far above the feet (S11 4.3).
pub const STEP_HEIGHT: f32 = 0.5;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A pair's lists as loading makes them (S9 sections 3, 4): the sound
/// lists' names and the particle and mark items.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PairLists {
    pub breaking: Vec<String>,
    pub step: Vec<String>,
    pub collide: Vec<String>,
    /// Kept as names; their effects are the contact specs'.
    pub particles: Vec<String>,
    /// TODO(#325): each mark item makes a surface mark (shader
    /// `effects\wallmark`, S9 section 4); kept as names until marks exist.
    pub marks: Vec<String>,
}

/// The material pairs and the table that finds them (S9 section 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialPairs {
    /// In load order.
    pub lists: Vec<PairLists>,
    pub table: PairTable,
}

impl MaterialPairs {
    /// From `gamemtl.xr`'s bytes and its materials: more than 4 items in a
    /// list is fatal (S9 section 3), and so is a pair naming a material no
    /// material has (S9 section 5: undefined in the original).
    pub fn read(file: &[u8], materials: &[GameMaterial]) -> io::Result<Self> {
        let pairs = gamemtl::parse_pairs(file)?;
        let lists = pairs
            .iter()
            .map(|p| {
                Ok(PairLists {
                    breaking: p.breaking_items()?,
                    step: p.step_items()?,
                    collide: p.collide_items()?,
                    particles: p.particle_items()?,
                    marks: p.mark_items()?,
                })
            })
            .collect::<io::Result<_>>()?;
        Ok(Self {
            lists,
            table: PairTable::build(materials, &pairs)?,
        })
    }

    /// No materials and no pairs (a library without them).
    pub fn empty() -> Self {
        Self {
            lists: Vec::new(),
            table: PairTable::build(&[], &[]).expect("no pairs"),
        }
    }
}

/// The cycles of the model `visual` (S10 sections 1, 2): its motion
/// files' (`$level$` + item + `.omf`, else `$game_meshes$` + item +
/// `.omf`; neither is "Can't find motion file"), or its own. TODO(#325):
/// read for the player's record's model; a model an outfit changes is not
/// followed (S11 2.1 chooses the leg cycle again then).
pub fn read_cycles(tree: &dyn Vfs, data: &GameData, dir: &str, visual: &str) -> io::Result<Cycles> {
    let path = objects::locate_model(tree, data, dir, visual)?;
    let context = |path: &str, e: io::Error| io::Error::new(e.kind(), format!("{path}: {e}"));
    let mut file = tree.open(&path).map_err(|e| context(&path, e))?;
    let sets = match skeletal_motions::read_model_motions(&mut file, &path)
        .map_err(|e| context(&path, e))?
    {
        ModelMotions::Files(items) => items
            .iter()
            .map(|item| {
                let name = format!("{item}.omf");
                let level = format!("{dir}\\{name}");
                let path = if tree.exists(&level) {
                    level
                } else {
                    let meshes = data.aliases.path("$game_meshes$", &name)?;
                    if !tree.exists(&meshes) {
                        return Err(invalid(format!("Can't find motion file '{item}'.")));
                    }
                    meshes
                };
                let mut f = tree.open(&path).map_err(|e| context(&path, e))?;
                skeletal_motions::read_motion_file(&mut f, &path).map_err(|e| context(&path, e))
            })
            .collect::<io::Result<Vec<_>>>()?,
        ModelMotions::Embedded(set) => vec![set],
        ModelMotions::None => Vec::new(),
    };
    Ok(Cycles::new(&sets))
}

/// A step to be heard (S11 4.1): the pair whose step list it plays from
/// (never empty), the leg's volume and where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepSound {
    pub pair: usize,
    pub volume: f32,
    pub position: Vec3,
}

/// The player's leg cycle and steps in a level.
#[derive(Clone, Debug)]
pub struct PlayerFootsteps {
    pairs: Arc<MaterialPairs>,
    /// The index of the player's own material (S11 section 1).
    own: u32,
    cycles: Cycles,
    table: StepTable,
    state: Footsteps,
    stepped: Vec<Step>,
    /// The steps of the latest frame that are heard.
    sounds: Vec<StepSound>,
}

impl PlayerFootsteps {
    /// The actor's section `actor` of the system configuration gives the
    /// player's own material (its name in `materials`) and the step table
    /// (S11 section 1).
    pub fn new(
        data: &GameData,
        actor: &str,
        materials: &[GameMaterial],
        pairs: Arc<MaterialPairs>,
        cycles: Cycles,
    ) -> io::Result<Self> {
        let name = own_material(&data.system, actor)?;
        let own = materials
            .iter()
            .position(|m| m.name == name)
            .ok_or_else(|| invalid(format!("[{actor}] material {name}: no such material")))?;
        let table = StepTable::read(&data.system, actor, &cycles)?;
        Ok(Self {
            pairs,
            own: own as u32,
            cycles,
            table,
            state: Footsteps::default(),
            stepped: Vec::new(),
            sounds: Vec::new(),
        })
    }

    pub fn pairs(&self) -> &Arc<MaterialPairs> {
        &self.pairs
    }

    pub fn cycles(&self) -> &Cycles {
        &self.cycles
    }

    pub fn table(&self) -> &StepTable {
        &self.table
    }

    /// The leg cycle playing.
    pub fn leg_cycle(&self) -> Option<&str> {
        self.state.playing()
    }

    /// The steps of the latest frame that are heard.
    pub fn sounds(&self) -> &[StepSound] {
        &self.sounds
    }

    /// A frame that advanced game time, at step clock `now` (ms; module
    /// docs), after the player's movement update: the leg cycle chosen from
    /// its state (S11 2.1), then
    /// its steps updated (3.3; the order within a frame is unestablished,
    /// section 8), the player `alive` (health above 0). Fatal when the pair
    /// lookup is (an index beyond the materials, S9 section 5).
    pub fn frame(&mut self, now: u32, player: &Player, alive: bool) -> Result<(), Fatal> {
        self.sounds.clear();
        if !alive {
            return Ok(());
        }
        let state = player.state();
        let legs = LegState {
            flags: state.flags,
            runs: player.state_runs(),
            // TODO(#325): climbing is out of the movement's scope
            // (player-movement.md section 10).
            climbing: false,
            // TODO(#325): when the flag for turning the body while standing
            // is set is not stated (S11's review gap); the body never turns
            // on its own here (crate::core::movement), so never.
            turning: false,
        };
        let name = leg_cycle(&legs);
        self.state.choose(&name, now, &self.cycles, &self.table);
        // S11 4.1: the ground material, `default` before any contact.
        // TODO(#325): standing on a physical object (S11's review gap): only
        // the level's geometry exists, so its contacts alone give it.
        let ground = player.body().ground_material().unwrap_or(0);
        let pair = self.pairs.table.pair(self.own, ground).map_err(Fatal)?;
        self.stepped.clear();
        self.state.update(now, pair.is_some(), &mut self.stepped);
        let Some(pair) = pair else { return Ok(()) };
        // S11 4.1: heard on the ground with a sound to play; otherwise the
        // step passes silently. TODO(#325): each step's particle effect
        // from the pair's collide particle list (4.3) awaits particles.
        if state.airborne || self.pairs.lists[pair].step.is_empty() {
            return Ok(());
        }
        let [x, y, z] = state.position;
        let position = [x, y + STEP_HEIGHT, z];
        self.sounds.extend(self.stepped.iter().map(|s| StepSound {
            pair,
            volume: s.volume,
            position,
        }));
        Ok(())
    }
}

/// The pairs' sounds (S9 section 4): per pair, one sound per item of each
/// sound list, in the effects category.
#[derive(Debug, Default)]
pub struct PairSounds {
    pub breaking: Vec<Vec<SoundId>>,
    pub step: Vec<Vec<SoundId>>,
    pub collide: Vec<Vec<SoundId>>,
}

impl PairSounds {
    /// Creates every sound of `pairs`: fatal when a file cannot be opened
    /// (S1 section 5). The caller discards what was made on failure.
    pub fn create(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        pairs: &MaterialPairs,
    ) -> Result<Self, Fatal> {
        let mut out = Self::default();
        for p in &pairs.lists {
            let mut list = |names: &[String]| -> Result<Vec<SoundId>, Fatal> {
                names
                    .iter()
                    .map(|n| system.create(tree, aliases, n, Category::Effects))
                    .collect()
            };
            out.breaking.push(list(&p.breaking)?);
            out.step.push(list(&p.step)?);
            out.collide.push(list(&p.collide)?);
        }
        Ok(out)
    }

    pub fn discard(&self, system: &mut SoundSystem) {
        for id in self
            .breaking
            .iter()
            .chain(&self.step)
            .chain(&self.collide)
            .flatten()
        {
            system.discard(*id);
        }
    }

    /// Plays a step (S11 4.1, 4.3): sound *r* mod *m* of the pair's step
    /// list, played independently, 3D, not looped, without delay, at the
    /// step's position with the leg's volume. TODO(#325): the player is the
    /// sound's owner for the reports to the AI, which do not exist (S24).
    pub fn play(&self, system: &mut SoundSystem, step: &StepSound) {
        let Some(list) = self.step.get(step.pair).filter(|l| !l.is_empty()) else {
            return;
        };
        let r = system.random().draw() as usize;
        system.play_independent(
            list[r % list.len()],
            PlayFlags::default(),
            Settings {
                position: Some(step.position),
                volume: Some(step.volume),
                ..Settings::default()
            },
        );
    }
}
