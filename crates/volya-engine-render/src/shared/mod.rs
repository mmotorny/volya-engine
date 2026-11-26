// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The platform-free part of each area, which the WebGL 2 module of that
//! area re-exports; it keeps native unit tests.

pub mod details;
pub mod particles;
pub mod present;
pub mod sky;
pub mod texture;
pub mod ui;
pub mod world;
