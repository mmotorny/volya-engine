// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's static world on WebGL 2 (docs/behavior/static-shading.md): `level.geom`'s vertex and index
//! buffers uploaded as stored, the level's index ranges rebased at upload
//! (WebGL 2 has no base vertex), one vertex array per (vertex layout,
//! vertex buffer, index buffer), the per-draw records in a texture, and a
//! shared draw's record named by the first-instance uniform (WebGL 2 has no
//! base instance), set only when it changes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::rc::Rc;

use volya_engine_formats::level_geom::{GeomBuffer, GeomLayout, GeomSource, Layout, LevelGeom};

use crate::gl::{
    self, Attribute, Blend as GlBlend, Buffer, BufferKind, Consts as C, Depth, Filter, FrontFace,
    Gl, RasterState, SamplerDesc, TexFormat, VertexArray, VertexFormat, Wrap,
};
use crate::sky::{SkyPrograms, SkyResources, SkyTextures};
use crate::{Renderer, Target, Texture};

use super::{NEAREST, link_with, points, units};

pub use crate::shared::world::*;

/// Attributes of `level.geom`'s light-mapped layout.
const LIGHT_MAPPED: [Attribute; 6] = [
    attribute(0, VertexFormat::Float32x3, 0),
    attribute(1, VertexFormat::Unorm8x4, 12),
    attribute(2, VertexFormat::Unorm8x4, 16),
    attribute(3, VertexFormat::Unorm8x4, 20),
    attribute(4, VertexFormat::Sint16x2, 24),
    attribute(5, VertexFormat::Sint16x2, 28),
];
/// The vertex-lit layout's.
const VERTEX_LIT: [Attribute; 6] = [
    attribute(0, VertexFormat::Float32x3, 0),
    attribute(1, VertexFormat::Unorm8x4, 12),
    attribute(2, VertexFormat::Unorm8x4, 16),
    attribute(3, VertexFormat::Unorm8x4, 20),
    attribute(4, VertexFormat::Unorm8x4, 24),
    attribute(5, VertexFormat::Sint16x2, 28),
];
/// The trees' layout's.
const TREE: [Attribute; 5] = [
    attribute(0, VertexFormat::Float32x3, 0),
    attribute(1, VertexFormat::Unorm8x4, 12),
    attribute(2, VertexFormat::Unorm8x4, 16),
    attribute(3, VertexFormat::Unorm8x4, 20),
    attribute(4, VertexFormat::Sint16x4, 24),
];
/// The objects' models' (level-objects.md section 4.7).
const MODEL: [Attribute; 3] = [
    attribute(0, VertexFormat::Sint16x4, 0),
    attribute(1, VertexFormat::Unorm8x4, 8),
    attribute(2, VertexFormat::Sint16x2, 12),
];
/// Each level vertex's draw (`id` in world.wgsl), from a buffer of its own.
const ID: gl::VertexLayout = gl::VertexLayout {
    stride: 4,
    instanced: false,
    attributes: &[attribute(7, VertexFormat::Uint32, 0)],
};

pub(crate) const fn attribute(location: u32, format: VertexFormat, offset: u32) -> Attribute {
    Attribute {
        location,
        format,
        offset,
    }
}

/// The vertex buffer layout a program reads.
fn vertex_layout(layout: VertexLayout) -> &'static gl::VertexLayout {
    const fn level(attributes: &'static [Attribute]) -> gl::VertexLayout {
        gl::VertexLayout {
            stride: 32,
            instanced: false,
            attributes,
        }
    }
    const L: gl::VertexLayout = level(&LIGHT_MAPPED);
    const V: gl::VertexLayout = level(&VERTEX_LIT);
    const T: gl::VertexLayout = level(&TREE);
    const M: gl::VertexLayout = gl::VertexLayout {
        stride: MODEL_VERTEX_BYTES as u32,
        instanced: false,
        attributes: &MODEL,
    };
    match layout {
        VertexLayout::Level(Layout::LightMapped) => &L,
        VertexLayout::Level(Layout::Tree) => &T,
        VertexLayout::Model => &M,
        _ => &V,
    }
}

/// The world's units: its records, a surface's maps, base and detail, the
/// light passes' textures.
fn world_unit(binding: (u32, u32)) -> u32 {
    match binding {
        (0, 1) => units::RECORDS,
        (1, 0) => units::SECOND,
        (1, 1) => units::HEMI,
        (1, 2) => units::ENV,
        (2, 0) => units::BASE,
        (2, 1) => units::DETAIL,
        (1, 6) => units::LIGHT_MAP,
        (1, 7) => units::LIGHT_ATT,
        _ => units::OTHER,
    }
}

fn world_point(binding: (u32, u32)) -> u32 {
    match binding {
        (1, 5) => points::LIGHT,
        _ => points::WORLD,
    }
}

/// Source alpha / inverse source alpha for colour and alpha alike: Direct3D
/// 9 applies the colour factors to alpha too.
pub(crate) const ALPHA_BLENDING: GlBlend = GlBlend {
    color: (C::SRC_ALPHA, C::ONE_MINUS_SRC_ALPHA, C::FUNC_ADD),
    alpha: (C::SRC_ALPHA, C::ONE_MINUS_SRC_ALPHA, C::FUNC_ADD),
};

/// The same factors and addition for colour and alpha.
pub(crate) const fn blend(src: u32, dst: u32) -> GlBlend {
    GlBlend {
        color: (src, dst, C::FUNC_ADD),
        alpha: (src, dst, C::FUNC_ADD),
    }
}

/// Depth test less-or-equal, with or without writes.
pub(crate) const fn less_equal(write: bool) -> Option<Depth> {
    Some(Depth {
        compare: C::LEQUAL,
        write,
    })
}

/// Section 4.4: s_base / s_detail wrap, anisotropic (linear min/mag/mip
/// with anisotropy); s_lmap wrap, linear; s_hemi clamp, linear, no mip.
/// The maximum anisotropy and the mip bias are those in force
/// (texture-quality-filtering.md 9; [`WorldPrograms::set_anisotropy`]).
const LINEAR_WRAPPED: SamplerDesc = SamplerDesc {
    mag: Filter::Linear,
    min: Filter::Linear,
    mip: Filter::Linear,
    wrap: [Wrap::Repeat; 3],
    anisotropy: 1,
    top_level: false,
};

/// Programs and samplers shared by every world.
pub(crate) struct WorldPrograms {
    /// [`Renderer::set_unblended_trees`].
    unblended_trees: bool,
    /// By their entry points.
    programs: HashMap<(&'static str, &'static str), gl::Program>,
    /// s_base, s_lmap, s_hemi.
    samplers: [gl::Sampler; 3],
    /// The records' (read with `textureLoad`).
    nearest: gl::Sampler,
    white: gl::Texture,
    white_cube: gl::Texture,
    /// The light passes (dynamic-lights.md 6): their programs, the
    /// attenuation sampler and the constants (one slot per pass).
    light_programs: HashMap<LightProgram, gl::Program>,
    light_sampler: gl::Sampler,
    light_constants: Buffer,
    light_slots: u64,
    /// The sky's programs (sky.md).
    pub(crate) sky: SkyPrograms,
}

impl WorldPrograms {
    pub(crate) fn new(gl: &Gl, unblended_trees: bool) -> Self {
        let white = gl.texture(TexFormat::Rgba8, 1, 1, 1, false);
        gl.write_texture(&white, 0, 0, 0, 0, 1, 1, &mut [255; 4]);
        let white_cube = gl.texture(TexFormat::Rgba8, 1, 1, 1, true);
        for face in 0..6 {
            gl.write_texture(&white_cube, 0, face, 0, 0, 1, 1, &mut [255; 4]);
        }
        let light_programs = LightProgram::ALL
            .iter()
            .map(|&p| {
                let program = link_with(
                    gl,
                    "world",
                    p.entry_point(),
                    "fs_light",
                    world_unit,
                    world_point,
                );
                (p, program)
            })
            .collect();
        Self {
            unblended_trees,
            programs: HashMap::new(),
            samplers: [
                gl.sampler(SamplerDesc {
                    anisotropy: Filtering::default().anisotropy,
                    ..LINEAR_WRAPPED
                }),
                gl.sampler(LINEAR_WRAPPED),
                gl.sampler(SamplerDesc {
                    mip: Filter::Nearest,
                    wrap: [Wrap::Clamp; 3],
                    top_level: true,
                    ..LINEAR_WRAPPED
                }),
            ],
            nearest: gl.sampler(NEAREST),
            white,
            white_cube,
            light_programs,
            // dynamic-lights.md 6.2: s_lmap and s_att clamp, linear, the
            // first level only.
            light_sampler: gl.sampler(SamplerDesc {
                mag: Filter::Linear,
                min: Filter::Linear,
                mip: Filter::Nearest,
                wrap: [Wrap::Clamp; 3],
                anisotropy: 1,
                top_level: true,
            }),
            light_constants: gl.buffer(BufferKind::Uniform, LIGHT_SLOTS * LIGHT_SLOT),
            light_slots: LIGHT_SLOTS,
            sky: SkyPrograms::new(gl),
        }
    }

    /// s_base / s_detail's sampler, and the anisotropic samplers of the
    /// sky, at the maximum anisotropy in force.
    pub(crate) fn set_anisotropy(&mut self, gl: &Gl, anisotropy: u16) {
        self.samplers[0] = gl.sampler(SamplerDesc {
            anisotropy,
            ..LINEAR_WRAPPED
        });
        self.sky.set_anisotropy(gl, anisotropy);
    }

    /// Room for `passes` light passes' constants, grown as needed.
    fn light_room(&mut self, gl: &Gl, passes: u64) {
        if passes <= self.light_slots {
            return;
        }
        self.light_slots = passes.next_power_of_two();
        self.light_constants =
            gl.keeping(|| gl.buffer(BufferKind::Uniform, self.light_slots * LIGHT_SLOT));
    }

    /// The program and states of `key`.
    fn pipeline(&mut self, gl: &Gl, key: PipelineKey) -> (gl::Program, RasterState) {
        let entry = key.program.entry_points();
        let program = self
            .programs
            .entry(entry)
            .or_insert_with(|| link_with(gl, "world", entry.0, entry.1, world_unit, world_point))
            .clone();
        let unblended = self.unblended_trees && key.program == Program::Tree;
        let blend = match if unblended { Blend::Off } else { key.blend } {
            Blend::Off => None,
            Blend::Alpha => Some(ALPHA_BLENDING),
            Blend::Add => Some(blend(C::SRC_ALPHA, C::ONE)),
        };
        // Triangles that appear counter-clockwise on screen are culled
        // (section 10.1).
        let raster = RasterState {
            blend,
            depth: less_equal(key.depth_write),
            cull: Some(C::BACK),
            front_face: FrontFace::Cw,
            color_mask: [true; 4],
        };
        (program, raster)
    }
}

/// The light passes' states: one / one, the result added and clamped
/// (section 6.2); depth test on (less or equal), write off.
const LIGHT_RASTER: RasterState = RasterState {
    blend: Some(blend(C::ONE, C::ONE)),
    depth: less_equal(false),
    cull: Some(C::BACK),
    front_face: FrontFace::Cw,
    color_mask: [true; 4],
};

/// A surface's textures as bound: its maps (second, hemi, environment)
/// and its base and detail.
#[derive(Clone)]
struct SurfaceTextures {
    maps: [gl::Texture; 3],
    base: [gl::Texture; 2],
}

/// Draws the draws of `step` that `shows` may show, in order: consecutive
/// draws of one surface and buffers whose vertices are their own and whose
/// ranges follow one another as one call (the same triangles in the same
/// order, #387).
fn draw_static(
    gl: &Gl,
    programs: &WorldPrograms,
    inner: &Inner,
    step: &Step,
    shows: impl Fn(u32) -> bool,
    hidden: impl Fn(u32) -> bool,
) {
    use crate::stats::StaticCall;
    /// A call being gathered: draws whose vertices are their own with
    /// consecutive index ranges.
    struct Pending {
        range: std::ops::Range<u32>,
        surface: u32,
        draws: u32,
        wide: bool,
    }
    let flush = |pending: &mut Option<Pending>| {
        if let Some(Pending {
            range,
            surface,
            draws,
            wide,
        }) = pending.take()
        {
            crate::stats::draw();
            crate::stats::static_call(
                inner.programs[surface as usize].name(),
                StaticCall::Merged,
                draws,
            );
            emitted(Kind::Merged);
            gl.set_first_instance(&inner.surface_programs[surface as usize].0, 0);
            gl.draw_elements(C::TRIANGLES, range.start, range.len() as u32, wide, 1);
        }
    };
    inner.bind_frame(gl, programs);
    let mut surface = None;
    let mut array = None;
    let mut pending: Option<Pending> = None;
    for group in &step.groups {
        let members = &step.order[group.start as usize..group.end as usize];
        let i = members[0];
        // A group of draws with their own vertices is drawn whole when any
        // of them may show (the rest yield no pixels), so its index range
        // is the same every frame: WebKit checks each range a draw call
        // hasn't used before on the GPU (#437).
        if !members.iter().any(|&m| shows(m)) {
            continue;
        }
        // A member that may show but the camera pass left out
        // (`hidden`): the group's other members are drawn in runs without
        // it.
        if inner.exclusive[i as usize] && members.iter().any(|&m| hidden(m)) {
            let d = &inner.calls[i as usize];
            if surface != Some(d.surface) {
                flush(&mut pending);
                surface = Some(d.surface);
                inner.bind_surface(gl, programs, d.surface);
            }
            let a = inner.arrays_of[i as usize];
            if array != Some(a) {
                flush(&mut pending);
                array = Some(a);
                crate::stats::buffer();
                gl.bind_vertex_array(&inner.arrays[a as usize]);
            }
            flush(&mut pending);
            let wide = inner.index_wide[d.index_buffer as usize];
            for run in members.split(|&m| hidden(m)) {
                if run.iter().any(|&m| shows(m)) {
                    let (first, last) = (
                        &inner.calls[run[0] as usize],
                        &inner.calls[run[run.len() - 1] as usize],
                    );
                    pending = Some(Pending {
                        range: first.first_index..last.first_index + last.index_count,
                        surface: d.surface,
                        draws: run.len() as u32,
                        wide,
                    });
                    flush(&mut pending);
                }
            }
            continue;
        }
        let d = &inner.calls[i as usize];
        // Rebased at upload: WebGL 2 has no base vertex.
        debug_assert_eq!(d.first_vertex, 0);
        if surface != Some(d.surface) {
            flush(&mut pending);
            surface = Some(d.surface);
            inner.bind_surface(gl, programs, d.surface);
        }
        let a = inner.arrays_of[i as usize];
        if array != Some(a) {
            flush(&mut pending);
            array = Some(a);
            crate::stats::buffer();
            gl.bind_vertex_array(&inner.arrays[a as usize]);
        }
        let wide = inner.index_wide[d.index_buffer as usize];
        let range = d.first_index..d.first_index + d.index_count;
        let program = inner.programs[d.surface as usize];
        if inner.exclusive[i as usize] {
            flush(&mut pending);
            let last = &inner.calls[*members.last().expect("a member") as usize];
            pending = Some(Pending {
                range: range.start..last.first_index + last.index_count,
                surface: d.surface,
                draws: members.len() as u32,
                wide,
            });
            continue;
        }
        // A draw alone, as its own instance (its record).
        flush(&mut pending);
        crate::stats::draw();
        let kind = if program.is_model() {
            StaticCall::Object
        } else {
            StaticCall::Shared
        };
        crate::stats::static_call(program.name(), kind, 1);
        emitted(if program.is_model() {
            Kind::Object
        } else {
            Kind::Single
        });
        gl.set_first_instance(&inner.surface_programs[d.surface as usize].0, i);
        gl.draw_elements(C::TRIANGLES, range.start, range.len() as u32, wide, 1);
    }
    flush(&mut pending);
}

struct Inner {
    /// Each vertex array, one per (vertex buffer, index buffer) the kept
    /// draws use (their layouts agree with their programs'), and each kept
    /// draw's.
    arrays: Vec<VertexArray>,
    arrays_of: Vec<u32>,
    /// Each index buffer's indices: 16-bit as stored, 32-bit where indices
    /// rebased past 16 bits.
    index_wide: Vec<bool>,
    /// Per-draw data, one record per kept draw, RGBA32F texels
    /// ([`DRAW_TEXELS`] a record, [`DRAWS_PER_ROW`] records a row);
    /// placements rewrite it.
    draw_texels: gl::Texture,
    /// Each draw given to the world: its index among the kept draws.
    kept_of: Vec<Option<u32>>,
    _buffers: Vec<Buffer>,
    _textures: Vec<Texture>,
    frame: Buffer,
    /// Each surface's textures, its program and states.
    surface_textures: Vec<SurfaceTextures>,
    surface_programs: Vec<(gl::Program, RasterState)>,
    /// Program of each surface.
    programs: Vec<Program>,
    /// Section 10.3 step 1 (priority 0–1, not strict) and step 5 (2–3):
    /// the draws in their fixed order.
    early: Step,
    late: Step,
    /// Every draw's call.
    calls: Vec<DrawCall>,
    /// Whether each draw's vertices are its own ([`owned_spans`]), so its
    /// call can merge with the next.
    exclusive: Vec<bool>,
    /// Step 6: strict-sorted draws, ordered each frame.
    strict: RefCell<Vec<(usize, WorldDraw)>>,
    /// The draws light passes may draw again.
    lit: Vec<LitDraw>,
    /// The bounding sphere each kept draw's record holds now.
    spheres: RefCell<Vec<[f32; 4]>>,
    sky: Option<SkyResources>,
    sky_error: Option<String>,
    draws: usize,
    skipped: usize,
    buffer_bytes: u64,
    texture_bytes: u64,
    object_draws: usize,
    object_buffer_bytes: u64,
    object_texture_bytes: u64,
}

impl Inner {
    /// Binds the frame's constants and the records.
    fn bind_frame(&self, gl: &Gl, programs: &WorldPrograms) {
        crate::stats::bind_group();
        gl.bind_uniforms(points::WORLD, &self.frame, 0, FRAME_BYTES);
        gl.bind_texture(units::RECORDS, &self.draw_texels, &programs.nearest);
    }

    /// Uses surface `s`'s program and states and binds its textures.
    fn bind_surface(&self, gl: &Gl, programs: &WorldPrograms, s: u32) {
        let (program, raster) = &self.surface_programs[s as usize];
        crate::stats::pipeline();
        gl.use_program(program);
        gl.set_raster(raster);
        let t = &self.surface_textures[s as usize];
        let [_, lmap, hemi] = &programs.samplers;
        crate::stats::bind_group();
        gl.bind_texture(units::SECOND, &t.maps[0], lmap);
        gl.bind_texture(units::HEMI, &t.maps[1], hemi);
        // The environment through s_hemi's sampler (world.wgsl's group 1,
        // binding 4).
        gl.bind_texture(units::ENV, &t.maps[2], hemi);
        self.bind_base(gl, programs, s);
    }

    /// Binds surface `s`'s base and detail textures.
    fn bind_base(&self, gl: &Gl, programs: &WorldPrograms, s: u32) {
        let t = &self.surface_textures[s as usize];
        crate::stats::bind_group();
        gl.bind_texture(units::BASE, &t.base[0], &programs.samplers[0]);
        gl.bind_texture(units::DETAIL, &t.base[1], &programs.samplers[0]);
    }
}

/// A static world uploaded to the GPU; cheap to clone (a handle).
#[derive(Clone)]
pub struct World(Rc<Inner>);

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("draws", &self.0.draws)
            .field("buffer_bytes", &self.0.buffer_bytes)
            .field("texture_bytes", &self.0.texture_bytes)
            .finish()
    }
}

impl World {
    /// Bytes of GPU buffers held (vertices, indices, per-draw data).
    pub fn gpu_bytes(&self) -> u64 {
        self.0.buffer_bytes
    }

    /// Bytes of the textures the surfaces use.
    pub fn texture_bytes(&self) -> u64 {
        self.0.texture_bytes
    }

    /// Draws of the static world.
    pub fn draw_count(&self) -> usize {
        self.0.draws
    }

    /// Draws of the objects' models.
    pub fn object_draw_count(&self) -> usize {
        self.0.object_draws
    }

    /// Bytes of GPU buffers held for the objects' models and their draws
    /// (included in [`World::gpu_bytes`]).
    pub fn object_gpu_bytes(&self) -> u64 {
        self.0.object_buffer_bytes
    }

    /// Bytes of the textures only the objects' surfaces use (included in
    /// [`World::texture_bytes`]).
    pub fn object_texture_bytes(&self) -> u64 {
        self.0.object_texture_bytes
    }

    /// Draws left out because their vertex layout does not suit their
    /// surface's program, or their surface is missing.
    pub fn skipped(&self) -> usize {
        self.0.skipped
    }

    /// Why the sky could not be set up, if it could not.
    pub fn sky_error(&self) -> Option<&str> {
        self.0.sky_error.as_deref()
    }

    /// The bounding sphere draw `draw` (its index among the draws the world
    /// was created with) is drawn with now; radius 0: hidden. `None` for a
    /// draw left out.
    pub fn draw_sphere(&self, draw: usize) -> Option<[f32; 4]> {
        let k = (*self.0.kept_of.get(draw)?)?;
        Some(self.0.spheres.borrow()[k as usize])
    }

    pub fn same(&self, other: &World) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

enum Source {
    /// Stored vertex buffer `i` of the level's geometry.
    Vertices(u32),
    /// Stored index buffer `i`, rebased by its segments.
    Rebased(u32),
    /// Vertex buffer `i`'s draw ids ([`fill_draw_ids`]).
    DrawIds(u32),
    /// The objects' vertices.
    ObjectVertices,
    /// The objects' indices rebased by `segments`, 32-bit when `wide`.
    ObjectRebased { segments: Vec<Segment>, wide: bool },
}

/// A buffer of a [`GeometryUpload`] and where its bytes come from.
struct Job {
    buffer: Buffer,
    label: &'static str,
    source: Source,
    bytes: u64,
}

/// A level's static geometry and the objects' models on their way to the
/// GPU ([`Renderer::begin_geometry`]): filled a piece at a time by
/// [`GeometryUpload::step`], so a platform can spread it over task turns.
/// WebGL copies each piece at the call (WebGL 2 specification), so no more
/// than a piece is held; the platform's waits between steps keep the GPU
/// process's memory down on the phone (#443).
pub struct GeometryUpload {
    jobs: Vec<Job>,
    /// The job being filled, and how far.
    next: usize,
    at: u64,
    piece: Vec<u8>,
    segments: Vec<Vec<Segment>>,
    /// Per index buffer and segment, what is learned of it.
    uses: Vec<Vec<SegmentUse>>,
    /// Whether each index buffer is rebased to 32 bits.
    wide: Vec<bool>,
    /// Each level draw's segment, by its range: (index buffer, segment).
    level_segments: HashMap<RangeKey, (u32, u32)>,
    /// [`owned_spans`], once every index buffer is filled.
    owned: Option<Vec<Vec<(u32, u32, u32)>>>,
    id_buffers: Vec<Option<Buffer>>,
    rebased: Rebased,
    vertex_buffers: Vec<Buffer>,
    index_buffers: Vec<Buffer>,
    /// Whether each index buffer (the objects' too) holds 32-bit indices.
    index_wide: Vec<bool>,
    vertex_layouts: Vec<Option<Layout>>,
    buffer_bytes: u64,
    object_buffer_bytes: u64,
    object_vb: u32,
    object_ib: u32,
    /// The draws' index ranges it was begun for.
    ranges: Vec<(bool, u32, u32, u32, u32)>,
    /// Whether it was begun with objects' vertices.
    objects: bool,
}

impl GeometryUpload {
    /// Uploads about `budget` more bytes (whole pieces of at most
    /// [`UPLOAD_PIECE`]), reading the level's buffers from `geom` and the
    /// objects' from `objects` (those it was begun with). Returns whether
    /// everything is uploaded.
    pub fn step(
        &mut self,
        renderer: &Renderer,
        geom: &mut dyn GeomSource,
        objects: &ObjectGeometry,
        budget: u64,
    ) -> Result<bool, String> {
        let mut spent = 0;
        while let Some(job) = self.jobs.get(self.next) {
            if self.at >= job.bytes {
                self.next += 1;
                self.at = 0;
                continue;
            }
            if spent >= budget {
                return Ok(false);
            }
            let at = self.at;
            let n = (job.bytes - at).min(UPLOAD_PIECE as u64) as usize;
            self.piece.clear();
            self.piece.resize(n.next_multiple_of(4), 0);
            let out = &mut self.piece[..n];
            let range = at as usize..at as usize + n;
            match &job.source {
                Source::Vertices(i) => geom.read(GeomBuffer::Vertices(*i), at, out),
                Source::Rebased(i) => fill_rebased(
                    geom,
                    GeomBuffer::Indices(*i),
                    &self.segments[*i as usize],
                    &mut self.uses[*i as usize],
                    self.wide[*i as usize],
                    at,
                    out,
                ),
                Source::DrawIds(i) => {
                    let owned = self.owned.get_or_insert_with(|| {
                        owned_spans(&self.segments, &self.uses, self.vertex_layouts.len())
                    });
                    fill_draw_ids(&owned[*i as usize], at, out);
                    Ok(())
                }
                Source::ObjectVertices => objects
                    .vertices
                    .get(range)
                    .map(|v| out.copy_from_slice(v))
                    .ok_or_else(|| io::Error::other("the objects' vertices changed")),
                Source::ObjectRebased { segments, wide } => {
                    fill_object_rebased(&objects.indices, segments, *wide, at, out)
                }
            }
            .map_err(|e| format!("{}: {e}", job.label))?;
            // Padded to whole words.
            let end = (at as usize + self.piece.len()).min(job.buffer.size() as usize);
            renderer
                .gl
                .write_buffer(&job.buffer, at, &self.piece[..end - at as usize]);
            (renderer.geometry_bytes).set(renderer.geometry_bytes.get() + self.piece.len() as u64);
            // Each piece handed to the GPU at once.
            crate::stats::submit();
            renderer.gl.flush();
            self.at += n as u64;
            spent += n as u64;
        }
        self.piece = Vec::new();
        Ok(true)
    }

    /// The bytes it uploads in all.
    pub fn bytes(&self) -> u64 {
        self.jobs.iter().map(|j| j.bytes).sum()
    }
}

impl Renderer {
    /// Uploads `geom`'s buffers as stored and records `draws` with
    /// `surfaces`. A draw whose vertex layout does not suit its surface's
    /// program, or whose surface is missing, is left out and counted
    /// ([`World::skipped`]). `sky` sets up the sky passes; when that fails
    /// the world has no sky ([`World::sky_error`]). Fails when a draw names
    /// a buffer `geom` lacks.
    pub fn create_world(
        &mut self,
        geom: &LevelGeom,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        self.create_world_with(geom, &ObjectGeometry::default(), surfaces, draws, sky)
    }

    /// [`Renderer::create_world`] with the objects' models: `objects`'
    /// buffers are uploaded beside `geom`'s, and draws with
    /// [`WorldDraw::object`] set read them. Fails when such a draw's range
    /// lies outside them.
    pub fn create_world_with(
        &mut self,
        geom: &LevelGeom,
        objects: &ObjectGeometry,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        self.create_world_from(&mut geom.source(), objects, surfaces, draws, sky)
    }

    /// [`Renderer::create_world_with`] reading the static geometry from
    /// `geom` as it uploads it, in pieces of at most [`UPLOAD_PIECE`]
    /// bytes, so the CPU never holds more of it than one piece. Also fails
    /// when reading fails.
    pub fn create_world_from(
        &mut self,
        geom: &mut dyn GeomSource,
        objects: &ObjectGeometry,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        let mut upload = self.begin_geometry(geom.layout(), objects, draws)?;
        upload.step(self, geom, objects, u64::MAX)?;
        self.finish_world(upload, surfaces, draws, sky)
    }

    /// Begins uploading a level's static geometry laid out as `layout` and
    /// the objects' models for `draws` ([`Renderer::create_world_from`] in
    /// steps): the buffers are made, empty, and filled by
    /// [`GeometryUpload::step`]; [`Renderer::finish_world`] then makes the
    /// world. Fails when a draw names a buffer the geometry lacks.
    pub fn begin_geometry(
        &mut self,
        layout: &GeomLayout,
        objects: &ObjectGeometry,
        draws: &[WorldDraw],
    ) -> Result<GeometryUpload, String> {
        if !objects.vertices.len().is_multiple_of(MODEL_VERTEX_BYTES) {
            return Err("object vertices are not whole model vertices".into());
        }
        let object_vertices = (objects.vertices.len() / MODEL_VERTEX_BYTES) as u64;
        for d in draws {
            let fits = if d.object.is_some() {
                u64::from(d.first_index) + u64::from(d.index_count) <= objects.indices.len() as u64
                    && u64::from(d.first_vertex) < object_vertices.max(1)
            } else {
                (d.vertex_buffer as usize) < layout.vertex_buffers.len()
                    && (d.index_buffer as usize) < layout.index_counts.len()
            };
            if !fits {
                return Err(format!("draw {d:?} names a missing buffer"));
            }
        }
        let vertex_layouts: Vec<Option<Layout>> =
            layout.vertex_buffers.iter().map(|b| b.layout).collect();
        let gl = &self.gl;
        // Whole 4-byte words.
        let new_buffer = |_label: &str, kind: BufferKind, bytes: u64, min_size: u64| {
            gl.buffer(kind, bytes.next_multiple_of(4).max(min_size))
        };
        let mut jobs = Vec::new();
        let mut buffer_bytes = 0;
        let mut vertex_buffers = Vec::with_capacity(layout.vertex_buffers.len() + 1);
        for (i, b) in layout.vertex_buffers.iter().enumerate() {
            let bytes = b.bytes();
            buffer_bytes += bytes;
            let label = "level.geom vertices";
            let buffer = new_buffer(label, BufferKind::Vertex, bytes, 0);
            vertex_buffers.push(buffer.clone());
            jobs.push(Job {
                buffer,
                label,
                source: Source::Vertices(i as u32),
                bytes,
            });
        }
        // Each index range a level draw uses is uploaded with its draw's
        // first vertex added (a range drawn from two bases twice), ranges
        // one after another per index buffer, by surface, vertex buffer and
        // draw: the draws read from vertex 0 (WebGL has no base vertex), and
        // draws of a surface and buffers drawn one after another read
        // consecutive ranges, which one draw call can then cover (#387).
        let mut rebased = Rebased::default();
        let mut segments: Vec<Vec<Segment>> = vec![Vec::new(); layout.index_counts.len()];
        let mut uses: Vec<Vec<SegmentUse>> = vec![Vec::new(); layout.index_counts.len()];
        let mut level_segments = HashMap::new();
        let mut order: Vec<u32> = (0..draws.len() as u32)
            .filter(|&i| draws[i as usize].object.is_none())
            .collect();
        order.sort_by_key(|&i| {
            let d = &draws[i as usize];
            (d.index_buffer, d.surface, d.vertex_buffer, i)
        });
        for i in order {
            let d = &draws[i as usize];
            let ib = d.index_buffer as usize;
            let (segments, uses) = (&mut segments[ib], &mut uses[ib]);
            match level_segments.entry(Rebased::key(d)) {
                std::collections::hash_map::Entry::Occupied(e) => {
                    let (_, k): (u32, u32) = *e.get();
                    uses[k as usize].uses += 1;
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    let at = segments.last().map_or(0, |s| s.at + s.count);
                    e.insert((ib as u32, segments.len() as u32));
                    segments.push(Segment {
                        at,
                        first: d.first_index,
                        count: d.index_count,
                        base: d.first_vertex,
                        vb: d.vertex_buffer,
                    });
                    uses.push(SegmentUse {
                        draw: i,
                        uses: 1,
                        span: [u32::MAX, 0],
                    });
                }
            }
        }
        let mut index_buffers = Vec::with_capacity(layout.index_counts.len() + 1);
        let mut index_wide = Vec::with_capacity(layout.index_counts.len() + 1);
        let mut wide = Vec::with_capacity(layout.index_counts.len());
        for (i, list) in segments.iter().enumerate() {
            // 32-bit indices where a vertex buffer drawn from passes 16 bits.
            let w = list.iter().any(|s| {
                layout.vertex_buffers[s.vb as usize].vertex_count > u32::from(u16::MAX) + 1
            });
            wide.push(w);
            let count = list.last().map_or(0, |s| u64::from(s.at + s.count));
            let bytes = count * if w { 4 } else { 2 };
            // Index buffers are at least 4 bytes, as before any were empty.
            buffer_bytes += bytes.next_multiple_of(4).max(4);
            index_wide.push(w);
            let label = "level.geom indices";
            let buffer = new_buffer(label, BufferKind::Index, bytes, 4);
            index_buffers.push(buffer.clone());
            jobs.push(Job {
                buffer,
                label,
                source: Source::Rebased(i as u32),
                bytes,
            });
        }
        // Each level vertex's draw, after the indices that tell it.
        let mut id_buffers = Vec::with_capacity(layout.vertex_buffers.len() + 1);
        for (i, b) in layout.vertex_buffers.iter().enumerate() {
            if b.layout.is_none() {
                id_buffers.push(None);
                continue;
            }
            let bytes = 4 * u64::from(b.vertex_count);
            buffer_bytes += bytes.max(4);
            let label = "level.geom draw ids";
            let buffer = new_buffer(label, BufferKind::Vertex, bytes, 4);
            id_buffers.push(Some(buffer.clone()));
            jobs.push(Job {
                buffer,
                label,
                source: Source::DrawIds(i as u32),
                bytes,
            });
        }
        // The objects' buffers follow the level's.
        let (object_vb, object_ib) = (vertex_buffers.len() as u32, index_buffers.len() as u32);
        let mut object_buffer_bytes = 0;
        if !objects.vertices.is_empty() {
            // Whole 16-byte vertices: a multiple of 4 bytes, as buffers need.
            let bytes = objects.vertices.len() as u64;
            let label = "object vertices";
            let buffer = new_buffer(label, BufferKind::Vertex, bytes, 0);
            vertex_buffers.push(buffer.clone());
            jobs.push(Job {
                buffer,
                label,
                source: Source::ObjectVertices,
                bytes,
            });
            object_buffer_bytes += bytes;
            // Rebased as they are uploaded, as the level's are (a rebased
            // copy made at once took 2.6 MiB, its 32-bit working copy 4 MiB,
            // in the wasm heap; #357), 32-bit once they pass 16 bits.
            let label = "object indices";
            let mut object_segments: Vec<Segment> = Vec::new();
            let mut widest = 0u32;
            for d in draws.iter().filter(|d| d.object.is_some()) {
                rebased.add(d, || {
                    let at = object_segments.last().map_or(0, |s| s.at + s.count);
                    let range = d.first_index as usize..(d.first_index + d.index_count) as usize;
                    let top = objects.indices[range].iter().copied().max().unwrap_or(0);
                    widest = widest.max(u32::from(top) + d.first_vertex);
                    object_segments.push(Segment {
                        at,
                        first: d.first_index,
                        count: d.index_count,
                        base: d.first_vertex,
                        vb: object_vb,
                    });
                    at
                });
            }
            let wide = widest > u32::from(u16::MAX);
            let count = object_segments.last().map_or(0, |s| s.at + s.count);
            let bytes = (u64::from(count) * if wide { 4 } else { 2 }).max(4);
            object_buffer_bytes += bytes.next_multiple_of(4);
            index_wide.push(wide);
            let buffer = new_buffer(label, BufferKind::Index, bytes, 0);
            index_buffers.push(buffer.clone());
            jobs.push(Job {
                buffer,
                label,
                source: Source::ObjectRebased {
                    segments: object_segments,
                    wide,
                },
                bytes,
            });
        }
        Ok(GeometryUpload {
            jobs,
            next: 0,
            at: 0,
            piece: Vec::new(),
            segments,
            uses,
            wide,
            level_segments,
            owned: None,
            id_buffers,
            rebased,
            vertex_buffers,
            index_buffers,
            index_wide,
            vertex_layouts,
            buffer_bytes,
            object_buffer_bytes,
            object_vb,
            object_ib,
            ranges: draws.iter().map(Rebased::key).collect(),
            objects: !objects.vertices.is_empty(),
        })
    }

    /// The world of a finished [`GeometryUpload`] (begun with the same
    /// `draws`), with `surfaces` and `sky` as
    /// [`Renderer::create_world_from`] takes them.
    pub fn finish_world(
        &mut self,
        upload: GeometryUpload,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        if upload.next < upload.jobs.len() {
            return Err("the geometry is not all uploaded".into());
        }
        if upload.ranges.len() != draws.len()
            || upload
                .ranges
                .iter()
                .zip(draws)
                .any(|(r, d)| *r != Rebased::key(d))
        {
            return Err("the geometry was uploaded for other draws".into());
        }
        let GeometryUpload {
            rebased,
            segments,
            level_segments,
            owned,
            id_buffers,
            vertex_buffers,
            index_buffers,
            index_wide,
            vertex_layouts,
            mut buffer_bytes,
            mut object_buffer_bytes,
            object_vb,
            object_ib,
            objects,
            ..
        } = upload;
        let gl = self.gl.clone();
        if self.world_programs.is_none() {
            self.make_world_programs();
        }
        let programs = self.world_programs.as_mut().expect("made");
        buffer_bytes += object_buffer_bytes;

        // Draws that can be drawn: surface present, layouts agree. Object
        // draws are pointed at the objects' buffers.
        let usable = |d: &WorldDraw| {
            surfaces.get(d.surface as usize).is_some_and(|s| {
                if d.object.is_some() {
                    s.program.layout() == VertexLayout::Model && objects
                } else {
                    vertex_layouts[d.vertex_buffer as usize].map(VertexLayout::Level)
                        == Some(s.program.layout())
                }
            })
        };
        // Every draw keeps its place (its record's index, which the level's
        // vertices name); those that can't be drawn never are.
        let kept_of: Vec<Option<u32>> = (draws.iter().enumerate())
            .map(|(i, d)| usable(d).then_some(i as u32))
            .collect();
        // Level draws read their rebased ranges from vertex 0.
        let kept: Vec<WorldDraw> = draws
            .iter()
            .copied()
            .map(|mut d| {
                if d.object.is_some() {
                    d.first_index = rebased.first_index(&d);
                    d.first_vertex = 0;
                    d.vertex_buffer = object_vb;
                    d.index_buffer = object_ib;
                } else if let Some(&(ib, k)) = level_segments.get(&Rebased::key(&d)) {
                    d.first_index = segments[ib as usize][k as usize].at;
                    d.first_vertex = 0;
                }
                d
            })
            .collect();
        // The level draws whose vertices are their own, which draw calls
        // can merge (their records named by the vertices).
        let mut exclusive = vec![false; draws.len()];
        for o in owned.iter().flatten().flatten() {
            exclusive[o.2 as usize] = true;
        }
        let usable_count = kept_of.iter().flatten().count();
        let skipped = draws.len() - usable_count;
        let object_draws = (kept.iter().zip(&kept_of))
            .filter(|(d, k)| k.is_some() && d.object.is_some())
            .count();
        // The draws light passes may draw again (dynamic-lights.md 5.5
        // rule 4), grouped by program and surface.
        let mut lit: Vec<LitDraw> = (kept.iter().enumerate())
            .filter(|(k, _)| kept_of[*k].is_some())
            .filter_map(|(k, d)| {
                let s = &surfaces[d.surface as usize];
                let light = s.light.filter(|l| l.priority <= 1)?;
                (light.program.layout() == s.program.layout()).then_some(LitDraw {
                    kept: k as u32,
                    program: light.program,
                    surface: d.surface,
                    vertex_buffer: d.vertex_buffer,
                    index_buffer: d.index_buffer,
                    first_vertex: d.first_vertex,
                    first_index: d.first_index,
                    index_count: d.index_count,
                })
            })
            .collect();
        lit.sort_by_key(|l| (l.program as u8, l.surface, l.vertex_buffer, l.index_buffer));
        lit.shrink_to_fit();

        let mut records = Vec::with_capacity(kept.len().max(1) * DRAW_BYTES);
        for (d, k) in kept.iter().zip(&kept_of) {
            match (k, surfaces.get(d.surface as usize)) {
                (Some(_), Some(s)) => draw_record(s, d, &mut records),
                _ => records.resize(records.len() + DRAW_BYTES, 0),
            }
        }
        records.resize(records.len().max(DRAW_BYTES), 0);
        buffer_bytes += records.len() as u64;
        object_buffer_bytes += (object_draws * DRAW_BYTES) as u64;
        // Whole rows of the draws texture.
        let rows = (kept.len().max(1) as u32).div_ceil(DRAWS_PER_ROW);
        records.resize(rows as usize * DRAWS_PER_ROW as usize * DRAW_BYTES, 0);
        let width = DRAWS_PER_ROW * DRAW_TEXELS;
        let draws_texture = gl.texture(TexFormat::Rgba32Float, width, rows, 1, false);
        gl.write_texture(&draws_texture, 0, 0, 0, 0, width, rows, &mut records);
        drop(records);
        let frame = gl.buffer(BufferKind::Uniform, FRAME_BYTES);

        // Textures once each (by handle), for their memory; a texture
        // counts as the objects' when no static draw's surface uses it.
        let mut static_surface = vec![false; surfaces.len()];
        for (d, _) in (kept.iter().zip(&kept_of)).filter(|(d, k)| k.is_some() && d.object.is_none())
        {
            static_surface[d.surface as usize] = true;
        }
        let mut textures: Vec<(Texture, bool)> = Vec::new();
        for (s, &st) in surfaces.iter().zip(&static_surface) {
            for t in s.textures.iter().chain([&s.env]).flatten() {
                match textures
                    .iter_mut()
                    .find(|(k, _)| k.texture.same(&t.texture))
                {
                    Some((_, used)) => *used |= st,
                    None => textures.push((t.clone(), st)),
                }
            }
        }
        let (mut texture_bytes, mut object_texture_bytes) = (0, 0);
        for (t, st) in &textures {
            let bytes = t.texture.bytes();
            texture_bytes += bytes;
            if !st {
                object_texture_bytes += bytes;
            }
        }
        let mut textures: Vec<Texture> = textures.into_iter().map(|(t, _)| t).collect();
        // Each surface's textures as bound: white for a missing one, and
        // for a cube map where a 2D texture is read (it has no 2D view on
        // WebGL 2, #492); a map that is not a 6-face square texture binds
        // white (our choice).
        let flat = |t: &Option<Texture>| match t {
            Some(t) if !t.texture.is_cube() => t.texture.clone(),
            _ => programs.white.clone(),
        };
        let surface_textures: Vec<SurfaceTextures> = surfaces
            .iter()
            .map(|s| {
                let [base, second, hemi, detail] = &s.textures;
                let env = match &s.env {
                    Some(t) if t.texture.is_cube() => t.texture.clone(),
                    _ => programs.white_cube.clone(),
                };
                SurfaceTextures {
                    maps: [flat(second), flat(hemi), env],
                    base: [flat(base), flat(detail)],
                }
            })
            .collect();
        let surface_programs: Vec<(gl::Program, RasterState)> = surfaces
            .iter()
            .map(|s| {
                programs.pipeline(
                    &gl,
                    PipelineKey {
                        program: s.program,
                        blend: s.blend,
                        depth_write: s.depth_write,
                    },
                )
            })
            .collect();

        // One vertex array per (vertex buffer, index buffer) the kept draws
        // use, with the draw ids of a level buffer.
        let mut array_keys: HashMap<(u32, u32), u32> = HashMap::new();
        let mut arrays = Vec::new();
        let arrays_of: Vec<u32> = kept
            .iter()
            .zip(&kept_of)
            .map(|(d, k)| {
                if k.is_none() {
                    return 0;
                }
                *array_keys
                    .entry((d.vertex_buffer, d.index_buffer))
                    .or_insert_with(|| {
                        let layout = surfaces[d.surface as usize].program.layout();
                        let vb = &vertex_buffers[d.vertex_buffer as usize];
                        let mut buffers = vec![(vb, 0, vertex_layout(layout))];
                        if let Some(Some(ids)) = id_buffers.get(d.vertex_buffer as usize) {
                            buffers.push((ids, 0, &ID));
                        }
                        let ib = &index_buffers[d.index_buffer as usize];
                        arrays.push(gl.vertex_array(&buffers, Some(ib)));
                        arrays.len() as u32 - 1
                    })
            })
            .collect();

        // Section 10.3: step 1 is priorities 0–1, step 5 priorities 2–3,
        // each grouped by state (opaque before blended); strict-sorted
        // draws are step 6. Blended draws within a step keep this fixed
        // order rather than near to far by ssa (our choice: the exact order
        // is unestablished, and a fixed one keeps them in the bundles).
        let mut early = Vec::new();
        let mut late = Vec::new();
        let mut strict = Vec::new();
        for (i, d) in kept.iter().enumerate() {
            if kept_of[i].is_none() {
                continue;
            }
            let s = &surfaces[d.surface as usize];
            if s.strict {
                strict.push((i, *d));
            } else if s.priority <= 1 {
                early.push(i);
            } else {
                late.push(i);
            }
        }
        let order_key = |i: &usize| {
            let d = &kept[*i];
            let s = &surfaces[d.surface as usize];
            (
                s.blend != Blend::Off,
                s.program as u8,
                d.surface,
                d.vertex_buffer,
                d.index_buffer,
            )
        };
        early.sort_by_key(order_key);
        late.sort_by_key(order_key);
        crate::stats::submit();
        gl.flush();
        let order = |v: Vec<usize>| v.into_iter().map(|i| i as u32).collect::<Vec<u32>>();
        let (early, late) = (order(early), order(late));
        let calls: Vec<DrawCall> = kept
            .iter()
            .map(|d| DrawCall {
                surface: d.surface,
                vertex_buffer: d.vertex_buffer,
                index_buffer: d.index_buffer,
                first_index: d.first_index,
                index_count: d.index_count,
                first_vertex: d.first_vertex as i32,
            })
            .collect();
        let (sky, sky_error) = match sky.map(|t| SkyResources::new(&gl, t)) {
            Some(Ok(sky)) => {
                for t in sky.textures() {
                    if !textures.iter().any(|k| k.texture.same(&t.texture)) {
                        texture_bytes += t.texture.bytes();
                        textures.push(t.clone());
                    }
                }
                (Some(sky), None)
            }
            Some(Err(e)) => (None, Some(e)),
            None => (None, None),
        };
        let mut buffers: Vec<Buffer> = vertex_buffers;
        buffers.extend(index_buffers);
        buffers.extend(id_buffers.into_iter().flatten());
        Ok(World(Rc::new(Inner {
            arrays,
            arrays_of,
            index_wide,
            draw_texels: draws_texture,
            kept_of,
            _buffers: buffers,
            _textures: textures,
            frame,
            surface_textures,
            surface_programs,
            programs: surfaces.iter().map(|s| s.program).collect(),
            early: Step::new(early, &calls, &exclusive),
            late: Step::new(late, &calls, &exclusive),
            calls,
            exclusive,
            strict: RefCell::new(strict),
            lit,
            spheres: RefCell::new(kept.iter().map(|d| d.sphere).collect()),
            sky,
            sky_error,
            draws: usable_count - object_draws,
            skipped,
            buffer_bytes,
            texture_bytes,
            object_draws,
            object_buffer_bytes,
            object_texture_bytes,
        })))
    }

    /// Places draw `draw` (its index among the draws `world` was created
    /// with) at `transform` with bounding sphere `sphere`; a radius of 0
    /// hides it (its screen size is then never above ssa_min, section
    /// 10.2). A draw left out of the world is ignored.
    pub fn place_draw(&self, world: &World, draw: usize, transform: &Mat4, sphere: [f32; 4]) {
        let inner = &world.0;
        let Some(Some(k)) = inner.kept_of.get(draw).copied() else {
            return;
        };
        let mut bytes = Vec::with_capacity(80);
        push_mat(&mut bytes, transform);
        push_vec4(&mut bytes, sphere);
        self.gl.write_texture(
            &inner.draw_texels,
            0,
            0,
            k % DRAWS_PER_ROW * DRAW_TEXELS,
            k / DRAWS_PER_ROW,
            (bytes.len() / 16) as u32,
            1,
            &mut bytes,
        );
        inner.spheres.borrow_mut()[k as usize] = sphere;
        for (i, d) in inner.strict.borrow_mut().iter_mut() {
            if *i == k as usize {
                d.transform = *transform;
                d.sphere = sphere;
            }
        }
    }

    /// The depth buffer for a `width` × `height` target.
    pub(crate) fn depth_texture(&mut self, width: u32, height: u32) -> gl::Texture {
        let fits = self
            .depth
            .as_ref()
            .is_some_and(|d| d.size() == (width, height));
        if !fits {
            self.depth = None;
            self.depth = Some(self.gl.keeping(|| {
                self.gl
                    .texture(TexFormat::Depth32Float, width, height, 1, false)
            }));
        }
        self.depth.clone().expect("depth created")
    }

    /// A framebuffer drawing into `target` with the depth buffer of its
    /// size, kept while both are.
    pub(crate) fn world_framebuffer(&mut self, target: &Target) -> gl::Framebuffer {
        let (w, h) = target.size();
        let depth = self.depth_texture(w, h);
        let kept = self.world_framebuffer.as_ref().filter(|fb| {
            fb.color().is_some_and(|c| c.same(&target.texture.texture))
                && fb.depth().is_some_and(|d| d.same(&depth))
        });
        if let Some(fb) = kept {
            return fb.clone();
        }
        self.world_framebuffer = None;
        let fb = self
            .gl
            .framebuffer(Some(&target.texture.texture), Some(&depth));
        self.world_framebuffer = Some(fb.clone());
        fb
    }

    /// Draws the world pass of `frame` into `framebuffer` (its colour and
    /// depth, `size`): cleared to the frame's clear colour, then the static
    /// world in the order of section 10.3. `particles` are the frame's
    /// prepared effects; `keep_depth` keeps the depth for the distortion
    /// pass.
    pub(crate) fn encode_world(
        &mut self,
        framebuffer: &gl::Framebuffer,
        size: (u32, u32),
        frame: &WorldFrame,
        particles: Option<&crate::particles::PreparedFrame>,
        keep_depth: bool,
    ) {
        let inner = &frame.world.0;
        EMITTED.set([0; 5]);
        let view_projection = multiply(&frame.view, &frame.projection);
        let l = &frame.light;
        let t = frame.time;
        let gl = self.gl.clone();
        let constants = frame_constants(frame, &view_projection, size);
        gl.write_buffer(&inner.frame, 0, &constants);
        let filtering = frame.filtering;
        let sky = inner.sky.as_ref().zip(frame.sky.as_ref());
        if let Some((resources, sky_frame)) = sky {
            resources.write(
                &gl,
                sky_frame,
                frame.eye,
                &view_projection,
                t,
                filtering.mip_bias,
            );
        }

        // Step 6 order: decreasing squared distance (back to front), the
        // strict-sorted effects among them by their keys (particle-drawing.md
        // 7.3; on equal keys the static draws first, our choice: the queue
        // order of the two is not stated).
        let strict = inner.strict.borrow().clone();
        let distance = |d: &WorldDraw| {
            (0..3)
                .map(|i| (d.sphere[i] - frame.eye[i]).powi(2))
                .sum::<f32>()
        };
        enum Sorted<'a> {
            Static(usize, WorldDraw),
            Effect(&'a crate::particles::Prepared),
        }
        let mut sorted: Vec<(f32, Sorted)> = strict
            .into_iter()
            .map(|(i, d)| (distance(&d), Sorted::Static(i, d)))
            .collect();
        if let Some(prepared) = particles {
            let queue = crate::particles::ParticleQueue::Strict;
            sorted.extend(
                (prepared.draws.iter())
                    .filter(|d| d.queue == queue)
                    .map(|d| (d.key, Sorted::Effect(d))),
            );
        }
        sorted.sort_by(|a, b| b.0.total_cmp(&a.0));

        let lights = self.prepare_lights(frame, &view_projection);
        let details = frame.details.as_ref().filter(|d| {
            let _span = tracing::info_span!("render.details").entered();
            self.details.prepare(
                &gl,
                d,
                &view_projection,
                [l.ambient, l.hemi, l.sun_color],
                filtering.mip_bias,
            )
        });
        let billboards = frame
            .billboards
            .as_ref()
            .filter(|b| (self.billboards).prepare(&gl, b, &view_projection, l, filtering.mip_bias));
        let programs = self
            .world_programs
            .as_ref()
            .expect("made with the renderer");
        let [r, g, b] = frame.clear;
        Self::begin_pass(&gl, framebuffer, size, Some([r, g, b, 1.0]), Some(1.0));
        // Section 10.3: step 1, the sky (step 3, sky.md section 10), step
        // 5, then step 6; the effects within steps 1, 5 and 6
        // (particle-drawing.md 7.4).
        use crate::particles::ParticleQueue;
        use crate::stats::{Part, set_part};
        // The static draws that may show (the camera's frustum and the size
        // test, conservatively; the shaders still drop the rest).
        let planes = frustum_planes(&view_projection);
        let ssa = ssa_min(size.0, size.1, frame.fov_y_degrees, frame.geometry_lod);
        let spheres = inner.spheres.borrow();
        // The draws the camera pass keeps, by kept draw.
        let kept: Option<Vec<bool>> = frame.drawn.as_ref().map(|drawn| {
            let mut kept = vec![false; spheres.len()];
            for (w, k) in inner.kept_of.iter().enumerate() {
                if let (Some(k), Some(&d)) = (k, drawn.get(w)) {
                    kept[*k as usize] = d;
                }
            }
            kept
        });
        let in_pass = |i: u32| kept.as_ref().is_none_or(|k| k[i as usize]);
        let may = |i: u32| may_show(spheres[i as usize], &planes, frame.eye, ssa);
        let shows = |i: u32| in_pass(i) && may(i);
        let hidden = |i: u32| !in_pass(i) && may(i);
        set_part(Part::WorldEarly);
        draw_static(&gl, programs, inner, &inner.early, shows, hidden);
        if let Some(prepared) = particles {
            set_part(Part::Particles);
            prepared.draw_queue(&gl, &self.particles, ParticleQueue::Early);
        }
        // The detail objects after the priority 0–1 draws, before the sky
        // (detail-objects.md 6.1).
        if let Some(d) = details {
            set_part(Part::Details);
            self.details.draw(&gl, d);
        }
        // The billboards: element 4 before the sky, element 1 after it
        // (static-shading.md 10.3 steps 2 – 4).
        if let Some(b) = billboards {
            set_part(Part::Billboards);
            self.billboards.draw(&gl, b, true);
        }
        if let Some((resources, sky_frame)) = sky {
            set_part(Part::Sky);
            resources.draw(&gl, &programs.sky, sky_frame, size);
        }
        if let Some(b) = billboards {
            set_part(Part::Billboards);
            self.billboards.draw(&gl, b, false);
        }
        set_part(Part::Lights);
        // The light passes follow the sky (dynamic-lights.md 5.1).
        if let Some(textures) = self.light_textures.as_ref().filter(|_| !lights.is_empty()) {
            inner.bind_frame(&gl, programs);
            gl.set_raster(&LIGHT_RASTER);
            for (offset, spot, draws) in &lights {
                // Point: s_lmap and s_att the round falloff; spot: s_lmap
                // the projected pattern, s_att the depth falloff (6.2).
                let (lmap, att) = if *spot {
                    (&textures.att, &textures.attclip)
                } else {
                    (&textures.attpoint, &textures.attpoint)
                };
                crate::stats::bind_group();
                gl.bind_uniforms(
                    points::LIGHT,
                    &programs.light_constants,
                    u64::from(*offset),
                    LIGHT_SLOT,
                );
                gl.bind_texture(units::LIGHT_MAP, &lmap.texture, &programs.light_sampler);
                gl.bind_texture(units::LIGHT_ATT, &att.texture, &programs.light_sampler);
                let mut program = None;
                let mut surface = None;
                let mut array = None;
                for &j in draws {
                    let d = &inner.lit[j as usize];
                    debug_assert_eq!(d.first_vertex, 0);
                    let p = &programs.light_programs[&d.program];
                    if program != Some(d.program) {
                        program = Some(d.program);
                        crate::stats::pipeline();
                        gl.use_program(p);
                    }
                    if surface != Some(d.surface) {
                        surface = Some(d.surface);
                        inner.bind_base(&gl, programs, d.surface);
                    }
                    let a = inner.arrays_of[d.kept as usize];
                    if array != Some(a) {
                        array = Some(a);
                        crate::stats::buffer();
                        gl.bind_vertex_array(&inner.arrays[a as usize]);
                    }
                    crate::stats::draw();
                    emitted(Kind::Light);
                    gl.set_first_instance(p, d.kept);
                    let wide = inner.index_wide[d.index_buffer as usize];
                    gl.draw_elements(C::TRIANGLES, d.first_index, d.index_count, wide, 1);
                }
            }
        }
        set_part(Part::WorldLate);
        draw_static(&gl, programs, inner, &inner.late, shows, hidden);
        if let Some(prepared) = particles {
            set_part(Part::Particles);
            prepared.draw_queue(&gl, &self.particles, ParticleQueue::Late);
            // The portal fade (portal-fade.md 5).
            prepared.draw_queue(&gl, &self.particles, ParticleQueue::Fade);
        }
        for (_, item) in sorted {
            let (i, d) = match item {
                Sorted::Static(i, _) if !shows(i as u32) => continue,
                Sorted::Static(i, d) => (i, d),
                Sorted::Effect(e) => {
                    if let Some(prepared) = particles {
                        set_part(Part::Particles);
                        prepared.draw(&gl, &self.particles, e);
                    }
                    continue;
                }
            };
            set_part(Part::Strict);
            inner.bind_frame(&gl, programs);
            inner.bind_surface(&gl, programs, d.surface);
            crate::stats::buffer();
            gl.bind_vertex_array(&inner.arrays[inner.arrays_of[i] as usize]);
            crate::stats::draw();
            emitted(Kind::Strict);
            gl.set_first_instance(&inner.surface_programs[d.surface as usize].0, i as u32);
            let wide = inner.index_wide[d.index_buffer as usize];
            gl.draw_elements(C::TRIANGLES, d.first_index, d.index_count, wide, 1);
        }
        // Step 7: the glows (glows.md section 4).
        if let Some(prepared) = particles {
            set_part(Part::Particles);
            prepared.draw_queue(&gl, &self.particles, ParticleQueue::Glow);
        }
        if !keep_depth {
            gl.invalidate_depth();
        }
        set_part(Part::Screen);
    }

    /// The world's programs for the trees' blending, with the light
    /// textures given so far.
    pub(crate) fn make_world_programs(&mut self) {
        let programs = self
            .gl
            .keeping(|| WorldPrograms::new(&self.gl, self.unblended_trees));
        self.world_programs = Some(programs);
    }

    /// The world's draw calls emitted in the last world frame encoded, by
    /// kind ([`CALL_KINDS`]; #443).
    pub fn world_calls_emitted(&self) -> [u32; 5] {
        EMITTED.get()
    }

    /// Tests only: draws the trees without blending (worlds made after).
    /// The Mac's GPU doesn't repeat the blended trees' pixels exactly from
    /// one drawing of a frame to the next (#435), so frames that should be
    /// the same are compared exactly this way.
    pub fn set_unblended_trees(&mut self, on: bool) {
        self.unblended_trees = on;
        self.make_world_programs();
    }

    /// The draws the last frame's light passes drew, over all passes.
    pub fn light_draws(&self) -> usize {
        self.light_draws
    }

    /// Gives the light passes their attenuation textures
    /// (dynamic-lights.md 6.4); without them no light pass is drawn.
    pub fn set_light_textures(&mut self, textures: LightTextures) {
        self.light_textures = Some(textures);
    }

    /// Writes the constants of `frame`'s light passes and returns each
    /// pass with draws: its constants' offset, whether it is a spot pass,
    /// and the lit draws (indices into the world's) that its frustum and
    /// the camera's view-projection `camera` meet: a superset of those the
    /// camera's pass submitted (section 5.5, rule 1, as level.md 9.2
    /// allows); the projected-size rule 3 is the shaders' as in the
    /// camera's pass.
    // TODO(#347): the first-person models' part of each pass (section 5.5)
    // waits for hud-scope.md item 26.
    fn prepare_lights(&mut self, frame: &WorldFrame, camera: &Mat4) -> Vec<(u32, bool, Vec<u32>)> {
        let Some(p) = self.world_programs.as_mut() else {
            return Vec::new();
        };
        self.light_draws = 0;
        if frame.lights.is_empty() || self.light_textures.is_none() {
            return Vec::new();
        }
        p.light_room(&self.gl, frame.lights.len() as u64);
        let inner = &frame.world.0;
        let spheres = inner.spheres.borrow();
        // The lit draws the camera's frustum meets, once for every pass.
        let camera = frustum_planes(camera);
        let seen: Vec<u32> = (inner.lit.iter().enumerate())
            .filter(|(_, d)| {
                let s = spheres[d.kept as usize];
                s[3] > 0.0 && meets(&camera, s)
            })
            .map(|(j, _)| j as u32)
            .collect();
        let mut constants = vec![0u8; frame.lights.len() * LIGHT_SLOT as usize];
        let mut out = Vec::new();
        for (i, l) in frame.lights.iter().enumerate() {
            let planes = frustum_planes(&l.frustum);
            let draws: Vec<u32> = (seen.iter().copied())
                .filter(|&j| meets(&planes, spheres[inner.lit[j as usize].kept as usize]))
                .collect();
            if draws.is_empty() {
                continue;
            }
            let at = i * LIGHT_SLOT as usize;
            let bytes = light_constants(l);
            constants[at..at + bytes.len()].copy_from_slice(&bytes);
            out.push((at as u32, l.spot, draws));
        }
        if !out.is_empty() {
            self.gl.write_buffer(&p.light_constants, 0, &constants);
        }
        self.light_draws = out.iter().map(|(_, _, d)| d.len()).sum();
        out
    }
}
