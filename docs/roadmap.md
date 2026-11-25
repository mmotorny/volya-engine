# Roadmap

Priorities, in order. Work items are tracked in GitHub issues and milestones.

## Game

1. S.T.A.L.K.E.R.: Shadow of Chernobyl, original release (GOG offline installer).
2. Shadow of Chernobyl, Enhanced Edition.
3. Clear Sky, Call of Pripyat.

## Next, in order

1. Sound and effects of the first level: the approved specs not yet built (detector, body sounds, artefacts, particle collision, climbing), then the remaining scoped ones.
2. The first renderer's remaining picture: display gamma (#377), frame presentation (#378), grass (#379); then its other settings (#380–#384).
3. Real-device performance on the iPhone (#355), driven by the user's diagnostic logs.
4. The script engine API (docs/behavior/script-api-scope.md), towards running the game's scripts: doors, characters, quests.
5. The HUD (docs/behavior/hud-scope.md).

## Game language

1. Russian (GOG Russian installer).
2. Ukrainian, English.

## Platforms

1. Web, including the iPhone Home Screen web app (PWA) as the primary iOS delivery.
2. Native iOS, then iPadOS.
3. VR.

## Web browsers

- Desktop Chrome.
- iOS: Chrome and Safari (both WebKit).
- Later: iPad.
- Out of scope: Firefox, Android.

Reference mobile device for real-device testing and performance and memory budgets: iPhone 17 Pro Max.

## Saved games

Volya Engine reads and writes the original game's saves. Saving and loading are built once every system whose state a save holds exists.

## Renderers

The first, static-lighting renderer is the one built. The dynamic renderers (`renderer_r2a`, `renderer_r2`; scoped in docs/behavior/r2-renderers-scope.md) wait until the game is more or less complete: a playable game comes before more graphics modes. Until then their entries in the options are disabled (#364); tracked in #385.
