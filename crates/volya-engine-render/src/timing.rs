// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The GPU time of frames (#355), where the device has timestamp queries
//! (WebGPU's optional `timestamp-query`): a pass clearing a texel, submitted
//! before the frame, writes a timestamp at its beginning, and the frame's
//! last pass (the one showing the frame in the target, after what it shows
//! is drawn) one at its end, so the time between them is the GPU's from the
//! start of the frame to the end of its last pass. Empty passes write no
//! timestamps on Metal (measured: zeros). The two timestamps are
//! resolved and copied to a buffer read back once the GPU is done; a few
//! frames are in flight at once, and a frame finding none free goes
//! untimed.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Frames timed at once.
const SLOTS: u32 = 4;
/// Where a slot's two timestamps are resolved: resolve offsets must be
/// multiples of 256 bytes (WebGPU).
const RESOLVE_STRIDE: u64 = 256;

const FREE: u8 = 0;
const PENDING: u8 = 1;
const MAPPED: u8 = 2;
/// The read back failed (the device lost): the slot stays out of use.
const FAILED: u8 = 3;

struct Slot {
    readback: wgpu::Buffer,
    state: Arc<AtomicU8>,
}

pub(crate) struct FrameTimer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    /// The texel the first pass clears.
    texel: wgpu::Texture,
    slots: Vec<Slot>,
    /// Nanoseconds per timestamp tick.
    period_ns: f64,
}

impl FrameTimer {
    /// A timer, `None` where the device has no timestamp queries.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame time"),
            ty: wgpu::QueryType::Timestamp,
            count: SLOTS * 2,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame time resolve"),
            size: RESOLVE_STRIDE * u64::from(SLOTS),
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = (0..SLOTS)
            .map(|_| Slot {
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("frame time readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU8::new(FREE)),
            })
            .collect();
        let texel = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame time begin"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        Some(Self {
            queries,
            resolve,
            texel,
            slots,
            period_ns: f64::from(queue.get_timestamp_period()),
        })
    }

    /// Starts timing a frame: submits the pass writing its first
    /// timestamp. The slot to end it with, `None` when every slot is busy.
    pub(crate) fn begin(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<usize> {
        let slot = self
            .slots
            .iter()
            .position(|s| s.state.load(Ordering::Acquire) == FREE)?;
        self.slots[slot].state.store(PENDING, Ordering::Release);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame time begin"),
        });
        let view = self.texel.create_view(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("frame time begin"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: Some(slot as u32 * 2),
                end_of_pass_write_index: None,
            }),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        queue.submit([encoder.finish()]);
        Some(slot)
    }

    /// The timestamp writes of the last pass of the frame of `slot`.
    pub(crate) fn last_pass(&self, slot: usize) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: None,
            end_of_pass_write_index: Some(slot as u32 * 2 + 1),
        }
    }

    /// Ends timing the frame of `slot`, once its last pass is submitted:
    /// submits the copy of both timestamps and asks for them to be read
    /// back.
    pub(crate) fn end(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, slot: usize) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame time end"),
        });
        let first = slot as u32 * 2;
        let offset = RESOLVE_STRIDE * slot as u64;
        encoder.resolve_query_set(&self.queries, first..first + 2, &self.resolve, offset);
        let s = &self.slots[slot];
        encoder.copy_buffer_to_buffer(&self.resolve, offset, &s.readback, 0, 16);
        queue.submit([encoder.finish()]);
        let state = s.state.clone();
        s.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let next = if result.is_ok() { MAPPED } else { FAILED };
                state.store(next, Ordering::Release);
            });
    }

    /// The GPU times of the frames read back since the last call,
    /// milliseconds. A frame whose last timestamp is not after its first
    /// (timestamps may be zero or reset, documented by WebGPU; natively on
    /// Metal some frames' last timestamp is zero, measured) is left out.
    pub(crate) fn take(&mut self) -> Vec<f64> {
        let mut times = Vec::new();
        for s in &self.slots {
            if s.state.load(Ordering::Acquire) != MAPPED {
                continue;
            }
            let ticks = s.readback.slice(..).get_mapped_range().ok().and_then(|view| {
                let at = |i: usize| u64::from_le_bytes(view[i * 8..i * 8 + 8].try_into().unwrap());
                at(1).checked_sub(at(0)).filter(|&t| t > 0)
            });
            s.readback.unmap();
            s.state.store(FREE, Ordering::Release);
            if let Some(ticks) = ticks {
                times.push(ticks as f64 * self.period_ns / 1e6);
            }
        }
        times
    }
}
