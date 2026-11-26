// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What both renderers share while wgpu is replaced (docs/architecture/
//! rendering.md, Design: direct WebGL 2): the platform-free parts of each
//! area, which each backend's module of that area re-exports, so the
//! engine sees one API whichever is built.

pub mod details;
pub mod particles;
pub mod present;
pub mod sky;
pub mod texture;
pub mod ui;
pub mod world;
