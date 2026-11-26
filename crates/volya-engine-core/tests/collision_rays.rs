// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The collision world's rays (`ray_nearest`, `ray_any`, `ray_hits`)
//! against the uniform grid they walked before the tree (#396), kept here
//! as the reference: random rays on synthetic worlds and on every level's
//! `level.cform` (the Russian edition, BYOA; skipped when
//! `VOLYA_GOG_INSTALLER` is unset) give the same hits, distances and
//! triangles under every caller's test, and the costs and the index's
//! memory are reported (meaningful in release builds).

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::{Duration, Instant};

use volya_engine_core::collision::geometry::{
    Vec3, cross, ray_triangle_distance, segment_hits_triangle, sub,
};
use volya_engine_core::collision::{CELL_SIZE, Cform, CollisionWorld, RAY_MARGIN};
use volya_engine_core::glows::ray_meets;
use volya_engine_core::particles::collision::front_hit;
use volya_engine_core::sound::occlusion::crosses;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::gamemtl::{self, GameMaterial};
use volya_engine_formats::level_cform::CformTriangle;
use volya_engine_formats::spawn;

/// The grid the rays walked before #396: 2 m cells over x and z (doubled
/// while above 2²² cells), each listing the triangles whose box overlaps it
/// in file order.
struct Grid {
    origin: [f32; 2],
    cell: f32,
    size: [usize; 2],
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl Grid {
    fn new(world: &CollisionWorld) -> Self {
        let (mut min, mut max) = (world.bbox_min, world.bbox_max);
        for v in world.vertices() {
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
            }
        }
        let extent = [(max[0] - min[0]).max(0.0), (max[2] - min[2]).max(0.0)];
        let mut cell = CELL_SIZE;
        while (extent[0] / cell + 1.0) * (extent[1] / cell + 1.0) > (1 << 22) as f32 {
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
        let n = world.triangle_count() as u32;
        for t in 0..n {
            let ((x0, x1), (z0, z1)) = grid.cells(&world.corners(t));
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
        for t in 0..n {
            let ((x0, x1), (z0, z1)) = grid.cells(&world.corners(t));
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let slot = &mut next[z * size[0] + x];
                    grid.items[*slot as usize] = t;
                    *slot += 1;
                }
            }
        }
        grid
    }

    /// The bytes the grid took as #371 stored it: 32-bit cell starts, and
    /// each cell's triangles as steps from the one before in 16-bit words
    /// (two from 2¹⁵).
    fn bytes(&self) -> usize {
        let words: usize = self
            .starts
            .windows(2)
            .map(|w| {
                let items = &self.items[w[0] as usize..w[1] as usize];
                (0..items.len())
                    .map(|i| {
                        let step = if i == 0 {
                            items[0]
                        } else {
                            items[i] - items[i - 1]
                        };
                        if step < 0x8000 { 1 } else { 2 }
                    })
                    .sum::<usize>()
            })
            .sum();
        self.starts.len() * 4 + words * 2
    }

    fn cell_range(&self, min: f32, max: f32, axis: usize) -> (usize, usize) {
        let last = self.size[axis] - 1;
        let at =
            |v: f32| (((v - self.origin[axis]) / self.cell).floor().max(0.0) as usize).min(last);
        (at(min), at(max))
    }

    fn cells(&self, t: &[Vec3; 3]) -> ((usize, usize), (usize, usize)) {
        let [a, b, c] = *t;
        (
            self.cell_range(a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]), 0),
            self.cell_range(a[2].min(b[2]).min(c[2]), a[2].max(b[2]).max(c[2]), 1),
        )
    }

    fn ray_any(
        &self,
        world: &CollisionWorld,
        origin: Vec3,
        dir: Vec3,
        length: f32,
        mut hit: impl FnMut(&[Vec3; 3]) -> bool,
    ) -> bool {
        let mut found = false;
        self.walk(origin, dir, length, |items, _| {
            found = items.iter().any(|&t| hit(&world.corners(t)));
            !found
        });
        found
    }

    fn ray_nearest(
        &self,
        origin: Vec3,
        dir: Vec3,
        length: f32,
        mut hit: impl FnMut(u32) -> Option<f32>,
    ) -> Option<(f32, u32)> {
        let mut best: Option<(f32, u32)> = None;
        self.walk(origin, dir, length, |items, exit| {
            for &t in items {
                if let Some(d) = hit(t)
                    && best.is_none_or(|(b, _)| d < b)
                {
                    best = Some((d, t));
                }
            }
            best.is_none_or(|(b, _)| b >= exit - 1e-4)
        });
        best
    }

    fn walk(
        &self,
        origin: Vec3,
        dir: Vec3,
        length: f32,
        mut visit: impl FnMut(&[u32], f32) -> bool,
    ) {
        let g = self;
        let start = [origin[0], origin[2]];
        let d = [dir[0], dir[2]];
        let (mut t0, mut t1) = (0.0f32, length);
        for axis in 0..2 {
            let lo = g.origin[axis];
            let hi = lo + g.cell * g.size[axis] as f32;
            if d[axis].abs() < 1e-12 {
                if start[axis] < lo || start[axis] > hi {
                    return;
                }
                continue;
            }
            let (a, b) = ((lo - start[axis]) / d[axis], (hi - start[axis]) / d[axis]);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
        if t0 > t1 {
            return;
        }
        let cell_of = |t: f32, axis: usize| {
            let v = start[axis] + d[axis] * t;
            (((v - g.origin[axis]) / g.cell).floor().max(0.0) as usize).min(g.size[axis] - 1)
        };
        let mut cell = [cell_of(t0, 0), cell_of(t0, 1)];
        let step: [isize; 2] = std::array::from_fn(|a| if d[a] > 0.0 { 1 } else { -1 });
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
            let items = &g.items[g.starts[c] as usize..g.starts[c + 1] as usize];
            let a = if next[0] < next[1] { 0 } else { 1 };
            if !visit(items, next[a]) || next[a] > t1 {
                return;
            }
            let moved = cell[a] as isize + step[a];
            if moved < 0 || moved >= g.size[a] as isize {
                return;
            }
            cell[a] = moved as usize;
            next[a] += delta[a];
        }
    }
}

/// SplitMix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in 0 … 1.
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.unit()
    }

    /// Log-uniform in `a` … `b`.
    fn log(&mut self, a: f32, b: f32) -> f32 {
        (a.ln() + (b.ln() - a.ln()) * self.unit()).exp()
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn direction(&mut self) -> Vec3 {
        loop {
            let v = [
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            ];
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if l > 0.1 && l <= 1.0 {
                return v.map(|x| x / l);
            }
        }
    }
}

fn normalize(v: Vec3) -> Option<Vec3> {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (l > 0.0 && l.is_finite()).then(|| v.map(|x| x / l))
}

fn add_scaled(a: Vec3, b: Vec3, k: f32) -> Vec3 {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

/// A random point of triangle `c`.
fn on(rng: &mut Rng, c: &[Vec3; 3]) -> Vec3 {
    let (mut u, mut v) = (rng.unit(), rng.unit());
    if u + v > 1.0 {
        (u, v) = (1.0 - u, 1.0 - v);
    }
    std::array::from_fn(|k| c[0][k] + (c[1][k] - c[0][k]) * u + (c[2][k] - c[0][k]) * v)
}

/// A ray of one of the kinds the callers cast, or one aimed at rounding's
/// cases: from on or near a random triangle, in a random or the triangle's
/// own plane's direction, or at its corner or edge, or straight up or down.
fn ray(rng: &mut Rng, world: &CollisionWorld) -> (Vec3, Vec3, f32) {
    let kind = rng.below(8);
    ray_of(rng, world, kind)
}

fn ray_of(rng: &mut Rng, world: &CollisionWorld, kind: usize) -> (Vec3, Vec3, f32) {
    let n = world.triangle_count();
    let c = world.corners(rng.below(n) as u32);
    let p = on(rng, &c);
    match kind {
        // A particle's step.
        0 | 1 => {
            let o = add_scaled(p, rng.direction(), rng.range(0.0, 0.3));
            (o, rng.direction(), rng.log(1e-5, 0.5))
        }
        // Glows', lights' and sounds' rays.
        2 => {
            let o = add_scaled(p, rng.direction(), rng.range(0.0, 3.0));
            (o, rng.direction(), rng.log(0.5, 300.0))
        }
        // The sector ray (500 m down) and the sky's (up).
        3 => {
            let o = add_scaled(p, [0.0, 1.0, 0.0], rng.range(0.0, 3.0));
            let up = rng.below(2) == 0;
            (
                o,
                [0.0, if up { 1.0 } else { -1.0 }, 0.0],
                rng.range(1.0, 500.0),
            )
        }
        // Along the triangle's plane, from on it, nearly grazing.
        4 | 5 => {
            let e1 = sub(c[1], c[0]);
            let e2 = sub(c[2], c[0]);
            let Some(normal) = normalize(cross(e1, e2)) else {
                return (p, rng.direction(), 1.0);
            };
            let along = add_scaled(e1, e2, rng.range(-2.0, 2.0));
            let tilt = if rng.below(4) == 0 {
                0.0
            } else {
                rng.log(1e-7, 1e-2) * if rng.below(2) == 0 { 1.0 } else { -1.0 }
            };
            let d = normalize(add_scaled(
                along,
                normal,
                tilt * (along[0].abs() + along[1].abs() + along[2].abs()),
            ))
            .unwrap_or([1.0, 0.0, 0.0]);
            let back = rng.range(0.0, 2.0);
            (add_scaled(p, d, -back), d, back + rng.log(1e-3, 5.0))
        }
        // At a corner or an edge's middle, ending just short or past it.
        _ => {
            let k = rng.below(3);
            let target = if rng.below(2) == 0 {
                c[k]
            } else {
                std::array::from_fn(|a| (c[k][a] + c[(k + 1) % 3][a]) * 0.5)
            };
            let o = add_scaled(target, rng.direction(), rng.log(1e-3, 20.0));
            let Some(d) = normalize(sub(target, o)) else {
                return (o, rng.direction(), 1.0);
            };
            let dist = sub(target, o);
            let dist = (dist[0] * dist[0] + dist[1] * dist[1] + dist[2] * dist[2]).sqrt();
            (o, d, dist * rng.range(0.999, 1.001))
        }
    }
}

/// Whether the segment meets the box of `c` widened by `RAY_MARGIN`.
fn passes(o: Vec3, d: Vec3, length: f32, c: &[Vec3; 3]) -> bool {
    let (mut t0, mut t1) = (0.0f32, length);
    for k in 0..3 {
        let lo = c[0][k].min(c[1][k]).min(c[2][k]) - RAY_MARGIN;
        let hi = c[0][k].max(c[1][k]).max(c[2][k]) + RAY_MARGIN;
        if d[k] == 0.0 {
            if !(o[k] >= lo && o[k] <= hi) {
                return false;
            }
        } else {
            let (a, b) = ((lo - o[k]) / d[k], (hi - o[k]) / d[k]);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
    }
    t0 <= t1
}

/// A caller's test of a triangle by its corners.
type Test<'a> = dyn FnMut(&[Vec3; 3]) -> bool + 'a;

/// What comparing a ray found.
#[derive(Default)]
struct Outcome {
    /// Some query hit.
    hit: bool,
    /// Under the grid, some caller's test met a triangle whose box the
    /// segment misses (rounding: the ray lies in the triangle's plane or
    /// the triangle is a sliver).
    off: bool,
    /// The results differed (only allowed where `off`).
    differed: bool,
}

/// Every caller's query on the ray, by the world and by the grid: the
/// same results, unless the grid's tests met a triangle off the segment.
fn compare(world: &CollisionWorld, grid: &Grid, (o, d, length): (Vec3, Vec3, f32)) -> Outcome {
    let off = std::cell::Cell::new(false);
    let mut differed = false;
    let mut check = |same: bool, what: &str| {
        if !same {
            assert!(
                off.get(),
                "{what}: o {o:?} d {d:?} length {length:?}: differs with every test's hit on the segment"
            );
            differed = true;
        }
    };
    let watch = |c: &[Vec3; 3], met: bool| {
        if met && !passes(o, d, length, c) {
            off.set(true);
        }
        met
    };
    let bits = |r: Option<(f32, u32)>| r.map(|(t, i)| (t.to_bits(), i));
    // Particles (front side, below the length).
    let front = |i: u32| front_hit(o, d, &world.corners(i)).filter(|&t| t < length);
    let a = world.ray_nearest(o, d, length, front);
    let old = grid.ray_nearest(o, d, length, |i| {
        front(i).filter(|_| watch(&world.corners(i), true))
    });
    check(bits(a) == bits(old), "front");
    // Either side, 0 … length.
    let either = |i: u32| {
        ray_triangle_distance(o, d, &world.corners(i)).filter(|&t| (0.0..=length).contains(&t))
    };
    let b = world.ray_nearest(o, d, length, either);
    let old = grid.ray_nearest(o, d, length, |i| {
        either(i).filter(|_| watch(&world.corners(i), true))
    });
    check(bits(b) == bits(old), "either");
    // Sound occlusion, both sides and front only.
    for two_sided in [true, false] {
        let s = |t: &[Vec3; 3]| crosses(o, d, length, t, two_sided);
        let new = world.ray_any(o, d, length, s);
        let old = grid.ray_any(world, o, d, length, |t| watch(t, s(t)));
        check(new == old, "crosses");
    }
    // The glows' occlusion: the first triangle met.
    let first = |any: &dyn Fn(&mut Test) -> bool| {
        let mut hit = None;
        let found = any(&mut |t| {
            let m = watch(t, ray_meets(o, d, length, t));
            if m {
                hit = Some(*t);
            }
            m
        });
        (found, hit)
    };
    let new = first(&|f| world.ray_any(o, d, length, f));
    let old = first(&|f| grid.ray_any(world, o, d, length, f));
    check(new == old, "glow");
    let h = world.ray_hits(o, d, length);
    let old = grid.ray_any(world, o, d, length, |t| {
        watch(t, segment_hits_triangle(o, d, length, t))
    });
    check(h == old, "hits");
    Outcome {
        hit: a.is_some() || b.is_some() || h,
        off: off.get(),
        differed,
    }
}

/// `count` random rays on `world`, compared: how many hit, how many met
/// a triangle off the segment under the grid, and how many of those
/// differed.
fn equivalent(world: &CollisionWorld, grid: &Grid, seed: u64, count: usize) -> [usize; 3] {
    let mut rng = Rng(seed);
    let mut n = [0; 3];
    for _ in 0..count {
        let r = compare(world, grid, ray(&mut rng, world));
        n[0] += usize::from(r.hit);
        n[1] += usize::from(r.off);
        n[2] += usize::from(r.differed);
    }
    n
}

fn materials() -> Vec<GameMaterial> {
    vec![GameMaterial {
        id: 0,
        name: "default".into(),
        ..GameMaterial::default()
    }]
}

#[test]
fn rays_match_the_grid_on_random_worlds() {
    let mut rng = Rng(396);
    for world_seed in 0..8 {
        // Clusters of small triangles, some large ones, and runs of
        // coplanar neighbours, in a file order that mixes them.
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        let extent = 20.0 * (world_seed + 1) as f32;
        for _ in 0..3000 {
            let size = match rng.below(10) {
                0 => rng.range(5.0, 40.0),
                1..=3 => rng.range(0.5, 3.0),
                _ => rng.log(0.01, 0.5),
            };
            let centre = [
                rng.range(-extent, extent),
                rng.range(-5.0, 10.0),
                rng.range(-extent, extent),
            ];
            let base = vertices.len() as u32;
            if rng.below(3) == 0 {
                // A strip of coplanar triangles.
                let (u, v) = (rng.direction(), rng.direction());
                for i in 0..6 {
                    for j in 0..2 {
                        let s = size * i as f32 / 5.0;
                        vertices.push(add_scaled(add_scaled(centre, u, s), v, size * j as f32));
                    }
                }
                for i in 0..5u32 {
                    let q = base + 2 * i;
                    triangles.push([q, q + 1, q + 3]);
                    triangles.push([q, q + 3, q + 2]);
                }
            } else {
                for _ in 0..3 {
                    vertices.push(add_scaled(centre, rng.direction(), size));
                }
                triangles.push([base, base + 1, base + 2]);
            }
        }
        let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for v in &vertices {
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
            }
        }
        let cform = Cform {
            bbox_min: min,
            bbox_max: max,
            vertices,
            triangles: triangles
                .into_iter()
                .map(|vertices| CformTriangle {
                    vertices,
                    attributes: 0,
                })
                .collect(),
        };
        let world = CollisionWorld::new(cform, materials()).unwrap();
        let grid = Grid::new(&world);
        let [hits, off, differed] = equivalent(&world, &grid, world_seed, 20_000);
        assert!(hits > 2_000, "{hits}");
        eprintln!(
            "random world {world_seed}: 20000 rays, {hits} hitting, {off} meeting a triangle \
             off the segment under the grid, {differed} of those differing"
        );
    }
}

fn with_tree(test: impl FnOnce(&dyn Vfs)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    test(&tree);
}

/// The mean time of `f` over `rays`.
fn per_ray(rays: &[(Vec3, Vec3, f32)], mut f: impl FnMut(Vec3, Vec3, f32) -> bool) -> Duration {
    let start = Instant::now();
    let mut hits = 0;
    for _ in 0..5 {
        for &(o, d, l) in rays {
            hits += usize::from(f(o, d, l));
        }
    }
    std::hint::black_box(hits);
    start.elapsed() / (5 * rays.len() as u32)
}

#[test]
fn rays_match_the_grid_on_every_level() {
    with_tree(|vfs| {
        let lib = gamemtl::parse(&read(vfs, "gamedata/gamemtl.xr").unwrap()).unwrap();
        let levels: Vec<String> = vfs
            .read_dir("gamedata/levels")
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == EntryKind::Dir)
            .map(|e| e.name)
            .filter(|n| {
                vfs.open(&format!("gamedata/levels/{n}/level.cform"))
                    .is_ok()
            })
            .collect();
        let count = if cfg!(debug_assertions) {
            1_000
        } else {
            20_000
        };
        let (mut grids, mut trees, mut total, mut offs, mut differs) = (0, 0, 0, 0, 0);
        for (i, name) in levels.iter().enumerate() {
            let f = vfs
                .open(&format!("gamedata/levels/{name}/level.cform"))
                .unwrap();
            let world = CollisionWorld::read(f, lib.clone()).unwrap();
            let grid = Grid::new(&world);
            let [hits, off, differed] = equivalent(&world, &grid, i as u64, count);
            assert!(hits > count / 10, "{name}: {hits} of {count} hit");
            // Triangles take 8 bytes where the vertex indices fit 21 bits,
            // with a material byte and two sector bytes each.
            let n = world.triangle_count();
            let per = if world.vertices().len() <= 1 << 21 {
                8
            } else {
                12
            };
            let tree = world.memory_bytes() - world.vertices().len() * 12 - n * (per + 3);
            eprintln!(
                "{name}: {count} rays, {hits} hitting, {off} meeting a triangle off the \
                 segment under the grid, {differed} of those differing; index {:.1} MB, \
                 was {:.1} MB",
                tree as f64 / 1e6,
                grid.bytes() as f64 / 1e6
            );
            (grids, trees, total) = (grids + grid.bytes(), trees + tree, total + count);
            (offs, differs) = (offs + off, differs + differed);
        }
        eprintln!(
            "{} levels: {total} rays the same but for {differs} of the {offs} meeting a \
             triangle off the segment under the grid; indices {:.1} MB, were {:.1} MB",
            levels.len(),
            trees as f64 / 1e6,
            grids as f64 / 1e6
        );
    });
}

#[test]
fn the_cost_of_rays_at_the_first_levels_start() {
    with_tree(|vfs| {
        let lib = gamemtl::parse(&read(vfs, "gamedata/gamemtl.xr").unwrap()).unwrap();
        let f = vfs.open("gamedata/levels/l01_escape/level.cform").unwrap();
        let world = CollisionWorld::read(f, lib).unwrap();
        let grid = Grid::new(&world);
        let actor = spawn::actor_start(&read(vfs, "gamedata/spawns/all.spawn").unwrap()).unwrap();
        let p = actor.position;
        let mut rng = Rng(1);
        let around = |rng: &mut Rng| {
            [
                p[0] + rng.range(-1.0, 1.0),
                p[1] + rng.range(0.0, 2.0),
                p[2] + rng.range(-1.0, 1.0),
            ]
        };
        let short: Vec<_> = (0..2000)
            .map(|_| (around(&mut rng), rng.direction(), rng.log(1e-3, 0.5)))
            .collect();
        let long: Vec<_> = (0..2000)
            .map(|_| (around(&mut rng), rng.direction(), rng.range(5.0, 100.0)))
            .collect();
        for (what, rays) in [("particle-like", &short), ("5–100 m", &long)] {
            for &r in rays {
                compare(&world, &grid, r);
            }
            let world = &world;
            let nearest = |o: Vec3, d: Vec3, l: f32| {
                move |i: u32| front_hit(o, d, &world.corners(i)).filter(|&t| t < l)
            };
            let new = per_ray(rays, |o, d, l| {
                world.ray_nearest(o, d, l, nearest(o, d, l)).is_some()
            });
            let old = per_ray(rays, |o, d, l| {
                grid.ray_nearest(o, d, l, nearest(o, d, l)).is_some()
            });
            let any = |o: Vec3, d: Vec3, l: f32| move |t: &[Vec3; 3]| crosses(o, d, l, t, true);
            let new_any = per_ray(rays, |o, d, l| world.ray_any(o, d, l, any(o, d, l)));
            let old_any = per_ray(rays, |o, d, l| grid.ray_any(world, o, d, l, any(o, d, l)));
            eprintln!(
                "l01_escape start, {what} rays: ray_nearest {new:.2?} (grid {old:.2?}), \
                 ray_any {new_any:.2?} (grid {old_any:.2?})"
            );
        }
    });
}
