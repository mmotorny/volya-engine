// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;
use volya_engine_formats::level_ai::{LevelAiHeader, NO_LINK};

const C: f32 = 0.7;

/// A full grid of `columns` × `rows` vertices at height 0, vertex
/// column · rows + row, each linked to the cells beside it except where
/// `cut` (a vertex, a link) says none.
fn grid(columns: u32, rows: u32, cut: &[(u32, usize)]) -> (LevelAi, LevelAiLinks) {
    let header = LevelAiHeader {
        vertex_count: columns * rows,
        cell_size: C,
        height_range: 10.0,
        box_min: [0.0; 3],
        box_max: [C * (columns - 1) as f32, 10.0, C * (rows - 1) as f32],
        identifier: [0; 16],
    };
    assert_eq!(header.row_length(), rows);
    let vertices: Vec<(u32, u16)> = (0..columns * rows).map(|k| (k, 0)).collect();
    let links: Vec<([u32; 4], [u8; 4])> = (0..columns * rows)
        .map(|k| {
            let (c, r) = (k / rows, k % rows);
            let mut l = [
                if c > 0 { k - rows } else { NO_LINK },
                if r + 1 < rows { k + 1 } else { NO_LINK },
                if c + 1 < columns { k + rows } else { NO_LINK },
                if r > 0 { k - 1 } else { NO_LINK },
            ];
            for &(v, i) in cut {
                if v == k {
                    l[i] = NO_LINK;
                }
            }
            (l, [0; 4])
        })
        .collect();
    (LevelAi::new(header, &vertices), LevelAiLinks::new(&links))
}

fn view<'a>(ai: &'a LevelAi, links: &'a LevelAiLinks, acc: &'a Accessibility) -> LevelGraph<'a> {
    LevelGraph {
        ai,
        links,
        accessibility: acc,
    }
}

fn centre(ai: &LevelAi, k: u32) -> [f32; 3] {
    ai.position(k).unwrap()
}

#[test]
fn neighbours_in_link_order() {
    let (ai, links) = grid(3, 3, &[(4, 1)]);
    let acc = Accessibility::default();
    let g = LevelGraph {
        ai: &ai,
        links: &links,
        accessibility: &acc,
    };
    assert_eq!(g.len(), 9);
    assert_eq!(g.neighbours(4).collect::<Vec<_>>(), [1, 7, 3]);
    assert_eq!(g.neighbours(0).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(g.neighbours(9).count(), 0);
    // A link past the last vertex is not a neighbour.
    let bad = LevelAiLinks::new(&[([5, NO_LINK, NO_LINK, NO_LINK], [0; 4])]);
    let g = LevelGraph { links: &bad, ..g };
    assert_eq!(g.neighbours(0).count(), 0);
}

#[test]
fn accessibility_bits() {
    let mut acc = Accessibility::default();
    assert!(acc.is_set(0) && acc.is_set(1_000_000));
    acc.clear(&[3, 130]);
    assert!(!acc.is_set(3) && !acc.is_set(130) && acc.is_set(4) && acc.is_set(131));
    acc.set(&[3, 130, 5_000]);
    assert!(acc.is_set(3) && acc.is_set(130));
    let (ai, links) = grid(2, 2, &[]);
    acc.clear(&[1]);
    let g = LevelGraph {
        ai: &ai,
        links: &links,
        accessibility: &acc,
    };
    assert!(g.accessible(0) && !g.accessible(1) && !g.accessible(4));
}

#[test]
fn crossing_a_cell() {
    let (ai, links) = grid(3, 3, &[]);
    let acc = Accessibility::default();
    let g = LevelGraph {
        ai: &ai,
        links: &links,
        accessibility: &acc,
    };
    // Along +x from vertex 0's centre: the row-0 cells, not row 1's.
    assert!(g.crosses(3, [0.0, 0.0], [1.0, 0.0]));
    assert!(g.crosses(6, [0.0, 0.0], [1.0, 0.0]));
    assert!(!g.crosses(4, [0.0, 0.0], [1.0, 0.0]));
    // The line is unbounded both ways.
    assert!(g.crosses(0, [1.4, 0.0], [1.0, 0.0]));
    // A line on a cell's side, within the widening.
    assert!(g.crosses(1, [0.0, 0.35], [1.0, 0.0]));
    assert!(!g.crosses(1, [0.0, 0.34], [1.0, 0.0]));
    // Diagonal through the corner shared by 0, 1, 3, 4.
    assert!(g.crosses(4, [0.0, 0.0], [1.0, 1.0]));
    assert!(g.crosses(1, [0.0, 0.0], [1.0, 1.0]));
    assert!(!g.crosses(6, [0.0, 0.0], [1.0, 1.0]));
    // No direction crosses nothing.
    assert!(!g.crosses(0, [0.0, 0.0], [0.0, 0.0]));
}

#[test]
fn walk_to_a_point() {
    let (ai, links) = grid(5, 5, &[]);
    let mut acc = Accessibility::default();
    let walk = |acc: &Accessibility, k0, a, b| {
        let g = LevelGraph {
            ai: &ai,
            links: &links,
            accessibility: acc,
        };
        let mut cells = Vec::new();
        let r = g.walk_to_point_visiting(k0, a, b, |k| cells.push(k));
        (r, cells)
    };
    // Straight along +x: 0, 5, 10, 15, 20.
    let (r, cells) = walk(&acc, 0, centre(&ai, 0), centre(&ai, 20));
    assert_eq!((r, cells), (Some(20), vec![0, 5, 10, 15, 20]));
    // A staircase toward column 2, row 2: link order takes +z before +x
    // when both are crossed and not farther.
    let (r, cells) = walk(&acc, 0, centre(&ai, 0), centre(&ai, 12));
    assert_eq!(r, Some(12));
    assert_eq!(cells, [0, 1, 6, 7, 12]);
    // An inaccessible cell on the way ends it; so does the end's own.
    acc.clear(&[10]);
    assert_eq!(walk(&acc, 0, centre(&ai, 0), centre(&ai, 20)).0, None);
    acc.set(&[10]);
    acc.clear(&[20]);
    assert_eq!(walk(&acc, 0, centre(&ai, 0), centre(&ai, 20)).0, None);
    // Off the grid.
    assert_eq!(walk(&acc, 0, centre(&ai, 0), [-1.0, 0.0, 0.0]).0, None);
    // A missing link ends it.
    let (ai2, links2) = grid(5, 5, &[(10, 2)]);
    let g = LevelGraph {
        ai: &ai2,
        links: &links2,
        accessibility: &Accessibility::default(),
    };
    assert_eq!(g.walk_to_point(0, centre(&ai, 0), centre(&ai, 20)), None);
    // The start's own cell is not compared.
    assert_eq!(
        walk(&Accessibility::default(), 0, centre(&ai, 0), centre(&ai, 0)).0,
        None
    );
}

#[test]
fn walk_to_a_vertex() {
    let (ai, links) = grid(5, 5, &[]);
    let mut acc = Accessibility::default();
    let walk = |acc: &Accessibility, k0, a, ke| {
        let mut cells = Vec::new();
        let r = view(&ai, &links, acc).walk_to_vertex_visiting(k0, a, ke, |k| cells.push(k));
        (r, cells)
    };
    // The same staircase as the walk to its centre.
    assert_eq!(
        walk(&acc, 0, centre(&ai, 0), 12),
        (true, vec![0, 1, 6, 7, 12])
    );
    // The line runs from the start point, not the start vertex's centre:
    // from near 0's +x side it goes +x before +z.
    let (r, cells) = walk(&acc, 0, [0.3, 0.0, -0.3], 12);
    assert!(r);
    assert_eq!(cells[1], 5, "{cells:?}");
    // The end vertex inaccessible, or one on the way: false.
    acc.clear(&[12]);
    assert!(!walk(&acc, 0, centre(&ai, 0), 12).0);
    acc.set(&[12]);
    acc.clear(&[10]);
    assert!(!walk(&acc, 0, centre(&ai, 0), 20).0);
    // An invalid end, the start itself, and a missing link: false.
    let open = Accessibility::default();
    assert!(!walk(&open, 0, centre(&ai, 0), 25).0);
    assert!(!walk(&open, 12, centre(&ai, 12), 12).0);
    let (ai2, links2) = grid(5, 5, &[(10, 2)]);
    assert!(!view(&ai2, &links2, &open).walk_to_vertex(0, centre(&ai, 0), 20));
    // No off-grid step: a start point off the grid still walks.
    assert!(walk(&open, 0, [-0.3, 0.0, 0.0], 20).0);
}

#[test]
fn step_test_in_a_direction() {
    let (ai, links) = grid(5, 5, &[(12, 2)]);
    let mut acc = Accessibility::default();
    assert!(view(&ai, &links, &acc).step_test(12, [-3.0, 5.0, 0.0]));
    assert!(view(&ai, &links, &acc).step_test(12, [0.0, 0.0, 1.0]));
    // No link toward +x; the −z neighbour is crossed but farther.
    assert!(!view(&ai, &links, &acc).step_test(12, [1.0, 0.0, 0.0]));
    assert!(!view(&ai, &links, &acc).step_test(12, [0.0, 0.0, 0.0]));
    acc.clear(&[13]);
    assert!(!view(&ai, &links, &acc).step_test(12, [0.0, 0.0, 1.0]));
    // A diagonal: the +z neighbour is crossed and nearer.
    assert!(view(&ai, &links, &Accessibility::default()).step_test(0, [1.0, 0.0, 1.0]));
}

#[test]
fn level_path_search() {
    let (ai, links) = grid(5, 5, &[]);
    let mut acc = Accessibility::default();
    let search = |acc: &Accessibility, s, t, limits| {
        LevelGraph {
            ai: &ai,
            links: &links,
            accessibility: acc,
        }
        .level_path(s, t, limits)
    };
    let all = SearchLimits::CREATURE_MOVEMENT;
    let found = search(&acc, 0, 13, all);
    let path = found.path.unwrap();
    assert_eq!(path.len(), 6, "{path:?}");
    assert_eq!((path[0], path[5]), (0, 13));
    for w in path.windows(2) {
        assert!(links.links(w[0]).unwrap().contains(&w[1]));
    }
    assert!(found.steps >= 5 && found.entries > found.steps);
    assert_eq!(
        search(&acc, 7, 7, all),
        LevelPathSearch {
            path: Some(vec![7]),
            entries: 1,
            steps: 0
        }
    );
    // A wall of inaccessible vertices along column 2 cuts it.
    acc.clear(&[10, 11, 12, 13, 14]);
    let failed = search(&acc, 0, 24, all);
    assert_eq!((failed.path, failed.entries, failed.steps), (None, 10, 10));
    acc.set(&[14]);
    let around = search(&acc, 0, 20, all).path.unwrap();
    assert!(around.contains(&14));
    // The limits.
    acc.set(&[10, 11, 12, 13]);
    let entries = SearchLimits { entries: 3, ..all };
    assert_eq!(search(&acc, 0, 24, entries).path, None);
    let steps = SearchLimits { steps: 2, ..all };
    assert_eq!(search(&acc, 0, 24, steps).steps, 2);
    let length = SearchLimits { length: 1.0, ..all };
    assert_eq!(search(&acc, 0, 24, length).steps, 0);
    assert_eq!(search(&acc, 0, 25, all).path, None);
}

#[test]
fn level_path_step_remembers_the_last_failure() {
    let (ai, links) = grid(3, 1, &[]);
    let mut acc = Accessibility::default();
    acc.clear(&[1]);
    let mut step = LevelPathStep::default();
    let g = LevelGraph {
        ai: &ai,
        links: &links,
        accessibility: &acc,
    };
    assert_eq!(step.search(&g, 0, 2), None);
    let open = Accessibility::default();
    let g = LevelGraph {
        accessibility: &open,
        ..g
    };
    // Same start and goal: no search.
    assert_eq!(step.search(&g, 0, 2), None);
    assert_eq!(step.search(&g, 2, 0), Some(vec![2, 1, 0]));
    // Still remembered: only a later failure replaces it.
    assert_eq!(step.search(&g, 0, 2), None);
    assert_eq!(
        LevelPathStep::default().search(&g, 0, 2),
        Some(vec![0, 1, 2])
    );
}

#[test]
fn nearest_vertex_search() {
    let (ai, links) = grid(7, 7, &[]);
    let mut acc = Accessibility::default();
    let all = SearchLimits::NEAREST_VERTEX;
    let start = 3 * 7 + 3;
    let far = [10.0, 0.0, 10.0];
    // Radius 1.4 m: 2 cells, Δ² ≤ floor(4 + 0.5) = 4: the corner (5, 5)
    // is out, (5, 3) and (4, 4) are in; of those (4, 4) is nearer.
    assert_eq!(
        view(&ai, &links, &acc).nearest_vertex(start, 1.4, far, all),
        Some(4 * 7 + 4)
    );
    // Radius 2.1 m: Δ² ≤ 9: (5, 5) (Δ² 8) is in.
    assert_eq!(
        view(&ai, &links, &acc).nearest_vertex(start, 2.1, far, all),
        Some(5 * 7 + 5)
    );
    // Radius 0: the start only.
    assert_eq!(
        view(&ai, &links, &acc).nearest_vertex(start, 0.0, far, all),
        Some(start)
    );
    // An inaccessible ring keeps it in.
    acc.clear(&[start + 1, start - 1, start + 7, start - 7]);
    assert_eq!(
        view(&ai, &links, &acc).nearest_vertex(start, 2.1, far, all),
        Some(start)
    );
    // One step: the start alone is taken.
    let one = SearchLimits { steps: 1, ..all };
    assert_eq!(
        view(&ai, &links, &Accessibility::default()).nearest_vertex(start, 2.1, far, one),
        Some(start)
    );
    assert_eq!(
        view(&ai, &links, &acc).nearest_vertex(49, 1.0, far, all),
        None
    );
}
