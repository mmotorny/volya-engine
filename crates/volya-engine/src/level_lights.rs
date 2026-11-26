// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's lights over time: the light records
//! (docs/behavior/dynamic-lights.md, [`crate::core::lights`]), the glows
//! (docs/behavior/glows.md, [`crate::core::glows`]) and the hanging lamps
//! that own some of both (docs/behavior/hanging-lamps.md), with their
//! colour animations (docs/formats/lanims-xr.md).
//!
//! Each level update the lamps run their frame on the global clock, the
//! sun's records follow the weather, and the frame's light passes and glow
//! sprites are made from the latest frame's view; under the pause only the
//! last two, with no time passing.

use std::io;

use serde::Serialize;

use crate::core::camera::{Mat4, Vec3};
use crate::core::collision::CollisionWorld;
use crate::core::glows::{GlowId, GlowSprite, GlowView, Glows};
use crate::core::lights::{
    Frustum, LevelLight, LightId, LightKind, LightPass, LightSettings, Lights, light_pass, multiply,
};
use crate::core::particles::random::Generator;
use crate::level_visibility::LevelVisibility;
use crate::objects::LampLights;
use volya_engine_formats::lanims::{ColourAnimations, K};

/// `r1_glows_per_frame`: 2 … 32, built in 16 (glows.md 5).
pub const GLOWS_PER_FRAME: u32 = 16;

/// A glow's pass and texture (glows.md 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlowShader<T> {
    pub texture: T,
    /// `effects\flare` (alpha reference 2, no depth test) rather than
    /// `effects\glow`.
    pub flare: bool,
}

/// What the level's loading stages read for its lights.
#[derive(Clone, Debug, Default)]
pub struct LightsData {
    /// Chunk 6 (dynamic-lights.md 3.1).
    pub level_lights: Vec<LevelLight>,
    /// Chunk 7: position, radius, glow shader (glows.md 2.1).
    pub level_glows: Vec<(Vec3, f32, u32)>,
    /// The glow shaders, textures as file paths.
    pub glow_shaders: Vec<GlowShader<String>>,
    /// The glow shader of each lamp of the level's objects, by lamp.
    pub lamp_glows: Vec<Option<u32>>,
    pub animations: ColourAnimations,
}

/// The light records a chunk-6 record makes (dynamic-lights.md 3.1); a
/// record of another type gives none (TODO(#347): none in the data).
pub fn level_light(l: &volya_engine_formats::level::Light) -> Option<LevelLight> {
    use volya_engine_formats::level::{LIGHT_DIRECTIONAL, LIGHT_POINT};
    match l.kind {
        LIGHT_POINT => Some(LevelLight::Point {
            colour: l.diffuse,
            position: l.position,
            range: l.range,
        }),
        LIGHT_DIRECTIONAL => Some(LevelLight::Directional {
            direction: l.direction,
        }),
        _ => None,
    }
}

fn row(m: &Mat4, r: usize) -> Vec3 {
    [m[r][0], m[r][1], m[r][2]]
}

/// A hanging lamp while it is online (hanging-lamps.md 3 – 5).
#[derive(Clone, Debug)]
pub struct Lamp {
    pub id: u16,
    pub name: String,
    pub health: f32,
    /// The 8-bit update count, and whether the frame update runs (5.1).
    update_count: u8,
    running: bool,
    /// A physics shell (flag 0x01 with a model): not built (TODO(#347)).
    physics_shell: bool,
    pub main: LightId,
    pub ambient: Option<LightId>,
    pub glow: Option<GlowId>,
    /// Its colour animator's entry in `lanims.xr`.
    animator: Option<usize>,
    brightness: f32,
    ambient_power: f32,
    /// M and M′ at rest (no motion or physics moves the bones,
    /// TODO(#347)).
    main_matrix: Mat4,
    ambient_matrix: Mat4,
    /// Its lights are switched on.
    pub on: bool,
}

impl Lamp {
    /// The lamp of `l` coming online (section 3): its lights created off,
    /// then switched on if its health is above 0.
    pub fn online(
        l: &LampLights,
        glow_shader: Option<u32>,
        animations: &ColourAnimations,
        lights: &mut Lights,
        glows: &mut Glows,
    ) -> Self {
        let m = &l.main;
        let main = lights.create();
        lights.set_shadow(main, m.shadows);
        lights.set_kind(
            main,
            match m.kind {
                crate::objects::LightKind::Spot => LightKind::Spot,
                crate::objects::LightKind::Point => LightKind::Point,
            },
        );
        lights.set_range(main, m.range);
        lights.set_rgb(main, m.colour);
        lights.set_cone(main, m.cone);
        lights.set_texture(main, &m.texture);
        let glow = l.glow.as_ref().zip(glow_shader).map(|(g, shader)| {
            let id = glows.create(shader);
            glows.set_radius(id, g.radius);
            id
        });
        let ambient = l.ambient.as_ref().map(|a| {
            let id = lights.create();
            lights.set_kind(id, LightKind::Point);
            lights.set_shadow(id, false);
            lights.set_rgb(id, a.colour);
            lights.set_range(id, a.range);
            lights.set_texture(id, &a.texture);
            id
        });
        let animator = animations.find(m.colour_animator.as_bytes());
        let mut lamp = Self {
            id: l.id,
            name: l.name.clone(),
            health: l.health,
            update_count: 0,
            running: false,
            physics_shell: l.physics_shell && l.has_model,
            main,
            ambient,
            glow,
            animator,
            brightness: l.brightness,
            ambient_power: l.ambient_power,
            main_matrix: l.main_matrix,
            ambient_matrix: l.ambient_matrix,
            on: false,
        };
        if lamp.health > 0.0 {
            lamp.switch_on(lights, glows);
        } else {
            lamp.count(1);
            lamp.switch_off(lights, glows);
        }
        lamp
    }

    /// Adds to the update count (wrapping): the frame update starts when it
    /// goes from 0 to 1 and stops when it reaches 0 (5.1).
    fn count(&mut self, delta: i8) {
        let before = self.update_count;
        self.update_count = before.wrapping_add_signed(delta);
        if before == 0 && self.update_count == 1 {
            self.running = true;
        }
        if self.update_count == 0 {
            self.running = false;
        }
    }

    /// Switching on (5.1): the three on, the count up by one.
    // TODO(#347): the light bone and those below it made visible.
    pub fn switch_on(&mut self, lights: &mut Lights, glows: &mut Glows) {
        self.set_on(true, lights, glows);
        self.count(1);
    }

    /// Switching off (5.1): the three off, the count down by one without a
    /// physics shell.
    // TODO(#347): the light bone and those below it hidden.
    pub fn switch_off(&mut self, lights: &mut Lights, glows: &mut Glows) {
        self.set_on(false, lights, glows);
        if !self.physics_shell {
            self.count(-1);
        }
    }

    fn set_on(&mut self, on: bool, lights: &mut Lights, glows: &mut Glows) {
        self.on = on;
        lights.set_active(self.main, on);
        if let Some(a) = self.ambient {
            lights.set_active(a, on);
        }
        if let Some(g) = self.glow {
            glows.set_on(g, on);
        }
    }

    /// A hit of power `power` (5.2): on the light bone the health becomes 0,
    /// elsewhere it loses 100 × power; health going from above 0 to 0 or
    /// below switches the lamp off. Nothing hits lamps yet (TODO(#347)).
    pub fn hit(&mut self, power: f32, light_bone: bool, lights: &mut Lights, glows: &mut Glows) {
        let before = self.health;
        self.health = if light_bone {
            0.0
        } else {
            self.health - 100.0 * power
        };
        if before > 0.0 && self.health <= 0.0 {
            self.switch_off(lights, glows);
        }
    }

    /// The frame update (section 4) at global clock `t` seconds: while it
    /// runs, with health above 0 and the main light on, the lights placed
    /// at their bones and coloured by the animator.
    fn frame(
        &mut self,
        t: f32,
        animations: &ColourAnimations,
        lights: &mut Lights,
        glows: &mut Glows,
    ) {
        if !self.running || self.health <= 0.0 || !self.on {
            return;
        }
        let m = &self.main_matrix;
        lights.set_direction(self.main, row(m, 2));
        lights.set_right(self.main, row(m, 0));
        lights.set_position(self.main, row(m, 3));
        if let Some(g) = self.glow {
            glows.set_position(g, row(m, 3));
        }
        if let Some(a) = self.ambient {
            let m = &self.ambient_matrix;
            lights.set_direction(a, row(m, 2));
            lights.set_right(a, row(m, 0));
            lights.set_position(a, row(m, 3));
        }
        if let Some(entry) = self.animator.and_then(|i| animations.entries.get(i)) {
            let main = animated_colour(entry.colour_at(t).0, self.brightness);
            lights.set_rgb(self.main, main);
            if let Some(a) = self.ambient {
                lights.set_rgb(a, main.map(|c| c * self.ambient_power));
            }
        }
    }

    /// Its records go (section 8: nothing is kept).
    fn destroy(self, lights: &mut Lights, glows: &mut Glows) {
        lights.destroy(self.main);
        if let Some(a) = self.ambient {
            lights.destroy(a);
        }
        if let Some(g) = self.glow {
            glows.destroy(g);
        }
    }
}

/// The main light's colour from the animator's colour `argb` (4.3):
/// (red × k, green × k, blue × k), k = brightness × K, each in f32.
pub fn animated_colour(argb: u32, brightness: f32) -> Vec3 {
    let k = brightness * K;
    [16, 8, 0].map(|s| ((argb >> s) & 0xFF) as f32 * k)
}

/// The static part of a glow's occlusion ray (glows.md 5.1): the first
/// triangle of `collision` met at 0 < t ≤ `length` from `from` along the
/// unit direction `dir`, from either side; every triangle counts, whatever
/// its material.
pub fn static_ray(
    collision: &CollisionWorld,
    from: Vec3,
    dir: Vec3,
    length: f32,
) -> Option<[Vec3; 3]> {
    let mut hit = None;
    collision.ray_any(from, dir, length, |t| {
        let found = crate::core::glows::ray_meets(from, dir, length, t);
        if found {
            hit = Some(*t);
        }
        found
    });
    hit
}

/// The camera of a frame.
#[derive(Clone, Copy, Debug)]
pub struct LightCamera {
    pub eye: Vec3,
    pub view: Mat4,
    pub projection: Mat4,
    /// The far plane's target (glows.md conventions).
    pub far: f32,
}

/// What the engine interface reports.
#[derive(Clone, Debug, Default, Serialize)]
pub struct LightsReport {
    /// Light records, and the active ones.
    pub records: usize,
    pub active: usize,
    /// This frame's light passes.
    pub passes: usize,
    pub lamps: usize,
    pub lamps_on: usize,
    pub glows: usize,
    /// This frame's glow sprites.
    pub sprites: usize,
}

/// The level's lights, glows and lamps.
#[derive(Debug, Default)]
pub struct LevelLights {
    pub lights: Lights,
    pub glows: Glows,
    pub lamps: Vec<Lamp>,
    /// Every lamp of the level's objects, for coming online, with its glow
    /// shader.
    lamp_data: Vec<(LampLights, Option<u32>)>,
    animations: ColourAnimations,
    pub settings: LightSettings,
    pub glows_per_frame: u32,
    passes: Vec<LightPass>,
    sprites: Vec<GlowSprite>,
    /// The listed records as spot then point lists, kept between frames.
    ordered: Vec<LightId>,
}

impl LevelLights {
    /// The level's records (chunk 6: fatal without a sun), its glows, and
    /// its lamps online at the start.
    pub fn new(data: LightsData, lamps: Vec<LampLights>) -> io::Result<Self> {
        let mut s = Self {
            animations: data.animations,
            glows_per_frame: GLOWS_PER_FRAME,
            ..Self::default()
        };
        s.lights
            .add_level_lights(&data.level_lights)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        for (p, r, shader) in data.level_glows {
            s.glows.add_level(p, r, shader);
        }
        s.lamp_data = lamps
            .into_iter()
            .zip(data.lamp_glows.into_iter().chain(std::iter::repeat(None)))
            .collect();
        let online: Vec<u16> = (s.lamp_data.iter())
            .filter(|(l, _)| l.online)
            .map(|(l, _)| l.id)
            .collect();
        for id in online {
            s.object_online(id);
        }
        Ok(s)
    }

    /// The colour animations of `lanims.xr`.
    pub fn animations(&self) -> &ColourAnimations {
        &self.animations
    }

    /// The object `id` came online: a lamp starts from its record (hanging-
    /// lamps.md 3, 8).
    pub fn object_online(&mut self, id: u16) {
        if self.lamps.iter().any(|l| l.id == id) {
            return;
        }
        if let Some((l, shader)) = self.lamp_data.iter().find(|(l, _)| l.id == id) {
            let lamp = Lamp::online(
                l,
                *shader,
                &self.animations,
                &mut self.lights,
                &mut self.glows,
            );
            self.lamps.push(lamp);
        }
    }

    /// The object `id` went offline or was removed: a lamp's lights go.
    pub fn object_offline(&mut self, id: u16) {
        if let Some(i) = self.lamps.iter().position(|l| l.id == id) {
            let lamp = self.lamps.swap_remove(i);
            lamp.destroy(&mut self.lights, &mut self.glows);
        }
    }

    /// The lamps' frame updates (hanging-lamps.md 4.1) at global clock `t`
    /// seconds, a frame of `dt` seconds; none in a frame of 10⁻⁷ s or less
    /// (the spec's review gap, believed the frame step).
    pub fn lamps_frame(&mut self, t: f32, dt: f32) {
        if dt <= 1e-7 {
            return;
        }
        for lamp in &mut self.lamps {
            lamp.frame(t, &self.animations, &mut self.lights, &mut self.glows);
        }
    }

    /// The start of the frame's lights seen from `eye`: the sun's records
    /// from the weather's `sun_dir` and `sun_color` (dynamic-lights.md
    /// 3.1), then the records' sectors.
    pub fn prepare(
        &mut self,
        collision: &CollisionWorld,
        eye: Vec3,
        sun: (Vec3, Vec3),
        visibility: &LevelVisibility,
    ) {
        self.lights
            .update_sun(eye, sun.0, sun.1, self.settings.sun_lumscale);
        self.lights
            .update_sectors(|p| visibility.sector_of(collision, p));
    }

    /// The rest of the frame's lights seen from `camera` (a frame of `dt`
    /// seconds; 0 while paused), with the records the camera pass listed
    /// (`listed`, in their order): their light passes (5), made with
    /// occlusion off (occlusion-tests.md 4), and the glows (glows.md 3 –
    /// 6), their occlusion rays against `collision`.
    // TODO(#347): the object part of the glows' occlusion ray (glows.md
    // 5.1): the bone shapes' fields (level-objects.md chunk 16: shape type
    // codes, the box, sphere and cylinder layouts) and which objects are
    // enabled are not specified.
    pub fn finish(
        &mut self,
        collision: &CollisionWorld,
        camera: &LightCamera,
        dt: f32,
        visibility: &mut LevelVisibility,
        listed: &[LightId],
    ) {
        let eye = camera.eye;
        self.lights.lists_into(listed, &mut self.ordered);
        let (lights, clip) = (&self.lights, self.settings.clip);
        let (ordered, passes) = (&self.ordered, &mut self.passes);
        visibility.light_passes(|| {
            passes.clear();
            passes
                .extend((ordered.iter()).filter_map(|&id| light_pass(lights.get(id)?, eye, clip)));
        });
        let v = &camera.view;
        let view = GlowView {
            eye,
            forward: [v[0][2], v[1][2], v[2][2]],
            up: [v[0][1], v[1][1], v[2][1]],
            right: [v[0][0], v[1][0], v[2][0]],
            frustum: Frustum::of(&multiply(&camera.view, &camera.projection)),
            far: camera.far,
            camera_in_sector: visibility.camera_sector().is_some(),
        };
        self.glows.frame_into(
            &view,
            dt,
            self.glows_per_frame,
            (
                |p| visibility.sector_of(collision, p),
                |s, c, r| visibility.reached(s, c, r).is_some(),
                |from, dir, length| static_ray(collision, from, dir, length),
            ),
            &mut self.sprites,
        );
    }

    /// The frame's lights with the lights handled alone, nearest first
    /// (benches): [`LevelLights::prepare`], each record's tests, then
    /// [`LevelLights::finish`].
    pub fn frame(
        &mut self,
        collision: &CollisionWorld,
        camera: &LightCamera,
        sun: (Vec3, Vec3),
        dt: f32,
        visibility: &mut LevelVisibility,
        random: &mut Generator,
    ) {
        self.prepare(collision, camera.eye, sun, visibility);
        let listed = if visibility.camera_sector().is_some() {
            self.lights.choose(&self.settings, camera.eye, |t| {
                visibility.reached(t.sector, t.centre, t.radius).is_some()
                    && visibility.cached_visible(t.cache, t.min, t.max, random)
            })
        } else {
            Vec::new()
        };
        self.finish(collision, camera, dt, visibility, &listed);
    }

    /// The colour animations, the records and the glows, for a source
    /// updating its own.
    pub fn parts_mut(&mut self) -> (&ColourAnimations, &mut Lights, &mut Glows) {
        (&self.animations, &mut self.lights, &mut self.glows)
    }

    /// The frame's light passes.
    pub fn passes(&self) -> &[LightPass] {
        &self.passes
    }

    /// The frame's glow sprites.
    pub fn sprites(&self) -> &[GlowSprite] {
        &self.sprites
    }

    pub fn report(&self) -> LightsReport {
        LightsReport {
            records: self.lights.len(),
            active: self.lights.iter().filter(|(_, r)| r.active).count(),
            passes: self.passes.len(),
            lamps: self.lamps.len(),
            lamps_on: self.lamps.iter().filter(|l| l.on).count(),
            glows: self.glows.iter().count(),
            sprites: self.sprites.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::{Glow, Light, LightKind as ObjectLightKind};
    use volya_engine_formats::lanims::Entry;

    const IDENTITY: Mat4 = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    fn light(kind: ObjectLightKind, colour: Vec3, range: f32, animator: &str) -> Light {
        Light {
            kind,
            shadows: false,
            colour,
            range,
            cone: 120f32.to_radians(),
            texture: String::new(),
            colour_animator: animator.into(),
            position: [0.0; 3],
            direction: [0.0; 3],
            right: [0.0; 3],
        }
    }

    fn lamp(health: f32, flags_physics: bool) -> LampLights {
        let mut m = IDENTITY;
        m[3] = [1.0, 2.0, 3.0, 1.0];
        m[2] = [0.0, -1.0, 0.0, 0.0];
        LampLights {
            name: "trader_tv_0000".into(),
            id: 9,
            online: true,
            health,
            brightness: 0.8,
            ambient_power: 0.2,
            on: health > 0.0,
            physics_shell: flags_physics,
            has_model: true,
            main: light(ObjectLightKind::Spot, [0.8; 3], 3.0, "tv light 1"),
            ambient: Some(light(ObjectLightKind::Point, [0.16; 3], 1.0, "")),
            glow: Some(Glow {
                texture: "glow\\glow_orange".into(),
                radius: 1.0,
                position: [0.0; 3],
            }),
            main_matrix: m,
            ambient_matrix: m,
        }
    }

    /// An entry named `tv light 1` holding the colour its frame 7 gives
    /// (lanims-xr.md 8: 0x00427DAF).
    fn animations() -> ColourAnimations {
        ColourAnimations {
            version: 1,
            entries: vec![Entry {
                name: b"tv light 1".to_vec(),
                frame_rate: 15.0,
                frame_count: 60,
                keys: vec![(0, 0x0042_7DAF)],
            }],
        }
    }

    #[test]
    fn the_animated_colours_of_the_first_level() {
        // hanging-lamps.md 9.3.
        let close = |a: Vec3, b: Vec3| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(close(
            animated_colour(0x00FF_DEAB, 0.8),
            [0.800000, 0.696471, 0.536471]
        ));
        assert!(close(
            animated_colour(0x00FF_D5A5, 0.6),
            [0.600000, 0.501176, 0.388235]
        ));
        assert!(close(
            animated_colour(0x0042_7DAF, 1.0),
            [0.258824, 0.490196, 0.686275]
        ));
        assert!(close(
            animated_colour(0x0050_AFFD, 0.8),
            [0.250980, 0.549020, 0.793726]
        ));
    }

    #[test]
    fn a_lamp_comes_online_on_and_follows_its_bone_and_animator() {
        let mut lights = Lights::default();
        let mut glows = Glows::default();
        let anims = animations();
        let mut l = Lamp::online(
            &lamp(100.0, false),
            Some(3),
            &anims,
            &mut lights,
            &mut glows,
        );
        assert!(l.on && l.running);
        let main = lights.get(l.main).unwrap().clone();
        assert!(main.active && main.kind == LightKind::Spot && !main.shadow);
        assert_eq!((main.range, main.colour), (3.0, [0.8, 0.8, 0.8, 1.0]));
        // Until the first frame, at the record's default place.
        assert_eq!(main.position, [0.0, -1000.0, 0.0]);
        let ambient = lights.get(l.ambient.unwrap()).unwrap();
        assert!(ambient.active && ambient.kind == LightKind::Point);
        let g = glows.get(l.glow.unwrap()).unwrap();
        assert!(g.on && g.radius == 1.0 && g.shader == 3);
        l.frame(0.5, &anims, &mut lights, &mut glows);
        let main = lights.get(l.main).unwrap();
        assert_eq!(main.position, [1.0, 2.0, 3.0]);
        assert_eq!(main.direction, [0.0, -1.0, 0.0]);
        let c = animated_colour(0x0042_7DAF, 0.8);
        assert_eq!(main.colour, [c[0], c[1], c[2], 1.0]);
        let ambient = lights.get(l.ambient.unwrap()).unwrap();
        assert_eq!(ambient.colour[0], c[0] * 0.2);
        assert_eq!(
            glows.get(l.glow.unwrap()).unwrap().position,
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn switching_and_the_update_count() {
        let mut lights = Lights::default();
        let mut glows = Glows::default();
        let anims = ColourAnimations::default();
        // Coming online with health 0: count 1, then switched off to 0.
        let mut l = Lamp::online(&lamp(0.0, false), None, &anims, &mut lights, &mut glows);
        assert!(!l.on && !l.running && l.glow.is_none());
        assert!(!lights.get(l.main).unwrap().active);
        // Switched on with health 0: on, but neither placed nor coloured.
        l.switch_on(&mut lights, &mut glows);
        assert!(l.on && l.running);
        l.frame(1.0, &anims, &mut lights, &mut glows);
        assert_eq!(lights.get(l.main).unwrap().position, [0.0, -1000.0, 0.0]);
        // Off twice: the count wraps past 0 to 255 and does not run.
        l.switch_off(&mut lights, &mut glows);
        l.switch_off(&mut lights, &mut glows);
        assert_eq!((l.update_count, l.running), (255, false));
        // On again: 255 → 0 does not start it.
        l.switch_on(&mut lights, &mut glows);
        assert_eq!((l.update_count, l.running), (0, false));

        // A hit on the light bone switches a lamp off; with a physics
        // shell the count stays.
        let mut l = Lamp::online(&lamp(30.0, true), None, &anims, &mut lights, &mut glows);
        l.hit(0.1, false, &mut lights, &mut glows);
        assert!(l.on && (l.health - 20.0).abs() < 1e-5);
        l.hit(0.0, true, &mut lights, &mut glows);
        assert!(!l.on && l.health == 0.0);
        assert_eq!((l.update_count, l.running), (1, true));
        // A further hit on a dead lamp changes nothing else.
        l.hit(1.0, false, &mut lights, &mut glows);
        assert!(!l.on);
    }

    #[test]
    fn lamps_come_and_go_with_their_objects() {
        let data = LightsData {
            level_lights: vec![LevelLight::Directional {
                direction: [0.0, -1.0, 0.0],
            }],
            lamp_glows: vec![Some(0)],
            ..LightsData::default()
        };
        let mut level = LevelLights::new(data, vec![lamp(100.0, false)]).unwrap();
        assert_eq!(level.lamps.len(), 1);
        // The sun's two records and the lamp's two lights.
        assert_eq!(level.lights.len(), 4);
        assert_eq!(level.glows.iter().count(), 1);
        level.object_offline(9);
        assert!(level.lamps.is_empty());
        assert_eq!((level.lights.len(), level.glows.iter().count()), (2, 0));
        level.object_online(9);
        assert_eq!(level.lamps.len(), 1);
        // Without a sun the level does not load.
        assert!(LevelLights::new(LightsData::default(), Vec::new()).is_err());
    }
}
