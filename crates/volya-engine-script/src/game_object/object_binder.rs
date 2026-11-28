// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Object binders (script API L, docs/behavior/script-api-object-binders.md;
//! section numbers are its): the engine class `object_binder` (2), the
//! binding of `script_binding` (3), the game-object members `bind_object`
//! and `binded_object` (4, with object_binder.lua) and the engine's calls
//! of an object's attached binder over its life (5, 6):
//! [`Scripts::spawn`] and [`Scripts::spawn_saved`] bind and spawn it,
//! [`Scripts::scheduled_update`] updates it, [`Scripts::save_relevant`]
//! and [`Scripts::save_binder`] save it, [`Scripts::release_destroyed`]
//! notifies and destroys the queued objects and [`Scripts::destroy_object`]
//! destroys one.
//!
//! The binders scripts hand to `bind_object` become the engine's: the Lua
//! side keeps them from the collector until the engine deletes them (5.6).

use volya_engine_formats::cp1251;

use super::members::Handle;
use super::{Context, ObjectClass};
use crate::classes::{
    Conversion, EngineClass, EnginePart, NOT_LOGGED, ObjectMaker, OverrideFailure,
};
use crate::loader::Scripts;
use crate::{Error, ErrorKind, Value};

/// The engine class (1).
pub const OBJECT_BINDER: &str = "object_binder";

/// The configuration key naming an object's binding function (3).
pub const SCRIPT_BINDING: &str = "script_binding";

/// The registry key of the Lua side's entry function (object_binder.lua).
const ENTRY: &str = "volya.object_binder";

/// An `object_binder`'s engine part (2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectBinder {
    /// Its own part: the binder's, once attached.
    pub part: EnginePart,
    /// The game object it was constructed with, its `object`.
    pub object: Option<EnginePart>,
}

/// The engine's own implementations of the 11 methods (2.2): nothing,
/// except `net_spawn`, true, and `net_save_relevant`, false.
const METHODS: [(&str, Option<bool>); 11] = [
    ("save", None),
    ("update", None),
    ("reload", None),
    ("net_export", None),
    ("net_save_relevant", Some(false)),
    ("load", None),
    ("net_destroy", None),
    ("reinit", None),
    ("net_Relcase", None),
    ("net_spawn", Some(true)),
    ("net_import", None),
];

/// `object_binder` (2): one constructor, the read-only `object` and the 11
/// methods with their defaults.
fn binder_class() -> EngineClass {
    let class = EngineClass::new(OBJECT_BINDER)
        .constructor(
            1,
            |_| Some(0),
            |part, args| match args.first() {
                // Our choice where the spec has the outcome unestablished
                // (2.1): an object is kept as it is, nil as none, anything
                // else raises (our wording).
                Some(Value::Object(object)) => Ok(ObjectBinder {
                    part,
                    object: Some(*object),
                }),
                None | Some(Value::Nil) => Ok(ObjectBinder { part, object: None }),
                Some(_) => Err("object_binder: a game object expected".into()),
            },
        )
        .constructible()
        .property("object", |b: &ObjectBinder| {
            b.object.map_or(Value::Nil, Value::Object)
        });
    METHODS.into_iter().fold(class, |class, (name, result)| {
        class.overridable(name, move |_: &mut ObjectBinder, _| {
            Ok(result.map(Value::Boolean).into_iter().collect())
        })
    })
}

/// How a client class answers whether its object is save-relevant (5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveRelevance {
    /// Always, without asking the binder.
    Always,
    /// The binder's `net_save_relevant()`; false without a binder.
    Binder,
    /// The binder's answer, or true when the object has a further part
    /// (believed its physics shell).
    BinderOrFurtherPart,
}

impl SaveRelevance {
    /// The rule of `class` (5.3): always for the creatures, the player,
    /// `CCar` and `CPhysicObject`; the binder's answer or a further part
    /// for `CHelicopter`; else the binder's, as for `CGameObject`,
    /// `CSpaceRestrictor` and the anomalous zones. Item classes and
    /// `ce_script_zone`, `ce_smart_zone` are outside the classes examined
    /// (unestablished): our choice, `CGameObject`'s rule. The engine
    /// passes the further part of the second combining class (believed the
    /// hanging lamp's, spec Y's) itself.
    pub fn of(class: ObjectClass) -> Self {
        match class {
            ObjectClass::Player => SaveRelevance::Always,
            ObjectClass::Named(name)
                if super::CREATURE_CLASSES.contains(&name)
                    || name == "CCar"
                    || name == "CPhysicObject" =>
            {
                SaveRelevance::Always
            }
            ObjectClass::Named("CHelicopter") => SaveRelevance::BinderOrFurtherPart,
            ObjectClass::Named(_) => SaveRelevance::Binder,
        }
    }
}

/// The interval `update(delta)` gets (5.2): the time since the object's
/// previous scheduled update as the scheduler measures it (believed ms),
/// at least 1 and at most the larger of 1000 and the object's maximum
/// scheduling interval.
pub fn update_delta(elapsed: u32, max_interval: u32) -> u32 {
    elapsed.clamp(1, max_interval.max(1000))
}

/// Lua's truth of a kept result (2.3).
fn truth(value: &Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => cp1251::decode(s),
        v => format!("{v:?}"),
    }
}

/// Registers `object_binder` and the Lua side of `bind_object` and
/// `binded_object` (4), after `game_object`.
pub(super) fn install(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    scripts.register_class(binder_class())?;
    let lua = scripts.lua_mut();
    let c = ctx.clone();
    lua.register_with_objects("__volya_binder_attach", move |maker: &ObjectMaker, args| {
        c.stopped()?;
        let id = handle_of(maker, args.first(), "bind_object")?;
        let binder = args
            .get(1)
            .and_then(|v| maker.get::<ObjectBinder>(v))
            // Our wording (depends on A).
            .ok_or_else(|| "bind_object: object_binder expected".to_owned())?;
        // 4: attached only when `IsGameTypeSingle()`; an earlier binder
        // is replaced without a call. Either way the engine owns it now
        // (in another game type it is believed never deleted).
        if c.services.borrow().game_type.is_single()
            && let Some(object) = c.objects.borrow_mut().get_mut(id)
        {
            object.binder = Some(binder.part);
        }
        Ok(vec![Value::Number(binder.part.value())])
    })?;
    let c = ctx.clone();
    lua.register_with_objects(
        "__volya_binder_attached",
        move |maker: &ObjectMaker, args| {
            c.stopped()?;
            let id = handle_of(maker, args.first(), "binded_object")?;
            let binder = c.objects.borrow().get(id).and_then(|o| o.binder);
            Ok(vec![binder.map_or(Value::Nil, Value::Object)])
        },
    )?;
    lua.register_kept_factory("__volya_binder_kept")?;
    lua.exec_kept("=object_binder", include_bytes!("object_binder.lua"), ENTRY)
}

/// The object id of handle `value` (our wording for anything else).
fn handle_of(maker: &ObjectMaker, value: Option<&Value>, name: &str) -> Result<u16, String> {
    value
        .and_then(|v| maker.get::<Handle>(v))
        .map(|h| h.id)
        .ok_or_else(|| format!("{name}: game object expected"))
}

impl Scripts {
    /// Object `id`'s attached binder, if any (4).
    pub fn attached_binder(&self, id: u16) -> Option<EnginePart> {
        self.game_object_state
            .as_ref()?
            .objects
            .borrow()
            .get(id)
            .and_then(|o| o.binder)
    }

    /// The engine's call of method `method` on object `id`'s attached
    /// binder (2.3): `None` without one.
    fn call_binder(
        &mut self,
        id: u16,
        method: &str,
        args: &[Value],
        conversion: Conversion,
    ) -> Result<Option<Value>, OverrideFailure> {
        match self.attached_binder(id) {
            Some(binder) => self.dispatch(binder, method, args, conversion).map(Some),
            None => Ok(None),
        }
    }

    /// The binding (3.1): the function the object's section names under
    /// `script_binding` (inherited keys count), called with its handle.
    /// A section without the key binds nothing; a name that finds no
    /// function goes to the message routine, which does nothing; a script
    /// error is fatal (`LUA error: <text>`, D 1.2) and stops the state.
    fn bind(&mut self, id: u16) -> Result<(), OverrideFailure> {
        let registered = self.registered()?;
        let objects = std::rc::Rc::clone(&registered.objects);
        let files = std::rc::Rc::clone(&registered.files);
        let (section, handle) = {
            let o = objects.borrow();
            let Some(object) = o.get(id) else {
                return Ok(());
            };
            (object.section.clone().unwrap_or_default(), object.handle)
        };
        let name = {
            let files = files.borrow();
            match files.system.get(&cp1251::decode(&section), SCRIPT_BINDING) {
                None => return Ok(()),
                Some(value) => value.unwrap_or("").trim().to_owned(),
            }
        };
        let name = cp1251::encode(&name).unwrap_or_else(|| name.as_bytes().to_vec());
        let handle = handle.map_or(Value::Nil, Value::Object);
        let results = self
            .guarded(|lua| lua.call_kept(ENTRY, &["bind".into(), Value::String(name), handle]))?;
        match results.as_slice() {
            [Value::String(o), message, ..] if o == b"error" => {
                let message = text_of(message);
                let error = format!("LUA error: {message}");
                *self.fatal.borrow_mut() = Some(error.clone());
                Err(OverrideFailure {
                    log: (message != NOT_LOGGED).then_some(message),
                    error: Error {
                        kind: ErrorKind::Fatal,
                        message: error,
                    },
                })
            }
            _ => Ok(()),
        }
    }

    /// The binder's part of spawning object `id` (5.1): the binding, then
    /// the attached binder's `reload(section)`, `reinit()`, `load(reader)`
    /// when the spawn data carries a client-data block, and last
    /// `net_spawn(data)`, whose result is returned (true without a
    /// binder). The engine's own steps between them are its own.
    pub(super) fn spawn_binder(
        &mut self,
        id: u16,
        data: Value,
        reader: Option<Value>,
    ) -> Result<bool, OverrideFailure> {
        self.bind(id)?;
        let section = self
            .game_objects()?
            .borrow()
            .get(id)
            .and_then(|o| o.section.clone())
            .unwrap_or_default();
        self.call_binder(id, "reload", &[Value::String(section)], Conversion::Nothing)?;
        self.call_binder(id, "reinit", &[], Conversion::Nothing)?;
        if let Some(reader) = reader {
            self.call_binder(id, "load", &[reader], Conversion::Nothing)?;
        }
        Ok(self
            .call_binder(id, "net_spawn", &[data], Conversion::Value)?
            .is_none_or(|v| truth(&v)))
    }

    /// Object `id`'s scheduled update (5.2), after the engine's own: its
    /// binder's `update(delta)` ([`update_delta`]).
    pub fn scheduled_update(&mut self, id: u16, delta: u32) -> Result<(), OverrideFailure> {
        self.call_binder(
            id,
            "update",
            &[Value::Number(f64::from(delta))],
            Conversion::Nothing,
        )
        .map(drop)
    }

    /// Whether object `id`'s client-data block is written when the game
    /// collects the level's (5.3): never for an object marked for
    /// destruction, else by its class's [`SaveRelevance`], asking the
    /// binder's `net_save_relevant()` where the rule does. `further_part`
    /// is the combining classes' further part.
    pub fn save_relevant(&mut self, id: u16, further_part: bool) -> Result<bool, OverrideFailure> {
        let class = {
            let objects = self.game_objects()?;
            let o = objects.borrow();
            match o.get(id) {
                Some(object) if !object.destroying => object.class,
                _ => return Ok(false),
            }
        };
        let ask = |s: &mut Self| -> Result<bool, OverrideFailure> {
            Ok(
                s.call_binder(id, "net_save_relevant", &[], Conversion::Value)?
                    .is_some_and(|v| truth(&v)),
            )
        };
        match SaveRelevance::of(class) {
            SaveRelevance::Always => Ok(true),
            SaveRelevance::Binder => ask(self),
            SaveRelevance::BinderOrFurtherPart => Ok(ask(self)? || further_part),
        }
    }

    /// The binder's part of object `id`'s client-data block (5.3), after
    /// the object's own state: `save(packet)`.
    pub fn save_binder(&mut self, id: u16, packet: Value) -> Result<(), OverrideFailure> {
        self.call_binder(id, "save", &[packet], Conversion::Nothing)
            .map(drop)
    }

    /// The release of the objects queued for destruction after the
    /// frame's update (5.5): every object, for each queued object from the
    /// last queued to the first, gets the release notice, its binder's
    /// `net_Relcase(handle)` (the queued objects are still listed, so they
    /// get it for themselves too); then each queued object is destroyed
    /// ([`Scripts::destroy_object`]) in queue order. The objects go in id
    /// order (our choice: the order of the level's two lists isn't
    /// stated), each taking every queued object in turn (our reading).
    pub fn release_destroyed(&mut self, queue: &[u16]) -> Result<(), OverrideFailure> {
        let objects = self.game_objects()?;
        let listed: Vec<u16> = objects.borrow().objects.keys().copied().collect();
        for id in listed {
            for queued in queue.iter().rev() {
                let handle = objects.borrow().handle_value(Some(*queued));
                self.call_binder(id, "net_Relcase", &[handle], Conversion::Nothing)?;
            }
        }
        for queued in queue {
            self.destroy_object(*queued)?;
        }
        Ok(())
    }

    /// The binder's part of destroying object `id` (5.6): its
    /// `net_destroy()`, then the engine deletes and detaches it. Deleting
    /// ends the engine's ownership (TODO(#1019): gap 3, the simplest
    /// reading: the script object then lives while scripts keep it, and is
    /// collected as any other, A 7.1).
    pub(super) fn destroy_binder(&mut self, id: u16) -> Result<(), OverrideFailure> {
        let Some(binder) = self.attached_binder(id) else {
            return Ok(());
        };
        self.dispatch(binder, "net_destroy", &[], Conversion::Nothing)?;
        if let Some(object) = self.game_objects()?.borrow_mut().get_mut(id) {
            object.binder = None;
        }
        self.guarded(|lua| lua.call_kept(ENTRY, &["delete".into(), binder.value().into()]))?;
        Ok(())
    }
}
