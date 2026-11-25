// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Textures from DDS files (#29): block-compressed formats upload as stored
//! when the GPU has BC (docs/architecture.md, Game data); without it they
//! are transcoded to ETC2 where the GPU has that (#113,
//! `volya_engine_formats::etc2`), and decoded to RGBA8 where it has neither.
//! Uncompressed formats convert to RGBA8.

use std::fmt;

use volya_engine_formats::dds::{Dds, Format};
use volya_engine_formats::etc2::{self, Etc2Format};

use crate::world::UPLOAD_PIECE;
use crate::{Renderer, Texture};

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
    /// The path for a device with `features`.
    pub fn for_features(features: wgpu::Features) -> Self {
        if features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
            Self::Bc
        } else if features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2) {
            Self::Etc2
        } else {
            Self::Rgba8
        }
    }

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

fn etc2_format(format: Etc2Format) -> wgpu::TextureFormat {
    match format {
        Etc2Format::Rgb8 => wgpu::TextureFormat::Etc2Rgb8Unorm,
        Etc2Format::Rgb8A1 => wgpu::TextureFormat::Etc2Rgb8A1Unorm,
        Etc2Format::Rgba8 => wgpu::TextureFormat::Etc2Rgba8Unorm,
    }
}

impl Renderer {
    /// How BC textures reach this device.
    pub fn texture_path(&self) -> TexturePath {
        TexturePath::for_features(self.device.features())
    }

    /// Uploads every face and mip level of `dds`, by this device's
    /// [`TexturePath`] (transcoding here when it is ETC2). Cube maps become
    /// 6-layer textures; drawing uses the first layer.
    pub fn create_texture(&self, dds: &Dds) -> Result<Texture, TextureError> {
        let transcoded = match self.texture_path() {
            TexturePath::Etc2 => transcode(dds),
            _ => None,
        };
        self.upload(dds, transcoded.as_ref())
    }

    /// Uploads `dds` from its ETC2 transcoding `transcoded` (made by
    /// [`transcode`], perhaps earlier and stored). The device must have
    /// ETC2.
    pub fn create_transcoded_texture(
        &self,
        dds: &Dds,
        transcoded: &Transcoded,
    ) -> Result<Texture, TextureError> {
        if transcoded.data.len() != transcoded_len(dds, transcoded.format) {
            return Err(TextureError::Transcoded);
        }
        self.upload(dds, Some(transcoded))
    }

    fn upload(&self, dds: &Dds, transcoded: Option<&Transcoded>) -> Result<Texture, TextureError> {
        let (width, height) = (dds.width, dds.height);
        let size_error = |reason| TextureError::Size {
            width,
            height,
            reason,
        };
        let bc = self.texture_path() == TexturePath::Bc;
        // (format, bytes per block) of what is uploaded; RGBA8 is decoded.
        let (format, block) = match (dds.format, transcoded) {
            (Format::Masked(_), _) => (wgpu::TextureFormat::Rgba8Unorm, None),
            (_, Some(t)) => (etc2_format(t.format), Some(t.format.block_bytes())),
            (f, None) if bc => (
                match f {
                    Format::Bc1 => wgpu::TextureFormat::Bc1RgbaUnorm,
                    Format::Bc2 => wgpu::TextureFormat::Bc2RgbaUnorm,
                    _ => wgpu::TextureFormat::Bc3RgbaUnorm,
                },
                f.block_bytes(),
            ),
            (_, None) => (wgpu::TextureFormat::Rgba8Unorm, None),
        };
        if dds.format.block_bytes().is_some()
            && (!width.is_multiple_of(4) || !height.is_multiple_of(4))
        {
            return Err(size_error("block-compressed sizes must be multiples of 4"));
        }
        let limits = self.device.limits();
        if width.max(height) > limits.max_texture_dimension_2d
            || dds.faces > limits.max_texture_array_layers
        {
            return Err(size_error("larger than the device supports"));
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dds"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: dds.faces,
            },
            mip_level_count: dds.mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut at = 0;
        for face in 0..dds.faces {
            for mip in 0..dds.mip_levels {
                let (w, h) = dds.mip_size(mip);
                let level = dds.level(face, mip);
                let converted;
                let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
                // Copies of compressed levels cover whole blocks.
                let (data, row_bytes, rows, extent) = match (block, transcoded) {
                    (Some(block), Some(t)) => {
                        let n = (bw * bh) as usize * block;
                        let data = &t.data[at..at + n];
                        at += n;
                        (data, bw * block as u32, bh, (bw * 4, bh * 4))
                    }
                    (Some(block), None) => (level, bw * block as u32, bh, (bw * 4, bh * 4)),
                    (None, _) => {
                        converted = dds.format.to_rgba8(level, w, h);
                        (converted.as_slice(), w * 4, h, (w, h))
                    }
                };
                self.texture_bytes
                    .set(self.texture_bytes.get() + data.len() as u64);
                // Written in bands of rows of at most [`UPLOAD_PIECE`]
                // bytes: wgpu stages each write whole, in the wasm heap on
                // its GL backend, where a large one needs new memory once
                // the heap is fragmented (#357).
                let row_height = extent.1 / rows;
                let band = (UPLOAD_PIECE as u32 / row_bytes).clamp(1, rows);
                for first in (0..rows).step_by(band as usize) {
                    let n = band.min(rows - first);
                    let start = (first * row_bytes) as usize;
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: mip,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: first * row_height,
                                z: face,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        &data[start..start + (n * row_bytes) as usize],
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(row_bytes),
                            rows_per_image: Some(n),
                        },
                        wgpu::Extent3d {
                            width: extent.0,
                            height: n * row_height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }
        Ok(self.wrap(texture))
    }
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
}
