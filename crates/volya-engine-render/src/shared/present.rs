// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Frame presentation's platform-free part
//! (docs/behavior/frame-presentation.md): the screen-effect values, the
//! off-screen target's size and the presentation quad.

/// The screen-effect values the renderer uses (section 2.1), as stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenEffects {
    pub blur: f32,
    pub gray: f32,
    /// dual.h, dual.v: absolute values.
    pub dual: [f32; 2],
    pub noise_intensity: f32,
    pub noise_grain: f32,
    /// |value| + 10⁻⁷.
    pub noise_fps: f32,
    pub color_base: [u8; 3],
    pub color_gray: [u8; 3],
    pub color_add: [u8; 3],
}

/// The ten values of an effect set as the engine combines them (2.2),
/// colours as reals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectValues {
    pub blur: f32,
    pub gray: f32,
    pub dual: [f32; 2],
    pub noise_intensity: f32,
    pub noise_grain: f32,
    pub noise_fps: f32,
    pub color_base: [f32; 3],
    pub color_gray: [f32; 3],
    pub color_add: [f32; 3],
}

impl EffectValues {
    /// The engine's identity (2.1).
    pub const IDENTITY: Self = Self {
        blur: 0.0,
        gray: 0.0,
        dual: [0.0; 2],
        noise_intensity: 0.0,
        noise_grain: 1.0,
        noise_fps: 30.0,
        color_base: [0.5; 3],
        color_gray: [0.333; 3],
        color_add: [0.0; 3],
    };
}

/// The renderer's margin below which a value counts as off (2.4).
pub(crate) const OFF: f32 = 0.001;

impl ScreenEffects {
    /// The renderer's own defaults, in force after the target is (re)built
    /// until the first hand-over (2.3).
    pub const DEFAULTS: Self = Self {
        blur: 0.0,
        gray: 0.0,
        dual: [0.0; 2],
        noise_intensity: 0.0,
        noise_grain: 1.0,
        noise_fps: 25.0,
        color_base: [127; 3],
        color_gray: [85; 3],
        color_add: [0; 3],
    };

    /// The set as the engine hands it to the renderer (2.2 steps 4, 5)
    /// and the renderer stores it (2.1).
    pub fn handed_over(v: &EffectValues) -> Self {
        let grain = if v.noise_grain < 0.0 {
            1.0
        } else {
            v.noise_grain
        };
        let byte = |c: f32| (c * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
        Self {
            blur: v.blur,
            gray: v.gray,
            dual: v.dual.map(f32::abs),
            noise_intensity: v.noise_intensity,
            noise_grain: grain.clamp(0.001, 1000.0),
            noise_fps: v.noise_fps.abs() + 1e-7,
            color_base: v.color_base.map(byte),
            color_gray: v.color_gray.map(byte),
            color_add: v.color_add.map(byte),
        }
    }

    /// The identity handed over: what a level hands over while no effect
    /// contributes (2.2 step 3) and while the main menu shows.
    pub fn identity() -> Self {
        Self::handed_over(&EffectValues::IDENTITY)
    }

    /// Whether the effects count as active (2.4); the user interface never
    /// asks for the post-process here (its world is not drawn under the
    /// menu).
    pub fn active(&self) -> bool {
        !(self.blur <= OFF
            && self.gray <= OFF
            && self.noise_intensity <= OFF
            && self.dual.iter().all(|&d| d <= OFF)
            && self.color_base.iter().all(|&b| (125..=129).contains(&b))
            && self.color_add.iter().all(|&b| b <= 2))
    }
}

/// What a frame's presentation takes from the engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presentation {
    /// `r__supersample` as the last video restart applied it, 1 … 4.
    pub supersample: u32,
    /// `r1_pps_u`, `r1_pps_v`: the quad's offset in pixels.
    pub pps: [f32; 2],
    /// The engine's frame time, seconds (the noise's countdown, 3.2).
    pub frame_time: f32,
    /// The device resets so far (a video restart, a level's start): each
    /// rebuilds the target, which brings back the defaults (1.2, 2.3).
    pub resets: u32,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            supersample: 1,
            pps: [0.0; 2],
            frame_time: 0.0,
            resets: 0,
        }
    }
}

/// The off-screen target's size for a `width` × `height` screen and
/// `r__supersample` `s` (section 1.1): each axis floor(N √s + 0.5),
/// clamped to 128 … 2048, then reduced by 1 if odd.
pub fn target_size(width: u32, height: u32, s: u32) -> (u32, u32) {
    let root = (s.clamp(1, 4) as f32).sqrt();
    let axis = |n: u32| {
        let v = ((n as f32 * root + 0.5).floor() as u32).clamp(128, 2048);
        v - (v & 1)
    };
    (axis(width), axis(height))
}

/// One corner of the quad: its Direct3D 9 screen position and texture
/// coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    pub position: [f32; 2],
    pub tc0: [f32; 2],
    pub tc1: [f32; 2],
    pub tc2: [f32; 2],
}

/// The noise cell n (3.2), at least 1: the grain's clamp keeps it so.
pub(crate) fn noise_cell(grain: f32) -> u32 {
    ((grain * 256.0 + 1e-7).ceil() as u32).max(1)
}

/// The quad's corners (3.2) in the order left top, right top, left bottom,
/// right bottom, for a `screen` of W × H, the `target`'s W_t × H_t, the
/// offset `pps` (U, V) and the noise offsets (o_x, o_y). `effects.blur`
/// must already be 1 where H_t ≠ H.
pub fn quad(
    screen: (u32, u32),
    target: (u32, u32),
    effects: &ScreenEffects,
    [u, v]: [f32; 2],
    [ox, oy]: [u32; 2],
) -> [Corner; 4] {
    let (w, h) = (screen.0 as f32, screen.1 as f32);
    let beta = effects.blur;
    let [dh, dv] = effects.dual;
    let a0 = (0.5 + 0.5 * beta) / target.0 as f32;
    let b0 = (0.5 + 0.5 * beta) / target.1 as f32;
    let (a1, b1) = (1.0 + a0, 1.0 + b0);
    let (a1p, b1p) = (a1 - dh / 2.0, b1 - dv / 2.0);
    let (a0p, b0p) = (a0 + dh / 2.0, b0 + dv / 2.0);
    let n = noise_cell(effects.noise_grain);
    let x0 = (ox as f32 + 0.5) / n as f32;
    let y0 = (oy as f32 + 0.5) / n as f32;
    let x1 = (screen.0 / n) as f32 + x0 + 1.0;
    let y1 = (screen.1 / n) as f32 + y0 + 1.0;
    [
        Corner {
            position: [u, v],
            tc0: [a0, b0],
            tc1: [a0p, b0p],
            tc2: [x0, y0],
        },
        Corner {
            position: [u + w, v],
            tc0: [a1p, b0],
            tc1: [a1, b0p],
            tc2: [x1, y0],
        },
        Corner {
            position: [u, v + h],
            tc0: [a0, b1p],
            tc1: [a0p, b1],
            tc2: [x0, y1],
        },
        Corner {
            position: [u + w, v + h],
            tc0: [a1p, b1p],
            tc1: [a1, b1],
            tc2: [x1, y1],
        },
    ]
}

/// Bytes of the quad's constants.
pub(crate) const CONSTANTS: u64 = 12 * 16;

/// The quad's constants ([`CONSTANTS`]) for a `screen` of W × H: its
/// `corners` and the colour values of `e`.
pub(crate) fn quad_constants(
    screen: (u32, u32),
    corners: &[Corner; 4],
    e: &ScreenEffects,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(CONSTANTS as usize);
    let mut push = |v: [f32; 4]| {
        for x in v {
            bytes.extend_from_slice(&x.to_le_bytes());
        }
    };
    for c in corners {
        push([c.position[0], c.position[1], c.tc0[0], c.tc0[1]]);
        push([c.tc1[0], c.tc1[1], c.tc2[0], c.tc2[1]]);
    }
    let unit = |b: u8| f32::from(b) / 255.0;
    let alpha = |v: f32| ((1.0 - v) * 255.0).floor().clamp(0.0, 255.0) / 255.0;
    let [r, g, b] = e.color_base.map(unit);
    push([r, g, b, alpha(e.noise_intensity)]);
    let [r, g, b] = e.color_gray.map(unit);
    push([r, g, b, alpha(e.gray)]);
    let [r, g, b] = e.color_add.map(unit);
    push([r, g, b, 0.0]);
    push([screen.0 as f32, screen.1 as f32, 0.0, 0.0]);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_sizes() {
        // Section 1.1's table.
        let cases = [
            ((1024, 768), 1, (1024, 768)),
            ((1024, 768), 2, (1448, 1086)),
            ((1024, 768), 3, (1774, 1330)),
            ((1024, 768), 4, (2048, 1536)),
            ((800, 600), 2, (1130, 848)),
            ((1280, 1024), 2, (1810, 1448)),
            ((1920, 1080), 2, (2048, 1526)),
            ((2560, 1440), 1, (2048, 1440)),
        ];
        for ((w, h), s, want) in cases {
            assert_eq!(target_size(w, h, s), want, "{w}×{h} s {s}");
        }
    }

    /// Where pixel `i` of a screen `n` wide samples the target, in target
    /// texels with centres at integers (3.5).
    fn sample_at(c: &[Corner; 4], i: f32, n: f32, target: f32, axis: usize) -> f32 {
        let (a, b) = (c[0].tc0[axis], c[3].tc0[axis]);
        let at = c[0].position[axis];
        (a + (i - at) * (b - a) / n) * target - 0.5
    }

    #[test]
    fn the_shipped_presentation_samples_between_texels() {
        // 3.5 and section 5: 1024 × 768 at r__supersample 2, the identity.
        let target = target_size(1024, 768, 2);
        let mut e = ScreenEffects::identity();
        // H_t ≠ H: β = 1.
        e.blur = 1.0;
        let c = quad((1024, 768), target, &e, [0.0; 2], [0; 2]);
        for i in [0.0, 1.0, 100.0, 1023.0] {
            let p = sample_at(&c, i, 1024.0, 1448.0, 0);
            assert!((p - (0.5 + 1.414_062_5 * i)).abs() < 1e-3, "{i}: {p}");
            let p = sample_at(&c, i, 768.0, 1086.0, 1);
            assert!((p - (0.5 + 1.414_062_5 * i)).abs() < 1e-3, "{i}: {p}");
        }
        // No duality: both look-ups alike.
        assert!(c.iter().all(|c| c.tc0 == c.tc1));
    }

    #[test]
    fn at_screen_size_each_pixel_takes_one_texel() {
        let e = ScreenEffects::identity();
        let c = quad((1024, 768), (1024, 768), &e, [0.0; 2], [0; 2]);
        for i in [0.0, 511.0, 1023.0] {
            assert!((sample_at(&c, i, 1024.0, 1024.0, 0) - i).abs() < 1e-3);
        }
        // Duality: the second look-up shifted by half the values (3.2).
        let mut e = e;
        e.dual = [0.1, 0.2];
        let c = quad((1024, 768), (1024, 768), &e, [0.0; 2], [0; 2]);
        assert!((c[0].tc1[0] - c[0].tc0[0] - 0.05).abs() < 1e-6);
        assert!((c[3].tc0[1] - (1.0 + 1.0 / 1536.0 - 0.1)).abs() < 1e-6);
    }

    #[test]
    fn noise_coordinates() {
        // 3.2: n = 256 at grain 1; x₁ = ⌊W / n⌋ + x₀ + 1.
        let e = ScreenEffects::identity();
        let c = quad((1024, 768), (1024, 768), &e, [0.0; 2], [3, 7]);
        assert_eq!(c[0].tc2, [3.5 / 256.0, 7.5 / 256.0]);
        assert_eq!(c[3].tc2, [4.0 + 3.5 / 256.0 + 1.0, 3.0 + 7.5 / 256.0 + 1.0]);
        // The pps offset moves the positions only.
        let moved = quad((1024, 768), (1024, 768), &e, [0.5, -1.0], [3, 7]);
        assert_eq!(moved[3].position, [1024.5, 767.0]);
        assert_eq!(moved[3].tc0, c[3].tc0);
    }

    #[test]
    fn the_identity_and_the_defaults_are_inactive() {
        // 2.1, 2.4.
        let id = ScreenEffects::identity();
        assert_eq!(id.color_base, [128; 3]);
        assert_eq!(id.color_gray, [85; 3]);
        assert_eq!(id.color_add, [0; 3]);
        assert!((id.noise_fps - 30.0).abs() < 1e-5);
        assert!(!id.active() && !ScreenEffects::DEFAULTS.active());
        for change in [
            |e: &mut ScreenEffects| e.blur = 0.01,
            |e: &mut ScreenEffects| e.gray = 0.01,
            |e: &mut ScreenEffects| e.dual[1] = 0.01,
            |e: &mut ScreenEffects| e.noise_intensity = 0.01,
            |e: &mut ScreenEffects| e.color_base[2] = 130,
            |e: &mut ScreenEffects| e.color_base[0] = 124,
            |e: &mut ScreenEffects| e.color_add[1] = 3,
        ] {
            let mut e = id;
            change(&mut e);
            assert!(e.active(), "{e:?}");
        }
        // color_gray, grain and fps never make them active.
        let mut e = id;
        e.color_gray = [200; 3];
        e.noise_grain = 3.0;
        e.noise_fps = 1.0;
        assert!(!e.active());
        // Step 4, 5: a negative grain is 1, then clamped; dual stored absolute.
        let mut v = EffectValues::IDENTITY;
        v.noise_grain = -2.0;
        v.dual = [-0.5, 0.25];
        let h = ScreenEffects::handed_over(&v);
        assert_eq!((h.noise_grain, h.dual), (1.0, [0.5, 0.25]));
        v.noise_grain = 5000.0;
        assert_eq!(ScreenEffects::handed_over(&v).noise_grain, 1000.0);
    }
}
