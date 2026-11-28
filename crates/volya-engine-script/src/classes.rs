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

use std::any::{Any, TypeId, type_name};
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
    pub(crate) fn value(self) -> f64 {
        // Ids stay far below 2^53: exact as Lua numbers.
        self.0 as f64
    }

    /// The part whose id C passed as `value`.
    pub(crate) fn from_value(value: f64) -> Self {
        EnginePart(value as u64)
    }
}

type Part = Rc<RefCell<Box<dyn Any>>>;
type PartFn = Rc<dyn Fn(&mut dyn Any, &Objects, &[Value]) -> Result<Vec<Value>, String>>;
type GetFn = Rc<dyn Fn(&dyn Any, &Objects) -> Result<Value, String>>;
type SetFn = Rc<dyn Fn(&mut dyn Any, &Objects, &Value) -> Result<(), String>>;
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
    /// Returns an engine object ([`EngineClass::object_method`]).
    object: bool,
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
    constructible: bool,
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
    Rc::new(move |part, _, args| f(downcast_mut(part)?, args))
}

/// An object argument's engine part as a method sees it ([`Objects::get`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operand<T> {
    /// The storage of the object the method is called on (the same object,
    /// or another referring to its storage): read it from the part the
    /// method has.
    This,
    /// A copy of another object's part.
    Other(T),
}

/// What a method or property registered `…_with_objects` gets besides its
/// part: the engine parts of the objects among its arguments, and new
/// objects for its results.
pub struct Objects<'a> {
    state: &'a State,
    this: u64,
    part: &'a Part,
    /// The type of `part`, which the method has borrowed.
    type_id: TypeId,
}

impl Objects<'_> {
    /// The object the method is called on, as a result: the same Lua value.
    pub fn this(&self) -> Value {
        Value::Object(EnginePart(self.this))
    }

    /// The part of object argument `value` if it is a `T`: [`Operand::This`]
    /// when it is the storage the method has, else a copy. `None` for
    /// anything else.
    pub fn get<T: Any + Clone>(&self, value: &Value) -> Option<Operand<T>> {
        let Value::Object(EnginePart(id)) = value else {
            return None;
        };
        let part = self.state.part(*id).ok()?;
        if Rc::ptr_eq(&part, self.part) {
            return (TypeId::of::<T>() == self.type_id).then_some(Operand::This);
        }
        let part = part.try_borrow().ok()?;
        part.downcast_ref::<T>().cloned().map(Operand::Other)
    }

    /// Writes `part` into object argument `value` if its part is a `T`
    /// other than the storage the method has (a result written into an
    /// argument, as a vector a getter fills). `false` for anything else.
    pub fn set<T: Any>(&self, value: &Value, part: T) -> bool {
        let Value::Object(EnginePart(id)) = value else {
            return false;
        };
        let Ok(target) = self.state.part(*id) else {
            return false;
        };
        if Rc::ptr_eq(&target, self.part) {
            return false;
        }
        let Ok(mut target) = target.try_borrow_mut() else {
            return false;
        };
        match target.downcast_mut::<T>() {
            Some(t) => {
                *t = part;
                true
            }
            None => false,
        }
    }

    /// A new object of engine class `class` holding `part` (a type its
    /// methods take), owned by the script side and collected like a
    /// constructed one.
    pub fn new_object<T: Any>(&self, class: &str, part: T) -> Value {
        let id = self
            .state
            .insert(Rc::from(class), Rc::new(RefCell::new(Box::new(part))));
        Value::Object(EnginePart(id))
    }

    /// A new object referring to the storage of the object the method is
    /// called on, of the same engine class: changing either changes both.
    /// It keeps the storage alive (our choice: whether it does is
    /// unestablished where the specs leave it).
    pub fn reference(&self) -> Value {
        let class = self
            .state
            .parts
            .borrow()
            .get(&self.this)
            .map(|(class, _)| Rc::clone(class));
        let Some(class) = class else {
            return Value::Nil;
        };
        let id = self.state.insert(class, Rc::clone(self.part));
        Value::Object(EnginePart(id))
    }
}

/// What a function registered with [`Lua::register_with_objects`] gets:
/// new objects for its results.
pub struct ObjectMaker {
    state: Rc<State>,
}

impl ObjectMaker {
    /// A copy of the part of object argument `value` if it is a `T`;
    /// `None` for anything else.
    pub fn get<T: Any + Clone>(&self, value: &Value) -> Option<T> {
        let Value::Object(EnginePart(id)) = value else {
            return None;
        };
        let part = self.state.part(*id).ok()?;
        let part = part.try_borrow().ok()?;
        part.downcast_ref::<T>().cloned()
    }

    /// A new object of engine class `class` holding `part`, owned by the
    /// script side and collected like a constructed one
    /// ([`Objects::new_object`]).
    pub fn new_object<T: Any>(&self, class: &str, part: T) -> Value {
        let id = self
            .state
            .insert(Rc::from(class), Rc::new(RefCell::new(Box::new(part))));
        Value::Object(EnginePart(id))
    }
}

impl EngineClass {
    pub fn new(name: &str) -> Self {
        EngineClass {
            name: name.to_owned(),
            base: None,
            constructible: false,
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

    /// Scripts construct this class's objects by calling it (`Class(...)`):
    /// with the constructor taking as many arguments whose score is best,
    /// raising when none matches (our wording).
    pub fn constructible(mut self) -> Self {
        self.constructible = true;
        self
    }

    /// A method that reads the engine parts of object arguments or gives
    /// objects as results ([`Objects`]), without a separate default.
    pub fn method_with_objects<T: Any>(
        mut self,
        name: &str,
        call: impl Fn(&mut T, &Objects, &[Value]) -> Result<Vec<Value>, String> + 'static,
    ) -> Self {
        self.methods.push(Method {
            name: name.to_owned(),
            call: Rc::new(move |part, objects, args| call(downcast_mut(part)?, objects, args)),
            default: None,
            object: false,
        });
        self
    }

    /// A method that reads the engine parts of object arguments or gives
    /// objects as results ([`Objects`]), `call` being both the member and
    /// the default implementation, as for [`EngineClass::overridable`].
    pub fn overridable_with_objects<T: Any>(
        mut self,
        name: &str,
        call: impl Fn(&mut T, &Objects, &[Value]) -> Result<Vec<Value>, String> + 'static,
    ) -> Self {
        let call: PartFn =
            Rc::new(move |part, objects, args| call(downcast_mut(part)?, objects, args));
        self.methods.push(Method {
            name: name.to_owned(),
            call: Rc::clone(&call),
            default: Some(call),
            object: false,
        });
        self
    }

    /// A property whose value may be an object ([`Objects`]); scripts can
    /// assign it if `set` is given.
    #[allow(clippy::type_complexity)]
    pub fn property_with_objects<T: Any>(
        mut self,
        name: &str,
        get: impl Fn(&T, &Objects) -> Result<Value, String> + 'static,
        set: Option<Box<dyn Fn(&mut T, &Objects, &Value) -> Result<(), String>>>,
    ) -> Self {
        self.properties.push(Property {
            name: name.to_owned(),
            get: Rc::new(move |part, objects| get(downcast_ref(part)?, objects)),
            set: set.map(|set| -> SetFn {
                Rc::new(move |part, objects, value| set(downcast_mut(part)?, objects, value))
            }),
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
            object: false,
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
            object: false,
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
            object: false,
        });
        self
    }

    /// A method returning an engine object: the object holding the part
    /// `f` returns (an engine object [`Lua::new_engine_object`] made, or a
    /// script object), or nil for `None` or a part no object holds. `f` is
    /// both the member and the default implementation, as for
    /// [`EngineClass::overridable`]; [`Lua::dispatch`] gets the part's id.
    pub fn object_method<T: Any>(
        mut self,
        name: &str,
        f: impl Fn(&mut T, &[Value]) -> Result<Option<EnginePart>, String> + 'static,
    ) -> Self {
        let f = part_fn(move |part: &mut T, args| Ok(vec![object_value(f(part, args)?)]));
        self.methods.push(Method {
            name: name.to_owned(),
            call: Rc::clone(&f),
            default: Some(f),
            object: true,
        });
        self
    }

    /// A read-only property.
    pub fn property<T: Any>(mut self, name: &str, get: impl Fn(&T) -> Value + 'static) -> Self {
        self.properties.push(Property {
            name: name.to_owned(),
            get: Rc::new(move |part, _| Ok(get(downcast_ref(part)?))),
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
            get: Rc::new(move |part, _| Ok(get(downcast_ref(part)?))),
            set: Some(Rc::new(move |part, _, value| {
                set(downcast_mut(part)?, value)
            })),
        });
        self
    }

    /// An integer constant, read as `Class.name` (4.4).
    pub fn constant(mut self, name: &str, value: i32) -> Self {
        self.constants.push((name.to_owned(), value));
        self
    }
}

/// An engine part as an object result: its id, which C turns into its
/// object, or nil.
fn object_value(part: Option<EnginePart>) -> Value {
    part.map_or(Value::Nil, |p| Value::Number(p.value()))
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
    /// The id `args[0]`, as C passes it, and its part.
    fn part_of(&self, args: &[Value]) -> Result<(u64, Part), String> {
        match args.first() {
            Some(Value::Number(id)) => Ok((*id as u64, self.part(*id as u64)?)),
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

    fn new_id(&self) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        id
    }

    /// Keeps `part`, made by engine class `class`, under a new id.
    fn insert(&self, class: Rc<str>, part: Part) -> u64 {
        let id = self.new_id();
        self.parts.borrow_mut().insert(id, (class, part));
        id
    }

    /// The method's view of objects, for the part `id` borrowed as `data`.
    fn objects<'a>(&'a self, id: u64, part: &'a Part, data: &dyn Any) -> Objects<'a> {
        Objects {
            state: self,
            this: id,
            part,
            type_id: data.type_id(),
        }
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
pub(crate) const NOT_LOGGED: &str = "cannot resume dead coroutine";

impl Lua {
    /// Opens the class system, as the binding library's opening does for
    /// the game state (1.1): the global `class`. Engine classes are
    /// registered after it.
    pub fn open_classes(&mut self) -> Result<(), Error> {
        let state = Rc::clone(&self.classes);
        let release = self.keep(move |args| {
            if let Some(Value::Number(id)) = args.first() {
                let released = state.parts.borrow_mut().remove(&(*id as u64));
                drop(released);
            }
            Ok(Vec::new())
        });
        // The engine class of a part made for a new object (`Objects`).
        let state = Rc::clone(&self.classes);
        let adopt = self.keep(move |args| {
            let class = match args.first() {
                Some(Value::Number(id)) => state
                    .parts
                    .borrow()
                    .get(&(*id as u64))
                    .map(|(class, _)| Value::from(&**class)),
                _ => None,
            };
            Ok(class.into_iter().collect())
        });
        let mut hooks = ffi::ClassHooks { release, adopt };
        // SAFETY: the state is open; `hooks` outlives the call.
        let status = unsafe { ffi::volya_open_classes_protected(self.state.as_ptr(), &mut hooks) };
        self.check(status)
    }

    /// Sets the global `name` to the object factory's tools
    /// (csrc/volya_class.c; object_factory.lua takes and clears it).
    pub(crate) fn open_factory_tools(&mut self, name: &str) -> Result<(), Error> {
        let name = cstring(name)?;
        // SAFETY: the state is open; `name` outlives the call.
        let status =
            unsafe { ffi::volya_open_factory_tools_protected(self.state.as_ptr(), name.as_ptr()) };
        self.check(status)
    }

    /// Registers `class` in the globals under its name ([`Lua::open_classes`]
    /// first; a base before the classes deriving from it).
    pub fn register_class(&mut self, class: EngineClass) -> Result<(), Error> {
        let EngineClass {
            name,
            base,
            constructible,
            constructors,
            methods,
            properties,
            constants,
        } = class;
        let c_name = cstring(&name)?;
        let c_base = base.as_deref().map(cstring).transpose()?;
        let class_name: Rc<str> = Rc::from(name.as_str());

        let constructors = Rc::new(constructors);
        let state = Rc::clone(&self.classes);
        let made_by = Rc::clone(&class_name);
        let all = Rc::clone(&constructors);
        let construct = self.keep(move |args| {
            let Some(constructor) = choose(&all, args) else {
                // No error when none matches (3.1).
                return Ok(Vec::new());
            };
            let id = state.new_id();
            let part = (constructor.make)(EnginePart(id), args)?;
            state
                .parts
                .borrow_mut()
                .insert(id, (Rc::clone(&made_by), Rc::new(RefCell::new(part))));
            Ok(vec![Value::Number(EnginePart(id).value())])
        });
        let script_construct = if constructible {
            let state = Rc::clone(&self.classes);
            let made_by = Rc::clone(&class_name);
            self.keep(move |args| {
                let constructor = constructors
                    .iter()
                    .filter(|c| c.params == args.len())
                    .filter_map(|c| (c.score)(args).map(|score| (score, c)))
                    .min_by_key(|(score, _)| *score)
                    .map(|(_, c)| c)
                    .ok_or_else(|| {
                        format!("no constructor of '{made_by}' matches the arguments")
                    })?;
                let id = state.new_id();
                let part = (constructor.make)(EnginePart(id), args)?;
                state
                    .parts
                    .borrow_mut()
                    .insert(id, (Rc::clone(&made_by), Rc::new(RefCell::new(part))));
                Ok(vec![Value::Object(EnginePart(id))])
            })
        } else {
            ffi::RustFn::NONE
        };

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
                object: c_int::from(method.object),
            });
            names.push(m_name);
        }
        let mut c_properties = Vec::new();
        for property in properties {
            let p_name = cstring(&property.name)?;
            let state = Rc::clone(&self.classes);
            let get_fn = property.get;
            let get = self.keep(move |args| {
                let (id, part) = state.part_of(args)?;
                let data = part
                    .try_borrow()
                    .map_err(|_| "the engine part is in use".to_owned())?;
                let objects = state.objects(id, &part, &**data);
                Ok(vec![get_fn(&**data, &objects)?])
            });
            let set = match property.set {
                Some(set_fn) => {
                    let state = Rc::clone(&self.classes);
                    self.keep(move |args| {
                        let (id, part) = state.part_of(args)?;
                        let mut data = part
                            .try_borrow_mut()
                            .map_err(|_| "the engine part is in use".to_owned())?;
                        let objects = state.objects(id, &part, &**data);
                        set_fn(&mut **data, &objects, args.get(1).unwrap_or(&Value::Nil))?;
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
            script_construct,
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

    /// Makes an object of engine class `class` (registered) as the engine
    /// does, not a script: it holds the part `make` returns from the start
    /// and lives as long as the state. Functions and methods returning
    /// engine objects ([`Lua::register_object_function`],
    /// [`EngineClass::object_method`]) hand it to scripts.
    pub fn new_engine_object<T: Any>(
        &mut self,
        class: &str,
        make: impl FnOnce(EnginePart) -> T,
    ) -> Result<EnginePart, Error> {
        let c_class = cstring(class)?;
        let id = self.classes.next.get() + 1;
        self.classes.next.set(id);
        let part = EnginePart(id);
        let data: Box<dyn Any> = Box::new(make(part));
        self.classes
            .parts
            .borrow_mut()
            .insert(id, (Rc::from(class), Rc::new(RefCell::new(data))));
        let mut object = ffi::EngineObject {
            class: c_class.as_ptr(),
            part: part.value(),
        };
        // SAFETY: the state is open; `object` outlives the call.
        let status =
            unsafe { ffi::volya_new_engine_object_protected(self.state.as_ptr(), &mut object) };
        if let Err(e) = self.check(status) {
            self.classes.parts.borrow_mut().remove(&id);
            return Err(e);
        }
        Ok(part)
    }

    /// Releases the engine object holding `part` ([`Lua::new_engine_object`])
    /// when what it stands for is gone: the state stops keeping it and its
    /// part is released, so its members fail on values scripts still hold
    /// (our choice where the specs leave that unestablished). `false` if no
    /// engine object holds the part.
    pub fn release_engine_object(&mut self, part: EnginePart) -> Result<bool, Error> {
        if self.classes.part(part.0).is_err() {
            return Ok(false);
        }
        let mut value = part.value();
        // SAFETY: the state is open; `value` outlives the call.
        let status =
            unsafe { ffi::volya_release_engine_object_protected(self.state.as_ptr(), &mut value) };
        self.check(status)?;
        Ok(self.classes.part(part.0).is_err())
    }

    /// Sets the global `name` to a function running `f`, which returns an
    /// engine object: the object holding the part `f` returns, or nil.
    pub fn register_object_function(
        &mut self,
        name: &str,
        mut f: impl FnMut(&[Value]) -> Result<Option<EnginePart>, String> + 'static,
    ) -> Result<(), Error> {
        let c_name = cstring(name)?;
        let f = self.keep(move |args| Ok(vec![object_value(f(args)?)]));
        let mut function = ffi::ObjectFunction {
            name: c_name.as_ptr(),
            f,
        };
        // SAFETY: the state is open; `function` and the function outlive the call.
        let status = unsafe {
            ffi::volya_register_object_function_protected(self.state.as_ptr(), &mut function)
        };
        self.check(status)
    }

    /// Sets the global `name` to a function running `f`, which may return
    /// new objects of engine classes ([`ObjectMaker::new_object`]), as
    /// methods registered `…_with_objects` do.
    pub fn register_with_objects(
        &mut self,
        name: &str,
        mut f: impl FnMut(&ObjectMaker, &[Value]) -> Result<Vec<Value>, String> + 'static,
    ) -> Result<(), Error> {
        let maker = ObjectMaker {
            state: Rc::clone(&self.classes),
        };
        self.register(name, move |args| f(&maker, args))
    }

    /// A Rust function C calls with an engine part's id, then arguments.
    fn part_function(&mut self, f: PartFn) -> ffi::RustFn {
        let state = Rc::clone(&self.classes);
        self.keep(move |args| {
            let (id, part) = state.part_of(args)?;
            let mut data = part
                .try_borrow_mut()
                .map_err(|_| "the engine part is in use".to_owned())?;
            let objects = state.objects(id, &part, &**data);
            f(&mut **data, &objects, &args[1..])
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

    /// The default implementation of `method` of `part`'s engine class
    /// (6.1), without looking for an override: what the engine runs for a
    /// method it doesn't dispatch.
    pub(crate) fn run_default(
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
        let mut borrowed = data
            .try_borrow_mut()
            .map_err(|_| api_error("the engine part is in use"))?;
        let objects = self.classes.objects(part.0, &data, &**borrowed);
        let running = crate::Running::enter(self.state.as_ptr());
        let results = default(&mut **borrowed, &objects, args);
        drop(running);
        let results = results.map_err(api_error)?;
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
