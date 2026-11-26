// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Textures from DDS files (#29) on WebGL 2: block-compressed formats
//! upload as stored where the context has BC (`WEBGL_compressed_texture_
//! s3tc`); without it they are transcoded to ETC2 where it has that (#113),
//! and decoded to RGBA8 where it has neither. Uncompressed formats convert
//! to RGBA8. A six-face file is a cube map.

use volya_engine_formats::dds::{Dds, Format};
use volya_engine_formats::etc2::{self, Etc2Format};

use crate::gl::{self, TexFormat};
use crate::world::UPLOAD_PIECE;
use crate::{Renderer, Texture};

pub use crate::shared::texture::*;

fn etc2_format(format: Etc2Format) -> TexFormat {
    match format {
        Etc2Format::Rgb8 => TexFormat::Etc2Rgb8,
        Etc2Format::Rgb8A1 => TexFormat::Etc2Rgb8A1,
        Etc2Format::Rgba8 => TexFormat::Etc2Rgba8,
    }
}

impl Renderer {
    /// How BC textures reach this context.
    pub fn texture_path(&self) -> TexturePath {
        self.texture_path
    }

    /// Makes BC textures take `path` from now on, where the context has it
    /// (tests: each path on one device); whether it does. Decoding to
    /// RGBA8 is always possible.
    pub fn set_texture_path(&mut self, path: TexturePath) -> bool {
        let e = self.gl.extensions();
        let has = match path {
            TexturePath::Bc => e.bc,
            TexturePath::Etc2 => e.etc2,
            TexturePath::Rgba8 => true,
        };
        if has {
            self.texture_path = path;
        }
        has
    }

    /// Uploads every face and mip level of `dds`, by this context's
    /// [`TexturePath`] (transcoding here when it is ETC2).
    pub fn create_texture(&self, dds: &Dds) -> Result<Texture, TextureError> {
        let mut upload = self.begin_texture(dds)?;
        upload.step(self, dds, &mut Vec::new(), u64::MAX);
        Ok(upload.finish(self))
    }

    /// Begins uploading `dds` by this context's [`TexturePath`]
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
        let etc2 = etc2_target(path, dds);
        // (format, bytes per block) of what is uploaded; RGBA8 is decoded.
        let (format, block) = match (dds.format, etc2) {
            (Format::Masked(_), _) => (TexFormat::Rgba8, None),
            (_, Some(t)) => (etc2_format(t), Some(t.block_bytes())),
            (f, None) if path == TexturePath::Bc => (
                match f {
                    Format::Bc1 => TexFormat::Bc1,
                    Format::Bc2 => TexFormat::Bc2,
                    _ => TexFormat::Bc3,
                },
                f.block_bytes(),
            ),
            (_, None) => (TexFormat::Rgba8, None),
        };
        if dds.format.block_bytes().is_some()
            && (!width.is_multiple_of(4) || !height.is_multiple_of(4))
        {
            return Err(size_error("block-compressed sizes must be multiples of 4"));
        }
        if width.max(height) > self.gl.max_texture_size() {
            return Err(size_error("larger than the device supports"));
        }
        let cube = dds.faces == 6;
        if (cube && width != height) || !(cube || dds.faces == 1) {
            return Err(size_error("not a texture or a cube map"));
        }
        let texture = self
            .gl
            .texture(format, width, height, dds.mip_levels.max(1), cube);
        Ok(TextureUpload {
            texture,
            faces: dds.faces,
            etc2,
            block,
            face: 0,
            mip: 0,
            row: 0,
        })
    }
}

/// A texture on its way to the GPU ([`Renderer::begin_texture`]), written
/// in bands of rows of at most [`UPLOAD_PIECE`] bytes. A transcoded texture is transcoded a band at a time, so no
/// more than a band of the transcoding is held (#440).
pub struct TextureUpload {
    texture: gl::Texture,
    faces: u32,
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
                    let mut data = std::mem::take(scratch);
                    let n = self.write(renderer, rows, row_bytes, &mut data);
                    *scratch = data;
                    n
                }
                (Some(_), None) => {
                    // Copied into `scratch`, which WebGL's compressed
                    // upload takes mutable.
                    let band = band(row_bytes as usize);
                    scratch.clear();
                    if scratch.capacity() < band.len() {
                        *scratch = Vec::new();
                        scratch.reserve_exact(band.len());
                    }
                    scratch.extend_from_slice(band);
                    let mut data = std::mem::take(scratch);
                    let n = self.write(renderer, rows, row_bytes, &mut data);
                    *scratch = data;
                    n
                }
                (None, _) => {
                    // Decoded whole, its bands written now.
                    let (w, h) = dds.mip_size(mip);
                    let mut rgba = dds.format.to_rgba8(level, w, h);
                    let mut n = 0;
                    while let Some((f, m, first, rows, row_bytes)) = self.next_band() {
                        if (f, m) != (face, mip) {
                            break;
                        }
                        let at = first as usize * row_bytes as usize;
                        let band = &mut rgba[at..at + rows as usize * row_bytes as usize];
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
    pub fn finish(self, _renderer: &Renderer) -> Texture {
        Texture {
            texture: self.texture,
            ycbcr: false,
        }
    }

    /// Level `mip`'s rows (of blocks, or of texels where decoded) and the
    /// bytes a row uploads.
    fn rows(&self, mip: u32) -> (u32, u32) {
        let (w, h) = self.level_size(mip);
        match self.block {
            Some(block) => (h.div_ceil(4), w.div_ceil(4) * block as u32),
            None => (h, w * 4),
        }
    }

    fn level_size(&self, mip: u32) -> (u32, u32) {
        let (w, h) = self.texture.size();
        ((w >> mip).max(1), (h >> mip).max(1))
    }

    /// The next band: face, level, first row, rows and bytes a row; `None`
    /// once all are written.
    fn next_band(&self) -> Option<(u32, u32, u32, u32, u32)> {
        (self.face < self.faces).then(|| {
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
    fn write(&mut self, renderer: &Renderer, rows: u32, row_bytes: u32, data: &mut [u8]) -> u64 {
        let (width, height) = self.level_size(self.mip);
        // Compressed bands are whole rows of blocks, clipped to the level
        // (WebGL takes a region past its edge in no case).
        let (y, h) = match self.block {
            Some(_) => {
                let y = self.row * 4;
                (y, (rows * 4).min(height - y))
            }
            None => (self.row, rows),
        };
        let bytes = u64::from(rows * row_bytes);
        renderer.gl.write_texture(
            &self.texture,
            self.mip,
            self.face,
            0,
            y,
            width,
            h,
            &mut data[..bytes as usize],
        );
        (renderer.texture_bytes).set(renderer.texture_bytes.get() + bytes);
        self.row += rows;
        if self.row >= self.rows(self.mip).0 {
            self.row = 0;
            self.mip += 1;
            if self.mip == self.texture.levels() {
                self.mip = 0;
                self.face += 1;
            }
        }
        bytes
    }
}
