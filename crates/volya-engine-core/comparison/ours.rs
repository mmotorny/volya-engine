// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `ode_harness_ours DIR`: every scenario of tests/odeharness against our
//! build, natively (the Mac's arm64 build), written into DIR as
//! `<scenario>.bin`, the release's `ours\` (docs/testing.md, The physics
//! library against the shipped one).

use std::process::ExitCode;

#[cfg(target_arch = "wasm32")]
fn main() -> ExitCode {
    // The scenarios' driver is linked natively only.
    ExitCode::FAILURE
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ExitCode {
    use std::path::PathBuf;

    let Some(dir) = std::env::args_os().nth(1) else {
        eprintln!("usage: ode_harness_ours DIR");
        return ExitCode::FAILURE;
    };
    match volya_engine_core::physics::ode::harness::write_all(&PathBuf::from(dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ode_harness_ours: {e}");
            ExitCode::FAILURE
        }
    }
}
