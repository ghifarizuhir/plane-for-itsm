use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Quoted from `plane/app/permissions/base.py:81-84`.
pub(crate) const FORBIDDEN_MSG: &str = "You don't have the required permissions.";
/// Quoted from `plane/app/views/base.py:92-96`.
pub(crate) const NOT_FOUND_MSG: &str = "The required object does not exist.";

pub(crate) fn deny() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"error": FORBIDDEN_MSG})))
}

pub(crate) fn missing() -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({"error": NOT_FOUND_MSG})))
}

/// Active workspace membership role for `user_id` in the workspace with
/// `slug`. Mirrors the `WorkspaceMember` lookups in
/// `plane/app/permissions/base.py` (`is_active=True`, soft-delete excluded).
pub(crate) async fn ws_role(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
) -> Result<Option<i16>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT wm.role FROM workspace_members wm \
         JOIN workspaces w ON w.id = wm.workspace_id \
         WHERE w.slug = $1 AND wm.member_id = $2 \
         AND wm.is_active = true AND wm.deleted_at IS NULL",
    )
    .bind(slug)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// Active project membership role. Mirrors the `ProjectMember` lookups in
/// `plane/app/views/project/member.py` (`is_active=True`).
pub(crate) async fn project_role(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    pid: uuid::Uuid,
) -> Result<Option<i16>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT role FROM project_members \
         WHERE project_id = $1 AND member_id = $2 AND is_active = true",
    )
    .bind(pid)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// Mirrors `plane/app/views/project/member.py:332-345`: a project admin may
/// leave only when at least one other active admin remains. Message is
/// verbatim (grammar quirks included).
pub fn guard_leave(is_admin: bool, admin_count: i64) -> Result<(), String> {
    if is_admin && admin_count <= 1 {
        return Err("You cannot leave the project as your the only admin of the project you will have to either delete the project or create an another admin".to_string());
    }
    Ok(())
}

/// Seed rows for `create`, mirroring `plane/db/models/state.py:24-66`
/// (`DEFAULT_STATES`).
pub(crate) struct SeedState {
    pub(crate) name: &'static str,
    pub(crate) color: &'static str,
    pub(crate) sequence: f64,
    pub(crate) group: &'static str,
    pub(crate) default: bool,
}

pub(crate) const DEFAULT_STATES_SEED: &[SeedState] = &[
    SeedState { name: "Backlog", color: "#60646C", sequence: 15000.0, group: "backlog", default: true },
    SeedState { name: "Todo", color: "#60646C", sequence: 25000.0, group: "unstarted", default: false },
    SeedState { name: "In Progress", color: "#F59E0B", sequence: 35000.0, group: "started", default: false },
    SeedState { name: "Done", color: "#46A758", sequence: 45000.0, group: "completed", default: false },
    SeedState { name: "Cancelled", color: "#9AA4BC", sequence: 55000.0, group: "cancelled", default: false },
    SeedState { name: "Triage", color: "#4E5355", sequence: 65000.0, group: "triage", default: false },
];

/// Mirrors `State.save` in `plane/db/models/state.py:117-118`
/// (`self.slug = slugify(self.name)`): Django `slugify` lowercases and turns
/// whitespace into hyphens, so for the seed names above the output equals
/// `name.to_lowercase().replace(' ', "-")` (`"In Progress"` → `"in-progress"`).
fn state_slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// Mirrors `plane/app/serializers/project.py:ProjectSerializer`
/// + `plane/db/models/project.py:FORBIDDEN_IDENTIFIER_CHARS_PATTERN`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateProject {
    pub name: String,
    pub identifier: String,
    /// Mirrors `ProjectSerializer` (`plane/app/serializers/project.py:30-37`,
    /// `fields = "__all__"`): nullable `project_lead` FK to users. An unknown
    /// id is rejected with 400, mirroring implicit DRF FK validation
    /// (`PrimaryKeyRelatedField does_not_exist`).
    #[serde(default)]
    pub project_lead: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectOut {
    pub id: uuid::Uuid,
    pub name: String,
    pub identifier: String,
}

const FORBIDDEN: &[char] = &[
    '&', '+', ',', ':', ';', '$', '^', '}', '{', '*', '=', '?', '@', '#', '|', '\'', '<', '>',
    '.', '(', ')', '%', '!', '-', '/',
];

fn has_forbidden(s: &str) -> bool {
    s.chars().any(|c| FORBIDDEN.contains(&c))
}

pub fn validate_create(body: &CreateProject) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.identifier.trim().is_empty() {
        return Err("identifier is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if body.identifier.chars().count() > 12 {
        return Err("identifier max length 12".to_string());
    }
    if has_forbidden(&body.name) {
        return Err("PROJECT_NAME_CANNOT_CONTAIN_SPECIAL_CHARACTERS".to_string());
    }
    if has_forbidden(&body.identifier) {
        return Err("PROJECT_IDENTIFIER_CANNOT_CONTAIN_SPECIAL_CHARACTERS".to_string());
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Vec<ProjectOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::project::Project>(
        "SELECT p.id, p.name FROM projects p JOIN workspaces w ON w.id = p.workspace_id WHERE w.slug = $1 AND p.deleted_at IS NULL ORDER BY p.name ASC",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|p| ProjectOut {
                id: p.id,
                name: p.name,
                identifier: String::new(),
            })
            .collect(),
    ))
}

/// 400 body for an unknown `project_lead`, mirroring implicit DRF FK
/// validation (`PrimaryKeyRelatedField does_not_exist`) from
/// `ProjectSerializer` (`plane/app/serializers/project.py:30-37`,
/// `fields = "__all__"`): `{"project_lead": ["Invalid pk \"<uuid>\" -
/// object does not exist."]}`.
pub(crate) fn invalid_lead_body(lead: &uuid::Uuid) -> Value {
    json!({"project_lead": [format!("Invalid pk \"{}\" - object does not exist.", lead)]})
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<CreateProject>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let creator = auth.0;
    if let Some(lead) = body.project_lead {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(lead)
            .fetch_one(&st.pool)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "project-create: project_lead lookup failed");
                common::errors::AppError(anyhow::anyhow!("internal error"))
            })?;
        if !exists {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(invalid_lead_body(&lead)),
            ));
        }
    }
    let ident = body.identifier.trim().to_uppercase();
    // Single transaction like `3df4f504b` (ws-create): project INSERT +
    // creator/lead ADMIN memberships + DEFAULT_STATES seed all commit or all
    // roll back. Mirrors `plane/app/views/project/base.py:258-313`.
    let mut tx = st.pool.begin().await.map_err(|e| {
        tracing::warn!(error = %e, "project-create: begin transaction failed");
        common::errors::AppError(anyhow::anyhow!("internal error"))
    })?;
    let row: (uuid::Uuid, String, uuid::Uuid) = sqlx::query_as(
        "INSERT INTO projects (id, name, description, identifier, workspace_id, project_lead_id, network, module_view, cycle_view, issue_views_view, page_view, intake_view, is_time_tracking_enabled, is_issue_type_enabled, guest_view_all_features, archive_in, close_in, logo_props, timezone, created_at, updated_at) SELECT gen_random_uuid(), $1, '', $2, w.id, $3, 2, false, false, false, true, false, false, false, false, 0, 0, '{}', w.timezone, now(), now() FROM workspaces w WHERE w.slug = $4 RETURNING id, name, workspace_id",
    )
    .bind(&body.name)
    .bind(&ident)
    .bind(body.project_lead)
    .bind(&slug)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "project-create: project insert failed");
        common::errors::AppError(anyhow::anyhow!("internal error"))
    })?;
    let (project_id, project_name, workspace_id) = row;
    // Column list mirrors `member.rs:125` (`view_props`/`default_props` '{}',
    // `sort_order` 65535, `preferences` '{}'); role 20 = ADMIN
    // (`base.py:266-270`).
    let member_res = sqlx::query(
        "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, view_props, default_props, sort_order, preferences, created_at, updated_at) VALUES (gen_random_uuid(), $1, 20, $2, $3, true, '{}', '{}', 65535, '{}', now(), now())",
    )
    .bind(creator)
    .bind(project_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await;
    if member_res.is_err() {
        tracing::warn!("project-create: creator member insert failed");
        return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
    }
    // `base.py:272-279`: project_lead (when set and not the creator) is added
    // as a second ADMIN.
    if let Some(lead) = body.project_lead {
        if lead != creator {
            let lead_res = sqlx::query(
                "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, view_props, default_props, sort_order, preferences, created_at, updated_at) VALUES (gen_random_uuid(), $1, 20, $2, $3, true, '{}', '{}', 65535, '{}', now(), now())",
            )
            .bind(lead)
            .bind(project_id)
            .bind(workspace_id)
            .execute(&mut *tx)
            .await;
            if lead_res.is_err() {
                tracing::warn!("project-create: lead member insert failed");
                return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
            }
        }
    }
    // `base.py:281-295`: seed DEFAULT_STATES (`created_by` = creator).
    // `states.slug`/`description`/`is_triage` are NOT NULL (live `\d states`),
    // so they are set explicitly; Django's `bulk_create` skips `State.save`
    // (leaving slug '') — here the `slugify` output is stored instead.
    for s in DEFAULT_STATES_SEED {
        let state_res = sqlx::query(
            "INSERT INTO states (id, name, description, color, slug, created_by_id, project_id, workspace_id, sequence, \"group\", \"default\", is_triage, created_at, updated_at) VALUES (gen_random_uuid(), $1, '', $2, $3, $4, $5, $6, $7, $8, $9, false, now(), now())",
        )
        .bind(s.name)
        .bind(s.color)
        .bind(state_slug(s.name))
        .bind(creator)
        .bind(project_id)
        .bind(workspace_id)
        .bind(s.sequence)
        .bind(s.group)
        .bind(s.default)
        .execute(&mut *tx)
        .await;
        if state_res.is_err() {
            tracing::warn!("project-create: state seed insert failed");
            return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
        }
    }
    if tx.commit().await.is_err() {
        tracing::warn!("project-create: commit failed");
        return Err(common::errors::AppError(anyhow::anyhow!("internal error")));
    }
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": project_id, "name": project_name, "identifier": ident})),
    ))
}

/// Mirrors `plane/app/views/project/base.py:partial_update`: archived
/// projects are immutable.
pub fn guard_patch(archived: bool) -> Result<(), String> {
    if archived {
        return Err("Archived projects cannot be updated".to_string());
    }
    Ok(())
}

/// Mirrors `plane/app/serializers/project.py:validate_name` /
/// `validate_identifier`: sibling-name/identifier collisions are rejected.
pub fn guard_name_unique(exists: bool) -> Result<(), String> {
    if exists {
        return Err("PROJECT_NAME_ALREADY_EXIST".to_string());
    }
    Ok(())
}

pub fn guard_identifier_unique(exists: bool) -> Result<(), String> {
    if exists {
        return Err("PROJECT_IDENTIFIER_ALREADY_EXIST".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchProject {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<common::models::project::Project> = sqlx::query_as(
        "SELECT id, name FROM projects p JOIN workspaces w ON w.id = p.workspace_id WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&_slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(p) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({"id": p.id, "name": p.name})),
        )),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Project does not exist"})),
        )),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<PatchProject>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<(Option<chrono::DateTime<chrono::Utc>>, uuid::Uuid)> = sqlx::query_as(
        "SELECT p.archived_at, p.workspace_id FROM projects p JOIN workspaces w ON w.id = p.workspace_id WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((archived_at, workspace_id)) = row else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Project does not exist"})),
        ));
    };
    if let Err(e) = guard_patch(archived_at.is_some()) {
        return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
    }
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 || has_forbidden(name) {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid name"}))));
        }
        let dup: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM projects WHERE workspace_id = $1 AND name = $2 AND id != $3 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(name)
        .bind(pk)
        .fetch_optional(&st.pool)
        .await?;
        if let Err(e) = guard_name_unique(dup.is_some()) {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
        }
    }
    if let Some(identifier) = &body.identifier {
        let ident = identifier.trim().to_uppercase();
        if ident.is_empty() || ident.chars().count() > 12 || has_forbidden(&ident) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid identifier"})),
            ));
        }
        let dup: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM projects WHERE workspace_id = $1 AND identifier = $2 AND id != $3 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(&ident)
        .bind(pk)
        .fetch_optional(&st.pool)
        .await?;
        if let Err(e) = guard_identifier_unique(dup.is_some()) {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
        }
        sqlx::query(
            "UPDATE projects SET name = COALESCE($1, name), identifier = $2, description = COALESCE($3, description), updated_at = now() WHERE id = $4",
        )
        .bind(&body.name)
        .bind(&ident)
        .bind(&body.description)
        .bind(pk)
        .execute(&st.pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE projects SET name = COALESCE($1, name), description = COALESCE($2, description), updated_at = now() WHERE id = $3",
        )
        .bind(&body.name)
        .bind(&body.description)
        .bind(pk)
        .execute(&st.pool)
        .await?;
    }
    Ok((StatusCode::OK, Json(serde_json::json!({"id": pk}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let n = sqlx::query(
        "UPDATE projects SET deleted_at = now() WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Project does not exist"})),
        ));
    }
    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!(null))))
}

#[cfg(test)]
mod batch_c_tests {
    use super::*;

    #[test]
    fn forbidden_message_matches_django() {
        // Quoted from `plane/app/permissions/base.py:81-84`.
        assert_eq!(FORBIDDEN_MSG, "You don't have the required permissions.");
    }

    #[test]
    fn not_found_message_matches_django() {
        // Quoted from `plane/app/views/base.py:92-96`.
        assert_eq!(NOT_FOUND_MSG, "The required object does not exist.");
    }

    #[test]
    fn sole_admin_guard() {
        // Mirrors `plane/app/views/project/member.py:332-345`: an admin may
        // leave only when another active admin remains.
        assert!(guard_leave(true, 1).is_err());
        assert!(guard_leave(true, 2).is_ok());
        assert!(guard_leave(false, 1).is_ok());
    }

    #[test]
    fn default_states_seed_count() {
        // Source: `plane/db/models/state.py:24-66` (DEFAULT_STATES).
        assert_eq!(DEFAULT_STATES_SEED.len(), 6);
        assert_eq!(
            DEFAULT_STATES_SEED.iter().filter(|s| s.default).count(),
            1
        );
    }

    #[test]
    fn invalid_lead_body_matches_drf_fk_error() {
        // Mirrors implicit DRF FK validation (`PrimaryKeyRelatedField
        // does_not_exist`) from `ProjectSerializer`
        // (`plane/app/serializers/project.py:30-37`, `fields = "__all__"`).
        let lead = uuid::Uuid::parse_str("12345678-1234-5678-1234-567812345678").unwrap();
        assert_eq!(
            invalid_lead_body(&lead),
            serde_json::json!({"project_lead": ["Invalid pk \"12345678-1234-5678-1234-567812345678\" - object does not exist."]})
        );
    }
}
