// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The render self-test (docs/testing.md, Rendering): scenes drawn from
//! synthetic data, read back and checked pixel by pixel. It runs where the
//! renderer draws, in the engine worker through the web platform's
//! `render_selftest` op (headless Chrome's WebGL 2, the iOS Simulator's),
//! as the script runtime's self-test does; nothing draws natively.
//!
//! Each check gets a fresh [`Renderer`] from the platform and uses only the
//! API both backends share (rendering.md, Design: direct WebGL 2), so it
//! checks the GL renderer as it does wgpu's. A check fails by returning an
//! error, never by panicking, which would stop the engine.

use std::future::Future;
use std::pin::Pin;

use crate::{Color, DrawList, Renderer};

mod ui;

/// Why a check didn't pass.
#[derive(Debug)]
pub enum Problem {
    /// It doesn't apply here (e.g. the device lacks what it checks).
    Skip(String),
    /// It failed, with what was found.
    Fail(String),
}

impl From<String> for Problem {
    fn from(message: String) -> Self {
        Self::Fail(message)
    }
}

/// A check's outcome: passed with a note for the report (maybe empty).
pub type Outcome = Result<String, Problem>;

/// Makes a renderer for a check.
pub type Make<'a> = &'a dyn Fn() -> Renderer;

type Run = for<'a> fn(Make<'a>) -> Pin<Box<dyn Future<Output = Outcome> + 'a>>;

/// The checks, in the order they run.
const CHECKS: &[(&str, Run)] = &[
    // The UI layer, textures and video (ui.rs).
    ("empty list clears", |m| Box::pin(ui::empty_list_clears(m))),
];

/// Every check's name, in order.
pub fn names() -> Vec<&'static str> {
    CHECKS.iter().map(|(name, _)| *name).collect()
}

/// Runs the check called `name` with renderers from `make`.
pub async fn run(name: &str, make: Make<'_>) -> Outcome {
    match CHECKS.iter().find(|(n, _)| *n == name) {
        Some((_, check)) => check(make).await,
        None => Err(Problem::Fail(format!("no render check {name:?}"))),
    }
}

/// Fails the check with the message unless `condition` holds.
macro_rules! ensure {
    ($condition:expr, $($message:tt)+) => {
        if !$condition {
            return Err($crate::selftest::Problem::Fail(format!($($message)+)));
        }
    };
}

pub(crate) use ensure;

const BLACK: Color = Color::BLACK;

/// `list` drawn at `width` × `height` over black, as RGBA rows top to
/// bottom.
async fn capture(
    renderer: &mut Renderer,
    list: &DrawList,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Problem> {
    renderer
        .capture(list, width, height, BLACK)
        .await
        .map_err(|e| Problem::Fail(format!("capture: {e}")))
}
