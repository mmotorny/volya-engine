// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Particle sprites (docs/behavior/particle-drawing.md section 5.4).
// Matrices are row-vector convention uploaded row by row, so `M * v` here
// is `v · M` of the specs.

struct Frame {
    view_projection: mat4x4<f32>,
    fog_plane: vec4<f32>,
    // rgb: the fog unit's colour.
    fog_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var t_base: texture_2d<f32>;
@group(1) @binding(1) var s_base: sampler;
@group(1) @binding(2) var t_distort: texture_2d<f32>;
@group(1) @binding(3) var s_distort: sampler;

struct Vertex {
    @location(0) position: vec3<f32>,
    // (R, G, B, A) / 255.
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) fog: f32,
}

// particle.vs: the world position by the view-projection, the colour and
// texture coordinate unchanged, the fog factor (static-shading.md 9.3).
@vertex
fn vs_particle(v: Vertex) -> Out {
    var out: Out;
    out.clip = frame.view_projection * vec4(v.position, 1.0);
    out.color = v.color;
    out.uv = v.uv;
    out.fog = clamp(dot(vec4(v.position, 1.0), frame.fog_plane), 0.0, 1.0);
    return out;
}

// particle.ps: colour = c.rgb · B.rgb, alpha = c.a · B.a.
fn particle(in: Out) -> vec4<f32> {
    let b = textureSample(t_base, s_base, in.uv);
    return saturate(vec4(in.color.rgb * b.rgb, in.color.a * b.a));
}

// `SET`: alpha test A · 255 > 200, fog on (section 5.2).
@fragment
fn fs_set(in: Out) -> @location(0) vec4<f32> {
    let c = particle(in);
    if c.a * 255.0 <= 200.0 {
        discard;
    }
    let color = in.fog * c.rgb + (1.0 - in.fog) * frame.fog_color.rgb;
    return vec4(color, c.a);
}

// The other blendings of class `PARTICLE`: alpha test A · 255 > 0, no fog.
@fragment
fn fs_blended(in: Out) -> @location(0) vec4<f32> {
    let c = particle(in);
    if c.a * 255.0 <= 0.0 {
        discard;
    }
    return c;
}

// The scripted shaders' normal elements: no alpha test, no fog (5.3).
@fragment
fn fs_plain(in: Out) -> @location(0) vec4<f32> {
    return particle(in);
}

// particle_distort.ps: colour = D.rgb, alpha = D.a · ((c.r + c.g + c.b) ·
// 0.33).
@fragment
fn fs_distort(in: Out) -> @location(0) vec4<f32> {
    let d = textureSample(t_distort, s_distort, in.uv);
    let c = saturate(in.color);
    return saturate(vec4(d.rgb, d.a * ((c.r + c.g + c.b) * 0.33)));
}
