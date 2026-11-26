// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Vorbis decoding on the Russian edition's sounds. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "support/gog_archives.rs"]
mod gog_archives;

use std::fs::File;
use std::io::{BufReader, Cursor};
use std::time::Instant;

use volya_engine_formats::audio::{VorbisStream, ogg_end_granule};
use volya_engine_formats::db::{Archive, Scrambling};

fn archive(name: &str) -> Option<Archive<BufReader<File>>> {
    let cached = gog_archives::archives()?
        .into_iter()
        .find(|a| a.path == name)?;
    Some(
        Archive::open(
            BufReader::new(File::open(&cached.file).unwrap()),
            Scrambling::for_path(name),
        )
        .unwrap(),
    )
}

/// The rate, the channels and every sample of `data`.
fn decode_all(data: &[u8]) -> (u32, u16, Vec<f32>) {
    let mut stream = VorbisStream::open(Cursor::new(data)).unwrap();
    let mut samples = Vec::new();
    while stream.read_block(&mut samples).unwrap() {}
    assert!(samples.iter().all(|s| (-1.0..=1.0).contains(s)));
    (stream.sample_rate(), stream.channels(), samples)
}

/// Up to `n` samples of `data` after skipping `skip` frames.
fn after_skip(data: &[u8], skip: u64, n: usize) -> (u64, Vec<f32>) {
    let mut stream = VorbisStream::open(Cursor::new(data)).unwrap();
    let skipped = stream.skip(skip).unwrap();
    let mut rest = Vec::new();
    while rest.len() < n && stream.read_block(&mut rest).unwrap() {}
    rest.truncate(n);
    (skipped, rest)
}

#[test]
fn decodes_like_the_reference() {
    let Some(mut dba) = archive("gamedata.dba") else {
        return;
    };
    let entry = dba
        .entries()
        .iter()
        .find(|e| e.path == "sounds\\characters_voice\\scenario\\video\\intro_gsc_wp_l.ogg")
        .unwrap()
        .clone();
    let data = dba.read(&entry).unwrap();
    // Reference: ffmpeg decodes 1,144,576 mono samples at 44.1 kHz.
    let started = Instant::now();
    let (rate, channels, all) = decode_all(&data);
    let decoding = started.elapsed();
    assert_eq!((rate, channels, all.len()), (44_100, 1, 1_144_576));
    // The last page's granule gives the length without decoding.
    assert_eq!(ogg_end_granule(&data), Some(1_144_576));

    // Rewinding restarts from the first sample.
    let mut stream = VorbisStream::open(Cursor::new(data.as_slice())).unwrap();
    let (mut first, mut again) = (Vec::new(), Vec::new());
    stream.read_block(&mut first).unwrap();
    while stream.read_block(&mut Vec::new()).unwrap() {}
    stream.rewind().unwrap();
    stream.read_block(&mut again).unwrap();
    assert_eq!(first, again);

    // What follows a skip is the stream from that frame, near (decoded)
    // or far (sought by pages, #309).
    for skip in [0, 1, 1_000, 44_100, 100_000, 500_000, 1_000_000, 1_140_000] {
        let (skipped, rest) = after_skip(&data, skip, 10_000);
        assert_eq!(skipped, skip);
        let at = skip as usize;
        assert_eq!(rest, all[at..(at + 10_000).min(all.len())], "skip {skip}");
    }
    let mut stream = VorbisStream::open(Cursor::new(data.as_slice())).unwrap();
    assert_eq!(stream.skip(u64::MAX).unwrap(), 1_144_576);
    assert!(!stream.read_block(&mut Vec::new()).unwrap());
    // From a frame already reached.
    let mut stream = VorbisStream::open(Cursor::new(data.as_slice())).unwrap();
    let mut first = Vec::new();
    stream.read_block(&mut first).unwrap();
    assert_eq!(stream.skip(700_000).unwrap(), 700_000);
    let mut rest = Vec::new();
    while rest.len() < 1_000 && stream.read_block(&mut rest).unwrap() {}
    let at = first.len() + 700_000;
    assert_eq!(rest[..1_000], all[at..at + 1_000]);

    // A far skip costs a few pages, not the decode before it.
    let started = Instant::now();
    after_skip(&data, 1_100_000, 0);
    eprintln!(
        "skip to frame 1,100,000: {:.2?}; decoding the whole stream: {decoding:.2?}",
        started.elapsed()
    );
}

#[test]
fn decodes_every_sound_of_an_archive() {
    let Some(mut db9) = archive("gamedata.db9") else {
        return;
    };
    let sounds: Vec<_> = db9
        .entries()
        .iter()
        .filter(|e| e.path.to_ascii_lowercase().ends_with(".ogg"))
        .cloned()
        .collect();
    assert_eq!(sounds.len(), 1_379);
    for entry in sounds {
        let data = db9.read(&entry).unwrap();
        let (rate, channels, samples) = decode_all(&data);
        let frames = samples.len() / usize::from(channels.max(1));
        assert!(
            matches!(rate, 11_025 | 22_050 | 44_100 | 48_000),
            "{}: {rate} Hz",
            entry.path
        );
        assert!(
            (1..=2).contains(&channels),
            "{}: {channels} channels",
            entry.path
        );
        assert!(frames > 0, "{}: empty", entry.path);
        // Skipping to two thirds equals decoding up to there.
        let skip = frames * 2 / 3;
        let at = skip * usize::from(channels);
        let (skipped, rest) = after_skip(&data, skip as u64, 4_096);
        assert_eq!(skipped, skip as u64, "{}", entry.path);
        let expected = &samples[at..(at + 4_096).min(samples.len())];
        assert_eq!(rest, expected, "{}", entry.path);
    }
}
