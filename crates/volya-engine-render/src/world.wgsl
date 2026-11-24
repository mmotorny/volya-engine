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
    // The camera's forward direction (wall marks).
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
@group(0) @binding(1) var<storage, read> draws: array<Draw>;

@group(1) @binding(0) var t_base: texture_2d<f32>;
@group(1) @binding(1) var t_second: texture_2d<f32>;
@group(1) @binding(2) var t_hemi: texture_2d<f32>;
@group(1) @binding(3) var t_detail: texture_2d<f32>;
// s_base, s_detail: wrap, anisotropic; s_lmap: wrap, linear;
// s_hemi: clamp, linear, no mip (section 4.4).
@group(1) @binding(4) var s_aniso: sampler;
@group(1) @binding(5) var s_linear: sampler;
@group(1) @binding(6) var s_clamp: sampler;

// Element 0 (near) when |E − C| − r < 44 m (section 5).
const NEAR_DISTANCE: f32 = 44.0;
// dt_params.w (section 4.3).
const DETAIL_FADE: f32 = 0.02;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // Light-map coordinate (light-mapped) or detail coordinate.
    @location(1) uv2: vec2<f32>,
    @location(2) detail_uv: vec2<f32>,
    @location(3) c0: vec4<f32>,
    @location(4) c1: vec4<f32>,
    @location(5) fog: f32,
    // x: alpha ref, y: fog on, z: detail used.
    @location(6) @interpolate(flat) pass_flags: vec3<f32>,
}

struct LightMapped {
    @builtin(instance_index) draw: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) binormal: vec4<f32>,
    @location(4) tc0: vec2<i32>,
    @location(5) tc1: vec2<i32>,
}

struct VertexLit {
    @builtin(instance_index) draw: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) binormal: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) tc0: vec2<i32>,
}

struct TreeVertex {
    @builtin(instance_index) draw: u32,
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
    let d = draws[v.draw];
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
    let dt = textureSample(t_detail, s_aniso, in.detail_uv).rgb;
    return dt * in.c0.a + in.c1.a;
}

@fragment
fn fs_lmap(in: Out) -> @location(0) vec4<f32> {
    let b = textureSample(t_base, s_aniso, in.uv);
    let lm = textureSample(t_second, s_linear, in.uv2);
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
    let d = draws[v.draw];
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
    let b = textureSample(t_base, s_aniso, in.uv);
    let lm = textureSample(t_second, s_linear, in.uv2);
    let dt = textureSample(t_detail, s_aniso, in.detail_uv).rgb;
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
    let d = draws[v.draw];
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
    let b = textureSample(t_base, s_aniso, in.uv);
    let detail = detail_factor(in);
    var c = 2.0 * b.rgb * in.c0.rgb;
    if in.pass_flags.z > 0.5 {
        c = 2.0 * c * detail;
    }
    return finish(c, b.a, in);
}

// tree_w / tree_s (with _dt), section 7.3.
@vertex
fn vs_tree(v: TreeVertex) -> Out {
    let d = draws[v.draw];
    let x = d.transform;
    let detail = uses_detail(d);
    let m = vec4<f32>(v.tc);
    var p = (x * vec4(v.position, 1.0)).xyz;
    // Element 0 of a tree without object LOD waves; everything else stays.
    if near(d) && d.flags.y < 0.5 {
        let frac = m.z / 2048.0;
        let h = p.y - x[3].y;
        let phi = frame.wave.w + dot(p, frame.wave.xyz);
        let s = 2.0 * sqrt(2.0) * fract(phi) - sqrt(2.0);
        let c = s * s - 1.0;
        p.x += frame.wind.x * h * c * frac;
        p.z += frame.wind.z * h * c * frac;
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

// wmark (section 8.3).
@vertex
fn vs_wmark(v: VertexLit) -> Out {
    let d = draws[v.draw];
    let n = unpack_vec(v.normal);
    let e = frame.eye.xyz;
    var p = v.position + 0.007 * n;
    let s = mix(0.003, 0.011, length(e - v.position));
    p -= normalize(frame.forward.xyz + normalize(p - e)) * s;
    var out = start(d, p, false);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    out.c0 = vec4(vertex_light(v, n), 0.0);
    return out;
}

// simple: self-lit (section 8.1).
@vertex
fn vs_simple(v: VertexLit) -> Out {
    let d = draws[v.draw];
    var out = start(d, v.position, false);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    return out;
}

@fragment
fn fs_simple(in: Out) -> @location(0) vec4<f32> {
    let b = textureSample(t_base, s_aniso, in.uv);
    return finish(b.rgb, b.a, in);
}

// base_lplanes (section 8.2).
@vertex
fn vs_lplanes(v: VertexLit) -> Out {
    let d = draws[v.draw];
    var out = start(d, v.position, false);
    out.uv = base_uv(v.tc0, v.tangent, v.binormal);
    let pv = normalize((frame.view * vec4(v.position, 1.0)).xyz);
    let nv = normalize((frame.view * vec4(unpack_vec(v.normal), 0.0)).xyz);
    out.c0 = vec4(0.0, 0.0, 0.0, abs(dot(pv, nv)));
    return out;
}

@fragment
fn fs_lplanes(in: Out) -> @location(0) vec4<f32> {
    let b = textureSample(t_base, s_aniso, in.uv);
    return finish(b.rgb, b.a * in.c0.a, in);
}
