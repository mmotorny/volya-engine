// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The renderer's device, by backend while wgpu is replaced by direct
//! WebGL 2 (docs/architecture/rendering.md, Design: direct WebGL 2): the
//! context, where a frame goes, and fences.

#[cfg(not(feature = "gl"))]
pub(super) use wgpu_device::*;

#[cfg(feature = "gl")]
pub(super) use gl_device::*;

/// The context's renderer: `UNMASKED_RENDERER_WEBGL` where the browser
/// exposes `WEBGL_debug_renderer_info`, else `RENDERER` (as wgpu-hal names
/// its adapter, read in its source).
#[cfg(feature = "gl")]
fn renderer_name(gl: &web_sys::WebGl2RenderingContext) -> String {
    use web_sys::WebGl2RenderingContext as C;
    // UNMASKED_RENDERER_WEBGL (WEBGL_debug_renderer_info).
    let name = match gl.get_extension("WEBGL_debug_renderer_info") {
        Ok(Some(_)) => 0x9246,
        _ => C::RENDERER,
    };
    gl.get_parameter(name)
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

#[cfg(feature = "gl")]
mod gl_device {
    use std::cell::RefCell;
    use std::rc::Rc;

    use volya_engine::render::gl::{Fence, Gl};
    use volya_engine::render::{Color, Destination, DrawList, Renderer};
    use web_sys::OffscreenCanvas;

    /// What a frame is drawn into: the canvas, at its size then.
    pub(in crate::gpu) struct Frame((u32, u32));

    /// A callback and the fence it waits on.
    type Waiting = (Fence, Box<dyn FnOnce()>);

    /// The canvas's WebGL 2 context and the work waited on.
    pub(in crate::gpu) struct Device {
        canvas: OffscreenCanvas,
        gl: Rc<Gl>,
        /// Callbacks for when the GPU is done with the work before their
        /// fences, oldest first.
        waiting: RefCell<Vec<Waiting>>,
        /// Callbacks whose fence the next [`Device::poll`] makes.
        deferred: RefCell<Vec<Box<dyn FnOnce()>>>,
    }

    impl Device {
        /// The device on `canvas`'s context `context` (made by the platform),
        /// its renderer and the context's renderer name.
        pub(in crate::gpu) async fn connect(
            canvas: OffscreenCanvas,
            context: Option<web_sys::WebGl2RenderingContext>,
        ) -> Result<(Self, Renderer, String), String> {
            let context = context.ok_or("no WebGL 2 canvas")?;
            let name = super::renderer_name(&context);
            let gl = Rc::new(Gl::new(context));
            let renderer = Renderer::new(gl.clone());
            let device = Self {
                canvas,
                gl,
                waiting: RefCell::default(),
                deferred: RefCell::default(),
            };
            Ok((device, renderer, name))
        }

        pub(in crate::gpu) fn max_texture_size(&self) -> u32 {
            self.gl.max_texture_size()
        }

        /// The canvas's size in pixels.
        pub(in crate::gpu) fn size(&self) -> (u32, u32) {
            (self.canvas.width().max(1), self.canvas.height().max(1))
        }

        pub(in crate::gpu) fn resize(&mut self, width: u32, height: u32) {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
        }

        /// The format frames are shown in, as wgpu names it.
        pub(in crate::gpu) fn format_name(&self) -> String {
            "Rgba8Unorm".into()
        }

        /// Runs the callbacks whose work the GPU is done with, then fences
        /// the deferred ones.
        pub(in crate::gpu) fn poll(&self) {
            loop {
                let done = {
                    let mut w = self.waiting.borrow_mut();
                    match w.first() {
                        Some((fence, _)) if fence.done() => Some(w.remove(0).1),
                        _ => None,
                    }
                };
                match done {
                    Some(f) => f(),
                    None => break,
                }
            }
            let deferred = std::mem::take(&mut *self.deferred.borrow_mut());
            if !deferred.is_empty() {
                let fence = self.gl.fence();
                self.waiting.borrow_mut().push((
                    fence,
                    Box::new(move || deferred.into_iter().for_each(|f| f())),
                ));
            }
        }

        /// Calls `f` once the GPU is done with the work issued so far (a
        /// fence, checked by [`Device::poll`]).
        pub(in crate::gpu) fn on_done(&self, f: impl FnOnce() + 'static) {
            let fence = self.gl.fence();
            self.waiting.borrow_mut().push((fence, Box::new(f)));
        }

        /// Calls `f` once the GPU is done with the work issued so far and
        /// until the next [`Device::poll`], which makes its fence: for a
        /// frame's own work, whose fence made at once would wait for the
        /// browser's GPU process to take all of it in (`fenceSync` is a
        /// synchronous message to it in WebKit, read in its source).
        pub(in crate::gpu) fn on_done_later(&self, f: impl FnOnce() + 'static) {
            self.deferred.borrow_mut().push(Box::new(f));
        }

        pub(in crate::gpu) fn acquire(&mut self) -> Option<Frame> {
            Some(Frame(self.size()))
        }

        /// Draws `list` at `size` into `frame`, scaled to fit.
        pub(in crate::gpu) fn render_scaled(
            &self,
            renderer: &mut Renderer,
            frame: &Frame,
            list: &DrawList,
            clear: Color,
            size: (u32, u32),
        ) {
            renderer.render_scaled(&Destination::Canvas(frame.0), list, clear, size);
        }

        /// Shows `frame`: the browser shows the canvas once the task ends.
        pub(in crate::gpu) fn present(&self, _frame: Frame) {}

        /// Draws `list` at `size` into a frame never shown (the warm-up).
        pub(in crate::gpu) fn render_hidden(
            &self,
            renderer: &mut Renderer,
            list: &DrawList,
            clear: Color,
            size: (u32, u32),
        ) {
            renderer.render(&Destination::Hidden(size), list, clear);
        }

        /// Loses the context (`debug_lose_device`): `webglcontextlost`
        /// follows.
        #[cfg(feature = "test-ops")]
        pub(in crate::gpu) fn lose(&self) -> Result<(), String> {
            use wasm_bindgen::JsCast;
            let ext = self
                .gl
                .context()
                .get_extension("WEBGL_lose_context")
                .ok()
                .flatten()
                .ok_or("no WEBGL_lose_context")?;
            let lose: js_sys::Function = js_sys::Reflect::get(&ext, &"loseContext".into())
                .ok()
                .and_then(|f| f.dyn_into().ok())
                .ok_or("no loseContext")?;
            lose.call0(&ext).map_err(|e| format!("{e:?}"))?;
            Ok(())
        }
    }

    /// The render self-test's device: a WebGL 2 context of its own, so its
    /// renderers draw beside the app's without touching its state.
    #[cfg(feature = "test-ops")]
    pub(in crate::gpu) struct SelftestDevice {
        gl: Rc<Gl>,
    }

    #[cfg(feature = "test-ops")]
    impl SelftestDevice {
        pub(in crate::gpu) async fn new() -> Result<Self, String> {
            let canvas = OffscreenCanvas::new(4, 4).map_err(|e| format!("{e:?}"))?;
            let context = crate::gpu::make_webgl_context(&canvas).ok_or("no WebGL 2 canvas")?;
            Ok(Self {
                gl: Rc::new(Gl::new(context)),
            })
        }

        pub(in crate::gpu) fn make(&self) -> Renderer {
            Renderer::new(self.gl.clone())
        }

        /// Nothing waits on polling: captures are read at once.
        pub(in crate::gpu) fn poll(&self) {}
    }
}

#[cfg(not(feature = "gl"))]
mod wgpu_device {
    use std::sync::Arc;

    use volya_engine::render::{Color, DrawList, Renderer, wgpu};
    use web_sys::OffscreenCanvas;

    /// What a frame is drawn into: the surface's texture.
    pub(in crate::gpu) struct Frame(wgpu::SurfaceTexture);

    /// wgpu's device on the canvas, through its GL backend.
    pub(in crate::gpu) struct Device {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
        device: wgpu::Device,
        queue: wgpu::Queue,
    }

    /// The adapter and device on `surface`, with what WebGL 2 has.
    async fn device_on(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'static>,
    ) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(surface),
                ..Default::default()
            })
            .await
            .map_err(|_| "no WebGL 2 adapter".to_owned())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: volya_engine::render::device_features(&adapter),
                // What WebGL 2 has: less than wgpu's defaults.
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no WebGL 2 device: {e}"))?;
        Ok((adapter, device, queue))
    }

    fn instance() -> wgpu::Instance {
        wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        })
    }

    /// Colour values pass through: a non-sRGB format, preferred one first.
    fn format(
        surface: &wgpu::Surface<'static>,
        adapter: &wgpu::Adapter,
    ) -> Result<wgpu::TextureFormat, String> {
        surface
            .get_capabilities(adapter)
            .formats
            .into_iter()
            .find(|f| !f.is_srgb())
            .ok_or_else(|| "no non-sRGB canvas format".to_owned())
    }

    impl Device {
        /// The device on `canvas` (whose context the platform made), its
        /// renderer and the adapter's name.
        pub(in crate::gpu) async fn connect(
            canvas: OffscreenCanvas,
            _context: Option<web_sys::WebGl2RenderingContext>,
        ) -> Result<(Self, Renderer, String), String> {
            let instance = instance();
            let (width, height) = (canvas.width().max(1), canvas.height().max(1));
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas))
                .map_err(|e| format!("no WebGL 2 canvas: {e}"))?;
            let (adapter, device, queue) = device_on(&instance, &surface).await?;
            // Report instead of panicking, which would stop the whole engine.
            device.on_uncaptured_error(Arc::new(move |e| {
                web_sys::console::error_1(&format!("webgl2: {e}").into());
                tracing::error!(error = %e, "GPU error");
            }));
            // A destroyed device (`debug_lose_device`); a lost context is
            // `watch_context_loss`'s.
            device.set_device_lost_callback(move |reason, message| {
                let message = format!("webgl2: {reason:?}: {message}");
                web_sys::console::error_1(&message.clone().into());
                crate::diagnostics::end(1, "device_lost", &message, &[]);
            });
            let format = format(&surface, &adapter)?;
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
            let renderer = Renderer::new(&device, &queue, format);
            let name = adapter.get_info().name;
            Ok((
                Self {
                    surface,
                    config,
                    device,
                    queue,
                },
                renderer,
                name,
            ))
        }

        pub(in crate::gpu) fn max_texture_size(&self) -> u32 {
            self.device.limits().max_texture_dimension_2d
        }

        /// The canvas's size in pixels.
        pub(in crate::gpu) fn size(&self) -> (u32, u32) {
            (self.config.width, self.config.height)
        }

        pub(in crate::gpu) fn resize(&mut self, width: u32, height: u32) {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }

        /// The format frames are shown in.
        pub(in crate::gpu) fn format_name(&self) -> String {
            format!("{:?}", self.config.format)
        }

        /// Runs buffer-mapping callbacks (captures) and frees finished
        /// uploads: wgpu's GL backend does so only when polled.
        pub(in crate::gpu) fn poll(&self) {
            let _ = self.device.poll(wgpu::PollType::Poll);
        }

        /// Calls `f` once the GPU is done with the work submitted so far
        /// (wgpu's fences, checked when polled).
        pub(in crate::gpu) fn on_done(&self, f: impl FnOnce() + Send + 'static) {
            self.queue.on_submitted_work_done(f);
        }

        /// As [`Device::on_done`].
        pub(in crate::gpu) fn on_done_later(&self, f: impl FnOnce() + Send + 'static) {
            self.on_done(f);
        }

        pub(in crate::gpu) fn acquire(&mut self) -> Option<Frame> {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(Frame(t)),
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    // Drawn on the next frame.
                    self.surface.configure(&self.device, &self.config);
                    None
                }
                _ => None,
            }
        }

        /// Draws `list` at `size` into `frame`, scaled to fit.
        pub(in crate::gpu) fn render_scaled(
            &self,
            renderer: &mut Renderer,
            frame: &Frame,
            list: &DrawList,
            clear: Color,
            size: (u32, u32),
        ) {
            renderer.render_scaled(&frame.0.texture, list, clear, size);
        }

        pub(in crate::gpu) fn present(&self, frame: Frame) {
            self.queue.present(frame.0);
        }

        /// Draws `list` at `size` into a frame never shown (the warm-up).
        pub(in crate::gpu) fn render_hidden(
            &self,
            renderer: &mut Renderer,
            list: &DrawList,
            clear: Color,
            size: (u32, u32),
        ) {
            let target = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("warm-up"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            renderer.render(&target, list, clear);
            target.destroy();
        }

        /// Destroys the device (`debug_lose_device`), as a lost one.
        #[cfg(feature = "test-ops")]
        pub(in crate::gpu) fn lose(&self) -> Result<(), String> {
            self.device.destroy();
            Ok(())
        }
    }

    /// The render self-test's device: a WebGL 2 context of its own, so its
    /// renderers draw beside the app's without touching its state.
    #[cfg(feature = "test-ops")]
    pub(in crate::gpu) struct SelftestDevice {
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        _surface: wgpu::Surface<'static>,
    }

    #[cfg(feature = "test-ops")]
    impl SelftestDevice {
        pub(in crate::gpu) async fn new() -> Result<Self, String> {
            let canvas = OffscreenCanvas::new(4, 4).map_err(|e| format!("{e:?}"))?;
            crate::gpu::make_webgl_context(&canvas);
            let instance = instance();
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas))
                .map_err(|e| e.to_string())?;
            let (adapter, device, queue) = device_on(&instance, &surface).await?;
            device.on_uncaptured_error(Arc::new(move |e| {
                web_sys::console::error_1(&format!("render self-test: {e}").into());
            }));
            let format = format(&surface, &adapter)?;
            Ok(Self {
                device,
                queue,
                format,
                _surface: surface,
            })
        }

        pub(in crate::gpu) fn make(&self) -> Renderer {
            Renderer::new(&self.device, &self.queue, self.format)
        }

        /// Runs buffer-mapping callbacks (the captures): wgpu's GL backend
        /// does so only when polled.
        pub(in crate::gpu) fn poll(&self) {
            let _ = self.device.poll(wgpu::PollType::Poll);
        }
    }
}
