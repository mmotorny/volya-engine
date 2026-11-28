// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Script API X's sight, vision and memory, enemies and dangers, covers
//! and evaluation functions (docs/behavior/script-api-movement-sight-and-
//! memory.md sections 6 – 9 and 12): the `game_object` members, the class
//! `CSightParams`, the memory record classes, `danger_object` and
//! `cef_storage`.
//!
//! No engine system perceives yet (CH-13, CH-15, CH-19, TODO(#1131)), so
//! every fighter's memory is empty: nothing is remembered, seen, chosen as
//! enemy or danger, and no member hands out a memory record or a
//! `danger_object`; their classes are registered as the spec states.

use super::members::{Call, Handle, boolean, member, number};
use super::movement::{FLT_MAX, SightRequest, float_or, unavailable};
use super::{Context, Kinds};
use crate::classes::{EngineClass, EnginePart, Objects, Operand};
use crate::loader::Scripts;
use crate::values::{Vector, to_bits32, to_f32};
use crate::{Error, Value};

/// `CSightParams`'s constants, the sight types (6.3).
pub const SIGHT_TYPES: [(&str, i32); 12] = [
    ("eSightTypeDummy", -1),
    ("eSightTypeCurrentDirection", 0),
    ("eSightTypePathDirection", 1),
    ("eSightTypeDirection", 2),
    ("eSightTypePosition", 3),
    ("eSightTypeObject", 4),
    ("eSightTypeCover", 5),
    ("eSightTypeSearch", 6),
    ("eSightTypeLookOver", 7),
    ("eSightTypeCoverLookOver", 8),
    ("eSightTypeFireObject", 9),
    ("eSightTypeFirePosition", 10),
];

const POSITION: i32 = 3;
const OBJECT: i32 = 4;
const FIRE_OBJECT: i32 = 9;
const FIRE_POSITION: i32 = 10;

/// `danger_object`'s constants (8.2): the danger types and the perceive
/// types.
pub const DANGER_CONSTANTS: [(&str, i32); 11] = [
    ("bullet_ricochet", 0),
    ("attack_sound", 1),
    ("entity_attacked", 2),
    ("entity_death", 3),
    ("entity_corpse", 4),
    ("attacked", 5),
    ("grenade", 6),
    ("enemy_sound", 7),
    ("visual", 0),
    ("sound", 1),
    ("hit", 2),
];

/// The class id number of the searchlight class (6.4, section 2: believed
/// the client class of `clsid.projector`'s objects).
pub const SEARCHLIGHT_CLSID: usize = 84;

/// A `CSightParams` (6.2, 6.3).
#[derive(Clone, Debug, PartialEq)]
pub struct SightParams {
    pub sight_type: i32,
    /// The object's handle.
    pub object: Option<EnginePart>,
    pub vector: [f32; 3],
}

impl Default for SightParams {
    /// `CSightParams()`'s fields are unestablished, and so is a request's
    /// vector not written: our choice, as `sight_params()` on an object
    /// that is not a stalker (type −1, no object, FLT_MAX).
    fn default() -> Self {
        SightParams {
            sight_type: -1,
            object: None,
            vector: [FLT_MAX; 3],
        }
    }
}

/// A memory record (7.1): one part for every record class, so the base
/// classes' members read the derived classes' objects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryRecord {
    pub level_time: u32,
    pub last_level_time: u32,
    /// The remembered object's handle.
    pub object: Option<EnginePart>,
    pub visual_info: bool,
    pub sound_info: bool,
    pub hit_info: bool,
    pub power: f32,
    pub sound_type: i32,
    pub direction: [f32; 3],
    pub bone_index: u16,
    pub amount: f32,
    pub value: f32,
}

/// A `danger_object` (8.2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DangerObject {
    /// The object the danger comes from: its id and handle.
    pub object: Option<(u16, EnginePart)>,
    pub dependent_object: Option<EnginePart>,
    pub position: [f32; 3],
    pub time: u32,
    pub danger_type: i32,
    pub perceive_type: i32,
}

impl DangerObject {
    /// `a == b` (8.2).
    pub fn same(&self, other: &DangerObject) -> bool {
        self.object.map(|(id, _)| id) == other.object.map(|(id, _)| id)
            && self.danger_type == other.danger_type
            && self.perceive_type == other.perceive_type
    }
}

/// A `cef_storage`'s part: the store of evaluation functions (12).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationFunctions;

fn handle_value(part: Option<EnginePart>) -> Value {
    part.map_or(Value::Nil, Value::Object)
}

/// The members of 6 – 9.
pub(super) fn game_object_members(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = sight(class, ctx);
    let class = memory(class, ctx);
    let class = vision(class, ctx);
    let class = enemies(class, ctx);
    covers(class, ctx)
}

/// The sight request of `set_sight`'s arguments (6.1); `None` when no form
/// takes them.
fn sight_request(c: &Call) -> Result<SightRequest, String> {
    let no_form = || {
        format!(
            "game_object:{}: no form of it takes these arguments",
            c.name
        )
    };
    let flag = |i: usize| match c.arg(i) {
        Value::Boolean(b) => Ok(*b),
        _ => Err(no_form()),
    };
    match c.arg(0) {
        Value::Number(t) => {
            let t = to_bits32(*t);
            match (c.arg(1), c.arg(2), c.args.len()) {
                (Value::Boolean(a), Value::Boolean(b), 3) => Ok(SightRequest {
                    sight_type: t,
                    vector: None,
                    object: None,
                    a: *a,
                    b: *b,
                    c: None,
                }),
                (Value::Object(_), Value::Boolean(a), 3) => {
                    let v = c.vector(1).map_err(|_| no_form())?;
                    Ok(SightRequest {
                        sight_type: t,
                        vector: Some([v.x, v.y, v.z]),
                        object: None,
                        a: *a,
                        b: false,
                        c: None,
                    })
                }
                (Value::Nil | Value::Object(_), Value::Number(_), 3)
                | (Value::Nil | Value::Object(_), Value::Nil, 2) => {
                    let vector = match c.arg(1) {
                        Value::Nil => None,
                        _ => {
                            let v = c.vector(1).map_err(|_| no_form())?;
                            Some([v.x, v.y, v.z])
                        }
                    };
                    let fire = t == FIRE_POSITION;
                    Ok(SightRequest {
                        sight_type: if fire { POSITION } else { t },
                        vector,
                        object: None,
                        a: fire,
                        b: false,
                        c: None,
                    })
                }
                _ => Err(no_form()),
            }
        }
        Value::Object(_) => {
            let object = c.handle(0).map_err(|_| no_form())?;
            let n = c.args.len();
            if !(1..=4).contains(&n) {
                return Err(no_form());
            }
            let a = if n >= 2 { flag(1)? } else { false };
            let fire = n >= 3 && flag(2)?;
            let cc = if n == 4 { flag(3)? } else { false };
            Ok(SightRequest {
                sight_type: if fire { FIRE_OBJECT } else { OBJECT },
                vector: None,
                object,
                a,
                b: false,
                c: Some(cc),
            })
        }
        _ => Err(no_form()),
    }
}

/// 6.
fn sight(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "set_sight", |c| {
        let request = sight_request(c)?;
        // TODO(#1129): the sight control follows it (CH-10).
        if let Some(s) = &mut c.object()?.movement.stalker {
            s.sight = request;
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "sight_params", |c| {
        let request = c
            .object()?
            .movement
            .stalker
            .as_ref()
            .map(|s| s.sight.clone());
        let params = match request {
            None => SightParams::default(),
            Some(r) => SightParams {
                sight_type: r.sight_type,
                object: r
                    .object
                    .and_then(|id| c.objects.get(id))
                    .and_then(|o| o.handle),
                vector: r.vector.unwrap_or([FLT_MAX; 3]),
            },
        };
        Ok(vec![c.o.new_object("CSightParams", params)])
    });
    member(class, ctx, "get_current_direction", |c| {
        if c.object()?.clsid == SEARCHLIGHT_CLSID {
            // TODO(#1129): the searchlight's heading and pitch aren't kept.
            return Err(unavailable(c, 1129));
        }
        Ok(vec![c.new_vector([0.0; 3])])
    })
}

/// Whether handle argument `i`'s object is dead (a living object with
/// health 0 or less).
fn dead(c: &mut Call) -> Result<bool, String> {
    let o = c.object()?;
    Ok(o.kinds.has(Kinds::LIVING) && o.entity.as_ref().is_some_and(|e| e.health <= 0.0))
}

/// 7.3: the members on remembered objects. The fighter's memory is empty
/// (TODO(#1131)).
fn memory(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "memory_time", |c| {
        c.handle(0)?;
        Ok(number(0))
    });
    let class = member(class, ctx, "memory_position", |c| {
        c.handle(0)?;
        Ok(vec![c.new_vector([0.0; 3])])
    });
    let class = member(class, ctx, "enable_memory_object", |c| {
        c.handle(0)?;
        c.boolean(1)?;
        Ok(Vec::new())
    });
    member(class, ctx, "make_object_visible_somewhen", |c| {
        c.handle(0)?;
        Ok(Vec::new())
    })
}

/// 7.4.
fn vision(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "fov", |c| {
        match c.object()?.movement.eye.as_ref().map(|e| e.fov) {
            None => Ok(number(-1)),
            Some(Some(v)) => Ok(number(v)),
            Some(None) => Err(unavailable(c, 1131)),
        }
    });
    let class = member(class, ctx, "range", |c| {
        match c.object()?.movement.eye.as_ref().map(|e| e.range) {
            None => Ok(number(-1)),
            Some(Some(v)) => Ok(number(v)),
            Some(None) => Err(unavailable(c, 1131)),
        }
    });
    let class = member(class, ctx, "set_fov", |c| {
        let v = to_f32(c.number(0)?);
        let o = c.object()?;
        if o.kinds.has(Kinds::FIGHTER)
            && let Some(e) = &mut o.movement.eye
        {
            e.fov = Some(v);
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "set_range", |c| {
        let v = to_f32(c.number(0)?);
        let o = c.object()?;
        if o.kinds.has(Kinds::FIGHTER)
            && let Some(e) = &mut o.movement.eye
        {
            e.range = Some(v);
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "enable_vision", |c| {
        let on = c.boolean(0)?;
        if let Some(f) = &mut c.object()?.movement.fighter {
            f.vision_enabled = Some(on);
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "vision_enabled", |c| {
        match c
            .object()?
            .movement
            .fighter
            .as_ref()
            .map(|f| f.vision_enabled)
        {
            None => Ok(boolean(false)),
            Some(Some(v)) => Ok(boolean(v)),
            Some(None) => Err(unavailable(c, 1131)),
        }
    });
    let class = member(class, ctx, "visibility_threshold", |c| {
        if !c.object()?.kinds.has(Kinds::FIGHTER) {
            return Err(c.crash("the object is not a fighter"));
        }
        // The vision parameters' thresholds are CH-13's.
        Err(unavailable(c, 1131))
    });
    // Nothing is seen (TODO(#1131)): every form gives false; a dead living
    // object's diagnostic goes nowhere.
    member(class, ctx, "see", |c| {
        match c.arg(0) {
            Value::String(_) => drop(c.text(0)?),
            _ => drop(c.handle(0)?),
        }
        dead(c)?;
        Ok(boolean(false))
    })
}

/// 8.
fn enemies(class: EngineClass, ctx: &Context) -> EngineClass {
    // No enemy or danger is chosen (TODO(#1131)).
    let class = member(class, ctx, "best_enemy", |_| Ok(vec![Value::Nil]));
    let class = member(class, ctx, "get_enemy", |_| Ok(vec![Value::Nil]));
    let class = member(class, ctx, "best_danger", |_| Ok(vec![Value::Nil]));
    let class = member(class, ctx, "get_enemy_strength", |c| {
        if !c.object()?.kinds.has(Kinds::SCRIPTED) {
            return Ok(number(0));
        }
        Err(unavailable(c, 1131))
    });
    let class = member(class, ctx, "ignore_monster_threshold", |c| {
        if c.args.is_empty() {
            let s = c.object()?.movement.stalker.as_ref();
            return Ok(number(s.map_or(0.0, |s| s.ignore_monster_threshold)));
        }
        let v = to_f32(c.number(0)?).clamp(0.0, 1.0);
        if let Some(s) = &mut c.object()?.movement.stalker {
            s.ignore_monster_threshold = v;
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "max_ignore_monster_distance", |c| {
        if c.args.is_empty() {
            let s = c.object()?.movement.stalker.as_ref();
            return Ok(number(s.map_or(0.0, |s| s.max_ignore_monster_distance)));
        }
        let v = to_f32(c.number(0)?);
        if let Some(s) = &mut c.object()?.movement.stalker {
            s.max_ignore_monster_distance = v;
        }
        Ok(Vec::new())
    });
    // The section's key, its default when absent (8.1).
    let restore = |c: &mut Call| -> Result<Vec<Value>, String> {
        let system = c.ctx.physics.borrow().system.clone();
        let threshold = c.name == "restore_ignore_monster_threshold";
        let o = c.object()?;
        let section = o.section.clone().unwrap_or_default();
        if let Some(s) = &mut o.movement.stalker {
            let system = system.as_deref();
            if threshold {
                s.ignore_monster_threshold =
                    float_or(system, &section, "ignore_monster_threshold", 1.0);
            } else {
                s.max_ignore_monster_distance =
                    float_or(system, &section, "max_ignore_distance", 0.0);
            }
        }
        Ok(Vec::new())
    };
    let class = member(class, ctx, "restore_ignore_monster_threshold", restore);
    member(class, ctx, "restore_max_ignore_monster_distance", restore)
}

/// 9: no cover point is found (the evaluators and the level's cover points
/// are CH-18's and spec N's, TODO(#1130)).
fn covers(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "best_cover", |c| {
        c.vector(0)?;
        c.vector(1)?;
        for i in 2..5 {
            c.number(i)?;
        }
        Ok(vec![Value::Nil])
    });
    member(class, ctx, "safe_cover", |c| {
        c.vector(0)?;
        c.number(1)?;
        c.number(2)?;
        Ok(vec![Value::Nil])
    })
}

/// `CSightParams` (6.3).
fn sight_params_class() -> EngineClass {
    let class = EngineClass::new("CSightParams")
        .constructor(0, |_| Some(0), |_, _| Ok(SightParams::default()))
        .constructible()
        .property("m_sight_type", |p: &SightParams| {
            Value::Number(f64::from(p.sight_type))
        })
        .property("m_object", |p: &SightParams| handle_value(p.object))
        .property_with_objects(
            "m_vector",
            |p: &SightParams, o: &Objects| {
                let [x, y, z] = p.vector;
                Ok(o.new_object("vector", Vector::new(x, y, z)))
            },
            None,
        );
    SIGHT_TYPES
        .into_iter()
        .fold(class, |class, (name, value)| class.constant(name, value))
}

/// The memory record classes (7.1), bases first.
fn memory_classes() -> Vec<EngineClass> {
    let number = |f: fn(&MemoryRecord) -> f64| move |r: &MemoryRecord| Value::Number(f(r));
    let flag = |f: fn(&MemoryRecord) -> bool| move |r: &MemoryRecord| Value::Boolean(f(r));
    // `object_params` is script API N's (TODO(#1112)).
    let object_params = |class: EngineClass, name: &str| {
        class.property_with_objects(
            name,
            |_: &MemoryRecord, _: &Objects| {
                Err("object_params values are script API N's (TODO(#1112))".into())
            },
            None,
        )
    };
    let object = |class: EngineClass| {
        class.method("object", |r: &mut MemoryRecord, _| {
            Ok(vec![handle_value(r.object)])
        })
    };
    let memory_object = EngineClass::new("memory_object")
        .property("level_time", number(|r| f64::from(r.level_time)))
        .property("last_level_time", number(|r| f64::from(r.last_level_time)));
    let game = object(object_params(
        object_params(
            EngineClass::new("game_memory_object").base("memory_object"),
            "object_info",
        ),
        "self_info",
    ));
    let entity = object(object_params(
        object_params(
            EngineClass::new("entity_memory_object").base("memory_object"),
            "object_info",
        ),
        "self_info",
    ));
    let visible = EngineClass::new("visible_memory_object").base("game_memory_object");
    let sound = EngineClass::new("sound_memory_object")
        .base("game_memory_object")
        .property("power", number(|r| f64::from(r.power)))
        .method("type", |r: &mut MemoryRecord, _| {
            Ok(vec![Value::Number(f64::from(r.sound_type))])
        });
    let info = EngineClass::new("memory_info")
        .base("visible_memory_object")
        .property("visual_info", flag(|r| r.visual_info))
        .property("sound_info", flag(|r| r.sound_info))
        .property("hit_info", flag(|r| r.hit_info));
    let hit = EngineClass::new("hit_memory_object")
        .base("entity_memory_object")
        .property_with_objects(
            "direction",
            |r: &MemoryRecord, o: &Objects| {
                let [x, y, z] = r.direction;
                Ok(o.new_object("vector", Vector::new(x, y, z)))
            },
            None,
        )
        .property("bone_index", number(|r| f64::from(r.bone_index)))
        .property("amount", number(|r| f64::from(r.amount)));
    let not_yet_visible = object(
        EngineClass::new("not_yet_visible_object")
            .property("value", number(|r| f64::from(r.value))),
    );
    vec![
        memory_object,
        game,
        entity,
        visible,
        sound,
        info,
        hit,
        not_yet_visible,
    ]
}

/// `danger_object` (8.2).
fn danger_class() -> EngineClass {
    let class = EngineClass::new("danger_object")
        .method("object", |d: &mut DangerObject, _| {
            Ok(vec![handle_value(d.object.map(|(_, h)| h))])
        })
        .method("dependent_object", |d: &mut DangerObject, _| {
            Ok(vec![handle_value(d.dependent_object)])
        })
        .method_with_objects("position", |d: &mut DangerObject, o, _| {
            let [x, y, z] = d.position;
            Ok(vec![o.new_object("vector", Vector::new(x, y, z))])
        })
        .method("time", |d: &mut DangerObject, _| {
            Ok(vec![Value::Number(f64::from(d.time))])
        })
        .method("type", |d: &mut DangerObject, _| {
            Ok(vec![Value::Number(f64::from(d.danger_type))])
        })
        .method("perceive_type", |d: &mut DangerObject, _| {
            Ok(vec![Value::Number(f64::from(d.perceive_type))])
        })
        .method_with_objects("__eq", |d: &mut DangerObject, o, args| {
            let same = match o.get::<DangerObject>(args.first().unwrap_or(&Value::Nil)) {
                Some(Operand::This) => true,
                Some(Operand::Other(b)) => d.same(&b),
                None => false,
            };
            Ok(vec![Value::Boolean(same)])
        });
    DANGER_CONSTANTS
        .into_iter()
        .fold(class, |class, (name, value)| class.constant(name, value))
}

/// `cef_storage` (12): its 8 forms of `evaluate` (TODO(#1135)).
fn cef_storage_class() -> EngineClass {
    EngineClass::new("cef_storage").method("evaluate", |_: &mut EvaluationFunctions, _| {
        Err("cef_storage:evaluate: not available yet (TODO(#1135))".into())
    })
}

/// The classes of 6.3, 7.1, 8.2 and 12.
pub(super) fn classes(_: &Context) -> Vec<EngineClass> {
    let mut classes = vec![sight_params_class()];
    classes.extend(memory_classes());
    classes.push(danger_class());
    classes.push(cef_storage_class());
    classes
}

/// Installs the memory iterators (7.2) over a hidden function: on a
/// fighter the memory's list (empty, TODO(#1131)), on another object a read
/// through no object, fatal here.
pub(super) fn install(s: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    let c = ctx.clone();
    let lua = s.lua_mut();
    lua.register_with_objects("__volya_memory", move |maker, args| {
        c.stopped()?;
        let name = match args.first() {
            Some(Value::String(n)) => String::from_utf8_lossy(n).into_owned(),
            _ => return Err("memory: operation expected".into()),
        };
        let id = (args.get(1))
            .and_then(|v| maker.get::<Handle>(v))
            .map(|h| h.id)
            .ok_or_else(|| format!("game_object:{name}: game object expected"))?;
        let fighter = (c.objects.borrow().get(id))
            .ok_or_else(|| format!("game object {id} no longer exists"))?
            .kinds
            .has(Kinds::FIGHTER);
        if !fighter {
            return Err(c.fatal(
                &name,
                line!(),
                "<no expression>",
                &format!("{name}: the object is not a fighter"),
            ));
        }
        Ok(vec![Value::Table(Vec::new())])
    })?;
    lua.exec("=memory", include_bytes!("sight.lua")).map(drop)
}
