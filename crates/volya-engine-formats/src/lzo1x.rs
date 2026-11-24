// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The public LZO1X compression format, as saved games use it
//! (docs/formats/sav.md, section 2.1): [`decompress`] reads any stream of the
//! format, [`compress`] writes one.
//!
//! The compressor is our own and simple (greedy matching over a hash chain);
//! the spec only requires a valid stream ("a writer need not reproduce the
//! game's choice of matches"). It uses only matches of 3 bytes or more, so the
//! two-byte and state-4 three-byte forms of instruction bytes below 16 are
//! read but never written (our choice).

use std::io;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("LZO1X: {}", message.into()),
    )
}

/// Reads a stream, failing on overrun instead of trusting it (the game's
/// decompressor checks nothing, section 2).
struct Input<'a> {
    data: &'a [u8],
    at: usize,
}

impl Input<'_> {
    fn byte(&mut self) -> io::Result<u8> {
        let b = *self
            .data
            .get(self.at)
            .ok_or_else(|| invalid("stream ends without an end marker"))?;
        self.at += 1;
        Ok(b)
    }

    fn u16(&mut self) -> io::Result<usize> {
        Ok(usize::from(u16::from_le_bytes([
            self.byte()?,
            self.byte()?,
        ])))
    }

    /// An extended length with base `base` whose length field is `field`.
    fn length(&mut self, field: usize, base: usize) -> io::Result<usize> {
        if field != 0 {
            return Ok(field);
        }
        let mut length = base;
        loop {
            match self.byte()? {
                0 => length += 255,
                b => return Ok(length + usize::from(b)),
            }
        }
    }

    fn literals(&mut self, out: &mut Vec<u8>, count: usize, limit: usize) -> io::Result<()> {
        grow(out, count, limit)?;
        let bytes = self
            .data
            .get(self.at..self.at + count)
            .ok_or_else(|| invalid("literals run past the end of the stream"))?;
        out.extend_from_slice(bytes);
        self.at += count;
        Ok(())
    }
}

/// Fails if `count` more bytes would take the output past `limit`.
fn grow(out: &[u8], count: usize, limit: usize) -> io::Result<()> {
    if count > limit - out.len() {
        return Err(invalid(format!(
            "output exceeds the expected {limit} bytes"
        )));
    }
    Ok(())
}

/// Copies a match byte by byte, so that it may overlap its own output.
fn copy_match(out: &mut Vec<u8>, distance: usize, length: usize, limit: usize) -> io::Result<()> {
    grow(out, length, limit)?;
    if distance > out.len() {
        return Err(invalid(format!(
            "match distance {distance} before the start of the output ({} bytes)",
            out.len()
        )));
    }
    let start = out.len() - distance;
    for i in 0..length {
        out.push(out[start + i]);
    }
    Ok(())
}

/// Most output reserved up front; more grows as the stream proves it (our
/// choice: an untrusted length must not allocate by itself).
const MAX_RESERVE: usize = 16 << 20;

/// Decompresses one stream up to its end marker into at most `limit` bytes:
/// output beyond `limit` is an error. Bytes after the end marker are ignored
/// (our choice).
pub fn decompress(stream: &[u8], limit: usize) -> io::Result<Vec<u8>> {
    let mut input = Input {
        data: stream,
        at: 0,
    };
    let mut out = Vec::with_capacity(limit.min(MAX_RESERVE));
    // `state` is S: literals copied by the previous instruction, 4 for a
    // literal run of 4 or more.
    let mut state = 0;
    if let Some(&first) = stream.first()
        && first > 17
    {
        input.at = 1;
        let count = usize::from(first - 17);
        input.literals(&mut out, count, limit)?;
        state = count.min(4);
    }
    loop {
        let t = usize::from(input.byte()?);
        let (distance, length, trailing) = if t >= 64 {
            let b = usize::from(input.byte()?);
            (((t >> 2) & 7) + 8 * b + 1, (t >> 5) + 1, t & 3)
        } else if t >= 32 {
            let length = input.length(t & 31, 31)? + 2;
            let d = input.u16()?;
            ((d >> 2) + 1, length, d & 3)
        } else if t >= 16 {
            let length = input.length(t & 7, 7)? + 2;
            let d = input.u16()?;
            let h = ((t & 8) << 11) + (d >> 2);
            if h == 0 {
                return Ok(out);
            }
            (16384 + h, length, d & 3)
        } else if state == 0 {
            let count = input.length(t, 15)? + 3;
            input.literals(&mut out, count, limit)?;
            state = 4;
            continue;
        } else if state < 4 {
            let b = usize::from(input.byte()?);
            ((t >> 2) + 4 * b + 1, 2, t & 3)
        } else {
            let b = usize::from(input.byte()?);
            ((t >> 2) + 4 * b + 2049, 3, t & 3)
        };
        copy_match(&mut out, distance, length, limit)?;
        input.literals(&mut out, trailing, limit)?;
        state = trailing;
    }
}

/// The end marker the game writes (section 2.1).
const END: [u8; 3] = [0x11, 0x00, 0x00];
/// Shortest match the compressor uses.
const MIN_MATCH: usize = 3;
/// Farthest match distance of the format (a 16–31 instruction: 16384 + 32767).
const MAX_DISTANCE: usize = 49_151;
/// Hash-chain candidates tried per position (our choice).
const MAX_CHAIN: usize = 32;
/// Matches are cut at this length (our choice; any length is encodable).
const MAX_MATCH: usize = 1 << 16;
const HASH_BITS: u32 = 15;

/// Appends an extended length: `value` above `base` takes a zero length field
/// and the bytes of section 2.1; otherwise `value` is the field itself
/// (returned, with nothing appended).
fn extended(out: &mut Vec<u8>, value: usize, base: usize) -> usize {
    if value <= base {
        return value;
    }
    let mut rest = value - base;
    while rest > 255 {
        out.push(0);
        rest -= 255;
    }
    out.push(rest as u8);
    0
}

/// Writes a match of `length` ≥ 3 at `distance` ≤ [`MAX_DISTANCE`] with
/// `trailing` (0–3) literals to follow.
fn write_match(out: &mut Vec<u8>, distance: usize, length: usize, trailing: usize) {
    debug_assert!(length >= MIN_MATCH && (1..=MAX_DISTANCE).contains(&distance) && trailing < 4);
    if length <= 8 && distance <= 2048 {
        let d = distance - 1;
        out.push((((length - 1) << 5) | ((d & 7) << 2) | trailing) as u8);
        out.push((d >> 3) as u8);
        return;
    }
    let (at, field, d) = if distance <= 16384 {
        let at = out.len();
        out.push(0);
        let field = extended(out, length - 2, 31);
        (at, 32 | field, ((distance - 1) << 2) | trailing)
    } else {
        let h = distance - 16384;
        let at = out.len();
        out.push(0);
        let field = extended(out, length - 2, 7);
        (
            at,
            16 | ((h >> 11) & 8) | field,
            ((h & 0x3FFF) << 2) | trailing,
        )
    };
    out[at] = field as u8;
    out.extend((d as u16).to_le_bytes());
}

/// Writes a literal run of `count` ≥ 1 bytes that starts the stream or
/// follows a match with no trailing literals.
fn write_literals(out: &mut Vec<u8>, literals: &[u8], first: bool) {
    let count = literals.len();
    if first && count <= 238 {
        out.push((count + 17) as u8);
    } else {
        debug_assert!(count >= 4);
        let at = out.len();
        out.push(0);
        out[at] = extended(out, count - 3, 15) as u8;
    }
    out.extend_from_slice(literals);
}

fn hash(bytes: &[u8]) -> usize {
    let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
    (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

/// Compresses `data` into one stream ending with the game's end marker.
/// Inputs shorter than 14 bytes become one literal run, as the game stores
/// them (section 2.1).
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2 + 16);
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; data.len()];
    // The pending match (distance, length) waits for the literal count after
    // it, which its instruction carries when below 4.
    let mut pending: Option<(usize, usize)> = None;
    let mut literal_start = 0;
    let mut pos = 0;
    let insert = |head: &mut [usize], prev: &mut [usize], at: usize| {
        if at + MIN_MATCH <= data.len() {
            let h = hash(&data[at..]);
            prev[at] = head[h];
            head[h] = at;
        }
    };
    let flush =
        |out: &mut Vec<u8>, pending: &mut Option<(usize, usize)>, literals: &[u8]| match pending
            .take()
        {
            None if literals.is_empty() => {}
            None => write_literals(out, literals, true),
            Some((distance, length)) if literals.len() < 4 => {
                write_match(out, distance, length, literals.len());
                out.extend_from_slice(literals);
            }
            Some((distance, length)) => {
                write_match(out, distance, length, 0);
                write_literals(out, literals, false);
            }
        };
    while data.len() >= 14 && pos + MIN_MATCH <= data.len() {
        let mut best = (0, 0);
        let mut candidate = head[hash(&data[pos..])];
        let limit = (data.len() - pos).min(MAX_MATCH);
        for _ in 0..MAX_CHAIN {
            if candidate == usize::MAX || pos - candidate > MAX_DISTANCE {
                break;
            }
            let length = data[candidate..]
                .iter()
                .zip(&data[pos..pos + limit])
                .take_while(|(a, b)| a == b)
                .count();
            if length > best.1 {
                best = (pos - candidate, length);
                if length == limit {
                    break;
                }
            }
            candidate = prev[candidate];
        }
        if best.1 < MIN_MATCH {
            insert(&mut head, &mut prev, pos);
            pos += 1;
            continue;
        }
        flush(&mut out, &mut pending, &data[literal_start..pos]);
        pending = Some(best);
        for at in pos..pos + best.1 {
            insert(&mut head, &mut prev, at);
        }
        pos += best.1;
        literal_start = pos;
    }
    flush(&mut out, &mut pending, &data[literal_start..]);
    out.extend(END);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn decodes_the_test_vector() {
        let stream = hex(
            "0c532e542e412e4c2e4b2e452e522e20204838000031000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f616c6c200b08000a6c616c6c616c6c616c6c616c6c110000",
        );
        assert_eq!(stream.len(), 110);
        let mut expected = b"S.T.A.L.K.E.R. ".repeat(8);
        expected.extend(0..0x40u8);
        expected.extend(b"all".repeat(20));
        let out = decompress(&stream, 244).unwrap();
        assert_eq!(out.len(), 244);
        assert_eq!(out, expected);
        assert_eq!(decompress(&compress(&expected), 244).unwrap(), expected);
    }

    #[test]
    fn short_inputs_are_one_literal_run() {
        assert_eq!(compress(b"a"), [0x12, 0x61, 0x11, 0x00, 0x00]);
        assert_eq!(compress(b""), END);
        assert!(decompress(&END, 0).unwrap().is_empty());
        let thirteen = [7u8; 13];
        let mut expected = vec![17 + 13];
        expected.extend(thirteen);
        expected.extend(END);
        assert_eq!(compress(&thirteen), expected);
    }

    /// Inputs exercising every instruction form the compressor writes: long
    /// and short literal runs, near, middle and far matches, long matches.
    fn samples() -> Vec<Vec<u8>> {
        let mut noise = Vec::new();
        let mut x = 0x1234_5678u32;
        for _ in 0..200_000 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            noise.push((x >> 24) as u8);
        }
        let mut far = noise[..60_000].to_vec();
        far.extend_from_slice(&noise[20_000..20_100]); // distance 40,000
        far.extend_from_slice(&noise[50_000..50_005]); // distance 10,105
        far.extend_from_slice(&noise[59_000..59_004]); // distance ~1,100
        far.extend_from_slice(&noise[100_000..100_300]);
        let mut sparse = Vec::new();
        for i in 0..30_000u32 {
            sparse.extend(&noise[(i as usize * 7) % 1000..][..(i % 5) as usize]);
            sparse.extend(b"abc");
        }
        let mut out: Vec<Vec<u8>> = (0..40).map(|n| noise[..n].to_vec()).collect();
        out.extend([
            vec![0; 100_000],
            b"ab".repeat(5000),
            noise.clone(),
            far,
            sparse,
            [&noise[..300], &[9; 70_000][..], &noise[..300]].concat(),
        ]);
        out
    }

    #[test]
    fn round_trips() {
        for data in samples() {
            let packed = compress(&data);
            assert_eq!(
                decompress(&packed, data.len()).unwrap(),
                data,
                "{}",
                data.len()
            );
        }
    }

    /// An independent implementation (the `lzokay-native` crate) reads our
    /// streams, and we read its.
    #[test]
    fn agrees_with_an_independent_implementation() {
        for data in samples() {
            let ours = compress(&data);
            assert_eq!(
                lzokay_native::decompress_all(&ours, Some(data.len())).unwrap(),
                data
            );
            if data.is_empty() {
                continue; // lzokay writes nothing for no input, without an end marker.
            }
            let theirs = lzokay_native::compress(&data).unwrap();
            assert_eq!(
                decompress(&theirs, data.len()).unwrap(),
                data,
                "{}",
                data.len()
            );
        }
    }

    #[test]
    fn rejects_broken_streams() {
        let packed = compress(&b"S.T.A.L.K.E.R. ".repeat(40));
        for cut in 0..packed.len() {
            assert!(decompress(&packed[..cut], 600).is_err(), "cut at {cut}");
        }
        // A match before the start of the output.
        assert!(decompress(&[0x12, 0x61, 0x40, 0x01, 0x11, 0, 0], 10).is_err());
        // Output past the limit, by literals and by a match.
        let packed = compress(&[5; 1000]);
        assert_eq!(decompress(&packed, 1000).unwrap().len(), 1000);
        assert!(decompress(&packed, 999).is_err());
        assert!(decompress(&compress(b"S.T.A.L.K.E.R."), 13).is_err());
        // A huge declared length reserves only what the stream fills.
        let out = decompress(&packed, usize::MAX).unwrap();
        assert_eq!(out.len(), 1000);
        assert!(out.capacity() <= MAX_RESERVE);
    }
}
