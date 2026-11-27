// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Direct3D 9 probes' actions (build/d3d9probes.bzl): xtask's
//! `d3d9probes` module alone.
//!
//! - `build CLANG OUT SRC...`: `d3d9probes.exe` compiled with llvm-mingw's
//!   `CLANG`, into `OUT`.
//! - `build-odeharness CLANG OUT SRC...`: `odeharness.exe`, likewise.
//! - `zip OUT FILE...`: the files in a zip, a Windows runner.
//! - `references OUT NAMES DEST=SRC...`: the comparison suite's
//!   references, real hardware's results.

#![allow(dead_code)] // The modules' other tasks are xtask's.

mod common;
mod d3d9probes;

use std::path::Path;
use std::process::ExitCode;

pub use common::{Result, cmd, output, root, run, runfile, step};

fn main() -> ExitCode {
    common::start();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["build", clang, out, sources @ ..] if !sources.is_empty() => {
            d3d9probes::build(Path::new(clang), Path::new(out), sources)
        }
        ["build-odeharness", clang, out, sources @ ..] if !sources.is_empty() => {
            d3d9probes::build_odeharness(Path::new(clang), Path::new(out), sources)
        }
        ["zip", out, files @ ..] if !files.is_empty() => d3d9probes::zip(Path::new(out), files),
        ["references", out, names, pairs @ ..] => {
            d3d9probes::references(Path::new(out), names, pairs)
        }
        _ => Err("usage: d3d9probes-action build CLANG OUT SRC... \
                  | build-odeharness CLANG OUT SRC... | zip OUT FILE... \
                  | references OUT NAMES DEST=SRC..."
            .into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("d3d9probes-action: {error}");
            ExitCode::FAILURE
        }
    }
}
