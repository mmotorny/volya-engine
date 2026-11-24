# Chunk container

Functional specification of the generic "chunked" container used by most binary game-data files of
S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): the level files (`level`, `level.geom`,
`level.geomx`, `level.details`, …), the root `*.xr` libraries, `.ogf` models and others (section 8).
It covers the chunk header, ids and flags, nesting, sizes and alignment, compressed chunks, and how
lookups by id behave, including duplicate and missing ids. What the chunks of a particular file
contain is left to that file's own specification.

Conventions: integers are little-endian and unsigned unless stated otherwise. Sizes are in bytes.
Paths are stored paths in the combined game-data tree of the archive specification (`db-archive`),
with `\` separating directories.

## 1. Chunk sequences

A **chunk** is an 8-byte header followed by a payload:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | **id word**: bits 0–30 are the chunk **id**; bit 31 (`0x80000000`) is the **compressed** flag (section 5) |
| 4 | 4 | **size**: number of payload bytes as stored, excluding the 8-byte header |
| 8 | size | payload |

A **chunk sequence** is a byte region (a whole file, or the payload of a chunk) holding chunks back
to back: the next chunk's header starts immediately after the previous payload. There is no
sequence header, no count, no terminator, no padding and no alignment; a payload may have any
length, including 0, and the next header may start at any offset (for example `level` of
`levels\l01_escape\` has its chunk 2 header at offset 4,075,776, payload at 4,075,784).

A well-formed sequence is filled exactly by its chunks. This holds for every chunked file of the
edition (section 8) at the top level. The game itself does not check sizes against the enclosing
region; an implementation should treat a size larger than the bytes remaining, or 1–7 bytes left
over after the last chunk, as a malformed file.

Ids are arbitrary 31-bit numbers chosen by each format (the largest top-level id in the edition is
0x2000, in `level.game`; `.thm` files use 0x810–0x819, `.anm` files 0x1100). Chunk order within a
sequence carries no meaning for lookups by id: `levels\l01_escape\level` stores its chunks in the
order 1, 4, 6, 7, 3, 2, 8.

## 2. Nesting

A payload is either opaque data or itself a chunk sequence (sub-chunks). Nothing in the header
marks which; the format of the enclosing file decides. Sub-chunk sequences follow exactly the same
rules as top-level ones, to any depth (for example `spawns\all.spawn` chunk 1 → sub-chunk 1 →
one sub-chunk per object → its sub-chunk 1 → two further chunks, five levels in all).

A common convention for lists is a sequence of sub-chunks with ids 0, 1, 2, … (for example the
static visuals in `level` chunk 3, sub-chunks 0 … 11,537 for `l01_escape`). The game reads such a
list by looking up id 0, then 1, then 2, and so on, and stops at the first id that is not found
(section 3), so a list ends at its first gap and at its first sub-chunk of size 0.

## 3. Lookup by id

Looking up id `n` in a sequence:

1. Starts at the beginning of the sequence, every time (lookups are independent of earlier reads).
2. Walks the chunks in order and compares bits 0–30 of each id word with `n`; the compressed flag
   is ignored for the comparison.
3. The first matching chunk is the result: its size and payload position, and whether its
   compressed flag is set.
4. If the end of the sequence is reached without a match, the result is "not found", reported as
   size 0.

Consequences:

- **Duplicate ids:** only the first chunk with a given id can be found by id; later ones are
  reachable only by sequential iteration. No top-level sequence of any shipped file contains a
  duplicate id.
- **Missing ids:** "not found". Whether that is an error is up to the reader of each format; for
  example a missing `level.geom` chunk 9 is fatal for the first renderer, while a missing
  chunk 11 is silently skipped.
- **Empty chunks:** a chunk whose size is 0 cannot be told apart from a missing one by a lookup.
  Readers that only need the size (record count = size ÷ record size) get 0 records; readers that
  open the chunk as a sub-sequence get "not found". Empty chunks occur in the data: for example
  `shaders.xr` chunk 0, chunk 0x2000 of `level.game` of the single-player levels, and chunk 4
  (portals) of `levels\l02_garbage\level`, `levels\l12_stancia_2\level` and
  `levels\testers_mp_factory\level`, which therefore have no portals.
- A chunk read as a fixed-size record (for example a 24-byte header) must have exactly that size;
  otherwise the game stops with an assertion failure.

## 4. Sequential iteration

Some readers visit every chunk of a sequence in stored order instead of looking up ids. Iteration
starts at the beginning, yields each chunk's full 32-bit id word (including the compressed flag),
its size and its payload, and continues after the payload. It ends when fewer than 8 bytes remain.
Duplicate ids are all visited; empty chunks are visited too.

## 5. Compressed chunks

If bit 31 of the id word is set, the stored payload (`size` bytes) is compressed. It is decoded
with the LZSS/adaptive-Huffman ("LZHUF") scheme of the archive specification (`db-archive`,
section 5), exactly as the archive directory is decoded there except that there is **no
descrambling step**, for any file:

- the first 4 bytes of the stored payload are the decoded length `n` (32-bit);
- the bit stream that follows decodes to exactly `n` bytes, which are the chunk's content;
- `n` = 0 gives empty content.

The decoded content takes the place of the payload for everything that interprets the chunk (for
example a sub-chunk sequence inside it). `size` in the header always counts the stored, compressed
bytes, so walking a sequence never requires decoding.

Notes:

- In the game, decoding happens when a chunk is opened as a nested sequence or visited by
  iteration. A reader that only looks a chunk up and copies its payload directly receives the
  stored bytes. No shipped file relies on this; an implementation should always decode.
- Files that the game reads as streams instead of loading them whole cannot contain compressed
  chunks: opening a compressed chunk in a stream is an assertion failure. This applies to
  `level.geom` and `level.geomx`.
- When the game's own writer produces a chunk whose requested id has bit 31 set, it stores the
  payload compressed in this form; the flag is therefore part of the stored id word.

In the whole edition (all 14 `gamedata.db*` archives and `mods\mp_military_2.xdb0`) exactly one
chunk is compressed: in `textures\map\map_mp_military_2.thm` (`mp_military_2.xdb0`), the second
top-level chunk, id word `0x80000811` (id 0x811), stored size 45,965. Its first 4 bytes are
`00 00 01 00` (decoded length 65,536) and it decodes to 65,536 bytes with MD5
`ecb320c95e2e6845d0069d670567584e`. The file's other nine top-level chunks (ids 0x810 and
0x812–0x819) are uncompressed; the file is 46,111 bytes.

## 6. Reading algorithm (summary)

To read a chunked file: treat the file as one chunk sequence; walk it from offset 0 reading
8-byte headers (`id = word & 0x7FFFFFFF`, `compressed = word & 0x80000000`), validating each size
against the remaining bytes; record the first chunk per id; decode compressed payloads when their
content is needed. Repeat on a payload whenever the file's format says that payload is a chunk
sequence.

## 7. Non-chunked files

Not every binary file is chunked. In particular `level.cform`, `level.ai`, `level.graph`,
`game.graph`, `shaders_xrlc.xr`, `.dds`, `.ogg`/`.ogm` and all text files (`.ltx`, `.xml`,
`.script`, shader sources) do not use this container.

## 8. Files that use it

A file was counted as chunked when its whole contents parse as a top-level chunk sequence that
fills it exactly with all ids below 0x10000; every file of the listed kinds in the edition
(counting each archive's copy separately) passes, and so does no file of the kinds in section 7:

| File kind | Files | Top-level chunks | Notes |
|---|---|---|---|
| `levels\<level>\level` | 31 | 7 each | three have an empty chunk 4 |
| `level.geom`, `level.geomx` | 31 each | 4 each | |
| `level.details` | 24 | 72 | |
| `level.hom` | 26 | 52 | |
| `level.env_mod` (and one `level.env_mod_`) | 15 (+1) | 38 (+3) | |
| `level.ps_static` | 12 | 133 | |
| `level.snd_static` | 14 | 123 | |
| `level.snd_env` | 5 | 10 | |
| `level.som` | 8 | 16 | |
| `level.wallmarks` | 17 | 17 | |
| `level.game` | 39 | 78 | every one has an empty chunk |
| `level.gct` | 36 | 72 | |
| `level.spawn`, `spawns\*.spawn` | 34 | 12,386 | |
| `*.xr` except `shaders_xrlc.xr` (`shaders.xr`, `gamemtl.xr`, `particles.xr`, `lanims.xr`, `senvironment.xr`, …) | 10 | 48 | `shaders.xr` has an empty chunk 0 |
| `.ogf` (models) | 1,581 | 8,842 | |
| `.omf` (motions; and one `.omf-`) | 31 (+1) | 62 (+2) | |
| `.anm` (animations) | 92 | 92 | |
| `.thm` (texture descriptions) | 4 | 37 | the only compressed chunk (section 5) |

## 9. Test vectors

All from the Russian GOG edition; files are taken from the combined tree (later archive wins)
unless an archive is named.

- `levels\l01_escape\level.geom` (56,995,500 bytes): top-level chunks (id, payload offset, size)
  (1, 8, 4), (9, 20, 37,295,312), (10, 37,295,340, 19,330,928), (11, 56,626,276, 369,224); the
  last payload ends exactly at the end of the file.
- `levels\l01_escape\level.geomx` (13,401,518 bytes): chunks 1 (4), 9 (5,673,404),
  10 (7,728,074), 11 (4), in that order.
- `levels\l01_escape\level` (4,094,604 bytes): chunk ids in stored order 1, 4, 6, 7, 3, 2, 8 with
  sizes 4, 5,520, 4,428, 396, 4,065,388, 17,864, 948.
- `shaders.xr` (`gamedata.db4` and `gamedata.dba`): chunks 0 (0 bytes), 1 (3,277), 2 (155,525),
  3 (3,531).
- `levels\l02_garbage\level`: chunk order 1, 4, 6, 7, 3, 2, 8 with chunk 4 of size 0.
- `levels\testers_mp_agroprom\level.game`: chunks 0x1000 (size 0) and 0x2000 (3,456).
- The compressed chunk of section 5.

## Provenance

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): archives
  `gamedata.db0` … `gamedata.dbd`, `mods\mp_military_2.xdb0`; binaries `bin\xrCore.dll`,
  `bin\xrRender_R1.dll`, `bin\xrRender_R2.dll`, `bin\XR_3DA.exe`.
- Data examined: every file entry of all 15 archives, walked as chunk sequences (top level, and
  recursively where payloads parse as sequences); counts of chunked files, empty chunks,
  duplicate ids and compressed flags; the one compressed chunk decoded with the archive
  specification's decoder and its result checked (decoded length equals the declared length, 64 KiB
  of pixel-like data).
- Binaries: static analysis (Ghidra 12.2, headless) of the core library's chunk reading and
  writing (lookup, iteration, opening nested sequences, the stream reader's restriction,
  compressed-chunk decoding and encoding) and of the renderers' use of it for `level.geom`
  and `level.geomx`. No execution or debugging.
- A throwaway parser written from this text walked every file listed in section 8 and decoded the
  compressed chunk to the stated MD5.
- No leaked source code, derived projects or third-party documentation of the engine were used.
  The compression scheme is the public LZHUF method as already specified in `db-archive`.
