// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `bazel test //...` holds only hermetic, deterministic tests
//! (docs/architecture/build.md, Tests): no static check (`check`), game-data
//! test (`gamedata`) or browser suite (`browser`). `:default_unwanted` (a
//! genquery over every package's `default_tests`) lists those it would run.

use std::fs;

#[test]
fn the_default_run_is_hermetic() {
    let path = std::env::var("DEFAULT_UNWANTED").expect("DEFAULT_UNWANTED is set");
    let listed = fs::read_to_string(&path).expect("the query's output is readable");
    let unwanted: Vec<&str> = listed.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        unwanted.is_empty(),
        "`bazel test //...` runs static checks, game-data tests or browser \
         suites; tag them `manual` (they run by name: //:checks, //:gamedata, \
         //tests/browser:<suite>_<target>):\n{}",
        unwanted.join("\n")
    );
}
