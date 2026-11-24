// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Y'CbCr planes of a video frame into a video texture, one fragment per
// picture pixel (the viewport covers the picture): red = Y', green = Cb,
// blue = Cr, alpha = 1, unconverted (docs/behavior/startup.md 6.6). Texel
// (x, y) takes chroma sample (x >> 1, y >> 1) for subsampled axes: no
// interpolation, no siting adjustment.

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
    // A chroma plane narrower (shorter) than luma is subsampled on that axis.
    let shift = vec2u(textureDimensions(cb_plane) < textureDimensions(luma));
    let c = p >> shift;
    return vec4f(
        textureLoad(luma, p, 0).r,
        textureLoad(cb_plane, c, 0).r,
        textureLoad(cr_plane, c, 0).r,
        1.0,
    );
}
