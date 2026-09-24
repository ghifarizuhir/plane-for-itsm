//! Rust-only AI scheduler endpoints (no Django counterpart).
//!
//! `GET/POST /api/workspaces/:slug/ai-schedules/` — list + confirm-created
//! schedules. Creates are idempotent per `(workspace_id, proposal_key)`;
//! reads are ADMIN/MEMBER.

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
struct ScheduleListRow {
    id: Uuid,
    name: String,
    prompt: String,
    frequency: String,
    time_of_day: String,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: String,
    enabled: bool,
    next_run_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    last_status: Option<String>,
    last_finished_at: Option<chrono::DateTime<chrono::Utc>>,
    last_run_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn schedule_list_json(row: &ScheduleListRow) -> Value {
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
        "last_status": row.last_status,
        "last_finished_at": row.last_finished_at,
        "last_run_at": row.last_run_at,
    })
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
    let rows: Vec<ScheduleListRow> = sqlx::query_as(
        "SELECT s.id, s.name, s.prompt, s.frequency, s.time_of_day, s.day_of_week, \
                s.day_of_month, s.timezone, s.enabled, s.next_run_at, s.created_by_id, \
                s.created_at, r.status AS last_status, r.finished_at AS last_finished_at, \
                r.created_at AS last_run_at \
         FROM ai_schedules s \
         LEFT JOIN LATERAL (SELECT status, finished_at, created_at FROM ai_schedule_runs \
                            WHERE schedule_id = s.id ORDER BY created_at DESC LIMIT 1) r ON true \
         WHERE s.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) \
           AND s.deleted_at IS NULL \
         ORDER BY s.created_at DESC, s.id",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .iter()
            .map(schedule_list_json)
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
