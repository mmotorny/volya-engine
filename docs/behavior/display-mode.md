# Display mode and statistics overlay

Functional specification, in two parts, of proposals 8 (`display-mode`) and 9 (`statistics-overlay`)
of `docs/behavior/r1-settings-scope.md` (here "RS"), sections 1, 2.8, 2.11 and 5, for S.T.A.L.K.E.R.:
Shadow of Chernobyl (Russian edition):

- **Part A, display mode:** the game window, full screen or windowed (`rs_fullscreen`), the screen size
  (`vid_mode`), the refresh rate (`rs_refresh_60hz`), vertical sync (`rs_v_sync`), the colour depth, at
  device creation and at every video restart; with the corrections of
  `docs/behavior/options-screens.md` 6.2 and 9 that RS section 5 calls for (section A.10).
- **Part B, statistics overlay:** what `rs_stats` makes the engine draw, where, and what each value is.

Built on, and not repeated here: `docs/behavior/options-screens.md` (here "OS") sections 5.4, 6.1, 6.2,
7, 9 and 12; `docs/formats/text-and-fonts.md` (here "TF") sections 2 and 3; RS sections 2.8 and 2.10
(startup order); `docs/behavior/display-gamma.md` (here "DG") sections 3 and 4 (warm-up count, device
resets).

Conventions: the original game was not run. Every statement is **static** (read from the shipped
binaries or data) unless marked **documented** (Microsoft's public Direct3D 9 documentation),
**believed** (inferred, not traced to the end) or **unestablished**. A bracketed id names the check that
backs a statement: `DM-n` and `ST-n` in the dirty workspace's `checks/display-mode/run.py` (helpers
`lib_dm.py`, `pe_exports.py`). "The engine" is `bin\XR_3DA.exe`, "the first renderer"
`bin\xrRender_R1.dll`, "the game library" `bin\xrGame.dll`, "the core library" `bin\xrCore.dll`, "the
network library" `bin\xrNetServer.dll`. *W* × *H* is the screen size in pixels in force (A.5). A
**video restart** is the console command `vid_restart` (OS 5.4); a **device reset** is any of the resets
of DG section 4, `vid_restart` among them. Direct3D 9 format names are those of Microsoft's `D3DFORMAT`
page (**documented** values: X8R8G8B8 22, A8R8G8B8 21, R8G8B8 20, R5G6B5 23, X1R5G5B5 24, X4R4G4B4 30,
D24S8 75, D24X4S4 79, D32 71, D24X8 77, D16 80, D15S1 73).

# Part A. Display mode

## A.1 The variables

| Variable | Kind | Built-in value | Shipped `user.ltx` |
|---|---|---|---|
| `rs_fullscreen` | on/off | on | `on` |
| `rs_refresh_60hz` | on/off | off | `on` |
| `rs_v_sync` | on/off | off | `off` |
| `rs_stats` | on/off | off | `off` |
| `vid_mode` | resolution `W`x`H` | `1024x768` | `1024x768` |

[DM-1, DM-2]. All are registered by the engine. The four on/off variables are bits of one engine flag
word, which the renderers and the game library read directly [DM-1]; setting one changes only the bit
(OS 7.1), with no other action. The colour depth is a separate engine value, 32, that nothing writes
(A.4) [DM-6].

`vid_mode <text>`: if the text begins with two decimal integers separated by `x` (scanned as
`%dx%d`; anything after the second number is ignored), they become the width and height; otherwise the
line `! Wrong video mode [<text>]` is logged and the value is kept [DM-3]. Any pair of integers is
accepted; the value is used only at the next device creation or reset (A.5). Its status is `<W>x<H>`,
its help text `change screen resolution WxH`, and the list it offers (the options screen's resolution
drop-down, OS 6.1) is the resolution list of A.6 [DM-3].

## A.2 The window

At startup, before the user configuration file runs (RS 2.10), the engine creates its one window
[DM-4]:

- window class `_XRAY_`, title `S.T.A.L.K.E.R.: Shadow Of Chernobyl`, the program's icon, the standard
  arrow cursor, a black background brush;
- style: caption only (not yet visible), at the system's default position, sized so that its client
  area is 640 × 480.

After `user.ltx` has run, the window is shown in its normal state, and then the graphics device is
created (A.3) [DM-4]. At the end of device creation and at the end of every device reset the window
style is set from `rs_fullscreen` [DM-5]:

| `rs_fullscreen` | Window |
|---|---|
| on | style popup + visible (no caption or border); position and size are left to the full-screen device |
| off | style visible + caption + system menu + minimise box (no maximise box, not resizable); placed so that its client area is *W* × *H* with the client's top-left at (0, 0) of the desktop, or, with the command-line switch `-center_screen`, centred on the desktop's client area ((desktop − size) / 2, integer halves); placed on top, shown, frame redrawn |

In both cases the mouse cursor is then hidden and the window is brought to the foreground [DM-5].
(The style values are read as the Win32 window-style bits WS_POPUP | WS_VISIBLE and WS_VISIBLE |
WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX: **believed**, from the documented meaning of the numbers.)

### A.2.1 Window messages

The window's message handling [DM-13, DM-14]:

- **Activation:** the window counts as *active* when it is activated and not minimised. On becoming
  active the cursor is hidden, on becoming inactive it is shown; each change also runs the engine's list
  of subscribers to activation changes [DM-14].
- **Close** (the window's close button or Alt+F4's close request) is ignored; the game ends only from
  its own menus or console.
- The system commands **maximise, size, move** and **monitor power** are ignored. The cursor is never
  set by the window (the request is answered as handled).
- While the device is not ready the main loop waits 100 ms per pass; while the window is inactive no
  frame is drawn and the loop waits 1 ms per pass [DM-14].

## A.3 Device creation

The graphics device is created once, at startup, after the window is shown [DM-4]. Steps that concern
the display:

1. **Windowed or full screen:** windowed when `rs_fullscreen` is off [DM-6].
2. **Back-buffer format** [DM-6]:
   - windowed: the desktop's current display format (the device type is checked for it in a window);
   - full screen: the first of X8R8G8B8, A8R8G8B8, R8G8B8 that the adapter accepts as both display and
     back-buffer format in full screen (A.4: the colour depth is always 32);
   - if none is accepted, a message box titled `Error!` with the text `Failed to initialize graphics
     hardware.` / `Please try to restart the game.` (two lines) is shown and the program ends. The same
     happens if creating the device reports that it is lost.
3. **Depth buffer format:** the first of D24S8, D24X4S4, D32, D24X8, D16, D15S1 that the adapter
   accepts as a depth-stencil surface and that matches the back-buffer format (with the second
   renderer: D24S8 without testing) [DM-12].
4. **Resolution list and screen size:** A.6, A.5.
5. **Presentation parameters** [DM-7]:

| Parameter | Value |
|---|---|
| back-buffer size | *W* × *H* (A.5) |
| back-buffer format | step 2 |
| back buffers | 1 |
| multisampling | none |
| swap effect | value 1 full screen, 3 windowed (**believed**: discard and copy) |
| windowed | as step 1 |
| automatic depth buffer | yes, format step 3 |
| flags | none |
| full-screen refresh rate | A.7 (windowed: 0) |
| presentation interval | always 1 (A.8) |

6. The engine's screen size becomes the back-buffer size, its centre half of it (integer halves)
   [DM-11]. The gamma ramp, the warm-up count (DG 3.2), and the window style (A.2) follow.

## A.4 Colour depth

The engine's colour depth value is 32 and is never written; it is read only in step 2 of A.3, so the
full-screen format is always one of the three 32-bit formats [DM-6]. (The 16-bit alternatives R5G6B5,
X1R5G5B5, X4R4G4B4 are never tried.) A windowed device uses the desktop's format, whatever its depth.
`vid_bpp` set by the preset files is not a variable (RS 2.10) and has no effect.

## A.5 Screen size at creation and reset

At device creation and at every device reset the back-buffer size is chosen [DM-10]:

- **windowed:** exactly the current `vid_mode`, even if the adapter does not list it;
- **full screen:** the text `<W>x<H>` of the current `vid_mode` is looked up in the resolution list
  (A.6; case-insensitive). If it is not there, the console command `vid_mode <first entry of the list>`
  is executed, which sets the variable, and that size is used.

The engine's screen size and centre are then taken from the chosen size (A.3 step 6) [DM-11]. After a
reset, if the aspect ratio *W* / *H* has crossed 1.3433 (from at most to above, or back), the running
level is notified; what it does then is outside this specification [DM-11].

## A.6 The resolution list

- Built once, the first time a size is chosen at device creation, from the display modes the adapter
  lists for the back-buffer format chosen at creation (A.3 step 2) [DM-10]. A device reset keeps the
  list; it is freed only when the device is destroyed at exit [DM-10]. So when the game was started
  windowed the list holds the modes of the desktop's format, also after a switch to full screen.
- Modes narrower than 800 pixels are skipped; each other mode is written `<width>x<height>` (decimal)
  and added unless the same text (case-insensitive) is already in the list; the order is the adapter's
  enumeration order; entry *k* has the value *k* [DM-10]. (As OS 9.)

## A.7 Refresh rate

Chosen at device creation and at every reset, full screen only (windowed: 0) [DM-7, DM-8]:

- `rs_refresh_60hz` **on:** 0, i.e. the adapter's default rate (**documented**: a full-screen rate of 0
  means the default). Despite its name the setting does not request 60 Hz.
- **off:** the highest refresh rate among the adapter's modes of exactly *W* × *H* in the format of
  A.3 step 2; 0 (default) if there is none.

## A.8 Vertical sync

`rs_v_sync` has **no effect** [DM-9]. Creation and every reset present with interval 1 regardless
[DM-7]; the only code that reads the bit (a choice between waiting for the vertical retrace and
presenting immediately) is never called. **Documented:** with presentation interval "one" the driver
waits for the vertical retrace and completes at most one presentation per display refresh, in a window
as in full screen (that value 1 is "one" is **believed**, from the headers). So the frame rate is
limited to the display's refresh rate at all times, and frames never tear. `rs_no_v_sync`, named in the
options screen's restart list (OS 5.4), is not a variable [DM-9].

## A.9 Device reset (video restart)

`vid_restart` resets the device only when it has been created [DM-11]. Each device reset, for the
display [DM-5, DM-7, DM-10, DM-11]:

1. reads `rs_fullscreen` again (windowed or full screen) and chooses the size (A.5) and refresh rate
   (A.7); presentation interval 1;
2. keeps the back-buffer format and depth format chosen at creation (not chosen again);
3. resets the device; while that fails it logs `! ERROR: [<W>x<H>]: <error text>`, waits 100 ms and
   tries again, without limit;
4. sets the window style (A.2), updates the screen size (A.5), then notifies the engine's subscribers
   of the reset (e.g. fonts, TF 2.3).

So switching between full screen and window, the size, and the refresh rate all take effect at the
next video restart, which the options screen marks for `rs_fullscreen`, `vid_mode` and
`rs_refresh_60hz` (OS 5.4, 6.1, 6.2).

## A.10 Corrections of `options-screens.md`

- **OS 6.2, row 11** (`check_vsync`, `rs_v_sync`), column Effect: "on Apply; used at the next video
  restart (does not itself mark one)" should read "on Apply; no effect: the display always waits for
  the vertical retrace (display-mode A.8)".
- **OS 9, first item:** "(full screen: 32-bit colour if `vid_bpp` is 32 — X8R8G8B8, else A8R8G8B8, else
  R8G8B8 — or for 16-bit R5G6B5, else X1R5G5B5, else X4R4G4B4, the first the adapter supports;
  windowed: the desktop's format)" should read "(full screen: X8R8G8B8, else A8R8G8B8, else R8G8B8, the
  first the adapter supports; the colour depth is always 32 and `vid_bpp` is not a variable; windowed:
  the desktop's format)". Also in that item, "a video restart resets the device and is believed to keep
  the list" is established: the list is built once from the format chosen at device creation and kept
  across every reset (A.6).
- OS 7.2's `rs_refresh_60hz` row ("on: the display's default refresh rate; off: the highest refresh
  rate the adapter lists for the chosen resolution") agrees with A.7, the highest rate being among
  modes of the back-buffer format.

# Part B. Statistics overlay

## B.1 When it is drawn

Each frame that is drawn (window active, A.2.1), after everything else of the frame has been drawn and
before the frame is ended and presented, the engine draws the **overlay** when `rs_stats` is on [ST-1].
(Two further conditions exist but never hold in this edition: a flag bit that nothing sets and no
variable names, and a list of entries that nothing adds to [ST-1].) The overlay is therefore drawn over
the 3D view, HUD, menus and console alike.

Measurement: while `rs_stats` is on (tested at the start of each frame) the engine and the other
libraries measure the times and counts shown below; while it is off nothing is measured [ST-1]. Turning
it on takes effect from the next frame.

## B.2 Font and layout

- Font `stat_font` of `config\fonts.ltx` (TF 2.1, 2.2): texture `ui\ui_font_hud_01` (fixed 8 × 8
  cells, TF 2.6), interval 0.75, 1, screen-relative size; created at device creation [ST-2].
- The overlay sets the font height to **0.01 × *H*** pixels (e.g. 7.68 at 1024 × 768) [ST-4]. Glyphs are
  drawn as in TF 3: 8-pixel cell widths, each advancing 0.75 × 8 = 6 pixels, stretched to the font
  height.
- **Lines:** each line is drawn at the current pen position, which then moves down by the height ×
  1 [ST-2]; a **blank line** moves it down the same amount. The text of a line is formatted with C
  `printf` rules to at most 511 bytes; an empty result draws nothing but still advances [ST-2].
- Strings are queued and drawn together at the end of each block (B.3–B.6; blocks 2 and 3 share one queue and are drawn together), in queue order.

## B.3 Block 1: evaluation notice

On frames whose frame count modulo 2000 is below 1000, three lines are drawn in white at (0, 0) at the
font's height then in force: the values of `line1`, `line2`, `line3` of section `[evaluation]` of the
system configuration (quotes removed) [ST-3]. The section and the three lines must exist at device
creation, else it is a fatal error [ST-3]. The shipped `config\evaluation.ltx` (included by
`system.ltx`) sets all three to `""`, so nothing is drawn [ST-3]. A further centred caption `--= tune =--`
depends on an engine value that is 0 and never set, and is never drawn [ST-3].

## B.4 Block 2: engine statistics

White, height 0.01 × *H*, pen at (0, 0) [ST-4]. The lines, top to bottom (exact texts; spaces as
shown; "—" a blank line; "—×2" two blank lines) [ST-4]:

```
FPS/RFPS:    %3.1f/%3.1f
TPS:         %2.2f M
VERT:        %d/%d
POLY:        %d/%d
DIP/DP:      %d
xforms:      %d
—
*** ENGINE:  %2.2fms
Memory:      %2.2fa
uClients:    %2.2fms, %2.1f%%, crow(%d)/active(%d)/total(%d)
uSheduler:   %2.2fms, %2.1f%%
uSheduler_L: %2.2fms
uParticles:  Qstart[%d] Qactive[%d] Qdestroy[%d]
spInsert:    o[%.2fms, %2.1f%%], p[%.2fms, %2.1f%%]
spRemove:    o[%.2fms, %2.1f%%], p[%.2fms, %2.1f%%]
Physics:     %2.2fms, %2.1f%%
  collider:  %2.2fms
  solver:    %2.2fms, %d
aiThink:     %2.2fms, %d
  aiRange:   %2.2fms, %d
  aiPath:    %2.2fms, %d
  aiNode:    %2.2fms, %d
aiVision:    %2.2fms, %d
  Query:     %2.2fms
  RayCast:   %2.2fms
—
*** RENDER:  %2.2fms
R_CALC:      %2.2fms, %2.1f%%
  HOM:       %2.2fms, %d
  Skeletons: %2.2fms, %d
R_DUMP:      %2.2fms, %2.1f%%
  Wait-L:    %2.2fms
  Wait-S:    %2.2fms
  Skinning:  %2.2fms
  DT_Vis/Cnt:%2.2fms
  DT_Render: %2.2fms
  DT_Cache:  %2.2fms
  Wallmarks: %2.2fms, %d/%d - %d
  Glows:     %2.2fms
  Lights:    %2.2fms, %d
  RT:        %2.2fms, %d
  HUD:       %2.2fms
  P_calc:    %2.2fms
  S_calc:    %2.2fms
  S_render:  %2.2fms, %d
—
*** SOUND:   %2.2fms
  TGT/SIM/E: %d/%d/%d
  HIT/MISS:  %d/%d
—
Input:       %2.2fms
clRAY:       %2.2fms, %d, %2.0fK
clBOX:       %2.2fms, %d, %2.0fK
clFRUSTUM:   %2.2fms, %d
—
netClientRecv:   %2.2fms, %d
netClientSend:   %2.2fms, %d
netServer:   %2.2fms, %d
—×2
TEST 0:      %2.2fms, %d
TEST 1:      %2.2fms, %d
TEST 2:      %2.2fms, %d
TEST 3:      %2.2fms, %d
—
qpc[%3d]
—
static:        %3.1f/%d
flora:         %3.1f/%d
  flora_lods:  %3.1f/%d
dynamic:       %3.1f/%d
  dynamic_sw:  %3.1f/%d
  dynamic_inst:%3.1f/%d
  dynamic_1B:  %3.1f/%d
  dynamic_2B:  %3.1f/%d
details:       %3.1f/%d
```

(`%%` prints `%`. `DT_Vis/Cnt` prints one number although a second is supplied.)

### B.4.1 Values

**Timers.** Most `ms` values are one of 45 engine timers [ST-5]. A timer adds up the durations of the
measurements made of it in a frame (high-resolution performance counter) and counts them; when the
overlay is drawn, the frame's total *t* in milliseconds updates the shown value *v*: *v* = *t* if
*t* > *v*, else *v* = 0.99 · *v* + 0.01 · *t* (a peak that decays slowly); then the frame's total and
count restart from 0 [ST-5]. Shown values start at 0. In every `%2.2fms, %d` line the number is that
timer's count of measurements in the frame [ST-5]. A `%2.1f%%` after a time is that time as a
percentage of the `*** ENGINE` value (lines from `uClients` to `RayCast`) or of the `*** RENDER` value
(`R_CALC`, `R_DUMP`) [ST-5]. Which code each timer surrounds (what exactly `aiThink`, `HOM`, `DT_Cache`
and the others measure) is **unestablished**; the labels are the only description. The `o[…]` and
`p[…]` parts of `spInsert`/`spRemove` are two further timers of the same kind each (not among the 45), for two spatial databases (believed:
objects and physics).

**Other values** [ST-5]:

| Line | Value |
|---|---|
| FPS | *F* ← 0.7 · *F* + 0.3 / Δ, Δ the frame time in seconds, each drawn frame with Δ > 10⁻⁷; starts at 30 |
| RFPS | *R* ← 0.7 · *R* + 300 / *r*, *r* the `*** RENDER` value in ms, on frames where FPS is updated and *r* > 10⁻⁷; starts at 30 (i.e. a smoothed 1000 / *r*) |
| TPS | *T* ← 0.7 · *T* + 0.3 · *p* / (1000 · *r*), *p* the frame's polygon count (POLY), on the same frames as RFPS; starts at 0 (millions of polygons per second) |
| VERT, POLY | (believed from the labels) the frame's vertex and polygon counts drawn, and each divided by DIP/DP (integer; 0 if no calls) |
| DIP/DP | (believed) the frame's number of draw calls |
| xforms | (believed) the frame's number of transform changes |
| `*** RENDER` | the time from just before a frame's drawing begins to after it has been ended and presented, of the previous frame (smoothed as a timer) [ST-5] |
| Memory | *M* ← *c* if *c* > *M*, else 0.9 · *M* + 0.1 · *c*, *c* the number of memory-manager calls since the previous overlay; the counter then restarts. **Disputed in review:** only the update rule and the restart are backed; that the counter counts memory-manager calls is not backed by a check (it is a counter of the core library's memory manager; what increments it is unestablished) |
| uClients crow/active/total, uParticles Q… | counts kept by the game; the uParticles counts restart after each overlay. **Disputed in review:** these are values of the engine's statistics record; that the game library writes them, and the restart of the uParticles counts, are not backed by a check |
| uSheduler_L | a time kept by the game (not a timer). **Disputed in review:** a value of the engine's statistics record; who writes it is not backed by a check |
| TGT/SIM/E, HIT/MISS | five numbers reported by the sound system each overlay; their meaning is **unestablished** beyond the labels (believed: targets, simulated sources, events; cache hits and misses) |
| clRAY, clBOX `K` | *K* ← 0.99 · *K* + 0.01 · (count / time in ms) of that timer, i.e. thousands of queries per second, smoothed |
| qpc | number of reads of the performance counter since the previous overlay; then restarts |
| static … details | first number: a count divided by 1024; second: a count; kept by the drawing back end per kind of geometry; which counts (believed: vertices and calls) is **unestablished** |

Where these counts are reset and which drawing feeds each is **unestablished** except as stated.

## B.5 Block 3: renderer and game statistics

After block 2, at height 0.01 × *H* [ST-6]:

- the running renderer's block, pen at (200, 0). The first renderer draws: ` **** Occ-Q(%03.1f) **** `
  (100 × culled / total, total taken as 1 if 0), ` total  : %2d`, ` culled : %2d`, then one blank line;
  both counts then restart from 0 [ST-6]. What the two counts count is **unestablished** (believed:
  occlusion tests issued, and those that found the object hidden).
- the game's block, pen at (400, 0): empty in this edition [ST-6].

## B.6 Block 4: camera position

One line `CAMERA POSITION:  [%3.2f,%3.2f,%3.2f]` (believed: the camera's world position x, y, z), white, at height
0.02 × *H*, drawn at the fixed pixel position (10, 600) regardless of the screen size (so it is
below the bottom edge at 800 × 600) [ST-6]. Then all timers start a new frame [ST-6].

## B.7 Network lines of the game library

While `rs_stats` is on and a level is running, the game library draws network statistics (server:
`IN:`/`OUT:`, `client_2_sever ping: %d`, `SPS/Sended : %4d/%4d`, `sv_urate/cl_urate : %4d/%4d`, one
`P(%d), BPS(%2.1fK), …` line per client, partly in green) — but only when the connection is **not
direct** [ST-7]. The network library makes the connection direct when its options contain `/single`
and the command line lacks `-no_direct_connect` [ST-7]; the shipped single-player commands contain
`/single` (e.g. `start server(all/single/alife/new) client(localhost)`) [ST-7], so in single-player
these lines are not drawn (**believed**: which options text the network library tests was not traced).
Their font, layout and values are not specified here.

## B.8 Verification facts

1. Shipped values: `rs_fullscreen on`, `rs_refresh_60hz on`, `rs_v_sync off`, `rs_stats off`,
   `vid_mode 1024x768`; with them the game starts full screen at 1024 × 768 (if the adapter lists it),
   default refresh rate, X8R8G8B8 if supported, synchronised to the display, no overlay [DM-1, DM-2].
2. At 1024 × 768 the overlay's lines are 7.68 pixels apart; block 2 has 75 entries (67 text lines,
   8 blank-line steps of which one is double) [ST-4].
3. FPS and RFPS show 30.0 before the first measurement [ST-5].

## B.9 Unestablished

- What each engine timer surrounds; the meaning of the sound, occlusion and geometry counts (B.4, B.5).
- Which options text the network library tests for `/single` (B.7).
- The game's reaction to crossing aspect 1.3433 at a reset (A.5).
- The numeric meaning of the swap-effect, presentation-interval and window-style values (believed
  names given in A.2, A.3, A.8).

## Provenance

- Date: 2026-11-25. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; the shipped `__support\userdocs\user.ltx`; game data `config\fonts.ltx`,
  `config\evaluation.ltx`, `config\system.ltx`, `scripts\ui_main_menu.script` from the archives
  (`gamedata.db0` and `gamedata.dbb` both hold the same `evaluation.ltx` text).
- Binary analysis: static analysis with Ghidra 12.2 (headless, read-only, disassembly and decompilation)
  of the engine (variable registration, window creation and window procedure, device creation and
  reset, format, depth, resolution and refresh selection, the resolution list, the main loop, the
  statistics overlay and its font), the core library (timers, configuration string reading), the first
  renderer (its statistics block) and the game library (its statistics entry, network lines); export
  and import tables, constants and strings read directly from the files; a byte search of the network
  library for its direct-connection rule. No execution or debugging.
- Public documentation: Microsoft Learn pages for `D3DFORMAT` and `D3DPRESENT`.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- OS 5.4's restart step says a video restart resets the device "with the current resolution, full-screen, refresh and vertical-sync settings"; A.10 lists no correction of "vertical-sync" there, although A.8 makes it have no effect.
- Loss of the device during play (e.g. switching away from a full-screen game): when the device counts as not ready (A.2.1), and how and when it is reset and the display mode restored, are not specified.
