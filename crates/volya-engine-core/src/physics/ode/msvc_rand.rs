// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The C run time's `rand` the shipped library's row shuffle draws from
//! (docs/behavior/physics-library.md 8): its state per thread, starting at
//! 1; each call sets s = s·214,013 + 2,531,011 and returns (s >> 16) &
//! 32,767. ODE's `rand` is this (csrc/ode/include/ode/config.h).

use std::cell::Cell;
use std::ffi::c_int;

thread_local! {
    static STATE: Cell<u32> = const { Cell::new(1) };
}

/// The next value of this thread's generator.
pub fn rand() -> i32 {
    STATE.with(|s| {
        let v = s.get().wrapping_mul(214_013).wrapping_add(2_531_011);
        s.set(v);
        ((v >> 16) & 0x7FFF) as i32
    })
}

/// This thread's generator state.
pub fn state() -> u32 {
    STATE.with(Cell::get)
}

/// Sets this thread's generator state (`srand`'s effect).
pub fn set_state(s: u32) {
    STATE.with(|c| c.set(s));
}

/// ODE's `rand` (config.h).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn volya_ode_rand() -> c_int {
    rand()
}

/// The run time's `srand`, for the comparison's driver
/// (tests/odeharness/native.c).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn volya_ode_srand(seed: u32) {
    set_state(seed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_time_sequence_from_1() {
        std::thread::spawn(|| {
            let v: Vec<i32> = (0..5).map(|_| rand()).collect();
            assert_eq!(v, [41, 18467, 6334, 26500, 19169]);
            set_state(1);
            assert_eq!(rand(), 41);
            assert_eq!(state(), 1u32.wrapping_mul(214_013).wrapping_add(2_531_011));
        })
        .join()
        .unwrap();
    }
}
