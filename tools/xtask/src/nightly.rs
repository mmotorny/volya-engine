// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `nightly`: the suites too slow for every change (`//:nightly`), with each
//! failing test and its log listed at the end, to be filed as issues
//! (docs/architecture/build.md, Nightly).

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{Result, cmd, output, step};

const SUITE: &str = "//:nightly";

/// Bazel's summary statuses of a test that didn't pass.
const FAILING: &[&str] = &[
    "FAILED",
    "FAILED TO BUILD",
    "FLAKY",
    "INCOMPLETE",
    "NO STATUS",
    "TIMEOUT",
];

/// Bazel's summary statuses of a test that passed or wasn't meant to run.
const PASSING: &[&str] = &["PASSED", "SKIPPED"];

/// Runs `//:nightly` (`bazel test --keep_going`, with `bazel_args` after),
/// once memory pressure is normal, and lists each failing test with its log.
pub fn run(bazel_args: &[&str]) -> Result {
    crate::machine::wait_memory()?;
    let start = Instant::now();
    report_revision();
    let testlogs = PathBuf::from(output(&mut cmd("bazel", &["info", "bazel-testlogs"]))?.trim());

    step(&format!("bazel test {SUITE}"));
    let mut args = vec!["test", "--keep_going", SUITE];
    args.extend_from_slice(bazel_args);
    // Bazel writes its summary to stdout or stderr (measured: either, with
    // the other redirected), so both go through one pipe: echoed, and kept
    // for the summary's lines.
    let (reader, writer) = std::io::pipe().map_err(|e| format!("pipe: {e}"))?;
    let mut bazel = cmd("bazel", &args);
    bazel
        .stdout(writer.try_clone().map_err(|e| format!("pipe: {e}"))?)
        .stderr(writer);
    let mut child = bazel.spawn().map_err(|e| format!("bazel: {e}"))?;
    // The pipe ends with Bazel only once this process holds no writer.
    drop(bazel);
    let mut summary = String::new();
    for line in BufReader::new(reader).lines() {
        let line = line.map_err(|e| format!("bazel: {e}"))?;
        eprintln!("{line}");
        summary.push_str(&line);
        summary.push('\n');
    }
    let status = child.wait().map_err(|e| format!("bazel: {e}"))?;
    let failures = failures(&summary);
    let elapsed = start.elapsed().as_secs();
    let took = format!("{}m{:02}s", elapsed / 60, elapsed % 60);

    if failures.is_empty() {
        if status.success() {
            step(&format!("Nightly passed in {took}"));
            return Ok(());
        }
        return Err(format!(
            "bazel test {SUITE} failed with {status} in {took}, no test failing (see above)"
        ));
    }
    step(&format!("Nightly: {} failing, in {took}", failures.len()));
    let mut out = std::io::stdout().lock();
    for failure in &failures {
        let log = log_path(&testlogs, &failure.label);
        let _ = writeln!(out, "{} {}", failure.label, failure.status);
        if log.is_file() {
            let _ = writeln!(out, "  {}", log.display());
        } else {
            let _ = writeln!(out, "  no log: see the build's output above");
        }
    }
    Err(format!("{} nightly tests failing", failures.len()))
}

/// Prints the commit tested, and whether it is origin/main's (as of the last
/// fetch).
fn report_revision() {
    let rev = |r: &str| output(&mut cmd("git", &["rev-parse", "--short", r])).ok();
    let (Some(head), main) = (rev("HEAD"), rev("origin/main")) else {
        return;
    };
    let head = head.trim();
    match main.as_deref().map(str::trim) {
        Some(main) if main == head => step(&format!("Testing {head} (origin/main)")),
        Some(main) => step(&format!("Testing {head}, not origin/main ({main})")),
        None => step(&format!("Testing {head}")),
    }
}

#[derive(Debug, PartialEq)]
struct Failure {
    label: String,
    status: String,
}

/// The tests Bazel's summary reports as not passing, in its order, each
/// once.
fn failures(summary: &str) -> Vec<Failure> {
    let mut failures: Vec<Failure> = Vec::new();
    for line in summary.lines() {
        let Some((label, status)) = summary_line(line) else {
            continue;
        };
        failures.retain(|f| f.label != label);
        if FAILING.contains(&status) {
            failures.push(Failure {
                label: label.to_owned(),
                status: status.to_owned(),
            });
        }
    }
    failures
}

/// A summary line's label and status: `//p:t  (cached) PASSED in 1.2s`,
/// `//p:t  FLAKY, failed in 1 out of 2 in 3.0s`, `//p:t  NO STATUS`.
fn summary_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if !line.starts_with("//") && !line.starts_with('@') {
        return None;
    }
    let (label, rest) = line.split_once(char::is_whitespace)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("(cached)").unwrap_or(rest).trim_start();
    let status = rest.split([',']).next()?;
    let status = status.split(" in ").next()?.trim();
    (FAILING.contains(&status) || PASSING.contains(&status)).then_some((label, status))
}

/// Where Bazel keeps `label`'s log: `<testlogs>/<package>/<name>/test.log`.
fn log_path(testlogs: &Path, label: &str) -> PathBuf {
    let label = label.trim_start_matches('@');
    let label = label.split_once("//").map_or(label, |(_, l)| l);
    let (package, name) = label.split_once(':').unwrap_or((label, ""));
    testlogs.join(package).join(name).join("test.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_failing_tests_of_the_summary() {
        let summary = "\
INFO: Build completed, 2 tests FAILED, 120 total actions
FAIL: //tests/browser:game_level_chrome (see /logs/tests/browser/game_level_chrome/test.log)
//crates/volya-engine-core:particles                     (cached) PASSED in 1.2s
//crates/volya-engine-script:gog                                  SKIPPED
//crates/volya-engine-formats:dds                        FLAKY, failed in 1 out of 2 in 3.0s
//tests/browser:game_level_chrome                                 FAILED in 812.5s
  /logs/tests/browser/game_level_chrome/test.log
//tests/browser:game_memory_chrome                               TIMEOUT in 3600.0s
//tests/browser:game_input_chrome                                 NO STATUS
//tests/browser:game_startup_chrome                         FAILED TO BUILD
//crates/volya-engine:sound                    FAILED in 2 out of 2 in 0.4s

Executed 4 out of 8 tests: 1 test passes and 5 fail locally.
";
        let want = [
            ("//crates/volya-engine-formats:dds", "FLAKY"),
            ("//tests/browser:game_level_chrome", "FAILED"),
            ("//tests/browser:game_memory_chrome", "TIMEOUT"),
            ("//tests/browser:game_input_chrome", "NO STATUS"),
            ("//tests/browser:game_startup_chrome", "FAILED TO BUILD"),
            ("//crates/volya-engine:sound", "FAILED"),
        ]
        .map(|(label, status)| Failure {
            label: label.to_owned(),
            status: status.to_owned(),
        });
        assert_eq!(failures(summary), want);
    }

    #[test]
    fn a_passing_summary_has_no_failures() {
        let summary =
            "//:a  PASSED in 1s\n//b:c  (cached) PASSED in 0.1s\nTarget //b:c up-to-date\n";
        assert!(failures(summary).is_empty());
    }

    #[test]
    fn ignores_lines_that_are_not_the_summary() {
        assert_eq!(summary_line("//p:t: some error FAILED"), None);
        assert_eq!(summary_line("Target //p:t up-to-date:"), None);
        assert_eq!(
            summary_line("@repo//p:t  TIMEOUT in 9s"),
            Some(("@repo//p:t", "TIMEOUT"))
        );
    }

    #[test]
    fn finds_the_logs() {
        let logs = Path::new("/out/testlogs");
        assert_eq!(
            log_path(logs, "//tests/browser:game_level_chrome"),
            logs.join("tests/browser/game_level_chrome/test.log")
        );
        assert_eq!(log_path(logs, "//:spdx"), logs.join("spdx/test.log"));
    }
}
