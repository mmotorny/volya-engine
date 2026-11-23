# `.db` / `.xdb` archive format

Functional specification of the game-data archives of S.T.A.L.K.E.R.: Shadow of Chernobyl:
`gamedata.db0` … `gamedata.db9` and `gamedata.dba` … `gamedata.dbd` in the game root, and
`mods/mp_military_2.xdb0`. It covers how to list the files stored in an archive, how to obtain
each file's original bytes, and how several archives combine into one file tree.

Conventions: all integers are little-endian and unsigned, 16 or 32 bits wide as stated.
Offsets are absolute byte offsets from the start of the archive file unless stated otherwise. Hexadecimal byte strings are written as space-separated pairs.

## 1. Archive layout: a sequence of chunks

An archive has no file-level header or trailer. It is a concatenation of chunks, each made of an
8-byte chunk header followed by its payload:

| Offset in chunk | Size | Meaning |
|---|---|---|
| 0 | 4 | chunk id (32-bit). Bit 31 (`0x80000000`) is the *compressed* flag; bits 0–30 are the chunk type. |
| 4 | 4 | payload length in bytes (32-bit), excluding the 8-byte chunk header |
| 8 | length | payload |

The next chunk begins right after the payload; the chunks fill the file exactly.

| Chunk type | Meaning |
|---|---|
| 0 | **Data chunk.** Holds the stored bytes of all files, back to back. Its compressed flag is clear. |
| 1 | **Directory chunk.** Holds the file table (sections 2 and 3). |

The file table is found by walking the chunks from offset 0 and taking the first chunk whose type
(id with bit 31 cleared) is 1. Chunks of other types are skipped.

Observed in all 15 archives of the Russian edition: exactly two chunks — the data chunk first
(id `0x00000000`, so its payload starts at offset 8), then the directory chunk (id `0x80000001`,
type 1 with the compressed flag set), which ends at end of file.

## 2. Decoding the directory chunk

If the directory chunk's compressed flag is **clear**, its payload is the file table as is
(no archive of this edition does this).

If the flag is **set** (every archive of this edition), the payload is decoded in two steps:

1. **Descrambling** (section 4) of the whole payload. This step is skipped for archives whose
   file path contains the substring `.xdb`. All `gamedata.db*` archives are scrambled;
   `mp_military_2.xdb0` is not.
2. **Decompression** with the LZSS/adaptive-Huffman scheme of section 5. The descrambled payload
   starts with a 32-bit integer giving the decoded length; the decoded bytes are the file table.

Descrambling never changes the payload length.

## 3. The file table

The file table is a sequence of variable-length entries that fills it exactly. Each entry is a
16-bit body length `L` followed by an `L`-byte body:

| Offset in body | Size | Field |
|---|---|---|
| 0 | 4 | **original size**: length of the file's contents after any decompression (32-bit) |
| 4 | 4 | **stored size**: number of bytes the file occupies in the archive (32-bit) |
| 8 | 4 | **checksum**: CRC-32 of the stored bytes (32-bit, see below) |
| 12 | `L − 16` | **path**, single-byte characters, not NUL-terminated |
| `L − 4` | 4 | **data offset**: absolute archive offset of the stored bytes (32-bit) |

The path occupies everything between the checksum and the last four bytes, so its length is
`L − 16`.

**Paths.** Relative to the archive's mount directory (section 7), with backslash (`\`) as the
separator and no leading separator. A path ending in `\` names a directory; any other path names a
file.

**Directory entries.** All four numeric fields are 0. They are optional: several archives
(e.g. `gamedata.db1`–`gamedata.db4`) list files without listing any directories. A reader
building a tree must create every ancestor directory of every path itself.

**File entries.** The stored bytes are the `stored size` bytes starting at `data offset`.
- If `stored size` equals `original size`, the stored bytes are the file's contents verbatim.
  This is the case for every file of the Russian edition.
- Otherwise the stored bytes are an LZO1X stream (section 6) that decompresses to
  `original size` bytes.
- File data is never scrambled; scrambling applies only to the directory chunk.

**Checksum.** The standard CRC-32 (reflected polynomial `0xEDB88320`, initial value `0xFFFFFFFF`,
final XOR `0xFFFFFFFF`, identical to zlib's `crc32`) over the `stored size` stored bytes. It
matches for all 28,790 file entries of this edition. Zero-length files have checksum 0.
Verifying it is not required for reading. Whether it covers the stored or the decompressed bytes
of a compressed entry cannot be told from this edition's data, where the two coincide.

**Shared data.** Several entries can carry the same data offset (identical contents stored once);
in this edition such entries always also have identical sizes and checksums. The total of all file
sizes can therefore exceed the archive size (`gamedata.db4`: 890,560,150 bytes of files in a
671,392,842-byte archive). Zero-length files also carry an offset inside the data chunk, often
shared with other zero-length entries; it has no meaning.

**Properties observed in every archive of this edition** (sanity checks, not format
requirements):
- paths are lowercase printable 7-bit ASCII, 3 to 92 characters long;
- no path occurs twice within one archive;
- all directory entries precede all file entries, and each group is in ascending byte order of
  path;
- every file lies inside the data chunk payload, and the distinct stored byte ranges cover that
  payload exactly, with no gaps and no overlaps.

## 4. Descrambling

Descrambling is a byte substitution combined with a pseudo-random keystream. Both use one 32-bit
linear congruential generator: *advancing* a state `s` replaces it with
`(s × 0x08088405 + 1) mod 2³²`, and the *byte drawn* from a state is its top 8 bits
(`s` shifted right by 24).

**Substitution table.** Begin with the identity permutation `P` of 0…255 and state `0x01329436`.
Do 2,048 rounds. In each round, advance and draw byte `a`, then advance and draw byte `b`; while
`b` equals `a`, advance and draw a new `b`. Exchange `P[a]` and `P[b]`. The state carries over
between rounds. The descrambling table `Q` is the inverse permutation (`Q[P[i]] = i`).

**Keystream and output.** For each directory payload, start a fresh state at `0x0131A9D3`. For each
payload byte `c`, in order, advance, draw `k`, and output `Q[c XOR k]`.

Test vectors:
- `P[0…15]`: `be d6 7d 5b c5 76 f6 66 42 33 a2 89 e2 29 cf 79`
- `Q[0…15]`: `15 e8 c6 a7 d3 76 64 94 60 bc 19 2e ed 40 f9 dc`; MD5 of all 256 bytes of `Q`:
  `58efe758d41da298e781286f998e5555`
- first 16 keystream bytes: `88 a4 7d f7 d1 c9 98 bb 13 8f 87 c8 9f d3 9a 92`

Both seeds appear in the Russian edition's game executable and in its dedicated-server
executable. Other editions were not examined.

## 5. Directory decompression (LZSS with adaptive Huffman coding)

The scheme is the well-known public-domain "LZHUF" method: LZSS dictionary coding, with an
adaptive Huffman code for literals and match lengths and a fixed prefix code for the upper bits of
match distances. Parameters are the usual ones except the frequency-rescaling threshold. The
behaviour needed for bit-exact decoding follows.

**Framing.** A 32-bit integer `n` (the decoded length), then a bit stream read from the most significant bit
of each byte downwards. Bits past the end of the input read as 0. Decoding stops as soon as `n`
bytes have been produced (a match may be cut short by this).

**Dictionary.** A 4,096-byte ring, initially all spaces (`0x20`), with the write position starting
at 4,036 (4,096 − 60). Every output byte is also written at the write position, which then
advances modulo 4,096. Matches may reach into the initial spaces.

**Symbols.** 314 symbols: 0–255 are literal bytes; 256–313 are matches of length `symbol − 253`
(3–60).

**Huffman model.** 627 slots numbered 0–626, kept in non-decreasing order of frequency. Each slot
holds a frequency and is either a leaf (holding a symbol) or an internal node (holding a pair of
adjacent child slots `c` and `c + 1`). Slot 626 is the root. Every slot but the root has a parent.
For comparisons, the position just after the root counts as having frequency `0xFFFF`.

- *Initial state:* slots 0–313 are leaves for symbols 0–313 in order, each with frequency 1.
  Slots 314–626 are internal: slot `j` has children `2(j − 314)` and `2(j − 314) + 1` and the sum
  of their frequencies.
- *Reading a symbol:* from the root, while at an internal node with children `c, c + 1`, read a
  bit `b` and move to slot `c + b`. The leaf reached gives the symbol.
- *Adapting after every symbol read* (literal or length):
  1. If the root's frequency is exactly `0x4000`, first rebuild the model (below). (The textbook
     value `0x8000` decodes this game's tables incorrectly; `gamedata.db4` exposes the
     difference.)
  2. Walk from the symbol's leaf up to and including the root. At each slot, add 1 to its
     frequency, giving `f`. If `f` now exceeds the frequency of the following slot, locate the
     last slot `l` in the run of following slots whose frequencies are all below `f`; swap the
     contents (frequency and leaf/child payload) of the current slot and `l`, update the parent of
     whatever each now holds (the leaf, or both children), and continue from `l`. Then go to the
     parent.
- *Rebuilding:* collect the leaves in current slot order into slots 0–313, halving each frequency
  rounding up (`(f + 1) / 2` in integer arithmetic). Then for `j` = 314 … 626, with
  `i = 2(j − 314)`, form an internal node with children `i, i + 1` and frequency `f` equal to the
  sum of theirs, and insert it into the ordered slots 0 … `j − 1` directly after the last slot
  whose frequency is ≤ `f`, moving the later slots up by one. Children already recorded are not
  renumbered by this shift. Finally recompute every parent from the child pairs.

**Literal (symbol < 256):** output the byte.

**Match (symbol ≥ 256):** read the distance first:
1. read 8 bits as a byte `p`;
2. look up `U = UPPER[p]` and `W = WIDTH[p]` in the two fixed tables below;
3. read `W − 2` more bits, shifting each into the low end of `p`;
4. the distance is `D = U × 64 + (p mod 64)`, in 0 … 4,095.

Then copy `length` bytes one at a time from ring position `(write position − D − 1) mod 4096`
onwards; both positions advance modulo 4,096 per byte and every copied byte is output and written
to the ring, so source and destination may overlap.

The two tables are the standard LZHUF distance tables, described as runs over `p` = 0 … 255:
- `UPPER`: 0 for 32 entries; 1, 2 and 3 for 16 entries each; 4–11 for 8 each; 12–23 for 4 each;
  24–47 for 2 each; 48–63 for 1 each. MD5 of the 256 bytes: `d43a9b84d1d221fddd5253ae3c1af3f9`.
- `WIDTH`: 3 for 32 entries, 4 for 48, 5 for 64, 6 for 48, 7 for 48, 8 for 16. MD5 of the 256
  bytes: `c06b2e9b5a1ef82097400f668a13450d`.

In every archive of this edition the decoded length declared in the stream equals the length of
the decoded table, and the table splits into whole entries exactly.

## 6. Compressed file data (not present in this edition)

An entry whose stored size differs from its original size is stored as a raw LZO1X stream (the
public LZO1X format, no header, terminated by the standard LZO1X end-of-stream marker) that
decompresses to `original size` bytes. This was established from the shipped core library only;
no entry of the Russian edition uses it, so it is untested against data. The game refuses to
read such files incrementally (streamed audio and similar), so files meant for streaming must be
stored verbatim.

## 7. Mounting and combining archives into one tree

**Which files are archives.** In the game root directory (scanned without descending into
subdirectories), every file whose last extension begins with `.db` is an archive: `.db0` … `.db9`,
`.dba` … `.dbd`. (`goggame-1207660573.hashdb` does not qualify: its extension is `.hashdb`.)
Separately, files matching `*.xdb*` in the mods directory are archives.

**Mount directory.** A root-directory archive is mounted at the directory whose path is the
archive's path with its last extension removed: `<game root>\gamedata.db0` mounts at
`<game root>\gamedata\`, so all 14 `gamedata.db*` archives share the root `gamedata\`. Every
`*.xdb*` archive is mounted at `<game root>\gamedata\` too; its stored paths (e.g.
`levels\mp_military_2\level.game`) are relative to that root. The path of a file in the combined
tree is the mount directory followed by the stored path.

**Precedence.**
- Paths are compared case-insensitively; the game lowercases all paths.
- Root-directory archives are processed in ascending byte order of their file names:
  `db0, db1, …, db9, dba, dbb, dbc, dbd` (digits sort before letters).
- `*.xdb*` archives are processed after all of the regular game data. The relative order of
  several `*.xdb*` archives was not established (this edition has one).
- When a path is met again, the later archive's entry (archive, data offset, sizes, checksum)
  **replaces** the earlier one. Later archives thus override earlier ones file by file; paths only
  in earlier archives stay visible; nothing is ever removed.
- Outside the archive format: after the root-directory archives, the game also indexes loose files
  in the data directories named by its path configuration file `fsgame.ltx` (here under
  `<game root>\gamedata\`, which this install does not have), and these override archive entries by
  the same rule. This was established by static analysis only.

The path configuration that this edition's installer writes does not name a mods directory, so
whether the unmodified install actually loads `mods/mp_military_2.xdb0` was not established; an
implementer that loads it should apply it last, as above.

Evidence from the Russian edition:
- 28,790 file entries and 2,915 directory entries in total, covering 25,707 distinct file paths.
  2,031 paths occur in more than one archive (996 in two, 1,023 in three, 7 in four, 5 in five);
  187 of these differ in content between archives.
- `config\system.ltx` exists in `db0`, `dbb` and `dbc`. The `dbc` copy (the one that wins) differs
  from the `dbb` copy in a single line, `current_server_entity_version = 7` instead of `= 6`,
  consistent with later archives being later patches.
- `textures\map\map_mp_military_2.dds` (5,592,560 bytes) is in both `gamedata.db6` and
  `mp_military_2.xdb0`, with identical content.

## 8. Test vectors (Russian GOG edition 2.1.0.10)

### Archives

| Archive | Size | MD5 | Directory payload: stored → decoded bytes | Directories / files |
|---|---|---|---|---|
| gamedata.db0 | 678,577,379 | f5d5cd5465ba6425951c95ed98363abd | 44,920 → 219,261 | 2,379 / 1,195 |
| gamedata.db1 | 708,434,331 | 54377855a148e652c63fcb17f91ad324 | 7,264 → 21,772 | 0 / 413 |
| gamedata.db2 | 671,169,415 | 4d2b7f30fc25d206e742e142c220ca4b | 3,651 → 10,402 | 0 / 200 |
| gamedata.db3 | 684,792,231 | c7dee0acaf23b336b02f58b19128e61c | 5,468 → 17,269 | 0 / 300 |
| gamedata.db4 | 671,392,842 | f65088b47f3ff704d03c190e3c58461f | 299,340 → 1,207,547 | 0 / 14,800 |
| gamedata.db5 | 696,277,331 | e9bec4aceefcce9782eb18cb24806e05 | 68,441 → 233,224 | 292 / 3,520 |
| gamedata.db6 | 667,613,134 | 9b845995d1551620cc1685b024a3b969 | 3,425 → 9,340 | 7 / 170 |
| gamedata.db7 | 672,770,451 | 05b59242e2204cd30937bba3eba40a9c | 42,196 → 135,125 | 17 / 2,570 |
| gamedata.db8 | 296,000,567 | 695aae125fdd783ba0abe6192336a840 | 16,947 → 51,611 | 8 / 1,035 |
| gamedata.db9 | 79,312,307 | 14d8ba99f592e8561dc5c40322b6d6a4 | 31,388 → 125,094 | 96 / 1,473 |
| gamedata.dba | 32,500,627 | 6199871098b8f5c76a4964a0e76ece75 | 31,867 → 74,506 | 42 / 1,526 |
| gamedata.dbb | 55,194,918 | 43b5002ed9224b3052c3466dbab09c64 | 31,983 → 77,466 | 65 / 1,547 |
| gamedata.dbc | 160,948 | f123823ace87152638e746cfed5c8d6e | 323 → 573 | 3 / 11 |
| gamedata.dbd | 101,014 | 9bb19579fb9e7fade0251070165540af | 210 → 438 | 5 / 6 |
| mods/mp_military_2.xdb0 | 50,350,518 | 3c81bfe2c343c9b9a1e1c008b602df89 | 531 → 1,347 | 1 / 24 |

In each archive the directory chunk header directly follows the data chunk payload, e.g. at offset
100,796 in `gamedata.dbd` (data payload 100,788 bytes) and at 50,349,979 in `mp_military_2.xdb0`.

### Directory decoding

`gamedata.dbd`, directory payload of 210 bytes:
- stored payload MD5 `ad998b60d8ec5ae31fd0b977a75a7a11`;
- descrambled MD5 `163b793c4c2d1c540bc3af1026c126be`, starting
  `b6 01 00 00 d1 e3 25 40 1e ff df eb e5 f5 f9 fa` (declared decoded length `0x1B6` = 438);
- decoded table: 438 bytes, MD5 `f62474358e825f9069188a2ea0631b5e`. Its first entry has `L` = 23,
  four zero fields and the path `config\`.

| Archive | Descrambled payload MD5 | Decoded table MD5 |
|---|---|---|
| gamedata.db0 | f0a101fa1a7baeb1a0ef99a9e6b1552e | 50578d01b03df9ee7262f352ab80a261 |
| gamedata.db4 (many model rebuilds) | 8f0fc97ced5f6e1a29e241565b6f5d56 | a1494e398cc20ae8a33139b32e2c2bc5 |
| mp_military_2.xdb0 (not scrambled; payload starts `43 05 00 00`) | 9b51991978a5e427d73d4e18f6a7a212 | 7c38b0ab4ad2d4c88de3ccf920498c52 |

### Complete listing of `gamedata.dbd`

Directories: `config\`, `config\mp\`, `config\mp\weapons_mp\`, `config\ui\`, `scripts\`.

| Path | Data offset | Size | CRC-32 | MD5 |
|---|---|---|---|---|
| config\mp\weapons_mp\weapons_mp.ltx | 8 | 57,567 | a5c60df5 | 0e95eaf004e8652ca9dba98aeeee5621 |
| config\ui\ui_mm_mp_tabclient.xml | 57,575 | 7,311 | ce9244d4 | 66503c06703e143a98aff6640a9c566f |
| config\ui\ui_mm_mp_taboptions.xml | 64,886 | 10,286 | f1855d14 | 00c9b9ebd997f05538fdc291381f0a14 |
| scripts\ui_mm_mp_join.script | 75,172 | 2,811 | 720eb5bd | 2959c6c8f154acc1be3de9dcfb16b26d |
| scripts\ui_mm_mp_options.script | 77,983 | 6,567 | 6efd782e | 8ac118a2e100329bc4bee724c145e8c3 |
| scripts\ui_mp_main.script | 84,550 | 16,246 | 36410070 | 1aaac179eb3bd60c639d4f35218723e8 |

### Selected files from other archives

| Archive | Path | Data offset | Size | CRC-32 | MD5 |
|---|---|---|---|---|---|
| gamedata.db0 | config\system.ltx | 4,130,936 | 22,829 | eaff4f19 | c29b88beb9b146237e6d6c1ce14d0574 |
| gamedata.dbb | config\system.ltx | 3,603,573 | 22,829 | 7a606873 | 31c4df515eadc2d09b456ad622a20809 |
| gamedata.dbc | config\system.ltx | 8 | 22,829 | 0a157555 | 3c35ee63942d827a970e2ddf6096cb40 |
| gamedata.db0 | config\creatures\actor.ltx | 319,696 | 11,293 | 934bf7a4 | 83d22db80a1bf872a1f681e2065e4499 |
| gamedata.dbb | config\creatures\actor.ltx | 17,186 | 11,359 | 92537db7 | c35d9476c2d399f493158169168857a5 |
| gamedata.db4 | scripts\_g.script | 447,148,550 | 39,542 | c32cb361 | c7562102faf72dc95222be7f913533df |
| gamedata.db7 | textures\ui\ui_icon_equipment.dds | 667,069,639 | 2,097,280 | 8dc50bd0 | eb05cc55d1ce2dc8cd75719422eee42b |
| gamedata.db9 | config\text\rus\string_table_general.xml | 840,653 | 5,741 | 81355eda | e58ec94e298a55a0cbfef477b9bd1c3b |
| gamedata.dbb | config\text\rus\string_table_general.xml | 4,449,421 | 5,733 | 00a6e9e7 | ea3d5d35bb518eefa32a520f205a49eb |
| mp_military_2.xdb0 | levels\mp_military_2\level.game | 8,108,687 | 1,960 | 37be42c9 | b3f3577f8ecd502d69f7fb93e182fb77 |

In the combined tree, `config\system.ltx` resolves to the `gamedata.dbc` copy,
`config\creatures\actor.ltx` and `config\text\rus\string_table_general.xml` to the `gamedata.dbb`
copies (`actor.ltx` is also in `gamedata.dba`, identical to the `db0` copy), and
`scripts\_g.script` to the `gamedata.dbb` copy (identical in `db4`, `dba` and `dbb`).

### Content sanity checks

Over all extracted files: all 16,308 `.ogg` and all 39 `.ogm` files begin with `OggS`, all 5,602
`.dds` files begin with `DDS `, and the 3,749 `.ltx`, `.xml` and `.script` files are text
(29 of them contain a few stray control bytes, such as a single NUL, in the original data).

## Provenance

- **Date:** 2026-11-23.
- **Edition:** S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian GOG installer 2.1.0.10, unpacked with
  the project's own installer extraction tool.
- **Data examined:** all 14 `gamedata.db*` archives and `mods/mp_military_2.xdb0` (sizes and MD5s
  above); the installer's script that writes `fsgame.ltx`; the shipped 32-bit Windows binaries
  `bin/XR_3DA.exe`, `bin/dedicated/XR_3DA.exe` and `bin/xrCore.dll`.
- **Analysis performed:** inspection of the archive bytes; static analysis of the shipped binaries
  with Ghidra 12.2 (headless disassembly and decompilation) to establish the chunk layout, entry
  fields, scrambling seeds and procedure, the rescaling threshold, the handling of compressed file
  data, mount directories and precedence; byte searches of the binaries for the seeds. The binaries
  were not executed. Every data-related statement was then checked with an independently written
  scratch decoder, whose distance tables were built from the run descriptions above rather than
  read from the binaries, that listed and extracted every file of all 15 archives: all directory
  tables decoded to their declared lengths and split into whole entries, all 28,790 checksums
  matched, the stored ranges cover each data chunk exactly, and file contents have the expected
  signatures. The LZO1X case (section 6), the loose-file rule and the mods-directory location
  (section 7) are not exercised by data.
- **Sources not used:** no leaked or derived engine source code, no third-party reimplementations
  and no third-party documentation of this game's formats. The only external references are the
  public descriptions of the generic LZHUF, LZO1X and CRC-32 algorithms.
