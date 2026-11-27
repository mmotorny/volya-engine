// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The Direct3D 9 probes' scenes replayed (d3d9probes.rs, tests/d3d9probes/):
// pre-transformed vertices through texture stage 0, the alpha test and fog,
// as our programs do these where they have them (world.wgsl,
// particle.wgsl, billboards.wgsl).

struct Probe {
    // xy: the target's size in pixels.
    frame_size: vec4<f32>,
    // x: the alpha reference, y: the comparison (D3DCMP), z: the test on
    // (1) or off (0), w: the LOD bias.
    alpha: vec4<f32>,
    // Stage 0's colour and alpha: x the operation (D3DTOP), y and z its
    // arguments (D3DTA).
    color_op: vec4<f32>,
    alpha_op: vec4<f32>,
    // x: fog off (0), from the specular alpha (1), linear in the pixel's z
    // (2); y: its start, z: its end.
    fog: vec4<f32>,
    // rgb: the fog colour.
    fog_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> probe: Probe;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var image_sampler: sampler;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) diffuse: vec4<f32>,
    @location(1) specular: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) fog: f32,
}

// D3DCOLOR arrives as bytes (b0, b1, b2, b3) / 255: (r, g, b, a) is
// (b2, b1, b0, b3).
fn color(e: vec4<f32>) -> vec4<f32> {
    return e.zyxw;
}

// A vertex (x, y, z, 1/w) in pixels: Direct3D 9's pixel centres are at
// integer coordinates, WebGL's half a pixel further on.
@vertex
fn vs(
    @location(0) position: vec4<f32>,
    @location(1) diffuse: vec4<f32>,
    @location(2) specular: vec4<f32>,
    @location(3) uv: vec2<f32>,
) -> Out {
    let size = probe.frame_size.xy;
    let ndc = vec2(
        (position.x + 0.5) / size.x * 2.0 - 1.0,
        1.0 - (position.y + 0.5) / size.y * 2.0,
    );
    let w = 1.0 / position.w;
    var out: Out;
    out.clip = vec4(ndc * w, position.z * w, w);
    out.diffuse = color(diffuse);
    out.specular = color(specular);
    out.uv = uv;
    // Vertex fog: the factor at the vertex, interpolated, as our programs'
    // fog. Table fog is per pixel (fs).
    out.fog = 1.0;
    if probe.fog.x == 1.0 {
        out.fog = out.specular.a;
    }
    return out;
}

fn argument(a: f32, in: Out, texel: vec4<f32>) -> vec4<f32> {
    // D3DTA_TEXTURE, D3DTA_SPECULAR; else D3DTA_DIFFUSE or D3DTA_CURRENT,
    // which is the diffuse colour at stage 0.
    if a == 2.0 {
        return texel;
    }
    if a == 4.0 {
        return in.specular;
    }
    return in.diffuse;
}

fn operation(op: vec4<f32>, in: Out, texel: vec4<f32>) -> vec4<f32> {
    let a1 = argument(op.y, in, texel);
    let a2 = argument(op.z, in, texel);
    // D3DTOP_SELECTARG2, D3DTOP_MODULATE; else D3DTOP_SELECTARG1.
    if op.x == 3.0 {
        return a2;
    }
    if op.x == 4.0 {
        return a1 * a2;
    }
    return a1;
}

// Whether `v` (alpha × 255) passes against `r` by D3DCMP `f`; GREATER is
// our programs' test (they discard at a · 255 <= ref).
fn passes(f: f32, v: f32, r: f32) -> bool {
    switch u32(f) {
        case 1u: { return false; }
        case 2u: { return v < r; }
        case 3u: { return v == r; }
        case 4u: { return v <= r; }
        case 5u: { return !(v <= r); }
        case 6u: { return v != r; }
        case 7u: { return v >= r; }
        default: { return true; }
    }
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let texel = textureSampleBias(image, image_sampler, in.uv, probe.alpha.w);
    let c = operation(probe.color_op, in, texel).rgb;
    let a = operation(probe.alpha_op, in, texel).a;
    if probe.alpha.z == 1.0 && !passes(probe.alpha.y, a * 255.0, probe.alpha.x) {
        discard;
    }
    // The pixel result clamped, then fog, as world.wgsl's finish.
    var rgb = saturate(c);
    var fog = in.fog;
    if probe.fog.x == 2.0 {
        // FOGTABLEMODE LINEAR: Direct3D 9's table fog, per pixel on the
        // pixel's z (the window depth); our programs never meet it
        // (static-shading.md 9.3: table fog is never enabled).
        let f = probe.fog;
        fog = clamp((f.z - in.clip.z) / (f.z - f.y), 0.0, 1.0);
    }
    if probe.fog.x != 0.0 {
        rgb = fog * rgb + (1.0 - fog) * probe.fog_color.rgb;
    }
    return vec4(rgb, saturate(a));
}
