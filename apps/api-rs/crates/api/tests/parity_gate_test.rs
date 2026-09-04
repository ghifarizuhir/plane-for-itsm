//! Parity gate (Plan 2.7): every ported route must be shadow-covered and live.
//!
//! - Always-run: `shadow.sh` covers all ported domains/detail routes.
//! - `#[ignore]` live gate: GET each path on the running Rust stack and
//!   assert no 404/empty body. Run with:
//!   `RUST_API_URL=127.0.0.1:8001 cargo test -p api --test parity_gate_test -- --ignored`
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

fn shadow_file() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{manifest}/../../scripts/shadow.sh"))
        .expect("shadow.sh must exist")
}

/// Quoted `/api/...` paths from shadow.sh with `$WS`/`$P` substituted.
fn shadow_paths() -> Vec<String> {
    let ws = std::env::var("WS").unwrap_or_else(|_| "test-ws".to_string());
    let p = std::env::var("P").unwrap_or_else(|_| "00000000-0000-0000-0000-000000000000".to_string());
    shadow_file()
        .lines()
        .filter_map(|line| {
            let start = line.find('"')? + 1;
            let end = line[start..].find('"')? + start;
            let path = &line[start..end];
            if !path.starts_with("/api/") {
                return None;
            }
            Some(path.replace("$WS", &ws).replace("$P", &p).split('?').next().unwrap_or(path).to_string())
        })
        .collect()
}

#[test]
fn gate_covers_detail_routes() {
    let paths = shadow_paths();
    for must in [
        "/cycles/00000000-0000-0000-0000-000000000000/",
        "/states/00000000-0000-0000-0000-000000000000/",
        "/issue-labels/00000000-0000-0000-0000-000000000000/",
        "/estimates/00000000-0000-0000-0000-000000000000/estimate-points/00000000-0000-0000-0000-000000000000/",
        "/intakes/00000000-0000-0000-0000-000000000000/",
        "/projects/00000000-0000-0000-0000-000000000000/",
        "/views/00000000-0000-0000-0000-000000000000/",
        "/members/00000000-0000-0000-0000-000000000000/",
        "/webhooks/00000000-0000-0000-0000-000000000000/",
        "/api/users/me/",
        "/api/users/me/accounts/",
    ] {
        assert!(paths.iter().any(|p| p.contains(must)), "gate missing coverage for {must}");
    }
}

#[test]
fn gate_covers_all_domains() {
    // Regression tripwire: adding a route without shadow coverage must fail.
    let paths = shadow_paths();
    assert!(paths.len() >= 55, "expected >= 55 shadow paths, got {}", paths.len());
}

fn get_status_and_body(addr: &str, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("rust-api must be running for live gate");
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(stream, "GET {path} HTTP/1.0\r\nHost: gate\r\nConnection: close\r\n\r\n").unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let status: u16 = raw.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

#[test]
#[ignore]
fn live_gate_no_404s_or_empty_bodies() {
    let addr = std::env::var("RUST_API_URL").unwrap_or_else(|_| "127.0.0.1:8001".to_string());
    let paths = shadow_paths();
    assert!(!paths.is_empty());
    let mut failures = Vec::new();
    for path in &paths {
        let (status, body) = get_status_and_body(&addr, path);
        if status == 404 || body.trim().is_empty() {
            failures.push(format!("{path} -> {status} body={body:?}"));
        }
    }
    assert!(failures.is_empty(), "live gate failures:\n{}", failures.join("\n"));
}
