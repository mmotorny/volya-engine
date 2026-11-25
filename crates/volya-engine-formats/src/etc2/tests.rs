// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The encoder checked against an independent decoder (`texture2ddecoder`):
//! every mode's bits decode to the colours the encoder fitted.

use super::*;

/// A small deterministic generator (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
}

fn rgba(p: u32) -> [u8; 4] {
    let [b, g, r, a] = p.to_le_bytes();
    [r, g, b, a]
}

/// RGBA texels (row order) of an ETC2 RGB8 block, by the oracle.
fn decode_rgb(bits: [u8; 8]) -> [[u8; 4]; 16] {
    let mut out = [0u32; 16];
    texture2ddecoder::decode_etc2_rgb_block(&bits, &mut out);
    out.map(rgba)
}

/// RGBA texels (row order) of an ETC2 RGB8A1 block, by the oracle.
fn decode_rgb_a1(bits: [u8; 8]) -> [[u8; 4]; 16] {
    let mut out = [0u32; 16];
    texture2ddecoder::decode_etc2_rgba1_block(&bits, &mut out);
    out.map(rgba)
}

/// Alpha (row order) of an ETC2 RGBA8 block whose alpha half is `alpha`.
fn decode_alpha(alpha: [u8; 8]) -> [u8; 16] {
    let mut block = [0u8; 16];
    block[..8].copy_from_slice(&alpha);
    let mut out = [0u32; 16];
    texture2ddecoder::decode_etc2_rgba8_block(&block, &mut out);
    out.map(|p| p.to_le_bytes()[3])
}

fn opaque(texels: &[[u8; 4]; 16]) -> Block {
    Block::new(texels, |_| false, false)
}

/// Squared RGB error over the texels `seen` keeps.
fn squared_error_where(
    a: &[[u8; 4]; 16],
    b: &[[u8; 4]; 16],
    seen: impl Fn(&[u8; 4]) -> bool,
) -> u64 {
    a.iter()
        .zip(b)
        .filter(|(x, _)| seen(x))
        .map(|(x, y)| {
            (0..3)
                .map(|c| (i64::from(x[c]) - i64::from(y[c])).pow(2) as u64)
                .sum::<u64>()
        })
        .sum()
}

fn squared_error(a: &[[u8; 4]; 16], b: &[[u8; 4]; 16]) -> u64 {
    squared_error_where(a, b, |_| true)
}

/// Blocks of several kinds: noise, BC1-like four-colour lines, two
/// colours, gradients, flat.
fn blocks(rng: &mut Rng, n: usize) -> Vec<[[u8; 4]; 16]> {
    (0..n)
        .map(|k| match k % 5 {
            0 => std::array::from_fn(|_| [rng.byte(), rng.byte(), rng.byte(), 255]),
            1 => {
                let mut bc1 = [0u8; 8];
                bc1.iter_mut().for_each(|b| *b = rng.byte());
                Format::Bc1.decode_block(&bc1).map(|mut t| {
                    t[3] = 255;
                    t
                })
            }
            2 => {
                let pair = [
                    [rng.byte(), rng.byte(), rng.byte(), 255],
                    [rng.byte(), rng.byte(), rng.byte(), 255],
                ];
                let mask = rng.next();
                std::array::from_fn(|i| pair[(mask >> i) as usize & 1])
            }
            3 => {
                let (o, dx, dy) = (rng.byte() / 2, rng.byte() % 24, rng.byte() % 24);
                std::array::from_fn(|i| {
                    let v = u32::from(o)
                        + u32::from(dx) * (i % 4) as u32
                        + u32::from(dy) * (i / 4) as u32;
                    let v = v.min(255) as u8;
                    [v, v / 2 + 20, 255 - v, 255]
                })
            }
            _ => [[rng.byte(), rng.byte(), rng.byte(), 255]; 16],
        })
        .collect()
}

/// Every mode's encoding decodes, by the oracle, with exactly the error the
/// encoder computed for it (so its bits say what it fitted).
#[test]
fn every_mode_decodes_as_fitted() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for texels in blocks(&mut rng, 2000) {
        let block = opaque(&texels);
        for (mode, candidate) in [
            ("etc1", etc1(&block, Modes::All)),
            ("planar", planar(&block)),
            ("t/h", t_and_h(&block)),
        ] {
            let decoded = decode_rgb(candidate.bits.to_be_bytes());
            assert_eq!(
                squared_error(&texels, &decoded),
                candidate.error,
                "{mode}: {texels:?}"
            );
        }
    }
}

/// The mode a block's bits select.
fn mode(bits: [u8; 8]) -> &'static str {
    if bits[3] & 2 == 0 {
        return "individual";
    }
    let overflow = |byte: u8| {
        let base = i32::from(byte >> 3);
        let delta = i32::from(byte & 7) - if byte & 4 != 0 { 8 } else { 0 };
        !(0..=31).contains(&(base + delta))
    };
    if overflow(bits[0]) {
        "t"
    } else if overflow(bits[1]) {
        "h"
    } else if overflow(bits[2]) {
        "planar"
    } else {
        "differential"
    }
}

/// Each mode is what its bits select (an error match above alone could hide
/// a mode decoded as another with the same colours), and T and H each win
/// somewhere.
#[test]
fn modes_are_selected_by_their_bits() {
    let mut rng = Rng(7);
    for texels in blocks(&mut rng, 500) {
        let block = opaque(&texels);
        assert_eq!(mode(planar(&block).bits.to_be_bytes()), "planar");
        let th = mode(t_and_h(&block).bits.to_be_bytes());
        assert!(th == "t" || th == "h", "{th}");
        let e = mode(etc1(&block, Modes::All).bits.to_be_bytes());
        assert!(e == "individual" || e == "differential", "{e}");
        let e = mode(etc1(&block, Modes::Opaque).bits.to_be_bytes());
        assert_eq!(e, "differential");
    }
    let mut seen = std::collections::HashSet::new();
    for texels in blocks(&mut Rng(99), 2000) {
        seen.insert(mode(t_and_h(&opaque(&texels)).bits.to_be_bytes()));
    }
    assert!(seen.contains("t") && seen.contains("h"), "{seen:?}");
}

#[test]
fn flat_and_two_colour_blocks() {
    // A flat block is within the smallest modifier (2) per channel.
    let flat = [[0x88, 0x44, 0xcc, 255]; 16];
    assert!(close(&decode_rgb(encode_rgb(&flat)), &flat, 2));
    // Two colours far apart in hue (which the ETC1 modes can't mix within
    // a half) are close in T or H mode.
    let red_green: [[u8; 4]; 16] = std::array::from_fn(|i| {
        if i % 2 == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        }
    });
    let decoded = decode_rgb(encode_rgb(&red_green));
    assert!(squared_error(&red_green, &decoded) <= 16 * 3 * 9, "{decoded:?}");
}

#[test]
fn transparent_texels_are_not_fitted() {
    // Black transparent texels beside a colour: the colour is exact.
    let texels: [[u8; 4]; 16] = std::array::from_fn(|i| {
        if i < 8 {
            [0, 0, 0, 0]
        } else {
            [0x33, 0x66, 0x99, 255]
        }
    });
    let decoded = decode_rgb(encode_rgb(&texels));
    assert!(close(&decoded[8..], &texels[8..], 2), "{decoded:?}");
}

/// Whether every channel of `a` is within `tolerance` of `b`'s.
fn close(a: &[[u8; 4]], b: &[[u8; 4]], tolerance: u8) -> bool {
    a.iter()
        .zip(b)
        .all(|(x, y)| (0..3).all(|c| x[c].abs_diff(y[c]) <= tolerance))
}

/// RGB8A1: transparent texels decode transparent, opaque ones opaque, and
/// each mode's error is what the encoder fitted.
#[test]
fn punch_through_blocks() {
    let mut rng = Rng(31);
    for (n, mut texels) in blocks(&mut rng, 2000).into_iter().enumerate() {
        // Some texels transparent (BC1's transparent black), in most blocks.
        let mask = if n % 4 == 0 { 0 } else { rng.next() };
        for (i, t) in texels.iter_mut().enumerate() {
            if (mask >> i) & 1 == 1 {
                *t = [0, 0, 0, 0];
            }
        }
        let decoded = decode_rgb_a1(encode_rgb_a1(&texels));
        for (t, d) in texels.iter().zip(&decoded) {
            assert_eq!(d[3], if t[3] == 0 { 0 } else { 255 }, "{texels:?}");
        }
        let block = Block::new(&texels, |t| t[3] < 128, true);
        let modes = if block.has_clear() {
            Modes::Punch
        } else {
            Modes::Opaque
        };
        let mut candidates = vec![etc1(&block, modes), t_and_h(&block)];
        if modes == Modes::Opaque {
            candidates.push(planar(&block));
        }
        for mut c in candidates {
            if modes == Modes::Punch {
                c.bits &= !(1 << 33);
            }
            let decoded = decode_rgb_a1(c.bits.to_be_bytes());
            assert_eq!(
                squared_error_where(&texels, &decoded, |t| t[3] != 0),
                c.error,
                "{texels:?}"
            );
        }
    }
}

#[test]
fn alpha_blocks_decode_as_fitted() {
    // Constant alpha is exact, 0 and 255 included.
    for a in [0u8, 1, 127, 128, 254, 255] {
        assert_eq!(decode_alpha(encode_alpha(&[a; 16])), [a; 16]);
    }
    // Two levels (cut-outs) are exact.
    let cut: [u8; 16] = std::array::from_fn(|i| if i % 3 == 0 { 0 } else { 255 });
    assert_eq!(decode_alpha(encode_alpha(&cut)), cut);
    // Random BC3 alpha blocks (any two endpoints, any of their eight levels
    // per texel: the hardest case).
    let mut rng = Rng(42);
    let mut total = 0u64;
    let n = 2000;
    for _ in 0..n {
        let mut bc3 = [0u8; 16];
        bc3.iter_mut().for_each(|b| *b = rng.byte());
        let alpha = Format::Bc3.decode_block(&bc3).map(|t| t[3]);
        let decoded = decode_alpha(encode_alpha(&alpha));
        total += alpha
            .iter()
            .zip(decoded)
            .map(|(&a, b)| (i64::from(a) - i64::from(b)).pow(2) as u64)
            .sum::<u64>();
    }
    let psnr = psnr(total, n * 16);
    assert!(psnr > 34.0, "BC3 alpha → EAC: {psnr:.1} dB");
}

fn psnr(squared: u64, samples: usize) -> f64 {
    let mse = squared as f64 / samples as f64;
    10.0 * (255.0f64 * 255.0 / mse).log10()
}

/// PSNR of ETC2 over random BC1 blocks (the hardest case: any four
/// colours on any line, unrelated to their neighbours) and over BC1 blocks
/// with near endpoints (smooth content).
#[test]
fn quality_over_bc1_blocks() {
    let quality = |pairs: &[([[u8; 4]; 16], [[u8; 4]; 16])]| {
        let total: u64 = pairs.iter().map(|(a, b)| squared_error(a, b)).sum();
        psnr(total, pairs.len() * 48)
    };
    let mut rng = Rng(5);
    let random: Vec<_> = (0..4000)
        .map(|_| {
            let mut bc1 = [0u8; 8];
            bc1.iter_mut().for_each(|b| *b = rng.byte());
            // Four-colour mode only (opaque).
            if u16::from_le_bytes([bc1[0], bc1[1]]) <= u16::from_le_bytes([bc1[2], bc1[3]]) {
                bc1.swap(0, 2);
                bc1.swap(1, 3);
            }
            let texels = Format::Bc1.decode_block(&bc1);
            (texels, decode_rgb(encode_rgb(&texels)))
        })
        .collect();
    let random = quality(&random);
    let smooth: Vec<_> = (0..4000)
        .map(|_| {
            let c0 = u16::from(rng.byte()) << 8 | u16::from(rng.byte());
            let step = (rng.next() % 3) as u16;
            let c1 = c0.saturating_sub(step << 11 | step << 5 | step);
            let mut bc1 = [0u8; 8];
            bc1[..2].copy_from_slice(&c0.max(c1 + 1).to_le_bytes());
            bc1[2..4].copy_from_slice(&c1.to_le_bytes());
            for b in &mut bc1[4..] {
                *b = rng.byte();
            }
            let texels = Format::Bc1.decode_block(&bc1);
            (texels, decode_rgb(encode_rgb(&texels)))
        })
        .collect();
    let smooth = quality(&smooth);
    assert!(random > 21.0, "random BC1 blocks: {random:.1} dB");
    assert!(smooth > 32.0, "smooth BC1 blocks: {smooth:.1} dB");
}

#[test]
fn transcodes_images_and_picks_formats() {
    // Red to blue (index 0 red, 1 blue): BC1's four-colour mode, opaque.
    let solid = [0x00, 0xf8, 0x1f, 0x00, 0b1110_0100, 0, 0, 0];
    // Three-colour mode, its first row transparent.
    let cut = [0x1f, 0x00, 0x00, 0xf8, 0b1111_1111, 0, 0, 0];
    assert_eq!(Etc2Format::for_bc(Format::Bc1, &solid), Some(Etc2Format::Rgb8));
    let both = [solid, cut].concat();
    assert_eq!(
        Etc2Format::for_bc(Format::Bc1, &both),
        Some(Etc2Format::Rgb8A1)
    );
    assert_eq!(Etc2Format::for_bc(Format::Bc3, &[]), Some(Etc2Format::Rgba8));
    assert_eq!(Etc2Format::for_bc(Format::Bc2, &[]), Some(Etc2Format::Rgba8));

    let out = transcode_bc(Format::Bc1, &both, Etc2Format::Rgb8A1);
    assert_eq!(out.len(), 16);
    let alpha = |bits: &[u8]| decode_rgb_a1(bits.try_into().unwrap()).map(|t| t[3]);
    assert_eq!(alpha(&out[..8]), [255; 16]);
    assert_eq!(alpha(&out[8..]), Format::Bc1.decode_block(&cut).map(|t| t[3]));

    // BC3: alpha and colour halves.
    let mut bc3 = [0u8; 16];
    bc3[0] = 255;
    bc3[1] = 0;
    bc3[8..].copy_from_slice(&solid);
    let out = transcode_bc(Format::Bc3, &bc3, Etc2Format::Rgba8);
    assert_eq!(out.len(), 16);
    let source = Format::Bc3.decode_block(&bc3);
    assert_eq!(
        decode_alpha(out[..8].try_into().unwrap()),
        source.map(|t| t[3])
    );
    let colour = decode_rgb(out[8..].try_into().unwrap());
    assert!(squared_error(&colour, &source) < 16 * 3 * 24 * 24);
}
