// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The distortion's composition (docs/behavior/particle-drawing.md
// section 6.3): the scene looked up at t + o, o =
// (D.r − 0.5, D.g − 0.5) · 0.05, D the distortion target's texel at t.
@group(0) @binding(0) var t_scene: texture_2d<f32>;
@group(0) @binding(1) var t_target: texture_2d<f32>;
@group(0) @binding(2) var s_clamp: sampler;

struct Screen {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// A triangle covering the screen.
@vertex
fn vs_screen(@builtin(vertex_index) i: u32) -> Screen {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var out: Screen;
    out.clip = vec4(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2(x, y);
    return out;
}

@fragment
fn fs_compose(in: Screen) -> @location(0) vec4<f32> {
    let d = textureSample(t_target, s_clamp, in.uv);
    let o = (d.rg - vec2(0.5)) * 0.05;
    return vec4(textureSample(t_scene, s_clamp, in.uv + o).rgb, 1.0);
}
