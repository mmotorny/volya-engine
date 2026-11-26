// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The sky's platform-free part, both renderers' (docs/behavior/sky.md):
//! the sky box, the cloud dome, a frame's values and its constants.

use crate::world::{Mat4, multiply, push_mat, push_vec4};

/// The sky box: positions and texture coordinates (section 5.1).
pub const SKY_BOX: [([f32; 3], [f32; 3]); 12] = [
    ([-1.0, -1.0, -1.0], [-1.0, -1.01, -1.0]),
    ([1.0, -1.0, -1.0], [1.0, -1.01, -1.0]),
    ([-1.0, -1.0, 1.0], [-1.0, -1.01, 1.0]),
    ([1.0, -1.0, 1.0], [1.0, -1.01, 1.0]),
    ([-1.0, 1.0, -1.0], [-1.0, 1.0, -1.0]),
    ([1.0, 1.0, -1.0], [1.0, 1.0, -1.0]),
    ([-1.0, 1.0, 1.0], [-1.0, 1.0, 1.0]),
    ([1.0, 1.0, 1.0], [1.0, 1.0, 1.0]),
    ([-1.0, 0.0, -1.0], [-1.0, -1.0, -1.0]),
    ([1.0, 0.0, -1.0], [1.0, -1.0, -1.0]),
    ([1.0, 0.0, 1.0], [1.0, -1.0, 1.0]),
    ([-1.0, 0.0, 1.0], [-1.0, -1.0, 1.0]),
];

pub const SKY_BOX_TRIANGLES: [[u16; 3]; 20] = [
    [0, 2, 3],
    [3, 1, 0],
    [4, 5, 7],
    [7, 6, 4],
    [0, 1, 9],
    [9, 8, 0],
    [8, 9, 5],
    [5, 4, 8],
    [1, 3, 10],
    [10, 9, 1],
    [9, 10, 7],
    [7, 5, 9],
    [3, 2, 11],
    [11, 10, 3],
    [10, 11, 6],
    [6, 7, 10],
    [2, 0, 8],
    [8, 11, 2],
    [11, 8, 4],
    [4, 6, 11],
];

/// The viewport depth range of the sky box and the cloud dome (5.5).
pub const SKY_DEPTH: (f32, f32) = (0.99999, 1.0);

pub(crate) fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn unit(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    a.map(|c| c / l)
}

/// The cloud dome (section 6.2): the upper half of a twice-subdivided
/// icosahedron on the unit sphere, every triangle facing the centre.
pub fn cloud_dome() -> (Vec<[f32; 3]>, Vec<[u16; 3]>) {
    let (s, c) = (1.0 / 5f32.sqrt(), 2.0 / 5f32.sqrt());
    let tau = std::f32::consts::TAU;
    let mut v: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0], [0.0, -1.0, 0.0]];
    for k in 0..5 {
        let a = tau * k as f32 / 5.0;
        v.push([c * a.cos(), s, c * a.sin()]);
    }
    for k in 0..5 {
        let a = tau * k as f32 / 5.0 + tau / 10.0;
        v.push([c * a.cos(), -s, c * a.sin()]);
    }
    let (u, l) = (|k: usize| 2 + k % 5, |k: usize| 7 + k % 5);
    let mut faces: Vec<[usize; 3]> = Vec::new();
    for k in 0..5 {
        faces.push([0, u(k), u(k + 1)]);
        faces.push([1, l(k + 1), l(k)]);
        faces.push([u(k), l(k), u(k + 1)]);
        faces.push([u(k + 1), l(k), l(k + 1)]);
    }
    for _ in 0..2 {
        let mut midpoints = std::collections::HashMap::new();
        let mut next = Vec::new();
        for [a, b, c] in faces {
            let mut mid = |i: usize, j: usize| {
                *midpoints.entry((i.min(j), i.max(j))).or_insert_with(|| {
                    let m = unit([0, 1, 2].map(|k| (v[i][k] + v[j][k]) / 2.0));
                    v.push(m);
                    v.len() - 1
                })
            };
            let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
            next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        faces = next;
    }
    // The upper half, renumbered; wound to face the centre.
    let mut index = vec![None; v.len()];
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for f in faces {
        if f.iter().any(|&i| v[i][1] < -1e-6) {
            continue;
        }
        let [a, b, c] = f.map(|i| {
            *index[i].get_or_insert_with(|| {
                vertices.push(v[i]);
                vertices.len() - 1
            }) as u16
        });
        let n = cross(
            sub(vertices[b as usize], vertices[a as usize]),
            sub(vertices[c as usize], vertices[a as usize]),
        );
        let outward = dot(n, vertices[a as usize]) > 0.0;
        triangles.push(if outward { [a, c, b] } else { [a, b, c] });
    }
    (vertices, triangles)
}

/// The textures of one hourly section of the weather cycle: its cube map,
/// its cloud texture, and the sun sprite's texture if its lens-flare section
/// draws one.
#[derive(Clone, Debug)]
pub struct SkySection<T> {
    pub cube: T,
    pub clouds: T,
    pub sun: Option<T>,
}

/// The textures of the sky passes: every hourly section of the weather
/// cycle, in time order. A frame binds the textures of a section and of the
/// one after it, wrapping (sky.md 3.3).
#[derive(Clone, Debug)]
pub struct SkyTextures<T> {
    pub sections: Vec<SkySection<T>>,
}

/// The sky's values in one frame (bytes as the specs give them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyFrame {
    /// Section A of [`SkyTextures::sections`]; B is the one after it.
    pub section: usize,
    /// The section whose sun sprite is drawn.
    pub sun_section: usize,
    /// Bytes of C_sky and of the hourly blend weight t (5.3).
    pub color: [u8; 4],
    /// θ_sky, radians.
    pub rotation: f32,
    /// Colour 0 and colour 1 of the cloud dome (6.3); `None` when the
    /// clouds are not drawn (6.1).
    pub clouds: Option<([u8; 4], [u8; 4])>,
    /// The sun sprite's corners 0 … 3 and colour (7.4); `None` when not
    /// drawn.
    pub sun: Option<([[f32; 3]; 4], [u8; 4])>,
}

/// Bytes of the sky constants.
pub(crate) const SKY_BYTES: u64 = 336;

pub(crate) fn bytes_of<T: Copy>(values: &[T], each: impl Fn(&T, &mut Vec<u8>)) -> Vec<u8> {
    let mut out = Vec::new();
    for v in values {
        each(v, &mut out);
    }
    out.resize(out.len().next_multiple_of(4), 0);
    out
}

/// The sky box's vertices (position, texture coordinate) and indices, as
/// uploaded.
pub(crate) fn box_bytes() -> (Vec<u8>, Vec<u8>) {
    (
        bytes_of(&SKY_BOX, |(p, c), out| {
            for v in p.iter().chain(c) {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }),
        bytes_of(&SKY_BOX_TRIANGLES.concat(), |i, out| {
            out.extend_from_slice(&i.to_le_bytes())
        }),
    )
}

/// The cloud dome's vertices and indices, as uploaded, and its index
/// count.
pub(crate) fn dome_bytes() -> (Vec<u8>, Vec<u8>, u32) {
    let (dome, triangles) = cloud_dome();
    let indices = triangles.concat();
    (
        bytes_of(&dome, |p, out| {
            for v in p {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }),
        bytes_of(&indices, |i, out| out.extend_from_slice(&i.to_le_bytes())),
        indices.len() as u32,
    )
}

/// The sky constants of `frame` ([`SKY_BYTES`]) for a camera at `eye`
/// with `view_projection`, at `time` seconds.
pub(crate) fn constants(
    frame: &SkyFrame,
    eye: [f32; 3],
    view_projection: &Mat4,
    time: f32,
) -> Vec<u8> {
    let (s, c) = frame.rotation.sin_cos();
    let world = |h: f32, v: f32| -> Mat4 {
        [
            [h * c, 0.0, -h * s, 0.0],
            [0.0, v, 0.0, 0.0],
            [h * s, 0.0, h * c, 0.0],
            [eye[0], eye[1], eye[2], 1.0],
        ]
    };
    let byte = |b: [u8; 4]| b.map(|v| f32::from(v) / 255.0);
    let mut out = Vec::with_capacity(SKY_BYTES as usize);
    push_mat(&mut out, &multiply(&world(1.0, 1.0), view_projection));
    push_mat(&mut out, &multiply(&world(10.0, 0.4), view_projection));
    push_mat(&mut out, view_projection);
    push_vec4(&mut out, byte(frame.color));
    let (directions, color) = frame.clouds.unwrap_or_default();
    push_vec4(&mut out, byte(directions));
    push_vec4(&mut out, byte(color));
    push_vec4(&mut out, [time, 0.0, 0.0, 0.0]);
    let (corners, sun) = frame.sun.unwrap_or_default();
    for p in corners {
        push_vec4(&mut out, [p[0], p[1], p[2], 1.0]);
    }
    push_vec4(&mut out, byte(sun));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_box_faces_the_centre() {
        let tex_y: f32 = SKY_BOX.iter().map(|(_, c)| c[1]).sum();
        assert!((tex_y + 4.04).abs() < 1e-5);
        assert_eq!(SKY_BOX.iter().map(|(p, _)| p[1]).sum::<f32>(), 0.0);
        for t in SKY_BOX_TRIANGLES {
            let [a, b, c] = t.map(|i| SKY_BOX[i as usize].0);
            let n = cross(sub(b, a), sub(c, a));
            let centroid = [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0);
            assert!(dot(n, centroid) < 0.0, "{t:?} faces outward");
        }
    }

    /// The interpolated texture coordinate where the ray from the centre
    /// along `d` hits the box (attributes interpolate linearly across each
    /// planar triangle, sky.md 5.6).
    fn lookup(d: [f32; 3]) -> [f32; 3] {
        for t in SKY_BOX_TRIANGLES {
            let [(a, ca), (b, cb), (c, cc)] = t.map(|i| SKY_BOX[i as usize]);
            let (e1, e2) = (sub(b, a), sub(c, a));
            let n = cross(e1, e2);
            let denom = dot(n, d);
            if denom.abs() < 1e-9 {
                continue;
            }
            let k = dot(n, a) / denom;
            if k <= 0.0 {
                continue;
            }
            let p = d.map(|x| x * k);
            // Barycentric coordinates of p.
            let w = sub(p, a);
            let (d00, d01, d11) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
            let (d20, d21) = (dot(w, e1), dot(w, e2));
            let det = d00 * d11 - d01 * d01;
            let v = (d11 * d20 - d01 * d21) / det;
            let u = (d00 * d21 - d01 * d20) / det;
            if v >= -1e-6 && u >= -1e-6 && v + u <= 1.0 + 1e-6 {
                return std::array::from_fn(|i| ca[i] + v * (cb[i] - ca[i]) + u * (cc[i] - ca[i]));
            }
        }
        panic!("{d:?} hits nothing")
    }

    #[test]
    fn sky_box_lookup_follows_section_5_6() {
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
        for (d, c) in [
            // Bottom face: the whole −y face, scaled by 1/1.01.
            ([0.0, -1.0, 0.2], [0.0, -1.01, 0.2]),
            ([0.5, -1.0, -0.9], [0.5, -1.01, -0.9]),
            // Lower side quad: −y border texels.
            ([1.0, -0.5, 0.0], [1.0, -1.005, 0.0]),
            // Horizon.
            ([0.0, 0.0, 1.0], [0.0, -1.0, 1.0]),
            // Upper side quads: the side face stretched; its middle at
            // p.y = 0.5.
            ([0.0, 0.5, 1.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.75, 0.25], [-1.0, 0.5, 0.25]),
            // Top face: a direct lookup.
            ([0.0, 2.0, 1.0], [0.0, 1.0, 0.5]),
        ] {
            let got = lookup(d);
            assert!(close(got, c), "{d:?}: {got:?} != {c:?}");
        }
    }

    #[test]
    fn cloud_dome_matches_the_spec() {
        let (v, t) = cloud_dome();
        assert_eq!((v.len(), t.len()), (91, 160));
        assert_eq!(v.iter().filter(|p| p[1].abs() < 1e-6).count(), 20);
        let sum: f32 = v.iter().map(|p| p[1]).sum();
        assert!((sum - 40.1546).abs() < 1e-3, "{sum}");
        for tri in t {
            let [a, b, c] = tri.map(|i| v[i as usize]);
            assert!(dot(cross(sub(b, a), sub(c, a)), a) < 0.0);
        }
    }
}
