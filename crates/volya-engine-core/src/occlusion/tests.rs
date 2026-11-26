// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The worked examples of occlusion-grid.md 9 ("OG") and
//! occlusion-tests.md 9 ("OT"), and level-hom.md 3.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use super::*;

/// VP-spec 2 with fov 90, aspect 1, n 0.2, f 100 and the identity view.
fn vp() -> Mat4 {
    let (n, f) = (0.2f32, 100.0f32);
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, f / (f - n), 1.0],
        [0.0, 0.0, -n * f / (f - n), 0.0],
    ]
}

fn view(frame: u32) -> View {
    View {
        frame,
        view_projection: vp(),
        eye: [0.0; 3],
        menu_shown: false,
    }
}

fn record(v: [Vec3; 3]) -> HomRecord {
    HomRecord {
        vertices: v,
        two_sided: 1,
    }
}

/// OG example 1's square.
fn square() -> Vec<HomRecord> {
    vec![
        record([[-4.3, -3.7, 10.0], [5.1, -3.7, 10.0], [5.1, 4.9, 10.0]]),
        record([[-4.3, -3.7, 10.0], [5.1, 4.9, 10.0], [-4.3, 4.9, 10.0]]),
    ]
}

fn drawn(records: &[HomRecord], frame: u32, state: u32) -> (Occlusion, Generator) {
    let mut o = Occlusion::new(Some(OcclusionMesh::new(records)), 0);
    let mut g = Generator::new(state);
    o.draw(&view(frame), &mut g);
    (o, g)
}

/// The written cells by row.
fn written(o: &Occlusion) -> BTreeMap<usize, Vec<usize>> {
    let mut m = BTreeMap::<usize, Vec<usize>>::new();
    for (k, &v) in o.grid().iter().enumerate() {
        if v < EMPTY {
            m.entry(k / GRID).or_default().push(k % GRID);
        }
    }
    m
}

/// Each range of rows with its range of columns.
fn cells(spec: &[(RangeInclusive<usize>, RangeInclusive<usize>)]) -> BTreeMap<usize, Vec<usize>> {
    let mut m = BTreeMap::new();
    for (rows, cols) in spec {
        for r in rows.clone() {
            m.insert(r, cols.clone().collect());
        }
    }
    m
}

fn at(o: &Occlusion, i: usize, j: usize) -> i32 {
    o.grid()[i * GRID + j]
}

#[test]
fn the_originals_floor_and_ceil() {
    assert_eq!((floor_i(-0.0), ceil_i(-0.0)), (-1, 1));
    assert_eq!((floor_i(0.0), ceil_i(0.0)), (0, 0));
    assert_eq!((floor_i(-0.5), ceil_i(1.5)), (-1, 2));
}

#[test]
fn loading_merges_vertices_and_links_neighbours() {
    // A vertex within 0.01 on every axis takes the first kept one's
    // position; 0.01 itself does not merge.
    let r = [
        record([[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
        record([[1.005, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]]),
        record([[0.0, 1.02, 0.0], [5.0, 5.0, 5.0], [6.0, 5.0, 5.0]]),
    ];
    let m = OcclusionMesh::new(&r);
    assert_eq!(m.vertices.len(), 7);
    assert_eq!(m.triangles[1].vertices, [1, 2, 3]);
    assert_eq!(m.vertices[1], [1.0, 0.0, 0.0]);
    // Edge 1 of triangle 0 (v₁ v₂) is edge 0 of triangle 1.
    assert_eq!(m.triangles[0].neighbours, [None, Some(1), None]);
    assert_eq!(m.triangles[1].neighbours, [Some(0), None, None]);
    // n = normalize((v₀ − v₁) × (v₀ − v₂)), d = −n·v₀, the centre.
    assert_eq!(
        (m.triangles[0].normal, m.triangles[0].d),
        ([0.0, 0.0, 1.0], 0.0)
    );
    assert_eq!(m.triangles[0].centre, [THIRD, THIRD, 0.0]);

    // An edge of three triangles t₁ < t₂ < t₃: t₁ – t₂, t₂ – t₃ (the
    // review's correction).
    let fan = [
        record([[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
        record([[0.0; 3], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]]),
        record([[1.0, 0.0, 0.0], [0.0; 3], [0.0, 0.0, 1.0]]),
    ];
    let m = OcclusionMesh::new(&fan);
    assert_eq!(m.triangles[0].neighbours[0], Some(1));
    assert_eq!(m.triangles[1].neighbours[0], Some(2));
    assert_eq!(m.triangles[2].neighbours[0], Some(1));
}

#[test]
fn og_example_1_a_square_of_two_neighbours() {
    let (o, _) = drawn(&square(), 0, 1);
    assert_eq!((o.writes(0), o.writes(1)), (353, 399));
    let w = written(&o);
    assert_eq!(w, cells(&[(17..=17, 19..=45), (18..=42, 19..=47)]));
    assert_eq!(w.values().map(Vec::len).sum::<usize>(), 752);
    for (r, cols) in &w {
        for &c in cols {
            assert_eq!(at(&o, *r, c), 1_054_375_872, "({r}, {c})");
        }
    }
    assert_eq!(o.coarse(32)[15 * 32 + 15], 1_054_375_872);
    assert_eq!(o.coarse(32)[8 * 32 + 9], EMPTY);
    assert_eq!(o.coarse(32)[8 * 32 + 10], EMPTY);
    let eight: Vec<usize> = (0..64).filter(|&k| o.coarse(8)[k] < EMPTY).collect();
    assert_eq!(
        eight,
        [
            3 * 8 + 3,
            3 * 8 + 4,
            3 * 8 + 5,
            4 * 8 + 3,
            4 * 8 + 4,
            4 * 8 + 5
        ]
    );
}

#[test]
fn og_example_2_without_neighbour_links_a_crack() {
    let mut o = Occlusion::new(Some(OcclusionMesh::new(&square())), 0);
    for t in &mut o.mesh.as_mut().unwrap().triangles {
        t.neighbours = [None; 3];
    }
    o.draw(&view(0), &mut Generator::new(1));
    let w = written(&o);
    assert_eq!(w.values().map(Vec::len).sum::<usize>(), 700);
    assert_eq!(w[&18], (19..=44).chain([47]).collect::<Vec<_>>());
    assert_eq!(w[&30], (19..=31).chain(34..=47).collect::<Vec<_>>());
    assert_eq!(w[&42], (21..=47).collect::<Vec<_>>());
}

#[test]
fn og_example_3_a_hidden_triangle_and_its_skip_count() {
    let mut r = square();
    r.push(record([
        [-2.0, -2.0, 20.0],
        [2.0, -2.0, 20.0],
        [0.0, 2.0, 20.0],
    ]));
    let mut o = Occlusion::new(Some(OcclusionMesh::new(&r)), 0);
    let mut g = Generator::new(12345);
    o.draw(&view(100), &mut g);
    assert_eq!((o.writes(0), o.writes(1), o.writes(2)), (353, 399, 0));
    assert_eq!(
        (o.skip_frame(0), o.skip_frame(1), o.skip_frame(2)),
        (0, 0, 103)
    );
    // Frames 101 and 102: dropped before sorting, no value taken.
    for f in [101, 102] {
        let before = g;
        o.draw(&view(f), &mut g);
        let mut two = before;
        two.next_k();
        two.next_k();
        assert_eq!(g, two);
    }
    assert_eq!((o.skip_frame(0), o.skip_frame(1)), (0, 0));
}

#[test]
fn og_example_4_facing() {
    let one = |v| HomRecord {
        vertices: v,
        two_sided: 0,
    };
    let (o, _) = drawn(
        &[one([
            [-2.0, -2.0, 10.0],
            [2.0, -2.0, 10.0],
            [0.0, 2.0, 10.0],
        ])],
        50,
        7,
    );
    let t = &o.mesh().unwrap().triangles[0];
    assert_eq!((t.normal, t.d), ([0.0, 0.0, 1.0], -10.0));
    assert_eq!((o.writes(0), o.skip_frame(0)), (0, 58));
    let (o, _) = drawn(
        &[one([
            [-2.0, -2.0, 10.0],
            [0.0, 2.0, 10.0],
            [2.0, -2.0, 10.0],
        ])],
        50,
        7,
    );
    let t = &o.mesh().unwrap().triangles[0];
    assert_eq!((t.normal, t.d), ([0.0, 0.0, -1.0], 10.0));
    assert!(o.writes(0) > 0);
    assert_eq!(o.skip_frame(0), 0);
}

#[test]
fn og_example_5_near_clipping() {
    let near = near_plane(&vp());
    assert_eq!(&near[..3], &[0.0, 0.0, -1.0]);
    assert!((near[3] - 0.1001001).abs() < 1e-7, "{}", near[3]);
    let v = [[-1.0, 0.0, 0.05], [1.0, 0.0, 0.05], [0.0, 1.0, 2.0]];
    let mut out = Vec::new();
    clip(&near, &v, &mut out);
    let expected = [
        [0.97431, 0.02569, 0.1001],
        [0.0, 1.0, 2.0],
        [-0.97431, 0.02569, 0.1001],
    ];
    assert_eq!(out.len(), 3);
    for (p, e) in out.iter().zip(expected) {
        assert!((0..3).all(|k| (p[k] - e[k]).abs() < 1e-5), "{p:?}");
    }
    let (o, _) = drawn(&[record(v)], 10, 3);
    assert_eq!(o.writes(0), 396);
    let all: Vec<usize> = (0..64).collect();
    assert_eq!(written(&o), (17..=22).map(|r| (r, all.clone())).collect());
}

#[test]
fn og_example_6_depth_across_a_triangle() {
    let v = [[-3.1, -2.05, 8.0], [4.07, -1.13, 8.0], [0.53, 3.41, 12.0]];
    let s = screen_matrix(&vp());
    let expected = [
        [19.6, 40.2, 0.976954],
        [48.28, 36.52, 0.976954],
        [33.413334, 22.906668, 0.985304],
    ];
    for (p, e) in v.iter().zip(expected) {
        let q = project(&s, *p);
        assert!((0..3).all(|k| (q[k] - e[k]).abs() < 1e-5), "{q:?}");
    }
    let (o, _) = drawn(&[record(v)], 1, 1);
    assert_eq!(o.writes(0), 181);
    let rows: [(usize, std::ops::RangeInclusive<usize>); 15] = [
        (24, 33..=33),
        (25, 32..=34),
        (26, 31..=35),
        (27, 31..=36),
        (28, 30..=37),
        (29, 29..=39),
        (30, 28..=40),
        (31, 27..=41),
        (32, 27..=42),
        (33, 26..=43),
        (34, 25..=44),
        (35, 24..=45),
        (36, 23..=46),
        (37, 23..=35),
        (38, 22..=27),
    ];
    assert_eq!(
        written(&o),
        rows.into_iter().map(|(r, c)| (r, c.collect())).collect()
    );
    for (i, j, e) in [
        (25, 33, 1_056_783_488),
        (30, 28, 1_054_265_792),
        (30, 33, 1_053_895_168),
        (30, 39, 1_053_450_560),
        (35, 33, 1_051_006_848),
        (35, 45, 1_050_117_568),
    ] {
        assert_eq!(at(&o, i, j), e, "({i}, {j})");
    }
}

#[test]
fn og_example_7_the_gap_fill() {
    let p = [
        [1.76, 4.63, 10.0],
        [-2.49, -0.44, 10.0],
        [0.93, -1.8, 10.0],
        [-1.36, -1.87, 10.0],
    ];
    let r = [record([p[0], p[1], p[2]]), record([p[0], p[2], p[3]])];
    let (o, _) = drawn(&r, 0, 1);
    assert_eq!((o.writes(0), o.writes(1)), (46, 46));
    let owner = |i: usize, j: usize| o.buffers.owner[(i + 2) * BUFFER + j + 2];
    assert_eq!((owner(34, 29), owner(35, 29), owner(36, 29)), (0, 0, 1));
    let depth = |i: usize, j: usize| o.buffers.depth[(i + 2) * BUFFER + j + 2];
    assert_eq!(depth(35, 29), (depth(34, 29) + depth(36, 29)) * 0.5);
    let w = written(&o);
    assert_eq!(
        w,
        cells(&[
            (20..=20, 36..=36),
            (21..=21, 35..=36),
            (22..=22, 34..=35),
            (23..=23, 33..=35),
            (24..=25, 32..=35),
            (26..=26, 31..=35),
            (27..=27, 30..=35),
            (28..=28, 29..=35),
            (29..=29, 28..=34),
            (30..=31, 27..=34),
            (32..=33, 26..=34),
            (34..=36, 29..=34),
        ])
    );
    // The only cell filled: 92 written, 93 in the grid.
    assert_eq!(w.values().map(Vec::len).sum::<usize>(), 93);
}

#[test]
fn og_example_8_the_generator() {
    let mut g = Generator::new(0);
    let k: Vec<u32> = (0..5).map(|_| g.next_k()).collect();
    assert_eq!(k, [38, 7719, 21238, 2437, 8855]);
}

#[test]
fn ot_the_threshold_and_cell_edges() {
    // OT 2.1.
    assert_eq!(threshold(0.99999994), 1_073_741_761);
    assert_eq!(threshold(1.0), 1_073_741_825);
    assert_eq!(cell_index(0.49999997 / 64.0), 1);
    // A cell holding a depth one step farther than d = 0.01 does not make
    // it visible; three steps do.
    let d = 0.01f32;
    let code = |s: u32| floor_i(f32::from_bits(d.to_bits() + s) * SCALE);
    assert!(code(1) <= threshold(d));
    assert!(code(3) > threshold(d));
}

/// OG example 1's square drawn in frame 0, with the main menu shown so the
/// tests read the grid as it is.
fn tested() -> (Occlusion, View) {
    let (mut o, _) = drawn(&square(), 0, 1);
    o.on = true;
    (
        o,
        View {
            menu_shown: true,
            ..view(0)
        },
    )
}

#[test]
fn ot_examples_1_to_3_boxes() {
    let (mut o, v) = tested();
    let mut g = Generator::new(1);
    let mut visible = |o: &mut Occlusion, a, b| o.box_visible(a, b, &v, &mut g);
    // Example 1.
    assert!(!visible(&mut o, [-1.0, -1.0, 12.0], [1.0, 1.0, 14.0]));
    assert!(visible(&mut o, [-1.0, -1.0, 8.0], [1.0, 1.0, 9.0]));
    // Example 2.
    assert!(!visible(
        &mut o,
        [-5.025, -3.075, 12.0],
        [5.775, 4.425, 13.0]
    ));
    assert!(visible(&mut o, [-4.425, -3.075, 12.0], [5.85, 4.425, 13.0]));
    // Example 3.
    assert!(visible(&mut o, [-1.0, -1.0, 0.1], [1.0, 1.0, 14.0]));
    // Before the tests' matrix is first built every box is visible.
    let mut fresh = Occlusion::new(Some(OcclusionMesh::new(&square())), 0);
    assert!(visible(&mut fresh, [-1.0, -1.0, 12.0], [1.0, 1.0, 14.0]));
}

#[test]
fn ot_example_4_the_depth_comparison() {
    let (mut o, v) = tested();
    let mut g = Generator::new(1);
    for (value, seen) in [
        (1_057_962_112, false),
        (1_057_962_113, false),
        (1_057_962_114, true),
    ] {
        o.grid.fill(-5);
        o.grid[32 * GRID + 32] = value;
        assert_eq!(
            o.box_visible([-0.1, -0.1, 12.0], [0.1, 0.1, 12.2], &v, &mut g),
            seen,
            "{value}"
        );
    }
}

#[test]
fn ot_examples_5_and_6_portals_and_the_camera_check() {
    let (mut o, v) = tested();
    let mut g = Generator::new(1);
    let quad = |z: f32| {
        [
            [-2.0, -2.0, z],
            [2.0, -2.0, z],
            [2.0, 2.0, z],
            [-2.0, 2.0, z],
        ]
    };
    assert!(o.portal_hidden(&quad(12.0), &v, &mut g));
    assert!(!o.portal_hidden(&quad(8.0), &v, &mut g));
    assert!(!o.portal_hidden(
        &[[-2.0, -2.0, 0.15], [2.0, -2.0, 12.0], [2.0, 2.0, 12.0]],
        &v,
        &mut g
    ));
    // Example 6.
    assert!(!o.box_with_camera_visible([-1.0, -1.0, 12.0], [1.0, 1.0, 14.0], &v, &mut g));
    assert!(o.box_with_camera_visible([-1.0, -1.0, -1.0], [1.0, 1.0, 14.0], &v, &mut g));
    o.on = false;
    assert!(o.box_with_camera_visible([-1.0, -1.0, 12.0], [1.0, 1.0, 14.0], &v, &mut g));
    assert!(!o.portal_hidden(&quad(12.0), &v, &mut g));
}

#[test]
fn ot_example_7_the_cache() {
    let (mut o, v) = tested();
    let mut g = Generator::new(12345);
    let (front, behind) = (
        ([-1.0, -1.0, 8.0], [1.0, 1.0, 9.0]),
        ([-1.0, -1.0, 12.0], [1.0, 1.0, 14.0]),
    );
    let mut c = Cached::default();
    let mut test = |o: &mut Occlusion, c: &mut Cached, f: u32, (a, b): (Vec3, Vec3)| {
        o.cached_visible(c, a, b, &View { frame: f, ..v }, &mut g)
    };
    assert!(test(&mut o, &mut c, 100, front));
    assert_eq!(
        c,
        Cached {
            next: 119,
            tested: 100
        }
    );
    for f in 101..=118 {
        assert!(test(&mut o, &mut c, f, behind));
    }
    assert_eq!(c.next, 119);
    assert!(!test(&mut o, &mut c, 119, behind));
    assert_eq!(
        c,
        Cached {
            next: 120,
            tested: 119
        }
    );
    assert!(test(&mut o, &mut c, 119, behind));
    assert!(test(&mut o, &mut c, 120, front));
    assert_eq!(
        c,
        Cached {
            next: 139,
            tested: 120
        }
    );
    // Off: visible, nothing stored.
    o.on = false;
    assert!(test(&mut o, &mut c, 139, behind));
    assert_eq!(c.next, 139);
}

#[test]
fn a_test_before_the_frames_drawing_draws_the_grid() {
    let (mut o, _) = drawn(&square(), 0, 1);
    o.on = true;
    let mut g = Generator::new(1);
    let mut v = view(5);
    v.menu_shown = true;
    o.box_visible([-1.0, -1.0, 8.0], [1.0, 1.0, 9.0], &v, &mut g);
    assert_eq!(o.drawn, Some(0));
    v.menu_shown = false;
    o.box_visible([-1.0, -1.0, 8.0], [1.0, 1.0, 9.0], &v, &mut g);
    assert_eq!(o.drawn, Some(5));
    // Without a mesh nothing is drawn and nothing is hidden.
    let mut none = Occlusion::new(None, 1);
    none.camera_pass(&v, &mut g);
    assert!(!none.on && none.drawn.is_none());
    assert!(!none.visual_hidden(0, [-1.0, -1.0, 12.0], [1.0, 1.0, 14.0], &v, &mut g));
}

#[test]
fn occlusion_is_off_during_the_light_passes() {
    // OT 4: no cached test hides anything or changes its cache then.
    let (mut o, mut g) = drawn(&square(), 1, 12345);
    o.on = true;
    let v = view(1);
    let (a, b) = ([-1.0, -1.0, 12.0], [1.0, 1.0, 14.0]);
    let mut c = Cached::default();
    let hidden = o.light_passes(|o| !o.cached_visible(&mut c, a, b, &v, &mut g));
    assert!(!hidden && c == Cached::default());
    // Back on after them: the box behind the square is hidden.
    assert!(o.on && !o.cached_visible(&mut c, a, b, &v, &mut g));
}
