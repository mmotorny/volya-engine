# Audio

## Split across workers

- The engine worker runs the positional sound system once per worker frame and drives the mixer through `volya-engine-core::audio::remote::Remote`, which only records commands (init, play with file number, skip, loop, gain, pan and category, stop, stop all, set voice gain, pan and pitch, category volume, master gain, watch).
- The audio worker (`static/audio-worker.js`) is the engine's wasm build in its own instance (`platforms/web/src/audio_host.rs`): it applies the commands to `volya-engine-core::audio::Mixer` through `remote::Host`, decodes Vorbis and mixes on the output's schedule, so a long engine frame never starves the output.
- The page's AudioWorklet (`static/audio-worklet.js`, processor `volya-output`) plays what the audio worker mixed.
- Audio is an output channel, not shared state: the engine learns only which voices ended, from the host's reports.
- `main.js` connects audio (worklet, ring, audio worker over a `MessageChannel`) only after the engine worker reports `started` (workers start one at a time, [web-platform.md](web-platform.md#isolation)) and only on a cross-origin isolated page. Without isolation there is no audio path (no fallback, decided by the user), and the interface says so before anything else.
- The page suspends the AudioContext while hidden and resumes it on return and on any gesture while its state isn't `running`, which covers WebKit's non-standard `interrupted` (documented: MDN; WebKit bugs 263627 and 281955; not measured on a device).

## Transport

`SharedArrayBuffer` for audio output only, no wasm threads (decided by the user, #354): replies by message put latency and the garbage collector on the output's path, and long frames starved it.

- One `SharedArrayBuffer` per output. Its layout is defined twice and must match: `platforms/web/src/audio_ring.rs` and `static/audio-ring.js`. Control words, a 256 KiB ring of commands (engine worker → audio worker) and a 4,096-frame ring of stereo `f32` frames (audio worker → worklet).
- Each ring has one writer and one reader; positions only grow and are published with `Atomics.store` after the data.
- After every frame, op call and visibility change the engine worker writes the mixer's commands to the command ring (no message, no JavaScript garbage) and wakes the audio worker.
- The worklet reads its ring on every render quantum without allocating, counts frames it can't fill as underruns, and when the fill drops under the target bumps and notifies the wake word.
- The audio worker sleeps on the wake word with `Atomics.waitAsync` (Chrome 87, Safari 16.4; documented: MDN), for at most 10 ms (the wait's timeout and a timer, whichever ends first): on an iPhone the worker slept up to 15 s with the output starving while the page and the engine ran, until a file's message reached it (measured, diagnostic logs; why the wakes were lost is not established, #452). Woken, it applies the pending commands and mixes 256-frame blocks until the ring holds the target, 30 ms (`ringTarget`): the audio worker's share of the output latency, covering its wake-up.
- A gain or pan change ramps over one block. Commands read together apply together before the next block, so the hosted mixer's output equals a directly driven mixer's for the same changes between the same blocks (unit-tested, `remote::tests`).
- Compressed files go apart from the commands, one per voice started, numbered, over the port. Commands read together wait until every file they play has arrived. Neither wasm heap holds a whole file, since a wasm heap never shrinks: the engine worker reads each from the game tree into a JavaScript array 256 KiB at a time (`WebMixer`, #491; 64 KiB pieces doubled the worst sound update of starting 24 voices in Chrome, 4.1 to 7.4 ms, 256 KiB took 4.5, one run each), and the audio worker keeps it there and reads it a piece at a time (`JsFile`).
- The host reports ended voices, its heap, its memory manager's bytes in use (the sound memory, reported beside *h*, [memory.md](memory.md#deviations)), mixing time and underruns to the engine worker whenever something changed (the bytes in use with the other news, at least once a second: reporting each change of them flooded the engine worker, whose figures then lagged); diagnostics series once per second of output ([diagnostics.md](diagnostics.md)).

## No sound cache

docs/behavior/console-diagnostics.md 2.1.1; a deviation ([memory.md](memory.md#deviations), #812).

- The original's sound library holds the decoded audio of the sounds played in a cache sized from `snd_cache_size`. Ours keeps none: each voice decodes its file as it plays, and `snd_cache_size` is a console variable that sizes nothing. The cache saves only decoding time, which costs no underrun and no frame time here (#812).
- Measured (the `audio-load` measurement, `xtask measure audio-load`, 12 minutes of a level's sounds with a music track, a busy moment of 24 sounds every minute and footsteps; mixing per second of audio): headless Chrome 43.0 ms on average, 73 ms the worst second, without the cache; 50.1 and 112 with it (its lines coded, #796). iOS Simulator (an `OfflineAudioContext` paced in 20 ms steps): 11.3–11.9 ms average, 34–42 worst, without; 10.3–10.5 and 48–50 with. No underrun either way, the engine's frame CPU the same (1.2 ms Chrome, 1.5–1.6 ms the Simulator), the audio worker's wasm memory 3.7–4.1 MiB without against 11–20 MiB with. One run each in Chrome, two in the Simulator, other agents' suites sharing the machine.

## Decoder

- `lewton` (MIT or Apache-2.0, pure Rust, runs in wasm) decodes the packets that `VorbisStream` (`volya-engine-formats::audio`) reads with `ogg` (BSD-3-Clause); whether to keep it is #312. Symphonia is MPL-2.0 and so excluded; `vorbis_rs` wraps the C libvorbis.
- `lewton` decodes a single-page stream's whole last block, so `VorbisStream` ends its output at the last page's granule, the stream's end by the Vorbis I specification.
- A voice resumed far into its sound (sound-positional.md section 4.2) seeks to the last page whose first packet ends before the offset, found from the pages' headers and each packet's mode and window flags, and decodes from there. Decoding up to the offset is believed to have caused the single underruns of 260 to 380 ms in an iPhone's levels (#447): a 40 s offset decodes in about 30 ms natively. Granules aren't used for it: some of the game's files are off by 448 frames on some pages (measured).
- Parsing a stream's setup header is most of what opening it costs (0.7 ms a file in Chrome's wasm, 0.5 natively). A loaded sound's file was checked on load, so the engine worker sends it without parsing its headers (`MixerControl::play_checked_file`), and streams open on a thread with the same headers' bytes share them, parsed once (`VorbisStream`, held only while a stream uses them). Starting 24 voices of 8 files in one update took the engine 13 ms and the mixing side 12 natively, 22 ms of a Chrome frame's sound update; now 1.7, 2.7 and 6.5, most of what is left copying the 29 MiB of files (measured, `tests/sound.rs`, the audio suite, #310).
- The browser's `decodeAudioData` is not an option: Web Audio's contexts are `[Exposed=Window]`, not available to the engine worker, and WebKit reads Ogg Vorbis only from iOS 18.4 (documented: the Web Audio specification, WebKit's Safari 18.4 notes).

## Sounds by name

`volya-engine-core::sound`, docs/formats/sound-ogg-params.md.

- A name's first request resolves its file (`$level$` first, then `$game_sounds$`), checks it and keeps only its parameters, reading only its headers and its last page (its length), as the original keeps no file data (console-diagnostics.md 2.1.1; reading whole files peaked 4.9 MB above the level's sounds, now 0.9, measured natively for `l01_escape`, #749). Not mono, not 44,100 Hz, missing, empty or invalid, or a max distance or max AI distance below 0.1: `Fatal`.
- Each play sends the compressed file again, which its voice decodes as it plays ([No sound cache](#no-sound-cache)); the sound keeps no decoded audio.
- `sound_params` (`volya-engine-formats`) reads the record; one shorter than its version's size is treated as an unknown version (defaults and a warning; our choice).

## Positional playback

`volya-engine-core::sound::system`, docs/behavior/sound-positional.md.

- The spec's sounds, emitters, per-frame update, voices and device parameters, against a `Device` trait shaped like its OpenAL interface. Its emitters' room is made with its first sound, so without audio output (no sound is created) it holds nothing beyond its fields (no device, no sound memory: console-diagnostics.md 2.1.1).
- `volya-engine::audio::SoundHost` owns it with its clock: real time since the first update, without the page's hidden time (#93). `MixerDevice` plays each voice as a mixer voice in `Category::Scaled` (its gain already holds the category volume): gain is the source gain times OpenAL's inverse distance clamped factor; pan is the direction's component along the listener's right vector (how a direction is rendered is unestablished; our choice). The listener's gain is the mixer's master gain.
- On the web the host lives in the engine worker (`platforms/web/src/audio.rs`) and updates every worker frame, game running or not. The app sets the voice count once from `snd_targets` (4 to 32, default 16), makes the level's camera the listener, pauses while the menu is open over a level, and stops every sound when the level is left.
- Test builds have the `debug_play_sound` and `sound_state` ops.

## Level sounds

`volya-engine::level_sound` over `volya-engine-core::sound::{static_sources, music, ambient, rain_thunder, occlusion}`; docs/behavior/sound-static-sources.md, sound-level-music.md, sound-weather-ambient.md, sound-rain-thunder.md, sound-occlusion.md. It also holds the zones' idle sounds, the torches' night-vision sounds and the material pairs' sounds.

- Their data (`level.snd_static`, `level.som`, `game.ltx`'s track list, the weather's ambient sets and thunderbolt collections) is read under `st_client_spawning` with the weather. Their files (the pairs' and the zones' too) are checked in that stage by creating them in a scratch sound system, so a file that can't be opened fails the load with or without audio output (S1 section 5).
- Their sounds are created on the first frame the level runs with audio output and discarded when the level is left. Without audio output the rules don't run (#352).
- The rules run in the specs' order (static sources, music, thunder, rain, ambience) every level frame while the pause is off; the static sources and the music also not under the loading image (S3 4.1, S5 4.1).
- A sound file that can't be opened and a draw from a range with equal bounds (S3 4.2, S4 4.7) are fatal ([screens.md](screens.md#fatal-errors)); none of the level's sounds is kept.
- Their clock is S4 3.1's global clock (`volya-engine-core::pause::GlobalClock`, kept by the app): the engine clock's real time, loading included, standing still while the pause is on; rate 1, no fixed-step mode (the original's initial values). S8's frame time is its smoothed step, 0 while paused. S2's sound clock stays its own.
- The pause (`volya-engine-core::pause`, S4 3.1's table) is on while the main menu shows (the title menu, the menu over a level; it closes as a New Game starts, before loading), while an interface sequence's item with `pause_state` `on` plays, and while the page is hidden, our platform's focus loss (after the menu opens on hiding, so the return leaves the pause to the menu). The table's other events: #334, #335.
- The ambience's and the thunder's state is kept for the run (`SoundRun`). Random draws come from one generator, the specs' linear congruential one seeded with 1 (`sound/random.rs`).
- Occlusion is computed along a jittered segment against the level's collision world (shared as an `Arc`) and `level.som`, without the original's remembered-triangle shortcut, which the spec lets an implementation omit.
- The player's sky visibility (outdoors for the ambience, the rain's sky factor; `volya-engine-core::object_light::SkyVisibility`, S4 4.3): five rays a frame from the player model's header sphere at its feet and heading, cast as the previous frame's drawing gathered objects, and once when the level is made, so the first read already sees the bunker's 0; the rules' read smooths it once per frame.
- The ambient effects' particle objects are the level's ([simulation.md](simulation.md#particles)), made, stopped and removed by S4's rule through `AmbientParticles`, with the wind it moves.

## Footsteps and landings

`volya-engine::footsteps` over `volya-engine-core::{footsteps, cycles}` and `volya-engine-formats::{gamemtl, skeletal_motions}`; docs/formats/material-pairs.md, docs/behavior/player-cycle-lengths.md, player-footsteps.md.

- The material pairs are read with the materials under `st_loading_cform` (`gamemtl.xr` is read there, not at game start; our choice); their lists' sounds are created and discarded with the level's sounds. A list of more than 4 items, or a pair naming no material, fails the load.
- The player's cycles are read under `st_client_spawning` from its model's motion files as streams: chunk 15 and each motion's name and key count, no tracks. Only each cycle's length, speed, duration and stop flag are kept, since no pose is drawn. Compressed chunks are decoded (chunks.md section 5).
- On each frame that advances game time with audio output and the pause off, after the movement update, the leg cycle is chosen from the movement state and the steps timed on the global clock (S11 3.1); the app plays them independently at the feet.
