// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Scenes to renderer draw lists: textures loaded from the game tree as
//! stored, videos decoded frame by frame to the clock (playing from the
//! start time their sprite names, looping or holding their last frame), the
//! splash picture.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use crate::core::vfs::{self, Vfs};
use crate::level::LevelWorld;
use crate::render::world::multiply;
use crate::render::{
    DrawList, Quad, Rect, Renderer, Texture, VideoTexture, World, WorldDraw, WorldFrame,
};
use crate::scene::{Scene, TextureId, WorldScene};
use volya_engine_formats::dds::Dds;
use volya_engine_formats::pe::Rgba;
use volya_engine_formats::ui::{UI_HEIGHT, UI_WIDTH};
use volya_engine_formats::video::TheoraVideo;

/// Most video frames decoded per video in one call when playback lags.
const MAX_DECODE: u32 = 4;

struct Player {
    video: TheoraVideo<Cursor<Vec<u8>>>,
    texture: VideoTexture,
    fps: f64,
    start_ms: u64,
    looping: bool,
    /// Passes through the video completed (looping).
    pass: u64,
    /// Frames decoded in this pass.
    decoded: u64,
    /// Frames in the video, once its end was reached.
    frames: Option<u64>,
}

impl Player {
    /// Decodes up to the frame due at `clock`.
    fn advance(&mut self, renderer: &Renderer, clock: u64) -> Result<(), String> {
        let elapsed = clock.saturating_sub(self.start_ms) as f64;
        let due = (elapsed * self.fps / 1000.0).floor() as u64;
        let mut budget = MAX_DECODE;
        // At most two rounds: the second after learning the frame count.
        for _ in 0..2 {
            let (pass, index) = match self.frames {
                Some(0) => return Err("video without frames".into()),
                Some(n) if self.looping => (due / n, due % n),
                Some(n) => (0, due.min(n - 1)),
                None => (0, due),
            };
            if pass != self.pass {
                self.video.rewind().map_err(|e| e.to_string())?;
                self.pass = pass;
                self.decoded = 0;
            }
            while self.decoded <= index && budget > 0 {
                budget -= 1;
                match self.video.next_frame().map_err(|e| e.to_string())? {
                    Some(frame) => {
                        renderer.update_video(&mut self.texture, &frame);
                        self.decoded += 1;
                    }
                    None => {
                        self.frames = Some(self.decoded);
                        break;
                    }
                }
            }
            if !(self.looping && self.frames.is_some() && self.decoded <= index) {
                break;
            }
        }
        Ok(())
    }
}

/// GPU resources of the scenes drawn.
#[derive(Default)]
pub struct Presenter {
    /// Loaded images; `None` for one that failed (reported once).
    images: HashMap<String, Option<Texture>>,
    videos: HashMap<String, Player>,
    splash: Option<Texture>,
    white: Option<Texture>,
    /// The static world on the GPU and the level it was made from; `None`
    /// inside when the upload failed (reported once).
    world: Option<(Arc<LevelWorld>, Option<World>)>,
    errors: Vec<String>,
}

impl Presenter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the splash picture.
    pub fn set_splash(&mut self, renderer: &Renderer, image: &Rgba) {
        self.splash = Some(renderer.create_rgba_texture(image.width, image.height, &image.pixels));
    }

    /// Errors met since the last call (each texture or video fails once and
    /// is then left out).
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    fn image(&mut self, renderer: &Renderer, tree: &dyn Vfs, path: &str) -> Option<Texture> {
        if let Some(t) = self.images.get(path) {
            return t.clone();
        }
        let loaded = vfs::read(tree, path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                let dds = Dds::parse(&bytes).map_err(|e| e.to_string())?;
                renderer.create_texture(&dds).map_err(|e| e.to_string())
            });
        let texture = loaded
            .map_err(|e| self.errors.push(format!("{path}: {e}")))
            .ok();
        self.images.insert(path.to_owned(), texture.clone());
        texture
    }

    fn video(
        &mut self,
        renderer: &Renderer,
        tree: &dyn Vfs,
        path: &str,
        start_ms: u64,
        looping: bool,
        clock: u64,
    ) -> Option<Texture> {
        let stale = self
            .videos
            .get(path)
            .is_some_and(|p| p.start_ms != start_ms || p.looping != looping);
        if stale {
            self.videos.remove(path);
        }
        if !self.videos.contains_key(path) {
            let opened = vfs::read(tree, path)
                .and_then(|bytes| TheoraVideo::open(Cursor::new(bytes)))
                .map_err(|e| e.to_string());
            match opened {
                Ok(video) => {
                    let info = video.info().clone();
                    let player = Player {
                        texture: renderer.create_video_texture(info.width, info.height),
                        video,
                        fps: f64::from(info.fps_numerator) / f64::from(info.fps_denominator.max(1)),
                        start_ms,
                        looping,
                        pass: 0,
                        decoded: 0,
                        frames: None,
                    };
                    self.videos.insert(path.to_owned(), player);
                }
                Err(e) => {
                    self.errors.push(format!("{path}: {e}"));
                    return None;
                }
            }
        }
        let player = self.videos.get_mut(path).expect("player inserted");
        if let Err(e) = player.advance(renderer, clock) {
            self.errors.push(format!("{path}: {e}"));
            self.videos.remove(path);
            return None;
        }
        Some(player.texture.texture().clone())
    }

    /// The GPU world of the current level, once uploaded.
    pub fn world(&self) -> Option<&World> {
        self.world.as_ref().and_then(|(_, w)| w.as_ref())
    }

    /// The GPU copy of `scene`'s static world, uploaded on first sight.
    fn world_frame(&mut self, renderer: &mut Renderer, scene: &WorldScene) -> Option<WorldFrame> {
        let current = self
            .world
            .as_ref()
            .is_some_and(|(level, _)| Arc::ptr_eq(level, &scene.world));
        if !current {
            // The previous world's buffers go before the next's arrive.
            self.world = None;
            let uploaded = match scene.world.take_geometry() {
                Some(geom) => {
                    let draws: Vec<WorldDraw> = scene
                        .world
                        .draw_items
                        .iter()
                        .map(|d| WorldDraw {
                            vertex_buffer: d.vertex_buffer,
                            first_vertex: d.first_vertex,
                            index_buffer: d.index_buffer,
                            first_index: d.first_index,
                            index_count: 3 * d.triangle_count,
                            transform: d.transform,
                        })
                        .collect();
                    renderer.create_world(&geom, &draws)
                }
                None => Err("geometry already uploaded elsewhere".into()),
            };
            let world = uploaded
                .map_err(|e| self.errors.push(format!("{}: {e}", scene.world.dir)))
                .ok();
            self.world = Some((scene.world.clone(), world));
        }
        let world = self.world.as_ref()?.1.clone()?;
        Some(WorldFrame {
            world,
            view_projection: multiply(&scene.view, &scene.projection),
            // No sky yet: the fog colour shows (our choice).
            clear: scene.environment.fog_rgb(),
        })
    }

    /// The draw list of `scene` at engine clock `clock`. Videos no longer in
    /// the scene are released, and so is a world no longer shown.
    pub fn draw_list(
        &mut self,
        renderer: &mut Renderer,
        tree: &dyn Vfs,
        scene: &Scene,
        clock: u64,
    ) -> DrawList {
        let (w, h) = (
            scene.resolution.width as f32,
            scene.resolution.height as f32,
        );
        // Output pixels with integer centres (the scene's convention) to UI
        // units with the renderer's half-pixel centres.
        let to_ui = |[x, y]: [f32; 2]| [(x + 0.5) * UI_WIDTH / w, (y + 0.5) * UI_HEIGHT / h];
        let mut list = DrawList::default();
        match &scene.world {
            Some(world) => list.world = self.world_frame(renderer, world),
            None => self.world = None,
        }
        for sprite in &scene.sprites {
            let texture = match &sprite.texture {
                TextureId::Image { path } => self.image(renderer, tree, path),
                TextureId::Video {
                    path,
                    start_ms,
                    looping,
                } => self.video(renderer, tree, path, *start_ms, *looping, clock),
                TextureId::Splash => self.splash.clone(),
                TextureId::White => Some(
                    self.white
                        .get_or_insert_with(|| renderer.create_rgba_texture(1, 1, &[255; 4]))
                        .clone(),
                ),
            };
            let Some(texture) = texture else { continue };
            let (tw, th) = texture.size();
            let (tw, th) = (tw as f32, th as f32);
            let corners = sprite.corners.map(to_ui);
            let mut quad = Quad::new(Rect::new(0.0, 0.0, 0.0, 0.0), &texture);
            quad.corners = Some(corners);
            quad.source = Rect::new(
                sprite.uv_min[0] * tw,
                sprite.uv_min[1] * th,
                (sprite.uv_max[0] - sprite.uv_min[0]) * tw,
                (sprite.uv_max[1] - sprite.uv_min[1]) * th,
            );
            let c = sprite.color;
            quad.color = [c.r, c.g, c.b, c.a];
            list.quads.push(quad);
        }
        self.videos.retain(|path, _| {
            scene
                .sprites
                .iter()
                .any(|s| matches!(&s.texture, TextureId::Video { path: p, .. } if p == path))
        });
        list
    }
}
