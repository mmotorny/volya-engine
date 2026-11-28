// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Zones at rest (docs/behavior/zone-idle-effects.md, "P9"): what a zone
//! object reads from its section (section 1.2) and its record's Zone block
//! (1.3), its states (2), its creation as it comes online (3.1), the idle
//! start and stop of its effect, light and sound (4), its zone and
//! scheduled updates with fast mode (6) and the switching rule (7); the
//! sounds a zone creates and how its state changes play them
//! (docs/behavior/sound-anomalies.md, "S15", sections 2, 3).
//!
//! The effect objects are the level's particle objects
//! ([`crate::particles::placed`]); the sounds are played by the level's
//! sound side from [`Zones::sound_events`]; the idle light is kept here as
//! the light record's values, given to its record among the level's dynamic
//! lights (P16) by [`Zones::sync_lights`]. Their destruction with the level
//! (3.2) is the
//! level's: its sounds are stopped and discarded, its particle objects and
//! zones dropped.
//!
//! The zones' reactions to the player and their hits on it follow
//! docs/behavior/environment-hits.md ("PS-2", cited as `env`): the hit
//! keys (`env` 1.2), the player in a zone's list and whether it counts
//! (4.1, 4.2), the relative power (4.3), the cycle of awaking, blowout and
//! accumulation (5) and each kind's effect on the player (6), handed to
//! the level as [`PlayerEffect`]s.
//!
//! What waits for other specifications:
//! - Objects other than the player in a zone's list (physics objects,
//!   characters, monsters; `env` 4.1, 4.2): TODO(#699).
//! - The sounds of the states and hits (S15 sections 4 – 7): TODO(#350).
//! - The mincer kind's pull and telekinesis (`env` 6.4): TODO(#700).
//! - The hits' directions, positions and impulses acting on the player
//!   (`env` 2, 8): TODO(#701).
//! - The moving zone's motion (P9 5): `Z_TORRID` stays at its record's
//!   position (TODO(#351)).

use std::io;

use volya_engine_formats::ltx::{Section, value};
use volya_engine_formats::object::{Object, ObjectRecord, Shape, Zone as ZoneBlock};

use crate::character::hits::{Hit, HitType, NAMES};
use crate::lights::{LightId, Lights};
use crate::particles::placed::{Origin, PlacedEffects};
use crate::particles::{Matrix, Vec3, place_direction, place_point};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The behaviour a class id uses (P9 1.1, S15 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behaviour {
    /// `Z_MBALD`, `ZS_MBALD`.
    Mbald,
    /// `Z_MINCER`, `Z_GALANT`, `ZS_MINCE`, `ZS_GALAN`.
    Mincer,
    /// `Z_BFUZZ`, `ZS_BFUZZ`.
    Bfuzz,
    /// `Z_TORRID`, the moving zone (P9 5).
    Torrid,
    /// `Z_RADIO`.
    Radio,
}

impl Behaviour {
    /// The behaviour of `class_id`; `None` for class ids neither
    /// specification covers.
    pub fn of(class_id: &str) -> Option<Self> {
        Some(match class_id {
            "Z_MBALD" | "ZS_MBALD" => Self::Mbald,
            "Z_MINCER" | "Z_GALANT" | "ZS_MINCE" | "ZS_GALAN" => Self::Mincer,
            "Z_BFUZZ" | "ZS_BFUZZ" => Self::Bfuzz,
            "Z_TORRID" => Self::Torrid,
            "Z_RADIO" => Self::Radio,
            _ => return None,
        })
    }
}

/// The idle light's keys (P9 1.2).
#[derive(Clone, Debug, PartialEq)]
pub struct LightKeys {
    /// *R* (m).
    pub range: f32,
    /// Δ (m).
    pub range_delta: f32,
    /// The colour animation's name in `lanims.xr`.
    pub anim: String,
    /// *h* (m).
    pub height: f32,
}

/// The keys of the mincer kind's hits (`env` 1.2, 6.4).
#[derive(Clone, Debug, PartialEq)]
pub struct MincerKeys {
    /// `throw_in_atten`: the reach is *r* × this (4.3).
    pub throw_in_atten: f32,
    /// `throw_in_impulse_alive`: the pull's scale (6.4).
    pub throw_in_impulse_alive: f32,
    /// `blowout_radius_percent`, `actor_blowout_radius_percent`: the core
    /// fractions of other objects and of the player (6.4).
    pub blowout_radius_percent: f32,
    pub actor_blowout_radius_percent: f32,
}

/// What a zone's hits read from its section (`env` 1.2).
#[derive(Clone, Debug, PartialEq)]
pub struct HitKeys {
    /// `hit_type` (bald, burning fuzz, mincer kinds).
    pub hit_type: Option<HitType>,
    /// `attenuation` *a* (4.3).
    pub attenuation: f32,
    /// `effective_radius` *e* (bald, burning fuzz, radiation kinds).
    pub effective_radius: Option<f32>,
    /// `hit_impulse_scale` *s* (bald, burning fuzz, mincer kinds).
    pub hit_impulse_scale: Option<f32>,
    /// `period` (ms; radiation kind).
    pub period: Option<u32>,
    /// `blowout_explosion_time` (ms): 0 when absent, at most
    /// `blowout_time`.
    pub explosion_time: u32,
    pub ignore_nonalive: bool,
    pub ignore_small: bool,
    pub ignore_artefacts: bool,
    /// `disable_time`, `disable_time_small` (ms; −1 never).
    pub disable_time: i64,
    pub disable_time_small: i64,
    pub mincer: Option<MincerKeys>,
    /// `min_speed_to_react` (m/s; burning fuzz kind).
    pub min_speed_to_react: Option<f32>,
}

/// The hit type named by a zone's `hit_type` (`env` 1.2): a name of types
/// 0 … 9 ignoring case; any other, `physic_strike` included, is fatal.
pub fn zone_hit_type(name: &str) -> io::Result<HitType> {
    let name = name.trim();
    (NAMES.iter().take(10))
        .position(|n| n.eq_ignore_ascii_case(name))
        .map(|t| HitType::new(t as u32))
        .unwrap_or_else(|| Err(invalid("Unsupported hit type!")))
}

/// The sounds a zone creates (S15 2.1), by index.
pub const IDLE: usize = 0;
/// The keys of S15 2.1, in the table's order.
pub const SOUND_KEYS: [&str; 8] = [
    "idle_sound",
    "awake_sound",
    "accum_sound",
    "blowout_sound",
    "hit_sound",
    "entrance_sound",
    "artefact_born_sound",
    "body_tearing_sound",
];

/// What a zone object reads from its section (P9 1.2, 2; S15 2.1, 2.2).
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneSection {
    pub name: String,
    pub behaviour: Behaviour,
    /// `idle_particles`.
    pub idle_particles: Option<String>,
    /// `idle_light` on, with its keys.
    pub idle_light: Option<LightKeys>,
    /// Each sound key's value where the zone reads it (S15 2.1).
    pub sounds: [Option<String>; 8],
    /// `awaking_time`, `blowout_time`, `accamulate_time` (ms).
    pub awaking_time: i64,
    pub blowout_time: i64,
    pub accumulate_time: i64,
    /// `tele_height` (m), read by the `ZS_MINCE` behaviour (S15 2.1).
    pub tele_height: Option<f32>,
    /// `visible_by_detector` (required; sound-detector.md 5.2).
    pub visible_by_detector: bool,
    /// The keys of its hits (`env` 1.2).
    pub hits: HitKeys,
}

/// A string key "if present": its value, `None` when absent. An empty
/// value, which no shipped zone section has for these keys, is taken as
/// absent (TODO(#330): unestablished, P9 10).
fn optional(section: Section<'_>, key: &str) -> Option<String> {
    section
        .get(key)
        .flatten()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

impl ZoneSection {
    /// Reads `section` as a zone of `behaviour` reads it. A required key
    /// that is missing is fatal (ltx-config.md): an error.
    pub fn read(section: Section<'_>, behaviour: Behaviour) -> io::Result<Self> {
        let name = section.name();
        let at = |e: io::Error| invalid(format!("[{name}]: {e}"));
        let float = |key: &str| section.read(key).map(value::float).map_err(at);
        let int = |key: &str| section.read(key).map(value::int).map_err(at);
        let idle_light = if value::bool(section.read("idle_light").map_err(at)?) {
            Some(LightKeys {
                range: float("idle_light_range")?,
                range_delta: float("idle_light_range_delta")?,
                anim: section.read("idle_light_anim").map_err(at)?.to_owned(),
                height: float("idle_light_height")?,
            })
        } else {
            None
        };
        let mincer = behaviour == Behaviour::Mincer;
        // TODO(#330): a missing `spawn_blowout_artefacts` is read as off
        // (unestablished; S15 2.1 reads it only as on or off).
        let artefacts = section
            .get("spawn_blowout_artefacts")
            .flatten()
            .is_some_and(value::bool);
        let mut sounds: [Option<String>; 8] = Default::default();
        for (i, key) in SOUND_KEYS.iter().enumerate() {
            sounds[i] = match *key {
                "artefact_born_sound" if !artefacts => None,
                "body_tearing_sound" if !mincer => None,
                "body_tearing_sound" => Some(section.read(key).map_err(at)?.to_owned()),
                _ => optional(section, key),
            };
        }
        let blowout_time = int("blowout_time")?;
        let hits = read_hit_keys(section, behaviour, blowout_time).map_err(at)?;
        Ok(Self {
            name: name.to_owned(),
            behaviour,
            idle_particles: optional(section, "idle_particles"),
            idle_light,
            sounds,
            awaking_time: int("awaking_time")?,
            blowout_time,
            accumulate_time: int("accamulate_time")?,
            tele_height: if mincer {
                Some(float("tele_height")?)
            } else {
                None
            },
            visible_by_detector: value::bool(section.read("visible_by_detector").map_err(at)?),
            hits,
        })
    }
}

/// The hit keys of `section` for `behaviour` (`env` 1.2), all without a
/// default but `blowout_explosion_time`.
fn read_hit_keys(
    section: Section<'_>,
    behaviour: Behaviour,
    blowout_time: i64,
) -> io::Result<HitKeys> {
    let float = |key: &str| section.read(key).map(value::float);
    let int = |key: &str| section.read(key).map(value::int);
    let flag = |key: &str| section.read(key).map(value::bool);
    let (bald, fuzz, radiation, mincer) = match behaviour {
        Behaviour::Mbald | Behaviour::Torrid => (true, false, false, false),
        Behaviour::Bfuzz => (false, true, false, false),
        Behaviour::Radio => (false, false, true, false),
        Behaviour::Mincer => (false, false, false, true),
    };
    let when = |on: bool, key: &str| on.then(|| float(key)).transpose();
    let hit_type = if bald || fuzz || mincer {
        Some(zone_hit_type(section.read("hit_type")?)?)
    } else {
        None
    };
    // A value above `blowout_time` is replaced by it (the original's error
    // message is not kept: no shipped section has one).
    let explosion_time = section
        .get("blowout_explosion_time")
        .flatten()
        .map_or(0, |v| value::int(v) as u32)
        .min(blowout_time.max(0) as u32);
    Ok(HitKeys {
        hit_type,
        attenuation: float("attenuation")?,
        effective_radius: when(bald || fuzz || radiation, "effective_radius")?,
        hit_impulse_scale: when(bald || fuzz || mincer, "hit_impulse_scale")?,
        period: radiation
            .then(|| int("period").map(|v| v as u32))
            .transpose()?,
        explosion_time,
        ignore_nonalive: flag("ignore_nonalive")?,
        ignore_small: flag("ignore_small")?,
        ignore_artefacts: flag("ignore_artefacts")?,
        disable_time: int("disable_time")?,
        disable_time_small: int("disable_time_small")?,
        mincer: if mincer {
            Some(MincerKeys {
                throw_in_atten: float("throw_in_atten")?,
                throw_in_impulse_alive: float("throw_in_impulse_alive")?,
                blowout_radius_percent: float("blowout_radius_percent")?,
                actor_blowout_radius_percent: float("actor_blowout_radius_percent")?,
            })
        } else {
            None
        },
        min_speed_to_react: when(fuzz, "min_speed_to_react")?,
    })
}

/// The Zone block of a zone record (P9 1.3); `None` for other kinds.
pub fn zone_block(record: &ObjectRecord) -> Option<&ZoneBlock> {
    match &record.object {
        Object::Anomaly(a) => Some(&a.anomaly.zone),
        Object::VisualAnomaly(a) => Some(&a.anomaly.zone),
        Object::TorridZone(t) => Some(&t.zone),
        Object::Zone(z) => Some(z),
        _ => None,
    }
}

/// What a zone object takes from its record (P9 1.3, 3.1, 6.2).
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneRecord {
    pub id: u16,
    pub name: String,
    /// The record's class id (sound-detector.md 5.2).
    pub class_id: String,
    pub position: Vec3,
    pub angles: Vec3,
    pub shapes: Vec<Shape>,
    /// *e*, *d*, *s* (s).
    pub enabled_time: u32,
    pub disabled_time: u32,
    pub start_time_shift: u32,
    /// `[fast_mode] always_fast` of the custom data.
    pub always_fast: bool,
    /// The maximum power *M* of its Zone block (`env` 1.2).
    pub maximum_power: f32,
}

impl ZoneRecord {
    /// Object `id`'s record; `None` when it is not a zone's.
    pub fn of(id: u16, record: &ObjectRecord) -> Option<Self> {
        let block = zone_block(record)?;
        let h = &record.header;
        let always_fast = volya_engine_formats::ltx::parse(block.object.custom_data.as_bytes())
            .ok()
            .and_then(|c| c.get("fast_mode", "always_fast").flatten().map(value::bool))
            .unwrap_or(false);
        Some(Self {
            id,
            name: h.name.to_string(),
            class_id: record.class_id.as_str().to_owned(),
            position: h.position,
            angles: h.angles,
            shapes: block.shapes.clone(),
            enabled_time: block.enabled_time,
            disabled_time: block.disabled_time,
            start_time_shift: block.start_time_shift,
            always_fast,
            maximum_power: block.maximum_power,
        })
    }
}

/// A zone's five states (P9 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Awaking,
    Blowout,
    Accumulation,
    Disabled,
}

/// The idle light: the light record's values (P16) as the zone sets them
/// (P9 4.4). Its type is never set, so it is the record's default, point;
/// its shadow setting is on.
#[derive(Clone, Debug, PartialEq)]
pub struct IdleLight {
    pub keys: LightKeys,
    pub active: bool,
    /// The range last set (m); the record's stored range follows P16's
    /// rule for range updates ([`Lights::set_range`]).
    pub range: f32,
    /// Red, green, blue, alpha; `None` until the first light step sets it.
    pub colour: Option<[f32; 4]>,
    pub position: Vec3,
    pub shadow: bool,
    /// The light record among the level's dynamic lights, once made
    /// ([`Zones::sync_lights`]).
    pub record: Option<LightId>,
}

/// The colour animations of `lanims.xr` (lanims-xr.md, P12): the colour
/// of entry `name` at time `t` seconds (its section 6.2), as the value of
/// its section 5 (alpha, red, green, blue from the top byte down); `None`
/// where there is no such entry.
pub trait ColourAnimations {
    fn colour_at(&self, name: &str, t: f32) -> Option<u32>;
}

impl ColourAnimations for volya_engine_formats::lanims::ColourAnimations {
    /// The entry found by name (lanims-xr.md 4). An `idle_light_anim`
    /// naming no entry is unestablished (P9 10; none occurs): the colour
    /// stays as it was (TODO(#330)).
    fn colour_at(&self, name: &str, t: f32) -> Option<u32> {
        let name = volya_engine_formats::cp1251::encode(name)?;
        let i = self.find(&name)?;
        Some(self.entries[i].colour_at(t).0)
    }
}

/// No colour animations: the idle light's colour stays unset (tests).
pub struct NoColourAnimations;

impl ColourAnimations for NoColourAnimations {
    fn colour_at(&self, _: &str, _: f32) -> Option<u32> {
        None
    }
}

/// The f32 nearest to 1/255 (P9 4.4).
const UNIT: f32 = f32::from_bits(0x3B80_8081);
/// The f32 nearest to 2/32767 (P9 4.4).
const TWO_OVER_MAX: f32 = (2.0 / 32767.0) as f32;
/// The scale per metre of the camera's distance (P9 6.2).
const SCALE_PER_METRE: f32 = f32::from_bits(0x3BA3_D70A);
/// The general limits of game objects' scheduled updates, ms (P9 6.2).
const SCHEDULED_MAX: u32 = 1000;
/// Fast mode within this distance of the shape sphere, m (P9 6.2).
const FAST_DISTANCE: f32 = 50.0;

/// The light's colour from a colour animation's value `argb` (section 5
/// of lanims-xr.md, P9 4.4): the value with red and blue exchanged, its
/// bytes 0, 1, 2 each times the f32 nearest to 1/255, alpha 1.
pub fn light_colour(argb: u32) -> [f32; 4] {
    let swapped = (argb & 0xFF00_FF00) | ((argb >> 16) & 0xFF) | ((argb & 0xFF) << 16);
    let byte = |i: u32| ((swapped >> (8 * i)) & 0xFF) as f32 * UNIT;
    [byte(0), byte(1), byte(2), 1.0]
}

/// The light's range from *R*, Δ and a draw *k* (0 … 32767; P9 4.4):
/// *R* + Δ · (*k* · *q* − 1) in `f32`.
pub fn light_range(range: f32, delta: f32, k: u32) -> f32 {
    range + delta * (k as f32 * TWO_OVER_MAX - 1.0)
}

/// The scheduled period of a zone at `distance` m from the camera (P9
/// 6.2): clamp(30 + trunc(970 · σ), 30, 1000), σ = 0.005 · distance.
pub fn scheduled_period(distance: f64) -> u32 {
    let s = distance * f64::from(SCALE_PER_METRE);
    let product = (970.0 * s) as f32;
    (30 + product.trunc() as i64).clamp(30, 1000) as u32
}

/// The shape sphere in the zone's own space (P9 6.2): a single sphere
/// itself; otherwise the sphere about the axis-aligned box of all shapes,
/// a box counting with its 8 corners, a sphere with its centre ± its
/// radius (our reading of "all its shapes").
pub fn shape_sphere(shapes: &[Shape]) -> (Vec3, f32) {
    if let [Shape::Sphere { centre, radius }] = shapes {
        return (*centre, *radius);
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let mut add = |p: Vec3| {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    };
    for s in shapes {
        match s {
            Shape::Sphere { centre, radius } => {
                add(centre.map(|c| c - radius));
                add(centre.map(|c| c + radius));
            }
            Shape::Box([a, b, c, o]) => {
                for i in 0..8 {
                    let sign = |bit: u32| if i & bit != 0 { 0.5 } else { -0.5 };
                    add(std::array::from_fn(|k| {
                        o[k] + sign(1) * a[k] + sign(2) * b[k] + sign(4) * c[k]
                    }));
                }
            }
        }
    }
    if shapes.is_empty() {
        return ([0.0; 3], 0.0);
    }
    let centre = std::array::from_fn(|k| (lo[k] + hi[k]) * 0.5);
    let d: f32 = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum();
    (centre, d.sqrt() * 0.5)
}

/// What the level's sound side does with a zone's idle sound (P9 4.1,
/// 4.2; S15 3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoundEvent {
    /// Play at position, looped, not 2D, delay 0, no owner.
    Play { zone: usize, position: Vec3 },
    /// Stop at once.
    Stop { zone: usize },
}

/// One zone object.
#[derive(Clone, Debug)]
pub struct Zone {
    /// The object's id.
    pub id: u16,
    pub name: String,
    /// The class id (sound-detector.md 5.2).
    pub class_id: String,
    pub section: ZoneSection,
    /// The object's matrix *X* (object-visual-and-transform.md 4); row 3 is **p**.
    pub transform: Matrix,
    /// *e*′, *d*′, *s*′ (ms) and the creation clock *C*₀ (P9 1.3).
    enabled: u32,
    disabled: u32,
    shift: u32,
    created: u32,
    /// The switching rule is armed (P9 7).
    pub armed: bool,
    pub state: State,
    /// The state timer (ms).
    pub timer: u32,
    pub fast: bool,
    /// `[fast_mode] always_fast` of the record's custom data.
    always_fast: bool,
    /// The shape sphere in the zone's space.
    sphere: (Vec3, f32),
    /// The shapes in the zone's space.
    shapes: Vec<Shape>,
    /// Its scheduled update: next due and the previous one's clock.
    due: u32,
    previous: Option<u32>,
    /// The effect object, made at the first idle start.
    pub effect: Option<usize>,
    pub light: Option<IdleLight>,
    /// The idle sound plays, and where its latest start put it.
    pub sound_on: bool,
    pub sound_position: Vec3,
    /// The maximum power *M* (`env` 1.2).
    pub maximum_power: f32,
    /// The player's record in the zone's list (`env` 4.1, 4.2).
    pub player: Option<Inside>,
    /// The period timer (ms; `env` 5.1).
    pub period_timer: u32,
    /// The frame the zone last affected the objects (`env` 5.1).
    affected: Option<u64>,
}

/// An object's record in a zone's list (`env` 4.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inside {
    /// Its time in the zone (ms).
    pub time: u32,
    /// Non-alive when it entered.
    pub non_alive: bool,
    pub excluded: bool,
}

impl Zone {
    /// **p**, row 3 of *X*.
    pub fn position(&self) -> Vec3 {
        [
            self.transform[3][0],
            self.transform[3][1],
            self.transform[3][2],
        ]
    }

    /// The shape sphere placed by *X*.
    pub fn placed_sphere(&self) -> (Vec3, f32) {
        (place_point(self.sphere.0, &self.transform), self.sphere.1)
    }

    /// Whether the zone has an idle sound (S15 2.1).
    pub fn has_idle_sound(&self) -> bool {
        self.section.sounds[IDLE].is_some()
    }

    /// Something counts (`env` 4.2): the player, the only object of the
    /// lists (TODO(#699)), is listed and not excluded.
    pub fn counts(&self) -> bool {
        self.player.is_some_and(|p| !p.excluded)
    }

    /// Whether `point` (the player's position, a sphere of radius 0.001 m)
    /// is inside one of the zone's shapes placed by *X* (`env` 4.1).
    pub fn contains_point(&self, point: Vec3) -> bool {
        let x = &self.transform;
        self.shapes.iter().any(|s| match s {
            Shape::Sphere { centre, radius } => {
                distance(point, place_point(*centre, x)) < radius + CONTACT_RADIUS
            }
            Shape::Box([a, b, c, o]) => {
                let q = sub(point, place_point(*o, x));
                [a, b, c].iter().all(|axis| {
                    let u = place_direction(**axis, x);
                    let l = length(u);
                    l > 0.0 && (dot(q, u) / l).abs() <= l * 0.5 + CONTACT_RADIUS
                })
            }
        })
    }

    /// The relative power `k(d)` (`env` 4.3) with reach `reach`.
    pub fn relative_power(&self, d: f32, reach: f32) -> f32 {
        relative_power(self.section.hits.attenuation, d, reach)
    }
}

/// `k(d) = clamp(1 − a · (d / R)², 0, 1)` for `d ≤ R`, 0 beyond
/// (`env` 4.3).
pub fn relative_power(a: f32, d: f32, reach: f32) -> f32 {
    if d > reach {
        return 0.0;
    }
    (1.0 - a * (d / reach).powi(2)).clamp(0.0, 1.0)
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

fn distance(a: Vec3, b: Vec3) -> f32 {
    length(sub(a, b))
}

/// The radius of the player's position in the contact test (`env` 4.1).
const CONTACT_RADIUS: f32 = 0.001;
/// The least powers of a hit by the bald, burning fuzz and mincer kinds,
/// and by the radiation kind (`env` 6).
const LEAST_POWER: f32 = 0.01;
const LEAST_RADIATION: f32 = 1e-5;

/// The player as the zones read it (`env` 4, 6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    /// Its position (the feet).
    pub position: Vec3,
    /// Its radius ρ (`env` 4.2).
    pub radius: f32,
    /// Health > 0.
    pub alive: bool,
    /// The length of its movement velocity (m/s; `env` 5 item 1).
    pub speed: f32,
}

/// A zone's effect on the player (`env` 6), in the order made: the bald
/// and burning fuzz kinds first draw the three values of their direction
/// from the game's shared 15-bit generator (6.1 item 1), then the hit, if
/// any, is sent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerEffect {
    /// The zone's object id (the hitter and weapon).
    pub zone: u16,
    pub draws_direction: bool,
    pub hit: Option<Hit>,
}

/// The level's zone objects.
#[derive(Clone, Debug, Default)]
pub struct Zones {
    zones: Vec<Zone>,
    /// This frame's idle sound starts and stops, in order.
    events: Vec<SoundEvent>,
    /// The effects on the player since [`Zones::take_player_effects`].
    effects: Vec<PlayerEffect>,
    /// Frames run.
    frames: u64,
}

/// What a frame's updates read.
pub struct Frame<'a> {
    /// The global clock *C* (ms).
    pub clock: u32,
    /// The frame's elapsed milliseconds.
    pub elapsed: u32,
    /// The camera position *E*.
    pub camera: Vec3,
    pub colours: &'a dyn ColourAnimations,
    /// The player; `None` without a player character.
    pub player: Option<Target>,
}

impl Zones {
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    /// The zones, to change in tests.
    #[cfg(test)]
    pub(crate) fn zones_mut(&mut self) -> &mut [Zone] {
        &mut self.zones
    }

    /// The idle sound starts and stops since [`Zones::clear_events`].
    pub fn sound_events(&self) -> &[SoundEvent] {
        &self.events
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// The zones' effects on the player since last taken (`env` 6), to
    /// send as hits in order.
    pub fn take_player_effects(&mut self) -> Vec<PlayerEffect> {
        std::mem::take(&mut self.effects)
    }

    /// The active idle lights.
    pub fn lights(&self) -> impl Iterator<Item = &IdleLight> {
        (self.zones.iter())
            .filter_map(|z| z.light.as_ref())
            .filter(|l| l.active)
    }

    /// Gives the idle lights' values to their records among the level's
    /// dynamic lights (docs/behavior/dynamic-lights.md), making each record
    /// the first time (P9 4.4: shadow on, its type the default point).
    /// Unchanged values change nothing, so it runs after every frame's
    /// updates and creations.
    pub fn sync_lights(&mut self, lights: &mut Lights) {
        for l in self.zones.iter_mut().filter_map(|z| z.light.as_mut()) {
            let id = *l.record.get_or_insert_with(|| {
                let id = lights.create();
                lights.set_shadow(id, l.shadow);
                id
            });
            lights.set_range(id, l.range);
            lights.set_position(id, l.position);
            if let Some(c) = l.colour {
                lights.set_colour(id, c);
            }
            lights.set_active(id, l.active);
        }
    }

    /// Whether object `id` has a zone.
    pub fn contains(&self, id: u16) -> bool {
        self.zones.iter().any(|z| z.id == id)
    }

    /// Creates the zone of `record`, read with `section`, as it first comes
    /// online at clock `clock` (P9 3.1): its matrix, its times and
    /// switching rule, its light record, the idle start (whose effect
    /// object is made in `effects`), the idle state with timer 0, fast
    /// mode. An effect name that is neither an effect nor a group is the
    /// game's assertion failure: an error.
    pub fn create(
        &mut self,
        record: &ZoneRecord,
        section: ZoneSection,
        clock: u32,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        let transform = volya_engine_formats::model::transform(record.position, record.angles);
        let (e, d, s) = (
            record.enabled_time.wrapping_mul(1000),
            record.disabled_time.wrapping_mul(1000),
            record.start_time_shift.wrapping_mul(1000),
        );
        let light = section.idle_light.clone().map(|keys| IdleLight {
            range: keys.range,
            keys,
            active: false,
            colour: None,
            position: [0.0; 3],
            shadow: true,
            record: None,
        });
        let zone = Zone {
            id: record.id,
            name: record.name.clone(),
            class_id: record.class_id.clone(),
            section,
            transform,
            enabled: e,
            disabled: d,
            shift: s,
            created: clock,
            armed: e != 0 && d != 0,
            state: State::Idle,
            timer: 0,
            fast: true,
            always_fast: record.always_fast,
            sphere: shape_sphere(&record.shapes),
            shapes: record.shapes.clone(),
            due: clock,
            previous: Some(clock),
            effect: None,
            light,
            sound_on: false,
            sound_position: [0.0; 3],
            maximum_power: record.maximum_power,
            player: None,
            period_timer: 0,
            affected: None,
        };
        self.zones.push(zone);
        let i = self.zones.len() - 1;
        self.idle_start(i, effects)?;
        Ok(())
    }

    /// The idle start of zone `i` (P9 4.1).
    fn idle_start(&mut self, i: usize, effects: &mut PlacedEffects) -> Result<(), String> {
        let z = &mut self.zones[i];
        let p = z.position();
        if z.has_idle_sound() {
            z.sound_on = true;
            z.sound_position = p;
            self.events.push(SoundEvent::Play {
                zone: i,
                position: p,
            });
        }
        if let Some(name) = &z.section.idle_particles {
            let object = match z.effect {
                Some(o) => o,
                None => {
                    let o = effects.add(name, Origin::Zone)?;
                    z.effect = Some(o);
                    o
                }
            };
            effects.start(object, &z.transform, [0.0; 3]);
        }
        if let Some(l) = &mut z.light {
            l.range = l.keys.range;
            l.position = [p[0], p[1] + l.keys.height, p[2]];
            l.active = true;
        }
        Ok(())
    }

    /// The idle stop of zone `i` (P9 4.2).
    fn idle_stop(&mut self, i: usize, effects: &mut PlacedEffects) {
        let z = &mut self.zones[i];
        if z.sound_on {
            z.sound_on = false;
            self.events.push(SoundEvent::Stop { zone: i });
        }
        if let Some(o) = z.effect {
            effects.stop_immediate(o);
        }
        if let Some(l) = &mut z.light {
            l.active = false;
        }
    }

    /// A change of zone `i` to `state` arrives (S15 3, P9 7 as disputed in
    /// review): leaving idle (or idle again) stops, entering idle starts.
    /// TODO(#350): the event's round trip through the server is
    /// unestablished (P9 7); it arrives at once, so no request repeats.
    fn change(
        &mut self,
        i: usize,
        state: State,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        if self.zones[i].state == State::Idle {
            self.idle_stop(i, effects);
        }
        if state == State::Idle {
            self.idle_start(i, effects)?;
        }
        let z = &mut self.zones[i];
        z.state = state;
        z.timer = 0;
        Ok(())
    }

    /// The switching test of zone `i` at clock `clock` (P9 7).
    fn switching_test(
        &mut self,
        i: usize,
        clock: u32,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        let z = &self.zones[i];
        if !z.armed {
            return Ok(());
        }
        let Some(t) = clock
            .wrapping_sub(z.created)
            .wrapping_add(z.shift)
            .checked_rem(z.disabled.wrapping_add(z.enabled))
        else {
            return Ok(());
        };
        match z.state {
            State::Disabled if t < z.enabled => self.change(i, State::Idle, effects),
            State::Idle if t >= z.enabled => self.change(i, State::Disabled, effects),
            _ => Ok(()),
        }
    }

    /// The zone update of zone `i` with `elapsed` ms (P9 6.1): the timer
    /// `t0` kept and `t` advanced, the idle light's step, the state's step
    /// (`env` 5) and the period timer (5.1).
    fn zone_update(
        &mut self,
        i: usize,
        elapsed: u32,
        frame: &Frame,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        let z = &mut self.zones[i];
        let t0 = z.timer;
        z.timer = z.timer.wrapping_add(elapsed);
        if z.state == State::Disabled {
            return Ok(());
        }
        let p = z.position();
        if let Some(l) = z.light.as_mut().filter(|l| l.active) {
            let t = frame.clock as f32 / 1000.0;
            let colour = frame.colours.colour_at(&l.keys.anim, t).map(light_colour);
            let k = effects.random_mut().uniform.next_k();
            l.range = light_range(l.keys.range, l.keys.range_delta, k);
            if colour.is_some() {
                l.colour = colour;
            }
            l.position = [p[0], p[1] + l.keys.height, p[2]];
        }
        let t = i64::from(z.timer);
        let (awaking, accumulate) = (z.section.awaking_time, z.section.accumulate_time);
        let counts = z.counts();
        match z.state {
            State::Idle => self.switching_test(i, frame.clock, effects)?,
            State::Awaking if t >= awaking => self.change(i, State::Blowout, effects)?,
            State::Blowout => self.blowout_step(i, t0, frame, effects)?,
            State::Accumulation if t >= accumulate => {
                let next = if counts { State::Blowout } else { State::Idle };
                self.change(i, next, effects)?;
            }
            _ => {}
        }
        // 5.1: after the step of a zone not disabled.
        let z = &mut self.zones[i];
        if z.state != State::Disabled {
            z.period_timer = if z.counts() {
                let period = z.section.hits.period.unwrap_or(0);
                z.period_timer.saturating_add(elapsed).min(period)
            } else {
                0
            };
        }
        Ok(())
    }

    /// The blowout step of zone `i` whose timer was `t0` (`env` 5 item 3):
    /// the timed events, the explosion moment among them, and the change to
    /// accumulation at `blowout_time`.
    fn blowout_step(
        &mut self,
        i: usize,
        t0: u32,
        frame: &Frame,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        let z = &self.zones[i];
        let t = z.timer;
        let explosion_time = z.section.hits.explosion_time;
        let explosion = t0 <= explosion_time && explosion_time < t;
        let ended = i64::from(t) >= z.section.blowout_time;
        match z.section.behaviour {
            // Bald: the events at each update, that of `blowout_time`
            // included.
            Behaviour::Mbald | Behaviour::Torrid => {
                if explosion {
                    self.affect(i, false, frame);
                }
            }
            Behaviour::Bfuzz | Behaviour::Radio => {
                if explosion && !ended {
                    self.affect(i, false, frame);
                }
            }
            // Mincer: it affects the objects at every update, its explosion
            // hit only at the explosion moment.
            Behaviour::Mincer => self.affect(i, explosion, frame),
        }
        if ended {
            self.change(i, State::Accumulation, effects)?;
        }
        Ok(())
    }

    /// Zone `i` affects the objects (`env` 5.1, 6): at most once per
    /// frame, its kind's effect on the player if listed, then the period
    /// timer is 0. `explosion`: the mincer kind's explosion moment. The
    /// device counter that stops zones affecting objects is unestablished
    /// (`env` 10; believed: the level's start frames): taken as 0
    /// (TODO(#684)).
    fn affect(&mut self, i: usize, explosion: bool, frame: &Frame) {
        let z = &mut self.zones[i];
        if z.affected == Some(self.frames) {
            return;
        }
        z.affected = Some(self.frames);
        if let (Some(inside), Some(player)) = (z.player, frame.player)
            && let Some(effect) = effect_on_player(z, inside, player, explosion)
        {
            self.effects.push(effect);
        }
        self.zones[i].period_timer = 0;
    }

    /// The scheduled update of zone `i` (P9 6.2) with `elapsed` ms: for a
    /// zone not disabled, its list (`env` 4.1, 4.2), the wake test of an
    /// idle zone (5 item 1), fast mode and, out of it, the zone update.
    fn scheduled_update(
        &mut self,
        i: usize,
        elapsed: u32,
        frame: &Frame,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        if self.zones[i].state != State::Disabled {
            let z = &mut self.zones[i];
            refresh_list(z, elapsed, frame.player);
            if z.state == State::Idle && wakes(z, frame.player) {
                self.change(i, State::Awaking, effects)?;
            }
            let z = &mut self.zones[i];
            let (c, r) = z.placed_sphere();
            let d: f32 = (0..3)
                .map(|k| (frame.camera[k] - c[k]).powi(2))
                .sum::<f32>()
                .sqrt();
            z.fast = d - r <= FAST_DISTANCE || z.always_fast;
            if !z.fast {
                self.zone_update(i, elapsed, frame, effects)?;
            }
        }
        self.switching_test(i, frame.clock, effects)
    }

    /// One frame of the zones: the pass over the game's objects, in which
    /// every zone in fast mode has its zone update with the frame's time,
    /// then the scheduler's pass over the zones that are due (P9 6.1,
    /// 6.2; every due zone, the scheduler's budget being unestablished).
    /// Runs only while the game is not paused.
    pub fn frame(&mut self, frame: &Frame, effects: &mut PlacedEffects) -> Result<(), String> {
        self.frames += 1;
        for i in 0..self.zones.len() {
            if self.zones[i].fast {
                self.zone_update(i, frame.elapsed, frame, effects)?;
            }
        }
        for i in 0..self.zones.len() {
            let z = &self.zones[i];
            if (frame.clock.wrapping_sub(z.due) as i32) < 0 {
                continue;
            }
            let elapsed = match z.previous {
                Some(at) => frame.clock.wrapping_sub(at).clamp(1, SCHEDULED_MAX),
                None => 1,
            };
            let p = z.position();
            let d = (0..3)
                .map(|k| f64::from(frame.camera[k] - p[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            let z = &mut self.zones[i];
            z.previous = Some(frame.clock);
            z.due = frame.clock.wrapping_add(scheduled_period(d));
            self.scheduled_update(i, elapsed, frame, effects)?;
        }
        Ok(())
    }
}

/// The zone's list at a scheduled update (`env` 4.1, 4.2): the player
/// enters while its position is inside a shape and leaves at the first
/// update it is not; while listed its time grows, and past the disable time
/// a non-alive player becomes excluded. The player is never small (its
/// radius is above 0.6 m) nor an artefact.
fn refresh_list(z: &mut Zone, elapsed: u32, player: Option<Target>) {
    let Some(player) = player.filter(|p| z.contains_point(p.position)) else {
        z.player = None;
        return;
    };
    let keys = &z.section.hits;
    match &mut z.player {
        Some(inside) => {
            inside.time = inside.time.saturating_add(elapsed);
            if keys.disable_time >= 0 && i64::from(inside.time) > keys.disable_time && !player.alive
            {
                inside.excluded = true;
            }
        }
        None => {
            let non_alive = !player.alive;
            z.player = Some(Inside {
                time: 0,
                non_alive,
                excluded: non_alive && keys.ignore_nonalive,
            });
        }
    }
}

/// The wake test of an idle zone (`env` 5 item 1): something counts; for
/// the burning fuzz, a listed living thing moves faster than
/// `min_speed_to_react`, whether or not it counts.
fn wakes(z: &Zone, player: Option<Target>) -> bool {
    match z.section.behaviour {
        Behaviour::Bfuzz => {
            let least = z.section.hits.min_speed_to_react.unwrap_or(f32::MAX);
            z.player.is_some() && player.is_some_and(|p| p.speed > least)
        }
        _ => z.counts(),
    }
}

/// The effect of zone `z` on the listed player (`env` 6); `None` when it
/// makes none. `explosion`: the mincer kind's explosion moment.
fn effect_on_player(
    z: &Zone,
    inside: Inside,
    player: Target,
    explosion: bool,
) -> Option<PlayerEffect> {
    let keys = &z.section.hits;
    let m = z.maximum_power;
    let (c, r) = z.placed_sphere();
    let p = player.position;
    let hit = |kind: HitType, power: f32, bone: Option<u16>| Hit {
        kind,
        power,
        bone,
        armour_piercing: 0.0,
    };
    let effect = |draws_direction: bool, hit: Option<Hit>| PlayerEffect {
        zone: z.id,
        draws_direction,
        hit,
    };
    let reach = r * keys.effective_radius.unwrap_or(0.0);
    match z.section.behaviour {
        // 6.1, 6.2: bone 0, the direction drawn first.
        Behaviour::Mbald | Behaviour::Torrid | Behaviour::Bfuzz => {
            if inside.excluded {
                return None;
            }
            let d = if z.section.behaviour == Behaviour::Bfuzz {
                let (dx, dz) = (p[0] - c[0], p[2] - c[2]);
                (dx * dx + p[1] * p[1] + dz * dz).sqrt()
            } else {
                (distance(p, c) - player.radius).max(0.0)
            };
            let power = z.relative_power(d, reach) * m;
            let kind = keys.hit_type?;
            Some(effect(
                true,
                (power > LEAST_POWER).then(|| hit(kind, power, Some(0))),
            ))
        }
        // 6.3: when the period timer is at least `period`; always the
        // radiation type, no bone.
        Behaviour::Radio => {
            if z.period_timer < keys.period.unwrap_or(0) {
                return None;
            }
            let power = z.relative_power(distance(p, c), reach) * m;
            (power > LEAST_RADIATION)
                .then(|| effect(false, Some(hit(HitType::RADIATION, power, None))))
        }
        // 6.4: within the core at the explosion moment, the explosion hit;
        // otherwise the pull, which hits only other living things
        // (TODO(#700): its pending vector on the player).
        Behaviour::Mincer => {
            let mincer = keys.mincer.as_ref()?;
            let d = distance(z.position(), p);
            if d / r > mincer.actor_blowout_radius_percent || !explosion {
                return None;
            }
            let power = z.relative_power(d, r * mincer.throw_in_atten) * m;
            let kind = keys.hit_type?;
            (power > LEAST_POWER).then(|| effect(false, Some(hit(kind, power, Some(0)))))
        }
    }
}

/// Hit keys of every kind (`env` 1.2), for test sections.
#[cfg(test)]
pub(crate) const TEST_HIT_KEYS: &str = "hit_type = burn\nattenuation = 1\n\
    effective_radius = 1.3\nhit_impulse_scale = 0\nperiod = 1\nignore_nonalive = on\n\
    ignore_small = on\nignore_artefacts = on\ndisable_time = 10000\n\
    disable_time_small = 1000\nthrow_in_atten = 1.5\nthrow_in_impulse_alive = 1000\n\
    blowout_radius_percent = 0.3\nactor_blowout_radius_percent = 0.3\n\
    min_speed_to_react = 7.01\n";

#[cfg(test)]
mod tests;
