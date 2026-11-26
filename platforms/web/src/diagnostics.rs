// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The diagnostic log on the web (#346; `volya_engine::core::diagnostics`,
//! docs/architecture/diagnostics.md). Off by default; the player turns
//! it on in Settings. While on, the session's recorder is the global
//! `tracing` subscriber and its events are appended once per period to the
//! log kept in OPFS (`volya-engine/diagnostic-log/0` … `7`, one sync access
//! handle each, held while on). Where those files can't be opened (another
//! copy of the app holds them), the log is kept in memory for the session.
//! While off, recording's callsites are disabled and nothing runs per frame
//! beyond `tracing`'s check of them; the log is kept.
//!
//! Every way the engine or a game ends records an `engine.end` event and
//! stores the period at once ([`end`], and after a frame whose engine
//! recorded one), before the context can go away.

use std::cell::{Cell, RefCell};

use volya_engine::core::diagnostics::{
    self, Diagnostics, Log, MemoryStore, RecordedEnd, SEGMENTS, SegmentStore, Value,
};
use wasm_bindgen::prelude::*;
use web_sys::{FileSystemReadWriteOptions, FileSystemSyncAccessHandle, WorkerGlobalScope};

/// The log's OPFS directory, in the engine's namespace.
const DIRECTORY: &str = "diagnostic-log";
/// The period while the game runs, ms.
const PERIOD_MS: f64 = 1000.0;
/// The period otherwise (the app's own screens), ms.
const IDLE_PERIOD_MS: f64 = 10_000.0;

enum Store {
    Opfs(Vec<FileSystemSyncAccessHandle>),
    Memory(MemoryStore),
}

impl Drop for Store {
    fn drop(&mut self) {
        if let Store::Opfs(handles) = self {
            for h in handles {
                h.close();
            }
        }
    }
}

fn io(e: JsValue) -> std::io::Error {
    std::io::Error::other(format!("OPFS: {e:?}"))
}

fn at(offset: u64) -> FileSystemReadWriteOptions {
    let options = FileSystemReadWriteOptions::new();
    options.set_at(offset as f64);
    options
}

impl SegmentStore for Store {
    fn len(&mut self, segment: usize) -> std::io::Result<u64> {
        match self {
            Store::Opfs(h) => h[segment].get_size().map(|n| n as u64).map_err(io),
            Store::Memory(m) => m.len(segment),
        }
    }

    fn read_at(&mut self, segment: usize, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Store::Opfs(h) => h[segment]
                .read_with_u8_array_and_options(buf, &at(offset))
                .map(|n| n as usize)
                .map_err(io),
            Store::Memory(m) => m.read_at(segment, offset, buf),
        }
    }

    fn write_at(&mut self, segment: usize, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Store::Opfs(h) => h[segment]
                .write_with_u8_array_and_options(bytes, &at(offset))
                .map(drop)
                .map_err(io),
            Store::Memory(m) => m.write_at(segment, offset, bytes),
        }
    }

    fn truncate(&mut self, segment: usize, len: u64) -> std::io::Result<()> {
        match self {
            Store::Opfs(h) => h[segment].truncate_with_f64(len as f64).map_err(io),
            Store::Memory(m) => m.truncate(segment, len),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Store::Opfs(h) => h.iter().try_for_each(|h| h.flush()).map_err(io),
            Store::Memory(m) => m.flush(),
        }
    }
}

struct Session {
    diagnostics: Diagnostics,
    /// Set once the store is open while logging is on; events wait in the
    /// recorder until then.
    log: Option<Log<Store>>,
    /// Whether the log is kept in OPFS (else in memory).
    stored: bool,
    /// What the page told of the session (a JSON object).
    page: serde_json::Value,
    /// When the period began (`performance.now()`, ms).
    period_start: f64,
}

thread_local! {
    /// Made when logging is first turned on in this run.
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    /// The player's setting: logging is off by default.
    static ON: Cell<bool> = const { Cell::new(false) };
    /// Whether graphics are set up (the session event can be made).
    static STARTED: Cell<bool> = const { Cell::new(false) };
    /// Settles once the last op on the log's store is done ([`exclusive`]).
    static STORE_OPS: RefCell<Option<js_sys::Promise>> = const { RefCell::new(None) };
}

/// Runs `f` once the ops on the log's store before it are done, so one
/// can't open the store's handles while another holds them (#428): the
/// log is reported on, and read, only once its store is open.
async fn exclusive<T>(f: impl Future<Output = T>) -> T {
    let mut done = None;
    let mine = js_sys::Promise::new(&mut |resolve, _| done = Some(resolve));
    let before = STORE_OPS.with(|o| o.borrow_mut().replace(mine));
    if let Some(before) = before {
        let _ = wasm_bindgen_futures::JsFuture::from(before).await;
    }
    let result = f.await;
    if let Some(done) = done {
        let _ = done.call0(&JsValue::UNDEFINED);
    }
    result
}

/// Microseconds since the Unix epoch, at `performance.now()`'s resolution:
/// the time origin plus the worker's clock.
fn clock() -> f64 {
    js_sys::global()
        .dyn_into::<WorkerGlobalScope>()
        .ok()
        .and_then(|g| g.performance())
        .map_or_else(js_sys::Date::now, |p| p.time_origin() + p.now())
        * 1000.0
}

/// Whether logging is on.
pub fn on() -> bool {
    ON.with(Cell::get)
}

/// The `diagnostics_set` op: turns logging on or off (the player's
/// setting). On, the session's recorder starts (the first time in this run:
/// `page` is what the page tells of it, a JSON object: `version`,
/// `user_agent`, `screen`, `dpr`, `cores`) and the log's store opens, and
/// `previous_end` (the `engine.end` an earlier session's page kept as it
/// closed: `{pid, cat, ts, reason, message}`) is added to the log unless
/// that session stored its own. Off, the period so far is stored,
/// recording stops and the store closes; the log is kept. Answers the
/// log's status after ([`status`]).
pub async fn set(
    on: bool,
    page: &serde_json::Value,
    previous_end: &serde_json::Value,
) -> serde_json::Value {
    exclusive(async {
        switch(on, page, previous_end).await;
        status_now().await
    })
    .await
}

async fn switch(on: bool, page: &serde_json::Value, previous_end: &serde_json::Value) {
    if on == self::on() {
        return;
    }
    ON.with(|o| o.set(on));
    if !on {
        SESSION.with(|s| {
            if let Some(s) = s.borrow_mut().as_mut() {
                store(s);
                s.log = None;
                s.stored = false;
            }
        });
        diagnostics::set_recording(false);
        return;
    }
    diagnostics::set_recording(true);
    if SESSION.with(|s| s.borrow().is_none()) {
        begin(page);
    } else {
        tracing::info!("diagnostic log on");
    }
    if STARTED.with(Cell::get) {
        session_event().await;
    }
    let (mut log, stored) = open_log().await;
    if let Some(end) = recorded_end(previous_end) {
        let prefix = SESSION.with(|s| {
            s.borrow()
                .as_ref()
                .map(|s| s.diagnostics.session_metadata())
        });
        if let Err(e) = log.add_recorded_end(&end, &prefix.unwrap_or_default()) {
            tracing::error!(error = %e, "diagnostic log write failed");
        }
    }
    SESSION.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut()
            && self::on()
        {
            s.log = Some(log);
            s.stored = stored;
        }
    });
}

/// The `engine.end` a page kept as it closed (main.js), if `value` is one.
fn recorded_end(value: &serde_json::Value) -> Option<RecordedEnd> {
    let text = |key: &str| value.get(key)?.as_str().map(str::to_owned);
    Some(RecordedEnd {
        pid: u32::try_from(value.get("pid")?.as_u64()?).ok()?,
        cat: text("cat")?,
        ts: value.get("ts")?.as_f64()?,
        tid: 2,
        reason: text("reason")?,
        message: text("message").unwrap_or_default(),
    })
}

/// The session's recorder, as the global `tracing` subscriber.
fn begin(page: &serde_json::Value) {
    let version = page
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    // Sessions show as processes: one number each (its start, in seconds).
    let pid = (js_sys::Date::now() / 1000.0) as u32;
    let d = Diagnostics::new(clock, version, pid, &format!("Volya Engine {version}"));
    for (tid, name) in [(1, "engine worker"), (2, "page"), (3, "audio worklet")] {
        d.metadata("thread_name", tid, &[("name", Value::Text(name.into()))]);
    }
    use tracing_subscriber::layer::SubscriberExt;
    let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry().with(d.layer()));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Recorded and stored at once: the engine stops here.
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| info.to_string());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        end(1, "panic", &message, &[("location", Value::Text(location))]);
        previous(info);
    }));
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            diagnostics: d,
            log: None,
            stored: false,
            page: page.clone(),
            period_start: crate::now(),
        })
    });
}

/// The log's segments, opened for synchronous access (readwrite: one copy
/// of the app at a time).
async fn open_store() -> Result<Store, JsValue> {
    let namespace = crate::opfs::namespace().await?;
    let dir = crate::opfs::create_dir(&namespace, DIRECTORY).await?;
    // Closed by the store's drop should a later one fail.
    let mut store = Store::Opfs(Vec::with_capacity(SEGMENTS));
    for i in 0..SEGMENTS {
        let handle = crate::opfs::open_sync(&dir, &i.to_string()).await?;
        if let Store::Opfs(handles) = &mut store {
            handles.push(handle);
        }
    }
    Ok(store)
}

/// The log's segments if logging was ever on here (nothing is created);
/// `None` too while another copy of the app holds them.
async fn existing() -> Option<Store> {
    let namespace = crate::opfs::namespace().await.ok()?;
    crate::opfs::existing_dir(&namespace, DIRECTORY)
        .await
        .ok()??;
    open_store().await.ok()
}

/// The log, from OPFS where its files can be opened, else in memory; and
/// whether it is in OPFS.
async fn open_log() -> (Log<Store>, bool) {
    let problem = match open_store().await {
        Ok(store) => match Log::open(store) {
            Ok(log) => return (log, true),
            Err(e) => e.to_string(),
        },
        Err(e) => format!("{e:?}"),
    };
    tracing::warn!(error = %problem, "diagnostic log kept in memory");
    match Log::open(Store::Memory(MemoryStore::default())) {
        Ok(log) => (log, false),
        // A memory store never fails.
        Err(_) => unreachable!(),
    }
}

/// Graphics are set up: the session's event, if logging is on.
pub async fn started() {
    STARTED.with(|s| s.set(true));
    if on() {
        session_event().await;
    }
}

/// The `debug_session_event` op (tests): the session's event again, as a
/// new session would make it now, if logging is on.
#[cfg(feature = "test-ops")]
pub async fn repeat_session_event() {
    if on() {
        session_event().await;
    }
}

/// The session's event: what the page told, the engine's graphics (WebGL
/// 2's renderer string and the texture path), and the imported game (`game::identity`).
async fn session_event() {
    let game = crate::game::identity().await;
    let gpu = serde_json::to_value(crate::gpu::status()).unwrap_or_default();
    let page = SESSION.with(|s| s.borrow().as_ref().map(|s| s.page.clone()));
    let mut args = args_of(&page.unwrap_or_default());
    for key in ["state", "renderer", "textures", "message"] {
        if let Some(v) = gpu.get(key).and_then(|v| v.as_str()) {
            args.push((format!("gpu_{key}"), Value::Text(v.into())));
        }
    }
    args.extend(args_of(&game.into()));
    instant("session", 1, &args);
}

/// A JSON object's members as event arguments (other values as text).
fn args_of(value: &serde_json::Value) -> Vec<(String, Value)> {
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    map.iter()
        .map(|(k, v)| {
            let v = match v {
                serde_json::Value::String(s) => Value::Text(s.clone()),
                serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
                serde_json::Value::Bool(b) => Value::Bool(*b),
                other => Value::Text(other.to_string()),
            };
            (k.clone(), v)
        })
        .collect()
}

/// Records how the engine or a game ended (an `engine.end` event on
/// thread `tid`, 1: the engine, 2: the page, 3: the audio worklet) and
/// stores the period at once, while logging is on. Safe in a panic hook
/// (skipped where the session is in use: the panic came from inside it).
pub fn end(tid: u32, reason: &str, message: &str, more: &[(&str, Value)]) {
    if !on() {
        return;
    }
    let _ = SESSION.try_with(|s| {
        if let Ok(mut s) = s.try_borrow_mut()
            && let Some(s) = s.as_mut()
        {
            s.diagnostics.end(tid, reason, message, more);
            store(s);
        }
    });
}

/// Stores the period at once if an `engine.end` was recorded since the
/// last store (by the engine, through `tracing`).
pub fn store_if_ended() {
    let _ = SESSION.try_with(|s| {
        if let Ok(mut s) = s.try_borrow_mut()
            && let Some(s) = s.as_mut()
            && s.diagnostics.take_ended()
        {
            store(s);
        }
    });
}

/// The JS bridge: how the engine or a game ended (`engine.end`, on thread
/// `tid`), stored at once: an uncaught error or rejection in the page, the
/// engine worker or the audio worklet, the page hidden or closed.
#[wasm_bindgen]
pub fn diagnostics_end(reason: &str, message: &str, tid: u32) {
    end(tid, reason, message, &[]);
}

/// Runs `f` on the recorder while logging is on.
fn with<T>(f: impl FnOnce(&Diagnostics) -> T) -> Option<T> {
    if !on() {
        return None;
    }
    SESSION.with(|s| s.borrow().as_ref().map(|s| f(&s.diagnostics)))
}

/// An instant event `name` on thread `tid` with `args`, while logging is on.
pub fn instant(name: &str, tid: u32, args: &[(String, Value)]) {
    let args: Vec<(&str, Value)> = args.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    with(|d| d.instant(name, tid, &args));
}

/// A step of a level's GPU work about to run (#443): an instant event
/// `name` on the engine's thread with `args` and the bytes handed to the
/// GPU so far by kind, stored at once, so the last record before the page
/// dies names the step (the byte totals left out without `renderer`).
/// While logging is on.
pub fn gpu_step(
    name: &str,
    renderer: Option<&volya_engine::render::Renderer>,
    args: &[(&str, Value)],
) {
    if !on() {
        return;
    }
    let mib = |b: u64| Value::Number(b as f64 / f64::from(1 << 20));
    let mut all: Vec<(&str, Value)> = args.to_vec();
    if let Some(r) = renderer {
        all.push(("texture_MiB", mib(r.texture_bytes_written())));
        all.push(("geometry_MiB", mib(r.geometry_bytes_written())));
        all.push(("particle_buffer_MiB", mib(r.particle_buffer_size())));
        #[cfg(feature = "gl")]
        {
            let live = r.gpu_live();
            all.push(("live_texture_MiB", mib(live.texture_bytes)));
            all.push(("live_buffer_MiB", mib(live.buffer_bytes)));
        }
    }
    all.push(("heap_MiB", mib(crate::heap_bytes() as u64)));
    let _ = SESSION.try_with(|s| {
        if let Ok(mut s) = s.try_borrow_mut()
            && let Some(s) = s.as_mut()
        {
            s.diagnostics.instant(name, 1, &all);
            append_pending(s);
        }
    });
}

/// Appends the events recorded so far to the log (one write), leaving the
/// period running: unlike [`store`], no summary, so a step's record costs
/// only its own event.
fn append_pending(s: &mut Session) {
    let Some(log) = s.log.as_mut() else {
        return;
    };
    let pending = s.diagnostics.take_pending();
    if let Err(e) = log.append(&pending, &s.diagnostics.session_metadata()) {
        s.log = Log::open(Store::Memory(MemoryStore::default())).ok();
        s.stored = false;
        tracing::error!(error = %e, "diagnostic log write failed");
    }
}

/// The JS bridge: an instant event `name` on thread `tid` (2: the page, 3:
/// the audio worklet) with `args`, a JSON object.
#[wasm_bindgen]
pub fn diagnostics_event(name: &str, tid: u32, args: &str) {
    if on() {
        let args = serde_json::from_str(args).unwrap_or_default();
        instant(name, tid, &args_of(&args));
    }
}

/// The JS bridge: `count` values of the metric `name` aggregated by the
/// page (their minimum, sum and maximum).
#[wasm_bindgen]
pub fn diagnostics_metrics(name: &str, count: f64, min: f64, sum: f64, max: f64) {
    with(|d| d.metrics(name, count as u64, min, sum, max));
}

/// The JS bridge: adds `n` to the count `name`.
#[wasm_bindgen]
pub fn diagnostics_count(name: &str, n: f64) {
    with(|d| d.count(name, n.max(0.0) as u64));
}

/// The audio worker's numbers since its last report (#354), in frames at
/// `rate`: the frames the AudioWorklet played as silence for want of mixed
/// ones; the ring of mixed frames' fill each time the audio worker woke
/// (least, sum, greatest, count); and its mixing time per block, ms (the
/// same).
pub struct AudioRing {
    pub underrun: f64,
    pub fill: [f64; 4],
    pub mix: [f64; 4],
    /// Services woken by the audio worker's own timeout with the ring
    /// under half its target (#452), and the time between services (ms).
    pub missed: f64,
    pub gap: [f64; 4],
    pub rate: f64,
}

/// Records [`AudioRing`] while the log is on.
pub fn audio_ring(a: &AudioRing) {
    with(|d| {
        let ms = 1000.0 / a.rate.max(1.0);
        let n = |v: f64| v.max(0.0) as u64;
        d.count("audio.underrun_frames", n(a.underrun));
        if a.fill[3] > 0.0 {
            let [min, sum, max, count] = a.fill;
            d.metrics("audio.fill_ms", n(count), min * ms, sum * ms, max * ms);
        }
        if a.mix[3] > 0.0 {
            let [min, sum, max, count] = a.mix;
            d.metrics("audio.mix_ms", n(count), min, sum, max);
        }
        d.count("audio.missed_wakes", n(a.missed));
        if a.gap[3] > 0.0 {
            let [min, sum, max, count] = a.gap;
            d.metrics("audio.service_gap_ms", n(count), min, sum, max);
        }
    });
}

/// Ends the period and appends its events to the log (one write).
fn store(s: &mut Session) {
    s.diagnostics.take_ended();
    let heap = crate::heap_bytes() / f64::from(1 << 20);
    s.diagnostics.counter("wasm heap", 1, &[("MiB", heap)]);
    s.diagnostics.summarize();
    s.period_start = crate::now();
    let Some(log) = s.log.as_mut() else {
        // The store isn't open yet: the events wait.
        return;
    };
    let start = crate::now();
    let pending = s.diagnostics.take_pending();
    if let Err(e) = log.append(&pending, &s.diagnostics.session_metadata()) {
        // Kept in memory from here.
        s.log = Log::open(Store::Memory(MemoryStore::default())).ok();
        s.stored = false;
        tracing::error!(error = %e, "diagnostic log write failed");
    }
    diagnostics::metric("diagnostics.write_ms", crate::now() - start);
}

/// After each frame: ends the period when it is over (1 s while the game
/// runs, 10 s otherwise), or now with `force` (the page hiding).
pub fn tick(force: bool) {
    if !on() {
        return;
    }
    let period = if crate::engine::active() {
        PERIOD_MS
    } else {
        IDLE_PERIOD_MS
    };
    SESSION.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut()
            && (force || crate::now() - s.period_start >= period)
        {
            store(s);
        }
    });
    // The engine recorded how it or the game ended: stored now, not at the
    // period's end.
    store_if_ended();
}

/// The trace's `otherData`.
fn other_data(stored: bool) -> String {
    serde_json::json!({
        "downloaded": js_sys::Date::new_0().to_iso_string().as_string(),
        "cap_bytes": diagnostics::CAP_BYTES,
        "stored": stored,
        "on": on(),
    })
    .to_string()
}

fn io_error(e: std::io::Error) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// The `diagnostic_log` op: the whole log as a trace (Trace Event Format,
/// JSON Object Format), `{text, bytes}`, after storing the period so far
/// (unless `stored_only`, for tests: what is in storage now); while logging
/// is off, the log kept from before.
pub async fn trace(stored_only: bool) -> Result<serde_json::Value, JsValue> {
    exclusive(trace_now(stored_only)).await
}

async fn trace_now(stored_only: bool) -> Result<serde_json::Value, JsValue> {
    let open = SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut()?;
        s.log.as_ref()?;
        let pending = if stored_only {
            String::new()
        } else {
            store(s);
            s.diagnostics.take_pending()
        };
        let stored = s.stored;
        let log = s.log.as_mut()?;
        Some(
            log.trace(&pending, &other_data(stored))
                .map(|t| (t, log.bytes())),
        )
    });
    let (text, bytes) = match open {
        Some(result) => result.map_err(io_error)?,
        None => {
            let store = existing()
                .await
                .ok_or_else(|| JsValue::from_str("no diagnostic log here"))?;
            let mut log = Log::open(store).map_err(io_error)?;
            let text = log.trace("", &other_data(true)).map_err(io_error)?;
            (text, log.bytes())
        }
    };
    Ok(serde_json::json!({ "text": text, "bytes": bytes }))
}

/// The `diagnostic_log_status` op: `{on, entries, bytes, session}`:
/// whether logging is on, whether there is a log to download (stored, or
/// waiting to be), the stored log's size and, once logging was on in this
/// run, the session's `{pid, cat}` (for the page's record of how it ended,
/// main.js).
pub async fn status() -> serde_json::Value {
    exclusive(status_now()).await
}

async fn status_now() -> serde_json::Value {
    let open = SESSION.with(|s| Some(s.borrow().as_ref()?.log.as_ref()?.bytes()));
    let bytes = match open {
        Some(bytes) => bytes,
        None => match existing().await {
            Some(store) => Log::open(store).map_or(0, |l| l.bytes()),
            None => 0,
        },
    };
    let waiting = on() && SESSION.with(|s| s.borrow().is_some());
    let session = SESSION.with(|s| {
        let s = s.borrow();
        let d = &s.as_ref()?.diagnostics;
        Some(serde_json::json!({ "pid": d.pid(), "cat": d.cat() }))
    });
    serde_json::json!({
        "on": on(),
        "entries": bytes > 0 || waiting,
        "bytes": bytes,
        "session": session,
    })
}

/// The `debug_diagnostics_bench` op (tests): the recording a frame makes
/// (a root span with four inside, a metric, a count), `frames` times; the
/// time per frame, microseconds, and whether logging was on.
#[cfg(feature = "test-ops")]
pub fn bench(frames: u32) -> serde_json::Value {
    let begin = crate::now();
    for i in 0..frames {
        let _frame = tracing::info_span!("bench.frame").entered();
        tracing::info_span!("bench.engine").in_scope(|| ());
        tracing::info_span!("bench.sound").in_scope(|| ());
        tracing::info_span!("bench.render").in_scope(|| ());
        tracing::info_span!("bench.textures").in_scope(|| ());
        diagnostics::metric("bench.interval_ms", 16.7);
        diagnostics::count("bench.late", u64::from(i % 100 == 0));
    }
    let total = crate::now() - begin;
    serde_json::json!({
        "frames": frames,
        "us_per_frame": total * 1000.0 / f64::from(frames.max(1)),
        "on": on(),
    })
}

/// The `debug_diagnostics_fill` op (tests): `events` numbered instant
/// events (`fill 0000000` …, about 200 bytes each) stored as they come,
/// 2,000 a period; the log's size after.
#[cfg(feature = "test-ops")]
pub fn fill(events: u32) -> Result<serde_json::Value, JsValue> {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let s = s
            .as_mut()
            .filter(|_| on())
            .ok_or_else(|| JsValue::from_str("the diagnostic log is off"))?;
        let pad = Value::Text("x".repeat(60));
        for i in 0..events {
            s.diagnostics
                .instant(&format!("fill {i:07}"), 1, &[("pad", pad.clone())]);
            if i % 2000 == 1999 {
                store(s);
            }
        }
        store(s);
        Ok(serde_json::json!({ "bytes": s.log.as_ref().map(|l| l.bytes()) }))
    })
}
