mod common;

#[test]
fn v1_project_object_routes_registered() {
    let src = std::fs::read_to_string(
        std::path::Path::new(common::MANIFEST_DIR).join("src/main.rs"),
    )
    .expect("main.rs");
    for path in [
        "/api/v1/workspaces/:slug/projects/",
        "/api/v1/workspaces/:slug/projects/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/archive/",
    ] {
        assert!(src.contains(path), "missing v1 route: {path}");
    }
}
