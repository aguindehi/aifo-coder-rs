/*!
HTTP helpers for the in-process proxy: tolerant request parsing and endpoint classification.

This module introduces a minimal request model and utilities to parse a single HTTP
request from a Read stream, with compatibility for both CRLFCRLF and LFLF header
termination and a 64 KiB header cap (matching existing behavior in spirit).
*/

use crate::find_header_end;
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
    Signal,
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
        if let Some(end_idx) = find_header_end(&buf) {
            header_end = Some(end_idx);
        }
    }

    let hend = header_end.unwrap_or(buf.len());
    let mut body = Vec::new();
    // Compute header_bytes and body_start using canonical helper semantics
    let (header_bytes, body_start) = if let Some(end_idx) = header_end {
        // Determine which terminator was used by inspecting bytes before end_idx
        let header_bytes: &[u8] = if end_idx >= 4 && &buf[end_idx - 4..end_idx] == b"\r\n\r\n" {
            &buf[..end_idx - 4]
        } else if end_idx >= 2 && &buf[end_idx - 2..end_idx] == b"\n\n" {
            &buf[..end_idx - 2]
        } else {
            // Fallback: treat everything up to end_idx as headers
            &buf[..end_idx]
        };
        (header_bytes, end_idx)
    } else {
        // No terminator found; treat entire buffer as headers, no body yet
        (&buf[..hend], hend)
    };
    if buf.len() > body_start {
        body.extend_from_slice(&buf[body_start..]);
    }

    let header_str = String::from_utf8_lossy(header_bytes);
    let mut lines = header_str.lines();
    let request_line = lines.next().unwrap_or_default().trim().to_string();

    let (method, path_lc, query_pairs) = parse_request_line_and_query(&request_line);
    let headers = parse_headers(lines);

    // Support Transfer-Encoding: chunked by de-chunking into body; otherwise honor Content-Length.
    let te = headers
        .get("transfer-encoding")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if te.contains("chunked") {
        // Initialize a buffer with any bytes already read after headers
        let mut rbuf: Vec<u8> = Vec::new();
        if buf.len() > body_start {
            rbuf.extend_from_slice(&buf[body_start..]);
        }

        // Helper: read a single line ending in CRLF or LF directly from reader
        fn read_line_from<R2: Read>(reader: &mut R2, rbuf: &mut Vec<u8>) -> Option<String> {
            loop {
                if let Some(pos) = rbuf
                    .windows(2)
                    .position(|w| w == b"\r\n")
                    .or_else(|| rbuf.iter().position(|&b| b == b'\n'))
                {
                    let (line, rest) = if pos + 1 < rbuf.len() && rbuf[pos] == b'\r' {
                        let line = rbuf[..pos].to_vec();
                        let rest = rbuf[pos + 2..].to_vec();
                        (line, rest)
                    } else {
                        let line = rbuf[..pos].to_vec();
                        let rest = rbuf[pos + 1..].to_vec();
                        (line, rest)
                    };
                    *rbuf = rest;
                    return String::from_utf8(line).ok();
                }
                let mut tmp2 = [0u8; 1024];
                match reader.read(&mut tmp2) {
                    Ok(0) => return None,
                    Ok(n) => rbuf.extend_from_slice(&tmp2[..n]),
                    Err(_e) => return None,
                }
            }
        }

        // Decode chunks
        body.clear();
        loop {
            let ln = match read_line_from(reader, &mut rbuf) {
                Some(s) => s,
                None => break,
            };
            let ln_trim = ln.trim();
            if ln_trim.is_empty() {
                continue;
            }
            // Parse chunk size (hex), tolerate extensions after ';'
            let size_hex = ln_trim.split(';').next().unwrap_or(ln_trim);
            let mut size = match usize::from_str_radix(size_hex, 16) {
                Ok(v) => v,
                Err(_) => break,
            };
            if size == 0 {
                // Consume trailers until blank line
                loop {
                    match read_line_from(reader, &mut rbuf) {
                        Some(tr) => {
                            if tr.trim().is_empty() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                break;
            }
            // Read exactly 'size' bytes for this chunk (respect BODY_CAP)
            while rbuf.len() < size {
                let mut tmp2 = [0u8; 1024];
                match reader.read(&mut tmp2) {
                    Ok(0) => break,
                    Ok(n) => rbuf.extend_from_slice(&tmp2[..n]),
                    Err(_e) => break,
                }
            }
            let take = size.min(rbuf.len());
            if !rbuf.is_empty() {
                // Enforce BODY_CAP
                let available = BODY_CAP.saturating_sub(body.len());
                let to_copy = available.min(take);
                if to_copy > 0 {
                    body.extend_from_slice(&rbuf[..to_copy]);
                }
                // Even if we didn't copy all (cap reached), we must still drain the full chunk size
                rbuf.drain(..take);
            }
            // Consume trailing CRLF after chunk payload
            while rbuf.len() < 2 {
                let mut tmp2 = [0u8; 1024];
                match reader.read(&mut tmp2) {
                    Ok(0) => break,
                    Ok(n) => rbuf.extend_from_slice(&tmp2[..n]),
                    Err(_e) => break,
                }
            }
            if rbuf.starts_with(b"\r\n") {
                rbuf.drain(..2);
            } else if rbuf.starts_with(b"\n") {
                rbuf.drain(..1);
            }
            // If BODY_CAP reached, we can continue draining remaining chunks without appending
            if body.len() >= BODY_CAP {
                // Drain until zero-size chunk encountered
                loop {
                    let ln2 = match read_line_from(reader, &mut rbuf) {
                        Some(s) => s,
                        None => break,
                    };
                    let size_hex2 = ln2.trim().split(';').next().unwrap_or(ln2.trim());
                    let sz2 = usize::from_str_radix(size_hex2, 16).unwrap_or(0);
                    if sz2 == 0 {
                        break;
                    }
                    // Drain payload + trailing CRLF
                    let mut left = sz2;
                    while left > 0 {
                        if rbuf.is_empty() {
                            let mut tmp2 = [0u8; 1024];
                            match reader.read(&mut tmp2) {
                                Ok(0) => break,
                                Ok(n) => rbuf.extend_from_slice(&tmp2[..n]),
                                Err(_e) => break,
                            }
                        }
                        let drop = left.min(rbuf.len());
                        rbuf.drain(..drop);
                        left -= drop;
                    }
                    while rbuf.len() < 2 {
                        let mut tmp2 = [0u8; 1024];
                        match reader.read(&mut tmp2) {
                            Ok(0) => break,
                            Ok(n) => rbuf.extend_from_slice(&tmp2[..n]),
                            Err(_e) => break,
                        }
                    }
                    if rbuf.starts_with(b"\r\n") {
                        rbuf.drain(..2);
                    } else if rbuf.starts_with(b"\n") {
                        rbuf.drain(..1);
                    }
                }
                // Consume trailers (best-effort)
                loop {
                    match read_line_from(reader, &mut rbuf) {
                        Some(tr) => {
                            if tr.trim().is_empty() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                break;
            }
        }
    } else {
        // Non-chunked: honor Content-Length if present; keep any already-read bytes after headers.
        let mut content_len: usize = 0;
        if let Some(v) = headers.get("content-length") {
            content_len = v.trim().parse().unwrap_or(0);
        }
        if content_len > BODY_CAP {
            content_len = BODY_CAP;
        }
        let mut remaining = content_len.saturating_sub(body.len());
        while remaining > 0 {
            let chunk = remaining.min(8 * 1024);
            let mut rem_buf = vec![0u8; chunk];
            let got: usize = reader.read(&mut rem_buf).unwrap_or_default();
            if got == 0 {
                break;
            }
            let new_len = body.len().saturating_add(got);
            if new_len > BODY_CAP {
                let allowed = BODY_CAP.saturating_sub(body.len());
                if allowed > 0 {
                    body.extend_from_slice(&rem_buf[..allowed]);
                }
                break;
            } else {
                body.extend_from_slice(&rem_buf[..got]);
                remaining -= got;
            }
        }
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
        "/signal" => Some(Endpoint::Signal),
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
    let mut out = Vec::new();
    for pair in s.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or_default();
        let v = it.next().unwrap_or_default();
        out.push((crate::url_decode(k), crate::url_decode(v)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_form_urlencoded_basic_and_repeated() {
        let pairs = parse_form_urlencoded("arg=a&arg=b&tool=cargo&cwd=.");
        let expected = vec![
            ("arg".to_string(), "a".to_string()),
            ("arg".to_string(), "b".to_string()),
            ("tool".to_string(), "cargo".to_string()),
            ("cwd".to_string(), ".".to_string()),
        ];
        assert_eq!(pairs, expected);
    }

    #[test]
    fn test_parse_form_urlencoded_empty_and_missing_values() {
        let pairs = parse_form_urlencoded("a=1&b=&c");
        assert!(pairs.contains(&(String::from("a"), String::from("1"))));
        assert!(pairs.contains(&(String::from("b"), String::from(""))));
        assert!(pairs.contains(&(String::from("c"), String::from(""))));
    }
}
