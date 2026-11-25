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

use std::cell::RefCell;
use std::io::{self, Read, Seek};

pub use volya_engine_formats::gamemtl::GameMaterial;
pub use volya_engine_formats::level_cform::{Cform, CformReader, CformTriangle};

use volya_engine_formats::gamemtl::{DYNAMIC, PASS_THROUGH, TRIANGLE_BIT_14, TRIANGLE_BIT_15};
use volya_engine_formats::level_cform::{BIT_14, BIT_15, MATERIAL_MASK};

use self::geometry::Vec3;

/// The large arrays of the collision world dropped last, kept for the
/// next one (#357). In the wasm heap, which never shrinks, a level's
/// largest blocks (16 MiB of triangles, 13 MiB of grid entries for
/// `l01_escape`) are freed when it is left, and the next load's earlier
/// stages (the offline simulation's many small allocations) split that
/// space before its collision world is read, so the world needed new
/// memory: up to 13 MiB more for a second New Game (measured in headless
/// Chrome). Kept, they are reused whole when large enough.
#[derive(Default)]
struct Spare {
    vertices: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    material_of: Vec<u8>,
    starts: Vec<u32>,
    items: Vec<u32>,
}

thread_local! {
    static SPARE: RefCell<Spare> = RefCell::default();
}

/// `spare` emptied with room for `n` elements: reused when it has the room,
/// else freed before a new one is made.
fn reuse<T>(spare: &mut Vec<T>, n: usize) -> Vec<T> {
    let mut v = std::mem::take(spare);
    v.clear();
    if v.capacity() < n {
        v = Vec::new();
        v.reserve_exact(n);
    }
    v
}

/// Takes the spare array `pick` with room for `n` elements.
fn spare<T>(pick: impl FnOnce(&mut Spare) -> &mut Vec<T>, n: usize) -> Vec<T> {
    SPARE.with(|s| reuse(pick(&mut s.borrow_mut()), n))
}

/// Grid cell edge, metres (our choice: about three player widths, a few
/// triangles of the first level per cell on average).
pub const CELL_SIZE: f32 = 2.0;
/// Most cells a grid may have; larger levels get larger cells (our choice).
const MAX_CELLS: f32 = (1 << 22) as f32;
/// Most cell entries the grid may hold (256 MB): a malformed file whose
/// triangles span many cells fails the load instead (our choice; the first
/// level needs about 5.5 million).
pub const MAX_GRID_ENTRIES: u64 = 1 << 26;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The load-time lookup of section 3.3: material id → index and flags,
/// among the materials without the dynamic flag. The first of several
/// with one id wins (our choice; ids are unique in the shipped file).
pub struct MaterialMap {
    by_id: Vec<Option<(u32, u32)>>,
}

impl MaterialMap {
    pub fn new(materials: &[GameMaterial]) -> Self {
        let mut by_id = vec![None; MATERIAL_MASK as usize + 1];
        for (index, m) in materials.iter().enumerate().rev() {
            if !m.has(DYNAMIC) && m.id <= MATERIAL_MASK && index <= MATERIAL_MASK as usize {
                by_id[m.id as usize] = Some((index as u32, m.flags));
            }
        }
        Self { by_id }
    }

    /// The loaded copy of attribute word `w`: bits 0–13 the material's
    /// index, bits 14 and 15 from its flags 0x400 and 0x800, the sector kept;
    /// a missing id is the game's fatal error.
    pub fn map(&self, w: u32) -> io::Result<u32> {
        let id = w & MATERIAL_MASK;
        let (index, flags) = self.by_id[id as usize]
            .ok_or_else(|| invalid(format!("Game material '{id}' not found")))?;
        let mut mapped = (w & 0xFFFF_0000) | index;
        if flags & TRIANGLE_BIT_14 != 0 {
            mapped |= BIT_14;
        }
        if flags & TRIANGLE_BIT_15 != 0 {
            mapped |= BIT_15;
        }
        Ok(mapped)
    }
}

/// Maps every triangle's attribute word in place (section 3.3).
pub fn map_materials(
    triangles: &mut [CformTriangle],
    materials: &[GameMaterial],
) -> io::Result<()> {
    let map = MaterialMap::new(materials);
    for t in triangles {
        t.attributes = map.map(t.attributes)?;
    }
    Ok(())
}

/// The material index collision keeps per triangle (one byte: our choice;
/// the shipped library has 57 materials).
fn material_byte(mapped: u32) -> io::Result<u8> {
    u8::try_from(mapped & MATERIAL_MASK).map_err(|_| {
        invalid(format!(
            "material index {} beyond the collision world's 255",
            mapped & MATERIAL_MASK
        ))
    })
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

    /// The cells triangle `t` overlaps: x range, z range.
    fn cells(&self, t: &[Vec3; 3]) -> ((usize, usize), (usize, usize)) {
        let [a, b, c] = *t;
        (
            self.cell_range(a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]), 0),
            self.cell_range(a[2].min(b[2]).min(c[2]), a[2].max(b[2]).max(c[2]), 1),
        )
    }

    /// The grid over `min`…`max` listing each triangle in every cell its
    /// bounding box overlaps. Ranges are computed from the vertices in both
    /// passes rather than stored.
    fn build(min: Vec3, max: Vec3, vertices: &[Vec3], triangles: &[[u32; 3]]) -> io::Result<Self> {
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
            starts: spare(|s| &mut s.starts, size[0] * size[1] + 1),
            items: Vec::new(),
        };
        grid.starts.resize(size[0] * size[1] + 1, 0);
        let corners = |t: &[u32; 3]| t.map(|i| vertices[i as usize]);
        let mut total = 0u64;
        for t in triangles {
            let ((x0, x1), (z0, z1)) = grid.cells(&corners(t));
            total += ((x1 - x0 + 1) * (z1 - z0 + 1)) as u64;
            if total > MAX_GRID_ENTRIES {
                return Err(invalid(format!(
                    "level.cform: the spatial index would need more than {MAX_GRID_ENTRIES} entries"
                )));
            }
            for z in z0..=z1 {
                for x in x0..=x1 {
                    grid.starts[z * size[0] + x + 1] += 1;
                }
            }
        }
        // Within u32: the total is capped below it.
        for i in 1..grid.starts.len() {
            grid.starts[i] += grid.starts[i - 1];
        }
        let mut next = grid.starts.clone();
        grid.items = spare(|s| &mut s.items, total as usize);
        grid.items.resize(total as usize, 0);
        for (i, t) in triangles.iter().enumerate() {
            let ((x0, x1), (z0, z1)) = grid.cells(&corners(t));
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let slot = &mut next[z * size[0] + x];
                    grid.items[*slot as usize] = i as u32;
                    *slot += 1;
                }
            }
        }
        Ok(grid)
    }
}

/// A level's static collision mesh with mapped materials and its index.
/// Collision keeps each triangle's vertex indices and material index only:
/// the sector and bits 14 and 15 are not used by it (section 2.2, 3.3).
#[derive(Clone, Debug)]
pub struct CollisionWorld {
    pub bbox_min: Vec3,
    pub bbox_max: Vec3,
    vertices: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    /// Each triangle's material index (section 3.3).
    material_of: Vec<u8>,
    materials: Vec<GameMaterial>,
    grid: Grid,
}

impl Drop for CollisionWorld {
    fn drop(&mut self) {
        let grid = &mut self.grid;
        SPARE.with(|s| {
            *s.borrow_mut() = Spare {
                vertices: std::mem::take(&mut self.vertices),
                triangles: std::mem::take(&mut self.triangles),
                material_of: std::mem::take(&mut self.material_of),
                starts: std::mem::take(&mut grid.starts),
                items: std::mem::take(&mut grid.items),
            }
        });
    }
}

impl CollisionWorld {
    /// Reads `level.cform` from `reader` with the material library
    /// `materials` (`gamemtl.xr` in file order), mapping each triangle as
    /// it is read; fails as the game does on a material id that is missing
    /// or dynamic (section 3.3).
    pub fn read(reader: impl Read + Seek, materials: Vec<GameMaterial>) -> io::Result<Self> {
        let mut reader = CformReader::new(reader)?;
        let header = *reader.header();
        let mut vertices = spare(|s| &mut s.vertices, header.vertex_count as usize);
        reader.vertices_into(&mut vertices)?;
        let map = MaterialMap::new(&materials);
        let count = header.triangle_count as usize;
        let mut triangles = spare(|s| &mut s.triangles, count);
        let mut material_of = spare(|s| &mut s.material_of, count);
        reader.triangles(|_, t| {
            material_of.push(material_byte(map.map(t.attributes)?)?);
            triangles.push(t.vertices);
            Ok(())
        })?;
        Self::from_parts(
            header.bbox_min,
            header.bbox_max,
            vertices,
            triangles,
            material_of,
            materials,
        )
    }

    /// The world of a parsed `cform` (attribute words as stored).
    pub fn new(cform: Cform, materials: Vec<GameMaterial>) -> io::Result<Self> {
        let map = MaterialMap::new(&materials);
        let material_of = cform
            .triangles
            .iter()
            .map(|t| material_byte(map.map(t.attributes)?))
            .collect::<io::Result<_>>()?;
        let triangles = cform.triangles.iter().map(|t| t.vertices).collect();
        Self::from_parts(
            cform.bbox_min,
            cform.bbox_max,
            cform.vertices,
            triangles,
            material_of,
            materials,
        )
    }

    fn from_parts(
        bbox_min: Vec3,
        bbox_max: Vec3,
        vertices: Vec<Vec3>,
        triangles: Vec<[u32; 3]>,
        material_of: Vec<u8>,
        materials: Vec<GameMaterial>,
    ) -> io::Result<Self> {
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
        let grid = Grid::build(min, max, &vertices, &triangles)?;
        Ok(Self {
            bbox_min,
            bbox_max,
            vertices,
            triangles,
            material_of,
            materials,
            grid,
        })
    }

    pub fn vertices(&self) -> &[Vec3] {
        &self.vertices
    }

    /// Each triangle's vertex indices.
    pub fn triangles(&self) -> &[[u32; 3]] {
        &self.triangles
    }

    pub fn materials(&self) -> &[GameMaterial] {
        &self.materials
    }

    /// The corner positions of triangle `i`.
    pub fn corners(&self, i: u32) -> [Vec3; 3] {
        self.triangles[i as usize].map(|v| self.vertices[v as usize])
    }

    /// The material index of triangle `i` (section 3.3).
    pub fn material_index(&self, i: u32) -> u32 {
        u32::from(self.material_of[i as usize])
    }

    /// The material of triangle `i`.
    pub fn material(&self, i: u32) -> &GameMaterial {
        &self.materials[self.material_of[i as usize] as usize]
    }

    /// Whether triangle `i` blocks bodies at all (section 3.2: not
    /// pass-through).
    pub fn is_solid(&self, i: u32) -> bool {
        !self.material(i).has(PASS_THROUGH)
    }

    /// Bytes held (vertices, triangles, materials, index), for reporting.
    pub fn memory_bytes(&self) -> usize {
        self.vertices.len() * 12
            + self.triangles.len() * 12
            + self.material_of.len()
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

    /// Whether the segment from `origin` along the unit direction `dir` for
    /// `length` metres meets any triangle (either side, any material: our
    /// choice, the spec does not say which sides or materials block the
    /// objects' light rays). The
    /// segment's cells are walked in order from the origin, so a hit near it
    /// ends the walk early.
    pub fn ray_hits(&self, origin: Vec3, dir: Vec3, length: f32) -> bool {
        self.ray_any(origin, dir, length, |t| {
            geometry::segment_hits_triangle(origin, dir, length, t)
        })
    }

    /// Whether `hit` holds for any triangle (its corners) in the grid cells
    /// the segment from `origin` along the unit direction `dir` for `length`
    /// metres passes, walked in order from the origin and stopping at the
    /// first. A triangle may be offered more than once.
    pub fn ray_any(
        &self,
        origin: Vec3,
        dir: Vec3,
        length: f32,
        mut hit: impl FnMut(&[Vec3; 3]) -> bool,
    ) -> bool {
        let g = &self.grid;
        let start = [origin[0], origin[2]];
        let d = [dir[0], dir[2]];
        // Clip to the grid's extent in x and z.
        let (mut t0, mut t1) = (0.0f32, length);
        for axis in 0..2 {
            let lo = g.origin[axis];
            let hi = lo + g.cell * g.size[axis] as f32;
            if d[axis].abs() < 1e-12 {
                if start[axis] < lo || start[axis] > hi {
                    return false;
                }
                continue;
            }
            let (a, b) = ((lo - start[axis]) / d[axis], (hi - start[axis]) / d[axis]);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
        if t0 > t1 {
            return false;
        }
        let cell_of = |t: f32, axis: usize| {
            let v = start[axis] + d[axis] * t;
            (((v - g.origin[axis]) / g.cell).floor().max(0.0) as usize).min(g.size[axis] - 1)
        };
        let mut cell = [cell_of(t0, 0), cell_of(t0, 1)];
        let step: [isize; 2] = std::array::from_fn(|a| if d[a] > 0.0 { 1 } else { -1 });
        // Ray parameter of the next cell boundary on each axis, and the
        // parameter step between boundaries.
        let mut next = [f32::INFINITY; 2];
        let mut delta = [f32::INFINITY; 2];
        for a in 0..2 {
            if d[a].abs() >= 1e-12 {
                let edge =
                    g.origin[a] + g.cell * (cell[a] as f32 + if d[a] > 0.0 { 1.0 } else { 0.0 });
                next[a] = (edge - start[a]) / d[a];
                delta[a] = g.cell / d[a].abs();
            }
        }
        loop {
            let c = cell[1] * g.size[0] + cell[0];
            for &t in &g.items[g.starts[c] as usize..g.starts[c + 1] as usize] {
                if hit(&self.corners(t)) {
                    return true;
                }
            }
            let a = if next[0] < next[1] { 0 } else { 1 };
            if next[a] > t1 {
                return false;
            }
            let moved = cell[a] as isize + step[a];
            if moved < 0 || moved >= g.size[a] as isize {
                return false;
            }
            cell[a] = moved as usize;
            next[a] += delta[a];
        }
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
    fn caps_the_grid_against_malformed_meshes() {
        // 70 quads each spanning the whole 4 km grid: about 140 × 1 million
        // entries, over the cap.
        let mut b = Builder::default();
        for _ in 0..70 {
            b.floor([0.0, 4096.0], [0.0, 4096.0], 0.0, true);
        }
        let cform = Cform {
            bbox_min: [0.0; 3],
            bbox_max: [4096.0, 0.0, 4096.0],
            vertices: b.vertices.clone(),
            triangles: b.triangles.clone(),
        };
        let e = CollisionWorld::new(cform, materials()).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert!(e.to_string().contains("spatial index"), "{e}");
    }

    #[test]
    fn reads_a_stream_like_a_parse() {
        let mut b = Builder::default();
        b.floor([-1.0, 1.0], [-1.0, 1.0], 0.0, true);
        // The file of section 2.1.
        let mut bytes = Vec::new();
        for w in [4u32, 4, 2] {
            bytes.extend(w.to_le_bytes());
        }
        for f in [-1.0f32, 0.0, -1.0, 1.0, 0.0, 1.0] {
            bytes.extend(f.to_le_bytes());
        }
        for v in &b.vertices {
            bytes.extend(v.iter().flat_map(|f| f.to_le_bytes()));
        }
        for t in &b.triangles {
            for w in [t.vertices[0], t.vertices[1], t.vertices[2], t.attributes] {
                bytes.extend(w.to_le_bytes());
            }
        }
        let w = CollisionWorld::read(std::io::Cursor::new(&bytes), materials()).unwrap();
        assert_eq!(w.triangles(), [[0, 1, 2], [0, 2, 3]]);
        assert_eq!((w.material_index(0), w.material(1).id), (1, 1));
        assert_eq!(
            w.memory_bytes() - (w.grid.starts.len() + w.grid.items.len()) * 4,
            4 * 12 + 2 * 13
        );
        // Read again once dropped: the same arrays, the same world (#357).
        let (triangles, items) = (w.triangles().as_ptr(), w.grid.items.as_ptr());
        drop(w);
        let again = CollisionWorld::read(std::io::Cursor::new(&bytes), materials()).unwrap();
        assert_eq!(again.triangles(), [[0, 1, 2], [0, 2, 3]]);
        assert_eq!(
            (again.triangles().as_ptr(), again.grid.items.as_ptr()),
            (triangles, items)
        );
        let mut out = Vec::new();
        again.query([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0], &mut out);
        assert_eq!(out, [0, 1]);
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
