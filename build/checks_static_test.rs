// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `//:checks` holds only static checks (AGENTS.md): every test it expands
//! to is tagged `check`. `:unchecked_tests` (a genquery) lists those that
//! aren't, such as a browser suite pulled in by a package's suite.

use std::fs;

#[test]
fn every_test_of_the_checks_is_a_static_check() {
    let path = std::env::var("UNCHECKED_TESTS").expect("UNCHECKED_TESTS is set");
    let listed = fs::read_to_string(&path).expect("the query's output is readable");
    let unchecked: Vec<&str> = listed.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        unchecked.is_empty(),
        "//:checks runs tests not tagged `check` (static checks only; browser \
         suites and engine tests belong in //... and by name):\n{}",
        unchecked.join("\n")
    );
}
