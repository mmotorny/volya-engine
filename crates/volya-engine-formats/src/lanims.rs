// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `lanims.xr`, the colour animation library (docs/formats/lanims-xr.md):
//! its layout (sections 2, 3), finding an entry by name (4), and an
//! entry's colour at a frame and at a time (6), in single precision as the
//! game evaluates it.

use std::io;

use crate::chunks::Chunks;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The f32 nearest to 1/255 (bit pattern `0x3B808081`, section 6.1).
pub const K: f32 = f32::from_bits(0x3B80_8081);

/// One named entry (section 3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// As stored (Windows-1251 bytes, without the 0 byte).
    pub name: Vec<u8>,
    /// Frames per second.
    pub frame_rate: f32,
    pub frame_count: i32,
    /// (frame, ARGB value), ordered by frame, frames unique.
    pub keys: Vec<(i32, u32)>,
}

/// The library: its entries in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColourAnimations {
    /// Chunk 0's version (0 when missing).
    pub version: u16,
    pub entries: Vec<Entry>,
}

impl ColourAnimations {
    /// Parses the file (section 3). A missing chunk 1 gives an empty
    /// library; an entry without its chunk 1 or 2 is a fatal error of the
    /// game, an error here, as is a chunk too short for its fields.
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(file)?;
        let version = match chunks.find(0) {
            Some(c) => {
                let b = c.content()?;
                let b: [u8; 2] = b
                    .get(..2)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| invalid("lanims.xr: version chunk too short"))?;
                u16::from_le_bytes(b)
            }
            None => 0,
        };
        let Some(list) = chunks.find(1) else {
            return Ok(Self {
                version,
                entries: Vec::new(),
            });
        };
        let list = list.content()?;
        let list = Chunks::parse(&list)?;
        let entries = list
            .list()
            .into_iter()
            .map(|c| Entry::parse(&c.content()?, version))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self { version, entries })
    }

    /// The first entry whose name equals `name` byte for byte (section 4);
    /// an empty name finds nothing.
    pub fn find(&self, name: &[u8]) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        self.entries.iter().position(|e| e.name == name)
    }
}

impl Entry {
    fn parse(data: &[u8], version: u16) -> io::Result<Self> {
        let chunks = Chunks::parse(data)?;
        let common = chunks.require(1, "lanims.xr entry")?.content()?;
        let keys = chunks.require(2, "lanims.xr entry")?.content()?;
        let end = common
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("lanims.xr: unterminated name"))?;
        let rest = &common[end + 1..];
        if rest.len() < 8 {
            return Err(invalid("lanims.xr: common data too short"));
        }
        let word = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let frame_rate = f32::from_bits(word(rest, 0));
        let frame_count = word(rest, 4) as i32;
        if keys.len() < 4 {
            return Err(invalid("lanims.xr: key chunk too short"));
        }
        let count = (word(&keys, 0) as i32).max(0) as usize;
        if keys.len() < 4 + 8 * count {
            return Err(invalid("lanims.xr: key chunk too short"));
        }
        // A set ordered by frame (signed); a later key at an earlier key's
        // frame replaces its value.
        let mut set: Vec<(i32, u32)> = Vec::with_capacity(count);
        for i in 0..count {
            let frame = word(&keys, 4 + 8 * i) as i32;
            let mut value = word(&keys, 8 + 8 * i);
            if version == 0 {
                // Bytes 0 and 2 exchanged.
                value = (value & 0xFF00_FF00) | (value >> 16 & 0xFF) | (value & 0xFF) << 16;
            }
            match set.binary_search_by_key(&frame, |k| k.0) {
                Ok(at) => set[at].1 = value,
                Err(at) => set.insert(at, (frame, value)),
            }
        }
        Ok(Self {
            name: common[..end].to_vec(),
            frame_rate,
            frame_count,
            keys: set,
        })
    }

    /// The colour at integer frame `f` (section 6.1); `None` for a frame
    /// greater than the frame count, a fatal error of the game. A frame
    /// before the first key is unestablished: the first key's value
    /// (TODO(#347)); an entry without keys gives 0 (TODO(#347): a review
    /// gap).
    pub fn colour_at_frame(&self, f: i32) -> Option<u32> {
        if f > self.frame_count {
            return None;
        }
        let at = self.keys.partition_point(|k| k.0 < f);
        Some(match (self.keys.get(at), at) {
            (Some(&(frame, value)), _) if frame == f => value,
            (None, 0) => 0,
            (None, _) => self.keys[at - 1].1,
            (Some(&(_, value)), 0) => value,
            (Some(&(f1, c1)), _) => {
                let (f0, c0) = self.keys[at - 1];
                let w = (f as f32 - f0 as f32) / (f1 as f32 - f0 as f32);
                let u = 1.0 - w;
                [24, 16, 8, 0].iter().fold(0, |acc, &s| {
                    let p = ((c0 >> s) & 0xFF) as f32 * K;
                    let q = ((c1 >> s) & 0xFF) as f32 * K;
                    let r = (u * p + q * w) * 255.0;
                    acc | (r.floor().clamp(0.0, 255.0) as u32) << s
                })
            }
        })
    }

    /// The length L = frame count ÷ frame rate, seconds (section 6.2).
    pub fn length(&self) -> f32 {
        self.frame_count as f32 / self.frame_rate
    }

    /// The length in whole milliseconds, floor(L × 1000) (section 6.3).
    pub fn length_ms(&self) -> i32 {
        (self.length() * 1000.0).floor() as i32
    }

    /// The frame at time `t` seconds (section 6.2): floor(phase × frame
    /// rate), the phase being t's remainder by L computed exactly. A
    /// length that is not positive and finite (a frame count or rate of 0)
    /// gives frame 0 (TODO(#347): a review gap).
    pub fn frame_at(&self, t: f32) -> i32 {
        let l = self.length();
        if !(l > 0.0 && l.is_finite()) {
            return 0;
        }
        // Rust's `%` on floats is the exact remainder truncated toward
        // zero, as C's fmod.
        let phase = t % l;
        (phase * self.frame_rate).floor() as i32
    }

    /// The colour and frame at time `t` seconds (section 6.2).
    pub fn colour_at(&self, t: f32) -> (u32, i32) {
        let frame = self.frame_at(t);
        // For t ≥ 0 the frame is never past the frame count (6.2).
        let colour = self
            .colour_at_frame(frame)
            .or_else(|| self.keys.last().map(|k| k.1))
            .unwrap_or(0);
        (colour, frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rate: f32, count: i32, keys: &[(i32, u32)]) -> Entry {
        Entry {
            name: b"test".to_vec(),
            frame_rate: rate,
            frame_count: count,
            keys: keys.to_vec(),
        }
    }

    fn chunk(id: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = id.to_le_bytes().to_vec();
        out.extend((payload.len() as u32).to_le_bytes());
        out.extend(payload);
        out
    }

    fn file(version: Option<u16>, entries: &[(&[u8], f32, i32, &[(i32, u32)])]) -> Vec<u8> {
        let mut list = Vec::new();
        for (i, (name, rate, count, keys)) in entries.iter().enumerate() {
            let mut common = name.to_vec();
            common.push(0);
            common.extend(rate.to_le_bytes());
            common.extend(count.to_le_bytes());
            let mut k = (keys.len() as i32).to_le_bytes().to_vec();
            for (f, v) in keys.iter() {
                k.extend(f.to_le_bytes());
                k.extend(v.to_le_bytes());
            }
            let mut e = chunk(1, &common);
            e.extend(chunk(2, &k));
            list.extend(chunk(i as u32, &e));
        }
        let mut out = version.map_or(Vec::new(), |v| chunk(0, &v.to_le_bytes()));
        out.extend(chunk(1, &list));
        out
    }

    #[test]
    fn k_is_the_nearest_f32_to_a_255th() {
        assert_eq!(K, 0.003_921_569);
        assert_eq!(K.to_bits(), (1.0f32 / 255.0).to_bits());
    }

    #[test]
    fn parse_and_find() {
        let f = file(
            Some(1),
            &[
                (b"a", 15.0, 10, &[(5, 2), (0, 1), (5, 3)]),
                (b"light\\omni_01", 15.0, 1, &[(0, 0x00FF_0000)]),
                (b"a", 30.0, 1, &[]),
            ],
        );
        let lib = ColourAnimations::parse(&f).unwrap();
        assert_eq!(lib.version, 1);
        assert_eq!(lib.entries.len(), 3);
        // Ordered by frame; the later key at frame 5 wins.
        assert_eq!(lib.entries[0].keys, vec![(0, 1), (5, 3)]);
        assert_eq!(lib.find(b"a"), Some(0));
        assert_eq!(lib.find(b"light\\omni_01"), Some(1));
        assert_eq!(lib.find(b"LIGHT\\omni_01"), None);
        assert_eq!(lib.find(b"light/omni_01"), None);
        assert_eq!(lib.find(b""), None);
        // Version 0 (here: no chunk 0) exchanges bytes 0 and 2.
        let f = file(None, &[(b"x", 1.0, 1, &[(0, 0x1122_3344)])]);
        let lib = ColourAnimations::parse(&f).unwrap();
        assert_eq!(lib.version, 0);
        assert_eq!(lib.entries[0].keys, vec![(0, 0x1144_3322)]);
        // Without chunk 1 the library is empty; an entry without its keys
        // is an error.
        assert!(
            ColourAnimations::parse(&chunk(0, &1u16.to_le_bytes()))
                .unwrap()
                .entries
                .is_empty()
        );
        let mut common = b"x\0".to_vec();
        common.extend([0; 8]);
        let broken = chunk(1, &chunk(0, &chunk(1, &common)));
        assert!(ColourAnimations::parse(&broken).is_err());
    }

    #[test]
    fn interpolation_in_single_precision() {
        // `asus_logo_01` (section 8, startup.md 6.4): 30 frames a second,
        // 120 frames; t = 0.5 is frame 15, 0x77000000; t = 3 frame 90,
        // 0xD9000000.
        let asus = entry(
            30.0,
            120,
            &[(0, 0), (32, 0xFF00_0000), (85, 0xFF00_0000), (119, 0)],
        );
        assert_eq!(asus.colour_at(0.5), (0x7700_0000, 15));
        assert_eq!(asus.colour_at(3.0), (0xD900_0000, 90));
        assert_eq!(asus.length_ms(), 4000);
        let e = entry(1.0, 10, &[(0, 0), (5, 0xFF)]);
        assert_eq!(e.colour_at_frame(5), Some(0xFF));
        // Held after the last key; past the frame count: fatal.
        assert_eq!(e.colour_at_frame(10), Some(0xFF));
        assert_eq!(e.colour_at_frame(11), None);
        // Each channel separately.
        let e = entry(1.0, 2, &[(0, 0x00FF_0000), (2, 0xFF00_00FF)]);
        assert_eq!(e.colour_at_frame(1), Some(0x7F7F_007F));
    }

    #[test]
    fn time_wraps_at_the_length() {
        // `light 40 hz`: 10 frames a second, 100 frames: L = 10 s.
        let e = entry(10.0, 100, &[(0, 0), (99, 0)]);
        assert_eq!(e.length(), 10.0);
        assert_eq!(e.length_ms(), 10000);
        assert_eq!(e.frame_at(0.35), 3);
        assert_eq!(e.frame_at(10.35), 3);
        // Single precision at the boundary (6.2: 0.7 gives 7, not 6).
        assert_eq!(e.frame_at(0.7), 7);
        // `warning_01`: 15 frames a second, 10 frames; L = 0.6666666865;
        // the largest f32 below it gives frame 9.
        let w = entry(15.0, 10, &[(0, 0), (9, 1)]);
        assert_eq!(w.length().to_bits(), 0x3F2A_AAAB);
        assert_eq!(w.length_ms(), 666);
        assert_eq!(w.frame_at(f32::from_bits(0x3F2A_AAAA)), 9);
        // `ui_new_jobs`: one key, held.
        let one = entry(15.0, 1, &[(0, 0xFFFF_FFFF)]);
        assert_eq!(one.colour_at(123.4), (0xFFFF_FFFF, 0));
        assert_eq!(one.length_ms(), 66);
    }
}
