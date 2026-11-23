// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Textures from DDS files (#29): block-compressed formats upload as stored
//! (docs/architecture.md, Game data); uncompressed ones convert to RGBA8.

use std::fmt;

use volya_engine_formats::dds::{Dds, Format};

use crate::{Renderer, Texture};

/// Why a texture could not be created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureError {
    /// A block-compressed texture on a device without
    /// `texture-compression-bc`.
    NoBcSupport { format: &'static str },
    /// A size the device or the format cannot hold.
    Size {
        width: u32,
        height: u32,
        reason: &'static str,
    },
}

impl fmt::Display for TextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBcSupport { format } => write!(
                f,
                "{format} texture needs the GPU feature texture-compression-bc, which this device lacks"
            ),
            Self::Size {
                width,
                height,
                reason,
            } => write!(f, "texture size {width}×{height}: {reason}"),
        }
    }
}

impl std::error::Error for TextureError {}

fn format_name(format: Format) -> &'static str {
    match format {
        Format::Bc1 => "BC1 (DXT1)",
        Format::Bc2 => "BC2 (DXT3)",
        Format::Bc3 => "BC3 (DXT5)",
        Format::Masked(_) => "uncompressed",
    }
}

impl Renderer {
    /// Uploads every face and mip level of `dds`. Cube maps become 6-layer
    /// textures; drawing uses the first layer.
    pub fn create_texture(&self, dds: &Dds) -> Result<Texture, TextureError> {
        let (width, height) = (dds.width, dds.height);
        let size_error = |reason| TextureError::Size {
            width,
            height,
            reason,
        };
        let format = match dds.format {
            Format::Bc1 => wgpu::TextureFormat::Bc1RgbaUnorm,
            Format::Bc2 => wgpu::TextureFormat::Bc2RgbaUnorm,
            Format::Bc3 => wgpu::TextureFormat::Bc3RgbaUnorm,
            Format::Masked(_) => wgpu::TextureFormat::Rgba8Unorm,
        };
        let compressed = dds.format.block_bytes().is_some();
        if compressed {
            if !self
                .device
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
            {
                return Err(TextureError::NoBcSupport {
                    format: format_name(dds.format),
                });
            }
            if !width.is_multiple_of(4) || !height.is_multiple_of(4) {
                return Err(size_error("block-compressed sizes must be multiples of 4"));
            }
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
        for face in 0..dds.faces {
            for mip in 0..dds.mip_levels {
                let (w, h) = dds.mip_size(mip);
                let level = dds.level(face, mip);
                let converted;
                let (data, row_bytes, rows, extent) = match dds.format {
                    Format::Masked(masks) => {
                        converted = masks.to_rgba8(level, w, h);
                        (converted.as_slice(), w * 4, h, (w, h))
                    }
                    format => {
                        let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
                        let block = format.block_bytes().unwrap_or_default() as u32;
                        // Copies of compressed levels cover whole blocks.
                        (level, bw * block, bh, (bw * 4, bh * 4))
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
