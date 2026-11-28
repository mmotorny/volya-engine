# HUD and PDA notification sounds

Behaviour specification S23 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`):
the notification sounds the player hears from the HUD and the PDA, in its full scope: which notification
sounds exist, which configuration they read, which are played by the program and which by scripts, and when
the program plays its one, the new-contact sound. It settles the scoping specification's open question:
`snd_new_contact1` is used (section 2.2), `new_contact_snd` is not (believed, section 2.3). The PDA
indicators that the same event animates (the contact counter and the flashing PDA icon) are spec 12
(`hud-pda-indicators`) of `docs/behavior/hud-scope.md` and are not specified here. What could not be
established is marked **unestablished**; what is inferred rather than read from the program or the data is
marked **believed**.

Builds on (not repeated):

- `docs/formats/sound-ogg-params.md` (S1): a sound name and its file, the file's min and max distance and
  base volume, the AI sound type and its flag names (section 7.2).
- `docs/behavior/sound-positional.md` (S2): creating a sound in a volume category, **play at position**
  with the flags looped and 2D and a delay, the play volume, stopping at once, discarding a sound, 2D
  emitters, everything about how a playing sound is heard.
- `docs/behavior/sound-detector.md` (S21): **sound lists** (section 2.2: the keys *K*, *K*`1`, *K*`2`, …
  of a section, each value "name, volume, delay"); **playing a list** (section 6, steps 1 – 4); the
  **range test** of a carried device (section 4.2: objects found near a point within a radius, kept in a
  list, reported new when they come into range and gone when they leave it); the object the level's player
  controls (section 4.1); the scheduled updates of objects (section 4.1).
- `docs/behavior/player-state.md`: the starting inventory and the PDA's state at creation (section 3.1:
  original owner id 0, the player), slots (section 3.3: the PDA in slot 7).
- `docs/behavior/hud-scope.md`: the HUD's layout and configuration data (sections 1.2, 1.5) and its spec 12.
- `docs/behavior/offline-simulation.md` section 9.1: the records of `spawns\all.spawn` and their class ids.
- `docs/formats/ltx-config.md`: key lookup with inheritance; reading a missing key is fatal.

Conventions: a key's value "names" the file `sounds\<value>.ogg` (S1). LTX values are read after
inheritance. "Health" is a living object's health, the value the scripts read as the property `health`;
the program's own test for "alive" is health > 0 [B5]. Distances are in metres.

**Checks.** Every claim carries a tag; the checks are scripts in the dirty workspace,
`checks/sound-notifications/`: `bin.py` (B1 – B11, static analysis of `xrGame.dll` and `XR_3DA.exe`),
`data.py` (D1 – D8, the game data); `run_all.py` runs them all (`ghidra.py` and `extract.py`, run first,
prepare the evidence).

## 1. Which notification sounds there are

- **New contact** (sections 2 – 4): played by the program, from the HUD, when the player's PDA reports a
  new contact [B1, B2, B3]. It is the only notification sound the program plays itself (**believed**: of the
  sounds the scoping specification's section 2 finds the program reading or naming, it is the only one of
  the HUD or the PDA; the PDA's own code plays no sound [B9]).
- **PDA messages, tips and tasks** (section 6): three sounds played by the shipped scripts when they send
  news, tips and task updates to the PDA [D8]. When the scripts do so is S24's.

## 2. The new-contact sound list

### 2.1 Configuration

`config\system.ltx`, section `[maingame_ui]` [D1]:

| Key | Value |
|---|---|
| `pda_msgs_max_show_time` | 20000 (not sound: hud-scope spec 9) |
| `info_msgs_max_show_time` | 10000 (not sound: hud-scope spec 9) |
| `snd_new_contact` | `detectors\contact_1` |
| `snd_new_contact1` | `detectors\contact_8` |

There is no `snd_new_contact2` [D1]. No shipped script names `snd_new_contact`, `new_contact_snd` or
`maingame_ui`; of the configuration files only `config\system.ltx`, `config\ui\maingame.xml` and
`config\ui\maingame_16.xml` name any of them [D1].

### 2.2 Reading

When the HUD's main window is set up (believed: when a level starts and its HUD is created), it reads the
sound list (S21 section 2.2) of the key prefix *K* = `snd_new_contact` in `[maingame_ui]` [B1]. So the list
reads `snd_new_contact`, then `snd_new_contact1`, and ends at the missing `snd_new_contact2`: **two sounds**,
in this order `detectors\contact_1`, `detectors\contact_8` [B1, D1]. (The name `snd_new_contact1` occurs
in no program file because the reader composes it from the prefix and the number [D4, B1].) Neither value
has a comma, so both have volume 1 and delay 0 [D1]. Each sound is created in the **effects** category with
the AI sound type `idle` (0x00000800, S1 section 7.2), so the files' own AI sound type is not used [B1].
A missing `snd_new_contact` would make the list empty, an assertion failure (S21 section 2.2).

The list belongs to the HUD's main window; when that window is destroyed every sound of the list is
discarded (S2) [B1]. Nothing else uses the list [B1].

### 2.3 `new_contact_snd`

`config\ui\maingame.xml` and `config\ui\maingame_16.xml` each have one element `new_contact_snd` with the
text `detectors\contact_1` [D3]. No program file contains the string `new_contact_snd` (also not as
UTF-16), and no script or other configuration file names it [D4, D1]. The HUD's setup reads its layout
elements by name (hud-scope section 1.2). So the element is not used (**believed**, as the string search is
the evidence); its value is the same as the first entry of the list anyway [D1, D3].

## 3. The PDA and its contacts

### 3.1 PDAs

`device_pda` is the only section of class id `D_PDA`: `radius` = 50, `slot` = 7 [D5]. A PDA reads `radius`
(a required float) when it is loaded: its **contact radius** *ρ* [B9]. On the first level the one PDA is the
player's, created by the simulation with the starting inventory (player-state.md section 3.1) with original
owner id 0, the player, and placed in slot 7 (section 3.3); no first-level record is a PDA [D6].

### 3.2 On and off

Each PDA has an **original owner** id and is **on** or **off** [B10]:

- It is created off, with the original owner 0xFFFF [B10].
- When it comes into the level, its original owner becomes the original-owner id of its simulation
  object [B10] (for the player's PDA: 0, the player; player-state.md section 3.1).
- Whenever it gets a parent (is taken into an inventory) whose object id equals its original owner, it is
  switched **on**; getting any other parent leaves it as it is [B10].
- Whenever it loses its parent (is dropped or otherwise let go), it is switched **off**; its contact list
  (section 3.4) is kept [B10].
- When it leaves the level (its object is destroyed), it is switched off and its contact list is emptied
  [B10].
- Its scheduled update switches it off when its parent is not alive (section 3.3).

So the player's own PDA is on while the player carries it and alive; a PDA the player picks up from someone
else (another original owner) stays off and never reports contacts.

### 3.3 The scheduled update

The PDA has scheduled updates (the general object schedule, as S21 section 4.1; that the PDA does not
change its interval is **believed**) [B9]. At each one [B9]:

1. if the PDA has a parent, its position becomes its parent's position (row 3 of the parent's matrix);
2. if it is on, has a parent, the level has a **controlled object**, and that object's id is the parent's
   id: if the parent is a living object with health > 0, the **range test** (S21 section 4.2) runs at the
   PDA's position with the radius *ρ* and the contact test of section 3.4; otherwise the PDA is switched
   off [B9, B5].

The level's controlled object is set together with the object the level's player views, which S21
section 4.1 uses; the viewed object can also be set on its own, so the two can differ (**believed**: both are
the player's character during normal play) [B11]. While the PDA is off, or its parent is not the controlled
object, no range test runs and its contact list does not change (except as section 3.2 states) [B9].

### 3.4 Contacts

The PDA's **contact list** is the range test's list (S21 section 4.2): objects found in range that pass the
**contact test** are added and reported **new**; objects of the list that are no longer found or no longer
pass are removed and reported **gone**; the PDA holds nothing back [B6]. An object passes the contact test
when all of these hold [B7]:

1. it is of a kind that can own an inventory;
2. the item in that inventory's slot 7 (the PDA slot) is not this PDA (so the PDA's own carrier is never its
   contact; a character with no PDA passes);
3. it is a living object;
4. it is not of a kind that a further kind test excludes (a test of the object's kind only).

On the first level these pass: the player's character, stalkers (class id `AI_STL_S`) and the trader
(`AI_TRD_S`); these never pass: boars, dogs, fleshes and pseudodogs (`SM_BOARW`, `SM_DOG_S`, `SM_FLESH`,
`SM_P_DOG`: they own an inventory but the kind test excludes them), the helicopter (`C_HLCP_S`: no inventory),
and crows (no inventory; that `AI_CROW` objects are of that kind is **believed**) [B8, D7]. The player's
character fails test 2 for its own PDA. Which other kinds the kind test excludes is **unestablished**
(believed: every monster kind).

The contact test does not look at health: a dead character in range is a contact, and a contact that dies
stays in the list [B7].

## 4. The notification

### 4.1 New contact

Each new contact of the player's PDA is reported to the PDA's parent, the player's character [B6, B4] (only
a PDA carried by the controlled object runs the range test, section 3.3). For each new contact, in a
single-player game, the player's character [B3]:

1. tests whether the contact's health is > 0 (strictly) [B3, B5];
2. notifies the HUD's main window, which sets a time for the PDA indicators from the current time (what the
   indicators do with it is hud-scope spec 12) and, **if the test of step 1 held**, plays the new-contact
   list (section 2) [B2, B3].

Outside a single-player game steps 1 and 2 do not happen: the player's character does nothing with the report
unless the game type is 1 or 0 (**believed**: 1 is single player, as in S21) [B3].

Playing the list is S21 section 6 steps 1 – 4 with the list of section 2 [B2]:

1. both sounds of the list are stopped at once;
2. one is chosen: index *k* mod 2, *k* drawn from the shared generator (`particle-lifecycle.md` 2.1), so
   `detectors\contact_1` for even *k* and `detectors\contact_8` for odd *k*;
3. it is played with **play at position**: **2D**, not looped, at (0, 0, 0) relative to the listener, with
   delay 0, associated with no object;
4. its play volume is set to 1.

So each new contact cuts off the previous new-contact sound; several new contacts found by one range test
play one after another in the same update, each cutting off the one before [B2, B6]. Heard at full level for
its volume (S21 section 6: a 2D emitter at the listener), in the effects category (S2) [B1].

### 4.2 No other notification

A contact that is reported gone plays no sound (the report changes map marks: not sound, not specified
here) [B4]. A dead
contact coming into range is a new contact without sound (section 4.1 step 1) [B3, B7]. A contact that dies
while in the list makes no report [B7]. The PDA's own code plays no sound [B9].

## 5. The first level

### 5.1 Sound files

Both mono, 44,100 Hz, with a version 3 record; S2 lengths *D* = ⌊*N* · 1000 / 44100⌋ [D2]:

| File (`sounds\detectors\…`) | Min (m) | Max (m) | Base volume | AI type (file) | Samples *N* | Length (ms) |
|---|---|---|---|---|---|---|
| `contact_1` | 1 | 300 | 1.0 | 0 | 5,097 | 115 |
| `contact_8` | 1 | 300 | 1.0 | 0 | 4,416 | 100 |

### 5.2 Contacts on the first level

The first level has 63 stalker records (`AI_STL_S`), one trader record (`AI_TRD_S`) and the player's record
(`O_ACTOR`) [D6]. Measured from the position of the player's record, the trader's record is 2.09 m away
and 8 stalker and trader records lie within 50 m (the next at 52.66 m) [D6]. So the player's PDA, on from the
start (section 3.2), is believed to find several new contacts at its first range test after a New Game and
play the new-contact sound then (**believed**: which characters are in the level and where at that moment
depends on the simulation and the scripts, and the range test also counts each object's bounding radius,
S21 section 4.2).

## 6. PDA sounds played by scripts

`scripts\news_manager.script` makes three sound objects [D8]:

| Sound object | File (`sounds\device\pda\…`) | Played by the script's | Delay (s) |
|---|---|---|---|
| `pda_tips` | `pda_tip` | `send_tip`, `send_tip_nosound` | the tip's timeout argument (**Disputed in review:** only for `send_tip`, where a missing timeout is 0; `send_tip_nosound` sets the timeout to 0 before playing, so its delay is always 0) |
| `pda_task` | `pda_objective` | `send_task` | 0 |
| `pda_news` | `pda_news` | `send_encyclopedy` (for a `Diary` article), `send_treasure` | 0 |

Each is played with the scripts' 2D flag (`sound_object.s2d`) at the player's character with the given
delay [D8]; `send_tip` then plays the tip's own voice, if it has one, 1 s later [D8]. **Disputed in review:** the unit of the scripts' play delay (the "(s)" of the table's heading and the "1 s" here) is not established by the check; the scripts pass the timeout, and the timeout plus 1 for the voice, and how the script API reads that number is S24's (unestablished here). No other script names these sounds or
`sounds\device\pda\` [D8]. The directory has a fourth file, `pda_sos`, which no script, configuration file or
program file names [D8]. The files [D8]:

| File (`sounds\device\pda\…`) | Min (m) | Max (m) | Base volume | AI type (file) | Samples *N* | Length (ms) |
|---|---|---|---|---|---|---|
| `pda_news` | 1 | 300 | 0.4 | 0 | 13,977 | 316 |
| `pda_tip` | 1 | 300 | 0.4 | 0 | 11,373 | 257 |
| `pda_objective` | 1 | 300 | 0.4 | 0 | 17,772 | 402 |

All mono, 44,100 Hz, version 3 records [D8]. How the scripts' sound objects play (their category, type and
emitter) and when the scripts call these functions are S24's (the scripts and the script API), as are the
tips' voices.

## 7. Unestablished

- That the HUD's main window is set up when a level starts (believed, section 2.2).
- That the new-contact sound is the program's only notification sound (believed, section 1).
- That the PDA does not change its schedule interval (believed, section 3.3).
- That `new_contact_snd` is used nowhere (believed from the string search, section 2.3).
- That the level's controlled object is the player's character during normal play (believed, section 3.3).
- Which kinds other than the first level's the kind test of section 3.4 excludes (believed: every monster
  kind); that crows are of a kind without an inventory (believed).
- That game type 1 is single player (believed, section 4.1).
- Which contacts the first range test after a New Game finds (believed: several, section 5.2), and whether a
  loaded game reports its contacts anew (believed: the contact list starts empty when the PDA comes into the
  level).
- What follows an assertion failure (section 2.2).

## 8. Provenance

- Date: 2026-11-25.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives `gamedata.db0` … `gamedata.dbd`;
  the shipped program files of `bin\` (`xrGame.dll` and `XR_3DA.exe` in detail; all program files searched
  for strings).
- Data examined: `config\system.ltx` with its include tree and inheritance; `config\ui\maingame.xml` and
  `maingame_16.xml` and every other configuration file (searched); the `sounds\detectors\` and
  `sounds\device\` files (comment headers and lengths); the 439 shipped scripts; every record of
  `spawns\all.spawn` (decoded per sav.md).
- Analysis: static analysis of the shipped program files (Ghidra, headless decompilation, instruction
  listings, entry tables, reference and string searches): the HUD window's setup, destruction and
  notification; the player character's contact reports; the PDA's loading, switching, scheduled update and
  contact test; the engine's controlled and viewed objects; the health value. No execution or debugging of
  the game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/sound-notifications/` reproduce every fact.

## Gaps found in review

- After each range test of section 3.3, and when the PDA leaves the level, the PDA also rebuilds a second list from its contact list: the contacts whose health is > 0 (strictly); what uses it is not stated (believed: the contact counter of hud-scope spec 12), and so the claim of section 3.4 that a dead contact stays a contact holds for the contact list only.
