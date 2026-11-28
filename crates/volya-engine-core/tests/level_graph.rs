// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The worked example of docs/behavior/level-graph.md section 7 on the
//! Russian edition (BYOA): the campfire novice `esc_lager1` of
//! `l01_escape` and its way to its seat over the level graph, with the
//! measured properties of 1.3 for that level. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine_core::simulation::level_graph::{LevelPathStep, SearchLimits};
use volya_engine_core::simulation::navigation::Navigation;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, read};
use volya_engine_formats::game_graph::GameGraph;

/// `l01_escape`'s navigation data, or `None` without the game data.
fn escape() -> Option<Navigation> {
    let archives = gog_archives::archives()?;
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let graph = GameGraph::parse(&read(&tree, "gamedata/game.graph").unwrap()).unwrap();
    Some(Navigation::read(&tree, "gamedata\\levels\\l01_escape", &graph, 1).unwrap())
}

/// The novice's position and its record's level vertex (7).
const NOVICE: [f32; 3] = [-209.067, -19.751, -132.083];
const NOVICE_VERTEX: u32 = 43_968;
/// The seat's vertex.
const SEAT: u32 = 45_947;

#[test]
fn the_novices_way_to_its_seat() {
    let Some(nav) = escape() else {
        return;
    };
    let g = nav.graph().unwrap();
    let ai = &nav.ai;
    let h = &ai.header;
    assert_eq!(
        (g.len(), h.cell_size, h.row_length(), h.column_count()),
        (595_499, 0.7, 1_697, 994)
    );
    // 1: its vertex, kept as it comes online.
    assert_eq!(ai.packed_at(NOVICE), Some(106 * 1_697 + 498));
    let v = ai.position(NOVICE_VERTEX).unwrap();
    assert!(
        (v[0] + 209.3).abs() < 1e-3 && (v[1] + 19.738).abs() < 1e-3 && (v[2] + 132.3).abs() < 1e-3,
        "{v:?}"
    );
    assert_eq!(
        nav.vertex_for_point(NOVICE_VERTEX, NOVICE),
        Some(NOVICE_VERTEX)
    );
    let (k, _) = nav.location(NOVICE_VERTEX, NOVICE).unwrap();
    assert_eq!(k, NOVICE_VERTEX);
    // 2: its neighbours, cover values and light.
    assert_eq!(
        g.neighbours(NOVICE_VERTEX).collect::<Vec<_>>(),
        [43_290, 43_969, 44_635, 43_967]
    );
    let links = nav.links.as_ref().unwrap();
    assert_eq!(links.cover(NOVICE_VERTEX), Some([3, 4, 4, 4]));
    assert_eq!(nav.vertex_lights[NOVICE_VERTEX as usize] >> 4, 0);
    // 3: the level path to the seat.
    let p = ai.packed(SEAT).unwrap();
    assert_eq!((p / 1_697, p % 1_697), (109, 490));
    let search = g.level_path(NOVICE_VERTEX, SEAT, SearchLimits::CREATURE_MOVEMENT);
    assert_eq!((search.entries, search.steps), (36, 13));
    assert_eq!(
        search.path.unwrap(),
        [
            43_968, 43_967, 43_966, 43_965, 43_964, 43_963, 43_962, 43_961, 43_960, 44_627, 45_289,
            45_947
        ]
    );
    let mut step = LevelPathStep::default();
    for (seat, len) in [
        (47_843, 18),
        (47_203, 15),
        (44_626, 11),
        (43_957, 12),
        (44_622, 15),
        (45_941, 18),
        (47_199, 19),
    ] {
        let path = step.search(&g, NOVICE_VERTEX, seat).unwrap();
        assert_eq!(path.len(), len, "to {seat}");
    }
    // 4: the straight walk to the seat's centre.
    let centre = ai.position(SEAT).unwrap();
    assert!((centre[0] + 207.2).abs() < 1e-3 && (centre[2] + 137.9).abs() < 1e-3);
    let mut cells = Vec::new();
    let reached = g.walk_to_point_visiting(NOVICE_VERTEX, NOVICE, centre, |k| cells.push(k));
    assert_eq!(reached, Some(SEAT));
    assert_eq!(
        cells,
        [
            43_968, 44_635, 44_634, 44_633, 44_632, 45_294, 45_293, 45_292, 45_291, 45_949, 45_948,
            45_947
        ]
    );
    // 4: the walk to the seat's vertex, from the stalker's position and
    // from its vertex's centre.
    let mut cells = Vec::new();
    assert!(g.walk_to_vertex_visiting(NOVICE_VERTEX, NOVICE, SEAT, |k| cells.push(k)));
    assert_eq!(
        cells,
        [
            43_968, 44_635, 44_634, 44_633, 44_632, 45_294, 45_293, 45_292, 45_291, 45_949, 45_948,
            45_947
        ]
    );
    cells.clear();
    let start = ai.position(NOVICE_VERTEX).unwrap();
    assert!(g.walk_to_vertex_visiting(NOVICE_VERTEX, start, SEAT, |k| cells.push(k)));
    assert_eq!(
        cells,
        [
            43_968, 43_967, 44_634, 44_633, 44_632, 44_631, 45_293, 45_292, 45_291, 45_290, 45_948,
            45_947
        ]
    );
}

/// 1.3 on `l01_escape`: every link that names a vertex is valid, not the
/// vertex itself, one step in its direction; 396 have no link back; every
/// vertex has a link.
#[test]
fn escapes_links() {
    let Some(nav) = escape() else {
        return;
    };
    let g = nav.graph().unwrap();
    let links = nav.links.as_ref().unwrap();
    let r = nav.ai.header.row_length();
    let step: [(i64, i64); 4] = [(-1, 0), (0, 1), (1, 0), (0, -1)];
    let (mut one_way, mut alone) = (0, 0);
    for k in 0..g.len() {
        let l = links.links(k).unwrap();
        let p = i64::from(nav.ai.packed(k).unwrap());
        let mut any = false;
        for (i, &to) in l.iter().enumerate() {
            if to == volya_engine_formats::level_ai::NO_LINK {
                continue;
            }
            any = true;
            assert!(g.valid(to) && to != k, "{k} → {to}");
            let q = i64::from(nav.ai.packed(to).unwrap());
            let r = i64::from(r);
            assert_eq!((q / r - p / r, q % r - p % r), step[i], "{k} → {to}");
            if links.links(to).unwrap()[(i + 2) % 4] != k {
                one_way += 1;
            }
        }
        if !any {
            alone += 1;
        }
    }
    assert_eq!((one_way, alone), (396, 0));
    assert_eq!(g.neighbours(0).count(), 2);
}
