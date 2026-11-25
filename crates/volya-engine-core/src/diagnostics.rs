// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The diagnostic log (#346; docs/architecture.md, Diagnostic log):
//! technical only (timings, counts, states, errors with their messages,
//! level and file names; no personal data), kept by size, downloaded as a trace in Chrome's Trace
//! Event Format (the JSON Object Format: `{"traceEvents": [...]}`), which
//! Perfetto and `chrome://tracing` open.
//!
//! Code records through the `tracing` crate, on targets starting with
//! `volya` (every crate of ours); [`TraceLayer`] records them:
//!
//! - A span is timed each time it is entered and exited. Per name, its
//!   mean, maximum and count over each period become series of the
//!   period's `time ms` counter event (`"<name> mean"`, `max`, `n`). A root span (one without a parent, e.g. `frame`)
//!   lasting at least [`long_ms`] is written as a complete event (`X`) with
//!   the spans that ran inside it.
//! - [`metric`] adds a value (`tracing` fields `metric`, `value`); per name
//!   its minimum, mean and maximum over each period become series of the
//!   period's `metrics` counter event.
//! - [`count`] adds to a count (`count`, `n`): its period's sum, a series
//!   of `metrics` too.
//! - Any other event is an instant event (`i`), named by its message, its
//!   fields as arguments; at most [`EVENTS_PER_NAME`] per name per period.
//! - [`end`] records how the engine or a game ended (an [`END`] event, never
//!   limited); the platform then stores the period at once
//!   ([`Diagnostics::take_ended`]), before the context can go away.
//!
//! The platform feeds its own events in through [`Diagnostics`] (the page's
//! JS, the audio worklet), ends each period with [`Diagnostics::summarize`]
//! and appends what it [takes](Diagnostics::take_pending) to a [`Log`] in
//! one write. Each event is one self-contained JSON line (its session's
//! `pid`, the version in `cat`), so dropping the oldest keeps the rest a
//! valid trace.

use std::fmt::{self, Write as _};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// Segments of the log.
pub const SEGMENTS: usize = 8;
/// A segment's size limit, header included.
pub const SEGMENT_BYTES: u64 = 256 * 1024;
/// The log's size limit: 2 MiB.
pub const CAP_BYTES: u64 = SEGMENTS as u64 * SEGMENT_BYTES;
/// A string argument's length limit (characters); longer ones are cut.
/// Error messages are kept whole (the engine's are far shorter): this only
/// keeps a line well inside a segment.
pub const MAX_TEXT_CHARS: usize = 16 * 1024;
/// The instant event recording how the engine or a game ended: `reason`
/// (a short kind: `fatal`, `load_failed`, `quit`, `panic`, `device_lost`,
/// `page_hidden`, `page_closed`, …) and `message`. A session whose last
/// event isn't one ended abruptly.
pub const END: &str = "engine.end";
/// Instant events kept per name per period; the rest are counted.
pub const EVENTS_PER_NAME: u32 = 20;
/// Spans written inside one long root span at most.
const MAX_CHILDREN: usize = 256;
/// Events waiting for the platform kept at most (oldest dropped).
const MAX_PENDING_BYTES: usize = CAP_BYTES as usize;

/// Whether layers record (on by default). Off, their callsites are
/// disabled: a span or event then costs `tracing`'s check of a cached
/// interest, and nothing is recorded.
static RECORDING: AtomicBool = AtomicBool::new(true);

/// Turns recording by every [`TraceLayer`] on or off (the player's setting).
pub fn set_recording(on: bool) {
    if RECORDING.swap(on, Ordering::Relaxed) != on {
        tracing::callsite::rebuild_interest_cache();
    }
}

/// Whether layers record.
pub fn recording() -> bool {
    RECORDING.load(Ordering::Relaxed)
}

/// The duration from which a root span named `name` is written whole: 50
/// ms for a frame (three frames at 60 Hz), 10 ms for anything else (e.g.
/// mixing an audio block, which must keep ahead of the output).
pub fn long_ms(name: &str) -> f64 {
    if name == "frame" { 50.0 } else { 10.0 }
}

/// Adds `value` to the metric `name` (units in the name, e.g.
/// `frame.interval_ms`): its period's minimum, mean and maximum are logged.
pub fn metric(name: &str, value: f64) {
    tracing::info!(target: "volya", metric = name, value);
}

/// Adds `n` to the count `name` (e.g. `frame.late`): its period's sum is
/// logged.
pub fn count(name: &str, n: u64) {
    tracing::info!(target: "volya", count = name, n);
}

/// Records how the engine or a game ended: an [`END`] event with `reason`
/// (a short kind, e.g. `fatal`) and `message`. The platform stores the
/// period at once after it ([`Diagnostics::take_ended`]).
pub fn end(reason: &str, message: &str) {
    tracing::warn!(target: "volya", end_reason = reason, end_message = message);
}

/// An argument of an event recorded by the platform.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
    Bool(bool),
}

#[derive(Default)]
struct Stat {
    name: Box<str>,
    count: u64,
    min: f64,
    max: f64,
    sum: f64,
}

impl Stat {
    fn add(&mut self, count: u64, min: f64, sum: f64, max: f64) {
        if self.count == 0 {
            (self.min, self.max) = (min, max);
        } else {
            self.min = self.min.min(min);
            self.max = self.max.max(max);
        }
        self.count += count;
        self.sum += sum;
    }
}

fn stat<'a>(stats: &'a mut Vec<Stat>, name: &str) -> &'a mut Stat {
    match stats.iter().position(|s| &*s.name == name) {
        Some(i) => &mut stats[i],
        None => {
            stats.push(Stat {
                name: name.into(),
                ..Stat::default()
            });
            stats.last_mut().unwrap_or_else(|| unreachable!())
        }
    }
}

struct State {
    /// Microseconds since the Unix epoch.
    clock: fn() -> f64,
    /// The events' `cat`: `volya,v:<version>`.
    cat: String,
    pid: u32,
    /// This session's metadata events, written at the start of every
    /// segment so a trimmed log still names its processes and threads.
    metadata: String,
    pending: String,
    spans: Vec<Stat>,
    metrics: Vec<Stat>,
    counts: Vec<Stat>,
    /// Instant events per name this period, and those over the limit.
    instants: Vec<Stat>,
    /// Complete events of spans inside the open root span.
    children: Vec<String>,
    /// An [`END`] event was recorded since [`Diagnostics::take_ended`].
    ended: bool,
    /// [`END`] events waiting, written after the rest of their batch.
    ends: String,
}

/// Writes `text` as a JSON string, cut to [`MAX_TEXT_CHARS`].
fn json_string(out: &mut String, text: &str) {
    out.push('"');
    for (i, c) in text.chars().enumerate() {
        if i == MAX_TEXT_CHARS {
            out.push('…');
            break;
        }
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A JSON number in few characters (JSON has no NaN or infinity: 0).
fn json_number(out: &mut String, v: f64) {
    let v = if v.is_finite() { v } else { 0.0 };
    if v == v.trunc() && v.abs() < 1e15 {
        let _ = write!(out, "{}", v as i64);
    } else {
        let text = format!("{v:.3}");
        out.push_str(text.trim_end_matches('0').trim_end_matches('.'));
    }
}

fn json_args(out: &mut String, args: &[(&str, Value)]) {
    out.push('{');
    for (i, (key, value)) in args.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json_string(out, key);
        out.push(':');
        match value {
            Value::Text(t) => json_string(out, t),
            Value::Number(n) => json_number(out, *n),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        }
    }
    out.push('}');
}

/// Where an event is: its session's `cat` and `pid`, its thread.
struct At<'a> {
    cat: &'a str,
    pid: u32,
    tid: u32,
}

/// One event line: `ph`, `name`, `ts` (µs), the duration for `X`, the
/// session and thread, the arguments.
fn event_line(
    at: At<'_>,
    ph: char,
    name: &str,
    ts: f64,
    dur: Option<f64>,
    args: &[(&str, Value)],
) -> String {
    let mut out = String::with_capacity(128);
    let _ = write!(out, "{{\"ph\":\"{ph}\",\"name\":");
    json_string(&mut out, name);
    out.push_str(",\"cat\":");
    json_string(&mut out, at.cat);
    out.push_str(",\"ts\":");
    json_number(&mut out, ts.round());
    if let Some(dur) = dur {
        out.push_str(",\"dur\":");
        json_number(&mut out, dur.round());
    }
    let _ = write!(out, ",\"pid\":{},\"tid\":{}", at.pid, at.tid);
    if ph == 'i' {
        out.push_str(",\"s\":\"t\"");
    }
    if !args.is_empty() {
        out.push_str(",\"args\":");
        json_args(&mut out, args);
    }
    out.push('}');
    out
}

impl State {
    fn now(&self) -> f64 {
        (self.clock)()
    }

    /// One event line of this session: `ph`, `name`, `ts` (µs), the
    /// duration for `X`, the thread, the arguments.
    fn line(
        &self,
        ph: char,
        name: &str,
        ts: f64,
        dur: Option<f64>,
        tid: u32,
        args: &[(&str, Value)],
    ) -> String {
        let at = At {
            cat: &self.cat,
            pid: self.pid,
            tid,
        };
        event_line(at, ph, name, ts, dur, args)
    }

    fn push(&mut self, line: &str) {
        self.pending.push_str(line);
        self.pending.push('\n');
        if self.pending.len() > MAX_PENDING_BYTES {
            let cut = self.pending.len() - MAX_PENDING_BYTES;
            let cut = self.pending[cut..].find('\n').map_or(0, |i| cut + i + 1);
            self.pending.drain(..cut);
        }
    }

    fn instant(&mut self, name: &str, tid: u32, args: &[(&str, Value)]) {
        let line = self.line('i', name, self.now(), None, tid, args);
        if name == END {
            // Last in its batch, after the period's counters: a session's
            // last event is its end.
            self.ended = true;
            self.ends.push_str(&line);
            self.ends.push('\n');
            return;
        }
        let s = stat(&mut self.instants, name);
        s.count += 1;
        if s.count > u64::from(EVENTS_PER_NAME) {
            return;
        }
        self.push(&line);
    }

    fn span(&mut self, name: &str, start: f64, dur: f64, root: bool) {
        let ms = dur / 1000.0;
        stat(&mut self.spans, name).add(1, ms, ms, ms);
        let line = self.line('X', name, start, Some(dur), 1, &[]);
        if !root {
            if self.children.len() < MAX_CHILDREN {
                self.children.push(line);
            }
            return;
        }
        let children = std::mem::take(&mut self.children);
        if ms >= long_ms(name) {
            self.push(&line);
            for child in &children {
                self.push(child);
            }
        }
        self.children = children;
        self.children.clear();
    }
}

/// The diagnostic recorder of one session: shared by its [`TraceLayer`] and
/// the platform.
#[derive(Clone)]
pub struct Diagnostics(Arc<Mutex<State>>);

impl Diagnostics {
    /// A session numbered `pid` (distinct per session: sessions show as
    /// processes) of `version`, its time from `clock` (microseconds since
    /// the Unix epoch). `process` names it.
    pub fn new(clock: fn() -> f64, version: &str, pid: u32, process: &str) -> Self {
        let state = State {
            clock,
            cat: String::new(),
            pid,
            metadata: String::new(),
            pending: String::new(),
            spans: Vec::new(),
            metrics: Vec::new(),
            counts: Vec::new(),
            instants: Vec::new(),
            children: Vec::new(),
            ended: false,
            ends: String::new(),
        };
        // The version's last word: its commit (e.g. `b0fa5b8`).
        let mut cat = String::from("volya,v:");
        let short = version.split_whitespace().last().unwrap_or("");
        cat.extend(short.chars().map(|c| match c {
            ',' | '"' | '\\' => '_',
            c if c.is_whitespace() => '_',
            c => c,
        }));
        let d = Self(Arc::new(Mutex::new(State { cat, ..state })));
        d.metadata(
            "process_name",
            0,
            &[("name", Value::Text(process.to_owned()))],
        );
        d
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The layer recording `tracing`'s spans and events into this.
    pub fn layer(&self) -> TraceLayer {
        TraceLayer(self.clone())
    }

    /// Now, microseconds since the Unix epoch.
    pub fn now_us(&self) -> f64 {
        self.state().now()
    }

    /// A metadata event (`M`, e.g. `thread_name`) for thread `tid` (0: the
    /// process): kept at the start of every segment too.
    pub fn metadata(&self, name: &str, tid: u32, args: &[(&str, Value)]) {
        let mut s = self.state();
        let line = s.line('M', name, 0.0, None, tid, args);
        s.metadata.push_str(&line);
        s.metadata.push('\n');
        s.push(&line);
    }

    /// An instant event on thread `tid` (1: the engine, 2: the page, 3: the
    /// audio worklet).
    pub fn instant(&self, name: &str, tid: u32, args: &[(&str, Value)]) {
        self.state().instant(name, tid, args);
    }

    /// Records how the engine or a game ended (an [`END`] event on thread
    /// `tid`: `reason`, `message`, then `more`), as [`end`] does. Safe in a
    /// panic hook: skipped, rather than waiting, while the recorder is
    /// busy (the panic came from inside it).
    pub fn end(&self, tid: u32, reason: &str, message: &str, more: &[(&str, Value)]) {
        let mut s = match self.0.try_lock() {
            Ok(s) => s,
            Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        let mut args = vec![
            ("reason", Value::Text(reason.to_owned())),
            ("message", Value::Text(message.to_owned())),
        ];
        args.extend(more.iter().cloned());
        s.instant(END, tid, &args);
    }

    /// Whether an [`END`] event was recorded since the last call: the
    /// platform then stores the period at once.
    pub fn take_ended(&self) -> bool {
        std::mem::take(&mut self.state().ended)
    }

    /// This session's number (its events' `pid`).
    pub fn pid(&self) -> u32 {
        self.state().pid
    }

    /// This session's events' `cat`.
    pub fn cat(&self) -> String {
        self.state().cat.clone()
    }

    /// A counter event (`C`) now: each argument a series.
    pub fn counter(&self, name: &str, tid: u32, values: &[(&str, f64)]) {
        let args: Vec<(&str, Value)> = values.iter().map(|&(k, v)| (k, Value::Number(v))).collect();
        let mut s = self.state();
        let line = s.line('C', name, s.now(), None, tid, &args);
        s.push(&line);
    }

    /// Adds `count` values of the metric `name` aggregated elsewhere: their
    /// minimum, sum and maximum.
    pub fn metrics(&self, name: &str, count: u64, min: f64, sum: f64, max: f64) {
        if count > 0 && min.is_finite() && sum.is_finite() && max.is_finite() {
            stat(&mut self.state().metrics, name).add(count, min, sum, max);
        }
    }

    /// Adds `n` to the count `name`.
    pub fn count(&self, name: &str, n: u64) {
        stat(&mut self.state().counts, name).add(1, 0.0, n as f64, 0.0);
    }

    /// Ends the period: a counter event `time ms` with each span's mean,
    /// max and n (`"<span> mean"`, …), one `metrics` with each metric's min,
    /// mean and max and each count's sum, and one instant event for
    /// instants over the limit.
    pub fn summarize(&self) {
        let mut s = self.state();
        let now = s.now();
        let mut lines = Vec::new();
        // Two counter events a period, each argument a series (Perfetto
        // makes a track of each): the spans' times, then the metrics and
        // counts. One event's overhead instead of one per series.
        let mut times: Vec<(String, Value)> = Vec::new();
        for st in s.spans.iter().filter(|st| st.count > 0) {
            times.push((
                format!("{} mean", st.name),
                Value::Number(st.sum / st.count as f64),
            ));
            times.push((format!("{} max", st.name), Value::Number(st.max)));
            times.push((format!("{} n", st.name), Value::Number(st.count as f64)));
        }
        let mut values: Vec<(String, Value)> = Vec::new();
        for st in s.metrics.iter().filter(|st| st.count > 0) {
            values.push((format!("{} min", st.name), Value::Number(st.min)));
            values.push((
                format!("{} mean", st.name),
                Value::Number(st.sum / st.count as f64),
            ));
            values.push((format!("{} max", st.name), Value::Number(st.max)));
        }
        for st in &s.counts {
            values.push((st.name.to_string(), Value::Number(st.sum)));
        }
        for (name, args) in [("time ms", times), ("metrics", values)] {
            if !args.is_empty() {
                let args: Vec<(&str, Value)> =
                    args.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                lines.push(s.line('C', name, now, None, 1, &args));
            }
        }
        let over: Vec<(&str, Value)> = s
            .instants
            .iter()
            .filter(|st| st.count > u64::from(EVENTS_PER_NAME))
            .map(|st| {
                (
                    &*st.name,
                    Value::Number((st.count - u64::from(EVENTS_PER_NAME)) as f64),
                )
            })
            .collect();
        if !over.is_empty() {
            lines.push(s.line('i', "events over the limit", now, None, 1, &over));
        }
        for line in &lines {
            s.push(line);
        }
        // Counts once seen go on being logged (0), so a counter track
        // doesn't hold an old value.
        for st in &mut s.counts {
            (st.count, st.sum) = (0, 0.0);
        }
        s.spans.clear();
        s.metrics.clear();
        s.instants.clear();
    }

    /// The events recorded since the last call, one per line; [`END`]
    /// events last.
    pub fn take_pending(&self) -> String {
        let mut s = self.state();
        let mut pending = std::mem::take(&mut s.pending);
        pending.push_str(&std::mem::take(&mut s.ends));
        pending
    }

    /// This session's metadata events, one per line.
    pub fn session_metadata(&self) -> String {
        self.state().metadata.clone()
    }
}

/// When a span was last entered (µs).
struct Entered(f64);

/// The `tracing` layer recording into a [`Diagnostics`]; on a registry
/// (`tracing_subscriber::registry()`), for our targets only.
pub struct TraceLayer(Diagnostics);

fn ours(metadata: &Metadata<'_>) -> bool {
    recording() && metadata.target().starts_with("volya")
}

#[derive(Default)]
struct Fields {
    message: Option<String>,
    end_reason: Option<String>,
    end_message: Option<String>,
    metric: Option<String>,
    value: Option<f64>,
    count: Option<String>,
    n: Option<u64>,
    args: Vec<(&'static str, Value)>,
}

impl Visit for Fields {
    fn record_f64(&mut self, field: &Field, value: f64) {
        match field.name() {
            "value" => self.value = Some(value),
            name => self.args.push((name, Value::Number(value))),
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_f64(field, value as f64);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "n" => self.n = Some(value),
            _ => self.record_f64(field, value as f64),
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.args.push((field.name(), Value::Bool(value)));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "metric" => self.metric = Some(value.to_owned()),
            "count" => self.count = Some(value.to_owned()),
            "message" => self.message = Some(value.to_owned()),
            "end_reason" => self.end_reason = Some(value.to_owned()),
            "end_message" => self.end_message = Some(value.to_owned()),
            name => self.args.push((name, Value::Text(value.to_owned()))),
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }
}

impl<S> Layer<S> for TraceLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        if ours(metadata) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn enabled(&self, metadata: &Metadata<'_>, _: Context<'_, S>) -> bool {
        ours(metadata)
    }

    fn on_new_span(&self, _: &Attributes<'_>, _: &Id, _: Context<'_, S>) {}

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let now = self.0.now_us();
            span.extensions_mut().replace(Entered(now));
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let Some(start) = span.extensions().get::<Entered>().map(|e| e.0) else {
            return;
        };
        let mut s = self.0.state();
        let dur = (s.now() - start).max(0.0);
        s.span(span.name(), start, dur, span.parent().is_none());
    }

    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let (Some(name), Some(value)) = (&fields.metric, fields.value) {
            self.0.metrics(name, 1, value, value, value);
        } else if let (Some(name), Some(n)) = (&fields.count, fields.n) {
            self.0.count(name, n);
        } else if let Some(reason) = &fields.end_reason {
            let message = fields.end_message.as_deref().unwrap_or_default();
            let mut args = vec![
                ("reason", Value::Text(reason.clone())),
                ("message", Value::Text(message.to_owned())),
            ];
            args.extend(fields.args);
            self.0.state().instant(END, 1, &args);
        } else {
            let name = fields
                .message
                .as_deref()
                .unwrap_or_else(|| event.metadata().name());
            self.0.instant(name, 1, &fields.args);
        }
    }
}

/// Where a [`Log`] keeps its segments: [`SEGMENTS`] byte files, each
/// readable and writable at any offset.
pub trait SegmentStore {
    fn len(&mut self, segment: usize) -> io::Result<u64>;
    /// Reads into `buf` from `at`; the bytes read.
    fn read_at(&mut self, segment: usize, at: u64, buf: &mut [u8]) -> io::Result<usize>;
    fn write_at(&mut self, segment: usize, at: u64, bytes: &[u8]) -> io::Result<()>;
    fn truncate(&mut self, segment: usize, len: u64) -> io::Result<()>;
    /// Makes the writes so far durable.
    fn flush(&mut self) -> io::Result<()>;
}

/// Segments in memory (where storage can't be had, and tests).
#[derive(Clone, Default)]
pub struct MemoryStore(pub Vec<Vec<u8>>);

impl MemoryStore {
    fn segment(&mut self, segment: usize) -> &mut Vec<u8> {
        if self.0.len() <= segment {
            self.0.resize(segment + 1, Vec::new());
        }
        &mut self.0[segment]
    }
}

impl SegmentStore for MemoryStore {
    fn len(&mut self, segment: usize) -> io::Result<u64> {
        Ok(self.segment(segment).len() as u64)
    }

    fn read_at(&mut self, segment: usize, at: u64, buf: &mut [u8]) -> io::Result<usize> {
        let data = self.segment(segment);
        let from = (at as usize).min(data.len());
        let n = buf.len().min(data.len() - from);
        buf[..n].copy_from_slice(&data[from..from + n]);
        Ok(n)
    }

    fn write_at(&mut self, segment: usize, at: u64, bytes: &[u8]) -> io::Result<()> {
        let data = self.segment(segment);
        let end = at as usize + bytes.len();
        if data.len() < end {
            data.resize(end, 0);
        }
        data[at as usize..end].copy_from_slice(bytes);
        Ok(())
    }

    fn truncate(&mut self, segment: usize, len: u64) -> io::Result<()> {
        self.segment(segment).truncate(len as usize);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// An [`END`] event the page kept in its own storage as its session's
/// context went (a closed page, whose engine worker may be ended before it
/// stores its own), for the next session to add ([`Log::add_recorded_end`]).
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedEnd {
    pub pid: u32,
    pub cat: String,
    /// Microseconds since the Unix epoch.
    pub ts: f64,
    pub tid: u32,
    pub reason: String,
    pub message: String,
}

/// A segment's first line, followed by its sequence number.
const HEADER: &str = "volya-engine diagnostic log segment ";

fn header(sequence: u64) -> String {
    format!("{HEADER}{sequence}\n")
}

/// The log kept by size over a [`SegmentStore`]: event lines are appended
/// to the newest segment; when it would pass [`SEGMENT_BYTES`], the oldest
/// is emptied and becomes the newest, so the oldest events go first and the
/// log never passes [`CAP_BYTES`]. A segment starts with a header line
/// holding its sequence number, so the order survives a reload; a segment
/// whose header doesn't read (damaged, or written by others: the origin is
/// shared) counts as empty, and a line cut by an interrupted write is
/// dropped.
pub struct Log<S> {
    store: S,
    /// Each segment's sequence number, 0 for an empty one.
    sequence: [u64; SEGMENTS],
    /// Each segment's length.
    len: [u64; SEGMENTS],
    newest: usize,
}

impl<S: SegmentStore> Log<S> {
    /// Opens the log kept in `store`.
    pub fn open(mut store: S) -> io::Result<Self> {
        let mut sequence = [0; SEGMENTS];
        let mut len = [0; SEGMENTS];
        for s in 0..SEGMENTS {
            let mut head = [0u8; 64];
            let n = store.read_at(s, 0, &mut head)?;
            let parsed = std::str::from_utf8(&head[..n])
                .ok()
                .and_then(|t| t.strip_prefix(HEADER))
                .and_then(|t| t.split_once('\n'))
                .and_then(|(seq, _)| seq.parse::<u64>().ok())
                .filter(|&seq| seq > 0);
            let size = store.len(s)?;
            match parsed {
                Some(seq) if size <= SEGMENT_BYTES => {
                    sequence[s] = seq;
                    len[s] = size;
                }
                _ if size > 0 => store.truncate(s, 0)?,
                _ => {}
            }
        }
        let newest = (0..SEGMENTS).max_by_key(|&s| sequence[s]).unwrap_or(0);
        let mut log = Self {
            store,
            sequence,
            len,
            newest,
        };
        // An interrupted write: the newest ends at its last whole line.
        if log.sequence[newest] > 0 {
            let mut data = vec![0; log.len[newest] as usize];
            let n = log.store.read_at(newest, 0, &mut data)?;
            let whole = data[..n]
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |i| i + 1);
            if (whole as u64) < log.len[newest] {
                log.store.truncate(newest, whole as u64)?;
                log.len[newest] = whole as u64;
            }
        }
        Ok(log)
    }

    /// The store, for reopening.
    pub fn into_store(self) -> S {
        self.store
    }

    /// The log's size in bytes, headers included.
    pub fn bytes(&self) -> u64 {
        self.len.iter().sum()
    }

    /// Starts a new newest segment in the oldest one, with `prefix`.
    fn rotate(&mut self, prefix: &str) -> io::Result<()> {
        let next = self.sequence[self.newest] + 1;
        let oldest = (0..SEGMENTS).min_by_key(|&s| self.sequence[s]).unwrap_or(0);
        let mut head = header(next);
        if (head.len() + prefix.len()) as u64 <= SEGMENT_BYTES / 2 {
            head.push_str(prefix);
        }
        self.store.truncate(oldest, 0)?;
        self.store.write_at(oldest, 0, head.as_bytes())?;
        self.sequence[oldest] = next;
        self.len[oldest] = head.len() as u64;
        self.newest = oldest;
        Ok(())
    }

    /// Appends `text`, whole lines, then flushes the store. A segment begun
    /// by a rotation starts with `prefix` (whole lines: the session's
    /// metadata events, so a trimmed log still names its sessions).
    pub fn append(&mut self, text: &str, prefix: &str) -> io::Result<()> {
        // The first segment: the text starts with the metadata itself.
        if self.sequence[self.newest] == 0 {
            self.rotate("")?;
        }
        let mut fresh = false;
        let mut rest = text;
        while !rest.is_empty() {
            let room = SEGMENT_BYTES.saturating_sub(self.len[self.newest]) as usize;
            // The longest run of whole lines that fits.
            let take = if rest.len() <= room {
                rest.len()
            } else {
                rest[..room].rfind('\n').map_or(0, |i| i + 1)
            };
            if take == 0 {
                if fresh {
                    // A line longer than a segment (never, from
                    // [`Diagnostics`]): dropped.
                    let line = rest.find('\n').map_or(rest.len(), |i| i + 1);
                    rest = &rest[line..];
                } else {
                    self.rotate(prefix)?;
                    fresh = true;
                }
                continue;
            }
            let (chunk, tail) = rest.split_at(take);
            self.store
                .write_at(self.newest, self.len[self.newest], chunk.as_bytes())?;
            self.len[self.newest] += take as u64;
            rest = tail;
            fresh = false;
        }
        self.store.flush()
    }

    /// Adds `end`, recorded by the page when its session's context went,
    /// unless the session stored anything from then on: its own [`END`]
    /// event for the same cause (recorded after the page's, the message
    /// taking its time), or later events (a hidden page shown again). A
    /// segment begun by a rotation starts with `prefix`, as [`Log::append`].
    /// Whether it was added.
    pub fn add_recorded_end(&mut self, end: &RecordedEnd, prefix: &str) -> io::Result<bool> {
        let pid = format!(",\"pid\":{},", end.pid);
        let lines = self.lines()?;
        if let Some(last) = lines.iter().rev().find(|l| l.contains(&pid)) {
            let ts = last
                .split_once(",\"ts\":")
                .and_then(|(_, rest)| rest.split(',').next())
                .and_then(|ts| ts.parse::<f64>().ok())
                .unwrap_or(0.0);
            if ts >= end.ts {
                return Ok(false);
            }
        }
        let at = At {
            cat: &end.cat,
            pid: end.pid,
            tid: end.tid,
        };
        let args = [
            ("reason", Value::Text(end.reason.clone())),
            ("message", Value::Text(end.message.clone())),
            ("recorded_by", Value::Text("page".into())),
        ];
        let mut line = event_line(at, 'i', END, end.ts, None, &args);
        line.push('\n');
        self.append(&line, prefix)?;
        Ok(true)
    }

    /// The log's event lines, oldest first.
    pub fn lines(&mut self) -> io::Result<Vec<String>> {
        let mut order: Vec<usize> = (0..SEGMENTS).filter(|&s| self.sequence[s] > 0).collect();
        order.sort_by_key(|&s| self.sequence[s]);
        let mut out = Vec::new();
        for s in order {
            let mut data = vec![0; self.len[s] as usize];
            let n = self.store.read_at(s, 0, &mut data)?;
            data.truncate(n);
            let text = String::from_utf8_lossy(&data);
            out.extend(
                text.lines()
                    .skip(1)
                    .filter(|l| l.starts_with('{') && l.ends_with('}'))
                    .map(str::to_owned),
            );
        }
        Ok(out)
    }

    /// The log as a trace in the JSON Object Format, with `extra` events
    /// (lines, e.g. not yet stored) after it and `other_data` (a JSON
    /// object) as its `otherData`.
    pub fn trace(&mut self, extra: &str, other_data: &str) -> io::Result<String> {
        let lines = self.lines()?;
        let extra = extra
            .lines()
            .filter(|l| l.starts_with('{') && l.ends_with('}'));
        let mut out =
            String::with_capacity(self.bytes() as usize + extra.clone().count() * 128 + 256);
        out.push_str("{\"traceEvents\":[\n");
        for (i, line) in lines.iter().map(String::as_str).chain(extra).enumerate() {
            if i > 0 {
                out.push_str(",\n");
            }
            out.push_str(line);
        }
        out.push_str("\n],\"displayTimeUnit\":\"ms\",\"otherData\":");
        out.push_str(other_data);
        out.push_str("}\n");
        Ok(out)
    }
}

#[cfg(test)]
mod tests;
