// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The physics library comparison's scenarios (tests/odeharness,
//! docs/testing.md) against our build, natively: their names, their output
//! (records of 32-bit words: tag, count, words) and a comparison of two
//! outputs.

use std::ffi::{CStr, c_char, c_int, c_void};

type Record = extern "C" fn(*mut c_void, u32, *const u32, u32);

unsafe extern "C" {
    fn volya_ode_harness(i: c_int, record: Record, ctx: *mut c_void) -> c_int;
    fn volya_ode_harness_name(i: c_int) -> *const c_char;
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

/// Where two outputs first differ, and how many words differ, counting any
/// two NaNs as equal (the shipped library's NaNs are the x87's and SSE's
/// default NaN, ours the platform's).
#[derive(Debug, PartialEq, Eq)]
pub struct Difference {
    /// The record (its index and tag) and the word in it.
    pub record: usize,
    pub tag: u32,
    pub word: usize,
    pub ours: u32,
    pub theirs: u32,
    /// Words that differ, of all words compared.
    pub differing: usize,
    pub compared: usize,
}

fn is_nan(w: u32) -> bool {
    w & 0x7F80_0000 == 0x7F80_0000 && w & 0x007F_FFFF != 0
}

/// None when the outputs are the same (NaNs aside); Err when their records
/// don't line up.
pub fn compare(ours: &[u32], theirs: &[u32]) -> Result<Option<Difference>, String> {
    let (mut i, mut record) = (0, 0);
    let mut first = None;
    let (mut differing, mut compared) = (0, 0);
    while i < ours.len() && i < theirs.len() {
        let (tag, n) = (ours[i], ours.get(i + 1).copied().unwrap_or(0) as usize);
        if theirs[i] != tag || theirs.get(i + 1).copied() != Some(n as u32) {
            return Err(format!(
                "record {record}: ours is tag {tag} of {n} words, theirs tag {} of {:?}",
                theirs[i],
                theirs.get(i + 1)
            ));
        }
        for w in 0..n {
            let (a, b) = (ours[i + 2 + w], theirs[i + 2 + w]);
            compared += 1;
            if a != b && !(is_nan(a) && is_nan(b)) {
                differing += 1;
                first.get_or_insert((record, tag, w, a, b));
            }
        }
        i += 2 + n;
        record += 1;
    }
    if ours.len() != theirs.len() {
        return Err(format!(
            "lengths differ: ours {} words, theirs {}",
            ours.len(),
            theirs.len()
        ));
    }
    Ok(first.map(|(record, tag, word, ours, theirs)| Difference {
        record,
        tag,
        word,
        ours,
        theirs,
        differing,
        compared,
    }))
}

/// Words of a scenario's file as written on Windows (little-endian).
pub fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect()
}
