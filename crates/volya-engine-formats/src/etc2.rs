// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! ETC2 and EAC texture blocks (Khronos Data Format Specification 1.3,
//! "ETC2 Compressed Texture Image Formats"; the formats of OpenGL ES 3.0 and
//! WebGL's `WEBGL_compressed_texture_etc`): a fast encoder, used to transcode
//! the game's BC textures for GPUs without BC (#113, #183).
//!
//! **Colour (ETC2 RGB8, and the colour of RGBA8 and RGB8A1).** A BC block
//! is taken as its four colours and its texels' indices into them, never
//! decoded texel by texel; errors are counted per colour. Each 4×4 block is
//! tried in the ETC1 modes (individual and differential, both flips: each
//! half's mean as its base colour). An ETC1 modifier moves all three
//! channels alike, so a table's error follows from each colour's distance
//! to the base along the grey axis alone (the idea of fast encoders such as
//! etcpak, which search tables on luma differences): every table is judged
//! that way, and only where entries clamp are the best two fitted entry by
//! entry. Unless that is already within a small error, the planar mode (a
//! least-squares plane) is tried where the best plane before quantization
//! could beat it, and the T and H modes (two colour clusters split at the
//! block's two farthest colours) where the error is large and, below a
//! larger one, the colours differ in hue or saturation (which ETC1 can't
//! span within a half): their candidates are judged along the grey axis
//! too, from the distance index nearest the clusters' spread outwards
//! while that improves (the errors are mostly V-shaped over the distances,
//! as QuickETC2, Nah 2020, observes). The smallest squared error wins. A
//! block identical to the one before it is encoded alike.
//!
//! **Punch-through alpha (ETC2 RGB8A1).** BC1's cut-outs (its transparent
//! texels) keep 4 bits per texel: a block with transparent texels is
//! encoded in the non-opaque differential, T or H mode with them on the
//! transparent index; an opaque block in the differential, planar, T or H
//! mode (RGB8A1 has no individual mode).
//!
//! **Alpha (EAC, the alpha of ETC2 RGBA8).** For each of the sixteen
//! modifier tables, the multiplier and base that stretch it over the
//! block's range; then, unless exact, the best table's neighbouring
//! multipliers and bases. A constant block is exact.
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

/// Squared error per block above which the T and H modes are tried
/// whatever the block's colours.
const T_AND_H_ANYWAY: u64 = 16 * 3 * 25;

/// The cross-grey spread of a block's colours from which the T and H modes
/// are tried above [`TRY_T_AND_H`] (see `Block::cross_grey_spread`).
const T_AND_H_SPREAD: i32 = 200;

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
    let size = target.block_bytes();
    let mut out = Vec::with_capacity(data.len() / block * size);
    // A block like the one before it (flat areas) is encoded alike.
    let mut previous: Option<&[u8]> = None;
    for bytes in data.chunks_exact(block) {
        if previous == Some(bytes) {
            out.extend_from_within(out.len() - size..);
            continue;
        }
        previous = Some(bytes);
        let Some((palette, indices)) = format.colour_block(bytes) else {
            return Vec::new();
        };
        match target {
            Etc2Format::Rgb8 => {
                let block = Block::from_palette(&palette, indices, |_| false, false);
                out.extend_from_slice(&encode(&block, Modes::All));
            }
            Etc2Format::Rgb8A1 => {
                let block = Block::from_palette(&palette, indices, |t| t[3] < 128, true);
                out.extend_from_slice(&encode_a1(&block));
            }
            Etc2Format::Rgba8 => {
                let alpha = format.block_alpha(bytes).unwrap_or([255; 16]);
                out.extend_from_slice(&encode_alpha(&alpha));
                let block = Block::from_palette(&palette, indices, |_| false, false);
                out.extend_from_slice(&encode(&block, Modes::All));
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
    /// How many texels of each half take each colour: by flip (0: halves
    /// side by side, 1: one above the other), then half (0: left or top).
    halves: [[[u8; 16]; 2]; 2],
    /// How many texels take each colour.
    counts: [u8; 16],
    /// Σ(2x − 3) and Σ(2y − 3) over the texels of each colour (`x`, `y`
    /// their column and row).
    moments: [(i8, i8); 16],
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
        Self::with_texels(colours, distinct, index, punch)
    }

    /// A BC block's colour part: its four colours (some perhaps unused or
    /// alike) and texel `i`'s index into them at bit `2 i` of `indices`;
    /// texels whose colour `skip` takes are left unfitted.
    fn from_palette(
        palette: &[[u8; 4]; 4],
        indices: u32,
        skip: impl Fn(&[u8; 4]) -> bool,
        punch: bool,
    ) -> Self {
        let mut colours = [[0; 3]; 16];
        for (c, p) in colours.iter_mut().zip(palette) {
            *c = [i32::from(p[0]), i32::from(p[1]), i32::from(p[2])];
        }
        let skipped = palette.each_ref().map(skip);
        let texels = std::array::from_fn(|i| {
            let k = (indices >> (2 * i)) as usize & 3;
            if skipped[k] { CLEAR } else { k as u8 }
        });
        Self::with_texels(colours, 4, texels, punch)
    }

    fn with_texels(colours: [Rgb; 16], distinct: usize, index: [u8; 16], punch: bool) -> Self {
        let mut halves = [[[0u8; 16]; 2]; 2];
        for (i, &k) in index.iter().enumerate() {
            if k != CLEAR {
                halves[0][(i % 4) / 2][k as usize] += 1;
                halves[1][i / 8][k as usize] += 1;
            }
        }
        let counts = std::array::from_fn(|u| halves[0][0][u] + halves[0][1][u]);
        let mut moments = [(0i8, 0i8); 16];
        for (i, &k) in index.iter().enumerate() {
            if let Some(m) = moments.get_mut(k as usize) {
                m.0 += 2 * (i % 4) as i8 - 3;
                m.1 += 2 * (i / 4) as i8 - 3;
            }
        }
        Self {
            colours,
            distinct,
            texels: index,
            halves,
            counts,
            moments,
            punch,
        }
    }

    /// The mean colour of the texels `counts` counts (rounded; black for
    /// none).
    fn mean(&self, counts: &[u8; 16]) -> Rgb {
        let mut sum = [0i32; 3];
        let mut weight = 0;
        for (c, &n) in self.colours[..self.distinct].iter().zip(counts) {
            let n = i32::from(n);
            for ch in 0..3 {
                sum[ch] += c[ch] * n;
            }
            weight += n;
        }
        if weight == 0 {
            return [0; 3];
        }
        sum.map(|s| (s + weight / 2) / weight)
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

    /// The total error of `palette` over every texel (its nearest entry
    /// for each).
    fn error(&self, palette: &[Option<Rgb>; 4]) -> u64 {
        let mut total = 0u64;
        for (c, &k) in self.colours[..self.distinct].iter().zip(&self.counts) {
            if k == 0 {
                continue;
            }
            let mut least = u32::MAX;
            for p in palette.iter().flatten() {
                least = least.min(distance(*c, *p));
            }
            total += u64::from(least) * u64::from(k);
        }
        total
    }

    /// The largest squared distance across the grey axis between two of the
    /// block's colours: how far apart their hues and saturations are, which
    /// the ETC1 modes (one grey-axis line per half) can't span within a half.
    fn cross_grey_spread(&self) -> i32 {
        let mut spread = 0;
        for i in (0..self.distinct).filter(|&i| self.counts[i] > 0) {
            for j in (i + 1..self.distinct).filter(|&j| self.counts[j] > 0) {
                let (a, b) = (self.colours[i], self.colours[j]);
                let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
                let g = d[0] + d[1] + d[2];
                // 3 |d|² − (Σd)², three times the squared distance.
                spread = spread.max(3 * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) - g * g);
            }
        }
        spread / 3
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
    encode_a1(&Block::new(texels, |t| t[3] < 128, true))
}

fn encode_a1(block: &Block) -> [u8; 8] {
    let modes = if block.has_clear() {
        Modes::Punch
    } else {
        Modes::Opaque
    };
    encode(block, modes)
}

fn encode(block: &Block, modes: Modes) -> [u8; 8] {
    let mut best = etc1(block, modes);
    if best.error > GOOD_ENOUGH && modes != Modes::Punch {
        // Only where the best plane, unquantized, could do better.
        let sums = plane_sums(block);
        if planar_bound(&sums) < best.error as f32 {
            best.keep_better(planar(block, &sums));
        }
    }
    if best.error > TRY_T_AND_H
        && (best.error > T_AND_H_ANYWAY || block.cross_grey_spread() >= T_AND_H_SPREAD)
    {
        best.keep_better(t_and_h(block));
    }
    // RGB8A1's opaque flag sits where the differential bit does: clear for
    // a block with transparent texels.
    if modes == Modes::Punch {
        best.bits &= !(1 << 33);
    }
    best.bits.to_be_bytes()
}

/// The best ETC1-mode encoding (individual or differential, either flip).
fn etc1(block: &Block, modes: Modes) -> Candidate {
    let punch = modes == Modes::Punch;
    let mut best = Candidate::NONE;
    for flip in [false, true] {
        let counts = &block.halves[usize::from(flip)];
        let means = [block.mean(&counts[0]), block.mean(&counts[1])];
        // Differential: 5-bit bases, the second within −4 … 3 of the first
        // (clamped there when the means are further apart).
        let q1 = means[0].map(|v| quantize(v, 5));
        let wanted = means[1].map(|v| quantize(v, 5));
        let q2: Rgb =
            std::array::from_fn(|c| wanted[c].clamp((q1[c] - 4).max(0), (q1[c] + 3).min(31)));
        let halves = [
            fit_half(block, &counts[0], q1.map(extend5), punch),
            fit_half(block, &counts[1], q2.map(extend5), punch),
        ];
        let mut bits = 0u64;
        for c in 0..3 {
            let delta = (q2[c] - q1[c]) & 7;
            bits |= ((q1[c] as u64) << 3 | delta as u64) << (56 - 8 * c);
        }
        best.keep_better(pack_etc1(block, bits, true, flip, &halves));
        // Individual: 4-bit bases, for halves too far apart for the
        // differential mode.
        if modes == Modes::All && q2 != wanted {
            let b1 = means[0].map(|v| quantize(v, 4));
            let b2 = means[1].map(|v| quantize(v, 4));
            let halves = [
                fit_half(block, &counts[0], b1.map(extend4), false),
                fit_half(block, &counts[1], b2.map(extend4), false),
            ];
            let mut bits = 0u64;
            for c in 0..3 {
                bits |= ((b1[c] as u64) << 4 | b2[c] as u64) << (56 - 8 * c);
            }
            best.keep_better(pack_etc1(block, bits, false, flip, &halves));
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

/// A half block fitted around a base colour: its error, table and each
/// distinct colour's index.
#[derive(Clone, Copy)]
struct HalfFit {
    error: u64,
    table: u32,
    index: [u8; 16],
}

/// The best table for the texels `counts` counts around `base`.
///
/// An ETC1 modifier adds one value `m` to all three channels, so before
/// clamping a colour `c`'s squared error is `|c − base|² − 2·m·s + 3·m²`,
/// `s` being the sum of `c − base`'s channels: every table is judged from
/// `s` alone, a few operations a colour (as fast ETC1 encoders do, e.g.
/// etcpak's table search on luma differences). Where no palette entry of
/// the best table clamps, that is its exact error; otherwise the two best
/// tables are fitted exactly, clamping included.
fn fit_half(block: &Block, counts: &[u8; 16], base: Rgb, punch: bool) -> HalfFit {
    // The half's colours: (colour, s, count); and the part every table shares.
    let mut present = [(0usize, 0i32, 0i32); 16];
    let mut n = 0;
    let mut shared = 0u64;
    for (u, (c, &k)) in block.colours[..block.distinct]
        .iter()
        .zip(counts)
        .enumerate()
    {
        if k > 0 {
            let d = [c[0] - base[0], c[1] - base[1], c[2] - base[2]];
            let k = i32::from(k);
            present[n] = (u, d[0] + d[1] + d[2], k);
            shared += ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) * k) as u64;
            n += 1;
        }
    }
    let present = &present[..n];
    // Each table's error less `shared`: for each colour the least of
    // 3m² − 2m·s over ±a, ±b (the sign of s's side).
    let near = if punch {
        [0; 8]
    } else {
        ETC1_TABLES.map(|t| t[0])
    };
    let far = ETC1_TABLES.map(|t| t[1]);
    let mut estimates = [0i32; 8];
    for &(_, s, k) in present {
        let s = s.abs();
        for t in 0..8 {
            let (a, b) = (near[t], far[t]);
            estimates[t] += (3 * a * a - 2 * a * s).min(3 * b * b - 2 * b * s) * k;
        }
    }
    let mut first = 0;
    for t in 1..8 {
        if estimates[t] < estimates[first] {
            first = t;
        }
    }
    let [a, b] = ETC1_TABLES[first];
    let (low, high) = (
        base.iter().min().unwrap_or(&0),
        base.iter().max().unwrap_or(&0),
    );
    if !punch && low - b >= 0 && high + b <= 255 {
        // No entry clamps: the estimate is exact.
        let mut fit = HalfFit {
            error: (shared as i64 + i64::from(estimates[first])) as u64,
            table: first as u32,
            index: [0; 16],
        };
        for &(u, s, _) in present {
            let far = 3 * b * b - 2 * b * s.abs() < 3 * a * a - 2 * a * s.abs();
            fit.index[u] = u8::from(far) + if s < 0 { 2 } else { 0 };
        }
        return fit;
    }
    let mut second = usize::from(first == 0);
    for t in 0..8 {
        if t != first && estimates[t] < estimates[second] {
            second = t;
        }
    }
    let mut best = HalfFit {
        error: u64::MAX,
        table: 0,
        index: [0; 16],
    };
    for t in [first, second] {
        let palette = etc1_palette(base, ETC1_TABLES[t], punch);
        let mut fit = HalfFit {
            error: 0,
            table: t as u32,
            index: [0; 16],
        };
        for &(u, _, k) in present {
            let c = block.colours[u];
            let mut nearest = (0u8, u32::MAX);
            for (i, p) in palette.iter().enumerate() {
                if let Some(p) = p {
                    let e = distance(c, *p);
                    if e < nearest.1 {
                        nearest = (i as u8, e);
                    }
                }
            }
            fit.index[u] = nearest.0;
            fit.error += u64::from(nearest.1) * k as u64;
        }
        if fit.error < best.error {
            best = fit;
        }
    }
    best
}

fn pack_etc1(
    block: &Block,
    colour_bits: u64,
    differential: bool,
    flip: bool,
    halves: &[HalfFit; 2],
) -> Candidate {
    let mut indices = [0u32; 16];
    for (i, index) in indices.iter_mut().enumerate() {
        let half = if flip { i / 8 } else { (i % 4) / 2 };
        *index = match block.texels[i] {
            CLEAR => u32::from(block.punch) * 2,
            u => u32::from(halves[half].index[u as usize]),
        };
    }
    let bits = colour_bits
        | u64::from(halves[0].table) << 37
        | u64::from(halves[1].table) << 34
        | u64::from(differential) << 33
        | u64::from(flip) << 32
        | index_bits(&indices);
    Candidate {
        error: halves[0].error + halves[1].error,
        bits,
    }
}

/// The free bits that make a 5-bit base (top three free, low two `high`)
/// plus a 3-bit delta (sign free, low two `low`) leave 0 … 31, selecting
/// T, H or planar mode: (the three top bits, the sign bit).
fn overflow_bits(high: u64, low: u64) -> (u64, u64) {
    if high + low > 3 { (0b111, 0) } else { (0, 1) }
}

/// Per channel, over every texel: Σv, Σ(2x − 3)·v, Σ(2y − 3)·v and Σv²
/// (`x`, `y` the texel's column and row), what a least-squares plane needs.
fn plane_sums(block: &Block) -> [[i64; 4]; 3] {
    let mut sums = [[0i64; 4]; 3];
    for ((c, &k), &(sx, sy)) in block.colours[..block.distinct]
        .iter()
        .zip(&block.counts)
        .zip(&block.moments)
    {
        for ch in 0..3 {
            let v = i64::from(c[ch]);
            let s = &mut sums[ch];
            s[0] += i64::from(k) * v;
            s[1] += i64::from(sx) * v;
            s[2] += i64::from(sy) * v;
            s[3] += i64::from(k) * v * v;
        }
    }
    sums
}

/// The least squared error any plane has over the block (before the
/// planar mode's quantization): no planar encoding does better.
fn planar_bound(sums: &[[i64; 4]; 3]) -> f32 {
    let mut residual = 0f32;
    for &[s, sx, sy, q] in sums {
        // Σ(2x − 3)² over the block is 80, and x and y are uncorrelated.
        let (s, sx, sy, q) = (s as f32, sx as f32, sy as f32, q as f32);
        residual += q - s * s / 16.0 - sx * sx / 80.0 - sy * sy / 80.0;
    }
    residual
}

/// The planar-mode encoding: a least-squares plane through the block
/// (`sums` from [`plane_sums`]).
fn planar(block: &Block, sums: &[[i64; 4]; 3]) -> Candidate {
    // c(x, y) = o + (h − o)·x/4 + (v − o)·y/4 = a + b·x + c·y.
    let colour = |i: usize| {
        block
            .colours
            .get(block.texels[i] as usize)
            .copied()
            .unwrap_or_default()
    };
    let mut channels = [[0i32; 3]; 3];
    for (c, channel) in channels.iter_mut().enumerate() {
        let [s, sx, sy, _] = sums[c];
        let mean = s as f32 / 16.0;
        // Σ(2x − 3)² over the block is 80; the slopes are per texel.
        let (b, cy) = (sx as f32 / 40.0, sy as f32 / 40.0);
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
    let counts = block.counts.map(i32::from);
    let mut far = (0, 0, 0);
    for i in (0..n).filter(|&i| counts[i] > 0) {
        for j in (i + 1..n).filter(|&j| counts[j] > 0) {
            let d = distance(block.colours[i], block.colours[j]);
            if d > far.2 {
                far = (i, j, d);
            }
        }
    }
    let (seed0, seed1) = (block.colours[far.0], block.colours[far.1]);
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
    // Before clamping, a colour c's squared error from base B moved by m
    // along the grey axis is |c − B|² − 2·m·s + 3·m², s the sum of c − B's
    // channels (as in `fit_half`): each candidate's error is summed from
    // these per colour, and only candidates whose entries clamp are
    // fitted entry by entry.
    let bases = q.map(|c| c.map(extend4));
    let mut terms = [[(0i64, 0i64); 16]; 2];
    for (k, base) in bases.iter().enumerate() {
        for u in (0..n).filter(|&u| counts[u] > 0) {
            let c = block.colours[u];
            let d = [c[0] - base[0], c[1] - base[1], c[2] - base[2]];
            terms[k][u] = (
                i64::from(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]),
                i64::from(d[0] + d[1] + d[2]),
            );
        }
    }

    let clamps = |c: Rgb, d: i32| c.iter().any(|&v| v < d || v + d > 255);
    // (error, palette, T or H, first colour, second, distance index)
    let mut best = (u64::MAX, [None; 4], true, q[0], q[1], 0usize);
    let try_distance =
        |d_index: usize, best: &mut (u64, [Option<Rgb>; 4], bool, Rgb, Rgb, usize)| {
            let d = DISTANCES[d_index];
            let d64 = i64::from(d);
            let both = |s: i64| 3 * d64 * d64 - 2 * d64 * s.abs();
            let plus = |c: Rgb| Some(c.map(|v| clamp(v + d)));
            let minus = |c: Rgb| Some(c.map(|v| clamp(v - d)));
            // T: one colour alone, the other with ± d; either cluster alone.
            for (single, pair) in [(0, 1), (1, 0)] {
                let (s, p) = (bases[single], bases[pair]);
                let palette = [Some(s), plus(p), (!punch).then_some(p), minus(p)];
                let error = if clamps(p, d) {
                    block.error(&palette)
                } else {
                    let mut total = 0i64;
                    for u in 0..n {
                        let (alone, _) = terms[single][u];
                        let (e, sum) = terms[pair][u];
                        let moved = if punch { both(sum) } else { both(sum).min(0) };
                        total += alone.min(e + moved) * i64::from(counts[u]);
                    }
                    total as u64
                };
                if error < best.0 {
                    *best = (error, palette, true, q[single], q[pair], d_index);
                }
            }
            // H: both with ± d; the distance's low bit is the colours' order.
            let value = |c: Rgb| (c[0] << 8) | (c[1] << 4) | c[2];
            let (ka, kb) = if (value(q[0]) >= value(q[1])) == (d_index & 1 == 1) {
                (0, 1)
            } else {
                (1, 0)
            };
            if value(q[ka]) == value(q[kb]) && d_index & 1 == 0 {
                return;
            }
            let (ea, eb) = (bases[ka], bases[kb]);
            let palette = [
                plus(ea),
                minus(ea),
                if punch { None } else { plus(eb) },
                minus(eb),
            ];
            let error = if clamps(ea, d) || clamps(eb, d) {
                block.error(&palette)
            } else {
                let mut total = 0i64;
                for u in 0..n {
                    let (e0, s0) = terms[ka][u];
                    let (e1, s1) = terms[kb][u];
                    // In RGB8A1's non-opaque mode the second colour has only − d.
                    let second = if punch {
                        3 * d64 * d64 + 2 * d64 * s1
                    } else {
                        both(s1)
                    };
                    total += (e0 + both(s0)).min(e1 + second) * i64::from(counts[u]);
                }
                total as u64
            };
            if error < best.0 {
                *best = (error, palette, false, q[ka], q[kb], d_index);
            }
        };
    // The distances either side of `centre`, then on while the best is at
    // an end of those tried (errors over the distances are mostly V-shaped,
    // as QuickETC2 observes).
    let (mut low, mut high) = (
        centre.saturating_sub(1),
        (centre + 1).min(DISTANCES.len() - 1),
    );
    for d_index in low..=high {
        try_distance(d_index, &mut best);
    }
    while best.5 == low && low > 0 {
        low -= 1;
        try_distance(low, &mut best);
    }
    while best.5 == high && high + 1 < DISTANCES.len() {
        high += 1;
        try_distance(high, &mut best);
    }
    let (_, palette, t, c1, c2, d) = best;
    let (error, indices) = block.fit(&palette);
    if t {
        pack_t(error, &indices, c1, c2, d as u64)
    } else {
        pack_h(error, &indices, c1, c2, d as u64)
    }
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
    // The error of a table with a base and multiplier, given up once it
    // reaches `limit`.
    let error = |base: i32, multiplier: i32, table: &[i32; 8], limit: u64| -> u64 {
        let levels = table.map(|m| clamp(base + m * multiplier));
        let mut total = 0;
        for &(v, n) in values {
            let e = levels.iter().map(|&l| (v - l).abs()).min().unwrap_or(0);
            total += (e * e) as u64 * n;
            if total >= limit {
                break;
            }
        }
        total
    };
    // (error, base, multiplier, table)
    let mut best = (u64::MAX, 0, 1, 0);
    for (t, table) in EAC_TABLES.iter().enumerate() {
        let span = table[7] - table[3];
        let multiplier = ((max - min + span / 2) / span).clamp(1, 15);
        let base = clamp((min + max - (table[3] + table[7]) * multiplier + 1) / 2);
        let e = error(base, multiplier, table, best.0);
        if e < best.0 {
            best = (e, base, multiplier, t);
            if e == 0 {
                break;
            }
        }
    }
    let (_, centre, m0, t) = best;
    // The best table's neighbouring multipliers and bases, unless exact.
    if best.0 > 0 {
        for multiplier in (m0 - 1).max(1)..=(m0 + 1).min(15) {
            for base in (centre - 1).max(0)..=(centre + 1).min(255) {
                let e = error(base, multiplier, &EAC_TABLES[t], best.0);
                if e < best.0 {
                    best = (e, base, multiplier, t);
                }
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
