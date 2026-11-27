// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The render self-test (docs/testing.md, Rendering): scenes drawn from
//! synthetic data, read back and checked pixel by pixel. It runs where the
//! renderer draws, in the engine worker through the web platform's
//! `render_selftest` op (headless Chrome's WebGL 2, the iOS Simulator's),
//! as the script runtime's self-test does; nothing draws natively.
//!
//! Each check gets a fresh [`Renderer`] from the platform and uses only the
//! renderer's API. A check fails by returning an
//! error, never by panicking, which would stop the engine.

use std::future::Future;
use std::pin::Pin;

use crate::{Color, DrawList, Renderer};

mod ui;
mod world;

/// Why a check didn't pass.
#[derive(Debug)]
pub enum Problem {
    /// It doesn't apply here (e.g. the device lacks what it checks).
    Skip(String),
    /// It failed, with what was found.
    Fail(String),
}

impl From<&str> for Problem {
    fn from(message: &str) -> Self {
        Self::Fail(message.into())
    }
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
    ("texels map one to one and sub-rectangles select", |m| {
        Box::pin(ui::texels_map_one_to_one_and_sub_rectangles_select(m))
    }),
    ("corners place a rotated quad", |m| {
        Box::pin(ui::corners_place_a_rotated_quad(m))
    }),
    ("axes scale independently", |m| {
        Box::pin(ui::axes_scale_independently(m))
    }),
    (
        "a smaller frame fits the target with its aspect kept",
        |m| Box::pin(ui::a_smaller_frame_fits_the_target_with_its_aspect_kept(m)),
    ),
    (
        "the gamma ramp maps every channel of the whole screen",
        |m| Box::pin(ui::the_gamma_ramp_maps_every_channel_of_the_whole_screen(m)),
    ),
    ("frames are timed on the GPU while timing is on", |m| {
        Box::pin(ui::frames_are_timed_on_the_gpu_while_timing_is_on(m))
    }),
    ("draws in order with tint and alpha", |m| {
        Box::pin(ui::draws_in_order_with_tint_and_alpha(m))
    }),
    ("BC textures by every path", |m| {
        Box::pin(ui::bc_textures_by_every_path(m))
    }),
    ("a large level is written in bands", |m| {
        Box::pin(ui::a_large_level_is_written_in_bands(m))
    }),
    (
        "converts uncompressed formats and uploads mips and cube faces",
        |m| Box::pin(ui::converts_uncompressed_formats_and_uploads_mips_and_cube_faces(m)),
    ),
    (
        "video frames map to the top left of a power-of-two texture",
        |m| Box::pin(ui::video_frames_map_to_the_top_left_of_a_power_of_two_texture(m)),
    ),
    ("video conversion cost", |m| {
        Box::pin(ui::video_conversion_cost(m))
    }),
    // The static world, sky, objects, details, particles, lights and
    // glows (world.rs).
    ("shared index ranges with different bases", |m| {
        Box::pin(world::shared_index_ranges_with_different_bases(m))
    }),
    ("every program builds and base coordinates decode", |m| {
        Box::pin(world::every_program_builds_and_base_coordinates_decode(m))
    }),
    ("vertex colour bytes swap", |m| {
        Box::pin(world::vertex_colour_bytes_swap(m))
    }),
    ("tree coordinates and constants", |m| {
        Box::pin(world::tree_coordinates_and_constants(m))
    }),
    ("sky shows where the world does not", |m| {
        Box::pin(world::sky_shows_where_the_world_does_not(m))
    }),
    ("objects draw quantized models", |m| {
        Box::pin(world::objects_draw_quantized_models(m))
    }),
    ("objects draw their detail window", |m| {
        Box::pin(world::objects_draw_their_detail_window(m))
    }),
    ("static progressive visuals draw their detail window", |m| {
        Box::pin(world::static_progressive_visuals_draw_their_detail_window(
            m,
        ))
    }),
    ("placed draws hide, show and move", |m| {
        Box::pin(world::placed_draws_hide_show_and_move(m))
    }),
    ("environment-mapped models", |m| {
        Box::pin(world::environment_mapped_models(m))
    }),
    ("detail objects are cut, lit and swayed", |m| {
        Box::pin(world::detail_objects_are_cut_lit_and_swayed(m))
    }),
    ("the presentation reduces a supersampled target", |m| {
        Box::pin(world::the_presentation_reduces_a_supersampled_target(m))
    }),
    ("particles queue, blend, cull and distort", |m| {
        Box::pin(world::particles_queue_blend_cull_and_distort(m))
    }),
    ("light passes add to the image", |m| {
        Box::pin(world::light_passes_add_to_the_image(m))
    }),
    ("glows add after the world", |m| {
        Box::pin(world::glows_add_after_the_world(m))
    }),
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

/// Fails the check (returns [`Problem::Fail`]) unless `condition` holds,
/// with the message given, else the condition's text.
#[macro_export]
#[doc(hidden)]
macro_rules! __selftest_ensure {
    ($condition:expr $(,)?) => {
        $crate::selftest::ensure!($condition, "{}", stringify!($condition))
    };
    ($condition:expr, $($message:tt)+) => {{
        let holds: bool = $condition;
        if !holds {
            return Err($crate::selftest::Problem::Fail(format!($($message)+)).into());
        }
    }};
}

/// Fails the check unless the two are equal, saying what was found.
#[macro_export]
#[doc(hidden)]
macro_rules! __selftest_ensure_eq {
    ($actual:expr, $expected:expr $(,)?) => {
        $crate::selftest::ensure_eq!($actual, $expected, "{}", stringify!($actual))
    };
    ($actual:expr, $expected:expr, $($message:tt)+) => {{
        let (actual, expected) = (&$actual, &$expected);
        if actual != expected {
            return Err($crate::selftest::Problem::Fail(format!(
                "{}: {actual:?} != {expected:?}",
                format_args!($($message)+)
            ))
            .into());
        }
    }};
}

/// Fails the check unless the two differ.
#[macro_export]
#[doc(hidden)]
macro_rules! __selftest_ensure_ne {
    ($actual:expr, $other:expr $(,)?) => {
        $crate::selftest::ensure_ne!($actual, $other, "{}", stringify!($actual))
    };
    ($actual:expr, $other:expr, $($message:tt)+) => {{
        let (actual, other) = (&$actual, &$other);
        if actual == other {
            return Err($crate::selftest::Problem::Fail(format!(
                "{}: {actual:?} == {other:?}",
                format_args!($($message)+)
            ))
            .into());
        }
    }};
}

/// Fails unless `actual` is [`near`] `expected`.
#[macro_export]
#[doc(hidden)]
macro_rules! __selftest_ensure_near {
    ($actual:expr, $expected:expr, $tolerance:expr, $($message:tt)+) => {{
        let (actual, expected, tolerance) = ($actual, $expected, $tolerance);
        $crate::selftest::ensure!(
            $crate::selftest::near(actual, expected, tolerance),
            "{}: {actual:?} != {expected:?} (±{tolerance})",
            format_args!($($message)+)
        );
    }};
}

pub use {
    __selftest_ensure as ensure, __selftest_ensure_eq as ensure_eq,
    __selftest_ensure_ne as ensure_ne, __selftest_ensure_near as ensure_near,
};

const BLACK: Color = Color::BLACK;

/// `list` drawn at `width` × `height` over black, as RGBA rows top to
/// bottom.
pub async fn capture(
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

/// `list` drawn at `size` and shown scaled to fit a `width` × `height`
/// target.
pub async fn capture_scaled(
    renderer: &mut Renderer,
    list: &DrawList,
    size: (u32, u32),
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Problem> {
    renderer
        .capture_scaled(list, size, width, height, BLACK)
        .await
        .map_err(|e| Problem::Fail(format!("capture: {e}")))
}

/// The pixel at (`x`, `y`) of an RGBA `image` `width` wide.
pub fn pixel(image: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [image[i], image[i + 1], image[i + 2], image[i + 3]]
}

/// Whether every channel of `actual` is within `tolerance` of `expected`'s.
pub fn near(actual: [u8; 4], expected: [u8; 4], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= tolerance)
}

/// Milliseconds from an arbitrary start (`Instant` panics in the browser's
/// wasm).
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1000.0
    }
}
