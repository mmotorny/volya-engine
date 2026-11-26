// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The 2D layer's platform-free part
//! (docs/behavior/title-menu.md): textured rectangles in a 1024 × 768 UI
//! space, and the draw list a frame is.

use std::ops::Range;

use crate::{Texture, WorldFrame};

/// Size of the UI space in units.
pub const UI_WIDTH: f32 = 1024.0;
pub const UI_HEIGHT: f32 = 768.0;

/// An axis-aligned rectangle: UI units for screen positions, texels for
/// texture regions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The whole UI space.
    pub const SCREEN: Self = Self::new(0.0, 0.0, UI_WIDTH, UI_HEIGHT);
}

/// One textured rectangle.
#[derive(Clone, Debug)]
pub struct Quad {
    /// Where to draw, in UI units.
    pub rect: Rect,
    pub texture: Texture,
    /// The texture region to draw, in texels of the allocated texture.
    pub source: Rect,
    /// Multiplies the texel colour: RGBA, 255 = unchanged.
    pub color: [u8; 4],
    /// When set, the corners to draw at instead of `rect`, in UI units, in
    /// the order top-left, top-right, bottom-right, bottom-left of `source`
    /// (e.g. a rotated picture).
    pub corners: Option<[[f32; 2]; 4]>,
    /// When set, a colour per corner (in the order of `corners`) instead of
    /// `color`, interpolated across the quad.
    pub corner_colors: Option<[[u8; 4]; 4]>,
}

impl Quad {
    /// The whole of `texture`, untinted.
    pub fn new(rect: Rect, texture: &Texture) -> Self {
        let (width, height) = texture.size();
        Self {
            rect,
            texture: texture.clone(),
            source: Rect::new(0.0, 0.0, width as f32, height as f32),
            color: [255; 4],
            corners: None,
            corner_colors: None,
        }
    }
}

/// What to draw in one frame: the 3D world, if any, then the quads over
/// it, back to front.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub world: Option<WorldFrame>,
    pub quads: Vec<Quad>,
    /// The display's gamma ramp the picture is shown through
    /// (docs/behavior/display-gamma.md): entry *i* / 65535 for channel
    /// value *i*, the same for R, G and B; `None` for the identity.
    pub ramp: Option<[u16; 256]>,
    /// Without a world: the main menu drawn through the off-screen target
    /// with its lens (docs/behavior/title-menu.md 4.6).
    pub lens: Option<MenuLens>,
}

/// A main-menu frame through the off-screen target
/// (docs/behavior/title-menu.md 4.6, frame-presentation.md 3.1): quads
/// `..drawn` (the menu) into the target, quad `drawn` (the lens's offset
/// map, in UI units of the target) into the distortion target, cleared to
/// 127 / 255 first, then the presentation through `postprocess_d` with
/// the quads after it (the cursor) over it.
#[derive(Clone, Copy, Debug)]
pub struct MenuLens {
    pub drawn: usize,
    pub presentation: crate::present::Presentation,
}

/// Bytes per vertex: position (2 × f32), texture coordinates (2 × f32),
/// colour (4 × u8).
pub(crate) const VERTEX_BYTES: u64 = 20;
pub(crate) const VERTICES_PER_QUAD: u32 = 6;

/// A world frame's distortion pass (particle-drawing.md 6.2).
#[derive(Clone, Copy)]
pub(crate) struct Distortion {
    /// The frame has a distortion list (frame-presentation.md 3.1).
    pub pass: bool,
    /// The pass was drawn into the distortion target.
    pub drawn: bool,
}

/// The vertices of `quads` and their draw batches: runs of quads sharing
/// a texture (by `same`), as (index of a quad with that texture, vertex
/// range).
pub(crate) fn quad_vertices(
    quads: &[Quad],
    same: impl Fn(&Texture, &Texture) -> bool,
) -> (Vec<u8>, Vec<(usize, Range<u32>)>) {
    let mut data = Vec::with_capacity(quads.len() * (VERTICES_PER_QUAD * 20) as usize);
    let mut batches: Vec<(usize, Range<u32>)> = Vec::new();
    for (i, quad) in quads.iter().enumerate() {
        let (tw, th) = quad.texture.size();
        let (r, s) = (quad.rect, quad.source);
        let (u0, v0) = (s.x / tw as f32, s.y / th as f32);
        let (u1, v1) = ((s.x + s.width) / tw as f32, (s.y + s.height) / th as f32);
        let [tl, tr, br, bl] = quad.corners.unwrap_or([
            [r.x, r.y],
            [r.x + r.width, r.y],
            [r.x + r.width, r.y + r.height],
            [r.x, r.y + r.height],
        ]);
        // Two triangles: top-left, bottom-left, top-right; top-right,
        // bottom-left, bottom-right.
        let [ctl, ctr, cbr, cbl] = quad.corner_colors.unwrap_or([quad.color; 4]);
        for ([x, y], u, v, color) in [
            (tl, u0, v0, ctl),
            (bl, u0, v1, cbl),
            (tr, u1, v0, ctr),
            (tr, u1, v0, ctr),
            (bl, u0, v1, cbl),
            (br, u1, v1, cbr),
        ] {
            for value in [x, y, u, v] {
                data.extend_from_slice(&value.to_le_bytes());
            }
            data.extend_from_slice(&color);
        }
        let end = (i as u32 + 1) * VERTICES_PER_QUAD;
        match batches.last_mut() {
            Some((first, range)) if same(&quads[*first].texture, &quad.texture) => {
                range.end = end;
            }
            _ => batches.push((i, end - VERTICES_PER_QUAD..end)),
        }
    }
    (data, batches)
}
