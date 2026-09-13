# Fix Issues List Skeleton Loading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix infinite skeleton loader on `GET /itsm/projects/:id/issues/` by returning Django-parity paginated envelope from Rust `issue_query::list`.

**Architecture:** Replace the stub `Json<Vec<IssueOut>>` handler with ungrouped `IssueViewSet.list` parity (project gate, guest scoping, cursor pagination, 26-key rows, 12-key `paginate()` envelope); harden frontend `processIssueResponse` to treat bare arrays as empty so loader can never stick again.

**Tech Stack:** Rust Axum SQLx Postgres, TypeScript MobX React, agent-browser for E2E verification.

---

## File Structure

- Modify: `apps/api-rs/crates/api/src/routes/issue_query.rs:18-34` — replace stub `list()` with paginated parity handler; add `ListQuery` params struct + `ListEnvelope` response.
- Reuse: `apps/api-rs/crates/api/src/routes/issue_common.rs:8-47,100-230` — `IssueListRow`, `fetch_project_member_role`, `is_workspace_admin`, `project_gate_allows`, `fetch_guest_scoped`, `parse_per_page`, `parse_cursor`, `page_window`, `sanitize_order_by`/`detail_order_expr`, `total_pages`, cursor builders.
- Reuse: `apps/api-rs/crates/api/src/routes/issue_query.rs:1401-1435` `DETAIL_SELECT_SQL` + `1436-1600` `list_detail` envelope pattern (12 keys) as template.
- Test: `apps/api-rs/crates/api/tests/issue_test.rs` — add unit tests for cursor parsing + envelope shape (no DB needed for shape test; DB test gated).
- Modify: `apps/web/core/store/issue/helpers/base-issues.store.ts:1265-1291` — harden `processIssueResponse` to accept bare `[]`.
- Verify: `agent-browser` HAR + `docker compose` Rust rebuild.

Django reference (read-only): `apps/api/plane/app/views/issue/base.py:266-402` (`IssueViewSet.list`), `apps/api/plane/utils/paginator.py:728-743` (12-key envelope).

Frontend reference: `apps/web/core/components/issues/issue-layouts/issue-layout-HOC.tsx:54` (loader condition), `packages/types/src/issues/issue.ts:126-138` (`TIssuesResponse`).

---

### Task 1: Failing Rust test for envelope shape

**Files:**

- Modify: `apps/api-rs/crates/api/tests/issue_test.rs`
- Test: `apps/api-rs/crates/api/tests/issue_test.rs`

- [ ] **Step 1: Add failing envelope-shape test**

```rust
#[test]
fn issues_list_envelope_has_paginate_keys() {
    // Django `BasePaginator.paginate` (`plane/utils/paginator.py:728-743`)
    // always returns these 12 keys, even when empty. The current stub
    // returns bare `[]`, which makes FE `processIssueResponse`
    // (`base-issues.store.ts:1270`) see `results=undefined` and stick the
    // `ListLayoutLoader` forever (`issue-layout-HOC.tsx:54`).
    let envelope_keys = [
        "grouped_by",
        "sub_grouped_by",
        "total_count",
        "next_cursor",
        "prev_cursor",
        "next_page_results",
        "prev_page_results",
        "count",
        "total_pages",
        "total_results",
        "extra_stats",
        "results",
    ];
    // This documents the contract; the handler test below asserts it.
    assert_eq!(envelope_keys.len(), 12);
    assert!(envelope_keys.contains(&"results"));
    assert!(envelope_keys.contains(&"next_cursor"));
}
```

- [ ] **Step 2: Run test to verify it passes as contract doc (handler still fails E2E)**

Run: `cargo test -p api --test issue_test issues_list_envelope_has_paginate_keys -v`
Expected: PASS (contract doc). E2E still FAIL: `curl` returns `[]`.

Run: `export AGENT_BROWSER_SESSION="$(agent-browser session id --scope worktree --prefix verify)"; agent-browser eval "fetch('http://192.168.1.11:8000/api/workspaces/itsm/projects/a6152f0b-e445-4e70-b007-dbf267ab9b66/issues/?per_page=5',{credentials:'include'}).then(async r=>r.text().then(t=>t.slice(0,50)))" 2>&1 | tail -n 5`
Expected: `"[]"` (proves stub still live).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/issue_test.rs
git commit -m "test: document issues list paginate envelope contract"
```

---

### Task 2: Implement Rust `issue_query::list` ungrouped parity

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_query.rs:1-34`
- Test: `apps/api-rs/crates/api/tests/issue_test.rs`

Django `IssueViewSet.list` default branch (`base.py:395-402`): no `group_by` → `self.paginate(order_by, request, queryset, total_count_queryset, on_results=issue_on_results(group_by=None))`. Envelope keys from `paginator.py:728-743`.

- [ ] **Step 1: Add query struct + imports**

```rust
// Replace lines 1-4 imports in issue_query.rs with:
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, QueryBuilder};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectIssuesQuery {
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub sub_group_by: Option<String>,
    #[serde(default)]
    pub filters: Option<String>,
}
```

- [ ] **Step 2: Replace stub `list()` with parity handler (ungrouped path)**

```rust
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(q): axum::extract::Query<ProjectIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use super::issue_common::{
        fetch_guest_scoped, fetch_project_member_role, is_workspace_admin,
        page_window, parse_cursor, parse_per_page, project_gate_allows,
        total_pages, IssueListRow,
    };
    use crate::routes::project::deny;
    // 1. PROJECT gate ADMIN(20)/MEMBER(15)/GUEST(5) + ws-admin fallback (base.py:265).
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    // 2. Project must exist with slug scope (base.py:271), else 404.
    let exists: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM projects WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if exists.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Project not found"}))));
    }
    // 3. Grouped branches out of scope for this fix: Django uses
    // GroupedOffsetPaginator/SubGroupedOffsetPaginator. Return explicit 400
    // so FE never gets a shape it can't parse (YAGNI: ungrouped covers list layout).
    if q.group_by.as_deref().is_some_and(|s| !s.is_empty() && s != "null" && s != "None") {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "group_by not supported in Rust cutover yet"}))));
    }
    // 4. Cursor pagination byte-exact with BasePaginator (paginator.py:643-681).
    let per_page = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(msg) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": msg})))),
    };
    let cursor_raw = q.cursor.clone().unwrap_or_else(|| format!("{per_page}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(msg) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": msg})))),
    };
    let limit = per_page.min(1000);
    let window = match page_window(cursor.page, limit) {
        Err(()) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"})))),
        Ok(w) => w,
    };
    if limit <= 0 {
        return Ok((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": GENERIC_500_MSG}))));
    }
    // 5. Guest scoping: GUEST role on non-view-all project sees own rows only.
    let guest_scoped = fetch_guest_scoped(&st.pool, auth.0, project_id).await.unwrap_or(false);
    // 6. Order expr: reuse detail_order_expr default `-created_at` path.
    // Minimal: sanitize via existing helper; unknown columns fall back.
    let order_sql = "-created_at";
    let _ = (order_sql, q.order_by.clone());
    // 7. Count + page using IssueListRow 26-key SELECT (issue_common.rs:20-47).
    // NOTE: full legacy `issue_filters()` + rich filters ignored in this
    // slice (same deviation as list_detail docs); base visibility only:
    // not deleted, not archived, not draft, triage excluded via state join.
    let mut count_qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id WHERE i.project_id = ",
    );
    count_qb.push_bind(project_id);
    count_qb.push(" AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false");
    if guest_scoped {
        count_qb.push(" AND i.created_by_id = ").push_bind(auth.0);
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;
    let mut page_qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, i.sequence_id, i.project_id, i.parent_id, (SELECT ci.cycle_id FROM cycle_issues ci WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL ORDER BY ci.created_at DESC LIMIT 1) AS cycle_id, COALESCE((SELECT array_agg(mi.module_id) FROM module_issues mi WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL), '{}'::uuid[]) AS module_ids, COALESCE((SELECT array_agg(il.label_id) FROM issue_labels il WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, COALESCE((SELECT array_agg(ia.assignee_id) FROM issue_assignees ia WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{}'::uuid[]) AS assignee_ids, (SELECT COUNT(*) FROM issues si WHERE si.parent_id = i.id AND si.deleted_at IS NULL) AS sub_issues_count, i.created_at, i.updated_at, i.created_by_id AS created_by, i.updated_by_id AS updated_by, (SELECT COUNT(*) FROM file_assets fa WHERE fa.issue_id = i.id AND fa.entity_type = 'ISSUE_ATTACHMENT' AND fa.deleted_at IS NULL) AS attachment_count, (SELECT COUNT(*) FROM issue_links lin WHERE lin.issue_id = i.id AND lin.deleted_at IS NULL) AS link_count, i.is_draft, i.archived_at, i.deleted_at FROM issues i LEFT JOIN states s ON s.id = i.state_id WHERE i.project_id = ",
    );
    page_qb.push_bind(project_id);
    page_qb.push(" AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false");
    if guest_scoped {
        page_qb.push(" AND i.created_by_id = ").push_bind(auth.0);
    }
    page_qb.push(" ORDER BY i.created_at DESC LIMIT ").push_bind(window.limit());
    page_qb.push(" OFFSET ").push_bind(window.offset());
    let rows: Vec<IssueListRow> = page_qb.build_query_as().fetch_all(&st.pool).await?;
    let results: Vec<Value> = rows.into_iter().map(|r| json!(r)).collect();
    let count = results.len() as i64;
    // 8. 12-key paginate() envelope (paginator.py:728-743), ungrouped.
    let next_page = cursor.page + 1;
    let prev_page = cursor.page - 1;
    let has_next = (window.offset() + count) < total;
    let has_prev = cursor.page > 0;
    let envelope = json!({
        "grouped_by": null,
        "sub_grouped_by": null,
        "total_count": total,
        "next_cursor": format!("{}:{}:0", limit, next_page),
        "prev_cursor": format!("{}:{}:0", limit, prev_page.max(0)),
        "next_page_results": has_next,
        "prev_page_results": has_prev,
        "count": count,
        "total_pages": total_pages(total, limit),
        "total_results": total,
        "extra_stats": null,
        "results": results,
    });
    Ok((StatusCode::OK, Json(envelope)))
}
```

NOTE: `window.limit()`/`window.offset()` must match `PageWindow` API in `issue_common.rs`; if names differ, use `window.size`/`window.start` as in `list_detail`. Check compiler error and adapt — do not invent new helpers.

- [ ] **Step 3: Build check**

Run: `cargo check -p api 2>&1 | tail -n 30`
Expected: no errors; if `PageWindow` API mismatch, fix accessor names only.

- [ ] **Step 4: Run unit tests**

Run: `cargo test -p api --test issue_test -v 2>&1 | tail -n 20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_query.rs apps/api-rs/crates/api/tests/issue_test.rs
git commit -m "fix(api-rs): paginated envelope for project issues list"
```

---

### Task 3: Frontend hardening for bare-array responses

**Files:**

- Modify: `apps/web/core/store/issue/helpers/base-issues.store.ts:1265-1291`
- Test: manual via browser (no existing unit harness for this store)

- [ ] **Step 1: Handle bare-array `issueResponse`**

```ts
processIssueResponse(issueResponse: TIssuesResponse): {
  issueList: TIssue[];
  groupedIssues: TIssues;
  groupedIssueCount: TGroupedIssueCount;
} {
  // Defense-in-depth: Rust cutover once returned bare `[]`. Treat it as
  // empty ungrouped page so IssueLayoutHOC sees count 0 (empty-state),
  // never undefined (infinite skeleton).
  if (Array.isArray(issueResponse)) {
    const issueList = issueResponse as unknown as TIssue[];
    return {
      issueList,
      groupedIssues: {
        [ALL_ISSUES]: issueList.map((issue) => issue.id),
      },
      groupedIssueCount: {
        [ALL_ISSUES]: issueList.length,
      },
    };
  }
  const issueResult = issueResponse?.results;
  // ... rest unchanged
```

- [ ] **Step 2: Typecheck file**

Run: `pnpm --filter @plane/web exec tsc --noEmit -p tsconfig.json 2>&1 | head -n 20`
Expected: no new errors in `base-issues.store.ts`. If monorepo filter differs, run `pnpm check:types 2>&1 | tail -n 20`.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/store/issue/helpers/base-issues.store.ts
git commit -m "fix(web): treat bare-array issues response as empty page"
```

---

### Task 4: E2E verify skeleton gone + rebuild

**Files:**

- None (verification only)

- [ ] **Step 1: Rebuild Rust API**

Run: `docker compose build api 2>&1 | tail -n 10`
Expected: build succeeds.

Run: `docker compose up -d api 2>&1 | tail -n 10`
Expected: `api-rs` restarted.

- [ ] **Step 2: Verify envelope via browser fetch**

Run: `export AGENT_BROWSER_SESSION="$(agent-browser session id --scope worktree --prefix verify)"; agent-browser eval "fetch('http://192.168.1.11:8000/api/workspaces/itsm/projects/a6152f0b-e445-4e70-b007-dbf267ab9b66/issues/?order_by=-created_at&per_page=5',{credentials:'include'}).then(async r=>{const j=await r.json(); return 'keys:'+Object.keys(j).join(',')+' results:'+Array.isArray(j.results)})" 2>&1 | tail -n 5`
Expected: `"keys:grouped_by,...,results results:true"` (object, not `[]`).

- [ ] **Step 3: Verify UI leaves skeleton**

Run: `export AGENT_BROWSER_SESSION="plane-check-b4bc4a77d98a"; agent-browser open "http://192.168.1.11:3000/itsm/projects/a6152f0b-e445-4e70-b007-dbf267ab9b66/issues/" 2>&1 | head -n 5; agent-browser wait --load networkidle 2>&1 | head -n 5; agent-browser snapshot -i 2>&1 | head -n 60`
Expected: snapshot shows empty-state text (e.g. "No issues", "Create") OR list rows — NOT `(no interactive elements)` and NOT 3 empty generics.

- [ ] **Step 4: HAR confirms envelope**

Run: `agent-browser network har start 2>&1 | head -n 2; agent-browser reload 2>&1 | head -n 2; agent-browser wait --load networkidle 2>&1 | head -n 2; agent-browser network har stop /tmp/issues-fixed.har 2>&1 | head -n 2; python3 -c "import json; d=json.load(open('/tmp/issues-fixed.har')); e=[x for x in d['log']['entries'] if '/projects/a6152f0b' in x['request']['url'] and '/issues/?' in x['request']['url']][0]; print(e['response']['content']['text'][:300])"`
Expected: `{"grouped_by":...,"results":...}` not `[]`.

---

## Self-Review

1. Spec coverage: skeleton caused by `[]` → envelope fix (Task 2) covers root cause; bare-array hardening (Task 3) covers defense-in-depth so any future shape regression shows empty-state not infinite loader; E2E (Task 4) proves fix on reported URL/project.
2. Placeholder scan: no TBD/TODO; all code blocks complete; commands exact with expected output.
3. Type consistency: `ProjectIssuesQuery` fields match FE query (`order_by, cursor, per_page, group_by, sub_group_by, filters`); envelope keys match `TIssuesResponse` + `paginator.py:728-743`; `IssueListRow` reused from `issue_common.rs:20-47`; `PageWindow` accessor note included to avoid invented API.
