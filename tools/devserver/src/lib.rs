// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Static file server that mimics GitHub Pages for local development:
//! serves a directory under [`BASE`], sends no custom headers (so no
//! cross-origin isolation), and optionally speaks HTTPS for real devices.

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

const MAX_HEADER_BYTES: usize = 16 * 1024;

pub struct Server {
    root: PathBuf,
    tls: Option<Arc<rustls::ServerConfig>>,
}

impl Server {
    pub fn new(root: &Path) -> io::Result<Self> {
        Ok(Self {
            root: root.canonicalize()?,
            tls: None,
        })
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
        if request.method != "GET" && request.method != "HEAD" {
            return write_status(&mut stream, 405, "Method Not Allowed");
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
        let Some(rest) = url_path.strip_prefix(BASE) else {
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
        let mut path = self.root.join(relative);
        if path.is_dir() {
            path.push("index.html");
        }
        // Reject anything that escapes the root, e.g. via symlinks.
        match path.canonicalize() {
            Ok(real) if real.starts_with(&self.root) && real.is_file() => Route::File(real),
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
}

fn read_request(stream: &mut impl Read) -> io::Result<Request> {
    let mut reader = BufReader::new(stream.take(MAX_HEADER_BYTES as u64));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(io::ErrorKind::InvalidData.into());
    };
    let path = target.split(['?', '#']).next().unwrap_or_default();
    let request = Request {
        method: method.to_owned(),
        path: path.to_owned(),
    };
    // Drain headers; none are needed.
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if line == "\r\n" || line == "\n" {
            return Ok(request);
        }
    }
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

    fn fixture() -> (PathBuf, Server) {
        let dir = std::env::temp_dir().join(format!(
            "volya-devserver-test-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("root/pkg")).unwrap();
        fs::write(dir.join("root/index.html"), "<p>hi</p>").unwrap();
        fs::write(dir.join("root/pkg/a b.wasm"), [0, 97, 115, 109]).unwrap();
        fs::write(dir.join("secret.txt"), "nope").unwrap();
        let server = Server::new(&dir.join("root")).unwrap();
        (dir, server)
    }

    fn get(server: Server, path: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || server.run(listener));
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn routes() {
        let (dir, server) = fixture();
        let root = dir.join("root").canonicalize().unwrap();
        assert_eq!(server.route("/"), Route::Redirect);
        assert_eq!(server.route("/volya-engine"), Route::Redirect);
        assert_eq!(server.route(BASE), Route::File(root.join("index.html")));
        assert_eq!(
            server.route("/volya-engine/pkg/a%20b.wasm"),
            Route::File(root.join("pkg/a b.wasm"))
        );
        assert_eq!(server.route("/other/index.html"), Route::NotFound);
        assert_eq!(server.route("/volya-engine/missing"), Route::NotFound);
        assert_eq!(server.route("/volya-engine/../secret.txt"), Route::NotFound);
        assert_eq!(
            server.route("/volya-engine/%2e%2e/secret.txt"),
            Route::NotFound
        );
        assert_eq!(server.route("/volya-engine//etc/passwd"), Route::NotFound);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn serves_over_http_without_isolation_headers() {
        let (dir, server) = fixture();
        let response = get(server, "/volya-engine/?test=smoke");
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(response.contains("Content-Type: text/html; charset=utf-8\r\n"));
        assert!(response.ends_with("<p>hi</p>"));
        assert!(!response.contains("Cross-Origin"));
        fs::remove_dir_all(dir).unwrap();
    }
}
