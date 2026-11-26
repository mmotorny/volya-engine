// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Dependencies are permissively licensed (AGENTS.md; deny.toml): cargo-deny
//! over the workspace in this test's runfiles, with the toolchain's cargo.

use std::path::PathBuf;
use std::process::Command;

fn runfile(var: &str) -> PathBuf {
    let path = std::env::var(var).unwrap_or_else(|_| panic!("{var}"));
    let root = PathBuf::from(std::env::var_os("RUNFILES_DIR").expect("RUNFILES_DIR"));
    root.join(
        path.strip_prefix("../")
            .map_or(format!("_main/{path}"), str::to_owned),
    )
}

#[test]
fn dependencies_are_permissively_licensed() {
    let deny = runfile("CARGO_DENY");
    let cargo = runfile("CARGO");
    let workspace = runfile("WORKSPACE_MANIFEST");
    let status = Command::new(&deny)
        .args(["--log-level", "error", "--manifest-path"])
        .arg(&workspace)
        .args(["check", "licenses", "bans", "sources"])
        .env("CARGO", &cargo)
        .status()
        .unwrap_or_else(|e| panic!("{}: {e} (docs/testing.md, Setup)", deny.display()));
    assert!(status.success(), "cargo-deny: {status}");
}
