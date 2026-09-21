# Legacy Issue-Create Authorization Gate — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `POST /api/workspaces/:slug/projects/:project_id/issues/` and `/work-items/` must enforce the same PROJECT-level ADMIN/MEMBER permission as Django and return 403 `{"error": "You don't have the required permissions."}` for non-members, without changing the success path.

**Architecture:** Reuse the gate that v1 work-item handlers already apply (`require_project_write`), move it to `issue_common.rs` so both surfaces share one implementation, and call it as the first statement of `issue_write::create`. The handler's return type widens to `Json<Value>` so it can emit the shared `deny()` body. DB-backed integration tests extend the existing scratch fixture in the create test file.

**Tech Stack:** Rust 1.96 (edition 2021), Axum 0.7, sqlx 0.7 (Postgres), cargo test, local Postgres `plane` DB.

---

## Context an engineer must know

- Branch `preview`. The working tree already contains an **uncommitted sequence fix**: modified `issue_write.rs`, `intake.rs`, plus untracked `tests/issue_sequence_test.rs` and `migrations/0005_fix_issue_sequences.sql`. Do not revert these; build on top of them.
- The legacy create handler currently requires only a valid `AuthUser`; there is no membership query. Django reference: `apps/api/plane/app/views/issue/base.py:404` (`@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`) implemented by `apps/api/plane/app/permissions/base.py:53-81` — branch 1 = active project role 20/15; fallback = any active project membership **and** workspace role 20.
- The exact gate already exists at `apps/api-rs/crates/api/src/routes/v1/work_item.rs:571-587`. Reuse it, do not re-invent it.
- Deny body parity: `FORBIDDEN_MSG = "You don't have the required permissions."` (`routes/project.rs:8`), returned by `project::deny()`.
- Tests need a reachable DB. Host command:

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test
```

## File structure

| File                                                  | Change          | Responsibility                                                   |
| ----------------------------------------------------- | --------------- | ---------------------------------------------------------------- |
| `apps/api-rs/crates/api/src/routes/issue_common.rs`   | modify          | shared `require_project_write` gate                              |
| `apps/api-rs/crates/api/src/routes/v1/work_item.rs`   | modify          | delete its private copy, import the shared one                   |
| `apps/api-rs/crates/api/src/routes/issue_write.rs`    | modify          | call gate first; return `Json<Value>`                            |
| `apps/api-rs/crates/api/tests/issue_sequence_test.rs` | rename + modify | becomes `issue_create_test.rs`: fixture + sequence + authz tests |
| DB migration                                          | none            | authorization only, no schema change                             |

---

### Task 1: Scratch fixture + failing authorization tests (RED)

**Files:**

- Rename: `apps/api-rs/crates/api/tests/issue_sequence_test.rs` → `apps/api-rs/crates/api/tests/issue_create_test.rs`
- Test: same file (all edits below)

- [ ] **Step 1: Rename the file (it is untracked, plain `mv` is enough)**

```bash
mv apps/api-rs/crates/api/tests/issue_sequence_test.rs \
   apps/api-rs/crates/api/tests/issue_create_test.rs
```

- [ ] **Step 2: Update the module doc + imports**

Replace lines 1-19 of `issue_create_test.rs` with:

```rust
//! Regression tests for the legacy issue-create handler
//! (`issue_write::create`, backing both `/issues/` and `/work-items/`):
//! per-project sequence allocation and PROJECT-level ADMIN/MEMBER authz.

use api::middleware::auth::AuthUser;
use api::routes::intake::{
    create_issue as intake_create_issue, CreateIntakeIssue, IntakeIssuePayload,
};
use api::routes::issue_write::{create, CreateIssue};
use api::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
```

- [ ] **Step 3: Replace the `Scratch` struct + impl (current lines 42-195) with the version below**

The fixture gains a `project_members` row (the new gate requires it), reusable insert helpers, `add_actor()` for extra users with configurable memberships, and `extra_users` cleanup.

```rust
struct Scratch {
    slug: String,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    state_id: Uuid,
    extra_users: Vec<Uuid>,
}

async fn insert_user(pool: &PgPool, user_id: Uuid, username: &str) {
    sqlx::query(
        "INSERT INTO users (id, password, username, email, first_name, last_name, avatar, \
         date_joined, created_at, updated_at, last_location, created_location, is_superuser, \
         is_managed, is_password_expired, is_active, is_staff, is_email_verified, \
         is_password_autoset, token, user_timezone, last_login_ip, last_logout_ip, \
         last_login_medium, last_login_uagent, is_bot, display_name, is_email_valid, \
         is_password_reset_required) \
         VALUES ($1, '', $2, $3, '', '', '', now(), now(), now(), '', '', false, false, \
         false, true, false, false, true, $4, 'UTC', '', '', '', '', false, $2, true, false)",
    )
    .bind(user_id)
    .bind(username)
    .bind(format!("{username}@example.invalid"))
    .bind(Uuid::new_v4().simple().to_string())
    .execute(pool)
    .await
    .expect("scratch user");
}

async fn insert_workspace_member(pool: &PgPool, user_id: Uuid, workspace_id: Uuid, role: i16) {
    sqlx::query(
        "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
         workspace_id, view_props, default_props, issue_props, explored_features, \
         getting_started_checklist, tips, is_active) \
         VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', '{}', \
         '{}', true)",
    )
    .bind(role)
    .bind(user_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch workspace member");
}

async fn insert_project_member(
    pool: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    role: i16,
) {
    sqlx::query(
        "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, \
         view_props, default_props, sort_order, preferences, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, true, '{}', '{}', 65535, '{}', now(), now())",
    )
    .bind(user_id)
    .bind(role)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch project member");
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        purge(pool).await;

        let slug = format!("itseq-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let state_id = Uuid::new_v4();
        let identifier =
            format!("ITSQ{}", &Uuid::new_v4().simple().to_string()[..4]).to_uppercase();

        insert_user(pool, user_id, &slug).await;

        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'IT Seq Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("scratch workspace");

        insert_workspace_member(pool, user_id, workspace_id, 20).await;

        sqlx::query(
            "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
             identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
             intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
             is_issue_type_enabled, guest_view_all_features, timezone) \
             VALUES ($1, now(), now(), 'IT Seq Scratch', '', 2, $2, $3, false, false, false, \
             false, false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
        )
        .bind(project_id)
        .bind(&identifier)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch project");

        insert_project_member(pool, user_id, project_id, workspace_id, 20).await;

        sqlx::query(
            "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, \
             sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
             VALUES ($1, 'Backlog', '', '#60646C', 'backlog', $2, $3, 65535, 'backlog', true, \
             false, now(), now())",
        )
        .bind(state_id)
        .bind(project_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch state");

        Self {
            slug,
            workspace_id,
            user_id,
            project_id,
            state_id,
            extra_users: Vec::new(),
        }
    }

    /// Add an extra actor with the given roles. `None` skips that membership
    /// (outsider = `(None, None)`; ws-admin-only = `(Some(20), None)`).
    async fn add_actor(
        &mut self,
        pool: &PgPool,
        ws_role: Option<i16>,
        project_role: Option<i16>,
    ) -> Uuid {
        let user_id = Uuid::new_v4();
        let username = format!("{}-{}", self.slug, &user_id.simple().to_string()[..8]);
        insert_user(pool, user_id, &username).await;
        if let Some(role) = ws_role {
            insert_workspace_member(pool, user_id, self.workspace_id, role).await;
        }
        if let Some(role) = project_role {
            insert_project_member(pool, user_id, self.project_id, self.workspace_id, role).await;
        }
        self.extra_users.push(user_id);
        user_id
    }

    async fn cleanup(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM intake_issues WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_sequences WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issues WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM intakes WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM states WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM project_members WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(self.user_id)
            .execute(pool)
            .await
            .ok();
        for user_id in &self.extra_users {
            sqlx::query("DELETE FROM users WHERE id = $1")
                .bind(user_id)
                .execute(pool)
                .await
                .ok();
        }
    }
}
```

- [ ] **Step 4: Append the authz helper + tests at the end of the file**

```rust
async fn create_as(
    st: &AppState,
    scratch: &Scratch,
    actor: Uuid,
    name: &str,
) -> (StatusCode, Value) {
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(actor),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateIssue {
            name: name.to_string(),
            assignee_ids: None,
            label_ids: None,
            state_id: Some(scratch.state_id),
        }),
    )
    .await
    .expect("handler must return a response");
    (status, serde_json::to_value(body).expect("json body"))
}

#[tokio::test]
async fn outsider_create_is_forbidden_without_insert() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let outsider = scratch.add_actor(&st.pool, None, None).await;

    let (status, body) = create_as(&st, &scratch, outsider, "outsider-probe").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"error": "You don't have the required permissions."})
    );

    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = $1")
        .bind(scratch.project_id)
        .fetch_one(&st.pool)
        .await
        .unwrap();
    assert_eq!(persisted, 0, "forbidden create must not insert an issue");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn gate_runs_before_body_validation() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let outsider = scratch.add_actor(&st.pool, None, None).await;

    let result = create(
        State(st.clone()),
        AuthUser(outsider),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateIssue {
            name: String::new(),
            assignee_ids: None,
            label_ids: None,
            state_id: None,
        }),
    )
    .await;

    match result {
        Ok((status, Json(body))) => {
            assert_eq!(status, StatusCode::FORBIDDEN, "gate must win over validation");
            assert_eq!(
                serde_json::to_value(body).unwrap(),
                json!({"error": "You don't have the required permissions."})
            );
        }
        Err(e) => panic!("expected 403 before validation, got handler error: {e:?}"),
    }

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn guest_project_member_is_forbidden() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let guest = scratch.add_actor(&st.pool, Some(5), Some(5)).await;

    let (status, _) = create_as(&st, &scratch, guest, "guest-probe").await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn project_member_role_15_can_create() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;

    let (status, body) = create_as(&st, &scratch, member, "member-probe").await;
    assert_eq!(status, StatusCode::CREATED);

    let id = Uuid::parse_str(body["id"].as_str().expect("id in body")).unwrap();
    let created_by: Option<Uuid> =
        sqlx::query_scalar("SELECT created_by_id FROM issues WHERE id = $1")
            .bind(id)
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(created_by, Some(member));

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn workspace_admin_with_any_project_membership_can_create() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let admin = scratch.add_actor(&st.pool, Some(20), Some(5)).await;

    let (status, _) = create_as(&st, &scratch, admin, "ws-admin-probe").await;
    assert_eq!(status, StatusCode::CREATED);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn workspace_admin_without_project_membership_is_forbidden() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let admin = scratch.add_actor(&st.pool, Some(20), None).await;

    let (status, _) = create_as(&st, &scratch, admin, "ws-admin-outsider-probe").await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 5: Run the tests and watch the negative cases fail (RED)**

Run:

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

(`--test-threads=1` is required, not optional: `Scratch::new()->purge()` deletes all
`itseq-%` rows globally, so parallel tests race and delete each other's fixture rows.)

Expected: compilation succeeds; failures are behavioral:

- `outsider_create_is_forbidden_without_insert` → `left: 201, right: 403`
- `gate_runs_before_body_validation` → panics `expected 403 before validation, got handler error: AppError(...)`
- `guest_project_member_is_forbidden` → `left: 201, right: 403`
- The three positive tests already pass (they pin behavior that must not regress).

Do not proceed while any failure is a compile error.

---

### Task 2: Shared gate + wire it into the legacy create (GREEN)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_common.rs`
- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs:571-587` + imports at `:8-11`
- Modify: `apps/api-rs/crates/api/src/routes/issue_write.rs` (the `issue_common` import line and the `create` handler at `:204`)
- Modify: `apps/api-rs/crates/api/tests/issue_create_test.rs` (success-path assertions now read `Json<Value>`)

- [ ] **Step 1: Add the shared gate to `issue_common.rs`**

At the top, after `use crate::routes::project::ws_role;`, add:

```rust
use crate::state::AppState;
```

Append at the end of the file:

```rust
/// PROJECT-level ADMIN/MEMBER write gate, mirroring
/// `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])` (`permissions/base.py:53-81`):
/// branch 1 is an active project role 20/15; the fallback is any active
/// project membership plus a workspace ADMIN role. Shared by the v1
/// work-item handlers and the legacy issue create.
pub(crate) async fn require_project_write(
    st: &AppState,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, common::errors::AppError> {
    if !crate::routes::work_item::ws_active_member(&st.pool, user_id, slug).await? {
        return Ok(false);
    }
    let member_role = fetch_project_member_role(&st.pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, user_id, slug).await?;
    Ok(project_gate_allows(
        matches!(member_role, Some(20) | Some(15)),
        member_role.is_some(),
        ws_admin,
    ))
}
```

- [ ] **Step 2: Delete the private copy in `v1/work_item.rs`**

Delete this block (currently lines 571-587):

```rust
async fn require_project_write(
    st: &AppState,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, common::errors::AppError> {
    if !ws_active_member(&st.pool, user_id, slug).await? {
        return Ok(false);
    }
    let member_role = fetch_project_member_role(&st.pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, user_id, slug).await?;
    Ok(project_gate_allows(
        matches!(member_role, Some(20) | Some(15)),
        member_role.is_some(),
        ws_admin,
    ))
}
```

Add `require_project_write` to the existing import (currently lines 8-11):

```rust
use crate::routes::issue_common::{
    IssueDetailRow, IssueListRow, PageWindow, fetch_guest_scoped, fetch_project_member_role,
    is_workspace_admin, page_window, project_gate_allows, require_project_write,
};
```

The three other symbols stay: they are still used at `v1/work_item.rs:106-108, 247-249, 316-319, 362, 785-788`.

- [ ] **Step 3: Gate the legacy handler first and widen its return type**

In `issue_write.rs`, add `require_project_write` to the `issue_common` import:

```rust
use super::issue_common::{
    IssueOut, fetch_project_member_role, is_workspace_admin, project_gate_allows,
    require_project_write,
};
```

In `create` (currently line 204), change the signature and add the gate as the very first statement:

```rust
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django's permission decorator runs before serializer validation
    // (`views/issue/base.py:404`): a non-member gets 403 even with a bad body.
    if !require_project_write(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
```

At the end of `create`, serialize the success body (JSON shape stays `{"id": ..., "name": ...}`):

```rust
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(out).expect("IssueOut serializes")),
    ))
}
```

- [ ] **Step 4: Fix the two success-path assertions in `issue_create_test.rs`**

With `Json<Value>`, `first.id` no longer exists. In `create_allocates_distinct_sequences_and_records_counter_rows`, replace everything from the first `let (s1, Json(first)) = create(` down to the final `created_by` assertion with:

```rust
    let (s1, Json(first)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(body("seq-probe-a")),
    )
    .await
    .expect("first create must succeed");
    assert_eq!(s1, axum::http::StatusCode::CREATED);
    let first_id = Uuid::parse_str(first["id"].as_str().expect("id")).unwrap();

    let (s2, Json(second)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(body("seq-probe-b")),
    )
    .await
    .expect("second create must succeed");
    assert_eq!(s2, axum::http::StatusCode::CREATED);
    let second_id = Uuid::parse_str(second["id"].as_str().expect("id")).unwrap();

    let seq1 = sequence_of(&st.pool, first_id).await;
    let seq2 = sequence_of(&st.pool, second_id).await;
    assert_ne!(seq1, seq2, "consecutive creates must not reuse a sequence");

    assert_eq!(
        sequence_rows(&st.pool, first_id).await,
        vec![seq1 as i64],
        "create must persist the issue_sequences counter row"
    );
    assert_eq!(
        sequence_rows(&st.pool, second_id).await,
        vec![seq2 as i64],
        "create must persist the issue_sequences counter row"
    );

    let created_by: Option<Uuid> =
        sqlx::query_scalar("SELECT created_by_id FROM issues WHERE id = $1")
            .bind(first_id)
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(
        created_by,
        Some(scratch.user_id),
        "creator must be recorded"
    );
```

- [ ] **Step 5: Run the tests and watch everything pass (GREEN)**

Run:

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_create_test -- --test-threads=1
```

Expected: `test result: ok. 11 passed; 0 failed` (5 pre-existing + 6 new). Serial execution is required (see Task 1).

---

### Task 3: Full verification + live HTTP end-to-end

**Files:** none (verification only)

- [ ] **Step 0: Force a fresh `common` build (required, not optional)**

`sqlx::migrate!` embeds `migrations/*.sql` at compile time, and Cargo does **not**
recompile the `common` crate when a _new_ migration file appears (rerun-if-changed
gap). A stale test binary then fails with
`migration N was previously applied but is missing in the resolved migrations`.
Always force it first (build-only action, no source change):

```bash
cd apps/api-rs
cargo clean -p common
```

Running the suite against the live dev DB is intended: `migrate_test` deletes and
re-applies `_sqlx_migrations` by design, and migrations 0001-0005 are all verified
idempotent (0005 was proven no-op on clean data).

- [ ] **Step 1: Full Rust suite (no regressions)**

Run:

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane REDIS_URL=redis://localhost:6379 \
  cargo test -p api -p common --no-fail-fast -- --test-threads=1
```

Expected: `0 failed` in every target (baseline before this plan: 1063 passed, 1 ignored). Serial execution is required (see Task 1).

- [ ] **Step 2: Formatting + lints on changed code**

```bash
rustfmt --edition 2021 --check apps/api-rs/crates/api/tests/issue_create_test.rs
cd apps/api-rs && cargo clippy -p api --tests 2>&1 | grep -E "issue_create_test|issue_write.rs:(1[3-9][0-9]|2[0-9][0-9])|issue_common.rs" || true
```

Expected: test file is rustfmt-clean; no clippy warning points at the new lines. Do **not** run repo-wide `cargo fmt` (pre-existing files are not rustfmt-clean).

- [ ] **Step 3: Rebuild + restart the Rust API**

```bash
docker compose -f docker-compose-local.yml build api
docker compose -f docker-compose-local.yml up -d api worker beat-worker
sleep 5
docker logs plane-for-itsm-api-1 2>&1 | tail -n 3   # expect: rust-api listening on 8000
```

- [ ] **Step 4: HTTP end-to-end with two tokens (member vs outsider)**

Create scratch data (member role 20; outsider no membership):

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane -v ON_ERROR_STOP=1 <<'SQL'
INSERT INTO users (id, password, username, email, first_name, last_name, avatar, date_joined,
                   created_at, updated_at, last_location, created_location, is_superuser,
                   is_managed, is_password_expired, is_active, is_staff, is_email_verified,
                   is_password_autoset, token, user_timezone, last_login_ip, last_logout_ip,
                   last_login_medium, last_login_uagent, is_bot, display_name, is_email_valid,
                   is_password_reset_required)
VALUES ('00000000-0000-0000-0000-00000000f001', '', 'itseq-authz-member', 'm@example.invalid', '', '', '',
        now(), now(), now(), '', '', false, false, false, true, false, false, true, 'tk-f001',
        'UTC', '', '', '', '', false, 'itseq-authz-member', true, false),
       ('00000000-0000-0000-0000-00000000f005', '', 'itseq-authz-outsider', 'o@example.invalid', '', '', '',
        now(), now(), now(), '', '', false, false, false, true, false, false, true, 'tk-f005',
        'UTC', '', '', '', '', false, 'itseq-authz-outsider', true, false);
INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color)
VALUES ('00000000-0000-0000-0000-00000000f002', 'IT Authz E2E', 'itseq-authz',
        '00000000-0000-0000-0000-00000000f001', now(), now(), 'UTC', '#FFFFFF');
INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, workspace_id, view_props,
        default_props, issue_props, explored_features, getting_started_checklist, tips, is_active)
VALUES (gen_random_uuid(), now(), now(), 20, '00000000-0000-0000-0000-00000000f001',
        '00000000-0000-0000-0000-00000000f002', '{}', '{}', '{}', '{}', '{}', '{}', true);
INSERT INTO projects (id, created_at, updated_at, name, description, network, identifier,
        workspace_id, cycle_view, module_view, issue_views_view, page_view, intake_view, archive_in,
        close_in, logo_props, is_time_tracking_enabled, is_issue_type_enabled,
        guest_view_all_features, timezone)
VALUES ('00000000-0000-0000-0000-00000000f003', now(), now(), 'IT Authz E2E', '', 2, 'AUTHZ',
        '00000000-0000-0000-0000-00000000f002', false, false, false, false, false, 30, 30,
        '{}'::jsonb, false, false, false, 'UTC');
INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, view_props,
        default_props, sort_order, preferences, created_at, updated_at)
VALUES (gen_random_uuid(), '00000000-0000-0000-0000-00000000f001', 20,
        '00000000-0000-0000-0000-00000000f003', '00000000-0000-0000-0000-00000000f002', true,
        '{}', '{}', 65535, '{}', now(), now());
INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, "group",
        "default", is_triage, created_at, updated_at)
VALUES ('00000000-0000-0000-0000-00000000f004', 'Backlog', '', '#60646C', 'backlog',
        '00000000-0000-0000-0000-00000000f003', '00000000-0000-0000-0000-00000000f002', 65535,
        'backlog', true, false, now(), now());
INSERT INTO api_tokens (id, token, label, user_type, user_id, description, is_active, is_service,
        allowed_rate_limit, created_at, updated_at)
VALUES (gen_random_uuid(), 'itseq-authz-member-key', 'authz-member', 0,
        '00000000-0000-0000-0000-00000000f001', '', true, false, '60/minute', now(), now()),
       (gen_random_uuid(), 'itseq-authz-outsider-key', 'authz-outsider', 0,
        '00000000-0000-0000-0000-00000000f005', '', true, false, '60/minute', now(), now());
SQL
```

Then:

```bash
URL=http://localhost:8000/api/workspaces/itseq-authz/projects/00000000-0000-0000-0000-00000000f003/issues/
H=(-H 'Content-Type: application/json' -H 'Origin: http://localhost:3000')

# member -> 201
curl -sS -o /tmp/opencode/authz_member.json -w 'member: HTTP %{http_code}\n' -X POST "$URL" \
  "${H[@]}" -H 'X-Api-Key: itseq-authz-member-key' -d '{"name":"authz-member-probe"}'

# outsider -> 403 with the exact Django error body
curl -sS -o /tmp/opencode/authz_outsider.json -w 'outsider: HTTP %{http_code}\n' -X POST "$URL" \
  "${H[@]}" -H 'X-Api-Key: itseq-authz-outsider-key' -d '{"name":"authz-outsider-probe"}'
cat /tmp/opencode/authz_outsider.json      # {"error":"You don't have the required permissions."}

# outsider with invalid body -> still 403 (gate before validation)
curl -sS -o /dev/null -w 'outsider-invalid: HTTP %{http_code}\n' -X POST "$URL" \
  "${H[@]}" -H 'X-Api-Key: itseq-authz-outsider-key' -d '{"name":""}'
```

Expected: `member: HTTP 201`, `outsider: HTTP 403`, `outsider-invalid: HTTP 403`.
DB check — only the member's issue exists:

```bash
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c \
 "SELECT name, sequence_id FROM issues WHERE project_id = '00000000-0000-0000-0000-00000000f003';"
```

- [ ] **Step 5: Clean up the e2e scratch data**

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane -v ON_ERROR_STOP=1 -q <<'SQL'
DELETE FROM issue_sequences WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM issues WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM states WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM project_members WHERE project_id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM projects WHERE id = '00000000-0000-0000-0000-00000000f003';
DELETE FROM workspace_members WHERE workspace_id = '00000000-0000-0000-0000-00000000f002';
DELETE FROM workspaces WHERE id = '00000000-0000-0000-0000-00000000f002';
DELETE FROM api_tokens WHERE token IN ('itseq-authz-member-key', 'itseq-authz-outsider-key');
DELETE FROM users WHERE id IN ('00000000-0000-0000-0000-00000000f001', '00000000-0000-0000-0000-00000000f005');
SQL
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -t -A -c \
 "SELECT 'leftover_authz=' || count(*) FROM users WHERE username LIKE 'itseq-authz-%';"
```

Expected: `leftover_authz=0`.

- [ ] **Step 6: Optional live regression smoke (needs a token)**

`apps/api-rs/scripts/smoke.sh` creates its own workspace/project with the token owner, who becomes project ADMIN (role 20) at `routes/project.rs:560`, so `issue-create 201` must still pass:

```bash
TOKEN=<a valid api_tokens.token> bash apps/api-rs/scripts/smoke.sh
```

Expected: no new FAILs versus the pre-change baseline.

---

### Task 4: Commit (only when the user asks)

**Files:**

- `apps/api-rs/crates/api/src/routes/issue_common.rs`
- `apps/api-rs/crates/api/src/routes/v1/work_item.rs`
- `apps/api-rs/crates/api/src/routes/issue_write.rs`
- `apps/api-rs/crates/api/tests/issue_create_test.rs`

- [ ] **Step 1: Commit (skip unless the user explicitly requests it)**

```bash
git add apps/api-rs/crates/api/src/routes/issue_common.rs \
        apps/api-rs/crates/api/src/routes/v1/work_item.rs \
        apps/api-rs/crates/api/src/routes/issue_write.rs \
        apps/api-rs/crates/api/tests/issue_create_test.rs
git commit -m "fix(api-rs): enforce ADMIN/MEMBER gate on legacy issue create"
```

Note: the working tree also holds the earlier uncommitted sequence fix (`intake.rs`, `migrations/0005_fix_issue_sequences.sql`) — leave those for a separate commit unless the user asks to include them.

---

## Out of scope (do not touch in this plan)

- Assignee/label bridges + default assignee on create (finding #1 of the audit).
- `validate_create` failures still map to 500 instead of Django's 400 (`AppError` is 500-only). Pre-existing; tracked separately.
- Create response shape (`{id, name}` vs Django's full issue payload).
- `intake::create_issue` keeps its existing workspace-level `ws_role` gate.

## Self-review

- Spec coverage: only the gate is in scope; negative tests cover outsider, guest, ws-admin-only, and gate-before-validation; positive tests cover role 15 and ws-admin fallback. ✅
- Placeholder scan: every code/command block is complete. ✅
- Type consistency: `require_project_write(&AppState, Uuid, &str, Uuid) -> Result<bool, AppError>` is identical in `issue_common.rs` and both call sites; `create` returns `Json<Value>` and the tests destructure `Value`. ✅
