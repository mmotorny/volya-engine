// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Scenes to renderer draw lists: textures loaded from the game tree as
//! stored, videos decoded frame by frame to the clock (playing from the
//! start time their sprite names, looping or holding their last frame), the
//! splash picture.

use std::collections::HashMap;
use std::sync::Arc;

use crate::core::camera::Mat4;
use crate::core::glows::{CORNER_UV, GlowSprite};
use crate::core::texture_quality::TextureQuality;
use crate::core::vfs::read_ahead::ReadAhead;
use crate::core::vfs::{self, Vfs};
use crate::level::LevelWorld;
use crate::level_visibility::Seen;
use crate::particle_drawing::{ParticleDraw, ParticleShader};
use crate::render::details::{DetailDraw, DetailFrame};
use crate::render::gl::Owner;
use crate::render::particles::{
    ParticleBatch, ParticleBlend, ParticleCull, ParticlePass as RenderPass, ParticleProgram,
    ParticleQueue, ParticleVertex,
};
use crate::render::present::ScreenEffects;
use crate::render::sky::{SkySection, SkyTextures};
use crate::render::world::{
    GeometryUpload, LightPass, LightTextures, ObjectGeometry, ProjectedCell, ProjectedFrame,
    ProjectedReceiver, Surface, TreeLight, WorldLight, cell_view_projection,
};
use crate::render::{
    DrawList, MenuLens, Quad, Rect, Renderer, Texture, VideoTexture, World, WorldDraw, WorldFrame,
};
use crate::scene::{Scene, TextureId, WorldScene};
use volya_engine_formats::dds::Dds;
use volya_engine_formats::level::GEOMETRY_LOD;
use volya_engine_formats::pe::Rgba;
use volya_engine_formats::ui::{UI_HEIGHT, UI_WIDTH};
use volya_engine_formats::video::TheoraVideo;

/// `r1_lmodel_lerp` before set: the shipped `user.ltx`'s and the console's
/// default (level-objects.md 5.5).
pub const LMODEL_LERP: f32 = 0.1;

/// Most video frames decoded per video in one call when playback lags.
const MAX_DECODE: u32 = 4;

struct Player {
    /// Decodes from `source`, which is topped up from the tree before each
    /// call, so only a window of the file is in memory.
    video: TheoraVideo<ReadAhead>,
    source: ReadAhead,
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
    fn advance(&mut self, renderer: &Renderer, tree: &dyn Vfs, clock: u64) -> Result<(), String> {
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
                self.source.top_up(tree).map_err(|e| e.to_string())?;
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

/// The lens's offset map `map` drawn as `q`, in pixels of a distortion
/// target of `size` with integer centres, as a quad in UI units
/// (title-menu.md 4.6): white, the texture's coordinates as placed.
fn lens_quad(q: &volya_engine_formats::ui::picture::Quad, size: (u32, u32), map: &Texture) -> Quad {
    let (w, h) = (size.0 as f32, size.1 as f32);
    let ui = |(x, y): (f32, f32)| ((x + 0.5) * UI_WIDTH / w, (y + 0.5) * UI_HEIGHT / h);
    let (min, max) = (ui(q.min), ui(q.max));
    let mut quad = Quad::new(Rect::new(min.0, min.1, max.0 - min.0, max.1 - min.1), map);
    let (tw, th) = map.size();
    let (tw, th) = (tw as f32, th as f32);
    quad.source = Rect::new(
        q.uv_min.0 * tw,
        q.uv_min.1 * th,
        (q.uv_max.0 - q.uv_min.0) * tw,
        (q.uv_max.1 - q.uv_min.1) * th,
    );
    quad
}

/// The texture of the DDS file at `path` of `tree`: as the platform
/// prepared it, else read and uploaded ([`Renderer::create_texture`]) with
/// `quality`'s top levels dropped (texture-quality-filtering.md 1).
fn load_texture(
    renderer: &Renderer,
    tree: &dyn Vfs,
    quality: &TextureQuality,
    path: &str,
) -> Result<Texture, String> {
    // Prepared while the level loaded (transcoded for this GPU, #113).
    if let Some(texture) = renderer.take_prepared(path) {
        return Ok(texture);
    }
    let bytes = vfs::read(tree, path).map_err(|e| e.to_string())?;
    let dds = Dds::parse(&bytes).map_err(|e| e.to_string())?;
    let dds = quality.apply(path, &dds);
    renderer.create_texture(&dds).map_err(|e| e.to_string())
}

/// The portal fade's batch (portal-fade.md 2 – 5) of its triangles
/// `fade`, none without any: `portal` with alpha blending, depth tested,
/// no depth writes, no culling.
fn fade_batch(fade: &[crate::render::particles::ParticleVertex]) -> Option<ParticleBatch> {
    (!fade.is_empty()).then(|| ParticleBatch {
        pass: RenderPass {
            program: ParticleProgram::Portal,
            blend: ParticleBlend::Alpha,
            depth_write: false,
            clamp: false,
            base: None,
            distort: None,
        },
        queue: ParticleQueue::Fade,
        cull: ParticleCull::None,
        key: 0.0,
        vertices: fade.to_vec(),
    })
}

/// The world's draws of `level`: its static draws, then its objects'.
fn world_draws(level: &LevelWorld) -> Vec<WorldDraw> {
    level
        .draw_items
        .iter()
        .zip(&level.draw_surfaces)
        .zip(&level.spheres)
        .filter_map(|((d, surface), sphere)| {
            Some(WorldDraw {
                surface: (*surface)?,
                vertex_buffer: d.vertex_buffer,
                first_vertex: d.first_vertex,
                index_buffer: d.index_buffer,
                first_index: d.first_index,
                index_count: 3 * d.triangle_count,
                transform: d.transform,
                sphere: *sphere,
                tree: d.tree.map(|t| TreeLight {
                    scale: t.scale,
                    bias: t.bias,
                }),
                object: None,
                windows: [0, 0],
            })
        })
        .chain(level.objects.draws.iter().map(|d| WorldDraw {
            surface: d.surface,
            vertex_buffer: 0,
            first_vertex: d.first_vertex,
            index_buffer: 0,
            first_index: d.first_index,
            index_count: d.index_count,
            transform: d.transform,
            // Offline at the start: uploaded hidden (a radius of 0,
            // [`Presenter::place_objects`]).
            sphere: if level.objects.instances[d.instance].online {
                d.sphere
            } else {
                [d.sphere[0], d.sphere[1], d.sphere[2], 0.0]
            },
            tree: None,
            object: Some(d.dynamic_props),
            windows: d.windows,
        }))
        .collect()
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
    /// The objects' placements last applied to the world's draws; `None`
    /// as uploaded.
    placed: Option<Arc<Vec<Option<Mat4>>>>,
    /// The world's draw index of the first object draw.
    first_object_draw: usize,
    /// Each static draw item's world draw index (none: not drawn).
    static_draw_of: Vec<Option<u32>>,
    /// The projected light cells as last handed to the renderer, by slot.
    projected_cells: std::cell::RefCell<HashMap<u32, Arc<ProjectedCell>>>,
    /// The latest camera pass's draws and the world draws they keep.
    drawn: Option<(Arc<Seen>, Arc<Vec<bool>>)>,
    /// The level's effect shaders with their textures, by library index
    /// (particle-drawing.md 5.1), loaded with the world.
    particle_shaders: Vec<Option<ParticleShader<Texture>>>,
    /// The level's glow shaders with their textures (glows.md 6), loaded
    /// with the world; `None` for one whose texture failed.
    glow_shaders: Vec<Option<(Texture, bool)>>,
    /// A level's geometry uploaded while it loaded
    /// ([`Presenter::prepare_world`]).
    prepared: Option<PreparedWorld>,
    /// The level whose world is kept through scenes without one
    /// ([`Presenter::hold_world`]).
    held: Option<Arc<LevelWorld>>,
    /// `texture_lod` and its list as the latest scene had them, for the
    /// textures loaded now (texture-quality-filtering.md 1.3, 2).
    quality: TextureQuality,
    errors: Vec<String>,
}

/// A level's geometry on its way to the GPU before the level shows (#184).
struct PreparedWorld {
    level: Arc<LevelWorld>,
    upload: GeometryUpload,
    objects: ObjectGeometry,
    draws: Vec<WorldDraw>,
    done: bool,
}

impl Presenter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the splash picture.
    pub fn set_splash(&mut self, renderer: &Renderer, image: &Rgba) {
        self.splash = Some(renderer.owning(Owner::Images, || {
            renderer.create_rgba_texture(image.width, image.height, &image.pixels)
        }));
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
                let dds = self.quality.apply(path, &dds);
                renderer
                    .owning(Owner::Images, || renderer.create_texture(&dds))
                    .map_err(|e| e.to_string())
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
            let opened = ReadAhead::open(tree, path)
                .and_then(|source| Ok((TheoraVideo::open(source.clone())?, source)))
                .map_err(|e| e.to_string());
            match opened {
                Ok((video, source)) => {
                    let info = video.info().clone();
                    let player = Player {
                        texture: renderer.create_video_texture(info.width, info.height),
                        video,
                        source,
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
        if let Err(e) = player.advance(renderer, tree, clock) {
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

    /// The GPU copy of `scene`'s static world, uploaded on first sight:
    /// its geometry and its surfaces' textures, read from `tree`, and the
    /// objects' geometry (taken from the level).
    fn world_frame(
        &mut self,
        renderer: &mut Renderer,
        tree: &dyn Vfs,
        scene: &WorldScene,
    ) -> Option<WorldFrame> {
        let quality = self.quality.clone();
        let current = self
            .world
            .as_ref()
            .is_some_and(|(level, _)| Arc::ptr_eq(level, &scene.world));
        if !current {
            // The previous world's buffers go before the next's arrive,
            // with its effects' textures.
            self.world = None;
            self.placed = None;
            self.particle_shaders.clear();
            self.glow_shaders.clear();
            renderer.clear_level_textures();
            // The effects' textures first: the world's upload drops the
            // prepared textures it leaves.
            self.particle_shaders = self.load_particle_shaders(renderer, tree, &scene.world);
            // Buffers for every particle the level's effects can hold, made
            // now rather than while the level runs.
            renderer.reserve_particles(scene.world.particle_capacity);
            self.load_lights(renderer, tree, &scene.world);
            // The detail models with the world (detail-objects.md 6), so
            // nothing is made while the level runs.
            renderer.clear_details();
            if let Some(d) = &scene.world.details {
                match load_texture(renderer, tree, &quality, &d.texture) {
                    Ok(t) => renderer.set_details(&d.models, &t, d.alpha_test, d.capacity),
                    Err(e) => self.errors.push(format!("{}: {e}", d.texture)),
                }
            }
            self.projected_cells.borrow_mut().clear();
            // The billboards' sets with the world (static-shading.md 8.4).
            renderer.clear_billboards();
            if let Some(b) = &scene.world.billboards {
                let sets: Result<Vec<_>, _> = (b.sets.iter())
                    .map(|[a, h]| {
                        let t = |p: &String| {
                            load_texture(renderer, tree, &quality, p)
                                .map_err(|e| format!("{p}: {e}"))
                        };
                        Ok::<_, String>((t(a)?, t(h)?))
                    })
                    .collect();
                match sets {
                    Ok(sets) => renderer.set_billboards(&sets, b.groups),
                    Err(e) => self.errors.push(e),
                }
            }
            if let Some(path) = &scene.world.presentation_noise {
                match load_texture(renderer, tree, &quality, path) {
                    Ok(t) => renderer.set_presentation_noise(&t),
                    Err(e) => self.errors.push(format!("{path}: {e}")),
                }
            }
            let world = tracing::info_span!("render.upload_world")
                .in_scope(|| self.upload_world(renderer, tree, &scene.world));
            let world = world
                .map_err(|e| self.errors.push(format!("{}: {e}", scene.world.dir)))
                .ok();
            self.world = Some((scene.world.clone(), world));
        }
        let world = self.world.as_ref()?.1.clone()?;
        self.place_objects(renderer, &world, scene);
        let drawn = scene.seen.as_ref().map(|s| self.drawn(&scene.world, s));
        let e = &scene.environment;
        Some(WorldFrame {
            drawn,
            world,
            view: scene.view,
            projection: scene.projection,
            eye: scene.eye,
            forward: scene.forward,
            fov_y_degrees: scene.fov_y_degrees,
            light: WorldLight {
                ambient: e.ambient,
                hemi: [e.hemi_color[0], e.hemi_color[1], e.hemi_color[2]],
                sun_color: e.sun_color,
                sun_dir: e.sun_dir,
                fog_color: e.fog_rgb(),
                fog_near: e.fog_near,
                fog_far: e.fog_far,
            },
            time: scene.time,
            sky: scene.sky,
            // Where neither the world nor the sky draws (does not occur:
            // the sky box encloses the eye; sky.md 14) and without a sky.
            clear: e.fog_rgb(),
            particles: {
                let mut batches = self.particle_batches(&scene.particles);
                batches.extend(self.glow_batches(&scene.glows));
                batches.extend(scene.seen.as_ref().and_then(|s| fade_batch(&s.fade)));
                batches
            },
            lights: scene
                .lights
                .iter()
                .map(|l| LightPass {
                    spot: l.spot,
                    position: l.position,
                    colour: l.colour,
                    xform: l.xform,
                    frustum: l.frustum,
                })
                .collect(),
            presentation: Default::default(),
            filtering: Default::default(),
            geometry_lod: GEOMETRY_LOD,
            effects: None,
            projected: scene.seen.as_ref().map(|s| self.projected(&scene.world, s)),
            billboards: (scene.seen.as_ref())
                .map(|s| s.billboards.clone())
                .filter(|b| !b.draws.is_empty()),
            details: scene.details.as_ref().map(|d| DetailFrame {
                instances: d.frame.instances.clone(),
                draws: (d.frame.draws.iter())
                    .map(|d| DetailDraw {
                        model: d.model,
                        list: d.list,
                        first: d.first,
                        count: d.count,
                    })
                    .collect(),
                sway: [
                    d.sway.amp1,
                    d.sway.amp2,
                    d.sway.rot1,
                    d.sway.rot2,
                    d.sway.speed,
                ],
                time: d.time,
            }),
        })
    }

    /// The world draws `seen` keeps (in [`world_draws`]' order), made
    /// again only when it changes.
    /// The projected light texture's part of the frame
    /// (object-projected-light.md): the cells drawn, with their draws as
    /// the world's, and every object draw of each receiver.
    fn projected(&self, level: &LevelWorld, seen: &Seen) -> ProjectedFrame {
        let mut cached = self.projected_cells.borrow_mut();
        let cells = (seen.cells.iter())
            .map(|c| {
                if let Some(p) = cached.get(&c.slot)
                    && p.version == c.version
                {
                    return p.clone();
                }
                let p = Arc::new(ProjectedCell {
                    version: c.version,
                    slot: c.slot,
                    view_projection: cell_view_projection(c.centre, c.h),
                    clear: c.clear,
                    draws: (c.draws.iter())
                        .filter_map(|&d| *self.static_draw_of.get(d as usize)?)
                        .collect(),
                });
                cached.insert(c.slot, p.clone());
                p
            })
            .collect();
        let mut receivers = Vec::new();
        if !seen.receivers.is_empty() {
            let mut slot_of = vec![None; level.objects.instances.len()];
            for r in &seen.receivers {
                if let Some(s) = slot_of.get_mut(r.instance) {
                    *s = Some(r);
                }
            }
            for (j, d) in level.objects.draws.iter().enumerate() {
                if let Some(Some(r)) = slot_of.get(d.instance) {
                    receivers.push(ProjectedReceiver {
                        draw: (self.first_object_draw + j) as u32,
                        slot: r.slot,
                        centre: r.centre,
                        h: r.h,
                    });
                }
            }
        }
        ProjectedFrame {
            cells,
            receivers,
            lerp: LMODEL_LERP,
        }
    }

    fn drawn(&mut self, level: &LevelWorld, seen: &Arc<Seen>) -> Arc<Vec<bool>> {
        if let Some((s, d)) = &self.drawn
            && Arc::ptr_eq(s, seen)
        {
            return d.clone();
        }
        let items = (level.draw_items.iter().enumerate())
            .zip(&level.draw_surfaces)
            .filter(|(_, surface)| surface.is_some())
            .map(|((i, _), _)| seen.items.get(i).copied().unwrap_or(false));
        let objects = (level.objects.draws.iter())
            .map(|d| seen.objects.get(d.instance).copied().unwrap_or(false));
        let d = Arc::new(items.chain(objects).collect::<Vec<bool>>());
        self.drawn = Some((seen.clone(), d.clone()));
        d
    }

    /// The level's glow textures, and the light passes' attenuation
    /// textures given to the renderer; one that fails is reported and
    /// leaves its glows, or every light pass, undrawn.
    fn load_lights(&mut self, renderer: &mut Renderer, tree: &dyn Vfs, level: &LevelWorld) {
        let quality = self.quality.clone();
        let mut load = |path: &String| {
            load_texture(renderer, tree, &quality, path)
                .map_err(|e| self.errors.push(format!("{path}: {e}")))
                .ok()
        };
        let shaders = level
            .glow_shaders
            .iter()
            .map(|s| Some((load(&s.texture)?, s.flare)))
            .collect();
        let attenuation = level.light_textures.as_ref().and_then(|[a, c, p]| {
            Some(LightTextures {
                att: load(a)?,
                attclip: load(c)?,
                attpoint: load(p)?,
            })
        });
        self.glow_shaders = shaders;
        if let Some(t) = attenuation {
            renderer.set_light_textures(t);
        }
    }

    /// The renderer's batches of the scene's glows: one per shader, in
    /// shader order (glows.md 6; the blending makes the order irrelevant).
    fn glow_batches(&self, sprites: &[GlowSprite]) -> Vec<ParticleBatch> {
        let mut batches: Vec<(u32, ParticleBatch)> = Vec::new();
        for g in sprites {
            let Some(Some((texture, flare))) = self.glow_shaders.get(g.shader as usize) else {
                continue;
            };
            let i = match batches.iter().position(|(s, _)| *s == g.shader) {
                Some(i) => i,
                None => {
                    batches.push((
                        g.shader,
                        ParticleBatch {
                            pass: RenderPass {
                                program: if *flare {
                                    ParticleProgram::Flare
                                } else {
                                    ParticleProgram::Glow
                                },
                                blend: ParticleBlend::AlphaAdd,
                                depth_write: false,
                                clamp: true,
                                base: Some(texture.clone()),
                                distort: None,
                            },
                            queue: ParticleQueue::Glow,
                            // The default culling: triangles appearing
                            // counter-clockwise are removed (glows.md 6).
                            cull: ParticleCull::CounterClockwise,
                            key: 0.0,
                            vertices: Vec::new(),
                        },
                    ));
                    batches.len() - 1
                }
            };
            let colour = u32::from(g.b) * 0x0101_0101;
            batches[i]
                .1
                .vertices
                .extend(
                    g.corners
                        .iter()
                        .zip(CORNER_UV)
                        .map(|(&position, uv)| ParticleVertex {
                            position,
                            colour,
                            uv,
                        }),
                );
        }
        batches.sort_by_key(|(s, _)| *s);
        batches.into_iter().map(|(_, b)| b).collect()
    }

    /// Every effect shader's textures, each file read once; one that fails
    /// is reported and leaves its pass without it (not drawn).
    fn load_particle_shaders(
        &mut self,
        renderer: &Renderer,
        tree: &dyn Vfs,
        level: &LevelWorld,
    ) -> Vec<Option<ParticleShader<Texture>>> {
        let quality = self.quality.clone();
        let mut loaded: HashMap<String, Option<Texture>> = HashMap::new();
        level
            .particle_shaders
            .iter()
            .map(|s| {
                s.clone().map(|s| {
                    s.map(|path| {
                        loaded
                            .entry(path.clone())
                            .or_insert_with(|| {
                                load_texture(renderer, tree, &quality, &path)
                                    .map_err(|e| self.errors.push(format!("{path}: {e}")))
                                    .ok()
                            })
                            .clone()
                    })
                })
            })
            .collect()
    }

    /// The renderer's batches of the scene's queued effects.
    fn particle_batches(&self, draws: &[ParticleDraw]) -> Vec<ParticleBatch> {
        draws
            .iter()
            .filter_map(|d| {
                let shader = self.particle_shaders.get(d.effect)?.as_ref()?;
                let pass = if d.distortion {
                    shader.distortion.as_ref()?
                } else {
                    shader.normal.as_ref()?
                };
                Some(ParticleBatch {
                    pass: RenderPass {
                        program: pass.program,
                        blend: pass.blend,
                        depth_write: pass.depth_write,
                        clamp: pass.clamp,
                        base: pass.base.clone(),
                        distort: pass.distort.clone(),
                    },
                    queue: d.queue,
                    cull: d.cull,
                    key: d.key,
                    vertices: d.vertices.clone(),
                })
            })
            .collect()
    }

    /// Moves, hides or shows the objects' draws whose instances' placements
    /// differ from those last applied (the objects switching online and
    /// offline, offline-simulation.md 6.3). A hidden draw gets a bounding
    /// radius of 0, so it is never drawn.
    /// The world's draw index of object draw `j` of the level's
    /// [`crate::objects::LevelObjects::draws`] (tests).
    pub fn object_draw_index(&self, j: usize) -> usize {
        self.first_object_draw + j
    }

    fn place_objects(&mut self, renderer: &Renderer, world: &World, scene: &WorldScene) {
        if self
            .placed
            .as_ref()
            .is_some_and(|p| Arc::ptr_eq(p, &scene.objects))
        {
            return;
        }
        let objects = &scene.world.objects;
        let before = self.placed.take();
        let first = self.first_object_draw;
        for (j, d) in objects.draws.iter().enumerate() {
            let k = d.instance;
            let now = scene.objects.get(k).copied().flatten();
            let was = match &before {
                Some(b) => b.get(k).copied().flatten(),
                // As uploaded: the instance's own placement if online,
                // hidden otherwise.
                None => objects.instances[k]
                    .online
                    .then_some(objects.instances[k].transform),
            };
            if now == was {
                continue;
            }
            let instance = &objects.instances[k];
            let (transform, sphere) = match now {
                Some(t) => (t, d.sphere_at(&t)),
                None => (
                    instance.transform,
                    [d.sphere[0], d.sphere[1], d.sphere[2], 0.0],
                ),
            };
            renderer.place_draw(world, first + j, &transform, sphere);
        }
        self.placed = Some(scene.objects.clone());
    }

    /// Whether a level's geometry is held, prepared before it shows.
    pub fn preparing_world(&self) -> bool {
        self.prepared.is_some()
    }

    /// Frees what was prepared for a load that was abandoned (#262): its
    /// geometry being uploaded and its prepared textures.
    pub fn abandon_prepared(&mut self, renderer: &Renderer) {
        self.prepared = None;
        renderer.clear_prepared();
    }

    /// Uploads about `budget` more bytes of `level`'s geometry before it
    /// shows (a platform spreading the upload over task turns, #184); the
    /// world is then made from it when the level is first drawn. Returns
    /// whether all of it is uploaded.
    pub fn prepare_world(
        &mut self,
        renderer: &mut Renderer,
        tree: &dyn Vfs,
        level: &Arc<LevelWorld>,
        budget: u64,
    ) -> Result<bool, String> {
        if !self
            .prepared
            .as_ref()
            .is_some_and(|p| Arc::ptr_eq(&p.level, level))
        {
            self.prepared = None;
            let objects = level
                .objects
                .take_geometry()
                .ok_or("object geometry already uploaded elsewhere")?;
            let draws = world_draws(level);
            let upload = renderer.begin_geometry(&level.geometry().layout, &objects, &draws)?;
            self.prepared = Some(PreparedWorld {
                level: level.clone(),
                upload,
                objects,
                draws,
                done: false,
            });
        }
        let Some(p) = self.prepared.as_mut().filter(|p| !p.done) else {
            return Ok(true);
        };
        let path = level.geometry_path();
        let mut file = tree.open(&path).map_err(|e| format!("{path}: {e}"))?;
        let mut source = level.geometry().reader(&mut file);
        p.done = p.upload.step(renderer, &mut source, &p.objects, budget)?;
        if p.done {
            // Uploaded: the CPU copy goes before the level shows.
            p.objects = ObjectGeometry::default();
        }
        Ok(p.done)
    }

    fn upload_world(
        &mut self,
        renderer: &mut Renderer,
        tree: &dyn Vfs,
        level: &LevelWorld,
    ) -> Result<World, String> {
        let quality = self.quality.clone();
        // Prepared while the level loaded, finished here if need be.
        let mut prepared = self
            .prepared
            .take()
            .filter(|p| std::ptr::eq(Arc::as_ptr(&p.level), level));
        let (objects, draws) = match &prepared {
            Some(p) => (None, p.draws.clone()),
            None => (
                Some(
                    level
                        .objects
                        .take_geometry()
                        .ok_or("object geometry already uploaded elsewhere")?,
                ),
                world_draws(level),
            ),
        };
        // Each texture file once; one that fails is left unbound (reported).
        let mut textures: HashMap<String, Option<Texture>> = HashMap::new();
        let surfaces: Vec<Surface<Texture>> = level
            .surfaces
            .iter()
            .map(|s| {
                s.clone().map_textures(|path| {
                    textures
                        .entry(path.clone())
                        .or_insert_with(|| {
                            load_texture(renderer, tree, &quality, &path)
                                .map_err(|e| self.errors.push(format!("{path}: {e}")))
                                .ok()
                        })
                        .clone()
                })
            })
            .collect();
        let mut texture = |path: &String| {
            textures
                .entry(path.clone())
                .or_insert_with(|| {
                    load_texture(renderer, tree, &quality, path)
                        .map_err(|e| self.errors.push(format!("{path}: {e}")))
                        .ok()
                })
                .clone()
        };
        // Any section's cube map or cloud texture failing to upload: no sky
        // at any hour (our choice; reported like every texture). A sun
        // texture failing leaves only that section without its sun.
        let sky = level
            .sky
            .sections
            .iter()
            .map(|s| {
                Some(SkySection {
                    cube: texture(&s.cube)?,
                    clouds: texture(&s.clouds)?,
                    sun: s.sun.as_ref().and_then(&mut texture),
                })
            })
            .collect::<Option<Vec<_>>>()
            .map(|sections| SkyTextures { sections });
        self.first_object_draw = draws.len() - level.objects.draws.len();
        let mut next = 0;
        self.static_draw_of = (level.draw_surfaces.iter())
            .map(|s| {
                s.map(|_| {
                    next += 1;
                    next - 1
                })
            })
            .collect();
        // Prepared textures no surface or effect took are not kept.
        renderer.clear_prepared();
        // The static geometry streams from its file to the GPU, a piece
        // at a time.
        let path = level.geometry_path();
        let mut file = tree.open(&path).map_err(|e| format!("{path}: {e}"))?;
        let mut source = level.geometry().reader(&mut file);
        let world = match (prepared.take(), objects) {
            (Some(mut p), _) => {
                if !p.done {
                    p.upload.step(renderer, &mut source, &p.objects, u64::MAX)?;
                }
                renderer.finish_world(p.upload, &surfaces, &draws, sky.as_ref())?
            }
            (None, Some(objects)) => renderer.create_world_from(
                &mut source,
                &objects,
                &surfaces,
                &draws,
                sky.as_ref(),
            )?,
            (None, None) => unreachable!("the objects are taken without a prepared world"),
        };
        if let Some(e) = world.sky_error() {
            self.errors.push(format!("{}: sky: {e}", level.dir));
        }
        Ok(world)
    }

    /// Keeps the GPU world of `level`, once made, through scenes without a
    /// world (the loading screen's, after the level was drawn unseen ahead
    /// of showing, #355), until called with `None`.
    pub fn hold_world(&mut self, level: Option<Arc<LevelWorld>>) {
        self.held = level;
    }

    fn holds_world(&self) -> bool {
        match (&self.held, &self.world) {
            (Some(held), Some((level, _))) => Arc::ptr_eq(held, level),
            _ => false,
        }
    }

    /// The draw list of `scene` at engine clock `clock`. Images and videos
    /// no longer in the scene are released, and so is a world no longer
    /// shown (unless held, [`Presenter::hold_world`]).
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
        let mut list = DrawList {
            ramp: scene.ramp,
            ..Default::default()
        };
        if self.quality != scene.texture_quality {
            self.quality = scene.texture_quality.clone();
        }
        // The off-screen target, made at the game's size before any level
        // (frame-presentation.md 1.2).
        let screen = (scene.resolution.width, scene.resolution.height);
        renderer.prepare_presentation(screen, &scene.presentation);
        match &scene.world {
            Some(world) => {
                // Uploaded (or kept) even when covered, so it is ready.
                let frame = self.world_frame(renderer, tree, world);
                list.world = frame.filter(|_| !world.covered).map(|mut f| {
                    f.presentation = scene.presentation;
                    f.filtering = scene.filtering;
                    f.geometry_lod = scene.geometry_lod;
                    if let Some(p) = &mut f.projected {
                        p.lerp = scene.lmodel_lerp;
                    }
                    // The level hands over the identity: no screen effect
                    // exists yet (frame-presentation.md 2.2 step 3).
                    // TODO(#190): the effects' values (screen-effects).
                    f.effects = Some(ScreenEffects::identity());
                    f
                });
            }
            None if self.holds_world() => {}
            None => {
                if self.world.take().is_some() {
                    renderer.clear_details();
                    renderer.clear_billboards();
                    renderer.clear_level_textures();
                }
                self.particle_shaders.clear();
                self.glow_shaders.clear();
            }
        }
        // The quads of the sprites before the lens's (some may have no
        // texture).
        let mut before_lens = 0;
        for (i, sprite) in scene.sprites.iter().enumerate() {
            if scene.lens.as_ref().is_some_and(|l| i == l.drawn) {
                before_lens = list.quads.len();
            }
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
                        .get_or_insert_with(|| {
                            renderer.owning(Owner::Images, || {
                                renderer.create_rgba_texture(1, 1, &[255; 4])
                            })
                        })
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
            quad.corner_colors = sprite
                .corner_colors
                .map(|cs| cs.map(|c| [c.r, c.g, c.b, c.a]));
            list.quads.push(quad);
        }
        if let Some(lens) = scene.lens.as_ref().filter(|_| list.world.is_none()) {
            if lens.drawn >= scene.sprites.len() {
                before_lens = list.quads.len();
            }
            let map = match &lens.texture {
                TextureId::Image { path } => self.image(renderer, tree, path),
                _ => None,
            };
            if let (Some(map), Some((size, _))) = (map, renderer.presentation_target()) {
                list.quads
                    .insert(before_lens, lens_quad(&lens.quad(size), size, &map));
                list.lens = Some(MenuLens {
                    drawn: before_lens,
                    presentation: scene.presentation,
                });
            }
        }
        // Images no longer drawn are released too, e.g. the loading image's
        // textures once the overlay ends (loading-screen.md section 2,
        // step 6).
        let lens = scene.lens.as_ref().map(|l| &l.texture);
        self.images.retain(|path, _| {
            let drawn = |t: &TextureId| matches!(t, TextureId::Image { path: p } if p == path);
            scene.sprites.iter().any(|s| drawn(&s.texture)) || lens.is_some_and(drawn)
        });
        self.videos.retain(|path, _| {
            scene
                .sprites
                .iter()
                .any(|s| matches!(&s.texture, TextureId::Video { path: p, .. } if p == path))
        });
        list
    }
}
