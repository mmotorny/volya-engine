# Roadmap

Priorities, in order. Work items are tracked in GitHub issues and milestones.

## Game

1. S.T.A.L.K.E.R.: Shadow of Chernobyl, original release (GOG offline installer).
2. Shadow of Chernobyl, Enhanced Edition.
3. Clear Sky, Call of Pripyat.

## Large systems

Work that needs a large system not yet built (physics, character motions and AI, dialogs and trade) waits until that system is implemented, rather than being stubbed piecemeal; its scope stays recorded in the specs and its issues are blocked by the system's.

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

The browser renderer targets WebGL 2 only (decided by the user): WebGPU is reconsidered if the target platforms come to favor it.

The first, static-lighting renderer is the one built. The dynamic renderers (`renderer_r2a`, `renderer_r2`; scoped in docs/behavior/r2-renderers-scope.md) wait until the game is more or less complete: a playable game comes before more graphics modes. Until then their entries in the options are disabled.
