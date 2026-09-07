//! Route-inventory gate (Batch F T0): every migration-baseline path must be
//! registered in `main.rs`. Mirrors the audit in
//! `docs/superpowers/plans/2026-09-08-batch-f-route-parity-gap.md`.
use std::fs;

/// Extract every `.route("<path>", ...)` path string from `src/main.rs`,
/// skipping `.route(` occurrences inside `//` line comments. Multi-line
/// `.route(` calls are supported (path is the first string after the call).
fn rust_routes() -> Vec<String> {
    let src = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs")).expect("main.rs");
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

/// Canonical Django-side form; parameter names must match `main.rs`.
const BASELINE: &[&str] = &[
    // T1
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-links/",
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-links/:pk/",
    // T2
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-relation/",
    // T3
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-attachments/",
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-attachments/:pk/",
    // T4
    "/api/workspaces/:slug/projects/:project_id/bulk-create-labels/",
    // T5
    "/api/workspaces/:slug/projects/:project_id/project-estimates/",
    // T6
    "/api/workspaces/:slug/projects/:project_id/project-views/",
    // T7
    "/api/workspaces/:slug/users/notifications/:pk/",
    // T8
    "/api/workspaces/:slug/workspace-themes/",
    "/api/workspaces/:slug/workspace-themes/:pk/",
    // T9
    "/api/workspaces/:slug/analytics/",
    "/api/workspaces/:slug/saved-analytic-view/:analytic_id/",
    "/api/workspaces/:slug/export-analytics/",
    "/api/workspaces/:slug/analytic-view/:pk/",
    // T10 (route shape kept as `:ident`; handler enforces the constraint)
    "/api/workspaces/:slug/work-items/:ident/",
];

#[test]
fn migration_baseline_is_registered() {
    let routes = rust_routes();
    let missing: Vec<&str> = BASELINE.iter().copied().filter(|p| !routes.iter().any(|r| r == p)).collect();
    assert!(missing.is_empty(), "routes missing from main.rs:\n{}", missing.join("\n"));
}

#[test]
fn baseline_is_complete() {
    // Tripwire: 15 gap routes + 1 constraint route.
    assert!(BASELINE.len() == 16, "expected 16 baseline paths, got {}", BASELINE.len());
}
