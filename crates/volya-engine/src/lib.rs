// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya Engine: the platform-free product library. Platform packages
//! (`platforms/*`) build on it and supply platform specifics.

/// Engine version, taken from the crate manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Text shown once the engine has started.
pub fn status_text() -> String {
    format!("Volya {VERSION}")
}

/// Engine test interface: what tests and debugging tools may observe and
/// drive. Grows with the engine (input injection, frame stepping, state
/// queries, framebuffer readback); see docs/testing.md.
pub mod test_api {
    /// Engine status as a JSON object.
    pub fn status() -> String {
        format!(r#"{{"state":"ready","version":"{}"}}"#, super::VERSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_text_contains_version() {
        assert_eq!(status_text(), format!("Volya {VERSION}"));
    }

    #[test]
    fn test_api_status_reports_version() {
        assert_eq!(
            test_api::status(),
            format!(r#"{{"state":"ready","version":"{VERSION}"}}"#)
        );
    }
}
