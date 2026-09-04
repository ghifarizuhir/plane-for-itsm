//! Plan 3.4 Step 1: cutover guard — after cutover the renamed `api`
//! service alone (Rust, host port CUTOVER_PORT, default 8000) must serve
//! all ported paths. No new deps: raw TCP + std only.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn port() -> u16 {
    std::env::var("CUTOVER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000)
}

fn get(path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port())).expect("cutover api must listen");
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(s, "GET {} HTTP/1.0\r\nHost: x\r\nConnection: close\r\n\r\n", path).unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let head = String::from_utf8_lossy(&buf).into_owned();
    let status: u16 = head
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    (status, head)
}

#[test]
fn cutover_api_serves_all_paths() {
    let (st, body) = get("/health");
    assert_eq!(st, 200, "/health must be 200 after cutover");
    assert!(body.contains("\"status\":\"ok\""), "health body must be ok");
    // Ported contract paths: 401 (no token) proves Rust serves them, never 404/502.
    for p in [
        "/api/workspaces/",
        "/api/workspaces/ws/projects/",
        "/api/users/me/",
        "/api/timezones/",
    ] {
        let (st, _) = get(p);
        assert!(
            st == 200 || st == 401,
            "{} must be served after cutover, got {}",
            p,
            st
        );
    }
}
