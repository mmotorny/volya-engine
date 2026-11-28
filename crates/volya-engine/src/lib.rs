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
pub use volya_engine_script as script;

pub mod app;
pub mod audio;
pub mod characters;
pub mod console_window;
pub mod controls;
pub mod data;
pub mod detector;
pub mod dialog;
pub mod draw;
#[cfg(test)]
mod fixture;
pub mod footsteps;
pub mod inventory_screen;
pub mod level;
pub mod level_details;
pub mod level_doors;
pub mod level_lights;
pub mod level_maps;
pub mod level_motion_channels;
pub mod level_motions;
pub mod level_physics;
pub mod level_sound;
pub mod level_visibility;
pub mod loading_screen;
pub mod menu_screens;
pub mod objects;
pub mod on_screen_keyboard;
pub mod onsets;
pub mod options;
pub mod particle_drawing;
pub mod pda_window;
pub mod present;
pub mod projected_light;
pub mod quick_info;
pub mod save_load;
pub mod saved_games;
pub mod scene;
pub mod scripts;
#[cfg(feature = "render-selftest")]
pub mod selftest;
pub mod shading;
pub mod sky;
pub mod startup;
pub mod statistics_overlay;
pub mod talk_window;
pub mod title_menu;
pub mod torch;
pub mod ui_image;

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
