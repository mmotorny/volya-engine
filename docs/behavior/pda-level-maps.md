# Level maps

Format specification of the level-map data of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): the
configuration `config\game_maps_single.ltx` (its list of level maps, the global map, the per-level sections),
the pictures it names (`textures\map\`, `textures\ui\ui_global_map`, `textures\ui\ui_nomap2`), and how a level
position maps into a level map's picture and a level's rectangle into the global map. It is PD-3 of the PDA
scoping specification (`pda-scope` 4). The PDA map tab's behaviour (switching maps, zooming, centring, spots,
hints) is PD-4's; the minimap's is `hud-scope` spec 10's; both build on this specification.

It builds on the approved specifications `ltx-config` (reading, case of section names and keys, entry order,
vector values), `ui-layout` 5.2, 5.3 and 6.3 (pictures and texture paths) and `level-loading` (level names,
level coordinates, `level.ltx`), cited by name.

Conventions:

- Every fact carries its check, a script in `checks/level-maps/` of the dirty workspace: D = `data.py` (game
  data), B = `bin.py` (static analysis of `bin\xrGame.dll` and `bin\xrCore.dll`), M = `model.py` (worked
  examples), R = `run.py` (the black-box run made by `drive.py`), followed by the label of the assertion.
- Level coordinates are `level-loading`'s: metres, x and z horizontal, y vertical. A **rectangle** is four
  floats `x1, y1, x2, y2` (a vector of 4 floats, `ltx-config` 7). **Global-map units** are the units of
  `[global_map] bound_rect`.
- "Fatal" means the game stops with an error, as reading a missing key or section is (`ltx-config` 6).

## 1. Files

- `config\game.ltx` (a root configuration, `ltx-config` 8) includes `game_maps_single.ltx` on its first line
  and `game_maps_mp.ltx` on its second (D1). All map data is read from the configuration `game.ltx` (B1, B9),
  so the sections below are sections of `game.ltx`.
- `game_maps_mp.ltx` (`[level_maps_mp]` and the multiplayer levels' sections, same format) is read instead of
  `[level_maps_single]` when the game is not a single-player game (B1); multiplayer is outside this scope
  (`pda-scope` 3, `B-mp`).
- The keys `weathers` and `music_tracks` of the level sections belong to the weather and music specifications;
  `[level_music_tracks]` likewise. Nothing in this specification reads them.

## 2. The list of level maps: `[level_maps_single]`

- 17 entries, each a key without a value (D2), in file order: `L01_escape`, `L02_garbage`, `L03_agroprom`,
  `L03u_agr_underground`, `l04_darkvalley`, `l04u_LabX18`, `L05_bar`, `L06_rostok`, `l07_military`,
  `l08_yantar`, `l10_radar`, `l11_pripyat`, `l12_stancia`, `l12_stancia_2`, `l12u_control_monolith`,
  `l12u_sarcofag`, `l08u_brainlab`. Keys keep their case (`ltx-config` 3): 7 entries have upper-case letters
  (`L01_escape`, `L02_garbage`, `L03_agroprom`, `L03u_agr_underground`, `l04u_LabX18`, `L05_bar`,
  `L06_rostok`) (D2).
- The PDA map makes one **level map** per entry, in the section's entry order (B1). That order is
  `ltx-config` 6's byte-wise order of the keys as written: `L01_escape`, `L02_garbage`, `L03_agroprom`,
  `L03u_agr_underground`, `L05_bar`, `L06_rostok`, `l04_darkvalley`, `l04u_LabX18`, `l07_military`,
  `l08_yantar`, `l08u_brainlab`, `l10_radar`, `l11_pripyat`, `l12_stancia`, `l12_stancia_2`,
  `l12u_control_monolith`, `l12u_sarcofag` (D2).
- A level map's **name** is its key **lowercased** (B1a); the name is used for everything after: the duplicate
  test, its section (section 3, 5) and finding the current level's map (section 9).
- If the section does not exist there are no level maps (B1). A name registered twice is fatal ("Duplicate
  level name not allowed", B1, after lowercasing); the 17 shipped keys stay distinct when lowercased (D2).
- Every entry, lowercased, names a section (D2). The 18 single-player level folders (`level-loading` level
  list) are the 17 entries lowercased and `l10u_bunker`, which has no level map (D2, section 8).

## 3. A map section

The global map (section 4), each level map (section 5) and the minimap (section 7) read their section the
same way first (B2):

- **`texture`** (optional): the picture, a texture path (`ui-layout` 6.3), drawn with the shader
  `hud\default` (B2) using the whole texture: no texture description names any of these pictures or their
  files (`ui-layout` 5.2; D11). It is stretched over the whole map rectangle (`ui-layout` 5.3 with `stretch`
  on): **believed**, from the reader giving the picture its texture and then switching one setting on
  (B2), which matches `stretch`; the size of the drawn picture was not measured.
- **`bound_rect`**: the rectangle the picture covers, read only when `texture` exists, and then required
  (B2).
- Without the key `texture` the picture is `ui\ui_nomap2` (`textures\ui\ui_nomap2.dds`, 64 × 64, DXT3, D8) and
  the rectangle is (−10000, −10000, 10000, 10000), whatever `bound_rect` holds (B2).
- The test for the key `texture` uses the plain existence test of `ltx-config` 6, which compares the section
  name as given, byte for byte, with the stored (lowercased) names (B3); reading values lowercases the name
  (`ltx-config` 6, B3). Every name the maps use is lowercase (level maps: B1a; `global_map`; the minimap's
  level names, section 7), so the test finds every section that has `texture` (D2); a reimplementation must
  lowercase level-map names before the test.
- A map starts with the size (x2 − x1) × (y2 − y1) (B2); each kind of map then sets its own size
  (sections 4, 5, 7).

No level folder holds a `map\` or `ui\` picture, so the level-folder search of `ui-layout` 6.3 never applies
and every picture comes from `textures\` (D8).

## 4. The global map: `[global_map]`

- `texture = ui\ui_global_map` (`textures\ui\ui_global_map.dds`, 1024 wide × 2048 high, DXT1),
  `bound_rect = 0.0,0.0, 1024, 2634.0`, `max_zoom = 6.0`; no `global_rect` (D3). The section is read as in
  section 3 with `hud\default`, then `max_zoom` (float, required) (B1, B4).
- Its rectangle (0, 0, 1024, 2634) is in global-map units; y grows downward (B7). The 1024 × 2048 picture is
  stretched over it (section 3), so one picture pixel row covers 2634 / 2048 ≈ 1.286 units (D3).
- **Scale and zoom.** The global map's scale s is its width in UI units divided by (x2 − x1), the same for
  both axes (B7). It is first fitted to the map pane (which pane: PD-4): fitting to a W × H rectangle gives
  the smallest size of the map's proportions that covers it, (H · (x2 − x1) / (y2 − y1)) × H when
  (y2 − y1) / H < (x2 − x1) / W, else W × (W · (y2 − y1) / (x2 − x1)) (B5). The scale after fitting is the
  smallest zoom and the starting zoom; every zoom change is clamped to [smallest zoom, `max_zoom`] (B4).
  `max_zoom` is thus in UI units per global-map unit.
- A point (px, py) in global-map units lies at ((px − x1) · s, (py − y1) · s) from the global map's top-left
  corner (B7); with this section's x1 = y1 = 0, at (px · s, py · s).

## 5. Level sections

### 5.1 Keys

- `texture`, `bound_rect`: section 3. `bound_rect` is the level area the picture shows, in level metres:
  x1, x2 on the level's x axis, y1, y2 on its z axis (section 6).
- `global_rect`: the level's rectangle on the global map, in global-map units (y downward); read after
  `texture` and `bound_rect`, required (B6). It is read even for the sections that count as having no
  texture (section 3).
- The listed sections hold no other keys than these and `weathers`, `music_tracks` (D4).

### 5.2 The listed levels

| Section | `texture` | Picture | `bound_rect` | `global_rect` |
|---|---|---|---|---|
| `l01_escape` | `map\map_escape` | 512 × 1024 | −317.164, −633.718, 404.880, 811.627 | 355, 2011, 576, 2453 |
| `l02_garbage` | `map\map_garbage` | 1024 × 1024 | −322.334, −347.225, 321.685, 318.000 | 371, 1767, 561, 1963.256226 |
| `l03_agroprom` | `map\map_agroprom` | 1024 × 1024 | −266.505, −355.611, 331.388, 213.838 | 161, 1834, 344, 2008.293945 |
| `l03u_agr_underground` | `map\map_agr_underground` | 1024 × 1024 | −137.000, −147.000, −35.302, 50.254 | 255, 1900, 257, 1903.879150 |
| `l04_darkvalley` | `map\map_darkvalley` | 1024 × 1024 | −445.320, −663.867, 449.864, 231.317 | 567, 1608, 1003, 2044.338 |
| `l04u_labx18` | `map\map_labx18_map` | 1024 × 1024 | −51.236, −37.911, 47.999, 81.159 | 788, 1144, 790, 1146.399780 |
| `l05_bar` | `map\map_bar` | 512 × 1024 | 0.000, −512.030, 512.000, 512.001 | 407, 1414, 556, 1712 |
| `l06_rostok` | `map\map_rostok` | 512 × 1024 | −512.000, −512.030, 0.000, 512.001 | 258, 1414, 407, 1712 |
| `l07_military` | `map\map_military` | 1024 × 1024 | −443.567, −130.466, 196.434, 509.534 | 414, 1217, 604, 1406.999756 |
| `l08_yantar` | `map\map_yantar` | 1024 × 1024 | −282.869, −432.963, 287.342, 138.271 | 93, 1566, 256, 1729.292480 |
| `l08u_brainlab` | `map\map_mind` | 512 × 256 | −146.670, −41.626, 153.227, 21.611 | 172, 1612, 174, 1614 |
| `l10_radar` | `map\map_radar` | 1024 × 1024 | −136.256, −503.108, 714.384, 353.524 | 527, 975, 769, 1218.704712 |
| `l11_pripyat` | `map\map_pripyat` | 1024 × 1024 | −529.399, −457.516, 574.854, 769.050 | 457, 586, 798, 964.770996 |
| `l12_stancia` | `map\map_aes_1` | 1024 × 1024 | −256.776, −747.782, 1398.440, 850.523 | 253, 74, 737, 541.358704 |
| `l12_stancia_2` | `map\map_aes_1` | 1024 × 1024 | −256.776, −747.782, 1398.440, 850.523 | 253, 74, 737, 541.358704 |
| `l12u_control_monolith` | — | — | — | 440, 333, 442, 335 |
| `l12u_sarcofag` | — | — | — | 440, 333, 442, 335 |

(D4.) All pictures are DXT5 `.dds` files under `textures\` and all exist; every rectangle has x2 > x1 and
y2 > y1 (D4). Section headers `[L02_garbage]`, `[l04u_LabX18]`, `[L05_bar]`, `[L06_rostok]`, `[L10_radar]`
are written with upper case; names are stored lowercased (`ltx-config` 2; D6).

- **Levels without a picture**: `l12u_control_monolith` and `l12u_sarcofag` have only `global_rect` (and
  weather and music); their picture is `ui\ui_nomap2` with the rectangle of section 3 (D4, B2).
- **Small rectangles**: `l03u_agr_underground`, `l04u_labx18`, `l08u_brainlab`, `l12u_control_monolith`,
  `l12u_sarcofag` have a `global_rect` 2 units wide (D5): marks on the global map rather than areas.
- **Proportions.** For every listed level with a picture except `l08u_brainlab`, the height/width ratio of
  `global_rect` equals that of `bound_rect` within 0.01 % (`l01_escape` and `l04_darkvalley`: 0.2 %) (D5).
  `l08u_brainlab`'s `bound_rect` is 0.211 as high as wide, its `global_rect` square (D5); section 6 gives the
  consequence.

## 6. Mappings

### 6.1 A level map on the global map

At every update a level map sets its rectangle to the global map's conversion (section 4) of the corners
(gx1, gy1) and (gx2, gy2) of its `global_rect` (B6). It is therefore (gx2 − gx1) · s wide and
(gy2 − gy1) · s high, placed at (gx1 · s, gy1 · s) from the global map's top-left, and its picture (or
`ui\ui_nomap2`) is stretched over that rectangle (section 3).

### 6.2 A level position in a level map

A level map's scale k is its width divided by (x2 − x1) of its `bound_rect`; k is used for **both** axes
(B7). A level point (x, z) (the y coordinate is not used) lies at

- u = (x − x1) · k to the right of the level map's left edge,
- v = (y2 − z) · k below its top edge (B7):

level x grows to the right and level z grows upward on the map. The inverse is x = u / k + x1,
z = y2 − v / k (B7).

Combining 6.1 and 6.2 (k = (gx2 − gx1) · s / (x2 − x1)), a level point lies at the global-map point

- gx = gx1 + (x − x1) · q, gy = gy1 + (y2 − z) · q, with q = (gx2 − gx1) / (x2 − x1) global-map units per
  metre (B6, B7).

Because only the width ratio is used, a point lies where the stretched picture shows it only when the two
rectangles have the same proportions (section 5.2). Example, Escape: q = 221 / 722.044 = 0.306076; the
player's start position of `level-loading` 2.5 (x −246.7263, z −134.4339) is at global-map point
(376.56, 2300.57), at 0.097553 of the level rectangle's width from its left and 0.655127 of its height from
its top, i.e. picture pixel (49.9, 670.8) of 512 × 1024 (M1, M2). For `l08u_brainlab` the whole z range of
its `bound_rect` spans 0.4217 global-map units while its rectangle and picture are 2 units high (M3).

### 6.3 A global-map point

Section 4: ((px − x1) · s, (py − y1) · s) from the global map's top-left, y downward, no flip (B7). Where the
global map sits in the pane and how it moves are PD-4's.

## 7. The minimap's section

For `hud-scope` spec 10 (B9):

- The minimap's section is the current level's name (`level-loading` 2.2, e.g. `l01_escape`; the level
  names are lowercase, D2), looked up in
  `game.ltx` with the plain (byte-exact) existence test; if `game.ltx` has no such section, the same name is
  read from the level's own `level.ltx` (`$level$`) instead.
- It is read as in section 3 with `hud\default`; `global_rect` and `max_zoom` are not read (B2, B9).
- Its size becomes (x2 − x1) · F · 0.01 × (y2 − y1) · F · 0.01, F the width of the minimap's level frame
  (`zone_map.xml` `minimap:level_frame`) in UI units: F / 100 UI units per metre on both axes (B9).
- Level points map as in 6.2 with k = F / 100 (B7); with the minimap's rotation on (the minimap turns it on,
  the PDA's maps do not), the result is then turned about a centre point, which is `hud-scope` spec 10's (B7).
- All 18 single-player level names are sections of `game.ltx` (D2), so in the shipped data the minimap never
  reads `level.ltx`.

## 8. Sections outside the list

- `l10u_bunker` (a shipped level, D2): `global_rect = 588.0, 1082.0, 590.0, 1084.0`, weather and music, no
  `texture` or `bound_rect` (D7). No level map is made for it (section 2); the minimap reads it and finds no
  `texture` (section 7, section 3).
- `l06u_labx16` (`global_rect` and weather only), `l09_deadcity`, `andy_test`, `ai_test` (D7): no level folder
  exists for any of them, and nothing in the list names them, so no map reads them in the shipped game (D7,
  B1, B9). For the record: `l09_deadcity` names `map\map_deadcity`, which no archive holds, and its
  `bound_rect` has five numbers (`-502.148,-412.284,379.976,474,479`; as a 4-float vector it reads
  −502.148, −412.284, 379.976, 474, `ltx-config` 7); `andy_test` names `map\map_andy_test` (512 × 512,
  DXT1) and `ai_test` `map\map_test` (256 × 256, DXT1) (D7).
- `textures\map\map_selo.dds` is named by no `texture` key of `game.ltx` (D10).

## 9. The current level's map

- The PDA map finds the current level's level map by the level's name (`level-loading` 2.2: `l01_escape` for
  the New Game) among the registered names (B8). A name not registered writes the log line
  `~ Level Map '<name>' not registered` (B8); what the map then shows is PD-4's.
- Measured: in a New Game (level `l01_escape`, listed as `L01_escape`), the map key M opens the PDA with a
  detailed picture (not a flat one) in its map pane, and the game log, written out after the PDA was shown,
  holds no such line (R1, R2): the mixed-case entry is found through its lowercased name (B1a).
- `l10u_bunker` has no level map (section 2), so there the lookup fails as above (B8; not observed).

## 10. `level.ltx` `[level_map]`

`level.ltx` of `l01_escape` has `[level_map]` with `texture = ui\ui_map_escape` (no such file), `x1 = -490.7`,
`x2 = 504.8`, `z1 = -1081`, `z2 = 1114.2`; that of `l08_yantar` has `texture = ui\ui_map_mp_factory` and the
same four keys (D9). The section is **not read** by the maps: no program file names `level_map` as a section
(`xrGame.dll` names it only as the suffix `:level_map` of `map_spots.xml` element paths, B10), the
`[level_map]` keys `x1`, `x2`, `z1`, `z2` are not the map keys, the only map read of `level.ltx` is the
minimap's fallback (section 7) under the level's name, which no `level.ltx` has as a section (D9), and the
fallback is never taken in the shipped data (section 7). `level-loading` requires the file to exist.

## 11. Unestablished

- Which game-kind values mean the single-player game: the program reads `[level_maps_single]` for the values
  0 and 1 (B1); which of them a New Game uses was not established (believed 1; either gives the same list).
- The drawing order of the level maps on the global map: they are made in entry order (B1); that they are
  drawn in that order, later ones over earlier ones, is **believed** from `ui-layout` 4.3 (children are drawn
  in the order added), not checked for maps. The shipped rectangles of `l12_stancia` and `l12_stancia_2`
  coincide (D4), as do those of `l12u_control_monolith` and `l12u_sarcofag`.
  **Disputed in review:** the belief's basis does not hold: the level maps are not added to the global
  map when they are made but each time the map window is shown, in the order of the registry of level
  maps, which is ordered by an internal identity of each name (not by the name's text, nor by entry
  order). The drawing order of overlapping level maps is therefore not entry order and is unestablished;
  in the shipped data it matters only for the two coinciding pairs above.
- What the PDA map shows while the current level has no level map (`l10u_bunker`): PD-4's; not observed.
- The rotation the minimap applies (centre, angle): `hud-scope` spec 10's.

## Provenance

- Date: 2026-11-26.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG 2.1.0.10), archives `gamedata.db0` …
  `gamedata.dbd`; the shipped program files of `bin\` (`xrGame.dll` and `xrCore.dll` in detail; all program
  files searched for strings).
- Data examined: `config\game.ltx` and its includes, `config\game_maps_single.ltx`, `config\game_maps_mp.ltx`;
  every level folder's `level.ltx`; the pictures of `textures\map\` and `textures\ui\ui_global_map`,
  `ui_nomap2` (headers); the archive listing; `config\ui\` (searched for the picture names).
- Analysis: static analysis of the shipped program files (Ghidra, headless decompilation, instruction
  listings, import and string references): the PDA map window's set-up, the reading of map sections, the
  global and level maps' conversions, fitting and zoom limits, the minimap's set-up, the configuration
  reader's existence tests. Black-box runs of the unmodified game (headless, driven by key presses typed
  into its virtual display, its frames and log captured from outside).
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/level-maps/` reproduce every fact.
