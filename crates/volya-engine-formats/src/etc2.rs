// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! ETC2 and EAC texture blocks (Khronos Data Format Specification 1.3,
//! "ETC2 Compressed Texture Image Formats"; the formats of OpenGL ES 3.0 and
//! WebGL's `WEBGL_compressed_texture_etc`): a fast encoder, used to transcode
//! the game's BC textures for GPUs without BC (#113).
//!
//! **Colour (ETC2 RGB8, and the colour of RGBA8 and RGB8A1).** Each 4×4
//! block is tried in the ETC1 modes (individual and differential, both
//! flips: each half's mean as its base colour, the best of the eight
//! modifier tables); unless that is already within a small error, in the
//! planar mode (a least-squares plane) and in the T and H modes (two colour
//! clusters split at the block's two farthest colours, each distance
//! tried). The smallest squared error wins. Errors are counted over the
//! block's distinct colours (a BC block has at most four), so each candidate
//! costs a few dozen distance evaluations.
//!
//! **Punch-through alpha (ETC2 RGB8A1).** BC1's cut-outs (its transparent
//! texels) keep 4 bits per texel: a block with transparent texels is
//! encoded in the non-opaque differential, T or H mode with them on the
//! transparent index; an opaque block in the differential, planar, T or H
//! mode (RGB8A1 has no individual mode).
//!
//! **Alpha (EAC, the alpha of ETC2 RGBA8).** For each of the sixteen
//! modifier tables, the multiplier and base that stretch it over the
//! block's range; then the best table's neighbouring multipliers and bases.
//! A constant block is exact.
//!
//! Texels are given in row order (`y * 4 + x`), as the DDS block decoders
//! give them; ETC orders a block's texel indices by columns.

use crate::dds::Format;

/// ETC1 intensity modifier tables: index 0 and 1 add `a` and `b`, 2 and 3
/// subtract them. In RGB8A1's non-opaque mode `a` is 0 and index 2 is
/// transparent.
const ETC1_TABLES: [[i32; 2]; 8] = [
    [2, 8],
    [5, 17],
    [9, 29],
    [13, 42],
    [18, 60],
    [24, 80],
    [33, 106],
    [47, 183],
];

/// T and H mode distances.
const DISTANCES: [i32; 8] = [3, 6, 11, 16, 23, 32, 41, 64];

/// EAC modifier tables.
const EAC_TABLES: [[i32; 8]; 16] = [
    [-3, -6, -9, -15, 2, 5, 8, 14],
    [-3, -7, -10, -13, 2, 6, 9, 12],
    [-2, -5, -8, -13, 1, 4, 7, 12],
    [-2, -4, -6, -13, 1, 3, 5, 12],
    [-3, -6, -8, -12, 2, 5, 7, 11],
    [-3, -7, -9, -11, 2, 6, 8, 10],
    [-4, -7, -8, -11, 3, 6, 7, 10],
    [-3, -5, -8, -11, 2, 4, 7, 10],
    [-2, -6, -8, -10, 1, 5, 7, 9],
    [-2, -5, -8, -10, 1, 4, 7, 9],
    [-2, -4, -8, -10, 1, 3, 7, 9],
    [-2, -5, -7, -10, 1, 4, 6, 9],
    [-3, -4, -7, -10, 2, 3, 6, 9],
    [-1, -2, -3, -10, 0, 1, 2, 9],
    [-4, -6, -8, -9, 3, 5, 7, 8],
    [-3, -5, -7, -9, 2, 4, 6, 8],
];

/// Squared error per block up to which the ETC1 modes are kept without
/// trying the others: one level per channel on average.
const GOOD_ENOUGH: u64 = 16 * 3;

/// Squared error per block above which the T and H modes are tried too:
/// three levels per channel on average.
const TRY_T_AND_H: u64 = 16 * 3 * 9;

/// Which ETC2 format a transcoded texture takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Etc2Format {
    /// ETC2 RGB8: 8 bytes per block, opaque.
    Rgb8,
    /// ETC2 RGB8A1: 8 bytes per block, each texel opaque or transparent
    /// black (BC1's cut-outs).
    Rgb8A1,
    /// ETC2 RGBA8: an EAC alpha block (8 bytes) then an ETC2 colour block.
    Rgba8,
}

impl Etc2Format {
    /// Bytes per 4×4 block.
    pub fn block_bytes(self) -> usize {
        match self {
            Self::Rgb8 | Self::Rgb8A1 => 8,
            Self::Rgba8 => 16,
        }
    }

    /// The format for BC texture data of `format`: RGBA8 for BC2 and BC3;
    /// for BC1, RGB8A1 when any of `blocks` (whole BC1 blocks, any number
    /// of images) has a transparent texel, RGB8 otherwise. `None` for an
    /// uncompressed format.
    pub fn for_bc(format: Format, blocks: &[u8]) -> Option<Self> {
        match format {
            Format::Bc1 => Some(
                if blocks
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .any(|b| format.block_has_transparency(b))
                {
                    Self::Rgb8A1
                } else {
                    Self::Rgb8
                },
            ),
            Format::Bc2 | Format::Bc3 => Some(Self::Rgba8),
            Format::Masked(_) => None,
        }
    }
}

/// Transcodes one image of BC blocks (`format`, as stored: rows of blocks)
/// to `target`, block for block.
pub fn transcode_bc(format: Format, data: &[u8], target: Etc2Format) -> Vec<u8> {
    let Some(block) = format.block_bytes() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(data.len() / block * target.block_bytes());
    for bytes in data.chunks_exact(block) {
        let texels = format.decode_block(bytes);
        match target {
            Etc2Format::Rgb8 => out.extend_from_slice(&encode_rgb(&texels)),
            Etc2Format::Rgb8A1 => out.extend_from_slice(&encode_rgb_a1(&texels)),
            Etc2Format::Rgba8 => {
                let alpha: [u8; 16] = std::array::from_fn(|i| texels[i][3]);
                out.extend_from_slice(&encode_alpha(&alpha));
                out.extend_from_slice(&encode_rgb(&texels));
            }
        }
    }
    out
}

type Rgb = [i32; 3];

fn distance(a: Rgb, b: Rgb) -> u32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) as u32
}

fn clamp(v: i32) -> i32 {
    v.clamp(0, 255)
}

fn extend4(v: i32) -> i32 {
    v * 17
}

fn extend5(v: i32) -> i32 {
    (v << 3) | (v >> 2)
}

fn extend6(v: i32) -> i32 {
    (v << 2) | (v >> 4)
}

fn extend7(v: i32) -> i32 {
    (v << 1) | (v >> 6)
}

/// `v` (0 … 255) quantized to `bits` bits, rounded.
fn quantize(v: i32, bits: u32) -> i32 {
    let max = (1 << bits) - 1;
    ((v * max + 127) / 255).clamp(0, max)
}

/// A texel with no colour to fit: transparent.
const CLEAR: u8 = u8::MAX;

/// The texels to fit, as indices into their distinct colours.
struct Block {
    colours: [Rgb; 16],
    distinct: usize,
    /// Each texel's colour, or [`CLEAR`].
    texels: [u8; 16],
    /// Whether clear texels must take the transparent index 2 (RGB8A1's
    /// non-opaque mode); otherwise they take any index.
    punch: bool,
}

impl Block {
    /// `skip` texels are left unfitted (transparent ones).
    fn new(texels: &[[u8; 4]; 16], skip: impl Fn(&[u8; 4]) -> bool, punch: bool) -> Self {
        let mut colours = [[0; 3]; 16];
        let mut distinct = 0;
        let mut index = [CLEAR; 16];
        for (i, t) in texels.iter().enumerate() {
            if skip(t) {
                continue;
            }
            let c = [i32::from(t[0]), i32::from(t[1]), i32::from(t[2])];
            let k = match colours[..distinct].iter().position(|&d| d == c) {
                Some(k) => k,
                None => {
                    colours[distinct] = c;
                    distinct += 1;
                    distinct - 1
                }
            };
            index[i] = k as u8;
        }
        Self {
            colours,
            distinct,
            texels: index,
            punch,
        }
    }

    /// Each distinct colour's best of `palette` (`None` entries are
    /// transparent and never chosen for a colour): its index and error.
    fn nearest(&self, palette: &[Option<Rgb>; 4]) -> [(u32, u32); 16] {
        let mut best = [(0, u32::MAX); 16];
        for (u, b) in best.iter_mut().enumerate().take(self.distinct) {
            for (k, p) in palette.iter().enumerate() {
                if let Some(p) = p {
                    let e = distance(self.colours[u], *p);
                    if e < b.1 {
                        *b = (k as u32, e);
                    }
                }
            }
        }
        best
    }

    /// Texel `i`'s index and error given `nearest`'s result.
    fn pick(&self, i: usize, nearest: &[(u32, u32); 16]) -> (u32, u64) {
        match self.texels[i] {
            CLEAR => (if self.punch { 2 } else { 0 }, 0),
            u => {
                let (k, e) = nearest[u as usize];
                (k, u64::from(e))
            }
        }
    }

    /// Indices and total error of `palette` over every texel.
    fn fit(&self, palette: &[Option<Rgb>; 4]) -> (u64, [u32; 16]) {
        let nearest = self.nearest(palette);
        let mut indices = [0u32; 16];
        let mut error = 0u64;
        for (i, index) in indices.iter_mut().enumerate() {
            let (k, e) = self.pick(i, &nearest);
            *index = k;
            error = error.saturating_add(e);
        }
        (error, indices)
    }

    fn has_clear(&self) -> bool {
        self.texels.contains(&CLEAR)
    }
}

/// A candidate encoding: its error and its 64 bits.
#[derive(Clone, Copy)]
struct Candidate {
    error: u64,
    bits: u64,
}

impl Candidate {
    const NONE: Self = Self {
        error: u64::MAX,
        bits: 0,
    };

    fn keep_better(&mut self, other: Self) {
        if other.error < self.error {
            *self = other;
        }
    }
}

/// The column-major position of row-major texel `i`.
fn column_major(i: usize) -> usize {
    (i % 4) * 4 + i / 4
}

/// The 32 index bits of per-texel 2-bit indices (row-major `indices`).
fn index_bits(indices: &[u32; 16]) -> u64 {
    let mut msb = 0u64;
    let mut lsb = 0u64;
    for (i, &k) in indices.iter().enumerate() {
        let j = column_major(i);
        msb |= u64::from(k >> 1) << j;
        lsb |= u64::from(k & 1) << j;
    }
    (msb << 16) | lsb
}

/// Which modes a block may use.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Modes {
    /// ETC2 RGB8 (and RGBA8's colour): every mode.
    All,
    /// RGB8A1, opaque block: no individual mode.
    Opaque,
    /// RGB8A1 with transparent texels: differential, T and H, non-opaque.
    Punch,
}

/// Encodes a 4×4 block of RGBA texels (row order) as an ETC2 RGB8 block
/// (also RGBA8's colour block). Every texel's colour is fitted, whatever
/// its alpha: the game's alpha is often data (light maps keep a lighting
/// term there, 0 in places), not coverage.
pub fn encode_rgb(texels: &[[u8; 4]; 16]) -> [u8; 8] {
    let block = Block::new(texels, |_| false, false);
    encode(&block, Modes::All)
}

/// Encodes a 4×4 block of RGBA texels (row order) as an ETC2 RGB8A1 block:
/// texels whose alpha is below 128 transparent, the others opaque.
pub fn encode_rgb_a1(texels: &[[u8; 4]; 16]) -> [u8; 8] {
    let block = Block::new(texels, |t| t[3] < 128, true);
    let modes = if block.has_clear() {
        Modes::Punch
    } else {
        Modes::Opaque
    };
    encode(&block, modes)
}

fn encode(block: &Block, modes: Modes) -> [u8; 8] {
    let mut best = etc1(block, modes);
    if best.error > GOOD_ENOUGH && modes != Modes::Punch {
        best.keep_better(planar(block));
    }
    if best.error > TRY_T_AND_H {
        best.keep_better(t_and_h(block));
    }
    // RGB8A1's opaque flag sits where the differential bit does: clear for
    // a block with transparent texels.
    if modes == Modes::Punch {
        best.bits &= !(1 << 33);
    }
    best.bits.to_be_bytes()
}

/// The texels of half `half` (0: left or top) of a block split by `flip`.
fn in_half(i: usize, flip: bool, half: usize) -> bool {
    let (x, y) = (i % 4, i / 4);
    let coordinate = if flip { y } else { x };
    usize::from(coordinate >= 2) == half
}

/// The best ETC1-mode encoding (individual or differential, either flip).
fn etc1(block: &Block, modes: Modes) -> Candidate {
    let mut best = Candidate::NONE;
    for flip in [false, true] {
        let means: [Rgb; 2] = std::array::from_fn(|half| {
            let mut sum = [0i32; 3];
            let mut weight = 0;
            for i in (0..16).filter(|&i| in_half(i, flip, half)) {
                if let Some(c) = block.colours.get(block.texels[i] as usize) {
                    for ch in 0..3 {
                        sum[ch] += c[ch];
                    }
                    weight += 1;
                }
            }
            sum.map(|s| {
                if weight == 0 {
                    0
                } else {
                    (s + weight / 2) / weight
                }
            })
        });
        // Differential: 5-bit bases, the second within −4 … 3 of the first
        // (clamped there when the means are further apart).
        let q1 = means[0].map(|v| quantize(v, 5));
        let wanted = means[1].map(|v| quantize(v, 5));
        let q2: Rgb =
            std::array::from_fn(|c| wanted[c].clamp((q1[c] - 4).max(0), (q1[c] + 3).min(31)));
        let punch = modes == Modes::Punch;
        let differential = half_pair(block, flip, [q1.map(extend5), q2.map(extend5)], punch);
        let mut bits = 0u64;
        for c in 0..3 {
            let delta = (q2[c] - q1[c]) & 7;
            bits |= ((q1[c] as u64) << 3 | delta as u64) << (56 - 8 * c);
        }
        best.keep_better(pack_etc1(bits, true, flip, differential));
        // Individual: 4-bit bases, for halves too far apart for the
        // differential mode.
        if modes == Modes::All && q2 != wanted {
            let b1 = means[0].map(|v| quantize(v, 4));
            let b2 = means[1].map(|v| quantize(v, 4));
            let individual = half_pair(block, flip, [b1.map(extend4), b2.map(extend4)], false);
            let mut bits = 0u64;
            for c in 0..3 {
                bits |= ((b1[c] as u64) << 4 | b2[c] as u64) << (56 - 8 * c);
            }
            best.keep_better(pack_etc1(bits, false, flip, individual));
        }
    }
    best
}

/// The palette of ETC1 table `[a, b]` around `base`; in RGB8A1's
/// non-opaque mode `a` is 0 and index 2 transparent.
fn etc1_palette(base: Rgb, [a, b]: [i32; 2], punch: bool) -> [Option<Rgb>; 4] {
    let at = |m: i32| Some(base.map(|v| clamp(v + m)));
    if punch {
        [at(0), at(b), None, at(-b)]
    } else {
        [at(a), at(b), at(-a), at(-b)]
    }
}

/// Each half's best table with `bases`: (error, tables, indices).
fn half_pair(
    block: &Block,
    flip: bool,
    bases: [Rgb; 2],
    punch: bool,
) -> (u64, [u32; 2], [u32; 16]) {
    let mut indices = [0u32; 16];
    let mut tables = [0u32; 2];
    let mut total = 0u64;
    for (half, &base) in bases.iter().enumerate() {
        // The half's distinct colours with their counts.
        let mut counts = [0u64; 16];
        for i in (0..16).filter(|&i| in_half(i, flip, half)) {
            if let Some(n) = counts.get_mut(block.texels[i] as usize) {
                *n += 1;
            }
        }
        let mut best = (u64::MAX, 0usize);
        for (t, &table) in ETC1_TABLES.iter().enumerate() {
            let palette = etc1_palette(base, table, punch);
            let mut error = 0u64;
            for u in (0..block.distinct).filter(|&u| counts[u] > 0) {
                let e = palette
                    .iter()
                    .flatten()
                    .map(|&p| distance(block.colours[u], p))
                    .min()
                    .unwrap_or(u32::MAX);
                error = error.saturating_add(u64::from(e) * counts[u]);
            }
            if error < best.0 {
                best = (error, t);
            }
        }
        let nearest = block.nearest(&etc1_palette(base, ETC1_TABLES[best.1], punch));
        for i in (0..16).filter(|&i| in_half(i, flip, half)) {
            indices[i] = block.pick(i, &nearest).0;
        }
        total = total.saturating_add(best.0);
        tables[half] = best.1 as u32;
    }
    (total, tables, indices)
}

fn pack_etc1(
    colour_bits: u64,
    differential: bool,
    flip: bool,
    (error, tables, indices): (u64, [u32; 2], [u32; 16]),
) -> Candidate {
    let bits = colour_bits
        | u64::from(tables[0]) << 37
        | u64::from(tables[1]) << 34
        | u64::from(differential) << 33
        | u64::from(flip) << 32
        | index_bits(&indices);
    Candidate { error, bits }
}

/// The free bits that make a 5-bit base (top three free, low two `high`)
/// plus a 3-bit delta (sign free, low two `low`) leave 0 … 31, selecting
/// T, H or planar mode: (the three top bits, the sign bit).
fn overflow_bits(high: u64, low: u64) -> (u64, u64) {
    if high + low > 3 { (0b111, 0) } else { (0, 1) }
}

/// The planar-mode encoding: a least-squares plane through the block.
fn planar(block: &Block) -> Candidate {
    // c(x, y) = o + (h − o)·x/4 + (v − o)·y/4 = a + b·x + c·y, fitted over
    // every texel (clear ones too: planar is opaque).
    let colour = |i: usize| {
        block
            .colours
            .get(block.texels[i] as usize)
            .copied()
            .unwrap_or_default()
    };
    let mut channels = [[0i32; 3]; 3];
    for (c, channel) in channels.iter_mut().enumerate() {
        let (mut mean, mut sx, mut sy) = (0f32, 0f32, 0f32);
        for i in 0..16 {
            let v = colour(i)[c] as f32;
            let (x, y) = ((i % 4) as f32 - 1.5, (i / 4) as f32 - 1.5);
            mean += v;
            sx += x * v;
            sy += y * v;
        }
        mean /= 16.0;
        // Σ(x − 1.5)² over the block is 20.
        let (b, cy) = (sx / 20.0, sy / 20.0);
        let a = mean - 1.5 * b - 1.5 * cy;
        let bits = if c == 1 { 7 } else { 6 };
        let q = |v: f32| quantize(v.round().clamp(0.0, 255.0) as i32, bits);
        *channel = [q(a), q(a + 4.0 * b), q(a + 4.0 * cy)];
    }
    let extend = |c: usize, v: i32| if c == 1 { extend7(v) } else { extend6(v) };
    let mut error = 0u64;
    for (i, &texel) in block.texels.iter().enumerate() {
        if texel == CLEAR {
            continue;
        }
        let (x, y) = ((i % 4) as i32, (i / 4) as i32);
        let decoded: Rgb = std::array::from_fn(|c| {
            let [o, h, v] = channels[c].map(|q| extend(c, q));
            clamp((x * (h - o) + y * (v - o) + 4 * o + 2) >> 2)
        });
        error += u64::from(distance(colour(i), decoded));
    }
    let [[ro, rh, rv], [go, gh, gv], [bo, bh, bv]] = channels.map(|c| c.map(|v| v as u64));
    let mut bits = ro << 57
        | (go >> 6) << 56
        | (go & 63) << 49
        | (bo >> 5) << 48
        | ((bo >> 3) & 3) << 43
        | (bo & 7) << 39
        | (rh >> 1) << 34
        | 1 << 33
        | (rh & 1) << 32
        | gh << 25
        | bh << 19
        | rv << 13
        | gv << 6
        | bv;
    // R and G within range, B out of it: planar.
    bits |= ((bits >> 58) & 1) << 63;
    bits |= ((bits >> 50) & 1) << 55;
    let (top, sign) = overflow_bits((bits >> 43) & 3, (bits >> 40) & 3);
    bits |= top << 45 | sign << 42;
    Candidate { error, bits }
}

/// The best T- or H-mode encoding (in RGB8A1's non-opaque mode, index 2 is
/// transparent).
fn t_and_h(block: &Block) -> Candidate {
    let n = block.distinct;
    if n == 0 {
        return Candidate::NONE;
    }
    // The block's two farthest colours seed two clusters.
    let mut far = (0, 0, 0);
    for i in 0..n {
        for j in i + 1..n {
            let d = distance(block.colours[i], block.colours[j]);
            if d > far.2 {
                far = (i, j, d);
            }
        }
    }
    let (seed0, seed1) = (block.colours[far.0], block.colours[far.1]);
    let mut counts = [0i32; 16];
    for &u in block.texels.iter().filter(|&&u| u != CLEAR) {
        counts[u as usize] += 1;
    }
    let mut sums = [[0i32; 3]; 2];
    let mut weights = [0i32; 2];
    for u in 0..n {
        let c = block.colours[u];
        let k = usize::from(distance(c, seed1) < distance(c, seed0));
        for ch in 0..3 {
            sums[k][ch] += c[ch] * counts[u];
        }
        weights[k] += counts[u];
    }
    let means: [Rgb; 2] = std::array::from_fn(|k| {
        let w = weights[k].max(1);
        sums[k].map(|s| (s + w / 2) / w)
    });
    let q = means.map(|m| m.map(|v| quantize(v, 4)));
    // The distance nearest the clusters' mean spread (mean absolute
    // channel deviation from their means), and those either side.
    let mut spread = 0i32;
    for u in 0..n {
        let c = block.colours[u];
        let k = usize::from(distance(c, seed1) < distance(c, seed0));
        spread += (0..3).map(|ch| (c[ch] - means[k][ch]).abs()).sum::<i32>() * counts[u];
    }
    let spread = spread / (3 * weights[0] + 3 * weights[1]).max(1);
    let centre = (0..DISTANCES.len())
        .min_by_key(|&i| (DISTANCES[i] - spread).abs())
        .unwrap_or(0);
    let punch = block.punch;
    let mut best = Candidate::NONE;
    let tried = centre.saturating_sub(1)..(centre + 2).min(DISTANCES.len());
    for (d_index, &d) in DISTANCES
        .iter()
        .enumerate()
        .skip(tried.start)
        .take(tried.len())
    {
        let plus = |c: Rgb| Some(c.map(|v| clamp(v + d)));
        let minus = |c: Rgb| Some(c.map(|v| clamp(v - d)));
        // T: one colour alone, the other with ± d; either cluster alone.
        for (single, pair) in [(q[0], q[1]), (q[1], q[0])] {
            let (s, p) = (single.map(extend4), pair.map(extend4));
            let palette = [Some(s), plus(p), (!punch).then_some(p), minus(p)];
            let (error, indices) = block.fit(&palette);
            best.keep_better(pack_t(error, &indices, single, pair, d_index as u64));
        }
        // H: both with ± d; the distance's low bit is the colours' order.
        let value = |c: Rgb| (c[0] << 8) | (c[1] << 4) | c[2];
        let (a, b) = if (value(q[0]) >= value(q[1])) == (d_index & 1 == 1) {
            (q[0], q[1])
        } else {
            (q[1], q[0])
        };
        if value(a) == value(b) && d_index & 1 == 0 {
            continue;
        }
        let (ea, eb) = (a.map(extend4), b.map(extend4));
        let palette = [
            plus(ea),
            minus(ea),
            if punch { None } else { plus(eb) },
            minus(eb),
        ];
        let (error, indices) = block.fit(&palette);
        best.keep_better(pack_h(error, &indices, a, b, d_index as u64));
    }
    best
}

fn pack_t(error: u64, indices: &[u32; 16], c1: Rgb, c2: Rgb, d: u64) -> Candidate {
    let [r1, g1, b1] = c1.map(|v| v as u64);
    let [r2, g2, b2] = c2.map(|v| v as u64);
    let mut bits = (r1 >> 2) << 59
        | (r1 & 3) << 56
        | g1 << 52
        | b1 << 48
        | r2 << 44
        | g2 << 40
        | b2 << 36
        | (d >> 1) << 34
        | 1 << 33
        | (d & 1) << 32
        | index_bits(indices);
    // R out of range: T.
    let (top, sign) = overflow_bits(r1 >> 2, r1 & 3);
    bits |= top << 61 | sign << 58;
    Candidate { error, bits }
}

fn pack_h(error: u64, indices: &[u32; 16], c1: Rgb, c2: Rgb, d: u64) -> Candidate {
    let [r1, g1, b1] = c1.map(|v| v as u64);
    let [r2, g2, b2] = c2.map(|v| v as u64);
    let mut bits = r1 << 59
        | (g1 >> 1) << 56
        | (g1 & 1) << 52
        | (b1 >> 3) << 51
        | (b1 & 7) << 47
        | r2 << 43
        | g2 << 39
        | b2 << 35
        | (d >> 2) << 34
        | 1 << 33
        | ((d >> 1) & 1) << 32
        | index_bits(indices);
    // R within range (its delta's sign repeated on top), G out of it: H.
    bits |= ((bits >> 58) & 1) << 63;
    let (top, sign) = overflow_bits((bits >> 51) & 3, (bits >> 48) & 3);
    bits |= top << 53 | sign << 50;
    Candidate { error, bits }
}

/// Encodes 16 alpha values (row order) as an EAC block (the alpha half of
/// ETC2 RGBA8).
pub fn encode_alpha(alpha: &[u8; 16]) -> [u8; 8] {
    // Distinct values with their counts (a BC3 block has at most eight).
    let mut values = [(0i32, 0u64); 16];
    let mut distinct = 0;
    for &a in alpha {
        let a = i32::from(a);
        match values[..distinct].iter_mut().find(|(v, _)| *v == a) {
            Some((_, n)) => *n += 1,
            None => {
                values[distinct] = (a, 1);
                distinct += 1;
            }
        }
    }
    let values = &values[..distinct];
    let min = values.iter().map(|v| v.0).min().unwrap_or(0);
    let max = values.iter().map(|v| v.0).max().unwrap_or(0);
    if min == max {
        // Table 13, modifier index 4 (0): every texel the base.
        return pack_alpha(min as u64, 1, 13, Some(&[4; 16]), alpha);
    }
    let error = |base: i32, multiplier: i32, table: &[i32; 8]| -> u64 {
        values
            .iter()
            .map(|&(v, n)| {
                let e = table
                    .iter()
                    .map(|&m| (v - clamp(base + m * multiplier)).abs())
                    .min()
                    .unwrap_or(0);
                (e * e) as u64 * n
            })
            .sum()
    };
    // (error, base, multiplier, table)
    let mut best = (u64::MAX, 0, 1, 0);
    for (t, table) in EAC_TABLES.iter().enumerate() {
        let span = table[7] - table[3];
        let multiplier = ((max - min + span / 2) / span).clamp(1, 15);
        let base = clamp((min + max - (table[3] + table[7]) * multiplier + 1) / 2);
        let e = error(base, multiplier, table);
        if e < best.0 {
            best = (e, base, multiplier, t);
        }
    }
    let (_, centre, m0, t) = best;
    for multiplier in (m0 - 1).max(1)..=(m0 + 1).min(15) {
        for base in (centre - 2).max(0)..=(centre + 2).min(255) {
            let e = error(base, multiplier, &EAC_TABLES[t]);
            if e < best.0 {
                best = (e, base, multiplier, t);
            }
        }
    }
    let (_, base, multiplier, t) = best;
    pack_alpha(base as u64, multiplier as u64, t as u64, None, alpha)
}

/// Packs an EAC block: `indices` as given, or else each texel's nearest of
/// the table's values.
fn pack_alpha(
    base: u64,
    multiplier: u64,
    table: u64,
    indices: Option<&[u32; 16]>,
    alpha: &[u8; 16],
) -> [u8; 8] {
    let mut bits = base << 56 | multiplier << 52 | table << 48;
    let modifiers = EAC_TABLES[table as usize];
    for (i, &a) in alpha.iter().enumerate() {
        let k = if let Some(indices) = indices {
            indices[i]
        } else {
            let value = |m: i32| clamp(base as i32 + m * multiplier as i32);
            (0..8)
                .min_by_key(|&k| (i32::from(a) - value(modifiers[k])).abs())
                .unwrap_or(0) as u32
        };
        bits |= u64::from(k) << (45 - 3 * column_major(i));
    }
    bits.to_be_bytes()
}

#[cfg(any(test, feature = "etc2-decode"))]
pub mod decode;
#[cfg(test)]
mod tests;
