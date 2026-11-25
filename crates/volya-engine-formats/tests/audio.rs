// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Vorbis decoding on the Russian edition's sounds. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "support/gog_archives.rs"]
mod gog_archives;

use std::fs::File;
use std::io::{BufReader, Cursor};

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

fn decode_all(data: &[u8]) -> (u32, u16, usize) {
    let mut stream = VorbisStream::open(Cursor::new(data)).unwrap();
    let mut samples = Vec::new();
    while stream.read_block(&mut samples).unwrap() {}
    assert!(samples.iter().all(|s| (-1.0..=1.0).contains(s)));
    (
        stream.sample_rate(),
        stream.channels(),
        samples.len() / usize::from(stream.channels()),
    )
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
    assert_eq!(decode_all(&data), (44_100, 1, 1_144_576));
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

    // Skipping decodes and drops whole frames: what follows is the stream
    // from that frame.
    let mut all = Vec::new();
    stream.rewind().unwrap();
    while all.len() < 100_000 && stream.read_block(&mut all).unwrap() {}
    for skip in [0, 1, 1_000, 44_100] {
        stream.rewind().unwrap();
        assert_eq!(stream.skip(skip).unwrap(), skip);
        let mut rest = Vec::new();
        while rest.len() < 10_000 && stream.read_block(&mut rest).unwrap() {}
        assert_eq!(rest[..10_000], all[skip as usize..skip as usize + 10_000]);
    }
    stream.rewind().unwrap();
    assert_eq!(stream.skip(u64::MAX).unwrap(), 1_144_576);
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
        let (rate, channels, frames) = decode_all(&data);
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
    }
}
