# RCB/TCB Backend — Phase 1: Schema + Releases API — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the full RCB/TCB database schema plus a working workspace-level Release API (CRUD + soft delete + change bundling lintas project) so releases can dirakit sebelum review engine dibangun.

**Architecture:** Satu migrasi (`0012_releases_review_control.sql`) membuat keenam tabel fitur. Phase ini mengimplementasikan `routes/release.rs` saja (tabel `releases` + `release_changes`); tabel review (requests/sessions/participants/items) hanya dibuat skemanya, dipakai Phase 2. Handler workspace-scoped, cookie-auth, mengikuti `routes/workflow.rs` (gate workspace) dan `routes/service.rs` (validasi enum, bentuk error 400). Test integrasi DB-backed mengikuti pola `tests/war_room_test.rs`.

**Tech Stack:** Rust (axum 0.7, sqlx runtime queries), PostgreSQL 15, uuid, chrono (serde), serde_json. Test: `cargo test -p api --test release_review_test -- --test-threads=1`.

**Spec:** `docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md`

**Keputusan scope yang disepakati saat planning:**

- Tidak ada release activity log server-side di v1; perubahan lingkup saat review aktif hanya memunculkan warning di UI (web phase). Spec direvisi mengikuti ini.
- `minutes`/`submission_note` memakai `text NOT NULL DEFAULT ''` (pola `war_rooms.notes_html`), bukan nullable — bentuk JSON tetap string kosong.
- Field JSON `target_date` = `YYYY-MM-DD` atau `null`.

**Conventions (verified 2026-10-02):**

- `crate::routes::project::{deny, missing, ws_role}` — `deny()` = 403 `{"error":"You don't have the required permissions."}`; `missing()` = 404 `{"error":"The required object does not exist."}` (`project.rs:12-38`).
- Workspace member = `ws_role(...).is_some()`; workspace admin = `Some(20)` (`workflow.rs:720-722`).
- `crate::routes::workflow::validate_name(&str, field)` → trimmed `Result<String, String>` (`workflow.rs:29-38`).
- `crate::routes::service::{validate_enum, bad_request}` — `pub fn validate_enum(field, value, allowed)`, `pub(crate) fn bad_request(msg)` (`service.rs:32-38, 201-206`).
- Migrasi diterapkan saat boot oleh `common::db::migrate` (`crates/common/src/db.rs:21`); rebuild `docker compose -f docker-compose-local.yml up -d --build api`.
- Registrasi route di `crates/api/src/main.rs` (blok war rooms berakhir di baris ~800); daftar modul di `routes/mod.rs` (alfabetis).
- Handler signature: `State(st): State<AppState>`, `auth: AuthUser` (`auth.0` = user id), `Path(...)`, `Query(...)`, `Json(body)`; return `Result<(StatusCode, Json<Value>), common::errors::AppError>`.
- Test scratch fixture: `tests/war_room_test.rs:48-302` (insert_user/workspace_member/project_member; `Scratch`; `cleanup` hapus notifications → issue_sequences → workspaces → users).
- Semua timestamp `timestamptz`; `issues.sequence_id` dipakai untuk identifier `PROJ-<n>`.

---

### Task 1: Migration `0012_releases_review_control.sql`

**Files:**

- Create: `apps/api-rs/migrations/0012_releases_review_control.sql`

- [ ] **Step 1: Write the migration**

Create `apps/api-rs/migrations/0012_releases_review_control.sql`:

```sql
-- Release & Testing Control Boards (RCB/TCB): release bundles + generic
-- review engine (requests, sessions, agenda outcomes, participants).
-- Spec: docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md
-- Delta applied at boot by `common::db::migrate`.

CREATE TABLE IF NOT EXISTS public.releases (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    sequence_id bigint NOT NULL,
    name character varying(255) NOT NULL,
    version character varying(100),
    description_html text NOT NULL DEFAULT '',
    status character varying(20) NOT NULL DEFAULT 'draft',
    target_date date,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT releases_status_check
        CHECK (status IN ('draft', 'planned', 'in_review', 'approved', 'released', 'cancelled'))
);

CREATE UNIQUE INDEX IF NOT EXISTS releases_workspace_sequence_idx
    ON public.releases (workspace_id, sequence_id);

CREATE INDEX IF NOT EXISTS releases_workspace_status_idx
    ON public.releases (workspace_id, status) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.release_changes (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    release_id uuid NOT NULL REFERENCES public.releases(id) ON DELETE CASCADE,
    issue_id uuid NOT NULL REFERENCES public.issues(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS release_changes_pair_idx
    ON public.release_changes (release_id, issue_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS release_changes_issue_idx
    ON public.release_changes (issue_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS release_changes_release_idx
    ON public.release_changes (release_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.review_requests (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    board_type character varying(3) NOT NULL,
    change_issue_id uuid REFERENCES public.issues(id) ON DELETE CASCADE,
    release_id uuid REFERENCES public.releases(id) ON DELETE CASCADE,
    project_id uuid REFERENCES public.projects(id) ON DELETE CASCADE,
    status character varying(10) NOT NULL DEFAULT 'pending',
    submission_note text NOT NULL DEFAULT '',
    submitted_by_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    submitted_at timestamp with time zone NOT NULL DEFAULT now(),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT review_requests_board_check CHECK (board_type IN ('tcb', 'rcb')),
    CONSTRAINT review_requests_status_check
        CHECK (status IN ('pending', 'scheduled', 'decided', 'withdrawn')),
    CONSTRAINT review_requests_subject_check CHECK (
        (board_type = 'tcb' AND change_issue_id IS NOT NULL AND project_id IS NOT NULL AND release_id IS NULL)
        OR
        (board_type = 'rcb' AND release_id IS NOT NULL AND change_issue_id IS NULL AND project_id IS NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS review_requests_active_tcb_subject_idx
    ON public.review_requests (change_issue_id)
    WHERE board_type = 'tcb' AND status IN ('pending', 'scheduled') AND deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS review_requests_active_rcb_subject_idx
    ON public.review_requests (release_id)
    WHERE board_type = 'rcb' AND status IN ('pending', 'scheduled') AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS review_requests_workspace_board_status_idx
    ON public.review_requests (workspace_id, board_type, status) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS review_requests_project_idx
    ON public.review_requests (project_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.review_sessions (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    board_type character varying(3) NOT NULL,
    project_id uuid REFERENCES public.projects(id) ON DELETE CASCADE,
    title character varying(255) NOT NULL,
    scheduled_at timestamp with time zone NOT NULL,
    status character varying(10) NOT NULL DEFAULT 'scheduled',
    minutes text NOT NULL DEFAULT '',
    location character varying(255),
    completed_at timestamp with time zone,
    cancelled_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT review_sessions_board_check CHECK (board_type IN ('tcb', 'rcb')),
    CONSTRAINT review_sessions_status_check
        CHECK (status IN ('scheduled', 'completed', 'cancelled')),
    CONSTRAINT review_sessions_scope_check CHECK (
        (board_type = 'tcb' AND project_id IS NOT NULL)
        OR (board_type = 'rcb' AND project_id IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS review_sessions_workspace_board_status_idx
    ON public.review_sessions (workspace_id, board_type, status) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.review_session_participants (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    session_id uuid NOT NULL REFERENCES public.review_sessions(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    role character varying(20) NOT NULL DEFAULT 'member',
    attendance character varying(10) NOT NULL DEFAULT 'invited',
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT review_session_participants_role_check
        CHECK (role IN ('chair', 'secretary', 'member')),
    CONSTRAINT review_session_participants_attendance_check
        CHECK (attendance IN ('invited', 'present', 'absent'))
);

CREATE UNIQUE INDEX IF NOT EXISTS review_session_participants_pair_idx
    ON public.review_session_participants (session_id, user_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.review_session_items (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    session_id uuid NOT NULL REFERENCES public.review_sessions(id) ON DELETE CASCADE,
    review_request_id uuid NOT NULL REFERENCES public.review_requests(id) ON DELETE CASCADE,
    position integer NOT NULL DEFAULT 65535,
    outcome character varying(30),
    outcome_note text NOT NULL DEFAULT '',
    decided_by_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    decided_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id),
    CONSTRAINT review_session_items_outcome_check CHECK (
        outcome IS NULL
        OR outcome IN ('approved', 'rejected', 'approved_with_notes', 'deferred')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS review_session_items_pair_idx
    ON public.review_session_items (session_id, review_request_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS review_session_items_request_idx
    ON public.review_session_items (review_request_id) WHERE deleted_at IS NULL;
```

- [ ] **Step 2: Apply the migration by rebuilding the Rust API**

Run (detached; link stage can take 10+ minutes with no output — see `AGENTS.md`):

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Then poll until healthy:

```bash
until curl -sf http://localhost:8000/health > /dev/null; do sleep 15; done; echo up
```

Expected: `up`; `migrate failed` tidak muncul di `docker logs plane-for-itsm-api-1`.

- [ ] **Step 3: Verify the tables exist**

Run:

```bash
docker compose -f docker-compose-local.yml exec -T plane-db psql -U plane -d plane \
  -c "\d public.releases" -c "\d public.release_changes" -c "\d public.review_requests" \
  -c "\d public.review_sessions" -c "\d public.review_session_participants" \
  -c "\d public.review_session_items"
```

Expected: enam tabel dengan index/constraint yang diberi nama di atas (mis. `releases_workspace_sequence_idx`, `review_requests_subject_check`).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/0012_releases_review_control.sql
git commit -m "feat(api-rs): rcb/tcb schema migration"
```

---

### Task 2: `routes/release.rs` — helpers, list, create

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/release.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Test: `apps/api-rs/crates/api/tests/release_review_test.rs`

- [ ] **Step 1: Write the failing tests**

Create `apps/api-rs/crates/api/tests/release_review_test.rs`:

```rust
//! RCB/TCB backend integration tests. DB-backed; run serially:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test release_review_test -- --test-threads=1`

use api::middleware::auth::AuthUser;
use api::routes::release::{create, list, CreateRelease, ListParams};
use api::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://plane:plane@localhost:5432/plane".into())
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url())
        .await
        .expect("test database must be reachable (set DATABASE_URL)")
}

async fn state() -> AppState {
    AppState {
        pool: pool().await,
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: AppConfig::from_env(),
    }
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

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    state_id: Uuid,
    type_id: Uuid,
    extra_users: Vec<Uuid>,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("rr-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let state_id = Uuid::new_v4();
        let type_id = Uuid::new_v4();
        let identifier = format!("RR{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();

        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Release Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
             VALUES ($1, now(), now(), 'Release Scratch', '', 2, $2, $3, false, false, false, \
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
             VALUES ($1, 'New', '', '#F59E0B', 'new', $2, $3, 65535, \
             'backlog', true, false, now(), now())",
        )
        .bind(state_id)
        .bind(project_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch state");
        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
             is_default, level, is_epic, created_at, updated_at) \
             VALUES ($1, 'Change', '', '{}'::jsonb, $2, true, true, 0, false, now(), now())",
        )
        .bind(type_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch issue type");

        Self {
            slug,
            workspace_id,
            user_id,
            project_id,
            state_id,
            type_id,
            extra_users: Vec::new(),
        }
    }

    async fn add_actor(&mut self, pool: &PgPool, ws_role: Option<i16>) -> Uuid {
        let user_id = Uuid::new_v4();
        let username = format!("{}-{}", self.slug, &user_id.simple().to_string()[..8]);
        insert_user(pool, user_id, &username).await;
        if let Some(role) = ws_role {
            insert_workspace_member(pool, user_id, self.workspace_id, role).await;
        }
        self.extra_users.push(user_id);
        user_id
    }

    async fn insert_issue(&self, pool: &PgPool) -> Uuid {
        let mut tx = pool.begin().await.expect("scratch tx");
        let (issue_id, sequence_id): (Uuid, i32) = sqlx::query_as(
            "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, \
             sort_order, sequence_id, state_id, type_id, project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Change request', '<p></p>', '{}', 'none', false, \
             65535, (SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM issues WHERE project_id = $3), \
             $1, $2, $3, $4, now(), now()) RETURNING id, sequence_id",
        )
        .bind(self.state_id)
        .bind(self.type_id)
        .bind(self.project_id)
        .bind(self.workspace_id)
        .fetch_one(&mut *tx)
        .await
        .expect("scratch issue");
        // DB-wide `issue_sequences` invariant (checked by a parallel test binary).
        sqlx::query(
            "INSERT INTO issue_sequences (id, sequence, issue_id, project_id, workspace_id, \
             created_by_id, deleted, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, false, now(), now())",
        )
        .bind(sequence_id)
        .bind(issue_id)
        .bind(self.project_id)
        .bind(self.workspace_id)
        .bind(self.user_id)
        .execute(&mut *tx)
        .await
        .expect("scratch issue sequence");
        tx.commit().await.expect("scratch commit");
        issue_id
    }

    async fn cleanup(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_sequences WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        let mut user_ids = vec![self.user_id];
        user_ids.extend(self.extra_users.iter().copied());
        sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(&user_ids)
            .execute(pool)
            .await
            .ok();
    }
}

fn create_body(name: &str) -> CreateRelease {
    CreateRelease {
        name: Some(name.to_string()),
        version: None,
        description_html: None,
        status: None,
        target_date: None,
    }
}

#[tokio::test]
async fn create_release_defaults_and_sequence() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis 2026.10")),
    )
    .await
    .expect("create release");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["sequence_id"], 1);
    assert_eq!(body["name"], "Rilis 2026.10");
    assert_eq!(body["status"], "draft");
    assert!(body["version"].is_null());
    assert!(body["target_date"].is_null());

    let (status, Json(second)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis 2026.11")),
    )
    .await
    .expect("create second release");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["sequence_id"], 2);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_release_validates_name_status_and_membership() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("   ")),
    )
    .await
    .expect("blank name");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "name is required");

    let mut bad_status = create_body("Rilis X");
    bad_status.status = Some("bogus".into());
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(bad_status),
    )
    .await
    .expect("bad status");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid status");

    let outsider = scratch.add_actor(&st.pool, None).await;
    let (status, _) = create(
        State(st.clone()),
        AuthUser(outsider),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis Y")),
    )
    .await
    .expect("outsider");
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn list_releases_filters_status() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis A")),
    )
    .await
    .expect("create first");
    let mut cancelled_body = create_body("Rilis B");
    cancelled_body.status = Some("cancelled".into());
    create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(cancelled_body),
    )
    .await
    .expect("create second");

    let (status, Json(all)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(ListParams {
            status: None,
            target_date_from: None,
            target_date_to: None,
        }),
    )
    .await
    .expect("list all");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().unwrap().len(), 2);

    let (_, Json(cancelled)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(ListParams {
            status: Some("cancelled".into()),
            target_date_from: None,
            target_date_to: None,
        }),
    )
    .await
    .expect("list filtered");
    assert_eq!(cancelled.as_array().unwrap().len(), 1);
    assert_eq!(cancelled[0]["name"], "Rilis B");

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd apps/api-rs && cargo test -p api --test release_review_test --no-run
```

Expected: FAIL to compile — `api::routes::release` not found.

- [ ] **Step 3: Create `routes/release.rs`**

Create `apps/api-rs/crates/api/src/routes/release.rs`:

```rust
//! Releases: workspace-level bundling of work items for RCB review.
//! Spec: docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md
//! Review engine (requests/sessions) lives in a later phase.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing, ws_role},
    state::AppState,
};

use super::service::{bad_request, validate_enum};
use super::workflow::validate_name;

/// Allowed `releases.status` values (spec §Model data).
pub const RELEASE_STATUSES: &[&str] =
    &["draft", "planned", "in_review", "approved", "released", "cancelled"];

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

/// `YYYY-MM-DD`, or `Invalid target_date` for anything else.
pub fn parse_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .map_err(|_| "Invalid target_date - expected YYYY-MM-DD.".to_string())
}

pub(crate) async fn gate_ws_member(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    Ok(ws_role(pool, user, slug).await?.is_some())
}

pub(crate) async fn gate_ws_admin(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    Ok(matches!(ws_role(pool, user, slug).await?, Some(20)))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReleaseRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub sequence_id: i64,
    pub name: String,
    pub version: Option<String>,
    pub description_html: String,
    pub status: String,
    pub target_date: Option<NaiveDate>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

const RELEASE_SELECT: &str = "SELECT r.id, r.workspace_id, r.sequence_id, r.name, r.version, \
    r.description_html, r.status, r.target_date, r.created_at, r.updated_at, \
    r.created_by_id, r.updated_by_id FROM releases r";

pub(crate) fn release_json(row: &ReleaseRow) -> Value {
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "sequence_id": row.sequence_id,
        "name": row.name,
        "version": row.version,
        "description_html": row.description_html,
        "status": row.status,
        "target_date": row.target_date,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": row.created_by_id,
        "updated_by": row.updated_by_id,
    })
}

pub(crate) async fn release_in_workspace(
    pool: &sqlx::PgPool,
    slug: &str,
    release_id: Uuid,
) -> Result<Option<ReleaseRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{RELEASE_SELECT} JOIN workspaces w ON w.id = r.workspace_id \
         WHERE r.id = $1 AND w.slug = $2 AND w.deleted_at IS NULL AND r.deleted_at IS NULL"
    ))
    .bind(release_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub target_date_from: Option<NaiveDate>,
    pub target_date_to: Option<NaiveDate>,
}

/// GET `/api/workspaces/:slug/releases/`
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(params): Query<ListParams>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    if let Some(status) = params.status.as_deref() {
        if let Err(e) = validate_enum("status", status, RELEASE_STATUSES) {
            return Ok(bad_request(e));
        }
    }
    let rows: Vec<ReleaseRow> = sqlx::query_as(&format!(
        "{RELEASE_SELECT} JOIN workspaces w ON w.id = r.workspace_id \
         WHERE w.slug = $1 AND w.deleted_at IS NULL AND r.deleted_at IS NULL \
         AND ($2::text IS NULL OR r.status = $2) \
         AND ($3::date IS NULL OR r.target_date >= $3) \
         AND ($4::date IS NULL OR r.target_date <= $4) \
         ORDER BY r.created_at DESC"
    ))
    .bind(&slug)
    .bind(&params.status)
    .bind(params.target_date_from)
    .bind(params.target_date_to)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(release_json).collect())),
    ))
}

#[derive(Debug, Deserialize)]
pub struct CreateRelease {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description_html: Option<String>,
    pub status: Option<String>,
    pub target_date: Option<NaiveDate>,
}

/// POST `/api/workspaces/:slug/releases/`
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<CreateRelease>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let name = match validate_name(body.name.as_deref().unwrap_or(""), "name") {
        Ok(name) => name,
        Err(e) => return Ok(bad_request(e)),
    };
    let version = match body.version.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(v) if v.chars().count() > 100 => {
            return Ok(bad_request("Ensure version has no more than 100 characters."))
        }
        Some(v) => Some(v.to_string()),
    };
    let status = body.status.clone().unwrap_or_else(|| "draft".to_string());
    if let Err(e) = validate_enum("status", &status, RELEASE_STATUSES) {
        return Ok(bad_request(e));
    }
    let id = Uuid::new_v4();
    let mut tx = st.pool.begin().await?;
    let workspace_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(&slug)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(workspace_id) = workspace_id else {
        return Ok(missing());
    };
    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM releases WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO releases (id, workspace_id, sequence_id, name, version, description_html, \
         status, target_date, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now(), $9, $9)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(sequence)
    .bind(&name)
    .bind(&version)
    .bind(body.description_html.clone().unwrap_or_default())
    .bind(&status)
    .bind(body.target_date)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row: ReleaseRow = sqlx::query_as(&format!("{RELEASE_SELECT} WHERE r.id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(release_json(&row))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_accepts_iso_and_trims() {
        assert_eq!(
            parse_date(" 2026-10-02 ").unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 2).unwrap()
        );
        assert_eq!(
            parse_date("02-10-2026").unwrap_err(),
            "Invalid target_date - expected YYYY-MM-DD."
        );
    }

    #[test]
    fn status_enum_matches_spec() {
        assert!(validate_enum("status", "draft", RELEASE_STATUSES).is_ok());
        assert!(validate_enum("status", "in_review", RELEASE_STATUSES).is_ok());
        assert!(validate_enum("status", "bogus", RELEASE_STATUSES).is_err());
    }
}
```

- [ ] **Step 4: Register the module and routes**

In `apps/api-rs/crates/api/src/routes/mod.rs`, add between `pub mod reactions;` and `pub mod s3proxy;`:

```rust
pub mod release;
```

In `apps/api-rs/crates/api/src/main.rs`, add after the war-room block (after line 800, before the states block at line ~801):

```rust
        // Releases (RCB scope): workspace-level bundles of work items.
        // Session auth; reads/writes = active workspace member; delete =
        // creator or workspace admin. Spec:
        // docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md
        .route(
            "/api/workspaces/:slug/releases/",
            get(routes::release::list).post(routes::release::create),
        )
        .route(
            "/api/workspaces/:slug/releases/:pk/",
            get(routes::release::detail)
                .patch(routes::release::patch)
                .delete(routes::release::destroy),
        )
        .route(
            "/api/workspaces/:slug/releases/:pk/changes/",
            get(routes::release::changes_list).post(routes::release::changes_create),
        )
        .route(
            "/api/workspaces/:slug/releases/:pk/changes/:issue_id/",
            delete(routes::release::changes_destroy),
        )
```

Note: `detail`, `patch`, `destroy`, `changes_*` belum ada di Task 2 — kompilasi `main.rs` baru lengkap setelah Task 3 & 4. Sementara task ini, registrasikan **hanya** blok `list`/`create` dulu; blok route lainnya ditambahkan di task terkait. (Urutan: Task 2 tambah route `/releases/`; Task 3 tambah `/releases/:pk/`; Task 4 tambah route `changes`.)

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test create_release -- --test-threads=1
```

Expected: PASS (3 tests: `create_release_defaults_and_sequence`, `create_release_validates_name_status_and_membership`, `list_releases_filters_status`).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/release.rs \
  apps/api-rs/crates/api/src/routes/mod.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/release_review_test.rs
git commit -m "feat(api-rs): release list/create handlers"
```

---

### Task 3: Release detail, patch, destroy (guards)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/release.rs`
- Modify: `apps/api-rs/crates/api/src/routes/service.rs` (export `deserialize_present`)
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Test: `apps/api-rs/crates/api/tests/release_review_test.rs`

- [ ] **Step 1: Export `deserialize_present` from `service.rs`**

In `apps/api-rs/crates/api/src/routes/service.rs:160`, change:

```rust
fn deserialize_present<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
```

to:

```rust
pub(crate) fn deserialize_present<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
```

- [ ] **Step 2: Update imports and write the failing tests**

In `apps/api-rs/crates/api/tests/release_review_test.rs`, change the import line to:

```rust
use api::routes::release::{
    create, destroy, detail, list, patch, CreateRelease, ListParams, PatchRelease,
};
```

Then append to the file:

```rust
async fn insert_review_request(
    pool: &PgPool,
    workspace_id: Uuid,
    release_id: Uuid,
    user_id: Uuid,
    status: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO review_requests (id, workspace_id, board_type, release_id, status, \
         submission_note, submitted_by_id, submitted_at, created_at, updated_at, \
         created_by_id, updated_by_id) \
         VALUES (gen_random_uuid(), $1, 'rcb', $2, $3, '', $4, now(), now(), now(), $4, $4) \
         RETURNING id",
    )
    .bind(workspace_id)
    .bind(release_id)
    .bind(status)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("scratch review request")
}

fn release_id(body: &Value) -> Uuid {
    body["id"].as_str().unwrap().parse().unwrap()
}

#[tokio::test]
async fn detail_release_includes_empty_bundles() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis detail")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);

    let (status, Json(body)) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("detail");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Rilis detail");
    assert_eq!(body["changes"].as_array().unwrap().len(), 0);
    assert_eq!(body["review_requests"].as_array().unwrap().len(), 0);

    let (status, _) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), Uuid::new_v4())),
    )
    .await
    .expect("missing detail");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn patch_release_updates_and_clears_optional_fields() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(CreateRelease {
            name: Some("Rilis patch".into()),
            version: Some("v1.0.0".into()),
            description_html: None,
            status: None,
            target_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()),
        }),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);
    assert_eq!(release["version"], "v1.0.0");

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
        Json(PatchRelease {
            name: Some("Rilis patch v2".into()),
            version: Some(None),
            description_html: None,
            status: Some("planned".into()),
            target_date: Some(Some(
                chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            )),
        }),
    )
    .await
    .expect("patch");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Rilis patch v2");
    assert!(body["version"].is_null());
    assert_eq!(body["status"], "planned");
    assert_eq!(body["target_date"], "2026-11-01");

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
        Json(PatchRelease {
            status: Some("bogus".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("bad status");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid status");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn destroy_release_guards_active_review_and_permissions() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis hapus")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);

    let member = scratch.add_actor(&st.pool, Some(15)).await;
    let (status, _) = destroy(
        State(st.clone()),
        AuthUser(member),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("non-creator delete");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let request_id = insert_review_request(
        &st.pool,
        scratch.workspace_id,
        rid,
        scratch.user_id,
        "pending",
    )
    .await;
    let (status, Json(body)) = destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("guarded delete");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Release has an active review request");

    sqlx::query("UPDATE review_requests SET status = 'withdrawn' WHERE id = $1")
        .bind(request_id)
        .execute(&st.pool)
        .await
        .expect("withdraw request");

    let (status, _) = destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("delete");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, Json(all)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(ListParams {
            status: None,
            target_date_from: None,
            target_date_to: None,
        }),
    )
    .await
    .expect("list after delete");
    assert_eq!(all.as_array().unwrap().len(), 0);

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
cd apps/api-rs && cargo test -p api --test release_review_test detail_includes --no-run
```

Expected: FAIL to compile — `detail`, `patch`, `destroy`, `PatchRelease` not found.

- [ ] **Step 4: Implement detail, patch, destroy**

Append to `apps/api-rs/crates/api/src/routes/release.rs` (sebelum `#[cfg(test)] mod tests`):

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReleaseChangeRow {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub project_id: Uuid,
    pub issue_name: String,
    pub issue_identifier: String,
    pub project_identifier: String,
}

fn change_json(row: &ReleaseChangeRow) -> Value {
    json!({
        "id": row.id,
        "issue_id": row.issue_id,
        "project_id": row.project_id,
        "issue_name": row.issue_name,
        "issue_identifier": row.issue_identifier,
        "project_identifier": row.project_identifier,
    })
}

pub(crate) const CHANGE_SELECT: &str = "SELECT rc.id, rc.issue_id, rc.project_id, \
    i.name AS issue_name, (p.identifier || '-' || i.sequence_id::text) AS issue_identifier, \
    p.identifier AS project_identifier FROM release_changes rc \
    JOIN issues i ON i.id = rc.issue_id JOIN projects p ON p.id = rc.project_id";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReviewRequestRow {
    pub id: Uuid,
    pub board_type: String,
    pub status: String,
    pub submission_note: String,
    pub submitted_by_id: Option<Uuid>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

fn request_json(row: &ReviewRequestRow) -> Value {
    json!({
        "id": row.id,
        "board_type": row.board_type,
        "status": row.status,
        "submission_note": row.submission_note,
        "submitted_by": row.submitted_by_id,
        "submitted_at": row.submitted_at,
    })
}

/// GET `/api/workspaces/:slug/releases/:pk/`
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let Some(row) = release_in_workspace(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    let changes: Vec<ReleaseChangeRow> = sqlx::query_as(&format!(
        "{CHANGE_SELECT} WHERE rc.release_id = $1 AND rc.deleted_at IS NULL ORDER BY rc.created_at ASC"
    ))
    .bind(pk)
    .fetch_all(&st.pool)
    .await?;
    let requests: Vec<ReviewRequestRow> = sqlx::query_as(
        "SELECT id, board_type, status, submission_note, submitted_by_id, submitted_at \
         FROM review_requests WHERE release_id = $1 AND deleted_at IS NULL \
         ORDER BY submitted_at DESC",
    )
    .bind(pk)
    .fetch_all(&st.pool)
    .await?;
    let mut value = release_json(&row);
    value["changes"] = json!(changes.iter().map(change_json).collect::<Vec<_>>());
    value["review_requests"] = json!(requests.iter().map(request_json).collect::<Vec<_>>());
    Ok((StatusCode::OK, Json(value)))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchRelease {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub version: Option<Option<String>>,
    pub description_html: Option<String>,
    pub status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub target_date: Option<Option<NaiveDate>>,
}

/// PATCH `/api/workspaces/:slug/releases/:pk/`
pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
    Json(body): Json<PatchRelease>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let Some(current) = release_in_workspace(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    let name = match body.name.as_deref() {
        Some(raw) => match validate_name(raw, "name") {
            Ok(name) => name,
            Err(e) => return Ok(bad_request(e)),
        },
        None => current.name.clone(),
    };
    let version = match body.version.clone() {
        Some(Some(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.chars().count() > 100 {
                return Ok(bad_request(
                    "Ensure version has no more than 100 characters.",
                ));
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(None) => None,
        None => current.version.clone(),
    };
    let status = body.status.clone().unwrap_or_else(|| current.status.clone());
    if let Err(e) = validate_enum("status", &status, RELEASE_STATUSES) {
        return Ok(bad_request(e));
    }
    let target_date = match body.target_date {
        Some(value) => value,
        None => current.target_date,
    };
    let description_html = body
        .description_html
        .clone()
        .unwrap_or_else(|| current.description_html.clone());
    sqlx::query(
        "UPDATE releases SET name = $1, version = $2, description_html = $3, status = $4, \
         target_date = $5, updated_at = now(), updated_by_id = $6 \
         WHERE id = $7 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $8 AND deleted_at IS NULL) \
         AND deleted_at IS NULL",
    )
    .bind(&name)
    .bind(&version)
    .bind(&description_html)
    .bind(&status)
    .bind(target_date)
    .bind(auth.0)
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    let Some(row) = release_in_workspace(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    Ok((StatusCode::OK, Json(release_json(&row))))
}

/// DELETE `/api/workspaces/:slug/releases/:pk/`
pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let Some(current) = release_in_workspace(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    if current.created_by_id != Some(auth.0) && !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM review_requests WHERE release_id = $1 \
         AND status IN ('pending', 'scheduled') AND deleted_at IS NULL)",
    )
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if active {
        return Ok(bad_request("Release has an active review request"));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE release_changes SET deleted_at = now(), updated_at = now() \
         WHERE release_id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE releases SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

Tambahkan import di header `release.rs`:

```rust
use super::service::deserialize_present;
```

- [ ] **Step 5: Register `/releases/:pk/` routes**

In `apps/api-rs/crates/api/src/main.rs`, add to the releases block from Task 2 (after the `/releases/` route):

```rust
        .route(
            "/api/workspaces/:slug/releases/:pk/",
            get(routes::release::detail)
                .patch(routes::release::patch)
                .delete(routes::release::destroy),
        )
```

- [ ] **Step 6: Run tests to verify they pass**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test -- --test-threads=1
```

Expected: PASS — 6 tests: `create_release_defaults_and_sequence`, `create_release_validates_name_status_and_membership`, `list_releases_filters_status`, `detail_release_includes_empty_bundles`, `patch_release_updates_and_clears_optional_fields`, `destroy_release_guards_active_review_and_permissions`.

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/release.rs \
  apps/api-rs/crates/api/src/routes/service.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/release_review_test.rs
git commit -m "feat(api-rs): release detail/patch/delete with guards"
```

---

### Task 4: Release changes link/unlink

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/release.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Test: `apps/api-rs/crates/api/tests/release_review_test.rs`

- [ ] **Step 1: Update imports and write the failing tests**

In `apps/api-rs/crates/api/tests/release_review_test.rs`, change the import line to:

```rust
use api::routes::release::{
    changes_create, changes_destroy, changes_list, create, destroy, detail, list, patch,
    CreateRelease, LinkChanges, ListParams, PatchRelease,
};
```

Then append to the file:

```rust
#[tokio::test]
async fn detail_includes_linked_changes() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool).await;

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis dengan change")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);
    changes_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
        Json(LinkChanges {
            issue_ids: vec![issue_id],
        }),
    )
    .await
    .expect("link change");

    let (status, Json(body)) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("detail");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["changes"].as_array().unwrap().len(), 1);
    assert!(body["changes"][0]["issue_identifier"]
        .as_str()
        .unwrap()
        .ends_with("-1"));

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn link_changes_requires_same_workspace_and_dedupes() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let other = Scratch::new(&st.pool).await;
    let issue_a = scratch.insert_issue(&st.pool).await;
    let issue_b = scratch.insert_issue(&st.pool).await;
    let foreign_issue = other.insert_issue(&st.pool).await;

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis bundling")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);

    let (status, Json(body)) = changes_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
        Json(LinkChanges {
            issue_ids: vec![issue_a, issue_b, issue_a],
        }),
    )
    .await
    .expect("link changes");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body.as_array().unwrap().len(), 2);

    let (status, Json(body)) = changes_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
        Json(LinkChanges {
            issue_ids: vec![issue_a],
        }),
    )
    .await
    .expect("idempotent link");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body.as_array().unwrap().len(), 2);

    let (status, Json(body)) = changes_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
        Json(LinkChanges {
            issue_ids: vec![foreign_issue],
        }),
    )
    .await
    .expect("foreign issue");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid issue_ids - object does not exist.");

    scratch.cleanup(&st.pool).await;
    other.cleanup(&st.pool).await;
}

#[tokio::test]
async fn changes_destroy_unlinks() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool).await;

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis unlink")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);
    changes_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
        Json(LinkChanges {
            issue_ids: vec![issue_id],
        }),
    )
    .await
    .expect("link");

    let (status, _) = changes_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid, issue_id)),
    )
    .await
    .expect("unlink");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, Json(body)) = changes_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("list changes");
    assert_eq!(body.as_array().unwrap().len(), 0);

    let (status, _) = changes_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid, issue_id)),
    )
    .await
    .expect("idempotent unlink");
    assert_eq!(status, StatusCode::NO_CONTENT);

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd apps/api-rs && cargo test -p api --test release_review_test link_changes --no-run
```

Expected: FAIL to compile — `LinkChanges`, `changes_create`, `changes_destroy`, `changes_list` not found.

- [ ] **Step 3: Implement the changes handlers**

Append to `apps/api-rs/crates/api/src/routes/release.rs` (sebelum `#[cfg(test)] mod tests`):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct LinkChanges {
    pub issue_ids: Vec<Uuid>,
}

/// GET `/api/workspaces/:slug/releases/:pk/changes/`
pub async fn changes_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    if release_in_workspace(&st.pool, &slug, pk).await?.is_none() {
        return Ok(missing());
    }
    let rows: Vec<ReleaseChangeRow> = sqlx::query_as(&format!(
        "{CHANGE_SELECT} WHERE rc.release_id = $1 AND rc.deleted_at IS NULL ORDER BY rc.created_at ASC"
    ))
    .bind(pk)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(change_json).collect())),
    ))
}

/// POST `/api/workspaces/:slug/releases/:pk/changes/`
pub async fn changes_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
    Json(body): Json<LinkChanges>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let Some(release) = release_in_workspace(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    let mut ids = body.issue_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(bad_request("Invalid issue_ids - object does not exist."));
    }
    let valid: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues i JOIN projects p ON p.id = i.project_id \
         WHERE i.id = ANY($1) AND i.deleted_at IS NULL AND p.deleted_at IS NULL \
         AND p.workspace_id = $2",
    )
    .bind(&ids)
    .bind(release.workspace_id)
    .fetch_one(&st.pool)
    .await?;
    if valid as usize != ids.len() {
        return Ok(bad_request("Invalid issue_ids - object does not exist."));
    }
    for issue_id in &ids {
        sqlx::query(
            "INSERT INTO release_changes (id, workspace_id, release_id, issue_id, project_id, \
             created_at, updated_at, created_by_id, updated_by_id) \
             SELECT gen_random_uuid(), $1, $2, i.id, i.project_id, now(), now(), $3, $3 \
             FROM issues i JOIN projects p ON p.id = i.project_id \
             WHERE i.id = $4 AND p.workspace_id = $1 AND i.deleted_at IS NULL \
             AND NOT EXISTS (SELECT 1 FROM release_changes rc WHERE rc.release_id = $2 \
                             AND rc.issue_id = i.id AND rc.deleted_at IS NULL)",
        )
        .bind(release.workspace_id)
        .bind(pk)
        .bind(auth.0)
        .bind(issue_id)
        .execute(&st.pool)
        .await?;
    }
    let rows: Vec<ReleaseChangeRow> = sqlx::query_as(&format!(
        "{CHANGE_SELECT} WHERE rc.release_id = $1 AND rc.deleted_at IS NULL ORDER BY rc.created_at ASC"
    ))
    .bind(pk)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(Value::Array(rows.iter().map(change_json).collect())),
    ))
}

/// DELETE `/api/workspaces/:slug/releases/:pk/changes/:issue_id/`
pub async fn changes_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk, issue_id)): Path<(String, Uuid, Uuid)>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    sqlx::query(
        "UPDATE release_changes rc SET deleted_at = now(), updated_at = now() \
         FROM releases r JOIN workspaces w ON w.id = r.workspace_id \
         WHERE rc.release_id = r.id AND r.id = $1 AND w.slug = $2 \
         AND rc.issue_id = $3 AND rc.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .bind(issue_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 4: Register the changes routes**

In `apps/api-rs/crates/api/src/main.rs`, add to the releases block (after the `/releases/:pk/` route):

```rust
        .route(
            "/api/workspaces/:slug/releases/:pk/changes/",
            get(routes::release::changes_list).post(routes::release::changes_create),
        )
        .route(
            "/api/workspaces/:slug/releases/:pk/changes/:issue_id/",
            delete(routes::release::changes_destroy),
        )
```

- [ ] **Step 5: Run the full test file**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test -- --test-threads=1
```

Expected: PASS — 9 tests (`create_release_defaults_and_sequence`, `create_release_validates_name_status_and_membership`, `list_releases_filters_status`, `detail_release_includes_empty_bundles`, `patch_release_updates_and_clears_optional_fields`, `destroy_release_guards_active_review_and_permissions`, `detail_includes_linked_changes`, `link_changes_requires_same_workspace_and_dedupes`, `changes_destroy_unlinks`).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/release.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/release_review_test.rs
git commit -m "feat(api-rs): release change bundling endpoints"
```

---

### Task 5: Verification wrap-up

**Files:** none (verification only).

- [ ] **Step 1: Unit tests + route inventory**

Run:

```bash
cd apps/api-rs && cargo test -p api --lib release:: && cargo test -p api --test route_inventory_test
```

Expected: `release::tests` PASS (2 tests); `route_inventory_test` PASS (new fork routes tidak ada di inventory Django-parity — tidak menambah path yang diklaim).

- [ ] **Step 2: Format check**

Run:

```bash
cd apps/api-rs && cargo fmt --check
```

Expected: no diff. Jika ada, jalankan `cargo fmt` lalu ulangi test Task 5 Step 1.

- [ ] **Step 3: Rebuild + smoke test via HTTP**

Rebuild (detached) sesuai `AGENTS.md`, lalu verifikasi:

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
until curl -sf http://localhost:8000/health > /dev/null; do sleep 15; done
```

Expected: `200` dari `/health`. Smoke endpoint releases memerlukan cookie session; verifikasi cukup lewat test integrasi (Step 1) + boot migrasi (Task 1 Step 2).

- [ ] **Step 4: Commit (jika ada perbaikan format)**

```bash
git add -A apps/api-rs
git commit -m "chore(api-rs): fmt release module"
```

(Skip jika tidak ada perubahan.)

---

## Self-review notes

- **Spec coverage Phase 1:** tabel `releases` + `release_changes` (§Model data) ✔; lifecycle status release (validasi enum) ✔; API releases list/create/detail/patch/delete + changes list/link/unlink ✔; guard delete saat request aktif ✔; permission creator/admin untuk delete ✔; filter status/target_date ✔. Review requests/sessions/participants/items schema ✔ (dibuat di migration), API-nya Phase 2.
- **Phase 2 (plan terpisah):** `routes/review.rs` (submit/queue/withdraw, sessions, agenda, outcome, complete/cancel, participants), notifikasi 3 event, integration tests-nya, `GET /review-requests/` filter badges.
- **Phase 3 (web):** types/services/stores, halaman Releases + RCB + TCB, session detail, integrasi change detail, render notifikasi.
- **Urutan import test:** tiap task memperbarui baris `use api::routes::release::{...}` karena satu file test = satu unit kompilasi; jangan menambahkan referensi handler yang belum diimplementasikan ke file test pada task sebelumnya.
