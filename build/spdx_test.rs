// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Every source file carries SPDX headers (AGENTS.md): the files of every
//! package (`//:all_files`), found in this test's runfiles.

use std::fs;
use std::path::{Path, PathBuf};

/// Third-party sources kept as released, under their own licences.
const VENDORED: &str = "/vendor/";

/// File names that must carry SPDX headers, by suffix.
const SUFFIXES: &[&str] = &[
    ".rs",
    ".c",
    ".h",
    ".S",
    ".lua",
    ".sh",
    ".js",
    ".mjs",
    ".html",
    ".css",
    ".svg",
    ".toml",
    ".wgsl",
    ".swift",
    ".pbxproj",
    ".xcscheme",
    ".bazel",
    ".bzl",
    ".bazelrc",
];

fn has_spdx_headers(text: &str) -> bool {
    let head: Vec<&str> = text.lines().take(5).collect();
    head.iter().any(|l| l.contains("SPDX-FileCopyrightText: "))
        && head
            .iter()
            .any(|l| l.contains("SPDX-License-Identifier: BSD-3-Clause"))
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, files);
        } else {
            files.push(path);
        }
    }
}

#[test]
fn every_source_file_has_spdx_headers() {
    let root = PathBuf::from(std::env::var_os("RUNFILES_DIR").expect("RUNFILES_DIR")).join("_main");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let checked: Vec<String> = files
        .iter()
        .map(|f| {
            f.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|f| !format!("/{f}").contains(VENDORED))
        .filter(|f| SUFFIXES.iter().any(|s| f.ends_with(s)))
        // This test's own binary and the runfiles' bookkeeping.
        .filter(|f| !f.starts_with("build/spdx"))
        .collect();
    assert!(
        checked.iter().any(|f| f == "Cargo.toml") && checked.len() > 100,
        "the runfiles hold the tree: {} files",
        checked.len()
    );
    let missing: Vec<&String> = checked
        .iter()
        .filter(|f| {
            fs::read_to_string(root.join(f))
                .map(|text| !has_spdx_headers(&text))
                .unwrap_or(false)
        })
        .collect();
    assert!(missing.is_empty(), "missing SPDX headers: {missing:#?}");
}

#[test]
fn spdx_headers() {
    let ok = "// SPDX-FileCopyrightText: 2026 A\n// SPDX-License-Identifier: BSD-3-Clause\n";
    assert!(has_spdx_headers(ok));
    assert!(has_spdx_headers(&format!("#!/bin/sh\n{ok}")));
    assert!(!has_spdx_headers(
        "// SPDX-License-Identifier: BSD-3-Clause\n"
    ));
    assert!(!has_spdx_headers(
        "// SPDX-FileCopyrightText: 2026 A\n// SPDX-License-Identifier: MIT\n"
    ));
    assert!(!has_spdx_headers(&format!("\n\n\n\n\n{ok}")));
}
