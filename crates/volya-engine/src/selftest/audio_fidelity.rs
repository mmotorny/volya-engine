// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The `audio-fidelity` suite's walks (#838, `tests/fidelity/walks.rs`):
//! each a check named after its walk. A New Game reached as the Windows
//! capture reaches it, in its frames of `FRAME_MS`, from the walk's
//! configuration file (`audio_ltx`, the bytes the capture starts the
//! original with); the walk's keys held at its frames; and our audio
//! rendered offline in step with the frames, with no output device: after
//! each frame's update and sound update the mixer mixes exactly the frame's
//! samples. Kept: `<walk>.wav` (ours, 16-bit stereo at 44,100 Hz),
//! `<walk>.frames.txt` (our frames' samples, the capture's table) and
//! `<walk>.report.txt`; and the frames where the player starts and ends,
//! as the capture keeps the original's: `<walk>-start` (the last frame
//! before the keys) and `<walk>-end` (the walk's last), which the suite
//! compares with the original's as the fidelity suite compares its views. With the original's capture (`<walk>.wav` and
//! `<walk>.frames.txt`, the suite's references) both recordings' onsets are
//! found, given their frames, told apart by material against the
//! fingerprints of every footstep sound of the player's material pairs, and
//! compared step by step (`crate::onsets`); a report, never a failure.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::title_menu::key;
use super::{Context, Outcome, Problem, Run, capture, ensure};
use crate::app::{App, AppState};
use crate::audio::{MixerOut, SoundHost};
use crate::core::audio::{Mixer, open_vorbis};
use crate::core::input::{Event, Input, Key};
use crate::core::vfs::{self, Vfs};
use crate::data::GameData;
use crate::onsets::{Found, Frames, Recording, Template, find};
use crate::present::Presenter;
use volya_engine_formats::ui::Resolution;

#[allow(dead_code)]
mod list {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fidelity/views.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fidelity/walks.rs"
    ));
}
use list::{FRAME_MS, HEIGHT, PREROLL, SKIP_EVERY, WALKS, WIDTH, Walk, WalkKey};

/// One check per walk.
pub const CHECKS: &[(&str, Run)] = &[(WALKS[0].name, |c| Box::pin(walk(c, 0)))];

const SIZE: Resolution = Resolution::new(WIDTH, HEIGHT);
/// Our offline output: the game's sounds' rate, each frame exactly
/// [`FRAME_SAMPLES`] of it.
const RATE: u32 = 44_100;
const FRAME_SAMPLES: usize = (RATE * FRAME_MS / 1000) as usize;
/// The frames the startup sequence, the loading and the intro may take.
const TIMEOUT: u32 = 20_000;
/// Two onsets of the two recordings are one step's when their frames are
/// at most this far apart.
const MATCH_FRAMES: i64 = 6;

fn key_of(k: WalkKey) -> Key {
    match k {
        WalkKey::Forward => Key::W,
        WalkKey::TurnLeft => Key::Left,
        WalkKey::TurnRight => Key::Right,
    }
}

/// The keys `walk` holds at its frame `k` (0 its first).
fn keys_at(walk: &Walk, mut k: i64) -> &'static [WalkKey] {
    for s in walk.segments {
        if k < 0 {
            break;
        }
        if k < i64::from(s.frames) {
            return s.keys;
        }
        k -= i64::from(s.frames);
    }
    &[]
}

/// A step sound's material: its file name without digits
/// (`material\actor\step\earth3` is `earth`).
fn material(sound: &str) -> String {
    let name = sound.rsplit('\\').next().unwrap_or(sound);
    name.chars().filter(|c| !c.is_ascii_digit()).collect()
}

/// Our app and its offline audio.
struct Game {
    app: App,
    input: Input,
    mixer: Mixer,
    host: SoundHost,
    voices: Vec<crate::core::audio::VoiceId>,
    frame: u32,
}

impl Game {
    /// One frame: the app's update and the sound system's at the frame's
    /// time, then the frame's samples mixed.
    fn frame(&mut self, tree: &dyn Vfs) -> Vec<f32> {
        self.frame += 1;
        let t = f64::from(self.frame) * f64::from(FRAME_MS);
        self.app.update(
            tree,
            t,
            &mut self.input,
            &mut MixerOut {
                mixer: &mut self.mixer,
                tree,
                voices: &mut self.voices,
                sounds: Some(&mut self.host.system),
            },
        );
        self.input.end_frame();
        self.host.update(t, Some(&mut self.mixer), Some(tree));
        self.mixer.mix(FRAME_SAMPLES)
    }
}

async fn walk(cx: &Context<'_>, index: usize) -> Outcome {
    let walk = WALKS[index];
    let tree = cx.tree;
    let wav_name = format!("{}.wav", walk.name);
    let table_name = format!("{}.frames.txt", walk.name);
    // The original's capture, where the suite has one; without it ours is
    // still rendered and kept, and the check skips.
    let original = match (cx.reference(&wav_name), cx.reference(&table_name)) {
        (Some(wav), Some(table)) => Some((
            Recording::from_wav(wav).map_err(|e| format!("{wav_name}: {e}"))?,
            Frames::parse(&String::from_utf8_lossy(table))
                .map_err(|e| format!("{table_name}: {e}"))?,
        )),
        _ => None,
    };

    let mut data = GameData::load(tree, cx.root, SIZE).map_err(|e| format!("game data: {e}"))?;
    data.set_user_config(list::audio_ltx(&[]));
    let aliases = data.aliases.clone();
    let mut run = Game {
        app: App::new(data).map_err(|e| format!("app: {e}"))?,
        input: Input::new(),
        mixer: Mixer::new(RATE),
        host: SoundHost::default(),
        voices: Vec::new(),
        frame: 0,
    };
    // The title menu as the capture reaches it: Esc every SKIP_EVERY
    // frames while the startup sequence plays.
    while !matches!(run.app.state(), AppState::Menu { .. }) {
        ensure!(run.frame < TIMEOUT, "no title menu");
        if run.frame % SKIP_EVERY == 0 {
            key(&mut run.input, Key::Escape);
        }
        run.frame(tree);
    }
    // New Game, then the intro skipped once it shows: frame 1 of play is
    // the first without it.
    for _ in 0..2 {
        key(&mut run.input, Key::Enter);
        run.frame(tree);
    }
    loop {
        ensure!(run.frame < TIMEOUT, "no intro: {:?}", run.app.state());
        if let AppState::Level { intro: Some(_), .. } = run.app.state() {
            break;
        }
        run.frame(tree);
    }
    key(&mut run.input, Key::Escape);
    loop {
        run.frame(tree);
        ensure!(run.frame < TIMEOUT, "the intro was not skipped");
        if let AppState::Level { intro: None, .. } = run.app.state() {
            break;
        }
    }
    // Frame 1 of play is the frame just run; up to the recording's first.
    let from = i64::from(walk.start) - i64::from(PREROLL);
    let first = i64::from(walk.start);
    let end = first + i64::from(walk.frames());
    let mut play = 1i64;
    while play + 1 < from {
        run.frame(tree);
        play += 1;
    }
    // The recording, with our steps as the engine played them (frame of
    // play, the step list's material) and where the walk took the player.
    let mut ours: Vec<f32> = Vec::new();
    let mut steps: Vec<(i64, String)> = Vec::new();
    let mut path = String::new();
    let mut held: &[WalkKey] = &[];
    // The frames drawn, the start's and the end's, each composed in the
    // frame before it too, as the app composes every frame (the detail
    // pass selects only when it ran in the previous frame, #557).
    let views = [(first - 1, "start"), (end - 1, "end")];
    let drawn = |g: i64| views.iter().find(|(f, _)| *f == g).map(|(_, n)| *n);
    let mut presenter = Presenter::new();
    let mut gpu = cx.renderer();
    for g in from..end {
        let want = keys_at(&walk, g - first);
        for k in held.iter().filter(|k| !want.contains(k)) {
            let (key, pressed) = (key_of(*k), false);
            run.input.handle(Event::Key { key, pressed });
        }
        for k in want.iter().filter(|k| !held.contains(k)) {
            let (key, pressed) = (key_of(*k), true);
            run.input.handle(Event::Key { key, pressed });
        }
        if want != held || g + 1 == end {
            if let Some(level) = run.app.level() {
                let p = level.player().state().position;
                let _ = writeln!(
                    path,
                    "  frame {g}: at ({:.2}, {:.2}, {:.2})",
                    p[0], p[1], p[2]
                );
            }
        }
        held = want;
        ours.extend(run.frame(tree));
        if drawn(g + 1).is_some() {
            (run.app.scene(tree, SIZE)).map_err(|e| format!("scene: {e}"))?;
        }
        if let Some(view) = drawn(g) {
            let s = (run.app.scene(tree, SIZE)).map_err(|e| format!("scene: {e}"))?;
            let list = presenter.draw_list(&mut gpu, tree, &s, run.app.clock());
            let pixels = capture(&mut gpu, &list, WIDTH, HEIGHT).await?;
            let errors = presenter.take_errors();
            ensure!(errors.is_empty(), "{errors:?}");
            cx.dump(&format!("{}-{view}", walk.name), WIDTH, HEIGHT, &pixels);
        }
        let level = run.app.level().ok_or("not in the level")?;
        if let Some(f) = level.footsteps() {
            for s in f.sounds() {
                let list = &f.pairs().lists[s.pair].step;
                steps.push((g, list.first().map_or_else(String::new, |n| material(n))));
            }
        }
    }
    let ours_table = Frames::even(from, (end - from) as usize, FRAME_SAMPLES as i64);
    cx.keep_file(&wav_name, wav16(RATE, 2, &ours));
    cx.keep_file(&table_name, ours_table.text(walk.name, RATE).into_bytes());
    let ours = Recording::from_interleaved(RATE, 2, &ours);

    // Every footstep sound of the player's pairs (its own material with
    // each material), a template each.
    let level = run.app.level().ok_or("not in the level")?;
    let footsteps = level.footsteps().ok_or("no footsteps")?;
    let (pairs, own) = (footsteps.pairs().clone(), footsteps.own());
    let mut names = BTreeSet::new();
    for m in 0..level.collision().materials().len() as u32 {
        if let Ok(Some(k)) = pairs.table.pair(own, m) {
            names.extend(pairs.lists[k].step.iter().cloned());
        }
    }
    let mut library = Vec::new();
    for name in &names {
        let Ok(path) = aliases.path("$game_sounds$", &format!("{name}.ogg")) else {
            continue;
        };
        let Ok(bytes) = vfs::read(tree, &path) else {
            continue;
        };
        let Ok(sound) = decode(bytes) else { continue };
        library.extend(Template::new(name, &sound));
    }
    ensure!(!library.is_empty(), "no footstep sound decoded");

    let empty = (
        Recording {
            rate: RATE,
            samples: Vec::new(),
        },
        Frames::default(),
    );
    let (original_audio, original_frames) = original.as_ref().unwrap_or(&empty);
    let report = compare(
        &walk,
        (original_audio, original_frames),
        (&ours, &ours_table),
        &steps,
        &library,
    );
    let mut text = report.text;
    let _ = write!(text, "\nOur walk:\n{path}");
    cx.keep_file(&format!("{}.report.txt", walk.name), text.into_bytes());
    if original.is_none() {
        return Err(Problem::Skip(format!(
            "no capture of the original's {} (//tests/fidelity:audio); ours: {}",
            walk.name, report.note
        )));
    }
    Ok(report.note)
}

/// A Vorbis file decoded, mono.
fn decode(bytes: Vec<u8>) -> Result<Recording, String> {
    let mut source = open_vorbis(bytes, 0).map_err(|e| e.to_string())?;
    let (rate, channels) = (source.sample_rate(), source.channels());
    let mut out = Vec::new();
    while source.read(&mut out).map_err(|e| e.to_string())? {}
    Ok(Recording::from_interleaved(
        rate,
        usize::from(channels),
        &out,
    ))
}

/// Interleaved samples as a 16-bit PCM WAV file.
fn wav16(rate: u32, channels: u16, samples: &[f32]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + samples.len() * 2);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    w.extend_from_slice(&(channels * 2).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    w
}

/// A footstep sound found, with its frame of play.
struct Heard {
    frame: i64,
    level_db: f32,
    material: String,
    sound: String,
    score: f32,
}

fn heard(found: &[Found], frames: &Frames, library: &[Template]) -> Vec<Heard> {
    found
        .iter()
        .filter_map(|f| {
            let name = &library[f.template].name;
            Some(Heard {
                frame: frames.frame_of(f.sample)?,
                level_db: f.level_db,
                material: material(name),
                sound: name.clone(),
                score: f.score,
            })
        })
        .collect()
}

struct Report {
    note: String,
    text: String,
}

fn compare(
    walk: &Walk,
    (original, original_frames): (&Recording, &Frames),
    (ours, ours_frames): (&Recording, &Frames),
    steps: &[(i64, String)],
    library: &[Template],
) -> Report {
    let first = i64::from(walk.start);
    let theirs = heard(&find(original, library), original_frames, library);
    let mine = heard(&find(ours, library), ours_frames, library);
    let mut text = String::new();
    let _ = writeln!(
        text,
        "{}: frames of play {}..{} (keys from {first}); the original's recording {:.1} s at {} Hz, \
         ours {:.1} s at {} Hz",
        walk.name,
        first - i64::from(PREROLL),
        first + i64::from(walk.frames()) - 1,
        original.seconds(original.samples.len()),
        original.rate,
        ours.seconds(ours.samples.len()),
        ours.rate
    );
    let _ = writeln!(
        text,
        "Our engine played {} steps: {}",
        steps.len(),
        steps
            .iter()
            .map(|(f, m)| format!("{f} {m}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    // How the finding does on ours, whose steps are known: each step found
    // within a frame of its own, and of its material.
    let near = |f: i64| mine.iter().filter(move |h| (h.frame - f).abs() <= 1);
    let timed = steps
        .iter()
        .filter(|(f, _)| near(*f).next().is_some())
        .count();
    let named = (steps.iter())
        .filter(|(f, m)| near(*f).any(|h| &h.material == m))
        .count();
    let _ = writeln!(
        text,
        "Found in ours: {timed} of {} steps within a frame of theirs, {named} of them as their \
         material; {} sounds found in all",
        steps.len(),
        mine.len()
    );
    // Onsets of the two paired in order, each to the nearest unpaired one
    // within MATCH_FRAMES.
    let mut used = vec![false; theirs.len()];
    let mut pairs = Vec::new();
    for (i, m) in mine.iter().enumerate() {
        let best = (theirs.iter().enumerate())
            .filter(|(j, t)| !used[*j] && (t.frame - m.frame).abs() <= MATCH_FRAMES)
            .min_by_key(|(_, t)| (t.frame - m.frame).abs());
        if let Some((j, _)) = best {
            used[j] = true;
            pairs.push((Some(i), Some(j)));
        } else {
            pairs.push((Some(i), None));
        }
    }
    for (j, u) in used.iter().enumerate() {
        if !u {
            pairs.push((None, Some(j)));
        }
    }
    let frame_of = |p: &(Option<usize>, Option<usize>)| match p {
        (Some(i), _) => mine[*i].frame,
        (None, Some(j)) => theirs[*j].frame,
        (None, None) => 0,
    };
    pairs.sort_by_key(|p| frame_of(p));
    let _ = writeln!(
        text,
        "Footstep sounds found (frame of play, +frames from the keys, the best matching sound \
         (its material: its name without digits), the match's correlation, level dBFS):"
    );
    let side = |h: Option<&Heard>| match h {
        Some(h) => format!(
            "{:5} ({:+4}) {:<16} {:5.1} {:6.1} dB",
            h.frame,
            h.frame - first,
            h.sound.rsplit('\\').next().unwrap_or(&h.sound),
            h.score,
            h.level_db
        ),
        None => format!("{:48}", "-"),
    };
    let _ = writeln!(text, "  {:<50}| original", "ours");
    let (mut matched, mut same_material, mut deltas, mut levels) = (0, 0, Vec::new(), Vec::new());
    for p in &pairs {
        let (m, t) = (p.0.map(|i| &mine[i]), p.1.map(|j| &theirs[j]));
        let mut line = format!("  {} | {}", side(m), side(t));
        if let (Some(m), Some(t)) = (m, t) {
            matched += 1;
            same_material += usize::from(m.material == t.material);
            deltas.push(t.frame - m.frame);
            levels.push(t.level_db - m.level_db);
            let _ = write!(line, " | Δ {:+} frames", t.frame - m.frame);
        }
        let _ = writeln!(text, "{line}");
    }
    let first_after = |h: &[Heard]| h.iter().find(|h| h.frame >= first).map(|h| h.frame - first);
    let delay = |d: Option<i64>| match d {
        Some(d) => format!("{d} frames ({} ms)", d * i64::from(FRAME_MS)),
        None => "none".into(),
    };
    let mean_delta = if deltas.is_empty() {
        f64::NAN
    } else {
        deltas.iter().map(|d| *d as f64).sum::<f64>() / deltas.len() as f64
    };
    levels.sort_by(f32::total_cmp);
    let median_level = levels.get(levels.len() / 2).copied().unwrap_or(f32::NAN);
    let note = format!(
        "{} onsets ours, {} the original's, {matched} paired: frames Δ mean {mean_delta:+.1}, \
         material agrees {same_material}/{matched}, level original − ours median {median_level:+.1} \
         dB; first step after the keys: ours {}, the original's {}",
        mine.len(),
        theirs.len(),
        delay(first_after(&mine)),
        delay(first_after(&theirs)),
    );
    let _ = writeln!(text, "\n{note}");
    Report { note, text }
}
