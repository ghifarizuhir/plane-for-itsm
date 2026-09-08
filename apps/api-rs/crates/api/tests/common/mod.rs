//! Shared helpers for the parity-inventory integration tests.
use std::path::{Path, PathBuf};

pub const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// Repo root: crates/api -> ../../../../ (apps, packages, docs live here).
pub fn repo_root() -> PathBuf {
    Path::new(MANIFEST_DIR).join("../../../../")
}

pub fn inventory_path() -> PathBuf {
    Path::new(MANIFEST_DIR).join("parity-inventory.json")
}

pub fn load_inventory() -> serde_json::Value {
    let raw = std::fs::read_to_string(inventory_path()).expect("parity-inventory.json must exist");
    serde_json::from_str(&raw).expect("parity-inventory.json must be valid JSON")
}

/// Flatten inventory into `(domain, endpoint)` pairs.
pub fn endpoints(inv: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    let mut out = Vec::new();
    let domains = inv["domains"].as_object().expect("domains must be an object");
    for (domain, d) in domains {
        let eps = d["endpoints"].as_array().expect("endpoints must be an array");
        for ep in eps {
            out.push((domain.clone(), ep.clone()));
        }
    }
    out
}

/// Extract every `.route("<path>", ...)` path string from `src/main.rs`,
/// skipping `.route(` occurrences inside `//` line comments. Multi-line
/// `.route(` calls are supported (path is the first string after the call).
pub fn rust_routes() -> Vec<String> {
    let src = std::fs::read_to_string(Path::new(MANIFEST_DIR).join("src/main.rs")).expect("main.rs");
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = src[i..].find(".route(") {
        let abs = i + p;
        let line_start = src[..abs].rfind('\n').map(|x| x + 1).unwrap_or(0);
        if !src[line_start..abs].contains("//") {
            let rest = &src[abs + ".route(".len()..];
            if let Some(q) = rest.find('"') {
                let after_q = &rest[q + 1..];
                if let Some(e) = after_q.find('"') {
                    out.push(after_q[..e].to_string());
                }
            }
        }
        i = abs + ".route(".len() + 1;
    }
    out
}

/// Split a path/URL template into wildcard segments.
/// - FE templates: a segment containing `${` becomes `*`.
/// - Inventory paths: a segment starting with `:` becomes `*`.
/// Literals stay. Trailing slash is dropped.
pub fn wildcard_segments(s: &str, is_fe: bool) -> Vec<String> {
    let s = s.trim_end_matches('/');
    s.split('/')
        .filter(|seg| !seg.is_empty())
        .map(|seg| {
            if is_fe {
                if seg.contains("${") {
                    "*".to_string()
                } else {
                    seg.to_string()
                }
            } else if seg.starts_with(':') {
                "*".to_string()
            } else {
                seg.to_string()
            }
        })
        .collect()
}

/// Position-wise wildcard match: equal, or either side is `*`.
pub fn segments_match(fe: &[String], matrix: &[String]) -> bool {
    if fe.len() != matrix.len() {
        return false;
    }
    fe.iter().zip(matrix.iter()).all(|(f, m)| f == "*" || m == "*" || f == m)
}

/// Extract `/api/...` URL template strings from a TS file (one URL per line).
pub fn fe_urls_in_file(path: &Path) -> Vec<String> {
    let src = std::fs::read_to_string(path).unwrap_or_default();
    let mut out = Vec::new();
    for line in src.lines() {
        let Some(start) = line.find("/api/") else { continue };
        let rest = &line[start..];
        let end = rest.find('`').or_else(|| rest.find('"')).unwrap_or(rest.len());
        let url = &rest[..end];
        let url = url
            .trim_end_matches(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == '`' || c == '"');
        if url.starts_with("/api/") && url.len() > 5 {
            out.push(url.to_string());
        }
    }
    out
}
