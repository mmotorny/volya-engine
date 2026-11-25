# Display gamma

Functional specification of how S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) applies the user's
gamma, brightness and contrast settings to the picture on the display: what the correction acts on, every
moment the game sets it, and what is known about it across a video restart, a lost device and in a
window. Proposal 1 (`display-gamma`) of `docs/behavior/r1-settings-scope.md` (here "RS"), section 2.1.

Built on the approved specifications, which are not repeated here:

- `docs/formats/save-picture.md` (here "SPIC") section 4.1: the colour table computed from the three
  settings and the per-channel balance;
- `docs/behavior/options-screens.md` (here "OS") sections 5.3, 5.5, 6.1 and 7.2: the three track bars,
  when they set the variables, Cancel;
- `docs/behavior/loading-screen.md` (here "LS") section 2: the overlay frames after loading, the
  command-line switch `-gpu_ref`;
- RS sections 2.1, 2.8 and 2.10 (startup order, colour depth 32 bits).

Conventions: the original game was not run. Every statement is **static** (read from the shipped
binaries or data) unless marked **documented** (Microsoft's public Direct3D 9 documentation),
**believed** (inferred, not traced to the end) or **unestablished**. A bracketed id names the check that
backs a statement: `DG-n` in the dirty workspace's `checks/display-gamma/run.py` (helpers `lib_dg.py`,
`scan.py`, `lut.py`); a statement taken from an approved specification names its section instead.
"The engine" is `bin\XR_3DA.exe`, "the first renderer" `bin\xrRender_R1.dll`, "the game library"
`bin\xrGame.dll`. A **frame end** is the step after a frame has been drawn and before it is presented.

## 1. The settings

| Variable | Kind, range | Built-in value | Shipped `user.ltx` |
|---|---|---|---|
| `rs_c_gamma` | real 0.5 – 1.5 | 1.0 | `1.` |
| `rs_c_brightness` | real 0.5 – 1.5 | 1.0 | `1.` |
| `rs_c_contrast` | real 0.5 – 1.5 | 1.0 | `1.` |

[DG-1, DG-8]. All three are registered by the engine and handled identically [DG-1]. The engine also
keeps a separate set of **display colour settings**: gamma, brightness, contrast and the balance values of
SPIC 4.1, all 1.0 when the program starts [DG-3]. The table of SPIC 4.1 is always computed from these
display colour settings, never from the variables directly [DG-2, DG-4, DG-5].

## 2. What the correction acts on

- The engine hands the table of SPIC 4.1 (256 sixteen-bit entries per channel R, G, B) to the Direct3D 9
  device as its **gamma ramp** for swap chain 0, flags value 0 [DG-2, DG-4, DG-5] (**documented**: the
  call sets the ramp of the device's implicit swap chain; the flags choose between passing the table
  unchanged and passing it through a system calibrator; that value 0 is the "no calibration" choice is
  **believed**, the page names the flags without their values).
- The ramp is not part of drawing: no shader or render target uses it, and neither the first renderer
  nor the game library sets or reads the ramp [DG-7]; the first renderer uses the table only for the save
  picture (SPIC 4). It therefore acts on **everything presented**: the 3D view, the HUD, menus, the
  loading screen, the console and the cursor alike.
- **Documented:** the ramp "takes effect immediately; there is no wait for a vertical sync", and it
  "will affect the entire screen (not just the active window if you are running in windowed mode)".
- How the display uses the entries is **believed** (the usual meaning of a hardware gamma ramp; the
  Microsoft pages consulted do not state it): an 8-bit channel value *i* of a presented pixel (the colour
  depth is always 32 bits, RS 2.8) is shown at entry[*i*] / 65535 of full intensity of that channel.
  Under that reading a reimplementation reproduces the effect by mapping every channel of the final
  presented image through the table, as the save picture does (SPIC 4.1), but with the full 16-bit entry
  instead of its high byte.
- The game never reads the ramp in force before it starts, and never sets a ramp when it ends [DG-7].
  Whether the system restores the previous ramp when the device is released or the program exits is
  **unestablished**.

### 2.1 Values for verification

Recomputed from the text of SPIC 4.1 [DG-8]:

- With the shipped values (1.0, 1.0, 1.0) every entry is exactly 257 × *i* (0, 257, …, 65535; sum of
  one channel 8 388 480), i.e. the identity: the shipped settings change no colour at any time.
- Entries for inputs 0, 64, 128, 192, 255 (same for R, G, B; the high bytes are the vectors of SPIC 4.1):

| gamma, brightness, contrast | entries |
|---|---|
| 1.5, 1.0, 1.0 | 0, 26075, 41392, 54239, 65535 |
| 0.5, 1.0, 1.0 | 0, 4128, 16512, 37153, 65535 |
| 1.0, 1.5, 1.0 | 8192, 24640, 41088, 57536, 65535 |
| 1.0, 0.5, 1.0 | 0, 8256, 24704, 41152, 57343 |
| 1.0, 1.0, 1.5 | 0, 12368, 32928, 53488, 65535 |
| 1.0, 1.0, 0.5 | 8192, 20528, 32864, 45200, 57343 |

- Every table is non-decreasing in *i* for these settings [DG-8]. The 16-bit values above come from
  evaluating SPIC 4.1 in double precision with the single-precision rounding it states; the game's own
  power function may differ in the last unit of an entry (**unestablished**; the high bytes agree with
  SPIC 4.1).

## 3. When the ramp is set

The ramp is set in exactly three situations and no others [DG-7]. In each, the only condition is that
the graphics device exists; none tests full-screen or windowed mode [DG-2, DG-4, DG-5].

### 3.1 Setting a variable

Executing `rs_c_gamma`, `rs_c_brightness` or `rs_c_contrast` with a value (from the console, a script
such as `user.ltx`, or the options screen):

1. the variable is set as any real variable (a value outside 0.5 – 1.5 is rejected and the variable keeps
   its value, OS 7) [DG-2];
2. all three current variable values are copied into the display colour settings (gamma, brightness,
   contrast) [DG-2];
3. if the graphics device exists, the table is computed and set as the ramp [DG-2].

So every execution sets the ramp from all three current values, even when the value was rejected or
unchanged. Reading a variable's status does not set it [DG-7]. Before the device exists (while
`user.ltx` runs at startup, RS 2.10) step 3 is skipped and only the display colour settings change.

From the options screen this happens on every movement of the gamma, contrast or brightness track bar,
and again for each of them changed when Apply or Cancel sets it (OS 5.3, 5.5, 6.1); the display therefore
follows the thumb as it moves.

### 3.2 Device creation

When the graphics device is created at startup, after its default render states are set up and before
textures and shaders are created, the ramp is set from the display colour settings [DG-4]. The device is
created after `user.ltx` has run (RS 2.10), so the first ramp reflects the stored values. Creation sets
the warm-up count (3.3) to 0 [DG-4].

### 3.3 End of a warm-up count

The engine keeps a **warm-up count** of frames. Whenever it is set to *n*, it is set to 0 instead if the
program was started with the command-line switch `-gpu_ref` [DG-5]. At every frame end, if the count is
not 0 it is decreased by one, and if it has reached 0 the ramp is set from the display colour settings;
the frame is then ended and presented [DG-5]. So the ramp is set again just before the *n*-th frame
after the count was set is presented. The count is set:

| When | *n* |
|---|---|
| graphics device created (3.2) | 0 (no re-application) |
| level loaded: with the last loading caption `st_client_synchronising` (the overlay frames of LS 2, steps 4–6) | 30 |
| device reset with warm-up: `vid_restart`, recovery of a lost device (section 4) | 20 |
| playback of a recorded camera demo (developer feature) | 50 |

[DG-5, DG-6]. The level-loaded case is the re-application LS 2 step 6 refers to. With `-gpu_ref` none
of these re-applications happens.

## 4. Device resets

A device reset (recreating the swap chain without recreating the device) never sets the ramp itself
[DG-6]. The engine resets the device in these cases [DG-6]:

| Case | Warm-up count afterwards | Ramp re-applied |
|---|---|---|
| `vid_restart` (OS 5.4), when the device has been created | 20 | 20 frames later |
| the device reports that it is lost: the frame is not drawn and the engine waits 33 ms, again at each frame, until the device reports that it can be reset; then it is reset | 20 | 20 frames later |
| starting a level (before loading; LS 2 step 2) | unchanged by the reset; 30 at the end of loading (3.3) | after the 30 overlay frames |
| a developer level-map capture of the camera demo recorder | 20 | 20 frames later |

Whether the device keeps the ramp through a reset, i.e. what is displayed between the reset and the
re-application (the ramp in force before, or no correction), is **unestablished**. **Documented:** a reset
causes "all state information to be lost"; whether that includes the gamma ramp is not stated. With the
shipped settings the ramp is the identity (2.1), so either way nothing is visible.

## 5. Windowed display

When `rs_fullscreen` is off the ramp is set at exactly the same moments [DG-2, DG-4, DG-5].
**Documented:** the ramp affects the entire screen, not only the game's window; if the device does not
support a gamma ramp in the current presentation mode (full-screen or windowed) the call silently does
nothing, and support is reported by a device capability the game does not consult for this. Whether the
ramp has an effect in a window on a given system is therefore **unestablished** (depends on the driver).
The save picture is not made in a window at all (SPIC 2).

## 6. Statement of an approved specification this refines

RS 2.1 states that "a video restart does not itself set" the ramp. That holds for the reset step, but the
restart sets the warm-up count to 20, and the ramp is set again 20 frames later (3.3, section 4) [DG-5,
DG-6].

## 7. Verification facts

1. Shipped `user.ltx`: `rs_c_gamma 1.`, `rs_c_brightness 1.`, `rs_c_contrast 1.`; the ramp is then
   257 × *i* for every entry, one channel summing to 8 388 480 [DG-8].
2. Built-in values 1.0, range 0.5 – 1.5 for all three variables [DG-1].
3. Warm-up counts: 30 after loading, 20 after `vid_restart` or a lost-device reset, 0 at creation, all 0
   with `-gpu_ref` [DG-5, DG-6].
4. The 16-bit test vectors of 2.1 [DG-8].

## 8. Unestablished

- Whether the device keeps the ramp across a reset (section 4).
- Whether the ramp acts in a window on a given system (section 5).
- Whether the previous system ramp is restored when the game exits (section 2).
- The exact display mapping of a 16-bit entry (believed: entry / 65535 of full intensity) and the value
  of flags 0 (believed: no calibration) (section 2).
- The last unit of 16-bit entries away from the shipped values (2.1).

## Provenance

- Date: 2026-11-25. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; the shipped `__support\userdocs\user.ltx`; binaries `bin\XR_3DA.exe`, `bin\xrRender_R1.dll`,
  `bin\xrGame.dll`.
- Binary analysis: static analysis with Ghidra 12.2 (headless, read-only, disassembly and decompilation)
  of the three variables' registration and handler, the engine's display colour settings and their
  initial values, device creation and reset, the frame begin and frame end, the warm-up count and every
  place it is set, the command-line switches of device creation, and the `vid_restart` command; a scan
  of all three binaries for calls of the device's gamma-ramp methods (set and get). Initial values and
  constants read directly from the files. No execution or debugging.
- The table values of 2.1 were recomputed from the text of SPIC 4.1 by a throwaway program (`lut.py`).
- Public documentation: Microsoft Learn pages for the Direct3D 9 device's gamma-ramp method, the gamma
  ramp structure, gamma in Direct3D 9, and device reset.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Whether the balance values of the display colour settings (SPIC 4.1) ever change from their starting value 1.0 is not stated (section 1 gives only their starting value).
- What executing `rs_c_gamma`, `rs_c_brightness` or `rs_c_contrast` with no value does (whether it sets the ramp) is not stated (3.1 covers execution with a value and reading the status).
