use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing, ws_role},
    state::AppState,
};

use super::state::{state_order, state_serializer_json, StateFullRow};

/// Mirrors `plane/app/views/workspace/base.py:WorkSpaceViewSet` +Serializer
/// `plane/app/serializers/workspace.py:WorkSpaceSerializer`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkspace {
    pub name: String,
    pub slug: String,
}

/// Pure create-body validation with Django-verbatim messages
/// (`base.py:103-119` + serializer `validate_name`/`validate_slug`),
/// unit-testable with no DB. Used by `create` and `workspace_test.rs`.
pub fn validate_create(body: &CreateWorkspace) -> Result<(), String> {
    if body.name.is_empty() || body.slug.is_empty() {
        return Err("Both name and slug are required".to_string());
    }
    if body.name.chars().count() > 80 || body.slug.chars().count() > 48 {
        return Err("The maximum length for name is 80 and for slug is 48".to_string());
    }
    if contains_url(&body.name) {
        return Err("Name cannot contain a URL".to_string());
    }
    if !has_alphanumeric(&body.name) {
        return Err("Name must contain at least one letter or number".to_string());
    }
    if RESTRICTED_SLUGS.contains(&body.slug.as_str()) {
        return Err("Slug is not valid".to_string());
    }
    if !valid_slug_chars(&body.slug) {
        return Err("Slug can only contain letters, numbers, hyphens (-), and underscores (_)"
            .to_string());
    }
    Ok(())
}

/// Full `WorkSpaceSerializer` row (`serializers/workspace.py:43-86`::/// `__all__` with FKs as PKs, + annotated `total_members` + `logo_url`
/// property + requester `role`). `deleted_at` is omitted per repo batch
/// convention (no FE reader, consistent with every other full builder).
#[derive(Debug, Clone, sqlx::FromRow)]
struct WsFullRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    name: String,
    logo: Option<String>,
    logo_asset_id: Option<uuid::Uuid>,
    logo_asset: Option<String>,
    owner_id: uuid::Uuid,
    slug: String,
    organization_size: Option<String>,
    timezone: String,
    background_color: String,
    total_members: i64,
    role: i16,
}

const WS_FULL_COLS: &str = "w.id, w.created_at, w.updated_at, w.created_by_id, w.updated_by_id, \
    w.name, w.logo, w.logo_asset_id, fa.asset AS logo_asset, w.owner_id, w.slug, \
    w.organization_size, w.timezone, w.background_color, \
    (SELECT COUNT(*) FROM workspace_members m JOIN users u ON u.id = m.member_id \
     WHERE m.workspace_id = w.id AND m.is_active = true AND m.deleted_at IS NULL \
     AND u.is_bot = false) AS total_members, \
    wm.role AS role";

fn ws_full_json(r: &WsFullRow) -> Value {
    let logo_url = r.logo_asset.clone().or_else(|| r.logo.clone());
    serde_json::json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": r.created_by_id,
        "updated_by": r.updated_by_id,
        "name": r.name,
        "logo": r.logo,
        "logo_asset": r.logo_asset_id,
        "owner": r.owner_id,
        "slug": r.slug,
        "organization_size": r.organization_size,
        "timezone": r.timezone,
        "background_color": r.background_color,
        "total_members": r.total_members,
        "logo_url": logo_url,
        "role": r.role,
    })
}

async fn fetch_ws_full(
    pool: &sqlx::PgPool,
    slug: &str,
    user: uuid::Uuid,
) -> Result<Option<WsFullRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {WS_FULL_COLS} FROM workspaces w \
         JOIN workspace_members wm ON wm.workspace_id = w.id AND wm.member_id = $2 \
           AND wm.is_active = true AND wm.deleted_at IS NULL \
         LEFT JOIN file_assets fa ON fa.id = w.logo_asset_id \
         WHERE w.slug = $1 AND w.deleted_at IS NULL",
    ))
    .bind(slug)
    .bind(user)
    .fetch_optional(pool)
    .await
}

pub const RESTRICTED_SLUGS: &[&str] = &[
    "404", "accounts", "api", "create-workspace", "god-mode", "installations", "invitations",
    "onboarding", "profile", "spaces", "workspace-invitations", "password", "flags", "monitor",
    "monitoring", "ingest", "plane-pro", "plane-ultimate", "enterprise", "plane-enterprise",
    "disco", "silo", "chat", "calendar", "drive", "channels", "upgrade", "billing", "sign-in",
    "sign-up", "signin", "signup", "config", "live", "admin", "m", "import", "importers",
    "integrations", "integration", "configuration", "initiatives", "initiative", "workflow",
    "workflows", "epics", "epic", "story", "mobile", "dashboard", "desktop", "onload",
    "real-time", "one", "pages", "business", "pro", "settings", "license", "licenses",
    "instances", "instance",
];

fn has_alphanumeric(value: &str) -> bool {
    value.chars().any(|c| c.is_alphanumeric())
}

fn contains_url(value: &str) -> bool {
    if value.len() > 1000 {
        return false;
    }
    let lower = value.to_lowercase();
    if lower.contains("http://") || lower.contains("https://") || lower.contains("www.") {
        return true;
    }
    // domain-like token: something.tld (tld 2-6 alpha) or IPv4
    for token in lower.split_whitespace() {
        let t = token.trim_matches(|c: char| c.is_ascii_punctuation());
        if is_ipv4(t) {
            return true;
        }
        if let Some(dot) = t.rfind('.') {
            let tld = &t[dot + 1..];
            let name_part = &t[..dot];
            if (2..=6).contains(&tld.len())
                && tld.chars().all(|c| c.is_ascii_alphabetic())
                && !name_part.is_empty()
                && name_part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            {
                return true;
            }
        }
    }
    false
}

fn is_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u8>().is_ok())
}

fn valid_slug_chars(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// GET /api/workspaces/ — member-scoped, ordered by name.
/// Mirrors `get_queryset` + `filter_queryset` (`base.py:63-90`): active
/// membership filter, `total_members` annotation (bots excluded), member
/// `role`, `ORDER BY name`, `?search=` over name + `?owner=` filter.
/// Gate AMG (`base.py:152-154`); deny is the DRF permission-class 403.
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use crate::routes::member::deny_detail;
    // AMG at no particular workspace: any ACTIVE membership passes
    // (`base.py:152-154` over the member-scoped queryset).
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_members WHERE member_id = $1 \
         AND is_active = true AND deleted_at IS NULL)",
    )
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    if !member {
        return Ok(deny_detail());
    }
    let search = params.get("search").cloned().unwrap_or_default();
    let owner: Option<uuid::Uuid> = params.get("owner").and_then(|s| uuid::Uuid::parse_str(s).ok());
    let rows: Vec<WsFullRow> = sqlx::query_as(&format!(
        "SELECT {WS_FULL_COLS} FROM workspaces w \
         JOIN workspace_members wm ON wm.workspace_id = w.id AND wm.member_id = $1 \
           AND wm.is_active = true AND wm.deleted_at IS NULL \
         LEFT JOIN file_assets fa ON fa.id = w.logo_asset_id \
         WHERE w.deleted_at IS NULL \
         AND ($2 = '' OR w.name ILIKE '%' || $2 || '%') \
         AND ($3::uuid IS NULL OR w.owner_id = $3) \
         ORDER BY w.name ASC",
    ))
    .bind(auth.0)
    .bind(&search)
    .bind(owner)
    .fetch_all(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(Value::Array(rows.iter().map(ws_full_json).collect()))))
}

/// POST /api/workspaces/ — mirrors `WorkSpaceViewSet.create`
/// (`base.py:87-150`): 403 `{"error": "Workspace creation is not allowed"}`
/// when `DISABLE_WORKSPACE_CREATION=1` (env, or instance-config when
/// `SKIP_ENV_VAR` — the DB path is not mirrored); 400 verbatims for
/// missing name/slug, over-length, and URL-in-name; serializer
/// name/slug validators; 201 full row + `total_members` + `role=20`;
/// slug conflict → 409 `{"slug": "The workspace with the slug already
/// exists"}`. Creator becomes ADMIN member (`company_role` passthrough);
/// `workspace_seed` celery skipped.
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if std::env::var("DISABLE_WORKSPACE_CREATION").unwrap_or_else(|_| "0".to_string()) == "1" {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Workspace creation is not allowed"})),
        ));
    }
    let name = body.get("name").and_then(Value::as_str).unwrap_or("");
    let slug = body.get("slug").and_then(Value::as_str).unwrap_or("");
    if let Err(e) = validate_create(&CreateWorkspace { name: name.to_string(), slug: slug.to_string() }) {
        return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
    }
    let owner = auth.0;
    let color = format!("#{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
    let company_role = body.get("company_role").and_then(Value::as_str).unwrap_or("");
    let mut tx = st.pool.begin().await.map_err(|e| {
        tracing::warn!(error = %e, "ws-create: begin transaction failed");
        common::errors::AppError(anyhow::anyhow!("internal error"))
    })?;
    let ws_id: Result<Option<(uuid::Uuid,)>, sqlx::Error> = sqlx::query_as(
        "INSERT INTO workspaces (id, name, slug, owner_id, timezone, background_color, created_by_id, updated_by_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 'UTC', $4, $3, $3, now(), now()) RETURNING id",
    )
    .bind(name)
    .bind(slug)
    .bind(owner)
    .bind(&color)
    .fetch_optional(&mut *tx)
    .await;
    let ws_id = match ws_id {
        Ok(v) => v,
        Err(e) => {
            tx.rollback().await.ok();
            // `base.py:143-149`: unique-violation on slug → 409.
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate key") {
                return Ok((
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"slug": "The workspace with the slug already exists"})),
                ));
            }
            tracing::warn!(error = %e, "ws-create: workspace insert failed");
            return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
        }
    };
    let Some((ws_id,)) = ws_id else {
        tx.rollback().await.ok();
        return Ok(missing());
    };
    // `view_props`/`default_props` mirror Django `get_default_props()`,
    // `issue_props` mirrors `get_issue_props()`.
    let view_props = serde_json::json!({
        "filters": {
            "priority": null, "state": null, "state_group": null,
            "assignees": null, "created_by": null, "labels": null,
            "start_date": null, "target_date": null, "subscriber": null,
        },
        "display_filters": {
            "group_by": null, "order_by": "-created_at", "type": null,
            "sub_issue": true, "show_empty_groups": true,
            "layout": "list", "calendar_date_range": "",
        },
        "display_properties": {
            "assignee": true, "attachment_count": true, "created_on": true,
            "due_date": true, "estimate": true, "key": true, "labels": true,
            "link": true, "priority": true, "start_date": true, "state": true,
            "sub_issue_count": true, "updated_on": true,
        },
    });
    let issue_props = serde_json::json!({"subscribed": true, "assigned": true, "created": true, "all_issues": true});
    if sqlx::query(
        "INSERT INTO workspace_members \
         (id, workspace_id, member_id, role, company_role, created_by_id, view_props, \
          default_props, issue_props, is_active, getting_started_checklist, \
          tips, explored_features, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, 20, $3, $2, $4, $4, $5, true, '{}', '{}', '{}', now(), now())",
    )
    .bind(ws_id)
    .bind(owner)
    .bind(company_role)
    .bind(&view_props)
    .bind(&issue_props)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        tracing::warn!("ws-create: member insert failed");
        tx.rollback().await.ok();
        return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
    }
    if tx.commit().await.is_err() {
        tracing::warn!("ws-create: commit failed");
        return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
    }
    match fetch_ws_full(&st.pool, slug, owner).await? {
        Some(row) => Ok((StatusCode::CREATED, Json(ws_full_json(&row)))),
        None => Ok(missing()),
    }
}

/// Mirrors `plane/app/views/workspace/base.py:WorkSpaceViewSet`
/// retrieve / update / partial_update / destroy on the slug detail route.
/// Retrieve is member-scoped (`get_queryset`): non-member (or unknown slug)
/// → 404 `missing()` — NOT 403. PATCH/PUT/DELETE are ADMIN-only
/// (`partial_update`/`destroy` gates + `urls/workspace.py:58` put→update);
/// PUT shares the PATCH core (partial-tolerant superset, F3 precedent).
/// PATCH/PUT return the full row (`serializer.data`); DELETE 204 with the
/// profile cleanup + Django's slug-rename on soft delete
/// (`models/workspace.py:delete`: `slug__epoch`, freeing the slug).
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    match fetch_ws_full(&st.pool, &slug, auth.0).await? {
        Some(r) => Ok((StatusCode::OK, Json(ws_full_json(&r)))),
        None => Ok(missing()),
    }
}

fn clean_ws_patch(body: &Value) -> Result<(), (StatusCode, Json<Value>)> {
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        if name.trim().is_empty() || name.chars().count() > 80 || contains_url(name) {
            return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid name"}))));
        }
        if !has_alphanumeric(name) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Name must contain at least one letter or number"})),
            ));
        }
    }
    if let Some(slug) = body.get("slug").and_then(Value::as_str) {
        if RESTRICTED_SLUGS.contains(&slug) || !valid_slug_chars(slug) || slug.chars().count() > 48 {
            return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid slug"}))));
        }
    }
    Ok(())
}

async fn apply_ws_patch(
    st: &AppState,
    auth: AuthUser,
    slug: &str,
    body: &Value,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    // ADMIN-only (`base.py:156-158`).
    match ws_role(&st.pool, auth.0, slug).await? {
        Some(r) if r >= 20 => {}
        _ => return Ok(deny()),
    }
    if let Err(e) = clean_ws_patch(body) {
        return Ok(e);
    }
    let res = sqlx::query(
        "UPDATE workspaces w SET name = COALESCE($1, name), slug = COALESCE($2, slug), \
         logo = COALESCE($3, logo), organization_size = COALESCE($4, organization_size), \
         timezone = COALESCE($5, timezone), background_color = COALESCE($6, background_color), \
         updated_at = now(), updated_by_id = $7 WHERE w.slug = $8 AND w.deleted_at IS NULL",
    )
    .bind(body.get("name").and_then(Value::as_str))
    .bind(body.get("slug").and_then(Value::as_str))
    .bind(body.get("logo").and_then(Value::as_str))
    .bind(body.get("organization_size").and_then(Value::as_str))
    .bind(body.get("timezone").and_then(Value::as_str))
    .bind(body.get("background_color").and_then(Value::as_str))
    .bind(auth.0)
    .bind(slug)
    .execute(&st.pool)
    .await;
    match res {
        Ok(r) if r.rows_affected() == 0 => Ok(missing()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate key") {
                return Ok((
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"slug": "The workspace with the slug already exists"})),
                ));
            }
            Err(common::errors::AppError(anyhow::anyhow!(e)))
        }
        Ok(_) => {
            // A slug rename moves the lookup key: re-fetch by the NEW slug.
            let key = body.get("slug").and_then(Value::as_str).unwrap_or(slug);
            match fetch_ws_full(&st.pool, key, auth.0).await? {
                Some(r) => Ok((StatusCode::OK, Json(ws_full_json(&r)))),
                None => Ok(missing()),
            }
        }
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    apply_ws_patch(&st, auth, &slug, &body).await
}

pub async fn put_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    apply_ws_patch(&st, auth, &slug, &body).await
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    // ADMIN-only (`base.py:167-172`).
    match ws_role(&st.pool, auth.0, &slug).await? {
        Some(r) if r >= 20 => {}
        _ => return Ok(deny()),
    }
    // Mirrors `remove_last_workspace_ids_from_user_settings`: profiles
    // pointing at the workspace lose their last-workspace pointer.
    let row: Option<(uuid::Uuid,)> =
        sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
            .bind(&slug)
            .fetch_optional(&st.pool)
            .await?;
    let Some((workspace_id,)) = row else {
        return Ok(missing());
    };
    sqlx::query("UPDATE profiles SET last_workspace_id = NULL WHERE last_workspace_id = $1")
        .bind(workspace_id)
        .execute(&st.pool)
        .await?;
    // Django soft-deletes AND renames `slug → slug__epoch`, freeing the
    // slug for reuse (`models/workspace.py:delete`).
    sqlx::query(
        "UPDATE workspaces SET deleted_at = now(), \
         slug = slug || '__' || EXTRACT(EPOCH FROM now())::bigint::text WHERE id = $1",
    )
    .bind(workspace_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!(null))))
}

/// GET-safe branch of `WorkspaceEntityPermission`
/// (`plane/app/permissions/workspace.py:74-82`): any ACTIVE ws member
/// passes, incl. GUEST — NO role filter (differs from the unsafe branch,
/// which requires ADMIN/MEMBER). Non-member → 403 `deny()`.
pub(crate) fn guard_ws_states(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(_) => Ok(()),
        None => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// GET `/api/workspaces/:slug/states/` — parity with Django
/// `WorkspaceStatesEndpoint.get`
/// (`plane/app/views/workspace/state.py:17-...`,
/// `plane/app/urls/workspace.py:167-171`).
///
/// - Gate: `WorkspaceEntityPermission` on a safe (GET) method = any
///   ACTIVE ws member incl. GUEST (`permissions/workspace.py:74-82`),
///   via `ws_role` (checks `is_active` + soft-delete, same helper the
///   Batch C gates use).
/// - Scope mirrors the queryset exactly: `State.objects` (default
///   `StateManager` = soft-deletion + `exclude(group='triage')`,
///   `plane/db/models/state.py:65-69`) + `workspace__slug=slug` +
///   active `project_members` row for the caller +
///   `project__archived_at__isnull=True` + explicit `is_triage=False`.
/// - `order` mirrors the Python loop (`state.py:30-37`): per-`group`
///   1-based `index / count`, serialized via the shared
///   `state_serializer_json` (D3a reuses the same struct/fn with
///   `order=None`).
/// - Deviations: `ORDER BY sequence ASC` for determinism (Django sets no
///   ordering); datetimes unneeded (no datetime keys in the shape);
///   JSON key order follows repo batch convention while the KEY SET
///   matches `StateSerializer` exactly.
pub async fn ws_states(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_ws_states(role).is_err() {
        return Ok(deny());
    }
    let rows: Vec<StateFullRow> = sqlx::query_as(
        "SELECT s.id, s.project_id, s.workspace_id, s.name, s.color, s.\"group\", s.\"default\" AS is_default, s.description, s.sequence \
         FROM states s JOIN projects p ON p.id = s.project_id \
         WHERE s.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $1) \
         AND s.deleted_at IS NULL AND s.\"group\" != 'triage' AND s.is_triage = false \
         AND p.archived_at IS NULL \
         AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = s.project_id AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
         ORDER BY s.sequence ASC",
    )
    .bind(&slug)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    let mut totals: HashMap<&str, usize> = HashMap::new();
    for r in &rows {
        *totals.entry(r.group.as_str()).or_insert(0) += 1;
    }
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            let n = seen.entry(r.group.as_str()).or_insert(0);
            *n += 1;
            state_serializer_json(r, Some(state_order(*n, totals[r.group.as_str()])))
        })
        .collect();
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

#[cfg(test)]
mod batch_d_d3_tests {
    use super::*;

    #[test]
    fn ws_states_gate_allows_any_active_member() {
        // GET-safe branch of `WorkspaceEntityPermission`
        // (`plane/app/permissions/workspace.py:74-82`): any ACTIVE ws
        // member passes, incl. GUEST — no role filter. Non-member → 403.
        assert!(guard_ws_states(Some(20)).is_ok());
        assert!(guard_ws_states(Some(15)).is_ok());
        assert!(guard_ws_states(Some(5)).is_ok());
        assert!(guard_ws_states(None).is_err());
        assert_eq!(
            guard_ws_states(None).unwrap_err(),
            crate::routes::project::FORBIDDEN_MSG
        );
    }
}
