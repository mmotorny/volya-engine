# Colour and transform animations (`lanims.xr`)

Functional specification of the colour-animation library of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): the file `lanims.xr`, the colour an entry gives at a given time, and how interface elements apply
an entry to a picture's or a text's colour and alpha (`light_anim`, `la_*`) and to a picture's rotation
and size (`xform_anim`, `xform_anim_cyclic`). It is proposed spec 1 of the approved scoping spec
`hud-scope` and covers the whole file, which level lights and the menus use too.

It builds on the approved `chunks` (`docs/formats/chunks.md`: chunk headers, lookup by id, lists of
sub-chunks 0, 1, 2, …), `db-archive`, `ui-layout` (`docs/formats/ui-layout.md`: colours §2, rectangles
§4.1, statics §5, rotatable pictures §5.4) and `startup` §6.4 (`docs/behavior/startup.md`: the entry
`asus_logo_01` on the logo pictures), and names the entries used by `level-objects`, `options-screens`
and the HUD specs of `hud-scope`.

Conventions:

- Integers are little-endian. Bit 0 is the least significant bit.
- **Single** means an IEEE-754 single-precision value, and "single(…)" the exact result of the operation
  rounded to the nearest single (ties to even). Where a formula below is marked single precision, every
  addition, subtraction, multiplication and division is rounded to single on its own. The program does
  its arithmetic this way: it sets the processor's floating-point unit to 24-bit precision with
  round-to-nearest at start-up and before every frame (c6 B8), and the rest is single-precision vector
  arithmetic.
- floor(x) is the largest integer not greater than x.
- Every fact carries its check, a script in `checks/colour-animations/` of the dirty workspace, run after
  `extract.py`: `c1` = `c1_file.py`, `c2` = `c2_keys.py`, `c3` = `c3_eval.py`, `c4` = `c4_layouts.py`,
  `c5` = `c5_users.py`, `c6` = `c6_binary.py` (items B1 … B8 for `XR_3DA.exe` and `xrCore.dll`, G1 … G12
  for `xrGame.dll`). `run_all.py` runs them all.
- **Believed** marks a statement that follows from established facts but was not observed directly;
  **unestablished** marks what this work could not settle.

## 1. The file

- Stored path `lanims.xr` at the root of the game data. Copies are in `gamedata.db0` and `gamedata.dba`;
  they are identical: 6,665 bytes, CRC-32 0xBFA9BD87 (c1).
- The program reads it at start-up (`startup`) into a library of named entries (c6 B1). If the file
  is absent the library is empty and nothing reports it (c6 B1).

## 2. Layout

### 2.1 Top-level chunks

A chunk sequence (`chunks`) filling the file. The shipped file holds exactly two chunks (id, payload
offset, size): (0, 8, 2) and (1, 18, 6,647) (c1).

| Chunk | Payload |
|---|---|
| 0 | `u16` **version**. The shipped file has 1. Without chunk 0 the version is 0 (c1, c6 B1). |
| 1 | The **entry list**: sub-chunks with ids 0, 1, 2, …, one per entry, read in id order until the first id that is missing (`chunks` §2). The shipped file has sub-chunks 0 … 64 in id order: 65 entries (c1, c6 B1). |

### 2.2 An entry

Each entry sub-chunk is itself a chunk sequence; the shipped entries hold exactly chunk 1 then chunk 2
(c1).

| Chunk | Offset | Size | Field |
|---|---|---|---|
| 1 | 0 | *n* + 1 | **name**: bytes, NUL-terminated (all shipped names are printable ASCII; c1) |
| 1 | *n* + 1 | 4 | **frame rate** *fps*: single, frames per second |
| 1 | *n* + 5 | 4 | **frame count** *N*: `u32` |
| 2 | 0 | 4 | **key count** *k*, read as a signed 32-bit integer; *k* ≤ 0 means no keys |
| 2 | 4 + 8*i* | 4 | key *i*: **frame**, signed 32-bit integer |
| 2 | 8 + 8*i* | 4 | key *i*: **value**, `u32` (2.4) |

- In the shipped file chunk 1 is exactly name + NUL + 8 bytes and chunk 2 exactly 4 + 8*k* bytes (c1).
- An entry without chunk 1 or chunk 2 makes the program report an internal error (an assertion) while
  loading; what follows is unestablished (c6 B2). No shipped entry lacks either.
- Names are compared byte for byte (2.5); all 65 shipped names are distinct, also ignoring case (c1).

### 2.3 Keys

- The keys of an entry form a set indexed by frame: a key whose frame equals that of an earlier key in the
  same entry replaces it, and the order in the file does not matter (c6 B2).
- With version 0 every key value has bits 0–7 and bits 16–23 exchanged when loaded (alpha and green
  unchanged); with any other version values are used as stored (c6 B1). The shipped file is version 1.
- Shipped data (c2): keys are stored in increasing frame order, no entry repeats a frame, every entry has a
  key at frame 0, no key is at or beyond the entry's frame count, and the only negative frame is −1 in
  `svet_01` (keys (−1, 0x00E9CE72), (0, 0x00FFC066), …).

### 2.4 Key values

A value is a colour packed as alpha in bits 24–31, red in bits 16–23, green in bits 8–15, blue in bits
0–7, i.e. the 8-bit ARGB colour of `ui-layout` §2 written as 0xAARRGGBB. Interface elements use the
evaluated value directly as such a colour (4.3). The hanging lamp, a level light, takes its red, green and
blue from the same bit positions (through the light variant of 3.3; c6 G12). Transform animations read
the alpha and red bytes as an angle and a scale (4.4).

### 2.5 Lookup by name

A name is looked up by comparing it byte for byte (case-sensitive) with the entries' names in library
order; the first equal entry is the result. An empty name finds nothing (c6 B3). What a caller does when
nothing is found is stated with the caller (4.1, 5).

## 3. Evaluation

### 3.1 From time to frame

For an entry with frame rate *fps* and frame count *N*, at a time *t* in seconds (a single):

1. **Length** *L* = single(*N* / *fps*) seconds (c6 B4, B6).
2. *r* = the exact remainder of *t* divided by *L* with the sign of *t* (the C `fmod` function).
3. **Frame** *f* = floor(single(*r* × *fps*)) (c6 B4).

So the animation repeats every *L* seconds and advances in whole frames.

### 3.2 From frame to value

The value at frame *f* (c6 B5):

1. If the entry has a key at *f*: that key's value, exactly.
2. Otherwise, if no key has a frame greater than *f*: the value of the key with the greatest frame.
3. Otherwise interpolate between the nearest key before *f* (frame *f*₀, value *a*) and the nearest key
   after it (frame *f*₁, value *b*), in single precision, with *K* = the single nearest to 1/255 (bit
   pattern 0x3B808081):
   - *s* = (*f* − *f*₀) / (*f*₁ − *f*₀), with *f*, *f*₀, *f*₁ converted to singles; *u* = 1 − *s*;
   - for each of the four 8-bit channels (alpha, red, green, blue) with byte *c*ₐ in *a* and *c*ᵦ in *b*:
     *A* = *c*ₐ × *K*, *B* = *c*ᵦ × *K*, channel = floor((*u* × *A* + *B* × *s*) × 255), clamped to 0 … 255;
   - the channels are packed back in their own bit positions (2.4).

A frame before the first key with no key at or before it is not defined by the program (it reports an
internal error when the entry has a single key; otherwise the result is unestablished). It cannot occur
with the shipped data, because every entry has a key at frame 0 (c2) and the times used are not negative
(4.2, 5).

An implementation that interpolates in double precision or rounds instead of flooring gives different
values at some frames: for `asus_logo_01` frame 105 is 0x68000000 (alpha 104) because *u* × 255 is just
below 105 in single precision (c3).

### 3.3 Light variant

The program also offers the value of 3.2 with bits 0–7 and 16–23 exchanged (c6 B4). The hanging lamp uses
this form and reads it back with the exchanged positions, so the colour it gets is the same 0xAARRGGBB
value; it evaluates at the game clock, a pausable timer, not the interface clock of 4.2 (c6 G12). The
code-driven animator of section 5 also uses this form and turns it back into 0xAARRGGBB (c6 G10). What
lights do with the colour, and the clocks of the other light users of 6.2, belong to their own
specifications.

### 3.4 Test vectors

All from the shipped file (c3):

- `asus_logo_01` (30 fps, 120 frames, *L* = 4 s): *t* = 0 → frame 0, 0x00000000; 0.5 → 15, 0x77000000;
  1.0 → 30, 0xEF000000; 1.1 → 33, 0xFF000000; 2.0 → 60, 0xFF000000; 3.5 → 105, 0x68000000; 3.99 → 119,
  0x00000000; 4.5 → 15, 0x77000000 (the times are the singles nearest to the decimals shown).
- `ui_slow_blinking_alpha` (30 fps, 30 frames; keys 0: 0xFFFFFFFF, 15: 0x48FFFFFF, 28: 0xFFFFFFFF), frames
  0 … 29: FFFFFFFF F2FFFFFF E6FFFFFF DAFFFFFF CEFFFFFF C1FFFFFF B5FFFFFF A9FFFFFF 9DFFFFFF 91FFFFFF
  85FFFFFF 78FFFFFF 6CFFFFFF 60FFFFFF 54FFFFFF 48FFFFFF 56FFFFFF 64FFFFFF 72FFFFFF 80FFFFFF 8EFFFFFF
  9CFFFFFF AAFFFFFF B8FFFFFF C6FFFFFF D4FFFFFF E2FFFFFF F0FFFFFF FFFFFFFF FFFFFFFF (frame 29 is past the last
  key).
- `light 40 hz` (10 fps, 100 frames): frames 0, 5, 10, 11, 12, 13, 30, 58, 59, 99 → 0x00FFD5A5,
  0x00FFE5B0, 0x00FFF6BB, 0x00FFEBB3, 0x00FFE0AC, 0x00FFD5A5, 0x00FFD5A5, 0x00FFD5A5, 0x00FFD5A5,
  0x00FFD5A5.
- Frames from times: `ui_slow_blinking` at 0.1 s → 3, at 1.0 s → 0, at 0.999 s → 29; `map_spot_new_xform`
  (6 fps, 15 frames) at 2.5 s → 0; `ui_too_slow_blinking` (17 fps, 30 frames) at 1.0 s → 17;
  `ui_minimap_enemy` (20 fps, 500 frames) at 24.99 s → 499.
- Whole file: the values of every entry at every frame 0 … *N* − 1, entries in file order, each written as
  a little-endian `u32`: 3,388 values, CRC-32 0x2AC808CF. Per entry (same layout): `torch_blink`
  0xBC4A27A7, `koster_00` 0x16DB4CFC, `tv light 1` 0x47F27F05, `weathers\thunderbolt_00` 0xA3A7E226,
  `ui_pda_contacts` 0x59FB51C3, `credits_head` 0x32C1146F, `tutor_rad_sign_clr` 0x65798572,
  `ui_main_msgs_short` 0xCC446E65.

## 4. Animations on interface elements

### 4.1 Attributes

Read for every element initialised as a static element (`ui-layout` §5), which includes `auto_static`
children (`ui-layout` §4.2; c6 G1):

| Attribute | Default | Meaning |
|---|---|---|
| `light_anim` | empty | name of the **colour animation** (2.5); empty or not found: none, silently |
| `la_cyclic` | 1 | non-zero: the colour animation repeats; 0: it plays once (4.3) |
| `la_texture` | 1 | non-zero: the animation sets the picture's texture colour (`ui-layout` §5.1) |
| `la_text` | 1 | non-zero: the animation sets the text colour |
| `la_alpha` | 0 | non-zero: only the alpha is taken from the animation; the colour's red, green and blue stay |
| `xform_anim` | empty | name of the **transform animation** (2.5); empty or not found: none, silently |
| `xform_anim_cyclic` | 1 | non-zero: the transform animation repeats; 0: it plays once (4.4) |

Integer attributes are read as `ui-layout` reads integers; any non-zero value counts as on (c6 G1, G2).
The names are looked up once, when the element is initialised (c6 G2). The attribute `light_anim_cyclic`
is not read (so it does not change `la_cyclic`), nor is `_light_anim` (c4, c6 G1; neither text occurs in
`xrGame.dll`).

### 4.2 Clock and start

- Interface animations run on the **interface clock**: a real-time millisecond counter of the program,
  read once at the start of every frame. It does not stop when the game is paused and is not scaled by the
  game's time factor (c6 B7). In seconds: *now* = single(*ms* × *M*), where *ms* is the counter (an exact
  integer, unsigned) and *M* is the single nearest to 0.001 (bit pattern 0x3A83126F) (c6 G3).
- Animations advance when the element is updated. An element is updated only while it is shown, and its
  children only while they are shown too (c6 G7); that shown elements are updated once per frame is
  believed.
- Each element keeps a colour start time and a transform start time, both unset when the element is made
  (c6 G4). The first update with a colour animation set fixes the colour start at
  single(single(*ms* + *delay*) × *M*), where *delay* is a per-element number of milliseconds that is 0
  unless code sets it; no layout attribute sets it (c6 G1, G3, G4). The transform start is fixed likewise
  (4.4).
- While *now* is less than the colour start, the update changes neither the element's colours nor its
  transform; its shown children are still updated, before its own animations (c6 G3, G7). Otherwise the colour animation (4.3) is applied before the transform animation (4.4).
- An element never restarts its animations by itself: hiding and showing it again does not reset them, and
  since the start stays, the animation's time keeps running while the element is hidden. Code can set the
  colour start; the logo sequence does this whenever it shows a picture (`startup` §6.4; c6 G11). Which
  other parts restart an element's animations is specified with those parts.

### 4.3 Colour animation

With *e* = single(*now* − colour start) (c6 G3):

- If `la_cyclic` is on, or *e* < *L* of the entry: *v* = the entry's value at time *e* (section 3), and
  - if `la_texture` is on, the texture colour becomes *v*, or with `la_alpha` on, the texture colour keeps
    its red, green and blue and takes the alpha of *v*;
  - if `la_text` is on, the text colour likewise.
- Otherwise (played once and *e* ≥ *L*) nothing is changed: the colours keep the value set by the last
  update before the end. That value is the entry's value at the last update's frame, which is the last
  frame only if an update fell within it (e.g. `credits_head`, 15 fps and 15 frames, ends at 1 s holding
  frame 14's value 0xFF000000 when the last update before 1 s came after 14/15 s; c3 shows an update
  1.067 s after the start is already past the end).
- The texture colour and text colour are the colours of `ui-layout` §5.1 and §7; the animation replaces
  what the layout set, from the first update on (c6 G3, G9).

### 4.4 Transform animation

The transform start is set on the first update when it is unset, to *now* (no delay) (c6 G3). With
*e* = single(*now* − transform start):

- If `xform_anim_cyclic` is off and *e* ≥ *L*: rotated drawing is switched off and the element's size is
  set back to its initial size, on this and every later update (c6 G3). (The program restores a recorded
  copy of the `heading` attribute here, but that record is cleared when the transform attributes are read,
  so for elements set up from a layout file rotated drawing ends up off even with `heading="1"`; c6 G1,
  G2. No shipped element combines `xform_anim` with `heading`, c4.)
- Otherwise, with *v* = the entry's value at time *e* (section 3):
  - rotated drawing is switched on (`ui-layout` §5.4) with angle = single(alpha × *R*) radians, *R* = the
    single nearest to 2π/255 (bit pattern 0x3CC9D9B5): alpha 0 is 0 and alpha 255 is a full turn
    (6.283185); positive angles turn counter-clockwise on screen (`ui-layout` §5.4);
    **Disputed in review:** 0x3CC9D9B5 (0.024639944) is not the single nearest to 2π/255, which is
    0x3CC9D9B4; the program uses 0x3CC9D9B5, one unit above it, so *R* is the single with bit pattern
    0x3CC9D9B5 (alpha 255 then gives 6.2831855 rounded to single; the six-decimal angles of the test
    vectors below are unchanged);
  - the element's width and height become single(*w* × red/64) and single(*h* × red/64), where *w* and *h*
    are the width and height of the rectangle the element had when it was initialised (`ui-layout` §4.1;
    red/64 is exact): red 64 is the initial size, 128 twice it;
  - green and blue are not used (c6 G3).
- The **initial size** is recorded when the element is initialised and code that resizes the element later
  does not change it (c6 G5).
- Consequences (believed, from `ui-layout`): the size changes the drawn picture only when `stretch` is on
  (`ui-layout` §5.3); an element with `alignment="c"` keeps its centre, because its top-left is recomputed
  from the current size whenever its rectangle is needed (c6 G6), while other elements grow from their
  top-left corner; and while rotated drawing is on, the wide-screen narrowing of `ui-layout` §5.4 applies
  even at angle 0. Rotated drawing uses the stored angle (c6 G8).
- Test vectors (c3): `tutor_rad_sign_xform` (15 fps, 41 frames; keys 0: 0x00280000, 10: 0x40400000,
  20: 0x80550000, 30: 0xC0400000, 40: 0xFF280000) at frames 0, 5, 10, 20, 35, 40 gives 0x00280000,
  0x20340000, 0x40400000, 0x80550000, 0xDF340000, 0xFF280000: angles 0, 0.788478, 1.576956, 3.153913,
  5.494708, 6.283185 rad; scales 0.625, 0.8125, 1, 1.328125, 0.8125, 0.625 (a 50-unit side: 31.25, 40.625,
  50, 66.40625, 40.625, 31.25). `map_spot_new_xform` (6 fps, 15 frames) scales by frame: 1, 1.078125,
  1.15625, 1.25, 1.1875, 1.125, 1.0625, 1, 1.0625, 1.125, 1.1875, 1.25, 1.15625, 1.078125, 1 (angle 0).

### 4.5 Use in the layout files

Survey of the 154 files `config\ui\*.xml`, comments removed (c4):

- `light_anim` 767 times (18 of them empty, all in `game_tutorials.xml`; `credits_head` 701 times),
  `la_alpha` 754 (741 × 1), `la_texture` 752 (42 × 1), `la_cyclic` 738 (29 × 1), `la_text` 9, `xform_anim`
  32, `xform_anim_cyclic` 32, `light_anim_cyclic` 16, `_light_anim` 1 (`job_item.xml`), and an `anim="fade"`
  in `maingame_new.xml` (not an attribute of 4.1).
- Every non-empty `light_anim` or `xform_anim` value names an entry. The names used are `asus_logo_01`,
  `buy_menu_info`, `credits_head`, `map_spot_new_xform`, `map_spot_rel_xform`, `monster_claws`,
  `tutor_rad_sign_clr`, `tutor_rad_sign_xform`, `ui_btn_hint`, `ui_mp_minimap_af_enemy`,
  `ui_mp_minimap_af_friend`, `ui_mp_minimap_af_neutral`, `ui_pda_contacts`, `ui_slow_blinking_alpha`,
  `ui_too_slow_blinking`.
- All 32 elements with `xform_anim` (in `game_tutorials.xml`, `map_spots.xml`, `map_spots_relations.xml`)
  have `stretch="1"` and `alignment="c"` and no `heading`.
- The 16 elements with `light_anim_cyclic` (in `game_tutorials.xml` and `ui_movies.xml`) have no
  `la_cyclic`, so their colour animations repeat; the three in `ui_movies.xml` say
  `light_anim_cyclic="0"`, which `startup` §6.4 notes makes no visible difference there.

## 5. Code-driven colour animator

Parts of the interface that animate a colour from code, not from a layout attribute, use a separate
animator; which parts, with which entry names and targets, is specified with those parts (`hud-scope`
§5 lists the names `xrGame.dll` contains, c5). It holds an entry, a cyclic flag, an accumulated time *T*,
the clock reading of its previous update, an offset *o*, an ended flag and a current colour; its clock is
the interface clock *now* of 4.2 (c6 G10).

- **Setting an entry by name** (2.5) resets it: *T* = 0, *o* = 0, not ended, current colour 0xFFFF0000,
  previous reading = *now*. One way of setting reports an internal error (an assertion) when a non-empty
  name is not found, another leaves the animator without an entry; which parts use which is unestablished
  (c6 G10).
- **Updates** (c6 G10). Every update records *now* as the previous reading. An animator without an entry,
  or ended, changes nothing else. Otherwise:
  - a cyclic animator shows the value at time *now* itself (section 3), not at a time relative to when the
    entry was set;
  - a played-once animator shows the value at time |*T* − *o*| and then adds to *T* the time since the
    previous reading; once *T* ≥ *N* × (1/*fps*) it shows instead the value at time
    (*N* − 1) × (1/*fps*) − *o* and is ended;
  - the shown colour is copied to the animator's target colour, if it has one.
- So a played-once animation starts at time 0 on the first update after setting, advances by the real time
  between updates (including any time in which its owner did not update it), and ends holding its last
  frame. With each step in single precision, the ending colour is the value of frame *N* − 1 for the eight
  entries named in `xrGame.dll` (`hud_hit_mark`, `monster_claws`, `ui_main_msgs_short`, `ui_map_area_anim`,
  `ui_mm_mp_srvinfo`, `ui_mp_chat`, `ui_pda_contacts`, `ui_slow_blinking`) (believed; c3).
- The colour value is used in the 0xAARRGGBB layout of 2.4 (c6 G10).
- Unestablished: how code sets the offset *o* or the cyclic flag and the target, and therefore whether an
  animation can be played backwards (|*T* − *o*| suggests so); the exact rounding of its time steps.

## 6. Library contents and users

### 6.1 Entries

Index (sub-chunk id), name, frame rate, frame count, key count, first and last key frame, and the CRC-32 of
the entry's chunk 2 payload (c1). 405 keys in all; frame rates 5, 6, 10, 15, 17, 20, 30, 50, 60, 100, 200;
frame counts 1 … 500 (c1). The name of `asus_logo_01` starts at file offset 6,600 (c1, `startup` §6.4).

| Index | Name | fps | Frames | Keys | First key | Last key | Chunk 2 CRC-32 |
|---|---|---|---|---|---|---|---|
| 0 | `spot_blim` | 200 | 50 | 10 | 0 | 45 | 5573B550 |
| 1 | `torch_blink` | 100 | 30 | 7 | 0 | 8 | CC4F2560 |
| 2 | `warning_01` | 15 | 10 | 3 | 0 | 9 | 2CC88B94 |
| 3 | `svet_00` | 30 | 30 | 3 | 0 | 29 | 01FD2563 |
| 4 | `light 40 hz` | 10 | 100 | 12 | 0 | 58 | EB757E86 |
| 5 | `light alarm 1` | 30 | 25 | 4 | 0 | 24 | 24895E71 |
| 6 | `light alarm 2` | 15 | 15 | 3 | 0 | 5 | 8561382F |
| 7 | `tv light 1` | 15 | 60 | 9 | 0 | 59 | 64898103 |
| 8 | `explode_light` | 20 | 30 | 7 | 0 | 29 | 34993F4C |
| 9 | `svet_01` | 30 | 30 | 4 | -1 | 29 | AC3028D7 |
| 10 | `svet_02` | 30 | 30 | 3 | 0 | 29 | AB179E9B |
| 11 | `alarm_r` | 15 | 15 | 3 | 0 | 14 | 8D5274D9 |
| 12 | `galogen_br` | 15 | 60 | 10 | 0 | 54 | 08B7C7A0 |
| 13 | `light_green` | 10 | 40 | 6 | 0 | 39 | F3F93976 |
| 14 | `koster` | 30 | 60 | 14 | 0 | 59 | CBD09223 |
| 15 | `koster_00` | 15 | 60 | 16 | 0 | 59 | F817265D |
| 16 | `koster_01` | 20 | 60 | 18 | 0 | 57 | EF32CFDD |
| 17 | `ui_pda_contacts` | 5 | 11 | 11 | 0 | 10 | 03FE95F1 |
| 18 | `ui_new_jobs` | 15 | 1 | 1 | 0 | 0 | 3ECBAAE3 |
| 19 | `ui_minimap_enemy` | 20 | 500 | 7 | 0 | 499 | E46F0804 |
| 20 | `ui_minimap_neutral` | 15 | 1 | 1 | 0 | 0 | C6E3F7DB |
| 21 | `ui_minimap_friend` | 15 | 1 | 1 | 0 | 0 | 73EA5160 |
| 22 | `ui_claws_animation` | 15 | 1 | 1 | 0 | 0 | 3ECBAAE3 |
| 23 | `ui_main_msgs` | 15 | 200 | 3 | 0 | 199 | 08DF5C4C |
| 24 | `ui_map_area_anim` | 15 | 10 | 2 | 0 | 9 | 04EFD6C9 |
| 25 | `ui_mp_chat` | 30 | 240 | 3 | 0 | 235 | D25BAE66 |
| 26 | `light\light alarm_idle` | 10 | 60 | 12 | 0 | 59 | C7060363 |
| 27 | `light\light_alarm_blim` | 15 | 60 | 17 | 0 | 59 | 5CBF9793 |
| 28 | `light_green_00` | 15 | 30 | 6 | 0 | 25 | 5BC72BAF |
| 29 | `light\light_omni_white_idle` | 15 | 60 | 5 | 0 | 59 | C8CC3A0A |
| 30 | `light\light_omni_blue_idle` | 15 | 60 | 8 | 0 | 59 | 82FB6A18 |
| 31 | `light\omni_01` | 15 | 1 | 1 | 0 | 0 | 50D34CE4 |
| 32 | `light\omni_01_br` | 15 | 120 | 17 | 0 | 119 | A12DDD7B |
| 33 | `light\omni_02` | 15 | 1 | 1 | 0 | 0 | 68A001EE |
| 34 | `levels\psy_antenna\starting` | 5 | 100 | 21 | 0 | 99 | 49AB277B |
| 35 | `levels\psy_antenna\working` | 5 | 100 | 27 | 0 | 99 | 04B6E6F0 |
| 36 | `levels\psy_antenna\stopping` | 10 | 100 | 8 | 0 | 99 | 53A2241D |
| 37 | `weathers\thunderbolt_00` | 50 | 50 | 6 | 0 | 49 | 88143870 |
| 38 | `ui_mp_minimap_af_neutral` | 20 | 31 | 4 | 0 | 30 | 16578573 |
| 39 | `ui_mp_minimap_af_friend` | 20 | 31 | 4 | 0 | 30 | 684DD769 |
| 40 | `ui_mp_minimap_af_enemy` | 20 | 31 | 4 | 0 | 30 | 41F241E8 |
| 41 | `ui_mm_mp_srvinfo` | 30 | 30 | 2 | 0 | 29 | 0FA72676 |
| 42 | `ui_task_selected` | 15 | 30 | 3 | 0 | 29 | 12C18CEE |
| 43 | `ui_slow_blinking` | 30 | 30 | 3 | 0 | 29 | F8A2C192 |
| 44 | `ui_slow_blinking_alpha` | 30 | 30 | 3 | 0 | 28 | A1EEE3C5 |
| 45 | `ui_too_slow_blinking` | 17 | 30 | 3 | 0 | 29 | F8A2C192 |
| 46 | `hud_hit_mark` | 15 | 30 | 3 | 0 | 29 | 590AAACE |
| 47 | `monster_claws` | 60 | 30 | 3 | 0 | 29 | 590AAACE |
| 48 | `ui_btn_hint` | 30 | 30 | 2 | 0 | 29 | 0FA72676 |
| 49 | `ui_main_msgs_short` | 15 | 36 | 2 | 0 | 35 | C5717E4A |
| 50 | `light_green_01` | 15 | 30 | 6 | 0 | 28 | 3CB13248 |
| 51 | `light_green_02` | 10 | 40 | 6 | 0 | 39 | F3F93976 |
| 52 | `light\omni_03` | 15 | 1 | 1 | 0 | 0 | 4722EA55 |
| 53 | `light\light_galogen_br_01` | 30 | 60 | 18 | 0 | 58 | 0DA7025C |
| 54 | `credits_vis` | 15 | 75 | 4 | 0 | 74 | 8F44F6A7 |
| 55 | `credits_vis_sub` | 15 | 60 | 4 | 0 | 59 | A7F1C8A6 |
| 56 | `credits_head` | 15 | 15 | 2 | 0 | 14 | FD9310BE |
| 57 | `buy_menu_info` | 15 | 75 | 5 | 0 | 74 | 78FE1D61 |
| 58 | `tutor_rad_sign_xform` | 15 | 41 | 5 | 0 | 40 | 77A455D6 |
| 59 | `tutor_rad_sign_clr` | 15 | 15 | 3 | 0 | 14 | 9BB91315 |
| 60 | `map_spot_rel` | 30 | 15 | 3 | 0 | 14 | 5F65434A |
| 61 | `map_spot_rel_xform` | 6 | 15 | 5 | 0 | 14 | F6DA885F |
| 62 | `map_spot_new_xform` | 6 | 15 | 5 | 0 | 14 | BD4A5A4B |
| 63 | `teleport_stancia_in_01` | 30 | 30 | 7 | 0 | 24 | FF7AEDFF |
| 64 | `asus_logo_01` | 30 | 120 | 4 | 0 | 119 | E950A3C6 |

Further facts (c2):

- 12 entries have their last key before their last frame: `galogen_br`, `koster_01`, `light 40 hz`,
  `light alarm 2`, `light\light_galogen_br_01`, `light_green_00`, `light_green_01`, `spot_blim`,
  `teleport_stancia_in_01`, `torch_blink`, `ui_mp_chat`, `ui_slow_blinking_alpha`; their remaining frames
  hold the last key's value (3.2).
- The 7 entries with a single key (`ui_new_jobs`, `ui_minimap_neutral`, `ui_minimap_friend`,
  `ui_claws_animation`, `light\omni_01`, `light\omni_02`, `light\omni_03`) have 1 frame.
- 33 entries have alpha 0 in every key.
- The three entries named `*_xform` have green and blue 0 in every key.

### 6.2 Who names the entries

Exact occurrences in the winning copies of `config\`, `scripts\`, `spawns\all.spawn`, every
`levels\*\level.spawn` and `levels\*\level`, and the printable strings of `xrGame.dll` and `XR_3DA.exe`
(c5):

- **Layout files**: the 15 names of 4.5 (and `ui_pda_contacts` also as an `id` in
  `config\gameplay\info_portions.xml` and as a string in the eleven scripts `task_11.script` …
  `task_20.script` and `task_sim.script`).
- **`xrGame.dll`**: `hud_hit_mark`, `monster_claws`, `ui_main_msgs_short`, `ui_map_area_anim`,
  `ui_mm_mp_srvinfo`, `ui_mp_chat`, `ui_pda_contacts`, `ui_slow_blinking` (owned, as code-driven uses, by
  the specs `hud-scope` §5 assigns them to). `XR_3DA.exe` names none.
- **Configuration**: keys `idle_light_anim` (`zone_kampfire.ltx`, `zone_torrid.ltx`,
  `zone_zharka_static.ltx`, `zone_buzz.ltx`, `zone_teleport.ltx`), `light_animation` (`zone_kampfire.ltx`,
  `zone_buzz.ltx`), `light_color_animmator` (`helicopter.ltx`: `koster_00`) and `color_anim`
  (`weathers\thunderbolt.ltx`: `weathers\thunderbolt_00`, 8 sections).
- **Spawn and level data**: `koster`, `koster_00`, `light 40 hz`, `light alarm 1`, `light\light alarm_idle`,
  `light\light_galogen_br_01`, `light\light_omni_blue_idle`, `light\light_omni_white_idle`,
  `light\omni_01`, `light_green`, `tv light 1` (among them the hanging lamps' colour animators,
  `level-objects`; which record kinds name the others was not examined).
- **Nowhere**: the other 28 entries have no exact occurrence in this data or the two binaries (a name
  built at run time would not be found this way).

## 7. Unestablished

- What happens after the loading assertions of 2.2 and the interpolation assertion of 3.2 (frame before the
  only key); the value for a frame before the first key otherwise (does not occur with shipped data).
- Whether any code sets a non-zero colour delay (4.2) or restarts element animations other than the logo
  sequence; owned by the parts concerned.
- For the code-driven animator (5): how the offset, cyclic flag and target are set, reverse play, and the
  exact rounding of its time steps.
- Where the interface clock counts from (only differences are used, but large values lose precision in
  single: above 2²⁴ ms, about 4.7 hours, *now* has steps coarser than 1 ms).
- How lights other than the hanging lamp, zones, the helicopter and thunderbolts apply their entries, and
  their clocks (their own specs).

## Provenance

- Date: 2026-11-24. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrCore.dll`, `bin\xrGame.dll`.
- Data examined: `lanims.xr` (both copies), the winning copies of `config\` (layouts, configuration),
  `scripts\`, `spawns\all.spawn`, `levels\*\level.spawn` and `levels\*\level`, extracted with the project's
  archive tool.
- Analysis: static analysis of the three binaries with Ghidra (headless; decompilation and disassembly of
  the library loader, lookup, evaluation, the floating-point mode set-up, the frame clock, the static
  element's initialisation, update and drawing, the code-driven animator and the hanging lamp), string
  scans of the binaries, and a parser and evaluator written from this text (`checks/colour-animations/lib.py`)
  run over the whole file. No execution of the game.
- Checks: `checks/colour-animations/extract.py`, then `c1_file.py` … `c6_binary.py` (or `run_all.py`), run
  with `python3 -I`; all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Setting an entry by name on the code-driven animator (section 5) also clears its target, so the colour is copied nowhere until a target is given again; the reset list of section 5 omits this.
- The fatal error (an assertion) for a frame greater than the frame count, stated in `lanims-xr` §6.1, is not repeated in section 3.2.
