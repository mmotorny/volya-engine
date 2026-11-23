// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya Engine: the platform-free product library. Platform packages
//! (`platforms/*`) build on it and supply platform specifics.

pub use volya_engine_core as core;

/// Engine version, taken from the crate manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Engine test interface: what tests and debugging tools may observe and
/// drive. Grows with the engine (input injection, frame stepping, state
/// queries, framebuffer readback); see docs/testing.md.
pub mod test_api {
    use serde::Serialize;

    #[derive(Clone, Debug, Serialize)]
    pub struct Status {
        pub state: &'static str,
        pub version: &'static str,
    }

    pub fn status() -> Status {
        Status {
            state: "ready",
            version: super::VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes() {
        assert_eq!(
            serde_json::to_string(&test_api::status()).unwrap(),
            format!(r#"{{"state":"ready","version":"{VERSION}"}}"#)
        );
    }
}
