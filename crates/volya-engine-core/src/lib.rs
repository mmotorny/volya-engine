// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya Engine core: platform-free engine code. Must not depend on any
//! platform API; see docs/architecture/README.md.

pub mod artefacts;
pub mod audio;
#[cfg(target_arch = "wasm32")]
pub mod c_heap;
pub mod camera;
pub mod character;
pub mod character_identity;
pub mod climbing;
pub mod clock;
pub mod collision;
pub mod config;
pub mod console;
pub mod cycles;
pub mod details;
pub mod detector;
pub mod diagnostics;
pub mod dialogs;
pub mod display;
pub mod door;
pub mod file_cache;
pub mod footsteps;
pub mod glows;
pub mod info_portions;
pub mod input;
pub mod level;
pub mod lights;
pub mod memory_manager;
pub mod motion_playback;
pub mod movement;
pub mod navigation;
pub mod object_light;
pub mod occlusion;
pub mod particles;
pub mod pause;
pub mod perf_counter;
pub mod physics;
pub mod simulation;
pub mod sound;
pub mod switching;
pub mod text;
pub mod texture_quality;
pub mod torch;
pub mod ui;
pub mod vfs;
pub mod visibility;
pub mod weather;
pub mod zones;
