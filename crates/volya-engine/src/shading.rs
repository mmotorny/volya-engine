// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! How a level's shader table entries resolve to surfaces: programs,
//! textures and states of the normal pass (docs/behavior/static-shading.md
//! sections 3–5, 7, 8).
//!
//! Scripted definitions (`shaders\r1\*.s`) are Lua; no Lua runs here. The
//! effect of the four scripts the first level uses is the spec's section 8,
//! applied when the module exists; another scripted module is reported as
//! unsupported (our choice).

use std::collections::HashMap;

use crate::core::config::{Config, value};
use crate::core::weather::texture_name;
use crate::render::world::{Blend, Program, Surface};
use volya_engine_formats::level::split_shader_entry;
use volya_engine_formats::shaders_xr::Definition;

/// A `[association]` entry of `textures\textures.ltx` (section 4.3).
#[derive(Clone, Debug, PartialEq)]
pub struct Association {
    pub detail: String,
    pub scale: f32,
    /// 3 diffuse or bump, 2 bump, 1 diffuse, 0 none.
    pub usage: u8,
}

impl Association {
    /// `<detail name>,<scale>` and the usage flag found by substring.
    pub fn parse(value: &str) -> Self {
        let (detail, rest) = value.split_once(',').unwrap_or((value, ""));
        let usage = if value.contains("usage[diffuse_or_bump]") {
            3
        } else if value.contains("usage[bump]") {
            2
        } else if value.contains("usage[diffuse]") {
            1
        } else {
            0
        };
        Self {
            detail: texture_name(detail.trim()),
            scale: value::float(rest),
            usage,
        }
    }

    /// Usage bit 0: a diffuse detail.
    pub fn diffuse(&self) -> bool {
        self.usage & 1 != 0
    }
}

/// The `[association]` table, by base texture name.
pub fn associations(textures_ltx: &Config) -> HashMap<String, Association> {
    textures_ltx
        .section("association")
        .map(|s| {
            s.entries()
                .map(|e| {
                    (
                        texture_name(e.key()),
                        Association::parse(e.value().unwrap_or("")),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// What resolves shader names.
pub struct ShaderLibrary<'a> {
    /// `shaders.xr` definitions by name.
    pub definitions: &'a HashMap<String, Definition>,
    /// Whether `shaders\r1\<module>.s` exists.
    pub scripted: &'a dyn Fn(&str) -> bool,
    pub associations: &'a HashMap<String, Association>,
}

/// The texture list of an entry: split at every `,` (no trimming),
/// lower-cased, extensions removed (section 4.1).
pub fn texture_list(list: &str) -> Vec<String> {
    list.split(',').map(texture_name).collect()
}

fn surface(program: Program, textures: [Option<String>; 4]) -> Surface<String> {
    Surface {
        program,
        blend: Blend::Off,
        depth_write: true,
        fog: true,
        alpha_ref: None,
        detail_scale: None,
        priority: 1,
        strict: false,
        object_lod: false,
        textures,
    }
}

fn nonempty(t: Option<&String>) -> Option<String> {
    t.filter(|t| !t.is_empty()).cloned()
}

impl ShaderLibrary<'_> {
    /// The surface of a non-empty shader table entry, or why it is not
    /// drawn.
    pub fn resolve(&self, entry: &str) -> Result<Surface<String>, String> {
        let (shader, list) = split_shader_entry(entry).ok_or("empty entry")?;
        let t = texture_list(list);
        let t0 = nonempty(t.first());
        let module = shader.replace('\\', "_");
        if (self.scripted)(&module) {
            return scripted(&module, t0);
        }
        let def = self
            .definitions
            .get(shader)
            .ok_or_else(|| format!("{shader} not found in library"))?;
        // The diffuse detail of T[0] (section 4.3).
        let association = t0.as_ref().and_then(|t| self.associations.get(t));
        let diffuse_detail = association.filter(|a| a.diffuse());
        let detail = |s: &mut Surface<String>| {
            if let Some(a) = diffuse_detail {
                s.detail_scale = Some(a.scale);
                s.textures[3] = Some(a.detail.clone());
            }
        };
        let aref = |def: &Definition| def.integer("Alpha ref").map(|r| r.clamp(0, 255) as u8);
        let blend = |def: &Definition| {
            if def.boolean("Alpha-blend").unwrap_or(false) {
                Blend::Alpha
            } else {
                Blend::Off
            }
        };
        let mut s = match def.tag.as_str() {
            tag @ ("LM      " | "LM_AREF ") => {
                if t.len() < 3 {
                    return Err("Not enought textures for shader".into());
                }
                let mut s = surface(
                    Program::LightMapped,
                    [t0, nonempty(t.get(1)), nonempty(t.get(2)), None],
                );
                if tag == "LM_AREF " {
                    s.alpha_ref = aref(def);
                    s.blend = blend(def);
                }
                detail(&mut s);
                s
            }
            tag @ ("V       " | "V_AREF  ") => {
                let mut s = surface(Program::VertexLit, [t0, None, None, None]);
                if tag == "V_AREF  " {
                    s.alpha_ref = aref(def);
                    s.blend = blend(def);
                }
                detail(&mut s);
                s
            }
            "D_TREE  " => {
                let mut s = surface(Program::Tree, [t0, None, None, None]);
                s.object_lod = def.boolean("Object LOD").unwrap_or(false);
                s.alpha_ref = Some(if s.object_lod { 0 } else { 200 });
                s.blend = blend(def);
                detail(&mut s);
                s
            }
            "BmmDold " => {
                if t.len() < 2 {
                    return Err("Not enought textures for shader".into());
                }
                let mut s = surface(Program::Terrain, [t0, nonempty(t.get(1)), None, None]);
                // The definition's own detail texture; dt_params from T[0]'s
                // association (section 7.4). Without one, no detail (our
                // choice; dt_params would be missing).
                s.textures[3] = def
                    .detail_texture()
                    .map(texture_name)
                    .filter(|d| !d.is_empty());
                s.detail_scale = association.map(|a| a.scale);
                s
            }
            other => return Err(format!("class {other:?} is not drawn for static visuals")),
        };
        s.priority = def.priority().clamp(0, 3) as u8;
        s.strict = def.strict();
        Ok(s)
    }
}

/// The normal pass of a scripted definition (section 8).
fn scripted(module: &str, t0: Option<String>) -> Result<Surface<String>, String> {
    let base = [t0, None, None, None];
    Ok(match module {
        "selflight" => surface(Program::SelfLight, base),
        "effects_lightplanes" => Surface {
            fog: false,
            depth_write: false,
            blend: Blend::Add,
            alpha_ref: Some(0),
            priority: 2,
            ..surface(Program::LightPlanes, base)
        },
        "effects_wallmarkblend" => Surface {
            depth_write: false,
            blend: Blend::Alpha,
            alpha_ref: Some(0),
            priority: 2,
            ..surface(Program::Wallmark, base)
        },
        "details_lod" => return Err("distant billboards are not drawn".into()),
        other => return Err(format!("scripted shader {other} is not supported")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(
        tag: &str,
        name: &str,
        props: Vec<(&str, volya_engine_formats::shaders_xr::Value)>,
    ) -> Definition {
        use volya_engine_formats::shaders_xr::Value;
        let mut properties = vec![
            ("General".into(), Value::Marker),
            (
                "Priority".into(),
                Value::Integer {
                    value: 1,
                    min: 0,
                    max: 3,
                },
            ),
            ("Strict sorting".into(), Value::Boolean(false)),
        ];
        properties.extend(props.into_iter().map(|(n, v)| (n.to_owned(), v)));
        Definition {
            tag: tag.into(),
            name: name.into(),
            version: 1,
            properties,
        }
    }

    fn library() -> HashMap<String, Definition> {
        use volya_engine_formats::shaders_xr::Value;
        let int = |value| Value::Integer {
            value,
            min: 0,
            max: 255,
        };
        let mut trans = definition(
            "LM_AREF ",
            "def_shaders\\def_trans",
            vec![("Alpha ref", int(0)), ("Alpha-blend", Value::Boolean(true))],
        );
        trans.properties[1].1 = int(2);
        trans.properties[2].1 = Value::Boolean(true);
        [
            definition("LM      ", "default", vec![]),
            definition("V       ", "def_shaders\\def_vertex", vec![]),
            trans,
            definition(
                "D_TREE  ",
                "flora\\leaf_wave",
                vec![
                    ("Alpha-blend", Value::Boolean(true)),
                    ("Object LOD", Value::Boolean(false)),
                ],
            ),
            definition(
                "D_TREE  ",
                "def_shaders\\def_objects_lod",
                vec![
                    ("Alpha-blend", Value::Boolean(false)),
                    ("Object LOD", Value::Boolean(true)),
                ],
            ),
            definition(
                "BmmDold ",
                "levels\\escape_grass",
                vec![
                    ("Detail map", Value::Marker),
                    ("Name", Value::Texture("detail\\detail_grnd_grass".into())),
                ],
            ),
            definition("S_SET   ", "effects\\glow", vec![]),
        ]
        .into_iter()
        .map(|d| (d.name.clone(), d))
        .collect()
    }

    fn resolve(entry: &str) -> Result<Surface<String>, String> {
        let definitions = library();
        let associations: HashMap<String, Association> = [
            (
                "mtl\\mtl_a",
                "detail\\detail_metal, 6, usage[diffuse_or_bump]",
            ),
            ("mtl\\mtl_b", "detail\\detail_metal, 6, usage[bump]"),
            ("terrain\\terrain_escape", "detail\\x, 144, usage[diffuse]"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), Association::parse(v)))
        .collect();
        let scripted = |m: &str| ["selflight", "effects_wallmarkblend", "details_lod"].contains(&m);
        ShaderLibrary {
            definitions: &definitions,
            scripted: &scripted,
            associations: &associations,
        }
        .resolve(entry)
    }

    fn names(s: &Surface<String>) -> [Option<&str>; 4] {
        s.textures.each_ref().map(|t| t.as_deref())
    }

    #[test]
    fn associations_parse() {
        let a = Association::parse("detail\\detail_grnd_grass.dds,144, usage[diffuse]");
        assert_eq!(
            (a.detail.as_str(), a.scale, a.usage),
            ("detail\\detail_grnd_grass", 144.0, 1)
        );
        assert!(a.diffuse());
        assert!(!Association::parse("d, 2, usage[bump]").diffuse());
        assert_eq!(Association::parse("d,2").usage, 0);
        assert_eq!(texture_list("A.TGA,lmap#1_1,"), ["a", "lmap#1_1", ""]);
    }

    #[test]
    fn library_shaders() {
        let s = resolve("default/mtl\\mtl_a,lmap#5_1,lmap#5_2").unwrap();
        assert_eq!(s.program, Program::LightMapped);
        assert_eq!(
            names(&s),
            [
                Some("mtl\\mtl_a"),
                Some("lmap#5_1"),
                Some("lmap#5_2"),
                Some("detail\\detail_metal")
            ]
        );
        assert_eq!(
            (s.detail_scale, s.blend, s.alpha_ref, s.fog),
            (Some(6.0), Blend::Off, None, true)
        );
        // A bump-only association gives no detail.
        let s = resolve("default/mtl\\mtl_b,lmap#5_1,lmap#5_2").unwrap();
        assert_eq!((s.detail_scale, s.textures[3].as_deref()), (None, None));
        assert!(
            resolve("default/mtl\\mtl_a,lmap#5_1").is_err(),
            "too few textures"
        );
        let s = resolve("def_shaders\\def_trans/glass,lmap#1_1,lmap#1_2").unwrap();
        assert_eq!(
            (s.blend, s.alpha_ref, s.priority, s.strict),
            (Blend::Alpha, Some(0), 2, true)
        );
        let s = resolve("def_shaders\\def_vertex/mtl\\mtl_a").unwrap();
        assert_eq!((s.program, s.detail_scale), (Program::VertexLit, Some(6.0)));
        let s = resolve("flora\\leaf_wave/trees\\leaf").unwrap();
        assert_eq!(
            (s.program, s.blend, s.alpha_ref, s.object_lod),
            (Program::Tree, Blend::Alpha, Some(200), false)
        );
        let s = resolve("def_shaders\\def_objects_lod/act\\x").unwrap();
        assert_eq!(
            (s.blend, s.alpha_ref, s.object_lod),
            (Blend::Off, Some(0), true)
        );
        let s = resolve("levels\\escape_grass/terrain\\terrain_escape,terrain\\terrain_escape_lm")
            .unwrap();
        assert_eq!(s.program, Program::Terrain);
        assert_eq!(
            names(&s),
            [
                Some("terrain\\terrain_escape"),
                Some("terrain\\terrain_escape_lm"),
                None,
                Some("detail\\detail_grnd_grass")
            ]
        );
        assert_eq!(s.detail_scale, Some(144.0));
        assert!(resolve("effects\\glow/glow\\glow_04").is_err());
        assert!(
            resolve("nothing/x")
                .unwrap_err()
                .contains("not found in library")
        );
    }

    #[test]
    fn scripted_shaders_take_precedence() {
        let s = resolve("selflight/mtl\\lamp").unwrap();
        assert_eq!(
            (s.program, s.fog, s.depth_write),
            (Program::SelfLight, true, true)
        );
        let s = resolve("effects\\wallmarkblend/wm\\wm_blood").unwrap();
        assert_eq!(
            (s.program, s.blend, s.depth_write, s.alpha_ref, s.priority),
            (Program::Wallmark, Blend::Alpha, false, Some(0), 2)
        );
        assert!(resolve("details\\lod/level_lods").is_err());
    }
}
