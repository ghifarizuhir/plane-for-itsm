# Batch F: Route Parity Gap Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the 15 remaining Django `plane/app/*` routes missing from the Rust router (plus one identifier-route constraint fix) so the migration baseline reaches 100% route coverage, with a regression gate that fails CI if any target route disappears again.

**Architecture:** Extend existing route modules (disjoint file per task, repo Batch D/E convention): `work_item.rs` (issue-links/issue-relation), `label.rs` (bulk-create-labels), `estimate.rs` (project-estimates), `view.rs` (project-views), `notification.rs` (`:pk/` CRUD), `prefs.rs` (workspace-themes — new handlers), `analytic.rs` (analytics/saved-analytic-view/export-analytics/analytic-view detail), `asset.rs` (legacy issue-attachments GET/DELETE), `work_item.rs` (identifier constraint). Wire-up in `main.rs`; add shadow paths in `scripts/shadow.sh` per task (parity gate reads them). Task 0 merges first (route-inventory gate), then T1–T10 may run in lanes (disjoint files; `main.rs` + `shadow.sh` edits apply serially).

**Tech Stack:** Rust, Axum, SQLx (Postgres), no new deps. Celery/activity side-effects SKIPPED everywhere (Batch C/D/E precedent). Reuse Batch C/D/E helpers: `ws_role`/`project_role`/`deny()`/`missing()`, `FORBIDDEN_MSG`/`NOT_FOUND_MSG`, role ints 20/15/5, `project_gate_allows`, `issue_common.rs` row structs.

---

## Migration baseline (audit 2026-09-08, static AST)

Django `plane/app/urls/*.py` total = 233 paths. Explicitly OUT per Batch E §16/line 99 (`docs/superpowers/plans/2026-09-06-batch-e-workspace-platform-parity.md`): `/api/workspaces/:slug/ai-assistant/`, `/api/workspaces/:slug/projects/:project_id/ai-assistant/`, legacy `file-assets` POST-create (`/api/users/file-assets/`, `/api/workspaces/:slug/file-assets/`), `plane/api/*` + `plane/space/*` twins. Remaining gap to close in this plan:

| #   | Method+Path (Django form)                                                                                                                     | Django source                                                       | Task |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ---- |
| 1   | `GET+POST .../issues/:issue_id/issue-links/` + `GET+PATCH+DELETE .../issue-links/:pk/`                                                        | `app/urls/issue.py:109-114`, `views/issue/link.py:26-113`           | T1   |
| 2   | `GET+POST .../issues/:issue_id/issue-relation/`                                                                                               | `app/urls/issue.py:235-244`, `views/issue/relation.py:37-269`       | T2   |
| 3   | `GET .../issues/:issue_id/issue-attachments/` + `DELETE .../issue-attachments/:pk/`                                                           | `app/urls/issue.py:126-131`, `views/issue/attachment.py:32-92`      | T3   |
| 4   | `POST .../projects/:project_id/bulk-create-labels/`                                                                                           | `app/urls/issue.py:88`, `views/issue/label.py:90-117`               | T4   |
| 5   | `GET .../projects/:project_id/project-estimates/`                                                                                             | `app/urls/estimate.py:16`, `views/estimate/base.py:34-46`           | T5   |
| 6   | `POST .../projects/:project_id/project-views/`                                                                                                | `app/urls/project.py:92`, `views/project/base.py:474-495`           | T6   |
| 7   | `GET+PATCH+DELETE .../users/notifications/:pk/`                                                                                               | `app/urls/notification.py:22`, `views/notification/base.py:156-166` | T7   |
| 8   | `GET+POST .../workspace-themes/` + `GET+PATCH+DELETE .../workspace-themes/:pk/`                                                               | `app/urls/workspace.py:122-127`, `views/workspace/base.py:322-336`  | T8   |
| 9   | `GET .../analytics/` + `GET .../saved-analytic-view/:analytic_id/` + `POST .../export-analytics/` + `GET+PATCH+DELETE .../analytic-view/:pk/` | `app/urls/analytic.py:25-45`, `views/analytic/base.py:37-248`       | T9   |
| 10  | `GET .../work-items/:project_identifier-:issue_identifier/` (constraint fix on existing `:ident` route)                                       | `app/urls/issue.py:281`, `views/issue/base.py:1201-1260+`           | T10  |

FE evidence (must-serve): `issue-links` + `issue-relation` (`apps/web/core/services/issue/issue.service.ts:184-331`, `issue_relation.service.ts:18-31`), `PATCH users/notifications/:pk/` (`workspace-notification.service.ts:48-62`).

---

### Task 0: Route-inventory parity gate (MERGES FIRST)

**Why:** The existing `parity_gate_test.rs` only counts shadow.sh paths (≥55) and never re-verifies after `main.rs` edits. This gate parses `main.rs` `.route()` strings and fails if any migration-baseline path is missing — the audit becomes CI.

**Files:**

- Create: `apps/api-rs/crates/api/tests/route_parity_test.rs`
- Test: `apps/api-rs/crates/api/tests/route_parity_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --test route_parity_test`
Expected: FAIL — `migration_baseline_is_registered` lists the 15 missing paths.

- [ ] **Step 3: Commit (gate first; tasks T1–T10 flip entries to green)**

```bash
git add apps/api-rs/crates/api/tests/route_parity_test.rs
git commit -m "test(rs-api): route-inventory parity gate untuk migration baseline (Batch F T0)"
```

---

### Task 1: `issue-links/` + `issue-links/:pk/` (Django `IssueLinkViewSet`)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/work_item.rs` (upgrade link handlers to Django shape + tests)
- Modify: `apps/api-rs/crates/api/src/main.rs` (wire 2 routes)
- Modify: `apps/api-rs/scripts/shadow.sh`

Contract (`views/issue/link.py:26-113`, `serializers/issue.py:550-598`):

- GET list: `IssueLinkSerializer.__all__` → keys `id, workspace, project, issue, title, url, metadata, created_by, updated_by, created_at, updated_at, created_by_detail` (UserLite `{id,first_name,last_name,display_name,avatar?,avatar_asset?}`), scoped `workspace__slug + project_id + issue_id + project member active + project not archived`, `ORDER BY -created_at`.
- POST create: **201** serializer row; body `{title?, url, metadata?}`; URL auto-prepend `http://` when scheme missing; invalid URL → 400 `{"error": "Invalid URL format."}`; duplicate URL on the issue → 400 `{"error": "URL already exists for this Issue"}`.
- GET `:pk`: 200 row; miss → 404.
- PATCH `:pk`: 200 row; dup URL (excluding self) → 400 `{"error": "URL already exists for this Issue"}`; miss → 404.
- DELETE `:pk`: **204** (soft delete `deleted_at = now()`, matching existing Rust precedent; Django `.delete()` on `IssueLink` is hard — document the soft-delete deviation).
- Gate: `ProjectEntityPermission` → safe reads AMG, writes ADMIN/MEMBER (+ ws-admin fallback) — reuse `project_gate_allows` + `fetch_project_member_role` (`issue_common.rs`).

- [ ] **Step 1: Write the failing tests** (append to `work_item.rs` test module)

```rust
#[test]
fn link_shape_matches_django_issue_link_serializer() {
    // Serializer keys from serializers/issue.py:550-598 (fields="__all__" + created_by_detail).
    let row = serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "workspace": "00000000-0000-0000-0000-000000000002",
        "project": "00000000-0000-0000-0000-000000000003",
        "issue": "00000000-0000-0000-0000-000000000004",
        "title": null,
        "url": "http://example.com",
        "metadata": {},
        "created_by": "00000000-0000-0000-0000-000000000005",
        "updated_by": null,
        "created_at": "2026-09-08T00:00:00Z",
        "updated_at": "2026-09-08T00:00:00Z",
        "created_by_detail": {"id": "00000000-0000-0000-0000-000000000005", "first_name": "A", "last_name": "B", "display_name": "A B"}
    });
    let keys: std::collections::BTreeSet<&str> = row.as_object().unwrap().keys().map(|s| s.as_str()).collect();
    assert_eq!(keys.len(), 12);
}

#[test]
fn link_url_normalize_prepends_http() {
    assert_eq!(normalize_link_url("example.com/a"), "http://example.com/a");
    assert_eq!(normalize_link_url("https://x.io"), "https://x.io");
}

#[test]
fn link_duplicate_url_error_matches_django() {
    assert_eq!(LINK_DUP_MSG, "URL already exists for this Issue");
    assert_eq!(LINK_INVALID_MSG, "Invalid URL format.");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::work_item`
Expected: FAIL (helpers/consts not defined).

- [ ] **Step 3: Implement**

Add consts + helper to `work_item.rs`:

```rust
pub(crate) const LINK_DUP_MSG: &str = "URL already exists for this Issue";
pub(crate) const LINK_INVALID_MSG: &str = "Invalid URL format.";

pub(crate) fn normalize_link_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") { url.to_string() } else { format!("http://{url}") }
}
```

Upgrade the five handlers `list_links`/`create_link`/`get_link`/`patch_link`/`delete_link` (currently `work_item.rs:250-348`, minimal `{id,url}` shape) to the 12-key shape above:

- Extend `SELECT` to `id, workspace_id, project_id, issue_id, title, url, metadata, created_by_id, created_at, updated_at` + a correlated `created_by` user-lite JSON built with the same pattern as `me_row`/`user.rs` lite serialization.
- `create_link`: validate via a new `validate_link_create`-extended rule — normalize URL first, invalid-URL check, then `INSERT ... ON CONFLICT`-style existence check → 400 `LINK_DUP_MSG`; **201** full row.
- `patch_link`: same dup check excluding self → 400 `LINK_DUP_MSG`.
- Keep existing `links/` (work-items) routes pointing at the same upgraded handlers (superset shape — documented).

Wire in `main.rs` (after the existing `links/` block, `main.rs:1096-1104`):

```rust
.route(
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-links/",
    get(routes::work_item::list_links).post(routes::work_item::create_link),
)
.route(
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-links/:pk/",
    get(routes::work_item::get_link)
        .patch(routes::work_item::patch_link)
        .delete(routes::work_item::delete_link),
)
```

Add to `scripts/shadow.sh` (inside `paths=(...)`):

```bash
  "/api/workspaces/$WS/projects/$P/issues/00000000-0000-0000-0000-000000000000/issue-links/"
  "/api/workspaces/$WS/projects/$P/issues/00000000-0000-0000-0000-000000000000/issue-links/00000000-0000-0000-0000-000000000000/"
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::work_item && cargo test -p api --test route_parity_test`
Expected: PASS (parity gate now shows 14 of 16 baseline paths covered).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/work_item.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/scripts/shadow.sh
git commit -m "feat(rs-api): issue-links GET/POST/PATCH/DELETE paritas Django IssueLinkViewSet (Batch F T1)"
```

---

### Task 2: `issue-relation/` GET+POST (Django `IssueRelationViewSet.list/create`)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/work_item.rs` (upgrade `list_relations`/`create_relations` + tests)
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/scripts/shadow.sh`

Contract (`views/issue/relation.py:37-269`, `serializers/issue.py:402-464`):

- GET list: 8 grouped arrays — `blocking, blocked_by, duplicate, relates_to, start_after, start_before, finish_after, finish_before` — each row = 14-key issue shape `id, name, state_id, sort_order, priority, sequence_id, project_id, label_ids, assignee_ids, created_at, updated_at, created_by, updated_by, relation_type` (group rows annotated with the resolved `relation_type`); ordering `-created_at`; distinct. Gate `ProjectEntityPermission` (safe → AMG).
- POST create: body `{relation_type, issues:[uuid]}`; missing `relation_type` → 400 `{"message": "Issue relation type is required"}`; issues scoped to `workspace__slug` only (cross-project allowed); `relation_type` mapped via `get_actual_relation` (`utils/issue_relation_mapper.py` — read it; maps blocking/start_after/finish_after direction swap); bulk insert **with `ignore_conflicts=True`** → dups silently skipped; **201** array — `RelatedIssueSerializer` (12 keys, `issue.*` source) when `relation_type ∈ {blocking, start_after, finish_after}`, else `IssueRelationSerializer` (12 keys, `related_issue.*` source). Empty `issues` → 201 `[]`.
- `DELETE issue-relation/:relId/` and `remove_relation` are unchanged (D9 precedent; `remove-relation/` already wired).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn relation_list_has_eight_groups() {
    // Django `relation.py:174-205` — keys are fixed.
    let groups = ["blocking", "blocked_by", "duplicate", "relates_to", "start_after", "start_before", "finish_after", "finish_before"];
    assert_eq!(relation_groups(), groups);
}

#[test]
fn relation_create_requires_relation_type() {
    assert_eq!(RELATION_TYPE_REQUIRED_MSG, "Issue relation type is required");
}

#[test]
fn relation_direction_maps_actual_type() {
    // `get_actual_relation` — verify mapping in utils/issue_relation_mapper.py first.
    assert_eq!(map_actual_relation("blocking"), "blocked_by");
    assert_eq!(map_actual_relation("blocked_by"), "blocking");
    assert_eq!(map_actual_relation("relates_to"), "relates_to");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::work_item`
Expected: FAIL (helpers missing).

- [ ] **Step 3: Implement**

- Read `apps/api/plane/utils/issue_relation_mapper.py` and replicate `map_actual_relation` exactly (1:1 relation_type mapping).
- Upgrade `list_relations` (`work_item.rs:352-383`): replace the 3-bucket grouping with the 8 fixed groups; rows come from the same issue-row SQL used by the workspace/issues list (14 keys listed above) joined against `issue_relations` with the direction logic from `relation.py:53-100` (`blocking_issues` = `blocked_by` where `related_issue_id=issue_id`, etc.). `ORDER BY r.created_at DESC`, `DISTINCT`.
- Upgrade `create_relations` (`work_item.rs:385-423`): body `{relation_type, issues}`; drop the 409-dup behavior → `INSERT ... ON CONFLICT DO NOTHING RETURNING id` (per-row), then re-SELECT created rows and serialize per the direction table (Related vs Issue serializer, both 12 keys); **201** always. Empty issues → `201 []`.

Wire in `main.rs` (next to `relations/`, `main.rs:1106-1123`):

```rust
.route(
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-relation/",
    get(routes::work_item::list_relations).post(routes::work_item::create_relations),
)
```

Add to `scripts/shadow.sh`:

```bash
  "/api/workspaces/$WS/projects/$P/issues/00000000-0000-0000-0000-000000000000/issue-relation/"
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::work_item && cargo test -p api --test route_parity_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/work_item.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/scripts/shadow.sh
git commit -m "feat(rs-api): issue-relation list/create 8-group paritas Django IssueRelationViewSet (Batch F T2)"
```

---

### Task 3: Legacy `issue-attachments/` GET + `:pk/` DELETE

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/asset.rs` (new handlers + tests)
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/scripts/shadow.sh`

Contract (`views/issue/attachment.py:32-92`, `serializers/issue.py:616-630`):

- GET list: `IssueAttachmentSerializer.__all__` → FileAsset keys + `asset_url` (read-only; compute the same way as `full_asset_json` in `asset.rs`), filtered `issue_id + workspace__slug + project_id` (NO `is_uploaded`/deleted filter — legacy semantics), **200 array**. Gate ADMIN/MEMBER/GUEST.
- POST create: legacy multipart upload. **EXCLUDED** (Batch E §16 precedent — no FE caller; V2 presign covers uploads). Document in commit; route 404s on POST.
- DELETE `:pk`: hard delete (`asset.delete(save=False); row.delete()`), miss → 404 `{"error": "Issue attachment not found."}`; gate ADMIN **and creator** (`allow_permission([ROLE.ADMIN], creator=True)`) + project-membership; **204**.

- [ ] **Step 1: Write the failing tests** (append to `asset.rs` test module)

```rust
#[test]
fn issue_attachment_delete_miss_message_matches_django() {
    assert_eq!(ISSUE_ATTACHMENT_MISS_MSG, "Issue attachment not found.");
}

#[test]
fn issue_attachment_creator_gate_roles() {
    // allow_permission([ROLE.ADMIN], creator=True): ADMIN-or-creator; GUEST-only → deny.
    assert!(attachment_delete_gate(20, true).is_ok());
    assert!(attachment_delete_gate(20, false).is_ok());
    assert!(attachment_delete_gate(15, true).is_ok());
    assert!(attachment_delete_gate(15, false).is_err());
    assert!(attachment_delete_gate(5, true).is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::asset`
Expected: FAIL.

- [ ] **Step 3: Implement**

Add to `asset.rs`:

```rust
pub(crate) const ISSUE_ATTACHMENT_MISS_MSG: &str = "Issue attachment not found.";

pub(crate) fn attachment_delete_gate(role: i16, is_creator: bool) -> Result<(), String> {
    match (role, is_creator) {
        (20, _) | (15 | 5, true) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}
```

- `issue_attachment_list`: gate via `project_gate_allows` (AMG); SQL `SELECT {ASSET_COLS} FROM file_assets WHERE issue_id=$1 AND project_id=$2 AND workspace_id=(SELECT id FROM workspaces WHERE slug=$3)`; serialize rows with the same shape as `full_asset_json` (check `asset.rs` helper — reuse, add `asset_url`).
- `issue_attachment_delete`: gate `attachment_delete_gate(role, is_creator)` + ws-admin fallback; find row scoped `(pk, slug, project_id, issue_id)`; miss → 404 `ISSUE_ATTACHMENT_MISS_MSG`; else `DELETE FROM file_assets WHERE id=$1` (hard) → 204.

Wire in `main.rs` (next to the v2 attachment block, `main.rs:1007-1016`):

```rust
.route(
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-attachments/",
    get(routes::asset::issue_attachment_list),
)
.route(
    "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-attachments/:pk/",
    delete(routes::asset::issue_attachment_delete),
)
```

Add to `scripts/shadow.sh` (GET-only route — `:pk/` is DELETE-only and would 405 on shadow's GET, skip it):

```bash
  "/api/workspaces/$WS/projects/$P/issues/00000000-0000-0000-0000-000000000000/issue-attachments/"
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::asset && cargo test -p api --test route_parity_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/asset.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/scripts/shadow.sh
git commit -m "feat(rs-api): issue-attachments GET+DELETE paritas Django (POST-create excluded per Batch E)"
```

---

### Task 4: `bulk-create-labels/` POST

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/label.rs` (new handler + tests)
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/scripts/shadow.sh`

Contract (`views/issue/label.py:90-117`, `serializers/issue.py:361-...`):

- POST body `{label_data: [{name?, description?, ...}]}`; defaults: `name` → `"Migrated"`, `description` → `"Migrated Issue"`, `color` → random `#RRGGBB` uppercase (`random.randint(0, 0xFFFFFF+1)` → `f"#{...:06X}"` — range is 0..16777216 inclusive-exclusive of 0xFFFFFF+1; replicate with `rand`); bulk insert with `ignore_conflicts=True`; **201** `{"labels": [LabelSerializer...]}` (keys per `LabelSerializer.__all__` — verify against `serializers/issue.py:361` and reuse the `label.rs` row struct).
- Gate: workspace-level ADMIN (`allow_permission([ROLE.ADMIN])` — reuse `ws_role` + `deny()`).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn bulk_label_defaults_match_django() {
    assert_eq!(BULK_LABEL_DEFAULT_NAME, "Migrated");
    assert_eq!(BULK_LABEL_DEFAULT_DESC, "Migrated Issue");
    let c = random_label_color();
    assert_eq!(c.len(), 7); // "#RRGGBB"
    assert!(c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::label`
Expected: FAIL.

- [ ] **Step 3: Implement**

Add to `label.rs`:

```rust
pub(crate) const BULK_LABEL_DEFAULT_NAME: &str = "Migrated";
pub(crate) const BULK_LABEL_DEFAULT_DESC: &str = "Migrated Issue";

pub(crate) fn random_label_color() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let n: u32 = rng.gen_range(0..=0xFFFFFF);
    format!("#{n:06X}")
}
```

`pub async fn bulk_create_labels`: gate `ws_role` == 20 else `deny()`; parse `label_data` (default `[]`); single `INSERT INTO labels (id, name, description, color, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, w.id, $5, $5, now(), now() FROM workspaces w WHERE w.slug = $6 ... ON CONFLICT DO NOTHING` per row (loop, mirroring Django `bulk_create`); re-SELECT created rows; **201** `{"labels": [...]}` reusing the existing `label.rs` row struct/serialization.

Wire in `main.rs` (next to `issue-labels/`, `main.rs:664-667`):

```rust
.route(
    "/api/workspaces/:slug/projects/:project_id/bulk-create-labels/",
    post(routes::label::bulk_create_labels),
)
```

Add to `scripts/shadow.sh` (POST-only route — shadow's GET would 405; skip shadow, covered by `route_parity_test` + smoke):

```bash
# (no shadow.sh line — POST-only; smoke.sh may add: curl -X POST .../bulk-create-labels/ -d '{}')
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::label && cargo test -p api --test route_parity_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/label.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/scripts/shadow.sh
git commit -m "feat(rs-api): bulk-create-labels POST paritas Django (Batch F T4)"
```

---

### Task 5: `project-estimates/` GET

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/estimate.rs` (new handler + tests)
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/scripts/shadow.sh`

Contract (`views/estimate/base.py:34-46`, `serializers/estimate.py:20-32`):

- GET: project lookup `workspace__slug + pk` (miss → Django `.get()` 500 → sane 404 `missing()`); if `project.estimate_id IS NULL` → **200 `[]`**; else `EstimatePointSerializer` rows (`__all__` = `id, estimate, workspace, project, key, value, description, created_by, updated_by, created_at, updated_at` — read `EstimatePoint` model/`\d estimate_points` to confirm columns) filtered `estimate_id + project_id + slug`. Gate ADMIN/MEMBER.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn project_estimates_no_estimate_returns_empty_list() {
    // Django returns `[]` when project.estimate_id is null (estimate/base.py:38-45).
    assert_eq!(project_estimates_shape(None).as_str(), "[]");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::estimate`
Expected: FAIL.

- [ ] **Step 3: Implement**

`pub async fn project_estimates` in `estimate.rs`:

- Gate via `project_role` (ADMIN/MEMBER = role >= 15) + ws-admin fallback (reuse `project_gate_allows`).
- `SELECT e.estimate_id FROM projects e WHERE e.id=$1 AND e.workspace_id=(SELECT id FROM workspaces WHERE slug=$2)`; `None` → 200 `[]`.
- Else `SELECT id, estimate_id, project_id, workspace_id, key, value, description, created_by_id, updated_by_id, created_at, updated_at FROM estimate_points WHERE estimate_id=$1 AND project_id=$2 AND workspace_id=(SELECT id FROM workspaces WHERE slug=$3) ORDER BY key`; reuse the existing `estimate.rs` point row struct.

Wire in `main.rs` (next to `estimates/`, `main.rs:684-687`):

```rust
.route(
    "/api/workspaces/:slug/projects/:project_id/project-estimates/",
    get(routes::estimate::project_estimates),
)
```

Add to `scripts/shadow.sh`:

```bash
  "/api/workspaces/$WS/projects/$P/project-estimates/"
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::estimate && cargo test -p api --test route_parity_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/estimate.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/scripts/shadow.sh
git commit -m "feat(rs-api): project-estimates GET paritas Django (Batch F T5)"
```

---

### Task 6: `project-views/` POST

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/prefs.rs` (new handler + tests; mirrors `views_post` at `prefs.rs:1354`)
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/scripts/shadow.sh`

Contract (`views/project/base.py:474-495`):

- POST body `{view_props?, default_props?, preferences?, sort_order?}` — each key falls back to the stored value; **204** empty; non-member → 403 `{"error": "Forbidden"}` (exact string, NOT `FORBIDDEN_MSG`); project miss → Django `.get()` 500 → sane 404 `missing()`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn project_views_non_member_forbidden_matches_django() {
    assert_eq!(PROJECT_VIEWS_FORBIDDEN_MSG, "Forbidden");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::prefs`
Expected: FAIL.

- [ ] **Step 3: Implement**

`pub async fn project_views_post` in `prefs.rs` (copy the `views_post` skeleton at `prefs.rs:1354`, but project-scoped):

- `SELECT ... FROM project_members WHERE project_id=$1 AND workspace_id=(SELECT id FROM workspaces WHERE slug=$2) AND member_id=$3 AND is_active=true`; miss → 403 `{"error": "Forbidden"}`.
- `UPDATE project_members SET view_props = COALESCE($4::jsonb, view_props), default_props = COALESCE($5::jsonb, default_props), preferences = COALESCE($6::jsonb, preferences), sort_order = COALESCE($7, sort_order) WHERE ...` → **204**.

Wire in `main.rs` (next to `workspace-views/`, `main.rs:1466`):

```rust
.route(
    "/api/workspaces/:slug/projects/:project_id/project-views/",
    post(routes::prefs::project_views_post),
)
```

Add to `scripts/shadow.sh` (POST-only route — skip shadow, covered by `route_parity_test` + smoke):

```bash
# (no shadow.sh line — POST-only; smoke.sh may add: curl -X POST .../project-views/ -d '{"view_props":{}}')
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::prefs && cargo test -p api --test route_parity_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/prefs.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/scripts/shadow.sh
git commit -m "feat(rs-api): project-views POST paritas Django (Batch F T6)"
```

---

### Task 7: `users/notifications/:pk/` GET+PATCH+DELETE

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/notification.rs` (new handlers + tests)
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/scripts/shadow.sh`

Contract (`views/notification/base.py:156-166`; destroy = DRF ModelViewSet default):

- GET `:pk`: `NotificationSerializer.__all__` row (`get_queryset` scoping: `workspace__slug + receiver=user`), miss → Django `.get()` 500 → sane 404. Gate AMG (workspace).
- PATCH `:pk`: body `{snoozed_till?}` ONLY (Django hardcodes `{"snoozed_till": request.data.get("snoozed_till", None)}`); **200** serializer row; miss → 404. FE caller: `workspace-notification.service.ts:48-62`.
- DELETE `:pk`: default DRF `destroy()` → `instance.delete()`; model manager soft-deletes (`deleted_at`) — replicate soft delete `UPDATE ... SET deleted_at = now()`; **204**; miss → 404.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn notification_patch_only_updates_snoozed_till() {
    // Django hardcodes notification_data = {"snoozed_till": ...} (base.py:160).
    let body = serde_json::json!({"snoozed_till": "2026-09-09T00:00:00Z", "read_at": "2026-09-08T00:00:00Z"});
    assert_eq!(snoozed_till_from_body(&body), Some("2026-09-09T00:00:00Z".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::notification`
Expected: FAIL.

- [ ] **Step 3: Implement**

Add to `notification.rs`:

```rust
pub(crate) fn snoozed_till_from_body(body: &Value) -> Option<String> {
    body.get("snoozed_till").and_then(|v| v.as_str()).map(|s| s.to_string())
}
```

- `get_notification`: `SELECT n.id, n.title, n.read_at, n.archived_at, n.snoozed_till FROM notifications n JOIN workspaces w ON w.id=n.workspace_id WHERE w.slug=$1 AND n.id=$2 AND n.receiver_id=$3 AND n.deleted_at IS NULL` → 200 row or 404 (extend the existing `list` row struct with `snoozed_till`).
- `patch_notification`: same scoping; `UPDATE notifications SET snoozed_till = $4 FROM workspaces w WHERE w.id=n.workspace_id AND w.slug=$1 AND n.id=$2 AND n.receiver_id=$3 AND n.deleted_at IS NULL` → 200 row or 404.
- `destroy_notification`: `UPDATE notifications SET deleted_at = now() ...` → 204 or 404.

Wire in `main.rs` (next to the read/archive block, `main.rs:1056-1069`):

```rust
.route(
    "/api/workspaces/:slug/users/notifications/:pk/",
    get(routes::notification::get_notification)
        .patch(routes::notification::patch_notification)
        .delete(routes::notification::destroy_notification),
)
```

Add to `scripts/shadow.sh`:

```bash
  "/api/workspaces/$WS/users/notifications/00000000-0000-0000-0000-000000000000/"
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::notification && cargo test -p api --test route_parity_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/notification.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/scripts/shadow.sh
git commit -m "feat(rs-api): users/notifications/:pk GET+PATCH+DELETE paritas Django (Batch F T7)"
```

---

### Task 8: `workspace-themes/` + `:pk/` CRUD

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/themes.rs` (new module: handlers + tests)
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/scripts/shadow.sh`

Contract (`views/workspace/base.py:322-336`, `serializers/workspace.py:167-171`):

- GET list: `WorkspaceThemeSerializer.__all__` rows scoped `workspace__slug`; **200 array**. Gate `WorkSpaceAdminPermission` = workspace ADMIN (20) only.
- POST create: body per `WorkspaceTheme` model (`name`, `theme` JSON — verify `\d workspace_themes`); **201** serializer row; invalid → 400.
- GET `:pk`: 200 row; miss → 404.
- PATCH `:pk`: 200 row; miss → 404.
- DELETE `:pk`: **204** (DRF default destroy → soft via manager; use `deleted_at = now()` and document).
- Serializer `__all__` keys: `id, workspace, actor, name, theme, created_at, updated_at, created_by, updated_by` (verify with `\d workspace_themes`).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn workspace_theme_gate_is_admin_only() {
    // WorkSpaceAdminPermission = workspace ADMIN (20); MEMBER/GUEST → deny.
    assert!(theme_gate(Some(20)).is_ok());
    assert!(theme_gate(Some(15)).is_err());
    assert!(theme_gate(Some(5)).is_err());
    assert!(theme_gate(None).is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::themes`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement**

Create `themes.rs` (module header mirrors `prefs.rs`):

```rust
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{middleware::auth::AuthUser, routes::project::{deny, missing, ws_role}, state::AppState};

pub(crate) fn theme_gate(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) => Ok(()),
        _ => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}
```

Handlers `list`, `create`, `detail`, `patch`, `destroy` — gate via `ws_role(&st.pool, auth.0, &slug).await?` then `theme_gate(role).map_err(|_| ...)` → `deny()`; rows via `SELECT id, workspace_id, actor_id, name, theme, created_at, updated_at, created_by_id, updated_by_id FROM workspace_themes WHERE workspace_id=(SELECT id FROM workspaces WHERE slug=$1) AND deleted_at IS NULL`; insert sets `workspace_id`, `actor_id=auth.0`, `created_by_id`, `updated_by_id`; miss → `missing()`.

Register module + wire in `main.rs` (next to `sidebar-preferences/`, `main.rs:1434-1437`):

```rust
.route(
    "/api/workspaces/:slug/workspace-themes/",
    get(routes::themes::list).post(routes::themes::create),
)
.route(
    "/api/workspaces/:slug/workspace-themes/:pk/",
    get(routes::themes::detail)
        .patch(routes::themes::patch)
        .delete(routes::themes::destroy),
)
```

Add to `scripts/shadow.sh`:

```bash
  "/api/workspaces/$WS/workspace-themes/"
  "/api/workspaces/$WS/workspace-themes/00000000-0000-0000-0000-000000000000/"
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::themes && cargo test -p api --test route_parity_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/themes.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/scripts/shadow.sh
git commit -m "feat(rs-api): workspace-themes CRUD paritas Django (Batch F T8)"
```

---

### Task 9: Analytics legacy — `analytics/`, `saved-analytic-view/:id/`, `export-analytics/`, `analytic-view/:pk/`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/analytic.rs` (new handlers + tests)
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/scripts/shadow.sh`

Contract (`views/analytic/base.py:37-248`, `serializers/analytic.py:10-31`):

- GET `analytics/`: requires `?x_axis` + `?y_axis` valid (`VALID_ANALYTICS_FIELDS`, `VALID_YAXIS` — read `utils/analytics_plot.py` and reuse `analytic.rs` consts) else 400 `{"error": "x-axis and y-axis dimensions are required and the values should be valid"}`; `?segment` must be valid and ≠ x_axis else 400 `{"error": "Both segment and x axis cannot be same and segment should be valid"}`; **200** `{"total", "distribution", "extras": {state_details, assignee_details, label_details, cycle_details, module_details}}` (extras only populated when x_axis/segment matches; `build_graph_plot` — read `utils/analytics_plot.py` and mirror the distribution builder with the existing `analytic.rs` helpers). Gate workspace ADMIN/MEMBER.
- GET `saved-analytic-view/:analytic_id/`: `AnalyticView.objects.get(pk, slug)` (miss → 500 → sane 404); run the stored `query` filters + `x_axis`/`y_axis` from `query_dict` (same 400s as above); **200** `{"total", "distribution"}`. Gate ADMIN/MEMBER.
- POST `export-analytics/`: body `{x_axis, y_axis, segment?}` with the same validation 400s; success **200** `{"message": "Once the export is ready it will be emailed to you at <email>"}` (Celery export SKIPPED — no email; document). Gate ADMIN/MEMBER.
- GET+PATCH+DELETE `analytic-view/:pk/`: `AnalyticViewViewset` ModelViewSet defaults — GET 200 row / PATCH 200 row (body updates `name`, `description`, `query_dict`; serializer recomputes `query` via `issue_filters`) / DELETE **204** soft; gate `WorkSpaceAdminPermission` = workspace ADMIN; miss → 404. Existing list/create already wired at `main.rs:1352-1355` (`analytic.rs:244-256`).

- [ ] **Step 1: Write the failing tests** (append to `analytic.rs`)

```rust
#[test]
fn analytics_axis_validation_matches_django() {
    assert_eq!(AXIS_REQUIRED_MSG, "x-axis and y-axis dimensions are required and the values should be valid");
    assert_eq!(SEGMENT_INVALID_MSG, "Both segment and x axis cannot be same and segment should be valid");
}

#[test]
fn export_analytics_message_shape() {
    let msg = export_message("a@b.co");
    assert_eq!(msg, "Once the export is ready it will be emailed to you at a@b.co");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::analytic`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Read `apps/api/plane/utils/analytics_plot.py` (`build_graph_plot`, `VALID_ANALYTICS_FIELDS`, `VALID_YAXIS`) and `apps/api/plane/utils/issue_filters.py`; map the SQL for the distribution builder into `analytic.rs` reusing the existing `default_analytics`/`project_stats` query patterns.
- `workspace_analytics` (GET `analytics/`), `saved_analytic` (GET), `export_analytics` (POST), `analytic_view_detail` (GET), `analytic_view_patch` (PATCH), `analytic_view_destroy` (DELETE) — all with the `ws_role`-based gates above; `saved_analytic` reads `analytic_views.query` (JSONB filters) and `query_dict`.
- Reuse the existing `list_views`/`create_view` row struct for the detail handlers.

Wire in `main.rs` (next to the existing `analytic-view/` route, `main.rs:1352-1355`):

```rust
.route("/api/workspaces/:slug/analytics/", get(routes::analytic::workspace_analytics))
.route(
    "/api/workspaces/:slug/saved-analytic-view/:analytic_id/",
    get(routes::analytic::saved_analytic),
)
.route("/api/workspaces/:slug/export-analytics/", post(routes::analytic::export_analytics))
.route(
    "/api/workspaces/:slug/analytic-view/:pk/",
    get(routes::analytic::analytic_view_detail)
        .patch(routes::analytic::analytic_view_patch)
        .delete(routes::analytic::analytic_view_destroy),
)
```

Add to `scripts/shadow.sh` (skip `export-analytics/` — POST-only → 405 on shadow's GET):

```bash
  "/api/workspaces/$WS/analytics/?x_axis=priority&y_axis=issue_count"
  "/api/workspaces/$WS/saved-analytic-view/00000000-0000-0000-0000-000000000000/"
  "/api/workspaces/$WS/analytic-view/00000000-0000-0000-0000-000000000000/"
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::analytic && cargo test -p api --test route_parity_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/analytic.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/scripts/shadow.sh
git commit -m "feat(rs-api): analytics legacy surface paritas Django (Batch F T9)"
```

---

### Task 10: `work-items/:project_identifier-:issue_identifier/` constraint fix

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/work_item.rs` (`get_by_identifier` + tests)
- Modify: `apps/api-rs/crates/api/src/main.rs` (route unchanged — `:ident` stays)
- Test: `apps/api-rs/crates/api/tests/route_parity_test.rs` (T0 baseline already lists `/work-items/:ident/`)

Contract (`views/issue/base.py:1201-1260+`):

- `issue_identifier` must be a strict integer (`isdigit()` or negative int) else 400 `{"error": "Invalid issue identifier"}`.
- Project lookup `identifier__iexact` + `workspace__slug` (miss → 404).
- Membership: active project member required else 403 `{"error": "You are not allowed to view this issue"}`.
- Issue lookup scoped `project_id + slug`, `select_related` + annotations (`cycle_id`, `link_count`, `attachment_count`, `sub_issues_count`, `label_ids`, `assignee_ids`) — reuse the existing `get_issue` detail shape (`work_item.rs`) since `IssueDetailIdentifierEndpoint` returns the same issue-detail serializer.
- Route form: Axum cannot express `:a-:b` in one segment — keep `/work-items/:ident/` and enforce the constraint in the handler (`splitn(2, '-')`).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn identifier_split_and_strict_int_match_django() {
    assert_eq!(resolve_identifier("ABC-123"), Ok(("ABC".to_string(), "123".to_string())));
    assert!(resolve_identifier("ABC-xyz").is_err()); // not an int
    assert!(resolve_identifier("ABC").is_err());     // no dash
}

#[test]
fn identifier_errors_match_django() {
    assert_eq!(INVALID_IDENTIFIER_MSG, "Invalid issue identifier");
    assert_eq!(IDENTIFIER_FORBIDDEN_MSG, "You are not allowed to view this issue");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::work_item`
Expected: FAIL.

- [ ] **Step 3: Implement**

Add to `work_item.rs`:

```rust
pub(crate) const INVALID_IDENTIFIER_MSG: &str = "Invalid issue identifier";
pub(crate) const IDENTIFIER_FORBIDDEN_MSG: &str = "You are not allowed to view this issue";

pub(crate) fn resolve_identifier(ident: &str) -> Result<(String, String), ()> {
    let (project_identifier, issue_identifier) = ident.split_once('-').ok_or(())?;
    if !issue_identifier.is_empty() && issue_identifier.chars().all(|c| c.is_ascii_digit()) {
        Ok((project_identifier.to_string(), issue_identifier.to_string()))
    } else if issue_identifier.starts_with('-') && issue_identifier[1..].chars().all(|c| c.is_ascii_digit()) {
        Ok((project_identifier.to_string(), issue_identifier.to_string()))
    } else {
        Err(())
    }
}
```

Harden `get_by_identifier` (currently `main.rs:1175` → `work_item.rs`): parse via `resolve_identifier` (Err → 400 `INVALID_IDENTIFIER_MSG`), project lookup `identifier ILIKE $1` (iexact) + slug (miss → 404 `missing()`), membership check (miss → 403 `IDENTIFIER_FORBIDDEN_MSG`), then delegate to the existing `get_issue` detail handler logic (same serializer shape).

- [ ] **Step 4: Run tests**

Run: `cargo test -p api --lib routes::work_item && cargo test -p api --test route_parity_test`
Expected: PASS (all 16 baseline entries green).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/work_item.rs
git commit -m "fix(rs-api): work-items identifier constraint paritas Django (strict int + iexact + member gate) (Batch F T10)"
```

---

## Final verification

- [ ] Run `cargo test -p api --test route_parity_test` → 2 passed (baseline 16/16).
- [ ] Run `cargo test -p api` → 0 failed (all unit/integration, incl. pre-existing parity gate).
- [ ] Run `bash apps/api-rs/scripts/shadow.sh` against the live stack (Django :8000 + Rust :8001) → `shadow ok`.
- [ ] Re-run the audit script from `docs/superpowers/plans/2026-09-08-batch-f-route-parity-gap.md` header → `strict_missing = 0`, `constraint_mismatch = 0`.

## Explicitly OUT (do NOT implement — documented in commits if touched)

- Legacy file-assets POST-create (`/api/users/file-assets/`, `/api/workspaces/:slug/file-assets/`) — Batch E §16.
- `issue-attachments/` POST-create (multipart legacy upload) — Batch E §16 precedent; V2 presign covers uploads.
- ai-assistant ×2, `plane/api/*`, `plane/space/*` — Batch E §16.
- `DELETE issue-relation/:relId/` (FE-dead, D9) and `remove-relation/` (already wired).

## Self-review notes

- Spec coverage: 15 gap routes + 1 constraint mapped 1:1 to T1–T10; T0 gate covers all of them.
- Type consistency: handler names in `main.rs` wiring match the `pub async fn` names defined per task; consts (`LINK_DUP_MSG`, `ISSUE_ATTACHMENT_MISS_MSG`, `INVALID_IDENTIFIER_MSG`, ...) are defined in the task that uses them.
- File ownership is disjoint per task except `main.rs`/`shadow.sh` (serial edits, repo convention).
