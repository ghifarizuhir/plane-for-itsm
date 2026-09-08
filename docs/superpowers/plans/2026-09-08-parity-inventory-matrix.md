# Parity Inventory Matrix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create `apps/api-rs/crates/api/parity-inventory.json` — a per-domain JSON source of truth for Django↔api-rs route parity — and two gate tests that read it (route-inventory gate + FE-evidence tripwire), replacing the hardcoded `BASELINE` in `route_parity_test.rs`.

**Architecture:** One JSON file at crate root read via `CARGO_MANIFEST_DIR` (same pattern as the existing tests). Shared test-only helpers move into `tests/common/mod.rs`. `route_inventory_test.rs` asserts every non-`missing`, non-`out_scope` inventory path is registered in `main.rs`; `fe_tripwire_test.rs` asserts each `fe_evidence` entry's service file exists, its method name exists, and at least one `/api/` URL in that file matches the endpoint path (wildcard segment matching: `${...}` ↔ `:param`). `route_parity_test.rs` is deleted once its assertions are covered.

**Tech Stack:** Rust (Axum/SQLx crate `api`), `serde_json` (already a normal dep), integration tests under `apps/api-rs/crates/api/tests/`. No new deps, no backend handler changes.

---

## File structure

| File                                                   | Status | Responsibility                                                                                                                                        |
| ------------------------------------------------------ | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/api-rs/crates/api/parity-inventory.json`         | Create | Per-domain parity matrix, seeded with the 16 Batch F baseline paths                                                                                   |
| `apps/api-rs/crates/api/tests/common/mod.rs`           | Create | Shared helpers: `repo_root`, `inventory_path`, `load_inventory`, `rust_routes`, `wildcard_segments`, `segments_match`, `fe_urls_in_file`, `endpoints` |
| `apps/api-rs/crates/api/tests/helper_test.rs`          | Create | Unit tests for the shared helpers                                                                                                                     |
| `apps/api-rs/crates/api/tests/route_inventory_test.rs` | Create | JSON schema validation + implemented-paths-registered gate + baseline tripwire                                                                        |
| `apps/api-rs/crates/api/tests/fe_tripwire_test.rs`     | Create | FE evidence: file exists, method present, URL matches inventory                                                                                       |
| `apps/api-rs/crates/api/tests/route_parity_test.rs`    | Delete | Superseded by `route_inventory_test.rs`                                                                                                               |

---

## Task 0: Seed `parity-inventory.json`

**Files:**

- Create: `apps/api-rs/crates/api/parity-inventory.json`

The 16 Batch F baseline paths, organized by domain. `fe_evidence` is filled only where a real FE caller exists (verified: `issue.service.ts`, `issue_relation.service.ts`, `packages/services/src/workspace/notification.service.ts`).

- [ ] **Step 1: Write the JSON**

```json
{
  "schema_version": 1,
  "domains": {
    "issue": {
      "rust_module": "routes/work_item.rs",
      "endpoints": [
        {
          "methods": ["GET", "POST"],
          "path": "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-links/",
          "django_source": "app/urls/issue.py:109-114",
          "rust_status": "implemented",
          "rust_handler": "routes::work_item::list_links",
          "fe_evidence": [
            { "service": "apps/web/core/services/issue/issue.service.ts", "method": "fetchIssueLinks" },
            { "service": "apps/web/core/services/issue/issue.service.ts", "method": "createIssueLink" }
          ],
          "fe_pages": ["issue-detail"],
          "batch_task": "Batch F T1",
          "out_scope": false,
          "notes": "POST create normalizes URL + dup-400"
        },
        {
          "methods": ["GET", "PATCH", "DELETE"],
          "path": "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-links/:pk/",
          "django_source": "app/urls/issue.py:109-114",
          "rust_status": "implemented",
          "rust_handler": "routes::work_item::get_link",
          "fe_evidence": [
            { "service": "apps/web/core/services/issue/issue.service.ts", "method": "updateIssueLink" },
            { "service": "apps/web/core/services/issue/issue.service.ts", "method": "deleteIssueLink" }
          ],
          "fe_pages": ["issue-detail"],
          "batch_task": "Batch F T1",
          "out_scope": false,
          "notes": "DELETE is soft delete (documented deviation)"
        },
        {
          "methods": ["GET", "POST"],
          "path": "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-relation/",
          "django_source": "app/urls/issue.py:235-244",
          "rust_status": "implemented",
          "rust_handler": "routes::work_item::list_relations",
          "fe_evidence": [
            { "service": "apps/web/core/services/issue/issue_relation.service.ts", "method": "listIssueRelations" },
            { "service": "apps/web/core/services/issue/issue_relation.service.ts", "method": "createIssueRelations" }
          ],
          "fe_pages": ["issue-detail"],
          "batch_task": "Batch F T2",
          "out_scope": false,
          "notes": "GET returns 8 fixed groups; POST 201 array, dups skipped"
        },
        {
          "methods": ["GET"],
          "path": "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-attachments/",
          "django_source": "app/urls/issue.py:126-131",
          "rust_status": "implemented",
          "rust_handler": "routes::asset::issue_attachment_list",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T3",
          "out_scope": false,
          "notes": "GET only; legacy POST-create OUT (V2 presign covers uploads)"
        },
        {
          "methods": ["DELETE"],
          "path": "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-attachments/:pk/",
          "django_source": "app/urls/issue.py:126-131",
          "rust_status": "implemented",
          "rust_handler": "routes::asset::issue_attachment_delete",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T3",
          "out_scope": false,
          "notes": "DELETE only; hard delete; ADMIN-or-creator gate"
        },
        {
          "methods": ["GET"],
          "path": "/api/workspaces/:slug/work-items/:ident/",
          "django_source": "app/urls/issue.py:281",
          "rust_status": "implemented",
          "rust_handler": "routes::work_item::get_by_identifier",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T10",
          "out_scope": false,
          "notes": "constraint enforced in handler (strict int + iexact + member gate); route shape :ident"
        }
      ]
    },
    "label": {
      "rust_module": "routes/label.rs",
      "endpoints": [
        {
          "methods": ["POST"],
          "path": "/api/workspaces/:slug/projects/:project_id/bulk-create-labels/",
          "django_source": "app/urls/issue.py:88",
          "rust_status": "implemented",
          "rust_handler": "routes::label::bulk_create_labels",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T4",
          "out_scope": false,
          "notes": "workspace ADMIN gate; default name Migrated / desc Migrated Issue"
        }
      ]
    },
    "estimate": {
      "rust_module": "routes/estimate.rs",
      "endpoints": [
        {
          "methods": ["GET"],
          "path": "/api/workspaces/:slug/projects/:project_id/project-estimates/",
          "django_source": "app/urls/estimate.py:16",
          "rust_status": "implemented",
          "rust_handler": "routes::estimate::project_estimates",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T5",
          "out_scope": false,
          "notes": "project.estimate_id null -> 200 []"
        }
      ]
    },
    "project": {
      "rust_module": "routes/prefs.rs",
      "endpoints": [
        {
          "methods": ["POST"],
          "path": "/api/workspaces/:slug/projects/:project_id/project-views/",
          "django_source": "app/urls/project.py:92",
          "rust_status": "implemented",
          "rust_handler": "routes::prefs::project_views_post",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T6",
          "out_scope": false,
          "notes": "POST only; non-member 403 {\"error\": \"Forbidden\"}"
        }
      ]
    },
    "notification": {
      "rust_module": "routes/notification.rs",
      "endpoints": [
        {
          "methods": ["GET", "PATCH", "DELETE"],
          "path": "/api/workspaces/:slug/users/notifications/:pk/",
          "django_source": "app/urls/notification.py:22",
          "rust_status": "implemented",
          "rust_handler": "routes::notification::get_notification",
          "fe_evidence": [{ "service": "packages/services/src/workspace/notification.service.ts", "method": "update" }],
          "fe_pages": ["notification"],
          "batch_task": "Batch F T7",
          "out_scope": false,
          "notes": "PATCH hardcodes snoozed_till only; DELETE soft"
        }
      ]
    },
    "workspace": {
      "rust_module": "routes/themes.rs",
      "endpoints": [
        {
          "methods": ["GET", "POST"],
          "path": "/api/workspaces/:slug/workspace-themes/",
          "django_source": "app/urls/workspace.py:122-127",
          "rust_status": "implemented",
          "rust_handler": "routes::themes::list",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T8",
          "out_scope": false,
          "notes": "workspace ADMIN gate"
        },
        {
          "methods": ["GET", "PATCH", "DELETE"],
          "path": "/api/workspaces/:slug/workspace-themes/:pk/",
          "django_source": "app/urls/workspace.py:122-127",
          "rust_status": "implemented",
          "rust_handler": "routes::themes::detail",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T8",
          "out_scope": false,
          "notes": "DELETE soft via deleted_at"
        }
      ]
    },
    "analytics": {
      "rust_module": "routes/analytic.rs",
      "endpoints": [
        {
          "methods": ["GET"],
          "path": "/api/workspaces/:slug/analytics/",
          "django_source": "app/urls/analytic.py:25-45",
          "rust_status": "implemented",
          "rust_handler": "routes::analytic::workspace_analytics",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T9",
          "out_scope": false,
          "notes": "x_axis+y_axis validation, segment != x_axis"
        },
        {
          "methods": ["GET"],
          "path": "/api/workspaces/:slug/saved-analytic-view/:analytic_id/",
          "django_source": "app/urls/analytic.py:25-45",
          "rust_status": "implemented",
          "rust_handler": "routes::analytic::saved_analytic",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T9",
          "out_scope": false,
          "notes": ""
        },
        {
          "methods": ["POST"],
          "path": "/api/workspaces/:slug/export-analytics/",
          "django_source": "app/urls/analytic.py:25-45",
          "rust_status": "implemented",
          "rust_handler": "routes::analytic::export_analytics",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T9",
          "out_scope": false,
          "notes": "Celery export skipped; 200 message only"
        },
        {
          "methods": ["GET", "PATCH", "DELETE"],
          "path": "/api/workspaces/:slug/analytic-view/:pk/",
          "django_source": "app/urls/analytic.py:25-45",
          "rust_status": "implemented",
          "rust_handler": "routes::analytic::analytic_view_detail",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F T9",
          "out_scope": false,
          "notes": "workspace ADMIN gate; DELETE soft"
        }
      ]
    }
  }
}
```

- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): seed parity-inventory.json with Batch F baseline"
```

---

## Task 1: Shared test helpers

**Files:**

- Create: `apps/api-rs/crates/api/tests/common/mod.rs`
- Test: `apps/api-rs/crates/api/tests/helper_test.rs`

- [ ] **Step 1: Write the failing tests**

Create `apps/api-rs/crates/api/tests/helper_test.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --test helper_test`
Expected: FAIL — `error[E0432]: unresolved import common` (module `common` not created yet).

- [ ] **Step 3: Implement the helpers**

Create `apps/api-rs/crates/api/tests/common/mod.rs`:

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --test helper_test`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/tests/common/mod.rs apps/api-rs/crates/api/tests/helper_test.rs
git commit -m "test(rs-api): shared parity-inventory test helpers"
```

---

## Task 2: Route-inventory gate

**Files:**

- Create: `apps/api-rs/crates/api/tests/route_inventory_test.rs`
- Delete: `apps/api-rs/crates/api/tests/route_parity_test.rs`

- [ ] **Step 1: Write the gate tests**

Create `apps/api-rs/crates/api/tests/route_inventory_test.rs`:

```rust
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
    "/api/workspaces/:slug/work-items/:ident/",
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
        if ep["rust_status"].as_str() == Some("missing") {
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
```

- [ ] **Step 2: Run test to verify it passes against the seed**

Run: `cargo test -p api --test route_inventory_test`
Expected: PASS (3 tests) — the JSON seeded in Task 0 satisfies all three.

- [ ] **Step 3: Verify the gate actually fails (mutation check)**

Temporarily remove one baseline path from `parity-inventory.json` (e.g. delete the `"path": ".../issue-links/"` line's object), then:

Run: `cargo test -p api --test route_inventory_test`
Expected: FAIL — `baseline_is_present_in_inventory` lists the removed path.

Restore the removed entry via `git checkout apps/api-rs/crates/api/parity-inventory.json` (or re-add the object), then re-run and confirm PASS.

- [ ] **Step 4: Delete the superseded gate**

```bash
git rm apps/api-rs/crates/api/tests/route_parity_test.rs
```

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/tests/route_inventory_test.rs apps/api-rs/crates/api/parity-inventory.json
git commit -m "test(rs-api): JSON-backed route-inventory gate replaces hardcoded BASELINE"
```

---

## Task 3: FE-evidence tripwire

**Files:**

- Create: `apps/api-rs/crates/api/tests/fe_tripwire_test.rs`

- [ ] **Step 1: Write the failing tests**

Create `apps/api-rs/crates/api/tests/fe_tripwire_test.rs`:

```rust
//! FE-evidence tripwire: every `fe_evidence` entry in parity-inventory.json
//! must point to a real service file that still defines the method and a URL
//! matching the endpoint path. If the FE drops a caller, CI fails loudly.
mod common;

use common::{endpoints, fe_urls_in_file, load_inventory, repo_root, segments_match, wildcard_segments};
use std::path::PathBuf;

#[test]
fn fe_evidence_files_exist() {
    let inv = load_inventory();
    let mut checked = 0;
    for (_domain, ep) in endpoints(&inv) {
        let Some(ev) = ep["fe_evidence"].as_array() else { continue };
        for e in ev {
            let file: PathBuf = repo_root().join(e["service"].as_str().expect("service string"));
            assert!(file.exists(), "FE service file missing: {}", file.display());
            checked += 1;
        }
    }
    assert!(checked >= 4, "expected seeded FE evidence entries, got {checked}");
}

#[test]
fn fe_evidence_methods_exist() {
    let inv = load_inventory();
    for (_domain, ep) in endpoints(&inv) {
        let Some(ev) = ep["fe_evidence"].as_array() else { continue };
        for e in ev {
            let file: PathBuf = repo_root().join(e["service"].as_str().expect("service string"));
            let src = std::fs::read_to_string(&file).unwrap();
            let method = e["method"].as_str().expect("method string");
            assert!(src.contains(method), "{}: method {method} not found", file.display());
        }
    }
}

#[test]
fn fe_evidence_urls_match_inventory() {
    let inv = load_inventory();
    let mut failures = Vec::new();
    for (_domain, ep) in endpoints(&inv) {
        let Some(ev) = ep["fe_evidence"].as_array() else { continue };
        let matrix_path = ep["path"].as_str().expect("path string");
        let matrix_segs = wildcard_segments(matrix_path, false);
        for e in ev {
            let file: PathBuf = repo_root().join(e["service"].as_str().expect("service string"));
            let urls = fe_urls_in_file(&file);
            let matched = urls.iter().any(|u| {
                let fe_segs = wildcard_segments(u, true);
                segments_match(&fe_segs, &matrix_segs)
            });
            if !matched {
                failures.push(format!(
                    "{} ({}): no URL matches {}",
                    file.display(),
                    e["method"].as_str().expect("method string"),
                    matrix_path
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "FE evidence no longer matches inventory:\n{}",
        failures.join("\n")
    );
}
```

- [ ] **Step 2: Run test to verify it passes against the seed**

Run: `cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 tests). If a failure occurs, the failing `{service, method}` entry's evidence is stale — fix the JSON entry (not the test).

- [ ] **Step 3: Verify the tripwire actually fails (mutation check)**

Temporarily change one `method` in `parity-inventory.json` (e.g. `fetchIssueLinks` → `fetchIssueLinksRenamed`), then:

Run: `cargo test -p api --test fe_tripwire_test`
Expected: FAIL — `fe_evidence_methods_exist` reports the method not found.

Restore via `git checkout apps/api-rs/crates/api/parity-inventory.json`, re-run, confirm PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/tests/fe_tripwire_test.rs
git commit -m "test(rs-api): FE-evidence tripwire over parity-inventory.json"
```

---

## Task 4: Final verification

- [ ] **Step 1: Run the full api test suite**

Run: `cargo test -p api`
Expected: 0 failed — includes `helper_test`, `route_inventory_test`, `fe_tripwire_test`, the pre-existing `parity_gate_test` and all unit tests. Confirm `route_parity_test.rs` is gone and no test references it.

- [ ] **Step 2: Confirm parity gate still covers shadow paths**

Run: `cargo test -p api --test parity_gate_test`
Expected: PASS (2 tests; `live_gate_no_404s_or_empty_bodies` stays `#[ignore]` as before).

- [ ] **Step 3: Summary of the resulting source of truth**

- `parity-inventory.json` — 16 baseline endpoints across 7 domains, each with `rust_status`, handler, FE evidence, batch reference.
- `route_inventory_test.rs` — CI fails if a non-`missing`/non-`out_scope` path leaves `main.rs`, or if the schema drifts, or a baseline path is deleted.
- `fe_tripwire_test.rs` — CI fails if FE evidence points at a file/method/URL that no longer exists.

## Out of scope (next batches)

- Expanding the inventory beyond the 16 Batch F baseline paths (done incrementally per domain in Batch G+).
- Implementing backend handlers or flipping `rust_status` for endpoints not yet migrated.
- Auto-scan scripts for extracting routes from Django urls / main.rs / FE services.
- Markdown/HTML report generation from the JSON.

## Self-review notes

- Spec coverage: spec §3 (schema) → Task 0 + `inventory_json_is_valid`; §4.1 (route gate) → Task 2; §4.2 (FE tripwire) → Task 3; §6 (rollout) → Task 0-4 ordering; `route_parity_test.rs` replacement → Task 2 step 4. No spec requirement is left without a task.
- Type consistency: helper names (`load_inventory`, `endpoints`, `rust_routes`, `wildcard_segments`, `segments_match`, `fe_urls_in_file`, `repo_root`, `inventory_path`) are defined in Task 1 and used identically in Tasks 2-3. `rust_status` values in tests match the JSON seed (`implemented`); `VALID_STATUSES`/`VALID_METHODS` cover all values used.
- Mutation checks in Tasks 2-3 prove each gate fails when its invariant is violated, so the tests are not vacuously green.
- FE URL matching relies on segment-position wildcards; `${this.serviceType}` ↔ literal `issues` resolves because the FE `*` absorbs the matrix literal (verified in `helper_test`).
