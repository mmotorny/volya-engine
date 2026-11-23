// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Project tasks: `cargo xtask <task>`.

mod e2e;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
Usage: cargo xtask <task>

Tasks:
  check       Fast static checks (SPDX, licenses, fmt, clippy). Gates every push.
  e2e [TARGET...]
              Native tests, then browser suites on TARGETs (chrome, iphone;
              default: all). On demand only.
  web [--dev] Build the web app into dist/ (release unless --dev).";

/// Crates that never build for the web target.
const NATIVE_ONLY: &[&str] = &["volya-devserver", "xtask"];

const WASM_TARGET: &str = "wasm32-unknown-unknown";

type Result<T = ()> = std::result::Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["check"] => check(),
        ["e2e", targets @ ..] => e2e::run(targets),
        ["web"] => web(Profile::Release),
        ["web", "--dev"] => web(Profile::Dev),
        ["-h" | "--help"] | [] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(format!("unknown arguments: {}\n\n{USAGE}", args.join(" "))),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("\nxtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask lives at tools/xtask")
        .to_path_buf()
}

fn step(name: &str) {
    println!("\n==> {name}");
}

fn cmd(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command.args(args).current_dir(root());
    command
}

fn run(command: &mut Command) -> Result {
    let status = command
        .status()
        .map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?} failed with {status}"))
    }
}

fn output(command: &mut Command) -> Result<String> {
    let out = command
        .output()
        .map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if !out.status.success() {
        return Err(format!(
            "{command:?} failed with {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `--exclude` arguments for crates that don't build for wasm.
fn exclude_native_only() -> Vec<&'static str> {
    NATIVE_ONLY.iter().flat_map(|c| ["--exclude", c]).collect()
}

fn check() -> Result {
    step("SPDX headers");
    check_spdx()?;
    step("Dependency licenses");
    run(&mut cmd(
        "cargo",
        &["deny", "--log-level", "error", "check"],
    ))?;
    step("Format");
    run(&mut cmd("cargo", &["fmt", "--all", "--check"]))?;
    step("Clippy");
    let lint = ["--all-targets", "--", "-D", "warnings"];
    run(cmd("cargo", &["clippy", "--workspace"]).args(lint))?;
    run(
        cmd("cargo", &["clippy", "--workspace", "--target", WASM_TARGET])
            .args(exclude_native_only())
            .args(lint),
    )?;
    step("Static checks passed");
    Ok(())
}

#[derive(Clone, Copy)]
enum Profile {
    Dev,
    Release,
}

fn web(profile: Profile) -> Result {
    step("Web build");
    let (flags, dir): (&[&str], _) = match profile {
        Profile::Dev => (&[], "debug"),
        Profile::Release => (&["--release"], "release"),
    };
    run(cmd(
        "cargo",
        &["build", "-p", "volya-engine", "--target", WASM_TARGET],
    )
    .args(flags))?;
    let dist = root().join("dist");
    if dist.exists() {
        fs::remove_dir_all(&dist).map_err(|e| format!("remove dist: {e}"))?;
    }
    copy_dir(&root().join("web"), &dist).map_err(|e| format!("copy web: {e}"))?;
    let wasm = format!("target/{WASM_TARGET}/{dir}/volya_engine.wasm");
    run(&mut cmd(
        "wasm-bindgen",
        &[
            "--target",
            "web",
            "--no-typescript",
            "--out-dir",
            "dist/pkg",
            &wasm,
        ],
    ))
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Source files that must carry SPDX headers.
const SPDX_PATTERNS: &[&str] = &[
    "*.rs", "*.sh", "*.js", "*.mjs", "*.html", "*.css", "*.svg", "*.toml",
];

fn check_spdx() -> Result {
    let files = output(
        cmd(
            "git",
            &["ls-files", "--cached", "--others", "--exclude-standard"],
        )
        .args(SPDX_PATTERNS),
    )?;
    let missing: Vec<&str> = files
        .lines()
        .filter(|file| {
            fs::read_to_string(root().join(file))
                .map(|text| !has_spdx_headers(&text))
                .unwrap_or(false) // Deleted but still tracked.
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("missing SPDX headers:\n  {}", missing.join("\n  ")))
    }
}

fn has_spdx_headers(text: &str) -> bool {
    let head: Vec<&str> = text.lines().take(5).collect();
    head.iter().any(|l| l.contains("SPDX-FileCopyrightText: "))
        && head
            .iter()
            .any(|l| l.contains("SPDX-License-Identifier: BSD-3-Clause"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
