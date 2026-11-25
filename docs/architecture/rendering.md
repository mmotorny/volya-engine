# Rendering

`crates/volya-engine-render` draws; `crates/volya-engine` turns screens into scenes and draw lists (`scene.rs`, `present.rs`); `platforms/web/src/gpu.rs` owns the device and the canvas.

## Backends

- WebGPU on an `OffscreenCanvas` transferred from the main thread. Where the browser gives no WebGPU adapter, wgpu's WebGL 2 backend draws on the same canvas (#113, decided by the user). Detected by the failed adapter request, asked for before any surface since a canvas keeps the first kind of context it gives (`gpu.rs` `connect`), not by the absence of `navigator.gpu`: the iOS Simulator's WebKit has it and gives no adapter (measured). The Simulator's WebGL 2 has ETC2 and ASTC, no BC (measured).
- `status().gpu` reports the backend (`webgpu`, `webgl2`) and how BC textures reach the GPU (`bc`, `etc2`, `rgba8`). With neither backend the engine still starts, without graphics: `status().gpu.state` is `unavailable` with a `message`, and the page shows "This browser can't run the game" with it.
- One wasm build with both backends (wgpu's GL backend through the `webgl` feature of `volya-engine` and `volya-engine-render`, which `volya-engine-web` always enables), decided by the user: download size is not a priority (#121; a build without the GL backend was tried and dropped, #186).
- The Graphics setting (#121, the app's Settings): WebGPU (Recommended), then WebGL 2; an option the browser can't run is disabled, "Not supported by this browser".
  - Before the engine takes the canvas the engine worker finds what the browser supports (`static/graphics.js`: a WebGPU adapter asked for; a WebGL 2 context on a canvas of its own) and starts the engine with the stored choice where supported, else the first supported option (`start(canvas, webgl)`). WebGL 2 chosen draws through WebGL 2 where WebGPU works too.
  - Only an explicit choice is stored (the page's localStorage, `volya-engine.graphics`, sent in the worker's start message), so a browser that gains WebGPU moves to it, and a stored choice the browser can't run stays stored.
  - The choice applies when the game next starts: Settings is reachable only outside a game, and Play starts the engine anew where the choice differs from the running backend. A canvas keeps its first kind of context and gives its drawing to one worker, so the page unmounts the game's files, replaces the canvas and the worker, and reconnects audio.
  - One restart at a time: Play shows Starting… and takes no second press until the game starts or fails, since a restart can take seconds and a second one would end the engine the first press waits for (#369).

## The WebGL 2 path

`Renderer::compat`: no storage buffers in vertex programs and no base-vertex draws.

- The world reads its per-draw records from an RGBA32F texture (9 texels a draw, 64 draws a row; a variant of `world.wgsl` made by `world::world_source`), indexed by instance.
- Index ranges are rebased at upload: each range a draw uses is written with the draw's first vertex added (a range used from two first vertices is written twice), the objects' indices 32-bit where they pass 16 bits; draws then start at vertex 0.
- wgpu's GL backend keeps every written byte in the wasm heap until it knows the submission finished (read in wgpu-hal's source), and a WebGL sync object never signals in the frame it was issued (documented, WebGL 2 specification). So uploads go in pieces submitted one by one, the level's geometry turns wait two frames each, and texture preparation waits two frames whenever 4 MiB has been written since the frame two frames back (`Renderer::texture_bytes_written`).
- Native tests run the world's tests both ways on the Mac's GPU (`set_compat`) and translate every program to GLSL ES 3.00 with naga as wgpu's GL backend does (`tests/glsl.rs`); docs/testing.md.

## Textures

- Textures upload as stored: BC when the GPU has `texture-compression-bc` ([game-data.md](game-data.md#original-formats-no-conversion)). Every level is written in bands of rows of at most 2 MiB.
- Without BC they are transcoded at run time to ETC2 (`volya-engine-formats::etc2`, our encoder from the Khronos Data Format Specification: BC1 to RGB8, or RGB8A1 when the texture has cut-out texels; BC2 and BC3 to RGBA8 with EAC alpha); with neither BC nor ETC2 they are decoded to RGBA8.
- ETC2 over ASTC (#113): ETC2 takes BC's memory (127 MiB of textures for `l01_escape` either way, measured natively); ASTC 4×4 takes 64 % more (BC1's 4 bits a texel become 8) for a difference visible only magnified; ASTC 6×6 is worse than ETC2 and slower (measured with astcenc on the level's textures).
- The encoder fits every texel's colour whatever its alpha: the game's alpha is often data (light maps keep a lighting term there). Method (#183): it works from a BC block's four colours, judges ETC1 tables and T/H candidates along the grey axis (as etcpak's table search), tries planar only where the best unquantized plane could win and T/H only where the error is large and the colours differ in hue, and walks T/H distances outwards while they improve (QuickETC2, Nah 2020). `volya-engine/tests/transcode.rs` holds its quality to floors per kind of block.
- The release build compiles `volya-engine-formats` at `opt-level = 3`, about twice as fast as `"s"` for the encoder (measured natively).
- Transcoding stays on the engine worker, decided by the user: transcode workers only if the encoder alone leaves a load well above a few seconds.
- Nothing is stored: every load transcodes again, decided by the user, as stored transcodings would take as much storage as the BC textures (133 MB for the first level, 2.05 GB for all the game's BC textures; measured), too much on a phone on top of the game. Textures outside a level's preparation (the menus') are transcoded when first drawn.

## Level preparation

On every backend (`textures::wanted`; #184, #355): the browser's GPU process copies what the engine hands it later, so uploads made at the level's first frame stalled that frame's presentation on an iPhone (believed, from its diagnostic log).

1. A New Game's loading waits under `st_loading_textures` once its work is done (`App::set_prepare_textures`, our addition) while the platform prepares the level's textures, about 30 ms of them per task turn so frames run between and the loading screen keeps drawing: each is read into one buffer, transcoded (without BC) into another, both reused, and uploaded; the presenter takes them at the world's upload (`Renderer::add_prepared`).
2. Once the level is made the loading waits under its last caption while the platform uploads the level's geometry and the objects' models a few pieces per turn (`App::world_to_prepare`, `Presenter::prepare_world`, `Renderer::begin_geometry`, `GeometryUpload::step`); the placed effects' world clock receives the wait (`LevelScreen::wait`). The objects' CPU geometry goes once uploaded.
3. On WebGPU each turn then waits until the browser reports the GPU done with what was submitted (`onSubmittedWorkDone`, giving up after 30 s; `load.gpu_wait_ms`), so the copying happens while the loading image shows; on WebGL 2 the turns wait frames instead (above).
4. Last, the level's start view is drawn once into a frame of its own that is never shown, and the GPU waited for (`gpu::warm_up`, span `prepare.warm_up`): the world's making and the first-draw work (believed: program compilation, the render bundles; #376) come before the level shows. What the loading screen shows is unchanged.

## The canvas and the video mode

- The page sends the canvas's CSS size and `devicePixelRatio` on start and on change; the worker sizes the canvas and surface in device pixels. The frame loop runs in the worker on `requestAnimationFrame` (a 60 Hz timer where workers lack it); while the game runs its screen is drawn every frame.
- The game draws at its video mode (`vid_mode`, docs/behavior/options-screens.md section 9), not at the canvas's size: menus and 3D go to an offscreen frame of that size, drawn into the canvas scaled to fit with its aspect kept, black around it (#49). Pointer positions are taken relative to the drawn picture.
- The resolution list offers the canvas's own size and the common 4:3, 5:4, 16:10 and 16:9 sizes that fit it, at least 800 wide (the canvas's size alone if none is left); browsers expose no display modes. The page counts as windowed, so a stored mode not in the list is used as it is.

## The static world

- `level.geom`'s vertex and index buffers upload as stored; packed attributes are decoded in the vertex programs (WebGPU vertex formats reproduce the spec's decoding).
- Its draws are recorded once into two render bundles (priorities 0–1 and 2–3) replayed every frame; strict-sorted draws are ordered each frame. What depends on the camera (near/far shader variants, size culling) is decided per draw in the shaders from per-draw data.
- No CPU copy of the geometry is kept: the loading stage reads only the buffers' layout, the spatial stage checks the draws' indices a piece at a time, and the presenter streams the file to the GPU in pieces of at most 2 MiB with `Queue::write_buffer`, which unlike a buffer mapped at creation copies nothing into the wasm heap on the web (wgpu's WebGPU backend source).
- Scripted shader definitions (Lua) are not run: the specified effect of each script the level uses is implemented directly (`shading.rs`).

## Objects

- A level's spawned objects (docs/behavior/level-objects.md) load across its stages: the switch pass under `st_server_connecting`, the model files under `st_client_spawning`, their surfaces with the level's under `st_loading_textures`, their light inputs (rays against the collision world, taken converged at once) under `st_client_synchronising`.
- Each model file is read once and shared by every object showing it; its vertices are quantized as the first renderer stores them (16 bytes: `s16` position, `u8` normal, `s16` texture coordinate) into one vertex buffer for all models, with only the full-detail indices, uploaded with the static world and then dropped on the CPU.
- Objects are drawn at rest (every vertex at its stored position, transformed per object in the vertex program) in the world's render bundles, with element 1 of their model shaders (the projected light texture is unestablished). An object switching changes its instance's placement in the world's per-draw data; a hidden draw gets a bounding radius of 0, so the shaders' size test drops it.

## Particles

docs/behavior/particle-drawing.md (#322); the simulation: [simulation.md](simulation.md#particles).

- The frame values at birth and the frame animation are part of the simulation (they draw from the shared generator). The sprites are built on the CPU each frame from the camera (`volya-engine-core::particles::sprite`), four vertices of 24 bytes a particle, uploaded with absolute indices (no base vertex) and drawn without storage buffers, so the WebGL 2 path draws them as WebGPU does.
- Each effect's shader is resolved with the level's under `st_loading_textures` (`particle_drawing::resolve`): class `PARTICLE` from `shaders.xr`; `particles\xadd` and `particles\xdistort`, scripted, implemented from the spec's section 5.3 (no Lua runs).
- Effects are tested against the view frustum (a sphere is rejected only when outside a plane by more than its radius) and the screen measure; sectors and `level.hom` are not (a superset; #329).
- They are queued into the world pass as section 7.4 orders (`ParticleQueue`): `SET` after the priority 0–1 static draws; `ADD`, `ALPHA-ADD` and the scripted shaders' normal pass after the priority 2–3 ones; `BLEND` merged with the static strict-sorted draws back to front (on equal keys statics first, our choice).
- With a distortion list the world is drawn into a texture of its own with its depth kept; the distortion target (cleared to 127/255) gets element 4 of the scripted shaders back to front, depth-tested against the scene with the scene's view and projection (believed, #329); a composition pass looks the scene up at t + (D.rg − 0.5) · 0.05 into the frame (`distortion.wgsl`). The rest of the presentation is unestablished (static-shading.md 10.2), so nothing else is applied. Distortion is always available (our programs exceed pixel shader 1.4).
- The particles' pipelines (every pass the effects' shaders resolve to, with each culling) and their vertex and index buffers are made with the renderer and kept, the buffers grown and written with `Queue::write_buffer`: GPU objects made with a level, or buffers mapped at creation, stayed allocated amid the freed level and split the wasm heap, so the next level's collision world no longer fitted (measured, #357). At the world's upload the buffers are grown to the most particles the level's objects can hold at once (every placed record and zone, and the largest ambient effect; `Renderer::reserve_particles`), so nothing is made on the GPU while the level runs.

## Lights

The records and their sources: [simulation.md](simulation.md#dynamic-lights).

- The renderer draws every sector with the full view frustum (the superset level.md 9.2 allows).
- A surface carries its light elements (`Surface::light`: program, alpha reference, priority; dynamic-lights.md 6.1); the draws whose elements have priority 0 or 1 are listed once per world, grouped by program and surface.
- Each light pass (`WorldFrame::lights`) draws again, after the sky and before the priority 2–3 bundle, those whose bounding sphere meets both the light's frustum and the camera's (a superset of what the camera's pass submitted), with one of five light pipelines (`vs_light_*` and `fs_light` in `world.wgsl`) and the pass's constants at a dynamic offset of one uniform buffer. All are made with the renderer (`Renderer::new`), the constants buffer grown only past 64 passes in a frame. No storage buffer or base vertex is needed beyond the world's own, so the WebGL 2 path draws them alike.
- The positions are `@invariant` in both passes, so the light passes' depth test meets the drawn surfaces.
- Glows are particle batches (`ParticleProgram::Glow`, `Flare`; `ParticleQueue::Glow`) drawn after the strict-sorted passes, their textures loaded with the world.

## Colour and video

- Colour values pass through unconverted (non-sRGB targets). UI blending and filtering follow docs/formats/ui-layout.md (#23).
- Video frames upload as Y'CbCr planes, are packed unconverted into the video texture on the GPU and convert to RGB when drawn, after filtering (docs/behavior/startup.md 6.6): about 0.2 ms a 876×400 frame against 1.2 ms converting on the CPU (measured natively).
