// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The weather's `level` functions and the two environment classes
//! (script API U 7): the names checked as 7.1 and 7.2 state, switching to
//! them #1195's; the current name, whether an effect plays, the rain
//! density and the environment's blended values as the level last blended
//! them ([`super::Environment`]).

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_formats::cp1251;

use super::{
    Context, EffectsAndWeather, Results, WEATHER_SWITCHING_ISSUE, boolean, forms, not_implemented,
    text,
};
use crate::classes::{EngineClass, EnginePart};
use crate::loader::Scripts;
use crate::{Error, Value};

/// The environment class's registered name (7.6): the spec does not give
/// it, so ours.
pub const ENVIRONMENT_CLASS: &str = "environment";
/// The descriptor class's registered name (7.6): ours, as for
/// [`ENVIRONMENT_CLASS`].
pub const DESCRIPTOR_CLASS: &str = "environment_descriptor";

/// The environment object's engine part: the environment, and its
/// descriptor object.
struct EnvironmentPart {
    descriptor: EnginePart,
}

/// The descriptor object's engine part: it refers to the environment's own
/// values (believed, 7.6).
struct Descriptor(Rc<RefCell<EffectsAndWeather>>);

/// The weather functions of the hidden Rust function (7.1 – 7.5).
pub(super) fn raw(ctx: &Context, op: &str, args: &[Value]) -> Results {
    let what = format!("level.{op}");
    let what = what.as_str();
    let system = Rc::clone(&ctx.files.borrow().system);
    match op {
        // 7.1.
        "set_weather" => {
            forms(args, &[2], what)?;
            let name = text(args, 0, what)?;
            boolean(args, 1, what)?;
            if name.is_empty() {
                return Err(ctx.fatal(
                    what,
                    line!(),
                    "fatal error",
                    "<no expression>",
                    &["! Empty weather name"],
                ));
            }
            let name = cp1251::decode(&name);
            if system.get("weathers", &name).is_none() {
                let line = format!("! Invalid weather name: {name}");
                ctx.services.borrow_mut().console.log_mut().write(&line);
                return Ok(Vec::new());
            }
            Err(not_implemented(
                what,
                "weather changes",
                WEATHER_SWITCHING_ISSUE,
            ))
        }
        // 7.2.
        "set_weather_fx" => {
            forms(args, &[1], what)?;
            let name = text(args, 0, what)?;
            if ctx.state.borrow().environment.effect_playing {
                return Ok(vec![Value::Boolean(false)]);
            }
            if name.is_empty() {
                return Err(ctx.fatal(
                    what,
                    line!(),
                    "fatal error",
                    "<no expression>",
                    &["! Empty weather effect name"],
                ));
            }
            if system
                .get("weather_effects", &cp1251::decode(&name))
                .is_none()
            {
                return Err(ctx.fatal(
                    what,
                    line!(),
                    "assertion failed",
                    "Invalid weather effect name.",
                    &[],
                ));
            }
            Err(not_implemented(
                what,
                "weather effects",
                WEATHER_SWITCHING_ISSUE,
            ))
        }
        // 7.4.
        "get_weather" => {
            let name = ctx.state.borrow().environment.name.clone();
            Ok(vec![name.map_or(Value::Nil, Value::String)])
        }
        "is_wfx_playing" => Ok(vec![Value::Boolean(
            ctx.state.borrow().environment.effect_playing,
        )]),
        // 7.5.
        "rain_factor" => Ok(vec![Value::Number(f64::from(
            ctx.state.borrow().environment.rain_density,
        ))]),
        _ => Err(format!("unknown operation {op:?}")),
    }
}

/// The two environment classes, the environment object and
/// `__volya_environment` for `level.environment` (7.6): it works whenever
/// the program runs, returning the one environment object.
pub(super) fn register(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    let property = |name: &str, get: fn(&super::Environment) -> f32| {
        let name = name.to_owned();
        move |class: EngineClass| {
            class.property(&name, move |d: &Descriptor| {
                Value::Number(f64::from(get(&d.0.borrow().environment)))
            })
        }
    };
    let descriptor = EngineClass::new(DESCRIPTOR_CLASS);
    let descriptor = property("far_plane", |e| e.far_plane)(descriptor);
    let descriptor = property("fog_density", |e| e.fog_density)(descriptor);
    scripts.register_class(descriptor)?;
    scripts.register_class(
        EngineClass::new(ENVIRONMENT_CLASS)
            .object_method("current", |e: &mut EnvironmentPart, _| {
                Ok(Some(e.descriptor))
            }),
    )?;
    let state = Rc::clone(&ctx.state);
    let lua = scripts.lua_mut();
    let descriptor = lua.new_engine_object(DESCRIPTOR_CLASS, |_| Descriptor(state))?;
    let environment =
        lua.new_engine_object(ENVIRONMENT_CLASS, |_| EnvironmentPart { descriptor })?;
    lua.register_object_function("__volya_environment", move |_| Ok(Some(environment)))
}
