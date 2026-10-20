# RCB/TCB Backend — Phase 2: Review Engine (requests, sessions, outcomes, notifications) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Prerequisite:** `docs/superpowers/plans/2026-10-02-rcb-tcb-backend-releases.md` (Phase 1) sudah dieksekusi — migration `0012_releases_review_control.sql` sudah berisi keenam tabel, `routes/release.rs` sudah ada dengan `gate_ws_member`, `gate_ws_admin`, `release_in_workspace`, dan `tests/release_review_test.rs` sudah berisi fixture `Scratch`/`create_body`/`release_id`/`insert_issue`.

**Goal:** Implementasi review engine generik TCB/RCB di atas skema Phase 1 — submit/queue/withdraw request, rapat (session) beserta agenda + outcome, peserta, complete/cancel, dan notifikasi in-app — sehingga kedua board berfungsi end-to-end via API.

**Architecture:** Satu modul baru `routes/review.rs` menangani requests + sessions + items + participants (generik via `board_type`), mengikuti pola `routes/war_room.rs` (nested resources, guard transisi status) dan `routes/release.rs` (workspace gate, JSON shape). Notifikasi ditulis langsung ke tabel `notifications` seperti `war_room.rs:1897-1934`. Semua route terdaftar workspace-scoped di `main.rs`; gate TCB memakai project membership, gate RCB memakai workspace membership.

**Tech Stack:** Rust (axum 0.7, sqlx runtime queries), PostgreSQL 15, uuid, chrono, serde_json. Test: `cargo test -p api --test release_review_test -- --test-threads=1`.

**Spec:** `docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md`

**Keputusan implementasi (konsisten dengan spec):**

- List requests/sessions **wajib** `board_type`; `tcb` wajib `project_id` (400 `project_id is required for TCB`).
- Outcome dicatat hanya saat session `scheduled`; status request diperbarui **saat outcome dicatat** (final → `decided`, `deferred` → `pending`).
- Complete: wajib ≥1 item; item tanpa outcome di-set `deferred`; request yang masih `scheduled` → `pending`.
- Cancel: hanya bila belum ada outcome final; request `scheduled` di agenda → `pending`; item dibiarkan sebagai riwayat.
- Withdraw saat `scheduled`: item di session berstatus `scheduled` di-soft-delete (keluar agenda).
- Notifikasi peserta dikirim **saat participant ditambahkan** (peserta dikelola setelah session dibuat), sender `in_app:review:session_scheduled`.
- Item dengan outcome final tidak bisa dihapus dari agenda; yang blank/`deferred` bisa.

**Conventions (verified 2026-10-02):**

- Gate workspace: `release::gate_ws_member`, `release::gate_ws_admin` (Phase 1). Gate project: `service::gate_member` (roles 20/15/5 + ws-admin fallback) via alias lokal `gate_project_member`; project admin = role `20` atau ws admin (`gate_project_admin`).
- `project::{deny, missing}`; `service::{bad_request, validate_enum}`; `workflow::validate_name`.
- Notifikasi: kolom `notifications (id, workspace_id, project_id, receiver_id, entity_name, entity_identifier, title, sender, data, message_html, created_at, updated_at, created_by_id, triggered_by_id)`; `project_id` nullable (`war_room.rs:1909-1931`).
- Peserta upsert pola war room: `ON CONFLICT (session_id, user_id) WHERE deleted_at IS NULL DO UPDATE` (`war_room.rs:1424-1425`).
- Test menambah ke `tests/release_review_test.rs` yang sudah ada; satu file test = satu unit kompilasi → **update baris import tiap task**, jangan referensi handler yang belum diimplementasikan.

---

### Task 1: Review requests — submit, queue, detail, withdraw

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/review.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Test: `apps/api-rs/crates/api/tests/release_review_test.rs`

- [ ] **Step 1: Update imports and write the failing tests**

In `apps/api-rs/crates/api/tests/release_review_test.rs`, change the import lines to:

```rust
use api::routes::release::{
    changes_create, changes_destroy, changes_list, create, destroy, detail, list, patch,
    CreateRelease, LinkChanges, ListParams, PatchRelease,
};
use api::routes::review::{
    list_requests, request_detail, submit_request, withdraw_request, RequestListParams,
    SubmitRequest,
};
```

Then append to the file:

```rust
fn submit_body(board: &str, change: Option<Uuid>, release: Option<Uuid>) -> SubmitRequest {
    SubmitRequest {
        board_type: Some(board.to_string()),
        change_issue_id: change,
        release_id: release,
        submission_note: Some("Bukti uji terlampir".to_string()),
    }
}

fn no_request_filters(board: &str, project_id: Option<Uuid>) -> RequestListParams {
    RequestListParams {
        board_type: board.to_string(),
        project_id,
        status: None,
        change_issue_id: None,
        release_id: None,
    }
}

#[tokio::test]
async fn submit_change_request_tcb_dedupes_and_gates() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool).await;

    let (status, Json(body)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("submit");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["board_type"], "tcb");
    assert_eq!(body["status"], "pending");
    assert_eq!(body["project_id"].as_str().unwrap(), scratch.project_id.to_string());
    assert_eq!(body["subject"]["kind"], "change");
    assert!(body["subject"]["issue"]["identifier"]
        .as_str()
        .unwrap()
        .ends_with("-1"));

    let (status, Json(body)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("duplicate submit");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "An active review request already exists for this subject"
    );

    let outsider = scratch.add_actor(&st.pool, Some(15)).await;
    let (status, _) = submit_request(
        State(st.clone()),
        AuthUser(outsider),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("outsider submit");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, Json(body)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(Uuid::new_v4()), None)),
    )
    .await
    .expect("missing issue");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid change_issue_id - object does not exist.");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn submit_release_request_rcb_and_list_filters() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis RCB")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);

    let (status, Json(body)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("rcb", None, Some(rid))),
    )
    .await
    .expect("submit rcb");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["board_type"], "rcb");
    assert!(body["project_id"].is_null());
    assert_eq!(body["subject"]["kind"], "release");
    assert_eq!(body["subject"]["release"]["sequence_id"], 1);

    let (_, Json(rcb)) = list_requests(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(no_request_filters("rcb", None)),
    )
    .await
    .expect("list rcb");
    assert_eq!(rcb.as_array().unwrap().len(), 1);

    let (status, Json(body)) = list_requests(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(no_request_filters("tcb", None)),
    )
    .await
    .expect("list tcb missing project");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "project_id is required for TCB");

    let (_, Json(tcb)) = list_requests(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(no_request_filters("tcb", Some(scratch.project_id))),
    )
    .await
    .expect("list tcb");
    assert_eq!(tcb.as_array().unwrap().len(), 0);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn request_detail_history_and_withdraw() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool).await;
    let (_, Json(request)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("submit");
    let request_id: Uuid = request["id"].as_str().unwrap().parse().unwrap();

    let (status, Json(body)) = request_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), request_id)),
    )
    .await
    .expect("detail");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["history"].as_array().unwrap().len(), 0);

    let member = scratch.add_actor(&st.pool, Some(15)).await;
    let (status, _) = withdraw_request(
        State(st.clone()),
        AuthUser(member),
        Path((scratch.slug.clone(), request_id)),
    )
    .await
    .expect("member withdraw");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, Json(body)) = withdraw_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), request_id)),
    )
    .await
    .expect("withdraw");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "withdrawn");

    let (status, Json(body)) = withdraw_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), request_id)),
    )
    .await
    .expect("withdraw again");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Only pending or scheduled requests can be withdrawn"
    );

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd apps/api-rs && cargo test -p api --test release_review_test --no-run
```

Expected: FAIL to compile — `api::routes::review` not found.

- [ ] **Step 3: Create `routes/review.rs`**

Create `apps/api-rs/crates/api/src/routes/review.rs`:

```rust
//! Review control boards (TCB/RCB): generic review requests + rapat (sessions),
//! agenda outcomes, participants, in-app notifications.
//! Spec: docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing},
    state::AppState,
};

use super::issue_common::{fetch_project_member_role, is_workspace_admin};
use super::release::{gate_ws_admin, gate_ws_member, release_in_workspace};
use super::service::{bad_request, validate_enum};
use super::workflow::validate_name;

pub const BOARD_TYPES: &[&str] = &["tcb", "rcb"];
pub const REQUEST_STATUSES: &[&str] = &["pending", "scheduled", "decided", "withdrawn"];
pub const SESSION_STATUSES: &[&str] = &["scheduled", "completed", "cancelled"];
pub const OUTCOMES: &[&str] = &["approved", "rejected", "approved_with_notes", "deferred"];
pub const FINAL_OUTCOMES: &[&str] = &["approved", "rejected", "approved_with_notes"];
pub const PARTICIPANT_ROLES: &[&str] = &["chair", "secretary", "member"];
pub const ATTENDANCE_VALUES: &[&str] = &["invited", "present", "absent"];

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

pub(crate) async fn gate_project_member(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    super::service::gate_member(pool, user, slug, project_id).await
}

pub(crate) async fn gate_project_admin(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(matches!(role, Some(20)) || ws_admin)
}

/// Description for notification titles, e.g. `PROJ-12 Fix login` / `REL-3 Rilis 2026.10`.
pub(crate) fn subject_label(row: &ReviewRequestRow) -> String {
    if row.board_type == "tcb" {
        match (row.issue_identifier.as_deref(), row.issue_name.as_deref()) {
            (Some(identifier), Some(name)) => format!("{identifier} {name}"),
            _ => "Change request".to_string(),
        }
    } else {
        match (row.release_sequence_id, row.release_name.as_deref()) {
            (Some(sequence), Some(name)) => format!("REL-{sequence} {name}"),
            _ => "Release".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Review requests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReviewRequestRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub board_type: String,
    pub status: String,
    pub change_issue_id: Option<Uuid>,
    pub release_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub submission_note: String,
    pub submitted_by_id: Option<Uuid>,
    pub submitted_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub issue_identifier: Option<String>,
    pub issue_name: Option<String>,
    pub issue_project_id: Option<Uuid>,
    pub issue_project_identifier: Option<String>,
    pub release_sequence_id: Option<i64>,
    pub release_name: Option<String>,
    pub release_version: Option<String>,
    pub release_status: Option<String>,
    pub session_id: Option<Uuid>,
    pub session_title: Option<String>,
    pub session_scheduled_at: Option<DateTime<Utc>>,
}

pub(crate) const REQUEST_SELECT: &str = "SELECT rr.id, rr.workspace_id, rr.board_type, rr.status, \
    rr.change_issue_id, rr.release_id, rr.project_id, rr.submission_note, rr.submitted_by_id, \
    rr.submitted_at, rr.created_at, rr.updated_at, \
    (p.identifier || '-' || i.sequence_id::text) AS issue_identifier, i.name AS issue_name, \
    i.project_id AS issue_project_id, p.identifier AS issue_project_identifier, \
    rel.sequence_id AS release_sequence_id, rel.name AS release_name, rel.version AS release_version, \
    rel.status AS release_status, \
    sess.id AS session_id, sess.title AS session_title, sess.scheduled_at AS session_scheduled_at \
    FROM review_requests rr \
    LEFT JOIN issues i ON i.id = rr.change_issue_id \
    LEFT JOIN projects p ON p.id = i.project_id \
    LEFT JOIN releases rel ON rel.id = rr.release_id \
    LEFT JOIN LATERAL ( \
        SELECT s.id, s.title, s.scheduled_at FROM review_session_items rsi \
        JOIN review_sessions s ON s.id = rsi.session_id \
        WHERE rsi.review_request_id = rr.id AND rsi.deleted_at IS NULL \
          AND s.status = 'scheduled' AND s.deleted_at IS NULL \
        ORDER BY s.scheduled_at ASC LIMIT 1 \
    ) sess ON true";

pub(crate) fn request_json(row: &ReviewRequestRow) -> Value {
    let subject = if row.board_type == "tcb" {
        json!({
            "kind": "change",
            "id": row.change_issue_id,
            "issue": {
                "id": row.change_issue_id,
                "identifier": row.issue_identifier,
                "name": row.issue_name,
                "project_id": row.issue_project_id,
                "project_identifier": row.issue_project_identifier,
            },
            "release": Value::Null,
        })
    } else {
        json!({
            "kind": "release",
            "id": row.release_id,
            "issue": Value::Null,
            "release": {
                "id": row.release_id,
                "sequence_id": row.release_sequence_id,
                "name": row.release_name,
                "version": row.release_version,
                "status": row.release_status,
            },
        })
    };
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "board_type": row.board_type,
        "status": row.status,
        "change_issue_id": row.change_issue_id,
        "release_id": row.release_id,
        "project_id": row.project_id,
        "submission_note": row.submission_note,
        "submitted_by": row.submitted_by_id,
        "submitted_at": row.submitted_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "subject": subject,
        "session": row.session_id.map(|id| json!({
            "id": id,
            "title": row.session_title,
            "scheduled_at": row.session_scheduled_at,
        })),
    })
}

pub(crate) async fn request_in_workspace(
    pool: &sqlx::PgPool,
    slug: &str,
    request_id: Uuid,
) -> Result<Option<ReviewRequestRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{REQUEST_SELECT} WHERE rr.id = $1 \
         AND rr.workspace_id = (SELECT id FROM workspaces WHERE slug = $2 AND deleted_at IS NULL) \
         AND rr.deleted_at IS NULL"
    ))
    .bind(request_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

pub(crate) async fn gate_request_read(
    st: &AppState,
    user: Uuid,
    slug: &str,
    row: &ReviewRequestRow,
) -> Result<bool, sqlx::Error> {
    if row.board_type == "tcb" {
        match row.project_id {
            Some(project_id) => gate_project_member(&st.pool, user, slug, project_id).await,
            None => Ok(false),
        }
    } else {
        gate_ws_member(&st.pool, user, slug).await
    }
}

#[derive(Debug, Deserialize)]
pub struct RequestListParams {
    pub board_type: String,
    pub project_id: Option<Uuid>,
    pub status: Option<String>,
    pub change_issue_id: Option<Uuid>,
    pub release_id: Option<Uuid>,
}

/// GET `/api/workspaces/:slug/review-requests/`
pub async fn list_requests(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(params): Query<RequestListParams>,
) -> R {
    if let Err(e) = validate_enum("board_type", &params.board_type, BOARD_TYPES) {
        return Ok(bad_request(e));
    }
    if let Some(status) = params.status.as_deref() {
        if let Err(e) = validate_enum("status", status, REQUEST_STATUSES) {
            return Ok(bad_request(e));
        }
    }
    if params.board_type == "tcb" {
        let Some(project_id) = params.project_id else {
            return Ok(bad_request("project_id is required for TCB"));
        };
        if !gate_project_member(&st.pool, auth.0, &slug, project_id).await? {
            return Ok(deny());
        }
    } else if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let rows: Vec<ReviewRequestRow> = sqlx::query_as(&format!(
        "{REQUEST_SELECT} WHERE rr.workspace_id = (SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL) \
         AND rr.board_type = $2 AND rr.deleted_at IS NULL \
         AND ($3::uuid IS NULL OR rr.project_id = $3) \
         AND ($4::text IS NULL OR rr.status = $4) \
         AND ($5::uuid IS NULL OR rr.change_issue_id = $5) \
         AND ($6::uuid IS NULL OR rr.release_id = $6) \
         ORDER BY rr.submitted_at DESC"
    ))
    .bind(&slug)
    .bind(&params.board_type)
    .bind(params.project_id)
    .bind(&params.status)
    .bind(params.change_issue_id)
    .bind(params.release_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(request_json).collect())),
    ))
}

#[derive(Debug, Deserialize)]
pub struct SubmitRequest {
    pub board_type: Option<String>,
    pub change_issue_id: Option<Uuid>,
    pub release_id: Option<Uuid>,
    pub submission_note: Option<String>,
}

/// POST `/api/workspaces/:slug/review-requests/`
pub async fn submit_request(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<SubmitRequest>,
) -> R {
    let board = body.board_type.clone().unwrap_or_default();
    if let Err(e) = validate_enum("board_type", &board, BOARD_TYPES) {
        return Ok(bad_request(e));
    }
    let workspace_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL",
    )
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(workspace_id) = workspace_id else {
        return Ok(missing());
    };
    let note = body.submission_note.clone().unwrap_or_default().trim().to_string();

    let (project_id, change_issue_id, release_id) = if board == "tcb" {
        let Some(issue_id) = body.change_issue_id else {
            return Ok(bad_request("change_issue_id is required"));
        };
        let issue: Option<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT i.project_id, p.workspace_id FROM issues i JOIN projects p ON p.id = i.project_id \
             WHERE i.id = $1 AND i.deleted_at IS NULL AND p.deleted_at IS NULL",
        )
        .bind(issue_id)
        .fetch_optional(&st.pool)
        .await?;
        let Some((issue_project_id, issue_workspace_id)) = issue else {
            return Ok(bad_request("Invalid change_issue_id - object does not exist."));
        };
        if issue_workspace_id != workspace_id {
            return Ok(bad_request("Invalid change_issue_id - object does not exist."));
        }
        if !gate_project_member(&st.pool, auth.0, &slug, issue_project_id).await? {
            return Ok(deny());
        }
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM review_requests WHERE board_type = 'tcb' \
             AND change_issue_id = $1 AND status IN ('pending', 'scheduled') AND deleted_at IS NULL)",
        )
        .bind(issue_id)
        .fetch_one(&st.pool)
        .await?;
        if active {
            return Ok(bad_request(
                "An active review request already exists for this subject",
            ));
        }
        (Some(issue_project_id), Some(issue_id), None)
    } else {
        let Some(release_id) = body.release_id else {
            return Ok(bad_request("release_id is required"));
        };
        if release_in_workspace(&st.pool, &slug, release_id).await?.is_none() {
            return Ok(bad_request("Invalid release_id - object does not exist."));
        }
        if !gate_ws_member(&st.pool, auth.0, &slug).await? {
            return Ok(deny());
        }
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM review_requests WHERE board_type = 'rcb' \
             AND release_id = $1 AND status IN ('pending', 'scheduled') AND deleted_at IS NULL)",
        )
        .bind(release_id)
        .fetch_one(&st.pool)
        .await?;
        if active {
            return Ok(bad_request(
                "An active review request already exists for this subject",
            ));
        }
        (None, None, Some(release_id))
    };

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO review_requests (id, workspace_id, board_type, change_issue_id, release_id, \
         project_id, status, submission_note, submitted_by_id, submitted_at, created_at, updated_at, \
         created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8, now(), now(), now(), $8, $8)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(&board)
    .bind(change_issue_id)
    .bind(release_id)
    .bind(project_id)
    .bind(&note)
    .bind(auth.0)
    .execute(&st.pool)
    .await?;
    let row = request_in_workspace(&st.pool, &slug, id)
        .await?
        .expect("request just inserted");
    Ok((StatusCode::CREATED, Json(request_json(&row))))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RequestHistoryRow {
    pub session_id: Uuid,
    pub title: String,
    pub scheduled_at: DateTime<Utc>,
    pub session_status: String,
    pub outcome: Option<String>,
    pub outcome_note: String,
    pub decided_at: Option<DateTime<Utc>>,
}

/// GET `/api/workspaces/:slug/review-requests/:request_id/`
pub async fn request_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, request_id)): Path<(String, Uuid)>,
) -> R {
    let Some(row) = request_in_workspace(&st.pool, &slug, request_id).await? else {
        return Ok(missing());
    };
    if !gate_request_read(&st, auth.0, &slug, &row).await? {
        return Ok(deny());
    }
    let history: Vec<RequestHistoryRow> = sqlx::query_as(
        "SELECT s.id AS session_id, s.title, s.scheduled_at, s.status AS session_status, \
         i.outcome, i.outcome_note, i.decided_at \
         FROM review_session_items i JOIN review_sessions s ON s.id = i.session_id \
         WHERE i.review_request_id = $1 AND i.deleted_at IS NULL \
         ORDER BY s.scheduled_at ASC",
    )
    .bind(request_id)
    .fetch_all(&st.pool)
    .await?;
    let mut value = request_json(&row);
    value["history"] = json!(history
        .iter()
        .map(|h| json!({
            "session_id": h.session_id,
            "title": h.title,
            "scheduled_at": h.scheduled_at,
            "session_status": h.session_status,
            "outcome": h.outcome,
            "outcome_note": h.outcome_note,
            "decided_at": h.decided_at,
        }))
        .collect::<Vec<_>>());
    Ok((StatusCode::OK, Json(value)))
}

/// POST `/api/workspaces/:slug/review-requests/:request_id/withdraw/`
pub async fn withdraw_request(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, request_id)): Path<(String, Uuid)>,
) -> R {
    let Some(row) = request_in_workspace(&st.pool, &slug, request_id).await? else {
        return Ok(missing());
    };
    if !gate_request_read(&st, auth.0, &slug, &row).await? {
        return Ok(deny());
    }
    let is_submitter = row.submitted_by_id == Some(auth.0);
    let is_admin = if row.board_type == "tcb" {
        match row.project_id {
            Some(project_id) => gate_project_admin(&st.pool, auth.0, &slug, project_id).await?,
            None => false,
        }
    } else {
        gate_ws_admin(&st.pool, auth.0, &slug).await?
    };
    if !is_submitter && !is_admin {
        return Ok(deny());
    }
    if row.status != "pending" && row.status != "scheduled" {
        return Ok(bad_request(
            "Only pending or scheduled requests can be withdrawn",
        ));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE review_session_items i SET deleted_at = now(), updated_at = now() \
         FROM review_sessions s \
         WHERE i.session_id = s.id AND i.review_request_id = $1 \
         AND s.status = 'scheduled' AND i.deleted_at IS NULL",
    )
    .bind(request_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE review_requests SET status = 'withdrawn', updated_at = now(), updated_by_id = $1 \
         WHERE id = $2 AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(request_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row = request_in_workspace(&st.pool, &slug, request_id)
        .await?
        .expect("request just withdrawn");
    Ok((StatusCode::OK, Json(request_json(&row))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_and_outcome_enums() {
        assert!(validate_enum("board_type", "tcb", BOARD_TYPES).is_ok());
        assert!(validate_enum("board_type", "cab", BOARD_TYPES).is_err());
        assert!(validate_enum("outcome", "approved_with_notes", OUTCOMES).is_ok());
        assert!(validate_enum("outcome", "passed", OUTCOMES).is_err());
        assert!(FINAL_OUTCOMES.contains(&"approved"));
        assert!(!FINAL_OUTCOMES.contains(&"deferred"));
    }
}
```

- [ ] **Step 4: Register the module and request routes**

In `apps/api-rs/crates/api/src/routes/mod.rs`, add between `pub mod release;` and `pub mod s3proxy;`:

```rust
pub mod review;
```

In `apps/api-rs/crates/api/src/main.rs`, add after the releases block (Phase 1):

```rust
        // Review control boards (RCB/TCB) — requests. Session auth. TCB =
        // project member; RCB = workspace member. Spec:
        // docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md
        .route(
            "/api/workspaces/:slug/review-requests/",
            get(routes::review::list_requests).post(routes::review::submit_request),
        )
        .route(
            "/api/workspaces/:slug/review-requests/:request_id/",
            get(routes::review::request_detail),
        )
        .route(
            "/api/workspaces/:slug/review-requests/:request_id/withdraw/",
            post(routes::review::withdraw_request),
        )
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test submit_ -- --test-threads=1
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --lib review:: -- --test-threads=1
```

Expected: PASS — `submit_change_request_tcb_dedupes_and_gates`, `submit_release_request_rcb_and_list_filters`; unit `board_and_outcome_enums`. (`request_detail_history_and_withdraw` juga PASS — jalankan filter `request_detail` untuk memastikan.)

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/review.rs \
  apps/api-rs/crates/api/src/routes/mod.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/release_review_test.rs
git commit -m "feat(api-rs): review requests submit/queue/withdraw"
```

---

### Task 2: Sessions — create, list, detail, patch

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/review.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Test: `apps/api-rs/crates/api/tests/release_review_test.rs`

- [ ] **Step 1: Update imports and write the failing tests**

In `apps/api-rs/crates/api/tests/release_review_test.rs`, change the review import block to:

```rust
use api::routes::review::{
    create_session, list_requests, list_sessions, patch_session, request_detail, session_detail,
    submit_request, withdraw_request, CreateSession, PatchSession, RequestListParams,
    SessionListParams, SubmitRequest,
};
```

Then append to the file:

```rust
fn session_body(board: &str, project_id: Option<Uuid>, title: &str) -> CreateSession {
    CreateSession {
        board_type: Some(board.to_string()),
        project_id,
        title: Some(title.to_string()),
        scheduled_at: Some(chrono::DateTime::parse_from_rfc3339("2026-10-10T09:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)),
        location: Some("Ruang Rapat 3".to_string()),
        minutes: None,
    }
}

fn no_session_filters(board: &str, project_id: Option<Uuid>) -> SessionListParams {
    SessionListParams {
        board_type: board.to_string(),
        project_id,
        status: None,
    }
}

#[tokio::test]
async fn create_session_requires_board_scope_admin() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;

    let (status, Json(body)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Mingguan")),
    )
    .await
    .expect("create tcb session");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["board_type"], "tcb");
    assert_eq!(body["status"], "scheduled");
    assert_eq!(body["project_id"].as_str().unwrap(), scratch.project_id.to_string());

    let member = scratch.add_actor(&st.pool, Some(15)).await;
    let (status, _) = create_session(
        State(st.clone()),
        AuthUser(member),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Ilegal")),
    )
    .await
    .expect("member create");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, Json(body)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "   ")),
    )
    .await
    .expect("blank title");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "title is required");

    let (status, _) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("rcb", Some(scratch.project_id), "RCB Ilegal")),
    )
    .await
    .expect("rcb with project");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, Json(body)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("rcb", None, "RCB Bulanan")),
    )
    .await
    .expect("create rcb session");
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["project_id"].is_null());

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn session_list_filters_and_patch() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Mingguan")),
    )
    .await
    .expect("create tcb session");
    create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("rcb", None, "RCB Bulanan")),
    )
    .await
    .expect("create rcb session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();

    let (_, Json(tcb)) = list_sessions(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(no_session_filters("tcb", Some(scratch.project_id))),
    )
    .await
    .expect("list tcb");
    assert_eq!(tcb.as_array().unwrap().len(), 1);

    let (_, Json(rcb)) = list_sessions(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(no_session_filters("rcb", None)),
    )
    .await
    .expect("list rcb");
    assert_eq!(rcb.as_array().unwrap().len(), 1);

    let (status, Json(body)) = patch_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(PatchSession {
            title: Some("TCB Mingguan (revisi)".into()),
            scheduled_at: None,
            location: None,
            minutes: Some("Agenda: review 3 change.".into()),
        }),
    )
    .await
    .expect("patch session");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["title"], "TCB Mingguan (revisi)");
    assert_eq!(body["minutes"], "Agenda: review 3 change.");

    let (status, _) = patch_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), Uuid::new_v4())),
        Json(PatchSession::default()),
    )
    .await
    .expect("missing patch");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn session_detail_reads_scope() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;

    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Detail")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();

    let (status, Json(body)) = session_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("detail");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["title"], "TCB Detail");
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert_eq!(body["participants"].as_array().unwrap().len(), 0);

    let outsider = scratch.add_actor(&st.pool, Some(15)).await;
    let (status, _) = session_detail(
        State(st.clone()),
        AuthUser(outsider),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("outsider detail");
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd apps/api-rs && cargo test -p api --test release_review_test create_session --no-run
```

Expected: FAIL to compile — `create_session`, `PatchSession`, `SessionListParams` not found.

- [ ] **Step 3: Implement sessions**

Append to `apps/api-rs/crates/api/src/routes/review.rs` (sebelum `#[cfg(test)] mod tests`):

```rust
// ---------------------------------------------------------------------------
// Review sessions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReviewSessionRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub board_type: String,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub scheduled_at: DateTime<Utc>,
    pub status: String,
    pub minutes: String,
    pub location: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by_id: Option<Uuid>,
    pub item_count: i64,
    pub participant_count: i64,
    pub pending_outcome_count: i64,
}

pub(crate) const SESSION_SELECT: &str = "SELECT s.id, s.workspace_id, s.board_type, s.project_id, \
    s.title, s.scheduled_at, s.status, s.minutes, s.location, s.completed_at, s.cancelled_at, \
    s.created_at, s.updated_at, s.created_by_id, \
    (SELECT COUNT(*) FROM review_session_items i WHERE i.session_id = s.id AND i.deleted_at IS NULL) AS item_count, \
    (SELECT COUNT(*) FROM review_session_participants p WHERE p.session_id = s.id AND p.deleted_at IS NULL) AS participant_count, \
    (SELECT COUNT(*) FROM review_session_items i WHERE i.session_id = s.id AND i.deleted_at IS NULL AND i.outcome IS NULL) AS pending_outcome_count \
    FROM review_sessions s";

pub(crate) fn session_json(row: &ReviewSessionRow) -> Value {
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "board_type": row.board_type,
        "project_id": row.project_id,
        "title": row.title,
        "scheduled_at": row.scheduled_at,
        "status": row.status,
        "minutes": row.minutes,
        "location": row.location,
        "completed_at": row.completed_at,
        "cancelled_at": row.cancelled_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": row.created_by_id,
        "counts": {
            "items": row.item_count,
            "participants": row.participant_count,
            "pending_outcome": row.pending_outcome_count,
        },
    })
}

pub(crate) async fn session_in_workspace(
    pool: &sqlx::PgPool,
    slug: &str,
    session_id: Uuid,
) -> Result<Option<ReviewSessionRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{SESSION_SELECT} WHERE s.id = $1 \
         AND s.workspace_id = (SELECT id FROM workspaces WHERE slug = $2 AND deleted_at IS NULL) \
         AND s.deleted_at IS NULL"
    ))
    .bind(session_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

pub(crate) async fn gate_session_read(
    st: &AppState,
    user: Uuid,
    slug: &str,
    session: &ReviewSessionRow,
) -> Result<bool, sqlx::Error> {
    if session.board_type == "tcb" {
        match session.project_id {
            Some(project_id) => gate_project_member(&st.pool, user, slug, project_id).await,
            None => Ok(false),
        }
    } else {
        gate_ws_member(&st.pool, user, slug).await
    }
}

pub(crate) async fn can_manage_session(
    st: &AppState,
    user: Uuid,
    slug: &str,
    session: &ReviewSessionRow,
) -> Result<bool, sqlx::Error> {
    if session.created_by_id == Some(user) {
        return Ok(true);
    }
    let admin = if session.board_type == "tcb" {
        match session.project_id {
            Some(project_id) => gate_project_admin(&st.pool, user, slug, project_id).await?,
            None => false,
        }
    } else {
        gate_ws_admin(&st.pool, user, slug).await?
    };
    if admin {
        return Ok(true);
    }
    let participant: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM review_session_participants WHERE session_id = $1 \
         AND user_id = $2 AND role IN ('chair', 'secretary') AND deleted_at IS NULL)",
    )
    .bind(session.id)
    .bind(user)
    .fetch_one(&st.pool)
    .await?;
    Ok(participant)
}

#[derive(Debug, Deserialize)]
pub struct CreateSession {
    pub board_type: Option<String>,
    pub project_id: Option<Uuid>,
    pub title: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub location: Option<String>,
    pub minutes: Option<String>,
}

/// POST `/api/workspaces/:slug/review-sessions/`
pub async fn create_session(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<CreateSession>,
) -> R {
    let board = body.board_type.clone().unwrap_or_default();
    if let Err(e) = validate_enum("board_type", &board, BOARD_TYPES) {
        return Ok(bad_request(e));
    }
    let title = match validate_name(body.title.as_deref().unwrap_or(""), "title") {
        Ok(title) => title,
        Err(e) => return Ok(bad_request(e)),
    };
    let Some(scheduled_at) = body.scheduled_at else {
        return Ok(bad_request("scheduled_at is required"));
    };
    let location = body
        .location
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let workspace_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL",
    )
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(workspace_id) = workspace_id else {
        return Ok(missing());
    };
    let project_id = if board == "tcb" {
        let Some(project_id) = body.project_id else {
            return Ok(bad_request("project_id is required for TCB"));
        };
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
             WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL AND w.deleted_at IS NULL)",
        )
        .bind(project_id)
        .bind(&slug)
        .fetch_one(&st.pool)
        .await?;
        if !exists {
            return Ok(bad_request("Invalid project_id - object does not exist."));
        }
        if !gate_project_admin(&st.pool, auth.0, &slug, project_id).await? {
            return Ok(deny());
        }
        Some(project_id)
    } else {
        if body.project_id.is_some() {
            return Ok(bad_request("project_id is not allowed for RCB"));
        }
        if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
            return Ok(deny());
        }
        None
    };
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO review_sessions (id, workspace_id, board_type, project_id, title, \
         scheduled_at, status, minutes, location, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, 'scheduled', $7, $8, now(), now(), $9, $9)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(&board)
    .bind(project_id)
    .bind(&title)
    .bind(scheduled_at)
    .bind(body.minutes.clone().unwrap_or_default())
    .bind(&location)
    .bind(auth.0)
    .execute(&st.pool)
    .await?;
    let row = session_in_workspace(&st.pool, &slug, id)
        .await?
        .expect("session just inserted");
    Ok((StatusCode::CREATED, Json(session_json(&row))))
}

#[derive(Debug, Deserialize)]
pub struct SessionListParams {
    pub board_type: String,
    pub project_id: Option<Uuid>,
    pub status: Option<String>,
}

/// GET `/api/workspaces/:slug/review-sessions/`
pub async fn list_sessions(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(params): Query<SessionListParams>,
) -> R {
    if let Err(e) = validate_enum("board_type", &params.board_type, BOARD_TYPES) {
        return Ok(bad_request(e));
    }
    if let Some(status) = params.status.as_deref() {
        if let Err(e) = validate_enum("status", status, SESSION_STATUSES) {
            return Ok(bad_request(e));
        }
    }
    if params.board_type == "tcb" {
        let Some(project_id) = params.project_id else {
            return Ok(bad_request("project_id is required for TCB"));
        };
        if !gate_project_member(&st.pool, auth.0, &slug, project_id).await? {
            return Ok(deny());
        }
    } else if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let rows: Vec<ReviewSessionRow> = sqlx::query_as(&format!(
        "{SESSION_SELECT} WHERE s.workspace_id = (SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL) \
         AND s.board_type = $2 AND s.deleted_at IS NULL \
         AND ($3::uuid IS NULL OR s.project_id = $3) \
         AND ($4::text IS NULL OR s.status = $4) \
         ORDER BY s.scheduled_at DESC"
    ))
    .bind(&slug)
    .bind(&params.board_type)
    .bind(params.project_id)
    .bind(&params.status)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(session_json).collect())),
    ))
}

/// GET `/api/workspaces/:slug/review-sessions/:session_id/`
pub async fn session_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id)): Path<(String, Uuid)>,
) -> R {
    let Some(row) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &row).await? {
        return Ok(deny());
    }
    let mut value = session_json(&row);
    value["items"] = json!([]);
    value["participants"] = json!([]);
    Ok((StatusCode::OK, Json(value)))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchSession {
    pub title: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub location: Option<String>,
    pub minutes: Option<String>,
}

/// PATCH `/api/workspaces/:slug/review-sessions/:session_id/`
pub async fn patch_session(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id)): Path<(String, Uuid)>,
    Json(body): Json<PatchSession>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if session.status == "cancelled" {
        return Ok(bad_request("Session is cancelled"));
    }
    if session.status == "completed"
        && (body.title.is_some() || body.scheduled_at.is_some() || body.location.is_some())
    {
        return Ok(bad_request("Session is completed"));
    }
    let title = match body.title.as_deref() {
        Some(raw) => match validate_name(raw, "title") {
            Ok(title) => title,
            Err(e) => return Ok(bad_request(e)),
        },
        None => session.title.clone(),
    };
    let scheduled_at = body.scheduled_at.unwrap_or(session.scheduled_at);
    let location = match body.location.as_deref() {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => session.location.clone(),
    };
    let minutes = body.minutes.clone().unwrap_or_else(|| session.minutes.clone());
    sqlx::query(
        "UPDATE review_sessions SET title = $1, scheduled_at = $2, location = $3, minutes = $4, \
         updated_at = now(), updated_by_id = $5 WHERE id = $6 AND deleted_at IS NULL",
    )
    .bind(&title)
    .bind(scheduled_at)
    .bind(&location)
    .bind(&minutes)
    .bind(auth.0)
    .bind(session_id)
    .execute(&st.pool)
    .await?;
    let row = session_in_workspace(&st.pool, &slug, session_id)
        .await?
        .expect("session just updated");
    Ok((StatusCode::OK, Json(session_json(&row))))
}
```

- [ ] **Step 4: Register session routes**

In `apps/api-rs/crates/api/src/main.rs`, add after the review-requests routes (Task 1):

```rust
        .route(
            "/api/workspaces/:slug/review-sessions/",
            get(routes::review::list_sessions).post(routes::review::create_session),
        )
        .route(
            "/api/workspaces/:slug/review-sessions/:session_id/",
            get(routes::review::session_detail).patch(routes::review::patch_session),
        )
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test session_ -- --test-threads=1
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test create_session -- --test-threads=1
```

Expected: PASS — `create_session_requires_board_scope_admin`, `session_list_filters_and_patch`, `session_detail_reads_scope`.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/review.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/release_review_test.rs
git commit -m "feat(api-rs): review sessions create/list/detail/patch"
```

---

### Task 3: Participants + `session_scheduled` notification

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/review.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Test: `apps/api-rs/crates/api/tests/release_review_test.rs`

- [ ] **Step 1: Update imports and write the failing tests**

In `apps/api-rs/crates/api/tests/release_review_test.rs`, change the review import block to:

```rust
use api::routes::review::{
    create_session, list_requests, list_sessions, participants_create, participants_destroy,
    participants_list, participants_patch, patch_session, request_detail, session_detail,
    submit_request, withdraw_request, CreateSession, ParticipantCreate, ParticipantPatch,
    PatchSession, RequestListParams, SessionListParams, SubmitRequest,
};
```

Then append to the file:

```rust
fn participant_body(user_id: Uuid, role: Option<&str>) -> ParticipantCreate {
    ParticipantCreate {
        user_id,
        role: role.map(str::to_string),
        attendance: None,
    }
}

#[tokio::test]
async fn participants_add_list_patch_remove_and_notify() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15)).await;

    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Peserta")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();

    let (status, Json(body)) = participants_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(participant_body(member, Some("secretary"))),
    )
    .await
    .expect("add participant");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["role"], "secretary");
    assert_eq!(body["attendance"], "invited");

    let notified: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE receiver_id = $1 \
         AND sender = 'in_app:review:session_scheduled' AND deleted_at IS NULL",
    )
    .bind(member)
    .fetch_one(&st.pool)
    .await
    .expect("count notifications");
    assert_eq!(notified, 1);

    let (status, Json(body)) = participants_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(participant_body(member, Some("member"))),
    )
    .await
    .expect("upsert participant");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["role"], "member");

    let (_, Json(list)) = participants_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("list participants");
    assert_eq!(list.as_array().unwrap().len(), 1);

    let participant_id: Uuid = list[0]["id"].as_str().unwrap().parse().unwrap();
    let (status, Json(body)) = participants_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id, participant_id)),
        Json(ParticipantPatch {
            role: None,
            attendance: Some("present".into()),
        }),
    )
    .await
    .expect("patch participant");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["attendance"], "present");

    let (status, _) = participants_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id, participant_id)),
    )
    .await
    .expect("remove participant");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, Json(list)) = participants_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("list after remove");
    assert_eq!(list.as_array().unwrap().len(), 0);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn participant_requires_workspace_member() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Non-member")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();

    let (status, Json(body)) = participants_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(participant_body(Uuid::new_v4(), None)),
    )
    .await
    .expect("foreign participant");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid user_id - not a workspace member.");

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd apps/api-rs && cargo test -p api --test release_review_test participants_ --no-run
```

Expected: FAIL to compile — `participants_create`, `ParticipantCreate`, `ParticipantPatch` not found.

- [ ] **Step 3: Implement participants + notification helper**

Append to `apps/api-rs/crates/api/src/routes/review.rs` (sebelum `#[cfg(test)] mod tests`):

```rust
// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

/// One `notifications` row, shaped like the war-room helper
/// (`war_room.rs:1897-1934`). Skips self-notification.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_review_notification(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Option<Uuid>,
    receiver_id: Uuid,
    actor: Uuid,
    entity_name: &str,
    entity_identifier: Uuid,
    title: &str,
    sender: &str,
    data: Value,
) -> Result<(), sqlx::Error> {
    if receiver_id == actor {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO notifications (id, workspace_id, project_id, receiver_id, entity_name, \
         entity_identifier, title, sender, data, message_html, created_at, updated_at, \
         created_by_id, triggered_by_id) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, '<p></p>', now(), now(), $9, $9)",
    )
    .bind(workspace_id)
    .bind(project_id)
    .bind(receiver_id)
    .bind(entity_name)
    .bind(entity_identifier)
    .bind(title)
    .bind(sender)
    .bind(data)
    .bind(actor)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Participants
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ParticipantRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub attendance: String,
    pub created_at: DateTime<Utc>,
    pub display_name: String,
    pub email: Option<String>,
}

const PARTICIPANT_SELECT: &str = "SELECT p.id, p.user_id, p.role, p.attendance, p.created_at, \
    COALESCE(u.display_name, u.username) AS display_name, u.email \
    FROM review_session_participants p JOIN users u ON u.id = p.user_id";

fn participant_json(row: &ParticipantRow) -> Value {
    json!({
        "id": row.id,
        "user_id": row.user_id,
        "role": row.role,
        "attendance": row.attendance,
        "created_at": row.created_at,
        "display_name": row.display_name,
        "email": row.email,
    })
}

pub(crate) async fn participants_for_session(
    pool: &sqlx::PgPool,
    session_id: Uuid,
) -> Result<Vec<ParticipantRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{PARTICIPANT_SELECT} WHERE p.session_id = $1 AND p.deleted_at IS NULL \
         ORDER BY p.created_at ASC"
    ))
    .bind(session_id)
    .fetch_all(pool)
    .await
}

async fn participant_by_id(
    pool: &sqlx::PgPool,
    session_id: Uuid,
    participant_id: Uuid,
) -> Result<Option<ParticipantRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{PARTICIPANT_SELECT} WHERE p.id = $1 AND p.session_id = $2 AND p.deleted_at IS NULL"
    ))
    .bind(participant_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
}

async fn participant_by_user(
    pool: &sqlx::PgPool,
    session_id: Uuid,
    user_id: Uuid,
) -> Result<Option<ParticipantRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{PARTICIPANT_SELECT} WHERE p.user_id = $1 AND p.session_id = $2 AND p.deleted_at IS NULL"
    ))
    .bind(user_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Deserialize)]
pub struct ParticipantCreate {
    pub user_id: Uuid,
    pub role: Option<String>,
    pub attendance: Option<String>,
}

/// POST `/api/workspaces/:slug/review-sessions/:session_id/participants/`
pub async fn participants_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id)): Path<(String, Uuid)>,
    Json(body): Json<ParticipantCreate>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if session.status != "scheduled" {
        return Ok(bad_request("Session is not scheduled"));
    }
    let role = body.role.clone().unwrap_or_else(|| "member".to_string());
    if let Err(e) = validate_enum("role", &role, PARTICIPANT_ROLES) {
        return Ok(bad_request(e));
    }
    let attendance = body
        .attendance
        .clone()
        .unwrap_or_else(|| "invited".to_string());
    if let Err(e) = validate_enum("attendance", &attendance, ATTENDANCE_VALUES) {
        return Ok(bad_request(e));
    }
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id \
         WHERE w.slug = $1 AND wm.member_id = $2 AND wm.is_active = true AND wm.deleted_at IS NULL \
         AND w.deleted_at IS NULL)",
    )
    .bind(&slug)
    .bind(body.user_id)
    .fetch_one(&st.pool)
    .await?;
    if !is_member {
        return Ok(bad_request("Invalid user_id - not a workspace member."));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "INSERT INTO review_session_participants (id, workspace_id, session_id, user_id, role, \
         attendance, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now(), $6, $6) \
         ON CONFLICT (session_id, user_id) WHERE deleted_at IS NULL \
         DO UPDATE SET role = EXCLUDED.role, attendance = EXCLUDED.attendance, \
         updated_at = now(), updated_by_id = $6",
    )
    .bind(session.workspace_id)
    .bind(session.id)
    .bind(body.user_id)
    .bind(&role)
    .bind(&attendance)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;
    insert_review_notification(
        &mut tx,
        session.workspace_id,
        session.project_id,
        body.user_id,
        auth.0,
        "review_session",
        session.id,
        &format!("You are scheduled for {}", session.title),
        "in_app:review:session_scheduled",
        json!({
            "review_session": {
                "id": session.id,
                "board_type": session.board_type,
                "project_id": session.project_id,
                "workspace_slug": slug,
                "title": session.title,
                "scheduled_at": session.scheduled_at,
            }
        }),
    )
    .await?;
    tx.commit().await?;
    let row = participant_by_user(&st.pool, session.id, body.user_id)
        .await?
        .expect("participant just inserted");
    Ok((StatusCode::CREATED, Json(participant_json(&row))))
}

/// GET `/api/workspaces/:slug/review-sessions/:session_id/participants/`
pub async fn participants_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id)): Path<(String, Uuid)>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    let rows = participants_for_session(&st.pool, session.id).await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(participant_json).collect())),
    ))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ParticipantPatch {
    pub role: Option<String>,
    pub attendance: Option<String>,
}

/// PATCH `/api/workspaces/:slug/review-sessions/:session_id/participants/:participant_id/`
pub async fn participants_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id, participant_id)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<ParticipantPatch>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if body.role.is_none() && body.attendance.is_none() {
        return Ok(bad_request("Nothing to update"));
    }
    if let Some(role) = body.role.as_deref() {
        if let Err(e) = validate_enum("role", role, PARTICIPANT_ROLES) {
            return Ok(bad_request(e));
        }
    }
    if let Some(attendance) = body.attendance.as_deref() {
        if let Err(e) = validate_enum("attendance", attendance, ATTENDANCE_VALUES) {
            return Ok(bad_request(e));
        }
    }
    let Some(_current) = participant_by_id(&st.pool, session.id, participant_id).await? else {
        return Ok(missing());
    };
    let role = body.role.clone().unwrap_or_else(|| "member".to_string());
    let attendance = body
        .attendance
        .clone()
        .unwrap_or_else(|| "invited".to_string());
    sqlx::query(
        "UPDATE review_session_participants SET role = COALESCE($1, role), \
         attendance = COALESCE($2, attendance), updated_at = now(), updated_by_id = $3 \
         WHERE id = $4 AND session_id = $5 AND deleted_at IS NULL",
    )
    .bind(body.role.as_ref().map(|_| role))
    .bind(body.attendance.as_ref().map(|_| attendance))
    .bind(auth.0)
    .bind(participant_id)
    .bind(session.id)
    .execute(&st.pool)
    .await?;
    let row = participant_by_id(&st.pool, session.id, participant_id)
        .await?
        .expect("participant just updated");
    Ok((StatusCode::OK, Json(participant_json(&row))))
}

/// DELETE `/api/workspaces/:slug/review-sessions/:session_id/participants/:participant_id/`
pub async fn participants_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id, participant_id)): Path<(String, Uuid, Uuid)>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    sqlx::query(
        "UPDATE review_session_participants SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND session_id = $2 AND deleted_at IS NULL",
    )
    .bind(participant_id)
    .bind(session.id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

Then wire participants into `session_detail`: replace

```rust
    value["participants"] = json!([]);
```

with

```rust
    let participants = participants_for_session(&st.pool, session_id).await?;
    value["participants"] = json!(participants
        .iter()
        .map(participant_json)
        .collect::<Vec<_>>());
```

- [ ] **Step 4: Register participant routes**

In `apps/api-rs/crates/api/src/main.rs`, add after the review-session routes (Task 2):

```rust
        .route(
            "/api/workspaces/:slug/review-sessions/:session_id/participants/",
            get(routes::review::participants_list).post(routes::review::participants_create),
        )
        .route(
            "/api/workspaces/:slug/review-sessions/:session_id/participants/:participant_id/",
            patch(routes::review::participants_patch).delete(routes::review::participants_destroy),
        )
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test participant -- --test-threads=1
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test session_detail -- --test-threads=1
```

Expected: PASS — `participants_add_list_patch_remove_and_notify`, `participant_requires_workspace_member`; `session_detail_reads_scope` masih PASS (participants masih 0 di test itu).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/review.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/release_review_test.rs
git commit -m "feat(api-rs): review session participants + notifications"
```

---

### Task 4: Agenda items, outcomes, complete/cancel

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/review.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Test: `apps/api-rs/crates/api/tests/release_review_test.rs`

- [ ] **Step 1: Update imports and write the failing tests**

In `apps/api-rs/crates/api/tests/release_review_test.rs`, change the review import block to:

```rust
use api::routes::review::{
    cancel_session, complete_session, create_session, items_create, items_destroy, items_list,
    items_patch, list_requests, list_sessions, participants_create, participants_destroy,
    participants_list, participants_patch, patch_session, request_detail, session_detail,
    submit_request, withdraw_request, AddItems, CreateSession, ParticipantCreate, ParticipantPatch,
    PatchItem, PatchSession, RequestListParams, SessionListParams, SubmitRequest,
};
```

Then append to the file:

```rust
fn add_items(request_id: Uuid) -> AddItems {
    AddItems {
        request_ids: vec![request_id],
    }
}

fn request_id_of(body: &Value) -> Uuid {
    body["id"].as_str().unwrap().parse().unwrap()
}

#[tokio::test]
async fn agenda_outcomes_complete_and_notifications() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15)).await;
    insert_project_member(&st.pool, member, scratch.project_id, scratch.workspace_id, 15).await;
    let issue_id = scratch.insert_issue(&st.pool).await;
    let (_, Json(request)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("submit");
    let request_id = request_id_of(&request);
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Agenda")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();

    participants_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(participant_body(member, Some("secretary"))),
    )
    .await
    .expect("add secretary");

    let (status, Json(items)) = items_create(
        State(st.clone()),
        AuthUser(member),
        Path((scratch.slug.clone(), session_id)),
        Json(add_items(request_id)),
    )
    .await
    .expect("add items");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(items.as_array().unwrap().len(), 1);
    assert_eq!(items[0]["subject"]["kind"], "change");
    let item_id: Uuid = items[0]["id"].as_str().unwrap().parse().unwrap();

    let (_, Json(after_add)) = request_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), request_id)),
    )
    .await
    .expect("request after add");
    assert_eq!(after_add["status"], "scheduled");
    assert_eq!(after_add["history"].as_array().unwrap().len(), 1);

    let agenda_notified: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE receiver_id = $1 \
         AND sender = 'in_app:review:agenda_added' AND deleted_at IS NULL",
    )
    .bind(scratch.user_id)
    .fetch_one(&st.pool)
    .await
    .expect("count agenda notifications");
    assert_eq!(agenda_notified, 1);

    let (status, Json(item)) = items_patch(
        State(st.clone()),
        AuthUser(member),
        Path((scratch.slug.clone(), session_id, item_id)),
        Json(PatchItem {
            outcome: Some("approved_with_notes".into()),
            outcome_note: Some("Lanjut dengan catatan monitoring.".into()),
        }),
    )
    .await
    .expect("record outcome");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(item["outcome"], "approved_with_notes");

    let (_, Json(after_decision)) = request_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), request_id)),
    )
    .await
    .expect("request after decision");
    assert_eq!(after_decision["status"], "decided");

    let decided_notified: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE receiver_id = $1 \
         AND sender = 'in_app:review:decided' AND deleted_at IS NULL",
    )
    .bind(scratch.user_id)
    .fetch_one(&st.pool)
    .await
    .expect("count decided notifications");
    assert_eq!(decided_notified, 1);

    let (status, Json(done)) = complete_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("complete");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(done["status"], "completed");
    assert_eq!(done["counts"]["pending_outcome"], 0);

    let (status, Json(body)) = withdraw_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), request_id)),
    )
    .await
    .expect("withdraw decided");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Only pending or scheduled requests can be withdrawn"
    );

    scratch.cleanup(&st.pool).await;
}
```

Note: agenda/decided notification di test ini dikirim ke submitter (`scratch.user_id`) oleh aktor berbeda (`member` sebagai secretary). Karakter berbeda-aktor ini yang membuat notifikasi tidak di-skip `insert_review_notification`.

```rust
#[tokio::test]
async fn deferred_item_and_complete_release_to_pending() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool).await;
    let (_, Json(request)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("submit");
    let request_id = request_id_of(&request);
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Ditunda")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();
    let (_, Json(items)) = items_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(add_items(request_id)),
    )
    .await
    .expect("add item");
    let item_id: Uuid = items[0]["id"].as_str().unwrap().parse().unwrap();

    let (_, Json(_)) = items_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id, item_id)),
        Json(PatchItem {
            outcome: Some("deferred".into()),
            outcome_note: Some("Tunggu bukti UAT.".into()),
        }),
    )
    .await
    .expect("defer");

    let (_, Json(after_defer)) = request_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), request_id)),
    )
    .await
    .expect("request after defer");
    assert_eq!(after_defer["status"], "pending");

    complete_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("complete session");

    let (_, Json(second)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Susulan")),
    )
    .await
    .expect("create second session");
    let second_id: Uuid = second["id"].as_str().unwrap().parse().unwrap();
    let (status, Json(items)) = items_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), second_id)),
        Json(add_items(request_id)),
    )
    .await
    .expect("readd to second session");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(items.as_array().unwrap().len(), 1);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn item_guards_board_mismatch_double_schedule_and_empty_complete() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool).await;
    let (_, Json(request)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("submit");
    let request_id = request_id_of(&request);
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Guard A")),
    )
    .await
    .expect("create session A");
    let session_a: Uuid = session["id"].as_str().unwrap().parse().unwrap();
    items_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_a)),
        Json(add_items(request_id)),
    )
    .await
    .expect("add to A");

    let (_, Json(session_b)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Guard B")),
    )
    .await
    .expect("create session B");
    let session_b: Uuid = session_b["id"].as_str().unwrap().parse().unwrap();
    let (status, Json(body)) = items_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_b)),
        Json(add_items(request_id)),
    )
    .await
    .expect("double schedule");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Request is already in another scheduled session");

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis Guard")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);
    let (_, Json(rcb_request)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("rcb", None, Some(rid))),
    )
    .await
    .expect("submit rcb");
    let rcb_id = request_id_of(&rcb_request);
    let (status, Json(body)) = items_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_b)),
        Json(add_items(rcb_id)),
    )
    .await
    .expect("board mismatch");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Request board does not match session board");

    let (_, Json(empty)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Kosong")),
    )
    .await
    .expect("create empty session");
    let empty_id: Uuid = empty["id"].as_str().unwrap().parse().unwrap();
    let (status, Json(body)) = complete_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), empty_id)),
    )
    .await
    .expect("complete empty");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Session has no agenda items");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn cancel_session_guards_final_outcomes_and_resets_requests() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis Cancel")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);
    let (_, Json(request)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("rcb", None, Some(rid))),
    )
    .await
    .expect("submit rcb");
    let request_id = request_id_of(&request);
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("rcb", None, "RCB Cancel")),
    )
    .await
    .expect("create rcb session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();
    let (_, Json(items)) = items_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(add_items(request_id)),
    )
    .await
    .expect("add rcb item");
    let item_id: Uuid = items[0]["id"].as_str().unwrap().parse().unwrap();
    items_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id, item_id)),
        Json(PatchItem {
            outcome: Some("approved".into()),
            outcome_note: None,
        }),
    )
    .await
    .expect("approve");
    let (status, Json(body)) = cancel_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("cancel with final");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Session already has final decisions");

    let (_, Json(items)) = items_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("items list");
    assert_eq!(items.as_array().unwrap().len(), 1);

    let (status, _) = items_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id, item_id)),
    )
    .await
    .expect("remove final item");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    sqlx::query(
        "UPDATE review_session_items SET outcome = 'deferred', decided_at = NULL WHERE id = $1",
    )
    .bind(item_id)
    .execute(&st.pool)
    .await
    .expect("reset outcome");

    let (status, Json(done)) = cancel_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("cancel after reset");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(done["status"], "cancelled");

     scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd apps/api-rs && cargo test -p api --test release_review_test agenda_ --no-run
```

Expected: FAIL to compile — `items_create`, `AddItems`, `PatchItem`, `complete_session`, `cancel_session` not found.

- [ ] **Step 3: Implement items + complete/cancel**

Append to `apps/api-rs/crates/api/src/routes/review.rs` (sebelum `#[cfg(test)] mod tests`):

```rust
// ---------------------------------------------------------------------------
// Agenda items + outcomes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionItemRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub review_request_id: Uuid,
    pub position: i32,
    pub outcome: Option<String>,
    pub outcome_note: String,
    pub decided_by_id: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub request_status: String,
    pub board_type: String,
    pub submission_note: String,
    pub issue_identifier: Option<String>,
    pub issue_name: Option<String>,
    pub release_name: Option<String>,
    pub release_version: Option<String>,
}

const ITEM_SELECT: &str = "SELECT i.id, i.session_id, i.review_request_id, i.position, i.outcome, \
    i.outcome_note, i.decided_by_id, i.decided_at, i.created_at, rr.status AS request_status, \
    rr.board_type, rr.submission_note, \
    (p.identifier || '-' || iss.sequence_id::text) AS issue_identifier, iss.name AS issue_name, \
    rel.name AS release_name, rel.version AS release_version \
    FROM review_session_items i \
    JOIN review_requests rr ON rr.id = i.review_request_id \
    LEFT JOIN issues iss ON iss.id = rr.change_issue_id \
    LEFT JOIN projects p ON p.id = iss.project_id \
    LEFT JOIN releases rel ON rel.id = rr.release_id";

fn item_json(row: &SessionItemRow) -> Value {
    let subject = if row.board_type == "tcb" {
        json!({
            "kind": "change",
            "identifier": row.issue_identifier,
            "name": row.issue_name,
            "version": Value::Null,
        })
    } else {
        json!({
            "kind": "release",
            "identifier": Value::Null,
            "name": row.release_name,
            "version": row.release_version,
        })
    };
    json!({
        "id": row.id,
        "session_id": row.session_id,
        "review_request_id": row.review_request_id,
        "position": row.position,
        "outcome": row.outcome,
        "outcome_note": row.outcome_note,
        "decided_by": row.decided_by_id,
        "decided_at": row.decided_at,
        "created_at": row.created_at,
        "request_status": row.request_status,
        "submission_note": row.submission_note,
        "subject": subject,
    })
}

pub(crate) async fn items_for_session(
    pool: &sqlx::PgPool,
    session_id: Uuid,
) -> Result<Vec<SessionItemRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{ITEM_SELECT} WHERE i.session_id = $1 AND i.deleted_at IS NULL \
         ORDER BY i.position ASC, i.created_at ASC"
    ))
    .bind(session_id)
    .fetch_all(pool)
    .await
}

async fn item_by_id(
    pool: &sqlx::PgPool,
    session_id: Uuid,
    item_id: Uuid,
) -> Result<Option<SessionItemRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{ITEM_SELECT} WHERE i.id = $1 AND i.session_id = $2 AND i.deleted_at IS NULL"
    ))
    .bind(item_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Deserialize)]
pub struct AddItems {
    pub request_ids: Vec<Uuid>,
}

/// POST `/api/workspaces/:slug/review-sessions/:session_id/items/`
pub async fn items_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id)): Path<(String, Uuid)>,
    Json(body): Json<AddItems>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if session.status != "scheduled" {
        return Ok(bad_request("Session is not scheduled"));
    }
    let mut ids = body.request_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(bad_request("Invalid request_ids - object does not exist."));
    }
    let mut tx = st.pool.begin().await?;
    for request_id in &ids {
        let Some(request) = request_in_workspace(&st.pool, &slug, *request_id).await? else {
            return Ok(bad_request("Invalid request_ids - object does not exist."));
        };
        if request.board_type != session.board_type {
            return Ok(bad_request("Request board does not match session board"));
        }
        if request.board_type == "tcb" && request.project_id != session.project_id {
            return Ok(bad_request("Request is not part of this project"));
        }
        if request.status != "pending" {
            return Ok(bad_request("Request is not pending"));
        }
        let in_other: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM review_session_items i \
             JOIN review_sessions s ON s.id = i.session_id \
             WHERE i.review_request_id = $1 AND i.deleted_at IS NULL \
             AND s.status = 'scheduled' AND s.id != $2 AND s.deleted_at IS NULL)",
        )
        .bind(request_id)
        .bind(session.id)
        .fetch_one(&mut *tx)
        .await?;
        if in_other {
            return Ok(bad_request("Request is already in another scheduled session"));
        }
        let already: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM review_session_items \
             WHERE session_id = $1 AND review_request_id = $2 AND deleted_at IS NULL)",
        )
        .bind(session.id)
        .bind(request_id)
        .fetch_one(&mut *tx)
        .await?;
        if already {
            continue;
        }
        let position: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM review_session_items WHERE session_id = $1",
        )
        .bind(session.id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO review_session_items (id, workspace_id, session_id, review_request_id, \
             position, created_at, updated_at, created_by_id, updated_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now(), $5, $5)",
        )
        .bind(session.workspace_id)
        .bind(session.id)
        .bind(request_id)
        .bind(position)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE review_requests SET status = 'scheduled', updated_at = now(), updated_by_id = $1 \
             WHERE id = $2 AND deleted_at IS NULL",
        )
        .bind(auth.0)
        .bind(request_id)
        .execute(&mut *tx)
        .await?;
        if let Some(receiver) = request.submitted_by_id {
            insert_review_notification(
                &mut tx,
                session.workspace_id,
                session.project_id,
                receiver,
                auth.0,
                "review_request",
                *request_id,
                &format!("Review request added to {}", session.title),
                "in_app:review:agenda_added",
                json!({
                    "review_request": {
                        "id": request.id,
                        "board_type": request.board_type,
                        "project_id": request.project_id,
                        "workspace_slug": slug,
                        "status": "scheduled",
                        "subject_label": subject_label(&request),
                        "session_id": session.id,
                        "session_title": session.title,
                        "scheduled_at": session.scheduled_at,
                    }
                }),
            )
            .await?;
        }
    }
    tx.commit().await?;
    let rows = items_for_session(&st.pool, session.id).await?;
    Ok((
        StatusCode::CREATED,
        Json(Value::Array(rows.iter().map(item_json).collect())),
    ))
}

/// GET `/api/workspaces/:slug/review-sessions/:session_id/items/`
pub async fn items_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id)): Path<(String, Uuid)>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    let rows = items_for_session(&st.pool, session.id).await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(item_json).collect())),
    ))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchItem {
    pub outcome: Option<String>,
    pub outcome_note: Option<String>,
}

/// PATCH `/api/workspaces/:slug/review-sessions/:session_id/items/:item_id/`
pub async fn items_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id, item_id)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchItem>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if session.status != "scheduled" {
        return Ok(bad_request("Session is not scheduled"));
    }
    let Some(current) = item_by_id(&st.pool, session.id, item_id).await? else {
        return Ok(missing());
    };
    let outcome = body.outcome.clone().unwrap_or_default();
    if let Err(e) = validate_enum("outcome", &outcome, OUTCOMES) {
        return Ok(bad_request(e));
    }
    let is_final = FINAL_OUTCOMES.contains(&outcome.as_str());
    let note = body
        .outcome_note
        .clone()
        .unwrap_or_else(|| current.outcome_note.clone());
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE review_session_items SET outcome = $1, outcome_note = $2, decided_by_id = $3, \
         decided_at = now(), updated_at = now(), updated_by_id = $3 \
         WHERE id = $4 AND session_id = $5 AND deleted_at IS NULL",
    )
    .bind(&outcome)
    .bind(&note)
    .bind(auth.0)
    .bind(item_id)
    .bind(session.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE review_requests SET status = $1, updated_at = now(), updated_by_id = $2 \
         WHERE id = $3 AND deleted_at IS NULL",
    )
    .bind(if is_final { "decided" } else { "pending" })
    .bind(auth.0)
    .bind(current.review_request_id)
    .execute(&mut *tx)
    .await?;
    if is_final {
        if let Some(request) = request_in_workspace(&st.pool, &slug, current.review_request_id)
            .await?
        {
            if let Some(receiver) = request.submitted_by_id {
                insert_review_notification(
                    &mut tx,
                    session.workspace_id,
                    session.project_id,
                    receiver,
                    auth.0,
                    "review_request",
                    request.id,
                    &format!(
                        "Review decision for {}: {outcome}",
                        subject_label(&request)
                    ),
                    "in_app:review:decided",
                    json!({
                        "review_request": {
                            "id": request.id,
                            "board_type": request.board_type,
                            "project_id": request.project_id,
                            "workspace_slug": slug,
                            "status": "decided",
                            "subject_label": subject_label(&request),
                            "outcome": outcome,
                            "session_id": session.id,
                            "session_title": session.title,
                        }
                    }),
                )
                .await?;
            }
        }
    }
    tx.commit().await?;
    let row = item_by_id(&st.pool, session.id, item_id)
        .await?
        .expect("item just updated");
    Ok((StatusCode::OK, Json(item_json(&row))))
}

/// DELETE `/api/workspaces/:slug/review-sessions/:session_id/items/:item_id/`
pub async fn items_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id, item_id)): Path<(String, Uuid, Uuid)>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if session.status != "scheduled" {
        return Ok(bad_request("Session is not scheduled"));
    }
    let Some(current) = item_by_id(&st.pool, session.id, item_id).await? else {
        return Ok(missing());
    };
    if current
        .outcome
        .as_deref()
        .is_some_and(|outcome| FINAL_OUTCOMES.contains(&outcome))
    {
        return Ok(bad_request("Item with a final outcome cannot be removed"));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE review_session_items SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND session_id = $2 AND deleted_at IS NULL",
    )
    .bind(item_id)
    .bind(session.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE review_requests SET status = 'pending', updated_at = now(), updated_by_id = $1 \
         WHERE id = $2 AND status = 'scheduled' AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(current.review_request_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ---------------------------------------------------------------------------
// Complete / cancel
// ---------------------------------------------------------------------------

/// POST `/api/workspaces/:slug/review-sessions/:session_id/complete/`
pub async fn complete_session(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id)): Path<(String, Uuid)>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if session.status != "scheduled" {
        return Ok(bad_request("Session is not scheduled"));
    }
    if session.item_count == 0 {
        return Ok(bad_request("Session has no agenda items"));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE review_session_items SET outcome = 'deferred', updated_at = now() \
         WHERE session_id = $1 AND outcome IS NULL AND deleted_at IS NULL",
    )
    .bind(session.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE review_requests SET status = 'pending', updated_at = now() \
         WHERE status = 'scheduled' AND id IN ( \
             SELECT review_request_id FROM review_session_items \
             WHERE session_id = $1 AND deleted_at IS NULL)",
    )
    .bind(session.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE review_sessions SET status = 'completed', completed_at = now(), updated_at = now(), \
         updated_by_id = $1 WHERE id = $2 AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(session.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row = session_in_workspace(&st.pool, &slug, session_id)
        .await?
        .expect("session just completed");
    Ok((StatusCode::OK, Json(session_json(&row))))
}

/// POST `/api/workspaces/:slug/review-sessions/:session_id/cancel/`
pub async fn cancel_session(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id)): Path<(String, Uuid)>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if session.status != "scheduled" {
        return Ok(bad_request("Session is not scheduled"));
    }
    let has_final: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM review_session_items WHERE session_id = $1 \
         AND deleted_at IS NULL AND outcome IN ('approved', 'rejected', 'approved_with_notes'))",
    )
    .bind(session.id)
    .fetch_one(&st.pool)
    .await?;
    if has_final {
        return Ok(bad_request("Session already has final decisions"));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE review_requests SET status = 'pending', updated_at = now() \
         WHERE status = 'scheduled' AND id IN ( \
             SELECT review_request_id FROM review_session_items \
             WHERE session_id = $1 AND deleted_at IS NULL)",
    )
    .bind(session.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE review_sessions SET status = 'cancelled', cancelled_at = now(), updated_at = now(), \
         updated_by_id = $1 WHERE id = $2 AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(session.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row = session_in_workspace(&st.pool, &slug, session_id)
        .await?
        .expect("session just cancelled");
    Ok((StatusCode::OK, Json(session_json(&row))))
}
```

Then wire items into `session_detail`: replace

```rust
    value["items"] = json!([]);
```

with

```rust
    let items = items_for_session(&st.pool, session_id).await?;
    value["items"] = json!(items.iter().map(item_json).collect::<Vec<_>>());
```

- [ ] **Step 4: Register items + complete/cancel routes**

In `apps/api-rs/crates/api/src/main.rs`, add after the participant routes (Task 3):

```rust
        .route(
            "/api/workspaces/:slug/review-sessions/:session_id/complete/",
            post(routes::review::complete_session),
        )
        .route(
            "/api/workspaces/:slug/review-sessions/:session_id/cancel/",
            post(routes::review::cancel_session),
        )
        .route(
            "/api/workspaces/:slug/review-sessions/:session_id/items/",
            get(routes::review::items_list).post(routes::review::items_create),
        )
        .route(
            "/api/workspaces/:slug/review-sessions/:session_id/items/:item_id/",
            patch(routes::review::items_patch).delete(routes::review::items_destroy),
        )
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test -- --test-threads=1
```

Expected: PASS — seluruh 21 test di `release_review_test` (9 Phase 1 + 12 Phase 2).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/review.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/release_review_test.rs
git commit -m "feat(api-rs): review agenda outcomes + complete/cancel"
```

---

### Task 5: Verification wrap-up

**Files:** none (verification only).

- [ ] **Step 1: Unit tests + route inventory**

Run:

```bash
cd apps/api-rs && cargo test -p api --lib review:: && cargo test -p api --test route_inventory_test
```

Expected: `review::tests` PASS; `route_inventory_test` PASS (route fork baru tidak masuk inventory Django-parity).

- [ ] **Step 2: Format check**

Run:

```bash
cd apps/api-rs && cargo fmt --check
```

Expected: no diff. Jika ada, `cargo fmt` lalu ulangi Step 1.

- [ ] **Step 3: Rebuild + boot smoke**

Rebuild (detached) sesuai `AGENTS.md`:

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
until curl -sf http://localhost:8000/health > /dev/null; do sleep 15; done; echo up
systemctl --user restart plane-live.service && curl -sf http://localhost:3100/live/health/ && echo live-up
```

Expected: `up` dan `live-up`.

- [ ] **Step 4: Commit (jika ada perbaikan format)**

```bash
git add -A apps/api-rs
git commit -m "chore(api-rs): fmt review engine"
```

(Skip jika tidak ada perubahan.)

---

## Self-review notes

- **Spec coverage Phase 2:** submit eksplisit + 1 request aktif per subject ✔; lifecycle request (pending/scheduled/decided/withdrawn, deferred → pending) ✔; session + peserta + agenda + outcome tunggal ✔; complete (auto-defer, wajib ≥1 item) & cancel (guard outcome final) ✔; withdraw menghapus dari agenda ✔; guard item final tidak bisa dihapus ✔; notifikasi 3 event ✔; permission per board (project member/admin vs workspace member/admin) ✔; `GET /review-requests/` filter badges (`change_issue_id`/`release_id`) ✔.
- **Phase 3 (web):** types/services/stores, halaman Releases + RCB + TCB, session detail, integrasi change detail, render notifikasi.
- **Deviasi sadar:** notifikasi `session_scheduled` dikirim saat participant ditambahkan (bukan saat session dibuat) karena peserta dikelola via endpoint terpisah.
