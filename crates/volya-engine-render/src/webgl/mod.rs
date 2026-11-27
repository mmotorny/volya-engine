// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The renderer over the GL layer (`crate::gl`; docs/architecture/
//! rendering.md, The GL renderer). The programs flip positions in y, so a
//! texture's first row is the picture's top. Every pass draws into the renderer's own frame, which
//! is then copied, flipped, into the [`Destination`].
//!
//! Colour values pass through unchanged (no sRGB conversion).

pub mod billboards;
pub mod details;
pub mod particles;
pub mod present;
pub(crate) mod ramp;
pub mod sky;
pub(crate) mod texture;
pub(crate) mod video;
pub mod world;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::ops::Range;
use std::rc::Rc;

use web_sys::WebGlFramebuffer;

use crate::gl::{
    self, Attribute, Blend, Buffer, BufferKind, Consts as C, Filter, Framebuffer, Gl, Owner,
    RasterState, SamplerDesc, TexFormat, VertexArray, VertexFormat, VertexLayout, Wrap,
};
use crate::glsl;
use crate::stats;

pub use texture::{
    TextureError, TexturePath, TextureUpload, Transcoded, transcode, transcode_into, transcoded_len,
};
pub use video::VideoTexture;
pub use world::{World, WorldDraw, WorldFrame};

pub use crate::shared::ui::*;

/// A clear colour, RGBA in 0 … 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    fn rgba(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a].map(|c| c as f32)
    }
}

/// Where a finished frame goes (rendering.md, The GL layer).
#[derive(Clone, Copy, Debug)]
pub enum Destination<'a> {
    /// The canvas's default framebuffer, `size` pixels.
    Canvas((u32, u32)),
    /// A framebuffer made outside the renderer (an XR layer's), the frame
    /// shown within `viewport` (x, y, width, height in its pixels).
    Framebuffer(&'a WebGlFramebuffer, [i32; 4]),
    /// Nowhere: the frame is drawn and kept, not shown (the warm-up).
    Hidden((u32, u32)),
}

impl Destination<'_> {
    /// Its size in pixels.
    pub fn size(&self) -> (u32, u32) {
        match *self {
            Self::Canvas(size) | Self::Hidden(size) => size,
            Self::Framebuffer(_, [_, _, w, h]) => (w.max(0) as u32, h.max(0) as u32),
        }
    }
}

/// A texture the renderer can draw: cheap to clone (a handle).
#[derive(Clone, Debug)]
pub struct Texture {
    texture: gl::Texture,
    /// Holds Y'CbCr, converted when drawn (a [`VideoTexture`]); drawn with
    /// the movie sampler.
    ycbcr: bool,
}

impl Texture {
    /// Allocated size in texels; texture regions are relative to it.
    pub fn size(&self) -> (u32, u32) {
        self.texture.size()
    }

    /// The underlying texture (every face and mip level).
    pub fn gl(&self) -> &gl::Texture {
        &self.texture
    }

    /// Its mip levels.
    pub fn mip_levels(&self) -> u32 {
        self.texture.levels()
    }

    /// Its layers: 6 for a cube map, else 1.
    pub fn layers(&self) -> u32 {
        if self.texture.is_cube() { 6 } else { 1 }
    }

    /// Its format, by name: `bc1`, `bc2`, `bc3`, `etc2-rgb8`,
    /// `etc2-rgb8a1`, `etc2-rgba8`, `rgba8`, `bgra8` or `other` (the same
    /// names by either backend, for tests).
    pub fn format_name(&self) -> &'static str {
        match self.texture.format() {
            TexFormat::Bc1 => "bc1",
            TexFormat::Bc2 => "bc2",
            TexFormat::Bc3 => "bc3",
            TexFormat::Etc2Rgb8 => "etc2-rgb8",
            TexFormat::Etc2Rgb8A1 => "etc2-rgb8a1",
            TexFormat::Etc2Rgba8 => "etc2-rgba8",
            TexFormat::Rgba8 => "rgba8",
            _ => "other",
        }
    }

    /// Whether `other` is the same texture drawn the same way.
    fn same(&self, other: &Texture) -> bool {
        self.texture.same(&other.texture) && self.ycbcr == other.ycbcr
    }
}

/// A colour texture with its framebuffer: what a pass draws into.
#[derive(Clone)]
pub(crate) struct Target {
    pub(crate) texture: Texture,
    pub(crate) framebuffer: Framebuffer,
}

impl Target {
    /// A colour target of `size`, RGBA8, counted as the video mode's.
    pub(crate) fn new(gl: &Gl, size: (u32, u32)) -> Self {
        let texture = gl.owning(Owner::Targets, || {
            gl.texture(TexFormat::Rgba8, size.0.max(1), size.1.max(1), 1, false)
        });
        let framebuffer = gl.framebuffer(Some(&texture), None);
        Target {
            texture: Texture {
                texture,
                ycbcr: false,
            },
            framebuffer,
        }
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        self.texture.size()
    }
}

/// The texture units programs read on, fixed across programs so that what
/// stays bound from one program to the next (the world's records and a
/// surface's maps) is not bound again.
pub(crate) mod units {
    /// The world's per-draw records (its vertex programs).
    pub const RECORDS: u32 = 0;
    /// A surface's second texture (light map), hemi map and environment.
    pub const SECOND: u32 = 1;
    pub const HEMI: u32 = 2;
    pub const ENV: u32 = 3;
    /// A surface's base and detail textures.
    pub const BASE: u32 = 4;
    pub const DETAIL: u32 = 5;
    /// The light passes' projected pattern and falloff.
    pub const LIGHT_MAP: u32 = 6;
    pub const LIGHT_ATT: u32 = 7;
    /// The projected light texture (object-projected-light.md).
    pub const PROJECTOR: u32 = 8;
    /// Every other program's textures, from here in binding order.
    pub const OTHER: u32 = 9;
}

/// The uniform buffer binding points, one per program family.
pub(crate) mod points {
    pub const WORLD: u32 = 0;
    pub const LIGHT: u32 = 1;
    pub const PARTICLES: u32 = 2;
    pub const SKY: u32 = 3;
    pub const DETAILS: u32 = 4;
    pub const POST: u32 = 5;
    pub const RAMP: u32 = 6;
    pub const BILLBOARDS: u32 = 7;
}

/// Links `module`'s entry points `vs` and `fs`, its textures on units from
/// `units::OTHER` in the order of `bindings` (WGSL (group, binding)) and its
/// uniform buffer on `point`.
pub(crate) fn link(
    gl: &Gl,
    module: &str,
    vs: &str,
    fs: &str,
    bindings: &[(u32, u32)],
    point: u32,
) -> gl::Program {
    link_with(
        gl,
        module,
        vs,
        fs,
        |b| units::OTHER + bindings.iter().position(|&x| x == b).unwrap_or(0) as u32,
        |_| point,
    )
}

/// Links `module`'s `vs` and `fs` with `unit` and `point` as
/// [`Gl::program`] takes them. A program that doesn't link is a bug in the
/// build's translation, fatal.
pub(crate) fn link_with(
    gl: &Gl,
    module: &str,
    vs: &str,
    fs: &str,
    unit: impl Fn((u32, u32)) -> u32,
    point: impl Fn((u32, u32)) -> u32,
) -> gl::Program {
    let stage = |entry: &str| {
        glsl::stage(module, entry).unwrap_or_else(|| panic!("no program {module} {entry}"))
    };
    gl.program(stage(vs), stage(fs), unit, point)
        .unwrap_or_else(|e| panic!("program {module} {vs} {fs}: {e}"))
}

/// The UI's vertex layout: position, texture coordinates, colour.
const UI_LAYOUT: VertexLayout = VertexLayout {
    stride: VERTEX_BYTES as u32,
    instanced: false,
    attributes: &[
        Attribute {
            location: 0,
            format: VertexFormat::Float32x2,
            offset: 0,
        },
        Attribute {
            location: 1,
            format: VertexFormat::Float32x2,
            offset: 8,
        },
        Attribute {
            location: 2,
            format: VertexFormat::Unorm8x4,
            offset: 16,
        },
    ],
};

/// The UI's states: straight (non-premultiplied) alpha, also the movie
/// shader's blending (startup.md 6.6).
const UI_RASTER: RasterState = RasterState {
    blend: Some(Blend::ALPHA),
    ..RasterState::OPAQUE
};

/// Bilinear within a level, nearest level; edges clamped.
pub(crate) const LINEAR_CLAMPED: SamplerDesc = SamplerDesc {
    mag: Filter::Linear,
    min: Filter::Linear,
    mip: Filter::Nearest,
    wrap: [Wrap::Clamp; 3],
    anisotropy: 1,
    top_level: false,
};

/// Textures read with `textureLoad` (no filtering).
pub(crate) const NEAREST: SamplerDesc = SamplerDesc {
    mag: Filter::Nearest,
    min: Filter::Nearest,
    mip: Filter::Nearest,
    wrap: [Wrap::Clamp; 3],
    anisotropy: 1,
    top_level: false,
};

/// The UI's vertices: a buffer for `quads` quads, with its vertex array.
struct UiVertices {
    buffer: Buffer,
    array: VertexArray,
}

impl UiVertices {
    fn new(gl: &Gl, quads: u64) -> Self {
        gl.owning(Owner::Renderer, || {
            let buffer = gl.buffer(
                BufferKind::Dynamic,
                quads * VERTICES_PER_QUAD as u64 * VERTEX_BYTES,
            );
            let array = gl.vertex_array(&[(&buffer, 0, &UI_LAYOUT)], None);
            Self { buffer, array }
        })
    }
}

pub struct Renderer {
    gl: Rc<Gl>,
    ui: gl::Program,
    /// For Y'CbCr video textures.
    movie: gl::Program,
    sampler: gl::Sampler,
    /// For video textures.
    movie_sampler: gl::Sampler,
    vertices: UiVertices,
    /// A vertex array without attributes, for programs that make their
    /// vertices from their index.
    empty: VertexArray,
    video: video::Converter,
    /// The world's programs (made with the renderer, again when the trees'
    /// blending changes).
    world_programs: Option<world::WorldPrograms>,
    /// The light passes' attenuation textures, once given.
    light_textures: Option<world::LightTextures>,
    /// The draws the last frame's light passes drew.
    light_draws: usize,
    /// The world pass's depth buffer, sized to the latest target.
    depth: Option<gl::Texture>,
    /// The latest target drawn straight into with that depth buffer.
    world_framebuffer: Option<Framebuffer>,
    /// The frame every pass ends in, shown in the destination.
    frame: Option<Target>,
    /// The frame [`Renderer::render_scaled`] draws into.
    offscreen: Option<Target>,
    /// Textures made ahead of their first use, by source path
    /// ([`Renderer::add_prepared`]).
    prepared: RefCell<HashMap<String, Texture>>,
    /// Textures kept while the game type stays, by source path
    /// ([`Renderer::keep_texture`]): the prefetched models'.
    kept: RefCell<HashMap<String, Texture>>,
    /// Bytes of texture data written so far.
    texture_bytes: Cell<u64>,
    /// Bytes of world geometry written so far.
    geometry_bytes: Cell<u64>,
    /// The particles' programs and buffers (frame-presentation.md 3.1).
    particles: particles::ParticlePrograms,
    /// The off-screen target and the presentation quad
    /// (frame-presentation.md).
    post: present::PostProcess,
    /// Detail objects' programs and the level's models (detail-objects.md).
    details: details::DetailPrograms,
    /// The billboard groups' programs and the level's billboards
    /// (static-shading.md 8.4).
    billboards: billboards::BillboardPrograms,
    /// The display's gamma ramp (display-gamma.md).
    ramp: ramp::RampPass,
    /// [`Renderer::set_unblended_trees`].
    unblended_trees: bool,
    /// [`Renderer::set_texture_path`].
    texture_path: TexturePath,
}

impl Renderer {
    /// A renderer drawing through `gl`.
    pub fn new(gl: Rc<Gl>) -> Self {
        let g = &*gl;
        let ui = link(g, "ui", "vs", "fs", &[(0, 0)], 0);
        let movie = link(g, "ui", "vs", "fs_movie", &[(0, 0)], 0);
        // Video textures: wrap addressing, linear filtering (startup.md
        // 6.6). Anisotropic filtering, which the original enables, is left
        // off (our choice): these quads are scaled nearly evenly, where it
        // makes no visible difference.
        let movie_sampler = g.sampler(SamplerDesc {
            wrap: [Wrap::Repeat, Wrap::Repeat, Wrap::Clamp],
            ..LINEAR_CLAMPED
        });
        let extensions = g.extensions();
        let texture_path = if extensions.bc {
            TexturePath::Bc
        } else if extensions.etc2 {
            TexturePath::Etc2
        } else {
            TexturePath::Rgba8
        };
        // What the renderer makes for itself is kept across games.
        let mut renderer = g.owning(Owner::Renderer, || Self {
            ui,
            movie,
            sampler: g.sampler(LINEAR_CLAMPED),
            movie_sampler,
            vertices: UiVertices::new(g, 64),
            empty: g.vertex_array(&[], None),
            video: video::Converter::new(g),
            world_programs: None,
            light_textures: None,
            light_draws: 0,
            depth: None,
            world_framebuffer: None,
            frame: None,
            offscreen: None,
            prepared: Default::default(),
            kept: Default::default(),
            texture_bytes: Default::default(),
            geometry_bytes: Default::default(),
            // Made with the renderer, before any level: what lives across
            // levels is allocated below their data.
            particles: particles::ParticlePrograms::new(g),
            post: present::PostProcess::new(g),
            details: details::DetailPrograms::new(g),
            billboards: billboards::BillboardPrograms::new(g),
            ramp: ramp::RampPass::new(g),
            unblended_trees: false,
            texture_path,
            gl: gl.clone(),
        });
        renderer.make_world_programs();
        renderer
    }

    /// The GL layer it draws through.
    pub fn gl(&self) -> &Gl {
        &self.gl
    }

    /// The WebGL calls made since the last call, by kind.
    pub fn take_gl_counts(&self) -> gl::Counts {
        self.gl.take_counts()
    }

    /// The largest texture side the context takes.
    pub fn max_texture_size(&self) -> u32 {
        self.gl.max_texture_size()
    }

    /// A fence after everything drawn so far.
    pub fn fence(&self) -> gl::Fence {
        self.gl.fence()
    }

    /// Grows the particles' vertex buffer to hold `particles` sprites at
    /// once, so that no buffer is made while a level runs.
    pub fn reserve_particles(&mut self, particles: usize) {
        self.particles.reserve(&self.gl, particles);
    }

    /// The particles' vertex buffer's size in bytes.
    pub fn particle_buffer_size(&self) -> u64 {
        self.particles.buffer_size()
    }

    /// Keeps `texture`, made from the file at `path`, for
    /// [`Renderer::take_prepared`].
    pub fn add_prepared(&self, path: &str, texture: Texture) {
        self.prepared.borrow_mut().insert(path.to_owned(), texture);
    }

    /// Keeps `texture`, made from the file at `path`, for every level
    /// that draws it while the renderer lives ([`Renderer::kept_texture`]).
    pub fn keep_texture(&self, path: &str, texture: Texture) {
        self.kept.borrow_mut().insert(path.to_owned(), texture);
    }

    /// The texture kept for `path`, if any (it stays kept).
    pub fn kept_texture(&self, path: &str) -> Option<Texture> {
        self.kept.borrow().get(path).cloned()
    }

    /// How many textures are kept.
    pub fn kept_count(&self) -> usize {
        self.kept.borrow().len()
    }

    /// The buffers, textures and vertex arrays its context holds now
    /// (every renderer on the context, the prepared textures included).
    pub fn gpu_live(&self) -> crate::gl::Live {
        self.gl.live()
    }

    /// Those of [`Renderer::gpu_live`] the renderer keeps across games
    /// (every [`Owner`]'s but the level's).
    pub fn gpu_kept(&self) -> crate::gl::Live {
        self.gl.kept()
    }

    /// Those of [`Renderer::gpu_live`] made for `owner`.
    pub fn gpu_live_of(&self, owner: Owner) -> crate::gl::Live {
        self.gl.live_of(owner)
    }

    /// Runs `f`, counting the textures it makes as `owner`'s (else they
    /// are the level's): the presenter's menu images, for one.
    pub fn owning<R>(&self, owner: Owner, f: impl FnOnce() -> R) -> R {
        self.gl.owning(owner, f)
    }

    /// Bytes of texture data written since the renderer was made (WebGL
    /// copies each write at the call: none is held).
    pub fn texture_bytes_written(&self) -> u64 {
        self.texture_bytes.get()
    }

    /// Bytes of the worlds' geometry written since the renderer was made,
    /// for the diagnostic log (#443).
    pub fn geometry_bytes_written(&self) -> u64 {
        self.geometry_bytes.get()
    }

    /// The texture prepared for `path`, if any (it is then forgotten).
    pub fn take_prepared(&self, path: &str) -> Option<Texture> {
        self.prepared.borrow_mut().remove(path)
    }

    /// Hands the uploads written so far to the GPU: WebGL copies them at
    /// each call, so nothing waits in the wasm heap (WebGL 2
    /// specification).
    pub fn flush_uploads(&self) {
        stats::submit();
        self.gl.flush();
    }

    /// How many prepared textures are held.
    pub fn prepared_count(&self) -> usize {
        self.prepared.borrow().len()
    }

    /// Forgets every prepared texture.
    pub fn clear_prepared(&self) {
        self.prepared.borrow_mut().clear();
    }

    /// An RGBA texture (8 bits per channel, rows top to bottom), one level.
    pub fn create_rgba_texture(&self, width: u32, height: u32, rgba: &[u8]) -> Texture {
        let texture = self.gl.texture(TexFormat::Rgba8, width, height, 1, false);
        self.gl
            .write_texture(&texture, 0, 0, 0, 0, width, height, &mut rgba.to_vec());
        Texture {
            texture,
            ycbcr: false,
        }
    }

    /// A colour target of `size`, RGBA8.
    fn make_target(&self, size: (u32, u32)) -> Target {
        Target::new(&self.gl, size)
    }

    /// The frame shown in a destination of `size`.
    fn frame_target(&mut self, size: (u32, u32)) -> Target {
        if self.frame.as_ref().is_none_or(|t| t.size() != size) {
            self.frame = None;
            self.frame = Some(self.make_target(size));
        }
        self.frame.clone().expect("frame")
    }

    /// The frame drawn into before it is scaled or shown through the ramp,
    /// `size` pixels.
    fn offscreen_frame(&mut self, size: (u32, u32)) -> Target {
        if self.offscreen.as_ref().is_none_or(|t| t.size() != size) {
            self.offscreen = None;
            self.offscreen = Some(self.make_target(size));
        }
        self.offscreen.clone().expect("offscreen frame")
    }

    /// Copies `frame` into `destination`, flipped (the frame's first row is
    /// the picture's top; a framebuffer's is its bottom).
    fn show(&self, frame: &Target, destination: &Destination) {
        let (w, h) = frame.size();
        match *destination {
            Destination::Canvas(_) => {
                self.gl.bind_draw_framebuffer(None);
                self.gl
                    .blit(&frame.framebuffer, w, h, [0, 0, w as i32, h as i32], true);
            }
            Destination::Framebuffer(fb, viewport) => {
                self.gl.bind_draw_external(fb);
                self.gl.blit(&frame.framebuffer, w, h, viewport, true);
            }
            Destination::Hidden(_) => {}
        }
    }

    /// Draws `list` into `destination` (cleared to `clear` first, or to
    /// the world's clear colour when it has a world); through the list's
    /// gamma ramp, if any.
    pub fn render(&mut self, destination: &Destination, list: &DrawList, clear: Color) {
        let frame = self.frame_target(destination.size());
        self.draw_ramped(&frame, list, clear);
        self.show(&frame, destination);
    }

    /// [`Renderer::render`] into `target`.
    fn draw_ramped(&mut self, target: &Target, list: &DrawList, clear: Color) {
        let size = target.size();
        match &list.ramp {
            None => self.draw(target, list, clear),
            Some(table) => {
                let frame = self.offscreen_frame(size);
                self.draw(&frame, list, clear);
                let rect = [0.0, 0.0, size.0 as f32, size.1 as f32];
                self.ramp
                    .draw(&self.gl, &self.empty, &frame, target, rect, table);
                stats::submit();
            }
        }
    }

    /// Binds `target` for a pass: its whole size, depth range 0 … 1, and
    /// clears its colour to `clear` and its depth to `depth`, each if
    /// given.
    pub(crate) fn begin_pass(
        gl: &Gl,
        framebuffer: &Framebuffer,
        size: (u32, u32),
        clear: Option<[f32; 4]>,
        depth: Option<f32>,
    ) {
        stats::pass();
        gl.bind_draw_framebuffer(Some(framebuffer));
        gl.viewport(0, 0, size.0 as i32, size.1 as i32);
        gl.depth_range(0.0, 1.0);
        if clear.is_some() || depth.is_some() {
            gl.clear(clear, depth);
        }
    }

    /// Draws `list` into `target` as [`Renderer::render`], without the
    /// ramp.
    fn draw(&mut self, target: &Target, list: &DrawList, clear: Color) {
        let mut batches = self.upload_vertices(list);
        let drawn = match (&list.world, list.lens) {
            (Some(frame), _) => self.present_frame(target, frame, &list.quads, &batches),
            (None, Some(lens)) => self.present_menu(target, lens, &list.quads, &batches, clear),
            (None, None) => false,
        };
        if !drawn {
            if let Some(lens) = list.lens.filter(|_| list.world.is_none()) {
                // No target: the lens's map is not a picture.
                let n = list.quads.len();
                batches = [0..lens.drawn, lens.drawn + 1..n]
                    .into_iter()
                    .flat_map(|r| clip_batches(&batches, r))
                    .collect();
            }
            // Over the world drawn straight into the target, else cleared.
            let clear = list.world.is_none().then(|| clear.rgba());
            Self::begin_pass(&self.gl, &target.framebuffer, target.size(), clear, None);
            self.draw_quads(&list.quads, &batches);
        }
        stats::submit();
    }

    /// A main-menu frame through the off-screen target with its lens
    /// (title-menu.md 4.6) into the `screen`. The menu asks for the
    /// post-process, so the effects count as active
    /// (frame-presentation.md 2.4): the frame goes through the target
    /// whenever it is built (3.1 step 1), and the distortion pass runs (step
    /// 3) with the lens's map alone (particle-drawing.md 6.2). The colour
    /// values are those in force (2.3). Returns whether it drew (a target
    /// was built).
    fn present_menu(
        &mut self,
        screen: &Target,
        lens: MenuLens,
        quads: &[Quad],
        batches: &[(usize, Range<u32>)],
        clear: Color,
    ) -> bool {
        let p = lens.presentation;
        self.post
            .prepare(&self.gl, screen.size(), p.supersample, p.resets);
        let Some(t) = self.post.targets.clone() else {
            return false;
        };
        let menu = clip_batches(batches, 0..lens.drawn);
        let map = clip_batches(batches, lens.drawn..lens.drawn + 1);
        let over = clip_batches(batches, lens.drawn + 1..quads.len());
        // The menu's background is opaque; the clear only keeps the target
        // defined.
        Self::begin_pass(&self.gl, &t.scene, t.size, Some(clear.rgba()), None);
        self.draw_quads(quads, &menu);
        let neutral = 127.0 / 255.0;
        Self::begin_pass(&self.gl, &t.distortion, t.size, Some([neutral; 4]), None);
        self.draw_quads(quads, &map);
        let gl = self.gl.clone();
        let (ui, movie) = (&self.ui, &self.movie);
        let samplers = [&self.sampler, &self.movie_sampler];
        let vertices = &self.vertices.array;
        let distortion = Distortion {
            pass: true,
            drawn: true,
        };
        let drawn = tracing::info_span!("render.post").in_scope(|| {
            self.post
                .draw(&gl, &self.empty, screen, &p, distortion, || {
                    draw_quads(&gl, vertices, [ui, movie], samplers, quads, &over)
                })
        });
        self.post.used = true;
        self.post.distorted = true;
        drawn
    }

    /// Draws `list` at `size` pixels and shows the result in `destination`
    /// scaled to fit with its aspect ratio kept, centred, black around it
    /// (#49). At the destination's own size it draws straight into the
    /// frame.
    pub fn render_scaled(
        &mut self,
        destination: &Destination,
        list: &DrawList,
        clear: Color,
        size: (u32, u32),
    ) {
        let frame = self.frame_target(destination.size());
        self.draw_scaled(&frame, list, clear, size);
        self.show(&frame, destination);
    }

    /// The draw calls and state changes made since the last call (#387).
    pub fn take_frame_stats(&self) -> stats::FrameStats {
        stats::take()
    }

    /// The static world's draw calls by program and kind since the last
    /// call (#387).
    pub fn take_static_calls(&self) -> stats::StaticCalls {
        stats::take_static()
    }

    /// Whether frames can be timed on the GPU: not yet with WebGL 2
    /// (`EXT_disjoint_timer_query_webgl2`, #493).
    pub fn can_time_gpu(&self) -> bool {
        false
    }

    /// Turns timing frames on the GPU on or off: nothing, as
    /// [`Renderer::can_time_gpu`].
    pub fn set_gpu_timing(&mut self, _on: bool) {}

    /// The GPU times of frames drawn: none ([`Renderer::can_time_gpu`]).
    pub fn take_gpu_times(&mut self) -> Vec<f64> {
        Vec::new()
    }

    /// The GPU times of frames' passes: none ([`Renderer::can_time_gpu`]).
    pub fn take_gpu_pass_times(&mut self) -> Vec<(&'static str, f64)> {
        Vec::new()
    }

    /// [`Renderer::render_scaled`] into `target`.
    fn draw_scaled(&mut self, target: &Target, list: &DrawList, clear: Color, size: (u32, u32)) {
        let target_size = target.size();
        if size == target_size || size.0 == 0 || size.1 == 0 {
            self.draw_ramped(target, list, clear);
            return;
        }
        let frame = self.offscreen_frame(size);
        self.draw(&frame, list, clear);
        let [x, y, w, h] = volya_engine_formats::ui::Resolution::new(size.0, size.1)
            .letterbox((target_size.0 as f32, target_size.1 as f32));
        if let Some(table) = &list.ramp {
            self.ramp
                .draw(&self.gl, &self.empty, &frame, target, [x, y, w, h], table);
            stats::submit();
            return;
        }
        let blit = DrawList {
            quads: vec![Quad::new(Rect::SCREEN, &frame.texture)],
            ..Default::default()
        };
        let batches = self.upload_vertices(&blit);
        Self::begin_pass(
            &self.gl,
            &target.framebuffer,
            target_size,
            Some(Color::BLACK.rgba()),
            None,
        );
        if w >= 1.0 && h >= 1.0 {
            // Truncated to whole pixels.
            self.gl.viewport(x as i32, y as i32, w as i32, h as i32);
            stats::buffer();
            self.gl.bind_vertex_array(&self.vertices.array);
            stats::pipeline();
            self.gl.use_program(&self.ui);
            self.gl.set_raster(&UI_RASTER);
            stats::bind_group();
            self.gl
                .bind_texture(units::OTHER, &frame.texture.texture, &self.sampler);
            for (_, vertices) in batches {
                stats::draw();
                self.gl
                    .draw_arrays(C::TRIANGLES, vertices.start, vertices.len() as u32, 1);
            }
        }
        stats::submit();
    }

    /// The level's detail models (docs/behavior/detail-objects.md), their
    /// texture, whether their shader has the alpha test, and room for
    /// `capacity` instances a frame: made with the level's world, so
    /// nothing is made while it runs.
    pub fn set_details(
        &mut self,
        models: &[details::DetailGeometry],
        texture: &Texture,
        alpha_test: bool,
        capacity: usize,
    ) {
        self.details
            .set_level(&self.gl, models, texture, alpha_test, capacity);
    }

    /// Drops the level's detail models.
    pub fn clear_details(&mut self) {
        self.details.clear_level();
    }

    /// Bytes of the detail objects' buffers on the GPU.
    pub fn details_gpu_bytes(&self) -> u64 {
        self.details.gpu_bytes()
    }

    /// The level's billboard texture sets (docs/behavior/static-shading.md
    /// 8.4: each an atlas and its `_nm`), with room for `groups`
    /// billboards a frame: made with the level's world.
    pub fn set_billboards(&mut self, sets: &[(Texture, Texture)], groups: usize) {
        self.billboards.set_level(&self.gl, sets, groups);
    }

    /// Drops the level's billboards.
    pub fn clear_billboards(&mut self) {
        self.billboards.clear_level();
    }

    /// Bytes of the billboards' buffer on the GPU.
    pub fn billboards_gpu_bytes(&self) -> u64 {
        self.billboards.gpu_bytes()
    }

    /// Builds the off-screen target for a `screen` of the game's size and
    /// the presentation's `r__supersample` when either changed, and returns
    /// to the renderer's defaults at a device reset
    /// (frame-presentation.md 1.2: at device creation and every reset);
    /// the platform calls it every frame, so it is made before a level is.
    pub fn prepare_presentation(&mut self, screen: (u32, u32), p: &crate::present::Presentation) {
        self.post.prepare(&self.gl, screen, p.supersample, p.resets);
    }

    /// The `$user$rendertarget` size and whether the last world frame went
    /// through it (tests).
    pub fn presentation_target(&self) -> Option<((u32, u32), bool)> {
        self.post.targets.as_ref().map(|t| (t.size, self.post.used))
    }

    /// The noise texture of the presentation (`fx\fx_noise2`,
    /// frame-presentation.md 3.4).
    pub fn set_presentation_noise(&mut self, texture: &Texture) {
        self.post.set_noise(texture);
    }

    /// Drops the level's presentation noise and the light passes'
    /// attenuation textures, which are the level's (memory.md rule 6).
    pub fn clear_level_textures(&mut self) {
        self.post.clear_noise();
        self.light_textures = None;
    }

    /// One world frame into the `screen` (frame-presentation.md 3.1): the
    /// world into the off-screen target and the presentation quad onto the
    /// screen with `quads` over it in the same pass, or the world straight
    /// onto the screen. Returns whether `quads` were drawn.
    fn present_frame(
        &mut self,
        screen: &Target,
        frame: &WorldFrame,
        quads: &[Quad],
        batches: &[(usize, Range<u32>)],
    ) -> bool {
        let size = screen.size();
        let p = frame.presentation;
        self.post.prepare(&self.gl, size, p.supersample, p.resets);
        // The hand-over of the frame's camera update (2.2).
        if let Some(e) = frame.effects {
            self.post.effects = e;
        }
        let through = self.post.use_target(p.supersample);
        self.post.used = through;
        let mut drawn = false;
        let distorted = match self.post.targets.clone().filter(|_| through) {
            Some(t) => {
                let distortion = tracing::info_span!("render.world")
                    .in_scope(|| self.encode_frame(&t.scene, t.size, frame, Some(&t.distortion)));
                let gl = self.gl.clone();
                let (ui, movie) = (&self.ui, &self.movie);
                let samplers = [&self.sampler, &self.movie_sampler];
                let vertices = &self.vertices.array;
                drawn = tracing::info_span!("render.post").in_scope(|| {
                    self.post
                        .draw(&gl, &self.empty, screen, &p, distortion, || {
                            draw_quads(&gl, vertices, [ui, movie], samplers, quads, batches)
                        })
                });
                distortion.pass
            }
            None => {
                let framebuffer = self.world_framebuffer(screen);
                tracing::info_span!("render.world")
                    .in_scope(|| self.encode_frame(&framebuffer, size, frame, None))
                    .pass
            }
        };
        self.post.distorted = distorted;
        drawn
    }

    /// The world pass of `frame` with its particle effects into
    /// `framebuffer` of `size`, then, with a distortion list, the
    /// distortion pass into `distort` (particle-drawing.md 6.2). Returns
    /// whether the frame had a distortion pass and whether it was drawn.
    /// Drawn straight to the screen (no `distort`), the pass's picture
    /// would not be shown (frame-presentation.md 3.1), so it is left
    /// undrawn, but the frame still counts as one with a distortion pass.
    fn encode_frame(
        &mut self,
        framebuffer: &Framebuffer,
        size: (u32, u32),
        frame: &WorldFrame,
        distort: Option<&Framebuffer>,
    ) -> Distortion {
        // The maximum anisotropy in force (texture-quality-filtering.md
        // 5); the samplers are cached by description.
        let anisotropy = frame.filtering.anisotropy.clamp(1, 16);
        if let Some(p) = &mut self.world_programs {
            p.set_anisotropy(&self.gl, anisotropy);
        }
        self.details.set_anisotropy(&self.gl, anisotropy);
        self.particles.set_anisotropy(&self.gl, anisotropy);
        let prepared = if frame.particles.is_empty() {
            None
        } else {
            let l = &frame.light;
            self.particles.prepare(
                &self.gl,
                &frame.particles,
                &frame.view,
                &frame.projection,
                (l.fog_color, l.fog_near, l.fog_far),
                frame.filtering.mip_bias,
            )
        };
        let distorted = prepared
            .as_ref()
            .is_some_and(|p| p.has(particles::ParticleQueue::Distortion));
        // A distortion list none of whose sprites reaches the screen
        // leaves the target as cleared: the presentation reads a texel of
        // the clear value instead and the pass is not drawn (#433).
        let drawn = prepared.as_ref().is_some_and(|p| p.distortion_shown);
        let shown = distort.filter(|_| distorted && drawn);
        self.encode_world(framebuffer, size, frame, prepared.as_ref(), shown.is_some());
        if let (Some(prepared), Some(distort)) = (&prepared, shown) {
            // TODO(#329): particle-drawing.md 6.2 does not state the view
            // and projection of the distortion pass; the scene's
            // (believed).
            stats::set_part(stats::Part::Particles);
            self.particles.distort(&self.gl, prepared, distort, size);
            stats::set_part(stats::Part::Screen);
        }
        Distortion {
            pass: distorted,
            drawn: shown.is_some(),
        }
    }

    /// Draws `quads` in their `batches` ([`Renderer::upload_vertices`])
    /// into the bound framebuffer.
    fn draw_quads(&self, quads: &[Quad], batches: &[(usize, Range<u32>)]) {
        draw_quads(
            &self.gl,
            &self.vertices.array,
            [&self.ui, &self.movie],
            [&self.sampler, &self.movie_sampler],
            quads,
            batches,
        );
    }

    /// Writes the vertices of `list` and returns its draw batches: runs of
    /// quads sharing a texture, as (index of a quad with that texture, vertex
    /// range).
    fn upload_vertices(&mut self, list: &DrawList) -> Vec<(usize, Range<u32>)> {
        let (data, batches) = quad_vertices(&list.quads, Texture::same);
        if data.len() as u64 > self.vertices.buffer.size() {
            let quads =
                (data.len() as u64 / (VERTEX_BYTES * VERTICES_PER_QUAD as u64)).next_power_of_two();
            self.vertices = UiVertices::new(&self.gl, quads);
        }
        self.gl.write_buffer(&self.vertices.buffer, 0, &data);
        batches
    }

    /// Renders `list` into a `width` × `height` frame, cleared to `clear`,
    /// and returns its pixels as RGBA, 8 bits per channel, rows top to
    /// bottom. Read at once (tests only); the future is ready.
    pub fn capture(
        &mut self,
        list: &DrawList,
        width: u32,
        height: u32,
        clear: Color,
    ) -> impl Future<Output = Result<Vec<u8>, String>> + use<> {
        let target = self.make_target((width, height));
        self.draw_ramped(&target, list, clear);
        std::future::ready(Ok(self.gl.read_pixels(&target.framebuffer, width, height)))
    }

    /// As [`Renderer::capture`], with `list` drawn at `size` and shown
    /// scaled to fit ([`Renderer::render_scaled`]).
    pub fn capture_scaled(
        &mut self,
        list: &DrawList,
        size: (u32, u32),
        width: u32,
        height: u32,
        clear: Color,
    ) -> impl Future<Output = Result<Vec<u8>, String>> + use<> {
        let target = self.make_target((width, height));
        self.draw_scaled(&target, list, clear, size);
        std::future::ready(Ok(self.gl.read_pixels(&target.framebuffer, width, height)))
    }
}

/// The parts of `batches` ([`Renderer::upload_vertices`]) that draw the
/// quads `range`.
fn clip_batches(batches: &[(usize, Range<u32>)], range: Range<usize>) -> Vec<(usize, Range<u32>)> {
    let per = VERTICES_PER_QUAD;
    let (start, end) = (range.start as u32 * per, range.end as u32 * per);
    batches
        .iter()
        .filter_map(|(_, r)| {
            let (a, b) = (r.start.max(start), r.end.min(end));
            (a < b).then(|| ((a / per) as usize, a..b))
        })
        .collect()
}

/// Draws `quads` in their `batches` from `vertices` into the bound
/// framebuffer, with the UI program or, for Y'CbCr textures, the movie
/// program and sampler.
fn draw_quads(
    gl: &Gl,
    vertices: &VertexArray,
    [program, movie]: [&gl::Program; 2],
    [sampler, movie_sampler]: [&gl::Sampler; 2],
    quads: &[Quad],
    batches: &[(usize, Range<u32>)],
) {
    if batches.is_empty() {
        return;
    }
    stats::buffer();
    gl.bind_vertex_array(vertices);
    gl.set_raster(&UI_RASTER);
    for (texture, range) in batches {
        let texture = &quads[*texture].texture;
        stats::pipeline();
        gl.use_program(if texture.ycbcr { movie } else { program });
        stats::bind_group();
        let s = if texture.ycbcr {
            movie_sampler
        } else {
            sampler
        };
        gl.bind_texture(units::OTHER, &texture.texture, s);
        stats::draw();
        gl.draw_arrays(C::TRIANGLES, range.start, range.len() as u32, 1);
    }
}
