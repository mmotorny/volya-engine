// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The billboard groups' distant billboards, platform-free part
//! (docs/formats/ogf.md 7, docs/behavior/static-shading.md 8.4): the quad
//! a drawn billboard emits, a frame's quads, and the program's constants.

use std::sync::Arc;

use volya_engine_formats::ogf::{Billboards, billboard_fade};

use crate::world::{Mat4, WorldLight, fog_plane, push_mat, push_vec4, vec4};

/// One vertex as the program `lod` reads it (ogf.md 7 step 4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BillboardVertex {
    pub position0: [f32; 3],
    pub position1: [f32; 3],
    pub normal0: [f32; 3],
    pub normal1: [f32; 3],
    /// sun0, sun1, α, β.
    pub colour: [u8; 4],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    /// The faces' light colour r, g, b and hemisphere factor.
    pub rgbh0: [u8; 4],
    pub rgbh1: [u8; 4],
}

/// A vertex as uploaded: its fields in order, each group of four bytes
/// as the bits of one float.
pub type PackedVertex = [f32; 19];

/// Bytes of one vertex.
pub(crate) const VERTEX_BYTES: u64 = std::mem::size_of::<PackedVertex>() as u64;

impl BillboardVertex {
    /// The vertex as uploaded.
    pub fn packed(&self) -> PackedVertex {
        let bytes = |b: [u8; 4]| f32::from_bits(u32::from_le_bytes(b));
        let mut out = [0.0; 19];
        let floats = (self.position0.iter())
            .chain(&self.position1)
            .chain(&self.normal0)
            .chain(&self.normal1)
            .copied()
            .chain([bytes(self.colour)])
            .chain(self.uv0)
            .chain(self.uv1)
            .chain([bytes(self.rgbh0), bytes(self.rgbh1)]);
        for (o, f) in out.iter_mut().zip(floats) {
            *o = f;
        }
        out
    }
}
/// Vertices of one billboard: its quad's two triangles.
pub const QUAD_VERTICES: usize = 6;
/// Bytes of the frame constants.
pub(crate) const FRAME_BYTES: u64 = 160;

/// One draw: `count` vertices from `first`, with the textures of `set`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BillboardDraw {
    pub set: u32,
    pub first: u32,
    pub count: u32,
}

/// The billboards of a frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BillboardFrame {
    pub vertices: Arc<Vec<PackedVertex>>,
    /// In drawing order.
    pub draws: Vec<BillboardDraw>,
}

/// A billboard group as drawn (ogf.md 7): its faces, its header sphere
/// and its coverage factor.
#[derive(Clone, Debug, PartialEq)]
pub struct BillboardGroup {
    pub faces: Box<Billboards>,
    pub centre: [f32; 3],
    pub radius: f32,
    pub coverage: f32,
}

/// D3DCOLOR's stored bytes (B, G, R, A) as (R, G, B, A).
fn rgba(stored: [u8; 4]) -> [u8; 4] {
    [stored[2], stored[1], stored[0], stored[3]]
}

impl BillboardGroup {
    /// The group's screen measure for a camera at `eye` (ogf.md 8.2).
    pub fn measure(&self, eye: [f32; 3]) -> f32 {
        let d: [f32; 3] = std::array::from_fn(|i| self.centre[i] - eye[i]);
        self.radius / ((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2] + 1e-5) * self.coverage
    }

    /// The quad drawn for a camera at `eye`, the group's screen measure
    /// `sigma` and the thresholds `t_a`, `t_b` (ogf.md 7, drawn quad), as
    /// its two triangles (q0, q1, q2), (q3, q2, q1).
    pub fn quad(&self, eye: [f32; 3], sigma: f32, t_a: f32, t_b: f32) -> [BillboardVertex; 6] {
        let choice = self.faces.choose_faces(self.centre, eye);
        let alpha = billboard_fade(sigma, t_a, t_b);
        let d: [f32; 3] = std::array::from_fn(|i| self.centre[i] - eye[i]);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let offset = d.map(|c| c / len * (-0.5 * self.radius));
        let (f0, f1) = (&self.faces.faces[choice.f2], &self.faces.faces[choice.f1]);
        let (n0, n1) = (
            self.faces.face_normal(choice.f2),
            self.faces.face_normal(choice.f1),
        );
        let moved = |p: [f32; 3]| std::array::from_fn(|i| p[i] + offset[i]);
        let q: [BillboardVertex; 4] = std::array::from_fn(|q| {
            let k = [3, 0, 2, 1][q];
            let (a, b) = (&f0[k], &f1[k]);
            BillboardVertex {
                position0: moved(a.position),
                position1: moved(b.position),
                normal0: n0,
                normal1: n1,
                colour: [a.sun, b.sun, alpha, choice.beta],
                uv0: a.uv,
                uv1: b.uv,
                rgbh0: rgba(a.colour),
                rgbh1: rgba(b.colour),
            }
        });
        [q[0], q[1], q[2], q[3], q[2], q[1]]
    }
}

/// The frame constants ([`FRAME_BYTES`]): the view-projection, the
/// weather's light, fog plane and colour, and the mip bias in force.
pub(crate) fn frame_constants(
    view_projection: &Mat4,
    light: &WorldLight,
    mip_bias: f32,
) -> Vec<u8> {
    let mut constants = Vec::with_capacity(FRAME_BYTES as usize);
    push_mat(&mut constants, view_projection);
    push_vec4(&mut constants, vec4(light.ambient, 0.0));
    push_vec4(&mut constants, vec4(light.hemi, 0.0));
    push_vec4(&mut constants, vec4(light.sun_color, 0.0));
    push_vec4(&mut constants, vec4(light.sun_dir, 0.0));
    push_vec4(
        &mut constants,
        fog_plane(view_projection, light.fog_near, light.fog_far),
    );
    push_vec4(&mut constants, vec4(light.fog_color, mip_bias));
    constants
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::ogf::BillboardVertex as Stored;

    /// Faces of a box around the origin, face f facing direction f · 45°
    /// about y, each corner telling its face and corner apart.
    fn group() -> BillboardGroup {
        let faces = std::array::from_fn(|f| {
            let a = f as f32 * std::f32::consts::FRAC_PI_4;
            // The face's plane faces (sin a, 0, cos a) away from its
            // selection normal's sign convention: corners wound so that
            // N_f = −normalize(mean corner normal) = (sin a, 0, cos a).
            let (s, c) = a.sin_cos();
            let right = [c, 0.0, -s];
            let corner = |x: f32, y: f32| [right[0] * x, y, right[2] * x];
            let ps = [
                corner(-1.0, 0.0),
                corner(-1.0, 2.0),
                corner(1.0, 2.0),
                corner(1.0, 0.0),
            ];
            std::array::from_fn(|k| Stored {
                position: ps[k],
                uv: [f as f32, k as f32],
                colour: [10 + k as u8, 20, 30, 40 + f as u8],
                sun: 100 + f as u8,
            })
        });
        BillboardGroup {
            faces: Box::new(Billboards { faces }),
            centre: [0.0, 1.0, 0.0],
            radius: 2.0,
            coverage: 1.0,
        }
    }

    #[test]
    fn the_quad_of_a_drawn_billboard() {
        let g = group();
        let eye = [0.0, 1.0, -100.0];
        let choice = g.faces.choose_faces(g.centre, eye);
        let quad = g.quad(eye, 1.0, 2.0, 0.0);
        // Triangles (q0, q1, q2), (q3, q2, q1).
        assert_eq!((quad[4], quad[5]), (quad[2], quad[1]));
        // Vertex q takes face vertex (3, 0, 2, 1)[q], "0" from F2, "1"
        // from F1, moved half the radius towards the camera.
        for (q, k) in [(0, 3), (1, 0), (2, 2), (3, 1)] {
            let v = quad[q];
            let a = &g.faces.faces[choice.f2][k];
            let b = &g.faces.faces[choice.f1][k];
            assert_eq!(v.uv0, a.uv);
            assert_eq!(v.uv1, b.uv);
            assert_eq!(v.position0[2], a.position[2] - 1.0);
            assert_eq!(v.position1[0], b.position[0]);
            assert_eq!(v.colour, [a.sun, b.sun, 127, choice.beta]);
            assert_eq!(v.rgbh0, [30, 20, a.colour[0], a.colour[3]]);
        }
    }

    #[test]
    fn the_vertex_is_76_bytes() {
        // ogf.md 7 step 4's layout size, the bytes in place.
        assert_eq!(VERTEX_BYTES, 76);
        let v = BillboardVertex {
            colour: [1, 2, 3, 4],
            uv0: [5.0, 6.0],
            rgbh1: [7, 8, 9, 10],
            ..Default::default()
        };
        let p = v.packed();
        let bytes: Vec<u8> = p.iter().flat_map(|f| f.to_le_bytes()).collect();
        assert_eq!(bytes[48..52], [1, 2, 3, 4]);
        assert_eq!(p[13], 5.0);
        assert_eq!(bytes[72..76], [7, 8, 9, 10]);
    }
}
