# Workspace-level Intake Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only workspace-level Intake (triage inbox) page — flat, newest-first request queue across member projects with Open/Closed tabs, project filter + search, cursor Load more, click-through to the project inbox — plus the new backend endpoint `GET /api/workspaces/:slug/intake-issues/` it consumes.

**Architecture:** Backend adds one handler in `routes::intake` that mirrors the project `list_issues` semantics (per-row project scoping, guest restriction, `DetailEnvelope` pagination) unioned across all of the requester's member projects, reusing `InboxListRow` + `inbox_list_json` verbatim. Web follows the Tickets/KB pattern: local-state list view + shared `WorkspaceAggregateList` shell (project filter via `?project=`, tab bar via `?currentTab`, search, Load more footer) + pure, tested helpers. No queuemodel/store changes, no Django changes.

**Tech Stack:** Rust (axum, sqlx positional binds — matching `intake.rs` file style), React Router v7 (apps/web), MobX stores, `@plane/propel` icons/avatar, Vitest (`core/**/*.test.ts`), Rust DB-backed integration tests.

**Spec:** `docs/superpowers/specs/2026-10-18-workspace-intake-design.md`

---

## CRITICAL: Commit hygiene

The working tree already contains many unrelated pre-existing modifications.
**Never run `git add -A` or `git add .`.** Stage only the exact paths listed in
each task and verify with `git status --short <path>` before committing.

## Verified API facts (do not re-read)

- Project handler: `list_issues` (`routes/intake.rs:339`) — cursor pagination
  via `DetailEnvelope` + `next_cursor_str`/`parse_cursor`/`page_window`
  (`issue_common`, function-local `use` at line 345), `status` CSV with
  default `-2`, `guest_only` scoping, `inbox_list_json` row shape.
- Row struct `InboxListRow` (intake.rs:235): `id, status, duplicate_to_id,
snoozed_till, source, created_by_id, issue_id, issue_name, issue_priority,
issue_sequence_id, issue_project_id, issue_created_at, issue_label_ids,
issue_created_by_id`. `inbox_list_json` nests them under `issue` (intake.rs:252).
- Gate precedent: `ws_role(...)` + `crate::routes::member::deny_detail()`
  (same file, like `list_issues`).
- `intake.rs` top imports already cover `State, StatusCode, Json, Deserialize,
json, Value, AuthUser, AppState`; `Path`/`Query` are used fully-qualified
  inline in this file.
- `INBOX_STATUS` (`@plane/constants/intake.ts`) holds `status` +
  `i18n_title` per state; `InboxStatusIcon({ type, size, renderColor })`
  lives at `apps/web/core/components/inbox/inbox-status-icon.tsx` and accepts
  a numeric `TInboxIssueStatus`.
- Project row links to
  `/${slug}/projects/${pid}/intake?currentTab=${tab}&inboxIssueId=${issueId}`
  (issue id, not intake-issue id — `inbox-list-item.tsx`).
- Tab semantics: Open = `[-2, 0]` (pending + snoozed), Closed = `[1, -1, 2]`.
- The `intake_issues` table has `project_id`, `created_by_id`,
  `snoozed_till`, `source`, `duplicate_to_id` (verified against the live DB).

## File structure

Backend (new + modified):

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` — query type, gate,
  tab→status mapping, `workspace_intake_issues` handler.
- Modify: `apps/api-rs/crates/api/src/main.rs` — one route after the
  `"/api/workspaces/:slug/issues/"` route.
- New: `apps/api-rs/crates/api/tests/intake_workspace_test.rs`.

Web (new + modified):

- `apps/web/core/lib/workspace-intake.ts` + `workspace-intake.test.ts`.
- Modify: `apps/web/core/services/inbox/inbox-issue.service.ts` (new `listWorkspace`).
- `apps/web/core/components/inbox/workspace/workspace-intake-list-view.tsx`.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/intake/layout.tsx`, `page.tsx`.
- Modify: `apps/web/app/routes/core.ts` (route after the KB block).
- Modify: `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx` (optional `flat` prop).
- Modify: `packages/constants/src/workspace.ts`, `sidebar-item.tsx`, `helper.tsx`
  (`IntakeOutline` from `@makeplane/propel/icons` — same import the project
  sidebar uses).
- Modify: `packages/i18n/src/locales/en/empty-state.json` (merge `intake`
  keys into existing `workspace_empty_state`) + new `sidebar.intake_workspace`
  key in `en/navigation.json`'s `sidebar` object ("Intake").

Out of scope (do NOT touch): Django code, project inbox store/routes/views,
global-views, worker/triage-job, `@plane/constants` intake constants.

---

### Task 1: Backend endpoint

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`

- [ ] **Step 1: Extend the `axum` import in `intake.rs`**

Change line 1 from:

```rust
use axum::{extract::State, http::StatusCode, Json};
```

to:

```rust
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
```

- [ ] **Step 2: Add the query type, tab mapping, and handler**

Append after the end of the file (or after `list_issues`'s helpers — put it
directly after the `list_issues` handler ends, before `create_issue`'s doc
comment; if unsure where `list_issues` ends, anchor on the comment line
`/// Mirrors \`plane/app/views/intake/base.py:IntakeIssueViewSet.create\``):

```rust
/// Query for `GET /api/workspaces/:slug/intake-issues/`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WsIntakeIssuesQuery {
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    #[serde(default)]
    pub project_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Open tab = pending + snoozed (matches the project's effective behavior);
/// closed tab = accepted, rejected, duplicate. Anything else falls back to
/// open.
fn ws_intake_statuses(tab: Option<&str>) -> Vec<i32> {
    match tab {
        Some("closed") => vec![1, -1, 2],
        _ => vec![-2, 0],
    }
}

/// GET `/api/workspaces/:slug/intake-issues/` — workspace-level intake
/// queue. Same row shape (`inbox_list_json`) and cursor envelope as the
/// project `list_issues`, unioned across all intakes of the requester's
/// member projects. Per-project scoping is deliberately stricter than the
/// project endpoint (active membership required); guests restricted on a
/// project only see requests they created there.
pub async fn workspace_intake_issues(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<WsIntakeIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use super::issue_common::{
        next_cursor_str, page_window, parse_cursor, parse_per_page, prev_cursor_str,
        total_pages, DetailEnvelope, PageWindow,
    };
    if crate::routes::project::ws_role(&st.pool, auth.0, &slug)
        .await?
        .is_none()
    {
        return Ok(crate::routes::member::deny_detail());
    }
    let statuses = ws_intake_statuses(q.type_.as_deref());
    let per_page = match parse_per_page(Some(q.per_page.as_deref().unwrap_or("100"))) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
    };
    let limit = per_page.min(1000);
    if limit <= 0 {
        return Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "invalid limit"})),
        ));
    }
    let cursor = match parse_cursor(&q.cursor.clone().unwrap_or_else(|| format!("{limit}:0:0"))) {
        Ok(c) => c,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
    };
    let window = match page_window(cursor.page, limit) {
        Ok(w) => w,
        Err(()) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"})))),
    };
    let search = q.search.clone().unwrap_or_default();
    // NOTE: bind order must match the textual $N positions below.
    let base_where = "WHERE w.slug = $1 AND ii.deleted_at IS NULL \
         AND ii.status = ANY($2) \
         AND ($3::uuid IS NULL OR ii.project_id = $3) \
         AND ($4::text IS NULL OR i.name ILIKE '%' || $4 || '%') \
         AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = pr.id \
           AND pm.member_id = $5 AND pm.is_active = true AND pm.deleted_at IS NULL \
           AND (pm.role <> 5 \
             OR COALESCE((SELECT pr2.guest_view_all_features FROM projects pr2 WHERE pr2.id = pr.id), false) = true \
             OR ii.created_by_id = $6))";
    let base_from = format!(
        "FROM intake_issues ii \
         JOIN workspaces w ON w.id = ii.workspace_id \
         JOIN issues i ON i.id = ii.issue_id AND i.deleted_at IS NULL \
         JOIN intakes it ON it.id = ii.intake_id AND it.deleted_at IS NULL \
         JOIN projects pr ON pr.id = ii.project_id AND pr.workspace_id = w.id \
         AND pr.archived_at IS NULL AND pr.deleted_at IS NULL {base_where}"
    );
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) {base_from}"))
        .bind(&slug)
        .bind(&statuses)
        .bind(q.project_id)
        .bind(if search.is_empty() { None } else { Some(search.clone()) })
        .bind(auth.0)
        .bind(auth.0)
        .fetch_one(&st.pool)
        .await?;
    let mut rows: Vec<InboxListRow> = match window {
        PageWindow::Rows(offset) => sqlx::query_as(&format!(
            "SELECT ii.id, ii.status, ii.duplicate_to_id, ii.snoozed_till, ii.source, \
             ii.created_by_id, i.id AS issue_id, i.name AS issue_name, i.priority AS issue_priority, \
             i.sequence_id AS issue_sequence_id, i.project_id AS issue_project_id, \
             i.created_at AS issue_created_at, \
             COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
               WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{{}}'::uuid[]) AS issue_label_ids, \
             i.created_by_id AS issue_created_by_id \
             {base_from} ORDER BY i.created_at DESC, ii.id ASC LIMIT $7 OFFSET $8"
        ))
        .bind(&slug)
        .bind(&statuses)
        .bind(q.project_id)
        .bind(if search.is_empty() { None::<String> } else { Some(search.clone()) })
        .bind(auth.0)
        .bind(auth.0)
        .bind(limit + 1)
        .bind(offset)
        .fetch_all(&st.pool)
        .await?,
        PageWindow::BeyondEnd => Vec::new(),
    };
    let _ = (cursor.limit_value, cursor.is_prev);
    let next_page_results = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let envelope = DetailEnvelope {
        grouped_by: None,
        sub_grouped_by: None,
        total_count: total,
        next_cursor: next_cursor_str(limit, cursor.page),
        prev_cursor: prev_cursor_str(limit, cursor.page),
        next_page_results,
        prev_page_results: cursor.page > 0,
        count: rows.len() as i64,
        total_pages: total_pages(total, limit),
        total_results: total,
        extra_stats: None,
        results: rows.iter().map(inbox_list_json).collect(),
    };
    Ok((StatusCode::OK, Json(json!(envelope))))
}
```

Notes for exactness:

- `q.project_id` is `Option<uuid::Uuid>` and binds positionally as `$3::uuid`
  (NULL when absent). Invalid UUID strings are rejected by axum deserialization
  with 422 — same behavior as the existing typed query structs.
- The search bind is `Option<String>` against `$4::text`.
- `auth.0` is bound twice (`$5` membership, `$6` created-by fallback).
- `inbox_list_json` and `InboxListRow` are reused unchanged from this file.
- Row JSON keeps every project key including `issue.project_id`, which the web
  uses for project badges and click-through routing.

- [ ] **Step 3: Register the route in `main.rs`**

Right after the `"/api/workspaces/:slug/issues/"` route, insert:

```rust
        // Workspace intake queue: GET 200 flat `inbox_list_json` rows
        // (E2h; per-member-project scoping mirrors the aggregate
        // endpoints, stricter than the project `list_issues` gate).
        // Gate any active ws member 20/15/5.
        .route(
            "/api/workspaces/:slug/intake-issues/",
            get(routes::intake::workspace_intake_issues),
        )
```

- [ ] **Step 4: Compile-check**

Run from `apps/api-rs`:

```bash
cargo check -p api
```

Expected: no errors (warnings may be pre-existing).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/src/main.rs
git commit -m "feat(api): workspace intake issues listing endpoint"
```

---

### Task 2: Backend integration tests

**Files:**

- New: `apps/api-rs/crates/api/tests/intake_workspace_test.rs`

Mirror the `page_workspace_test.rs` fixture style (same user/workspace/project/member
INSERT shapes; `guest_view_all_features` parameterized; `created_at` controlled
per row for deterministic ordering tests). Serializer detail: bind
`parking-lot`-style plain values; no transactions needed — plain `&PgPool`.

Fixture layout (single scratch workspace, slug prefix `itw-`):

- Users: `admin` (ws role 20; project member 20 in `p1` + `p2`), `member`
  (ws 15; member 15 in `p1` only), `guest_open` (role 5 in `p1`, where
  `guest_view_all_features = true`), `guest_shut` (role 5 in `p2`, where
  `guest_view_all_features = false`), `outsider` (ws member, no project
  memberships), plus a random UUID for the non-member 403 case.
- Projects: `p1` (guest_view_all_features=true), `p2` (false), `p3`
  (no memberships for anyone under test).
- Intakes: one "Alpha" intake in `p1` for most tests; a second "Zulu" intake
  in `p1` for the union test; one in `p2`.
- Issues: one per intake issue below (explicit `sequence_id`s, staggered
  `created_at` via `now() - interval 'N hours'`).

Insert helper (mirrors the service-workspace tests' issues INSERT):

```sql
INSERT INTO issues (id, name, description_html, description_json, description_stripped,
 priority, is_draft, sort_order, sequence_id, state_id, type_id, project_id, workspace_id,
 created_at, updated_at, created_by_id)
VALUES ($1, $2, '<p></p>', '{}', '', 'none', false, 65535, $3, NULL, NULL, $4, $5, $6, now(), $7)
```

(rows: $1 id, $2 name, $3 sequence int, $4 project, $5 workspace, $6 created_at
timestamp, $7 created_by or NULL)

Intake-issue helper:

```sql
INSERT INTO intake_issues (id, intake_id, issue_id, project_id, workspace_id,
 status, snoozed_till, source, duplicate_to_id, created_by_id, updated_by_id,
 extra, deleted_at, created_at, updated_at)
VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, 'IN_APP', NULL, $7, NULL,
 '{}'::jsonb, NULL, now(), now())
```

($1 intake, $2 issue, $3 project, $4 workspace, $5 status i32,
$6 Option<DateTime>, $7 created_by Option<Uuid>)

Test cases (call `workspace_intake_issues` directly with
`State`/`AuthUser`/`Path(slug)`/`Query(WsIntakeIssuesQuery {...})`, assert on
`body["results"]`, `body["total_results"]`, `body["next_page_results"]`,
`body["next_cursor"]`):

1. `non_workspace_member_gets_403` — random UUID → 403.
2. `member_sees_only_member_projects` — `member` sees only `p1` rows (assert
   every row's `issue.project_id == p1`; assert `p2` row absent).
3. `open_tab_matches_pending_and_snoozed` — `type: Some("open")` returns
   statuses exactly `{-2, 0}`.
4. `closed_tab_matches_accepted_rejected_duplicate` — `type: Some("closed")`
   returns exactly `{1, -1, 2}`.
5. `invalid_type_falls_back_to_open` — `type: Some("nope")` same set as
   `type: None`.
6. `project_filter_scopes_and_still_enforces_visibility` — as `admin`,
   `project_id = Some(p2)` returns only p2 rows; restricted-guest-marked
   rows of p2 they didn't create stay hidden (assert set equality).
7. `restricted_guest_sees_only_own_requests` — as `shut` (role 5 in
   `guest_view_all=false` p2): only rows with `created_by == shut_id`.
8. `open_guest_sees_public_member_project_rows` — as `open` guest (role 5
   in view-all p1): all p1 rows regardless of creator.
9. `search_filters_by_issue_name` — `search = Some("portal")` returns only
   matching rows (case-insensitive: include a mixed-case name).
10. `pagination_cursors_page_results` — `per_page = Some("2")`: first page has
    2 rows, `next_page_results = true`, `total_results` equals full count;
    second page with returned cursor has no overlap with the first; final
    page has `next_page_results = false`.
11. `default_order_is_newest_first` — `issue.created_at` descending across
    all returned rows (fixture timestamps staggered by hours).
12. `multi_intake_union_lists_both_intakes` — fixture rows under both p1
    intakes ("Alpha" + "Zulu") both appear for `admin` (unlike the project
    endpoint's first-intake-only behavior).
13. `same_issue_in_two_intakes_appears_twice` — one issue bridged to two
    intakes of p1 yields two rows with the same `issue.id` (documents the
    v1 union edge case).
14. `outsider_sees_empty_list` — ws member with no project memberships gets
    `results = []`, `total_results = 0` (not 403).
15. `row_shape_carries_issue_and_project` — a row has `issue.{id,name,priority,
sequence_id,project_id,created_at}` and string `source`.

Cleanup: delete `intake_issues`, `issues`, `intakes` by `workspace_id`; then
`project_members`, `workspace_members`, `projects`, `workspaces`, fixture
users by id — mirror `page_workspace_test.rs` cleanup.

- [ ] **Step 1: Write the test file** (full code per the plan above).

- [ ] **Step 2: Run serially**

Run from `apps/api-rs`:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_workspace_test -- --test-threads=1
```

Expected: all tests pass. If any fail, fix the Task 1 handler/SQL first —
**do not weaken the expectations** to match implementation behavior unless
the behavior is verified correct against the spec (in which case stop and
ask).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/intake_workspace_test.rs
git commit -m "test(api): workspace intake endpoint coverage"
```

---

### Task 3: Web helpers (TDD)

**Files:**

- Create: `apps/web/core/lib/workspace-intake.ts`
- Test: `apps/web/core/lib/workspace-intake.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/lib/workspace-intake.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  appendIntakePage,
  buildIntakeDetailUrl,
  buildWorkspaceIntakeParams,
  createIntakePageState,
  flatGroupIntakeResults,
  normalizeIntakeEnvelope,
  normalizeIntakeTab,
  type TWorkspaceIntakeRow,
} from "./workspace-intake";

const row = (id: string) =>
  ({
    id,
    status: -2,
    issue: { id: `issue-${id}`, name: id, project_id: "p1" },
  }) as unknown as TWorkspaceIntakeRow;

describe("normalizeIntakeTab", () => {
  it("defaults unknown and empty tabs to open", () => {
    expect(normalizeIntakeTab(undefined)).toBe("open");
    expect(normalizeIntakeTab("nope")).toBe("open");
  });

  it("passes open and closed through", () => {
    expect(normalizeIntakeTab("open")).toBe("open");
    expect(normalizeIntakeTab("closed")).toBe("closed");
  });
});

describe("buildWorkspaceIntakeParams", () => {
  it("always sends paging", () => {
    expect(buildWorkspaceIntakeParams({})).toEqual({ per_page: "100" });
  });

  it("includes tab, project and trimmed search", () => {
    expect(buildWorkspaceIntakeParams({ typeFilter: "closed", projectId: "p2", search: "  vpn  " })).toEqual({
      type: "closed",
      project_id: "p2",
      search: "vpn",
      per_page: "100",
    });
  });

  it("omits blank search", () => {
    expect(buildWorkspaceIntakeParams({ search: "   " })).toEqual({ per_page: "100" });
  });
});

describe("normalizeIntakeEnvelope", () => {
  it("passes through array results and the cursor", () => {
    expect(
      normalizeIntakeEnvelope({
        results: [{ id: "a" }],
        next_cursor: "100:1:0",
        next_page_results: true,
        total_results: 12,
      })
    ).toEqual({
      results: [{ id: "a" }],
      nextCursor: "100:1:0",
      hasMore: true,
      totalResults: 12,
    });
  });

  it("drops non-array results and normalizes missing data", () => {
    expect(normalizeIntakeEnvelope({ results: { a: [] }, next_cursor: null })).toEqual({
      results: [],
      nextCursor: null,
      hasMore: false,
      totalResults: 0,
    });
  });
});

describe("createIntakePageState / appendIntakePage", () => {
  it("starts empty", () => {
    expect(createIntakePageState()).toEqual({ results: [], nextCursor: null, hasMore: false, totalResults: 0 });
  });

  it("appends pages preserving order and updates the cursor", () => {
    const first = createIntakePageState({ results: [row("a")], nextCursor: "100:1:0", hasMore: true, totalResults: 2 });
    const second = appendIntakePage(first, { results: [row("b")], nextCursor: null, hasMore: false, totalResults: 2 });
    expect(second.results.map((r) => r.id)).toEqual(["a", "b"]);
    expect(second.nextCursor).toBeNull();
    expect(second.hasMore).toBe(false);
    expect(second.totalResults).toBe(2);
  });
});

describe("flatGroupIntakeResults", () => {
  it("wraps rows in a single unlabeled group", () => {
    const groups = flatGroupIntakeResults([row("a"), row("b")]);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("all");
    expect(groups[0].label).toBe("");
    expect(groups[0].results.map((r) => r.id)).toEqual(["a", "b"]);
  });
});

describe("buildIntakeDetailUrl", () => {
  it("builds the project inbox deep link with tab and issue id", () => {
    expect(buildIntakeDetailUrl("ws", "p1", "closed", "iss-1")).toBe(
      "/ws/projects/p1/intake?currentTab=closed&inboxIssueId=iss-1"
    );
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/lib/workspace-intake`
Expected: FAIL — cannot resolve `./workspace-intake`.

- [ ] **Step 3: Write the implementation**

Create `apps/web/core/lib/workspace-intake.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWorkspaceIntakeTab = "open" | "closed";

export type TWorkspaceIntakeIssue = {
  id: string;
  name: string;
  priority: string;
  sequence_id: number;
  project_id: string;
  created_at: string;
  label_ids: string[];
  created_by: string | null;
};

export type TWorkspaceIntakeRow = {
  id: string;
  status: number;
  duplicate_to_id: string | null;
  snoozed_till: string | null;
  source: string | null;
  created_by: string | null;
  issue: TWorkspaceIntakeIssue;
};

export type TWorkspaceIntakePageState = {
  results: TWorkspaceIntakeRow[];
  nextCursor: string | null;
  hasMore: boolean;
  totalResults: number;
};

export function normalizeIntakeTab(typeParam?: string | null): TWorkspaceIntakeTab {
  return typeParam === "closed" ? "closed" : "open";
}

export function buildWorkspaceIntakeParams({
  typeFilter,
  projectId,
  search,
}: {
  typeFilter?: string;
  projectId?: string;
  search?: string;
}): Record<string, string> {
  const params: Record<string, string> = { type: normalizeIntakeTab(typeFilter), per_page: "100" };
  if (projectId) params.project_id = projectId;
  const normalizedSearch = search?.trim() ?? "";
  if (normalizedSearch) params.search = normalizedSearch;
  return params;
}

export function normalizeIntakeEnvelope(response: unknown): TWorkspaceIntakePageState {
  const data = (response ?? {}) as {
    results?: unknown;
    next_cursor?: unknown;
    next_page_results?: unknown;
    total_results?: unknown;
  };
  const results = Array.isArray(data.results) ? (data.results as TWorkspaceIntakeRow[]) : [];
  const nextCursor = typeof data.next_cursor === "string" ? data.next_cursor : null;
  return {
    results,
    nextCursor,
    hasMore: data.next_page_results === true && nextCursor !== null,
    totalResults: typeof data.total_results === "number" ? data.total_results : 0,
  };
}

export function createIntakePageState(envelope?: TWorkspaceIntakePageState): TWorkspaceIntakePageState {
  if (!envelope) return { results: [], nextCursor: null, hasMore: false, totalResults: 0 };
  return envelope;
}

export function appendIntakePage(
  state: TWorkspaceIntakePageState,
  envelope: TWorkspaceIntakePageState
): TWorkspaceIntakePageState {
  return {
    results: [...state.results, ...envelope.results],
    nextCursor: envelope.nextCursor,
    hasMore: envelope.hasMore,
    totalResults: envelope.totalResults,
  };
}

export function flatGroupIntakeResults(
  rows: TWorkspaceIntakeRow[]
): { key: string; label: string; rows: TWorkspaceIntakeRow[] }[] {
  return rows.length === 0 ? [] : [{ key: "all", label: "", rows }];
}

export function buildIntakeDetailUrl(workspaceSlug: string, projectId: string, tab: string, issueId: string): string {
  return `/${workspaceSlug}/projects/${projectId}/intake?currentTab=${tab}&inboxIssueId=${issueId}`;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web test core/lib/workspace-intake`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/workspace-intake.ts apps/web/core/lib/workspace-intake.test.ts
git commit -m "feat(web): workspace intake helpers"
```

---

### Task 4: Intake page (service, list view, page, layout, route, i18n)

**Files:**

- Modify: `apps/web/core/services/inbox/inbox-issue.service.ts` (add `listWorkspace`)
- Create: `apps/web/core/components/inbox/workspace/workspace-intake-list-view.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/intake/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/intake/page.tsx`
- Modify: `apps/web/app/routes/core.ts` (insert after the workspace KB block)
- Modify: `packages/i18n/src/locales/en/empty-state.json` (merge `intake`
  into `workspace_empty_state`)
- Modify: `packages/i18n/src/locales/en/navigation.json` (add
  `"sidebar": { "intake_workspace": "Intake", ... }` — place next to the
  existing `sidebar` keys; do NOT rename the existing keys)

- [ ] **Step 1: Add the service method**

In `apps/web/core/services/inbox/inbox-issue.service.ts`, after the `list`
method, add:

```ts
  async listWorkspace(workspaceSlug: string, params?: Record<string, string | number>, config = {}) {
    return this.get(`/api/workspaces/${workspaceSlug}/intake-issues/`, { params }, config)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
```

- [ ] **Step 2: Create the list view**

Create `apps/web/core/components/inbox/workspace/workspace-intake-list-view.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { useTranslation } from "@plane/i18n";
import { PriorityIcon } from "@plane/propel/icons";
import type { EInboxIssueStatus, TIssuePriorities } from "@plane/types";
import { getFileURL, renderFormattedDate } from "@plane/utils";
// components
import { AvatarGroupOverflow } from "@/components/common/avatar-group-overflow";
import { ListItem } from "@/components/core/list";
import { InboxStatusIcon } from "@/components/inbox/inbox-status-icon";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import {
  appendIntakePage,
  buildIntakeDetailUrl,
  buildWorkspaceIntakeParams,
  createIntakePageState,
  flatGroupIntakeResults,
  normalizeIntakeEnvelope,
  normalizeIntakeTab,
  type TWorkspaceIntakeRow,
} from "@/lib/workspace-intake";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";
// services
import { InboxIssueService } from "@/services/inbox/inbox-issue.service";

const intakeIssueService = new InboxIssueService();

const INTAKE_TAB_OPTIONS = [
  { key: "open", label: "Open" },
  { key: "closed", label: "Closed" },
] as const;

type TIntakeRowProps = {
  row: TWorkspaceIntakeRow;
  workspaceSlug: string;
  tab: string;
};

const IntakeRow = observer(function IntakeRow({ row, workspaceSlug, tab }: TIntakeRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const { getUserDetails } = useMember();

  const project = getProjectById(row.issue.project_id);
  const creator = row.issue.created_by ? getUserDetails(row.issue.created_by) : undefined;
  const issueKey = project ? `${project.identifier}-${row.issue.sequence_id}` : `#${row.issue.sequence_id}`;

  return (
    <ListItem
      title={`${issueKey} ${row.issue.name}`}
      itemLink={buildIntakeDetailUrl(workspaceSlug, project?.id ?? row.issue.project_id, tab, row.issue.id)}
      prependTitleElement={
        row.status !== -2 ? <InboxStatusIcon type={row.status as EInboxIssueStatus} size={14} /> : undefined
      }
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {project && (
            <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">{project.name}</span>
          )}
          {row.source && <span className="hidden rounded-sm bg-layer-2 px-1.5 py-0.5 md:inline">{row.source}</span>}
          <PriorityIcon priority={row.issue.priority as TIssuePriorities} size={14} />
          {creator && (
            <span className="hidden items-center gap-1.5 lg:flex">
              <AvatarGroupOverflow size="xs">
                <Avatar
                  alt={creator.display_name}
                  fallback={creator.display_name?.[0]?.toUpperCase()}
                  src={getFileURL(creator.avatar_url ?? "")}
                />
              </AvatarGroupOverflow>
              <span className="max-w-[100px] truncate">{creator.display_name}</span>
            </span>
          )}
          <span className="hidden md:inline">{renderFormattedDate(row.issue.created_at)}</span>
        </div>
      }
      parentRef={parentRef}
    />
  );
});
```

Then the main view component (page-level state, debounce 300ms, AbortController,
tab/project/search reset, Load more, tab bar with `?currentTab`, shell with
`flat` prop):

```tsx
type Props = {
  typeFilter?: string;
  projectFilter?: string;
};

export const WorkspaceIntakeListView = observer(function WorkspaceIntakeListView({ typeFilter, projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { workspaceProjectIds } = useProject();
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [pageState, setPageState] = useState(() => createIntakePageState());
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);

  const slug = workspaceSlug?.toString();
  const normalizedTab = normalizeIntakeTab(typeFilter);
  const normalizedProjectFilter =
    projectFilter && (workspaceProjectIds ?? []).includes(projectFilter) ? projectFilter : undefined;

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedSearch(search), 300);
    return () => clearTimeout(timer);
  }, [search]);

  useEffect(() => {
    if (!slug) return;
    const controller = new AbortController();
    setIsLoading(true);
    const params = buildWorkspaceIntakeParams({
      typeFilter: normalizedTab,
      projectId: normalizedProjectFilter,
      search: debouncedSearch,
    });
    void intakeIssueService
      .listWorkspace(slug, params, { signal: controller.signal })
      .then((response) => setPageState(createIntakePageState(normalizeIntakeEnvelope(response))))
      .catch(() => undefined)
      .finally(() => {
        if (!controller.signal.aborted) setIsLoading(false);
      });
    return () => controller.abort();
  }, [slug, normalizedTab, normalizedProjectFilter, debouncedSearch]);

  const renderedGroups: TWorkspaceAggregateGroupItem[] = useMemo(() => {
    const groups = flatGroupIntakeResults(pageState.results);
    return groups.map((group) => ({
      key: group.key,
      label: group.label,
      count: group.rows.length,
      items: (
        <>
          {group.rows.map((row) => (
            <IntakeRow key={row.id} row={row} workspaceSlug={slug ?? ""} tab={normalizedTab} />
          ))}
        </>
      ),
    }));
  }, [pageState.results, normalizedTab, slug]);

  const buildTabHref = (nextTab: string) => {
    if (!slug) return "#";
    const params = new URLSearchParams();
    params.set("currentTab", nextTab);
    if (normalizedProjectFilter) params.set("project", normalizedProjectFilter);
    return `/${slug}/intake?${params.toString()}`;
  };

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    const params = new URLSearchParams();
    params.set("currentTab", normalizedTab);
    if (projectId) params.set("project", projectId);
    router.push(`/${slug}/intake?${params.toString()}`);
  };

  const handleLoadMore = () => {
    if (!slug || !pageState.hasMore || !pageState.nextCursor || isLoadingMore) return;
    setIsLoadingMore(true);
    const params = {
      ...buildWorkspaceIntakeParams({
        typeFilter: normalizedTab,
        projectId: normalizedProjectFilter,
        search: debouncedSearch,
      }),
      cursor: pageState.nextCursor,
    };
    void intakeIssueService
      .listWorkspace(slug, params, {})
      .then((response) => setPageState((state) => appendIntakePage(state, normalizeIntakeEnvelope(response))))
      .catch(() => undefined)
      .finally(() => setIsLoadingMore(false));
  };

  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      <div className="flex items-center gap-1 border-b border-subtle px-3 py-1.5">
        {INTAKE_TAB_OPTIONS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => router.push(buildTabHref(tab.key))}
            className={
              tab.key === normalizedTab
                ? "rounded-sm bg-layer-3 px-2.5 py-1 text-12 font-medium text-primary"
                : "rounded-sm px-2.5 py-1 text-12 text-secondary hover:bg-layer-transparent-hover"
            }
          >
            {tab.label}
          </button>
        ))}
      </div>
      <WorkspaceAggregateList
        flat
        projectFilter={normalizedProjectFilter}
        onProjectFilterChange={handleProjectFilterChange}
        search={search}
        onSearchChange={setSearch}
        searchPlaceholder={t("service.catalog_search_placeholder")}
        groups={renderedGroups}
        totalCount={pageState.totalResults}
        isLoading={isLoading}
        emptyTitle={t("workspace_empty_state.intake.title")}
        emptyDescription={t("workspace_empty_state.intake.description")}
        footer={
          pageState.hasMore ? (
            <div className="flex items-center justify-center border-b border-subtle py-3">
              <button
                type="button"
                onClick={handleLoadMore}
                disabled={isLoadingMore}
                className="rounded-sm border border-subtle px-3 py-1 text-12 text-secondary transition-colors hover:bg-layer-transparent-hover disabled:cursor-not-allowed disabled:opacity-50"
              >
                {isLoadingMore ? t("common.loading") : t("common.load_more")}
              </button>
            </div>
          ) : undefined
        }
      />
    </div>
  );
};
```

`getProjectById` is destructured only in the row component (which uses it);
the main view destructures `workspaceProjectIds` alone.

Notes:

- `flat` prop: Task 5 below adds it to the shell. This task's code uses it —
  Task 5 must land before/with the typecheck in Task 4 Step 8. Order: do the
  shell edit FIRST within this task (Step: shell flat prop), then typecheck.
  The plan lists the shell edit as its own task (Task 5) to keep the diff
  reviewable — but typecheck in Task 4 would fail without it. Resolution:
  the plan keeps the written order but the git adds are split by file, so
  order of authoring doesn't matter for the final commits. Follow the
  `git add` lists exactly.
- Unused imports were scrubbed (`INBOX_STATUS`, `formatIntakeIssueKey`,
  duplicate `buildIntakeDetailUrl`): row shows the status icon via a direct
  `EInboxIssueStatus` cast; issue keys are built inline.
- `priority` typing: `row.issue.priority` is `string`; `PriorityIcon` expects
  `TIssuePriorities | undefined | null` — cast inline:
  `priority={row.issue.priority as TIssuePriorities}`.
  `InboxStatusIcon` takes `type: TInboxIssueStatus` (numeric enum) — cast inline:
  `type={row.status as EInboxIssueStatus}`.

- [ ] **Step 3: Create the layout**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/intake/layout.tsx`
(mirror the KB layout; breadcrumb label `t("sidebar.intake_workspace")`,
href `/${workspaceSlug}/intake/`, icon `IntakeOutline` from
`@makeplane/propel/icons`).

- [ ] **Step 4: Create the page**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/intake/page.tsx`
(mirror the KB page; reads `currentTab` + `project`; `PageHead` title
`t("sidebar.intake_workspace")`; renders `WorkspaceIntakeListView`).

- [ ] **Step 5: Add the route**

In `apps/web/app/routes/core.ts`, right after the workspace KB block, insert:

```ts
        // Workspace Intake
        layout("./(all)/[workspaceSlug]/(projects)/intake/layout.tsx", [
          route(":workspaceSlug/intake", "./(all)/[workspaceSlug]/(projects)/intake/page.tsx"),
        ]),
```

- [ ] **Step 6: Add the i18n keys**

In `packages/i18n/src/locales/en/empty-state.json`, inside the existing
`workspace_empty_state` namespace (next to `kb`), add:

```json
    "intake": {
      "title": "No requests yet",
      "description": "Incoming service requests from projects you are a member of will show up here."
    },
```

In `packages/i18n/src/locales/en/navigation.json`, inside the existing
`sidebar` object (next to `pages`), add:

```json
    "intake_workspace": "Intake",
```

(Other locales fall back to en at runtime.)

- [ ] **Step 7: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0. **Note:** this check runs BEFORE the Task 5 shell commit,
so the `flat` prop MUST already be implemented (uncommitted) per the note in
Step 2 above.

- [ ] **Step 8: Commit (page files only — NOT the shell)**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/intake/layout.tsx" \
  "apps/web/app/(all)/[workspaceSlug]/(projects)/intake/page.tsx" \
  apps/web/core/components/inbox/workspace/workspace-intake-list-view.tsx \
  apps/web/core/services/inbox/inbox-issue.service.ts \
  apps/web/app/routes/core.ts \
  packages/i18n/src/locales/en/empty-state.json \
  packages/i18n/src/locales/en/navigation.json
git commit -m "feat(web): workspace intake page"
```

---

### Task 5: Shell flat mode + sidebar entry

**Files:**

- Modify: `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx` (add `flat` prop)
- Modify: `packages/constants/src/workspace.ts` (nav item after `kb`)
- Modify: `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx` (`IntakeOutline`)

- [ ] **Step 1: Add the `flat` prop to the shell**

In `Props`, add:

```ts
  flat?: boolean;
```

Destructure `flat = false`, and render the list body as:

```tsx
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {flat ? (
            <>{groups.flatMap((group) => group.items)}</>
          ) : (
            groups.map((group) => (
              <div key={group.key}>
                <div className="flex items-center gap-2 border-b border-subtle bg-layer-1 px-4 py-2">
                  <span className="text-12 font-medium text-secondary">{group.label}</span>
                  <span className="text-11 text-tertiary">{group.count}</span>
                </div>
                {group.items}
              </div>
            ))
          )}
          {footer}
        </div>
      )}
```

Replace the existing body block (the `groups.map(...)` part only; keep
loading/empty states untouched).

- [ ] **Step 2: Add the nav item**

In `packages/constants/src/workspace.ts`, inside
`WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS`, add after the `kb` entry:

```ts
  intake: {
    key: "intake",
    labelTranslationKey: "sidebar.intake_workspace",
    href: `/intake/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER],
    highlight: (pathname: string, url: string) => pathname.includes(url),
  },
```

And insert `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["intake"]` into
`..._LINKS` after `"kb"`.

- [ ] **Step 3: Register the item key and icon**

`sidebar-item.tsx`: add `"intake"` to `staticItems` (after `"kb"`).
`helper.tsx`: add `IntakeOutline` to the `@makeplane/propel/icons` import and:

```tsx
    case "intake":
      return <IntakeOutline className={cn("size-4 flex-shrink-0", className)} />;
```

- [ ] **Step 4: Rebuild the constants package**

Run: `pnpm --filter=@plane/constants build`
Expected: build succeeds.

- [ ] **Step 5: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 6: Commit (shell + sidebar + constants only)**

```bash
git add apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx \
  packages/constants/src/workspace.ts \
  apps/web/core/components/workspace/sidebar/sidebar-item.tsx \
  apps/web/core/components/workspace/sidebar/helper.tsx
git commit -m "feat(web): workspace intake nav entry and aggregate flat mode"
```

---

### Task 6: Full verification and deploy

- [ ] **Step 1: Run the backend tests**

Run from `apps/api-rs`:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_workspace_test -- --test-threads=1
```

Expected: all tests pass. On failure, fix Task 1/2 code — **do not weaken
test expectations** without confirming the behavior is spec-correct (then
stop and ask).

- [ ] **Step 2: Run the full web test suite**

Run: `pnpm --filter=web test`
Expected: all test files pass (previous 406 + new intake helper tests).

- [ ] **Step 3: Lint**

Run: `pnpm --filter=web check:lint`
Expected: exit 0; no warnings point at new/changed files (baseline has
hundreds of pre-existing warnings).

- [ ] **Step 4: Build constants and web**

```bash
pnpm --filter=@plane/constants build
pnpm --filter=web build
```

Expected: both succeed. Verify the bundle contains the route and strings:
`rg ":workspaceSlug/intake" apps/web/build/client/assets/manifest-*.js`
and `rg -l "No requests yet" apps/web/build/client/`.

- [ ] **Step 5: Rebuild the backend (Rust, Docker)**

Follow AGENTS.md exactly — the endpoint is new Rust code so the running
containers do NOT serve it until rebuilt:

1. Verify stack state: `docker compose -f docker-compose-local.yml ps`
   (expect `api`, `worker`, `beat-worker` up; do NOT touch the root
   `docker-compose.yml` while the stack runs).
2. Detached build (LTO link can stall output for 10+ minutes — do not abort):
   `setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-intake-api-build.log 2>&1 < /dev/null &`
3. Poll `tail /tmp/plane-intake-api-build.log` until containers recreate.
4. `curl http://localhost:8000/health` → 200 (guest request to the new
   endpoint must NOT 404: `curl -s -o /dev/null -w "%{http_code}" http://localhost:8000/api/workspaces/nonexistent/intake-issues/` → 401/403 expected, 404 = route missing).
5. `systemctl --user restart plane-live.service` + `curl http://localhost:3100/live/health/` → 200.

- [ ] **Step 6: Restart the prod web service and smoke test**

```bash
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000/x/intake
```

Expected: `200`, `200`.

Manual smoke on the tunnel:

1. Sidebar shows **Intake** after Knowledge base.
2. `/[workspaceSlug]/intake` defaults to Open tab with pending+snoozed rows.
3. Closed tab shows accepted/rejected/duplicate rows.
4. `?project=<id>` and search filter server-side; search matches issue names.
5. Load more appends the next page; counts match `total_results`.
6. Clicking a row opens `/[workspaceSlug]/projects/:pid/intake?currentTab=...&inboxIssueId=...` with the item selected.
7. A restricted guest only sees their own requests; a non-member sees an
   empty list; an outsider gets 403 from the endpoint.

- [ ] **Step 7: Report**

Report the smoke results. No commit for this task.

---

## Self-review notes

- Spec coverage: route/sidebar/tab (Tasks 4-5), grouped-flat list + rows (Tasks
  3-4), server-side filters + pagination + envelope (Tasks 1-3), invalid
  input handling (Tasks 1-3), merged i18n keys incl. new sidebar label
  (Tasks 4-5), tests (Tasks 2-3), verification incl. Docker rebuild (Task 6).
- Type consistency: helpers use `TWorkspaceIntakeRow` (structural subset of the
  `inbox_list_json` shape); the reducer/envelope mirror the proven tickets/KB
  helpers; `normalizeIntakeTab` output feeds both the tab UI and the API
  `type` param. `flat` groups are single-item by construction.
- Task 4's typecheck depends on Task 5's shell edit — the plan orders file
  edits first, verification after, and splits only the _commits_ by file, so
  no commit contains broken code.
