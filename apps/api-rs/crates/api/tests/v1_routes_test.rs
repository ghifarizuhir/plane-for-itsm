mod common;

#[test]
fn v1_project_object_routes_registered() {
    let src = std::fs::read_to_string(
        std::path::Path::new(common::MANIFEST_DIR).join("src/main.rs"),
    )
    .expect("main.rs");
    for path in [
        "/api/v1/workspaces/:slug/projects/",
        "/api/v1/workspaces/:slug/projects-lite/",
        "/api/v1/workspaces/:slug/projects/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/archive/",
    ] {
        assert!(src.contains(path), "missing v1 route: {path}");
    }
}

#[test]
fn v1_work_item_routes_registered() {
    let src = std::fs::read_to_string(
        std::path::Path::new(common::MANIFEST_DIR).join("src/main.rs"),
    )
    .expect("main.rs");
    for path in [
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:pk/archive/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:pk/unarchive/",
        "/api/v1/workspaces/:slug/projects/:project_id/archived-work-items/",
        "/api/v1/workspaces/:slug/work-items/",
        "/api/v1/workspaces/:slug/work-items/count/",
        "/api/v1/workspaces/:slug/work-items/search/",
        "/api/v1/workspaces/:slug/work-items/:ident/",
    ] {
        assert!(src.contains(path), "missing v1 route: {path}");
    }
}
