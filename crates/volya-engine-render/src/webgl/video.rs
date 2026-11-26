// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Video frames as textures (docs/behavior/startup.md 6.6, the pixel-shader
//! path), as wgpu's renderer makes them: the texture is RGBA8, one level,
//! the next power of two in each dimension of the picture; texel (x, y) of
//! the picture block holds the samples unconverted: red = Y', green = Cb,
//! blue = Cr, alpha = 255. Each frame's planes upload as they are and a
//! pass packs them into the texture (`yuv.wgsl`); drawing converts to RGB
//! after filtering (`ui.wgsl`, `fs_movie`).
//!
//! Texels outside the picture are never written; the spec leaves their
//! content to the graphics runtime (startup.md 9). Our choice: Y'CbCr black
//! (16, 128, 128, 255), so filtering at a picture edge blends in black.

use volya_engine_formats::video::{Frame, Plane};

use crate::gl::{self, Consts as C, Framebuffer, Gl, RasterState, TexFormat};
use crate::{Rect, Renderer, Texture};

use super::{NEAREST, link, units};

/// The pass packing a frame's planes into a video texture, shared by all
/// video textures.
pub(crate) struct Converter {
    program: gl::Program,
    sampler: gl::Sampler,
}

impl Converter {
    pub(crate) fn new(gl: &Gl) -> Self {
        Self {
            program: link(gl, "yuv", "vs", "fs", &[(0, 0), (0, 1), (0, 2)], 0),
            sampler: gl.sampler(NEAREST),
        }
    }
}

/// A texture showing video frames ([`Renderer::update_video`]).
pub struct VideoTexture {
    texture: Texture,
    framebuffer: Framebuffer,
    width: u32,
    height: u32,
    planes: Option<[gl::Texture; 3]>,
    /// A plane's rows packed tightly, where its stride is wider.
    rows: Vec<u8>,
}

impl VideoTexture {
    /// The texture to draw.
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    /// The frame's region of the texture, in texels.
    pub fn frame_rect(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32, self.height as f32)
    }
}

/// The initial content of a video texture: Y'CbCr black (our choice, see
/// the module documentation).
const OUTSIDE: [f32; 4] = [16.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0];

impl Renderer {
    /// A texture for frames whose picture is `width` × `height` pixels.
    pub fn create_video_texture(&self, width: u32, height: u32) -> VideoTexture {
        let texture = self.gl.texture(
            TexFormat::Rgba8,
            width.next_power_of_two(),
            height.next_power_of_two(),
            1,
            false,
        );
        let framebuffer = self.gl.framebuffer(Some(&texture), None);
        let size = texture.size();
        Self::begin_pass(&self.gl, &framebuffer, size, Some(OUTSIDE), None);
        crate::stats::submit();
        VideoTexture {
            texture: Texture {
                texture,
                ycbcr: true,
            },
            framebuffer,
            width,
            height,
            planes: None,
            rows: Vec::new(),
        }
    }

    /// Shows `frame` in `video`: its top-left picture-sized block
    /// (startup.md 6.6).
    ///
    /// # Panics
    ///
    /// If the frame's picture is not the size `video` was created for.
    pub fn update_video(&self, video: &mut VideoTexture, frame: &Frame) {
        let size = |p: &Plane| (p.width as u32, p.height as u32);
        let frame_planes = frame.texture_planes();
        assert_eq!(
            size(&frame_planes[0]),
            (video.width, video.height),
            "video frame size"
        );
        let stale = video.planes.as_ref().is_none_or(|planes| {
            planes
                .iter()
                .zip(&frame_planes)
                .any(|(t, p)| t.size() != size(p))
        });
        if stale {
            video.planes = Some(frame_planes.each_ref().map(|p| {
                self.gl
                    .texture(TexFormat::R8, p.width as u32, p.height as u32, 1, false)
            }));
        }
        let planes = video.planes.as_ref().expect("planes allocated");
        for (texture, plane) in planes.iter().zip(&frame_planes) {
            let (w, h) = size(plane);
            video.rows.clear();
            for row in plane.data.chunks(plane.stride).take(h as usize) {
                video.rows.extend_from_slice(&row[..w as usize]);
            }
            self.gl
                .write_texture(texture, 0, 0, 0, 0, w, h, &mut video.rows);
        }
        crate::stats::pass();
        let gl = &self.gl;
        gl.bind_draw_framebuffer(Some(&video.framebuffer));
        // Outside the viewport the texture keeps its initial content.
        gl.viewport(0, 0, video.width as i32, video.height as i32);
        gl.depth_range(0.0, 1.0);
        crate::stats::pipeline();
        gl.use_program(&self.video.program);
        gl.set_raster(&RasterState::OPAQUE);
        crate::stats::bind_group();
        for (i, plane) in planes.iter().enumerate() {
            gl.bind_texture(units::OTHER + i as u32, plane, &self.video.sampler);
        }
        gl.bind_vertex_array(&self.empty);
        crate::stats::draw();
        gl.draw_arrays(C::TRIANGLES, 0, 3, 1);
        crate::stats::submit();
    }
}
