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
