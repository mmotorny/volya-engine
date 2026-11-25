// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya Engine core: platform-free engine code. Must not depend on any
//! platform API; see docs/architecture.md (Crates).

pub mod audio;
pub mod camera;
pub mod character;
pub mod clock;
pub mod collision;
pub mod config;
pub mod console;
pub mod cycles;
pub mod diagnostics;
pub mod footsteps;
pub mod glows;
pub mod input;
pub mod level;
pub mod lights;
pub mod movement;
pub mod object_light;
pub mod particles;
pub mod pause;
pub mod simulation;
pub mod sound;
pub mod switching;
pub mod text;
pub mod ui;
pub mod vfs;
pub mod weather;
