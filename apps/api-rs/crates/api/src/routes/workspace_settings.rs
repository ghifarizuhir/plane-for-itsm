//! Workspace-level feature settings (`workspace_settings`, program P1/S2).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::routes::{
    project::{deny, missing, ws_role},
    v1::work_item_type::ws_id,
};
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

const SETTINGS_COLS: &str = "id, workspace_id, cycle_view, module_view, issue_views_view, \
    page_view, intake_view, is_issue_type_enabled, is_time_tracking_enabled, \
    guest_view_all_features, archive_in, close_in, default_assignee_id, updated_at";

#[derive(Debug, Clone, FromRow)]
pub struct WorkspaceSettingsRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub cycle_view: bool,
    pub module_view: bool,
    pub issue_views_view: bool,
    pub page_view: bool,
    pub intake_view: bool,
    pub is_issue_type_enabled: bool,
    pub is_time_tracking_enabled: bool,
    pub guest_view_all_features: bool,
    pub archive_in: i32,
    pub close_in: i32,
    pub default_assignee_id: Option<Uuid>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateWorkspaceSettings {
    // NOTE: absent and explicit-null are conflated (COALESCE keeps the current
    // value), so `default_assignee_id` cannot be cleared back to NULL via this
    // endpoint — set it to another member instead. Revisit in S8 if needed.
    #[serde(default)]
    pub cycle_view: Option<bool>,
    #[serde(default)]
    pub module_view: Option<bool>,
    #[serde(default)]
    pub issue_views_view: Option<bool>,
    #[serde(default)]
    pub page_view: Option<bool>,
    #[serde(default)]
    pub intake_view: Option<bool>,
    #[serde(default)]
    pub is_issue_type_enabled: Option<bool>,
    #[serde(default)]
    pub is_time_tracking_enabled: Option<bool>,
    #[serde(default)]
    pub guest_view_all_features: Option<bool>,
    #[serde(default)]
    pub archive_in: Option<i32>,
    #[serde(default)]
    pub close_in: Option<i32>,
    #[serde(default)]
    pub default_assignee_id: Option<Uuid>,
}

fn settings_json(slug: &str, row: &WorkspaceSettingsRow) -> Value {
    json!({
        "id": row.id,
        "workspace": slug,
        "cycle_view": row.cycle_view,
        "module_view": row.module_view,
        "issue_views_view": row.issue_views_view,
        "page_view": row.page_view,
        "intake_view": row.intake_view,
        "is_issue_type_enabled": row.is_issue_type_enabled,
        "is_time_tracking_enabled": row.is_time_tracking_enabled,
        "guest_view_all_features": row.guest_view_all_features,
        "archive_in": row.archive_in,
        "close_in": row.close_in,
        "default_assignee_id": row.default_assignee_id,
        "updated_at": row.updated_at,
    })
}

/// Idempotent union recompute for one workspace (same semantics as migration
/// 0021 backfill). Backs GET auto-create and later domain-phase cutovers
/// (S6/S7/S8 call this to absorb interim project-level flag edits).
pub(crate) async fn ensure_settings(
    pool: &sqlx::PgPool,
    ws: Uuid,
) -> Result<(), common::errors::AppError> {
    sqlx::query(
        "INSERT INTO workspace_settings (id, workspace_id, cycle_view, module_view, \
         issue_views_view, page_view, intake_view, is_issue_type_enabled, \
         is_time_tracking_enabled, guest_view_all_features, archive_in, close_in, \
         default_assignee_id) \
         SELECT gen_random_uuid(), w.id, \
            COALESCE(BOOL_OR(p.cycle_view), true), \
            COALESCE(BOOL_OR(p.module_view), true), \
            COALESCE(BOOL_OR(p.issue_views_view), true), \
            COALESCE(BOOL_OR(p.page_view), true), \
            COALESCE(BOOL_OR(p.intake_view), false), \
            COALESCE(BOOL_OR(p.is_issue_type_enabled), false), \
            COALESCE(BOOL_OR(p.is_time_tracking_enabled), false), \
            COALESCE(BOOL_OR(p.guest_view_all_features), false), \
            COALESCE(mode_int.mode_val, 0), \
            COALESCE(mode_close.mode_val, 0), \
            mode_assignee.mode_val \
         FROM workspaces w \
         LEFT JOIN projects p ON p.workspace_id = w.id AND p.deleted_at IS NULL \
         LEFT JOIN LATERAL ( \
            SELECT p2.archive_in AS mode_val FROM projects p2 \
            WHERE p2.workspace_id = w.id AND p2.deleted_at IS NULL AND p2.archive_in IS NOT NULL \
            GROUP BY p2.archive_in \
            ORDER BY COUNT(*) DESC, \
                MAX((SELECT COUNT(*) FROM issues i3 WHERE i3.project_id = p2.id AND i3.deleted_at IS NULL)) DESC, \
                MIN(p2.created_at) \
            LIMIT 1 \
         ) mode_int ON true \
         LEFT JOIN LATERAL ( \
            SELECT p2.close_in AS mode_val FROM projects p2 \
            WHERE p2.workspace_id = w.id AND p2.deleted_at IS NULL AND p2.close_in IS NOT NULL \
            GROUP BY p2.close_in \
            ORDER BY COUNT(*) DESC, \
                MAX((SELECT COUNT(*) FROM issues i3 WHERE i3.project_id = p2.id AND i3.deleted_at IS NULL)) DESC, \
                MIN(p2.created_at) \
            LIMIT 1 \
         ) mode_close ON true \
         LEFT JOIN LATERAL ( \
            SELECT p2.default_assignee_id AS mode_val FROM projects p2 \
            WHERE p2.workspace_id = w.id AND p2.deleted_at IS NULL AND p2.default_assignee_id IS NOT NULL \
            GROUP BY p2.default_assignee_id \
            ORDER BY COUNT(*) DESC, \
                MAX((SELECT COUNT(*) FROM issues i3 WHERE i3.project_id = p2.id AND i3.deleted_at IS NULL)) DESC, \
                MIN(p2.created_at) \
            LIMIT 1 \
         ) mode_assignee ON true \
         WHERE w.id = $1 AND w.deleted_at IS NULL \
         GROUP BY w.id, mode_int.mode_val, mode_close.mode_val, mode_assignee.mode_val \
         ON CONFLICT (workspace_id) DO UPDATE SET \
            cycle_view = EXCLUDED.cycle_view, \
            module_view = EXCLUDED.module_view, \
            issue_views_view = EXCLUDED.issue_views_view, \
            page_view = EXCLUDED.page_view, \
            intake_view = EXCLUDED.intake_view, \
            is_issue_type_enabled = EXCLUDED.is_issue_type_enabled, \
            is_time_tracking_enabled = EXCLUDED.is_time_tracking_enabled, \
            guest_view_all_features = EXCLUDED.guest_view_all_features, \
            archive_in = EXCLUDED.archive_in, \
            close_in = EXCLUDED.close_in, \
            default_assignee_id = EXCLUDED.default_assignee_id, \
            updated_at = now()",
    )
    .bind(ws)
    .execute(pool)
    .await?;
    Ok(())
}

async fn fetch_settings(
    pool: &sqlx::PgPool,
    ws: Uuid,
) -> Result<Option<WorkspaceSettingsRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {SETTINGS_COLS} FROM workspace_settings \
         WHERE workspace_id = $1 AND deleted_at IS NULL"
    ))
    .bind(ws)
    .fetch_optional(pool)
    .await
}

pub async fn get_settings(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> R {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny());
    }
    let Some(ws) = ws_id(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    ensure_settings(&st.pool, ws).await?;
    let row = fetch_settings(&st.pool, ws).await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(settings_json(&slug, &r)))),
        None => Ok(missing()),
    }
}

pub async fn update_settings(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<UpdateWorkspaceSettings>,
) -> R {
    let Some(role) = ws_role(&st.pool, auth.0, &slug).await? else {
        return Ok(deny());
    };
    if role < 20 {
        return Ok(deny());
    }
    let Some(ws) = ws_id(&st.pool, &slug).await? else {
        return Ok(missing());
    };

    if let Some(archive_in) = body.archive_in {
        if archive_in < 0 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "archive_in must be >= 0"})),
            ));
        }
    }
    if let Some(close_in) = body.close_in {
        if close_in < 0 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "close_in must be >= 0"})),
            ));
        }
    }
    if let Some(assignee) = body.default_assignee_id {
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspace_members WHERE workspace_id = $1 \
             AND member_id = $2 AND is_active = true AND deleted_at IS NULL)",
        )
        .bind(ws)
        .bind(assignee)
        .fetch_one(&st.pool)
        .await?;
        if !active {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "default_assignee must be an active workspace member"})),
            ));
        }
    }

    ensure_settings(&st.pool, ws).await?;
    sqlx::query(
        "UPDATE workspace_settings SET \
         cycle_view = COALESCE($1, cycle_view), \
         module_view = COALESCE($2, module_view), \
         issue_views_view = COALESCE($3, issue_views_view), \
         page_view = COALESCE($4, page_view), \
         intake_view = COALESCE($5, intake_view), \
         is_issue_type_enabled = COALESCE($6, is_issue_type_enabled), \
         is_time_tracking_enabled = COALESCE($7, is_time_tracking_enabled), \
         guest_view_all_features = COALESCE($8, guest_view_all_features), \
         archive_in = COALESCE($9, archive_in), \
         close_in = COALESCE($10, close_in), \
         default_assignee_id = COALESCE($11, default_assignee_id), \
         updated_by_id = $12, updated_at = now() WHERE workspace_id = $13",
    )
    .bind(body.cycle_view)
    .bind(body.module_view)
    .bind(body.issue_views_view)
    .bind(body.page_view)
    .bind(body.intake_view)
    .bind(body.is_issue_type_enabled)
    .bind(body.is_time_tracking_enabled)
    .bind(body.guest_view_all_features)
    .bind(body.archive_in)
    .bind(body.close_in)
    .bind(body.default_assignee_id)
    .bind(auth.0)
    .bind(ws)
    .execute(&st.pool)
    .await?;

    let row = fetch_settings(&st.pool, ws).await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(settings_json(&slug, &r)))),
        None => Ok(missing()),
    }
}
