// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's footsteps from the Russian edition's data (BYOA): the
//! verification facts of docs/formats/material-pairs.md (S9) and
//! docs/behavior/player-cycle-lengths.md (S10) and the configuration and
//! pairs of docs/behavior/player-footsteps.md (S11); then a New Game on
//! `l01_escape` whose player runs and jumps, with the steps and the sounds
//! they play checked against S11's timing and S11 7.2's step lists, and
//! the footsteps' cost per frame (meaningful in release builds); and a walk
//! out of the start bunker onto the earth outside (#837), its first step's
//! time and each step's material against the ground under the feet. Skips
//! when `VOLYA_GOG_INSTALLER` is unset.

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
            // The step clock: the global clock (the app's), here the
            // frames' real time, nothing paused.
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

/// A device recording the voices started.
#[derive(Default)]
struct Starts(Vec<usize>);

impl volya_engine::core::sound::system::Device for Starts {
    fn start(&mut self, voice: usize, _: &volya_engine::core::sound::Sound, _: bool, _: u64) {
        self.0.push(voice);
    }
    fn stop(&mut self, _: usize) {}
    fn set_listener(&mut self, _: &volya_engine::core::sound::system::DeviceListener) {}
    fn set_source(&mut self, _: usize, _: &volya_engine::core::sound::system::DeviceSource) {}
}

/// The material id of the solid triangle under `feet` (within 1 m below,
/// 0.3 m above) and the ids of pass-through ones there.
fn under(world: &CollisionWorld, feet: Vec3) -> (Option<u32>, BTreeSet<u32>) {
    let mut found = Vec::new();
    world.query(
        [feet[0] - 0.01, feet[1] - 1.0, feet[2] - 0.01],
        [feet[0] + 0.01, feet[1] + 0.3, feet[2] + 0.01],
        &mut found,
    );
    let origin = [feet[0], feet[1] + 0.3, feet[2]];
    let mut solid: Option<(f32, u32)> = None;
    let mut passing = BTreeSet::new();
    for &t in &found {
        let Some(d) = geometry::ray_triangle_distance(origin, [0.0, -1.0, 0.0], &world.corners(t))
        else {
            continue;
        };
        if !(0.0..=1.3).contains(&d) {
            continue;
        }
        if world.is_solid(t) {
            if solid.is_none_or(|(e, _)| d < e) {
                solid = Some((d, world.material(t).id));
            }
        } else {
            passing.insert(world.material(t).id);
        }
    }
    (solid.map(|s| s.1), passing)
}

/// Materials whose steps play `actor\step\default*` (S11 7.2): the hard
/// surfaces.
const HARD: [u32; 11] = [0, 3, 4, 5, 6, 12, 18, 49, 53, 80, 81];

/// Triangle `t` is solid and faces up within about 25°.
fn walkable(world: &CollisionWorld, t: u32) -> bool {
    let [a, b, c] = world.corners(t);
    world.is_solid(t) && normalize(cross(sub(b, a), sub(c, a))).is_some_and(|n| n[1] > 0.9)
}

/// Free of solid triangles from 0.45 to 1.85 m above `feet`, 0.3 m around.
fn headroom(world: &CollisionWorld, feet: Vec3) -> bool {
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let centre = add(feet, [0.0, 1.15, 0.0]);
    let half = [0.3, 0.7, 0.3];
    let mut found = Vec::new();
    world.query(sub(centre, half), add(centre, half), &mut found);
    !found.iter().any(|&t| {
        world.is_solid(t) && geometry::box_intersects(centre, axes, half, &world.corners(t))
    })
}

/// The walkable solid floors on the vertical line through (`x`, `z`)
/// between `y0` and `y1`, with headroom: their heights and material ids.
fn floors(world: &CollisionWorld, x: f32, z: f32, y0: f32, y1: f32) -> Vec<(f32, u32)> {
    let mut found = Vec::new();
    world.query(
        [x - 0.01, y0, z - 0.01],
        [x + 0.01, y1, z + 0.01],
        &mut found,
    );
    let mut out = Vec::new();
    for &t in &found {
        if !walkable(world, t) {
            continue;
        }
        let Some(d) =
            geometry::ray_triangle_distance([x, y1, z], [0.0, -1.0, 0.0], &world.corners(t))
        else {
            continue;
        };
        let y = y1 - d;
        if (y0..=y1).contains(&y) && headroom(world, [x, y, z]) {
            out.push((y, world.material(t).id));
        }
    }
    out
}

/// The way out of the bunker from `start`: a breadth-first search over a
/// 0.25 m grid of walkable floors (a neighbour's floor within 0.45 m of
/// height, with headroom) to the nearest place with nothing above it for
/// 10 m; the path's points, from `start`.
fn way_out(world: &CollisionWorld, start: Vec3, goal: impl Fn(Vec3) -> bool) -> Vec<Vec3> {
    use std::collections::VecDeque;
    let step = 0.25f32;
    let key = |p: Vec3| {
        (
            ((p[0] - start[0]) / step).round() as i32,
            ((p[2] - start[2]) / step).round() as i32,
            (p[1] * 4.0).round() as i32,
        )
    };
    let open = |p: Vec3| {
        let mut found = Vec::new();
        world.query(
            [p[0] - 0.01, p[1] + 2.0, p[2] - 0.01],
            [p[0] + 0.01, p[1] + 12.0, p[2] + 0.01],
            &mut found,
        );
        !found.iter().any(|&t| {
            world.is_solid(t)
                && geometry::ray_triangle_distance(
                    [p[0], p[1] + 2.0, p[2]],
                    [0.0, 1.0, 0.0],
                    &world.corners(t),
                )
                .is_some_and(|d| (0.0..=10.0).contains(&d))
        })
    };
    let mut seen: HashMap<(i32, i32, i32), Option<Vec3>> = HashMap::new();
    let mut queue = VecDeque::from([start]);
    seen.insert(key(start), None);
    let mut end = None;
    while let Some(p) = queue.pop_front() {
        if goal(p) && open(p) {
            end = Some(p);
            break;
        }
        if length(sub(p, start)) > 80.0 {
            continue;
        }
        for d in [[step, 0.0], [-step, 0.0], [0.0, step], [0.0, -step]] {
            let (x, z) = (p[0] + d[0], p[2] + d[1]);
            for (y, _) in floors(world, x, z, p[1] - 0.45, p[1] + 0.45) {
                let q = [x, y, z];
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key(q)) {
                    e.insert(Some(p));
                    queue.push_back(q);
                }
            }
        }
    }
    let mut path = vec![end.expect("no way out")];
    while let Some(Some(p)) = seen.get(&key(*path.last().unwrap())) {
        path.push(*p);
    }
    path.reverse();
    path
}

/// #837: leaving the start bunker. From just past the trader's locked door
/// (#874) the player stands, then runs (W held, steered by the mouse)
/// along the way out to the first
/// earth outside: the time from the key to the first step and to its
/// voice starting (S11 3.2, 3.3: the first step is due 173 ms into the
/// cycle), and per step the pair's ground material against the triangles
/// under the feet (S11 4.1, level-cform.md 4.4).
#[test]
fn footsteps_leaving_the_bunker() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();
    let sound = level.take_sound_data().unwrap();
    let mut system = SoundSystem::new(32);
    system.set_volume(Category::Effects, 1.0);
    let pair_sounds = PairSounds::create(
        &mut system,
        &tree,
        &data.aliases,
        sound.pairs.as_ref().unwrap(),
    )
    .unwrap();
    let start = level.player().state().position;
    let world = level.shared_collision();
    let mut path = way_out(&world, start, |_| true);
    // The trader's door is locked at the start (trader-door.md, #874): the
    // way out starts past its leaf, 1 m beyond it, the player placed there.
    let leaf = level.player().body().bodies()[0].centre;
    let flat = |p: Vec3| (p[0] - leaf[0]).hypot(p[2] - leaf[2]);
    let at_door = path
        .iter()
        .position(|&p| flat(p) < 0.5)
        .expect("the door on the way");
    let past = at_door + (path[at_door..].iter().position(|&p| flat(p) > 1.0).unwrap());
    path.drain(..past);
    let p = level.player();
    let h = p.config.eye_height(p.body().box_index());
    let (feet, next) = (path[0], path[1]);
    level.set_camera(
        [feet[0], feet[1] + h, feet[2]],
        volya_engine::core::camera::Angles {
            yaw: (next[0] - feet[0]).atan2(next[2] - feet[2]),
            pitch: 0.0,
            roll: 0.0,
        },
    );
    let exit = *path.last().unwrap();
    let outside = way_out(&world, exit, |p| {
        under(&world, p).0.is_some_and(|m| matches!(m, 7 | 10))
    });
    path.extend(&outside[1..]);
    // The path's length up to each point.
    let mut along = vec![0.0f32];
    for w in path.windows(2) {
        along.push(along.last().unwrap() + length(sub(w[1], w[0])));
    }
    let earth = *along.last().unwrap();
    eprintln!("the way out: {earth:.2} m to earth, open sky at {exit:?}");

    let mut input = Input::new();
    let mut device = Starts::default();
    let mut now = 0u64;
    let mut frame = |level: &mut LevelScreen, input: &mut Input| {
        now += FRAME_MS;
        level.update(now, input);
        input.end_frame();
        level.footsteps_frame(now as u32).unwrap();
        let mut steps = Vec::new();
        for s in level.footsteps().unwrap().sounds() {
            pair_sounds.play(&mut system, s);
            steps.push(*s);
        }
        let v = level.player().view();
        system.set_listener(volya_engine::core::sound::system::Listener {
            position: v.eye,
            direction: v.direction,
            up: v.up,
        });
        device.0.clear();
        system.update(now, &mut device);
        (now as u32, device.0.len(), steps)
    };
    for _ in 0..60 {
        assert!(frame(&mut level, &mut input).2.is_empty(), "steps standing");
    }
    // The mouse's turn per pixel.
    let yaw = level.player().angles().yaw;
    input.handle(Event::PointerMotion {
        delta: [100.0, 0.0],
    });
    frame(&mut level, &mut input);
    let per_pixel = (level.player().angles().yaw - yaw) / 100.0;
    assert!(per_pixel.abs() > 1e-5, "the mouse does not turn");
    let wrap = |a: f32| {
        (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    };
    let steer = |level: &LevelScreen, input: &mut Input, next: &mut usize| {
        let p = level.player().state().position;
        // The nearest path point from the last one on, then 1 m ahead.
        let near = (*next..path.len())
            .take(40)
            .min_by(|&i, &k| length(sub(path[i], p)).total_cmp(&length(sub(path[k], p))))
            .unwrap();
        *next = near;
        let target = (near..path.len())
            .find(|&i| along[i] > along[near] + 1.0)
            .unwrap_or(path.len() - 1);
        let d = sub(path[target], p);
        let error = wrap(d[0].atan2(d[2]) - level.player().angles().yaw);
        input.handle(Event::PointerMotion {
            delta: [error / per_pixel, 0.0],
        });
        along[near]
    };
    let mut next = 0;
    // Facing the way out before the key.
    for _ in 0..10 {
        steer(&level, &mut input, &mut next);
        frame(&mut level, &mut input);
    }
    let mut pressed = None;
    // Per frame: step clock, voices started, the place on the path, and
    // per step the ground's id and the triangles under the feet.
    let mut log = Vec::new();
    let mut beyond = 0;
    for _ in 0..1500 {
        // Past the way's end, straight on for 2 s.
        let at = if beyond == 0 {
            steer(&level, &mut input, &mut next)
        } else {
            earth
        };
        if pressed.is_none() {
            input.handle(Event::Key {
                key: Key::W,
                pressed: true,
            });
        }
        let (clock, voices, steps) = frame(&mut level, &mut input);
        // The key is taken in this frame, whose time began a frame ago.
        pressed.get_or_insert(clock - FRAME_MS as u32);
        let p = level.player().state().position;
        let ground = level.player().body().ground_material().unwrap_or(0);
        let id = world.materials()[ground as usize].id;
        let contacts: Vec<_> = level
            .player()
            .body()
            .contacts()
            .iter()
            .map(|c| {
                (
                    c.shape,
                    world.materials()[c.material as usize].id,
                    c.normal[1],
                )
            })
            .collect();
        log.push((
            clock,
            voices,
            at,
            steps.len(),
            id,
            under(&world, p),
            contacts,
            p,
        ));
        if at >= earth - 0.01 {
            beyond += 1;
            if beyond > 125 {
                break;
            }
        }
    }
    let pressed = pressed.unwrap();
    let last = log.last().unwrap();
    assert!(
        last.2 >= earth - 0.01,
        "stuck {:.2} m along the way",
        last.2
    );
    // S11 3.2, 3.3: the run's cycle starts in the key's frame and its first
    // step is due 173 ms later, heard at the first frame at or after that;
    // its voice starts in the same frame's sound update.
    let first_step = log.iter().find(|f| f.3 > 0).unwrap().0 - pressed;
    let first_voice = log.iter().find(|f| f.1 > 0).unwrap().0 - pressed;
    eprintln!(
        "from the key (frames of {FRAME_MS} ms): first step at {first_step} ms, its voice \
         started at {first_voice} ms"
    );
    let frame_ms = FRAME_MS as u32;
    assert_eq!(first_step, (frame_ms + 173).div_ceil(frame_ms) * frame_ms);
    assert_eq!(first_voice, first_step);
    // S11 4.1, level-cform.md 4.4: each step plays the material most
    // recently touched, a pass-through one first. On the way out: the
    // bunker's hard floors and stairs, and the pass-through `materials\fake`
    // over the stairs; on the earth outside, the earth's from its first or
    // second step (a frame without contacts keeps the last material).
    let mut mismatched = 0;
    let mut outside = Vec::new();
    for (clock, _, at, steps, id, (solid, passing), contacts, p) in &log {
        if *steps == 0 {
            continue;
        }
        eprintln!(
            "step at +{} ms, {at:6.2} m along: ground {id}, under the feet {solid:?}, \
             pass-through {passing:?}, at {p:?}, contacts {contacts:?}",
            clock - pressed
        );
        if Some(*id) != *solid && !passing.contains(id) {
            mismatched += 1;
        }
        if *solid == Some(EARTH) {
            outside.push(*id);
        } else {
            assert!(HARD.contains(id), "a step on {id} inside");
        }
    }
    eprintln!("steps whose ground is not under the feet: {mismatched}");
    assert!(outside.len() >= 4, "{outside:?}");
    assert!(outside[1..].iter().all(|&id| id == EARTH), "{outside:?}");
}
