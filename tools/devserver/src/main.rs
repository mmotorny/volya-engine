// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use volya_engine_devserver::{BASE, Server};

const USAGE: &str = "\
Usage: volya-engine-devserver [--root DIR] [--tests DIR] [--installer DIR] [--port PORT]

Serves --root (default: dist) under /volya-engine/ like GitHub Pages, and the
browser test suites (default: tests/browser, if present) under
/volya-engine/__test/; open /volya-engine/__test/<suite>.html to run a suite
and print its report here. --installer serves a GOG installer directory to
tests (default: $VOLYA_GOG_INSTALLER, if set). Frames suites dump are written
under $VOLYA_DUMP_DIR, if set.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("volya-engine-devserver: {error}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut root = PathBuf::from("dist");
    let mut tests = Some(PathBuf::from("tests/browser")).filter(|d| d.is_dir());
    let mut port = 8080u16;
    let mut installer = std::env::var_os("VOLYA_GOG_INSTALLER").map(PathBuf::from);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--root" => root = value()?.into(),
            "--tests" => tests = Some(value()?.into()),
            "--installer" => installer = Some(value()?.into()),
            "--port" => port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => return Err(format!("unknown argument {arg}")),
        }
    }

    let server = Server::new(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    let server = match tests {
        Some(dir) => server
            .with_tests(&dir, Arc::new(|report| println!("{report}\n")))
            .map_err(|e| format!("{}: {e}", dir.display()))?,
        None => server,
    };
    let server = match installer {
        Some(dir) => server
            .with_installer(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?,
        None => server,
    };
    let server = match std::env::var_os("VOLYA_DUMP_DIR") {
        Some(dir) => server
            .with_dumps(std::path::Path::new(&dir))
            .map_err(|e| format!("VOLYA_DUMP_DIR: {e}"))?,
        None => server,
    };
    let listener = TcpListener::bind(("localhost", port))
        .map_err(|e| format!("bind localhost:{port}: {e}"))?;
    println!(
        "Serving {} at http://localhost:{port}{BASE}",
        root.display()
    );
    server.run(listener).map_err(|e| e.to_string())
}
