//! Parity with Django `WorkspaceThemeViewSet`
//! (`plane/app/views/workspace/base.py:322-336`,
//! `plane/app/urls/workspace.py:122-131`): collection `GET list + POST
//! create`, detail `GET retrieve + PATCH partial_update + DELETE destroy`,
//! rows scoped `workspace__slug`. Serializer `WorkspaceThemeSerializer`
//! (`serializers/workspace.py:167-171`, `fields="__all__"`, read-only
//! `workspace, actor`).
//!
//! Live columns verified in `apps/api-rs/migrations/0001_initial.sql`
//! (`workspace_themes` table, `:2486-2497`): `created_at, updated_at, id,
//! name(300), colors(jsonb NOT NULL), actor_id(NOT NULL), created_by_id,
//! updated_by_id, workspace_id, deleted_at`. All audit columns exist —
//! none omitted. `Meta.ordering = ("-created_at",)`
//! (`db/models/workspace.py:307`) → list `ORDER BY created_at DESC`.
//!
//! Deviations (documented per task):
//! - The serialized key is `colors`, NOT `theme`: the task text says
//!   `theme`, but the model (`db/models/workspace.py:290`) and the
//!   migration (`0001_initial.sql:2491`) both name the JSON column
//!   `colors`. Rows serialize `colors`; create/patch accept `colors`
//!   (plus a lenient `theme` alias on input only).
//! - The gate mirrors Django `WorkSpaceAdminPermission`
//!   (`permissions/workspace.py:61-71`): active workspace Admin (20) +
//!   Member (15) pass; GUEST (5) / non-members deny. DRF
//!   permission-class denials render 403 `{"detail": ...}` — denials
//!   here use the repo `deny()` (`{"error": ...}`) per repo precedent.
//! - Create stamps `created_by_id` AND `updated_by_id` = request user per
//!   task; Django `BaseModel.save` (`db/models/base.py:36-42`) leaves
//!   `updated_by` NULL on create.
//! - DELETE is a soft-delete (`deleted_at = now()`), matching Django's
//!   default soft `destroy` (`db/mixins.py:72-78`, `soft=True`).
//! - Duplicate `(workspace, name)` → 400
//!   `{"error": "The payload is not valid"}` via the `IntegrityError`
//!   branch (`views/base.py:80-84`).
//! - Miss (unknown slug on create / unknown pk) → 404 `missing()`
//!   (`views/base.py:92-96`); datetimes serialize RFC3339 (chrono) vs
//!   DRF ISO (same instants).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Map, Value};

use crate::routes::project::{
    deny, is_integrity_error, missing, ws_role, FORBIDDEN_MSG, INVALID_PAYLOAD_MSG,
};
use crate::{middleware::auth::AuthUser, state::AppState};

/// Table-qualified `workspace_themes` columns for joined selects (every
/// live column — `__all__` parity).
const THEME_COLS: &str = "t.id, t.created_at, t.updated_at, t.deleted_at, t.name, \
    t.colors, t.actor_id, t.created_by_id, t.updated_by_id, t.workspace_id";
/// Same columns unqualified for `INSERT/UPDATE ... RETURNING`.
const THEME_COLS_BARE: &str = "id, created_at, updated_at, deleted_at, name, \
    colors, actor_id, created_by_id, updated_by_id, workspace_id";

/// Gate for ALL five handlers: mirrors Django `WorkSpaceAdminPermission`
/// (`permissions/workspace.py:61-71`) — active workspace Admin (20) +
/// Member (15) pass; GUEST (5) / non-members deny.
pub(crate) fn theme_gate(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// One `WorkspaceThemeSerializer` row (`serializers/workspace.py:167-171`,
/// `__all__`): every model column, FKs as id strings (DRF default PK
/// representation). Field names match the SELECT aliases.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ThemeRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) name: String,
    pub(crate) colors: Value,
    pub(crate) actor_id: uuid::Uuid,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
    pub(crate) workspace_id: uuid::Uuid,
}

pub(crate) fn theme_json(row: &ThemeRow) -> Value {
    json!({
        "id": row.id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "deleted_at": row.deleted_at,
        "name": row.name,
        "colors": row.colors,
        "actor": row.actor_id,
        "created_by": row.created_by_id,
        "updated_by": row.updated_by_id,
        "workspace": row.workspace_id,
    })
}

fn field_errors(errors: Map<String, Value>) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(Value::Object(errors)))
}

fn is_db_integrity(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| is_integrity_error(c.as_ref()))
        .unwrap_or(false)
}

/// DRF `CharField` messages for `name` (`blank=False`, `max_length=300`).
fn check_name(value: &Value, errors: &mut Map<String, Value>) -> Option<String> {
    match value {
        Value::String(s) => {
            if s.is_empty() {
                errors.insert("name".to_string(), json!(["This field may not be blank."]));
                None
            } else if s.chars().count() > 300 {
                errors.insert(
                    "name".to_string(),
                    json!(["Ensure this field has no more than 300 characters."]),
                );
                None
            } else {
                Some(s.clone())
            }
        }
        Value::Null => {
            errors.insert("name".to_string(), json!(["This field may not be null."]));
            None
        }
        _ => {
            errors.insert("name".to_string(), json!(["Not a valid string."]));
            None
        }
    }
}

/// `colors` is optional on create (model `default=dict` → `{}`); an
/// explicit null is rejected like DRF (`"This field may not be null."`).
/// The task-text `theme` key is accepted as a fallback alias.
fn check_colors(body: &Value, errors: &mut Map<String, Value>) -> Option<Value> {
    match body.get("colors").or_else(|| body.get("theme")) {
        None => Some(json!({})),
        Some(Value::Null) => {
            errors.insert("colors".to_string(), json!(["This field may not be null."]));
            None
        }
        Some(v) => Some(v.clone()),
    }
}

fn check_colors_opt(body: &Value, errors: &mut Map<String, Value>) -> Option<Value> {
    match body.get("colors").or_else(|| body.get("theme")) {
        None => None,
        Some(Value::Null) => {
            errors.insert("colors".to_string(), json!(["This field may not be null."]));
            None
        }
        Some(v) => Some(v.clone()),
    }
}

/// GET `/api/workspaces/:slug/workspace-themes/` — parity with Django
/// `WorkspaceThemeViewSet.list` (`base.py:327-328`): 200 array scoped
/// `workspace__slug`, `ORDER BY -created_at`. Gate ADMIN (20) + MEMBER (15).
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if theme_gate(role).is_err() {
        return Ok(deny());
    }
    let rows: Vec<ThemeRow> = sqlx::query_as(&format!(
        "SELECT {THEME_COLS} FROM workspace_themes t \
         JOIN workspaces w ON w.id = t.workspace_id \
         WHERE w.slug = $1 AND t.deleted_at IS NULL ORDER BY t.created_at DESC"
    ))
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(theme_json).collect())),
    ))
}

/// POST `/api/workspaces/:slug/workspace-themes/` — parity with Django
/// `WorkspaceThemeViewSet.create` (`base.py:330-336`): 201 serializer
/// row; invalid → 400. Stamps `workspace_id` (from slug), `actor_id` =
/// request user, `created_by_id`/`updated_by_id` = request user (per
/// task; see module docs). Unknown slug → 404 `missing()` (Django
/// `Workspace.objects.get` → `DoesNotExist`, `views/base.py:92-96`).
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if theme_gate(role).is_err() {
        return Ok(deny());
    }
    let mut errors = Map::new();
    let name = match body.get("name") {
        None => {
            errors.insert("name".to_string(), json!(["This field is required."]));
            None
        }
        Some(v) => check_name(v, &mut errors),
    };
    let colors = check_colors(&body, &mut errors);
    if !errors.is_empty() {
        return Ok(field_errors(errors));
    }
    let (name, colors) = (name.unwrap(), colors.unwrap());
    let ws: Option<(uuid::Uuid,)> =
        sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(&st.pool)
            .await?;
    let Some((ws_id,)) = ws else {
        return Ok(missing());
    };
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_themes \
         WHERE workspace_id = $1 AND name = $2 AND deleted_at IS NULL)",
    )
    .bind(ws_id)
    .bind(&name)
    .fetch_one(&st.pool)
    .await?;
    if taken {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_PAYLOAD_MSG})),
        ));
    }
    let row = sqlx::query_as::<_, ThemeRow>(&format!(
        "INSERT INTO workspace_themes (id, created_at, updated_at, deleted_at, name, \
         colors, actor_id, created_by_id, updated_by_id, workspace_id) \
         VALUES (gen_random_uuid(), now(), now(), NULL, $1, $2, $3, $3, $3, $4) \
         RETURNING {THEME_COLS_BARE}"
    ))
    .bind(&name)
    .bind(&colors)
    .bind(auth.0)
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await;
    match row {
        Ok(r) => Ok((StatusCode::CREATED, Json(theme_json(&r)))),
        Err(e) if is_db_integrity(&e) => Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_PAYLOAD_MSG})),
        )),
        Err(e) => Err(e.into()),
    }
}

/// GET `/api/workspaces/:slug/workspace-themes/:pk/` — parity with
/// Django `retrieve`: 200 row; miss → 404 `missing()`.
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if theme_gate(role).is_err() {
        return Ok(deny());
    }
    let row: Option<ThemeRow> = sqlx::query_as(&format!(
        "SELECT {THEME_COLS} FROM workspace_themes t \
         JOIN workspaces w ON w.id = t.workspace_id \
         WHERE w.slug = $1 AND t.id = $2 AND t.deleted_at IS NULL"
    ))
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Ok(missing());
    };
    Ok((StatusCode::OK, Json(theme_json(&row))))
}

/// PATCH `/api/workspaces/:slug/workspace-themes/:pk/` — parity with
/// Django `partial_update`: 200 row; miss → 404 `missing()`; duplicate
/// sibling name → 400 invalid-payload (IntegrityError branch).
pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if theme_gate(role).is_err() {
        return Ok(deny());
    }
    let current: Option<ThemeRow> = sqlx::query_as(&format!(
        "SELECT {THEME_COLS} FROM workspace_themes t \
         JOIN workspaces w ON w.id = t.workspace_id \
         WHERE w.slug = $1 AND t.id = $2 AND t.deleted_at IS NULL"
    ))
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(current) = current else {
        return Ok(missing());
    };
    let mut errors = Map::new();
    let name = body
        .get("name")
        .map(|v| check_name(v, &mut errors))
        .unwrap_or(None);
    let colors = check_colors_opt(&body, &mut errors);
    if !errors.is_empty() {
        return Ok(field_errors(errors));
    }
    if let Some(ref n) = name {
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspace_themes \
             WHERE workspace_id = $1 AND name = $2 AND id != $3 AND deleted_at IS NULL)",
        )
        .bind(current.workspace_id)
        .bind(n)
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
        if taken {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": INVALID_PAYLOAD_MSG})),
            ));
        }
    }
    let new_name = name.unwrap_or(current.name);
    let new_colors = colors.unwrap_or(current.colors);
    let row = sqlx::query_as::<_, ThemeRow>(&format!(
        "UPDATE workspace_themes SET name = $1, colors = $2, updated_by_id = $3, \
         updated_at = now() WHERE id = $4 AND deleted_at IS NULL \
         RETURNING {THEME_COLS_BARE}"
    ))
    .bind(&new_name)
    .bind(&new_colors)
    .bind(auth.0)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await;
    match row {
        Ok(Some(r)) => Ok((StatusCode::OK, Json(theme_json(&r)))),
        Ok(None) => Ok(missing()),
        Err(e) if is_db_integrity(&e) => Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_PAYLOAD_MSG})),
        )),
        Err(e) => Err(e.into()),
    }
}

/// DELETE `/api/workspaces/:slug/workspace-themes/:pk/` — 204 via
/// soft-delete (`deleted_at = now()`); miss → 404 `missing()`.
pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if theme_gate(role).is_err() {
        return Ok(deny());
    }
    let n = sqlx::query(
        "UPDATE workspace_themes SET deleted_at = now() WHERE id = $1 \
         AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) \
         AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_theme_gate_is_admin_member_only() {
        // WorkSpaceAdminPermission (`permissions/workspace.py:61-71`) =
        // workspace ADMIN (20) + MEMBER (15); GUEST (5) / non-member → deny.
        assert!(theme_gate(Some(20)).is_ok());
        assert!(theme_gate(Some(15)).is_ok());
        assert!(theme_gate(Some(5)).is_err());
        assert!(theme_gate(None).is_err());
    }
}
