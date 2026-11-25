// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::cell::Cell;

use serde_json::Value as Json;
use tracing_subscriber::layer::SubscriberExt;

use super::*;

/// An event line of about 100 bytes, numbered.
fn entry(i: usize) -> String {
    format!(
        "{{\"ph\":\"i\",\"name\":\"test {i:08}\",\"cat\":\"volya,v:abc1234\",\"ts\":{i},\"pid\":1,\"tid\":1,\"s\":\"t\",\"args\":{{\"pad\":\"{}\"}}}}\n",
        "x".repeat(10)
    )
}

fn number(line: &str) -> usize {
    line.split("test ").nth(1).unwrap()[..8].parse().unwrap()
}

const META: &str = "{\"ph\":\"M\",\"name\":\"process_name\",\"pid\":1,\"tid\":0,\"ts\":0,\"args\":{\"name\":\"s\"}}\n";

#[test]
fn drops_the_oldest_and_holds_the_cap() {
    let mut log = Log::open(MemoryStore::default()).unwrap();
    let total = 40_000; // about 5 MiB: more than twice the cap
    for i in 0..total {
        log.append(&entry(i), META).unwrap();
        assert!(log.bytes() <= CAP_BYTES);
    }
    let lines = log.lines().unwrap();
    let events: Vec<&String> = lines.iter().filter(|l| l.contains("test ")).collect();
    assert_eq!(number(events.last().unwrap()), total - 1);
    // In order, without gaps, the oldest gone.
    let first = number(events[0]);
    assert!(first > 0);
    for (k, line) in events.iter().enumerate() {
        assert_eq!(number(line), first + k);
    }
    // More than all but one segment is kept; every segment names the session.
    let kept: usize = lines.iter().map(|l| l.len() + 1).sum();
    assert!(kept as u64 > CAP_BYTES - SEGMENT_BYTES);
    assert_eq!(
        lines.iter().filter(|l| l.contains("process_name")).count(),
        SEGMENTS
    );
}

#[test]
fn persists_and_reloads() {
    let mut log = Log::open(MemoryStore::default()).unwrap();
    for i in 0..5000 {
        log.append(&entry(i), META).unwrap();
    }
    let before = log.lines().unwrap();
    let mut log = Log::open(log.into_store()).unwrap();
    assert_eq!(log.lines().unwrap(), before);
    // Appending goes on after the reload's last event, past rotations.
    for i in 5000..30_000 {
        log.append(&entry(i), META).unwrap();
    }
    let mut log = Log::open(log.into_store()).unwrap();
    assert!(log.bytes() <= CAP_BYTES);
    let lines = log.lines().unwrap();
    let events: Vec<&String> = lines.iter().filter(|l| l.contains("test ")).collect();
    assert_eq!(number(events.last().unwrap()), 29_999);
    assert_eq!(events.len(), 30_000 - number(events[0]));
}

#[test]
fn damaged_segments_count_as_empty() {
    let mut store = MemoryStore::default();
    store.write_at(0, 0, b"someone else's file\n").unwrap();
    // A write cut short: its partial line goes.
    store
        .write_at(
            1,
            0,
            format!("{HEADER}3\n{{\"kept\":1}}\n{{\"cut").as_bytes(),
        )
        .unwrap();
    let mut log = Log::open(store).unwrap();
    assert_eq!(log.lines().unwrap(), ["{\"kept\":1}"]);
    log.append("{\"next\":2}\n", "").unwrap();
    assert_eq!(log.lines().unwrap(), ["{\"kept\":1}", "{\"next\":2}"]);
    let mut store = log.into_store();
    assert_eq!(store.len(0).unwrap(), 0);
}

thread_local! {
    static NOW: Cell<f64> = const { Cell::new(1_795_516_503_000_000.0) };
}

fn clock() -> f64 {
    NOW.with(Cell::get)
}

fn advance(us: f64) {
    NOW.with(|n| n.set(n.get() + us));
}

/// The trace assembled from `d`'s pending events.
fn trace(d: &Diagnostics) -> Json {
    let mut log = Log::open(MemoryStore::default()).unwrap();
    log.append(&d.take_pending(), &d.session_metadata())
        .unwrap();
    let text = log.trace("", "{\"version\":\"test\"}").unwrap();
    // A trace to open elsewhere (Perfetto's trace processor).
    if let Some(path) = std::env::var_os("VOLYA_DUMP_TRACE") {
        std::fs::write(path, &text).unwrap();
    }
    serde_json::from_str(&text).unwrap()
}

fn events<'a>(trace: &'a Json, ph: &str) -> Vec<&'a Json> {
    trace["traceEvents"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["ph"] == ph)
        .collect()
}

#[test]
fn records_spans_metrics_and_events() {
    let d = Diagnostics::new(clock, "2026-11-24 00:47 · b0fa5b8", 7, "Volya Engine");
    let subscriber = tracing_subscriber::registry().with(d.layer());
    tracing::subscriber::with_default(subscriber, || {
        // A short frame and a long one, each with a subsystem inside.
        for ms in [5.0, 60.0] {
            let frame = tracing::info_span!(target: "volya", "frame").entered();
            advance(1000.0);
            tracing::info_span!(target: "volya", "sound").in_scope(|| advance(ms * 1000.0));
            drop(frame);
        }
        metric("audio.fill_ms", 10.0);
        metric("audio.fill_ms", 30.0);
        count("audio.underruns", 2);
        count("audio.underruns", 1);
        for i in 0..EVENTS_PER_NAME + 3 {
            tracing::error!(target: "volya", error = %format!("bad \"thing\" {i}\n"), "fatal error");
        }
        // Not ours: not recorded.
        tracing::error!(target: "wgpu", "ignored");
    });
    d.instant(
        "AudioContext state",
        2,
        &[("state", Value::Text("running".into()))],
    );
    d.summarize();
    let t = trace(&d);
    assert_eq!(t["otherData"]["version"], "test");
    let meta = events(&t, "M");
    assert_eq!(meta[0]["name"], "process_name");
    assert_eq!(meta[0]["args"]["name"], "Volya Engine");
    // Only the long frame is written whole, with what ran inside.
    let complete = events(&t, "X");
    assert_eq!(complete.len(), 2);
    assert_eq!(complete[0]["name"], "frame");
    assert_eq!(complete[0]["dur"], 61_000);
    assert_eq!(complete[1]["name"], "sound");
    assert_eq!(complete[1]["dur"], 60_000);
    assert_eq!(complete[0]["cat"], "volya,v:2026-11-24_00:47_·_b0fa5b8");
    assert_eq!(complete[0]["pid"], 7);
    let instants = events(&t, "i");
    let fatal: Vec<_> = instants
        .iter()
        .filter(|e| e["name"] == "fatal error")
        .collect();
    assert_eq!(fatal.len(), EVENTS_PER_NAME as usize);
    assert_eq!(fatal[0]["args"]["error"], "bad \"thing\" 0\n");
    assert!(
        instants
            .iter()
            .any(|e| e["name"] == "AudioContext state" && e["tid"] == 2)
    );
    let over = instants
        .iter()
        .find(|e| e["name"] == "events over the limit")
        .unwrap();
    assert_eq!(over["args"]["fatal error"], 3);
    assert!(!instants.iter().any(|e| e["name"] == "ignored"));
    let counter = |name: &str| {
        events(&t, "C")
            .into_iter()
            .find(|e| e["name"] == name)
            .unwrap_or_else(|| panic!("no counter {name}"))["args"]
            .clone()
    };
    assert_eq!(
        counter("frame ms"),
        serde_json::json!({"mean": 33.5, "max": 61, "n": 2})
    );
    assert_eq!(
        counter("audio.fill_ms"),
        serde_json::json!({"min": 10, "mean": 20, "max": 30})
    );
    assert_eq!(counter("audio.underruns"), serde_json::json!({"count": 3}));
    // The next period: counts go on (0), the rest only with values.
    d.summarize();
    let next = d.take_pending();
    assert_eq!(next.lines().count(), 1);
    assert!(next.contains("\"audio.underruns\"") && next.contains("{\"count\":0}"));
}

#[test]
fn long_text_is_cut() {
    let d = Diagnostics::new(clock, "v", 1, "p");
    d.instant("x", 1, &[("text", Value::Text("y".repeat(5000)))]);
    let t = trace(&d);
    let text = events(&t, "i")[0]["args"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(text.chars().count(), MAX_TEXT_CHARS + 1);
}
