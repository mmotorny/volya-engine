// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The diagnostic log on the web (#346; `volya_engine::core::diagnostics`,
//! docs/architecture.md, Diagnostic log): the session's recorder as the
//! global `tracing` subscriber, its events appended once per period to the
//! log kept in OPFS (`volya-engine/diagnostic-log/0` … `7`, one sync access
//! handle each, opened at start). Where those files can't be opened (another
//! copy of the app holds them), the log is kept in memory for the session.

use std::cell::RefCell;

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
    /// Set once the store is open; events wait in the recorder until then.
    log: Option<Log<Store>>,
    /// Whether the log is kept in OPFS (else in memory).
    stored: bool,
    /// What the page told of the session (a JSON object).
    page: serde_json::Value,
    /// When the period began (`performance.now()`, ms).
    period_start: f64,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// Microseconds since the Unix epoch, at `performance.now()`'s resolution:
/// the time origin plus the page's clock.
fn clock() -> f64 {
    js_sys::global()
        .dyn_into::<WorkerGlobalScope>()
        .ok()
        .and_then(|g| g.performance())
        .map_or_else(js_sys::Date::now, |p| p.time_origin() + p.now())
        * 1000.0
}

/// Starts the session's log, before anything else of the engine: `page` is
/// what the page tells of it (a JSON object: `version`, `user_agent`,
/// `screen`, `dpr`, `cores`). The log's store opens in the background.
#[wasm_bindgen]
pub fn diagnostics_start(page: &str) {
    if SESSION.with(|s| s.borrow().is_some()) {
        return;
    }
    let page: serde_json::Value = serde_json::from_str(page).unwrap_or_default();
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
            page,
            period_start: crate::now(),
        })
    });
    wasm_bindgen_futures::spawn_local(async {
        let (store, problem) = match open_store().await {
            Ok(handles) => (Store::Opfs(handles), None),
            Err(e) => (Store::Memory(MemoryStore::default()), Some(e)),
        };
        let stored = matches!(store, Store::Opfs(_));
        let log = Log::open(store).or_else(|e| {
            tracing::error!(error = %e, "diagnostic log unreadable");
            Log::open(Store::Memory(MemoryStore::default()))
        });
        if let Some(e) = problem {
            tracing::warn!(error = %format!("{e:?}"), "diagnostic log kept in memory");
        }
        SESSION.with(|s| {
            if let Some(s) = s.borrow_mut().as_mut() {
                s.log = log.ok();
                s.stored = stored;
            }
        });
    });
}

/// Opens the log's segments for synchronous access (readwrite: one copy of
/// the app at a time).
async fn open_store() -> Result<Vec<FileSystemSyncAccessHandle>, JsValue> {
    let namespace = crate::opfs::namespace().await?;
    let dir = crate::opfs::create_dir(&namespace, DIRECTORY).await?;
    let mut handles = Vec::with_capacity(SEGMENTS);
    for i in 0..SEGMENTS {
        match crate::opfs::open_sync(&dir, &i.to_string()).await {
            Ok(h) => handles.push(h),
            Err(e) => {
                for h in &handles {
                    h.close();
                }
                return Err(e);
            }
        }
    }
    Ok(handles)
}

/// The session's start, once graphics are set up: what the page told and
/// the engine's build, backend and texture path.
pub fn session_started(build: &str, gpu: &serde_json::Value) {
    let args = SESSION.with(|s| {
        let s = s.borrow();
        let page = s.as_ref().map(|s| s.page.clone()).unwrap_or_default();
        let mut args = args_of(&page);
        args.push(("build".into(), Value::Text(build.into())));
        for key in ["state", "backend", "textures", "message"] {
            if let Some(v) = gpu.get(key).and_then(|v| v.as_str()) {
                args.push((format!("gpu_{key}"), Value::Text(v.into())));
            }
        }
        args
    });
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

fn with<T>(f: impl FnOnce(&Diagnostics) -> T) -> Option<T> {
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
    let args = serde_json::from_str(args).unwrap_or_default();
    instant(name, tid, &args_of(&args));
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

/// Ends the period and appends its events to the log (one write).
fn store(s: &mut Session) {
    let heap = crate::heap_bytes() / f64::from(1 << 20);
    s.diagnostics.counter("wasm heap", 1, &[("MiB", heap)]);
    s.diagnostics.summarize();
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
    s.period_start = crate::now();
}

/// After each frame: ends the period when it is over (1 s while the game
/// runs, 10 s otherwise), or now with `force` (the page hiding).
pub fn tick(force: bool) {
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

/// The `diagnostic_log` op: the whole log as a trace (Trace Event Format,
/// JSON Object Format), after storing the period so far; `stored` whether
/// it is kept in OPFS.
pub fn trace() -> Result<serde_json::Value, JsValue> {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut().ok_or_else(|| JsValue::from_str("no diagnostic log"))?;
        store(s);
        let other = serde_json::json!({
            "version": s.page.get("version"),
            "downloaded": js_sys::Date::new_0().to_iso_string().as_string(),
            "cap_bytes": diagnostics::CAP_BYTES,
            "stored": s.stored,
        });
        let pending = s.diagnostics.take_pending();
        let log = s
            .log
            .as_mut()
            .ok_or_else(|| JsValue::from_str("the diagnostic log is not open yet"))?;
        let text = log
            .trace(&pending, &other.to_string())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(serde_json::json!({ "text": text, "bytes": log.bytes(), "stored": s.stored }))
    })
}

/// The `debug_diagnostics_bench` op (tests): the recording a frame makes
/// (a root span with four inside, a metric, a count), `frames` times; the
/// time per frame, microseconds.
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
    serde_json::json!({ "frames": frames, "us_per_frame": total * 1000.0 / f64::from(frames.max(1)) })
}
