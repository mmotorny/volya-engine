# GOG offline installer: original Shadow of Chernobyl

Survey of the GOG offline installer (#4). Sources: observation of a legally obtained installer and the output of [innoextract](https://constexpr.org/innoextract/) 1.9 (zlib license) used as a measuring tool. No game content is reproduced here.

## Download

Two files, both required:

| File | Size | Contents |
|---|---|---|
| `setup_s.t.a.l.k.e.r._shadow_of_chernobyl_2.1.0.10_gog_0.13_(77895).exe` | 0.8 MiB | Windows PE loader with the Inno Setup headers (file list, compressed) |
| `setup_s.t.a.l.k.e.r._shadow_of_chernobyl_2.1.0.10_gog_0.13_(77895)-1.bin` | 3.47 GiB | Data slice 1 |

`2.1.0.10` is GOG's installer version (the install script writes it as `InstallVers`), not the game's patch level; identifying the patch level is #8. GOG game ID `1207660573`.

## Format

- Inno Setup, setup data version **5.6.2 (Unicode)**. Not passworded.
- The `.bin` is a standard Inno data slice: signature `idska32\x1a`. Chunks start with `zlb\x1a`; most chunk payloads begin with a zlib header (`78 9c`), others appear stored. No RAR wrapping, no encryption seen; LZMA not seen.
- Languages: de-DE, en-US, es-ES, fr-FR, it-IT, pl-PL, ru-RU, uk-UA, zh-Hant. Shared files are tagged with seven of them; pl-PL and ru-RU tag no files in this download. `gamedata.db9`, `ReadMe.txt`, the manual and GOG metadata are en-US only.

## Contents (en-US selection)

60 files, 5.58 GiB installed, of which 5.55 GiB are archives:

- `gamedata.db0`–`gamedata.db8` (284–675 MiB each), `gamedata.db9` (en-US, 76 MiB), `gamedata.dba`–`gamedata.dbd` (0.1–57 MiB).
- `mods/mp_military_2.xdb0` (48 MiB, multiplayer map).
- `bin/`: the original Windows executables and DLLs (`XR_3DA.exe`, `xr*.dll`, ...). Not needed by Volya.
- `Settings.exe`, `ReadMe.txt`, `StalkerUserManual.pdf`, GOG metadata (`goggame-1207660573.*`), user-docs templates (`user.ltx`, `banned.ltx`).
- No `fsgame.ltx`: GOG's install script generates it ("Generate fsgame.ltx"). Volya defines its own mounts.

## Implications

- The importer needs both files picked; validate by the `.exe`'s slice reference and the `.bin` signature.
- Extract only what the engine reads: `gamedata.db*` (plus `mods/*.xdb*` when multiplayer matters), ~5.5 GiB into OPFS. Skip `bin/`, redistributables and installer UI (`tmp/`).
- Unpacker (#5) must support Inno 5.6.2 Unicode headers, zlib and stored chunks, and per-file language conditions (select en-US).
