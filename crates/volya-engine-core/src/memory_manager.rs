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
//! its 23 bytes. The game scripts' pool ([`GAME_SCRIPTS`], *g*) is the same
//! [`Pools`] with chunks of its own, outside that count; its figure is its
//! bytes in use, its free blocks not among them (console-diagnostics.md 2.1).
//! So is the renderer's ([`RENDER`], *r*): the global allocator's blocks
//! made while a level's frame builds its lists ([`frame_lists`]). Each block
//! keeps the pools it came from in its byte after the request (a pool's
//! block holds at least the request and one byte, a large block 23 more),
//! so it goes back to them wherever it is freed.

// `GlobalAlloc` is unsafe to implement; the pools hand out raw blocks.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, UnsafeCell};
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
    HEAP.chunks.load(Ordering::Relaxed) * CHUNK_BYTES + HEAP.large.load(Ordering::Relaxed)
}

/// The chunks the global allocator's pools have taken.
pub fn chunks() -> usize {
    HEAP.chunks.load(Ordering::Relaxed)
}

/// The global allocator's pools, counted in [`in_use_bytes`].
static HEAP: Pools = Pools::new();

/// The game scripts' pool (*g*): the script runtime's allocator, outside
/// [`in_use_bytes`], as the original's game library commits its script
/// machine's pool apart from its heap (console-diagnostics.md 2.1, 2.1.1).
pub static GAME_SCRIPTS: Pools = Pools::new();

/// The renderer's pool (*r*): the lists a level's frame builds (its
/// passes, its scene and its draw list, [`frame_lists`]), outside
/// [`in_use_bytes`], as the original's renderer commits its own pool apart
/// from its heap (console-diagnostics.md 2.1, 2.1.1).
pub static RENDER: Pools = Pools::new();

/// The byte after a block's request: which pools it came from.
const FROM_HEAP: u8 = 0;
const FROM_RENDER: u8 = 1;

thread_local! {
    /// Whether this thread is building a frame's lists ([`frame_lists`]).
    static FRAME_LISTS: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` with the global allocator's new blocks taken from [`RENDER`]:
/// the lists a level's frame builds and refills (docs/architecture/memory.md,
/// The allocator). A block keeps its pools when it grows or is freed later.
pub fn frame_lists<R>(f: impl FnOnce() -> R) -> R {
    scoped(true, f)
}

/// Runs `f` with the global allocator's new blocks taken from the heap again,
/// inside [`frame_lists`]: what a frame loads to keep (the GPU's copies of a
/// level shown first, the images drawn), not its lists.
pub fn outside_frame_lists<R>(f: impl FnOnce() -> R) -> R {
    scoped(false, f)
}

fn scoped<R>(frame: bool, f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            FRAME_LISTS.with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(FRAME_LISTS.with(|c| c.replace(frame)));
    f()
}

/// The pools new blocks come from now, and their tag.
fn current() -> (&'static Pools, u8) {
    if FRAME_LISTS.with(Cell::get) {
        (&RENDER, FROM_RENDER)
    } else {
        (&HEAP, FROM_HEAP)
    }
}

/// The pools of a block whose request was `size` bytes.
///
/// # Safety
/// `p` was made by [`MemoryManager`] with a request of `size` bytes.
unsafe fn owner(p: *mut u8, size: usize) -> &'static Pools {
    // SAFETY: the block holds its tag after the request.
    match unsafe { p.add(size).read() } {
        FROM_RENDER => &RENDER,
        _ => &HEAP,
    }
}

/// Writes `tag` after the request of `size` bytes, if `p` is a block.
///
/// # Safety
/// `p` is null or a block of these pools for a request of `size` bytes.
unsafe fn tagged(p: *mut u8, size: usize, tag: u8) -> *mut u8 {
    if !p.is_null() {
        // SAFETY: every block holds a byte after its request.
        unsafe { p.add(size).write(tag) };
    }
    p
}

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

/// The 54 pools and the large blocks over `System` ([module docs](self)),
/// with chunks of their own: the global allocator's, and each pool kept
/// apart from the heap's figures ([`GAME_SCRIPTS`]).
pub struct Pools {
    pool: [Pool; POOLS],
    /// The chunks taken.
    chunks: AtomicUsize,
    /// The large blocks' bytes, each with its 23.
    large: AtomicUsize,
    /// The blocks handed out, bytes (each its pool's block size).
    used: AtomicUsize,
}

impl Pools {
    /// Pools that hold nothing yet.
    pub const fn new() -> Self {
        Self {
            pool: [const { Pool::new() }; POOLS],
            chunks: AtomicUsize::new(0),
            large: AtomicUsize::new(0),
            used: AtomicUsize::new(0),
        }
    }

    /// The bytes in use: what was taken from `System` less the free
    /// blocks, i.e. the blocks handed out and the large blocks with their
    /// 23 bytes (*g* for [`GAME_SCRIPTS`]).
    pub fn in_use_bytes(&self) -> usize {
        self.used.load(Ordering::Relaxed) + self.large.load(Ordering::Relaxed)
    }

    /// The chunks taken.
    pub fn chunks(&self) -> usize {
        self.chunks.load(Ordering::Relaxed)
    }

    /// A block of `pool`, or null when `System` has no chunk to give.
    fn take(&self, pool: usize) -> *mut u8 {
        let block: *mut u8 = self.pool[pool].with(|free| {
            if free.is_null() {
                // SAFETY: the layout's size is nonzero.
                let chunk =
                    unsafe { System.alloc(Layout::from_size_align_unchecked(CHUNK_BYTES, ALIGN)) };
                if chunk.is_null() {
                    return chunk;
                }
                self.chunks.fetch_add(1, Ordering::Relaxed);
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
        });
        if !block.is_null() {
            self.used.fetch_add(block_bytes(pool), Ordering::Relaxed);
        }
        block
    }

    /// Gives `block` back to `pool`.
    fn give(&self, pool: usize, block: *mut u8) {
        self.pool[pool].with(|free| {
            let block = block.cast::<Free>();
            // SAFETY: a block of this pool, no longer used.
            unsafe { block.write(Free { next: *free }) };
            *free = block;
        });
        self.used.fetch_sub(block_bytes(pool), Ordering::Relaxed);
    }

    /// The `System` layout of a large request.
    fn large(layout: Layout) -> Option<Layout> {
        let size = layout.size().checked_add(LARGE_EXTRA)?;
        Layout::from_size_align(size, layout.align()).ok()
    }

    /// A block for `layout`, or null when `System` has none to give.
    ///
    /// # Safety
    /// As [`GlobalAlloc::alloc`]: `layout`'s size is nonzero.
    pub unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if let Some(pool) = pool_of(layout.size(), layout.align()) {
            return self.take(pool);
        }
        let Some(large) = Self::large(layout) else {
            return std::ptr::null_mut();
        };
        // SAFETY: a nonzero size.
        let p = unsafe { System.alloc(large) };
        if !p.is_null() {
            self.large.fetch_add(large.size(), Ordering::Relaxed);
        }
        p
    }

    /// As [`Pools::alloc`], the bytes zeroed.
    ///
    /// # Safety
    /// As [`GlobalAlloc::alloc_zeroed`].
    pub unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if let Some(pool) = pool_of(layout.size(), layout.align()) {
            let p = self.take(pool);
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
            self.large.fetch_add(large.size(), Ordering::Relaxed);
        }
        p
    }

    /// Frees `p`.
    ///
    /// # Safety
    /// As [`GlobalAlloc::dealloc`]: `p` was made by these pools with
    /// `layout`.
    pub unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        if let Some(pool) = pool_of(layout.size(), layout.align()) {
            return self.give(pool, p);
        }
        // SAFETY: made by `alloc` with this layout, so `large` held then.
        let large = unsafe { Self::large(layout).unwrap_unchecked() };
        unsafe { System.dealloc(p, large) };
        self.large.fetch_sub(large.size(), Ordering::Relaxed);
    }

    /// `p` resized to `size` bytes; within the same pool it stays in place.
    ///
    /// # Safety
    /// As [`GlobalAlloc::realloc`].
    pub unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
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
                    self.large.fetch_add(new, Ordering::Relaxed);
                    self.large.fetch_sub(old.size(), Ordering::Relaxed);
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

impl Default for Pools {
    fn default() -> Self {
        Self::new()
    }
}

/// The global allocator ([module docs](self)).
pub struct MemoryManager;

// SAFETY: the pools keep GlobalAlloc's contract; each block's tag names
// the pools it came from.
unsafe impl GlobalAlloc for MemoryManager {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (pools, tag) = current();
        // SAFETY: the caller's contract is the pools'; the block is its.
        unsafe { tagged(pools.alloc(layout), layout.size(), tag) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let (pools, tag) = current();
        // SAFETY: as `alloc`.
        unsafe { tagged(pools.alloc_zeroed(layout), layout.size(), tag) }
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        // SAFETY: made by `alloc` with this layout.
        unsafe { owner(p, layout.size()).dealloc(p, layout) }
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: as `dealloc`; the block keeps its pools.
        unsafe {
            let tag = p.add(layout.size()).read();
            tagged(owner(p, layout.size()).realloc(p, layout, size), size, tag)
        }
    }
}

/// Bytes taken from `System` directly, outside the pools and not counted
/// in [`in_use_bytes`]: the file cache's (docs/architecture/memory.md,
/// File-backed data), which has a figure of its own.
pub struct SystemBytes {
    ptr: std::ptr::NonNull<u8>,
    len: usize,
}

// SAFETY: it owns its bytes, as a `Box<[u8]>` does.
unsafe impl Send for SystemBytes {}
unsafe impl Sync for SystemBytes {}

impl SystemBytes {
    /// `len` zeroed bytes, or `None` when `System` has none to give.
    pub fn zeroed(len: usize) -> Option<Self> {
        if len == 0 {
            return Some(Self {
                ptr: std::ptr::NonNull::dangling(),
                len,
            });
        }
        let layout = Layout::from_size_align(len, 1).ok()?;
        // SAFETY: a nonzero size.
        let ptr = std::ptr::NonNull::new(unsafe { System.alloc_zeroed(layout) })?;
        Some(Self { ptr, len })
    }

    /// Keeps the first `len` bytes and gives the rest back to `System`
    /// (all kept where it can't shrink the block).
    pub fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }
        if len == 0 {
            *self = Self {
                ptr: std::ptr::NonNull::dangling(),
                len,
            };
            return;
        }
        // SAFETY: made by `zeroed` with this layout; a nonzero new size.
        let ptr = unsafe {
            System.realloc(
                self.ptr.as_ptr(),
                Layout::from_size_align_unchecked(self.len, 1),
                len,
            )
        };
        if let Some(ptr) = std::ptr::NonNull::new(ptr) {
            (self.ptr, self.len) = (ptr, len);
        }
    }
}

impl std::ops::Deref for SystemBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        // SAFETY: `len` initialised bytes owned by `self`.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl std::ops::DerefMut for SystemBytes {
    fn deref_mut(&mut self) -> &mut [u8] {
        // SAFETY: as `deref`, borrowed mutably.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for SystemBytes {
    fn drop(&mut self) {
        if self.len > 0 {
            // SAFETY: made by `zeroed` with this layout.
            unsafe {
                System.dealloc(
                    self.ptr.as_ptr(),
                    Layout::from_size_align_unchecked(self.len, 1),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_bytes_are_outside_the_count() {
        let before = in_use_bytes();
        let mut bytes = SystemBytes::zeroed(100_000).unwrap();
        bytes[99_999] = 3;
        assert_eq!(bytes.iter().map(|&b| u32::from(b)).sum::<u32>(), 3);
        // Other tests' threads may allocate meanwhile, but none 100 K.
        assert!(in_use_bytes() < before + 100_000);
        assert!(SystemBytes::zeroed(0).unwrap().is_empty());
    }

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
    fn a_pool_of_its_own_counts_its_bytes_in_use_outside_the_heap() {
        let pools = Pools::new();
        let (small, big) = (
            Layout::from_size_align(40, 8).unwrap(),
            Layout::from_size_align(2000, 8).unwrap(),
        );
        // SAFETY: allocations of a nonzero size, freed with their layouts.
        unsafe {
            let a = pools.alloc(small);
            let b = pools.alloc(big);
            // The pool of 48 bytes; the large block with its 23.
            assert_eq!(pools.in_use_bytes(), 48 + 2023);
            assert_eq!(pools.chunks(), 1);
            let c = pools.realloc(a, small, 100);
            assert_eq!(pools.in_use_bytes(), 112 + 2023);
            pools.dealloc(c, Layout::from_size_align(100, 8).unwrap());
            pools.dealloc(b, big);
        }
        // Its chunks are kept, free; its figure is what is in use.
        assert_eq!(pools.in_use_bytes(), 0);
        assert_eq!(pools.chunks(), 2);
    }

    #[test]
    fn a_frames_blocks_are_the_renderers_wherever_freed() {
        let m = MemoryManager;
        let (small, big) = (
            Layout::from_size_align(100, 8).unwrap(),
            Layout::from_size_align(3000, 8).unwrap(),
        );
        // Other tests' threads use the heap, not this thread's frame.
        let before = RENDER.in_use_bytes();
        // SAFETY: allocations of a nonzero size, freed with their layouts.
        unsafe {
            let (a, b) = frame_lists(|| (m.alloc(small), m.alloc(big)));
            let kept = outside_frame_lists(|| m.alloc(small));
            frame_lists(|| m.dealloc(kept, small));
            // The pool of 112 bytes; the large block with its 23.
            assert_eq!(RENDER.in_use_bytes(), before + 112 + 3023);
            // Grown and freed outside the frame: still the renderer's.
            let a = m.realloc(a, small, 2000);
            assert_eq!(RENDER.in_use_bytes(), before + 2023 + 3023);
            m.dealloc(a, Layout::from_size_align(2000, 8).unwrap());
            m.dealloc(b, big);
        }
        assert_eq!(RENDER.in_use_bytes(), before);
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
