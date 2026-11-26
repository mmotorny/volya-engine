// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Detail objects (docs/behavior/detail-objects.md 6.3 – 6.5): the programs
// detail_still, detail_wave and detail. Matrices are row-vector convention
// uploaded row by row, so `M * v` here is `v · M` of the specs.

struct Frame {
    view_projection: mat4x4<f32>,
    // The sway constants of 6.5: w and δ of sway 1, then of sway 2.
    wave1: vec4<f32>,
    dir1: vec4<f32>,
    wave2: vec4<f32>,
    dir2: vec4<f32>,
    // The blended weather's L_ambient, L_hemi_color, L_sun_color.
    ambient: vec4<f32>,
    hemi: vec4<f32>,
    sun: vec4<f32>,
    // x: the alpha reference × 255 (−1: no alpha test); y: the mip bias in
    // force (texture-quality-filtering.md 5).
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var t_base: texture_2d<f32>;
@group(1) @binding(1) var s_base: sampler;

struct Vertex {
    @location(0) position: vec3<f32>,
    // u, v and the height fraction φ, each stored in 1/16384 steps.
    @location(1) uv_phi: vec3<f32>,
    // The instance: position, drawing scale σ′; cos θ, sin θ, sun factor,
    // hemisphere factor.
    @location(2) place: vec4<f32>,
    @location(3) turn_light: vec4<f32>,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) colour: vec4<f32>,
}

// P = A · (p, 1): the model point turned about y, scaled, moved (4.2).
fn world_position(v: Vertex) -> vec3<f32> {
    let c = v.turn_light.x;
    let s = v.turn_light.y;
    let p = v.position;
    let turned = vec3(p.x * c + p.z * s, p.y, -p.x * s + p.z * c);
    return v.place.xyz + turned * v.place.w;
}

// c = ambient + hemi · h_f + sun · 0.5 · s_f (6.3).
fn instance_colour(v: Vertex) -> vec3<f32> {
    return frame.ambient.rgb + frame.hemi.rgb * v.turn_light.w + frame.sun.rgb * 0.5 * v.turn_light.z;
}

@vertex
fn vs_still(v: Vertex) -> Out {
    var out: Out;
    out.clip = frame.view_projection * vec4(world_position(v), 1.0);
    out.uv = v.uv_phi.xy;
    out.colour = saturate(vec4(instance_colour(v), 1.0));
    return out;
}

fn wave(v: Vertex, w: vec4<f32>, dir: vec4<f32>) -> Out {
    var p = world_position(v);
    let phi = v.uv_phi.z;
    let g = dot(p, w.xyz) + w.w;
    let a = 2.8284271 * fract(g) - 1.4142136;
    let gamma = a * a - 1.0;
    let h = p.y - v.place.y;
    p.x = p.x + dir.x * h * gamma * phi;
    p.z = p.z + dir.z * h * gamma * phi;
    var out: Out;
    out.clip = frame.view_projection * vec4(p, 1.0);
    out.uv = v.uv_phi.xy;
    let k = 0.9 + 0.25 * max(0.0, gamma) * phi;
    out.colour = saturate(vec4(instance_colour(v), 1.0) * k);
    return out;
}

@vertex
fn vs_wave1(v: Vertex) -> Out {
    return wave(v, frame.wave1, frame.dir1);
}

@vertex
fn vs_wave2(v: Vertex) -> Out {
    return wave(v, frame.wave2, frame.dir2);
}

// detail: t.rgb · C.rgb · 2, alpha t.a, then the alpha test (6.2, 6.4).
@fragment
fn fs_detail(in: Out) -> @location(0) vec4<f32> {
    let t = textureSampleBias(t_base, s_base, in.uv, frame.params.y);
    if frame.params.x >= 0.0 && t.a * 255.0 <= frame.params.x {
        discard;
    }
    return saturate(vec4(t.rgb * in.colour.rgb * 2.0, t.a));
}
