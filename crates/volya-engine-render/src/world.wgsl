// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The static world's passes (docs/behavior/static-shading.md sections 6–9).
// Vertex attributes arrive as stored in level.geom and are decoded here
// (docs/formats/level-geom.md section 8). Matrices are row-vector
// convention uploaded row by row, so `M * v` here is `v · M` of the specs.

struct Frame {
    view_projection: mat4x4<f32>,
    view: mat4x4<f32>,
    // xyz: camera position E; w: ssa_min (section 10.2).
    eye: vec4<f32>,
    // xyz: the camera's forward direction (wall marks); w: the mip bias in
    // force (texture-quality-filtering.md 5).
    forward: vec4<f32>,
    ambient: vec4<f32>,
    hemi: vec4<f32>,
    sun_color: vec4<f32>,
    // The direction the sunlight travels.
    sun_dir: vec4<f32>,
    fog_plane: vec4<f32>,
    // rgb: the fog unit's colour.
    fog_color: vec4<f32>,
    // Trees (section 7.3).
    wave: vec4<f32>,
    wind: vec4<f32>,
    // x: r1_lmodel_lerp (object-projected-light.md 6).
    projector: vec4<f32>,
}

struct Draw {
    // Model to world: identity for meshes, the tree's transform X.
    transform: mat4x4<f32>,
    // Bounding sphere: centre, radius.
    sphere: vec4<f32>,
    // Trees: c_scale, and c_bias without the ambient term.
    c_scale: vec4<f32>,
    c_bias: vec4<f32>,
    // c_sun.x, c_sun.y, detail scale (0: no detail), alpha ref (−1: none).
    params: vec4<f32>,
    // x: fog on (1) or off (0); y: tree object LOD (1) or not (0).
    flags: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
// The draws' records, DRAW_TEXELS RGBA32F texels each, DRAWS_PER_ROW to a
// row (world.rs): WebGL 2's vertex programs have no storage buffers.
@group(0) @binding(1) var draw_texels: texture_2d<f32>;

const DRAW_TEXELS: u32 = 9u;
const DRAWS_PER_ROW: u32 = 64u;

// The objects' render matrices (model_point).
@group(0) @binding(2) var bone_texels: texture_2d<f32>;

const BONE_TEXELS: u32 = 3u;
const BONES_PER_ROW: u32 = 64u;

fn record(i: u32) -> Draw {
    let x = i32((i % DRAWS_PER_ROW) * DRAW_TEXELS);
    let y = i32(i / DRAWS_PER_ROW);
    let t = array<vec4<f32>, DRAW_TEXELS>(
        textureLoad(draw_texels, vec2(x, y), 0),
        textureLoad(draw_texels, vec2(x + 1, y), 0),
        textureLoad(draw_texels, vec2(x + 2, y), 0),
        textureLoad(draw_texels, vec2(x + 3, y), 0),
        textureLoad(draw_texels, vec2(x + 4, y), 0),
        textureLoad(draw_texels, vec2(x + 5, y), 0),
        textureLoad(draw_texels, vec2(x + 6, y), 0),
        textureLoad(draw_texels, vec2(x + 7, y), 0),
        textureLoad(draw_texels, vec2(x + 8, y), 0),
    );
    return Draw(mat4x4(t[0], t[1], t[2], t[3]), t[4], t[5], t[6], t[7], t[8]);
}

// A level vertex's id when draws share it.
const SHARED: u32 = 0xffffffffu;

// The record of a level vertex's draw. Vertices only one draw uses carry
// its index (id), so draws merged into one draw call each keep their own
// record; shared vertices (e.g. a tree's instances) take the instance's,
// the draw drawn as its own instance.
fn level_record(id: u32, shared_record: u32) -> Draw {
    return record(select(id, shared_record, id == SHARED));
}

// A surface's textures in two groups: its light maps and environment map,
// which many surfaces share, and its base and detail textures, which change
// from surface to surface; one group of each per distinct set (#429). The
// latter is last, as it changes most often.
@group(1) @binding(0) var t_second: texture_2d<f32>;
@group(1) @binding(1) var t_hemi: texture_2d<f32>;
// The cube environment map of the objects' MODELEbB surfaces.
@group(1) @binding(2) var t_env: texture_cube<f32>;
// s_base, s_detail: wrap, anisotropic; s_lmap: wrap, linear;
// s_hemi: clamp, linear, no mip (section 4.4).
@group(1) @binding(3) var s_linear: sampler;
@group(1) @binding(4) var s_clamp: sampler;
// The projected light texture (object-projected-light.md 2), sampled
// through s_clamp: clamp, linear, no mip (section 6).
@group(1) @binding(9) var t_projector: texture_2d<f32>;
@group(2) @binding(0) var t_base: texture_2d<f32>;
@group(2) @binding(1) var t_detail: texture_2d<f32>;
@group(2) @binding(2) var s_aniso: sampler;

// Element 0 (near) when |E − C| − r < 44 m (section 5).
const NEAR_DISTANCE: f32 = 44.0;
// dt_params.w (section 4.3).
const DETAIL_FADE: f32 = 0.02;

struct Out {
    // Invariant: the light passes draw the same positions again.
    @invariant @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // Light-map coordinate (light-mapped) or detail coordinate.
    @location(1) uv2: vec2<f32>,
    @location(2) detail_uv: vec2<f32>,
    @location(3) c0: vec4<f32>,
    @location(4) c1: vec4<f32>,
    @location(5) fog: f32,
    // x: alpha ref, y: fog on, z: detail used.
    @location(6) @interpolate(flat) pass_flags: vec3<f32>,
    // The environment map's look-up direction (objects).
    @location(7) reflection: vec3<f32>,
    // The objects' sun term S(n), clamped (receivers).
    @location(8) sun: vec3<f32>,
}

struct LightMapped {
    // The record of a draw whose vertices are shared: the instance.
    @builtin(instance_index) record: u32,
    // The draw this vertex belongs to, or SHARED (see `level_record`).
    @location(7) id: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) binormal: vec4<f32>,
    @location(4) tc0: vec2<i32>,
    @location(5) tc1: vec2<i32>,
}

struct VertexLit {
    // The record of a draw whose vertices are shared: the instance.
    @builtin(instance_index) record: u32,
    // The draw this vertex belongs to, or SHARED (see `level_record`).
    @location(7) id: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) binormal: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) tc0: vec2<i32>,
}

struct TreeVertex {
    // The record of a draw whose vertices are shared: the instance.
    @builtin(instance_index) record: u32,
    // The draw this vertex belongs to, or SHARED (see `level_record`).
    @location(7) id: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) binormal: vec4<f32>,
    @location(4) tc: vec4<i32>,
}

// D3DCOLOR arrives as bytes (b0, b1, b2, b3) / 255: rgb is (b2, b1, b0).
fn rgb(e: vec4<f32>) -> vec3<f32> {
    return e.zyx;
}

fn unpack_vec(e: vec4<f32>) -> vec3<f32> {
    return 2.0 * e.zyx - 1.0;
}

// (s + tangent/binormal alpha) × 32 / 32768 (level-geom.md 8.2, 8.3).
fn base_uv(tc: vec2<i32>, tangent: vec4<f32>, binormal: vec4<f32>) -> vec2<f32> {
    return (vec2<f32>(tc) + vec2(tangent.w, binormal.w)) * (32.0 / 32768.0);
}

fn sun_term(n: vec3<f32>) -> vec3<f32> {
    return frame.sun_color.rgb * max(0.0, dot(n, -frame.sun_dir.xyz));
}

// Whether draw `d` is drawn this frame (ssa above ssa_min, section 10.2).
fn visible(d: Draw) -> bool {
    let to = frame.eye.xyz - d.sphere.xyz;
    return d.sphere.w / (dot(to, to) + 1e-5) > frame.eye.w;
}

// Whether draw `d` takes element 0, the near variant.
fn near(d: Draw) -> bool {
    return length(frame.eye.xyz - d.sphere.xyz) - d.sphere.w < NEAR_DISTANCE;
}

// Detail fade: (dt_mul, dt_add) at world position p.
fn detail_fade(p: vec3<f32>) -> vec2<f32> {
    let f = length(p - frame.eye.xyz) * DETAIL_FADE;
    let delta = min(f * f, 1.0);
    return vec2(1.0 - delta, delta / 2.0);
}

fn fog_factor(p: vec3<f32>) -> f32 {
    return clamp(dot(vec4(p, 1.0), frame.fog_plane), 0.0, 1.0);
}

// The common outputs; `detail` is whether element 0 with detail applies.
fn start(d: Draw, p: vec3<f32>, detail: bool) -> Out {
    var out: Out;
    out.clip = frame.view_projection * vec4(p, 1.0);
    if !visible(d) {
        // Outside the clip volume: the whole draw is dropped.
        out.clip = vec4(2.0, 2.0, 2.0, 1.0);
    }
    out.fog = fog_factor(p);
    out.pass_flags = vec3(d.params.w, d.flags.x, select(0.0, 1.0, detail));
    return out;
}

fn uses_detail(d: Draw) -> bool {
    return d.params.z > 0.0 && near(d);
}

// lmap / lmap_dt (section 7.1).
@vertex
fn vs_lmap(v: LightMapped) -> Out {
    let d = level_record(v.id, v.record);
    let detail = uses_detail(d);
    var out = start(d, v.position, detail);
    let n = unpack_vec(v.normal);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    out.uv2 = vec2<f32>(v.tc1) / 32768.0;
    out.detail_uv = out.uv * d.params.z;
    let fade = detail_fade(v.position);
    out.c0 = vec4(saturate(frame.hemi.rgb), fade.x);
    out.c1 = vec4(saturate(sun_term(n)), fade.y);
    return out;
}

fn alpha_test(a: f32, flags: vec3<f32>) {
    if flags.x >= 0.0 && a * 255.0 <= flags.x {
        discard;
    }
}

// The pixel result clamped, then the fixed fog unit (section 9.3).
fn finish(c: vec3<f32>, a: f32, in: Out) -> vec4<f32> {
    alpha_test(a, in.pass_flags);
    var color = saturate(c);
    if in.pass_flags.y > 0.5 {
        color = in.fog * color + (1.0 - in.fog) * frame.fog_color.rgb;
    }
    return vec4(color, a);
}

fn detail_factor(in: Out) -> vec3<f32> {
    let dt = textureSampleBias(t_detail, s_aniso, in.detail_uv, frame.forward.w).rgb;
    return dt * in.c0.a + in.c1.a;
}

@fragment
fn fs_lmap(in: Out) -> @location(0) vec4<f32> {
    let b = textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w);
    let lm = textureSampleBias(t_second, s_linear, in.uv2, frame.forward.w);
    let hm = textureSample(t_hemi, s_clamp, in.uv2);
    let detail = detail_factor(in);
    let h = (hm.r + hm.g + hm.b) / 3.0;
    let light = frame.ambient.rgb + lm.rgb + in.c1.rgb * lm.a + in.c0.rgb * h;
    var c = 2.0 * light * b.rgb;
    if in.pass_flags.z > 0.5 {
        c = 2.0 * c * detail;
    }
    return finish(c, b.a, in);
}

// impl_dt, terrain (section 7.4).
@vertex
fn vs_terrain(v: LightMapped) -> Out {
    let d = level_record(v.id, v.record);
    var out = start(d, v.position, d.params.z > 0.0);
    let n = unpack_vec(v.normal);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    out.uv2 = out.uv;
    out.detail_uv = out.uv * d.params.z;
    out.c0 = vec4(saturate(frame.hemi.rgb), 0.0);
    out.c1 = vec4(saturate(sun_term(n)), 0.0);
    return out;
}

@fragment
fn fs_terrain(in: Out) -> @location(0) vec4<f32> {
    let b = textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w);
    let lm = textureSampleBias(t_second, s_linear, in.uv2, frame.forward.w);
    let dt = textureSampleBias(t_detail, s_aniso, in.detail_uv, frame.forward.w).rgb;
    let light = frame.ambient.rgb + lm.rgb + in.c1.rgb * lm.a + in.c0.rgb * b.a;
    var c = 2.0 * light * b.rgb;
    if in.pass_flags.z > 0.5 {
        c = 2.0 * c * dt;
    }
    return finish(c, 1.0, in);
}

// Vertex lighting of vert / vert_dt (section 7.2).
fn vertex_light(v: VertexLit, n: vec3<f32>) -> vec3<f32> {
    let color = v.color;
    return saturate(rgb(color) + frame.hemi.rgb * v.normal.w + sun_term(n) * color.w
        + frame.ambient.rgb);
}

@vertex
fn vs_vert(v: VertexLit) -> Out {
    let d = level_record(v.id, v.record);
    let detail = uses_detail(d);
    var out = start(d, v.position, detail);
    let n = unpack_vec(v.normal);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    out.detail_uv = out.uv * d.params.z;
    let fade = detail_fade(v.position);
    out.c0 = vec4(vertex_light(v, n), fade.x);
    out.c1 = vec4(fade.y);
    return out;
}

// vert / vert_dt pixel stage: also trees and wall marks.
@fragment
fn fs_vert(in: Out) -> @location(0) vec4<f32> {
    let b = textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w);
    let detail = detail_factor(in);
    var c = 2.0 * b.rgb * in.c0.rgb;
    if in.pass_flags.z > 0.5 {
        c = 2.0 * c * detail;
    }
    return finish(c, b.a, in);
}

// The waving position of tree_w (section 7.3).
fn tree_wave(world: vec3<f32>, x: mat4x4<f32>, m: vec4<f32>) -> vec3<f32> {
    var p = world;
    let frac = m.z / 2048.0;
    let h = p.y - x[3].y;
    let phi = frame.wave.w + dot(p, frame.wave.xyz);
    let s = 2.0 * sqrt(2.0) * fract(phi) - sqrt(2.0);
    let c = s * s - 1.0;
    p.x += frame.wind.x * h * c * frac;
    p.z += frame.wind.z * h * c * frac;
    return p;
}

// tree_w / tree_s (with _dt), section 7.3.
@vertex
fn vs_tree(v: TreeVertex) -> Out {
    let d = level_record(v.id, v.record);
    let x = d.transform;
    let detail = uses_detail(d);
    let m = vec4<f32>(v.tc);
    var p = (x * vec4(v.position, 1.0)).xyz;
    // Element 0 of a tree without object LOD waves; everything else stays.
    if near(d) && d.flags.y < 0.5 {
        p = tree_wave(p, x, m);
    }
    var out = start(d, p, detail);
    let nw = (x * vec4(unpack_vec(v.normal), 0.0)).xyz;
    let l = v.normal.w;
    let u = d.c_scale * l + d.c_bias + vec4(frame.ambient.rgb, 0.0);
    let sun = frame.sun_color.rgb * (0.25 + 0.75 * dot(nw, -frame.sun_dir.xyz))
        * (l * d.params.x + d.params.y);
    out.uv = m.xy / 2048.0;
    out.detail_uv = out.uv * d.params.z;
    let fade = detail_fade(p);
    out.c0 = vec4(saturate(u.rgb + frame.hemi.rgb * u.w + sun), fade.x);
    out.c1 = vec4(fade.y);
    return out;
}

// The wall marks' position pushed off the surface (section 8.3).
fn wmark_position(position: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let e = frame.eye.xyz;
    var p = position + 0.007 * n;
    let s = mix(0.003, 0.011, length(e - position));
    p -= normalize(frame.forward.xyz + normalize(p - e)) * s;
    return p;
}

// wmark (section 8.3).
@vertex
fn vs_wmark(v: VertexLit) -> Out {
    let d = level_record(v.id, v.record);
    let n = unpack_vec(v.normal);
    let p = wmark_position(v.position, n);
    var out = start(d, p, false);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    out.c0 = vec4(vertex_light(v, n), 0.0);
    return out;
}

// simple: self-lit (section 8.1).
@vertex
fn vs_simple(v: VertexLit) -> Out {
    let d = level_record(v.id, v.record);
    var out = start(d, v.position, false);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    return out;
}

@fragment
fn fs_simple(in: Out) -> @location(0) vec4<f32> {
    let b = textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w);
    return finish(b.rgb, b.a, in);
}

// base_lplanes (section 8.2).
@vertex
fn vs_lplanes(v: VertexLit) -> Out {
    let d = level_record(v.id, v.record);
    var out = start(d, v.position, false);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    let pv = normalize((frame.view * vec4(v.position, 1.0)).xyz);
    let nv = normalize((frame.view * vec4(unpack_vec(v.normal), 0.0)).xyz);
    out.c0 = vec4(0.0, 0.0, 0.0, abs(dot(pv, nv)));
    return out;
}

@fragment
fn fs_lplanes(in: Out) -> @location(0) vec4<f32> {
    let b = textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w);
    return finish(b.rgb, b.a * in.c0.a, in);
}

// The objects' models (docs/behavior/model-shading.md 1, 2, 4 – 7),
// element 1 for every object. A vertex arrives quantized and is drawn at
// its stored position (at rest) or skinned (posed), transformed by the
// object's X (`transform`);
// `c_scale` holds L_dynamic_props, `sphere` the model's world sphere, so
// very small objects are skipped by the static meshes' size test
// (model-shading.md 3, object-detail-level.md 3).

struct ModelVertex {
    @builtin(instance_index) draw: u32,
    @location(0) position: vec4<i32>,
    @location(1) normal: vec4<f32>,
    @location(2) uv: vec2<i32>,
}

struct ModelPoint {
    p: vec3<f32>,
    n: vec3<f32>,
    uv: vec2<f32>,
}

// The render matrix K[i] (model-shading.md 2) as its columns 0 – 2,
// BONE_TEXELS texels each, BONES_PER_ROW to a row (world.rs).
fn bone_columns(i: u32) -> array<vec4<f32>, 3> {
    let x = i32((i % BONES_PER_ROW) * BONE_TEXELS);
    let y = i32(i / BONES_PER_ROW);
    return array<vec4<f32>, 3>(
        textureLoad(bone_texels, vec2(x, y), 0),
        textureLoad(bone_texels, vec2(x + 1, y), 0),
        textureLoad(bone_texels, vec2(x + 2, y), 0),
    );
}

// q · 12 / 32768, 2c/255 − 1 and q · 16 / 32768 (section 4.7); a posed
// object's vertex skinned (model-shading.md 2: flags.w its first render
// matrix + 1, the vertex's bones b₀ + 64 b₁ in position.w and the weight
// of b₁ in normal.w): by K[b₀] alone with element 1, by (1 − w) K[b₀] +
// w K[b₁] with element 0, a projected-light receiver's (params.x > 0);
// the normal turned by X's rotation and normalised (section 5.5).
fn model_point(d: Draw, v: ModelVertex) -> ModelPoint {
    var m: ModelPoint;
    var local = vec3<f32>(v.position.xyz) * (12.0 / 32768.0);
    var n = 2.0 * v.normal.xyz - 1.0;
    if d.flags.w > 0.5 {
        let first = u32(d.flags.w - 0.5);
        let links = u32(v.position.w);
        let k0 = bone_columns(first + links % 64u);
        let k1 = bone_columns(first + links / 64u);
        let w = select(0.0, v.normal.w, d.params.x > 0.5);
        let c0 = mix(k0[0], k1[0], w);
        let c1 = mix(k0[1], k1[1], w);
        let c2 = mix(k0[2], k1[2], w);
        let p = vec4(local, 1.0);
        local = vec3(dot(p, c0), dot(p, c1), dot(p, c2));
        let q = vec4(n, 0.0);
        n = vec3(dot(q, c0), dot(q, c1), dot(q, c2));
    }
    m.p = (d.transform * vec4(local, 1.0)).xyz;
    m.n = normalize((d.transform * vec4(n, 0.0)).xyz);
    m.uv = vec2<f32>(v.uv) * (16.0 / 32768.0);
    return m;
}

// The model light Q, clamped as a vertex colour (section 5.5).
fn model_light(d: Draw, n: vec3<f32>) -> vec3<f32> {
    let props = d.c_scale;
    return saturate((n.y * 0.5 + 0.5) * props.w * frame.hemi.rgb + frame.ambient.rgb
        + props.xyz * sun_term(n));
}

// A receiver's projected coordinate (object-projected-light.md 6): the
// world position clamped into its slot's box and mapped into its cell, as
// (u d, v d, d) with d the depth below the cell's eye, divided per pixel;
// w 1. A draw that is no receiver this frame (params.x 0) gets w 0. The
// record holds C′ and h in c_bias and the slot + 1 in params.x. In WebGL's
// pixel centres the coordinate is where 4.1 draws the point (Direct3D 9's
// lookup of section 6 is half a texel off ours, landing on the same texel).
fn projected(d: Draw, p: vec3<f32>) -> vec4<f32> {
    if d.params.x < 0.5 {
        return vec4(0.0);
    }
    let slot = u32(d.params.x - 0.5);
    let c = d.c_bias.xyz;
    let h = d.c_bias.w;
    let lo = vec3(c.x - h + 0.05, c.y - h - 7.0 + 0.05, c.z - h + 0.05);
    let hi = vec3(c.x + h - 0.05, c.y - 0.05, c.z + h - 0.05);
    let q = clamp(p, lo, hi);
    let depth = c.y + 200.0 - q.y;
    let k = 25.5 * 200.0 / h;
    let cx = f32(slot % 10u) * 51.0 + 25.5;
    let cy = f32(slot / 10u) * 51.0 + 25.5 + 51.0 / 1024.0;
    let u = (cx * depth + k * (q.x - c.x)) / 512.0;
    let v = (cy * depth - k * (q.z - c.z)) / 512.0;
    return vec4(u, v, depth, 1.0);
}

// The fade φ = c + (1 − c) g², g = (|E − C′| + R) / (50 R), R = h / 1.1.
fn projected_fade(d: Draw) -> f32 {
    let r = d.c_bias.w / 1.1;
    let g = (length(frame.eye.xyz - d.c_bias.xyz) + r) / (50.0 * r);
    let c = frame.projector.x;
    return c + (1.0 - c) * g * g;
}

// model_def_lq, or model_def_hq for a receiver (model-shading.md 7): Q
// in c0.rgb, φ in c0.a, the projected coordinate in c1, S(n) in sun.
@vertex
fn vs_model(v: ModelVertex) -> Out {
    let d = record(v.draw);
    let m = model_point(d, v);
    var out = start(d, m.p, false);
    out.uv = m.uv;
    out.c0 = vec4(model_light(d, m.n), projected_fade(d));
    out.c1 = projected(d, m.p);
    out.sun = saturate(sun_term(m.n));
    return out;
}

// A receiver's light lerp(t.rgb + S(n) · t.a, Q, φ); Q for the others.
fn receiver_light(in: Out) -> vec3<f32> {
    if in.c1.w < 0.5 {
        return in.c0.rgb;
    }
    let t = textureSampleLevel(t_projector, s_clamp, in.c1.xy / in.c1.z, 0.0);
    return mix(t.rgb + in.sun * t.a, in.c0.rgb, in.c0.a);
}

// C = 2 · light · B.rgb, A = B.a.
@fragment
fn fs_model(in: Out) -> @location(0) vec4<f32> {
    let b = textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w);
    return finish(2.0 * receiver_light(in) * b.rgb, b.a, in);
}

// model_env_lq: the view direction reflected about the normal picks the
// environment texel (our reading of calc_reflection).
@vertex
fn vs_model_env(v: ModelVertex) -> Out {
    let d = record(v.draw);
    let m = model_point(d, v);
    var out = start(d, m.p, false);
    out.uv = m.uv;
    out.c0 = vec4(model_light(d, m.n), projected_fade(d));
    out.c1 = projected(d, m.p);
    out.sun = saturate(sun_term(m.n));
    out.reflection = reflect(normalize(m.p - frame.eye.xyz), m.n);
    return out;
}

// model_env_lq / model_env_hq.
@fragment
fn fs_model_env(in: Out) -> @location(0) vec4<f32> {
    let b = textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w);
    let e = textureSample(t_env, s_clamp, in.reflection).rgb;
    let base = mix(e, b.rgb, b.a);
    return finish(2.0 * receiver_light(in) * base, b.a, in);
}

// model_def_lqs with simple: the base texel, unlit.
@vertex
fn vs_model_simple(v: ModelVertex) -> Out {
    let d = record(v.draw);
    let m = model_point(d, v);
    var out = start(d, m.p, false);
    out.uv = m.uv;
    return out;
}

// model_def_lplanes: faded by |view direction · normal| (as 8.2).
@vertex
fn vs_model_lplanes(v: ModelVertex) -> Out {
    let d = record(v.draw);
    let m = model_point(d, v);
    var out = start(d, m.p, false);
    out.uv = m.uv;
    let pv = normalize((frame.view * vec4(m.p, 1.0)).xyz);
    let nv = normalize((frame.view * vec4(m.n, 0.0)).xyz);
    out.c0 = vec4(0.0, 0.0, 0.0, abs(dot(pv, nv)));
    return out;
}

// The dynamic light passes (docs/behavior/dynamic-lights.md section 6):
// elements 2 (point, `add_point`) and 3 (spot, `add_spot`) of the
// shaders that have them, drawn again over the lit geometry, adding.

struct Light {
    // L_dynamic_pos: the light's position, 0.5 / r (point) or 1 / r (spot).
    pos: vec4<f32>,
    // L_dynamic_color: the colour times the fade, alpha 1.
    color: vec4<f32>,
    // L_dynamic_xform (spot passes).
    xform: mat4x4<f32>,
    // x: 1 for a spot pass, 0 for a point pass.
    kind: vec4<f32>,
}

// In place of the light maps' group (its bindings after theirs).
@group(1) @binding(5) var<uniform> light: Light;
// s_lmap and s_att: clamped, linear, the first level only (section 6.2).
@group(1) @binding(6) var t_lmap: texture_2d<f32>;
@group(1) @binding(7) var t_att: texture_2d<f32>;
@group(1) @binding(8) var s_att: sampler;

struct LightOut {
    @invariant @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // c, a colour output (clamped).
    @location(1) c: vec4<f32>,
    // t₁ with its divisor (spot: projective; point: 1), and t₂.
    @location(2) t1: vec3<f32>,
    @location(3) t2: vec2<f32>,
    // The light elements' alpha reference.
    @location(4) @interpolate(flat) aref: f32,
}

// Section 6.3's vertex stage at world position p with normal n.
fn light_vertex(d: Draw, p: vec3<f32>, n: vec3<f32>, uv: vec2<f32>) -> LightOut {
    var out: LightOut;
    out.clip = frame.view_projection * vec4(p, 1.0);
    // The projected-size measure, as in the camera's pass (5.5 rule 3).
    if !visible(d) {
        out.clip = vec4(2.0, 2.0, 2.0, 1.0);
    }
    out.uv = uv;
    let l = normalize(p - light.pos.xyz);
    let s = dot(n, -l);
    out.c = saturate(light.color * s * fog_factor(p));
    if light.kind.x > 0.5 {
        let sigma = light.xform * vec4(p, 1.0);
        out.t1 = vec3(sigma.x, sigma.y, sigma.w);
        out.t2 = vec2(sigma.z, sigma.z);
    } else {
        let q = (p - light.pos.xyz) * light.pos.w + vec3(0.5);
        out.t1 = vec3(q.x, q.z, 1.0);
        out.t2 = q.xy;
    }
    out.aref = d.flags.z;
    return out;
}

// lmap_*, impl_* (section 6.5): the vertex position and unpacked normal.
@vertex
fn vs_light_lmap(v: LightMapped) -> LightOut {
    let d = level_record(v.id, v.record);
    return light_vertex(d, v.position, unpack_vec(v.normal),
        base_uv(v.tc0, v.tangent, v.binormal));
}

// vert_*, simple_*.
@vertex
fn vs_light_vert(v: VertexLit) -> LightOut {
    let d = level_record(v.id, v.record);
    return light_vertex(d, v.position, unpack_vec(v.normal),
        base_uv(v.tc0, v.tangent, v.binormal));
}

// wmark_*: the wall mark's shifted position.
@vertex
fn vs_light_wmark(v: VertexLit) -> LightOut {
    let d = level_record(v.id, v.record);
    let n = unpack_vec(v.normal);
    return light_vertex(d, wmark_position(v.position, n), n,
        base_uv(v.tc0, v.tangent, v.binormal));
}

// tree_w_* (waving) unless the definition is object LOD (tree_s_*), with
// the normal turned by X's rotation and normalised.
@vertex
fn vs_light_tree(v: TreeVertex) -> LightOut {
    let d = level_record(v.id, v.record);
    let x = d.transform;
    let m = vec4<f32>(v.tc);
    var p = (x * vec4(v.position, 1.0)).xyz;
    if d.flags.y < 0.5 {
        p = tree_wave(p, x, m);
    }
    let n = normalize((x * vec4(unpack_vec(v.normal), 0.0)).xyz);
    return light_vertex(d, p, n, m.xy / 2048.0);
}

// model_def_*: the model as its camera passes draw it, by the object's
// matrix.
@vertex
fn vs_light_model(v: ModelVertex) -> LightOut {
    let d = record(v.draw);
    let m = model_point(d, v);
    return light_vertex(d, m.p, m.n, m.uv);
}

// add_point / add_spot: C = 2 · B.rgb · B.a · a₁.rgb · a₂.rgb · c.rgb,
// A = 2 · B.a · a₁.a · a₂.a · c.a, clamped, then the alpha test.
@fragment
fn fs_light(in: LightOut) -> @location(0) vec4<f32> {
    let b = textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w);
    let a1 = textureSample(t_lmap, s_att, in.t1.xy / in.t1.z);
    let a2 = textureSample(t_att, s_att, in.t2);
    let color = saturate(2.0 * b.rgb * b.a * a1.rgb * a2.rgb * in.c.rgb);
    let alpha = saturate(2.0 * b.a * a1.a * a2.a * in.c.a);
    if alpha * 255.0 <= in.aref {
        discard;
    }
    return vec4(color, alpha);
}

// Element 4 of the library classes and selflight: the static geometry's
// baked light (rgb) and sun visibility (a) drawn into a cell of the
// projected light texture (object-projected-light.md 4.4), with the cell's
// camera as the view-projection; no fog, no sun, no detail, no alpha test.

struct CellOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv2: vec2<f32>,
    @location(2) c: vec4<f32>,
}

// lmap_l: rgb = L_ambient + Lm.rgb + sat(L_hemi_color) · h, a = Lm.a.
@vertex
fn vs_lmap_l(v: LightMapped) -> CellOut {
    var out: CellOut;
    out.clip = frame.view_projection * vec4(v.position, 1.0);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    out.uv2 = vec2<f32>(v.tc1) / 32768.0;
    return out;
}

@fragment
fn fs_lmap_l(in: CellOut) -> @location(0) vec4<f32> {
    let lm = textureSampleBias(t_second, s_linear, in.uv2, frame.forward.w);
    let hm = textureSample(t_hemi, s_clamp, in.uv2);
    let h = (hm.r + hm.g + hm.b) / 3.0;
    return saturate(vec4(frame.ambient.rgb + lm.rgb + saturate(frame.hemi.rgb) * h, lm.a));
}

// vert_l: rgb = sat(color.rgb + L_hemi_color · N.w + L_ambient),
// a = sat(color.a).
@vertex
fn vs_vert_l(v: VertexLit) -> CellOut {
    var out: CellOut;
    out.clip = frame.view_projection * vec4(v.position, 1.0);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    out.c = saturate(vec4(rgb(v.color) + frame.hemi.rgb * v.normal.w + frame.ambient.rgb,
        v.color.w));
    return out;
}

@fragment
fn fs_vert_l(in: CellOut) -> @location(0) vec4<f32> {
    return in.c;
}

// impl_l: rgb = L_ambient + Lm.rgb + sat(L_hemi_color) · B.a, a = Lm.a,
// both at the base coordinate.
@vertex
fn vs_impl_l(v: LightMapped) -> CellOut {
    var out: CellOut;
    out.clip = frame.view_projection * vec4(v.position, 1.0);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    return out;
}

@fragment
fn fs_impl_l(in: CellOut) -> @location(0) vec4<f32> {
    let b = textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w);
    let lm = textureSampleBias(t_second, s_linear, in.uv, frame.forward.w);
    return saturate(vec4(frame.ambient.rgb + lm.rgb + saturate(frame.hemi.rgb) * b.a, lm.a));
}

// simple: the base texel.
@vertex
fn vs_simple_l(v: VertexLit) -> CellOut {
    var out: CellOut;
    out.clip = frame.view_projection * vec4(v.position, 1.0);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    return out;
}

@fragment
fn fs_simple_l(in: CellOut) -> @location(0) vec4<f32> {
    return saturate(textureSampleBias(t_base, s_aniso, in.uv, frame.forward.w));
}
