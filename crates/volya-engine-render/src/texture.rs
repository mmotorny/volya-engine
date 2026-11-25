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
    let levels = || (0..dds.faces).flat_map(|f| (0..dds.mip_levels).map(move |m| (f, m)));
    let all: Vec<&[u8]> = levels().map(|(f, m)| dds.level(f, m)).collect();
    let format = if dds.format == Format::Bc1 {
        let transparent = all
            .iter()
            .any(|l| Etc2Format::for_bc(dds.format, l) == Some(Etc2Format::Rgb8A1));
        if transparent {
            Etc2Format::Rgb8A1
        } else {
            Etc2Format::Rgb8
        }
    } else {
        Etc2Format::for_bc(dds.format, &[])?
    };
    let mut data = Vec::with_capacity(transcoded_len(dds, format));
    for level in all {
        data.extend(etc2::transcode_bc(dds.format, level, format));
    }
    Some(Transcoded { format, data })
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
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: mip,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: face,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row_bytes),
                        rows_per_image: Some(rows),
                    },
                    wgpu::Extent3d {
                        width: extent.0,
                        height: extent.1,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        Ok(self.wrap(texture))
    }
}
