// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A lossless code for the sound cache's lines (#796): each sample
//! predicted from those before it by a fixed polynomial of order 0 to 3, the
//! one with the smallest residuals for the line, the residuals Rice coded in
//! partitions of [`PARTITION`] samples with a parameter each (the scheme of
//! FLAC's fixed predictors). A line the code would not shrink is kept as
//! its samples. Decoding gives back the samples exactly.

/// Samples per partition, each with its own Rice parameter.
const PARTITION: usize = 256;
/// The quotient at and past which a residual is written whole.
const ESCAPE: u32 = 24;
/// Bits of a residual written whole: zigzagged, an order 3 residual of
/// 16-bit samples is below 2^20.
const WHOLE: u32 = 20;
/// The first byte of a line kept as its samples.
const RAW: u8 = 0xff;

/// The prediction of order `order` for the sample after `h`, the samples
/// before it, most recent first (those before the line's start are 0).
#[inline(always)]
fn predict(order: u8, h: [i32; 3]) -> i32 {
    match order {
        0 => 0,
        1 => h[0],
        2 => 2 * h[0] - h[1],
        _ => 3 * h[0] - 3 * h[1] + h[2],
    }
}

#[inline(always)]
fn zigzag(r: i32) -> u32 {
    ((r << 1) ^ (r >> 31)) as u32
}

#[inline(always)]
fn unzigzag(u: u32) -> i32 {
    (u >> 1) as i32 ^ -((u & 1) as i32)
}

struct Writer<'a> {
    out: &'a mut Vec<u8>,
    acc: u64,
    bits: u32,
}

impl Writer<'_> {
    /// Writes the low `n` bits of `v` (`n` ≤ 32), most significant first.
    #[inline(always)]
    fn put(&mut self, v: u32, n: u32) {
        self.acc = (self.acc << n) | u64::from(v & ((1u64 << n) - 1) as u32);
        self.bits += n;
        while self.bits >= 8 {
            self.bits -= 8;
            self.out.push((self.acc >> self.bits) as u8);
        }
    }

    fn finish(mut self) {
        if self.bits > 0 {
            let pad = 8 - self.bits;
            self.put(0, pad);
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    /// Bits not yet read, from the most significant end.
    acc: u64,
    bits: u32,
}

impl Reader<'_> {
    #[inline(always)]
    fn refill(&mut self) {
        // Eight bytes at once where there are: the bits past the whole
        // bytes counted are the stream's next, read again next time.
        if let Some(chunk) = self.bytes.get(self.at..self.at + 8) {
            let v = u64::from_be_bytes(chunk.try_into().unwrap_or([0; 8]));
            self.acc |= v >> self.bits;
            let bytes = (63 - self.bits) / 8;
            self.at += bytes as usize;
            self.bits += bytes * 8;
            return;
        }
        while self.bits <= 56 {
            let b = self.bytes.get(self.at).copied().unwrap_or(0);
            self.at += 1;
            self.acc |= u64::from(b) << (56 - self.bits);
            self.bits += 8;
        }
    }

    /// The next `n` bits (`n` ≤ 32).
    #[inline(always)]
    fn take(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        if self.bits < n {
            self.refill();
        }
        let v = (self.acc >> (64 - n)) as u32;
        self.acc <<= n;
        self.bits -= n;
        v
    }

    /// The zeros before the next 1, which is read too (at most
    /// [`ESCAPE`]).
    #[inline(always)]
    fn unary(&mut self) -> u32 {
        if self.bits <= ESCAPE {
            self.refill();
        }
        let q = self.acc.leading_zeros();
        self.acc <<= q + 1;
        self.bits -= q + 1;
        q
    }
}

/// The Rice parameter that codes `values` in the fewest bits.
fn parameter(values: &[u32]) -> u32 {
    let sum: u64 = values.iter().map(|&u| u64::from(u)).sum();
    let mean = sum / values.len() as u64;
    let guess = (64 - mean.leading_zeros()).saturating_sub(1).min(WHOLE - 1);
    let cost = |k: u32| -> u64 {
        values
            .iter()
            .map(|&u| {
                let q = u >> k;
                if q >= ESCAPE {
                    u64::from(ESCAPE + 1 + WHOLE)
                } else {
                    u64::from(q + 1 + k)
                }
            })
            .sum()
    };
    let lo = guess.saturating_sub(1);
    let hi = (guess + 1).min(WHOLE - 1);
    (lo..=hi).min_by_key(|&k| cost(k)).unwrap_or(0)
}

/// Codes `samples` into `out` (cleared first); `scratch` is a buffer
/// kept between calls.
pub fn encode(samples: &[i16], out: &mut Vec<u8>, scratch: &mut Vec<u32>) {
    out.clear();
    // The order with the smallest residuals.
    let mut sums = [0u64; 4];
    let mut h = [0i32; 3];
    for &s in samples {
        let x = i32::from(s);
        for (order, sum) in sums.iter_mut().enumerate() {
            *sum += u64::from((x - predict(order as u8, h)).unsigned_abs());
        }
        h = [x, h[0], h[1]];
    }
    let order = (0..4u8).min_by_key(|&o| sums[usize::from(o)]).unwrap_or(0);
    scratch.clear();
    let mut h = [0i32; 3];
    scratch.extend(samples.iter().map(|&s| {
        let x = i32::from(s);
        let r = x - predict(order, h);
        h = [x, h[0], h[1]];
        zigzag(r)
    }));
    out.push(order);
    let mut w = Writer {
        out,
        acc: 0,
        bits: 0,
    };
    for part in scratch.chunks(PARTITION) {
        let k = parameter(part);
        w.put(k, 5);
        for &u in part {
            let q = u >> k;
            if q >= ESCAPE {
                w.put(1, ESCAPE + 1);
                w.put(u, WHOLE);
            } else {
                w.put(1, q + 1);
                w.put(u, k);
            }
        }
    }
    w.finish();
    if out.len() > samples.len() * 2 {
        out.clear();
        out.push(RAW);
        out.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
    }
}

/// The `n` samples coded in `code` ([`encode`]), into `out` (cleared
/// first).
pub fn decode(code: &[u8], n: usize, out: &mut Vec<i16>) {
    out.clear();
    let Some((&order, rest)) = code.split_first() else {
        return;
    };
    if order == RAW {
        let (pairs, _) = rest.as_chunks::<2>();
        out.extend(pairs.iter().take(n).map(|&b| i16::from_le_bytes(b)));
        return;
    }
    let mut r = Reader {
        bytes: rest,
        at: 0,
        acc: 0,
        bits: 0,
    };
    let mut h = [0i32; 3];
    let mut left = n;
    while left > 0 {
        let len = left.min(PARTITION);
        left -= len;
        let k = r.take(5);
        for _ in 0..len {
            let q = r.unary();
            let u = if q >= ESCAPE {
                r.take(WHOLE)
            } else {
                (q << k) | r.take(k)
            };
            let x = unzigzag(u) + predict(order, h);
            h = [x, h[0], h[1]];
            out.push(x as i16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(samples: &[i16]) -> usize {
        let (mut code, mut scratch, mut back) = (Vec::new(), Vec::new(), Vec::new());
        encode(samples, &mut code, &mut scratch);
        decode(&code, samples.len(), &mut back);
        assert_eq!(back, samples);
        assert!(code.len() <= 1 + samples.len() * 2);
        code.len()
    }

    /// A deterministic noise source.
    fn noise(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }

    #[test]
    fn every_kind_of_line_comes_back_exactly() {
        let mut seed = 0x9e37_79b9_7f4a_7c15;
        let lines: Vec<Vec<i16>> = vec![
            vec![],
            vec![0; 4_410],
            vec![i16::MIN; 4_410],
            vec![i16::MAX; 4_410],
            // Full-scale square waves: the largest residuals of every order.
            (0..4_410)
                .map(|i| if i % 2 == 0 { i16::MAX } else { i16::MIN })
                .collect(),
            (0..4_410)
                .map(|i| if i % 3 == 0 { i16::MIN } else { i16::MAX })
                .collect(),
            (0..4_410).map(|_| noise(&mut seed) as i16).collect(),
            (0..4_410)
                .map(|_| (noise(&mut seed) % 64) as i16 - 32)
                .collect(),
            (0..4_410)
                .map(|i| ((i as f32 * 0.05).sin() * 20_000.0) as i16)
                .collect(),
            // Quiet with a click: an escape in a small parameter.
            (0..4_410)
                .map(|i| if i == 1_000 { i16::MIN } else { (i % 3) as i16 })
                .collect(),
            ((0..4_410).map(|i| ((i * 7) % 65_536) as u16 as i16)).collect(),
        ];
        for line in &lines {
            round_trip(line);
        }
        // Every length a line's end can have.
        for n in 0..=600 {
            let line: Vec<i16> = (0..n).map(|_| (noise(&mut seed) % 2_000) as i16).collect();
            round_trip(&line);
        }
    }

    #[test]
    fn smooth_audio_shrinks() {
        let line: Vec<i16> = (0..4_410)
            .map(|i| ((i as f32 * 0.03).sin() * 8_000.0 + (i as f32 * 0.31).sin() * 300.0) as i16)
            .collect();
        assert!(round_trip(&line) < 4_410);
        // Noise of full scale stays as its samples.
        let mut seed = 1;
        let loud: Vec<i16> = (0..4_410).map(|_| noise(&mut seed) as i16).collect();
        assert_eq!(round_trip(&loud), 1 + 4_410 * 2);
    }
}
