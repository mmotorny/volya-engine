// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `cargo xtask parity` (#478): the test app built with the wgpu renderer
//! and with the GL one (the render crate's `gl` feature), and the `parity`
//! suite run on each in headless Chrome, one after the other, on the same
//! origin and kept profile: the first run keeps its views' frames, the
//! second compares its own with them pixel by pixel (`render-game.js`) and
//! reports the pixels that differ (docs/architecture/rendering.md, The
//! parity test).

use std::fs;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use volya_engine_devserver::{BASE, Server};

use super::{Profile, Result, root, step, web_into};
use crate::e2e;

const SUITE: &str = "parity";
const TIMEOUT: Duration = Duration::from_secs(1500);

/// Runs the parity test against `args`' renderer: the GL one by default,
/// `wgpu` to compare the wgpu build with itself (a check of the test).
pub fn run(args: &[&str]) -> Result {
    let (name, features): (&str, &[&str]) = match args {
        [] | ["gl"] => ("gl", &["gl"]),
        ["wgpu"] => ("wgpu", &[]),
        _ => return Err(format!("unknown arguments {args:?}; expected gl or wgpu")),
    };
    crate::machine::wait_memory()?;
    let dir = root().join("target/parity");
    let first = dir.join("build-wgpu");
    let second = dir.join(format!("build-{name}-compared"));
    step("Parity: the wgpu build");
    web_into(Profile::Test, &[], &first)?;
    step(&format!("Parity: the {name} build"));
    web_into(Profile::Test, features, &second)?;
    let frames_first = dir.join("frames-wgpu");
    let frames_second = dir.join(format!("frames-{name}"));
    for frames in [&frames_first, &frames_second] {
        if frames.exists() {
            fs::remove_dir_all(frames).map_err(|e| format!("{}: {e}", frames.display()))?;
        }
    }

    let (tx, reports) = mpsc::channel();
    let server = Server::new(&first)
        .and_then(|s| s.with_next(&second))
        .and_then(|s| {
            s.with_tests(
                &root().join("tests/browser"),
                Arc::new(move |report| {
                    let _ = tx.send(report);
                }),
            )
        })
        .and_then(|s| match std::env::var_os("VOLYA_GOG_INSTALLER") {
            Some(dir) => s.with_installer(std::path::Path::new(&dir)),
            None => Ok(s),
        })
        .map_err(|e| format!("dev server: {e}"))?;
    let server = Arc::new(server);
    let listener = e2e::listen()?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let running = Arc::clone(&server);
    thread::spawn(move || running.run_shared(listener));

    let suite = |label: &str| -> Result<String> {
        step(&format!("Parity suite on the {label} build"));
        let id = e2e::run_id("chrome", SUITE);
        let url = format!("http://localhost:{port}{BASE}__test/run/{id}/{SUITE}.html");
        let report = e2e::chrome(&url, &id, true, &reports, TIMEOUT)?;
        println!("{report}");
        Ok(report)
    };
    let io = |e: std::io::Error| format!("dev server: {e}");
    server.set_dumps(Some(&frames_first)).map_err(io)?;
    let before = suite("wgpu")?;
    if !e2e::passed(&before) {
        return Err("the wgpu build's parity suite failed".into());
    }
    server.set_dumps(Some(&frames_second)).map_err(io)?;
    server.set_references(Some(&frames_first)).map_err(io)?;
    server.set_next(true);
    let after = suite(name)?;
    if e2e::passed(&after) {
        step(&format!("The {name} build draws the wgpu build's views"));
        Ok(())
    } else {
        Err(format!(
            "the {name} build's views differ from the wgpu build's (frames in {} and {})",
            frames_first.display(),
            frames_second.display()
        ))
    }
}
