// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `xtask frames-receive`: the original's frames of a Windows capture
//! (docs/testing.md, The original on Windows) received from the PC over the
//! local network, into the fidelity suite's references,
//! `~/Library/Caches/volya-engine/original-frames/fidelity/` (game assets:
//! they go nowhere else).
//!
//! A one-shot HTTP receiver on the Mac's LAN address and a port the system
//! picks, both printed with a random token: every request carries the token
//! (`X-Volya-Token`). The PC first posts a manifest (`POST /manifest`, lines
//! `<sha256> <path>`), then puts each file (`PUT /file/<path>`); a file is
//! kept only when its path is in the manifest and its bytes match its hash.
//! It ends when every file of the manifest has arrived, or after a timeout,
//! and reports what arrived.
//!
//! `--into NAME` receives into another of a fixed set of cache folders
//! ([`DESTINATIONS`]) instead, for other black-box runs' outputs, which are
//! local-only too (docs/clean-room.md).

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::{Result, step};

/// How long the receiver waits for the whole capture, and for one request.
const TIMEOUT: Duration = Duration::from_secs(3600);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// The largest file accepted (a 1920 × 1080 frame stored uncompressed is
/// about 6 MB).
const MAX_FILE: usize = 64 << 20;

/// The folders a receiver may write into, under
/// `~/Library/Caches/volya-engine/`, by their `--into` names; the first is
/// the default.
pub const DESTINATIONS: &[(&str, &str)] = &[
    ("fidelity", "original-frames/fidelity"),
    // The shipped physics library's results (docs/testing.md, The physics
    // library against the shipped one).
    ("ode-harness", "ode-harness"),
];

/// `xtask frames-receive [--into NAME] [ADDRESS]`: on ADDRESS, one of the
/// Mac's own, or else the one routing to the local network, into the folder
/// NAME names ([`DESTINATIONS`]).
pub fn receive(into: Option<&str>, address: Option<&str>) -> Result {
    let home = std::env::var_os("HOME").ok_or("HOME is unset")?;
    let sub = match into {
        None => DESTINATIONS[0].1,
        Some(name) => DESTINATIONS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, d)| *d)
            .ok_or_else(|| {
                let names: Vec<&str> = DESTINATIONS.iter().map(|(n, _)| *n).collect();
                format!("--into {name}: not one of {names:?}")
            })?,
    };
    let dest = PathBuf::from(home)
        .join("Library/Caches/volya-engine")
        .join(sub);
    fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let address = match address {
        Some(a) => a.parse().map_err(|e| format!("{a}: {e}"))?,
        None => lan_address()?,
    };
    let listener =
        TcpListener::bind((address, 0)).map_err(|e| format!("listen on {address}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token = token()?;
    step(&format!("Receiving files into {}", dest.display()));
    println!("address {address}\nport {port}\ntoken {token}");
    let mut manifest: Option<BTreeMap<String, String>> = None;
    let mut arrived: BTreeMap<String, usize> = BTreeMap::new();
    let start = Instant::now();
    while start.elapsed() < TIMEOUT {
        if let Some(m) = &manifest
            && m.keys().all(|k| arrived.contains_key(k))
        {
            break;
        }
        match listener.accept() {
            Ok((stream, peer)) => {
                if let Err(e) = serve(stream, &token, &dest, &mut manifest, &mut arrived) {
                    println!("{peer}: {e}");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("accept: {e}")),
        }
    }
    let Some(manifest) = manifest else {
        return Err("no manifest arrived".into());
    };
    let missing: Vec<&String> = manifest
        .keys()
        .filter(|k| !arrived.contains_key(*k))
        .collect();
    for (path, size) in &arrived {
        println!("{size:>10}  {path}");
    }
    if missing.is_empty() {
        step(&format!(
            "All {} files arrived, verified, in {}",
            arrived.len(),
            dest.display()
        ));
        Ok(())
    } else {
        Err(format!(
            "{} files did not arrive: {missing:?}",
            missing.len()
        ))
    }
}

/// The address of the interface that routes to the local network: a UDP
/// socket's local address once connected (connecting sends nothing).
fn lan_address() -> Result<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket
        .connect("10.255.255.255:9")
        .map_err(|e| format!("no route to the local network: {e}"))?;
    let ip = socket.local_addr().map_err(|e| e.to_string())?.ip();
    if ip.is_unspecified() || ip.is_loopback() {
        return Err(format!("no LAN address ({ip})"));
    }
    Ok(ip)
}

/// 16 random bytes in hex.
fn token() -> Result<String> {
    let mut bytes = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// A path of the manifest, as it lands below the fidelity folder: one or
/// two parts of letters, digits, `.`, `_` and `-`, none starting with `.`
/// (so no `..`, no hidden file), separated by `/`; nothing absolute.
fn safe_path(path: &str) -> Option<&str> {
    let parts: Vec<&str> = path.split('/').collect();
    let ok = (1..=2).contains(&parts.len())
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.len() <= 128
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        });
    ok.then_some(path)
}

fn manifest_of(text: &str) -> std::result::Result<BTreeMap<String, String>, String> {
    let mut m = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (hash, path) = line
            .split_once(' ')
            .ok_or_else(|| format!("manifest line {line:?}"))?;
        let hash = hash.to_ascii_lowercase();
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("manifest: {hash:?} is not a SHA-256"));
        }
        let path = safe_path(path.trim()).ok_or_else(|| format!("manifest: path {path:?}"))?;
        m.insert(path.to_string(), hash);
    }
    if m.is_empty() {
        return Err("an empty manifest".into());
    }
    Ok(m)
}

/// One request: the manifest, or a file of it.
fn serve(
    stream: TcpStream,
    token: &str,
    dest: &Path,
    manifest: &mut Option<BTreeMap<String, String>>,
    arrived: &mut BTreeMap<String, usize>,
) -> Result {
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(REQUEST_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let mut out = stream.try_clone().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut words = line.split_whitespace();
    let (method, target) = (
        words.next().unwrap_or("").to_string(),
        words.next().unwrap_or("").to_string(),
    );
    let (mut length, mut given, mut expect) = (0usize, String::new(), false);
    loop {
        let mut h = String::new();
        reader.read_line(&mut h).map_err(|e| e.to_string())?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((name, value)) = h.split_once(':') {
            let value = value.trim();
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.parse().unwrap_or(usize::MAX),
                "x-volya-token" => given = value.to_string(),
                "expect" => expect = value.eq_ignore_ascii_case("100-continue"),
                _ => {}
            }
        }
    }
    let reply = |out: &mut TcpStream, code: &str, text: &str| {
        let _ = write!(
            out,
            "HTTP/1.1 {code}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
    };
    if given != token {
        reply(&mut out, "403 Forbidden", "token\n");
        return Err(format!("{method} {target}: wrong token"));
    }
    if length > MAX_FILE {
        reply(&mut out, "413 Payload Too Large", "too large\n");
        return Err(format!("{method} {target}: {length} bytes"));
    }
    if expect {
        let _ = write!(out, "HTTP/1.1 100 Continue\r\n\r\n");
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    match (method.as_str(), target.as_str()) {
        ("POST", "/manifest") => match manifest_of(&String::from_utf8_lossy(&body)) {
            Ok(m) => {
                println!("manifest: {} files", m.len());
                *manifest = Some(m);
                reply(&mut out, "200 OK", "ok\n");
                Ok(())
            }
            Err(e) => {
                reply(&mut out, "400 Bad Request", &format!("{e}\n"));
                Err(e)
            }
        },
        ("PUT", t) if t.starts_with("/file/") => {
            let path = &t["/file/".len()..];
            let Some(want) = manifest.as_ref().and_then(|m| m.get(path)) else {
                reply(&mut out, "400 Bad Request", "not in the manifest\n");
                return Err(format!("{path}: not in the manifest"));
            };
            let got: String = Sha256::digest(&body)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            if &got != want {
                reply(&mut out, "400 Bad Request", "hash differs\n");
                return Err(format!(
                    "{path}: its SHA-256 is {got}, the manifest's {want}"
                ));
            }
            // Written beside, then renamed: the suite never sees half a file.
            let to = dest.join(path);
            let parent = to.parent().ok_or("no folder")?;
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            let part = parent.join(format!(
                ".{}.part",
                to.file_name().unwrap_or_default().to_string_lossy()
            ));
            fs::write(&part, &body).map_err(|e| format!("{}: {e}", part.display()))?;
            fs::rename(&part, &to).map_err(|e| format!("{}: {e}", to.display()))?;
            println!("{path}: {} bytes, verified", body.len());
            arrived.insert(path.to_string(), body.len());
            reply(&mut out, "200 OK", "ok\n");
            Ok(())
        }
        _ => {
            reply(&mut out, "404 Not Found", "no\n");
            Err(format!("{method} {target}: unknown"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_below_the_folder() {
        assert_eq!(safe_path("title-menu.png"), Some("title-menu.png"));
        assert_eq!(safe_path("display/leaves.png"), Some("display/leaves.png"));
        for bad in [
            "../x.png", "/etc/x", "a/../b", ".x", "a\\b", "a/b/c", "", "a//b", "C:x",
        ] {
            assert_eq!(safe_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_manifest() {
        let h = "a".repeat(64);
        let m = manifest_of(&format!(
            "{h} title-menu.png\r\n{} display/x.png\n",
            "B".repeat(64)
        ))
        .unwrap();
        assert_eq!(m["display/x.png"], "b".repeat(64));
        assert!(manifest_of(&format!("{h} ../x")).is_err());
        assert!(manifest_of("abc x.png").is_err());
        assert!(manifest_of("").is_err());
    }
}
