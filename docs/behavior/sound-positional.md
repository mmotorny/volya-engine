# Positional sound playback

Specification S2 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`): how a
sound is heard. It covers the operations by which the game plays a sound, the life of a playing sound
(delay, start, end, looping, stopping, pausing), its volume (volume category, smoothing, fades), the
limited number of sounds rendered at once, and what is sent to the audio device: distance attenuation
from the file's parameters, the listener (the camera), and listener-relative ("2D") sounds. Every other
specification of the split plays its sounds through these rules; which of its sounds are 2D, looped or in
which category is for each of them to establish. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated here):

- S1, `docs/formats/sound-ogg-params.md` (sound parameters of `.ogg` files): how a sound name finds its file, the
  required file format (mono, 44,100 Hz), and the file's **min distance** *m*, **max distance** *M* and
  **base volume** *b*, including the defaults of a file without parameters; the reports to the AI and
  their range. Its statement on how volume falls with distance (its section 7.1) is made exact by
  section 7.4 here.
- `docs/behavior/title-menu.md` section 6 (Ogg Vorbis sound files; the menu music, a 2D music-category
  pair) and section 2 (the menu pauses game sounds).
- `docs/behavior/options-screens.md` (the settings `snd_volume_eff`, `snd_volume_music`, `snd_targets`;
  the sound restart).
- `docs/behavior/view-projection.md` section 1.1 (the camera's eye position *e*, direction *d*, up vector
  *u*, and the derived *u'* and right vector *r*), `docs/behavior/world-coordinates.md` section 1 (the
  left-handed world) and `docs/behavior/player-movement.md` sections 8, 9 (the camera during play).
- `docs/behavior/loading-screen.md` section 6 (sound is muted during the loading overlay frames).
- `docs/behavior/startup.md` (the user settings file `user.ltx`) and `docs/formats/ltx-config.md`
  section 10 (path aliases).
- The audio device interface is OpenAL 1.1, a public specification (Creative Labs, "OpenAL 1.1
  Specification and Reference", 2005); where this text names an OpenAL behaviour, it is that document's.

Conventions: "now" is the sound clock in milliseconds (section 5.1); *dt* is the time step of an update in
seconds. clamp(*x*, *a*, *b*) limits *x* to *a* … *b*. Distances are in metres in the world coordinates of
the level files.

**Checks.** Every factual claim carries a tag. The checks are scripts in the dirty workspace,
`checks/sound-positional/`: `export.py` regenerates the evidence in `ev/` (headless Ghidra exports of the
shipped program files); `run_all.py` runs `check_snd.py` (tags B1 – B20: the sound library),
`check_game.py` (G1 – G8: the engine and the game library, the default settings file), `check_data.py`
(D1 – D4: the archives) and `model.py` (M1 – M6: an implementation of sections 5 – 7 written from this
text, reproducing the worked examples of section 10).

## 1. Overview

- The game creates a **sound** from a sound name and a volume category (section 3.1). A sound is a handle:
  playing it creates an **emitter**, an instance of the file being played at a position with its own
  timeline and volume (section 3.2). A sound has at most one emitter attached; independent plays create
  emitters that are attached to no sound (section 3.2).
- Once per frame the sound system updates every emitter (section 5), decides which of the audible ones
  are rendered by the device's limited **voices** (section 6), and sends the rendered ones and the
  listener to the audio device (section 7).
- An emitter is either **3D**, at a world position, attenuated with its distance to the listener (the
  camera), or **2D**, at a position relative to the listener (section 7.3).

## 2. Sound system start-up and files

- The sound system uses the OpenAL device unless the command line contains `-dsound`, which selects a
  different device path (not covered here: **unestablished**). With `-nosound` on the command line the
  sound system is disabled: creating and playing sounds does nothing [B1].
- Files: S1. The decoded data of a file is its sequence of *N* 16-bit samples; its **duration** is
  *D* = floor(*N* · 1000 / 44,100) ms (integer arithmetic on the byte length 2*N* and 88,200 bytes per
  second) [B3]. *N* is the granule position of the file's last Ogg page (Vorbis I specification) (believed:
  the program asks its Vorbis decoder for the total sample count; the two agree for streams starting at
  granule 0).
- The **output rate** is 44,100 Hz, 16-bit, while the sound-quality variable has its initial value 1
  [B3, B18]. With the value 0 the output rate is 22,050 Hz and the data is decoded at half length; how
  the samples are reduced is **unestablished** (no menu control sets this variable:
  `options-screens.md`, and the default `user.ltx` has no line for it [G8]).
- The number of **voices** is the setting `snd_targets` (console range 4 … 32; the engine's initial
  value 16; the default `user.ltx` sets 24), taken when the sound system starts (also at a sound restart);
  if the device refuses to create that many, the voices created so far are used [B13, B18, G7, G8].

## 3. Sounds and emitters

### 3.1 Creating a sound

A sound is created from a name (resolved and loaded per S1) and a **volume category**: **effects** or
**music** [B4]. Which category a sound has is for the specification of each sound (the menu music is
music: `title-menu.md` section 6). A new sound
has no emitter. Discarding a sound stops its emitter at once (section 4.4) [B11].

### 3.2 Playing

Each play request carries flags **looped** and **2D** and a **delay** *t*_d in seconds [B5]:

| Request | Effect |
|---|---|
| **play** | If the sound has no emitter, a new emitter is created and attached to it. If it has one, that emitter is **restarted** (section 4.3); its looped flag, delay, position and settings are kept. Then, if 2D is set, the emitter is switched to 2D [B5]. |
| **play at position** *p* | As play; then the emitter's position is set to *p* [B5]. |
| **play independently** | Always a new emitter, attached to no sound (it cannot be addressed afterwards and runs until it ends); optionally its position, frequency, range and volume are set at once; then the 2D switch as above [B5]. |

A **new emitter** has: position (0, 0, 0); play volume *v* = 1; frequency *f* = 1; range *m*, *M* and base
volume *b* from the file; 3D; priority factor *k* = 1; not stopping; not paused; fade *φ* = 1, occlusion
*o* = 1 and output volume *V* = 1 until it starts [B6]. If |*t*_d| < 0.001 it is **starting**, otherwise
it is **waiting** for *t*_d seconds [B6].

### 3.3 Controls of an emitter

A sound with an emitter (or the caller of an independent play, at once) can change [B7, B8]:

- **position** *p* (world position; listener-relative for 2D, section 7.3);
- **frequency** *f* (playback-rate factor, section 7.1);
- **range**: min distance *m* and max distance *M*, replacing the file's;
- **play volume** *v*;
- **priority factor** *k* (section 6);
- **switch to 2D**: the emitter becomes 2D and *k* becomes 100; **switch to 3D**: the emitter becomes 3D
  (*k* is kept) [B7];
- **stop** at once or **stop with fade** (section 4.4).

A sound can also report its **play time**: now − (start time) while its emitter has started (playing,
silent or ended-but-not-yet-removed), else 0 [B8]. **Disputed in review:** the play time is
now − (start time) only while the emitter is playing or silent; once it has ended (and while waiting or
starting) it is 0.

## 4. Life of an emitter

### 4.1 Phases

| Phase | Meaning | Left when |
|---|---|---|
| waiting | the delay runs | the delay has elapsed: starting |
| starting | starts at its next update | always, at that update: playing or silent |
| playing | audible and rendered by a voice | it becomes inaudible or loses its voice: silent; it ends |
| silent | started, its timeline runs, but nothing is rendered (inaudible, or no voice free) | it is audible and gets a voice: playing; it ends |
| ended | removed at the end of the update | — |

Each phase exists for a looped and a non-looped emitter [B9].

### 4.2 Timeline

- **Waiting:** at each update (unless paused) *t*_d decreases by *dt*; when *t*_d ≤ 0 the emitter is
  starting (and starts at its next update) [B9].
- **Start** (at the emitter's update in the starting phase, unless paused): start time *T*_s = now; for a
  non-looped emitter end time *T*_e = now + *D* (a looped one has none); *φ* = 1; for a 3D emitter the
  occlusion *o* is set to the occlusion target *o\** (section 5.3); the output volume is set to
  *V* = *b* · *v* · *c* · *o* (with *o* taken as 1 for a 2D emitter; *c*: section 5.4). Then, in the same
  update, the volume step of section 5.3 runs; if it finds the emitter audible and a voice available, the
  emitter is playing from the first sample, else it is silent [B9].
- **End:** a non-looped emitter, playing or silent, ends at the first update with now ≥ *T*_e: its data is
  not played past its end time whatever the frequency [B9]. A looped emitter never ends by itself.
- **Data:** a non-looped emitter's data is followed by silence; a looped emitter's data repeats from its
  first sample immediately after its last, without a gap [B9].
- **Resuming:** when a silent emitter becomes playing again, it resumes at sample
  44 · ((now − *T*_s) mod *D*) (the offset in milliseconds times 44 samples, the program's 88 bytes per
  millisecond), so its timeline continued while it was silent [B9, B3].

### 4.3 Restart

Restarting (play on a sound that has an emitter) clears the stopping mark, moves *T*_s to now and *T*_e by
the same amount (so the remaining length is again *D*), and starts the data again from its first sample:
a playing emitter's voice is rewound at the next update; a silent one resumes from the start when it next
plays [B10, B9]. A waiting emitter keeps waiting.

### 4.4 Stopping

- **Stop at once:** the emitter releases its voice, is detached from its sound and ends [B11].
- **Stop with fade:** the emitter is marked stopping; its fade then decreases (section 5.3) and the emitter
  ends at the first update after which *φ* < 10⁻⁷ [B11, B12]. Since the fade is only updated by the
  volume step, a stopping emitter that is waiting, paused, or 3D beyond its max distance does not fade
  (a non-looped one still ends at *T*_e) [B9, B12].
- **Stop all:** every emitter is stopped at once [B11]. The game does this when the running game is
  disconnected (a level is left) [G5].
- When an emitter ends, its sound no longer has an emitter [B11].

### 4.5 Pause

Pausing is nested. Pausing raises the pause level *L* by one and marks every existing emitter that is not
paused yet with *L*; emitters created afterwards are not paused. Resuming lowers *L* by one and unmarks the
emitters marked with the level being left [B15]. A paused emitter:

- does not count down its delay and does not start [B9];
- if playing, releases its voice at its next update and becomes silent [B9, B12];
- if started, has *T*_s and *T*_e moved forward by every update's time step, so that its timeline is frozen
  and it resumes where it was paused (section 4.2) [B12, B9];
- skips the volume step (its fade and volume are unchanged) [B9].

Who pauses sounds [G3, G4]: the main menu, from opening to closing (`title-menu.md` section 2); the pause
action (it toggles the pause of game time and sounds together); switching away from the program (single
player). The scripts' device pause pauses game time only, not sounds [G4]. Resuming through the device
happens only while a sound pause is held [G3].

## 5. The update

### 5.1 Clock and order

- The **sound clock** is the sound system's own millisecond counter of real time, started when the sound
  system starts; game pauses and the game's time factor do not affect it [B14]. At each update *dt* =
  (now − now of the previous update) · 0.001 s [B14].
- The update runs once per frame of the engine [G1]; that it runs on paused frames too is believed (the
  menu, which pauses the game, plays its music).
- Order within an update [B14, B13]: first the emitters that hold a voice, in voice order, each followed by
  the recomputation of that voice's priority (section 6); then every other emitter, in creation order
  (each emitter is updated once); ended emitters are removed; then the listener is sent to the device
  (section 7.2) and the voices' parameters (section 7.1).

### 5.2 Per-emitter update

At each update an emitter, by phase: waiting → section 4.2; starting → start (section 4.2); playing or
silent → if paused, section 4.5; otherwise the end test (non-looped, section 4.2) and then the volume step
(section 5.3), whose result decides playing or silent [B9]. Then the stopping test of section 4.4 and,
while the emitter has not ended, the AI report timing (section 5.5) [B9].

### 5.3 Volume step

With *c* the category volume (section 5.4) and *L* the listener position (section 7.2):

**3D emitter.** Let *δ* = |*L* − *p*| [B12].

1. If *δ* > *M*: *V* = 0 and the emitter is inaudible; nothing else changes this update [B12].
2. *a* = clamp(*m* / (0.75 · *δ*), 0, 1) (0.75 is the rolloff, section 7.4) [B12]. *a* is not a factor of
   the volume: it only decides the fade direction (step 3) and the priority (section 6); the attenuation
   heard is the device's (section 7.4) [B20].
3. The fade *φ* moves down by 10 · *dt* if the emitter is stopping or *b* · *v* · *c* · *a* < 0.01, else up
   by 10 · *dt*; then clamped to 0 … 1 [B12]. So a sound fades out (over 0.1 s) while its expected level at
   the listener falls below 0.01, and back in when it rises again.
4. The occlusion *o* moves towards the **occlusion target** *o\** by at most *dt* (rate 1 per second; no
   change when |*o\** − *o*| < 10⁻⁷), then is clamped to 0 … 1 [B12]. *o\** is computed at this update by
   S6 (sound occlusion); without S6 it is 1.
5. *V* = 0.1 · (*b* · *v* · *c* · *φ* · *o*) + 0.9 · *V* [B12].

**2D emitter.** *o* = 1; *φ* moves down by 10 · *dt* if stopping, else up, clamped to 0 … 1; no distance
test; then step 5 [B12].

The emitter is **audible** when *V* ≥ 0.01 [B12]. An audible emitter without a voice is playing only if it
obtains one (section 6); an inaudible one is silent (releasing its voice if it had one) [B9, B12].

Note: the smoothing in step 5 is per update, so its speed depends on the frame rate: a change of the
target value is 99 % complete after 44 updates (0.9⁴⁴ < 0.01) [M3].

### 5.4 Volume categories

The category volume *c* is read at every start and volume step, so changes take effect at once (through
the smoothing of step 5) [B4]:

- **effects:** *c* = `snd_volume_eff` · *F* [B4];
- **music:** *c* = `snd_volume_music` [B4].

`snd_volume_eff` and `snd_volume_music` are the volume settings (console range 0 … 1 [G7];
`options-screens.md`). *F* is the **effects factor**, 1 initially [B18]. The game lowers it temporarily
with a shock effect and restores the saved value when the effect ends; a further setter stores a value
clamped to 0 … 1 [G6]. When the shock effect runs and how it changes *F* belongs to the specification of
the player's body sounds (scoping S17) and is **unestablished** here; which code uses the setter is
**unestablished** (believed: scripts).

### 5.5 Reports to the AI

While an emitter has not ended, at the end of each update at which now ≥ its next report time, its next
report time grows by 470 + *r* ms, *r* a pseudo-random integer 0 … 59, and a report is made if S1's
conditions hold [B9, B19]. The first report time is the start time (for a delayed emitter, the time of the
play request) [B9, B6]. Reports are not heard; their use is S24's.

## 6. Voices

The device renders at most as many emitters as there are voices (section 2). Each voice has a priority
*π*: −1 when free [B13].

- The **priority** of an emitter is *P* = *V* · *k* · clamp(*m* / (0.75 · *δ*), 0, 1), with *δ* the
  distance from the listener's world position to the emitter's position (also for a 2D emitter, whose
  position is listener-relative, so for it *δ* is usually large and *P* small; its *k* of 100 compensates
  only partly; believed consequence, not executed) [B13].
- An audible emitter without a voice gets one only if some voice has *π* < *P* (strictly) [B13]. It then
  takes the voice with the lowest *π* (the first in voice order among equal ones); the emitter that held
  that voice, if any, becomes silent; the voice's *π* becomes *P* [B13].
- After each voice's emitter is updated (section 5.1), the voice's *π* is that emitter's *P*, or −1 if the
  voice no longer has an emitter [B13].

## 7. Output to the device

### 7.1 Each voice

A voice is an OpenAL source streaming the emitter's data (mono, 16-bit, at the output rate); the source
itself does not loop (looping is done in the data, section 4.2); its minimum gain is 0 and its maximum
gain 1 [B16]. Every update, for each voice with an emitter [B16]:

| OpenAL source property | Value |
|---|---|
| reference distance | *m* |
| max distance | *M* |
| rolloff factor | 0.75 |
| position | (*p*.x, *p*.y, −*p*.z) |
| source relative | the emitter is 2D |
| gain | clamp(*V*, 10⁻⁷, 1) |
| pitch | clamp(*f*, 0.001, 2) |

No distance model is selected, so OpenAL's default, the inverse distance clamped model, applies; no
velocity is set (no Doppler shift) [B16]. With that model the device multiplies the gain by

    g = m / (m + 0.75 · (clamp(δ', m, M) − m))

where *δ'* is the distance between source and listener as the device sees it (section 7.3), and limits the
result to 0 … 1 (OpenAL 1.1 specification, section 3.4.2) [B16]. So a sound is at full level within *m*,
falls off as an inverse of distance beyond it and keeps its level at *M* until the emitter becomes
inaudible beyond *M* (section 5.3). The base volume may exceed 1 (S1: up to 2.0) [D2], so *V* may exceed 1;
the gain sent is limited to 1.

### 7.2 Listener

Every update the listener is the camera of the frame: its eye position *e*, view direction *d* and up
vector *u* as applied to the device (the values the view of `view-projection.md` section 1.1 is built from;
during play those of `player-movement.md` sections 8, 9) [G1]. They are sent as OpenAL listener position
(*e*.x, *e*.y, −*e*.z), orientation "at" (*d*.x, *d*.y, −*d*.z) and "up" (*u*.x, *u*.y, −*u*.z), and zero
velocity [B17]. Negating z turns the left-handed world (`world-coordinates.md` section 1) into OpenAL's
right-handed one, for the listener and the sources alike, so a sound on the side of the camera's right
vector *r* is heard on the right, one along *d* in front [M5]. In terms of the view space of
`view-projection.md` section 1.1, a source at world position *p* is heard in the direction
((*p* − *e*) · *r*, (*p* − *e*) · *u'*, (*p* − *e*) · *d*) (x right, y up, z ahead) (believed for *u* not
perpendicular to *d*, e.g. while bobbing: OpenAL implementations make "up" perpendicular to "at").
How the device renders a direction (panning, filtering) is the device's and **unestablished**; any
ordinary stereo panning by direction is a reasonable substitute.

The listener's gain is the **overall gain**: 1, except 0 on each of the loading overlay frames
(`loading-screen.md` section 6) [G2, B17].

### 7.3 2D emitters

A 2D emitter's position is relative to the listener: x to the right, y up, z ahead (its OpenAL position
is (x, y, −z) relative to the listener) [B16, M5]. The device still attenuates it by its distance from
the listener, *δ'* = |*p*| (the menu music at (±0.5, 0, 0.3), 0.583 m, is within its min distance 1, so
*g* = 1 [D3, M4]). The volume step uses no distance (section 5.3).

For a 3D emitter *δ'* = *δ* = |*e* − *p*| (negating z keeps distances).

### 7.4 Distance attenuation and the rolloff

What is heard of a 3D emitter at distance *δ* ≤ *M* is the device gain clamp(*V*, 10⁻⁷, 1) times the
distance factor *g* of section 7.1 (then the overall gain, section 7.2) [B16]:

- *g* = 1 for *δ* ≤ *m*: full level within the min distance;
- *g* = *m* / (*m* + 0.75 · (*δ* − *m*)) for *m* < *δ* ≤ *M*; e.g. half level at *δ* = *m* + *m* / 0.75
  = 2.333 *m*;
- beyond *M* nothing is heard: the emitter is inaudible (section 5.3), so the device's clamping at *M*
  never shows.

The **rolloff** is 0.75: a value fixed in the sound library's initialised data, only read (by the volume
step, the priority and the device parameters), not imported by any other program file, so no console
command, configuration or game code changes it [B18, B20]. The same 0.75 enters the engine-side factor
*a* (section 5.3), which reaches 1 within *m* / 0.75 = 1.333 *m*; *a* does not attenuate what is heard
[B20]. That the device applies the inverse distance clamped model with these values is believed from the
OpenAL 1.1 specification (no distance model is selected [B16]); the device library itself was not
analysed.

## 8. Summary of defaults

| Item | Value |
|---|---|
| play volume *v*, frequency *f*, priority factor *k* | 1, 1, 1 (2D: *k* = 100) [B6, B7] |
| fade rate | 10 per second [B12] |
| occlusion rate | 1 per second [B12] |
| smoothing | *V* ← 0.1 · target + 0.9 · *V* per update [B12] |
| audibility threshold | *V* ≥ 0.01 [B12] |
| fade-out level | *b* · *v* · *c* · *a* < 0.01 [B12] |
| rolloff | 0.75 [B16, B18] |
| voices | `snd_targets`: engine 16, default `user.ltx` 24, range 4 … 32 [B18, G7, G8] |
| category volumes | `snd_volume_eff` 1.0, `snd_volume_music` 0.7 (engine initial values), both 1. in the default `user.ltx`; effects factor *F* 1 [B18, G8] |

## 9. Not covered here

- Occlusion target *o\**: S6. Reverberation (the environment of each voice): S7.
- What the AI does with reports: S24.
- The DirectSound device path (`-dsound`) and the 22,050 Hz output rate (section 2).

## 10. Verification facts

Data (all sound files of the archives) [D1, D2]:

- 16,306 `.ogg` files, all under `sounds\`, all mono 44,100 Hz; 16,304 carry parameters, in all of them
  *m* ≤ *M* and *M* ≥ 0.1; base volume 0.01 … 2.0, above 1.0 in 76 files; *m* 0.068 … 1000, *M* 5 … 1000.
- `sounds\music\wasteland2_l.ogg` and `_r.ogg`: 5,518,127 samples, *D* = 125,127 ms; *m* = 1, *M* = 300,
  *b* = 1 [D3]. `sounds\ambient\sparks1.ogg` (a static source of `l01_escape`): 176,953 samples,
  *D* = 4,012 ms, *m* = 0.2, *M* = 10, *b* = 1 [D4].

Worked examples, computed by `model.py` from sections 5 – 7 (*o\** = 1, *F* = 1, `snd_volume_eff` 1,
*v* = 1, *dt* = 0.016 s) [M1 – M6]:

| Case | Result |
|---|---|
| `sparks1`, 3D, *δ* = 0.2 / 1 / 3 / 5 / 10 m | device distance gain *g* = 1 / 0.25 / 0.086957 / 0.052632 / 0.02649; *a* = 1 / 0.266667 / 0.088889 / 0.053333 / 0.026667; steady *V* = 1 at each [M1] |
| `sparks1`, 3D, *δ* = 10.5 m | inaudible: *V* = 0 [M1] |
| `sparks1`, 3D, half level | *g* = 0.5 at *δ* = 0.2 + 0.2 / 0.75 = 0.4667 m [M1] |
| `sparks1` at 3 m, priority | *P* = 0.088889 [M2] |
| `sparks1` at 3 m, stop with fade | *φ* after updates 1 … 7: 0.84, 0.68, 0.52, 0.36, 0.2, 0.04, 0; *V*: 0.984, 0.9536, 0.91024, 0.855216, 0.789694, 0.714725, 0.643252; ends after update 7 [M2] |
| listener walks from 11 m to 5 m (the first audible update) | *V* = 0.1, 0.19, 0.271, … ≥ 0.99 after 44 updates [M3] |
| menu music, 2D, `snd_volume_music` 0.7, at (−0.5, 0, 0.3) | *g* = 1; *V* = 0.7 from the start [M4] |
| listener at the origin looking along +z (up +y); 3D source at (1, 0, 0) | world right of the camera; OpenAL: listener right (at × up) = (1, 0, 0) = the source's direction: heard on the right [M5] |
| non-looped emitter of *D* = 4,012 ms silent from 1,000 ms to 6,000 ms after its start | ended at 4,012 ms; a looped one resumes at sample 44 · (6,000 mod 4,012) = 87,472 [M6] |

## 11. Unestablished

- The DirectSound device path (`-dsound`) and the 22,050 Hz output rate (section 2).
- How the device renders a direction (section 7.2).
- When the shock effect lowers the effects factor and by how much (S17), and who uses the factor's setter
  (section 5.4).
- That the update also runs on paused frames (believed, section 5.1); the order of the sound update and
  the camera update within a frame (a one-frame difference at most; believed inaudible).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives
  `gamedata.db0` … `gamedata.dbd`, the default user settings file `__support\userdocs\user.ltx`, and the
  shipped program files of `bin\`.
- Data examined: every `.ogg` file of the archives (Ogg pages, Vorbis identification and comment headers).
- Analysis: static analysis (decompilation and disassembly with Ghidra, headless) of the shipped sound
  library, engine executable and game library; reading constants and import names from them. No program
  was executed. Public documents used: the OpenAL 1.1 specification (distance models, listener and source
  properties), the Ogg and Vorbis I specifications.

## Gaps found in review

- The per-sound volume factor of S1's AI report range (S1 section 7.1, "how it changes: S2") is the
  emitter's play volume *v* (section 3.3); the range is the max AI distance times *v*, capped at the max
  AI distance. This text does not state it.
