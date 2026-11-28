// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The script runtime: the public LuaJIT 1.0.3 (Lua 5.1 alpha) interpreter
//! the original embeds (docs/behavior/script-environment.md), vendored
//! unmodified in `vendor/luajit-1.0.3` (build.rs patches a copy, adding the
//! original's C-style comments) and run without its machine-code compiler,
//! behind a thin safe API; [`loader`] loads the game's script files,
//! [`classes`] gives scripts classes deriving from engine classes,
//! [`object_factory`] keeps the class ids and creates objects by them, and
//! [`services`], [`values`] and [`files`] register the engine's names of
//! the script API. See docs/architecture/scripts.md.
//!
//! Lua raises errors with longjmp, which must never cross Rust frames: every
//! Lua call that can raise runs in C inside `lua_cpcall` (csrc/volya_glue.c),
//! and errors come back as [`Error`]s. Rust functions registered with
//! [`Lua::register`] run with their arguments already read and return their
//! results or error to C, which hands them to Lua.

// FFI to the vendored interpreter and our C library shims.
#![allow(unsafe_code)]

pub mod classes;
mod ffi;
pub mod files;
mod libc;
pub mod loader;
pub mod object_factory;
pub mod selftest;
pub mod services;
pub mod values;

use std::collections::HashSet;
use std::ffi::{CString, c_int, c_void};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::rc::Rc;

use ffi::{FlatValue, LuaState};

/// `_VERSION` of the runtime, as the original's (spec 1.1).
pub const VERSION: &str = "Lua 5.1 (alpha) + LuaJIT 1.0.3";

/// A Lua value crossing the API.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Boolean(bool),
    Number(f64),
    /// Lua strings are bytes (the game's are mostly Windows-1251).
    String(Vec<u8>),
    /// Key-value pairs, in Lua's traversal order.
    Table(Vec<(Value, Value)>),
    /// An object with its engine part ([`classes`]): the object itself when
    /// it comes from Lua or goes back to it, or a new object for a part
    /// made for one ([`classes::Objects`]).
    Object(classes::EnginePart),
    /// A value that doesn't cross (function, other userdata, thread), by
    /// type name.
    Other(&'static str),
}

impl Value {
    pub fn string(s: impl Into<Vec<u8>>) -> Self {
        Value::String(s.into())
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The string, if it is one and valid UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => std::str::from_utf8(s).ok(),
            _ => None,
        }
    }

    /// A table's value at `key`.
    pub fn get(&self, key: &Value) -> Option<&Value> {
        match self {
            Value::Table(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Boolean(b)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::string(s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The chunk doesn't compile.
    Syntax,
    /// Raised while running.
    Runtime,
    /// Raised by an error handler.
    ErrorHandler,
    /// A value or name that can't cross the API.
    Api,
    /// The script loader's fatal error: the original stops the game
    /// (docs/behavior/script-loading.md 2.2; [`loader::Scripts`]).
    Fatal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    /// The error message (lossy UTF-8).
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

fn api_error(message: impl Into<String>) -> Error {
    Error {
        kind: ErrorKind::Api,
        message: message.into(),
    }
}

type Function = dyn FnMut(&[Value]) -> Result<Vec<Value>, String>;

/// A Lua state with the libraries the original opens by default (spec 2.1):
/// base, table, string, math, jit and coco; never io, os, debug or package.
pub struct Lua {
    state: NonNull<LuaState>,
    /// Kept alive for the state's functions until it closes. Boxed twice:
    /// Lua holds a thin pointer to each inner box, which must not move.
    #[allow(clippy::vec_box)]
    functions: Vec<Box<Box<Function>>>,
    /// The engine parts of script objects and the engine classes' default
    /// implementations ([`classes`]).
    classes: Rc<classes::State>,
}

impl Lua {
    pub fn new() -> Result<Self, Error> {
        // SAFETY: lua_alloc is a valid allocator for the state's lifetime.
        let state = unsafe { ffi::volya_newstate(libc::lua_alloc, std::ptr::null_mut()) };
        let state = NonNull::new(state).ok_or_else(|| Error {
            kind: ErrorKind::Runtime,
            message: "the Lua state could not be created".into(),
        })?;
        Ok(Lua {
            state,
            functions: Vec::new(),
            classes: Rc::default(),
        })
    }

    /// Compiles `code` as a chunk named `name` (Lua's conventions: `=name`
    /// as it is, `@file` a file, else the source) and runs it.
    pub fn exec(&mut self, name: &str, code: &[u8]) -> Result<Vec<Value>, Error> {
        let name = cstring(name)?;
        self.run(Some((code, &name)), None, false, &[], true)
    }

    /// Runs `code` like [`Lua::exec`] and keeps its first result (or nil)
    /// where only [`Lua::call_kept`] reaches it: Lua's registry, which scripts
    /// can't read (there is no `debug` library).
    pub(crate) fn exec_kept(&mut self, name: &str, code: &[u8], key: &str) -> Result<(), Error> {
        let name = cstring(name)?;
        let key = cstring(key)?;
        self.run(Some((code, &name)), Some(&key), true, &[], true)
            .map(drop)
    }

    /// Calls the function [`Lua::exec_kept`] kept at `key` with `args`.
    pub(crate) fn call_kept(&mut self, key: &str, args: &[Value]) -> Result<Vec<Value>, Error> {
        let key = cstring(key)?;
        self.run(None, Some(&key), true, args, true)
    }

    /// Sets the global `name` to a factory: `name(key, op)` returns a C
    /// function calling the function kept at `key` with `op`, then its own
    /// arguments. Lua code can hand those out without exposing what they
    /// call.
    pub(crate) fn register_kept_factory(&mut self, name: &str) -> Result<(), Error> {
        let name = cstring(name)?;
        // SAFETY: the state is open; `name` outlives the call.
        let status =
            unsafe { ffi::volya_register_kept_protected(self.state.as_ptr(), name.as_ptr()) };
        self.check(status)
    }

    /// Compiles `code` without running it.
    pub fn compile(&mut self, name: &str, code: &[u8]) -> Result<(), Error> {
        let name = cstring(name)?;
        self.run(Some((code, &name)), None, false, &[], false)
            .map(drop)
    }

    /// Calls the global `function` with `args`.
    pub fn call(&mut self, function: &str, args: &[Value]) -> Result<Vec<Value>, Error> {
        let function = cstring(function)?;
        self.run(None, Some(&function), false, args, true)
    }

    /// Sets the global `name` to a function running `f`. A returned `Err`
    /// is raised in Lua as an error with that message.
    pub fn register(
        &mut self,
        name: &str,
        f: impl FnMut(&[Value]) -> Result<Vec<Value>, String> + 'static,
    ) -> Result<(), Error> {
        let name = cstring(name)?;
        let f = self.keep(f);
        let mut register = ffi::Register {
            name: name.as_ptr(),
            f: invoke,
            data: f.data,
        };
        // SAFETY: the state is open; `register` and the function outlive the call.
        let status = unsafe { ffi::volya_register_protected(self.state.as_ptr(), &mut register) };
        self.check(status)
    }

    /// Keeps `f` until the state closes, for C to call.
    fn keep(
        &mut self,
        f: impl FnMut(&[Value]) -> Result<Vec<Value>, String> + 'static,
    ) -> ffi::RustFn {
        let mut f: Box<Box<Function>> = Box::new(Box::new(f));
        let data = (&mut *f as *mut Box<Function>).cast();
        self.functions.push(f);
        ffi::RustFn {
            f: Some(invoke),
            data,
        }
    }

    fn run(
        &mut self,
        chunk: Option<(&[u8], &CString)>,
        global: Option<&CString>,
        registry: bool,
        args: &[Value],
        call: bool,
    ) -> Result<Vec<Value>, Error> {
        let mut flat = Vec::new();
        for arg in args {
            flatten(arg, &mut flat)?;
        }
        let mut results = Results::default();
        let mut run = ffi::Run {
            code: chunk.map_or(std::ptr::null(), |(code, _)| code.as_ptr().cast()),
            len: chunk.map_or(0, |(code, _)| code.len()),
            name: chunk.map_or(std::ptr::null(), |(_, name)| name.as_ptr()),
            global: global.map_or(std::ptr::null(), |g| g.as_ptr()),
            registry: c_int::from(registry),
            call: c_int::from(call),
            load_status: 0,
            args: flat.as_ptr(),
            nargs: args.len(),
            read: read_results,
            ctx: (&mut results as *mut Results).cast(),
        };
        // SAFETY: the state is open; everything `run` points to outlives the call.
        let status = unsafe { ffi::volya_run_protected(self.state.as_ptr(), &mut run) };
        // A load error is raised again to leave lua_cpcall: report its own status.
        self.check(if run.load_status != 0 {
            run.load_status
        } else {
            status
        })?;
        match results.error {
            Some(message) => Err(api_error(message)),
            None => Ok(results.values),
        }
    }

    /// Turns a protected call's status into a result, popping the error.
    fn check(&mut self, status: c_int) -> Result<(), Error> {
        if status == 0 {
            return Ok(());
        }
        let l = self.state.as_ptr();
        // SAFETY: a failed protected call leaves its error at the top. Only
        // scalars are read (a table isn't traversed): nothing is pushed.
        let message = unsafe {
            let message = match ffi::lua_type(l, -1) {
                ffi::LUA_TTABLE => "(error object is a table value)".to_owned(),
                ffi::LUA_TUSERDATA => "(error object is a userdata value)".to_owned(),
                _ => match read_value(l, -1, &mut HashSet::new()) {
                    Ok(Value::String(s)) => String::from_utf8_lossy(&s).into_owned(),
                    Ok(Value::Number(n)) => number_text(n),
                    Ok(Value::Nil) => "(error object is nil)".to_owned(),
                    Ok(Value::Boolean(b)) => format!("(error object is a boolean value: {b})"),
                    Ok(Value::Other(kind)) => format!("(error object is a {kind} value)"),
                    Ok(Value::Table(_) | Value::Object(_)) | Err(_) => {
                        unreachable!("not a table or userdata")
                    }
                },
            };
            ffi::lua_settop(l, -2);
            message
        };
        let kind = match status {
            ffi::LUA_ERRSYNTAX => ErrorKind::Syntax,
            ffi::LUA_ERRERR => ErrorKind::ErrorHandler,
            // Never: the allocator aborts instead.
            ffi::LUA_ERRMEM => ErrorKind::Runtime,
            _ => ErrorKind::Runtime,
        };
        Err(Error { kind, message })
    }
}

impl Drop for Lua {
    fn drop(&mut self) {
        // SAFETY: the state is open; its functions are dropped after it closes.
        unsafe { ffi::lua_close(self.state.as_ptr()) };
    }
}

/// A number as Lua writes it (`%.14g`, spec 2.5).
fn number_text(n: f64) -> String {
    let sign = if n.is_sign_negative() && !n.is_nan() {
        "-"
    } else {
        ""
    };
    format!("{sign}{}", libc::format_double(n, b'g', 14, false))
}

fn cstring(s: &str) -> Result<CString, Error> {
    CString::new(s).map_err(|_| api_error(format!("{s:?} contains a NUL byte")))
}

/// Appends `v` to `out` in prefix order (csrc/volya_glue.c). The pointers
/// borrow `v`.
fn flatten(v: &Value, out: &mut Vec<FlatValue>) -> Result<(), Error> {
    let mut flat = FlatValue {
        kind: ffi::VOLYA_NIL,
        boolean: 0,
        number: 0.0,
        bytes: std::ptr::null(),
        len: 0,
    };
    match v {
        Value::Nil => {}
        Value::Boolean(b) => {
            flat.kind = ffi::VOLYA_BOOLEAN;
            flat.boolean = c_int::from(*b);
        }
        Value::Number(n) => {
            flat.kind = ffi::VOLYA_NUMBER;
            flat.number = *n;
        }
        Value::String(s) => {
            flat.kind = ffi::VOLYA_STRING;
            flat.bytes = s.as_ptr().cast();
            flat.len = s.len();
        }
        Value::Table(pairs) => {
            flat.kind = ffi::VOLYA_TABLE;
            flat.len = pairs.len();
            out.push(flat);
            for (k, v) in pairs {
                match k {
                    Value::Nil => return Err(api_error("a table key can't be nil")),
                    Value::Number(n) if n.is_nan() => {
                        return Err(api_error("a table key can't be NaN"));
                    }
                    _ => {}
                }
                flatten(k, out)?;
                flatten(v, out)?;
            }
            return Ok(());
        }
        Value::Object(part) => {
            flat.kind = ffi::VOLYA_OBJECT;
            flat.number = part.value();
        }
        Value::Other(kind) => {
            return Err(api_error(format!("a {kind} value can't be passed to Lua")));
        }
    }
    out.push(flat);
    Ok(())
}

#[derive(Default)]
struct Results {
    values: Vec<Value>,
    error: Option<String>,
}

/// `volya_reader`: reads a call's results.
unsafe extern "C" fn read_results(ctx: *mut c_void, l: *mut LuaState, first: c_int, count: c_int) {
    // SAFETY: ctx is the `Results` of `Lua::run`, and the values are on the stack.
    unsafe {
        let results = &mut *ctx.cast::<Results>();
        for i in first..first + count {
            match read_value(l, i, &mut HashSet::new()) {
                Ok(v) => results.values.push(v),
                Err(e) => {
                    results.error = Some(e);
                    return;
                }
            }
        }
    }
}

/// Tables nested deeper than this don't cross to Rust. C reserves the stack
/// slots reading them takes (2 per level) before Rust reads
/// (`VOLYA_READ_SLOTS`, csrc/volya_glue.c).
const MAX_DEPTH: usize = 64;

/// Reads the value at `idx`, raising nothing: no metamethods, no
/// conversions, and no allocation inside Lua, as the stack slots it pushes
/// were reserved in C beforehand.
///
/// # Safety
/// `l` is open, `idx` valid, and 2 × [`MAX_DEPTH`] stack slots reserved.
unsafe fn read_value(
    l: *mut LuaState,
    idx: c_int,
    open: &mut HashSet<*const c_void>,
) -> Result<Value, String> {
    // SAFETY: the caller's contract; only non-raising API functions.
    unsafe {
        let idx = if idx < 0 {
            ffi::lua_gettop(l) + idx + 1
        } else {
            idx
        };
        Ok(match ffi::lua_type(l, idx) {
            ffi::LUA_TNIL => Value::Nil,
            ffi::LUA_TBOOLEAN => Value::Boolean(ffi::lua_toboolean(l, idx) != 0),
            ffi::LUA_TNUMBER => Value::Number(ffi::lua_tonumber(l, idx)),
            ffi::LUA_TSTRING => {
                let mut len = 0;
                let p = ffi::lua_tolstring(l, idx, &mut len);
                Value::String(std::slice::from_raw_parts(p.cast::<u8>(), len).to_vec())
            }
            ffi::LUA_TTABLE => {
                let table = ffi::lua_topointer(l, idx);
                if !open.insert(table) {
                    return Err("a table containing itself can't be passed to Rust".into());
                }
                if open.len() > MAX_DEPTH {
                    return Err(format!(
                        "tables nested more than {MAX_DEPTH} deep can't be passed to Rust"
                    ));
                }
                let mut pairs = Vec::new();
                ffi::lua_pushnil(l);
                while ffi::lua_next(l, idx) != 0 {
                    let pair =
                        read_value(l, -2, open).and_then(|k| Ok((k, read_value(l, -1, open)?)));
                    match pair {
                        Ok(pair) => pairs.push(pair),
                        Err(e) => {
                            ffi::lua_settop(l, -3);
                            return Err(e);
                        }
                    }
                    ffi::lua_settop(l, -2);
                }
                open.remove(&table);
                Value::Table(pairs)
            }
            ffi::LUA_TFUNCTION => Value::Other("function"),
            ffi::LUA_TUSERDATA => {
                let mut part = 0.0;
                if ffi::volya_object_part(l, idx, &mut part) != 0 {
                    Value::Object(classes::EnginePart::from_value(part))
                } else {
                    Value::Other("userdata")
                }
            }
            ffi::LUA_TLIGHTUSERDATA => Value::Other("userdata"),
            ffi::LUA_TTHREAD => Value::Other("thread"),
            _ => Value::Other("none"),
        })
    }
}

/// A Rust function's results, owned until C has pushed them.
struct Reply {
    _values: Vec<Value>,
    flat: Vec<FlatValue>,
    error: Vec<u8>,
}

unsafe extern "C" fn release(token: *mut c_void) {
    // SAFETY: token is a `Reply` from `invoke`, released once.
    drop(unsafe { Box::from_raw(token.cast::<Reply>()) });
}

/// `volya_function`: reads the arguments, runs the function and fills `reply`.
unsafe extern "C" fn invoke(data: *mut c_void, l: *mut LuaState, out: *mut ffi::Reply) {
    // SAFETY: data is a registered function, alive while its state is; the
    // arguments are on the stack at 1..top.
    unsafe {
        let f = &mut *data.cast::<Box<Function>>();
        let args: Result<Vec<Value>, String> = (1..=ffi::lua_gettop(l))
            .map(|i| read_value(l, i, &mut HashSet::new()))
            .collect();
        let result = args.and_then(|args| {
            catch_unwind(AssertUnwindSafe(|| f(&args)))
                .unwrap_or_else(|_| Err("a Rust function panicked".into()))
        });
        let mut reply = Reply {
            _values: Vec::new(),
            flat: Vec::new(),
            error: Vec::new(),
        };
        let mut count = 0;
        let mut failed = false;
        match result {
            Ok(values) => {
                for v in &values {
                    if let Err(e) = flatten(v, &mut reply.flat) {
                        reply.error = e.message.into_bytes();
                        failed = true;
                        break;
                    }
                }
                count = values.len();
                reply._values = values;
            }
            Err(e) => {
                reply.error = e.into_bytes();
                failed = true;
            }
        }
        let reply = Box::new(reply);
        let out = &mut *out;
        out.values = reply.flat.as_ptr();
        out.count = count;
        out.error = if failed {
            reply.error.as_ptr().cast()
        } else {
            std::ptr::null()
        };
        out.error_len = reply.error.len();
        out.release = Some(release);
        out.token = Box::into_raw(reply).cast();
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
