# Startup sequence (launch to title menu)

What a player of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) sees and hears from launching the game
until the title menu is shown and accepts input, and which game data each part comes from. The title menu
itself is out of scope; this document only says when "the title menu is shown".

Paths of game data are stored paths in the combined `gamedata\` tree described in `docs/formats/db-archive.md`
(later archives override earlier ones). Archives are abbreviated `db0` … `dbd` for `gamedata.db0` …
`gamedata.dbd`. A file is cited in the copy that wins unless noted. Text files are Windows-1251; configuration
syntax is described in `docs/formats/ltx-config.md`.

Scope: the standard launch, i.e. the way the shipped GOG metadata starts the game (`bin\XR_3DA.exe`, working
directory `bin`, no command-line arguments). Command-line options are not specified here (§8).

## 1. Overview

| # | What is shown | Sound | Ends after (nominal) | Can the player skip it |
|---|---|---|---|---|
| 0 | Only if the system has too little memory: an error box, then the game exits (§2.1) | — | player dismisses the box | — |
| 1 | Splash picture in a small borderless window while the game loads | none | loading finished | no |
| 2 | Logo 1: THQ logo video | THQ jingle | 1 s black + 6.833 s | yes |
| 3 | Logo 2: GSC World Publishing logo video | GSC WP jingle | 1 s black + 25.954 s | yes |
| 4 | Logo 3: GSC Game World logo video | GSC jingle | 1 s black + 13.839 s | yes |
| 5 | Logo 4: two ASUS still images, each fading in and out | none | 9 s | yes |
| 6 | Logo 5: NVIDIA still image fading in and out | none | 5 s | yes |
| 7 | The title menu is shown | (title menu's own) | — | — |

Steps 2–6 are the logo sequence; its nominal length is about 63.6 s. Nothing else (no loading screen,
progress bar, on-screen text or console) appears between the splash window and the title menu other than what
the pictures and videos themselves contain. The same sequence plays on every launch; no first-run-specific
difference was found.

## 2. Conditions under which nothing is shown

### 2.1 Too little memory

Before any window appears, the game checks the amount of memory reported by the operating system, each value
counted in whole MiB (rounded down). It refuses to start if

- physical memory is 500 MiB or less, or
- physical memory plus page-file size (virtual-memory commit limit) is 2500 MiB or less.

In that case a modal error message box (error icon, no parent window) is shown; after it is dismissed the game
exits without showing anything else. The title and text are string resources 2071 and 2072 of `bin\XR_3DA.exe`;
the file provides them in six languages and the operating system's normal resource-language selection picks one.
Russian:

- Title: `Ошибка`
- Text (lines separated by CR LF; the second line is empty):
  `В системе отключен файл подкачки или установлен слишком маленький размер.` /
  (empty line) /
  `Для корректной работы игры требуется файл подкачки размером как минимум 2GB.` /
  `Сделайте необходимые изменения в системные настройки перед тем как запустить игру снова.`

English (UK) resources: `Error` / "The page file is disabled, or the file size set is too small." / (empty
line) / "To make the game work correctly, the page file of minimum 2GB is required." / "Please make all
necessary changes to the system configuration before launching the game again." German, French, Italian and
Spanish versions also exist.

### 2.2 Game already running

If the game is already running, a second launch exits at once and silently: it shows no window and plays no
sound.

## 3. Splash window

Immediately after these checks a splash window appears.

- **Picture:** bitmap resource 116 of `bin\XR_3DA.exe` (500 × 268 pixels, 24 bits per pixel, 402,042-byte
  resource). It shows the S.T.A.L.K.E.R. logo with the subtitle "SHADOW OF CHERNOBYL" in the lower left and a
  gas-masked stalker with a rifle on the right over a dark sepia landscape; its upper left carries small
  English legal text beginning "GSC Game World, © Transavision Ltd 2007. Exclusively published and distributed
  by THQ Inc." and ending "GameSpy and the "Powered by GameSpy" design are trademarks of GameSpy Industries,
  Inc. All rights reserved."
- **Window:** borderless pop-up exactly the size of the picture (shown 1:1), centred on the screen, kept above
  all other windows.
- **Lifetime:** visible for the whole loading period (§4); it disappears at the moment the logo sequence
  starts. If the player closes it (e.g. Alt+F4 while it has focus) it disappears earlier; loading and the rest
  of the startup are unaffected.
- **Audio:** none. **Input:** none apart from closing it.

## 4. Loading period (while the splash is visible)

During the splash the game window appears; its size and mode follow the user settings (e.g. `vid_mode`,
`rs_fullscreen` in `user.ltx`). It shows nothing until the logo sequence starts. Duration depends on the
machine; there is no minimum display time.

The following must be present and readable before the logo sequence can start (data dependencies, not an
order):

- The archives and the path configuration `fsgame.ltx` in the game root (`docs/formats/db-archive.md`). The
  GOG installer generates `fsgame.ltx`, setting `$app_data_root$` to `<Documents>\Stalker-SHOC`.
- `config\system.ltx` (winning copy in `dbc`) and `config\game.ltx`, with their `#include`s. If either is
  missing, the game stops with a message consisting of `Cannot find file <full path>.` and a second line
  `Reinstalling application may fix this problem.` (how the message is presented was not established).
- The user settings file `user.ltx` in the user folder (`$app_data_root$`). The GOG installer copies a default
  one there from `__support\userdocs\user.ltx`; it contains among others `bind quit kESCAPE`,
  `vid_mode 1024x768`, `rs_fullscreen on`, `renderer renderer_r1`.
- Key bindings: the effective bindings are those of `config\default_controls.ltx` (winning copy in `dbb`,
  1,111 bytes, first line `bind quit kESCAPE`) with `user.ltx` applied on top. The "quit" action is therefore
  bound to Esc even if `user.ltx` is absent or has no `bind quit` line.
- `shaders.xr` and the colour-animation library `lanims.xr` (6,665 bytes; the `db0` and `dba` copies are
  identical), the latter providing the fade of the still images (§6.4).
- The texture-description files named in `[texture_desc] files` of `config\system.ltx`; the list ends with
  `ui_asus_intro`, i.e. `config\ui\ui_asus_intro.xml`, which defines the ASUS image regions (§6.2).

## 5. From the splash to the title menu

When loading has finished, the splash disappears and in the same moment the logo sequence `intro_logo` (§6)
starts in the game window, beginning with its first item. The in-game console is not visible.

When the last item of the logo sequence has ended or been skipped, the title menu is shown within one frame.

`config\ui\ui_movies.xml` also defines a second sequence, `intro_game` (the long intro video
`textures\intro\intro_stalker.ogm` in db6, 512 × 288, 5,467 frames, 182.2 s). It is not part of the startup.

## 6. The logo sequence `intro_logo`

### 6.1 Definition

The sequence is the element `<intro_logo>` reached from `config\ui\game_tutorials.xml` (winning copy in `dbb`)
through its line `#include "ui\ui_movies.xml"`. The winning `config\ui\ui_movies.xml` is the `dbb` copy
(13,247 bytes). The `db0` and `dba` copies (12,143 bytes, identical) lack the GSC World Publishing item and the
NVIDIA item; they are overridden.

Content of `<intro_logo>` in the `dbb` copy:

- `<global_wnd x="0" y="0" width="1024" height="768">` holding one full-screen, stretched `auto_static` with
  texture `intro\intro_back` — the background behind every item.
- `<play_each_item>1</play_each_item>`.
- Five `<item>`s, played in document order:
  1. `type="video"`: `delay` 1, `pause_state` on, `sound` `characters_voice\scenario\video\thq_logo_audio`,
     `video_wnd x=0 y=0 width=1024 height=768 stretch=1`, `texture` `intro\thq_logo_hyuv` with attributes
     x=0 y=1 width=720 height=484.
  2. `type="video"`: `delay` 1, `pause_state` on, `sound` `characters_voice\scenario\video\intro_gsc_wp`, same
     window, `texture` `intro\intro_gsc-wp` with x=1 y=1 width=638 height=286.
  3. `type="video"`: `sound` `characters_voice\scenario\video\intro_gsc`, `delay` 1, `pause_state` on, same
     window, `texture` `intro\intro_gsc` with x=0 y=1 width=800 height=358.
  4. `type="image"`: `length_sec` 9, `pause_state` on, `main_wnd` with two full-screen (1024 × 768, stretched)
     `auto_static`s: texture `ui_asus_01` with `start_time="0" length_sec="4"`, and texture `ui_asus_02` with
     `start_time="4" length_sec="4"`; both carry `light_anim="asus_logo_01" light_anim_cyclic="0" la_alpha="1"
     la_texture="1"`.
  5. `type="image"`: `length_sec` 5, `pause_state` on, `main_wnd` with one full-screen `auto_static`: texture
     `ui\ui_nvidia_intro`, `start_time="0" length_sec="4"`, same light-animation attributes.
- A block inside an XML comment (marked "TEST, DONT DELETE") listing other videos; being a comment, it has no
  effect.

All window coordinates are in a virtual 1024 × 768 screen that is stretched to the game window.

### 6.2 Media files

A texture name `X` refers to `textures\X.ogm` for a video item and `textures\X.dds` for a picture; the `sound`
`S` of a video item refers to the two files `sounds\S_l.ogg` and `sounds\S_r.ogg`.

| Data | Winning file (archive) | Size (bytes) | Properties |
|---|---|---|---|
| Background | `textures\intro\intro_back.dds` (db6) | 136 | 4 × 4 DXT1, every texel black |
| THQ video | `textures\intro\thq_logo_hyuv.ogm` (db6) | 1,361,121 | Ogg Theora 720 × 486, 500000/16683 fps (29.97), 203 frames, 6.773 s, video only |
| THQ sound L / R | `sounds\characters_voice\scenario\video\thq_logo_audio_l.ogg` / `_r.ogg` (db9) | 62,025 / 61,577 | Ogg Vorbis mono 44.1 kHz, 6.833 s each |
| GSC WP video | `textures\intro\intro_gsc-wp.ogm` (dbb) | 613,967 | Theora 640 × 288, 25 fps, 136 frames, 5.44 s, video only |
| GSC WP sound L | `sounds\characters_voice\scenario\video\intro_gsc_wp_l.ogg` (dbb; identical copy in dba) | 111,028 | Vorbis mono 44.1 kHz, 25.954 s; below −60 dBFS from about 25.3 s |
| GSC WP sound R | `sounds\characters_voice\scenario\video\intro_gsc_wp_r.ogg` (dbb) | 38,837 | Vorbis mono 44.1 kHz, 5.400 s |
| GSC video | `textures\intro\intro_gsc.ogm` (db6) | 2,398,515 | Theora 800 × 360, 25 fps, 301 frames, 12.04 s, video only |
| GSC sound L / R | `sounds\characters_voice\scenario\video\intro_gsc_l.ogg` / `_r.ogg` (db9) | 114,689 / 115,980 | Vorbis mono 44.1 kHz, 13.839 s each |
| ASUS images | `textures\ui\ui_asus_intro.dds` (db7) | 524,416 | 1024 × 512 DXT5; `config\ui\ui_asus_intro.xml` (`<file_name>ui\ui_asus_intro</file_name>`) defines `ui_asus_01` = x 0, y 0, 512 × 512 (left half) and `ui_asus_02` = x 512, y 0, 512 × 512 (right half) |
| NVIDIA image | `textures\ui\ui_nvidia_intro.dds` (dbb) | 131,200 | 512 × 512 DXT1 |
| Fade curve | entry `asus_logo_01` of `lanims.xr` | — | see §6.4 |

Content, for recognition: the THQ video animates the chrome "THQ" logo onto black; the GSC WP video shows a
glowing GSC logo resolving into a red "GSC WORLD PUBLISHING" logo on black; the GSC video flies through a
metallic corridor and ends on the chrome "GSC GAME WORLD" logo on black. `ui_asus_01` is the white ASUS logo
with "Rock Solid · Heart Touching"; `ui_asus_02` is the ROG emblem with "ASUS Gaming Series" and a red
"Let's game"; the NVIDIA image is the "The way it's meant to be played" badge. The top and bottom pixel rows of
the decoded video frames contain coloured garbage; the texture rectangles in §6.1 exclude them.

Overridden copies not used at launch: `textures\intro\intro_gsc-wp.ogm` in dba (9,385,148 bytes, 800 × 360,
20.04 s) and `sounds\characters_voice\scenario\video\intro_gsc_wp_r.ogg` in dba (110,101 bytes, 25.954 s).
Also present but not used by the startup: `textures\intro\intro_thq.ogm` and the files in `sounds\intro\`.

### 6.3 Video items

What the player sees and hears during a video item, from the moment it becomes current:

1. For `delay` seconds (1 s for all three items; an absent, zero or negative `delay` means none) only the black
   background is visible and nothing is heard.
2. Then the video appears and both sound files start, together with the video's first frame (in practice
   within one or two rendered frames after the delay).
3. While the left sound plays, the picture is synchronised to it: the frame shown is the one for the left
   sound's current playback position. When the video reaches its last frame it stops, and that last frame
   stays on screen.
4. The item ends when the **left** sound file has finished. (An item with no sound would end when its video
   ends.)

Resulting durations with the shipped data (delay + left sound):

| Item | Duration | Video length | Last frame held for |
|---|---|---|---|
| THQ | 7.833 s | 6.773 s | about 0.06 s |
| GSC WP | 26.954 s | 5.44 s | about 20.5 s (the red GSC World Publishing logo on black, while the left channel continues; the right channel ends after 5.4 s) |
| GSC | 14.839 s | 12.04 s | about 1.8 s |

Sound: each of the `_l` and `_r` files plays once (not looped) as a mono source positioned relative to the
listener at x = −0.5 and x = +0.5 respectively (y = 0, z = 0.3), giving a left/right pair. Which volume
setting applies was not established.

Placement: the `texture` attributes x, y, width, height select the part of the decoded frame (in pixels) that is
shown. Because the `video_wnd` has no `fullscreen="1"` attribute, its own x/y/width/height are not used for the
picture's size. Instead the shown picture is centred on the virtual screen at (512, 384), is 1024 virtual units
wide, and is 1024 × (rectangle height ÷ rectangle width) virtual units tall. If the game window's width divided
by its height is greater than 1.3433334 (wider than 4:3), that height is additionally multiplied by 1.328 so
the picture keeps its proportions after the virtual screen is stretched. The rest of the screen shows the black
background.

| Item | Rectangle | Height, 4:3 window | Height, wide window |
|---|---|---|---|
| THQ | 720 × 484 | 688.4 | 914.2 (exceeds 768: top and bottom are cut off by the screen edges) |
| GSC WP | 638 × 286 | 459.0 | 609.6 |
| GSC | 800 × 358 | 458.2 | 608.6 |

`pause_state on`: no visible or audible effect at launch was identified (no pause indication is displayed).

### 6.4 Image items

An image item lasts exactly `length_sec` seconds from the moment it becomes current and is silent. Each of its
pictures (`auto_static`s) is visible while the time since the item started is strictly greater than its
`start_time` and strictly less than `start_time + length_sec`; otherwise only the black background shows
there.

A picture's opacity follows the colour animation named in `light_anim` (`asus_logo_01`), which starts from its
beginning each time the picture becomes visible. With `la_alpha="1"` only the opacity is taken from the
animation; the picture's own colours are unchanged. The entry `asus_logo_01` of `lanims.xr` (its name starts
at byte offset 6,600 of the file) has 30 frames per second, 120 frames (4.0 s) and four keys, colour values
stored as 32-bit ARGB:

| Key frame | Time | Value | Alpha (0–255) |
|---|---|---|---|
| 0 | 0.000 s | `0x00000000` | 0 |
| 32 | 1.067 s | `0xFF000000` | 255 |
| 85 | 2.833 s | `0xFF000000` | 255 |
| 119 | 3.967 s | `0x00000000` | 0 |

The animation advances in whole frames (elapsed seconds × 30, fraction discarded) and is interpolated
linearly between keys. Whether it would repeat after 4 s makes no visible difference, since no picture is
visible for longer than 4 s.

Resulting timeline:

- ASUS item (9 s): `ui_asus_01` fades in over about 1.07 s, is fully opaque until 2.83 s and has faded out by
  3.97 s; from 4 s `ui_asus_02` does the same until 8 s; 8–9 s black.
- NVIDIA item (5 s): the badge fades in, holds and fades out within 0–4 s; 4–5 s black.

All pictures are stretched to the full 1024 × 768 virtual screen (the 512 × 512 sources are therefore stretched
non-uniformly).

### 6.5 Input during the logo sequence

- Pressing a key bound to the **quit** action (Esc by default, §4) immediately ends the current item: its
  picture and sound stop and the next item starts at once; during the last item it ends the sequence and the
  title menu is shown. Each key press advances by exactly one item (it acts on press, not release). This holds
  for every item of `intro_logo` because `play_each_item` is 1, no video item sets `can_be_stopped` (default
  on, i.e. stoppable) and no image item sets a `guard_key` (without one an image item is stoppable from its
  start).
- Every other key and mouse input is ignored: no console, no menu, no other effect.
- Any key bound to "quit" in the effective bindings (§4) works, including a second key bound to it.

## 7. Audio summary

- Memory error box, splash and loading period: silent.
- Logo sequence: only the sound pairs of the three video items, each starting with its video after 1 s of
  black. Image items are silent. No music plays between or under the items.
- Title menu: its own audio (out of scope).

## 8. Command line

The shipped GOG metadata (`goggame-1207660573.info`, `playTasks`) starts `bin\XR_3DA.exe` with working
directory `bin` and no arguments; `Settings.exe` is a separate tool and takes no part in the sequence. This
document specifies only that launch. The only launch options mentioned in shipped material are in a developer
comment near the top of `scripts\ce_switcher.script`
(`-i -noprefetch -ltx user_andy.ltx -external -start server(andy_test_switch/single) client(localhost)`); their
effect on the startup is not specified here.

## 9. Not established

- How the "Cannot find file" message is presented.
- Effect of the game window losing focus during the logo sequence.
- Which volume setting applies to the logo sounds.
- Exact rounding of interpolated alpha to 8 bits (believed to discard the fraction).
- Mouse-cursor visibility during the logo sequence.
- Any effect of command-line options on the sequence.
- Timings in §6.3–6.4 were derived from the data and static analysis, not measured on a running game.

## Provenance

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build; `goggame-1207660573.info` language
  "Russian"), archives `gamedata.db0` … `gamedata.dbd`, binaries in `bin\` (`XR_3DA.exe` 1,636,424 bytes,
  `xrGame.dll` 6,246,400 bytes), GOG installer script `goggame-1207660573.script`, and the default user files in
  `__support\userdocs\`.
- Data analysis: every archive listed and relevant entries extracted with the project's archive tool
  (`volya-engine-db list|dump`), precedence applied as in `docs/formats/db-archive.md`; text read as
  Windows-1251. Videos and sounds examined with ffprobe/ffmpeg (stream parameters, frame counts by decoding,
  silence detection, still frames for recognising content). DDS headers and texels and the `lanims.xr` entry
  read byte by byte. PE resources (bitmap, string table) of `XR_3DA.exe` extracted with a small resource
  reader and decoded.
- Binary analysis: static analysis of `XR_3DA.exe` and `xrGame.dll` with Ghidra to establish behaviour
  (memory thresholds, defaults of XML fields, timing rules, input handling, placement constants). The binaries
  were not executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.
