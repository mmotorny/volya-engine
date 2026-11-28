// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Driverless end-to-end tests: serve `dist/` plus the harness in-process,
//! open `__test/run/<id>/<suite>.html` in each target browser, and wait for the
//! page to POST its report back (see tests/browser/harness.js).

use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs};

use volya_engine_devserver::{BASE, Server};

use super::{Result, cmd, output, root, run as run_cmd, step};
use crate::machine::SimulatorLock;

const SUITES: &[&str] = &[
    "smoke",
    "platform",
    "import",
    "audio",
    // A measurement (`measure.rs`), not a test target.
    "audio-load",
    "render",
    "render-screens",
    "render-level",
    "draw-calls",
    "input",
    "game-startup",
    "game-level",
    "game-input",
    "game-screens",
    "game-memory",
    "fidelity",
    "audio-fidelity",
    "d3d9probes",
];
/// Suites served references Bazel made (`VOLYA_REFERENCES`) rather than
/// VOLYA_COMPARE_DIR's.
const REFERENCE_SUITES: &[&str] = &["platform", "fidelity", "audio-fidelity", "d3d9probes"];
const TARGETS: &[&str] = &["chrome", "iphone"];
/// Suites run in Chrome with the kept profile (see [`kept_profile`]), so the
/// game data they read is imported once (they import only what the
/// profile's storage lacks); the others get a fresh profile, as they test
/// the import.
const KEPT_PROFILE: &[&str] = &[
    "render",
    "render-screens",
    "render-level",
    "draw-calls",
    "game-level",
    "game-input",
    "game-screens",
    "game-memory",
    "fidelity",
    "audio-fidelity",
];
const REPORT_TIMEOUT: Duration = Duration::from_secs(60);
/// Suites importing gigabytes from the installer get longer (the audio
/// suite takes about a minute in the iOS Simulator); the game suites
/// also transcode a level's textures at each load where the GPU has no BC
/// (#113).
const SLOW_SUITES: &[(&str, Duration)] = &[
    ("game-startup", Duration::from_secs(1500)),
    ("game-level", Duration::from_secs(1500)),
    ("game-input", Duration::from_secs(1500)),
    ("game-screens", Duration::from_secs(1500)),
    ("game-memory", Duration::from_secs(1500)),
    ("render", Duration::from_secs(240)),
    // The whole game imported on a profile's first run (minutes), then
    // levels loaded by each check.
    ("render-screens", Duration::from_secs(1500)),
    ("render-level", Duration::from_secs(1500)),
    ("draw-calls", Duration::from_secs(1500)),
    ("fidelity", Duration::from_secs(1500)),
    ("audio-fidelity", Duration::from_secs(1500)),
    ("audio", Duration::from_secs(240)),
    // A level session of 12 minutes after the game suites' setup.
    ("audio-load", Duration::from_secs(2400)),
];

fn report_timeout(suite: &str) -> Duration {
    SLOW_SUITES
        .iter()
        .find(|(s, _)| *s == suite)
        .map_or(REPORT_TIMEOUT, |(_, t)| *t)
}

const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const SIMULATOR_NAME: &str = "Volya Engine iPhone 17 Pro Max";
const SIMULATOR_TYPE: &str = "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro-Max";
const IOS_RUNTIME_PREFIX: &str = "com.apple.CoreSimulator.SimRuntime.iOS-";

/// Runs `suite` on `target` (`xtask suite`): the site and the suites
/// from VOLYA_SITE, VOLYA_SITE_NEXT and VOLYA_TESTS (Bazel's browser suite
/// tests, docs/architecture/build.md, Browser suites).
pub fn suite(target: &str, suite: &str) -> Result {
    if !TARGETS.contains(&target) || !SUITES.contains(&suite) {
        return Err(format!(
            "expected a target of {TARGETS:?} and a suite of {SUITES:?}"
        ));
    }
    let dir = |var: &str| {
        env::var_os(var)
            .map(PathBuf::from)
            .ok_or_else(|| format!("{var} is unset"))
    };
    run_on(
        target,
        suite,
        &dir("VOLYA_SITE")?,
        &dir("VOLYA_SITE_NEXT")?,
        &dir("VOLYA_TESTS")?,
    )
}

/// Runs `suite` on `target` on the sites `site` and `next` with the
/// suites in `tests` (a test's runfiles, or the workspace's for a
/// measurement, `measure.rs`).
pub fn run_on(target: &str, suite: &str, site: &Path, next: &Path, tests: &Path) -> Result {
    // The suites' own directory, as the server serves nothing reached through
    // a link out of its mount and runfiles are links to the sources.
    let tests = fs::canonicalize(tests.join("harness.js"))
        .map_err(|e| format!("{}: {e}", tests.display()))?
        .parent()
        .expect("a file's directory")
        .to_path_buf();
    crate::machine::wait_memory()?;
    let simulator = if target == "iphone" {
        Some(Simulator::take()?)
    } else {
        None
    };
    let kept = target == "chrome" && KEPT_PROFILE.contains(&suite);
    let profile = if kept {
        Some(KeptProfile::take()?)
    } else {
        None
    };
    let (server, port, reports) = serve(site, next, &tests, kept)?;
    step(&format!("Browser suite {suite} on {target}"));
    let outcome = run_suite(&server, port, &reports, simulator.as_ref(), target, suite);
    let size = match (profile, &simulator) {
        (Some(profile), _) => Some(profile.check_size()),
        (None, Some(simulator)) => Some(simulator.check_size()),
        (None, None) => None,
    };
    match size {
        Some(Err(size)) if outcome.is_ok() => Err(size),
        Some(Err(size)) => outcome.map_err(|e| format!("{e}; {size}")),
        _ => outcome,
    }
}

/// The dev server for the sites `dist` and `next` and the suites in
/// `tests`, listening (on the kept profile's port when `kept`), with the
/// reports pages post.
fn serve(
    dist: &Path,
    next: &Path,
    tests: &Path,
    kept: bool,
) -> Result<(Arc<Server>, u16, Receiver<String>)> {
    let (tx, reports) = mpsc::channel();
    // Cached as GitHub Pages caches, so browser caches are tested too (#446).
    let server = Server::new(dist)
        .and_then(|s| s.with_next(next))
        .map(Server::with_pages_caching)
        .and_then(|s| {
            s.with_tests(
                tests,
                Arc::new(move |report| {
                    let _ = tx.send(report);
                }),
            )
        })
        .and_then(|s| match std::env::var_os("VOLYA_GOG_INSTALLER") {
            Some(dir) => s.with_installer(Path::new(&dir)),
            None => Ok(s),
        })
        .and_then(|s| match std::env::var_os("VOLYA_DUMP_DIR") {
            Some(dir) => s.with_dumps(Path::new(&dir)),
            None => Ok(s),
        })
        .and_then(|s| match std::env::var_os("VOLYA_COMPARE_DIR") {
            Some(dir) => s.with_references(Path::new(&dir)),
            None => Ok(s),
        })
        .map_err(|e| format!("dev server: {e}"))?;
    let (port, listeners) = listen(kept)?;
    let server = Arc::new(server);
    for listener in listeners {
        let server = Arc::clone(&server);
        thread::spawn(move || server.run_shared(listener));
    }
    Ok((server, port, reports))
}

/// Runs one suite on one target against `server`, printing its report;
/// the failure, if any.
fn run_suite(
    server: &Server,
    port: u16,
    reports: &Receiver<String>,
    simulator: Option<&Simulator>,
    target: &str,
    suite: &str,
) -> std::result::Result<(), String> {
    // The fidelity suite compares with the original's frames (the local
    // cache's), the d3d9probes suite with the probes' results, the
    // platform suite with the native physics outputs, the others with
    // VOLYA_COMPARE_DIR's.
    let references = if REFERENCE_SUITES.contains(&suite) {
        bazel_references()
    } else {
        std::env::var_os("VOLYA_COMPARE_DIR").map(PathBuf::from)
    };
    server
        .set_references(references.as_deref())
        .map_err(|e| format!("{suite} on {target}: dev server: {e}"))?;
    let id = run_id(target, suite);
    let url = format!("http://localhost:{port}{BASE}__test/run/{id}/{suite}.html");
    let outcome = match target {
        "chrome" => chrome(
            &url,
            &id,
            KEPT_PROFILE.contains(&suite),
            reports,
            report_timeout(suite),
        ),
        "iphone" => iphone(
            simulator.expect("taken for iphone"),
            &url,
            &id,
            reports,
            report_timeout(suite),
        ),
        _ => unreachable!(),
    };
    match outcome {
        Ok(report) => {
            println!("{report}");
            if passed(&report) {
                Ok(())
            } else {
                Err(format!("{suite} on {target}"))
            }
        }
        Err(error) => {
            println!("{error}");
            Err(format!("{suite} on {target}: {error}"))
        }
    }
}

/// What a suite of [`REFERENCE_SUITES`] compares with, Bazel's
/// (`VOLYA_REFERENCES`): the original's frames (`//tests/fidelity:frames`)
/// or captured audio (`//tests/fidelity:audio`),
/// the Direct3D 9 probes' results (`//tests/d3d9probes:references`) or the
/// physics comparison's native outputs
/// (`//tests/odeharness:platform_references`).
fn bazel_references() -> Option<PathBuf> {
    env::var_os("VOLYA_REFERENCES").map(|dir| crate::runfile(Path::new(&dir)))
}

/// The kept Chrome profile's directory (see [`KEPT_PROFILE`]), one for the
/// machine (docs/architecture/build.md, Browser suites): in the cache
/// directory, outside every checkout.
fn kept_profile() -> PathBuf {
    // Bazel's tests are told the machine's cache directory (their HOME is
    // their own).
    env::var_os("VOLYA_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").expect("HOME")).join("Library/Caches/volya-engine")
        })
        .join("chrome-profile")
}

/// The kept profile's port, the one every run holding it serves on: storage
/// belongs to an origin, port included, so each port the profile saw kept
/// a whole import of its own (39 GB, #564). Below macOS's ephemeral range
/// (49152–65535), where the other runs' any-port binds land.
const KEPT_PORT: u16 = 47613;

/// The kept profile's origin, as the buckets of Chrome's quota database
/// name it.
const KEPT_ORIGIN: &str = "http://localhost:47613/";

/// The kept profile, held by this process until dropped (an exclusive lock
/// on `chrome-profile.lock` beside it): one Chrome uses it at a time.
pub(crate) struct KeptProfile(#[allow(dead_code)] fs::File);

impl KeptProfile {
    /// Takes the profile, waiting for its holder, and deletes it when it
    /// holds another origin's storage.
    pub(crate) fn take() -> Result<Self> {
        let dir = kept_profile();
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.with_extension("lock");
        let file = fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if file.try_lock().is_err() {
            println!(
                "==> Waiting for the kept Chrome profile ({})",
                path.display()
            );
            file.lock()
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        // Where earlier runs remembered their port.
        let _ = fs::remove_file(dir.join("port"));
        let data = dir.join("data");
        let others = other_origins(&data)?;
        if !others.is_empty() {
            // Chrome keeps an origin's storage across several databases of
            // its own (OPFS's directories through a LevelDB index), so the
            // profile goes whole and the game is imported again.
            println!(
                "==> Deleting the kept Chrome profile, which holds storage of {}",
                others.join(", ")
            );
            fs::remove_dir_all(&data).map_err(|e| format!("{}: {e}", data.display()))?;
        }
        Ok(Self(file))
    }

    /// Reports the profile's size, and fails when it exceeds 1.5 times its
    /// import (its OPFS) and 1 GB for Chrome's own files.
    pub(crate) fn check_size(&self) -> Result {
        let data = kept_profile().join("data");
        let total = disk_usage(&data);
        let import = disk_usage(&data.join("Default/File System"));
        let gb = |bytes: u64| bytes as f64 / 1e9;
        println!(
            "==> Kept Chrome profile: {:.2} GB, its import {:.2} GB",
            gb(total),
            gb(import)
        );
        if total > import + import / 2 + 1_000_000_000 {
            return Err(format!(
                "the kept Chrome profile ({}) takes {:.2} GB, more than 1.5 times its import ({:.2} GB) and 1 GB",
                data.display(),
                gb(total),
                gb(import)
            ));
        }
        Ok(())
    }
}

/// The web origins other than [`KEPT_ORIGIN`] with storage in the Chrome
/// profile `data`, from its quota database (each origin using OPFS, the
/// Cache API or IndexedDB has a bucket there; measured with Chrome 158).
fn other_origins(data: &Path) -> Result<Vec<String>> {
    let quota = data.join("Default/WebStorage/QuotaManager");
    if !quota.exists() {
        return Ok(Vec::new());
    }
    let keys = output(&mut cmd(
        "/usr/bin/sqlite3",
        &[
            "-readonly",
            &quota.to_string_lossy(),
            "select storage_key from buckets",
        ],
    ))?;
    Ok(foreign(&keys))
}

/// The web origins among the storage keys `keys` (one a line) other than
/// [`KEPT_ORIGIN`].
fn foreign(keys: &str) -> Vec<String> {
    let mut origins: Vec<String> = keys
        .lines()
        .map(str::trim)
        .filter(|k| (k.starts_with("http://") || k.starts_with("https://")) && *k != KEPT_ORIGIN)
        .map(String::from)
        .collect();
    origins.sort();
    origins.dedup();
    origins
}

/// The bytes the files under `dir` take on disk (none when it is missing).
fn disk_usage(dir: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(m) if m.is_dir() => disk_usage(&entry.path()),
            Ok(m) => m.blocks() * 512,
            Err(_) => 0,
        })
        .sum()
}

/// The server's port and sockets, on both loopback addresses (#503): pages
/// open `localhost`, which Chrome tries at ::1 as well as 127.0.0.1, and a
/// connection to a port nothing listens on at ::1 can meet itself (TCP
/// simultaneous open; measured on macOS: one in a few hundred connects to
/// an unused port in the ephemeral range), so the page reads its own
/// request back as the response, a network error. With the kept profile
/// (held by the caller), on [`KEPT_PORT`], waiting a while for it and
/// failing when another process keeps it; without it, any port.
pub(crate) fn listen(kept: bool) -> Result<(u16, Vec<TcpListener>)> {
    let bind = |port: u16| -> std::io::Result<(u16, Vec<TcpListener>)> {
        let v4 = TcpListener::bind(("127.0.0.1", port))?;
        let port = v4.local_addr()?.port();
        let v6 = TcpListener::bind(("::1", port))?;
        Ok((port, vec![v4, v6]))
    };
    if kept {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match bind(KEPT_PORT) {
                Ok(bound) => return Ok(bound),
                Err(e) if e.kind() != std::io::ErrorKind::AddrInUse => {
                    return Err(format!("bind port {KEPT_PORT}: {e}"));
                }
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_secs(1)),
                Err(e) => {
                    return Err(format!(
                        "port {KEPT_PORT}, the kept Chrome profile's, is taken by another process ({e}; `lsof -nP -iTCP:{KEPT_PORT}` names it)"
                    ));
                }
            }
        }
    }
    // A free port at 127.0.0.1 may be taken at ::1: a few tries.
    let mut error = None;
    for _ in 0..10 {
        match bind(0) {
            Ok(bound) => return Ok(bound),
            Err(e) => error = Some(e),
        }
    }
    Err(format!("bind: {error:?}"))
}

pub(crate) fn run_id(target: &str, suite: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{target}-{suite}-{nanos:x}")
}

/// Waits for the report of run `id`, skipping stale ones (e.g. from restored
/// Safari tabs reloading old runs).
fn wait_report(id: &str, reports: &Receiver<String>, timeout: Duration) -> Result<String> {
    let deadline = Instant::now() + timeout;
    let header = format!("run {id}");
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match reports.recv_timeout(left) {
            Ok(report) if report.lines().next() == Some(header.as_str()) => return Ok(report),
            Ok(_) => continue,
            Err(_) => return Err(format!("no report within {timeout:?}")),
        }
    }
}

pub(crate) fn passed(report: &str) -> bool {
    let mut lines = report.lines();
    lines.clone().any(|l| l.starts_with("ok ")) && !lines.any(|l| l.starts_with("fail "))
}

pub(crate) fn chrome(
    url: &str,
    id: &str,
    keep: bool,
    reports: &Receiver<String>,
    timeout: Duration,
) -> Result<String> {
    let profile = if keep {
        kept_profile().join("data")
    } else {
        env::temp_dir().join(format!("volya-chrome-{id}"))
    };
    // The HTTP cache is the run's own, as in a fresh profile: a kept one
    // would serve an earlier build's files (cached as GitHub Pages caches).
    let cache = env::temp_dir().join(format!("volya-chrome-cache-{id}"));
    let mut child: Child = cmd(
        CHROME,
        &[
            "--headless=new",
            "--no-first-run",
            "--no-default-browser-check",
            // The page's AudioContext runs without a gesture (synthesized
            // input is not one), so suites see it suspended and resumed.
            "--autoplay-policy=no-user-gesture-required",
            // Silent on the machine running the tests: audio still runs and
            // is measured inside the page, only the device output is muted.
            "--mute-audio",
            &format!("--user-data-dir={}", profile.display()),
            &format!("--disk-cache-dir={}", cache.display()),
            url,
        ],
    )
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|e| format!("launch Chrome: {e}"))?;
    let report = wait_report(id, reports, timeout);
    let _ = child.kill();
    let _ = child.wait();
    if !keep {
        let _ = fs::remove_dir_all(profile);
    }
    let _ = fs::remove_dir_all(cache);
    report
}

/// Opens `url` in Mobile Safari on the test simulator, in a Safari started
/// for it and quit after it (see [`Simulator`]), with Safari's website data
/// deleted before and after: each run serves on a port of its own and
/// storage belongs to an origin, port included, so each run's import stayed
/// (97 GB, #568); deleted, every run starts from empty storage, as before.
fn iphone(
    sim: &Simulator,
    url: &str,
    id: &str,
    reports: &Receiver<String>,
    timeout: Duration,
) -> Result<String> {
    quit_safari(&sim.udid);
    delete_website_data(&sim.udid)?;
    let report = iphone_suite(&sim.udid, url, id, reports, timeout);
    quit_safari(&sim.udid);
    if let Err(e) = delete_website_data(&sim.udid) {
        println!("==> {e}");
    }
    report
}

/// Deletes Safari's website data in simulator `udid` (Safari quit): each
/// origin's directory under `WebsiteData/Default` in Safari's data
/// container, which holds all of that origin's storage (OPFS, the Cache
/// API, service workers) and an `origin` file naming it. Measured with iOS
/// 27.2: a directory per top and frame origin, named by salted hashes, and
/// a run on a new port made a new one.
fn delete_website_data(udid: &str) -> Result {
    let container = output(&mut cmd(
        "xcrun",
        &["simctl", "get_app_container", udid, SAFARI, "data"],
    ))?;
    let dir = Path::new(container.trim())
        .join("Library/WebKit/com.apple.mobilesafari/WebsiteData/Default");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(());
    };
    let (mut origins, mut bytes) = (0, 0);
    for entry in entries.flatten() {
        // Beside them is the salt, kept.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let path = entry.path();
        bytes += disk_usage(&path);
        fs::remove_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        origins += 1;
    }
    if origins > 0 {
        println!(
            "==> Deleted Safari's website data: {origins} origins, {:.2} GB",
            bytes as f64 / 1e9
        );
    }
    Ok(())
}

/// The most the test simulator's data may take: about three times its size
/// after an erase (2.9 GB measured with iOS 27.2, 2 GB of it system assets).
const SIMULATOR_DATA_LIMIT: u64 = 10_000_000_000;

fn iphone_suite(
    udid: &str,
    url: &str,
    id: &str,
    reports: &Receiver<String>,
    timeout: Duration,
) -> Result<String> {
    // With VOLYA_SIM_MEDIA, the run is recorded and its end screenshotted.
    let media = env::var_os("VOLYA_SIM_MEDIA").map(PathBuf::from);
    let recording = match &media {
        Some(dir) => {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            Some(Recording::start(udid, &dir.join(format!("{id}.mp4")))?)
        }
        None => None,
    };
    run_cmd(&mut cmd("xcrun", &["simctl", "openurl", udid, url]))?;
    let report = wait_report(id, reports, timeout);
    if let Some(dir) = &media {
        screenshot_to(udid, &dir.join(format!("{id}.png")))?;
    }
    if let Some(recording) = recording {
        recording.stop()?;
    }
    report
}

/// A screenshot of the test simulator into `path` (default: a new file in
/// the system's temporary directory); returns where.
pub fn screenshot(path: Option<&Path>) -> Result<PathBuf> {
    let udid = simulator()?;
    run_cmd(cmd("xcrun", &["simctl", "bootstatus", &udid, "-b"]).stdout(Stdio::null()))?;
    let path = path.map_or_else(|| media_path("png"), Path::to_path_buf);
    screenshot_to(&udid, &path)?;
    Ok(path)
}

/// Records the test simulator's screen into `path` for `duration`.
pub fn record_for(path: &Path, duration: Duration) -> Result {
    let udid = simulator()?;
    run_cmd(cmd("xcrun", &["simctl", "bootstatus", &udid, "-b"]).stdout(Stdio::null()))?;
    let recording = Recording::start(&udid, path)?;
    thread::sleep(duration);
    recording.stop()
}

/// A new file name for simulator media in the system's temporary
/// directory (outside the repository).
fn media_path(extension: &str) -> PathBuf {
    let dir = env::temp_dir().join("volya-simulator");
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("{}.{extension}", run_id("iphone", "media")))
}

fn screenshot_to(udid: &str, path: &Path) -> Result {
    let path_str = path.to_str().ok_or("non-UTF-8 path")?;
    run_cmd(cmd("xcrun", &["simctl", "io", udid, "screenshot", path_str]).stderr(Stdio::null()))?;
    println!("screenshot: {}", path.display());
    Ok(())
}

/// `simctl io recordVideo` running; stopped with SIGINT, which makes it
/// finish the file (a killed recorder leaves it unreadable).
struct Recording {
    child: Child,
    path: PathBuf,
}

impl Recording {
    fn start(udid: &str, path: &Path) -> Result<Self> {
        let path_str = path.to_str().ok_or("non-UTF-8 path")?;
        let child = cmd(
            "xcrun",
            &[
                "simctl",
                "io",
                udid,
                "recordVideo",
                "--codec=h264",
                "--force",
                path_str,
            ],
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("recordVideo: {e}"))?;
        // The recorder needs a moment before frames arrive.
        thread::sleep(Duration::from_millis(500));
        Ok(Self {
            child,
            path: path.to_path_buf(),
        })
    }

    fn stop(mut self) -> Result {
        run_cmd(&mut cmd("kill", &["-INT", &self.child.id().to_string()]))?;
        self.child.wait().map_err(|e| format!("recordVideo: {e}"))?;
        println!("recording: {}", self.path.display());
        Ok(())
    }
}

/// Runs `steps` (one per argument; see tools/simdriver) in the test
/// simulator with real touches, through an XCUITest that drives Safari.
pub fn sim(steps: &[&str]) -> Result {
    crate::machine::wait_memory()?;
    let sim = Simulator::take()?;
    let udid = &sim.udid;
    let derived = root().join("target/simdriver");
    let derived = derived.to_str().ok_or("non-UTF-8 path")?;
    let out = cmd(
        "xcodebuild",
        &[
            "test",
            "-project",
            "tools/simdriver/SimDriver.xcodeproj",
            "-scheme",
            "SimDriver",
            "-destination",
            &format!("id={udid}"),
            "-derivedDataPath",
            derived,
        ],
    )
    .current_dir(root())
    // xcodebuild passes TEST_RUNNER_-prefixed variables to the test.
    .env("TEST_RUNNER_SIMDRIVER_SCRIPT", steps.join("\n"))
    .output()
    .map_err(|e| format!("xcodebuild: {e}"))?;
    let log = String::from_utf8_lossy(&out.stdout);
    let relevant = |l: &&str| {
        l.contains("simdriver: ")
            || l.contains("error:")
            || l.contains(": failed")
            || l.contains("XCTAssert")
    };
    for line in log.lines().filter(relevant) {
        println!("{}", line.trim());
    }
    if out.status.success() {
        Ok(())
    } else {
        Err("simulator steps failed (see above)".into())
    }
}

/// The test simulator, held by this process (one user at a time), with
/// Safari quit and its tabs closed when taken and when released: a suite
/// page left open keeps its engines' GPU memory, tens of GiB of the Mac's
/// (#430). A helper process does the same if this one dies holding it.
pub struct Simulator {
    udid: String,
    reaper: Option<Child>,
    _lock: SimulatorLock,
}

impl Simulator {
    pub fn take() -> Result<Self> {
        let lock = SimulatorLock::acquire()?;
        let udid = simulator()?;
        run_cmd(cmd("xcrun", &["simctl", "bootstatus", &udid, "-b"]).stdout(Stdio::null()))?;
        quit_safari(&udid);
        // In its own process group, so a Ctrl-C of this one doesn't reach it.
        let reaper = env::current_exe().ok().and_then(|exe| {
            Command::new(exe)
                .args([REAPER, &std::process::id().to_string(), &udid])
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
        });
        Ok(Self {
            udid,
            reaper,
            _lock: lock,
        })
    }
}

impl Simulator {
    /// Reports the size of the simulator's data, and fails when it exceeds
    /// [`SIMULATOR_DATA_LIMIT`].
    fn check_size(&self) -> Result {
        let home = output(&mut cmd("xcrun", &["simctl", "getenv", &self.udid, "HOME"]))?;
        let data = Path::new(home.trim());
        let total = disk_usage(data);
        let gb = |bytes: u64| bytes as f64 / 1e9;
        println!("==> Test simulator's data: {:.2} GB", gb(total));
        if total > SIMULATOR_DATA_LIMIT {
            return Err(format!(
                "the test simulator's data ({}) takes {:.2} GB, more than {:.0} GB",
                data.display(),
                gb(total),
                gb(SIMULATOR_DATA_LIMIT)
            ));
        }
        Ok(())
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        if let Some(mut reaper) = self.reaper.take() {
            let _ = reaper.kill();
            let _ = reaper.wait();
        }
        quit_safari(&self.udid);
    }
}

/// The hidden task [`Simulator`] starts: `<REAPER> <pid> <udid>`.
pub const REAPER: &str = "simulator-reaper";

/// Waits for process `holder` to end, then quits Safari in simulator `udid`
/// unless another process holds the simulator by then.
pub fn reap(holder: &str, udid: &str) -> Result {
    let holder: u32 = holder.parse().map_err(|e| format!("pid: {e}"))?;
    while crate::machine::alive(holder) {
        thread::sleep(Duration::from_secs(2));
    }
    match crate::machine::simulator_holder() {
        Some(pid) if pid != holder && crate::machine::alive(pid) => {}
        _ => quit_safari(udid),
    }
    Ok(())
}

/// Quits Safari in simulator `udid` (which ends its web content and GPU
/// processes) and deletes its tabs, so the next launch restores none.
/// Measured: Safari creates a fresh tab database holding only a start page.
fn quit_safari(udid: &str) {
    // Fails when Safari isn't running.
    let _ = cmd("xcrun", &["simctl", "terminate", udid, SAFARI])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let Ok(home) = output(&mut cmd("xcrun", &["simctl", "getenv", udid, "HOME"])) else {
        return;
    };
    let safari = Path::new(home.trim()).join("Library/Safari");
    for file in ["SafariTabs.db", "SafariTabs.db-wal", "SafariTabs.db-shm"] {
        let _ = fs::remove_file(safari.join(file));
    }
}

const SAFARI: &str = "com.apple.mobilesafari";

fn simulator() -> Result<String> {
    let devices = output(&mut cmd(
        "xcrun",
        &["simctl", "list", "devices", "available"],
    ))?;
    if let Some(udid) = find_device(&devices, SIMULATOR_NAME) {
        return Ok(udid);
    }
    let runtimes = output(&mut cmd("xcrun", &["simctl", "list", "runtimes"]))?;
    let runtime = latest_ios_runtime(&runtimes).ok_or("no iOS simulator runtime installed")?;
    println!("Creating simulator {SIMULATOR_NAME} ({runtime})");
    let udid = output(&mut cmd(
        "xcrun",
        &["simctl", "create", SIMULATOR_NAME, SIMULATOR_TYPE, &runtime],
    ))?;
    Ok(udid.trim().to_owned())
}

/// Parses `    <name> (<udid>) (<state>)` lines from `simctl list devices`.
fn find_device(list: &str, name: &str) -> Option<String> {
    list.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(name)?.strip_prefix(" (")?;
        Some(rest.split_once(')')?.0.to_owned())
    })
}

fn latest_ios_runtime(list: &str) -> Option<String> {
    list.lines()
        .filter_map(|line| line.split_whitespace().next_back())
        .rfind(|id| id.starts_with(IOS_RUNTIME_PREFIX))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_verdict() {
        assert!(passed("run x\nsuite s\nok a\nok b"));
        assert!(!passed("run x\nsuite s\nok a\nfail b\tboom"));
        assert!(!passed("run x\nsuite s"));
    }

    #[test]
    fn finds_other_origins() {
        assert_eq!(KEPT_ORIGIN, format!("http://localhost:{KEPT_PORT}/"));
        let keys = "chrome-extension://nkeimhogjdpnpccoofpliimaahmaaome/\nhttp://localhost:47613/\nhttp://localhost:51234/\nhttp://localhost:51234/\nhttp://127.0.0.1:47613/\n";
        assert_eq!(
            foreign(keys),
            ["http://127.0.0.1:47613/", "http://localhost:51234/"]
        );
        assert!(foreign("http://localhost:47613/\n").is_empty());
    }

    #[test]
    fn parses_simctl_output() {
        let devices = "== Devices ==\n-- iOS 27.2 --\n    iPhone 17 (3E95) (Shutdown) \n    Volya Engine iPhone 17 Pro Max (AB-CD) (Booted) \n";
        assert_eq!(
            find_device(devices, SIMULATOR_NAME).as_deref(),
            Some("AB-CD")
        );
        assert_eq!(find_device(devices, "iPhone 18"), None);
        let runtimes = "== Runtimes ==\niOS 26.0 (26.0 - 23A) - com.apple.CoreSimulator.SimRuntime.iOS-26-0\nwatchOS 27.0 (27.0 - 24R) - com.apple.CoreSimulator.SimRuntime.watchOS-27-0\niOS 27.2 (27.2 - 24B) - com.apple.CoreSimulator.SimRuntime.iOS-27-2\n";
        assert_eq!(
            latest_ios_runtime(runtimes).as_deref(),
            Some("com.apple.CoreSimulator.SimRuntime.iOS-27-2")
        );
    }
}
