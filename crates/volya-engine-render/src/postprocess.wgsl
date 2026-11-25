// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The presentation quad (docs/behavior/frame-presentation.md 3.2 – 3.4): the
// off-screen target reduced to the screen through the programs `postprocess`
// and `postprocess_d`.

struct Post {
    // Per corner (left top, right top, left bottom, right bottom): its
    // Direct3D 9 screen position and tc0, then tc1 and tc2.
    corners: array<vec4<f32>, 8>,
    // Colour 0 and 1 (bytes / 255), c_brightness.
    c0: vec4<f32>,
    c1: vec4<f32>,
    add: vec4<f32>,
    // The screen's width and height in pixels.
    screen: vec4<f32>,
}

@group(0) @binding(0) var<uniform> post: Post;
@group(0) @binding(1) var t_scene: texture_2d<f32>;
@group(0) @binding(2) var s_scene: sampler;
@group(0) @binding(3) var t_noise: texture_2d<f32>;
@group(0) @binding(4) var s_noise: sampler;
@group(0) @binding(5) var t_distort: texture_2d<f32>;

struct Quad {
    @builtin(position) clip: vec4<f32>,
    @location(0) tc0: vec2<f32>,
    @location(1) tc1: vec2<f32>,
    @location(2) tc2: vec2<f32>,
}

// A strip of the four corners. Direct3D 9 puts pixel (i, j)'s centre at
// (i, j), the target here at (i + 0.5, j + 0.5): positions move by half a
// pixel, so every pixel samples where it does there.
@vertex
fn vs_quad(@builtin(vertex_index) i: u32) -> Quad {
    let a = post.corners[i * 2u];
    let b = post.corners[i * 2u + 1u];
    let p = (a.xy + vec2(0.5)) / post.screen.xy;
    var out: Quad;
    out.clip = vec4(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
    out.tc0 = a.zw;
    out.tc1 = b.xy;
    out.tc2 = b.zw;
    return out;
}

// 3.4: the two scene look-ups averaged, gray, noise, base and add.
fn present(t0: vec3<f32>, t1: vec3<f32>, tc2: vec2<f32>) -> vec4<f32> {
    let n = textureSampleLevel(t_noise, s_noise, tc2, 0.0).rgb;
    var image = (t0 + t1) * 0.5;
    let g = dot(image, post.c1.rgb);
    image = vec3(g) + (image - vec3(g)) * post.c1.a;
    image = image + (image * n * 2.0 - image) * (1.0 - post.c0.a);
    return vec4(clamp((image * post.c0.rgb + post.add.rgb) * 2.0, vec3(0.0), vec3(1.0)), 1.0);
}

@fragment
fn fs_postprocess(in: Quad) -> @location(0) vec4<f32> {
    let t0 = textureSampleLevel(t_scene, s_scene, in.tc0, 0.0).rgb;
    let t1 = textureSampleLevel(t_scene, s_scene, in.tc1, 0.0).rgb;
    return present(t0, t1, in.tc2);
}

// postprocess_d: both look-ups moved by the distortion offset
// (particle-drawing.md 6.3), D sampled at tc0.
@fragment
fn fs_postprocess_d(in: Quad) -> @location(0) vec4<f32> {
    let d = textureSampleLevel(t_distort, s_scene, in.tc0, 0.0);
    let o = (d.rg - vec2(0.5)) * 0.05;
    let t0 = textureSampleLevel(t_scene, s_scene, in.tc0 + o, 0.0).rgb;
    let t1 = textureSampleLevel(t_scene, s_scene, in.tc1 + o, 0.0).rgb;
    return present(t0, t1, in.tc2);
}
