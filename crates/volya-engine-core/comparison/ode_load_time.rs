// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The physics library's load-time state (docs/behavior/physics-harness.md
//! 2), in a process of its own so that no world has been made yet: the
//! auto-disable linear threshold reads back as the one `dWorldCreate` sets
//! (the square root of 0.001 squared, in single precision), and as before
//! after a world is made.

#![allow(unsafe_code)]

use std::ffi::c_void;

// The library's functions, linked through volya-engine-core.
use volya_engine_core as _;

unsafe extern "C" {
    fn dWorldCreate() -> *mut c_void;
    fn dWorldDestroy(w: *mut c_void);
    fn dWorldGetAutoDisableLinearThreshold(w: *mut c_void) -> f32;
}

#[test]
fn the_linear_threshold_loads_as_dworldcreate_sets_it() {
    let default = (0.001_f32 * 0.001_f32).sqrt();
    // SAFETY: the getter reads only the library-wide value; a world made
    // and destroyed.
    unsafe {
        let loaded = dWorldGetAutoDisableLinearThreshold(std::ptr::null_mut());
        assert_eq!(loaded.to_bits(), default.to_bits(), "{loaded}");
        let w = dWorldCreate();
        let made = dWorldGetAutoDisableLinearThreshold(w);
        assert_eq!(made.to_bits(), default.to_bits(), "{made}");
        dWorldDestroy(w);
    }
}
