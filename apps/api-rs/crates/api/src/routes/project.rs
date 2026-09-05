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

/// Workspace role → project visibility scope for `project_details`.
/// Mirrors `plane/app/views/project/base.py:105-128`: GUEST (5) sees only
/// projects with an active membership ("own"), MEMBER (15) sees own +
/// public (`network=2`), ADMIN (20) sees all. "Own" is an active
/// `project_members` row
/// (`project_projectmember__member=user, is_active=True`), NOT `created_by`.
pub(crate) fn details_scope(role: i16) -> &'static str {
    match role {
        20 => "all",
        15 => "own_or_public",
        // 5 (GUEST) and anything unknown fall back to the most restrictive
        // scope; Django only knows 20/15/5 (`ROLE_CHOICES`).
        _ => "own",
    }
}

/// Mirrors `get_next_work_item_sequence`
/// (`plane/app/serializers/project.py:132-135`): `MAX(sequence)+1`, or 1
/// when the project has no `IssueSequence` rows.
pub(crate) fn next_work_item_sequence(max_seq: Option<i64>) -> i64 {
    max_seq.map(|m| m + 1).unwrap_or(1)
}

/// Mirrors `Project.cover_image_url` (`plane/db/models/project.py:128-137`)
/// + `FileAsset.asset_url` (`plane/db/models/asset.py:83-90`): a linked
/// cover asset wins over the legacy `cover_image` text column; empty text
/// counts as missing.
pub(crate) fn cover_image_url(
    asset_id: Option<uuid::Uuid>,
    asset_entity_type: Option<&str>,
    cover_image: Option<&str>,
) -> Option<String> {
    if let Some(id) = asset_id {
        // Practically the linked asset is always `PROJECT_COVER`
        // (`/api/assets/v2/static/<id>/`); any other entity type yields
        // Django's per-type branch URLs or `None` — mapped to `None` here
        // (deviation, see `project_details` docs).
        if asset_entity_type == Some("PROJECT_COVER") {
            return Some(format!("/api/assets/v2/static/{id}/"));
        }
        return None;
    }
    cover_image
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// One row of the `project_details` listing: all `projects` model columns
/// plus the `get_queryset` annotations (`base.py:54-97`). Field names match
/// the SELECT aliases in `project_details`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct DetailRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) description_text: Option<serde_json::Value>,
    pub(crate) description_html: Option<serde_json::Value>,
    pub(crate) network: i16,
    pub(crate) identifier: String,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) default_assignee_id: Option<uuid::Uuid>,
    pub(crate) project_lead_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) emoji: Option<String>,
    pub(crate) cycle_view: bool,
    pub(crate) module_view: bool,
    pub(crate) cover_image: Option<String>,
    pub(crate) issue_views_view: bool,
    pub(crate) page_view: bool,
    pub(crate) estimate_id: Option<uuid::Uuid>,
    pub(crate) icon_prop: Option<serde_json::Value>,
    pub(crate) intake_view: bool,
    pub(crate) archive_in: i32,
    pub(crate) close_in: i32,
    pub(crate) default_state_id: Option<uuid::Uuid>,
    pub(crate) logo_props: serde_json::Value,
    pub(crate) archived_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) is_time_tracking_enabled: bool,
    pub(crate) is_issue_type_enabled: bool,
    pub(crate) guest_view_all_features: bool,
    pub(crate) timezone: String,
    pub(crate) cover_image_asset_id: Option<uuid::Uuid>,
    pub(crate) external_id: Option<String>,
    pub(crate) external_source: Option<String>,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) is_favorite: bool,
    pub(crate) member_role: Option<i16>,
    pub(crate) anchor: Option<String>,
    pub(crate) sort_order: Option<f64>,
    pub(crate) max_seq: Option<i64>,
    pub(crate) cover_asset_entity_type: Option<String>,
}

/// Serializes one `DetailRow` like `ProjectListSerializer`
/// (`serializers/project.py:115-139`): all model fields (FKs as id strings,
/// matching DRF's default PK representation) + the 8 annotation keys.
pub(crate) fn project_detail_json(row: &DetailRow, members: &[uuid::Uuid]) -> Value {
    let opt_id = |id: &Option<uuid::Uuid>| id.map(|u| json!(u)).unwrap_or(Value::Null);
    // Built via explicit `Map` inserts: a 44-key `json!` literal exceeds the
    // macro recursion limit.
    let mut m = serde_json::Map::with_capacity(44);
    let mut put = |k: &str, v: Value| {
        m.insert(k.to_string(), v);
    };
    put("id", json!(row.id));
    put("name", json!(&row.name));
    put("description", json!(&row.description));
    put("description_text", json!(&row.description_text));
    put("description_html", json!(&row.description_html));
    put("network", json!(row.network));
    put("identifier", json!(&row.identifier));
    put("created_by", opt_id(&row.created_by_id));
    put("default_assignee", opt_id(&row.default_assignee_id));
    put("project_lead", opt_id(&row.project_lead_id));
    put("updated_by", opt_id(&row.updated_by_id));
    put("workspace", json!(row.workspace_id));
    put("emoji", json!(&row.emoji));
    put("cycle_view", json!(row.cycle_view));
    put("module_view", json!(row.module_view));
    put("cover_image", json!(&row.cover_image));
    put("issue_views_view", json!(row.issue_views_view));
    put("page_view", json!(row.page_view));
    put("estimate", opt_id(&row.estimate_id));
    put("icon_prop", json!(&row.icon_prop));
    put("intake_view", json!(row.intake_view));
    put("archive_in", json!(row.archive_in));
    put("close_in", json!(row.close_in));
    put("default_state", opt_id(&row.default_state_id));
    put("logo_props", json!(&row.logo_props));
    put("archived_at", json!(&row.archived_at));
    put("is_time_tracking_enabled", json!(row.is_time_tracking_enabled));
    put("is_issue_type_enabled", json!(row.is_issue_type_enabled));
    // Always null here: the listing query filters `deleted_at IS NULL`
    // (Django includes the field as null the same way).
    put("deleted_at", Value::Null);
    put(
        "guest_view_all_features",
        json!(row.guest_view_all_features),
    );
    put("timezone", json!(&row.timezone));
    put("cover_image_asset", opt_id(&row.cover_image_asset_id));
    put("external_id", json!(&row.external_id));
    put("external_source", json!(&row.external_source));
    put("created_at", json!(&row.created_at));
    put("updated_at", json!(&row.updated_at));
    put("is_favorite", json!(row.is_favorite));
    put("member_role", json!(&row.member_role));
    put("anchor", json!(&row.anchor));
    put("sort_order", json!(&row.sort_order));
    put("members", json!(members));
    put(
        "cover_image_url",
        json!(cover_image_url(
            row.cover_image_asset_id,
            row.cover_asset_entity_type.as_deref(),
            row.cover_image.as_deref(),
        )),
    );
    put("inbox_view", json!(row.intake_view));
    put(
        "next_work_item_sequence",
        json!(next_work_item_sequence(row.max_seq)),
    );
    Value::Object(m)
}

/// GET `/api/workspaces/:slug/projects/details/` — parity with Django
/// `ProjectViewSet.list_detail` full-list branch
/// (`plane/app/views/project/base.py:101-143`, branch at 142-143).
///
/// - Gate: workspace non-members → 403 via `deny()` (`allow_permission`
///   ADMIN/MEMBER/GUEST, level WORKSPACE).
/// - Role filter mirrors `base.py:105-128` via `details_scope`.
/// - Annotations mirror `get_queryset` (`base.py:54-97`): `is_favorite`
///   (`user_favorites`, `entity_type='project'`), `member_role` (active
///   membership or null), `anchor` (`deploy_boards`, nullable), `sort_order`
///   (`project_user_properties`, nullable), `members` (active non-bot member
///   ids, mirroring `members_list` + `get_members`), `cover_image_url`,
///   `inbox_view` (`=intake_view`), `next_work_item_sequence` (max+1 else 1).
/// - Ordering mirrors `base.py:104`: `sort_order, name` (both ASC).
///
/// Deviations: paginated branch (`?per_page`, `base.py:130-140`) is OUT — FE
/// `getProjects()` never paginates (`project.service.ts:55-61`); `?fields=`
/// filtering is not honored (FE never sends it); datetimes serialize as
/// RFC3339 (chrono) vs DRF ISO8601 (same instants); annotation subqueries
/// add explicit `deleted_at IS NULL` (Django's soft-delete default manager
/// does this implicitly, `mixins.py:58`); `IssueSequence` MAX ignores the
/// `deleted` boolean flag exactly like Django (only `deleted_at` excluded);
/// member rows with NULL `member_id` are skipped (Django would crash
/// dereferencing `member.member.is_bot`); non-`PROJECT_COVER` asset entity
/// types map `cover_image_url` to null instead of Django's branch URLs.
pub async fn project_details(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let user_id = auth.0;
    let role = ws_role(&st.pool, user_id, &slug).await.map_err(|e| {
        tracing::warn!(error = %e, "projects-details: ws_role lookup failed");
        common::errors::AppError(anyhow::anyhow!("internal error"))
    })?;
    let Some(role) = role else {
        return Ok(deny());
    };
    // Role filter mirrors `base.py:105-128` (`network=2` is PUBLIC,
    // `project.py:30-33`).
    let scope_filter = match details_scope(role) {
        "all" => String::new(),
        "own_or_public" => "AND (p.network = 2 OR EXISTS(SELECT 1 FROM project_members pmf \
            WHERE pmf.project_id = p.id AND pmf.member_id = $2 \
            AND pmf.is_active = true AND pmf.deleted_at IS NULL))"
            .to_string(),
        _ => "AND EXISTS(SELECT 1 FROM project_members pmf \
            WHERE pmf.project_id = p.id AND pmf.member_id = $2 \
            AND pmf.is_active = true AND pmf.deleted_at IS NULL)"
            .to_string(),
    };
    let sql = format!(
        "SELECT p.id, p.name, p.description, p.description_text, p.description_html, \
        p.network, p.identifier, p.created_by_id, p.default_assignee_id, \
        p.project_lead_id, p.updated_by_id, p.workspace_id, p.emoji, \
        p.cycle_view, p.module_view, p.cover_image, p.issue_views_view, \
        p.page_view, p.estimate_id, p.icon_prop, p.intake_view, p.archive_in, \
        p.close_in, p.default_state_id, p.logo_props, p.archived_at, \
        p.is_time_tracking_enabled, p.is_issue_type_enabled, \
        p.guest_view_all_features, p.timezone, p.cover_image_asset_id, \
        p.external_id, p.external_source, p.created_at, p.updated_at, \
        EXISTS(SELECT 1 FROM user_favorites uf \
            WHERE uf.user_id = $2 AND uf.entity_identifier = p.id \
            AND uf.entity_type = 'project' AND uf.project_id = p.id \
            AND uf.deleted_at IS NULL) AS is_favorite, \
        (SELECT pm.role FROM project_members pm \
            WHERE pm.project_id = p.id AND pm.member_id = $2 \
            AND pm.is_active = true AND pm.deleted_at IS NULL) AS member_role, \
        (SELECT db.anchor FROM deploy_boards db \
            WHERE db.entity_name = 'project' AND db.entity_identifier = p.id \
            AND db.workspace_id = p.workspace_id \
            AND db.deleted_at IS NULL) AS anchor, \
        (SELECT pup.sort_order FROM project_user_properties pup \
            WHERE pup.user_id = $2 AND pup.project_id = p.id \
            AND pup.workspace_id = p.workspace_id \
            AND pup.deleted_at IS NULL) AS sort_order, \
        (SELECT MAX(seq.sequence) FROM issue_sequences seq \
            WHERE seq.project_id = p.id AND seq.deleted_at IS NULL) AS max_seq, \
        fa.entity_type AS cover_asset_entity_type \
        FROM projects p \
        JOIN workspaces w ON w.id = p.workspace_id \
        LEFT JOIN file_assets fa ON fa.id = p.cover_image_asset_id \
        WHERE w.slug = $1 AND p.deleted_at IS NULL {scope_filter} \
        ORDER BY sort_order ASC NULLS LAST, p.name ASC"
    );
    let rows: Vec<DetailRow> = sqlx::query_as(&sql)
        .bind(&slug)
        .bind(user_id)
        .fetch_all(&st.pool)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "projects-details: listing query failed");
            common::errors::AppError(anyhow::anyhow!("internal error"))
        })?;
    // Members for all rows in one query (mirrors the `members_list`
    // prefetch `base.py:89-97` + `get_members`
    // `serializers/project.py:125-130`: active members, bots excluded).
    let ids: Vec<uuid::Uuid> = rows.iter().map(|r| r.id).collect();
    let member_rows: Vec<(uuid::Uuid, uuid::Uuid)> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT pm.project_id, pm.member_id FROM project_members pm \
             JOIN users u ON u.id = pm.member_id \
             WHERE pm.project_id = ANY($1) AND pm.is_active = true \
             AND pm.deleted_at IS NULL AND pm.member_id IS NOT NULL \
             AND u.is_bot = false",
        )
        .bind(&ids)
        .fetch_all(&st.pool)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "projects-details: members query failed");
            common::errors::AppError(anyhow::anyhow!("internal error"))
        })?
    };
    let mut by_project: std::collections::HashMap<uuid::Uuid, Vec<uuid::Uuid>> =
        std::collections::HashMap::new();
    for (pid, mid) in member_rows {
        by_project.entry(pid).or_default().push(mid);
    }
    let empty: Vec<uuid::Uuid> = Vec::new();
    let out: Vec<Value> = rows
        .iter()
        .map(|r| project_detail_json(r, by_project.get(&r.id).unwrap_or(&empty)))
        .collect();
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

/// Query params for `check_identifier`: `?name=` (Django reads
/// `request.GET.get("name", "")`, so the param is optional here — a missing
/// param normalizes to `""` and yields the same 400).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct IdentifierQuery {
    #[serde(default)]
    pub name: Option<String>,
}

/// One `project_identifiers` row for `check_identifier`: `id` is `bigint`
/// (live `\d project_identifiers`), `project` maps `project_id` (Django
/// `.values("project")` yields the FK id under the field name).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct IdentifierRow {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) project_id: uuid::Uuid,
}

/// GET `/api/workspaces/:slug/project-identifiers/` — parity with Django
/// `ProjectIdentifierEndpoint.get` (`plane/app/views/project/base.py:444-454`).
///
/// - Gate: workspace ADMIN/MEMBER only — GUEST (5) and non-members → 403 via
///   `deny()` (`allow_permission([ADMIN, MEMBER], level="WORKSPACE")`).
/// - `?name=`: strip + UPPERCASE; missing/empty → 400
///   `{"error": "Name is required"}`.
/// - Else 200 `{"exists": <count>, "identifiers": [{"id", "name",
///   "project"}]}` filtered `name=X AND workspace__slug=slug`.
///
/// Deviations: none on the filter — the SELECT mirrors Django exactly:
/// `ProjectIdentifier(AuditModel)` inherits `SoftDeletionManager`
/// (`apps/api/plane/db/mixins.py:56-58`), so `objects.filter()` excludes
/// soft-deleted rows (`pi.deleted_at IS NULL`); row order follows
/// `Meta.ordering = ("-created_at",)`. DELETE on this path is out of scope
/// (FE never calls it).
pub async fn check_identifier(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<IdentifierQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    match role {
        Some(r) if r >= 15 => {}
        _ => return Ok(deny()),
    }
    let name = normalize_ident(params.name.as_deref().unwrap_or(""));
    if let Err(e) = validate_ident_name(&name) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    let rows: Vec<IdentifierRow> = sqlx::query_as(
        "SELECT pi.id, pi.name, pi.project_id FROM project_identifiers pi \
         JOIN workspaces w ON w.id = pi.workspace_id \
         WHERE pi.name = $1 AND w.slug = $2 AND pi.deleted_at IS NULL \
         ORDER BY pi.created_at DESC",
    )
    .bind(&name)
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    let identifiers: Vec<Value> = rows
        .iter()
        .map(|r| json!({"id": r.id, "name": r.name, "project": r.project_id}))
        .collect();
    Ok((
        StatusCode::OK,
        Json(json!({"exists": identifiers.len(), "identifiers": identifiers})),
    ))
}

/// Mirrors `ProjectIdentifierEndpoint.get`
/// (`plane/app/views/project/base.py:444-454`):
/// `request.GET.get("name", "").strip().upper()`.
pub(crate) fn normalize_ident(raw: &str) -> String {
    raw.trim().to_uppercase()
}

/// Mirrors `base.py:447-448`: missing/empty `name` → 400
/// `{"error": "Name is required"}`.
pub(crate) fn validate_ident_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Name is required".to_string());
    }
    Ok(())
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

    #[test]
    fn details_scope_matches_django_role_filters() {
        // Mapping mirrors `plane/app/views/project/base.py:105-128`: GUEST
        // (5) sees only projects with an active membership ("own"), MEMBER
        // (15) sees own + public (`network=2`), ADMIN (20) sees all. "Own"
        // means an active `project_members` row
        // (`project_projectmember__member=user, is_active=True`), NOT
        // `created_by` — read from code, not guessed.
        assert_eq!(details_scope(20), "all");
        assert_eq!(details_scope(15), "own_or_public");
        assert_eq!(details_scope(5), "own");
    }

    #[test]
    fn next_sequence_matches_django_serializer() {
        // Mirrors `get_next_work_item_sequence`
        // (`plane/app/serializers/project.py:132-135`): max+1, or 1 when no
        // `IssueSequence` rows exist.
        assert_eq!(next_work_item_sequence(None), 1);
        assert_eq!(next_work_item_sequence(Some(4)), 5);
    }

    #[test]
    fn cover_url_matches_django_property() {
        // Mirrors `Project.cover_image_url`
        // (`plane/db/models/project.py:128-137`) + `FileAsset.asset_url`
        // (`plane/db/models/asset.py:83-90`): asset wins over the legacy
        // `cover_image` text column; empty text counts as missing.
        let asset = uuid::Uuid::parse_str("12345678-1234-5678-1234-567812345678").unwrap();
        assert_eq!(
            cover_image_url(Some(asset), Some("PROJECT_COVER"), None),
            Some(format!("/api/assets/v2/static/{asset}/"))
        );
        assert_eq!(
            cover_image_url(Some(asset), Some("ISSUE_ATTACHMENT"), Some("https://x/y.png")),
            None
        );
        assert_eq!(
            cover_image_url(None, None, Some("https://x/y.png")),
            Some("https://x/y.png".to_string())
        );
        assert_eq!(cover_image_url(None, None, Some("")), None);
        assert_eq!(cover_image_url(None, None, None), None);
    }

    #[test]
    fn detail_json_has_all_serializer_fields() {
        // `ProjectListSerializer` (`serializers/project.py:115-139`) =
        // all model fields + 8 annotation keys (`is_favorite`,
        // `member_role`, `anchor`, `sort_order`, `members`,
        // `cover_image_url`, `inbox_view`, `next_work_item_sequence`).
        let row = DetailRow {
            id: uuid::Uuid::nil(),
            name: "P".to_string(),
            description: String::new(),
            description_text: None,
            description_html: None,
            network: 2,
            identifier: "P".to_string(),
            created_by_id: None,
            default_assignee_id: None,
            project_lead_id: None,
            updated_by_id: None,
            workspace_id: uuid::Uuid::nil(),
            emoji: None,
            cycle_view: false,
            module_view: false,
            cover_image: None,
            issue_views_view: false,
            page_view: true,
            estimate_id: None,
            icon_prop: None,
            intake_view: false,
            archive_in: 0,
            close_in: 0,
            default_state_id: None,
            logo_props: serde_json::json!({}),
            archived_at: None,
            is_time_tracking_enabled: false,
            is_issue_type_enabled: false,
            guest_view_all_features: false,
            timezone: "UTC".to_string(),
            cover_image_asset_id: None,
            external_id: None,
            external_source: None,
            created_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            updated_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            is_favorite: false,
            member_role: None,
            anchor: None,
            sort_order: None,
            max_seq: None,
            cover_asset_entity_type: None,
        };
        let v = project_detail_json(&row, &[]);
        for key in [
            "is_favorite",
            "member_role",
            "anchor",
            "sort_order",
            "members",
            "cover_image_url",
            "inbox_view",
            "next_work_item_sequence",
        ] {
            assert!(v.get(key).is_some(), "missing key {key}");
        }
        // 36 model columns + 8 annotation keys.
        assert_eq!(v.as_object().unwrap().len(), 44);
    }

    #[test]
    fn normalize_ident_strips_and_uppercases() {
        // Mirrors `ProjectIdentifierEndpoint.get`
        // (`plane/app/views/project/base.py:444-454`):
        // `request.GET.get("name", "").strip().upper()`.
        assert_eq!(normalize_ident("  abc "), "ABC");
    }

    #[test]
    fn validate_ident_name_requires_name() {
        // Mirrors `base.py:447-448`: missing/empty `name` → 400
        // `{"error": "Name is required"}`.
        assert_eq!(validate_ident_name(""), Err("Name is required".to_string()));
        assert!(validate_ident_name("x").is_ok());
    }
}
