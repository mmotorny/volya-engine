// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The high-resolution performance counter that the statistics overlay's
//! timers read (docs/behavior/display-mode.md B.4.1): milliseconds from an
//! arbitrary origin, the platform's counter once installed; natively,
//! without one, the monotonic clock; in wasm without one, 0.

use std::sync::OnceLock;

static COUNTER: OnceLock<fn() -> f64> = OnceLock::new();

/// Installs the platform's counter, once; false when one was installed.
pub fn install(counter: fn() -> f64) -> bool {
    COUNTER.set(counter).is_ok()
}

/// The counter's reading, ms.
pub fn now_ms() -> f64 {
    if let Some(counter) = COUNTER.get() {
        return counter();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0
    }
    #[cfg(target_arch = "wasm32")]
    0.0
}
