# Game data: bring your own installer, local only

Game data never reaches any server. The user brings the GOG offline installer (the Russian edition of the original Shadow of Chernobyl first; docs/roadmap.md); GOG's servers can't be used from the browser (no CORS on auth, API, content system or CDN; believed).

## Import

- **Eager inflation, the same on every platform.** The importer (`static/import-worker.js`, `platforms/web/src/import.rs`, over `volya-engine-installer`) inflates every game file once into OPFS; the engine then reads plain files through the OPFS VFS with true random access: no on-demand inflation, no decompression cache.
- `volya-engine-installer::extract` finds each file's parts at their offsets in the `.bin` (docs/formats/gog-installer.md), inflates them with `zlib-rs` as streams through fixed buffers (4 MiB read, 1 MiB written at a time, so the worker's heap doesn't grow with the parts), verifies part and file MD5s, and writes the file.
- One import worker, file by file. Parts are independent zlib streams, so inflation can be spread across workers (#305).
- **Inno Setup only for headers.** The `inno` crate parses the `.exe` headers to find each game file's parts; only the installer crate uses it. It needs file install conditions exposed, so the build applies `third_party/inno/expose-file-conditions.patch` to the crates.io release (docs/architecture/build.md, Rust).
- Remembering the installer's file handles, so a re-import needs no re-picking: #306.

## Original formats, no conversion

- The engine reads the game files as the importer extracted them: textures upload as stored (BC when the GPU has `texture-compression-bc`; transcoded at run time where it hasn't, [rendering.md](rendering.md#textures)), audio and video are decoded from the original Vorbis and Theora, models and levels are parsed from their original formats. No install-time transcoding or cooking; reconsider only when measurements demand it.
- **Detect from the data, never by edition.** No edition-specific code paths and no tables of known editions: formats are decoded by what the data says (headers, chunk types, flags, paths), and content differences (languages, patches, mods) are resolved by what files exist. Edition identity (the MD5s in `import.json`, and the installer's file names, `AppName` and `AppVersion` it records) serves diagnostics only. The title menu's version string is the built-in `1.0006` whatever the registry holds (title-menu.md 3.4), so neither the installer's registry entries nor GOG's install script are read for it.
- The engine's own files live beside the game in the OPFS namespace, never in the game's files: `user.ltx` ([screens.md](screens.md#settings)), `savedgames/` ([screens.md](screens.md#saved-games)), the diagnostic log ([diagnostics.md](diagnostics.md#storage)).

## Storage

- Sizes behind the import advice (measured): the extracted game is 5.55 GiB. Desktop Chrome: installer and extraction, 9.0 GB at peak and at rest, 5.55 GiB if the user deletes the installer. iPhone Home Screen app (import must run inside the installed app, whose storage is separate from the browser's): iOS copies every picked file to a temporary location, so the peak is installer, copy and extraction, 12.5 GB; 9.0 GB at rest.
- Before importing, `importGame` (`static/game-ui.js`) checks `storage.estimate()` (the browser's allowance for the app, not the disk's free space; the error says so), then requests `storage.persist()`.
- Persistence: granted in the installed Home Screen app on the reference device, added from Chrome; not in a plain browser tab (measured). Desktop Chrome never prompts: it grants silently when the site is installed or bookmarked, used a lot, or allowed notifications, and decides again on each request (documented: web.dev, Persistent storage); a fresh profile's tab is denied (measured, #63). Best-effort storage is evicted under storage pressure, least recently used site first, with no time limit in Chrome (documented).
- So the import screen gives this device's advice for keeping the game before importing (install the app; on iPhone, import in the Home Screen app), hidden when storage is already persistent or the app installed. Importing in a tab anyway accepts the risk: no storage warnings after.
- A finished import sets localStorage `volya-engine.imported`. Chromium's eviction clears the site's quota-managed storage (the imported files, the service worker's cache) but not localStorage or cookies (measured), so game files the browser deleted are said as such on the import screen; where the mark goes too (Safari's 7-day limit covers all script-writable storage; the user clearing site data) it's a first start. localStorage rather than a cookie: nothing is sent to the server.
- **OPFS is a cache**: it can be wiped (the user clearing data, deleting the app, eviction without persistence). At start `game_status` mounts the import, checking that every file in `import.json` exists with its size (not its MD5); a damaged import is reported at Play with the offer to import again; an unreadable `import.json` shows on the import screen with that offer; a partial import offers to continue.
- The origin is shared with unrelated sites ([web-platform.md](web-platform.md#hosting)): stored data may have been modified by others, so it is validated on load, never trusted.
