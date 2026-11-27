// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The C library functions our C code takes from Rust: Lua's allocator and
//! its string-to-number conversion on every target, floating-point digits
//! for our `printf` (csrc/volya_printf.c). On wasm32 the rest of our C
//! library (csrc/wasm/libc.c) takes `malloc` and friends from
//! `volya_engine_core::c_heap`.

use std::alloc::{Layout, handle_alloc_error};
use std::ffi::{CStr, c_char, c_int, c_void};

use volya_engine_core::memory_manager::GAME_SCRIPTS;

/// Lua's values need at most a double's or a pointer's alignment.
const ALIGN: usize = 8;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap_or_else(|_| handle_alloc_error(Layout::new::<u8>()))
}

/// Lua's allocator (`lua_Alloc`): the game scripts' pool, outside the
/// heap's figures and reported as *g* (docs/architecture/memory.md, The
/// allocator), aborting when memory runs out, so Lua never raises a memory
/// error (a recoverable budget is #107).
pub unsafe extern "C" fn lua_alloc(
    _ud: *mut c_void,
    ptr: *mut c_void,
    osize: usize,
    nsize: usize,
) -> *mut c_void {
    let ptr = ptr.cast::<u8>();
    let new = if nsize == 0 {
        if !ptr.is_null() && osize != 0 {
            // SAFETY: Lua passes the block's size as allocated.
            unsafe { GAME_SCRIPTS.dealloc(ptr, layout(osize)) };
        }
        return std::ptr::null_mut();
    } else if ptr.is_null() || osize == 0 {
        // SAFETY: nsize is nonzero.
        unsafe { GAME_SCRIPTS.alloc(layout(nsize)) }
    } else {
        // SAFETY: as above; nsize is nonzero.
        unsafe { GAME_SCRIPTS.realloc(ptr, layout(osize), nsize) }
    };
    if new.is_null() {
        handle_alloc_error(layout(nsize));
    }
    new.cast()
}

/// The length of the longest prefix of `s` (after leading white space)
/// that is a decimal number in ANSI C's `strtod` syntax, and its value.
fn decimal_prefix(s: &[u8]) -> Option<(f64, usize)> {
    let digits = |from: usize| s[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut i = s
        .iter()
        .take_while(|&&c| c == b' ' || (b'\t'..=b'\r').contains(&c))
        .count();
    let start = i;
    if matches!(s.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let int = digits(i);
    i += int;
    let mut frac = 0;
    if s.get(i) == Some(&b'.') {
        frac = digits(i + 1);
        if int + frac > 0 {
            i += 1 + frac;
        }
    }
    if int + frac == 0 {
        return None;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(s.get(i + 1), Some(b'+' | b'-')));
        let exp = digits(i + 1 + sign);
        if exp > 0 {
            i += 1 + sign + exp;
        }
    }
    let text = std::str::from_utf8(&s[start..i]).ok()?;
    Some((text.parse().ok()?, i))
}

/// `strtod` for Lua (`lua_str2number`, patched into luaconf.h by build.rs):
/// decimal only, correctly rounded, the same on every target.
///
/// # Safety
/// `s` is NUL-terminated; `end` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn volya_strtod(s: *const c_char, end: *mut *mut c_char) -> f64 {
    // SAFETY: the caller's contract.
    let bytes = unsafe { CStr::from_ptr(s) }.to_bytes();
    let (value, len) = decimal_prefix(bytes).unwrap_or((0.0, 0));
    if !end.is_null() {
        // SAFETY: len <= the string's length.
        unsafe { *end = s.add(len).cast_mut() };
    }
    value
}

/// The text of `|v|` for the C conversion `conv` (`e`, `f` or `g`, either
/// case) with precision `prec`; `alt` is the `#` flag.
pub fn format_double(v: f64, conv: u8, prec: usize, alt: bool) -> String {
    let upper = conv.is_ascii_uppercase();
    let v = v.abs();
    let text = if v.is_nan() {
        "nan".to_owned()
    } else if v.is_infinite() {
        "inf".to_owned()
    } else {
        match conv.to_ascii_lowercase() {
            b'e' => exponential(v, prec, alt),
            b'f' => fixed(v, prec, alt),
            _ => general(v, prec, alt),
        }
    };
    if upper {
        text.to_ascii_uppercase()
    } else {
        text
    }
}

fn fixed(v: f64, prec: usize, alt: bool) -> String {
    let mut s = format!("{v:.prec$}");
    if alt && prec == 0 {
        s.push('.');
    }
    s
}

/// `d.ddde±xx`, and the exponent.
fn exponential_parts(v: f64, prec: usize, alt: bool) -> (String, i32) {
    let s = format!("{v:.prec$e}");
    let (mantissa, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let dot = if alt && prec == 0 { "." } else { "" };
    let sign = if exp < 0 { '-' } else { '+' };
    (
        format!("{mantissa}{dot}e{sign}{:02}", exp.unsigned_abs()),
        exp,
    )
}

fn exponential(v: f64, prec: usize, alt: bool) -> String {
    exponential_parts(v, prec, alt).0
}

fn general(v: f64, prec: usize, alt: bool) -> String {
    let p = prec.max(1);
    let (e_text, x) = exponential_parts(v, p - 1, alt);
    let mut s = if (x as i64) < p as i64 && x >= -4 {
        fixed(v, (p as i64 - 1 - x as i64) as usize, alt)
    } else {
        e_text
    };
    if !alt {
        let (body, tail) = match s.find('e') {
            Some(at) => s.split_at(at),
            None => (s.as_str(), ""),
        };
        if body.contains('.') {
            let body = body.trim_end_matches('0').trim_end_matches('.');
            s = format!("{body}{tail}");
        }
    }
    s
}

/// `volya_format_double` (csrc/volya_printf.c): writes [`format_double`]'s
/// text to `out` (at most `cap` bytes) and returns its full length.
///
/// # Safety
/// `out` is writable for `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn volya_format_double(
    out: *mut c_char,
    cap: usize,
    v: f64,
    conv: c_int,
    prec: c_int,
    alt: c_int,
) -> usize {
    let text = format_double(v, conv as u8, prec.max(0) as usize, alt != 0);
    let n = text.len().min(cap);
    // SAFETY: n <= cap.
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), out.cast::<u8>(), n) };
    text.len()
}
