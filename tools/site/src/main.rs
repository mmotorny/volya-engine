// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The site rule's tool (build/web.bzl):
//!
//! - `site build STATIC_DIR NOTICES WASM WASM_BINDGEN OUT`
//! - `site next DIST STATIC_DIR OUT`
//! - `site notices MANIFEST OUT` (build/notices.bzl)
//!
//! Sites are stamped with [`volya_engine_site::TEST_STAMP`]; `deploy` stamps its
//! copy with the commit.

use std::path::Path;
use std::process::ExitCode;

use volya_engine_site::{TEST_STAMP, build, next_build};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["build", static_dir, notices, wasm, wasm_bindgen, out] => build(
            Path::new(static_dir),
            Path::new(notices),
            Path::new(wasm),
            Path::new(wasm_bindgen),
            Path::new(out),
            &TEST_STAMP,
        ),
        ["next", dist, static_dir, out] => next_build(
            Path::new(dist),
            Path::new(static_dir),
            Path::new(out),
            &volya_engine_site::Stamp {
                commit: "0000000-next",
                date: TEST_STAMP.date,
            },
        ),
        ["notices", manifest, out] => {
            volya_engine_site::notices::notices(Path::new(manifest), Path::new(out))
        }
        _ => Err(format!("unknown arguments: {}", args.join(" "))),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("site: {error}");
            ExitCode::FAILURE
        }
    }
}
