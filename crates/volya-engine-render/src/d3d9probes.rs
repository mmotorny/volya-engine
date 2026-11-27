// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Direct3D 9 probes' scenes (tests/d3d9probes/, docs/testing.md)
//! replayed through the GL layer, for the `d3d9probes` browser suite to
//! compare with a reference's frames: `scenes.bin` (its format in
//! `probe.h`) read into cases, each drawn with the probe program
//! (`probe.wgsl`) into a target of its size and read back.
//!
//! Where our renderer has a choice, the replay makes it: the alpha test as
//! our programs' `a · 255` against the reference, the LOD bias through
//! `textureSampleBias`, fog as the blend with a factor interpolated from the
//! vertices, the gamma ramp through [`RampPass`]. Direct3D 9's pixel
//! centres are at integer coordinates, so positions are moved half a pixel
//! (our UI and world programs make no such move). A state the replay doesn't
//! model at a value other than the one it assumes fails the case rather than
//! being ignored.

use std::collections::HashMap;

use crate::gl::{
    Attribute, Blend, BufferKind, Consts as C, Filter, Gl, RasterState, SamplerDesc, TexFormat,
    Texture, VertexFormat, VertexLayout, Wrap,
};
use crate::webgl::ramp::RampPass;
use crate::webgl::{Target, link_with};

/// One case of a probe: what it draws, in order.
#[derive(Debug, PartialEq)]
pub struct Case {
    pub probe: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// The colour the target is cleared to (ARGB).
    pub clear: u32,
    pub steps: Vec<Step>,
    /// The gamma ramp the frame is mapped through (red, green, blue).
    pub ramp: Option<Box<[[u16; 256]; 3]>>,
}

/// A record of a case.
#[derive(Debug, PartialEq)]
pub enum Step {
    /// A render state and its value.
    Rs(u32, u32),
    /// A texture stage's state: stage, state, value.
    Tss(u32, u32, u32),
    /// A sampler's state: sampler, state, value.
    Ss(u32, u32, u32),
    Texture(TextureData),
    /// A stage's texture (0: none).
    SetTexture(u32, u32),
    /// A primitive type, its primitive count and the vertices (32 bytes
    /// each).
    Draw(u32, u32, Vec<u8>),
}

/// A texture's levels as the probe stored them.
#[derive(Debug, PartialEq)]
pub struct TextureData {
    pub id: u32,
    pub format: u32,
    pub width: u32,
    pub height: u32,
    /// Each level's bytes, from level 0.
    pub levels: Vec<Vec<u8>>,
}

const TAG_CASE: u32 = 1;
const TAG_RS: u32 = 2;
const TAG_TSS: u32 = 3;
const TAG_SS: u32 = 4;
const TAG_TEXTURE: u32 = 5;
const TAG_SET_TEXTURE: u32 = 6;
const TAG_DRAW: u32 = 7;
const TAG_RAMP: u32 = 8;
const TAG_END: u32 = 9;

/// D3DFORMAT values the probes use.
const A8R8G8B8: u32 = 21;
const DXT1: u32 = u32::from_le_bytes(*b"DXT1");
const DXT5: u32 = u32::from_le_bytes(*b"DXT5");

const VERTEX_BYTES: usize = 32;

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("four bytes"))
}

/// The bytes of a level `width` × `height` of `format`.
fn level_bytes(format: u32, width: u32, height: u32) -> Result<usize, String> {
    let (w, h) = (width as usize, height as usize);
    match format {
        A8R8G8B8 => Ok(w * h * 4),
        DXT1 => Ok(w.div_ceil(4) * h.div_ceil(4) * 8),
        DXT5 => Ok(w.div_ceil(4) * h.div_ceil(4) * 16),
        _ => Err(format!("texture format {format:#x}")),
    }
}

/// A NUL-terminated string from `b` at `at`, and where it ends.
fn string_at(b: &[u8], at: usize) -> Result<(String, usize), String> {
    let end = b[at..]
        .iter()
        .position(|&c| c == 0)
        .ok_or("a name without its end")?;
    let s = std::str::from_utf8(&b[at..at + end]).map_err(|e| e.to_string())?;
    Ok((s.to_owned(), at + end + 1))
}

/// The cases of `scenes.bin`.
pub fn parse(bytes: &[u8]) -> Result<Vec<Case>, String> {
    if bytes.len() < 8 || &bytes[..4] != b"VD9P" {
        return Err("not a scenes.bin".into());
    }
    if u32_at(bytes, 4) != 1 {
        return Err(format!("scenes.bin version {}", u32_at(bytes, 4)));
    }
    let mut cases = Vec::new();
    let mut case: Option<Case> = None;
    let mut at = 8;
    while at < bytes.len() {
        if bytes.len() - at < 8 {
            return Err(format!("a record cut short at byte {at}"));
        }
        let (tag, size) = (u32_at(bytes, at), u32_at(bytes, at + 4) as usize);
        let p = bytes
            .get(at + 8..at + 8 + size)
            .ok_or_else(|| format!("a record cut short at byte {at}"))?;
        at += 8 + size;
        let need = |n: usize| {
            if p.len() < n {
                Err(format!("record {tag}: {} bytes", p.len()))
            } else {
                Ok(())
            }
        };
        if tag == TAG_CASE {
            if let Some(c) = &case {
                return Err(format!("{}/{} has no end", c.probe, c.name));
            }
            need(13)?;
            let (probe, next) = string_at(p, 12)?;
            let (name, _) = string_at(p, next)?;
            case = Some(Case {
                probe,
                name,
                width: u32_at(p, 0),
                height: u32_at(p, 4),
                clear: u32_at(p, 8),
                steps: Vec::new(),
                ramp: None,
            });
            continue;
        }
        let c = case
            .as_mut()
            .ok_or_else(|| format!("record {tag} outside a case"))?;
        let step = match tag {
            TAG_RS => {
                need(8)?;
                Step::Rs(u32_at(p, 0), u32_at(p, 4))
            }
            TAG_TSS => {
                need(12)?;
                Step::Tss(u32_at(p, 0), u32_at(p, 4), u32_at(p, 8))
            }
            TAG_SS => {
                need(12)?;
                Step::Ss(u32_at(p, 0), u32_at(p, 4), u32_at(p, 8))
            }
            TAG_TEXTURE => {
                need(20)?;
                let (format, width, height) = (u32_at(p, 4), u32_at(p, 8), u32_at(p, 12));
                let mut levels = Vec::new();
                let mut from = 20;
                for l in 0..u32_at(p, 16) {
                    let n = level_bytes(format, (width >> l).max(1), (height >> l).max(1))?;
                    let level = p
                        .get(from..from + n)
                        .ok_or("a texture's levels cut short")?;
                    levels.push(level.to_vec());
                    from += n;
                }
                Step::Texture(TextureData {
                    id: u32_at(p, 0),
                    format,
                    width,
                    height,
                    levels,
                })
            }
            TAG_SET_TEXTURE => {
                need(8)?;
                Step::SetTexture(u32_at(p, 0), u32_at(p, 4))
            }
            TAG_DRAW => {
                need(12)?;
                let count = u32_at(p, 8) as usize;
                let vertices = p
                    .get(12..12 + count * VERTEX_BYTES)
                    .ok_or("a draw's vertices cut short")?;
                Step::Draw(u32_at(p, 0), u32_at(p, 4), vertices.to_vec())
            }
            TAG_RAMP => {
                need(3 * 256 * 2)?;
                let mut ramp = Box::new([[0u16; 256]; 3]);
                for (i, v) in ramp.iter_mut().flatten().enumerate() {
                    *v = u16::from_le_bytes([p[2 * i], p[2 * i + 1]]);
                }
                c.ramp = Some(ramp);
                continue;
            }
            TAG_END => {
                cases.extend(case.take());
                continue;
            }
            _ => return Err(format!("record tag {tag}")),
        };
        c.steps.push(step);
    }
    if let Some(c) = case {
        return Err(format!("{}/{} has no end", c.probe, c.name));
    }
    Ok(cases)
}

// Direct3D 9's states the replay reads (d3d9types.h's values).
mod rs {
    pub const ZENABLE: u32 = 7;
    pub const FILLMODE: u32 = 8;
    pub const SHADEMODE: u32 = 9;
    pub const ZWRITEENABLE: u32 = 14;
    pub const ALPHATESTENABLE: u32 = 15;
    pub const SRCBLEND: u32 = 19;
    pub const DESTBLEND: u32 = 20;
    pub const CULLMODE: u32 = 22;
    pub const ALPHAREF: u32 = 24;
    pub const ALPHAFUNC: u32 = 25;
    pub const DITHERENABLE: u32 = 26;
    pub const ALPHABLENDENABLE: u32 = 27;
    pub const FOGENABLE: u32 = 28;
    pub const SPECULARENABLE: u32 = 29;
    pub const FOGCOLOR: u32 = 34;
    pub const FOGTABLEMODE: u32 = 35;
    pub const FOGSTART: u32 = 36;
    pub const FOGEND: u32 = 37;
    pub const LIGHTING: u32 = 137;
    pub const FOGVERTEXMODE: u32 = 140;
    pub const COLORWRITEENABLE: u32 = 168;
    pub const BLENDOP: u32 = 171;
    pub const SRGBWRITEENABLE: u32 = 194;
}

mod tss {
    pub const COLOROP: u32 = 1;
    pub const COLORARG1: u32 = 2;
    pub const COLORARG2: u32 = 3;
    pub const ALPHAOP: u32 = 4;
    pub const ALPHAARG1: u32 = 5;
    pub const ALPHAARG2: u32 = 6;
    pub const TEXCOORDINDEX: u32 = 11;
    pub const TEXTURETRANSFORMFLAGS: u32 = 24;
}

mod ss {
    pub const ADDRESSU: u32 = 1;
    pub const ADDRESSV: u32 = 2;
    pub const MAGFILTER: u32 = 5;
    pub const MINFILTER: u32 = 6;
    pub const MIPFILTER: u32 = 7;
    pub const MIPMAPLODBIAS: u32 = 8;
    pub const MAXMIPLEVEL: u32 = 9;
    pub const MAXANISOTROPY: u32 = 10;
    pub const SRGBTEXTURE: u32 = 11;
}

const TOP_DISABLE: u32 = 1;
const TA_TEXTURE: u32 = 2;
const FOG_NONE: u32 = 0;
const FOG_LINEAR: u32 = 3;

/// The states in force, as last set.
#[derive(Default)]
struct States {
    rs: HashMap<u32, u32>,
    tss: HashMap<(u32, u32), u32>,
    ss: HashMap<(u32, u32), u32>,
    /// Stage 0's texture (0: none).
    texture: u32,
}

/// What a draw needs besides its vertices: the probe program's constants,
/// the fixed-function state and stage 0's sampler.
#[derive(Debug, PartialEq)]
struct Draw {
    constants: [f32; 24],
    raster: RasterState,
    sampler: SamplerDesc,
    /// Whether stage 0 reads its texture.
    textured: bool,
}

impl States {
    fn set(&mut self, step: &Step) {
        match *step {
            Step::Rs(s, v) => {
                self.rs.insert(s, v);
            }
            Step::Tss(stage, s, v) => {
                self.tss.insert((stage, s), v);
            }
            Step::Ss(sampler, s, v) => {
                self.ss.insert((sampler, s), v);
            }
            Step::SetTexture(0, id) => self.texture = id,
            _ => {}
        }
    }

    fn rs(&self, s: u32) -> u32 {
        self.rs.get(&s).copied().unwrap_or(0)
    }

    fn ss(&self, s: u32) -> u32 {
        self.ss.get(&(0, s)).copied().unwrap_or(0)
    }

    /// The draw these states make, or the first state the replay doesn't
    /// model.
    fn draw(&self, width: u32, height: u32) -> Result<Draw, String> {
        for (&s, &v) in &self.rs {
            let expected = match s {
                rs::ZENABLE
                | rs::ZWRITEENABLE
                | rs::LIGHTING
                | rs::DITHERENABLE
                | rs::SPECULARENABLE
                | rs::SRGBWRITEENABLE => Some(0),
                rs::FILLMODE => Some(3),  // D3DFILL_SOLID
                rs::SHADEMODE => Some(2), // D3DSHADE_GOURAUD
                rs::CULLMODE => Some(1),  // D3DCULL_NONE
                rs::FOGVERTEXMODE => Some(FOG_NONE),
                rs::FOGTABLEMODE if v != FOG_NONE => Some(FOG_LINEAR),
                rs::ALPHATESTENABLE
                | rs::SRCBLEND
                | rs::DESTBLEND
                | rs::ALPHAREF
                | rs::ALPHAFUNC
                | rs::ALPHABLENDENABLE
                | rs::FOGENABLE
                | rs::FOGCOLOR
                | rs::FOGTABLEMODE
                | rs::FOGSTART
                | rs::FOGEND
                | rs::COLORWRITEENABLE
                | rs::BLENDOP => None,
                _ => return Err(format!("render state {s} ({v})")),
            };
            if expected.is_some_and(|e| e != v) {
                return Err(format!("render state {s} at {v}"));
            }
        }
        let mut stage = [[0.0f32; 4]; 2];
        let mut textured = false;
        for (&(n, s), &v) in &self.tss {
            let ok = match (n, s) {
                (0, tss::COLOROP | tss::ALPHAOP) => (2..=4).contains(&v),
                (0, tss::COLORARG1 | tss::COLORARG2 | tss::ALPHAARG1 | tss::ALPHAARG2) => {
                    matches!(v, 0 | 1 | 2 | 4)
                }
                (0, tss::TEXCOORDINDEX | tss::TEXTURETRANSFORMFLAGS) => v == 0,
                (1, tss::COLOROP | tss::ALPHAOP) => v == TOP_DISABLE,
                _ => false,
            };
            if !ok {
                return Err(format!("texture stage {n} state {s} at {v}"));
            }
        }
        let tss = |s| self.tss.get(&(0, s)).copied().unwrap_or(0);
        for (i, (op, a1, a2)) in [
            (tss::COLOROP, tss::COLORARG1, tss::COLORARG2),
            (tss::ALPHAOP, tss::ALPHAARG1, tss::ALPHAARG2),
        ]
        .into_iter()
        .enumerate()
        {
            let (op, a1, a2) = (tss(op), tss(a1), tss(a2));
            // SELECTARG1 reads its first argument, SELECTARG2 its second,
            // MODULATE both.
            let read = match op {
                2 => [a1].to_vec(),
                3 => [a2].to_vec(),
                _ => [a1, a2].to_vec(),
            };
            textured |= read.contains(&TA_TEXTURE);
            stage[i] = [op as f32, a1 as f32, a2 as f32, 0.0];
        }
        if textured && self.texture == 0 {
            return Err("stage 0 reads a texture and has none".into());
        }
        for (&(n, s), &v) in &self.ss {
            let ok = n == 0
                && match s {
                    ss::ADDRESSU | ss::ADDRESSV => v == 1 || v == 3, // WRAP, CLAMP
                    ss::MAGFILTER | ss::MINFILTER => (1..=3).contains(&v),
                    ss::MIPFILTER => v <= 2,
                    ss::MIPMAPLODBIAS | ss::MAXANISOTROPY => true,
                    ss::MAXMIPLEVEL | ss::SRGBTEXTURE => v == 0,
                    _ => false,
                };
            if !ok {
                return Err(format!("sampler {n} state {s} at {v}"));
            }
        }
        let filter = |v| {
            if v <= 1 {
                Filter::Nearest
            } else {
                Filter::Linear
            }
        };
        let wrap = |v| if v == 1 { Wrap::Repeat } else { Wrap::Clamp };
        let min = self.ss(ss::MINFILTER);
        let mip = self.ss(ss::MIPFILTER);
        let sampler = SamplerDesc {
            mag: filter(self.ss(ss::MAGFILTER)),
            min: filter(min),
            mip: filter(mip),
            wrap: [
                wrap(self.ss(ss::ADDRESSU)),
                wrap(self.ss(ss::ADDRESSV)),
                Wrap::Clamp,
            ],
            // Only an anisotropic minification filter is anisotropic.
            anisotropy: if min == 3 {
                self.ss(ss::MAXANISOTROPY).clamp(1, 16) as u16
            } else {
                1
            },
            // D3DTEXF_NONE: the top level alone.
            top_level: mip == 0,
        };
        let float = |v: u32| f32::from_bits(v);
        let fog = if self.rs(rs::FOGENABLE) == 0 {
            0.0
        } else if self.rs(rs::FOGTABLEMODE) == FOG_LINEAR {
            2.0
        } else {
            1.0
        };
        let [_, r, g, b] = self.rs(rs::FOGCOLOR).to_be_bytes();
        let alpha_test = self.rs(rs::ALPHATESTENABLE) != 0;
        #[rustfmt::skip]
        let constants = [
            width as f32, height as f32, 0.0, 0.0,
            (self.rs(rs::ALPHAREF) & 0xFF) as f32, self.rs(rs::ALPHAFUNC) as f32,
            if alpha_test { 1.0 } else { 0.0 }, float(self.ss(ss::MIPMAPLODBIAS)),
            stage[0][0], stage[0][1], stage[0][2], 0.0,
            stage[1][0], stage[1][1], stage[1][2], 0.0,
            fog, float(self.rs(rs::FOGSTART)), float(self.rs(rs::FOGEND)), 0.0,
            f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 0.0,
        ];
        let mask = self.rs.get(&rs::COLORWRITEENABLE).copied().unwrap_or(0xF);
        let blend = if self.rs(rs::ALPHABLENDENABLE) != 0 {
            let src = factor(self.rs(rs::SRCBLEND))?;
            let dst = factor(self.rs(rs::DESTBLEND))?;
            let op = match self.rs.get(&rs::BLENDOP).copied().unwrap_or(1) {
                1 => C::FUNC_ADD,
                2 => C::FUNC_SUBTRACT,
                3 => C::FUNC_REVERSE_SUBTRACT,
                4 => C::MIN,
                5 => C::MAX,
                v => return Err(format!("blend operation {v}")),
            };
            Some(Blend {
                color: (src, dst, op),
                alpha: (src, dst, op),
            })
        } else {
            None
        };
        Ok(Draw {
            constants,
            raster: RasterState {
                blend,
                color_mask: [0, 1, 2, 3].map(|i| mask & (1 << i) != 0),
                ..RasterState::OPAQUE
            },
            sampler,
            textured,
        })
    }
}

/// GL's blend factor for D3DBLEND `v`.
fn factor(v: u32) -> Result<u32, String> {
    Ok(match v {
        1 => C::ZERO,
        2 => C::ONE,
        3 => C::SRC_COLOR,
        4 => C::ONE_MINUS_SRC_COLOR,
        5 => C::SRC_ALPHA,
        6 => C::ONE_MINUS_SRC_ALPHA,
        7 => C::DST_ALPHA,
        8 => C::ONE_MINUS_DST_ALPHA,
        9 => C::DST_COLOR,
        10 => C::ONE_MINUS_DST_COLOR,
        11 => C::SRC_ALPHA_SATURATE,
        _ => return Err(format!("blend factor {v}")),
    })
}

/// The probe's vertex: x, y, z, 1/w; diffuse and specular D3DCOLORs;
/// u, v.
const LAYOUT: VertexLayout = VertexLayout {
    stride: VERTEX_BYTES as u32,
    instanced: false,
    attributes: &[
        Attribute {
            location: 0,
            format: VertexFormat::Float32x4,
            offset: 0,
        },
        Attribute {
            location: 1,
            format: VertexFormat::Unorm8x4,
            offset: 16,
        },
        Attribute {
            location: 2,
            format: VertexFormat::Unorm8x4,
            offset: 20,
        },
        Attribute {
            location: 3,
            format: VertexFormat::Float32x2,
            offset: 24,
        },
    ],
};

/// A case's frame read back: RGBA rows from the top, or why it couldn't be
/// drawn.
pub struct Frame {
    pub probe: String,
    pub case: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Result<Vec<u8>, String>,
}

/// Every case of `scenes` drawn on `gl` (a context of its own).
pub fn replay(gl: &Gl, scenes: &[u8]) -> Result<Vec<Frame>, String> {
    let cases = parse(scenes)?;
    let program = link_with(gl, "probe", "vs", "fs", |_| 0, |_| 0);
    let constants = gl.buffer(BufferKind::Uniform, 24 * 4);
    let white = gl.texture(TexFormat::Rgba8, 1, 1, 1, false);
    gl.write_texture(&white, 0, 0, 0, 0, 1, 1, &mut [255; 4]);
    let ramp = RampPass::new(gl);
    let empty = gl.vertex_array(&[], None);
    Ok(cases
        .iter()
        .map(|case| Frame {
            probe: case.probe.clone(),
            case: case.name.clone(),
            width: case.width,
            height: case.height,
            rgba: draw_case(gl, case, &program, &constants, &white, &ramp, &empty),
        })
        .collect())
}

fn draw_case(
    gl: &Gl,
    case: &Case,
    program: &crate::gl::Program,
    constants: &crate::gl::Buffer,
    white: &Texture,
    ramp: &RampPass,
    empty: &crate::gl::VertexArray,
) -> Result<Vec<u8>, String> {
    let (w, h) = (case.width, case.height);
    let frame = Target::new(gl, (w, h));
    gl.bind_draw_framebuffer(Some(&frame.framebuffer));
    gl.viewport(0, 0, w as i32, h as i32);
    let [a, r, g, b] = case.clear.to_be_bytes().map(|c| f32::from(c) / 255.0);
    gl.clear(Some([r, g, b, a]), None);
    let mut states = States::default();
    let mut textures = HashMap::new();
    for step in &case.steps {
        match step {
            Step::Texture(t) => {
                textures.insert(t.id, upload(gl, t)?);
            }
            Step::Draw(kind, primitives, vertices) => {
                let d = states.draw(w, h)?;
                let (mode, count) = match kind {
                    4 => (C::TRIANGLES, primitives * 3),
                    5 => (C::TRIANGLE_STRIP, primitives + 2),
                    6 => (C::TRIANGLE_FAN, primitives + 2),
                    _ => return Err(format!("primitive type {kind}")),
                };
                if count as usize * VERTEX_BYTES > vertices.len() {
                    return Err(format!(
                        "{primitives} primitives of {kind} need {count} vertices"
                    ));
                }
                let texture = if d.textured {
                    textures
                        .get(&states.texture)
                        .ok_or_else(|| format!("no texture {}", states.texture))?
                } else {
                    white
                };
                let bytes: Vec<u8> = d.constants.iter().flat_map(|f| f.to_le_bytes()).collect();
                gl.write_buffer(constants, 0, &bytes);
                let buffer = gl.buffer_with(BufferKind::Vertex, vertices);
                let va = gl.vertex_array(&[(&buffer, 0, &LAYOUT)], None);
                gl.bind_draw_framebuffer(Some(&frame.framebuffer));
                gl.viewport(0, 0, w as i32, h as i32);
                gl.use_program(program);
                gl.bind_uniforms(0, constants, 0, constants.size());
                gl.bind_texture(0, texture, &gl.sampler(d.sampler));
                gl.bind_vertex_array(&va);
                gl.set_raster(&d.raster);
                gl.draw_arrays(mode, 0, count, 1);
            }
            _ => states.set(step),
        }
    }
    let Some(table) = &case.ramp else {
        return Ok(gl.read_pixels(&frame.framebuffer, w, h));
    };
    if table[0] != table[1] || table[0] != table[2] {
        return Err("a ramp whose channels differ".into());
    }
    let mapped = Target::new(gl, (w, h));
    ramp.draw(
        gl,
        empty,
        &frame,
        &mapped,
        [0.0, 0.0, w as f32, h as f32],
        &table[0],
    );
    Ok(gl.read_pixels(&mapped.framebuffer, w, h))
}

/// `t` as a texture: A8R8G8B8's B, G, R, A as RGBA, DXT1 and DXT5 as BC1
/// and BC3.
fn upload(gl: &Gl, t: &TextureData) -> Result<Texture, String> {
    let format = match t.format {
        A8R8G8B8 => TexFormat::Rgba8,
        DXT1 | DXT5 if !gl.extensions().bc => return Err("no BC textures".into()),
        DXT1 => TexFormat::Bc1,
        DXT5 => TexFormat::Bc3,
        f => return Err(format!("texture format {f:#x}")),
    };
    let texture = gl.texture(format, t.width, t.height, t.levels.len() as u32, false);
    for (l, level) in t.levels.iter().enumerate() {
        let (w, h) = ((t.width >> l).max(1), (t.height >> l).max(1));
        let mut data = level.clone();
        if format == TexFormat::Rgba8 {
            for p in data.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
            }
        }
        gl.write_texture(&texture, l as u32, 0, 0, 0, w, h, &mut data);
    }
    Ok(texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(out: &mut Vec<u8>, tag: u32, payload: &[u8]) {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
    }

    fn words(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A scenes.bin of one case: a 2x1 texture drawn as a triangle.
    fn scenes() -> Vec<u8> {
        let mut b = b"VD9P".to_vec();
        b.extend_from_slice(&1u32.to_le_bytes());
        let mut case = words(&[4, 2, 0xFF00_0000]);
        case.extend_from_slice(b"blend\0alpha\0");
        record(&mut b, TAG_CASE, &case);
        record(&mut b, TAG_RS, &words(&[rs::ALPHABLENDENABLE, 1]));
        record(&mut b, TAG_RS, &words(&[rs::SRCBLEND, 5]));
        record(&mut b, TAG_RS, &words(&[rs::DESTBLEND, 6]));
        record(&mut b, TAG_TSS, &words(&[0, tss::COLORARG1, TA_TEXTURE]));
        record(&mut b, TAG_SS, &words(&[0, ss::MINFILTER, 2]));
        let mut texture = words(&[1, A8R8G8B8, 2, 1, 1]);
        texture.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        record(&mut b, TAG_TEXTURE, &texture);
        record(&mut b, TAG_SET_TEXTURE, &words(&[0, 1]));
        let mut draw = words(&[4, 1, 3]);
        draw.extend_from_slice(&[0; 3 * VERTEX_BYTES]);
        record(&mut b, TAG_DRAW, &draw);
        record(&mut b, TAG_RAMP, &[0; 1536]);
        record(&mut b, TAG_END, &[]);
        b
    }

    #[test]
    fn reads_every_record() {
        let cases = parse(&scenes()).unwrap();
        assert_eq!(cases.len(), 1);
        let c = &cases[0];
        assert_eq!((c.probe.as_str(), c.name.as_str()), ("blend", "alpha"));
        assert_eq!((c.width, c.height, c.clear), (4, 2, 0xFF00_0000));
        assert_eq!(c.steps.len(), 8);
        assert_eq!(c.steps[0], Step::Rs(rs::ALPHABLENDENABLE, 1));
        assert_eq!(c.steps[4], Step::Ss(0, ss::MINFILTER, 2));
        let Step::Texture(t) = &c.steps[5] else {
            panic!("{:?}", c.steps[5]);
        };
        assert_eq!((t.id, t.width, t.height), (1, 2, 1));
        assert_eq!(t.levels, [vec![1, 2, 3, 4, 5, 6, 7, 8]]);
        assert_eq!(c.steps[6], Step::SetTexture(0, 1));
        assert!(matches!(&c.steps[7], Step::Draw(4, 1, v) if v.len() == 96));
        assert_eq!(c.ramp.as_deref(), Some(&[[0; 256]; 3]));
    }

    #[test]
    fn rejects_what_is_not_a_whole_log() {
        assert!(parse(b"VD9Q\x01\0\0\0").is_err());
        let whole = scenes();
        // Without its END record.
        assert!(parse(&whole[..whole.len() - 8]).is_err());
        assert!(parse(&whole[..whole.len() - 9]).is_err());
    }

    #[test]
    fn states_make_the_draw() {
        let mut s = States::default();
        for step in &parse(&scenes()).unwrap()[0].steps {
            s.set(step);
        }
        let d = s.draw(4, 2).unwrap();
        assert!(d.textured);
        let straight = (C::SRC_ALPHA, C::ONE_MINUS_SRC_ALPHA, C::FUNC_ADD);
        assert_eq!(
            d.raster.blend,
            Some(Blend {
                color: straight,
                alpha: straight
            })
        );
        assert_eq!(d.sampler.min, Filter::Linear);
        assert_eq!(d.sampler.mag, Filter::Nearest);
        assert!(d.sampler.top_level);
        assert_eq!(&d.constants[..4], &[4.0, 2.0, 0.0, 0.0]);
        // Stage 0's colour: SELECTARG1 of the texture.
        assert_eq!(&d.constants[8..11], &[0.0, 2.0, 0.0]);
    }

    #[test]
    fn states_the_replay_does_not_model_fail_the_draw() {
        let mut s = States::default();
        s.set(&Step::Rs(rs::CULLMODE, 3));
        assert!(s.draw(1, 1).is_err());
        let mut s = States::default();
        s.set(&Step::Ss(0, ss::ADDRESSU, 2));
        assert!(s.draw(1, 1).is_err());
        let mut s = States::default();
        s.set(&Step::Tss(0, tss::COLORARG1, TA_TEXTURE));
        s.set(&Step::Tss(0, tss::COLOROP, 2));
        assert!(s.draw(1, 1).is_err(), "a texture read with none set");
    }
}
