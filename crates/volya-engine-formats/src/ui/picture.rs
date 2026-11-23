// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Which texture a static's picture shows and where it lands on screen
//! (docs/formats/ui-layout.md, sections 5.2–5.4).

use super::Color;
use super::layout::{Rect, StaticDesc, TextureRef};
use super::texture_desc::TextureDescriptions;

/// A picture's texture: a texture path (relative to `textures\`, no
/// extension; resolve its kind with section 6.3) and the pixel rectangle, if
/// any.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub texture: String,
    pub rect: Option<Rect>,
}

impl Picture {
    /// Section 5.2: the value is first looked up as a texture id (exact
    /// match), giving that description's file and rectangle; otherwise it is
    /// a texture path with no rectangle. The `texture` child's own rectangle,
    /// when set, overrides either. A description rectangle with zero width or
    /// height counts as unset (section 5.3: an empty texture rectangle means
    /// the whole texture).
    pub fn resolve(texture: &TextureRef, descriptions: &TextureDescriptions) -> Self {
        let (path, rect) = match descriptions.get(&texture.name) {
            Some(d) => (
                d.file.clone(),
                Some(d.rect).filter(|r| r.width != 0.0 && r.height != 0.0),
            ),
            None => (texture.name.clone(), None),
        };
        Self {
            texture: path,
            rect: texture.rect.or(rect),
        }
    }
}

/// A screen-space quad: corners in pixels, texture coordinates and the vertex
/// colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    /// Top-left and bottom-right corners in screen pixels.
    pub min: (f32, f32),
    pub max: (f32, f32),
    /// Texture coordinates at `min` and at `max`.
    pub uv_min: (f32, f32),
    pub uv_max: (f32, f32),
    pub color: Color,
}

/// A rotated quad: four corners in screen pixels in the order (0, 0),
/// (w, 0), (w, h), (0, h) of the unrotated picture, with their texture
/// coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotatedQuad {
    pub corners: [(f32, f32); 4],
    pub uvs: [(f32, f32); 4],
    pub color: Color,
}

/// A picture placed in UI space, before mapping to the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PicturePlacement {
    /// Absolute top-left of the picture in units: the element's absolute
    /// top-left plus the texture offset.
    pub position: (f32, f32),
    /// Size in units (section 5.3).
    pub size: (f32, f32),
    /// Texture rectangle in pixels (the whole texture when none is set).
    pub texture_rect: Rect,
    /// The texture's own size in pixels.
    pub texture_size: (f32, f32),
    pub color: Color,
}

impl PicturePlacement {
    /// Places the picture of `desc` whose absolute rectangle is `absolute`
    /// (section 5.3): with `stretch` the element's size, otherwise the
    /// texture rectangle's size (one texture pixel = one unit).
    pub fn new(
        desc: &StaticDesc,
        absolute: Rect,
        picture: &Picture,
        texture_size: (u32, u32),
    ) -> Self {
        let texture_size = (texture_size.0 as f32, texture_size.1 as f32);
        let texture_rect =
            picture
                .rect
                .unwrap_or(Rect::new(0.0, 0.0, texture_size.0, texture_size.1));
        let size = if desc.stretch {
            (absolute.width, absolute.height)
        } else {
            (texture_rect.width, texture_rect.height)
        };
        Self {
            position: (
                absolute.x + desc.texture_offset.0,
                absolute.y + desc.texture_offset.1,
            ),
            size,
            texture_rect,
            texture_size,
            color: desc.texture.as_ref().map_or(Color::WHITE, |t| t.color),
        }
    }

    /// The non-rotated screen quad (section 5.3): the top-left corner is
    /// floored to a pixel, then everything is shifted by −0.5; the size is
    /// not rounded.
    pub fn quad(&self, (sx, sy): (f32, f32)) -> Quad {
        let left = (self.position.0 * sx).floor();
        let top = (self.position.1 * sy).floor();
        let r = self.texture_rect;
        let (tw, th) = self.texture_size;
        Quad {
            min: (left - 0.5, top - 0.5),
            max: (left + self.size.0 * sx - 0.5, top + self.size.1 * sy - 0.5),
            uv_min: (r.x / tw, r.y / th),
            uv_max: ((r.x + r.width) / tw, (r.y + r.height) / th),
            color: self.color,
        }
    }

    /// The rotatable picture (`heading`, section 5.4) at angle `angle`
    /// (radians; positive turns counter-clockwise on screen) about `pivot`
    /// (relative to the picture's top-left; `None` is its centre). On wide
    /// screens (`wide`, the `_16` test) it is narrowed horizontally by
    /// 0.8333 around the pivot. Corners are scaled without flooring or the
    /// half-pixel shift; texture coordinates get half a texel.
    pub fn rotated(
        &self,
        (sx, sy): (f32, f32),
        angle: f32,
        pivot: Option<(f32, f32)>,
        wide: bool,
    ) -> RotatedQuad {
        let (w, h) = self.size;
        let pivot = pivot.unwrap_or((w / 2.0, h / 2.0));
        let k = if wide { 0.8333 } else { 1.0 };
        let (sin, cos) = angle.sin_cos();
        let (px, py) = self.position;
        let corner = |cx: f32, cy: f32| {
            let (dx, dy) = (cx - pivot.0, cy - pivot.1);
            (
                (px + pivot.0 + k * (dx * cos + dy * sin)) * sx,
                (py + pivot.1 + (dy * cos - dx * sin)) * sy,
            )
        };
        let r = self.texture_rect;
        let (tw, th) = self.texture_size;
        let (u0, v0) = ((r.x + 0.5) / tw, (r.y + 0.5) / th);
        let (u1, v1) = ((r.x + r.width + 0.5) / tw, (r.y + r.height + 0.5) / th);
        RotatedQuad {
            corners: [
                corner(0.0, 0.0),
                corner(w, 0.0),
                corner(w, h),
                corner(0.0, h),
            ],
            uvs: [(u0, v0), (u1, v0), (u1, v1), (u0, v1)],
            color: self.color,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;
    use crate::ui::{ColorDefs, Resolution};
    use crate::xml::parse;

    fn descriptions() -> TextureDescriptions {
        let mut d = TextureDescriptions::default();
        d.add_file(
            &parse(
                b"<ui_texture><file_name>ui\\ui_mainMenu</file_name>\
                <texture id=\"ui_menu_backgraund\" x=\"0\" y=\"0\" width=\"1024\" height=\"768\" />\
                <texture id=\"ui_menu_grating_l\" x=\"865\" y=\"878\" width=\"46\" height=\"70\" />\
                <texture id=\"ui_menu_bigroll_l\" x=\"723\" y=\"878\" width=\"86\" height=\"86\" />\
                </ui_texture>",
            )
            .unwrap(),
        )
        .unwrap();
        d
    }

    fn element(xml: &str) -> StaticDesc {
        let doc = parse(xml.as_bytes()).unwrap();
        StaticDesc::read(doc.root().unwrap(), &ColorDefs::default())
    }

    fn place(xml: &str, parent: (f32, f32), size: (u32, u32)) -> (StaticDesc, PicturePlacement) {
        let s = element(xml);
        let picture = Picture::resolve(s.texture.as_ref().unwrap(), &descriptions());
        let abs = s.window.absolute(parent.0, parent.1);
        let p = PicturePlacement::new(&s, abs, &picture, size);
        (s, p)
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 2e-3 && (a.1 - b.1).abs() < 2e-3
    }

    const HD: Resolution = Resolution::new(1920, 1080);
    const XGA: Resolution = Resolution::new(1024, 768);

    #[test]
    fn background_and_videos() {
        // Section 5.3 test vectors.
        let (_, p) = place(
            "<background x=\"0\" y=\"0\" width=\"1024\" height=\"768\">\
            <texture>ui_menu_backgraund</texture></background>",
            (0.0, 0.0),
            (1024, 1024),
        );
        let q = p.quad(XGA.scale());
        assert_eq!((q.min, q.max), ((-0.5, -0.5), (1023.5, 767.5)));
        assert_eq!((q.uv_min, q.uv_max), ((0.0, 0.0), (1.0, 0.75)));
        assert_eq!(q.color, Color::WHITE);
        let q = p.quad(HD.scale());
        assert_eq!((q.min, q.max), ((-0.5, -0.5), (1919.5, 1079.5)));

        let (s, p) = place(
            "<back_movie x=\"148\" y=\"0\" width=\"876\" height=\"400\">\
            <texture x=\"0\" y=\"0\" width=\"876\" height=\"400\">ui\\ui_mm_main_back_crop</texture>\
            </back_movie>",
            (0.0, 0.0),
            (1024, 512),
        );
        let picture = Picture::resolve(s.texture.as_ref().unwrap(), &descriptions());
        assert_eq!(picture.texture, "ui\\ui_mm_main_back_crop");
        let q = p.quad(XGA.scale());
        assert!(close(q.uv_max, (0.855_468_8, 0.78125)));
        assert_eq!(p.size, (876.0, 400.0));

        let (_, p) = place(
            "<fire_movie x=\"525\" y=\"459\" width=\"128\" height=\"256\">\
            <texture>ui\\ui_mm_fire</texture></fire_movie>",
            (0.0, 0.0),
            (128, 256),
        );
        let q = p.quad(XGA.scale());
        assert_eq!((q.uv_min, q.uv_max), ((0.0, 0.0), (1.0, 1.0)));
        assert_eq!(p.size, (128.0, 256.0));
    }

    #[test]
    fn grating_draws_at_texture_size() {
        // Section 5.3: bar at y = 8, absolute top-left (599, 431).
        let (_, p) = place(
            "<left_grating x=\"0\" y=\"4\" width=\"55\" height=\"67\">\
            <texture>ui_menu_grating_l</texture></left_grating>",
            (599.0, 427.0),
            (1024, 1024),
        );
        assert_eq!(p.size, (46.0, 70.0));
        let q = p.quad(XGA.scale());
        assert_eq!((q.min, q.max), ((598.5, 430.5), (644.5, 500.5)));
        assert!(close(q.uv_min, (0.844_726_6, 0.857_421_9)));
        assert!(close(q.uv_max, (0.889_648_4, 0.925_781_3)));
        let q = p.quad(HD.scale());
        assert_eq!(q.min, (1122.5, 605.5));
        assert!(close(q.max, (1208.75, 703.9375)));
        let q = p.quad(Resolution::new(800, 600).scale());
        assert_eq!(q.min, (466.5, 335.5));
        assert!(close(q.max, (502.4375, 390.1875)));
    }

    #[test]
    fn rotates_cog_wheels() {
        // Section 5.4 test vectors: bar at (599, 419).
        let (s, p) = place(
            "<left_anim x=\"43\" y=\"37\" width=\"55\" height=\"55\" stretch=\"1\" heading=\"1\" \
            alignment=\"c\"><texture>ui_menu_bigroll_l</texture></left_anim>",
            (599.0, 419.0),
            (1024, 1024),
        );
        assert!(s.heading);
        assert_eq!(p.position, (614.5, 428.5));
        let r = p.rotated(XGA.scale(), FRAC_PI_2, None, XGA.is_wide());
        assert!(close(r.uvs[0], (0.706_543, 0.857_910_2)));
        assert!(close(r.uvs[2], (0.790_527_3, 0.941_894_5)));
        let expect = [
            (614.5, 483.5),
            (614.5, 428.5),
            (669.5, 428.5),
            (669.5, 483.5),
        ];
        for (got, want) in r.corners.iter().zip(expect) {
            assert!(close(*got, want), "{got:?} {want:?}");
        }
        let r = p.rotated(XGA.scale(), 0.3, None, false);
        let expect = [
            (607.601, 437.855),
            (660.145, 421.601),
            (676.399, 474.145),
            (623.855, 490.399),
        ];
        for (got, want) in r.corners.iter().zip(expect) {
            assert!(close(*got, want), "{got:?} {want:?}");
        }
        let r = p.rotated(HD.scale(), FRAC_PI_2, None, HD.is_wide());
        let expect = [
            (1160.783, 679.922),
            (1160.783, 602.578),
            (1246.717, 602.578),
            (1246.717, 679.922),
        ];
        for (got, want) in r.corners.iter().zip(expect) {
            assert!(close(*got, want), "{got:?} {want:?}");
        }
    }
}
