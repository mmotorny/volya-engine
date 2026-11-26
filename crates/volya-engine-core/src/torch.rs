// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Torches and torch night vision (docs/behavior/torch.md): a torch's
//! `torch_definition` (section 1.2), its parts (2) as light records
//! ([`crate::lights`]) and a glow ([`crate::glows`]), spawning (3),
//! switching (4), the owners that withhold its lights, the attachment
//! offset (6), the placement each frame (7) with the player's aim
//! smoothing (7.4), its colour (7.5) and the torch byte (9); the night
//! vision's sounds (14) and request (15), as commands for the sound system.
//!
//! Who owns a torch and where the owner is are the caller's
//! ([`Owner`], [`OwnerPose`]).

use std::f32::consts::{PI, TAU};
use std::io;

use crate::config::{Section, value};
use crate::glows::{GlowId, Glows};
use crate::lights::{LightId, LightKind, Lights};
use volya_engine_formats::lanims::Entry;

pub type Vec3 = [f32; 3];
/// Row-vector convention (`[p 1] · M`), as `crate::lights`.
pub type Mat4 = [[f32; 4]; 4];

/// The section of a torch model's user data (1.2).
pub const DEFINITION: &str = "torch_definition";

/// The fatal error of a torch model without user data (1.2).
pub const EMPTY_USER_DATA: &str = "Empty Torch user data!";

/// The smoothing's least and greatest speed (rad/s) and its range R
/// (7.4 step 2).
pub const SPEED_MIN: f32 = 0.5;
pub const SPEED_MAX: f32 = 7.5;
pub const RANGE: f32 = PI / 6.0;

/// The near form's squared distance from the camera (7.2).
pub const NEAR_SQUARED: f32 = 10_000.0;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// `torch_definition` (1.2), the keys of the first renderer (3 step 1):
/// the plain `range`, `color`, `omni_range` and `omni_color`.
#[derive(Clone, Debug, PartialEq)]
pub struct TorchDefinition {
    pub range: f32,
    /// (r, g, b, a).
    pub colour: [f32; 4],
    pub omni_range: f32,
    pub omni_colour: [f32; 4],
    pub colour_animator: String,
    pub spot_texture: String,
    /// Degrees.
    pub spot_angle: f32,
    pub glow_texture: String,
    pub glow_radius: f32,
    pub guide_bone: String,
}

impl TorchDefinition {
    /// From the `torch_definition` section; a missing key is an error.
    pub fn read(s: Section<'_>) -> io::Result<Self> {
        let text = |key: &str| s.read(key).map(|v| value::unquote(v).to_owned());
        let float = |key: &str| s.read(key).map(value::float);
        let colour = |key: &str| s.read(key).map(value::floats::<4>);
        Ok(Self {
            range: float("range")?,
            colour: colour("color")?,
            omni_range: float("omni_range")?,
            omni_colour: colour("omni_color")?,
            colour_animator: text("color_animator")?,
            spot_texture: text("spot_texture")?,
            spot_angle: float("spot_angle")?,
            glow_texture: text("glow_texture")?,
            glow_radius: float("glow_radius")?,
            guide_bone: text("guide_bone")?,
        })
    }

    /// The brightness b = (r + g + b) / 3 of `color` (3 step 4).
    pub fn brightness(&self) -> f32 {
        let [r, g, b, _] = self.colour;
        (r + g + b) / 3.0
    }

    /// The main light's cone, radians (3 step 5).
    pub fn cone(&self) -> f32 {
        self.spot_angle * PI / 180.0
    }

    /// The heading offset π/2 − atan((0.5 × r) / 0.2), r the main light's
    /// range (3 step 10).
    pub fn heading_offset(&self) -> f32 {
        PI / 2.0 - ((0.5 * self.range) / 0.2).atan()
    }
}

/// The keys of a torch's item section that the torch reads (1.1).
#[derive(Clone, Debug, PartialEq)]
pub struct TorchSection {
    /// `light_trace_bone`: shown only while the torch is on (4 step 5).
    pub light_trace_bone: String,
    /// `attach_bone_name`: the owner's bone it hangs on (6).
    pub attach_bone_name: String,
    pub attach_angle_offset: Vec3,
    pub attach_position_offset: Vec3,
    /// With `night_vision` on: its sounds and disabled levels (14, 15.2).
    pub night_vision: Option<NightVisionSection>,
}

impl TorchSection {
    /// From the torch's item section; a missing key is an error.
    pub fn read(s: Section<'_>) -> io::Result<Self> {
        let text = |key: &str| s.read(key).map(|v| value::unquote(v).to_owned());
        let floats = |key: &str| s.read(key).map(value::floats::<3>);
        let night_vision = if s.get("night_vision").flatten().is_some_and(value::bool) {
            Some(NightVisionSection::read(s)?)
        } else {
            None
        };
        Ok(Self {
            light_trace_bone: text("light_trace_bone")?,
            attach_bone_name: text("attach_bone_name")?,
            attach_angle_offset: floats("attach_angle_offset")?,
            attach_position_offset: floats("attach_position_offset")?,
            night_vision,
        })
    }

    /// The attachment offset O (6).
    pub fn offset(&self) -> Mat4 {
        attach_offset(self.attach_angle_offset, self.attach_position_offset)
    }
}

/// The offset O of an attached item (6): rows 0 – 2 the rotation of
/// `angles` (o₀, o₁, o₂), row 3 (`position`, 1).
pub fn attach_offset(angles: Vec3, position: Vec3) -> Mat4 {
    let (sx, cx) = angles[0].sin_cos();
    let (sy, cy) = angles[1].sin_cos();
    let (sz, cz) = angles[2].sin_cos();
    [
        [
            cz * cx - sz * sx * sy,
            -sz * cy,
            sz * cx * sy + cz * sx,
            0.0,
        ],
        [cz * sx * sy + sz * cx, cy * cz, sz * sx - cz * cx * sy, 0.0],
        [-cy * sx, sy, cy * cx, 0.0],
        [position[0], position[1], position[2], 1.0],
    ]
}

/// `a · b`, row-vector convention.
pub fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

fn row(m: &Mat4, r: usize) -> Vec3 {
    [m[r][0], m[r][1], m[r][2]]
}

/// One of a sound key's lines (14): the sound's name, play volume and delay
/// in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: String,
    pub volume: f32,
    pub delay: f32,
}

/// The variants of sound key `key` (14): the lines `key`, `key1`, `key2`, …
/// without a gap, each `name[, volume[, delay]]`; at least one is required.
pub fn sound_variants(s: Section<'_>, key: &str) -> io::Result<Vec<Variant>> {
    let line = |k: &str| s.get(k).flatten();
    let mut out = Vec::new();
    let mut k = key.to_owned();
    while let Some(v) = line(&k) {
        let mut parts = v.split(',').map(str::trim);
        let name = value::unquote(parts.next().unwrap_or("")).to_owned();
        let volume = parts.next().map_or(1.0, value::float);
        let delay = parts.next().map_or(0.0, value::float);
        out.push(Variant {
            name,
            volume,
            delay,
        });
        k = format!("{key}{}", out.len());
    }
    if out.is_empty() {
        return Err(invalid(format!("[{}] has no sound {key}", s.name())));
    }
    Ok(out)
}

/// The four night-vision sounds (14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NightVisionSound {
    On,
    Off,
    Idle,
    Broken,
}

impl NightVisionSound {
    pub const ALL: [NightVisionSound; 4] = [
        NightVisionSound::On,
        NightVisionSound::Off,
        NightVisionSound::Idle,
        NightVisionSound::Broken,
    ];

    /// Its key (14).
    pub fn key(self) -> &'static str {
        match self {
            NightVisionSound::On => "snd_night_vision_on",
            NightVisionSound::Off => "snd_night_vision_off",
            NightVisionSound::Idle => "snd_night_vision_idle",
            NightVisionSound::Broken => "snd_night_vision_broken",
        }
    }
}

/// A torch section's night vision (14, 15.2).
#[derive(Clone, Debug, PartialEq)]
pub struct NightVisionSection {
    /// In [`NightVisionSound::ALL`]'s order.
    pub sounds: [Vec<Variant>; 4],
    /// `disabled_maps`' items, spaces around them removed.
    pub disabled_maps: Vec<String>,
}

impl NightVisionSection {
    pub fn read(s: Section<'_>) -> io::Result<Self> {
        let [on, off, idle, broken] = NightVisionSound::ALL.map(|k| sound_variants(s, k.key()));
        let disabled_maps = s
            .get("disabled_maps")
            .flatten()
            .map(|v| v.split(',').map(|m| m.trim().to_owned()).collect())
            .unwrap_or_default();
        Ok(Self {
            sounds: [on?, off?, idle?, broken?],
            disabled_maps,
        })
    }

    pub fn variants(&self, sound: NightVisionSound) -> &[Variant] {
        &self.sounds[sound as usize]
    }

    /// Whether level `name` is disabled (15.2 step 3): equal, ignoring
    /// case, to one of the items.
    pub fn disabled(&self, name: &str) -> bool {
        self.disabled_maps
            .iter()
            .any(|m| m.eq_ignore_ascii_case(name))
    }
}

/// What the sound system is asked to do with a torch's night-vision sound
/// (14): play it (stopping its current emitter first) or stop it at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundCommand {
    Play {
        sound: NightVisionSound,
        looped: bool,
    },
    Stop(NightVisionSound),
}

/// A torch's night vision (15): the request, and the night-vision effect
/// on the player's camera while it is active, by its section.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NightVision {
    pub requested: bool,
    /// TODO(#191): the effect's picture and its ending are hud-scope spec
    /// 21's; told to stop, it ends at once here.
    pub effect: Option<String>,
}

impl NightVision {
    /// Switching to request `q` (15.2) for a torch whose owner is of the
    /// player kind or not, the worn outfit's effect section `outfit` and
    /// whether the level is disabled; the sounds to play go to `out`.
    pub fn switch(
        &mut self,
        q: bool,
        player: bool,
        outfit: Option<&str>,
        disabled: bool,
        out: &mut Vec<SoundCommand>,
    ) {
        self.requested = q;
        if !player {
            return;
        }
        let Some(section) = outfit else {
            if !q && self.effect.is_some() {
                self.stop(out);
            }
            return;
        };
        if disabled {
            out.push(SoundCommand::Play {
                sound: NightVisionSound::Broken,
                looped: false,
            });
            return;
        }
        if q {
            if self.effect.is_none() {
                self.effect = Some(section.to_owned());
                out.push(SoundCommand::Play {
                    sound: NightVisionSound::On,
                    looped: false,
                });
                out.push(SoundCommand::Play {
                    sound: NightVisionSound::Idle,
                    looped: true,
                });
            }
        } else if self.effect.is_some() {
            self.stop(out);
        }
    }

    fn stop(&mut self, out: &mut Vec<SoundCommand>) {
        self.effect = None;
        out.push(SoundCommand::Play {
            sound: NightVisionSound::Off,
            looped: false,
        });
        out.push(SoundCommand::Stop(NightVisionSound::Idle));
    }
}

/// Who owns a torch (conventions, 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// Of the player kind (`O_ACTOR`).
    Player,
    /// A stalker (`AI_STL`, `AI_STL_S`).
    Character,
    /// Any other kind.
    Other,
}

/// Whether `owner` withholds torch lights (4): a character while
/// `ai_use_torch_dynamic_lights` is off; nobody else, nor a torch without
/// an owner.
pub fn withholds(owner: Option<Owner>, use_torch_dynamic_lights: bool) -> bool {
    owner == Some(Owner::Character) && !use_torch_dynamic_lights
}

/// Where an owner is in a frame (6, 7.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnerPose {
    pub owner: Owner,
    /// X_owner (`LO` 3.4).
    pub transform: Mat4,
    /// B: the model-space matrix of its bone `attach_bone_name`.
    pub bone: Mat4,
    /// Its bounding sphere's centre (world) and radius, for the far form.
    pub centre: Vec3,
    pub radius: f32,
}

/// The player's camera (7.4 step 1): the yaw and pitch of
/// `player-movement.md` section 8.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub yaw: f32,
    pub pitch: f32,
}

/// An angle brought into [−π, π] (7.4 step 2): outside it, reduced modulo
/// 2π into [0, 2π) and then, if above π, 2π subtracted.
pub fn wrap(a: f32) -> f32 {
    if (-PI..=PI).contains(&a) {
        return a;
    }
    let r = a.rem_euclid(TAU);
    if r > PI { r - TAU } else { r }
}

/// A difference brought into [−π, π] by adding or subtracting 2π once.
fn difference(d: f32) -> f32 {
    if d > PI {
        d - TAU
    } else if d < -PI {
        d + TAU
    } else {
        d
    }
}

/// One step of the smoothing of 7.4 step 2: `h` toward `target` over a
/// frame of `dt` seconds, kept within R of it.
pub fn smooth(h: f32, target: f32, dt: f32) -> f32 {
    let mut h = wrap(h);
    let t = wrap(target);
    let d = difference(t - h);
    let s = SPEED_MIN + ((SPEED_MAX - SPEED_MIN) / RANGE * d.abs()).abs();
    if d.abs() >= 1e-7 {
        h += d.signum() * d.abs().min(s * dt);
        if h < 0.0 {
            h += TAU;
        } else if h > TAU {
            h -= TAU;
        }
    }
    h = wrap(h);
    let e = difference(h - t);
    if e < -RANGE {
        h -= e + RANGE;
    } else if e > RANGE {
        h -= e - RANGE;
    }
    h
}

/// The direction g = (−sin h′ · cos p, sin p, cos h′ · cos p) of 7.4
/// step 3, h′ = `heading` + `offset`.
pub fn direction(heading: f32, pitch: f32, offset: f32) -> Vec3 {
    let (sh, ch) = (heading + offset).sin_cos();
    let (sp, cp) = pitch.sin_cos();
    [-sh * cp, sp, ch * cp]
}

/// A unit vector perpendicular to `g` (7.4 step 4: not used by the
/// passes; our choice of which).
fn perpendicular(g: Vec3) -> Vec3 {
    let a = if g[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let c = [
        a[1] * g[2] - a[2] * g[1],
        a[2] * g[0] - a[0] * g[2],
        a[0] * g[1] - a[1] * g[0],
    ];
    let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    c.map(|v| v / l)
}

/// The placement frame M (7.2): near the camera `eye` (squared distance
/// below 10,000; the game is single player), G · X_torch with X_torch =
/// O · B · X_owner; far, the owner's transform with its translation
/// replaced by its centre raised by 2/3 of its radius.
pub fn placement(guide: &Mat4, offset: &Mat4, pose: &OwnerPose, eye: Vec3) -> Mat4 {
    let x = pose.transform;
    let d: f32 = (0..3).map(|k| (x[3][k] - eye[k]).powi(2)).sum();
    if d < NEAR_SQUARED {
        let torch = multiply(&multiply(offset, &pose.bone), &x);
        multiply(guide, &torch)
    } else {
        let mut m = x;
        let c = pose.centre;
        m[3] = [c[0], c[1] + 2.0 / 3.0 * pose.radius, c[2], 1.0];
        m
    }
}

/// The player's main light's position from M (7.4 step 4): row 3 − 0.2 ·
/// row 0 + 0.1 · row 1 − 0.3 · row 2.
pub fn player_light_position(m: &Mat4) -> Vec3 {
    std::array::from_fn(|k| m[3][k] - 0.2 * m[0][k] + 0.1 * m[1][k] - 0.3 * m[2][k])
}

/// A torch (2 – 9).
#[derive(Clone, Debug)]
pub struct Torch {
    pub on: bool,
    /// The cone: the bone `light_trace_bone` and its descendants shown.
    pub cone_visible: bool,
    pub brightness: f32,
    /// The player's smoothed heading and pitch, and their heading offset
    /// (7.3, 7.4).
    pub heading: f32,
    pub pitch: f32,
    pub heading_offset: f32,
    pub main: LightId,
    pub second: LightId,
    pub glow: GlowId,
    /// With `night_vision` on (15); `None` ignores every request.
    pub night_vision: Option<NightVision>,
}

impl Torch {
    /// Spawning (3) from `definition` with glow shader `glow_shader` (the
    /// caller's index for `effects\glow` with `glow_texture`) and the
    /// stored torch byte `stored`, for `owner`. The colour animator is the
    /// caller's to find (3 step 2; [`Torch::frame`]).
    // TODO(#361): whether the owner is known when the torch is switched at
    // spawn is unestablished (sections 4 and 7.4, disputed in review); the
    // simplest reading is taken: it is.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        definition: &TorchDefinition,
        glow_shader: u32,
        stored: u8,
        owner: Option<Owner>,
        use_torch_dynamic_lights: bool,
        night_vision: bool,
        lights: &mut Lights,
        glows: &mut Glows,
    ) -> Self {
        let d = definition;
        let main = lights.create();
        lights.set_kind(main, LightKind::Spot);
        lights.set_shadow(main, true);
        lights.set_colour(main, d.colour);
        lights.set_range(main, d.range);
        lights.set_cone(main, d.cone());
        lights.set_texture(main, &d.spot_texture);
        let second = lights.create();
        lights.set_kind(second, LightKind::Point);
        lights.set_shadow(second, false);
        lights.set_colour(second, d.omni_colour);
        lights.set_range(second, d.omni_range);
        let glow = glows.create(glow_shader);
        glows.set_radius(glow, d.glow_radius);
        let mut torch = Self {
            on: false,
            cone_visible: false,
            brightness: d.brightness(),
            heading: 0.0,
            pitch: 0.0,
            heading_offset: d.heading_offset(),
            main,
            second,
            glow,
            night_vision: night_vision.then(NightVision::default),
        };
        torch.switch(
            stored & 1 != 0,
            owner,
            use_torch_dynamic_lights,
            lights,
            glows,
        );
        // Step 9: the request cleared; no effect is active yet, so no
        // sound plays.
        torch.switch_night_vision(false, owner, None, false, &mut Vec::new());
        torch
    }

    /// Switching to `on` (4) for `owner`.
    pub fn switch(
        &mut self,
        on: bool,
        owner: Option<Owner>,
        use_torch_dynamic_lights: bool,
        lights: &mut Lights,
        glows: &mut Glows,
    ) {
        self.on = on;
        if !withholds(owner, use_torch_dynamic_lights) {
            lights.set_active(self.main, on);
            if owner != Some(Owner::Player) {
                lights.set_active(self.second, on);
            }
        }
        glows.set_on(self.glow, on);
        self.cone_visible = on;
    }

    /// The night-vision request switched to `q` (15.2); a torch without
    /// night vision ignores it.
    pub fn switch_night_vision(
        &mut self,
        q: bool,
        owner: Option<Owner>,
        outfit: Option<&str>,
        disabled: bool,
        out: &mut Vec<SoundCommand>,
    ) {
        if let Some(nv) = &mut self.night_vision {
            nv.switch(q, owner == Some(Owner::Player), outfit, disabled, out);
        }
    }

    /// The torch leaves its owner (4): switched off, the night-vision
    /// request switched off with the owner still known, then the on, off
    /// and idle sounds stopped at once.
    #[allow(clippy::too_many_arguments)]
    pub fn leave(
        &mut self,
        owner: Option<Owner>,
        use_torch_dynamic_lights: bool,
        outfit: Option<&str>,
        disabled: bool,
        lights: &mut Lights,
        glows: &mut Glows,
        out: &mut Vec<SoundCommand>,
    ) {
        self.switch(false, owner, use_torch_dynamic_lights, lights, glows);
        if self.night_vision.is_some() {
            self.switch_night_vision(false, owner, outfit, disabled, out);
            for s in [
                NightVisionSound::On,
                NightVisionSound::Off,
                NightVisionSound::Idle,
            ] {
                out.push(SoundCommand::Stop(s));
            }
        }
    }

    /// An update without owner (7.1): switched off when its model is
    /// visible and it lies in the world (`body`, believed a physics body);
    /// the cone is not hidden.
    pub fn frame_without_owner(
        &mut self,
        model_visible: bool,
        body: bool,
        lights: &mut Lights,
        glows: &mut Glows,
    ) {
        if !self.on || !(model_visible && body) {
            return;
        }
        self.on = false;
        lights.set_active(self.main, false);
        lights.set_active(self.second, false);
        glows.set_on(self.glow, false);
    }

    /// A frame (7.2 – 7.5) of `dt` seconds with the owner at `pose`, its
    /// torch's guide bone matrix G (`guide`) and attachment offset O, the
    /// camera at `eye`, the player's `aim` (owners of the player kind), and
    /// the colour animator's `animation` at clock `t` seconds.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        guide: &Mat4,
        offset: &Mat4,
        pose: &OwnerPose,
        eye: Vec3,
        aim: Aim,
        dt: f32,
        use_torch_dynamic_lights: bool,
        animation: Option<(&Entry, f32)>,
        lights: &mut Lights,
        glows: &mut Glows,
    ) {
        if !self.on {
            return;
        }
        let withheld = withholds(Some(pose.owner), use_torch_dynamic_lights);
        let m = placement(guide, offset, pose, eye);
        if pose.owner == Owner::Player {
            self.heading = smooth(self.heading, -aim.yaw, dt);
            self.pitch = smooth(self.pitch, -aim.pitch, dt);
            let g = direction(self.heading, self.pitch, self.heading_offset);
            if !withheld {
                lights.set_position(self.main, player_light_position(&m));
                lights.set_direction(self.main, g);
                lights.set_right(self.main, perpendicular(g));
            }
            glows.set_position(self.glow, row(&m, 3));
            glows.set_direction(self.glow, g);
        } else {
            if !withheld {
                for id in [self.main, self.second] {
                    lights.set_position(id, row(&m, 3));
                    lights.set_direction(id, row(&m, 2));
                    lights.set_right(id, row(&m, 0));
                }
            }
            glows.set_position(self.glow, row(&m, 3));
            glows.set_direction(self.glow, row(&m, 2));
        }
        if let Some((entry, t)) = animation {
            let argb = entry.colour_at(t).0;
            let [r, g, b] =
                [16, 8, 0].map(|s| ((argb >> s) & 0xFF) as f32 * self.brightness / 255.0);
            if !withheld {
                lights.set_colour(self.main, [r, g, b, 1.0]);
                lights.set_colour(self.second, [r, g, b, 1.0]);
            }
        }
    }

    /// The torch byte (9): bit 0 on, bit 1 night vision requested, bit 2
    /// the owner of the player kind with the torch attached to it.
    pub fn byte(&self, attached_to_player: bool) -> u8 {
        u8::from(self.on)
            | u8::from(self.night_vision.as_ref().is_some_and(|n| n.requested)) << 1
            | u8::from(attached_to_player) << 2
    }

    /// Its records go.
    pub fn destroy(self, lights: &mut Lights, glows: &mut Glows) {
        lights.destroy(self.main);
        lights.destroy(self.second);
        glows.destroy(self.glow);
    }
}

#[cfg(test)]
mod tests;
