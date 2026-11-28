# Script API I: data streams

What the data-stream names of the game scripts' interface do in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): the class `net_packet` (a packet that scripts write to and read from), the class `reader` (a read-only
stream over a block of bytes), and the class `ClientID` that packets carry; their constructors and every read and
write, the values they store and give back, and what happens at the edges. This is spec I of the approved scoping
specification `docs/behavior/script-api-scope.md` (section 6, order 15), which assigns these names to it; its scope
rule applies: this spec owns every name of these classes as the binary registers them, listed or not, and answers the
scoping spec's gap 2 (properties the listing lacks) for them. The worked example (section 8) is the beginners'
village campfire on `l01_escape`: what the scripted state readers of a stalker and of the camp's smart terrain read
from their `spawns\all.spawn` records on a New Game.

Builds on (not repeated here):

- `docs/behavior/script-environment.md` (the runtime, LuaJIT 1.0.3; one number type, the 64-bit double),
  `docs/behavior/script-loading.md` (the 250 loadable scripts, its section 3).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (cited as **A**): how a call
  reaches an engine method, base-class calls (its 6.1), and that a failed match has no message (its 7.3).
  Statements that rest on what happens when no form of a method matches say **depends on A**.
- Spec E, `docs/behavior/script-api-value-types-and-math.md` (cited as **E**): `vector` and `matrix` (its 3, 4),
  **f32**, the argument conversions "To f32" and "To flags" (its 2.2), the x87 mode and the rule for X[…] (its 2.5).
- Spec J, `docs/behavior/script-api-simulation-objects.md` (cited as **J**): when the engine calls `STATE_Read`,
  `STATE_Write`, `UPDATE_Read`, `UPDATE_Write` with a packet (its 5.1) and the scripted simulation classes (its 7).
- Spec L, `docs/behavior/script-api-object-binders.md` (cited as **L**): the binder methods `save(packet)`,
  `load(reader)`, `net_import`, `net_export` (its 3) and when they are called.
- Spec F, `docs/behavior/script-api-configuration-and-files.md` (cited as **F**): the file system's `r_open`, which
  returns a `reader` (its 7), and the configuration readers whose names (`r_u32`, `r_float`, `r_bool`, …) these
  classes share.
- `docs/formats/sav.md` (cited as **sav**): the spawn packet (7.2), the scripted state additions and the script time
  (7.5), the spawn file's object list (10.1).
- `docs/behavior/smart-terrain-jobs.md` (its section 6: `esc_lager` and its campfire job).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the core
  library" `bin\xrCore.dll` (856,064 bytes, MD5 `eab71f90f01575eca3500e623873e57c`), "the runtime library"
  `bin\xrLUA.dll` (290,816 bytes, MD5 `a65ba53867962afc92e0f17409e9cd1f`), which also holds the binding library that
  exports the engine interface to scripts [DS-1].
- **The listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established
  from the game library's registration code.
- `p` is a `net_packet`, `r` a `reader`. A **number** is the runtime's double; a **text** a Lua string;
  **f32** as in E. Integers are little-endian; `u8`/`u16`/`u24`/`u32`/`u64` are unsigned of 8, 16, 24, 32, 64 bits,
  `s8`/`s16`/`s32`/`s64` signed (two's complement).
- ⌊x⌋ is the floor, ⌈x⌉ the ceiling; "the low n bits" of an integer is its value modulo 2ⁿ.
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (read from
  game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag [DS-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-data-streams/run.py` (`python3 -I run.py`): it reads the binaries, the evidence of read-only
headless static analysis of them (listed in `runs.py` beside it, outputs in `bin/`), the listing, the shipped
scripts and `spawns\all.spawn`. All checks pass (`ALL PASS 46`, output in `run.out`).

## 1. What is registered (static)

The listing gives this spec 81 items: the classes `net_packet` (a constructor and 63 functions with 53 distinct
names), `reader` (no constructor, 32 functions, 23 names) and `ClientID` (a constructor, `value`, `set`, and the
operator `==`, which the scoping spec does not count) [DS-2]. The game library registers exactly these functions
under these names, one constructor for `net_packet` and one for `ClientID`, none for `reader`, and no property for
any of the three classes; so there is no property the listing lacks (the scoping spec's gap 2) [DS-3]. The scoping
spec's one candidate string for this spec, `__eq`, is a registration: `ClientID`'s equality operator [DS-3].

**Two forms.** Ten names of `net_packet` (`r_float`, `r_u64`, `r_s64`, `r_u32`, `r_s32`, `r_u24`, `r_u16`, `r_s16`,
`r_u8`, `r_s8`) and nine of `reader` (the same without `r_u24`) are registered twice: once taking no argument and
giving the value read (the **value form**), and once taking one argument that the listing shows as `number&` (the
**reference form**). Every other name has one form [DS-4].

| `net_packet` methods | Section |
|---|---|
| `w_begin(type)`, `w_tell()`, `w_chunk_open8(n)`, `w_chunk_close8(pos)`, `w_chunk_open16(n)`, `w_chunk_close16(pos)` | 3.4 |
| `w_u8`, `w_u16`, `w_u24`, `w_u32`, `w_s16`, `w_s32`, `w_u64`, `w_s64`, `w_float`, `w_bool`, `w_stringZ`, `w_vec3`, `w_matrix`, `w_clientID` (one argument each) | 3.2 |
| `w_float_q8(v, min, max)`, `w_float_q16(v, min, max)`, `w_angle8(a)`, `w_angle16(a)`, `w_dir(v)`, `w_sdir(v)` | 3.5, 3.6 |
| `r_begin(n)`, `r_tell()`, `r_seek(pos)`, `r_advance(n)`, `r_elapsed()`, `r_eof()` | 3.4 |
| `r_u8`, `r_u16`, `r_u24`, `r_u32`, `r_s8`, `r_s16`, `r_s32`, `r_u64`, `r_s64`, `r_float` (two forms each), `r_bool()`, `r_stringZ()`, `r_clientID()`, `r_vec3(v)`, `r_matrix(m)` | 3.3 |
| `r_float_q8(n, min, max)`, `r_float_q16(n, min, max)`, `r_angle8(n)`, `r_angle16(n)`, `r_dir(v)`, `r_sdir(v)` | 3.5, 3.6 |

| `reader` methods | Section |
|---|---|
| `r_tell()`, `r_seek(pos)`, `r_advance(n)`, `r_elapsed()`, `r_eof()` | 4 |
| `r_u8`, `r_u16`, `r_u32`, `r_s8`, `r_s16`, `r_s32`, `r_u64`, `r_s64`, `r_float` (two forms each), `r_bool()`, `r_stringZ()`, `r_vec3(v)`, `r_dir(v)`, `r_sdir(v)` | 4 |
| `r_float_q8(min, max)`, `r_float_q16(min, max)`, `r_angle8()`, `r_angle16()` | 4 |

## 2. Calls

### 2.1 Which form runs (static)

- A method with **one form** is called without any check of its arguments: neither their number nor their types are
  compared with the registration [DS-5]. A missing argument converts as no value would (a number argument as 0), extra ones are ignored,
  and each argument goes through its conversion (2.2) whatever its type. Which object the method is called on is not
  checked either; a call on a value that is not a `net_packet` (`reader`, `ClientID`) is unestablished.
- A method with **two forms** is resolved by the number of values in the call, the object included: the value form
  takes exactly one (`p:r_u8()`), the reference form exactly two (`p:r_u8(x)`) [DS-5]. The chosen form's argument
  check then applies.
- A **reference form** accepts only an object of a registered class of that number type; no class of any integer or
  float type is registered (none of the game library's 305 class registrations is for one), so a call `p:r_u8(x)`
  matches no form whatever `x` is (a number does not match, nor nil) [DS-6]. What follows a call with no matching
  form **depends on A** (believed to fail: A 7.3 gives no message for it).

### 2.2 Argument conversions (static)

Each argument of a number type is converted from whatever value is passed, by the runtime's conversion of a value
to a number (as Lua 5.1's documented `lua_tonumber`: a number, or a text that reads as one; anything else gives 0),
then [DS-7]:

| Conversion | Used by | Result |
|---|---|---|
| **To u32** | `w_u32`, `w_u24`, `w_chunk_close8`, `w_chunk_close16`, `r_seek`, `r_advance` of `net_packet`; `ClientID:set` | truncated toward zero to a 64-bit integer, the low 32 bits used; NaN or a magnitude of 2⁶³ or more gives 0 (as E's "To flags" for `flags32`) |
| **To u16** | `w_u16`, `w_begin` | truncated toward zero to a 32-bit integer, the low 16 bits used; NaN or a value outside [−2³¹, 2³¹) gives 0 (as E's "To flags" for `flags16`) |
| **To u8** | `w_u8` | truncated toward zero to a 32-bit integer, the low 8 bits used; NaN or a value outside [−2³¹, 2³¹) gives 0 |
| **To s32** | `w_s32`, `w_s16` (its low 16 bits), `r_seek` and `r_advance` of `reader` | truncated toward zero; when the run-time library's flag for the SSE2 conversion is set (believed: on every processor with SSE2), to a 32-bit integer, NaN or a value outside [−2³¹, 2³¹) giving −2³¹ (processor documentation); otherwise to a 64-bit integer whose low 32 bits are used |
| **To f32** | `w_float`, the three numbers of `w_float_q8`/`w_float_q16`, `w_angle8`/`w_angle16`, the two of `reader`'s `r_float_q8`/`r_float_q16` | E's "To f32" |

Other arguments:

- **Boolean** (`w_bool`): the runtime's truth value: nil and false are false, every other value true [DS-7].
- **Text** (`w_stringZ`): the runtime's conversion of a value to a text (as Lua 5.1's documented `lua_tolstring`: a
  text, or a number converted to its text); for any other value there is no text, and the write then reads through
  absent storage (believed to fail with an access violation) [DS-7], [DS-9]. A text is used up to its first zero
  byte.
- **Objects** (`vector` for `w_vec3`, `w_dir`, `w_sdir`, `r_dir`, `r_sdir` and `reader`'s `r_vec3`; `matrix` for
  `w_matrix`, `r_matrix`; `ClientID` for `w_clientID`): the object passed is used (read, or filled). A value that is
  not an object, or nil, is converted without a check of its type (2.1) and is believed to fail with an access
  violation [DS-6], [DS-8].
- **64-bit integers** (`w_u64`, `w_s64`): the value is taken as an object of a 64-bit integer class [DS-6]; no such
  class is registered, so every call is believed to fail with an access violation. No shipped script uses them
  [DS-15].

### 2.3 Results (static)

| Read | Result |
|---|---|
| `r_u8`, `r_u16`, `r_u24`, `r_u32` | a number, the stored bits as an unsigned integer: [0, 255], [0, 65535], [0, 2²⁴), [0, 2³²) |
| `r_s8`, `r_s16`, `r_s32` | a number, the stored bits as a signed integer: [−128, 127], [−32768, 32767], [−2³¹, 2³¹) |
| `r_float`, `reader`'s `r_float_q8`, `r_float_q16`, `r_angle8`, `r_angle16` | a number, an f32 value exactly |
| `r_bool` | a boolean |
| `r_stringZ` | a text |
| `r_clientID` | a new `ClientID` |
| `net_packet:r_vec3` | a new `vector` (3.3) |
| `r_u64`, `r_s64` (value forms) | an object of a 64-bit integer class; none is registered, so the call is believed to fail with an access violation after the 8 bytes are read |
| `net_packet` `w_tell`, `r_tell`, `r_elapsed` | a number in [0, 2³²): the position or difference as an unsigned 32-bit integer |
| `reader` `r_tell`, `r_elapsed` | a number, as a signed 32-bit integer |
| `r_eof` | a boolean |

Every other method gives nothing [DS-8], [DS-14].

## 3. `net_packet`

### 3.1 The packet (static)

A packet holds a **buffer** of 8,192 bytes, a **write position** and a **read position** (both `u32`, byte
positions in the buffer), and a further `u32` that scripts cannot reach (it is the value `r_begin` returns to the
engine, believed a receive time) [DS-9]. In the object the two positions directly follow the buffer.

- `net_packet()` makes a new packet with every byte of the buffer 0 and both positions 0 [DS-9]. It belongs to the
  script (believed: the binding library's ownership of an object its constructor made).
- The packets the engine passes to scripts (to `STATE_Read`, `STATE_Write`, `UPDATE_Read`, `UPDATE_Write` of
  simulation objects, J 5.1; to the binder methods `save`, `net_import`, `net_export`, L) are the engine's own
  packets, which the script uses in place: its writes go into that packet and its reads advance that packet's read
  position (believed: they are passed as references; that the script must not keep them after the call is believed).
- **No read or write checks a position.** A write stores at the write position and advances it; a read takes from
  the read position and advances it; nothing compares either with the other, with the buffer's size or with the
  other's end [DS-9]. Reading past the write position gives the bytes the buffer holds there (0 in a new packet that
  was never written there). A write or read that reaches past the 8,192 bytes uses the positions themselves and
  then memory outside the packet (believed to corrupt the packet or other memory).

### 3.2 Writes (static, [DS-9])

Each write stores its bytes at the write position, then advances it by their number:

| Method | Stores | Bytes |
|---|---|---|
| `w_u8(n)` | n To u8 | 1 |
| `w_u16(n)`, `w_s16(n)` | n To u16; n To s32, low 16 bits | 2 |
| `w_u24(n)` | the low 24 bits of n To u32 | 3 |
| `w_u32(n)`, `w_s32(n)` | n To u32; n To s32 | 4 |
| `w_float(x)` | x To f32 | 4 |
| `w_bool(b)` | 1 if b is true, else 0 | 1 |
| `w_stringZ(t)` | the text's bytes up to its first zero byte, then a zero byte | length + 1 |
| `w_vec3(v)` | v.x, v.y, v.z as f32 | 12 |
| `w_matrix(m)` | E's matrix rows 0, 1, 2, 3, each as its columns 0–2 (elements 0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14); column 3 is not stored | 48 |
| `w_clientID(id)` | the id's value as `u32` | 4 |
| `w_u64(n)`, `w_s64(n)` | 8 bytes, but no call reaches them (2.2) | 8 |
| `w_float_q8`, `w_float_q16`, `w_angle8`, `w_angle16` | section 3.5 | 1, 2, 1, 2 |
| `w_dir(v)`, `w_sdir(v)` | section 3.6 | 2, 6 |

### 3.3 Reads (static, [DS-8], [DS-9])

Each read takes its bytes at the read position, then advances it by their number:

| Method | Reads | Bytes |
|---|---|---|
| `r_u8()`, `r_s8()`; `r_u16()`, `r_s16()`; `r_u24()`; `r_u32()`, `r_s32()`; `r_float()` | the value (2.3) | 1; 2; 3; 4; 4 |
| `r_u64()`, `r_s64()` | 8 bytes, then fail (2.3) | 8 |
| `r_bool()` | true if the byte is not 0 | 1 |
| `r_stringZ()` | the bytes from the read position up to the first zero byte, as a text (the empty text if the first byte is 0); the read position advances past the zero byte. Nothing limits the search to the written part or to the buffer | length + 1 |
| `r_clientID()` | a `u32`, as a new `ClientID` | 4 |
| `r_vec3(v)` | three f32 as a **new** `vector`, which is the result; the argument `v` is neither checked nor changed | 12 |
| `r_matrix(m)` | fills `m`: elements 0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14 from twelve f32 in that order, elements 3, 7, 11 = 0, element 15 = 1 | 48 |
| `r_dir(v)`, `r_sdir(v)` | fill `v` (section 3.6) | 2, 6 |
| reference forms, `r_float_q8`, `r_float_q16`, `r_angle8`, `r_angle16`, `r_begin` | below | — |

`r_vec3` has one form that takes one argument besides the packet; as a one-form method it is called whatever is
passed (2.1), so `p:r_vec3(v)` and `p:r_vec3()` both read 12 bytes and give the new vector [DS-5], [DS-8]. A script
that passes a vector to be filled gets it unchanged.

`r_float_q8(n, min, max)`, `r_float_q16(n, min, max)`, `r_angle8(n)`, `r_angle16(n)` and `r_begin(n)` of
`net_packet` have one form each, whose first argument is a reference to a number, converted as an object of that
number type (2.1) [DS-6]; the conversion of any value a script can pass gives no storage, and the read is believed
to fail with an access violation when it stores its result. These methods are therefore unusable from scripts on a
packet; `reader` has value forms of the quantised and angle reads (section 4). No shipped script uses them [DS-15].
Section 3.5 gives what they store, for the record.

### 3.4 Positions and chunks (static, [DS-9])

- `w_begin(type)`: the write position becomes 0, then `type` To u16 is written (2 bytes).
- `w_tell()`: the write position. `r_tell()`: the read position. `r_seek(pos)`: the read position becomes `pos` To
  u32. `r_advance(n)`: the read position increases by `n` To u32, modulo 2³² (so a negative `n` moves it back).
- `r_elapsed()`: write position − read position, modulo 2³² (2.3: a read position past the write position gives a
  number near 2³²).
- `r_eof()`: true when the write position is at most the read position (compared as unsigned integers).
- `r_begin(n)`: for the engine, the read position becomes 0 and the first two bytes are read; unusable from scripts
  (3.3).
- `w_chunk_close8(pos)`: stores at byte `pos` (To u32) the low 8 bits of (write position − pos − 1); the write
  position does not change. `w_chunk_close16(pos)`: stores at bytes `pos`, `pos` + 1 the `u16` (write position − pos
  − 2), low 16 bits. Neither checks `pos`.
- `w_chunk_open8(n)`, `w_chunk_open16(n)`: write a 0 byte (two 0 bytes) and give the position where they were
  written back through a reference to a number, which, as in 3.3, no script can pass: believed to fail with an access
  violation when the position is stored. With `w_chunk_close8`/`16` they would mark the length of the data written
  after them [DS-6], [DS-9].

### 3.5 Quantised values and angles (static, [DS-10], [DS-11])

**The packet's floor.** The quantised writers turn an f32 into an integer by a bit-level computation that gives
⌊f⌋ for every finite f with |f| < 2³¹, except f = −0, which gives −1. Outside that range it gives, for example:
2³¹ → −2³¹; 3·10⁹ → 3·10⁹ − 2³²; 2³² → 1; 10¹⁰ → 2; −3·10⁹ → 1,294,967,296; +∞ → 1; −∞ → −1; NaN → 1 [DS-10]. (For
f in [2³¹, 2³²) the result is ⌊f⌋ − 2³².) Only its low 16 or low 8 bits are stored.

- `w_float_q16(v, min, max)`: with v, min, max To f32, f = ((v − min) / (max − min)) × 65535 + 0.5, each of the four
  operations an f32 operation (rounded to nearest), in that order; stores the low 16 bits of the packet's floor of f
  as a `u16` (2 bytes). `w_float_q8` is the same with 255, storing the low 8 bits (1 byte) [DS-11]. So a v in [min,
  max] gives the nearest of 0 … 65535 (0 … 255) steps; outside, the integer wraps modulo 65536 (256): a v whose f
  reaches 65536 stores 0, one whose f is below 0 stores 65535 or less. With min = max the quotient is +∞, −∞ or NaN
  (v above, below or equal to min), stored as 1, 65535 (255) and 1.
- `w_angle16(a)`, `w_angle8(a)`: with a To f32, if a < 0 or a > 2π (the f32 6.2831855) or a is NaN, a is first
  reduced to one turn: t = a × (1/2π) (the f32 0.15915494), i = ⌊t⌋ if t > 0, else ⌈t⌉ (with −0 giving 1), t′ = t − i,
  plus 1 if t′ < 0, and a = t′ × 2π, each an f32 operation; then `w_float_q16(a, 0, 2π)` (`w_float_q8(a, 0, 2π)`)
  [DS-10], [DS-11]. An a in [0, 2π] is stored as it is.

Reading (each an f32 operation in this order, giving an f32; q the stored integer):

| Read | Value |
|---|---|
| `r_float_q16(min, max)` | ((max − min) × q) × (1/65535) + min |
| `r_float_q8(min, max)` | (q × (1/255)) × (max − min) + min |
| `r_angle16()` | (q × 2π) × (1/65535) |
| `r_angle8()` | (q × (1/255)) × 2π |

with 1/65535 and 1/255 the f32 constants 1.5259022·10⁻⁵ (`0x37800080`) and 0.003921567 (`0x3b80807d`)
<!-- spec-lint: constant --> [DS-11]. On a `reader` (section 4) the same formulas are computed by the x87 unit under E's rule for
X[…], the result stored as f32; on a packet (unusable, 3.3) they are SSE f32 operations [DS-11].

### 3.6 Directions (static, [DS-12], [DS-13])

**The direction code** of a vector (x, y, z) is a `u16`:

- bits 15, 14, 13: set when the sign bit of x, y, z respectively is set (so −0 counts as negative);
- with |x|, |y|, |z| the magnitudes, s = (|z| + |y|) + |x| and k = 126 / s (f32 operations), X = the packet's floor
  (3.5) of |x| × k and Y = that of |y| × k; if X ≥ 64, X becomes 127 − X and Y becomes 127 − Y;
- the code is X × 128 + Y plus the sign bits (bits 7–12 hold X, 0–6 hold Y) [DS-12].

So the direction is mapped to the octahedron |x| + |y| + |z| = 126, whose positive face is stored as the integer pair
(X, Y), the half X ≥ 64 folded onto the other. Examples [DS-12]: (0, 0, 1) → 0; (1, 0, 0) → 255; (0, 1, 0) → 126;
(0, 0, −1) → 8192; (1, 1, 1) → 5418; (−1, 2, −3) → 43690; the zero vector (k infinite, each product NaN) → 129.

**Decoding** a code c: X = bits 7–12, Y = bits 0–6; if X + Y ≥ 127, X becomes 127 − X and Y becomes 127 − Y; Z = 126 −
X − Y; the vector is (X·t, Y·t, Z·t) with t = T[c modulo 8192], each product an f32 operation, then each component
whose sign bit is set in c gets its sign bit set (a 0 becoming −0) [DS-12]. The table T has 8,192 entries, built
once at the core library's start-up: entry i, with X = ⌊i/128⌋, Y = i modulo 128 folded as above and Z = 126 − X − Y,
is 1/√(X² + Y² + Z²), computed by the x87 unit and stored as f32 [DS-12]. It is built before the core library sets
its x87 mode (E 2.5), so at the precision then in effect: **believed** the process default of 53 bits; at 24 bits,
2,076 of the 8,192 entries would differ in their last bit [DS-12]. Decoding gives: 0 → (0, 0, 1); 255 → (1, 0, 0); 126 →
(0, 1, 0); 8192 → (0, 0, −1); 129 → (0.008064, 0.008064, 0.999935); 5418 → (0.57735026, 0.57735026, 0.57735026)
[DS-12].

- `w_dir(v)`: stores v's direction code (2 bytes). v need not have length 1: only its direction is kept; `r_dir`
  gives a vector of length about 1.
- `w_sdir(v)`: L = √(x² + y² + z²), the sum of squares in f32 operations ((x·x + y·y) + z·z), the root by the x87
  unit, kept as f32. If L ≤ 10⁻⁷ (the f32 1.0000000116860974·10⁻⁷, compared before L is rounded), the direction is
  (0, 0, 1) and L is 0; otherwise the direction is (x·u, y·u, z·u) with u = 1/L, f32 operations. Stores the
  direction's code (2 bytes), then L as f32 (4 bytes) [DS-13].
- `r_dir(v)`: reads a code and sets v to its decoding. `r_sdir(v)`: reads a code and an f32 L, sets v to the
  decoding with each component multiplied by L (f32) [DS-12], [DS-13].

## 4. `reader` (static, [DS-14])

A reader is a stream over a block of bytes that it does not own: a **data** start, a **size** and a **position**
(signed 32-bit). Scripts get readers from the engine only (there is no constructor): the binder's `load(reader)`
(L) and the file system's `r_open` (F) [DS-3]. Its reads are those of the core library and match the packet's:

- `r_u8`, `r_u16`, `r_u32`, `r_s8`, `r_s16`, `r_s32`, `r_float` (value forms), `r_bool`, `r_stringZ`, the
  directions `r_dir(v)`, `r_sdir(v)`: as on a packet (3.3, 3.6), taking from the position and advancing it.
- `r_vec3(v)`: **fills** `v` with three f32 and gives nothing (unlike the packet's, 3.3) [DS-8].
- `r_float_q16(min, max)`, `r_float_q8(min, max)`, `r_angle16()`, `r_angle8()`: as in 3.5 (2 or 1 bytes), giving the
  value.
- `r_u64()`, `r_s64()`: as on a packet (2.3). The reference forms never match (2.1).
- `r_tell()`: the position. `r_seek(pos)`: the position becomes `pos` To s32. `r_advance(n)`: the position increases
  by `n` To s32. `r_elapsed()`: size − position. `r_eof()`: true when size − position < 1.
- No read checks the position against the size: a read past the end takes the bytes that follow the block in
  memory (believed to read other data or fail). `r_stringZ` searches for the zero byte without regard to the size.

## 5. `ClientID` (static, [DS-9])

A `ClientID` holds one `u32`. `ClientID()` makes one holding 0; `id:value()` gives it (a number in [0, 2³²));
`id:set(n)` sets it to `n` To u32; `a == b` compares the two values. Packets carry it as a `u32` (`w_clientID`,
`r_clientID`). In the listing, only the simulation objects' `OnEvent(packet, number, number, ClientID)` (J), four
multiplayer game-type classes (scoping spec Z) and `net_packet` take it [DS-2].

## 6. Where scripts meet streams

- A simulation object's `STATE_Read(packet, size)` gets the engine's packet with its read position at the start of
  the object state and `size` = the spawn packet's data size field (J 5.1, sav 7.2); `STATE_Write(packet)` writes
  the object state at the write position; `UPDATE_Read` and `UPDATE_Write` the update data (J). A scripted class
  first calls the engine's implementation (`Base.STATE_Read(self, packet, size)`, A 6.1), which reads the engine part
  of the state, then reads its own fields (sav 7.5).
- A binder's `save(packet)`, `net_import(packet)`, `net_export(packet)` get packets and `load(reader)` a reader
  (L).
- Scripts make packets with `net_packet()` only in the multiplayer scripts (measured: `game_last.script`,
  `game_last_cl.script`, `game_last_sv.script`) [DS-15].

## 7. Use by the shipped scripts (measured, [DS-15])

The loadable scripts use 22 names of this spec: the class `net_packet`, the methods `r_bool`, `r_clientID`, `r_eof`,
`r_float`, `r_s16`, `r_s32`, `r_stringZ`, `r_u16`, `r_u32`, `r_u8`, `r_vec3`, `set`, `w_bool`, `w_float`, `w_s16`,
`w_s32`, `w_stringZ`, `w_u16`, `w_u32`, `w_u8`, `w_vec3`, in 25 loadable scripts (51 counting the names `set`,
`r_bool`, `r_float`, `r_u32`, `r_s32` that other classes also declare, as the scoping spec counts). Every number
read with an argument in the shipped scripts (27 calls) has two arguments, so it is a configuration reader of F, not
a reference form of this spec. `heli_combat.script` (loadable) calls `packet:r_vec3(self.enemy_last_seen_pos)` and
`packet:r_vec3(self.center_pos)` on a `net_packet` and discards the result: by 3.3 those vectors keep the values
they had (static consequence). **Disputed in review:** these two calls are not on a `net_packet`: `heli_combat:load`
is called by the helicopter binder's `load(packet)` (`bind_heli.script`) with the value it received, which is the
binder's `reader` (L: `load(reader)`). So they are `reader:r_vec3(v)` calls, which fill the vectors passed with the
three f32 read (section 4); the vectors get the stored values.

## 8. Worked example: the beginners' village campfire on a New Game

The smart terrain `esc_lager` (the novices' camp, smart-terrain-jobs.md section 6) and one of its campers, the
stalker `esc_lager1` (one of the 11 stalker records whose `[smart_terrains]` section names `esc_lager`), are records
of `spawns\all.spawn` (sav 10.1), `esc_lager` on `l01_escape` [DS-16]. Their classes are the scripted `se_smart_terrain` (`smart_terrain.script`, class
id `SMRTTRRN`) and `se_stalker` (`se_stalker.script`, `AI_STL_S`) (J 7). Positions below are byte positions in the
spawn packet, its message type at 0 (sav 7.2, 10.1).

| Record | Object | Spawn packet | Data size (at) | Object state | Engine part | Script part |
|---|---|---|---|---|---|---|
| 93 | `esc_lager1`, section `stalker`, `AI_STL_S`, `l01_escape` | 284 bytes | 219 (65) | 67 … 283 | 67 … 280 (214 bytes) | 281 … 283: `02 00 00` |
| 101 | `esc_lager`, section `smart_terrain`, `SMRTTRRN` | 234 bytes | 164 (70) | 72 … 233 | 72 … 230 (159 bytes) | 231 … 233: `00 00 00` |

<!-- spec-lint: data position -->

Both records have script version 6 [DS-16]. Every one of the 1,073 stalker records of the file ends its object
state with `02 00 00`, and every one of the 282 smart terrains with `00 00 00` [DS-16].

**`se_stalker:STATE_Read(packet, 219)`** (static and measured, [DS-16]). The script calls the engine's
`cse_alife_human_stalker.STATE_Read(self, packet, size)`, which reads the engine part (sav 7.4); the packet's read position is
then 281 and `r_elapsed()` would give 3 (believed, as are all packet positions in this section: the packet holds the
spawn packet from its message type at 0, so its write position is 284). Then, as the script version is at least 3, 5 and 6:

1. `packet:r_u8()` reads byte 281 = 2 and gives the number 2: the job-online mode is "nil" (`self.job_online = nil`);
   no condition text follows (that is only for 3).
2. `packet:r_bool()` reads byte 282 = 0: `self.was_in_smart_terrain = false`.
3. `packet:r_bool()` reads byte 283 = 0: `self.death_droped = false`.

The read position is then 284; `r_eof()` would give true (believed, as above).

**`se_smart_terrain:STATE_Read(packet, 164)`** (static and measured, [DS-16]). The script calls
`cse_alife_smart_zone.STATE_Read`, which reads the engine part (read position then 231), reads its parameters from
the record's custom data (not a stream read) and, the editor not running, reads:

1. `utils.r_CTime(packet)`: `packet:r_u8()` at 231 gives 0, so the time is unset and no further byte is read: the
   duration end is the script's "unset" time `CTime_0` (sav 7.5's script time).
2. `utils.r_CTime(packet)` again: byte 232 = 0: the idle end is unset.
3. `packet:r_bool()` at 233 = 0: `self.gulag_working = false`, so nothing more is read (no members, no gulag state).

**The second read.** When the New Game creates the objects from these records (J 5.1), the record's object writes
its packet and the new object reads it with the same `STATE_Read`. The scripted writers write the same bytes back:
`se_stalker:STATE_Write` writes `w_u8(2)` for the nil mode, then `w_bool(false)` twice; `se_smart_terrain:STATE_Write`
writes `w_u8(0)` for each unset time (`utils.w_CTime`) and `w_bool(false)`; so the new objects read 2, false, false
and unset, unset, false as above [DS-16].

## 9. Verification facts

- Files: the three libraries as identified [DS-1]. Listing: 81 items; 63, 32 and 2 functions [DS-2].
- Registrations: exactly the listed functions; 10 and 9 names with two forms; no property; `__eq` registered [DS-3],
  [DS-4].
- Calls: one-form methods called unchecked; forms chosen by count; reference forms never match; 305 class
  registrations, none for a number type [DS-5], [DS-6].
- Conversions and results [DS-7], [DS-8]; packet layout and methods [DS-9]; the floor [DS-10]; quantised and angle
  values [DS-11]; directions, examples and the table's precision dependence (2,076 entries) [DS-12], [DS-13];
  reader [DS-14]; use by scripts: 22 names, 25 (51) loadable scripts, 27 two-argument reads, the two `r_vec3` calls
  [DS-15]; worked example [DS-16].

## 10. Unestablished and believed

- What follows a call for which no form matches (2.1): depends on A; believed to fail.
- The access violations believed for: reference forms and reference parameters (`r_float_q8`, `r_float_q16`,
  `r_angle8`, `r_angle16`, `r_begin`, `w_chunk_open8`, `w_chunk_open16` of `net_packet`), 64-bit reads and writes,
  non-object or nil arguments where an object is expected, `w_stringZ` with a value that has no text, and reads or
  writes past the buffer or the reader's block. That they fail, rather than read or write something harmless, is
  inferred from the conversions and the absence of checks; no run observed it.
- Whether the run-time library's SSE2 flag is set (so which branch of "To s32" applies): believed set on processors
  with SSE2.
- The x87 precision when the direction table is built (believed 53 bits, the process default), and when the
  `reader`'s quantised reads run (E 2.5, believed 24 bits).
- That engine-supplied packets and readers are the engine's own objects used in place, and must not be kept by a
  script after the call.
- The read and write positions of the packet given to `STATE_Read` on a New Game (section 8: believed from J 5.1
  and sav 10.1).
- What the packet's fourth field holds (believed a receive time); scripts cannot reach it.
- Calls on a value that is not an object of the method's class (2.1).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the binaries as identified above; the
  scripts and `spawns\all.spawn` as winning copies of the root archives, extracted with the project's archive tool.
- Analysis: static analysis of `xrGame.dll`, `xrCore.dll` and `xrLUA.dll` with Ghidra in read-only headless mode
  (decompilation and disassembly of the registration code of the three classes, the per-signature call and match
  functions, the bound functions, the conversions, the binding library's method dispatch, and the core library's
  stream reads, direction coding and its table); an emulation of the bit-level float-to-integer sequences on their
  disassembled instructions; a model of the direction coding and of the table at two precisions; scans of the shipped
  scripts and of `spawns\all.spawn`. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used. Public
  documentation used: the Lua 5.1 reference manual (value conversions of the runtime interface) and the processor
  documentation (results of out-of-range conversions).

## Gaps found in review

- `w_angle8`/`w_angle16`: the integer part i of the angle reduction (3.5) is given only as ⌊t⌋ or ⌈t⌉; what it is,
  and so what is stored, when t is NaN or |t| ≥ 2³¹ (an angle of NaN or of a magnitude of about 1.35·10¹⁰ or more) is
  not stated.
