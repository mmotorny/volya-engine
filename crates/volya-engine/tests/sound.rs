// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The positional sound system on the mixer with the Russian edition's
//! first-level sounds (BYOA; docs/behavior/sound-positional.md): sounds
//! placed around a listener are heard by side and distance, and what the
//! update and the mixing cost with every voice busy. Timings are
//! meaningful in release builds. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::{Duration, Instant};

use volya_engine::audio::SoundHost;
use volya_engine::core::audio::{Category, Mixer};
use volya_engine::core::config::{LEVEL, PathAliases};
use volya_engine::core::sound::system::{Listener, Phase, PlayFlags};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::INSTALLER_FSGAME;

const RATE: u32 = 48_000;

fn rms(samples: impl Iterator<Item = f32>) -> f32 {
    let (sum, n) = samples.fold((0.0, 0), |(s, n), x| (s + x * x, n + 1));
    (sum / n.max(1) as f32).sqrt()
}

#[test]
fn positional_sounds_on_the_mixer() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let mut aliases = PathAliases::parse(INSTALLER_FSGAME.as_bytes(), "").unwrap();
    aliases.set_dir(LEVEL, "gamedata\\levels\\l01_escape");

    // The level's looped static sounds (S3), heard one at a time: on the
    // right, on the left, beyond its max distance (20 m).
    let looped = PlayFlags {
        looped: true,
        ..PlayFlags::default()
    };
    for (position, heard) in [
        ([0.5, 0.0, 0.0], Some(true)),
        ([-0.5, 0.0, 0.0], Some(false)),
        ([25.0, 0.0, 0.0], None),
    ] {
        let mut host = SoundHost::new(24);
        let mut mixer = Mixer::new(RATE);
        host.set_listener(Listener::default());
        let id = host
            .system
            .create(&tree, &aliases, "nature\\new_flies1", Category::Effects)
            .unwrap();
        host.system.play_at(id, position, looped);
        let mut out = Vec::new();
        for frame in 0..30 {
            host.update(f64::from(frame) * 16.0, Some(&mut mixer), Some(&tree));
            out.extend(mixer.mix(RATE as usize * 16 / 1000));
        }
        let left = rms(out.iter().step_by(2).copied());
        let right = rms(out.iter().skip(1).step_by(2).copied());
        eprintln!("{position:?}: RMS left {left:.4}, right {right:.4}");
        match heard {
            Some(true) => assert!(right > 0.001 && left < 1e-6),
            Some(false) => assert!(left > 0.001 && right < 1e-6),
            None => {
                assert_eq!((left, right), (0.0, 0.0));
                assert_eq!(host.system.emitters()[0].phase, Phase::Silent);
            }
        }
    }

    // Every voice busy: 32 looped emitters of the level's files around the
    // listener for 10 s at 60 updates per second, each update's output
    // mixed. The 8 farthest stay silent.
    let names = [
        "nature\\new_flies1",
        "ambient\\sparks1",
        "scripts\\magnitofon\\magnitofon_2",
        "script_replics\\radio\\message\\radio_noise_1",
        "ambient\\indoors\\small_fan",
        "music\\amb01_l",
        "music\\amb20_l",
        "music\\wasteland2_l",
    ];
    let mut host = SoundHost::new(24);
    let mut mixer = Mixer::new(RATE);
    let start = Instant::now();
    for i in 0..32 {
        let id = host
            .system
            .create(&tree, &aliases, names[i % names.len()], Category::Effects)
            .unwrap();
        let angle = i as f32 * 0.7;
        let distance = 0.1 + i as f32 * 0.05;
        host.system.play_at(
            id,
            [distance * angle.cos(), 0.0, distance * angle.sin()],
            looped,
        );
    }
    eprintln!("created 32 sounds in {:.1?}", start.elapsed());
    let (mut update, mut mix, mut worst) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let frames = 600;
    for frame in 0..frames {
        let t = Instant::now();
        host.update(
            f64::from(frame) * 1000.0 / 60.0,
            Some(&mut mixer),
            Some(&tree),
        );
        let u = t.elapsed();
        let t = Instant::now();
        mixer.mix(RATE as usize / 60);
        let m = t.elapsed();
        update += u;
        mix += m;
        worst = worst.max(u + m);
    }
    let playing = host
        .system
        .emitters()
        .iter()
        .filter(|e| e.phase == Phase::Playing)
        .count();
    eprintln!(
        "{playing} playing of 32: per frame update {:.1?}, mix {:.1?}, worst {worst:.1?}",
        update / frames,
        mix / frames
    );
    assert_eq!(playing, 24);
}
