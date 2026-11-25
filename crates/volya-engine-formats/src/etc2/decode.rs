// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! An ETC2 / EAC block decoder written from the Khronos Data Format
//! Specification's decoding rules, apart from the encoder (its own reading
//! of every field), for the tests. The GPU's decoder checks both natively
//! (`volya-engine-render`'s tests on Metal, which has ETC2).

use super::{DISTANCES, EAC_TABLES, ETC1_TABLES};

fn bits(word: u64, high: u32, count: u32) -> i32 {
    ((word >> (high + 1 - count)) & ((1 << count) - 1)) as i32
}

fn sign3(v: i32) -> i32 {
    if v >= 4 { v - 8 } else { v }
}

fn clamp(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// Texel `(x, y)`'s 2-bit index (MSB plane in bits 31..16, LSB in 15..0,
/// texels by columns).
fn index(word: u64, x: usize, y: usize) -> usize {
    let j = x * 4 + y;
    (((word >> (16 + j)) & 1) << 1 | ((word >> j) & 1)) as usize
}

/// RGBA texels (row order) of an ETC2 RGB8 block, or of an RGB8A1 block
/// with `punch` (bit 33 then the opaque flag; transparent texels black).
pub fn rgb(block: [u8; 8], punch: bool) -> [[u8; 4]; 16] {
    let w = u64::from_be_bytes(block);
    let flag = (w >> 33) & 1 == 1;
    let differential = punch || flag;
    let opaque = !punch || flag;
    let mut out = [[0, 0, 0, 255]; 16];
    let paint = |out: &mut [[u8; 4]; 16], colours: [[i32; 3]; 4], transparent_2: bool| {
        for y in 0..4 {
            for x in 0..4 {
                let k = index(w, x, y);
                out[y * 4 + x] = if transparent_2 && k == 2 {
                    [0, 0, 0, 0]
                } else {
                    let c = colours[k];
                    [clamp(c[0]), clamp(c[1]), clamp(c[2]), 255]
                };
            }
        }
    };
    let ext4 = |v: i32| v * 17;
    let ext5 = |v: i32| (v << 3) | (v >> 2);
    if differential {
        let r = bits(w, 63, 5) + sign3(bits(w, 58, 3));
        let g = bits(w, 55, 5) + sign3(bits(w, 50, 3));
        let b = bits(w, 47, 5) + sign3(bits(w, 42, 3));
        if !(0..=31).contains(&r) {
            // T mode.
            let c1 = [
                bits(w, 60, 2) << 2 | bits(w, 57, 2),
                bits(w, 55, 4),
                bits(w, 51, 4),
            ];
            let c2 = [bits(w, 47, 4), bits(w, 43, 4), bits(w, 39, 4)];
            let d = DISTANCES[(bits(w, 35, 2) << 1 | bits(w, 32, 1)) as usize];
            let (c1, c2) = (c1.map(ext4), c2.map(ext4));
            paint(
                &mut out,
                [c1, c2.map(|v| v + d), c2, c2.map(|v| v - d)],
                !opaque,
            );
            return out;
        }
        if !(0..=31).contains(&g) {
            // H mode.
            let c1 = [
                bits(w, 62, 4),
                bits(w, 58, 3) << 1 | bits(w, 52, 1),
                bits(w, 51, 1) << 3 | bits(w, 49, 3),
            ];
            let c2 = [bits(w, 46, 4), bits(w, 42, 4), bits(w, 38, 4)];
            let value = |c: [i32; 3]| c[0] << 8 | c[1] << 4 | c[2];
            let low = i32::from(value(c1) >= value(c2));
            let d = DISTANCES[(bits(w, 34, 1) << 2 | bits(w, 32, 1) << 1 | low) as usize];
            let (c1, c2) = (c1.map(ext4), c2.map(ext4));
            paint(
                &mut out,
                [
                    c1.map(|v| v + d),
                    c1.map(|v| v - d),
                    c2.map(|v| v + d),
                    c2.map(|v| v - d),
                ],
                !opaque,
            );
            return out;
        }
        if !(0..=31).contains(&b) {
            // Planar mode (always opaque).
            let e6 = |v: i32| (v << 2) | (v >> 4);
            let e7 = |v: i32| (v << 1) | (v >> 6);
            let o = [
                e6(bits(w, 62, 6)),
                e7(bits(w, 56, 1) << 6 | bits(w, 54, 6)),
                e6(bits(w, 48, 1) << 5 | bits(w, 44, 2) << 3 | bits(w, 41, 3)),
            ];
            let h = [
                e6(bits(w, 38, 5) << 1 | bits(w, 32, 1)),
                e7(bits(w, 31, 7)),
                e6(bits(w, 24, 6)),
            ];
            let v = [e6(bits(w, 18, 6)), e7(bits(w, 12, 7)), e6(bits(w, 5, 6))];
            for y in 0..4 {
                for x in 0..4 {
                    let c: [u8; 3] = std::array::from_fn(|i| {
                        clamp((x * (h[i] - o[i]) + y * (v[i] - o[i]) + 4 * o[i] + 2) >> 2)
                    });
                    out[(y * 4 + x) as usize] = [c[0], c[1], c[2], 255];
                }
            }
            return out;
        }
    }
    // Individual or differential: two subblocks.
    let (b1, b2) = if differential {
        let base = |hi: u32, delta: u32| {
            let v = bits(w, hi, 5);
            (ext5(v), ext5(v + sign3(bits(w, delta, 3))))
        };
        let (r, g, b) = (base(63, 58), base(55, 50), base(47, 42));
        ([r.0, g.0, b.0], [r.1, g.1, b.1])
    } else {
        (
            [bits(w, 63, 4), bits(w, 55, 4), bits(w, 47, 4)].map(ext4),
            [bits(w, 59, 4), bits(w, 51, 4), bits(w, 43, 4)].map(ext4),
        )
    };
    let tables = [bits(w, 39, 3) as usize, bits(w, 36, 3) as usize];
    let flip = (w >> 32) & 1 == 1;
    for y in 0..4 {
        for x in 0..4 {
            let second = if flip { y >= 2 } else { x >= 2 };
            let (base, [a, b]) = if second {
                (b2, ETC1_TABLES[tables[1]])
            } else {
                (b1, ETC1_TABLES[tables[0]])
            };
            let k = index(w, x, y);
            out[y * 4 + x] = if !opaque && k == 2 {
                [0, 0, 0, 0]
            } else {
                let a = if opaque { a } else { 0 };
                let m = [a, b, -a, -b][k];
                [
                    clamp(base[0] + m),
                    clamp(base[1] + m),
                    clamp(base[2] + m),
                    255,
                ]
            };
        }
    }
    out
}

/// The 16 alpha values (row order) of an EAC block.
pub fn alpha(block: [u8; 8]) -> [u8; 16] {
    let w = u64::from_be_bytes(block);
    let base = bits(w, 63, 8);
    let multiplier = bits(w, 55, 4);
    let table = EAC_TABLES[bits(w, 51, 4) as usize];
    std::array::from_fn(|i| {
        let (x, y) = (i % 4, i / 4);
        let j = (x * 4 + y) as u32;
        let k = bits(w, 47 - 3 * j, 3) as usize;
        clamp(base + table[k] * multiplier)
    })
}
