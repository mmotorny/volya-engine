// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's collision world and the player's collision body
//! (docs/formats/level-cform.md): the triangles of `level.cform` with their
//! materials mapped at load (section 3.3), a spatial index over them, and
//! [`body::PlayerBody`], the player's shapes moved against them under the
//! spec's contact rules (sections 4–6).
//!
//! The spatial index is our choice (the spec only says the bounding box
//! sizes the engine's spatial structures): a uniform grid over the
//! horizontal plane whose cells list the triangles whose bounding box
//! overlaps them, stored as one flat list.

pub mod body;
pub mod geometry;

use std::io;

pub use volya_engine_formats::gamemtl::GameMaterial;
pub use volya_engine_formats::level_cform::{Cform, CformTriangle};

use volya_engine_formats::gamemtl::{DYNAMIC, PASS_THROUGH, TRIANGLE_BIT_14, TRIANGLE_BIT_15};
use volya_engine_formats::level_cform::{BIT_14, BIT_15, MATERIAL_MASK};

use self::geometry::Vec3;

/// Grid cell edge, metres (our choice: about three player widths, a few
/// triangles of the first level per cell on average).
pub const CELL_SIZE: f32 = 2.0;
/// Most cells a grid may have; larger levels get larger cells (our choice).
const MAX_CELLS: f32 = (1 << 22) as f32;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Maps every triangle's material id to its material index and sets bits 14
/// and 15 from the material's flags 0x400 and 0x800, keeping the sector
/// (section 3.3). Only materials without the dynamic flag are found; the
/// first of several with one id wins (our choice; ids are unique in the
/// shipped file).
pub fn map_materials(
    triangles: &mut [CformTriangle],
    materials: &[GameMaterial],
) -> io::Result<()> {
    let mut by_id = vec![None; MATERIAL_MASK as usize + 1];
    for (index, m) in materials.iter().enumerate().rev() {
        if !m.has(DYNAMIC) && m.id <= MATERIAL_MASK && index <= MATERIAL_MASK as usize {
            by_id[m.id as usize] = Some((index as u32, m.flags));
        }
    }
    for t in triangles {
        let id = t.attributes & MATERIAL_MASK;
        let (index, flags) =
            by_id[id as usize].ok_or_else(|| invalid(format!("Game material '{id}' not found")))?;
        let mut w = (t.attributes & 0xFFFF_0000) | index;
        if flags & TRIANGLE_BIT_14 != 0 {
            w |= BIT_14;
        }
        if flags & TRIANGLE_BIT_15 != 0 {
            w |= BIT_15;
        }
        t.attributes = w;
    }
    Ok(())
}

/// A uniform grid over x and z.
#[derive(Clone, Debug)]
struct Grid {
    origin: [f32; 2],
    cell: f32,
    size: [usize; 2],
    /// Start of each cell's run in `items`; one more than the cell count.
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl Grid {
    fn cell_range(&self, min: f32, max: f32, axis: usize) -> (usize, usize) {
        let last = self.size[axis] - 1;
        let at =
            |v: f32| (((v - self.origin[axis]) / self.cell).floor().max(0.0) as usize).min(last);
        (at(min), at(max))
    }

    fn build(min: Vec3, max: Vec3, boxes: &[[f32; 4]]) -> Self {
        let extent = [(max[0] - min[0]).max(0.0), (max[2] - min[2]).max(0.0)];
        let mut cell = CELL_SIZE;
        while (extent[0] / cell + 1.0) * (extent[1] / cell + 1.0) > MAX_CELLS {
            cell *= 2.0;
        }
        let size = extent.map(|e| (e / cell) as usize + 1);
        let mut grid = Self {
            origin: [min[0], min[2]],
            cell,
            size,
            starts: vec![0; size[0] * size[1] + 1],
            items: Vec::new(),
        };
        let ranges: Vec<_> = boxes
            .iter()
            .map(|b| {
                (
                    grid.cell_range(b[0], b[2], 0),
                    grid.cell_range(b[1], b[3], 1),
                )
            })
            .collect();
        for &((x0, x1), (z0, z1)) in &ranges {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    grid.starts[z * size[0] + x + 1] += 1;
                }
            }
        }
        for i in 1..grid.starts.len() {
            grid.starts[i] += grid.starts[i - 1];
        }
        let mut next = grid.starts.clone();
        grid.items = vec![0; *grid.starts.last().unwrap() as usize];
        for (t, &((x0, x1), (z0, z1))) in ranges.iter().enumerate() {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let slot = &mut next[z * size[0] + x];
                    grid.items[*slot as usize] = t as u32;
                    *slot += 1;
                }
            }
        }
        grid
    }
}

/// A level's static collision mesh with mapped materials and its index.
#[derive(Clone, Debug)]
pub struct CollisionWorld {
    pub bbox_min: Vec3,
    pub bbox_max: Vec3,
    vertices: Vec<Vec3>,
    /// Attribute words as mapped at load (section 3.3).
    triangles: Vec<CformTriangle>,
    materials: Vec<GameMaterial>,
    grid: Grid,
}

impl CollisionWorld {
    /// The world of `cform` with the material library `materials`
    /// (`gamemtl.xr` in file order); fails as the game does on a material
    /// id that is missing or dynamic (section 3.3).
    pub fn new(cform: Cform, materials: Vec<GameMaterial>) -> io::Result<Self> {
        let Cform {
            bbox_min,
            bbox_max,
            vertices,
            mut triangles,
        } = cform;
        map_materials(&mut triangles, &materials)?;
        let boxes: Vec<[f32; 4]> = triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.vertices.map(|i| vertices[i as usize]);
                [
                    a[0].min(b[0]).min(c[0]),
                    a[2].min(b[2]).min(c[2]),
                    a[0].max(b[0]).max(c[0]),
                    a[2].max(b[2]).max(c[2]),
                ]
            })
            .collect();
        // Index over the vertices' extent: the stored box equals it in the
        // shipped files (section 7.3), but the index must cover every vertex.
        let mut min = bbox_min;
        let mut max = bbox_max;
        for v in &vertices {
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
            }
        }
        let grid = Grid::build(min, max, &boxes);
        Ok(Self {
            bbox_min,
            bbox_max,
            vertices,
            triangles,
            materials,
            grid,
        })
    }

    pub fn vertices(&self) -> &[Vec3] {
        &self.vertices
    }

    /// The triangles, attribute words mapped (section 3.3): bits 0–13 are
    /// the material's index.
    pub fn triangles(&self) -> &[CformTriangle] {
        &self.triangles
    }

    pub fn materials(&self) -> &[GameMaterial] {
        &self.materials
    }

    /// The corner positions of triangle `i`.
    pub fn corners(&self, i: u32) -> [Vec3; 3] {
        self.triangles[i as usize]
            .vertices
            .map(|v| self.vertices[v as usize])
    }

    /// The material of triangle `i`.
    pub fn material(&self, i: u32) -> &GameMaterial {
        &self.materials[self.triangles[i as usize].material() as usize]
    }

    /// Whether triangle `i` blocks bodies at all (section 3.2: not
    /// pass-through).
    pub fn is_solid(&self, i: u32) -> bool {
        !self.material(i).has(PASS_THROUGH)
    }

    /// Bytes held (vertices, triangles, index), for reporting.
    pub fn memory_bytes(&self) -> usize {
        self.vertices.len() * 12
            + self.triangles.len() * 16
            + (self.grid.starts.len() + self.grid.items.len()) * 4
    }

    /// Appends to `out` the triangles whose bounding box overlaps the box
    /// `min`…`max`, each once, in ascending order.
    pub fn query(&self, min: Vec3, max: Vec3, out: &mut Vec<u32>) {
        let start = out.len();
        let g = &self.grid;
        let (x0, x1) = g.cell_range(min[0], max[0], 0);
        let (z0, z1) = g.cell_range(min[2], max[2], 1);
        for z in z0..=z1 {
            for x in x0..=x1 {
                let c = z * g.size[0] + x;
                for &t in &g.items[g.starts[c] as usize..g.starts[c + 1] as usize] {
                    let [a, b, c] = self.corners(t);
                    let overlaps = (0..3).all(|k| {
                        a[k].min(b[k]).min(c[k]) <= max[k] && a[k].max(b[k]).max(c[k]) >= min[k]
                    });
                    if overlaps {
                        out.push(t);
                    }
                }
            }
        }
        out[start..].sort_unstable();
        let mut w = start;
        for r in start..out.len() {
            if r == start || out[r] != out[w - 1] {
                out[w] = out[r];
                w += 1;
            }
        }
        out.truncate(w);
    }

    /// The highest point at or below `top` where the vertical line through
    /// `(x, z)` meets a triangle's front side facing up, with the
    /// triangle; for checks and placement.
    pub fn floor_below(&self, x: f32, z: f32, top: f32, bottom: f32) -> Option<(f32, u32)> {
        let mut found = Vec::new();
        self.query([x, bottom, z], [x, top, z], &mut found);
        let mut best: Option<(f32, u32)> = None;
        for t in found {
            let c = self.corners(t);
            let Some(n) = geometry::front_normal(&c) else {
                continue;
            };
            if n[1] <= 0.0 {
                continue;
            }
            let y = c[0][1] - (n[0] * (x - c[0][0]) + n[2] * (z - c[0][2])) / n[1];
            let on = geometry::closest_point([x, y, z], &c);
            let inside = (on[0] - x).abs() < 1e-4 && (on[2] - z).abs() < 1e-4;
            if inside && y <= top && y >= bottom && best.is_none_or(|(b, _)| y > b) {
                best = Some((y, t));
            }
        }
        best
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    //! Synthetic worlds for the unit tests.

    use super::*;

    /// Material ids of the test library: 0 `default` (index 0), 1 a solid
    /// floor, 2 pass-through, 3 pass-through actor obstacle, 4 dynamic.
    pub fn materials() -> Vec<GameMaterial> {
        let m = |id, name: &str, flags| GameMaterial {
            id,
            name: name.into(),
            flags,
            ..GameMaterial::default()
        };
        vec![
            m(0, "default", 0x6000_0010),
            m(1, "materials\\concrete", 0),
            m(
                2,
                "materials\\bush",
                PASS_THROUGH | TRIANGLE_BIT_14 | TRIANGLE_BIT_15,
            ),
            m(3, "materials\\fake", PASS_THROUGH | 0x1000),
            m(4, "objects\\box", DYNAMIC),
        ]
    }

    /// Builds triangles from quads; each quad `[a, b, c, d]` (front side
    /// by the spec's winding a → b → c) becomes two triangles.
    #[derive(Default)]
    pub struct Builder {
        pub vertices: Vec<Vec3>,
        pub triangles: Vec<CformTriangle>,
    }

    impl Builder {
        pub fn quad(&mut self, q: [Vec3; 4], material: u32) -> &mut Self {
            let base = self.vertices.len() as u32;
            self.vertices.extend(q);
            for v in [[0, 1, 2], [0, 2, 3]] {
                self.triangles.push(CformTriangle {
                    vertices: v.map(|k| base + k),
                    attributes: material,
                });
            }
            self
        }

        /// A horizontal rectangle at height `y` facing up (or down).
        pub fn floor(&mut self, x: [f32; 2], z: [f32; 2], y: f32, up: bool) -> &mut Self {
            let q = [
                [x[0], y, z[0]],
                [x[0], y, z[1]],
                [x[1], y, z[1]],
                [x[1], y, z[0]],
            ];
            self.quad(if up { q } else { [q[3], q[2], q[1], q[0]] }, 1)
        }

        /// A vertical wall in the plane x = `x` facing −x (or +x).
        pub fn wall_x(&mut self, x: f32, y: [f32; 2], z: [f32; 2], minus: bool) -> &mut Self {
            let q = [
                [x, y[0], z[1]],
                [x, y[1], z[1]],
                [x, y[1], z[0]],
                [x, y[0], z[0]],
            ];
            self.quad(if minus { q } else { [q[3], q[2], q[1], q[0]] }, 1)
        }

        pub fn world(&self) -> CollisionWorld {
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            for v in &self.vertices {
                for k in 0..3 {
                    min[k] = min[k].min(v[k]);
                    max[k] = max[k].max(v[k]);
                }
            }
            let cform = Cform {
                bbox_min: min,
                bbox_max: max,
                vertices: self.vertices.clone(),
                triangles: self.triangles.clone(),
            };
            CollisionWorld::new(cform, materials()).unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_world::{Builder, materials};
    use super::*;

    #[test]
    fn maps_ids_to_indices_and_flag_bits() {
        let mut lib = materials();
        // Ids need not follow indices: move `default` to the end.
        lib.rotate_left(1);
        let mut t = [
            CformTriangle {
                vertices: [0, 1, 2],
                attributes: (7 << 16) | BIT_15 | 2,
            },
            CformTriangle {
                vertices: [0, 1, 2],
                attributes: BIT_14 | BIT_15,
            },
        ];
        map_materials(&mut t, &lib).unwrap();
        assert_eq!(t[0].attributes, (7 << 16) | BIT_14 | BIT_15 | 1);
        assert_eq!(t[1].attributes, 4);
        let mut dynamic = [CformTriangle {
            vertices: [0, 1, 2],
            attributes: 4,
        }];
        let e = map_materials(&mut dynamic, &lib).unwrap_err();
        assert_eq!(e.to_string(), "Game material '4' not found");
        let mut missing = [CformTriangle {
            vertices: [0, 1, 2],
            attributes: 9,
        }];
        assert!(map_materials(&mut missing, &lib).is_err());
    }

    #[test]
    fn grid_finds_each_overlapping_triangle_once() {
        let mut b = Builder::default();
        b.floor([-10.0, 10.0], [-10.0, 10.0], 0.0, true);
        b.floor([3.0, 3.5], [3.0, 3.5], 1.0, true);
        b.floor([-9.0, -8.0], [-9.0, -8.0], 5.0, true);
        let w = b.world();
        let mut out = Vec::new();
        w.query([2.0, -0.5, 2.0], [4.0, 2.0, 4.0], &mut out);
        assert_eq!(out, [0, 1, 2, 3]);
        out.clear();
        w.query([2.0, 0.5, 2.0], [4.0, 2.0, 4.0], &mut out);
        assert_eq!(out, [2, 3]);
        out.clear();
        w.query([-9.5, 4.0, -9.5], [-7.0, 6.0, -7.0], &mut out);
        assert_eq!(out, [4, 5]);
        assert_eq!(w.floor_below(3.2, 3.2, 3.0, -1.0).map(|f| f.0), Some(1.0));
        assert_eq!(w.floor_below(0.0, 0.0, 3.0, -1.0).map(|f| f.0), Some(0.0));
        assert!(w.is_solid(0) && w.material(0).id == 1);
    }
}
