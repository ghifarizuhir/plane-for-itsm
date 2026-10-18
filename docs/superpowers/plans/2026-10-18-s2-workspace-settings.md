# S2 — Workspace Settings + Feature Flags Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Workspace-level feature settings exist as a single config set: `workspace_settings` table + union backfill, canonical `GET/PATCH /api/workspaces/:slug/settings/`, AI tools read workspace flags, Features admin page, and workspace sidebar (Sprints/Tracks/KB/Intake) gated by the flags.

**Architecture:** Additive-only (master program pattern). New table + idempotent union backfill; new endpoint pair mirroring `document_export` get/patch handlers; AI queries swap `projects.<flag>` predicates to `workspace_settings`; web adds a service + admin Features page + sidebar flag filter. Project flag columns keep working untouched until their domain phases (S6/S7/S8 recompute the union at cutover — no dual-write).

**Tech Stack:** Rust (sqlx, axum), PostgreSQL, React + MobX + SWR, oxfmt/oxlint, vitest, cargo tests.

**Program context:** Sub-project S2 of `docs/superpowers/specs/2026-10-18-project-to-application-design.md`. Locked decisions applied: S2 scope = flags + assignee + archive rules only (timezone already lives on `workspaces`; `network` stays per-application; `estimate_id` pointer lands in S3).

**Out of scope (later phases):** switching project-scoped readers (app nav, project settings pages, intake auto-create, PowerK gating) — S6/S7/S8. Integrations/importers — S9.

**Key semantics (locked):**

- Booleans: union = enabled-if-any (`BOOL_OR`); fresh workspaces default all-ON except `guest_view_all_features` (default OFF).
- Scalars (`archive_in`, `close_in`, `default_assignee_id`): most-frequent non-NULL value; ties → canonical application (most non-deleted issues); all-NULL → `0` / `NULL`.
- `timezone`: already `workspaces.timezone NOT NULL` — S2 does not touch it (project create already copies it).
- PATCH requires workspace ADMIN (`role >= 20`, mirroring `apply_ws_patch` in `routes/workspace.rs:459-468`); GET requires any active workspace membership (mirroring `document_export::get_settings`).
- `default_assignee_id` on PATCH: must be `NULL` or an active workspace member, else 400. `archive_in`/`close_in`: must be `>= 0`, else 400. Unknown JSON keys are ignored (typed-struct behavior, same as document-export).
- Divergence window: project-level flag edits between S2 and each domain cutover are absorbed by re-running the union at cutover (S6/S7/S8). Documented, accepted.

---

### Task 1: Migration `0021_workspace_settings.sql`

**Files:**

- Create: `apps/api-rs/migrations/0021_workspace_settings.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Workspace-level feature settings (program P1/S2: single config set).
-- Additive: new table + idempotent union backfill. Project flag columns stay
-- untouched until S6/S7/S8 recompute this union at their cutovers.

CREATE TABLE IF NOT EXISTS public.workspace_settings (
    id uuid NOT NULL PRIMARY KEY,
    workspace_id uuid NOT NULL UNIQUE REFERENCES public.workspaces(id) ON DELETE CASCADE,
    cycle_view boolean NOT NULL DEFAULT true,
    module_view boolean NOT NULL DEFAULT true,
    issue_views_view boolean NOT NULL DEFAULT true,
    page_view boolean NOT NULL DEFAULT true,
    intake_view boolean NOT NULL DEFAULT false,
    is_issue_type_enabled boolean NOT NULL DEFAULT false,
    is_time_tracking_enabled boolean NOT NULL DEFAULT false,
    guest_view_all_features boolean NOT NULL DEFAULT false,
    archive_in integer NOT NULL DEFAULT 0,
    close_in integer NOT NULL DEFAULT 0,
    default_assignee_id uuid,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

-- Idempotent union backfill, one row per live workspace. Scalars use the
-- most-frequent non-null project value; ties go to the canonical application
-- (most non-deleted issues); all-null falls back to column defaults.
INSERT INTO public.workspace_settings (
    id, workspace_id, cycle_view, module_view, issue_views_view, page_view,
    intake_view, is_issue_type_enabled, is_time_tracking_enabled,
    guest_view_all_features, archive_in, close_in, default_assignee_id
)
SELECT gen_random_uuid(), w.id,
    COALESCE(BOOL_OR(p.cycle_view), true),
    COALESCE(BOOL_OR(p.module_view), true),
    COALESCE(BOOL_OR(p.issue_views_view), true),
    COALESCE(BOOL_OR(p.page_view), true),
    COALESCE(BOOL_OR(p.intake_view), false),
    COALESCE(BOOL_OR(p.is_issue_type_enabled), false),
    COALESCE(BOOL_OR(p.is_time_tracking_enabled), false),
    COALESCE(BOOL_OR(p.guest_view_all_features), false),
    COALESCE(mode_int.mode_val, 0),
    COALESCE(mode_close.mode_val, 0),
    mode_assignee.mode_val
FROM public.workspaces w
LEFT JOIN public.projects p ON p.workspace_id = w.id AND p.deleted_at IS NULL
LEFT JOIN LATERAL (
    SELECT p2.archive_in AS mode_val
    FROM public.projects p2
    WHERE p2.workspace_id = w.id AND p2.deleted_at IS NULL AND p2.archive_in IS NOT NULL
    GROUP BY p2.archive_in
    ORDER BY COUNT(*) DESC,
        MAX((SELECT COUNT(*) FROM public.issues i3 WHERE i3.project_id = p2.id AND i3.deleted_at IS NULL)) DESC,
        MIN(p2.created_at)
    LIMIT 1
) mode_int ON true
LEFT JOIN LATERAL (
    SELECT p2.close_in AS mode_val
    FROM public.projects p2
    WHERE p2.workspace_id = w.id AND p2.deleted_at IS NULL AND p2.close_in IS NOT NULL
    GROUP BY p2.close_in
    ORDER BY COUNT(*) DESC,
        MAX((SELECT COUNT(*) FROM public.issues i3 WHERE i3.project_id = p2.id AND i3.deleted_at IS NULL)) DESC,
        MIN(p2.created_at)
    LIMIT 1
) mode_close ON true
LEFT JOIN LATERAL (
    SELECT p2.default_assignee_id AS mode_val
    FROM public.projects p2
    WHERE p2.workspace_id = w.id AND p2.deleted_at IS NULL AND p2.default_assignee_id IS NOT NULL
    GROUP BY p2.default_assignee_id
    ORDER BY COUNT(*) DESC,
        MAX((SELECT COUNT(*) FROM public.issues i3 WHERE i3.project_id = p2.id AND i3.deleted_at IS NULL)) DESC,
        MIN(p2.created_at)
    LIMIT 1
) mode_assignee ON true
WHERE w.deleted_at IS NULL
GROUP BY w.id, mode_int.mode_val, mode_close.mode_val, mode_assignee.mode_val
ON CONFLICT (workspace_id) DO UPDATE SET
    cycle_view = EXCLUDED.cycle_view,
    module_view = EXCLUDED.module_view,
    issue_views_view = EXCLUDED.issue_views_view,
    page_view = EXCLUDED.page_view,
    intake_view = EXCLUDED.intake_view,
    is_issue_type_enabled = EXCLUDED.is_issue_type_enabled,
    is_time_tracking_enabled = EXCLUDED.is_time_tracking_enabled,
    guest_view_all_features = EXCLUDED.guest_view_all_features,
    archive_in = EXCLUDED.archive_in,
    close_in = EXCLUDED.close_in,
    default_assignee_id = EXCLUDED.default_assignee_id,
    updated_at = now();
```

NOTE: the file must NOT contain any `canon` CTE — mode tie-breaks are inline in each LATERAL subquery.

- [ ] **Step 2: Validate SQL safely on the dev DB (transaction, rolled back)**

Run:

```bash
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -v ON_ERROR_STOP=1 -c "BEGIN;" -f /dev/stdin -c "SELECT workspace_id, cycle_view, intake_view, archive_in FROM workspace_settings;" -c "ROLLBACK;" < apps/api-rs/migrations/0021_workspace_settings.sql
```

Expected: no error; the SELECT shows one row per live workspace with unioned flags; ROLLBACK leaves the DB unchanged. (Do NOT apply manually — `sqlx::migrate!` is compile-time embedded; the real apply happens at API boot after rebuild in Task 6. Manual apply would desync the migration ledger.)

- [ ] **Step 3: Commit the migration file alone**

```bash
git add apps/api-rs/migrations/0021_workspace_settings.sql
git commit -m "feat(api): add workspace_settings table and union backfill"
```

---

### Task 2: Rust `workspace_settings` routes + tests

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/workspace_settings.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs` (add `pub mod workspace_settings;` alphabetical), `apps/api-rs/crates/api/src/main.rs` (register route after document-export-settings, ~line 690)
- Create: `apps/api-rs/crates/api/tests/workspace_settings_test.rs`

Handler contract (mirror `routes/document_export.rs:131-240` exactly for structure, gates, and error shapes):

```rust
//! Workspace-level feature settings (`workspace_settings`, program P1/S2).

use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::routes::{project::{deny, missing, ws_role}, v1::work_item_type::ws_id};
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

#[derive(Debug, Clone, FromRow)]
pub struct WorkspaceSettingsRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub cycle_view: bool,
    pub module_view: bool,
    pub issue_views_view: bool,
    pub page_view: bool,
    pub intake_view: bool,
    pub is_issue_type_enabled: bool,
    pub is_time_tracking_enabled: bool,
    pub guest_view_all_features: bool,
    pub archive_in: i32,
    pub close_in: i32,
    pub default_assignee_id: Option<Uuid>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateWorkspaceSettings {
    #[serde(default)] pub cycle_view: Option<bool>,
    #[serde(default)] pub module_view: Option<bool>,
    #[serde(default)] pub issue_views_view: Option<bool>,
    #[serde(default)] pub page_view: Option<bool>,
    #[serde(default)] pub intake_view: Option<bool>,
    #[serde(default)] pub is_issue_type_enabled: Option<bool>,
    #[serde(default)] pub is_time_tracking_enabled: Option<bool>,
    #[serde(default)] pub guest_view_all_features: Option<bool>,
    #[serde(default)] pub archive_in: Option<i32>,
    #[serde(default)] pub close_in: Option<i32>,
    #[serde(default)] pub default_assignee_id: Option<Uuid>,
}
```

- `settings_json(row)`: all 11 flags + `"id"`, `"workspace"` (slug — resolve via `ws_id` reverse or accept slug param into the JSON).
- `ensure_settings(pool, ws)`: runs the Task-1 union upsert **scoped to one workspace** (`WHERE w.id = $1`). Reuse the exact SELECT body from the migration with the workspace filter; this one function backs GET auto-create AND later domain-phase recomputes (S6/S7/S8 call it at cutover).
- `get_settings`: gate = any active membership (`ws_role(...).is_none() → deny()`); workspace missing → `missing()`; `ensure_settings` then fetch + 200.
- `update_settings`: `ws_role` none → deny; `role < 20` → deny; validate `default_assignee_id` (NULL ok, else must be an active `workspace_members` row → else 400 `{"error": "default_assignee must be an active workspace member"}`), `archive_in`/`close_in` `>= 0` else 400; `ensure_settings` then COALESCE UPDATE (mirror document_export UPDATE shape); return fresh row, 200.
- Register in `main.rs` directly after the document-export-settings route:

```rust
// Workspace feature settings (program P1/S2: single config set).
.route(
    "/api/workspaces/:slug/settings/",
    get(routes::workspace_settings::get_settings).patch(routes::workspace_settings::update_settings),
),
```

- [ ] **Step 1: Write the module** (read `document_export.rs:1-100` and `:200-240` first for exact import/gate/error idioms; keep the same `R` alias, `deny()`/`missing()` usage, and `StatusCode::FORBIDDEN`/`BAD_REQUEST` shapes)
- [ ] **Step 2: Register module + route**
- [ ] **Step 3: Write the test file** (`workspace_settings_test.rs`, serial: slug prefix `wsf-`, purge mirrors `document_export_settings_test.rs:86-110` plus project cleanup)

Scratch: workspace + admin (20) + member (15) + guest (5) rows (mirror `document_export_settings_test.rs:60-110` insert_user + workspace_members SQL; add the guest row). Helpers: `insert_project(pool, ws, name, identifier, flags...)` inserting ALL NOT NULL-without-default columns (`archive_in, close_in, created_at, cycle_view, description, guest_view_all_features, id, identifier, intake_view, is_issue_type_enabled, is_time_tracking_enabled, issue_views_view, logo_props, module_view, name, network, page_view, timezone, updated_at, workspace_id`).

Cases (each purges at the end, mirroring the doc-export file):

1. `backfill_union_across_projects`: project A (cycle_view=t, module_view=t, intake_view=t, archive_in=3, default_assignee=member) + project B (all false, close_in=6, guest_view_all_features=t) → call `ensure_settings` (or PATCH-less GET) → assert row: all three views true, intake true, guest true, archive_in=3, close_in=6, assignee=member.
2. `get_creates_row_when_missing`: delete row → GET as member → 200 + row exists with union values.
3. `get_forbidden_for_non_member`: outsider user → 403. (Need a second user with no membership; mirror how other suites build outsiders — check `intake_workspace_test.rs` for an outsider pattern.)
4. `patch_admin_only`: member → 403; guest → 403; admin → 200 and persisted (GET shows new values; untouched flags preserved).
5. `patch_validates`: negative `archive_in` → 400; unknown `default_assignee_id` (random UUID) → 400; assignee = member → 200.
6. `patch_partial_preserves_rest`: flip only `cycle_view=false` → other flags unchanged.

- [ ] **Step 4: Compile-check only** (the DB does not have migration 0021 yet — the full run happens in Task 6 after the backend rebuild applies it at boot)

Run: `cargo test -p api --test workspace_settings_test --no-run`
Expected: compiles with no errors.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workspace_settings.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/workspace_settings_test.rs
git commit -m "feat(api): workspace feature settings endpoints with union backfill"
```

---

### Task 3: AI crate reads workspace flags

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs` (~lines 300-320, `ProjectFeature` mapping), `apps/api-rs/crates/ai/src/tools/sprints.rs` (lines ~60-70, ~205-215, test ~534), `apps/api-rs/crates/ai/src/tools/kb.rs` (lines ~30-45, ~137-145, ~240-250, ~300-310, test ~376), plus any other hits from the inventory grep below.

Rule: **visibility scoping via `project_members` stays**; only the feature-flag predicate moves from `projects.<flag>` to `workspace_settings`.

- [ ] **Step 1: Inventory**

Run: `rg -n "p\.(cycle_view|module_view|intake_view|page_view|issue_views_view)|_view = true|ProjectFeature::" apps/api-rs/crates/ai/src/`
Expected hits: `mod.rs` enum + `sprints.rs` (cycle_view ×2 + assert) + `kb.rs` (page_view ×4 + assert). If `tracks`/`intake` tools have flag predicates, include them with the same rewrite.

- [ ] **Step 2: Rewrite each workspace-scoped query** — replace the `projects` flag join/predicate with:

```sql
JOIN workspace_settings ws_flags ON ws_flags.workspace_id = <workspace_id_expr> AND ws_flags.deleted_at IS NULL
-- and the predicate:
AND ws_flags.cycle_view = true
```

(Use the query's existing workspace binding for `<workspace_id_expr>`; if the query resolves the workspace id from slug inline, bind it the same way. Keep every `project_members` membership predicate untouched.)

- [ ] **Step 3: Update the SQL-contains asserts** (`sprints.rs` test asserting `SPRINTS_SQL.contains("p.cycle_view = true")`, `kb.rs` asserting `ARTICLE_SEARCH_SQL.contains("p.page_view = true")`) to the new predicate strings.
- [ ] **Step 4: Run AI tests**

Run: `cargo test -p ai`
Expected: green (including the updated SQL-contains asserts).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai
git commit -m "feat(ai): read feature flags from workspace settings"
```

---

### Task 4: Web — Features settings page

**Files:**

- Create: `packages/types/src/workspace-settings/index.ts` (mirror `packages/types/src/document-export/index.ts`; export `TWorkspaceSettings`, `TWorkspaceSettingsPayload`), register in the types barrel if that package uses one (check how document-export is exported).
- Create: `apps/web/core/services/workspace/workspace-settings.service.ts` (mirror `services/document-export/document-export-settings.service.ts`: `fetchSettings` GET + `updateSettings` PATCH on `/api/workspaces/${workspaceSlug}/settings/`).
- Modify: `packages/constants/src/fetch-keys.ts` (add `WORKSPACE_SETTINGS = (workspaceSlug) => ...` after `DOCUMENT_EXPORT_SETTINGS`, mirroring its shape).
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/features/page.tsx` + `header.tsx` (mirror the `document-export/` pair; title from `t("workspace_settings.settings.features.title")`).
- Create: `apps/web/core/components/settings/workspace-features/form.tsx` (mirror `components/settings/document-export/form.tsx`: `useSWR(WORKSPACE_SETTINGS(slug), fetch)`, admin-only editing — the tab itself is ADMIN-gated so no extra role check needed inside).
- Modify: `apps/web/app/routes/core.ts` (add `:workspaceSlug/settings/features` route after document-export, same shape).
- Modify: `packages/constants/src/settings/workspace.ts` (add `features` tab: `key`, `i18n_label: "workspace_settings.settings.features.title"`, `href: /settings/features`, `access: [ADMIN]`, highlight `pathname === base + "/settings/features/"`; add to `GROUPED_WORKSPACE_SETTINGS[FEATURES]`).
- Modify: `packages/i18n/src/locales/en/workspace-settings.json` (new `features` namespace block, see keys below) + the same keys in the other 19 locales following `.claude/skills/translate/SKILL.md` (DNT glossary, CLDR plurals, placeholders; mark `machine_translated` in the commit message).

i18n keys (EN source of truth):

```json
"features": {
  "title": "Features",
  "description": "Turn workspace capabilities on or off. These apply to every application in this workspace.",
  "sprints_label": "Sprints",
  "sprints_description": "Timeboxed iterations across all applications.",
  "tracks_label": "Tracks",
  "tracks_description": "Grouped initiatives across all applications.",
  "views_label": "Views",
  "views_description": "Saved ticket filters.",
  "pages_label": "Knowledge base",
  "pages_description": "Workspace pages and documentation.",
  "intake_label": "Intake",
  "intake_description": "Front door for incoming requests.",
  "time_tracking_label": "Time tracking",
  "time_tracking_description": "Log time spent on tickets.",
  "guests_label": "Guests see all tickets",
  "guests_description": "When off, guests only see tickets they created.",
  "default_assignee_label": "Default assignee",
  "default_assignee_description": "Pre-selected when creating a ticket.",
  "archive_label": "Auto-archive after (months)",
  "archive_description": "0 disables auto-archiving.",
  "close_label": "Auto-close after (months)",
  "close_description": "0 disables auto-closing."
}
```

(Form shows toggles for the 7 booleans + assignee dropdown (workspace members + None) + two number inputs. `is_issue_type_enabled` has no UI here — work item types are managed in their own tab, which stays the enablement surface until S3.)

- [ ] **Step 1: Types + service + fetch key**
- [ ] **Step 2: Page + header + form** (read the three document-export counterparts first; keep the same wrapper/header/data pattern)
- [ ] **Step 3: Route + constants nav (tab ADMIN-only, FEATURES group)**
- [ ] **Step 4: i18n EN + 19 locales** (translate skill; run `pnpm --filter=@plane/i18n run generate:types`; `sync:check` must show no NEW drift vs the S1 baseline in `/tmp/opencode/sync-baseline-s1.log`)
- [ ] **Step 5: Typecheck + commit**

```bash
git add packages/types/src/workspace-settings apps/web/core/services/workspace/workspace-settings.service.ts packages/constants/src/fetch-keys.ts "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/features" apps/web/app/routes/core.ts packages/constants/src/settings/workspace.ts packages/i18n/src/locales
git commit -m "feat(web): workspace features settings page" -m "New EN keys machine_translated into 19 locales; native review pending (S1b tracks the S1 rename backlog)."
```

---

### Task 5: Sidebar gating on workspace flags

**Files:**

- Modify: `packages/constants/src/workspace.ts` (`IWorkspaceSidebarNavigationItem` + the four static items), `apps/web/core/components/workspace/sidebar/sidebar-menu-items.tsx` (filter).

- [ ] **Step 1: Add optional flag to nav items**

In `packages/constants/src/workspace.ts`, extend the item type with an optional field and set it on four items:

```ts
export interface IWorkspaceSidebarNavigationItem {
  key: string;
  labelTranslationKey: string;
  href: string;
  access: EUserWorkspaceRoles[];
  highlight: (pathname: string, url: string) => boolean;
  flag?: "cycle_view" | "module_view" | "page_view" | "intake_view";
}
```

`sprints → flag: "cycle_view"`, `tracks → flag: "module_view"`, `kb → flag: "page_view"`, `intake → flag: "intake_view"`. All other items: no `flag` (always visible subject to role).

- [ ] **Step 2: Filter by flag in the sidebar**

In `sidebar-menu-items.tsx`, fetch settings once and extend the `visibleStaticGroups` filter (mirror the existing `allowPermissions` filter style):

```tsx
import useSWR from "swr";
import { WORKSPACE_SETTINGS } from "@plane/constants";
import { WorkspaceSettingsService } from "@/services/workspace/workspace-settings.service";
// inside the component, next to the other hooks:
const workspaceSettingsService = useMemo(() => new WorkspaceSettingsService(), []);
const { data: workspaceSettings } = useSWR(
  workspaceSlug ? WORKSPACE_SETTINGS(workspaceSlug.toString()) : null,
  workspaceSlug ? () => workspaceSettingsService.fetchSettings(workspaceSlug.toString()) : null
);
// in visibleStaticGroups mapping, after the role filter:
.filter(
  (item) =>
    allowPermissions(item.access, EUserPermissionsLevel.WORKSPACE, slug) &&
    (item.flag === undefined || workspaceSettings?.[item.flag] !== false)
);
```

(Fail-open: missing row/settings → item stays visible. SWR dedupes with the settings page request.)

- [ ] **Step 3: Types + vitest for the mapping**

Add/extend a web unit test asserting the flag→item mapping contract (e.g. a test that every `flag` value is a key of `TWorkspaceSettings` and that `sprints/tracks/kb/intake` each carry the expected flag). Put it next to existing sidebar tests if present (`rg -l "sidebar" apps/web --glob '*.test.*'`), else create `apps/web/core/components/workspace/sidebar/sidebar-flag-gating.test.ts`. Run the suite for the file.

- [ ] **Step 4: Typecheck + commit**

```bash
git add packages/constants/src/workspace.ts apps/web/core/components/workspace/sidebar/sidebar-menu-items.tsx <test-file>
git commit -m "feat(web): gate workspace sidebar items on workspace feature flags"
```

---

### Task 6: Verification, deploy, smoke, push

- [ ] **Step 1: Rebuild backend (detached — LTO link takes 10+ min with no output, do NOT abort; this also applies migration 0021 at boot)**

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-s2-build.log 2>&1 < /dev/null &
```

Poll the log; when done: `curl http://localhost:8000/health` → 200 (migration applied), then `systemctl --user restart plane-live.service && curl http://localhost:3100/live/health/` (per AGENTS.md).

- [ ] **Step 2: Rust tests (serial)**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workspace_settings_test -- --test-threads=1
```

Expected: all 6 pass.

- [ ] **Step 3: Web checks**

```bash
pnpm --filter=web check:types
pnpm --filter=web test
pnpm --filter=web check:lint
```

Expected: types exit 0; vitest green; lint 0 errors (warnings may drift slightly from the 660 baseline — errors must stay 0).

- [ ] **Step 4: Web build + prod restart + smoke**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service && sleep 12 && curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000
```

Expected: `200`. Smoke in browser: settings → Features tab visible to admin only (member sees NotAuthorized); toggles persist after reload; sidebar Sprints/Tracks/KB/Intake hide when their flag is off and reappear when on; guest still sees allowed items.

- [ ] **Step 5: Push**

```bash
git push origin preview
```

---

## S2 exit checklist (must all hold)

- [ ] Migration 0021 applied via boot (no manual ledger edits); rerun-safe.
- [ ] `GET` any member (incl. guest) 200 with union values; missing row auto-created.
- [ ] `PATCH` admin-only; validation 400s; unknown keys ignored.
- [ ] AI workspace queries read `workspace_settings`; old SQL asserts updated.
- [ ] Features tab ADMIN-only; 20-locale i18n synced (`sync:check` delta = baseline).
- [ ] Sidebar hides Sprints/Tracks/KB/Intake when flags off; fail-open when row missing.
- [ ] Project flag columns + readers untouched (S6/S7/S8 own them).
