// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Sound files and their parameters on the Russian edition (BYOA): the
//! verification of docs/formats/sound-ogg-params.md section 8 (the first
//! level's files, the digest and sums over every file) and sections 1 and 3,
//! and what decoding the first level's sounds costs (time, heap). Its own
//! test binary with a counting allocator; its tests run one at a time.
//! Timings are meaningful in release builds. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]
// A counting allocator needs `unsafe impl GlobalAlloc`; it only forwards to
// the system allocator.
#![allow(unsafe_code)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use sha2::{Digest, Sha256};
use volya_engine_core::config::{LEVEL, PathAliases};
use volya_engine_core::sound::{self, Sound, Sounds};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::audio::{VorbisStream, ogg_end_granule};
use volya_engine_formats::sound_params::{self, SoundParams};

struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            if size > layout.size() {
                let now = CURRENT.fetch_add(size - layout.size(), Ordering::Relaxed)
                    + (size - layout.size());
                PEAK.fetch_max(now, Ordering::Relaxed);
            } else {
                CURRENT.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The tests run one at a time, so the allocator counts one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

/// Bytes allocated now; resets the peak to it.
fn reset_peak() -> usize {
    let now = CURRENT.load(Ordering::Relaxed);
    PEAK.store(now, Ordering::Relaxed);
    now
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// The game's archives as shipped (db-archive.md), without the `mods`
/// directory: the spec examined `gamedata.db0` … `gamedata.dbd`.
fn with_tree(f: impl FnOnce(&dyn Vfs)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, None).unwrap();
    f(&tree);
}

/// The installer's path aliases (docs/formats/ltx-config.md section 10), with
/// `$level$` at `l01_escape`.
fn aliases() -> PathAliases {
    let mut aliases = PathAliases::parse(
        b"$game_data$ =     false| true | $fs_root$      | gamedata\\\n\
          $game_levels$ =   true | false| $game_data$    | levels\\\n\
          $game_sounds$ =   true | true | $game_data$    | sounds\\\n\
          $level$ =         false| false| $game_levels$\n",
        "",
    )
    .unwrap();
    aliases.set_dir(LEVEL, "gamedata\\levels\\l01_escape");
    aliases
}

/// `a` equals the spec's `expected`, shown rounded to 7 significant digits.
fn close(a: f32, expected: f64) -> bool {
    (f64::from(a) - expected).abs() <= expected.abs() * 5e-7
}

/// Section 8's table: name, min, max, volume, AI type, max AI.
const LEVEL_FILES: [(&str, f64, f64, f64, u32, f64); 11] = [
    ("nature\\new_flies1", 1.0, 20.0, 1.0, 0, 20.0),
    ("ambient\\sparks1", 0.2, 10.0, 1.0, 0, 10.0),
    (
        "scripts\\magnitofon\\magnitofon_2",
        0.4,
        15.0,
        1.0,
        0x0800_0080,
        1.0,
    ),
    (
        "script_replics\\radio\\message\\radio_noise_1",
        0.136_363_6,
        20.0,
        1.0,
        0,
        20.0,
    ),
    (
        "ambient\\indoors\\small_fan",
        0.5,
        15.0,
        1.0,
        0x0800_0080,
        15.0,
    ),
    ("music\\amb01_l", 1.0, 300.0, 1.0, 0, 300.0),
    ("music\\amb01_r", 1.0, 300.0, 1.0, 0, 300.0),
    ("music\\amb20_l", 1.0, 300.0, 1.0, 0, 300.0),
    ("music\\amb20_r", 1.0, 300.0, 1.0, 0, 300.0),
    ("music\\wasteland2_l", 1.0, 300.0, 1.0, 0, 300.0),
    ("music\\wasteland2_r", 1.0, 300.0, 1.0, 0, 300.0),
];

#[test]
fn first_level_files() {
    with_tree(|tree| {
        let aliases = aliases();
        let mut sounds = Sounds::new();
        let mut total_frames = 0u64;
        let mut total_decode = 0.0;
        for (name, min, max, volume, ai, max_ai) in LEVEL_FILES {
            let start = Instant::now();
            let sound = sounds.get(tree, &aliases, name).unwrap();
            let load = start.elapsed();
            assert_eq!(sound.path, format!("gamedata\\sounds\\{name}.ogg"));
            let p = sound.params;
            assert!(
                close(p.min_distance, min)
                    && close(p.max_distance, max)
                    && close(p.base_volume, volume)
                    && p.ai_sound_type == ai
                    && close(p.max_ai_distance, max_ai),
                "{name}: {p:?}"
            );
            // The same name, written otherwise, is the same sound.
            let again = sounds
                .get(tree, &aliases, &format!("{}.ogg", name.to_uppercase()))
                .unwrap();
            assert!(std::rc::Rc::ptr_eq(&sound, &again));

            // Decoding on demand: the heap holds the file and one block at a
            // time, never the PCM.
            let base = reset_peak();
            let start = Instant::now();
            let mut stream = sound.open(tree).unwrap();
            assert_eq!(
                (stream.channels(), stream.sample_rate()),
                (1, 44_100),
                "{name}"
            );
            let (mut frames, mut block) = (0u64, Vec::new());
            while stream.read_block(&mut block).unwrap() {
                assert!(block.iter().all(|s| (-1.0..=1.0).contains(s)), "{name}");
                frames += block.len() as u64;
                block.clear();
            }
            let decode = start.elapsed();
            let peak = PEAK.load(Ordering::Relaxed) - base;
            drop(stream);
            let file = read(tree, &sound.path).unwrap();
            let end = ogg_end_granule(&file).unwrap();
            eprintln!(
                "{name}: {:.2} s, file {:.2} MiB, loaded in {load:.1?}, decoded in {decode:.1?} \
                 ({:.0}x real time), peak heap {:.2} MiB (PCM would be {:.2} MiB), \
                 {frames} frames, end granule {end}",
                frames as f64 / 44_100.0,
                mib(file.len()),
                frames as f64 / 44_100.0 / decode.as_secs_f64(),
                mib(peak),
                mib(frames as usize * 4),
            );
            assert_eq!(frames, end, "{name}: decoded frames");
            // The file and the decoder's state, well under the PCM.
            assert!(peak < file.len() + (1 << 20), "{name}: peak {peak}");
            total_frames += frames;
            total_decode += decode.as_secs_f64();
        }
        assert_eq!(sounds.len(), LEVEL_FILES.len());
        // docs/behavior/sound-positional.md section 10: samples N and
        // duration D.
        for (name, samples, duration) in [
            ("music\\wasteland2_l", 5_518_127, 125_127),
            ("music\\wasteland2_r", 5_518_127, 125_127),
            ("ambient\\sparks1", 176_953, 4_012),
        ] {
            let sound = sounds.get(tree, &aliases, name).unwrap();
            assert_eq!(
                (sound.samples, sound.duration_ms()),
                (samples, duration),
                "{name}"
            );
        }
        assert!(sounds.take_warnings().is_empty());
        eprintln!(
            "all: {:.1} s of audio decoded in {total_decode:.2} s",
            total_frames as f64 / 44_100.0
        );
    });
}

/// Every file under `dir`, recursively, by path.
fn files(tree: &dyn Vfs, dir: &str, out: &mut Vec<String>) {
    for e in tree.read_dir(dir).unwrap() {
        let path = format!("{dir}/{}", e.name);
        match e.kind {
            EntryKind::File => out.push(path),
            EntryKind::Dir => files(tree, &path, out),
        }
    }
}

/// The parameters of every file (sections 1, 3, 6 and 8): counts, digest and
/// sums.
#[test]
fn every_file() {
    with_tree(|tree| {
        let start = Instant::now();
        let mut all = Vec::new();
        files(tree, "gamedata", &mut all);
        let oggs: Vec<_> = all.iter().filter(|p| p.ends_with(".ogg")).collect();
        assert_eq!(oggs.len(), 16_306);
        assert!(oggs.iter().all(|p| p.starts_with("gamedata/sounds/")));

        // Stored path → first user comment.
        let mut comments = BTreeMap::new();
        let (mut sums, mut versions) = ([0f64; 4], BTreeMap::new());
        for path in &oggs {
            let file = tree.open(path).unwrap();
            let stored = path["gamedata/".len()..].replace('/', "\\");
            let comment = sound_params::first_user_comment(file).unwrap();
            if let Some(c) = &comment {
                assert_eq!(c.len(), 24, "{stored}");
                let p = sound_params::parse_record(c).unwrap();
                *versions
                    .entry(u32::from_le_bytes(c[..4].try_into().unwrap()))
                    .or_insert(0) += 1;
                for (sum, v) in sums.iter_mut().zip([
                    p.min_distance,
                    p.max_distance,
                    p.base_volume,
                    p.max_ai_distance,
                ]) {
                    *sum += f64::from(v);
                }
                assert!(p.min_distance > 0.0 && p.min_distance <= p.max_distance);
                assert!(p.max_distance >= 0.1 && p.max_ai_distance >= 0.1);
            }
            comments.insert(stored, comment);
        }
        eprintln!(
            "read {} comment headers in {:.1?}",
            oggs.len(),
            start.elapsed()
        );

        let missing: Vec<_> = comments
            .iter()
            .filter(|(_, c)| c.is_none())
            .map(|(p, _)| p.as_str())
            .collect();
        assert_eq!(
            missing,
            [
                "sounds\\characters_voice\\scenario\\video\\intro_gsc_wp_r.ogg",
                "sounds\\scripts\\m_gameplay\\podoidi-blije.sfk.ogg",
            ]
        );
        assert_eq!(versions, BTreeMap::from([(3, 16_304)]));

        let mut text = String::new();
        for (path, comment) in &comments {
            text.push_str(path);
            text.push(' ');
            for b in comment.iter().flatten() {
                text.push_str(&format!("{b:02x}"));
            }
            text.push('\n');
        }
        let digest: String = Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            digest,
            "406b0717d2331713eef09cca9dd0c1efe31f3941efae7e67092a879c775c5345"
        );
        let rounded = sums.map(|s| (s * 1e6).round() / 1e6);
        assert_eq!(
            rounded,
            [38_288.879_544, 1_567_394.369_48, 16_268.079_999, 774_157.0]
        );
    });
}

/// The files without a record load with the defaults and a warning; the one
/// with a further `.` in its stored path is reached through its full name
/// only as the name lacking that part (section 2).
#[test]
fn files_without_a_record() {
    with_tree(|tree| {
        let aliases = aliases();
        let mut sounds = Sounds::new();
        let sound = sounds
            .get(
                tree,
                &aliases,
                "characters_voice\\scenario\\video\\intro_gsc_wp_r",
            )
            .unwrap();
        assert_eq!(sound.params, SoundParams::default());
        let warnings = sounds.take_warnings();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("no sound parameters"), "{warnings:?}");

        let err = sounds
            .get(tree, &aliases, "scripts\\m_gameplay\\podoidi-blije.sfk")
            .unwrap_err();
        assert!(
            err.0
                .contains("gamedata\\sounds\\scripts\\m_gameplay\\podoidi-blije.ogg"),
            "{err}"
        );
        // Loaded directly by its path, it has the defaults too.
        let (sound, warning) = Sound::load(
            tree,
            "podoidi-blije",
            "gamedata\\sounds\\scripts\\m_gameplay\\podoidi-blije.sfk.ogg",
        )
        .unwrap();
        assert_eq!(sound.params, SoundParams::default());
        assert!(warning.is_some());
    });
}

/// Ogg's page checksum (RFC 3533: CRC-32, polynomial 0x04c11db7, not
/// reflected, initial value 0).
fn ogg_crc(page: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &b in page {
        crc ^= u32::from(b) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// A sound file at another sample rate is fatal (sections 1 and 5): a real
/// file with its identification header's rate changed.
#[test]
fn another_sample_rate_is_fatal() {
    with_tree(|tree| {
        let mut file = read(tree, "gamedata/sounds/ambient/sparks1.ogg").unwrap();
        // The first page holds only the identification packet: 27 header
        // bytes, its segment table, then `\x01vorbis`, version, channels,
        // rate (Vorbis I section 4.2.2).
        let segments = usize::from(file[26]);
        let packet = 27 + segments;
        assert_eq!(&file[packet..packet + 7], b"\x01vorbis");
        let rate = packet + 12;
        assert_eq!(&file[rate..rate + 4], 44_100u32.to_le_bytes());
        file[rate..rate + 4].copy_from_slice(&22_050u32.to_le_bytes());
        let len = packet
            + file[27..packet]
                .iter()
                .map(|&s| usize::from(s))
                .sum::<usize>();
        file[22..26].fill(0);
        let crc = ogg_crc(&file[..len]);
        file[22..26].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(
            VorbisStream::open(Cursor::new(&file))
                .unwrap()
                .sample_rate(),
            22_050
        );

        let err = load_bytes(&file).unwrap_err();
        assert!(err.0.contains("1 channels at 22050 Hz"), "{err}");
    });
}

/// [`Sound::load`] over a one-file tree holding `bytes`.
fn load_bytes(bytes: &[u8]) -> Result<Sound, sound::Fatal> {
    // Bazel's test directory, else Cargo's.
    let tmp = std::env::var_os("TEST_TMPDIR")
        .map(std::path::PathBuf::from)
        .or_else(|| option_env!("CARGO_TARGET_TMPDIR").map(std::path::PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    let dir = tmp.join("sound-load-bytes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("x.ogg"), bytes).unwrap();
    let tree = StdVfs::new(&dir);
    let result = Sound::load(&tree, "x", "x.ogg").map(|(s, _)| s);
    std::fs::remove_dir_all(&dir).unwrap();
    result
}
