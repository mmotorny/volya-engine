// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The capture actions' tool (build/original.bzl): xtask's `original`
//! modules alone, so an edit elsewhere in xtask doesn't capture the
//! original's frames again.
//!
//! - `capture OUT`: the frames of the views, into `OUT/fidelity/`, the
//!   folder the fidelity suite is served (`xtask original capture`).

#![allow(dead_code)] // The modules' other tasks are xtask's.

mod common;
mod machine;
mod original;
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
        [out] => original::capture(Path::new(out)),
        _ => Err("usage: capture OUT".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("capture: {error}");
            ExitCode::FAILURE
        }
    }
}
