// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;
use crate::simulation::level_graph::Accessibility;
use volya_engine_formats::level_ai::{LevelAiHeader, LevelAiLinks, NO_LINK};

const C: f32 = 0.7;

/// A full flat grid of `columns` × `rows` vertices at height 0, vertex
/// column · rows + row, at x = 0.7 · column, z = 0.7 · row.
fn grid(columns: u32, rows: u32) -> (LevelAi, LevelAiLinks) {
    let header = LevelAiHeader {
        vertex_count: columns * rows,
        cell_size: C,
        height_range: 10.0,
        box_min: [0.0; 3],
        box_max: [C * (columns - 1) as f32, 10.0, C * (rows - 1) as f32],
        identifier: [0; 16],
    };
    let vertices: Vec<(u32, u16)> = (0..columns * rows).map(|k| (k, 0)).collect();
    let links: Vec<([u32; 4], [u8; 4])> = (0..columns * rows)
        .map(|k| {
            let (c, r) = (k / rows, k % rows);
            let l = [
                if c > 0 { k - rows } else { NO_LINK },
                if r + 1 < rows { k + 1 } else { NO_LINK },
                if c + 1 < columns { k + rows } else { NO_LINK },
                if r > 0 { k - 1 } else { NO_LINK },
            ];
            (l, [0; 4])
        })
        .collect();
    (LevelAi::new(header, &vertices), LevelAiLinks::new(&links))
}

/// An axis-aligned box restrictor spanning x0…x1, z0…z1, y −5…5.
fn boxed(x0: f32, x1: f32, z0: f32, z1: f32) -> RestrictorShape {
    let (cx, cz) = ((x0 + x1) / 2.0, (z0 + z1) / 2.0);
    RestrictorShape::placed(
        &[Shape::Box([
            [x1 - x0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, z1 - z0],
            [0.0; 3],
        ])],
        [cx, 0.0, cz],
        [0.0; 3],
    )
}

fn vertex(rows: u32, c: u32, r: u32) -> u32 {
    c * rows + r
}

#[test]
fn texts() {
    assert_eq!(union(b"a,b", b" c , a"), b"a,b,c");
    assert_eq!(union(b"", b"x"), b"x");
    assert_eq!(difference(b"a, b,c", b"b"), b"a,c");
    assert_eq!(normalised(b"c,a,b,a"), b"a,a,b,c");
    assert_eq!(normalised(b" z "), b"z");
    // A character exempts itself from a default by naming it in the other
    // text (2.2).
    let (out, inside) = combined(b"k", b"m", b"d,m", b"f,e,k");
    assert_eq!(
        (out.as_slice(), inside.as_slice()),
        (&b"d,k"[..], &b"e,f,m"[..])
    );
    assert!(registered(0) && registered(1) && registered(2) && !registered(3));
}

#[test]
fn shapes_meet_spheres() {
    let s = RestrictorShape::placed(
        &[Shape::Sphere {
            centre: [1.0, 0.0, 0.0],
            radius: 2.0,
        }],
        [10.0, 0.0, 0.0],
        [0.0; 3],
    );
    assert!(s.meets([13.0, 0.0, 0.0], 0.5));
    assert!(!s.meets([13.5, 0.0, 0.0], 0.5));
    assert_eq!(s.elements[0].low, [9.0, -2.0]);
    let b = boxed(0.0, 2.0, 0.0, 4.0);
    assert!(b.meets([1.0, 0.0, 2.0], 0.0));
    assert!(b.meets([2.1, 0.0, 2.0], 0.2));
    assert!(!b.meets([2.1, 0.0, 2.0], 0.05));
    assert_eq!(
        (b.elements[0].low, b.elements[0].high),
        ([0.0, 0.0], [2.0, 4.0])
    );
    // A box turned a quarter about y spans the other way.
    let t = RestrictorShape::placed(
        &[Shape::Box([
            [2.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 4.0],
            [0.0; 3],
        ])],
        [0.0; 3],
        [0.0, std::f32::consts::FRAC_PI_2, 0.0],
    );
    let (low, high) = (t.elements[0].low, t.elements[0].high);
    assert!(
        (low[0] + 2.0).abs() < 1e-5 && (high[1] - 1.0).abs() < 1e-5,
        "{low:?} {high:?}"
    );
    assert!(t.meets([1.9, 0.0, 0.0], 0.0) && !t.meets([0.0, 0.0, 1.9], 0.0));
}

/// A 10 × 10 grid with an out-restrictor holding the whole cells of
/// columns and rows 2 … 7 and nothing of the others, and an
/// in-restrictor holding the centre of cell (5, 5) and of its +x
/// neighbour.
#[test]
fn accessibility_and_borders() {
    let (ai, links) = grid(10, 10);
    let acc = Accessibility::default();
    let g = LevelGraph {
        ai: &ai,
        links: &links,
        accessibility: &acc,
    };
    let out = boxed(1.0515, 5.2495, 1.0515, 5.2495);
    let fire = boxed(3.3, 4.2, 3.3, 3.7);
    let shape = |name: &[u8]| match name {
        b"zone" => Some(&out),
        b"fire" => Some(&fire),
        _ => None,
    };
    let v = |c, r| vertex(10, c, r);

    // Not initialised while a name isn't registered (3.5).
    let r = Restrictions::of(b"zone,missing", b"", shape);
    assert!(!r.initialised && r.accessible_vertex(&g, v(0, 0)) && r.border(&g).is_empty());

    let r = Restrictions::of(b"zone", b"fire", shape);
    assert!(r.initialised);
    // 3.3.
    assert!(r.accessible_vertex(&g, v(2, 2)) && r.accessible_vertex(&g, v(7, 7)));
    assert!(!r.accessible_vertex(&g, v(1, 4)), "outside the out area");
    assert!(!r.accessible_vertex(&g, v(5, 5)) && !r.accessible_vertex(&g, v(6, 5)));
    assert!(
        r.accessible_vertex(&g, v(4, 5)),
        "touching nothing of the fire"
    );
    assert!(!r.accessible_vertex(&g, 100), "invalid");

    // 3.6: the out area's whole cells have no partly-inside vertex, so
    // only the fire's border is the character's.
    let fire_area = Area {
        members: vec![&fire],
    };
    assert_eq!(fire_area.border(&g), [v(5, 5), v(6, 5)]);
    assert!(!fire_area.wholly_inside(&g, v(5, 5)) && fire_area.partly_inside(&g, v(5, 5)));
    assert_eq!(r.border(&g), [v(5, 5), v(6, 5)]);
    let small = boxed(1.4, 5.2495, 1.0515, 5.2495);
    let r2 = Restrictions::of(b"small", b"", |n: &[u8]| (n == b"small").then_some(&small));
    let border = r2.border(&g);
    assert_eq!(border, (2..=7).map(|r| v(2, r)).collect::<Vec<_>>());

    // 3.4.
    assert!(r.accessible_position(&g, [2.1, 0.0, 2.1]));
    assert!(!r.accessible_position(&g, [0.3, 0.0, 2.1]), "outside");
    assert!(!r.accessible_position(&g, [3.5, 0.0, 3.5]), "in the fire");
    assert!(
        !r.accessible_position(&g, [4.5, 0.0, 3.5]),
        "in a border vertex's cell column"
    );
    assert!(r.accessible_position(&g, [4.5, 3.0, 3.5]), "3 m above it");
    assert!(!r.accessible_position(&g, [50.0, 0.0, 2.0]), "off the grid");
    let none = Restrictions::of(b"", b"", shape);
    assert!(none.initialised && !none.any());
    assert!(none.accessible_position(&g, [50.0, 0.0, 2.0]));

    // 4: the border vertex nearest a point of the fire, (5, 5); its
    // accessible link nearest the point, (4, 5); that link's test point
    // nearest it.
    let border = r.border(&g);
    let (k, p) = r.accessible_nearest(&g, &border, [3.4, 0.0, 3.45]).unwrap();
    assert_eq!(k, v(4, 5));
    assert!(
        (p[0] - 3.149).abs() < 1e-4 && (p[2] - 3.151).abs() < 1e-4,
        "{p:?}"
    );
    assert_eq!(none.accessible_nearest(&g, &[], [0.0; 3]), None);

    // 6.1: an inaccessible destination is replaced; an inaccessible end
    // position becomes the vertex's position.
    let d = r
        .accessible_destination(&g, &border, v(5, 5), [0.0; 3])
        .unwrap();
    let nearest = r.accessible_nearest(&g, &border, ai.position(v(5, 5)).unwrap());
    assert_eq!(Some((d.vertex, d.end_position)), nearest);
    assert_ne!(d.vertex, v(5, 5));
    let d = r
        .accessible_destination(&g, &border, v(3, 3), [0.3, 0.0, 0.3])
        .unwrap();
    assert_eq!(
        (d.vertex, d.end_position),
        (v(3, 3), ai.position(v(3, 3)).unwrap())
    );

    // 5: from an accessible start the border is disabled; from outside,
    // nothing is.
    let mut step = LevelPathStep::default();
    let path = r
        .level_path(&g, &border, &mut step, v(6, 3), v(6, 7))
        .unwrap();
    assert!(!path.contains(&v(6, 5)), "{path:?}");
    assert_eq!(path.len(), 7, "around the border vertex: {path:?}");
    let mut step = LevelPathStep::default();
    let path = r
        .level_path(&g, &border, &mut step, v(5, 5), v(6, 7))
        .unwrap();
    assert_eq!(path.len(), 4, "{path:?}");
    // A border that cuts the goal off fails, and is remembered.
    let mut step = LevelPathStep::default();
    assert_eq!(r.level_path(&g, &border, &mut step, v(6, 3), v(6, 5)), None);
}
