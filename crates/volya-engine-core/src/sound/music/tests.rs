// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The track list and the rule of docs/behavior/sound-level-music.md, with
//! the examples of sections 2.2 and 8.3.

use std::io;

use super::*;
use crate::sound::Sound;
use crate::sound::system::NoDevice;
use crate::vfs::db::DbVfs;
use volya_engine_formats::sound_params::SoundParams;

#[test]
fn entry_values_scan_as_the_c_library_does() {
    // Section 2.2's examples.
    let p = TrackParams::parse("6,22,0.2,20,40");
    assert_eq!(
        (p.start, p.end, p.pause_min, p.pause_max),
        (21_600_000, 79_200_000, 20_000, 40_000)
    );
    assert_eq!(p.volume.to_bits(), 0x3E4C_CCCD);
    let p = TrackParams::parse("0,0,0.2,20,40");
    assert_eq!((p.start, p.end), (0, 0));
    let p = TrackParams::parse("1,2,0.5,30,30");
    assert_eq!(
        (p.start, p.end, p.volume, p.pause_min, p.pause_max),
        (3_600_000, 7_200_000, 0.5, 30_000, 31_000)
    );
    let p = TrackParams::parse("6");
    assert_eq!(
        (p.start, p.end, p.volume, p.pause_min, p.pause_max),
        (21_600_000, 0, 1.0, 0, 1_000)
    );
    let p = TrackParams::parse("6,22,x");
    assert_eq!(
        (p.start, p.end, p.volume, p.pause_min, p.pause_max),
        (21_600_000, 79_200_000, 1.0, 0, 1_000)
    );
}

const HOUR: u32 = 3_600_000;

#[test]
fn escape_windows() {
    // Section 8.3, M1.
    let day = TrackParams::parse("6,22,0.2,20,40");
    let night = TrackParams::parse("22,6,0.2,20,40");
    let ms = |h: u32, m: u32, s: u32, ms: u32| h * HOUR + m * 60_000 + s * 1_000 + ms;
    for tau in [ms(5, 30, 0, 0), ms(5, 59, 59, 999)] {
        assert!(!day.active(tau));
    }
    for tau in [ms(6, 0, 0, 0), ms(12, 0, 0, 0), ms(21, 59, 59, 999)] {
        assert!(day.active(tau));
    }
    for tau in [ms(22, 0, 0, 0), ms(23, 30, 0, 0), 0, ms(3, 0, 0, 0)] {
        assert!(!day.active(tau));
    }
    assert!((0..86_400).all(|s| !night.active(s * 1_000)));
}

fn config(text: &str) -> Config {
    volya_engine_formats::ltx::parse(text.as_bytes()).unwrap()
}

#[test]
fn the_track_list_of_a_level() {
    let game = config(
        "[l01_escape]\nmusic_tracks = l01_escape_musics\n\
         [l01_escape_musics]\nmusic\\amb20 = 22, 6, 0.2, 20, 40\nmusic\\amb01 = 6, 22, 0.2, 20, 40\n\
         [l05_bar]\nweathers = default\n\
         [l_empty]\nmusic_tracks =\n\
         [l_missing]\nmusic_tracks = nowhere\n",
    );
    let list = track_list(&game, "l01_escape").unwrap();
    // Entry order (sorted by key): amb01 before amb20.
    let names: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["music\\amb01", "music\\amb20"]);
    assert_eq!(list[0].1, TrackParams::parse("6,22,0.2,20,40"));
    assert!(track_list(&game, "l05_bar").unwrap().is_empty());
    assert!(track_list(&game, "l_empty").unwrap().is_empty());
    assert!(track_list(&game, "testers_mp_pool").unwrap().is_empty());
    // The existence test does not lowercase.
    assert!(track_list(&game, "L01_escape").unwrap().is_empty());
    assert!(track_list(&game, "l_missing").is_err());
}

fn sound(name: &str, samples: u64) -> Sound {
    Sound {
        name: name.into(),
        path: format!("gamedata\\sounds\\{name}.ogg"),
        params: SoundParams {
            min_distance: 1.0,
            max_distance: 300.0,
            base_volume: 1.0,
            ai_sound_type: 0,
            max_ai_distance: 300.0,
        },
        samples,
    }
}

/// `l01_escape`'s tracks; `amb01` of 521,142 ms (22,982,378 samples).
fn escape(system: &mut SoundSystem) -> LevelMusic {
    for (name, samples) in [("music\\amb01", 22_982_378), ("music\\amb20", 14_229_607)] {
        system.preload(sound(&format!("{name}_l"), samples));
        system.preload(sound(&format!("{name}_r"), samples));
    }
    let list = [
        (
            "music\\amb01".to_owned(),
            TrackParams::parse("6,22,0.2,20,40"),
        ),
        (
            "music\\amb20".to_owned(),
            TrackParams::parse("22,6,0.2,20,40"),
        ),
    ];
    let tree = DbVfs::<io::Empty>::new();
    LevelMusic::create(system, &tree, &PathAliases::default(), &list).unwrap()
}

/// τ at `now` for a first run at now = 100,000, τ = 05:30:00, game time 10
/// times the clock (section 8.3).
fn tau(now: u32) -> u32 {
    19_800_000 + (now - 100_000) * 10
}

#[test]
fn new_game_waits_for_six_then_plays_amb01() {
    // Section 8.3, M3 and M4: frames every 1 ms, n = 0 at every draw. Our
    // generator is not 0 at every draw, but with one active track any n
    // picks it; the pause after it uses n, so the M4 check below takes it
    // from the generator.
    let mut s = SoundSystem::new(16);
    let mut music = escape(&mut s);
    let mut checks = Vec::new();
    let mut started = None;
    let mut now = 100_000u32;
    while started.is_none() {
        let before = music.next_check;
        music.frame(&mut s, now, tau(now));
        if music.current.is_some() {
            started = Some(now);
        } else if music.next_check != before {
            checks.push(now);
        }
        now += 1;
    }
    assert_eq!(&checks[..2], [100_000, 110_001]);
    assert_eq!(checks.len(), 18);
    assert_eq!(checks[17], 270_017);
    assert_eq!(started, Some(280_018));
    assert_eq!(music.current_name(), Some("music\\amb01"));
    // Both channels: 2D, not looped, at (∓0.5, 0, 0.3), volume 0.2.
    let e = s.emitters();
    assert_eq!(e.len(), 2);
    assert!(e.iter().all(|e| e.two_d && !e.looped && e.volume == 0.2f32));
    assert_eq!(e[0].position, [-0.5, 0.0, 0.3]);
    assert_eq!(e[1].position, [0.5, 0.0, 0.3]);
    // The track plays its 521,142 ms: started at the update at 280,018, it
    // ends at 801,160; then the pause is drawn.
    s.update(280_018, &mut NoDevice);
    let mut now = 280_019u32;
    loop {
        s.update(u64::from(now), &mut NoDevice);
        music.frame(&mut s, now, tau(now));
        if music.current.is_none() {
            break;
        }
        now += 1;
    }
    assert_eq!(now, 801_160);
    let pause = music.next_check - now;
    assert!((20_000..40_000).contains(&pause), "{pause}");
}

#[test]
fn a_track_is_not_stopped_when_its_window_ends() {
    // Section 8.3, M5: started at 21:59:59 it plays to its end.
    let mut s = SoundSystem::new(16);
    let mut music = escape(&mut s);
    let late = 22 * HOUR - 1_000;
    music.frame(&mut s, 1, late);
    assert!(music.current.is_some());
    s.update(1, &mut NoDevice);
    music.frame(&mut s, 2, 22 * HOUR + 10);
    s.update(2, &mut NoDevice);
    assert!(s.emitters().iter().all(|e| !e.stopping));
    assert!(music.current.is_some());
}

#[test]
fn the_pause_draw_is_not_uniform() {
    // Section 8.3, M2: over all 32,768 values of n, 20,000 … 32,767 occur
    // twice and 32,768 … 39,999 once.
    let mut counts = vec![0u32; 40_000];
    for n in 0..32_768 {
        counts[range(n, 20_000, 40_000).unwrap() as usize] += 1;
    }
    assert!((20_000..32_768).all(|v| counts[v] == 2));
    assert!((32_768..40_000).all(|v| counts[v] == 1));
}
