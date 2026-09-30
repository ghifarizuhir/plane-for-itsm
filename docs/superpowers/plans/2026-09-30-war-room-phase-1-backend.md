# War Room — Phase 1: Backend Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fase 1 dari `docs/superpowers/specs/2026-09-30-war-room-design.md` — tabel war room + API session-auth (room, links, participants, runbook, events, summary) di `apps/api-rs`, lengkap dengan gate akses dan test integrasi, tanpa realtime/publish Redis (fase 2) dan tanpa UI (fase 3+).

**Architecture:** Satu migrasi sqlx (`0011_war_rooms.sql`) menambah 7 tabel; modul baru `routes/war_room.rs` berisi pure helper (unit-tested) + handler SQL mengikuti pola `routes/service.rs`; route didaftarkan di `main.rs`; test integrasi DB ada di `crates/api/tests/war_room_test.rs`. Tanpa gate izin khusus: read = `gate_member`, write = `gate_writer` (keduanya di-`pub(crate)`-kan dari `service.rs`).

**Tech Stack:** Rust 1.96 (axum 0.7, sqlx runtime queries, PostgreSQL 15, uuid, chrono, serde_json), Docker Compose stack lokal.

**Spec:** `docs/superpowers/specs/2026-09-30-war-room-design.md` (§1–§2, §5; fase 2–5 menyusul di plan terpisah).

**Conventions to follow (verified):**

- `crate::routes::project::{deny, missing}` → `deny()` = `403 {"error":"You don't have the required permissions."}`; `missing()` = `404 {"error":"The required object does not exist."}`.
- Gate pattern `routes/service.rs:129-159` (`gate_member` = guest+member+ws admin; `gate_writer` = ADMIN/MEMBER + ws admin). Fase 1 mengubah keduanya jadi `pub(crate)` agar dipakai `war_room.rs`.
- Handler signature: `State(st): State<AppState>`, `auth: AuthUser` (pakai `auth.0`), `Path((slug, project_id, pk))`, return `Result<(StatusCode, Json<Value>), common::errors::AppError>`.
- `AppState { pool, redis, config }`; test membuat `redis::Client::open("redis://127.0.0.1:6379")` tanpa koneksi (fase 1 tidak memakai Redis).
- UUID v4 dibuat aplikasi (`Uuid::new_v4()`); timestamp `chrono::DateTime<chrono::Utc>`; JSONB via `serde_json::Value`.
- Migration style `0003_services.sql`: `CREATE TABLE IF NOT EXISTS`, index `... IF NOT EXISTS`, partial index untuk soft-delete.
- Test DB: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane` (port 5432 & 6379 ter-expose di `docker-compose-local.yml`).
- Suite test scratch harus serial: `-- --test-threads=1`.

**File structure fase 1:**

| File                                            | Tanggung jawab                                    |
| ----------------------------------------------- | ------------------------------------------------- |
| `apps/api-rs/migrations/0011_war_rooms.sql`     | 7 tabel + index                                   |
| `apps/api-rs/crates/api/src/routes/war_room.rs` | constants, pure helper, gate, handler, serializer |
| `apps/api-rs/crates/api/src/routes/mod.rs`      | registrasi modul                                  |
| `apps/api-rs/crates/api/src/routes/service.rs`  | `gate_member`/`gate_writer` → `pub(crate)`        |
| `apps/api-rs/crates/api/src/main.rs`            | registrasi route                                  |
| `apps/api-rs/crates/api/tests/war_room_test.rs` | test integrasi DB                                 |

---

### Task 1: Migrasi `0011_war_rooms.sql`

**Files:**

- Create: `apps/api-rs/migrations/0011_war_rooms.sql`

- [ ] **Step 1: Tulis migrasi**

Create `apps/api-rs/migrations/0011_war_rooms.sql`:

```sql
-- War room (incident command room): rooms, links, participants, chat,
-- runbook, activity feed. Delta applied at boot by `common::db::migrate`.

CREATE TABLE IF NOT EXISTS public.war_rooms (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    sequence_id bigint NOT NULL,
    name character varying(255) NOT NULL,
    description_html text NOT NULL DEFAULT '',
    notes_html text NOT NULL DEFAULT '',
    severity character varying(10) NOT NULL DEFAULT 'sev3',
    status character varying(20) NOT NULL DEFAULT 'active',
    primary_issue_id uuid NOT NULL REFERENCES public.issues(id) ON DELETE CASCADE,
    started_at timestamp with time zone NOT NULL DEFAULT now(),
    resolved_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS war_rooms_project_sequence_idx
    ON public.war_rooms (project_id, sequence_id);

CREATE INDEX IF NOT EXISTS war_rooms_project_status_idx
    ON public.war_rooms (project_id, status) WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS war_rooms_one_active_per_issue_idx
    ON public.war_rooms (project_id, primary_issue_id)
    WHERE status IN ('active', 'monitoring') AND deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_services (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS war_room_services_pair_idx
    ON public.war_room_services (war_room_id, service_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_issues (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    issue_id uuid NOT NULL REFERENCES public.issues(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS war_room_issues_pair_idx
    ON public.war_room_issues (war_room_id, issue_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_participants (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    member_id uuid NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    role character varying(20) NOT NULL DEFAULT 'responder',
    joined_at timestamp with time zone NOT NULL DEFAULT now(),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS war_room_participants_member_idx
    ON public.war_room_participants (war_room_id, member_id) WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS war_room_participants_one_commander_idx
    ON public.war_room_participants (war_room_id)
    WHERE role = 'commander' AND deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_messages (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    author_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    body text NOT NULL,
    mentions jsonb NOT NULL DEFAULT '[]'::jsonb,
    edited_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS war_room_messages_room_idx
    ON public.war_room_messages (war_room_id, created_at DESC) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_runbook_items (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    title character varying(500) NOT NULL,
    sort_order double precision NOT NULL DEFAULT 65535,
    is_done boolean NOT NULL DEFAULT false,
    done_by_id uuid,
    done_at timestamp with time zone,
    template_key character varying(100),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS war_room_runbook_items_room_idx
    ON public.war_room_runbook_items (war_room_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.war_room_events (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    war_room_id uuid NOT NULL REFERENCES public.war_rooms(id) ON DELETE CASCADE,
    actor_id uuid,
    event_type character varying(50) NOT NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS war_room_events_room_idx
    ON public.war_room_events (war_room_id, created_at DESC);
```

- [ ] **Step 2: Apply ke DB dev (psql langsung, tanpa rebuild image)**

Run:

```bash
docker compose -f docker-compose-local.yml exec -T plane-db \
  psql -U plane -d plane < apps/api-rs/migrations/0011_war_rooms.sql
```

Expected: deretan `CREATE TABLE` / `CREATE INDEX`. Ini tidak mencatat baris `_sqlx_migrations`; saat container `api` dibangun ulang nanti, `sqlx::migrate!` akan menjalankan file yang sama lagi (semua statement `IF NOT EXISTS`, aman) lalu mencatatnya.

- [ ] **Step 3: Verifikasi tabel + index**

Run:

```bash
docker compose -f docker-compose-local.yml exec -T plane-db psql -U plane -d plane \
  -c "\d public.war_rooms" -c "\d public.war_room_participants" -c "\d public.war_room_events"
```

Expected: tiga deskripsi tabel; `war_rooms` memuat `war_rooms_project_sequence_idx`, `war_rooms_project_status_idx`, `war_rooms_one_active_per_issue_idx`.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/0011_war_rooms.sql
git commit -m "feat(api-rs): war room schema migration"
```

---

### Task 2: Pure helpers + gate reuse + unit tests

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/war_room.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/routes/service.rs:129-159` (gate jadi `pub(crate)`)

- [ ] **Step 1: Register modul**

Di `apps/api-rs/crates/api/src/routes/mod.rs`, tambahkan setelah `pub mod view;`:

```rust
pub mod war_room;
```

- [ ] **Step 2: Buka gate service untuk dipakai bersama**

Di `apps/api-rs/crates/api/src/routes/service.rs`, ubah signature dua fungsi (baris ~129 dan ~144):

```rust
pub(crate) async fn gate_member(
```

```rust
pub(crate) async fn gate_writer(
```

(Body tidak berubah.)

- [ ] **Step 3: Tulis test yang gagal**

Create `apps/api-rs/crates/api/src/routes/war_room.rs` berisi HANYA ini:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_mapping_follows_priority() {
        assert_eq!(severity_from_priority("urgent"), "sev1");
        assert_eq!(severity_from_priority("high"), "sev2");
        assert_eq!(severity_from_priority("medium"), "sev3");
        assert_eq!(severity_from_priority("low"), "sev4");
        assert_eq!(severity_from_priority("none"), "sev4");
        assert_eq!(severity_from_priority("bogus"), "sev4");
    }

    #[test]
    fn transition_map_is_terminal_on_archived() {
        assert!(status_transition_allowed("active", "monitoring"));
        assert!(status_transition_allowed("monitoring", "active"));
        assert!(status_transition_allowed("active", "resolved"));
        assert!(status_transition_allowed("monitoring", "resolved"));
        assert!(status_transition_allowed("resolved", "active"));
        assert!(status_transition_allowed("resolved", "archived"));
        assert!(status_transition_allowed("active", "archived"));
        assert!(!status_transition_allowed("monitoring", "monitoring"));
        assert!(!status_transition_allowed("archived", "active"));
        assert!(!status_transition_allowed("archived", "resolved"));
        assert!(!status_transition_allowed("resolved", "monitoring"));
    }

    #[test]
    fn runbook_template_matches_type_name_case_insensitively() {
        let incident = runbook_template(Some("Incident"));
        assert_eq!(incident.len(), 5);
        assert_eq!(incident[0].0, "triage");
        assert_eq!(incident[4].0, "postmortem");
        assert_eq!(runbook_template(Some(" problem ")).len(), 5);
        assert_eq!(runbook_template(Some("Change")).len(), 5);
        assert_eq!(runbook_template(Some("Request")).len(), 4);
        assert!(runbook_template(Some("Custom type")).is_empty());
        assert!(runbook_template(None).is_empty());
    }

    #[test]
    fn active_status_check() {
        assert!(is_active_status("active"));
        assert!(is_active_status("monitoring"));
        assert!(!is_active_status("resolved"));
        assert!(!is_active_status("archived"));
    }
}
```

- [ ] **Step 4: Run test untuk memastikan gagal**

Run: `cargo test -p api --lib war_room::tests` (dari `apps/api-rs`)
Expected: FAIL compile — `severity_from_priority`, `status_transition_allowed`, `runbook_template`, `is_active_status` tidak ada.

- [ ] **Step 5: Implementasi helper (di atas modul test)**

Tambahkan ke `apps/api-rs/crates/api/src/routes/war_room.rs` (tanpa import baru — helper murni tidak butuh dependency; import ditambahkan di Task 3):

```rust
/// Allowed `status` values (`packages/types/src/war-room/core.ts`).
pub const WAR_ROOM_STATUSES: &[&str] = &["active", "monitoring", "resolved", "archived"];
/// Allowed `severity` values.
pub const WAR_ROOM_SEVERITIES: &[&str] = &["sev1", "sev2", "sev3", "sev4"];
/// Allowed participant roles (coordination labels, not permission gates).
pub const PARTICIPANT_ROLES: &[&str] = &["commander", "comms", "scribe", "responder"];

/// Default severity derived from the incident priority.
pub fn severity_from_priority(priority: &str) -> &'static str {
    match priority {
        "urgent" => "sev1",
        "high" => "sev2",
        "medium" => "sev3",
        _ => "sev4",
    }
}

pub fn is_active_status(status: &str) -> bool {
    status == "active" || status == "monitoring"
}

/// Allowed status transitions; `archived` is terminal.
pub fn transitions_allowed(status: &str) -> &'static [&'static str] {
    match status {
        "active" => &["monitoring", "resolved", "archived"],
        "monitoring" => &["active", "resolved", "archived"],
        "resolved" => &["active", "archived"],
        _ => &[],
    }
}

pub fn status_transition_allowed(from: &str, to: &str) -> bool {
    transitions_allowed(from).contains(&to)
}

/// Built-in runbook per work item type name (lowercased, trimmed).
/// Returns `(template_key, title)` pairs.
pub fn runbook_template(type_name: Option<&str>) -> Vec<(&'static str, &'static str)> {
    match type_name.map(|n| n.trim().to_lowercase()).as_deref() {
        Some("incident") => vec![
            ("triage", "Triage & assess impact"),
            ("mitigate", "Mitigate (rollback/redeploy)"),
            ("communicate", "Communicate status update"),
            ("verify", "Verify recovery & monitor"),
            ("postmortem", "Schedule postmortem"),
        ],
        Some("problem") => vec![
            ("confirm_cause", "Confirm root cause hypothesis"),
            ("evidence", "Collect evidence & timeline"),
            ("fix", "Identify permanent fix"),
            ("change_plan", "Create change plan"),
            ("knowledge", "Update knowledge base"),
        ],
        Some("change") => vec![
            ("pre_verify", "Pre-change verification"),
            ("execute", "Execute change steps"),
            ("validate", "Validate service health"),
            ("rollback", "Rollback if needed"),
            ("close", "Close change record"),
        ],
        Some("request") => vec![
            ("requester", "Confirm requester details"),
            ("steps", "Check fulfilment steps"),
            ("fulfil", "Execute fulfilment"),
            ("notify", "Notify requester"),
        ],
        _ => vec![],
    }
}
```

- [ ] **Step 6: Run test untuk memastikan lulus**

Run: `cargo test -p api --lib war_room::tests`
Expected: PASS (4 tests).

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/mod.rs \
  apps/api-rs/crates/api/src/routes/service.rs \
  apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room helpers, gates shared with services"
```

---

### Task 3: Row types, serializer, list, summary

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan imports + row structs + serializer**

Sisipkan blok import + row types + serializer di **paling atas** `war_room.rs` (di atas constants helper Task 2); `#[cfg(test)] mod tests` tetap di paling bawah.

```rust
use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{middleware::auth::AuthUser, routes::project::deny, state::AppState};

use super::service::{bad_request, gate_member, validate_enum};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WarRoomRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub sequence_id: i64,
    pub name: String,
    pub description_html: String,
    pub notes_html: String,
    pub severity: String,
    pub status: String,
    pub primary_issue_id: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

const WAR_ROOM_SELECT: &str = "SELECT r.id, r.workspace_id, r.project_id, r.sequence_id, \
    r.name, r.description_html, r.notes_html, r.severity, r.status, r.primary_issue_id, \
    r.started_at, r.resolved_at, r.created_at, r.updated_at, r.created_by_id, r.updated_by_id \
    FROM war_rooms r";

#[derive(Debug, Clone, sqlx::FromRow)]
struct RoomServiceRow {
    id: Uuid,
    name: String,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LinkedIssueRow {
    id: Uuid,
    identifier: String,
    name: String,
    priority: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ParticipantRow {
    pub id: Uuid,
    pub member_id: Uuid,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunbookItemRow {
    pub id: Uuid,
    pub title: String,
    pub sort_order: f64,
    pub is_done: bool,
    pub done_by_id: Option<Uuid>,
    pub done_at: Option<chrono::DateTime<chrono::Utc>>,
    pub template_key: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WarRoomEventRow {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,
    pub event_type: String,
    pub payload: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub fn room_base_json(r: &WarRoomRow) -> Value {
    serde_json::json!({
        "id": r.id,
        "workspace_id": r.workspace_id,
        "project_id": r.project_id,
        "sequence_id": r.sequence_id,
        "name": r.name,
        "description_html": r.description_html,
        "notes_html": r.notes_html,
        "severity": r.severity,
        "status": r.status,
        "primary_issue_id": r.primary_issue_id,
        "started_at": r.started_at,
        "resolved_at": r.resolved_at,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": r.created_by_id,
    })
}

fn service_json(r: &RoomServiceRow) -> Value {
    serde_json::json!({ "id": r.id, "name": r.name, "status": r.status })
}

fn linked_issue_json(r: &LinkedIssueRow) -> Value {
    serde_json::json!({
        "id": r.id, "identifier": r.identifier, "name": r.name, "priority": r.priority
    })
}

pub fn participant_json(r: &ParticipantRow) -> Value {
    serde_json::json!({
        "id": r.id, "member_id": r.member_id, "role": r.role, "joined_at": r.joined_at,
        "display_name": r.display_name, "avatar_url": r.avatar_url,
    })
}

pub fn runbook_item_json(r: &RunbookItemRow) -> Value {
    serde_json::json!({
        "id": r.id, "title": r.title, "sort_order": r.sort_order, "is_done": r.is_done,
        "done_by_id": r.done_by_id, "done_at": r.done_at, "template_key": r.template_key,
    })
}

pub fn event_json(r: &WarRoomEventRow) -> Value {
    serde_json::json!({
        "id": r.id, "actor_id": r.actor_id, "event_type": r.event_type,
        "payload": r.payload, "created_at": r.created_at,
    })
}

async fn fetch_room(pool: &PgPool, project_id: Uuid, pk: Uuid) -> Result<Option<WarRoomRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{WAR_ROOM_SELECT} WHERE r.id = $1 AND r.project_id = $2 AND r.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

async fn room_services(pool: &PgPool, room_id: Uuid) -> Result<Vec<RoomServiceRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT s.id, s.name, s.status FROM war_room_services l \
         JOIN services s ON s.id = l.service_id \
         WHERE l.war_room_id = $1 AND l.deleted_at IS NULL AND s.deleted_at IS NULL \
         ORDER BY s.name ASC",
    )
    .bind(room_id)
    .fetch_all(pool)
    .await
}

async fn room_issues(pool: &PgPool, room_id: Uuid) -> Result<Vec<LinkedIssueRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.id, p.identifier || '-' || i.sequence_id AS identifier, i.name, i.priority \
         FROM war_room_issues l JOIN issues i ON i.id = l.issue_id \
         JOIN projects p ON p.id = i.project_id \
         WHERE l.war_room_id = $1 AND l.deleted_at IS NULL AND i.deleted_at IS NULL \
         ORDER BY l.created_at ASC",
    )
    .bind(room_id)
    .fetch_all(pool)
    .await
}

pub async fn room_participants(pool: &PgPool, room_id: Uuid) -> Result<Vec<ParticipantRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.id, p.member_id, p.role, p.joined_at, u.display_name, \
         CASE WHEN u.avatar_asset_id IS NOT NULL \
           THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
         FROM war_room_participants p JOIN users u ON u.id = p.member_id \
         WHERE p.war_room_id = $1 AND p.deleted_at IS NULL ORDER BY p.joined_at ASC",
    )
    .bind(room_id)
    .fetch_all(pool)
    .await
}

pub async fn room_runbook(pool: &PgPool, room_id: Uuid) -> Result<Vec<RunbookItemRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, title, sort_order, is_done, done_by_id, done_at, template_key \
         FROM war_room_runbook_items WHERE war_room_id = $1 AND deleted_at IS NULL \
         ORDER BY sort_order ASC, created_at ASC",
    )
    .bind(room_id)
    .fetch_all(pool)
    .await
}

async fn message_count(pool: &PgPool, room_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM war_room_messages WHERE war_room_id = $1 AND deleted_at IS NULL",
    )
    .bind(room_id)
    .fetch_one(pool)
    .await
}

async fn primary_issue_json(pool: &PgPool, project_id: Uuid, issue_id: Uuid) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<(Uuid, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT i.id, p.identifier || '-' || i.sequence_id, i.name, i.priority, s.\"group\" \
         FROM issues i JOIN projects p ON p.id = i.project_id \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.project_id = $2 AND i.deleted_at IS NULL",
    )
    .bind(issue_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, identifier, name, priority, state_group)| {
        serde_json::json!({
            "id": id, "identifier": identifier, "name": name,
            "priority": priority, "state_group": state_group,
        })
    }))
}

pub async fn room_detail_json(pool: &PgPool, room: &WarRoomRow) -> Result<Value, sqlx::Error> {
    let services = room_services(pool, room.id).await?;
    let issues = room_issues(pool, room.id).await?;
    let participants = room_participants(pool, room.id).await?;
    let runbook = room_runbook(pool, room.id).await?;
    let messages = message_count(pool, room.id).await?;
    let primary_issue = primary_issue_json(pool, room.project_id, room.primary_issue_id).await?;
    let mut json = room_base_json(room);
    json["primary_issue"] = primary_issue.unwrap_or(Value::Null);
    json["services"] = Value::Array(services.iter().map(service_json).collect());
    json["issues"] = Value::Array(issues.iter().map(linked_issue_json).collect());
    json["participants"] = Value::Array(participants.iter().map(participant_json).collect());
    json["runbook_items"] = Value::Array(runbook.iter().map(runbook_item_json).collect());
    json["counts"] = serde_json::json!({ "messages": messages });
    Ok(json)
}

// ---------------------------------------------------------------------------
// List + summary
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub severity: Option<String>,
    pub q: Option<String>,
}

fn parse_csv(value: &Option<String>) -> Vec<String> {
    value
        .as_deref()
        .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

fn rooms_by_room_id<T>(rows: Vec<(Uuid, T)>) -> HashMap<Uuid, Vec<T>> {
    let mut map: HashMap<Uuid, Vec<T>> = HashMap::new();
    for (room_id, value) in rows {
        map.entry(room_id).or_default().push(value);
    }
    map
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Query(params): Query<ListParams>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let statuses = parse_csv(&params.status);
    for s in &statuses {
        if let Err(e) = validate_enum("status", s, WAR_ROOM_STATUSES) {
            return Ok(bad_request(e));
        }
    }
    let severities = parse_csv(&params.severity);
    for s in &severities {
        if let Err(e) = validate_enum("severity", s, WAR_ROOM_SEVERITIES) {
            return Ok(bad_request(e));
        }
    }
    let q = params.q.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let status_filter = if statuses.is_empty() { None } else { Some(statuses) };
    let severity_filter = if severities.is_empty() { None } else { Some(severities) };

    let rooms: Vec<WarRoomRow> = sqlx::query_as(&format!(
        "{WAR_ROOM_SELECT} WHERE r.project_id = $1 AND r.deleted_at IS NULL \
         AND ($2::text[] IS NULL OR r.status = ANY($2)) \
         AND ($3::text[] IS NULL OR r.severity = ANY($3)) \
         AND ($4::text IS NULL OR r.name ILIKE '%' || $4 || '%' OR EXISTS ( \
            SELECT 1 FROM issues i JOIN projects p ON p.id = i.project_id \
            WHERE i.id = r.primary_issue_id \
              AND (i.name ILIKE '%' || $4 || '%' OR (p.identifier || '-' || i.sequence_id) ILIKE '%' || $4 || '%') \
         )) \
         ORDER BY array_position(ARRAY['active','monitoring','resolved','archived'], r.status), \
                  r.severity ASC, r.started_at DESC"
    ))
    .bind(project_id)
    .bind(&status_filter)
    .bind(&severity_filter)
    .bind(&q)
    .fetch_all(&st.pool)
    .await?;

    let room_ids: Vec<Uuid> = rooms.iter().map(|r| r.id).collect();
    let services: HashMap<Uuid, Vec<RoomServiceRow>> = rooms_by_room_id(
        sqlx::query_as::<_, (Uuid, Uuid, String, String)>(
            "SELECT l.war_room_id, s.id, s.name, s.status FROM war_room_services l \
             JOIN services s ON s.id = l.service_id \
             WHERE l.war_room_id = ANY($1) AND l.deleted_at IS NULL AND s.deleted_at IS NULL \
             ORDER BY s.name ASC",
        )
        .bind(&room_ids)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|(room_id, id, name, status)| (room_id, RoomServiceRow { id, name, status }))
        .collect::<Vec<_>>(),
    );
    let primary_issues: HashMap<Uuid, Value> = sqlx::query_as::<_, (Uuid, Uuid, String, String, String, Option<String>)>(
        "SELECT r.id, i.id, p.identifier || '-' || i.sequence_id, i.name, i.priority, s.\"group\" \
         FROM war_rooms r JOIN issues i ON i.id = r.primary_issue_id \
         JOIN projects p ON p.id = i.project_id \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE r.id = ANY($1) AND i.deleted_at IS NULL",
    )
    .bind(&room_ids)
    .fetch_all(&st.pool)
    .await?
    .into_iter()
    .map(|(room_id, issue_id, identifier, name, priority, state_group)| {
        (
            room_id,
            serde_json::json!({
                "id": issue_id, "identifier": identifier, "name": name,
                "priority": priority, "state_group": state_group,
            }),
        )
    })
    .collect();
    let participants: HashMap<Uuid, Vec<ParticipantRow>> = rooms_by_room_id(
        sqlx::query_as(
            "SELECT p.war_room_id, p.id, p.member_id, p.role, p.joined_at, u.display_name, \
             CASE WHEN u.avatar_asset_id IS NOT NULL \
               THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
             FROM war_room_participants p JOIN users u ON u.id = p.member_id \
             WHERE p.war_room_id = ANY($1) AND p.deleted_at IS NULL ORDER BY p.joined_at ASC",
        )
        .bind(&room_ids)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|row: ParticipantListRow| {
            let room_id = row.war_room_id;
            (
                room_id,
                ParticipantRow {
                    id: row.id,
                    member_id: row.member_id,
                    role: row.role,
                    joined_at: row.joined_at,
                    display_name: row.display_name,
                    avatar_url: row.avatar_url,
                },
            )
        })
        .collect::<Vec<_>>(),
    );
    let message_counts: HashMap<Uuid, i64> = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT war_room_id, COUNT(*) FROM war_room_messages \
         WHERE war_room_id = ANY($1) AND deleted_at IS NULL GROUP BY war_room_id",
    )
    .bind(&room_ids)
    .fetch_all(&st.pool)
    .await?
    .into_iter()
    .collect();
    let last_activity: HashMap<Uuid, chrono::DateTime<chrono::Utc>> = sqlx::query_as::<_, (Uuid, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT room_id, MAX(ts) FROM ( \
            SELECT war_room_id AS room_id, MAX(created_at) AS ts FROM war_room_messages \
            WHERE war_room_id = ANY($1) AND deleted_at IS NULL GROUP BY war_room_id \
            UNION ALL \
            SELECT war_room_id, MAX(created_at) FROM war_room_events \
            WHERE war_room_id = ANY($1) GROUP BY war_room_id \
         ) x GROUP BY room_id",
    )
    .bind(&room_ids)
    .fetch_all(&st.pool)
    .await?
    .into_iter()
    .filter_map(|(room_id, ts)| ts.map(|t| (room_id, t)))
    .collect();

    let items: Vec<Value> = rooms
        .iter()
        .map(|r| {
            let svc = services.get(&r.id).cloned().unwrap_or_default();
            let parts = participants.get(&r.id).cloned().unwrap_or_default();
            let mut json = room_base_json(r);
            json["primary_issue"] = primary_issues.get(&r.id).cloned().unwrap_or(Value::Null);
            json["services"] = Value::Array(svc.iter().map(service_json).collect());
            json["participants"] = Value::Array(parts.iter().map(participant_json).collect());
            json["service_count"] = serde_json::json!(svc.len());
            json["participant_count"] = serde_json::json!(parts.len());
            json["message_count"] = serde_json::json!(message_counts.get(&r.id).copied().unwrap_or(0));
            json["last_activity_at"] = serde_json::json!(last_activity.get(&r.id));
            json
        })
        .collect();
    Ok((StatusCode::OK, Json(Value::Array(items))))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ParticipantListRow {
    war_room_id: Uuid,
    id: Uuid,
    member_id: Uuid,
    role: String,
    joined_at: chrono::DateTime<chrono::Utc>,
    display_name: Option<String>,
    avatar_url: Option<String>,
}

pub async fn summary(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
           COUNT(*) FILTER (WHERE status IN ('active','monitoring')), \
           COUNT(*) FILTER (WHERE status IN ('active','monitoring') AND severity IN ('sev1','sev2')), \
           COUNT(*) FILTER (WHERE status = 'resolved' AND resolved_at >= now() - interval '7 days') \
         FROM war_rooms WHERE project_id = $1 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "active": row.0, "sev1_2": row.1, "resolved_7d": row.2
        })),
    ))
}
```

- [ ] **Step 2: Buka `bad_request` service agar dipakai bersama**

Di `apps/api-rs/crates/api/src/routes/service.rs`, ubah `fn bad_request(...)` menjadi `pub(crate) fn bad_request(...)`. Body tidak berubah. `war_room.rs` memakainya lewat `use super::service::bad_request;` (sudah ada di Step 1).

- [ ] **Step 3: Verifikasi compile**

Run: `cargo check -p api`
Expected: LULUS. Warning `room_detail_json`/`fetch_room` belum terpakai boleh muncul (hilang di Task 4; crate tidak `deny(warnings)`).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs apps/api-rs/crates/api/src/routes/service.rs
git commit -m "feat(api-rs): war room list and summary handlers"
```

---

### Task 4: Create handler (transaksi, sequence, seed)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan request struct + issue summary fetch + event recorder**

Tambahkan sebelum `#[cfg(test)]`:

```rust
#[derive(Debug, Deserialize)]
pub struct CreateWarRoom {
    pub name: Option<String>,
    pub primary_issue_id: Uuid,
    pub severity: Option<String>,
    pub description_html: Option<String>,
    pub service_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IssueSummaryRow {
    pub id: Uuid,
    pub name: String,
    pub priority: String,
    pub type_name: Option<String>,
}

async fn fetch_issue_summary(
    pool: &PgPool,
    project_id: Uuid,
    issue_id: Uuid,
) -> Result<Option<IssueSummaryRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.id, i.name, i.priority, t.name AS type_name \
         FROM issues i LEFT JOIN issue_types t ON t.id = i.type_id AND t.deleted_at IS NULL \
         WHERE i.id = $1 AND i.project_id = $2 AND i.deleted_at IS NULL",
    )
    .bind(issue_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

async fn issue_assignees(pool: &PgPool, issue_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT assignee_id FROM issue_assignees \
         WHERE issue_id = $1 AND deleted_at IS NULL ORDER BY created_at ASC",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
}

pub async fn record_event(
    pool: &PgPool,
    room: &WarRoomRow,
    actor: Uuid,
    event_type: &str,
    payload: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO war_room_events (id, workspace_id, project_id, war_room_id, actor_id, \
         event_type, payload, created_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, now())",
    )
    .bind(room.workspace_id)
    .bind(room.project_id)
    .bind(room.id)
    .bind(actor)
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}

async fn record_event_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    room: &WarRoomRow,
    actor: Uuid,
    event_type: &str,
    payload: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO war_room_events (id, workspace_id, project_id, war_room_id, actor_id, \
         event_type, payload, created_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, now())",
    )
    .bind(room.workspace_id)
    .bind(room.project_id)
    .bind(room.id)
    .bind(actor)
    .bind(event_type)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
```

- [ ] **Step 2: Tambahkan handler `create`**

```rust
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateWarRoom>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(issue) = fetch_issue_summary(&st.pool, project_id, body.primary_issue_id).await? else {
        return Ok(bad_request("Invalid primary_issue_id - object does not exist."));
    };
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM war_rooms WHERE project_id = $1 AND primary_issue_id = $2 \
         AND status IN ('active','monitoring') AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .bind(body.primary_issue_id)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(existing_id) = existing {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "active_war_room_exists", "war_room_id": existing_id
            })),
        ));
    }
    let name = body
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| issue.name.clone());
    let severity = body
        .severity
        .clone()
        .unwrap_or_else(|| severity_from_priority(&issue.priority).to_string());
    if let Err(e) = validate_enum("severity", &severity, WAR_ROOM_SEVERITIES) {
        return Ok(bad_request(e));
    }
    let service_ids = body.service_ids.clone().unwrap_or_default();
    if !service_ids.is_empty() {
        let valid: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM services WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(&service_ids)
        .fetch_one(&st.pool)
        .await?;
        if valid as usize != service_ids.len() {
            return Ok(bad_request("Invalid service_ids - object does not exist."));
        }
    }
    let assignees = issue_assignees(&st.pool, body.primary_issue_id).await?;
    let room_id = Uuid::new_v4();

    let mut tx = st.pool.begin().await?;
    sqlx::query("SELECT id FROM projects WHERE id = $1 FOR UPDATE")
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await?;
    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM war_rooms WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await?;
    let workspace_id: Uuid = sqlx::query_scalar("SELECT workspace_id FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO war_rooms (id, workspace_id, project_id, sequence_id, name, description_html, \
         notes_html, severity, status, primary_issue_id, started_at, created_at, updated_at, \
         created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, '', $7, 'active', $8, now(), now(), now(), $9, $9)",
    )
    .bind(room_id)
    .bind(workspace_id)
    .bind(project_id)
    .bind(sequence)
    .bind(&name)
    .bind(body.description_html.clone().unwrap_or_default())
    .bind(&severity)
    .bind(body.primary_issue_id)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;

    let room = WarRoomRow {
        id: room_id,
        workspace_id,
        project_id,
        sequence_id: sequence,
        name: name.clone(),
        description_html: body.description_html.clone().unwrap_or_default(),
        notes_html: String::new(),
        severity: severity.clone(),
        status: "active".to_string(),
        primary_issue_id: body.primary_issue_id,
        started_at: chrono::Utc::now(),
        resolved_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        created_by_id: Some(auth.0),
        updated_by_id: Some(auth.0),
    };

    let mut initial_participants: Vec<(Uuid, &str)> = vec![(auth.0, "commander")];
    for assignee in assignees {
        if assignee != auth.0 {
            initial_participants.push((assignee, "responder"));
        }
    }
    for (member_id, role) in &initial_participants {
        sqlx::query(
            "INSERT INTO war_room_participants (id, workspace_id, project_id, war_room_id, \
             member_id, role, joined_at, created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now(), now(), $6, $6) \
             ON CONFLICT DO NOTHING",
        )
        .bind(workspace_id)
        .bind(project_id)
        .bind(room_id)
        .bind(*member_id)
        .bind(*role)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    for service_id in &service_ids {
        sqlx::query(
            "INSERT INTO war_room_services (id, workspace_id, project_id, war_room_id, service_id, \
             created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now(), $5, $5) \
             ON CONFLICT DO NOTHING",
        )
        .bind(workspace_id)
        .bind(project_id)
        .bind(room_id)
        .bind(service_id)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    let mut sort_order = 0.0_f64;
    for (template_key, title) in runbook_template(issue.type_name.as_deref()) {
        sort_order += 65535.0;
        sqlx::query(
            "INSERT INTO war_room_runbook_items (id, workspace_id, project_id, war_room_id, title, \
             sort_order, is_done, template_key, created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, false, $6, now(), now(), $7, $7)",
        )
        .bind(workspace_id)
        .bind(project_id)
        .bind(room_id)
        .bind(title)
        .bind(sort_order)
        .bind(template_key)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    record_event_tx(
        &mut tx,
        &room,
        auth.0,
        "room.created",
        serde_json::json!({
            "name": name, "severity": severity, "primary_issue_id": body.primary_issue_id
        }),
    )
    .await?;
    for (member_id, role) in &initial_participants {
        record_event_tx(
            &mut tx,
            &room,
            auth.0,
            "participant.joined",
            serde_json::json!({ "member_id": member_id, "role": role }),
        )
        .await?;
    }
    tx.commit().await?;

    let row = fetch_room(&st.pool, project_id, room_id)
        .await?
        .expect("room just inserted");
    let detail = room_detail_json(&st.pool, &row).await?;
    Ok((StatusCode::CREATED, Json(detail)))
}
```

- [ ] **Step 3: Tambahkan import `gate_writer`**

Handler `create` memakai `gate_writer`; tambahkan ke import yang sudah ada di Task 3:

```rust
use super::service::{bad_request, gate_member, gate_writer, validate_enum};
```

(`fetch_project_member_role`/`is_workspace_admin`/`missing` baru dipakai di Task 7 — importnya ditambahkan di sana.)

- [ ] **Step 4: Verifikasi compile**

Run: `cargo check -p api`
Expected: LULUS tanpa error (warning dead code hilang untuk `room_detail_json`, karena sudah dipakai `create`).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room create handler with sequence and seeds"
```

---

### Task 5: Patch (transisi status) + destroy

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan `PatchWarRoom` + handler `detail`, `patch`, `destroy`**

```rust
#[derive(Debug, Deserialize)]
pub struct PatchWarRoom {
    pub name: Option<String>,
    pub severity: Option<String>,
    pub status: Option<String>,
    pub description_html: Option<String>,
    pub notes_html: Option<String>,
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(room) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "War room not found"}))));
    };
    let detail = room_detail_json(&st.pool, &room).await?;
    Ok((StatusCode::OK, Json(detail)))
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchWarRoom>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(current) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "War room not found"}))));
    };
    if current.status == "archived" {
        return Ok((StatusCode::CONFLICT, Json(serde_json::json!({"error": "room_archived"}))));
    }
    let name = body.name.clone().unwrap_or_else(|| current.name.clone());
    if name.trim().is_empty() {
        return Ok(bad_request("Invalid name"));
    }
    let severity = body.severity.clone().unwrap_or_else(|| current.severity.clone());
    if let Err(e) = validate_enum("severity", &severity, WAR_ROOM_SEVERITIES) {
        return Ok(bad_request(e));
    }
    let status = body.status.clone().unwrap_or_else(|| current.status.clone());
    if let Err(e) = validate_enum("status", &status, WAR_ROOM_STATUSES) {
        return Ok(bad_request(e));
    }
    if status != current.status && !status_transition_allowed(&current.status, &status) {
        return Ok(bad_request("invalid_status_transition"));
    }
    let description_html = body
        .description_html
        .clone()
        .unwrap_or_else(|| current.description_html.clone());
    let notes_html = body.notes_html.clone().unwrap_or_else(|| current.notes_html.clone());
    let resolved_at = match status.as_str() {
        "resolved" => current.resolved_at.or(Some(chrono::Utc::now())),
        "active" | "monitoring" => None,
        _ => current.resolved_at,
    };
    sqlx::query(
        "UPDATE war_rooms SET name = $1, severity = $2, status = $3, description_html = $4, \
         notes_html = $5, resolved_at = $6, updated_at = now(), updated_by_id = $7 WHERE id = $8",
    )
    .bind(&name)
    .bind(&severity)
    .bind(&status)
    .bind(&description_html)
    .bind(&notes_html)
    .bind(resolved_at)
    .bind(auth.0)
    .bind(pk)
    .execute(&st.pool)
    .await?;

    if severity != current.severity {
        record_event(
            &st.pool,
            &current,
            auth.0,
            "room.severity_changed",
            serde_json::json!({ "from": current.severity, "to": severity }),
        )
        .await?;
    }
    if status != current.status {
        record_event(
            &st.pool,
            &current,
            auth.0,
            "room.status_changed",
            serde_json::json!({ "from": current.status, "to": status }),
        )
        .await?;
        let specific = match status.as_str() {
            "resolved" => Some("room.resolved"),
            "active" if current.status == "resolved" => Some("room.reopened"),
            "archived" => Some("room.archived"),
            _ => None,
        };
        if let Some(event_type) = specific {
            record_event(&st.pool, &current, auth.0, event_type, serde_json::json!({})).await?;
        }
    }
    let row = fetch_room(&st.pool, project_id, pk)
        .await?
        .expect("room just updated");
    let detail = room_detail_json(&st.pool, &row).await?;
    Ok((StatusCode::OK, Json(detail)))
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE war_room_messages SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_room_runbook_items SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_room_participants SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_room_services SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_room_issues SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE war_rooms SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 2: Verifikasi compile**

Run: `cargo check -p api`
Expected: LULUS.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room detail, status transitions, soft delete"
```

---

### Task 6: Link handlers (services + issues)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan request struct + handler**

```rust
#[derive(Debug, Deserialize)]
pub struct LinkServices {
    pub service_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct LinkIssues {
    pub issue_ids: Vec<Uuid>,
}

async fn room_for_write(
    st: &AppState,
    slug: &str,
    project_id: Uuid,
    pk: Uuid,
    user: Uuid,
) -> Result<Result<WarRoomRow, (StatusCode, Json<Value>)>, common::errors::AppError> {
    if !gate_writer(&st.pool, user, slug, project_id).await? {
        return Ok(Err(deny()));
    }
    let Some(room) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok(Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "War room not found"})),
        )));
    };
    if room.status == "archived" {
        return Ok(Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "room_archived"})),
        )));
    }
    Ok(Ok(room))
}

pub async fn services_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<LinkServices>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    if !body.service_ids.is_empty() {
        let valid: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM services WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(&body.service_ids)
        .fetch_one(&st.pool)
        .await?;
        if valid as usize != body.service_ids.len() {
            return Ok(bad_request("Invalid service_ids - object does not exist."));
        }
    }
    let mut linked = 0_u64;
    for service_id in &body.service_ids {
        let result = sqlx::query(
            "INSERT INTO war_room_services (id, workspace_id, project_id, war_room_id, service_id, \
             created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now(), $5, $5) \
             ON CONFLICT DO NOTHING",
        )
        .bind(room.workspace_id)
        .bind(project_id)
        .bind(room.id)
        .bind(service_id)
        .bind(auth.0)
        .execute(&st.pool)
        .await?;
        linked += result.rows_affected();
    }
    if linked > 0 {
        record_event(
            &st.pool,
            &room,
            auth.0,
            "service.linked",
            serde_json::json!({ "service_ids": body.service_ids }),
        )
        .await?;
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "linked": linked }))))
}

pub async fn services_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, service_id)): Path<(String, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let result = sqlx::query(
        "UPDATE war_room_services SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND service_id = $3 AND deleted_at IS NULL",
    )
    .bind(room.id)
    .bind(project_id)
    .bind(service_id)
    .execute(&st.pool)
    .await?;
    if result.rows_affected() > 0 {
        record_event(
            &st.pool,
            &room,
            auth.0,
            "service.unlinked",
            serde_json::json!({ "service_id": service_id }),
        )
        .await?;
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

pub async fn issues_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<LinkIssues>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    if body.issue_ids.contains(&room.primary_issue_id) {
        return Ok(bad_request("primary_issue_not_linkable"));
    }
    if !body.issue_ids.is_empty() {
        let valid: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM issues WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(&body.issue_ids)
        .fetch_one(&st.pool)
        .await?;
        if valid as usize != body.issue_ids.len() {
            return Ok(bad_request("Invalid issue_ids - object does not exist."));
        }
    }
    let mut linked = 0_u64;
    for issue_id in &body.issue_ids {
        let result = sqlx::query(
            "INSERT INTO war_room_issues (id, workspace_id, project_id, war_room_id, issue_id, \
             created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now(), $5, $5) \
             ON CONFLICT DO NOTHING",
        )
        .bind(room.workspace_id)
        .bind(project_id)
        .bind(room.id)
        .bind(issue_id)
        .bind(auth.0)
        .execute(&st.pool)
        .await?;
        linked += result.rows_affected();
    }
    if linked > 0 {
        record_event(
            &st.pool,
            &room,
            auth.0,
            "issue.linked",
            serde_json::json!({ "issue_ids": body.issue_ids }),
        )
        .await?;
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "linked": linked }))))
}

pub async fn issues_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, issue_id)): Path<(String, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let result = sqlx::query(
        "UPDATE war_room_issues SET deleted_at = now(), updated_at = now() \
         WHERE war_room_id = $1 AND project_id = $2 AND issue_id = $3 AND deleted_at IS NULL",
    )
    .bind(room.id)
    .bind(project_id)
    .bind(issue_id)
    .execute(&st.pool)
    .await?;
    if result.rows_affected() > 0 {
        record_event(
            &st.pool,
            &room,
            auth.0,
            "issue.unlinked",
            serde_json::json!({ "issue_id": issue_id }),
        )
        .await?;
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 2: Verifikasi compile + unit test**

Run: `cargo test -p api --lib war_room::tests`
Expected: PASS (4 tests).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room service and work-item link handlers"
```

---

### Task 7: Participant handlers

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan request struct + handler**

```rust
#[derive(Debug, Deserialize)]
pub struct ParticipantCreate {
    pub member_id: Uuid,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ParticipantPatch {
    pub role: String,
}

async fn participant_by_id(
    pool: &PgPool,
    room_id: Uuid,
    participant_id: Uuid,
) -> Result<Option<ParticipantRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.id, p.member_id, p.role, p.joined_at, u.display_name, \
         CASE WHEN u.avatar_asset_id IS NOT NULL \
           THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
         FROM war_room_participants p JOIN users u ON u.id = p.member_id \
         WHERE p.id = $1 AND p.war_room_id = $2 AND p.deleted_at IS NULL",
    )
    .bind(participant_id)
    .bind(room_id)
    .fetch_optional(pool)
    .await
}

pub async fn participants_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<ParticipantCreate>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let role = body.role.clone().unwrap_or_else(|| "responder".to_string());
    if let Err(e) = validate_enum("role", &role, PARTICIPANT_ROLES) {
        return Ok(bad_request(e));
    }
    let project_role = fetch_project_member_role(&st.pool, body.member_id, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, body.member_id, &slug).await?;
    if project_role.is_none() && !ws_admin {
        return Ok(bad_request("Invalid member_id - not a project member."));
    }
    let mut tx = st.pool.begin().await?;
    if role == "commander" {
        sqlx::query(
            "UPDATE war_room_participants SET role = 'responder', updated_at = now(), updated_by_id = $1 \
             WHERE war_room_id = $2 AND role = 'commander' AND member_id != $3 AND deleted_at IS NULL",
        )
        .bind(auth.0)
        .bind(room.id)
        .bind(body.member_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO war_room_participants (id, workspace_id, project_id, war_room_id, member_id, \
         role, joined_at, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now(), now(), $6, $6) \
         ON CONFLICT (war_room_id, member_id) WHERE deleted_at IS NULL \
         DO UPDATE SET role = EXCLUDED.role, updated_at = now(), updated_by_id = $6",
    )
    .bind(room.workspace_id)
    .bind(project_id)
    .bind(room.id)
    .bind(body.member_id)
    .bind(&role)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    record_event(
        &st.pool,
        &room,
        auth.0,
        "participant.joined",
        serde_json::json!({ "member_id": body.member_id, "role": role }),
    )
    .await?;
    let row = sqlx::query_as::<_, ParticipantRow>(
        "SELECT p.id, p.member_id, p.role, p.joined_at, u.display_name, \
         CASE WHEN u.avatar_asset_id IS NOT NULL \
           THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
         FROM war_room_participants p JOIN users u ON u.id = p.member_id \
         WHERE p.war_room_id = $1 AND p.member_id = $2 AND p.deleted_at IS NULL",
    )
    .bind(room.id)
    .bind(body.member_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(participant_json(&row))))
}

pub async fn participants_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, participant_id)): Path<(String, Uuid, Uuid, Uuid)>,
    Json(body): Json<ParticipantPatch>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    if let Err(e) = validate_enum("role", &body.role, PARTICIPANT_ROLES) {
        return Ok(bad_request(e));
    }
    let current = participant_by_id(&st.pool, room.id, participant_id).await?;
    let Some(current) = current else {
        return Ok(missing());
    };
    let mut tx = st.pool.begin().await?;
    if body.role == "commander" {
        sqlx::query(
            "UPDATE war_room_participants SET role = 'responder', updated_at = now(), updated_by_id = $1 \
             WHERE war_room_id = $2 AND role = 'commander' AND id != $3 AND deleted_at IS NULL",
        )
        .bind(auth.0)
        .bind(room.id)
        .bind(participant_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE war_room_participants SET role = $1, updated_at = now(), updated_by_id = $2 \
         WHERE id = $3 AND war_room_id = $4 AND deleted_at IS NULL",
    )
    .bind(&body.role)
    .bind(auth.0)
    .bind(participant_id)
    .bind(room.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    record_event(
        &st.pool,
        &room,
        auth.0,
        "participant.role_changed",
        serde_json::json!({
            "member_id": current.member_id, "from": current.role, "to": body.role
        }),
    )
    .await?;
    let row = participant_by_id(&st.pool, room.id, participant_id)
        .await?
        .expect("participant just updated");
    Ok((StatusCode::OK, Json(participant_json(&row))))
}

pub async fn participants_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, participant_id)): Path<(String, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let current = participant_by_id(&st.pool, room.id, participant_id).await?;
    let Some(current) = current else {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    };
    sqlx::query(
        "UPDATE war_room_participants SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND war_room_id = $2 AND deleted_at IS NULL",
    )
    .bind(participant_id)
    .bind(room.id)
    .execute(&st.pool)
    .await?;
    record_event(
        &st.pool,
        &room,
        auth.0,
        "participant.left",
        serde_json::json!({ "member_id": current.member_id }),
    )
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 2: Verifikasi compile**

Catatan import (bagian Step ini): tambahkan `use super::issue_common::{fetch_project_member_role, is_workspace_admin};` dan ubah import project menjadi `use crate::routes::project::{deny, missing};`.

Run: `cargo check -p api`
Expected: LULUS.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room participant handlers"
```

---

### Task 8: Runbook handlers

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan request struct + handler**

```rust
#[derive(Debug, Deserialize)]
pub struct RunbookCreate {
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct RunbookPatch {
    pub title: Option<String>,
    pub is_done: Option<bool>,
}

async fn runbook_item_by_id(
    pool: &PgPool,
    room_id: Uuid,
    item_id: Uuid,
) -> Result<Option<RunbookItemRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, title, sort_order, is_done, done_by_id, done_at, template_key \
         FROM war_room_runbook_items WHERE id = $1 AND war_room_id = $2 AND deleted_at IS NULL",
    )
    .bind(item_id)
    .bind(room_id)
    .fetch_optional(pool)
    .await
}

pub async fn runbook_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<RunbookCreate>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let title = body.title.trim();
    if title.is_empty() {
        return Ok(bad_request("Invalid title"));
    }
    let max_order: f64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), 0) FROM war_room_runbook_items \
         WHERE war_room_id = $1 AND deleted_at IS NULL",
    )
    .bind(room.id)
    .fetch_one(&st.pool)
    .await?;
    let item_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO war_room_runbook_items (id, workspace_id, project_id, war_room_id, title, \
         sort_order, is_done, template_key, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, false, NULL, now(), now(), $7, $7)",
    )
    .bind(item_id)
    .bind(room.workspace_id)
    .bind(project_id)
    .bind(room.id)
    .bind(title)
    .bind(max_order + 65535.0)
    .bind(auth.0)
    .execute(&st.pool)
    .await?;
    let row = runbook_item_by_id(&st.pool, room.id, item_id)
        .await?
        .expect("runbook item just inserted");
    Ok((StatusCode::CREATED, Json(runbook_item_json(&row))))
}

pub async fn runbook_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, item_id)): Path<(String, Uuid, Uuid, Uuid)>,
    Json(body): Json<RunbookPatch>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let Some(current) = runbook_item_by_id(&st.pool, room.id, item_id).await? else {
        return Ok(missing());
    };
    let title = match body.title.clone() {
        Some(t) if t.trim().is_empty() => return Ok(bad_request("Invalid title")),
        Some(t) => t,
        None => current.title.clone(),
    };
    let is_done = body.is_done.unwrap_or(current.is_done);
    let (done_by_id, done_at) = if is_done {
        (Some(auth.0), Some(chrono::Utc::now()))
    } else {
        (None, None)
    };
    sqlx::query(
        "UPDATE war_room_runbook_items SET title = $1, is_done = $2, done_by_id = $3, \
         done_at = $4, updated_at = now(), updated_by_id = $5 WHERE id = $6 AND war_room_id = $7",
    )
    .bind(&title)
    .bind(is_done)
    .bind(done_by_id)
    .bind(done_at)
    .bind(auth.0)
    .bind(item_id)
    .bind(room.id)
    .execute(&st.pool)
    .await?;
    if is_done != current.is_done {
        let event_type = if is_done { "runbook.item_done" } else { "runbook.item_reopened" };
        record_event(
            &st.pool,
            &room,
            auth.0,
            event_type,
            serde_json::json!({ "item_id": item_id, "title": title }),
        )
        .await?;
    }
    let row = runbook_item_by_id(&st.pool, room.id, item_id)
        .await?
        .expect("runbook item just updated");
    Ok((StatusCode::OK, Json(runbook_item_json(&row))))
}

pub async fn runbook_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, item_id)): Path<(String, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    sqlx::query(
        "UPDATE war_room_runbook_items SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND war_room_id = $2 AND deleted_at IS NULL",
    )
    .bind(item_id)
    .bind(room.id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 2: Verifikasi compile**

Run: `cargo check -p api`
Expected: LULUS.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room runbook handlers"
```

---

### Task 9: Events list + route registration

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`

- [ ] **Step 1: Tambahkan handler `events_list`**

```rust
#[derive(Debug, Deserialize)]
pub struct EventsParams {
    pub before_id: Option<Uuid>,
    pub limit: Option<i64>,
}

pub async fn events_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Query(params): Query<EventsParams>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(_room) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "War room not found"}))));
    };
    let cursor: Option<(chrono::DateTime<chrono::Utc>, Uuid)> = match params.before_id {
        Some(before_id) => {
            sqlx::query_as("SELECT created_at, id FROM war_room_events WHERE id = $1 AND war_room_id = $2")
                .bind(before_id)
                .bind(pk)
                .fetch_optional(&st.pool)
                .await?
        }
        None => None,
    };
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let rows: Vec<WarRoomEventRow> = sqlx::query_as(
        "SELECT e.id, e.actor_id, e.event_type, e.payload, e.created_at FROM war_room_events e \
         WHERE e.war_room_id = $1 \
         AND ($2::timestamptz IS NULL OR (e.created_at, e.id) < ($2::timestamptz, $3::uuid)) \
         ORDER BY e.created_at DESC, e.id DESC LIMIT $4",
    )
    .bind(pk)
    .bind(cursor.as_ref().map(|c| c.0))
    .bind(cursor.as_ref().map(|c| c.1))
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(event_json).collect())),
    ))
}
```

- [ ] **Step 2: Daftarkan route di `main.rs`**

Di `apps/api-rs/crates/api/src/main.rs`, setelah blok route `service-issues/:pk/` (sekitar baris 740), tambahkan:

```rust
        // War rooms (ITSM incident command rooms). Session auth;
        // project-scoped. Reads = any active member (incl. guest), writes =
        // project ADMIN/MEMBER. Real-time relay lives in apps/live (phase 2).
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/",
            get(routes::war_room::list).post(routes::war_room::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/summary/",
            get(routes::war_room::summary),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/",
            get(routes::war_room::detail)
                .patch(routes::war_room::patch)
                .delete(routes::war_room::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/services/",
            post(routes::war_room::services_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/services/:service_id/",
            delete(routes::war_room::services_destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/issues/",
            post(routes::war_room::issues_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/issues/:issue_id/",
            delete(routes::war_room::issues_destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/participants/",
            post(routes::war_room::participants_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/participants/:participant_id/",
            patch(routes::war_room::participants_patch)
                .delete(routes::war_room::participants_destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/runbook-items/",
            post(routes::war_room::runbook_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/runbook-items/:item_id/",
            patch(routes::war_room::runbook_patch).delete(routes::war_room::runbook_destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/events/",
            get(routes::war_room::events_list),
        )
```

- [ ] **Step 3: Verifikasi route gate + shape**

Run:

```bash
cargo test -p api --test route_inventory_test
```

Expected: PASS — tidak ada path baru yang bentrok (`route_shapes_build_without_conflict`) dan inventory existing tetap terdaftar. Jika `war-rooms/summary` vs `war-rooms/:pk` bentrok (panic matchit), pindahkan summary ke `/api/workspaces/:slug/projects/:project_id/war-room-summary/`.

- [ ] **Step 4: Build full**

Run: `cargo build -p api`
Expected: LULUS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs apps/api-rs/crates/api/src/main.rs
git commit -m "feat(api-rs): register war room routes and events endpoint"
```

---

### Task 10: Test scaffolding + create/sequence/duplicate tests

**Files:**

- Create: `apps/api-rs/crates/api/tests/war_room_test.rs`

- [ ] **Step 1: Tulis scaffolding + test create**

Create `apps/api-rs/crates/api/tests/war_room_test.rs`:

```rust
//! War room backend integration tests (Phase 1): create defaults/sequence,
//! duplicate guard, transitions, links, participants, runbook, events,
//! access gates, list/summary. DB-backed; run serially:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test war_room_test -- --test-threads=1`

use api::middleware::auth::AuthUser;
use api::routes::war_room::{
    create, detail, destroy, events_list, issues_create, issues_destroy, list,
    participants_create, participants_destroy, participants_patch, patch, runbook_create,
    runbook_patch, services_create, services_destroy, summary, CreateWarRoom, EventsParams,
    LinkIssues, LinkServices, ListParams, ParticipantCreate, ParticipantPatch, PatchWarRoom,
    RunbookCreate, RunbookPatch,
};
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

async fn insert_project_member(pool: &PgPool, user_id: Uuid, project_id: Uuid, workspace_id: Uuid, role: i16) {
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
        let slug = format!("wr-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let state_id = Uuid::new_v4();
        let type_id = Uuid::new_v4();
        let identifier = format!("WR{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();

        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'War Room Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
             VALUES ($1, now(), now(), 'War Room Scratch', '', 2, $2, $3, false, false, false, \
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
             VALUES ($1, 'Investigating', '', '#F59E0B', 'investigating', $2, $3, 65535, \
             'started', true, false, now(), now())",
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
             VALUES ($1, 'Incident', '', '{}'::jsonb, $2, true, true, 0, false, now(), now())",
        )
        .bind(type_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch issue type");

        Self { slug, workspace_id, user_id, project_id, state_id, type_id, extra_users: Vec::new() }
    }

    async fn add_actor(&mut self, pool: &PgPool, ws_role: Option<i16>, project_role: Option<i16>) -> Uuid {
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

    async fn insert_issue(&self, pool: &PgPool, assignee: Option<Uuid>) -> Uuid {
        let issue_id: Uuid = sqlx::query_scalar(
            "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, \
             sort_order, sequence_id, state_id, type_id, project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'QRIS timeout massal', '<p></p>', '{}', 'urgent', false, \
             65535, (SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM issues WHERE project_id = $3), \
             $1, $2, $3, $4, now(), now()) RETURNING id",
        )
        .bind(self.state_id)
        .bind(self.type_id)
        .bind(self.project_id)
        .bind(self.workspace_id)
        .fetch_one(pool)
        .await
        .expect("scratch issue");
        if let Some(user_id) = assignee {
            sqlx::query(
                "INSERT INTO issue_assignees (id, assignee_id, issue_id, project_id, workspace_id, \
                 created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
            )
            .bind(user_id)
            .bind(issue_id)
            .bind(self.project_id)
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .expect("scratch issue assignee");
        }
        issue_id
    }

    async fn insert_service(&self, pool: &PgPool, name: &str) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO services (id, workspace_id, project_id, name, description, \
             description_html, status, criticality, \"type\", sort_order, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, '', '', 'active', 'high', 'internal', 65535, \
             now(), now()) RETURNING id",
        )
        .bind(self.workspace_id)
        .bind(self.project_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("scratch service")
    }

    async fn cleanup(&self, pool: &PgPool) {
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

#[tokio::test]
async fn create_seeds_defaults_runbook_and_sequence() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let assignee = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let issue_id = scratch.insert_issue(&st.pool, Some(assignee)).await;
    let service_id = scratch.insert_service(&st.pool, "Payment Gateway").await;

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateWarRoom {
            name: None,
            primary_issue_id: issue_id,
            severity: None,
            description_html: None,
            service_ids: Some(vec![service_id]),
        }),
    )
    .await
    .expect("create room");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["sequence_id"], 1);
    assert_eq!(body["name"], "QRIS timeout massal");
    assert_eq!(body["severity"], "sev1");
    assert_eq!(body["status"], "active");
    assert!(body["primary_issue"]["identifier"].as_str().unwrap().ends_with("-1"));
    assert_eq!(body["services"].as_array().unwrap().len(), 1);
    assert_eq!(body["participants"].as_array().unwrap().len(), 2);
    assert_eq!(body["runbook_items"].as_array().unwrap().len(), 5);
    assert_eq!(body["counts"]["messages"], 0);

    let other_issue = scratch.insert_issue(&st.pool, None).await;
    let (status, Json(second)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateWarRoom {
            name: Some("Second room".into()),
            primary_issue_id: other_issue,
            severity: Some("sev3".into()),
            description_html: None,
            service_ids: None,
        }),
    )
    .await
    .expect("create second room");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["sequence_id"], 2);
    assert_eq!(second["runbook_items"].as_array().unwrap().len(), 5);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_conflicts_when_active_room_exists() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let payload = || CreateWarRoom {
        name: None,
        primary_issue_id: issue_id,
        severity: None,
        description_html: None,
        service_ids: None,
    };
    let (status, _) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(payload()),
    )
    .await
    .expect("first create");
    assert_eq!(status, StatusCode::CREATED);

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(payload()),
    )
    .await
    .expect("duplicate create");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "active_war_room_exists");
    assert!(body["war_room_id"].is_string());

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Jalankan test baru**

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test war_room_test -- --test-threads=1
```

Expected: PASS (2 tests). Jika gagal karena kolom NOT NULL kurang pada scratch (`issue_types`, `services`, `issues`), tambahkan nilai default yang diminta error Postgres — jangan ubah handler.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/war_room_test.rs
git commit -m "test(api-rs): war room create and duplicate guard integration tests"
```

---

### Task 11: Test integrasi transisi, links, participants, runbook, events, akses, list/summary

**Files:**

- Modify: `apps/api-rs/crates/api/tests/war_room_test.rs`

- [ ] **Step 1: Tambahkan test transisi + links**

Tambahkan di akhir file:

```rust
async fn create_room(st: &AppState, scratch: &Scratch, issue_id: Uuid) -> Value {
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateWarRoom {
            name: None,
            primary_issue_id: issue_id,
            severity: None,
            description_html: None,
            service_ids: None,
        }),
    )
    .await
    .expect("create room");
    assert_eq!(status, StatusCode::CREATED);
    body
}

fn room_id(body: &Value) -> Uuid {
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn status_transitions_freeze_and_reopen() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: Some("sev2".into()),
            status: Some("resolved".into()),
            description_html: None,
            notes_html: Some("<p>Rollback done</p>".into()),
        }),
    )
    .await
    .expect("resolve");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["severity"], "sev2");
    assert!(body["resolved_at"].is_string());

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("monitoring".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("invalid transition");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_status_transition");

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("active".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("reopen");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "active");
    assert!(body["resolved_at"].is_null());

    let (status, _) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("archived".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("archive");
    assert_eq!(status, StatusCode::OK);

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("active".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("archived terminal");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "room_archived");

    let (status, Json(body)) = runbook_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(RunbookCreate { title: "No write when archived".into() }),
    )
    .await
    .expect("archived write");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "room_archived");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn links_are_idempotent_and_primary_is_rejected() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let other_issue = scratch.insert_issue(&st.pool, None).await;
    let service_id = scratch.insert_service(&st.pool, "QRIS Processor").await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, Json(body)) = services_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(LinkServices { service_ids: vec![service_id] }),
    )
    .await
    .expect("link service");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["linked"], 1);

    let (_, Json(body)) = services_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(LinkServices { service_ids: vec![service_id] }),
    )
    .await
    .expect("relink service");
    assert_eq!(body["linked"], 0);

    let (status, Json(body)) = issues_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(LinkIssues { issue_ids: vec![issue_id] }),
    )
    .await
    .expect("link primary");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "primary_issue_not_linkable");

    let (status, Json(body)) = issues_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(LinkIssues { issue_ids: vec![other_issue] }),
    )
    .await
    .expect("link other");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["linked"], 1);

    let (status, Json(body)) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
    )
    .await
    .expect("detail");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["issues"].as_array().unwrap().len(), 1);

    let (status, _) = issues_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, other_issue)),
    )
    .await
    .expect("unlink");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = services_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, service_id)),
    )
    .await
    .expect("unlink service");
    assert_eq!(status, StatusCode::NO_CONTENT);

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Tambahkan test participants + runbook + events + akses + list/summary**

```rust
#[tokio::test]
async fn participants_keep_single_commander() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let teammate = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, Json(body)) = participants_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(ParticipantCreate { member_id: teammate, role: None }),
    )
    .await
    .expect("add participant");
    assert_eq!(status, StatusCode::CREATED);
    let participant_id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    assert_eq!(body["role"], "responder");

    let (status, Json(body)) = participants_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, participant_id)),
        Json(ParticipantPatch { role: "commander".into() }),
    )
    .await
    .expect("promote commander");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "commander");

    let (_, Json(body)) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
    )
    .await
    .expect("detail");
    let roles: Vec<&str> = body["participants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles.iter().filter(|r| **r == "commander").count(), 1);

    let (status, _) = participants_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, participant_id)),
    )
    .await
    .expect("leave");
    assert_eq!(status, StatusCode::NO_CONTENT);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn runbook_toggle_records_events() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);
    let item_id = Uuid::parse_str(room["runbook_items"][0]["id"].as_str().unwrap()).unwrap();

    let (status, Json(body)) = runbook_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, item_id)),
        Json(RunbookPatch { title: None, is_done: Some(true) }),
    )
    .await
    .expect("mark done");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["is_done"], true);
    assert!(body["done_by_id"].is_string());

    let (status, Json(body)) = runbook_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, item_id)),
        Json(RunbookPatch { title: None, is_done: Some(false) }),
    )
    .await
    .expect("reopen item");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["is_done"], false);
    assert!(body["done_by_id"].is_null());

    let (status, Json(events)) = events_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Query(EventsParams { before_id: None, limit: None }),
    )
    .await
    .expect("events");
    assert_eq!(status, StatusCode::OK);
    let types: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"runbook.item_done"));
    assert!(types.contains(&"runbook.item_reopened"));
    assert!(types.contains(&"room.created"));

    let (status, Json(body)) = runbook_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(RunbookCreate { title: "Custom check".into() }),
    )
    .await
    .expect("add item");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["title"], "Custom check");
    assert!(body["template_key"].is_null());

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn access_gates_deny_guest_writes_and_non_members() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let guest = scratch.add_actor(&st.pool, Some(5), Some(5)).await;
    let outsider = scratch.add_actor(&st.pool, None, None).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, _) = detail(
        State(st.clone()),
        AuthUser(guest),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
    )
    .await
    .expect("guest read");
    assert_eq!(status, StatusCode::OK);

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(guest),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("monitoring".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("guest write");
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["error"].is_string());

    let (status, _) = detail(
        State(st.clone()),
        AuthUser(outsider),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
    )
    .await
    .expect("outsider read");
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn list_filters_and_summary_counts() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_a = scratch.insert_issue(&st.pool, None).await;
    let issue_b = scratch.insert_issue(&st.pool, None).await;
    let room_a = create_room(&st, &scratch, issue_a).await;
    let pk_a = room_id(&room_a);
    let _room_b = create_room(&st, &scratch, issue_b).await;

    let (status, _) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk_a)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("resolved".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("resolve room a");
    assert_eq!(status, StatusCode::OK);

    let (status, Json(body)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(ListParams {
            status: Some("active,monitoring".into()),
            severity: None,
            q: None,
        }),
    )
    .await
    .expect("list active");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["sequence_id"], 2);

    let (status, Json(body)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(ListParams { status: None, severity: None, q: Some("WR".into()) }),
    )
    .await
    .expect("list by incident identifier");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 2);

    let (status, Json(body)) = summary(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
    )
    .await
    .expect("summary");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], 1);
    assert_eq!(body["sev1_2"], 1);
    assert_eq!(body["resolved_7d"], 1);

    let (status, Json(body)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(ListParams { status: Some("bogus".into()), severity: None, q: None }),
    )
    .await
    .expect("invalid status filter");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk_a)),
    )
    .await
    .expect("destroy");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk_a)),
    )
    .await
    .expect("detail after destroy");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 3: Jalankan seluruh suite war room**

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test war_room_test -- --test-threads=1
```

Expected: PASS (8 tests). Perbaiki hanya scratch/assertion bila ada kolom NOT NULL kurang; jangan longgarkan assertion handler.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/tests/war_room_test.rs
git commit -m "test(api-rs): war room transitions, links, participants, runbook, access tests"
```

---

### Task 12: Verifikasi penuh + format

**Files:**

- Tidak ada file baru.

- [ ] **Step 1: Format**

Run: `cargo fmt --all` (dari `apps/api-rs`), lalu `git diff --stat`
Expected: perubahan format hanya di file phase 1; commit bila ada.

- [ ] **Step 2: Unit + integrasi war room**

Run:

```bash
cargo test -p api --lib war_room::tests
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test war_room_test -- --test-threads=1
cargo test -p api --test route_inventory_test
```

Expected: semua PASS.

- [ ] **Step 3: Suite penuh api-rs**

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api
```

Expected: tidak ada regresi. (Suite scratch lain berjalan sesuai konvensi masing-masing.)

- [ ] **Step 4: Commit sisa format bila ada**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "style(api-rs): format war room module"
```

---

## Catatan untuk fase berikutnya (bukan bagian plan ini)

- **Fase 2 (chat + realtime):** handler messages (list cursor + create + edit/delete) + parse mention `@{user_id}` + row `notifications` + publish Redis `war-room:events`; controller WS di `apps/live`.
- **Fase 3 (web list + create):** types/constants/store/service, halaman list, modal create, single-select picker, sidebar, entry point work item.
- **Fase 4 (room page):** header/lifecycle, graph refactor + peta blast radius, chat panel + socket hook, tab Notes/Work items/Runbook/Activity/People.
- **Fase 5 (notifikasi + docs):** cabang notification card, `docs/features/war-rooms.md`, update backlog.
- Inventory parity (`parity-inventory.json`) ditambahkan saat fase 3 (butuh `fe_evidence` file FE yang sudah ada).
