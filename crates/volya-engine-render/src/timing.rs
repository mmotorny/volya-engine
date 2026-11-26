// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The GPU time of frames (#355), where the device has timestamp queries
//! (natively; WebGL 2 has none): each render pass of a timed frame
//! writes timestamps at its beginning and end ([`pass_writes`], at most
//! [`PASSES`] a frame; the frame's last pass, the one showing the frame in
//! the target, writes the frame's end), so a frame's GPU time is split by
//! pass (#387): each pass from the later of its beginning and the previous
//! pass's end, as passes overlap (#433). The frame's time is the sum of its
//! passes', the GPU's work on it without the idle time between them, such
//! as while the CPU encodes the next submission (#441). Empty passes write
//! no timestamps on Metal (measured: zeros). The timestamps are resolved
//! and copied to a buffer read back once the GPU is done; a few frames are
//! in flight at once, and a frame finding none free goes untimed.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Frames timed at once.
const SLOTS: u32 = 4;
/// Passes of a frame timed one by one.
pub(crate) const PASSES: u32 = 8;
/// Timestamps of a slot: the frame's end, then two a pass.
const QUERIES: u32 = 1 + 2 * PASSES;
/// Where a slot's timestamps are resolved: resolve offsets must be
/// multiples of 256 bytes (wgpu).
const RESOLVE_STRIDE: u64 = 256;

thread_local! {
    /// The frame being timed: its query set, first query and the passes
    /// named so far.
    static TIMED: std::cell::RefCell<Option<(wgpu::QuerySet, u32, Vec<&'static str>)>> =
        const { std::cell::RefCell::new(None) };
}

/// The timestamp writes for the next pass of the frame being timed, named
/// `name`, `None` when no frame is timed or it has [`PASSES`] already.
/// `last`: the frame's last pass, which ends the frame too.
pub(crate) fn pass_writes(name: &'static str, last: bool) -> Option<(wgpu::QuerySet, u32, u32)> {
    TIMED.with_borrow_mut(|t| {
        let (queries, first, names) = t.as_mut()?;
        if names.len() as u32 >= PASSES {
            return None;
        }
        let begin = *first + 1 + 2 * names.len() as u32;
        names.push(name);
        Some((
            queries.clone(),
            begin,
            if last { *first } else { begin + 1 },
        ))
    })
}

/// [`pass_writes`] as a pass descriptor's field.
pub(crate) fn writes(
    t: &Option<(wgpu::QuerySet, u32, u32)>,
) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
    t.as_ref()
        .map(|(q, begin, end)| wgpu::RenderPassTimestampWrites {
            query_set: q,
            beginning_of_pass_write_index: Some(*begin),
            end_of_pass_write_index: Some(*end),
        })
}

const FREE: u8 = 0;
const PENDING: u8 = 1;
const MAPPED: u8 = 2;
/// The read back failed (the device lost): the slot stays out of use.
const FAILED: u8 = 3;

struct Slot {
    readback: wgpu::Buffer,
    state: Arc<AtomicU8>,
    /// The timed passes' names, in order.
    passes: Vec<&'static str>,
}

pub(crate) struct FrameTimer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<Slot>,
    /// Nanoseconds per timestamp tick.
    period_ns: f64,
    /// The passes' times of the frames read back, milliseconds.
    pass_times: Vec<(&'static str, f64)>,
}

impl FrameTimer {
    /// A timer, `None` where the device has no timestamp queries.
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame time"),
            ty: wgpu::QueryType::Timestamp,
            count: SLOTS * QUERIES,
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
                    size: 8 * u64::from(QUERIES),
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU8::new(FREE)),
                passes: Vec::new(),
            })
            .collect();
        Some(Self {
            queries,
            resolve,
            slots,
            period_ns: f64::from(queue.get_timestamp_period()),
            pass_times: Vec::new(),
        })
    }

    /// Starts timing a frame: the slot to end it with, `None` when every
    /// slot is busy.
    pub(crate) fn begin(&mut self) -> Option<usize> {
        let slot = self
            .slots
            .iter()
            .position(|s| s.state.load(Ordering::Acquire) == FREE)?;
        self.slots[slot].state.store(PENDING, Ordering::Release);
        TIMED.set(Some((
            self.queries.clone(),
            slot as u32 * QUERIES,
            Vec::new(),
        )));
        Some(slot)
    }

    /// Ends timing the frame of `slot`, once its last pass is submitted:
    /// submits the copy of its timestamps and asks for them to be read
    /// back.
    pub(crate) fn end(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, slot: usize) {
        // Natively on Metal a resolve submitted while the frame's passes
        // still run reads some of their timestamps from before they were
        // written (measured: the frame's last end and a few passes' ends
        // were the frame before's), so it waits for them there.
        #[cfg(not(target_arch = "wasm32"))]
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame time end"),
        });
        let passes = TIMED.take().map(|t| t.2).unwrap_or_default();
        let used = 1 + 2 * passes.len() as u32;
        let first = slot as u32 * QUERIES;
        let offset = RESOLVE_STRIDE * slot as u64;
        encoder.resolve_query_set(&self.queries, first..first + used, &self.resolve, offset);
        let s = &mut self.slots[slot];
        s.passes = passes;
        encoder.copy_buffer_to_buffer(&self.resolve, offset, &s.readback, 0, 8 * u64::from(used));
        crate::stats::submit();
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
    /// milliseconds: the sum of their passes'. A frame whose end is not
    /// after its first pass's beginning (timestamps may be zero or reset,
    /// documented by wgpu; on Metal some frames' last timestamp is zero,
    /// measured) is left out.
    pub(crate) fn take(&mut self) -> Vec<f64> {
        let mut times = Vec::new();
        for s in &mut self.slots {
            if s.state.load(Ordering::Acquire) != MAPPED {
                continue;
            }
            let ticks = s
                .readback
                .slice(..)
                .get_mapped_range()
                .ok()
                .and_then(|view| {
                    let at =
                        |i: usize| u64::from_le_bytes(view[i * 8..i * 8 + 8].try_into().unwrap());
                    let ms = |ticks: u64| ticks as f64 * self.period_ns / 1e6;
                    at(0).checked_sub(at(1)).filter(|&t| t > 0)?;
                    // A pass's end: its own, or the frame's for the last.
                    // A pass begins while the one before still draws (its
                    // vertex stage overlaps the previous fragment stage:
                    // measured natively on Metal, the four passes of a
                    // level frame all begin within its world pass), so a
                    // pass counts from the later of its beginning and the
                    // previous pass's end.
                    let n = s.passes.len();
                    let mut previous = 0;
                    let mut frame = 0;
                    for (k, name) in s.passes.iter().enumerate() {
                        let end = if k + 1 == n { at(0) } else { at(2 + 2 * k) };
                        let begin = at(1 + 2 * k).max(previous);
                        if let Some(t) = end.checked_sub(begin).filter(|&t| t > 0) {
                            self.pass_times.push((name, ms(t)));
                            frame += t;
                        }
                        previous = previous.max(end);
                    }
                    (frame > 0).then_some(frame)
                });
            s.readback.unmap();
            s.state.store(FREE, Ordering::Release);
            if let Some(ticks) = ticks {
                times.push(ticks as f64 * self.period_ns / 1e6);
            }
        }
        times
    }

    /// The passes' GPU times of the frames read back since the last call,
    /// milliseconds, by name.
    pub(crate) fn take_passes(&mut self) -> Vec<(&'static str, f64)> {
        std::mem::take(&mut self.pass_times)
    }
}
