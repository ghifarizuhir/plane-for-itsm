use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, is_integrity_error, missing, ws_role},
    state::AppState,
};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

/// Mirrors `plane/api/serializers/issue.py:LabelCreateUpdateSerializer`
/// served by `plane/api/urls/label.py` (LabelListCreateAPIEndpoint).
/// Uniqueness (project,name) and (external_source,external_id) → 409
/// mirrors the IntegrityError branch in `plane/api/views/issue.py:878`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateLabel {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub external_source: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub sort_order: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LabelOut {
    pub id: uuid::Uuid,
    pub name: String,
}

pub fn validate_create(body: &CreateLabel) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if let Some(color) = &body.color {
        if color.chars().count() > 255 {
            return Err("color max length 255".to_string());
        }
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<LabelOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::label::Label>(
        "SELECT id, name FROM labels WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|l| LabelOut { id: l.id, name: l.name })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateLabel>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    // External-id idempotency guard (Django returns the existing label with 409).
    if let (Some(source), Some(external_id)) = (&body.external_source, &body.external_id) {
        let existing = sqlx::query_as::<_, common::models::label::Label>(
            "SELECT id, name FROM labels WHERE project_id = $1 AND external_source = $2 AND external_id = $3 AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(source)
        .bind(external_id)
        .fetch_optional(&st.pool)
        .await?;
        if let Some(label) = existing {
            return Ok((
                StatusCode::CONFLICT,
                Json(json!({"error": "Label with the same external id and external source already exists", "id": label.id})),
            ));
        }
    }

    // Unique (project, name) guard → 409 mirrors IntegrityError branch.
    let existing = sqlx::query_as::<_, common::models::label::Label>(
        "SELECT id, name FROM labels WHERE project_id = $1 AND name = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&body.name)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(label) = existing {
        return Ok((
            StatusCode::CONFLICT,
            Json(json!({"error": "Label with the same name already exists in the project", "id": label.id})),
        ));
    }

    // sort_order mirrors Label.save(): max + 10000, default 65535.
    let row = sqlx::query_as::<_, common::models::label::Label>(
        "INSERT INTO labels (id, name, color, description, external_source, external_id, parent_id, sort_order, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, $5, $6, COALESCE($7, (SELECT MAX(sort_order) + 10000 FROM labels WHERE project_id = $8), 65535), $8, w.id, now(), now() FROM workspaces w WHERE w.slug = $9 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(body.color.clone().unwrap_or_default())
    .bind(body.description.clone().unwrap_or_default())
    .bind(&body.external_source)
    .bind(&body.external_id)
    .bind(body.parent_id)
    .bind(body.sort_order)
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "name": row.name}))))
}

/// Mirrors `plane/app/views/issue/label.py:partial_update`: renaming onto an
/// existing sibling name is rejected with 400.
pub fn guard_patch(name_exists: bool) -> Result<(), String> {
    if name_exists {
        return Err("Label with the same name already exists in the project".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchLabel {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parent_id: Option<uuid::Uuid>,
}

/// One `LabelSerializer` row (`serializers/issue.py:365-371`: parent, name,
/// color, id, project_id, workspace_id, sort_order — no description).
fn label_json(
    id: uuid::Uuid,
    parent: Option<uuid::Uuid>,
    name: &str,
    color: &str,
    project_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    sort_order: f64,
) -> Value {
    json!({
        "id": id,
        "parent": parent,
        "name": name,
        "color": color,
        "project_id": project_id,
        "workspace_id": workspace_id,
        "sort_order": sort_order,
    })
}

type LabelRow = (uuid::Uuid, Option<uuid::Uuid>, String, String, Option<uuid::Uuid>, uuid::Uuid, f64);

/// Workspace+project member gate for safe label reads: any active project
/// membership passes (Django `get_queryset` member filter +
/// `ProjectBasePermission`), with the shared ws-admin fallback.
async fn gate_label_read(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(member_role.is_some(), member_role.is_some(), ws_admin))
}

/// ADMIN-only gate for unsafe label writes (Django `@allow_permission([ROLE.ADMIN])`
/// on create/partial_update/destroy).
async fn gate_label_admin(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(matches!(member_role, Some(20)), member_role.is_some(), ws_admin))
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_label_read(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<LabelRow> = sqlx::query_as(
        "SELECT l.id, l.parent_id, l.name, l.color, l.project_id, l.workspace_id, l.sort_order FROM labels l JOIN workspaces w ON w.id = l.workspace_id WHERE l.id = $1 AND l.project_id = $2 AND w.slug = $3 AND l.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some((id, parent, name, color, pid, wsid, sort)) => {
            Ok((StatusCode::OK, Json(label_json(id, parent, &name, &color, pid, wsid, sort))))
        }
        // Django `retrieve` miss → 404 (DRF `{"detail"}` normalized to
        // `missing()` per repo rule; unifies the old "Label not found").
        None => Ok(missing()),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchLabel>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_label_admin(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid name"}))));
        }
        let dup: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM labels WHERE project_id = $1 AND name = $2 AND id != $3 AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(name)
        .bind(pk)
        .fetch_optional(&st.pool)
        .await?;
        if let Err(e) = guard_patch(dup.is_some()) {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
        }
    }
    let n = sqlx::query(
        "UPDATE labels SET name = COALESCE($1, name), color = COALESCE($2, color), description = COALESCE($3, description), updated_at = now() WHERE id = $4 AND project_id = $5 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(&body.color)
    .bind(&body.description)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    // Django `partial_update` (`label.py:81`) returns 200 `serializer.data`.
    let row: Option<LabelRow> = sqlx::query_as(
        "SELECT l.id, l.parent_id, l.name, l.color, l.project_id, l.workspace_id, l.sort_order FROM labels l WHERE l.id = $1 AND l.deleted_at IS NULL",
    )
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some((id, parent, name, color, pid, wsid, sort)) => {
            Ok((StatusCode::OK, Json(label_json(id, parent, &name, &color, pid, wsid, sort))))
        }
        None => Ok(missing()),
    }
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_label_admin(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // DRF `destroy` 404s on miss (`get_object`); the old blind 204 is fixed.
    let n = sqlx::query(
        "UPDATE labels SET deleted_at = now() WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ===== Batch D D11: workspace labels + issue-labels collection =====

/// Quoted from `plane/app/views/issue/label.py:51-55`
/// (`LabelViewSet.create` IntegrityError branch): duplicate (project, name)
/// → 400 with this body.
pub(crate) const DUP_LABEL_MSG: &str = "Label with the same name already exists in the project";

/// D11a gate — `WorkspaceViewerPermission`
/// (`plane/app/permissions/workspace.py:93-100`): any ACTIVE ws member
/// (incl. GUEST, no role filter); non-member → 403 `deny()`.
pub(crate) fn guard_ws_labels(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(_) => Ok(()),
        None => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// D11b read gate — `ProjectBasePermission` safe-methods branch
/// (`plane/app/permissions/project.py:18-22`): any active ws member passes;
/// the per-project scoping lives in the queryset
/// (`project__project_projectmember__member=user`, `label.py:28-40`), not
/// the permission. Non-member → 403 `deny()`.
pub(crate) fn guard_issue_labels_read(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(_) => Ok(()),
        None => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// D11b create gate — `@allow_permission([ROLE.ADMIN])` (`label.py:42-44`,
/// level PROJECT default, `permissions/base.py:19`): project ADMIN (20)
/// passes outright; MEMBER (15) / GUEST (5) / non-member fall to the
/// workspace-ADMIN fallback (`permissions/base.py:61-78`) applied by the
/// caller via shared `project_gate_allows` — same shape as D3
/// `guard_mark_default`.
pub(crate) fn guard_issue_labels_create(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) => Ok(()),
        _ => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// One `LabelSerializer` row (`plane/app/serializers/issue.py:361-373`:
/// `parent, name, color, id, project_id, workspace_id, sort_order`). Real
/// delta vs the existing `common::models::label::Label` (`{id, name}`), so a
/// new struct (same precedent as D3 `StateFullRow`). `parent` is `parent_id`
/// aliased (DRF renders the FK as its pk); `project_id` is `Option` (model FK
/// nullable, `WorkspaceBaseModel.project null=True`) though non-null on
/// these scoped routes in practice. Live columns verified in
/// `apps/api-rs/migrations/0001_initial.sql` (`labels` table).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct LabelSerializerRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) project_id: Option<uuid::Uuid>,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) parent: Option<uuid::Uuid>,
    pub(crate) name: String,
    pub(crate) color: String,
    pub(crate) sort_order: f64,
}

/// Serializes one row like `LabelSerializer`: FKs render as id strings (DRF
/// default PK representation), null parent/project as null. The KEY SET
/// matches the serializer `fields` exactly; key ORDER follows repo batch
/// convention (serde_json sorts keys — same note as D3
/// `state_serializer_json`).
pub(crate) fn label_serializer_json(row: &LabelSerializerRow) -> Value {
    json!({
        "parent": row.parent,
        "name": row.name,
        "color": row.color,
        "id": row.id,
        "project_id": row.project_id,
        "workspace_id": row.workspace_id,
        "sort_order": row.sort_order,
    })
}

/// GET `/api/workspaces/:slug/labels/` — parity with Django
/// `WorkspaceLabelsEndpoint.get` (`plane/app/views/workspace/label.py:17-30`,
/// `plane/app/urls/workspace.py:157-161`).
///
/// - Gate: `WorkspaceViewerPermission` = any ACTIVE ws member incl. GUEST.
/// - Scope mirrors the queryset exactly: `workspace__slug=slug` + active
///   `project_members` row for the caller + `project__archived_at__isnull`.
///   (No `distinct`/ordering in Django — ordering falls back to the model
///   `Meta.ordering = ("-created_at",)`, `db/models/label.py:44`, mirrored
///   here for determinism.)
/// - 200 `LabelSerializer[]` (7 keys) via the shared struct/fn above.
/// - Deviations: none (Django `@cache_response` has no Rust equivalent —
///   no cache layer, Batch C/D precedent).
pub async fn ws_labels(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_ws_labels(role).is_err() {
        return Ok(deny());
    }
    let rows: Vec<LabelSerializerRow> = sqlx::query_as(
        "SELECT l.id, l.project_id, l.workspace_id, l.parent_id AS parent, l.name, l.color, l.sort_order \
         FROM labels l JOIN projects p ON p.id = l.project_id \
         WHERE l.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $1) \
         AND l.deleted_at IS NULL AND p.archived_at IS NULL \
         AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = l.project_id AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
         ORDER BY l.created_at DESC",
    )
    .bind(&slug)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(
            rows.iter().map(label_serializer_json).collect(),
        )),
    ))
}

/// GET `/api/workspaces/:slug/projects/:project_id/issue-labels/` — parity
/// with `LabelViewSet.list` via `ModelViewSet` defaults
/// (`plane/app/views/issue/label.py:23-40`,
/// `plane/app/urls/issue.py:71-75`).
///
/// - Gate: `ProjectBasePermission` safe-methods = any active ws member
///   (queryset additionally scopes to the caller's active project
///   membership, so a ws member outside the project gets `[]`, not 403 —
///   exactly like Django).
/// - Scope mirrors `get_queryset` exactly: ws slug + project_id + active
///   project membership of the caller (+ soft-delete default manager),
///   `ORDER BY sort_order ASC` (`label.py:39`). `select_related`/`distinct`
///   have no wire effect (single-row-per-label SQL via EXISTS).
/// - `filter_queryset` (filter backends) is a no-op here: `filterset_fields`
///   / `search_fields` are empty on this viewset (inherited `BaseViewSet`
///   defaults, `views/base.py:57-58`).
pub async fn issue_labels_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_issue_labels_read(role).is_err() {
        return Ok(deny());
    }
    let rows: Vec<LabelSerializerRow> = sqlx::query_as(
        "SELECT l.id, l.project_id, l.workspace_id, l.parent_id AS parent, l.name, l.color, l.sort_order \
         FROM labels l \
         WHERE l.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $1) \
         AND l.project_id = $2 AND l.deleted_at IS NULL \
         AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = $2 AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL) \
         ORDER BY l.sort_order ASC",
    )
    .bind(&slug)
    .bind(project_id)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(
            rows.iter().map(label_serializer_json).collect(),
        )),
    ))
}

/// Writable `LabelSerializer` subset for the D11b POST: `parent, name, color`
/// (id/project_id/workspace_id/sort_order are read-only — input ignored,
/// `serializers/issue.py:361-373`). Unknown keys are ignored by serde,
/// mirroring DRF (undeclared input keys are dropped, not errors).
#[derive(Debug, Clone, Deserialize)]
pub struct CreateIssueLabel {
    pub name: Option<String>,
    pub color: Option<String>,
    pub parent: Option<uuid::Uuid>,
}

/// DRF `CharField` validation for the required `name` (no blank, max 255):
/// missing → `{"name": ["This field is required."]}`, blank (post-trim,
/// `trim_whitespace`) → `{"name": ["This field may not be blank."]}`, >255
/// chars → `{"name": ["Ensure this field has no more than 255 characters."]}`
/// — same bodies as the D8 `validate_reaction` precedent. Returns the
/// TRIMMED name (DRF `to_internal_value` trims before save).
pub(crate) fn validate_issue_label_name(name: &Option<String>) -> Result<String, Value> {
    match name {
        None => Err(json!({"name": ["This field is required."]})),
        Some(n) if n.trim().is_empty() => Err(json!({"name": ["This field may not be blank."]})),
        Some(n) if n.chars().count() > 255 => {
            Err(json!({"name": ["Ensure this field has no more than 255 characters."]}))
        }
        Some(n) => Ok(n.trim().to_string()),
    }
}

fn is_integrity_err(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| is_integrity_error(c.as_ref()))
        .unwrap_or(false)
}

/// POST `/api/workspaces/:slug/projects/:project_id/issue-labels/` — parity
/// with `LabelViewSet.create` (`label.py:42-55`,
/// `plane/app/urls/issue.py:71-75`).
///
/// - Gate: `@allow_permission([ROLE.ADMIN])` (level PROJECT default) —
///   project ADMIN (20) outright + active project member who is a workspace
///   ADMIN, else 403 `deny()`.
/// - 201 `LabelSerializer` (7 keys). `sort_order` mirrors `Label.save()`
///   (`db/models/label.py:46-...`): max + 10000, default 65535 (same
///   expression as the existing `create` in this file).
/// - Duplicate (project, name) hits the partial unique index
///   `unique_project_name_when_not_deleted` (verified in
///   `migrations/0001_initial.sql:7200`) → `IntegrityError` → 400
///   `{"error": DUP_LABEL_MSG}` (`label.py:51-55`), mirroring Django
///   literally (direct INSERT, no pre-check — race-free). ANY other
///   integrity violation (e.g. bogus project FK) maps to the same body,
///   exactly like Django's blanket `except IntegrityError`.
/// - Deviations: Django's `validate_name` (`serializers/issue.py:375-386`)
///   rejects case-variant dups (`name__iexact`) with
///   `{"name": ["LABEL_NAME_ALREADY_EXISTS"]}`; Rust enforces the DB
///   (case-sensitive) constraint only, so a case-variant dup 201s here
///   while Django 400s — same STATUS class, different body/edge. Exact dup
///   (the smoke path) 400s identically.
pub async fn issue_labels_create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateIssueLabel>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        guard_issue_labels_create(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let name = match validate_issue_label_name(&body.name) {
        Ok(n) => n,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(e))),
    };
    if let Some(c) = &body.color {
        if c.chars().count() > 255 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"color": ["Ensure this field has no more than 255 characters."]})),
            ));
        }
    }
    if let Some(parent) = body.parent {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM labels WHERE id = $1 AND deleted_at IS NULL)",
        )
        .bind(parent)
        .fetch_one(&st.pool)
        .await?;
        if !exists {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"parent": [format!("Invalid pk \"{parent}\" - object does not exist.")]}),
                ),
            ));
        }
    }
    let inserted: Result<LabelSerializerRow, sqlx::Error> = sqlx::query_as(
        "INSERT INTO labels (id, name, color, description, project_id, workspace_id, parent_id, sort_order, created_by_id, updated_by_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, COALESCE($2, ''), '', $3, w.id, $4, COALESCE((SELECT MAX(sort_order) + 10000 FROM labels WHERE project_id = $3), 65535), $5, $5, now(), now() \
         FROM workspaces w WHERE w.slug = $6 \
         RETURNING id, project_id, workspace_id, parent_id AS parent, name, color, sort_order",
    )
    .bind(&name)
    .bind(&body.color)
    .bind(project_id)
    .bind(body.parent)
    .bind(auth.0)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await;
    match inserted {
        Ok(row) => Ok((StatusCode::CREATED, Json(label_serializer_json(&row)))),
        Err(e) if is_integrity_err(&e) => Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": DUP_LABEL_MSG})),
        )),
        Err(e) => Err(e.into()),
    }
}

/// Batch F T4 defaults — quoted from
/// `plane/app/views/issue/label.py:90-117`
/// (`BulkCreateIssueLabelsEndpoint.post`): per-item `name` defaults to
/// `"Migrated"`, `description` to `"Migrated Issue"` (plain `dict.get`
/// defaults — no trim/validation, unlike the D11b single-create path).
pub(crate) const BULK_LABEL_DEFAULT_NAME: &str = "Migrated";
pub(crate) const BULK_LABEL_DEFAULT_DESC: &str = "Migrated Issue";

/// Batch F T4 color — mirrors Django
/// `f"#{random.randint(0, 0xFFFFFF + 1):06X}"` (`label.py:104`): uniform over
/// `0..=0xFFFFFF`, 6 uppercase hex digits. `rand` 0.9 is already a direct
/// dependency of the `api` crate (no new deps). Note Django regenerates the
/// color per item and IGNORES any per-item `color` key — mirrored here.
pub(crate) fn random_label_color() -> String {
    let n: u32 = rand::random_range(0..=0xFFFFFF);
    format!("#{n:06X}")
}

/// One `label_data` entry (`label.py:96-108`): only `name`/`description` are
/// read (each defaulting per the constants above); every other key —
/// including `color` — is ignored by Django and dropped here by serde,
/// mirroring DRF. `None` (missing/null) → default, exactly like `dict.get`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BulkLabelItem {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// POST body `{label_data: [...]}` (`label.py:93`): missing → `[]` (Django
/// `request.data.get("label_data", [])`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BulkCreateLabelsBody {
    #[serde(default)]
    pub label_data: Vec<BulkLabelItem>,
}

/// POST `/api/workspaces/:slug/projects/:project_id/bulk-create-labels/` —
/// parity with `BulkCreateIssueLabelsEndpoint.post` (`label.py:90-117`,
/// `plane/app/urls/issue.py:89-90`).
///
/// - Gate: `@allow_permission([ROLE.ADMIN])` at the default level PROJECT
///   (`permissions/base.py:19,53-78`) — project ADMIN (20) outright, else
///   active project member who is a workspace ADMIN, else 403 `deny()`.
///   Identical shape to the sibling `issue_labels_create` above (reuses
///   `guard_issue_labels_create` + shared `project_gate_allows`).
/// - Body `{label_data: [...]}` defaults to `[]`; empty → 201
///   `{"labels": []}` (Django `bulk_create([])` → `[]`).
/// - Bulk INSERT loops per-row `INSERT … ON CONFLICT DO NOTHING`,
///   mirroring `bulk_create(..., batch_size=50, ignore_conflicts=True)`
///   (conflicting rows are skipped, not errors).
/// - Rows stamped `project_id` (path), `workspace_id` (from the project
///   row — Django `project.workspace_id`), `created_by` + `updated_by` =
///   request user; `parent_id` NULL, `color` fresh-random per item.
/// - Re-SELECTs the created rows ordered like the issue-labels list
///   (`ORDER BY sort_order ASC`) and returns **201** `{"labels":
///   [LabelSerializer…]}` reusing `LabelSerializerRow` /
///   `label_serializer_json` (same 7 keys).
/// - Deviations: (1) `sort_order` uses this file's standard max+10000
///   expression per row (same as both single-create paths) so the
///   re-SELECT order is deterministic and preserves input order; Django's
///   `bulk_create` bypasses `Label.save()`, leaving every row at the model
///   default 65535. Same response ORDER, different stored values — flag
///   for shadow-diff review. (2) Unknown project → 404 `missing()`; Django
///   `Project.objects.get` raises → 500.
pub async fn bulk_create_labels(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<BulkCreateLabelsBody>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        guard_issue_labels_create(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let workspace_id: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT workspace_id FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_optional(&st.pool)
            .await?;
    let Some(workspace_id) = workspace_id else {
        return Ok(missing());
    };
    let mut ids: Vec<uuid::Uuid> = Vec::with_capacity(body.label_data.len());
    for item in &body.label_data {
        let name = item
            .name
            .clone()
            .unwrap_or_else(|| BULK_LABEL_DEFAULT_NAME.to_string());
        let description = item
            .description
            .clone()
            .unwrap_or_else(|| BULK_LABEL_DEFAULT_DESC.to_string());
        let color = random_label_color();
        let id: Option<uuid::Uuid> = sqlx::query_scalar(
            "INSERT INTO labels (id, name, color, description, project_id, workspace_id, parent_id, sort_order, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, NULL, COALESCE((SELECT MAX(sort_order) + 10000 FROM labels WHERE project_id = $4), 65535), $6, $6, now(), now()) \
             ON CONFLICT DO NOTHING RETURNING id",
        )
        .bind(&name)
        .bind(&color)
        .bind(&description)
        .bind(project_id)
        .bind(workspace_id)
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await?;
        if let Some(id) = id {
            ids.push(id);
        }
    }
    let rows: Vec<LabelSerializerRow> = sqlx::query_as(
        "SELECT l.id, l.project_id, l.workspace_id, l.parent_id AS parent, l.name, l.color, l.sort_order \
         FROM labels l \
         WHERE l.project_id = $1 AND l.id = ANY($2) AND l.deleted_at IS NULL \
         ORDER BY l.sort_order ASC",
    )
    .bind(project_id)
    .bind(&ids)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"labels": rows.iter().map(label_serializer_json).collect::<Vec<_>>() })),
    ))
}

#[cfg(test)]
mod batch_d_d11_tests {
    use super::*;

    #[test]
    fn dup_label_msg_verbatim() {
        // Quoted from `plane/app/views/issue/label.py:51-55`
        // (`LabelViewSet.create` IntegrityError branch): duplicate
        // (project, name) → 400 with this body.
        assert_eq!(
            DUP_LABEL_MSG,
            "Label with the same name already exists in the project"
        );
    }

    #[test]
    fn ws_labels_gate_allows_any_active_member() {
        // `WorkspaceViewerPermission`
        // (`plane/app/permissions/workspace.py:93-100`): any ACTIVE ws
        // member passes, incl. GUEST — no role filter. Non-member → 403.
        assert!(guard_ws_labels(Some(20)).is_ok());
        assert!(guard_ws_labels(Some(15)).is_ok());
        assert!(guard_ws_labels(Some(5)).is_ok());
        assert!(guard_ws_labels(None).is_err());
        assert_eq!(
            guard_ws_labels(None).unwrap_err(),
            crate::routes::project::FORBIDDEN_MSG
        );
    }

    #[test]
    fn issue_labels_read_gate_allows_any_active_ws_member() {
        // `ProjectBasePermission` safe-methods branch
        // (`plane/app/permissions/project.py:18-22`): any active ws member
        // passes (per-project scoping lives in the queryset, not the
        // permission). Non-member → 403.
        assert!(guard_issue_labels_read(Some(20)).is_ok());
        assert!(guard_issue_labels_read(Some(15)).is_ok());
        assert!(guard_issue_labels_read(Some(5)).is_ok());
        assert!(guard_issue_labels_read(None).is_err());
        assert_eq!(
            guard_issue_labels_read(None).unwrap_err(),
            crate::routes::project::FORBIDDEN_MSG
        );
    }

    #[test]
    fn issue_labels_create_gate_admin_only() {
        // `@allow_permission([ROLE.ADMIN])` (`label.py:42-44`, level PROJECT
        // default, `permissions/base.py:19`): project ADMIN (20) passes
        // outright; MEMBER (15) / GUEST (5) / non-member fall to the
        // ws-admin fallback applied by the caller (same shape as D3
        // `mark_default`).
        assert!(guard_issue_labels_create(Some(20)).is_ok());
        assert!(guard_issue_labels_create(Some(15)).is_err());
        assert!(guard_issue_labels_create(Some(5)).is_err());
        assert!(guard_issue_labels_create(None).is_err());
    }

    #[test]
    fn label_serializer_shape_matches_django() {
        // `plane/app/serializers/issue.py:361-373` fields (declaration
        // order): parent, name, color, id, project_id, workspace_id,
        // sort_order — the real delta vs the existing `{id, name}` row.
        let row = LabelSerializerRow {
            id: uuid::Uuid::nil(),
            project_id: Some(uuid::Uuid::nil()),
            workspace_id: uuid::Uuid::nil(),
            parent: None,
            name: "Bug".to_string(),
            color: "#ff0000".to_string(),
            sort_order: 65535.0,
        };
        let v = label_serializer_json(&row);
        // Key SET matches Django exactly; key ORDER follows repo batch
        // convention (serde_json sorts keys — same note as D3
        // `state_serializer_json`).
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "color",
                "id",
                "name",
                "parent",
                "project_id",
                "sort_order",
                "workspace_id"
            ]
        );
        assert_eq!(v["parent"], serde_json::json!(null));
        assert_eq!(v["name"], serde_json::json!("Bug"));
        assert_eq!(v["sort_order"], serde_json::json!(65535.0));
    }

    #[test]
    fn issue_label_name_validation_mirrors_drf() {
        // DRF `CharField` (required, no blank): missing → required error,
        // blank (post-trim, `trim_whitespace`) → blank error, >255 chars →
        // length error — same bodies as the D8 `validate_reaction` precedent.
        assert!(validate_issue_label_name(&None).is_err());
        assert!(validate_issue_label_name(&Some("   ".to_string())).is_err());
        assert!(validate_issue_label_name(&Some("x".repeat(256))).is_err());
        assert_eq!(
            validate_issue_label_name(&None).unwrap_err(),
            serde_json::json!({"name": ["This field is required."]})
        );
        assert_eq!(
            validate_issue_label_name(&Some(" Bug ".to_string())).unwrap(),
            "Bug".to_string()
        );
    }

    #[test]
    fn bulk_label_defaults_match_django() {
        assert_eq!(BULK_LABEL_DEFAULT_NAME, "Migrated");
        assert_eq!(BULK_LABEL_DEFAULT_DESC, "Migrated Issue");
        let c = random_label_color();
        assert_eq!(c.len(), 7); // "#RRGGBB"
        assert!(c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit()));
    }

    #[test]
    fn bulk_create_gate_matches_project_level_admin() {
        // `@allow_permission([ROLE.ADMIN])` at the default level PROJECT
        // (`permissions/base.py:19,53-78`), same decorator as the sibling
        // `LabelViewSet.create`: project ADMIN (20) passes outright; else an
        // active project member who is a workspace ADMIN passes; anything
        // else → 403 `deny()`. Reuses `guard_issue_labels_create` + shared
        // `project_gate_allows`, so the pure decision is pinned here.
        let allows = |member_role: Option<i16>, ws_admin: bool| {
            project_gate_allows(
                guard_issue_labels_create(member_role).is_ok(),
                member_role.is_some(),
                ws_admin,
            )
        };
        // Discriminating: project ADMIN who is NOT a ws admin ⇒ allow.
        assert!(allows(Some(20), false));
        // Discriminating: ws ADMIN with NO project membership ⇒ deny
        // (the fallback branch requires an active project membership).
        assert!(!allows(None, true));
        // Existing ws cases: ws ADMIN + project member ⇒ allow …
        assert!(allows(Some(20), true));
        assert!(allows(Some(15), true));
        // … plain member / guest / outsider without ws-admin ⇒ deny.
        assert!(!allows(Some(15), false));
        assert!(!allows(Some(5), false));
        assert!(!allows(None, false));
    }
}
