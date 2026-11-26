// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The renderer: draw lists to frames (docs/architecture/rendering.md).
//!
//! It draws through WebGL 2 alone (rendering.md, WebGL 2): `gl` is the
//! only code calling it, `glsl` holds the programs' GLSL ES 3.00,
//! translated from WGSL at build time.

pub mod gl;
pub mod glsl;
#[cfg(any(test, feature = "selftest"))]
pub mod selftest;
mod shared;
pub mod stats;
mod webgl;

pub use webgl::*;
