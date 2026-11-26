// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The distant billboards of the static world's billboard groups
// (docs/behavior/static-shading.md 8.4, the program `lod`; docs/formats/
// ogf.md 7). Matrices are row-vector convention uploaded row by row, so
// `M * v` here is `v · M` of the specs.

struct Frame {
    view_projection: mat4x4<f32>,
    ambient: vec4<f32>,
    hemi: vec4<f32>,
    sun_color: vec4<f32>,
    // L_sun_dir: the direction the sunlight travels.
    sun_dir: vec4<f32>,
    fog_plane: vec4<f32>,
    // rgb: the fog unit's colour; w: the mip bias in force
    // (texture-quality-filtering.md 5).
    fog_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
// s_base0, s_base1: the atlas (T[0]); s_hemi0, s_hemi1: its `_nm`.
@group(1) @binding(0) var t_base: texture_2d<f32>;
@group(1) @binding(1) var t_hemi: texture_2d<f32>;
@group(1) @binding(2) var s_wrap: sampler;

struct Vertex {
    @location(0) position0: vec3<f32>,
    @location(1) position1: vec3<f32>,
    @location(2) normal0: vec3<f32>,
    @location(3) normal1: vec3<f32>,
    // (sun0, sun1, α, β) / 255.
    @location(4) colour: vec4<f32>,
    @location(5) uv0: vec2<f32>,
    @location(6) uv1: vec2<f32>,
    // The faces' light: (r, g, b, hemisphere) / 255.
    @location(7) rgbh0: vec4<f32>,
    @location(8) rgbh1: vec4<f32>,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv0: vec2<f32>,
    @location(1) uv1: vec2<f32>,
    @location(2) c: vec4<f32>,
    // x: the blend factor f; y: the fog factor.
    @location(3) f_fog: vec2<f32>,
}

@vertex
fn vs_lod(v: Vertex) -> Out {
    let f = v.colour.w;
    let p = mix(v.position0, v.position1, f);
    var n = normalize(mix(v.normal0, v.normal1, f));
    n.y += 1.0;
    n = normalize(n);
    let g = mix(v.rgbh0, v.rgbh1, f) * 1.55;
    let sigma = mix(v.colour.x, v.colour.y, f) * 1.55;
    var out: Out;
    out.clip = frame.view_projection * vec4(p, 1.0);
    out.uv0 = v.uv0;
    out.uv1 = v.uv1;
    let c = g.rgb + frame.hemi.rgb * g.w
        + frame.sun_color.rgb * sigma * (1.0 + 0.7 * dot(n, frame.sun_dir.xyz)) + frame.ambient.rgb;
    out.c = vec4(saturate(c), v.colour.z);
    let fog = clamp(dot(vec4(p, 1.0), frame.fog_plane), 0.0, 1.0);
    out.f_fog = vec2(f, fog);
    return out;
}

// base = lerp(B0, B1, f) · c, h = lerp(H0, H1, f).a,
// C = 2 · base.rgb · (0.5 + 0.5 h), A = base.a; the alpha test, then fog.
fn lod(in: Out, alpha_ref: f32) -> vec4<f32> {
    let bias = frame.fog_color.w;
    let f = in.f_fog.x;
    let b0 = textureSampleBias(t_base, s_wrap, in.uv0, bias);
    let b1 = textureSampleBias(t_base, s_wrap, in.uv1, bias);
    let h0 = textureSampleBias(t_hemi, s_wrap, in.uv0, bias).a;
    let h1 = textureSampleBias(t_hemi, s_wrap, in.uv1, bias).a;
    let base = mix(b0, b1, f) * in.c;
    let h = mix(h0, h1, f);
    let a = base.a;
    if a * 255.0 <= alpha_ref {
        discard;
    }
    let c = saturate(2.0 * base.rgb * (0.5 + 0.5 * h));
    let fog = in.f_fog.y;
    return vec4(fog * c + (1.0 - fog) * frame.fog_color.rgb, a);
}

// Element 4: alpha test 200, no blending, depth written.
@fragment
fn fs_lod_opaque(in: Out) -> @location(0) vec4<f32> {
    return lod(in, 200.0);
}

// Elements 0, 1: alpha test 8, blended, depth not written.
@fragment
fn fs_lod_blended(in: Out) -> @location(0) vec4<f32> {
    return lod(in, 8.0);
}
