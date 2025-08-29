use std::env;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process;

const PROTO_VERSION: &str = "1";

fn encode_www_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        let c = *b as char;
        match c {
            ' ' => out.push('+'),
            'A'..='Z'
            | 'a'..='z'
            | '0'..='9'
            | '-'
            | '_'
            | '.'
            | '~' => out.push(c),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn parse_http_url(u: &str) -> Option<(String, u16, String)> {
    // Very simple parser for http://host:port/path
    let s = u.trim();
    let after = s.strip_prefix("http://")?;
    let mut parts = after.splitn(2, '/');
    let hostport = parts.next().unwrap_or_default();
    let path_rest = parts.next().unwrap_or_default();
    let mut hp = hostport.rsplitn(2, ':');
    let port_str = hp.next().unwrap_or("80");
    let host_part = hp.next().unwrap_or(hostport);
    let port = port_str.parse::<u16>().ok()?;
    let path = format!("/{}", path_rest);
    Some((host_part.to_string(), port, path))
}

fn main() {
    let url = match env::var("AIFO_TOOLEEXEC_URL") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!("aifo-shim: proxy not configured. Please launch agent with --toolchain.");
            process::exit(86);
        }
    };
    let token = match env::var("AIFO_TOOLEEXEC_TOKEN") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!("aifo-shim: proxy token missing. Please launch agent with --toolchain.");
            process::exit(86);
        }
    };

    let tool = std::env::args_os()
        .next()
        .and_then(|p| {
            let pb = PathBuf::from(p);
            pb.file_name().map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    let cwd = env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let mut body = String::new();
    body.push_str("tool=");
    body.push_str(&encode_www_form(&tool));
    body.push('&');
    body.push_str("cwd=");
    body.push_str(&encode_www_form(&cwd));
    for a in std::env::args().skip(1) {
        body.push('&');
        body.push_str("arg=");
        body.push_str(&encode_www_form(&a));
    }

    let (host, port, path) = match parse_http_url(&url) {
        Some(v) => v,
        None => {
            eprintln!("aifo-shim: unsupported URL: {}", url);
            process::exit(86);
        }
    };

    // Connect
    let addr_iter = (host.as_str(), port)
        .to_socket_addrs()
        .expect("failed to resolve proxy host");
    let mut last_err = None;
    let mut stream_opt = None;
    for addr in addr_iter {
        match TcpStream::connect(addr) {
            Ok(s) => {
                stream_opt = Some(s);
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let mut stream = match stream_opt {
        Some(s) => s,
        None => {
            eprintln!(
                "aifo-shim: failed to connect to proxy at {}:{} ({:?})",
                host, port, last_err
            );
            process::exit(86);
        }
    };

    // Write request
    let req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: {}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        host,
        token,
        PROTO_VERSION,
        body.len(),
        body
    );
    if let Err(e) = stream.write_all(req.as_bytes()) {
        eprintln!("aifo-shim: write failed: {}", e);
        process::exit(86);
    }

    // Read response headers
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    let header_end = loop {
        match stream.read(&mut tmp) {
            Ok(0) => break None,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break Some(pos + 4);
                }
                if buf.len() > 64 * 1024 {
                    break None;
                }
            }
            Err(_) => break None,
        }
    };
    let hend = header_end.unwrap_or(buf.len());
    let header = String::from_utf8_lossy(&buf[..hend]).to_string();

    // Extract status code hint and Content-Length and X-Exit-Code
    let mut content_len: usize = 0;
    let mut exit_code: i32 = 1;
    for line in header.lines() {
        if let Some(v) = line.strip_prefix("Content-Length: ") {
            content_len = v.trim().parse::<usize>().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("X-Exit-Code: ") {
            exit_code = v.trim().parse::<i32>().unwrap_or(1);
        }
    }

    // Read body: may already have some bytes buffered beyond headers
    let mut body_bytes = buf[hend..].to_vec();
    while body_bytes.len() < content_len {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => body_bytes.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }

    let _ = std::io::stdout().write_all(&body_bytes);
    let _ = std::io::stdout().flush();

    // Fallback exit code if header missing
    if exit_code == 0 {
        process::exit(0);
    } else {
        process::exit(exit_code);
    }
}
