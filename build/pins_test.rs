// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Versions pinned twice agree: the wasm-bindgen CLI in MODULE.bazel and
//! the wasm-bindgen crate in Cargo.lock (they must match exactly).

use std::fs;
use std::path::PathBuf;

fn read(name: &str) -> String {
    let root = PathBuf::from(std::env::var_os("RUNFILES_DIR").expect("RUNFILES_DIR"));
    fs::read_to_string(root.join("_main").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn wasm_bindgen_cli_matches_the_crate() {
    let lock = read("Cargo.lock");
    let mut lines = lock.lines();
    lines
        .find(|l| *l == "name = \"wasm-bindgen\"")
        .expect("wasm-bindgen in Cargo.lock");
    let locked = lines
        .next()
        .and_then(|l| l.strip_prefix("version = \""))
        .map(|v| v.trim_end_matches('"'))
        .expect("its version");
    let module = read("MODULE.bazel");
    let spec = module
        .split("package = \"wasm-bindgen-cli\"")
        .nth(1)
        .expect("the wasm-bindgen-cli spec in MODULE.bazel");
    let pinned = spec
        .split("version = \"=")
        .nth(1)
        .and_then(|v| v.split('"').next())
        .expect("its version");
    assert_eq!(
        pinned, locked,
        "MODULE.bazel's wasm-bindgen-cli against Cargo.lock"
    );
}
