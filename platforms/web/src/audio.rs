// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Audio output from the engine worker: the mixer runs here; the page's
//! AudioWorklet (`static/audio-worklet.js`) pulls mixed stereo blocks over a
//! MessagePort handled in `static/engine-worker.js`.

use std::cell::RefCell;

use volya_engine::core::audio::Mixer;
use volya_engine::core::vfs::Vfs;
use volya_engine_formats::audio::VorbisStream;
use wasm_bindgen::prelude::*;

thread_local! {
    static MIXER: RefCell<Option<Mixer>> = const { RefCell::new(None) };
}

/// (Re)starts audio output at the output's sample rate; stops any sounds.
#[wasm_bindgen]
pub fn audio_init(sample_rate: u32) {
    MIXER.with(|m| *m.borrow_mut() = Some(Mixer::new(sample_rate)));
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

/// Plays the Ogg Vorbis file at `path` in `tree`; returns the voice id.
pub fn play(tree: &dyn Vfs, path: &str, looping: bool, gain: f32) -> Result<u32, JsValue> {
    // Compressed sounds are small; owning the bytes lets the voice outlive
    // the borrow of the tree.
    let bytes = volya_engine::core::vfs::read(tree, path).map_err(error)?;
    let stream = VorbisStream::open(std::io::Cursor::new(bytes)).map_err(error)?;
    MIXER.with(|m| {
        let mut mixer = m.borrow_mut();
        let mixer = mixer
            .as_mut()
            .ok_or_else(|| error("audio output not connected"))?;
        Ok(mixer.play(Box::new(stream), looping, gain).index())
    })
}

pub fn stop_all() {
    MIXER.with(|m| {
        if let Some(mixer) = m.borrow_mut().as_mut() {
            mixer.stop_all();
        }
    });
}
