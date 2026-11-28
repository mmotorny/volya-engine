// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `xtask frames-receive`: the original's outputs of a Windows run
//! (docs/testing.md, The original on Windows) received from the PC over the
//! local network into one folder of `~/Library/Caches/volya-engine/`
//! ([`Dest`]): the fidelity suite's frames, the audio-fidelity suite's
//! captured audio, the physics harness's results, the Direct3D 9 probes'
//! results, or a capture run's text results (game assets and black-box
//! outputs: they go nowhere else).
//!
//! A one-shot HTTP receiver on the Mac's LAN address and a port the system
//! picks, both printed with a random token: every request carries the token
//! (`X-Volya-Token`). The PC first posts a manifest (`POST /manifest`, lines
//! `<sha256> <path>`), then puts each file (`PUT /file/<path>`); a file is
//! kept only when its path is in the manifest and its bytes match its hash.
//! It ends when every file of the manifest has arrived, or after a timeout,
//! and reports what arrived.

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
/// about 6 MB; a walk's audio, 16-bit stereo at 48 kHz, about 12 MB a
/// minute).
const MAX_FILE: usize = 64 << 20;

/// Where a transfer lands, below `~/Library/Caches/volya-engine/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dest {
    /// `fidelity` (the default): the fidelity suite's references
    /// (`//tests/fidelity:frames`).
    Fidelity,
    /// `audio`: the audio-fidelity suite's (`//tests/fidelity:audio`).
    Audio,
    /// `ode-harness`: the physics harness's results.
    OdeHarness,
    /// `d3d9probes`: the Direct3D 9 probes' results of one adapter,
    /// `<gpu>/results/<probe>/<case>.png` and the rest.
    D3d9Probes,
    /// `results`: a capture run's text results, `<gpu>/<file>.txt` and
    /// `<gpu>/<mode>-<n>/<file>` (log, events, hashes, frame clock log,
    /// drrun's output); text only.
    Results,
}

impl Dest {
    /// The `--into` value's destination; `None` for any other value.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "fidelity" => Some(Self::Fidelity),
            "audio" => Some(Self::Audio),
            "ode-harness" => Some(Self::OdeHarness),
            "d3d9probes" => Some(Self::D3d9Probes),
            "results" => Some(Self::Results),
            _ => None,
        }
    }

    /// The folder below the cache.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Fidelity => "original-frames/fidelity",
            Self::Audio => "original-audio/audio-fidelity",
            Self::OdeHarness => "ode-harness",
            Self::D3d9Probes => "d3d9probes",
            Self::Results => "wincapture-results",
        }
    }

    /// The most parts a path below the folder has.
    fn depth(self) -> usize {
        match self {
            Self::Fidelity | Self::Audio | Self::OdeHarness => 2,
            Self::Results => 3,
            Self::D3d9Probes => 4,
        }
    }

    /// The extensions a file may have; `None` for any.
    fn extensions(self) -> Option<&'static [&'static str]> {
        match self {
            Self::Results => Some(&["txt", "log"]),
            _ => None,
        }
    }
}

/// `xtask frames-receive [--into DEST] [ADDRESS]`: into `into`'s folder,
/// on ADDRESS, one of the Mac's own, or else the one routing to the local
/// network.
pub fn receive(into: Dest, address: Option<&str>) -> Result {
    let home = std::env::var_os("HOME").ok_or("HOME is unset")?;
    let dest = PathBuf::from(home)
        .join("Library/Caches/volya-engine")
        .join(into.folder());
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
    step(&format!(
        "Receiving a capture's files into {}",
        dest.display()
    ));
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
                if let Err(e) = serve(stream, &token, into, &dest, &mut manifest, &mut arrived) {
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

/// A path of the manifest, as it lands below `dest`'s folder: one to
/// [`Dest::depth`] parts of letters, digits, `.`, `_` and `-`, none starting
/// with `.` (so no `..`, no hidden file), separated by `/`; nothing
/// absolute; the file's extension one of [`Dest::extensions`].
fn safe_path(dest: Dest, path: &str) -> Option<&str> {
    let parts: Vec<&str> = path.split('/').collect();
    let extension_ok = dest.extensions().is_none_or(|allowed| {
        parts
            .last()
            .and_then(|name| name.rsplit_once('.'))
            .is_some_and(|(_, ext)| allowed.contains(&ext))
    });
    let ok = extension_ok
        && (1..=dest.depth()).contains(&parts.len())
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.len() <= 128
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        });
    ok.then_some(path)
}

fn manifest_of(dest: Dest, text: &str) -> std::result::Result<BTreeMap<String, String>, String> {
    let mut m = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (hash, path) = line
            .split_once(' ')
            .ok_or_else(|| format!("manifest line {line:?}"))?;
        let hash = hash.to_ascii_lowercase();
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("manifest: {hash:?} is not a SHA-256"));
        }
        let path =
            safe_path(dest, path.trim()).ok_or_else(|| format!("manifest: path {path:?}"))?;
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
    into: Dest,
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
        ("POST", "/manifest") => match manifest_of(into, &String::from_utf8_lossy(&body)) {
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
    fn destinations() {
        assert_eq!(Dest::parse("fidelity"), Some(Dest::Fidelity));
        assert_eq!(
            Dest::parse("audio").map(Dest::folder),
            Some("original-audio/audio-fidelity")
        );
        assert_eq!(
            Dest::parse("ode-harness").map(Dest::folder),
            Some("ode-harness")
        );
        assert_eq!(Dest::parse("frames"), None);
    }

    #[test]
    fn paths_stay_below_the_folder() {
        let f = Dest::Fidelity;
        assert_eq!(safe_path(f, "title-menu.png"), Some("title-menu.png"));
        assert_eq!(
            safe_path(f, "display/leaves.png"),
            Some("display/leaves.png")
        );
        for bad in [
            "../x.png", "/etc/x", "a/../b", ".x", "a\\b", "a/b/c", "", "a//b", "C:x",
        ] {
            assert_eq!(safe_path(f, bad), None, "{bad}");
        }
    }

    #[test]
    fn a_runs_text_results() {
        let r = Dest::Results;
        assert_eq!(
            Dest::parse("results").map(Dest::folder),
            Some("wincapture-results")
        );
        for good in [
            "rtx-4090/release.txt",
            "rtx-4090/full-summary.txt",
            "rtx-4090/full-1/log.txt",
            "rtx-4090/full-1/frameclock.log",
            "rtx-4090/check-2/drrun-output.txt",
        ] {
            assert_eq!(safe_path(r, good), Some(good), "{good}");
        }
        for bad in [
            "rtx-4090/full-1/fidelity/title-menu.png",
            "rtx-4090/full-1/title-menu.png",
            "rtx-4090/full-1/title-menu.user.ltx",
            "rtx-4090/full-1/audio.raw",
            "rtx-4090/txt",
            "rtx-4090/.txt",
            "rtx-4090/../x.txt",
            "/rtx-4090/x.txt",
            "a/b/c/d.txt",
        ] {
            assert_eq!(safe_path(r, bad), None, "{bad}");
        }
        let h = "a".repeat(64);
        assert!(manifest_of(r, &format!("{h} rtx-4090/full-1/events.txt")).is_ok());
        assert!(manifest_of(r, &format!("{h} rtx-4090/full-1/x.png")).is_err());
    }

    #[test]
    fn the_d3d9_probes_results() {
        let d = Dest::D3d9Probes;
        assert_eq!(
            safe_path(d, "rtx-4090/results/fog/linear.png"),
            Some("rtx-4090/results/fog/linear.png")
        );
        assert_eq!(safe_path(d, "rtx-4090/log.txt"), Some("rtx-4090/log.txt"));
        assert_eq!(safe_path(d, "a/b/c/d/e.png"), None);
    }

    #[test]
    fn a_manifest() {
        let h = "a".repeat(64);
        let f = Dest::Fidelity;
        let m = manifest_of(
            f,
            &format!("{h} title-menu.png\r\n{} display/x.png\n", "B".repeat(64)),
        )
        .unwrap();
        assert_eq!(m["display/x.png"], "b".repeat(64));
        assert!(manifest_of(f, &format!("{h} ../x")).is_err());
        assert!(manifest_of(f, "abc x.png").is_err());
        assert!(manifest_of(f, "").is_err());
    }
}
