// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Sharing this machine between agents (AGENTS.md): wait for normal memory
//! pressure before heavy work, and hold the iOS Simulator one run at a time.

use std::{fs, io::ErrorKind, path::PathBuf, process::Command, thread, time::Duration};

use crate::Result;

/// The least system-wide free memory, in percent, at which heavy work starts.
const MIN_FREE_PERCENT: u32 = 30;

/// macOS's system-wide free memory percentage, from `memory_pressure`.
fn free_percent() -> Option<u32> {
    let out = Command::new("memory_pressure").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|l| l.contains("System-wide memory free percentage"))?;
    line.rsplit(':')
        .next()?
        .trim()
        .trim_end_matches('%')
        .parse()
        .ok()
}

/// Returns once memory pressure is normal (or when it can't be read, e.g.
/// not on macOS), printing while it waits.
pub fn wait_memory() -> Result {
    let mut told = false;
    loop {
        match free_percent() {
            Some(free) if free < MIN_FREE_PERCENT => {
                if !told {
                    println!("==> Waiting for memory: {free}% free, need {MIN_FREE_PERCENT}%");
                    told = true;
                }
                thread::sleep(Duration::from_secs(10));
            }
            _ => return Ok(()),
        }
    }
}

/// The iOS Simulator held by this process until dropped: a lock directory
/// with the holder's PID; a lock whose holder is gone is taken over.
pub struct SimulatorLock(PathBuf);

impl SimulatorLock {
    pub fn acquire() -> Result<Self> {
        let dir = std::env::temp_dir().join("volya-engine-simulator.lock");
        let mut told = false;
        loop {
            match fs::create_dir(&dir) {
                Ok(()) => {
                    fs::write(dir.join("pid"), std::process::id().to_string())
                        .map_err(|e| format!("{}: {e}", dir.display()))?;
                    return Ok(Self(dir));
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    let holder = fs::read_to_string(dir.join("pid")).unwrap_or_default();
                    let alive = holder.trim().parse::<u32>().is_ok_and(|pid| {
                        Command::new("kill")
                            .args(["-0", &pid.to_string()])
                            .status()
                            .is_ok_and(|s| s.success())
                    });
                    if !alive && !holder.is_empty() {
                        let _ = fs::remove_dir_all(&dir);
                        continue;
                    }
                    if !told {
                        println!(
                            "==> Waiting for the iOS Simulator (held by pid {})",
                            holder.trim()
                        );
                        told = true;
                    }
                    thread::sleep(Duration::from_secs(10));
                }
                Err(e) => return Err(format!("{}: {e}", dir.display())),
            }
        }
    }
}

impl Drop for SimulatorLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
