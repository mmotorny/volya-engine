# Talk window opening and input

Functional specification of how the talk screen of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition)
opens and closes and of all its input in the single-player game: what opening and closing do to the game, the
player character, the cursor and the HUD; the information portions `ui_talk` and `ui_talk_hide`; the keyboard
(`use`, `quit`, `pause`, the console, every other key) and the mouse (a click on a question, message 46; a click
on the trade button, message 45; the wheel and dragging on the two lists); what the screen does on each of its
updates while it is shown (when it closes itself, the refill of the questions, the turn of the camera toward the
partner); the other in-game windows (inventory, PDA, body search, level change, tutorials) and a talk starting
while one of them is open; saving while a talk is open; and the set of choices and events the original's input
gives, from which the touch and gamepad paths are designed.

It is the spec DG-6 of `dialogue-scope` (section 4), whose section 2.7 and "Gaps found in review" apply (the
gaps on whether the game goes on while the window is open and on saving during a talk are answered in sections 2
and 8). It builds on, without repeating, these approved specifications (`docs/behavior/` unless noted), cited by
name and section:

- `talk-window` (DG-5): the talk screen and its talk window, their elements, the **talk start** (the set-up of
  the window for a new talk, its sections 4, 5, 6, 7, 8, 9), the **special mode** of a partner (section 9), the
  question list's refill (section 7);
- `use-action` (cited as `use`): the use action, its talk with the living (6.2), the use flag (1.3);
- `script-api-dialogs-and-trade` (cited as `R`): `run_talk_dialog` (4.2), `is_talking`, `stop_talk` (4.1),
  `switch_to_trade` (5.1), `set_trader_sound` (6);
- `console-key-bindings` (cited as `keys`): how a key press reaches the level and the in-game interface, the
  key's **action** (section 5, rule 1);
- `inventory-screen` (cited as `inv`): opening and closing an in-game window (1.1 to 1.4), the keyboard and mouse
  summary (6);
- `pda-window` (cited as `pda`): the PDA's opening, closing and input (4 to 7), the death rule and the other
  windows (5, 6);
- `save-load-screens` (cited as `saveload`): the `quit` key and the menu (2), mouse routing, double click and
  push buttons (3.1, 3.2);
- `options-screens` (cited as `options`): actions and default keys (11), scroll views and scroll bars (4.7, 4.8);
- `pda-controls`: buttons and their *clicked* report (4);
- `info-portions` (PD-8): giving and disabling portions (3, 4), the interface portions (7.3);
- `autosave-timing`: the periodic automatic save (2.4), F6 and F7 (5.2);
- `player-movement`: the movement request and state and their direction flags (3), the camera's yaw and pitch
  (conventions, 7);
- `narrowed-specs-scope`: SL-3 leaves the `quit` key in this window to this spec;
- `title-menu`: the menu cursor (7); `dialogue-scope` (cited as `DG`): *player character*, *partner*, the parts
  DG-3, DG-4, DG-7, DG-10.

Conventions:

- The **talk screen** is the in-game window that opens for a talk (`talk-window` 2: the talk screen, holding the
  talk window and the trade window). The **talk window** is `talk-window`'s window of the talk, inside the talk
  screen; `info-portions` 7.3 calls it "the dialogue window's dialog part". "Shown", "open" and "closed" refer to
  the talk screen unless the talk window is named.
- **In-game window**, **topmost window** and **toggling** a window are those of `inv` 1 and `pda` 4.1: toggling
  opens a closed window and closes an open one; a window opened over another leaves that one open beneath it
  [W1, O1b].
- A key's **action** is that of `keys` 5 rule 1. Actions by name and number (`options` 11): `use` 42, `quit` 47,
  `pause` 40, `console` 48, `inventory` 49, `wpn_fire` 32, `wpn_zoom` 33 [K0]; default keys (`config\default_controls.ltx`):
  `use` `kF`, `quit` `kESCAPE`, `pause` `kPAUSE`, `console` `kGRAVE` [D4].
- Angles are in radians; *dt* is the frame time in seconds.
- Each fact is **static** (from static analysis of the shipped `bin\xrGame.dll`, the game library of
  `talk-window`, and of `bin\XR_3DA.exe`) or **measured** (from the game data) unless marked **believed**
  (inferred; the reason is given) or **unestablished**. Nothing was run.

**Checks.** Every claim carries the label of an assertion of `checks/talk-window-input/checks.py` in the dirty
workspace (`run_all.py` runs it; `python3 -I run_all.py`): K (keys), M (mouse), O (opening), C (closing), U (the
screen's update), W (other windows, saving), V (input devices), D (game data). All 55 pass.

## 1. Opening

### 1.1 When

The talk screen opens when a talk starts with the player character: by the use action on a living character
(`use` 6.2) and by `run_talk_dialog` (`R` 4.2). Both go through the same steps [O1]: the partner accepts the
talk and starts talking with the player character; the player character starts talking with the partner; then,
with the single-player interface, the topmost in-game window, if one is open, is toggled (closed), and then the
talk screen is toggled (opened). Of the routines that reach the talk screen through the single-player interface,
this talk start is the only one that toggles it; so no other route opens it in single play [O1, W4] (believed: a
route by other means was not found; `switch_to_talk` does nothing, `R` 4.3).

### 1.2 What opening does

Opening is the opening of an in-game window of `inv` 1.3, with these values for the talk screen [O2, K4, M2]:

1. The HUD indicators are hidden and the further HUD display flag of `inv` 1.3 is cleared; both are restored on
   closing (section 5).
2. The talk screen and its talk window are shown and set up (1.3).
3. The menu cursor (`title-menu` 7) is shown: the talk screen needs the cursor.
4. The talk screen **stops the player character's movement** (unlike the inventory screen and the PDA, `inv`
   1.3, `pda` 4.2): the direction flags (forward, back, left, right; `player-movement` 3) are cleared in both its
   movement request and its movement state [O2, O2c], and the **use flag** of `use` 1.3 is cleared [O2b]. (The
   two words cleared are believed to be the request and the state: their lowest four bits are read elsewhere as
   "any direction" [O2c].) So the use flag that the press of `use` set is not left set by a talk, as `use` 1.3
   allows for windows in general.
5. The actions `wpn_fire` and `wpn_zoom` are released, as if their keys were let go [O2].
6. The game is **not** paused: nothing in the opening pauses it [O2].

### 1.3 Setting up

Showing the talk screen does, in order [O3]:

1. If the player character exists and is talking (its talk flag, `R` 4.1), the talk start of `talk-window`
   (sections 4 to 9) is done with the player character and its talk partner (the character it started talking
   with, 1.1); it ends by marking the question list for a refill (4.3), running one update of the screen
   (section 4), showing the talk window (which gives `ui_talk`, section 6) and hiding the trade window (which
   gives `ui_trade_hide`, section 6) [O3, O4, O6]. If the player character does not exist or is not talking,
   none of this happens.
2. The talk screen is shown and enabled.

So a talk screen opened while the player character is not talking (believed possible only by the case of 7.3)
shows what it showed last, and closes itself at its first update (4.1).

## 2. While the talk screen is open

- **The game goes on.** Game time, the world and the characters continue; the level is drawn behind the screen
  (believed: as for the inventory screen, `inv` 1.3; nothing of the talk screen's opening changes the drawing)
  [O2].
- **The player character does not move or act by itself.** Its movement was stopped on opening (1.2), and no
  key press, release or hold and no mouse event reaches its controls while the screen is open (3.3, 3.5). It can
  still be hurt, and its condition changes as without the screen (believed: nothing of the opening or the
  screen's update touches them).
- **The camera turns toward the partner** on every update of the screen (4.2).
- **HUD**: hidden as in 1.2 item 1. **Cursor**: the menu cursor, moved by the mouse (3.5).
- **Weapon**: `wpn_fire` and `wpn_zoom` were released (1.2); no press reaches the character (3.3), so the weapon
  is not fired or aimed. The item in the hands is not changed (believed: nothing of the opening touches the
  slots).

## 3. Keyboard and mouse

### 3.1 How a key press is handled

A key press reaches the level's key handling as `keys` 5 says. While the talk screen is open [K8, K9, K3]:

1. `pause`, `screenshot`, `quit` and `console` are handled by the level first (`keys` 5 rule 2); `quit` is
   offered to the topmost window (3.2).
2. Every other key is offered to the in-game interface before the level tests the pause, and the interface
   offers it to the topmost window first; while any in-game window is open the interface keeps the press, so
   neither `quick_save` (F6) nor `quick_load` (F7) acts (`autosave-timing` 5.2) and no console-line binding is
   executed (`keys` 5) [K9]. The game type's interface still sees the press when the topmost window did not
   take it (`keys` 5 rule 3), but `inventory`, `active_jobs`, `map` and `contacts` act only when no window is
   open or their own window is the topmost one, so with the talk screen topmost they do nothing [K10].
   **Disputed in review:** the list is incomplete. `scores` acts with any window open and is not taken (`keys`
   5 rule 3), so it acts during a talk too. And before the game type's interface, the in-game interface's own
   handling is offered every press the topmost window did not take: numpad minus and numpad plus (key codes
   74 and 78), with neither Shift key held, hide and show the HUD indicators and are taken. So during a talk
   these keys show or hide the indicators that opening hid (1.2 item 1). Closing, as the topmost window,
   restores the state saved at opening (5 item 4).

The talk screen, offered a key press as the topmost window, handles it in this order [K1, K1b, K3, K6]:

1. **`use`.** If the key's action is `use`: when the partner has the special mode (`talk-window` 9), nothing
   happens; otherwise the talk screen is toggled, that is closed (section 5). Either way the press is taken. This
   test comes before any test of the pause, so `use` closes the screen also while the game is paused [K1].
2. Otherwise, nothing is done when the screen is disabled, or when the game is paused (the talk screen does not
   work during the pause) [K2, K3].
3. A press of the left, right or middle mouse button (key codes 337, 338, 339) is a mouse press at the cursor
   (3.4); if an element takes it, the press is taken [K3].
4. The key is offered to the screen's elements [K3, K6]: when the partner has the special mode, the talk screen
   takes every key here; otherwise the key goes to the enabled elements, of which only buttons take keys, and
   only their accelerator keys; neither the talk screen's buttons nor the trade window's have one (no `accel` in
   `talk.xml`, `talk_16.xml`, `trade.xml`; a button without one keeps key code 0, believed to be no key's, `options` 11.2) [K7, D5]. So no
   element takes a key.
5. The key is **not** passed to the player character's controls, because the talk screen stops movement (1.2):
   unlike the inventory screen (`inv` 1.3) and the PDA (`pda` 6), the character cannot walk, crouch, jump or use
   other actions during a talk [K3, K4].

### 3.2 The `quit` key

The key whose action is `quit` (default Esc) is offered to the talk screen as the topmost window (`saveload` 2;
the rule `narrowed-specs-scope` SL-3 leaves to this spec) [K8]:

- When the screen does not take it, the screen is toggled (closed) and the main menu does **not** open on this
  press. This is the normal case: no element takes the key (3.1 item 4) [K8, K7].
- With a special-mode partner, the screen takes every key while the game is not paused (3.1 item 4), so `quit`
  does nothing at all: the screen stays open and the menu does not open [K6, K8].
- While the game is paused the screen takes no key (3.1 item 2), so `quit` closes it then too, with any partner
  [K3, K8].

The menu therefore cannot be opened while the talk screen is open; it opens on a press of `quit` after the screen
has closed.

### 3.3 Releases and holds

Key releases and holds go to the topmost window as for the inventory screen (the same handling) [K5]; the talk
screen's elements may use them (the scroll bars' repeat while held, `options` 4.8), and they are never passed to
the character's controls, by the screen (which stops movement) or by the level (which passes releases and holds
to the character only when no in-game window is open) [K5, K11]. So a key that was held when the talk started
does not act again until it is pressed again after the screen closed (believed, from 1.2 item 4 and this rule).

### 3.4 Mouse buttons and clicks

Mouse presses and releases go to the enabled elements under the cursor (later-attached first), as `saveload` 3.1
describes for the menu screens, with the same double click (a left press less than 250 ms after the previous one
is delivered as a double click) [M3]; hidden elements are disabled and get none [M3]. Nothing happens while the
game is paused (3.1 item 2) [K5]. What reacts:

- **A question** (an entry of the question list, `talk-window` 7): its caption is a push button (`pda-controls`
  4, `saveload` 3.2), activated by a left release over it after a press. Its *clicked* report (16) makes the
  entry report to the talk window, which keeps the entry's choice (the dialog or phrase id the entry stands for,
  DG-4) and reports **message 46** (`TALK_DIALOG_QUESTION_CLICKED`) to the talk screen [M4, M5, M5b]. The talk
  screen then [M6, M7]:
  - ignores the click when the question list is marked for a refill (4.3), that is when a choice was already
    taken and the list has not yet been refilled;
  - otherwise takes the choice (what it does, which dialog starts or which phrase is said, and the error text
    `ID = [<id>] of selected question is out of range of available dialogs `, is DG-4's) and marks the list for
    a refill.
  So at most one choice is taken per update of the screen, and the list shows the new choices from the next
  update on.
- **The trade button** (`talk-window` 8): a push button; its *clicked* report makes the talk window report
  **message 45** (`TALK_DIALOG_TRADE_BUTTON_CLICKED`) to the talk screen, which switches to trade when both
  speakers have trade enabled (`R` 5.1; what the switch does is DG-10's) [M5, M5b, M6, M8]. With a special-mode
  partner the button is hidden (`talk-window` 9) and so gets no click [M3].
- The pictures, panels, frame lines, titles and answer entries do not react to clicks (they are statics and
  windows without a reaction of their own) [K7].
- **The two lists** (the answer log and the question list) are scroll views: a left-button drag over a list and
  its scroll bar (arrows, box, the repeat while held) act as `options` 4.7 and 4.8 say [M1, K5].
- Mouse buttons 4 to 8 reach the screen as key presses (`keys` 5) and are handled as other keys (3.1).

The talk screen also handles **message 52** (`TRADE_WND_CLOSED`) from the trade window: the talk window is shown
again (giving `ui_talk`, section 6) and the trade window hidden [M6]; when 52 is sent is DG-10's.

### 3.5 Mouse movement and the wheel

- **Movement** moves the cursor (`title-menu` 7) and is delivered to the elements under the cursor (a list
  follows it while the left button is held, `options` 4.7); it never turns the camera or reaches the character's
  controls while the screen is open [M1, K5].
- **The wheel** goes to the element under the cursor: over either list it scrolls that list by one step per
  notch (`options` 4.7); elsewhere nothing [M1].

### 3.6 Pause and console

- **Pause.** The `pause` key pauses the game as without the screen (`keys` 5 rule 2) [K8]. While paused the
  screen stays drawn and takes no keyboard or mouse input, except that `use` still closes it (3.1) and `quit`
  closes it (3.2) [K1, K3].
- **Console.** The `console` key opens the console as without the screen (`keys` 5 rule 2) [K8]; the talk screen
  stays open behind it (believed: nothing in the console's opening touches in-game windows, as `pda` 6).

### 3.7 Summary

| Input | Effect while the talk screen is open |
|---|---|
| key bound to `use` | closes the screen (5); with a special-mode partner nothing; also while paused |
| key bound to `quit` | closes the screen, no menu; with a special-mode partner nothing (unless paused) |
| key bound to `pause` | pauses the game; the screen then ignores input except `use` and `quit` |
| key bound to `console` | opens the console |
| `inventory`, `active_jobs`, `map`, `contacts` | nothing |
| `quick_save`, `quick_load`, console-line bindings | nothing |
| any other key | nothing (not passed to the character) **Disputed in review:** `scores` acts; numpad minus and plus (no Shift) hide and show the HUD indicators (3.1 item 2) |
| left click on a question | message 46: the choice is taken unless one is pending (3.4) |
| left click on the trade button | message 45: trade when both speakers have it enabled (DG-10) |
| wheel, drag, scroll bar over a list | scrolls that list |
| mouse movement | moves the cursor |

## 4. The talk screen's update

Every in-game window that is shown and enabled is updated once per frame, not only the topmost one [U7]; so the
talk screen is updated also while another window (the level-change window, a PDA opened by a tutorial, section 7)
is open over it. Each update of the talk screen does, in order [U1]:

### 4.1 Closing itself

The screen closes itself (it is toggled, section 5) when [U1]:

- the player character exists and is not talking (for example after `stop_talk` on it, `R` 4.1, or `break_dialog`,
  DG-3); or
- either speaker no longer exists (the talk's player character or partner); or
- the two speakers are more than **3 m** apart (the distance between their positions; believed: their world
  positions) and the partner does not have the special mode. A special-mode partner is never left by distance.

When there is no player-character object at all, only the last two tests apply [U1].

### 4.2 Refill, then the camera

- If the question list is marked for a refill, it is refilled (`talk-window` 7; the choices are DG-4's) and the
  mark is cleared [U1, U2]; what the refill does besides listing the choices is DG-4's.
- The screen's elements are updated (`talk-window` 5, the panels' refresh, among them).
- **The camera turn** [U4, U5, U6]: the player character's active camera is turned toward the partner.
  - The **target point** is the centre of the partner's bounds raised by half their radius (believed: the bounding
    sphere of the partner's model) [U4]. With *d* = target point − the camera's position, *h* = √(*d*ₓ² + *d*_z²):
    the **target yaw** is the direction of (*d*ₓ, *d*_z) in `player-movement`'s convention (forward is
    (sin yaw, 0, cos yaw)), that is atan2(*d*ₓ, *d*_z) up to a whole turn (π/2 or −π/2 by the sign of *d*ₓ when
    |*d*_z| is below 10⁻⁷); the **target pitch** is −atan(*d*ᵧ / *h*) (positive pitch looks down). When |*d*ₓ| and
    |*d*_z| are both below 10⁻⁷ the target yaw is 0 and the target pitch is −π/2 when *d*ᵧ > 0, π/2 when
    *d*ᵧ ≤ 0, and 0 when |*d*ᵧ| is also below 10⁻⁷ [U4].
  - The yaw, and then the pitch, is turned only when it differs from its target by more than **0.2** (both
    brought into −π … π first) [U5]. Then, with δ the difference target − current (in −π … π):
    1. the turning speed is *v* = 0.15 + ((0.2 − 0.15) / (π/6)) · |δ| per second;
    2. the angle moves toward the target the shorter way round by min(|δ|, *v* · *dt*), and the result is
       brought into 0 … 2π;
    3. if it is still more than **π/6** from its target, it is set to the target ± π/6, on its own side [U6].

    **Disputed in review:** the result of step 2 is not what is kept. After the step, the angle is brought
    back into −π … π (when it lies outside it). Step 3 tests that angle. The angle stored is then either that
    angle or the target (in −π … π) ± π/6. So a turned yaw or pitch is never above π, except that target
    ± π/6 can go past ±π by up to π/6.
  - So a partner far to the side is faced within π/6 at once, and the camera then drifts slowly (0.15 to 0.2 per
    second) until it is within 0.2 of the target, where it stops turning. The mouse cannot turn the camera while
    the screen is open (3.5). How the camera treats a pitch brought into 0 … 2π (above π) is **unestablished**
    here (`player-movement` 7's limits are applied by the camera itself). **Disputed in review:** no pitch
    above π is stored (see the note after step 3); the question does not arise.

### 4.3 The refill mark

The question list is marked for a refill [O3, M7, U3]:

- at each talk start (1.3);
- when a choice is taken (3.4);
- while the talk screen is shown, when an information portion is given to the player character (only when it
  did not hold it, `info-portions` 3.4 item 4) or disabled on it (`info-portions` 4.2) [U3];
- while the talk screen is shown, when a phrase is said in a dialog and the dialog's notice of the phrase goes to the player
  character (believed: the notice to the speaker who answers next; DG-4) [U3].

## 5. Closing

The talk screen closes on any of:

1. a press of the key bound to `use` (3.1), unless the partner has the special mode;
2. a press of the key bound to `quit` (3.2), unless the partner has the special mode and the game is not paused;
3. its own update (4.1): the player character no longer talking, a speaker gone, or more than 3 m between them;
4. the player character's death: the periodic update of the in-game interface closes the topmost window when it
   is the inventory screen, the PDA, the talk screen or the body-search window (`inv` 1.2 item 4, `pda` 5) [C2];
5. a talk starting while it is open (7.3).

Closing does, in order [C1, C1b, C1c, C1d, M2, O2]:

1. If the partner is a trader that is playing a trader sound (`R` 6), that sound is stopped (the same stop
   `set_trader_sound` does first) [C1b]. Otherwise a sound held by the screen itself is stopped; nothing but the
   screen's creation sets that one, so it is believed never to play [C1c].
2. The talk screen is disabled and hidden; the trade window is hidden (giving `ui_trade_hide`, section 6) [C1,
   O6].
3. The current dialog is forgotten (DG-4) and, if the player character is talking, its talk is stopped as
   `stop_talk` on it does (`R` 4.1: its own side only; the partner's side is DG-3's) [C1, C1d].
4. As for any in-game window, if the talk screen was the topmost window, the HUD indicators and flag return to
   what they were at opening; the cursor is hidden unless the window now topmost needs it [M2].

The talk window is **not** hidden on closing, so closing gives no `ui_talk_hide` (section 6) [C1, O5].

## 6. Information portions

`ui_talk` and `ui_talk_hide` are declared in `config\gameplay\info_portions.xml`, each disabling the other [D1];
no shipped script names either [D2]. They are given as `info-portions` 7.3 says (by a transfer to the player
character, in single player, while the controlled object is the player character) [O4]:

- `ui_talk` when the talk window is shown: at each talk start (1.3) and on returning from trade (message 52,
  3.4) [O4, O5];
- `ui_talk_hide` when the talk window is hidden: only when the talk switches to trade (the trade button, 3.4, or
  `switch_to_trade`, `R` 5.1) [O4, O5, M8].

Consequences (from these rules and `info-portions` 3.2, 4.2): a talk that never switches to trade gives `ui_talk`
and never `ui_talk_hide`; after the first such talk the player character holds `ui_talk` until a switch to trade,
and the later talk starts give nothing new (receiving a held portion does nothing). So holding `ui_talk` does not
mean the talk screen is open.

The talk screen's opening and closing also hide the trade window, which gives `ui_trade_hide` each time (at every
talk start and every closing) [O6]; what `ui_trade` and `ui_trade_hide` are for is the trade screen's (#284).

## 7. Other in-game windows

### 7.1 While the talk screen is open

- **Inventory**: does not open (its action needs no window open or the inventory topmost, `inv` 1.1) [K10].
- **PDA**: its three actions do nothing (`pda` 4.1) [K10]. A tutorial item with a PDA tab opens the PDA without
  testing other windows (`pda` 7), so it opens over the talk screen, which stays open beneath and keeps updating
  (section 4) [W1, U7]; tutorial items without a `pda_section` hide their own window while the talk screen is open
  (`pda` 7).
- **Body search and boxes**: open only when no in-game window is open (`pda` 6), and the use action that would
  open them does not run (the `use` press closes the talk screen, 3.1).
- **Level change**: its window opens over any window (`pda` 6); the talk screen stays beneath, gets no input and
  keeps updating, so its closing rules of 4.1 still apply [W1, U7].

### 7.2 A talk starting while another window is open

A talk can start while the inventory screen or the PDA is open: both pass key presses to the character's controls
(`inv` 1.3, `pda` 6), so the use action runs, and scripts may call `run_talk_dialog` at any time. The topmost
window is then closed (toggled, with all its own closing effects: for the PDA `ui_pda_hide`, `pda` 5; for the
inventory screen its sound and `ui_inventory_hide`, `inv` 1.3) and the talk screen opens (1.1) [O1]. A window open
beneath the topmost one stays open beneath the talk screen [W1]. This answers `pda-window`'s second review gap:
the PDA, when it is the topmost window, is closed, not left beneath and not refused.

### 7.3 A talk starting while the talk screen is open

The use action cannot start one (`use` 6.2 step 1: nothing while the player character is talking; and its key
closes the screen, 3.1). `run_talk_dialog` does not test whether the player character is talking (`R` 4.2); on a
partner that accepts, the steps of 1.1 then give, in order (believed: the consequence of 1.1, 1.3, 4.1 and 5; not
traced as one run) [W3, O1, C1, O3, U1]:

1. the new partner starts talking with the player character, and the player character with the new partner;
2. the open talk screen, the topmost window, is closed, which stops the player character's talk (5 item 3);
3. the talk screen is opened again, but the player character is no longer talking, so no talk start is done (1.3);
4. at its first update it closes itself (4.1).

So the talk screen ends up closed, the player character not talking, and the new partner talking (its side is
DG-3's).

## 8. Saving and loading during a talk

- The **periodic automatic save** does not consult open in-game windows (`autosave-timing` 2.4), so it is written
  during a talk like at any other time.
- **F6 and F7** do nothing while the talk screen is open (`autosave-timing` 5.2 item 3; 3.1) [K9].
- The **menu** cannot be opened while the talk screen is open (3.2), so its save and load screens are reached only
  after the talk screen closed.
- The console command **`save`** tests only that the game is single-player and that the player character exists
  with health above 0, not open windows or talks; so a save can be written from the console during a talk [W2].
- What a save holds of a talk (the characters' talk state, the talk contacts of `sav` chunk 9 part 3, `DG` 2.3)
  is DG-3's.
  The talk screen's own state (shown or not, its lists, the refill mark) is believed not saved: the screen is
  built with the level's in-game interface (`talk-window` 1) and nothing of it is written (believed; the save's
  contents are `sav`'s). After loading, the talk screen is therefore believed closed; whether a talk saved as
  running is resumed is DG-3's (**unestablished** here).

## 9. The original's set of choices and events

The original reads only the keyboard and the mouse: `XR_3DA.exe` creates DirectInput 8 devices and holds the
public identifiers of the system keyboard and the system mouse, and no shipped binary holds the joystick device
identifier, names XInput or a gamepad, or names the Windows joystick functions [V1] (static; so no gamepad
support, as `inv` 6.1). How the touch and gamepad paths reach the same choices is the clean side's design; what
they must reach is this set:

1. **Choose one of the listed questions** (one choice per update; the list is refilled after it) (3.4, 4.3).
2. **Switch to trade** (the trade button; usable only when the button is shown, that is the partner has no special
   mode, and acting only when both have trade enabled) (3.4).
3. **Scroll the answer log** and **scroll the question list** (3.4, 3.5).
4. **Leave the talk** (`use` or `quit`; neither with a special-mode partner while not paused) (3.1, 3.2).
5. **Pause** and **open the console**, as without the screen (3.6).

Nothing else of the screen takes input: no keyboard navigation of the questions, no key for the trade button, no
way to move the character or the camera during a talk.

## 10. Messages 49 and 50

The listing's `PDA_DIALOG_WND_BACK_BUTTON_CLICKED` (49) and `PDA_DIALOG_WND_MESSAGE_BUTTON_CLICKED` (50) [D3] are not
sent by the talk screen or its talk window [M9]; `pda` 9 found no sender anywhere (believed never sent). The
messages the talk screen uses are 45, 46 and 52 (3.4) and, inside the talk window, the entry report 32, which the
listing's `ui_events` does not have [D3, M4]. No shipped script names 45, 46, 49 or 50 [D2]: scripts do not take part
in the screen's input.

## 11. Unestablished and believed

- That nothing but the talk start of 1.1 opens the talk screen in single play (believed, from a search).
- That the two words cleared on opening are the movement request and state (believed, 1.2).
- That the level is drawn behind the screen, that the player character can be hurt and the item in its hands is
  unchanged during a talk (believed, section 2).
- That a key held when the talk started does not act after it until pressed again (believed, 3.3).
- That the console leaves the screen open behind it (believed, 3.6).
- That the speakers' positions of 4.1 are their world positions, and the target point's bounds are the partner's
  bounding sphere (believed, 4.1, 4.2).
- How the camera treats a pitch the turn brought above π (4.2). **Disputed in review:** moot, the turn
  stores no angle above π (4.2).
- Which speaker gets the dialog's phrase notice that marks a refill (believed: the one answering next, 4.3).
- That the screen's own sound is never set (believed, section 5).
- The outcome of 7.3 as one run (believed, from the rules).
- That nothing of the talk screen is saved, and that after loading it is closed (believed, section 8); whether a
  saved running talk resumes (DG-3).
- Whether the screen's update (and so its camera turn and closing rules) runs while the game is paused.

## 12. Coverage

The names `dialogue-scope` section 7 gives DG-6: `api:TALK_DIALOG_QUESTION_CLICKED` (3.4), `ui_talk`, `ui_talk_hide`
(section 6), `api:PDA_DIALOG_WND_BACK_BUTTON_CLICKED`, `api:PDA_DIALOG_WND_MESSAGE_BUTTON_CLICKED` (section 10).

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll` (6,246,400 bytes) and `bin\XR_3DA.exe`, and the other shipped binaries
  of `bin\` for the input-device search.
- Data examined: the winning copies of `config\` and `scripts\` as extracted for the approved `pda-scope`:
  `config\gameplay\info_portions.xml`, `config\default_controls.ltx`, `config\ui\talk.xml`, `talk_16.xml`,
  `trade.xml`, the shipped scripts and the API listing `scripts\lua_help.script`.
- Analysis: static analysis of `bin\xrGame.dll` with Ghidra 12.2 (headless; the decompilation and instruction
  listings of the approved `pda-controls` and `talk-window` work, and class tables found by run-time type
  information): the talk screen's key, mouse, message, show, hide and update handling, the talk window's show
  and hide, the question entries' and trade button's reports, the in-game windows' shared input handlers and their
  opening and closing, the level's key press, release and hold handling, the talk start of the player character,
  the camera turn and its angle routines, the refill triggers, the death rule, the `save` console command;
  constants read from the binary; byte searches of the shipped binaries for input-device identifiers and names.
  No debugging or execution; the game was not run.
- Checks: `checks/talk-window-input/checks.py` (with `twilib.py`, the `talk-window`, `use-action`, `pda-scope` and
  `script-api-dialogs-and-trade` helpers and evidence), run with `python3 -I run_all.py`; all 55 pass.
- No leaked source code, derived projects or third-party documentation of the engine were used. The DirectInput
  device identifiers are those of Microsoft's public DirectInput documentation.

## Gaps found in review

- Section 9's set leaves out the inputs that act during a talk without being the talk screen's: `scores`, and
  numpad minus and plus (no Shift) hiding and showing the HUD indicators (3.1 item 2).
