// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Rendering in the engine worker on the `OffscreenCanvas` the page
//! transferred (docs/architecture.md, Threading): WebGPU when the browser
//! gives an adapter, else WebGL 2 (#113). The page reports the canvas's CSS
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
    static GPU: RefCell<Option<Gpu>> = const { RefCell::new(None) };
    /// Why there is no GPU, once initialisation failed.
    static UNAVAILABLE: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Sets up WebGPU on `canvas`; on failure records why and returns normally.
pub async fn init(canvas: OffscreenCanvas) {
    match connect(canvas).await {
        Ok(gpu) => GPU.with(|g| *g.borrow_mut() = Some(gpu)),
        Err(reason) => UNAVAILABLE.with(|u| *u.borrow_mut() = Some(reason)),
    }
}

async fn connect(canvas: OffscreenCanvas) -> Result<Gpu, String> {
    // A canvas keeps the first kind of context it gives, so WebGPU's
    // adapter is asked for before any surface: without one, WebGL 2 takes
    // the canvas (the iOS Simulator's WebKit has `navigator.gpu` but gives
    // no adapter, measured).
    let navigator = js_sys::Reflect::get(&js_sys::global(), &"navigator".into())
        .map_err(|_| "no navigator in the engine worker")?;
    let has_gpu = js_sys::Reflect::get(&navigator, &"gpu".into())
        .is_ok_and(|gpu| !gpu.is_undefined() && !gpu.is_null());
    let webgpu = if has_gpu {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        match instance.request_adapter(&Default::default()).await {
            Ok(_) => Ok(instance),
            Err(_) => Err("no WebGPU adapter"),
        }
    } else {
        Err("no WebGPU")
    };
    let (instance, backend, why) = match webgpu {
        Ok(instance) => (instance, "webgpu", None::<&str>),
        // The build for WebGPU: no other way to draw.
        #[cfg(not(feature = "webgl"))]
        Err(why) => return Err(why.to_owned()),
        #[cfg(feature = "webgl")]
        Err(why) => {
            web_sys::console::info_1(&format!("{why}: trying WebGL 2").into());
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::GL,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            (instance, "webgl2", Some(why))
        }
    };
    let (width, height) = (canvas.width().max(1), canvas.height().max(1));
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas))
        .map_err(|e| format!("no {backend} canvas: {e}"))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .map_err(|_| match why {
            Some(why) => format!("{why}; no WebGL 2 adapter"),
            None => "no WebGPU adapter for the canvas".to_owned(),
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
    }));
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
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    /// Canvas size in pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<[u32; 2]>,
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
        json!({
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

/// One animation frame at `time` (milliseconds): advances video and
/// presents if anything changed.
pub fn frame(time: f64) {
    let _ = with_gpu(|gpu| {
        // wgpu's GL backend runs buffer-mapping callbacks (captures) and
        // frees finished uploads only when polled; the browser's WebGPU
        // does both on its own (this is then a no-op).
        let _ = gpu.renderer.device().poll(wgpu::PollType::Poll);
        let canvas = Resolution::new(gpu.config.width, gpu.config.height);
        // The game draws at its video mode (#49), within the GPU's limit.
        let max = gpu.renderer.device().limits().max_texture_dimension_2d;
        let resolution = crate::engine::render_size().map_or(canvas, |r| {
            Resolution::new(r.width.clamp(1, max), r.height.clamp(1, max))
        });
        // The game's screen, redrawn every frame while it runs (videos and
        // the cursor animate); otherwise what `debug_draw` set.
        match crate::engine::draw_list(&mut gpu.renderer, &mut gpu.presenter, resolution) {
            Some(list) => {
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
        Ok(())
    });
}

impl Gpu {
    fn present(&mut self, list: &DrawList) {
        self.present_at(list, (self.config.width, self.config.height));
    }

    /// Presents `list` drawn at `size`, scaled to fit the canvas.
    fn present_at(&mut self, list: &DrawList, size: (u32, u32)) {
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                // Stays dirty: drawn on the next frame.
                self.surface.configure(self.renderer.device(), &self.config);
                return;
            }
            _ => return,
        };
        self.renderer
            .render_scaled(&texture.texture, list, CLEAR, size);
        self.queue.present(texture);
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
