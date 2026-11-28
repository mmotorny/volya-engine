// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Items shown on their holders from the Russian edition's data (BYOA),
//! docs/behavior/character-items.md section 8 on a New Game's
//! `l01_escape`: a novice of the beginners' village holding his guitar
//! (its worked example, section 11: the offset *O*, the left hand), drawn
//! at *O* · *B* · *X* as his bones move and hidden again; and through the
//! shipped scripts, the trader's item enabled by `enable_attachable_item`
//! shown nowhere, as the trader accepts nothing (8.3). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::HashMap;
use std::io;
use std::rc::Rc;

use volya_engine::app::{App, AppState};
use volya_engine::attached_items::Holdings;
use volya_engine::audio::Silent;
use volya_engine::core::character::inventory::Place;
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::torch::multiply;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs, VfsFile};
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::script::Value;
use volya_engine::script::loader::Trees;
use volya_engine_formats::model::Mat4;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

/// The guitar's offset O, rounded (character-items.md 11 step 4).
const GUITAR_OFFSET: [[f32; 3]; 4] = [
    [-0.9341, -0.2116, -0.2876],
    [0.2763, 0.0821, -0.9576],
    [0.2262, -0.9739, -0.0182],
    [0.077492, -0.030020, 0.090059],
];

/// Holdings over a table: item → (holder, place, enabled).
#[derive(Default)]
struct Table(HashMap<u16, (u16, Place, bool)>);

impl Holdings for Table {
    fn held(&self, item: u16) -> Option<(u16, Place)> {
        self.0.get(&item).map(|&(h, p, _)| (h, p))
    }
    fn enabled(&self, item: u16) -> bool {
        self.0.get(&item).is_some_and(|e| e.2)
    }
    fn dropped(&self, _: u16) -> bool {
        false
    }
    fn disable(&mut self, item: u16) {
        if let Some(e) = self.0.get_mut(&item) {
            e.2 = false;
        }
    }
}

#[track_caller]
fn near(got: &Mat4, want: &Mat4, tolerance: f32, what: &str) {
    let close = (0..4).all(|r| (0..4).all(|c| (got[r][c] - want[r][c]).abs() <= tolerance));
    assert!(close, "{what}: {got:?} != {want:?}");
}

#[test]
fn a_novice_holds_his_guitar_on_his_left_hand() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut screen = LevelScreen::load(&tree, &data, 0).unwrap();

    // A novice of the village (esc_lager1 … 7) with a guitar, online with
    // a model (section 11).
    let objects = &screen.world().objects;
    let sim = screen.simulation().unwrap();
    let found = (screen.attached_items().items().iter())
        .filter(|(_, i)| i.section == "guitar_a")
        .find_map(|(&id, i)| {
            let holder = sim.object(sim.object(id)?.record.header.parent_id)?;
            let name = holder.record.header.name.to_string();
            let k = screen.instance_of(holder.id)?;
            (name.starts_with("esc_lager") && screen.placement(k).is_some())
                .then(|| (id, i.clone(), holder.id, name, k))
        });
    let (guitar, item, novice, name, k) = found.expect("a novice of the village with a guitar");
    let model = &objects.models[objects.instances[k].model];
    let hand = (0..model.bone_count())
        .find(|&b| objects.names.get(model.bone_name(b)) == "bip01_l_hand")
        .expect("the left hand");
    for (r, row) in GUITAR_OFFSET.iter().enumerate() {
        for (c, &want) in row.iter().enumerate() {
            let got = item.attachment.offset[r][c];
            assert!((got - want).abs() < 1e-4, "O[{r}][{c}] = {got}");
        }
    }
    // Its own model, the section's `visual` (8.5).
    let path = objects
        .names
        .get(objects.models[objects.instances[item.instance].model].path);
    assert!(
        path.to_ascii_lowercase()
            .ends_with("equipments\\item_guitar.ogf"),
        "{path}"
    );
    drop(sim);

    // Disabled in his backpack (step 1): not shown.
    let mut table = Table::default();
    table.0.insert(guitar, (novice, Place::Backpack, false));
    screen.attach_items(&[], &mut table);
    assert_eq!(screen.placement(item.instance), None);
    // Enabled by a script (step 3): attached to his left hand, drawn at
    // O · B · X, B at rest while he plays no motion.
    table.0.get_mut(&guitar).unwrap().2 = true;
    screen.attach_items(&[(guitar, novice, true)], &mut table);
    let x = screen.placement(k).unwrap();
    let objects = &screen.world().objects;
    let rest = objects.models[objects.instances[k].model].bind(hand);
    let at_rest = multiply(&multiply(&item.attachment.offset, &rest), &x);
    near(
        &screen.placement(item.instance).unwrap(),
        &at_rest,
        1e-5,
        "at rest",
    );

    // As his bones move, the guitar follows the hand.
    assert!(screen.play_cycle(&tree, &data, &name, "guitar_3").unwrap());
    let mut now = 0;
    let mut placed = Vec::new();
    for _ in 0..3 {
        for _ in 0..15 {
            now += 20;
            screen.set_global_clock(now);
            screen.update(now, &Default::default());
        }
        let b = screen.motions().bone_poses(k).unwrap()[hand];
        let want = multiply(&multiply(&item.attachment.offset, &b), &x);
        let got = screen.placement(item.instance).unwrap();
        near(&got, &want, 1e-5, "playing");
        placed.push(got);
    }
    assert!(
        placed.iter().any(|p| *p != at_rest) && placed.windows(2).any(|w| w[0] != w[1]),
        "the guitar never moved"
    );

    // Disabled again (step 5): hidden.
    table.0.get_mut(&guitar).unwrap().2 = false;
    screen.attach_items(&[(guitar, novice, false)], &mut table);
    assert_eq!(screen.placement(item.instance), None);
    assert!(screen.attached_items().attached(novice).is_empty());
}

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
fn the_trader_shows_no_item_a_script_enables() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let mut data = GameData::read(&trees.tree, None, SIZE).unwrap();
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

    let level = app.level().unwrap();
    let trader = (level.world().objects.instances.iter())
        .find(|i| i.name == "escape_trader")
        .unwrap()
        .id;
    // Its first item enabled (script API S 8.1): the flag set, the
    // attachment asked of the engine.
    let code = format!(
        "local t = level.object_by_id({trader})
         local first
         t:iterate_inventory(function(_, item) first = first or item end, t)
         first:enable_attachable_item(true)
         return first:id(), first:attachable_item_enabled()"
    );
    let scripts = app.game_scripts().unwrap();
    let r = scripts.scripts().exec("=enable", code.as_bytes()).unwrap();
    assert_eq!(r[1], Value::Boolean(true));
    frames(&mut app, &mut input, 2);
    // The trader accepts nothing (8.3): nothing attached, nothing shown.
    let level = app.level().unwrap();
    assert!(level.attached_items().attached(trader).is_empty());
    for i in level.attached_items().items().values() {
        assert_eq!(level.placement(i.instance), None);
    }
}
