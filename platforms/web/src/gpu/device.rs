// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The renderer's device: the canvas's WebGL 2 context, where a frame
//! goes, and fences.

pub(super) use gl_device::*;

/// The context's renderer: `UNMASKED_RENDERER_WEBGL` where the browser
/// exposes `WEBGL_debug_renderer_info`, else `RENDERER`.
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

        /// The format frames are shown in.
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
    }
}
