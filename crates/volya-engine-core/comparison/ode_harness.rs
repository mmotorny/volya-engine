// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The physics library comparison (docs/testing.md, The physics library
//! against the shipped one) on our side: every scenario of tests/odeharness
//! against our build natively, in a process of its own (the first scenario
//! reads the values the library loads with), repeats; and the comparison
//! odeharness.exe makes on the PC (compare.c) finds our outputs identical to
//! themselves and reports a changed word where it is. Native only (the
//! scenarios' driver is linked natively).

#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use volya_engine_core::physics::ode::harness::{compare, run, scenarios, write_all};

#[test]
fn scenarios_repeat_and_the_comparison_finds_a_changed_word() {
    let names = scenarios();
    let dir = PathBuf::from(std::env::var_os("TEST_TMPDIR").expect("TEST_TMPDIR"));
    let (ours, other) = (dir.join("ours"), dir.join("other"));
    write_all(&ours).unwrap();
    for (i, name) in names.iter().enumerate().skip(1) {
        let again: Vec<u8> = run(i).iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(
            std::fs::read(ours.join(format!("{name}.bin"))).unwrap(),
            again,
            "{name}: not repeatable"
        );
    }

    let (n, report) = compare(&ours, &ours, &dir.join("same.txt")).unwrap();
    assert_eq!(n, 0, "{report}");
    assert_eq!(
        report.lines().filter(|l| l.contains(": identical")).count(),
        names.len() + 2
    );

    // One word of the free bodies' 10th record changed (its tag and count
    // first): reported there.
    std::fs::create_dir_all(&other).unwrap();
    for name in &names {
        let file = format!("{name}.bin");
        std::fs::copy(ours.join(&file), other.join(&file)).unwrap();
    }
    let free = other.join("free.bin");
    let mut bytes = std::fs::read(&free).unwrap();
    let record = 4 * (2 + 25) * 9;
    bytes[record + 8 + 4 * 3] ^= 1;
    std::fs::write(&free, bytes).unwrap();
    let (n, report) = compare(&ours, &other, &dir.join("changed.txt")).unwrap();
    assert_eq!(n, 1, "{report}");
    let line = report.lines().find(|l| l.starts_with("free:")).unwrap();
    assert!(
        line.contains("1 of") && line.contains("record 9 (body") && line.contains("word 3"),
        "{line}"
    );
}
