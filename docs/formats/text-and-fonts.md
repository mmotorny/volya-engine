# Localized text and fonts

Functional specification of how S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) turns a string
key into pixels: the string tables, the font definitions and glyph metric files, the choice of
resolution-dependent font textures, the drawing of one line of text, and the placement, colouring and
wrapping of text inside UI text elements. It covers everything the title menu (approved spec
`docs/behavior/title-menu.md`, sections 3.4, 4 and 5) needs and states the general rules.

Conventions: file paths are paths in the combined game-data tree (archive specification); the copy
that wins under archive precedence is the one described. "UI units" are the 1024 × 768 virtual UI
space of the title-menu specification; *W* × *H* is the output resolution in pixels and
*s*ₓ = *W* / 1024, *s*ᵧ = *H* / 768 are the UI-to-pixel scale factors (title-menu spec, section 8).
LTX files follow the LTX specification. `floor` rounds toward −∞.

## 1. String tables

### 1.1 Configuration

`config\localization.ltx` (in this edition):

```
[string_table]
language	= rus
font_prefix	= ;_cent ;_west
files = ui_st_pda, ui_st_mm_mp, ui_st_inventory, … , ui_st_other, stable_game_credits
```

- `language` names the language directory (`rus`).
- `files` is a comma-separated list of table names (41 in this edition); each item is trimmed of
  surrounding whitespace. Order matters (section 1.4).
- `font_prefix` is a font matter (section 2.4). Under LTX comment rules its value here is empty.

### 1.2 Which files are loaded, and when

- The string table is built once, on the first lookup of any key (in practice while the first UI
  screen is built), and kept for the rest of the run.
- For each name *N* of `files`, in list order, the file `config\text\<language>\N.xml` is loaded
  (path alias `$game_config$`; here e.g. `config\text\rus\ui_st_mm.xml`). A listed file that does not
  exist is a fatal error ("string table xml file not found"). All 41 listed files exist in this
  edition.

### 1.3 File format

The file is preprocessed line by line, then parsed as XML.

**Include lines.** A line whose first byte is `#` and which contains `#include` is replaced by the
(recursively preprocessed) contents of the file named between the first pair of double quotes. The
path is relative to the same alias as the including file (`$game_config$`), e.g.
`#include "text\rus\string_table_tips_escape.xml"`. (A path starting with `ui\` may be swapped for a
wide-screen `_16` variant; this applies only to UI layout files.) A missing include is an error. All
other lines are passed unchanged. `string_table_includes.xml` is a list of 47 such includes; the
included files are bare fragments (a sequence of `<string>` elements without XML declaration or
root).

**XML structure.**

```xml
<?xml version="1.0" encoding="windows-1251" ?>
<string_table>
	<string id="ui_mm_newgame">
		<text>Новая игра</text>
	</string>
	…
</string_table>
```

- The root element's name is not checked (all files use `string_table`). Only direct children named
  `string` are entries; anything else (comments, other elements) is ignored.
- `id` is the key. The value is the character content of the first `text` child of the entry.
- Standard XML rules apply: comments are skipped; the five predefined entities (`&amp;` `&lt;`
  `&gt;` `&quot;` `&apos;`) and numeric character references are decoded; CDATA sections are
  supported. The data uses none of these inside texts.
- **Whitespace normalisation.** The text content is normalised: leading and trailing whitespace is
  removed and every run of whitespace characters (space, tab, CR, LF, other ASCII whitespace) inside
  it becomes one space. Physical line breaks therefore never survive into a string; e.g. the credits
  entry `st_zaitsev` stored as `Евгений ` (with a trailing space) yields `Евгений`.
- Every entry in this edition has a non-empty `text`; the behaviour for an entry without text was not
  established.

### 1.4 Key resolution

- Keys are compared exactly (byte for byte, case-sensitive).
- Entries are inserted in load order (files in `files` order, includes expanded in place, entries in
  document order). If a key occurs again, the **later** text replaces the earlier one. (This edition
  has no duplicate keys: 7,639 distinct ids, 0 repeats, across the 41 files including the 47 included
  fragments.)
- **Missing key:** a lookup of a key that is not in the table returns the key itself, so the UI shows
  the key text verbatim. A lookup of an empty key returns an empty string.
- Every UI text taken from layout XML (the content of a `<text>` element, button captions etc.) and
  every caption key set by game scripts is passed through this lookup; text that is not a key is
  therefore shown unchanged.

### 1.5 Encoding

All string-table files declare `encoding="windows-1251"` and are stored in Windows-1251. No character
conversion takes place anywhere on the way to the screen: each byte of the string is used directly
as a glyph index (0–255) of the font (section 3.1). The Russian texts use the Cyrillic range
`0xC0`–`0xFF`, `0xA8`/`0xB8` (Ё/ё) and a few typographic bytes: `0x85` (…), `0xAB`/`0xBB` («»),
`0x96` (–), `0x93`/`0x94` (“”), `0xA0` (no-break space), `0xAD` (soft hyphen), `0xAE` (®).

### 1.6 Sequences inside strings

| Sequence | Where it is interpreted | Meaning |
|---|---|---|
| `$$ACTION_<name>$$` | when the table is loaded | Replaced by the name of the key bound to the input action `<name>` (case-insensitive action name), as `<primary>` or `<primary> , <secondary>` when two keys are bound. The unsubstituted text is remembered and the substitution is redone whenever key bindings change. Used by 32 strings (e.g. `$$ACTION_crouch$$`). The key-name strings belong to the key-binding specification. |
| `\n` (the two bytes `0x5C 0x6E`, not a real line break) | only by UI text elements in complex mode (section 4.3) | Line break. Used by 96 strings (dialogue texts). In simple mode and in direct font output it is drawn as the two glyphs `\` and `n`. |
| `%c[…]` | only by UI text elements in complex mode (section 4.3) | Colour change. In simple mode and in direct font output it is drawn literally. Not used in the Russian string tables; used by texts that game code builds (e.g. `%c[default]`, `%c[green]`, `%c[UI_orange]`, `%c[255,192,192,192]`). |
| `%s`, `%d` | only where specific game code uses a string as a format template | Placeholders filled by that code (9 strings of the multiplayer tables, e.g. `Игрок %s выиграл бой!`). Text elements draw strings literally, so a `%` is shown as `%`. |

### 1.7 Evidence (Russian edition)

| Key | File | Text |
|---|---|---|
| `ui_mm_newgame` | `ui_st_mm.xml` | Новая игра |
| `ui_mm_load_game` | `ui_st_mm.xml` | Загрузить игру |
| `ui_mm_last_save` | `ui_st_mm.xml` | Последн. сохран. |
| `ui_mm_quit2windows` | `ui_st_mm.xml` | Выйти в Windows |
| `ui_mm_quit_windows_message` | `ui_st_mm.xml` | Действительно ли вы хотите выйти в Windows? |
| `ui_st_btn_yes` | `ui_st_other.xml` | Да |
| `gd_stalker` | `ui_st_mm.xml` | опытный |

## 2. Font definitions

### 2.1 Fonts available to the UI

The game creates a fixed set of 11 fonts when its UI is first created. UI layout XML refers to them by
the `font` attribute; the names are matched exactly (case-sensitive). All 11 names in the table below
are accepted (including `di`, which no layout file of this edition uses). An absent or empty `font`
attribute leaves the element's font unchanged (no error). Any other non-empty value is a **fatal
error** in the shipped game: an error report "unknown font" naming the value is produced and the game
stops. (Were execution forced past that error, the lookup would yield no font and a text element
would keep its current font; all occurrences below are on `text` elements.)

The layout data of this edition contains three values that are not font names — `header` (10
times), `normal` (7) and `graffiti18` (1) — but only in the eight files `carbody.xml`,
`inventory.xml`, `inventorymp_new.xml`, `inventorymp_new2.xml`, `jobs_new.xml`, `maingame_new.xml`,
`map.xml` and `trade_new.xml` of `config\ui\`. None of these files is named by the game's code or
scripts or included by another file; they are never loaded, so the fatal error does not occur in
practice.

| `font=` in UI XML | Section of `config\fonts.ltx` | Flags |
|---|---|---|
| `graffiti19` | `ui_font_graffiti19_russian` | – |
| `graffiti22` | `ui_font_graffiti22_russian` | – |
| `graffiti32` | `ui_font_graff_32` | – |
| `graffiti50` | `ui_font_graff_50` | – |
| `arial_14` | `ui_font_arial_14` | – |
| `letterica16` | `ui_font_letterica16_russian` | – |
| `letterica18` | `ui_font_letterica18_russian` | – |
| `letterica25` | `ui_font_letter_25` | – |
| `medium` | `hud_font_medium` | – |
| `small` | `stat_font` | screen-relative size |
| `di` | `hud_font_di` | gradient, screen-relative size |

`fonts.ltx` also contains `[hud_font_small]:stat_font`, `[ui_font_arial_21]` and `[ui_font_graff_40]`,
which no UI font name reaches. The section names are fixed; they do not depend on the language.
(The engine's own loading-screen text uses `ui_font_graffiti19_russian` with the same rules.)

**Flags.** *Gradient*: the bottom edge of every glyph is drawn with the colour's red, green and blue
halved (alpha unchanged), the top edge with the full colour (section 3.4). *Screen-relative size*: a
`size` value (below) is a fraction of the screen height in pixels rather than pixels.

### 2.2 Section keys

| Key | Meaning |
|---|---|
| `shader` | Shader name for the glyphs; `font` for every font (section 3.5). |
| `texture` | Base name of the glyph texture and metric file, relative to `textures\` (e.g. `ui\ui_font_graff_22_1024`). Required. |
| `texture800`, `texture1600` | Optional low- and high-resolution variants (section 2.3). |
| `size` | Optional glyph height override (pixels, or × screen height for screen-relative fonts). When absent, the height from the metric file is used. No UI font in this edition has it. |
| `interval` | Optional `x, y` multipliers (default `1, 1`): *x* scales each glyph's advance, *y* scales the line height. Only `stat_font` sets it (`0.75,1`). |

### 2.3 Resolution-dependent variant

The variant is chosen from the output **height** *H* in pixels:

| *H* | Key tried first | Fallback order |
|---|---|---|
| ≤ 600 | `texture800` | `texture` |
| 601 – 900 | `texture` | – |
| ≥ 901 | `texture1600` | `texture` |

If the preferred key is absent the next lower one in the order `texture1600`, `texture`,
`texture800` is tried; if none of the tried keys exists, `texture` is used (it is required). The
choice is made when the fonts are created and made again, with the metric file reloaded, whenever the video device is reset (e.g. a resolution change).
The width and aspect ratio play no part.

Examples (every UI font of this edition except `medium`, `small` and `di` has all three keys; those
three have only `texture` and therefore use the same texture at every resolution):

| Resolution | `graffiti22` texture | `graffiti19` texture |
|---|---|---|
| 800 × 600 | `ui\ui_font_graff_22_800` | `ui\ui_font_graff_19_800` |
| 1024 × 768, 1280 × 720, 1366 × 768 | `ui\ui_font_graff_22_1024` | `ui\ui_font_graff_19_1024` |
| 1280 × 1024, 1600 × 1200, 1920 × 1080 | `ui\ui_font_graff_22_1600` | `ui\ui_font_graff_19_1600` |

### 2.4 Language prefix (`font_prefix`)

If `[string_table] font_prefix` of `localization.ltx` has a value, it is appended to the chosen
texture name (e.g. `ui\ui_font_graff_22_1024` + `_west` → `ui\ui_font_graff_22_1024_west`), except for
textures whose name contains `ui_font_hud_01`, `ui_font_hud_02` or `ui_font_console_02`. The data
ships `_cent` and `_west` variants of the UI font files, but in this edition the value is commented
out (empty), so **no prefix is applied**.

### 2.5 Files of a font

For the final name *T* (e.g. `ui\ui_font_graff_22_1024`):

- **Metric file** `textures\T.ini` (after removing anything from the last `.` in *T*, if any). It
  must exist (fatal error otherwise). It is an LTX file (section 2.6).
- **Glyph texture** `textures\T.dds`, loaded through the shader (section 3.5). All font textures of
  this edition are DXT5. The glyphs are white (RGB 255, 255, 255) with their shape in the alpha
  channel; e.g. `Н` of `ui_font_graff_22_1024` is fully transparent in the top 4 and bottom 6 rows of
  its 22-pixel cell.

### 2.6 Metric file formats

The metric file contains one of four sections. When a file contains more than one, the first present
in the order `[mb_symbol_coords]`, `[symbol_coords]`, `[Char Widths]`, `[font_size]` is used (section
names are case-insensitive, as in all LTX). No metric file of this edition contains more than one.

1. **`[symbol_coords]`** — used by 98 of the 101 metric files `textures\ui\ui_font_*.ini`, among them
   those of every UI font except `medium` and `small`. Two of the 98, `ui_font_letter_16.ini` and
   `ui_font_letter_18.ini`, lack `height`; no font references them (the letterica fonts use the
   `_800`, `_1024` and `_1600` files).
   - `height` — the glyph height *h* in texture pixels (also the default font height).
   - `000` … `255` (three-digit decimal byte codes, all 256 required) —
     `x1, y1, x2, y2` in texture pixels. The glyph's cell is *x* = `x1`, *y* = `y1`, width
     *w* = `x2` − `x1`; the height is always *h* (the fourth number is not used; in all files it equals
     `y1` + *h*). A glyph with *w* = 0 is invisible and has zero advance; codes 0–31 are all
     `0, 0, 0, 0`.
2. **`[mb_symbol_coords]`** — a 65,536-entry variant for multi-byte text (keys `00000`–`65535`,
   width `x2` + 1 − `x1`, absent keys are empty glyphs). Not used by this edition. (Listed second
   here for readability; it takes precedence over `[symbol_coords]`, see above.)
3. **`[Char Widths]`** — `height` *h* and keys `0`…`255` giving each glyph's width; glyph *i* lies in
   a grid of *h* × *h* cells, 16 per row: *x* = (*i* mod 16)·*h*, *y* = ⌊*i* / 16⌋·*h*. Used by two
   files, both with `height = 16`: `ui_font_hud_02` (font `medium`) and `ui_font_console_01`, which no
   font references (`di` uses `ui_font_console_02`, a `[symbol_coords]` file).
4. **`[font_size]`** — fixed-width grid: `height` *h*, `width` *c*, `cpl` (cells per line) *n*;
   glyph *i* is at *x* = (*i* mod *n*)·*c*, *y* = ⌊*i* / *n*⌋·*h*, width *c*. Used by `ui_font_hud_01`
   (`small`: 8 × 8, 16 per line), the only such file.

If none is present the font cannot be created (fatal).

### 2.7 Evidence: metrics

`ui_font_graff_22_1024` (256 × 256, `height = 22`):

| Char (code) | Line | Cell x, y | Width |
|---|---|---|---|
| space (32) | `032 =    0,    0,   10,   22` | 0, 0 | 10 |
| `.` (46) | `046 =  111,    0,  116,   22` | 111, 0 | 5 |
| `1` (49) | `049 =  134,    0,  141,   22` | 134, 0 | 7 |
| `Н` (205) | `205 =   12,  110,   22,  132` | 12, 110 | 10 |
| `а` (224) | `224 =  217,  110,  225,  132` | 217, 110 | 8 |
| `я` (255) | `255 =  246,  132,  255,  154` | 246, 132 | 9 |

The same characters in the other variants of `graffiti22`: `ui_font_graff_22_800` (256 × 256,
`height = 21`): space 9, `Н` 9 (cell 56, 105), `а` 8; `ui_font_graff_22_1600` (512 × 256,
`height = 32`): space 14, `Н` 14 (cell 421, 96), `а` 12.

All variants of the menu fonts:

| Font | 800 variant | 1024 variant (`texture`) | 1600 variant |
|---|---|---|---|
| `graffiti22` | 256 × 256, *h* 21 | 256 × 256, *h* 22 | 512 × 256, *h* 32 |
| `graffiti19` | 256 × 128, *h* 17 | 256 × 256, *h* 19 | 512 × 256, *h* 30 |
| `letterica16` | 256 × 128, *h* 14 | 256 × 128, *h* 16 | 256 × 256, *h* 21 |
| `arial_14` | 256 × 64, *h* 14 | 512 × 32, *h* 13 | 256 × 64, *h* 16 |

## 3. Drawing one string with a font

This is the font-level operation used by every text element: a string, a pen position (*x*, *y*) in
**screen pixels**, a colour (32-bit ARGB) and a horizontal alignment (left, right, centre).

### 3.1 Characters and advance

- The string is drawn on **one line**; it is not split, wrapped or interpreted in any way (`\n`,
  `%c[…]` are ordinary glyphs). Each byte selects the glyph with that code; the string ends at the
  first zero byte.
- Glyph widths are in **texture pixels and are not scaled to the screen**: a glyph *w* texels wide
  occupies *w* screen pixels at every resolution. (The variants of section 2.3 provide the size
  change between resolutions.)
- Each glyph advances the pen by *w* · interval.*x*. There is no kerning and no extra spacing; the
  metric files pack the glyph cells edge to edge, so the advance equals the cell width.
- The string's width is *S* = interval.*x* · Σ *w* over its bytes.
- A string of 512 bytes or more is not drawn at all.
- (A global glyph scale exists for UI rendered into an off-screen target of a different size than
  the screen; it is 1 for UI drawn to the screen, which is the case for all menus, and is omitted
  below.)

### 3.2 Position and alignment

- *x*₀ = floor(*x*) for left alignment, floor(*x*) − floor(*S*) for right alignment, and
  floor(*x*) − floor(*S* / 2) for centre alignment. The first glyph's left edge is at *x*₀; glyph *k*
  starts at *x*₀ + interval.*x* · (*w*₀ + … + *w*ₖ₋₁).
- The top of every glyph is at *y*₀ = floor(*y*) and the bottom at *y*₀ + *f*, where *f* is the font
  height (the metric file's `height`, or `size` from `fonts.ltx`). There is no baseline: the pen *y* is
  the top of the glyph cell.

### 3.3 Glyph quad and texture coordinates

Each visible glyph (width not 0) is a screen-aligned rectangle *w* pixels wide and *f* pixels high.
With the texture size *T*w × *T*h and the glyph cell (*gx*, *gy*, *w*) and metric height *h*:

- u from (*gx* + 0.5) / *T*w to (*gx* + 0.5) / *T*w + *w* / *T*w;
- v from (*gy* + 0.5) / *T*h to (*gy* + 0.5) / *T*h + *h* / *T*h.

(The half-texel offset maps texel centres to pixel centres for rectangles at integer pixel
positions.) If *f* ≠ *h* the glyph is stretched vertically. The quads are drawn as pre-transformed
screen-space vertices (depth 0.0001, reciprocal w 0.9999), without depth test or depth writes.

Example, `Н` of `ui_font_graff_22_1024`: u 0.048828 … 0.087891, v 0.431641 … 0.517578.

### 3.4 Colour

All four corners of a glyph get the string's colour; for a font with the *gradient* flag (`di` only)
the two bottom corners get the colour with R, G, B halved (alpha kept). The colour multiplies the
texture (section 3.5).

### 3.5 Shader `font`

All fonts use the shader `font` of the shader library `shaders.xr`. Its entry is a screen-space
blender with: base texture = the font texture, blending `BLEND`, texture clamp on, alpha reference
32, Z-test off, Z-write off, lighting off, fog off. As compiled by the renderer (examined for the
static-lighting renderer), this means:

- output colour = texture colour × vertex colour, output alpha = texture alpha × vertex alpha;
- alpha blending: result = source · α + destination · (1 − α);
- alpha test enabled with reference value 32 (the comparison function used was not established);
- clamped texture addressing (the filtering mode was not examined).

Since the glyph texels are white, a glyph appears in exactly the string colour with the texture's
alpha (times the colour's alpha) as coverage.

### 3.6 Line height

The height of one text line used by layout is *f* · interval.*y* pixels (for UI fonts: *f*).

## 4. Text in UI elements

### 4.1 Attributes (UI layout XML)

A static element's text is configured by its `<text>` child:

| Attribute | Default | Meaning |
|---|---|---|
| `font` | `letterica16` | font name (section 2.1) |
| `r`, `g`, `b`, `a` | 255 each | text colour; or `color="<name>"` naming a colour of `config\ui\color_defs.xml` |
| `align` | left | `l` left, `r` right, `c` centre; any other value keeps the default |
| `vert_align` | top | `t` top, `c` centre, `b` bottom; any other value keeps the default |
| `x`, `y` | 0 | text offset in UI units, added to the element's absolute top-left corner |
| `complex_mode` | 0 | 1 enables colour codes, `\n` line breaks and word wrapping (section 4.3) |
| element content | – | the text, looked up as a string-table key (section 1.4) |

`config\ui\color_defs.xml` defines `red`, `green`, `blue`, `UI_orange` (238, 153, 26), `white`,
`gray`, `ui_clr_1` (240, 217, 182), `ui_clr_2` (216, 186, 140), `ui_clr_3` (135, 123, 116); its
`a` defaults to 255.

The element's text is drawn after its own texture and its child elements. An empty text draws
nothing. Each element sets its font's colour and alignment before drawing, so elements sharing a font
do not affect each other.

### 4.2 Simple mode (`complex_mode` 0, the default)

The whole text is drawn as one string (section 3): colour codes `%c[…]` and `\n` are **not**
interpreted in this mode; they are drawn literally as their glyphs, like any other text. With the
element's absolute rectangle (*X*, *Y*, *w*, *h*) in UI units, its text offset (*tx*, *ty*), and the line height in UI units
*t* = *f* · interval.*y* / *s*ᵧ:

- horizontal anchor *a*ₓ = 0 (left), *w* (right) or *w* / 2 (centre); the font is drawn with the same
  alignment, so the text starts at, ends at, or is centred on the anchor;
- vertical offset *a*ᵧ = 0 (top), (*h* − *t*) / 2 (centre) or *h* − *t* (bottom);
- pen position in pixels: *x* = *s*ₓ · (*X* + *tx* + *a*ₓ), *y* = *s*ᵧ · (*Y* + *ty* + *a*ᵧ), then
  section 3.2.

There is no clipping to the element rectangle and no wrapping; a text wider than the element
overflows it.

### 4.3 Complex mode (`complex_mode` 1)

The text is split into lines and coloured segments, then each line is drawn.

**Colour codes.** A code is a `%c[` together with the first `]` at or after it; its *content* is
what lies between the two. The text is cut into pieces [EP-C1, EP-C2, EP-C3]:

- If the first `%c[` of the text has no `]` anywhere after it, the text has no code: all of it is
  drawn as it is, `%c[` included, in the element's text colour.
- Otherwise the text before the first code has the element's text colour, and each code starts a
  piece that runs up to the next code, or to the end of the text when there is none. A further
  `%c[` counts as a code only if a `]` follows it; otherwise it is ordinary text of the current
  piece, drawn as it is.

The code sets the colour of its piece. It is chosen from the content, in this order:

- the content is `default` or any beginning of it, including empty (`%c[]`, `%c[d]`, `%c[def]`):
  the element's text colour [EP-C4]. **Disputed in review:** the test is a search, not a comparison at the code: the element's text colour is chosen when `%c[` followed by the first *n* characters of `default` (*n* = the content's length) occurs anywhere in the piece from the code on. This is the content being a beginning of `default`, or, besides, an unclosed `%c[` later in the same piece followed by those characters (e.g. `%c[1,2]x%c[def` gives the element's text colour). A content of 8 or more characters never selects it;
- the content equals the name of a colour of `color_defs.xml` (exact, case-sensitive match): that
  colour [EP-C5];
- otherwise four numbers are read from the piece: A from just after `%c[`, R, G and B from just
  after the first, second and third comma of the piece, the commas being searched from the code on
  to the end of the piece, past the `]` if need be. Each number is a leading decimal integer as C
  `atoi` reads it (optional white space and sign, then digits; none gives 0); R, G and B are used
  modulo 256 [EP-C6, EP-C7, EP-C8]. If the piece has fewer than three commas, the game stops with
  an error report: the error text is written to the game's log, a crash report is produced and the
  program ends (UI layout specification section 1) [EP-C6, EP-E0]. **Disputed in review:** that the program ends is not established (see the review note in UI layout specification section 1); established is that the text is not drawn on with a default colour.

Examples, each code being the last one of its text: `%c[255,200,100,50]Hello` → RGB (200, 100, 50);
`%c[255,200,100]Hello, world` → RGB (200, 100, 0), the third comma being the one after `Hello` and
B read from ` world`; `%c[255,200,100]Hello` → the game stops; `%c[]Hello` → the element's text
colour; `%c[red` (no `]`) → the whole text drawn as it is [EP-C10]. All 22 colour codes of the shipped
string tables, layouts and scripts are `default` or four numbers [EP-C9].

In every case only the red, green and blue come from the code; the **alpha is always the element's
text alpha**. The codes themselves are not drawn.

**Line breaks.** Each `\n` (backslash, `n`) ends the current line; the two bytes are removed.

**Wrapping.** Lines are wrapped to the element width *w* (UI units). Wrapping works on
**segments**: the pieces obtained by cutting the text at every colour code and then cutting each
coloured piece at every `\n`, so a segment contains neither. Positions are counted within each
segment from 0. Character widths are measured in UI units as *w*ᶜ · interval.*x* / *s*ₓ; *r* is the
width of the letter `o` measured the same way.

- The running width *L* of the current line starts at 0 on every new line and carries over from one
  segment to the next segment on the same line.
- Each character of a segment, in order, is **accepted** when *L* + (its width) + *r* ≤ *w*. An
  accepted character's width is added to *L* — except for the last character of a segment, whose
  width is not added (so the following segment on the same line is measured without it).
- **Break points** are whitespace characters of the *current segment only*, at a position other
  than 0, that have been reached since the segment started or since the last break in it (the
  character being tested counts as reached). Whitespace in earlier segments of the line is never a
  break point, and a break point is forgotten once a break has been made.
- When a character is **not accepted** and it is not the last character of its segment: if there is
  a break point, the line ends after the most recent one (the whitespace stays at the end of that
  line) and filling resumes on a new line with the character after it, which is tested again; if
  there is none, the line ends **after the rejected character itself, which stays on the line**,
  and filling resumes on a new line with the next character.
- When the last character of a segment is not accepted, the rest of the segment including that
  character stays on the current line (no break point is used) and the line ends.
- When a segment ends without a rejected character, its text stays on the current line; the line
  ends there if the segment was ended by `\n` and otherwise continues with the next segment.

Whitespace means the ASCII white-space characters (space, tab, line feed, vertical tab, form feed,
carriage return); whether bytes 0x80–0xFF (e.g. the no-break space 0xA0) count was not established.
The text is re-wrapped whenever it, the font or the element width changes.

**Drawing.** Lines are stacked from the top with a pitch of *t* + *g* UI units, where *t* is the line
height of section 4.2 and *g* an extra line gap (0 by default). The text block height is
*n*·(*t* + *g*) − *g* for *n* lines, and `vert_align` positions this block as in simple mode. Each
line is placed at the horizontal anchor *a*ₓ; its segments are drawn one after another, each starting
where the previous segment's width (in UI units) ends, and each drawn with the element's alignment
applied to the segment itself (for a single-colour line this aligns the whole line; for multi-colour
lines that are right or centre aligned each segment is aligned on its own start point).

## 5. Title menu

Fonts and strings used (title-menu spec sections 3.4, 4 and 5):

- Captions: font `graffiti22` (`[ui_font_graffiti22_russian]`), colour (0, 0, 0, 255), simple mode,
  horizontally **centred**, vertically **centred**, in rows of 164 × 30 UI units at absolute
  (700, 419 + 30·*i*); text = string-table lookup of the caption key. The menu code sets simple
  mode and both centre alignments on every item it creates; the layout data does not specify them
  (the row height *h* used for vertical centring is the item height 30, the list's `btn_height`).
- Version caption `static_version` (15, 740, 100 × 30): `<text align="l" font="graffiti19" r="135"
  g="123" b="116" complex_mode="0"/>` — font `graffiti19`, colour (135, 123, 116, 255), simple mode,
  left aligned, top aligned (no `vert_align`), no offset.

Resulting glyph boxes (pixels; left/right of the whole text, top/bottom of the glyph cells):

| Resolution | Texture | `Новая игра` (item 0) | `Выйти в Windows` (item 5) | `ver. 1.0006` |
|---|---|---|---|---|
| 1024 × 768 | `_1024` | width 85: x 740–825, y 423–445 | width 138: x 713–851, y 573–595 | width 79: x 15–94, y 740–759 |
| 800 × 600 | `_800` | width 81: x 570–651, y 328–349 | width 129: x 546–675, y 445–466 | width 66: x 11–77, y 578–595 |
| 1280 × 720 | `_1024` | width 85: x 935–1020, y 395–417 | width 138: x 908–1046, y 536–558 | width 79: x 18–97, y 693–712 |
| 1280 × 1024 | `_1600` | width 122: x 916–1038, y 562–594 | width 195: x 880–1075, y 762–794 | width 115: x 18–133, y 986–1016 |

Derivation for item 0 at 1024 × 768: anchor x = 700 + 164/2 = 782; vertical offset (30 − 22)/2 = 4,
so pen (782, 423); *S* = 10 + 8 + 8 + 8 + 9 + 10 + 9 + 7 + 8 + 8 = 85 (widths of `Н о в а я`,
space, `и г р а`); *x*₀ = 782 − floor(42.5) = 740. At 800 × 600 the widths are
9 + 8 + 8 + 8 + 8 + 9 + 8 + 7 + 8 + 8 = 81. At 800 × 600: pen x = 0.78125 · 782 = 610.9375,
pen y = 0.78125 · (419 + (30 − 21/0.78125)/2) = 328.5625; *x*₀ = 610 − 40 = 570, *y*₀ = 328.

## 6. Not established

- Behaviour of a string-table entry without `<text>` content, and of a `<string>` without `id`.
- The alpha-test comparison function of the `font` shader; the dynamic-lighting renderer's
  compilation of it (assumed identical).
- Which UI data, if any, sets the complex-mode extra line gap.
- Multi-byte (`[mb_symbol_coords]`) text handling beyond the table layout (unused by this edition).
- The exact key-name strings substituted for `$$ACTION_…$$`.
- Whether bytes 0x80–0xFF count as whitespace when choosing a wrap break point (section 4.3).

## 7. Provenance

- **Date:** 2026-11-23. **Edition:** S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG
  distribution), archives `gamedata.db0`…`gamedata.dbd`, original 32-bit binaries in `bin\`.
- **Data examined:** `config\localization.ltx`, `config\fonts.ltx`, all 91 files of
  `config\text\rus\` (the 41 listed tables and their includes, parsed and checked for duplicates,
  entities, markup and byte ranges), `config\ui\color_defs.xml`, `config\ui\ui_mm_main.xml`, all 101
  font metric files `textures\ui\ui_font_*.ini` (sections, completeness, heights) and the DDS headers
  of their textures, one decoded DXT5 glyph, and the `font` entry of `shaders.xr`. Files were
  extracted with the project's own archive tool and analysed with scratch scripts written for this
  purpose.
- **Binaries:** static analysis (Ghidra 12.2, headless) of the engine executable, the game library,
  the XML parser library and the static-lighting renderer library: string references, data tables,
  and reading of the routines that load string tables and fonts, choose texture variants, draw text,
  and lay out text in UI elements. The binaries were not executed.
- **Sources not used:** no leaked or derived engine source code, no third-party reimplementations or
  documentation of the engine's internals; no web searches. Statements of the form "not established"
  mark what neither the data nor the binaries settled.
- **Revision 2026-11-23** (reconciliation with the UI layout specification; same edition and tools,
  plus: a survey of all `font` attribute values in `config\`; a survey of the sections and heights of
  all 101 metric files and of the texture keys of the UI font sections; a search of the game
  library, the engine executable, all scripts and all include lines for the layout files that are
  loaded; and a re-reading in the game library of the font-name lookup and its callers, the error
  path, the simple-mode drawing, the wrapping and the code that builds the title-menu items, and in
  the engine executable of the metric-section choice). Changes against the previously approved text:
  - 2.1: the 11 font names including `di` are confirmed; an unknown non-empty name is now stated to
    be a fatal error (was "reported as an error and the element gets no font"); an absent or empty
    `font` keeps the current font; the three unused values `header`, `normal`, `graffiti18` and the
    eight never-loaded files that contain them are listed.
  - 2.3: the note that every UI font has all three texture keys now excludes `medium`, `small`, `di`.
  - 2.6: metric-section precedence corrected (`[mb_symbol_coords]` before `[symbol_coords]`); the
    two `[symbol_coords]` files without `height` are named (`ui_font_letter_16.ini`,
    `ui_font_letter_18.ini`); `[Char Widths]` is also used by `ui_font_console_01`; `[symbol_coords]`
    is not used by `medium` and `small`.
  - 1.6, 4.2: stated explicitly that simple mode draws colour codes and `\n` literally.
  - 4.3: wrapping rule made precise (segments, break points only in the current segment and never
    at its position 0, no-whitespace break after the rejected character, which stays on the line,
    end-of-segment cases); the whitespace classification of high bytes added to section 6.
  - 5: the vertical centring of the title-menu captions is confirmed from the code that builds the
    menu items (no change to the numbers) and stated to come from that code, not from the data.
- **Revision 2026-11-25** (what a malformed colour code does; same edition and tools, plus a
  re-reading at instruction level in the game library of the cutting of complex-mode text at colour
  codes and of the reading of a code's colour, a static reading of the core library's error-report
  path, and a survey of the colour codes in the shipped string tables, layouts and scripts).
  Bracketed ids [EP-…] name the checks in the investigation workspace that reproduce each statement
  from the binaries and data. Changes against the previously approved text:
  - 4.3: how a code and the pieces are delimited, including an unclosed `%c[`; that any beginning of
    `default` selects the element colour; that the commas are searched to the end of the piece,
    past the `]`; how the numbers are read; that fewer than three commas stop the game (was "a
    missing comma is an error"); examples added.
