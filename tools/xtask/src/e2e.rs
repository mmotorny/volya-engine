// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Driverless end-to-end tests: serve `dist/` plus the harness in-process,
//! open `__test/run/<id>/<suite>.html` in each target browser, and wait for the
//! page to POST its report back (see tests/browser/harness.js).

use std::net::TcpListener;
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs};

use volya_engine_devserver::{BASE, Server};

use super::{Profile, Result, cmd, output, root, run as run_cmd, step, web};

const SUITES: &[&str] = &[
    "smoke", "platform", "import", "audio", "render", "input", "menu",
];
const TARGETS: &[&str] = &["chrome", "iphone"];
const REPORT_TIMEOUT: Duration = Duration::from_secs(60);
/// Suites importing gigabytes from the installer get longer.
const SLOW_SUITES: &[(&str, Duration)] = &[("menu", Duration::from_secs(300))];

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

    if suites.len() == SUITES.len() {
        step("Native tests");
        run_cmd(&mut cmd("cargo", &["test", "--workspace"]))?;
    }
    web(Profile::Test)?;

    let (tx, reports) = mpsc::channel();
    let server = Server::new(&root().join("dist"))
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
        .map_err(|e| format!("dev server: {e}"))?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("bind: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    thread::spawn(move || server.run(listener));

    let mut failures = Vec::new();
    for &target in &targets {
        for &suite in &suites {
            step(&format!("Browser suite {suite} on {target}"));
            let id = run_id(target, suite);
            let url = format!("http://localhost:{port}{BASE}__test/run/{id}/{suite}.html");
            let outcome = match target {
                "chrome" => chrome(&url, &id, &reports, report_timeout(suite)),
                "iphone" => iphone(&url, &id, &reports, report_timeout(suite)),
                _ => unreachable!(),
            };
            match outcome.map(|report| known_failures(target, &report)) {
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

fn run_id(target: &str, suite: &str) -> String {
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

/// Tests a target is known not to support, with the reason. Verified: the iOS
/// Simulator exposes `navigator.gpu` but never returns an adapter, on the main
/// thread or in workers; WebGPU needs the real device.
const KNOWN_FAILURES: &[(&str, &str, &str)] = &[
    ("iphone", "WebGPU adapter on the main thread", NO_WEBGPU),
    (
        "iphone",
        "WebGPU renders from a worker to an OffscreenCanvas",
        NO_WEBGPU,
    ),
    (
        "iphone",
        "WebGPU renderer runs in the engine worker",
        NO_WEBGPU,
    ),
    ("iphone", "empty draw list renders black", NO_WEBGPU),
    (
        "iphone",
        "draws an uncompressed game texture exactly",
        NO_WEBGPU,
    ),
    ("iphone", "draws a BC game texture as stored", NO_WEBGPU),
    ("iphone", "plays a game video as a texture", NO_WEBGPU),
];

const NO_WEBGPU: &str = "iOS Simulator has no WebGPU adapter; check on a real device";

/// Turns `fail` lines of tests in [`KNOWN_FAILURES`] for `target` into
/// `skip` lines carrying the known reason.
fn known_failures(target: &str, report: &str) -> String {
    report
        .lines()
        .map(|line| {
            let known = line.strip_prefix("fail ").and_then(|rest| {
                let name = rest.split('\t').next()?;
                KNOWN_FAILURES
                    .iter()
                    .find(|(t, n, _)| *t == target && *n == name)
                    .map(|(_, n, reason)| format!("skip {n}\t{reason}"))
            });
            known.unwrap_or_else(|| line.to_owned())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn passed(report: &str) -> bool {
    let mut lines = report.lines();
    lines.clone().any(|l| l.starts_with("ok ")) && !lines.any(|l| l.starts_with("fail "))
}

fn chrome(url: &str, id: &str, reports: &Receiver<String>, timeout: Duration) -> Result<String> {
    let profile = env::temp_dir().join(format!("volya-chrome-{id}"));
    let mut child: Child = cmd(
        CHROME,
        &[
            "--headless=new",
            "--no-first-run",
            "--no-default-browser-check",
            // The page's AudioContext runs without a gesture (synthesized
            // input is not one), so suites see it suspended and resumed.
            "--autoplay-policy=no-user-gesture-required",
            &format!("--user-data-dir={}", profile.display()),
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
    let _ = fs::remove_dir_all(profile);
    report
}

/// Opens `url` in Mobile Safari on a dedicated iPhone 17 Pro Max simulator,
/// creating and booting it as needed. The simulator is left booted so later
/// runs start fast.
fn iphone(url: &str, id: &str, reports: &Receiver<String>, timeout: Duration) -> Result<String> {
    let udid = simulator()?;
    run_cmd(cmd("xcrun", &["simctl", "bootstatus", &udid, "-b"]).stdout(Stdio::null()))?;
    run_cmd(&mut cmd("xcrun", &["simctl", "openurl", &udid, url]))?;
    wait_report(id, reports, timeout)
}

/// Runs `steps` (one per argument; see tools/simdriver) in the test
/// simulator with real touches, through an XCUITest that drives Safari.
pub fn sim(steps: &[&str]) -> Result {
    let udid = simulator()?;
    run_cmd(cmd("xcrun", &["simctl", "bootstatus", &udid, "-b"]).stdout(Stdio::null()))?;
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
    fn known_failures_become_skips() {
        let report =
            "run x\nfail WebGPU adapter on the main thread\tno WebGPU adapter\nfail other\tboom";
        assert_eq!(
            known_failures("iphone", report),
            "run x\nskip WebGPU adapter on the main thread\tiOS Simulator has no WebGPU adapter; check on a real device\nfail other\tboom"
        );
        assert_eq!(known_failures("chrome", report), report);
    }

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
