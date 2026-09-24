//! Rust-only AI scheduler endpoints (no Django counterpart).
//!
//! `GET/POST /api/workspaces/:slug/ai-schedules/` — list + confirm-created
//! schedules; `GET/PATCH/DELETE /:schedule_id/` — detail, pause/resume, soft
//! delete; `POST /:schedule_id/run/` — queue a manual run. Creates are
//! idempotent per `(workspace_id, proposal_key)`; reads are ADMIN/MEMBER,
//! mutations are creator-or-workspace-admin.

use std::collections::HashMap;

use ai::schedule::ScheduleProposal;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::routes::module::guard_am;
use crate::routes::project::{deny, missing, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

pub const MAX_SCHEDULES_PER_WORKSPACE: i64 = 20;

pub(crate) async fn workspace_id_for_slug(
    pool: &PgPool,
    slug: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

#[derive(serde::Deserialize)]
pub struct CreateScheduleBody {
    pub name: String,
    pub prompt: String,
    pub frequency: String,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub day_of_week: Option<i16>,
    #[serde(default)]
    pub day_of_month: Option<i16>,
    #[serde(default)]
    pub timezone: Option<String>,
    pub proposal_key: Uuid,
}

#[derive(sqlx::FromRow)]
struct ScheduleRow {
    id: Uuid,
    workspace_id: Uuid,
    created_by_id: Uuid,
    name: String,
    prompt: String,
    frequency: String,
    time_of_day: String,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: String,
    enabled: bool,
    next_run_at: chrono::DateTime<chrono::Utc>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct RunRow {
    schedule_id: Uuid,
    id: Uuid,
    status: String,
    trigger: String,
    prompt: String,
    response: Option<String>,
    response_html: Option<String>,
    error: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Shared run shape for the detail history and the list's last-run summary.
fn run_json(row: &RunRow) -> Value {
    json!({
        "id": row.id,
        "status": row.status,
        "trigger": row.trigger,
        "prompt": row.prompt,
        "response": row.response,
        "response_html": row.response_html,
        "error": row.error,
        "created_at": row.created_at,
        "started_at": row.started_at,
        "finished_at": row.finished_at,
    })
}

fn schedule_json(row: &ScheduleRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "prompt": row.prompt,
        "frequency": row.frequency,
        "time": row.time_of_day,
        "day_of_week": row.day_of_week,
        "day_of_month": row.day_of_month,
        "timezone": row.timezone,
        "enabled": row.enabled,
        "next_run_at": row.next_run_at,
        "created_by_id": row.created_by_id,
        "created_at": row.created_at,
    })
}

fn schedule_list_json(row: &ScheduleRow, last_run: Option<&RunRow>) -> Value {
    let mut value = schedule_json(row);
    value["last_status"] = json!(last_run.map(|run| run.status.clone()));
    value["last_finished_at"] = json!(last_run.and_then(|run| run.finished_at));
    value["last_run_at"] = json!(last_run.map(|run| run.created_at));
    value
}

/// Load one non-deleted schedule by workspace slug + id, scoped to the
/// workspace (unknown slug or schedule → `None`).
async fn load_schedule(
    pool: &PgPool,
    slug: &str,
    schedule_id: Uuid,
) -> Result<Option<ScheduleRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT s.id, s.workspace_id, s.created_by_id, s.name, s.prompt, s.frequency, \
                s.time_of_day, s.day_of_week, s.day_of_month, s.timezone, s.enabled, \
                s.next_run_at, s.created_at \
         FROM ai_schedules s \
         JOIN workspaces w ON w.id = s.workspace_id AND w.slug = $1 AND w.deleted_at IS NULL \
         WHERE s.id = $2 AND s.deleted_at IS NULL",
    )
    .bind(slug)
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
}

/// Existing schedules are managed by their creator or a workspace admin.
fn can_manage(created_by_id: Uuid, user_id: Uuid, ws_role: Option<i16>) -> bool {
    created_by_id == user_id || ws_role == Some(20)
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(workspace_id) = workspace_id_for_slug(&st.pool, &slug).await? else {
        return Ok((StatusCode::OK, Json(json!([]))));
    };
    let rows: Vec<ScheduleRow> = sqlx::query_as(
        "SELECT id, workspace_id, created_by_id, name, prompt, frequency, time_of_day, \
                day_of_week, day_of_month, timezone, enabled, next_run_at, created_at \
         FROM ai_schedules WHERE workspace_id = $1 AND deleted_at IS NULL \
         ORDER BY created_at DESC, id",
    )
    .bind(workspace_id)
    .fetch_all(&st.pool)
    .await?;
    let schedule_ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    let last_runs: Vec<RunRow> = if schedule_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT DISTINCT ON (schedule_id) schedule_id, id, status, trigger, prompt, response, \
                    response_html, error, created_at, started_at, finished_at \
             FROM ai_schedule_runs WHERE schedule_id = ANY($1) \
             ORDER BY schedule_id, created_at DESC, id",
        )
        .bind(&schedule_ids)
        .fetch_all(&st.pool)
        .await?
    };
    let last_by_schedule: HashMap<Uuid, &RunRow> =
        last_runs.iter().map(|run| (run.schedule_id, run)).collect();
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .iter()
            .map(|row| schedule_list_json(row, last_by_schedule.get(&row.id).copied()))
            .collect::<Vec<_>>())),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let body: CreateScheduleBody = match serde_json::from_value(payload) {
        Ok(body) => body,
        Err(err) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid schedule payload: {err}")})),
            ));
        }
    };
    let Some(workspace_id) = workspace_id_for_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    let proposal = match ScheduleProposal::new(
        &body.name,
        &body.prompt,
        &body.frequency,
        body.time.as_deref(),
        body.day_of_week,
        body.day_of_month,
        body.timezone.as_deref(),
    ) {
        Ok(proposal) => proposal,
        Err(message) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    };

    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM ai_schedules WHERE workspace_id = $1 AND proposal_key = $2 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .bind(body.proposal_key)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(existing) = existing {
        return Ok((
            StatusCode::OK,
            Json(json!({"id": existing, "already_exists": true})),
        ));
    }

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::int8 FROM ai_schedules WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_one(&st.pool)
    .await?;
    if count >= MAX_SCHEDULES_PER_WORKSPACE {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error": format!("at most {MAX_SCHEDULES_PER_WORKSPACE} schedules per workspace")}),
            ),
        ));
    }

    let next_run_at = proposal.next_occurrence(chrono::Utc::now());
    let id = Uuid::new_v4();
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, \
         time_of_day, day_of_week, day_of_month, timezone, enabled, next_run_at, proposal_key, \
         created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, true, $11, $12, now(), now()) \
         ON CONFLICT (workspace_id, proposal_key) WHERE deleted_at IS NULL DO NOTHING RETURNING id",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(auth.0)
    .bind(&proposal.name)
    .bind(&proposal.prompt)
    .bind(&proposal.frequency)
    .bind(&proposal.time)
    .bind(proposal.day_of_week)
    .bind(proposal.day_of_month)
    .bind(&proposal.timezone)
    .bind(next_run_at)
    .bind(body.proposal_key)
    .fetch_optional(&st.pool)
    .await?;

    match inserted {
        Some(id) => Ok((StatusCode::CREATED, Json(json!({"id": id})))),
        None => {
            let existing: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM ai_schedules WHERE workspace_id = $1 AND proposal_key = $2 AND deleted_at IS NULL",
            )
            .bind(workspace_id)
            .bind(body.proposal_key)
            .fetch_optional(&st.pool)
            .await?;
            match existing {
                Some(existing) => Ok((
                    StatusCode::OK,
                    Json(json!({"id": existing, "already_exists": true})),
                )),
                None => Err(common::errors::AppError(anyhow::anyhow!(
                    "schedule insert conflicted but no row was found"
                ))),
            }
        }
    }
}

/// `GET /api/workspaces/:slug/ai-schedules/:schedule_id/` — schedule + the
/// 20 most recent runs.
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, schedule_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(row) = load_schedule(&st.pool, &slug, schedule_id).await? else {
        return Ok(missing());
    };
    let runs: Vec<RunRow> = sqlx::query_as(
        "SELECT schedule_id, id, status, trigger, prompt, response, response_html, error, \
                created_at, started_at, finished_at \
         FROM ai_schedule_runs WHERE schedule_id = $1 \
         ORDER BY created_at DESC, id DESC LIMIT 20",
    )
    .bind(schedule_id)
    .fetch_all(&st.pool)
    .await?;
    let mut body = schedule_json(&row);
    body["runs"] = json!(runs.iter().map(run_json).collect::<Vec<_>>());
    Ok((StatusCode::OK, Json(body)))
}

/// `PATCH /api/workspaces/:slug/ai-schedules/:schedule_id/` — `{"enabled"}`.
/// Pausing keeps `next_run_at`; resuming recomputes it from the preset.
pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, schedule_id)): Path<(String, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(row) = load_schedule(&st.pool, &slug, schedule_id).await? else {
        return Ok(missing());
    };
    if !can_manage(row.created_by_id, auth.0, role) {
        return Ok(deny());
    }
    let Some(enabled) = body.get("enabled").and_then(Value::as_bool) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "enabled must be a boolean"})),
        ));
    };
    let next_run_at = if enabled {
        match ScheduleProposal::new(
            &row.name,
            &row.prompt,
            &row.frequency,
            Some(&row.time_of_day),
            row.day_of_week,
            row.day_of_month,
            Some(&row.timezone),
        ) {
            Ok(proposal) => Some(proposal.next_occurrence(chrono::Utc::now())),
            Err(message) => {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
            }
        }
    } else {
        None
    };
    let updated: Uuid = sqlx::query_scalar(
        "UPDATE ai_schedules SET enabled = $2, \
         next_run_at = COALESCE($3, next_run_at), updated_at = now() \
         WHERE id = $1 RETURNING id",
    )
    .bind(schedule_id)
    .bind(enabled)
    .bind(next_run_at)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!({"id": updated, "enabled": enabled})),
    ))
}

/// `DELETE /api/workspaces/:slug/ai-schedules/:schedule_id/` — soft delete;
/// the partial unique index frees the `proposal_key` for reuse.
pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, schedule_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(row) = load_schedule(&st.pool, &slug, schedule_id).await? else {
        return Ok(missing());
    };
    if !can_manage(row.created_by_id, auth.0, role) {
        return Ok(deny());
    }
    sqlx::query("UPDATE ai_schedules SET deleted_at = now(), updated_at = now() WHERE id = $1")
        .bind(schedule_id)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// `POST /api/workspaces/:slug/ai-schedules/:schedule_id/run/` — queue a
/// manual run (asynchronous; the worker consumes `ai.schedule.run`).
pub async fn run_now(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, schedule_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(row) = load_schedule(&st.pool, &slug, schedule_id).await? else {
        return Ok(missing());
    };
    if !can_manage(row.created_by_id, auth.0, role) {
        return Ok(deny());
    }
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'manual', $4, now())",
    )
    .bind(run_id)
    .bind(schedule_id)
    .bind(row.workspace_id)
    .bind(&row.prompt)
    .execute(&st.pool)
    .await?;
    let mut redis = st.redis_client().await?;
    if let Err(error) =
        common::stream::push_job(&mut redis, "ai.schedule.run", json!({"run_id": run_id})).await
    {
        tracing::error!(run_id=%run_id, error=%error, "ai.schedule.run: push failed");
        let _ = sqlx::query(
            "UPDATE ai_schedule_runs SET status = 'failed', error = 'could not queue run', finished_at = now() WHERE id = $1",
        )
        .bind(run_id)
        .execute(&st.pool)
        .await;
        return Err(common::errors::AppError::internal());
    }
    Ok((StatusCode::CREATED, Json(json!({"run_id": run_id}))))
}
