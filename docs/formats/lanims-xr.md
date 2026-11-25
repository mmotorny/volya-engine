# `lanims.xr` (colour animations: format and evaluation)

Functional specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the colour
animation library `lanims.xr`: its layout, how named entries are found, and the colour (and frame
number) an entry gives at any time, for all 65 shipped entries. It is specification P12 of the
approved `particles-and-lights-scope`.

Builds on (not repeated): `docs/formats/chunks.md` (chunk headers, nesting, lookup by id, the
numbered-list convention), `docs/formats/db-archive.md` (archives, the combined tree).

Conventions: integers are little-endian; sizes in bytes. "f32" is an IEEE 754 single-precision
(binary32) value; "rounded to f32" means rounded to the nearest f32, ties to even. `floor(x)` is the
greatest integer ≤ x. Text is Windows-1251. Every fact carries the name of its check, a script in
`checks/lanims-xr/` of the dirty workspace; `run_all.py` runs them all.

## 1. Scope and boundary

This specification owns: the file and its layout (sections 2–3); finding an entry by name
(section 4); the meaning of an entry's values (section 5); the colour and frame number an entry gives
at a frame and at a time, and an entry's length (section 6); the parameters and per-frame colours of
all 65 entries (section 7).

Not owned here:

- How user-interface elements apply an animation (the `light_anim`, `la_*` and `xform_anim*`
  attributes, cyclic or one-shot playback, alpha-only or texture-only use, transforms): the HUD
  specification 1, `colour-animations`. That specification covers the same file; the file layout,
  the lookup by name and the evaluation of an entry are stated in full here.
- Which time a light passes and how it turns the colour into light: the owners of the users
  (hanging lamps P13, zone idle lights P9, searchlights P14, thunderbolts P21, the torch and others
  in the approved `particles-and-lights-scope`).
- The approved `startup` §6.4 already states the entry `asus_logo_01`; section 8 agrees with it.

## 2. The file

- Stored path `lanims.xr`, read from the combined game-data tree. Two copies exist, in
  `gamedata.db0` and `gamedata.dba`, identical: 6,665 bytes, CRC-32 `0xBFA9BD87`. The `dba` copy
  wins. (`c_file.py`)
- If the file is missing, the library is empty (no entry can be found) and no error is raised.
  When the game reads the file, and whether it is ever read again, is **unestablished**.
  (`c_reading.py`)

## 3. Layout

The file is a chunk sequence (`chunks.md`) with two top-level chunks, in this order (`c_layout.py`):

| Id | Size | Content |
|---|---|---|
| 0 | 2 | **version**, 16-bit; the shipped value is 1 |
| 1 | 6,647 | the entries: a sub-chunk sequence |

Reading rules (`c_reading.py`):

- Chunk 0 is looked up by id; if it is missing, the version is taken as 0.
- Chunk 1 is opened as a sub-chunk sequence; if it is missing, the library is empty. Its entries are
  sub-chunks with ids 0, 1, 2, … read as a numbered list (`chunks.md` section 2: the list ends at the
  first id not found). The shipped file has exactly 65, ids 0–64, stored in id order, filling
  chunk 1 exactly. (`c_layout.py`)
- **Version 0 only:** after an entry is read, bytes 0 and 2 of every key's value (section 5) are
  exchanged; bytes 1 and 3 are kept. Version 1 values are used as stored.

### 3.1 Entry

Each entry sub-chunk is itself a chunk sequence. The shipped entries hold exactly two chunks, ids 1
and 2 in that order, each filled exactly by the fields below (`c_layout.py`). Both are required: a
missing chunk 1 or 2 is a fatal error of the game (an assertion) (`c_reading.py`).

Chunk 1 (common data):

| Offset | Size | Field |
|---|---|---|
| 0 | n + 1 | **name**: zero-terminated string (n bytes and a 0 byte) |
| n + 1 | 4 | **frame rate**: f32, frames per second |
| n + 5 | 4 | **frame count**: 32-bit integer |

Chunk 2 (keys):

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | **key count** k: 32-bit signed integer; 0 or less means no keys are read |
| 4 + 8i | 4 | key i **frame**: 32-bit signed integer |
| 8 + 8i | 4 | key i **value**: 32-bit colour (section 5) |

Keys form a set ordered by frame, compared as signed integers. A key whose frame equals an earlier
key's frame of the same entry replaces that key's value (the later one wins). Stored order does not
matter for reading. (`c_reading.py`)

Shipped data (`c_layout.py`):

- 405 keys in all; every entry has 1 to 27 keys, stored in strictly increasing frame order (signed),
  so no entry has two keys at the same frame.
- Every entry has a key at frame 0. Every key frame is at most frame count − 1. The only negative key
  frame is −1, the first key of `svet_01`, stored first.
- Frame rates are 5, 6, 10, 15, 17, 20, 30, 50, 60, 100 or 200; frame counts 1 to 500.
- Entry names are unique, 6 to 27 bytes (without the 0 byte), ASCII; 13 contain `\` (for example
  `light\omni_01`, `weathers\thunderbolt_00`, `levels\psy_antenna\starting`), 5 contain spaces
  (`light 40 hz`, `light alarm 1`, `light alarm 2`, `tv light 1`, `light\light alarm_idle`).
  Section 7 lists every name.

## 4. Finding an entry by name

An entry is found by its name: the entries are compared in file order with the requested name, byte
for byte (case-sensitive, no path or slash folding); the first entry whose name is equal is the
result. An empty requested name (or none) finds nothing; a name not in the file finds nothing. What a
user does when nothing is found is that user's specification. (`c_reading.py`)

## 5. Values

A key value is a 32-bit integer holding four 8-bit channels: bits 24–31 **alpha**, bits 16–23
**red**, bits 8–15 **green**, bits 0–7 **blue** (stored as bytes blue, green, red, alpha). Each
channel is 0–255. This is the 32-bit ARGB of the approved `startup` §6.4. (`c_values.py`)

Evidence for the channel order: entries named for colours have their dominant channel where this
order puts it (`alarm_r` keys `0x00FF0000` and `0x00FF3E1F`; `light alarm 2` `0x00FF0000`;
`light\light_omni_blue_idle` first key `0x00C0E1FF`; `light_green` first key `0x0099D538`); and the
thunderbolt adds bits 16–23 of its colour to the first of the three colour components it changes
(P21). (`c_values.py`)

Alpha: in 33 entries every key has alpha 0; the others use alpha values from 0 to 255 (section 7
lists the alpha values of each entry). Whether alpha matters depends on the user (section 1).
(`c_layout.py`)

Some users ask for the value with bytes 0 and 2 exchanged (red and blue swapped in the integer); this
is a representation for that user, not a different colour. (`c_reading.py`)

## 6. Evaluation

### 6.1 Colour at a frame

The colour of an entry at an integer frame f, where f ≥ the entry's first key frame and
f ≤ frame count (`c_eval_bin.py`):

1. If a key has frame f, the result is that key's value, exactly as stored.
2. Otherwise, if no key has a frame greater than f, the result is the value of the key with the
   greatest frame (the last key is held to the end; there is no wrap-around interpolation to the
   first key).
3. Otherwise, let key 1 be the key with the smallest frame greater than f, at frame f1, and key 0 the
   key with the greatest frame less than f, at frame f0. Each channel is interpolated separately,
   with every operation below carried out in f32 and its result rounded to f32:
   - w = (f − f0) ÷ (f1 − f0), where f, f0 and f1 are first converted to f32;
   - u = 1 − w;
   - with c0 and c1 the channel's values (0–255) in key 0 and key 1, and K the f32 nearest to 1/255
     (bit pattern `0x3B808081`, 0.003921569): p = c0 × K, q = c1 × K;
   - r = (u × p + q × w) × 255;
   - the channel is floor(r), limited to 0–255.

   The four channels form the result in the layout of section 5.

The f32 rounding is part of the behaviour: over the 3,453 frames 0 … frame count of all entries, it
makes 127 channel values, in 75 frames, one less than the exact value floor(c0 + (f − f0)
(c1 − c0) ÷ (f1 − f0)) would give, and none differs otherwise (for example `koster` frame 38 gives
`0x00FFA553`, not `0x00FFA554`; `ui_minimap_enemy` frame 201 gives `0xB9B92300`, not `0xBABA2300`).
(`c_eval.py`)

Frames outside that range: a frame greater than frame count is a fatal error of the game (an
assertion); a frame before the entry's first key is **unestablished** (the game does not guard it).
Neither occurs for times ≥ 0 (6.2). (`c_eval_bin.py`, `c_layout.py`)

### 6.2 Colour at a time

The colour of an entry at a time t (seconds, an f32) is the colour at frame (6.1)
`frame = floor(F)`, where (`c_eval_bin.py`):

- the **length** L = frame count ÷ frame rate, the frame count converted to f32 and the quotient
  rounded to f32;
- the **phase** = the remainder of t (the dividend) divided by L: t − n × L for the integer n that
  truncates t ÷ L toward zero, computed exactly; for t ≥ 0 it is in [0, L);
- F = phase × frame rate, rounded to f32.

So playback is cyclic: time L, 2L, … starts the entry again at frame 0, and the frame advances in
whole frames (the fraction of F is discarded). The frame number is also handed to the user together
with the colour. For t ≥ 0 the frame is 0 … frame count; the shipped entries never reach frame count
at the largest f32 below L, and even if a frame equals frame count rule 2 of 6.1 holds the last key.
(`c_eval.py`)

Negative times: the phase is then negative and the frame negative; the result is **unestablished**
(6.1).

Precision: the game sets its floating-point arithmetic to single precision, rounding to nearest, when
it starts and at the start of every frame; the one switch to higher precision found in `xrGame.dll`
switches back before returning; the two renderer libraries also import a higher-precision setting,
not examined further (`c_eval_bin.py`). Every evaluation is therefore taken to run at single
precision, as 6.1 and 6.2 state; whether any evaluation runs while another precision is in effect is
**unestablished**. It matters at frame boundaries: computing L, the phase and F in double precision
instead gives a different frame for 6,312 of the 3,900,065 pairs (entry, t) with t = n ÷ 1000 rounded
to f32, n = 0 … 60,000 (for example `light 40 hz` at t = 0.7 rounded to f32, 0.699999988: frame 7 at
single precision, 6 at double). (`c_eval.py`)

### 6.3 Length

- In seconds: L of 6.2.
- In whole milliseconds, as some users ask for it: floor(G) with G = L × 1000 rounded to f32, L as
  in 6.2. Section 7 lists it for every entry. (`c_eval_bin.py`, `c_eval.py`)

## 7. The 65 entries

In file order (entry sub-chunk id = row number − 1). "Keys" is the key count, "Last" the greatest key
frame, "ms" the length in whole milliseconds (6.3), "Alpha" the distinct alpha values of the keys,
"CRC" the CRC-32 of the colours at frames 0, 1, …, frame count (6.1) written as 32-bit little-endian
integers, one after another. (`c_layout.py`, `c_eval.py`)

| # | Name | Rate | Frames | Keys | Last | ms | Alpha | CRC |
|---|---|---|---|---|---|---|---|---|
| 1 | `spot_blim` | 200 | 50 | 10 | 45 | 250 | 0 | `3A14F57B` |
| 2 | `torch_blink` | 100 | 30 | 7 | 8 | 300 | 0 | `23722D5B` |
| 3 | `warning_01` | 15 | 10 | 3 | 9 | 666 | 0 | `F04AEA5F` |
| 4 | `svet_00` | 30 | 30 | 3 | 29 | 1000 | 0 | `B2899AEA` |
| 5 | `light 40 hz` | 10 | 100 | 12 | 58 | 10000 | 0 | `E3DDAAAB` |
| 6 | `light alarm 1` | 30 | 25 | 4 | 24 | 833 | 0 | `5CABFE1F` |
| 7 | `light alarm 2` | 15 | 15 | 3 | 5 | 1000 | 0 | `99C0C4C1` |
| 8 | `tv light 1` | 15 | 60 | 9 | 59 | 4000 | 0 | `EB736E77` |
| 9 | `explode_light` | 20 | 30 | 7 | 29 | 1500 | 0 | `14610579` |
| 10 | `svet_01` | 30 | 30 | 4 | 29 | 1000 | 0 | `D111A0B8` |
| 11 | `svet_02` | 30 | 30 | 3 | 29 | 1000 | 0 | `B517F2AE` |
| 12 | `alarm_r` | 15 | 15 | 3 | 14 | 1000 | 0 | `0A57792B` |
| 13 | `galogen_br` | 15 | 60 | 10 | 54 | 4000 | 0 | `DE95B578` |
| 14 | `light_green` | 10 | 40 | 6 | 39 | 4000 | 0 | `4DE1FF2D` |
| 15 | `koster` | 30 | 60 | 14 | 59 | 2000 | 0 | `7425B6D0` |
| 16 | `koster_00` | 15 | 60 | 16 | 59 | 4000 | 0 | `41F70164` |
| 17 | `koster_01` | 20 | 60 | 18 | 57 | 3000 | 0 | `65AFB891` |
| 18 | `ui_pda_contacts` | 5 | 11 | 11 | 10 | 2200 | 0, 255 | `13862BB6` |
| 19 | `ui_new_jobs` | 15 | 1 | 1 | 0 | 66 | 255 | `2144DF1C` |
| 20 | `ui_minimap_enemy` | 20 | 500 | 7 | 499 | 25000 | 0, 255 | `D61B8B40` |
| 21 | `ui_minimap_neutral` | 15 | 1 | 1 | 0 | 66 | 255 | `66AC3B0D` |
| 22 | `ui_minimap_friend` | 15 | 1 | 1 | 0 | 66 | 255 | `AEEA89B5` |
| 23 | `ui_claws_animation` | 15 | 1 | 1 | 0 | 66 | 255 | `2144DF1C` |
| 24 | `ui_main_msgs` | 15 | 200 | 3 | 199 | 13333 | 0, 254, 255 | `B171C0BC` |
| 25 | `ui_map_area_anim` | 15 | 10 | 2 | 9 | 666 | 0, 255 | `5B5AFF5E` |
| 26 | `ui_mp_chat` | 30 | 240 | 3 | 235 | 8000 | 0, 255 | `EB48E4F0` |
| 27 | `light\light alarm_idle` | 10 | 60 | 12 | 59 | 6000 | 0 | `7B497D31` |
| 28 | `light\light_alarm_blim` | 15 | 60 | 17 | 59 | 4000 | 0 | `A08DC7E3` |
| 29 | `light_green_00` | 15 | 30 | 6 | 25 | 2000 | 0 | `0D436A99` |
| 30 | `light\light_omni_white_idle` | 15 | 60 | 5 | 59 | 4000 | 0 | `70FEFD7B` |
| 31 | `light\light_omni_blue_idle` | 15 | 60 | 8 | 59 | 4000 | 0 | `D7447C68` |
| 32 | `light\omni_01` | 15 | 1 | 1 | 0 | 66 | 0 | `57F579B6` |
| 33 | `light\omni_01_br` | 15 | 120 | 17 | 119 | 8000 | 0 | `CD1C0E8E` |
| 34 | `light\omni_02` | 15 | 1 | 1 | 0 | 66 | 0 | `546958E0` |
| 35 | `levels\psy_antenna\starting` | 5 | 100 | 21 | 99 | 20000 | 0, 10, 20, 30, 40, 50, 90, 120 | `0F8A0B22` |
| 36 | `levels\psy_antenna\working` | 5 | 100 | 27 | 99 | 20000 | 0, 20, 50, 70, 80, 90, 100, 120, 150 | `91057D3C` |
| 37 | `levels\psy_antenna\stopping` | 10 | 100 | 8 | 99 | 10000 | 0, 10, 40, 255 | `9EFC8B2C` |
| 38 | `weathers\thunderbolt_00` | 50 | 50 | 6 | 49 | 1000 | 0 | `A0E2F34C` |
| 39 | `ui_mp_minimap_af_neutral` | 20 | 31 | 4 | 30 | 1550 | 0, 255 | `785CECBC` |
| 40 | `ui_mp_minimap_af_friend` | 20 | 31 | 4 | 30 | 1550 | 0, 255 | `2AC116DE` |
| 41 | `ui_mp_minimap_af_enemy` | 20 | 31 | 4 | 30 | 1550 | 0, 255 | `38817DB5` |
| 42 | `ui_mm_mp_srvinfo` | 30 | 30 | 2 | 29 | 1000 | 0, 255 | `576AC204` |
| 43 | `ui_task_selected` | 15 | 30 | 3 | 29 | 2000 | 255 | `D210DF1E` |
| 44 | `ui_slow_blinking` | 30 | 30 | 3 | 29 | 1000 | 100, 255 | `5845510B` |
| 45 | `ui_slow_blinking_alpha` | 30 | 30 | 3 | 28 | 1000 | 72, 255 | `4B09B447` |
| 46 | `ui_too_slow_blinking` | 17 | 30 | 3 | 29 | 1764 | 100, 255 | `5845510B` |
| 47 | `hud_hit_mark` | 15 | 30 | 3 | 29 | 2000 | 0, 200, 230 | `282891B7` |
| 48 | `monster_claws` | 60 | 30 | 3 | 29 | 500 | 0, 200, 230 | `282891B7` |
| 49 | `ui_btn_hint` | 30 | 30 | 2 | 29 | 1000 | 0, 255 | `576AC204` |
| 50 | `ui_main_msgs_short` | 15 | 36 | 2 | 35 | 2400 | 0, 255 | `9A8BF5F3` |
| 51 | `light_green_01` | 15 | 30 | 6 | 28 | 2000 | 0 | `C385B1CF` |
| 52 | `light_green_02` | 10 | 40 | 6 | 39 | 4000 | 0 | `4DE1FF2D` |
| 53 | `light\omni_03` | 15 | 1 | 1 | 0 | 66 | 0 | `B64B8211` |
| 54 | `light\light_galogen_br_01` | 30 | 60 | 18 | 58 | 2000 | 0 | `DF55ACBD` |
| 55 | `credits_vis` | 15 | 75 | 4 | 74 | 5000 | 0, 255 | `2551D553` |
| 56 | `credits_vis_sub` | 15 | 60 | 4 | 59 | 4000 | 0, 255 | `0A87507F` |
| 57 | `credits_head` | 15 | 15 | 2 | 14 | 1000 | 0, 255 | `6E1A6BE7` |
| 58 | `buy_menu_info` | 15 | 75 | 5 | 74 | 5000 | 0, 255 | `401CF456` |
| 59 | `tutor_rad_sign_xform` | 15 | 41 | 5 | 40 | 2733 | 0, 64, 128, 192, 255 | `D1864C9A` |
| 60 | `tutor_rad_sign_clr` | 15 | 15 | 3 | 14 | 1000 | 64, 128 | `41CCB7F6` |
| 61 | `map_spot_rel` | 30 | 15 | 3 | 14 | 500 | 0, 255 | `4BFC4B0D` |
| 62 | `map_spot_rel_xform` | 6 | 15 | 5 | 14 | 2500 | 0 | `C2F3D3A4` |
| 63 | `map_spot_new_xform` | 6 | 15 | 5 | 14 | 2500 | 0 | `1CE9A8AE` |
| 64 | `teleport_stancia_in_01` | 30 | 30 | 7 | 24 | 1000 | 0 | `E6F5F27C` |
| 65 | `asus_logo_01` | 30 | 120 | 4 | 119 | 4000 | 0, 255 | `922F3B21` |

Every entry's first key is at frame 0 except `svet_01` (keys at −1, 0, …); its key at −1 never
decides a result for t ≥ 0, since frame 0 has its own key. Over all 65 entries: 3,453 frames
(0 … frame count each); the CRC-32 of all their colours in table order, frame by frame, is
`6B7D7C28`. (`c_eval.py`)

## 8. Test vectors

Times are the f32 nearest to the decimal given. (`c_eval.py`)

| Entry | t (s) | Frame | Colour |
|---|---|---|---|
| `light 40 hz` | 0 | 0 | `0x00FFD5A5` |
| `light 40 hz` | 0.35 | 3 | `0x00FFDEAB` |
| `light 40 hz` | 1.15 | 11 | `0x00FFEBB3` |
| `light 40 hz` | 10.35 | 3 | `0x00FFDEAB` |
| `tv light 1` | 0.5 | 7 | `0x00427DAF` |
| `tv light 1` | 2.0 | 30 | `0x0050AFFD` |
| `koster_00` | 1.3 | 19 | `0x00C58F34` |
| `koster` | 1.27 | 38 | `0x00FFA553` |
| `weathers\thunderbolt_00` | 0.6 | 30 | `0x003F3F3F` |
| `asus_logo_01` | 0.5 | 15 | `0x77000000` |
| `asus_logo_01` | 3.0 | 90 | `0xD9000000` |
| `spot_blim` | 0.24 | 48 | `0x00000000` (last key, frame 45, held) |
| `svet_01` | 0 | 0 | `0x00FFC066` |
| `ui_new_jobs` | 123.4 | 0 | `0xFFFFFFFF` (single key) |
| `ui_minimap_enemy` | 10.05 | 201 | `0xB9B92300` |
| `warning_01` | 0.6666666269 (largest f32 below L = 0.6666666865, bits `0x3F2AAAAB`) | 9 | `0x00AB1111` |

`asus_logo_01` matches the keys of `startup` §6.4 (frames 0, 32, 85, 119; values `0x00000000`,
`0xFF000000`, `0xFF000000`, `0x00000000`; 30 frames per second, 120 frames). (`c_layout.py`)

## 9. Unestablished

- When the file is read, and whether it is read again (section 2).
- The result for a frame before an entry's first key, and hence for negative times (6.1, 6.2).
- Whether any evaluation runs while a precision other than single is in effect (6.2).

## Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll` (and, for the list of importers,
  `bin\xrRender_R1.dll`, `bin\xrRender_R2.dll`, `bin\xrParticles.dll`), and the runtime library
  `bin\msvcr80.dll` (its floating-point remainder: operand order).
- Data examined: both copies of `lanims.xr`, every chunk, entry and key; all entries evaluated at
  every frame and on a grid of times.
- Binaries: static analysis (Ghidra 12.2, headless) of the executable's reading of `lanims.xr`, its
  lookup by name, its evaluation at a frame and at a time, its length computation, its
  floating-point precision setting and the thunderbolt's use of a colour; of the game library's
  imports of these functions and its precision switch. No execution or debugging.
- A throwaway evaluator written from this text reproduced every value of sections 3, 6, 7 and 8.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The colour (and frame) given by an entry with no keys (key count 0 or less) is not stated; no shipped entry has one.
- The length, phase and frame for an entry whose frame count is 0 or whose frame rate is 0 or negative are not stated; no shipped entry has one.
