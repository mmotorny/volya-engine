// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! DirectDraw Surface (`.dds`) textures: Microsoft's public format. Parses the
//! header and splits the data into per-face, per-mip-level byte slices without
//! copying. Covers what the game's textures use (#26): DXT1/DXT3/DXT5 (BC1,
//! BC2, BC3) and uncompressed formats described by channel bit masks (32-bit
//! RGBA, RGB565, A8, ...), with or without mip chains, 2D or cube map.

use std::io;

const MAGIC: &[u8; 4] = b"DDS ";
const HEADER_SIZE: u32 = 124;
const PIXEL_FORMAT_SIZE: u32 = 32;
/// Magic plus header: where the data starts.
const DATA_OFFSET: usize = 4 + HEADER_SIZE as usize;

const FLAG_MIPMAPCOUNT: u32 = 0x2_0000;
const PF_ALPHAPIXELS: u32 = 0x1;
const PF_ALPHA: u32 = 0x2;
const PF_FOURCC: u32 = 0x4;
const PF_RGB: u32 = 0x40;
const PF_LUMINANCE: u32 = 0x2_0000;
const CAPS2_CUBEMAP: u32 = 0x200;
/// +X, -X, +Y, -Y, +Z, -Z, in storage order.
const CAPS2_CUBEMAP_FACES: [u32; 6] = [0x400, 0x800, 0x1000, 0x2000, 0x4000, 0x8000];
const CAPS2_VOLUME: u32 = 0x20_0000;

/// How texels are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// DXT1: 4×4 blocks of 8 bytes; 1-bit alpha.
    Bc1,
    /// DXT3: 4×4 blocks of 16 bytes; explicit 4-bit alpha.
    Bc2,
    /// DXT5: 4×4 blocks of 16 bytes; interpolated alpha.
    Bc3,
    /// Uncompressed texels, each a little-endian integer whose channels are
    /// picked by bit masks.
    Masked(Masks),
}

/// Channel bit masks of an uncompressed format. A zero mask means the channel
/// is absent: colour channels then read as 0, alpha as fully opaque.
/// Luminance formats carry the same mask in `r`, `g` and `b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Masks {
    /// Bits per texel: 8, 16, 24 or 32.
    pub bits: u32,
    pub r: u32,
    pub g: u32,
    pub b: u32,
    pub a: u32,
}

impl Format {
    /// Bytes per 4×4 block for block-compressed formats.
    pub fn block_bytes(self) -> Option<usize> {
        match self {
            Self::Bc1 => Some(8),
            Self::Bc2 | Self::Bc3 => Some(16),
            Self::Masked(_) => None,
        }
    }

    /// Size in bytes of one `width` × `height` image.
    pub fn image_bytes(self, width: u32, height: u32) -> usize {
        let (w, h) = (width as usize, height as usize);
        match self.block_bytes() {
            Some(block) => w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * block,
            None => w * h * self.texel_bytes(),
        }
    }

    fn texel_bytes(self) -> usize {
        match self {
            Self::Masked(m) => m.bits as usize / 8,
            _ => 0,
        }
    }
}

impl Masks {
    /// 32-bit A8R8G8B8 as stored by D3D (bytes B, G, R, A).
    pub const BGRA8: Self = Self {
        bits: 32,
        r: 0x00ff_0000,
        g: 0x0000_ff00,
        b: 0x0000_00ff,
        a: 0xff00_0000,
    };

    /// Converts one image of `width` × `height` texels to RGBA, 8 bits per
    /// channel, rows top to bottom. Channels narrower than 8 bits are scaled
    /// to the full range (31 → 255).
    pub fn to_rgba8(&self, data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let bytes = self.bits as usize / 8;
        let count = width as usize * height as usize;
        let channels = [self.r, self.g, self.b, self.a].map(Channel::new);
        let mut out = Vec::with_capacity(count * 4);
        for texel in data[..count * bytes].chunks_exact(bytes) {
            let mut value = 0u32;
            for (i, &byte) in texel.iter().enumerate() {
                value |= u32::from(byte) << (8 * i);
            }
            for (i, channel) in channels.iter().enumerate() {
                let absent = if i == 3 { 255 } else { 0 };
                out.push(channel.map_or(absent, |c| c.extract(value)));
            }
        }
        out
    }
}

impl Format {
    /// Decodes one image of `width` × `height` texels (as stored for this
    /// format) to RGBA, 8 bits per channel, rows top to bottom.
    ///
    /// Block formats follow the public S3TC/BC1–3 definitions: 5:6:5
    /// endpoints expanded by bit replication; colour interpolants
    /// (2·c0 + c1) / 3 and (c0 + 2·c1) / 3, or (c0 + c1) / 2 and transparent
    /// black when BC1's c0 ≤ c1; BC3 alpha interpolants ((7 − k)·a0 + k·a1) / 7
    /// when a0 > a1, else ((5 − k)·a0 + k·a1) / 5 plus 0 and 255; integer
    /// division truncating (as docs/behavior/loading-screen.md 8.1 measures).
    pub fn to_rgba8(self, data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let block = match self {
            Self::Masked(masks) => return masks.to_rgba8(data, width, height),
            _ => self.block_bytes().unwrap_or(16),
        };
        let (w, h) = (width as usize, height as usize);
        let mut out = vec![0u8; w * h * 4];
        let blocks_x = w.div_ceil(4).max(1);
        for (index, bytes) in data
            .chunks_exact(block)
            .enumerate()
            .take(blocks_x * h.div_ceil(4).max(1))
        {
            let (bx, by) = (index % blocks_x * 4, index / blocks_x * 4);
            let (alpha, colour) = match self {
                Self::Bc1 => (None, bytes),
                _ => (Some(&bytes[..8]), &bytes[8..16]),
            };
            let texels = decode_colour_block(colour, self == Self::Bc1);
            let alphas = alpha.map(|a| {
                if self == Self::Bc2 {
                    explicit_alpha(a)
                } else {
                    interpolated_alpha(a)
                }
            });
            for (i, mut texel) in texels.into_iter().enumerate() {
                let (x, y) = (bx + i % 4, by + i / 4);
                if x >= w || y >= h {
                    continue;
                }
                if let Some(alphas) = &alphas {
                    texel[3] = alphas[i];
                }
                out[(y * w + x) * 4..][..4].copy_from_slice(&texel);
            }
        }
        out
    }
}

impl Format {
    /// Decodes one 4×4 block of a block-compressed format (its
    /// [`block_bytes`](Self::block_bytes) bytes) to RGBA texels in row
    /// order, as [`to_rgba8`](Self::to_rgba8) does. An uncompressed format
    /// gives transparent black.
    pub fn decode_block(self, bytes: &[u8]) -> [[u8; 4]; 16] {
        let mut texels = match self {
            Self::Bc1 => return decode_colour_block(bytes, true),
            Self::Bc2 | Self::Bc3 => decode_colour_block(&bytes[8..16], false),
            Self::Masked(_) => return [[0; 4]; 16],
        };
        let alphas = if self == Self::Bc2 {
            explicit_alpha(&bytes[..8])
        } else {
            interpolated_alpha(&bytes[..8])
        };
        for (t, a) in texels.iter_mut().zip(alphas) {
            t[3] = a;
        }
        texels
    }

    /// A BC block's colour part as its four colours (RGBA; BC1's
    /// transparent black included) and its texels' 2-bit indices into them
    /// (row order, texel `i`'s at bit `2 i`): what
    /// [`decode_block`](Self::decode_block) gives each texel, alpha aside
    /// for BC2 and BC3. `None` for an uncompressed format.
    pub fn colour_block(self, bytes: &[u8]) -> Option<([[u8; 4]; 4], u32)> {
        match self {
            Self::Bc1 => Some(colour_palette(bytes, true)),
            Self::Bc2 | Self::Bc3 => Some(colour_palette(&bytes[8..16], false)),
            Self::Masked(_) => None,
        }
    }

    /// The alpha block of a BC2 or BC3 block, per texel (row order), as
    /// [`decode_block`](Self::decode_block) gives it; `None` for formats
    /// without one.
    pub fn block_alpha(self, bytes: &[u8]) -> Option<[u8; 16]> {
        match self {
            Self::Bc2 => Some(explicit_alpha(&bytes[..8])),
            Self::Bc3 => Some(interpolated_alpha(&bytes[..8])),
            _ => None,
        }
    }

    /// Whether a BC1 block has a transparent texel (BC1's three-colour mode
    /// with index 3); false for other formats.
    pub fn block_has_transparency(self, bytes: &[u8]) -> bool {
        if self != Self::Bc1 {
            return false;
        }
        let raw0 = u16::from_le_bytes([bytes[0], bytes[1]]);
        let raw1 = u16::from_le_bytes([bytes[2], bytes[3]]);
        let indices = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        raw0 <= raw1 && (0..16).any(|i| (indices >> (2 * i)) & 3 == 3)
    }
}

fn rgb565(value: u16) -> [u32; 3] {
    let (r, g, b) = (
        u32::from(value >> 11),
        u32::from((value >> 5) & 63),
        u32::from(value & 31),
    );
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// The 16 texels of a BC colour block, in row order.
fn decode_colour_block(bytes: &[u8], bc1: bool) -> [[u8; 4]; 16] {
    let (palette, indices) = colour_palette(bytes, bc1);
    std::array::from_fn(|i| palette[((indices >> (2 * i)) & 3) as usize])
}

/// A BC colour block's four colours (RGBA) and its texels' 2-bit indices
/// into them (texel `i`'s at bit `2 i`).
fn colour_palette(bytes: &[u8], bc1: bool) -> ([[u8; 4]; 4], u32) {
    let raw0 = u16::from_le_bytes([bytes[0], bytes[1]]);
    let raw1 = u16::from_le_bytes([bytes[2], bytes[3]]);
    let (c0, c1) = (rgb565(raw0), rgb565(raw1));
    let mix = |f: fn(u32, u32) -> u32| [0, 1, 2].map(|i| f(c0[i], c1[i]));
    let four = !bc1 || raw0 > raw1;
    let palette: [[u32; 4]; 4] = {
        let opaque = |c: [u32; 3]| [c[0], c[1], c[2], 255];
        if four {
            [
                opaque(c0),
                opaque(c1),
                opaque(mix(|a, b| (2 * a + b) / 3)),
                opaque(mix(|a, b| (a + 2 * b) / 3)),
            ]
        } else {
            [
                opaque(c0),
                opaque(c1),
                opaque(mix(|a, b| (a + b) / 2)),
                [0, 0, 0, 0],
            ]
        }
    };
    let indices = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    (palette.map(|c| c.map(|v| v as u8)), indices)
}

/// BC2's alpha: 4 bits per texel, scaled to 8.
fn explicit_alpha(bytes: &[u8]) -> [u8; 16] {
    let bits = u64::from_le_bytes(bytes.try_into().unwrap());
    std::array::from_fn(|i| (((bits >> (4 * i)) & 15) * 17) as u8)
}

/// BC3's alpha: two endpoints and 3-bit indices.
fn interpolated_alpha(bytes: &[u8]) -> [u8; 16] {
    let (a0, a1) = (u32::from(bytes[0]), u32::from(bytes[1]));
    let mut palette = [a0, a1, 0, 0, 0, 0, 0, 0];
    if a0 > a1 {
        for k in 1..=6 {
            palette[k + 1] = ((7 - k as u32) * a0 + k as u32 * a1) / 7;
        }
    } else {
        for k in 1..=4 {
            palette[k + 1] = ((5 - k as u32) * a0 + k as u32 * a1) / 5;
        }
        palette[6] = 0;
        palette[7] = 255;
    }
    let mut bits = 0u64;
    for (i, &b) in bytes[2..8].iter().enumerate() {
        bits |= u64::from(b) << (8 * i);
    }
    std::array::from_fn(|i| palette[((bits >> (3 * i)) & 7) as usize] as u8)
}

/// One channel's position and width within a texel.
#[derive(Clone, Copy)]
struct Channel {
    shift: u32,
    max: u32,
}

impl Channel {
    fn new(mask: u32) -> Option<Self> {
        (mask != 0).then(|| {
            let shift = mask.trailing_zeros();
            Self {
                shift,
                max: mask >> shift,
            }
        })
    }

    fn extract(self, value: u32) -> u8 {
        let v = (value >> self.shift) & self.max;
        if self.max == 255 {
            v as u8
        } else {
            ((u64::from(v) * 255 + u64::from(self.max) / 2) / u64::from(self.max)) as u8
        }
    }
}

/// A parsed `.dds` file, borrowing the file's bytes.
#[derive(Clone, Debug)]
pub struct Dds<'a> {
    /// Size of the top mip level in texels.
    pub width: u32,
    pub height: u32,
    pub format: Format,
    /// Mip levels per face, at least 1.
    pub mip_levels: u32,
    /// Whether this is a cube map.
    pub cube: bool,
    /// Faces stored: 1 for a plain texture, up to 6 for a cube map (in the
    /// order +X, -X, +Y, -Y, +Z, -Z, absent faces skipped).
    pub faces: u32,
    /// The texel data, faces in order, each with its mip chain.
    data: &'a [u8],
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

impl<'a> Dds<'a> {
    /// Parses a whole `.dds` file.
    pub fn parse(bytes: &'a [u8]) -> io::Result<Self> {
        if bytes.len() < DATA_OFFSET || &bytes[..4] != MAGIC {
            return Err(invalid("not a DDS file"));
        }
        // Header fields, offsets from the start of the file.
        let field = |offset: usize| u32_at(bytes, 4 + offset);
        if field(0) != HEADER_SIZE || field(72) != PIXEL_FORMAT_SIZE {
            return Err(invalid("damaged DDS header"));
        }
        let (flags, height, width) = (field(4), field(8), field(12));
        let mip_count = field(24);
        let (pf_flags, fourcc, bits) = (field(76), field(80), field(84));
        let masks = [field(88), field(92), field(96), field(100)];
        let caps2 = field(108);

        if width == 0 || height == 0 {
            return Err(invalid(format!("DDS size {width}×{height}")));
        }
        if caps2 & CAPS2_VOLUME != 0 {
            return Err(invalid("volume DDS textures are not supported"));
        }
        let format = if pf_flags & PF_FOURCC != 0 {
            match &fourcc.to_le_bytes() {
                b"DXT1" => Format::Bc1,
                b"DXT3" => Format::Bc2,
                b"DXT5" => Format::Bc3,
                other => {
                    return Err(invalid(format!(
                        "unsupported DDS format {:?}",
                        String::from_utf8_lossy(other)
                    )));
                }
            }
        } else {
            masked_format(pf_flags, bits, masks)?
        };

        // Writers disagree on whether the flag accompanies the count; a count
        // of 0 or 1 means no chain either way.
        let mip_levels = if flags & FLAG_MIPMAPCOUNT != 0 || mip_count > 1 {
            mip_count.max(1)
        } else {
            1
        };
        let full_chain = 32 - width.max(height).leading_zeros();
        if mip_levels > full_chain {
            return Err(invalid(format!(
                "{mip_levels} mip levels for a {width}×{height} texture"
            )));
        }
        let cube = caps2 & CAPS2_CUBEMAP != 0;
        let faces = if cube {
            CAPS2_CUBEMAP_FACES
                .iter()
                .filter(|&&f| caps2 & f != 0)
                .count() as u32
        } else {
            1
        };
        if faces == 0 {
            return Err(invalid("cube map without faces"));
        }

        let mut dds = Self {
            width,
            height,
            format,
            mip_levels,
            cube,
            faces,
            data: &bytes[DATA_OFFSET..],
        };
        let needed = dds.face_bytes() * faces as usize;
        if dds.data.len() < needed {
            return Err(invalid(format!(
                "DDS data truncated: {} of {needed} bytes",
                dds.data.len()
            )));
        }
        dds.data = &dds.data[..needed];
        Ok(dds)
    }

    /// Size of mip level `mip` in texels.
    pub fn mip_size(&self, mip: u32) -> (u32, u32) {
        ((self.width >> mip).max(1), (self.height >> mip).max(1))
    }

    /// The bytes of mip level `mip` of face `face`, rows top to bottom
    /// (rows of 4×4 blocks for compressed formats).
    ///
    /// # Panics
    ///
    /// If `face` ≥ [`faces`](Self::faces) or `mip` ≥ [`mip_levels`](Self::mip_levels).
    pub fn level(&self, face: u32, mip: u32) -> &'a [u8] {
        assert!(face < self.faces && mip < self.mip_levels);
        let mut start = face as usize * self.face_bytes();
        for m in 0..mip {
            start += self.mip_bytes(m);
        }
        &self.data[start..start + self.mip_bytes(mip)]
    }

    /// Total texel data in bytes (all faces and levels).
    pub fn data_len(&self) -> usize {
        self.data.len()
    }

    fn mip_bytes(&self, mip: u32) -> usize {
        let (w, h) = self.mip_size(mip);
        self.format.image_bytes(w, h)
    }

    fn face_bytes(&self) -> usize {
        (0..self.mip_levels).map(|m| self.mip_bytes(m)).sum()
    }
}

fn masked_format(flags: u32, bits: u32, [r, g, b, a]: [u32; 4]) -> io::Result<Format> {
    if !matches!(bits, 8 | 16 | 24 | 32) {
        return Err(invalid(format!("unsupported DDS texel size {bits} bits")));
    }
    let alpha = if flags & (PF_ALPHAPIXELS | PF_ALPHA) != 0 {
        a
    } else {
        0
    };
    let masks = if flags & PF_RGB != 0 {
        Masks {
            bits,
            r,
            g,
            b,
            a: alpha,
        }
    } else if flags & PF_LUMINANCE != 0 {
        Masks {
            bits,
            r,
            g: r,
            b: r,
            a: alpha,
        }
    } else if flags & PF_ALPHA != 0 {
        Masks {
            bits,
            r: 0,
            g: 0,
            b: 0,
            a,
        }
    } else {
        return Err(invalid(format!(
            "unsupported DDS pixel format flags {flags:#x}"
        )));
    };
    let used = u64::from(masks.r | masks.g | masks.b | masks.a);
    if used == 0 || used >> bits != 0 {
        return Err(invalid("DDS channel masks exceed the texel size"));
    }
    Ok(Format::Masked(masks))
}

/// Builders of synthetic `.dds` files, for tests here and in other crates.
pub mod build {
    use super::*;

    /// How a synthetic file's pixel format is described.
    #[derive(Clone, Copy, Debug)]
    pub enum PixelFormat {
        FourCc(&'static [u8; 4]),
        /// Pixel format flags and masks as stored.
        Masks {
            flags: u32,
            bits: u32,
            masks: [u32; 4],
        },
    }

    impl PixelFormat {
        pub const BGRA8: Self = Self::Masks {
            flags: PF_RGB | PF_ALPHAPIXELS,
            bits: 32,
            masks: [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000],
        };
        pub const RGB565: Self = Self::Masks {
            flags: PF_RGB,
            bits: 16,
            masks: [0xf800, 0x07e0, 0x001f, 0],
        };
        pub const A8: Self = Self::Masks {
            flags: PF_ALPHA,
            bits: 8,
            masks: [0, 0, 0, 0xff],
        };
    }

    /// A `.dds` file: header for `width` × `height` with `mips` levels
    /// (0: no chain flag) and `cube` faces, followed by `data` verbatim.
    pub fn file(
        width: u32,
        height: u32,
        mips: u32,
        cube: bool,
        format: PixelFormat,
        data: &[u8],
    ) -> Vec<u8> {
        let mut header = [0u32; 31];
        header[0] = HEADER_SIZE;
        header[1] = 0x1007 | if mips > 0 { FLAG_MIPMAPCOUNT } else { 0 };
        header[2] = height;
        header[3] = width;
        header[6] = mips;
        header[18] = PIXEL_FORMAT_SIZE;
        match format {
            PixelFormat::FourCc(code) => {
                header[19] = PF_FOURCC;
                header[20] = u32::from_le_bytes(*code);
            }
            PixelFormat::Masks { flags, bits, masks } => {
                header[19] = flags;
                header[21] = bits;
                header[22..26].copy_from_slice(&masks);
            }
        }
        header[26] = 0x1000;
        if cube {
            header[27] = CAPS2_CUBEMAP | CAPS2_CUBEMAP_FACES.iter().sum::<u32>();
        }
        let mut out = MAGIC.to_vec();
        for value in header {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(data);
        out
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_block_formats() {
        // BC1, four colours: c0 pure red, c1 pure blue; indices 0,1,2,3 in row 0.
        let bc1 = [0x00, 0xF8, 0x1F, 0x00, 0b1110_0100, 0, 0, 0];
        let rgba = Format::Bc1.to_rgba8(&bc1, 4, 4);
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[4..8], &[0, 0, 255, 255]);
        assert_eq!(&rgba[8..12], &[170, 0, 85, 255]);
        assert_eq!(&rgba[12..16], &[85, 0, 170, 255]);
        // BC1, c0 ≤ c1: index 3 is transparent black, index 2 the average.
        let bc1 = [0x1F, 0x00, 0x00, 0xF8, 0b1110_0100, 0, 0, 0];
        let rgba = Format::Bc1.to_rgba8(&bc1, 4, 4);
        assert_eq!(&rgba[8..12], &[127, 0, 127, 255]);
        assert_eq!(&rgba[12..16], &[0, 0, 0, 0]);
        // BC3 alpha: a0 = 255 > a1 = 0; indices 0, 1, 2 (= (6·255 + 0)/7 = 218).
        let mut bc3 = [
            255,
            0,
            0b0101_0000,
            0,
            0,
            0,
            0,
            0,
            0,
            0xF8,
            0,
            0xF8,
            0,
            0,
            0,
            0,
        ];
        bc3[2] = 0b1000_1000; // texel 0: 0, texel 1: 1, texel 2: 2 (bits 6..8)
        bc3[3] = 0;
        let rgba = Format::Bc3.to_rgba8(&bc3, 4, 4);
        assert_eq!((rgba[3], rgba[7], rgba[11]), (255, 0, 218));
        // BC2 alpha: nibbles F, 0, 8.
        let bc2 = [0x0F, 0x08, 0, 0, 0, 0, 0, 0, 0, 0xF8, 0, 0xF8, 0, 0, 0, 0];
        let rgba = Format::Bc2.to_rgba8(&bc2, 4, 4);
        assert_eq!((rgba[3], rgba[7], rgba[11]), (255, 0, 136));
        // A 2 × 2 image keeps only the block's top-left texels.
        assert_eq!(Format::Bc1.to_rgba8(&[0; 8], 2, 2).len(), 16);
    }

    use super::build::{PixelFormat, file};
    use super::*;

    #[test]
    fn bc_mip_chain_levels() {
        // 8×4 DXT1 with 4 levels: 8×4, 4×2, 2×1, 1×1 → 2, 1, 1, 1 blocks.
        let data: Vec<u8> = (0..5 * 8).map(|i| i as u8).collect();
        let bytes = file(8, 4, 4, false, PixelFormat::FourCc(b"DXT1"), &data);
        let dds = Dds::parse(&bytes).unwrap();
        assert_eq!((dds.width, dds.height, dds.format), (8, 4, Format::Bc1));
        assert_eq!((dds.mip_levels, dds.faces, dds.cube), (4, 1, false));
        assert_eq!(dds.level(0, 0), &data[..16]);
        assert_eq!(dds.level(0, 1), &data[16..24]);
        assert_eq!(dds.level(0, 3), &data[32..40]);
        assert_eq!(dds.mip_size(2), (2, 1));
    }

    #[test]
    fn bc2_bc3_blocks_and_trailing_bytes() {
        for (code, format) in [(b"DXT3", Format::Bc2), (b"DXT5", Format::Bc3)] {
            let mut data = vec![7; 4 * 16];
            data.extend_from_slice(b"trailer");
            let bytes = file(8, 8, 0, false, PixelFormat::FourCc(code), &data);
            let dds = Dds::parse(&bytes).unwrap();
            assert_eq!((dds.format, dds.mip_levels), (format, 1));
            assert_eq!(dds.level(0, 0).len(), 64);
            assert_eq!(dds.data_len(), 64);
        }
    }

    #[test]
    fn cube_map_faces_in_order() {
        // 4×4 BGRA8, 3 levels (64 + 16 + 4 bytes per face).
        let face = 84;
        let data: Vec<u8> = (0..6 * face).map(|i| (i / face) as u8).collect();
        let bytes = file(4, 4, 3, true, PixelFormat::BGRA8, &data);
        let dds = Dds::parse(&bytes).unwrap();
        assert!(dds.cube);
        assert_eq!((dds.faces, dds.mip_levels), (6, 3));
        for f in 0..6 {
            assert!(dds.level(f, 0).iter().all(|&b| b == f as u8));
            assert_eq!(dds.level(f, 2).len(), 4);
            assert_eq!(dds.level(f, 2)[0], f as u8);
        }
    }

    #[test]
    fn mip_count_without_flag() {
        let data = vec![0; 16 + 4];
        let mut bytes = file(2, 2, 2, false, PixelFormat::BGRA8, &data);
        let flags = u32_at(&bytes, 8) & !FLAG_MIPMAPCOUNT;
        bytes[8..12].copy_from_slice(&flags.to_le_bytes());
        assert_eq!(Dds::parse(&bytes).unwrap().mip_levels, 2);
    }

    /// Parses a one-level masked file and converts it.
    fn convert(width: u32, height: u32, format: PixelFormat, data: &[u8]) -> (Masks, Vec<u8>) {
        let bytes = file(width, height, 0, false, format, data);
        let dds = Dds::parse(&bytes).unwrap();
        let Format::Masked(masks) = dds.format else {
            panic!("{:?}", dds.format)
        };
        (masks, masks.to_rgba8(dds.level(0, 0), width, height))
    }

    #[test]
    fn converts_masked_formats() {
        // BGRA8: bytes B, G, R, A.
        let (masks, rgba) = convert(1, 1, PixelFormat::BGRA8, &[1, 2, 3, 4]);
        assert_eq!(masks, Masks::BGRA8);
        assert_eq!(rgba, [3, 2, 1, 4]);

        // RGB565: white, pure red, mid green; opaque.
        let texels = [0xffffu16, 0xf800, 0x0400];
        let data: Vec<u8> = texels.iter().flat_map(|t| t.to_le_bytes()).collect();
        let (_, rgba) = convert(3, 1, PixelFormat::RGB565, &data);
        assert_eq!(rgba, [255, 255, 255, 255, 255, 0, 0, 255, 0, 130, 0, 255]);

        // A8: alpha only, colour 0.
        let (_, rgba) = convert(2, 1, PixelFormat::A8, &[0x80, 0xff]);
        assert_eq!(rgba, [0, 0, 0, 128, 0, 0, 0, 255]);

        // Luminance: replicated into R, G, B.
        let format = PixelFormat::Masks {
            flags: PF_LUMINANCE,
            bits: 8,
            masks: [0xff, 0, 0, 0],
        };
        assert_eq!(convert(1, 1, format, &[9]).1, [9, 9, 9, 255]);
    }

    #[test]
    fn rejects_bad_files() {
        let error = |bytes: &[u8]| Dds::parse(bytes).unwrap_err().to_string();
        assert_eq!(error(b"DDS "), "not a DDS file");
        assert!(
            error(&file(4, 4, 0, false, PixelFormat::FourCc(b"DXT1"), &[0; 7]))
                .contains("truncated")
        );
        assert!(
            error(&file(
                4,
                4,
                4,
                false,
                PixelFormat::FourCc(b"DXT1"),
                &[0; 64]
            ))
            .contains("mip levels")
        );
        assert!(
            error(&file(
                4,
                4,
                0,
                false,
                PixelFormat::FourCc(b"ATI2"),
                &[0; 16]
            ))
            .contains("ATI2")
        );
        assert!(error(&file(0, 4, 0, false, PixelFormat::FourCc(b"DXT1"), &[])).contains("size"));
        let wide = PixelFormat::Masks {
            flags: PF_RGB,
            bits: 16,
            masks: [0xff_0000, 0, 0, 0],
        };
        assert!(error(&file(1, 1, 0, false, wide, &[0; 2])).contains("masks"));
    }
}
