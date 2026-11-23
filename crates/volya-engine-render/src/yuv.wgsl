// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Y'CbCr planes of a video frame to RGBA, one fragment per frame pixel
// (the viewport covers the frame). Chroma is taken from the co-sited sample,
// no filtering. BT.601 coefficients with video range (Y' 16–235, Cb/Cr
// 16–240), as the Theora specification defines for its colour spaces.

@group(0) @binding(0) var luma: texture_2d<f32>;
@group(0) @binding(1) var cb_plane: texture_2d<f32>;
@group(0) @binding(2) var cr_plane: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    // One triangle covering the viewport.
    let xy = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4f(xy * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let p = vec2u(position.xy);
    let c = p * textureDimensions(cb_plane) / textureDimensions(luma);
    let y = (textureLoad(luma, p, 0).r * 255.0 - 16.0) * 1.164383;
    let cb = textureLoad(cb_plane, c, 0).r * 255.0 - 128.0;
    let cr = textureLoad(cr_plane, c, 0).r * 255.0 - 128.0;
    let rgb = vec3f(
        y + 1.596027 * cr,
        y - 0.391762 * cb - 0.812968 * cr,
        y + 2.017232 * cb,
    );
    return vec4f(clamp(rgb / 255.0, vec3f(0.0), vec3f(1.0)), 1.0);
}
