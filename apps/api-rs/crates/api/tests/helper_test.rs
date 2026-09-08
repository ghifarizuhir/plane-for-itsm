mod common;

use common::{fe_urls_in_file, inventory_path, repo_root, rust_routes, segments_match, wildcard_segments};

#[test]
fn inventory_file_exists() {
    assert!(inventory_path().exists(), "parity-inventory.json missing");
}

#[test]
fn repo_root_points_to_workspace_root() {
    let root = repo_root();
    assert!(root.join("apps").exists(), "repo_root must contain apps/");
    assert!(root.join("packages").exists(), "repo_root must contain packages/");
}

#[test]
fn rust_routes_parses_main_rs() {
    let routes = rust_routes();
    assert!(routes.iter().any(|r| r == "/api/users/me/"), "missing /api/users/me/");
    assert!(routes.iter().any(|r| r.starts_with("/api/workspaces/")), "no workspace routes parsed");
}

#[test]
fn wildcard_segments_normalizes_params_and_templates() {
    let fe = wildcard_segments("/api/workspaces/${workspaceSlug}/projects/${projectId}/issues/${issueId}/issue-relation/", true);
    assert_eq!(fe, ["api", "workspaces", "*", "projects", "*", "issues", "*", "issue-relation"]);
    let matrix = wildcard_segments("/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-relation/", false);
    assert_eq!(matrix, ["api", "workspaces", "*", "projects", "*", "issues", "*", "issue-relation"]);
}

#[test]
fn segments_match_handles_service_type_segment() {
    let fe = wildcard_segments("/api/workspaces/${workspaceSlug}/projects/${projectId}/${this.serviceType}/${issueId}/issue-relation/", true);
    let matrix = wildcard_segments("/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-relation/", false);
    assert!(segments_match(&fe, &matrix), "serviceType wildcard must absorb issues literal");
}

#[test]
fn segments_match_rejects_unrelated_paths() {
    let fe = wildcard_segments("/api/workspaces/${workspaceSlug}/projects/${projectId}/states/", true);
    let matrix = wildcard_segments("/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-relation/", false);
    assert!(!segments_match(&fe, &matrix), "different path must not match");
}

#[test]
fn fe_urls_in_file_finds_api_urls() {
    let root = repo_root();
    let file = root.join("apps/web/core/services/issue/issue_relation.service.ts");
    let urls = fe_urls_in_file(&file);
    assert!(urls.iter().any(|u| u.contains("issue-relation")), "should find issue-relation URL, got: {urls:?}");
}
