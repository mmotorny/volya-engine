// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Textures from DDS files (#29): block-compressed formats upload as stored
//! when the GPU has BC (docs/architecture/rendering.md); without it they
//! are transcoded to ETC2 where the GPU has that (#113,
//! `volya_engine_formats::etc2`), and decoded to RGBA8 where it has neither.
//! Uncompressed formats convert to RGBA8.

use volya_engine_formats::dds::{Dds, Format};
use volya_engine_formats::etc2::{self, Etc2Format};

use crate::world::UPLOAD_PIECE;
use crate::{Renderer, Texture};

pub use crate::shared::texture::*;

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
        let mut upload = self.begin_texture(dds)?;
        upload.step(self, dds, &mut Vec::new(), u64::MAX);
        Ok(upload.finish(self))
    }

    /// Begins uploading `dds` by this device's [`TexturePath`]
    /// ([`Renderer::create_texture`] in steps): the texture is made, empty,
    /// and filled by [`TextureUpload::step`], so a platform can spread the
    /// writes and wait between them (#440).
    pub fn begin_texture(&self, dds: &Dds) -> Result<TextureUpload, TextureError> {
        let (width, height) = (dds.width, dds.height);
        let size_error = |reason| TextureError::Size {
            width,
            height,
            reason,
        };
        let path = self.texture_path();
        let etc2 = match (path, dds.format) {
            (TexturePath::Etc2, Format::Bc1) => {
                let transparent = (0..dds.faces)
                    .flat_map(|f| (0..dds.mip_levels).map(move |m| (f, m)))
                    .any(|(f, m)| {
                        Etc2Format::for_bc(dds.format, dds.level(f, m)) == Some(Etc2Format::Rgb8A1)
                    });
                Some(if transparent {
                    Etc2Format::Rgb8A1
                } else {
                    Etc2Format::Rgb8
                })
            }
            (TexturePath::Etc2, f) => Etc2Format::for_bc(f, &[]),
            _ => None,
        };
        // (format, bytes per block) of what is uploaded; RGBA8 is decoded.
        let (format, block) = match (dds.format, etc2) {
            (Format::Masked(_), _) => (wgpu::TextureFormat::Rgba8Unorm, None),
            (_, Some(t)) => (etc2_format(t), Some(t.block_bytes())),
            (f, None) if path == TexturePath::Bc => (
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
        Ok(TextureUpload {
            texture,
            etc2,
            block,
            face: 0,
            mip: 0,
            row: 0,
        })
    }
}

/// A texture on its way to the GPU ([`Renderer::begin_texture`]), written
/// in bands of rows of at most [`UPLOAD_PIECE`] bytes: wgpu stages each
/// write whole, in the wasm heap on its GL backend until WebGL reports it
/// done, where a large one needs new memory once the heap is fragmented
/// (#357). A transcoded texture is transcoded a band at a time, so no more
/// than a band of the transcoding is held (#440).
pub struct TextureUpload {
    texture: wgpu::Texture,
    /// The ETC2 format transcoded to, if any.
    etc2: Option<Etc2Format>,
    /// Bytes per block uploaded; `None` where levels are decoded to RGBA8.
    block: Option<usize>,
    /// The next band's face, level and first row (of blocks, or of texels
    /// where decoded).
    face: u32,
    mip: u32,
    row: u32,
}

impl TextureUpload {
    /// Writes bands of `dds` (the file it was begun with) while they fit in
    /// `budget` bytes, at least one, transcoding each into `scratch` where
    /// the texture is transcoded (a level decoded to RGBA8 is written
    /// whole). Returns whether every band is written.
    pub fn step(
        &mut self,
        renderer: &Renderer,
        dds: &Dds,
        scratch: &mut Vec<u8>,
        budget: u64,
    ) -> bool {
        let mut spent = 0u64;
        while let Some((face, mip, first, rows, row_bytes)) = self.next_band() {
            if spent > 0 && spent + u64::from(rows * row_bytes) > budget {
                return false;
            }
            let level = dds.level(face, mip);
            let band = |row_bytes: usize| {
                &level[first as usize * row_bytes..(first + rows) as usize * row_bytes]
            };
            spent += match (self.block, self.etc2) {
                (Some(block), Some(etc2)) => {
                    let blocks_per_row = row_bytes as usize / block;
                    let stored = blocks_per_row * dds.format.block_bytes().unwrap_or(0);
                    let n = rows as usize * row_bytes as usize;
                    scratch.clear();
                    if scratch.capacity() < n {
                        // Freed before its replacement is made.
                        *scratch = Vec::new();
                        scratch.reserve_exact(n);
                    }
                    etc2::transcode_bc_into(dds.format, band(stored), etc2, scratch);
                    // Not BC after all: the band is left blank.
                    scratch.resize(n, 0);
                    self.write(renderer, rows, row_bytes, scratch)
                }
                (Some(_), None) => self.write(renderer, rows, row_bytes, band(row_bytes as usize)),
                (None, _) => {
                    // Decoded whole, its bands written now.
                    let (w, h) = dds.mip_size(mip);
                    let rgba = dds.format.to_rgba8(level, w, h);
                    let mut n = 0;
                    while let Some((f, m, first, rows, row_bytes)) = self.next_band() {
                        if (f, m) != (face, mip) {
                            break;
                        }
                        let at = first as usize * row_bytes as usize;
                        let band = &rgba[at..at + rows as usize * row_bytes as usize];
                        n += self.write(renderer, rows, row_bytes, band);
                    }
                    n
                }
            };
        }
        true
    }

    /// The bytes the next band writes; `None` once all are written.
    pub fn next_bytes(&self) -> Option<u64> {
        (self.next_band()).map(|(_, _, _, rows, row_bytes)| u64::from(rows * row_bytes))
    }

    /// Whether it is transcoded to ETC2.
    pub fn transcoded(&self) -> bool {
        self.etc2.is_some()
    }

    /// The texture, once every band is written.
    pub fn finish(self, renderer: &Renderer) -> Texture {
        renderer.wrap(self.texture)
    }

    /// Level `mip`'s rows (of blocks, or of texels where decoded) and the
    /// bytes a row uploads.
    fn rows(&self, mip: u32) -> (u32, u32) {
        let size = self.texture.size();
        let (w, h) = ((size.width >> mip).max(1), (size.height >> mip).max(1));
        match self.block {
            Some(block) => (h.div_ceil(4), w.div_ceil(4) * block as u32),
            None => (h, w * 4),
        }
    }

    /// The next band: face, level, first row, rows and bytes a row; `None`
    /// once all are written.
    fn next_band(&self) -> Option<(u32, u32, u32, u32, u32)> {
        let size = self.texture.size();
        (self.face < size.depth_or_array_layers).then(|| {
            let (rows, row_bytes) = self.rows(self.mip);
            let band = (UPLOAD_PIECE as u32 / row_bytes).clamp(1, rows);
            (
                self.face,
                self.mip,
                self.row,
                band.min(rows - self.row),
                row_bytes,
            )
        })
    }

    /// Writes the next band, `rows` rows of `row_bytes` from `data`, and
    /// moves on past it; the bytes written.
    fn write(&mut self, renderer: &Renderer, rows: u32, row_bytes: u32, data: &[u8]) -> u64 {
        let size = self.texture.size();
        let width = (size.width >> self.mip).max(1);
        // Copies of compressed levels cover whole blocks.
        let (width, row_height) = match self.block {
            Some(_) => (width.div_ceil(4) * 4, 4),
            None => (width, 1),
        };
        renderer.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: self.mip,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: self.row * row_height,
                    z: self.face,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &data[..(rows * row_bytes) as usize],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_bytes),
                rows_per_image: Some(rows),
            },
            wgpu::Extent3d {
                width,
                height: rows * row_height,
                depth_or_array_layers: 1,
            },
        );
        let bytes = u64::from(rows * row_bytes);
        (renderer.texture_bytes).set(renderer.texture_bytes.get() + bytes);
        self.row += rows;
        if self.row >= self.rows(self.mip).0 {
            self.row = 0;
            self.mip += 1;
            if self.mip == self.texture.mip_level_count() {
                self.mip = 0;
                self.face += 1;
            }
        }
        bytes
    }
}
