// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The GL layer: the only code calling WebGL 2 (docs/architecture/rendering.md,
//! The GL layer). Safe wrappers over web-sys's
//! `WebGl2RenderingContext` with a state cache: a setter calls WebGL only
//! when the value differs from what it last set, and every call is counted
//! by kind ([`Gl::take_counts`]).
//!
//! It builds for any target (web-sys does), so the crate's platform-free code
//! keeps its native tests; nothing calls it natively.
//!
//! Conventions: samplers always filter between mip levels,
//! an 8-bit attribute read as a float is normalized, a texture's base and top
//! levels are set once, the program's positions are flipped in y (the
//! shaders' `ADJUST_COORDINATE_SPACE`), so front faces are given flipped
//! too ([`FrontFace`]).

// Until the GL renderer uses all of it (#475).
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use web_sys::WebGl2RenderingContext as C;
use web_sys::{
    WebGlBuffer, WebGlFramebuffer, WebGlProgram, WebGlSampler, WebGlShader, WebGlSync,
    WebGlTexture, WebGlUniformLocation, WebGlVertexArrayObject,
};

use crate::glsl::Stage;

/// What a call does, for [`Counts`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    Program,
    VertexArray,
    Texture,
    Sampler,
    Buffer,
    Uniform,
    State,
    Framebuffer,
    Clear,
    Draw,
    Upload,
    Create,
    Other,
}

/// Every kind, in [`Counts`]' order.
pub const CALLS: [Call; 13] = [
    Call::Program,
    Call::VertexArray,
    Call::Texture,
    Call::Sampler,
    Call::Buffer,
    Call::Uniform,
    Call::State,
    Call::Framebuffer,
    Call::Clear,
    Call::Draw,
    Call::Upload,
    Call::Create,
    Call::Other,
];

impl Call {
    pub fn name(self) -> &'static str {
        match self {
            Call::Program => "program",
            Call::VertexArray => "vertex_array",
            Call::Texture => "texture",
            Call::Sampler => "sampler",
            Call::Buffer => "buffer",
            Call::Uniform => "uniform",
            Call::State => "state",
            Call::Framebuffer => "framebuffer",
            Call::Clear => "clear",
            Call::Draw => "draw",
            Call::Upload => "upload",
            Call::Create => "create",
            Call::Other => "other",
        }
    }
}

/// WebGL calls made, by [`Call`] kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts(pub [u32; CALLS.len()]);

impl Counts {
    pub fn get(&self, call: Call) -> u32 {
        self.0[call as usize]
    }

    pub fn total(&self) -> u32 {
        self.0.iter().sum()
    }
}

/// The optional extensions the renderer uses.
#[derive(Clone, Copy, Debug, Default)]
pub struct Extensions {
    /// `WEBGL_compressed_texture_s3tc`: BC1 to BC3.
    pub bc: bool,
    /// ETC2 and EAC (`WEBGL_compressed_texture_etc`).
    pub etc2: bool,
    /// `EXT_texture_filter_anisotropic`.
    pub anisotropic: bool,
    /// `EXT_color_buffer_float`: float colour attachments.
    pub color_buffer_float: bool,
}

/// A unique number for each object, so the state cache compares numbers
/// rather than JavaScript objects.
fn next_id() -> u32 {
    thread_local! {
        static NEXT: Cell<u32> = const { Cell::new(1) };
    }
    NEXT.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    })
}

/// What the context's state is, as last set through [`Gl`]. 0 is no object.
#[derive(Default)]
struct State {
    program: u32,
    vertex_array: u32,
    /// The buffer on `COPY_WRITE_BUFFER`, which uploads use.
    copy_buffer: u32,
    /// The buffer on `ARRAY_BUFFER`, which attribute pointers read.
    array_buffer: u32,
    draw_framebuffer: u32,
    read_framebuffer: u32,
    active_unit: u32,
    /// Each unit's texture and sampler.
    textures: [u32; UNITS],
    samplers: [u32; UNITS],
    /// Each uniform binding point's (buffer, offset, size).
    uniform_ranges: [(u32, i32, i32); UNIFORM_POINTS],
    viewport: [i32; 4],
    /// The depth range's near and far, as bits (WebGL's default 0, 1).
    depth_range: [u32; 2],
    raster: Option<RasterState>,
    /// The first-instance uniform's value by program.
    first_instance: HashMap<u32, u32>,
}

impl State {
    /// A new context's state, as far as the cache tracks it.
    fn fresh() -> Self {
        Self {
            depth_range: [0f32.to_bits(), 1f32.to_bits()],
            ..Self::default()
        }
    }
}

/// Texture units the cache tracks (WebGL 2 guarantees 32 combined).
pub const UNITS: usize = 32;
/// The unit uploads bind textures to, apart from drawing's.
const UPLOAD_UNIT: u32 = UNITS as u32 - 1;
/// Uniform buffer binding points the cache tracks (WebGL 2 guarantees 24).
pub const UNIFORM_POINTS: usize = 24;

/// The WebGL 2 context with its state cache.
pub struct Gl {
    gl: C,
    /// What it made that still lives.
    live: Rc<Cell<Live>>,
    /// What of that was made while [`Gl::keeping`].
    kept: Rc<Cell<Live>>,
    /// Nesting depth of [`Gl::keeping`].
    keeping: Cell<u32>,
    state: RefCell<State>,
    counts: Cell<Counts>,
    extensions: Extensions,
    max_texture_size: u32,
    max_anisotropy: f32,
    /// Samplers by description, made once each.
    samplers: RefCell<HashMap<SamplerDesc, Sampler>>,
}

impl Gl {
    /// Takes the canvas's context (made by the platform) and enables the
    /// extensions the renderer uses.
    pub fn new(gl: C) -> Self {
        let has = |name: &str| gl.get_extension(name).ok().flatten().is_some();
        let extensions = Extensions {
            bc: has("WEBGL_compressed_texture_s3tc"),
            etc2: has("WEBGL_compressed_texture_etc"),
            anisotropic: has("EXT_texture_filter_anisotropic"),
            color_buffer_float: has("EXT_color_buffer_float"),
        };
        let max_texture_size = gl
            .get_parameter(C::MAX_TEXTURE_SIZE)
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(2048.0) as u32;
        let max_anisotropy = if extensions.anisotropic {
            // MAX_TEXTURE_MAX_ANISOTROPY_EXT.
            gl.get_parameter(0x84FF)
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0) as f32
        } else {
            1.0
        };
        // Rows are written tightly packed, whatever their width.
        gl.pixel_storei(C::UNPACK_ALIGNMENT, 1);
        gl.pixel_storei(C::PACK_ALIGNMENT, 1);
        Self {
            gl,
            live: Rc::default(),
            kept: Rc::default(),
            keeping: Cell::new(0),
            state: RefCell::new(State::fresh()),
            counts: Cell::default(),
            extensions,
            max_texture_size,
            max_anisotropy,
            samplers: RefCell::default(),
        }
    }

    pub fn extensions(&self) -> Extensions {
        self.extensions
    }

    /// The buffers, textures and vertex arrays made and not yet deleted.
    pub fn live(&self) -> Live {
        self.live.get()
    }

    /// Those of [`Gl::live`] made inside [`Gl::keeping`].
    pub fn kept(&self) -> Live {
        self.kept.get()
    }

    /// Runs `f`, counting what it makes as kept: the renderer's own
    /// resources, which outlive a game, apart from a level's.
    pub fn keeping<R>(&self, f: impl FnOnce() -> R) -> R {
        self.keeping.set(self.keeping.get() + 1);
        let r = f();
        self.keeping.set(self.keeping.get() - 1);
        r
    }

    /// Where a resource made now is counted.
    fn counted(&self) -> Counted {
        Counted {
            all: self.live.clone(),
            kept: (self.keeping.get() > 0).then(|| self.kept.clone()),
        }
    }

    /// Counts a resource being made, `f` adding it.
    fn add_live(&self, f: impl Fn(&mut Live)) -> Counted {
        let c = self.counted();
        c.update(f);
        c
    }

    pub fn max_texture_size(&self) -> u32 {
        self.max_texture_size
    }

    /// The context itself, for what the platform does with it (its
    /// extensions, context loss).
    pub fn context(&self) -> &C {
        &self.gl
    }

    fn count(&self, call: Call) {
        let mut c = self.counts.get();
        c.0[call as usize] += 1;
        self.counts.set(c);
    }

    /// The calls made since the last call.
    pub fn take_counts(&self) -> Counts {
        self.counts.take()
    }

    /// Forgets the cached state, after code outside the layer changed it.
    pub fn reset_state(&self) {
        *self.state.borrow_mut() = State::fresh();
    }

    // Objects.

    /// A buffer of `size` bytes, zero-filled.
    pub fn buffer(&self, kind: BufferKind, size: u64) -> Buffer {
        let b = self.create_buffer(kind, size);
        self.bind_copy(&b);
        self.count(Call::Upload);
        self.gl
            .buffer_data_with_i32(C::COPY_WRITE_BUFFER, size as i32, kind.usage());
        b
    }

    /// A buffer holding `data`.
    pub fn buffer_with(&self, kind: BufferKind, data: &[u8]) -> Buffer {
        let b = self.create_buffer(kind, data.len() as u64);
        self.bind_copy(&b);
        self.count(Call::Upload);
        self.gl
            .buffer_data_with_u8_array(C::COPY_WRITE_BUFFER, data, kind.usage());
        b
    }

    fn create_buffer(&self, kind: BufferKind, size: u64) -> Buffer {
        self.count(Call::Create);
        let raw = self.gl.create_buffer().expect("a WebGL buffer");
        // A buffer an index buffer is to be may be bound only to
        // ELEMENT_ARRAY_BUFFER and the copy targets (WebGL 2 specification,
        // 5.1), so it is first bound there, outside any vertex array.
        if kind == BufferKind::Index {
            self.bind_vertex_array_raw(0, None);
            self.count(Call::Buffer);
            self.gl.bind_buffer(C::ELEMENT_ARRAY_BUFFER, Some(&raw));
        }
        let live = self.add_live(|l| {
            l.buffers += 1;
            l.buffer_bytes += size;
        });
        Buffer(Rc::new(BufferInner {
            live,
            gl: self.gl.clone(),
            raw,
            id: next_id(),
            size,
            kind,
        }))
    }

    fn bind_copy(&self, b: &Buffer) {
        let mut s = self.state.borrow_mut();
        if s.copy_buffer != b.0.id {
            s.copy_buffer = b.0.id;
            self.count(Call::Buffer);
            self.gl.bind_buffer(C::COPY_WRITE_BUFFER, Some(&b.0.raw));
        }
    }

    /// Writes `data` into `buffer` at `offset` (copied at the call).
    pub fn write_buffer(&self, buffer: &Buffer, offset: u64, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.bind_copy(buffer);
        self.count(Call::Upload);
        self.gl
            .buffer_sub_data_with_i32_and_u8_array(C::COPY_WRITE_BUFFER, offset as i32, data);
    }

    /// A 2D texture (or a cube map, `cube`) of `levels` levels, contents
    /// undefined until written.
    pub fn texture(
        &self,
        format: TexFormat,
        width: u32,
        height: u32,
        levels: u32,
        cube: bool,
    ) -> Texture {
        self.count(Call::Create);
        let raw = self.gl.create_texture().expect("a WebGL texture");
        let target = if cube {
            C::TEXTURE_CUBE_MAP
        } else {
            C::TEXTURE_2D
        };
        let t = Texture(Rc::new(TextureInner {
            live: self.counted(),
            gl: self.gl.clone(),
            raw,
            id: next_id(),
            target,
            format,
            width,
            height,
            levels,
        }));
        t.0.live.update(|l| {
            l.textures += 1;
            l.texture_bytes += t.bytes();
        });
        self.bind_upload(&t);
        self.count(Call::Create);
        self.gl.tex_storage_2d(
            target,
            levels as i32,
            format.internal(),
            width as i32,
            height as i32,
        );
        // Set once: the levels every view reads.
        self.count(Call::Other);
        self.gl.tex_parameteri(target, C::TEXTURE_BASE_LEVEL, 0);
        self.count(Call::Other);
        self.gl
            .tex_parameteri(target, C::TEXTURE_MAX_LEVEL, levels as i32 - 1);
        t
    }

    fn bind_upload(&self, t: &Texture) {
        self.active(UPLOAD_UNIT);
        let mut s = self.state.borrow_mut();
        if s.textures[UPLOAD_UNIT as usize] != t.0.id {
            s.textures[UPLOAD_UNIT as usize] = t.0.id;
            self.count(Call::Texture);
            self.gl.bind_texture(t.0.target, Some(&t.0.raw));
        }
    }

    fn active(&self, unit: u32) {
        let mut s = self.state.borrow_mut();
        if s.active_unit != unit + 1 {
            s.active_unit = unit + 1;
            self.count(Call::Texture);
            self.gl.active_texture(C::TEXTURE0 + unit);
        }
    }

    /// Writes a region of one level (of one face, 0 – 5, of a cube map):
    /// rows top to bottom, tightly packed; whole blocks for compressed
    /// formats. `data` is mutable only because web-sys's compressed upload
    /// takes it so; it is not changed.
    #[allow(clippy::too_many_arguments)]
    pub fn write_texture(
        &self,
        texture: &Texture,
        level: u32,
        face: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &mut [u8],
    ) {
        self.bind_upload(texture);
        let target = if texture.0.target == C::TEXTURE_CUBE_MAP {
            C::TEXTURE_CUBE_MAP_POSITIVE_X + face
        } else {
            C::TEXTURE_2D
        };
        let f = texture.0.format;
        self.count(Call::Upload);
        let (x, y, w, h, level) = (
            x as i32,
            y as i32,
            width as i32,
            height as i32,
            level as i32,
        );
        let result = if f.compressed() {
            self.gl.compressed_tex_sub_image_2d_with_u8_array(
                target,
                level,
                x,
                y,
                w,
                h,
                f.internal(),
                data,
            );
            Ok(())
        } else if f == TexFormat::Rgba32Float {
            // FLOAT data must be a Float32Array (WebGL 2 specification).
            let bytes = js_sys::Uint8Array::from(&data[..]);
            let floats = js_sys::Float32Array::new_with_byte_offset_and_length(
                &bytes.buffer(),
                0,
                (data.len() / 4) as u32,
            );
            self.gl
                .tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_array_buffer_view(
                    target,
                    level,
                    x,
                    y,
                    w,
                    h,
                    f.format(),
                    f.data_type(),
                    Some(&floats),
                )
        } else {
            self.gl
                .tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
                    target,
                    level,
                    x,
                    y,
                    w,
                    h,
                    f.format(),
                    f.data_type(),
                    Some(data),
                )
        };
        if let Err(e) = result {
            tracing::error!(error = ?e, "texture upload failed");
        }
    }

    /// The sampler of `desc`, made at its first use.
    pub fn sampler(&self, desc: SamplerDesc) -> Sampler {
        if let Some(s) = self.samplers.borrow().get(&desc) {
            return s.clone();
        }
        self.count(Call::Create);
        let raw = self.gl.create_sampler().expect("a WebGL sampler");
        let filter = |f: Filter| match f {
            Filter::Nearest => C::NEAREST,
            Filter::Linear => C::LINEAR,
        };
        // Always filtered between levels.
        let min = match (desc.min, desc.mip) {
            (Filter::Nearest, Filter::Nearest) => C::NEAREST_MIPMAP_NEAREST,
            (Filter::Nearest, Filter::Linear) => C::NEAREST_MIPMAP_LINEAR,
            (Filter::Linear, Filter::Nearest) => C::LINEAR_MIPMAP_NEAREST,
            (Filter::Linear, Filter::Linear) => C::LINEAR_MIPMAP_LINEAR,
        };
        let wrap = |w: Wrap| match w {
            Wrap::Repeat => C::REPEAT,
            Wrap::Clamp => C::CLAMP_TO_EDGE,
        } as i32;
        let set = |p: u32, v: i32| {
            self.count(Call::Create);
            self.gl.sampler_parameteri(&raw, p, v);
        };
        set(C::TEXTURE_MIN_FILTER, min as i32);
        set(C::TEXTURE_MAG_FILTER, filter(desc.mag) as i32);
        set(C::TEXTURE_WRAP_S, wrap(desc.wrap[0]));
        set(C::TEXTURE_WRAP_T, wrap(desc.wrap[1]));
        set(C::TEXTURE_WRAP_R, wrap(desc.wrap[2]));
        self.count(Call::Create);
        self.gl.sampler_parameterf(&raw, C::TEXTURE_MIN_LOD, 0.0);
        self.count(Call::Create);
        let max_lod = if desc.top_level { 0.0 } else { 32.0 };
        self.gl
            .sampler_parameterf(&raw, C::TEXTURE_MAX_LOD, max_lod);
        if desc.anisotropy > 1 && self.extensions.anisotropic {
            self.count(Call::Create);
            // TEXTURE_MAX_ANISOTROPY_EXT.
            self.gl.sampler_parameterf(
                &raw,
                0x84FE,
                f32::from(desc.anisotropy).min(self.max_anisotropy),
            );
        }
        let s = Sampler(Rc::new(SamplerInner {
            gl: self.gl.clone(),
            raw,
            id: next_id(),
        }));
        self.samplers.borrow_mut().insert(desc, s.clone());
        s
    }

    /// Links the vertex stage `vs` and the fragment stage `fs`: each
    /// texture (as its WGSL (group, binding)) is read on the unit `unit`
    /// gives it, each uniform buffer from the binding point `point` gives
    /// it.
    pub fn program(
        &self,
        vs: &Stage,
        fs: &Stage,
        unit: impl Fn((u32, u32)) -> u32,
        point: impl Fn((u32, u32)) -> u32,
    ) -> Result<Program, String> {
        let compile = |stage: &Stage, kind: u32| -> Result<WebGlShader, String> {
            self.count(Call::Create);
            let shader = self.gl.create_shader(kind).ok_or("no shader")?;
            self.count(Call::Create);
            self.gl.shader_source(&shader, stage.source);
            self.count(Call::Create);
            self.gl.compile_shader(&shader);
            Ok(shader)
        };
        let v = compile(vs, C::VERTEX_SHADER)?;
        let f = compile(fs, C::FRAGMENT_SHADER)?;
        self.count(Call::Create);
        let raw = self.gl.create_program().ok_or("no program")?;
        for s in [&v, &f] {
            self.count(Call::Create);
            self.gl.attach_shader(&raw, s);
        }
        self.count(Call::Create);
        self.gl.link_program(&raw);
        self.count(Call::Create);
        let linked = self
            .gl
            .get_program_parameter(&raw, C::LINK_STATUS)
            .as_bool()
            .unwrap_or(false);
        if !linked {
            let log = |s: &WebGlShader| self.gl.get_shader_info_log(s).unwrap_or_default();
            let message = format!(
                "{} {} + {}: {} {} {}",
                vs.module,
                vs.entry,
                fs.entry,
                self.gl.get_program_info_log(&raw).unwrap_or_default(),
                log(&v),
                log(&f)
            );
            return Err(message);
        }
        for s in [&v, &f] {
            self.count(Call::Create);
            self.gl.delete_shader(Some(s));
        }
        let id = next_id();
        self.use_program_raw(id, &raw);
        let mut units = Vec::new();
        for s in vs.textures.iter().chain(fs.textures) {
            let u = unit(s.texture);
            self.count(Call::Create);
            let location = self.gl.get_uniform_location(&raw, s.name);
            self.count(Call::Create);
            self.gl.uniform1i(location.as_ref(), u as i32);
            if !units.iter().any(|x: &Unit| x.unit == u) {
                units.push(Unit {
                    unit: u,
                    texture: s.texture,
                    sampler: s.sampler,
                });
            }
        }
        for b in vs.blocks.iter().chain(fs.blocks) {
            self.count(Call::Create);
            let index = self.gl.get_uniform_block_index(&raw, b.name);
            if index != C::INVALID_INDEX {
                self.count(Call::Create);
                self.gl.uniform_block_binding(&raw, index, point(b.binding));
            }
        }
        self.count(Call::Create);
        let first_instance = self.gl.get_uniform_location(&raw, "naga_vs_first_instance");
        Ok(Program(Rc::new(ProgramInner {
            gl: self.gl.clone(),
            raw,
            id,
            units,
            first_instance,
        })))
    }

    /// A vertex array reading `buffers` (each at its byte offset, with its
    /// layout) and, if given, the 16- or 32-bit indices of `index`.
    pub fn vertex_array(
        &self,
        buffers: &[(&Buffer, u64, &VertexLayout)],
        index: Option<&Buffer>,
    ) -> VertexArray {
        self.count(Call::Create);
        let raw = self.gl.create_vertex_array().expect("a WebGL vertex array");
        let live = self.add_live(|l| l.vertex_arrays += 1);
        let va = VertexArray(Rc::new(VertexArrayInner {
            live,
            gl: self.gl.clone(),
            raw,
            id: next_id(),
        }));
        self.bind_vertex_array(&va);
        for &(buffer, offset, layout) in buffers {
            self.attributes(buffer, offset, layout, true);
        }
        if let Some(index) = index {
            self.count(Call::Buffer);
            self.gl
                .bind_buffer(C::ELEMENT_ARRAY_BUFFER, Some(&index.0.raw));
        }
        va
    }

    /// Points `layout`'s attributes of the bound vertex array at `buffer`
    /// from `offset`; `enable` also enables them and sets their divisors.
    fn attributes(&self, buffer: &Buffer, offset: u64, layout: &VertexLayout, enable: bool) {
        {
            let mut s = self.state.borrow_mut();
            if s.array_buffer != buffer.0.id {
                s.array_buffer = buffer.0.id;
                self.count(Call::Buffer);
                self.gl.bind_buffer(C::ARRAY_BUFFER, Some(&buffer.0.raw));
            }
        }
        for a in layout.attributes {
            let (count, ty, integer) = a.format.describe();
            let at = (offset + u64::from(a.offset)) as i32;
            self.count(Call::VertexArray);
            if integer {
                self.gl.vertex_attrib_i_pointer_with_i32(
                    a.location,
                    count,
                    ty,
                    layout.stride as i32,
                    at,
                );
            } else {
                // Always normalized (a float format ignores it).
                self.gl.vertex_attrib_pointer_with_i32(
                    a.location,
                    count,
                    ty,
                    true,
                    layout.stride as i32,
                    at,
                );
            }
            if enable {
                self.count(Call::VertexArray);
                self.gl.enable_vertex_attrib_array(a.location);
                self.count(Call::VertexArray);
                self.gl
                    .vertex_attrib_divisor(a.location, u32::from(layout.instanced));
            }
        }
    }

    /// Points the attributes of `layout` in `va` at `buffer` from `offset`
    /// (an instance range without a first instance).
    pub fn repoint(&self, va: &VertexArray, buffer: &Buffer, offset: u64, layout: &VertexLayout) {
        self.bind_vertex_array(va);
        self.attributes(buffer, offset, layout, false);
    }

    /// A framebuffer drawing into level 0 of `color` (if any) with `depth`
    /// (if any).
    pub fn framebuffer(&self, color: Option<&Texture>, depth: Option<&Texture>) -> Framebuffer {
        self.count(Call::Create);
        let raw = self.gl.create_framebuffer().expect("a WebGL framebuffer");
        let fb = Framebuffer(Rc::new(FramebufferInner {
            gl: self.gl.clone(),
            raw,
            id: next_id(),
            color: color.cloned(),
            depth: depth.cloned(),
        }));
        self.bind_draw_framebuffer(Some(&fb));
        for (attachment, t) in [(C::COLOR_ATTACHMENT0, color), (C::DEPTH_ATTACHMENT, depth)] {
            if let Some(t) = t {
                self.count(Call::Create);
                self.gl.framebuffer_texture_2d(
                    C::DRAW_FRAMEBUFFER,
                    attachment,
                    C::TEXTURE_2D,
                    Some(&t.0.raw),
                    0,
                );
            }
        }
        if color.is_none() {
            let none = js_sys::Array::of1(&C::NONE.into());
            self.count(Call::Create);
            self.gl.draw_buffers(&none);
        }
        fb
    }

    // Drawing state.

    fn use_program_raw(&self, id: u32, raw: &WebGlProgram) {
        let mut s = self.state.borrow_mut();
        if s.program != id {
            s.program = id;
            self.count(Call::Program);
            self.gl.use_program(Some(raw));
        }
    }

    pub fn use_program(&self, p: &Program) {
        self.use_program_raw(p.0.id, &p.0.raw);
    }

    fn bind_vertex_array_raw(&self, id: u32, raw: Option<&WebGlVertexArrayObject>) {
        let mut s = self.state.borrow_mut();
        if s.vertex_array != id {
            s.vertex_array = id;
            self.count(Call::VertexArray);
            self.gl.bind_vertex_array(raw);
        }
    }

    pub fn bind_vertex_array(&self, va: &VertexArray) {
        self.bind_vertex_array_raw(va.0.id, Some(&va.0.raw));
    }

    /// Binds `texture` with `sampler` to `unit`.
    pub fn bind_texture(&self, unit: u32, texture: &Texture, sampler: &Sampler) {
        let u = unit as usize;
        let (t, s) = {
            let s = self.state.borrow();
            (s.textures[u] != texture.0.id, s.samplers[u] != sampler.0.id)
        };
        if t {
            self.active(unit);
            self.state.borrow_mut().textures[u] = texture.0.id;
            self.count(Call::Texture);
            self.gl.bind_texture(texture.0.target, Some(&texture.0.raw));
        }
        if s {
            self.state.borrow_mut().samplers[u] = sampler.0.id;
            self.count(Call::Sampler);
            self.gl.bind_sampler(unit, Some(&sampler.0.raw));
        }
    }

    /// Binds `size` bytes of `buffer` from `offset` to uniform binding
    /// point `point`.
    pub fn bind_uniforms(&self, point: u32, buffer: &Buffer, offset: u64, size: u64) {
        let range = (buffer.0.id, offset as i32, size as i32);
        let mut s = self.state.borrow_mut();
        if s.uniform_ranges[point as usize] != range {
            s.uniform_ranges[point as usize] = range;
            self.count(Call::Buffer);
            self.gl.bind_buffer_range_with_i32_and_i32(
                C::UNIFORM_BUFFER,
                point,
                Some(&buffer.0.raw),
                offset as i32,
                size as i32,
            );
        }
    }

    /// Sets the bound program's first instance (naga's
    /// `naga_vs_first_instance`: WebGL 2 draws have none).
    pub fn set_first_instance(&self, program: &Program, first: u32) {
        let Some(location) = &program.0.first_instance else {
            return;
        };
        self.use_program(program);
        let mut s = self.state.borrow_mut();
        let current = s.first_instance.entry(program.0.id).or_insert(0);
        if *current != first {
            *current = first;
            self.count(Call::Uniform);
            self.gl.uniform1ui(Some(location), first);
        }
    }

    /// Draws into `framebuffer`, the canvas's default framebuffer with
    /// `None`.
    pub fn bind_draw_framebuffer(&self, framebuffer: Option<&Framebuffer>) {
        let id = framebuffer.map_or(0, |f| f.0.id);
        let mut s = self.state.borrow_mut();
        if s.draw_framebuffer != id {
            s.draw_framebuffer = id;
            self.count(Call::Framebuffer);
            self.gl
                .bind_framebuffer(C::DRAW_FRAMEBUFFER, framebuffer.map(|f| &f.0.raw));
        }
    }

    /// Draws into a framebuffer made outside the layer (an XR layer's).
    pub fn bind_draw_external(&self, framebuffer: &WebGlFramebuffer) {
        self.state.borrow_mut().draw_framebuffer = u32::MAX;
        self.count(Call::Framebuffer);
        self.gl
            .bind_framebuffer(C::DRAW_FRAMEBUFFER, Some(framebuffer));
    }

    fn bind_read_framebuffer(&self, framebuffer: &Framebuffer) {
        let mut s = self.state.borrow_mut();
        if s.read_framebuffer != framebuffer.0.id {
            s.read_framebuffer = framebuffer.0.id;
            self.count(Call::Framebuffer);
            self.gl
                .bind_framebuffer(C::READ_FRAMEBUFFER, Some(&framebuffer.0.raw));
        }
    }

    pub fn viewport(&self, x: i32, y: i32, width: i32, height: i32) {
        let v = [x, y, width, height];
        let mut s = self.state.borrow_mut();
        if s.viewport != v {
            s.viewport = v;
            self.count(Call::State);
            self.gl.viewport(x, y, width, height);
        }
    }

    /// The window depth range the viewport maps depth 0 … 1 to.
    pub fn depth_range(&self, near: f32, far: f32) {
        let r = [near.to_bits(), far.to_bits()];
        let mut s = self.state.borrow_mut();
        if s.depth_range != r {
            s.depth_range = r;
            self.count(Call::State);
            self.gl.depth_range(near, far);
        }
    }

    /// Discards the bound draw framebuffer's depth, which nothing reads
    /// after.
    pub fn invalidate_depth(&self) {
        let depth = js_sys::Array::of1(&C::DEPTH_ATTACHMENT.into());
        self.count(Call::Other);
        if let Err(e) = self.gl.invalidate_framebuffer(C::DRAW_FRAMEBUFFER, &depth) {
            tracing::error!(error = ?e, "invalidateFramebuffer failed");
        }
    }

    /// Sets what differs from `raster`.
    pub fn set_raster(&self, raster: &RasterState) {
        let old = self.state.borrow().raster;
        if old.as_ref() == Some(raster) {
            return;
        }
        let toggle = |cap: u32, on: bool, was: Option<bool>| {
            if was != Some(on) {
                self.count(Call::State);
                if on {
                    self.gl.enable(cap);
                } else {
                    self.gl.disable(cap);
                }
            }
        };
        toggle(
            C::BLEND,
            raster.blend.is_some(),
            old.map(|o| o.blend.is_some()),
        );
        if let Some(b) = raster.blend
            && old.and_then(|o| o.blend) != Some(b)
        {
            self.count(Call::State);
            self.gl
                .blend_func_separate(b.color.0, b.color.1, b.alpha.0, b.alpha.1);
            self.count(Call::State);
            self.gl.blend_equation_separate(b.color.2, b.alpha.2);
        }
        toggle(
            C::DEPTH_TEST,
            raster.depth.is_some(),
            old.map(|o| o.depth.is_some()),
        );
        if let Some(d) = raster.depth
            && old.and_then(|o| o.depth) != Some(d)
        {
            self.count(Call::State);
            self.gl.depth_func(d.compare);
        }
        let write = raster.depth.is_some_and(|d| d.write);
        if old.map(|o| o.depth.is_some_and(|d| d.write)) != Some(write) {
            self.count(Call::State);
            self.gl.depth_mask(write);
        }
        toggle(
            C::CULL_FACE,
            raster.cull.is_some(),
            old.map(|o| o.cull.is_some()),
        );
        if let Some(c) = raster.cull
            && old.and_then(|o| o.cull) != Some(c)
        {
            self.count(Call::State);
            self.gl.cull_face(c);
        }
        if old.map(|o| o.front_face) != Some(raster.front_face) {
            self.count(Call::State);
            self.gl.front_face(raster.front_face.gl());
        }
        if old.map(|o| o.color_mask) != Some(raster.color_mask) {
            let [r, g, b, a] = raster.color_mask;
            self.count(Call::State);
            self.gl.color_mask(r, g, b, a);
        }
        self.state.borrow_mut().raster = Some(*raster);
    }

    /// Clears the bound framebuffer's colour to `color` and depth to
    /// `depth`, each if given, whole (masks opened for it).
    pub fn clear(&self, color: Option<[f32; 4]>, depth: Option<f32>) {
        let mut raster = self.state.borrow().raster.unwrap_or(RasterState::OPAQUE);
        if color.is_some() {
            raster.color_mask = [true; 4];
        }
        if depth.is_some() {
            raster.depth = Some(raster.depth.map_or(
                Depth {
                    compare: C::ALWAYS,
                    write: true,
                },
                |d| Depth { write: true, ..d },
            ));
        }
        self.set_raster(&raster);
        if let Some(c) = color {
            self.count(Call::Clear);
            self.gl.clear_bufferfv_with_f32_array(C::COLOR, 0, &c);
        }
        if let Some(d) = depth {
            self.count(Call::Clear);
            self.gl.clear_bufferfv_with_f32_array(C::DEPTH, 0, &[d]);
        }
    }

    /// Draws `count` vertices from `first` of the bound vertex array.
    pub fn draw_arrays(&self, mode: u32, first: u32, count: u32, instances: u32) {
        self.count(Call::Draw);
        if instances == 1 {
            self.gl.draw_arrays(mode, first as i32, count as i32);
        } else {
            self.gl
                .draw_arrays_instanced(mode, first as i32, count as i32, instances as i32);
        }
    }

    /// Draws `count` indices of the bound vertex array's index buffer from
    /// index `first` (`wide`: 32-bit indices, else 16-bit).
    pub fn draw_elements(&self, mode: u32, first: u32, count: u32, wide: bool, instances: u32) {
        let (ty, size) = if wide {
            (C::UNSIGNED_INT, 4)
        } else {
            (C::UNSIGNED_SHORT, 2)
        };
        self.count(Call::Draw);
        let offset = (first * size) as i32;
        if instances == 1 {
            self.gl
                .draw_elements_with_i32(mode, count as i32, ty, offset);
        } else {
            self.gl.draw_elements_instanced_with_i32(
                mode,
                count as i32,
                ty,
                offset,
                instances as i32,
            );
        }
    }

    /// Copies `source`'s colour (`width` × `height`) into the bound draw
    /// framebuffer at `to` (x, y, width, height), flipped in y when `flip`.
    pub fn blit(&self, source: &Framebuffer, width: u32, height: u32, to: [i32; 4], flip: bool) {
        self.bind_read_framebuffer(source);
        let [x, y, w, h] = to;
        let (y0, y1) = if flip { (y + h, y) } else { (y, y + h) };
        self.count(Call::Draw);
        self.gl.blit_framebuffer(
            0,
            0,
            width as i32,
            height as i32,
            x,
            y0,
            x + w,
            y1,
            C::COLOR_BUFFER_BIT,
            C::NEAREST,
        );
    }

    /// Reads `width` × `height` RGBA pixels of `source`'s colour from its
    /// first row (the picture's top, in the layer's coordinate space).
    pub fn read_pixels(&self, source: &Framebuffer, width: u32, height: u32) -> Vec<u8> {
        self.bind_read_framebuffer(source);
        let mut out = vec![0; width as usize * height as usize * 4];
        self.count(Call::Other);
        if let Err(e) = self.gl.read_pixels_with_opt_u8_array(
            0,
            0,
            width as i32,
            height as i32,
            C::RGBA,
            C::UNSIGNED_BYTE,
            Some(&mut out),
        ) {
            tracing::error!(error = ?e, "readPixels failed");
        }
        out
    }

    /// Hands what was issued to the GPU.
    pub fn flush(&self) {
        self.count(Call::Other);
        self.gl.flush();
    }

    /// A fence after everything issued so far.
    pub fn fence(&self) -> Fence {
        self.count(Call::Other);
        let raw = self.gl.fence_sync(C::SYNC_GPU_COMMANDS_COMPLETE, 0);
        self.flush();
        Fence {
            gl: self.gl.clone(),
            raw,
        }
    }
}

/// What a buffer is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferKind {
    Vertex,
    Index,
    Uniform,
    /// Written every frame (`DYNAMIC_DRAW`).
    Dynamic,
}

impl BufferKind {
    fn usage(self) -> u32 {
        match self {
            BufferKind::Dynamic | BufferKind::Uniform => C::DYNAMIC_DRAW,
            _ => C::STATIC_DRAW,
        }
    }
}

struct BufferInner {
    live: Counted,
    gl: C,
    raw: WebGlBuffer,
    id: u32,
    size: u64,
    kind: BufferKind,
}

impl Drop for BufferInner {
    fn drop(&mut self) {
        self.gl.delete_buffer(Some(&self.raw));
        let size = self.size;
        self.live.update(|l| {
            l.buffers -= 1;
            l.buffer_bytes -= size;
        });
    }
}

/// A buffer (a handle).
#[derive(Clone)]
pub struct Buffer(Rc<BufferInner>);

impl Buffer {
    pub fn size(&self) -> u64 {
        self.0.size
    }
}

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Buffer({}, {} bytes)", self.0.id, self.0.size)
    }
}

/// Texture formats the renderer uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexFormat {
    Rgba8,
    R8,
    Rg8,
    Rgba32Float,
    Depth32Float,
    Bc1,
    Bc2,
    Bc3,
    Etc2Rgb8,
    Etc2Rgb8A1,
    Etc2Rgba8,
}

impl TexFormat {
    fn internal(self) -> u32 {
        match self {
            TexFormat::Rgba8 => C::RGBA8,
            TexFormat::R8 => C::R8,
            TexFormat::Rg8 => C::RG8,
            TexFormat::Rgba32Float => C::RGBA32F,
            TexFormat::Depth32Float => C::DEPTH_COMPONENT32F,
            // WEBGL_compressed_texture_s3tc.
            TexFormat::Bc1 => 0x83F1,
            TexFormat::Bc2 => 0x83F2,
            TexFormat::Bc3 => 0x83F3,
            // WEBGL_compressed_texture_etc.
            TexFormat::Etc2Rgb8 => 0x9274,
            TexFormat::Etc2Rgb8A1 => 0x9276,
            TexFormat::Etc2Rgba8 => 0x9278,
        }
    }

    fn format(self) -> u32 {
        match self {
            TexFormat::R8 => C::RED,
            TexFormat::Rg8 => C::RG,
            TexFormat::Depth32Float => C::DEPTH_COMPONENT,
            _ => C::RGBA,
        }
    }

    fn data_type(self) -> u32 {
        match self {
            TexFormat::Rgba32Float | TexFormat::Depth32Float => C::FLOAT,
            _ => C::UNSIGNED_BYTE,
        }
    }

    /// Stored in 4 × 4 blocks.
    pub fn compressed(self) -> bool {
        !matches!(
            self,
            TexFormat::Rgba8
                | TexFormat::R8
                | TexFormat::Rg8
                | TexFormat::Rgba32Float
                | TexFormat::Depth32Float
        )
    }

    /// Bytes of a texel, or of a 4 × 4 block when compressed.
    pub fn bytes(self) -> u32 {
        match self {
            TexFormat::R8 => 1,
            TexFormat::Rg8 => 2,
            TexFormat::Rgba8 | TexFormat::Depth32Float => 4,
            TexFormat::Rgba32Float => 16,
            TexFormat::Bc1 | TexFormat::Etc2Rgb8 | TexFormat::Etc2Rgb8A1 => 8,
            TexFormat::Bc2 | TexFormat::Bc3 | TexFormat::Etc2Rgba8 => 16,
        }
    }
}

struct TextureInner {
    live: Counted,
    gl: C,
    raw: WebGlTexture,
    id: u32,
    target: u32,
    format: TexFormat,
    width: u32,
    height: u32,
    levels: u32,
}

impl Drop for TextureInner {
    fn drop(&mut self) {
        self.gl.delete_texture(Some(&self.raw));
        let bytes = texture_bytes(self);
        self.live.update(|l| {
            l.textures -= 1;
            l.texture_bytes -= bytes;
        });
    }
}

/// Bytes `t`'s levels (and faces) take.
fn texture_bytes(t: &TextureInner) -> u64 {
    let f = t.format;
    let faces = if t.target == C::TEXTURE_CUBE_MAP {
        6
    } else {
        1
    };
    (0..t.levels)
        .map(|l| {
            let (w, h) = ((t.width >> l).max(1), (t.height >> l).max(1));
            let (w, h) = if f.compressed() {
                (w.div_ceil(4), h.div_ceil(4))
            } else {
                (w, h)
            };
            u64::from(w) * u64::from(h) * u64::from(f.bytes())
        })
        .sum::<u64>()
        * faces
}

/// The counts a resource is in: every live one's, and the kept ones' if
/// made while [`Gl::keeping`].
struct Counted {
    all: Rc<Cell<Live>>,
    kept: Option<Rc<Cell<Live>>>,
}

impl Counted {
    fn update(&self, f: impl Fn(&mut Live)) {
        for c in std::iter::once(&self.all).chain(&self.kept) {
            let mut l = c.get();
            f(&mut l);
            c.set(l);
        }
    }
}

/// The resources a [`Gl`] made that live: their counts and bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Live {
    pub buffers: u64,
    pub buffer_bytes: u64,
    pub textures: u64,
    pub texture_bytes: u64,
    pub vertex_arrays: u64,
}

impl std::ops::Sub for Live {
    type Output = Live;

    fn sub(self, o: Live) -> Live {
        Live {
            buffers: self.buffers - o.buffers,
            buffer_bytes: self.buffer_bytes - o.buffer_bytes,
            textures: self.textures - o.textures,
            texture_bytes: self.texture_bytes - o.texture_bytes,
            vertex_arrays: self.vertex_arrays - o.vertex_arrays,
        }
    }
}

/// A texture (a handle).
#[derive(Clone)]
pub struct Texture(Rc<TextureInner>);

impl Texture {
    pub fn size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }

    pub fn levels(&self) -> u32 {
        self.0.levels
    }

    pub fn format(&self) -> TexFormat {
        self.0.format
    }

    pub fn is_cube(&self) -> bool {
        self.0.target == C::TEXTURE_CUBE_MAP
    }

    /// Whether `other` is this texture.
    pub fn same(&self, other: &Texture) -> bool {
        self.0.id == other.0.id
    }

    /// Bytes its levels (and faces) take.
    pub fn bytes(&self) -> u64 {
        texture_bytes(&self.0)
    }
}

impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Texture({}, {:?} {}×{})",
            self.0.id, self.0.format, self.0.width, self.0.height
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    Nearest,
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wrap {
    Repeat,
    Clamp,
}

/// A sampler's description.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerDesc {
    pub mag: Filter,
    pub min: Filter,
    /// Between mip levels.
    pub mip: Filter,
    /// U, V, W.
    pub wrap: [Wrap; 3],
    /// 1 for none.
    pub anisotropy: u16,
    /// Only the top level is read (the maximum level of detail 0).
    pub top_level: bool,
}

impl Default for SamplerDesc {
    /// Nearest everywhere, clamped.
    fn default() -> Self {
        Self {
            mag: Filter::Nearest,
            min: Filter::Nearest,
            mip: Filter::Nearest,
            wrap: [Wrap::Clamp; 3],
            anisotropy: 1,
            top_level: false,
        }
    }
}

struct SamplerInner {
    gl: C,
    raw: WebGlSampler,
    id: u32,
}

impl Drop for SamplerInner {
    fn drop(&mut self) {
        self.gl.delete_sampler(Some(&self.raw));
    }
}

/// A sampler (a handle).
#[derive(Clone)]
pub struct Sampler(Rc<SamplerInner>);

struct ProgramInner {
    gl: C,
    raw: WebGlProgram,
    id: u32,
    /// Its units: (unit, the WGSL texture, the WGSL sampler).
    units: Vec<Unit>,
    first_instance: Option<WebGlUniformLocation>,
}

impl Drop for ProgramInner {
    fn drop(&mut self) {
        self.gl.delete_program(Some(&self.raw));
    }
}

/// A linked program (a handle).
#[derive(Clone)]
pub struct Program(Rc<ProgramInner>);

impl Program {
    /// The units it reads: (unit, the WGSL texture's and sampler's (group,
    /// binding)).
    pub fn units(&self) -> &[Unit] {
        &self.0.units
    }

    /// Whether it reads the first instance (naga's uniform for it).
    pub fn reads_first_instance(&self) -> bool {
        self.0.first_instance.is_some()
    }
}

/// A texture unit a program reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unit {
    pub unit: u32,
    /// The WGSL texture's (group, binding).
    pub texture: (u32, u32),
    /// The WGSL sampler's (group, binding) it is read through.
    pub sampler: Option<(u32, u32)>,
}

/// Vertex formats the renderer uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
    Unorm8x4,
    Sint16x2,
    Sint16x4,
    Uint32,
}

impl VertexFormat {
    /// (components, type, read as integers).
    fn describe(self) -> (i32, u32, bool) {
        match self {
            VertexFormat::Float32x2 => (2, C::FLOAT, false),
            VertexFormat::Float32x3 => (3, C::FLOAT, false),
            VertexFormat::Float32x4 => (4, C::FLOAT, false),
            VertexFormat::Unorm8x4 => (4, C::UNSIGNED_BYTE, false),
            VertexFormat::Sint16x2 => (2, C::SHORT, true),
            VertexFormat::Sint16x4 => (4, C::SHORT, true),
            VertexFormat::Uint32 => (1, C::UNSIGNED_INT, true),
        }
    }
}

/// One attribute: its location, format and offset in the vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub location: u32,
    pub format: VertexFormat,
    pub offset: u32,
}

/// How a vertex buffer's elements are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexLayout {
    pub stride: u32,
    /// Stepped per instance, else per vertex.
    pub instanced: bool,
    pub attributes: &'static [Attribute],
}

struct VertexArrayInner {
    live: Counted,
    gl: C,
    raw: WebGlVertexArrayObject,
    id: u32,
}

impl Drop for VertexArrayInner {
    fn drop(&mut self) {
        self.gl.delete_vertex_array(Some(&self.raw));
        self.live.update(|l| l.vertex_arrays -= 1);
    }
}

/// A vertex array (a handle).
#[derive(Clone)]
pub struct VertexArray(Rc<VertexArrayInner>);

struct FramebufferInner {
    gl: C,
    raw: WebGlFramebuffer,
    id: u32,
    color: Option<Texture>,
    depth: Option<Texture>,
}

impl Drop for FramebufferInner {
    fn drop(&mut self) {
        self.gl.delete_framebuffer(Some(&self.raw));
    }
}

/// A framebuffer with its attachments (a handle).
#[derive(Clone)]
pub struct Framebuffer(Rc<FramebufferInner>);

impl Framebuffer {
    pub fn color(&self) -> Option<&Texture> {
        self.0.color.as_ref()
    }

    pub fn depth(&self) -> Option<&Texture> {
        self.0.depth.as_ref()
    }
}

/// Blend factors and equation, `(source, destination, operation)` for
/// colour and alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blend {
    pub color: (u32, u32, u32),
    pub alpha: (u32, u32, u32),
}

impl Blend {
    /// Straight alpha.
    pub const ALPHA: Self = Self {
        color: (C::SRC_ALPHA, C::ONE_MINUS_SRC_ALPHA, C::FUNC_ADD),
        alpha: (C::ONE, C::ONE_MINUS_SRC_ALPHA, C::FUNC_ADD),
    };
}

/// The depth test and write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Depth {
    pub compare: u32,
    pub write: bool,
}

/// The front face as a WebGPU pipeline states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontFace {
    Ccw,
    Cw,
}

impl FrontFace {
    /// Flipped: the programs flip positions in y.
    fn gl(self) -> u32 {
        match self {
            FrontFace::Ccw => C::CW,
            FrontFace::Cw => C::CCW,
        }
    }
}

/// A draw's fixed-function state, besides its program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterState {
    pub blend: Option<Blend>,
    /// `None`: no depth test (and no write).
    pub depth: Option<Depth>,
    /// The face culled (`BACK`, `FRONT`), `None` for none.
    pub cull: Option<u32>,
    pub front_face: FrontFace,
    pub color_mask: [bool; 4],
}

impl RasterState {
    /// No blending, depth or culling.
    pub const OPAQUE: Self = Self {
        blend: None,
        depth: None,
        cull: None,
        front_face: FrontFace::Ccw,
        color_mask: [true; 4],
    };
}

/// A fence after submitted work.
pub struct Fence {
    gl: C,
    raw: Option<WebGlSync>,
}

impl Fence {
    /// Whether the GPU is done with what was issued before the fence (a
    /// WebGL fence never signals in the task it was made in: WebGL 2
    /// specification).
    pub fn done(&self) -> bool {
        self.raw.as_ref().is_none_or(|raw| {
            self.gl
                .get_sync_parameter(raw, C::SYNC_STATUS)
                .as_f64()
                .is_some_and(|s| s as u32 == C::SIGNALED)
        })
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        self.gl.delete_sync(self.raw.as_ref());
    }
}

/// GL's constants, for the renderer's parts.
pub use web_sys::WebGl2RenderingContext as Consts;

/// Checks the layer on `context` (a context of its own, at least 4 × 4):
/// a textured quad drawn through a framebuffer and read back, and a second
/// identical draw making no state calls. The `gl_selftest` op (tests).
pub fn selftest(context: C) -> Result<String, String> {
    let gl = Gl::new(context);
    let stage = |m: &str, e: &str| crate::glsl::stage(m, e).ok_or(format!("no {m} {e}"));
    let program = gl.program(stage("ui", "vs")?, stage("ui", "fs")?, |_| 0, |_| 0)?;
    // One texel of orange; the quad multiplies it by white.
    let texture = gl.texture(TexFormat::Rgba8, 1, 1, 1, false);
    gl.write_texture(&texture, 0, 0, 0, 0, 1, 1, &mut [255, 128, 0, 255]);
    let sampler = gl.sampler(SamplerDesc::default());
    const UI: VertexLayout = VertexLayout {
        stride: 20,
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
    let mut vertices = Vec::new();
    for (x, y) in [
        (0.0f32, 0.0f32),
        (0.0, 768.0),
        (1024.0, 0.0),
        (1024.0, 768.0),
    ] {
        for v in [x, y, 0.5, 0.5] {
            vertices.extend_from_slice(&v.to_le_bytes());
        }
        vertices.extend_from_slice(&[255; 4]);
    }
    let buffer = gl.buffer_with(BufferKind::Vertex, &vertices);
    let va = gl.vertex_array(&[(&buffer, 0, &UI)], None);
    let target = gl.texture(TexFormat::Rgba8, 4, 4, 1, false);
    let fb = gl.framebuffer(Some(&target), None);
    let draw = || {
        gl.bind_draw_framebuffer(Some(&fb));
        gl.viewport(0, 0, 4, 4);
        gl.use_program(&program);
        gl.bind_vertex_array(&va);
        gl.bind_texture(0, &texture, &sampler);
        gl.set_raster(&RasterState::OPAQUE);
        gl.draw_arrays(Consts::TRIANGLE_STRIP, 0, 4, 1);
    };
    gl.bind_draw_framebuffer(Some(&fb));
    gl.clear(Some([0.0, 0.0, 0.0, 1.0]), None);
    draw();
    let rgba = gl.read_pixels(&fb, 4, 4);
    if rgba.chunks(4).any(|p| p != [255, 128, 0, 255]) {
        return Err(format!("drawn {:?}", &rgba[..16]));
    }
    gl.take_counts();
    draw();
    let again = gl.take_counts();
    if again.total() != again.get(Call::Draw) || again.get(Call::Draw) != 1 {
        return Err(format!("an identical draw made state calls: {again:?}"));
    }
    let fence = gl.fence();
    Ok(format!("fence done at once: {}", fence.done()))
}
