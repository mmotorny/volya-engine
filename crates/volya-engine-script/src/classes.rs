// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Script classes deriving from engine classes (script API A,
//! docs/behavior/script-api-script-classes-deriving-from-engine-classes.md;
//! section numbers are its): the global `class`, construction, `super`,
//! members of script objects and class objects, base-class calls,
//! `__finalize` and operators run in C (csrc/volya_class.c). Engine classes
//! are described here ([`EngineClass`]) and registered on a state with
//! [`Lua::register_class`]; the engine calls a method a script class may
//! override with [`Lua::dispatch`]. See docs/architecture/scripts.md,
//! Script classes.

use std::any::{Any, type_name};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_int;
use std::fmt;
use std::rc::Rc;

use crate::{
    Error, ErrorKind, Lua, Results, Value, api_error, cstring, ffi, flatten, read_results,
};

/// The engine part of a script object (or of an engine class's own
/// object): what an engine constructor makes, owned by the script object
/// (our choice; the spec leaves the lifetime unestablished, 9). It is also
/// the weak reference to the script object the constructor receives (3.1),
/// which [`Lua::dispatch`] takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EnginePart(u64);

impl EnginePart {
    fn value(self) -> f64 {
        // Ids stay far below 2^53: exact as Lua numbers.
        self.0 as f64
    }
}

type Part = Rc<RefCell<Box<dyn Any>>>;
type PartFn = Rc<dyn Fn(&mut dyn Any, &[Value]) -> Result<Vec<Value>, String>>;
type GetFn = Rc<dyn Fn(&dyn Any) -> Result<Value, String>>;
type SetFn = Rc<dyn Fn(&mut dyn Any, &Value) -> Result<(), String>>;
type ScoreFn = Rc<dyn Fn(&[Value]) -> Option<u32>>;
type MakeFn = Rc<dyn Fn(EnginePart, &[Value]) -> Result<Box<dyn Any>, String>>;

struct Constructor {
    params: usize,
    score: ScoreFn,
    make: MakeFn,
}

struct Method {
    name: String,
    call: PartFn,
    default: Option<PartFn>,
}

struct Property {
    name: String,
    get: GetFn,
    set: Option<SetFn>,
}

/// An engine class as scripts see it: its constructors, methods,
/// properties (readable and writable attributes) and integer constants
/// (1.3). Operators are methods named as Lua's metamethods (`__add`, 7.2).
/// Each method has the member its objects see and a default (6.1): the
/// engine class's own implementation, which [`Lua::dispatch`] runs when a
/// script class doesn't override the method; `EngineClass.m` of a method
/// without one raises `pure virtual function called`.
///
/// Engine parts are Rust values of the types the constructors make;
/// methods and properties take the part as that type and fail for another.
pub struct EngineClass {
    name: String,
    base: Option<String>,
    constructors: Vec<Constructor>,
    methods: Vec<Method>,
    properties: Vec<Property>,
    constants: Vec<(String, i32)>,
}

fn downcast_mut<T: Any>(part: &mut dyn Any) -> Result<&mut T, String> {
    part.downcast_mut::<T>()
        .ok_or_else(|| format!("the engine part isn't a {}", type_name::<T>()))
}

fn downcast_ref<T: Any>(part: &dyn Any) -> Result<&T, String> {
    part.downcast_ref::<T>()
        .ok_or_else(|| format!("the engine part isn't a {}", type_name::<T>()))
}

fn part_fn<T: Any>(f: impl Fn(&mut T, &[Value]) -> Result<Vec<Value>, String> + 'static) -> PartFn {
    Rc::new(move |part, args| f(downcast_mut(part)?, args))
}

impl EngineClass {
    pub fn new(name: &str) -> Self {
        EngineClass {
            name: name.to_owned(),
            base: None,
            constructors: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
        }
    }

    /// Derives from engine class `name`, registered before: its methods,
    /// properties and constants are this class's too, and its methods
    /// accept this class's objects.
    pub fn base(mut self, name: &str) -> Self {
        self.base = Some(name.to_owned());
        self
    }

    /// A constructor taking `params` arguments. `score` is the argument-match
    /// score `super` compares between constructors taking as many arguments
    /// as it passes (3.1): `None` when the arguments don't match, else lower
    /// is better (the first registered wins a tie). A class's only
    /// constructor is used whatever the arguments.
    pub fn constructor<T: Any>(
        mut self,
        params: usize,
        score: impl Fn(&[Value]) -> Option<u32> + 'static,
        make: impl Fn(EnginePart, &[Value]) -> Result<T, String> + 'static,
    ) -> Self {
        self.constructors.push(Constructor {
            params,
            score: Rc::new(score),
            make: Rc::new(move |part, args| Ok(Box::new(make(part, args)?))),
        });
        self
    }

    /// A method registered without a separate default implementation.
    pub fn method<T: Any>(
        mut self,
        name: &str,
        call: impl Fn(&mut T, &[Value]) -> Result<Vec<Value>, String> + 'static,
    ) -> Self {
        self.methods.push(Method {
            name: name.to_owned(),
            call: part_fn(call),
            default: None,
        });
        self
    }

    /// A method script classes override for the engine (5.1): `f` is both
    /// the member and the default implementation.
    pub fn overridable<T: Any>(
        mut self,
        name: &str,
        f: impl Fn(&mut T, &[Value]) -> Result<Vec<Value>, String> + 'static,
    ) -> Self {
        let f = part_fn(f);
        self.methods.push(Method {
            name: name.to_owned(),
            call: Rc::clone(&f),
            default: Some(f),
        });
        self
    }

    /// A method whose member and default implementation differ (6.1).
    pub fn method_with_default<T: Any>(
        mut self,
        name: &str,
        call: impl Fn(&mut T, &[Value]) -> Result<Vec<Value>, String> + 'static,
        default: impl Fn(&mut T, &[Value]) -> Result<Vec<Value>, String> + 'static,
    ) -> Self {
        self.methods.push(Method {
            name: name.to_owned(),
            call: part_fn(call),
            default: Some(part_fn(default)),
        });
        self
    }

    /// A read-only property.
    pub fn property<T: Any>(mut self, name: &str, get: impl Fn(&T) -> Value + 'static) -> Self {
        self.properties.push(Property {
            name: name.to_owned(),
            get: Rc::new(move |part| Ok(get(downcast_ref(part)?))),
            set: None,
        });
        self
    }

    /// A property scripts can assign.
    pub fn writable_property<T: Any>(
        mut self,
        name: &str,
        get: impl Fn(&T) -> Value + 'static,
        set: impl Fn(&mut T, &Value) -> Result<(), String> + 'static,
    ) -> Self {
        self.properties.push(Property {
            name: name.to_owned(),
            get: Rc::new(move |part| Ok(get(downcast_ref(part)?))),
            set: Some(Rc::new(move |part, value| set(downcast_mut(part)?, value))),
        });
        self
    }

    /// An integer constant, read as `Class.name` (4.4).
    pub fn constant(mut self, name: &str, value: i32) -> Self {
        self.constants.push((name.to_owned(), value));
        self
    }
}

/// The constructor `super` uses for `args` (3.1): the only one, or the
/// best-scoring of those taking as many arguments; `None` when none
/// matches.
fn choose<'a>(constructors: &'a [Constructor], args: &[Value]) -> Option<&'a Constructor> {
    if let [only] = constructors {
        return Some(only);
    }
    constructors
        .iter()
        .filter(|c| c.params == args.len())
        .filter_map(|c| (c.score)(args).map(|score| (score, c)))
        .min_by_key(|(score, _)| *score)
        .map(|(_, c)| c)
}

/// A state's engine parts and the default implementations of its engine
/// classes' methods.
#[derive(Default)]
pub(crate) struct State {
    next: Cell<u64>,
    /// Each part with its engine class's name.
    parts: RefCell<HashMap<u64, (Rc<str>, Part)>>,
    /// Engine class name -> method -> default implementation.
    defaults: RefCell<HashMap<String, HashMap<String, PartFn>>>,
}

impl State {
    /// The part whose id is `args[0]`, as C passes it.
    fn part_of(&self, args: &[Value]) -> Result<Part, String> {
        match args.first() {
            Some(Value::Number(id)) => self.part(*id as u64),
            _ => Err("engine part expected".into()),
        }
    }

    fn part(&self, id: u64) -> Result<Part, String> {
        self.parts
            .borrow()
            .get(&id)
            .map(|(_, part)| Rc::clone(part))
            .ok_or_else(|| "the engine part was released".into())
    }
}

/// What the engine makes of an override's results (5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conversion {
    /// A call without a result: [`Value::Nil`].
    Nothing,
    /// The first result (or nil), as it is.
    Value,
    /// The first result by Lua's number conversion (a non-number gives 0),
    /// rounded to a 32-bit integer.
    Integer,
}

/// An engine call into an override that failed (5.3): the engine's error
/// handler writes `log` to the log, then stops the game with `error`
/// (`LUA error: <message>`, [`ErrorKind::Fatal`]). A failure of the call
/// itself (no such part, no default implementation) has no log line and
/// [`ErrorKind::Api`].
#[derive(Clone, Debug, PartialEq)]
pub struct OverrideFailure {
    pub log: Option<String>,
    pub error: Error,
}

impl fmt::Display for OverrideFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for OverrideFailure {}

impl From<Error> for OverrideFailure {
    fn from(error: Error) -> Self {
        OverrideFailure { log: None, error }
    }
}

/// The message the engine's error handler doesn't log (5.3).
const NOT_LOGGED: &str = "cannot resume dead coroutine";

impl Lua {
    /// Opens the class system, as the binding library's opening does for
    /// the game state (1.1): the global `class`. Engine classes are
    /// registered after it.
    pub fn open_classes(&mut self) -> Result<(), Error> {
        let state = Rc::clone(&self.classes);
        let mut release = self.keep(move |args| {
            if let Some(Value::Number(id)) = args.first() {
                let released = state.parts.borrow_mut().remove(&(*id as u64));
                drop(released);
            }
            Ok(Vec::new())
        });
        // SAFETY: the state is open; `release` outlives the call.
        let status =
            unsafe { ffi::volya_open_classes_protected(self.state.as_ptr(), &mut release) };
        self.check(status)
    }

    /// Registers `class` in the globals under its name ([`Lua::open_classes`]
    /// first; a base before the classes deriving from it).
    pub fn register_class(&mut self, class: EngineClass) -> Result<(), Error> {
        let EngineClass {
            name,
            base,
            constructors,
            methods,
            properties,
            constants,
        } = class;
        let c_name = cstring(&name)?;
        let c_base = base.as_deref().map(cstring).transpose()?;
        let class_name: Rc<str> = Rc::from(name.as_str());

        let state = Rc::clone(&self.classes);
        let made_by = Rc::clone(&class_name);
        let construct = self.keep(move |args| {
            let Some(constructor) = choose(&constructors, args) else {
                // No error when none matches (3.1).
                return Ok(Vec::new());
            };
            let id = state.next.get() + 1;
            state.next.set(id);
            let part = (constructor.make)(EnginePart(id), args)?;
            state
                .parts
                .borrow_mut()
                .insert(id, (Rc::clone(&made_by), Rc::new(RefCell::new(part))));
            Ok(vec![Value::Number(EnginePart(id).value())])
        });

        let mut names = Vec::new();
        let mut c_methods = Vec::new();
        let mut defaults = match &base {
            Some(base) => self
                .classes
                .defaults
                .borrow()
                .get(base)
                .cloned()
                .unwrap_or_default(),
            None => HashMap::new(),
        };
        for method in methods {
            let m_name = cstring(&method.name)?;
            let call = self.part_function(method.call.clone());
            let default = match &method.default {
                Some(f) => {
                    defaults.insert(method.name.clone(), Rc::clone(f));
                    self.part_function(Rc::clone(f))
                }
                None => {
                    defaults.remove(&method.name);
                    ffi::RustFn::NONE
                }
            };
            c_methods.push(ffi::EngineMethod {
                name: m_name.as_ptr(),
                call,
                default,
            });
            names.push(m_name);
        }
        let mut c_properties = Vec::new();
        for property in properties {
            let p_name = cstring(&property.name)?;
            let state = Rc::clone(&self.classes);
            let get_fn = property.get;
            let get = self.keep(move |args| {
                let part = state.part_of(args)?;
                let part = part
                    .try_borrow()
                    .map_err(|_| "the engine part is in use".to_owned())?;
                Ok(vec![get_fn(&**part)?])
            });
            let set = match property.set {
                Some(set_fn) => {
                    let state = Rc::clone(&self.classes);
                    self.keep(move |args| {
                        let part = state.part_of(args)?;
                        let mut part = part
                            .try_borrow_mut()
                            .map_err(|_| "the engine part is in use".to_owned())?;
                        set_fn(&mut **part, args.get(1).unwrap_or(&Value::Nil))?;
                        Ok(Vec::new())
                    })
                }
                None => ffi::RustFn::NONE,
            };
            c_properties.push(ffi::EngineProperty {
                name: p_name.as_ptr(),
                get,
                set,
            });
            names.push(p_name);
        }
        let mut c_constants = Vec::new();
        for (c_name, value) in &constants {
            let k = cstring(c_name)?;
            c_constants.push(ffi::EngineConstant {
                name: k.as_ptr(),
                value: c_int::from(*value),
            });
            names.push(k);
        }
        let mut spec = ffi::EngineClass {
            name: c_name.as_ptr(),
            base: c_base.as_ref().map_or(std::ptr::null(), |b| b.as_ptr()),
            construct,
            methods: c_methods.as_ptr(),
            nmethods: c_methods.len(),
            properties: c_properties.as_ptr(),
            nproperties: c_properties.len(),
            constants: c_constants.as_ptr(),
            nconstants: c_constants.len(),
        };
        // SAFETY: the state is open; `spec` and the names it points to
        // outlive the call.
        let status = unsafe { ffi::volya_register_class_protected(self.state.as_ptr(), &mut spec) };
        drop(names);
        self.check(status)?;
        self.classes.defaults.borrow_mut().insert(name, defaults);
        Ok(())
    }

    /// A Rust function C calls with an engine part's id, then arguments.
    fn part_function(&mut self, f: PartFn) -> ffi::RustFn {
        let state = Rc::clone(&self.classes);
        self.keep(move |args| {
            let part = state.part_of(args)?;
            let mut part = part
                .try_borrow_mut()
                .map_err(|_| "the engine part is in use".to_owned())?;
            f(&mut **part, &args[1..])
        })
    }

    /// The engine calls method `method` on `part` (5.2): the override the
    /// script object's class defines, or the engine class's default
    /// implementation when it defines none or the script object is gone.
    /// Returns the result as `conversion` makes it (5.3).
    pub fn dispatch(
        &mut self,
        part: EnginePart,
        method: &str,
        args: &[Value],
        conversion: Conversion,
    ) -> Result<Value, OverrideFailure> {
        let c_method = cstring(method)?;
        let mut flat = Vec::new();
        for arg in args {
            flatten(arg, &mut flat)?;
        }
        let mut results = Results::default();
        let mut d = ffi::Dispatch {
            part: part.value(),
            method: c_method.as_ptr(),
            args: flat.as_ptr(),
            nargs: args.len(),
            conversion: match conversion {
                Conversion::Nothing => ffi::VOLYA_NOTHING,
                Conversion::Value => ffi::VOLYA_VALUE,
                Conversion::Integer => ffi::VOLYA_INTEGER,
            },
            outcome: ffi::VOLYA_CALLED,
            read: read_results,
            ctx: (&mut results as *mut Results).cast(),
        };
        // SAFETY: the state is open; everything `d` points to outlives the call.
        let status = unsafe { ffi::volya_dispatch_protected(self.state.as_ptr(), &mut d) };
        self.check(status)?;
        if let Some(message) = results.error {
            return Err(api_error(message).into());
        }
        let first = results.values.into_iter().next().unwrap_or(Value::Nil);
        match d.outcome {
            ffi::VOLYA_CALLED => Ok(first),
            ffi::VOLYA_FAILED => {
                let message = match first {
                    Value::String(s) => String::from_utf8_lossy(&s).into_owned(),
                    v => format!("{v:?}"),
                };
                Err(OverrideFailure {
                    log: (message != NOT_LOGGED).then(|| message.clone()),
                    error: Error {
                        kind: ErrorKind::Fatal,
                        message: format!("LUA error: {message}"),
                    },
                })
            }
            ffi::VOLYA_NO_OBJECT | ffi::VOLYA_NOT_OVERRIDDEN => {
                self.run_default(part, method, args, conversion)
            }
            outcome => Err(api_error(format!("unknown engine call outcome {outcome}")).into()),
        }
    }

    /// The default implementation of `method` of `part`'s engine class.
    fn run_default(
        &mut self,
        part: EnginePart,
        method: &str,
        args: &[Value],
        conversion: Conversion,
    ) -> Result<Value, OverrideFailure> {
        let (class, data) = self
            .classes
            .parts
            .borrow()
            .get(&part.0)
            .map(|(class, data)| (Rc::clone(class), Rc::clone(data)))
            .ok_or_else(|| api_error("the engine part was released"))?;
        let default = self
            .classes
            .defaults
            .borrow()
            .get(&*class)
            .and_then(|methods| methods.get(method))
            .cloned()
            .ok_or_else(|| api_error(format!("{class}.{method} has no default implementation")))?;
        let mut data = data
            .try_borrow_mut()
            .map_err(|_| api_error("the engine part is in use"))?;
        let results = default(&mut **data, args).map_err(api_error)?;
        Ok(match conversion {
            Conversion::Nothing => Value::Nil,
            _ => results.into_iter().next().unwrap_or(Value::Nil),
        })
    }

    /// Runs `f` on `part` if it exists and is a `T`.
    pub fn with_part<T: Any, R>(&self, part: EnginePart, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let data = self.classes.part(part.0).ok()?;
        let mut data = data.try_borrow_mut().ok()?;
        data.downcast_mut::<T>().map(f)
    }

    /// The engine parts alive.
    pub fn engine_parts(&self) -> usize {
        self.classes.parts.borrow().len()
    }
}
