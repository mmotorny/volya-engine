// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Windows capture's actions (build/wincapture.bzl): xtask's
//! `wincapture` module alone.
//!
//! - `build CLANG OUT KIND SRC...`: the proxy `winmm.dll` (KIND `dll`) or
//!   `volyacapture.exe` (KIND `exe`) compiled with llvm-mingw into `OUT`.
//! - `plan OUT`: the driver's plan from the fidelity suite's views.

#![allow(dead_code)] // The modules' other tasks are xtask's.

mod common;
mod d3d9probes;
mod wincapture;

use std::path::Path;
use std::process::ExitCode;

pub use common::{Result, cmd, output, root, run, runfile, step};

fn main() -> ExitCode {
    common::start();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["build", clang, out, kind, sources @ ..] if !sources.is_empty() => {
            wincapture::build(Path::new(clang), Path::new(out), kind, sources)
        }
        ["plan", out] => wincapture::write_plan(Path::new(out)),
        _ => Err("usage: wincapture-action build CLANG OUT KIND SRC... | plan OUT".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("wincapture-action: {error}");
            ExitCode::FAILURE
        }
    }
}
