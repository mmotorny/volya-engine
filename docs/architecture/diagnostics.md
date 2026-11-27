# Diagnostic log

For problems seen only on a player's device (#346). It is Volya Engine's own and separate from the original's log (docs/behavior/console-log.md, [screens.md](screens.md#settings)): no console log line enters it, and it writes none to the console log. It is always on, as the game's log is (decided by the user, #669, revising #646 and #536; the page turns it on as the engine starts; the setting earlier versions kept, localStorage `volya-engine.diagnostic-log`, is removed, so a log that was off starts at the next launch). Settings' Logs panel has only a line naming the logs there are ("The game's log and Volya Engine's performance log, kept on this device. Download them to report a problem."; one alone: "The game's log, kept on this device. Download it to report a problem.", likewise "Volya Engine's performance log"; #681) and "Download logs", shown while it or the game's log file ([screens.md](screens.md#settings)) exists (the panel hidden otherwise). The download is one ZIP archive, `volya-engine-logs-<date>-version-<version>.zip`, its files stored (written by hand, `volya-engine-formats::zip`; the `logs` op): `xray_player.log`, the game's log byte for byte, and `performance-log.json`, this log's trace, each when there is one. Delete saves, settings, and logs ([web-platform.md](web-platform.md)) deletes both logs: the page turns this one off (`diagnostics_set`: the engine drops the events waiting, closes the store and removes `diagnostic-log/`; the page removes the kept end, `volya-engine.diagnostic-log-end`) and on again, a new log; files left (another copy of the app holds them: `diagnostics_set` answers `deleted` false) show as "Couldn't delete the saves, settings, and logs.". Off is otherwise only for tests (`diagnostics.setEnabled`, this page load only). Technical only: level and file names and error messages are kept whole, as they aren't personal data (decided by the user, #356); no per-voice events.

## Cost

Measured in headless Chrome on the Mac (WebGL 2; #669, the `game-level` suite's "the diagnostic log's cost per frame": the level's start at 1024×768, the log turned off and on four times, each turn the mean of the latest 60 frames after one second):

| Fence | Frame CPU off → on | Frame interval off → on | `render.poll` on | OPFS write once a second (`diagnostics.write_ms`) |
|---|---|---|---|---|
| every frame | 1.70 → 1.79 ms | 16.67 → 16.67 ms | 0.027 ms | 1.76 ms |
| once a second, run 1 | 1.70 → 2.02 ms | 16.67 → 16.67 ms | 0.003 ms | 1.17 ms |
| once a second, run 2 | 1.68 → 1.86 ms | 16.67 → 16.67 ms | 0.014 ms | 1.43 ms |

The CPU difference changes by more between runs than the fence costs, so in Chrome the log costs about 0.1–0.3 ms of the worker's time per frame and no frame time. The write happens once per period, after the frame, and is not counted in the frame's CPU time. The iOS Simulator's figures don't stand for an iPhone, so WebKit is not measured. Its cost is bounded by design instead: in WebKit, `fenceSync` and `getSyncParameter` are synchronous messages to the GPU process (read in WebKit's source). For that reason `gpu.done_ms` samples one frame a second on every browser, not every frame (decided by the user, #669).

## Against the game's log

The two are one record (decided by the user, #646): the diagnostic log holds nothing the game's log records (docs/behavior/console-log.md, as Volya Engine writes it), only what it lacks. Every event, reviewed:

| Event | Kept | Why |
|---|---|---|
| Metadata (`process_name`, thread names) | yes | the game's log has no sessions or threads |
| `session` (the app, the browser, the device, the GPU, the imported game) | yes | none of it is in the game's log |
| `game started` (`loading_ms`) | yes | the game's log marks no launch |
| `level loading`, `level made` (`level`, `load_ms`) | yes | the game's log has each stage's time and memory (`* phase time`, `* phase cmem`, console-diagnostics.md 2.4), not the level's name or the time to its first frame; the diagnostic log records no duration per stage |
| `settings` as a game starts | yes | the game's log lists no values |
| `settings` after a typed console command | no | the game's log has the command (`~ <line>`) |
| `settings` after any other change (the options screen, a configuration file, a bound key) | yes | the game's log doesn't record it |
| `video mode` | yes | the sizes the page draws at and shows, not in the game's log |
| `frame gap`, page visibility, `pagehide`, `pageshow`, AudioContext state | yes | the page's, outside the game |
| GPU errors, `level.hom`, the `gpu.` steps | yes | not in the game's log |
| The log's own failures | yes | the diagnostic log's own |
| `engine.end`, `fatal` | reason only | the game's log has the report (console-log.md 3.8) |
| `engine.end`, other reasons | yes, with the message | none of them is in the game's log |
| Spans, metrics and counts per period, `wasm heap` | yes | the game's log has no timings but the load's stages, and `stat_memory`'s figures only when asked and at a load's end |

A line the game's log gains joins this review: what it records leaves the diagnostic log.

## Format

Chrome's Trace Event Format, JSON Object Format (`{"traceEvents": [...], "displayTimeUnit": "ms", "otherData": {...}}`, built by `Log::trace`), so Perfetto and `chrome://tracing` open it. In the download, `performance-log.json`.

- A session is a process: `pid` its start in Unix seconds, `process_name` "Volya Engine <version>". Threads: the engine worker 1, the page 2, the audio worklet 3.
- Every event carries the version's commit in `cat` (`volya,v:<commit>`). `ts` is microseconds since the Unix epoch (`performance.timeOrigin + now()`), so sessions line up.
- A root span is written whole, as a complete event (`X`) with the spans inside it, only when it lasts at least 50 ms (`frame`) or 10 ms (anything else).
- Per period, every span's mean, maximum and count are series of one counter event (`time ms`), and metrics' min, mean and max and counts' sums series of another (`metrics`): one event's overhead instead of one per series.
- Errors and state changes are instants (`i`), at most 20 per name per period; the excess is reported as one `events over the limit` instant.
- Checked: Perfetto's trace processor loads a `Log::trace` (`VOLYA_DUMP_TRACE` in the core tests), and the `smoke` suite checks a downloaded archive (its entries read back through its central directory) and its trace against the format's documented fields.

## Recording

- `volya-engine-core::diagnostics`: code uses the `tracing` crate on targets starting with `volya` (others are never enabled): `tracing::info_span!("name")` around work; `diagnostics::metric(name, value)` and `diagnostics::count(name, n)`; `tracing::error!` (or `warn!`, `info!`) for instants, fields as arguments. `TraceLayer`, on `tracing_subscriber`'s registry, records them into a `Diagnostics`, which the platform also feeds directly and ends each period with (`summarize`).
- Off (tests), the callsites are disabled (`set_recording`, `tracing`'s interest cache), so instrumentation stays in shipped code: a frame's spans cost 0.03 µs off against 5.4 µs on in headless Chrome's wasm (measured, `debug_diagnostics_bench`).
- The page (`static/diagnostics.js`): `event(name, args)`, `metric(name, value)` (aggregated on the page, sent once a second) and `count(name, n)`, sent to the engine worker only while the log is on.
- The audio worker reports about once a second of output its underruns, its ring's fill at each wake and its mixing time per block; the engine worker records them (`volya-engine-web` `diagnostics::audio_ring`) as `audio.underrun_frames`, `audio.fill_ms` and `audio.mix_ms`, with `audio.missed_wakes` (services its own timeout woke with the ring under half its target: a wake that never came, #452) and `audio.service_gap_ms` (the time between services while the output is expected to run: the page shown, its AudioContext running; #496). The audio worker's instance records no spans.
- Logged instants: the session (version, user agent, screen, `devicePixelRatio`, cores, how the page load started (`navigation`: the navigation entry's `type`, `navigate`, `reload` or `back_forward`, documented in Navigation Timing Level 2; `restored_reload` when it is the reload of a page restored from the back-forward cache, below), `display_mode` (`standalone` where `(display-mode: standalone)` matches or iOS's `navigator.standalone` is true, else `browser`), the GPU's state (`gpu_state`), WebGL 2's renderer string (`gpu_renderer`: `UNMASKED_RENDERER_WEBGL` where exposed, else `RENDERER`), texture path (`gpu_textures`) and message, and the imported game below); game started, level loading and level made (name, loading time); `video mode` when the drawn or canvas size changes; `frame gap` (an interval over 250 ms); every end (below); GPU errors; page visibility; `pagehide` and `pageshow` with `persisted` (a restored page reloads at once, its worker ended at `pagehide`, so its `pageshow` reaches only the next session, as `restored_reload`); AudioContext state changes; the log's own failures.
- A level's GPU work, each step stored at once as it begins (without ending the period, `gpu_step`), so the last record before the page's process dies names the step (#443): `gpu.texture` (each prepared texture, its path and size), `gpu.geometry` (each turn of the world's upload), `gpu.world_make`, `gpu.warm_up_draw`, `gpu.warm_up_submitted`, `gpu.level_frame` (the level's first 10 frames), `gpu.level_frame_presented` and `gpu.level_frame_done` (the browser reporting the GPU done with it), the steps before the frames with the texture and geometry bytes written since the renderer was made (cumulative: a second level adds to the first's), the particles' vertex buffer, and the textures and buffers the GPU holds now (`texture_MiB`, `geometry_MiB`, `particle_buffer_MiB`, `live_texture_MiB`, `live_buffer_MiB`), all with the wasm heap (`heap_MiB`); up to 1000 a period for each `gpu.` name (20 for other instants).
- `settings` as a game starts, and again in the frame after a command not typed in the console (the options screen's, a configuration file's, a bound key's; `Console::untyped_generation`) changes any of them: the console's values (`status`, as `cfg_save` writes them) of every setting that changes the picture or a frame's cost: `renderer`, `vid_mode`, `r__supersample`, `_preset`, `r__detail_density`, `r__geometry_lod`, `texture_lod`, `r__tf_aniso`, `r1_tf_mipbias`, `r1_dlights`, `r1_dlights_clip`, `r1_glows_per_frame`, `r2_sun_lumscale`, `r1_pps_u`, `r1_pps_v`, `rs_c_gamma`, `rs_c_contrast`, `rs_c_brightness`, `rs_vis_distance`, `r1_ssa_lod_a`, `r1_ssa_lod_b`, `r1_lmodel_lerp`, `rs_stats`. A setting added to the engine's reads joins the list (`engine::SETTINGS`). The `game-memory` suite checks one at each game start and one after a video mode change.
- The session's game, from `import.json` (`game::identity`), never assumed by edition: `game` (`imported`, `partly imported`, `not imported`, or `unreadable` with `game_problem`); `game_version`, the title menu's caption (`ver. 1.0006`, whatever the registry holds: title-menu.md 3.4); `game_language`, the language imported (empty when the installer tags none); and, for imports recorded since, `installer_exe` and `installer_bin` (the picked files' names) and `installer_name` and `installer_version` (the setup's `AppName` and `AppVersion`). The `smoke` suite checks both versions with written records.
- Per period (a second while a game runs, 10 s otherwise): the spans (`frame`, `engine`, `sound`, `render`, `render.poll`, `render.scene` (`render.app_scene`, `render.draw_list`), `render.upload_world`, `render.acquire`, `render.encode` (`render.world` with `render.details`, `render.post`, `render.submit`, `render.submit_fit`), `render.present`, `textures`, `prepare.texture`, `prepare.world`, `prepare.warm_up`), the metrics (`frame.interval_ms`, `frame.late` for intervals over 25 ms, `gpu.done_ms`, a world frame's draw calls and state changes `render.draws`, `render.pipelines`, `render.bind_groups`, `render.buffers` (`volya-engine-render::stats`, #387), `load.gpu_wait_ms`, the audio ones, `diagnostics.write_ms`) and the `wasm heap` counter.

## GPU time

Spans time only the engine worker's CPU, not the browser's GPU process or the GPU (#355). While the log is on, `gpu.done_ms` is, for one frame a second ([Cost](#cost)), the time from its last submission until WebGL reports the GPU done (a fence made at the next frame's poll, not right after the frame's calls: in WebKit `fenceSync` is a synchronous message to the GPU process, read in its source; checked at each poll, so to a frame): the GPU process's handling, the wait behind earlier frames and the GPU's work, not presentation. `level_stats`' `gpu_done_ms` is the latest two samples' mean.

WebGL 2 has no timestamp queries (headless Chrome's neither): the renderer times no pass on the GPU (`Renderer::set_gpu_timing` does nothing; `EXT_disjoint_timer_query_webgl2` is #493).

## Storage

- OPFS `volya-engine/diagnostic-log/0` … `7`, segments of 256 KiB with one sync access handle each (readwrite: one copy of the app gets them; another keeps its log in memory).
- A reload's page can find them still held: the closed page ends its worker in `pagehide`, but Chrome lets go of the worker's handles a little later (measured, #633, headless Chrome under load: `NoModificationAllowedError` 89 ms after a reload; let go of 29–355 ms after the new worker first tried, in 18 of 54 reloads). A copy given a `page_closed` end the page before kept retries every 25 ms for up to 5 s while they're held, recording the wait; otherwise (a live copy holds them) it keeps its log in memory at once.
- Turning the log on or off, reading it and its status run one at a time, so a read can't hold the handles while they open (#428): the log is reported on only once its store is open.
- Each event is one self-contained JSON line; a period's events are appended in one write and flushed (each period, and at once after an end).
- When the newest segment is full the oldest is emptied and becomes the newest, so the oldest events go first. Each segment begins with a header holding its sequence number (the order across reloads) and the session's metadata events. On opening, a segment whose header doesn't read (the origin is shared) is emptied and a line cut by an interrupted write is dropped. The download joins the lines into one trace.
- The cap is 2 MiB: at about 1.2 KB a second while a game runs (measured, the `menu` suite's session in headless Chrome) it holds about half an hour of play, and stays small to send by mail or chat.

## Ends

Decided by the user (#356): every way the engine or a game ends writes an instant `engine.end` with `reason` and `message`, then stores the period at once. It is never limited per period and is the last event of its write. A session can go on after one (the title menu after a quit, a page shown again), so a session's last event tells how it ended: one that isn't `engine.end` ended abruptly (the tab killed or crashed, out of memory).

- Reasons: `fatal` (`App::fatal`; no message: the game's log has its report), `load_failed` (a New Game's loading, or the game's data at launch), `quit` (to the title menu, or leaving the game for the app's screens), `unmounted` (the game's files released under it), `panic` (its message and `location`), `engine_error` (an exception out of a frame), `worker_error`, `page_error`, `worklet_error` (an uncaught error or unhandled rejection in the engine worker, the page, the audio worklet), `device_lost` (`webglcontextlost`), `page_hidden` (`visibilitychange`), `page_closed` (`pagehide`, which also ends the worker).
- Ends the engine worker meets itself are recorded and written in the same task through the sync access handle's `write` and `flush`, which are synchronous (documented, File System Standard); the engine's own at the end of the frame that met them. A panic's hook writes before the wasm aborts, unless the panic came from inside the log.
- The page's and the worklet's ends are messages to the worker, written when it handles them.
- Hidden or closed, the page first keeps the end itself (localStorage `volya-engine.diagnostic-log-end`: `setItem` is synchronous; shown again, the page forgets it), then tells the worker, and on `pagehide` ends it. `terminate()` discards the worker's queued tasks (documented, HTML Standard), so the worker's record is only attempted: it stored `page_closed` 9 times in 12 in headless Chrome and 0 times in 3 in the iOS Simulator (measured, `smoke`), hence the page's copy. A closing page's `visibilitychange` can come after its `pagehide` (seen in headless Chrome), so a kept `page_closed` isn't replaced by `page_hidden`.
- The next session with the log on adds the page's copy (`recorded_by: "page"`) unless the closed session stored anything from that time on.
- Only attempted: `pagehide` itself, which a phone's system ending a page in the background doesn't fire (documented, Page Lifecycle API; the last end is then `page_hidden`), and the page's copy reaching disk once the page is gone (believed).
