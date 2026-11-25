// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The display's gamma ramp (docs/behavior/display-gamma.md section 2): every
// channel of the presented picture, as an 8-bit value i, is shown at
// entry[i] / 65535. The frame is drawn over the whole target, scaled into
// `rect` (target pixels: left, top, right, bottom), black around it; the ramp
// acts on the entire screen, the black too.

struct Ramp {
    rect: vec4<f32>,
    // Entry i / 65535 at entries[i / 4][i % 4].
    entries: array<vec4<f32>, 64>,
}

@group(0) @binding(0) var<uniform> ramp: Ramp;
@group(0) @binding(1) var frame: texture_2d<f32>;
@group(0) @binding(2) var frame_sampler: sampler;

// A triangle covering the target.
@vertex
fn vs_screen(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn entry(v: f32) -> f32 {
    let i = u32(round(clamp(v, 0.0, 1.0) * 255.0));
    return ramp.entries[i / 4u][i % 4u];
}

@fragment
fn fs_ramp(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let r = ramp.rect;
    var c = vec3(0.0);
    if all(p.xy >= r.xy) && all(p.xy < r.zw) {
        let uv = (p.xy - r.xy) / (r.zw - r.xy);
        c = textureSampleLevel(frame, frame_sampler, uv, 0.0).rgb;
    }
    return vec4(entry(c.r), entry(c.g), entry(c.b), 1.0);
}
