// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The trader's door in a New Game on the Russian edition's data (BYOA),
//! run by the shipped door scheme `ph_door` from its logic file
//! `scripts\esc_trader_door.ltx` (script-api-configuration-and-files.md 9;
//! the scheme's calls, script API Y 11; its use, use-action.md 4.1, 11.1):
//! locked at the start (its hinge held, the door-locked hint, the player
//! run at it stopped, a use playing its sound at the door and leaving it
//! locked); once the player holds `esc_trader_can_leave` (given as a
//! dialog's effect gives it, through the story), unlocked and closed
//! (`ph_door@closed`, its 2D sound), and a use opening it (`ph_door@open`),
//! the player walking out through it. Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::io;
use std::rc::Rc;

use volya_engine::app::{App, AppState};
use volya_engine::audio::{AudioOut, Voice};
use volya_engine::controls::MoveAction;
use volya_engine::core::audio::{Category, Playback};
use volya_engine::core::camera::Angles;
use volya_engine::core::collision::geometry::{Vec3, sub};
use volya_engine::core::dialogs::conditions::DialogWorld;
use volya_engine::core::input::{Control, Event, Input, Key};
use volya_engine::core::sound::system::SoundSystem;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs, VfsFile};
use volya_engine::data::GameData;
use volya_engine::script::Value;
use volya_engine::script::loader::Trees;
use volya_engine_formats::ui::Resolution;

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

/// An audio output with the positional sound system; plays nothing.
struct Output(SoundSystem);

impl AudioOut for Output {
    fn play(&mut self, _: &str, _: Playback) -> Option<Voice> {
        None
    }
    fn stop(&mut self, _: Voice) {}
    fn is_playing(&self, _: Voice) -> bool {
        false
    }
    fn set_master_gain(&mut self, _: f32) {}
    fn set_volume(&mut self, _: Category, _: f32) {}
    fn sounds(&mut self) -> Option<&mut SoundSystem> {
        Some(&mut self.0)
    }
}

/// The servomotor's sound, the locked use's and the unlock's
/// (use-action.md 11.1; the logic file's `play_snd`).
const SERVOMOTOR: &str = "door_servomotor";

struct Run {
    app: App,
    trees: Rc<GameTrees>,
    input: Input,
    audio: Output,
    t: f64,
    door: u16,
}

impl Run {
    fn frames(&mut self, n: u32) {
        for _ in 0..n {
            self.t += 16.0;
            (self.app).update(&self.trees.tree, self.t, &mut self.input, &mut self.audio);
            self.input.end_frame();
        }
    }

    fn press(&mut self, key: Key) {
        for pressed in [true, false] {
            self.input.handle(Event::Key { key, pressed });
        }
        self.frames(1);
    }

    fn feet(&self) -> Vec3 {
        self.app.level().unwrap().player().state().position
    }

    fn hint(&self) -> Option<String> {
        self.app.level().unwrap().state().player.hint
    }

    /// The door's motor and hint text, as the level reports them.
    fn door(&self) -> (Option<[f32; 2]>, Option<String>) {
        let d = &self.app.level().unwrap().state().doors[0];
        (d.motor, d.tip.clone())
    }

    /// The door's active logic section (the scripts' storage).
    fn section(&mut self) -> Value {
        let door = self.door;
        let scripts = self.app.game_scripts().unwrap();
        let code = format!("return db.storage[{door}].active_section");
        scripts.scripts().exec("=section", code.as_bytes()).unwrap()[0].clone()
    }

    /// The servomotor's emitters: their positions and whether 2D.
    fn servomotor(&self) -> Vec<(Vec3, bool)> {
        (self.audio.0.emitters().into_iter())
            .filter(|e| e.name.contains(SERVOMOTOR))
            .map(|e| (e.position, e.two_d))
            .collect()
    }

    fn face(&mut self, feet: Vec3, to: Vec3) {
        let level = self.app.level_mut().unwrap();
        let h = (level.player().config).eye_height(level.player().body().box_index());
        let d = sub(to, feet);
        let angles = Angles {
            yaw: d[0].atan2(d[2]),
            pitch: 0.0,
            roll: 0.0,
        };
        level.set_camera([feet[0], feet[1] + h, feet[2]], angles);
    }
}

/// The horizontal distance from `p` to the plane through `centre` with
/// normal `n` (unit, horizontal), signed.
fn across(p: Vec3, centre: Vec3, n: Vec3) -> f32 {
    let d = sub(p, centre);
    d[0] * n[0] + d[2] * n[2]
}

#[test]
fn the_scheme_keeps_the_door_locked_until_the_player_may_leave() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let mut data = GameData::read(&trees.tree, None, Resolution::new(1024, 768)).unwrap();
    data.measure_intro(&trees.tree).unwrap();
    data.intro_lengths = vec![0.0; data.intro.items.len()];
    assert_eq!(data.strings.get("tip_door_locked"), "Дверь заперта");
    let mut app = App::new(data).unwrap();
    app.start_scripts(trees.clone()).unwrap();
    let mut audio = SoundSystem::new(32);
    audio.set_volume(Category::Effects, 1.0);
    let mut run = Run {
        app,
        trees,
        input: Input::new(),
        audio: Output(audio),
        t: 0.0,
        door: 0,
    };
    for _ in 0..20 {
        if matches!(run.app.state(), AppState::Menu { .. }) {
            break;
        }
        run.press(Key::Escape);
    }
    run.press(Key::Enter);
    run.press(Key::Enter);
    for _ in 0..400 {
        if matches!(run.app.state(), AppState::Level { .. }) {
            break;
        }
        run.frames(1);
    }
    assert!(matches!(run.app.state(), AppState::Level { .. }));
    // Escape skips the intro (startup.md 6.5).
    run.frames(40);
    run.press(Key::Escape);
    assert!(matches!(
        run.app.state(),
        AppState::Level { intro: None, .. }
    ));
    run.frames(5);

    // The locked section: the hinge held closed (Y 11 step 4), the hint
    // `tip_door_locked`.
    let level = run.app.level().unwrap();
    let doors = level.state().doors;
    let names: Vec<&str> = doors.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names[0], "trader_door", "{names:?}");
    run.door = level
        .simulation()
        .unwrap()
        .level_list()
        .find(|&id| {
            (level.simulation().unwrap().object(id))
                .is_some_and(|o| o.record.header.name.as_str() == "trader_door")
        })
        .unwrap();
    let leaf = level.doors().boxes()[0];
    let start = level.player().state().position;
    assert_eq!(run.section(), Value::from("ph_door@locked"));
    assert_eq!(
        run.door(),
        (Some([10_000.0, 1.0]), Some("tip_door_locked".into()))
    );

    // Its leaf, facing it from the start.
    let k = (0..3)
        .min_by(|&a, &b| leaf.half_size[a].total_cmp(&leaf.half_size[b]))
        .unwrap();
    let mut n = leaf.axes[k];
    if across(start, leaf.centre, n) < 0.0 {
        n = n.map(|v| -v);
    }
    run.face(start, leaf.centre);
    run.frames(2);
    assert_eq!(run.hint().as_deref(), Some("tip_door_locked"));

    // Run at it: stopped in front of the leaf.
    run.input.handle(Event::Key {
        key: Key::W,
        pressed: true,
    });
    let mut closest = f32::INFINITY;
    for _ in 0..150 {
        run.frames(1);
        closest = closest.min(across(run.feet(), leaf.centre, n) - leaf.half_size[k]);
    }
    run.input.handle(Event::Key {
        key: Key::W,
        pressed: false,
    });
    run.frames(1);
    let left = across(run.feet(), leaf.centre, n) - leaf.half_size[k];
    eprintln!("the locked door: closest {closest:.3} m, {left:.3} m from its face");
    assert!(
        closest > 0.25 && left < 0.5,
        "{closest} … {left} m from the face"
    );
    assert_eq!(run.hint().as_deref(), Some("tip_door_locked"));

    // Use it (use-action.md 4.1, 11.1): the servomotor plays at the door,
    // and it stays locked.
    let use_key = (run.app.level().unwrap().controls().bindings)
        .controls(MoveAction::Use)
        .find_map(|c| match c {
            Control::Key(k) => Some(k),
            _ => None,
        })
        .expect("a key bound to use");
    assert!(run.servomotor().is_empty());
    run.press(use_key);
    let played = run.servomotor();
    eprintln!("the locked use: {played:?}");
    assert_eq!(played.len(), 1, "{played:?}");
    let (at, two_d) = played[0];
    assert!(!two_d);
    assert!(sub(at, leaf.centre).iter().all(|v| v.abs() < 2.0), "{at:?}");
    run.frames(5);
    assert_eq!(run.section(), Value::from("ph_door@locked"));
    assert_eq!(run.door().0, Some([10_000.0, 1.0]));

    // The player may leave: `esc_trader_can_leave` given to the player as
    // a dialog's effect gives it. The logic's `on_info` switches to
    // `ph_door@closed`, playing the servomotor 2D; closed and unlocked,
    // the hint `tip_door_open`.
    let scripts = run.app.game_scripts().unwrap();
    (scripts.scripts().give_to_player("esc_trader_can_leave")).unwrap();
    run.frames(3);
    assert_eq!(run.section(), Value::from("ph_door@closed"));
    let played = run.servomotor();
    eprintln!("the unlock: {played:?}");
    assert!(played.iter().any(|&(_, two_d)| two_d), "{played:?}");
    run.frames(10);
    assert_eq!(
        run.door(),
        (Some([10_000.0, 1.0]), Some("tip_door_open".into()))
    );
    assert_eq!(run.hint().as_deref(), Some("tip_door_open"));

    // A use opens it: `ph_door@open`, the opening motor (Y 11 step 4) and
    // the hint `tip_door_close`.
    run.press(use_key);
    run.frames(2);
    assert_eq!(run.section(), Value::from("ph_door@open"));
    let (motor, tip) = run.door();
    assert_eq!(motor, Some([2100.0, -3.0]));
    assert_eq!(tip.as_deref(), Some("tip_door_close"));
    run.frames(120);
    let angle = run.app.level().unwrap().state().doors[0].angle.unwrap();
    eprintln!("the open door's hinge angle {angle}");
    assert!(angle.abs() > 1.0, "{angle}");

    // The player walks out through the doorway, past where the leaf stood.
    let feet = run.feet();
    let target = [
        leaf.centre[0] - n[0] * 2.0,
        feet[1],
        leaf.centre[2] - n[2] * 2.0,
    ];
    let mut farthest = f32::INFINITY;
    run.face(feet, target);
    run.frames(1);
    run.input.handle(Event::Key {
        key: Key::W,
        pressed: true,
    });
    for _ in 0..200 {
        run.frames(1);
        farthest = farthest.min(across(run.feet(), leaf.centre, n));
    }
    eprintln!("walking out: {farthest:.3} m past the leaf's plane");
    assert!(farthest < -1.0, "{farthest} m from the leaf's plane");
}
