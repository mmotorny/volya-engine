// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Render checks on game data (docs/testing.md, Rendering): the game's
//! screens and levels drawn from the imported game and read back, run in
//! the engine worker through the web platform's `render_game_selftest`
//! op, as the render crate's self-test is (`volya_engine_render::selftest`);
//! nothing draws natively. The checks that need no drawing stay native
//! tests (`tests/*.rs`).
//!
//! A check gets the game's archive tree and fresh renderers from the
//! platform, and keeps frames for inspection with [`Context::dump`]. It
//! uses only the renderer's API and fails by returning
//! an error, never by panicking.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;

use crate::core::vfs::Vfs;
use crate::render::Renderer;
pub use crate::render::selftest::{
    Outcome, Problem, capture, capture_scaled, ensure, ensure_eq, ensure_ne, ensure_near, near,
    now_ms, pixel,
};

mod artefacts;
mod details;
mod draw_calls;
mod fidelity;
mod frame_memory;
mod geometry_lod;
mod inventory_screen;
mod lights;
mod new_game;
mod objects;
mod options;
mod presentation;
mod save_load;
mod statistics;
mod texture_settings;
mod textures;
mod title_menu;
mod torch;
mod visibility;
mod zones;

/// What a check runs with.
pub struct Context<'a> {
    /// The game's combined archive tree.
    pub tree: &'a dyn Vfs,
    /// The game's root folder (the installer's files outside the archives),
    /// where the platform has it.
    pub root: Option<&'a dyn Vfs>,
    make: &'a dyn Fn() -> Renderer,
    /// Frames kept for inspection, when asked for.
    frames: Option<RefCell<Vec<Frame>>>,
    /// [`Context::gl_calls`].
    gl_calls: Option<&'a dyn Fn(bool) -> Vec<(String, u64)>>,
    /// [`Context::heap`].
    heap: Option<&'a dyn Fn() -> HeapCount>,
}

/// The heap's counts so far, where the platform counts them.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeapCount {
    /// Allocations made (a growing reallocation counts as one).
    pub allocations: usize,
    /// Bytes allocated (a growing reallocation counts its new size).
    pub bytes: usize,
    /// Bytes live.
    pub live: usize,
}

/// A frame a check keeps for inspection (the native `VOLYA_DUMP_*`).
pub struct Frame {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// RGBA, rows top to bottom.
    pub rgba: Vec<u8>,
    /// Compared with an earlier run's frame of the name, where the suite
    /// has one: every pixel held to it when `Some(true)`, the differences
    /// only reported when `Some(false)`.
    pub compare: Option<bool>,
}

impl<'a> Context<'a> {
    /// A check's context: frames are kept only with `keep_frames`.
    pub fn new(tree: &'a dyn Vfs, make: &'a dyn Fn() -> Renderer, keep_frames: bool) -> Self {
        Self {
            tree,
            root: None,
            make,
            frames: keep_frames.then(Default::default),
            gl_calls: None,
            heap: None,
        }
    }

    /// Lets [`Context::gl_calls`] count with `count`: `count(true)` starts
    /// counting WebGL 2's calls, `count(false)` stops and returns them by
    /// name.
    pub fn with_gl_calls(mut self, count: &'a dyn Fn(bool) -> Vec<(String, u64)>) -> Self {
        self.gl_calls = Some(count);
        self
    }

    /// Gives the checks the game's root folder.
    pub fn with_root(mut self, root: &'a dyn Vfs) -> Self {
        self.root = Some(root);
        self
    }

    /// Lets [`Context::heap`] read the heap's counts with `count`.
    pub fn with_heap(mut self, count: &'a dyn Fn() -> HeapCount) -> Self {
        self.heap = Some(count);
        self
    }

    /// Reads the heap's counts (`None` where the platform doesn't count
    /// them).
    pub fn heap(&self) -> Option<&'a dyn Fn() -> HeapCount> {
        self.heap
    }

    /// WebGL 2's calls by name while `f` runs (`None` where the platform
    /// doesn't count them).
    pub fn gl_calls<T>(&self, f: impl FnOnce() -> T) -> (T, Option<Vec<(String, u64)>>) {
        let Some(count) = self.gl_calls else {
            return (f(), None);
        };
        count(true);
        let value = f();
        (value, Some(count(false)))
    }

    /// A fresh renderer.
    pub fn renderer(&self) -> Renderer {
        (self.make)()
    }

    /// Keeps `rgba` (`width` × `height`) as frame `name`, if frames are
    /// kept.
    pub fn dump(&self, name: &str, width: u32, height: u32, rgba: &[u8]) {
        if let Some(frames) = &self.frames {
            frames.borrow_mut().push(Frame {
                name: name.into(),
                width,
                height,
                rgba: rgba.to_vec(),
                compare: None,
            });
        }
    }

    /// As [`Context::dump`], and compared with an earlier run's frame of
    /// the name where the suite has one (`VOLYA_COMPARE_DIR`): every pixel
    /// held to it when `exact`, else the differences reported.
    pub fn dump_compared(&self, name: &str, width: u32, height: u32, rgba: &[u8], exact: bool) {
        if let Some(frames) = &self.frames {
            frames.borrow_mut().push(Frame {
                name: name.into(),
                width,
                height,
                rgba: rgba.to_vec(),
                compare: Some(exact),
            });
        }
    }

    /// The frames kept.
    pub fn take_frames(&self) -> Vec<Frame> {
        self.frames
            .as_ref()
            .map(|f| std::mem::take(&mut *f.borrow_mut()))
            .unwrap_or_default()
    }
}

/// A check: `("name", |c| Box::pin(module::check(c)))`.
type Run = for<'a> fn(&'a Context<'a>) -> Pin<Box<dyn Future<Output = Outcome> + 'a>>;

/// A module's checks, by name.
type Checks = &'static [(&'static str, Run)];

/// The checks of each browser suite (each module's `CHECKS`), in the order
/// they run.
const SUITES: &[(&str, &[Checks])] = &[
    (
        "render-screens",
        &[
            title_menu::CHECKS,
            statistics::CHECKS,
            textures::CHECKS,
            save_load::CHECKS,
            inventory_screen::CHECKS,
            options::CHECKS,
        ],
    ),
    (
        "render-level",
        &[
            new_game::CHECKS,
            presentation::CHECKS,
            texture_settings::CHECKS,
            geometry_lod::CHECKS,
            objects::CHECKS,
            lights::CHECKS,
            torch::CHECKS,
            zones::CHECKS,
            artefacts::CHECKS,
            details::CHECKS,
            visibility::CHECKS,
            frame_memory::CHECKS,
        ],
    ),
    ("draw-calls", &[draw_calls::CHECKS]),
    ("fidelity", &[fidelity::CHECKS]),
];

fn checks() -> impl Iterator<Item = (&'static str, &'static str, Run)> {
    SUITES.iter().flat_map(|(suite, modules)| {
        modules
            .iter()
            .flat_map(move |checks| checks.iter().map(move |(name, run)| (*suite, *name, *run)))
    })
}

/// The `parity` suite's checks (`xtask parity`): the views two
/// builds draw, compared pixel by pixel: the title menu and a
/// video, a New Game's start view and an outdoor one, its objects, lights
/// and glows, a caged lamp's glass and glow, the torch, a campfire's and an artefact's particles, the
/// placed distortion effect, the grass, the presentation at
/// `r__supersample` 1 and 2, and the draw-call views with the trees
/// blended and unblended.
const PARITY: &[&str] = &[
    "startup and title menu from game data",
    "video frames as textures from game data",
    "new game start view",
    "placed effects are drawn",
    "start view supersampled",
    "start view shows the objects",
    "a lamp lights the level",
    "a hanging lamp and its glow",
    "the torch lights the level",
    "a campfire is drawn",
    "an artefact's effect is drawn",
    "grass from game data",
    "draw calls from game data",
];

/// The names of `suite`'s checks, in order.
pub fn names(suite: &str) -> Vec<&'static str> {
    if suite == "parity" {
        return PARITY.to_vec();
    }
    checks()
        .filter(|(s, _, _)| *s == suite)
        .map(|(_, name, _)| name)
        .collect()
}

/// Runs the check called `name`.
pub async fn run(name: &str, context: &Context<'_>) -> Outcome {
    match checks().find(|(_, n, _)| *n == name) {
        Some((_, _, run)) => run(context).await,
        None => Err(Problem::Fail(format!("no render check {name:?}"))),
    }
}
