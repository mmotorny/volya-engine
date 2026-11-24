// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The sky's per-frame values (docs/behavior/sky.md): the sky box's colour
//! (section 5.3), the cloud dome's colours (6.1, 6.3) and the sun sprite
//! (7.2–7.4). The lens flares and the gradient (7.4) are not drawn: they fade
//! with the sun's occlusion by the collision mesh, which is not loaded.

use crate::core::camera::Vec3;
use crate::core::config::{Config, value};
use crate::core::weather::Environment;
use crate::render::sky::SkyFrame;

/// The sun sprite of a lens-flare section (section 7.2).
#[derive(Clone, Debug, PartialEq)]
pub struct SunSource {
    /// Texture name (`source_texture`).
    pub texture: String,
    pub radius: f32,
    pub ignore_color: bool,
}

/// A lens-flare section (section 7.2): its fade rates and its sun sprite.
#[derive(Clone, Debug, PartialEq)]
pub struct Flares {
    /// The section's name; empty for none.
    pub name: String,
    /// λ↑ = 1/(`blend_rise_time` + 10⁻⁷), rise per second.
    pub rise: f32,
    /// λ↓ = 1/(`blend_down_time` + 10⁻⁷), fall per second.
    pub down: f32,
    /// `None` when it has no `source`.
    pub sun: Option<SunSource>,
}

impl Flares {
    /// Section `name` of `config`. A section that does not exist (or an
    /// empty name) has no sun and reads its times as 0 (the specs do not say
    /// what its rates are; our choice).
    pub fn read(config: &Config, name: &str) -> Self {
        let section = config.section(name);
        let get = |key: &str| section.and_then(|s| s.get(key).flatten()).unwrap_or("");
        let rate = |key: &str| 1.0 / (value::float(get(key)).max(0.0) + 1e-7);
        Self {
            name: name.to_owned(),
            rise: rate("blend_rise_time"),
            down: rate("blend_down_time"),
            sun: value::bool(get("source")).then(|| SunSource {
                texture: crate::core::weather::texture_name(get("source_texture")),
                radius: value::float(get("source_radius")),
                ignore_color: value::bool(get("source_ignore_color")),
            }),
        }
    }
}

/// The section whose lens-flare section is in effect: A's if t < 0.5, else
/// B's (3.3).
pub fn flares_in_effect(a: usize, b: usize, t: f32) -> usize {
    if t < 0.5 { a } else { b }
}

/// The fade a of the sun, flares and gradient (section 7.3), and the
/// lens-flare section taken, as the index of the hourly section that names
/// it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SunFade {
    pub fade: f32,
    /// `None` before the level's first frame.
    pub taken: Option<usize>,
}

impl SunFade {
    /// One frame of `dt` real seconds with game time `k_t` times faster than
    /// real time, the lens-flare section of hourly section `in_effect` in
    /// effect; `flares` per hourly section. On the first frame the section is
    /// taken and a stays 0; then a rises by λ↑·dt·k_t up to 1. When the
    /// section in effect (by name) changes, a falls by the old section's
    /// λ↓·dt·k_t; on the frame it reaches 0 the new section is taken, and a
    /// rises from the next frame (our choice).
    pub fn frame(&mut self, in_effect: usize, flares: &[Flares], dt: f32, k_t: f32) {
        let Some(taken) = self.taken else {
            self.taken = Some(in_effect);
            return;
        };
        if flares[taken].name == flares[in_effect].name {
            self.taken = Some(in_effect);
            self.fade = (self.fade + flares[in_effect].rise * dt * k_t).min(1.0);
        } else {
            self.fade -= flares[taken].down * dt * k_t;
            if self.fade <= 0.0 {
                self.fade = 0.0;
                self.taken = Some(in_effect);
            }
        }
    }
}

/// The clamped byte of x: clamp(trunc(255·x), 0, 255).
pub fn byte(x: f32) -> u8 {
    (255.0 * x).trunc().clamp(0.0, 255.0) as u8
}

fn bytes(v: Vec3, a: f32) -> [u8; 4] {
    [byte(v[0]), byte(v[1]), byte(v[2]), byte(a)]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Colour 0 of the clouds: bytes of (½w₀.x + ½, ½w₀.z + ½, ½w₁.z + ½,
/// ½w₁.x + ½) with w₀ = (−sin 45°, 0, cos 45°), w₁ = (−sin 67.5°, 0,
/// cos 67.5°) (6.3).
pub fn cloud_directions() -> [u8; 4] {
    let (w0, w1) = (45f32.to_radians(), 67.5f32.to_radians());
    let (w0x, w0z, w1x, w1z) = (-w0.sin(), w0.cos(), -w1.sin(), w1.cos());
    [w0x, w0z, w1z, w1x].map(|c| byte(0.5 * c + 0.5))
}

/// The camera of a frame as the sun sprite needs it.
pub struct SkyCamera {
    pub eye: Vec3,
    /// Viewing direction F and right vector r (start-position.md 6).
    pub forward: Vec3,
    pub right: Vec3,
}

/// The sky's values for `environment`, blended from hourly section
/// `section` and the one after it, seen from `camera`; the sun sprite from
/// `sun` (with the hourly section whose lens-flare section it is) faded by
/// `fade` (0 … 1).
pub fn sky_frame(
    environment: &Environment,
    section: usize,
    camera: &SkyCamera,
    sun: Option<(usize, &SunSource)>,
    fade: f32,
) -> SkyFrame {
    let e = environment;
    let clouds = (e.clouds_opacity.abs() >= 0.001)
        .then(|| (cloud_directions(), bytes(e.clouds_color, e.clouds_opacity)));
    SkyFrame {
        section,
        sun_section: sun.map_or(section, |(s, _)| s),
        color: bytes(e.sky_color, e.t),
        rotation: e.sky_rotation,
        clouds,
        sun: sun.and_then(|(_, s)| sun_sprite(e, camera, s, fade)),
    }
}

/// The sun sprite's corners and colour (7.3, 7.4); `None` when F · D_s ≤
/// 0.01.
fn sun_sprite(
    e: &Environment,
    camera: &SkyCamera,
    sun: &SunSource,
    fade: f32,
) -> Option<([[f32; 3]; 4], [u8; 4])> {
    let d = e.sun_dir.map(|c| -c);
    let f = camera.forward;
    let c = dot(f, d);
    if c <= 0.01 {
        return None;
    }
    let q = 0.75 * e.far_plane;
    let centre: Vec3 = std::array::from_fn(|i| camera.eye[i] + q / c * d[i]);
    let h = sun.radius * q;
    let x = camera.right;
    let y = cross(x, f);
    let corner = |sx: f32, sy: f32| -> [f32; 3] {
        std::array::from_fn(|i| centre[i] + sx * h * x[i] + sy * h * y[i])
    };
    let rgb = if sun.ignore_color {
        [1.0; 3]
    } else {
        e.sun_color
    };
    Some((
        [
            corner(1.0, -1.0),
            corner(1.0, 1.0),
            corner(-1.0, -1.0),
            corner(-1.0, 1.0),
        ],
        bytes(rgb, fade),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment() -> Environment {
        // sky.md section 4's start values.
        Environment {
            t: 0.5,
            ambient: [0.04814, 0.05170, 0.05525],
            hemi_color: [0.37496, 0.39573, 0.41801, 1.0],
            sun_color: [0.759, 0.380, 0.102],
            sun_dir: [0.91318, -0.20791, 0.35054],
            fog_color: [0.31374, 0.31702, 0.32380],
            fog_density: 0.97604,
            fog_distance: 350.0,
            fog_near: 7.1284,
            fog_far: 346.5,
            far_plane: 254.478,
            sky_color: [0.68043, 0.69522, 0.71001],
            sky_rotation: 0.0,
            clouds_color: [0.44667, 0.33150, 0.31535],
            clouds_opacity: 0.5,
        }
    }

    #[test]
    fn start_values() {
        assert_eq!(cloud_directions(), [37, 217, 176, 9]);
        let sun = SunSource {
            texture: "fx\\fx_sun_rise".into(),
            radius: 0.03,
            ignore_color: false,
        };
        let start = SkyCamera {
            eye: [-246.7263, -23.26326, -134.43387],
            forward: [0.984797, -0.0043165, 0.173655],
            right: [0.173657, 0.0, -0.984806],
        };
        let f = sky_frame(&environment(), 4, &start, Some((5, &sun)), 0.0);
        assert_eq!((f.section, f.sun_section), (4, 5));
        assert_eq!(f.color, [173, 177, 181, 127]);
        assert_eq!(f.clouds, Some(([37, 217, 176, 9], [113, 84, 80, 127])));
        assert_eq!(f.sun, None, "the sun is behind the player");
        // Facing the sun: the sprite sits at view depth 0.75 × far.
        let d = [-0.91318, 0.20791, -0.35054];
        let toward = SkyCamera {
            eye: [0.0; 3],
            forward: d,
            right: {
                let r = cross([0.0, 1.0, 0.0], d);
                let l = dot(r, r).sqrt();
                r.map(|c| c / l)
            },
        };
        let (corners, color) = sky_frame(&environment(), 4, &toward, Some((4, &sun)), 1.0)
            .sun
            .unwrap();
        assert_eq!(color, [193, 96, 26, 255]);
        let centre: Vec3 = std::array::from_fn(|i| corners.iter().map(|c| c[i]).sum::<f32>() / 4.0);
        assert!((dot(centre, d) - 0.75 * 254.478).abs() < 1e-2);
        let half = 0.03 * 0.75 * 254.478;
        let side: f32 = (0..3)
            .map(|i| (corners[0][i] - corners[2][i]).powi(2))
            .sum::<f32>()
            .sqrt();
        assert!((side - 2.0 * half).abs() < 1e-3);
        let mut e = environment();
        e.clouds_opacity = 0.0005;
        assert_eq!(sky_frame(&e, 4, &start, None, 0.0).clouds, None);
    }

    #[test]
    fn the_fade_rises_then_falls_for_a_new_section() {
        let config = volya_engine_formats::ltx::parse(
            b"[rise]\nsource = on\nsource_texture = fx\\fx_sun_rise.tga\nsource_radius = 0.03\n\
              blend_rise_time = 300\nblend_down_time = 100\n\
              [day]\nblend_rise_time = 200\nblend_down_time = 200\n",
        )
        .unwrap();
        let flares = [
            Flares::read(&config, "rise"),
            Flares::read(&config, "rise"),
            Flares::read(&config, "day"),
        ];
        assert_eq!(flares[0].sun.as_ref().unwrap().texture, "fx\\fx_sun_rise");
        assert!(flares[2].sun.is_none());
        let mut a = SunFade::default();
        // First frame: taken, still 0.
        a.frame(0, &flares, 1.0, 10.0);
        assert_eq!((a.fade, a.taken), (0.0, Some(0)));
        // 30 real seconds at k_t = 10 fade in [rise] (300 s).
        for _ in 0..15 {
            a.frame(0, &flares, 1.0, 10.0);
        }
        assert!((a.fade - 0.5).abs() < 1e-5);
        // The same section named by the next hour: still rising.
        a.frame(1, &flares, 1.0, 10.0);
        assert!((a.fade - 1.0 / 30.0 * 16.0).abs() < 1e-5);
        assert_eq!(a.taken, Some(1));
        // Another section: falls at [rise]'s λ↓ (100 s), then takes [day].
        a.frame(2, &flares, 1.0, 10.0);
        assert!((a.fade - (16.0 / 30.0 - 0.1)).abs() < 1e-5);
        assert_eq!(a.taken, Some(1));
        for _ in 0..5 {
            a.frame(2, &flares, 1.0, 10.0);
        }
        assert_eq!((a.fade, a.taken), (0.0, Some(2)));
        a.frame(2, &flares, 1.0, 10.0);
        assert!((a.fade - 0.05).abs() < 1e-5);
    }
}
