// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The classes `hit` (9), `CGameObject` with the overrides the engine
//! calls (10.1, 10.2), the client classes (10.3) and `explosive` (10.4).

use std::cell::RefCell;
use std::rc::Rc;

use super::callbacks::{self, ObjectEvent};
use super::members::Handle;
use super::{CLIENT_BASE, Context, Event, ObjectClass, SCRIPT_ZONE_CLASSES, client_classes};
use crate::Value;
use crate::classes::{EngineClass, EnginePart, Objects, Operand};
use crate::values::{Vector, lua_number, to_bits32, to_f32};

/// A `hit`'s values (9).
#[derive(Clone, Debug, PartialEq)]
pub struct HitData {
    pub power: f32,
    pub direction: Vector,
    /// The bone named by `bone(name)`, resolved when the hit is delivered.
    pub bone: Option<Vec<u8>>,
    /// The handle of the object the hit comes from.
    pub draftsman: Option<EnginePart>,
    pub impulse: f32,
    pub hit_type: i32,
}

impl Default for HitData {
    /// `hit()` (9).
    fn default() -> Self {
        HitData {
            power: 100.0,
            direction: Vector::new(1.0, 0.0, 0.0),
            bone: None,
            draftsman: None,
            impulse: 100.0,
            hit_type: 3,
        }
    }
}

/// A `hit` object's part: its values, shared with the copy constructor's
/// table of live hits.
#[derive(Clone, Debug)]
pub struct Hit(pub Rc<RefCell<HitData>>);

/// The hit types as `hit`'s constants (9): `dummy` is 11, not the listing's
/// 10.
pub const HIT_TYPES: [(&str, i32); 10] = [
    ("burn", 0),
    ("shock", 1),
    ("strike", 2),
    ("wound", 3),
    ("radiation", 4),
    ("telepatic", 5),
    ("chemical_burn", 6),
    ("explosion", 7),
    ("fire_wound", 8),
    ("dummy", 11),
];

/// A `CGameObject`'s part (10): the engine object it stands for once it
/// has spawned.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientObject {
    /// Its own part, which `_construct` returns.
    pub part: EnginePart,
    /// The engine class it was constructed as.
    pub class: &'static str,
    pub object: Option<u16>,
}

/// An `explosive`'s part (10.4): the object whose explosive part it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Explosive {
    pub object: u16,
}

fn new_hit(ctx: &Context, part: EnginePart, data: HitData) -> Hit {
    let hit = Hit(Rc::new(RefCell::new(data)));
    let mut hits = ctx.hits.borrow_mut();
    hits.retain(|_, h| h.strong_count() > 0);
    hits.insert(part, Rc::downgrade(&hit.0));
    hit
}

/// The live hit whose object `value` is.
fn hit_of(ctx: &Context, value: &Value) -> Option<HitData> {
    let Value::Object(part) = value else {
        return None;
    };
    let hit = ctx.hits.borrow().get(part)?.upgrade()?;
    let data = hit.borrow().clone();
    Some(data)
}

fn number_property(
    class: EngineClass,
    name: &str,
    get: fn(&HitData) -> f64,
    set: fn(&mut HitData, f64),
) -> EngineClass {
    class.writable_property(
        name,
        move |h: &Hit| Value::Number(get(&h.0.borrow())),
        move |h: &mut Hit, v| {
            set(&mut h.0.borrow_mut(), lua_number(v));
            Ok(())
        },
    )
}

/// `hit` (9).
fn hit_class(ctx: &Context) -> EngineClass {
    let c = ctx.clone();
    let made = ctx.clone();
    let c2 = ctx.clone();
    let copied = ctx.clone();
    let mut class = EngineClass::new("hit")
        .constructor(
            0,
            |_| Some(0),
            move |part, _| Ok(new_hit(&made, part, HitData::default())),
        )
        .constructor(
            1,
            move |args| args.first().and_then(|v| hit_of(&c, v)).map(|_| 0),
            move |part, args| {
                let data = args
                    .first()
                    .and_then(|v| hit_of(&c2, v))
                    .ok_or_else(|| "hit: a hit expected".to_owned())?;
                Ok(new_hit(&copied, part, data))
            },
        )
        .constructible();
    class = number_property(
        class,
        "power",
        |h| f64::from(h.power),
        |h, n| h.power = to_f32(n),
    );
    class = number_property(
        class,
        "impulse",
        |h| f64::from(h.impulse),
        |h, n| h.impulse = to_f32(n),
    );
    class = number_property(
        class,
        "type",
        |h| f64::from(h.hit_type),
        |h, n| h.hit_type = to_bits32(n),
    );
    class = class.property_with_objects(
        "direction",
        |h: &Hit, o| Ok(o.new_object("vector", h.0.borrow().direction)),
        Some(Box::new(|h: &mut Hit, o: &Objects, v: &Value| {
            match o.get::<Vector>(v) {
                Some(Operand::Other(w)) => {
                    h.0.borrow_mut().direction = w;
                    Ok(())
                }
                // Our wording (depends on A).
                _ => Err("direction: a vector expected".into()),
            }
        })),
    );
    class = class.property_with_objects(
        "draftsman",
        |h: &Hit, _| Ok(h.0.borrow().draftsman.map_or(Value::Nil, Value::Object)),
        Some(Box::new(|h: &mut Hit, o: &Objects, v: &Value| {
            let draftsman = match v {
                Value::Nil => None,
                v => match o.get::<Handle>(v) {
                    Some(Operand::Other(handle)) => Some(handle.part),
                    _ => return Err("draftsman: a game object expected".into()),
                },
            };
            h.0.borrow_mut().draftsman = draftsman;
            Ok(())
        })),
    );
    class = class.method("bone", |h: &mut Hit, args| match args.first() {
        Some(Value::String(name)) => {
            h.0.borrow_mut().bone = Some(name.clone());
            Ok(Vec::new())
        }
        _ => Err("bone: text expected".into()),
    });
    HIT_TYPES
        .into_iter()
        .fold(class, |class, (name, value)| class.constant(name, value))
}

/// `CGameObject` (10.1): its methods, with the engine's own
/// implementations of the overrides it dispatches (10.2).
fn game_object_base(ctx: &Context) -> EngineClass {
    let visual = ctx.clone();
    let enabled = ctx.clone();
    let visible = ctx.clone();
    let spawn = ctx.clone();
    let used = ctx.clone();
    EngineClass::new(CLIENT_BASE)
        .base("DLL_Pure")
        .constructor(
            0,
            |_| Some(0),
            |part, _| {
                Ok(ClientObject {
                    part,
                    class: CLIENT_BASE,
                    object: None,
                })
            },
        )
        // The engine's own `_construct` returns the object itself (spec B 5.1).
        .object_method("_construct", |c: &mut ClientObject, _| Ok(Some(c.part)))
        .object_method("Visual", move |c: &mut ClientObject, _| {
            let objects = visual.objects.borrow();
            Ok(c.object
                .and_then(|id| objects.get(id))
                .and_then(|o| o.visual.as_ref())
                .and_then(|v| v.part))
        })
        // Numbers 1 or 0 (believed); 0 before it spawns (our choice).
        .method("getEnabled", move |c: &mut ClientObject, _| {
            let objects = enabled.objects.borrow();
            let on = c
                .object
                .and_then(|id| objects.get(id))
                .is_some_and(|o| o.enabled);
            Ok(vec![Value::Number(f64::from(u8::from(on)))])
        })
        .method("getVisible", move |c: &mut ClientObject, _| {
            let objects = visible.objects.borrow();
            let on = c
                .object
                .and_then(|id| objects.get(id))
                .is_some_and(|o| o.visible);
            Ok(vec![Value::Number(f64::from(u8::from(on)))])
        })
        .overridable("net_Spawn", move |c: &mut ClientObject, args| {
            spawn.stopped()?;
            // The engine spawns from the data of the spawning in progress;
            // `data`, its simulation object, is the scripts'. Outside a
            // spawning there is none (our choice: false).
            let _ = args;
            let token = spawn.objects.borrow().spawning(c.part);
            let Some(token) = token else {
                return Ok(vec![Value::Boolean(false)]);
            };
            let spawned = spawn.objects.borrow_mut().spawn_pending(
                token,
                ObjectClass::Named(c.class),
                &spawn.factory,
                Some(c.part),
            );
            if spawned.is_some() {
                c.object = spawned;
            }
            Ok(vec![Value::Boolean(spawned.is_some())])
        })
        .overridable("net_Export", |_: &mut ClientObject, _| Ok(Vec::new()))
        .overridable("net_Import", |_: &mut ClientObject, _| Ok(Vec::new()))
        .overridable("use", move |c: &mut ClientObject, args| {
            used.stopped()?;
            let user = match args.first() {
                Some(Value::Object(part)) => used.objects.borrow().of_handle(*part),
                _ => None,
            };
            // Before it spawns there is no callback to call (our choice).
            if let Some(object) = c.object {
                callbacks::raise_within(&used, object, &ObjectEvent::UseObject { user })?;
            }
            Ok(vec![Value::Boolean(true)])
        })
}

/// A client class (10.3): `base`, one constructor taking no arguments, no
/// members of its own; calling it constructs a new, not yet spawned,
/// engine object of the class.
pub(super) fn client_class(name: &'static str, base: &str) -> EngineClass {
    let class = EngineClass::new(name)
        .base(base)
        .constructor(
            0,
            |_| Some(0),
            move |part, _| {
                Ok(ClientObject {
                    part,
                    class: name,
                    object: None,
                })
            },
        )
        .constructible();
    if base == CLIENT_BASE {
        class
    } else {
        // `DLL_Pure`'s `_construct` for this part type.
        class.object_method("_construct", |c: &mut ClientObject, _| Ok(Some(c.part)))
    }
}

/// `hit`, `CGameObject`, the client classes and `explosive`, in
/// registration order (bases first).
pub(super) fn classes(ctx: &Context) -> Vec<EngineClass> {
    let mut out = vec![hit_class(ctx), game_object_base(ctx)];
    out.extend(client_classes().map(|name| client_class(name, CLIENT_BASE)));
    out.extend(
        SCRIPT_ZONE_CLASSES
            .into_iter()
            .map(|name| client_class(name, "DLL_Pure")),
    );
    let c = ctx.clone();
    out.push(
        EngineClass::new("explosive").method("explode", move |e: &mut Explosive, _| {
            c.objects
                .borrow_mut()
                .push(Event::Explode { object: e.object });
            Ok(Vec::new())
        }),
    );
    out
}
