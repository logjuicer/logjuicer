// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a bare minimal http server for reading report with xdg-open

use anyhow::Result;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
};

/// Return the first available port lower than 10_000
fn get_listener(port: u16) -> Option<(u16, TcpListener)> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => Some((port, l)),
        _ if port < 9_999 => get_listener(port + 1),
        _ => None,
    }
}

/// Spawn an http service to serve the given report
pub fn serve(name: &str, index: &str, report: &logjuicer_report::Report) -> Result<()> {
    // Find available port
    match get_listener(8000) {
        None => Err(anyhow::anyhow!("Couldn't find available port!")),
        Some((port, listener)) => {
            // Run xdg-open
            let url = format!("http://127.0.0.1:{port}/{name}");
            match std::process::Command::new("xdg-open").arg(&url).spawn() {
                Ok(_) => {}
                Err(e) => {
                    println!("Failed to start xdg-open {url} : {e}");
                }
            }

            // The request line for the HTML content
            let index_line = format!("GET /{name} ");

            // Render the report
            let mut report_bytes: Vec<u8> = Vec::new();
            report.save_writer(&mut report_bytes)?;

            // Accept connections
            for stream in listener.incoming() {
                // Read the request
                let mut stream = stream?;
                let buf_reader = BufReader::new(&mut stream);
                let request_line = buf_reader.lines().next().unwrap_or(Ok("".to_string()))?;

                // Prepare the response body
                let content = if request_line.starts_with(&index_line) {
                    index.as_bytes()
                } else {
                    &report_bytes
                };

                // Prepare the response headers
                let length = content.len();
                let status_line = "HTTP/1.1 200 OK";
                let headers = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n");

                // Respond...
                stream.write_all(headers.as_bytes())?;
                stream.write_all(content)?;

                // Stop after serving the report
                if !request_line.starts_with(&index_line) {
                    break;
                };
            }
            Ok(())
        }
    }
}
