// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;

use volya_devserver::{BASE, Server};

const USAGE: &str = "\
Usage: volya-devserver [--root DIR] [--port PORT] [--tls-cert PEM --tls-key PEM]

Serves DIR (default: dist) under /volya-engine/ like GitHub Pages.
Plain HTTP binds to localhost; with TLS it binds to all interfaces so real
devices can connect over a secure context.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("volya-devserver: {error}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut root = PathBuf::from("dist");
    let mut port = 8080u16;
    let (mut cert, mut key) = (None, None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--root" => root = value()?.into(),
            "--port" => port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "--tls-cert" => cert = Some(PathBuf::from(value()?)),
            "--tls-key" => key = Some(PathBuf::from(value()?)),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => return Err(format!("unknown argument {arg}")),
        }
    }

    let server = Server::new(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    let server = match (cert, key) {
        (Some(cert), Some(key)) => server.with_tls(&cert, &key).map_err(|e| e.to_string())?,
        (None, None) => server,
        _ => return Err("--tls-cert and --tls-key go together".into()),
    };
    let (scheme, host) = if server.is_tls() {
        ("https", "0.0.0.0")
    } else {
        ("http", "localhost")
    };
    let listener =
        TcpListener::bind((host, port)).map_err(|e| format!("bind {host}:{port}: {e}"))?;
    println!(
        "Serving {} at {scheme}://{host}:{port}{BASE}",
        root.display()
    );
    server.run(listener).map_err(|e| e.to_string())
}
