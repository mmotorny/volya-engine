// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What a screen draws in one frame, before any GPU work: textured quads in
//! output pixels, built for one output resolution (the UI rules floor
//! positions to pixels, so the result depends on it). [`crate::present`]
//! turns a [`Scene`] into a renderer draw list.
//!
//! Corners are in the convention of the UI specs (docs/formats/ui-layout.md
//! section 5.3, docs/formats/text-and-fonts.md section 3.3): output pixels
//! whose centres are at integer coordinates, so a picture covering pixels
//! 0 … *W* − 1 spans −0.5 … *W* − 0.5 and a glyph cell at integer *x*₀ spans
//! *x*₀ … *x*₀ + *w* with half-texel texture coordinates.

use std::sync::Arc;

use serde::Serialize;

use crate::core::camera::{Mat4, Vec3};
use crate::core::weather::Environment;
use crate::level::LevelWorld;
use volya_engine_formats::text::layout::GlyphQuad;
use volya_engine_formats::ui::picture::{Quad, RotatedQuad};
use volya_engine_formats::ui::{Color, Resolution};

/// A texture a sprite samples.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TextureId {
    /// The `.dds` file at this path of the game tree.
    Image { path: String },
    /// The Ogg Theora video at this path, played from its first frame at
    /// `start_ms` of the engine clock; `looping` restarts it at its end,
    /// otherwise its last frame stays.
    Video {
        path: String,
        start_ms: u64,
        looping: bool,
    },
    /// The splash picture (docs/behavior/startup.md section 3).
    Splash,
    /// A white texel (flat-coloured shapes).
    White,
}

/// One textured quad.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sprite {
    /// What the sprite is, for tests and debugging (e.g. `background`,
    /// `caption 0`).
    pub layer: String,
    pub texture: TextureId,
    /// Corners in output pixels (see the module docs), in the order
    /// top-left, top-right, bottom-right, bottom-left of the texture region.
    pub corners: [[f32; 2]; 4],
    /// Normalised texture coordinates of the top-left and bottom-right of
    /// the region.
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    /// Multiplies the texels (ARGB).
    #[serde(serialize_with = "argb")]
    pub color: Color,
    /// Per-corner colours replacing `color`, in the order of `corners`,
    /// interpolated across the quad.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "argb_corners"
    )]
    pub corner_colors: Option<[Color; 4]>,
}

fn argb<S: serde::Serializer>(c: &Color, s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq([c.a, c.r, c.g, c.b])
}

fn argb_corners<S: serde::Serializer>(c: &Option<[Color; 4]>, s: S) -> Result<S::Ok, S::Error> {
    let corners = c.unwrap_or([Color::NONE; 4]);
    s.collect_seq(corners.map(|c| [c.a, c.r, c.g, c.b]))
}

impl Sprite {
    /// A screen-aligned picture quad (ui-layout.md section 5.3).
    pub fn picture(layer: impl Into<String>, texture: TextureId, q: &Quad) -> Self {
        Self {
            layer: layer.into(),
            texture,
            corners: [
                [q.min.0, q.min.1],
                [q.max.0, q.min.1],
                [q.max.0, q.max.1],
                [q.min.0, q.max.1],
            ],
            uv_min: [q.uv_min.0, q.uv_min.1],
            uv_max: [q.uv_max.0, q.uv_max.1],
            color: q.color,
            corner_colors: None,
        }
    }

    /// A rotated picture (ui-layout.md section 5.4).
    pub fn rotated(layer: impl Into<String>, texture: TextureId, q: &RotatedQuad) -> Self {
        Self {
            layer: layer.into(),
            texture,
            corners: q.corners.map(|(x, y)| [x, y]),
            uv_min: [q.uvs[0].0, q.uvs[0].1],
            uv_max: [q.uvs[2].0, q.uvs[2].1],
            color: q.color,
            corner_colors: None,
        }
    }

    /// A glyph of a font whose texture is `texture`, `size` texels
    /// (text-and-fonts.md section 3.3). Glyphs take the top colour: only
    /// the gradient font `di`, which no screen here uses, differs at the
    /// bottom.
    pub fn glyph(
        layer: impl Into<String>,
        texture: TextureId,
        g: &GlyphQuad,
        size: (u32, u32),
    ) -> Self {
        let (uv_min, uv_max) = g.uv(size.0 as f32, size.1 as f32);
        Self {
            layer: layer.into(),
            texture,
            corners: [
                [g.min.0, g.min.1],
                [g.max.0, g.min.1],
                [g.max.0, g.max.1],
                [g.min.0, g.max.1],
            ],
            uv_min: [uv_min.0, uv_min.1],
            uv_max: [uv_max.0, uv_max.1],
            color: g.top_color,
            corner_colors: None,
        }
    }

    /// The axis-aligned bounds (min x, min y, max x, max y) of the corners.
    pub fn bounds(&self) -> [f32; 4] {
        let xs = self.corners.map(|c| c[0]);
        let ys = self.corners.map(|c| c[1]);
        let min = |v: [f32; 4]| v.into_iter().fold(f32::INFINITY, f32::min);
        let max = |v: [f32; 4]| v.into_iter().fold(f32::NEG_INFINITY, f32::max);
        [min(xs), min(ys), max(xs), max(ys)]
    }
}

/// The 3D world of a frame: a level's static world seen from a camera.
#[derive(Clone, Debug, Serialize)]
pub struct WorldScene {
    #[serde(skip)]
    pub world: Arc<LevelWorld>,
    pub eye: Vec3,
    /// The viewing direction.
    pub forward: Vec3,
    /// Vertical field of view, degrees.
    pub fov_y_degrees: f32,
    /// Row-vector convention (start-position.md sections 6, 7).
    pub view: Mat4,
    pub projection: Mat4,
    #[serde(skip)]
    pub environment: Environment,
    /// Seconds since the level was shown (the shaders' `time`).
    pub time: f32,
    #[serde(skip)]
    pub sky: Option<crate::render::sky::SkyFrame>,
    /// Hidden under an opaque picture: kept on the GPU but not drawn.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub covered: bool,
}

impl PartialEq for WorldScene {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.world, &other.world)
            && self.eye == other.eye
            && self.forward == other.forward
            && self.fov_y_degrees == other.fov_y_degrees
            && self.view == other.view
            && self.projection == other.projection
            && self.environment == other.environment
            && self.time == other.time
            && self.sky == other.sky
            && self.covered == other.covered
    }
}

/// Everything drawn in one frame: the world, if any, then the sprites over
/// it, back to front, over black.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Scene {
    #[serde(serialize_with = "resolution")]
    pub resolution: Resolution,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world: Option<WorldScene>,
    pub sprites: Vec<Sprite>,
}

fn resolution<S: serde::Serializer>(r: &Resolution, s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq([r.width, r.height])
}

impl Scene {
    pub fn new(resolution: Resolution) -> Self {
        Self {
            resolution,
            world: None,
            sprites: Vec::new(),
        }
    }

    /// The sprites whose layer is `layer`.
    pub fn layer<'a>(&'a self, layer: &'a str) -> impl Iterator<Item = &'a Sprite> + 'a {
        self.sprites.iter().filter(move |s| s.layer == layer)
    }

    /// The first sprite of `layer`.
    pub fn first(&self, layer: &str) -> Option<&Sprite> {
        self.sprites.iter().find(|s| s.layer == layer)
    }

    /// Clips the sprites from index `start` on to `rect` (output pixels:
    /// min x, min y, max x, max y): screen-aligned sprites are cut, their
    /// texture coordinates with them; those outside, and rotated ones not
    /// wholly inside, are dropped (a clipping window, ui-layout.md 4.3).
    pub fn clip_from(&mut self, start: usize, [x0, y0, x1, y1]: [f32; 4]) {
        let mut kept = Vec::with_capacity(self.sprites.len() - start);
        for mut s in self.sprites.drain(start..) {
            let c = s.corners;
            let aligned = c[0][1] == c[1][1] && c[1][0] == c[2][0] && c[0][0] == c[3][0];
            let [bx0, by0, bx1, by1] = s.bounds();
            if bx1 <= x0 || bx0 >= x1 || by1 <= y0 || by0 >= y1 {
                continue;
            }
            if bx0 >= x0 && bx1 <= x1 && by0 >= y0 && by1 <= y1 {
                kept.push(s);
                continue;
            }
            if !aligned {
                continue;
            }
            let (l, t, r, b) = (c[0][0], c[0][1], c[2][0], c[2][1]);
            let (nl, nt, nr, nb) = (l.max(x0), t.max(y0), r.min(x1), b.min(y1));
            let lerp = |a: f32, b: f32, f: f32| a + (b - a) * f;
            let fx = |x: f32| if r != l { (x - l) / (r - l) } else { 0.0 };
            let fy = |y: f32| if b != t { (y - t) / (b - t) } else { 0.0 };
            let (u0, v0, u1, v1) = (s.uv_min[0], s.uv_min[1], s.uv_max[0], s.uv_max[1]);
            s.uv_min = [lerp(u0, u1, fx(nl)), lerp(v0, v1, fy(nt))];
            s.uv_max = [lerp(u0, u1, fx(nr)), lerp(v0, v1, fy(nb))];
            s.corners = [[nl, nt], [nr, nt], [nr, nb], [nl, nb]];
            kept.push(s);
        }
        self.sprites.extend(kept);
    }

    /// The bounds of all sprites of `layer` together (as
    /// [`Sprite::bounds`]), `None` when there are none.
    pub fn layer_bounds(&self, layer: &str) -> Option<[f32; 4]> {
        self.layer(layer).map(Sprite::bounds).reduce(|a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        })
    }
}
