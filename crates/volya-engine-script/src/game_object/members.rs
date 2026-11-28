// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The class `game_object` (1.2): the handle with this spec's members,
//! identity (3), place (4), condition (5), zones, restrictors, use and
//! visibility (6), trade parameters and character names (7), and its
//! constants (8). It has no constructor: scripts only receive handles.
//!
//! Each member asks its object's kind (2.2); on an object without it, it
//! does nothing and gives its section's fallback (2.3). Members the spec
//! believes crash on an object without the part they read are fatal errors
//! here (our wording).
//!
//! Later specs add their members to the class [`game_object_class`]
//! builds before it is registered.

use volya_engine_core::config::value;
use volya_engine_formats::cp1251;
use volya_engine_formats::model::transform_point;

use super::engine_classes::Hit;
use super::{Context, Event, GameObject, GameObjects, HitEvent, Kinds, Restrictor};
use crate::Value;
use crate::classes::{EngineClass, EnginePart, Objects, Operand};
use crate::files::IniFile;
use crate::values::{Vector, lua_number, to_bits32, to_f32};

/// A handle's part (2.1): the engine object it refers to, and its own part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle {
    pub id: u16,
    pub part: EnginePart,
}

/// The 22 constants (8).
pub const CONSTANTS: [(&str, i32); 22] = [
    ("game_path", 0),
    ("level_path", 1),
    ("patrol_path", 2),
    ("no_path", 3),
    ("movement", 0),
    ("watch", 1),
    ("animation", 2),
    ("sound", 3),
    ("particle", 4),
    ("object", 5),
    ("action_type_count", 6),
    ("friend", 0),
    ("neutral", 1),
    ("enemy", 2),
    ("dummy", -1),
    ("relation_kill", 0),
    ("relation_attack", 1),
    ("relation_fight_help_human", 2),
    ("relation_fight_help_monster", 4),
    ("dialog_pda_msg", 0),
    ("info_pda_msg", 1),
    ("no_pda_msg", 2),
];

/// `is_body_turning`'s threshold (5.5).
const TURNING: f32 = 0.00001;

/// `inside`'s default radius (6.2).
const INSIDE_RADIUS: f32 = 0.001;

type Results = Result<Vec<Value>, String>;

/// What a member gets: the engine objects, its object's id, the
/// arguments and the objects among them.
struct Call<'a> {
    ctx: &'a Context,
    objects: &'a mut GameObjects,
    id: u16,
    args: &'a [Value],
    o: &'a Objects<'a>,
    name: &'static str,
}

impl Call<'_> {
    fn object(&mut self) -> Result<&mut GameObject, String> {
        let id = self.id;
        self.objects
            .get_mut(id)
            .ok_or_else(|| format!("game object {id} no longer exists"))
    }

    fn arg(&self, i: usize) -> &Value {
        self.args.get(i).unwrap_or(&Value::Nil)
    }

    /// A number argument (our wording for a mismatch, depends on A).
    fn number(&self, i: usize) -> Result<f64, String> {
        match self.arg(i) {
            Value::Number(n) => Ok(*n),
            _ => Err(format!("{}: number expected", self.name)),
        }
    }

    fn text(&self, i: usize) -> Result<Vec<u8>, String> {
        match self.arg(i) {
            Value::String(s) => Ok(s.clone()),
            _ => Err(format!("{}: text expected", self.name)),
        }
    }

    fn boolean(&self, i: usize) -> Result<bool, String> {
        match self.arg(i) {
            Value::Boolean(b) => Ok(*b),
            _ => Err(format!("{}: boolean expected", self.name)),
        }
    }

    fn vector(&self, i: usize) -> Result<Vector, String> {
        match self.o.get::<Vector>(self.arg(i)) {
            Some(Operand::Other(v)) => Ok(v),
            _ => Err(format!("{}: vector expected", self.name)),
        }
    }

    /// The object id of handle argument `i`, or `None` for nil.
    fn handle(&self, i: usize) -> Result<Option<u16>, String> {
        match self.arg(i) {
            Value::Nil => Ok(None),
            v => match self.o.get::<Handle>(v) {
                Some(Operand::Other(h)) => Ok(Some(h.id)),
                _ => Err(format!("{}: game object expected", self.name)),
            },
        }
    }

    fn new_vector(&self, v: [f32; 3]) -> Value {
        self.o.new_object("vector", Vector::new(v[0], v[1], v[2]))
    }

    /// A fatal error standing for a crash the spec believes (our wording).
    fn crash(&self, what: &str) -> String {
        self.ctx.fatal(
            self.name,
            line!(),
            "<no expression>",
            &format!("{}: {what}", self.name),
        )
    }
}

fn text_value(text: Option<&[u8]>) -> Value {
    text.map_or(Value::Nil, |t| Value::String(t.to_vec()))
}

fn number(n: impl Into<f64>) -> Vec<Value> {
    vec![Value::Number(n.into())]
}

fn boolean(b: bool) -> Vec<Value> {
    vec![Value::Boolean(b)]
}

/// Adds member `name` running `f`.
fn member(
    class: EngineClass,
    ctx: &Context,
    name: &'static str,
    f: impl Fn(&mut Call) -> Results + 'static,
) -> EngineClass {
    let ctx = ctx.clone();
    class.method_with_objects(name, move |h: &mut Handle, o, args| {
        ctx.stopped()?;
        let mut objects = ctx.objects.borrow_mut();
        let mut call = Call {
            ctx: &ctx,
            objects: &mut objects,
            id: h.id,
            args,
            o,
            name,
        };
        f(&mut call)
    })
}

/// The class `game_object` with this spec's members and constants.
pub(crate) fn game_object_class(ctx: &Context) -> EngineClass {
    let class = EngineClass::new("game_object");
    let class = identity(class, ctx);
    let class = place(class, ctx);
    let class = conditions(class, ctx);
    let class = entities_and_items(class, ctx);
    let class = fighters(class, ctx);
    let class = zones(class, ctx);
    let class = trade(class, ctx);
    CONSTANTS
        .into_iter()
        .fold(class, |class, (name, value)| class.constant(name, value))
}

/// 3.
fn identity(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "id", |c| Ok(number(c.object()?.id)));
    let class = member(class, ctx, "name", |c| {
        Ok(vec![text_value(c.object()?.name.as_deref())])
    });
    let class = member(class, ctx, "section", |c| {
        Ok(vec![text_value(c.object()?.section.as_deref())])
    });
    let class = member(class, ctx, "clsid", |c| {
        Ok(number(c.object()?.clsid as f64))
    });
    let class = member(class, ctx, "story_id", |c| Ok(number(c.object()?.story_id)));
    let class = member(class, ctx, "spawn_ini", |c| {
        Ok(vec![
            c.object()?.spawn_ini.map_or(Value::Nil, Value::Object),
        ])
    });
    member(class, ctx, "parent", |c| {
        let parent = c.object()?.parent;
        Ok(vec![c.objects.handle_value(parent)])
    })
}

/// 4.
fn place(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "position", |c| {
        let p = c.object()?.placement[3];
        Ok(vec![c.new_vector([p[0], p[1], p[2]])])
    });
    let class = member(class, ctx, "direction", |c| {
        let k = c.object()?.placement[2];
        Ok(vec![c.new_vector([k[0], k[1], k[2]])])
    });
    let class = member(class, ctx, "center", |c| {
        let o = c.object()?;
        let Some(visual) = &o.visual else {
            return Err(c.crash("the object has no visual"));
        };
        let centre = transform_point(&o.placement, visual.centre);
        Ok(vec![c.new_vector(centre)])
    });
    let class = member(class, ctx, "bone_position", |c| {
        let name = c.text(0)?;
        let o = c.object()?;
        let placement = o.placement;
        let Some(visual) = &o.visual else {
            return Err(c.crash("the object has no visual"));
        };
        let bone = if name.is_empty() {
            Some(visual.root)
        } else {
            visual.bone(&name)
        };
        // A bone the visual lacks is unestablished (believed an invalid
        // read): fatal here.
        let Some(transform) = bone.and_then(|b| visual.bones.get(b)).map(|(_, t)| t[3]) else {
            let what = format!("no bone {}", cp1251::decode(&name));
            return Err(c.crash(&what));
        };
        let p = transform_point(&placement, [transform[0], transform[1], transform[2]]);
        Ok(vec![c.new_vector(p)])
    });
    let class = member(class, ctx, "level_vertex_id", |c| {
        Ok(number(c.object()?.level_vertex))
    });
    let class = member(class, ctx, "game_vertex_id", |c| {
        Ok(number(c.object()?.game_vertex))
    });
    let class = member(class, ctx, "level_vertex_light", |c| {
        let vertex = to_bits32(c.number(0)?) as u32;
        // A vertex beyond the level's is unestablished (read unchecked):
        // fatal here.
        let Some(&byte) = c.objects.level_vertex_lights.get(vertex as usize) else {
            return Err(c.crash(&format!("no level vertex {vertex}")));
        };
        Ok(number(f32::from(byte >> 4) / 15.0))
    });
    let class = member(class, ctx, "set_actor_position", |c| {
        let p = c.vector(0)?;
        let id = c.id;
        let o = c.object()?;
        if o.kinds.has(Kinds::PLAYER) {
            o.placement[3] = [p.x, p.y, p.z, 1.0];
            let placement = o.placement;
            c.objects.push(Event::ForcePlacement {
                object: id,
                placement,
            });
        }
        Ok(Vec::new())
    });
    member(class, ctx, "set_actor_direction", |c| {
        let yaw = to_f32(c.number(0)?);
        let id = c.id;
        if c.object()?.kinds.has(Kinds::PLAYER) {
            c.objects.push(Event::CameraAngles {
                object: id,
                angles: [yaw, 0.0, 0.0],
            });
        }
        Ok(Vec::new())
    })
}

/// A living object's condition (5.1).
#[derive(Clone, Copy)]
enum Condition {
    Health,
    PsyHealth,
    Power,
    Radiation,
    Morale,
}

impl Condition {
    /// The value of a living object; its health is the entity's (5.2,
    /// believed the same).
    fn value(self, o: &mut GameObject) -> Option<&mut f32> {
        let c = o.conditions.as_mut()?;
        Some(match self {
            Condition::Health => &mut o.entity.as_mut()?.health,
            Condition::PsyHealth => &mut c.psy_health,
            Condition::Power => &mut c.power,
            Condition::Radiation => &mut c.radiation,
            Condition::Morale => &mut c.morale,
        })
    }
}

/// 5.1: the 5 properties.
fn conditions(class: EngineClass, ctx: &Context) -> EngineClass {
    [
        ("health", Condition::Health),
        ("psy_health", Condition::PsyHealth),
        ("power", Condition::Power),
        ("radiation", Condition::Radiation),
        ("morale", Condition::Morale),
    ]
    .into_iter()
    .fold(class, |class, (name, condition)| {
        let get = ctx.clone();
        let set = ctx.clone();
        class.writable_property(
            name,
            move |h: &Handle| {
                let mut objects = get.objects.borrow_mut();
                let value = objects
                    .get_mut(h.id)
                    .and_then(|o| condition.value(o).map(|v| *v));
                Value::Number(f64::from(value.unwrap_or(-1.0)))
            },
            move |h: &mut Handle, v| {
                set.stopped()?;
                let amount = to_f32(lua_number(v));
                let mut objects = set.objects.borrow_mut();
                let Some(o) = objects.get_mut(h.id) else {
                    return Ok(());
                };
                let harmable = o.conditions.as_ref().is_some_and(|c| c.may_be_harmed);
                if matches!(condition, Condition::Health) && amount <= 0.0 && !harmable {
                    return Ok(());
                }
                // Assigning adds (5.1). That the read returns the value
                // before a later update applies it is disputed in review:
                // the value changes at once here.
                if let Some(value) = condition.value(o) {
                    *value += amount;
                }
                Ok(())
            },
        )
    })
}

/// 5.2, 5.3, 5.4.
fn entities_and_items(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "alive", |c| {
        Ok(boolean(
            c.object()?.entity.as_ref().is_some_and(|e| e.health > 0.0),
        ))
    });
    let class = member(class, ctx, "max_health", |c| {
        Ok(number(
            c.object()?.entity.as_ref().map_or(-1.0, |e| e.max_health),
        ))
    });
    let class = member(class, ctx, "death_time", |c| {
        Ok(number(
            c.object()?.entity.as_ref().map_or(0, |e| e.death_time),
        ))
    });
    let class = member(class, ctx, "kill", |c| {
        let who = c.handle(0)?;
        let now = c.ctx.services.borrow().device.time_global();
        let game_time = c.objects.game_time;
        let id = c.id;
        let o = c.object()?;
        let destroying = o.destroying;
        let Some(e) = o.entity.as_mut().filter(|e| e.death_time == 0) else {
            return Ok(Vec::new());
        };
        let killer = who.unwrap_or(id);
        if killer == id && e.killer.is_some() {
            return Ok(Vec::new());
        }
        e.killer = Some(killer);
        e.death_time = now;
        e.death_game_time = game_time;
        if !destroying {
            c.objects.push(Event::Death { object: id, killer });
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "get_bleeding", |c| {
        let Some(conditions) = &c.object()?.conditions else {
            return Ok(number(-1.0));
        };
        let wounds = &conditions.wounds;
        if wounds.is_empty() {
            return Ok(number(0.0));
        }
        let total: f32 = wounds.iter().map(|w| w.iter().sum::<f32>()).sum();
        Ok(number(total / wounds.len() as f32))
    });
    let who_hit = |c: &mut Call, section: bool| -> Results {
        let who = c.object()?.conditions.as_ref().and_then(|k| k.who_hit);
        let hitter = who.and_then(|id| c.objects.get(id));
        let text = hitter.and_then(|o| {
            if section {
                o.section.as_deref()
            } else {
                o.name.as_deref()
            }
        });
        Ok(vec![text_value(text)])
    };
    let class = member(class, ctx, "who_hit_name", move |c| who_hit(c, false));
    let class = member(class, ctx, "who_hit_section_name", move |c| {
        who_hit(c, true)
    });
    let class = member(class, ctx, "condition", |c| {
        Ok(number(
            c.object()?.item.as_ref().map_or(0.0, |i| i.condition),
        ))
    });
    let class = member(class, ctx, "set_condition", |c| {
        let value = to_f32(c.number(0)?);
        if let Some(item) = &mut c.object()?.item {
            item.condition = value.clamp(0.0, 1.0);
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "cost", |c| {
        Ok(number(c.object()?.item.as_ref().map_or(0, |i| i.cost)))
    });
    let class = member(class, ctx, "mass", |c| {
        Ok(number(
            c.object()?.physics.as_ref().map_or(-1.0, |p| p.mass),
        ))
    });
    member(class, ctx, "hit", deliver_hit)
}

/// `obj:hit(h)` (5.6): the hit event.
fn deliver_hit(c: &mut Call) -> Results {
    let hit = match c.o.get::<Hit>(c.arg(0)) {
        Some(Operand::Other(hit)) => hit.0.borrow().clone(),
        _ => return Err("hit: a hit expected".into()),
    };
    // Without a draftsman the hitter's id is read through nothing (believed
    // a crash): fatal here, as for a handle no longer valid.
    let Some(hitter) = hit.draftsman.and_then(|p| c.objects.of_handle(p)) else {
        return Err(c.crash("the hit has no draftsman"));
    };
    let time = c.objects.server_time;
    let target = c.id;
    let o = c.object()?;
    let bone = match &hit.bone {
        None => 0,
        Some(name) => match &o.visual {
            // TODO(#1001): gap 3, the simplest reading: a named bone on an
            // object without a visual is looked up in no visual, believed a
            // crash.
            None => return Err(c.crash("the object has no visual")),
            // A name the visual lacks gives the lookup's "no bone" index
            // (our choice: the spec doesn't say).
            Some(visual) => visual
                .bone(name)
                .and_then(|b| u16::try_from(b).ok())
                .unwrap_or(u16::MAX),
        },
    };
    let d = hit.direction;
    c.objects.push(Event::Hit(HitEvent {
        time,
        target,
        power: hit.power,
        direction: [d.x, d.y, d.z],
        hitter,
        weapon: 0,
        bone,
        position: [0.0; 3],
        impulse: hit.impulse,
        hit_type: hit.hit_type,
        armour_piercing: (hit.hit_type == 8).then_some(0.0),
        aimed_bullet: false,
    }));
    Ok(Vec::new())
}

/// 5.5 (with the unlisted `invulnerable`).
fn fighters(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "invulnerable", |c| {
        if c.args.is_empty() {
            let o = c.object()?;
            return Ok(boolean(o.fighter.as_ref().is_some_and(|f| f.invulnerable)));
        }
        let on = c.boolean(0)?;
        if let Some(f) = &mut c.object()?.fighter {
            f.invulnerable = on;
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "critically_wounded", |c| {
        Ok(boolean(
            c.object()?
                .fighter
                .as_ref()
                .is_some_and(|f| f.critically_wounded),
        ))
    });
    let class = member(class, ctx, "get_corpse", |c| {
        let corpse = c.object()?.fighter.as_ref().and_then(|f| f.corpse);
        let corpse = corpse.filter(|id| c.objects.get(*id).is_some_and(|o| !o.destroying));
        Ok(vec![c.objects.handle_value(corpse)])
    });
    let class = member(class, ctx, "is_body_turning", |c| {
        let o = c.object()?;
        let turning = |(current, target): (f32, f32)| (current - target).abs() >= TURNING;
        let body = o.fighter.as_ref().is_some_and(|f| turning(f.body_heading));
        let head = o.stalker.as_ref().is_some_and(|s| turning(s.head_heading));
        Ok(boolean(body || head))
    });
    member(class, ctx, "wounded", |c| {
        if c.args.is_empty() {
            return Ok(boolean(
                c.object()?.stalker.as_ref().is_some_and(|s| s.wounded),
            ));
        }
        let on = c.boolean(0)?;
        let id = c.id;
        let Some(s) = &mut c.object()?.stalker else {
            return Ok(Vec::new());
        };
        if s.wounded != on {
            s.wounded = on;
            c.objects.push(Event::Wounded {
                object: id,
                wounded: on,
            });
        }
        Ok(Vec::new())
    })
}

/// Whether sphere (`p`, `r`) overlaps restrictor `shape` (6.2).
pub fn inside(shape: &Restrictor, p: [f32; 3], r: f32) -> bool {
    let distance = |a: [f32; 3], b: [f32; 3]| {
        let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    };
    let (centre, radius) = shape.bound;
    if distance(p, centre) >= radius + r {
        return false;
    }
    if shape
        .spheres
        .iter()
        .any(|&(c, radius)| distance(c, p) < radius + r)
    {
        return true;
    }
    shape.boxes.iter().any(|planes| {
        planes
            .iter()
            .all(|(n, d)| n[0] * p[0] + n[1] * p[1] + n[2] * p[2] + d <= r)
    })
}

/// 6.
fn zones(class: EngineClass, ctx: &Context) -> EngineClass {
    // 6.1: these don't test the kind (believed a crash): fatal here.
    let anomaly = |c: &mut Call| -> Result<(), String> {
        if c.object()?.anomaly.is_none() {
            return Err(c.crash("the object is not an anomalous zone"));
        }
        Ok(())
    };
    let class = member(class, ctx, "enable_anomaly", move |c| {
        anomaly(c)?;
        if let Some(a) = &mut c.object()?.anomaly {
            a.state = 0;
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "disable_anomaly", move |c| {
        anomaly(c)?;
        if let Some(a) = &mut c.object()?.anomaly {
            a.state = 4;
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "get_anomaly_power", move |c| {
        anomaly(c)?;
        Ok(number(c.object()?.anomaly.map_or(0.0, |a| a.power)))
    });
    let class = member(class, ctx, "set_anomaly_power", move |c| {
        let power = to_f32(c.number(0)?);
        anomaly(c)?;
        if let Some(a) = &mut c.object()?.anomaly {
            a.power = power;
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "inside", |c| {
        let p = c.vector(0)?;
        let r = if c.args.len() > 1 {
            to_f32(c.number(1)?)
        } else {
            INSIDE_RADIUS
        };
        let o = c.object()?;
        Ok(boolean(
            o.restrictor
                .as_ref()
                .is_some_and(|s| inside(s, [p.x, p.y, p.z], r)),
        ))
    });
    let class = member(class, ctx, "set_nonscript_usable", |c| {
        let on = c.boolean(0)?;
        c.object()?.usable = on;
        Ok(Vec::new())
    });
    let class = member(class, ctx, "active_zone_contact", |c| {
        let id = c.number(0)?;
        let o = c.object()?;
        Ok(boolean(o.contacts.as_ref().is_some_and(|contacts| {
            contacts.iter().any(|&k| f64::from(k) == id)
        })))
    });
    let class = member(class, ctx, "set_invisible", |c| {
        let on = c.boolean(0)?;
        let id = c.id;
        let o = c.object()?;
        if let Some(flag) = &mut o.invisible {
            *flag = on;
            c.objects.push(Event::Visibility {
                object: id,
                hidden: on,
            });
        }
        Ok(Vec::new())
    });
    member(class, ctx, "set_manual_invisibility", |_| Ok(Vec::new()))
}

/// Which factors a trade member sets (7).
#[derive(Clone, Copy)]
enum Factors {
    Sell,
    Buy,
}

/// Section `section` of `ini`, for an inventory owner's member: a missing
/// one is a failed check (believed fatal).
fn trade_section<'a>(
    c: &Call,
    ini: &'a IniFile,
    section: &[u8],
) -> Result<volya_engine_core::config::Section<'a>, String> {
    let name = cp1251::decode(section);
    ini.0.section(&name).ok_or_else(|| {
        c.ctx
            .fatal(c.name, line!(), &format!("cannot find section {name}"), "")
    })
}

fn ini_arg(c: &Call, i: usize) -> Result<IniFile, String> {
    match c.o.get::<IniFile>(c.arg(i)) {
        Some(Operand::Other(ini)) => Ok(ini),
        _ => Err(format!("{}: ini_file expected", c.name)),
    }
}

/// `sell_condition` and `buy_condition` (7).
fn set_factors(c: &mut Call, which: Factors) -> Results {
    if c.object()?.owner.is_none() {
        return Ok(Vec::new());
    }
    let factors = if matches!(c.arg(0), Value::Number(_)) {
        let (f1, f2) = (to_f32(c.number(0)?), to_f32(c.number(1)?));
        (Some((f1, f2)), None)
    } else {
        let ini = ini_arg(c, 0)?;
        let section = c.text(1)?;
        let found = trade_section(c, &ini, &section)?;
        // A line without a value is handled separately (unestablished
        // how): skipped here.
        let items = found
            .entries()
            .filter_map(|e| {
                let [a, b] = value::floats::<2>(e.value()?);
                Some((crate::game_object::game_bytes(e.key()), a, b))
            })
            .collect::<Vec<_>>();
        (None, Some(items))
    };
    let owner = c.object()?.owner.as_mut().expect("tested above");
    let target = match which {
        Factors::Sell => &mut owner.sell,
        Factors::Buy => &mut owner.buy,
    };
    match factors {
        (Some(default), _) => target.default = default,
        (_, Some(items)) => target.items = items,
        _ => {}
    }
    Ok(Vec::new())
}

/// 7.
fn trade(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "sell_condition", |c| {
        set_factors(c, Factors::Sell)
    });
    let class = member(class, ctx, "buy_condition", |c| {
        set_factors(c, Factors::Buy)
    });
    let class = member(class, ctx, "show_condition", |c| {
        if c.object()?.owner.is_none() {
            return Ok(Vec::new());
        }
        let ini = ini_arg(c, 0)?;
        let section = c.text(1)?;
        // A missing section: the same failed check (our choice).
        let found = trade_section(c, &ini, &section)?;
        let show = found
            .entries()
            .filter(|e| e.value().is_none())
            .map(|e| crate::game_object::game_bytes(e.key()))
            .collect();
        if let Some(owner) = &mut c.object()?.owner {
            owner.show = show;
        }
        Ok(Vec::new())
    });
    let class = member(class, ctx, "profile_name", |c| {
        let o = c.object()?;
        Ok(vec![text_value(
            o.owner.as_ref().and_then(|w| w.profile.as_deref()),
        )])
    });
    member(class, ctx, "character_name", |c| {
        let o = c.object()?;
        Ok(vec![text_value(
            o.owner.as_ref().map(|w| w.character_name.as_slice()),
        )])
    })
}
