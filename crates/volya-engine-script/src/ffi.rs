// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The parts of the Lua C API (vendor/luajit-1.0.3/src/lua.h) and of our C
//! glue (csrc/volya_glue.c) the safe API uses. Only functions that never
//! raise a Lua error are called from Rust; everything else runs in C.

use std::ffi::{c_char, c_int, c_void};

#[repr(C)]
pub struct LuaState {
    _private: [u8; 0],
}

pub const LUA_TNIL: c_int = 0;
pub const LUA_TBOOLEAN: c_int = 1;
pub const LUA_TLIGHTUSERDATA: c_int = 2;
pub const LUA_TNUMBER: c_int = 3;
pub const LUA_TSTRING: c_int = 4;
pub const LUA_TTABLE: c_int = 5;
pub const LUA_TFUNCTION: c_int = 6;
pub const LUA_TUSERDATA: c_int = 7;
pub const LUA_TTHREAD: c_int = 8;

pub const LUA_ERRSYNTAX: c_int = 3;
pub const LUA_ERRMEM: c_int = 4;
pub const LUA_ERRERR: c_int = 5;

pub type LuaAlloc = unsafe extern "C" fn(
    ud: *mut c_void,
    ptr: *mut c_void,
    osize: usize,
    nsize: usize,
) -> *mut c_void;

/// `volya_value` (csrc/volya_glue.c).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FlatValue {
    pub kind: c_int,
    pub boolean: c_int,
    pub number: f64,
    pub bytes: *const c_char,
    pub len: usize,
}

pub const VOLYA_NIL: c_int = 0;
pub const VOLYA_BOOLEAN: c_int = 1;
pub const VOLYA_NUMBER: c_int = 2;
pub const VOLYA_STRING: c_int = 3;
pub const VOLYA_TABLE: c_int = 4;

pub type Reader =
    unsafe extern "C" fn(ctx: *mut c_void, l: *mut LuaState, first: c_int, count: c_int);

/// `volya_run` (csrc/volya_glue.c).
#[repr(C)]
pub struct Run {
    pub code: *const c_char,
    pub len: usize,
    pub name: *const c_char,
    pub global: *const c_char,
    pub registry: c_int,
    pub call: c_int,
    pub load_status: c_int,
    pub args: *const FlatValue,
    pub nargs: usize,
    pub read: Reader,
    pub ctx: *mut c_void,
}

/// `volya_reply` (csrc/volya_glue.c).
#[repr(C)]
pub struct Reply {
    pub values: *const FlatValue,
    pub count: usize,
    pub error: *const c_char,
    pub error_len: usize,
    pub token: *mut c_void,
    pub release: Option<unsafe extern "C" fn(token: *mut c_void)>,
}

pub type Function = unsafe extern "C" fn(data: *mut c_void, l: *mut LuaState, reply: *mut Reply);

/// `volya_register` (csrc/volya_glue.c).
#[repr(C)]
pub struct Register {
    pub name: *const c_char,
    pub f: Function,
    pub data: *mut c_void,
}

unsafe extern "C" {
    pub fn volya_newstate(alloc: LuaAlloc, ud: *mut c_void) -> *mut LuaState;
    pub fn volya_run_protected(l: *mut LuaState, run: *mut Run) -> c_int;
    pub fn volya_register_protected(l: *mut LuaState, register: *mut Register) -> c_int;

    pub fn lua_close(l: *mut LuaState);
    pub fn lua_gettop(l: *mut LuaState) -> c_int;
    pub fn lua_settop(l: *mut LuaState, idx: c_int);
    pub fn lua_type(l: *mut LuaState, idx: c_int) -> c_int;
    pub fn lua_tonumber(l: *mut LuaState, idx: c_int) -> f64;
    pub fn lua_toboolean(l: *mut LuaState, idx: c_int) -> c_int;
    pub fn lua_tolstring(l: *mut LuaState, idx: c_int, len: *mut usize) -> *const c_char;
    pub fn lua_topointer(l: *mut LuaState, idx: c_int) -> *const c_void;
    pub fn lua_pushnil(l: *mut LuaState);
    pub fn lua_next(l: *mut LuaState, idx: c_int) -> c_int;
}

#[cfg(all(test, not(target_arch = "wasm32")))]
unsafe extern "C" {
    /// csrc/volya_printf.c, compared with the system's in tests.
    pub fn volya_snprintf(buf: *mut c_char, cap: usize, fmt: *const c_char, ...) -> c_int;
}
