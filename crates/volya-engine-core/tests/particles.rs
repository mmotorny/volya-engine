// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The particle library and the levels' placed effects on the Russian
//! edition (BYOA): the verification facts of docs/formats/particles-xr.md
//! section 8, particle-lifecycle.md 9.1, particle-motion-actions.md 6.2 –
//! 6.3, particle-appearance-actions.md 9, particle-groups.md 9.1 and
//! level-placed-effects.md 8, and the first level's effects run for a few
//! seconds. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use md5::{Digest, Md5};
use volya_engine_core::particles::effect::time_limit_step;
use volya_engine_core::particles::placed::{Instance, PlacedEffects};
use volya_engine_core::particles::{GroupInstance, IDENTITY, Mode, Particle, Random, apply};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, read};
use volya_engine_formats::chunks::Chunks;
use volya_engine_formats::particles::{
    ActionKind, Library, PlacedRecord, Region, member_flags, parse_placed,
};

const LEVELS: [(&str, usize, usize); 12] = [
    ("l01_escape", 83, 1),
    ("l03u_agr_underground", 772, 8),
    ("l04u_labx18", 1186, 12),
    ("l05_bar", 224, 2),
    ("l08_yantar", 89, 1),
    ("l08u_brainlab", 86, 1),
    ("l10u_bunker", 474, 5),
    ("l11_pripyat", 372, 4),
    ("l12_stancia", 1995, 21),
    ("l12_stancia_2", 7190, 73),
    ("l12u_control_monolith", 204, 2),
    ("l12u_sarcofag", 284, 3),
];

struct Data {
    file: Vec<u8>,
    library: Library,
    levels: Vec<(String, Vec<u8>)>,
}

fn data() -> Option<Data> {
    let archives = gog_archives::archives()?;
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let file = read(&tree, "gamedata/particles.xr").unwrap();
    let library = Library::parse(&file).unwrap();
    let levels = LEVELS
        .iter()
        .map(|(name, ..)| {
            let path = format!("gamedata\\levels\\{name}\\level.ps_static");
            ((*name).to_owned(), read(&tree, &path).unwrap())
        })
        .collect();
    Some(Data {
        file,
        library,
        levels,
    })
}

fn type_name(kind: &ActionKind) -> &'static str {
    match kind {
        ActionKind::Bounce { .. } => "bounce",
        ActionKind::Damping { .. } => "damping",
        ActionKind::Follow { .. } => "follow",
        ActionKind::Gravitate { .. } => "gravitate",
        ActionKind::Gravity { .. } => "gravity",
        ActionKind::Jet { .. } => "jet",
        ActionKind::KillOld { .. } => "killold",
        ActionKind::Move => "move",
        ActionKind::OrbitPoint { .. } => "orbitpoint",
        ActionKind::RandomAccel { .. } => "randomaccel",
        ActionKind::RandomDisplace { .. } => "randomdisplace",
        ActionKind::RandomVelocity { .. } => "randomvelocity",
        ActionKind::Sink { .. } => "sink",
        ActionKind::Source(_) => "source",
        ActionKind::SpeedLimit { .. } => "speedlimit",
        ActionKind::TargetColor { .. } => "targetcolor",
        ActionKind::TargetSize { .. } => "targetsize",
        ActionKind::TargetRotate { .. } => "targetrotate",
        ActionKind::TargetVelocity { .. } => "targetvelocity",
        ActionKind::Vortex { .. } => "vortex",
        ActionKind::Turbulence { .. } => "turbulence",
        ActionKind::Scatter { .. } => "scatter",
        ActionKind::Unspecified { .. } => "unspecified",
    }
}

fn regions(kind: &ActionKind) -> Vec<&Region> {
    match kind {
        ActionKind::Bounce { surface, .. } => vec![surface],
        ActionKind::Jet { acceleration, .. } => vec![acceleration],
        ActionKind::RandomAccel { region }
        | ActionKind::RandomDisplace { region }
        | ActionKind::RandomVelocity { region }
        | ActionKind::Sink { region, .. } => vec![region],
        ActionKind::Source(s) => vec![&s.position, &s.velocity, &s.rotation, &s.size, &s.colour],
        _ => Vec::new(),
    }
}

#[test]
fn the_library_is_p1s() {
    let Some(d) = data() else { return };
    let lib = &d.library;
    assert!(lib.log.is_empty());
    assert_eq!(d.file.len(), 1_394_071);
    assert_eq!(lib.effects.len(), 1075);
    assert_eq!(lib.groups.len(), 252);
    let actions: usize = lib.effects.iter().map(|e| e.actions.len()).sum();
    assert_eq!(actions, 6921);
    let members: usize = lib.groups.iter().map(|g| g.members.len()).sum();
    assert_eq!(members, 1047);

    let mut per_type: BTreeMap<&str, usize> = BTreeMap::new();
    let mut per_kind = [0usize; 11];
    for e in &lib.effects {
        for a in &e.actions {
            *per_type.entry(type_name(&a.kind)).or_default() += 1;
            for r in regions(&a.kind) {
                per_kind[r.kind as usize] += 1;
            }
        }
    }
    let want: BTreeMap<&str, usize> = [
        ("bounce", 15),
        ("damping", 14),
        ("follow", 2),
        ("gravitate", 17),
        ("gravity", 310),
        ("jet", 1),
        ("killold", 1070),
        ("move", 1070),
        ("orbitpoint", 174),
        ("randomaccel", 35),
        ("randomdisplace", 33),
        ("randomvelocity", 2),
        ("sink", 89),
        ("source", 1087),
        ("speedlimit", 36),
        ("targetcolor", 976),
        ("targetsize", 747),
        ("targetrotate", 543),
        ("targetvelocity", 494),
        ("vortex", 133),
        ("turbulence", 58),
        ("scatter", 15),
    ]
    .into_iter()
    .collect();
    assert_eq!(per_type, want);
    assert_eq!(per_kind, [2608, 1722, 13, 67, 75, 489, 48, 349, 21, 217, 1]);

    // Flag bits (P1 3) and the chunks they gate (P1 2).
    let bit = |b: u32| lib.effects.iter().filter(|e| e.flags & b != 0).count();
    let bits = [
        (0x1, 1070),
        (0x400, 423),
        (0x800, 421),
        (0x1000, 339),
        (0x2000, 95),
        (0x4000, 623),
        (0x8000, 239),
        (0x10000, 71),
        (0x20000, 40),
        (0x40000, 113),
        (0x80000, 2),
        (0x100000, 51),
        (0x200000, 126),
        (0x400000, 12),
        (0x800000, 1),
    ];
    for (b, n) in bits {
        assert_eq!(bit(b), n, "{b:#x}");
    }
    let union = lib.effects.iter().fold(0, |u, e| u | e.flags);
    assert_eq!(union, 0xFFFC01);
    assert_eq!(
        lib.effects.iter().filter(|e| e.sprite.is_some()).count(),
        1070
    );

    // Sums (P1 8).
    let max: u64 = lib.effects.iter().map(|e| u64::from(e.max_particles)).sum();
    assert_eq!(max, 57_143);
    let limits: f64 = lib.effects.iter().map(|e| f64::from(e.time_limit)).sum();
    assert!((limits - 10_366.5).abs() < 0.01, "{limits}");
    let ages: f64 = (lib.effects.iter().flat_map(|e| &e.actions))
        .filter_map(|a| match a.kind {
            ActionKind::KillOld { age_limit, .. } => Some(f64::from(age_limit)),
            _ => None,
        })
        .sum();
    assert!((ages - 115_395.31).abs() < 0.01, "{ages}");
    let group_limits: f64 = lib.groups.iter().map(|g| f64::from(g.time_limit)).sum();
    assert!((group_limits - 5_088.15).abs() < 0.01, "{group_limits}");

    // The chunk-4 payloads in stored order (P1 8).
    let top = Chunks::parse(&d.file).unwrap();
    let effects = Chunks::parse(top.find(3).unwrap().payload).unwrap();
    let mut all = Vec::new();
    for c in effects.list() {
        all.extend(Chunks::parse(c.payload).unwrap().find(4).unwrap().payload);
    }
    assert_eq!(all.len(), 582_240);
    let digest: String = Md5::digest(&all)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(digest, "86c509f3d97270a1850655a0543916c1");

    // Member flag values (P1 6.2).
    let mut values: BTreeMap<u32, usize> = BTreeMap::new();
    for m in lib.groups.iter().flat_map(|g| &g.members) {
        *values.entry(m.flags).or_default() += 1;
    }
    let want: BTreeMap<u32, usize> = [
        (0x0, 48),
        (0x4, 878),
        (0x5, 44),
        (0x6, 26),
        (0x7, 2),
        (0x14, 1),
        (0x16, 1),
        (0x20, 5),
        (0x24, 18),
        (0x26, 4),
        (0x36, 1),
        (0x40, 1),
        (0x44, 10),
        (0x46, 2),
        (0x64, 2),
        (0x66, 3),
        (0x23E004, 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(values, want);

    // The test vectors of P1 8.
    let muhi = &lib.effects[lib.effect("tests\\muhi").unwrap()];
    assert_eq!(muhi.id, 934);
    assert_eq!(muhi.max_particles, 7);
    assert_eq!(muhi.flags, 0x1401);
    let sprite = muhi.sprite.as_ref().unwrap();
    assert_eq!(
        (sprite.shader.as_str(), sprite.texture.as_str()),
        ("particles\\blend", "pfx\\pfx_leaves_01.tga")
    );
    assert_eq!(
        (muhi.frames.width, muhi.frames.per_row, muhi.frames.count),
        (0.5, 2, 4)
    );
    let names: Vec<&str> = muhi.actions.iter().map(|a| type_name(&a.kind)).collect();
    assert_eq!(
        names,
        [
            "source",
            "targetrotate",
            "orbitpoint",
            "orbitpoint",
            "randomdisplace",
            "vortex",
            "killold",
            "move"
        ]
    );
    let ActionKind::Source(s) = &muhi.actions[0].kind else {
        panic!()
    };
    assert_eq!(muhi.actions[0].flags, 0x8000_0002);
    assert_eq!((s.rate, s.alpha), (17.0, 1.0));
    assert_eq!(s.velocity.r[3], 99.999_99);
    let campfire = &lib.groups[lib.group("explosions\\campfire_05").unwrap()];
    assert_eq!(campfire.id, 92);
    assert_eq!(campfire.members.len(), 7);
    assert_eq!(campfire.members[2].start_time, 0.5);
    assert_eq!(campfire.members[1].flags, 0);
}

#[test]
fn effect_facts_are_p2s_p3s_and_p4s() {
    let Some(d) = data() else { return };
    let lib = &d.library;
    // P2 9.1: the time-limited effects' stop steps.
    let steps: Vec<u64> = (lib.effects.iter())
        .filter(|e| e.flags & 0x4000 != 0)
        .map(|e| time_limit_step(e.time_limit))
        .collect();
    assert_eq!(steps.len(), 623);
    assert_eq!(steps.iter().sum::<u64>(), 314_659);
    assert_eq!(*steps.iter().min().unwrap(), 2);
    assert_eq!(*steps.iter().max().unwrap(), 60_636);
    let mut one_birth = 0;
    let mut fill = 0;
    let mut aged = 0;
    for e in &lib.effects {
        for a in &e.actions {
            if let ActionKind::Source(s) = &a.kind {
                let z = s.rate * 0.033;
                one_birth += usize::from(z < 1.0);
                fill += usize::from(z.floor() as u64 >= u64::from(e.max_particles));
                aged += usize::from(s.age != 0.0 || s.age_spread != 0.0);
            }
        }
    }
    assert_eq!((one_birth, fill, aged), (392, 395, 16));

    // P3 6.3: every motion record applied once to the standard particles.
    let standard = || {
        let p = |x: [f32; 3], w: [f32; 3], age: f32| Particle {
            position: x,
            previous: x,
            velocity: w,
            age,
            ..Particle::default()
        };
        vec![
            p([1.0, 2.0, 3.0], [0.5, -1.0, 2.0], 0.25),
            p([-2.0, 0.5, 1.0], [3.0, 0.0, -1.0], 1.0),
            p([0.5, -1.0, -0.5], [0.0; 3], 0.0),
        ]
    };
    let mut sums: HashMap<&str, (usize, [f64; 6])> = HashMap::new();
    for a in lib.effects.iter().flat_map(|e| &e.actions) {
        let name = type_name(&a.kind);
        if matches!(
            name,
            "source" | "killold" | "sink" | "targetcolor" | "targetsize" | "targetrotate"
        ) {
            continue;
        }
        let mut particles = standard();
        let mut time = 0.0;
        apply(
            a,
            &mut particles,
            100,
            &mut Random::new(1),
            0.033,
            &mut time,
        );
        let entry = sums.entry(name).or_default();
        entry.0 += 1;
        for p in &particles {
            for i in 0..3 {
                entry.1[i] += f64::from(p.position[i]);
                entry.1[3 + i] += f64::from(p.velocity[i]);
                assert!(!p.velocity[i].is_nan() && !p.position[i].is_nan(), "{name}");
            }
        }
    }
    let want: [(&str, usize, [f64; 6]); 16] = [
        (
            "move",
            1070,
            [-411.415, 1569.69, 3780.31, 3745.0, -1070.0, 1070.0],
        ),
        (
            "gravity",
            310,
            [-155.0, 465.0, 1085.0, 1085.2, -402.612, 308.362],
        ),
        ("gravitate", 17, [-8.5, 25.5, 59.5, 59.5, -17.0, 17.0]),
        (
            "orbitpoint",
            174,
            [-87.0, 261.0, 609.0, 616.622, -122.616, 153.888],
        ),
        (
            "vortex",
            133,
            [-55.4706, 199.412, 468.249, 465.5, -133.0, 133.0],
        ),
        (
            "randomaccel",
            35,
            [-17.5, 52.5, 122.5, 116.719, -32.334, 35.2177],
        ),
        (
            "randomdisplace",
            33,
            [-24.91, 51.3686, 116.893, 115.5, -33.0, 33.0],
        ),
        (
            "randomvelocity",
            2,
            [-1.0, 3.0, 7.0, 10.6791, 10.6791, 10.6791],
        ),
        ("damping", 14, [-7.0, 21.0, 49.0, 34.0675, -10.271, 10.898]),
        (
            "speedlimit",
            36,
            [-18.0, 54.0, 126.0, 147.148, -50.8238, 61.0689],
        ),
        ("jet", 1, [-0.5, 1.5, 3.5, 3.5, -0.998635, 1.0]),
        (
            "turbulence",
            58,
            [-29.0, 87.0, 203.0, 173.373, -51.8733, 47.1742],
        ),
        ("follow", 2, [-1.0, 3.0, 7.0, 7.00068, -2.00224, 1.99749]),
        (
            "scatter",
            15,
            [-7.5, 22.5, 52.5, 64.3555, -95.8072, 7.38167],
        ),
        ("bounce", 15, [-7.5, 22.5, 52.5, 47.1667, -13.6667, 17.6667]),
        (
            "targetvelocity",
            494,
            [-247.0, 741.0, 1729.0, 1470.66, -239.603, 522.283],
        ),
    ];
    for (name, records, want) in want {
        let (n, got) = sums[name];
        assert_eq!(n, records, "{name}");
        for i in 0..6 {
            assert!(
                (got[i] - want[i]).abs() <= want[i].abs() * 1e-4 + 2e-3,
                "{name} {i}: {} vs {}",
                got[i],
                want[i]
            );
        }
    }

    // P4 9.
    let killers: Vec<_> = (lib.effects.iter())
        .filter(|e| {
            e.actions.iter().any(|a| {
                matches!(
                    a.kind,
                    ActionKind::KillOld {
                        kill_younger: 1,
                        ..
                    }
                )
            })
        })
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(killers, ["ghoul\\fx-11-anomaly_wild-red-lights"]);
    let mut sinks: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for a in lib.effects.iter().flat_map(|e| &e.actions) {
        if let ActionKind::Sink {
            kill_inside,
            region,
        } = &a.kind
        {
            *sinks.entry((*kill_inside, region.kind)).or_default() += 1;
        }
    }
    let want: BTreeMap<(u32, u32), usize> = [
        ((1, 3), 29),
        ((0, 3), 22),
        ((1, 5), 16),
        ((1, 4), 10),
        ((0, 6), 6),
        ((0, 7), 6),
    ]
    .into_iter()
    .collect();
    assert_eq!(sinks, want);
}

/// Member effects first played, by update number, for the group `name`.
fn timeline(lib: &Library, name: &str, e: u32, updates: usize) -> (Vec<Vec<usize>>, Option<usize>) {
    let mut g = GroupInstance::new(lib, lib.group(name).unwrap()).unwrap();
    g.place(&IDENTITY, [0.0; 3], Mode::World);
    g.play();
    let mut random = Random::default();
    let mut was = vec![false; g.member_count()];
    let mut starts = Vec::new();
    let mut stopped = None;
    for k in 1..=updates {
        g.update(e, lib, &mut random);
        let mut now = Vec::new();
        for (i, w) in was.iter_mut().enumerate() {
            if !*w && g.member_effect(i).is_playing() {
                *w = true;
                now.push(i);
            }
        }
        starts.push(now);
        if stopped.is_none() && g.is_stopping() {
            stopped = Some(k);
        }
    }
    (starts, stopped)
}

#[test]
fn group_timelines_are_p6s() {
    let Some(d) = data() else { return };
    let lib = &d.library;
    let (starts, _) = timeline(lib, "explosions\\campfire_05", 33, 20);
    assert_eq!(starts[0], [0, 3, 4, 5, 6]);
    assert_eq!(starts[15], [2]);

    let expect = |starts: &[Vec<usize>], want: &[(usize, &[usize])]| {
        for (update, members) in want {
            assert_eq!(starts[update - 1], *members, "update {update}");
        }
        let total: usize = starts.iter().map(Vec::len).sum();
        assert_eq!(total, want.iter().map(|(_, m)| m.len()).sum::<usize>());
    };
    let (starts, stopped) = timeline(lib, "anomaly2\\teleport", 33, 40);
    expect(
        &starts,
        &[
            (1, &[0, 5, 7, 12]),
            (13, &[4]),
            (19, &[3]),
            (26, &[8]),
            (28, &[1, 6, 9]),
            (29, &[10]),
            (31, &[11]),
        ],
    );
    assert_eq!(stopped, Some(34));
    let (starts, stopped) = timeline(lib, "anomaly2\\teleport", 16, 75);
    expect(
        &starts,
        &[
            (1, &[0, 5, 7, 12]),
            (25, &[4]),
            (38, &[3]),
            (54, &[8]),
            (57, &[1, 6, 9]),
            (60, &[10]),
            (63, &[11]),
        ],
    );
    assert_eq!(stopped, Some(69));

    // P6 8: the first particle of `anomaly2\pux_blast00`'s member with an
    // empty per-particle child name, and the first death in
    // `static\bubble`'s member naming a missing death child, are fatal.
    for (name, error) in [
        ("anomaly2\\pux_blast00", "Particle effect doesn't exist"),
        ("static\\bubble", "Particle effect doesn't exist"),
    ] {
        let mut g = GroupInstance::new(lib, lib.group(name).unwrap()).unwrap();
        g.play();
        let mut random = Random::default();
        for _ in 0..2000 {
            g.update(33, lib, &mut random);
        }
        assert_eq!(g.fatal(), Some(error), "{name}");
    }
    // Every group that can be made (P6 9.1), on its timeline: those two
    // without the slots that end the game (their bits cleared; the members'
    // starts and the groups' stops do not depend on children).
    let mut patched = lib.clone();
    for g in &mut patched.groups {
        for m in &mut g.members {
            if m.flags & member_flags::PER_PARTICLE_CHILD != 0 && m.per_particle_child.is_empty() {
                m.flags &= !member_flags::PER_PARTICLE_CHILD;
            }
            if m.flags & member_flags::DEATH_CHILD != 0 && lib.effect(&m.death_child).is_none() {
                m.flags &= !member_flags::DEATH_CHILD;
            }
        }
    }
    let lib = &patched;
    let mut made = 0;
    let mut used = 0;
    let mut start_sum = 0;
    let mut timed = 0;
    let mut stop_sum = 0;
    for (i, group) in lib.groups.iter().enumerate() {
        let Ok(mut g) = GroupInstance::new(lib, i) else {
            continue;
        };
        made += 1;
        g.play();
        let mut random = Random::default();
        let want = (group.members.iter())
            .filter(|m| m.flags & member_flags::USED != 0)
            .count();
        let mut was = vec![false; g.member_count()];
        let mut started = 0;
        let mut stopped = false;
        let mut k = 0;
        loop {
            k += 1;
            g.update(33, lib, &mut random);
            for (j, w) in was.iter_mut().enumerate() {
                if !*w && g.member_effect(j).is_playing() {
                    *w = true;
                    started += 1;
                    start_sum += k;
                }
            }
            // Stopped deferred in the first update past the limit; when
            // nothing plays then, it also ends in that update.
            if !stopped && group.time_limit > 0.0 && g.time() > group.time_limit {
                stopped = true;
                timed += 1;
                stop_sum += k;
            }
            let all = started == want;
            if (all && (group.time_limit <= 0.0 || stopped)) || !g.is_playing() {
                break;
            }
            assert!(k < 30_000, "{}", group.name);
        }
        used += started;
    }
    assert_eq!(made, 248);
    assert_eq!(used, 973);
    assert_eq!(start_sum, 6_843);
    assert_eq!(timed, 159);
    assert_eq!(stop_sum, 153_160);
}

#[test]
fn placed_effects_are_p8s() {
    let Some(d) = data() else { return };
    let mut per_name: BTreeMap<String, usize> = BTreeMap::new();
    let mut sum = [0.0f64; 3];
    let mut identity = 0;
    let mut all: Vec<(String, Vec<PlacedRecord>)> = Vec::new();
    for ((name, bytes), (_, size, count)) in d.levels.iter().zip(LEVELS) {
        assert_eq!(bytes.len(), size, "{name}");
        let records = parse_placed(bytes).unwrap();
        assert_eq!(records.len(), count, "{name}");
        for r in &records {
            *per_name.entry(r.name.clone()).or_default() += 1;
            for (i, s) in sum.iter_mut().enumerate() {
                let mut t = r.transform[3][i];
                if i == 1 {
                    t += f32::from_bits(0x3C23_D70A);
                }
                *s += f64::from(t);
            }
            let rows = &r.transform;
            identity += usize::from(
                (0..3).all(|i| (0..3).all(|j| rows[i][j] == f32::from(u8::from(i == j)))),
            );
        }
        all.push((name.clone(), records));
    }
    assert_eq!(per_name.len(), 22);
    assert_eq!(per_name["static\\static_stancia2"], 37);
    assert_eq!(per_name["explosions\\campfire_05"], 14);
    assert_eq!(per_name["tests\\muhi"], 1);
    assert_eq!(identity, 106);
    for (got, want) in sum.iter().zip([37_576.653, 1_869.736, 12_318.863]) {
        assert!((got - want).abs() < 0.01, "{got}");
    }
    let lib = &d.library;
    let effects = per_name.keys().filter(|n| lib.effect(n).is_some()).count();
    let groups = per_name.keys().filter(|n| lib.group(n).is_some()).count();
    assert_eq!((effects, groups), (6, 16));
    let members: Vec<_> = (per_name.keys())
        .filter_map(|n| lib.group(n))
        .flat_map(|g| &lib.groups[g].members)
        .collect();
    assert_eq!(members.len(), 63);
    assert_eq!(members.iter().filter(|m| m.flags & 4 != 0).count(), 56);

    // l01_escape (P8 8).
    let l01 = &all[0].1;
    assert_eq!(l01[0].name, "tests\\muhi");
    assert_eq!(
        l01[0].transform[3].map(f32::to_bits),
        [0xC37F_B655, 0xC196_A9EA, 0xC2FE_7DA2, 0x3F80_0000]
    );

    // Every level's effects are made, placed and started, and run 10 s.
    let library = Arc::new(d.library.clone());
    for (name, records) in &all {
        let mut placed = PlacedEffects::new(library.clone(), records, 5_000).unwrap();
        for p in placed.effects() {
            assert!(p.instance.is_playing(), "{name}: {}", p.name);
        }
        let camera = placed.effects().next().unwrap().instance.sphere().centre;
        for _ in 0..600 {
            placed.frame(16, camera);
        }
        for p in placed.effects() {
            assert!(p.instance.is_playing(), "{name}: {} ended", p.name);
            if let Instance::Group(g) = &p.instance {
                assert!(g.fatal().is_none(), "{name}: {:?}", g.fatal());
            }
        }
        if name == "l01_escape" {
            let p = &placed.effects().next().unwrap();
            let Instance::Effect(e) = &p.instance else {
                panic!()
            };
            // Near the camera: about a 30 ms period, so it runs in real time; 17
            // births a second living 2 s fill its 7.
            assert!(p.period() <= 35, "{}", p.period());
            assert_eq!(e.particles().len(), 7);
            assert_eq!(e.position()[1].to_bits(), 0xC196_956F);
            let s = e.sphere();
            for q in e.particles() {
                let d: f32 = (0..3).map(|i| (q.position[i] - s.centre[i]).powi(2)).sum();
                assert!(d.sqrt() <= s.radius);
                // Its colour is set once at birth and has no colour action.
                assert_eq!(q.colour, 0xFF7F_7F7F);
            }
            eprintln!(
                "l01_escape after 10 s: {} particles, sphere {:?}, {} updates",
                e.particles().len(),
                s,
                p.updates()
            );
        }
    }
}
