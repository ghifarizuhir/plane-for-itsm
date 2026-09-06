use api::middleware::origin::{build_allowed_origins, origin_allowed, origin_allowed_many};
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

#[test]
fn post_second_origin_allowed_when_listed() {
    // Regression: admin (:3001) POST was 403 {"error":"bad origin"} while
    // FRONTEND_URL was web (:3000) — single-origin check rejected it.
    let allowed = vec![
        "http://192.168.1.11:3000".to_string(),
        "http://localhost:3001".to_string(),
    ];
    let h = headers(Some("http://localhost:3001"), None);
    assert!(origin_allowed_many(&Method::POST, &h, &allowed));
}

#[test]
fn post_foreign_origin_rejected_when_listed() {
    let allowed = vec![
        "http://192.168.1.11:3000".to_string(),
        "http://localhost:3001".to_string(),
    ];
    let h = headers(Some("https://evil.example"), None);
    assert!(!origin_allowed_many(&Method::POST, &h, &allowed));
}

#[test]
fn post_referer_fallback_matches_any_listed() {
    let allowed = vec![
        "http://192.168.1.11:3000".to_string(),
        "http://192.168.1.11:3001".to_string(),
    ];
    let h = headers(None, Some("http://192.168.1.11:3001/general"));
    assert!(origin_allowed_many(&Method::POST, &h, &allowed));
}

#[test]
fn build_allowed_origins_merges_and_dedups() {
    let list = build_allowed_origins(
        "http://192.168.1.11:3000",
        "http://localhost:3000, http://localhost:3001, http://192.168.1.11:3000/",
        &["http://192.168.1.11:3001/".to_string()],
    );
    assert_eq!(
        list,
        vec![
            "http://192.168.1.11:3000".to_string(),
            "http://localhost:3000".to_string(),
            "http://localhost:3001".to_string(),
            "http://192.168.1.11:3001".to_string(),
        ]
    );
}
