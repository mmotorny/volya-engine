// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Movie sequences (docs/behavior/startup.md section 6): [`MoviePlayer`]
//! plays the items of any sequence of `ui_movies.xml` in order on the
//! engine clock, each stoppable item skippable with the quit key;
//! [`Startup`] is the logo sequence `intro_logo`, and the app plays the New
//! Game intro `intro_game` with it too (loading-screen.md section 7).
//!
//! Timing follows the clock, not the audio output: a video item's picture
//! shows the frame for the time since its delay ended, and the item ends
//! when its left sound's length has passed. The spec synchronises the
//! picture to the left sound's playback position; the two agree while the
//! sound plays in real time from its start. Browsers start audio output only
//! after a user gesture, so a sound may start late or not at all; keeping the
//! clock as the master lets the sequence run (and end) regardless.

use std::io;

use serde::Serialize;

use crate::audio::{AudioOut, Voice};
use crate::core::audio::{Category, MUSIC_PLAYBACK, Playback};
use crate::data::GameData;
use crate::draw::Ctx;
use crate::scene::{Scene, Sprite};
use volya_engine_formats::ui::layout::Rect;
use volya_engine_formats::ui::movies::{MovieItemKind, MovieSequence};
use volya_engine_formats::ui::picture::{Picture, PicturePlacement};
use volya_engine_formats::ui::{Color, UI_HEIGHT, UI_WIDTH};

/// A colour animation: keys (frame, ARGB) at a frame rate, interpolated
/// linearly, advancing in whole frames (startup.md section 6.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorAnimation {
    pub fps: f64,
    pub keys: &'static [(u32, u32)],
}

/// `asus_logo_01` as startup.md section 6.4 gives it (docs/formats/lanims-xr.md
/// section 8 agrees). The one animation the startup uses is built in from that
/// table; other names have no animation (the picture keeps its colour).
// TODO(#337): read it from `lanims.xr` (lanims-xr.md), where a missing file
// gives no animation.
const ASUS_LOGO_01: ColorAnimation = ColorAnimation {
    fps: 30.0,
    keys: &[
        (0, 0x0000_0000),
        (32, 0xff00_0000),
        (85, 0xff00_0000),
        (119, 0x0000_0000),
    ],
};

impl ColorAnimation {
    pub fn named(name: &str) -> Option<Self> {
        (name == "asus_logo_01").then_some(ASUS_LOGO_01)
    }

    /// The colour `seconds` after the animation started: frame
    /// ⌊seconds · fps⌋, each channel interpolated linearly between the keys
    /// around it and the fraction discarded; before the first key the first,
    /// after the last the last (it is not repeated).
    pub fn color_at(&self, seconds: f64) -> Color {
        let frame = (seconds.max(0.0) * self.fps).floor() as u32;
        let channel = |argb: u32, shift: u32| ((argb >> shift) & 0xff) as f64;
        let argb = match self.keys.iter().position(|&(f, _)| f > frame) {
            Some(0) => self.keys[0].1,
            None => self.keys[self.keys.len() - 1].1,
            Some(i) => {
                let ((f0, c0), (f1, c1)) = (self.keys[i - 1], self.keys[i]);
                let t = f64::from(frame - f0) / f64::from(f1 - f0);
                [24, 16, 8, 0].iter().fold(0, |acc, &s| {
                    let v = channel(c0, s) + (channel(c1, s) - channel(c0, s)) * t;
                    acc | (v as u32) << s
                })
            }
        };
        Color {
            a: (argb >> 24) as u8,
            r: (argb >> 16) as u8,
            g: (argb >> 8) as u8,
            b: argb as u8,
        }
    }
}

/// How the two files of a logo sound play: like the music pair (startup.md
/// section 6.3 places them at x = −0.5 and +0.5, as the menu music), once.
/// Which volume applies was not established; they use the effects volume.
fn sound_playback(side: usize) -> Playback {
    Playback {
        category: Category::Effects,
        ..MUSIC_PLAYBACK[side]
    }
}

/// The wide-screen height correction of video pictures (section 6.3).
const WIDE_VIDEO_STRETCH: f32 = 1.328;

/// A movie sequence with the length of each item in seconds after its
/// delay (sections 6.3, 6.4): an image item its `length_sec`; a video item
/// the length of its left sound, or without a sound its video's.
#[derive(Clone, Copy, Debug)]
pub struct Sequence<'a> {
    pub movies: &'a MovieSequence,
    pub lengths: &'a [f64],
}

/// A movie sequence playing (section 6): the logo sequence at launch, the
/// intro of a New Game (loading-screen.md section 7).
#[derive(Clone, Debug)]
pub struct MoviePlayer {
    /// The current item (`items.len()` once finished).
    item: usize,
    /// When the current item became current, engine clock.
    item_start: u64,
    /// The sounds of the current item, once started.
    voices: Vec<Voice>,
    sound_started: bool,
    /// Items skipped with the quit key.
    skipped: u32,
}

/// The delay of a video item in milliseconds (absent, zero or negative:
/// none).
fn delay_ms(kind: &MovieItemKind) -> u64 {
    match kind {
        MovieItemKind::Video { delay, .. } if *delay > 0.0 => (f64::from(*delay) * 1000.0) as u64,
        _ => 0,
    }
}

impl MoviePlayer {
    /// The sequence, its first item current from `now`.
    pub fn new(now: u64) -> Self {
        Self {
            item: 0,
            item_start: now,
            voices: Vec::new(),
            sound_started: false,
            skipped: 0,
        }
    }

    pub fn finished(&self, seq: Sequence) -> bool {
        self.item >= seq.movies.items.len()
    }

    /// How long item `i` lasts, in milliseconds.
    fn length_ms(seq: Sequence, i: usize) -> u64 {
        delay_ms(&seq.movies.items[i].kind) + (seq.lengths[i] * 1000.0).round() as u64
    }

    /// The current item's `pause_state` is `on` (game time paused while
    /// it plays, level-loading.md section 2.3).
    pub fn pauses(&self, seq: Sequence) -> bool {
        seq.movies
            .items
            .get(self.item)
            .is_some_and(|i| i.pause_state.as_deref() == Some("on"))
    }

    /// The current item, with its `pause_state` (sound-weather-ambient.md
    /// section 3.1), until the sequence has finished.
    pub fn current<'a>(&self, seq: Sequence<'a>) -> Option<(usize, Option<&'a str>)> {
        let item = seq.movies.items.get(self.item)?;
        Some((self.item, item.pause_state.as_deref()))
    }

    fn next(&mut self, at: u64, audio: &mut dyn AudioOut) {
        for v in self.voices.drain(..) {
            audio.stop(v);
        }
        self.item += 1;
        self.item_start = at;
        self.sound_started = false;
    }

    /// Advances to `now`: items end when their time is up (the next starts
    /// exactly then); then each of `quit_presses` ends the current item at
    /// once if it is stoppable (section 6.5); then the
    /// current video item's sounds start once its delay has passed.
    pub fn update(
        &mut self,
        seq: Sequence,
        data: &GameData,
        now: u64,
        quit_presses: u32,
        audio: &mut dyn AudioOut,
    ) {
        while !self.finished(seq) {
            let end = self.item_start + Self::length_ms(seq, self.item);
            if end > now {
                break;
            }
            self.next(end, audio);
        }
        for _ in 0..quit_presses {
            if self.finished(seq) {
                break;
            }
            if self.stoppable(seq) {
                self.skipped += 1;
                self.next(now, audio);
            }
        }
        if self.finished(seq) {
            return;
        }
        let item = &seq.movies.items[self.item].kind;
        if let MovieItemKind::Video {
            sound: Some(sound), ..
        } = item
            && !self.sound_started
            && now >= self.item_start + delay_ms(item)
        {
            self.sound_started = true;
            if let Ok(files) = data.sound_files(sound) {
                for (side, file) in files.iter().enumerate() {
                    self.voices.extend(audio.play(file, sound_playback(side)));
                }
            }
        }
    }

    /// Whether the current item can be ended with the quit key: a video
    /// unless `can_be_stopped` is 0; an image always (the spec covers only
    /// items without `guard_key`; one with it is treated alike).
    fn stoppable(&self, seq: Sequence) -> bool {
        match &seq.movies.items[self.item].kind {
            MovieItemKind::Video { can_be_stopped, .. } => *can_be_stopped,
            MovieItemKind::Image { .. } => true,
        }
    }

    /// Stops the current item's sounds.
    pub fn stop(&mut self, audio: &mut dyn AudioOut) {
        for v in self.voices.drain(..) {
            audio.stop(v);
        }
    }

    /// Adds the frame at `ctx.clock_ms` to `scene`: the background, then
    /// the current item.
    pub(crate) fn draw(&self, seq: Sequence, ctx: &mut Ctx, scene: &mut Scene) -> io::Result<()> {
        let data = ctx.data;
        if let Some(global) = &seq.movies.global_wnd {
            let wnd = global.window.absolute(0.0, 0.0);
            for s in &global.window.auto_statics {
                let abs = s.window.absolute(wnd.x, wnd.y);
                ctx.picture(scene, "background", s, abs, 0.0, 0, false, None)?;
            }
        }
        if self.finished(seq) {
            return Ok(());
        }
        let now = ctx.clock_ms;
        let t = now.saturating_sub(self.item_start);
        match &seq.movies.items[self.item].kind {
            MovieItemKind::Video { texture, .. } => {
                let delay = delay_ms(&seq.movies.items[self.item].kind);
                if t < delay {
                    return Ok(());
                }
                let picture = Picture::resolve(texture, &data.descriptions);
                let resolved = ctx.assets.texture(ctx.tree, data, &picture.texture, now)?;
                let size = resolved.size();
                let rect =
                    picture
                        .rect
                        .unwrap_or(Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32));
                // Section 6.3: centred on (512, 384), 1024 wide, as tall as
                // the rectangle's proportions give, taller on wide screens.
                let mut height = UI_WIDTH * rect.height / rect.width;
                if ctx.resolution.is_wide() {
                    height *= WIDE_VIDEO_STRETCH;
                }
                let placement = PicturePlacement {
                    position: (0.0, UI_HEIGHT / 2.0 - height / 2.0),
                    size: (UI_WIDTH, height),
                    texture_rect: rect,
                    texture_size: (size.0 as f32, size.1 as f32),
                    color: Color::WHITE,
                };
                let id = resolved.id(self.item_start + delay, false);
                scene
                    .sprites
                    .push(Sprite::picture("video", id, &placement.quad(ctx.scale())));
            }
            MovieItemKind::Image { pictures, .. } => {
                let seconds = t as f32 / 1000.0;
                for (i, p) in pictures.iter().enumerate() {
                    if !p.visible_at(seconds) {
                        continue;
                    }
                    let base = p.desc.texture.as_ref().map_or(Color::WHITE, |t| t.color);
                    let color = p
                        .light_anim
                        .as_deref()
                        .and_then(ColorAnimation::named)
                        .filter(|_| p.la_texture)
                        .map(|anim| {
                            let c = anim.color_at(f64::from(seconds - p.start_time));
                            if p.la_alpha {
                                Color { a: c.a, ..base }
                            } else {
                                c
                            }
                        });
                    let abs = p.desc.window.absolute(0.0, 0.0);
                    let layer = format!("picture {i}");
                    ctx.picture(scene, &layer, &p.desc, abs, 0.0, 0, false, color)?;
                }
            }
        }
        Ok(())
    }

    pub fn state(&self, seq: Sequence, now: u64) -> MovieSequenceState {
        let kind = seq.movies.items.get(self.item).map(|i| match i.kind {
            MovieItemKind::Video { .. } => "video",
            MovieItemKind::Image { .. } => "image",
        });
        MovieSequenceState {
            item: self.item,
            items: seq.movies.items.len(),
            kind,
            elapsed_ms: now.saturating_sub(self.item_start),
            length_ms: (self.item < seq.movies.items.len())
                .then(|| Self::length_ms(seq, self.item)),
            sounds: self.voices.clone(),
            skipped: self.skipped,
        }
    }
}

/// The logo sequence `intro_logo` (section 6).
#[derive(Clone, Debug)]
pub struct Startup(MoviePlayer);

impl Startup {
    /// The sequence, its first item current from `now`.
    pub fn new(now: u64) -> Self {
        Self(MoviePlayer::new(now))
    }

    pub fn finished(&self, data: &GameData) -> bool {
        self.0.finished(data.intro_logo())
    }

    /// See [`MoviePlayer::update`].
    pub fn update(
        &mut self,
        data: &GameData,
        now: u64,
        quit_presses: u32,
        audio: &mut dyn AudioOut,
    ) {
        self.0
            .update(data.intro_logo(), data, now, quit_presses, audio);
    }

    pub fn stop(&mut self, audio: &mut dyn AudioOut) {
        self.0.stop(audio);
    }

    /// See [`MoviePlayer::current`].
    pub fn current<'a>(&self, data: &'a GameData) -> Option<(usize, Option<&'a str>)> {
        self.0.current(data.intro_logo())
    }

    /// The frame at `ctx.clock_ms`.
    pub fn scene(&self, ctx: &mut Ctx) -> io::Result<Scene> {
        let mut scene = Scene::new(ctx.resolution);
        self.0.draw(ctx.data.intro_logo(), ctx, &mut scene)?;
        Ok(scene)
    }

    pub fn state(&self, data: &GameData, now: u64) -> MovieSequenceState {
        self.0.state(data.intro_logo(), now)
    }
}

/// A movie sequence as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct MovieSequenceState {
    pub item: usize,
    pub items: usize,
    pub kind: Option<&'static str>,
    pub elapsed_ms: u64,
    pub length_ms: Option<u64>,
    pub sounds: Vec<Voice>,
    pub skipped: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{self, FakeAudio};
    use crate::scene::TextureId;
    use volya_engine_formats::ui::Resolution;

    const XGA: Resolution = Resolution::new(1024, 768);
    const HD: Resolution = Resolution::new(1920, 1080);

    fn scene(
        s: &Startup,
        tree: &fixture::MemVfs,
        data: &GameData,
        r: Resolution,
        now: u64,
    ) -> Scene {
        let mut assets = fixture::assets();
        let mut ctx = Ctx {
            tree,
            data,
            assets: &mut assets,
            resolution: r,
            clock_ms: now,
        };
        s.scene(&mut ctx).unwrap()
    }

    #[test]
    fn items_play_in_order_on_the_clock() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let mut audio = FakeAudio::default();
        let mut s = Startup::new(100);
        // 1 s of delay: silent.
        s.update(&data, 1099, 0, &mut audio);
        assert!(audio.started.is_empty());
        assert_eq!(s.state(&data, 1099).length_ms, Some(3500));
        // Then both files of the pair, left and right, effects volume.
        s.update(&data, 1100, 0, &mut audio);
        let started: Vec<(&str, f32, Category)> = audio
            .started
            .iter()
            .map(|(_, p, b)| (p.as_str(), b.pan, b.category))
            .collect();
        assert_eq!(
            started,
            [
                ("gamedata\\sounds\\v\\logo_l.ogg", -1.0, Category::Effects),
                ("gamedata\\sounds\\v\\logo_r.ogg", 1.0, Category::Effects),
            ]
        );
        assert!(!audio.started[0].2.looping);
        // The item ends 1 s + 2.5 s after it started; the image item
        // follows at exactly that time, and the sounds are stopped.
        s.update(&data, 3599, 0, &mut audio);
        assert_eq!(s.state(&data, 3599).item, 0);
        s.update(&data, 3650, 0, &mut audio);
        let st = s.state(&data, 3650);
        assert_eq!((st.item, st.kind, st.elapsed_ms), (1, Some("image"), 50));
        assert!(audio.playing.is_empty());
        // 9 s later the sequence is over.
        s.update(&data, 12_599, 0, &mut audio);
        assert!(!s.finished(&data));
        s.update(&data, 12_600, 0, &mut audio);
        assert!(s.finished(&data));
    }

    #[test]
    fn each_quit_press_skips_one_item() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let mut audio = FakeAudio::default();
        let mut s = Startup::new(0);
        s.update(&data, 1500, 0, &mut audio);
        assert_eq!(audio.playing.len(), 2);
        s.update(&data, 1600, 1, &mut audio);
        let st = s.state(&data, 1600);
        assert_eq!((st.item, st.elapsed_ms, st.skipped), (1, 0, 1));
        assert!(audio.playing.is_empty(), "the skipped item's sounds stop");
        // The image item now lasts its full 9 s from the press.
        s.update(&data, 10_599, 0, &mut audio);
        assert!(!s.finished(&data));
        s.update(&data, 10_600, 0, &mut audio);
        assert!(s.finished(&data));
        // Two presses at once skip two items.
        let mut s = Startup::new(0);
        s.update(&data, 10, 2, &mut audio);
        assert!(s.finished(&data));
    }

    #[test]
    fn video_is_centred_and_taller_on_wide_screens() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let s = Startup::new(0);
        let black = scene(&s, &tree, &data, XGA, 999);
        assert_eq!(black.sprites.len(), 1);
        assert_eq!(black.sprites[0].layer, "background");
        assert_eq!(black.sprites[0].bounds(), [-0.5, -0.5, 1023.5, 767.5]);
        let x = scene(&s, &tree, &data, XGA, 1000);
        let v = x.first("video").unwrap();
        // 720 × 484 of a 1024 × 512 texture; 1024 × 688.36 units centred.
        let h = 1024.0 * 484.0 / 720.0;
        let top = (384.0 - h / 2.0f32).floor();
        assert_eq!(v.bounds(), [-0.5, top - 0.5, 1023.5, top + h - 0.5]);
        assert_eq!(
            (v.uv_min, v.uv_max),
            ([0.0, 1.0 / 512.0], [720.0 / 1024.0, 485.0 / 512.0])
        );
        assert_eq!(
            v.texture,
            TextureId::Video {
                path: "gamedata\\textures\\intro\\logo.ogm".into(),
                start_ms: 1000,
                looping: false
            }
        );
        let w = scene(&s, &tree, &data, HD, 1000);
        let b = w.first("video").unwrap().bounds();
        assert!(((b[3] - b[1]) - h * 1.328 * 1.40625).abs() < 1e-3, "{b:?}");
    }

    #[test]
    fn pictures_fade_inside_their_window() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let mut s = Startup::new(0);
        s.update(&data, 0, 1, &mut FakeAudio::default());
        let alpha = |t: u64| {
            let sc = scene(&s, &tree, &data, XGA, t);
            (0..2)
                .map(|i| sc.first(&format!("picture {i}")).map(|p| p.color.a))
                .collect::<Vec<_>>()
        };
        // Strictly after start_time: nothing at 0 s.
        assert_eq!(alpha(0), [None, None]);
        assert_eq!(alpha(533 + 1), [Some(127), None]);
        assert_eq!(alpha(2000), [Some(255), None]);
        assert_eq!(alpha(4000), [None, None]);
        assert_eq!(alpha(6000), [None, Some(255)]);
        assert_eq!(alpha(8500), [None, None]);
        let sc = scene(&s, &tree, &data, XGA, 2000);
        let p = sc.first("picture 0").unwrap();
        assert_eq!((p.color.r, p.color.g, p.color.b), (255, 255, 255));
        assert_eq!(p.bounds(), [-0.5, -0.5, 1023.5, 767.5]);
        assert_eq!((p.uv_min, p.uv_max), ([0.0, 0.0], [0.5, 1.0]));
    }

    #[test]
    fn asus_fade() {
        let a = ColorAnimation::named("asus_logo_01").unwrap();
        assert_eq!(a.color_at(0.0).a, 0);
        // Frame 16 of 0 → 32: half way, fraction discarded.
        assert_eq!(a.color_at(16.0 / 30.0 + 1e-6).a, 127);
        assert_eq!(a.color_at(1.1).a, 255);
        assert_eq!(a.color_at(2.0).a, 255);
        // Frame 102: 17 of 34 frames into the fade-out.
        assert_eq!(a.color_at(102.0 / 30.0 + 1e-6).a, 127);
        assert_eq!(a.color_at(3.97).a, 0);
        assert_eq!(a.color_at(10.0).a, 0);
        assert_eq!(a.color_at(1.0).r, 0);
        assert!(ColorAnimation::named("other").is_none());
    }
}
