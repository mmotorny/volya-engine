// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The sky box, the cloud dome and the sun sprite (docs/behavior/sky.md
// sections 5–7). Matrices are row-vector convention uploaded row by row.

struct Sky {
    // W_sky · V · P and W_cl · V · P.
    sky_wvp: mat4x4<f32>,
    clouds_wvp: mat4x4<f32>,
    view_projection: mat4x4<f32>,
    // Vertex colour of the sky box: bytes of C_sky and of t, / 255.
    sky_color: vec4<f32>,
    // Colour 0 of the clouds ("directions") and colour 1, / 255.
    clouds_directions: vec4<f32>,
    clouds_color: vec4<f32>,
    // x: T, the global time in seconds.
    time: vec4<f32>,
    // The sun sprite's corners (world) and colour / 255.
    sun_corners: array<vec4<f32>, 4>,
    sun_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> sky: Sky;

// Each pass below is compiled on its own with the part above (the passes
// bind different textures at group 1).

//-- sky
@group(1) @binding(0) var t0: texture_cube<f32>;
@group(1) @binding(1) var t1: texture_cube<f32>;
@group(1) @binding(2) var s_sky: sampler;

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) c: vec3<f32>,
}

@vertex
fn vs_sky(@location(0) p: vec3<f32>, @location(1) c: vec3<f32>) -> SkyOut {
    var out: SkyOut;
    out.clip = sky.sky_wvp * vec4(p, 1.0);
    out.c = c;
    return out;
}

// C = sat(v.rgb · lerp(s0.rgb, s1.rgb, v.a) · 2), alpha 1 (section 5.4).
@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let s0 = textureSample(t0, s_sky, in.c).rgb;
    let s1 = textureSample(t1, s_sky, in.c).rgb;
    let v = sky.sky_color;
    return vec4(saturate(v.rgb * mix(s0, s1, v.a) * 2.0), 1.0);
}

//-- clouds
@group(1) @binding(0) var c0: texture_2d<f32>;
@group(1) @binding(1) var c1: texture_2d<f32>;
@group(1) @binding(2) var s_clouds: sampler;

struct CloudsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv0: vec2<f32>,
    @location(1) uv1: vec2<f32>,
    @location(2) color: vec4<f32>,
}

// Section 6.3.
@vertex
fn vs_clouds(@location(0) p: vec3<f32>) -> CloudsOut {
    var out: CloudsOut;
    out.clip = sky.clouds_wvp * vec4(p, 1.0);
    let k0 = sky.clouds_directions.xy * 2.0 - 1.0;
    let k1 = sky.clouds_directions.wz * 2.0 - 1.0;
    let drift = 0.1 * sky.time.x;
    out.uv0 = p.xz * 0.7 + k0 * drift * 0.1;
    out.uv1 = p.xz * 2.8 + k1 * drift * 0.05;
    out.color = vec4(sky.clouds_color.rgb, sky.clouds_color.a * pow(max(p.y, 0.0), 25.0));
    return out;
}

@fragment
fn fs_clouds(in: CloudsOut) -> @location(0) vec4<f32> {
    let s0 = textureSample(c0, s_clouds, in.uv0).rgb;
    let s1 = textureSample(c1, s_clouds, in.uv1).rgb;
    return vec4(saturate(in.color.rgb * (s0 + s1)), in.color.a);
}

//-- sun
@group(1) @binding(0) var sun_texture: texture_2d<f32>;
@group(1) @binding(1) var s_sun: sampler;

struct SunOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Corners 0 … 3 with texture coordinates (0, 0), (0, 1), (1, 0), (1, 1);
// triangles (0, 1, 2), (3, 2, 1) (section 7.4).
@vertex
fn vs_sun(@builtin(vertex_index) i: u32) -> SunOut {
    let corner = array(0u, 1u, 2u, 3u, 2u, 1u)[i];
    var out: SunOut;
    out.clip = sky.view_projection * vec4(sky.sun_corners[corner].xyz, 1.0);
    out.uv = vec2(f32(corner / 2u), f32(corner % 2u));
    return out;
}

// Texel × vertex colour; alpha test ref 2 (section 7.5).
@fragment
fn fs_sun(in: SunOut) -> @location(0) vec4<f32> {
    let c = textureSample(sun_texture, s_sun, in.uv) * sky.sun_color;
    if c.a * 255.0 <= 2.0 {
        discard;
    }
    return c;
}
