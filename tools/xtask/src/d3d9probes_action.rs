// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Direct3D 9 probes' actions (build/d3d9probes.bzl): xtask's
//! `d3d9probes` and `wine` modules alone.
//!
//! - `build OUT SRC...`: `d3d9probes.exe` compiled with MinGW in the Wine
//!   image, into `OUT`.
//! - `run EXE OUT`: the probes run under Wine, `OUT/results/` and
//!   `OUT/log.txt`.
//! - `zip OUT FILE...`: the files in a zip, the Windows runner.

#![allow(dead_code)] // The modules' other tasks are xtask's.

mod common;
mod d3d9probes;
mod machine;
mod wine;

use std::path::Path;
use std::process::ExitCode;

pub use common::{Result, cmd, output, root, run, runfile, step};

fn main() -> ExitCode {
    common::start();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        [wine::REAPER, container, stop_service, stop_builder] => {
            wine::reap(container, stop_service, stop_builder)
        }
        ["build", out, sources @ ..] if !sources.is_empty() => {
            d3d9probes::build(Path::new(out), sources)
        }
        ["run", exe, out] => d3d9probes::run_probes(Path::new(exe), Path::new(out)),
        ["zip", out, files @ ..] if !files.is_empty() => d3d9probes::zip(Path::new(out), files),
        _ => {
            Err("usage: d3d9probes-action build OUT SRC... | run EXE OUT | zip OUT FILE...".into())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("d3d9probes-action: {error}");
            ExitCode::FAILURE
        }
    }
}
