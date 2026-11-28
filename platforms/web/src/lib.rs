// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya Engine for web browsers. Runs inside the engine worker
//! (`static/engine-worker.js`); the page talks to it only by message
//! (docs/architecture/web-platform.md). Builds to nothing on other targets.

#![cfg(target_arch = "wasm32")]

mod audio;
mod audio_host;
mod audio_ring;
mod diagnostics;
mod engine;
mod game;
mod gpu;
mod import;
mod input;
mod opfs;
mod textures;

use serde::Serialize;
use volya_engine::core::audio::{Category, Playback};
use volya_engine::core::file_cache;
use volya_engine::core::vfs::conformance;
use volya_engine::formats::ui::Resolution;
use wasm_bindgen::prelude::*;
use web_sys::{OffscreenCanvas, WorkerGlobalScope};

/// The global allocator is the memory manager
/// (`volya_engine::core::memory_manager`), whose bytes in use are the
/// console's `* phase cmem` and *h* (docs/behavior/console-diagnostics.md
/// 2.1.1, 2.4). Every build also counts the live bytes asked for, as
/// `loading_memory.rs` counts them natively (the `load_memory` op). Test builds also count their
/// high-water mark (the `heap_log` op), to tell what a
/// stage holds from what fragmentation adds to the heap (#357); and where
/// the live bytes sit (the `heap_map` op, #432): per MiB of the heap, and
/// each live block of 256 KiB or more.
#[allow(unsafe_code)] // `GlobalAlloc` is unsafe to implement; this forwards.
mod counting {
    use std::alloc::{GlobalAlloc, Layout};

    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    use volya_engine::core::memory_manager::MemoryManager;

    static LIVE: AtomicUsize = AtomicUsize::new(0);

    pub fn live() -> usize {
        LIVE.load(Relaxed)
    }

    struct Counting;

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            #[cfg(feature = "test-ops")]
            let before = crate::tested::memory_bytes();
            let p = unsafe { MemoryManager.alloc(layout) };
            if !p.is_null() {
                LIVE.fetch_add(layout.size(), Relaxed);
                #[cfg(feature = "test-ops")]
                crate::tested::added(before, layout.size(), p);
            }
            p
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            #[cfg(feature = "test-ops")]
            let before = crate::tested::memory_bytes();
            let p = unsafe { MemoryManager.alloc_zeroed(layout) };
            if !p.is_null() {
                LIVE.fetch_add(layout.size(), Relaxed);
                #[cfg(feature = "test-ops")]
                crate::tested::added(before, layout.size(), p);
            }
            p
        }

        unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
            unsafe { MemoryManager.dealloc(p, layout) };
            LIVE.fetch_sub(layout.size(), Relaxed);
            #[cfg(feature = "test-ops")]
            crate::tested::mapped(p, layout.size(), false);
        }

        unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            #[cfg(feature = "test-ops")]
            let before = crate::tested::memory_bytes();
            let q = unsafe { MemoryManager.realloc(p, layout, size) };
            if !q.is_null() {
                if size > layout.size() {
                    LIVE.fetch_add(size - layout.size(), Relaxed);
                } else {
                    LIVE.fetch_sub(layout.size() - size, Relaxed);
                }
                #[cfg(feature = "test-ops")]
                crate::tested::moved(before, p, layout.size(), q, size);
            }
            q
        }
    }

    #[global_allocator]
    static ALLOCATOR: Counting = Counting;

    #[cfg(feature = "test-ops")]
    pub use crate::tested::*;
}

/// The test builds' counts beside the live bytes ([`counting`]).
#[cfg(feature = "test-ops")]
mod tested {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

    use crate::counting::live;

    static PEAK: AtomicUsize = AtomicUsize::new(0);
    /// Allocations made (a growing reallocation counts as one) and their
    /// bytes (its new size), for a frame's heap traffic (#494).
    static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
    static BYTES: AtomicUsize = AtomicUsize::new(0);
    /// Live bytes per MiB of addresses (wasm32: at most 4 GiB).
    static BY_MIB: [AtomicUsize; 4096] = [const { AtomicUsize::new(0) }; 4096];
    /// Live blocks of at least `LARGE` bytes: (address, size), 0 for free.
    const LARGE: usize = 256 << 10;
    static LARGE_BLOCKS: [(AtomicUsize, AtomicUsize); 512] =
        [const { (AtomicUsize::new(0), AtomicUsize::new(0)) }; 512];

    /// Live bytes by block size, per power of two (`[k]`: 2^k ≤ size < 2^(k+1)).
    static BY_SIZE: [AtomicUsize; 33] = [const { AtomicUsize::new(0) }; 33];
    /// The most live since `reset_record`, and what was live then: the
    /// sizes' bytes and the large blocks (taken each 256 KiB of new record).
    static RECORD: AtomicUsize = AtomicUsize::new(0);
    static RECORD_TAKEN: AtomicUsize = AtomicUsize::new(0);
    static RECORD_BY_SIZE: [AtomicUsize; 33] = [const { AtomicUsize::new(0) }; 33];
    static RECORD_LARGE: [(AtomicUsize, AtomicUsize); 512] =
        [const { (AtomicUsize::new(0), AtomicUsize::new(0)) }; 512];

    /// The allocations that grew the wasm memory since `reset_record`
    /// (#487): (bytes asked for, address, the memory's bytes after).
    static GROWTHS: [(AtomicUsize, AtomicUsize, AtomicUsize); 64] = [const {
        (
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        )
    }; 64];
    static GROWTH_COUNT: AtomicUsize = AtomicUsize::new(0);

    pub(crate) fn memory_bytes() -> usize {
        #[cfg(target_arch = "wasm32")]
        return core::arch::wasm32::memory_size(0) << 16;
        #[cfg(not(target_arch = "wasm32"))]
        0
    }

    /// Notes an allocation of `size` at `p` if the memory grew from `before`.
    fn noted(before: usize, size: usize, p: *mut u8) {
        let after = memory_bytes();
        if after > before {
            let n = GROWTH_COUNT.fetch_add(1, Relaxed);
            if let Some(g) = GROWTHS.get(n) {
                g.0.store(size, Relaxed);
                g.1.store(p as usize, Relaxed);
                g.2.store(after, Relaxed);
            }
        }
    }

    /// The allocations that grew the memory since the reset, in order:
    /// (KiB asked for, address MiB, the memory's MiB after); and how many.
    pub fn growths() -> (Vec<(usize, f64, f64)>, usize) {
        let n = GROWTH_COUNT.load(Relaxed);
        let mib = |b: usize| b as f64 / (1 << 20) as f64;
        let list = (GROWTHS.iter().take(n))
            .map(|g| {
                (
                    g.0.load(Relaxed) >> 10,
                    mib(g.1.load(Relaxed)),
                    mib(g.2.load(Relaxed)),
                )
            })
            .collect();
        (list, n)
    }

    /// After the live bytes grew.
    fn grew() {
        PEAK.fetch_max(live(), Relaxed);
    }

    /// After a block of `size` was allocated at `p`, the memory's bytes
    /// `before` it.
    pub(crate) fn added(before: usize, size: usize, p: *mut u8) {
        noted(before, size, p);
        grew();
        allocated(size);
        mapped(p, size, true);
        recorded();
    }

    /// After the block of `old` bytes at `p` became `size` bytes at `q`.
    pub(crate) fn moved(before: usize, p: *mut u8, old: usize, q: *mut u8, size: usize) {
        noted(before, size, q);
        mapped(p, old, false);
        mapped(q, size, true);
        if size > old {
            grew();
            allocated(size);
            recorded();
        }
    }

    /// A block of `size` bytes allocated, or grown to it.
    fn allocated(size: usize) {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size, Relaxed);
    }

    /// Allocations made, their bytes, and the bytes live.
    pub fn traffic() -> (usize, usize, usize) {
        (ALLOCATIONS.load(Relaxed), BYTES.load(Relaxed), live())
    }

    /// After a block was added: a new record's sizes and large blocks.
    fn recorded() {
        let live = live();
        if live <= RECORD.load(Relaxed) {
            return;
        }
        RECORD.store(live, Relaxed);
        if live < RECORD_TAKEN.load(Relaxed) + (256 << 10) {
            return;
        }
        RECORD_TAKEN.store(live, Relaxed);
        for (r, b) in RECORD_BY_SIZE.iter().zip(&BY_SIZE) {
            r.store(b.load(Relaxed), Relaxed);
        }
        for (r, b) in RECORD_LARGE.iter().zip(&LARGE_BLOCKS) {
            r.0.store(b.0.load(Relaxed), Relaxed);
            r.1.store(b.1.load(Relaxed), Relaxed);
        }
    }

    /// Starts a new record from what is live now.
    pub fn reset_record() {
        GROWTH_COUNT.store(0, Relaxed);
        RECORD.store(0, Relaxed);
        RECORD_TAKEN.store(0, Relaxed);
        recorded();
    }

    /// The record: live bytes then (as of the last snapshot), bytes per
    /// size class (KiB, by the class's lower bound in bytes) and the large
    /// blocks (address MiB, KiB) by size, largest first.
    pub fn record() -> (usize, Vec<(usize, usize)>, Vec<(f64, usize)>) {
        let sizes = (RECORD_BY_SIZE.iter().enumerate())
            .map(|(k, b)| (1usize << k.min(31), b.load(Relaxed) >> 10))
            .filter(|&(_, kib)| kib > 0)
            .collect();
        let mut large: Vec<(f64, usize)> = (RECORD_LARGE.iter())
            .filter_map(|s| {
                let p = s.0.load(Relaxed);
                (p != 0).then(|| (p as f64 / (1 << 20) as f64, s.1.load(Relaxed) >> 10))
            })
            .collect();
        large.sort_by(|a, b| b.1.cmp(&a.1));
        (RECORD_TAKEN.load(Relaxed), sizes, large)
    }

    /// Adds (or with `add` false removes) `size` bytes at `p` to the map.
    pub(crate) fn mapped(p: *mut u8, size: usize, add: bool) {
        let class = &BY_SIZE[(usize::BITS - 1 - size.max(1).leading_zeros()) as usize];
        if add {
            class.fetch_add(size, Relaxed);
        } else {
            class.fetch_sub(size, Relaxed);
        }
        let (start, end) = (p as usize, p as usize + size);
        let mut at = start;
        while at < end {
            let mib = at >> 20;
            let next = ((mib + 1) << 20).min(end);
            if let Some(b) = BY_MIB.get(mib) {
                if add {
                    b.fetch_add(next - at, Relaxed);
                } else {
                    b.fetch_sub(next - at, Relaxed);
                }
            }
            at = next;
        }
        if size >= LARGE {
            if add {
                if let Some(slot) = LARGE_BLOCKS.iter().find(|s| s.0.load(Relaxed) == 0) {
                    slot.1.store(size, Relaxed);
                    slot.0.store(p as usize, Relaxed);
                }
            } else if let Some(slot) = LARGE_BLOCKS
                .iter()
                .find(|s| s.0.load(Relaxed) == p as usize)
            {
                slot.0.store(0, Relaxed);
            }
        }
    }

    /// Live bytes, and the most live since the last call.
    pub fn take() -> (usize, usize) {
        let live = live();
        (live, PEAK.swap(live, Relaxed))
    }

    /// Live KiB in each MiB of the first `mibs` MiB, and the live blocks of
    /// 256 KiB or more as (address MiB, size KiB), by address.
    pub fn map(mibs: usize) -> (Vec<usize>, Vec<(f64, usize)>) {
        let per = (BY_MIB.iter().take(mibs))
            .map(|b| b.load(Relaxed) >> 10)
            .collect();
        let mut large: Vec<(f64, usize)> = (LARGE_BLOCKS.iter())
            .filter_map(|s| {
                let p = s.0.load(Relaxed);
                (p != 0).then(|| (p as f64 / (1 << 20) as f64, s.1.load(Relaxed) >> 10))
            })
            .collect();
        large.sort_by(|a, b| a.0.total_cmp(&b.0));
        (per, large)
    }
}

/// Starts the engine on the canvas transferred from the page. Without
/// graphics the engine still starts; `status` reports why they are
/// unavailable.
#[wasm_bindgen]
pub async fn start(canvas: OffscreenCanvas) {
    // The statistics overlay's timers read the worker's performance.now().
    volya_engine::core::perf_counter::install(now);
    volya_engine::core::console::set_meters(volya_engine::core::console::Meters {
        memory: memory_statistics,
        resources: gpu::resource_counts,
        remaining: gpu::remaining_resources,
    });
    gpu::init(canvas).await;
    diagnostics::started().await;
}

/// The canvas's size in CSS pixels and the page's devicePixelRatio, on start
/// and whenever they change.
#[wasm_bindgen]
pub fn resize(width: f64, height: f64, dpr: f64) {
    gpu::resize(width, height, dpr);
    input::resize(width, height);
    let pixels = |css: f64| ((css * dpr).round() as u32).max(1);
    // The canvas's own size: what the resolution list offers (#49).
    engine::resized(gpu::resolution().unwrap_or(Resolution::new(pixels(width), pixels(height))));
}

/// An interval between frames the diagnostic log records as an event,
/// milliseconds.
const GAP_MS: f64 = 250.0;

/// Frames whose timings [`FrameStats`] averages.
const STATS_FRAMES: usize = 60;

/// Timings of the latest frames, milliseconds.
#[derive(Default)]
struct FrameStats {
    /// Time spent in [`frame`] (engine update, scene, draw submission).
    cpu: std::collections::VecDeque<f64>,
    /// Animation-frame time between consecutive frames.
    interval: std::collections::VecDeque<f64>,
    /// Time spent in the sound system's update (occlusion included).
    sound: std::collections::VecDeque<f64>,
    last: Option<f64>,
    /// Frames run so far.
    frames: u64,
}

thread_local! {
    static FRAME_STATS: std::cell::RefCell<FrameStats> = Default::default();
    /// While on (tests: the `heap_log` op), each frame's loading caption,
    /// wasm heap size at its end, time and CPU time.
    static HEAP_LOG: std::cell::RefCell<Option<Vec<serde_json::Value>>> = Default::default();
    /// The least time a frame takes (tests: the `debug_frame_cost` op), ms:
    /// a slower device's frames, the worker busy for that long.
    #[cfg(feature = "test-ops")]
    static FRAME_COST: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// The wasm heap (linear memory) size, bytes.
pub(crate) fn heap_bytes() -> f64 {
    wasm_bindgen::memory()
        .dyn_into::<js_sys::WebAssembly::Memory>()
        .map(|m| {
            m.buffer()
                .unchecked_into::<js_sys::ArrayBuffer>()
                .byte_length() as f64
        })
        .unwrap_or(0.0)
}

/// What `stat_memory` and the phase lines print
/// (docs/behavior/console-diagnostics.md 2.1, 2.4), by their counterparts
/// here (docs/architecture/memory.md, Measuring): the address space is the
/// wasm memory's (32-bit, 4 GiB: its pages committed, the rest free, none
/// reserved), the heaps and the memory manager are the memory manager's
/// bytes in use (its chunks whole, its large blocks with their 23 bytes),
/// the textures the sizes the renderer records for the files it holds by
/// name (`gl::Texture::record_size`), the game scripts' and the renderer's
/// pools their bytes in use (`memory_manager::GAME_SCRIPTS`, `RENDER`). No
/// part has a counterpart of the process heap, the engine's scripts' pool,
/// or the shared strings and blocks: 0.
/// The audio worker's instance (the sound memory) is in neither: the
/// `load_memory` op reports it beside them (docs/architecture/memory.md,
/// Deviations).
fn memory_statistics() -> volya_engine::core::console::MemoryStatistics {
    let committed = heap_bytes() as u64;
    let in_use = volya_engine::core::memory_manager::in_use_bytes() as u64;
    let gpu = gpu::live();
    volya_engine::core::console::MemoryStatistics {
        free: (1u64 << 32).saturating_sub(committed),
        reserved: 0,
        committed,
        textures: gpu.recorded_bytes,
        crt_heap: in_use,
        game_lua: volya_engine::core::memory_manager::GAME_SCRIPTS.in_use_bytes() as u64,
        render: volya_engine::core::memory_manager::RENDER.in_use_bytes() as u64,
        cmem: in_use,
        ..Default::default()
    }
}

/// `performance.now()` of the worker, milliseconds.
pub(crate) fn now() -> f64 {
    js_sys::global()
        .dyn_into::<WorkerGlobalScope>()
        .ok()
        .and_then(|g| g.performance())
        .map_or_else(js_sys::Date::now, |p| p.now())
}

/// Milliseconds since the epoch, comparable between workers (each
/// worker's `performance.now()` counts from its own start).
pub(crate) fn epoch_now() -> f64 {
    js_sys::global()
        .dyn_into::<WorkerGlobalScope>()
        .ok()
        .and_then(|g| g.performance())
        .map_or_else(js_sys::Date::now, |p| p.time_origin() + p.now())
}

/// Runs one animation frame; `time` in milliseconds.
#[wasm_bindgen]
pub fn frame(time: f64) {
    // The diagnostic log's spans (#346): the frame and what runs in it.
    let span = tracing::info_span!("frame").entered();
    let start = now();
    tracing::info_span!("engine").in_scope(|| engine::frame(time));
    let sound_start = now();
    tracing::info_span!("sound").in_scope(|| audio::sound_frame(time));
    let sound = now() - sound_start;
    let render_start = now();
    tracing::info_span!("render").in_scope(|| gpu::frame(time));
    // For the statistics overlay (`rs_stats`), shown next frame.
    engine::set_frame_times(volya_engine::statistics_overlay::FrameTimes {
        engine_ms: sound_start - start,
        render_ms: now() - render_start,
        sound_ms: sound,
        draws: gpu::last_draws(),
    });
    input::end_frame();
    tracing::info_span!("textures").in_scope(textures::poll);
    #[cfg(feature = "test-ops")]
    while now() - start < FRAME_COST.with(std::cell::Cell::get) {}
    let cpu = now() - start;
    drop(span);
    #[cfg(feature = "test-ops")]
    audio::capture::frame(cpu);
    if HEAP_LOG.with(|l| l.borrow().is_some()) {
        let state = engine::state();
        let entry = serde_json::json!({
            "phase": state.get("phase"),
            "caption": state.get("loading").and_then(|l| l.get("caption")),
            "wasm_heap_bytes": heap_bytes(),
            // When the frame ran and its CPU time, ms: each caption's time
            // and the level's first frames (#813).
            "time_ms": time,
            "cpu_ms": cpu,
        });
        #[cfg(feature = "test-ops")]
        let entry = {
            let mut entry = entry;
            let (live, peak) = counting::take();
            entry["live_bytes"] = live.into();
            entry["peak_bytes"] = peak.into();
            entry
        };
        HEAP_LOG.with(|l| {
            if let Some(log) = l.borrow_mut().as_mut() {
                log.push(entry);
            }
        });
    }
    FRAME_STATS.with(|s| {
        let mut s = s.borrow_mut();
        let push = |q: &mut std::collections::VecDeque<f64>, v: f64| {
            if q.len() == STATS_FRAMES {
                q.pop_front();
            }
            q.push_back(v);
        };
        push(&mut s.cpu, cpu);
        push(&mut s.sound, sound);
        if let Some(last) = s.last {
            push(&mut s.interval, time - last);
            textures::frame_interval(time - last);
            volya_engine::core::diagnostics::metric("frame.interval_ms", time - last);
            // Over one and a half frames at 60 Hz.
            volya_engine::core::diagnostics::count("frame.late", u64::from(time - last > 25.0));
            // A gap long enough to stall sound: when it came (#355).
            if time - last > GAP_MS {
                tracing::warn!(interval_ms = time - last, "frame gap");
            }
        }
        s.last = Some(time);
        s.frames += 1;
    });
    diagnostics::tick(false);
}

/// The page was hidden (`hidden`) or shown again (our design, #93): the
/// engine stops while hidden; the worker's frame loop idles meanwhile and
/// the page suspends audio output.
#[wasm_bindgen]
pub fn visibility(hidden: bool) {
    engine::set_hidden(hidden);
    audio::with_sounds(|host| host.set_hidden(hidden));
    // The interval spanning the hidden time is not a frame time.
    FRAME_STATS.with(|s| s.borrow_mut().last = None);
    tracing::info!(hidden, "page visibility");
    if hidden {
        // Hidden pages may be ended without notice (iOS): recorded as an
        // end and stored now. Shown again, the session goes on.
        diagnostics::end(2, "page_hidden", "the page was hidden", &[]);
    }
}

#[derive(Serialize)]
struct LevelStats {
    /// The wasm heap (linear memory) size.
    wasm_heap_bytes: f64,
    /// GPU memory of the static world (buffers and textures).
    #[serde(flatten)]
    gpu: serde_json::Value,
    /// The latest level's texture preparation (#113).
    textures: textures::Stats,
    /// Averages over the latest frames, milliseconds.
    frame_cpu_ms: f64,
    frame_cpu_max_ms: f64,
    frame_interval_ms: f64,
    /// The sound system's update per frame: average and worst.
    sound_update_ms: f64,
    sound_update_max_ms: f64,
    /// The audio worker's own (its heap, mixing time, underruns).
    #[serde(flatten)]
    audio: audio::HostStats,
    /// The file cache (docs/architecture/memory.md, File-backed data).
    file_cache: FileCacheStats,
}

/// The file cache's figures, bytes: held now, by file, and the most since
/// the load began.
#[derive(Serialize)]
struct FileCacheStats {
    bytes: u64,
    files: Vec<(String, u64)>,
    most: u64,
}

fn level_stats() -> LevelStats {
    let heap = heap_bytes();
    let average =
        |q: &std::collections::VecDeque<f64>| q.iter().sum::<f64>() / q.len().max(1) as f64;
    FRAME_STATS.with(|s| {
        let s = s.borrow();
        LevelStats {
            wasm_heap_bytes: heap,
            gpu: gpu::world_stats(),
            textures: textures::stats(),
            frame_cpu_ms: average(&s.cpu),
            frame_cpu_max_ms: s.cpu.iter().copied().fold(0.0, f64::max),
            frame_interval_ms: average(&s.interval),
            sound_update_ms: average(&s.sound),
            sound_update_max_ms: s.sound.iter().copied().fold(0.0, f64::max),
            audio: audio::host_stats(),
            file_cache: FileCacheStats {
                bytes: file_cache::bytes(),
                files: file_cache::held(),
                most: file_cache::most(),
            },
        }
    })
}

/// One input message from the page (format in the `input` module).
#[wasm_bindgen]
pub fn input(message: js_sys::Array) -> Result<(), JsValue> {
    input::handle(&message)
}

/// Whether the engine owns the screen, so the page should forward input to
/// it. Off by default; the `input_capture` op sets it.
#[wasm_bindgen]
pub fn input_capture() -> bool {
    input::capture()
}

/// The level's controls for the page: bit 1 while they are live,
/// plus 2 while the crouch toggle is engaged, 4 while a sprint is
/// requested, 8 while the toggled crouch is the low crouch and 16 while the
/// sprint lock holds the touch stick; 32 while the character takes keys
/// (the level's controls live, or the inventory screen over them, #110);
/// 64 while the touch Torch button shows and 128 while the torch is on, 256
/// while the Night vision button shows and 512 while night vision is
/// active (#363); 1024 while the PDA window is open over the running level
/// (the touch PDA button shows over it, #730); 2048 while an object under
/// the crosshair has a hint (the touch Use button shows, #874).
#[wasm_bindgen]
pub fn level_controls() -> u32 {
    let keys =
        u32::from(engine::character_takes_keys()) << 5 | u32::from(engine::pda_over_level()) << 10;
    let button = |b: Option<bool>, shift: u32| match b {
        None => 0,
        Some(on) => (1 | u32::from(on) << 1) << shift,
    };
    keys | engine::level_controls().map_or(0, |c| {
        1 | u32::from(c.crouch) << 1
            | u32::from(c.sprint) << 2
            | u32::from(c.low_crouch) << 3
            | u32::from(c.sprint_lock) << 4
            | button(c.torch, 6)
            | button(c.night_vision, 8)
            | u32::from(c.usable) << 11
    })
}

/// The game's resolution follows the canvas's size (`on`: the page's
/// ergonomic mobile mode, #806) or not; [`resize`] first gives the
/// size of now.
#[wasm_bindgen]
pub fn follow_screen(on: bool) {
    engine::set_follow_screen(on);
}

/// The page is in the browser's full screen (`on`) or not, or can't be
/// (`supported` false): `rs_fullscreen` follows it, and the options
/// screen's check box is disabled where it can't be.
#[wasm_bindgen]
pub fn full_screen(on: bool, supported: bool) {
    engine::set_full_screen(on, supported);
}

/// Full screen asked of the page by a video restart, once: 1 to enter, 0
/// to leave, −1 for nothing.
#[wasm_bindgen]
pub fn full_screen_request() -> i32 {
    engine::take_full_screen_request().map_or(-1, i32::from)
}

/// Whether the console is open: the page reads the clipboard for an
/// Insert press then (#639).
#[wasm_bindgen]
pub fn console_open() -> bool {
    engine::console_open()
}

/// Whether the console would open over the main menu now: the page's
/// Console button for touch screens shows over the menu then (#642).
#[wasm_bindgen]
pub fn console_opens_over_menu() -> bool {
    engine::console_opens_over_menu()
}

/// Whether the game shows its mouse cursor: the page's touch drives the
/// cursor then (the on-screen stick, #856, or direct clicks, #807).
#[wasm_bindgen]
pub fn cursor_shown() -> bool {
    engine::cursor_shown()
}

/// Where the destination of a tap-to-move trip under way shows in the
/// picture, for the touch controls' marker: `[x, y]` from its centre in half
/// its height, +y down; none without a trip or with it behind the eye.
#[wasm_bindgen]
pub fn trip_marker() -> Option<Vec<f32>> {
    engine::trip_marker().map(Vec::from)
}

/// The save screen's name field while it shows, for the page's text entry
/// (#94): `[x, y, width, height, focused, over]`, canvas CSS pixels, then 1
/// or 0 for its focus and for the cursor over it (#108).
#[wasm_bindgen]
pub fn text_field() -> Option<Vec<f32>> {
    let flag = |b: bool| f32::from(u8::from(b));
    engine::text_field()
        .map(|([x, y, w, h], focused, over)| vec![x, y, w, h, flag(focused), flag(over)])
}

/// The `logs` op: one ZIP archive (stored, not compressed) of the game's
/// log file (`xray_player.log`) and the diagnostic log's trace
/// (`performance-log.json`), each when there is one, as `{zip_base64}`
/// (the worker hands the page bytes, `zip`). Fails when there is neither.
async fn logs() -> Result<serde_json::Value, JsValue> {
    use volya_engine::formats::zip;
    let game_log = game::read_log_file().await?;
    let trace = match diagnostics::status().await["entries"].as_bool() {
        Some(true) => diagnostics::trace(false).await?["text"]
            .as_str()
            .map(str::to_owned),
        _ => None,
    };
    let mut entries = Vec::new();
    if let Some(bytes) = &game_log {
        entries.push(zip::Entry {
            name: game::LOG_FILE,
            bytes,
        });
    }
    if let Some(text) = &trace {
        entries.push(zip::Entry {
            name: "performance-log.json",
            bytes: text.as_bytes(),
        });
    }
    if entries.is_empty() {
        return Err(JsValue::from_str("no logs here"));
    }
    // The browser's local time.
    let now = js_sys::Date::new_0();
    let modified = zip::Modified {
        year: now.get_full_year() as u16,
        month: (now.get_month() + 1) as u8,
        day: now.get_date() as u8,
        hour: now.get_hours() as u8,
        minute: now.get_minutes() as u8,
        second: now.get_seconds() as u8,
    };
    let bytes = zip::store(&entries, modified);
    Ok(serde_json::json!({ "zip_base64": gpu::Base64Value(&bytes) }))
}

/// Engine interface for the page, tests and debugging: runs `op` with JSON
/// `args` and returns its JSON result.
#[wasm_bindgen]
pub async fn call(op: String, args: String) -> Result<String, JsValue> {
    let args: serde_json::Value = serde_json::from_str(&args).unwrap_or_default();
    let text = |key: &str| args.get(key).and_then(|v| v.as_str()).unwrap_or_default();
    match op.as_str() {
        "status" => to_json(&status()),
        "vfs_selftest" => vfs_selftest().await.map(|()| "null".into()),
        "script_selftest" => script_selftest(),
        "physics_selftest" => to_json(
            &volya_engine::core::physics::ode::fall_check().map_err(|e| JsValue::from_str(&e))?,
        ),
        // The physics comparison's scenarios (docs/testing.md): `{}` lists
        // them; `{index, native_base64}` runs that one and holds its output
        // to the native build's (`{words, difference}`, null when
        // identical).
        #[cfg(feature = "test-ops")]
        "physics_scenarios" => to_json(&volya_engine::core::physics::ode::harness::scenarios()),
        #[cfg(feature = "test-ops")]
        "physics_scenario" => to_json(&physics_scenario(
            args.get("index")
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX),
            text("native_base64"),
        )?),
        "game_status" => to_json(&game::status().await?),
        "unmount_game" => {
            game::unmount();
            Ok("null".into())
        }
        "forget_game" => game::forget().await.map(|()| "null".into()),
        "user_data_status" => {
            to_json(&serde_json::json!({ "present": game::has_user_data().await? }))
        }
        "forget_user_data" => game::forget_user_data().await.map(|()| "null".into()),
        "read_dir" => to_json(&game::read_dir(text("path"))?),
        "cover" => to_json(&game::cover()?),
        "play_sound" => {
            let number = |key: &str, default: f64| {
                args.get(key).and_then(|v| v.as_f64()).unwrap_or(default) as f32
            };
            let playback = Playback {
                looping: args.get("loop").and_then(|v| v.as_bool()).unwrap_or(false),
                gain: number("gain", 1.0),
                pan: number("pan", 0.0),
                category: category(args.get("category"))?,
            };
            to_json(&game::play_sound(text("path"), playback)?)
        }
        "play_music" => to_json(&game::play_music(text("track"))?),
        "stop_music" => {
            audio::stop_music();
            Ok("null".into())
        }
        "set_volume" => {
            let volume = args.get("volume").and_then(|v| v.as_f64());
            let volume = volume.ok_or_else(|| JsValue::from_str("`volume` must be a number"))?;
            audio::set_volume(category(args.get("category"))?, volume as f32)?;
            Ok("null".into())
        }
        "stop_sounds" => {
            audio::stop_all();
            Ok("null".into())
        }
        "debug_draw" => to_json(&gpu::debug_draw(args.get("path").and_then(|v| v.as_str()))?),
        "debug_capture" => {
            let size = |key| args.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            Ok(gpu::debug_capture(size("width"), size("height"))
                .await?
                .to_json())
        }
        "read_head" => {
            let max = args.get("max").and_then(|v| v.as_u64()).unwrap_or(4096);
            to_json(&game::read_head(text("path"), max)?)
        }
        "input_state" => to_json(&input::state()),
        "level_stats" => to_json(&level_stats()),
        // `{stored_only}` (tests): only what is in storage now.
        "diagnostic_log" => {
            let stored_only = args.get("stored_only").and_then(|v| v.as_bool());
            to_json(&diagnostics::trace(stored_only.unwrap_or(false)).await?)
        }
        "diagnostic_log_status" => to_json(&diagnostics::status().await),
        "game_log_status" => {
            to_json(&serde_json::json!({ "present": game::has_log_file().await? }))
        }
        "logs" => to_json(&logs().await?),
        // `{on, session, previous_end}`: the player's setting, what the
        // page tells of the session and how an earlier session's page
        // ended (diagnostics.js).
        "diagnostics_set" => {
            let on = args.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
            let session = args.get("session").cloned().unwrap_or_default();
            let previous = args.get("previous_end").cloned().unwrap_or_default();
            to_json(&diagnostics::set(on, &session, &previous).await)
        }
        #[cfg(feature = "test-ops")]
        "debug_diagnostics_bench" => {
            let frames = args
                .get("frames")
                .and_then(|v| v.as_u64())
                .unwrap_or(10_000);
            to_json(&diagnostics::bench(frames as u32))
        }
        #[cfg(feature = "test-ops")]
        "gl_selftest" => to_json(&gl_selftest()?),
        // `{scenes_base64}`: the Direct3D 9 probes' `scenes.bin` replayed,
        // each case `{probe, case, width, height}` with `rgba_base64` or
        // `error`.
        #[cfg(feature = "test-ops")]
        "d3d9probes_replay" => to_json(&d3d9probes_replay(text("scenes_base64"))?),
        // `{check}`: runs that render check (`{note}` or `{skip}`); `{}`
        // lists them.
        #[cfg(feature = "test-ops")]
        "render_selftest" => {
            to_json(&gpu::render_selftest(args.get("check").and_then(|v| v.as_str())).await?)
        }
        // `{suite}` lists that browser suite's render checks on game data;
        // `{check, frames, user_config, references}` runs one, with the
        // frames it keeps when `frames` (`{note, frames, files}` or
        // `{skip}`), starting from `user_config` (bytes) as the user
        // configuration file where given, reading `references` (file names
        // to base64 bytes) as the suite's files.
        #[cfg(feature = "test-ops")]
        "render_game_selftest" => {
            let frames = args
                .get("frames")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            to_json(
                &gpu::render_game_selftest(
                    args.get("suite").and_then(|v| v.as_str()),
                    args.get("check").and_then(|v| v.as_str()),
                    frames,
                    args.get("user_config").and_then(|v| v.as_array()).map(|a| {
                        a.iter()
                            .filter_map(|b| b.as_u64().map(|b| b as u8))
                            .collect()
                    }),
                    args.get("references")
                        .and_then(|v| v.as_object())
                        .map(|m| {
                            m.iter()
                                .filter_map(|(name, v)| {
                                    Some((name.clone(), from_base64(v.as_str()?)?))
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                )
                .await?,
            )
        }
        #[cfg(feature = "test-ops")]
        "debug_session_event" => {
            diagnostics::repeat_session_event().await;
            to_json(&serde_json::Value::Null)
        }
        #[cfg(feature = "test-ops")]
        "debug_diagnostics_fill" => {
            let events = args.get("events").and_then(|v| v.as_u64()).unwrap_or(0);
            to_json(&diagnostics::fill(events as u32)?)
        }
        "debug_transcode" => to_json(&textures::bench(&args)?),
        "ui_images" => to_json(&engine::ui_images(&args)?),
        // `{"on": true}` starts logging each frame's loading caption and
        // wasm heap; `{"on": false}` stops and returns the log.
        "heap_log" => {
            let on = args.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
            let log = HEAP_LOG.with(|l| std::mem::replace(&mut *l.borrow_mut(), on.then(Vec::new)));
            to_json(&log.unwrap_or_default())
        }
        // Where the live bytes sit (#432): live KiB per MiB of the heap and
        // the live blocks of 256 KiB or more (address MiB, KiB).
        #[cfg(feature = "test-ops")]
        "heap_map" => {
            let (per_mib, large) = counting::map((heap_bytes() as usize).div_ceil(1 << 20));
            let live = counting::live();
            to_json(&serde_json::json!({
                "wasm_heap_bytes": heap_bytes(),
                "live_bytes": live,
                "live_kib_per_mib": per_mib,
                "large": large,
            }))
        }
        // The most live since `{"reset": true}`, and by what (#440).
        #[cfg(feature = "test-ops")]
        "heap_record" => {
            if args.get("reset").and_then(|v| v.as_bool()).unwrap_or(false) {
                counting::reset_record();
            }
            let (live, sizes, large) = counting::record();
            let (growths, growth_count) = counting::growths();
            to_json(&serde_json::json!({
                "live_bytes": live,
                "sizes_kib": sizes,
                "large": large,
                "growths": growths,
                "growth_count": growth_count,
            }))
        }
        "launch" => to_json(&engine::launch().await?),
        "menu_state" => to_json(&engine::state()),
        // The latest load's `* phase cmem` by caption and its end's
        // figures; *h*, *t* and the live bytes asked for now
        // (docs/architecture/memory.md).
        "load_memory" => {
            let now = memory_statistics();
            to_json(&serde_json::json!({
                "load": engine::load_memory(),
                "now": {
                    "crt_heap": now.crt_heap,
                    "game_lua": now.game_lua,
                    "render": now.render,
                    "sound": audio::host_stats().audio_in_use_bytes,
                    "textures": now.textures,
                    "live_bytes": counting::live(),
                },
            }))
        }
        "bound_keys" => to_json(&engine::bound_keys()),
        #[cfg(feature = "test-ops")]
        "debug_lose_device" => gpu::lose_device().map(|()| "null".into()),
        // `{suffix}`: the archive tree hides the paths ending in `suffix`
        // (none without one).
        #[cfg(feature = "test-ops")]
        "debug_hide_file" => {
            game::hidden::set(args.get("suffix").and_then(|v| v.as_str()));
            Ok("null".into())
        }
        #[cfg(feature = "test-ops")]
        "debug_give_item" => to_json(&engine::give_item(text("section"))?),
        // `{type, power, bone, armour_piercing}`: a hit on the player
        // character (player-hits.md 2): `type` a hit type's name, `bone` a
        // bone of its model (none when absent).
        #[cfg(feature = "test-ops")]
        "debug_hit" => {
            let number = |key: &str| args.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let bone = args.get("bone").and_then(|v| v.as_str());
            engine::hit(
                text("type"),
                number("power"),
                bone,
                number("armour_piercing"),
            )
            .map(|()| "null".into())
        }
        // `{wear}`: an outfit with night vision put on, or taken away.
        #[cfg(feature = "test-ops")]
        "debug_nightvision_outfit" => {
            let wear = args.get("wear").and_then(|v| v.as_bool()).unwrap_or(true);
            to_json(&engine::nightvision_outfit(wear)?)
        }
        // `{name, position: [x, y, z], category, looped, two_d, delay}`;
        // `names` for several, each started by the same update.
        #[cfg(feature = "test-ops")]
        "debug_play_sound" => {
            let flag = |key: &str| args.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
            let position = args
                .get("position")
                .and_then(|v| serde_json::from_value::<[f32; 3]>(v.clone()).ok())
                .unwrap_or_default();
            let flags = volya_engine::core::sound::system::PlayFlags {
                looped: flag("looped"),
                two_d: flag("two_d"),
                delay: args.get("delay").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            };
            let category = category(args.get("category"))?;
            let names: Vec<&str> = match args.get("names").and_then(|v| v.as_array()) {
                Some(names) => names.iter().filter_map(|n| n.as_str()).collect(),
                None => vec![text("name")],
            };
            game::with_trees(|tree, _| {
                names.iter().try_for_each(|name| {
                    audio::debug_play_sound(tree, name, category, position, flags)
                })
            })??;
            Ok("null".into())
        }
        #[cfg(feature = "test-ops")]
        "sound_state" => to_json(&audio::sound_state()),
        // `{case}`: particle batches drawn (tests).
        #[cfg(feature = "test-ops")]
        "debug_draw_particles" => to_json(&gpu::debug_draw_particles(text("case"))?),
        // `{case}`: a light pass or a glow drawn (tests).
        #[cfg(feature = "test-ops")]
        "debug_draw_lights" => to_json(&gpu::debug_draw_lights(text("case"))?),
        #[cfg(feature = "test-ops")]
        "particles_state" => to_json(&engine::particles_state()?),
        // The zones and the weather's ambient effect (tests).
        #[cfg(feature = "test-ops")]
        "zones_state" => to_json(&engine::zones_state()?),
        // `{frames}`: the placed effects' update cost in wasm.
        #[cfg(feature = "test-ops")]
        "debug_particles_bench" => {
            let frames = args.get("frames").and_then(|v| v.as_u64()).unwrap_or(600);
            to_json(&engine::particles_bench(frames as u32)?)
        }
        // `{count}`: the collision world's ray cost in wasm (#396).
        #[cfg(feature = "test-ops")]
        "debug_rays_bench" => {
            let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(2000);
            to_json(&engine::rays_bench(count as u32)?)
        }
        // `{"name": …}` starts capturing that sound's emitter; `{}` stops
        // and returns the capture (`audio::capture::stop`).
        // `{"width": …, "height": …}`: the game draws at that size from now
        // on (the frame's cost at a size, #355).
        #[cfg(feature = "test-ops")]
        "debug_video_mode" => {
            let side = |key: &str| args.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let mode = Resolution::new(side("width").max(1), side("height").max(1));
            engine::set_video_mode(mode).map(|()| "null".into())
        }
        // `{"on": …}`: WebGL 2's calls counted by name from now, or the
        // counts so far with the frames presented (#429).
        #[cfg(feature = "test-ops")]
        "debug_gl_calls" => {
            let on = args.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
            to_json(&gpu::gl_calls(on)?)
        }
        // `{"supersample": …, "details": …}`: `r__supersample` applied as
        // a video restart would, and whether the detail objects are drawn
        // (a frame's cost with each, #415).
        #[cfg(feature = "test-ops")]
        "debug_render" => {
            if let Some(n) = args.get("supersample").and_then(|v| v.as_u64()) {
                engine::set_supersample(n as u32)?;
            }
            if let Some(on) = args.get("details").and_then(|v| v.as_bool()) {
                gpu::set_details_drawn(on);
            }
            Ok("null".into())
        }
        // `{"ms": …}`: every frame takes at least that long from now on.
        // `{"on": …}`: the mixer's commands wait while on (`audio::hold`).
        #[cfg(feature = "test-ops")]
        "debug_audio_hold" => {
            audio::hold(args.get("on").and_then(|v| v.as_bool()).unwrap_or(false));
            Ok("null".into())
        }
        #[cfg(feature = "test-ops")]
        "debug_frame_cost" => {
            let ms = args.get("ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
            FRAME_COST.with(|c| c.set(ms));
            Ok("null".into())
        }
        #[cfg(feature = "test-ops")]
        "audio_capture" => match args.get("name").and_then(|v| v.as_str()) {
            Some(name) => {
                audio::capture::start(name);
                Ok("null".into())
            }
            None => to_json(&audio::capture::stop()),
        },
        "menu_scene" => {
            let size = |key| args.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            to_json(&engine::scene(Resolution::new(
                size("width").max(1),
                size("height").max(1),
            ))?)
        }
        "user_config" => {
            let bytes = game::read_user_config().await?;
            to_json(&bytes.map(|b| volya_engine::formats::cp1251::decode(&b)))
        }
        "forget_user_config" => game::forget_user_config().await.map(|()| "null".into()),
        "quit_game" => {
            engine::stop("quit", "left the game");
            Ok("null".into())
        }
        "input_capture" => {
            input::set_capture(args.get("on").and_then(|v| v.as_bool()).unwrap_or(false));
            Ok("null".into())
        }
        _ => Err(JsValue::from_str(&format!("unknown op {op:?}"))),
    }
}

/// The volume category named by `value` (`"effects"`, the default, or
/// `"music"`).
fn category(value: Option<&serde_json::Value>) -> Result<Category, JsValue> {
    match value.map(|v| v.as_str()) {
        None | Some(Some("effects")) => Ok(Category::Effects),
        Some(Some("music")) => Ok(Category::Music),
        _ => Err(JsValue::from_str(&format!(
            "unknown sound category {}",
            value.map(|v| v.to_string()).unwrap_or_default()
        ))),
    }
}

fn to_json(value: &impl Serialize) -> Result<String, JsValue> {
    serde_json::to_string(value).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[derive(Serialize)]
struct Status {
    #[serde(flatten)]
    engine: volya_engine::test_api::Status,
    /// The size the game draws at while it runs (#49).
    #[serde(skip_serializing_if = "Option::is_none")]
    render: Option<[u32; 2]>,
    /// `worker` when running in a worker, as the architecture requires.
    thread: &'static str,
    /// Frames the worker's loop has run.
    frames: u64,
    gpu: gpu::Status,
}

/// Frames the worker's loop has run.
pub fn frames() -> u64 {
    FRAME_STATS.with(|s| s.borrow().frames)
}

fn status() -> Status {
    Status {
        engine: volya_engine::test_api::status(),
        render: engine::render_size().map(|r| [r.width, r.height]),
        thread: if js_sys::global().is_instance_of::<WorkerGlobalScope>() {
            "worker"
        } else {
            "main"
        },
        frames: frames(),
        gpu: gpu::status(),
    }
}

/// Runs the script runtime's shared self-test, then a chunk in a new state;
/// returns that chunk's result, the runtime's `_VERSION`.
/// Runs the GL layer's self-test on a WebGL 2 context of its own (#469).
#[cfg(feature = "test-ops")]
fn gl_selftest() -> Result<String, JsValue> {
    use wasm_bindgen::JsCast;
    let canvas = web_sys::OffscreenCanvas::new(4, 4)?;
    let context = canvas
        .get_context("webgl2")?
        .ok_or_else(|| JsValue::from_str("no WebGL 2"))?
        .dyn_into::<web_sys::WebGl2RenderingContext>()?;
    volya_engine::render::gl::selftest(context).map_err(|e| JsValue::from_str(&e))
}

/// Replays the Direct3D 9 probes' scenes (base64 `scenes`) on a WebGL 2
/// context of their own (#588).
#[cfg(feature = "test-ops")]
fn d3d9probes_replay(scenes: &str) -> Result<serde_json::Value, JsValue> {
    use wasm_bindgen::JsCast;
    let bytes = from_base64(scenes).ok_or_else(|| JsValue::from_str("scenes: not base64"))?;
    let canvas = web_sys::OffscreenCanvas::new(1, 1)?;
    let context = canvas
        .get_context("webgl2")?
        .ok_or_else(|| JsValue::from_str("no WebGL 2"))?
        .dyn_into::<web_sys::WebGl2RenderingContext>()?;
    let gl = volya_engine::render::gl::Gl::new(context);
    let frames =
        volya_engine::render::d3d9probes::replay(&gl, &bytes).map_err(|e| JsValue::from_str(&e))?;
    Ok(serde_json::Value::Array(
        frames
            .iter()
            .map(|f| {
                let mut v = serde_json::json!({
                    "probe": f.probe,
                    "case": f.case,
                    "width": f.width,
                    "height": f.height,
                });
                match &f.rgba {
                    Ok(rgba) => v["rgba_base64"] = serde_json::json!(gpu::Base64Value(rgba)),
                    Err(e) => v["error"] = serde_json::json!(e),
                }
                v
            })
            .collect(),
    ))
}

/// The physics comparison's scenario `index` run here against the native
/// build's output, `native` (base64 of its little-endian words).
#[cfg(feature = "test-ops")]
fn physics_scenario(index: u64, native: &str) -> Result<serde_json::Value, JsValue> {
    use volya_engine::core::physics::ode::harness;
    let count = harness::scenarios().len();
    let index = usize::try_from(index)
        .ok()
        .filter(|&i| i < count)
        .ok_or_else(|| JsValue::from_str(&format!("index: not below {count}")))?;
    let native: Vec<u32> = from_base64(native)
        .filter(|b| b.len().is_multiple_of(4))
        .ok_or_else(|| JsValue::from_str("native: not base64 of words"))?
        .chunks_exact(4)
        .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
        .collect();
    let ours = harness::run(index);
    Ok(serde_json::json!({
        "words": ours.len(),        "difference": harness::difference(&native, &ours),
    }))
}

/// The bytes of standard base64 `text` (padded), or `None`.
#[cfg(feature = "test-ops")]
fn from_base64(text: &str) -> Option<Vec<u8>> {
    let digit = |c: u8| -> Option<u32> {
        Some(u32::from(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        }))
    };
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for quad in text.chunks(4) {
        let pad = quad.iter().rev().take_while(|&&c| c == b'=').count().min(2);
        let mut n = 0;
        for &c in &quad[..4 - pad] {
            n = n << 6 | digit(c)?;
        }
        n <<= 6 * pad;
        out.extend_from_slice(&n.to_be_bytes()[1..4 - pad]);
    }
    Some(out)
}

fn script_selftest() -> Result<String, JsValue> {
    use volya_engine::script::{Lua, selftest};
    selftest::check().map_err(|e| JsValue::from_str(&e))?;
    let error = |e: volya_engine::script::Error| JsValue::from_str(&e.message);
    let version = Lua::new()
        .map_err(error)?
        .exec("=version", b"return _VERSION")
        .map_err(error)?;
    to_json(&version.first().and_then(|v| v.as_str()))
}

/// Runs the shared VFS conformance check against [`opfs::OpfsVfs`] on a
/// scratch directory inside the engine's OPFS namespace.
async fn vfs_selftest() -> Result<(), JsValue> {
    let namespace = opfs::namespace().await?;
    let name = format!("selftest-{}", js_sys::Date::now() as u64);
    let result = async {
        let dir = opfs::create_dir(&namespace, &name).await?;
        for (path, contents) in conformance::FILES {
            opfs::write_file(&dir, path, contents).await?;
        }
        opfs::write_file(&dir, conformance::BIG_PATH, &conformance::big_contents()).await?;
        let vfs = opfs::OpfsVfs::mount(&dir).await?;
        conformance::check(&vfs).map_err(|e| JsValue::from_str(&e))
        // `vfs` drops here, releasing its file locks before cleanup.
    }
    .await;
    opfs::remove(&namespace, &name).await?;
    result
}
