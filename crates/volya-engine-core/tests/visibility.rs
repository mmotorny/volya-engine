// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The camera pass's sectors and portals on the Russian edition's levels
//! (BYOA; skipped when `VOLYA_GOG_INSTALLER` is unset): `l01_escape`'s
//! sectors and portals (docs/formats/level.md 10), the camera sector and
//! the visited sectors in the bunker at the start and outdoors above it,
//! the one-sector levels and the sectors without portals
//! (docs/behavior/visibility-scope.md 2, 6), and the camera outside every
//! sector; `l01_escape`'s occlusion mesh (docs/formats/level-hom.md 4), its
//! grid and what the occlusion tests hide in the bunker and outdoors.
//! Reports the pass's cost (meaningful in release builds) and the static
//! visuals it keeps with occlusion, without it, and by the frustum and size
//! test alone.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::Instant;

use volya_engine_core::camera::{FOV_Y_DEGREES, Mat4, Vec3, projection, view_matrix};
use volya_engine_core::collision::CollisionWorld;
use volya_engine_core::lights::multiply;
use volya_engine_core::occlusion::{EMPTY, Occlusion, OcclusionMesh, View};
use volya_engine_core::particles::random::Generator;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, Vfs, read};
use volya_engine_core::visibility::{CameraFrame, CameraSector, Frustum, Sectors};
use volya_engine_formats::gamemtl::{self, GameMaterial};
use volya_engine_formats::level::{GEOMETRY_LOD, Level, SSA_LOD_A, SSA_LOD_B, Thresholds};
use volya_engine_formats::level_hom;
use volya_engine_formats::ogf::VisualType;
use volya_engine_formats::spawn;

const W: u32 = 1920;
const H: u32 = 1080;
/// The far plane of the views here (our choice).
const FAR: f32 = 200.0;

fn with_tree(test: impl FnOnce(&dyn Vfs)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    test(&tree);
}

struct Loaded {
    level: Level,
    sectors: Sectors,
    collision: CollisionWorld,
    /// The occlusion mesh, none without `level.hom`.
    mesh: Option<OcclusionMesh>,
}

fn load(vfs: &dyn Vfs, lib: &[GameMaterial], name: &str) -> Loaded {
    let level =
        Level::parse(&read(vfs, &format!("gamedata/levels/{name}/level")).unwrap()).unwrap();
    let sectors = Sectors::new(&level).unwrap();
    let f = vfs
        .open(&format!("gamedata/levels/{name}/level.cform"))
        .unwrap();
    let collision = CollisionWorld::read(f, lib.to_vec()).unwrap();
    let mesh = read(vfs, &format!("gamedata/levels/{name}/level.hom"))
        .ok()
        .map(|d| OcclusionMesh::new(&level_hom::parse(&d).unwrap()));
    Loaded {
        level,
        sectors,
        collision,
        mesh,
    }
}

impl Loaded {
    /// A fresh occlusion, with the mesh or without.
    fn occlusion(&self, with_mesh: bool) -> Occlusion {
        let mesh = self.mesh.clone().filter(|_| with_mesh);
        Occlusion::new(mesh, self.sectors.visual_count())
    }

    /// The camera pass in frame 1 from `sector` at `eye` with heading
    /// `yaw`, with the occlusion `o`.
    fn pass(&self, sector: Option<u16>, eye: Vec3, yaw: f32, o: &mut Occlusion) -> CameraFrame {
        let v = View {
            frame: 1,
            view_projection: view(eye, yaw),
            eye,
            menu_shown: false,
        };
        (self.sectors).camera_pass(sector, &v, &thresholds(), o, &mut Generator::new(1))
    }
}

fn thresholds() -> Thresholds {
    Thresholds::new(W, H, FOV_Y_DEGREES, GEOMETRY_LOD, SSA_LOD_A, SSA_LOD_B)
}

/// The view-projection at `eye` with heading `yaw` (radians, 0 along +z).
fn view(eye: Vec3, yaw: f32) -> Mat4 {
    let p = projection(FOV_Y_DEGREES.to_radians(), H as f32 / W as f32, 0.2, FAR);
    multiply(
        &view_matrix(eye, [yaw.sin(), 0.0, yaw.cos()], [0.0, 1.0, 0.0]),
        &p,
    )
}

/// The headings of eight views around.
fn around() -> impl Iterator<Item = f32> {
    (0..8).map(|k| k as f32 * std::f32::consts::TAU / 8.0)
}

/// The sectors visited from `eye` in sector `sector` over the eight views,
/// without occlusion.
fn visited_around(l: &Loaded, sector: u16, eye: Vec3) -> Vec<u16> {
    let mut all: Vec<u16> = around()
        .flat_map(|yaw| {
            l.pass(Some(sector), eye, yaw, &mut l.occlusion(false))
                .traversal
                .visited
        })
        .collect();
    all.sort_unstable();
    all.dedup();
    all
}

/// The meshes and trees drawn by the camera pass, with occlusion (its first
/// frame, every visual tested) and without, and those the frustum and size
/// test alone keep (as the renderer selects them today: every sector's
/// visuals, groups descended, billboards not drawn).
fn kept(l: &Loaded, sector: u16, eye: Vec3, yaw: f32) -> (usize, usize, usize) {
    let t = thresholds();
    let m = view(eye, yaw);
    let drawn = |o: &mut Occlusion| {
        let frame = l.pass(Some(sector), eye, yaw, o);
        (frame.visuals.iter().flatten())
            .filter(|&&v| l.level.visuals[v as usize].header.kind != VisualType::BillboardGroup)
            .count()
    };
    let occluded = drawn(&mut l.occlusion(true));
    let traversed = drawn(&mut l.occlusion(false));
    let f = Frustum::view(&m);
    let mut today = 0;
    let mut seen = vec![false; l.level.visuals.len()];
    let mut stack: Vec<u32> = l.level.sectors.iter().map(|s| s.root).collect();
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut seen[i as usize], true) {
            continue;
        }
        let v = &l.level.visuals[i as usize];
        let h = &v.header;
        if matches!(h.kind, VisualType::Group | VisualType::BillboardGroup) {
            stack.extend(&v.children);
            continue;
        }
        let d: f32 = (0..3).map(|k| (h.sphere_centre[k] - eye[k]).powi(2)).sum();
        if f.meets(h.sphere_centre, h.sphere_radius) && h.sphere_radius / (d + 1e-5) > t.t_min {
            today += 1;
        }
    }
    (occluded, traversed, today)
}

/// The written cells of the grid drawn from `eye` with heading `yaw`.
fn written(l: &Loaded, eye: Vec3, yaw: f32) -> usize {
    let mut o = l.occlusion(true);
    let v = View {
        frame: 1,
        view_projection: view(eye, yaw),
        eye,
        menu_shown: false,
    };
    o.draw(&v, &mut Generator::new(1));
    o.grid().iter().filter(|&&c| c < EMPTY).count()
}

#[test]
fn the_first_levels_sectors_and_portals() {
    with_tree(|vfs| {
        let lib = gamemtl::parse(&read(vfs, "gamedata/gamemtl.xr").unwrap()).unwrap();
        let l = load(vfs, &lib, "l01_escape");
        // level.md 10.
        let s = &l.sectors;
        assert_eq!((s.portals.len(), s.sector_portals.len()), (69, 24));
        assert_eq!(s.sector_portals.iter().map(Vec::len).sum::<usize>(), 138);
        assert_eq!(s.sector_portals[0], [25, 57]);
        let p = &s.portals[0];
        assert_eq!((p.sector_a, p.sector_b), (16, 23));
        assert!(
            (p.normal[0] + 1.0).abs() < 1e-4 && p.normal[1].abs() < 1e-4,
            "{:?}",
            p.normal
        );
        assert!((p.d + 215.882).abs() < 1e-3, "{}", p.d);
        let c = [-215.882, -18.394, -167.358];
        assert!(
            (0..3).all(|k| (p.centre[k] - c[k]).abs() < 1e-3),
            "{:?}",
            p.centre
        );
        assert!((p.radius - 0.881).abs() < 1e-3, "{}", p.radius);

        // The start in the bunker and outdoors on the ground above it
        // (y −7.659, as in level_sound.rs), 1.5 m up (our choice).
        let actor = spawn::actor_start(&read(vfs, "gamedata/spawns/all.spawn").unwrap()).unwrap();
        let bunker = [
            actor.position[0],
            actor.position[1] + 1.5,
            actor.position[2],
        ];
        let outdoors = [bunker[0], -7.659 + 1.5, bunker[2]];
        // visibility-scope.md 6: none after a load, also with the camera
        // outside every sector.
        let mut camera = CameraSector::default();
        let nowhere = [l.collision.bbox_max[0] + 1000.0, 0.0, 0.0];
        assert_eq!(camera.update(nowhere, s, &l.collision), None);
        let frame = l.pass(camera.get(), nowhere, 0.0, &mut l.occlusion(true));
        assert!(frame.traversal.visited.is_empty() && frame.visuals.is_empty());
        // The visited sectors over eight views (measured with this
        // implementation): the bunker's two sectors, none outdoors.
        let inside = camera.update(bunker, s, &l.collision).unwrap();
        let v_in = visited_around(&l, inside, bunker);
        assert_eq!((inside, v_in.as_slice()), (2, &[1, 2][..]));
        let outside = camera.update(outdoors, s, &l.collision).unwrap();
        let v_out = visited_around(&l, outside, outdoors);
        assert_eq!((outside, v_out.as_slice()), (23, &[16, 17, 18, 23][..]));
        // Outside every sector after being inside, the last is kept.
        assert_eq!(camera.update(nowhere, s, &l.collision), Some(23));
        // Re-determined only after a move.
        assert_eq!(camera.update(nowhere, s, &l.collision), Some(23));

        // The cost: the camera sector's ray and the camera pass, with
        // occlusion and without, frame after frame (so with the caches and
        // skip counts in play), 50 frames a heading.
        eprintln!(
            "the collision triangles' sectors: {:.1} MB",
            l.collision.triangle_count() as f64 * 2.0 / 1e6
        );
        let t = thresholds();
        let frames = |eye: Vec3| {
            let views: Vec<Mat4> = around().map(|yaw| view(eye, yaw)).collect();
            (0..400u32).map(move |frame| View {
                frame,
                view_projection: views[frame as usize / 50],
                eye,
                menu_shown: false,
            })
        };
        for (what, sector, eye) in [("bunker", inside, bunker), ("outdoors", outside, outdoors)] {
            let per_frame = |with_mesh: bool| {
                let mut o = l.occlusion(with_mesh);
                let mut g = Generator::new(1);
                let start = Instant::now();
                let mut n = 0;
                for v in frames(eye) {
                    let mut c = CameraSector::default();
                    let found = c.update(eye, s, &l.collision);
                    n += std::hint::black_box(s.camera_pass(found, &v, &t, &mut o, &mut g))
                        .visuals
                        .len();
                }
                std::hint::black_box(n);
                start.elapsed() / 400
            };
            let (with, without) = (per_frame(true), per_frame(false));
            let start = Instant::now();
            for _ in 0..400 {
                let mut c = CameraSector::default();
                std::hint::black_box(c.update(eye, s, &l.collision));
            }
            let ray = start.elapsed() / 400;
            let mut o = l.occlusion(true);
            let mut g = Generator::new(1);
            let start = Instant::now();
            for v in frames(eye) {
                o.draw(&v, &mut g);
            }
            let grid = start.elapsed() / 400;
            eprintln!(
                "{what}: {with:.1?} a frame with occlusion (the grid's drawing {grid:.1?}), \
                 {without:.1?} without; the camera sector's ray {ray:.1?} of each"
            );
            for yaw in around().step_by(2) {
                let (occluded, traversed, today) = kept(&l, sector, eye, yaw);
                assert!(occluded <= traversed && traversed <= today);
                eprintln!(
                    "{what}, heading {:.0}°: {occluded} meshes and trees drawn with occlusion, \
                     {traversed} without, {today} by the frustum and size alone",
                    yaw.to_degrees()
                );
            }
        }
    });
}

#[test]
fn the_first_levels_occlusion() {
    with_tree(|vfs| {
        let lib = gamemtl::parse(&read(vfs, "gamedata/gamemtl.xr").unwrap()).unwrap();
        let l = load(vfs, &lib, "l01_escape");
        // level-hom.md 4: 904 records, all two-sided, 918 vertices after
        // merging, 862 edges of one triangle.
        let data = read(vfs, "gamedata/levels/l01_escape/level.hom").unwrap();
        assert_eq!(data.len(), 36_180);
        let records = level_hom::parse(&data).unwrap();
        assert_eq!(records.len(), 904);
        let mesh = l.mesh.as_ref().unwrap();
        assert!(mesh.triangles.iter().all(|t| t.two_sided));
        assert_eq!(mesh.vertices.len(), 918);
        let single = (mesh.triangles.iter())
            .flat_map(|t| t.neighbours)
            .filter(Option::is_none)
            .count();
        assert_eq!(single, 862);

        // The grid at a few camera positions (measured with this
        // implementation): from the bunker the occluders above it cover
        // much of the screen, outdoors fewer, and none to the west.
        let actor = spawn::actor_start(&read(vfs, "gamedata/spawns/all.spawn").unwrap()).unwrap();
        let bunker = [
            actor.position[0],
            actor.position[1] + 1.5,
            actor.position[2],
        ];
        let outdoors = [bunker[0], -7.659 + 1.5, bunker[2]];
        for (what, eye) in [("bunker", bunker), ("outdoors", outdoors)] {
            let cells: Vec<usize> = around().map(|yaw| written(&l, eye, yaw)).collect();
            eprintln!("{what}: grid cells written over eight headings {cells:?}");
            assert!(cells.iter().all(|&c| c > 2000) || what == "outdoors");
            assert!(cells[2] > 1000 && cells[6] == 0 || what == "bunker");
        }

        // What the tests hide: the static visuals the camera pass keeps
        // with occlusion against without, over the eight headings. In the
        // bunker every visual it keeps is nearer than the occluders above;
        // outdoors the buildings hide some.
        let mut camera = CameraSector::default();
        for (what, eye) in [("bunker", bunker), ("outdoors", outdoors)] {
            let sector = camera.update(eye, &l.sectors, &l.collision).unwrap();
            let mut hidden = Vec::new();
            for yaw in around() {
                let (occluded, traversed, _) = kept(&l, sector, eye, yaw);
                hidden.push(traversed - occluded);
            }
            eprintln!(
                "{what}: meshes and trees hidden by occlusion over eight headings {hidden:?}"
            );
            let total: usize = hidden.iter().sum();
            assert_eq!(total == 0, what == "bunker", "{what}");
        }
    });
}

/// A point 0.5 m above a triangle of `sector` facing up from which the
/// camera sector is `sector`, if any.
fn above(l: &Loaded, sector: u16) -> Option<Vec3> {
    (0..l.collision.triangle_count() as u32)
        .filter(|&i| l.collision.sector(i) == sector)
        .map(|i| {
            let c = l.collision.corners(i);
            [
                (c[0][0] + c[1][0] + c[2][0]) / 3.0,
                (c[0][1] + c[1][1] + c[2][1]) / 3.0 + 0.5,
                (c[0][2] + c[1][2] + c[2][2]) / 3.0,
            ]
        })
        .find(|&p| CameraSector::default().update(p, &l.sectors, &l.collision) == Some(sector))
}

#[test]
fn one_sector_levels_and_sectors_without_portals() {
    with_tree(|vfs| {
        let lib = gamemtl::parse(&read(vfs, "gamedata/gamemtl.xr").unwrap()).unwrap();
        // visibility-scope.md 2 [S1]: one sector and no portals; the ray
        // meets only collision triangles.
        for name in ["l02_garbage", "l12_stancia_2", "testers_mp_factory"] {
            let l = load(vfs, &lib, name);
            assert_eq!(
                (l.sectors.roots.len(), l.sectors.portals.len()),
                (1, 0),
                "{name}"
            );
            let eye = above(&l, 0).unwrap_or_else(|| panic!("{name}: no camera sector"));
            for yaw in around() {
                let frame = l.pass(Some(0), eye, yaw, &mut l.occlusion(true));
                assert_eq!(frame.traversal.visited, [0], "{name}");
                assert!(frame.traversal.fade.is_empty());
            }
        }
        // Seven sectors in five levels have no portal: visited only from
        // within, and from them no other is reached.
        let mut found = 0;
        for (name, count) in [
            ("l04u_labx18", 1),
            ("l05_bar", 1),
            ("l08u_brainlab", 1),
            ("l10u_bunker", 1),
            ("l12u_control_monolith", 3),
        ] {
            let l = load(vfs, &lib, name);
            // level-hom.md 1: l10u_bunker has no occlusion mesh.
            assert_eq!(l.mesh.is_none(), name == "l10u_bunker", "{name}");
            let alone: Vec<u16> = (0..l.sectors.roots.len() as u16)
                .filter(|&s| l.sectors.sector_portals[usize::from(s)].is_empty())
                .collect();
            assert_eq!(alone.len(), count, "{name}");
            for s in alone {
                let Some(eye) = above(&l, s) else {
                    eprintln!("{name}: sector {s} is found by no ray from above its floor");
                    continue;
                };
                found += 1;
                for yaw in around() {
                    let frame = l.pass(Some(s), eye, yaw, &mut l.occlusion(false));
                    assert_eq!(frame.traversal.visited, [s], "{name}");
                }
            }
        }
        eprintln!("{found} of the 7 sectors without portals reached by the camera");
        assert!(found > 0);
    });
}
