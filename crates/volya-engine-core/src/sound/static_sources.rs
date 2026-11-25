// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's static sound sources (docs/behavior/sound-static-sources.md):
//! the file `level.snd_static` (sections 1, 2), each source's sound
//! (section 3) and the per-frame play rule (section 4).
//!
//! Section numbers below are the spec's.

use std::io;

use volya_engine_formats::chunks::Chunks;

use super::Fatal;
use super::random::range;
use super::system::{PlayFlags, SoundId, SoundSystem, Vec3};
use crate::audio::Category;
use crate::config::PathAliases;
use crate::vfs::Vfs;

/// The file's name in the level's directory (section 1).
pub const FILE: &str = "level.snd_static";

/// "Never" for a stop time (4.2).
const NEVER: u32 = u32::MAX;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// One source's record (section 2).
#[derive(Clone, Debug, PartialEq)]
pub struct SourceRecord {
    pub name: String,
    pub position: Vec3,
    /// Volume *v*.
    pub volume: f32,
    /// Frequency *f*.
    pub frequency: f32,
    pub active_from: u32,
    pub active_to: u32,
    pub play_min: u32,
    pub play_max: u32,
    pub pause_min: u32,
    pub pause_max: u32,
}

/// The sources of `level.snd_static`, in file order (section 2): every
/// top-level chunk is a source whose record is its sub-chunk 0; other
/// sub-chunks and bytes after the record's last field are ignored.
pub fn parse(bytes: &[u8]) -> io::Result<Vec<SourceRecord>> {
    let mut sources = Vec::new();
    for (i, chunk) in Chunks::parse(bytes)?.iter().enumerate() {
        let content = chunk.content()?;
        let sub = Chunks::parse(&content)?;
        // A missing sub-chunk 0 is an assertion failure in the original.
        let record = sub
            .find(0)
            .ok_or_else(|| invalid(format!("source {i}: no record (sub-chunk 0)")))?
            .content()?;
        sources.push(
            read_record(&record).ok_or_else(|| invalid(format!("source {i}: short record")))?,
        );
    }
    Ok(sources)
}

fn read_record(r: &[u8]) -> Option<SourceRecord> {
    let end = r.iter().position(|&b| b == 0)?;
    let name = volya_engine_formats::cp1251::decode(&r[..end]);
    // TODO(#320, docs/behavior/sound-static-sources.md section 7): what a
    // record shorter than 44 bytes after the name holds is unestablished
    // (the original does not detect it); we reject it.
    let f = r.get(end + 1..end + 45)?;
    let word = |k: usize| u32::from_le_bytes(f[4 * k..4 * k + 4].try_into().unwrap());
    let float = |k: usize| f32::from_bits(word(k));
    Some(SourceRecord {
        name,
        position: [float(0), float(1), float(2)],
        volume: float(3),
        frequency: float(4),
        active_from: word(5),
        active_to: word(6),
        play_min: word(7),
        play_max: word(8),
        pause_min: word(9),
        pause_max: word(10),
    })
}

/// A source with its sound and times (section 3).
#[derive(Clone, Debug)]
pub struct Source {
    pub record: SourceRecord,
    pub sound: SoundId,
    /// Next start and stop time, values of the clock *now* (ms).
    pub next_start: u32,
    pub stop_time: u32,
}

impl Source {
    /// Active at time of day τ (ms; 4.2): always for a window of 0, 0; else
    /// active from ≤ τ < active to, compared as signed values.
    pub fn active(&self, tau: u32) -> bool {
        let r = &self.record;
        (r.active_from == 0 && r.active_to == 0)
            || ((r.active_from as i32) <= (tau as i32) && (tau as i32) < (r.active_to as i32))
    }
}

/// The level's static sources.
#[derive(Debug, Default)]
pub struct StaticSources {
    pub sources: Vec<Source>,
}

impl StaticSources {
    /// Sets up every source of `records` in file order (section 3): its
    /// sound in the effects category (its file resolved and loaded now, a
    /// file that cannot be opened being fatal), both times 0. Identical
    /// records are separate sources.
    pub fn create(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        records: &[SourceRecord],
    ) -> Result<Self, Fatal> {
        let mut sources = Vec::with_capacity(records.len());
        for record in records {
            // The AI sound type is left unspecified (S1 7.2); reports to the
            // AI are not made yet (S24).
            let sound = system.create(tree, aliases, &record.name, Category::Effects)?;
            sources.push(Source {
                record: record.clone(),
                sound,
                next_start: 0,
                stop_time: 0,
            });
        }
        Ok(Self { sources })
    }

    /// Discards the sources' sounds (when the level is left).
    pub fn discard(&mut self, system: &mut SoundSystem) {
        for s in self.sources.drain(..) {
            system.discard(s.sound);
        }
    }

    /// The rule of section 4.2 for every source in file order, at clock
    /// `now` (ms) and time of day `tau` (ms). Run once per frame of the
    /// running level, not while paused nor under the loading image (4.1).
    /// A draw from a range with equal bounds is fatal (4.2).
    pub fn frame(&mut self, system: &mut SoundSystem, now: u32, tau: u32) -> Result<(), Fatal> {
        for source in &mut self.sources {
            let playing = system.has_emitter(source.sound);
            let r = source.record.clone();
            if !source.active(tau) {
                if playing {
                    system.stop_with_fade(source.sound);
                }
                continue;
            }
            if playing {
                if now >= source.stop_time {
                    system.stop_with_fade(source.sound);
                }
                continue;
            }
            if r.pause_min == 0 && r.pause_max == 0 {
                start(system, source, true);
                source.stop_time = NEVER;
            } else if now < source.next_start {
                // Waiting for the next start.
            } else if r.play_min == 0 && r.play_max == 0 {
                start(system, source, false);
                source.stop_time = NEVER;
                let duration = system.duration_ms(source.sound).unwrap_or(0) as u32;
                let pause = draw(system, r.pause_min, r.pause_max)?;
                source.next_start = now.wrapping_add(duration).wrapping_add(pause);
            } else {
                start(system, source, true);
                source.stop_time = now.wrapping_add(draw(system, r.play_min, r.play_max)?);
                source.next_start =
                    source
                        .stop_time
                        .wrapping_add(draw(system, r.pause_min, r.pause_max)?);
            }
        }
        Ok(())
    }
}

/// R(*a*, *b*) (4.2). With *b* = *a* the original divides by zero and
/// fails, so it is fatal here (no shipped source draws from such a range,
/// 6.2).
fn draw(system: &mut SoundSystem, a: u32, b: u32) -> Result<u32, Fatal> {
    let n = system.random().draw();
    range(n, a, b).ok_or_else(|| Fatal(format!("{FILE}: a range from {a} to {b} ms")))
}

/// Start, looped or once (4.2): play at the position, 3D, without delay;
/// then the play volume and frequency.
fn start(system: &mut SoundSystem, source: &Source, looped: bool) {
    let flags = PlayFlags {
        looped,
        two_d: false,
        delay: 0.0,
    };
    system.play_at(source.sound, source.record.position, flags);
    system.set_play_volume(source.sound, source.record.volume);
    system.set_frequency(source.sound, source.record.frequency);
}

#[cfg(test)]
mod tests;
