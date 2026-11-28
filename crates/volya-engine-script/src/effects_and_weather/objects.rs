// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The classes `color_animator` (script API U 8), `particles_object` (9,
//! #1196's) and `particle_params` (10).

use std::rc::Rc;

use volya_engine_formats::cp1251;
use volya_engine_formats::lanims::{ColourAnimations, Entry};

use super::{Context, PARTICLE_OBJECTS_ISSUE, not_implemented, number, text};
use crate::Value;
use crate::classes::{EngineClass, ObjectMaker};
use crate::values::{Vector, fcolor_of, to_f32};

/// The colour animation library's path under `$game_data$` (CA 1).
pub const LIBRARY: &str = "lanims.xr";

/// The members of `particles_object` (9; 1.1).
pub const PARTICLES_OBJECT_MEMBERS: [&str; 11] = [
    "play",
    "play_at_pos",
    "stop",
    "stop_deffered",
    "playing",
    "looped",
    "move_to",
    "load_path",
    "start_path",
    "stop_path",
    "pause_path",
];

/// A `color_animator`'s engine part (8): its entry.
#[derive(Clone)]
struct ColorAnimator(Entry);

/// A `particles_object`'s engine part, never made (#1196).
struct ParticlesObject;

/// A `particle_params` record (10): three vectors.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticleParams {
    pub a: Vector,
    pub b: Vector,
    pub c: Vector,
}

impl Context {
    /// The colour animation library, read at first use (CA 1: without the
    /// file it is empty and nothing reports it); a file that cannot be
    /// parsed is fatal (our choice).
    fn animations(&self, function: &str) -> Result<Rc<ColourAnimations>, String> {
        if let Some(library) = &self.state.borrow().animations {
            return Ok(Rc::clone(library));
        }
        let read = {
            let files = self.files.borrow();
            let path = files.aliases.path("$game_data$", LIBRARY);
            path.ok().and_then(|path| files.index.read(&path).ok())
        };
        let library = match read {
            None => ColourAnimations::default(),
            Some(bytes) => ColourAnimations::parse(&bytes).map_err(|e| {
                self.fatal(
                    function,
                    line!(),
                    "fatal error",
                    "<no expression>",
                    &[&format!("{LIBRARY}: {e}")],
                )
            })?,
        };
        let library = Rc::new(library);
        self.state.borrow_mut().animations = Some(Rc::clone(&library));
        Ok(library)
    }

    /// The entry `name` (CA 2.5); a failed assertion `Can't find color
    /// anim:` with the name when there is none (8).
    fn colour_animation(&self, function: &str, name: &[u8]) -> Result<Entry, String> {
        let library = self.animations(function)?;
        match library.find(name) {
            Some(i) => Ok(library.entries[i].clone()),
            None => Err(self.fatal(
                function,
                line!(),
                "assertion failed",
                "Can't find color anim:",
                &[&cp1251::decode(name)],
            )),
        }
    }
}

/// `color_animator` (8).
fn color_animator_class(ctx: &Context) -> EngineClass {
    let make = ctx.clone();
    let load = ctx.clone();
    EngineClass::new("color_animator")
        .constructor(
            1,
            |args: &[Value]| matches!(args, [Value::String(_)]).then_some(0),
            move |_, args: &[Value]| -> Result<ColorAnimator, String> {
                make.stopped()?;
                let name = text(args, 0, "color_animator")?;
                Ok(ColorAnimator(
                    make.colour_animation("color_animator", &name)?,
                ))
            },
        )
        .constructible()
        .method("load", move |a: &mut ColorAnimator, args| {
            load.stopped()?;
            let name = text(args, 0, "color_animator:load")?;
            a.0 = load.colour_animation("color_animator:load", &name)?;
            Ok(Vec::new())
        })
        // CA 3 at t seconds, each channel byte × 1/255 as E's
        // `fcolor:set(number)` makes it.
        .method_with_objects("calculate", |a: &mut ColorAnimator, o, args| {
            let t = to_f32(number(args, 0, "color_animator:calculate")?);
            let (colour, _) = a.0.colour_at(t);
            Ok(vec![o.new_object("fcolor", fcolor_of(f64::from(colour)))])
        })
        // N / fps × 1000 in single precision, the fraction cut.
        .method("length", |a: &mut ColorAnimator, _| {
            let ms = a.0.length() * 1000.0;
            Ok(vec![Value::Number(f64::from(ms as i32))])
        })
}

/// `particles_object` (9): its constructor of a text and its members, each
/// raising #1196's error.
fn particles_object_class() -> EngineClass {
    let class = EngineClass::new("particles_object")
        .constructor(
            1,
            |args: &[Value]| matches!(args, [Value::String(_)]).then_some(0),
            |_, _| -> Result<ParticlesObject, String> {
                Err(not_implemented(
                    "particles_object",
                    "particle objects",
                    PARTICLE_OBJECTS_ISSUE,
                ))
            },
        )
        .constructible();
    PARTICLES_OBJECT_MEMBERS.iter().fold(class, |class, &name| {
        class.method(name, move |_: &mut ParticlesObject, _| {
            Err(not_implemented(
                &format!("particles_object:{name}"),
                "particle objects",
                PARTICLE_OBJECTS_ISSUE,
            ))
        })
    })
}

/// `particle_params` (10): four constructors of 0 to 3 vectors, the rest
/// (0, 0, 0); no member.
fn particle_params_class() -> EngineClass {
    let vectors = |maker: &ObjectMaker, args: &[Value]| -> Option<Vec<Vector>> {
        args.iter().map(|v| maker.get::<Vector>(v)).collect()
    };
    (0..=3)
        .fold(EngineClass::new("particle_params"), |class, n| {
            class.constructor_with_objects(
                n,
                move |maker, args| vectors(maker, args).map(|_| 0),
                move |_, maker, args| -> Result<ParticleParams, String> {
                    let v = vectors(maker, args)
                        .ok_or_else(|| "particle_params: vector expected".to_owned())?;
                    let at = |i: usize| v.get(i).copied().unwrap_or_default();
                    Ok(ParticleParams {
                        a: at(0),
                        b: at(1),
                        c: at(2),
                    })
                },
            )
        })
        .constructible()
}

/// The classes of 8, 9 and 10.
pub(super) fn classes(ctx: &Context) -> [EngineClass; 3] {
    [
        color_animator_class(ctx),
        particles_object_class(),
        particle_params_class(),
    ]
}
