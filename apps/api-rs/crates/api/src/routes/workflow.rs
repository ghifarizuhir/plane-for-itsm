//! Workflows workspace-level (state + transisi) dan materialization state ke
//! project. Spec: docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md

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
    routes::project::{deny, is_integrity_error, missing, ws_role},
    state::AppState,
};

use super::issue_common::{
    bad, fetch_project_member_role, is_workspace_admin, project_gate_allows,
};

/// Group valid untuk workflow state; `triage` bukan bagian workflow
/// (`StateGroup` di `apps/api/plane/db/models/state.py:14-20`).
/// Alias ke `state::ALLOWED_GROUPS` agar daftarnya tidak pernah menyimpang.
pub const STATE_GROUPS: &[&str] = super::state::ALLOWED_GROUPS;

/// Validasi nama ala DRF (required, <=255). Mengembalikan nama ter-trim.
pub fn validate_name(name: &str, field: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    if trimmed.chars().count() > 255 {
        return Err(format!("Ensure {field} has no more than 255 characters."));
    }
    Ok(trimmed.to_string())
}

pub fn validate_state_group(group: &str) -> Result<(), String> {
    if STATE_GROUPS.contains(&group) {
        Ok(())
    } else {
        Err(format!("\"{group}\" is not a valid choice."))
    }
}

/// Target state (mirror) yang diizinkan dari `current_state`, dihitung murni
/// dari pasangan `(state_id, workflow_state_id)` dan daftar transisi
/// `(from_workflow_state_id, to_workflow_state_id)`. Urutan output mengikuti
/// urutan `mirror_pairs` dan tidak di-sort lebih lanjut.
pub fn allowed_target_state_ids(
    current_state: Uuid,
    mirror_pairs: &[(Uuid, Uuid)],
    transitions: &[(Uuid, Uuid)],
) -> Vec<Uuid> {
    let Some(current_workflow_state) = mirror_pairs
        .iter()
        .find(|(state_id, _)| *state_id == current_state)
        .map(|(_, workflow_state_id)| *workflow_state_id)
    else {
        return Vec::new();
    };
    let allowed_workflow_states: Vec<Uuid> = transitions
        .iter()
        .filter(|(from, _)| *from == current_workflow_state)
        .map(|(_, to)| *to)
        .collect();
    mirror_pairs
        .iter()
        .filter(|(_, workflow_state_id)| allowed_workflow_states.contains(workflow_state_id))
        .map(|(state_id, _)| *state_id)
        .collect()
}

/// Materialize seluruh state workflow milik `type_id` ke project (idempotent).
/// Return jumlah mirror yang diproses (update + insert), bukan jumlah baris
/// yang benar-benar berubah.
pub async fn materialize_type_states(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let workflow_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT workflow_id FROM issue_types WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(type_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    let Some(workflow_id) = workflow_id else {
        return Ok(0);
    };
    materialize_workflow_for_project(pool, project_id, type_id, workflow_id).await
}

/// Inti materialization: turunkan default lama, soft-delete mirror usang,
/// revive + update mirror, lalu insert mirror baru. Satu transaksi. Return
/// jumlah mirror yang diproses (update + insert), bukan jumlah baris yang
/// benar-benar berubah.
pub(crate) async fn materialize_workflow_for_project(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
    workflow_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // 1. Turunkan default lama lebih dulu agar partial-unique default
    //    (project_id, type_id) tidak bentrok saat mirror baru di-insert.
    sqlx::query(
        "UPDATE states SET \"default\" = false, updated_at = now() \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL AND \"default\" = true",
    )
    .bind(project_id)
    .bind(type_id)
    .execute(&mut *tx)
    .await?;

    // 2. Soft-delete mirror usang: workflow_state-nya sudah dihapus ATAU
    //    milik workflow lain (type pindah workflow). Harus sebelum insert,
    //    kalau tidak nama state lama menabrak partial-unique
    //    (project_id, type_id, name) saat nama state baru sama.
    sqlx::query(
        "UPDATE states s SET deleted_at = now(), updated_at = now() \
         WHERE s.project_id = $1 AND s.type_id = $2 AND s.deleted_at IS NULL \
         AND s.workflow_state_id IS NOT NULL \
         AND NOT EXISTS (SELECT 1 FROM workflow_states ws \
                         WHERE ws.id = s.workflow_state_id AND ws.deleted_at IS NULL \
                           AND ws.workflow_id = $3)",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(workflow_id)
    .execute(&mut *tx)
    .await?;

    // 3. Revive + update mirror (termasuk mirror workflow ini yang sempat
    //    soft-deleted).
    let updated = sqlx::query(
        "UPDATE states s SET name = ws.name, description = ws.description, color = ws.color, \
         slug = ws.slug, sequence = ws.sequence, \"group\" = ws.\"group\", \"default\" = ws.is_default, \
         deleted_at = NULL, updated_at = now() \
         FROM workflow_states ws \
         WHERE s.workflow_state_id = ws.id AND s.project_id = $1 AND s.type_id = $2 \
           AND ws.workflow_id = $3 AND ws.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(workflow_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    // 4. Insert mirror baru untuk workflow_state yang belum punya mirror.
    //    Hanya baris hidup yang menghalangi: baris soft-deleted milik type
    //    lain (mis. type yang sudah di-unlink) tidak boleh menahan insert,
    //    karena partial-unique `(project, workflow_state) WHERE deleted_at
    //    IS NULL` hanya mengikat baris hidup.
    let inserted = sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, type_id, workflow_state_id, created_at, updated_at) \
         SELECT gen_random_uuid(), ws.name, ws.description, ws.color, ws.slug, ws.sequence, ws.\"group\", \
         false, ws.is_default, $1, p.workspace_id, $2, ws.id, now(), now() \
         FROM workflow_states ws JOIN projects p ON p.id = $1 \
         WHERE ws.workflow_id = $3 AND ws.deleted_at IS NULL \
         AND NOT EXISTS (SELECT 1 FROM states s WHERE s.project_id = $1 AND s.workflow_state_id = ws.id \
                         AND s.deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(workflow_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(updated + inserted)
}

/// Sync satu workflow ke semua project hidup yang mengaktifkan type-nya.
pub(crate) async fn sync_workflow_to_projects(
    pool: &sqlx::PgPool,
    workflow_id: Uuid,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT DISTINCT pit.project_id, t.id FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         JOIN projects p ON p.id = pit.project_id \
         WHERE t.workflow_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
           AND p.deleted_at IS NULL AND p.archived_at IS NULL",
    )
    .bind(workflow_id)
    .fetch_all(pool)
    .await?;
    for (project_id, type_id) in rows {
        materialize_workflow_for_project(pool, project_id, type_id, workflow_id).await?;
    }
    Ok(())
}

/// Pastikan semua type yang aktif di project sudah ter-materialize.
pub(crate) async fn ensure_project_workflows(
    pool: &sqlx::PgPool,
    project_id: Uuid,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT pit.issue_type_id, t.workflow_id FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         WHERE pit.project_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
           AND t.workflow_id IS NOT NULL",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    for (type_id, workflow_id) in rows {
        materialize_workflow_for_project(pool, project_id, type_id, workflow_id).await?;
    }
    Ok(())
}

// --- Konteks + evaluasi transisi -------------------------------------------

/// Konteks transisi satu type di satu project.
#[derive(Debug, Clone, Default)]
pub struct TransitionContext {
    /// (state_id, workflow_state_id) — mirror typed state di project.
    pub pairs: Vec<(Uuid, Uuid)>,
    /// (from_workflow_state_id, to_workflow_state_id).
    pub transitions: Vec<(Uuid, Uuid)>,
    pub default_state_id: Option<Uuid>,
}

/// Pure: `Ok(())` bila transisi boleh; `Err(allowed_state_ids)` bila ditolak.
///
/// Dua bentuk `Err`:
/// - `Err(vec![])` — target di luar workflow type ini, atau legacy tanpa
///   default: tidak ada state yang diizinkan;
/// - `Err(allowed)` — target di dalam workflow tapi tidak terjangkau dari
///   state sekarang; untuk state legacy, `allowed` = `[default_state_id]`.
pub fn evaluate_transition(
    current_state: Uuid,
    target_state: Uuid,
    ctx: &TransitionContext,
) -> Result<(), Vec<Uuid>> {
    if current_state == target_state {
        return Ok(());
    }
    if !ctx
        .pairs
        .iter()
        .any(|(state_id, _)| *state_id == current_state)
    {
        return match ctx.default_state_id {
            Some(default_state_id) if default_state_id == target_state => Ok(()),
            Some(default_state_id) => Err(vec![default_state_id]),
            None => Err(Vec::new()),
        };
    }
    if !ctx
        .pairs
        .iter()
        .any(|(state_id, _)| *state_id == target_state)
    {
        return Err(Vec::new());
    }
    let allowed = allowed_target_state_ids(current_state, &ctx.pairs, &ctx.transitions);
    if allowed.contains(&target_state) {
        Ok(())
    } else {
        Err(allowed)
    }
}

/// DB: konteks untuk (project, type). `None` = legacy (type tanpa workflow /
/// epic / tidak aktif di project).
pub(crate) async fn fetch_transition_context(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
) -> Result<Option<TransitionContext>, sqlx::Error> {
    let workflow_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT t.workflow_id FROM issue_types t \
         JOIN project_issue_types pit ON pit.issue_type_id = t.id AND pit.project_id = $2 \
           AND pit.deleted_at IS NULL \
         WHERE t.id = $1 AND t.deleted_at IS NULL AND t.is_epic = false",
    )
    .bind(type_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    let Some(workflow_id) = workflow_id else {
        return Ok(None);
    };
    let pairs: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, workflow_state_id FROM states \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL \
           AND workflow_state_id IS NOT NULL \
         ORDER BY sequence, created_at, id",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_all(pool)
    .await?;
    let transitions: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT from_state_id, to_state_id FROM workflow_transitions \
         WHERE workflow_id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .fetch_all(pool)
    .await?;
    let default_state_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND type_id = $2 AND \"default\" = true \
         AND deleted_at IS NULL ORDER BY sequence LIMIT 1",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_optional(pool)
    .await?;
    Ok(Some(TransitionContext {
        pairs,
        transitions,
        default_state_id,
    }))
}

/// `EXISTS`: `target_state` is a live state owned by `type_id` in this
/// project. Both validators enforce this BEFORE consulting any workflow
/// context, so a typed issue can never move to a legacy (`type_id IS NULL`)
/// or cross-type state even when the type is workflow-less, not enabled in
/// the project, or an epic (rule 3, unconditional). The type default passes
/// because it belongs to the type.
async fn target_state_belongs_to_type(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    target_state: Uuid,
    type_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 \
         AND deleted_at IS NULL AND type_id IS NOT DISTINCT FROM $3)",
    )
    .bind(target_state)
    .bind(project_id)
    .bind(type_id)
    .fetch_one(pool)
    .await
}

/// Validasi transisi untuk satu issue. `Ok(Ok(()))` lolos/legacy,
/// `Ok(Err(allowed))` ditolak.
pub(crate) async fn validate_state_transition(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Option<Uuid>,
    current_state: Uuid,
    target_state: Uuid,
) -> Result<Result<(), Vec<Uuid>>, sqlx::Error> {
    let Some(type_id) = type_id else {
        return Ok(Ok(()));
    };
    if !target_state_belongs_to_type(pool, project_id, target_state, type_id).await? {
        return Ok(Err(Vec::new()));
    }
    let Some(ctx) = fetch_transition_context(pool, project_id, type_id).await? else {
        return Ok(Ok(()));
    };
    Ok(evaluate_transition(current_state, target_state, &ctx))
}

/// Validasi saat issue belum punya state: hanya default type yang diizinkan.
/// `Uuid::nil()` bukan state nyata dan tidak pernah ada di `pairs`, jadi
/// `evaluate_transition` memakai cabang legacy → hanya default type lolos.
pub(crate) async fn validate_initial_transition(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Option<Uuid>,
    target_state: Uuid,
) -> Result<Result<(), Vec<Uuid>>, sqlx::Error> {
    let Some(type_id) = type_id else {
        return Ok(Ok(()));
    };
    if !target_state_belongs_to_type(pool, project_id, target_state, type_id).await? {
        return Ok(Err(Vec::new()));
    }
    let Some(ctx) = fetch_transition_context(pool, project_id, type_id).await? else {
        return Ok(Ok(()));
    };
    Ok(evaluate_transition(Uuid::nil(), target_state, &ctx))
}

/// 400 untuk transisi ditolak (dipakai semua jalur ubah state).
pub(crate) fn transition_denied(allowed: Vec<Uuid>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "Invalid state transition", "allowed_state_ids": allowed })),
    )
}

// --- Workspace-admin CRUD --------------------------------------------------

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

/// Duplikat nama workflow saja. Django membuat
/// `workflow_unique_name_workspace_when_deleted_at_null` sebagai partial unique
/// index (`apps/api/plane/db/models/workflow.py:28-34`), dan PostgreSQL
/// melaporkan namanya lewat field constraint saat index itu dilanggar. Error
/// kelas 23 lain (FK/check) tidak boleh dilabeli "sudah ada". Fallback: kalau
/// driver tidak mengekspos nama constraint, pakai pemetaan kelas-23 lama.
fn is_duplicate_workflow_name(e: &sqlx::Error) -> bool {
    let Some(db) = e.as_database_error() else {
        return false;
    };
    if let Some(constraint) = db
        .try_downcast_ref::<sqlx::postgres::PgDatabaseError>()
        .and_then(|pg| pg.constraint())
    {
        return constraint == "workflow_unique_name_workspace_when_deleted_at_null";
    }
    db.code()
        .as_deref()
        .map(is_integrity_error)
        .unwrap_or(false)
}

/// Duplikat nama/slug state workflow. Django membuat partial unique index
/// `workflow_state_unique_name_workflow_when_deleted_at_null` dan
/// `workflow_state_unique_slug_workflow_when_deleted_at_null`
/// (`apps/api/plane/db/models/workflow.py:57-73`); PostgreSQL melaporkan
/// namanya lewat field constraint saat index itu dilanggar. Error kelas 23
/// lain (FK/check/default) tidak boleh dilabeli "sudah ada". Fallback: kalau
/// driver tidak mengekspos nama constraint, pakai pemetaan kelas-23 lama.
fn is_duplicate_workflow_state(e: &sqlx::Error) -> bool {
    let Some(db) = e.as_database_error() else {
        return false;
    };
    if let Some(constraint) = db
        .try_downcast_ref::<sqlx::postgres::PgDatabaseError>()
        .and_then(|pg| pg.constraint())
    {
        return constraint == "workflow_state_unique_name_workflow_when_deleted_at_null"
            || constraint == "workflow_state_unique_slug_workflow_when_deleted_at_null";
    }
    db.code()
        .as_deref()
        .map(is_integrity_error)
        .unwrap_or(false)
}

/// Pelanggaran partial-unique default
/// (`workflow_state_unique_default_workflow_when_deleted_at_null`). Hanya
/// constraint ini yang dipetakan; fallback kelas-23 sengaja tidak dipakai
/// agar error duplikat nama/slug tidak salah dilabeli default.
fn is_duplicate_workflow_default(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|db| db.try_downcast_ref::<sqlx::postgres::PgDatabaseError>())
        .and_then(|pg| pg.constraint())
        .map(|constraint| {
            constraint == "workflow_state_unique_default_workflow_when_deleted_at_null"
        })
        .unwrap_or(false)
}

/// Duplikat pasangan transisi. Django membuat partial unique index
/// `workflow_transition_unique_pair_when_deleted_at_null`
/// (`apps/api/plane/db/models/workflow.py:94-98`); PostgreSQL melaporkan
/// namanya lewat field constraint saat index itu dilanggar. Error kelas 23
/// lain (FK/check) tidak boleh dilabeli "sudah ada". Fallback: kalau driver
/// tidak mengekspos nama constraint, pakai pemetaan kelas-23 lama.
fn is_duplicate_transition(e: &sqlx::Error) -> bool {
    let Some(db) = e.as_database_error() else {
        return false;
    };
    if let Some(constraint) = db
        .try_downcast_ref::<sqlx::postgres::PgDatabaseError>()
        .and_then(|pg| pg.constraint())
    {
        return constraint == "workflow_transition_unique_pair_when_deleted_at_null";
    }
    db.code()
        .as_deref()
        .map(is_integrity_error)
        .unwrap_or(false)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkflowRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub workspace_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub fn workflow_json(row: &WorkflowRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "description": row.description,
        "is_active": row.is_active,
        "workspace_id": row.workspace_id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct WorkflowBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

async fn is_ws_admin(st: &AppState, user: Uuid, slug: &str) -> Result<bool, sqlx::Error> {
    Ok(matches!(ws_role(&st.pool, user, slug).await?, Some(r) if r >= 20))
}

async fn workflow_in_workspace(
    pool: &sqlx::PgPool,
    slug: &str,
    workflow_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let (ok,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM workflows w JOIN workspaces ws ON ws.id = w.workspace_id \
         WHERE w.id = $1 AND ws.slug = $2 AND w.deleted_at IS NULL AND ws.deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .bind(slug)
    .fetch_one(pool)
    .await?;
    Ok(ok)
}

/// GET `/api/workspaces/:slug/workflows/`
pub async fn list_workflows(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    let rows: Vec<WorkflowRow> = sqlx::query_as(
        "SELECT w.id, w.name, w.description, w.is_active, w.workspace_id, w.created_at, w.updated_at \
         FROM workflows w JOIN workspaces ws ON ws.id = w.workspace_id \
         WHERE ws.slug = $1 AND ws.deleted_at IS NULL AND w.deleted_at IS NULL \
         ORDER BY w.created_at DESC",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(workflow_json).collect::<Vec<_>>())),
    ))
}

/// POST `/api/workspaces/:slug/workflows/`
pub async fn create_workflow(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<WorkflowBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    let name = match validate_name(body.name.as_deref().unwrap_or(""), "name") {
        Ok(name) => name,
        Err(e) => return Ok(bad(&e)),
    };
    let inserted: Result<Option<WorkflowRow>, sqlx::Error> = sqlx::query_as(
        "INSERT INTO workflows (id, name, description, is_active, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, COALESCE($2, ''), COALESCE($3, true), w.id, $4, $4, now(), now() \
         FROM workspaces w WHERE w.slug = $5 AND w.deleted_at IS NULL \
         RETURNING id, name, description, is_active, workspace_id, created_at, updated_at",
    )
    .bind(&name)
    .bind(&body.description)
    .bind(body.is_active)
    .bind(auth.0)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await;
    match inserted {
        Ok(Some(row)) => Ok((StatusCode::CREATED, Json(workflow_json(&row)))),
        Ok(None) => Ok(missing()),
        Err(e) if is_duplicate_workflow_name(&e) => {
            Ok(bad("Workflow with this name already exists"))
        }
        Err(e) => Err(e.into()),
    }
}

/// GET `/api/workspaces/:slug/workflows/:workflow_id/`
pub async fn retrieve_workflow(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    let row: Option<WorkflowRow> = sqlx::query_as(
        "SELECT w.id, w.name, w.description, w.is_active, w.workspace_id, w.created_at, w.updated_at \
         FROM workflows w JOIN workspaces ws ON ws.id = w.workspace_id \
         WHERE w.id = $1 AND ws.slug = $2 AND ws.deleted_at IS NULL AND w.deleted_at IS NULL",
    )
    .bind(workflow_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(row) => Ok((StatusCode::OK, Json(workflow_json(&row)))),
        None => Ok(missing()),
    }
}

/// PATCH `/api/workspaces/:slug/workflows/:workflow_id/`
pub async fn patch_workflow(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
    Json(body): Json<WorkflowBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let name = match body.name.as_deref() {
        Some(raw) => match validate_name(raw, "name") {
            Ok(name) => Some(name),
            Err(e) => return Ok(bad(&e)),
        },
        None => None,
    };
    let updated: Result<Option<WorkflowRow>, sqlx::Error> = sqlx::query_as(
        "UPDATE workflows SET name = COALESCE($3, name), description = COALESCE($4, description), \
         is_active = COALESCE($5, is_active), updated_by_id = $6, updated_at = now() \
         WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND deleted_at IS NULL \
         RETURNING id, name, description, is_active, workspace_id, created_at, updated_at",
    )
    .bind(workflow_id)
    .bind(&slug)
    .bind(&name)
    .bind(&body.description)
    .bind(body.is_active)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await;
    match updated {
        Ok(Some(row)) => Ok((StatusCode::OK, Json(workflow_json(&row)))),
        Ok(None) => Ok(missing()),
        Err(e) if is_duplicate_workflow_name(&e) => {
            Ok(bad("Workflow with this name already exists"))
        }
        Err(e) => Err(e.into()),
    }
}

/// DELETE `/api/workspaces/:slug/workflows/:workflow_id/`
pub async fn delete_workflow(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issue_types WHERE workflow_id = $1 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await?;
    if in_use {
        return Ok(bad("Workflow is in use by a work item type"));
    }
    sqlx::query(
        "UPDATE workflows SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// --- Workflow state CRUD ---------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkflowStateRow {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    pub slug: String,
    pub sequence: f64,
    pub group: String,
    pub is_default: bool,
}

pub fn workflow_state_json(row: &WorkflowStateRow) -> Value {
    json!({
        "id": row.id,
        "workflow_id": row.workflow_id,
        "name": row.name,
        "description": row.description,
        "color": row.color,
        "slug": row.slug,
        "sequence": row.sequence,
        "group": row.group,
        "is_default": row.is_default,
    })
}

const WF_STATE_COLS: &str =
    "id, workflow_id, name, description, color, slug, sequence, \"group\", is_default";

#[derive(Debug, Deserialize, Default)]
pub struct WorkflowStateBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub sequence: Option<f64>,
    #[serde(default)]
    pub is_default: Option<bool>,
}

fn default_color(group: &str) -> &'static str {
    match group {
        "started" => "#F59E0B",
        "completed" => "#46A758",
        "cancelled" => "#9AA4BC",
        _ => "#60646C",
    }
}

/// GET `/api/workspaces/:slug/workflows/:workflow_id/states/`
pub async fn list_states(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let rows: Vec<WorkflowStateRow> = sqlx::query_as(&format!(
        "SELECT {WF_STATE_COLS} FROM workflow_states \
         WHERE workflow_id = $1 AND deleted_at IS NULL ORDER BY sequence"
    ))
    .bind(workflow_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .iter()
            .map(workflow_state_json)
            .collect::<Vec<_>>())),
    ))
}

/// POST `/api/workspaces/:slug/workflows/:workflow_id/states/`
pub async fn create_state(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
    Json(body): Json<WorkflowStateBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let name = match validate_name(body.name.as_deref().unwrap_or(""), "name") {
        Ok(name) => name,
        Err(e) => return Ok(bad(&e)),
    };
    let group = body.group.clone().unwrap_or_else(|| "backlog".to_string());
    if let Err(e) = validate_state_group(&group) {
        return Ok(bad(&e));
    }
    let color = body
        .color
        .clone()
        .unwrap_or_else(|| default_color(&group).to_string());

    let mut tx = st.pool.begin().await?;
    let (default_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE is_default) FROM workflow_states \
         WHERE workflow_id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .fetch_one(&mut *tx)
    .await?;
    // Workflow wajib punya tepat satu default: state pertama selalu jadi
    // default, dan workflow yang drift tanpa default (mis. hasil import)
    // diheal ke state baru. `is_default: true` eksplisit menurunkan default
    // lama agar partial-unique (workflow) WHERE is_default tidak bentrok.
    let make_default = body.is_default.unwrap_or(default_count == 0) || default_count == 0;
    if make_default {
        sqlx::query(
            "UPDATE workflow_states SET is_default = false, updated_at = now() \
             WHERE workflow_id = $1 AND deleted_at IS NULL AND is_default = true",
        )
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
    }
    let inserted: Result<WorkflowStateRow, sqlx::Error> = sqlx::query_as(&format!(
        "INSERT INTO workflow_states (id, workflow_id, name, description, color, slug, sequence, \
         \"group\", is_default, created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, COALESCE($3, ''), $4, \
         trim(both '-' from regexp_replace(lower($2), '[^a-z0-9]+', '-', 'g')), \
         COALESCE($5, (SELECT COALESCE(MAX(sequence), 0) + 15000 FROM workflow_states WHERE workflow_id = $1)), \
         $6, $7, $8, $8, now(), now()) RETURNING {WF_STATE_COLS}"
    ))
    .bind(workflow_id)
    .bind(&name)
    .bind(&body.description)
    .bind(&color)
    .bind(body.sequence)
    .bind(&group)
    .bind(make_default)
    .bind(auth.0)
    .fetch_one(&mut *tx)
    .await;
    let row = match inserted {
        Ok(row) => row,
        Err(e) if is_duplicate_workflow_default(&e) => {
            tx.rollback().await?;
            return Ok(bad("Another state is already the default"));
        }
        Err(e) if is_duplicate_workflow_state(&e) => {
            tx.rollback().await?;
            return Ok(bad("Workflow state with this name already exists"));
        }
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;

    sync_workflow_to_projects(&st.pool, workflow_id).await?;
    Ok((StatusCode::CREATED, Json(workflow_state_json(&row))))
}

/// PATCH `/api/workspaces/:slug/workflows/:workflow_id/states/:state_id/`
pub async fn patch_state(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id, state_id)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<WorkflowStateBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let current: Option<WorkflowStateRow> = sqlx::query_as(&format!(
        "SELECT {WF_STATE_COLS} FROM workflow_states \
         WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL"
    ))
    .bind(state_id)
    .bind(workflow_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(current) = current else {
        return Ok(missing());
    };
    let name = match body.name.as_deref() {
        Some(raw) => match validate_name(raw, "name") {
            Ok(name) => Some(name),
            Err(e) => return Ok(bad(&e)),
        },
        None => None,
    };
    let group = body.group.clone().unwrap_or_else(|| current.group.clone());
    if let Err(e) = validate_state_group(&group) {
        return Ok(bad(&e));
    }
    if body.is_default == Some(false) && current.is_default {
        return Ok(bad("A workflow must have a default state"));
    }

    let mut tx = st.pool.begin().await?;
    if body.is_default == Some(true) && !current.is_default {
        sqlx::query(
            "UPDATE workflow_states SET is_default = false, updated_at = now() \
             WHERE workflow_id = $1 AND deleted_at IS NULL AND is_default = true",
        )
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
    }
    let updated: Result<Option<WorkflowStateRow>, sqlx::Error> = sqlx::query_as(&format!(
        "UPDATE workflow_states SET name = COALESCE($3, name), \
         slug = CASE WHEN $3::text IS NULL THEN slug \
         ELSE trim(both '-' from regexp_replace(lower($3), '[^a-z0-9]+', '-', 'g')) END, \
         description = COALESCE($4, description), \
         color = COALESCE($5, color), \"group\" = $6, sequence = COALESCE($7, sequence), \
         is_default = COALESCE($8, is_default), updated_by_id = $9, updated_at = now() \
         WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL RETURNING {WF_STATE_COLS}"
    ))
    .bind(state_id)
    .bind(workflow_id)
    .bind(&name)
    .bind(&body.description)
    .bind(&body.color)
    .bind(&group)
    .bind(body.sequence)
    .bind(body.is_default)
    .bind(auth.0)
    .fetch_optional(&mut *tx)
    .await;
    let row = match updated {
        Ok(Some(row)) => row,
        // Baris ter-soft-delete oleh request lain setelah SELECT awal → 404.
        Ok(None) => {
            tx.rollback().await?;
            return Ok(missing());
        }
        Err(e) if is_duplicate_workflow_default(&e) => {
            tx.rollback().await?;
            return Ok(bad("Another state is already the default"));
        }
        Err(e) if is_duplicate_workflow_state(&e) => {
            tx.rollback().await?;
            return Ok(bad("Workflow state with this name already exists"));
        }
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;

    sync_workflow_to_projects(&st.pool, workflow_id).await?;
    Ok((StatusCode::OK, Json(workflow_state_json(&row))))
}

/// DELETE `/api/workspaces/:slug/workflows/:workflow_id/states/:state_id/`
pub async fn delete_state(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id, state_id)): Path<(String, Uuid, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let mut tx = st.pool.begin().await?;
    // Lock baris state sampai commit agar promote bersamaan (patch is_default)
    // tidak bisa menyisipkan di antara cek default dan soft-delete.
    let locked: Option<(bool,)> = sqlx::query_as(
        "SELECT is_default FROM workflow_states \
         WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(state_id)
    .bind(workflow_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((is_default,)) = locked else {
        tx.rollback().await?;
        return Ok(missing());
    };
    if is_default {
        tx.rollback().await?;
        return Ok(bad(
            "Cannot delete the default workflow state; set another state as default first",
        ));
    }
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues i JOIN states s ON s.id = i.state_id \
         WHERE s.workflow_state_id = $1 AND i.deleted_at IS NULL)",
    )
    .bind(state_id)
    .fetch_one(&mut *tx)
    .await?;
    if in_use {
        tx.rollback().await?;
        return Ok(bad("Workflow state is in use by work items"));
    }
    sqlx::query(
        "UPDATE workflow_transitions SET deleted_at = now(), updated_at = now() \
         WHERE workflow_id = $1 AND (from_state_id = $2 OR to_state_id = $2) AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .bind(state_id)
    .execute(&mut *tx)
    .await?;
    // Guard kedua: kalau promote lolos dari lock (mis. jalur tulis lain),
    // rows_affected = 0 dan delete dibatalkan, bukan diam-diam menghapus default.
    let deleted = sqlx::query(
        "UPDATE workflow_states SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL AND is_default = false",
    )
    .bind(state_id)
    .bind(workflow_id)
    .execute(&mut *tx)
    .await?;
    if deleted.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(bad(
            "Cannot delete the default workflow state; set another state as default first",
        ));
    }
    tx.commit().await?;

    sync_workflow_to_projects(&st.pool, workflow_id).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// --- Workflow transition CRUD ----------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkflowTransitionRow {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub from_state_id: Uuid,
    pub to_state_id: Uuid,
}

pub fn transition_json(row: &WorkflowTransitionRow) -> Value {
    json!({
        "id": row.id,
        "workflow_id": row.workflow_id,
        "from_state_id": row.from_state_id,
        "to_state_id": row.to_state_id,
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct TransitionBody {
    #[serde(default)]
    pub from_state_id: Option<Uuid>,
    #[serde(default)]
    pub to_state_id: Option<Uuid>,
}

async fn state_in_workflow(
    pool: &sqlx::PgPool,
    workflow_id: Uuid,
    state_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let (ok,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM workflow_states WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL)",
    )
    .bind(state_id)
    .bind(workflow_id)
    .fetch_one(pool)
    .await?;
    Ok(ok)
}

/// GET `/api/workspaces/:slug/workflows/:workflow_id/transitions/`
pub async fn list_transitions(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let rows: Vec<WorkflowTransitionRow> = sqlx::query_as(
        "SELECT id, workflow_id, from_state_id, to_state_id FROM workflow_transitions \
         WHERE workflow_id = $1 AND deleted_at IS NULL ORDER BY created_at, id",
    )
    .bind(workflow_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(transition_json).collect::<Vec<_>>())),
    ))
}

/// POST `/api/workspaces/:slug/workflows/:workflow_id/transitions/`
pub async fn create_transition(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
    Json(body): Json<TransitionBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let (Some(from_state_id), Some(to_state_id)) = (body.from_state_id, body.to_state_id) else {
        return Ok(bad("from_state_id and to_state_id are required"));
    };
    if from_state_id == to_state_id {
        return Ok(bad("A transition cannot start and end on the same state"));
    }
    if !state_in_workflow(&st.pool, workflow_id, from_state_id).await?
        || !state_in_workflow(&st.pool, workflow_id, to_state_id).await?
    {
        return Ok(bad("States must belong to this workflow"));
    }
    let inserted: Result<WorkflowTransitionRow, sqlx::Error> = sqlx::query_as(
        "INSERT INTO workflow_transitions (id, workflow_id, from_state_id, to_state_id, \
         created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $4, now(), now()) \
         RETURNING id, workflow_id, from_state_id, to_state_id",
    )
    .bind(workflow_id)
    .bind(from_state_id)
    .bind(to_state_id)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await;
    match inserted {
        Ok(row) => Ok((StatusCode::CREATED, Json(transition_json(&row)))),
        Err(e) if is_duplicate_transition(&e) => Ok(bad("This transition already exists")),
        Err(e) => Err(e.into()),
    }
}

/// DELETE `/api/workspaces/:slug/workflows/:workflow_id/transitions/:transition_id/`
pub async fn delete_transition(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id, transition_id)): Path<(String, Uuid, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let rows = sqlx::query(
        "UPDATE workflow_transitions SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL",
    )
    .bind(transition_id)
    .bind(workflow_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if rows == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// --- Project enablement + workflow map -------------------------------------

/// Soft-delete link + state mirror satu type dari project (satu transaksi).
/// Dipakai un-enable (`unlink_type`) dan project-scope v1 DELETE.
pub(crate) async fn detach_type_from_project(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE project_issue_types SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND issue_type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE states SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// DELETE `/api/workspaces/:slug/projects/:project_id/work-item-types/:pk/`
/// (un-enable type dari project).
pub async fn unlink_type(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, type_id)): Path<(String, Uuid, Uuid)>,
) -> R {
    if !super::v1::work_item_type::can_write(&st.pool, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    // Project wajib milik workspace di slug; kalau tidak, jangan sentuh data
    // workspace lain (404).
    let (project_ok,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL AND w.deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    if !project_ok {
        return Ok(missing());
    }
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await?;
    if in_use {
        return Ok(bad("Type is in use by work items"));
    }
    detach_type_from_project(&st.pool, project_id, type_id).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// GET `/api/workspaces/:slug/projects/:project_id/workflow-map/`
pub async fn workflow_map(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> R {
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(role.is_some(), role.is_some(), ws_admin) {
        return Ok(deny());
    }
    ensure_project_workflows(&st.pool, project_id).await?;

    let types: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT t.id, t.workflow_id, t.name FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         JOIN projects p ON p.id = pit.project_id \
         WHERE pit.project_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
           AND t.workflow_id IS NOT NULL AND p.deleted_at IS NULL \
         ORDER BY t.name",
    )
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;

    let mut out = Vec::new();
    for (type_id, workflow_id, type_name) in types {
        let states: Vec<(Uuid, String, String, String, f64, bool)> = sqlx::query_as(
            "SELECT id, name, color, \"group\", sequence, \"default\" FROM states \
             WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL ORDER BY sequence, id",
        )
        .bind(project_id)
        .bind(type_id)
        .fetch_all(&st.pool)
        .await?;
        let default_state_id = states.iter().find(|s| s.5).map(|s| s.0);
        let transitions: Vec<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT mf.id, mt.id FROM workflow_transitions tr \
             JOIN states mf ON mf.workflow_state_id = tr.from_state_id AND mf.project_id = $1 \
               AND mf.deleted_at IS NULL AND mf.type_id = $3 \
             JOIN states mt ON mt.workflow_state_id = tr.to_state_id AND mt.project_id = $1 \
               AND mt.deleted_at IS NULL AND mt.type_id = $3 \
             WHERE tr.workflow_id = $2 AND tr.deleted_at IS NULL \
             ORDER BY tr.created_at, tr.id",
        )
        .bind(project_id)
        .bind(workflow_id)
        .bind(type_id)
        .fetch_all(&st.pool)
        .await?;
        out.push(json!({
            "type_id": type_id,
            "type_name": type_name,
            "workflow_id": workflow_id,
            "default_state_id": default_state_id,
            "states": states.iter().map(|(id, name, color, group, sequence, is_default)| json!({
                "id": id, "name": name, "color": color, "group": group,
                "sequence": sequence, "is_default": is_default,
            })).collect::<Vec<_>>(),
            "transitions": transitions.iter().map(|(from, to)| json!({
                "from_state_id": from, "to_state_id": to,
            })).collect::<Vec<_>>(),
        }));
    }
    Ok((StatusCode::OK, Json(json!({ "types": out }))))
}
