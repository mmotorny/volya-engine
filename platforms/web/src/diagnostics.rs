// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The diagnostic log on the web (#346; `volya_engine::core::diagnostics`,
//! docs/architecture.md, Diagnostic log). Off by default; the player turns
//! it on in Settings. While on, the session's recorder is the global
//! `tracing` subscriber and its events are appended once per period to the
//! log kept in OPFS (`volya-engine/diagnostic-log/0` … `7`, one sync access
//! handle each, held while on). Where those files can't be opened (another
//! copy of the app holds them), the log is kept in memory for the session.
//! While off, recording's callsites are disabled and nothing runs per frame
//! beyond `tracing`'s check of them; the log is kept.

use std::cell::{Cell, RefCell};

use volya_engine::core::diagnostics::{
    self, Diagnostics, Log, MemoryStore, SEGMENTS, SegmentStore, Value,
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
/// `user_agent`, `screen`, `dpr`, `cores`) and the log's store opens. Off,
/// the period so far is stored, recording stops and the store closes; the
/// log is kept.
pub async fn set(on: bool, page: &serde_json::Value) {
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
        session_event();
    }
    let (log, stored) = open_log().await;
    SESSION.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut()
            && self::on()
        {
            s.log = Some(log);
            s.stored = stored;
        }
    });
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
        tracing::error!(error = %info, "panic");
        let _ = SESSION.try_with(|s| {
            if let Ok(mut s) = s.try_borrow_mut()
                && let Some(s) = s.as_mut()
            {
                store(s);
            }
        });
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
pub fn started() {
    STARTED.with(|s| s.set(true));
    if on() {
        session_event();
    }
}

/// The session's event: what the page told and the engine's backend and
/// texture path.
fn session_event() {
    let gpu = serde_json::to_value(crate::gpu::status()).unwrap_or_default();
    let page = SESSION.with(|s| s.borrow().as_ref().map(|s| s.page.clone()));
    let mut args = args_of(&page.unwrap_or_default());
    for key in ["state", "backend", "textures", "message"] {
        if let Some(v) = gpu.get(key).and_then(|v| v.as_str()) {
            args.push((format!("gpu_{key}"), Value::Text(v.into())));
        }
    }
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

/// Runs `f` on the recorder while logging is on.
fn with<T>(f: impl FnOnce(&Diagnostics) -> T) -> Option<T> {
    if !on() {
        return None;
    }
    SESSION.with(|s| s.borrow().as_ref().map(|s| f(&s.diagnostics)))
}

fn instant(name: &str, tid: u32, args: &[(String, Value)]) {
    let args: Vec<(&str, Value)> = args.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    with(|d| d.instant(name, tid, &args));
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

/// The AudioWorklet's report with each request (#343), in frames at
/// `rate`: those played as silence for want of mixed ones since the last
/// request; how long the last reply took to arrive; the buffer's least,
/// summed and greatest fill over the `fill_n` process calls since; and
/// whether the last reply left it over-full.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen]
pub fn diagnostics_audio(
    underrun: f64,
    wait: f64,
    fill_min: f64,
    fill_sum: f64,
    fill_max: f64,
    fill_n: f64,
    overrun: f64,
    rate: f64,
) {
    with(|d| {
        let ms = 1000.0 / rate.max(1.0);
        d.count("audio.underrun_frames", underrun.max(0.0) as u64);
        d.count("audio.overruns", overrun.max(0.0) as u64);
        d.metrics("audio.reply_wait_ms", 1, wait * ms, wait * ms, wait * ms);
        d.metrics(
            "audio.fill_ms",
            fill_n.max(0.0) as u64,
            fill_min * ms,
            fill_sum * ms,
            fill_max * ms,
        );
    });
}

/// Ends the period and appends its events to the log (one write).
fn store(s: &mut Session) {
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
/// JSON Object Format), `{text, bytes}`, after storing the period so far;
/// while logging is off, the log kept from before.
pub async fn trace() -> Result<serde_json::Value, JsValue> {
    let open = SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut()?;
        s.log.as_ref()?;
        store(s);
        let pending = s.diagnostics.take_pending();
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

/// The `diagnostic_log_status` op: `{on, entries, bytes}`: whether logging
/// is on, whether there is a log to download (stored, or waiting to be)
/// and the stored log's size.
pub async fn status() -> serde_json::Value {
    let open = SESSION.with(|s| Some(s.borrow().as_ref()?.log.as_ref()?.bytes()));
    let bytes = match open {
        Some(bytes) => bytes,
        None => match existing().await {
            Some(store) => Log::open(store).map_or(0, |l| l.bytes()),
            None => 0,
        },
    };
    let waiting = on() && SESSION.with(|s| s.borrow().is_some());
    serde_json::json!({ "on": on(), "entries": bytes > 0 || waiting, "bytes": bytes })
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
