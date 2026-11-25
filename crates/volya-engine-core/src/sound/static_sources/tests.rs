// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The format and the play rule of docs/behavior/sound-static-sources.md,
//! with `l01_escape`'s sources (section 5) and their consequences
//! (section 4.2).

use std::io;

use super::*;
use crate::sound::Sound;
use crate::sound::system::NoDevice;
use crate::vfs::db::DbVfs;
use volya_engine_formats::sound_params::SoundParams;

fn chunk(out: &mut Vec<u8>, id: u32, data: &[u8]) {
    out.extend(id.to_le_bytes());
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
}

fn record_bytes(r: &SourceRecord) -> Vec<u8> {
    let mut b = r.name.as_bytes().to_vec();
    b.push(0);
    for f in r.position.iter().chain([&r.volume, &r.frequency]) {
        b.extend(f.to_le_bytes());
    }
    for w in [
        r.active_from,
        r.active_to,
        r.play_min,
        r.play_max,
        r.pause_min,
        r.pause_max,
    ] {
        b.extend(w.to_le_bytes());
    }
    b
}

fn file(records: &[SourceRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, r) in records.iter().enumerate() {
        let mut payload = Vec::new();
        chunk(&mut payload, 0, &record_bytes(r));
        chunk(&mut out, i as u32, &payload);
    }
    out
}

fn source(name: &str, position: Vec3, volume: f32) -> SourceRecord {
    SourceRecord {
        name: name.into(),
        position,
        volume,
        frequency: 1.0,
        active_from: 0,
        active_to: 0,
        play_min: 0,
        play_max: 0,
        pause_min: 0,
        pause_max: 0,
    }
}

/// `levels\l01_escape\level.snd_static` as section 5 tabulates it.
fn escape() -> Vec<SourceRecord> {
    vec![
        source("nature\\new_flies1", [-255.7179, -18.8127, -127.2483], 0.3),
        source("ambient\\sparks1", [-245.3441, -23.2075, -135.9194], 0.5),
        source(
            "scripts\\magnitofon\\magnitofon_2",
            [-245.3842, -23.9443, -133.3607],
            0.5,
        ),
        SourceRecord {
            active_from: 7_200_000,
            active_to: 10_800_000,
            ..source(
                "script_replics\\radio\\message\\radio_noise_1",
                [-245.1295, -23.9443, -134.5009],
                0.5,
            )
        },
        source(
            "ambient\\indoors\\small_fan",
            [-243.9568, -22.9818, -133.2737],
            1.0,
        ),
    ]
}

fn sound(name: &str, samples: u64) -> Sound {
    Sound {
        name: name.into(),
        path: format!("gamedata\\sounds\\{name}.ogg"),
        params: SoundParams {
            min_distance: 1.0,
            max_distance: 50.0,
            base_volume: 1.0,
            ai_sound_type: 0,
            max_ai_distance: 50.0,
        },
        samples,
    }
}

fn system_with(records: &[SourceRecord]) -> (SoundSystem, StaticSources) {
    let mut s = SoundSystem::new(16);
    for r in records {
        // 4,000 ms of samples.
        s.preload(sound(&r.name, 176_400));
    }
    let tree = DbVfs::<io::Empty>::new();
    let sources = StaticSources::create(&mut s, &tree, &PathAliases::default(), records).unwrap();
    (s, sources)
}

#[test]
fn parses_the_records_of_section_5() {
    let records = escape();
    let bytes = file(&records);
    // Section 6.1: 435 bytes; records of 62, 60, 76, 87, 70 bytes.
    assert_eq!(bytes.len(), 435);
    let lengths: Vec<usize> = records.iter().map(|r| record_bytes(r).len()).collect();
    assert_eq!(lengths, [62, 60, 76, 87, 70]);
    let parsed = parse(&bytes).unwrap();
    assert_eq!(parsed, records);
    // Section 6.1: volume bit patterns.
    let bits: Vec<u32> = parsed.iter().map(|r| r.volume.to_bits()).collect();
    assert_eq!(
        bits,
        [
            0x3E99_999A,
            0x3F00_0000,
            0x3F00_0000,
            0x3F00_0000,
            0x3F80_0000
        ]
    );
    // Bytes after the last field are ignored; a missing sub-chunk 0 fails.
    let mut longer = Vec::new();
    let mut payload = Vec::new();
    let mut rec = record_bytes(&records[0]);
    rec.extend([1, 2, 3]);
    chunk(&mut payload, 0, &rec);
    chunk(&mut payload, 7, b"other");
    chunk(&mut longer, 0, &payload);
    assert_eq!(parse(&longer).unwrap(), &records[..1]);
    let mut missing = Vec::new();
    let mut payload = Vec::new();
    chunk(&mut payload, 1, &record_bytes(&records[0]));
    chunk(&mut missing, 0, &payload);
    assert!(parse(&missing).is_err());
}

/// 05:30:00 and 02:30:00 of the day (ms).
const START: u32 = 19_800_000;
const NIGHT: u32 = 9_000_000;

#[test]
fn escape_at_new_game_starts_four_looped_sources() {
    let records = escape();
    let (mut s, mut sources) = system_with(&records);
    sources.frame(&mut s, 100, START).unwrap();
    s.update(100, &mut NoDevice);
    let playing: Vec<bool> = sources
        .sources
        .iter()
        .map(|src| s.has_emitter(src.sound))
        .collect();
    assert_eq!(playing, [true, true, true, false, true]);
    let emitters = s.emitters();
    assert!(emitters.iter().all(|e| e.looped && !e.two_d));
    assert_eq!(emitters[1].position, records[1].position);
    let volumes: Vec<f32> = emitters.iter().map(|e| e.volume).collect();
    assert_eq!(volumes, [0.3, 0.5, 0.5, 1.0]);
    assert!(emitters.iter().all(|e| e.frequency == 1.0));
    // Their stop time is never; next start unchanged.
    assert!(
        sources
            .sources
            .iter()
            .enumerate()
            .all(|(i, src)| i == 3 || (src.stop_time == u32::MAX && src.next_start == 0))
    );
    // Source 3 starts at 02:00 and is stopped with fade at 03:00.
    sources.frame(&mut s, 200, NIGHT).unwrap();
    s.update(200, &mut NoDevice);
    assert!(s.has_emitter(sources.sources[3].sound));
    sources.frame(&mut s, 300, 10_800_000).unwrap();
    let radio = s
        .emitters()
        .into_iter()
        .find(|e| e.name.contains("radio_noise_1"))
        .unwrap();
    assert!(radio.stopping);
    // It counts as playing while it fades: not restarted.
    sources.frame(&mut s, 316, NIGHT).unwrap();
    assert!(
        s.emitters()
            .iter()
            .any(|e| e.name.contains("radio") && e.stopping)
    );
}

#[test]
fn a_once_source_with_a_pause_restarts_after_duration_and_pause() {
    // `l05_bar`'s kitchen sources: once, a pause of 2,000 … 29,999 ms.
    let rec = SourceRecord {
        pause_min: 2_000,
        pause_max: 30_000,
        ..source("ambient\\bar\\kitchen_1", [0.0; 3], 1.0)
    };
    let (mut s, mut sources) = system_with(std::slice::from_ref(&rec));
    sources.frame(&mut s, 1_000, START).unwrap();
    let e = &s.emitters()[0];
    assert!(!e.looped);
    let src = &sources.sources[0];
    assert_eq!(src.stop_time, u32::MAX);
    // The shared generator's first draw with seed 1 is 41: 2,000 + 41.
    assert_eq!(src.next_start, 1_000 + 4_000 + 2_041);
    let mut now = 1_000;
    let mut starts = vec![now];
    while now < 20_000 {
        now += 20;
        s.update(u64::from(now), &mut NoDevice);
        let had = s.has_emitter(sources.sources[0].sound);
        sources.frame(&mut s, now, START).unwrap();
        if !had && s.has_emitter(sources.sources[0].sound) {
            starts.push(now);
        }
    }
    // The next start is the first frame at or after 7,041.
    assert_eq!(starts[1], 7_060);
}

#[test]
fn a_looped_source_with_play_and_pause_ranges() {
    let rec = SourceRecord {
        play_min: 1_000,
        play_max: 2_000,
        pause_min: 500,
        pause_max: 600,
        ..source("a", [0.0; 3], 1.0)
    };
    let (mut s, mut sources) = system_with(std::slice::from_ref(&rec));
    sources.frame(&mut s, 10, START).unwrap();
    let src = &sources.sources[0];
    // Draws 41 then 18,467: stop = 10 + 1,041; next = stop + 500 + 67.
    assert_eq!(src.stop_time, 1_051);
    assert_eq!(src.next_start, 1_051 + 567);
    assert!(s.emitters()[0].looped);
    s.update(10, &mut NoDevice);
    sources.frame(&mut s, 1_051, START).unwrap();
    assert!(s.emitters()[0].stopping);
}

#[test]
fn windows_compare_signed_and_never_wrap() {
    let (_, sources) = system_with(&escape());
    let radio = &sources.sources[3];
    assert!(!radio.active(START));
    assert!(radio.active(7_200_000));
    assert!(!radio.active(10_800_000));
    let mut wrapping = radio.clone();
    wrapping.record.active_from = 79_200_000;
    wrapping.record.active_to = 21_600_000;
    assert!((0..24).all(|h| !wrapping.active(h * 3_600_000)));
}

#[test]
fn identical_records_are_separate_sources() {
    let r = source("a", [1.0, 2.0, 3.0], 1.0);
    let (mut s, mut sources) = system_with(&[r.clone(), r]);
    sources.frame(&mut s, 10, START).unwrap();
    assert_eq!(s.emitters().len(), 2);
}

#[test]
fn a_range_with_equal_bounds_is_fatal() {
    // 4.2: with b = a the original divides by zero and fails.
    let rec = SourceRecord {
        pause_min: 500,
        pause_max: 500,
        ..source("a", [0.0; 3], 1.0)
    };
    let (mut s, mut sources) = system_with(std::slice::from_ref(&rec));
    assert!(sources.frame(&mut s, 10, START).is_err());
}
