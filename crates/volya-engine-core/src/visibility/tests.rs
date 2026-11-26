// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;
use crate::camera::{projection, view_matrix};
use crate::collision::{Cform, test_world};
use crate::lights::multiply;
use volya_engine_formats::level::{GEOMETRY_LOD, SSA_LOD_A, SSA_LOD_B};
use volya_engine_formats::level_cform::CformTriangle;

/// A square portal of half-size 1 in the plane x = `x`, sector A on the
/// side x < `x`.
fn portal(x: f32, a: u16, b: u16) -> Portal {
    Portal {
        sector_a: a,
        sector_b: b,
        polygon: vec![
            [x, -1.0, -1.0],
            [x, 1.0, -1.0],
            [x, 1.0, 1.0],
            [x, -1.0, 1.0],
        ],
        normal: [-1.0, 0.0, 0.0],
        d: x,
        centre: [x, 0.0, 0.0],
        radius: 2f32.sqrt(),
    }
}

fn node(kind: VisualType, centre: Vec3, children: Vec<u32>) -> Node {
    Node {
        kind,
        centre,
        radius: 1.0,
        box_min: [0.0; 3],
        box_max: [0.0; 3],
        coverage: 1.0,
        children,
    }
}

/// Three sectors in a row along x: 0 (x < 0), 1 (0 … 10), 2 (x > 10),
/// joined by portals 0 (x = 0) and 1 (x = 10); a mesh at x = −5, 5 and
/// 15 in each, under the roots 3, 4, 5.
fn row() -> Sectors {
    Sectors {
        portals: vec![portal(0.0, 0, 1), portal(10.0, 1, 2)],
        sector_portals: vec![vec![0], vec![0, 1], vec![1]],
        roots: vec![3, 4, 5],
        nodes: vec![
            node(VisualType::Mesh, [-5.0, 0.0, 0.0], vec![]),
            node(VisualType::Mesh, [5.0, 0.0, 0.0], vec![]),
            node(VisualType::Mesh, [15.0, 0.0, 0.0], vec![]),
            node(VisualType::Group, [-5.0, 0.0, 0.0], vec![0]),
            node(VisualType::Group, [5.0, 0.0, 0.0], vec![1]),
            node(VisualType::Group, [15.0, 0.0, 0.0], vec![2]),
        ],
    }
}

fn thresholds() -> Thresholds {
    Thresholds::new(1920, 1080, 67.5, GEOMETRY_LOD, SSA_LOD_A, SSA_LOD_B)
}

/// The view-projection at `eye` looking along `dir`.
fn view(eye: Vec3, dir: Vec3) -> Mat4 {
    let p = projection(67.5f32.to_radians(), 1080.0 / 1920.0, 0.2, 2000.0);
    multiply(&view_matrix(eye, dir, [0.0, 1.0, 0.0]), &p)
}

#[test]
fn sectors_are_entered_through_portals_with_narrowed_frusta() {
    let s = row();
    let eye = [-8.0, 0.0, 0.0];
    let frame = s.camera_pass(Some(0), &view(eye, [1.0, 0.0, 0.0]), eye, &thresholds());
    assert_eq!(frame.traversal.visited, [0, 1, 2]);
    assert_eq!(frame.visuals, [vec![0], vec![1], vec![2]]);
    // Sector 1 is seen through the 2 m portal 8 m away: at x = 5 the
    // narrowed frustum is 2 × 13 / 8 m high, the view's about 17 m.
    let f = &frame.traversal.frusta[1];
    assert_eq!(f.len(), 1);
    assert!(f[0].meets([5.0, 1.5, 0.0], 0.0));
    assert!(!f[0].meets([5.0, 2.0, 0.0], 0.0));
    assert!(Frustum::view(&view(eye, [1.0, 0.0, 0.0])).meets([5.0, 2.0, 0.0], 0.0));
    assert!(frame.traversal.fade.is_empty());

    // Facing away no portal is in the frustum.
    let frame = s.camera_pass(Some(0), &view(eye, [-1.0, 0.0, 0.0]), eye, &thresholds());
    assert_eq!(frame.traversal.visited, [0]);
    // Without a camera sector nothing is visited.
    let frame = s.camera_pass(None, &view(eye, [1.0, 0.0, 0.0]), eye, &thresholds());
    assert_eq!(frame, CameraFrame::default());
}

#[test]
fn a_frustum_through_a_portal() {
    let m = view([0.0; 3], [1.0, 0.0, 0.0]);
    let far = Frustum::view(&m).planes[4];
    // traversal-options.md 5: the edge planes, the portal plane (the side
    // beyond the portal from the eye inside, whichever side the eye is on
    // and whatever the vertex order), and the far plane of the matrix.
    let square = portal(10.0, 0, 1).polygon;
    for eye in [[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]] {
        for poly in [square.clone(), square.iter().rev().copied().collect()] {
            let f = Frustum::through(eye, &poly, &m);
            assert_eq!(f.planes.len(), 6);
            assert_eq!(f.planes[5], far);
            let beyond = [10.0 + (10.0 - eye[0]) * 0.1, 0.0, 0.0];
            let before = [10.0 - (10.0 - eye[0]) * 0.1, 0.0, 0.0];
            assert!(f.meets(beyond, 0.0) && !f.meets(before, 0.0), "{eye:?}");
            assert!(!f.meets([beyond[0], 1.5, 0.0], 0.0));
        }
    }
    // Beyond six vertices the polygon is replaced by its bounding rectangle
    // in its plane, sides along y: an octagon in x = 10 gives the square.
    let octagon: Vec<Vec3> = (0..8)
        .map(|k| {
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            [10.0, a.sin(), a.cos()]
        })
        .collect();
    let f = Frustum::through([0.0; 3], &octagon, &m);
    assert_eq!(f.planes.len(), 6);
    assert!(f.meets([20.0, 1.99, 1.99], 1e-4));
    assert!(!f.meets([20.0, 2.1, 0.0], 0.0));
    // A zero normal becomes (0, 1, 0): the eye on the line of the last
    // edge.
    let f = Frustum::through([10.0, -1.0, -2.0], &square, &m);
    assert_eq!(f.planes[3], [0.0, -1.0, 0.0, -1.0]);
}

#[test]
fn small_portals_fade_and_are_skipped() {
    let s = row();
    let t = thresholds();
    let visited = |x: f32, options| {
        let eye = [x, 0.0, 0.0];
        let m = view(eye, [1.0, 0.0, 0.0]);
        s.traverse(0, Frustum::view(&m), eye, &m, options, &t, |_| false)
    };
    // m = 2 / D²: below T_A beyond 127 m, below T_B beyond 170 m, below
    // T_min beyond 776 m.
    let near = visited(-150.0, CAMERA_PASS);
    assert_eq!(near.visited, [0, 1, 2]);
    assert_eq!(near.fade.iter().map(|f| f.0).collect::<Vec<_>>(), [0, 1]);
    let far = visited(-200.0, CAMERA_PASS);
    assert_eq!(far.visited, [0]);
    assert_eq!(far.fade.len(), 1);
    assert!((far.fade[0].1 - 2.0 / 200.0f32.powi(2)).abs() < 1e-9);
    assert!(visited(-800.0, CAMERA_PASS).fade.is_empty());
    // Without the fade option only T_min applies; without size, nothing.
    let size_only = Options {
        fade: false,
        ..CAMERA_PASS
    };
    let r = visited(-200.0, size_only);
    assert_eq!((r.visited.len(), r.fade.len()), (3, 0));
    assert_eq!(visited(-800.0, size_only).visited, [0]);
    let none = Options {
        occlusion: false,
        size: false,
        fade: false,
    };
    assert_eq!(visited(-800.0, none).visited, [0, 1, 2]);
    // The occlusion hook is asked only with its option.
    let eye = [-3.0, 0.0, 0.0];
    let m = view(eye, [1.0, 0.0, 0.0]);
    let f = Frustum::view(&m);
    assert_eq!(
        s.traverse(0, f.clone(), eye, &m, CAMERA_PASS, &t, |_| true)
            .visited,
        [0]
    );
    assert_eq!(
        s.traverse(0, f, eye, &m, none, &t, |_| true).visited.len(),
        3
    );
}

#[test]
fn portals_into_the_start_sector_are_skipped_unless_touched() {
    // A ring: 0 → 1 → 2 → 0, the last portal back into the start sector.
    let mut s = row();
    s.portals.push(Portal {
        sector_a: 2,
        sector_b: 0,
        ..portal(20.0, 2, 0)
    });
    s.sector_portals = vec![vec![0, 2], vec![0, 1], vec![1, 2]];
    let eye = [-3.0, 0.0, 0.0];
    let m = view(eye, [1.0, 0.0, 0.0]);
    let r = s.traverse(
        0,
        Frustum::view(&m),
        eye,
        &m,
        CAMERA_PASS,
        &thresholds(),
        |_| false,
    );
    assert_eq!(r.visited, [0, 1, 2]);
    assert_eq!(r.frusta[0].len(), 1);

    // A camera just on sector 0's side of portal 0 but in sector 1 (kept
    // from before) enters sector 0 through it, the portal being touched;
    // untouched, sector 0 would be on the camera's own side. The touched
    // portal gives a new frustum like any other: the cone from the eye
    // through the portal beyond its plane, away from sector 0
    // (traversal-options.md 5).
    let s = row();
    let eye = [-0.001, 0.0, 0.0];
    let m = view(eye, [1.0, 0.0, 0.0]);
    let r = s.traverse(
        1,
        Frustum::view(&m),
        eye,
        &m,
        CAMERA_PASS,
        &thresholds(),
        |_| false,
    );
    assert_eq!(r.visited, [1, 0, 2]);
    let f = &r.frusta[0][0];
    assert!(f.meets([1.0, 0.0, 0.0], 0.0));
    assert!(!f.meets([-1.0, 0.0, 0.0], 0.0));
    assert!(!f.meets([1.0, 1.0, 0.0], 0.0));
    let eye = [-0.01, 0.0, 0.0];
    let m = view(eye, [1.0, 0.0, 0.0]);
    let r = s.traverse(
        1,
        Frustum::view(&m),
        eye,
        &m,
        CAMERA_PASS,
        &thresholds(),
        |_| false,
    );
    assert_eq!(r.visited, [1, 2]);
}

#[test]
fn the_camera_sector_comes_from_the_ray_and_is_kept() {
    let s = row();
    // A floor of sector 1 at y = −2 under x 2 … 8, and sector 0's at
    // y = −0.5 under x −8 … −2, beneath portal 0's polygon (it spans
    // y −1 … 1 at x = 0, so no downward ray meets it).
    let quad =
        |x0: f32, x1: f32, y: f32| [[x0, y, -5.0], [x0, y, 5.0], [x1, y, 5.0], [x1, y, -5.0]];
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for (q, sector) in [(quad(2.0, 8.0, -2.0), 1u32), (quad(-8.0, -2.0, -0.5), 0)] {
        let b = vertices.len() as u32;
        vertices.extend(q);
        for t in [[b, b + 1, b + 2], [b, b + 2, b + 3]] {
            triangles.push(CformTriangle {
                vertices: t,
                attributes: (sector << 16) | 1,
            });
        }
    }
    let world = CollisionWorld::new(
        Cform {
            bbox_min: [-8.0, -2.0, -5.0],
            bbox_max: [8.0, -0.5, 5.0],
            vertices,
            triangles,
        },
        test_world::materials(),
    )
    .unwrap();
    let mut camera = CameraSector::default();
    assert_eq!(camera.get(), None);
    // Outside every sector after a load: none.
    assert_eq!(camera.update([50.0, 0.0, 0.0], &s, &world), None);
    assert_eq!(camera.update([5.0, 0.0, 0.0], &s, &world), Some(1));
    assert_eq!(camera.update([-5.0, 0.0, 0.0], &s, &world), Some(0));
    // Then outside again: the last is kept.
    assert_eq!(camera.update([50.0, 0.0, 0.0], &s, &world), Some(0));

    // A horizontal portal 1 m above sector 1's floor is nearer: its side.
    let mut s = row();
    s.portals[0] = Portal {
        sector_a: 2,
        sector_b: 1,
        polygon: vec![
            [2.0, -1.0, -5.0],
            [2.0, -1.0, 5.0],
            [8.0, -1.0, 5.0],
            [8.0, -1.0, -5.0],
        ],
        normal: [0.0, 1.0, 0.0],
        d: 1.0,
        centre: [5.0, -1.0, 0.0],
        radius: 6.0,
    };
    assert_eq!(
        s.sector_below([5.0, 0.0, 0.0], triangle_below(&world, [5.0, 0.0, 0.0])),
        Some(2)
    );
    // Within 10⁻⁵ of the triangle the portal is still taken; nearer by
    // more, the triangle.
    assert_eq!(
        s.sector_below([5.0, 0.0, 0.0], Some((1.0 - 5e-6, 7))),
        Some(2)
    );
    assert_eq!(
        s.sector_below([5.0, 0.0, 0.0], Some((1.0 - 2e-5, 7))),
        Some(7)
    );
}

#[test]
fn billboard_groups_draw_by_their_measure() {
    let mut s = row();
    // Sector 1's root holds a billboard group (6) with the tree 7.
    s.nodes
        .push(node(VisualType::BillboardGroup, [5.0, 0.0, 0.0], vec![7]));
    s.nodes
        .push(node(VisualType::Tree, [5.0, 0.0, 0.0], vec![]));
    s.nodes[4].children = vec![6];
    let t = thresholds();
    let seen = |x: f32| {
        let eye = [x, 0.0, 0.0];
        let m = view(eye, [1.0, 0.0, 0.0]);
        let r = s.traverse(1, Frustum::view(&m), eye, &m, CAMERA_PASS, &t, |_| false);
        s.static_visuals(&r, eye, &t, |_, _| false)[0].clone()
    };
    // σ = 1 / D²: trees above T_B (D < 120 m), the billboard below T_A
    // (D > 90 m), nothing below T_min (D > 549 m).
    assert_eq!(seen(4.0), [7]);
    assert_eq!(seen(-100.0), [6, 7]);
    assert_eq!(seen(-200.0), [6]);
    assert!(seen(-600.0).is_empty());
}
