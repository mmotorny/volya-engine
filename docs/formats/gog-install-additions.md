# GOG install: what it creates beyond the game files

What a GOG offline install of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition, installer version
`2.1.0.10`, see `docs/formats/gog-installer.md`) creates or changes on the machine besides extracting the game
files, as the installer's own entries, the install script it ships (`goggame-1207660573.script`) and its
metadata (`goggame-1207660573.info`) define it; which of these the game needs to start; and where the game
looks for its path configuration file `fsgame.ltx`.

**Checks.** Static claims (§1–§6, the names in §7) are checked by `gog-install/scripts/checks.py` in the dirty
workspace (after `inno_header.py` has decompressed the installer's and the interpreter's header streams and
innoextract has extracted the non-game entries to `gog-install/ext/`); the runs of §8, on which §3's user-folder
facts, §4's registry view and §7 rest, by `gog-install/scripts/experiments.py` (which uses `run_isi.py` and
`run_game.py`). The order of §7 was first read from the binaries and is backed by runs B–E.

Notation: `{app}` is the install folder chosen in the installer, `{userdocs}` the current user's Documents
folder ("My Documents"; Inno Setup's documented meaning of that constant), `{commonprograms}` and
`{commondesktop}` the all-users Start-menu programs folder and desktop, `{commonappdata}` the all-users
application-data folder, `{sys}` the Windows system folder. Paths inside `{app}` are given with `\`.

## 1. How the install works

1. The Inno Setup installer extracts its entries (the game files of `gog-installer.md` and the items of §6).
2. Its own registry entries are written (§4.2).
3. It then starts GOG's script interpreter, `{app}\__redist\ISI\scriptInterpreter.exe` (a 32-bit Windows
   program, itself an Inno Setup executable), with the command line

   `/verysilent /supportDir="{app}\__support" /SUPPRESSMSGBOXES /NORESTART /DIR="{app}" /productId="1207660573" /buildId="58117267108351024" /versionName="2.1.0.10 GOG 0.13" /Language="<language>" /LANG="<language>"`

   and, under a condition of the installer's own (believed: the user declined a desktop shortcut), the further
   option ` /nodesktopshortcut`. The two language values are the installer's chosen language (a run with
   `russian` was accepted, §8); where exactly the optional option is placed in the line was not established.
4. The interpreter reads `{app}\goggame-1207660573.info` (play tasks → shortcuts, §5) and
   `{app}\goggame-1207660573.script` (the actions of §2–§4.1, all for every language, `"languages": ["*"]`).
5. The installer finally runs the DirectX redistributable when needed (§6).

## 2. The generated `fsgame.ltx`

Action "Generate fsgame.ltx" (type `saveToFile`): writes `{app}\fsgame.ltx` with the text below. The text in the
script is 965 bytes of ASCII, 17 lines, each ending in a single LF (no CR anywhere, no byte-order mark, the last
line LF-terminated). The only placeholder in it is `{userdocs}` (first line); every other byte is literal:

```
$app_data_root$ = true | false| {userdocs}\Stalker-SHOC
$game_data$ =     false| true | $fs_root$      | gamedata\
$game_ai$ =       true | false| $game_data$    | ai\
$game_spawn$ =    true | false| $game_data$    | spawns\
$game_anims$ =    true | true | $game_data$    | anims\
$game_levels$ =   true | false| $game_data$    | levels\
$game_meshes$ =   true | true | $game_data$    | meshes\
$game_dm$ =       true | true | $game_data$    | meshes\
$game_shaders$ =  true | true | $game_data$    | shaders\
$game_sounds$ =   true | true | $game_data$    | sounds\
$game_textures$ = true | true | $game_data$    | textures\
$game_scripts$ =  true | false| $game_data$    | scripts\
$game_config$ =   true | false| $game_data$    | config\
$level$ =         false| false| $game_levels$
$game_saves$ =    true | false| $app_data_root$| savedgames\
$logs$ =          true | false| $app_data_root$| logs\
$screenshots$ =   true | false| $app_data_root$| screenshots\
```

This is byte for byte the block in `docs/formats/ltx-config.md` §10, which specifies the syntax and resolution.

Derivation of each value:

- `{userdocs}` is replaced by the current user's Documents folder as an absolute path, e.g.
  `C:\Users\<name>\Documents` (under Wine `C:\users\<name>\Documents`), so `$app_data_root$` is
  `<Documents>\Stalker-SHOC`. Believed (Inno Setup's documented convention for folder constants): the
  replacement has no trailing `\`.
- The install folder does not appear in the file: `$game_data$` is relative to `$fs_root$`, which the game
  derives itself (§7). Moving the install folder therefore needs no change to the file.
- All other aliases are fixed text.

Then action "Set fsgame.ltx to readonly" (type `execute`): runs `{sys}\attrib.exe +r fsgame.ltx` with working
folder `{app}`, giving the file the read-only attribute. The game only reads the file.

Not established: whether the interpreter writes the text with LF unchanged (the action's optional `utf8` and
`append` flags are absent, so it is written in the interpreter's default text mode), and how a Documents path
containing characters outside ASCII is encoded in the file. The game accepts the LF-only text (runs D, F, A
in §8 used it as above).

## 3. Files in the user folder

| Action (type) | Source | Target |
|---|---|---|
| "Default game settings" (`supportData`, `File`) | `{app}\__support\userdocs\user.ltx` (5,028 bytes) | `{userdocs}\STALKER-SHOC\user.ltx` |
| "Some file, idk" (`supportData`, `File`) | `{app}\__support\userdocs\traffic.net` (2,056 bytes) | `{userdocs}\STALKER-SHOC\traffic.net` |
| "Save folder" (`savePath`, `Folder`) | — | registers `{userdocs}\STALKER-SHOC` as the game's save folder with GOG's tooling |

(`{supportDir}` in the script is the `/supportDir` value of §1, i.e. `{app}\__support`.) `STALKER-SHOC` and the
`Stalker-SHOC` of `fsgame.ltx` name the same folder (Windows folder names are case-insensitive). The copy is the
file unchanged (believed: a plain file copy); whether an existing `user.ltx` is overwritten (the action's
optional `overwrite` flag is absent) was not established. What `savePath` changes on the machine was not
established.

`{app}\__support\userdocs\banned.ltx` (0 bytes) is installed but no action copies it anywhere.

Whether the game needs them:

- `user.ltx`: not needed to get past the file system. Without the user folder at all, the game creates
  `<Documents>\Stalker-SHOC\` and its `logs\` itself, logs that it cannot open `user.ltx`, and goes on with its
  built-in settings (runs A, D2). With it, the settings in it apply (`docs/behavior/startup.md` §4). Under Wine
  some runs, with and without `user.ltx`, ended at the start of the graphics device with a crash dump in
  `logs\` (§8); this is not tied to `user.ltx`. Whether the game writes a `user.ltx` of its own when there is
  none was not established (none appeared within 40–60 s; the game was then terminated).
- `traffic.net`, `banned.ltx`: no game executable or library in `bin\` contains either name. Believed not
  needed; their purpose was not established.

## 4. Registry

### 4.1 Written by the install script

Key `HKEY_LOCAL_MACHINE\SOFTWARE\GSC Game World\STALKER-SHOC`, five values:

| Value name | Type | Data |
|---|---|---|
| `InstallPath` | string | `{app}` (the install folder; believed without trailing `\`) |
| `InstallSource` | string | `GOG` |
| `InstallVers` | string | `2.1.0.10` |
| `BonusPack1` | DWORD | script text `$1`; believed the number 1 (`$` as hexadecimal prefix) |
| `BonusPack2` | DWORD | as `BonusPack1` |

So `InstallVers` **is** written under the key the title menu reads (`docs/behavior/title-menu.md` §3.4: value
`InstallVers` of `HKEY_LOCAL_MACHINE\Software\GSC Game World\STALKER-SHOC\`; key names are case-insensitive in
Windows). It is written by the interpreter's script, not by the Inno Setup registry entries, which contain no
`GSC Game World` key at all. On a GOG install the caption therefore reads `ver. 2.1.0.10`; this follows from
the data and title-menu.md, it was not observed on a running game.

Registry view: the interpreter is a 32-bit program, as are all of the game's executables and libraries. On 64-bit
Windows both therefore use the 32-bit view of `HKEY_LOCAL_MACHINE\SOFTWARE`
(`HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\…`; Windows' documented registry redirection), so the game sees what
the script wrote. Observed under Wine: the interpreter's `HKEY_LOCAL_MACHINE\SOFTWARE` writes land in the
`Wow6432Node` view (§8). **Disputed in review:** being a 32-bit program does not by itself settle the view, since
the interpreter can address both views (it contains the root names `HKLM32` and `HKLM64`); the only observed write
is `installer_language` (§4.2), not one of the script's values, so that the five values above land in the 32-bit
view is believed, not established.

Read by the game (names in its libraries, under `Software\GSC Game World\STALKER-SHOC\`): `InstallVers`
(built-in default `1.0006`) and `InstallPatchID` by the GameSpy library; `InstallUserName` and `InstallCDKEY` by
the game library. GOG writes neither `InstallPatchID`, `InstallUserName` nor `InstallCDKEY`; no game binary
contains the names `InstallPath`, `InstallSource` or `BonusPack`. What the game does without
`InstallPatchID`, `InstallUserName` or `InstallCDKEY` is out of scope here (none is needed to reach the title
menu per `startup.md`).

### 4.2 Written by the installer itself

Inno Setup registry entries, all `HKEY_LOCAL_MACHINE\SOFTWARE\GOG.com\Games\1207660573`, string values:
`buildId` = `58117267108351024`, `ver` = `2.1.0.10 GOG 0.13`, `lang_code` = a language code computed by the
installer's code at install time (value not established), `osbit` = `neutral`. The interpreter adds at least
`installer_language` = the `/LANG` value (observed: `russian`) under the same key. Other values GOG's code may
write there (the installer's code names e.g. `PATH`, `EXE`, `WORKINGDIR`, `LAUNCHEXE`, `STARTMENULINK`) were not
established. Believed (Inno Setup's documented behaviour): the usual uninstall registration of an Inno Setup
install. The game reads none of the `GOG.com` values.

## 5. Shortcuts and the launch task

The interpreter turns the `playTasks` of `goggame-1207660573.info` into shortcuts in a Start-menu folder
`{commonprograms}\S.T.A.L.K.E.R. Shadow of Chernobyl [GOG.com]\` (and, unless `/nodesktopshortcut` is given,
on `{commondesktop}`; **Disputed in review:** the desktop shortcut rests only on the folder constant and the
option's name appearing in the interpreter and installer; no desktop shortcut was observed or checked, so it is
believed):

| Play task | Category | Target | Working folder | Arguments | Shortcut (observed under Wine, §8) |
|---|---|---|---|---|---|
| `S.T.A.L.K.E.R. Shadow of Chernobyl` (primary) | game | `{app}\bin\XR_3DA.exe` | `{app}\bin` | none | not observed |
| `Settings` | tool | `{app}\Settings.exe` | not given | none | `Инструменты\Settings.lnk` |
| `Поддержка` | document | URL `http://www.gog.com/support/stalker_shadow_of_chernobyl` | — | — | `Документация\Поддержка.url` |

The primary task also has `compatibilityFlags` `HIGHDPIAWARE`; the interpreter contains the Windows
compatibility-layer key `SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers`, so it is believed
to record `HIGHDPIAWARE` there for the executable (registry root not established). That flag only changes
Windows' display scaling of the window.

The launch the game is designed for is therefore `bin\XR_3DA.exe` with working folder `bin` and no command-line
arguments. The shortcuts are not needed by the game. Name and location of the game's own shortcut were not
observed (the interpreter stops under Wine before it, §8).

## 6. Other installer entries

Besides the game files, the installer places (none is needed by the game):

- Empty folders `{app}\userdocs\downloads`, `{app}\userdocs\logs`, `{app}\userdocs\savedgames`. The game does not
  use them; its user folder is `$app_data_root$` (§2).
- `{app}\__redist\ISI\scriptinterpreter.exe` (§1), `{app}\__support\userdocs\` (§3),
  `{app}\goggame-1207660573.script`, `{app}\goggame-1207660573.ico` (not on Windows XP), `{app}\webcache.zip`,
  `{commonappdata}\GOG.com\supportInstaller\uninstall.dll`.
- The DirectX 9.0c redistributable in `{app}\__redist\DirectX\` (`DXSETUP.exe`, `DSETUP.dll`, `dsetup32.dll` and
  `.cab` files). The installer's run entry starts `{app}\__redist\DirectX\DXSETUP.exe /silent` only if its
  dependency check finds it not yet installed. Every library the game's modules import is either shipped in
  `bin\` (including `d3dx9_34.dll`, `d3dx9_31.dll`, the Visual C++ 7.1 and 8.0 runtimes, OpenAL, EAX) or a
  standard Windows library; believed not needed on a Windows that has Direct3D 9.

Whether `{app}\__redist` and `{app}\__support` are removed after the install was not established.

## 7. Where the game looks for `fsgame.ltx`

The name is `fsgame.ltx`, or the text after the command-line option `-fsltx ` up to the next space if that
option is given (the launch task gives none). The game looks for it in this order:

1. In the folder that holds the game's core library `bin\xrCore.dll` (the executables' folder). If found there,
   the game root `$fs_root$` is **empty**, so every alias resolves relative to the working folder; with the
   generated file and working folder `bin` the game then stops with
   `Cannot find file gamedata\config\system.ltx.` / `Reinstalling application may fix this problem.` (run E).
2. In the parent of that folder (the install folder `{app}`). If found there, `$fs_root$` is that folder with a
   trailing `\` (e.g. `C:\Games\Stalker\`).

The working folder is **not** searched: with `fsgame.ltx` only in the working folder the file is not found
(run C). With the generated file in `{app}`, the game passed this stage with working folder `{app}\bin`
(run D, the launch task), `{app}` (run F) and an unrelated folder `C:\` (run A): no message, and the log file
appeared in `<Documents>\Stalker-SHOC\logs\` (whose path comes from the file).

If the file is in neither folder, a message box with the text `Cannot open file "fsgame.ltx".` + line feed +
`Check your working folder.` is shown (runs B, C) and the game goes no further; this is fatal. (Its title and
icon, and the exit after dismissal, were not checked by a run.) **Disputed in review:** that it is fatal is not
backed by a check; the runs show only that the game goes no further while the box is open. A name given with
`-fsltx` was not tried by the investigator; in review, `{app}\fs2.ltx` alone with the arguments
`-fsltx fs2.ltx -foo` and working folder `bin` passed this stage without a message and the log file appeared
(run X2), and with the generated file in both `bin\` and `{app}` and working folder `bin` the game stopped with the
`system.ltx` message of run E (run X1), so the copy in `bin\` is used first.

What a launcher must therefore provide for the original to start: the game files of `gog-installer.md` with
`bin\` as a subfolder of the install folder and `fsgame.ltx` (§2, `{userdocs}` replaced) in the install folder;
`user.ltx` in `<Documents>\Stalker-SHOC\` only for the shipped default settings (§3: not required); the
registry value `InstallVers` only for the title menu's version caption (without the key the game logs that it is
missing and goes on).

## 8. Black-box runs (Wine)

Under Wine (Wine Stable, a dedicated prefix with its audio driver disabled):

- The interpreter, started with the §1 command line (`/Language=Russian /LANG=russian`) on a folder holding the
  script, the metadata, `__support\userdocs\`, `Settings.exe` and `bin\XR_3DA.exe`, logged reading the metadata,
  created the two Start-menu entries of §5 and `installer_language` (§4.2) in the `Wow6432Node` view, then
  stopped with a stack overflow before running any script action. The script actions (§2–§4.1) are therefore
  established from the script and the interpreter's data, not from an observed run.
- `XR_3DA.exe`, unmodified, with the game files and the §2 file (`{userdocs}` replaced by the prefix's
  Documents folder), 40–60 s per run, message boxes recorded through Wine's message-box trace:

| Run | Working folder | `fsgame.ltx` in | `user.ltx` present | Result |
|---|---|---|---|---|
| D | `{app}\bin` | `{app}` | yes | no message; `logs\` created |
| F | `{app}` | `{app}` | yes | no message; `logs\` created |
| A | `C:\` | `{app}` | no (no user folder) | no message; user folder and `logs\` created by the game; no `user.ltx` written |
| A2 | `C:\` | `{app}` | yes | no message; `logs\` created |
| D2 | `{app}\bin` | `{app}` | no (no user folder) | no message; user folder and `logs\` created by the game |
| B | `{app}` | nowhere | yes | `Cannot open file "fsgame.ltx".` … |
| C | `C:\` | `C:\` only | yes | `Cannot open file "fsgame.ltx".` … |
| E | `{app}\bin` | `{app}\bin` only | yes | `Cannot find file gamedata\config\system.ltx.` … |

Whether the runs without a message reached the title menu was not observed (no frames were captured). Across
repetitions, runs of every configuration without a message (D, A2, D2, A) sometimes wrote a crash dump to `logs\`
right after the log line announcing the start of the graphics device, with and without `user.ltx`; some runs
(of A, F and D2) ended silently after 4 s. These are taken as faults of the Wine environment, not of the install layout. The
logs also show that without `user.ltx` the game logs "Cannot open script file [user.ltx]" and goes on, and that
without the registry key it logs that `Software\GSC Game World\STALKER-SHOC\` was not found and goes on (the
test prefix has no such key).

## 9. Not established

- The interpreter's exact output: line ends and encoding of the written `fsgame.ltx` (§2), overwrite behaviour
  of `user.ltx` (§3), the effect of `savePath`, the DWORD value of `$1` (believed 1), trailing `\` in `{app}` and
  `{userdocs}` (believed none).
- The `lang_code` value and further `GOG.com` registry values (§4.2); the root of the compatibility-flag entry
  (§5); the name and location of the game's own shortcut (§5); the condition and position of
  `/nodesktopshortcut` (§1).
- Purpose of `traffic.net` and `banned.ltx`; whether `__redist` and `__support` are deleted after install.
- Whether the game writes `user.ltx` when it has none.
- The title-menu caption `ver. 2.1.0.10` was derived, not seen.

## Provenance

- Date: 2026-11-26.
- Data: the Russian GOG offline installer `setup_s.t.a.l.k.e.r._shadow_of_chernobyl_2.1.0.10_gog_0.13_(russian)_(77895).exe`
  (+ `-1.bin`); its entries listed and its non-game entries extracted with innoextract 1.9; its Inno Setup header
  stream (and that of the bundled `scriptinterpreter.exe`) decompressed (LZMA, Python standard library) and its
  length-prefixed strings and registry-entry fields read; `goggame-1207660573.script` and
  `goggame-1207660573.info` read as JSON; `__support\userdocs\*` sizes; the game binaries in `bin\` searched for
  file and registry names and their PE import tables listed.
- Binary analysis: static analysis of `bin\xrCore.dll` (Ghidra) for the order in which `fsgame.ltx` is sought and
  how the game root follows from it, checked by the black-box runs of §8.
- Execution: the interpreter and the unmodified `XR_3DA.exe` run under Wine as described in §8.
- No leaked source code, derived engines, third-party engine documentation or recollection was used. Inno Setup's
  public documentation was used only for the meaning of its folder constants and its uninstall registration.

## Gaps found in review

- The uninstall registration (key, value names and data) is only believed from Inno Setup's documentation and is not listed in §9.
- The `Поддержка` play task is limited to the language `ru-RU` in the metadata (the other two tasks to `*`); whether the interpreter skips it for other install languages was not established.
- Whether the game copes with `fsgame.ltx` carrying the read-only attribute was not checked by a run (the runs used a writable copy), nor that the game never writes it.
- Whether the script actions run in the order listed, and whether a failing action stops the later ones (e.g. the registry values), was not established.
