// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The first renderer's occlusion on the CPU: the level's occlusion mesh
//! (docs/formats/level-hom.md), the 64 × 64 grid drawn from it each camera
//! pass (docs/behavior/occlusion-grid.md, "OG") and the box, polygon and
//! screen-rectangle tests against it with their caching
//! (docs/behavior/occlusion-tests.md, "OT").
//!
//! Arithmetic is single precision, as the specs state for the original,
//! with one exception left open by them: the original normalises the
//! frustum planes (so the near-side clipping plane, OG 4.3) in the x87 unit
//! at a precision that is unestablished; here that is single precision too,
//! so the plane may differ from the original's in the last bit.

use crate::camera::{Mat4, Vec3};
use crate::collision::geometry::{cross, dot, length, normalize, sub};
use crate::particles::random::Generator;
use crate::visibility::Frustum;
use volya_engine_formats::level_hom::HomRecord;

/// Cells on a side of the grid (OG 1).
pub const GRID: usize = 64;
/// Cells on a side of the drawing's buffers: the grid and a border of two.
const BUFFER: usize = GRID + 4;
/// An empty cell's encoded depth, 2³⁰ (OG 7.2).
pub const EMPTY: i32 = 1 << 30;
/// 2³⁰ as a real.
const SCALE: f32 = (1u32 << 30) as f32;
/// No owner (OG 1).
const NONE: u32 = u32::MAX;
/// A shape with a point whose z is below this is visible untested (OT 2.2).
const NEAR_LIMIT: f32 = 1e-5;
/// Vertices nearer than this on every axis are merged (level-hom.md 3).
const MERGE: f32 = 0.01;
/// A triangle with a smaller area is reported invalid (level-hom.md 3).
const MIN_AREA: f32 = 0.001;
/// A clipped edge shorter than this on every axis is ignored (OG 4.3).
const SHORT_EDGE: f32 = 1e-7;
const THIRD: f32 = 1.0 / 3.0;

/// The original's conversions (OG conventions): floor and ceil, but −0.0
/// gives floor −1 and ceil 1.
fn floor_i(x: f32) -> i32 {
    if x == 0.0 && x.is_sign_negative() {
        -1
    } else {
        x.floor() as i32
    }
}

fn ceil_i(x: f32) -> i32 {
    if x == 0.0 && x.is_sign_negative() {
        1
    } else {
        x.ceil() as i32
    }
}

/// One triangle of the occlusion mesh after loading (level-hom.md 3).
#[derive(Clone, Debug, PartialEq)]
pub struct Occluder {
    /// Indices into the merged vertices, in record order.
    pub vertices: [u32; 3],
    pub two_sided: bool,
    /// The plane `n·p + d` (level-hom.md 2.1); the front is where positive.
    pub normal: Vec3,
    pub d: f32,
    /// The mean of the three vertices.
    pub centre: Vec3,
    /// The neighbour across edges 0 (v₀ v₁), 1 (v₁ v₂) and 2 (v₂ v₀), if
    /// any.
    pub neighbours: [Option<u32>; 3],
}

/// A level's occlusion mesh (level-hom.md 3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcclusionMesh {
    /// The merged vertices.
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<Occluder>,
}

impl OcclusionMesh {
    /// The mesh the records of `level.hom` make when the level loads:
    /// vertices merged, neighbours linked, planes and centres computed.
    pub fn new(records: &[HomRecord]) -> Self {
        // Step 1: each vertex replaced by the first kept one within 0.01.
        let mut vertices: Vec<Vec3> = Vec::new();
        let mut merged = |v: Vec3| {
            let near = |k: &Vec3| (0..3).all(|a| (k[a] - v[a]).abs() < MERGE);
            match vertices.iter().position(near) {
                Some(i) => i as u32,
                None => {
                    vertices.push(v);
                    vertices.len() as u32 - 1
                }
            }
        };
        let indices: Vec<[u32; 3]> = records
            .iter()
            .map(|r| r.vertices.map(&mut merged))
            .collect();

        // Step 2: edges sorted by their lower index, higher index and
        // triangle (the review's correction); indices compared on their low
        // 16 bits; each adjacent equal pair linked, later links replacing
        // earlier ones.
        let mut edges: Vec<(u16, u16, u32, usize)> = Vec::with_capacity(3 * indices.len());
        for (t, v) in indices.iter().enumerate() {
            for e in 0..3 {
                let (a, b) = (v[e] as u16, v[(e + 1) % 3] as u16);
                edges.push((a.min(b), a.max(b), t as u32, e));
            }
        }
        edges.sort_unstable();
        let mut neighbours = vec![[None; 3]; indices.len()];
        for w in edges.windows(2) {
            let ((a0, b0, t0, e0), (a1, b1, t1, e1)) = (w[0], w[1]);
            if (a0, b0) == (a1, b1) && t0 != t1 {
                neighbours[t0 as usize][e0] = Some(t1);
                neighbours[t1 as usize][e1] = Some(t0);
            }
        }

        // Step 3.
        let mut invalid = 0;
        let triangles = indices
            .iter()
            .zip(records)
            .zip(neighbours)
            .map(|((&i, r), neighbours)| {
                let [v0, v1, v2] = i.map(|k| vertices[k as usize]);
                let normal = normalize(cross(sub(v0, v1), sub(v0, v2))).unwrap_or([0.0; 3]);
                let (a, b, c) = (
                    length(sub(v0, v1)),
                    length(sub(v1, v2)),
                    length(sub(v2, v0)),
                );
                let s = (a + b + c) * 0.5;
                if ((s * (s - a) * (s - b) * (s - c)).max(0.0)).sqrt() < MIN_AREA {
                    invalid += 1;
                }
                Occluder {
                    vertices: i,
                    two_sided: r.two_sided != 0,
                    normal,
                    d: -dot(normal, v0),
                    centre: std::array::from_fn(|k| ((v0[k] + v1[k]) + v2[k]) * THIRD),
                    neighbours,
                }
            })
            .collect();
        if invalid > 0 {
            tracing::warn!("level.hom: {invalid} invalid triangles (area below {MIN_AREA})");
        }
        Self {
            vertices,
            triangles,
        }
    }
}

/// What the drawing and the tests take from the moment they run: the frame
/// number (OG conventions), the view-projection **VP** and eye **E** then,
/// and whether the main menu is shown (OG 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub frame: u32,
    pub view_projection: Mat4,
    pub eye: Vec3,
    pub menu_shown: bool,
}

/// An item's cached result (OT 5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cached {
    /// The next-test frame *N*.
    pub next: u32,
    /// The frame of the last test.
    pub tested: u32,
}

/// The drawing's two buffers (OG 1).
#[derive(Clone, Debug)]
struct Buffers {
    depth: Vec<f32>,
    owner: Vec<u32>,
}

/// A level's occlusion: its mesh, the grid and the state the drawing and
/// the tests keep between frames.
#[derive(Clone, Debug)]
pub struct Occlusion {
    mesh: Option<OcclusionMesh>,
    /// Each triangle's skip frame (OG 3).
    skip: Vec<u32>,
    /// Each triangle's cell writes in the latest drawing (0 when not drawn).
    writes: Vec<u32>,
    /// The switch of OT 4.
    pub on: bool,
    /// The frame the grid was last drawn in.
    drawn: Option<u32>,
    /// The tests' matrix A (OT 1), all zero until first built (believed).
    tests_matrix: Mat4,
    buffers: Buffers,
    grid: Vec<i32>,
    /// The 32, 16 and 8-cell grids (OG 7.3); nothing reads them.
    coarse: [Vec<i32>; 3],
    /// The static visuals' cached results, by visual.
    visuals: Vec<Cached>,
    candidates: Vec<u32>,
    clipped: Vec<Vec3>,
}

impl Occlusion {
    /// The occlusion of a level with `mesh` (none without `level.hom`) and
    /// `visuals` static visuals. Off until the first camera pass (OT 4).
    pub fn new(mesh: Option<OcclusionMesh>, visuals: usize) -> Self {
        let n = mesh.as_ref().map_or(0, |m| m.triangles.len());
        Self {
            mesh,
            skip: vec![0; n],
            writes: vec![0; n],
            on: false,
            drawn: None,
            tests_matrix: [[0.0; 4]; 4],
            buffers: Buffers {
                depth: vec![1.0; BUFFER * BUFFER],
                owner: vec![NONE; BUFFER * BUFFER],
            },
            grid: vec![EMPTY; GRID * GRID],
            coarse: [32, 16, 8].map(|s| vec![EMPTY; s * s]),
            visuals: vec![Cached::default(); visuals],
            candidates: Vec::new(),
            clipped: Vec::new(),
        }
    }

    pub fn mesh(&self) -> Option<&OcclusionMesh> {
        self.mesh.as_ref()
    }

    /// The 64 × 64 grid, row by row from the top (OG 1, 7.2).
    pub fn grid(&self) -> &[i32] {
        &self.grid
    }

    /// The coarser grid of `size` (32, 16 or 8) cells a side (OG 7.3).
    pub fn coarse(&self, size: usize) -> &[i32] {
        &self.coarse[match size {
            32 => 0,
            16 => 1,
            8 => 2,
            _ => panic!("no coarse grid of {size}"),
        }]
    }

    /// Triangle `t`'s skip frame (OG 3).
    pub fn skip_frame(&self, t: usize) -> u32 {
        self.skip[t]
    }

    /// Triangle `t`'s cell writes in the latest drawing (OG 4.3).
    pub fn writes(&self, t: usize) -> u32 {
        self.writes[t]
    }

    /// A static visual's cached result.
    pub fn visual_cache(&self, visual: u32) -> Cached {
        self.visuals[visual as usize]
    }

    /// The start of a camera pass (OG 2, OT 4): occlusion on exactly when
    /// the level has a mesh, then the grid drawn.
    pub fn camera_pass(&mut self, view: &View, random: &mut Generator) {
        self.on = self.mesh.is_some();
        self.draw(view, random);
    }

    /// Draws the grid from `view` (OG 2 – 7), the skip counts' values from
    /// `random`.
    pub fn draw(&mut self, view: &View, random: &mut Generator) {
        let Self {
            mesh: Some(mesh),
            skip,
            writes,
            drawn,
            tests_matrix,
            buffers,
            grid,
            coarse,
            candidates,
            clipped,
            ..
        } = self
        else {
            return;
        };
        let frame = view.frame;
        *drawn = Some(frame);
        buffers.depth.fill(1.0);
        buffers.owner.fill(NONE);
        writes.fill(0);
        let vp = &view.view_projection;

        // Section 3. The candidates: here every triangle not wholly outside
        // one of the frustum's planes (the original's search structure,
        // whose extra candidates are unestablished, is not reproduced).
        let frustum = Frustum::view(vp);
        candidates.clear();
        for (i, t) in mesh.triangles.iter().enumerate() {
            let v = t.vertices.map(|k| mesh.vertices[k as usize]);
            let outside = frustum.planes.iter().any(|p| {
                v.iter()
                    .all(|v| p[0] * v[0] + p[1] * v[1] + p[2] * v[2] + p[3] < 0.0)
            });
            if !outside {
                candidates.push(i as u32);
            }
        }
        if !candidates.is_empty() {
            *tests_matrix = tests_matrix_of(vp);
        }
        candidates.retain(|&i| skip[i as usize] <= frame);
        // Nearest first; equal keys in mesh order (unestablished).
        let e = view.eye;
        let key = |i: &u32| {
            let c = mesh.triangles[*i as usize].centre;
            let d = sub(e, c);
            (d[0] * d[0] + d[1] * d[1]) + d[2] * d[2]
        };
        candidates.sort_by(|a, b| key(a).total_cmp(&key(b)));

        // Section 4.
        let near = near_plane(vp);
        let screen = screen_matrix(vp);
        for &i in candidates.iter() {
            let t = &mesh.triangles[i as usize];
            let k = random.next_k();
            let skip_frame = frame.wrapping_add(3 + k % 7);
            let faces = t.two_sided || dot(t.normal, e) + t.d > 0.0;
            let mut count = 0;
            if faces {
                let v = t.vertices.map(|k| mesh.vertices[k as usize]);
                clip(&near, &v, clipped);
                if clipped.len() >= 3 {
                    let p: Vec<[f32; 3]> = clipped.iter().map(|&p| project(&screen, p)).collect();
                    let neighbours = t.neighbours.map(|n| n.unwrap_or(NONE));
                    for k in 1..p.len() - 1 {
                        count += buffers.triangle(i, neighbours, [p[0], p[k], p[k + 1]]);
                    }
                }
            }
            writes[i as usize] = count;
            if count == 0 {
                skip[i as usize] = skip_frame;
            }
        }

        // Section 7.
        buffers.gap_fill(mesh);
        for r in 0..GRID {
            for c in 0..GRID {
                let d = buffers.depth[(r + 2) * BUFFER + c + 2];
                let d = if d >= -1.99 { d.min(1.99) } else { -1.99 };
                grid[r * GRID + c] = floor_i(d * SCALE);
            }
        }
        let mut finer: &[i32] = grid;
        let mut size = GRID;
        for g in coarse.iter_mut() {
            let half = size / 2;
            for r in 0..half {
                for c in 0..half {
                    let at = |rr: usize, cc: usize| finer[(2 * r + rr) * size + 2 * c + cc];
                    g[r * half + c] = at(0, 0).max(at(0, 1)).max(at(1, 0)).max(at(1, 1));
                }
            }
            finer = g;
            size = half;
        }
    }

    /// The scan of OT 2.1: whether the rectangle (`u0`, `v0`) … (`u1`,
    /// `v1`) at depth `d` is visible, drawing the grid first when it was
    /// not drawn in this frame.
    fn scan(&mut self, rect: [f32; 4], d: f32, view: &View, random: &mut Generator) -> bool {
        let last = GRID as i32 - 1;
        let range = |a: f32, b: f32| {
            let a = cell_index(a).clamp(0, last);
            (a, cell_index(b).clamp(a, last))
        };
        let (j0, j1) = range(rect[0], rect[2]);
        let (i0, i1) = range(rect[1], rect[3]);
        let t = threshold(d);
        if self.drawn != Some(view.frame) && !view.menu_shown {
            self.draw(view, random);
        }
        (i0..=i1).any(|i| {
            let row = &self.grid[i as usize * GRID..][..GRID];
            (j0..=j1).any(|j| row[j as usize] > t)
        })
    }

    /// The screen-rectangle test (OT 2.1).
    pub fn rectangle_visible(
        &mut self,
        rect: [f32; 4],
        d: f32,
        view: &View,
        random: &mut Generator,
    ) -> bool {
        self.scan(rect, d, view, random)
    }

    /// The scan of projected points (OT 2.2 step 2, 2.3 step 3, 2.4), or
    /// visible untested when one is below the near limit.
    fn shape_visible(
        &mut self,
        points: impl Iterator<Item = [f32; 4]>,
        view: &View,
        random: &mut Generator,
    ) -> bool {
        let mut rect: Option<[f32; 5]> = None;
        for [u, v, z, w] in points {
            if z < NEAR_LIMIT {
                return true;
            }
            let q = 1.0 / w;
            let (u, v, d) = (u * q, v * q, z * q);
            match &mut rect {
                None => rect = Some([u, v, u, v, d]),
                Some(r) => {
                    if u < r[0] {
                        r[0] = u;
                    } else if u > r[2] {
                        r[2] = u;
                    }
                    if v < r[1] {
                        r[1] = v;
                    } else if v > r[3] {
                        r[3] = v;
                    }
                    if d < r[4] {
                        r[4] = d;
                    }
                }
            }
        }
        let Some(r) = rect else { return true };
        self.scan([r[0], r[1], r[2], r[3]], r[4], view, random)
    }

    /// The box test (OT 2.2) of the box `min` … `max`; it does not look at
    /// the switch.
    pub fn box_visible(
        &mut self,
        min: Vec3,
        max: Vec3,
        view: &View,
        random: &mut Generator,
    ) -> bool {
        let a = self.tests_matrix;
        let corners = (0..8).map(move |c| {
            let p = std::array::from_fn(|k| if c >> k & 1 == 0 { min[k] } else { max[k] });
            project_ordered(&a, p, BOX_ORDER)
        });
        self.shape_visible(corners, view, random)
    }

    /// The box test with a camera check (OT 2.5).
    pub fn box_with_camera_visible(
        &mut self,
        min: Vec3,
        max: Vec3,
        view: &View,
        random: &mut Generator,
    ) -> bool {
        let e = view.eye;
        !self.on
            || (0..3).all(|k| min[k] <= e[k] && e[k] <= max[k])
            || self.box_visible(min, max, view, random)
    }

    /// The polygon test (OT 2.3).
    pub fn polygon_visible(
        &mut self,
        polygon: &[Vec3],
        view: &View,
        random: &mut Generator,
    ) -> bool {
        if !self.on {
            return true;
        }
        let a = self.tests_matrix;
        let points = polygon.iter().enumerate().map(move |(i, &p)| {
            project_ordered(&a, p, if i == 0 { FIRST_ORDER } else { LATER_ORDER })
        });
        self.shape_visible(points, view, random)
    }

    /// The cached box test (OT 5) of an item with the cache `cached`.
    pub fn cached_visible(
        &mut self,
        cached: &mut Cached,
        min: Vec3,
        max: Vec3,
        view: &View,
        random: &mut Generator,
    ) -> bool {
        let f = view.frame;
        if f < cached.next || !self.on {
            return true;
        }
        let visible = self.box_visible(min, max, view, random);
        cached.next = if visible {
            f.wrapping_add(10 + random.next_k() % 15)
        } else {
            f.wrapping_add(1)
        };
        cached.tested = f;
        visible
    }

    /// The traversal's rule 6 (traversal-options.md 2.3): whether the
    /// polygon test hides the clipped portal `polygon`.
    pub fn portal_hidden(&mut self, polygon: &[Vec3], view: &View, random: &mut Generator) -> bool {
        !self.polygon_visible(polygon, view, random)
    }

    /// Whether the cached box test hides static `visual` with the box
    /// `min` … `max` (OT 6).
    pub fn visual_hidden(
        &mut self,
        visual: u32,
        min: Vec3,
        max: Vec3,
        view: &View,
        random: &mut Generator,
    ) -> bool {
        let mut cached = self.visuals[visual as usize];
        let visible = self.cached_visible(&mut cached, min, max, view, random);
        self.visuals[visual as usize] = cached;
        !visible
    }
}

/// A coordinate's cell index (OT 2.1 step 1).
fn cell_index(c: f32) -> i32 {
    floor_i(64.0 * c + 0.5)
}

/// The scan's threshold for depth `d` (OT 2.1 step 2).
fn threshold(d: f32) -> i32 {
    ceil_i(d * SCALE).saturating_add(1)
}

/// The tests' matrix A of `vp` (OT 1).
fn tests_matrix_of(vp: &Mat4) -> Mat4 {
    vp.map(|r| [0.5 * r[0] + 0.5 * r[3], 0.5 * r[3] - 0.5 * r[1], r[2], r[3]])
}

/// The order of axes in each of *u*′, *v*′, *z*, *w* (OT 2.4).
type Order = [[usize; 3]; 4];
const BOX_ORDER: Order = [[0, 2, 1], [2, 1, 0], [2, 1, 0], [2, 1, 0]];
const FIRST_ORDER: Order = [[1, 2, 0], [0, 1, 2], [0, 1, 2], [0, 1, 2]];
const LATER_ORDER: Order = [[2, 1, 0], [0, 2, 1], [2, 1, 0], [0, 2, 1]];

fn project_ordered(a: &Mat4, p: Vec3, order: Order) -> [f32; 4] {
    std::array::from_fn(|c| {
        let [x, y, z] = order[c];
        ((a[x][c] * p[x] + a[y][c] * p[y]) + a[z][c] * p[z]) + a[3][c]
    })
}

/// The near-side plane of `vp` (OG 4.3): minus the sum of its w and z
/// columns, normalised (in single precision: see the module's note).
fn near_plane(vp: &Mat4) -> [f32; 4] {
    let p: [f32; 4] = std::array::from_fn(|r| -(vp[r][3] + vp[r][2]));
    let l = 1.0 / (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    p.map(|v| v * l)
}

/// The near-side clipping of OG 4.3 of the triangle `v` into `out`.
fn clip(plane: &[f32; 4], v: &[Vec3; 3], out: &mut Vec<Vec3>) {
    let value = |p: Vec3| ((plane[0] * p[0] + plane[1] * p[1]) + plane[2] * p[2]) + plane[3];
    out.clear();
    for i in 0..3 {
        let (p, q) = (v[i], v[(i + 1) % 3]);
        if (0..3).all(|k| (q[k] - p[k]).abs() < SHORT_EDGE) {
            continue;
        }
        let (vp, vq) = (value(p), value(q));
        let p_in = vp.is_sign_negative();
        if p_in {
            out.push(p);
        }
        if p_in != vq.is_sign_negative() {
            let dir = sub(q, p);
            let den = (plane[0] * dir[0] + plane[1] * dir[1]) + plane[2] * dir[2];
            if den != 0.0 {
                let t = -vp / den;
                out.push(std::array::from_fn(|k| p[k] + t * dir[k]));
            }
        }
    }
}

/// The matrix of OG 4.3 mapping to buffer-ready screen coordinates.
fn screen_matrix(vp: &Mat4) -> Mat4 {
    vp.map(|r| [32.0 * (r[0] + r[3]), 32.0 * (r[3] - r[1]), r[2], r[3]])
}

/// (*s*ₓ, *s*ᵧ, *s*_z) of `p` (OG 4.3).
fn project(s: &Mat4, p: Vec3) -> [f32; 3] {
    let c: [f32; 4] =
        std::array::from_fn(|k| ((s[0][k] * p[0] + s[1][k] * p[1]) + s[2][k] * p[2]) + s[3][k]);
    // Divided as 1 / w first, then the products (as OT 1 states for the
    // tests; the worked examples of OG 9 need it).
    let q = 1.0 / c[3];
    [c[0] * q, c[1] * q, c[2] * q]
}

/// An edge of a half (OG 5): its x and depth at the current row and their
/// steps per row.
#[derive(Clone, Copy)]
struct Edge {
    x: f32,
    z: f32,
    dx: f32,
    dz: f32,
}

impl Edge {
    fn new(p: [f32; 3], q: [f32; 3], row: i32) -> Self {
        let inv = 1.0 / (q[1] - p[1]);
        let r = (row as f32 - p[1]) * inv;
        Self {
            x: r * (q[0] - p[0]) + p[0],
            z: r * (q[2] - p[2]) + p[2],
            dx: (q[0] - p[0]) * inv,
            dz: (q[2] - p[2]) * inv,
        }
    }

    fn step(&mut self) {
        self.x += self.dx;
        self.z += self.dz;
    }

    /// The extent over the row strip, smaller and larger end.
    fn extent(&self) -> (f32, f32) {
        let (c, h) = (self.x + self.dx * 0.5, self.dx * 0.5);
        let (a, b) = (c - h, c + h);
        if b < a { (b, a) } else { (a, b) }
    }
}

impl Buffers {
    fn is_own(&self, cell: usize, t: u32, neighbours: [u32; 3]) -> bool {
        let o = self.owner[cell];
        o != NONE && (o == t || neighbours.contains(&o))
    }

    /// One triangle of a fan (OG 5, 6) of occluder `t`; its cell writes.
    fn triangle(&mut self, t: u32, neighbours: [u32; 3], p: [[f32; 3]; 3]) -> u32 {
        let mut p = p;
        // Ordered by y, equal ones in listed order.
        for (i, j) in [(0, 1), (1, 2), (0, 1)] {
            if p[j][1] < p[i][1] {
                p.swap(i, j);
            }
        }
        for v in &mut p {
            v[0] += 2.0;
            v[1] += 2.0;
        }
        let [a, b, c] = p;
        let upper = b[1] - b[1].floor() > 0.5;
        let clamp = |r: i32| r.clamp(0, BUFFER as i32 - 1);
        let mut writes = 0;

        let first = ceil_i(a[1]);
        let mut last = floor_i(b[1]) - 1 + i32::from(upper);
        if last >= floor_i(c[1]) {
            last -= 1;
        }
        let (r0, r1) = (clamp(first), clamp(last));
        if r0 < r1 {
            let (ab, ac) = (Edge::new(a, b, r0), Edge::new(a, c, r0));
            let (l, r) = if ac.dx > ab.dx { (ab, ac) } else { (ac, ab) };
            writes += self.rows(t, neighbours, r0, r1, l, r);
        }

        let mut first = ceil_i(b[1]) - i32::from(!upper);
        if first < ceil_i(a[1]) {
            first += 1;
        }
        let (r0, r1) = (clamp(first), clamp(floor_i(c[1])));
        if r0 < r1 {
            let (ac, bc) = (Edge::new(a, c, r0), Edge::new(b, c, r0));
            let (l, r) = if ac.dx > bc.dx { (ac, bc) } else { (bc, ac) };
            writes += self.rows(t, neighbours, r0, r1, l, r);
        }
        writes
    }

    fn rows(
        &mut self,
        t: u32,
        neighbours: [u32; 3],
        r0: i32,
        r1: i32,
        mut l: Edge,
        mut r: Edge,
    ) -> u32 {
        let mut writes = 0;
        for row in r0..=r1 {
            writes += self.row(t, neighbours, row as usize, &l, &r);
            l.step();
            r.step();
        }
        writes
    }

    /// The cells of one row (OG 6).
    fn row(&mut self, t: u32, neighbours: [u32; 3], row: usize, l: &Edge, r: &Edge) -> u32 {
        let (l_lo, l_hi) = l.extent();
        let (r_lo, r_hi) = r.extent();
        let c0 = (floor_i(l_lo) - 1).clamp(1, 66);
        let c1 = (ceil_i(r_hi) + 1).clamp(1, 66);
        if c0 >= c1 {
            return 0;
        }
        let a = ceil_i(l_hi).clamp(0, 67);
        let b = floor_i(r_lo).clamp(0, 67);
        let lo = a.min(b);
        let hi = a.max(b);
        let inv = 1.0 / (r.x - l.x);
        let z_at = |c: i32| ((c as f32 - l.x) * inv) * (r.z - l.z) + l.z;
        let (z0, z1) = (z_at(c0), z_at(c1));
        let step = (z1 - z0) / (c1 - c0) as f32;
        let base = row * BUFFER;
        let mut writes = 0;
        let mut write = |buf: &mut Self, cell: usize, z: f32| {
            if z < buf.depth[cell] {
                buf.depth[cell] = z;
                buf.owner[cell] = t;
                writes += 1;
            }
        };
        // Left edge cells, then the interior, left to right.
        let mut z = z0;
        for c in c0..b {
            let cell = base + c as usize;
            if c < lo {
                if self.is_own(cell - 1, t, neighbours) {
                    let w = (2.0 * self.depth[cell - 1] + z) * THIRD;
                    write(self, cell, w);
                }
            } else {
                write(self, cell, z);
            }
            z += step;
        }
        // Right edge cells, right to left.
        let mut z = z1;
        for c in (hi..c1).rev() {
            z -= step;
            let cell = base + c as usize;
            if self.is_own(cell + 1, t, neighbours) {
                let w = (2.0 * self.depth[cell + 1] + z) * THIRD;
                write(self, cell, w);
            }
        }
        writes
    }

    /// The gap fill of OG 7.1.
    fn gap_fill(&mut self, mesh: &OcclusionMesh) {
        for i in 0..GRID {
            for j in 0..GRID {
                let x = (i + 2) * BUFFER + j + 2;
                let (u, d1, d2) = (x - BUFFER, x + BUFFER, x + 2 * BUFFER);
                let owner = self.owner[u];
                if owner == NONE {
                    continue;
                }
                let n = mesh.triangles[owner as usize]
                    .neighbours
                    .map(|n| n.unwrap_or(NONE));
                let candidate = if self.is_own(d1, owner, n) {
                    (self.depth[u] + self.depth[d1]) * 0.5
                } else if self.is_own(d2, owner, n) {
                    (self.depth[u] + self.depth[d2]) * 0.5
                } else {
                    continue;
                };
                if candidate < self.depth[x] {
                    self.depth[x] = candidate;
                    self.owner[x] = owner;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
