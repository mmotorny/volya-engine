// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Audio output from the engine worker: the engine drives a [`Remote`]
//! mixer here, whose commands (with the compressed files they play) go to
//! the audio worker (`static/audio-worker.js`) after every frame; that
//! worker mixes them on the output's own schedule (`crate::audio_host`) for
//! the page's AudioWorklet (`static/audio-worklet.js`), as an audio device
//! mixes apart from the game's frames (#354). The positional sound system
//! (docs/behavior/sound-positional.md) runs here, updated once per frame of
//! the worker whether or not the game runs, its voices on that mixer.

use std::cell::RefCell;

use volya_engine::audio::SoundHost;
use volya_engine::core::audio::remote::Remote;
use volya_engine::core::audio::{self as mix, Category, MixerControl, Playback, VoiceId};
use volya_engine::core::vfs::Vfs;
use wasm_bindgen::prelude::*;

use crate::audio_ring::Ring;

thread_local! {
    static OUTPUT: RefCell<Option<Remote>> = const { RefCell::new(None) };
    /// The voices of the music track playing, left and right.
    static MUSIC: RefCell<Option<[VoiceId; 2]>> = const { RefCell::new(None) };
    static SOUNDS: RefCell<Option<SoundHost>> = const { RefCell::new(None) };
    /// The output's shared memory, and commands that did not fit in it.
    static RING: RefCell<Option<(Ring, Vec<u8>)>> = const { RefCell::new(None) };
    /// Tests (`debug_audio_hold`): the mixer's commands wait, so those of
    /// several calls reach the audio worker together, as one frame's do.
    #[cfg(feature = "test-ops")]
    static HOLD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The audio worker's latest report.
    static HOST_STATS: RefCell<HostStats> = RefCell::new(HostStats::default());
}

/// What the audio worker last reported of itself (`crate::audio_host`).
#[derive(Clone, Copy, Default, serde::Serialize)]
pub struct HostStats {
    /// Its wasm heap (linear memory).
    pub audio_heap_bytes: f64,
    /// Time spent mixing (decoding included), ms, and frames mixed, since
    /// the audio worker started.
    pub audio_mix_ms: f64,
    pub audio_mixed_frames: f64,
    /// Frames the AudioWorklet played as silence for want of mixed ones,
    /// since the audio worker started.
    pub audio_underrun_frames: f64,
}

pub fn host_stats() -> HostStats {
    HOST_STATS.with(|s| *s.borrow())
}

/// Runs `f` on the sound system (made on first use).
pub fn with_sounds<T>(f: impl FnOnce(&mut SoundHost) -> T) -> T {
    SOUNDS.with(|s| f(s.borrow_mut().get_or_insert_with(SoundHost::default)))
}

/// The sound system's update of the frame at page time `time` (ms), on the
/// mixer once audio output is connected, reading sounds from the game
/// tree when one is mounted.
pub fn sound_frame(time: f64) {
    with_sounds(|host| {
        let mut host = Some(host);
        let mut run = |tree: Option<&dyn Vfs>| {
            if let Some(host) = host.take() {
                with_mixer_opt(|mixer| host.update(time, mixer, tree));
            }
        };
        if crate::game::with_trees(|tree, _| run(Some(tree))).is_err() {
            run(None);
        }
    });
}

/// The game data directory in the game tree: the archives hold their files
/// under it (`$game_data$`, docs/formats/ltx-config.md section 10).
const GAME_DATA: &str = "gamedata\\";

/// (Re)starts audio output at the output's sample rate on `memory`, a new
/// output's shared memory (`crate::audio_ring`); stops any sounds and keeps
/// the category volumes and the master gain.
#[wasm_bindgen]
pub fn audio_init(sample_rate: u32, memory: JsValue) {
    OUTPUT.with(|m| {
        let mut m = m.borrow_mut();
        match m.as_mut() {
            Some(remote) => remote.connect(sample_rate),
            None => *m = Some(Remote::new(sample_rate)),
        }
    });
    RING.with(|r| *r.borrow_mut() = Some((Ring::new(&memory), Vec::new())));
    MUSIC.with(|m| *m.borrow_mut() = None);
    with_sounds(SoundHost::mixer_replaced);
}

/// Sends the mixer's commands since the last call to the audio worker
/// through the ring of commands (no message, nothing for a garbage
/// collector) and wakes it; returns the files they play, `[number,
/// Uint8Array]` pairs for the worker to post, or null. The files leave the
/// engine's heap.
#[wasm_bindgen]
pub fn audio_flush() -> JsValue {
    #[cfg(feature = "test-ops")]
    if HOLD.with(std::cell::Cell::get) {
        return JsValue::NULL;
    }
    let message = OUTPUT.with(|m| m.borrow_mut().as_mut().and_then(Remote::take));
    let connected = RING.with(|r| {
        let mut r = r.borrow_mut();
        let (ring, backlog) = r.as_mut()?;
        if let Some(m) = &message {
            backlog.extend_from_slice(&m.commands);
        }
        // A full ring (the audio worker stalled) keeps them, whole, for the
        // next call.
        if !backlog.is_empty() && ring.write_commands(backlog) {
            backlog.clear();
            ring.wake();
        }
        Some(())
    });
    let Some(message) = message.filter(|m| connected.is_some() && !m.files.is_empty()) else {
        return JsValue::NULL;
    };
    message
        .files
        .iter()
        .map(|(n, f)| js_sys::Array::of2(&(*n).into(), &js_sys::Uint8Array::from(f.as_slice())))
        .collect::<js_sys::Array>()
        .into()
}

/// The `debug_audio_hold` op (tests): while `on`, the mixer's commands
/// wait ([`audio_flush`] sends nothing).
#[cfg(feature = "test-ops")]
pub fn hold(on: bool) {
    HOLD.with(|h| h.set(on));
}

/// A report of the audio worker (`crate::audio_host::audio_host_report`):
/// the voices that ended, its counters, the diagnostic log's numbers and,
/// while capturing, its blocks.
#[wasm_bindgen]
pub fn audio_report(report: &str) -> Result<(), JsValue> {
    let report: serde_json::Value = serde_json::from_str(report).map_err(error)?;
    let ended: Vec<u32> = report["ended"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(serde_json::Value::as_u64)
                .map(|v| v as u32)
                .collect()
        })
        .unwrap_or_default();
    OUTPUT.with(|m| {
        if let Some(remote) = m.borrow_mut().as_mut() {
            remote.ended(&ended);
        }
    });
    let number = |k: &str| report[k].as_f64().unwrap_or(0.0);
    HOST_STATS.with(|s| {
        *s.borrow_mut() = HostStats {
            audio_heap_bytes: number("heap"),
            audio_mix_ms: number("mix_ms"),
            audio_mixed_frames: number("frames"),
            audio_underrun_frames: number("underrun"),
        }
    });
    if let Some(d) = report["diagnostics"].as_array() {
        let v = |i: usize| d.get(i).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
        crate::diagnostics::audio_ring(&crate::diagnostics::AudioRing {
            underrun: v(0),
            fill: [v(1), v(2), v(3), v(4)],
            mix: [v(5), v(6), v(7), v(8)],
            rate: number("rate"),
        });
    }
    #[cfg(feature = "test-ops")]
    if let Some(blocks) = report["blocks"].as_array() {
        capture::blocks(blocks);
    }
    Ok(())
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// The file at `path` of `tree`, compressed.
fn read(tree: &dyn Vfs, path: &str) -> Result<Vec<u8>, JsValue> {
    volya_engine::core::vfs::read(tree, path).map_err(|e| error(format!("{path}: {e}")))
}

fn with_mixer<T>(f: impl FnOnce(&mut Remote) -> T) -> Result<T, JsValue> {
    OUTPUT.with(|m| {
        let mut mixer = m.borrow_mut();
        let mixer = mixer
            .as_mut()
            .ok_or_else(|| error("audio output not connected"))?;
        Ok(f(mixer))
    })
}

/// Starts `bytes`, the file at `path`; fails when they are not a stream
/// that plays.
fn start(
    mixer: &mut Remote,
    path: &str,
    bytes: Vec<u8>,
    playback: Playback,
) -> Result<VoiceId, JsValue> {
    mixer
        .play_file(bytes, 0, playback)
        .ok_or_else(|| error(format!("{path}: not an Ogg Vorbis stream")))
}

/// Plays the Ogg Vorbis file at `path` in `tree`; returns the voice id.
pub fn play(tree: &dyn Vfs, path: &str, playback: Playback) -> Result<u32, JsValue> {
    let bytes = read(tree, path)?;
    with_mixer(|mixer| start(mixer, path, bytes, playback))?.map(VoiceId::index)
}

/// Starts music track `track` (e.g. `music\wasteland2`) from its pair of
/// files, both channels together, replacing any music playing; returns the
/// voice ids, left and right.
pub fn play_music(tree: &dyn Vfs, track: &str) -> Result<[u32; 2], JsValue> {
    let paths = mix::music_files(track).map(|file| format!("{GAME_DATA}{file}"));
    let [left, right] = [read(tree, &paths[0])?, read(tree, &paths[1])?];
    stop_music();
    // Both before the next message, so on the same output frame.
    let ids = with_mixer(|mixer| -> Result<_, JsValue> {
        let l = start(mixer, &paths[0], left, mix::MUSIC_PLAYBACK[0])?;
        match start(mixer, &paths[1], right, mix::MUSIC_PLAYBACK[1]) {
            Ok(r) => Ok([l, r]),
            Err(e) => {
                mixer.stop(l);
                Err(e)
            }
        }
    })??;
    MUSIC.with(|m| *m.borrow_mut() = Some(ids));
    Ok(ids.map(VoiceId::index))
}

/// Stops the music track, if one plays.
pub fn stop_music() {
    if let Some(ids) = MUSIC.with(|m| m.borrow_mut().take()) {
        let _ = with_mixer(|mixer| ids.map(|id| mixer.stop(id)));
    }
}

/// Runs `f` on the mixer, `None` before audio output is connected.
pub fn with_mixer_opt<T>(f: impl FnOnce(Option<&mut dyn MixerControl>) -> T) -> T {
    OUTPUT.with(|m| f(m.borrow_mut().as_mut().map(|m| m as &mut dyn MixerControl)))
}

pub fn set_volume(category: Category, volume: f32) -> Result<(), JsValue> {
    with_sounds(|host| host.system.set_volume(category, volume));
    with_mixer(|mixer| mixer.set_volume(category, volume))
}

pub fn stop_all() {
    MUSIC.with(|m| *m.borrow_mut() = None);
    with_sounds(|host| host.system.stop_all());
    let _ = with_mixer(|mixer| mixer.stop_all());
}

/// The `debug_play_sound` op (tests): plays the sound named `name` (S1's
/// name rules, with the installer's path aliases) at `position` with the
/// play flags given; its voice and emitter show in [`sound_state`].
#[cfg(feature = "test-ops")]
pub fn debug_play_sound(
    tree: &dyn Vfs,
    name: &str,
    category: Category,
    position: [f32; 3],
    flags: volya_engine::core::sound::system::PlayFlags,
) -> Result<(), JsValue> {
    let aliases = volya_engine::core::config::PathAliases::parse(
        volya_engine::data::INSTALLER_FSGAME.as_bytes(),
        "",
    )
    .map_err(error)?;
    with_sounds(|host| {
        let id = host
            .system
            .create(tree, &aliases, name, category)
            .map_err(error)?;
        host.system.play_at(id, position, flags);
        Ok(())
    })
}

/// The `sound_state` op (tests): the sound clock, the voices and every
/// emitter.
#[cfg(feature = "test-ops")]
pub fn sound_state() -> serde_json::Value {
    with_sounds(|host| {
        let s = &host.system;
        serde_json::json!({
            "now": s.now(),
            "voices": s.voice_count(),
            "pause_level": s.pause_level(),
            "emitters": s.emitters().iter().map(|e| serde_json::json!({
                "name": e.name,
                "phase": format!("{:?}", e.phase).to_lowercase(),
                "position": e.position,
                "two_d": e.two_d,
                "out": e.out,
                "voice": e.voice,
                "looped": e.looped,
                "volume": e.volume,
                "occlusion": e.occlusion,
            })).collect::<Vec<_>>(),
        })
    })
}

/// The `audio_capture` op (tests): records, for the emitter of one sound
/// name, every block the audio worker mixed (its mixer voice, how far the
/// voice advanced, its gains) with the AudioWorklet's underruns, and every
/// worker frame (its time and the emitter's state), to find gaps and jumps
/// in what is heard.
#[cfg(feature = "test-ops")]
pub mod capture {
    use super::{OUTPUT, with_sounds};
    use std::cell::RefCell;
    use volya_engine::core::audio::VoiceId;

    struct Capture {
        name: String,
        /// Its start, ms since the epoch (blocks come from another worker).
        start: f64,
        /// The mixer voice the audio worker watches.
        watched: Option<VoiceId>,
        blocks: Vec<serde_json::Value>,
        frames: Vec<serde_json::Value>,
    }

    thread_local! {
        static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
    }

    fn watch(voice: Option<Option<VoiceId>>) {
        OUTPUT.with(|m| {
            if let Some(remote) = m.borrow_mut().as_mut() {
                remote.watch(voice);
            }
        });
    }

    /// Starts capturing the emitter of `name`, replacing any capture.
    pub fn start(name: &str) {
        CAPTURE.with(|c| {
            *c.borrow_mut() = Some(Capture {
                name: name.to_owned(),
                start: crate::epoch_now(),
                watched: None,
                blocks: Vec::new(),
                frames: Vec::new(),
            });
        });
        watch(Some(None));
    }

    /// Stops capturing; returns `{blocks, frames}` (null when none ran):
    /// blocks `[t ms, frames, mix ms, underrun frames, queued frames, mixer
    /// voice or -1, advanced before, advanced after, step, left gain at the
    /// block's start, at its end]`, frames `[t ms, frame CPU ms, phase,
    /// device voice or -1, output volume, occlusion]`.
    pub fn stop() -> serde_json::Value {
        watch(None);
        CAPTURE.with(|c| match c.borrow_mut().take() {
            Some(c) => serde_json::json!({ "blocks": c.blocks, "frames": c.frames }),
            None => serde_json::Value::Null,
        })
    }

    /// Blocks the audio worker reported, their times in ms since the epoch.
    pub fn blocks(blocks: &[serde_json::Value]) {
        CAPTURE.with(|c| {
            if let Some(c) = c.borrow_mut().as_mut() {
                for block in blocks {
                    let mut block = block.clone();
                    if let Some(t) = block.get(0).and_then(serde_json::Value::as_f64) {
                        block[0] = (t - c.start).into();
                    }
                    c.blocks.push(block);
                }
            }
        });
    }

    /// After a worker frame that took `cpu` ms: records the emitter's state
    /// and tells the audio worker which mixer voice is the emitter's.
    pub fn frame(cpu: f64) {
        let Some(name) = CAPTURE.with(|c| c.borrow().as_ref().map(|c| c.name.clone())) else {
            return;
        };
        let (emitter, voice) = with_sounds(|host| {
            let e = host.system.emitters().into_iter().find(|e| e.name == name);
            let voice = e
                .as_ref()
                .and_then(|e| e.voice)
                .and_then(|v| host.mixer_voice(v));
            (e, voice)
        });
        let t = crate::epoch_now();
        let changed = CAPTURE.with(|c| {
            let mut c = c.borrow_mut();
            let c = c.as_mut()?;
            c.frames.push(match emitter {
                Some(e) => serde_json::json!([
                    t - c.start,
                    cpu,
                    format!("{:?}", e.phase).to_lowercase(),
                    e.voice.map_or(-1, |v| v as i64),
                    e.out,
                    e.occlusion,
                ]),
                None => serde_json::json!([t - c.start, cpu, "none", -1, 0.0, 0.0]),
            });
            (c.watched != voice).then(|| {
                c.watched = voice;
                voice
            })
        });
        if let Some(voice) = changed {
            watch(Some(voice));
        }
    }
}
