// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `malloc`, `free` and `realloc` on wasm32, which has no C library: Rust's
//! allocator, for all of the app's C (the physics solver's, here, and the
//! script runtime's, `volya-engine-script`). Natively the system's serve.

#![allow(unsafe_code)]

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error, realloc as rust_realloc};
use std::ffi::c_void;

/// Each block starts with its size, so `free` can rebuild its layout; 8
/// bytes keep the rest aligned for a double or a pointer.
const HEADER: usize = 8;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, HEADER)
        .unwrap_or_else(|_| handle_alloc_error(Layout::new::<u8>()))
}

/// # Safety
/// As C's `malloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    let Some(total) = size.checked_add(HEADER) else {
        return std::ptr::null_mut();
    };
    // SAFETY: total is nonzero.
    let p = unsafe { alloc(layout(total)) };
    if p.is_null() {
        return p.cast();
    }
    // SAFETY: the block holds at least HEADER bytes, aligned.
    unsafe {
        p.cast::<usize>().write(total);
        p.add(HEADER).cast()
    }
}

/// # Safety
/// As C's `free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(p: *mut c_void) {
    if p.is_null() {
        return;
    }
    // SAFETY: p came from malloc or realloc.
    unsafe {
        let block = p.cast::<u8>().sub(HEADER);
        dealloc(block, layout(block.cast::<usize>().read()));
    }
}

/// # Safety
/// As C's `realloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(p: *mut c_void, size: usize) -> *mut c_void {
    if p.is_null() {
        // SAFETY: plain allocation.
        return unsafe { malloc(size) };
    }
    let Some(total) = size.checked_add(HEADER) else {
        return std::ptr::null_mut();
    };
    // SAFETY: p came from malloc or realloc.
    unsafe {
        let block = p.cast::<u8>().sub(HEADER);
        let old = block.cast::<usize>().read();
        let q = rust_realloc(block, layout(old), total);
        if q.is_null() {
            return q.cast();
        }
        q.cast::<usize>().write(total);
        q.add(HEADER).cast()
    }
}
