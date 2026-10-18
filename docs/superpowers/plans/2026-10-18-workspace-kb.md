# Workspace-level KB Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a workspace-level Knowledge Base page (flat aggregate page list across member projects: tabs All/Public/Private/Archived, project filter + search, 100-per-page Load more, click-through to the project page detail) plus the new read-only backend endpoint `GET /api/workspaces/:slug/pages/` it consumes.

**Architecture:** Backend adds one handler in `routes::page` reusing the project list visibility semantics (owner bypass; private non-owner denied; restricted-guest scoping), the same `PageRow`/`page_list_json` row shape (with member-scoped `project_ids`), the issue-style cursor pagination envelope (`DetailEnvelope`), and the same `sanitize_page_order_by` allowlist. Web follows the Tickets pattern: local-state list view + shared aggregate shell (project filter via `?project=`, tab bar for `?type=`, Load more footer) + pure, tested helpers. No queue model/store changes, no Django changes.

**Tech Stack:** Rust (axum, sqlx `QueryBuilder`), React Router v7 (apps/web), MobX stores, Vitest (`core/**/*.test.ts`), Rust DB-backed integration tests.

**Spec:** `docs/superpowers/specs/2026-10-18-workspace-kb-design.md`

---

## CRITICAL: Commit hygiene

The working tree already contains many unrelated pre-existing modifications.
**Never run `git add -A` or `git add .`.** Stage only the exact paths listed in
each task and verify with `git status --short <path>` before committing.

## Verified API facts (do not re-derive)

- `page.rs` already imports `serde::Deserialize`, `AuthUser`, `AppState`,
  `StatusCode`, `Json`, `json!`, `uuid::Uuid` (fully qualified), and
  `use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};`.
- `ws_role` is `pub(crate) async` in `crate::routes::project` (project.rs:23).
- `deny_detail()` (page.rs:853) returns `403 {"detail": ...}`.
- `parse_per_page` / `parse_cursor` / `page_window` / `next_cursor_str` /
  `prev_cursor_str` / `PageWindow` / `DetailEnvelope` / `total_pages` are
  `pub(crate)` in `super::issue_common`.
- `sanitize_page_order_by` (page.rs) has allowlist
  `{created_at,updated_at,name,sort_order}`, defaults `("p.created_at", true)`.
- `PageRow` maps positionally from `PAGE_ROW_COLS` order; `page_list_json`
  emits `id, name, owned_by, access, color, parent, is_favorite, is_locked,
archived_at, workspace, created_at, updated_at, created_by, updated_by,
view_props, logo_props, label_ids, project_ids`.
- `projects` has `workspace_id`; `project_pages` rows have their own
  `workspace_id` but links are pinned via the joined project row.
- `PageListQuery` derive pattern: `#[derive(Debug, Clone, Deserialize, Default)]`.
- Backend per-module `fn bad_request(body: Value)` precedent exists
  (e.g. `draft.rs:223`).
- Rust test suites auto-discover `tests/*.rs`; run serially with
  `DATABASE_URL=... cargo test -p api --test <name> -- --test-threads=1`
  from `apps/api-rs` (see AGENTS.md; `page_parent_filter_test.rs` header).
- `TPageNavigationTabs = "public" | "private" | "archived"`
  (`packages/types/src/page/core.ts`); workspace adds an `"all"` default.
- Web: `ProjectPageService` in `@/services/page/project-page.service.ts`;
  row pattern mirrors `PageListBlock` (`Logo` from
  `@plane/propel/emoji-icon-picker`, `PagesOutline` from
  `@makeplane/propel/icons`, `getPageName` from `@plane/utils`).
- Project tab labels are hardcoded English (`Public`/`Private`/`Archived` in
  `PageTabNavigation`) — the workspace tab row mirrors that style.

## File structure

Backend (new + modified):

- Modify: `apps/api-rs/crates/api/src/routes/page.rs` — imports, `WsPagesQuery`,
  `ws_bad_request`, `push_ws_pages_tail`, `workspace_pages` handler.
- Modify: `apps/api-rs/crates/api/src/main.rs` — one route registration.
- New: `apps/api-rs/crates/api/tests/page_workspace_test.rs` — DB-backed tests.

Web (new + modified):

- `apps/web/core/lib/workspace-kb.ts` + `workspace-kb.test.ts`.
- Modify: `apps/web/core/services/page/project-page.service.ts` — `getWorkspacePages`.
- `apps/web/core/components/pages/workspace/workspace-kb-list-view.tsx`.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/kb/layout.tsx`, `page.tsx`.
- Modify: `apps/web/app/routes/core.ts` (insert after the workspace tracks block).
- Modify: `packages/constants/src/workspace.ts` (nav item).
- Modify: `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`.
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx`.
- Modify: `packages/i18n/src/locales/en/empty-state.json` (merge into `workspace_empty_state`).

Out of scope (do NOT touch): Django code, project page store/routes, global
views, `issue_lists.rs`/`issue_common.rs` helpers, DB/docker.

---

### Task 1: Backend endpoint

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/page.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`

- [ ] **Step 1: Extend the imports in `page.rs`**

1. Extend the line-16 import:

```rust
use super::issue_common::{
    fetch_project_member_role, is_workspace_admin, project_gate_allows, DetailEnvelope, PageWindow,
    next_cursor_str, parse_cursor, parse_per_page, prev_cursor_str, total_pages,
};
```

2. Add `ws_role` to the line-13 use:

```rust
use crate::routes::project::{deny, missing, ws_role, FORBIDDEN_MSG};
```

3. Add a sqlx import for the query builder (page.rs has none today):

```rust
use sqlx::{Postgres, QueryBuilder};
```

- [ ] **Step 2: Add the query type, scope helper, and handler**

Append after the `summary` handler in `page.rs`:

```rust
/// Query for `GET /api/workspaces/:slug/pages/`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WsPagesQuery {
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
    #[serde(default)]
    pub order_by: Option<String>,
}

fn ws_bad_request(body: Value) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(body))
}

/// Shared WHERE-tail for the workspace pages COUNT + page queries.
/// Textual bind order inside the tail: user (membership), user (owner),
/// user (restricted guest), optional project_id, optional search. The caller
/// completes the head (`SELECT ... WHERE w.slug = $N`) before calling.
fn push_ws_pages_tail(
    qb: &mut QueryBuilder<Postgres>,
    user: uuid::Uuid,
    tab: &str,
    project_id: Option<uuid::Uuid>,
    search: &str,
) {
    match tab {
        "public" => qb.push(" AND p.access = 0 AND p.archived_at IS NULL "),
        "private" => qb.push(" AND p.access = 1 AND p.archived_at IS NULL "),
        "archived" => qb.push(" AND p.archived_at IS NOT NULL "),
        _ => qb.push(" AND p.archived_at IS NULL "),
    };
    qb.push(
        " AND EXISTS(SELECT 1 FROM project_pages ppv \
         JOIN projects prv ON prv.id = ppv.project_id AND prv.workspace_id = w.id \
         AND prv.archived_at IS NULL AND prv.deleted_at IS NULL \
         JOIN project_members mv ON mv.project_id = ppv.project_id AND mv.member_id = ",
    );
    qb.push_bind(user);
    qb.push(
        " AND mv.is_active = true AND mv.deleted_at IS NULL \
         WHERE ppv.page_id = p.id AND ppv.deleted_at IS NULL \
         AND (p.owned_by_id = ",
    );
    qb.push_bind(user);
    qb.push(" OR (p.access = 0 AND NOT EXISTS( \
         SELECT 1 FROM project_members rg JOIN projects rp ON rp.id = rg.project_id \
         WHERE rg.project_id = ppv.project_id AND rg.member_id = ");
    qb.push_bind(user);
    qb.push(
        " AND rg.role = 5 AND rg.is_active = true AND rg.deleted_at IS NULL \
         AND rp.guest_view_all_features = false AND rp.deleted_at IS NULL))) ",
    );
    if let Some(pid) = project_id {
        qb.push(
            " AND EXISTS(SELECT 1 FROM project_pages ppf \
             WHERE ppf.page_id = p.id AND ppf.deleted_at IS NULL AND ppf.project_id = ",
        );
        qb.push_bind(pid);
        qb.push(") ");
    }
    if !search.is_empty() {
        qb.push(" AND p.name ILIKE '%' || ");
        qb.push_bind(search.to_string());
        qb.push(" || '%' ");
    }
}

/// GET `/api/workspaces/:slug/pages/` — workspace-level page listing.
/// Visibility mirrors the project `list` (`owned OR (public AND
/// not-restricted-guest)`, per linked project), aggregated over the
/// requester's member projects. Rows reuse `page_list_json`; the envelope is
/// the issue-style cursor envelope (`DetailEnvelope`).
pub async fn workspace_pages(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<WsPagesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let per_page = match parse_per_page(Some(q.per_page.as_deref().unwrap_or("100"))) {
        Ok(v) => v,
        Err(msg) => return Ok(ws_bad_request(json!({"detail": msg}))),
    };
    // per_page=0/negative behaves as 1 (explicit, documented deviation: the
    // project list has no pagination at all, so there is no Django behavior
    // to mirror for these degenerate values).
    let limit = per_page.clamp(1, 1000);
    let cursor = match parse_cursor(&q.cursor.clone().unwrap_or_else(|| format!("{limit}:0:0"))) {
        Ok(c) => c,
        Err(msg) => return Ok(ws_bad_request(json!({"detail": msg}))),
    };
    let tab = q.type_.as_deref().unwrap_or("all");
    let search = q.search.clone().unwrap_or_default();
    let (order_expr, desc) = sanitize_page_order_by(q.order_by.as_deref().unwrap_or("-updated_at"));
    let order_dir = if desc { "DESC" } else { "ASC" };

    let mut count_qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT COUNT(*) FROM pages p JOIN workspaces w ON w.id = p.workspace_id WHERE w.slug = ",
    );
    count_qb.push_bind(slug.clone());
    count_qb.push(" AND p.deleted_at IS NULL ");
    push_ws_pages_tail(&mut count_qb, auth.0, tab, q.project_id, &search);
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;

    let mut page_qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT p.id, p.name, p.owned_by_id, p.access, p.color, p.parent_id, \
         EXISTS(SELECT 1 FROM user_favorites uf WHERE uf.entity_type = 'page' \
         AND uf.entity_identifier = p.id AND uf.user_id = ",
    );
    page_qb.push_bind(auth.0);
    page_qb.push(
        " AND uf.workspace_id = p.workspace_id AND uf.deleted_at IS NULL) AS is_favorite, \
         p.is_locked, p.archived_at, p.workspace_id, p.created_at, p.updated_at, \
         p.created_by_id, p.updated_by_id, p.view_props, p.logo_props, p.description_html, \
         COALESCE(ARRAY(SELECT DISTINCT pl.label_id FROM page_labels pl \
             WHERE pl.page_id = p.id AND pl.deleted_at IS NULL AND pl.label_id IS NOT NULL), '{}') AS label_ids, \
         COALESCE(ARRAY(SELECT DISTINCT pp2.project_id FROM project_pages pp2 \
             JOIN project_members m2 ON m2.project_id = pp2.project_id AND m2.member_id = ",
    );
    page_qb.push_bind(auth.0);
    page_qb.push(
        " AND m2.is_active = true AND m2.deleted_at IS NULL \
         JOIN projects px ON px.id = pp2.project_id AND px.workspace_id = p.workspace_id \
         AND px.archived_at IS NULL AND px.deleted_at IS NULL \
         WHERE pp2.page_id = p.id AND pp2.deleted_at IS NULL), '{}') AS project_ids \
         FROM pages p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE w.slug = ",
    );
    page_qb.push_bind(slug);
    page_qb.push(" AND p.deleted_at IS NULL ");
    push_ws_pages_tail(&mut page_qb, auth.0, tab, q.project_id, &search);
    page_qb
        .push(" ORDER BY ")
        .push(order_expr)
        .push(" ")
        .push(order_dir)
        .push(", p.id ASC LIMIT ")
        .push_bind(limit + 1);
    let window = match page_window(cursor.page, limit) {
        Ok(w) => w,
        Err(()) => return Ok(ws_bad_request(json!({"detail": "Error in parsing"}))),
    };
    let mut rows: Vec<PageRow> = match window {
        PageWindow::Rows(offset) => {
            page_qb.push(" OFFSET ").push_bind(offset);
            page_qb.build_query_as().fetch_all(&st.pool).await?
        }
        PageWindow::BeyondEnd => Vec::new(),
    };
    let _ = (cursor.limit_value, cursor.is_prev);
    let next_page_results = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let results: Vec<Value> = rows.iter().map(page_list_json).collect::<Vec<_>>();
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
        results,
    };
    Ok((StatusCode::OK, Json(json!(envelope))))
}
```

- [ ] **Step 3: Register the route in `main.rs`**

Right before the project pages list block (`"/api/workspaces/:slug/projects/:project_id/pages/"`), insert:

```rust
        // Parity with a workspace-level pages collection: GET flat,
        // member-project-scoped, cursor-paginated `page_list_json` rows
        // (E2h; the project `list` visibility shape, aggregated).
        .route(
            "/api/workspaces/:slug/pages/",
            get(routes::page::workspace_pages),
        )
```

- [ ] **Step 4: Compile-check the backend**

Run from `apps/api-rs`:

```bash
cargo check -p api
```

Expected: no errors (warnings acceptable if pre-existing).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/page.rs apps/api-rs/crates/api/src/main.rs
git commit -m "feat(api): workspace pages listing endpoint"
```

---

### Task 2: Backend integration tests

**Files:**

- Create: `apps/api-rs/crates/api/tests/page_workspace_test.rs`

Mirror the fixture style of `page_parent_filter_test.rs` (same `pool()`,
`state()`, `insert_page` extended with `access: i16` and `archived: bool`,
`Scratch` with `cleanup`, run serially with `-- --test-threads=1`).

Fixture layout (single scratch workspace, slug prefix `kbw-`):

- Users: `alice` (workspace admin, role 20 in `p1` + `p2`), `bob`
  (role 15 in `p1`), `carol` (role 5 guest in `p2`), `mallory`
  (workspace member, no project memberships), `dave` (role 5 guest in `p1`,
  added in the guest test), plus an unknown UUID for the non-member 403 case.
- Projects: `p1` (`guest_view_all_features = true`), `p2`
  (`guest_view_all_features = false`), `p3` (no memberships for alice).
- Extend `insert_page` with `access: i16` and `archived: bool` parameters
  (`archived_at = CASE WHEN $archived THEN CURRENT_DATE ELSE NULL END` style —
  write it as an explicit parameter in the VALUES).
- Pages (all with `project_pages` links):
  1. `Alpha Guide` — public, owner alice, linked `p1`.
  2. `Alice Private` — private, owner alice, linked `p1`.
  3. `Bob Private` — private, owner bob, linked `p1`.
  4. `Beta Manual` — public, owner alice, linked `p2`.
  5. `Carol Private` — private, owner carol, linked `p2`.
  6. `Old Runbook` — public **archived**, owner alice, linked `p1`.
  7. `Multi SOP` — public, owner alice, linked `p1` + `p2` + `p3`.
  8. `Child Note` — public, owner alice, linked `p1`, `parent_id` = `Alpha Guide`.

Cleanup: extend the existing `cleanup` to also delete `project_pages` and
`project_members` for every scratch project id and delete every scratch
project row (loop over the project ids).

- [ ] **Step 1: Write the test file**

Create `apps/api-rs/crates/api/tests/page_workspace_test.rs` with the fixture
above and these test cases (call `workspace_pages` directly like the
parent-filter suite calls `list`; assert with a `names()` helper over
`body["results"]`):

1. `non_workspace_member_gets_403` — unknown user id → `(403, {"detail": ...})`.
2. `non_project_member_sees_empty_list` — `mallory` → `200`, `results = []`, `total_results = 0`.
3. `all_tab_excludes_private_others_and_archived` — as `alice`, default query:
   names = `Alpha Guide`, `Alice Private`, `Beta Manual`, `Multi SOP`, `Child Note`;
   `Bob Private`, `Carol Private`, `Old Runbook` absent.
4. `public_tab` — as `alice`, `type_ = Some("public")`: only the visible public
   unarchived pages (`Alpha Guide`, `Beta Manual`, `Multi SOP`, `Child Note`).
5. `private_tab` — as `alice`, `type_ = Some("private")`: only `Alice Private`.
6. `archived_tab` — as `alice`, `type_ = Some("archived")`: only `Old Runbook`.
7. `invalid_type_falls_back_to_all` — `type_ = Some("nope")` returns the same
   set as the default query.
8. `project_filter_scopes_links` — as `alice`, `project_id = Some(p2)`:
   `Beta Manual` and `Multi SOP` present; `Alpha Guide` absent;
   `Carol Private` absent (visibility still enforced).
9. `restricted_guest_sees_only_own_pages` — as `carol`: only `Carol Private`
   (`Beta Manual` and `Multi SOP` are public in `p2` but carol is restricted).
10. `guest_in_open_project_sees_public` — as `dave` (role 5 in `p1`):
    `Alpha Guide`, `Multi SOP`, `Child Note` present; `Alice Private`,
    `Bob Private`, `Old Runbook` absent.
11. `search_filters_by_name` — `search = Some("guide")` matches `Alpha Guide`
    only (case-insensitive).
12. `pagination_cursors_page_results` — `per_page = Some("2")`:
    `next_page_results = true`, `total_results` equals the full-set count;
    second fetch with `cursor = next_cursor` returns the next slice with no
    overlap; final page has `next_page_results = false`.
13. `multi_linked_page_appears_once_with_member_projects` — `Multi SOP`
    appears exactly once and its `project_ids` contains exactly `{p1, p2}`
    (NOT `p3`).
14. `child_pages_have_parent_field` — `Child Note` row has
    `"parent" = "<Alpha Guide id>"`.

End every test with `scratch.cleanup(&pool).await;`.

- [ ] **Step 2: Run the tests**

Run from `apps/api-rs`:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test page_workspace_test -- --test-threads=1
```

Expected: all tests pass. If any fail, fix the handler/SQL from Task 1 —
**do not weaken the test expectations** unless they contradict the approved
spec; in that case stop and ask.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/page_workspace_test.rs
git commit -m "test(api): workspace pages endpoint coverage"
```

---

### Task 3: Web helpers (TDD)

**Files:**

- Create: `apps/web/core/lib/workspace-kb.ts`
- Test: `apps/web/core/lib/workspace-kb.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/lib/workspace-kb.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  appendKbPage,
  buildWorkspaceKbParams,
  createKbPageState,
  groupWorkspaceKbPages,
  normalizeKbResponse,
  normalizeWorkspaceKbType,
  type TWorkspaceKbPage,
} from "./workspace-kb";

const page = (id: string, projectIds: string[]) =>
  ({
    id,
    name: id,
    project_ids: projectIds,
  }) as unknown as TWorkspaceKbPage;

describe("normalizeWorkspaceKbType", () => {
  it("defaults unknown and empty values to all", () => {
    expect(normalizeWorkspaceKbType(undefined)).toBe("all");
    expect(normalizeWorkspaceKbType("nope")).toBe("all");
  });

  it("passes through known tabs", () => {
    expect(normalizeWorkspaceKbType("public")).toBe("public");
    expect(normalizeWorkspaceKbType("private")).toBe("private");
    expect(normalizeWorkspaceKbType("archived")).toBe("archived");
  });
});

describe("buildWorkspaceKbParams", () => {
  it("always sends paging and ordering", () => {
    expect(buildWorkspaceKbParams({})).toEqual({ per_page: "100", order_by: "-updated_at" });
  });

  it("includes type, project and trimmed search", () => {
    expect(buildWorkspaceKbParams({ typeFilter: "public", projectId: "p2", search: "  sop  " })).toEqual({
      per_page: "100",
      order_by: "-updated_at",
      type: "public",
      project_id: "p2",
      search: "sop",
    });
  });

  it("drops unknown types and blank search", () => {
    expect(buildWorkspaceKbParams({ typeFilter: "nope", search: "   " })).toEqual({
      per_page: "100",
      order_by: "-updated_at",
    });
  });
});

describe("normalizeKbResponse", () => {
  it("passes through array results and normalizes the cursor", () => {
    expect(
      normalizeKbResponse({
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
    expect(normalizeKbResponse({ results: { a: [] }, next_cursor: null })).toEqual({
      results: [],
      nextCursor: null,
      hasMore: false,
      totalResults: 0,
    });
  });
});

describe("createKbPageState / appendKbPage", () => {
  it("starts empty", () => {
    expect(createKbPageState()).toEqual({ results: [], nextCursor: null, hasMore: false, totalResults: 0 });
  });

  it("appends pages preserving order and updates the cursor", () => {
    const first = createKbPageState({
      results: [page("a", ["p1"])],
      nextCursor: "100:1:0",
      hasMore: true,
      totalResults: 2,
    });
    const second = appendKbPage(first, {
      results: [page("b", ["p2"])],
      nextCursor: null,
      hasMore: false,
      totalResults: 2,
    });
    expect(second.results.map((row) => row.id)).toEqual(["a", "b"]);
    expect(second.nextCursor).toBeNull();
    expect(second.hasMore).toBe(false);
    expect(second.totalResults).toBe(2);
  });
});

describe("groupWorkspaceKbPages", () => {
  const names: Record<string, string> = { p1: "Platform", p2: "Billing", p3: "Auth" };
  const projectName = (id: string) => names[id] ?? id;

  it("dedupes multi-linked pages and groups by the first alphabetical project", () => {
    const groups = groupWorkspaceKbPages(
      [page("a", ["p2", "p1"]), page("b", ["p2"]), page("a", ["p2", "p1"])],
      projectName
    );
    expect(groups.map((group) => group.key)).toEqual(["p2", "p1"]);
    expect(groups[0].label).toBe("Billing");
    expect(groups[0].pages.map((row) => row.id)).toEqual(["b"]);
    expect(groups[1].label).toBe("Platform");
    expect(groups[1].pages.map((row) => row.id)).toEqual(["a"]);
  });

  it("skips pages without member projects", () => {
    expect(groupWorkspaceKbPages([page("a", [])], projectName)).toEqual([]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/lib/workspace-kb`
Expected: FAIL — cannot resolve `./workspace-kb`.

- [ ] **Step 3: Write the implementation**

Create `apps/web/core/lib/workspace-kb.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TLogoProps } from "@plane/types";

export type TWorkspaceKbTab = "all" | "public" | "private" | "archived";

export type TWorkspaceKbPage = {
  id: string;
  name: string;
  access: number;
  owned_by: string;
  parent: string | null;
  is_favorite: boolean;
  is_locked: boolean;
  archived_at: string | null;
  created_at: string;
  updated_at: string;
  logo_props: TLogoProps | null;
  project_ids: string[];
};

export type TWorkspaceKbPageState = {
  results: TWorkspaceKbPage[];
  nextCursor: string | null;
  hasMore: boolean;
  totalResults: number;
};

export function normalizeWorkspaceKbType(typeParam?: string | null): TWorkspaceKbTab {
  return typeParam === "public" || typeParam === "private" || typeParam === "archived" ? typeParam : "all";
}

export function buildWorkspaceKbParams({
  typeFilter,
  projectId,
  search,
}: {
  typeFilter?: string;
  projectId?: string;
  search?: string;
}): Record<string, string> {
  const params: Record<string, string> = { per_page: "100", order_by: "-updated_at" };
  const type = normalizeWorkspaceKbType(typeFilter);
  if (type !== "all") params.type = type;
  if (projectId) params.project_id = projectId;
  const normalizedSearch = search?.trim() ?? "";
  if (normalizedSearch) params.search = normalizedSearch;
  return params;
}

export function normalizeKbResponse(response: unknown): TWorkspaceKbPageState {
  const data = (response ?? {}) as {
    results?: unknown;
    next_cursor?: unknown;
    next_page_results?: unknown;
    total_results?: unknown;
  };
  const results = Array.isArray(data.results) ? (data.results as TWorkspaceKbPage[]) : [];
  const nextCursor = typeof data.next_cursor === "string" ? data.next_cursor : null;
  return {
    results,
    nextCursor,
    hasMore: data.next_page_results === true && nextCursor !== null,
    totalResults: typeof data.total_results === "number" ? data.total_results : 0,
  };
}

export function createKbPageState(envelope?: TWorkspaceKbPageState): TWorkspaceKbPageState {
  if (!envelope) return { results: [], nextCursor: null, hasMore: false, totalResults: 0 };
  return envelope;
}

export function appendKbPage(state: TWorkspaceKbPageState, envelope: TWorkspaceKbPageState): TWorkspaceKbPageState {
  return {
    results: [...state.results, ...envelope.results],
    nextCursor: envelope.nextCursor,
    hasMore: envelope.hasMore,
    totalResults: envelope.totalResults,
  };
}

export function groupWorkspaceKbPages(
  pages: TWorkspaceKbPage[],
  projectName: (projectId: string) => string
): { key: string; label: string; pages: TWorkspaceKbPage[] }[] {
  const seen = new Set<string>();
  const unique: TWorkspaceKbPage[] = [];
  for (const page of pages) {
    if (seen.has(page.id)) continue;
    seen.add(page.id);
    unique.push(page);
  }
  const byProject = new Map<string, TWorkspaceKbPage[]>();
  for (const page of unique) {
    const linked = [...(page.project_ids ?? [])].sort((a, b) => projectName(a).localeCompare(projectName(b)));
    const primary = linked[0];
    if (!primary) continue;
    const list = byProject.get(primary) ?? [];
    list.push(page);
    byProject.set(primary, list);
  }
  return [...byProject.entries()]
    .map(([key, list]) => ({ key, label: projectName(key), pages: list }))
    .sort((a, b) => a.label.localeCompare(b.label));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web test core/lib/workspace-kb`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/workspace-kb.ts apps/web/core/lib/workspace-kb.test.ts
git commit -m "feat(web): workspace KB list helpers"
```

---

### Task 4: KB page (service, list view, page, layout, route, i18n)

**Files:**

- Modify: `apps/web/core/services/page/project-page.service.ts` (add `getWorkspacePages`)
- Create: `apps/web/core/components/pages/workspace/workspace-kb-list-view.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/kb/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/kb/page.tsx`
- Modify: `apps/web/app/routes/core.ts` (insert after the workspace tracks block)
- Modify: `packages/i18n/src/locales/en/empty-state.json` (merge `kb` into `workspace_empty_state`)

- [ ] **Step 1: Add the service method**

In `apps/web/core/services/page/project-page.service.ts`, after the
`fetchArchived` method, add:

```ts
  async getWorkspacePages(
    workspaceSlug: string,
    params?: Record<string, string | number>,
    config = {}
  ): Promise<{ results: any[]; next_cursor: string | null; next_page_results: boolean; total_results: number }> {
    return this.get(`/api/workspaces/${workspaceSlug}/pages/`, { params }, config)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
```

- [ ] **Step 2: Create the list view**

Create `apps/web/core/components/pages/workspace/workspace-kb-list-view.tsx`
(the full component, mirroring the tickets view plus the tab row):

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
import { PagesOutline } from "@makeplane/propel/icons";
import { Logo } from "@plane/propel/emoji-icon-picker";
import { getFileURL, getPageName, renderFormattedDate } from "@plane/utils";
// components
import { AvatarGroupOverflow } from "@/components/common/avatar-group-overflow";
import { ListItem } from "@/components/core/list";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import {
  appendKbPage,
  buildWorkspaceKbParams,
  createKbPageState,
  groupWorkspaceKbPages,
  normalizeKbResponse,
  normalizeWorkspaceKbType,
  type TWorkspaceKbPage,
  type TWorkspaceKbTab,
} from "@/lib/workspace-kb";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";
// services
import { ProjectPageService } from "@/services/page/project-page.service";

const pageService = new ProjectPageService();

const KB_TAB_OPTIONS: { key: TWorkspaceKbTab; label: string }[] = [
  { key: "all", label: "All" },
  { key: "public", label: "Public" },
  { key: "private", label: "Private" },
  { key: "archived", label: "Archived" },
];

type TKbRowProps = {
  page: TWorkspaceKbPage;
  projectId: string;
  workspaceSlug: string;
};

const KbRow = observer(function KbRow({ page, projectId, workspaceSlug }: TKbRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const { getUserDetails } = useMember();

  const owner = page.owned_by ? getUserDetails(page.owned_by) : undefined;

  return (
    <ListItem
      title={getPageName(page.name)}
      itemLink={`/${workspaceSlug}/projects/${projectId}/pages/${page.id}`}
      prependTitleElement={
        page.logo_props?.in_use ? (
          <Logo logo={page.logo_props} size={16} type="lucide" />
        ) : (
          <PagesOutline className="h-4 w-4 text-tertiary" />
        )
      }
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {page.project_ids.map((linkedId) => {
            const linkedProject = getProjectById(linkedId);
            if (!linkedProject) return null;
            return (
              <span key={linkedId} className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">
                {linkedProject.name}
              </span>
            );
          })}
          <span className="rounded-sm bg-layer-2 px-1.5 py-0.5">{page.access === 1 ? "Private" : "Public"}</span>
          {owner && (
            <span className="hidden items-center gap-1.5 lg:flex">
              <AvatarGroupOverflow size="xs">
                <Avatar
                  alt={owner.display_name}
                  fallback={owner.display_name?.[0]?.toUpperCase()}
                  src={getFileURL(owner.avatar_url ?? "")}
                />
              </AvatarGroupOverflow>
              <span className="max-w-[100px] truncate">{owner.display_name}</span>
            </span>
          )}
          <span className="hidden md:inline">{renderFormattedDate(page.updated_at)}</span>
        </div>
      }
      parentRef={parentRef}
    />
  );
});

type Props = {
  typeFilter?: string;
  projectFilter?: string;
};

export const WorkspaceKbListView = observer(function WorkspaceKbListView({ typeFilter, projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { workspaceProjectIds, getProjectById } = useProject();
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [pageState, setPageState] = useState(() => createKbPageState());
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);

  const slug = workspaceSlug?.toString();
  const normalizedType = normalizeWorkspaceKbType(typeFilter);
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
    const params = buildWorkspaceKbParams({
      typeFilter: normalizedType,
      projectId: normalizedProjectFilter,
      search: debouncedSearch,
    });
    void pageService
      .getWorkspacePages(slug, params, { signal: controller.signal })
      .then((response) => setPageState(createKbPageState(normalizeKbResponse(response))))
      .catch(() => undefined)
      .finally(() => {
        if (!controller.signal.aborted) setIsLoading(false);
      });
    return () => controller.abort();
  }, [slug, normalizedType, normalizedProjectFilter, debouncedSearch]);

  const groups = useMemo(
    () => groupWorkspaceKbPages(pageState.results, (id) => getProjectById(id)?.name ?? id),
    [pageState.results, getProjectById]
  );

  const renderedGroups: TWorkspaceAggregateGroupItem[] = groups.map((group) => ({
    key: group.key,
    label: group.label,
    count: group.pages.length,
    items: (
      <>
        {group.pages.map((page) => (
          <KbRow
            key={page.id}
            page={page}
            projectId={
              normalizedProjectFilter && page.project_ids.includes(normalizedProjectFilter)
                ? normalizedProjectFilter
                : group.key
            }
            workspaceSlug={slug ?? ""}
          />
        ))}
      </>
    ),
  }));

  const buildTabHref = (nextType: TWorkspaceKbTab) => {
    if (!slug) return "#";
    const params = new URLSearchParams();
    if (nextType !== "all") params.set("type", nextType);
    if (normalizedProjectFilter) params.set("project", normalizedProjectFilter);
    const query = params.toString();
    return `/${slug}/kb${query ? `?${query}` : ""}`;
  };

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    const params = new URLSearchParams();
    if (normalizedType !== "all") params.set("type", normalizedType);
    if (projectId) params.set("project", projectId);
    const query = params.toString();
    router.push(`/${slug}/kb${query ? `?${query}` : ""}`);
  };

  const handleLoadMore = () => {
    if (!slug || !pageState.hasMore || !pageState.nextCursor || isLoadingMore) return;
    setIsLoadingMore(true);
    const params = {
      ...buildWorkspaceKbParams({
        typeFilter: normalizedType,
        projectId: normalizedProjectFilter,
        search: debouncedSearch,
      }),
      cursor: pageState.nextCursor,
    };
    void pageService
      .getWorkspacePages(slug, params, {})
      .then((response) => setPageState((state) => appendKbPage(state, normalizeKbResponse(response))))
      .catch(() => undefined)
      .finally(() => setIsLoadingMore(false));
  };

  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      <div className="flex items-center gap-1 border-b border-subtle px-3 py-1.5">
        {KB_TAB_OPTIONS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => router.push(buildTabHref(tab.key))}
            className={
              tab.key === normalizedType
                ? "rounded-sm bg-layer-3 px-2.5 py-1 text-12 font-medium text-primary"
                : "rounded-sm px-2.5 py-1 text-12 text-secondary hover:bg-layer-transparent-hover"
            }
          >
            {tab.label}
          </button>
        ))}
      </div>
      <WorkspaceAggregateList
        projectFilter={normalizedProjectFilter}
        onProjectFilterChange={handleProjectFilterChange}
        search={search}
        onSearchChange={setSearch}
        searchPlaceholder={t("service.catalog_search_placeholder")}
        groups={renderedGroups}
        totalCount={pageState.totalResults}
        isLoading={isLoading}
        emptyTitle={t("workspace_empty_state.kb.title")}
        emptyDescription={t("workspace_empty_state.kb.description")}
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
});
```

- [ ] **Step 3: Create the layout**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/kb/layout.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { Outlet } from "react-router";
// icons
import { PagesOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";

const WorkspaceKbBreadcrumbs = observer(function WorkspaceKbBreadcrumbs() {
  const router = useAppRouter();
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("sidebar.pages")}
                href={`/${workspaceSlug}/kb/`}
                icon={<PagesOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});

export default function WorkspaceKbLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceKbBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

- [ ] **Step 4: Create the page**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/kb/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WorkspaceKbListView } from "@/components/pages/workspace/workspace-kb-list-view";

function WorkspaceKbPage() {
  const searchParams = useSearchParams();
  const typeFilter = searchParams.get("type") ?? undefined;
  const projectFilter = searchParams.get("project") ?? undefined;
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("sidebar.pages")} />
      <WorkspaceKbListView typeFilter={typeFilter} projectFilter={projectFilter} />
    </>
  );
}

export default observer(WorkspaceKbPage);
```

- [ ] **Step 5: Add the route**

In `apps/web/app/routes/core.ts`, right after the "Workspace Tracks" block
(before the Tickets block), insert:

```ts
        // Workspace KB
        layout("./(all)/[workspaceSlug]/(projects)/kb/layout.tsx", [
          route(":workspaceSlug/kb", "./(all)/[workspaceSlug]/(projects)/kb/page.tsx"),
        ]),
```

- [ ] **Step 6: Add the empty-state keys**

In `packages/i18n/src/locales/en/empty-state.json`, inside the existing
`workspace_empty_state` namespace (next to `sprints`, `tracks`, `tickets`), add:

```json
    "kb": {
      "title": "No pages yet",
      "description": "Knowledge base pages from projects you are a member of will show up here."
    },
```

- [ ] **Step 7: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 8: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/kb/layout.tsx" \
  "apps/web/app/(all)/[workspaceSlug]/(projects)/kb/page.tsx" \
  apps/web/core/components/pages/workspace/workspace-kb-list-view.tsx \
  apps/web/core/services/page/project-page.service.ts \
  apps/web/app/routes/core.ts packages/i18n/src/locales/en/empty-state.json
git commit -m "feat(web): workspace KB page"
```

---

### Task 5: Sidebar entry

**Files:**

- Modify: `packages/constants/src/workspace.ts`
- Modify: `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx`

- [ ] **Step 1: Add the nav item**

In `packages/constants/src/workspace.ts`, inside
`WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS`, add after the `services` entry:

```ts
  kb: {
    key: "kb",
    labelTranslationKey: "sidebar.pages",
    href: `/kb/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER, EUserWorkspaceRoles.GUEST],
    highlight: (pathname: string, url: string) => pathname.includes(url),
  },
```

Wait — where exactly is the `services` entry? It sits between other entries;
place `kb` right after it. Then update the links list to:

```ts
export const WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS_LINKS: IWorkspaceSidebarNavigationItem[] = [
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["home"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["tickets"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["sprints"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["tracks"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["services"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["kb"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["ai_scheduler"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["docs"],
];
```

- [ ] **Step 2: Register the item key**

In `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`, update `staticItems`
to include `"kb"` after `"services"`.

- [ ] **Step 3: Add the icon**

In `apps/web/core/components/workspace/sidebar/helper.tsx`, add `PagesOutline`
to the `@makeplane/propel/icons` import and add a case:

```tsx
    case "kb":
      return <PagesOutline className={cn("size-4 flex-shrink-0", className)} />;
```

- [ ] **Step 4: Rebuild the constants package**

Web consumes the built `@plane/constants` dist.

Run: `pnpm --filter=@plane/constants build`
Expected: build succeeds.

- [ ] **Step 5: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 6: Commit**

```bash
git add packages/constants/src/workspace.ts \
  apps/web/core/components/workspace/sidebar/sidebar-item.tsx \
  apps/web/core/components/workspace/sidebar/helper.tsx
git commit -m "feat(web): workspace KB sidebar entry"
```

---

### Task 6: Full verification and deploy

- [ ] **Step 1: Run the backend tests**

Run from `apps/api-rs`:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test page_workspace_test -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 2: Rebuild and restart the API (schema unchanged, no migration —
      skip the Docker rebuild; the running api-rs already serves the new binary
      only after a rebuild — wait, the endpoint is new code, so the running
      container does NOT have it.**

Correction — the endpoint is new Rust code, so the running API must be
rebuilt and the BE stack restarted for the smoke test to hit it. Follow
AGENTS.md: rebuild `api` (+ `worker`, `beat-worker` — unchanged, but the
stack shares the image; rebuild only what's needed per AGENTS.md:
`docker compose -f docker-compose-local.yml up -d --build api worker` —
confirm the exact AGENTS.md wording before running; see note below).

NOTE: the AGENTS.md backend-rebuild procedure uses a detached build (LTO
link takes 10+ minutes) and specific verification steps. Read
`AGENTS.md` before running this step — the exact commands live there, not in
this plan.

- [ ] **Step 3: Run the full web test suite**

Run: `pnpm --filter=web test`
Expected: all test files pass (395 + new helper tests).

- [ ] **Step 4: Lint**

Run: `pnpm --filter=web check:lint`
Expected: exit 0 (warnings may exist at baseline; none may point at the new files).

- [ ] **Step 5: Build constants and web**

```bash
pnpm --filter=@plane/constants build
pnpm --filter=web build
```

Expected: both succeed.

- [ ] **Step 6: Restart the prod web service and smoke test**

```bash
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w "%{http_code}" http://localhost:3000
```

Expected: `200`.

Manual smoke on the tunnel:

1. Sidebar shows **Knowledge base** after Services.
2. `/[workspaceSlug]/kb` loads; All/Public/Private/Archived tabs filter server-side.
3. Multi-project pages appear once with project badges; private pages of others never appear.
4. `?project=<id>` and search filter server-side; Load more appends.
5. Clicking a row opens the project page detail.
6. A restricted guest only sees their own pages.

- [ ] **Step 7: Report**

Report the smoke results. No commit for this task.

---

## Self-review notes

- Spec coverage: tabs (Tasks 1, 3, 4), server visibility matrix (Tasks 1-2),
  multi-link dedup + member-scoped `project_ids` (Tasks 1-4), cursor pagination
  - envelope (Tasks 1, 3), invalid type/project fallback (Tasks 1, 3-4),
    merged i18n keys (Task 4), typecheck/tests (Tasks 1-5), rebuild + deploy
    with the AGENTS.md backend procedure called out (Task 6).
- Type consistency: web rows use `TWorkspaceKbPage` (superset-compatible with
  `page_list_json` keys); the reducer/envelope/group types mirror the proven
  tickets pattern. Backend `PageRow` is reused as-is (no new row struct) and
  `page_list_json` serializes rows.
- The plan intentionally does NOT reuse `issue_common::DetailIssuesQuery` (it
  carries issue-specific legacy keys) — the endpoint has its own minimal
  `WsPagesQuery`.
- `parent` is carried through rows but not rendered in v1 (documented
  limitation, spec-consistent).
