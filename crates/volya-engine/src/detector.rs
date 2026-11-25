// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's detectors in a level (docs/behavior/sound-detector.md,
//! "S21", over [`crate::core::detector`]): each detector item of the
//! player character's inventory (class id `D_SIMDET`) is a detector whose
//! parent and holder is the player's character, switched by where the
//! inventory puts it (3), with its scheduled range test (4) and per-frame
//! ticks (5) against the level's zones; its sound lists (6) are played by
//! the app on the level's sounds ([`DetectorSounds`]).
//!
//! A detector's section is read when its item is made: with the level for
//! the items the character has then ([`read_sections`]), and for an item
//! given later by [`PlayerDetectors::add_section`].

use std::io;

use serde::Serialize;

use crate::core::audio::Category;
use crate::core::character::CharacterState;
use crate::core::config::{Config, PathAliases};
use crate::core::detector::{CLASS_ID, Detector, DetectorSection, ListSound, SoundCommand};
use crate::core::sound::Fatal;
use crate::core::sound::system::{PlayFlags, SoundId, SoundSystem};
use crate::core::vfs::Vfs;
use crate::core::zones::Zones;

/// Whether an item of `class_id` is a detector (S21 1).
pub fn is_detector(class_id: &str) -> bool {
    class_id == CLASS_ID
}

/// The detector sections of the detector items of `character`'s
/// inventory, each read once from `system` (S21 2.1): fatal as reading one
/// is.
pub fn read_sections(
    system: &Config,
    character: &CharacterState,
) -> io::Result<Vec<DetectorSection>> {
    let mut out: Vec<DetectorSection> = Vec::new();
    for i in &character.inventory.items {
        if is_detector(i.config.class_id.as_str())
            && !out.iter().any(|s| s.name == i.config.section)
        {
            out.push(DetectorSection::read(system, &i.config.section)?);
        }
    }
    Ok(out)
}

/// A detector's sound command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetectorSound {
    /// The detector's item id.
    pub detector: u16,
    pub command: DetectorCommand,
}

/// What the sound side does with a detector's sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectorCommand {
    Sound(SoundCommand),
    /// The detector object is destroyed: its sounds are discarded (S21 3).
    Discard,
}

/// One of the player's detectors.
#[derive(Clone, Debug)]
struct PlayerDetector {
    id: u16,
    /// Its section in [`PlayerDetectors::sections`].
    section: usize,
    detector: Detector,
}

/// A detector as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct DetectorReport {
    pub id: u16,
    pub section: String,
    pub on: bool,
    /// The zones of its range list and zone list (by object id), with the
    /// zone list's counters.
    pub range: Vec<u16>,
    pub zones: Vec<(u16, u32)>,
}

/// The player's detectors.
#[derive(Clone, Debug, Default)]
pub struct PlayerDetectors {
    sections: Vec<DetectorSection>,
    detectors: Vec<PlayerDetector>,
    sounds: Vec<DetectorSound>,
}

impl PlayerDetectors {
    pub fn new(sections: Vec<DetectorSection>) -> Self {
        Self {
            sections,
            ..Self::default()
        }
    }

    /// Adds the section of a detector item made after the level loaded.
    pub fn add_section(&mut self, section: DetectorSection) {
        if !self.sections.iter().any(|s| s.name == section.name) {
            self.sections.push(section);
        }
    }

    /// The section of detector `id`.
    pub fn section(&self, id: u16) -> Option<&DetectorSection> {
        let d = self.detectors.iter().find(|d| d.id == id)?;
        Some(&self.sections[d.section])
    }

    /// The section named `name`.
    pub fn section_named(&self, name: &str) -> Option<&DetectorSection> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// This frame's sound commands, taken.
    pub fn take_sounds(&mut self) -> Vec<DetectorSound> {
        std::mem::take(&mut self.sounds)
    }

    fn push(&mut self, detector: u16, command: DetectorCommand) {
        self.sounds.push(DetectorSound { detector, command });
    }

    /// Follows the inventory (S21 3): a new detector item is a detector,
    /// created off, whose parent and holder is the player's character at
    /// `position`; one that left the inventory loses its parent (its
    /// sounds stopped, its lists emptied) and goes with its sounds; each is
    /// switched by its item's place.
    // TODO(#111): an item that leaves the inventory is not placed in the
    // world (dropped items are not), so the detector goes with it.
    pub fn sync(&mut self, character: &CharacterState, position: [f32; 3]) {
        let inv = &character.inventory;
        let mut k = 0;
        while k < self.detectors.len() {
            if inv.items.iter().any(|i| i.id == self.detectors[k].id) {
                k += 1;
                continue;
            }
            let mut d = self.detectors.remove(k);
            let stop = d.detector.lose_parent();
            self.push(d.id, DetectorCommand::Sound(stop));
            self.push(d.id, DetectorCommand::Discard);
        }
        for i in &inv.items {
            if !is_detector(i.config.class_id.as_str())
                || self.detectors.iter().any(|d| d.id == i.id)
            {
                continue;
            }
            let Some(section) = self
                .sections
                .iter()
                .position(|s| s.name == i.config.section)
            else {
                continue;
            };
            let mut detector = Detector::new();
            detector.set_parent(true, position);
            self.detectors.push(PlayerDetector {
                id: i.id,
                section,
                detector,
            });
        }
        for d in &mut self.detectors {
            if let Some(item) = inv.items.iter().find(|i| i.id == d.id) {
                d.detector.placed(item.place);
            }
        }
    }

    /// A frame (S21 4, 5): the per-frame pass, each detector's zone list
    /// updated with the frame's `elapsed` ms and the player (its holder) at
    /// `player`, then the scheduler's pass at clock `clock`, the camera at
    /// `camera`: the detectors that are due follow the player, whose
    /// detectors they are, and run their range tests over `zones`. Runs
    /// only while the game is not paused.
    pub fn frame(
        &mut self,
        clock: u32,
        elapsed: u32,
        player: [f32; 3],
        camera: [f32; 3],
        zones: &Zones,
    ) {
        let mut out = Vec::new();
        for d in &mut self.detectors {
            out.clear();
            let s = &self.sections[d.section];
            d.detector.frame(elapsed, player, s, zones, &mut out);
            for &c in &out {
                self.sounds.push(DetectorSound {
                    detector: d.id,
                    command: DetectorCommand::Sound(c),
                });
            }
        }
        for d in &mut self.detectors {
            let s = &self.sections[d.section];
            d.detector.scheduled(clock, camera, player, true, s, zones);
        }
    }

    /// The detectors as the engine interface reports them.
    pub fn report(&self, zones: &Zones) -> Vec<DetectorReport> {
        let id = |i: usize| zones.zones().get(i).map_or(u16::MAX, |z| z.id);
        (self.detectors.iter())
            .map(|d| DetectorReport {
                id: d.id,
                section: self.sections[d.section].name.clone(),
                on: d.detector.on,
                range: d.detector.range.iter().map(|&i| id(i)).collect(),
                zones: (d.detector.zones.iter())
                    .map(|l| (id(l.zone), l.counter))
                    .collect(),
            })
            .collect()
    }
}

/// A detector's sound lists in the sound system (S21 2.2, 6): each entry's
/// sounds, in the effects category.
#[derive(Debug)]
pub struct DetectorSounds {
    lists: Vec<Vec<(SoundId, ListSound)>>,
}

impl DetectorSounds {
    /// Creates the sounds of `section`: fatal when a file cannot be opened
    /// (S1 section 5); the caller discards what was made on failure.
    pub fn create(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        section: &DetectorSection,
    ) -> Result<Self, Fatal> {
        let lists = (section.entries.iter())
            .map(|e| {
                (e.sounds.iter())
                    .map(|s| {
                        Ok((
                            system.create(tree, aliases, &s.name, Category::Effects)?,
                            s.clone(),
                        ))
                    })
                    .collect::<Result<Vec<_>, Fatal>>()
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { lists })
    }

    /// Carries out `command` (S21 6, 3): a tick stops every sound of its
    /// entry's list at once, chooses one with the shared generator (*k*
    /// mod *N*) and plays it at position (0, 0, 0), 2D, not looped, with
    /// its delay, its play volume then set to its volume; stopping all
    /// stops every sound of every list at once.
    // TODO(#147): the detector object the sound is associated with and the
    // AI sound type `item` are for the reports to the AI (S24).
    pub fn apply(&self, system: &mut SoundSystem, command: SoundCommand) {
        match command {
            SoundCommand::StopAll => {
                for (id, _) in self.lists.iter().flatten() {
                    system.stop(*id);
                }
            }
            SoundCommand::Tick { entry, .. } => {
                let Some(list) = self.lists.get(entry).filter(|l| !l.is_empty()) else {
                    return;
                };
                for (id, _) in list {
                    system.stop(*id);
                }
                let k = system.random().draw() as usize;
                let (id, s) = &list[k % list.len()];
                system.play_at(
                    *id,
                    [0.0; 3],
                    PlayFlags {
                        looped: false,
                        two_d: true,
                        delay: s.delay,
                    },
                );
                if system.has_emitter(*id) {
                    system.set_play_volume(*id, s.volume);
                }
            }
        }
    }

    /// The sounds, by entry (tests).
    pub fn ids(&self) -> impl Iterator<Item = SoundId> + '_ {
        self.lists.iter().flatten().map(|(id, _)| *id)
    }

    pub fn discard(&self, system: &mut SoundSystem) {
        for (id, _) in self.lists.iter().flatten() {
            system.discard(*id);
        }
    }
}
