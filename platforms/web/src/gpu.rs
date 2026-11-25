// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Rendering in the engine worker on the `OffscreenCanvas` the page
//! transferred (docs/architecture/rendering.md): WebGPU when the browser
//! gives an adapter, else WebGL 2 (#113), or WebGL 2 where the player chose
//! it (#121). The page reports the canvas's CSS
//! size and devicePixelRatio ([`resize`]); `engine-worker.js` calls
//! [`frame`] once per animation frame. Without either the engine runs on
//! without graphics and [`status`] says why.

use std::cell::RefCell;
use std::sync::Arc;

use serde::Serialize;
use serde_json::json;
use volya_engine::core::vfs::read_ahead::ReadAhead;
use volya_engine::formats::pe::Rgba;
use volya_engine::formats::ui::Resolution;
use volya_engine::level::LevelWorld;
use volya_engine::present::Presenter;
use volya_engine::render::{DrawList, Quad, Rect, Renderer, VideoTexture, wgpu};
use volya_engine_formats::dds::{Dds, Format};
use volya_engine_formats::video::TheoraVideo;
use wasm_bindgen::JsValue;
use web_sys::OffscreenCanvas;

/// What shows where nothing is drawn.
const CLEAR: wgpu::Color = wgpu::Color::BLACK;
/// Most video frames decoded in one animation frame when playback lags.
const MAX_CATCH_UP: u64 = 4;
/// Largest capture side, in pixels.
const MAX_CAPTURE: u32 = 4096;

struct Gpu {
    /// `webgpu` or `webgl2`.
    backend: &'static str,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    queue: wgpu::Queue,
    renderer: Renderer,
    /// What `debug_draw` shows when the game does not run.
    list: DrawList,
    video: Option<Playing>,
    /// The game's textures and videos.
    presenter: Presenter,
    /// Whether the canvas shows something other than the current state.
    dirty: bool,
}

/// A video playing into a texture, looping.
struct Playing {
    /// Decodes from `source`, topped up from the game tree before each
    /// frame (only a window of the file is in memory).
    video: TheoraVideo<ReadAhead>,
    source: ReadAhead,
    texture: VideoTexture,
    fps: f64,
    /// Animation time of frame 0, set on the first animation frame.
    start: Option<f64>,
    /// Frames shown since `start`.
    shown: u64,
}

thread_local! {
    /// Whether the detail objects are drawn (`debug_render`).
    static DETAILS_DRAWN: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };

    static GPU: RefCell<Option<Gpu>> = const { RefCell::new(None) };
    /// The Graphics setting's stored choice, empty when none (#121).
    static CHOICE: RefCell<String> = const { RefCell::new(String::new()) };
    /// Why there is no GPU, once initialisation failed.
    static UNAVAILABLE: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Milliseconds from a frame's submission until the browser reported
    /// the GPU done with it, for frames reported since the last frame.
    static DONE_MS: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) };
    /// The latest frames' GPU times and submitted-to-done times while the
    /// diagnostic log is on, milliseconds, for `level_stats`.
    static RECENT: RefCell<[std::collections::VecDeque<f64>; 2]> = Default::default();
    /// The size the game last drew at and the canvas's, for the diagnostic
    /// log.
    static DRAWN: std::cell::Cell<Option<(Resolution, Resolution)>> =
        const { std::cell::Cell::new(None) };
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Records a lost WebGL context as the engine's end: the WebGL
/// specification fires `webglcontextlost` at the canvas, an
/// `OffscreenCanvas` too (documented, not measured).
fn watch_context_loss(canvas: &OffscreenCanvas) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    let lost = Closure::<dyn FnMut()>::new(|| {
        let message = "webgl2: the WebGL context was lost";
        crate::diagnostics::end(1, "device_lost", message, &[]);
    });
    let _ =
        canvas.add_event_listener_with_callback("webglcontextlost", lost.as_ref().unchecked_ref());
    // For the canvas's lifetime, the engine's.
    lost.forget();
}

/// Keeps the Graphics setting's stored choice (`""` when none), for the
/// diagnostic log's `settings` event.
pub fn set_choice(choice: String) {
    CHOICE.with(|c| *c.borrow_mut() = choice);
}

/// The Graphics setting as stored (`""` when none) and the backend drawing
/// (`""` without graphics).
pub fn graphics() -> (String, &'static str) {
    let backend = GPU.with(|g| g.borrow().as_ref().map_or("", |gpu| gpu.backend));
    (CHOICE.with(|c| c.borrow().clone()), backend)
}

/// Sets up WebGPU, or WebGL 2 where there is no WebGPU or `webgl` asks for
/// it, on `canvas`; on failure records why and returns normally.
pub async fn init(canvas: OffscreenCanvas, webgl: bool) {
    match connect(canvas, webgl).await {
        Ok(gpu) => GPU.with(|g| *g.borrow_mut() = Some(gpu)),
        Err(reason) => UNAVAILABLE.with(|u| *u.borrow_mut() = Some(reason)),
    }
}

async fn connect(canvas: OffscreenCanvas, webgl: bool) -> Result<Gpu, String> {
    // A canvas keeps the first kind of context it gives, so WebGPU's
    // adapter is asked for before any surface: without one, WebGL 2 takes
    // the canvas (the iOS Simulator's WebKit has `navigator.gpu` but gives
    // no adapter, measured).
    let navigator = js_sys::Reflect::get(&js_sys::global(), &"navigator".into())
        .map_err(|_| "no navigator in the engine worker")?;
    let has_gpu = js_sys::Reflect::get(&navigator, &"gpu".into())
        .is_ok_and(|gpu| !gpu.is_undefined() && !gpu.is_null());
    let webgpu = if webgl {
        Err(None)
    } else if has_gpu {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        match instance.request_adapter(&Default::default()).await {
            Ok(_) => Ok(instance),
            Err(_) => Err(Some("no WebGPU adapter")),
        }
    } else {
        Err(Some("no WebGPU"))
    };
    let (instance, backend, why) = match webgpu {
        Ok(instance) => (instance, "webgpu", None::<&str>),
        // Without WebGPU, or WebGL 2 chosen (`why` is then `None`).
        Err(why) => {
            if let Some(why) = why {
                web_sys::console::info_1(&format!("{why}: trying WebGL 2").into());
            }
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::GL,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            (instance, "webgl2", why)
        }
    };
    let (width, height) = (canvas.width().max(1), canvas.height().max(1));
    if backend == "webgl2" {
        watch_context_loss(&canvas);
    }
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas))
        .map_err(|e| format!("no {backend} canvas: {e}"))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .map_err(|_| match (backend, why) {
            ("webgpu", _) => "no WebGPU adapter for the canvas".to_owned(),
            (_, Some(why)) => format!("{why}; no WebGL 2 adapter"),
            (_, None) => "no WebGL 2 adapter".to_owned(),
        })?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            required_features: volya_engine::render::device_features(&adapter),
            // WebGL 2 offers less than WebGPU's defaults: what it has.
            required_limits: if backend == "webgl2" {
                adapter.limits()
            } else {
                wgpu::Limits::default()
            },
            ..Default::default()
        })
        .await
        .map_err(|e| format!("no {backend} device: {e}"))?;
    // Report instead of panicking, which would stop the whole engine.
    device.on_uncaptured_error(Arc::new(move |e| {
        web_sys::console::error_1(&format!("{backend}: {e}").into());
        tracing::error!(backend, error = %e, "GPU error");
    }));
    // WebGPU's `device.lost` (wgpu's GL backend doesn't report a lost
    // context here: `watch_context_loss`).
    device.set_device_lost_callback(move |reason, message| {
        let message = format!("{backend}: {reason:?}: {message}");
        web_sys::console::error_1(&message.clone().into());
        crate::diagnostics::end(1, "device_lost", &message, &[]);
    });
    // Colour values pass through: a non-sRGB format, preferred one first.
    let format = surface
        .get_capabilities(&adapter)
        .formats
        .into_iter()
        .find(|f| !f.is_srgb())
        .ok_or("no non-sRGB canvas format")?;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width,
        height,
        desired_maximum_frame_latency: 2,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        view_formats: vec![],
    };
    surface.configure(&device, &config);
    Ok(Gpu {
        backend,
        surface,
        config,
        renderer: Renderer::new(&device, &queue, format),
        queue,
        list: DrawList::default(),
        video: None,
        presenter: Presenter::new(),
        dirty: true,
    })
}

#[derive(Serialize)]
pub struct Status {
    /// `ready`, `unavailable` (see `message`) or `starting`.
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    /// `webgpu` or `webgl2`.
    #[serde(skip_serializing_if = "Option::is_none")]
    backend: Option<&'static str>,
    /// Whether BC textures upload as stored.
    #[serde(skip_serializing_if = "Option::is_none")]
    bc: Option<bool>,
    /// How BC textures reach the GPU: `bc`, `etc2` (transcoded) or `rgba8`
    /// (decoded).
    #[serde(skip_serializing_if = "Option::is_none")]
    textures: Option<&'static str>,
    /// How the diagnostic log times frames on the GPU: `timestamp-query`
    /// (`gpu.time_ms` and `gpu.done_ms`) or `submitted-work-done`
    /// (`gpu.done_ms` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    timing: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    /// Canvas size in pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<[u32; 2]>,
}

/// The `debug_lose_device` op (tests): destroys the GPU device, as a lost
/// one; drawing stops for the rest of the run.
#[cfg(feature = "test-ops")]
pub fn lose_device() -> Result<(), JsValue> {
    GPU.with(|g| {
        let g = g.borrow();
        let gpu = g.as_ref().ok_or_else(|| error("no GPU"))?;
        gpu.renderer.device().destroy();
        Ok(())
    })
}

pub fn status() -> Status {
    let ready = GPU.with(|g| {
        g.borrow().as_ref().map(|gpu| Status {
            state: "ready",
            message: None,
            backend: Some(gpu.backend),
            textures: Some(gpu.renderer.texture_path().name()),
            bc: Some(
                gpu.renderer
                    .device()
                    .features()
                    .contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
            ),
            timing: Some(if gpu.renderer.can_time_gpu() {
                "timestamp-query"
            } else {
                "submitted-work-done"
            }),
            format: Some(format!("{:?}", gpu.config.format)),
            size: Some([gpu.config.width, gpu.config.height]),
        })
    });
    ready.unwrap_or_else(|| {
        let message = UNAVAILABLE.with(|u| u.borrow().clone());
        Status {
            state: if message.is_some() {
                "unavailable"
            } else {
                "starting"
            },
            message,
            backend: None,
            textures: None,
            bc: None,
            timing: None,
            format: None,
            size: None,
        }
    })
}

/// Uploads about `budget` more bytes of `level`'s world before it shows
/// ([`Presenter::prepare_world`]); whether all of it is.
pub fn prepare_world(level: &Arc<LevelWorld>, budget: u64) -> Result<bool, JsValue> {
    with_gpu(|gpu| {
        crate::game::with_trees(|tree, _| {
            gpu.presenter
                .prepare_world(&mut gpu.renderer, tree, level, budget)
        })?
        .map_err(error)
    })
}

/// Frames whose GPU times `level_stats` averages.
const RECENT_FRAMES: usize = 60;

/// The longest [`submitted_work_done`] waits, milliseconds.
const WORK_DONE_TIMEOUT_MS: f64 = 30_000.0;

/// Resolves once the browser reports the GPU done with all work submitted
/// so far (`onSubmittedWorkDone` on WebGPU; on WebGL 2 wgpu's fences,
/// checked when the frame loop polls the device), or after
/// [`WORK_DONE_TIMEOUT_MS`]; at once without graphics.
pub async fn submitted_work_done() -> Result<(), JsValue> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let done = Arc::new(AtomicBool::new(false));
    let set = done.clone();
    let asked = GPU.with(|g| {
        g.borrow().as_ref().map(|gpu| {
            gpu.queue
                .on_submitted_work_done(move || set.store(true, Ordering::Release))
        })
    });
    if asked.is_none() {
        return Ok(());
    }
    // A lost device may never report: given up on after a while (our
    // choice), the loading going on.
    let start = crate::now();
    while !done.load(Ordering::Acquire) && crate::now() - start < WORK_DONE_TIMEOUT_MS {
        crate::opfs::sleep(2).await?;
    }
    Ok(())
}

/// Draws the level's start view once into a frame of its own, never
/// shown, while its loading waits ([`crate::engine::level_to_prepare_scene`]):
/// the world is made from what was uploaded, and the browser and the GPU
/// do their first-draw work (programs, the world's render bundles) under
/// the loading image (#355, #376). Whether there was a view to draw.
pub fn warm_up() -> Result<bool, JsValue> {
    with_gpu(|gpu| {
        let max = gpu.renderer.device().limits().max_texture_dimension_2d;
        let Some(mode) = crate::engine::render_size() else {
            return Ok(false);
        };
        let size = Resolution::new(mode.width.clamp(1, max), mode.height.clamp(1, max));
        let Some(scene) = crate::engine::level_to_prepare_scene(size) else {
            return Ok(false);
        };
        // Kept through the loading screen's frames until the level shows.
        let level = scene.world.as_ref().map(|w| w.world.clone());
        gpu.presenter.hold_world(level);
        let list = tracing::info_span!("render.scene").in_scope(|| {
            crate::game::with_trees(|tree, _| {
                gpu.presenter.draw_list(&mut gpu.renderer, tree, &scene, 0)
            })
        })?;
        for e in gpu.presenter.take_errors() {
            web_sys::console::error_1(&format!("drawing: {e}").into());
        }
        let target = gpu
            .renderer
            .device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("warm-up"),
                size: wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: gpu.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
        tracing::info_span!("render.encode")
            .in_scope(|| gpu.renderer.render(&target, &list, CLEAR));
        target.destroy();
        Ok(true)
    })
}

/// Runs `f` with the renderer, `None` without graphics.
pub fn with_renderer<T>(f: impl FnOnce(&Renderer) -> T) -> Option<T> {
    GPU.with(|g| g.borrow().as_ref().map(|gpu| f(&gpu.renderer)))
}

fn with_gpu<T>(f: impl FnOnce(&mut Gpu) -> Result<T, JsValue>) -> Result<T, JsValue> {
    GPU.with(|g| match g.borrow_mut().as_mut() {
        Some(gpu) => f(gpu),
        None => {
            let reason = UNAVAILABLE.with(|u| u.borrow().clone());
            Err(error(format!(
                "graphics unavailable: {}",
                reason.as_deref().unwrap_or("starting")
            )))
        }
    })
}

/// Sizes the canvas to `width` × `height` CSS pixels at `dpr` device pixels
/// per CSS pixel.
pub fn resize(width: f64, height: f64, dpr: f64) {
    let _ = with_gpu(|gpu| {
        let max = gpu.renderer.device().limits().max_texture_dimension_2d;
        let pixels = |css: f64| ((css * dpr).round() as u32).clamp(1, max);
        let (width, height) = (pixels(width), pixels(height));
        if (width, height) != (gpu.config.width, gpu.config.height) {
            gpu.config.width = width;
            gpu.config.height = height;
            gpu.surface.configure(gpu.renderer.device(), &gpu.config);
            gpu.dirty = true;
        }
        Ok(())
    });
}

/// The canvas size in pixels, `None` without graphics.
pub fn resolution() -> Option<Resolution> {
    GPU.with(|g| {
        g.borrow()
            .as_ref()
            .map(|gpu| Resolution::new(gpu.config.width, gpu.config.height))
    })
}

/// GPU memory of the static world shown, if any, with its objects'
/// share.
pub fn world_stats() -> serde_json::Value {
    GPU.with(|g| {
        let g = g.borrow();
        let world = g.as_ref().and_then(|gpu| gpu.presenter.world());
        let [time, done] = RECENT.with(|r| {
            r.borrow()
                .clone()
                .map(|q| (!q.is_empty()).then(|| q.iter().sum::<f64>() / q.len() as f64))
        });
        json!({
            "gpu_time_ms": time,
            "gpu_done_ms": done,
            "gpu_world_bytes": world.map(|w| w.gpu_bytes()),
            "gpu_texture_bytes": world.map(|w| w.texture_bytes()),
            "skipped_draws": world.map(|w| w.skipped()),
            "world_draws": world.map(|w| w.draw_count()),
            "object_draws": world.map(|w| w.object_draw_count()),
            "gpu_object_bytes": world.map(|w| w.object_gpu_bytes()),
            "gpu_object_texture_bytes": world.map(|w| w.object_texture_bytes()),
        })
    })
}

/// Sets the splash picture the game shows while it loads.
pub fn set_splash(image: &Rgba) {
    let _ = with_gpu(|gpu| {
        gpu.presenter.set_splash(&gpu.renderer, image);
        Ok(())
    });
}

/// Whether the detail objects are drawn (tests, #415).
#[cfg(feature = "test-ops")]
pub fn set_details_drawn(on: bool) {
    DETAILS_DRAWN.set(on);
}

/// One animation frame at `time` (milliseconds): advances video and
/// presents if anything changed.
pub fn frame(time: f64) {
    let _ = with_gpu(|gpu| {
        // wgpu's GL backend runs buffer-mapping callbacks (captures) and
        // frees finished uploads only when polled; the browser's WebGPU
        // does both on its own (this is then a no-op).
        let _ = tracing::info_span!("render.poll")
            .in_scope(|| gpu.renderer.device().poll(wgpu::PollType::Poll));
        // A world drawn ahead of its level (`warm_up`) is held only while
        // the loading waits for it.
        if crate::engine::world_to_prepare().is_none() {
            gpu.presenter.hold_world(None);
        }
        let canvas = Resolution::new(gpu.config.width, gpu.config.height);
        // The game draws at its video mode (#49), within the GPU's limit.
        let max = gpu.renderer.device().limits().max_texture_dimension_2d;
        let resolution = crate::engine::render_size().map_or(canvas, |r| {
            Resolution::new(r.width.clamp(1, max), r.height.clamp(1, max))
        });
        // The game's screen, redrawn every frame while it runs (videos and
        // the cursor animate); otherwise what `debug_draw` set.
        let list = tracing::info_span!("render.scene").in_scope(|| {
            crate::engine::draw_list(&mut gpu.renderer, &mut gpu.presenter, resolution)
        });
        match list {
            Some(mut list) => {
                if !DETAILS_DRAWN.get()
                    && let Some(world) = &mut list.world
                {
                    world.details = None;
                }
                if DRAWN.get() != Some((resolution, canvas)) {
                    DRAWN.set(Some((resolution, canvas)));
                    tracing::info!(
                        width = resolution.width,
                        height = resolution.height,
                        canvas_width = canvas.width,
                        canvas_height = canvas.height,
                        "video mode"
                    );
                }
                gpu.present_at(&list, (resolution.width, resolution.height));
                // Once the game stops, the debug list is drawn over it.
                gpu.dirty = true;
            }
            None => {
                gpu.advance_video(time);
                if gpu.dirty {
                    let list = std::mem::take(&mut gpu.list);
                    gpu.present(&list);
                    gpu.list = list;
                }
            }
        }
        gpu.gpu_times();
        Ok(())
    });
}

impl Gpu {
    /// The diagnostic log's GPU times (#355), while it is on: each frame's
    /// GPU time by timestamp queries where the device has them
    /// (`gpu.time_ms`), and everywhere the time from a frame's submission
    /// until the browser reports the GPU done with all work submitted so
    /// far (`gpu.done_ms`, `onSubmittedWorkDone`; on WebGL 2 wgpu checks its
    /// fences when polled, once a frame, so it is then rounded up to
    /// frames).
    fn gpu_times(&mut self) {
        let on = crate::diagnostics::on();
        self.renderer.set_gpu_timing(on);
        if !on {
            return;
        }
        let keep = |i: usize, ms: f64| {
            RECENT.with(|r| {
                let q = &mut r.borrow_mut()[i];
                if q.len() == RECENT_FRAMES {
                    q.pop_front();
                }
                q.push_back(ms);
            });
        };
        for ms in self.renderer.take_gpu_times() {
            volya_engine::core::diagnostics::metric("gpu.time_ms", ms);
            keep(0, ms);
        }
        for ms in DONE_MS.with(|d| std::mem::take(&mut *d.borrow_mut())) {
            volya_engine::core::diagnostics::metric("gpu.done_ms", ms);
            keep(1, ms);
        }
    }

    /// Asks to be told when the GPU is done with the work submitted so far
    /// (`gpu.done_ms`), while the diagnostic log is on.
    fn watch_done(&self) {
        if !crate::diagnostics::on() {
            return;
        }
        let submitted = crate::now();
        self.queue.on_submitted_work_done(move || {
            let ms = crate::now() - submitted;
            DONE_MS.with(|d| d.borrow_mut().push(ms));
        });
    }

    fn present(&mut self, list: &DrawList) {
        self.present_at(list, (self.config.width, self.config.height));
    }

    /// Presents `list` drawn at `size`, scaled to fit the canvas.
    fn present_at(&mut self, list: &DrawList, size: (u32, u32)) {
        let current =
            tracing::info_span!("render.acquire").in_scope(|| self.surface.get_current_texture());
        let texture = match current {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                // Stays dirty: drawn on the next frame.
                self.surface.configure(self.renderer.device(), &self.config);
                return;
            }
            _ => return,
        };
        tracing::info_span!("render.encode").in_scope(|| {
            self.renderer
                .render_scaled(&texture.texture, list, CLEAR, size);
        });
        self.watch_done();
        tracing::info_span!("render.present").in_scope(|| self.queue.present(texture));
        self.dirty = false;
    }

    fn advance_video(&mut self, now: f64) {
        let Some(playing) = &mut self.video else {
            return;
        };
        let frame_ms = 1000.0 / playing.fps;
        let start = *playing.start.get_or_insert(now);
        let mut due = ((now - start) / frame_ms) as u64 + 1;
        if due > playing.shown + MAX_CATCH_UP {
            // Far behind (e.g. a hidden page): slow down instead of skipping.
            playing.start = Some(now - playing.shown as f64 * frame_ms);
            due = playing.shown + 1;
        }
        let mut failed = None;
        while playing.shown < due {
            let topped = crate::game::with_trees(|tree, _| playing.source.top_up(tree));
            if let Err(e) = topped
                .map_err(|e| std::io::Error::other(format!("{e:?}")))
                .and_then(|r| r)
            {
                failed = Some(e);
                break;
            }
            match playing.video.next_frame() {
                Ok(Some(frame)) => {
                    self.renderer.update_video(&mut playing.texture, &frame);
                    playing.shown += 1;
                    self.dirty = true;
                }
                // Loops: the next pass starts where this one ended.
                Ok(None) if playing.shown > 0 => {
                    playing.start = Some(start + playing.shown as f64 * frame_ms);
                    playing.shown = 0;
                    failed = playing.video.rewind().err();
                    break;
                }
                Ok(None) => {
                    failed = Some(std::io::Error::other("no frames"));
                    break;
                }
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        if let Some(e) = failed {
            web_sys::console::error_1(&format!("video: {e}").into());
            self.video = None;
        }
    }
}

/// Debugging: draws the texture (`.dds`) or video (`.ogm`) at `path` in the
/// game tree over the whole screen, or clears the screen when `path` is
/// absent or empty. Returns what was loaded.
pub fn debug_draw(path: Option<&str>) -> Result<serde_json::Value, JsValue> {
    let Some(path) = path.filter(|p| !p.is_empty()) else {
        return with_gpu(|gpu| {
            gpu.list.quads.clear();
            gpu.video = None;
            gpu.dirty = true;
            Ok(serde_json::Value::Null)
        });
    };
    // Fails early, before reading the file, without a GPU.
    with_gpu(|_| Ok(()))?;
    let is_video = path.to_ascii_lowercase().ends_with(".ogm");
    // A video streams from the file; a texture is read whole.
    let (bytes, source) = if is_video {
        let source =
            crate::game::with_trees(|tree, _| ReadAhead::open(tree, path))?.map_err(error)?;
        (Vec::new(), Some(source))
    } else {
        (crate::game::read(path)?, None)
    };
    with_gpu(|gpu| {
        let info;
        let quad = if let Some(source) = source {
            let video = TheoraVideo::open(source.clone()).map_err(error)?;
            let info_ = video.info().clone();
            let texture = gpu.renderer.create_video_texture(info_.width, info_.height);
            let mut quad = Quad::new(Rect::SCREEN, texture.texture());
            quad.source = texture.frame_rect();
            let fps = f64::from(info_.fps_numerator) / f64::from(info_.fps_denominator.max(1));
            info = json!({
                "kind": "video",
                "width": info_.width,
                "height": info_.height,
                "fps": fps,
                "texture": texture.texture().size(),
            });
            gpu.video = Some(Playing {
                video,
                source,
                texture,
                fps,
                start: None,
                shown: 0,
            });
            quad
        } else {
            let dds = Dds::parse(&bytes).map_err(error)?;
            let texture = gpu.renderer.create_texture(&dds).map_err(error)?;
            let format = match dds.format {
                Format::Masked(m) => format!("{}-bit", m.bits),
                f => format!("{f:?}"),
            };
            info = json!({
                "kind": "texture",
                "width": dds.width,
                "height": dds.height,
                "format": format,
                "mips": dds.mip_levels,
                "faces": dds.faces,
            });
            gpu.video = None;
            Quad::new(Rect::SCREEN, &texture)
        };
        gpu.list.quads = vec![quad];
        gpu.dirty = true;
        Ok(info)
    })
}

#[derive(Serialize)]
pub struct Capture {
    width: u32,
    height: u32,
    /// RGBA, 8 bits per channel, rows top to bottom.
    rgba: Vec<u8>,
}

/// Debugging and tests: renders the current frame offscreen at
/// `width` × `height` and reads it back (docs/testing.md): the game's screen
/// built for that size while the game runs, else the debug draw list.
pub async fn debug_capture(width: u32, height: u32) -> Result<Capture, JsValue> {
    if !(1..=MAX_CAPTURE).contains(&width) || !(1..=MAX_CAPTURE).contains(&height) {
        return Err(error(format!("capture size {width}×{height}")));
    }
    let pixels = with_gpu(|gpu| {
        // The game's screen as it would be at this size, else the debug list.
        let resolution = Resolution::new(width, height);
        let list = crate::engine::draw_list(&mut gpu.renderer, &mut gpu.presenter, resolution)
            .unwrap_or_else(|| gpu.list.clone());
        Ok(gpu.renderer.capture(&list, width, height, CLEAR))
    })?;
    let rgba = pixels.await.map_err(error)?;
    Ok(Capture {
        width,
        height,
        rgba,
    })
}

/// The `debug_capture_scene` op (tests): `scene`, as the `menu_scene` op
/// gives it, drawn at its resolution by a presenter of its own at engine
/// clock `clock_ms` and read back, so the same frame can be drawn by each
/// backend (the title menu on WebGPU and on WebGL 2) and compared: its
/// videos decoded from their start up to the frame due then, whatever the
/// engine's clock.
#[cfg(feature = "test-ops")]
pub async fn debug_capture_scene(args: &serde_json::Value) -> Result<Capture, JsValue> {
    use volya_engine::formats::ui::Color;
    use volya_engine::scene::{Scene, Sprite, TextureId};

    #[derive(serde::Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum JsonTexture {
        Image {
            path: String,
        },
        Video {
            path: String,
            start_ms: u64,
            looping: bool,
        },
        Splash,
        White,
    }
    #[derive(serde::Deserialize)]
    struct JsonSprite {
        layer: String,
        texture: JsonTexture,
        corners: [[f32; 2]; 4],
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [u8; 4],
        corner_colors: Option<[[u8; 4]; 4]>,
    }
    #[derive(serde::Deserialize)]
    struct JsonScene {
        resolution: [u32; 2],
        sprites: Vec<JsonSprite>,
    }
    let argb = |[a, r, g, b]: [u8; 4]| Color { a, r, g, b };
    let json: JsonScene = serde_json::from_value(args.get("scene").cloned().unwrap_or_default())
        .map_err(|e| error(format!("scene: {e}")))?;
    let [width, height] = json.resolution;
    if !(1..=MAX_CAPTURE).contains(&width) || !(1..=MAX_CAPTURE).contains(&height) {
        return Err(error(format!("capture size {width}×{height}")));
    }
    let mut scene = Scene::new(Resolution::new(width, height));
    scene.sprites = json
        .sprites
        .into_iter()
        .map(|s| Sprite {
            layer: s.layer,
            texture: match s.texture {
                JsonTexture::Image { path } => TextureId::Image { path },
                JsonTexture::Video {
                    path,
                    start_ms,
                    looping,
                } => TextureId::Video {
                    path,
                    start_ms,
                    looping,
                },
                JsonTexture::Splash => TextureId::Splash,
                JsonTexture::White => TextureId::White,
            },
            corners: s.corners,
            uv_min: s.uv_min,
            uv_max: s.uv_max,
            color: argb(s.color),
            corner_colors: s.corner_colors.map(|cs| cs.map(argb)),
        })
        .collect();
    let clock = args.get("clock_ms").and_then(|v| v.as_u64()).unwrap_or(0);
    let pixels = with_gpu(|gpu| {
        let mut presenter = Presenter::new();
        let list = crate::game::with_trees(|tree, _| {
            // Each call decodes a few video frames (at most 4); enough calls
            // for 60 frames a second reach the frame due at `clock`.
            let calls = clock * 60 / 4000 + 2;
            let mut list = DrawList::default();
            for _ in 0..calls {
                list = presenter.draw_list(&mut gpu.renderer, tree, &scene, clock);
            }
            list
        })?;
        let errors = presenter.take_errors();
        if !errors.is_empty() {
            return Err(error(errors.join("; ")));
        }
        Ok(gpu.renderer.capture(&list, width, height, CLEAR))
    })?;
    let rgba = pixels.await.map_err(error)?;
    Ok(Capture {
        width,
        height,
        rgba,
    })
}

/// The `debug_draw_particles` op (tests): sets the debug draw list to a
/// world without geometry holding particle batches of `case`, the scenes
/// of the native `particles_queue_blend_cull_and_distort` render test
/// (docs/behavior/particle-drawing.md 4.5 – 7), so `debug_capture` reads
/// them back through this browser's backend (WebGL 2 where there is no
/// WebGPU): `blend` (two `BLEND` sprites back to front, 64 × 64), `set`
/// (the `SET` alpha test and culling, 64 × 64), `plain` (a `SET` sprite
/// over the top left quarter, 256 × 256) and `distort` (`plain` with a
/// distortion sprite over the screen, 256 × 256).
#[cfg(feature = "test-ops")]
pub fn debug_draw_particles(case: &str) -> Result<serde_json::Value, JsValue> {
    use volya_engine::render::particles::{
        ParticleBatch, ParticleBlend, ParticleCull, ParticlePass, ParticleProgram, ParticleQueue,
        ParticleVertex,
    };
    use volya_engine::render::world::WorldLight;
    use volya_engine::render::{Texture, WorldFrame};

    /// A facing quad of particle-drawing.md 4.4 at depth `z`.
    fn sprite(x0: f32, x1: f32, y0: f32, y1: f32, z: f32, argb: u32) -> Vec<ParticleVertex> {
        [
            ([x0, y0, z], [0.0, 1.0]),
            ([x0, y1, z], [0.0, 0.0]),
            ([x1, y0, z], [1.0, 1.0]),
            ([x1, y1, z], [1.0, 0.0]),
        ]
        .into_iter()
        .map(|(position, uv)| ParticleVertex {
            position,
            colour: argb,
            uv,
        })
        .collect()
    }
    let pass = |program, blend, base: &Texture| ParticlePass {
        program,
        blend,
        depth_write: program == ParticleProgram::Set,
        clamp: true,
        base: Some(base.clone()),
        distort: None,
    };
    let batch = |pass, queue, key, vertices| ParticleBatch {
        pass,
        queue,
        cull: ParticleCull::None,
        key,
        vertices,
    };
    with_gpu(|gpu| {
        let r = &mut gpu.renderer;
        let white = r.create_rgba_texture(1, 1, &[255; 4]);
        let world = (r.create_world(&Default::default(), &[], &[], None)).map_err(error)?;
        let blend = |key, argb| {
            batch(
                pass(ParticleProgram::Blended, ParticleBlend::Alpha, &white),
                ParticleQueue::Strict,
                key,
                sprite(-0.25, 0.25, -0.25, 0.25, 0.5, argb),
            )
        };
        let set = |x0, x1, argb| {
            batch(
                pass(ParticleProgram::Set, ParticleBlend::Off, &white),
                ParticleQueue::Early,
                1.0,
                sprite(x0, x1, -0.25, 0.25, 0.5, argb),
            )
        };
        let left = batch(
            pass(ParticleProgram::Set, ParticleBlend::Off, &white),
            ParticleQueue::Early,
            1.0,
            sprite(-2.0, 0.0, 0.0, 2.0, 0.5, 0xFFFF_FFFF),
        );
        let particles = match case {
            // Nearer first in the queue; drawn farther (larger key) first.
            "blend" => vec![blend(1.0, 0x8000_FF00), blend(2.0, 0x80FF_0000)],
            "set" => {
                let mut culled = blend(1.0, 0xFFFF_FFFF);
                culled.cull = ParticleCull::Clockwise;
                vec![
                    set(-0.25, 0.0, 0xC7FF_FFFF),
                    set(0.0, 0.25, 0xCAFF_FFFF),
                    culled,
                ]
            }
            "plain" => vec![left],
            "distort" => {
                let red = r.create_rgba_texture(1, 1, &[255, 127, 0, 255]);
                let mut distort = pass(ParticleProgram::Distort, ParticleBlend::Alpha, &white);
                distort.distort = Some(red);
                vec![
                    left,
                    batch(
                        distort,
                        ParticleQueue::Distortion,
                        1.0,
                        sprite(-2.0, 2.0, -2.0, 2.0, 0.4, 0xFFFF_FFFF),
                    ),
                ]
            }
            other => return Err(error(format!("unknown case {other}"))),
        };
        // Looking along +z from the origin, near 0.1, far 100, square.
        let (n, z) = (0.1, 100.0);
        let mut identity = [[0.0; 4]; 4];
        (0..4).for_each(|i| identity[i][i] = 1.0);
        let mut projection = identity;
        projection[2] = [0.0, 0.0, z / (z - n), 1.0];
        projection[3] = [0.0, 0.0, -n * z / (z - n), 0.0];
        gpu.list = DrawList {
            world: Some(WorldFrame {
                world,
                view: identity,
                projection,
                eye: [0.0; 3],
                forward: [0.0, 0.0, 1.0],
                fov_y_degrees: 90.0,
                light: WorldLight {
                    ambient: [0.0; 3],
                    hemi: [0.0; 3],
                    sun_color: [0.0; 3],
                    sun_dir: [0.0, -1.0, 0.0],
                    fog_color: [0.0; 3],
                    fog_near: 10.0,
                    fog_far: 90.0,
                },
                time: 0.0,
                sky: None,
                clear: [0.0; 3],
                particles,
                lights: Vec::new(),
                presentation: Default::default(),
                effects: None,
                details: None,
            }),
            ..Default::default()
        };
        gpu.video = None;
        gpu.dirty = true;
        Ok(json!({ "compat": gpu.renderer.compat() }))
    })
}

/// The `debug_draw_lights` op (tests): sets the debug draw list to a black
/// vertex-lit triangle at depth 1 facing up, the scenes of the native
/// `light_passes_add_to_the_image` and `glows_add_after_the_world` render
/// tests (docs/behavior/dynamic-lights.md 6, glows.md 6), 16 × 16, read
/// back with `debug_capture` through this browser's backend: `none` (no
/// light), `point` and `spot` (a light far above, colour (¼, ⅛, 0), white
/// attenuation textures: 2 · c = (128, 64, 0) at the centre), `glow` (a
/// glow sprite in front with b = 128: (64, 64, 64)) and `hidden` (the glow
/// behind the triangle: black).
#[cfg(feature = "test-ops")]
pub fn debug_draw_lights(case: &str) -> Result<serde_json::Value, JsValue> {
    use volya_engine::render::particles::{
        ParticleBatch, ParticleBlend, ParticleCull, ParticlePass, ParticleProgram, ParticleQueue,
        ParticleVertex,
    };
    use volya_engine::render::world::{
        Blend, LightElement, LightPass, LightProgram, LightTextures, Program, Surface, WorldLight,
    };
    use volya_engine::render::{WorldDraw, WorldFrame};
    use volya_engine_formats::level_geom::{Layout, LevelGeom, VertexBuffer};

    let mut identity = [[0.0; 4]; 4];
    (0..4).for_each(|i| identity[i][i] = 1.0);
    // One screen-covering triangle at depth 1, normal (0, 1, 0), black.
    let mut data = Vec::new();
    for p in [[-3.0f32, -3.0, 1.0], [-3.0, 9.0, 1.0], [9.0, -3.0, 1.0]] {
        data.extend(p.iter().flat_map(|c| c.to_le_bytes()));
        data.extend([128, 255, 128, 255, 128, 128, 255, 0, 255, 128, 128, 0]);
        data.extend([0; 8]);
    }
    let geom = LevelGeom {
        header: None,
        vertex_buffers: vec![VertexBuffer {
            elements: Layout::VertexLit.elements(),
            layout: Some(Layout::VertexLit),
            stride: 32,
            vertex_count: 3,
            data,
        }],
        index_buffers: vec![vec![0, 1, 2]],
        progressive_meshes: Vec::new(),
    };
    // A light pass whose frustum holds everything (clip (0, 0, ½, 1)).
    let mut everywhere = [[0.0; 4]; 4];
    everywhere[3] = [0.0, 0.0, 0.5, 1.0];
    let light = |spot: bool| LightPass {
        spot,
        position: [0.0, 1000.0, 1.0, if spot { 0.125 } else { 0.0625 }],
        colour: [0.25, 0.125, 0.0, 1.0],
        xform: everywhere,
        frustum: everywhere,
    };
    let glow = |white: &volya_engine::render::Texture, z: f32| ParticleBatch {
        pass: ParticlePass {
            program: ParticleProgram::Glow,
            blend: ParticleBlend::AlphaAdd,
            depth_write: false,
            clamp: true,
            base: Some(white.clone()),
            distort: None,
        },
        queue: ParticleQueue::Glow,
        cull: ParticleCull::CounterClockwise,
        key: 0.0,
        vertices: [
            ([-0.25, -0.25, z], [0.0, 1.0]),
            ([-0.25, 0.25, z], [0.0, 0.0]),
            ([0.25, -0.25, z], [1.0, 1.0]),
            ([0.25, 0.25, z], [1.0, 0.0]),
        ]
        .into_iter()
        .map(|(position, uv)| ParticleVertex {
            position,
            colour: 0x8080_8080,
            uv,
        })
        .collect(),
    };
    with_gpu(|gpu| {
        let r = &mut gpu.renderer;
        let white = r.create_rgba_texture(1, 1, &[255; 4]);
        r.set_light_textures(LightTextures {
            att: white.clone(),
            attclip: white.clone(),
            attpoint: white.clone(),
        });
        let surface = Surface {
            program: Program::VertexLit,
            blend: Blend::Off,
            depth_write: true,
            fog: false,
            alpha_ref: None,
            detail_scale: None,
            priority: 1,
            strict: false,
            object_lod: true,
            textures: [Some(white.clone()), None, None, None],
            env: None,
            light: Some(LightElement {
                program: LightProgram::VertexLit,
                alpha_ref: 0,
                priority: 1,
            }),
        };
        let draw = WorldDraw {
            surface: 0,
            vertex_buffer: 0,
            first_vertex: 0,
            index_buffer: 0,
            first_index: 0,
            index_count: 3,
            transform: identity,
            sphere: [0.0, 0.0, 1.0, 10.0],
            tree: None,
            object: None,
        };
        let world = r
            .create_world(&geom, &[surface], &[draw], None)
            .map_err(error)?;
        let (lights, particles) = match case {
            "none" => (Vec::new(), Vec::new()),
            "point" => (vec![light(false)], Vec::new()),
            "spot" => (vec![light(true)], Vec::new()),
            "glow" => (Vec::new(), vec![glow(&white, 0.5)]),
            "hidden" => (Vec::new(), vec![glow(&white, 2.0)]),
            other => return Err(error(format!("unknown case {other}"))),
        };
        // Looking along +z from the origin, near 0.1, far 100, square.
        let (n, z) = (0.1, 100.0);
        let mut projection = identity;
        projection[2] = [0.0, 0.0, z / (z - n), 1.0];
        projection[3] = [0.0, 0.0, -n * z / (z - n), 0.0];
        gpu.list = DrawList {
            world: Some(WorldFrame {
                world,
                view: identity,
                projection,
                eye: [0.0; 3],
                forward: [0.0, 0.0, 1.0],
                fov_y_degrees: 90.0,
                light: WorldLight {
                    ambient: [0.0; 3],
                    hemi: [0.0; 3],
                    sun_color: [0.0; 3],
                    sun_dir: [0.0, -1.0, 0.0],
                    fog_color: [0.0; 3],
                    fog_near: 10.0,
                    fog_far: 90.0,
                },
                time: 0.0,
                sky: None,
                clear: [0.0; 3],
                particles,
                lights,
                presentation: Default::default(),
                effects: None,
                details: None,
            }),
            ..Default::default()
        };
        gpu.video = None;
        gpu.dirty = true;
        Ok(json!({ "compat": gpu.renderer.compat() }))
    })
}
