# GOG offline installer: original Shadow of Chernobyl

Survey of the GOG offline installers (#4): the Russian edition (first priority, docs/roadmap.md) and the English (WorldWide) edition. Sources: observation of a legally obtained installer and the output of [innoextract](https://constexpr.org/innoextract/) 1.9 (zlib license) used as a measuring tool. No game content is reproduced here.

## Download

Each edition is a separate download of two files, both required:

| File | Size | Contents |
|---|---|---|
| `setup_s.t.a.l.k.e.r._shadow_of_chernobyl_2.1.0.10_gog_0.13_(russian)_(77895).exe` | 0.8 MiB | Russian: Windows PE loader with the Inno Setup headers (file list, compressed) |
| `setup_s.t.a.l.k.e.r._shadow_of_chernobyl_2.1.0.10_gog_0.13_(russian)_(77895)-1.bin` | 3.48 GiB | Russian: data slice 1 |
| `setup_s.t.a.l.k.e.r._shadow_of_chernobyl_2.1.0.10_gog_0.13_(77895).exe` | 0.8 MiB | English (multi-language): headers |
| `setup_s.t.a.l.k.e.r._shadow_of_chernobyl_2.1.0.10_gog_0.13_(77895)-1.bin` | 3.47 GiB | English (multi-language): data slice 1 |

`2.1.0.10` is GOG's installer version (the install script writes it as `InstallVers`), not the game's patch level; identifying the patch level is #8. GOG game ID `1207660573`.

## Format

- Inno Setup, setup data version **5.6.2 (Unicode)**. Not passworded.
- The `.bin` is a standard Inno data slice: signature `idska32\x1a`. Chunks start with `zlb\x1a`; most chunk payloads begin with a zlib header (`78 9c`), others appear stored. No RAR wrapping, no encryption seen; LZMA not seen.
- Russian installer: single language, every game file tagged `ru-RU`.
- English installer languages: de-DE, en-US, es-ES, fr-FR, it-IT, pl-PL, ru-RU, uk-UA, zh-Hant. Shared files are tagged with seven of them; pl-PL and ru-RU tag no files in this download. `gamedata.db9`, `ReadMe.txt`, the manual and GOG metadata are en-US only.

## GOG Galaxy part layer

Inno Setup does not store the game files under their names. It stores 1036 *parts* (Russian edition) as `{tmp}/<aa>/<bb>\<md5>` entries; GOG's install code reassembles them. Each part's Inno file entry carries Pascal-script conditions:

- `check`: `check_if_install('ru-RU#','32#64#','')` (language, bitness); redistributables use `check_if_install_dependency(...)`.
- On the first part of a file only: `before_install('<md5 of the whole file>', '<target path>', <part count>)` (`_dependency` variants for redistributables).
- On every part: `after_install('<md5 of the inflated part>', <compressed size>, <inflated size>)`.

Parts of one file follow each other in entry order. Each part is a zlib stream; every part but a file's last inflates to exactly 10 MiB (10485760 bytes), and the last takes the remainder, up to ~18 MB observed (so under 20 MiB); its Inno entry name is the MD5 of its compressed bytes. Verified on `gamedata.dbd`: inflating its single part gives exactly the bytes and MD5 that innoextract `--gog` produces. So extraction is: stream each part out of Inno, inflate, verify the part MD5, append to the target, verify the whole-file MD5.

## Contents

Both editions have the same layout and sizes within a few percent: 60 files, 5.58 GiB installed, 5.55 GiB of archives. The archives differ between editions (e.g. `gamedata.dba` 31 MiB Russian vs 21.8 MiB English, `gamedata.dbb` 52.6 vs 57.4 MiB, `gamedata.db5` 664 vs 660 MiB), so they are distinct builds, not a shared base plus a language pack. Whether the Russian build carries Russian voice-over is answered by reading the archives (#9).

### English (en-US selection)

60 files, 5.58 GiB installed, of which 5.55 GiB are archives:

- `gamedata.db0`–`gamedata.db8` (284–675 MiB each), `gamedata.db9` (en-US, 76 MiB), `gamedata.dba`–`gamedata.dbd` (0.1–57 MiB).
- `mods/mp_military_2.xdb0` (48 MiB, multiplayer map).
- `bin/`: the original Windows executables and DLLs (`XR_3DA.exe`, `xr*.dll`, ...). Volya Engine reads only the splash picture from `XR_3DA.exe` (docs/behavior/startup.md).
- `Settings.exe`, `ReadMe.txt`, `StalkerUserManual.pdf`, GOG metadata (`goggame-1207660573.*`), user-docs templates (`user.ltx`, `banned.ltx`).
- No `fsgame.ltx`: GOG's install script generates it ("Generate fsgame.ltx"). Volya Engine defines its own mounts.

## Implications

- The importer needs both files picked; validate by the `.exe`'s slice reference and the `.bin` signature.
- Extract only what the engine reads: `gamedata.db*` (plus `mods/*.xdb*` when multiplayer matters), ~5.5 GiB into OPFS. Skip `bin/`, redistributables and installer UI (`tmp/`).
- Unpacker (#5) must support Inno 5.6.2 Unicode headers, zlib and stored chunks, and per-file language conditions (select the installer's language; ru-RU for the Russian edition).
- Fingerprinting (#8) must tell the editions apart by their archives.
