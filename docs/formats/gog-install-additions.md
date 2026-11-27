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
`gog-cond/scripts/checks.py` (which reads the interpreter's compiled code with `gog-cond/scripts/ifps.py`).

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

So `InstallVers` **is** written under the key the game queries for its version string
(`docs/behavior/title-menu.md` §3.4: value `InstallVers` of `HKEY_LOCAL_MACHINE\Software\GSC Game World\STALKER-SHOC\`;
key names are case-insensitive in Windows). It is written by the interpreter's script, not by the Inno Setup
registry entries, which contain no `GSC Game World` key at all. The game discards the data it reads, so the
title-menu caption reads `ver. 1.0006` on a GOG install too (title-menu.md §3.4; observed, §8).

Registry view: the interpreter is a 32-bit program, as are all of the game's executables and libraries. On 64-bit
Windows both therefore use the 32-bit view of `HKEY_LOCAL_MACHINE\SOFTWARE`
(`HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\…`; Windows' documented registry redirection). The game opens its key
without a registry-view flag, so it reads that 32-bit view (title-menu.md §3.4); that it sees what the script
wrote rests on the script's writes landing there. Observed under Wine: the interpreter's `HKEY_LOCAL_MACHINE\SOFTWARE` writes land in the
`Wow6432Node` view (§8). **Disputed in review:** being a 32-bit program does not by itself settle the view, since
the interpreter can address both views (it contains the root names `HKLM32` and `HKLM64`); the only observed write
is `installer_language` (§4.2), not one of the script's values, so that the five values above land in the 32-bit
view is believed, not established.

Read by the game (names in its libraries, under `Software\GSC Game World\STALKER-SHOC\`): `InstallVers`
(queried, but the data is not used: the version string is always `1.0006`, title-menu.md §3.4) and
`InstallPatchID` by the GameSpy library; `InstallUserName` and `InstallCDKEY` by
the game library. GOG writes neither `InstallPatchID`, `InstallUserName` nor `InstallCDKEY`; no game binary
contains the names `InstallPath`, `InstallSource` or `BonusPack`. What the game does without
`InstallPatchID`, `InstallUserName` or `InstallCDKEY` is out of scope here (none is needed to reach the title
menu per `startup.md`).

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
`uninstall` (the counterpart of `install`, used when uninstalling) and `order`; neither occurs in this script. How
`order` is written and evaluated was not established. `conditions` is not a member of the action but of
`install.arguments` (**Conditions** below).

`languages`: the name `languages` occurs nowhere in the interpreter (neither in its executable, nor in its compiled
code, nor in the JSON library it uses, in ASCII or UTF-16), so this interpreter is believed to ignore the member and
to run every action whatever the install language. In this script the question does not arise: every action has
`["*"]`. The script has no architecture or Windows-version field; the only architecture choice the interpreter
offers is in the registry root (below).

**Action kinds.** The interpreter knows the kinds `execute`, `setregistry`, `setini`, `supportdata`, `savetofile`,
`installsdb`, `deletedata`, `createshortcut`, `savepath`, `delregistry` and `pass`, and maps the older name
`saveGame` to `savePath`. Its code holds the kind names in lower case and uses a lower-casing operation, while the
script spells them `saveToFile`, `execute`, `supportData`, `savePath`, `setRegistry`; believed: the kind is matched
without regard to case. A reader of this script should therefore compare the kind case-insensitively.

**`setRegistry` arguments.** In this script exactly these five members, all JSON strings:

| Member | Meaning | Spellings |
|---|---|---|
| `root` | registry root | the interpreter knows `HKEY_LOCAL_MACHINE`, `HKLM`, `HKLM32`, `HKLM64`, `HKEY_CURRENT_USER`, `HKCU`, `HKEY_CLASSES_ROOT`, `HKCR`, `HKEY_CURRENT_CONFIG`, `HKCC`, `HKEY_USERS`, `HKU`; this script uses only `HKEY_LOCAL_MACHINE` |
| `subkey` | key path below the root, `\`-separated, no leading `\` | here `SOFTWARE\GSC Game World\STALKER-SHOC` |
| `valueName` | name of the value | |
| `valueType` | type of the value | the interpreter knows `string`, `expandstring`, `multistring`, `dword`, `binary` (and `none`); this script uses `string` and `dword` |
| `valueData` | the data, as text | for `string` the text itself; for `dword` a number in text, here `$1` |

The interpreter also knows the optional `setRegistry` members `conditions` (**Conditions** below), `deleteSubkeys`
and `saveGameData`, absent here (the meaning of the last two was not established). Believed: `string`, `expandstring`, `multistring`, `dword`,
`binary` are the Windows registry types REG_SZ, REG_EXPAND_SZ, REG_MULTI_SZ, REG_DWORD and REG_BINARY (its code
has write operations for exactly these five types and none for a 64-bit number). Believed: `$` in a `dword`'s
data marks hexadecimal, so `$1` is 1 (§4.1). Believed (§4.1 and its review mark): root `HKEY_LOCAL_MACHINE`, as
opposed to `HKLM64`, is the 32-bit view on 64-bit Windows.

**Conditions.** An action may carry conditions that decide whether it runs. Established from the interpreter's
compiled code (ISI version 0.1.2.538); no script with conditions was run (§8).

- **Where.** The optional member `conditions` of `install.arguments`, a JSON array. It is read for the kinds
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

  Believed (Inno Setup's documented registry and INI functions): a missing registry value reads as empty, so
  `onlanguagechange` and `onversionchange` hold when their value is absent. The registry view of the two reads is
  that of the interpreter's other `HKEY_LOCAL_MACHINE` accesses (§4.1, believed the 32-bit view). How the
  interpreter encodes `goglog.ini` on disk was not established.

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
  the installer's header or by the interpreter other than in these reads and the metadata lookup, so no value
  `language` exists under `<key>` and `onlanguagechange` holds (believed: nothing else writes it); `onlyonce` holds
  (no `goglog.ini` key yet); the installer itself writes `ver` under `<key>` with the same text as its `/versionName`
  (§4.2) before it starts the interpreter (§1), so `onversionchange` does not hold (believed: the order of §1); the
  adapter words depend on the machine's display adapters.

**Placeholders.** Text in braces is replaced before use. Those occurring in this script: `{app}` (the install
folder, `/DIR` of §1), `{supportDir}` (the `/supportDir` value of §1, `{app}\__support`), `{userdocs}` and `{sys}`
(Inno Setup's constants, see Notation). Others the interpreter knows: `{win}`, `{tmp}`, `{syswow64}`, `{sd}`,
`{pf}`, `{cf}`, `{group}`, `{localappdata}`, `{userappdata}`, `{commonappdata}` (Inno Setup's folder constants),
and its own `{productID}`, `{rootProductID}`, `{version}`, `{buildID}`, `{gameName}`, `{language}`, `{galaxy}`,
`{n}` (meanings believed from their names and the §1 command line: the product id, the base product id, the
version, the build id, the game name, the install language, the GOG Galaxy client's executable read from
`HKLM\SOFTWARE\GOG.com\GalaxyClient`; `{n}` not established). Paths in the script may use `/` as separator
(`{app}/fsgame.ltx`); believed accepted by Windows as `\`. Whether placeholders are replaced in `valueData` was not
observed; `{app}` in `InstallPath` is believed replaced (§4.1). The `InstallVers` data holds no placeholder, so
its value does not depend on the question.

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

Under Wine the interpreter stops after reading the metadata even with an empty action list and no play tasks
(§8), so none of this was observed in a run.

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
`user.ltx` in `<Documents>\Stalker-SHOC\` only for the shipped default settings (§3: not required); no
registry value (without the key the game logs that it is missing and goes on; the title menu's version caption
does not depend on `InstallVers`, title-menu.md §3.4).

## 8. Black-box runs (Wine)

Under Wine (Wine Stable, a dedicated prefix with its audio driver disabled):

- The interpreter, started with the §1 command line (`/Language=Russian /LANG=russian`) on a folder holding the
  script, the metadata, `__support\userdocs\`, `Settings.exe` and `bin\XR_3DA.exe`, logged reading the metadata,
  created the two Start-menu entries of §5 and `installer_language` (§4.2) in the `Wow6432Node` view, then
  stopped with a stack overflow before running any script action. The script actions (§2–§4.1) are therefore
  established from the script and the interpreter's data, not from an observed run. A run on a script with no actions and metadata with no play tasks (and the
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

## 9. Not established

- The interpreter's exact output: line ends and encoding of the written `fsgame.ltx` (§2), overwrite behaviour
  of `user.ltx` (§3), the effect of `savePath`, the DWORD value of `$1` (believed 1), trailing `\` in `{app}` and
  `{userdocs}` (believed none).
- How the script's `order` member is written and evaluated, the meaning of `deleteSubkeys` and
  `saveGameData`, whether the action kind and `languages` are handled as believed, and whether placeholders are
  replaced in `valueData` (§4.3).
- The `lang_code` value and further `GOG.com` registry values (§4.2); the root of the compatibility-flag entry
  (§5); the name and location of the game's own shortcut (§5); the condition and position of
  `/nodesktopshortcut` (§1).
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
  registry, INI and MD5 functions (the beliefs so marked in §4.3).

## Gaps found in review

- The uninstall registration (key, value names and data) is only believed from Inno Setup's documentation and is not listed in §9.
- The `Поддержка` play task is limited to the language `ru-RU` in the metadata (the other two tasks to `*`); whether the interpreter skips it for other install languages was not established.
- Whether the game copes with `fsgame.ltx` carrying the read-only attribute was not checked by a run (the runs used a writable copy), nor that the game never writes it.
- Whether the script actions run in the order listed, and whether a failing action stops the later ones (e.g. the registry values), was not established.
- Placeholders of Inno Setup's `{reg:…}` form occur in the interpreter (its `{galaxy}` is defined through two of them); whether the script's text may hold Inno Setup constants beyond those listed in §4.3, and how they are replaced, was not established.
- What the interpreter does with an action whose kind it does not know, or a `setRegistry` action whose `root` or `valueType` it does not know, was not established.
- When the metadata file is missing or invalid, or has no string member `language`, what `onlanguagechange` compares with was not stated (believed: empty text, so it then holds only when a stored `language` value exists).
- Whether `conditions` in an action's `uninstall.arguments` decide whether it runs when uninstalling was not established.
