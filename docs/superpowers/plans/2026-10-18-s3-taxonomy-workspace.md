# S3 — Taxonomy: Labels + Estimates + Work Item Types (Workspace-Level)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Labels, estimates, and work item types become single workspace-level sets: union/dedupe backfill, project-scoped endpoints re-scoped to the workspace set (URLs unchanged), validators read workspace scope, web pickers keep working via backend re-scope + tiny store shims.

**Architecture:** Backend re-scope (strangler) — keep the `/projects/:id/...` URL shapes, rewrite their SQL to the workspace set. Web changes are minimal shims (label store fallback, estimate active-pointer, drop 6 `project_ids.includes` type filters). `project_issue_types` stops being READ (writes continue via legacy import/unlink until S8; table dropped in S9).

**Tech Stack:** Rust (sqlx, axum), PostgreSQL, React/MobX, vitest, cargo tests.

**Program context:** Sub-project S3 of `docs/superpowers/specs/2026-10-18-project-to-application-design.md` (Wave 1②③). Follows S2 (workspace_settings exists; `estimate_id` pointer lands here).

**Out of scope (later phases):** switching app-nav/settings readers (S6/S8), intake auto-create on project flags (S7), dropping join tables/columns (S9). Integrations/importers untouched.

**Key semantics (locked):**

- Labels: dedupe by `(workspace_id, lower(trim(name)))`, canonical = oldest; remap `issue_labels.label_id`, `labels.parent_id`, and any label-id references inside `issue_views.filters` JSON; soft-delete dupes; NULL surviving `project_id`; new workspace unique index.
- Estimates: dedupe by `(workspace_id, lower(trim(name)), type)`; points union by value; canonical = most issue references else oldest; exactly one `last_used=true` per workspace; `workspace_settings.estimate_id` = canonical (NULL when the workspace has no estimates); remap `issues.estimate_point_id`.
- Types: union enablement = every non-deleted workspace issue type is enabled everywhere. Validators (draft create/convert, intake triage gate, intake sources list) check `issue_types` workspace-wide instead of the join. `list_project` (v1) returns all workspace types. Legacy import/unlink endpoints keep writing the join (harmless; nothing reads it after S3).
- Saved-filter references: dev DB has none (`issue_views.filters` all `{}`), but the migration must still remap any label-id strings found (inspection step included; general path).
- No dual-write. Interim divergence (project PATCH edits between S3 and S6/S7/S8) is absorbed by re-running the union at each domain cutover (same pattern as S2).

---

### Task 1: Migration `0022_taxonomy_workspace.sql`

**Files:**

- Create: `apps/api-rs/migrations/0022_taxonomy_workspace.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Program P1/S3: labels + estimates become single workspace-level sets.
-- Additive: new unique indexes + idempotent union backfill. Project-scoped
-- columns stay (nulled on survivors) until the S9 cleanup.

-- Labels: workspace-level uniqueness.
CREATE UNIQUE INDEX IF NOT EXISTS labels_ws_name_unique
    ON public.labels (workspace_id, lower(btrim(name)))
    WHERE deleted_at IS NULL;

-- (1) Remap issue_labels to the canonical label (oldest) per (workspace, name).
UPDATE public.issue_labels il SET label_id = m.canonical_id, updated_at = now()
FROM (
    SELECT id AS old_id,
           FIRST_VALUE(id) OVER w AS canonical_id
    FROM public.labels WHERE deleted_at IS NULL
    WINDOW w AS (PARTITION BY workspace_id, lower(btrim(name)) ORDER BY created_at, id)
) m
WHERE il.label_id = m.old_id AND il.label_id <> m.canonical_id AND il.deleted_at IS NULL;

-- (2) Remap hierarchical parents that point at merged-away labels.
UPDATE public.labels ch SET parent_id = m.canonical_id, updated_at = now()
FROM (
    SELECT id AS old_id,
           FIRST_VALUE(id) OVER w AS canonical_id
    FROM public.labels WHERE deleted_at IS NULL
    WINDOW w AS (PARTITION BY workspace_id, lower(btrim(name)) ORDER BY created_at, id)
) m
WHERE ch.parent_id = m.old_id AND ch.parent_id <> m.canonical_id AND ch.deleted_at IS NULL;

-- (3) Soft-delete non-canonical duplicates.
UPDATE public.labels l SET deleted_at = now(), updated_at = now()
WHERE l.deleted_at IS NULL AND l.id NOT IN (
    SELECT DISTINCT FIRST_VALUE(id) OVER w
    FROM public.labels WHERE deleted_at IS NULL
    WINDOW w AS (PARTITION BY workspace_id, lower(btrim(name)) ORDER BY created_at, id)
);

-- (4) Survivors become workspace-level.
UPDATE public.labels SET project_id = NULL, updated_at = now()
WHERE deleted_at IS NULL AND project_id IS NOT NULL;

-- Estimates: nullable project link + workspace uniques.
ALTER TABLE public.estimates ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE public.estimate_points ALTER COLUMN project_id DROP NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS estimates_ws_name_unique
    ON public.estimates (workspace_id, lower(btrim(name)), type)
    WHERE deleted_at IS NULL;
ALTER TABLE public.workspace_settings ADD COLUMN IF NOT EXISTS estimate_id uuid;

-- (5) Merge duplicate estimates per (workspace, name, type).
-- Canonical = most issue references, else oldest. Points union by value:
-- issues pointing at moved points follow to the canonical same-value point,
-- otherwise the point row is re-homed to the canonical estimate.
WITH ranked AS (
    SELECT e.id, e.workspace_id, lower(btrim(e.name)) AS norm_name, e.type,
           COUNT(i.id) AS use_count, e.created_at,
           ROW_NUMBER() OVER (
               PARTITION BY e.workspace_id, lower(btrim(e.name)), e.type
               ORDER BY COUNT(i.id) DESC, MIN(e.created_at), MIN(e.id::text)
           ) AS rn
    FROM public.estimates e
    LEFT JOIN public.estimate_points ep ON ep.estimate_id = e.id AND ep.deleted_at IS NULL
    LEFT JOIN public.issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL
    WHERE e.deleted_at IS NULL
    GROUP BY e.id, e.workspace_id, e.name, e.type, e.created_at
),
canon AS (SELECT id AS canonical_id, workspace_id, norm_name, type FROM ranked WHERE rn = 1),
dupes AS (SELECT id AS dupe_id, workspace_id, norm_name, type FROM ranked WHERE rn > 1)
-- 5a: move dupe points without a same-value canonical sibling to the canonical estimate
UPDATE public.estimate_points ep SET estimate_id = c.canonical_id, updated_at = now()
FROM dupes d JOIN canon c
  ON c.workspace_id = d.workspace_id AND c.norm_name = d.norm_name AND c.type = d.type
WHERE ep.estimate_id = d.dupe_id AND ep.deleted_at IS NULL
  AND NOT EXISTS (
      SELECT 1 FROM public.estimate_points cp
      WHERE cp.estimate_id = c.canonical_id AND cp.deleted_at IS NULL AND cp.value = ep.value
  );

-- 5b: remap issues off points that are about to die (same-value canonical point wins)
UPDATE public.issues i SET estimate_point_id = cp.id, updated_at = now()
FROM public.estimate_points ep
JOIN dupes d ON d.dupe_id = ep.estimate_id
JOIN canon c ON c.workspace_id = d.workspace_id AND c.norm_name = d.norm_name AND c.type = d.type
JOIN public.estimate_points cp
  ON cp.estimate_id = c.canonical_id AND cp.deleted_at IS NULL AND cp.value = ep.value
WHERE i.estimate_point_id = ep.id AND i.deleted_at IS NULL AND ep.deleted_at IS NULL;

-- 5c: soft-delete orphaned dupe points, then dupe estimates
UPDATE public.estimate_points ep SET deleted_at = now(), updated_at = now()
WHERE ep.deleted_at IS NULL AND ep.estimate_id IN (SELECT dupe_id FROM dupes)
  AND NOT EXISTS (SELECT 1 FROM public.issues i WHERE i.estimate_point_id = ep.id AND i.deleted_at IS NULL);
UPDATE public.estimates e SET deleted_at = now(), updated_at = now()
WHERE e.id IN (SELECT dupe_id FROM dupes) AND e.deleted_at IS NULL;

-- (6) Exactly one last_used per workspace + active pointer on workspace_settings.
UPDATE public.estimates e SET last_used = false, updated_at = now()
WHERE e.deleted_at IS NULL AND e.last_used IS DISTINCT FROM false;
WITH canon AS (
    SELECT e.id,
           ROW_NUMBER() OVER (
               PARTITION BY e.workspace_id
               ORDER BY (SELECT COUNT(*) FROM public.estimate_points ep
                         JOIN public.issues i ON i.estimate_point_id = ep.id AND i.deleted_at IS NULL
                         WHERE ep.estimate_id = e.id AND ep.deleted_at IS NULL) DESC,
                        e.created_at, e.id::text
           ) AS rn
    FROM public.estimates e WHERE e.deleted_at IS NULL
)
UPDATE public.estimates e SET last_used = true, updated_at = now()
FROM canon c WHERE c.id = e.id AND c.rn = 1;
UPDATE public.workspace_settings ws SET estimate_id = sub.canonical_id, updated_at = now()
FROM (
    SELECT DISTINCT ON (e.workspace_id) e.workspace_id, e.id AS canonical_id
    FROM public.estimates e WHERE e.deleted_at IS NULL AND e.last_used = true
    ORDER BY e.workspace_id
) sub WHERE sub.workspace_id = ws.workspace_id;
UPDATE public.workspace_settings SET estimate_id = NULL, updated_at = now()
WHERE estimate_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM public.estimates e
    WHERE e.id = workspace_settings.estimate_id AND e.deleted_at IS NULL
);

-- (7) Survivors become workspace-level.
UPDATE public.estimates SET project_id = NULL, updated_at = now()
WHERE deleted_at IS NULL AND project_id IS NOT NULL;
UPDATE public.estimate_points SET project_id = NULL, updated_at = now()
WHERE deleted_at IS NULL AND project_id IS NOT NULL;
```

Notes for the executor (do not skip):

- The migration MUST stay fully rerunnable (every statement is `IF NOT EXISTS` / `ON CONFLICT`-free idempotent UPDATEs guarded by current state). Verify by running it twice in the rollback transaction in Step 2 and diffing row counts.
- Saved-filter references: before writing, run `SELECT filters FROM issue_views WHERE deleted_at IS NULL AND filters::text <> '{}' LIMIT 5` on dev DB. If any filter JSON embeds label ids/names, add a remap UPDATE for `issue_views.filters` (string-replace old→canonical id only for merged-away ids; report counts). Dev DB currently has none (`{}` everywhere) — the general-path step stays in the plan regardless.

- [ ] **Step 2: Validate SQL safely on the dev DB (transaction, rolled back, run TWICE to prove idempotence)**

Run:

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane -v ON_ERROR_STOP=1 -c "BEGIN;" -f /dev/stdin -c "SELECT 'labels', count(*) FROM labels WHERE deleted_at IS NULL UNION ALL SELECT 'estimates', count(*) FROM estimates WHERE deleted_at IS NULL UNION ALL SELECT 'settings_with_estimate', count(*) FROM workspace_settings WHERE estimate_id IS NOT NULL;" -f /dev/stdin -c "SELECT 'labels', count(*) FROM labels WHERE deleted_at IS NULL UNION ALL SELECT 'estimates', count(*) FROM estimates WHERE deleted_at IS NULL;" -c "ROLLBACK;" < apps/api-rs/migrations/0022_taxonomy_workspace.sql < apps/api-rs/migrations/0022_taxonomy_workspace.sql
```

Expected: no error; second-run counts identical to first-run counts; ROLLBACK leaves the DB unchanged. (Never apply manually — `sqlx::migrate!` is compile-time embedded; the real apply happens at API boot after rebuild in Task 6. Manual apply would desync the sqlx ledger, cf. S2 lesson with migration 0020.)

- [ ] **Step 3: Commit the migration file alone**

```bash
git add apps/api-rs/migrations/0022_taxonomy_workspace.sql
git commit -m "feat(api): union labels and estimates into workspace-level sets"
```

---

### Task 2: Backfill helper + label routes re-scope + tests

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/taxonomy_backfill.rs` (`pub(crate) async fn backfill_labels(pool, ws)` + `backfill_estimates(pool, ws)` — per-workspace recompute SQL, parameterized versions of the migration 0022 bodies; used by tests now and by S6/S7/S8 cutover recomputes later).
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs` (register `pub mod taxonomy_backfill;`), `apps/api-rs/crates/api/src/routes/label.rs`.
- Create: `apps/api-rs/crates/api/tests/taxonomy_workspace_test.rs` (labels section here; estimates section in Task 3, types section in Task 4).

Backfill helpers mirror the migration SQL scoped to one workspace (`WHERE w.id = $1`). The migration file keeps its own self-contained full-DB copy (migrations must never import crate code). Duplication is intentional and documented in both files' headers. — `list`, `create`, `detail`, `patch`, `destroy`, `bulk_create_labels`, `issue_labels_create` (re-scope `project_id = $X` predicates to `workspace_id = $Y`; inserts set `project_id = NULL`; uniqueness guards check the workspace set), `ws_labels` (drop the `JOIN projects` + `project_members EXISTS`; keep the `guard_ws_labels` gate).

- Create: `apps/api-rs/crates/api/tests/taxonomy_workspace_test.rs` (labels part here; estimates + types parts added in Tasks 3–4 — one file, three sections).

Read each function first (`label.rs:55-215` for list/create shape already surveyed; read `detail` ~215, `patch` ~258, `destroy` ~320, `issue_labels_create` ~571, `bulk_create_labels` ~704 before editing).

Re-scope rules per function:

- `list`: `WHERE project_id = $1` → resolve `workspace_id` from `:slug` (the handler already takes slug — check; if it only takes project_id, add slug resolution via a projects lookup like other handlers do) and return the workspace set ordered as today.
- `create`: external-id guard + name guard check the workspace set; INSERT with `project_id = NULL`; sort_order `MAX()+10000` scoped to workspace; keep 409 shapes/messages but say "in the workspace" (update the two error strings: `"Label with the same name already exists in the workspace"` — the id variant message stays).
- `detail`/`patch`/`destroy`: look up by `(id, workspace)` instead of `(id, project)`; patch rename-guard checks the workspace set (keep `guard_patch` but pass workspace-scoped existence; update its message to workspace wording).
- `bulk_create_labels`: same re-scope per item (workspace set, NULL project_id).
- `issue_labels_create`: validate the label belongs to the workspace (not the project).
- `ws_labels`: remove the projects join + membership EXISTS (gate already enforces ws membership). New SQL: `SELECT ... FROM labels l WHERE l.workspace_id = (SELECT id FROM workspaces WHERE slug=$1) AND l.deleted_at IS NULL ORDER BY l.created_at DESC`.

Error-string updates are part of the rename: "in the project" → "in the workspace" for these label messages.

- [ ] **Step 1: Write `taxonomy_backfill.rs` + register module** (both helpers; compile via next step)
- [ ] **Step 2: Rewrite the seven label functions** (keep gates, status codes, and response shapes byte-identical except the documented message wording)
- [ ] **Step 3: Write the label section of `taxonomy_workspace_test.rs`** (call `backfill_labels(pool, ws)` — no inline SQL duplication)

Scratch: mirror `workspace_settings_test.rs` (admin/member/guest, slug prefix `wst-`, purge adds `issue_labels`, `labels` rows). Helpers: `insert_project` (same shape as S2's), `insert_label(pool, ws, project_id_or_null, name, color, parent_id?)`, `insert_issue_with_label` (minimal issue row — mirror the issue fixture from `label_test.rs`/`detail_label_test.rs`).

1. `labels_merge_dedupes_across_projects`: project A labels [Bug(red), Task], project B labels [bug(blue), Feature] (+ an issue in B tagged B's `bug`, + a child label under B's `bug`) → call `backfill_labels(pool, ws)` → assert one `bug` row (canonical = A's, red), `issue_labels` remapped to canonical, B's `bug` soft-deleted, child re-parented to canonical.
2. `project_label_endpoints_serve_workspace_set`: GET project-list returns workspace labels (incl. other project's); POST create in project B with existing name → 409 workspace message; POST new name → 201 with `project_id` NULL in DB.
3. `ws_labels_lists_without_membership_join`: guest with NO project membership gets workspace labels (200 non-empty) — proves the join removal.

- [ ] **Step 4: Compile-check only** (`cargo test -p api --test taxonomy_workspace_test --no-run`; full run in Task 6 after rebuild)

Existing suites (`label_test`, `detail_label_test`, `intake_triage_test`, `intake_test`) run in Task 6 Step 2 along with the new suite — they use single-project scratch workspaces so the union is identity (no behavior change expected).

- [ ] **Step 5: Commit backend re-scope + tests**

```bash
git add apps/api-rs/crates/api/src/routes/taxonomy_backfill.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/routes/label.rs apps/api-rs/crates/api/tests/taxonomy_workspace_test.rs
git commit -m "feat(api): serve workspace label set from project label endpoints"
```

---

### Task 3: Estimate routes re-scope

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/estimate.rs` — `list`, `create`, `detail`, `patch`, `destroy`, `create_point`, `patch_point`, `destroy_point`, `project_estimates` (re-scope to workspace; inserts NULL `project_id`; active-estimate semantics: exactly one `last_used=true` per workspace — `create` with `last_used=true` clears others; `destroy` of the active estimate promotes the oldest remaining or clears `workspace_settings.estimate_id`), `ws_estimates` in `prefs.rs:1574` (drop the `projects.estimate_id` filter → all workspace estimates with points).
- Modify: `apps/api-rs/crates/api/src/main.rs` ONLY if a new route is needed (none expected — re-scope keeps URLs).

Read each function first. Keep gates/status codes/shapes; update only scope predicates + messages ("in the project" → "in the workspace" where user-facing).

- [ ] **Step 1: Rewrite estimate handlers per the rules above**
- [ ] **Step 2: Add the estimates section to `taxonomy_workspace_test.rs`**

Cases (same scratch style, prefix `wst-`):

1. `estimates_merge_unions_points`: project A estimate "Points"(fib 1,2,3) + project B estimate "points"(1,2,5,8) + issue on B pointing at 5 → backfill → one "Points" row; points {1,2,3,5,8} unioned; issue repointed to canonical 5-point; `last_used` exactly one true workspace-wide; `workspace_settings.estimate_id` = canonical.
2. `project_estimate_endpoints_serve_workspace_set`: GET project-list returns workspace estimates; active estimate (last_used) resolves for any project id.
3. `create_clears_previous_active`: POST new estimate with last_used=true in project B → old active flipped false (single-active invariant holds).

- [ ] **Step 3: Compile-check only** (`--no-run`; full run in Task 6)
- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/estimate.rs apps/api-rs/crates/api/src/routes/prefs.rs apps/api-rs/crates/api/tests/taxonomy_workspace_test.rs
git commit -m "feat(api): serve workspace estimate set from project estimate endpoints"
```

---

### Task 4: Work item type validators switch to workspace scope

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/draft.rs` (~1242: type enablement check), `intake.rs` (~1556 has_types gate; ~1706 and ~2352 type-list joins), `intake_source.rs` (~190 type-list join); `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs` (`list_project` ~535: return all workspace types, ignore the join).
- Keep functional: `create_project`/`retrieve_project`/`update_project`/`delete_project`/`import_to_project`/`unlink_type` (they manage the vestigial join until S8/S9).
- Add the types section to `taxonomy_workspace_test.rs`.

Rewrites (read each site first):

- draft.rs convert check: replace the `EXISTS(project_issue_types … project_id=$2)` branch with a workspace check — resolve workspace id from the draft's project (`SELECT workspace_id FROM projects WHERE id=$2`), then `EXISTS(SELECT 1 FROM issue_types t WHERE t.id=$1 AND t.workspace_id=$W AND t.deleted_at IS NULL AND t.is_epic=false)`. Keep the epic-echo branch (`t.is_epic = true AND t.id = $3`) byte-identical.
- intake.rs has_types gate (~1556): same replacement (workspace from the intake's project).
- intake.rs type lists (~1706, ~2352) + intake_source.rs (~190): drop the `JOIN project_issue_types pit …` line; add `AND t.workspace_id = <ws> AND t.deleted_at IS NULL` (keep `is_epic` handling as-is per site).
- v1 `list_project`: return the workspace type list (same shape as `list_workspace`; keep the project-membership gate as-is).

- [ ] **Step 1: Rewrite the six sites**
- [ ] **Step 2: Add the types section to `taxonomy_workspace_test.rs`**

Cases:

1. `types_union_ignores_join`: workspace with 2 types, join rows ONLY for project A; draft-convert/intake-accept in project B with a type → succeeds (no 400). Concretely: call the intake accept-validation path? Simpler + robust: test at the SQL level? No — test through handlers is heavy for intake accept. Compromise: unit-test the rewritten EXISTS queries via a small `pub(crate)` helper? Better: test `list_project` returns both types for project B (handler-level, cheap) + one draft-convert validation case if the existing draft tests already cover the path (check: is there a draft test file? `ls` showed no draft_test — the intake tests cover triage; draft convert path may be untested upstream. Decision: handler-level test on `list_project` (union visible) + intake_source list test; the EXISTS rewrites are verified by the full intake suite green in Task 6.)
2. `list_project_returns_workspace_set`: covered above.

- [ ] **Step 3: Compile-check only**; **Step 4: commit**

```bash
git add apps/api-rs/crates/api/src/routes/draft.rs apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/src/routes/intake_source.rs apps/api-rs/crates/api/src/routes/v1/work_item_type.rs apps/api-rs/crates/api/tests/taxonomy_workspace_test.rs
git commit -m "feat(api): validate work item types at workspace scope"
```

---

### Task 5: Web shims (stores + type filters)

**Files:**

- Modify: `apps/web/core/store/label.store.ts` (`getProjectLabels` → fall back to workspace labels; add comment marking the S9 rename), `apps/web/core/store/estimates/project-estimate.store.ts` (`estimateIdsByProjectId` → workspace estimate ids; `currentActiveEstimateIdByProjectId` → `workspace_settings.estimate_id` via service read cached in store; `areEstimateEnabledByProjectId` → workspace has estimates), type-filter call sites (6 files: drop the `&& type.project_ids.includes(...)` clause).
- No new i18n keys (no copy changes). No new routes/pages.

Type-filter edits (one clause each, keep the rest of the predicate):

- `apps/web/core/components/dropdowns/work-item-type/dropdown.tsx:67`
- `apps/web/core/components/intake-sources/form-modal.tsx:47`
- `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx:115`
- `apps/web/core/components/inbox/content/inbox-issue-header.tsx:104`
- `apps/web/core/components/inbox/content/issue-properties.tsx:69`
- `apps/web/core/components/issues/issue-modal/components/default-properties.tsx:86`

(Pages KB filter at `workspace-kb-list-view.tsx:167` is page entities, not types — DO NOT touch.)

Label store shims (both computed getters fall back to the workspace set; `projectLabelsTree` derives from `projectLabels` so it follows free; `getProjectLabelIds` delegates unchanged; `fetchProjectLabels` unchanged — backend now returns the workspace set):

```ts
getProjectLabels = computedFn((projectId: string | undefined | null) => {
  const workspaceSlug = this.rootStore.router.workspaceSlug || "";
  // Program P1/S3: labels are workspace-level; the project argument is kept
  // for call-site compatibility until the S9 rename.
  if (!projectId || !(this.fetchedMap[projectId] || this.fetchedMap[workspaceSlug])) return;
  return this.getWorkspaceLabels(workspaceSlug);
});
```

Plus the `projectLabels` getter (same file, ~line 99) — delegate to the shim above:

```ts
get projectLabels() {
  // Program P1/S3: labels are workspace-level; project scoping removed.
  return this.getProjectLabels(this.rootStore.router.projectId);
}
```

(`getProjectLabelIds` delegates to `getProjectLabels` — unchanged. `fetchProjectLabels` unchanged — backend now returns the workspace set.)

Estimate store shims:

```ts
estimateIdsByProjectId = computedFn((projectId: string | undefined) => {
  if (!projectId) return undefined;
  const workspaceSlug = this.store.router.workspaceSlug || "";
  const workspaceDetails = this.store.workspaceRoot.getWorkspaceBySlug(workspaceSlug);
  if (!workspaceDetails) return undefined;
  return Object.values(this.estimates || {})
    .filter((p) => p.workspace_id === workspaceDetails.id)
    .map((p) => p.id) as string[];
});
```

Check `IEstimate` has `workspace_id` (verify in `packages/types/src/estimate.ts`; if missing, add it — the API already returns `workspace_id` in estimate rows per `ws_estimates` SELECT).

`currentActiveEstimateIdByProjectId`: needs the workspace active pointer. Source of truth = `workspace_settings.estimate_id`. The estimate store has no settings fetch — add a lightweight one: reuse `WorkspaceSettingsService.fetchSettings` inside a new store action `fetchWorkspaceActiveEstimateId(workspaceSlug)` caching into an observable `workspaceActiveEstimateId`, called from the same places that call `getProjectEstimates`... Simpler: since backfill guarantees exactly one `last_used=true` per workspace AND the store already tracks `last_used`, implement as: find `last_used` estimate within the workspace:

```ts
currentActiveEstimateIdByProjectId = computedFn((projectId: string | undefined): string | undefined => {
  if (!projectId) return undefined;
  const workspaceSlug = this.store.router.workspaceSlug || "";
  const workspaceDetails = this.store.workspaceRoot.getWorkspaceBySlug(workspaceSlug);
  if (!workspaceDetails) return undefined;
  return Object.values(this.estimates || {}).find((p) => p.workspace_id === workspaceDetails.id && p.last_used)?.id;
});
```

(`areEstimateEnabledByProjectId`: keep reading `projectDetails.estimate`?? — that field comes from `projects.estimate_id`, now stale. Change to: workspace has ≥1 estimate → `estimateIdsByProjectId(projectId)?.length > 0`. Verify call sites tolerate it — they already handle empty lists.)

- [ ] **Step 1: Label store shim** (verify `IEstimate`/`IIssueLabel` carry `workspace_id`; add to types if missing — API returns it)
- [ ] **Step 2: Estimate store shims** (three computedFns above)
- [ ] **Step 3: Drop the 6 `project_ids.includes` type-filter clauses**
- [ ] **Step 4: Typecheck + vitest + commit**

```bash
git add apps/web/core/store/label.store.ts apps/web/core/store/estimates/project-estimate.store.ts <6 type-filter files> [packages/types/src/estimate.ts if touched]
git commit -m "feat(web): read taxonomy from workspace scope with project-keyed shims"
```

---

### Task 6: Verification, deploy, smoke, push

- [ ] **Step 1: Rebuild backend detached** (applies 0022 at boot; LTO link 10+ min, do NOT abort)

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-s3-build.log 2>&1 < /dev/null &
```

Poll the log; on container recreation: `curl http://localhost:8000/health` → 200, then `systemctl --user restart plane-live.service && curl http://localhost:3100/live/health/`.

Watch for the S2 lesson: if boot logs `migrate failed: migration N was previously applied but has been modified`, STOP — do not touch the ledger blindly. Verify semantic no-op-ness (compare applied vs current file, check affected rows = 0) before updating the checksum, exactly as done for v20 in S2.

- [ ] **Step 2: Rust tests (serial)**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test taxonomy_workspace_test -- --test-threads=1
```

Expected: all pass. Then regression sweep on touched paths:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test label_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test detail_label_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test estimate_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test v1_work_item_type_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_test -- --test-threads=1
```

All green required. (S2 lesson: fix stale fixtures like the `last_logout_uagent` column instead of working around them.)

- [ ] **Step 3: Web checks**

```bash
pnpm --filter=web check:types
pnpm --filter=web test
pnpm --filter=web check:lint
```

Expected: types exit 0; vitest green; lint 0 errors.

- [ ] **Step 4: Web build + prod restart + smoke**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service && sleep 12 && curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000
```

Expected: `200`. Smoke: issue create modal label/type/estimate dropdowns show workspace sets; intake accept with a type works without join rows; project settings labels/estimates/types pages read/write (they now edit workspace-wide sets).

- [ ] **Step 5: Push**

```bash
git push origin preview
```

---

## S3 exit checklist (must all hold)

- [ ] Migration 0022 applied via boot (no manual ledger edits); rerun-safe.
- [ ] One label row per (workspace, name); issue_labels + parents remapped; survivors `project_id` NULL.
- [ ] Points unioned by value; exactly one `last_used` per workspace; `workspace_settings.estimate_id` set; issue points remapped.
- [ ] Draft/intake/source validators ignore the join; `list_project` returns the workspace set.
- [ ] Web pickers show workspace sets with zero call-site signature changes.
- [ ] New taxonomy suite + all touched-path suites green; web green; prod smoke 200.
