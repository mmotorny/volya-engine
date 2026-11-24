// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya Engine: the platform-free product library. Platform packages
//! (`platforms/*`) build on it and supply platform specifics.
//!
//! [`app::App`] runs the game from launch to the title menu on a clock and
//! input the platform supplies, producing [`scene::Scene`]s that
//! [`present::Presenter`] turns into renderer draw lists, and playing sounds
//! through [`audio::AudioOut`].

pub use volya_engine_core as core;
pub use volya_engine_formats as formats;
pub use volya_engine_render as render;

pub mod app;
pub mod audio;
pub mod data;
mod draw;
#[cfg(test)]
mod fixture;
pub mod free_controls;
pub mod level;
pub mod loading_screen;
pub mod present;
pub mod scene;
pub mod shading;
pub mod sky;
pub mod startup;
pub mod title_menu;

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
