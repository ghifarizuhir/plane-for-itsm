use api::middleware::origin::origin_allowed;
use axum::http::{HeaderMap, Method};

fn headers(origin: Option<&str>, referer: Option<&str>) -> HeaderMap {
    let mut h = HeaderMap::new();
    if let Some(o) = origin {
        h.insert("origin", o.parse().unwrap());
    }
    if let Some(r) = referer {
        h.insert("referer", r.parse().unwrap());
    }
    h
}

#[test]
fn get_always_allowed() {
    assert!(origin_allowed(&Method::GET, &headers(None, None), "https://app.example.com"));
}

#[test]
fn post_matching_origin_allowed() {
    let h = headers(Some("https://app.example.com"), None);
    assert!(origin_allowed(&Method::POST, &h, "https://app.example.com"));
}

#[test]
fn post_foreign_origin_rejected() {
    let h = headers(Some("https://evil.example"), None);
    assert!(!origin_allowed(&Method::POST, &h, "https://app.example.com"));
}

#[test]
fn post_referer_fallback_allowed() {
    let h = headers(None, Some("https://app.example.com/sign-in"));
    assert!(origin_allowed(&Method::POST, &h, "https://app.example.com"));
}

#[test]
fn post_no_origin_no_referer_rejected() {
    assert!(!origin_allowed(&Method::POST, &headers(None, None), "https://app.example.com"));
}
