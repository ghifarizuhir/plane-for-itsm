# Intake Webhook Source (Alertmanager → Incident) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Alert Prometheus Alertmanager masuk otomatis sebagai item intake Incident lewat token per source, dengan mapping label → service/priority, dedup fingerprint, auto-resolve/refire, toggle auto-accept, dan UI pengelolaan source.

**Architecture:** Satu endpoint publik `POST /api/inbound/alertmanager/:token/` di api-rs (token lookup → tabel baru `intake_sources`) membuat/meng-update issue intake existing. Semua keputusan ada di `docs/superpowers/specs/2026-10-04-intake-webhook-source-design.md`.

**Tech Stack:** Rust (axum, sqlx, sha2), Postgres, React + MobX + TypeScript, Vitest, i18n JSON.

---

## Prasyarat test DB (berlaku untuk semua task backend)

Integration test DB memakai Postgres dev di `localhost:5432`. Sweep beat bisa menabrak data scratch — **stop worker & beat-worker dulu**, nyalakan lagi setelah selesai:

```bash
docker stop plane-for-itsm-worker-1 plane-for-itsm-beat-worker-1
```

Setelah semua test backend selesai:

```bash
docker start plane-for-itsm-worker-1 plane-for-itsm-beat-worker-1
```

Jalankan test api-rs dari `apps/api-rs` dengan `DATABASE_URL` eksplisit. Scratch-workspace suite harus `--test-threads=1` (purge by slug prefix saling menabrak bila paralel).

## Struktur file

**Backend (api-rs):**

- `apps/api-rs/migrations/0015_intake_sources.sql` — tabel `intake_sources` + kolom seri alert di `issues`.
- `apps/api-rs/crates/api/src/routes/inbound.rs` — endpoint publik ingest + logika klasifikasi/dedup/resolve.
- `apps/api-rs/crates/api/src/routes/intake_source.rs` — CRUD source (auth).
- `apps/api-rs/crates/api/src/routes/intake.rs` — `accept_intake_issue` (helper bersama), `PRIORITIES` jadi `pub(crate)`, detail memuat source.
- `apps/api-rs/crates/api/src/routes/mod.rs` — `pub mod inbound; pub mod intake_source;`
- `apps/api-rs/crates/api/src/main.rs` — mount route publik + CRUD.
- `apps/api-rs/crates/api/src/middleware/origin.rs` — pengecualian `/api/inbound/`.
- `apps/api-rs/crates/api/tests/inbound_intake_test.rs` — test ingest (DB-backed).
- `apps/api-rs/crates/api/tests/intake_source_routes_test.rs` — test CRUD.

**Frontend:**

- `packages/types/src/intake-source.ts` — tipe source + config.
- `packages/types/src/inbox.ts` — `EInboxIssueSource.WEBHOOK` + field source di detail.
- `packages/constants/src/settings/project.ts` — tab settings baru.
- `packages/types/src/settings.ts` — key tab baru.
- `apps/web/core/components/settings/project/sidebar/item-icon.tsx` — ikon tab.
- `apps/web/core/services/intake-source.service.ts` — service API.
- `apps/web/core/store/intake-source.store.ts` — MobX store.
- `apps/web/core/store/intake-source.helpers.ts` + `.test.ts` — validasi config & builder URL (pure, diuji).
- `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/intake-sources/{page,header}.tsx` — halaman.
- `apps/web/core/components/intake-sources/` — root list + modal form + mapping editor.
- `apps/web/core/components/inbox/content/issue-properties.tsx` — baris Source.
- `packages/i18n/src/locales/en/workspace-settings.json` + `inbox.json` — label.

**Docs:**

- `docs/features/intake.md` — bagian channel webhook + changelog.

---

### Task 1: Migrasi `intake_sources` + kolom seri `issues`

**Files:**

- Create: `apps/api-rs/migrations/0015_intake_sources.sql`

- [ ] **Step 1: Tulis migrasi**

```sql
-- Intake webhook source: sumber inbound (Alertmanager) + identitas seri alert.
-- Diterapkan manual ke dev DB via psql (pola 0014); IF NOT EXISTS agar aman
-- bila sqlx migrate menyusul.

CREATE TABLE IF NOT EXISTS public.intake_sources (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id uuid NOT NULL,
    name character varying(255) NOT NULL,
    token character varying(64) NOT NULL,
    is_active boolean NOT NULL DEFAULT true,
    auto_accept boolean NOT NULL DEFAULT false,
    type_id uuid,
    config jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS intake_sources_token_uniq
    ON public.intake_sources (token) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS intake_sources_project_idx
    ON public.intake_sources (project_id) WHERE deleted_at IS NULL;

ALTER TABLE public.issues
    ADD COLUMN IF NOT EXISTS intake_source_id uuid,
    ADD COLUMN IF NOT EXISTS intake_fingerprint text,
    ADD COLUMN IF NOT EXISTS intake_occurrence_count integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS intake_last_seen_at timestamptz;

CREATE UNIQUE INDEX IF NOT EXISTS issues_intake_series_uniq
    ON public.issues (intake_source_id, intake_fingerprint)
    WHERE intake_source_id IS NOT NULL;
```

- [ ] **Step 2: Terapkan ke dev DB**

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane < apps/api-rs/migrations/0015_intake_sources.sql
```

Expected: `CREATE TABLE`, `CREATE INDEX` (x3), `ALTER TABLE`.

- [ ] **Step 3: Verifikasi kolom**

```bash
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "\d intake_sources" -c "\d issues" | grep -E "intake_source|intake_fingerprint|intake_occurrence|intake_last_seen|token|config"
```

Expected: kolom `intake_sources.token`, `config`, dan 4 kolom `intake_*` di `issues` tampil.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/0015_intake_sources.sql
git commit -m "feat(api-rs): add intake sources and alert series migration"
```

---

### Task 2: Origin middleware exemption untuk `/api/inbound/`

**Files:**

- Modify: `apps/api-rs/crates/api/src/middleware/origin.rs`

- [ ] **Step 1: Tulis failing test**

Tambahkan di akhir `origin.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbound_paths_are_exempt_from_origin_check() {
        assert!(inbound_path_exempt("/api/inbound/alertmanager/abc/"));
        assert!(!inbound_path_exempt("/api/workspaces/acme/projects/1/intake-sources/"));
    }
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

```bash
cd apps/api-rs && cargo test -p api middleware::origin::tests::inbound_paths_are_exempt_from_origin_check
```

Expected: FAIL — `cannot find function inbound_path_exempt`.

- [ ] **Step 3: Implementasi**

Di `origin.rs`, tambah fungsi setelah `origin_allowed`:

```rust
/// Endpoint inbound server-to-server (Alertmanager) tidak mengirim
/// Origin/Referer. Path di bawah prefix ini dikecualikan dari CSRF check;
/// autentikasi tetap lewat token di URL.
pub fn inbound_path_exempt(path: &str) -> bool {
    path.starts_with("/api/inbound/")
}
```

Lalu di `origin_middleware` (baris ~90), ganti kondisi `if !origin_allowed_many(...)` menjadi:

```rust
if !origin_allowed_many(req.method(), req.headers(), &frontends) && !inbound_path_exempt(req.uri().path()) {
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

```bash
cd apps/api-rs && cargo test -p api middleware::origin::tests
```

Expected: PASS (2 test).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/middleware/origin.rs
git commit -m "feat(api-rs): exempt inbound webhook paths from origin check"
```

---

### Task 3: CRUD `intake-sources` (auth)

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/intake_source.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs` (mount setelah route `webhook-logs` ~baris 1334)
- Test: `apps/api-rs/crates/api/tests/intake_source_routes_test.rs`

- [ ] **Step 1: Tulis failing test**

`intake_source_routes_test.rs` — salin harness `Scratch` dari `intake_triage_test.rs` (fungsi `database_url`, `pool`, `state`, `insert_user`, struct `Scratch` + `new/add_intake/add_type/add_default_state`), lalu tambah method dan test:

```rust
async fn add_source(&self, pool: &PgPool) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let token = format!("plane_is_{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO intake_sources (id, project_id, name, token, is_active, auto_accept, config, created_by_id, created_at, updated_at) \
         VALUES ($1, $2, 'Prometheus Prod', $3, true, false, '{}'::jsonb, $4, now(), now())",
    )
    .bind(id)
    .bind(self.project_id)
    .bind(&token)
    .bind(self.user_id)
    .execute(pool)
    .await
    .expect("scratch source");
    (id, token)
}

#[tokio::test]
async fn create_validates_config_and_returns_token() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment").await;
    let type_id = scratch.add_type(&pool, "Incident").await;
    let st = state(&pool).await;

    let (status, Json(created)) = api::routes::intake_source::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(api::routes::intake_source::CreateIntakeSource {
            name: "Prometheus Prod".into(),
            type_id: Some(type_id),
            auto_accept: Some(false),
            config: Some(serde_json::json!({
                "service_label_key": "service",
                "service_map": { "payment-api": service_id },
                "severity_label_key": "severity",
                "severity_map": { "critical": "urgent" },
                "default_priority": "none"
            })),
        }),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::CREATED);
    let token = created["token"].as_str().unwrap();
    assert!(token.starts_with("plane_is_"));
    assert_eq!(created["auto_accept"], false);

    // service asing ditolak
    let (bad, Json(_)) = api::routes::intake_source::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(api::routes::intake_source::CreateIntakeSource {
            name: "Bad".into(),
            type_id: Some(type_id),
            auto_accept: None,
            config: Some(serde_json::json!({
                "service_map": { "x": Uuid::new_v4() }
            })),
        }),
    )
    .await
    .unwrap();
    assert_eq!(bad, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rotate_invalidates_old_token() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let (source_id, old_token) = scratch.add_source(&pool).await;
    let st = state(&pool).await;

    let (status, Json(rotated)) = api::routes::intake_source::rotate(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, source_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    let new_token = rotated["token"].as_str().unwrap();
    assert_ne!(new_token, old_token);

    let (old_alive,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM intake_sources WHERE token = $1 AND deleted_at IS NULL)",
    )
    .bind(&old_token)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!old_alive);
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_source_routes_test -- --test-threads=1
```

Expected: FAIL — module `intake_source` belum ada.

- [ ] **Step 3: Implementasi `intake_source.rs`**

```rust
//! CRUD `intake_sources` — sumber webhook inbound per project.
//! Endpoint ingest publiknya ada di `routes/inbound.rs`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::{
        issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows},
        project::{deny, missing},
    },
    state::AppState,
};

const PRIORITIES: [&str; 5] = ["low", "medium", "high", "urgent", "none"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IntakeSourceRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub token: String,
    pub is_active: bool,
    pub auto_accept: bool,
    pub type_id: Option<Uuid>,
    pub config: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
}

const SOURCE_COLS: &str = "s.id, s.project_id, s.name, s.token, s.is_active, s.auto_accept, \
    s.type_id, s.config, s.created_at, s.updated_at, s.created_by_id";

fn source_json(row: &IntakeSourceRow) -> Value {
    json!({
        "id": row.id,
        "project_id": row.project_id,
        "name": row.name,
        "token": row.token,
        "is_active": row.is_active,
        "auto_accept": row.auto_accept,
        "type_id": row.type_id,
        "config": row.config,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": row.created_by_id,
    })
}

#[derive(Debug, Deserialize)]
pub struct CreateIntakeSource {
    pub name: String,
    #[serde(default)]
    pub type_id: Option<Uuid>,
    #[serde(default)]
    pub auto_accept: Option<bool>,
    #[serde(default)]
    pub config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct PatchIntakeSource {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub type_id: Option<Uuid>,
    #[serde(default)]
    pub auto_accept: Option<bool>,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub config: Option<Value>,
}

/// Validasi config mapping. `project_id` dipakai untuk memastikan service
/// milik project yang sama. Return `Err(pesan)` untuk 400.
pub async fn validate_config(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    config: &Value,
) -> Result<(), String> {
    let ok_key = |v: Option<&Value>| {
        v.and_then(Value::as_str)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(true)
    };
    if !ok_key(config.get("service_label_key")) || !ok_key(config.get("severity_label_key")) {
        return Err("Label key must not be empty".into());
    }
    let mut service_ids: Vec<Uuid> = Vec::new();
    if let Some(map) = config.get("service_map") {
        let Some(map) = map.as_object() else {
            return Err("service_map must be an object".into());
        };
        for (key, value) in map {
            if key.trim().is_empty() {
                return Err("service_map label value must not be empty".into());
            }
            let Some(id) = value.as_str().and_then(|s| Uuid::parse_str(s).ok()) else {
                return Err("service_map values must be service ids".into());
            };
            service_ids.push(id);
        }
    }
    if let Some(id) = config.get("fallback_service_id").and_then(Value::as_str) {
        if let Ok(id) = Uuid::parse_str(id) {
            service_ids.push(id);
        } else {
            return Err("Invalid fallback_service_id".into());
        }
    }
    for service_id in service_ids {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM services WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
        )
        .bind(service_id)
        .bind(project_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
        if !exists {
            return Err("Service does not belong to this project".into());
        }
    }
    for (field, value) in [
        (
            "severity_map",
            config.get("severity_map").and_then(Value::as_object),
        ),
        (
            "default_priority",
            config
                .get("default_priority")
                .map(|v| {
                    v.as_str()
                        .map(|s| (String::new(), Value::String(s.to_string())))
                        .into_iter()
                        .collect()
                })
                .as_ref(),
        ),
    ] {
        let Some(map) = value else { continue };
        if field == "severity_map" {
            for (_, priority) in map {
                let Some(p) = priority.as_str() else {
                    return Err("severity_map values must be priorities".into());
                };
                if !PRIORITIES.contains(&p) {
                    return Err("Invalid priority".into());
                }
            }
        }
    }
    if let Some(p) = config.get("default_priority").and_then(Value::as_str) {
        if !PRIORITIES.contains(&p) {
            return Err("Invalid priority".into());
        }
    }
    Ok(())
}

async fn writer_gate(pool: &sqlx::PgPool, user: Uuid, slug: &str, project_id: Uuid) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(matches!(role, Some(20) | Some(15)), role.is_some(), ws_admin))
}

async fn member_gate(pool: &sqlx::PgPool, user: Uuid, slug: &str, project_id: Uuid) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        matches!(role, Some(20) | Some(15) | Some(5)),
        role.is_some(),
        ws_admin,
    ))
}

/// Type harus live, non-epic, dan ter-link ke project.
async fn valid_type(pool: &sqlx::PgPool, project_id: Uuid, type_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issue_types t \
         JOIN project_issue_types pit ON pit.issue_type_id = t.id AND pit.deleted_at IS NULL \
         WHERE t.id = $1 AND pit.project_id = $2 AND t.deleted_at IS NULL AND t.is_epic = false)",
    )
    .bind(type_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !member_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<IntakeSourceRow> = sqlx::query_as(&format!(
        "SELECT {SOURCE_COLS} FROM intake_sources s \
         WHERE s.project_id = $1 AND s.deleted_at IS NULL ORDER BY s.created_at ASC"
    ))
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(Value::Array(rows.iter().map(source_json).collect()))))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateIntakeSource>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !writer_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Name is required"}))));
    }
    let Some(type_id) = body.type_id else {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Select a work item type"}))));
    };
    if !valid_type(&st.pool, project_id, type_id).await? {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid work item type"}))));
    }
    let config = body.config.unwrap_or_else(|| json!({}));
    if let Err(message) = validate_config(&st.pool, project_id, &config).await {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO intake_sources (id, project_id, name, token, is_active, auto_accept, type_id, \
         config, created_by_id, updated_by_id, created_at, updated_at) \
         VALUES ($1, $2, $3, 'plane_is_' || replace(gen_random_uuid()::text, '-', ''), true, $4, $5, $6, $7, $7, now(), now())",
    )
    .bind(id)
    .bind(project_id)
    .bind(&name)
    .bind(body.auto_accept.unwrap_or(false))
    .bind(type_id)
    .bind(&config)
    .bind(auth.0)
    .execute(&st.pool)
    .await?;
    let row: IntakeSourceRow =
        sqlx::query_as(&format!("SELECT {SOURCE_COLS} FROM intake_sources s WHERE s.id = $1"))
            .bind(id)
            .fetch_one(&st.pool)
            .await?;
    Ok((StatusCode::CREATED, Json(source_json(&row))))
}

async fn fetch_source(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    pk: Uuid,
) -> Result<Option<IntakeSourceRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {SOURCE_COLS} FROM intake_sources s \
         WHERE s.id = $1 AND s.project_id = $2 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !member_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    match fetch_source(&st.pool, project_id, pk).await? {
        Some(row) => Ok((StatusCode::OK, Json(source_json(&row)))),
        None => Ok(missing()),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchIntakeSource>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !writer_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(current) = fetch_source(&st.pool, project_id, pk).await? else {
        return Ok(missing());
    };
    let name = body.name.as_ref().map(|n| n.trim().to_string());
    if matches!(&name, Some(n) if n.is_empty()) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Name is required"}))));
    }
    if let Some(type_id) = body.type_id {
        if !valid_type(&st.pool, project_id, type_id).await? {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid work item type"}))));
        }
    }
    if let Some(config) = &body.config {
        if let Err(message) = validate_config(&st.pool, project_id, config).await {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    }
    let config = body.config.unwrap_or(current.config);
    sqlx::query(
        "UPDATE intake_sources SET name = COALESCE($1, name), type_id = COALESCE($2, type_id), \
         auto_accept = COALESCE($3, auto_accept), is_active = COALESCE($4, is_active), \
         config = $5, updated_by_id = $6, updated_at = now() WHERE id = $7 AND deleted_at IS NULL",
    )
    .bind(name)
    .bind(body.type_id)
    .bind(body.auto_accept)
    .bind(body.is_active)
    .bind(&config)
    .bind(auth.0)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    let row = fetch_source(&st.pool, project_id, pk).await?.unwrap();
    Ok((StatusCode::OK, Json(source_json(&row))))
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !writer_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let result = sqlx::query(
        "UPDATE intake_sources SET deleted_at = now(), updated_by_id = $1, updated_at = now() \
         WHERE id = $2 AND project_id = $3 AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::OK, Json(json!({"ok": true}))))
}

pub async fn rotate(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !writer_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let token: Option<String> = sqlx::query_scalar(
        "UPDATE intake_sources SET token = 'plane_is_' || replace(gen_random_uuid()::text, '-', ''), \
         updated_by_id = $1, updated_at = now() \
         WHERE id = $2 AND project_id = $3 AND deleted_at IS NULL RETURNING token",
    )
    .bind(auth.0)
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match token {
        Some(token) => Ok((StatusCode::OK, Json(json!({"id": pk, "token": token})))),
        None => Ok(missing()),
    }
}
```

Catatan: blok loop validasi `severity_map` di atas sengaja eksplisit agar mudah dibaca; rapikan bila perlu, yang penting perilakunya sama (nilai harus salah satu dari `PRIORITIES`).

- [ ] **Step 4: Daftarkan modul + mount route**

`routes/mod.rs`: tambah `pub mod inbound;` dan `pub mod intake_source;` (urut alfabetis).

`main.rs`, setelah blok route `webhook-logs` (~baris 1334):

```rust
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-sources/",
            get(routes::intake_source::list).post(routes::intake_source::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-sources/:pk/",
            get(routes::intake_source::detail)
                .patch(routes::intake_source::patch)
                .delete(routes::intake_source::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-sources/:pk/rotate/",
            post(routes::intake_source::rotate),
        )
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_source_routes_test -- --test-threads=1
```

Expected: PASS (2 test).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/intake_source.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/intake_source_routes_test.rs
git commit -m "feat(api-rs): add intake source CRUD routes"
```

---

### Task 4: Ingest — auth token, parse, klasifikasi, create

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/inbound.rs`
- Test: `apps/api-rs/crates/api/tests/inbound_intake_test.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs` (mount, dekat route health/awal router)
- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` (`PRIORITIES` → `pub(crate)`)

- [ ] **Step 1: Tulis failing test**

`inbound_intake_test.rs` — salin harness `Scratch` dari `intake_triage_test.rs` (tanpa `mod support`), tambah method:

```rust
async fn add_source_with_config(
    &self,
    pool: &PgPool,
    type_id: Uuid,
    config: serde_json::Value,
) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let token = format!("plane_is_{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO intake_sources (id, project_id, name, token, is_active, auto_accept, type_id, \
         config, created_by_id, created_at, updated_at) \
         VALUES ($1, $2, 'Prometheus Prod', $3, true, false, $4, $5, $6, now(), now())",
    )
    .bind(id)
    .bind(self.project_id)
    .bind(&token)
    .bind(type_id)
    .bind(&config)
    .bind(self.user_id)
    .execute(pool)
    .await
    .expect("scratch source");
    (id, token)
}

fn alert_payload(fingerprint: &str, status: &str, severity: &str) -> serde_json::Value {
    serde_json::json!({
        "version": "4",
        "status": status,
        "commonLabels": { "severity": severity },
        "alerts": [{
            "status": status,
            "labels": {
                "alertname": "HighErrorRate",
                "service": "payment-api",
                "severity": severity
            },
            "annotations": {
                "summary": "Error rate > 5%",
                "description": "5xx spike on payment-api"
            },
            "startsAt": "2026-10-04T00:00:00Z",
            "endsAt": "0001-01-01T00:00:00Z",
            "generatorURL": "http://prometheus.local/graph",
            "fingerprint": fingerprint
        }]
    })
}

#[tokio::test]
async fn creates_pending_item_with_mapped_type_service_priority() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    let (source_id, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({
                "service_label_key": "service",
                "service_map": { "payment-api": service_id },
                "severity_label_key": "severity",
                "severity_map": { "critical": "urgent" },
                "default_priority": "none"
            }),
        )
        .await;
    let st = state(&pool).await;

    let (status, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token),
        Json(alert_payload("fp-1", "firing", "critical")),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["created"], 1);

    let row: (String, String, Uuid, i32, i32, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT i.name, i.priority, i.type_id, ii.status, i.intake_occurrence_count, i.intake_last_seen_at \
         FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = 'fp-1'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "Error rate > 5%");
    assert_eq!(row.1, "urgent");
    assert_eq!(row.2, type_id);
    assert_eq!(row.3, -2);
    assert_eq!(row.4, 1);
    assert!(row.5.is_some());

    let (linked,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM service_issues WHERE issue_id = (SELECT issue_id FROM intake_issues WHERE id = (SELECT ii.id FROM intake_issues ii JOIN issues i ON i.id = ii.issue_id WHERE i.intake_fingerprint = 'fp-1')) AND service_id = $1 AND deleted_at IS NULL)",
    )
    .bind(service_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(linked);

    // token salah → 404
    let (not_found, _) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path("plane_is_nope".to_string()),
        Json(alert_payload("fp-2", "firing", "critical")),
    )
    .await
    .unwrap();
    assert_eq!(not_found, StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test -- --test-threads=1
```

Expected: FAIL — module `inbound` belum ada.

- [ ] **Step 3: Implementasi `inbound.rs` (bagian create)**

```rust
//! Endpoint ingest publik untuk webhook Alertmanager.
//! Spec: `docs/superpowers/specs/2026-10-04-intake-webhook-source-design.md`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{routes::intake::PRIORITIES, state::AppState};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Source {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub type_id: Option<Uuid>,
    pub is_active: bool,
    pub auto_accept: bool,
    pub config: Value,
    pub created_by_id: Option<Uuid>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Series {
    pub issue_id: Uuid,
    pub row_id: Uuid,
    pub status: i32,
    pub state_group: Option<String>,
}

#[derive(Default)]
struct Counts {
    created: u64,
    updated: u64,
    reopened: u64,
    accepted: u64,
    declined: u64,
    resolved: u64,
    ignored: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Created,
    Updated,
    Reopened,
    Declined,
    Resolved,
    Ignored,
}

impl Counts {
    fn bump(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Created => self.created += 1,
            Outcome::Updated => self.updated += 1,
            Outcome::Reopened => self.reopened += 1,
            Outcome::Declined => self.declined += 1,
            Outcome::Resolved => self.resolved += 1,
            Outcome::Ignored => self.ignored += 1,
        }
    }

    fn bump_accepted(&mut self) {
        self.accepted += 1;
    }
}

pub async fn alertmanager(
    State(st): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let source = sqlx::query_as::<_, Source>(
        "SELECT s.id, s.project_id, s.name, s.type_id, s.is_active, s.auto_accept, s.config, s.created_by_id \
         FROM intake_sources s \
         JOIN projects p ON p.id = s.project_id AND p.deleted_at IS NULL \
         WHERE s.token = $1 AND s.deleted_at IS NULL",
    )
    .bind(&token)
    .fetch_optional(&st.pool)
    .await?;
    let Some(source) = source else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Unknown token"}))));
    };
    if !source.is_active {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": "Intake source is inactive"}))));
    }
    let Some(alerts) = body.get("alerts").and_then(Value::as_array) else {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "alerts must be an array"}))));
    };
    let common_labels = body.get("commonLabels").cloned().unwrap_or_else(|| json!({}));
    let common_annotations = body
        .get("commonAnnotations")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let mut counts = Counts::default();
    for alert in alerts {
        let status = alert.get("status").and_then(Value::as_str).unwrap_or("firing");
        if status == "resolved" {
            // Task 6 mengisi resolve; sementara dihitung ignored.
            counts.bump(Outcome::Ignored);
            continue;
        }
        let outcome = create_series(&st, &source, alert, &common_labels, &common_annotations).await?;
        counts.bump(outcome);
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "created": counts.created,
            "updated": counts.updated,
            "reopened": counts.reopened,
            "accepted": counts.accepted,
            "declined": counts.declined,
            "resolved": counts.resolved,
            "ignored": counts.ignored,
        })),
    ))
}

/// Baca label dari label alert, fallback ke `commonLabels`.
fn label_value<'a>(alert_labels: &'a Value, common_labels: &'a Value, key: &str) -> Option<&'a str> {
    alert_labels
        .get(key)
        .and_then(Value::as_str)
        .or_else(|| common_labels.get(key).and_then(Value::as_str))
}

fn annotation(alert: &Value, common: &Value, key: &str) -> Option<String> {
    alert
        .get("annotations")
        .and_then(|a| a.get(key))
        .and_then(Value::as_str)
        .or_else(|| common.get(key).and_then(Value::as_str))
        .map(str::to_string)
}

fn map_service(config: &Value, alert_labels: &Value, common_labels: &Value) -> Option<Uuid> {
    let key = config
        .get("service_label_key")
        .and_then(Value::as_str)
        .unwrap_or("service");
    if let Some(label) = label_value(alert_labels, common_labels, key) {
        if let Some(id) = config
            .get("service_map")
            .and_then(|m| m.get(label))
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            return Some(id);
        }
    }
    config
        .get("fallback_service_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn map_priority(config: &Value, alert_labels: &Value, common_labels: &Value) -> String {
    let key = config
        .get("severity_label_key")
        .and_then(Value::as_str)
        .unwrap_or("severity");
    let mapped = label_value(alert_labels, common_labels, key)
        .and_then(|severity| config.get("severity_map").and_then(|m| m.get(severity)))
        .and_then(Value::as_str)
        .filter(|p| PRIORITIES.contains(p))
        .map(str::to_string);
    mapped
        .or_else(|| {
            config
                .get("default_priority")
                .and_then(Value::as_str)
                .filter(|p| PRIORITIES.contains(p))
                .map(str::to_string)
        })
        .unwrap_or_else(|| "none".to_string())
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn fingerprint_of(alert: &Value) -> String {
    if let Some(fp) = alert
        .get("fingerprint")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return fp.to_string();
    }
    let mut labels: Vec<(String, String)> = alert
        .get("labels")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    labels.sort();
    let joined = labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(joined.as_bytes()))
}

async fn resolve_or_create_triage(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project_id: Uuid,
    workspace_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND \"group\" = 'triage' AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&mut **tx)
    .await?;
    match existing {
        Some(id) => Ok(id),
        None => sqlx::query_scalar(
            "INSERT INTO states (id, name, description, slug, \"group\", color, sequence, is_triage, \"default\", project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Triage', '', 'triage', 'triage', '#4E5355', 65000, false, false, $1, $2, now(), now()) RETURNING id",
        )
        .bind(project_id)
        .bind(workspace_id)
        .fetch_one(&mut **tx)
        .await,
    }
}

async fn resolve_or_create_intake(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM intakes WHERE project_id = $1 AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&mut **tx)
    .await?;
    match existing {
        Some(id) => Ok(id),
        None => sqlx::query_scalar(
            "INSERT INTO intakes (id, name, description, is_default, view_props, logo_props, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Intake', '', true, '{}'::jsonb, '{}'::jsonb, $1, $2, $3, $3, now(), now()) RETURNING id",
        )
        .bind(project_id)
        .bind(workspace_id)
        .bind(actor)
        .fetch_one(&mut **tx)
        .await,
    }
}

async fn create_series(
    st: &AppState,
    source: &Source,
    alert: &Value,
    common_labels: &Value,
    common_annotations: &Value,
) -> Result<Outcome, common::errors::AppError> {
    let Some(actor) = source.created_by_id else {
        return Err(common::errors::AppError(anyhow::anyhow!(
            "intake source without created_by_id"
        )));
    };
    let labels = alert.get("labels").cloned().unwrap_or_else(|| json!({}));
    let fingerprint = fingerprint_of(alert);
    let (workspace_id, workspace_slug): (Uuid, String) = sqlx::query_as(
        "SELECT w.id, w.slug FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND p.deleted_at IS NULL",
    )
    .bind(source.project_id)
    .fetch_one(&st.pool)
    .await?;

    let title = annotation(alert, common_annotations, "summary")
        .or_else(|| label_value(&labels, common_labels, "alertname").map(str::to_string))
        .unwrap_or_else(|| "Alert".to_string());
    let mut description = String::new();
    if let Some(text) = annotation(alert, common_annotations, "description") {
        description.push_str(&format!("<p>{}</p>", escape_html(&text)));
    }
    if let Some(url) = alert.get("generatorURL").and_then(Value::as_str) {
        description.push_str(&format!(
            "<p><a href=\"{}\">{}</a></p>",
            escape_html(url),
            escape_html(url)
        ));
    }
    if description.is_empty() {
        description.push_str("<p></p>");
    }
    let priority = map_priority(&source.config, &labels, common_labels);
    let service_id = map_service(&source.config, &labels, common_labels);

    let mut tx = st.pool.begin().await?;
    let triage_id = resolve_or_create_triage(&mut tx, source.project_id, workspace_id).await?;
    let intake_id =
        resolve_or_create_intake(&mut tx, source.project_id, workspace_id, actor).await?;
    let issue = super::issue_write::insert_issue(
        &mut tx,
        super::issue_write::NewIssue {
            slug: &workspace_slug,
            project_id: source.project_id,
            state_id: Some(triage_id),
            name: &title,
            description_html: &description,
            priority: &priority,
            start_date: None,
            target_date: None,
            parent_id: None,
            type_id: source.type_id,
            estimate_point_id: None,
            created_by: actor,
        },
    )
    .await?;
    super::issue_version_write::record_description_version(
        &mut tx,
        issue.id,
        source.project_id,
        workspace_id,
        actor,
        Some(actor),
        None,
        &description,
        &json!({}),
    )
    .await?;
    sqlx::query(
        "UPDATE issues SET intake_source_id = $1, intake_fingerprint = $2, \
         intake_occurrence_count = 1, intake_last_seen_at = now() WHERE id = $3",
    )
    .bind(source.id)
    .bind(&fingerprint)
    .bind(issue.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO intake_issues (id, intake_id, issue_id, status, extra, source, project_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, -2, '{}'::jsonb, 'WEBHOOK', $3, $4, now(), now())",
    )
    .bind(intake_id)
    .bind(issue.id)
    .bind(source.project_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?;
    if let Some(service_id) = service_id {
        sqlx::query(
            "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
        )
        .bind(workspace_id)
        .bind(source.project_id)
        .bind(service_id)
        .bind(issue.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Outcome::Created)
}
```

- [ ] **Step 4: `PRIORITIES` jadi `pub(crate)` + mount route**

Di `apps/api-rs/crates/api/src/routes/intake.rs` baris 44: `const PRIORITIES` → `pub(crate) const PRIORITIES`.

`main.rs`, dekat route `/health`:

```rust
        .route(
            "/api/inbound/alertmanager/:token/",
            post(routes::inbound::alertmanager),
        )
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test -- --test-threads=1
```

Expected: PASS (1 test). Bila gagal karena `used before defined`, cek urutan `mod`/import.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/inbound.rs apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/inbound_intake_test.rs
git commit -m "feat(api-rs): ingest Alertmanager alerts as pending intake items"
```

### Task 5: Dedup firing — occurrence, declined reopen, completed reopen

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/inbound.rs`
- Test: `apps/api-rs/crates/api/tests/inbound_intake_test.rs`

- [ ] **Step 1: Tambah failing test**

```rust
#[tokio::test]
async fn firing_upserts_series_and_reopens_declined() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    scratch.add_default_state(&pool).await;
    let (source_id, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({ "service_map": { "payment-api": service_id } }),
        )
        .await;
    let st = state(&pool).await;

    for _ in 0..2 {
        let (status, _) = api::routes::inbound::alertmanager(
            State(st.clone()),
            Path(token.clone()),
            Json(alert_payload("fp-dup", "firing", "warning")),
        )
        .await
        .unwrap();
        assert_eq!(status, StatusCode::ACCEPTED);
    }

    let (count, occurrence, row_status): (i64, i32, i32) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM issues WHERE intake_source_id = $1 AND deleted_at IS NULL), \
                i.intake_occurrence_count, ii.status \
         FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = 'fp-dup'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(occurrence, 2);
    assert_eq!(row_status, -2);

    // Triager decline → firing berikutnya membuka lagi.
    sqlx::query(
        "UPDATE intake_issues SET status = -1 WHERE issue_id IN \
         (SELECT id FROM issues WHERE intake_source_id = $1 AND intake_fingerprint = 'fp-dup')",
    )
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();
    let (_, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-dup", "firing", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(body["reopened"], 1);
    let (status_again,): (i32,) = sqlx::query_as(
        "SELECT ii.status FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = 'fp-dup'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status_again, -2);
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test firing_upserts -- --test-threads=1
```

Expected: FAIL — `count = 2` (duplikat item) atau `occurrence` tetap 1.

- [ ] **Step 3: Implementasi `find_series` + `fire_series`**

Tambah di `inbound.rs`:

```rust
async fn find_series(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    fingerprint: &str,
) -> Result<Option<Series>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.id AS issue_id, ii.id AS row_id, ii.status, st.\"group\" AS state_group \
         FROM issues i \
         JOIN intake_issues ii ON ii.issue_id = i.id AND ii.deleted_at IS NULL \
         LEFT JOIN states st ON st.id = i.state_id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = $2 \
         AND i.deleted_at IS NULL LIMIT 1",
    )
    .bind(source_id)
    .bind(fingerprint)
    .fetch_optional(pool)
    .await
}

async fn fire_series(
    st: &AppState,
    source: &Source,
    series: &Series,
    actor: Uuid,
) -> Result<Outcome, common::errors::AppError> {
    sqlx::query(
        "UPDATE issues SET intake_occurrence_count = intake_occurrence_count + 1, \
         intake_last_seen_at = now(), updated_at = now() WHERE id = $1",
    )
    .bind(series.issue_id)
    .execute(&st.pool)
    .await?;

    if series.status == -2 || series.status == 0 || series.status == 2 {
        return Ok(Outcome::Updated);
    }
    if series.status == -1 {
        sqlx::query(
            "UPDATE intake_issues SET status = -2, updated_at = now() \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(series.row_id)
        .execute(&st.pool)
        .await?;
        return Ok(Outcome::Reopened);
    }
    // status == 1: hanya reopen bila issue sudah selesai/dibatalkan.
    let closed = matches!(
        series.state_group.as_deref(),
        Some("completed") | Some("cancelled")
    );
    if !closed {
        return Ok(Outcome::Updated);
    }
    let Some(target) =
        super::issue_common::resolve_issue_state(&st.pool, source.project_id, None).await?
    else {
        tracing::warn!(issue_id = %series.issue_id, "alert refire: no default state to reopen");
        return Ok(Outcome::Updated);
    };
    sqlx::query("UPDATE issues SET state_id = $2, completed_at = NULL, updated_at = now() WHERE id = $1")
        .bind(series.issue_id)
        .bind(target)
        .execute(&st.pool)
        .await?;
    system_comment(
        st,
        series.issue_id,
        source.project_id,
        actor,
        "<p>Reopened by alert refire</p>",
    )
    .await?;
    Ok(Outcome::Reopened)
}
```

Tambah `system_comment` (dipakai juga Task 6):

```rust
async fn system_comment(
    st: &AppState,
    issue_id: Uuid,
    project_id: Uuid,
    actor: Uuid,
    html: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO issue_comments (id, comment_html, comment_json, comment_stripped, access, attachments, \
         issue_id, project_id, workspace_id, actor_id, created_by_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, '{}', '', 'INTERNAL', '{}', $2, $3, i.workspace_id, $4, $4, now(), now() \
         FROM issues i WHERE i.id = $2",
    )
    .bind(html)
    .bind(issue_id)
    .bind(project_id)
    .bind(actor)
    .execute(&st.pool)
    .await?;
    Ok(())
}
```

Ganti cabang firing di `alertmanager` loop menjadi:

```rust
        let labels = alert.get("labels").cloned().unwrap_or_else(|| json!({}));
        let fingerprint = fingerprint_of(alert);
        let outcome = match find_series(&st.pool, source.id, &fingerprint).await? {
            Some(series) => {
                let actor = source.created_by_id.unwrap_or(series.issue_id);
                fire_series(&st, &source, &series, actor).await?
            }
            None => create_series(&st, &source, alert, &common_labels, &common_annotations).await?,
        };
        counts.bump(outcome);
```

(Catatan: `labels` sementara tidak dipakai untuk `find_series`; teruskan ke fungsi klasifikasi bila compiler mengeluh `unused` — hapus binding `labels` di loop ini.)

- [ ] **Step 4: Jalankan test, pastikan lulus**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test firing_upserts -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/inbound.rs apps/api-rs/crates/api/tests/inbound_intake_test.rs
git commit -m "feat(api-rs): dedup alert series and reopen on refire"
```

---

### Task 6: Resolve — auto-decline pending, complete accepted

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/inbound.rs`
- Test: `apps/api-rs/crates/api/tests/inbound_intake_test.rs`

- [ ] **Step 1: Tambah failing test**

```rust
#[tokio::test]
async fn resolved_declines_pending_and_completes_accepted() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    scratch.add_default_state(&pool).await;
    scratch.add_completed_state(&pool).await;
    let (source_id, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({ "service_map": { "payment-api": service_id } }),
        )
        .await;
    let st = state(&pool).await;

    // Pending → resolved = declined.
    let _ = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-res", "firing", "warning")),
    )
    .await
    .unwrap();
    let (_, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-res", "resolved", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(body["declined"], 1);
    let (declined,): (i32,) = sqlx::query_as(
        "SELECT ii.status FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = 'fp-res'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(declined, -1);

    // Resolved duplikat = ignored.
    let (_, Json(again)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-res", "resolved", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(again["ignored"], 1);

    // Accepted (manual) + resolved = completed.
    let _ = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-acc", "firing", "warning")),
    )
    .await
    .unwrap();
    let completed_state: Uuid = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND \"group\" = 'completed' LIMIT 1",
    )
    .bind(scratch.project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let default_state: Uuid = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND \"group\" = 'backlog' LIMIT 1",
    )
    .bind(scratch.project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let _ = default_state;
    sqlx::query(
        "UPDATE intake_issues SET status = 1 WHERE issue_id IN \
         (SELECT id FROM issues WHERE intake_fingerprint = 'fp-acc')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (_, Json(resolved)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-acc", "resolved", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(resolved["resolved"], 1);
    let (state_id,): (Option<Uuid>,) = sqlx::query_as(
        "SELECT state_id FROM issues WHERE intake_fingerprint = 'fp-acc'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state_id, Some(completed_state));

    // Refire setelah completed = reopen ke state default.
    let (_, Json(reopened)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-acc", "firing", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(reopened["reopened"], 1);
    let (state_id, completed_at): (Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT state_id, completed_at FROM issues WHERE intake_fingerprint = 'fp-acc'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state_id, Some(default_state));
    assert!(completed_at.is_none());
}
```

Tambah `add_completed_state` di harness:

```rust
async fn add_completed_state(&self, pool: &PgPool) -> Uuid {
    let state_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, slug, \"group\", color, sequence, is_triage, \
         \"default\", project_id, workspace_id, created_at, updated_at) \
         VALUES ($1, 'Done', '', 'done', 'completed', '#4E5355', 2000, false, false, $2, $3, now(), now())",
    )
    .bind(state_id)
    .bind(self.project_id)
    .bind(self.workspace_id)
    .execute(pool)
    .await
    .expect("scratch completed state");
    state_id
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test resolved_declines -- --test-threads=1
```

Expected: FAIL — `declined` 0 (masih `ignored`).

- [ ] **Step 3: Implementasi `resolve_series`**

Tambah di `inbound.rs`:

```rust
async fn first_completed_state(
    pool: &sqlx::PgPool,
    project_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" = 'completed' ORDER BY sequence ASC, created_at ASC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

async fn resolve_series(
    st: &AppState,
    source: &Source,
    series: &Series,
    actor: Uuid,
) -> Result<Outcome, common::errors::AppError> {
    match series.status {
        -2 => {
            sqlx::query(
                "UPDATE intake_issues SET status = -1, updated_at = now() \
                 WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(series.row_id)
            .execute(&st.pool)
            .await?;
            Ok(Outcome::Declined)
        }
        1 => {
            if matches!(
                series.state_group.as_deref(),
                Some("completed") | Some("cancelled")
            ) {
                return Ok(Outcome::Ignored);
            }
            let Some(target) = first_completed_state(&st.pool, source.project_id).await? else {
                tracing::warn!(
                    issue_id = %series.issue_id,
                    "alert resolved but project has no completed state"
                );
                return Ok(Outcome::Ignored);
            };
            sqlx::query(
                "UPDATE issues SET state_id = $2, completed_at = now(), updated_at = now() WHERE id = $1",
            )
            .bind(series.issue_id)
            .bind(target)
            .execute(&st.pool)
            .await?;
            system_comment(
                st,
                series.issue_id,
                source.project_id,
                actor,
                "<p>Auto-resolved by Alertmanager</p>",
            )
            .await?;
            Ok(Outcome::Resolved)
        }
        // snoozed (0), declined (-1), duplicate (2)
        _ => Ok(Outcome::Ignored),
    }
}
```

Ganti cabang `resolved` di loop `alertmanager` menjadi:

```rust
        if status == "resolved" {
            let fingerprint = fingerprint_of(alert);
            let outcome = match find_series(&st.pool, source.id, &fingerprint).await? {
                Some(series) => {
                    let actor = source.created_by_id.unwrap_or(series.issue_id);
                    resolve_series(&st, &source, &series, actor).await?
                }
                None => Outcome::Ignored,
            };
            counts.bump(outcome);
            continue;
        }
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test resolved_declines -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/inbound.rs apps/api-rs/crates/api/tests/inbound_intake_test.rs
git commit -m "feat(api-rs): resolve and auto-decline alert series"
```

---

### Task 7: Auto-accept per source

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` (helper bersama)
- Modify: `apps/api-rs/crates/api/src/routes/inbound.rs`
- Test: `apps/api-rs/crates/api/tests/inbound_intake_test.rs`

- [ ] **Step 1: Tambah failing test**

```rust
#[tokio::test]
async fn auto_accept_only_when_classification_complete() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    scratch.add_default_state(&pool).await;
    let (_, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({ "service_map": { "payment-api": service_id } }),
        )
        .await;
    // Nyalakan auto_accept.
    sqlx::query("UPDATE intake_sources SET auto_accept = true WHERE token = $1")
        .bind(&token)
        .execute(&pool)
        .await
        .unwrap();
    let st = state(&pool).await;

    let (_, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-auto", "firing", "critical")),
    )
    .await
    .unwrap();
    assert_eq!(body["accepted"], 1);
    let (status, state_group): (i32, Option<String>) = sqlx::query_as(
        "SELECT ii.status, st.\"group\" FROM issues i \
         JOIN intake_issues ii ON ii.issue_id = i.id \
         LEFT JOIN states st ON st.id = i.state_id \
         WHERE i.intake_fingerprint = 'fp-auto'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, 1);
    assert_ne!(state_group.as_deref(), Some("triage"));

    // Source tanpa mapping service → tetap pending (Incident requires_service).
    let (_, token_no_service) = scratch
        .add_source_with_config(&pool, type_id, serde_json::json!({}))
        .await;
    sqlx::query("UPDATE intake_sources SET auto_accept = true WHERE token = $1")
        .bind(&token_no_service)
        .execute(&pool)
        .await
        .unwrap();
    let (_, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token_no_service.clone()),
        Json(alert_payload("fp-nosvc", "firing", "critical")),
    )
    .await
    .unwrap();
    assert_eq!(body["accepted"], 0);
    assert_eq!(body["created"], 1);
    let (status,): (i32,) = sqlx::query_as(
        "SELECT ii.status FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_fingerprint = 'fp-nosvc'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, -2);
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test auto_accept_only -- --test-threads=1
```

Expected: FAIL — `accepted` 0.

- [ ] **Step 3: Implementasi helper `accept_intake_issue` di `intake.rs`**

Tambah setelah `accept_gate`:

```rust
/// Accept intake issue dari jalur non-HTTP (webhook auto-accept): gate,
/// status → 1, lalu pindah issue keluar dari triage. Return
/// `Ok(Err(pesan))` bila gate menolak (item tetap pending), `Err` hanya
/// untuk kegagalan DB. SQL sengaja dicerminkan dari `patch_issue` agar
/// PATCH manual dan ingest tidak divergen.
pub(crate) async fn accept_intake_issue(
    pool: &sqlx::PgPool,
    project_id: uuid::Uuid,
    row_id: uuid::Uuid,
    issue_id: uuid::Uuid,
    actor: Option<uuid::Uuid>,
) -> Result<Result<(), &'static str>, sqlx::Error> {
    if let Some(message) = accept_gate(pool, project_id, issue_id).await? {
        return Ok(Err(message));
    }
    let Some(target_state) = resolve_issue_state(pool, project_id, None).await? else {
        return Ok(Err(
            "Cannot accept intake issue: No default state found for the project",
        ));
    };
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE intake_issues SET status = 1, updated_at = now(), updated_by_id = $2 \
         WHERE id = $1 AND deleted_at IS NULL AND status = -2",
    )
    .bind(row_id)
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE issues i SET state_id = $2, \
           completed_at = CASE WHEN (SELECT \"group\" FROM states WHERE id = $2) = 'completed' \
                               THEN now() ELSE NULL END, \
           updated_at = now(), updated_by_id = $3 \
         WHERE i.id = $1 AND i.state_id IN \
           (SELECT id FROM states WHERE project_id = i.project_id AND \"group\" = 'triage' AND deleted_at IS NULL)",
    )
    .bind(issue_id)
    .bind(target_state)
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Ok(()))
}
```

- [ ] **Step 4: Wire auto-accept di `inbound.rs`**

Ganti isi loop `alertmanager` (bagian firing + resolved) menjadi:

```rust
        let resolved = status == "resolved";
        let fingerprint = fingerprint_of(alert);
        let existing = find_series(&st.pool, source.id, &fingerprint).await?;
        let actor = source.created_by_id.unwrap_or_else(Uuid::nil);
        if !resolved && actor.is_nil() {
            return Err(common::errors::AppError(anyhow::anyhow!(
                "intake source without created_by_id"
            )));
        }

        let outcome = if resolved {
            match &existing {
                Some(series) => resolve_series(&st, &source, series, actor).await?,
                None => Outcome::Ignored,
            }
        } else {
            match &existing {
                Some(series) => fire_series(&st, &source, series, actor).await?,
                None => create_series(&st, &source, alert, &common_labels, &common_annotations).await?,
            }
        };

        // Auto-accept: hanya untuk firing yang berakhir pending.
        let mut counted = false;
        if !resolved && source.auto_accept && matches!(outcome, Outcome::Created | Outcome::Updated | Outcome::Reopened) {
            let pending = match &existing {
                Some(series) => series.status == -1 || series.status == -2,
                None => true,
            };
            if pending {
                let (row_id, issue_id) = match &existing {
                    Some(series) => (series.row_id, series.issue_id),
                    None => sqlx::query_as::<_, (Uuid, Uuid)>(
                        "SELECT ii.id, ii.issue_id FROM issues i \
                         JOIN intake_issues ii ON ii.issue_id = i.id \
                         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = $2 \
                         AND i.deleted_at IS NULL",
                    )
                    .bind(source.id)
                    .bind(&fingerprint)
                    .fetch_one(&st.pool)
                    .await?,
                };
                match accept_intake_issue(&st.pool, source.project_id, row_id, issue_id, Some(actor)).await? {
                    Ok(()) => {
                        counts.bump_accepted();
                        counted = true;
                    }
                    Err(message) => tracing::warn!(
                        source_id = %source.id,
                        fingerprint = %fingerprint,
                        message,
                        "auto-accept skipped"
                    ),
                }
            }
        }
        if !counted {
            counts.bump(outcome);
        }
```

Import `accept_intake_issue`: ubah baris import menjadi `use crate::{routes::intake::{accept_intake_issue, PRIORITIES}, state::AppState};`

- [ ] **Step 5: Jalankan test, pastikan lulus**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test auto_accept_only -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/inbound.rs apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/tests/inbound_intake_test.rs
git commit -m "feat(api-rs): auto-accept classified webhook intake"
```

---

### Task 8: Fingerprint fallback, batch campuran, payload invalid

**Files:**

- Test: `apps/api-rs/crates/api/tests/inbound_intake_test.rs`
- Modify (bila perlu): `apps/api-rs/crates/api/src/routes/inbound.rs`

- [ ] **Step 1: Tambah test**

```rust
#[tokio::test]
async fn batch_common_labels_and_fingerprint_fallback() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    scratch.add_default_state(&pool).await;
    scratch.add_completed_state(&pool).await;
    let (source_id, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({
                "service_map": { "payment-api": service_id },
                "severity_map": { "critical": "urgent" },
                "default_priority": "none"
            }),
        )
        .await;
    let st = state(&pool).await;

    // Dua alert tanpa fingerprint; severity hanya di commonLabels.
    let payload = serde_json::json!({
        "version": "4",
        "commonLabels": { "severity": "critical", "service": "payment-api" },
        "alerts": [
            { "status": "firing", "labels": { "alertname": "A" }, "annotations": { "summary": "A down" } },
            { "status": "firing", "labels": { "alertname": "B" }, "annotations": { "summary": "B down" } }
        ]
    });
    let (status, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(payload.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["created"], 2);

    // Kirim ulang payload yang sama: idempoten (update, bukan duplikat).
    let (_, Json(second)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(payload.clone()),
    )
    .await
    .unwrap();
    assert_eq!(second["created"], 0);
    assert_eq!(second["updated"], 2);
    let (total,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issues WHERE intake_source_id = $1 AND deleted_at IS NULL",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total, 2);
    let (urgent_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issues WHERE intake_source_id = $1 AND priority = 'urgent' AND deleted_at IS NULL",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(urgent_count, 2);

    // Body invalid → 400.
    let (bad, _) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(serde_json::json!({ "alerts": "nope" })),
    )
    .await
    .unwrap();
    assert_eq!(bad, StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Jalankan test**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test batch_common -- --test-threads=1
```

Expected: PASS bila implementasi Task 4–7 benar. Bila `urgent_count = 0`, perbaiki `map_priority` agar membaca `commonLabels` (sudah dilakukan di Task 4) — jangan lanjut sebelum lulus.

- [ ] **Step 3: Jalankan seluruh suite file ini**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test -- --test-threads=1
```

Expected: PASS (5 test).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/tests/inbound_intake_test.rs apps/api-rs/crates/api/src/routes/inbound.rs
git commit -m "test(api-rs): cover batch, fallback fingerprint and invalid payload"
```

---

### Task 9: Detail intake memuat source

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs`
- Test: `apps/api-rs/crates/api/tests/inbound_intake_test.rs`

- [ ] **Step 1: Tambah failing test**

```rust
#[tokio::test]
async fn detail_exposes_webhook_source() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    let (_, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({ "service_map": { "payment-api": service_id } }),
        )
        .await;
    let st = state(&pool).await;

    let _ = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token),
        Json(alert_payload("fp-detail", "firing", "warning")),
    )
    .await
    .unwrap();
    let issue_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM issues WHERE intake_fingerprint = 'fp-detail'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (status, Json(detail)) = api::routes::intake::detail_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["source"], "WEBHOOK");
    assert_eq!(detail["intake_source"]["name"], "Prometheus Prod");
    assert_eq!(detail["intake_source"]["occurrence_count"], 1);
    assert!(detail["intake_source"]["last_seen_at"].is_string());
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test detail_exposes -- --test-threads=1
```

Expected: FAIL — `intake_source` null.

- [ ] **Step 3: Implementasi**

Di `intake.rs`:

1. Tambah struct:

```rust
/// Atribusi sumber webhook (ekstensi Rust; Django tidak punya serializer ini).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct InboxIntakeSource {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) occurrence_count: i32,
    pub(crate) last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

2. Tambah field di `InboxIssueDetail` setelah `issue`:

```rust
    pub(crate) intake_source: Option<InboxIntakeSource>,
```

3. Di `fetch_inbox_detail`, setelah `issue_row` didapat:

```rust
    let intake_source: Option<InboxIntakeSource> = sqlx::query_as(
        "SELECT s.id, s.name, i.intake_occurrence_count AS occurrence_count, i.intake_last_seen_at AS last_seen_at \
         FROM issues i JOIN intake_sources s ON s.id = i.intake_source_id AND s.deleted_at IS NULL \
         WHERE i.id = $1 AND i.intake_source_id IS NOT NULL",
    )
    .bind(issue_id)
    .fetch_optional(pool)
    .await?;
```

lalu set `intake_source` di construction `InboxIssueDetail { ... }`. Cari `Ok(Some(InboxIssueDetail {` di file dan tambahkan field.

4. Update konstanta + unit test key order:

```rust
pub(crate) const INBOX_DETAIL_KEYS: [&str; 8] = [
    "id",
    "status",
    "duplicate_to",
    "snoozed_till",
    "duplicate_issue_detail",
    "source",
    "issue",
    "intake_source",
];
```

dan di test `detail_keys_follow_django_field_order` tambahkan `"intake_source",` pada array expected + komentar `// "intake_source" = ekstensi Rust (webhook)`.

- [ ] **Step 4: Jalankan test, pastikan lulus**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test inbound_intake_test -- --test-threads=1
```

Expected: PASS (6 test).

- [ ] **Step 5: Jalankan test modul intake (regresi key order)**

```bash
cd apps/api-rs && cargo test -p api routes::intake::tests
```

Expected: PASS. Bila ada test lain yang membandingkan detail keys, sesuaikan.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/tests/inbound_intake_test.rs
git commit -m "feat(api-rs): expose intake webhook source on detail"
```

### Task 10: Tipe FE `TIntakeSource` + enum `WEBHOOK`

**Files:**

- Create: `packages/types/src/intake-source.ts`
- Modify: `packages/types/src/index.ts`
- Modify: `packages/types/src/inbox.ts`

- [ ] **Step 1: Buat `intake-source.ts`**

```ts
export type TIntakeSourcePriority = "urgent" | "high" | "medium" | "low" | "none";

export type TIntakeSourceConfig = {
  service_label_key?: string;
  service_map?: Record<string, string>;
  fallback_service_id?: string | null;
  severity_label_key?: string;
  severity_map?: Record<string, TIntakeSourcePriority>;
  default_priority?: TIntakeSourcePriority;
};

export type TIntakeSource = {
  id: string;
  project_id: string;
  name: string;
  token: string;
  is_active: boolean;
  auto_accept: boolean;
  type_id: string | null;
  config: TIntakeSourceConfig;
  created_at: string;
  updated_at: string;
  created_by: string | null;
};

export type TIntakeSourcePayload = {
  name: string;
  type_id: string;
  auto_accept?: boolean;
  config?: TIntakeSourceConfig;
};
```

- [ ] **Step 2: Ekspor + enum + detail type**

`packages/types/src/index.ts`: tambah `export * from "./intake-source";` (mengikuti urutan alfabetis).

`packages/types/src/inbox.ts`:

1. Tambah ke `EInboxIssueSource`:

```ts
  WEBHOOK = "WEBHOOK",
```

2. Tambah tipe + field:

```ts
export type TInboxIntakeSource = {
  id: string;
  name: string;
  occurrence_count: number;
  last_seen_at: string | null;
};
```

dan di `TInboxIssue` tambah `intake_source: TInboxIntakeSource | null;` setelah `source`.

- [ ] **Step 3: Cek tipe**

```bash
pnpm --filter=@plane/types check:types
```

Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add packages/types/src/intake-source.ts packages/types/src/index.ts packages/types/src/inbox.ts
git commit -m "feat(types): add intake source and webhook source types"
```

---

### Task 11: FE service, helpers, store

**Files:**

- Create: `apps/web/core/services/intake-source.service.ts`
- Create: `apps/web/core/store/intake-source.helpers.ts`
- Create: `apps/web/core/store/intake-source.helpers.test.ts`
- Create: `apps/web/core/store/intake-source.store.ts`
- Create: `apps/web/core/hooks/store/use-intake-source.ts`
- Modify: `apps/web/core/store/root.store.ts`
- Modify: `apps/web/core/hooks/store/index.ts`

- [ ] **Step 1: Tulis helper test dulu**

`intake-source.helpers.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  buildWebhookUrl,
  configFromForm,
  serviceRowsFromConfig,
  severityRowsFromConfig,
  validateIntakeSourceForm,
} from "./intake-source.helpers";

describe("intake-source helpers", () => {
  it("builds the webhook url", () => {
    expect(buildWebhookUrl("https://api.example.com/", "plane_is_abc")).toBe(
      "https://api.example.com/api/inbound/alertmanager/plane_is_abc/"
    );
  });

  it("round-trips config rows", () => {
    const config = configFromForm({
      serviceLabelKey: "service",
      serviceRows: [{ labelValue: "payment", serviceId: "s1" }],
      fallbackServiceId: "s1",
      severityLabelKey: "severity",
      severityRows: [{ labelValue: "critical", priority: "urgent" }],
      defaultPriority: "none",
    });
    expect(serviceRowsFromConfig(config)).toEqual([{ labelValue: "payment", serviceId: "s1" }]);
    expect(severityRowsFromConfig(config)).toEqual([{ labelValue: "critical", priority: "urgent" }]);
  });

  it("rejects duplicate label values and unknown services", () => {
    const base = {
      name: "Prometheus",
      typeId: "t1",
      autoAccept: false,
      serviceLabelKey: "service",
      serviceRows: [
        { labelValue: "payment", serviceId: "s1" },
        { labelValue: "payment", serviceId: "s1" },
      ],
      fallbackServiceId: "s1",
      severityLabelKey: "severity",
      severityRows: [{ labelValue: "critical", priority: "urgent" as const }],
      defaultPriority: "none" as const,
    };
    expect(validateIntakeSourceForm(base, ["s1", "t1"])).not.toBeNull();
    const unknown = { ...base, serviceRows: [{ labelValue: "payment", serviceId: "nope" }] };
    expect(validateIntakeSourceForm(unknown, ["s1", "t1"])).not.toBeNull();
    const valid = { ...base, serviceRows: [{ labelValue: "payment", serviceId: "s1" }] };
    expect(validateIntakeSourceForm(valid, ["s1", "t1"])).toBeNull();
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

```bash
pnpm --filter=web exec vitest run core/store/intake-source.helpers.test.ts
```

Expected: FAIL — modul belum ada.

- [ ] **Step 3: Implementasi helper**

`intake-source.helpers.ts`:

```ts
import type { TIntakeSourceConfig, TIntakeSourcePriority } from "@plane/types";

export const INTAKE_PRIORITIES: TIntakeSourcePriority[] = ["urgent", "high", "medium", "low", "none"];

export type TServiceRow = { labelValue: string; serviceId: string };
export type TSeverityRow = { labelValue: string; priority: TIntakeSourcePriority };

export type TIntakeSourceForm = {
  name: string;
  typeId: string;
  autoAccept: boolean;
  serviceLabelKey: string;
  serviceRows: TServiceRow[];
  fallbackServiceId: string | null;
  severityLabelKey: string;
  severityRows: TSeverityRow[];
  defaultPriority: TIntakeSourcePriority;
};

export const buildWebhookUrl = (apiBaseUrl: string, token: string): string =>
  `${apiBaseUrl.replace(/\/+$/, "")}/api/inbound/alertmanager/${token}/`;

export const serviceRowsFromConfig = (config?: TIntakeSourceConfig): TServiceRow[] =>
  Object.entries(config?.service_map ?? {}).map(([labelValue, serviceId]) => ({ labelValue, serviceId }));

export const severityRowsFromConfig = (config?: TIntakeSourceConfig): TSeverityRow[] =>
  Object.entries(config?.severity_map ?? {}).map(([labelValue, priority]) => ({
    labelValue,
    priority: priority as TIntakeSourcePriority,
  }));

export const configFromForm = (
  form: Pick<
    TIntakeSourceForm,
    "serviceLabelKey" | "serviceRows" | "fallbackServiceId" | "severityLabelKey" | "severityRows" | "defaultPriority"
  >
): TIntakeSourceConfig => ({
  service_label_key: form.serviceLabelKey.trim() || "service",
  service_map: Object.fromEntries(
    form.serviceRows
      .filter((row) => row.labelValue.trim() && row.serviceId)
      .map((row) => [row.labelValue.trim(), row.serviceId])
  ),
  fallback_service_id: form.fallbackServiceId || null,
  severity_label_key: form.severityLabelKey.trim() || "severity",
  severity_map: Object.fromEntries(
    form.severityRows
      .filter((row) => row.labelValue.trim() && row.priority)
      .map((row) => [row.labelValue.trim(), row.priority])
  ),
  default_priority: form.defaultPriority,
});

export const validateIntakeSourceForm = (form: TIntakeSourceForm, validServiceIds: string[]): string | null => {
  if (!form.name.trim()) return "Name is required";
  if (!form.typeId) return "Select a work item type";
  const labels = new Set<string>();
  for (const row of form.serviceRows) {
    const label = row.labelValue.trim();
    if (!label) return "Service label value is required";
    if (labels.has(label)) return `Duplicate service label value: ${label}`;
    labels.add(label);
    if (!validServiceIds.includes(row.serviceId)) return "Select a service for every mapping row";
  }
  if (form.fallbackServiceId && !validServiceIds.includes(form.fallbackServiceId)) return "Invalid fallback service";
  const severityLabels = new Set<string>();
  for (const row of form.severityRows) {
    const label = row.labelValue.trim();
    if (!label) return "Severity label value is required";
    if (severityLabels.has(label)) return `Duplicate severity label value: ${label}`;
    severityLabels.add(label);
    if (!INTAKE_PRIORITIES.includes(row.priority)) return "Invalid priority";
  }
  if (!INTAKE_PRIORITIES.includes(form.defaultPriority)) return "Invalid priority";
  return null;
};
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

```bash
pnpm --filter=web exec vitest run core/store/intake-source.helpers.test.ts
```

Expected: PASS (3 test).

- [ ] **Step 5: Implementasi service + store + hook**

`apps/web/core/services/intake-source.service.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type { TIntakeSource, TIntakeSourcePayload } from "@plane/types";
import { APIService } from "@/services/api.service";

export class IntakeSourceService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  private base(workspaceSlug: string, projectId: string): string {
    return `/api/workspaces/${workspaceSlug}/projects/${projectId}/intake-sources`;
  }

  async list(workspaceSlug: string, projectId: string): Promise<TIntakeSource[]> {
    return this.get(`${this.base(workspaceSlug, projectId)}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async create(workspaceSlug: string, projectId: string, data: TIntakeSourcePayload): Promise<TIntakeSource> {
    return this.post(`${this.base(workspaceSlug, projectId)}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async update(
    workspaceSlug: string,
    projectId: string,
    sourceId: string,
    data: Partial<TIntakeSourcePayload> & { is_active?: boolean }
  ): Promise<TIntakeSource> {
    return this.patch(`${this.base(workspaceSlug, projectId)}/${sourceId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async destroy(workspaceSlug: string, projectId: string, sourceId: string): Promise<void> {
    return this.delete(`${this.base(workspaceSlug, projectId)}/${sourceId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async rotate(workspaceSlug: string, projectId: string, sourceId: string): Promise<{ id: string; token: string }> {
    return this.post(`${this.base(workspaceSlug, projectId)}/${sourceId}/rotate/`, {})
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
}
```

`apps/web/core/store/intake-source.store.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set } from "lodash-es";
import { action, observable, makeObservable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
import type { TIntakeSource, TIntakeSourcePayload } from "@plane/types";
import { IntakeSourceService } from "@/services/intake-source.service";
import type { CoreRootStore } from "./root.store";

export interface IIntakeSourceStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  sourceMap: Record<string, TIntakeSource>;
  getSourcesByProject: (projectId: string) => TIntakeSource[] | null;
  fetchSources: (workspaceSlug: string, projectId: string) => Promise<void>;
  createSource: (workspaceSlug: string, projectId: string, data: TIntakeSourcePayload) => Promise<TIntakeSource>;
  updateSource: (
    workspaceSlug: string,
    projectId: string,
    sourceId: string,
    data: Partial<TIntakeSourcePayload> & { is_active?: boolean }
  ) => Promise<TIntakeSource>;
  deleteSource: (workspaceSlug: string, projectId: string, sourceId: string) => Promise<void>;
  rotateSource: (workspaceSlug: string, projectId: string, sourceId: string) => Promise<string>;
}

export class IntakeSourceStore implements IIntakeSourceStore {
  loader: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  sourceMap: Record<string, TIntakeSource> = {};
  rootStore;
  intakeSourceService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedMap: observable,
      sourceMap: observable,
      fetchSources: action,
      createSource: action,
      updateSource: action,
      deleteSource: action,
      rotateSource: action,
    });
    this.rootStore = _rootStore;
    this.intakeSourceService = new IntakeSourceService();
  }

  getSourcesByProject = computedFn((projectId: string) => {
    if (!this.fetchedMap[projectId]) return null;
    return Object.values(this.sourceMap).filter((source) => source.project_id === projectId);
  });

  fetchSources = async (workspaceSlug: string, projectId: string) => {
    this.loader = true;
    try {
      const sources = await this.intakeSourceService.list(workspaceSlug, projectId);
      runInAction(() => {
        sources.forEach((source) => set(this.sourceMap, [source.id], source));
        set(this.fetchedMap, [projectId], true);
      });
    } finally {
      runInAction(() => {
        this.loader = false;
      });
    }
  };

  createSource = async (workspaceSlug: string, projectId: string, data: TIntakeSourcePayload) => {
    const source = await this.intakeSourceService.create(workspaceSlug, projectId, data);
    runInAction(() => set(this.sourceMap, [source.id], source));
    return source;
  };

  updateSource = async (
    workspaceSlug: string,
    projectId: string,
    sourceId: string,
    data: Partial<TIntakeSourcePayload> & { is_active?: boolean }
  ) => {
    const source = await this.intakeSourceService.update(workspaceSlug, projectId, sourceId, data);
    runInAction(() => set(this.sourceMap, [source.id], source));
    return source;
  };

  deleteSource = async (workspaceSlug: string, projectId: string, sourceId: string) => {
    await this.intakeSourceService.destroy(workspaceSlug, projectId, sourceId);
    runInAction(() => {
      delete this.sourceMap[sourceId];
    });
  };

  rotateSource = async (workspaceSlug: string, projectId: string, sourceId: string) => {
    const rotated = await this.intakeSourceService.rotate(workspaceSlug, projectId, sourceId);
    runInAction(() => set(this.sourceMap, [sourceId, "token"], rotated.token));
    return rotated.token;
  };
}
```

`apps/web/core/hooks/store/use-intake-source.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useContext } from "react";
import { StoreContext } from "@/lib/store-context";
import type { IIntakeSourceStore } from "@/store/intake-source.store";

export const useIntakeSource = (): IIntakeSourceStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useIntakeSource must be used within StoreProvider");
  return context.intakeSource;
};
```

- [ ] **Step 6: Registrasi root store + hooks index**

`apps/web/core/store/root.store.ts`:

- Import: `import type { IIntakeSourceStore } from "./intake-source.store";` dan `import { IntakeSourceStore } from "./intake-source.store";`
- Interface `CoreRootStore`: tambah `intakeSource: IIntakeSourceStore;` (setelah `workItemType`).
- Kedua constructor: tambah `this.intakeSource = new IntakeSourceStore(this);` (setelah `this.workItemType = new WorkItemTypeStore(this);`).

`apps/web/core/hooks/store/index.ts`: tambah `export * from "./use-intake-source";` (jaga urutan alfabetis).

- [ ] **Step 7: Cek tipe + test**

```bash
pnpm --filter=web check:types && pnpm --filter=web exec vitest run core/store/intake-source.helpers.test.ts
```

Expected: exit 0, 3 test PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/services/intake-source.service.ts apps/web/core/store/intake-source.store.ts apps/web/core/store/intake-source.helpers.ts apps/web/core/store/intake-source.helpers.test.ts apps/web/core/hooks/store/use-intake-source.ts apps/web/core/store/root.store.ts apps/web/core/hooks/store/index.ts
git commit -m "feat(web): add intake source service and store"
```

---

### Task 12: Halaman settings Intake sources

**Files:**

- Modify: `packages/types/src/settings.ts`
- Modify: `packages/constants/src/settings/project.ts`
- Modify: `apps/web/core/components/settings/project/sidebar/item-icon.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/intake-sources/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/intake-sources/header.tsx`

- [ ] **Step 1: Tambah tab**

`packages/types/src/settings.ts` — tambah `| "intake_sources"` di union `TProjectSettingsTabs` (setelah `work_item_types`).

`packages/constants/src/settings/project.ts` — tambah entry:

```ts
  intake_sources: {
    key: "intake_sources",
    i18n_label: "project_settings.intake_sources.heading",
    href: `/intake-sources`,
    access: [EUserProjectRoles.ADMIN],
    highlight: (pathname: string, baseUrl: string) => pathname === `${baseUrl}/intake-sources/`,
  },
```

dan masukkan `PROJECT_SETTINGS["intake_sources"],` ke grup `WORK_STRUCTURE`.

`item-icon.tsx`: tambah `intake_sources: IntakeOutline,` (pakai ikon yang sama dengan `features_intake`).

- [ ] **Step 2: Buat page + header**

`intake-sources/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { NotAuthorizedView } from "@/components/auth-screens/not-authorized-view";
import { PageHead } from "@/components/core/page-title";
import { IntakeSourcesRoot } from "@/components/intake-sources";
import { SettingsContentWrapper } from "@/components/settings/content-wrapper";
import { SettingsHeading } from "@/components/settings/heading";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import type { Route } from "./+types/page";
import { IntakeSourcesSettingsHeader } from "./header";

function IntakeSourcesSettingsPage({ params }: Route.ComponentProps) {
  const { workspaceSlug, projectId } = params;
  const { currentProjectDetails } = useProject();
  const { workspaceUserInfo, allowPermissions } = useUserPermissions();
  const { t } = useTranslation();
  const pageTitle = currentProjectDetails?.name
    ? `${currentProjectDetails?.name} - ${t("project_settings.intake_sources.title")}`
    : undefined;
  const canPerformProjectAdminActions = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.PROJECT);

  if (workspaceUserInfo && !canPerformProjectAdminActions) {
    return <NotAuthorizedView section="settings" isProjectView className="h-auto" />;
  }

  return (
    <SettingsContentWrapper header={<IntakeSourcesSettingsHeader />}>
      <PageHead title={pageTitle} />
      <div className="w-full">
        <SettingsHeading
          title={t("project_settings.intake_sources.heading")}
          description={t("project_settings.intake_sources.description")}
        />
        <IntakeSourcesRoot workspaceSlug={workspaceSlug} projectId={projectId} />
      </div>
    </SettingsContentWrapper>
  );
}

export default observer(IntakeSourcesSettingsPage);
```

`intake-sources/header.tsx` — salin `work-item-types/header.tsx`, ganti nama komponen menjadi `IntakeSourcesSettingsHeader`, `settingsDetails = PROJECT_SETTINGS.intake_sources`, dan `Icon = PROJECT_SETTINGS_ICONS.intake_sources`.

- [ ] **Step 3: Cek tipe**

```bash
pnpm --filter=web check:types
```

Expected: error hanya untuk `@/components/intake-sources` yang belum dibuat (Task 13).

- [ ] **Step 4: Commit**

```bash
git add packages/types/src/settings.ts packages/constants/src/settings/project.ts apps/web/core/components/settings/project/sidebar/item-icon.tsx "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/intake-sources"
git commit -m "feat(web): add intake sources settings route"
```

---

### Task 13: Komponen list + form modal + i18n

**Files:**

- Create: `apps/web/core/components/intake-sources/index.ts`
- Create: `apps/web/core/components/intake-sources/root.tsx`
- Create: `apps/web/core/components/intake-sources/form-modal.tsx`
- Modify: `packages/i18n/src/locales/en/project-settings.json`

- [ ] **Step 1: Buat komponen**

`index.ts`:

```ts
export * from "./root";
```

`root.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useEffect, useState } from "react";
import { API_BASE_URL } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TIntakeSource } from "@plane/types";
import { Button } from "@plane/ui";
import { useIntakeSource } from "@/hooks/store/use-intake-source";
import { buildWebhookUrl } from "@/store/intake-source.helpers";
import { IntakeSourceFormModal } from "./form-modal";

type Props = {
  workspaceSlug: string;
  projectId: string;
};

export const IntakeSourcesRoot = observer(function IntakeSourcesRoot(props: Props) {
  const { workspaceSlug, projectId } = props;
  const { t } = useTranslation();
  const { loader, getSourcesByProject, fetchSources, updateSource, deleteSource, rotateSource } = useIntakeSource();
  const sources = getSourcesByProject(projectId);
  // undefined = tertutup; null = modal create; source = modal edit
  const [formSource, setFormSource] = useState<TIntakeSource | null | undefined>(undefined);

  useEffect(() => {
    void fetchSources(workspaceSlug, projectId);
  }, [workspaceSlug, projectId, fetchSources]);

  const copyUrl = (token: string) => {
    void navigator.clipboard.writeText(buildWebhookUrl(API_BASE_URL, token));
  };

  const handleRotate = async (source: TIntakeSource) => {
    if (!window.confirm(t("project_settings.intake_sources.rotate_confirm"))) return;
    const token = await rotateSource(workspaceSlug, projectId, source.id);
    copyUrl(token);
  };

  const handleDelete = async (source: TIntakeSource) => {
    if (!window.confirm(t("project_settings.intake_sources.delete_confirm", { name: source.name }))) return;
    await deleteSource(workspaceSlug, projectId, source.id);
  };

  if (sources === null) {
    return <div className="mt-4 text-13 text-tertiary">{loader ? t("common.loading") : null}</div>;
  }

  return (
    <div className="mt-4 flex flex-col gap-3">
      <div className="flex justify-end">
        <Button variant="primary" size="sm" onClick={() => setFormSource(null)}>
          {t("project_settings.intake_sources.add_source")}
        </Button>
      </div>

      {sources.length === 0 ? (
        <div className="rounded-md border border-subtle p-6 text-center text-13 text-tertiary">
          {t("project_settings.intake_sources.empty")}
        </div>
      ) : (
        sources.map((source) => (
          <div
            key={source.id}
            className="flex flex-col gap-3 rounded-md border border-subtle p-4 sm:flex-row sm:items-center sm:justify-between"
          >
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <p className="truncate text-body-sm-medium">{source.name}</p>
                <span className="rounded-sm bg-layer-2 px-1.5 py-0.5 text-caption-sm-regular text-tertiary">
                  {source.is_active
                    ? t("project_settings.intake_sources.active")
                    : t("project_settings.intake_sources.inactive")}
                </span>
                {source.auto_accept && (
                  <span className="rounded-sm bg-layer-2 px-1.5 py-0.5 text-caption-sm-regular text-tertiary">
                    {t("project_settings.intake_sources.auto_accept")}
                  </span>
                )}
              </div>
              <p className="mt-1 truncate text-caption-md-regular text-tertiary">
                {buildWebhookUrl(API_BASE_URL, source.token)}
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="secondary" size="sm" onClick={() => copyUrl(source.token)}>
                {t("project_settings.intake_sources.copy_url")}
              </Button>
              <Button variant="secondary" size="sm" onClick={() => void handleRotate(source)}>
                {t("project_settings.intake_sources.rotate")}
              </Button>
              <Button variant="secondary" size="sm" onClick={() => setFormSource(source)}>
                {t("common.edit")}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void updateSource(workspaceSlug, projectId, source.id, { is_active: !source.is_active })}
              >
                {source.is_active
                  ? t("project_settings.intake_sources.deactivate")
                  : t("project_settings.intake_sources.activate")}
              </Button>
              <Button variant="danger" size="sm" onClick={() => void handleDelete(source)}>
                {t("common.delete")}
              </Button>
            </div>
          </div>
        ))
      )}

      {formSource !== undefined && (
        <IntakeSourceFormModal
          workspaceSlug={workspaceSlug}
          projectId={projectId}
          source={formSource}
          isOpen
          onClose={() => setFormSource(undefined)}
        />
      )}
    </div>
  );
});
```

`form-modal.tsx` — form lengkap dengan mapping editor:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useState } from "react";
import { useTranslation } from "@plane/i18n";
import type { TIntakeSource } from "@plane/types";
import { Button, Input, ModalCore } from "@plane/ui";
import { useIntakeSource } from "@/hooks/store/use-intake-source";
import { useService } from "@/hooks/store/use-service";
import { useWorkItemType } from "@/hooks/store/use-work-item-type";
import {
  configFromForm,
  INTAKE_PRIORITIES,
  type TIntakeSourceForm,
  serviceRowsFromConfig,
  severityRowsFromConfig,
  validateIntakeSourceForm,
} from "@/store/intake-source.helpers";

type Props = {
  workspaceSlug: string;
  projectId: string;
  source: TIntakeSource | null;
  isOpen: boolean;
  onClose: () => void;
};

export const IntakeSourceFormModal = observer(function IntakeSourceFormModal(props: Props) {
  const { workspaceSlug, projectId, source, isOpen, onClose } = props;
  const { t } = useTranslation();
  const { createSource, updateSource } = useIntakeSource();
  const { getProjectServiceIds, getServiceById } = useService();
  const { workItemTypes } = useWorkItemType();
  const projectTypes = (workItemTypes ?? []).filter(
    (type) => !type.is_epic && type.is_active && type.project_ids.includes(projectId)
  );
  const serviceIds = getProjectServiceIds(projectId) ?? [];

  const [form, setForm] = useState<TIntakeSourceForm>({
    name: source?.name ?? "",
    typeId: source?.type_id ?? "",
    autoAccept: source?.auto_accept ?? false,
    serviceLabelKey: source?.config?.service_label_key ?? "service",
    serviceRows: serviceRowsFromConfig(source?.config),
    fallbackServiceId: source?.config?.fallback_service_id ?? null,
    severityLabelKey: source?.config?.severity_label_key ?? "severity",
    severityRows:
      severityRowsFromConfig(source?.config).length > 0
        ? severityRowsFromConfig(source?.config)
        : [
            { labelValue: "critical", priority: "urgent" },
            { labelValue: "warning", priority: "high" },
            { labelValue: "info", priority: "low" },
          ],
    defaultPriority: source?.config?.default_priority ?? "none",
  });
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const handleSubmit = async () => {
    const validation = validateIntakeSourceForm(form, serviceIds);
    if (validation) {
      setError(validation);
      return;
    }
    setSaving(true);
    try {
      const payload = {
        name: form.name.trim(),
        type_id: form.typeId,
        auto_accept: form.autoAccept,
        config: configFromForm(form),
      };
      if (source) await updateSource(workspaceSlug, projectId, source.id, payload);
      else await createSource(workspaceSlug, projectId, payload);
      onClose();
    } catch {
      setError(t("project_settings.intake_sources.save_error"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} width="xl">
      <div className="flex max-h-[85vh] flex-col gap-4 overflow-y-auto p-5">
        <h3 className="text-h4-medium">
          {source ? t("project_settings.intake_sources.edit_source") : t("project_settings.intake_sources.add_source")}
        </h3>
        {error && <p className="text-13 text-danger-primary">{error}</p>}

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.name")}</span>
            <Input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} />
          </label>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.type")}</span>
            <select
              className="rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
              value={form.typeId}
              onChange={(e) => setForm({ ...form, typeId: e.target.value })}
            >
              <option value="">{t("project_settings.intake_sources.form.select_type")}</option>
              {projectTypes.map((type) => (
                <option key={type.id} value={type.id}>
                  {type.name}
                </option>
              ))}
            </select>
          </label>
          <label className="flex items-center gap-2 text-13">
            <input
              type="checkbox"
              checked={form.autoAccept}
              onChange={(e) => setForm({ ...form, autoAccept: e.target.checked })}
            />
            <span>{t("project_settings.intake_sources.form.auto_accept")}</span>
          </label>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.default_priority")}</span>
            <select
              className="rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
              value={form.defaultPriority}
              onChange={(e) =>
                setForm({ ...form, defaultPriority: e.target.value as TIntakeSourceForm["defaultPriority"] })
              }
            >
              {INTAKE_PRIORITIES.map((priority) => (
                <option key={priority} value={priority}>
                  {priority}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="flex flex-col gap-2 rounded-md border border-subtle p-3">
          <p className="text-body-sm-medium">{t("project_settings.intake_sources.form.service_mapping")}</p>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.service_label_key")}</span>
            <Input
              value={form.serviceLabelKey}
              onChange={(e) => setForm({ ...form, serviceLabelKey: e.target.value })}
            />
          </label>
          {form.serviceRows.map((row, index) => (
            <div key={index} className="flex items-center gap-2">
              <Input
                placeholder={t("project_settings.intake_sources.form.label_value")}
                value={row.labelValue}
                onChange={(e) => {
                  const rows = [...form.serviceRows];
                  rows[index] = { ...row, labelValue: e.target.value };
                  setForm({ ...form, serviceRows: rows });
                }}
              />
              <select
                className="w-full rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
                value={row.serviceId}
                onChange={(e) => {
                  const rows = [...form.serviceRows];
                  rows[index] = { ...row, serviceId: e.target.value };
                  setForm({ ...form, serviceRows: rows });
                }}
              >
                <option value="">{t("project_settings.intake_sources.form.select_service")}</option>
                {serviceIds.map((serviceId) => (
                  <option key={serviceId} value={serviceId}>
                    {getServiceById(serviceId)?.name ?? serviceId}
                  </option>
                ))}
              </select>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setForm({ ...form, serviceRows: form.serviceRows.filter((_, i) => i !== index) })}
              >
                {t("common.remove")}
              </Button>
            </div>
          ))}
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setForm({ ...form, serviceRows: [...form.serviceRows, { labelValue: "", serviceId: "" }] })}
          >
            {t("project_settings.intake_sources.form.add_row")}
          </Button>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.fallback_service")}</span>
            <select
              className="rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
              value={form.fallbackServiceId ?? ""}
              onChange={(e) => setForm({ ...form, fallbackServiceId: e.target.value || null })}
            >
              <option value="">{t("project_settings.intake_sources.form.no_fallback")}</option>
              {serviceIds.map((serviceId) => (
                <option key={serviceId} value={serviceId}>
                  {getServiceById(serviceId)?.name ?? serviceId}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="flex flex-col gap-2 rounded-md border border-subtle p-3">
          <p className="text-body-sm-medium">{t("project_settings.intake_sources.form.severity_mapping")}</p>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.severity_label_key")}</span>
            <Input
              value={form.severityLabelKey}
              onChange={(e) => setForm({ ...form, severityLabelKey: e.target.value })}
            />
          </label>
          {form.severityRows.map((row, index) => (
            <div key={index} className="flex items-center gap-2">
              <Input
                placeholder={t("project_settings.intake_sources.form.label_value")}
                value={row.labelValue}
                onChange={(e) => {
                  const rows = [...form.severityRows];
                  rows[index] = { ...row, labelValue: e.target.value };
                  setForm({ ...form, severityRows: rows });
                }}
              />
              <select
                className="w-full rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
                value={row.priority}
                onChange={(e) => {
                  const rows = [...form.severityRows];
                  rows[index] = { ...row, priority: e.target.value as TIntakeSourceForm["defaultPriority"] };
                  setForm({ ...form, severityRows: rows });
                }}
              >
                {INTAKE_PRIORITIES.map((priority) => (
                  <option key={priority} value={priority}>
                    {priority}
                  </option>
                ))}
              </select>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setForm({ ...form, severityRows: form.severityRows.filter((_, i) => i !== index) })}
              >
                {t("common.remove")}
              </Button>
            </div>
          ))}
          <Button
            variant="secondary"
            size="sm"
            onClick={() =>
              setForm({ ...form, severityRows: [...form.severityRows, { labelValue: "", priority: "none" }] })
            }
          >
            {t("project_settings.intake_sources.form.add_row")}
          </Button>
        </div>

        <div className="flex justify-end gap-2">
          <Button variant="secondary" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" size="sm" loading={saving} onClick={() => void handleSubmit()}>
            {source ? t("common.save") : t("common.create")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
```

Catatan: nama prop/import (`Button size="sm"`, `Input`, `ModalCore`, `useWorkItemType().workItemTypes`, `useService().getProjectServiceIds/getServiceById`) mengikuti komponen existing; sesuaikan bila signature berbeda saat `check:types`.

- [ ] **Step 2: Tambah i18n**

`packages/i18n/src/locales/en/project-settings.json`, di dalam `project_settings` (setelah `work_item_types`):

```json
    "intake_sources": {
      "title": "Intake sources",
      "heading": "Intake sources",
      "description": "Connect monitoring tools to create intake items automatically from alerts.",
      "add_source": "Add source",
      "edit_source": "Edit source",
      "empty": "No intake sources yet. Add one and point your Alertmanager webhook at its URL.",
      "active": "Active",
      "inactive": "Inactive",
      "auto_accept": "Auto-accept",
      "copy_url": "Copy URL",
      "rotate": "Rotate token",
      "rotate_confirm": "Rotate this token? The old URL stops working immediately.",
      "delete_confirm": "Delete \"{name}\"? Incoming events will be rejected.",
      "activate": "Activate",
      "deactivate": "Deactivate",
      "save_error": "Could not save the intake source.",
      "form": {
        "name": "Name",
        "type": "Work item type",
        "select_type": "Select a type",
        "auto_accept": "Auto-accept once classified",
        "default_priority": "Default priority",
        "service_mapping": "Service mapping",
        "service_label_key": "Service label key",
        "severity_mapping": "Severity mapping",
        "severity_label_key": "Severity label key",
        "label_value": "Label value",
        "select_service": "Select a service",
        "fallback_service": "Fallback service",
        "no_fallback": "No fallback",
        "add_row": "Add row"
      }
    },
```

- [ ] **Step 3: Cek tipe + lint**

```bash
pnpm --filter=web check:types && pnpm check:lint
```

Expected: exit 0. Sesuaikan nama prop/komponen bila ada error.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/intake-sources packages/i18n/src/locales/en/project-settings.json
git commit -m "feat(web): intake sources settings UI"
```

---

### Task 14: Badge source di detail intake

**Files:**

- Modify: `apps/web/core/store/inbox/inbox-issue.store.ts`
- Modify: `apps/web/core/components/inbox/content/inbox-issue-header.tsx`
- Modify: `packages/i18n/src/locales/en/inbox.json`

- [ ] **Step 1: Store field**

`inbox-issue.store.ts`:

- Import `TInboxIntakeSource` dari `@plane/types`.
- Interface: tambah `intakeSource: TInboxIntakeSource | null;` (setelah `source`).
- Class: tambah `intakeSource: TInboxIntakeSource | null = null;`.
- Constructor (setelah `this.source = data?.source || undefined;`): `this.intakeSource = data?.intake_source ?? null;`.
- `makeObservable`: tambah `intakeSource: observable.ref,`.

- [ ] **Step 2: Badge di header**

Di `inbox-issue-header.tsx`, render badge setelah elemen status/identifier (dekat `currentInboxIssueId`):

```tsx
{
  inboxIssue?.intakeSource && (
    <Tooltip
      tooltipContent={t("inbox_issue.intake_source.tooltip", {
        count: inboxIssue.intakeSource.occurrence_count,
        date: inboxIssue.intakeSource.last_seen_at ?? "",
      })}
    >
      <span className="rounded-sm bg-layer-2 px-1.5 py-0.5 text-caption-sm-regular text-tertiary">
        {t("inbox_issue.intake_source.badge", { name: inboxIssue.intakeSource.name })}
      </span>
    </Tooltip>
  );
}
```

Pastikan `Tooltip` dan `useTranslation` sudah diimpor di file tersebut (tambahkan bila belum).

- [ ] **Step 3: i18n**

`packages/i18n/src/locales/en/inbox.json`, tambah di level `inbox_issue`:

```json
    "intake_source": {
      "badge": "via {name}",
      "tooltip": "{count} event(s), last at {date}"
    },
```

- [ ] **Step 4: Cek tipe**

```bash
pnpm --filter=web check:types
```

Expected: exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/inbox/inbox-issue.store.ts apps/web/core/components/inbox/content/inbox-issue-header.tsx packages/i18n/src/locales/en/inbox.json
git commit -m "feat(web): show webhook source badge on intake detail"
```

---

### Task 15: Dokumentasi + verifikasi penuh

**Files:**

- Modify: `docs/features/intake.md`

- [ ] **Step 1: Update docs**

Tambah bagian "Channel webhook (Alertmanager)" di `docs/features/intake.md` yang menjelaskan: endpoint `POST /api/inbound/alertmanager/:token/`, klasifikasi mapping, dedup fingerprint, auto-resolve/refire, auto-accept, halaman settings Intake sources, dan batas (non-goals). Tambah baris changelog tanggal 2026-10-04.

- [ ] **Step 2: Verifikasi backend**

```bash
cd apps/api-rs && cargo test -p api --test inbound_intake_test -- --test-threads=1
cd apps/api-rs && cargo test -p api --test intake_source_routes_test -- --test-threads=1
cd apps/api-rs && cargo test -p api --test intake_triage_test -- --test-threads=1
cd apps/api-rs && cargo check -p api
```

Expected: semua PASS; `cargo check` exit 0.

- [ ] **Step 3: Verifikasi frontend**

```bash
pnpm check:types
pnpm check:lint
pnpm --filter=web exec vitest run
```

Expected: exit 0; semua test PASS.

- [ ] **Step 4: Build + live**

```bash
pnpm --filter=web build
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Tunggu build Rust selesai (LTO, bisa 10+ menit; cek `tail -f /tmp/plane-api-build.log`). Setelah itu:

```bash
curl -s -o /dev/null -w "%{http_code}" http://localhost:8000/health
systemctl --user restart plane-live.service
curl -s -o /dev/null -w "%{http_code}" http://localhost:3100/live/health/
```

Expected: `200` keduanya. Bila tunnel dipakai untuk demo: rebuild + restart `plane-web-prod.service` (lihat AGENTS.md).

- [ ] **Step 5: E2E manual (curl)**

Ambil token dari halaman settings (atau psql), lalu:

```bash
curl -s -X POST http://localhost:8000/api/inbound/alertmanager/<TOKEN>/ \
  -H 'Content-Type: application/json' \
  -d '{"version":"4","status":"firing","commonLabels":{"severity":"critical"},"alerts":[{"status":"firing","labels":{"alertname":"HighErrorRate","service":"payment-api","severity":"critical"},"annotations":{"summary":"Error rate > 5%"},"fingerprint":"manual-1"}]}'
```

Expected: `202` + `{"created":1,...}`; item muncul di intake. Kirim `"status":"resolved"` dengan fingerprint sama → item pending menjadi declined. Kirim firing lagi → pending lagi.

- [ ] **Step 6: Commit**

```bash
git add docs/features/intake.md
git commit -m "docs(intake): document Alertmanager webhook channel"
```

---

## Catatan self-review

- **Cakupan spec:** model (Task 1), origin (Task 2), CRUD (Task 3), ingest+klasifikasi (Task 4), dedup (Task 5), resolve (Task 6), auto-accept (Task 7), payload/fingerprint/idempotensi (Task 8), detail source (Task 9), FE types (Task 10), service/store (Task 11), settings UI (Task 12–13), badge (Task 14), docs + verifikasi (Task 15). Non-goals tidak diimplementasi.
- **Deviasi kecil dari spec:** `accept_intake_issue` hidup di `intake.rs` sebagai helper baru yang dipakai jalur ingress; `patch_issue` tidak direfaktor agar tidak ada risiko regresi — keduanya memakai `accept_gate` + `resolve_issue_state` yang sama dan diuji terpisah.
- **Konsistensi nama:** `intake_sources`, `intake_source_id`, `intake_fingerprint`, `intake_occurrence_count`, `intake_last_seen_at`, `accept_intake_issue`, `find_series`, `fire_series`, `resolve_series`, `buildWebhookUrl`, `validateIntakeSourceForm`, `IntakeSourceStore`, `useIntakeSource` dipakai konsisten di seluruh task.
- **Placeholder:** tidak ada; semua step berisi perintah/kode nyata.
