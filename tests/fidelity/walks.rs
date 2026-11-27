// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The walks the `audio-fidelity` suite compares (#838): a New Game's player
// moved by held keys at fixed frames of play, the same plan in both
// engines. The Windows capture (tools/wincapture) holds the original's keys
// at these frames and records its audio output; our check
// (`volya_engine::selftest::audio_fidelity`) holds ours at the same frames
// and renders its audio offline, frame by frame. Included after views.rs
// (its `FRAME_MS`, `SETTINGS` and `ltx_with`); a change here needs a new
// capture.

/// A key a walk holds: the default bindings of player-movement.md 2.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkKey {
    /// `forward`, W.
    Forward,
    /// `left`, the left arrow: turns left at `rot_speed.x` (section 8.2).
    TurnLeft,
    /// `right`, the right arrow.
    TurnRight,
}

impl WalkKey {
    /// The key's set-1 scan code, `0xe0__` for an extended key (as
    /// SendInput sends it).
    pub const fn scan_code(self) -> u16 {
        match self {
            WalkKey::Forward => 0x11,
            WalkKey::TurnLeft => 0xe04b,
            WalkKey::TurnRight => 0xe04d,
        }
    }
}

/// Keys held for a number of frames.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub keys: &'static [WalkKey],
    pub frames: u32,
}

/// A walk: its name (a file name in both engines), what it does, the frame
/// of play whose update first reads its first segment's keys (frame 1 the
/// first after the New Game intro, as views.rs counts), its segments one
/// after the other, and the frames after them with no key, while the last
/// sounds end.
#[derive(Clone, Copy, Debug)]
pub struct Walk {
    pub name: &'static str,
    pub shows: &'static str,
    pub start: u32,
    pub segments: &'static [Segment],
    pub tail: u32,
}

impl Walk {
    /// The walk's frames, its segments' and its tail's.
    pub const fn frames(&self) -> u32 {
        let mut n = self.tail;
        let mut i = 0;
        while i < self.segments.len() {
            n += self.segments[i].frames;
            i += 1;
        }
        n
    }
}

/// Frames recorded before a walk's first, for what plays without it.
pub const PREROLL: u32 = 25;

/// The walks' sound lines: effects at full volume, music off, so the steps
/// are heard over the level's own sounds alone.
pub const AUDIO_SETTINGS: &[(&str, &str)] = &[("snd_volume_eff", "1."), ("snd_volume_music", "0.")];

/// A walk's user configuration file: views.rs's [`SETTINGS`] and
/// [`AUDIO_SETTINGS`] written into `shipped` (`ltx_with`).
pub fn audio_ltx(shipped: &[u8]) -> Vec<u8> {
    ltx_with(shipped, AUDIO_SETTINGS)
}

const fn seg(keys: &'static [WalkKey], frames: u32) -> Segment {
    Segment { keys, frames }
}

const W: &[WalkKey] = &[WalkKey::Forward];
const L: &[WalkKey] = &[WalkKey::TurnLeft];
const R: &[WalkKey] = &[WalkKey::TurnRight];

/// The walks, in the order they are captured.
pub const WALKS: &[Walk] = &[Walk {
    // Planned on our collision world (the tap routes' planner) and checked
    // by our movement to leave the bunker: turned at π rad/s (0.126 rad a
    // frame), run at the default run (section 4.1). It starts after the
    // loud bursts of the original's frames 728–802 and 859–873 of play
    // (#864), its preroll too.
    name: "bunker-exit",
    shows: "a New Game's player running out of the trader's bunker from the start: back \
            along the room, up the stairs, around the landing and out",
    start: 900,
    segments: &[
        seg(L, 23),
        seg(W, 66),
        seg(R, 12),
        seg(W, 50),
        seg(R, 12),
        seg(W, 24),
        seg(L, 12),
        seg(W, 8),
        seg(L, 12),
        seg(W, 100),
    ],
    tail: 50,
}];
