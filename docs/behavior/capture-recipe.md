# A deterministic full capture of the original on Windows

Environment control only: what the capture tooling (`tools/wincapture`) supplies to the unmodified game,
and when. It says nothing about the program's insides. It was found by a one-time dynamic investigation
(#836) and approved by a fresh reviewer, who rebuilt it independently from this text and main's source and
got the same frame hashes. The reviewer's two corrections are applied, and the choices the text left open
are stated as the reviewer made them (section 4).

## 1. Variant

DynamoRIO detached after the first held frame, plus a deterministic heap. This is the capture's existing
detach behaviour with two changes to our own tools:

1. the counter client imports only ntdll (rule 2);
2. the frame-clock proxy also replaces the heap functions (rule 3).

Everything else is unchanged: the frame clock, the secondary-pass ordering, the window kept in front, the
plan, the configuration file and the steps.

## 2. Rules

1. **DynamoRIO.**
   - Version: the pinned 32-bit Windows release, 11.92.20777.
   - drrun options: `-quiet -msgbox_mask 0 -stderr_mask 12 -c <client> <game folder copy>`, nothing more.
   - The game starts under drrun with the counter client.
   - The driver detaches DynamoRIO (`drconfig -detach <pid>`) once the first frame is held. The rest of the
     run is native.
2. **The counter client imports no system library except ntdll.**
   - It opens the frame clock's shared memory through ntdll's section functions:
     - the process's session from the process-information query (class 24);
     - the section `\Sessions\<session>\BaseNamedObjects\VolyaFrameClock`, or
       `\BaseNamedObjects\VolyaFrameClock` in session 0, opened for map-read and map-write, with
       case-insensitive object attributes;
     - a view of the shared memory's structure size, mapped read-write.
   - It imports nothing from kernel32, so DynamoRIO loads no private copy of the system libraries for it.
   - Linking: ntdll's import library comes before DynamoRIO's on the link line (DynamoRIO's also provides
     one of the section function names). The client's import table names only ntdll and DynamoRIO.
   - The substitution is unchanged: every `rdtsc` and `rdtscp` executed in a module of the game's folder copy
     returns 3,600,000 × F + clock_ms × F + n × 30, with F = 3,000,000, clock_ms the frame clock's
     milliseconds and n the number of reads substituted before in the process; `rdtscp`'s processor number
     is 0.
   - Measured: with a client importing kernel32, runs kept under DynamoRIO ended with exit code 0x502 at
     varying frames, and the detach after the first held frame ended with 0xc0000005 before frame 2. The
     cause is unestablished. With the ntdll-only client neither failure happened in any full run (about 17
     under DynamoRIO for the whole run and 4 with the detach).
3. **A deterministic heap for the game's modules.**
   - Which modules: the proxy (`winmm.dll`) replaces these kernel32 imports in the same modules whose clock
     imports it already replaces (every module that isn't Windows' own, the game's folder and the
     side-by-side runtimes included, and libraries loaded later): `HeapCreate`, `HeapDestroy`, `HeapAlloc`,
     `HeapReAlloc`, `HeapFree`, `HeapSize`, `HeapValidate`, `HeapWalk` and `HeapCompact`. The replacement
     starts when the proxy attaches.
   - The region:
     - When the proxy starts, it reserves one region of 896 MiB (0x38000000 bytes) at the fixed address
       0x20000000. If the reservation fails, the run fails; it never falls back to another address.
     - It commits the region in 4 MiB steps as allocations reach them.
     - Measured: about 446 MiB used by the end of a full run.
   - `HeapAlloc`:
     - Every request is served from the region, whatever heap handle is passed.
     - A request is rounded up to a size class: 8 to 512 bytes in steps of 8, then each class 1/8 larger than
       the one before, rounded up to a multiple of 8; 200 classes in all.
     - Each block carries an 8-byte header holding its class and the requested size. The returned pointer is
       8-aligned and follows the header.
     - A request takes the most recently freed block of its class; if there is none, a new block carved from
       the region in order, after the last one.
     - `HEAP_ZERO_MEMORY` zeroes the block's whole class size.
     - Failure returns null, or raises `STATUS_NO_MEMORY` when the call's own flags include
       `HEAP_GENERATE_EXCEPTIONS` (a placeholder heap's `HeapCreate` options don't count).
   - One lock (a critical section, taken through the system's own functions, not the replaced ones)
     serialises the free lists and the carving.
   - `HeapFree`: a block in the region goes to the head of its class's free list; any other block goes to the
     system's `HeapFree`.
   - `HeapSize`: a block in the region gives its requested size; any other block, the system's answer.
   - `HeapReAlloc`, for a block in the region:
     - If the new size fits the block's class, the block stays and its size is updated, zeroing the growth
       when asked.
     - Otherwise `HEAP_REALLOC_IN_PLACE_ONLY` fails. Without that flag, a new block is allocated, the old
       contents are copied and the old block is freed.
   - `HeapReAlloc`, for a block of a system heap:
     - Without `HEAP_REALLOC_IN_PLACE_ONLY`, the block moves into the region: allocate, copy the smaller of
       the old and new sizes, free with the system's function.
     - With that flag, it goes to the system's function.
   - `HeapCreate` returns a placeholder handle: multiples of 0x10 from 0x10 to 0xfff0, within the first
     64 KiB, where nothing is mapped. Allocations with it are served from the region like all others.
   - Placeholder handles:
     - `HeapDestroy` returns success and keeps its blocks.
     - `HeapValidate` returns success, and also for any block in the region.
     - `HeapWalk` reports one region entry at the region's start, with the region's full size as its size
       and committed size, then no more items.
     - `HeapCompact` returns 1 MiB.
     - A block outside the region passed with a placeholder handle (`HeapFree`, `HeapSize`, `HeapReAlloc`)
       goes to the process heap.
   - Real handles go to the system's functions for all of these. Heap functions not listed
     (`HeapLock`, `HeapUnlock`, `HeapSetInformation`, `HeapQueryInformation`, `GetProcessHeaps`) are not
     replaced.
   - Measured: this rule is what makes the New Game views identical. Without it the runs differ under every
     other combination tried. With it and no DynamoRIO, they differ too, so rule 1 is still needed.
4. **Unchanged:** the frame clock and its steps (the plan's `frame_ms 40`, `spin 1000`, `sleep_advances 1`,
   `first_frame_ms 120000`, `epoch`), the secondary-pass ordering at the frame's zero-length sleep, the
   window kept in front, the configuration file's settings (the plan's `ltx` lines), and the command line
   and steps of `volyacapture full`.

## 3. Confirmation

Runner files rebuilt locally: main's source plus rules 2 and 3, with DynamoRIO 11.92.20777's 32-bit
runtime.

| Variant | Full-run pairs | Result |
|---|---|---|
| Rules 1–4 (detach, heap, ntdll-only client) | 2 (investigator) + 3 (reviewer's independent build) | every pair: both runs exit 0, every view byte-identical (title-menu, bunker-start, caged-lamp, leaves, outdoor, display frames included), events identical |
| DynamoRIO for the whole run, heap, ntdll-only client | 1 | byte-identical |
| Heap and frame clock, no DynamoRIO | 1 | differ: bunker-start about 54,000 pixels, caged-lamp 440–600, outdoor about 23,000; title-menu and leaves identical |
| DynamoRIO for the whole run, ntdll-only client, no heap | 6 | all complete; bunker-start, caged-lamp and outdoor differ, title-menu and leaves identical |

Every identical run, the investigator's and the reviewer's, gave the same hash for each view's frame. The
title-menu check also passed byte-identical with the recipe's build.

## 4. Choices the reviewer made where the text was open

The object attributes and view size (rule 2), the placeholder range, the `HeapWalk` entry's fields, the
process heap for foreign blocks with a placeholder handle, the call's own flags for
`HEAP_GENERATE_EXCEPTIONS`, and leaving the other heap functions unreplaced (rule 3). All are stated above as
the reviewer built them; they reproduced the identical views, and none needed anything learned from the
program.

## 5. Unestablished

- Other machines, graphics drivers and Windows builds, including whether 0x20000000 is free at the proxy's
  start there.
- Whether rule 2 alone fixes the detach failure without rule 3 (the detach was only run with the heap).
- Which counter reads matter once the heap is controlled. Without DynamoRIO the views differ, so something
  it supplies is needed.
- Headroom: about 446 of the region's 896 MiB were used. Longer runs, more views or another level may need a
  larger region.
- Robustness beyond five identical pairs of the exact variant.
