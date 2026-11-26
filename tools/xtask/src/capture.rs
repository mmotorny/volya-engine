// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The capture actions' tool (build/original.bzl): xtask's `original`
//! modules alone, so an edit elsewhere in xtask doesn't capture the
//! original's frames again.
//!
//! - `capture title|level OUT`: the frames of the views there, into
//!   `OUT/fidelity/` (`xtask original capture`).
//! - `capture merge OUT DIR...`: every `DIR/fidelity/*.png` into
//!   `OUT/fidelity/`, the folder the fidelity suite is served.

#![allow(dead_code)] // The modules' other tasks are xtask's.

mod common;
mod machine;
mod original;
mod wine;

use std::fs;
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
        ["merge", out, dirs @ ..] => merge(Path::new(out), dirs),
        [place, out] => original::capture(place, Path::new(out)),
        _ => Err("usage: capture title|level OUT | capture merge OUT DIR...".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("capture: {error}");
            ExitCode::FAILURE
        }
    }
}

fn merge(out: &Path, dirs: &[&str]) -> Result {
    let to = out.join("fidelity");
    fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
    for dir in dirs {
        let from = Path::new(dir).join("fidelity");
        for entry in fs::read_dir(&from).map_err(|e| format!("{}: {e}", from.display()))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let name = path.file_name().ok_or("no name")?;
            fs::copy(&path, to.join(name)).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}
