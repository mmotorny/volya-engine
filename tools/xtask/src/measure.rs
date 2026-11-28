// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `bazel run //tools/xtask -- measure MEASUREMENT --target TARGET`: a
//! measurement run on demand, its figures a report, never a pass or fail
//! (docs/architecture/build.md, Measurements). Each is a browser suite's
//! page (`tests/browser/<measurement>.js`) run as the suites are, on the
//! test site built in the workspace.

use super::{Result, root, step};
use crate::e2e;

/// The measurements, each a page in `tests/browser/`.
const MEASUREMENTS: &[&str] = &["audio-load"];
const TARGETS: &[&str] = &["chrome", "iphone"];

/// The measurement and target `args` name: `MEASUREMENT --target TARGET`.
fn parse<'a>(args: &[&'a str]) -> Result<(&'a str, &'a str)> {
    let usage = || {
        format!(
            "measure: expected MEASUREMENT --target TARGET, a measurement of \
             {MEASUREMENTS:?} and a target of {TARGETS:?}"
        )
    };
    let (measurement, target) = match args {
        [measurement, "--target", target] | ["--target", target, measurement] => {
            (*measurement, *target)
        }
        _ => return Err(usage()),
    };
    if !MEASUREMENTS.contains(&measurement) || !TARGETS.contains(&target) {
        return Err(usage());
    }
    Ok((measurement, target))
}

/// Runs the measurement `args` name and prints its report.
pub fn run(args: &[&str]) -> Result {
    let (measurement, target) = parse(args)?;
    crate::machine::wait_memory()?;
    step("The test site");
    let workspace = root();
    let site = crate::bazel::build_output(&workspace, "//platforms/web:site_test")?;
    let next = crate::bazel::build_output(&workspace, "//platforms/web:site_next")?;
    e2e::run_on(
        target,
        measurement,
        &site,
        &next,
        &workspace.join("tests/browser"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_measurement_and_target() {
        assert_eq!(
            parse(&["audio-load", "--target", "chrome"]),
            Ok(("audio-load", "chrome"))
        );
        assert_eq!(
            parse(&["--target", "iphone", "audio-load"]),
            Ok(("audio-load", "iphone"))
        );
    }

    #[test]
    fn rejects_unknown_or_missing_arguments() {
        for args in [
            &[][..],
            &["audio-load"],
            &["audio-load", "--target"],
            &["audio-load", "chrome"],
            &["audio-load", "--target", "safari"],
            &["smoke", "--target", "chrome"],
            &["audio-load", "--target", "chrome", "extra"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
    }
}
