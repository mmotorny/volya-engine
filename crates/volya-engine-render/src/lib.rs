// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The renderer: draw lists to frames (docs/architecture/rendering.md).
//!
//! Two backends behind one API while wgpu is replaced by direct WebGL 2
//! (rendering.md, Design: direct WebGL 2): wgpu's by default, the GL one
//! with the `gl` feature. Both draw the same programs (`glsl` holds their
//! GLSL ES 3.00, translated at build time).

pub mod gl;
pub mod glsl;
#[cfg(any(test, feature = "selftest"))]
pub mod selftest;
mod shared;
pub mod stats;

#[cfg(not(feature = "gl"))]
mod wgpu_backend;
#[cfg(not(feature = "gl"))]
pub use wgpu_backend::*;

#[cfg(feature = "gl")]
mod webgl;
#[cfg(feature = "gl")]
pub use webgl::*;
