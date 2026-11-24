// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Audio output from the engine worker: the mixer runs here; the page's
//! AudioWorklet (`static/audio-worklet.js`) pulls mixed stereo blocks over a
//! MessagePort handled in `static/engine-worker.js`.

use std::cell::RefCell;

use volya_engine::core::audio::{self as mix, Category, Mixer, Playback, VoiceId};
use volya_engine::core::vfs::Vfs;
use volya_engine_formats::audio::VorbisStream;
use wasm_bindgen::prelude::*;

thread_local! {
    static MIXER: RefCell<Option<Mixer>> = const { RefCell::new(None) };
    /// The voices of the music track playing, left and right.
    static MUSIC: RefCell<Option<[VoiceId; 2]>> = const { RefCell::new(None) };
}

/// The game data directory in the game tree: the archives hold their files
/// under it (`$game_data$`, docs/formats/ltx-config.md section 10).
const GAME_DATA: &str = "gamedata\\";

/// (Re)starts audio output at the output's sample rate; stops any sounds and
/// keeps the category volumes.
#[wasm_bindgen]
pub fn audio_init(sample_rate: u32) {
    MIXER.with(|m| {
        let mut mixer = m.borrow_mut();
        let mut new = Mixer::new(sample_rate);
        if let Some(old) = mixer.as_ref() {
            for category in [Category::Effects, Category::Music] {
                new.set_volume(category, old.volume(category));
            }
        }
        *mixer = Some(new);
    });
    MUSIC.with(|m| *m.borrow_mut() = None);
}

/// Mixes the next `frames` frames (stereo, interleaved); silence before init.
#[wasm_bindgen]
pub fn audio_mix(frames: u32) -> Vec<f32> {
    MIXER.with(|m| match m.borrow_mut().as_mut() {
        Some(mixer) => mixer.mix(frames as usize),
        None => vec![0.0; frames as usize * 2],
    })
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

type Stream = VorbisStream<std::io::Cursor<Vec<u8>>>;

fn open(tree: &dyn Vfs, path: &str) -> Result<Stream, JsValue> {
    // Compressed sounds are small; owning the bytes lets the voice outlive
    // the borrow of the tree.
    let bytes =
        volya_engine::core::vfs::read(tree, path).map_err(|e| error(format!("{path}: {e}")))?;
    VorbisStream::open(std::io::Cursor::new(bytes)).map_err(|e| error(format!("{path}: {e}")))
}

fn with_mixer<T>(f: impl FnOnce(&mut Mixer) -> T) -> Result<T, JsValue> {
    MIXER.with(|m| {
        let mut mixer = m.borrow_mut();
        let mixer = mixer
            .as_mut()
            .ok_or_else(|| error("audio output not connected"))?;
        Ok(f(mixer))
    })
}

/// Plays the Ogg Vorbis file at `path` in `tree`; returns the voice id.
pub fn play(tree: &dyn Vfs, path: &str, playback: Playback) -> Result<u32, JsValue> {
    let stream = open(tree, path)?;
    with_mixer(|mixer| mixer.play(Box::new(stream), playback).index())
}

/// Starts music track `track` (e.g. `music\wasteland2`) from its pair of
/// files, both channels together, replacing any music playing; returns the
/// voice ids, left and right.
pub fn play_music(tree: &dyn Vfs, track: &str) -> Result<[u32; 2], JsValue> {
    let [left, right] =
        mix::music_files(track).map(|file| open(tree, &format!("{GAME_DATA}{file}")));
    let streams = [left?, right?];
    stop_music();
    // Both start before the next mix, so on the same output frame.
    let ids = with_mixer(|mixer| {
        let [left, right] = streams;
        [
            mixer.play(Box::new(left), mix::MUSIC_PLAYBACK[0]),
            mixer.play(Box::new(right), mix::MUSIC_PLAYBACK[1]),
        ]
    })?;
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
pub fn with_mixer_opt<T>(f: impl FnOnce(Option<&mut Mixer>) -> T) -> T {
    MIXER.with(|m| f(m.borrow_mut().as_mut()))
}

pub fn set_volume(category: Category, volume: f32) -> Result<(), JsValue> {
    with_mixer(|mixer| mixer.set_volume(category, volume))
}

pub fn stop_all() {
    MUSIC.with(|m| *m.borrow_mut() = None);
    let _ = with_mixer(Mixer::stop_all);
}
