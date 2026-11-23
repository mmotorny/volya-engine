// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Animated texture sequences, `textures\*.seq`
//! (docs/formats/ui-layout.md, section 6.4).

use std::io;

use crate::cp1251;
use crate::ltx::value;
use crate::xml::trim;

/// A frame sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sequence {
    /// Back-and-forth playback.
    pub cycled: bool,
    /// Frames per second.
    pub fps: u32,
    /// Frame texture paths in file order (still images, section 6.3).
    pub frames: Vec<String>,
}

impl Sequence {
    /// Parses a `.seq` file: lines split at CR/LF, empty lines (also after
    /// trimming) skipped; an optional first line `cycled` (case-insensitive),
    /// then the frame rate (read like `atoi`), then one frame path per line,
    /// trimmed. The spec does not cover a rate below 1 (its frame period
    /// would divide by zero) or a sequence without frames; both are
    /// rejected.
    pub fn parse(text: &[u8]) -> io::Result<Self> {
        let invalid = |m: String| io::Error::new(io::ErrorKind::InvalidData, m);
        let text = cp1251::decode(text);
        let mut lines = text.split(['\r', '\n']).map(trim).filter(|l| !l.is_empty());
        let mut first = lines
            .next()
            .ok_or_else(|| invalid("empty sequence".into()))?;
        let cycled = first.eq_ignore_ascii_case("cycled");
        if cycled {
            first = lines
                .next()
                .ok_or_else(|| invalid("sequence without a frame rate".into()))?;
        }
        let fps = value::int(first);
        let fps = u32::try_from(fps)
            .ok()
            .filter(|&f| f > 0)
            .ok_or_else(|| invalid(format!("sequence frame rate {fps}")))?;
        let frames: Vec<String> = lines.map(str::to_owned).collect();
        if frames.is_empty() {
            return Err(invalid("sequence without frames".into()));
        }
        Ok(Self {
            cycled,
            fps,
            frames,
        })
    }

    /// The frame period *p* = 1000 / *f* milliseconds, integer division.
    pub fn period_ms(&self) -> u64 {
        1000 / u64::from(self.fps)
    }

    /// The 0-based frame shown at time `t_ms` of the engine's continuous
    /// millisecond clock (no start offset): with *k* = ⌊*t* / *p*⌋ and *n*
    /// frames, *k* mod *n*; when cycled, *m* = *k* mod 2*n* and *m* if
    /// *m* < *n*, else *n* − 1 − (*m* mod *n*).
    pub fn frame_index(&self, t_ms: u64) -> usize {
        let n = self.frames.len() as u64;
        let k = t_ms / self.period_ms();
        let index = if self.cycled {
            let m = k % (2 * n);
            if m < n { m } else { n - 1 - (m % n) }
        } else {
            k % n
        };
        index as usize
    }

    /// The frame path shown at `t_ms`.
    pub fn frame(&self, t_ms: u64) -> &str {
        &self.frames[self.frame_index(t_ms)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_the_cursor() {
        let mut text = b"19\r\n".to_vec();
        for i in 1..=8 {
            text.extend_from_slice(format!("ui\\ui_ani_cursor_0{i}\r\n").as_bytes());
        }
        text.extend_from_slice(b"\r\n\r\n");
        let s = Sequence::parse(&text).unwrap();
        assert!(!s.cycled);
        assert_eq!((s.fps, s.period_ms(), s.frames.len()), (19, 52, 8));
        // Section 6.4 test vectors.
        for (t, frame) in [(0, 1), (51, 1), (52, 2), (415, 8), (416, 1), (1000, 4)] {
            assert_eq!(s.frame(t), format!("ui\\ui_ani_cursor_0{frame}"), "t = {t}");
        }
    }

    #[test]
    fn cycles_back_and_forth() {
        let s = Sequence::parse(b"Cycled\n1000\n a \nb\n\n  \nc").unwrap();
        assert!(s.cycled);
        assert_eq!(s.frames, ["a", "b", "c"]);
        let order: Vec<usize> = (0..9).map(|k| s.frame_index(k)).collect();
        assert_eq!(order, [0, 1, 2, 2, 1, 0, 0, 1, 2]);
    }

    #[test]
    fn rejects_unplayable() {
        assert!(Sequence::parse(b"").is_err());
        assert!(Sequence::parse(b"0\na").is_err());
        assert!(Sequence::parse(b"cycled\n").is_err());
        assert!(Sequence::parse(b"10\n").is_err());
    }
}
