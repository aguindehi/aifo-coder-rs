pub fn port_from_http_url(url: &str) -> u16 {
    let after = url.split("://").nth(1).unwrap_or(url);
    let host_port = after.split('/').next().unwrap_or(after);
    host_port
        .rsplit(':')
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0)
}
