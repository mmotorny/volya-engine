// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The memory manager: the global allocator natively and in wasm, as the
//! original's (docs/behavior/console-diagnostics.md 2.1.1,
//! docs/architecture/memory.md, The allocator). A request of up to 862
//! bytes (aligned at most to 16) goes to one of 54 pools of fixed block
//! sizes, 16 to 864 bytes; a pool takes its memory from `System` in chunks
//! of 16 KiB plus 22 bytes and never returns them. A larger request is one
//! `System` block of the request plus 23 bytes. [`in_use_bytes`] counts as
//! `* phase cmem` and *h* do: every chunk whole and every large block with
//! its 23 bytes.

// `GlobalAlloc` is unsafe to implement; the pools hand out raw blocks.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The pools' count; block sizes 16, 32, … 864 bytes.
pub const POOLS: usize = 54;
/// The largest request a pool serves, bytes.
pub const LARGEST_POOLED: usize = 862;
/// What a pool takes from `System` at a time, bytes.
pub const CHUNK_BYTES: usize = 16 * 1024 + 22;
/// What a large block adds to its request, bytes.
pub const LARGE_EXTRA: usize = 23;
/// The part of a chunk blocks are carved from.
const CHUNK_BLOCKS_BYTES: usize = 16 * 1024;
/// The alignment every block has (its size a multiple of 16, the chunk's
/// start aligned to 16).
const ALIGN: usize = 16;

/// The pool a request of `size` bytes aligned to `align` goes to: of
/// 16 × (⌊(size + 1) / 16⌋ + 1) bytes; `None` for a large request.
pub fn pool_of(size: usize, align: usize) -> Option<usize> {
    (size <= LARGEST_POOLED && align <= ALIGN).then_some((size + 1) / 16)
}

/// A pool's block size, bytes.
pub fn block_bytes(pool: usize) -> usize {
    16 * (pool + 1)
}

/// The bytes in use as the original's heap walk counts them: every pool's
/// chunks whole, used or free, and every large block with its 23 bytes
/// (`System`'s own overhead not in it).
pub fn in_use_bytes() -> usize {
    CHUNKS.load(Ordering::Relaxed) * CHUNK_BYTES + LARGE.load(Ordering::Relaxed)
}

/// The chunks the pools have taken.
pub fn chunks() -> usize {
    CHUNKS.load(Ordering::Relaxed)
}

static CHUNKS: AtomicUsize = AtomicUsize::new(0);
static LARGE: AtomicUsize = AtomicUsize::new(0);

/// A free block: the next free block of its pool.
struct Free {
    next: *mut Free,
}

/// A pool: its free blocks, behind a lock (native tests run threads; the
/// engine's worker is single-threaded, so it is never contended there).
struct Pool {
    locked: AtomicBool,
    free: UnsafeCell<*mut Free>,
}

// SAFETY: `free` is only reached while `locked` is held.
unsafe impl Sync for Pool {}

impl Pool {
    const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            free: UnsafeCell::new(std::ptr::null_mut()),
        }
    }

    /// Runs `f` on the free list with the lock held.
    fn with<R>(&self, f: impl FnOnce(&mut *mut Free) -> R) -> R {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        // SAFETY: the lock is held.
        let r = f(unsafe { &mut *self.free.get() });
        self.locked.store(false, Ordering::Release);
        r
    }
}

static POOL: [Pool; POOLS] = [const { Pool::new() }; POOLS];

/// The global allocator ([module docs](self)).
pub struct MemoryManager;

impl MemoryManager {
    /// A block of `pool`, or null when `System` has no chunk to give.
    fn take(pool: usize) -> *mut u8 {
        POOL[pool].with(|free| {
            if free.is_null() {
                // SAFETY: the layout's size is nonzero.
                let chunk =
                    unsafe { System.alloc(Layout::from_size_align_unchecked(CHUNK_BYTES, ALIGN)) };
                if chunk.is_null() {
                    return chunk;
                }
                CHUNKS.fetch_add(1, Ordering::Relaxed);
                let size = block_bytes(pool);
                for k in (0..CHUNK_BLOCKS_BYTES / size).rev() {
                    // SAFETY: within the chunk, aligned to 16.
                    let block = unsafe { chunk.add(k * size) }.cast::<Free>();
                    unsafe { block.write(Free { next: *free }) };
                    *free = block;
                }
            }
            let block = *free;
            // SAFETY: a free block of this pool.
            *free = unsafe { (*block).next };
            block.cast()
        })
    }

    /// Gives `block` back to `pool`.
    fn give(pool: usize, block: *mut u8) {
        POOL[pool].with(|free| {
            let block = block.cast::<Free>();
            // SAFETY: a block of this pool, no longer used.
            unsafe { block.write(Free { next: *free }) };
            *free = block;
        });
    }

    /// The `System` layout of a large request.
    fn large(layout: Layout) -> Option<Layout> {
        let size = layout.size().checked_add(LARGE_EXTRA)?;
        Layout::from_size_align(size, layout.align()).ok()
    }
}

unsafe impl GlobalAlloc for MemoryManager {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if let Some(pool) = pool_of(layout.size(), layout.align()) {
            return Self::take(pool);
        }
        let Some(large) = Self::large(layout) else {
            return std::ptr::null_mut();
        };
        // SAFETY: a nonzero size.
        let p = unsafe { System.alloc(large) };
        if !p.is_null() {
            LARGE.fetch_add(large.size(), Ordering::Relaxed);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if let Some(pool) = pool_of(layout.size(), layout.align()) {
            let p = Self::take(pool);
            if !p.is_null() {
                // SAFETY: the block holds at least the request.
                unsafe { p.write_bytes(0, layout.size()) };
            }
            return p;
        }
        let Some(large) = Self::large(layout) else {
            return std::ptr::null_mut();
        };
        // SAFETY: a nonzero size.
        let p = unsafe { System.alloc_zeroed(large) };
        if !p.is_null() {
            LARGE.fetch_add(large.size(), Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        if let Some(pool) = pool_of(layout.size(), layout.align()) {
            return Self::give(pool, p);
        }
        // SAFETY: made by `alloc` with this layout, so `large` held then.
        let large = unsafe { Self::large(layout).unwrap_unchecked() };
        unsafe { System.dealloc(p, large) };
        LARGE.fetch_sub(large.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let align = layout.align();
        let (from, to) = (pool_of(layout.size(), align), pool_of(size, align));
        match (from, to) {
            // Within the same pool it stays in place.
            (Some(a), Some(b)) if a == b => p,
            (None, None) => {
                // SAFETY: as `dealloc`.
                let old = unsafe { Self::large(layout).unwrap_unchecked() };
                let Some(new) = size.checked_add(LARGE_EXTRA) else {
                    return std::ptr::null_mut();
                };
                // SAFETY: `p` was made with `old`; `new` is nonzero.
                let q = unsafe { System.realloc(p, old, new) };
                if !q.is_null() {
                    LARGE.fetch_add(new, Ordering::Relaxed);
                    LARGE.fetch_sub(old.size(), Ordering::Relaxed);
                }
                q
            }
            _ => {
                // SAFETY: the caller's `size` with the block's alignment is a
                // valid layout (GlobalAlloc::realloc's contract).
                let new = unsafe { Layout::from_size_align_unchecked(size, align) };
                let q = unsafe { self.alloc(new) };
                if !q.is_null() {
                    // SAFETY: both hold at least the smaller size.
                    unsafe {
                        std::ptr::copy_nonoverlapping(p, q, layout.size().min(size));
                        self.dealloc(p, layout);
                    }
                }
                q
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_go_to_the_pools_of_the_spec() {
        // console-diagnostics.md 2.1.1: n bytes to 16 × (⌊(n + 1) / 16⌋ + 1).
        let size = |n| pool_of(n, 8).map(block_bytes);
        assert_eq!(size(1), Some(16));
        assert_eq!(size(14), Some(16));
        assert_eq!(size(15), Some(32));
        assert_eq!(size(31), Some(48));
        assert_eq!(size(862), Some(864));
        assert_eq!(pool_of(862, 8), Some(POOLS - 1));
        assert_eq!(size(863), None);
        assert_eq!(pool_of(16, 32), None, "aligned over 16: large");
    }

    #[test]
    fn blocks_are_aligned_reused_and_chunks_kept() {
        let m = MemoryManager;
        let layout = Layout::from_size_align(700, 16).unwrap();
        // SAFETY: allocations of a nonzero size, freed with their layouts.
        unsafe {
            let p = m.alloc(layout);
            assert_eq!(p as usize % 16, 0);
            p.write_bytes(7, 700);
            m.dealloc(p, layout);
            // The pool's free block serves the next request of its size.
            let q = m.alloc(layout);
            assert_eq!(q, p);
            // Grown within its pool, in place; beyond it, moved, the bytes kept.
            // (700 to 702 bytes: both the pool of 704.)
            let r = m.realloc(q, layout, 702);
            assert_eq!(r, q);
            let big = m.realloc(r, Layout::from_size_align(702, 16).unwrap(), 5000);
            assert_eq!(*big.add(699), 7);
            let zeroed = m.alloc_zeroed(Layout::from_size_align(900, 8).unwrap());
            assert_eq!(*zeroed.add(899), 0);
            m.dealloc(zeroed, Layout::from_size_align(900, 8).unwrap());
            m.dealloc(big, Layout::from_size_align(5000, 16).unwrap());
        }
        assert!(chunks() >= 1);
        assert!(in_use_bytes() >= CHUNK_BYTES);
    }
}
