# Workspace-Level Services — Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the service catalog from project-owned rows to a workspace-level catalog with owner/consumer project links, add cross-project dependency support with full/lite visibility, keep project-scoped endpoints working as a compat layer, and re-scope war rooms, intake, and AI tools to the same access rules.

**Architecture:** Additive Postgres migration (`service_project_links` + backfill + workspace-scoped name index, `project_id` columns made nullable for dual-write). New `service_access` module is the single source of truth for visibility and gates. `routes/service.rs` gains canonical workspace-level handlers plus compat wrappers for the existing project-scoped routes; war-room/intake/AI call sites adopt the access module.

**Tech Stack:** Rust (axum, sqlx, Postgres), sqlx migrations applied at API boot (`common::db::migrate`), DB-backed integration tests in `apps/api-rs/crates/api/tests/`.

**Spec:** `docs/superpowers/specs/2026-10-17-workspace-level-services-design.md` (locked decisions + access matrix — read it first).

---

## Global constraints (read before every task)

- **No Django changes.** Services schema is Rust-only (`apps/api-rs/migrations/`); do NOT touch `apps/api/plane/db/models/`.
- Role constants: admin=20, member=15, guest=5. `deny()` = 403 `{"error": "You don't have the required permissions."}`, `missing()` = 404 — from `crate::routes::project`.
- Handlers are called **directly** in integration tests (no HTTP server), like `tests/war_room_test.rs`.
- DB-backed tests: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1` (serial; scratch fixtures share the DB).
- The live `worker`/`beat-worker` containers sweep intake triage on the same DB. While running **intake** DB tests, stop them first (`docker stop plane-for-itsm-worker-1 plane-for-itsm-beat-worker-1`) and restart after (`docker start …`); otherwise `intake_triage_test` flakes when the worker claims a fixture row first.
- Commit after every task. Conventional style used in this repo: `feat(api): ...`, `test(api): ...`.
- The migration is idempotent (safe if re-applied by boot after manual `psql` execution): `CREATE ... IF NOT EXISTS`, backfill `ON CONFLICT DO NOTHING`, dedupe only touches duplicate rows.
- Keep existing error strings byte-identical on compat paths (`"A service with this name already exists."`, `"Invalid status"`, `"Source service not found."`, `"Target service not found."`, `"A service cannot depend on itself."`, `"This dependency already exists."`, `"This dependency would create a cycle."`).
- `cargo check -p api` must stay clean after every task; fix leftover references before committing.

## File structure (lock-in)

| File                                                     | Responsibility                                                                   |
| -------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `apps/api-rs/migrations/0020_workspace_services.sql`     | NEW — links table, backfill, dedupe, workspace name index, nullable `project_id` |
| `apps/api-rs/crates/api/src/routes/service_access.rs`    | NEW — visibility sets, full/lite checks, writer/admin gates, link helpers        |
| `apps/api-rs/crates/api/src/routes/service.rs`           | Reworked — shared row/JSON helpers, canonical handlers, compat wrappers          |
| `apps/api-rs/crates/api/src/routes/mod.rs`               | Add `pub mod service_access;`                                                    |
| `apps/api-rs/crates/api/src/main.rs`                     | Mount canonical workspace-level routes                                           |
| `apps/api-rs/crates/api/src/routes/war_room.rs`          | Attach gate: service must be linked to the war room's project                    |
| `apps/api-rs/crates/api/src/routes/intake.rs`            | Accept gate: linked service requirement                                          |
| `apps/api-rs/crates/ai/src/tools/services.rs`            | Visibility-scoped SQL (link membership / ws admin)                               |
| `apps/api-rs/crates/api/tests/service_workspace_test.rs` | NEW — full integration suite                                                     |

---

### Task 0: Preflight

**Files:** none (verification only)

- [ ] **Step 1: Confirm toolchain + DB**

Run: `cargo check -p api` (from `apps/api-rs`)
Expected: finishes without errors (warnings OK).

Run: `psql "${DATABASE_URL:-postgres://plane:plane@localhost:5432/plane}" -c "SELECT 1"`
Expected: `1`.

- [ ] **Step 2: Capture the current services schema**

Run:

```bash
DB="${DATABASE_URL:-postgres://plane:plane@localhost:5432/plane}"
psql "$DB" -c "\d services" && psql "$DB" -c "\d service_dependencies"
```

Expected: `services.project_id` NOT NULL; `service_dependencies.project_id` NOT NULL. Note the exact constraint/index names.

- [ ] **Step 3: Baseline test run (serial)**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test war_room_test -- --test-threads=1`
Expected: all pass. If the DB is missing schema, boot the local API once per `AGENTS.md` first.

---

### Task 1: Migration `0020_workspace_services.sql`

**Files:**

- Create: `apps/api-rs/migrations/0020_workspace_services.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Workspace-level services (design 2026-10-17): project links + workspace-scoped
-- name uniqueness. Phase 1 is additive: legacy project_id columns stay (made
-- nullable, dual-written by the API) until the cleanup release.

CREATE TABLE IF NOT EXISTS public.service_project_links (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    link_type character varying(20) NOT NULL CHECK (link_type IN ('owner', 'consumer')),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS service_project_links_service_project_idx
    ON public.service_project_links (service_id, project_id) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS service_project_links_one_owner_idx
    ON public.service_project_links (service_id) WHERE link_type = 'owner' AND deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS service_project_links_project_idx
    ON public.service_project_links (project_id) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS service_project_links_service_idx
    ON public.service_project_links (service_id) WHERE deleted_at IS NULL;

-- Backfill: every existing service's project becomes its owner link.
INSERT INTO public.service_project_links
    (id, workspace_id, service_id, project_id, link_type, created_at, updated_at)
SELECT gen_random_uuid(), s.workspace_id, s.id, s.project_id, 'owner', now(), now()
FROM public.services s
WHERE s.deleted_at IS NULL
ON CONFLICT DO NOTHING;

-- Dedupe pass 1: workspace duplicate names -> "Name (PROJECT-IDENTIFIER)".
WITH ranked AS (
    SELECT s.id, s.name, p.identifier,
           row_number() OVER (
             PARTITION BY s.workspace_id, lower(btrim(s.name))
             ORDER BY s.created_at, s.id
           ) AS rn
    FROM public.services s
    JOIN public.projects p ON p.id = s.project_id
    WHERE s.deleted_at IS NULL
)
UPDATE public.services s
SET name = r.name || ' (' || r.identifier || ')', updated_at = now()
FROM ranked r
WHERE s.id = r.id AND r.rn > 1;

-- Dedupe pass 2: residual collisions -> append a short id fragment.
WITH ranked AS (
    SELECT s.id, s.name, p.identifier,
           row_number() OVER (
             PARTITION BY s.workspace_id, lower(btrim(s.name))
             ORDER BY s.created_at, s.id
           ) AS rn
    FROM public.services s
    JOIN public.projects p ON p.id = s.project_id
    WHERE s.deleted_at IS NULL
)
UPDATE public.services s
SET name = r.name || ' (' || r.identifier || '-' || left(s.id::text, 8) || ')', updated_at = now()
FROM ranked r
WHERE s.id = r.id AND r.rn > 1;

CREATE UNIQUE INDEX IF NOT EXISTS services_unique_name_workspace_idx
    ON public.services (workspace_id, lower(btrim(name))) WHERE deleted_at IS NULL;

ALTER TABLE public.services ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE public.service_dependencies ALTER COLUMN project_id DROP NOT NULL;
```

- [ ] **Step 2: Apply and verify**

Run:

```bash
cd apps/api-rs
DB="${DATABASE_URL:-postgres://plane:plane@localhost:5432/plane}"
psql "$DB" -f migrations/0020_workspace_services.sql
psql "$DB" -c "\d service_project_links"
psql "$DB" -c "SELECT
  (SELECT count(*) FROM service_project_links WHERE link_type='owner' AND deleted_at IS NULL) AS owner_links,
  (SELECT count(*) FROM services WHERE deleted_at IS NULL) AS live_services;"
psql "$DB" -c "SELECT indexname FROM pg_indexes WHERE tablename='services' AND indexname='services_unique_name_workspace_idx';"
```

Expected: owner_links == live_services; the workspace index exists; no errors. Re-running the file must also succeed (idempotency check).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/migrations/0020_workspace_services.sql
git commit -m "feat(api): workspace-level services migration (project links)"
```

### Task 2: `service_access` module + integration test harness

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/service_access.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs` (add `pub mod service_access;` next to `pub mod service;`)
- Create: `apps/api-rs/crates/api/tests/service_workspace_test.rs`

- [ ] **Step 1: Write the module skeleton with stubs (so tests compile and fail)**

`apps/api-rs/crates/api/src/routes/service_access.rs`:

```rust
//! Workspace-level service access (design 2026-10-17): visibility sets and
//! role gates shared by services, war rooms, intake, and AI tooling.

use uuid::Uuid;

pub async fn full_visible(
    _pool: &sqlx::PgPool,
    _user_id: Uuid,
    _slug: &str,
    _service_id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(false)
}

pub async fn visible_service_ids(
    _pool: &sqlx::PgPool,
    _user_id: Uuid,
    _slug: &str,
) -> Result<Vec<Uuid>, sqlx::Error> {
    Ok(vec![])
}

pub async fn gate_writer(
    _pool: &sqlx::PgPool,
    _user_id: Uuid,
    _slug: &str,
    _service_id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(false)
}

pub async fn gate_admin(
    _pool: &sqlx::PgPool,
    _user_id: Uuid,
    _slug: &str,
    _service_id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(false)
}

pub async fn owner_project(
    _pool: &sqlx::PgPool,
    _service_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    Ok(None)
}

pub async fn linked_to_project(
    _pool: &sqlx::PgPool,
    _service_id: Uuid,
    _project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(false)
}

/// Pure project-admin check (does not include the ws-admin fallback).
pub async fn is_project_admin(
    _pool: &sqlx::PgPool,
    _user_id: Uuid,
    _slug: &str,
    _project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(false)
}
```

- [ ] **Step 2: Write the failing test harness + first tests**

`apps/api-rs/crates/api/tests/service_workspace_test.rs` (full file; later tasks append test fns):

```rust
//! Workspace-level services backend integration tests (design 2026-10-17).
//! DB-backed; run serially:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test service_workspace_test -- --test-threads=1`

use api::middleware::auth::AuthUser;
use api::state::AppState;
use axum::http::StatusCode;
use common::config::AppConfig;
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

async fn insert_project(pool: &PgPool, project_id: Uuid, workspace_id: Uuid, identifier: &str) {
    sqlx::query(
        "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
         identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
         intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
         is_issue_type_enabled, guest_view_all_features, timezone) \
         VALUES ($1, now(), now(), 'Scratch', '', 2, $2, $3, false, false, false, \
         false, false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
    )
    .bind(project_id)
    .bind(identifier)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch project");
}

async fn insert_service(pool: &PgPool, workspace_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO services (id, workspace_id, name, description, description_html, status, \
         criticality, \"type\", sort_order, created_at, updated_at) \
         VALUES ($1, $2, $3, '', '', 'active', 'medium', 'internal', 65535, now(), now())",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("scratch service");
    id
}

async fn link_service(
    pool: &PgPool,
    service_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    link_type: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_project_links (id, workspace_id, service_id, project_id, link_type, \
         created_at, updated_at) VALUES ($1, $2, $3, $4, $5, now(), now())",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(service_id)
    .bind(project_id)
    .bind(link_type)
    .execute(pool)
    .await
    .expect("scratch service link");
    id
}

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    p1: Uuid,
    p2: Uuid,
    p4: Uuid,
    admin: Uuid,
    owner_admin: Uuid,
    owner_member: Uuid,
    consumer_admin: Uuid,
    guest: Uuid,
    outsider: Uuid,
    s1: Uuid, // owned by P1, consumer link P2
    s2: Uuid, // owned by P2
    s4: Uuid, // owned by P4
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("svc-{}", Uuid::new_v4().simple());
        let workspace_id = Uuid::new_v4();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        let p4 = Uuid::new_v4();
        let admin = Uuid::new_v4();
        let owner_admin = Uuid::new_v4();
        let owner_member = Uuid::new_v4();
        let consumer_admin = Uuid::new_v4();
        let guest = Uuid::new_v4();
        let outsider = Uuid::new_v4();

        for (id, name) in [
            (admin, "admin"),
            (owner_admin, "owner-admin"),
            (owner_member, "owner-member"),
            (consumer_admin, "consumer-admin"),
            (guest, "guest"),
            (outsider, "outsider"),
        ] {
            insert_user(pool, id, &format!("{slug}-{name}")).await;
        }

        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Svc Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin)
        .execute(pool)
        .await
        .expect("scratch workspace");

        // Workspace roles: admin = ws admin (no project rows); owner/consumer/outsider
        // are ws members (15); guest is ws guest (5).
        insert_workspace_member(pool, admin, workspace_id, 20).await;
        for user in [owner_admin, owner_member, consumer_admin, outsider] {
            insert_workspace_member(pool, user, workspace_id, 15).await;
        }
        insert_workspace_member(pool, guest, workspace_id, 5).await;

        for (project_id, ident) in [(p1, "P1"), (p2, "P2"), (p4, "P4")] {
            insert_project(pool, project_id, workspace_id, ident).await;
        }
        insert_project_member(pool, owner_admin, p1, workspace_id, 20).await;
        insert_project_member(pool, owner_member, p1, workspace_id, 15).await;
        insert_project_member(pool, guest, p1, workspace_id, 5).await;
        insert_project_member(pool, consumer_admin, p2, workspace_id, 20).await;
        insert_project_member(pool, outsider, p4, workspace_id, 15).await;

        let s1 = insert_service(pool, workspace_id, "ISO Switch").await;
        link_service(pool, s1, p1, workspace_id, "owner").await;
        link_service(pool, s1, p2, workspace_id, "consumer").await;
        let s2 = insert_service(pool, workspace_id, "Core Banking").await;
        link_service(pool, s2, p2, workspace_id, "owner").await;
        let s4 = insert_service(pool, workspace_id, "P4 Widget").await;
        link_service(pool, s4, p4, workspace_id, "owner").await;

        Self {
            slug,
            workspace_id,
            p1,
            p2,
            p4,
            admin,
            owner_admin,
            owner_member,
            consumer_admin,
            guest,
            outsider,
            s1,
            s2,
            s4,
        }
    }
}

#[tokio::test]
async fn visibility_matrix_and_gates() {
    use api::routes::service_access::{
        full_visible, gate_admin, gate_writer, linked_to_project, visible_service_ids,
    };

    let pool = pool().await;
    let sc = Scratch::new(&pool).await;

    let mut admin = visible_service_ids(&pool, sc.admin, &sc.slug).await.unwrap();
    admin.sort();
    let mut all = vec![sc.s1, sc.s2, sc.s4];
    all.sort();
    assert_eq!(admin, all, "ws admin sees every service");

    assert_eq!(
        visible_service_ids(&pool, sc.owner_member, &sc.slug).await.unwrap(),
        vec![sc.s1],
        "P1 member sees only S1"
    );
    let mut consumer = visible_service_ids(&pool, sc.consumer_admin, &sc.slug).await.unwrap();
    consumer.sort();
    let mut consumer_expected = vec![sc.s1, sc.s2];
    consumer_expected.sort();
    assert_eq!(consumer, consumer_expected, "P2 admin sees S1 (consumer) + S2 (owner)");
    assert_eq!(
        visible_service_ids(&pool, sc.guest, &sc.slug).await.unwrap(),
        vec![sc.s1],
        "guest sees linked services read-only"
    );
    assert_eq!(
        visible_service_ids(&pool, sc.outsider, &sc.slug).await.unwrap(),
        vec![sc.s4],
        "outsider sees only P4"
    );

    assert!(full_visible(&pool, sc.admin, &sc.slug, sc.s4).await.unwrap());
    assert!(full_visible(&pool, sc.owner_member, &sc.slug, sc.s1).await.unwrap());
    assert!(full_visible(&pool, sc.consumer_admin, &sc.slug, sc.s1).await.unwrap());
    assert!(!full_visible(&pool, sc.outsider, &sc.slug, sc.s1).await.unwrap());
    assert!(!full_visible(&pool, sc.guest, &sc.slug, sc.s2).await.unwrap());

    assert!(gate_writer(&pool, sc.owner_member, &sc.slug, sc.s1).await.unwrap());
    assert!(gate_writer(&pool, sc.admin, &sc.slug, sc.s1).await.unwrap());
    assert!(!gate_writer(&pool, sc.consumer_admin, &sc.slug, sc.s1).await.unwrap());
    assert!(!gate_writer(&pool, sc.guest, &sc.slug, sc.s1).await.unwrap());
    assert!(!gate_writer(&pool, sc.outsider, &sc.slug, sc.s1).await.unwrap());

    assert!(gate_admin(&pool, sc.owner_admin, &sc.slug, sc.s1).await.unwrap());
    assert!(gate_admin(&pool, sc.admin, &sc.slug, sc.s1).await.unwrap());
    assert!(!gate_admin(&pool, sc.owner_member, &sc.slug, sc.s1).await.unwrap());
    assert!(!gate_admin(&pool, sc.consumer_admin, &sc.slug, sc.s1).await.unwrap());

    assert!(linked_to_project(&pool, sc.s1, sc.p2).await.unwrap());
    assert!(!linked_to_project(&pool, sc.s1, sc.p4).await.unwrap());
}

#[tokio::test]
async fn full_visible_rejects_inactive_membership() {
    use api::routes::service_access::full_visible;

    let pool = pool().await;
    let sc = Scratch::new(&pool).await;

    sqlx::query("UPDATE project_members SET is_active = false WHERE member_id = $1 AND project_id = $2")
        .bind(sc.owner_member)
        .bind(sc.p1)
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        !full_visible(&pool, sc.owner_member, &sc.slug, sc.s1).await.unwrap(),
        "inactive project membership grants no visibility"
    );
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1`
Expected: FAIL — assertions fail (stubs return empty/false); compile succeeds.

- [ ] **Step 4: Implement `service_access` for real (replace stub bodies)**

```rust
//! Workspace-level service access (design 2026-10-17): visibility sets and
//! role gates shared by services, war rooms, intake, and AI tooling.

use uuid::Uuid;

use super::issue_common::{fetch_project_member_role, is_workspace_admin};

/// Full-visibility: ws admin, or active member of a project linked
/// (owner or consumer) to the service.
pub async fn full_visible(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    service_id: Uuid,
) -> Result<bool, sqlx::Error> {
    if is_workspace_admin(pool, user_id, slug).await? {
        return Ok(true);
    }
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_project_links spl \
          JOIN projects p ON p.id = spl.project_id AND p.deleted_at IS NULL \
          JOIN project_members pm ON pm.project_id = spl.project_id AND pm.member_id = $2 \
            AND pm.is_active = true AND pm.deleted_at IS NULL \
          JOIN workspace_members wm ON wm.workspace_id = spl.workspace_id AND wm.member_id = $2 \
            AND wm.is_active = true AND wm.deleted_at IS NULL \
          JOIN workspaces w ON w.id = spl.workspace_id AND w.slug = $3 \
          JOIN services sv ON sv.id = spl.service_id AND sv.deleted_at IS NULL \
          WHERE spl.service_id = $1 AND spl.deleted_at IS NULL)",
    )
    .bind(service_id)
    .bind(user_id)
    .bind(slug)
    .fetch_one(pool)
    .await
}

/// All service ids the caller sees in full.
pub async fn visible_service_ids(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if is_workspace_admin(pool, user_id, slug).await? {
        return sqlx::query_scalar(
            "SELECT sv.id FROM services sv \
              JOIN workspaces w ON w.id = sv.workspace_id \
              WHERE w.slug = $1 AND sv.deleted_at IS NULL ORDER BY sv.id",
        )
        .bind(slug)
        .fetch_all(pool)
        .await;
    }
    sqlx::query_scalar(
        "SELECT DISTINCT spl.service_id FROM service_project_links spl \
          JOIN projects p ON p.id = spl.project_id AND p.deleted_at IS NULL \
          JOIN project_members pm ON pm.project_id = spl.project_id AND pm.member_id = $2 \
            AND pm.is_active = true AND pm.deleted_at IS NULL \
          JOIN workspace_members wm ON wm.workspace_id = spl.workspace_id AND wm.member_id = $2 \
            AND wm.is_active = true AND wm.deleted_at IS NULL \
          JOIN workspaces w ON w.id = spl.workspace_id AND w.slug = $3 \
          JOIN services sv ON sv.id = spl.service_id AND sv.deleted_at IS NULL \
          WHERE spl.deleted_at IS NULL ORDER BY spl.service_id",
    )
    .bind(user_id)
    .bind(slug)
    .fetch_all(pool)
    .await
}

async fn project_role(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<Option<i16>, sqlx::Error> {
    fetch_project_member_role(pool, user_id, slug, project_id).await
}

/// Writer: ws admin, or member+ (15/20) of the owner project.
pub async fn gate_writer(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    service_id: Uuid,
) -> Result<bool, sqlx::Error> {
    if is_workspace_admin(pool, user_id, slug).await? {
        return Ok(true);
    }
    let Some(owner) = owner_project(pool, service_id).await? else {
        return Ok(false);
    };
    Ok(matches!(project_role(pool, user_id, slug, owner).await?, Some(20) | Some(15)))
}

/// Admin: ws admin, or project admin (20) of the owner project.
pub async fn gate_admin(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    service_id: Uuid,
) -> Result<bool, sqlx::Error> {
    if is_workspace_admin(pool, user_id, slug).await? {
        return Ok(true);
    }
    let Some(owner) = owner_project(pool, service_id).await? else {
        return Ok(false);
    };
    Ok(matches!(project_role(pool, user_id, slug, owner).await?, Some(20)))
}

pub async fn owner_project(
    pool: &sqlx::PgPool,
    service_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT spl.project_id FROM service_project_links spl \
          JOIN services sv ON sv.id = spl.service_id AND sv.deleted_at IS NULL \
          WHERE spl.service_id = $1 AND spl.link_type = 'owner' AND spl.deleted_at IS NULL",
    )
    .bind(service_id)
    .fetch_optional(pool)
    .await
}

pub async fn linked_to_project(
    pool: &sqlx::PgPool,
    service_id: Uuid,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_project_links spl \
          JOIN services sv ON sv.id = spl.service_id AND sv.deleted_at IS NULL \
          WHERE spl.service_id = $1 AND spl.project_id = $2 AND spl.deleted_at IS NULL)",
    )
    .bind(service_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
}

/// Pure project-admin check (does not include the ws-admin fallback).
pub async fn is_project_admin(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(matches!(project_role(pool, user_id, slug, project_id).await?, Some(20)))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1`
Expected: `visibility_matrix_and_gates`, `full_visible_rejects_inactive_membership` PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service_access.rs \
  apps/api-rs/crates/api/src/routes/mod.rs \
  apps/api-rs/crates/api/tests/service_workspace_test.rs
git commit -m "feat(api): service access module (visibility + gates)"
```

### Task 3: Canonical list/detail + shared JSON refactor

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs`

Context: `ServiceRow` currently selects `s.project_id` (lines 74-100) and `service_json(row)` (line 102) is called by `list`, `create`, `detail`, `patch`. This task replaces those shared pieces with link-derived JSON and adds the canonical handlers; all call sites in this file must be adapted in the same task.

- [ ] **Step 1: Add failing tests**

Append to `service_workspace_test.rs`:

```rust
#[tokio::test]
async fn canonical_list_scopes_and_filters() {
    use api::routes::service::{list_workspace, WorkspaceServiceListParams};
    use axum::extract::{Path, Query, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let (status, axum::Json(body)) = list_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path(sc.slug.clone()),
        Query(WorkspaceServiceListParams::default()),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1, "outsider sees only P4-owned service");
    assert_eq!(rows[0]["name"], "P4 Widget");
    assert_eq!(rows[0]["owner_project_id"], sc.p4.to_string());
    assert_eq!(rows[0]["project_id"], sc.p4.to_string(), "deprecated alias");
    assert_eq!(rows[0]["access_level"], "full");

    let (_, axum::Json(body)) = list_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path(sc.slug.clone()),
        Query(WorkspaceServiceListParams::default()),
    )
    .await
    .unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 3, "ws admin sees all");
    let s1 = rows.iter().find(|r| r["id"] == sc.s1.to_string()).unwrap();
    assert_eq!(s1["project_links"].as_array().unwrap().len(), 2);

    let (_, axum::Json(body)) = list_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path(sc.slug.clone()),
        Query(WorkspaceServiceListParams {
            project: Some(sc.p2),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 2, "P2 links: S1 (consumer) + S2 (owner)");

    let (_, axum::Json(body)) = list_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path(sc.slug.clone()),
        Query(WorkspaceServiceListParams {
            project: Some(sc.p2),
            link: Some("owner".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], sc.s2.to_string());
}

#[tokio::test]
async fn canonical_detail_is_full_only() {
    use api::routes::service::detail_workspace;
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let (status, _) = detail_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::NOT_FOUND, "outsider cannot read S1 (404, not 403)");

    let (status, axum::Json(body)) = detail_workspace(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.s1)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["access_level"], "full");
    assert_eq!(body["owner_project_id"], sc.p1.to_string());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test canonical_ -- --test-threads=1`
Expected: compile error (`list_workspace` not found) — that is the failing state.

- [ ] **Step 3: Implement the refactor + canonical handlers**

3a. Replace `ServiceRow`/`SERVICE_SELECT`/`service_json` (currently lines 74-123) with:

```rust
/// Full `services` row (no `project_id`: ownership is link-derived).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: String,
    pub description_html: String,
    pub status: String,
    pub criticality: String,
    pub service_type: String,
    pub owner_id: Option<Uuid>,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
    pub position: Option<serde_json::Value>,
    pub sort_order: f64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

const SERVICE_SELECT: &str = "SELECT s.id, s.workspace_id, s.name, s.description, \
    s.description_html, s.status, s.criticality, s.\"type\" AS service_type, s.owner_id, \
    s.repository_url, s.documentation_url, s.position, s.sort_order, s.created_at, s.updated_at, \
    s.created_by_id, s.updated_by_id FROM services s";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceLinkRow {
    pub id: Uuid,
    pub service_id: Uuid,
    pub project_id: Uuid,
    pub link_type: String,
}

pub async fn links_for_services(
    pool: &sqlx::PgPool,
    ids: &[Uuid],
) -> Result<Vec<ServiceLinkRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    sqlx::query_as(
        "SELECT id, service_id, project_id, link_type FROM service_project_links \
         WHERE service_id = ANY($1) AND deleted_at IS NULL ORDER BY link_type, created_at",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
}

pub fn owner_project_of(links: &[ServiceLinkRow]) -> Option<Uuid> {
    links
        .iter()
        .find(|l| l.link_type == "owner")
        .map(|l| l.project_id)
}

/// Full service JSON. `project_id` is retained (deprecated) = owner project id.
pub fn service_json(row: &ServiceRow, links: &[ServiceLinkRow], access_level: &str) -> Value {
    let owner_project_id = owner_project_of(links);
    let project_links: Vec<Value> = links
        .iter()
        .map(|l| {
            json!({
                "id": l.id,
                "project_id": l.project_id,
                "link_type": l.link_type,
            })
        })
        .collect();
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "project_id": owner_project_id,
        "owner_project_id": owner_project_id,
        "project_links": project_links,
        "access_level": access_level,
        "name": row.name,
        "description": row.description,
        "description_html": row.description_html,
        "status": row.status,
        "criticality": row.criticality,
        "type": row.service_type,
        "owner_id": row.owner_id,
        "repository_url": row.repository_url,
        "documentation_url": row.documentation_url,
        "position": row.position,
        "sort_order": row.sort_order,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": row.created_by_id,
        "updated_by": row.updated_by_id,
    })
}

/// Lite projection used for dependency endpoints outside the caller's
/// full-visibility set (design §3).
pub fn lite_service_json(id: Uuid, name: &str, status: &str, criticality: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "status": status,
        "criticality": criticality,
        "access_level": "lite",
    })
}

pub(crate) async fn service_json_by_id(
    pool: &sqlx::PgPool,
    service_id: Uuid,
    access_level: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<ServiceRow> = sqlx::query_as(&format!("{SERVICE_SELECT} WHERE s.id = $1"))
        .bind(service_id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(row) => {
            let links = links_for_services(pool, &[row.id]).await?;
            Ok(Some(service_json(&row, &links, access_level)))
        }
        None => Ok(None),
    }
}

/// Shared list serializer: fetch links once, group per service.
pub(crate) async fn json_service_list(
    pool: &sqlx::PgPool,
    rows: &[ServiceRow],
    access_level: &str,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let links = links_for_services(pool, &ids).await?;
    let payload: Vec<Value> = rows
        .iter()
        .map(|row| {
            let row_links: Vec<ServiceLinkRow> = links
                .iter()
                .filter(|l| l.service_id == row.id)
                .cloned()
                .collect();
            service_json(row, &row_links, access_level)
        })
        .collect();
    Ok((StatusCode::OK, Json(Value::Array(payload))))
}
```

3b. Adapt the old handlers in the same file:

- `list` (old): replace the final `Ok((StatusCode::OK, Json(Value::Array(rows.iter().map(service_json).collect()))))` with `json_service_list(&st.pool, &rows, "full").await`.
- `detail` (old): replace the row fetch + tail with:

```rust
    let Some(row) = service_json_by_id(&st.pool, pk, "full").await? else {
        return Ok(missing());
    };
    Ok((StatusCode::OK, Json(row)))
```

- `create` (old): the row is fetched after insert; replace `Json(service_json(&row))` with:

```rust
    let links = links_for_services(&st.pool, &[id]).await?;
    Ok((StatusCode::CREATED, Json(service_json(&row, &links, "full"))))
```

- `patch` (old): same pattern after the UPDATE (`pk` is the id):

```rust
    let links = links_for_services(&st.pool, &[pk]).await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(service_json(&r, &links, "full")))),
        None => Ok(missing()),
    }
```

- `destroy` (old): unchanged for now (no JSON payload).
- Remove any `SELECT ... s.project_id` fragments that no longer compile (`cargo check -p api` will point at them).

3c. Add canonical handlers + params (same file):

```rust
#[derive(Debug, Deserialize, Default)]
pub struct WorkspaceServiceListParams {
    pub project: Option<Uuid>,
    pub q: Option<String>,
    pub status: Option<String>,
    pub criticality: Option<String>,
    pub link: Option<String>,
}

pub async fn list_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(params): Query<WorkspaceServiceListParams>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let visible: Option<Vec<Uuid>> = if is_workspace_admin(&st.pool, auth.0, &slug).await? {
        None
    } else {
        Some(service_access::visible_service_ids(&st.pool, auth.0, &slug).await?)
    };
    let rows: Vec<ServiceRow> = sqlx::query_as(&format!(
        "{SERVICE_SELECT} \
         WHERE s.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $1) \
         AND s.deleted_at IS NULL \
         AND ($2::uuid[] IS NULL OR s.id = ANY($2)) \
         AND ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM service_project_links spl \
               WHERE spl.service_id = s.id AND spl.project_id = $3 AND spl.deleted_at IS NULL \
               AND ($4::text IS NULL OR spl.link_type = $4))) \
         AND ($5::text IS NULL OR s.name ILIKE '%' || $5 || '%') \
         AND ($6::text IS NULL OR s.status = $6) \
         AND ($7::text IS NULL OR s.criticality = $7) \
         ORDER BY s.sort_order, s.name"
    ))
    .bind(&slug)
    .bind(visible)
    .bind(params.project)
    .bind(params.link)
    .bind(params.q)
    .bind(params.status)
    .bind(params.criticality)
    .fetch_all(&st.pool)
    .await?;
    json_service_list(&st.pool, &rows, "full").await
}

pub async fn detail_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !service_access::full_visible(&st.pool, auth.0, &slug, pk).await? {
        return Ok(missing());
    }
    match service_json_by_id(&st.pool, pk, "full").await? {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok(missing()),
    }
}
```

3d. Add imports at the top of `service.rs`: `use axum::extract::Query;` and `use super::service_access;` (place next to the existing `use super::issue_common::...` line).

- [ ] **Step 4: Run tests + compile check**

Run: `cd apps/api-rs && cargo check -p api`
Expected: clean (fix leftover `project_id` references it reports).

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service.rs
git commit -m "feat(api): canonical workspace service list/detail + link-aware JSON"
```

---

### Task 4: Canonical create/patch/destroy

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs`

- [ ] **Step 1: Add failing tests**

Append to `service_workspace_test.rs`:

```rust
#[tokio::test]
async fn canonical_create_patch_destroy() {
    use api::routes::service::{
        create_workspace, destroy_workspace, detail_workspace, patch_workspace, CreateService,
        CreateWorkspaceService, PatchService,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let new_service = |name: &str| CreateWorkspaceService {
        owner_project_id: sc.p1,
        service: CreateService {
            name: Some(name.into()),
            description: None,
            description_html: None,
            status: None,
            criticality: None,
            service_type: None,
            owner_id: None,
            repository_url: None,
            documentation_url: None,
        },
    };

    // outsider cannot create in P1
    let denied = create_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path(sc.slug.clone()),
        Json(new_service("Nope")),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::FORBIDDEN);

    // P1 member creates; owner link is created atomically
    let (status, Json(created)) = create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(new_service("Payments API")),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["owner_project_id"], sc.p1.to_string());
    let new_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    assert!(
        api::routes::service_access::linked_to_project(&st.pool, new_id, sc.p1)
            .await
            .unwrap(),
        "owner link exists"
    );

    // duplicate name inside the workspace (renamed case) is rejected
    let (status, Json(dup)) = create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(new_service("payments api")),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(dup["error"], "A service with this name already exists.");

    // writer patch; consumer admin cannot
    let consumer_patch = patch_workspace(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), new_id)),
        Json(PatchService {
            status: Some("maintenance".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(consumer_patch.0, StatusCode::FORBIDDEN);

    let (status, Json(patched)) = patch_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), new_id)),
        Json(PatchService {
            status: Some("maintenance".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["status"], "maintenance");

    // delete: member+ denied, project admin allowed
    let member_delete = destroy_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), new_id)),
    )
    .await
    .unwrap();
    assert_eq!(member_delete.0, StatusCode::FORBIDDEN);

    let admin_delete = destroy_workspace(
        State(st.clone()),
        AuthUser(sc.owner_admin),
        Path((sc.slug.clone(), new_id)),
    )
    .await
    .unwrap();
    assert_eq!(admin_delete.0, StatusCode::NO_CONTENT);

    let gone = detail_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path((sc.slug.clone(), new_id)),
    )
    .await
    .unwrap();
    assert_eq!(gone.0, StatusCode::NOT_FOUND, "soft-deleted service is invisible");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test canonical_create -- --test-threads=1`
Expected: compile error (`create_workspace` not found).

- [ ] **Step 3: Implement**

3a. New body structs + shared core (place near `CreateService`):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkspaceService {
    pub owner_project_id: Uuid,
    #[serde(flatten)]
    pub service: CreateService,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransferOwner {
    pub project_id: Uuid,
}

/// Shared create used by canonical + compat handlers. Gates are enforced by
/// the callers; `owner_project_id` is the owner project.
pub(crate) async fn create_service_core(
    st: &AppState,
    user_id: Uuid,
    owner_project_id: Uuid,
    body: &CreateService,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let name = body
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Untitled service".to_string());
    let status = body.status.clone().unwrap_or_else(|| "planned".to_string());
    let criticality = body.criticality.clone().unwrap_or_else(|| "medium".to_string());
    let service_type = body.service_type.clone().unwrap_or_else(|| "internal".to_string());
    if let Err(e) = validate_enum("status", &status, SERVICE_STATUSES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("criticality", &criticality, SERVICE_CRITICALITIES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("type", &service_type, SERVICE_TYPES) {
        return Ok(bad_request(e));
    }
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services s \
         WHERE s.workspace_id = (SELECT p.workspace_id FROM projects p WHERE p.id = $1) \
         AND lower(btrim(s.name)) = lower(btrim($2)) AND s.deleted_at IS NULL)",
    )
    .bind(owner_project_id)
    .bind(&name)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok(bad_request("A service with this name already exists."));
    }
    if let Some(owner) = body.owner_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(owner)
            .fetch_one(&st.pool)
            .await?;
        if !exists {
            return Ok(bad_request(format!(
                "Invalid owner_id \"{owner}\" - object does not exist."
            )));
        }
    }
    let max_order: f64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sv.sort_order), 0) FROM services sv \
         JOIN service_project_links spl ON spl.service_id = sv.id \
           AND spl.project_id = $1 AND spl.deleted_at IS NULL \
         WHERE sv.deleted_at IS NULL",
    )
    .bind(owner_project_id)
    .fetch_one(&st.pool)
    .await?;
    let id = Uuid::new_v4();
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "INSERT INTO services (id, workspace_id, project_id, name, description, description_html, \
         status, criticality, \"type\", owner_id, repository_url, documentation_url, position, \
         sort_order, created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, p.workspace_id, p.id, $2, $3, $4, $5, $6, $7, $8, $9, $10, NULL, $11, \
         now(), now(), $12, $12 FROM projects p WHERE p.id = $13",
    )
    .bind(id)
    .bind(&name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.description_html.clone().unwrap_or_default())
    .bind(&status)
    .bind(&criticality)
    .bind(&service_type)
    .bind(body.owner_id)
    .bind(body.repository_url.clone())
    .bind(body.documentation_url.clone())
    .bind(next_sort_order(max_order))
    .bind(user_id)
    .bind(owner_project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO service_project_links (id, workspace_id, service_id, project_id, link_type, \
         created_at, updated_at, created_by_id, updated_by_id) \
         SELECT gen_random_uuid(), s.workspace_id, s.id, $1, 'owner', now(), now(), $2, $2 \
         FROM services s WHERE s.id = $3",
    )
    .bind(owner_project_id)
    .bind(user_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let json = service_json_by_id(&st.pool, id, "full")
        .await?
        .ok_or_else(|| common::errors::AppError::from(sqlx::Error::RowNotFound()))?;
    Ok((StatusCode::CREATED, Json(json)))
}

pub async fn create_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<CreateWorkspaceService>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let owner_project_id = body.owner_project_id;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, owner_project_id).await?;
    if !(ws_admin || matches!(role, Some(20) | Some(15))) {
        return Ok(deny());
    }
    let project_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL)",
    )
    .bind(owner_project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    if !project_ok {
        return Ok(bad_request("Invalid owner_project_id - object does not exist."));
    }
    create_service_core(&st, auth.0, owner_project_id, &body.service).await
}
```

Note: if `sqlx::Error::RowNotFound()` is not a fn in this sqlx version, use `sqlx::Error::RowNotFound` (no parens) — match the sqlx version used by the crate.

3b. `patch_workspace` + shared core:

```rust
pub async fn patch_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
    Json(body): Json<PatchService>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !service_access::gate_writer(&st.pool, auth.0, &slug, pk).await? {
        return Ok(deny());
    }
    patch_service_core(&st, auth.0, pk, &body).await
}

/// Shared update used by canonical + compat handlers (gate first).
pub(crate) async fn patch_service_core(
    st: &AppState,
    user_id: Uuid,
    pk: Uuid,
    body: &PatchService,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let current: Option<ServiceRow> = sqlx::query_as(&format!(
        "{SERVICE_SELECT} WHERE s.id = $1 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(current) = current else {
        return Ok(missing());
    };
    let name = match body.name.clone() {
        Some(v) => {
            let trimmed = v.trim().to_string();
            if trimmed.is_empty() { current.name.clone() } else { trimmed }
        }
        None => current.name.clone(),
    };
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services s \
         WHERE s.workspace_id = $1 AND lower(btrim(s.name)) = lower(btrim($2)) \
         AND s.id <> $3 AND s.deleted_at IS NULL)",
    )
    .bind(current.workspace_id)
    .bind(&name)
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok(bad_request("A service with this name already exists."));
    }
    let status = body.status.clone().unwrap_or_else(|| current.status.clone());
    let criticality = body.criticality.clone().unwrap_or_else(|| current.criticality.clone());
    let service_type = body.service_type.clone().unwrap_or_else(|| current.service_type.clone());
    if let Err(e) = validate_enum("status", &status, SERVICE_STATUSES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("criticality", &criticality, SERVICE_CRITICALITIES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("type", &service_type, SERVICE_TYPES) {
        return Ok(bad_request(e));
    }
    let owner_id = match body.owner_id {
        Some(v) => v,
        None => current.owner_id,
    };
    let description = body.description.clone().unwrap_or_else(|| current.description.clone());
    let description_html = body
        .description_html
        .clone()
        .unwrap_or_else(|| current.description_html.clone());
    let repository_url = match body.repository_url {
        Some(v) => v,
        None => current.repository_url.clone(),
    };
    let documentation_url = match body.documentation_url {
        Some(v) => v,
        None => current.documentation_url.clone(),
    };
    let position = match body.position {
        Some(v) => v,
        None => current.position.clone(),
    };
    sqlx::query(
        "UPDATE services SET name = $1, description = $2, description_html = $3, status = $4, \
         criticality = $5, \"type\" = $6, owner_id = $7, repository_url = $8, documentation_url = $9, \
         position = $10, updated_at = now(), updated_by_id = $11 \
         WHERE id = $12 AND deleted_at IS NULL",
    )
    .bind(&name)
    .bind(&description)
    .bind(&description_html)
    .bind(&status)
    .bind(&criticality)
    .bind(&service_type)
    .bind(owner_id)
    .bind(&repository_url)
    .bind(&documentation_url)
    .bind(&position)
    .bind(user_id)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    let json = service_json_by_id(&st.pool, pk, "full")
        .await?
        .ok_or_else(|| common::errors::AppError::from(sqlx::Error::RowNotFound()))?;
    Ok((StatusCode::OK, Json(json)))
}
```

Note: `PatchService.owner_id`/`repository_url`/`documentation_url` are `Option<Option<T>>` (double option via `deserialize_present`); the `match { Some(v) => v, None => current… }` form in the code above preserves the explicit-null (clear field) case.

3c. `destroy_workspace` + core (cascade by service id, no project filter):

```rust
pub async fn destroy_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !service_access::gate_admin(&st.pool, auth.0, &slug, pk).await? {
        return Ok(deny());
    }
    destroy_service_core(&st, pk).await
}

pub(crate) async fn destroy_service_core(
    st: &AppState,
    pk: Uuid,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE service_dependencies SET deleted_at = now(), updated_at = now() \
         WHERE (from_service_id = $1 OR to_service_id = $1) AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE service_issues SET deleted_at = now(), updated_at = now() \
         WHERE service_id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE services SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

3d. Update the old handlers to delegate:

- Old `create`: keep its `gate_writer(project_id)` gate, then `create_service_core(&st, auth.0, project_id, &body).await`. Delete the old inline insert logic from this handler (the core now owns it).
- Old `patch`: keep its gate, then `patch_service_core(&st, auth.0, pk, &body).await`. Delete the inline update logic.
- Old `destroy`: keep its gate, then `destroy_service_core(&st, pk).await`. Delete the inline cascade (the new core covers it, keyed by id instead of project).

- [ ] **Step 4: Run tests**

Run: `cd apps/api-rs && cargo check -p api && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service.rs
git commit -m "feat(api): canonical workspace service create/patch/destroy"
```

### Task 5: Project links + transfer-owner

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs`

- [ ] **Step 1: Add failing tests**

Append to `service_workspace_test.rs`:

```rust
#[tokio::test]
async fn project_links_and_transfer_owner() {
    use api::routes::service::{
        project_links_create, project_links_destroy, project_links_list, transfer_owner,
        CreateProjectLink, TransferOwner,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // P4 member (15) cannot self-serve a consumer link
    let denied = project_links_create(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
        Json(CreateProjectLink {
            project_id: sc.p4,
            link_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::FORBIDDEN);

    // promote to P4 admin, retry -> self-serve works
    sqlx::query("UPDATE project_members SET role = 20 WHERE member_id = $1 AND project_id = $2")
        .bind(sc.outsider)
        .bind(sc.p4)
        .execute(&st.pool)
        .await
        .unwrap();
    let (status, Json(link)) = project_links_create(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
        Json(CreateProjectLink {
            project_id: sc.p4,
            link_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(link["link_type"], "consumer");
    let link_id = Uuid::parse_str(link["id"].as_str().unwrap()).unwrap();

    // duplicate active link rejected
    let dup = project_links_create(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
        Json(CreateProjectLink {
            project_id: sc.p4,
            link_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(dup.0, StatusCode::BAD_REQUEST);

    // owner admin can revoke it
    let (status, _) = project_links_destroy(
        State(st.clone()),
        AuthUser(sc.owner_admin),
        Path((sc.slug.clone(), sc.s1, link_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!api::routes::service_access::linked_to_project(&st.pool, sc.s1, sc.p4)
        .await
        .unwrap());

    // owner link cannot be deleted directly
    let (_, Json(links)) = project_links_list(
        State(st.clone()),
        AuthUser(sc.admin),
        Path((sc.slug.clone(), sc.s1)),
    )
    .await
    .unwrap();
    let owner_link_id = links
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["link_type"] == "owner")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let owner_del = project_links_destroy(
        State(st.clone()),
        AuthUser(sc.admin),
        Path((sc.slug.clone(), sc.s1, Uuid::parse_str(&owner_link_id).unwrap())),
    )
    .await
    .unwrap();
    assert_eq!(owner_del.0, StatusCode::BAD_REQUEST);

    // transfer: non ws-admin denied
    let denied = transfer_owner(
        State(st.clone()),
        AuthUser(sc.owner_admin),
        Path((sc.slug.clone(), sc.s1)),
        Json(TransferOwner { project_id: sc.p4 }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::FORBIDDEN);

    // re-add P4 consumer link so transfer can absorb it
    project_links_create(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
        Json(CreateProjectLink {
            project_id: sc.p4,
            link_type: None,
        }),
    )
    .await
    .unwrap();

    // ws admin transfers; old owner demoted to consumer; target link absorbed
    let (status, Json(after)) = transfer_owner(
        State(st.clone()),
        AuthUser(sc.admin),
        Path((sc.slug.clone(), sc.s1)),
        Json(TransferOwner { project_id: sc.p4 }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["owner_project_id"], sc.p4.to_string());
    let links = after["project_links"].as_array().unwrap();
    assert_eq!(
        links.iter().filter(|l| l["project_id"] == sc.p4.to_string()).count(),
        1,
        "absorbed consumer link: exactly one P4 link (owner)"
    );
    assert!(links
        .iter()
        .any(|l| l["project_id"] == sc.p4.to_string() && l["link_type"] == "owner"));
    assert!(links
        .iter()
        .any(|l| l["project_id"] == sc.p1.to_string() && l["link_type"] == "consumer"));
    assert!(links
        .iter()
        .any(|l| l["project_id"] == sc.p2.to_string() && l["link_type"] == "consumer"));
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test ... project_links_and_transfer_owner` won't compile (`project_links_create` not found).

- [ ] **Step 3: Implement** (all in `service.rs`)

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct CreateProjectLink {
    pub project_id: Uuid,
    pub link_type: Option<String>,
}

pub async fn project_links_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !service_access::full_visible(&st.pool, auth.0, &slug, pk).await? {
        return Ok(missing());
    }
    let links = links_for_services(&st.pool, &[pk]).await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(
            links
                .iter()
                .map(|l| json!({"id": l.id, "project_id": l.project_id, "link_type": l.link_type}))
                .collect(),
        )),
    ))
}

pub async fn project_links_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
    Json(body): Json<CreateProjectLink>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    let owner = service_access::owner_project(&st.pool, pk).await?;
    let actor_admin_target =
        service_access::is_project_admin(&st.pool, auth.0, &slug, body.project_id).await?;
    let actor_admin_owner = match owner {
        Some(owner_id) => service_access::is_project_admin(&st.pool, auth.0, &slug, owner_id).await?,
        None => false,
    };
    if !(ws_admin || actor_admin_target || actor_admin_owner) {
        return Ok(deny());
    }
    if body.link_type.as_deref().unwrap_or("consumer") != "consumer" {
        return Ok(bad_request(
            "Only consumer links can be added; ownership is transferred.",
        ));
    }
    let project_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL)",
    )
    .bind(body.project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    if !project_ok {
        return Ok(bad_request("Invalid project_id - object does not exist."));
    }
    let existing_active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_project_links \
         WHERE service_id = $1 AND project_id = $2 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(body.project_id)
    .fetch_one(&st.pool)
    .await?;
    if existing_active {
        return Ok(bad_request("This project is already linked to the service."));
    }
    let link_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_project_links (id, workspace_id, service_id, project_id, link_type, \
         created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, s.workspace_id, s.id, $2, 'consumer', now(), now(), $3, $3 \
         FROM services s WHERE s.id = $4",
    )
    .bind(link_id)
    .bind(body.project_id)
    .bind(auth.0)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": link_id, "project_id": body.project_id, "link_type": "consumer"})),
    ))
}

pub async fn project_links_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk, link_id)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let link: Option<ServiceLinkRow> = sqlx::query_as(
        "SELECT id, service_id, project_id, link_type FROM service_project_links \
         WHERE id = $1 AND service_id = $2 AND deleted_at IS NULL",
    )
    .bind(link_id)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(link) = link else {
        return Ok(missing());
    };
    if link.link_type == "owner" {
        return Ok(bad_request(
            "The owner link cannot be removed. Transfer ownership instead.",
        ));
    }
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    let actor_admin_link_project =
        service_access::is_project_admin(&st.pool, auth.0, &slug, link.project_id).await?;
    let actor_admin_owner = match service_access::owner_project(&st.pool, pk).await? {
        Some(owner_id) => service_access::is_project_admin(&st.pool, auth.0, &slug, owner_id).await?,
        None => false,
    };
    if !(ws_admin || actor_admin_link_project || actor_admin_owner) {
        return Ok(deny());
    }
    sqlx::query(
        "UPDATE service_project_links SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(link_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

pub async fn transfer_owner(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
    Json(body): Json<TransferOwner>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !is_workspace_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let Some(old_owner) = service_access::owner_project(&st.pool, pk).await? else {
        return Ok(missing());
    };
    let project_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL)",
    )
    .bind(body.project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    if !project_ok {
        return Ok(bad_request("Invalid project_id - object does not exist."));
    }
    let mut tx = st.pool.begin().await?;
    // absorb an existing consumer link for the new owner
    sqlx::query(
        "UPDATE service_project_links SET deleted_at = now(), updated_at = now() \
         WHERE service_id = $1 AND project_id = $2 AND link_type = 'consumer' AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(body.project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE service_project_links SET project_id = $1, updated_at = now(), updated_by_id = $2 \
         WHERE service_id = $3 AND link_type = 'owner' AND deleted_at IS NULL",
    )
    .bind(body.project_id)
    .bind(auth.0)
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    if old_owner != body.project_id {
        let revived = sqlx::query(
            "UPDATE service_project_links SET deleted_at = NULL, updated_at = now(), updated_by_id = $1 \
             WHERE service_id = $2 AND project_id = $3 AND link_type = 'consumer' AND deleted_at IS NOT NULL",
        )
        .bind(auth.0)
        .bind(pk)
        .bind(old_owner)
        .execute(&mut *tx)
        .await?;
        if revived.rows_affected() == 0 {
            sqlx::query(
                "INSERT INTO service_project_links (id, workspace_id, service_id, project_id, \
                 link_type, created_at, updated_at, created_by_id, updated_by_id) \
                 SELECT gen_random_uuid(), s.workspace_id, s.id, $1, 'consumer', now(), now(), $2, $2 \
                 FROM services s WHERE s.id = $3",
            )
            .bind(old_owner)
            .bind(auth.0)
            .bind(pk)
            .execute(&mut *tx)
            .await?;
        }
    }
    // dual-write the legacy column
    sqlx::query("UPDATE services SET project_id = $1, updated_at = now() WHERE id = $2")
        .bind(body.project_id)
        .bind(pk)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let json = service_json_by_id(&st.pool, pk, "full")
        .await?
        .ok_or_else(|| common::errors::AppError::from(sqlx::Error::RowNotFound()))?;
    Ok((StatusCode::OK, Json(json)))
}
```

- [ ] **Step 4: Run tests** — same serial command. Expected: all PASS.
- [ ] **Step 5: Commit** — `git add apps/api-rs/crates/api/src/routes/service.rs && git commit -m "feat(api): service project links + owner transfer"`.

---

### Task 6: Canonical dependencies (workspace graph + lite projection)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs`

- [ ] **Step 1: Add failing tests**

Append to `service_workspace_test.rs`:

```rust
#[tokio::test]
async fn canonical_dependencies_cross_project_with_lite() {
    use api::routes::service::{
        dependencies_create_workspace, dependencies_list_workspace, CreateDependency,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // S1 -> S4: S4 is not full-visible to a P1 member -> rejected
    let denied = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s4,
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::BAD_REQUEST);

    // S2 -> S1 by P2 admin: S1 is consumer-linked to P2, so both endpoints are full
    let (status, Json(edge)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s2,
            to_service_id: sc.s1,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(edge["from_service"]["access_level"], "full");
    assert_eq!(edge["to_service"]["access_level"], "full");

    // outsider sees no edges (neither endpoint visible to P4)
    let (_, Json(rows)) = dependencies_list_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 0);

    // P1 member sees the S2 -> S1 edge with S2 projected lite
    let (_, Json(rows)) = dependencies_list_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["from_service_id"], sc.s2.to_string());
    assert_eq!(rows[0]["to_service_id"], sc.s1.to_string());
    assert_eq!(rows[0]["from_service"]["access_level"], "lite");
    assert_eq!(rows[0]["from_service"]["name"], "Core Banking");
    assert!(rows[0]["from_service"].get("owner_id").is_none(), "lite hides details");
    assert_eq!(rows[0]["to_service"]["access_level"], "full");

    // cycle: P1 subscribes to S2, then S1 -> S2 closes S2 -> S1
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;
    let cycle = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
        }),
    )
    .await
    .unwrap();
    assert_eq!(cycle.0, StatusCode::BAD_REQUEST);
    assert_eq!(cycle.1["error"], "This dependency would create a cycle.");
}

#[tokio::test]
async fn canonical_dependency_destroy_by_from_writer() {
    use api::routes::service::{
        dependencies_create_workspace, dependency_destroy_workspace, CreateDependency,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // P1 subscribes to S2 so its member may create S1 -> S2
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;
    let (status, Json(edge)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    let edge_id = Uuid::parse_str(edge["id"].as_str().unwrap()).unwrap();

    // outsider cannot delete it
    let denied = dependency_destroy_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), edge_id)),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::FORBIDDEN);

    // from-side writer can
    let (status, _) = dependency_destroy_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), edge_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::NO_CONTENT);
}
```

- [ ] **Step 2: Run to verify failure** — compile error (`dependencies_create_workspace` not found).

- [ ] **Step 3: Implement**

3a. Replace `DependencyRow` (lines 515-523) — `project_id` becomes nullable:

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DependencyRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>, // legacy dual-write column (nullable)
    pub from_service_id: Uuid,
    pub to_service_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

3b. Replace the old sync `dependency_json(row)` with endpoint-aware async helpers:

```rust
async fn endpoint_json(
    pool: &sqlx::PgPool,
    service_id: Uuid,
    visible: &[Uuid],
    ws_admin: bool,
) -> Result<Value, sqlx::Error> {
    if ws_admin || visible.contains(&service_id) {
        if let Some(v) = service_json_by_id(pool, service_id, "full").await? {
            return Ok(v);
        }
    }
    let lite: Option<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT id, name, status, criticality FROM services WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(service_id)
    .fetch_optional(pool)
    .await?;
    Ok(match lite {
        Some((id, name, status, criticality)) => lite_service_json(id, &name, &status, &criticality),
        None => Value::Null,
    })
}

pub(crate) async fn dependency_json(
    pool: &sqlx::PgPool,
    row: &DependencyRow,
    visible: &[Uuid],
    ws_admin: bool,
    project_echo: Option<Uuid>,
) -> Result<Value, sqlx::Error> {
    let from = endpoint_json(pool, row.from_service_id, visible, ws_admin).await?;
    let to = endpoint_json(pool, row.to_service_id, visible, ws_admin).await?;
    Ok(json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "project_id": project_echo.or(row.project_id),
        "from_service_id": row.from_service_id,
        "to_service_id": row.to_service_id,
        "created_at": row.created_at,
        "from_service": from,
        "to_service": to,
    }))
}
```

3c. Extract the shared insert into `create_dependency_core` and add canonical handlers:

```rust
pub(crate) async fn create_dependency_core(
    st: &AppState,
    user_id: Uuid,
    slug: &str,
    from_service_id: Uuid,
    to_service_id: Uuid,
    project_echo: Option<Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if from_service_id == to_service_id {
        return Ok(bad_request("A service cannot depend on itself."));
    }
    let workspace_id: Uuid = sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1")
        .bind(slug)
        .fetch_one(&st.pool)
        .await?;
    let both: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM services WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .bind(vec![from_service_id, to_service_id])
    .fetch_one(&st.pool)
    .await?;
    if both != 2 {
        return Ok(bad_request("Source service not found."));
    }
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_dependencies WHERE workspace_id = $1 \
         AND from_service_id = $2 AND to_service_id = $3 AND deleted_at IS NULL)",
    )
    .bind(workspace_id)
    .bind(from_service_id)
    .bind(to_service_id)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok(bad_request("This dependency already exists."));
    }
    let creates_cycle: bool = sqlx::query_scalar(
        "WITH RECURSIVE reach(node) AS ( \
           SELECT to_service_id FROM service_dependencies \
             WHERE workspace_id = $1 AND from_service_id = $2 AND deleted_at IS NULL \
           UNION \
           SELECT d.to_service_id FROM service_dependencies d \
             JOIN reach r ON d.from_service_id = r.node \
             WHERE d.workspace_id = $1 AND d.deleted_at IS NULL \
         ) SELECT EXISTS(SELECT 1 FROM reach WHERE node = $3)",
    )
    .bind(workspace_id)
    .bind(to_service_id)
    .bind(from_service_id)
    .fetch_one(&st.pool)
    .await?;
    if creates_cycle {
        return Ok(bad_request("This dependency would create a cycle."));
    }
    let from_owner = service_access::owner_project(&st.pool, from_service_id).await?;
    let to_owner = service_access::owner_project(&st.pool, to_service_id).await?;
    let legacy_project = match (from_owner, to_owner) {
        (Some(a), Some(b)) if a == b => Some(a),
        _ => None,
    };
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_dependencies (id, workspace_id, project_id, from_service_id, \
         to_service_id, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, now(), now(), $6, $6)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(legacy_project)
    .bind(from_service_id)
    .bind(to_service_id)
    .bind(user_id)
    .execute(&st.pool)
    .await?;
    let row: DependencyRow = sqlx::query_as(&format!("{DEPENDENCY_SELECT} WHERE id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    let ws_admin = is_workspace_admin(&st.pool, user_id, slug).await?;
    let visible = if ws_admin {
        vec![]
    } else {
        service_access::visible_service_ids(&st.pool, user_id, slug).await?
    };
    Ok((
        StatusCode::CREATED,
        Json(dependency_json(&st.pool, &row, &visible, ws_admin, project_echo).await?),
    ))
}

pub async fn dependencies_create_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<CreateDependency>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !service_access::gate_writer(&st.pool, auth.0, &slug, body.from_service_id).await? {
        return Ok(deny());
    }
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !ws_admin
        && !service_access::full_visible(&st.pool, auth.0, &slug, body.to_service_id).await?
    {
        return Ok(bad_request("Target service not found."));
    }
    create_dependency_core(
        &st,
        auth.0,
        &slug,
        body.from_service_id,
        body.to_service_id,
        None,
    )
    .await
}

pub async fn dependencies_list_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    let visible = if ws_admin {
        vec![]
    } else {
        service_access::visible_service_ids(&st.pool, auth.0, &slug).await?
    };
    let rows: Vec<DependencyRow> = sqlx::query_as(&format!(
        "{DEPENDENCY_SELECT} WHERE workspace_id = (SELECT id FROM workspaces WHERE slug = $1) \
         AND deleted_at IS NULL \
         AND ($2::uuid[] IS NULL OR from_service_id = ANY($2) OR to_service_id = ANY($2)) \
         ORDER BY created_at ASC"
    ))
    .bind(&slug)
    .bind(if ws_admin { None } else { Some(visible.clone()) })
    .fetch_all(&st.pool)
    .await?;
    let mut payload = Vec::with_capacity(rows.len());
    for row in &rows {
        payload.push(dependency_json(&st.pool, row, &visible, ws_admin, None).await?);
    }
    Ok((StatusCode::OK, Json(Value::Array(payload))))
}

pub async fn dependency_destroy_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<DependencyRow> = sqlx::query_as(&format!("{DEPENDENCY_SELECT} WHERE id = $1"))
        .bind(pk)
        .fetch_optional(&st.pool)
        .await?;
    let Some(row) = row else {
        return Ok(missing());
    };
    if !service_access::gate_writer(&st.pool, auth.0, &slug, row.from_service_id).await? {
        return Ok(deny());
    }
    sqlx::query(
        "UPDATE service_dependencies SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

3d. Update the old compat handlers minimally so everything compiles and keeps old behavior until Task 7:

- Old `dependencies_create`: keep its `gate_writer(:pid)` + `service_exists` checks; replace the inline dup/cycle/insert block with `create_dependency_core(&st, auth.0, &slug, body.from_service_id, body.to_service_id, Some(project_id)).await`.
- Old `dependencies_list`: replace the final map with:

```rust
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    let visible = if ws_admin {
        vec![]
    } else {
        service_access::visible_service_ids(&st.pool, auth.0, &slug).await?
    };
    let mut payload = Vec::with_capacity(rows.len());
    for row in &rows {
        payload.push(
            dependency_json(&st.pool, row, &visible, ws_admin, Some(project_id)).await?,
        );
    }
    Ok((StatusCode::OK, Json(Value::Array(payload))))
```

- Old `dependency_destroy`: unchanged (soft delete keyed by id + project filter still works for legacy rows; Task 7 reworks it).
- Delete the now-unused `service_exists` helper only if nothing else references it (`grep -n service_exists`); otherwise leave it for Task 7 removal.
- Keep the old `dependency_json` removed; all callers now use the async version.

- [ ] **Step 4: Run tests** — `cargo check -p api` + serial suite, all PASS.
- [ ] **Step 5: Commit** — `feat(api): workspace-wide service dependency graph with lite projection`.

### Task 7: Compat layer tightening (project-scoped routes)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs`

Compat rules (design §3): the `:project_id` path segment is only an additional scoping assertion — mutation gates stay canonical; dependency responses echo `project_id = :pid` so the legacy web store filters keep working.

- [ ] **Step 1: Add failing tests**

Append to `service_workspace_test.rs`:

```rust
#[tokio::test]
async fn compat_services_scope_to_linked_and_owner_gates() {
    use api::routes::service::{detail as compat_detail, list as compat_list, patch as compat_patch};
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // consumer project list includes the consumer-linked service
    let (status, Json(rows)) = compat_list(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.p2)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let s1 = rows.iter().find(|r| r["id"] == sc.s1.to_string()).unwrap();
    assert_eq!(s1["project_id"], sc.p1.to_string(), "deprecated field = owner project");

    // consumer cannot patch through the compat route (owner-only writer)
    let denied = compat_patch(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.p2, sc.s1)),
        Json(api::routes::service::PatchService {
            status: Some("maintenance".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::FORBIDDEN);

    // outsider is a P4 member: gate passes, linked assertion 404s
    let not_linked = compat_detail(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.p4, sc.s1)),
    )
    .await
    .unwrap();
    assert_eq!(not_linked.0, StatusCode::NOT_FOUND);

    // non-member is denied at the gate
    let stranger = compat_detail(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p4, sc.s4)),
    )
    .await
    .unwrap();
    assert_eq!(stranger.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn compat_dependencies_echo_and_link_scope() {
    use api::routes::service::{
        dependencies_create as compat_deps_create, dependencies_list as compat_deps_list,
        CreateDependency,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // S2 is not linked to P1 -> target rejected
    let denied = compat_deps_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::BAD_REQUEST);
    assert_eq!(denied.1["error"], "Target service not found.");

    // link S2 to P1 as consumer, then the same create succeeds
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;
    let (status, Json(edge)) = compat_deps_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(edge["project_id"], sc.p1.to_string(), "compat echo");

    let (_, Json(rows)) = compat_deps_list(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
    )
    .await
    .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["project_id"], sc.p1.to_string());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test compat_ -- --test-threads=1`
Expected: FAIL — compat create currently allows the unlinked S2, and compat patch allows the consumer admin.

- [ ] **Step 3: Implement**

- Old `detail` (compat): after the existing `gate_member` check add:

```rust
    if !service_access::linked_to_project(&st.pool, pk, project_id).await? {
        return Ok(missing());
    }
```

- Old `patch` (compat): after its existing gate add the same linked assertion, then the canonical writer gate, then delegate:

```rust
    if !service_access::linked_to_project(&st.pool, pk, project_id).await? {
        return Ok(missing());
    }
    if !service_access::gate_writer(&st.pool, auth.0, &slug, pk).await? {
        return Ok(deny());
    }
    return patch_service_core(&st, auth.0, pk, &body).await;
```

(Replace the old body of `patch` entirely; the old `gate_writer(project_id)` check can be dropped — `gate_writer(service)` already implies the caller is a member+ somewhere linked, and the linked assertion pins `:pid`.)

- Old `destroy` (compat): after its existing gate add linked assertion + canonical admin gate, then delegate:

```rust
    if !service_access::linked_to_project(&st.pool, pk, project_id).await? {
        return Ok(missing());
    }
    if !service_access::gate_admin(&st.pool, auth.0, &slug, pk).await? {
        return Ok(deny());
    }
    return destroy_service_core(&st, pk).await;
```

- Old `dependencies_list` (compat): replace the rows query `WHERE project_id = $1` with the linked-services filter:

```rust
    let rows: Vec<DependencyRow> = sqlx::query_as(&format!(
        "{DEPENDENCY_SELECT} \
         WHERE workspace_id = (SELECT id FROM workspaces WHERE slug = $1) AND deleted_at IS NULL \
         AND (from_service_id IN (SELECT service_id FROM service_project_links \
                WHERE project_id = $2 AND deleted_at IS NULL) \
           OR to_service_id IN (SELECT service_id FROM service_project_links \
                WHERE project_id = $2 AND deleted_at IS NULL)) \
         ORDER BY created_at ASC"
    ))
    .bind(&slug)
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
```

Then keep the mapping code from Task 6 (`project_echo = Some(project_id)`).

- Old `dependencies_create` (compat): keep `gate_writer(:pid)`; replace the `service_exists` checks with:

```rust
    if !service_access::linked_to_project(&st.pool, body.from_service_id, project_id).await? {
        return Ok(bad_request("Source service not found."));
    }
    if !service_access::linked_to_project(&st.pool, body.to_service_id, project_id).await? {
        return Ok(bad_request("Target service not found."));
    }
    return create_dependency_core(
        &st,
        auth.0,
        &slug,
        body.from_service_id,
        body.to_service_id,
        Some(project_id),
    )
    .await;
```

- Old `dependency_destroy` (compat): fetch the row first, then:

```rust
    let row: Option<DependencyRow> = sqlx::query_as(&format!(
        "{DEPENDENCY_SELECT} WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Ok(missing());
    };
    if !service_access::linked_to_project(&st.pool, row.from_service_id, project_id).await? {
        return Ok(missing());
    }
    if !service_access::gate_writer(&st.pool, auth.0, &slug, row.from_service_id).await? {
        return Ok(deny());
    }
```

then soft delete by id (no project filter).

- Remove `service_exists` if `grep -n "service_exists" apps/api-rs/crates/api/src/routes/service.rs` only shows the definition.

- [ ] **Step 4: Run tests** — `cargo check -p api` + serial suite, all PASS.
- [ ] **Step 5: Commit** — `feat(api): scope compat service routes to linked services`.

---

### Task 8: Service work-item links (canonical + compat validation)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs`

- [ ] **Step 1: Add failing tests**

Append to `service_workspace_test.rs` (plus the `insert_issue` fixture helper):

```rust
async fn insert_issue(pool: &PgPool, sc: &Scratch, project_id: Uuid, name: &str) -> Uuid {
    let state_id = Uuid::new_v4();
    let type_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, \
         sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, 'Open', '', '#cccccc', $2, $3, $4, 65535, 'unstarted', true, false, now(), now())",
    )
    .bind(state_id)
    .bind(format!("open-{}", Uuid::new_v4().simple()))
    .bind(project_id)
    .bind(sc.workspace_id)
    .execute(pool)
    .await
    .expect("scratch state");
    sqlx::query(
        "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
         is_default, level, is_epic, requires_service, created_at, updated_at) \
         VALUES ($1, 'Incident', '', '{}'::jsonb, $2, true, true, 0, false, true, now(), now())",
    )
    .bind(type_id)
    .bind(sc.workspace_id)
    .execute(pool)
    .await
    .expect("scratch type");
    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, name, description, description_html, priority, state_id, \
         project_id, workspace_id, sequence_id, type_id, created_at, updated_at, is_draft) \
         VALUES ($1, $2, '{}'::jsonb, '<p></p>', 'none', $3, $4, $5, $6, $7, now(), now(), false)",
    )
    .bind(issue_id)
    .bind(name)
    .bind(state_id)
    .bind(project_id)
    .bind(sc.workspace_id)
    .bind(1_i64)
    .bind(type_id)
    .execute(pool)
    .await
    .expect("scratch issue");
    issue_id
}
```

Note: if the `issues` insert fails on a NOT NULL column in your DB, copy the column list from the `INSERT INTO issues` statements in `tests/war_room_test.rs` (grep `INSERT INTO issues`). That is the only allowed deviation from this snippet.

```rust
#[tokio::test]
async fn service_issues_require_linked_service() {
    use api::routes::service::{issues_create as compat_issues_create, CreateServiceIssue};
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    let issue_id = insert_issue(&st.pool, &sc, sc.p1, "ISO one").await;

    // S2 is not linked to P1 -> 404
    let denied = compat_issues_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateServiceIssue {
            service_id: sc.s2,
            issue_id,
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::NOT_FOUND);

    // S1 is linked -> ok
    let (status, Json(link)) = compat_issues_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateServiceIssue {
            service_id: sc.s1,
            issue_id,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(link["service_id"], sc.s1.to_string());
}

#[tokio::test]
async fn canonical_service_issues_resolve_project_from_issue() {
    use api::routes::service::{issues_create_workspace, issues_list_workspace, CreateServiceIssue, ServiceIssueListParams};
    use axum::extract::{Path, Query, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    let issue_id = insert_issue(&st.pool, &sc, sc.p1, "ISO two").await;

    // P4 member cannot link an issue from P1
    let denied = issues_create_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path(sc.slug.clone()),
        Json(CreateServiceIssue {
            service_id: sc.s1,
            issue_id,
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        denied.0,
        StatusCode::FORBIDDEN,
        "outsider is not a member of the issue's project (P1)"
    );

    let (status, Json(link)) = issues_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateServiceIssue {
            service_id: sc.s1,
            issue_id,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    let link_id = Uuid::parse_str(link["id"].as_str().unwrap()).unwrap();

    // canonical list filtered by service_id
    let (_, Json(rows)) = issues_list_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Query(ServiceIssueListParams {
            service_id: Some(sc.s1),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);

    // destroy: member+ of the issue project (P2 admin is not in P1 -> denied)
    let denied = api::routes::service::issue_destroy_workspace(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), link_id)),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Run to verify failure** — compile error (`CreateServiceIssue` variants / canonical handlers missing).

- [ ] **Step 3: Implement**

- Old `issues_create` (compat): replace the `service_exists(project_id, ...)` check with:

```rust
    if !service_access::linked_to_project(&st.pool, body.service_id, project_id).await? {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Service not found."})),
        ));
    }
```

Then delegate the rest to `issues_create_core` (below).

- Add canonical handlers + core (reuse the existing `ServiceIssueRow`, `SERVICE_ISSUE_SELECT`, `service_issue_json` — check their exact names around lines 665-700 and reuse verbatim):

```rust
#[derive(Debug, Deserialize, Default)]
pub struct ServiceIssueListParams {
    pub service_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
}

pub(crate) async fn issues_create_core(
    st: &AppState,
    user_id: Uuid,
    project_id: Uuid,
    body: &CreateServiceIssue,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let issue_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
    )
    .bind(body.issue_id)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    if !issue_exists {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Issue not found."})),
        ));
    }
    let existing: Option<ServiceIssueRow> = sqlx::query_as(&format!(
        "{SERVICE_ISSUE_SELECT} WHERE si.project_id = $1 AND si.service_id = $2 \
         AND si.issue_id = $3 AND si.deleted_at IS NULL"
    ))
    .bind(project_id)
    .bind(body.service_id)
    .bind(body.issue_id)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(row) = existing {
        return Ok((StatusCode::CREATED, Json(service_issue_json(&row))));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
         created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, i.workspace_id, i.project_id, $2, $3, now(), now(), $4, $4 \
         FROM issues i WHERE i.id = $3",
    )
    .bind(id)
    .bind(body.service_id)
    .bind(body.issue_id)
    .bind(user_id)
    .execute(&st.pool)
    .await?;
    let row: ServiceIssueRow = sqlx::query_as(&format!("{SERVICE_ISSUE_SELECT} WHERE si.id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(service_issue_json(&row))))
}

pub async fn issues_create_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<CreateServiceIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let issue_project: Option<Uuid> = sqlx::query_scalar(
        "SELECT i.project_id FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         WHERE i.id = $1 AND w.slug = $2 AND i.deleted_at IS NULL",
    )
    .bind(body.issue_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(project_id) = issue_project else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Issue not found."})),
        ));
    };
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !(ws_admin || matches!(role, Some(20) | Some(15))) {
        return Ok(deny());
    }
    if !service_access::linked_to_project(&st.pool, body.service_id, project_id).await? {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Service not found."})),
        ));
    }
    issues_create_core(&st, auth.0, project_id, &body).await
}

pub async fn issues_list_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(params): Query<ServiceIssueListParams>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    let visible: Option<Vec<Uuid>> = if ws_admin {
        None
    } else {
        Some(service_access::visible_service_ids(&st.pool, auth.0, &slug).await?)
    };
    let rows: Vec<ServiceIssueRow> = sqlx::query_as(&format!(
        "{SERVICE_ISSUE_SELECT} \
         WHERE si.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) AND si.deleted_at IS NULL \
         AND ($2::uuid IS NULL OR si.service_id = $2) \
         AND ($3::uuid IS NULL OR si.project_id = $3) \
         AND ($4::uuid[] IS NULL OR si.service_id = ANY($4)) \
         ORDER BY si.created_at ASC"
    ))
    .bind(&slug)
    .bind(params.service_id)
    .bind(params.project_id)
    .bind(visible)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(service_issue_json).collect())),
    ))
}

pub async fn issue_destroy_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<ServiceIssueRow> = sqlx::query_as(&format!(
        "{SERVICE_ISSUE_SELECT} WHERE si.id = $1 AND si.deleted_at IS NULL"
    ))
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Ok(missing());
    };
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, row.project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !(ws_admin || matches!(role, Some(20) | Some(15))) {
        return Ok(deny());
    }
    sqlx::query("UPDATE service_issues SET deleted_at = now(), updated_at = now() WHERE id = $1")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- Old `issues_list` (compat): unchanged (project filter still valid).
- Old `issue_destroy` (compat): unchanged (gate + project filter).
- Old `issues_create` (compat): its gate stays `gate_writer(:pid)`; after the linked check, delegate: `issues_create_core(&st, auth.0, project_id, &body).await`.

Note: `ServiceIssueRow.project_id` must be `Uuid` for `row.project_id` above — check the struct; if it is `Option<Uuid>`, adjust the gate call to use `.unwrap_or(project_id)` from the query and keep behavior (there is no nullable case in practice).

- [ ] **Step 4: Run tests** — `cargo check -p api` + serial suite, all PASS.
- [ ] **Step 5: Commit** — `feat(api): service work-item links scoped to linked services`.

### Task 9: War room attach gate

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs` (validation block at lines 1197-1208)

- [ ] **Step 1: Add failing test**

Append to `service_workspace_test.rs` (plus the `insert_war_room` helper):

```rust
async fn insert_war_room(pool: &PgPool, sc: &Scratch, project_id: Uuid) -> Uuid {
    let issue_id = insert_issue(pool, sc, project_id, "War room primary").await;
    let room_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO war_rooms (id, workspace_id, project_id, sequence_id, name, severity, \
         status, primary_issue_id, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, 'Incident room', 'sev3', 'active', $5, now(), now())",
    )
    .bind(room_id)
    .bind(sc.workspace_id)
    .bind(project_id)
    .bind(1_i64)
    .bind(issue_id)
    .execute(pool)
    .await
    .expect("scratch war room");
    room_id
}

#[tokio::test]
async fn war_room_attach_requires_linked_service() {
    use api::routes::war_room::{services_create, LinkServices};
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    let room_id = insert_war_room(&st.pool, &sc, sc.p1).await;

    // S2 is not linked to P1 -> rejected
    let denied = services_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1, room_id)),
        Json(LinkServices {
            service_ids: vec![sc.s2],
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, StatusCode::BAD_REQUEST);

    // S1 is linked -> allowed
    let (status, _) = services_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1, room_id)),
        Json(LinkServices {
            service_ids: vec![sc.s1],
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
}
```

- [ ] **Step 2: Run to verify failure** — first assert fails (current code accepts S2 because `services.project_id = P1` for S2? No: S2's legacy `project_id` is NULL in this fixture, so the current count query returns 0 → 400 already passes. To see a genuine failure, the more precise assertion is the second part (S1 attach succeeds) plus the consumer case below. Add this case to make the test fail on current code:

```rust
    // S1 (consumer-linked to P2) can be attached to a P2 war room
    let p2_room = insert_war_room(&st.pool, &sc, sc.p2).await;
    let (status, _) = services_create(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.p2, p2_room)),
        Json(LinkServices {
            service_ids: vec![sc.s1],
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "consumer-linked service is attachable by the consumer project"
    );
```

Current code fails this because `services.project_id` for S1 is NULL (fixture) — the COUNT is 0.

- [ ] **Step 3: Implement**

Replace the validation query in `services_create` (lines 1197-1208):

```rust
    if !body.service_ids.is_empty() {
        let valid: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM services sv \
             WHERE sv.id = ANY($1) AND sv.deleted_at IS NULL \
             AND EXISTS (SELECT 1 FROM service_project_links spl \
               WHERE spl.service_id = sv.id AND spl.project_id = $2 AND spl.deleted_at IS NULL)",
        )
        .bind(&body.service_ids)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await?;
        if valid as usize != body.service_ids.len() {
            return Ok(bad_request("Invalid service_ids - object does not exist."));
        }
    }
```

- [ ] **Step 4: Run tests** — `cargo check -p api` + serial suite (include `--test war_room_test` to catch regressions), all PASS.
- [ ] **Step 5: Commit** — `feat(api): war room service attach scoped to linked services`.

---

### Task 10: Intake accept gate

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` (accept gate at lines 1412-1421)

- [ ] **Step 1: Add failing test**

Append to `service_workspace_test.rs`:

```rust
#[tokio::test]
async fn intake_accept_gate_requires_linked_service() {
    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    let issue_id = insert_issue(&st.pool, &sc, sc.p1, "Intake item").await;

    // required type + service linked to another project only -> gate message
    sqlx::query(
        "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
    )
    .bind(sc.workspace_id)
    .bind(sc.p1) // issue project (service_issues.project_id = issue's project)
    .bind(sc.s2) // service NOT linked to P1
    .bind(issue_id)
    .execute(&st.pool)
    .await
    .unwrap();

    let gate = api::routes::intake::accept_gate_for_test(&st.pool, sc.p1, issue_id)
        .await
        .unwrap();
    assert_eq!(gate, Some("Select a service before accepting this work item"));

    // linked service -> gate passes
    sqlx::query(
        "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
    )
    .bind(sc.workspace_id)
    .bind(sc.p1)
    .bind(sc.s1)
    .bind(issue_id)
    .execute(&st.pool)
    .await
    .unwrap();
    let gate = api::routes::intake::accept_gate_for_test(&st.pool, sc.p1, issue_id)
        .await
        .unwrap();
    assert_eq!(gate, None);
}
```

If `accept_gate` is private, add a test-only wrapper in `intake.rs`:

```rust
#[cfg(test)]
pub async fn accept_gate_for_test(
    pool: &sqlx::PgPool,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
) -> Result<Option<&'static str>, sqlx::Error> {
    accept_gate(pool, project_id, issue_id).await
}
```

(`#[cfg(test)]` does NOT apply to integration tests — instead make `accept_gate` `pub(crate)` is also insufficient. Use `#[doc(hidden)] pub async fn accept_gate_for_test(...)` unconditionally, next to `accept_gate`, delegating to it.)

- [ ] **Step 2: Run to verify failure** — first assert fails (current query ignores linkage).

- [ ] **Step 3: Implement**

Replace the `has_service` query inside `accept_gate`:

```rust
        let (has_service,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM service_issues si \
             JOIN service_project_links spl ON spl.service_id = si.service_id \
               AND spl.project_id = $2 AND spl.deleted_at IS NULL \
             WHERE si.issue_id = $1 AND si.deleted_at IS NULL)",
        )
        .bind(issue_id)
        .bind(project_id)
        .fetch_one(pool)
        .await?;
```

- [ ] **Step 4: Run tests** — also run `--test intake_triage_test` and `--test intake_test` to catch regressions; adjust fixtures if any test inserted `service_issues` rows without a link (add the link in those fixtures — that is the intended semantic change).
- [ ] **Step 5: Commit** — `feat(api): intake accept gate requires service linked to the project`.

---

### Task 11: AI tools visibility

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/services.rs` (SQL constants at lines 50-65, 157-190; descriptions at 114-119, 372+)

- [ ] **Step 1: Add failing test**

Append to `service_workspace_test.rs`:

```rust
#[tokio::test]
async fn ai_list_services_respects_visibility() {
    use ai::agent::new_trace;
    use ai::tools::services::{ListServices, ListServicesArgs};
    use rig::tool::{Tool, ToolContext};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let args = |project: Option<&str>, query: Option<&str>| ListServicesArgs {
        query: query.map(str::to_string),
        status: None,
        criticality: None,
        service_type: None,
        project: project.map(str::to_string),
        limit: None,
    };

    // P1 member: only S1
    let tool = ListServices {
        pool: st.pool.clone(),
        workspace_id: sc.workspace_id,
        user_id: sc.owner_member,
        trace: new_trace(),
    };
    let out = tool
        .call(&mut ToolContext::new(), args(None, None))
        .await
        .unwrap();
    assert!(out.contains("ISO Switch"));
    assert!(!out.contains("Core Banking"), "S2 is not linked to P1");
    assert!(!out.contains("P4 Widget"));

    // P2 admin sees S1 (consumer) + S2 (owner)
    let tool = ListServices {
        pool: st.pool.clone(),
        workspace_id: sc.workspace_id,
        user_id: sc.consumer_admin,
        trace: new_trace(),
    };
    let out = tool
        .call(&mut ToolContext::new(), args(Some("P2"), None))
        .await
        .unwrap();
    assert!(out.contains("ISO Switch"));
    assert!(out.contains("Core Banking"));

    // ws admin sees all
    let tool = ListServices {
        pool: st.pool.clone(),
        workspace_id: sc.workspace_id,
        user_id: sc.admin,
        trace: new_trace(),
    };
    let out = tool
        .call(&mut ToolContext::new(), args(None, None))
        .await
        .unwrap();
    assert!(out.contains("P4 Widget"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test ai_list_services -- --test-threads=1`
Expected: FAIL — the P1 member currently sees services via `sv.project_id` (NULL in fixtures) or misses consumer-linked ones depending on fixture legacy columns; the consumer/admin assertions expose it.

- [ ] **Step 3: Implement — replace the three SQL constants' ownership/visibility fragments**

New `SERVICES_SQL` (same row/filter shape, link-based ownership + visibility):

```rust
pub const SERVICES_SQL: &str = "SELECT sv.id, sv.name, sv.description, sv.status, \
     sv.criticality, sv.\"type\" AS service_type, \
     COALESCE(u.display_name, u.email, '') AS owner, p.identifier AS project, \
     sv.repository_url, sv.documentation_url \
     FROM services sv \
     JOIN service_project_links owner_link ON owner_link.service_id = sv.id \
       AND owner_link.link_type = 'owner' AND owner_link.deleted_at IS NULL \
     JOIN projects p ON p.id = owner_link.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = sv.owner_id \
     WHERE sv.workspace_id = $1 AND sv.deleted_at IS NULL \
     AND (EXISTS (SELECT 1 FROM workspace_members wm WHERE wm.workspace_id = sv.workspace_id \
            AND wm.member_id = $2 AND wm.role = 20 AND wm.is_active = true \
            AND wm.deleted_at IS NULL) \
       OR EXISTS (SELECT 1 FROM service_project_links spl \
            JOIN project_members pm ON pm.project_id = spl.project_id \
            WHERE spl.service_id = sv.id AND spl.deleted_at IS NULL \
            AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL)) \
     AND ($3::text IS NULL OR sv.name ILIKE '%' || $3 || '%') \
     AND ($4::text IS NULL OR sv.status = $4) \
     AND ($5::text IS NULL OR sv.criticality = $5) \
     AND ($6::text IS NULL OR sv.\"type\" = $6) \
     AND ($7::text IS NULL OR p.identifier ILIKE $7 OR p.name ILIKE '%' || $7 || '%') \
     ORDER BY sv.name LIMIT $8";
```

`SERVICE_BY_ID_SQL` (params: $1 id, $2 workspace, $3 user): same owner join; visibility fragment with `wm.workspace_id = sv.workspace_id AND wm.member_id = $3` and `pm.member_id = $3`; `WHERE sv.id = $1 AND sv.workspace_id = $2 AND sv.deleted_at IS NULL`.

`SERVICES_BY_NAME_SQL` (params: $1 workspace, $2 user, $3 name): same as `SERVICES_SQL` visibility fragment with `$2`, plus the name predicate.

`SERVICES_BY_NAME_PROJECT_SQL` (params: $1 project, $2 workspace, $3 name): owner join for display + project filter becomes a link filter:

```sql
AND EXISTS (SELECT 1 FROM service_project_links spl2 WHERE spl2.service_id = sv.id \
  AND spl2.project_id = $1 AND spl2.deleted_at IS NULL)
```

plus the visibility fragment using `$2` (project membership in `$1` is implied by `resolve_project` gating, but keep ws-admin fallback).

- Update the `ListServices::description()` string (`:114-119`) and `GetService` description: add “Services are workspace-scoped; dependencies may cross projects, and a dependency on a service you cannot fully access is reported with name/status only.”

- [ ] **Step 4: Run tests** — `cargo check -p ai && cargo test -p ai` plus the serial api suite (AI tool tests live in both crates; run the ai crate's unit tests too), all PASS.
- [ ] **Step 5: Commit** — `feat(ai): scope service tools to workspace link visibility`.

---

### Task 12: Mount canonical routes, full verification, ops

**Files:**

- Modify: `apps/api-rs/crates/api/src/main.rs` (after the war-room block, near line 779)

- [ ] **Step 1: Mount the canonical routes**

```rust
        // Workspace-level service catalog (canonical, design 2026-10-17).
        // Project-scoped routes above remain as a compat layer.
        .route(
            "/api/workspaces/:slug/services/",
            get(routes::service::list_workspace).post(routes::service::create_workspace),
        )
        .route(
            "/api/workspaces/:slug/services/:pk/",
            get(routes::service::detail_workspace)
                .put(routes::service::patch_workspace)
                .patch(routes::service::patch_workspace)
                .delete(routes::service::destroy_workspace),
        )
        .route(
            "/api/workspaces/:slug/services/:pk/transfer-owner/",
            post(routes::service::transfer_owner),
        )
        .route(
            "/api/workspaces/:slug/services/:pk/project-links/",
            get(routes::service::project_links_list).post(routes::service::project_links_create),
        )
        .route(
            "/api/workspaces/:slug/services/:pk/project-links/:link_id/",
            delete(routes::service::project_links_destroy),
        )
        .route(
            "/api/workspaces/:slug/service-dependencies/",
            get(routes::service::dependencies_list_workspace)
                .post(routes::service::dependencies_create_workspace),
        )
        .route(
            "/api/workspaces/:slug/service-dependencies/:pk/",
            delete(routes::service::dependency_destroy_workspace),
        )
        .route(
            "/api/workspaces/:slug/service-issues/",
            get(routes::service::issues_list_workspace).post(routes::service::issues_create_workspace),
        )
        .route(
            "/api/workspaces/:slug/service-issues/:pk/",
            delete(routes::service::issue_destroy_workspace),
        )
```

- [ ] **Step 2: Full check + suite**

```bash
cd apps/api-rs
cargo fmt --all
cargo clippy -p api -p ai --all-targets
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test war_room_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_test -- --test-threads=1
cargo test -p ai
```

Expected: everything green. If `intake_triage_test`/`intake_test` fixtures fail on the new accept-gate link requirement, add the `service_project_links` row to their fixtures (intended semantic change).

- [ ] **Step 3: Route smoke (running stack)**

Rebuild + restart per `AGENTS.md` (detached build; link stage can take 10+ minutes with no output):

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
# poll: tail -f /tmp/plane-api-build.log
curl -s -o /dev/null -w '%{http_code}' http://localhost:8000/health   # expect 200
systemctl --user restart plane-live.service && curl http://localhost:3100/live/health/
```

Then, authenticated as a workspace user, hit `GET /api/workspaces/<slug>/services/` and confirm workspace-level JSON (`owner_project_id`, `project_links`, `access_level`), and `GET /api/workspaces/<slug>/projects/<pid>/services/` still returns the legacy shape.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/main.rs
git commit -m "feat(api): mount workspace-level service routes"
```

- [ ] **Step 5: Follow-up note (not this plan)** — open a cleanup spec for phase 2: drop `services.project_id`, `service_dependencies.project_id`, the legacy per-project name index, compat endpoints, and the deprecated `project_id` JSON field after the web release ships and stabilizes.

---

## Plan self-review notes (already applied)

- Spec coverage: data model §1 → Tasks 1-2; access matrix §2 → Task 2 (+ enforced in 3-8); API surface §3 → Tasks 3-8, 12; consumers §5 → Tasks 9-11; migration/rollout §-deploy → Tasks 1, 12. Frontend §4 is a separate plan (`2026-10-17-workspace-level-services-web.md`).
- Type consistency: `service_json(row, links, access)` used by every handler; `ServiceLinkRow` reused by links/transfer/deps; `dependency_json(pool, row, visible, ws_admin, project_echo)` is the only dependency serializer.
- Known acceptable deviations: fixture column lists for `issues`/`war_rooms` inserts may need a one-line adjustment to match the live schema (noted at each site); `sqlx::Error::RowNotFound` call form depends on the sqlx version.

<!-- plan-chunk-6-end -->
