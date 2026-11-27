// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The physics library comparison (docs/testing.md, The physics library
//! against the shipped one): every scenario of tests/odeharness against our
//! build, in a process of its own (the first scenario reads the values the
//! library loads with). Our output is the same when run again, and is
//! written to the test's outputs as odeharness.exe writes the shipped
//! library's. With `VOLYA_ODE_SHIPPED` naming a folder of the shipped
//! library's results, each scenario is compared with them and the shipped
//! library's runs with and without denormals-are-zero with each other (#769);
//! the findings are printed, not asserted. Native only (the scenarios'
//! driver is linked natively).

#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use volya_engine_core::physics::ode::harness::{compare, run, scenarios, words};

fn bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

#[test]
fn scenarios_against_our_build_and_the_shipped_results() {
    let names = scenarios();
    let ours: Vec<Vec<u32>> = (0..names.len()).map(run).collect();
    for (i, name) in names.iter().enumerate().skip(1) {
        assert_eq!(run(i), ours[i], "{name}: not repeatable");
    }
    if let Some(dir) = std::env::var_os("TEST_UNDECLARED_OUTPUTS_DIR") {
        for (name, out) in names.iter().zip(&ours) {
            std::fs::write(PathBuf::from(&dir).join(format!("{name}.bin")), bytes(out)).unwrap();
        }
    }
    let Some(shipped) = std::env::var_os("VOLYA_ODE_SHIPPED") else {
        println!("VOLYA_ODE_SHIPPED not set: our build only");
        return;
    };
    let shipped = PathBuf::from(shipped);
    let read = |name: &str| std::fs::read(shipped.join(format!("{name}.bin"))).map(|b| words(&b));
    for (name, out) in names.iter().zip(&ours) {
        match read(name) {
            Err(e) => println!("{name}: no shipped result ({e})"),
            Ok(theirs) => match compare(out, &theirs) {
                Ok(None) => println!("{name}: identical"),
                Ok(Some(d)) => println!(
                    "{name}: {} of {} words differ; first: record {} (tag {:#x}) word {}: ours {:#010x}, shipped {:#010x}",
                    d.differing, d.compared, d.record, d.tag, d.word, d.ours, d.theirs
                ),
                Err(e) => println!("{name}: records differ: {e}"),
            },
        }
    }
    for (with, without) in [("joints", "joints-nodaz"), ("contacts", "contacts-nodaz")] {
        if let (Ok(a), Ok(b)) = (read(with), read(without)) {
            match compare(&a, &b) {
                Ok(None) => println!("shipped {with}: denormals-are-zero changes nothing"),
                Ok(Some(d)) => println!(
                    "shipped {with}: denormals-are-zero changes {} of {} words, first at record {}",
                    d.differing, d.compared, d.record
                ),
                Err(e) => println!("shipped {with}: records differ with denormals: {e}"),
            }
        }
    }
}
