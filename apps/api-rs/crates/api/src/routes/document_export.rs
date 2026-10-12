//! Workspace document-export settings (`workspace_document_settings`).

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

const LOGO_MODES: [&str; 3] = ["none", "workspace", "custom"];
const ALIGNS: [&str; 3] = ["left", "center", "right"];
const MAX_TEXT_LINES: usize = 5;
const MAX_LINE_LEN: usize = 200;

const SETTINGS_COLS: &str = "id, header_logo_mode, header_logo_asset_id, header_text_lines, \
    footer_text_lines, show_page_numbers, show_generated_by, created_at, updated_at, \
    created_by_id, updated_by_id";

#[derive(Debug, Clone, FromRow)]
pub struct DocumentExportSettingsRow {
    pub id: Uuid,
    pub header_logo_mode: String,
    pub header_logo_asset_id: Option<Uuid>,
    pub header_text_lines: Value,
    pub footer_text_lines: Value,
    pub show_page_numbers: bool,
    pub show_generated_by: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateDocumentExportSettings {
    #[serde(default)]
    pub header_logo_mode: Option<String>,
    #[serde(default)]
    pub header_logo_asset_id: Option<Uuid>,
    #[serde(default)]
    pub header_text_lines: Option<Value>,
    #[serde(default)]
    pub footer_text_lines: Option<Value>,
    #[serde(default)]
    pub show_page_numbers: Option<bool>,
    #[serde(default)]
    pub show_generated_by: Option<bool>,
}

fn settings_json(row: Option<&DocumentExportSettingsRow>) -> Value {
    match row {
        Some(r) => json!({
            "id": r.id,
            "header_logo_mode": r.header_logo_mode,
            "header_logo_asset_id": r.header_logo_asset_id,
            "header_logo_url": r
                .header_logo_asset_id
                .map(|id| format!("/api/assets/v2/static/{id}/")),
            "header_text_lines": r.header_text_lines,
            "footer_text_lines": r.footer_text_lines,
            "show_page_numbers": r.show_page_numbers,
            "show_generated_by": r.show_generated_by,
            "created_at": r.created_at,
            "updated_at": r.updated_at,
        }),
        None => json!({
            "id": null,
            "header_logo_mode": "none",
            "header_logo_asset_id": null,
            "header_logo_url": null,
            "header_text_lines": [],
            "footer_text_lines": [],
            "show_page_numbers": true,
            "show_generated_by": true,
            "created_at": null,
            "updated_at": null,
        }),
    }
}

fn validate_text_lines(value: &Value, field: &str) -> Result<(), String> {
    let Some(lines) = value.as_array() else {
        return Err(format!("{field} must be an array"));
    };
    if lines.len() > MAX_TEXT_LINES {
        return Err(format!("{field} supports at most {MAX_TEXT_LINES} lines"));
    }
    for line in lines {
        let Some(text) = line.get("text").and_then(Value::as_str) else {
            return Err(format!("{field} line text is required"));
        };
        if text.chars().count() > MAX_LINE_LEN {
            return Err(format!(
                "{field} line text must be at most {MAX_LINE_LEN} characters"
            ));
        }
        if let Some(align) = line.get("align") {
            match align.as_str() {
                Some(a) if ALIGNS.contains(&a) => {}
                _ => return Err(format!("{field} line align must be left, center, or right")),
            }
        }
    }
    Ok(())
}

async fn fetch_settings(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
) -> Result<Option<DocumentExportSettingsRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {SETTINGS_COLS} FROM workspace_document_settings \
         WHERE workspace_id = $1 AND deleted_at IS NULL"
    ))
    .bind(workspace_id)
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
    let row = fetch_settings(&st.pool, ws).await?;
    Ok((StatusCode::OK, Json(settings_json(row.as_ref()))))
}

pub async fn update_settings(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<UpdateDocumentExportSettings>,
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

    if let Some(mode) = &body.header_logo_mode {
        if !LOGO_MODES.contains(&mode.as_str()) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "header_logo_mode must be none, workspace, or custom"})),
            ));
        }
    }
    if let Some(lines) = &body.header_text_lines {
        if let Err(message) = validate_text_lines(lines, "header_text_lines") {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    }
    if let Some(lines) = &body.footer_text_lines {
        if let Err(message) = validate_text_lines(lines, "footer_text_lines") {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    }
    if let Some(asset_id) = body.header_logo_asset_id {
        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM file_assets WHERE id = $1 AND workspace_id = $2 \
             AND entity_type = 'DOC_HEADER_LOGO' AND deleted_at IS NULL)",
        )
        .bind(asset_id)
        .bind(ws)
        .fetch_one(&st.pool)
        .await?;
        if !owned {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Invalid header logo asset"})),
            ));
        }
    }

    let current = fetch_settings(&st.pool, ws).await?;
    match current {
        Some(row) => {
            sqlx::query(
                "UPDATE workspace_document_settings SET \
                 header_logo_mode = COALESCE($1, header_logo_mode), \
                 header_logo_asset_id = COALESCE($2, header_logo_asset_id), \
                 header_text_lines = COALESCE($3, header_text_lines), \
                 footer_text_lines = COALESCE($4, footer_text_lines), \
                 show_page_numbers = COALESCE($5, show_page_numbers), \
                 show_generated_by = COALESCE($6, show_generated_by), \
                 updated_by_id = $7, updated_at = now() WHERE id = $8",
            )
            .bind(&body.header_logo_mode)
            .bind(body.header_logo_asset_id)
            .bind(&body.header_text_lines)
            .bind(&body.footer_text_lines)
            .bind(body.show_page_numbers)
            .bind(body.show_generated_by)
            .bind(auth.0)
            .bind(row.id)
            .execute(&st.pool)
            .await?;
        }
        None => {
            sqlx::query(
                "INSERT INTO workspace_document_settings (id, workspace_id, header_logo_mode, \
                 header_logo_asset_id, header_text_lines, footer_text_lines, show_page_numbers, \
                 show_generated_by, created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, COALESCE($2, 'none'), $3, COALESCE($4, '[]'::jsonb), \
                 COALESCE($5, '[]'::jsonb), COALESCE($6, true), COALESCE($7, true), $8, $8, now(), now())",
            )
            .bind(ws)
            .bind(&body.header_logo_mode)
            .bind(body.header_logo_asset_id)
            .bind(&body.header_text_lines)
            .bind(&body.footer_text_lines)
            .bind(body.show_page_numbers)
            .bind(body.show_generated_by)
            .bind(auth.0)
            .execute(&st.pool)
            .await?;
        }
    }

    let row = fetch_settings(&st.pool, ws).await?;
    Ok((StatusCode::OK, Json(settings_json(row.as_ref()))))
}
