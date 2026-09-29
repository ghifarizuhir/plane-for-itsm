//! Routing guard: every triage-suggestion URL used by the web inbox service
//! must match a route registered in `main.rs`, on both the `intake-issues/`
//! and the `inbox-issues/` aliases. (The FE service calls `inbox-issues/`;
//! a route registered only under `intake-issues/` serves a 404 here.)
//! No DB, no env.
mod common;

#[test]
fn fe_triage_urls_are_routed() {
    let service = crate::common::repo_root()
        .join("apps/web/core/services/inbox/inbox-issue.service.ts");
    let urls = crate::common::fe_urls_in_file(&service);
    let triage_urls: Vec<String> = urls
        .into_iter()
        .filter(|url| url.contains("triage-suggestion"))
        .collect();
    assert_eq!(
        triage_urls.len(),
        3,
        "expected 3 triage-suggestion FE urls, got {triage_urls:?}"
    );
    let routes = crate::common::rust_routes();
    for url in &triage_urls {
        let fe_segments = crate::common::wildcard_segments(url, true);
        let matched = routes.iter().any(|route| {
            crate::common::segments_match(
                &fe_segments,
                &crate::common::wildcard_segments(route, false),
            )
        });
        assert!(
            matched,
            "FE triage url has no backend route: {url}\nroutes: {routes:?}"
        );
    }
}
