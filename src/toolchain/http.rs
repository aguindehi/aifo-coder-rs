/*!
HTTP helpers for the in-process proxy: tolerant request parsing and endpoint classification.

This module introduces a minimal request model and utilities to parse a single HTTP
request from a Read stream, with compatibility for both CRLFCRLF and LFLF header
termination and a 64 KiB header cap (matching existing behavior in spirit).
*/
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{self, Read};

/// Supported HTTP methods (minimal)
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Method {
    Get,
    Post,
    Other(String),
}

/// Proxy endpoints we recognize
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Endpoint {
    Exec,
    Notifications,
}

/// Simple case-insensitive header map (keys lowercased)
pub(crate) type HeaderMap = HashMap<String, String>;

/// Parsed HTTP request (lowercased path, normalized headers)
#[derive(Debug, Clone)]
pub(crate) struct HttpRequest {
    pub method: Method,
    pub path_lc: String,
    pub query: Vec<(String, String)>,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

/// Parse a single HTTP request from a reader with a 64 KiB header cap.
/// Tolerant to CRLFCRLF and LFLF as header terminators. Best-effort body read
/// based on Content-Length if present; otherwise returns whatever is available.
pub(crate) fn read_http_request<R: Read>(reader: &mut R) -> io::Result<HttpRequest> {
    const HDR_CAP: usize = 64 * 1024;
    const BODY_CAP: usize = 1024 * 1024; // 1 MiB soft cap for forms
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    let mut header_end: Option<usize> = None;

    // Read until we find the end of headers or hit the cap/EOF
    while header_end.is_none() && buf.len() < HDR_CAP {
        let n = reader.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_crlfcrlf(&buf) {
            header_end = Some(pos);
        } else if let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
            header_end = Some(pos);
        }
    }

    let hend = header_end.unwrap_or(buf.len());
    let header_bytes = &buf[..hend];
    let mut body = Vec::new();
    // Skip terminator if it exists
    let mut body_start = hend;
    if buf.len() >= hend + 4 && &buf[hend..hend + 4] == b"\r\n\r\n" {
        body_start = hend + 4;
    } else if buf.len() >= hend + 2 && &buf[hend..hend + 2] == b"\n\n" {
        body_start = hend + 2;
    }
    if buf.len() > body_start {
        body.extend_from_slice(&buf[body_start..]);
    }

    let header_str = String::from_utf8_lossy(header_bytes);
    let mut lines = header_str.lines();
    let request_line = lines.next().unwrap_or_default().trim().to_string();

    let (method, path_lc, query_pairs) = parse_request_line_and_query(&request_line);
    let headers = parse_headers(lines);

    // If Content-Length is present, read the remaining body bytes from the reader
    let mut content_len: usize = 0;
    if let Some(v) = headers.get("content-length") {
        content_len = v.trim().parse().unwrap_or(0);
    }
    // Enforce body cap: callers may map this to HTTP 400 "bad request\n"
    if content_len > BODY_CAP {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "form body too large",
        ));
    }
    let mut remaining = content_len.saturating_sub(body.len());
    while remaining > 0 {
        let chunk = remaining.min(8 * 1024);
        let mut rem_buf = vec![0u8; chunk];
        let got = match reader.read(&mut rem_buf) {
            Ok(n) => n,
            Err(_) => 0,
        };
        if got == 0 {
            // EOF or peer closed; best-effort stop
            break;
        }
        // Best-effort: do not exceed BODY_CAP even if Content-Length lied
        let new_len = body.len().saturating_add(got);
        if new_len > BODY_CAP {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "form body too large",
            ));
        }
        body.extend_from_slice(&rem_buf[..got]);
        remaining -= got;
    }

    Ok(HttpRequest {
        method,
        path_lc,
        query: query_pairs,
        headers,
        body,
    })
}

/// Classify a lowercased path into a known endpoint.
pub(crate) fn classify_endpoint(path_lc: &str) -> Option<Endpoint> {
    match path_lc {
        "/exec" => Some(Endpoint::Exec),
        "/notify" => Some(Endpoint::Notifications),
        _ => None,
    }
}

fn parse_headers<'a, I: Iterator<Item = &'a str>>(lines: I) -> HeaderMap {
    let mut map = HeaderMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            map.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    map
}

fn parse_request_line_and_query(request_line: &str) -> (Method, String, Vec<(String, String)>) {
    let mut parts = request_line.split_whitespace();
    let method = match parts.next().unwrap_or("").to_ascii_uppercase().as_str() {
        "GET" => Method::Get,
        "POST" => Method::Post,
        other => Method::Other(other.to_string()),
    };
    let target = parts.next().unwrap_or("/");
    let path_only = target
        .split('?')
        .next()
        .unwrap_or(target)
        .to_ascii_lowercase();
    let mut query = Vec::new();
    if let Some(idx) = target.find('?') {
        let q = &target[idx + 1..];
        query.extend(parse_form_urlencoded(q));
    }
    (method, path_only, query)
}

/*
Unified application/x-www-form-urlencoded parser with decoding:
- '+' → space
- %XX → byte decode; invalid sequences preserved literally (best-effort)
*/
pub(crate) fn parse_form_urlencoded(s: &str) -> Vec<(String, String)> {
    fn decode_component(input: &str) -> String {
        let bytes = input.as_bytes();
        let mut out = String::with_capacity(input.len());
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => {
                    out.push(' ');
                    i += 1;
                }
                b'%' if i + 2 < bytes.len() => {
                    let h1 = bytes[i + 1];
                    let h2 = bytes[i + 2];
                    let v1 = (h1 as char).to_digit(16);
                    let v2 = (h2 as char).to_digit(16);
                    if let (Some(a), Some(b)) = (v1, v2) {
                        let byte = ((a as u8) << 4) | (b as u8);
                        out.push(byte as char);
                        i += 3;
                    } else {
                        // Best-effort: leave as literal '%'
                        out.push('%');
                        i += 1;
                    }
                }
                c => {
                    out.push(c as char);
                    i += 1;
                }
            }
        }
        out
    }

    let mut out = Vec::new();
    for pair in s.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or_default();
        let v = it.next().unwrap_or_default();
        out.push((decode_component(k), decode_component(v)));
    }
    out
}

// Local helper for CRLFCRLF detection
fn find_crlfcrlf(buf: &[u8]) -> Option<usize> {
    if buf.len() < 4 {
        return None;
    }
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}
