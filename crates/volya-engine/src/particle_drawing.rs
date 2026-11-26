// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! How the level's effects are drawn (docs/behavior/particle-drawing.md):
//! each effect's shader resolved from its sprite (section 5.1 – 5.3), the
//! effect objects tested against the view (7.1), and each effect queued
//! with its sprites (7.2, section 4) for the renderer, which draws them in
//! the frame order of 7.4 and composes the distortion target (section 6).
//!
//! Which effect objects are drawn is the camera pass's (7.1;
//! object-visibility.md 4 – 6; `LevelScreen`'s dynamic items).

use serde::Serialize;

use crate::core::particles::placed::PlacedEffects;
use crate::core::particles::sprite::{self, Camera};
use crate::core::particles::{IDENTITY, Matrix, place_point};
use crate::render::particles::{
    ParticleBlend, ParticleCull, ParticleProgram, ParticleQueue, ParticleVertex,
};
use crate::render::world::{Mat4, multiply};
use crate::shading::texture_list;
use std::collections::HashMap;
use volya_engine_formats::particles::effect_flags;
use volya_engine_formats::shaders_xr::Definition;

/// One pass of an effect's shader; `T` is a texture: a name while
/// resolving, a path once located, a GPU texture when drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticlePass<T> {
    pub program: ParticleProgram,
    pub blend: ParticleBlend,
    pub depth_write: bool,
    /// `s_base` clamps (`Texture clamp`).
    pub clamp: bool,
    pub priority: u8,
    pub strict: bool,
    /// `s_base`, `s_distort`.
    pub base: Option<T>,
    pub distort: Option<T>,
}

impl<T> ParticlePass<T> {
    pub fn map<U>(self, mut f: impl FnMut(T) -> Option<U>) -> ParticlePass<U> {
        ParticlePass {
            program: self.program,
            blend: self.blend,
            depth_write: self.depth_write,
            clamp: self.clamp,
            priority: self.priority,
            strict: self.strict,
            base: self.base.and_then(&mut f),
            distort: self.distort.and_then(&mut f),
        }
    }

    /// Where the normal pass is drawn (7.2, 7.4).
    fn queue(&self) -> ParticleQueue {
        if self.strict {
            ParticleQueue::Strict
        } else if self.priority <= 1 {
            ParticleQueue::Early
        } else {
            ParticleQueue::Late
        }
    }
}

/// An effect's shader: the pass of elements 0 / 1 (the normal pass) and
/// element 4 with the distortion flag (section 6.2), each when it exists.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleShader<T> {
    pub normal: Option<ParticlePass<T>>,
    pub distortion: Option<ParticlePass<T>>,
}

impl<T> ParticleShader<T> {
    pub fn map<U>(self, mut f: impl FnMut(T) -> Option<U>) -> ParticleShader<U> {
        ParticleShader {
            normal: self.normal.map(|p| p.map(&mut f)),
            distortion: self.distortion.map(|p| p.map(&mut f)),
        }
    }

    /// Every texture of its passes.
    pub fn textures(&self) -> impl Iterator<Item = &T> {
        [&self.normal, &self.distortion]
            .into_iter()
            .flatten()
            .flat_map(|p| [p.base.as_ref(), p.distort.as_ref()])
            .flatten()
    }
}

fn nonempty(t: Option<&String>) -> Option<String> {
    t.filter(|t| !t.is_empty()).cloned()
}

/// The shader of a sprite's engine shader `shader` and texture string
/// `texture` (section 5.1): a scripted definition of `shaders\r1` first
/// (`scripted` says whether `<module>.s` defines it; only
/// `particles_xadd` and `particles_xdistort` are known, section 5.3), else
/// the library's class `PARTICLE` (5.2). Why it is not drawn otherwise.
pub fn resolve(
    definitions: &HashMap<String, Definition>,
    scripted: &dyn Fn(&str) -> bool,
    shader: &str,
    texture: &str,
) -> Result<ParticleShader<String>, String> {
    let t = texture_list(texture);
    let t0 = nonempty(t.first());
    let t1 = nonempty(t.get(1));
    let module = shader.replace('\\', "_");
    // Section 5.3: sampler defaults wrap; element 4 source alpha / inverse
    // source alpha; depth test on, write off; no alpha test, no fog;
    // priority 3, not strict.
    let distortion = |distort: Option<String>| ParticlePass {
        program: ParticleProgram::Distort,
        blend: ParticleBlend::Alpha,
        depth_write: false,
        clamp: false,
        priority: 3,
        strict: false,
        base: t0.clone(),
        distort,
    };
    if scripted(&module) {
        return match module.as_str() {
            "particles_xadd" => Ok(ParticleShader {
                normal: Some(ParticlePass {
                    program: ParticleProgram::Plain,
                    blend: ParticleBlend::Add,
                    depth_write: false,
                    clamp: false,
                    priority: 3,
                    strict: false,
                    base: t0.clone(),
                    distort: None,
                }),
                distortion: Some(distortion(t1)),
            }),
            "particles_xdistort" => Ok(ParticleShader {
                normal: None,
                distortion: Some(distortion(t0.clone())),
            }),
            other => Err(format!("scripted shader {other} is not supported")),
        };
    }
    let def = definitions
        .get(shader)
        .ok_or_else(|| format!("{shader} not found in library"))?;
    if def.tag != "PARTICLE" {
        return Err(format!("class {:?} is not a particle class", def.tag));
    }
    // Section 5.2: `Blending` 0 SET … 5 ALPHA-ADD; other values make no
    // pass.
    let blend = match def.token("Blending") {
        Some(0) => Some(ParticleBlend::Off),
        Some(1) => Some(ParticleBlend::Alpha),
        Some(2) => Some(ParticleBlend::Add),
        Some(3) => Some(ParticleBlend::Mul),
        Some(4) => Some(ParticleBlend::Mul2x),
        Some(5) => Some(ParticleBlend::AlphaAdd),
        _ => None,
    };
    let normal = blend.map(|blend| {
        let set = blend == ParticleBlend::Off;
        ParticlePass {
            program: if set {
                ParticleProgram::Set
            } else {
                ParticleProgram::Blended
            },
            blend,
            depth_write: set,
            clamp: def.boolean("Texture clamp").unwrap_or(false),
            priority: def.priority().clamp(0, 3) as u8,
            strict: def.strict(),
            base: t0,
            distort: None,
        }
    });
    Ok(ParticleShader {
        normal,
        distortion: None,
    })
}

/// Face culling from the flag word (section 4.5).
fn cull(flags: u32) -> ParticleCull {
    if flags & effect_flags::CULL == 0 {
        ParticleCull::None
    } else if flags & effect_flags::CULL_CCW != 0 {
        ParticleCull::CounterClockwise
    } else {
        ParticleCull::Clockwise
    }
}

/// One effect's sprites with one pass of its shader, as the scene holds
/// them; the presenter gives the pass its textures.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParticleDraw {
    /// The effect's index in the library.
    pub effect: usize,
    /// The distortion pass (element 4), else the normal pass.
    pub distortion: bool,
    #[serde(skip)]
    pub queue: ParticleQueue,
    #[serde(skip)]
    pub cull: ParticleCull,
    pub key: f32,
    pub particles: usize,
    #[serde(skip)]
    pub vertices: Vec<ParticleVertex>,
}

/// The view frustum's planes from `view_projection` (row-vector clip =
/// [**p** 1] · *VP*, depth 0 … 1), each (*a*, *b*, *c*, *d*) with
/// *a x* + *b y* + *c z* + *d* ≥ 0 inside, normalised.
fn frustum(vp: &Mat4) -> [[f32; 4]; 6] {
    let col = |c: usize| [vp[0][c], vp[1][c], vp[2][c], vp[3][c]];
    let (x, y, z, w) = (col(0), col(1), col(2), col(3));
    let plane = |p: [f32; 4]| {
        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-20);
        p.map(|v| v / l)
    };
    let add = |a: [f32; 4], b: [f32; 4], s: f32| std::array::from_fn(|i| a[i] + s * b[i]);
    [
        plane(add(w, x, 1.0)),
        plane(add(w, x, -1.0)),
        plane(add(w, y, 1.0)),
        plane(add(w, y, -1.0)),
        plane(z),
        plane(add(w, z, -1.0)),
    ]
}

/// Whether a sphere meets the frustum: it is rejected only when, for some
/// plane, its centre lies outside by more than its radius (7.1, as
/// disputed in review).
fn meets(planes: &[[f32; 4]; 6], centre: [f32; 3], radius: f32) -> bool {
    planes.iter().all(|p| {
        let d = p[0] * centre[0] + p[1] * centre[1] + p[2] * centre[2] + p[3];
        d >= -radius
    })
}

/// The placed effects whose culling sphere (their registered visibility
/// sphere, level-placed-effects.md 5.4, mapped by the identity object
/// transform) meets the view of `view` and `projection` (7.1).
pub fn visible(effects: &PlacedEffects, view: &Mat4, projection: &Mat4) -> Vec<usize> {
    let planes = frustum(&multiply(view, projection));
    effects
        .indexed()
        .filter(|(_, p)| {
            p.registered()
                .is_some_and(|s| meets(&planes, s.centre, s.radius))
        })
        .map(|(i, _)| i)
        .collect()
}

/// The effects of the visible records `records`, queued with their
/// sprites built for `camera` (7.1 step 2, 7.2): each effect the record
/// holds whose shader has a pass, with its key |**E** − **C**′|² + 10⁻⁵
/// and its screen measure above `ssa_min` (*T*\_min of level.md 9.4).
pub fn queue(
    effects: &PlacedEffects,
    records: &[usize],
    shaders: &[Option<ParticleShader<String>>],
    camera: &Camera,
    ssa_min: f32,
) -> Vec<ParticleDraw> {
    let library = effects.library();
    let mut out = Vec::new();
    // The level's effect objects keep the identity transform (P8 3).
    let t: Matrix = IDENTITY;
    for &r in records {
        let Some(object) = effects.get(r) else {
            continue;
        };
        for e in object.instance.effects() {
            let i = e.record();
            let Some(Some(shader)) = shaders.get(i) else {
                continue;
            };
            let record = library.effect_at(i);
            if e.particles().is_empty() || record.flags & effect_flags::SPRITE == 0 {
                continue;
            }
            let s = e.sphere();
            let c = place_point(s.centre, &t);
            let d: f32 = (0..3).map(|k| (camera.eye[k] - c[k]).powi(2)).sum();
            let key = d + 1.0e-5;
            if s.radius / key <= ssa_min {
                continue;
            }
            let mut vertices = Vec::new();
            sprite::quads(record, e, camera, &mut vertices);
            let vertices: Vec<ParticleVertex> = vertices
                .into_iter()
                .map(|v| ParticleVertex {
                    position: v.position,
                    colour: v.colour,
                    uv: v.uv,
                })
                .collect();
            let draw = |distortion: bool, queue: ParticleQueue| ParticleDraw {
                effect: i,
                distortion,
                queue,
                cull: cull(record.flags),
                key,
                particles: e.particles().len(),
                vertices: vertices.clone(),
            };
            if shader.distortion.is_some() {
                out.push(draw(true, ParticleQueue::Distortion));
            }
            if let Some(p) = &shader.normal {
                out.push(draw(false, p.queue()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::shaders_xr::Value;

    fn particle_definition(name: &str, blending: u32, priority: i32, strict: bool) -> Definition {
        Definition {
            tag: "PARTICLE".into(),
            name: name.into(),
            version: 0,
            properties: vec![
                ("General".into(), Value::Marker),
                (
                    "Priority".into(),
                    Value::Integer {
                        value: priority,
                        min: 0,
                        max: 3,
                    },
                ),
                ("Strict sorting".into(), Value::Boolean(strict)),
                (
                    "Blending".into(),
                    Value::Token {
                        value: blending,
                        tokens: Vec::new(),
                    },
                ),
                ("Texture clamp".into(), Value::Boolean(true)),
                (
                    "Alpha ref".into(),
                    Value::Integer {
                        value: 32,
                        min: 0,
                        max: 255,
                    },
                ),
            ],
        }
    }

    fn library() -> HashMap<String, Definition> {
        [
            particle_definition("particles\\add", 2, 3, false),
            particle_definition("particles\\blend", 1, 3, true),
            particle_definition("particles\\alpha_add", 5, 3, false),
            particle_definition("particles\\set", 0, 1, false),
        ]
        .into_iter()
        .map(|d| (d.name.clone(), d))
        .collect()
    }

    fn resolved(shader: &str, texture: &str) -> ParticleShader<String> {
        let scripted = |m: &str| m == "particles_xadd" || m == "particles_xdistort";
        resolve(&library(), &scripted, shader, texture).unwrap()
    }

    #[test]
    fn library_particle_shaders_follow_section_5_2() {
        let blend = resolved("particles\\blend", "pfx\\pfx_leaves_01.tga");
        let n = blend.normal.unwrap();
        assert_eq!(
            (n.program, n.blend, n.depth_write, n.clamp),
            (ParticleProgram::Blended, ParticleBlend::Alpha, false, true)
        );
        assert_eq!(n.base.as_deref(), Some("pfx\\pfx_leaves_01"));
        assert_eq!(n.queue(), ParticleQueue::Strict);
        assert!(blend.distortion.is_none());
        let set = resolved("particles\\set", "a").normal.unwrap();
        assert_eq!(
            (set.program, set.depth_write, set.queue()),
            (ParticleProgram::Set, true, ParticleQueue::Early)
        );
        let add = resolved("particles\\add", "a,b").normal.unwrap();
        assert_eq!(
            (add.blend, add.queue()),
            (ParticleBlend::Add, ParticleQueue::Late)
        );
        // A second texture is not used by the class.
        assert!(add.distort.is_none());
        assert_eq!(
            resolved("particles\\alpha_add", "a").normal.unwrap().blend,
            ParticleBlend::AlphaAdd
        );
    }

    #[test]
    fn scripted_particle_shaders_follow_section_5_3() {
        let xadd = resolved("particles\\xadd", "pfx\\a,pfx\\pfx_distortion");
        let n = xadd.normal.unwrap();
        assert_eq!(
            (n.program, n.blend, n.clamp),
            (ParticleProgram::Plain, ParticleBlend::Add, false)
        );
        let d = xadd.distortion.unwrap();
        assert_eq!(d.distort.as_deref(), Some("pfx\\pfx_distortion"));
        assert_eq!(d.program, ParticleProgram::Distort);
        // One texture: s_distort has none.
        assert!(
            resolved("particles\\xadd", "pfx\\a")
                .distortion
                .unwrap()
                .distort
                .is_none()
        );
        let xd = resolved("particles\\xdistort", "pfx\\d");
        assert!(xd.normal.is_none());
        assert_eq!(xd.distortion.unwrap().distort.as_deref(), Some("pfx\\d"));
    }

    #[test]
    fn culling_follows_section_4_5() {
        assert_eq!(cull(0x1), ParticleCull::None);
        assert_eq!(cull(0x400001), ParticleCull::Clockwise);
        assert_eq!(cull(0xC00001), ParticleCull::CounterClockwise);
        assert_eq!(cull(0x800001), ParticleCull::None);
    }

    #[test]
    fn spheres_partly_inside_the_frustum_pass() {
        // A view along +z with a unit-ish projection: x, y within ±w, z
        // 0 … w.
        let vp = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0, -0.1, 0.0],
        ];
        let planes = frustum(&vp);
        assert!(meets(&planes, [0.0, 0.0, 5.0], 0.1));
        assert!(meets(&planes, [5.5, 0.0, 5.0], 1.0));
        assert!(!meets(&planes, [7.0, 0.0, 5.0], 1.0));
        assert!(!meets(&planes, [0.0, 0.0, -5.0], 1.0));
    }
}
