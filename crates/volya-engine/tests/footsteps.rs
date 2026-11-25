// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's footsteps from the Russian edition's data (BYOA): the
//! verification facts of docs/formats/material-pairs.md (S9) and
//! docs/behavior/player-cycle-lengths.md (S10) and the configuration and
//! pairs of docs/behavior/player-footsteps.md (S11); then a New Game on
//! `l01_escape` whose player runs and jumps, with the steps and the sounds
//! they play checked against S11's timing and S11 7.2's step lists, and
//! the footsteps' cost per frame (meaningful in release builds). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use md5::{Digest, Md5};
use volya_engine::core::audio::Category;
use volya_engine::core::collision::CollisionWorld;
use volya_engine::core::collision::geometry::{
    self, Vec3, add, cross, length, normalize, scale, sub,
};
use volya_engine::core::cycles::Cycle;
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::sound::system::{NoDevice, SoundSystem};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{self, StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::footsteps::{self, MaterialPairs, PairSounds};
use volya_engine::level::LevelScreen;
use volya_engine_formats::gamemtl;
use volya_engine_formats::skeletal_motions::{self, FX, ModelMotions};
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);
const FRAME_MS: u64 = 16;
/// `materials\earth`'s id.
const EARTH: u32 = 7;

/// The centre of a large, level triangle of material `id` near `near`
/// with nothing above it for 4 m.
fn open_ground(world: &CollisionWorld, near: Vec3, id: u32) -> Vec3 {
    let (r, mut found) = (80.0, Vec::new());
    world.query(
        [near[0] - r, near[1] - 30.0, near[2] - r],
        [near[0] + r, near[1] + 30.0, near[2] + r],
        &mut found,
    );
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut best: Option<(f32, Vec3)> = None;
    for &t in &found {
        if world.material(t).id != id {
            continue;
        }
        let [a, b, c] = world.corners(t);
        let n = cross(sub(b, a), sub(c, a));
        let area = length(n) / 2.0;
        let Some(n) = normalize(n) else { continue };
        if n[1].abs() < 0.97 || area < 6.0 {
            continue;
        }
        let centre = scale(add(add(a, b), c), 1.0 / 3.0);
        let mut above = Vec::new();
        let box_centre = add(centre, [0.0, 2.2, 0.0]);
        world.query(
            sub(box_centre, [1.0, 2.0, 1.0]),
            add(box_centre, [1.0, 2.0, 1.0]),
            &mut above,
        );
        if above.iter().any(|&u| {
            geometry::box_intersects(box_centre, axes, [1.0, 2.0, 1.0], &world.corners(u))
        }) {
            continue;
        }
        let d = length(sub(centre, near));
        if best.is_none_or(|(e, _)| d < e) {
            best = Some((d, centre));
        }
    }
    best.expect("no open ground").1
}

fn md5_hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// S11 7.2: the step sounds of `l01_escape`'s materials (by id), all under
/// `material\`.
fn l01_step_sounds(id: u32) -> Vec<String> {
    let (base, count) = match id {
        0 | 3 | 4 | 5 | 6 | 12 | 18 | 49 | 53 | 80 | 81 => ("actor\\step\\default", 4),
        2 | 19 | 39 => ("actor\\step\\new_wood", 4),
        7 | 10 => ("actor\\step\\earth", 4),
        8 => ("actor\\step\\gravel", 2),
        13 | 34 => ("actor\\step\\metal_plate", 4),
        37 => ("actor\\step\\grass", 4),
        48 => {
            return (1..=4)
                .map(|k| format!("material\\human\\step\\metall0{k}gr"))
                .collect();
        }
        60 => ("actor\\step\\tin", 4),
        _ => panic!("material {id} is not one of l01_escape's"),
    };
    (1..=count)
        .map(|k| format!("material\\{base}{k}"))
        .collect()
}

#[test]
fn footsteps_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();

    // S9 section 1, 7.
    let library = vfs::read(
        &tree,
        &data.aliases.path("$game_data$", "gamemtl.xr").unwrap(),
    )
    .unwrap();
    assert_eq!(library.len(), 310_494);
    let materials = gamemtl::parse(&library).unwrap();
    assert_eq!(materials.len(), 57);
    let pairs = gamemtl::parse_pairs(&library).unwrap();
    assert_eq!(pairs.len(), 1_247);
    let canonical: String = pairs.iter().map(|p| p.canonical_line() + "\n").collect();
    assert_eq!(canonical.len(), 252_322);
    assert_eq!(
        md5_hex(canonical.as_bytes()),
        "57522a1ae37621a8fa33228ce9a21a0b"
    );
    let by_id: HashMap<u32, &gamemtl::MaterialPair> = pairs.iter().map(|p| (p.id, p)).collect();
    let p = by_id[&1211];
    assert_eq!(
        (p.first, p.second, p.link, p.fifth_word),
        (90, 7, gamemtl::NO_LINK, gamemtl::NO_LINK)
    );
    assert_eq!(p.step_items().unwrap().len(), 4);
    assert_eq!(p.step_items().unwrap()[0], "material\\actor\\step\\earth1");
    assert!(
        p.breaking.is_empty()
            && p.collide.is_empty()
            && p.particles.is_empty()
            && p.marks.is_empty()
    );
    let p = &pairs[100];
    assert_eq!(
        (p.id, p.first, p.second, p.link, p.fifth_word),
        (172, 3, 71, 85, 0x30)
    );
    assert_eq!((by_id[&1051].link, by_id[&1051].fifth_word), (1047, 0x76));
    // Section 3's item counts.
    let counts = |list: fn(&gamemtl::MaterialPair) -> &str| {
        let mut c = [0usize; 5];
        for p in &pairs {
            c[gamemtl::item_count(list(p))] += 1;
        }
        c
    };
    assert_eq!(counts(|p| &p.breaking), [1_245, 2, 0, 0, 0]);
    assert_eq!(counts(|p| &p.step), [406, 0, 146, 72, 623]);
    assert_eq!(counts(|p| &p.collide), [443, 40, 266, 127, 371]);
    assert_eq!(counts(|p| &p.particles), [637, 241, 268, 39, 62]);
    assert_eq!(counts(|p| &p.marks), [1_075, 36, 6, 63, 67]);
    let all = MaterialPairs::read(&library, &materials).unwrap();
    let index = |id: u32| materials.iter().position(|m| m.id == id).unwrap() as u32;
    // Section 7: `l01_escape`'s materials and their pairs with the actor.
    let l01: [(u32, u32); 22] = [
        (0, 1204),
        (2, 1206),
        (3, 1207),
        (4, 1208),
        (5, 1209),
        (6, 1210),
        (7, 1211),
        (8, 1212),
        (10, 1213),
        (12, 1215),
        (13, 1216),
        (18, 1219),
        (19, 1220),
        (34, 1223),
        (37, 1224),
        (39, 1226),
        (48, 1227),
        (49, 1228),
        (53, 1229),
        (60, 1231),
        (80, 1249),
        (81, 1250),
    ];
    let actor = index(90);
    let mut lines: Vec<(usize, String)> = Vec::new();
    for (material, pair) in l01 {
        let k = all.table.pair(actor, index(material)).unwrap().unwrap();
        assert_eq!(pairs[k].id, pair, "material {material}");
        lines.push((k, pairs[k].canonical_line() + "\n"));
        // S11 7.2.
        assert_eq!(
            all.lists[k].step,
            l01_step_sounds(material),
            "material {material}"
        );
        assert_eq!(
            all.lists[k].particles.is_empty(),
            material != 37,
            "material {material}"
        );
    }
    lines.sort();
    let text: String = lines.into_iter().map(|(_, l)| l).collect();
    assert_eq!(md5_hex(text.as_bytes()), "5ed900ed7bf672191be953f738b55a0e");

    // S11 7.1, 7.2.
    let actor_section = data.system.section("actor").unwrap();
    assert_eq!(
        actor_section.get("material"),
        Some(Some("creatures\\actor"))
    );
    assert_eq!(
        actor_section.get("step_params"),
        Some(Some("stalker_step_manager"))
    );
    assert_eq!(actor_section.get("LegsCount"), Some(Some("2")));
    assert_eq!(index(90), 53);
    assert_eq!(index(0), 0);
    let with_actor: Vec<usize> = (0..materials.len() as u32)
        .filter_map(|m| all.table.pair(actor, m).unwrap())
        .collect();
    assert_eq!(with_actor.len(), 57);
    let names: BTreeSet<&String> = with_actor
        .iter()
        .flat_map(|&k| &all.lists[k].step)
        .collect();
    assert_eq!(names.len(), 40);
    let mut sizes = [0; 5];
    for &k in &with_actor {
        sizes[all.lists[k].step.len()] += 1;
    }
    assert_eq!(sizes, [4, 0, 4, 0, 49]);
    let default = all.table.pair(actor, 0).unwrap().unwrap();
    assert_eq!(pairs[default].id, 1204);

    // S10 section 1: the player's model and its motion file.
    let model = "actors\\hero\\stalker_novice";
    let mut file = tree
        .open("gamedata\\meshes\\actors\\hero\\stalker_novice.ogf")
        .unwrap();
    assert_eq!(
        skeletal_motions::read_model_motions(&mut file, model).unwrap(),
        ModelMotions::Files(vec!["actors\\stalker_animation".into()])
    );
    let t = Instant::now();
    let mut file = tree
        .open("gamedata\\meshes\\actors\\stalker_animation.omf")
        .unwrap();
    let set = skeletal_motions::read_motion_file(&mut file, "stalker_animation").unwrap();
    eprintln!(
        "read stalker_animation.omf's definitions and key counts in {:.1?}",
        t.elapsed()
    );
    let defs = &set.params.definitions;
    assert_eq!(defs.len(), 1_569);
    let cycles: Vec<(usize, &skeletal_motions::MotionDefinition)> = defs
        .iter()
        .enumerate()
        .filter(|(_, d)| d.flags & FX == 0)
        .collect();
    assert_eq!(cycles.len(), 1_545);
    // S10 5.2.
    let sum_n: u64 = cycles
        .iter()
        .map(|&(k, _)| u64::from(set.key_counts[k]))
        .sum();
    assert_eq!(sum_n, 125_350);
    let values: Vec<Cycle> = cycles
        .iter()
        .map(|&(k, d)| Cycle::new(set.key_counts[k], d.flags, d.speed))
        .collect();
    let sum_l: f64 = values.iter().map(|c| f64::from(c.length)).sum();
    let sum_d: f64 = values.iter().map(|c| f64::from(c.duration)).sum();
    assert!((sum_l - 4178.3335).abs() < 1e-3, "{sum_l}");
    assert!((sum_d - 4139.9268).abs() < 1e-3, "{sum_d}");
    let mut flags: HashMap<u16, usize> = HashMap::new();
    for (_, d) in &cycles {
        *flags.entry(d.flags).or_default() += 1;
    }
    assert_eq!(
        flags,
        HashMap::from([(0, 270), (2, 1_055), (6, 1), (8, 219)])
    );
    let player_cycles =
        footsteps::read_cycles(&tree, &data, "gamedata\\levels\\l01_escape", model).unwrap();
    assert_eq!(player_cycles.len(), 1_545);
    let walk = player_cycles.get("norm_walk_fwd_0").unwrap();
    assert_eq!(
        (f64::from(walk.length) * 1e10).round() / 1e10,
        0.866_666_734_2
    );
    assert_eq!((f64::from(walk.duration) * 1e7).round() / 1e7, 0.867_129_8);
    // S10 5.1, S11 section 1: every line of the step table is a cycle, with
    // 5 items and one pass; the player's walk, run, sprint and landing
    // cycles do not stop at the end (S11 2.1).
    let table = data.system.section("stalker_step_manager").unwrap();
    assert_eq!(table.len(), 37);
    for e in table.entries() {
        assert!(player_cycles.get(e.key()).is_some(), "{}", e.key());
        let items: Vec<&str> = e.value().unwrap().split(',').collect();
        assert_eq!((items.len(), items[0].trim()), (5, "1"), "{}", e.key());
    }
    for name in [
        "norm_walk_fwd_0",
        "norm_run_rs_0",
        "cr_walk_ls_0",
        "cr_run_back_0",
        "norm_escape_00",
        "norm_jump_end",
    ] {
        assert!(!player_cycles.get(name).unwrap().stops_at_end, "{name}");
    }

    // A New Game: the player runs forward, then jumps.
    let t = Instant::now();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();
    eprintln!("loaded the level in {:.1?}", t.elapsed());
    let sound = level.take_sound_data().unwrap();
    let mut system = SoundSystem::new(32);
    system.set_volume(Category::Effects, 1.0);
    let t = Instant::now();
    let pair_sounds = PairSounds::create(
        &mut system,
        &tree,
        &data.aliases,
        sound.pairs.as_ref().unwrap(),
    )
    .unwrap();
    eprintln!(
        "created the pairs' {} sounds ({} files) in {:.1?}",
        system.sound_ids().count(),
        system.files().len(),
        t.elapsed()
    );
    let f = level.footsteps().unwrap();
    assert_eq!(f.table().legs, 2);
    assert_eq!(f.table().len(), 37);
    assert_eq!(f.cycles().len(), 1_545);

    let mut input = Input::new();
    let mut now = 0;
    let mut cost = (Duration::ZERO, Duration::ZERO, 0u32);
    // (frame time, step clock, leg cycle, sounds' positions and volumes, the
    // ground material's id, the pair's names).
    let mut frames = Vec::new();
    let mut run = |level: &mut LevelScreen,
                   input: &mut Input,
                   system: &mut SoundSystem,
                   n: u32,
                   frames: &mut Vec<_>| {
        for _ in 0..n {
            now += FRAME_MS;
            // The footsteps' own cost: the same frame on a copy of them.
            // The step clock: the frames' real time (the app's level sound
            // clock), here the frame time itself.
            let clock = now as u32;
            let mut copy = level.footsteps().unwrap().clone();
            level.update(now, input);
            input.end_frame();
            let t = Instant::now();
            copy.frame(clock, level.player(), true).unwrap();
            let d = t.elapsed();
            cost = (cost.0 + d, cost.1.max(d), cost.2 + 1);
            level.footsteps_frame(clock).unwrap();
            let f = level.footsteps().unwrap();
            let ground = level.player().body().ground_material().unwrap_or(0);
            let id = level.collision().materials()[ground as usize].id;
            // Each step plays from the ground's step list (S11 7.2), at the
            // feet raised by 0.5 m with the leg's volume, 3D, not looped.
            let mut names = Vec::new();
            for s in f.sounds() {
                pair_sounds.play(system, s);
                let e = system.emitters().pop().unwrap();
                assert!(l01_step_sounds(id).contains(&e.name), "{} on {id}", e.name);
                assert_eq!((e.position, e.volume), (s.position, s.volume));
                assert!(!e.looped && !e.two_d);
                names.push(e.name);
            }
            let p = level.player().state().position;
            assert!(
                f.sounds()
                    .iter()
                    .all(|s| s.position == [p[0], p[1] + 0.5, p[2]])
            );
            system.update(now, &mut NoDevice);
            frames.push((
                now,
                clock,
                f.leg_cycle().map(str::to_owned),
                f.sounds().to_vec(),
                id,
                names,
            ));
        }
    };
    // Standing at the start: the idle cycle on the level's ground, no steps.
    run(&mut level, &mut input, &mut system, 30, &mut frames);
    assert_eq!(frames.last().unwrap().2.as_deref(), Some("norm_idle_0"));
    // Then on open ground of `materials\earth` (id 7) near the start.
    let at = open_ground(level.collision(), level.player().state().position, EARTH);
    eprintln!("open earth at {at:?}");
    let h = level.player().view().eye[1] - level.player().state().position[1];
    let angles = level.player().angles();
    level.set_camera([at[0], at[1] + 0.05 + h, at[2]], angles);
    run(&mut level, &mut input, &mut system, 30, &mut frames);
    assert_eq!(frames.last().unwrap().4, EARTH);
    let start = level.player().state().position;
    assert!(frames.iter().all(|f| f.3.is_empty()));
    assert_eq!(frames.last().unwrap().2.as_deref(), Some("norm_idle_0"));
    input.handle(Event::Key {
        key: Key::W,
        pressed: true,
    });
    run(&mut level, &mut input, &mut system, 100, &mut frames);
    input.handle(Event::Key {
        key: Key::W,
        pressed: false,
    });
    run(&mut level, &mut input, &mut system, 60, &mut frames);
    let end = level.player().state().position;
    let moved = ((end[0] - start[0]).powi(2) + (end[2] - start[2]).powi(2)).sqrt();
    eprintln!("ran {moved:.2} m");
    assert!(moved > 2.0, "{moved}");

    // The run's steps: S11 3.3's due times (173, 563 ms into each pass of
    // 867 ms from the cycle's start), each heard at the first frame at or
    // after its due time, 2 a pass, volume 1, from the ground's list.
    let runs: Vec<usize> = (0..frames.len())
        .filter(|&i| frames[i].2.as_deref() == Some("norm_run_fwd_0"))
        .collect();
    assert!(runs.len() > 90, "{}", runs.len());
    let first = runs[0];
    assert!(
        runs.windows(2).all(|w| w[1] == w[0] + 1),
        "the run's cycle restarted"
    );
    let t0 = frames[first].1;
    let mut due: Vec<u32> = Vec::new();
    for pass in 0..10 {
        for offset in [173, 563] {
            due.push(t0 + pass * 867 + offset);
        }
    }
    let last = *runs.last().unwrap();
    let mut heard = Vec::new();
    for (_, clock, _, sounds, ..) in &frames[first..=last] {
        for s in sounds {
            assert_eq!(s.volume, 1.0);
            heard.push(*clock);
        }
    }
    let expected: Vec<u32> = due
        .iter()
        .filter(|&&d| d <= frames[last].1)
        .map(|&d| {
            frames[first..=last]
                .iter()
                .map(|f| f.1)
                .find(|&c| c >= d)
                .unwrap()
        })
        .collect();
    assert_eq!(heard, expected);
    eprintln!("run steps at {heard:?} ms (cycle from {t0})");
    let grounds: BTreeSet<u32> = frames[first..=last].iter().map(|f| f.4).collect();
    let names: Vec<&String> = frames[first..=last].iter().flat_map(|f| &f.5).collect();
    assert_eq!(names.len(), heard.len());
    eprintln!("ran over materials {grounds:?}, playing {names:?}");

    // A jump: the landing cycle's two steps (due +66 and +70 ms, volume 1.3)
    // both in the frame at +80 ms, on the ground.
    input.handle(Event::Key {
        key: Key::Space,
        pressed: true,
    });
    run(&mut level, &mut input, &mut system, 1, &mut frames);
    input.handle(Event::Key {
        key: Key::Space,
        pressed: false,
    });
    run(&mut level, &mut input, &mut system, 90, &mut frames);
    let landing = frames
        .iter()
        .position(|f| f.2.as_deref() == Some("norm_jump_end"))
        .expect("no landing");
    let t0 = frames[landing].1;
    let steps: Vec<(u32, f32)> = frames[landing..]
        .iter()
        .take_while(|f| f.2.as_deref() == Some("norm_jump_end"))
        .flat_map(|f| f.3.iter().map(move |s| (f.1 - t0, s.volume)))
        .collect();
    assert_eq!(steps, [(80, 1.3), (80, 1.3)]);
    assert!(
        frames[..landing]
            .iter()
            .rev()
            .take(5)
            .any(|f| f.2.as_deref() == Some("norm_jump_idle"))
    );

    eprintln!(
        "footsteps per frame: {:.2} µs average, {:.2} µs worst over {} frames",
        cost.0.as_secs_f64() * 1e6 / f64::from(cost.2),
        cost.1.as_secs_f64() * 1e6,
        cost.2
    );
}
