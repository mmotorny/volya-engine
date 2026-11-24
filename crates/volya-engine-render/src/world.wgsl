// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The static world, untextured: surfaces coloured by their decoded normal
// (our choice of a debugging look). Vertex attributes arrive as stored in
// level.geom and are decoded here (docs/formats/level-geom.md section 8).
// Matrices are row-vector convention, uploaded row by row, so `M * v` here
// is `v · M` of the specs.

struct Frame {
    view_projection: mat4x4<f32>,
}

struct Draw {
    // Model to world: identity for meshes, the tree's transform for trees.
    transform: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> draws: array<Draw>;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
}

// D3DCOLOR arrives as bytes (b0, b1, b2, b3) / 255; rgb is (b2, b1, b0).
fn unpack_vec(e: vec4<f32>) -> vec3<f32> {
    return 2.0 * e.zyx - 1.0;
}

@vertex
fn vs(
    @builtin(instance_index) draw: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
) -> Out {
    let x = draws[draw].transform;
    let world = x * vec4(position, 1.0);
    let n = (x * vec4(unpack_vec(normal), 0.0)).xyz;
    var out: Out;
    out.clip = frame.view_projection * world;
    out.color = 0.5 + 0.5 * normalize(n);
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return vec4(in.color, 1.0);
}
