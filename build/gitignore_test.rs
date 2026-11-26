// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Build output and the machine's own files are never committed: `.gitignore`
//! ignores each (dropping a line once committed 23 MB of it, #547), and
//! `deploy` refuses a tree that tracks any.

use std::fs;
use std::path::PathBuf;

/// What must stay ignored: Bazel's links, the output a build or a tool
/// leaves in the tree, and the machine's own settings.
const IGNORED: &[&str] = &[
    "/bazel-*",
    "/target",
    "/dist",
    "/dist-next",
    "/.cache",
    "/rust-project.json",
    "/.rules_rust_analyzer",
    "/user.bazelrc",
];

#[test]
fn build_output_stays_ignored() {
    let root = PathBuf::from(std::env::var_os("RUNFILES_DIR").expect("RUNFILES_DIR"));
    let text = fs::read_to_string(root.join("_main/.gitignore")).expect(".gitignore");
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let missing: Vec<&&str> = IGNORED.iter().filter(|p| !lines.contains(p)).collect();
    assert!(missing.is_empty(), ".gitignore lacks {missing:?}");
}
