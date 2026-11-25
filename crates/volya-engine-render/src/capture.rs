// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Framebuffer readback for tests and debugging (docs/testing.md): renders a
//! draw list offscreen and reads the pixels back.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use crate::{DrawList, Renderer};

/// Copies of texture rows to buffers are aligned to this many bytes.
const ROW_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

impl Renderer {
    /// Renders `list` into a `width` × `height` target of the renderer's
    /// format, cleared to `clear`, and returns its pixels as RGBA, 8 bits per
    /// channel, rows top to bottom.
    ///
    /// The draw is submitted before this returns; the future only waits for
    /// the pixels and borrows nothing. Natively awaiting it blocks until the
    /// GPU is done; on the web it waits for the browser.
    pub fn capture(
        &mut self,
        list: &DrawList,
        width: u32,
        height: u32,
        clear: wgpu::Color,
    ) -> impl Future<Output = Result<Vec<u8>, wgpu::BufferAsyncError>> + use<> {
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.render(&target, list, clear);
        read_texture(&self.device, &self.queue, &target)
    }

    /// As [`Renderer::capture`], with `list` drawn at `size` and shown
    /// scaled to fit ([`Renderer::render_scaled`]).
    pub fn capture_scaled(
        &mut self,
        list: &DrawList,
        size: (u32, u32),
        width: u32,
        height: u32,
        clear: wgpu::Color,
    ) -> impl Future<Output = Result<Vec<u8>, wgpu::BufferAsyncError>> + use<> {
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.render_scaled(&target, list, clear, size);
        read_texture(&self.device, &self.queue, &target)
    }
}

/// Reads mip level 0 of an RGBA8 or BGRA8 `texture` back as RGBA.
fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> impl Future<Output = Result<Vec<u8>, wgpu::BufferAsyncError>> + use<> {
    let (width, height) = (texture.width(), texture.height());
    let row = width * 4;
    let padded = row.div_ceil(ROW_ALIGNMENT) * ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture"),
        size: u64::from(padded) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    crate::stats::submit();
    queue.submit([encoder.finish()]);

    let (sender, receiver) = oneshot();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| sender.send(result));
    // Natively this runs the callback; the browser runs it on its own.
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let bgra = matches!(
        texture.format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    async move {
        receiver.await?;
        let mapped = buffer
            .get_mapped_range(..)
            .map_err(|_| wgpu::BufferAsyncError)?;
        let mut pixels = Vec::with_capacity((row * height) as usize);
        for r in mapped.chunks(padded as usize) {
            pixels.extend_from_slice(&r[..row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        if bgra {
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        Ok(pixels)
    }
}

/// A single-value channel: the buffer-mapping callback sends, the capture
/// awaits.
struct Shared<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

struct Sender<T>(Arc<Mutex<Shared<T>>>);
struct Receiver<T>(Arc<Mutex<Shared<T>>>);

fn oneshot<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Mutex::new(Shared {
        value: None,
        waker: None,
    }));
    (Sender(shared.clone()), Receiver(shared))
}

impl<T> Sender<T> {
    fn send(self, value: T) {
        let mut shared = self.0.lock().unwrap_or_else(|e| e.into_inner());
        shared.value = Some(value);
        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut shared = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match shared.value.take() {
            Some(value) => Poll::Ready(value),
            None => {
                shared.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
