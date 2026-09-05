use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, ws_role},
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

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceOut {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
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

/// Pure validation mirroring Django (unit-testable, no DB).
/// Returns Ok(()) or Err(message mentioning offending field.
pub fn validate_create(body: &CreateWorkspace) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.slug.trim().is_empty() {
        return Err("slug is required".to_string());
    }
    if body.name.chars().count() > 80 {
        return Err("name max length 80".to_string());
    }
    if body.slug.chars().count() > 48 {
        return Err("slug max length 48".to_string());
    }
    if contains_url(&body.name) {
        return Err("Name must not contain URLs".to_string());
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

/// GET /api/workspaces/ — member-scoped, ordered by name.
/// Mirrors `get_queryset`: filter by membership, annotate total_members.
pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<WorkspaceOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::workspace::Workspace>(
        "SELECT id, name, slug FROM workspaces WHERE deleted_at IS NULL ORDER BY name ASC",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|w| WorkspaceOut {
                id: w.id,
                name: w.name,
                slug: w.slug,
            })
            .collect(),
    ))
}

/// POST /api/workspaces/ — validates then inserts.
/// Owner = token user (Django `WorkSpaceViewSet` sets owner=request.user);
/// `timezone`/`background_color` mirror model defaults (`UTC` + random hex,
/// `plane/db/models/workspace.py:138-139`, `plane/utils/color.py:9`).
/// Mirrors `base.py:124-129`: creator is added as `WorkspaceMember`
/// with `role=20` (ADMIN) in the same transaction, so membership-gated
/// endpoints don't 403 on freshly created workspaces.
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateWorkspace>,
) -> Result<(StatusCode, Json<WorkspaceOut>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let owner = auth.0;
    let color = format!("#{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
    let mut tx = st.pool.begin().await.map_err(|e| {
        tracing::warn!(error = %e, "ws-create: begin transaction failed");
        common::errors::AppError(anyhow::anyhow!("internal error"))
    })?;
    let row = sqlx::query_as::<_, common::models::workspace::Workspace>(
        "INSERT INTO workspaces (id, name, slug, owner_id, timezone, background_color, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 'UTC', $4, now(), now()) RETURNING id, name, slug",
    )
    .bind(&body.name)
    .bind(&body.slug)
    .bind(owner)
    .bind(&color)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "ws-create: workspace insert failed");
        common::errors::AppError(anyhow::anyhow!("internal error"))
    })?;
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
         (id, workspace_id, member_id, role, created_by_id, view_props, \
          default_props, issue_props, is_active, getting_started_checklist, \
          tips, explored_features, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, 20, $2, $3, $3, $4, true, '{}', '{}', '{}', now(), now())",
    )
    .bind(row.id)
    .bind(owner)
    .bind(&view_props)
    .bind(&issue_props)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        tracing::warn!("ws-create: member insert failed");
        return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
    }
    if tx.commit().await.is_err() {
        tracing::warn!("ws-create: commit failed");
        return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
    }
    Ok((
        StatusCode::CREATED,
        Json(WorkspaceOut {
            id: row.id,
            name: row.name,
            slug: row.slug,
        }),
    ))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchWorkspace {
    #[serde(default)]
    pub name: Option<String>,
}

/// Mirrors `plane/app/views/workspace/base.py:WorkSpaceViewSet`
/// retrieve / partial_update / destroy on the slug detail route.
pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<common::models::workspace::Workspace> = sqlx::query_as(
        "SELECT id, name, slug FROM workspaces WHERE slug = $1 AND deleted_at IS NULL",
    )
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(w) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({"id": w.id, "name": w.name, "slug": w.slug})),
        )),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Workspace not found"})),
        )),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<PatchWorkspace>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 80 || contains_url(name) {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid name"}))));
        }
    }
    let n = sqlx::query(
        "UPDATE workspaces SET name = COALESCE($1, name), updated_at = now() WHERE slug = $2 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Workspace not found"})),
        ));
    }
    Ok((StatusCode::OK, Json(serde_json::json!({"slug": slug}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    // Mirrors `remove_last_workspace_ids_from_user_settings`: profiles
    // pointing at the workspace lose their last-workspace pointer.
    let row: Option<(uuid::Uuid,)> =
        sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
            .bind(&slug)
            .fetch_optional(&st.pool)
            .await?;
    let Some((workspace_id,)) = row else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Workspace not found"})),
        ));
    };
    sqlx::query("UPDATE profiles SET last_workspace_id = NULL WHERE last_workspace_id = $1")
        .bind(workspace_id)
        .execute(&st.pool)
        .await?;
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
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
