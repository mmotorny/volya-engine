// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Static file server that mimics GitHub Pages for local development:
//! serves a directory under [`BASE`], sends no custom headers (so no
//! cross-origin isolation), and optionally speaks HTTPS for real devices.
//!
//! With [`Server::with_tests`] it also serves the browser test harness under
//! [`TEST_BASE`] (kept out of `dist/`) and accepts test reports POSTed to
//! [`RESULTS_PATH`].

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::thread;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// URL prefix of the GitHub Pages project site.
pub const BASE: &str = "/volya-engine/";
/// URL prefix of the browser test harness.
pub const TEST_BASE: &str = "/volya-engine/__test/";
/// Endpoint receiving test reports.
pub const RESULTS_PATH: &str = "/volya-engine/__test/results";

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Receives the body of every test report.
pub type ResultsSink = Arc<dyn Fn(String) + Send + Sync>;

pub struct Server {
    /// URL prefix and directory, most specific first.
    mounts: Vec<(&'static str, PathBuf)>,
    results: Option<ResultsSink>,
    tls: Option<Arc<rustls::ServerConfig>>,
}

impl Server {
    pub fn new(root: &Path) -> io::Result<Self> {
        Ok(Self {
            mounts: vec![(BASE, root.canonicalize()?)],
            results: None,
            tls: None,
        })
    }

    /// Serves the test harness from `dir` and passes reports to `sink`.
    pub fn with_tests(mut self, dir: &Path, sink: ResultsSink) -> io::Result<Self> {
        self.mounts.insert(0, (TEST_BASE, dir.canonicalize()?));
        self.results = Some(sink);
        Ok(self)
    }

    /// Enables HTTPS with a PEM certificate chain and private key.
    pub fn with_tls(mut self, cert: &Path, key: &Path) -> io::Result<Self> {
        let certs = CertificateDer::pem_file_iter(cert)
            .and_then(|certs| certs.collect::<Result<Vec<_>, _>>())
            .map_err(io::Error::other)?;
        let key = PrivateKeyDer::from_pem_file(key).map_err(io::Error::other)?;
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(io::Error::other)?;
        self.tls = Some(Arc::new(config));
        Ok(self)
    }

    pub fn is_tls(&self) -> bool {
        self.tls.is_some()
    }

    /// Accepts connections forever, one thread per connection.
    pub fn run(self, listener: TcpListener) -> io::Result<()> {
        let server = Arc::new(self);
        for stream in listener.incoming() {
            let stream = stream?;
            let server = Arc::clone(&server);
            thread::spawn(move || {
                if let Err(error) = server.connection(stream) {
                    eprintln!("connection error: {error}");
                }
            });
        }
        Ok(())
    }

    fn connection(&self, stream: TcpStream) -> io::Result<()> {
        match &self.tls {
            None => self.respond(stream),
            Some(config) => {
                let conn =
                    rustls::ServerConnection::new(Arc::clone(config)).map_err(io::Error::other)?;
                let mut tls = rustls::StreamOwned::new(conn, stream);
                self.respond(&mut tls)?;
                tls.conn.send_close_notify();
                tls.flush()
            }
        }
    }

    fn respond<S: Read + Write>(&self, mut stream: S) -> io::Result<()> {
        let request = match read_request(&mut stream) {
            Ok(request) => request,
            Err(_) => return write_status(&mut stream, 400, "Bad Request"),
        };
        match (request.method.as_str(), &self.results) {
            ("POST", Some(sink)) if request.path == RESULTS_PATH => {
                sink(String::from_utf8_lossy(&request.body).into_owned());
                return write_status(&mut stream, 204, "No Content");
            }
            ("GET" | "HEAD", _) => {}
            _ => return write_status(&mut stream, 405, "Method Not Allowed"),
        }
        let head_only = request.method == "HEAD";
        match self.route(&request.path) {
            Route::Redirect => write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: {BASE}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
            Route::NotFound => write_status(&mut stream, 404, "Not Found"),
            Route::File(path) => {
                let mut file = File::open(&path)?;
                let len = file.metadata()?.len();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {len}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                    content_type(&path)
                )?;
                if !head_only {
                    io::copy(&mut file, &mut stream)?;
                }
                stream.flush()
            }
        }
    }

    fn route(&self, url_path: &str) -> Route {
        if url_path == "/" || url_path == BASE.trim_end_matches('/') {
            return Route::Redirect;
        }
        let Some((rest, dir)) = self
            .mounts
            .iter()
            .find_map(|(prefix, dir)| Some((url_path.strip_prefix(prefix)?, dir)))
        else {
            return Route::NotFound;
        };
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

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
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
    }
    if content_length > MAX_BODY_BYTES {
        return Err(invalid());
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    Ok(Request { method, path, body })
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
    fn serves_over_http_without_isolation_headers() {
        let (dir, server) = fixture("http");
        let response = exchange(
            server,
            "GET /volya-engine/?test=smoke HTTP/1.1\r\nHost: x\r\n\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(response.contains("Content-Type: text/html; charset=utf-8\r\n"));
        assert!(response.ends_with("<p>hi</p>"));
        assert!(!response.contains("Cross-Origin"));
        fs::remove_dir_all(dir).unwrap();
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
