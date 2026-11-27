# GOG install: what it creates beyond the game files

What a GOG offline install of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition, installer version
`2.1.0.10`, see `docs/formats/gog-installer.md`) creates or changes on the machine besides extracting the game
files, as the installer's own entries, the install script it ships (`goggame-1207660573.script`) and its
metadata (`goggame-1207660573.info`) define it; which of these the game needs to start; and where the game
looks for its path configuration file `fsgame.ltx`.

**Checks.** The game's read of `InstallVers` (§4.1, §7) and the box64 run of §8 are checked by
`titlever/scripts/check.py` and `titlever/scripts/check_run.py`. Static claims (§1–§6, the names in §7) are
checked by `gog-install/scripts/checks.py` in the dirty workspace (after `inno_header.py` has decompressed the installer's and the interpreter's header streams and
innoextract has extracted the non-game entries to `gog-install/ext/`); the runs of §8, on which §3's user-folder
facts, §4's registry view and §7 rest, by `gog-install/scripts/experiments.py` (which uses `run_isi.py` and
`run_game.py`). The order of §7 was first read from the binaries and is backed by runs B–E. §4.3 is checked by
`gog-script/scripts/checks.py` (the script's encoding, and the interpreter's names) and
`gog-script/scripts/experiments.py` (the run that could not observe the actions); its **Conditions** paragraph by
`gog-cond/scripts/checks.py` (which reads the interpreter's compiled code with `gog-cond/scripts/ifps.py`). Everything changed in the fix of
2026-11-27 (the container runs of §8 and what rests on them, the interpreter's further code, the §7 message box) is
checked by `gog-fix603/scripts/checks.py`, from the outputs of `run_isi_c.py`, `run_game_c.py` and `run_full_c.py`
(test files written by `make_tests.py`) and of the project's `xtask original --quit 150`. `titlever/scripts/check_run.py` also passes on
that last run (its folder is named in `gog-fix603/runs/orig/source.txt`).

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
   `russian` was accepted, §8); **Disputed in review:** run `full2` below contradicts this for `/Language`: with
   English chosen it was `Russian`, the metadata's language; only `/LANG` was the chosen language. Where exactly the optional option is placed in the line was not established.
   Observed in a whole silent install (run `full2` of §8, which chose English): the line ended
   `/Language="Russian" /LANG="english"`, the first value matching the metadata's language, and had no
   `/nodesktopshortcut`.
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
  `<Documents>\Stalker-SHOC`. The replacement has no trailing `\` (observed: run `real` of §8 wrote
  `C:\users\root\Documents\Stalker-SHOC`).
- The install folder does not appear in the file: `$game_data$` is relative to `$fs_root$`, which the game
  derives itself (§7). Moving the install folder therefore needs no change to the file.
- All other aliases are fixed text.

Then action "Set fsgame.ltx to readonly" (type `execute`): runs `{sys}\attrib.exe +r fsgame.ltx` with working
folder `{app}`, giving the file the read-only attribute (observed, run `real` of §8). The game copes with the
attribute and does not write the file: in run `g1` of §8 (the file as the interpreter wrote it, read-only, in
`{app}`; working folder `{app}\bin`; Wine refusing writes to read-only files, checked on a copy) the game opened
`fsgame.ltx` once, for reading only, opened the archives and created its log file in the folder the file names,
with no message; the file was unchanged afterwards. In run `orig` (the game started from a writable copy of the file,
taken to its title menu and quit from its console, exit code 0) the file's content and modification time were
unchanged after the quit.

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
established. Observed (run `real` of §8, a fresh Wine prefix whose Documents folder holds no `STALKER-SHOC`): both
copies fail (logged as errors), because the copy does not create the target folder (its code), and nothing is copied; the
following actions still run (§4.3, **When an action fails**). A whole install (runs `full`, `full2`) left the
Documents folder empty in the same way. On a machine without that folder the install
therefore (believed on Windows too) leaves no `user.ltx` and no `traffic.net`; the game then creates the folder itself (below).

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
| `InstallPath` | string | `{app}`, the install folder without trailing `\` (observed: `Z:\work\app` for `/DIR=Z:\work\app`, run `real` of §8) |
| `InstallSource` | string | `GOG` |
| `InstallVers` | string | `2.1.0.10` |
| `BonusPack1` | DWORD | script text `$1`: the number 1 (observed, run `real`; `$` marks hexadecimal, §4.3) |
| `BonusPack2` | DWORD | as `BonusPack1` |

So `InstallVers` **is** written under the key the game queries for its version string
(`docs/behavior/title-menu.md` §3.4: value `InstallVers` of `HKEY_LOCAL_MACHINE\Software\GSC Game World\STALKER-SHOC\`;
key names are case-insensitive in Windows). It is written by the interpreter's script, not by the Inno Setup
registry entries, which contain no `GSC Game World` key at all. The game discards the data it reads, so the
title-menu caption reads `ver. 1.0006` on a GOG install too (title-menu.md §3.4; observed, §8).

Registry view: the interpreter is a 32-bit program, as are all of the game's executables and libraries. On 64-bit
Windows both therefore use the 32-bit view of `HKEY_LOCAL_MACHINE\SOFTWARE`
(`HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\…`; Windows' documented registry redirection). The game opens its key
without a registry-view flag, so it reads that 32-bit view (title-menu.md §3.4); that it sees what the script
wrote rests on the script's writes landing there. Observed (run `real` of §8, the shipped script and metadata
run by the interpreter under Wine's WoW64 build): all five values are in the 32-bit view and the key is absent from
the 64-bit view. The interpreter cannot reach the 64-bit view from a script at all: the root names `HKLM32` and
`HKLM64` it knows give the same root as `HKEY_LOCAL_MACHINE` (its code; run `t6`: values written with `HKLM`,
`HKLM32` and `HKLM64` all landed in the 32-bit view), and it runs in Inno Setup's 32-bit install mode (its log:
`64-bit install mode: No`).

Read by the game (names in its libraries, under `Software\GSC Game World\STALKER-SHOC\`): `InstallVers`
(queried, but the data is not used: the version string is always `1.0006`, title-menu.md §3.4) and
`InstallPatchID` by the GameSpy library; `InstallUserName` and `InstallCDKEY` by
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
established. **Disputed in review:** after a whole install (runs `full`, `full2` of §8) the key holds exactly the
interpreter's values listed below plus `osbit` = `neutral` (value names are case-insensitive, so `PATH`, `EXE`,
`WORKINGDIR` and `STARTMENULINK` are the interpreter's `path`, `exe`, `workingDir` and `startMenuLink`; no
`LAUNCHEXE` value is present), with `lang_code` = `ru-RU` and `installer_language` = `english` in those runs; the
value the installer itself writes for `lang_code` is overwritten by the interpreter's and stays unestablished. The game reads none of the `GOG.com` values.

The interpreter, an Inno Setup program itself (§1), finishes with its own install steps after the script's actions
(observed, run `real` of §8: the interpreter alone, run with the §1 command line on an install folder
`Z:\work\app`). All registry writes below are in the 32-bit view (§4.1).

- Under `HKEY_LOCAL_MACHINE\SOFTWARE\GOG.com\Games\1207660573`, string values: `BUILDID` = the `/buildId` value,
  `dependsOn` = empty, `DLC` = empty, `exe` = `{app}\bin\XR_3DA.exe`, `exeFile` = `XR_3DA.exe`, `gameID` and
  `productID` = `1207660573`, `gameName` = `S.T.A.L.K.E.R. Shadow of Chernobyl`, `INSTALLDATE` = the date and time
  (`2026-11-27 00:15:59`), `installer_language` = `russian`, `lang_code` = `ru-RU`, `language` = the metadata's
  language lower-cased (`russian`), `launchCommand` = `{app}\bin\XR_3DA.exe` followed by one space, `launchParam` =
  empty, `path` = `{app}`, `savegamefolder` = `{userdocs}\STALKER-SHOC`, `startMenu` = `S.T.A.L.K.E.R. Shadow of
  Chernobyl`, `startMenuLink` = `{commonprograms}\S.T.A.L.K.E.R. Shadow of Chernobyl [GOG.com]\S.T.A.L.K.E.R.
  Shadow of Chernobyl`, `supportLink` = empty, `uninstallCommand` = `{app}\unins000.exe`, `ver` = the
  `/versionName` value, `workingDir` = `{app}\bin` (key names are case-insensitive: `BUILDID` and `buildId` are one
  value, so are `productID` and the installer's names). Value names are as the run's registry listing shows them.
- **Uninstall registration.** The uninstaller `{app}\unins000.exe` with its files `unins000.dat`, `unins000.msg`
  and `unins000.ini` (section `[InstallSettings]`, `productID=1207660573`), and the key
  `HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\1207660573_is1` with these values
  (an earlier key of that name is deleted first, per the interpreter's log):

  | Value | Type | Data (run `real`) |
  |---|---|---|
  | `DisplayIcon` | string | `{app}\gog.ico` |
  | `DisplayName` | string | `S.T.A.L.K.E.R. Shadow of Chernobyl` |
  | `DisplayVersion` | string | `2.1.0.10 GOG 0.13` |
  | `EstimatedSize` | DWORD | a size (here 2153; unit not established) |
  | `HelpLink`, `URLInfoAbout`, `URLUpdateInfo` | string | `http://www.gog.com` |
  | `Inno Setup: App Path` | string | `{app}` |
  | `Inno Setup: Icon Group` | string | `S.T.A.L.K.E.R. Shadow of Chernobyl [GOG.com]` |
  | `Inno Setup: Language` | string | `russian` |
  | `Inno Setup: Setup Version` | string | `5.6.1 (u)` |
  | `Inno Setup: User` | string | the user's name (`root`) |
  | `InstallDate` | string | the date, `YYYYMMDD` (`20261127`) |
  | `InstallLocation` | string | `{app}` followed by `\` |
  | `MajorVersion`, `VersionMajor` | DWORD | 2 |
  | `MinorVersion`, `VersionMinor` | DWORD | 1 |
  | `NoModify`, `NoRepair` | DWORD | 1 |
  | `Publisher` | string | `GOG.com` |
  | `QuietUninstallString` | string | `"{app}\unins000.exe" /SILENT` (with the quotes) |
  | `UninstallString` | string | `"{app}\unins000.exe"` |

  In a whole install (runs `full` and `full2` of §8: the installer itself, silent, `/DIR=Z:\work\app`) the
  installer first writes its own registration under the same key name, with `{app}\unins000.*` (its log); the
  interpreter, started later, deletes that key and writes its own (its log, later times), so the key left is the
  interpreter's: the values of the table, except `DisplayIcon` = `{app}\goggame-1207660573.ico` (a file a whole
  install has and run `real` had not) and `Inno Setup: Language` = the installer's language (`english` in the silent
  runs). Both runs left the same values (apart from dates). The uninstaller removes this key, the product key above
  and the install's shortcuts (observed, run `t3u`).

### 4.3 How the script encodes its actions

So that an importer can read `InstallVers` (and the other values of §4.1) from the script instead of a table.

**File.** `{app}\goggame-1207660573.script` (5,300 bytes in this installer) is one JSON object, UTF-8 without a
byte-order mark (in this file all bytes are ASCII), indented with spaces, LF line ends, no LF after the closing
`}`. In every object of this file the members appear in ascending order of their names; nothing indicates that
the order matters. Backslashes inside strings are written with the JSON escape `\\`; after decoding, a string
holds single `\`.

**Top level.** Two members:

| Member | JSON type | Value here |
|---|---|---|
| `actions` | array of objects | the 10 actions below, in file order |
| `productId` | string | `"1207660573"` |

**Action.** Each element of `actions` has the members:

| Member | JSON type | Meaning |
|---|---|---|
| `install` | object | what to do at install: `action` (string, the kind) and `arguments` (object, the kind's fields) |
| `languages` | array of strings | here always `["*"]` |
| `name` | string, optional | a label (present on the first five actions, absent on all five `setRegistry` actions) |

The interpreter's vocabulary (string constants in its code, ISI version 0.1.2.538) also has per-action members
`uninstall` (the counterpart of `install`, used when uninstalling, **Uninstalling** below) and `order`; neither
occurs in this script. `conditions` is not a member of the action but of `install.arguments` (**Conditions** below).

**Order of the actions.** `order` is read from the action object itself (beside `install`), as a JSON number. The
observed run order (runs `t1`, `t4`, `t5` of §8, actions with `order` values 9 … 1, 0, 1000000, -5, -1, 0.4,
-0.4, 0.5, 0.6, 1.5, 2.4, 2.5, 3.5, 4.4, absent, and the string `"7"`): first every action whose `order` is a
number other than 0, in file order, whatever the numbers; then every action whose `order` is 0, absent or not a
number, in file order. So a script without `order` members, like this one, runs its actions in file order
(observed, run `real`). The interpreter's code reads the number and sorts the list before running it; why the sort
gives this grouping and not ascending numbers was not established, and other values (e.g. non-finite or very large
numbers) were not tried.

**When an action fails.** The actions run one after another; each one's success or failure is only written to the
interpreter's log (`[OK]` or `[ERROR]`), and the next action runs either way: in run `real` both `supportData`
actions failed (§3) and the five `setRegistry` actions after them ran; in run `t1` an `execute` of a missing
program failed and the later actions ran. An action that raises an error instead of failing (as a `setRegistry`
action with an unknown `root` does, below) ends the interpreter: a message box is shown (suppressed with
`/SUPPRESSMSGBOXES`, the log records it), the interpreter exits with code 3, no later action runs, and the
interpreter's own install steps after the actions (its uninstall registration and registry values, §4.2;
shortcuts) are not made; the effects of the actions before it stay (run `t2`). The action loop has no handler for
such errors (its code). When the script file is missing or not accepted as JSON, no action runs (its code).

`languages`: the name `languages` occurs nowhere in the interpreter (neither in its executable, nor in its compiled
code, nor in the JSON library it uses, in ASCII or UTF-16), so this interpreter ignores the member and runs every
action whatever the install language (observed, run `t11`: actions with `["en-US"]` and `[]` ran in a Russian
install). In this script the question does not arise: every action has `["*"]`. The script has no architecture or Windows-version field, and the
interpreter offers no architecture choice: its root names `HKLM32` and `HKLM64` give the same view (§4.1).

**Action kinds.** The interpreter knows the kinds `execute`, `setregistry`, `setini`, `supportdata`, `savetofile`,
`installsdb`, `deletedata`, `createshortcut`, `savepath`, `delregistry` and `pass`, and maps the older name
`saveGame` to `savePath`. Its code holds the kind names in lower case and uses a lower-casing operation, while the
script spells them `saveToFile`, `execute`, `supportData`, `savePath`, `setRegistry`: the kind is lower-cased and
then compared with these names (its code; observed, run `real`: all ten actions ran). A reader of this script should
therefore compare the kind case-insensitively. An action of any other kind does nothing: it is logged as `[OK]` and
the next action runs (its code; observed, run `t1` with the kind `frobnicate`).

**`setRegistry` arguments.** In this script exactly these five members, all JSON strings:

| Member | Meaning | Spellings |
|---|---|---|
| `root` | registry root | the interpreter knows `HKEY_LOCAL_MACHINE`, `HKLM`, `HKLM32`, `HKLM64`, `HKEY_CURRENT_USER`, `HKCU`, `HKEY_CLASSES_ROOT`, `HKCR`, `HKEY_CURRENT_CONFIG`, `HKCC`, `HKEY_USERS`, `HKU`, compared exactly (with case); this script uses only `HKEY_LOCAL_MACHINE` |
| `subkey` | key path below the root, `\`-separated, no leading `\` | here `SOFTWARE\GSC Game World\STALKER-SHOC` |
| `valueName` | name of the value | |
| `valueType` | type of the value | lower-cased, then one of `string`, `expandstring`, `multistring`, `dword`, `binary`, `none`; an empty or absent type counts as `none`; this script uses `string` and `dword` |
| `valueData` | the data, as text | for `string` the text itself; for `dword` a number in text, decimal or with a leading `$` hexadecimal, here `$1`; for `binary` believed Base64 text (its code decodes it before writing; not run) |

The interpreter also knows the optional `setRegistry` members `conditions` (**Conditions** below), `deleteSubkeys`
and `saveGameData`, absent here (the meaning of the last two was not established). Believed: `string`, `expandstring`, `multistring`, `dword`,
`binary` are the Windows registry types REG_SZ, REG_EXPAND_SZ, REG_MULTI_SZ, REG_DWORD and REG_BINARY (its code
has write operations for exactly these five types and none for a 64-bit number; observed for `string` and
`dword`). Observed (runs `real`, `t1`): `$1` is the DWORD 1, `$10` is 16, `10` is 10; the type `DWORD` works as
`dword`. `none` writes the key's default value as empty text, so it only creates the key (its code; not run).
Every `HKEY_LOCAL_MACHINE` root, `HKLM64` included, is the 32-bit view on 64-bit Windows (§4.1).

Unknown spellings (its code; runs `t1`, `t2`, `t6`): an unknown `valueType` (e.g. `qword`) writes nothing, the
action is logged as `[OK]` and the next action runs. An unknown `root`, including a known name in other case
(`hkey_current_user`), raises an error (`Invalid RootKey value`) that ends the interpreter (**When an action
fails** above).

**Conditions.** An action may carry conditions that decide whether it runs. Established from the interpreter's
compiled code (ISI version 0.1.2.538); the language and version words, and conditions when uninstalling, were also
observed in runs (`t7a`–`t7c`, `t3u` of §8).

- **Where.** The optional member `conditions` of `install.arguments` (of `uninstall.arguments` when uninstalling,
  **Uninstalling** below), a JSON array. It is read for the kinds
  `execute`, `setregistry`, `setini`, `supportdata`, `savetofile`, `installsdb`, `deletedata`, `createshortcut` and
  `delregistry` (kind names as the interpreter holds them, see **Action kinds**); for `savepath` and `pass` it is not
  read, so conditions never stop those. A `conditions` member anywhere else
  (e.g. beside `install` in the action) is not read. Member names are matched exactly (with case) and the first
  member of that name counts. A `conditions` member that is not an array (e.g. a single string) counts as absent.
- **How.** The action runs only if every condition holds; no `conditions`, or an empty array, means it runs. Elements
  that are not JSON strings are ignored. Each string is lower-cased and then compared with the words below; a string
  that is none of them is ignored (it holds). The elements are tested in array order, and the first that does not
  hold ends the test: later elements are not tested (this matters for `onlyonce`, which writes when tested).
- **The words.** `<productId>` is the `/productId` value of §1 (here `1207660573`), `<key>` is
  `HKEY_LOCAL_MACHINE\SOFTWARE\GOG.com\Games\<productId>` (the key of §4.2). The stored values are read once per
  action, before its elements are tested; comparisons of text ignore case (both sides are lower-cased).

  | Word | Holds when |
  |---|---|
  | `onlanguagechange` | the metadata's language (member `language` of `{app}\goggame-<productId>.info`, here `Russian`) differs from the string value `language` of `<key>` |
  | `onversionchange` | the `/versionName` value of §1 (here `2.1.0.10 GOG 0.13`) differs from the string value `ver` of `<key>` |
  | `onlyonce` | the integer under key `DoOnce<productId>[<identity>]` of section `[setup]` in `{app}\goglog.ini` is not 1 (absent: 0). When it holds, 1 is written to that key at once, whether or not a later element then stops the action |
  | `ati`, `amd` (the same test) | the display adapters' descriptions, lower-cased, contain `ati` or `amd` |
  | `nvidia` | the display adapters' descriptions, lower-cased, contain `nvidia` |
  | `!ati`, `!amd`, `!nvidia` | the test of the word without `!` is false |
  | `nodesktopshortcutParam`, `nostartmenushortcutParam` | always: the interpreter's own spellings contain a capital `P`, so a lower-cased element never equals them and is ignored (believed: the comparison is exact, as text equality in the interpreter's language is) |

  A missing registry value reads as empty text (observed: with no stored values, `onversionchange` held and
  `onlanguagechange` held exactly when the metadata had a language, runs `t7a`, `t3u`). The registry view of the
  two reads is the 32-bit view, as for all of the interpreter's `HKEY_LOCAL_MACHINE` accesses (§4.1). How the
  interpreter encodes `goglog.ini` on disk was not established.

  The metadata's language: if the metadata has no member `language`, or one that is not a JSON string, the
  metadata's language is empty text (its code), so `onlanguagechange` holds exactly when a non-empty `language`
  value is stored under `<key>` (observed: metadata without `language`, nothing stored: does not hold, run `t7a`;
  stored `russian`: holds, run `t7b`; metadata `Russian`, stored `russian`: does not hold, run `t7c`). If the
  metadata file is missing, the interpreter ends before any action (exit code 1, run `t9`), so no condition is
  tested. A metadata file that the JSON library does not accept ends it the same way (its code; not run: a file cut
  off before its closing `}` was accepted and its `language` read, run `t8`, and which faults the library rejects
  was not established).

  Display adapters: the descriptions are the string value `Device Description` of each key named by the values
  `\Device\Video0` … `\Device\Video<n-1>` of `HKEY_LOCAL_MACHINE\HARDWARE\DEVICEMAP\VIDEO` (n its DWORD
  `MaxObjectNumber`; each named path is read below `HKEY_LOCAL_MACHINE` with its prefix `\Registry\Machine\`
  removed), taken together as one text. Without `MaxObjectNumber` there are no descriptions, and `ati`, `amd`,
  `nvidia` do not hold. The test is a plain substring test: any description containing the three letters `ati`
  anywhere counts.
- **`<identity>`** (for `onlyonce`) is computed from the action's own arguments as written in the script, before
  placeholders are replaced: it starts as the empty string, and for each member of the list below that is present
  in `install.arguments` as a JSON string, in the list's order, it becomes the MD5 of the text formed by the identity
  so far followed by the member's text. Believed (Inno Setup's documented MD5 function for Unicode strings, which the
  interpreter uses): the MD5 is taken over the text's UTF-16LE bytes and written as 32 lower-case hexadecimal digits.
  Absent members and members that are not strings are skipped.

  | Kind | Members, in order |
  |---|---|
  | `execute` | `executable`, `argument`, `workingDir` |
  | `setini` | `filename`, `section`, `keyType`, `keyName`, `keyValue` |
  | `setregistry` | `root`, `subkey`, `valueType`, `valueName`, `valueData` |
  | `supportdata` | `type`, `source`, `target` |
  | `savetofile` | `filename`, `text` |
  | `installsdb` | `sdbFile`, `GUID` |
  | `deletedata` | `type`, `target` |
  | `createshortcut` | `type`, `name`, `targetFile`, `arguments`, `workingdir`, `icon`, `compatibilityFlags`, `easyTarget` |
  | `delregistry` | `root`, `subkey`, `valueName` |
- **On a first install** (no earlier GOG install of the product on the machine): no string `language` is named by
  the installer's header; the interpreter itself writes the value `language` under `<key>` (the metadata's
  language, lower-cased; empty text without one), but only after all actions have run (observed, runs `real`,
  `t7a`; §4.2). So when the conditions are tested no value `language` exists under `<key>`, and `onlanguagechange`
  holds when the metadata has a language (observed, install pass of run `t3u`); `onlyonce` holds
  (no `goglog.ini` key yet); the installer itself writes `ver` under `<key>` with the same text as its `/versionName`
  (§4.2) before it starts the interpreter (§1), so `onversionchange` does not hold (believed: the order of §1); the
  adapter words depend on the machine's display adapters.
- **Uninstalling.** The uninstaller `{app}\unins000.exe` (§4.2) runs the actions again with each action's member
  `uninstall` in place of `install` (same layout: `action` and `arguments`), from the copy of the script that the
  install leaves in `{commonappdata}\GOG.com\supportInstaller\<productId>\goggame-<productId>.script` (with the
  metadata beside it), after its own removal of the install's files and shortcuts (its code and log). The
  `conditions` member of `uninstall.arguments` decides whether an action runs exactly as at install; the
  `conditions` of `install.arguments` play no part (observed, run `t3u` of §8: of seven actions with uninstall
  conditions `nvidia`, `!nvidia`, `onlanguagechange`, `onversionchange`, `onlyonce`, none, and one with `onlyonce`
  only in its install arguments, the `nvidia`, `onlanguagechange` and `onversionchange` ones did not run, the
  others did). The values compared differ from an install: the metadata's language and the `/versionName` value are
  empty text when uninstalling (they are only read when installing, its code), the install folder is the string
  value `path` of `<key>`, the product id comes from `unins000.ini`; and in run `t3u` `<key>` had already been
  removed when the actions ran (believed: removed by the uninstall's own registry step, which runs before them),
  so both stored values were empty and `onlanguagechange` and `onversionchange` did not hold. `onlyonce` counts
  separately for the uninstall arguments, whose identity is computed from them. For this product (empty
  `dependsOn`) the uninstaller first asks whether to keep the saved games, even with `/SUPPRESSMSGBOXES`, unless it
  is started with `/galaxyclient`, when the option `/keepsaves` answers it (its code; observed: a run without
  `/galaxyclient` waited on the question). **Disputed in review:** no check reproduces the question or the run
  that waited on it; believed.

**Placeholders.** Text in braces (and a few `<?…?>` words) is replaced before use, in these arguments only
(its code; observed for `setRegistry` in runs `real` and `t1`, for `saveToFile` in run `real`):

| Kind | Arguments with placeholders |
|---|---|
| `execute` | `executable`, `argument`, `workingDir` |
| `setini` | `filename`, `section`, `keyName`, `keyValue` |
| `setregistry` | `subkey`, `valueName`, `valueData` (not `root`, `valueType`) |
| `supportdata` | `source`, `target` |
| `savetofile` | `filename`, `text` |
| `installsdb` | `sdbFile` |
| `deletedata` | `target` |
| `createshortcut` | `name`, `targetFile`, `arguments`, `workingdir`, `icon` |
| `savepath` | `savePath` |
| `delregistry` | `subkey`, `valueName` |

The replacement has two stages (its code; the results observed in run `t1`, install folder `Z:\work\app`, Russian
metadata):

1. The interpreter's own words, each replaced wherever it occurs, matched exactly (with case: `{ProductId}` and
   `{APP}` are not replaced here), in this order:

   | Word | Replaced by |
   |---|---|
   | `{supportDir}` | `{app}\__support` (observed `Z:\work\app\__support`; with the option `/galaxyclient` given: the `/supportDir` value of §1 followed by `\` and the product id, or without a `/supportDir` value a folder derived from the interpreter's own path; not run) |
   | `{productID}` | the `/productId` value of §1 (`1207660573`) |
   | `{rootProductID}` | the metadata's string member `rootGameId` (`1207660573`) |
   | `{version}` | the `/versionName` value of §1 (`2.1.0.10 GOG 0.13`) |
   | `{buildID}` | the `/buildId` value of §1 (`58117267108351024`) |
   | `{gameName}` | the metadata's string member `name` (`S.T.A.L.K.E.R. Shadow of Chernobyl`) |
   | `{n}` | a line break, CR LF (observed: a line break) |
   | `{language}` | the metadata's string member `language` as written (`Russian`), not the installer's language |
   | `{galaxy}` | `{reg:HKLM\SOFTWARE\GOG.com\GalaxyClient\paths,client}\{reg:HKLM\SOFTWARE\GOG.com\GalaxyClient,clientExecutable}`, expanded in stage 2 |
   | `<?APPLICATION_DATA_LOCAL?>`, `<?APPLICATION_DATA_ROAMING?>`, `<?DOCUMENTS?>` | `{localappdata}`, `{userappdata}`, `{userdocs}`, expanded in stage 2 |
   | `<?INSTALL?>`, `{app}` | the `/DIR` value of §1, as given (observed `Z:\work\app`) |
   | `<?SAVED_GAMES?>`, `<?HOME?>` | on Windows XP `{userdocs}\Saved Games\` and `{userdocs}\..\`; otherwise the Windows known folders Saved Games and the user profile (not run) |
   | `{C}`, `{c}`, `{i}` | each occurrence a random character: an upper-case letter `A`–`Z`, a lower-case letter `a`–`z`, a digit `0`–`9` |

2. Then the whole text goes through Inno Setup's constant expansion (the interpreter is an Inno Setup program,
   §1), so every constant Inno Setup documents is accepted: folder constants (observed `{win}` → `C:\windows`,
   `{pf}` → `C:\Program Files (x86)`, `{sys}` → `C:\windows\system32`, `{commondesktop}` →
   `C:\users\Public\Desktop`, `{userdocs}` → `C:\users\root\Documents`, without trailing `\`), `{reg:…}` (observed
   `{reg:HKLM\SOFTWARE\GOG.com\Games\1207660573,ver|none}` → `none`, the default, as the value did not exist yet),
   `{%NAME}` environment variables (observed `{%HOMEDRIVE}` → `C:`), `{{` for a literal `{` (observed `{{x}` →
   `{x}`), and the others of Inno Setup's documentation (not run). Inno Setup's constant names are matched with
   case (`{APP}` is unknown to it). If the text holds a constant Inno Setup does not know (e.g. `{bogus}`, `{APP}`),
   this stage fails as a whole: the failure is logged, no constant of the text is expanded, and the text is used as
   left by stage 1 (observed: `{app}|{win}|{bogus}` → `Z:\work\app|{win}|{bogus}`); the action still runs.

Paths in the script may use `/` as separator (`{app}/fsgame.ltx`); believed accepted by Windows as `\`. The
`InstallVers` data holds no placeholder, so its value does not depend on any of this.

**Reading `InstallVers`.** Among the elements of `actions`, take the one whose `install.action` is `setRegistry`
(compared without case) and whose `install.arguments` has `valueName` `InstallVers`, `root` `HKEY_LOCAL_MACHINE`
and `subkey` `SOFTWARE\GSC Game World\STALKER-SHOC` (key names compared without case, as in Windows); with
`valueType` `string` its `valueData` is the value the game reads (`docs/behavior/title-menu.md` §3.4), provided the
action runs: if its `install.arguments` has `conditions`, only when they hold (**Conditions** above). Exactly one
action of this script matches; it has no `conditions`.

**Worked example (Russian installer).** The five `setRegistry` actions are the last five of the ten (1–5 are
`saveToFile`, `execute`, `supportData`, `supportData`, `savePath`), none has `name`, each has
`"languages": ["*"]`. The third, as it appears in the file:

```
        {
            "install": {
                "action": "setRegistry",
                "arguments": {
                    "root": "HKEY_LOCAL_MACHINE",
                    "subkey": "SOFTWARE\\GSC Game World\\STALKER-SHOC",
                    "valueData": "2.1.0.10",
                    "valueName": "InstallVers",
                    "valueType": "string"
                }
            },
            "languages": [
                "*"
            ]
        },
```

All five (decoded strings; `root` is `HKEY_LOCAL_MACHINE` and `subkey` `SOFTWARE\GSC Game World\STALKER-SHOC` in
each):

| # | `valueName` | `valueType` | `valueData` |
|---|---|---|---|
| 6 | `InstallPath` | `string` | `{app}` |
| 7 | `InstallSource` | `string` | `GOG` |
| 8 | `InstallVers` | `string` | `2.1.0.10` |
| 9 | `BonusPack1` | `dword` | `$1` |
| 10 | `BonusPack2` | `dword` | `$1` |

In the first Wine environment of §8 the interpreter stopped after reading the metadata; in the container runs of §8
it ran the whole script, and the registry held the five values of the table (§4.1).

## 5. Shortcuts and the launch task

The interpreter turns the `playTasks` of `goggame-1207660573.info` into shortcuts in a Start-menu folder
`{commonprograms}\S.T.A.L.K.E.R. Shadow of Chernobyl [GOG.com]\`, and the primary task also into
`{commondesktop}\S.T.A.L.K.E.R. Shadow of Chernobyl.lnk` and `{app}\Launch S.T.A.L.K.E.R. Shadow of Chernobyl.lnk`
(observed, run `real` of §8: all three to `{app}\bin\XR_3DA.exe` with working folder `{app}\bin`). With the option
`/nodesktopshortcut` added to the command line the desktop shortcut is not created and the other two are (run
`t11`).

| Play task | Category | Target | Working folder | Arguments | Shortcut (observed under Wine, §8) |
|---|---|---|---|---|---|
| `S.T.A.L.K.E.R. Shadow of Chernobyl` (primary) | game | `{app}\bin\XR_3DA.exe` | `{app}\bin` | none | `S.T.A.L.K.E.R. Shadow of Chernobyl.lnk` (run `real`) |
| `Settings` | tool | `{app}\Settings.exe` | not given | none | `Инструменты\Settings.lnk` |
| `Поддержка` | document | URL `http://www.gog.com/support/stalker_shadow_of_chernobyl` | — | — | `Документация\Поддержка.url` |

The primary task also has `compatibilityFlags` `HIGHDPIAWARE`; the interpreter contains the Windows
compatibility-layer key `SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers`, so it is believed
to record `HIGHDPIAWARE` there for the executable (registry root not established). **Disputed in review:** observed
in runs `real`, `full` and `full2` of §8 (the interpreter writes it: run `real` ran the interpreter alone): string
values named by the executable's full path under that key in both `HKEY_CURRENT_USER` and `HKEY_LOCAL_MACHINE`
(under Wine the `HKEY_LOCAL_MACHINE` entry appeared in the 64-bit view, not the 32-bit one), `{app}\bin\XR_3DA.exe` =
`HIGHDPIAWARE HIGHDPIAWARE` (the word twice) and `{app}\Settings.exe` = `HIGHDPIAWARE`, although the `Settings`
task has no `compatibilityFlags`. That flag only changes
Windows' display scaling of the window.

Languages of the play tasks: in the metadata the `Поддержка` task has `"languages": ["ru-RU"]`, the other two
`["*"]`. The interpreter reads only the members `type`, `name`, `path`, `arguments`, `workingDir`, `link`, `icon`,
`category`, `compatibilityFlags`, `isPrimary`, `isHidden` and `runAsAdmin` of a play task, and no `languages`
anywhere (§4.3), so every task is processed whatever the install language (its code; observed, run `t10`: with the
`Settings` task limited to `de-DE` and `Поддержка` to `en-US`, both were processed in a Russian install exactly as
with the shipped metadata). In the container runs of §8 (not in the earlier Wine run) the `Settings` and
`Поддержка` entries were logged but did not appear in the Start-menu folder (`Settings` with a shell-link error);
that is taken as a fault of that Wine environment.

The launch the game is designed for is therefore `bin\XR_3DA.exe` with working folder `bin` and no command-line
arguments. The shortcuts are not needed by the game.

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
`Check your working folder.` is shown (runs B, C) and the game goes no further; this is fatal: the box has the
title `Error`, an error icon and an OK button and is system-modal, and once it is dismissed the game's process ends
with exit code 1 (static analysis of `bin\xrCore.dll`; the dismissal was not done in a run). With `-fsltx` the
text names the file given there. A name given with
`-fsltx` was not tried by the investigator; in review, `{app}\fs2.ltx` alone with the arguments
`-fsltx fs2.ltx -foo` and working folder `bin` passed this stage without a message and the log file appeared
(run X2), and with the generated file in both `bin\` and `{app}` and working folder `bin` the game stopped with the
`system.ltx` message of run E (run X1), so the copy in `bin\` is used first.

What a launcher must therefore provide for the original to start: the game files of `gog-installer.md` with
`bin\` as a subfolder of the install folder and `fsgame.ltx` (§2, `{userdocs}` replaced) in the install folder;
`user.ltx` in `<Documents>\Stalker-SHOC\` only for the shipped default settings (§3: not required); no
registry value (without the key the game logs that it is missing and goes on; the title menu's version caption
does not depend on `InstallVers`, title-menu.md §3.4).

## 8. Black-box runs (Wine)

First environment: Wine Stable, a dedicated prefix with its audio driver disabled:

- The interpreter, started with the §1 command line (`/Language=Russian /LANG=russian`) on a folder holding the
  script, the metadata, `__support\userdocs\`, `Settings.exe` and `bin\XR_3DA.exe`, logged reading the metadata,
  created the two Start-menu entries of §5 and `installer_language` (§4.2) in the `Wow6432Node` view, then
  stopped with a stack overflow before running any script action (the container runs below did run them). A run on a script with no actions and metadata with no play tasks (and the
  installer's `uninstall.dll` in place) stopped the same way after reading the metadata.
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
- `XR_3DA.exe`, unmodified, under Wine's WoW64 build in the box64 emulator, with the five values of §4.1 imported
  into the 32-bit view (`InstallVers` = `2.1.0.10`, string; the key absent from the 64-bit view): it reached the
  title menu, whose caption read `ver. 1.0006` in the frames captured from outside; its log reported the value
  `InstallUserName` missing, not the key.

Second environment, the container runs: the project's headless container (Wine's WoW64 build under the box64
emulator, a fresh prefix per run, running as the user `root`, so `{userdocs}` is `C:\users\root\Documents`).
The interpreter ran unmodified with the §1 command line (`/DIR=Z:\work\app`, `/supportDir=Z:\work\app\__support`,
`/LOG` added) on a folder holding the interpreter, `__support\userdocs\`, `Settings.exe`, `bin\XR_3DA.exe`, a
script and a metadata file; it ran the whole script and its own install steps, exit code 0 unless stated. Test
scripts other than the shipped one were written for these runs; their actions append a label to a text file
(`saveToFile` with `append`) or write test values under `HKEY_CURRENT_USER\Software\Volya603` or
`HKEY_LOCAL_MACHINE\SOFTWARE\Volya603`, so their order and effects can be read afterwards.

| Run | Script, metadata, options | Shows |
|---|---|---|
| `real` | the shipped script and metadata | the actions, registry values, shortcuts and uninstall registration of §2–§5 |
| `t1` | actions with `order` values, a failing `execute`, kind `frobnicate`, value types, placeholders | §4.3 order, failure, kinds, value types, placeholders |
| `t4`, `t5` | labels with further `order` values | §4.3 order |
| `t2`, `t6` | roots `HKEY_BOGUS`, `hkey_current_user`, `HKLM`, `HKLM32`, `HKLM64` | §4.1, §4.3 roots (exit code 3 on the bad roots) |
| `t7a`, `t7b`, `t7c` | `onlanguagechange`, `onversionchange`; metadata without `language` (`t7a`, `t7b`) or shipped (`t7c`); a stored `language` = `russian` added before (`t7b`, `t7c`) | §4.3 Conditions |
| `t8`, `t9` | metadata cut off before its closing `}`; no metadata file (exit code 1) | §4.3 Conditions |
| `t10` | metadata with the `Settings` task limited to `de-DE`, `Поддержка` to `en-US` | §5 |
| `t11` | actions limited to `en-US` and to `[]`; option `/nodesktopshortcut` | §4.3 `languages`, §5 |
| `t3u` | install, then `{app}\unins000.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /galaxyclient /keepsaves`; actions with install and uninstall arguments and conditions | §4.3 Uninstalling |
| `g1` | `XR_3DA.exe`, unmodified, with the game files, the `fsgame.ltx` written in run `real` (read-only) in the install folder, working folder `bin`, 90 s, Wine's file and message-box traces | §2 |
| `orig` | the project's run of the original (`xtask original --quit 150`): the game files with a writable `fsgame.ltx`, title menu reached (frames captured from outside), quit from the console | §2 |
| `full`, `full2` | the whole installer, `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /DIR=Z:\work\app /LOG`; the interpreter's own log kept in `full2` | §1, §3, §4.2 |

## 9. Not established

- The interpreter's exact output: line ends and encoding of the written `fsgame.ltx` (§2), overwrite behaviour
  of `user.ltx` when its folder exists (§3), the effect of `savePath`.
- Why the sort by `order` groups the actions as observed rather than by ascending number, and its result for
  values not tried; the meaning of `deleteSubkeys` and `saveGameData`; which faults in a metadata or script file the
  JSON library rejects (§4.3).
- The value `lang_code` the installer itself writes and further `GOG.com` values it may write (§4.2); the root of
  the compatibility-flag entry (§5; **Disputed in review:** observed, see §5); the condition and position of `/nodesktopshortcut` (§1).
- Purpose of `traffic.net` and `banned.ltx`; whether `__redist` and `__support` are deleted after install.
- Whether the game writes `user.ltx` when it has none.

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
- Binary analysis (2026-11-26): static analysis of `bin\xrGameSpy.dll` and `bin\xrGame.dll` (Ghidra) for the
  game's read of `InstallVers` and the registry view it uses (§4.1, title-menu.md §3.4).
- Execution: the interpreter and the unmodified `XR_3DA.exe` run under Wine as described in §8.
- For §4.3, **Conditions** (2026-11-26): the interpreter's compiled Pascal Script code read with a script of our own
  (its types, procedures and instructions listed) for where `conditions` is read, how its elements are tested, the
  values they compare with and the identity `onlyonce` uses; static analysis only.
- For §4.3: the script's bytes and JSON members read; the interpreter's string constants (UTF-16 with a character
  count) and the names its compiled code imports listed; the JSON library it ships (`uninstall.dll`, extracted with
  innoextract) searched for member names; the interpreter run under Wine on test copies of the script (§8).
- No leaked source code, derived engines, third-party engine documentation or recollection was used. Inno Setup's
  public documentation was used only for the meaning of its folder constants, its uninstall registration, and its
  registry, INI and MD5 functions (the beliefs so marked in §4.3) and the list of its constants (§4.3, Placeholders).
- For the fix of 2026-11-27 (§2 read-only file, §3 failed copies, §4.1 registry view, §4.2 interpreter values and
  uninstall registration, §4.3 order, failure, kinds, roots, value types, `languages`, metadata language,
  uninstalling, placeholders, §5 shortcuts and task languages, §7 fatal box): the interpreter, the uninstaller it
  installs, the whole installer and the unmodified `XR_3DA.exe` run headless in the project's Wine container
  (§8, second environment), with test scripts and metadata of our own, Inno Setup's logs, registry listings, file
  listings and Wine's file and message-box traces read; the interpreter's compiled code read further with the same
  script of our own (placeholders, order, roots and value types, uninstall pass); the decompiled `bin\xrCore.dll`
  (Ghidra) read at the missing-`fsgame.ltx` message box.

## Gaps found in review

- A whole install (runs `full`, `full2` of §8, not run `real`) also writes, outside the product key: under
  `HKEY_LOCAL_MACHINE\SOFTWARE\GOG.com` (32-bit view) `DefaultPackPath` = the install folder's parent with a trailing
  `\` (`Z:\work\` for `/DIR=Z:\work\app`) and `DesktopIcon` = `ON`, a key `Dependencies` below it with a DWORD 1
  named by 32 hexadecimal digits, and under `HKEY_CURRENT_USER\Software\GOG.com` `DefaultMD5` = `OFF`; §4.2 does not
  list them, and their meaning was not established.
