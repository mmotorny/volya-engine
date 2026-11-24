// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// 2D textured quads in UI units (1024 × 768, origin top-left, y down),
// stretched per axis to the target.

struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

@vertex
fn vs(@location(0) position: vec2f, @location(1) uv: vec2f, @location(2) color: vec4f) -> Vertex {
    let ndc = vec2f(position.x / 512.0 - 1.0, 1.0 - position.y / 384.0);
    return Vertex(vec4f(ndc, 0.0, 1.0), uv, color);
}

@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

@fragment
fn fs(in: Vertex) -> @location(0) vec4f {
    return textureSample(image, image_sampler, in.uv) * in.color;
}

// A video texture holding Y'CbCr unconverted (docs/behavior/startup.md 6.6,
// the data's `hud\movie` shader): BT.601 limited range to RGB after
// filtering, opaque; the quad's colour is not applied. f32 rather than the
// original's half precision (our choice). Out-of-range results are clamped,
// as an 8-bit target does.
@fragment
fn fs_movie(in: Vertex) -> @location(0) vec4f {
    let s = textureSample(image, image_sampler, in.uv);
    let y = s.r;
    let u = s.g;
    let v = s.b;
    let rgb = vec3f(
        1.16406 * y + 1.59765 * v - 0.86961,
        1.16406 * y - 0.390625 * u - 0.8125 * v + 0.53076,
        1.16406 * y + 2.01562 * u - 1.0786,
    );
    return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), 1.0);
}
