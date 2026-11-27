// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Static file server that mimics GitHub Pages for local development:
//! serves a directory under [`BASE`]. The app's page comes without custom
//! headers, as from GitHub Pages, so only the service worker makes it
//! cross-origin isolated (`static/sw.js`, #354); every other response
//! carries [`ISOLATION_HEADERS`], so the browser test pages (which run
//! without the service worker) are isolated, and the files they load can
//! be.
//!
//! With [`Server::with_tests`] it also serves the browser test harness under
//! [`TEST_BASE`] (kept out of `dist/`) and accepts test reports POSTed to
//! [`RESULTS_PATH`]; with [`Server::with_dumps`] it writes the frames tests
//! POST under [`DUMP_BASE`] to a local directory, and with
//! [`Server::with_references`] serves an earlier run's frames under
//! [`REFERENCE_BASE`] for comparison.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::TcpListener;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// URL prefix of the GitHub Pages project site.
pub const BASE: &str = "/volya-engine/";
/// URL prefix of the browser test harness.
pub const TEST_BASE: &str = "/volya-engine/__test/";
/// Endpoint receiving test reports.
pub const RESULTS_PATH: &str = "/volya-engine/__test/results";
/// Tests POST `on` or `off` under this prefix to make the app's own files
/// unreachable (connections close without a response, like a lost network),
/// to check the service worker's offline copies. Test files stay reachable.
pub const OFFLINE_BASE: &str = "/volya-engine/__test/offline/";
/// With [`Server::with_next`], tests POST `on` or `off` under this prefix to
/// serve the next build instead of the root (a deploy, for the update flow).
pub const NEXT_BASE: &str = "/volya-engine/__test/next/";
/// GitHub Pages' cache lifetime for every file (measured: `max-age=600`).
const PAGES_CACHE_CONTROL: &str = "max-age=600";
/// URL prefix of the game installer served to tests (BYOA, local only), with
/// `index.json` listing its files.
pub const INSTALLER_BASE: &str = "/volya-engine/__test/installer/";

/// Tests POST a frame as PNG to `<DUMP_BASE><suite>/<name>.png`; with
/// [`Server::with_dumps`] it is written to `<dir>/<suite>/<name>.png`. A GET
/// of `DUMP_BASE` answers 204 when dumps are written, else 404, so pages
/// encode frames only when they are kept. A configuration file
/// (`<name>.ltx`) is posted and served the same way.
pub const DUMP_BASE: &str = "/volya-engine/__test/dump/";
/// With [`Server::with_references`], `<REFERENCE_BASE><suite>/<name>.png`
/// serves `<dir>/<suite>/<name>.png`: a frame an earlier run dumped, which
/// a suite compares with its own (404 without one); deeper paths and the
/// kinds [`reference_path`] takes are served likewise. A GET of
/// `REFERENCE_BASE` answers 204 when references are served, else 404.
pub const REFERENCE_BASE: &str = "/volya-engine/__test/reference/";

/// What makes a page cross-origin isolated (COOP and COEP), as the service
/// worker adds them.
pub const ISOLATION_HEADERS: &str =
    "Cross-Origin-Opener-Policy: same-origin\r\nCross-Origin-Embedder-Policy: require-corp\r\n";

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
/// A dumped frame: a PNG of up to 4096 × 4096.
const MAX_DUMP_BYTES: usize = 96 * 1024 * 1024;

/// Receives the body of every test report.
pub type ResultsSink = Arc<dyn Fn(String) + Send + Sync>;

pub struct Server {
    /// URL prefix and directory, most specific first.
    mounts: Vec<(&'static str, PathBuf)>,
    results: Option<ResultsSink>,
    installer: Option<PathBuf>,
    offline: AtomicBool,
    /// The next build's directory, served while `use_next` is set.
    next: Option<PathBuf>,
    use_next: AtomicBool,
    /// The app's files carry GitHub Pages' cache lifetime rather than
    /// `no-store`.
    pages_caching: bool,
    /// Where frames POSTed under [`DUMP_BASE`] go.
    dumps: Mutex<Option<PathBuf>>,
    /// Frames served under [`REFERENCE_BASE`].
    references: Mutex<Option<PathBuf>>,
}

impl Server {
    pub fn new(root: &Path) -> io::Result<Self> {
        Ok(Self {
            mounts: vec![(BASE, root.canonicalize()?)],
            results: None,
            installer: None,
            offline: AtomicBool::new(false),
            next: None,
            use_next: AtomicBool::new(false),
            pages_caching: false,
            dumps: Mutex::new(None),
            references: Mutex::new(None),
        })
    }

    /// Serves `dir` instead of the root while tests ask for it under
    /// [`NEXT_BASE`].
    pub fn with_next(mut self, dir: &Path) -> io::Result<Self> {
        self.next = Some(dir.canonicalize()?);
        Ok(self)
    }

    /// Gives the app's files GitHub Pages' cache lifetime, so browsers may
    /// keep them as they do the deployed app's; test files stay `no-store`.
    pub fn with_pages_caching(mut self) -> Self {
        self.pages_caching = true;
        self
    }

    /// Serves the test harness from `dir` and passes reports to `sink`.
    pub fn with_tests(mut self, dir: &Path, sink: ResultsSink) -> io::Result<Self> {
        self.mounts.insert(0, (TEST_BASE, dir.canonicalize()?));
        self.results = Some(sink);
        Ok(self)
    }

    /// Serves the frames of `dir` (an earlier run's dumps) under
    /// [`REFERENCE_BASE`].
    pub fn with_references(self, dir: &Path) -> io::Result<Self> {
        self.set_references(Some(dir))?;
        Ok(self)
    }

    /// [`Server::with_references`] while the server runs (`None`: none).
    pub fn set_references(&self, dir: Option<&Path>) -> io::Result<()> {
        let dir = dir.map(Path::canonicalize).transpose()?;
        *self.references.lock().unwrap_or_else(|e| e.into_inner()) = dir;
        Ok(())
    }

    /// [`Server::with_dumps`] while the server runs (`None`: none).
    pub fn set_dumps(&self, dir: Option<&Path>) -> io::Result<()> {
        let dir = match dir {
            Some(dir) => {
                std::fs::create_dir_all(dir)?;
                Some(dir.canonicalize()?)
            }
            None => None,
        };
        *self.dumps.lock().unwrap_or_else(|e| e.into_inner()) = dir;
        Ok(())
    }

    /// Serves the next build (see [`Server::with_next`]) instead of the
    /// root, or the root again.
    pub fn set_next(&self, on: bool) {
        self.use_next.store(on, Ordering::SeqCst);
    }

    /// Writes the frames tests POST under [`DUMP_BASE`] into `dir` (created
    /// when missing); never anything else.
    pub fn with_dumps(self, dir: &Path) -> io::Result<Self> {
        self.set_dumps(Some(dir))?;
        Ok(self)
    }

    /// Serves the files of `dir` (a GOG installer) to tests under
    /// [`INSTALLER_BASE`]; never deployed.
    pub fn with_installer(mut self, dir: &Path) -> io::Result<Self> {
        self.installer = Some(dir.canonicalize()?);
        Ok(self)
    }

    /// Accepts connections forever, one thread per connection.
    pub fn run(self, listener: TcpListener) -> io::Result<()> {
        Arc::new(self).run_shared(listener)
    }

    /// [`Server::run`], with the server shared, so its settings can change
    /// while it runs.
    pub fn run_shared(self: Arc<Self>, listener: TcpListener) -> io::Result<()> {
        let server = self;
        for stream in listener.incoming() {
            let stream = stream?;
            let server = Arc::clone(&server);
            thread::spawn(move || {
                if let Err(error) = server.respond(stream) {
                    eprintln!("connection error: {error}");
                }
            });
        }
        Ok(())
    }

    fn respond<S: Read + Write>(&self, mut stream: S) -> io::Result<()> {
        let request = match read_request(&mut stream) {
            Ok(request) => request,
            Err(_) => return write_status(&mut stream, 400, "Bad Request"),
        };
        match (request.method.as_str(), &self.results) {
            ("POST", Some(_)) if request.path.starts_with(DUMP_BASE) => {
                let dumps = self.dumps.lock().unwrap_or_else(|e| e.into_inner()).clone();
                return match (&dumps, dump_path(&request.path[DUMP_BASE.len()..])) {
                    (Some(dir), Some((suite, name))) => {
                        let dir = dir.join(suite);
                        std::fs::create_dir_all(&dir)?;
                        std::fs::write(dir.join(name), &request.body)?;
                        write_status(&mut stream, 204, "No Content")
                    }
                    (Some(_), None) => write_status(&mut stream, 400, "Bad Request"),
                    (None, _) => write_status(&mut stream, 404, "Not Found"),
                };
            }
            ("GET", Some(_)) if request.path == REFERENCE_BASE => {
                return match *self.references.lock().unwrap_or_else(|e| e.into_inner()) {
                    Some(_) => write_status(&mut stream, 204, "No Content"),
                    None => write_status(&mut stream, 404, "Not Found"),
                };
            }
            ("GET", Some(_)) if request.path.starts_with(REFERENCE_BASE) => {
                let references = self
                    .references
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let found = references.as_ref().and_then(|dir| {
                    let path = reference_path(&request.path[REFERENCE_BASE.len()..])?;
                    Some(dir.join(path)).filter(|p| p.is_file())
                });
                return match found {
                    Some(path) => serve_file(&mut stream, &path, None, false, true, "no-store"),
                    None => write_status(&mut stream, 404, "Not Found"),
                };
            }
            ("GET", Some(_)) if request.path == DUMP_BASE => {
                return match *self.dumps.lock().unwrap_or_else(|e| e.into_inner()) {
                    Some(_) => write_status(&mut stream, 204, "No Content"),
                    None => write_status(&mut stream, 404, "Not Found"),
                };
            }
            ("POST", Some(sink)) if request.path == RESULTS_PATH => {
                sink(String::from_utf8_lossy(&request.body).into_owned());
                return write_status(&mut stream, 204, "No Content");
            }
            ("POST", Some(_)) if request.path.starts_with(OFFLINE_BASE) => {
                let on = match &request.path[OFFLINE_BASE.len()..] {
                    "on" => true,
                    "off" => false,
                    _ => return write_status(&mut stream, 404, "Not Found"),
                };
                self.offline.store(on, Ordering::SeqCst);
                return write_status(&mut stream, 204, "No Content");
            }
            ("POST", Some(_)) if self.next.is_some() && request.path.starts_with(NEXT_BASE) => {
                let on = match &request.path[NEXT_BASE.len()..] {
                    "on" => true,
                    "off" => false,
                    _ => return write_status(&mut stream, 404, "Not Found"),
                };
                self.use_next.store(on, Ordering::SeqCst);
                return write_status(&mut stream, 204, "No Content");
            }
            ("GET" | "HEAD", _) => {}
            _ => return write_status(&mut stream, 405, "Method Not Allowed"),
        }
        let head_only = request.method == "HEAD";
        if let Some(dir) = &self.installer
            && let Some(name) = request.path.strip_prefix(INSTALLER_BASE)
        {
            return match name {
                "index.json" => {
                    let body = installer_index(dir)?;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )?;
                    stream.flush()
                }
                _ => match percent_decode(name) {
                    Some(name) if !name.contains(['/', '\\']) && dir.join(&name).is_file() => {
                        serve_file(
                            &mut stream,
                            &dir.join(name),
                            request.range,
                            head_only,
                            true,
                            "no-store",
                        )
                    }
                    _ => write_status(&mut stream, 404, "Not Found"),
                },
            };
        }
        if self.offline.load(Ordering::SeqCst) && !request.path.starts_with(TEST_BASE) {
            // No response at all: the browser sees a network error.
            return Ok(());
        }
        match self.route(&request.path) {
            Route::Redirect => write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: {BASE}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
            Route::NotFound => write_status(&mut stream, 404, "Not Found"),
            Route::File(path) => {
                // The app's page: as GitHub Pages serves it.
                let page = request.path == BASE || request.path == format!("{BASE}index.html");
                let cache = if self.pages_caching && !request.path.starts_with(TEST_BASE) {
                    PAGES_CACHE_CONTROL
                } else {
                    "no-store"
                };
                serve_file(&mut stream, &path, request.range, head_only, !page, cache)
            }
        }
    }

    fn route(&self, url_path: &str) -> Route {
        if url_path == "/" || url_path == BASE.trim_end_matches('/') {
            return Route::Redirect;
        }
        let Some((prefix, mut rest, mut dir)) = self
            .mounts
            .iter()
            .find_map(|(prefix, dir)| Some((*prefix, url_path.strip_prefix(prefix)?, dir)))
        else {
            return Route::NotFound;
        };
        if prefix == BASE
            && self.use_next.load(Ordering::SeqCst)
            && let Some(next) = &self.next
        {
            dir = next;
        }
        // `__test/run/<id>/<file>` serves `<file>`; the id only tags reports.
        if prefix == TEST_BASE
            && let Some(run) = rest.strip_prefix("run/")
        {
            match run.split_once('/') {
                Some((_, file)) => rest = file,
                None => return Route::NotFound,
            }
        }
        let Some(relative) = percent_decode(rest) else {
            return Route::NotFound;
        };
        let relative = Path::new(&relative);
        if !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return Route::NotFound;
        }
        let mut path = dir.join(relative);
        if path.is_dir() {
            path.push("index.html");
        }
        // Reject anything that escapes the mount, e.g. via symlinks.
        match path.canonicalize() {
            Ok(real) if real.starts_with(dir) && real.is_file() => Route::File(real),
            _ => Route::NotFound,
        }
    }
}

#[derive(Debug, PartialEq)]
enum Route {
    Redirect,
    NotFound,
    File(PathBuf),
}

/// A reference's path below the references' directory: `<suite>/` and one
/// or more names, each a [`plain_name`], the last a frame, a configuration
/// file, or the Direct3D 9 probes' `.bin` log and `.json` index.
fn reference_path(rest: &str) -> Option<&str> {
    let names: Vec<&str> = rest.split('/').collect();
    let kind = [".png", ".ltx", ".bin", ".json"]
        .iter()
        .any(|k| rest.ends_with(k));
    (names.len() >= 2 && names.iter().all(|n| plain_name(n)) && kind).then_some(rest)
}

/// ASCII letters, digits, `-`, `_` and `.` (not leading).
fn plain_name(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// `<suite>/<name>.png` (a frame) or `<suite>/<name>.ltx` (a configuration
/// file, the fidelity suite's), each of [`plain_name`]'s characters, as the
/// suite's directory and the file's name.
fn dump_path(rest: &str) -> Option<(&str, &str)> {
    let (suite, name) = rest.split_once('/')?;
    let kind = name.ends_with(".png") || name.ends_with(".ltx");
    (plain_name(suite) && plain_name(name) && kind).then_some((suite, name))
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
    /// `Range: bytes=start-[end]` (inclusive end).
    range: Option<(u64, Option<u64>)>,
}

/// Serves `path`, honoring a single byte range, with
/// [`ISOLATION_HEADERS`] when `isolated`, and `cache` as its Cache-Control.
fn serve_file(
    stream: &mut impl Write,
    path: &Path,
    range: Option<(u64, Option<u64>)>,
    head_only: bool,
    isolated: bool,
    cache: &str,
) -> io::Result<()> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let kind = content_type(path);
    let (status, start, len) = match range {
        Some((start, end)) if start < size => {
            let end = end.unwrap_or(size - 1).min(size - 1);
            if end < start {
                return write_status(stream, 416, "Range Not Satisfiable");
            }
            write!(
                stream,
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{size}\r\n"
            )?;
            ("", start, end - start + 1)
        }
        Some(_) => return write_status(stream, 416, "Range Not Satisfiable"),
        None => ("HTTP/1.1 200 OK\r\n", 0, size),
    };
    write!(
        stream,
        "{status}Content-Type: {kind}\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\nCache-Control: {cache}\r\n{}Connection: close\r\n\r\n",
        if isolated { ISOLATION_HEADERS } else { "" }
    )?;
    if !head_only {
        file.seek(SeekFrom::Start(start))?;
        io::copy(&mut file.take(len), stream)?;
    }
    stream.flush()
}

/// JSON list of the installer directory's files: `[{"name", "size"}]`.
fn installer_index(dir: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if let (true, Some(name)) = (meta.is_file(), entry.file_name().to_str()) {
            let name = name.replace('\\', "\\\\").replace('"', "\\\"");
            files.push(format!(r#"{{"name":"{name}","size":{}}}"#, meta.len()));
        }
    }
    files.sort();
    Ok(format!("[{}]", files.join(",")))
}

fn read_request(stream: &mut impl Read) -> io::Result<Request> {
    let invalid = || io::Error::from(io::ErrorKind::InvalidData);
    let mut reader = BufReader::new(stream);
    let mut header_bytes = 0;
    let mut next_line = |reader: &mut BufReader<_>, line: &mut String| -> io::Result<()> {
        line.clear();
        let n = reader
            .by_ref()
            .take((MAX_HEADER_BYTES - header_bytes) as u64)
            .read_line(line)?;
        header_bytes += n;
        if n == 0 || !line.ends_with('\n') {
            return Err(invalid());
        }
        Ok(())
    };

    let mut line = String::new();
    next_line(&mut reader, &mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(invalid());
    };
    let method = method.to_owned();
    let path = target
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_owned();

    let mut content_length = 0;
    let mut range = None;
    loop {
        next_line(&mut reader, &mut line)?;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().map_err(|_| invalid())?;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("range")
            && let Some((start, end)) = value
                .trim()
                .strip_prefix("bytes=")
                .and_then(|r| r.split_once('-'))
        {
            let start = start.parse().map_err(|_| invalid())?;
            let end = if end.is_empty() {
                None
            } else {
                Some(end.parse().map_err(|_| invalid())?)
            };
            range = Some((start, end));
        }
    }
    let limit = if path.starts_with(DUMP_BASE) {
        MAX_DUMP_BYTES
    } else {
        MAX_BODY_BYTES
    };
    if content_length > limit {
        return Err(invalid());
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    Ok(Request {
        method,
        path,
        body,
        range,
    })
}

fn write_status(stream: &mut impl Write, code: u16, reason: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::net::TcpStream;
    use std::sync::mpsc;

    fn fixture(name: &str) -> (PathBuf, Server) {
        let dir = std::env::temp_dir().join(format!(
            "volya-engine-devserver-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("root/pkg")).unwrap();
        fs::create_dir_all(dir.join("tests")).unwrap();
        fs::write(dir.join("root/index.html"), "<p>hi</p>").unwrap();
        fs::write(dir.join("root/pkg/a b.wasm"), [0, 97, 115, 109]).unwrap();
        fs::write(dir.join("tests/harness.js"), "// harness").unwrap();
        fs::write(dir.join("secret.txt"), "nope").unwrap();
        let server = Server::new(&dir.join("root")).unwrap();
        (dir, server)
    }

    fn exchange(server: Server, request: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || server.run(listener));
        let mut stream = TcpStream::connect(addr).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn routes() {
        let (dir, server) = fixture("routes");
        let server = server
            .with_tests(&dir.join("tests"), Arc::new(|_| {}))
            .unwrap();
        let root = dir.join("root").canonicalize().unwrap();
        let tests = dir.join("tests").canonicalize().unwrap();
        assert_eq!(server.route("/"), Route::Redirect);
        assert_eq!(server.route("/volya-engine"), Route::Redirect);
        assert_eq!(server.route(BASE), Route::File(root.join("index.html")));
        assert_eq!(
            server.route("/volya-engine/pkg/a%20b.wasm"),
            Route::File(root.join("pkg/a b.wasm"))
        );
        assert_eq!(
            server.route("/volya-engine/__test/harness.js"),
            Route::File(tests.join("harness.js"))
        );
        assert_eq!(
            server.route("/volya-engine/__test/run/abc123/harness.js"),
            Route::File(tests.join("harness.js"))
        );
        assert_eq!(
            server.route("/volya-engine/__test/run/abc123"),
            Route::NotFound
        );
        assert_eq!(server.route("/other/index.html"), Route::NotFound);
        assert_eq!(server.route("/volya-engine/missing"), Route::NotFound);
        assert_eq!(server.route("/volya-engine/../secret.txt"), Route::NotFound);
        assert_eq!(
            server.route("/volya-engine/%2e%2e/secret.txt"),
            Route::NotFound
        );
        assert_eq!(
            server.route("/volya-engine/__test/../secret.txt"),
            Route::NotFound
        );
        assert_eq!(server.route("/volya-engine//etc/passwd"), Route::NotFound);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn serves_the_page_without_isolation_headers_and_the_rest_with() {
        let (dir, server) = fixture("http");
        let response = exchange(server, "GET /volya-engine/ HTTP/1.1\r\nHost: x\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(response.contains("Content-Type: text/html; charset=utf-8\r\n"));
        assert!(response.ends_with("<p>hi</p>"));
        assert!(!response.contains("Cross-Origin"));
        let (dir2, server) = fixture("http-isolated");
        let response = exchange(
            server,
            "GET /volya-engine/pkg/a%20b.wasm HTTP/1.1\r\nHost: x\r\n\r\n",
        );
        assert!(response.contains(ISOLATION_HEADERS), "{response}");
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(dir2).unwrap();
    }

    #[test]
    fn accepts_test_results() {
        let (dir, server) = fixture("results");
        let (tx, rx) = mpsc::channel();
        let server = server
            .with_tests(
                &dir.join("tests"),
                Arc::new(move |body| tx.send(body).unwrap()),
            )
            .unwrap();
        let body = "run 1\nok smoke";
        let response = exchange(
            server,
            &format!(
                "POST {RESULTS_PATH} HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        );
        assert!(response.starts_with("HTTP/1.1 204 "), "{response}");
        assert_eq!(rx.recv().unwrap(), body);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn writes_dumped_frames_only_where_asked() {
        let (dir, server) = fixture("dumps");
        let dumps = dir.join("dumps");
        let server = server
            .with_tests(&dir.join("tests"), Arc::new(|_| {}))
            .unwrap()
            .with_dumps(&dumps)
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || server.run(listener));
        let request = |head: &str| {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream.write_all(head.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        };
        let post = |path: &str| {
            request(&format!(
                "POST {DUMP_BASE}{path} HTTP/1.1\r\nContent-Length: 3\r\n\r\npng"
            ))
        };
        assert!(request(&format!("GET {DUMP_BASE} HTTP/1.1\r\n\r\n")).starts_with("HTTP/1.1 204 "));
        assert!(post("render/a-frame_1.png").starts_with("HTTP/1.1 204 "));
        assert_eq!(
            fs::read(dumps.join("render/a-frame_1.png")).unwrap(),
            b"png"
        );
        for bad in [
            "..%2Fx.png",
            "render/../x.png",
            "render/.x.png",
            "render/x.txt",
            "x.png",
        ] {
            assert!(post(bad).starts_with("HTTP/1.1 400 "), "{bad}");
        }
        let (dir3, server) = fixture("references");
        let server = server
            .with_tests(&dir3.join("tests"), Arc::new(|_| {}))
            .unwrap()
            .with_references(&dumps)
            .unwrap();
        let found = exchange(
            server,
            &format!("GET {REFERENCE_BASE}render/a-frame_1.png HTTP/1.1\r\n\r\n"),
        );
        assert!(
            found.starts_with("HTTP/1.1 200 ") && found.ends_with("png"),
            "{found}"
        );
        fs::create_dir_all(dumps.join("d3d9probes/rtx-4090")).unwrap();
        fs::write(dumps.join("d3d9probes/rtx-4090/scenes.bin"), b"VD9P").unwrap();
        let get = |path: &str| {
            let (dir, server) = fixture("nested-reference");
            let server = server
                .with_tests(&dir.join("tests"), Arc::new(|_| {}))
                .unwrap()
                .with_references(&dumps)
                .unwrap();
            let response = exchange(
                server,
                &format!("GET {REFERENCE_BASE}{path} HTTP/1.1\r\n\r\n"),
            );
            fs::remove_dir_all(dir).unwrap();
            response
        };
        let nested = get("d3d9probes/rtx-4090/scenes.bin");
        assert!(
            nested.starts_with("HTTP/1.1 200 ") && nested.ends_with("VD9P"),
            "{nested}"
        );
        for bad in [
            "d3d9probes/../render/a-frame_1.png",
            "d3d9probes/rtx-4090/.x.bin",
            "scenes.bin",
        ] {
            assert!(get(bad).starts_with("HTTP/1.1 404 "), "{bad}");
        }
        let (dir4, server) = fixture("no-reference");
        let server = server
            .with_tests(&dir4.join("tests"), Arc::new(|_| {}))
            .unwrap()
            .with_references(&dumps)
            .unwrap();
        let missing = exchange(
            server,
            &format!("GET {REFERENCE_BASE}render/other.png HTTP/1.1\r\n\r\n"),
        );
        assert!(missing.starts_with("HTTP/1.1 404 "), "{missing}");
        fs::remove_dir_all(dir3).unwrap();
        fs::remove_dir_all(dir4).unwrap();
        let (dir2, server) = fixture("no-dumps");
        let server = server
            .with_tests(&dir2.join("tests"), Arc::new(|_| {}))
            .unwrap();
        let response = exchange(server, &format!("GET {DUMP_BASE} HTTP/1.1\r\n\r\n"));
        assert!(response.starts_with("HTTP/1.1 404 "), "{response}");
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(dir2).unwrap();
    }

    #[test]
    fn serves_installer_ranges_and_index() {
        let (dir, server) = fixture("installer");
        fs::create_dir_all(dir.join("installer")).unwrap();
        fs::write(dir.join("installer/setup.bin"), b"0123456789").unwrap();
        let installer = dir.join("installer");
        let request = |head: &str| {
            let server = Server::new(&dir.join("root"))
                .unwrap()
                .with_installer(&installer)
                .unwrap();
            exchange(server, head)
        };
        let index = request("GET /volya-engine/__test/installer/index.json HTTP/1.1\r\n\r\n");
        assert!(
            index.ends_with(r#"[{"name":"setup.bin","size":10}]"#),
            "{index}"
        );
        let part = request(
            "GET /volya-engine/__test/installer/setup.bin HTTP/1.1\r\nRange: bytes=2-5\r\n\r\n",
        );
        assert!(part.starts_with("HTTP/1.1 206 "), "{part}");
        assert!(part.contains("Content-Range: bytes 2-5/10\r\n"));
        assert!(part.ends_with("\r\n\r\n2345"));
        let missing =
            request("GET /volya-engine/__test/installer/..%2Fsecret.txt HTTP/1.1\r\n\r\n");
        assert!(missing.starts_with("HTTP/1.1 404 "), "{missing}");
        drop(server);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn offline_switch_cuts_only_app_files() {
        let (dir, server) = fixture("offline");
        let server = server
            .with_tests(&dir.join("tests"), Arc::new(|_| {}))
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || server.run(listener));
        let request = |head: &str| {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream.write_all(head.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        };
        let post = |what: &str| {
            request(&format!(
                "POST {OFFLINE_BASE}{what} HTTP/1.1\r\nContent-Length: 0\r\n\r\n"
            ))
        };
        let app = "GET /volya-engine/ HTTP/1.1\r\n\r\n";
        let test = "GET /volya-engine/__test/harness.js HTTP/1.1\r\n\r\n";
        assert!(request(app).starts_with("HTTP/1.1 200 "));
        assert!(post("on").starts_with("HTTP/1.1 204 "));
        assert_eq!(request(app), "", "app files unreachable");
        assert!(
            request(test).starts_with("HTTP/1.1 200 "),
            "tests reachable"
        );
        assert!(post("off").starts_with("HTTP/1.1 204 "));
        assert!(request(app).starts_with("HTTP/1.1 200 "));
        assert!(post("sideways").starts_with("HTTP/1.1 404 "));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_posts_without_test_mode() {
        let (dir, server) = fixture("no-tests");
        let response = exchange(
            server,
            &format!("POST {RESULTS_PATH} HTTP/1.1\r\nContent-Length: 0\r\n\r\n"),
        );
        assert!(response.starts_with("HTTP/1.1 405 "), "{response}");
        fs::remove_dir_all(dir).unwrap();
    }
}
