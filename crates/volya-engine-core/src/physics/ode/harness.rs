// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The physics library comparison's scenarios (tests/odeharness,
//! docs/testing.md) against our build: their names and their output
//! (records of 32-bit words: tag, count, words), natively and for wasm32;
//! natively, written as the release's `ours\` (`ode_harness_ours`) and the
//! comparison odeharness.exe makes; and [`difference`], which the engine
//! worker's check (the `physics_scenario` op) holds wasm32's output to the
//! native one with.

use std::ffi::{CStr, c_char, c_int, c_void};
#[cfg(not(target_arch = "wasm32"))]
use std::{ffi::CString, path::Path};

type Record = extern "C" fn(*mut c_void, u32, *const u32, u32);

unsafe extern "C" {
    fn volya_ode_harness(i: c_int, record: Record, ctx: *mut c_void) -> c_int;
    fn volya_ode_harness_name(i: c_int) -> *const c_char;
    #[cfg(not(target_arch = "wasm32"))]
    fn volya_ode_harness_compare(
        ours: *const c_char,
        shipped: *const c_char,
        report: *const c_char,
    ) -> c_int;
}

extern "C" fn append(ctx: *mut c_void, tag: u32, words: *const u32, n: u32) {
    // SAFETY: `ctx` is run's Vec; the driver passes n words.
    let (out, words) = unsafe {
        (
            &mut *ctx.cast::<Vec<u32>>(),
            std::slice::from_raw_parts(words, n as usize),
        )
    };
    out.push(tag);
    out.push(n);
    out.extend_from_slice(words);
}

/// The scenarios' names, in their order.
pub fn scenarios() -> Vec<String> {
    // SAFETY: an index out of range only returns the count.
    let n = unsafe { volya_ode_harness(-1, append, std::ptr::null_mut()) };
    (0..n)
        .map(|i| {
            // SAFETY: a static string of the driver's.
            unsafe { CStr::from_ptr(volya_ode_harness_name(i)) }
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// Scenario `i`'s output from our build. The library's values are global:
/// the solver's lock is held; the first scenario reads the values the
/// library loads with, so it is meaningful only before any world is made.
pub fn run(i: usize) -> Vec<u32> {
    let _turn = super::STEPPING.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = Vec::new();
    // SAFETY: the driver calls `append` with `out` and returns.
    unsafe { volya_ode_harness(i as c_int, append, (&raw mut out).cast()) };
    out
}

/// How `ours` differs from `native`, word for word (bits, NaNs too); `None`
/// when identical: the record where their records part, or how many words
/// differ and the first difference (record, tag, word, both words), as
/// compare.c reports.
pub fn difference(native: &[u32], ours: &[u32]) -> Option<String> {
    let (mut i, mut record, mut compared, mut differing) = (0, 0, 0, 0);
    let mut first = None;
    while i + 1 < native.len() && i + 1 < ours.len() {
        let (tag, n) = (native[i], native[i + 1] as usize);
        let end = i + 2 + n;
        if ours[i] != tag || ours[i + 1] as usize != n || end > native.len() || end > ours.len() {
            return Some(format!(
                "records differ at record {record}: native tag {tag:#x} of {n} words, ours tag \
                 {:#x} of {}",
                ours[i],
                ours[i + 1]
            ));
        }
        for w in 0..n {
            let (a, b) = (native[i + 2 + w], ours[i + 2 + w]);
            compared += 1;
            if a != b {
                differing += 1;
                first.get_or_insert((record, tag, w, a, b));
            }
        }
        i = end;
        record += 1;
    }
    if native.len() != ours.len() {
        return Some(format!(
            "lengths differ: native {} words, ours {}",
            native.len(),
            ours.len()
        ));
    }
    first.map(|(record, tag, word, a, b)| {
        format!(
            "{differing} of {compared} words differ; first: record {record} (tag {tag:#x}) word \
             {word}: native {a:#010x}, ours {b:#010x}"
        )
    })
}

/// Writes every scenario's output from our build into `dir` as
/// `<scenario>.bin`, as odeharness.exe writes the shipped library's (32-bit
/// little-endian words). Meaningful in a fresh process only ([`run`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn write_all(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (i, name) in scenarios().iter().enumerate() {
        let bytes: Vec<u8> = run(i).iter().flat_map(|w| w.to_le_bytes()).collect();
        std::fs::write(dir.join(format!("{name}.bin")), bytes)?;
    }
    Ok(())
}

/// The comparison odeharness.exe makes (tests/odeharness/compare.c) of the
/// outputs in `ours` and `shipped`: its lines, and how many scenarios are
/// not identical.
#[cfg(not(target_arch = "wasm32"))]
pub fn compare(ours: &Path, shipped: &Path, report: &Path) -> std::io::Result<(usize, String)> {
    let c =
        |p: &Path| CString::new(p.to_string_lossy().into_owned()).map_err(std::io::Error::other);
    let (a, b, r) = (c(ours)?, c(shipped)?, c(report)?);
    // SAFETY: three NUL-terminated paths; the driver writes `report`.
    let n = unsafe { volya_ode_harness_compare(a.as_ptr(), b.as_ptr(), r.as_ptr()) };
    let text = std::fs::read_to_string(report)?;
    usize::try_from(n)
        .map(|n| (n, text))
        .map_err(|_| std::io::Error::other("cannot write the report"))
}

#[cfg(test)]
mod tests {
    use super::difference;

    #[test]
    fn difference_names_the_record_and_word() {
        let native = [4, 2, 1, 2, 5, 3, 7, 8, 9];
        assert_eq!(difference(&native, &native), None);
        let mut ours = native;
        ours[7] ^= 1;
        assert_eq!(
            difference(&native, &ours).unwrap(),
            "1 of 5 words differ; first: record 1 (tag 0x5) word 1: native 0x00000008, ours \
             0x00000009"
        );
        assert_eq!(
            difference(&native, &native[..4]).unwrap(),
            "lengths differ: native 9 words, ours 4"
        );
        let mut parted = native;
        parted[5] = 4;
        assert!(
            difference(&native, &parted)
                .unwrap()
                .starts_with("records differ at record 1")
        );
    }
}
