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
//! the light record's values (P16) for the dynamic lights to take
//! ([`Zones::lights`]).
//!
//! What waits for other specifications:
//! - The contact test and the zones' reaction to objects (awaking,
//!   blowout, accumulation; F1 of particles-and-lights-scope.md, S15
//!   sections 4 – 7): no object is ever found inside, so a zone is only
//!   ever idle or disabled (TODO(#350)).
//! - The moving zone's motion (P9 5): `Z_TORRID` stays at its record's
//!   position (TODO(#351)).
//! - The colour animation of the idle light (lanims-xr.md, P12) and the
//!   dynamic lights that draw it (P16): TODO(#344).

use std::io;

use volya_engine_formats::ltx::{Section, value};
use volya_engine_formats::object::{Object, ObjectRecord, Shape, Zone as ZoneBlock};

use crate::particles::placed::{Origin, PlacedEffects};
use crate::particles::{Matrix, Vec3, place_point};

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
}

/// A string key "if present": its value, `None` when absent. An empty
/// value, which no shipped zone section has for these keys, is taken as
/// absent (TODO(#330): unestablished, P9 10).
fn optional(section: &Section, key: &str) -> Option<String> {
    section
        .get(key)
        .flatten()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

impl ZoneSection {
    /// Reads `section` as a zone of `behaviour` reads it. A required key
    /// that is missing is fatal (ltx-config.md): an error.
    pub fn read(section: &Section, behaviour: Behaviour) -> io::Result<Self> {
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
        Ok(Self {
            name: name.to_owned(),
            behaviour,
            idle_particles: optional(section, "idle_particles"),
            idle_light,
            sounds,
            awaking_time: int("awaking_time")?,
            blowout_time: int("blowout_time")?,
            accumulate_time: int("accamulate_time")?,
            tele_height: if mincer {
                Some(float("tele_height")?)
            } else {
                None
            },
        })
    }
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
    pub position: Vec3,
    pub angles: Vec3,
    pub shapes: Vec<Shape>,
    /// *e*, *d*, *s* (s).
    pub enabled_time: u32,
    pub disabled_time: u32,
    pub start_time_shift: u32,
    /// `[fast_mode] always_fast` of the custom data.
    pub always_fast: bool,
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
            name: h.name.clone(),
            position: h.position,
            angles: h.angles,
            shapes: block.shapes.clone(),
            enabled_time: block.enabled_time,
            disabled_time: block.disabled_time,
            start_time_shift: block.start_time_shift,
            always_fast,
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
    /// The range last set (m). Whether the stored range follows is P16's
    /// rule for range updates (TODO(#344)).
    pub range: f32,
    /// Red, green, blue, alpha; `None` until the first light step sets it.
    pub colour: Option<[f32; 4]>,
    pub position: Vec3,
    pub shadow: bool,
}

/// The colour animations of `lanims.xr` (lanims-xr.md, P12): the colour
/// of entry `name` at time `t` seconds (its section 6.2), as the value of
/// its section 5 (alpha, red, green, blue from the top byte down); `None`
/// where there is no such entry.
pub trait ColourAnimations {
    fn colour_at(&self, name: &str, t: f32) -> Option<u32>;
}

/// No colour animations: the idle light's colour stays unset.
/// TODO(#344): `lanims.xr`'s entries.
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
    pub section: ZoneSection,
    /// The object's matrix *X* (level-objects.md 3.4); row 3 is **p**.
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
    /// Its scheduled update: next due and the previous one's clock.
    due: u32,
    previous: Option<u32>,
    /// The effect object, made at the first idle start.
    pub effect: Option<usize>,
    pub light: Option<IdleLight>,
    /// The idle sound plays, and where its latest start put it.
    pub sound_on: bool,
    pub sound_position: Vec3,
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
}

/// The level's zone objects.
#[derive(Clone, Debug, Default)]
pub struct Zones {
    zones: Vec<Zone>,
    /// This frame's idle sound starts and stops, in order.
    events: Vec<SoundEvent>,
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
}

impl Zones {
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    /// The idle sound starts and stops since [`Zones::clear_events`].
    pub fn sound_events(&self) -> &[SoundEvent] {
        &self.events
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// The active idle lights, for the dynamic lights (P16; TODO(#344):
    /// submitted nowhere yet).
    pub fn lights(&self) -> impl Iterator<Item = &IdleLight> {
        (self.zones.iter())
            .filter_map(|z| z.light.as_ref())
            .filter(|l| l.active)
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
        });
        let zone = Zone {
            id: record.id,
            name: record.name.clone(),
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
            due: clock,
            previous: Some(clock),
            effect: None,
            light,
            sound_on: false,
            sound_position: [0.0; 3],
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

    /// The zone update of zone `i` with `elapsed` ms (P9 6.1).
    fn zone_update(
        &mut self,
        i: usize,
        elapsed: u32,
        frame: &Frame,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        let z = &mut self.zones[i];
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
        // TODO(#350): the steps of awaking, blowout and accumulation
        // (S15 4, 5) are never reached: no contact test finds an object.
        if z.state == State::Idle {
            self.switching_test(i, frame.clock, effects)?;
        }
        Ok(())
    }

    /// The scheduled update of zone `i` (P9 6.2) with `elapsed` ms.
    fn scheduled_update(
        &mut self,
        i: usize,
        elapsed: u32,
        frame: &Frame,
        effects: &mut PlacedEffects,
    ) -> Result<(), String> {
        let z = &mut self.zones[i];
        if z.state != State::Disabled {
            // TODO(#350): the contact test (P9 2, 6.2; F1) finds nothing.
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

    /// The zones are destroyed with the level (P9 3.2): each idle stop
    /// runs; their effect objects go with the level's.
    pub fn destroy(&mut self, effects: &mut PlacedEffects) {
        for i in 0..self.zones.len() {
            self.idle_stop(i, effects);
        }
    }
}

#[cfg(test)]
mod tests;
