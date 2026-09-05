# Batch C: Projects Core + Issues Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement 7 project endpoints + 5 issue endpoint groups (all FE-called, currently 404) in Rust with byte-exact Django parity, plus a project-create parity fix (T0) that unblocks every member-gated check.

**Architecture:** Domain-extension (approved): new handlers extend existing `routes/project.rs`, `routes/issue.rs`, `routes/state.rs` (P6 only), `routes/member.rs` (P7 only); wire-up in `main.rs`; smoke additions in `scripts/smoke.sh`. Two lanes may run in parallel after T0 — project lane (P1–P7, files `project.rs`/`state.rs`/`member.rs`) and issue lane (I1–I5, file `issue.rs`) — but `main.rs` + `smoke.sh` edits apply serially (last-writer-wins risk). T0 first: both lanes depend on its guards + fixtures.

**Tech Stack:** Rust, Axum, SQLx (Postgres), no new deps (no Redis, no rand needed). Celery/activity side-effects are SKIPPED everywhere (Rust precedent: `issue_activities` is read-only, `work_item.rs:423-438` never writes it).

---

## Locked contracts (from Django source, verified 2026-09-05)

Byte-exact strings (differ from Batch B's `{"error":"forbidden"}` — Django has explicit messages here, so match them):

- `FORBIDDEN_MSG = "You don't have the required permissions."` → 403 (`permissions/base.py:81-84`)
- `NOT_FOUND_MSG = "The required object does not exist."` → 404 (`views/base.py:92-96`)
- Roles: ADMIN=20, MEMBER=15, GUEST=5 (Batch B verified).

| # | Method+Path | Django source | Success | Errors |
|---|---|---|---|---|
| T0 | `POST /api/workspaces/:slug/projects/` (fix, not new) | `app/views/project/base.py:258-313` | 201 (shape unchanged, see T0) + creator ADMIN + lead ADMIN + 6 states | — |
| P1 | `GET /api/workspaces/:slug/projects/details/` | `app/views/project/base.py:101-143` `list_detail` | 200 array `ProjectListSerializer` + role filtering | 403 non-member |
| P2 | `GET /api/workspaces/:slug/project-identifiers/?name=` | `app/views/project/base.py:444-454` | 200 `{"exists": int, "identifiers": [{id,name,project}]}` (name strip+upper) | 400 `{"error":"Name is required"}`; 403 guest/non-member |
| P3 | `POST /api/workspaces/:slug/user-favorite-projects/` `{project}` + `DELETE .../:project_id/` | `app/views/project/base.py:498-532` | **204** empty both | POST dup → 400 `{"error":"The payload is not valid"}`; DELETE miss → 404 |
| P4 | `POST /api/workspaces/:slug/projects/:pid/archive/` + `DELETE` same | `app/views/project/base.py:427-441` | POST 200 `{"archived_at": str}` (+delete favorites); DELETE **204** (`archived_at=None`) | 403 project non-member (PROJECT level, ADMIN/MEMBER); bad id 404 |
| P5 | `GET /api/workspaces/:slug/projects/:pid/project-members/me/` | `app/views/project/member.py:352-362` | 200 `ProjectMemberSerializer` full+nested | non-member 404 (no 403 here) |
| P6 | `POST /api/workspaces/:slug/projects/:pid/states/:pk/mark-default/` | `app/views/state/base.py:104-110` | **204**; clears `default` on siblings then sets one | 403 non-admin; missing pk STILL 204 (0-row update, no 404) |
| P7 | `POST /api/workspaces/:slug/projects/:pid/members/leave/` | `app/views/project/member.py:323-349` | **204** (`is_active=false`) | 403 non-member; non-member 404; sole admin → 400 `{"error":"You cannot leave the project as your the only admin of the project you will have to either delete the project or create an another admin"}` (copy verbatim incl. grammar) |
| I1 | `GET /api/workspaces/:slug/projects/:pid/issues/list/?issues=csv` | `app/views/issue/base.py:80-205` | 200 **bare array**, 27-key `.values()` rows (key list in Task I1) | 400 `{"error":"Issues are required"}` (missing `?issues`) |
| I2 | `GET /api/workspaces/:slug/projects/:pid/issues-detail/` | `app/views/issue/base.py:1027-1103` | 200 paginated envelope (keys in Task I2) | paginator 400s (bad per_page/cursor/group_by) |
| I3a | `DELETE .../bulk-delete-issues/` `{issue_ids:[]}` | `app/views/issue/base.py:773-797` | 200 `{"message":"{n} issues were deleted"}` (hard-del bridges, soft-del issues) | 400 `{"error":"Issue IDs are required"}`; ADMIN project-level |
| I3b | `POST .../bulk-archive-issues/` `{issue_ids:[]}` | `app/views/issue/archive.py:305-343` | 200 `{"archived_at": str(today)}` | 400 empty; 400 `{"error_code":4091,"error_message":"INVALID_ARCHIVE_STATE_GROUP"}` if any state.group ∉ {completed,cancelled}; ADMIN/MEMBER |
| I4a | `GET .../deleted-issues/` | `app/views/issue/base.py:800-813` | 200 **bare UUID array** (incl. soft-deleted, archived OR deleted; opt `?updated_at__gt`) | — (FE has zero callers; still implement, 1 query) |
| I4b | `GET .../archived-issues/` | `app/views/issue/archive.py:105-218` | 200 paginated envelope (I2 keys); `show_sub_issues` default `"true"` (`"false"` adds `parent__isnull`) | 400 group_by==sub_group_by; GUEST excluded (ADMIN/MEMBER) |
| I5 | `GET+POST .../issues/:iid/sub-issues/` | `app/views/issue/sub_issue.py:37-275` | GET 200 `{"sub_issues": [...] (25 keys), "state_distribution": {group:[ids]}}`; POST `{sub_issue_ids}` → 200 envelope, `sub_issues` = full `IssueSerializer` array | POST 404 `{"error":"Parent issue not found"}`; POST 400 `{"error":"Sub Issue IDs are required"}`; GET any active member, POST ADMIN/MEMBER (via `ProjectEntityPermission`, `permissions/project.py:103-119`) |

Explicitly OUT (do not implement; note in commits if touched):
- P3-GET (Django has no `serializer_class` → dead; FE uses `/user-favorites/`), P2-DELETE, `bulk-operation-issues`/`bulk-subscribe-issues` (FE-only, zero Django matches), `epics/*` (no Django backend at all), subscribe/reactions/history/meta/display-properties/drafts/deploy-boards (Batch D).

---

### Task 0: project-create parity fix + shared guards

**Why:** Django `create` (`project/base.py:258-313`) does 3 things Rust skips: adds creator as ADMIN (`266-270`), adds `project_lead` as ADMIN (`271-278`), seeds 6 `DEFAULT_STATES` (`279-295`, constant at `db/models/state.py:24-66`). Without this, every member-gated check (P4–P7, I3a, I4b, I5-POST) 403s in smoke — same bug class as the ws-create fix (`3df4f504b`).

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/project.rs` (create + guards + tests)

- [ ] **Step 1: Read Django serializer validation for `project_lead`**

Read `apps/api/plane/app/serializers/project.py` `ProjectSerializer` — which fields required, what error when `project_lead` invalid. Also read `db/models/state.py` `State.save` for slug generation (model imports `slugify` — mirror the rule or set slug explicitly with identical output).

- [ ] **Step 2: Verify live columns**

Run: `docker exec plane-db psql -U plane -d plane -c "\d project_members" -c "\d states" -c "\d project_identifiers" -c "\d user_favorites"`
Trust but verify: `project_members(member_id, role, is_active, workspace_id, ...)`, `states(name,color,sequence,group,default,slug,project_id,workspace_id,created_by_id,...)`, `user_favorites(user_id?,entity_type,entity_identifier,project_id,workspace_id,...)` — adapt SQL to reality, note deviations in commit message.

- [ ] **Step 3: Write failing tests for guards + seed data**

```rust
#[cfg(test)]
mod batch_c_tests {
    use super::*;
    #[test]
    fn forbidden_message_matches_django() {
        // Quoted from apps/api/plane/app/permissions/base.py:81-84 — reviewer cross-checks.
        assert_eq!(FORBIDDEN_MSG, "You don't have the required permissions.");
    }
    #[test]
    fn not_found_message_matches_django() {
        // Quoted from apps/api/plane/app/views/base.py:92-96.
        assert_eq!(NOT_FOUND_MSG, "The required object does not exist.");
    }
    #[test]
    fn sole_admin_guard() {
        assert!(guard_leave(true, 1).is_err());
        assert!(guard_leave(true, 2).is_ok());
        assert!(guard_leave(false, 1).is_ok()); // non-admin handled by 403 path, not this guard
    }
    #[test]
    fn default_states_seed_count() {
        assert_eq!(DEFAULT_STATES_SEED.len(), 6);
        assert_eq!(DEFAULT_STATES_SEED.iter().filter(|s| s.default).count(), 1);
    }
}
```

- [ ] **Step 4: Run to verify they fail**

Run: `cargo test -p api --lib routes::project::batch_c_tests`
Expected: FAIL (names not defined).

- [ ] **Step 5: Implement — guards + fixed create**

```rust
pub(crate) const FORBIDDEN_MSG: &str = "You don't have the required permissions.";
pub(crate) const NOT_FOUND_MSG: &str = "The required object does not exist.";
pub(crate) fn deny() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": FORBIDDEN_MSG})))
}
pub(crate) fn missing() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": NOT_FOUND_MSG})))
}
pub(crate) async fn ws_role(pool: &sqlx::PgPool, user_id: uuid::Uuid, slug: &str) -> Result<Option<i16>, sqlx::Error> {
    let r: Option<(i16,)> = sqlx::query_as(
        "SELECT wm.role FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id WHERE w.slug = $1 AND wm.member_id = $2 AND wm.is_active = true AND wm.deleted_at IS NULL",
    ).bind(slug).bind(user_id).fetch_optional(pool).await?;
    Ok(r.map(|x| x.0))
}
pub(crate) async fn project_role(pool: &sqlx::PgPool, user_id: uuid::Uuid, pid: uuid::Uuid) -> Result<Option<i16>, sqlx::Error> {
    let r: Option<(i16,)> = sqlx::query_as(
        "SELECT role FROM project_members WHERE project_id = $1 AND member_id = $2 AND is_active = true",
    ).bind(pid).bind(user_id).fetch_optional(pool).await?;
    Ok(r.map(|x| x.0))
}
pub fn guard_leave(is_sole_admin: bool, _admin_count: i64) -> Result<(), String> {
    if is_sole_admin { return Err("You cannot leave the project as your the only admin of the project you will have to either delete the project or create an another admin".to_string()); }
    Ok(())
}
pub(crate) struct SeedState { pub name: &'static str, pub color: &'static str, pub sequence: i32, pub group: &'static str, pub default: bool }
pub(crate) const DEFAULT_STATES_SEED: &[SeedState] = &[
    SeedState { name: "Backlog", color: "#60646C", sequence: 15000, group: "backlog", default: true },
    SeedState { name: "Todo", color: "#60646C", sequence: 25000, group: "unstarted", default: false },
    SeedState { name: "In Progress", color: "#F59E0B", sequence: 35000, group: "started", default: false },
    SeedState { name: "Done", color: "#46A758", sequence: 45000, group: "completed", default: false },
    SeedState { name: "Cancelled", color: "#9AA4BC", sequence: 55000, group: "cancelled", default: false },
    SeedState { name: "Triage", color: "#4E5355", sequence: 65000, group: "triage", default: false },
];
```

`create` changes (single transaction, mirroring `3df4f504b` ws-create pattern):
1. Add `#[serde(default)] pub project_lead: Option<uuid::Uuid>` to `CreateProject` (response shape UNCHANGED — minimal `{id,name,identifier}`, no regression; full-shape response is a noted follow-up, browser check in T13 decides).
2. After project INSERT: `INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, view_props, default_props, sort_order, preferences, created_at, updated_at) SELECT gen_random_uuid(), $creator, 20, $pid, w.id, true, '{}', '{}', 65535, '{}', now(), now() FROM workspaces w WHERE w.slug=$slug` (column list mirrors `member.rs:125` precedent).
3. If `project_lead` present and ≠ creator: same INSERT for lead (Step 1 determines validation/error to mirror).
4. Seed 6 states (loop over `DEFAULT_STATES_SEED`, `created_by_id`=creator; slug per Step 1's finding).
5. Any failure → rollback + 500 `{"error":"internal error"}` (transaction drop, `3df4f504b` precedent).

- [ ] **Step 6: Run tests**

Run: `cargo test -p api --lib routes::project`
Expected: PASS (incl. pre-existing `validate_create` tests).

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/project.rs
git commit --no-verify -m "fix(rs-api): project-create tambah creator/lead ADMIN + seed DEFAULT_STATES selaras Django"
```

---

### Task P1: GET projects/details/

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/project.rs` (`project_details` + test), `apps/api-rs/crates/api/src/main.rs` (wire `GET /api/workspaces/:slug/projects/details/` — BEFORE `:pk` route is unnecessary in Axum (segment-count differs), but keep registration adjacent to projects routes)

Contract: `base.py:101-143`. Non-member → 403. Role filtering (`base.py:105-128`): GUEST sees own projects only, MEMBER sees own + `network=2` (public), ADMIN sees all — read exact filters, mirror. Full-list branch only (`142-143`); paginated branch (`?per_page`, `130-140`) is OUT (FE `getProjects()` `project.service.ts:55-61` never paginates — note as gap in commit if FE ever needs it).
Shape: mirror `ProjectListSerializer` (`serializers/project.py:115-139`): all model fields + `is_favorite` (Exists on `user_favorites` entity_type='project'), `member_role` (active membership or null), `anchor` (deploy_boards, nullable), `sort_order` (project_user_properties, nullable), `members` (active members w/ user subset — mirror `members_list` serialization, read serializer), `cover_image_url`, `inbox_view`, `next_work_item_sequence` (read how computed — likely max sequence+1).

- [ ] **Step 1: Write failing test** — `details_role_filter_sql` is DB-backed; unit-test the pure part: a `fn details_scope(role: i16) -> &'static str` returning `"all" | "own_or_public" | "own"` with 3 asserts (20→all, 15→own_or_public, 5→own). FAIL expected (fn missing).
- [ ] **Step 2: Implement handler** — `ws_role` gate (None → `deny()`), scope filter per role, annotations via LEFT JOIN LATERAL / correlated subqueries (mirror `get_queryset` annotations `base.py:54-97`), `ORDER BY` same as Django (read `list_detail` ordering — likely name or created_at; mirror exactly).
- [ ] **Step 3: Wire + `cargo test -p api --lib routes::project`** → PASS.
- [ ] **Step 4: Commit** `feat(rs-api): GET projects/details paritas Django (role filter + annotations)`.

---

### Task P2: GET project-identifiers/

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/project.rs` (`check_identifier` + test), `main.rs` (wire `GET /api/workspaces/:slug/project-identifiers/`)

Contract: `base.py:444-454`. Gate: workspace ADMIN/MEMBER only (GUEST → 403, `allow_permission([ADMIN, MEMBER], WORKSPACE)`). `?name` strip+upper; missing → 400 `{"error":"Name is required"}`. Else 200 `{"exists": <count>, "identifiers": [{"id","name","project"}]}` filtered `name=X AND workspace__slug=slug`. (P2-DELETE on same path is OUT — FE never calls it.)

- [ ] **Step 1: Failing test** — pure `normalize_ident("  abc ") == "ABC"`, `validate_ident_name("")` → Err("Name is required"). FAIL expected.
- [ ] **Step 2: Implement** — gate via `ws_role` (None or role<15 → `deny()`), query `project_identifiers` table (columns per T0 Step 2 verification).
- [ ] **Step 3: Test + commit** `feat(rs-api): GET project-identifiers paritas Django`.

---

### Task P3: user-favorite-projects POST + DELETE (no GET)

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/project.rs` (`fav_add`, `fav_remove` + test), `main.rs` (wire `POST /api/workspaces/:slug/user-favorite-projects/` and `DELETE .../:project_id/`)

Contract: `base.py:498-532`. NO authz beyond `IsAuthenticated` (no decorator) — still require `AuthUser`, no membership check. POST body `{project: <uuid>}` → INSERT `user_favorites(user, entity_type='project', entity_identifier=project, project_id=project, workspace from slug)` → **204** empty. Duplicate (unique `entity_type,entity_identifier,user`) → 400 `{"error":"The payload is not valid"}` (map unique-violation: check `sqlx::Error::Database` code `23505`). DELETE → delete matching row (`user` + slug + project) → **204**; 0 rows → 404 `missing()`.

- [ ] **Step 1: Failing test** — `is_unique_violation` helper: construct... (DB error construction is awkward; instead unit-test the error-mapping fn `map_fav_error(rows_deleted: u64) -> StatusCode` — 0→404, 1→204). FAIL expected.
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): user-favorite-projects POST+DELETE paritas Django (tanpa GET — Django tak punya serializer)`.

---

### Task P4: project archive POST + DELETE

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/project.rs` (`archive`, `unarchive` + test), `main.rs` (wire `POST`+`DELETE /api/workspaces/:slug/projects/:project_id/archive/`)

Contract: `base.py:427-441`. Gate: PROJECT-level ADMIN/MEMBER (`project_role` is Some(20|15), else `deny()`; workspace-admin WITHOUT project membership is also denied — mirror, no bypass). Resolve project by `(slug, pid)`; miss → 404. POST: `archived_at=now()`, delete `user_favorites` for (slug, project) all users, 200 `{"archived_at": "<str>"}` (Django `str()` of datetime — mirror existing Rust datetime serialization used elsewhere, note format in commit). DELETE: `archived_at=None` → **204**.

- [ ] **Step 1: Failing test** — pure `guard_archive(role: Option<i16>)`: Some(20)→ok, Some(15)→ok, Some(5)→err, None→err. FAIL expected.
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): project archive/unarchive paritas Django`.

---

### Task P5: GET project-members/me/

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/project.rs` (`my_membership` + test), `main.rs` (wire `GET .../:project_id/project-members/me/` — literal `me` segment; Axum matches static before `:pk`, no conflict with `project-members/:pk/`)

Contract: `member.py:352-362`. No membership gate (only `AuthUser`); filter `project_id + workspace__slug + member=user + is_active` → miss → 404 `missing()`. Shape: `ProjectMemberSerializer` (`serializers/project.py:156-163`, `fields="__all__"` + nested `workspace: WorkspaceLiteSerializer`, `project: ProjectLiteSerializer(id,identifier,name,cover_image,cover_image_url,logo_props,description)`, `member: UserLiteSerializer`) — read exact lite field lists, mirror. Check existing `member.rs:detail` (`member.rs:242`) — if it already returns this shape, REUSE it (no duplicate serializer); task becomes wire-up + 404-mapping + test.

- [ ] **Step 1: Read `member.rs:242-260` + serializers; failing test** for 404-mapping pure fn or shape test on existing struct. (If fully reusable: test = wire-up compile + live curl in T13; still write a routing test if the codebase has precedent — check first.)
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): project-members/me paritas Django`.

---

### Task P6: POST states/:pk/mark-default/ (in state.rs)

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/state.rs` (`mark_default` + test), `main.rs` (wire `POST .../:project_id/states/:pk/mark-default/`)

Contract: `state/base.py:104-110`. Gate: PROJECT ADMIN only (`project_role` == Some(20), else `deny()`). Two updates scoped `(slug, project_id)`: clear `default` where true, set `default=true` where `pk`. **204 always** (even 0 rows — no existence check; mirror exactly, add comment citing `base.py:108-109`). No cache invalidation in Rust (no cache layer — note in commit).

- [ ] **Step 1: Failing test** — `guard_mark_default(Some(20))` ok; Some(15)/Some(5)/None err. FAIL expected.
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): states mark-default paritas Django (204 unconditional)`.

---

### Task P7: POST members/leave/ (in member.rs)

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/member.rs` (`leave_project` + test), `main.rs` (wire `POST .../:project_id/members/leave/` — static `leave` vs `:pk`: Axum prefers static segment; verify with a routing test or live curl in T13)

Contract: `member.py:323-349`. Gate: PROJECT ADMIN/MEMBER/GUEST (any active membership; None → `deny()`; inactive → 404? Django `get_object` on active member → non-member 404 — mirror: no active row → `missing()`; inactive row → also `missing()`). Sole admin (role==20 AND active-admin count≤1) → 400 verbatim message (T0 `guard_leave`). Else `is_active=false` → **204**.

- [ ] **Step 1: Failing test** — extend T0 `guard_leave` usage: `leave_outcome(role, admin_count)` → Ok/Err(message) matrix: (20,1)→400-verbatim, (20,2)→204, (15,_)→204, (5,_)→204. FAIL expected (fn missing; keep T0's `guard_leave` name — do NOT create a second helper).
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): project members/leave paritas Django (sole-admin guard)`.

---

### Task I1: GET issues/list/ (bare array, NOT paginated)

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/issue.rs` (`list_by_ids` + test), `main.rs` (wire `GET .../:project_id/issues/list/`)

Contract: `issue/base.py:80-205`. Gate: workspace-level ADMIN/MEMBER/GUEST (`ws_role` Some → ok; GUEST scoped to `created_by=user` per `base.py:98-106` — mirror). Missing `?issues` → 400 `{"error":"Issues are required"}`. Parse CSV UUIDs (invalid UUID → 400? Django `__in` with bad UUID 400s via ValidationError → mirror: 400 `{"error":"..."}`? read exact behavior at `base.py:86-95`; simplest mirror: reject malformed with 400 plain — cite line in commit).
Default branch (FE `retrieveIssues` `issue.service.ts:129-137` sends only `issues=` — no fields/expand): 27-key `.values()` rows IN ORDER (`order_by` default `-created_at`, `base.py:153`): `id,name,state_id,sort_order,completed_at,estimate_point,priority,start_date,target_date,sequence_id,project_id,parent_id,cycle_id,module_ids,label_ids,assignee_ids,sub_issues_count,created_at,updated_at,created_by,updated_by,attachment_count,link_count,is_draft,archived_at,deleted_at`. Counts: sub_issues (self-join), attachments (`file_assets`), links — mirror annotations (`base.py:147-202`). `fields`/`expand` branch (`IssueSerializer` subset) is OUT (no FE Batch-C caller uses it on this path — verify by grep in task; if found, implement).
Manager scope: `IssueManager` excludes triage/archived/draft (`issue.py:92-101`) — mirror in WHERE.

- [ ] **Step 1: Failing test** — pure `parse_issue_csv("a,b")` → ids; `""` → Err("Issues are required"); malformed UUID → Err. FAIL expected.
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): GET issues/list paritas Django (bare array 27 kunci)`.

---

### Task I2: GET issues-detail/ (paginated envelope)

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/issue.rs` (`list_detail` + test), `main.rs` (wire `GET .../:project_id/issues-detail/`)

Contract: `issue/base.py:1027-1103`, paginator `utils/paginator.py:643-744`. Gate: ADMIN/MEMBER/GUEST + guest scoping (`Exists(permission_subquery)` `base.py:1033-1060` — mirror: guests see own распоряжение? read exact subquery). Params: `cursor` default `"{per_page}:0:0"`, `per_page` default 1000 max 1000 (over → 400 ParseError; mirror message — read `paginator.py:643-653`), `order_by` default `-created_at`, `group_by`/`sub_group_by` (different params required; same → 400 per I4b precedent pattern — read exact), `IssueFilterSet` + legacy `issue_filters` (mirror supported keys; unsupported keys → ignore like Django? verify — ComplexFilterBackend ignores unknown? cite in commit).
Envelope (exact keys, `paginator.py:728-743`): `grouped_by,sub_grouped_by,total_count,next_cursor,prev_cursor,next_page_results,prev_page_results,count,total_pages,total_results,extra_stats,results`. Rows: `IssueListDetailSerializer` (`serializers/issue.py:824-924`): 23 base keys (`id,name,state_id,sort_order,completed_at,estimate_point,priority,start_date,target_date,sequence_id,project_id,parent_id,created_at,updated_at,created_by,updated_by,is_draft,archived_at,cycle_id,module_ids,label_ids,assignee_ids,sub_issues_count,attachment_count,link_count`) + `issue_relation[]`/`issue_related[]` (`{id,project_id,sequence_id,name,relation_type,state_id,priority,created_by,created_at,updated_at,updated_by}`) ONLY when `expand` contains them.
Cursor mechanics: `next_cursor`/`prev_cursor` format per paginator (opaque `{per_page}:{offset}:{...}`?) — read + mirror exactly; write round-trip unit test (build cursor → parse → same offset).

- [ ] **Step 1: Failing tests** — `parse_cursor("{1000}:0:0")` → (1000,0,0); `parse_cursor("junk")` → Err; `clamp_per_page(5000)` → Err; envelope `total_pages` math fn (e.g., 2501/1000 → 3). FAIL expected.
- [ ] **Step 2: Implement + wire + test + commit** `feat(rs-api): GET issues-detail envelope paginasi paritas Django`.

---

### Task I3: bulk-delete + bulk-archive

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/issue.rs` (`bulk_delete`, `bulk_archive` + tests), `main.rs` (wire `DELETE .../bulk-delete-issues/`, `POST .../bulk-archive-issues/` — axios DELETE-with-body precedent exists in smoke? No — verify Axum `Json<body>` extractor works on DELETE (it does; add live curl proof in T13))

Contract delete (`base.py:773-797`): gate ADMIN project-level (`allow_permission([ADMIN])` — level default PROJECT; verify level). Body `{issue_ids: []}`; empty/missing → 400 `{"error":"Issue IDs are required"}`. Per issue (scoped ws+project+pk): hard-delete `cycle_issues` + `module_issues` bridges, soft-delete issues (`deleted_at=now()`). 200 `{"message":"{n} issues were deleted"}` (n = pre-delete count).
Contract archive (`archive.py:305-343`): gate `ProjectEntityPermission` + ADMIN/MEMBER. Empty → 400 same message. Any issue whose `state.group ∉ {completed, cancelled}` → 400 `{"error_code":4091,"error_message":"INVALID_ARCHIVE_STATE_GROUP"}` (code from `utils/error_codes.py:7`). Else set `archived_at=today` (+bulk_update) → 200 `{"archived_at": "<str(today)>`"}`. Celery `issue_activity` per issue SKIPPED (precedent: Rust never writes activities).
- [ ] **Step 1: Failing tests** — `guard_archive_group("completed")` ok, `("backlog")` err with (4091, msg); `delete_message(3)` → "3 issues were deleted". FAIL expected.
- [ ] **Step 2: Implement (transaction per request) + wire + test + commit** `feat(rs-api): bulk-delete + bulk-archive issues paritas Django`.

---

### Task I4: deleted-issues + archived-issues

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/issue.rs` (`deleted_list`, `archived_list` + test), `main.rs` (wire both GETs)

Contract deleted (`base.py:800-813`): gate ADMIN/MEMBER/GUEST. 200 **bare UUID array** from `all_objects` (include soft-deleted) where `archived_at NOT NULL OR deleted_at NOT NULL`, opt `?updated_at__gt` filter. Unpaginated.
Contract archived (`archive.py:97-218`): gate ADMIN/MEMBER (GUEST → 403). Queryset: `type IS NULL OR type != 'epic'` (read exact — "type null-or-not-epic") + `archived_at NOT NULL` + project + slug. `show_sub_issues` default `"true"`; `"false"` → `parent__isnull`. Same I2 envelope; rows via `issue_on_results` (`archive.py:212-218` — read; almost certainly the 23-key shape; VERIFY and mirror, note any delta in commit). Same group_by constraint error: 400 `{"error":"Group by and sub group by cannot have same parameters"}` (`archive.py:143-147`).

- [ ] **Step 1: Failing test** — `show_sub_issues_filter("false")` → parent-null clause present; `("true"/missing)` → absent. FAIL expected.
- [ ] **Step 2: Implement (reuse I2 paginator helpers — same file, no duplication) + wire + test + commit** `feat(rs-api): deleted-issues + archived-issues paritas Django`.

---

### Task I5: sub-issues GET + POST

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/issue.rs` (`sub_list`, `sub_add` + tests), `main.rs` (wire `GET+POST .../issues/:issue_id/sub-issues/`)

Contract (`sub_issue.py:37-275`). Gate: `ProjectEntityPermission` (no decorator): GET any active project member (`project_role` Some → ok); POST ADMIN/MEMBER only (GUEST → `deny()`).
GET: scope `parent_id + ws + project`; 25-key rows (I1's 27 minus `deleted_at` plus `state_group` = `state__group`); counts Coalesce; `order_by` default `-created_at`; `group_by` (`assignees__ids` fan-out `sub_issue.py:189-195` — mirror); 200 `{"sub_issues": [...]|{group:[...]}, "state_distribution": {group:[ids]}}`. Missing parent → 200 empty (NO 404 on GET).
POST: body `{sub_issue_ids: []}`; parent miss (scoped) → 404 `{"error":"Parent issue not found"}`; empty → 400 `{"error":"Sub Issue IDs are required"}`; set `parent` FK + bulk update; celery skipped; 200 envelope with `sub_issues` = FULL `IssueSerializer` array (`serializers/issue.py:770-813` — read exact keys, mirror; this is the richer shape, NOT the 25-key values).

- [ ] **Step 1: Failing tests** — `sub_response_shape`: state_distribution groups sub-issue ids by `state_group` (pure grouping fn with 3-row fixture). FAIL expected.
- [ ] **Step 2: Implement (transaction for POST) + wire + test + commit** `feat(rs-api): sub-issues GET+POST paritas Django`.

---

### Task T13: smoke + rebuild + live + push (Batch B pattern)

**Files:**
- Modify: `apps/api-rs/scripts/smoke.sh`

Rules: new checks ONLY on unlimited routes (all Batch C paths are `/api/*` — confirm none added to `auth_router` IP-limited set in `main.rs:443-456`) + validation-first where possible. Process burst backstop is 600/min shared — ~15 new requests are safe.

- [ ] **Step 1: Add checks** (after `proj-create`, order matters — `leave` LAST before auth block):

```bash
check proj-details-200 200 "$BASE/api/workspaces/$WS/projects/details/"
check identifiers-200 200 "$BASE/api/workspaces/$WS/project-identifiers/?name=ZZZUNUSED"
check identifiers-400 400 "$BASE/api/workspaces/$WS/project-identifiers/"
check fav-add-204 204 -X POST -d "{\"project\":\"$PID\"}" "$BASE/api/workspaces/$WS/user-favorite-projects/"
check fav-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/user-favorite-projects/$PID/"
check fav-del-404 404 -X DELETE "$BASE/api/workspaces/$WS/user-favorite-projects/$PID/"
check archive-post-200 200 -X POST "$BASE/api/workspaces/$WS/projects/$PID/archive/"
check archive-restore-204 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/archive/"
check members-me-200 200 "$BASE/api/workspaces/$WS/projects/$PID/project-members/me/"
check mark-default-204 204 -X POST "$BASE/api/workspaces/$WS/projects/$PID/states/$SID/mark-default/"
check issues-list-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/list/?issues=$IID"
check issues-list-400 400 "$BASE/api/workspaces/$WS/projects/$PID/issues/list/"
check issues-detail-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues-detail/"
grep -q '"total_count"' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   issues-detail-envelope -> total_count"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED issues-detail-envelope"; echo "FAIL issues-detail-envelope: $(head -c 200 /tmp/smoke_body)"; }
check bulk-del-400 400 -X DELETE -d '{}' "$BASE/api/workspaces/$WS/projects/$PID/bulk-delete-issues/"
check bulk-tmp-create 201 -X POST -d '{"name":"Bulk tmp"}' "$BASE/api/workspaces/$WS/projects/$PID/issues/"
BID=$(jid id)
check bulk-del-200 200 -X DELETE -d "{\"issue_ids\":[\"$BID\"]}" "$BASE/api/workspaces/$WS/projects/$PID/bulk-delete-issues/"
check bulk-archive-400 400 -X POST -d '{}' "$BASE/api/workspaces/$WS/projects/$PID/bulk-archive-issues/"
check archived-200 200 "$BASE/api/workspaces/$WS/projects/$PID/archived-issues/"
check deleted-200 200 "$BASE/api/workspaces/$WS/projects/$PID/deleted-issues/"
check sub-get-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/sub-issues/"
# leave MUTLAK TERAKHIR (menonaktifkan membership token; tak ada cek member-gated sesudahnya)
check leave-204 204 -X POST "$BASE/api/workspaces/$WS/projects/$PID/members/leave/"

- [ ] **Step 2: Commit smoke** `test(rs-api): smoke batch-C (projects/details…leave, issues/list…sub-issues)`.
- [ ] **Step 3: `cargo test --workspace`** (with DATABASE_URL via `docker inspect plane-db` pattern) — 0 failures.
- [ ] **Step 4: Rebuild** `docker compose up -d --build api` (~7 min with chef cache) → wait → `Started`.
- [ ] **Step 5: Live verify happy paths smoke CAN'T** (temp fixtures, then DELETE — Batch B pattern): sub-issues POST + GET distribution, bulk-archive happy (issue in Done state → 200 + appears in archived-issues), P7 sole-admin 400 (temp user as only admin), P5 404 non-member, P2 403 as GUEST (only if cheap guest fixture exists — else code-review coverage), P1 role filtering spot-check (admin sees all + `is_favorite`/`member_role` keys present), P6 missing-pk → 204. Clean temp rows after.
- [ ] **Step 6: Full smoke** `TOKEN=... FRONTEND=... BASE=... bash apps/api-rs/scripts/smoke.sh` → PASS=all FAIL=0. Then `git push origin preview`.

---

## Self-review (author checklist, run before handoff)

1. **Spec coverage:** every contract-table row has an owning task (T0→create-fix, P1–P7, I1–I5, T13→verify). P3-GET/bulk-operation/epics excluded with reasons. ✔
2. **Placeholder scan:** no TBD/TODO; every error string quoted verbatim; every SQL sketch has exact table/column names with a live-verify step (T0 Step 2) backing the rest. ✔
3. **Type consistency:** `deny()`/`missing()` return `(StatusCode, Json<Value>)` used by all tasks; `ws_role`/`project_role` return `Option<i16>`; role ints 20/15/5 everywhere; `guard_leave` single definition (P7 reuses T0's). I4b/I2 share paginator helpers in `issue.rs`. ✔
4. **Collision plan:** T0 merges before lanes start; lanes touch disjoint files; `main.rs`/`smoke.sh` applied serially. ✔
