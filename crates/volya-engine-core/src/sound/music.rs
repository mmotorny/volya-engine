// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Level music (docs/behavior/sound-level-music.md): the level's track list
//! in the game configuration (section 2), the tracks' sounds (2.3), the
//! state (section 3) and the per-frame rule (section 4).
//!
//! Section numbers below are the spec's.

use std::io;

use super::Fatal;
use super::random::range;
use super::system::{PlayFlags, SoundId, SoundSystem};
use crate::audio::Category;
use crate::config::{Config, PathAliases};
use crate::vfs::Vfs;

/// The interval between checks while no track is active (4.3).
const RECHECK: u32 = 10_000;
/// Where the channels play, listener-relative (4.2).
const LEFT: [f32; 3] = [-0.5, 0.0, 0.3];
const RIGHT: [f32; 3] = [0.5, 0.0, 0.3];

/// A track's values (2.2).
#[derive(Clone, Debug, PartialEq)]
pub struct TrackParams {
    /// Window start and end (ms).
    pub start: u32,
    pub end: u32,
    /// Play volume *v*.
    pub volume: f32,
    /// Pause min and max (ms).
    pub pause_min: u32,
    pub pause_max: u32,
}

/// A C `sscanf` `%d` at the start of `s` (leading whitespace skipped):
/// the value and the rest, or `None` when nothing converts.
fn scan_int(s: &str) -> Option<(i32, &str)> {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let b = t.as_bytes();
    let sign = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let digits = b[sign..].iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let end = sign + digits;
    // Overflow is undefined for `%d`; wrapping is our choice.
    Some((value_int(&t[..end]), &t[end..]))
}

fn value_int(text: &str) -> i32 {
    crate::config::value::int(text) as i32
}

/// A C `sscanf` `%f` at the start of `s`.
fn scan_float(s: &str) -> Option<(f32, &str)> {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let b = t.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let whole = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    i += whole;
    let mut fraction = 0;
    if b.get(i) == Some(&b'.') {
        fraction = b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        i += 1 + fraction;
    }
    if whole + fraction == 0 {
        return None;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        j += usize::from(matches!(b.get(j), Some(b'+' | b'-')));
        let exp = b[j.min(b.len())..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if exp > 0 {
            i = j + exp;
        }
    }
    Some((crate::config::value::float(&t[..i]), &t[i..]))
}

/// A literal `,` of the format (whitespace before it is not skipped).
fn comma(s: &str) -> Option<&str> {
    s.strip_prefix(',')
}

impl TrackParams {
    /// An entry's value scanned as `%d,%d,%f,%d,%d` (2.2): the scan stops at
    /// the first field that does not convert, which keeps its default with
    /// all after it; *p*₁ is raised by 1 when equal to *p*₀; then hours and
    /// seconds become milliseconds.
    pub fn parse(value: &str) -> Self {
        let (mut h0, mut h1, mut volume, mut p0, mut p1) = (0i32, 0i32, 1.0f32, 0i32, 0i32);
        (|| {
            let (v, s) = scan_int(value)?;
            h0 = v;
            let (v, s) = scan_int(comma(s)?)?;
            h1 = v;
            let (v, s) = scan_float(comma(s)?)?;
            volume = v;
            let (v, s) = scan_int(comma(s)?)?;
            p0 = v;
            let (v, _) = scan_int(comma(s)?)?;
            p1 = v;
            Some(())
        })();
        if p0 == p1 {
            p1 = p1.wrapping_add(1);
        }
        Self {
            start: h0.wrapping_mul(3_600_000) as u32,
            end: h1.wrapping_mul(3_600_000) as u32,
            volume,
            pause_min: p0.wrapping_mul(1_000) as u32,
            pause_max: p1.wrapping_mul(1_000) as u32,
        }
    }

    /// Active at time of day τ (ms; 4.2): always for start = end = 0, else
    /// start ≤ τ < end as signed values; a window never wraps past midnight.
    pub fn active(&self, tau: u32) -> bool {
        (self.start == 0 && self.end == 0)
            || ((self.start as i32) <= (tau as i32) && (tau as i32) < (self.end as i32))
    }
}

/// The track list of level `level` (2.1) in the game configuration `game`
/// (`config\game.ltx` with its includes): the entries of the section
/// `[<level>] music_tracks` names, in its entry order, with their names as
/// written. No section, key or value: no music. A named section that does
/// not exist is an error.
pub fn track_list(game: &Config, level: &str) -> io::Result<Vec<(String, TrackParams)>> {
    if !game.contains(level) {
        return Ok(Vec::new());
    }
    let Some(Some(name)) = game.get(level, "music_tracks") else {
        return Ok(Vec::new());
    };
    if name.is_empty() {
        return Ok(Vec::new());
    }
    let section = game.section(name).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("[{level}] music_tracks: no section [{name}]"),
        )
    })?;
    section
        .entries()
        .map(|e| {
            // TODO(docs/behavior/sound-level-music.md section 2.2): an entry
            // without a value is unestablished; we fail the load.
            let value = e.value().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("[{name}] {}: no value", e.key()),
                )
            })?;
            Ok((e.key().to_owned(), TrackParams::parse(value)))
        })
        .collect()
}

/// A track with its two sounds (2.3).
#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    pub params: TrackParams,
    pub left: SoundId,
    pub right: SoundId,
}

/// The level's music: its tracks and state (section 3).
#[derive(Debug, Default)]
pub struct LevelMusic {
    pub tracks: Vec<Track>,
    /// Index of the current track.
    pub current: Option<usize>,
    /// Next check time, a value of the clock *now* (ms).
    pub next_check: u32,
}

impl LevelMusic {
    /// Creates each track's sounds, *N* `_l` and *N* `_r`, in the music
    /// category (2.3; their AI sound type stated as 0 makes no AI report).
    /// A file that cannot be opened is fatal.
    pub fn create(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        list: &[(String, TrackParams)],
    ) -> Result<Self, Fatal> {
        let mut tracks = Vec::with_capacity(list.len());
        for (name, params) in list {
            let left = system.create(tree, aliases, &format!("{name}_l"), Category::Music)?;
            let right = system.create(tree, aliases, &format!("{name}_r"), Category::Music)?;
            tracks.push(Track {
                name: name.clone(),
                params: params.clone(),
                left,
                right,
            });
        }
        Ok(Self {
            tracks,
            current: None,
            next_check: 0,
        })
    }

    /// Discards the tracks' sounds (the level's teardown, section 6).
    pub fn discard(&mut self, system: &mut SoundSystem) {
        for t in self.tracks.drain(..) {
            system.discard(t.left);
            system.discard(t.right);
        }
        self.current = None;
    }

    fn playing(system: &SoundSystem, t: &Track) -> bool {
        system.has_emitter(t.left) || system.has_emitter(t.right)
    }

    /// The current track's name.
    pub fn current_name(&self) -> Option<&str> {
        self.current.map(|i| self.tracks[i].name.as_str())
    }

    /// The rule of 4.3 at clock `now` and time of day `tau` (ms). Run once
    /// per frame of the running level, after the static sources, not while
    /// game time is paused nor under the loading image (4.1).
    pub fn frame(&mut self, system: &mut SoundSystem, now: u32, tau: u32) {
        if self.tracks.is_empty() {
            return;
        }
        if self.current.is_none() && now > self.next_check {
            for t in &self.tracks {
                if Self::playing(system, t) {
                    system.stop_with_fade(t.left);
                    system.stop_with_fade(t.right);
                }
            }
            let active: Vec<usize> = (0..self.tracks.len())
                .filter(|&i| self.tracks[i].params.active(tau))
                .collect();
            if active.is_empty() {
                self.next_check = now.wrapping_add(RECHECK);
            } else {
                let n = system.random().draw() as usize;
                let i = active[n % active.len()];
                self.current = Some(i);
                let t = &self.tracks[i];
                let flags = PlayFlags {
                    looped: false,
                    two_d: true,
                    delay: 0.0,
                };
                for (sound, at) in [(t.left, LEFT), (t.right, RIGHT)] {
                    system.play_at(sound, at, flags);
                    system.set_play_volume(sound, t.params.volume);
                }
            }
        }
        if let Some(i) = self.current
            && !Self::playing(system, &self.tracks[i])
        {
            self.current = None;
            let p = &self.tracks[i].params;
            // The original skips the draw for bounds 0, 0, which 2.2 never
            // produces (its bounds always differ).
            let pause = if p.pause_min == 0 && p.pause_max == 0 {
                0
            } else {
                let n = system.random().draw();
                range(n, p.pause_min, p.pause_max).unwrap_or(p.pause_min)
            };
            self.next_check = now.wrapping_add(pause);
        }
    }
}

#[cfg(test)]
mod tests;
