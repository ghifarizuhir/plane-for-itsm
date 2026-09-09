//! Route-inventory gate: reads `parity-inventory.json` and fails CI if any
//! path claimed implemented is missing from `main.rs`, or if the inventory
//! schema drifts. Replaces the old hardcoded BASELINE in route_parity_test.rs.
mod common;

use common::{endpoints, load_inventory, rust_routes};
use std::collections::HashSet;

const VALID_STATUSES: &[&str] = &["implemented", "missing", "shape_mismatch", "constraint_mismatch"];
const VALID_METHODS: &[&str] = &["GET", "POST", "PATCH", "DELETE", "PUT"];

/// Batch F baseline — every path must remain present in the inventory.
const BASELINE: &[&str] = &[
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-links/",
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-links/:pk/",
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-relation/",
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-attachments/",
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-attachments/:pk/",
    "/api/workspaces/:slug/projects/:project_id/bulk-create-labels/",
    "/api/workspaces/:slug/projects/:project_id/project-estimates/",
    "/api/workspaces/:slug/projects/:project_id/project-views/",
    "/api/workspaces/:slug/users/notifications/:pk/",
    "/api/workspaces/:slug/workspace-themes/",
    "/api/workspaces/:slug/workspace-themes/:pk/",
    "/api/workspaces/:slug/analytics/",
    "/api/workspaces/:slug/saved-analytic-view/:analytic_id/",
    "/api/workspaces/:slug/export-analytics/",
    "/api/workspaces/:slug/analytic-view/:pk/",
    "/api/workspaces/:slug/work-items/:project_identifier-:issue_identifier/",
];

#[test]
fn inventory_json_is_valid() {
    let inv = load_inventory();
    assert_eq!(inv["schema_version"], 1, "schema_version must be 1");
    let domains = inv["domains"].as_object().expect("domains must be an object");
    assert!(!domains.is_empty(), "at least one domain required");
    let mut seen_paths: HashSet<String> = HashSet::new();
    for (domain, d) in domains {
        assert!(d["rust_module"].is_string(), "{domain}: rust_module required");
        let eps = d["endpoints"].as_array().expect("endpoints must be an array");
        assert!(!eps.is_empty(), "{domain}: endpoints must be non-empty");
        for ep in eps {
            let methods = ep["methods"].as_array().expect("methods must be an array");
            assert!(!methods.is_empty(), "{domain}: methods must be non-empty");
            for m in methods {
                let m = m.as_str().expect("method must be a string");
                assert!(VALID_METHODS.contains(&m), "{domain}: invalid method {m}");
            }
            let path = ep["path"].as_str().expect("path must be a string");
            assert!(path.starts_with("/api/"), "{domain}: path must start with /api/: {path}");
            assert!(seen_paths.insert(path.to_string()), "duplicate path: {path}");
            let status = ep["rust_status"].as_str().expect("rust_status must be a string");
            assert!(VALID_STATUSES.contains(&status), "{domain}: invalid rust_status {status} for {path}");
            assert!(ep["out_scope"].is_boolean(), "{domain}: out_scope must be bool for {path}");
            assert!(ep["django_source"].is_string(), "{domain}: django_source required for {path}");
            if let Some(ev) = ep["fe_evidence"].as_array() {
                for e in ev {
                    assert!(e["service"].is_string(), "{domain}: fe_evidence.service must be string for {path}");
                    assert!(e["method"].is_string(), "{domain}: fe_evidence.method must be string for {path}");
                }
            }
        }
    }
}

#[test]
fn implemented_paths_are_registered_in_main_rs() {
    let inv = load_inventory();
    let routes = rust_routes();
    let mut missing = Vec::new();
    for (_domain, ep) in endpoints(&inv) {
        if ep["out_scope"].as_bool().unwrap_or(false) {
            continue;
        }
        if matches!(ep["rust_status"].as_str(), Some("missing" | "shape_mismatch")) {
            continue;
        }
        let path = ep["path"].as_str().expect("path string");
        if !routes.iter().any(|r| r == path) {
            missing.push(path.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "paths claimed non-missing but not registered in main.rs:\n{}",
        missing.join("\n")
    );
}

#[test]
fn baseline_is_present_in_inventory() {
    let inv = load_inventory();
    let mut present = HashSet::new();
    for (_domain, ep) in endpoints(&inv) {
        present.insert(ep["path"].as_str().expect("path string").to_string());
    }
    let absent: Vec<&str> = BASELINE.iter().copied().filter(|p| !present.contains(*p)).collect();
    assert!(absent.is_empty(), "baseline paths missing from inventory:\n{}", absent.join("\n"));
}
