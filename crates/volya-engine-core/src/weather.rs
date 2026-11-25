// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Weather inputs of the static world (docs/behavior/static-shading.md
//! section 9, docs/behavior/sky.md section 3): the hourly sections of a
//! weather cycle, their blending at a time of day, and the environment
//! modifiers of `level.env_mod` around the camera.

pub mod wind;

use std::io;

use crate::config::{Config, Section, value};
use volya_engine_formats::env_mod::EnvModifier;

pub type Vec3 = [f32; 3];

/// Seconds in a day: section times wrap (static-shading.md 9.2).
pub const DAY: f32 = 86_400.0;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    [0, 1, 2].map(|i| lerp(a[i], b[i], t))
}

fn normalize(a: Vec3) -> Vec3 {
    let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if len > 0.0 { a.map(|c| c / len) } else { a }
}

/// The values read from one hourly section (static-shading.md 9.1, sky.md
/// 3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherSection {
    pub name: String,
    pub ambient: Vec3,
    pub hemi_color: [f32; 4],
    pub sun_color: Vec3,
    /// `L_sun_dir = (−sin λ·cos α, sin α, cos λ·cos α)` from `sun_dir`
    /// (altitude α°, longitude λ°): the direction the light travels.
    pub sun_dir: Vec3,
    pub fog_color: Vec3,
    pub fog_distance: f32,
    pub fog_density: f32,
    pub far_plane: f32,
    /// `sky_texture`, the sky cube map.
    pub sky_texture: String,
    /// `sky_color` × 0.5.
    pub sky_color: Vec3,
    /// `sky_rotation` in radians (absent = 0).
    pub sky_rotation: f32,
    pub clouds_texture: String,
    /// `(r·k/2, g·k/2, b·k/2)` of `clouds_color = r, g, b, a, k`.
    pub clouds_color: Vec3,
    /// `a` of `clouds_color`.
    pub clouds_opacity: f32,
    /// `flares`: the lens-flare section, or empty (sky.md 3.1).
    pub flares: String,
    /// `env_ambient`: the ambient set, if any
    /// (sound-weather-ambient.md 2.4).
    pub env_ambient: Option<String>,
    /// `rain_density`, clamped to 0 … 1 (sound-rain-thunder.md 2.1).
    pub rain_density: f32,
    /// `thunderbolt`: the collection, or empty for none; `bolt_period` and
    /// `bolt_duration` (s), read only with a collection, else 0 (2.1).
    pub thunderbolt: String,
    pub bolt_period: f32,
    pub bolt_duration: f32,
}

impl WeatherSection {
    /// Reads `section`: the keys of static-shading.md 9.1, sky.md 3.1 and
    /// sound-rain-thunder.md 2.1. A missing key is fatal (ltx-config.md 11),
    /// except the optional `sky_rotation` (absent = 0) and `env_ambient`
    /// (absent = no set, sound-weather-ambient.md 2.4); `bolt_period` and
    /// `bolt_duration` are read only with a collection.
    pub fn read(section: &Section) -> io::Result<Self> {
        let get = |key: &str| section.read(key);
        let floats3 = |key: &str| get(key).map(value::floats::<3>);
        let [altitude, longitude] = value::floats::<2>(get("sun_dir")?).map(f32::to_radians);
        // `k` absent = 0 (sky.md 3.1): a missing vector component is 0.
        let clouds = value::floats::<5>(get("clouds_color")?);
        let k = clouds[4] / 2.0;
        let thunderbolt = get("thunderbolt")?.trim();
        let bolt = |key: &str| -> io::Result<f32> {
            if thunderbolt.is_empty() {
                Ok(0.0)
            } else {
                get(key).map(value::float)
            }
        };
        Ok(Self {
            name: section.name().to_owned(),
            ambient: floats3("ambient")?,
            hemi_color: value::floats::<4>(get("hemi_color")?),
            sun_color: floats3("sun_color")?,
            sun_dir: [
                -longitude.sin() * altitude.cos(),
                altitude.sin(),
                longitude.cos() * altitude.cos(),
            ],
            fog_color: floats3("fog_color")?,
            fog_distance: value::float(get("fog_distance")?),
            fog_density: value::float(get("fog_density")?),
            far_plane: value::float(get("far_plane")?),
            sky_texture: texture_name(get("sky_texture")?),
            sky_color: floats3("sky_color")?.map(|c| c * 0.5),
            sky_rotation: value::float(section.get("sky_rotation").flatten().unwrap_or(""))
                .to_radians(),
            clouds_texture: texture_name(get("clouds_texture")?),
            clouds_color: [clouds[0] * k, clouds[1] * k, clouds[2] * k],
            clouds_opacity: clouds[3],
            flares: get("flares")?.trim().to_owned(),
            env_ambient: section
                .get("env_ambient")
                .flatten()
                .filter(|v| !v.is_empty())
                .map(str::to_owned),
            rain_density: value::float(get("rain_density")?).clamp(0.0, 1.0),
            thunderbolt: thunderbolt.to_owned(),
            bolt_period: bolt("bolt_period")?,
            bolt_duration: bolt("bolt_duration")?,
        })
    }
}

/// A texture name as the shader table's (static-shading.md 4.1): lower-cased,
/// a `.tga`, `.dds`, `.bmp` or `.ogm` extension removed.
pub fn texture_name(name: &str) -> String {
    let lower = name.to_lowercase();
    for ext in [".tga", ".dds", ".bmp", ".ogm"] {
        if let Some(stem) = lower.strip_suffix(ext) {
            return stem.to_owned();
        }
    }
    lower
}

/// `h:m:s` as seconds of the day (level-loading.md 2.6).
pub fn parse_time(text: &str) -> Option<f32> {
    let mut parts = text.trim().split(':').map(|p| p.trim().parse::<f32>().ok());
    let h = parts.next()??;
    let m = parts.next().flatten().unwrap_or(0.0);
    let s = parts.next().flatten().unwrap_or(0.0);
    Some(h * 3600.0 + m * 60.0 + s)
}

/// A weather cycle: its hourly sections by time of day, in time order.
#[derive(Clone, Debug, PartialEq)]
pub struct Weather {
    pub sections: Vec<(f32, WeatherSection)>,
}

impl Weather {
    /// The cycle of weather `name`: `[weathers] name = <cycle section>` of
    /// `config` (`environment.ltx`, included by `system.ltx`), whose keys are
    /// `h:m:s = <hourly section>`.
    pub fn load(config: &Config, name: &str) -> io::Result<Self> {
        let cycle = config
            .get("weathers", name)
            .flatten()
            .ok_or_else(|| invalid(format!("[weathers] has no {name}")))?;
        let cycle = config
            .section(cycle)
            .ok_or_else(|| invalid(format!("no weather cycle [{cycle}]")))?;
        let mut sections = cycle
            .entries()
            .map(|e| {
                let time = parse_time(e.key()).ok_or_else(|| {
                    invalid(format!("[{}] {}: not a time", cycle.name(), e.key()))
                })?;
                let name = e.value().unwrap_or("");
                let section = config
                    .section(name)
                    .ok_or_else(|| invalid(format!("no weather section [{name}]")))?;
                Ok((time % DAY, WeatherSection::read(section)?))
            })
            .collect::<io::Result<Vec<_>>>()?;
        if sections.is_empty() {
            return Err(invalid(format!(
                "weather cycle [{}] is empty",
                cycle.name()
            )));
        }
        sections.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(Self { sections })
    }

    /// The bracketing sections A, B of time of day `tau` (seconds) and the
    /// weight `t = clamp((τ − τ_A)/(τ_B − τ_A), 0, 1)`, times wrapping at a
    /// day (static-shading.md 9.2).
    pub fn bracket(&self, tau: f32) -> (&WeatherSection, &WeatherSection, f32) {
        let (a, b, t) = self.bracket_indices(tau);
        (&self.sections[a].1, &self.sections[b].1, t)
    }

    /// [`Weather::bracket`] as indices into `sections`: B is always the
    /// section after A, wrapping.
    pub fn bracket_indices(&self, tau: f32) -> (usize, usize, f32) {
        let tau = tau.rem_euclid(DAY);
        let n = self.sections.len();
        // The last section at or before τ (wrapping to the last of the day).
        let a = self
            .sections
            .iter()
            .rposition(|(time, _)| *time <= tau)
            .unwrap_or(n - 1);
        let b = (a + 1) % n;
        let (ta, tb) = (self.sections[a].0, self.sections[b].0);
        let span = (tb - ta).rem_euclid(DAY);
        let since = (tau - ta).rem_euclid(DAY);
        let t = if span > 0.0 {
            (since / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (a, b, t)
    }
}

/// The blended environment of a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    /// The hourly blend weight.
    pub t: f32,
    pub ambient: Vec3,
    pub hemi_color: [f32; 4],
    pub sun_color: Vec3,
    pub sun_dir: Vec3,
    pub fog_color: Vec3,
    pub fog_density: f32,
    pub fog_distance: f32,
    pub fog_near: f32,
    pub fog_far: f32,
    pub far_plane: f32,
    pub sky_color: Vec3,
    pub sky_rotation: f32,
    pub clouds_color: Vec3,
    pub clouds_opacity: f32,
}

/// Modifier fields after position, radius, power and far plane
/// (static-shading.md 9.2): fog colour, fog density, ambient, sky colour,
/// hemi colour.
fn modifier_fields(m: &EnvModifier) -> (Vec3, f32, Vec3, Vec3, Vec3) {
    let r = &m.rest;
    (
        [r[0], r[1], r[2]],
        r[3],
        [r[4], r[5], r[6]],
        [r[7], r[8], r[9]],
        [r[10], r[11], r[12]],
    )
}

impl Environment {
    /// Sections `a`, `b` blended by `t` with the modifiers in range of
    /// `camera` (static-shading.md 9.2, sky.md 3.3), the far plane scaled by
    /// `vis_distance` (`rs_vis_distance`).
    pub fn blend(
        a: &WeatherSection,
        b: &WeatherSection,
        t: f32,
        modifiers: &[EnvModifier],
        camera: Vec3,
        vis_distance: f32,
    ) -> Self {
        let mut w = 0.0;
        let (mut fog, mut density, mut ambient, mut sky, mut hemi, mut far) =
            ([0.0; 3], 0.0, [0.0; 3], [0.0; 3], [0.0; 3], 0.0);
        for m in modifiers {
            let Some(omega) = crate::camera::modifier_weight(m.position, m.radius, m.power, camera)
            else {
                continue;
            };
            let (f, d, am, s, h) = modifier_fields(m);
            for i in 0..3 {
                fog[i] += omega * f[i];
                ambient[i] += omega * am[i];
                sky[i] += omega * s[i];
                hemi[i] += omega * h[i];
            }
            density += omega * d;
            far += omega * m.far_plane;
            w += omega;
        }
        let with = |base: Vec3, m: Vec3| [0, 1, 2].map(|i| (base[i] + m[i]) / (1.0 + w));
        let hemi_lerp = [0, 1, 2, 3].map(|i| lerp(a.hemi_color[i], b.hemi_color[i], t));
        let hemi_rgb = with([hemi_lerp[0], hemi_lerp[1], hemi_lerp[2]], hemi);
        let fog_density = (lerp(a.fog_density, b.fog_density, t) + density) / (1.0 + w);
        let fog_distance = lerp(a.fog_distance, b.fog_distance, t);
        Self {
            t,
            ambient: with(lerp3(a.ambient, b.ambient, t), ambient),
            hemi_color: [hemi_rgb[0], hemi_rgb[1], hemi_rgb[2], hemi_lerp[3]],
            sun_color: lerp3(a.sun_color, b.sun_color, t),
            sun_dir: normalize(lerp3(a.sun_dir, b.sun_dir, t)),
            fog_color: with(lerp3(a.fog_color, b.fog_color, t), fog),
            fog_density,
            fog_distance,
            fog_near: (1.0 - fog_density) * fog_distance * 0.85,
            fog_far: 0.99 * fog_distance,
            far_plane: (lerp(a.far_plane, b.far_plane, t) + far) * vis_distance / (1.0 + w),
            sky_color: with(lerp3(a.sky_color, b.sky_color, t), sky),
            sky_rotation: lerp(a.sky_rotation, b.sky_rotation, t),
            clouds_color: lerp3(a.clouds_color, b.clouds_color, t),
            clouds_opacity: lerp(a.clouds_opacity, b.clouds_opacity, t),
        }
    }

    /// The fog colour as the fixed fog unit uses it:
    /// `trunc(clamp(255·c, 0, 255))/255` (static-shading.md 9.3).
    pub fn fog_rgb(&self) -> Vec3 {
        self.fog_color
            .map(|c| (255.0 * c).clamp(0.0, 255.0).trunc() / 255.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Config {
        volya_engine_formats::ltx::parse(
            b"[weathers]\ndefault = cycle\n[cycle]\n05:00:00 = w5\n06:00:00 = w6\n\
              [w5]\nambient = 0.1, 0.1, 0.1\nhemi_color = 0.7, 0.8, 0.9, 1\n\
              sun_color = 0.7, 0.284, 0.096\nsun_dir = -10, 291\nfog_color = 0.3, 0.3, 0.4\n\
              fog_distance = 350\nfog_density = 0.9\nfar_plane = 350\nsky_texture = sky\\a.dds\n\
              sky_color = 0.8, 0.8, 0.8\nclouds_texture = sky\\c\nclouds_color = 0.4, 0.4, 0.4, 0.5, 2\n\
              flares =\nrain_density = 0\nthunderbolt =\n\
              [w6]\nambient = 0.2, 0.2, 0.2\nhemi_color = 0.7, 0.8, 0.9, 0\n\
              sun_color = 0.818, 0.476, 0.108\nsun_dir = -14, 291\nfog_color = 0.3, 0.3, 0.4\n\
              fog_distance = 350\nfog_density = 0.9\nfar_plane = 300\nsky_texture = sky\\b\n\
              sky_color = 1, 1, 1\nclouds_texture = sky\\c\nclouds_color = 0.4, 0.4, 0.4, 0.5\n\
              flares =\nrain_density = 0\nthunderbolt =\n",
        )
        .unwrap()
    }

    /// ltx-config.md 11: a key the sections are read for, missing, is fatal;
    /// the optional `sky_rotation` and `env_ambient` are not.
    #[test]
    fn a_missing_key_is_fatal() {
        let config = sample();
        let w5 = config.section("w5").unwrap();
        assert!(WeatherSection::read(w5).is_ok());
        let text = String::from_utf8(
            b"[w]\nambient = 0\nhemi_color = 0\nsun_color = 0\nsun_dir = 0\nfog_color = 0\n\
              fog_distance = 0\nfog_density = 0\nsky_texture = a\nsky_color = 0\n\
              clouds_texture = c\nclouds_color = 0\nflares =\nrain_density = 0\nthunderbolt =\n"
                .to_vec(),
        )
        .unwrap();
        let config = volya_engine_formats::ltx::parse(text.as_bytes()).unwrap();
        let err = WeatherSection::read(config.section("w").unwrap()).unwrap_err();
        assert!(err.to_string().contains("far_plane"), "{err}");
        // A collection makes `bolt_period` read.
        let with = text.replace(
            "thunderbolt =",
            "thunderbolt = t\nfar_plane = 1\nbolt_duration = 1",
        );
        let config = volya_engine_formats::ltx::parse(with.as_bytes()).unwrap();
        let err = WeatherSection::read(config.section("w").unwrap()).unwrap_err();
        assert!(err.to_string().contains("bolt_period"), "{err}");
    }

    #[test]
    fn brackets_and_blends() {
        let weather = Weather::load(&sample(), "default").unwrap();
        assert_eq!(parse_time("05:30:00"), Some(19_800.0));
        let (a, b, t) = weather.bracket(19_800.0);
        assert_eq!((a.name.as_str(), b.name.as_str(), t), ("w5", "w6", 0.5));
        // Wrapping: after 06:00 the cycle runs on to 05:00 of the next day.
        let (a, b, t) = weather.bracket(DAY - 3600.0);
        assert_eq!((a.name.as_str(), b.name.as_str()), ("w6", "w5"));
        assert!((t - 17.0 / 23.0).abs() < 1e-6);
        assert_eq!(a.sky_texture, "sky\\b");
        assert_eq!(b.sky_texture, "sky\\a");
        assert_eq!(b.sky_color, [0.4; 3]);
        assert_eq!(b.clouds_color, [0.4; 3]);
        assert_eq!(a.clouds_color, [0.0; 3], "k absent = 0");

        let (a, b, t) = weather.bracket(19_800.0);
        let e = Environment::blend(a, b, t, &[], [0.0; 3], 1.0);
        assert!((e.ambient[0] - 0.15).abs() < 1e-6);
        assert_eq!(e.hemi_color[3], 0.5);
        assert_eq!(e.far_plane, 325.0);
        assert!((e.fog_near - 0.1 * 350.0 * 0.85).abs() < 1e-3);
        assert!((e.fog_far - 346.5).abs() < 1e-3);
        // Sun direction (static-shading.md 9.4's start value).
        let d = e.sun_dir;
        assert!((d[0] - 0.9132).abs() < 1e-3 && (d[1] + 0.2079).abs() < 1e-3);
        assert!((d[2] - 0.3505).abs() < 1e-3);
        assert_eq!(e.fog_rgb()[0], 76.0 / 255.0);

        // A modifier of weight 1 at the camera halves towards its values.
        let m = EnvModifier {
            id: 0,
            position: [0.0; 3],
            radius: 2.0,
            power: 2.0,
            far_plane: 75.0,
            rest: [1.0; 13],
        };
        let e = Environment::blend(a, b, t, &[m], [0.0, 1.0, 0.0], 1.0);
        assert_eq!(e.far_plane, 200.0);
        assert!((e.sky_color[0] - (0.45 + 1.0) / 2.0).abs() < 1e-6);
        assert!((e.fog_density - 0.95).abs() < 1e-6);
    }
}
