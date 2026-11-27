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

    /// The frames both engines keep, at the walk's boundaries: each as the
    /// walk's frame (0 its first with keys) and its name's suffix. `start`
    /// the last frame before the keys, `segNN` segment NN's last (01 the
    /// first), `end` the walk's last.
    pub fn boundaries(&self) -> Vec<(i64, String)> {
        let mut views = vec![(-1, "start".to_string())];
        let mut k = 0i64;
        for (i, s) in self.segments.iter().enumerate() {
            k += i64::from(s.frames);
            views.push((k - 1, format!("seg{:02}", i + 1)));
        }
        views.push((i64::from(self.frames()) - 1, "end".to_string()));
        views
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

/// The trader-room walk's segments: a turn to face the room's far end and
/// a run there, then 14 times a half turn to the right (25 frames, π rad)
/// and a run of 10 frames back along the room.
const TRADER_ROOM: [Segment; 30] = {
    let mut s = [seg(R, 25); 30];
    s[0] = seg(R, 14);
    s[1] = seg(W, 7);
    let mut i = 3;
    while i < s.len() {
        s[i] = seg(W, 10);
        i += 2;
    }
    s
};

/// The walks, in the order they are captured; each from its own New Game
/// (the capture runs one at a time, `--walk`, when their frames overlap).
pub const WALKS: &[Walk] = &[
    Walk {
        // Planned on our collision world (the tap routes' planner) and checked
        // by our movement to leave the bunker, before the trader's door was
        // ours: turned at π rad/s (0.126 rad a frame), run at the default
        // run (section 4.1). It starts after the loud bursts of the
        // original's frames 728–802 and 859–873 of play (#864), its preroll
        // too. Both players are stopped at the trader's locked door in
        // segment 2 (#866, #874).
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
    },
    Walk {
        // Planned on our collision world (its clearance around the start)
        // and checked by our movement to stay on the flooring tiles between
        // the counter and the trader's door (#867), at least 1.4 m from
        // the door and 0.3 m from the walls, never touching one: the
        // player turns to the room's far end and runs back and forth
        // along it, from 1.0 m behind the start to 0.45 m before it,
        // turning right each time (one way, so the body's lag after a
        // turn sends the runs alternately to either side, not always to
        // one). Steps on one material, from frame 900 of play as
        // bunker-exit.
        name: "trader-room",
        shows: "a New Game's player running back and forth in the trader's room, between \
                the counter and the locked door, turning in place at each end",
        start: 900,
        segments: &TRADER_ROOM,
        tail: 50,
    },
];
