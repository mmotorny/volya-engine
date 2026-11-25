# The game's screens

The game's own interface, drawn by the engine (`crates/volya-engine`). The app's own interface (home, import, Settings) is the page's: [web-platform.md](web-platform.md#the-page).

## Fatal errors

- What a spec makes fatal is fatal here, never recovered from (AGENTS.md). While a level loads, the error fails the load. Otherwise it is kept and taken by `App::fatal` at the end of the update: the game ends, as when loading fails, and the title menu says why.
- Routed there: the level (sound rules, the simulation's switch pass, a placed effect's group), the sound system's frame, the New Game intro, the inventory screen, the menu's screens (one that fails to open, a message box the data lacks, a save's menu data), fonts, and building a frame (`App::scene`: a missing texture, a font, text layout). `App::scene`'s `ErrorKind::Unsupported` (a layout our decoders lack) is not fatal.
- An error from the title menu itself, which already says why, leaves the menu as it is rather than opening it again each frame.
- Failures of the platform (storage, GPU uploads, the presenter's reads) aren't the original's and stay recoveries of the app shell; a storage read failing while drawing is fatal through `App::scene` (#339).

## Settings

- The options screen's settings and the key bindings live in a console (`volya-engine-core::console`): the variables of docs/behavior/options-screens.md section 7 with their kinds and ranges, the 77 actions' bindings, and every other `<name> <value>` line of the user configuration file kept as written, so settings of systems Volya Engine lacks survive. Lines a preset's script alone sets are not kept (the script sets them again at the next start); with that, the installer's `user.ltx` read and saved again is written byte for byte as shipped (`tests/options.rs`).
- The user configuration file (section 12) is Volya Engine's own `user.ltx` in the OPFS namespace beside the imported game, never written into the game's files. Without one, the installer's copy in the game root (`__support/userdocs/user.ltx`) is read. It is stored when the options screen closes and when the player quits from the menu: the original stores it only at exit, but a browser tab is usually closed without one.
- What the settings do: `vid_mode` sets the size drawn at (at the video restart Apply and Cancel run; [rendering.md](rendering.md#the-canvas-and-the-video-mode)); `snd_volume_eff` and `snd_volume_music` the mixer's effects and music volumes, at once; `mouse_sens`, `mouse_invert`, `g_always_run` and the bindings the player's controls, of a running level too once applied; the `quit` binding which key skips movies; New Game sets `g_game_difficulty`. The other settings are shown, edited and stored only.
- The options screen opens from the title menu and from the menu during a game.
- A deviation decided by the user (#364): Volya Engine draws only the static renderer, so the renderer list's dynamic entries, `renderer_r2a` and `renderer_r2`, are disabled (`options::DYNAMIC_RENDERERS`): drawn as options-screens.md 4.6's disabled rows (text alpha 128), never chosen by mouse, touchpad or gamepad (the list has no keyboard control), so the restart message (section 10) doesn't open. A stored `renderer` naming one (a `user.ltx` written by the original) is kept as written and shown as `renderer_r1`, without counting as a change, so Apply doesn't rewrite it. The controls enabled only with entries 1 or 2 (section 6.3) stay disabled. The dynamic renderers: #385.

## The menu during a game

- The key bound to `quit` (and a gamepad's east button, the touch Menu button, and losing pointer lock, since the browser takes Escape to leave it) opens the menu over the level once the loading image is gone. Asked for while the image covers the level's first frames, it opens in the first frame the player's controls are live, after the intro (a Volya Engine addition, decided by the user, #96; presses during the intro skip it). On the menu's main page the key closes it again (`volya-engine::menu_screens`, docs/behavior/save-load-screens.md section 2).
- The level stays loaded and paused; its world stays in the scene, covered, so its GPU copy is kept while nothing of it is drawn.
- The save and load screens and the boxes are mouse screens; touch reaches them through the touchpad, gamepads through the options screen's mapping (`volya-engine::dialog`). Typing a save's name: [input.md](input.md#text-entry-without-a-keyboard).

## Saved games

- Saves live in `savedgames/` of the OPFS namespace (`$game_saves$`, beside `user.ltx`), behind the product's `SaveFiles` trait.
- The engine worker (`platforms/web/src/game.rs`) lists it when the game starts: every file with its modification time, and for the `.sav` files in it the first 8 bytes (all the validity test needs) and a `File` snapshot, from which the load screen reads a selected save synchronously (`FileReaderSync`). No access handle is held, so another copy of the app can delete saves meanwhile; a stale snapshot fails to read and counts as invalid.
- Files the player deleted come back through `App::take_deleted_saves`; the worker removes each, one file at a time, never a directory.
- Writing and loading saves: #75 (`TODO(#75)` in `volya-engine::save_load`).

## The inventory screen

- docs/behavior/inventory-screen.md, in `volya-engine::inventory_screen` (#110). Its layout is loaded when the level is made (the `_16` rule of that moment); the moves between places act on the player character's inventory (`volya-engine-core::character::inventory`). Its open parts: #111.
- The key bound to `inventory` opens it over the running level (a gamepad's north button and the touch Inventory button too: our design); the game is not paused.
- While it shows, only the keyboard's keys it doesn't use reach the character (`Controls::keys_only`); mouse, wheel, gamepad and touch are the screen's. The level's controls count as stopped, so the page hides its touch controls, releases pointer lock (the cursor follows the absolute pointer; an exit the page makes releases no held key and opens no menu, so a held W walks on) and turns touch into the touchpad.
- The quit key closes it (the spec's Escape); the page being hidden opens the menu over it, as in a level (our choice).
- The `debug_give_item` op puts an item into the inventory with an id from the simulation's pool (test builds only: the `test-ops` feature of `volya-engine-web`, which `cargo xtask e2e` and `web --dev` enable and the deployed build lacks).
