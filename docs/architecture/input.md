# Input and controls

Every feature ships with both input paths, touch and keyboard, mouse or gamepad (AGENTS.md).

## From the page to the engine

- `static/input.js` reads pointer events (mouse, pen, touch), the keyboard by `KeyboardEvent.code` and Standard-mapping gamepads polled each frame. It forwards them as `{type: "input", event}` (format in `platforms/web/src/input.rs`) only while the engine owns the screen; otherwise the DOM interface keeps them. The engine turns this on with the `input_capture` op and tells the page with `{type: "capture", on}`.
- `volya-engine-core::input` reduces every source to the menu's mouse and keyboard vocabulary: touch is a touchpad wherever the game's cursor shows (below), elsewhere the touch point and the left button; a gamepad is the arrows (d-pad, left stick), Enter (south) and Escape (east). `input_state` reports it.
- Keys pressed with Control reach the engine only while the character takes keys (the level's controls live, or the inventory screen over them): Control is the crouch key. Chrome keeps Ctrl+W, Ctrl+T and Ctrl+N from a page outside full screen with Keyboard Lock (documented, not measured), hence [Full screen](#full-screen-while-playing).

## Touchpad

A Volya Engine addition, decided by the user (#108): touch is a touchpad for the cursor wherever the game shows it (`App::cursor_shown`: the title menu and its screens, options, the menu during a game, save and load screens, message boxes, the on-screen keyboard, the inventory screen). A touch never clicks where it lands there, text fields included.

- `static/touchpad.js` is a pure recognizer (our values):
  - a swipe anywhere moves the cursor by its motion;
  - a tap (lifted within 500 ms, moved at most 10 CSS pixels) clicks at the cursor;
  - a touch beginning less than 250 ms after a tap's lift (the engine's double-click time, `DOUBLE_CLICK_MS`) is a second touch: lifted as a tap it is a double click; moved past the slop or held 500 ms it is a single press held until lifted, so double-tap and hold, then move, drags (sliders, scroll bars, list thumbs) and a drag never acts as a double click (it never loads a save);
  - the button a drag holds is tracked apart from the gesture and released when its finger lifts, on a cancelled touch and on a reset (blur, the cursor hiding, capture changing);
  - two fingers swiped up or down scroll, one wheel step per 40 CSS pixels of their mean travel, content following the fingers;
  - a long press (500 ms, Android's timeout, within 10 pixels) is a right click, at the timer or at the lift if that comes first.
- It sends semantic messages: `cursor_motion` with the swipe's speed smoothed over 75 ms, `cursor_button` with the recognizer's double-click flag, `wheel`. The engine keeps the touchpad's buttons apart from direct touch's (`Source::Touchpad`), applies the acceleration (`volya-engine-core::input::touchpad_gain`: one to one below 200 CSS px/s, rising linearly to 2.5 times at 1200, level beyond), then the menu's motion rule (title-menu.md section 7).
- The mode follows `cursor_shown`, told to the page with `{type: "cursor", shown}`. A touchpad touch stays one until lifted; direct touches still down when the cursor appears are dropped, their button released without moving the cursor (`touch_drop`).
- While the cursor shows the page cancels `touchend`, which suppresses the compatibility mouse events and the click (documented: Touch Events specification), so none blurs a text entry just focused.

## The level's controls

- `volya-engine::controls` maps every path onto the game's movement actions: keyboard and mouse by the game's bindings, gamepad sticks and buttons, and the on-screen touch controls. The engine owns their logic (the sprint lock, `SprintLock`; the look curve over the drag speed the page measures, `look_gain`).
- The look curve: one mouse count per CSS pixel up to 300 px/s, rising linearly to 1.6 at 1,500 px/s, level beyond; the counts turn by `mouse_sens` as a mouse's do (0.1375° per count at the default 0.12). Measured by game-input's look-swipe test, a swipe of the controls' width at the default sensitivity: iPhone 17 Pro Max Simulator, upright (turned, 796 px): 109° at 250 px/s, 143° at 1,000, 170° at 3,900 (a 0.2 s flick); desktop Chrome (756 px): 104°, 136°, 161°. Landscape is not measured (the Simulator runs upright); by the same curve, controls 956 px wide (believed, not measured) would turn 131° slow and up to 210° quick.
- Mouse look runs under Pointer Lock, taken on a click in the level and released by the page when the level's controls stop. A lost lock opens the menu, since the browser takes Escape to leave it.
- Touch controls (`static/touch-controls.js`): DOM elements over the canvas, shown on touch devices (a coarse pointer, or a touch seen) while the level's controls are live. They send semantic messages, not raw contacts: the stick's reach (past its rim too), look drags, buttons and the Crouch swipe. Menu at the top left, Console right of it (#642, below), Inventory at the top right (our design, #110; its glyph is our backpack, as the UI textures have no inventory icon); Torch and Night vision in a row left of Inventory (#363), each shown only while its command would do something. The page draws them and places the sprint lock's target.
- They look like the game's HUD instruments (#105), drawn from its own UI textures, which the engine decodes for the page once per session (`ui_images` op through `volya-engine::ui_image`: by texture path, texture description id or an item's inventory icon, optionally cut to a rectangle or turned into a silhouette or an engraving, returned as RGBA; `static/touch-art.js`). Nothing of the game is shipped: until then, or without game data (the UX audit page), vector approximations in the same palette stand in.
- `{type: "controls"}` tells the page when the level's controls go live or stop and the toggles they show.

## Text entry without a keyboard

A Volya Engine addition, decided by the user (#94), for the save screen's name field. The field keeps its own rules (the characters its keys type, its width limit); physical keyboards type as before.

- Touch (`static/text-entry.js`): the field is clicked with the touchpad's cursor like everything else. The engine reports the field's place, focus and whether its cursor is over it (`{type: "text_field"}`; nothing while the on-screen keyboard or a box covers the screen). On touch devices the page keeps an unseen text input there that never takes taps itself (`pointer-events: none`).
- A device's keyboard rises only for an input focused within a user gesture (iOS; believed, not measured), so when a touchpad gesture presses the left button over the field, the page focuses the input in that very event handler, and the same press focuses the engine's field as a click does; if the engine doesn't focus its field within 500 ms the input is blurred again.
- Typed characters go as text (`["text", s]`, `Event::Text`), the input emptied at once; typographic quotes become plain ones; an input method's composed text is sent when composing ends; Backspace, Delete, Enter, Left and Right go as keys. The field losing focus (Enter) closes the device's keyboard.
- Gamepad: an on-screen keyboard drawn by the engine (`volya-engine::on_screen_keyboard`), flat keys over the screen's bottom: the field's character keys as on a US keyboard, Shift (a toggle), Space, Backspace and Done, the last four drawn as line symbols. South in the field focuses it and opens the keyboard, which then takes the gamepad (d-pad moves, south presses, west is Backspace, east is Done) until the field loses focus. While it shows, mouse and touch don't reach the screen under it; keys do.

## The console on touch screens

A Volya Engine addition, decided by the user (#642, the proposal on #609): the original opens the console only with a key and types into it only by key codes (console-window.md section 1, console-input.md section 2). Keyboards type as before; on touch devices (`static/console-touch.js`):

- A Console button (`>_`, a 44 px disc like the touch controls'): in a level right of Menu (`static/touch-controls.js`), over the main menu at the same top-left place while a key bound to `console` would open the console there (`{type: "console", open, menu}`, `App::console_opens_over_menu`). It presses `TouchButton::Console`, which opens the console where and when such a key does, and is hidden while the console is open.
- The same tap focuses an unseen text input, so the device's keyboard rises: the button acts on the tap's release, as a touch's pointerup is a user activation and its pointerdown isn't (documented: HTML's activation-triggering events; that iOS raises the keyboard there is believed, as for the save field). If the console doesn't open within 500 ms the input is blurred again.
- Typed characters go as `["typed", s]`; the engine turns each into the presses of the key that types it on the console's table (`console::typing_key`), within a left Shift press where it needs one: upper-case letters are their key, typographic quotes the plain ones, other characters (Cyrillic, `€`, emoji) do nothing. The keyboard's Backspace and Return are Backspace and Enter; its own paste is what Insert does with the pasted text.
- A key strip just above the device's keyboard (at the visual viewport's bottom; that iOS shrinks it for the keyboard is believed), upright even when the game is turned: Esc, Tab, ↑, ↓, PgUp, PgDn and Paste. Each is that key, held from press to release, so the console's own key repeat applies; Paste is Insert on its tap's release, reading the clipboard there (input.js). Strip taps take no focus from the input.
- Hiding the keyboard leaves the console open and the strip goes; a tap on the console's area (the top half) raises the keyboard again. The console closing closes it.

## Full screen while playing

A Volya Engine addition, decided by the user (#85): the game crouches on Control, and outside a full screen the page requested, with Keyboard Lock, the browser keeps Ctrl+W, Ctrl+T and Ctrl+N from the page (documented: the Keyboard Lock specification, Chrome's article). Offered, never imposed (`static/fullscreen.js`).

- The offer ("Full screen", a note that it keeps such shortcuts in the game and that holding Esc leaves it, a close control) shows in the level's top right corner, with the system pointer, where `navigator.keyboard.lock`, `requestFullscreen` and `fullscreenEnabled` exist and the pointer isn't coarse (desktop Chrome; Safari and Firefox lack the API, documented by MDN), while the level's controls are live and the mouse is free. Not on macOS (`navigator.userAgentData.platform`, else `navigator.platform`): its browser shortcuts use Command, which is never locked (our choice).
- At most once per game (our choice): once hidden (by the click that takes pointer lock, the menu, accepting or closing it) it doesn't come back until the next game. Presses on it stay with the page.
- Accepting, in the click (full screen needs a user gesture): `navigator.keyboard.lock()` with KeyW, KeyT, KeyN and Escape and every key bound to an action (the `bound_keys` op, read as the controls go live), minus Tab, F4 and the OS keys, which stay the system's even when bound (our choice); then `requestFullscreen()` on the page's root, lock first as the specification recommends, so the browser shows one exit notice. Keyboard Lock works only in a full screen the page requested, not F11's (documented).
- Under the lock a short Escape reaches the game, which opens the menu; holding it two seconds leaves full screen, and the hold begins as a press the page receives, so the menu opens on the way out (documented; not measured). Pointer lock works inside full screen (documented); whether a short Escape releases it is not measured, and either way the menu opens.
- Bindings changed while in full screen are locked anew as the controls go live again. Leaving full screen ends the lock; when the player leaves the game the page leaves the full screen it entered, then unlocks.

## The options screen's full screen

A deviation decided by the user: the options screen's "Полный экран" (`rs_fullscreen`, display-mode.md A.2, A.9) is the browser's full screen for the page, not a display mode.

- `rs_fullscreen` follows the page: the page tells the engine at start and at every `fullscreenchange` whether it is in full screen (`full_screen`), and the engine sets the variable to match, so the check box shows what is (a shipped `rs_fullscreen on` reads off at start: a page can't enter full screen without a user gesture).
- At each video restart (the options screen's Apply or Cancel, display-mode.md A.9) a value differing from the page's is asked of it (`full_screen_request`): entered as the offer enters it, the keys locked where the offer would lock them, or left. The restart follows a click or key on the screen, so the browser's transient user activation still holds (documented: HTML's user activation); a gamepad's press gives none, and a refused request leaves the page as it was and the box off. Leaving the game leaves it, as the offer's.
- Where the page has no full screen (`requestFullscreen` missing or `fullscreenEnabled` false: Safari on iPhone, documented by MDN) the check box is disabled and the value kept (options-screens.md 6.3's disabled state).
