// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The worked example of docs/behavior/character-paths.md section 9 on a
//! New Game of the Russian edition (BYOA): the novice `esc_lager1` at the
//! beginners' village campfire of `l01_escape`, given its campfire job's
//! restrictions as `xr_gulag` gives them, reads its restrictions and
//! accessibility through the scripts and walks to its seat 45,947 under
//! them (its level path searched with its border disabled; placed at the
//! path's end at once by the fake movement, FAKE(#1234)). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::io;
use std::rc::Rc;

use volya_engine::app::{App, AppState};
use volya_engine::audio::Silent;
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::simulation::level_graph::{Accessibility, LevelGraph};
use volya_engine::core::simulation::restrictions::Area;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs, VfsFile};
use volya_engine::data::GameData;
use volya_engine::script::Value;
use volya_engine::script::game_object::{ObjectClass, SpawnData};
use volya_engine::script::loader::Trees;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

/// The game's trees, owned for the test's lifetime.
struct GameTrees {
    tree: DbVfs<Box<dyn VfsFile + 'static>>,
    root: &'static StdVfs,
}

impl Trees for GameTrees {
    fn with_trees(&self, f: &mut dyn FnMut(&dyn Vfs, &dyn Vfs)) -> io::Result<()> {
        f(&self.tree, self.root);
        Ok(())
    }
}

/// `l01_escape`'s 33 default in-restrictors, sorted (9).
fn default_in() -> String {
    let mut names: Vec<String> = (0..10).map(|i| format!("camp_fire_{i:04}")).collect();
    names.extend(
        [
            "esc_in_rest_5",
            "esc_mine_field_1",
            "esc_mine_field_2",
            "esc_soldiers_camp",
            "esc_space_restrictor",
        ]
        .map(String::from),
    );
    names.extend((0..17).map(|i| format!("esc_space_restrictor_{i:04}")));
    names.push("esc_specnaz_camp".into());
    names.sort();
    assert_eq!(names.len(), 33);
    names.join(",")
}

#[test]
fn the_campfire_novice_walks_to_its_seat_under_its_jobs_restrictions() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let mut data = GameData::read(&trees.tree, None, SIZE).unwrap();
    let system = data.system.clone();
    data.measure_intro(&trees.tree).unwrap();
    data.intro_lengths = vec![0.0; data.intro.items.len()];
    let mut app = App::new(data).unwrap();
    app.start_scripts(trees.clone()).unwrap();
    let (mut input, mut audio, mut t) = (Input::new(), Silent, 0.0);
    let mut frames = |app: &mut App, input: &mut Input, n: u32| {
        for _ in 0..n {
            t += 16.0;
            app.update(&trees.tree, t, input, &mut audio);
            input.end_frame();
        }
    };
    let press = |input: &mut Input, key: Key| {
        for pressed in [true, false] {
            input.handle(Event::Key { key, pressed });
        }
    };
    for _ in 0..20 {
        if matches!(app.state(), AppState::Menu { .. }) {
            break;
        }
        press(&mut input, Key::Escape);
        frames(&mut app, &mut input, 1);
    }
    for _ in 0..2 {
        press(&mut input, Key::Enter);
        frames(&mut app, &mut input, 1);
    }
    for _ in 0..400 {
        if matches!(app.state(), AppState::Level { .. }) {
            break;
        }
        frames(&mut app, &mut input, 1);
    }
    assert!(matches!(app.state(), AppState::Level { .. }));
    frames(&mut app, &mut input, 5);

    // The novice, online at its record's level vertex (9).
    let level = app.level().unwrap();
    let sim = level.simulation().unwrap();
    let novice = (sim.objects())
        .find(|o| o.record.header.name == "esc_lager1")
        .expect("esc_lager1");
    assert!(novice.online);
    let (id, record) = (novice.id, novice.record.clone());
    drop(sim);

    // Spawned in the scripts unbound (TODO(#1077)), as in
    // character_placement.rs.
    let scripts = app.game_scripts().unwrap();
    let shared = Rc::clone(scripts.objects());
    assert!(shared.borrow().get(id).is_none());
    {
        // 2.1, 2.2: the registered restrictors' default lists.
        let objects = shared.borrow();
        let r = &objects.restrictors;
        // The 33 of 9, and the type-2 anomaly zones beside them (1.1
        // counts the zone kinds as restrictors; 9 lists only space
        // restrictors: the discrepancy is the spec's to settle).
        let list = String::from_utf8(r.default_in()).unwrap();
        let names: Vec<&str> = list.split(',').collect();
        let example = default_in();
        let spaces: Vec<&str> = (names.iter().copied())
            .filter(|n| !n.starts_with("esc_zone_"))
            .collect();
        assert_eq!(spaces.join(","), example);
        assert_eq!(names.len(), 44, "{list}");
        assert_eq!(r.default_out(), b"");
    }
    let s = scripts.scripts();
    let mut data = SpawnData::from_record(id, &record, &system).unwrap();
    data.server = Some(s.server_object_for(&system, &record).unwrap());
    s.spawn_engine_object_with_binding(ObjectClass::Named("CAI_Stalker"), data, Some(b""))
        .unwrap();
    assert_eq!(shared.borrow().get(id).unwrap().level_vertex, 43_968);
    let expected_in = String::from_utf8(shared.borrow().restrictors.default_in()).unwrap();
    let code = format!(
        r#"local s = level.object_by_id({id})
        assert(s:base_out_restrictions() == "" and s:base_in_restrictions() == "", "own texts")
        assert(s:out_restrictions() == "", "no out-restrictions")
        assert(s:in_restrictions() == "{expected_in}", "the default in-list: " .. s:in_restrictions())
        s:add_restrictions("esc_lager_guard_kill_zone", "")
        assert(s:out_restrictions() == "esc_lager_guard_kill_zone", "the job's out-restriction")
        assert(s:base_out_restrictions() == "esc_lager_guard_kill_zone", "its own")
        assert(s:in_restrictions() == "{expected_in}", "in unchanged")
        assert(s:accessible(43968), "the start")
        assert(s:accessible(45947), "the seat")
        assert(not s:accessible(45944), "the camp centre")
        local centre = level.vertex_position(45944)
        assert(not s:accessible(centre), "the camp centre's position")
        assert(s:accessible(level.vertex_position(45947)), "the seat's position")
        local out = vector()
        local k = s:accessible_nearest(centre, out)
        return k, out.x, out.y, out.z"#
    );
    let got = s.exec("=restrictions", code.as_bytes()).unwrap();
    let n = |v: &Value| match v {
        Value::Number(n) => *n,
        other => panic!("{other:?}"),
    };
    // 9: the camp centre's nearest accessible vertex and its test point.
    assert_eq!(n(&got[0]), 45_942.0);
    let point = [n(&got[1]), n(&got[2]), n(&got[3])];
    for (got, want) in point.iter().zip([-206.851, -20.013, -141.051]) {
        assert!((got - want).abs() < 5e-4, "the test point {point:?}");
    }

    // 9: camp_fire_0004's border, the vertices wholly inside it, and the
    // character's border holding all ten.
    {
        let mut objects = shared.borrow_mut();
        let vertices = objects.level_graph.clone().unwrap();
        let accessibility = Accessibility::default();
        let g = LevelGraph {
            ai: &vertices.ai,
            links: &vertices.links,
            accessibility: &accessibility,
        };
        let fire = objects.restrictors.shape(b"camp_fire_0004").unwrap();
        let area = Area {
            members: vec![fire],
        };
        let border = [
            45_285, 45_286, 45_287, 45_943, 45_945, 46_573, 46_575, 47_200, 47_201, 47_202,
        ];
        assert_eq!(area.border(&g), border);
        assert!(area.wholly_inside(&g, 45_944) && area.wholly_inside(&g, 46_574));
        let (out, inside) = objects.combined_restrictions(id).unwrap();
        let character = objects.restrictors.border(&g, &out, &inside);
        assert!(border.iter().all(|k| character.contains(k)));
    }

    // The walk to the seat (stalker-movement.md 7).
    let code = format!(
        "local s = level.object_by_id({id})\n\
         s:set_dest_level_vertex_id(45947)\n\
         s:set_desired_direction(vector():set(0, 0, -2.1))\n\
         s:set_path_type(game_object.level_path)\n\
         s:set_movement_type(move.walk)\n\
         s:set_mental_state(anim.free)\n\
         s:set_body_state(move.standing)\n\
         s:script(true, 'volya_test')"
    );
    s.exec("=go", code.as_bytes()).unwrap();
    s.fake_stalker_movement(id);
    let objects = shared.borrow();
    let o = objects.get(id).unwrap();
    // 9: the level path, 12 vertices.
    let path = o.movement.fighter.as_ref().unwrap().level_path.clone();
    assert_eq!(
        path.unwrap(),
        [
            43_968, 43_967, 43_966, 43_965, 43_964, 43_963, 43_962, 43_961, 43_960, 44_627, 45_289,
            45_947
        ]
    );
    assert_eq!(o.level_vertex, 45_947);
    let [x, _, z, _] = o.placement[3];
    assert!(
        (x + 207.2).abs() < 0.05 && (z + 137.9).abs() < 0.05,
        "at the seat: {x}, {z}"
    );
}
