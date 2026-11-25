// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Sound occlusion (docs/behavior/sound-occlusion.md): the file
//! `level.som` and its mesh (section 2), when the meshes are present
//! (section 3), and the occlusion target *o\** along a jittered segment
//! from the listener to an emitter (section 4), with the loudness factor
//! for AI hearers (section 5).
//!
//! The level mesh is the level's collision world, shared with the level.
//! The remembered triangle of section 4.4 is an optimisation the spec lets
//! an implementation omit; it is omitted, so section 4.4's value holds
//! exactly.

use std::io;
use std::sync::Arc;

use volya_engine_formats::chunks::Chunks;

use super::random::{MAX, SharedRandom};
use super::system::Vec3;
use crate::collision::CollisionWorld;
use crate::config::{Config, value};

/// The file's name in the level's directory (section 3).
pub const FILE: &str = "level.som";
/// The size of a polygon record (2.1).
const RECORD: usize = 44;
/// Corners closer than this on every axis merge (2.2).
const MERGE: f32 = 0.01;
/// The smallest determinant a crossing needs (4.3).
const DETERMINANT: f32 = 1e-5;
/// The jitter's radius (4.2).
const JITTER: f32 = 0.2;
/// The occlusion scale's limits (section 3).
const SCALE_MIN: f32 = 0.1;
const SCALE_MAX: f32 = 0.5;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// A polygon record of `level.som` (2.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Polygon {
    pub corners: [Vec3; 3],
    pub two_sided: bool,
    /// The transmission value *t*.
    pub transmission: f32,
}

/// The polygons of `level.som` (2.1): chunk 0 must exist; chunk 1 holds the
/// records.
pub fn parse(bytes: &[u8]) -> io::Result<Vec<Polygon>> {
    let chunks = Chunks::parse(bytes)?;
    chunks.require(0, "level.som version")?;
    // TODO(docs/behavior/sound-occlusion.md section 8): a missing chunk 1,
    // one not a multiple of 44 bytes or a short chunk 0 is unestablished;
    // we fail on the first two (no shipped file has any).
    let data = chunks.require(1, "level.som polygons")?.content()?;
    if data.len() % RECORD != 0 {
        return Err(invalid(format!(
            "level.som: polygon chunk of {} bytes, not a multiple of {RECORD}",
            data.len()
        )));
    }
    Ok(data
        .as_chunks::<RECORD>()
        .0
        .iter()
        .map(|r| {
            let f = |k: usize| f32::from_le_bytes(r[4 * k..4 * k + 4].try_into().unwrap());
            let v = |k: usize| [f(3 * k), f(3 * k + 1), f(3 * k + 2)];
            Polygon {
                corners: [v(0), v(1), v(2)],
                two_sided: u32::from_le_bytes(r[36..40].try_into().unwrap()) != 0,
                transmission: f(10),
            }
        })
        .collect())
}

/// The sound occlusion mesh (2.2): merged vertices and faces with their
/// transmission values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundOcclusionMesh {
    pub vertices: Vec<Vec3>,
    /// Vertex indices and *t*, in face order.
    pub faces: Vec<([u32; 3], f32)>,
}

impl SoundOcclusionMesh {
    /// The mesh of `polygons` (2.1, 2.2): each polygon's face, and for a
    /// two-sided one also the reversed face; corners merged into the first
    /// vertex within 0.01 on every axis. At least 4 vertices and 2 faces,
    /// else fatal.
    pub fn build(polygons: &[Polygon]) -> io::Result<Self> {
        let mut mesh = Self::default();
        for p in polygons {
            let [a, b, c] = p.corners;
            mesh.add([a, b, c], p.transmission);
            if p.two_sided {
                mesh.add([c, b, a], p.transmission);
            }
        }
        if mesh.vertices.len() < 4 || mesh.faces.len() < 2 {
            return Err(invalid(format!(
                "level.som: {} vertices and {} faces; at least 4 and 2 are needed",
                mesh.vertices.len(),
                mesh.faces.len()
            )));
        }
        Ok(mesh)
    }

    fn add(&mut self, corners: [Vec3; 3], t: f32) {
        let face = corners.map(|c| self.vertex(c));
        self.faces.push((face, t));
    }

    fn vertex(&mut self, c: Vec3) -> u32 {
        let found = self
            .vertices
            .iter()
            .position(|v| (0..3).all(|k| (v[k] - c[k]).abs() < MERGE));
        match found {
            Some(i) => i as u32,
            None => {
                self.vertices.push(c);
                self.vertices.len() as u32 - 1
            }
        }
    }

    fn corners(&self, face: [u32; 3]) -> [Vec3; 3] {
        face.map(|v| self.vertices[v as usize])
    }

    /// The product of *t* over the faces the segment crosses with the
    /// one-sided test (4.3, 4.4).
    pub fn transmission(&self, from: Vec3, dir: Vec3, length: f32) -> f32 {
        self.faces
            .iter()
            .filter(|(f, _)| crosses(from, dir, length, &self.corners(*f), false))
            .map(|(_, t)| t)
            .product()
    }
}

/// Whether the segment from `from` along the unit `dir` for `length`
/// crosses the face (4.3): Möller–Trumbore with *g* = *e*₁ · (*d* × *e*₂),
/// edges included, 0 < *s* ≤ `length`; two-sided |*g*| ≥ 10⁻⁵, one-sided
/// *g* ≥ 10⁻⁵.
pub fn crosses(from: Vec3, dir: Vec3, length: f32, face: &[Vec3; 3], two_sided: bool) -> bool {
    let e1 = sub(face[1], face[0]);
    let e2 = sub(face[2], face[0]);
    let p = cross(dir, e2);
    let g = dot(e1, p);
    let passes = if two_sided {
        g.abs() >= DETERMINANT
    } else {
        g >= DETERMINANT
    };
    if !passes {
        return false;
    }
    let inv = 1.0 / g;
    let to = sub(from, face[0]);
    let u = dot(to, p) * inv;
    if u < 0.0 {
        return false;
    }
    let q = cross(to, e1);
    let w = dot(dir, q) * inv;
    if w < 0.0 || u + w > 1.0 {
        return false;
    }
    let s = dot(e2, q) * inv;
    s > 0.0 && s <= length
}

/// The jitter of draws `r1`, `r2` (4.2): 0.2 · (cos φ sin θ, sin φ sin θ,
/// cos θ), θ = *r*₁ π / 32,767, φ = *r*₂ 2π / 32,767.
pub fn jitter(r1: u32, r2: u32) -> Vec3 {
    let theta = r1 as f32 * std::f32::consts::PI / MAX as f32;
    let phi = r2 as f32 * 2.0 * std::f32::consts::PI / MAX as f32;
    let cos_t = theta.cos();
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    [
        JITTER * phi.cos() * sin_t,
        JITTER * phi.sin() * sin_t,
        JITTER * cos_t,
    ]
}

/// The occlusion scale *k* (section 3): `[sound] occlusion_scale` of
/// `config\system.ltx`, limited to 0.1 … 0.5; a missing key is fatal.
pub fn scale(system: &Config) -> io::Result<f32> {
    let text = system
        .get("sound", "occlusion_scale")
        .flatten()
        .ok_or_else(|| invalid("[sound] has no occlusion_scale"))?;
    Ok(value::float(text).clamp(SCALE_MIN, SCALE_MAX))
}

/// What occludes sound (section 3).
#[derive(Clone, Debug)]
pub struct Occlusion {
    /// The level mesh: the level's collision world while it is loaded.
    pub level: Option<Arc<CollisionWorld>>,
    /// The sound occlusion mesh: kept until replaced by a level that has one
    /// (section 3).
    pub sound: Option<SoundOcclusionMesh>,
    /// The occlusion scale *k*.
    pub scale: f32,
}

impl Default for Occlusion {
    /// No meshes; the scale of this edition until the configuration is read.
    fn default() -> Self {
        Self {
            level: None,
            sound: None,
            scale: 0.4,
        }
    }
}

/// The segment from `from` to `to`: its unit direction and length, `None`
/// at length 0.
fn segment(from: Vec3, to: Vec3) -> Option<(Vec3, f32)> {
    let v = sub(to, from);
    let length = dot(v, v).sqrt();
    (length > 0.0).then(|| (v.map(|c| c / length), length))
}

impl Occlusion {
    /// The occlusion target *o\** of an emitter at `position` heard at
    /// `listener` (4.2 – 4.4), drawing the jitter from `random`.
    pub fn target(&self, listener: Vec3, position: Vec3, random: &mut SharedRandom) -> f32 {
        if self.level.is_none() && self.sound.is_none() {
            // Both factors are 1; the draws change nothing audible.
            return 1.0;
        }
        let j = jitter(random.draw(), random.draw());
        let x = [position[0] + j[0], position[1] + j[1], position[2] + j[2]];
        // TODO(docs/behavior/sound-occlusion.md, Gaps found in review): the
        // target for a segment of length 0 is unestablished; we take 1.
        let Some((dir, length)) = segment(listener, x) else {
            return 1.0;
        };
        self.target_along(listener, dir, length)
    }

    /// *o\** along the segment from `from` along unit `dir` for `length`
    /// (4.4).
    pub fn target_along(&self, from: Vec3, dir: Vec3, length: f32) -> f32 {
        let level = match &self.level {
            Some(world)
                if world.ray_any(from, dir, length, |t| crosses(from, dir, length, t, true)) =>
            {
                self.scale
            }
            _ => 1.0,
        };
        let sound = self
            .sound
            .as_ref()
            .map_or(1.0, |m| m.transmission(from, dir, length));
        level * sound
    }

    /// The factor of the loudness given to an AI hearer at `hearer` of a
    /// sound at `position` (section 5): the sound occlusion mesh's product
    /// along a freshly jittered segment, 1 without that mesh.
    pub fn hearer_factor(&self, hearer: Vec3, position: Vec3, random: &mut SharedRandom) -> f32 {
        let Some(mesh) = &self.sound else {
            return 1.0;
        };
        let j = jitter(random.draw(), random.draw());
        let x = [position[0] + j[0], position[1] + j[1], position[2] + j[2]];
        match segment(hearer, x) {
            Some((dir, length)) => mesh.transmission(hearer, dir, length),
            None => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::test_world::Builder;

    #[test]
    fn jitter_of_the_worked_example() {
        // Section 7: r1 = 16,383, r2 = 8,191 → ≈ (0.0000288, 0.2, 0.0000096).
        let j = jitter(16_383, 8_191);
        assert!((j[0] - 0.000_028_8).abs() < 2e-6, "{j:?}");
        assert!((j[1] - 0.2).abs() < 1e-6, "{j:?}");
        assert!((j[2] - 0.000_009_6).abs() < 2e-6, "{j:?}");
        // r1 = 0: (0, 0, 0.2) whatever r2.
        assert_eq!(jitter(0, 12_345), [0.0, 0.0, 0.2]);
    }

    fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3, t: f32, two_sided: bool) -> [Polygon; 2] {
        [
            Polygon {
                corners: [a, b, c],
                two_sided,
                transmission: t,
            },
            Polygon {
                corners: [a, c, d],
                two_sided,
                transmission: t,
            },
        ]
    }

    /// A wall at x = 0 (y, z −5 … 5) whose faces' normal points to +x.
    fn wall(t: f32, two_sided: bool) -> SoundOcclusionMesh {
        let p = quad(
            [0.0, -5.0, -5.0],
            [0.0, 5.0, -5.0],
            [0.0, 5.0, 5.0],
            [0.0, -5.0, 5.0],
            t,
            two_sided,
        );
        SoundOcclusionMesh::build(&p).unwrap()
    }

    fn along(o: &Occlusion, from: Vec3, to: Vec3) -> f32 {
        let (d, l) = segment(from, to).unwrap();
        o.target_along(from, d, l)
    }

    #[test]
    fn one_sided_faces_occlude_only_from_their_normal_side() {
        let mesh = wall(0.6, false);
        // N = e1 × e2 of the first face.
        let [a, b, c] = mesh.corners(mesh.faces[0].0);
        let n = cross(sub(b, a), sub(c, a));
        assert!(n[0] > 0.0, "{n:?}");
        let o = Occlusion {
            sound: Some(mesh),
            ..Occlusion::default()
        };
        // Listener on the normal's side: multiplied (cf. section 7, M3).
        assert_eq!(along(&o, [3.0, 0.1, 0.2], [-3.0, 0.1, 0.2]), 0.6);
        // From behind: not affected.
        assert_eq!(along(&o, [-3.0, 0.1, 0.2], [3.0, 0.1, 0.2]), 1.0);
        // Not reaching the wall.
        assert_eq!(along(&o, [3.0, 0.1, 0.2], [1.0, 0.1, 0.2]), 1.0);
    }

    #[test]
    fn two_sided_polygons_multiply_once_from_either_side() {
        let o = Occlusion {
            sound: Some(wall(0.0, true)),
            ..Occlusion::default()
        };
        // As `l04_darkvalley` polygon 0 (t = 0): 0 from both sides.
        assert_eq!(along(&o, [2.0, 0.3, 0.1], [-2.0, 0.3, 0.1]), 0.0);
        assert_eq!(along(&o, [-2.0, 0.3, 0.1], [2.0, 0.3, 0.1]), 0.0);
        let o = Occlusion {
            sound: Some(wall(0.75, true)),
            ..Occlusion::default()
        };
        assert_eq!(along(&o, [-2.0, 0.3, 0.1], [2.0, 0.3, 0.1]), 0.75);
    }

    #[test]
    fn the_level_mesh_applies_the_scale_once() {
        // Two walls between listener and emitter, either side facing.
        let mut b = Builder::default();
        b.wall_x(1.0, [-5.0, 5.0], [-5.0, 5.0], false);
        b.wall_x(2.0, [-5.0, 5.0], [-5.0, 5.0], true);
        let o = Occlusion {
            level: Some(Arc::new(b.world())),
            sound: Some(wall(0.6, false)),
            scale: 0.4,
        };
        assert_eq!(along(&o, [3.0, 0.3, 0.1], [-3.0, 0.3, 0.1]), 0.4 * 0.6);
        assert_eq!(along(&o, [-3.0, 0.3, 0.1], [3.0, 0.3, 0.1]), 0.4);
        assert_eq!(along(&o, [5.0, 0.3, 0.1], [2.5, 0.3, 0.1]), 1.0);
        // A segment parallel to the walls crosses nothing.
        assert_eq!(along(&o, [1.5, 0.3, -3.0], [1.5, 0.3, 3.0]), 1.0);
        // With no meshes the target is 1.
        let mut r = SharedRandom::default();
        assert_eq!(Occlusion::default().target([0.0; 3], [9.0; 3], &mut r), 1.0);
    }

    #[test]
    fn corners_merge_and_small_meshes_are_fatal() {
        let p = quad(
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            0.5,
            false,
        );
        let mut moved = p;
        moved[1].corners[0] = [0.009, 0.0, -0.009];
        let mesh = SoundOcclusionMesh::build(&moved).unwrap();
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.faces[1].0, [0, 2, 3]);
        assert!(SoundOcclusionMesh::build(&p[..1]).is_err());
    }

    #[test]
    fn parse_reads_records() {
        let mut file = Vec::new();
        let chunk = |file: &mut Vec<u8>, id: u32, data: &[u8]| {
            file.extend(id.to_le_bytes());
            file.extend((data.len() as u32).to_le_bytes());
            file.extend(data);
        };
        chunk(&mut file, 0, &0u32.to_le_bytes());
        let mut rec = Vec::new();
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0] {
            rec.extend(v.to_le_bytes());
        }
        rec.extend(1u32.to_le_bytes());
        rec.extend(0.6f32.to_le_bytes());
        chunk(&mut file, 1, &rec);
        let polygons = parse(&file).unwrap();
        assert_eq!(
            polygons,
            [Polygon {
                corners: [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
                two_sided: true,
                transmission: 0.6,
            }]
        );
        // Without chunk 0: fatal.
        assert!(parse(&file[12..]).is_err());
    }

    #[test]
    fn scale_is_limited() {
        let config = |text: &str| {
            volya_engine_formats::ltx::parse(format!("[sound]\n{text}\n").as_bytes()).unwrap()
        };
        assert_eq!(scale(&config("occlusion_scale = 0.4")).unwrap(), 0.4);
        assert_eq!(scale(&config("occlusion_scale = 0.9")).unwrap(), 0.5);
        assert_eq!(scale(&config("occlusion_scale = 0.01")).unwrap(), 0.1);
        assert!(scale(&config("")).is_err());
    }

    #[test]
    fn occlusion_follows_at_one_per_second() {
        // Section 7 (M5): o from 1 towards 0.24 with dt = 0.016: 0.984 after
        // one update, 0.248 after 47, 0.24 from update 48.
        let mut o = 1.0f32;
        let mut after = Vec::new();
        for _ in 0..48 {
            let gap = 0.24 - o;
            o += gap.clamp(-0.016, 0.016);
            after.push(o);
        }
        assert!((after[0] - 0.984).abs() < 1e-6);
        assert!((after[46] - 0.248).abs() < 1e-4, "{}", after[46]);
        assert!((after[47] - 0.24).abs() < 1e-6);
    }
}
