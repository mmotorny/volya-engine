// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Textures' platform-free part, both renderers': the paths BC textures
//! take to the GPU and their transcoding to ETC2 (#113).

use std::fmt;

use volya_engine_formats::dds::{Dds, Format};
use volya_engine_formats::etc2::{self, Etc2Format};

/// Why a texture could not be created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureError {
    /// A size the device or the format cannot hold.
    Size {
        width: u32,
        height: u32,
        reason: &'static str,
    },
    /// Transcoded data that does not match its texture.
    Transcoded,
}

impl fmt::Display for TextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size {
                width,
                height,
                reason,
            } => write!(f, "texture size {width}×{height}: {reason}"),
            Self::Transcoded => write!(f, "transcoded data does not match the texture"),
        }
    }
}

impl std::error::Error for TextureError {}

/// How BC textures reach this device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturePath {
    /// As stored (`texture-compression-bc`).
    Bc,
    /// Transcoded to ETC2 (`texture-compression-etc2`).
    Etc2,
    /// Decoded to RGBA8.
    Rgba8,
}

impl TexturePath {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bc => "bc",
            Self::Etc2 => "etc2",
            Self::Rgba8 => "rgba8",
        }
    }
}

/// A BC texture transcoded to ETC2: every face's mip chain, in the DDS
/// order, blocks in rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transcoded {
    pub format: Etc2Format,
    pub data: Vec<u8>,
}

/// The ETC2 bytes [`transcode`] gives for `dds`, when it is BC.
pub fn transcoded_len(dds: &Dds, format: Etc2Format) -> usize {
    (0..dds.mip_levels)
        .map(|mip| {
            let (w, h) = dds.mip_size(mip);
            (w.div_ceil(4) * h.div_ceil(4)) as usize * format.block_bytes()
        })
        .sum::<usize>()
        * dds.faces as usize
}

/// Transcodes BC texture `dds` to ETC2; `None` for an uncompressed one.
pub fn transcode(dds: &Dds) -> Option<Transcoded> {
    let mut out = Transcoded {
        format: Etc2Format::Rgb8,
        data: Vec::new(),
    };
    transcode_into(dds, &mut out).then_some(out)
}

/// [`transcode`] into `out`, whose buffer is reused when it has the room
/// (many textures transcoded through one keep their bytes in one block of
/// the wasm heap, #357); whether `dds` is BC.
pub fn transcode_into(dds: &Dds, out: &mut Transcoded) -> bool {
    let levels = || (0..dds.faces).flat_map(|f| (0..dds.mip_levels).map(move |m| (f, m)));
    let format = if dds.format == Format::Bc1 {
        let transparent = levels().any(|(f, m)| {
            Etc2Format::for_bc(dds.format, dds.level(f, m)) == Some(Etc2Format::Rgb8A1)
        });
        if transparent {
            Etc2Format::Rgb8A1
        } else {
            Etc2Format::Rgb8
        }
    } else {
        match Etc2Format::for_bc(dds.format, &[]) {
            Some(format) => format,
            None => return false,
        }
    };
    let len = transcoded_len(dds, format);
    out.format = format;
    out.data.clear();
    if out.data.capacity() < len {
        // Freed before its replacement is made.
        out.data = Vec::new();
        out.data.reserve_exact(len);
    }
    for (f, m) in levels() {
        etc2::transcode_bc_into(dds.format, dds.level(f, m), format, &mut out.data);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::dds::build::{PixelFormat, file};

    /// An 8 × 8 texture with two mip levels: 2 × 2 blocks, then one.
    fn dds(fourcc: &'static [u8; 4], block: &[u8]) -> Vec<u8> {
        let data: Vec<u8> = (0..5).flat_map(|_| block.iter().copied()).collect();
        file(8, 8, 2, false, PixelFormat::FourCc(fourcc), &data)
    }

    #[test]
    fn transcodes_every_level_into_the_format_the_texture_needs() {
        let opaque = [0x00, 0xf8, 0x1f, 0x00, 0b1110_0100, 0, 0, 0];
        let cut = [0x1f, 0x00, 0x00, 0xf8, 0b1111_1111, 0, 0, 0];
        for (bytes, format, block) in [
            (dds(b"DXT1", &opaque), Etc2Format::Rgb8, 8),
            (dds(b"DXT1", &cut), Etc2Format::Rgb8A1, 8),
            (
                dds(b"DXT5", &[opaque, opaque].concat()),
                Etc2Format::Rgba8,
                16,
            ),
        ] {
            let dds = Dds::parse(&bytes).unwrap();
            let t = transcode(&dds).unwrap();
            assert_eq!(t.format, format);
            assert_eq!(t.data.len(), 5 * block);
            assert_eq!(transcoded_len(&dds, format), 5 * block);
        }
        let rgba = file(1, 1, 0, false, PixelFormat::BGRA8, &[0, 0, 255, 255]);
        assert_eq!(transcode(&Dds::parse(&rgba).unwrap()), None);
    }

    /// A level transcoded a band of block rows at a time, as
    /// [`TextureUpload::step`] does, gives the bytes of the level transcoded
    /// whole (#440).
    #[test]
    fn bands_transcode_as_the_whole_level() {
        let mut seed = 0x9e37_79b9u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        };
        // 16 × 12 blocks, runs of equal blocks across the bands' edges.
        for (format, block, target) in [
            (Format::Bc1, 8, Etc2Format::Rgb8),
            (Format::Bc3, 16, Etc2Format::Rgba8),
        ] {
            let mut level = Vec::new();
            for i in 0..16 * 12 {
                let b: Vec<u8> = (0..block).map(|_| next()).collect();
                let copies = if i % 7 == 0 { 3 } else { 1 };
                for _ in 0..copies {
                    level.extend_from_slice(&b);
                }
            }
            level.truncate(16 * 12 * block);
            let mut whole = Vec::new();
            assert!(etc2::transcode_bc_into(format, &level, target, &mut whole));
            let row = 16 * block;
            let mut banded = Vec::new();
            for band in level.chunks(5 * row) {
                assert!(etc2::transcode_bc_into(format, band, target, &mut banded));
            }
            assert_eq!(banded, whole, "{format:?}");
        }
    }
}
