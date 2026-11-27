// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Operational tasks: `bazel run //tools/xtask -- <task>`.

mod bazel;
mod common;
#[allow(dead_code)] // Its actions are d3d9probes-action's.
mod d3d9probes;
mod deploy;
mod e2e;
mod frames;
mod machine;
mod parity;
mod spec_lint;
#[allow(dead_code)] // Its build and plan are wincapture-action's.
mod wincapture;

use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "\
Usage: bazel run //tools/xtask -- <task>

Builds and tests are Bazel's (docs/architecture/build.md): `bazel test //...`,
the static checks `bazel test //:checks`, a browser suite `bazel test
//tests/browser:<suite>_<target>`, the dev server `bazel run //tools/devserver`.

Tasks:
  suite TARGET SUITE
              One browser suite on one target, on the site VOLYA_SITE (and
              VOLYA_SITE_NEXT) with the suites in VOLYA_TESTS: what Bazel's
              //tests/browser:<suite>_<target> runs.
  sim STEP...  Real touches in the iOS Simulator (tools/simdriver): runs the
              steps, e.g. 'open URL' 'tap-text Play' 'screenshot out.png'.
  simshot [PATH]
              A screenshot of the test simulator (PNG; default: a new file in
              the system's temporary directory). Prints its path.
  simrec PATH SECONDS
              Records the test simulator's screen for SECONDS (H.264).
              VOLYA_SIM_MEDIA=DIR (--test_env) records every iphone suite and
              screenshots its end into DIR.
  parity [REV]
              Builds the test app at the git revision REV (default HEAD) and
              from the working tree and compares their frames of the parity
              suite's views in headless Chrome.
  deploy      Check, build and publish to GitHub Pages (gh-pages branch).
  spec-lint   Flag decompiler output, code and mangled names in clean-room spec drafts.
  wait-memory Return once memory pressure is normal (run before heavy work).
  d3d9probes release
              Build the Direct3D 9 probes' Windows runner zip and publish it
              as a GitHub release tagged by its hash, unless it exists.
  odeharness release
              The same for the physics library comparison (tests/odeharness).
  wincapture release
              The same for the original's capture on Windows (tools/wincapture).
  frames-receive [ADDRESS]
              Receive a Windows capture's fidelity files from the PC over the
              local network into the fidelity suite's references; prints the
              address, port and token the PC sends them with.
  wincapture plan
              Print that capture's plan (from tests/fidelity/views.rs).
  update      Move to the latest Rust, crates, Bazel, its modules and the
              wasm LLVM, then run the checks and the advisories.
  gc          Remove Bazel's output bases of deleted worktrees.";

pub use common::{Result, cmd, output, root, run, runfile, step};

fn main() -> ExitCode {
    // `bazel run`: paths given are the caller's (commands still run in the
    // workspace, `cmd`).
    common::start();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["suite", target, suite] => e2e::suite(target, suite),
        ["wait-memory"] => machine::wait_memory(),
        ["sim", steps @ ..] if !steps.is_empty() => e2e::sim(steps),
        [e2e::REAPER, holder, udid] => e2e::reap(holder, udid),
        ["simshot"] => e2e::screenshot(None).map(drop),
        ["simshot", path] => e2e::screenshot(Some(Path::new(path))).map(drop),
        ["simrec", path, seconds] => seconds
            .parse::<f64>()
            .map_err(|e| format!("seconds: {e}"))
            .and_then(|s| e2e::record_for(Path::new(path), Duration::from_secs_f64(s))),
        ["parity", against @ ..] => parity::run(against),
        ["deploy"] => deploy::run_deploy(),
        ["spec-lint", paths @ ..] => spec_lint::run(paths),
        ["update"] => bazel::update(),
        ["gc"] => bazel::gc(),
        ["wincapture", "release"] => bazel::build_output(&root(), wincapture::RUNNER)
            .and_then(|zip| wincapture::release(&zip)),
        ["frames-receive"] => frames::receive(None),
        ["frames-receive", address] => frames::receive(Some(address)),
        ["wincapture", "plan"] => {
            print!("{}", wincapture::plan());
            Ok(())
        }
        ["d3d9probes", "release"] => bazel::build_output(&root(), d3d9probes::RUNNER)
            .and_then(|zip| d3d9probes::release(&zip)),
        ["odeharness", "release"] => bazel::build_output(&root(), d3d9probes::ODEHARNESS)
            .and_then(|zip| d3d9probes::release_odeharness(&zip)),
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

/// Version of `package` recorded in Cargo.lock.
fn locked_version(package: &str) -> Result<String> {
    let lock = fs::read_to_string(root().join("Cargo.lock"))
        .map_err(|e| format!("read Cargo.lock: {e}"))?;
    locked_version_in(&lock, package).ok_or_else(|| format!("{package} not in Cargo.lock"))
}

fn locked_version_in(lock: &str, package: &str) -> Option<String> {
    let name = format!("name = \"{package}\"");
    let mut lines = lock.lines();
    lines.find(|l| *l == name)?;
    let version = lines.next()?.strip_prefix("version = \"")?;
    Some(version.trim_end_matches('"').to_owned())
}

/// The commit (`+` when the working tree is dirty, then `suffix`) and its
/// date, as a build's stamp.
fn git_stamp(suffix: &str) -> Result<(String, String)> {
    let commit = output(&mut cmd("git", &["rev-parse", "--short", "HEAD"]))?;
    let dirty = !output(&mut cmd("git", &["status", "--porcelain"]))?
        .trim()
        .is_empty();
    let commit = format!("{}{}{suffix}", commit.trim(), if dirty { "+" } else { "" });
    let date = output(&mut cmd("git", &["show", "-s", "--format=%cI", "HEAD"]))?;
    Ok((commit, date.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_locked_versions() {
        let lock = "[[package]]\nname = \"wasm-bindgen\"\nversion = \"0.2.130\"\n\n[[package]]\nname = \"wasm-bindgen-shared\"\nversion = \"0.2.131\"\n";
        assert_eq!(
            locked_version_in(lock, "wasm-bindgen").as_deref(),
            Some("0.2.130")
        );
        assert_eq!(
            locked_version_in(lock, "wasm-bindgen-shared").as_deref(),
            Some("0.2.131")
        );
        assert_eq!(locked_version_in(lock, "serde"), None);
    }
}
