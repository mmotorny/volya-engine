// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The online stalkers and monsters of a New Game of the Russian edition
//! (BYOA) drawn where their engine objects are (sav.md 7.4.1, Creature
//! update; script API M 4): an `esc_lager` campfire stalker spawned in the
//! scripts, which send him to the campfire seat 45,947 (stalker-movement.md 7; placed there
//! at once by the fake movement, FAKE(#1234)) is drawn at the seat, the
//! items shown on him with him, while the trader stays drawn at his
//! record's transform with its online height (object-visual-and-transform.md
//! 4.1). Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::io;
use std::rc::Rc;

use volya_engine::app::{App, AppState};
use volya_engine::audio::Silent;
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs, VfsFile};
use volya_engine::data::GameData;
use volya_engine::script::Value;
use volya_engine::script::game_object::{Kinds, ObjectClass, SpawnData};
use volya_engine::script::loader::Trees;
use volya_engine_formats::model::{Mat4, transform};
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

#[test]
fn a_stalker_the_scripts_move_is_drawn_where_his_engine_object_is() {
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

    // An `esc_lager` stalker online with a model, drawn where he spawned.
    let level = app.level().unwrap();
    let sim = level.simulation().unwrap();
    let record_transform = |id: u16| {
        let h = &sim.object(id).unwrap().record.header;
        transform(h.position, h.angles)
    };
    let (stalker, k) = (level.world().objects.instances.iter())
        .filter(|i| i.name.starts_with("esc_lager"))
        .find_map(|i| {
            let k = level.instance_of(i.id)?;
            (sim.object(i.id)?.online && level.placement(k).is_some()).then_some((i.id, k))
        })
        .expect("an esc_lager stalker drawn");
    let spawned = record_transform(stalker);
    let record = sim.object(stalker).unwrap().record.clone();
    let trader = (level.world().objects.instances.iter())
        .find(|i| i.name == "escape_trader")
        .unwrap()
        .id;
    let trader_at = record_transform(trader);
    drop(sim);

    // Spawned in the scripts unbound: the engine spawns no stalker in the
    // scripts yet (TODO(#1077)), and his think step can't run in a level
    // yet (TODO(#1205)). The scripts send him to the campfire seat
    // (stalker-movement.md 7), and the fake movement at his think step's
    // end places him there (FAKE(#1234)).
    let scripts = app.game_scripts().unwrap();
    let spawned_already = scripts.objects().borrow().get(stalker).is_some();
    assert!(!spawned_already, "spawned in the scripts already");
    let s = scripts.scripts();
    let mut data = SpawnData::from_record(stalker, &record, &system).unwrap();
    data.server = Some(s.server_object_for(&system, &record).unwrap());
    let class = ObjectClass::Named("CAI_Stalker");
    s.spawn_engine_object_with_binding(class, data, Some(b""))
        .unwrap();
    let code = format!(
        "local s = level.object_by_id({stalker})\n\
         s:set_dest_level_vertex_id(45947)\n\
         s:set_desired_direction(vector():set(0, 0, -2.1))\n\
         s:set_path_type(game_object.level_path)\n\
         s:set_movement_type(move.walk)\n\
         s:set_mental_state(anim.free)\n\
         s:set_body_state(move.standing)"
    );
    s.exec("=go", code.as_bytes()).unwrap();
    // Under script control (script API V 2.2) with an empty queue, his
    // scheduled updates leave him be.
    let code = format!(
        "local s = level.object_by_id({stalker})\n\
         s:script(true, 'volya_test')\n\
         return s:get_script()"
    );
    let got = s.exec("=control", code.as_bytes()).unwrap();
    assert_eq!(got, [Value::Boolean(true)], "under script control");
    s.fake_stalker_movement(stalker);
    // Drawn where he spawned until the frame.
    assert_eq!(app.level().unwrap().placement(k), Some(spawned));
    frames(&mut app, &mut input, 2);
    if let AppState::Menu { menu, .. } = app.state() {
        panic!("left the level: {:?}", menu.error);
    }

    // Every online stalker and monster with a drawn instance is drawn
    // where its engine object is.
    let scripts = app.game_scripts().unwrap();
    let engine: Vec<(u16, Mat4)> = (scripts.objects().borrow().iter())
        .filter(|o| o.parent.is_none())
        .filter(|o| o.kinds.has(Kinds::STALKER) || o.kinds.has(Kinds::MONSTER))
        .map(|o| (o.id, o.placement))
        .collect();
    let level = app.level().unwrap();
    let mut drawn = 0;
    for &(id, at) in &engine {
        if let Some(p) = level.instance_of(id).and_then(|k| level.placement(k)) {
            assert_eq!(p, at, "object {id}");
            drawn += 1;
        }
    }
    eprintln!("{drawn} stalkers and monsters drawn");
    let at = engine.iter().find(|(id, _)| *id == stalker).unwrap().1;
    // He was moved away from where he spawned, and is drawn there.
    assert_ne!(at, spawned, "the stalker never moved");
    assert_eq!(level.placement(k), Some(at));
    let [x, _, z, _] = at[3];
    assert!(
        (x + 207.2).abs() < 0.05 && (z + 137.9).abs() < 0.05,
        "at the seat: {x}, {z}"
    );
    // The items shown on him are with him.
    let items = level.attached_items().items();
    for a in level.attached_items().attached(stalker) {
        let p = level.placement(items[&a.item].instance);
        let [ix, _, iz, _] = p.expect("an item shown")[3];
        assert!((ix - x).hypot(iz - z) < 2.0, "item at {ix}, {iz}");
    }
    // The trader, no stalker, stays at his record's transform, his height
    // corrected as he came online (object-visual-and-transform.md 4.5).
    let k = level.instance_of(trader).unwrap();
    assert_eq!(level.placement(k), Some(trader_at));
    // Within the direction's length factor left open (#1170).
    let y = trader_at[3][1];
    assert!(
        (y + 24.864_836).abs() <= 2e-6,
        "the trader's online height {y}"
    );
}
