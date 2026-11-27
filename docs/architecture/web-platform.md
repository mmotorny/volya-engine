# Web platform

`platforms/web`: the crate `volya-engine-web` (wasm `cdylib`, the only package using `web-sys` but the render crate's GL layer) and `static/` (HTML, JS, CSS, icons).

## Threads and workers

- **The page** (`static/main.js`) owns the DOM interface and input. It starts the engine worker (`static/engine-worker.js`), transfers the canvas to it (`OffscreenCanvas`), and sends requests as `{type: "call", id, op, args}`, answered by id. The worker's own messages to the page (`capture`, `controls`, `cursor`, `text_field`, …) are listed at the top of `engine-worker.js`; ops live in `call` in `platforms/web/src/lib.rs`.
- **The engine worker** runs the engine (VFS, simulation, scripting, rendering, the sound system) on one thread. Single-threaded simulation matches the original's observable update order and keeps the simulation deterministic. A worker because OPFS `FileSystemSyncAccessHandle` is worker-only (documented, File System Standard), and synchronous reads let archive readers be plain `Read + Seek` Rust.
- **The audio worker** (`static/audio-worker.js`) runs the same wasm build in its own instance and mixes; the **AudioWorklet** plays ([audio.md](audio.md)). They share only the audio rings' `SharedArrayBuffer`: no wasm threads, no shared simulation state.
- **The import worker** (`static/import-worker.js`) unpacks the installer ([game-data.md](game-data.md)), by messages only.

## I/O is synchronous on the engine thread

- `Read + Seek` over OPFS sync access handles for parsing, loading screens and in-game loads alike, like the original engine (`platforms/web/src/opfs.rs`). Keep large reads out of the frame loop where practical; whether synchronous I/O hitches frames is #17.
- Game files are opened read-only, through web-sys's bindings marked unstable (`--cfg=web_sys_unstable_apis` for wasm32, MODULE.bazel): Chrome lets several copies of the app hold them, so two tabs or a tab and the installed app can both play (documented: Chrome 121); Safari ignores the mode and locks each file to one handle, so a second copy's Play reports its error (measured: `InvalidStateError`).
- A browser shows a presented frame only once the worker yields (believed), so a loading screen runs one load stage per engine frame: the frame that begins a stage draws its image, the next does that stage's work under it (`volya-engine::loading_screen`, docs/behavior/loading-screen.md).

## The page

- The app opens on its own interface (`static/game-ui.js`): home with Play once a game is imported, else the import screen; "This browser can't run the game", with the reason, instead of both when the page isn't cross-origin isolated or has no WebGL 2.
- Play launches the game, one start at a time (#369): the engine owns the screen (input capture on) and the page hides its interface until the player quits ("Yes" in the menu's quit box, or Q), which returns home.
- The page ↔ engine input path: [input.md](input.md).
- Settings (shown only while no game runs) has a danger zone: Delete settings and saved games (#535, shown while either is stored: `user_data_status`) removes the engine's `user.ltx` and `savedgames/` from OPFS (`forget_user_data`), so the next game reads the installer's configuration; Remove imported game removes the game. Each asks first, in the page (#538). Neither touches the diagnostic log, the shell's preferences (localStorage, the `volya-engine-settings` cache) or the offline copy.

## A hidden page

A Volya Engine addition, decided by the user (#93): every browser throttles or suspends hidden pages, so the engine stops while the page is hidden (another tab, a minimised window, a Home Screen app sent to the background).

- The page listens to `visibilitychange` and tells the engine worker (`{type: "visibility", hidden}`). It is the one event for all of these (documented by MDN; measured in the iOS Simulator's Safari for an app switch; a Home Screen app believed the same). Chrome's `freeze` only ever follows hidden (documented, Page Lifecycle API), so it needs no handler. `pagehide` ends the worker, and a `pageshow` from the back/forward cache reloads.
- Hidden, in the engine (`App::set_hidden`): in a level whose controls are live the menu opens, as a lost pointer lock opens it, guarded against an Escape closing it at once; the menu or one of its screens already open stays; during the loading and the intro nothing opens. Input focus counts as lost (the pause, [audio.md](audio.md#level-sounds)). No update runs, so the engine clock, game time, movement and movies stand still; the first update back takes the clock on from where it stopped.
- Hidden, in the worker and the page: the frame loop idles (no update, no drawing); the page suspends its AudioContext and sends the input a `blur` (everything released).
- Shown: the frame loop resumes and the page resumes audio (when the browser refuses without a gesture, the next gesture resumes it). The player resumes from the menu.

## Browser zoom

None on touch screens anywhere in the app, shell included (decided by the user, #345): once zoomed, the game takes every touch and the page can't be zoomed back out. Three layers, each of which alone stopped a real pinch in the iOS Simulator's Safari tab (measured, `cargo xtask sim` with `tests/browser/gestures.html`):

- The viewport meta adds `maximum-scale=1, user-scalable=no`. Safari ignores both in tabs since iOS 10 (documented: WebKit's "New Interaction Behaviors in iOS 10"; measured) and honours them in WKWebView apps that don't set `ignoresViewportScaleLimits` (documented); whether a Home Screen web app does is unknown. Kept for that and for other browsers.
- `touch-action: pan-x pan-y` on every element (`style.css`), as WebKit resets an element's touch-action at scroll containers (its source, `Style::Adjuster`); `none` where the engine takes input. WebKit's touch-action stops pinch zoom unless `pinch-zoom` or `manipulation` is given, and double-tap zoom only for `none` or `manipulation` (its source).
- `static/zoom.js` cancels WebKit's `gesturestart`, `gesturechange` and `gestureend` and any `touchmove` with two or more touches, on the document. Pointer events, which the game and the touchpad read, still arrive; one-finger moves aren't cancelled, so the shell scrolls; text fields still focus (measured).
- Stopping pinches is believed to stop double-tap zoom too: WebKit allows it on a `width=device-width` page only once its scale differs from the initial scale (its source). `touchend` and `dblclick` aren't cancelled: that would drop the shell's clicks, and the double-tap recogniser runs ahead of the page's events (believed).
- A page found zoomed anyway (`visualViewport.scale` not 1) can't be reset by the page: there is no API, and changing the viewport meta doesn't (measured). So `zoom.js` marks it (`data-zoomed`) and stops cancelling, and `style.css` gives every element `manipulation`, letting the player pinch back out; at scale 1 the guard is back.
- The `smoke` and `game-input` suites check the viewport, the computed touch-action, the guard's cancelling and the scale.

## Hosting

Served from `https://mmotorny.github.io/volya-engine/`, deployed with `bazel run //tools/xtask -- deploy` (a clean, pushed tree; `//:checks`; Bazel's release site, `//platforms/web:site`, stamped with the commit and force-pushed as one commit to the `gh-pages` branch, #12). Tests are never deployed. No custom domain: it emphasizes the project's free, non-commercial nature.

- **Base path `/volya-engine/`.** All URLs are relative; the dev server serves under the same path.
- **Shared origin.** Every `mmotorny.github.io` project site shares one origin, so storage quota, OPFS, IndexedDB, caches and service worker registrations are shared with unrelated sites. Keep everything under a `volya-engine` namespace (the OPFS directory, cache names, localStorage keys `volya-engine.*`), scope the service worker to `/volya-engine/`, and treat stored data as possibly modified by others: validate on load, never trust it blindly.

### Isolation

The audio rings need `SharedArrayBuffer`, which needs cross-origin isolation (COOP and COEP), which GitHub Pages can't send, so the service worker adds it (#354).

- Every response the service worker serves carries `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` (`sw.js` `ISOLATION`); `require-corp`, not `credentialless`, which Safari lacks (documented: MDN); everything the app loads is same-origin. A browser applies a navigation's headers as the service worker's response gives them (documented: the Fetch and HTML standards; the common coi-serviceworker method).
- A page the service worker didn't serve (the first visit, a hard reload) reloads once as soon as it can (`static/isolation.js`: at once if a worker is active, else when it claims the page after installing), before anything shows; at most once a page session (sessionStorage), so a browser ignoring the headers isn't reloaded in a loop. Measured to end isolated in headless Chrome and the iOS Simulator's Safari tab; a Home Screen app believed the same.
- Still not isolated, the app shows "This browser can't run the game" with why, before importing (UX audit board: `import-not-isolated`); there is no second audio path (decided by the user).
- Safari's module workers can fail to load when several load at once under service-worker-added COEP (WebKit bug 322883; documented there, not measured here), so ours start one at a time: the engine worker, then the audio worker once the engine has started.
- The dev server (`tools/devserver`) sends the app's page without the headers, as GitHub Pages does, and every other response with them, so test pages, which run without the service worker, are isolated. The `smoke` suite checks the page is isolated, the service worker's responses carry the headers, and the reload's rules.

### Service worker and updates

GitHub Pages sets its own short cache lifetimes, so the service worker (`static/sw.js`) owns app versioning, with the established precache pattern (as Workbox's injectManifest, without its Node tooling).

- The site's build (`tools/site`) injects every file but `sw.js` and `ux-audit/` with a hash of its content into `sw.js`, with the build, so the script changes exactly when the build does.
- Each build has a cache of its own, named by a hash of that list (#444): one shared cache let an installing version overwrite files the running one still loaded, so a page got the glue of one build and the wasm of another. Install copies files an earlier build's cache holds with the same hash and downloads the rest, each checked against its hash (a stale or half-deployed GitHub Pages response fails the install, retried at the next check). The active build is served from its cache only, offline included; a new version waits until the page asks it to take over, and its activation deletes the other builds' caches.
- What the service worker serves carries `Cache-Control: no-cache` (#446): with GitHub Pages' `max-age=600` passed on, Safari, after Finish update, gave the engine worker's `import` of the glue the old build's copy without asking the service worker, while `fetch` of the same URL in the same worker got the new one (measured in the iOS Simulator; persisting across reloads until Safari is closed, on the device; believed to be WebKit's memory cache). Chrome didn't (measured).
  An update from a version before #446 can still get the old build's copies in Safari, until they are 10 minutes old or Safari is closed (measured in the iOS Simulator, #450); between later versions it doesn't (measured).
- Finish update reloads on `controllerchange`, or after 3 seconds without it (the player asked; #450).
- The browser suites' runner serves the app's files with GitHub Pages' `max-age=600` and, on a test's request, a next build (`dist-next`: the same, its engine stamped with another id and `game-ui.js` changed); the `smoke` suite runs Finish update against it, on its own page (the harness resumes a test across a reload, `resumeAfterReload`) and in a copy of the app, and checks the engine, the version shown and the shell module are the other build's, also after a reload.
- The engine's wasm and its wasm-bindgen glue carry one build id (a custom section and an export, stamped by the site's build, `tools/site`); the engine worker refuses to start on a mismatch, naming it.
- Updates download automatically by default (`static/update.js`) and never restart the app by themselves: once downloaded, the page offers Finish update (as Chrome does), and closing and reopening the app finishes it too.
- Players can turn automatic downloads off in Settings (metered connections): then the install refuses without consent, tells the page the update's size, and a tap downloads it. The setting lives in a cache of its own (`volya-engine-settings`), where both can read it.
- The browser checks `sw.js` itself when the app opens; with automatic downloads the page also calls `registration.update()` on returning to the foreground, at most hourly.
- Settings' Check for updates (#120) calls it at once: an update found leads Home, whose update row downloads or finishes it; otherwise the result shows in the button's place for a few seconds. A failed `registration.update()` rejects with a TypeError whether the network is down or `sw.js` can't be had (Service Workers specification, Update; measured in Chrome and Safari with `navigator.onLine` still true), so it says offline only when `navigator.onLine` is false.
- One version is shown: the build's commit time and short hash (`versionText`); Cargo's SemVer isn't.
- The `smoke` suite runs the real `sw.js` against a fake cache and checks every file with the test server's network cut; the harness drops service workers left by earlier runs.

## UX audit page

`/volya-engine/ux-audit/` shows every state of the app's own interface on each device (`DEVICES` in `ux-audit/states.js`: desktop Chrome, iPhone 17 Pro Max portrait and landscape), like a design file (AGENTS.md: every UI change updates it).

- Each frame is the live interface (`style.css`, `game-ui.js`) with a fake engine and fake updates (`ux-audit/frame.js`), so the page can't drift from the app; states live in `ux-audit/states.js`, and a state's `devices` limits it to some devices.
- It is for the project, not players: not precached by the service worker, `noindex`. Its frames load the app's files through the service worker, so after a deploy a hard reload shows the new version.
- It flags states whose text differs by device, and keeps its view (zoom, devices, full length, scroll) in localStorage.
