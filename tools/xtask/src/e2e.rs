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

use super::{Profile, Result, cmd, next_build, output, root, run as run_cmd, step, web};
use crate::machine::SimulatorLock;

const SUITES: &[&str] = &[
    "smoke",
    "platform",
    "import",
    "audio",
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
];
/// Suites only desktop Chrome runs: fidelity against the original is
/// judged there (docs/testing.md).
const CHROME_ONLY: &[&str] = &["fidelity"];
const TARGETS: &[&str] = &["chrome", "iphone"];
/// Suites run in Chrome with the profile kept under `target/`, so the game
/// data they read is imported once (they import only what the profile's
/// storage lacks); the others get a fresh profile, as they test the import.
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
    ("audio", Duration::from_secs(240)),
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

/// Runs `args` (targets and suite names, in any order): every suite on every
/// target by default. Naming suites runs only those, without the native
/// tests (run them with `cargo test`).
pub fn run(args: &[&str]) -> Result {
    if let Some(bad) = args
        .iter()
        .find(|a| !TARGETS.contains(a) && !SUITES.contains(a))
    {
        return Err(format!(
            "unknown argument {bad}; expected targets {TARGETS:?} or suites {SUITES:?}"
        ));
    }
    let named = |all: &'static [&'static str]| -> Vec<&'static str> {
        let some: Vec<_> = all.iter().copied().filter(|x| args.contains(x)).collect();
        if some.is_empty() { all.to_vec() } else { some }
    };
    let targets = named(TARGETS);
    let suites = named(SUITES);
    crate::machine::wait_memory()?;
    let simulator = if targets.contains(&"iphone") {
        Some(Simulator::take()?)
    } else {
        None
    };

    if suites.len() == SUITES.len() {
        step("Native tests");
        run_cmd(&mut cmd("cargo", &["test", "--workspace"]))?;
    }
    web(Profile::Test)?;
    let next = next_build()?;

    let (tx, reports) = mpsc::channel();
    // Cached as GitHub Pages caches, so browser caches are tested too (#446).
    let server = Server::new(&root().join("dist"))
        .and_then(|s| s.with_next(&next))
        .map(Server::with_pages_caching)
        .and_then(|s| {
            s.with_tests(
                &root().join("tests/browser"),
                Arc::new(move |report| {
                    let _ = tx.send(report);
                }),
            )
        })
        .and_then(|s| match std::env::var_os("VOLYA_GOG_INSTALLER") {
            Some(dir) => s.with_installer(std::path::Path::new(&dir)),
            None => Ok(s),
        })
        .and_then(|s| match std::env::var_os("VOLYA_DUMP_DIR") {
            Some(dir) => s.with_dumps(std::path::Path::new(&dir)),
            None => Ok(s),
        })
        .and_then(|s| match std::env::var_os("VOLYA_COMPARE_DIR") {
            Some(dir) => s.with_references(std::path::Path::new(&dir)),
            None => Ok(s),
        })
        .map_err(|e| format!("dev server: {e}"))?;
    let (port, listeners) = listen()?;
    let server = Arc::new(server);
    for listener in listeners {
        let server = Arc::clone(&server);
        thread::spawn(move || server.run_shared(listener));
    }

    let mut failures = Vec::new();
    for &target in &targets {
        for &suite in &suites {
            if target != "chrome" && CHROME_ONLY.contains(&suite) {
                continue;
            }
            step(&format!("Browser suite {suite} on {target}"));
            // The fidelity suite compares with the original's frames
            // (`cargo xtask original views`), the others with
            // VOLYA_COMPARE_DIR's.
            let references = if suite == "fidelity" {
                original_frames()?
            } else {
                std::env::var_os("VOLYA_COMPARE_DIR").map(PathBuf::from)
            };
            server
                .set_references(references.as_deref())
                .map_err(|e| format!("dev server: {e}"))?;
            let id = run_id(target, suite);
            let url = format!("http://localhost:{port}{BASE}__test/run/{id}/{suite}.html");
            let outcome = match target {
                "chrome" => chrome(
                    &url,
                    &id,
                    KEPT_PROFILE.contains(&suite),
                    &reports,
                    report_timeout(suite),
                ),
                "iphone" => iphone(
                    simulator.as_ref().expect("taken for iphone"),
                    &url,
                    &id,
                    &reports,
                    report_timeout(suite),
                ),
                _ => unreachable!(),
            };
            match outcome {
                Ok(report) => {
                    println!("{report}");
                    if !passed(&report) {
                        failures.push(format!("{suite} on {target}"));
                    }
                }
                Err(error) => {
                    println!("{error}");
                    failures.push(format!("{suite} on {target}: {error}"));
                }
            }
        }
    }

    if failures.is_empty() {
        step("All tests passed");
        Ok(())
    } else {
        Err(format!("failed:\n  {}", failures.join("\n  ")))
    }
}

/// The original's frames the fidelity suite compares with, where
/// VOLYA_GOG_INSTALLER names the installer.
fn original_frames() -> Result<Option<PathBuf>> {
    if std::env::var_os("VOLYA_GOG_INSTALLER").is_none() {
        return Ok(None);
    }
    crate::original::references(&crate::original::cache()?).map(Some)
}

/// The kept Chrome profile's directory (see [`KEPT_PROFILE`]).
fn kept_profile() -> PathBuf {
    root().join("target/chrome-profile")
}

/// The server's port and sockets, on both loopback addresses (#503): pages
/// open `localhost`, which Chrome tries at ::1 as well as 127.0.0.1, and a
/// connection to a port nothing listens on at ::1 can meet itself (TCP
/// simultaneous open; measured on macOS: one in a few hundred connects to
/// an unused port in the ephemeral range), so the page reads its own
/// request back as the response, a network error. On the port the kept
/// profile last saw when it is free, as storage belongs to an origin, port
/// included (else any port, remembered for the next run).
pub(crate) fn listen() -> Result<(u16, Vec<TcpListener>)> {
    let bind = |port: u16| -> std::io::Result<(u16, Vec<TcpListener>)> {
        let v4 = TcpListener::bind(("127.0.0.1", port))?;
        let port = v4.local_addr()?.port();
        let v6 = TcpListener::bind(("::1", port))?;
        Ok((port, vec![v4, v6]))
    };
    let file = kept_profile().join("port");
    let kept = fs::read_to_string(&file)
        .ok()
        .and_then(|p| p.trim().parse::<u16>().ok());
    if let Some(port) = kept
        && let Ok(bound) = bind(port)
    {
        return Ok(bound);
    }
    // A free port at 127.0.0.1 may be taken at ::1: a few tries.
    let mut error = None;
    for _ in 0..10 {
        match bind(0) {
            Ok(bound) => {
                fs::create_dir_all(kept_profile())
                    .map_err(|e| format!("{}: {e}", file.display()))?;
                fs::write(&file, bound.0.to_string())
                    .map_err(|e| format!("{}: {e}", file.display()))?;
                return Ok(bound);
            }
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
/// for it and quit after it (see [`Simulator`]).
fn iphone(
    sim: &Simulator,
    url: &str,
    id: &str,
    reports: &Receiver<String>,
    timeout: Duration,
) -> Result<String> {
    quit_safari(&sim.udid);
    let report = iphone_suite(&sim.udid, url, id, reports, timeout);
    quit_safari(&sim.udid);
    report
}

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
